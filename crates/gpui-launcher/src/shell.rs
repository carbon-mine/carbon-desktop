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
            && (380.0..=32768.0).contains(&self.width)
            && (420.0..=32768.0).contains(&self.height)
            && screens.iter().any(|s| {
                // Leave a usable section of the caption and the upper content on one live display.
                let left = self.x.max(f32::from(s.origin.x));
                let right = (self.x + self.width).min(f32::from(s.right()));
                right - left >= 96.
                    && self.y >= f32::from(s.origin.y) - 4.
                    && self.y + 48. <= f32::from(s.bottom())
            })
    }
}

pub fn initial_bounds(root: &Path, cx: &App) -> WindowBounds {
    let screens: Vec<_> = cx.displays().into_iter().map(|s| s.bounds()).collect();
    if let Some(pos) =
        read::<Placement>(root, "shell-placement.json").filter(|p| p.usable(&screens))
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
        WindowBounds::Windowed(Bounds::centered(None, size(px(1040.), px(760.)), cx))
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
        if pos.usable(&cx.displays().iter().map(|s| s.bounds()).collect::<Vec<_>>())
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
