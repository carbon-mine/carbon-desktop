// SPDX-License-Identifier: MPL-2.0
//! Native fixed-size policy for the main launcher window.
//!
//! The main window is fixed size: it may be moved, minimized and restored, but
//! not maximized, fullscreened or resized by dragging an edge. GPUI's
//! `WindowOptions::is_resizable` does not cover all of that on every backend,
//! so this module applies the smallest native correction possible to the main
//! window handle. It must only be called for that handle, never for popups or
//! other secondary windows.

/// Pin the main window to its current size.
///
/// Call this once, on the GPUI main thread, for the main launcher window only,
/// after it has been created with `is_resizable: false`.
#[cfg(target_os = "macos")]
pub(super) fn configure(window: &mut gpui::Window) {
    use objc2::MainThreadMarker;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSView, NSWindowButton, NSWindowCollectionBehavior};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    // AppKit windows are main-thread-only. `configure` is called from GPUI's
    // main thread, but verify that instead of trusting the call site.
    let Some(_main_thread) = MainThreadMarker::new() else {
        return;
    };

    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::AppKit(raw) = handle.as_raw() else {
        return;
    };

    // SAFETY: GPUI's raw AppKit handle is the live root NSView for this
    // window. Retaining it gives a typed handle whose lifetime is independent
    // of the borrowed `Window`, and we drop it at the end of this call.
    let Some(view) = (unsafe { Retained::retain(raw.ns_view.as_ptr().cast::<NSView>()) }) else {
        return;
    };
    let Some(native_window) = view.window() else {
        return;
    };

    // `is_resizable: false` only removes NSResizableWindowMask. Without this
    // behavior the green traffic light can still enter native fullscreen.
    let behavior = native_window.collectionBehavior() | NSWindowCollectionBehavior::FullScreenNone;
    native_window.setCollectionBehavior(behavior);

    // Disable the green zoom button itself; fullscreen-none alone leaves the
    // button clickable in some AppKit versions.
    if let Some(zoom) = native_window.standardWindowButton(NSWindowButton::ZoomButton) {
        zoom.setEnabled(false);
    }
}

/// Pin the main window to its current size on X11.
///
/// GPUI 0.2.2 ignores `is_resizable` on Linux and only writes a minimum size
/// hint at creation time (`WindowOptions::window_min_size`), with no runtime
/// setter. On X11 the toplevel is available through the raw window handle, so
/// write ICCCM `WM_NORMAL_HINTS` with `min == max`.
///
/// Wayland is not covered: `raw-window-handle` exposes only the `wl_surface`,
/// while `xdg_toplevel.set_min_size`/`set_max_size` live on GPUI's private
/// protocol object, which cannot be reached from another connection. GPUI also
/// offers no runtime max-size API. There the window stays resizable at the
/// compositor's discretion; fixing it requires a GPUI-side change (the
/// integrator can pass `window_min_size` at creation, which most compositors
/// honor as the lower bound only).
#[cfg(target_os = "linux")]
pub(super) fn configure(window: &mut gpui::Window) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use x11rb::properties::WmSizeHints;
    use x11rb::protocol::xproto::ConnectionExt as _;

    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let window_id = match handle.as_raw() {
        RawWindowHandle::Xcb(raw) => raw.window.get(),
        RawWindowHandle::Xlib(raw) => match u32::try_from(raw.window) {
            Ok(id) => id,
            Err(_) => return,
        },
        // Wayland and headless: see the limitation above.
        _ => return,
    };

    // Hints are in device pixels; `bounds()` is in logical pixels.
    let size = window.bounds().size;
    let scale = window.scale_factor();
    let width = (size.width.0 * scale).round().max(1.0) as i32;
    let height = (size.height.0 * scale).round().max(1.0) as i32;

    // A separate connection may change properties on any window of the same
    // display; WM_NORMAL_HINTS is the ICCCM fixed-size mechanism.
    let Ok((connection, _)) = x11rb::connect(None) else {
        return;
    };
    let mut hints = WmSizeHints::new();
    hints.min_size = Some((width, height));
    hints.max_size = Some((width, height));
    if let Ok(cookie) = hints.set_normal_hints(&connection, window_id) {
        let _ = cookie.check();
    }
    let _ = connection.flush();
}

/// Windows policy is owned by the integrator; intentionally a stub here.
///
/// Fixing the size there needs `WM_GETMINMAXINFO` (clamp `ptMinTrackSize` and
/// `ptMaxTrackSize` to the selected main size) and removal of
/// `WS_THICKFRAME`/`WS_MAXIMIZEBOX`, plus handling `WM_SYSCOMMAND`
/// `SC_MAXIMIZE`. GPUI's `is_resizable: false` already drops the thick frame,
/// so the integrator may not need anything further.
#[cfg(windows)]
pub(super) fn configure(_window: &mut gpui::Window) {}

/// No native policy is defined for other targets.
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub(super) fn configure(_window: &mut gpui::Window) {}
