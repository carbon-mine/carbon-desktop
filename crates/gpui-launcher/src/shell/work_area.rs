// SPDX-License-Identifier: MPL-2.0
//! Authoritative monitor work areas in GPUI logical pixels.
//!
//! GPUI 0.2 reports a display's full rectangle. Windows keeps the usable area
//! separately in `MONITORINFO::rcWork`, macOS exposes `NSScreen::visibleFrame`,
//! and X11 publishes `_NET_WORKAREA`. Query those here so the launcher geometry
//! in `shell` stays platform independent.

pub(super) fn primary(cx: &gpui::App) -> Option<gpui::Bounds<gpui::Pixels>> {
    for_display(cx, cx.primary_display()?.id(), None)
}

type Position = Option<gpui::Point<gpui::Pixels>>;

#[cfg(windows)]
pub(super) fn for_display(
    _cx: &gpui::App,
    id: gpui::DisplayId,
    _: Position,
) -> Option<gpui::Bounds<gpui::Pixels>> {
    use windows::Win32::{
        Foundation::{LPARAM, RECT},
        Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO},
        UI::{
            HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
            WindowsAndMessaging::USER_DEFAULT_SCREEN_DPI,
        },
    };

    // GPUI 0.2.2 assigns Win32 DisplayId from this enumeration index.
    // The chosen monitor supplies both its work area and effective DPI.
    unsafe extern "system" fn collect(
        monitor: HMONITOR,
        _: HDC,
        _: *mut RECT,
        data: LPARAM,
    ) -> windows::core::BOOL {
        unsafe { (*(data.0 as *mut Vec<HMONITOR>)).push(monitor) };
        true.into()
    }
    let mut monitors = Vec::<HMONITOR>::new();
    unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(std::ptr::from_mut(&mut monitors) as isize),
        )
        .ok()
        .ok()?;
    }
    let monitor = *monitors.get(u32::from(id) as usize)?;
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, std::ptr::from_mut(&mut info)) }.as_bool() {
        return None;
    }
    let (mut dpi_x, mut dpi_y) = (0, 0);
    unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) }.ok()?;
    if dpi_x == 0 || dpi_x != dpi_y {
        return None;
    }
    let scale = dpi_x as f32 / USER_DEFAULT_SCREEN_DPI as f32;
    super::windows_placement_area(
        (info.rcMonitor.left, info.rcMonitor.top),
        [
            info.rcWork.left,
            info.rcWork.top,
            info.rcWork.right,
            info.rcWork.bottom,
        ],
        scale,
    )
}

#[cfg(target_os = "macos")]
pub(super) fn for_display(
    cx: &gpui::App,
    id: gpui::DisplayId,
    _: Position,
) -> Option<gpui::Bounds<gpui::Pixels>> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSScreen;

    // NSScreen is main-thread-only. AppKit uses a bottom-left origin while
    // GPUI display coordinates start at the top-left, so flip the Y axis.
    let mtm = MainThreadMarker::new()?;
    // NSScreenNumber is the public CGDirectDisplayID used by GPUI. Screens
    // on macOS have overlapping local origins, so geometry cannot identify one.
    let key = objc2_foundation::NSString::from_str("NSScreenNumber");
    let screens = NSScreen::screens(mtm);
    let screen = (0..screens.count())
        .map(|i| screens.objectAtIndex(i))
        .find(|s| {
            s.deviceDescription().objectForKey(&key).and_then(|number| {
                number
                    .downcast_ref::<objc2_foundation::NSNumber>()
                    .map(|n| n.unsignedIntValue())
            }) == Some(u32::from(id))
        })?;
    let frame = screen.frame();
    let visible = screen.visibleFrame();
    let display = cx.displays().into_iter().find(|d| d.id() == id)?.bounds();

    let left = visible.origin.x - frame.origin.x;
    let top = frame.size.height - (visible.origin.y - frame.origin.y) - visible.size.height;
    screen_relative_area(display, left, top, visible.size.width, visible.size.height)
}

