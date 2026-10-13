use std::{ffi::c_void, ptr::NonNull};

use objc2::{MainThreadOnly, define_class, msg_send, rc::Retained};
use objc2_app_kit::{NSView, NSWindow};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};
use wgpu::rwh::{AppKitDisplayHandle, AppKitWindowHandle, RawDisplayHandle, RawWindowHandle};

use super::{Bounds, Rect};
use crate::AppCreationError;

define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    struct GpuView;
    impl GpuView {
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, _point: NSPoint) -> Option<&NSView> { None }
    }
);

pub(super) struct Child {
    window: tauri::WebviewWindow,
    view: Retained<GpuView>,
}
impl Child {
    pub fn requires_alpha(&self) -> bool {
        true
    }
    pub fn new(window: tauri::WebviewWindow) -> Result<Self, AppCreationError> {
        let thread =
            MainThreadMarker::new().ok_or("AppKit surface creation must run on the main thread")?;
        let pointer = window.ns_window()?;
        let native = NonNull::new(pointer).ok_or("NSWindow is unavailable")?;
        // Tauri owns the NSWindow. The retained child view remains alive until its swapchain drops.
        let native: &NSWindow = unsafe { native.cast().as_ref() };
        let content = native.contentView().ok_or("NSWindow has no content view")?;
        let view: Retained<GpuView> = unsafe {
            msg_send![GpuView::alloc(thread), initWithFrame: NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1.0, 1.0))]
        };
        view.setWantsLayer(true);
        content.addSubview(&view);
        Ok(Self { window, view })
    }
    pub fn instance_descriptor(
        &self,
        descriptor: wgpu::InstanceDescriptor,
    ) -> wgpu::InstanceDescriptor {
        descriptor.with_display_handle(Box::new(self.window.clone()))
    }
    pub fn handles(&self) -> Result<(RawDisplayHandle, RawWindowHandle), AppCreationError> {
        let view = NonNull::from(&*self.view).cast::<c_void>();
        Ok((
            RawDisplayHandle::AppKit(AppKitDisplayHandle::new()),
            RawWindowHandle::AppKit(AppKitWindowHandle::new(view)),
        ))
    }
    pub fn layout(
        &mut self,
        bounds: Bounds,
        scale: f64,
        _regions: &[Rect],
    ) -> Result<(), AppCreationError> {
        let parent = unsafe { self.view.superview() }.ok_or("native GPU view has been detached")?;
        let height = parent.bounds().size.height;
        self.view.setFrame(NSRect::new(
            NSPoint::new(
                bounds.x as f64 / scale,
                height - (bounds.y as f64 + bounds.height as f64) / scale,
            ),
            NSSize::new(bounds.width as f64 / scale, bounds.height as f64 / scale),
        ));
        Ok(())
    }
    pub fn parent_destroyed(&mut self) {}
}
impl Drop for Child {
    fn drop(&mut self) {
        self.view.removeFromSuperview();
    }
}
