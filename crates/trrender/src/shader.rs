use std::{
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicU8, AtomicUsize, Ordering},
    },
};

use super::*;

static NEXT_SHADER: AtomicUsize = AtomicUsize::new(1);

const PENDING: u8 = 0;
const READY: u8 = 1;
const ABANDONED: u8 = 2;

pub(super) struct ShaderRegistration(Arc<AtomicU8>);

impl ShaderRegistration {
    fn new() -> Self {
        Self(Arc::new(AtomicU8::new(PENDING)))
    }

    pub fn is_ready(&self) -> bool {
        self.0.load(Ordering::Acquire) == READY
    }

    fn is_live(&self) -> bool {
        self.0.load(Ordering::Acquire) != ABANDONED
    }
}

// Created before the future so dropping even an unpolled future marks it abandoned.
struct PendingRegistration(Arc<AtomicU8>);

impl PendingRegistration {
    fn complete(&self) {
        self.0.store(READY, Ordering::Release);
    }
}

impl Drop for PendingRegistration {
    fn drop(&mut self) {
        let _ = self
            .0
            .compare_exchange(PENDING, ABANDONED, Ordering::Release, Ordering::Relaxed);
    }
}

async fn finish_registration<T>(
    validation: impl Future<Output = Option<wgpu::Error>>,
    pending: PendingRegistration,
    label: String,
    handle: T,
) -> Result<T, RendererError> {
    if let Some(error) = validation.await {
        return Err(RendererError::InvalidShader {
            label,
            message: error.to_string(),
        });
    }
    pending.complete();
    Ok(handle)
}

pub(super) const DEFAULT_SURFACE: &str =
    "fn shade_surface(input: SurfaceInput, surface: Surface) -> Surface { return surface; }";

pub(super) fn mesh_shader_source(source: &str) -> String {
    format!(
        "{}\n{}\n{source}",
        include_str!("surface.wgsl"),
        include_str!("shader.wgsl")
    )
}

pub(super) struct MaterialShader {
    pub pipeline: wgpu::RenderPipeline,
    pub transparent_pipeline: wgpu::RenderPipeline,
    pub registration: ShaderRegistration,
}

pub(super) struct ShaderSelection {
    pub layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    pub stride: u32,
}

impl ShaderSelection {
    pub fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("surface shader selection layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(16),
                },
                count: None,
            }],
        });
        let stride = align_to(
            16,
            u64::from(device.limits().min_uniform_buffer_offset_alignment),
        ) as u32;
        let mut masks = vec![0; stride as usize * (1 << MATERIAL_SLOT_COUNT)];
        for mask in 0..(1u32 << MATERIAL_SLOT_COUNT) {
            let offset = (mask * stride) as usize;
            masks[offset..offset + 4].copy_from_slice(bytemuck::bytes_of(&mask));
        }
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("surface shader selection masks"),
            contents: &masks,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("surface shader selection"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(16),
                }),
            }],
        });
        Self {
            layout,
            bind_group,
            stride,
        }
    }
}

/// A fullscreen effect registered with a particular renderer.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PostProcessHandle(
    /// Opaque registration identity.
    pub usize,
);

struct PostProcess {
    pipeline: wgpu::RenderPipeline,
    parameters: wgpu::Buffer,
    inputs: Option<[wgpu::BindGroup; 2]>,
    registration: ShaderRegistration,
}

struct PostProcessTargets {
    size: [u32; 2],
    views: [wgpu::TextureView; 2],
}

pub(super) struct PostProcessing {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    effects: HashMap<PostProcessHandle, PostProcess>,
    active: Vec<PostProcessHandle>,
    targets: Option<PostProcessTargets>,
}

