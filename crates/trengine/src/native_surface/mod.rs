use serde::Deserialize;

use crate::{AppCreationError, RenderState, WgpuConfiguration};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
use linux::Child;
#[cfg(target_os = "macos")]
use macos::Child;
#[cfg(target_os = "windows")]
use windows::Child;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
impl Rect {
    pub(crate) fn full(size: [u32; 2]) -> Self {
        Self {
            x: 0,
            y: 0,
            width: size[0],
            height: size[1],
        }
    }
    pub(crate) fn intersection(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let end_x = (self.x + self.width).min(other.x + other.width);
        let end_y = (self.y + self.height).min(other.y + other.height);
        (end_x > x && end_y > y).then_some(Self {
            x,
            y,
            width: end_x.saturating_sub(x),
            height: end_y.saturating_sub(y),
        })
    }
}

pub(crate) fn visible_regions(size: [u32; 2], occlusions: &[Rect]) -> Vec<Rect> {
    let mut regions = vec![Rect::full(size)];
    for &hole in occlusions {
        let mut next = Vec::new();
        for region in regions {
            let Some(overlap) = region.intersection(hole) else {
                next.push(region);
                continue;
            };
            if overlap.y > region.y {
                next.push(Rect {
                    height: overlap.y - region.y,
                    ..region
                });
            }
            let bottom = overlap.y + overlap.height;
            if bottom < region.y + region.height {
                next.push(Rect {
                    y: bottom,
                    height: region.y + region.height - bottom,
                    ..region
                });
            }
            if overlap.x > region.x {
                next.push(Rect {
                    y: overlap.y,
                    width: overlap.x - region.x,
                    height: overlap.height,
                    ..region
                });
            }
            let right = overlap.x + overlap.width;
            if right < region.x + region.width {
                next.push(Rect {
                    x: right,
                    y: overlap.y,
                    width: region.x + region.width - right,
                    height: overlap.height,
                });
            }
        }
        regions = next;
    }
    regions
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub(crate) struct Layout {
    pub scene: [f32; 4],
    pub occlusions: Vec<[f32; 4]>,
    pub viewports: Vec<ViewportRegion>,
}
#[derive(Deserialize)]
pub(crate) struct ViewportRegion {
    pub key: String,
    pub id: String,
    pub bounds: [f32; 4],
    pub clip: [f32; 4],
    #[serde(default)]
    pub occlusions: Vec<[f32; 4]>,
}
impl Layout {
    pub(crate) fn physical_rect(bounds: [f32; 4], scale: f32, size: [u32; 2]) -> Option<Rect> {
        if !bounds.iter().all(|v| v.is_finite()) || bounds[2] <= 0.0 || bounds[3] <= 0.0 {
            return None;
        }
        let x = (bounds[0] * scale).floor().clamp(0.0, size[0] as f32) as u32;
        let y = (bounds[1] * scale).floor().clamp(0.0, size[1] as f32) as u32;
        let end_x = ((bounds[0] + bounds[2]) * scale)
            .ceil()
            .clamp(0.0, size[0] as f32) as u32;
        let end_y = ((bounds[1] + bounds[3]) * scale)
            .ceil()
            .clamp(0.0, size[1] as f32) as u32;
        (end_x > x && end_y > y).then_some(Rect {
            x,
            y,
            width: end_x.saturating_sub(x),
            height: end_y.saturating_sub(y),
        })
    }
}

pub(crate) struct Presenter {
    // The swapchain must be destroyed before the native child drawable.
    surface: wgpu::Surface<'static>,
    child: Child,
    configuration: wgpu::SurfaceConfiguration,
    regions: Vec<Rect>,
}
impl Presenter {
    pub(crate) fn initialize(
        window: &tauri::WebviewWindow,
        mut options: WgpuConfiguration,
        size: [u32; 2],
    ) -> Result<(Self, RenderState), AppCreationError> {
        let mut child = Child::new(window.clone())?;
        child.layout(
            Bounds {
                x: 0,
                y: 0,
                width: size[0],
                height: size[1],
            },
            window.scale_factor()?,
            &[],
        )?;
        let descriptor = std::mem::replace(
            &mut options.instance_descriptor,
            wgpu::InstanceDescriptor::new_without_display_handle(),
        );
        let instance = wgpu::Instance::new(child.instance_descriptor(descriptor));
        let surface = Self::surface(&instance, &child)?;
        let state = pollster::block_on(RenderState::create(options, instance, Some(&surface)))?;
        let configuration = Self::configuration(&surface, &state, size, child.requires_alpha())?;
        surface.configure(&state.device, &configuration);
        Ok((
            Self {
                surface,
                child,
                configuration,
                regions: Vec::new(),
            },
            state,
        ))
    }
    pub(crate) fn new(
        window: &tauri::WebviewWindow,
        state: &RenderState,
        size: [u32; 2],
        owner: &Self,
    ) -> Result<Self, AppCreationError> {
        let mut child = Child::new_on_display(window.clone(), &owner.child)?;
        child.layout(
            Bounds {
                x: 0,
                y: 0,
                width: size[0],
                height: size[1],
            },
            window.scale_factor()?,
            &[],
        )?;
        let surface = Self::surface(&state.instance, &child)?;
        let configuration = Self::configuration(&surface, state, size, child.requires_alpha())?;
        surface.configure(&state.device, &configuration);
        Ok(Self {
            surface,
            child,
            configuration,
            regions: Vec::new(),
        })
    }
    fn surface(
        instance: &wgpu::Instance,
        child: &Child,
    ) -> Result<wgpu::Surface<'static>, AppCreationError> {
        let (raw_display_handle, raw_window_handle) = child.handles()?;
        // Each presenter owns the drawable and drops the surface first. All platform operations
        // and swapchain access occur on Tauri's UI thread.
        Ok(unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(raw_display_handle),
                raw_window_handle,
            })
        }?)
    }
    fn configuration(
        surface: &wgpu::Surface<'_>,
        state: &RenderState,
        size: [u32; 2],
        requires_alpha: bool,
    ) -> Result<wgpu::SurfaceConfiguration, AppCreationError> {
        let capabilities = surface.get_capabilities(&state.adapter);
        if !capabilities.formats.contains(&state.target_format) {
            return Err("native child surface has an incompatible pixel format".into());
        }
        let mut configuration = surface
            .get_default_config(&state.adapter, size[0].max(1), size[1].max(1))
            .ok_or("native surface configuration is unavailable")?;
        configuration.format = state.target_format;
        let alpha = [
            wgpu::CompositeAlphaMode::PreMultiplied,
            wgpu::CompositeAlphaMode::PostMultiplied,
        ]
        .into_iter()
        .find(|mode| capabilities.alpha_modes.contains(mode));
        if requires_alpha && alpha.is_none() {
            return Err("native surface does not support UI transparency".into());
        }
        if let Some(alpha) = alpha {
            configuration.alpha_mode = alpha;
        }
        Ok(configuration)
    }
    pub(crate) fn layout(
        &mut self,
        state: &RenderState,
        bounds: Bounds,
        scale: f64,
        regions: Vec<Rect>,
    ) -> Result<(), AppCreationError> {
        self.child.layout(bounds, scale, &regions)?;
        if self.configuration.width != bounds.width || self.configuration.height != bounds.height {
            self.configuration.width = bounds.width;
            self.configuration.height = bounds.height;
            self.surface.configure(&state.device, &self.configuration);
        }
        self.regions = regions;
        Ok(())
    }
    pub(crate) fn present(
        &mut self,
        state: &RenderState,
        paint: impl FnOnce(&wgpu::TextureView, &[Rect]),
    ) -> Result<(), AppCreationError> {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                let target = texture
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                paint(&target, &self.regions);
                state.queue.present(texture);
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&state.device, &self.configuration)
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {}
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("native surface validation failed".into());
            }
        }
        Ok(())
    }
    pub(crate) fn parent_destroyed(&mut self) {
        self.child.parent_destroyed();
    }
}
