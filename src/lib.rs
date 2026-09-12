//! A small, opinionated 3D renderer built on wgpu.
//!
//! Build meshes once with [`Renderer::add_mesh`], put them into a [`Scene`], and pass that scene
//! to [`Renderer::render`] each frame.

use std::{num::NonZeroU64, sync::Arc};

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use thiserror::Error;
use wgpu::util::DeviceExt;
use winit::{
    event::{DeviceEvent, ElementState, WindowEvent},
    keyboard::{KeyCode, PhysicalKey},
    window::Window,
};

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Errors returned while creating or using a renderer.
#[derive(Debug, Error)]
pub enum RendererError {
    #[error("could not create the rendering surface: {0}")]
    SurfaceCreation(#[from] wgpu::CreateSurfaceError),
    #[error("could not find a compatible GPU adapter: {0}")]
    AdapterRequest(#[from] wgpu::RequestAdapterError),
    #[error("could not create the GPU device: {0}")]
    DeviceRequest(#[from] wgpu::RequestDeviceError),
    #[error("the window surface did not expose any texture formats")]
    NoSurfaceFormat,
    #[error("mesh must contain at least one vertex and one index")]
    EmptyMesh,
    #[error("mesh index {index} is outside the vertex range")]
    InvalidMeshIndex { index: u16 },
    #[error("the surface reported a validation error while acquiring a frame")]
    SurfaceValidation,
}

/// A position and vertex color consumed by the built-in pipeline.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

impl Vertex {
    pub fn new(position: [f32; 3], color: [f32; 4]) -> Self {
        Self { position, color }
    }

    fn layout<'a>() -> wgpu::VertexBufferLayout<'a> {
        const ATTRIBUTES: &[wgpu::VertexAttribute] =
            &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4];

        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: ATTRIBUTES,
        }
    }
}

/// CPU-side mesh data ready to be uploaded to a [`Renderer`].
#[derive(Clone, Debug)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
}

impl Mesh {
    pub fn new(vertices: Vec<Vertex>, indices: Vec<u16>) -> Self {
        Self { vertices, indices }
    }

    /// Creates a cube centered at the origin.
    pub fn cube(size: f32, color: [f32; 4]) -> Self {
        let h = size * 0.5;
        let faces = [
            ([-h, -h, h], [h, -h, h], [h, h, h], [-h, h, h], 1.0),
            ([h, -h, -h], [-h, -h, -h], [-h, h, -h], [h, h, -h], 0.72),
            ([-h, h, h], [h, h, h], [h, h, -h], [-h, h, -h], 1.15),
            ([-h, -h, -h], [h, -h, -h], [h, -h, h], [-h, -h, h], 0.52),
            ([h, -h, h], [h, -h, -h], [h, h, -h], [h, h, h], 0.86),
            ([-h, -h, -h], [-h, -h, h], [-h, h, h], [-h, h, -h], 0.66),
        ];

        let mut vertices = Vec::with_capacity(24);
        let mut indices = Vec::with_capacity(36);
        for (a, b, c, d, shade) in faces {
            let face_color = [
                (color[0] * shade).min(1.0),
                (color[1] * shade).min(1.0),
                (color[2] * shade).min(1.0),
                color[3],
            ];
            let start = vertices.len() as u16;
            vertices.extend([
                Vertex::new(a, face_color),
                Vertex::new(b, face_color),
                Vertex::new(c, face_color),
                Vertex::new(d, face_color),
            ]);
            indices.extend([start, start + 1, start + 2, start + 2, start + 3, start]);
        }

        Self { vertices, indices }
    }

    /// Creates a square on the XZ plane, centered at the origin.
    pub fn plane(size: f32, color: [f32; 4]) -> Self {
        let h = size * 0.5;
        Self {
            vertices: vec![
                Vertex::new([-h, 0.0, -h], color),
                Vertex::new([h, 0.0, -h], color),
                Vertex::new([h, 0.0, h], color),
                Vertex::new([-h, 0.0, h], color),
            ],
            indices: vec![0, 1, 2, 2, 3, 0],
        }
    }
}

/// A handle to mesh data stored on the GPU.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MeshHandle(usize);

/// One mesh placement in a scene.
#[derive(Clone, Copy, Debug)]
pub struct RenderObject {
    pub mesh: MeshHandle,
    pub transform: Mat4,
}

/// A collection of mesh placements rendered in insertion order.
#[derive(Clone, Debug, Default)]
pub struct Scene {
    objects: Vec<RenderObject>,
}

