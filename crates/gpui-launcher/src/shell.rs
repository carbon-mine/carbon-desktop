// SPDX-License-Identifier: MPL-2.0
#[cfg(any(windows, target_os = "linux"))]
use super::Picker;
use super::{Launcher, Preference, popup};
use gpui::{App, Bounds, Context, Window, WindowBounds, actions, point, px, size};
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::Path};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;
mod work_area;
#[cfg(windows)]
pub use windows::{NativeMenu, apply_frame, configure_renderer};
#[cfg(not(windows))]
pub fn apply_frame(_: &Window, _: bool) {}

actions!(shell, [CloseLauncher, OpenFile, OpenView]);

#[derive(Clone, Copy)]
pub enum Page {
    File,
    View,
}
#[cfg(any(windows, target_os = "linux"))]
#[derive(Clone, Copy)]
pub enum Command {
    New,
    Import,
    Close,
    Find,
    Activity,
    Appearance(Preference),
}

pub fn bind_menu_keys(cx: &mut App) {
    #[cfg(target_os = "linux")]
    linux::bind_keys(cx);
    #[cfg(not(target_os = "linux"))]
    let _ = cx;
}

#[derive(Serialize, Deserialize)]
struct Placement {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    maximized: bool,
}

pub(super) fn read<T: serde::de::DeserializeOwned>(root: &Path, file: &str) -> Option<T> {
    let path = root.join(file);
    let info = fs::symlink_metadata(&path).ok()?;
    if !info.is_file() || info.len() > 4096 {
        return None;
    }
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

pub(super) fn write(root: &Path, file: &str, value: &impl Serialize) -> std::io::Result<()> {
    fs::create_dir_all(root)?;
    let mut staged = tempfile::NamedTempFile::new_in(root)?;
    serde_json::to_writer(&mut staged, value)?;
    staged.flush()?;
    staged.as_file().sync_all()?;
    staged.persist(root.join(file))?;
    Ok(())
}

pub fn preference(root: &Path) -> Preference {
    read(root, "shell-appearance.json").unwrap_or_default()
}

impl Placement {
    fn usable(&self, screens: &[Bounds<gpui::Pixels>]) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && (1.0..=32768.0).contains(&self.width)
            && (1.0..=32768.0).contains(&self.height)
            && screens.iter().any(|s| {
                // Leave a usable section of the caption and the upper content on one live display.
                let left = self.x.max(f32::from(s.origin.x));
                let right = (self.x + self.width).min(f32::from(s.right()));
                let fit = default_window(*s).size;
                self.width >= f32::from(fit.width).min(380.)
                    && self.height >= f32::from(fit.height).min(420.)
                    && right - left >= self.width.min(96.)
                    && self.y >= f32::from(s.origin.y) - 4.
                    && self.y + self.height.min(48.) <= f32::from(s.bottom())
            })
    }
}

/// Owner-selected width measured in the reference launcher's client at 96 DPI.
const DEFAULT_CLIENT_WIDTH: f32 = 1368.0;
/// GPUI's Win32 layout includes the native menu; 808 yields a 788px client at 96 DPI.
const DEFAULT_LAYOUT_HEIGHT: f32 = 808.0;
/// Total logical space, across both sides, that ordinary native captions and
/// resize borders add around the client area. Reserve it before fitting a
/// window into a small work area.
const NATIVE_FRAME_WIDTH: f32 = 16.0;
const NATIVE_FRAME_HEIGHT: f32 = 40.0;

/// Convert a physical work-area rectangle into GPUI logical pixels.
///
/// GPUI exposes logical display and window bounds, while Win32 monitor
/// rectangles are physical. Dividing each edge by one monitor scale factor
/// keeps taskbar offsets and negative origins aligned with GPUI coordinates.
#[cfg(any(windows, test))]
fn logical_work_area(
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    scale_factor: f32,
) -> Option<Bounds<gpui::Pixels>> {
    if !scale_factor.is_finite() || scale_factor <= 0.0 || right <= left || bottom <= top {
        return None;
    }
    Some(Bounds::new(
        point(
            px(left as f32 / scale_factor),
            px(top as f32 / scale_factor),
        ),
        size(
            px((i64::from(right) - i64::from(left)) as f32 / scale_factor),
            px((i64::from(bottom) - i64::from(top)) as f32 / scale_factor),
        ),
    ))
}

/// GPUI passes the rectangle to Win32 WINDOWPLACEMENT. Its origin is in
/// workspace coordinates; Windows applies a top or left taskbar offset itself.
#[cfg(any(windows, test))]
fn windows_placement_area(
    monitor_origin: (i32, i32),
    work: [i32; 4],
    scale: f32,
) -> Option<Bounds<gpui::Pixels>> {
    let [left, top, right, bottom] = work;
    let mut bounds = logical_work_area(left, top, right, bottom, scale)?;
    bounds.origin = point(
        px(monitor_origin.0 as f32 / scale),
        px(monitor_origin.1 as f32 / scale),
    );
    Some(bounds)
}

