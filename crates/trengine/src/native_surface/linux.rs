use std::{ptr::NonNull, sync::Arc};

use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle,
    globals::{GlobalListContents, registry_queue_init},
    protocol::{
        wl_compositor, wl_region, wl_registry, wl_subcompositor, wl_subsurface, wl_surface,
    },
};
use wgpu::rwh::{
    DisplayHandle, HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
    XlibDisplayHandle, XlibWindowHandle,
};
use x11_dl::{xfixes, xlib};

use super::{Bounds, Rect};
use crate::AppCreationError;

pub(super) enum Child {
    X11(Box<X11Child>),
    Wayland(Box<WaylandChild>),
}
impl Child {
    pub fn requires_alpha(&self) -> bool {
        matches!(self, Self::Wayland(_))
    }
    pub fn new(window: tauri::WebviewWindow) -> Result<Self, AppCreationError> {
        match window.window_handle()?.as_raw() {
            RawWindowHandle::Xlib(parent) => Ok(Self::X11(Box::new(X11Child::new(parent.window)?))),
            RawWindowHandle::Wayland(parent) => {
                let RawDisplayHandle::Wayland(display) = window.display_handle()?.as_raw() else {
                    return Err("Wayland window has no Wayland display".into());
                };
                Ok(Self::Wayland(Box::new(WaylandChild::new(
                    window,
                    display.display.as_ptr() as usize,
                    parent.surface.as_ptr() as usize,
                )?)))
            }
            _ => Err("unsupported Linux native window handle".into()),
        }
    }
    pub fn handles(&self) -> Result<(RawDisplayHandle, RawWindowHandle), AppCreationError> {
        Ok(match self {
            Self::X11(child) => (
                child.connection.raw_handle(),
                RawWindowHandle::Xlib(XlibWindowHandle::new(child.window)),
            ),
            Self::Wayland(child) => (
                RawDisplayHandle::Wayland(wgpu::rwh::WaylandDisplayHandle::new(
                    NonNull::new(child.display as *mut _).ok_or("Wayland display was destroyed")?,
                )),
                RawWindowHandle::Wayland(wgpu::rwh::WaylandWindowHandle::new(
                    NonNull::new(child.surface.id().as_ptr().cast())
                        .ok_or("Wayland surface was destroyed")?,
                )),
            ),
        })
    }
    pub fn instance_descriptor(
        &self,
        descriptor: wgpu::InstanceDescriptor,
    ) -> wgpu::InstanceDescriptor {
        match self {
            Self::X11(child) => descriptor.with_display_handle(Box::new(child.connection.clone())),
            Self::Wayland(child) => descriptor.with_display_handle(Box::new(child.window.clone())),
        }
    }
    pub fn layout(
        &mut self,
        bounds: Bounds,
        scale: f64,
        regions: &[Rect],
    ) -> Result<(), AppCreationError> {
        match self {
            Self::X11(child) => child.layout(bounds, regions),
            Self::Wayland(child) => child.layout(bounds, scale),
        }
    }
    pub fn parent_destroyed(&mut self) {
        if let Self::X11(child) = self {
            child.alive = false;
        }
    }
}

struct X11Connection {
    api: xlib::Xlib,
    display: usize,
    screen: i32,
}
impl std::fmt::Debug for X11Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("X11Connection")
            .field("screen", &self.screen)
            .finish_non_exhaustive()
    }
}
impl X11Connection {
    fn raw_handle(&self) -> RawDisplayHandle {
        RawDisplayHandle::Xlib(XlibDisplayHandle::new(
            NonNull::new(self.display as *mut _),
            self.screen,
        ))
    }
}
impl HasDisplayHandle for X11Connection {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, wgpu::rwh::HandleError> {
        // The Arc owner outlives every borrowed display handle, including the WGPU instance.
        Ok(unsafe { DisplayHandle::borrow_raw(self.raw_handle()) })
    }
}
impl Drop for X11Connection {
    fn drop(&mut self) {
        unsafe {
            (self.api.XCloseDisplay)(self.display as *mut _);
        }
    }
}