impl Scene {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, mesh: MeshHandle, transform: Mat4) {
        self.objects.push(RenderObject { mesh, transform });
    }

    pub fn objects(&self) -> &[RenderObject] {
        &self.objects
    }
}

/// A perspective camera with yaw/pitch orientation.
#[derive(Clone, Debug)]
pub struct Camera {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub aspect: f32,
    pub fovy: f32,
    pub znear: f32,
    pub zfar: f32,
}

impl Camera {
    pub fn new(position: Vec3, target: Vec3, aspect: f32) -> Self {
        let direction = (target - position).normalize_or_zero();
        Self {
            position,
            yaw: direction.x.atan2(-direction.z),
            pitch: direction.y.asin(),
            aspect: aspect.max(0.001),
            fovy: 60.0_f32.to_radians(),
            znear: 0.1,
            zfar: 200.0,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if height != 0 {
            self.aspect = width as f32 / height as f32;
        }
    }

    pub fn forward(&self) -> Vec3 {
        Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        )
        .normalize_or_zero()
    }

    pub fn view_projection_matrix(&self) -> Mat4 {
        let view = glam::camera::rh::view::look_to_mat4(self.position, self.forward(), Vec3::Y);
        let projection = glam::camera::rh::proj::directx::perspective(
            self.fovy,
            self.aspect,
            self.znear,
            self.zfar,
        );
        projection * view
    }
}

/// First-person keyboard and raw mouse input for a [`Camera`].
#[derive(Clone, Debug)]
pub struct CameraController {
    pub speed: f32,
    pub sensitivity: f32,
    forward: bool,
    backward: bool,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    sprint: bool,
    mouse_delta: (f32, f32),
}

impl CameraController {
    pub fn new(speed: f32, sensitivity: f32) -> Self {
        Self {
            speed,
            sensitivity,
            forward: false,
            backward: false,
            left: false,
            right: false,
            up: false,
            down: false,
            sprint: false,
            mouse_delta: (0.0, 0.0),
        }
    }

    /// Feeds a winit window event into the controller. Returns true when it was used.
    pub fn process_window_event(&mut self, event: &WindowEvent) -> bool {
        let WindowEvent::KeyboardInput { event, .. } = event else {
            if matches!(event, WindowEvent::Focused(false)) {
                self.clear_keys();
            }
            return false;
        };

        let PhysicalKey::Code(key) = event.physical_key else {
            return false;
        };
        let pressed = event.state == ElementState::Pressed;
        match key {
            KeyCode::KeyW => set_key(&mut self.forward, pressed),
            KeyCode::KeyS => set_key(&mut self.backward, pressed),
            KeyCode::KeyA => set_key(&mut self.left, pressed),
            KeyCode::KeyD => set_key(&mut self.right, pressed),
            KeyCode::Space => set_key(&mut self.up, pressed),
            KeyCode::ControlLeft | KeyCode::ControlRight => set_key(&mut self.down, pressed),
            KeyCode::ShiftLeft | KeyCode::ShiftRight => set_key(&mut self.sprint, pressed),
            _ => false,
        }
    }

    /// Feeds raw device events into the controller for mouse-look.
    pub fn process_device_event(&mut self, event: &DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            self.mouse_delta.0 += delta.0 as f32;
            self.mouse_delta.1 += delta.1 as f32;
        }
    }

    pub fn update_camera(&mut self, camera: &mut Camera, delta_seconds: f32) {
        let delta_seconds = delta_seconds.min(0.1);
        let forward = camera.forward();
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        let horizontal_forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
        let mut movement = Vec3::ZERO;
        if self.forward {
            movement += horizontal_forward;
        }
        if self.backward {
            movement -= horizontal_forward;
        }
        if self.right {
            movement += right;
        }
        if self.left {
            movement -= right;
        }
        if self.up {
            movement += Vec3::Y;
        }
        if self.down {
            movement -= Vec3::Y;
        }

        if movement.length_squared() > 0.0 {
            let speed = self.speed * if self.sprint { 2.5 } else { 1.0 };
            camera.position += movement.normalize() * speed * delta_seconds;
        }

        camera.yaw -= self.mouse_delta.0 * self.sensitivity;
        camera.pitch = (camera.pitch - self.mouse_delta.1 * self.sensitivity)
            .clamp(-89.0_f32.to_radians(), 89.0_f32.to_radians());
        self.mouse_delta = (0.0, 0.0);
    }

    fn clear_keys(&mut self) {
        self.forward = false;
        self.backward = false;
        self.left = false;
        self.right = false;
        self.up = false;
        self.down = false;
        self.sprint = false;
        self.mouse_delta = (0.0, 0.0);
    }
}