/// Center the preferred client size in the usable area, reserving normal native
/// decorations and shrinking only when the area cannot hold the preference.
fn default_window(work_area: Bounds<gpui::Pixels>) -> Bounds<gpui::Pixels> {
    let available_width = (f32::from(work_area.size.width) - NATIVE_FRAME_WIDTH).max(1.0);
    let available_height = (f32::from(work_area.size.height) - NATIVE_FRAME_HEIGHT).max(1.0);
    let width = DEFAULT_CLIENT_WIDTH.min(available_width);
    let height = DEFAULT_LAYOUT_HEIGHT.min(available_height);
    Bounds::new(
        point(
            px(f32::from(work_area.origin.x) + (f32::from(work_area.size.width) - width) / 2.0),
            px(f32::from(work_area.origin.y) + (f32::from(work_area.size.height) - height) / 2.0),
        ),
        size(px(width), px(height)),
    )
}

fn usable_placement(pos: &Placement, cx: &App) -> bool {
    let screens: Vec<_> = cx.displays().into_iter().map(|s| s.bounds()).collect();
    pos.usable(&screens) || work_area::primary(cx).is_some_and(|work| pos.usable(&[work]))
}

pub fn initial_bounds(root: &Path, cx: &App) -> WindowBounds {
    if let Some(pos) =
        read::<Placement>(root, "shell-placement.json").filter(|p| usable_placement(p, cx))
    {
        let b = Bounds::new(
            point(px(pos.x), px(pos.y)),
            size(px(pos.width), px(pos.height)),
        );
        if pos.maximized {
            WindowBounds::Maximized(b)
        } else {
            WindowBounds::Windowed(b)
        }
    } else {
        // The native work area excludes reserved desktop space where reported.
        // Compositor-managed sessions can supply only the display bounds.
        WindowBounds::Windowed(
            work_area::primary(cx)
                .or_else(|| cx.primary_display().map(|display| display.bounds()))
                .map(default_window)
                .unwrap_or_else(|| {
                    Bounds::new(
                        point(px(0.), px(0.)),
                        size(px(DEFAULT_CLIENT_WIDTH), px(DEFAULT_LAYOUT_HEIGHT)),
                    )
                }),
        )
    }
}

