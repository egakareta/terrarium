use crate::AppCreationError;

/// Adapter and device configuration for native and browser rendering.
pub struct WgpuConfiguration {
    /// Backend selection and validation settings.
    pub instance_descriptor: wgpu::InstanceDescriptor,
    /// Adapter power preference.
    pub power_preference: wgpu::PowerPreference,
    /// Whether to request a software adapter.
    pub force_fallback_adapter: bool,
    /// Required device features and limits.
    pub device_descriptor: wgpu::DeviceDescriptor<'static>,
}

impl Default for WgpuConfiguration {
    fn default() -> Self {
        Self {
            instance_descriptor: wgpu::InstanceDescriptor::new_without_display_handle_from_env(),
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            device_descriptor: wgpu::DeviceDescriptor {
                label: Some("Terrarium device"),
                required_limits: wgpu::Limits::downlevel_defaults(),
                ..Default::default()
            },
        }
    }
}

/// GPU resources owned independently of the windowing and UI layers.
#[derive(Clone)]
pub struct RenderState {
    /// Instance kept alive for the lifetime of all renderers and surfaces.
    pub instance: wgpu::Instance,
    /// Selected GPU adapter.
    pub adapter: wgpu::Adapter,
    /// Shared graphics device.
    pub device: wgpu::Device,
    /// Shared command queue.
    pub queue: wgpu::Queue,
    /// Format of the final scene output.
    pub target_format: wgpu::TextureFormat,
}

impl RenderState {
    pub(crate) async fn create(
        options: WgpuConfiguration,
        instance: wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<Self, AppCreationError> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: options.power_preference,
                force_fallback_adapter: options.force_fallback_adapter,
                compatible_surface: surface,
                ..Default::default()
            })
            .await?;
        let mut descriptor = options.device_descriptor;
        if adapter.get_info().backend == wgpu::Backend::Gl
            && descriptor.required_limits == wgpu::Limits::downlevel_defaults()
        {
            descriptor.required_limits = wgpu::Limits::downlevel_webgl2_defaults();
        }
        descriptor.required_limits = descriptor
            .required_limits
            .using_resolution(adapter.limits());
        #[cfg(not(target_arch = "wasm32"))]
        if adapter.features().contains(wgpu::Features::PIPELINE_CACHE) {
            descriptor.required_features |= wgpu::Features::PIPELINE_CACHE;
        }
        let (device, queue) = adapter.request_device(&descriptor).await?;
        let target_format = surface.map_or(wgpu::TextureFormat::Rgba8Unorm, |surface| {
            let capabilities = surface.get_capabilities(&adapter);
            capabilities
                .formats
                .iter()
                .copied()
                .find(|format| {
                    matches!(
                        format,
                        wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
                    )
                })
                .unwrap_or(capabilities.formats[0])
        });
        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            target_format,
        })
    }
}

/// Resources made available to application initialization code.
#[derive(Clone)]
pub struct CreationContext {
    /// Renderer device, queue, adapter, and output format.
    pub wgpu_render_state: RenderState,
}