/// Clips an AppKit work area measured inside its screen to the GPUI display.
///
/// gpui 0.2.2 reports every macOS display with origin `(0, 0)` and applies
/// window bounds relative to the target screen
/// (`platform/mac/window.rs`: `screen_frame.origin.x + bounds.origin.x`).
/// `NSScreen` `frame`/`visibleFrame` use the same screen-relative convention,
/// so the offsets must stay screen-relative: adding the display origin here
/// would move a secondary screen's window off that screen.
#[cfg(any(target_os = "macos", test))]
fn screen_relative_area(
    display: gpui::Bounds<gpui::Pixels>,
    left: f64,
    top: f64,
    width: f64,
    height: f64,
) -> Option<gpui::Bounds<gpui::Pixels>> {
    use gpui::{Bounds, point, px, size};

    if !(left.is_finite() && top.is_finite() && width.is_finite() && height.is_finite())
        || width <= 0.0
        || height <= 0.0
    {
        return None;
    }

    let work_area = Bounds::new(
        point(px(left as f32), px(top as f32)),
        size(px(width as f32), px(height as f32)),
    );
    intersect(work_area, display)
}

#[cfg(target_os = "linux")]
pub(super) fn for_display(
    cx: &gpui::App,
    id: gpui::DisplayId,
    origin: Position,
) -> Option<gpui::Bounds<gpui::Pixels>> {
    use gpui::{Bounds, point, px, size};
    use x11rb::connection::Connection;
    use x11rb::protocol::randr::ConnectionExt;

    // On Wayland the X root may belong to XWayland and describe a layout that
    // does not match the compositor's logical coordinates. Fall back instead.
    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE").is_ok_and(|kind| kind.eq_ignore_ascii_case("wayland"))
    {
        return None;
    }

    let display = cx.displays().into_iter().find(|d| d.id() == id)?.bounds();
    let display_width = f32::from(display.size.width);
    let display_height = f32::from(display.size.height);
    if display_width <= 0.0 || display_height <= 0.0 {
        return None;
    }

    let (connection, screen_num) = x11rb::connect(None).ok()?;
    let screen = connection.setup().roots.get(screen_num)?;
    let screen_width = screen.width_in_pixels;
    let screen_height = screen.height_in_pixels;

    // GPUI 0.2's X11 display is the whole root window divided by one scale
    // factor. Recover that factor from the same rectangle instead of guessing
    // from Xft.dpi, RandR or the environment.
    let scale = screen_width as f32 / display_width;
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    if ((screen_height as f32 / display_height) - scale).abs() > 0.01 {
        return None;
    }

    // A single X root can span several outputs. Prefer the primary RandR
    // monitor, or the first active monitor if the WM has not named a primary.
    let root = screen.root;
    let monitors = connection
        .randr_get_monitors(root, true)
        .ok()
        .and_then(|r| r.reply().ok());
    let monitor = monitors
        .as_ref()
        .and_then(|r| {
            r.monitors
                .iter()
                .filter(|m| m.width > 0 && m.height > 0)
                .max_by_key(|m| {
                    let inside = origin.is_some_and(|p| {
                        let x = f32::from(p.x) * scale;
                        let y = f32::from(p.y) * scale;
                        x >= f32::from(m.x)
                            && y >= f32::from(m.y)
                            && x < f32::from(m.x) + f32::from(m.width)
                            && y < f32::from(m.y) + f32::from(m.height)
                    });
                    (inside, m.primary, m.x == 0 && m.y == 0)
                })
        })
        .and_then(|m| {
            intersect(
                Bounds::new(
                    point(px(f32::from(m.x) / scale), px(f32::from(m.y) / scale)),
                    size(
                        px(f32::from(m.width) / scale),
                        px(f32::from(m.height) / scale),
                    ),
                ),
                display,
            )
        })
        .unwrap_or(display);
    let work_area = x11_desktop_area(&connection, root, scale);
    Some(
        work_area
            .and_then(|area| intersect(area, monitor))
            .unwrap_or(monitor),
    )
}

