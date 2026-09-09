//! Observational native handles. Copying bytes does not extend the GPUI borrow.

use napi::bindgen_prelude::Buffer;
use napi_derive::napi;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};

#[napi(string_enum)]
pub enum NativeWindowHandleKind {
    AppKit,
    Win32,
    Xlib,
    Xcb,
    Wayland,
}

/// Borrowed native identifiers, encoded in native byte order. No ownership or
/// lifetime is transferred. See README Native integration snapshots before FFI use.
#[napi(object)]
pub struct NativeWindowHandle {
    pub kind: NativeWindowHandleKind,
    pub handle: Buffer,
    pub display: Option<Buffer>,
    pub screen: Option<i32>,
}

pub(crate) fn native_window_handle(window: &gpui::Window) -> Option<NativeWindowHandle> {
    // Window also has an inherent window_handle() returning a GPUI entity id.
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let display = HasDisplayHandle::display_handle(window).ok()?;
    encode_handles(handle.as_raw(), display.as_raw())
}

fn encode_handles(
    handle: RawWindowHandle,
    display: RawDisplayHandle,
) -> Option<NativeWindowHandle> {
    let (kind, handle, display, screen) = match (handle, display) {
        (RawWindowHandle::AppKit(handle), RawDisplayHandle::AppKit(_)) => (
            NativeWindowHandleKind::AppKit,
            (handle.ns_view.as_ptr() as usize).to_ne_bytes().to_vec(),
            None,
            None,
        ),
        (RawWindowHandle::Win32(handle), RawDisplayHandle::Windows(_)) => (
            NativeWindowHandleKind::Win32,
            handle.hwnd.get().to_ne_bytes().to_vec(),
            None,
            None,
        ),
        (RawWindowHandle::Xlib(handle), RawDisplayHandle::Xlib(display)) => (
            NativeWindowHandleKind::Xlib,
            handle.window.to_ne_bytes().to_vec(),
            Some((display.display?.as_ptr() as usize).to_ne_bytes().to_vec()),
            Some(display.screen),
        ),
        (RawWindowHandle::Xcb(handle), RawDisplayHandle::Xcb(display)) => (
            NativeWindowHandleKind::Xcb,
            handle.window.get().to_ne_bytes().to_vec(),
            Some(
                (display.connection?.as_ptr() as usize)
                    .to_ne_bytes()
                    .to_vec(),
            ),
            Some(display.screen),
        ),
        (RawWindowHandle::Wayland(handle), RawDisplayHandle::Wayland(display)) => (
            NativeWindowHandleKind::Wayland,
            (handle.surface.as_ptr() as usize).to_ne_bytes().to_vec(),
            Some((display.display.as_ptr() as usize).to_ne_bytes().to_vec()),
            None,
        ),
        _ => return None,
    };
    // Buffer::from(Vec) has no Env or napi_ref. Only the return conversion on
    // the JS thread creates JS buffers; these UI-thread results stay Rust-owned.
    Some(NativeWindowHandle {
        kind,
        handle: handle.into(),
        display: display.map(Into::into),
        screen,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::{NonZeroIsize, NonZeroU32};
    use std::ptr::NonNull;

    #[test]
    fn encodes_pointer_bits_without_js_number_rounding() {
        let bits = isize::MAX;
        let handle = raw_window_handle::Win32WindowHandle::new(NonZeroIsize::new(bits).unwrap());
        let result = encode_handles(
            handle.into(),
            RawDisplayHandle::Windows(raw_window_handle::WindowsDisplayHandle::new()),
        )
        .unwrap();
        assert!(matches!(result.kind, NativeWindowHandleKind::Win32));
        assert_eq!(&*result.handle, &bits.to_ne_bytes());
        assert!(result.display.is_none());
        assert!(result.screen.is_none());
    }

    #[test]
    fn xcb_requires_connection_and_preserves_id_width() {
        let handle = raw_window_handle::XcbWindowHandle::new(NonZeroU32::new(u32::MAX).unwrap());
        let missing = raw_window_handle::XcbDisplayHandle::new(None, 3);
        assert!(encode_handles(handle.into(), missing.into()).is_none());
        let connection = NonNull::dangling();
        let display = raw_window_handle::XcbDisplayHandle::new(Some(connection), 3);
        let result = encode_handles(handle.into(), display.into()).unwrap();
        assert!(matches!(result.kind, NativeWindowHandleKind::Xcb));
        assert_eq!(&*result.handle, &u32::MAX.to_ne_bytes());
        assert_eq!(
            &*result.display.unwrap(),
            &(connection.as_ptr() as usize).to_ne_bytes()
        );
        assert_eq!(result.screen, Some(3));
    }

    #[test]
    fn appkit_preserves_nsview_pointer_and_tag() {
        let pointer = NonNull::<std::ffi::c_void>::dangling();
        let handle = raw_window_handle::AppKitWindowHandle::new(pointer);
        let display = raw_window_handle::AppKitDisplayHandle::new();
        let result = encode_handles(handle.into(), display.into()).unwrap();
        assert!(matches!(result.kind, NativeWindowHandleKind::AppKit));
        assert_eq!(&*result.handle, &(pointer.as_ptr() as usize).to_ne_bytes());
        assert!(result.display.is_none());
        assert!(result.screen.is_none());
    }

    #[test]
    fn xlib_preserves_unsigned_long_and_requires_display() {
        let mut handle = raw_window_handle::XlibWindowHandle::new(std::ffi::c_ulong::MAX);
        handle.visual_id = 0;
        assert!(encode_handles(
            handle.into(),
            raw_window_handle::XlibDisplayHandle::new(None, -1).into()
        )
        .is_none());
        let pointer = NonNull::<std::ffi::c_void>::dangling();
        let display = raw_window_handle::XlibDisplayHandle::new(Some(pointer), -1);
        let result = encode_handles(handle.into(), display.into()).unwrap();
        assert!(matches!(result.kind, NativeWindowHandleKind::Xlib));
        assert_eq!(&*result.handle, &std::ffi::c_ulong::MAX.to_ne_bytes());
        assert_eq!(
            &*result.display.unwrap(),
            &(pointer.as_ptr() as usize).to_ne_bytes()
        );
        assert_eq!(result.screen, Some(-1));
    }

    #[test]
    fn wayland_preserves_surface_and_display() {
        let pointer = NonNull::<std::ffi::c_void>::dangling();
        let handle = raw_window_handle::WaylandWindowHandle::new(pointer);
        let display = raw_window_handle::WaylandDisplayHandle::new(pointer);
        let result = encode_handles(handle.into(), display.into()).unwrap();
        assert!(matches!(result.kind, NativeWindowHandleKind::Wayland));
        assert_eq!(&*result.handle, &(pointer.as_ptr() as usize).to_ne_bytes());
        assert_eq!(
            &*result.display.unwrap(),
            &(pointer.as_ptr() as usize).to_ne_bytes()
        );
        assert!(result.screen.is_none());
    }

    #[test]
    fn mismatched_backends_are_unavailable() {
        let handle = raw_window_handle::AppKitWindowHandle::new(NonNull::dangling());
        let display = raw_window_handle::WindowsDisplayHandle::new();
        assert!(encode_handles(handle.into(), display.into()).is_none());
    }
}