fn set_key(key: &mut bool, pressed: bool) -> bool {
    *key = pressed;
    true
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    view_projection: [[f32; 4]; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ModelUniform {
    model: [[f32; 4]; 4],
}

struct GpuMesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

/// The wgpu state and built-in colored mesh pipeline.
pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    model_buffer: wgpu::Buffer,
    model_bind_group_layout: wgpu::BindGroupLayout,
    model_bind_group: wgpu::BindGroup,
    model_stride: u64,
    meshes: Vec<GpuMesh>,
    clear_color: wgpu::Color,
}

impl Renderer {
    /// Creates a renderer and keeps the supplied window alive through its surface.
    pub async fn new(window: Arc<Window>) -> Result<Self, RendererError> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance.create_surface(window)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("terrarium device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
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
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: capabilities.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let (depth_texture, depth_view) = create_depth_texture(&device, &config);
        let model_stride = aligned_uniform_size(
            std::mem::size_of::<ModelUniform>() as u64,
            device.limits().min_uniform_buffer_offset_alignment as u64,
        );
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera uniform buffer"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let model_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("model uniform buffer"),
            size: model_stride,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("camera bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let model_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("model bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: NonZeroU64::new(
                            std::mem::size_of::<ModelUniform>() as u64
                        ),
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
        let model_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("model bind group"),
            layout: &model_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &model_buffer,
                    offset: 0,
                    size: NonZeroU64::new(std::mem::size_of::<ModelUniform>() as u64),
                }),
            }],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("colored mesh shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh pipeline layout"),
            bind_group_layouts: &[
                Some(&camera_bind_group_layout),
                Some(&model_bind_group_layout),
            ],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("colored mesh pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Some(Vertex::layout())],
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

        Ok(Self {
            surface,
            device,
            queue,
            config,
            depth_texture,
            depth_view,
            pipeline,
            camera_buffer,
            camera_bind_group,
            model_buffer,
            model_bind_group_layout,
            model_bind_group,
            model_stride,
            meshes: Vec::new(),
            clear_color: wgpu::Color {
                r: 0.018,
                g: 0.028,
                b: 0.065,
                a: 1.0,
            },
        })
    }

    pub fn set_clear_color(&mut self, color: wgpu::Color) {
        self.clear_color = color;
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        (self.depth_texture, self.depth_view) = create_depth_texture(&self.device, &self.config);
    }

    /// Uploads a mesh and returns the handle used by [`Scene::add`].
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

    fn ensure_model_capacity(&mut self, object_count: usize) {
        let required_size = self.model_stride * object_count.max(1) as u64;
        if self.model_buffer.size() >= required_size {
            return;
        }

        self.model_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("model uniform buffer"),
            size: required_size,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.model_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("model bind group"),
            layout: &self.model_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &self.model_buffer,
                    offset: 0,
                    size: NonZeroU64::new(std::mem::size_of::<ModelUniform>() as u64),
                }),
            }],
        });
    }

    /// Renders all objects in a scene. A lost or outdated surface is reconfigured and retried on
    /// the next frame; minimized and occluded windows simply skip their frame.
    pub fn render(&mut self, scene: &Scene, camera: &Camera) -> Result<(), RendererError> {
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
            view_projection: camera.view_projection_matrix().to_cols_array_2d(),
        };
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&camera_uniform));
        self.ensure_model_capacity(scene.objects().len());
        for (index, object) in scene.objects().iter().enumerate() {
            let model_uniform = ModelUniform {
                model: object.transform.to_cols_array_2d(),
            };
            self.queue.write_buffer(
                &self.model_buffer,
                index as u64 * self.model_stride,
                bytemuck::bytes_of(&model_uniform),
            );
        }

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
            for (index, object) in scene.objects().iter().enumerate() {
                let Some(mesh) = self.meshes.get(object.mesh.0) else {
                    continue;
                };
                pass.set_bind_group(
                    1,
                    &self.model_bind_group,
                    &[index as u32 * self.model_stride as u32],
                );
                pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }
        }
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
        Ok(())
    }
}

fn aligned_uniform_size(size: u64, alignment: u64) -> u64 {
    let alignment = alignment.max(1);
    size.div_ceil(alignment) * alignment
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
