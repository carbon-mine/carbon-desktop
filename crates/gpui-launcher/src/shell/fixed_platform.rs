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
/// setter. It also implements `X11Window::window_handle` with
/// `unimplemented!()`, so the raw-handle path must not be used on X11. Instead
/// find our own toplevel by `_NET_WM_PID` and `WM_CLASS` and write ICCCM
/// `WM_NORMAL_HINTS` with `min == max`.
///
/// Wayland is not covered: `raw-window-handle` exposes only the `wl_surface`,
/// while `xdg_toplevel.set_min_size`/`set_max_size` live on GPUI's private
/// protocol object, which cannot be reached from another connection. GPUI also
/// offers no runtime max-size API. There the window stays resizable at the
/// compositor's discretion; fixing it requires a GPUI-side change (the
/// integrator can pass `window_min_size` at creation, which most compositors
/// honor as the lower bound only).
#[cfg(target_os = "linux")]
pub(super) fn configure(_window: &mut gpui::Window) {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _};

    // On Wayland this would reach XWayland or no display at all, and GPUI's
    // private xdg_toplevel is the real policy surface.
    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE").is_ok_and(|kind| kind.eq_ignore_ascii_case("wayland"))
    {
        return;
    }

    let Ok((connection, screen_num)) = x11rb::connect(None) else {
        return;
    };
    let Some(screen) = connection.setup().roots.get(screen_num) else {
        return;
    };
    let root = screen.root;

    let Ok(pid_atom) = connection.intern_atom(false, b"_NET_WM_PID") else {
        return;
    };
    let Ok(pid_atom) = pid_atom.reply() else {
        return;
    };
    let Ok(tree) = connection.query_tree(root) else {
        return;
    };
    let Ok(tree) = tree.reply() else {
        return;
    };

    let pid = std::process::id();
    for window in tree.children {
        let Ok(property) =
            connection.get_property(false, window, pid_atom.atom, AtomEnum::CARDINAL, 0, 1)
        else {
            continue;
        };
        let Ok(property) = property.reply() else {
            continue;
        };
        if property.value32().and_then(|mut values| values.next()) != Some(pid) {
            continue;
        }
        let Ok(class) =
            connection.get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 1024)
        else {
            continue;
        };
        let Ok(class) = class.reply() else {
            continue;
        };
        if !wm_class_matches(&class.value, "dev.carbonmine.gpui") {
            continue;
        }
        set_fixed_hints(&connection, window);
        break;
    }
}

/// `WM_CLASS` is the NUL-separated instance and class names set from GPUI's
/// `app_id`; accept either segment.
#[cfg(target_os = "linux")]
fn wm_class_matches(value: &[u8], app_id: &str) -> bool {
    value
        .split(|byte| *byte == 0)
        .any(|part| part == app_id.as_bytes())
}

#[cfg(target_os = "linux")]
fn set_fixed_hints(connection: &x11rb::rust_connection::RustConnection, window: u32) {
    use x11rb::connection::Connection;
    use x11rb::properties::WmSizeHints;
    use x11rb::protocol::xproto::ConnectionExt as _;

    // Hints are in device pixels; `bounds()` is in logical pixels.
    let Some(size) = connection
        .get_geometry(window)
        .ok()
        .and_then(|request| request.reply().ok())
    else {
        return;
    };
    let (width, height) = (i32::from(size.width), i32::from(size.height));
    let mut hints = WmSizeHints::get_normal_hints(connection, window)
        .ok()
        .and_then(|request| request.reply().ok())
        .flatten()
        .unwrap_or_default();
    hints.min_size = Some((width, height));
    hints.max_size = Some((width, height));
    if let Ok(cookie) = hints.set_normal_hints(connection, window) {
        let _ = cookie.check();
    }
    let _ = connection.flush();
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::wm_class_matches;

    #[test]
    fn finds_either_wm_class_segment() {
        let app_id = "dev.carbonmine.gpui";
        assert!(wm_class_matches(
            b"dev.carbonmine.gpui\0dev.carbonmine.gpui\0",
            app_id
        ));
        assert!(wm_class_matches(b"other\0dev.carbonmine.gpui\0", app_id));
        assert!(!wm_class_matches(b"other\0another\0", app_id));
        assert!(!wm_class_matches(b"", app_id));
    }
}

/// No native policy is defined for other targets.
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub(super) fn configure(_window: &mut gpui::Window) {}