impl Launcher {
    pub(super) fn allow_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.picker_open {
            return false;
        }
        if let Some(popup) = self.popup {
            let _ = popup.update(cx, |_, child, _| child.activate_window());
            return false;
        }
        if self.task.as_ref().is_some_and(|t| t.protects_session) {
            let launcher = cx.weak_entity();
            window.defer(cx, move |window, cx| {
                let _ = launcher.update(cx, |launcher, cx| {
                    if launcher.popup.is_none()
                        && launcher.task.as_ref().is_some_and(|task| task.protects_session)
                    {
                        popup::open(
                            launcher,
                            popup::Kind::Notice {
                                title: "Protected test window is open",
                                message: "Use Save and close in the protected Java test window before closing Carbon Desktop.",
                            },
                            window,
                            cx,
                        );
                    }
                });
            });
            return false;
        }
        let status = window.window_bounds();
        let b = status.get_bounds();
        let pos = Placement {
            x: b.origin.x.into(),
            y: b.origin.y.into(),
            width: b.size.width.into(),
            height: b.size.height.into(),
            maximized: matches!(status, WindowBounds::Maximized(_)),
        };
        if usable_placement(&pos, cx)
            && let Err(error) = write(&self.root.path, "shell-placement.json", &pos)
        {
            eprintln!("Could not save window placement: {error}");
        }
        true
    }

    pub(super) fn close_from_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.popup.is_none() && !self.picker_open && self.allow_close(window, cx) {
            window.remove_window();
        }
    }

    #[cfg(any(windows, target_os = "linux"))]
    pub(super) fn shell_command(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.popup.is_some() || self.picker_open {
            return;
        }
        match command {
            Command::New => self.editor(popup::Mode::Create, window, cx),
            Command::Import => self.pick(Picker::Pack, window, cx),
            Command::Close => self.close_from_menu(window, cx),
            Command::Find => self.find(window, cx),
            Command::Activity => self.activity_focus.focus(window),
            Command::Appearance(preference) => {
                self.preference = preference;
                self.applied_dark = None;
                if let Err(error) = write(&self.root.path, "shell-appearance.json", &preference) {
                    self.message = format!("Could not save appearance: {error}");
                }
                #[cfg(windows)]
                if let Some(menu) = &self.native_menu {
                    menu.select(preference);
                }
                cx.notify();
            }
        }
    }

    pub(super) fn open_shell_menu(
        &mut self,
        page: Page,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(target_os = "linux")]
        linux::open(self, page, window, cx);
        #[cfg(not(target_os = "linux"))]
        let _ = (page, window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(x: f32, y: f32, width: f32, height: f32) -> Bounds<gpui::Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
    }

    #[test]
    fn windows_does_not_apply_a_left_or_top_taskbar_twice() {
        assert_eq!(
            windows_placement_area((0, 0), [50, 75, 1600, 900], 1.25),
            Some(area(0., 0., 1240., 660.))
        );
        assert_eq!(
            windows_placement_area((-1600, 0), [-1550, 75, 0, 900], 1.25),
            Some(area(-1280., 0., 1240., 660.))
        );
    }

    #[test]
    fn small_monitor_can_restore_its_fitted_size() {
        let screen = area(0., 0., 320., 400.);
        let b = default_window(screen);
        let placement = Placement {
            x: b.origin.x.into(),
            y: b.origin.y.into(),
            width: b.size.width.into(),
            height: b.size.height.into(),
            maximized: true,
        };
        assert!(placement.usable(&[screen]));
        assert!(!placement.usable(&[area(0., 0., 1920., 1080.)]));
    }

    #[test]
    fn first_run_uses_measured_width_centered_on_the_work_area() {
        let bounds = default_window(area(0., 0., 1920., 1040.));
        assert_eq!(bounds.size, size(px(1368.), px(808.)));
        assert_eq!(bounds.origin, point(px(276.), px(116.)));

        // The preferred height survives a work area that can still hold it.
        let nearly_short = default_window(area(0., 0., 1920., 848.));
        assert_eq!(nearly_short.size.height, px(808.));
        let short = default_window(area(0., 0., 1920., 780.));
        assert_eq!(short.size.height, px(740.));
    }

    #[test]
    fn first_run_centers_on_a_negative_origin_work_area() {
        let bounds = default_window(area(-1920., 0., 1920., 1040.));
        assert_eq!(bounds.size, size(px(1368.), px(808.)));
        assert_eq!(bounds.origin, point(px(-1644.), px(116.)));
    }

    #[test]
    fn physical_work_area_converts_125_percent_without_mixing_units() {
        // A 2400x1080 physical 125% monitor left of the primary display, with a
        // 40px taskbar: 1920x832 logical work area starting at -1920.
        let work_area = logical_work_area(-2400, 0, 0, 1040, 1.25).unwrap();
        assert_eq!(work_area, area(-1920., 0., 1920., 832.));

        let bounds = default_window(work_area);
        assert_eq!(bounds.size, size(px(1368.), px(792.)));
        assert_eq!(bounds.origin, point(px(-1644.), px(20.)));

        assert_eq!(logical_work_area(0, 0, 0, 1080, 1.25), None);
        assert_eq!(logical_work_area(0, 0, 1920, 1080, 0.), None);
    }

    #[test]
    fn tiny_work_area_reserves_normal_native_decorations() {
        let work_area = area(100., 50., 800., 560.);
        let bounds = default_window(work_area);
        assert_eq!(bounds.size, size(px(784.), px(520.)));
        assert_eq!(bounds.origin, point(px(108.), px(70.)));

        // Client bounds plus the reserved caption/border fit the work area.
        assert_eq!(f32::from(bounds.origin.x) - NATIVE_FRAME_WIDTH / 2.0, 100.);
        assert_eq!(f32::from(bounds.origin.y) - NATIVE_FRAME_HEIGHT / 2.0, 50.);
        assert_eq!(
            f32::from(bounds.origin.x) + f32::from(bounds.size.width) + NATIVE_FRAME_WIDTH / 2.0,
            900.
        );
        assert_eq!(
            f32::from(bounds.origin.y) + f32::from(bounds.size.height) + NATIVE_FRAME_HEIGHT / 2.0,
            610.
        );
    }

    #[test]
    fn rejects_unrecoverable_placement_and_keeps_negative_coordinate_display() {
        let screen = Bounds::new(point(px(-1920.), px(0.)), size(px(1920.), px(1080.)));
        let mut p = Placement {
            x: -1500.,
            y: 25.,
            width: 1040.,
            height: 760.,
            maximized: false,
        };
        assert!(p.usable(&[screen]));
        p.y = -1000.;
        assert!(!p.usable(&[screen]));
        p.y = 0.;
        p.width = f32::NAN;
        assert!(!p.usable(&[screen]));
    }
    #[test]
    fn appearance_roundtrips_and_invalid_json_falls_back_to_system() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "shell-appearance.json", &Preference::Dark).unwrap();
        assert_eq!(preference(root.path()), Preference::Dark);
        fs::write(root.path().join("shell-appearance.json"), b"unknown").unwrap();
        assert_eq!(preference(root.path()), Preference::System);
    }
}