pub(super) struct X11Child {
    connection: Arc<X11Connection>,
    fixes: xfixes::Xlib,
    window: u64,
    alive: bool,
    last: Option<(Bounds, Vec<Rect>)>,
}
impl X11Child {
    fn new(parent: u64) -> Result<Self, AppCreationError> {
        let api = xlib::Xlib::open()?;
        let fixes = xfixes::Xlib::open()?;
        unsafe {
            (api.XInitThreads)();
        }
        let display = unsafe { (api.XOpenDisplay)(std::ptr::null()) };
        if display.is_null() {
            return Err("could not open the X11 display".into());
        }
        let screen = unsafe { (api.XDefaultScreen)(display) };
        let connection = Arc::new(X11Connection {
            api,
            display: display as usize,
            screen,
        });
        let mut event = 0;
        let mut error = 0;
        if unsafe { (fixes.XFixesQueryExtension)(display, &mut event, &mut error) } == 0 {
            return Err("XFixes is required for native UI clipping".into());
        }
        let window =
            unsafe { (connection.api.XCreateSimpleWindow)(display, parent, 0, 0, 1, 1, 0, 0, 0) };
        let empty = unsafe { (fixes.XFixesCreateRegion)(display, std::ptr::null_mut(), 0) };
        unsafe {
            // Empty input shape lets the webview's DOM canvas receive scene gestures.
            (fixes.XFixesSetWindowShapeRegion)(display, window, 2, 0, 0, empty);
            (fixes.XFixesDestroyRegion)(display, empty);
            (connection.api.XMapWindow)(display, window);
            (connection.api.XFlush)(display);
        }
        Ok(Self {
            connection,
            fixes,
            window,
            alive: true,
            last: None,
        })
    }
    fn layout(&mut self, bounds: Bounds, regions: &[Rect]) -> Result<(), AppCreationError> {
        if self
            .last
            .as_ref()
            .is_some_and(|(old, mask)| *old == bounds && mask == regions)
        {
            return Ok(());
        }
        let display = self.connection.display as *mut _;
        let mut rectangles: Vec<xlib::XRectangle> = regions
            .iter()
            .map(|rect| xlib::XRectangle {
                x: rect.x as i16,
                y: rect.y as i16,
                width: rect.width as u16,
                height: rect.height as u16,
            })
            .collect();
        unsafe {
            (self.connection.api.XMoveResizeWindow)(
                display,
                self.window,
                bounds.x,
                bounds.y,
                bounds.width,
                bounds.height,
            );
            let region = (self.fixes.XFixesCreateRegion)(
                display,
                rectangles.as_mut_ptr(),
                rectangles.len() as i32,
            );
            (self.fixes.XFixesSetWindowShapeRegion)(display, self.window, 0, 0, 0, region);
            (self.fixes.XFixesDestroyRegion)(display, region);
            (self.connection.api.XRaiseWindow)(display, self.window);
            (self.connection.api.XFlush)(display);
        }
        self.last = Some((bounds, regions.to_vec()));
        Ok(())
    }
}
impl Drop for X11Child {
    fn drop(&mut self) {
        if self.alive {
            unsafe {
                (self.connection.api.XDestroyWindow)(
                    self.connection.display as *mut _,
                    self.window,
                );
            }
        }
    }
}

struct WaylandState;
impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for WaylandState {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
wayland_client::delegate_noop!(WaylandState: ignore wl_compositor::WlCompositor);
wayland_client::delegate_noop!(WaylandState: ignore wl_subcompositor::WlSubcompositor);
wayland_client::delegate_noop!(WaylandState: ignore wl_surface::WlSurface);
wayland_client::delegate_noop!(WaylandState: ignore wl_subsurface::WlSubsurface);
wayland_client::delegate_noop!(WaylandState: ignore wl_region::WlRegion);

pub(super) struct WaylandChild {
    window: tauri::WebviewWindow,
    display: usize,
    connection: Connection,
    queue: EventQueue<WaylandState>,
    surface: wl_surface::WlSurface,
    subsurface: wl_subsurface::WlSubsurface,
    parent: wl_surface::WlSurface,
}
impl WaylandChild {
    fn new(
        window: tauri::WebviewWindow,
        display: usize,
        parent: usize,
    ) -> Result<Self, AppCreationError> {
        // Tauri retains the GTK display and parent surface; this guest connection never closes it.
        let backend =
            unsafe { wayland_backend::client::Backend::from_foreign_display(display as *mut _) };
        let connection = Connection::from_backend(backend);
        let (globals, queue) = registry_queue_init::<WaylandState>(&connection)?;
        let handle = queue.handle();
        let compositor = globals.bind::<wl_compositor::WlCompositor, _, _>(&handle, 1..=6, ())?;
        let subcompositor =
            globals.bind::<wl_subcompositor::WlSubcompositor, _, _>(&handle, 1..=1, ())?;
        let id = unsafe {
            wayland_backend::client::ObjectId::from_ptr(
                wl_surface::WlSurface::interface(),
                parent as *mut _,
            )
        }?;
        let parent = wl_surface::WlSurface::from_id(&connection, id)?;
        let surface = compositor.create_surface(&handle, ());
        let region = compositor.create_region(&handle, ());
        surface.set_input_region(Some(&region));
        region.destroy();
        let subsurface = subcompositor.get_subsurface(&surface, &parent, &handle, ());
        subsurface.set_desync();
        subsurface.place_above(&parent);
        parent.commit();
        connection.flush()?;
        Ok(Self {
            window,
            display,
            connection,
            queue,
            surface,
            subsurface,
            parent,
        })
    }
    fn layout(&mut self, bounds: Bounds, scale: f64) -> Result<(), AppCreationError> {
        self.subsurface.set_position(
            (bounds.x as f64 / scale).round() as i32,
            (bounds.y as f64 / scale).round() as i32,
        );
        self.surface.set_buffer_scale(scale.round().max(1.0) as i32);
        self.parent.commit();
        self.queue.dispatch_pending(&mut WaylandState)?;
        self.connection.flush()?;
        Ok(())
    }
}
impl Drop for WaylandChild {
    fn drop(&mut self) {
        self.subsurface.destroy();
        self.surface.destroy();
        let _ = self.connection.flush();
    }
}
