//! Keep caption controls, system menu, resize handles and activation styling under OS ownership.
use serde::Deserialize;
use tauri::{AppHandle, Manager, Theme};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Preference {
    System,
    Light,
    Dark,
}

#[tauri::command]
pub fn apply_desktop_theme(app: AppHandle, preference: Preference) {
    let selected = match preference {
        Preference::System => 0,
        Preference::Light => 1,
        Preference::Dark => 2,
    };
    crate::dialogs::set_theme_preference(selected);
    if let Some(items) = app.try_state::<crate::menu::AppearanceItems>() {
        for (index, item) in items.0.iter().enumerate() {
            let _ = item.set_checked(selected == index);
        }
    }
    app.set_theme(match preference {
        Preference::System => None,
        Preference::Light => Some(Theme::Light),
        Preference::Dark => Some(Theme::Dark),
    });
}

#[cfg(windows)]
mod placement {
    use serde::{Deserialize, Serialize};
    use std::{
        fs,
        io::{Read, Write},
        mem::size_of,
        path::{Path, PathBuf},
    };
    use tauri::{WebviewWindow, WindowEvent};
    use windows_sys::Win32::{
        Foundation::{HWND, RECT},
        UI::WindowsAndMessaging::{
            GetWindowPlacement, SetWindowPlacement, SW_SHOWMAXIMIZED, SW_SHOWMINIMIZED,
            SW_SHOWNORMAL, WINDOWPLACEMENT, WPF_RESTORETOMAXIMIZED,
        },
    };

    #[derive(Deserialize, Serialize, PartialEq, Debug)]
    struct Placement {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
        maximized: bool,
    }

    impl Placement {
        fn valid(&self) -> bool {
            let width = i64::from(self.right) - i64::from(self.left);
            let height = i64::from(self.bottom) - i64::from(self.top);
            (200..=32768).contains(&width) && (180..=32768).contains(&height)
        }
    }

    fn load(path: &Path) -> Option<Placement> {
        let source = fs::File::open(path).ok()?;
        let mut bytes = Vec::new();
        source.take(1025).read_to_end(&mut bytes).ok()?;
        if bytes.len() > 1024 {
            return None;
        }
        let placement: Placement = serde_json::from_slice(&bytes).ok()?;
        placement.valid().then_some(placement)
    }

    fn save(path: &Path, placement: &Placement) -> std::io::Result<()> {
        let parent = path.parent().ok_or(std::io::ErrorKind::InvalidInput)?;
        fs::create_dir_all(parent)?;
        let mut staged = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer(staged.as_file_mut(), placement)?;
        staged.flush()?;
        staged.persist(path)?;
        Ok(())
    }

    fn native_placement(hwnd: HWND) -> Option<WINDOWPLACEMENT> {
        let mut native = WINDOWPLACEMENT {
            length: size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        // HWND identifies the Tauri-owned live top-level window. The initialized output survives the call.
        (unsafe { GetWindowPlacement(hwnd, &mut native) } != 0).then_some(native)
    }

    fn capture(hwnd: HWND) -> Option<Placement> {
        let native = native_placement(hwnd)?;
        let rect = native.rcNormalPosition;
        let placement = Placement {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
            maximized: native.showCmd == SW_SHOWMAXIMIZED as u32
                || (native.showCmd == SW_SHOWMINIMIZED as u32
                    && native.flags & WPF_RESTORETOMAXIMIZED != 0),
        };
        placement.valid().then_some(placement)
    }

    fn restore(hwnd: HWND, saved: Placement) {
        if let Some(mut native) = native_placement(hwnd) {
            native.showCmd = if saved.maximized {
                SW_SHOWMAXIMIZED
            } else {
                SW_SHOWNORMAL
            } as u32;
            native.flags = 0;
            native.rcNormalPosition = RECT {
                left: saved.left,
                top: saved.top,
                right: saved.right,
                bottom: saved.bottom,
            };
            // Keep workspace coordinates paired. Windows adjusts a placement when a display disappears.
            unsafe {
                SetWindowPlacement(hwnd, &native);
            }
        }
    }

    pub fn remember(window: &WebviewWindow, path: PathBuf) {
        if let Ok(hwnd) = window.hwnd() {
            use windows_sys::Win32::Graphics::Dwm::{
                DwmSetWindowAttribute, DWMSBT_MAINWINDOW, DWMWA_SYSTEMBACKDROP_TYPE,
            };
            let backdrop = DWMSBT_MAINWINDOW;
            // The compositor chooses the supported desktop material and accessibility fallback.
            unsafe {
                DwmSetWindowAttribute(
                    hwnd.0,
                    DWMWA_SYSTEMBACKDROP_TYPE as u32,
                    std::ptr::from_ref(&backdrop).cast(),
                    size_of::<i32>() as u32,
                );
            }
        }
        if let (Ok(hwnd), Some(saved)) = (window.hwnd(), load(&path)) {
            restore(hwnd.0, saved);
        }
        let frame = window.clone();
        window.on_window_event(move |event| {
            if matches!(event, WindowEvent::CloseRequested { .. }) {
                if let Some(value) = frame.hwnd().ok().and_then(|h| capture(h.0)) {
                    // A damaged or unwritable optional window preference never prevents closing the app.
                    let _ = save(&path, &value);
                }
            }
        });
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn preferences_accept_negative_monitor_positions_and_ignore_broken_files() {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join("frame.json");
            let saved = Placement {
                left: -1200,
                top: 30,
                right: -160,
                bottom: 790,
                maximized: true,
            };
            save(&path, &saved).unwrap();
            assert_eq!(load(&path), Some(saved));
            fs::write(&path, vec![b' '; 1100]).unwrap();
            assert!(load(&path).is_none());
            assert!(!Placement {
                left: i32::MIN,
                top: 0,
                right: i32::MAX,
                bottom: 760,
                maximized: false
            }
            .valid());
        }
    }
}

#[cfg(windows)]
pub use placement::remember;
