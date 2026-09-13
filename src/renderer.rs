use std::{collections::HashMap, sync::Arc};

use bytemuck::{Pod, Zeroable};
use thiserror::Error;
use web_time::Instant;

#[cfg(feature = "egui")]
use crate::egui_integration::EguiIntegration;
#[cfg(feature = "egui")]
use crate::winit::event::WindowEvent;
use crate::{
    DEPTH_FORMAT, Instance, InstanceId, MATERIAL_SLOT_COUNT, Material, Mesh, MeshMaterialSlots,
    Part, PartShape, Texture, TextureColorSpace, TextureError, TextureHandle, Transform, Vertex,
    Workspace, wgpu::util::DeviceExt, winit::window::Window,
};

/// Errors returned while creating or using a renderer.
#[derive(Debug, Error)]
pub enum RendererError {
    /// The window surface could not be created.
    #[error("could not create the rendering surface: {0}")]
    SurfaceCreation(#[from] wgpu::CreateSurfaceError),
    /// No compatible GPU adapter was available.
    #[error("could not find a compatible GPU adapter: {0}")]
    AdapterRequest(#[from] wgpu::RequestAdapterError),
    /// The selected adapter could not create a device and queue.
    #[error("could not create the GPU device: {0}")]
    DeviceRequest(#[from] wgpu::RequestDeviceError),
    /// The window surface exposed no texture format.
    #[error("the window surface did not expose any texture formats")]
    NoSurfaceFormat,
    /// A custom mesh had no vertices or indices.
    #[error("mesh must contain at least one vertex and one index")]
    EmptyMesh,
    /// A custom mesh index was not present in its vertex list.
    #[error("mesh index {index} is outside the vertex range")]
    InvalidMeshIndex {
        /// The invalid index value.
        index: u16,
    },
    /// A texture failed validation.
    #[error("invalid texture: {0}")]
    InvalidTexture(#[from] TextureError),
    /// A material referenced a texture that is not owned by its workspace.
    #[error("workspace texture handle {index} is not valid")]
    InvalidTextureHandle {
        /// The invalid workspace-local texture index.
        index: usize,
    },
    /// The surface reported a validation error while acquiring a frame.
    #[error("the surface reported a validation error while acquiring a frame")]
    SurfaceValidation,
    /// Waiting for GPU work failed.
    #[error("could not wait for submitted GPU work: {0}")]
    DevicePoll(#[from] wgpu::PollError),
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct InstanceRaw {
    model: [[f32; 4]; 4],
    normal_0: [f32; 4],
    normal_1: [f32; 4],
    normal_2: [f32; 4],
    base_color: [f32; 4],
    metallic_roughness: [f32; 4],
    emissive: [f32; 4],
}

impl InstanceRaw {
    fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        const ATTRIBUTES: &[wgpu::VertexAttribute] = &wgpu::vertex_attr_array![
            6 => Float32x4,
            7 => Float32x4,
            8 => Float32x4,
            9 => Float32x4,
            10 => Float32x4,
            11 => Float32x4,
            12 => Float32x4,
            13 => Float32x4,
            14 => Float32x4,
            15 => Float32x4,
        ];

        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: ATTRIBUTES,
        }
    }
}

/// A handle to mesh data stored on the GPU.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MeshHandle(usize);
struct RenderBatch {
    shape: PartShape,
    textures: MaterialTextures,
    instances: Vec<InstanceRaw>,
    instance_start: usize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct GpuTextureHandle(usize);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct MaterialTextures {
    base_color: [GpuTextureHandle; MATERIAL_SLOT_COUNT],
    normal: [GpuTextureHandle; MATERIAL_SLOT_COUNT],
    metallic_roughness: [GpuTextureHandle; MATERIAL_SLOT_COUNT],
}

struct GpuTexture {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

struct GpuMesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

/// The wgpu state and built-in PBR mesh pipeline.
pub struct Renderer {
    #[cfg(feature = "egui")]
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    material_bind_group_layout: wgpu::BindGroupLayout,
    material_sampler: wgpu::Sampler,
    textures: Vec<GpuTexture>,
    workspace_texture_handles: HashMap<(InstanceId, TextureHandle), GpuTextureHandle>,
    texture_dedup: HashMap<Texture, GpuTextureHandle>,
    default_material_textures: MaterialTextures,
    instance_buffer: wgpu::Buffer,
    instance_data: Vec<InstanceRaw>,
    meshes: Vec<GpuMesh>,
    primitive_meshes: [MeshHandle; PartShape::COUNT],
    clear_color: wgpu::Color,
    last_frame: Instant,
    fps_timer: Instant,
    frame_count: u32,
    fps: f32,
    #[cfg(feature = "egui")]
    egui: EguiIntegration,
}

#[repr(C)]
/// The camera and fixed lighting values consumed by the built-in shader.
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct CameraUniform {
    /// Camera view-projection matrix in column-major form.
    pub view_projection: [[f32; 4]; 4],
    /// Camera world position as an XYZ vector with an unused fourth component.
    pub camera_position: [f32; 4],
    /// World-space direction toward the fixed key light.
    pub light_direction: [f32; 4],
    /// RGB intensity of the fixed key light.
    pub light_color: [f32; 4],
    /// RGB intensity of the fixed ambient light.
    pub ambient_color: [f32; 4],
}

impl Renderer {
    /// Creates a renderer and keeps the supplied window alive through its surface.
    pub async fn new(window: Arc<Window>) -> Result<Self, RendererError> {
        Self::new_with_present_mode(window, wgpu::PresentMode::Fifo).await
    }

    /// Creates a renderer with the requested surface presentation mode.
    ///
    /// FIFO presentation is used when the surface does not support the requested mode.
    /// [`Renderer::new`] retains the normal FIFO presentation behavior.
    pub async fn new_with_present_mode(
        window: Arc<Window>,
        requested_present_mode: wgpu::PresentMode,
    ) -> Result<Self, RendererError> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: Default::default(),
            flags: wgpu::InstanceFlags::ALLOW_UNDERLYING_NONCOMPLIANT_ADAPTER,
            backend_options: Default::default(),
            display: Default::default(),
            memory_budget_thresholds: Default::default(),
        });
        let surface = instance.create_surface(window.clone())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await?;
        log::info!("selected wgpu adapter: {:?}", adapter.get_info());
        let required_limits = wgpu::Limits {
            max_sampled_textures_per_shader_stage: (MATERIAL_SLOT_COUNT * 3) as u32,
            ..Default::default()
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("terrarium device"),
                required_features: wgpu::Features::empty(),
                required_limits,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await?;

        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| capabilities.formats.first().copied())
            .ok_or(RendererError::NoSurfaceFormat)?;
        let present_mode = if capabilities.present_modes.contains(&requested_present_mode) {
            requested_present_mode
        } else {
            wgpu::PresentMode::Fifo
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode: capabilities.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let (depth_texture, depth_view) = create_depth_texture(&device, &config);
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera uniform buffer"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("part instance buffer"),
            size: std::mem::size_of::<InstanceRaw>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("camera bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera bind group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let mut material_bind_group_entries = Vec::with_capacity(MATERIAL_SLOT_COUNT * 3 + 1);
        for binding in 0..(MATERIAL_SLOT_COUNT * 3) {
            material_bind_group_entries.push(wgpu::BindGroupLayoutEntry {
                binding: binding as u32,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
        }
        material_bind_group_entries.push(wgpu::BindGroupLayoutEntry {
            binding: (MATERIAL_SLOT_COUNT * 3) as u32,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        });
        let material_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("material bind group layout"),
                entries: &material_bind_group_entries,
            });
        let material_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("material sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            lod_min_clamp: 0.0,
            lod_max_clamp: 32.0,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        });
        let default_base_color = Texture::new(1, 1, vec![255, 255, 255, 255])?;
        let default_normal = Texture::linear(1, 1, vec![128, 128, 255, 255])?;
        let default_metallic_roughness = Texture::linear(1, 1, vec![0, 255, 0, 255])?;
        let textures = vec![
            upload_texture(&device, &queue, &default_base_color)?,
            upload_texture(&device, &queue, &default_normal)?,
            upload_texture(&device, &queue, &default_metallic_roughness)?,
        ];
        let mut texture_dedup = HashMap::new();
        texture_dedup.insert(default_base_color, GpuTextureHandle(0));
        texture_dedup.insert(default_normal, GpuTextureHandle(1));
        texture_dedup.insert(default_metallic_roughness, GpuTextureHandle(2));
        let default_material_textures = MaterialTextures {
            base_color: [GpuTextureHandle(0); MATERIAL_SLOT_COUNT],
            normal: [GpuTextureHandle(1); MATERIAL_SLOT_COUNT],
            metallic_roughness: [GpuTextureHandle(2); MATERIAL_SLOT_COUNT],
        };
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("PBR mesh shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh pipeline layout"),
            bind_group_layouts: &[
                Some(&camera_bind_group_layout),
                Some(&material_bind_group_layout),
            ],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("PBR mesh pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(Vertex::layout()), Some(InstanceRaw::layout())],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let mut renderer = Self {
            #[cfg(feature = "egui")]
            window: window.clone(),
            surface,
            device,
            queue,
            config,
            depth_texture,
            depth_view,
            pipeline,
            camera_buffer,
            camera_bind_group,
            material_bind_group_layout,
            material_sampler,
            textures,
            workspace_texture_handles: HashMap::new(),
            texture_dedup,
            default_material_textures,
            instance_buffer,
            instance_data: Vec::new(),
            meshes: Vec::new(),
            primitive_meshes: [MeshHandle(usize::MAX); PartShape::COUNT],
            clear_color: wgpu::Color {
                r: 0.018,
                g: 0.028,
                b: 0.065,
                a: 1.0,
            },
            last_frame: Instant::now(),
            fps_timer: Instant::now(),
            frame_count: 0,
            fps: 0.0,
            #[cfg(feature = "egui")]
            egui: EguiIntegration::new(window.as_ref()),
        };
        for shape in PartShape::ALL {
            let mesh = renderer.add_mesh(&shape.mesh([1.0; 4]))?;
            renderer.primitive_meshes[shape.index()] = mesh;
        }
        Ok(renderer)
    }

    /// Sets the color used to clear the color attachment before each frame.
    pub fn set_clear_color(&mut self, color: wgpu::Color) {
        self.clear_color = color;
    }

    /// Returns the average number of successfully presented frames per second over the last
    /// measurement interval.
    pub fn fps(&self) -> f32 {
        self.fps
    }

    /// Returns the elapsed time in seconds since the previous call, capped at 100 milliseconds.
    pub fn delta_secs(&mut self) -> f32 {
        let now = Instant::now();
        let delta_seconds = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        delta_seconds.min(0.1)
    }

    /// Blocks until all GPU work submitted before this call has completed.
    ///
    /// This is intended for deterministic measurements and should not be used in a real-time
    /// render loop, where allowing multiple frames in flight is preferable.
    pub fn wait_for_gpu(&self) -> Result<(), RendererError> {
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .map(|_| ())
            .map_err(RendererError::DevicePoll)
    }

    /// Reconfigures the surface and depth buffer for a new non-zero size.
    ///
    /// Zero dimensions are ignored, which is useful while a window is
    /// minimized.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        (self.depth_texture, self.depth_view) = create_depth_texture(&self.device, &self.config);
    }

    /// Returns the egui context used by this renderer.
    #[cfg(feature = "egui")]
    pub fn egui_context(&self) -> &egui::Context {
        self.egui.context()
    }

    /// Returns the egui context used by this renderer for configuration.
    #[cfg(feature = "egui")]
    pub fn egui_context_mut(&mut self) -> &mut egui::Context {
        self.egui.context_mut()
    }

    /// Forwards a window event to egui and returns whether egui consumed it.
    #[cfg(feature = "egui")]
    pub fn on_window_event(&mut self, event: &WindowEvent) -> bool {
        self.egui.on_window_event(&self.window, event)
    }

    /// Renders a workspace with an egui overlay.
    ///
    /// The UI closure runs once per frame. This renderer handles egui input,
    /// platform output, tessellation, texture uploads, GPU buffer updates, and
    /// the overlay render pass.
    #[cfg(feature = "egui")]
    pub fn render_egui<F>(&mut self, workspace: &Workspace, run_ui: F) -> Result<(), RendererError>
    where
        F: FnMut(&mut egui::Ui),
    {
        let frame = self.egui.begin_frame(&self.window, run_ui);
        self.render_with_overlay_internal(workspace, |renderer, encoder, view| {
            renderer.egui.render_frame(
                frame,
                &renderer.device,
                &renderer.queue,
                encoder,
                view,
                renderer.config.format,
                [renderer.config.width, renderer.config.height],
            )
        })
    }

    /// Uploads a custom mesh and returns its GPU handle.
    pub fn add_mesh(&mut self, mesh: &Mesh) -> Result<MeshHandle, RendererError> {
        if mesh.vertices.is_empty() || mesh.indices.is_empty() {
            return Err(RendererError::EmptyMesh);
        }
        for &index in &mesh.indices {
            if index as usize >= mesh.vertices.len() {
                return Err(RendererError::InvalidMeshIndex { index });
            }
        }

        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mesh vertex buffer"),
                contents: bytemuck::cast_slice(&mesh.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let index_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("mesh index buffer"),
                contents: bytemuck::cast_slice(&mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        let handle = MeshHandle(self.meshes.len());
        self.meshes.push(GpuMesh {
            vertex_buffer,
            index_buffer,
            index_count: mesh.indices.len() as u32,
        });
        Ok(handle)
    }

    fn material_textures(
        &mut self,
        workspace: &Workspace,
        material: &Material,
        material_slots: &MeshMaterialSlots,
    ) -> Result<MaterialTextures, RendererError> {
        let resolve = |renderer: &mut Self,
                       handle: Option<TextureHandle>,
                       fallback: GpuTextureHandle|
         -> Result<GpuTextureHandle, RendererError> {
            handle.map_or(Ok(fallback), |handle| {
                renderer.upload_workspace_texture(workspace, handle)
            })
        };

        let base_color = resolve(
            self,
            material.textures.base_color,
            self.default_material_textures.base_color[0],
        )?;
        let normal = resolve(
            self,
            material.textures.normal,
            self.default_material_textures.normal[0],
        )?;
        let metallic_roughness = resolve(
            self,
            material.textures.metallic_roughness,
            self.default_material_textures.metallic_roughness[0],
        )?;
        let mut textures = MaterialTextures {
            base_color: [base_color; MATERIAL_SLOT_COUNT],
            normal: [normal; MATERIAL_SLOT_COUNT],
            metallic_roughness: [metallic_roughness; MATERIAL_SLOT_COUNT],
        };
        for (slot, material) in material_slots
            .slots
            .iter()
            .take(MATERIAL_SLOT_COUNT - 1)
            .enumerate()
        {
            let slot = slot + 1;
            textures.base_color[slot] = resolve(self, material.textures.base_color, base_color)?;
            textures.normal[slot] = resolve(self, material.textures.normal, normal)?;
            textures.metallic_roughness[slot] = resolve(
                self,
                material.textures.metallic_roughness,
                metallic_roughness,
            )?;
        }
        Ok(textures)
    }

    fn upload_workspace_texture(
        &mut self,
        workspace: &Workspace,
        handle: TextureHandle,
    ) -> Result<GpuTextureHandle, RendererError> {
        let workspace_handle = (workspace.id(), handle);
        if let Some(&gpu_handle) = self.workspace_texture_handles.get(&workspace_handle) {
            return Ok(gpu_handle);
        }

        let texture = workspace
            .get_texture(handle)
            .ok_or(RendererError::InvalidTextureHandle { index: handle.0 })?;
        let gpu_handle = if let Some(&gpu_handle) = self.texture_dedup.get(texture) {
            gpu_handle
        } else {
            let gpu_handle = GpuTextureHandle(self.textures.len());
            self.textures
                .push(upload_texture(&self.device, &self.queue, texture)?);
            self.texture_dedup.insert(texture.clone(), gpu_handle);
            gpu_handle
        };
        self.workspace_texture_handles
            .insert(workspace_handle, gpu_handle);
        Ok(gpu_handle)
    }

    fn create_material_bind_group(&self, textures: MaterialTextures) -> wgpu::BindGroup {
        let mut entries = Vec::with_capacity(MATERIAL_SLOT_COUNT * 3 + 1);
        for (slot, texture) in textures.base_color.into_iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: slot as u32,
                resource: wgpu::BindingResource::TextureView(&self.textures[texture.0].view),
            });
        }
        for (slot, texture) in textures.normal.into_iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: (MATERIAL_SLOT_COUNT + slot) as u32,
                resource: wgpu::BindingResource::TextureView(&self.textures[texture.0].view),
            });
        }
        for (slot, texture) in textures.metallic_roughness.into_iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: (MATERIAL_SLOT_COUNT * 2 + slot) as u32,
                resource: wgpu::BindingResource::TextureView(&self.textures[texture.0].view),
            });
        }
        entries.push(wgpu::BindGroupEntry {
            binding: (MATERIAL_SLOT_COUNT * 3) as u32,
            resource: wgpu::BindingResource::Sampler(&self.material_sampler),
        });
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("material bind group"),
            layout: &self.material_bind_group_layout,
            entries: &entries,
        })
    }

    fn ensure_instance_capacity(&mut self, instance_count: usize) {
        let required_size =
            std::mem::size_of::<InstanceRaw>() as u64 * instance_count.max(1) as u64;
        if self.instance_buffer.size() >= required_size {
            return;
        }

        self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("part instance buffer"),
            size: required_size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
    }

    /// Renders a workspace using its current camera. A lost or outdated surface is reconfigured
    /// and retried on the next frame; minimized and occluded windows simply skip their frame.
    pub fn render(&mut self, workspace: &Workspace) -> Result<(), RendererError> {
        self.render_with_overlay(
            workspace,
            |_device, _queue, _encoder, _view, _format, _size| Vec::new(),
        )
    }

    /// Renders a workspace and gives an overlay access to the frame before it is presented.
    ///
    /// The callback can encode additional commands into the frame, such as an egui render pass,
    /// and return command buffers that must be submitted alongside the scene command buffer.
    pub fn render_with_overlay<F>(
        &mut self,
        workspace: &Workspace,
        draw_overlay: F,
    ) -> Result<(), RendererError>
    where
        F: FnOnce(
            &wgpu::Device,
            &wgpu::Queue,
            &mut wgpu::CommandEncoder,
            &wgpu::TextureView,
            wgpu::TextureFormat,
            [u32; 2],
        ) -> Vec<wgpu::CommandBuffer>,
    {
        self.render_with_overlay_internal(workspace, |renderer, encoder, view| {
            draw_overlay(
                &renderer.device,
                &renderer.queue,
                encoder,
                view,
                renderer.config.format,
                [renderer.config.width, renderer.config.height],
            )
        })
    }

    pub(crate) fn render_with_overlay_internal<F>(
        &mut self,
        workspace: &Workspace,
        draw_overlay: F,
    ) -> Result<(), RendererError>
    where
        F: FnOnce(
            &mut Self,
            &mut wgpu::CommandEncoder,
            &wgpu::TextureView,
        ) -> Vec<wgpu::CommandBuffer>,
    {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err(RendererError::SurfaceValidation);
            }
        };

        let camera_uniform = CameraUniform {
            view_projection: workspace
                .current_camera
                .view_projection_matrix()
                .to_cols_array_2d(),
            camera_position: workspace
                .current_camera
                .pivot()
                .w_axis
                .truncate()
                .extend(1.0)
                .to_array(),
            light_direction: [-0.45, 0.85, 0.35, 0.0],
            light_color: [3.0, 2.8, 2.5, 0.0],
            ambient_color: [0.035, 0.045, 0.06, 0.0],
        };
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&camera_uniform));
        let parts = workspace.get_all::<Part>();
        let mut batches = Vec::<RenderBatch>::new();
        let mut batch_indices = HashMap::new();
        for part in parts {
            let textures =
                self.material_textures(workspace, &part.material, &part.material_slots)?;
            let key = (part.shape, textures);
            let batch_index = if let Some(&batch_index) = batch_indices.get(&key) {
                batch_index
            } else {
                let batch_index = batches.len();
                batch_indices.insert(key, batch_index);
                batches.push(RenderBatch {
                    shape: part.shape,
                    textures,
                    instances: Vec::new(),
                    instance_start: 0,
                });
                batch_index
            };
            let model = part.transform();
            let normal_matrix = model.inverse().transpose().to_cols_array_2d();
            let material = part.material;
            let tint = part.color.rgba();
            batches[batch_index].instances.push(InstanceRaw {
                model: model.to_cols_array_2d(),
                normal_0: [
                    normal_matrix[0][0],
                    normal_matrix[0][1],
                    normal_matrix[0][2],
                    0.0,
                ],
                normal_1: [
                    normal_matrix[1][0],
                    normal_matrix[1][1],
                    normal_matrix[1][2],
                    0.0,
                ],
                normal_2: [
                    normal_matrix[2][0],
                    normal_matrix[2][1],
                    normal_matrix[2][2],
                    0.0,
                ],
                base_color: [
                    material.base_color[0] * tint[0],
                    material.base_color[1] * tint[1],
                    material.base_color[2] * tint[2],
                    material.base_color[3] * tint[3],
                ],
                metallic_roughness: [
                    material.metallic.clamp(0.0, 1.0),
                    material.roughness.clamp(0.04, 1.0),
                    0.0,
                    0.0,
                ],
                emissive: [
                    material.emissive[0],
                    material.emissive[1],
                    material.emissive[2],
                    0.0,
                ],
            });
        }

        self.instance_data.clear();
        for batch in &mut batches {
            batch.instance_start = self.instance_data.len();
            self.instance_data.extend_from_slice(&batch.instances);
        }
        self.ensure_instance_capacity(self.instance_data.len());
        if !self.instance_data.is_empty() {
            self.queue.write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&self.instance_data),
            );
        }

        let material_bind_groups = batches
            .iter()
            .map(|batch| self.create_material_bind_group(batch.textures))
            .collect::<Vec<_>>();

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let color_attachment = wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(self.clear_color),
                store: wgpu::StoreOp::Store,
            },
        };
        let depth_attachment = wgpu::RenderPassDepthStencilAttachment {
            view: &self.depth_view,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("scene command encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene render pass"),
                color_attachments: &[Some(color_attachment)],
                depth_stencil_attachment: Some(depth_attachment),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            for (batch, material_bind_group) in batches.iter().zip(&material_bind_groups) {
                if batch.instances.is_empty() {
                    continue;
                }
                let mesh_handle = self.primitive_meshes[batch.shape.index()];
                let Some(mesh) = self.meshes.get(mesh_handle.0) else {
                    continue;
                };
                pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                let instance_start =
                    batch.instance_start as u64 * std::mem::size_of::<InstanceRaw>() as u64;
                let instance_end = instance_start
                    + batch.instances.len() as u64 * std::mem::size_of::<InstanceRaw>() as u64;
                pass.set_vertex_buffer(1, self.instance_buffer.slice(instance_start..instance_end));
                pass.set_bind_group(1, material_bind_group, &[]);
                pass.draw_indexed(0..mesh.index_count, 0, 0..batch.instances.len() as u32);
            }
        }
        let overlay_command_buffers = draw_overlay(self, &mut encoder, &view);
        self.queue.submit(
            overlay_command_buffers
                .into_iter()
                .chain(std::iter::once(encoder.finish())),
        );
        self.queue.present(frame);
        self.frame_count += 1;
        let elapsed = self.fps_timer.elapsed().as_secs_f32();
        if elapsed >= 1.0 {
            self.fps = self.frame_count as f32 / elapsed;
            self.frame_count = 0;
            self.fps_timer = Instant::now();
        }
        Ok(())
    }
}

fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &Texture,
) -> Result<GpuTexture, TextureError> {
    let format = match texture.color_space {
        TextureColorSpace::Srgb => wgpu::TextureFormat::Rgba8UnormSrgb,
        TextureColorSpace::Linear => wgpu::TextureFormat::Rgba8Unorm,
    };
    let mip_levels = texture.mip_levels()?;
    let gpu_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("material texture"),
        size: wgpu::Extent3d {
            width: texture.width,
            height: texture.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: mip_levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (mip_level, image) in mip_levels.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &gpu_texture,
                mip_level: mip_level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &image.pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(image.width * 4),
                rows_per_image: Some(image.height),
            },
            wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
        );
    }
    let view = gpu_texture.create_view(&wgpu::TextureViewDescriptor::default());
    Ok(GpuTexture {
        _texture: gpu_texture,
        view,
    })
}

fn create_depth_texture(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth texture"),
        size: wgpu::Extent3d {
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

#[cfg(test)]
mod tests {
    #[test]
    fn material_shader_validates_with_explicit_texture_gradients() {
        let module = wgpu::naga::front::wgsl::parse_str(include_str!("shader.wgsl"))
            .expect("material shader should parse");
        let mut validator = wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::empty(),
        );
        validator
            .validate(&module)
            .expect("material shader should validate");
    }
}
