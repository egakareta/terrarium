use std::ptr::NonNull;

use wgpu::rwh::{
    HasWindowHandle, RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle,
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    Graphics::Gdi::{CombineRgn, CreateRectRgn, DeleteObject, RGN_OR, SetWindowRgn, ValidateRect},
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, HTTRANSPARENT, HWND_TOP, RegisterClassW,
        SWP_NOACTIVATE, SetWindowPos, WM_ERASEBKGND, WM_NCHITTEST, WM_PAINT, WNDCLASSW, WS_CHILD,
        WS_CLIPSIBLINGS, WS_EX_NOACTIVATE, WS_EX_TRANSPARENT, WS_VISIBLE,
    },
};

use super::{Bounds, Rect};
use crate::AppCreationError;

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_NCHITTEST => HTTRANSPARENT as LRESULT,
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            unsafe {
                ValidateRect(window, std::ptr::null());
            }
            0
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

pub(super) struct Child {
    window: tauri::WebviewWindow,
    child: HWND,
    alive: bool,
    last: Option<(Bounds, Vec<Rect>)>,
}
impl Child {
    pub fn requires_alpha(&self) -> bool {
        false
    }
    pub fn new(window: tauri::WebviewWindow) -> Result<Self, AppCreationError> {
        let RawWindowHandle::Win32(parent) = window.window_handle()?.as_raw() else {
            return Err("Win32 parent window is unavailable".into());
        };
        let class: Vec<u16> = "TerrariumGpuSurface\0".encode_utf16().collect();
        let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
        let descriptor = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..Default::default()
        };
        unsafe {
            RegisterClassW(&descriptor);
        }
        let child = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
                class.as_ptr(),
                std::ptr::null(),
                WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS,
                0,
                0,
                1,
                1,
                parent.hwnd.get() as HWND,
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            )
        };
        if child.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(Self {
            window,
            child,
            alive: true,
            last: None,
        })
    }
    pub fn instance_descriptor(
        &self,
        descriptor: wgpu::InstanceDescriptor,
    ) -> wgpu::InstanceDescriptor {
        descriptor.with_display_handle(Box::new(self.window.clone()))
    }
    pub fn handles(&self) -> Result<(RawDisplayHandle, RawWindowHandle), AppCreationError> {
        let child = NonNull::new(self.child).ok_or("Win32 child window is unavailable")?;
        Ok((
            RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
            RawWindowHandle::Win32(Win32WindowHandle::new(
                std::num::NonZeroIsize::new(child.as_ptr() as isize)
                    .ok_or("invalid child handle")?,
            )),
        ))
    }
    pub fn layout(
        &mut self,
        bounds: Bounds,
        _scale: f64,
        regions: &[Rect],
    ) -> Result<(), AppCreationError> {
        if self
            .last
            .as_ref()
            .is_some_and(|(old, mask)| *old == bounds && mask == regions)
        {
            return Ok(());
        }
        unsafe {
            if SetWindowPos(
                self.child,
                HWND_TOP,
                bounds.x,
                bounds.y,
                bounds.width as i32,
                bounds.height as i32,
                SWP_NOACTIVATE,
            ) == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            let mask = CreateRectRgn(0, 0, 0, 0);
            for rect in regions {
                let region = CreateRectRgn(
                    rect.x as i32,
                    rect.y as i32,
                    (rect.x + rect.width) as i32,
                    (rect.y + rect.height) as i32,
                );
                CombineRgn(mask, mask, region, RGN_OR);
                DeleteObject(region);
            }
            if SetWindowRgn(self.child, mask, 1) == 0 {
                DeleteObject(mask);
                return Err(std::io::Error::last_os_error().into());
            }
        }
        self.last = Some((bounds, regions.to_vec()));
        Ok(())
    }
    pub fn parent_destroyed(&mut self) {
        self.alive = false;
    }
}
impl Drop for Child {
    fn drop(&mut self) {
        if self.alive {
            unsafe {
                DestroyWindow(self.child);
            }
        }
    }
}