impl PostProcessing {
    pub fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post-process layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(16),
                    },
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("post-process sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            layout,
            sampler,
            effects: HashMap::default(),
            active: Vec::new(),
            targets: None,
        }
    }

    fn inputs(
        &self,
        device: &wgpu::Device,
        parameters: &wgpu::Buffer,
    ) -> Option<[wgpu::BindGroup; 2]> {
        let targets = self.targets.as_ref()?;
        Some(std::array::from_fn(|index| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("post-process input"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&targets.views[index]),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: parameters.as_entire_binding(),
                    },
                ],
            })
        }))
    }

    pub fn ensure_targets(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        size: [u32; 2],
    ) {
        if self.active.is_empty()
            || self
                .targets
                .as_ref()
                .is_some_and(|targets| targets.size == size)
        {
            return;
        }
        self.targets = Some(PostProcessTargets {
            size,
            views: std::array::from_fn(|_| {
                create_output_texture(device, format, size[0], size[1])
                    .create_view(&Default::default())
            }),
        });
        let inputs: Vec<_> = self
            .effects
            .iter()
            .map(|(&handle, effect)| (handle, self.inputs(device, &effect.parameters)))
            .collect();
        for (handle, inputs) in inputs {
            self.effects
                .get_mut(&handle)
                .expect("registered effect")
                .inputs = inputs;
        }
    }

    pub fn scene_target(&self) -> Option<&wgpu::TextureView> {
        if self.active.is_empty() {
            None
        } else {
            self.targets.as_ref().map(|targets| &targets.views[0])
        }
    }

    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder, output: &wgpu::TextureView) {
        let Some(targets) = self.targets.as_ref() else {
            return;
        };
        for (index, handle) in self.active.iter().enumerate() {
            let effect = &self.effects[handle];
            let output = if index + 1 == self.active.len() {
                output
            } else {
                &targets.views[(index + 1) % 2]
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("custom post-process pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&effect.pipeline);
            pass.set_bind_group(
                0,
                &effect
                    .inputs
                    .as_ref()
                    .expect("prepared post-process targets")[index % 2],
                &[],
            );
            pass.draw(0..3, 0..1);
        }
    }
}

impl Renderer {
    pub(super) fn prune_shader_registrations(&mut self) {
        self.material_shaders
            .retain(|_, shader| shader.registration.is_live());
        self.post_processing
            .effects
            .retain(|_, effect| effect.registration.is_live());
    }

    /// Registers WGSL defining `fn shade_surface(input: SurfaceInput, surface: Surface) -> Surface`.
    ///
    /// The hook runs after texture sampling and before PBR lighting, fog, exposure,
    /// and tone mapping. See the [WGSL interface](Self::SURFACE_SHADER_INTERFACE).
    /// Assign the returned handle with [`Material::with_shader`]. Each distinct
    /// shader used by a mesh adds a draw of that mesh's matching material slots.
    ///
    /// Validation failures leave existing registrations unchanged. Handles belong
    /// to this renderer; register separately for other renderers.
    ///
    /// The returned future owns its data and does not borrow the renderer. When
    /// accessing it through `Engine::renderer()`, store the future in a local
    /// before awaiting it so the renderer guard is released first.
    ///
    /// ```no_run
    /// # use trrender::{Renderer, RendererError, Material};
    /// # async fn configure(renderer: &mut Renderer) -> Result<(), RendererError> {
    /// let pending = renderer.register_material_shader("stripes", r#"
    ///     fn shade_surface(input: SurfaceInput, surface: Surface) -> Surface {
    ///         var result = surface;
    ///         let stripe = step(0.5, fract(input.uv.x * input.parameters.x));
    ///         result.base_color = vec4<f32>(surface.base_color.rgb * stripe, surface.base_color.a);
    ///         return result;
    ///     }
    /// "#);
    /// let shader = pending.await?;
    /// let material = Material::default()
    ///     .with_shader(shader)
    ///     .with_shader_parameters([8.0, 0.0, 0.0, 0.0]);
    /// # Ok(()) }
    /// ```
    pub fn register_material_shader(
        &mut self,
        label: &str,
        source: &str,
    ) -> impl Future<Output = Result<MaterialShaderHandle, RendererError>> + use<> {
        self.prune_shader_registrations();
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(mesh_shader_source(source).into()),
            });
        let pipeline = init::create_mesh_pipeline(
            &self.device,
            &self.mesh_pipeline_layout,
            &module,
            self.output.format,
            false,
            None,
        );
        let transparent_pipeline = init::create_mesh_pipeline(
            &self.device,
            &self.mesh_pipeline_layout,
            &module,
            self.output.format,
            true,
            None,
        );
        let validation = scope.pop();
        let registration = ShaderRegistration::new();
        let pending = PendingRegistration(Arc::clone(&registration.0));
        let handle = MaterialShaderHandle(NEXT_SHADER.fetch_add(1, Ordering::Relaxed));
        self.material_shaders.insert(
            handle,
            MaterialShader {
                pipeline,
                transparent_pipeline,
                registration,
            },
        );
        finish_registration(validation, pending, label.into(), handle)
    }

    /// WGSL types available to material hooks; no extra bindings or entry points are required.
    ///
    /// `SurfaceInput` contains world-space position, geometric normal, direction
    /// toward the camera, mesh UVs, and [`Material::shader_parameters`]. `Surface`
    /// contains sampled linear base color (including tint), world-space mapped
    /// normal, emissive, and PBR factors. Return the modified surface. Set `unlit`
    /// to `1u` to use `base_color.rgb + emissive` without lighting; fog, exposure,
    /// and tone mapping still apply. Alpha uses the part's transparency mode;
    /// enable part transparency for blending. Shadows and outlines retain the
    /// original geometry and do not execute the hook. Extra renderer internals
    /// are not part of this interface.
    pub const SURFACE_SHADER_INTERFACE: &str = include_str!("surface.wgsl");

    /// Registers WGSL defining `fn post_process(input: PostProcessInput) -> vec4<f32>`.
    ///
    /// `PostProcessInput` has `uv: vec2<f32>` (origin at the top left),
    /// `resolution: vec2<f32>`, and `parameters: vec4<f32>` (initially zero).
    /// `sample_scene(uv)` samples the preceding pass with linear filtering and
    /// clamped edges. Both sampled and returned RGB are linear, display-referred
    /// values after tone mapping; alpha is straight, not premultiplied.
    ///
    /// Effects run after scene outlines, before eframe UI. Registration does not
    /// activate an effect; use [`Self::set_post_processing`]. Invalid WGSL returns
    /// [`RendererError::InvalidShader`] without changing existing registrations.
    ///
    /// As with [`Self::register_material_shader`], the returned future owns its
    /// data. Release any renderer guard before awaiting validation.
    ///
    /// ```no_run
    /// # use trrender::{Renderer, RendererError};
    /// # async fn configure(renderer: &mut Renderer) -> Result<(), RendererError> {
    /// let effect = renderer.register_post_process("vignette", r#"
    ///     fn post_process(input: PostProcessInput) -> vec4<f32> {
    ///         let color = sample_scene(input.uv);
    ///         let offset = input.uv - vec2<f32>(0.5);
    ///         let shade = 1.0 - input.parameters.x * dot(offset, offset);
    ///         return vec4<f32>(color.rgb * shade, color.a);
    ///     }
    /// "#).await?;
    /// renderer.set_post_process_parameters(effect, [0.8, 0.0, 0.0, 0.0])?;
    /// renderer.set_post_processing(&[effect])?;
    /// # Ok(()) }
    /// ```
    pub fn register_post_process(
        &mut self,
        label: &str,
        source: &str,
    ) -> impl Future<Output = Result<PostProcessHandle, RendererError>> + use<> {
        self.prune_shader_registrations();
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(
                    format!("{}\n{source}", include_str!("post_process.wgsl")).into(),
                ),
            });
        let layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &[Some(&self.post_processing.layout)],
                immediate_size: 0,
            });
        let constants = [(
            "FRAMEBUFFER_IS_SRGB",
            if self.output.format.is_srgb() {
                1.0
            } else {
                0.0
            },
        )];
        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &constants,
                        ..Default::default()
                    },
                    targets: &[Some(wgpu::ColorTargetState {
                        format: self.output.format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            });
        let validation = scope.pop();
        let registration = ShaderRegistration::new();
        let pending = PendingRegistration(Arc::clone(&registration.0));
        let parameters = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: bytemuck::cast_slice(&[0.0f32; 4]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        let inputs = self.post_processing.inputs(&self.device, &parameters);
        let handle = PostProcessHandle(NEXT_SHADER.fetch_add(1, Ordering::Relaxed));
        self.post_processing.effects.insert(
            handle,
            PostProcess {
                pipeline,
                parameters,
                inputs,
                registration,
            },
        );
        finish_registration(validation, pending, label.into(), handle)
    }

    /// Replaces the ordered fullscreen effect chain. An empty slice disables it.
    ///
    /// Handles must belong to this renderer. Repeated handles apply the same
    /// effect repeatedly with shared parameters. Invalid handles leave the chain
    /// unchanged. The chain applies to both owned and caller-owned output targets.
    pub fn set_post_processing(
        &mut self,
        effects: &[PostProcessHandle],
    ) -> Result<(), RendererError> {
        for &handle in effects {
            if !self
                .post_processing
                .effects
                .get(&handle)
                .is_some_and(|effect| effect.registration.is_ready())
            {
                return Err(RendererError::InvalidPostProcess(handle));
            }
        }
        self.post_processing.active = effects.to_vec();
        if effects.is_empty() {
            self.post_processing.targets = None;
            for effect in self.post_processing.effects.values_mut() {
                effect.inputs = None;
            }
        }
        Ok(())
    }

    /// Updates an effect's `PostProcessInput.parameters` without recompiling WGSL.
    ///
    /// Submit previously encoded frames before changing parameters.
    pub fn set_post_process_parameters(
        &mut self,
        effect: PostProcessHandle,
        parameters: [f32; 4],
    ) -> Result<(), RendererError> {
        let effect = self
            .post_processing
            .effects
            .get(&effect)
            .filter(|effect| effect.registration.is_ready())
            .ok_or(RendererError::InvalidPostProcess(effect))?;
        self.queue
            .write_buffer(&effect.parameters, 0, bytemuck::cast_slice(&parameters));
        Ok(())
    }
}