#[cfg(target_os = "linux")]
fn x11_desktop_area(
    connection: &x11rb::rust_connection::RustConnection,
    root: u32,
    scale: f32,
) -> Option<gpui::Bounds<gpui::Pixels>> {
    use gpui::{Bounds, point, px, size};
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};

    let cardinals = |name: &[u8], offset: u32, len: u32| -> Option<Vec<u32>> {
        let atom = connection.intern_atom(true, name).ok()?.reply().ok()?.atom;
        if atom == 0 {
            return None;
        }
        let prop = connection
            .get_property(false, root, atom, AtomEnum::CARDINAL, offset, len)
            .ok()?
            .reply()
            .ok()?;
        if prop.type_ != u32::from(AtomEnum::CARDINAL) {
            return None;
        }
        Some(prop.value32()?.collect())
    };
    let desktop = *cardinals(b"_NET_CURRENT_DESKTOP", 0, 1)?.first()?;
    let rect: [u32; 4] = cardinals(b"_NET_WORKAREA", desktop.checked_mul(4)?, 4)?
        .try_into()
        .ok()?;
    Some(Bounds::new(
        point(px(rect[0] as f32 / scale), px(rect[1] as f32 / scale)),
        size(px(rect[2] as f32 / scale), px(rect[3] as f32 / scale)),
    ))
}

#[cfg(any(target_os = "macos", target_os = "linux", test))]
fn intersect(
    a: gpui::Bounds<gpui::Pixels>,
    b: gpui::Bounds<gpui::Pixels>,
) -> Option<gpui::Bounds<gpui::Pixels>> {
    a.intersects(&b).then(|| a.intersect(&b))
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub(super) fn for_display(
    _cx: &gpui::App,
    _: gpui::DisplayId,
    _: Position,
) -> Option<gpui::Bounds<gpui::Pixels>> {
    // GPUI exposes no work area on other platforms. The caller falls back to
    // the primary display rectangle with a conservative frame inset.
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Bounds, point, px, size};

    #[test]
    fn a_bottom_dock_inset_stays_screen_relative() {
        // gpui 0.2.2 gives every macOS display a zero origin, and windows are
        // placed relative to their screen, so the inset keeps that origin.
        let display = Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.)));
        let area = screen_relative_area(display, 0., 0., 1920., 1020.).unwrap();
        assert_eq!(area.origin, point(px(0.), px(0.)));
        assert_eq!(area.size, size(px(1920.), px(1020.)));
    }

    #[test]
    fn a_left_dock_inset_stays_on_its_own_screen() {
        let display = Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.)));
        let area = screen_relative_area(display, 70., 0., 1850., 1080.).unwrap();
        assert_eq!(area.origin, point(px(70.), px(0.)));
        assert_eq!(area.size, size(px(1850.), px(1080.)));
    }

    #[test]
    fn a_menu_bar_inset_survives_the_y_flip() {
        // AppKit's visibleFrame starts below the menu bar, so the top inset
        // must move down from the screen's top edge.
        let display = Bounds::new(point(px(0.), px(0.)), size(px(1512.), px(982.)));
        let area = screen_relative_area(display, 0., 25., 1512., 957.).unwrap();
        assert_eq!(area.origin, point(px(0.), px(25.)));
        assert_eq!(area.size, size(px(1512.), px(957.)));
    }

    #[test]
    fn an_invalid_visible_frame_has_no_work_area() {
        let display = Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.)));
        assert!(screen_relative_area(display, 0., 0., 0., 1080.).is_none());
        assert!(screen_relative_area(display, f64::NAN, 0., 1920., 1080.).is_none());
    }
}
