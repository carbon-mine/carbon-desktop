// SPDX-License-Identifier: MPL-2.0
use super::{Command, Launcher, Preference};
use futures::{StreamExt, channel::mpsc};
use gpui::{Context, Window};
use muda::{CheckMenuItem, Menu, MenuEvent, MenuItem, MenuTheme, PredefinedMenuItem, Submenu};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::mem::size_of;
use windows::Win32::{
    Foundation::HWND,
    Graphics::{
        Dwm::{
            DWMWA_CAPTION_COLOR, DWMWA_COLOR_DEFAULT, DWMWA_TEXT_COLOR,
            DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute,
        },
        Gdi::{RDW_FRAME, RDW_INVALIDATE, RedrawWindow},
    },
    UI::{
        Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
        WindowsAndMessaging::{SPI_GETHIGHCONTRAST, SystemParametersInfoW},
    },
};

pub fn configure_renderer() {
    // GPUI's HWND swap chain keeps the window redirection bitmap used by GDI menus.
    // SAFETY: Called before GPUI initializes. Windows also supports threaded environment access.
    unsafe { std::env::set_var("GPUI_DISABLE_DIRECT_COMPOSITION", "1") };
}

/// Suppress native size/maximize commands for the main HWND. The creation
/// style already omits resize borders. Move and minimize keep normal handling.
pub(super) fn configure_fixed_window(window: &Window) {
    use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::{
        Shell::{DefSubclassProc, SetWindowSubclass},
        WindowsAndMessaging::{SC_MAXIMIZE, SC_SIZE, WM_SYSCOMMAND},
    };
    unsafe extern "system" fn filter(
        hwnd: HWND,
        message: u32,
        command: WPARAM,
        data: LPARAM,
        _: usize,
        _: usize,
    ) -> LRESULT {
        if message == WM_SYSCOMMAND && matches!((command.0 as u32) & 0xfff0, SC_MAXIMIZE | SC_SIZE)
        {
            return LRESULT(0);
        }
        unsafe { DefSubclassProc(hwnd, message, command, data) }
    }
    if let Some(raw) = handle(window) {
        // Windows removes subclasses when it destroys their HWND.
        unsafe {
            let _ = SetWindowSubclass(HWND(raw as *mut _), Some(filter), 0x4344_4658, 0);
        }
    }
}

fn handle(window: &Window) -> Option<isize> {
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Win32(raw) => Some(raw.hwnd.get()),
        _ => None,
    }
}

pub fn apply_frame(window: &Window, dark: bool) {
    if let Some(id) = handle(window) {
        let hwnd = HWND(id as _);
        let system_colors = high_contrast();
        let (caption, text) = match (system_colors, dark) {
            (true, _) => (DWMWA_COLOR_DEFAULT, DWMWA_COLOR_DEFAULT),
            (false, true) => (0x202020, 0xf4f4f4),
            (false, false) => (0xf2f2f2, 0x161616),
        };
        // DWM borrows the live HWND and reads each supplied value during the call.
        unsafe {
            for (attribute, value) in [
                (
                    DWMWA_USE_IMMERSIVE_DARK_MODE,
                    u32::from(dark && !system_colors),
                ),
                (DWMWA_CAPTION_COLOR, caption),
                (DWMWA_TEXT_COLOR, text),
            ] {
                let _ = DwmSetWindowAttribute(
                    hwnd,
                    attribute,
                    std::ptr::from_ref(&value).cast(),
                    size_of::<u32>() as u32,
                );
            }
            let _ = RedrawWindow(Some(hwnd), None, None, RDW_INVALIDATE | RDW_FRAME);
        }
    }
}

pub fn high_contrast() -> bool {
    let mut state = HIGHCONTRASTW {
        cbSize: size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    // The system copies this accessibility setting into the initialized structure.
    let result = unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            state.cbSize,
            Some(std::ptr::from_mut(&mut state).cast()),
            Default::default(),
        )
    };
    result.is_ok() && state.dwFlags.contains(HCF_HIGHCONTRASTON)
}

pub struct NativeMenu {
    menu: Menu,
    hwnd: isize,
    choices: [CheckMenuItem; 3],
}

impl NativeMenu {
    pub fn install(
        window: &mut Window,
        cx: &mut Context<Launcher>,
        pref: Preference,
    ) -> anyhow::Result<Self> {
        let hwnd = handle(window)
            .ok_or_else(|| anyhow::anyhow!("No native window for application menus"))?;
        let file = Submenu::new("&File", true);
        let item = |id, text| MenuItem::with_id(id, text, true, None);
        // GPUI dispatches the keyboard bindings. Win32 renders the shortcuts after the tab.
        file.append_items(&[
            &item("new", "&New vanilla instance…\tCtrl+N"),
            &item("import", "&Import pack…\tCtrl+O"),
            &PredefinedMenuItem::separator(),
            &item("close", "&Close window\tCtrl+W"),
        ])?;
        let view = Submenu::new("&View", true);
        let appearance = Submenu::new("&Appearance", true);
        let choices = [
            CheckMenuItem::with_id(
                "system",
                "Use &system setting",
                true,
                pref == Preference::System,
                None,
            ),
            CheckMenuItem::with_id("light", "&Light", true, pref == Preference::Light, None),
            CheckMenuItem::with_id("dark", "&Dark", true, pref == Preference::Dark, None),
        ];
        for choice in &choices {
            appearance.append(choice)?;
        }
        view.append_items(&[
            &item("find", "&Find instance\tCtrl+F"),
            &item("activity", "View acti&vity\tF6"),
            &appearance,
        ])?;
        let menu = Menu::with_items(&[&file, &view])?;
        // muda attaches a ComCtl32 subclass and forwards other messages.
        unsafe {
            let dark = super::super::theme::Palette::read(pref, window.appearance()).dark;
            menu.init_for_hwnd_with_theme(hwnd, menu_theme(dark))?;
        }
        let (send, mut receive) = mpsc::unbounded();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let _ = send.unbounded_send(event.id.0);
        }));
        cx.spawn_in(window, async move |this, cx| {
            while let Some(id) = receive.next().await {
                let cmd = match id.as_str() {
                    "new" => Command::New,
                    "import" => Command::Import,
                    "close" => Command::Close,
                    "find" => Command::Find,
                    "activity" => Command::Activity,
                    "system" => Command::Appearance(Preference::System),
                    "light" => Command::Appearance(Preference::Light),
                    "dark" => Command::Appearance(Preference::Dark),
                    _ => continue,
                };
                if this
                    .update_in(cx, |launcher, window, cx| {
                        launcher.shell_command(cmd, window, cx)
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        Ok(Self {
            menu,
            hwnd,
            choices,
        })
    }
    pub fn select(&self, pref: Preference) {
        for (item, value) in
            self.choices
                .iter()
                .zip([Preference::System, Preference::Light, Preference::Dark])
        {
            item.set_checked(value == pref);
        }
    }

    pub fn apply_appearance(&self, dark: bool) {
        // The menu is owned by the launcher for this live HWND.
        unsafe {
            let _ = self.menu.set_theme_for_hwnd(self.hwnd, menu_theme(dark));
        }
    }
}

fn menu_theme(dark: bool) -> MenuTheme {
    if dark && !high_contrast() {
        MenuTheme::Dark
    } else {
        MenuTheme::Light
    }
}
