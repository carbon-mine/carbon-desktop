// SPDX-License-Identifier: MPL-2.0
use super::{Command, Launcher, Page, Preference};
use crate::accessibility::Accessibility;
use crate::theme::{Palette, Partition, compact_action, label};
use accesskit::Role;
use gpui::{
    App, Bounds, Context, FocusHandle, KeyBinding, TitlebarOptions, Window, WindowBounds,
    WindowDecorations, WindowKind, WindowOptions, actions, div, prelude::*, px, size,
};
use gpui_component::Root;

actions!(command_menu, [Previous, Next, Accept, Dismiss]);

pub(super) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("alt-f", super::OpenFile, Some("CarbonDesktop")),
        KeyBinding::new("alt-v", super::OpenView, Some("CarbonDesktop")),
        KeyBinding::new("up", Previous, Some("CarbonCommandMenu")),
        KeyBinding::new("shift-tab", Previous, Some("CarbonCommandMenu")),
        KeyBinding::new("down", Next, Some("CarbonCommandMenu")),
        KeyBinding::new("tab", Next, Some("CarbonCommandMenu")),
        KeyBinding::new("enter", Accept, Some("CarbonCommandMenu")),
        KeyBinding::new("space", Accept, Some("CarbonCommandMenu")),
        KeyBinding::new("escape", Dismiss, Some("CarbonCommandMenu")),
    ]);
}

struct MenuWindow {
    accessibility: Accessibility,
    items: Vec<(&'static str, Command)>,
    focus: FocusHandle,
    cursor: usize,
    selected: Option<Command>,
    pref: Preference,
}
pub(super) fn open(
    launcher: &mut Launcher,
    page: Page,
    window: &mut Window,
    cx: &mut Context<Launcher>,
) {
    if launcher.popup.is_some() || launcher.picker_open {
        return;
    }
    let items = match page {
        Page::File => vec![
            ("New vanilla instance…   Ctrl+N", Command::New),
            ("Import pack…   Ctrl+O", Command::Import),
            ("Close window   Ctrl+W", Command::Close),
        ],
        Page::View => vec![
            ("Find instance   Ctrl+F", Command::Find),
            ("View activity   F6", Command::Activity),
            (
                "Use system setting",
                Command::Appearance(Preference::System),
            ),
            ("Light", Command::Appearance(Preference::Light)),
            ("Dark", Command::Appearance(Preference::Dark)),
        ],
    };
    let parent = window.window_handle();
    let owner = cx.weak_entity();
    let restore = window.focused(cx);
    let pref = launcher.preference;
    let title = match page {
        Page::File => "File",
        Page::View => "View",
    };
    let bounds = Bounds::centered_at(
        window.bounds().center(),
        size(px(315.), px(28. + 44. * items.len() as f32)),
    );
    let opened = cx.open_window(
        WindowOptions {
            kind: WindowKind::Floating,
            titlebar: Some(TitlebarOptions {
                title: Some(title.into()),
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_decorations: Some(WindowDecorations::Server),
            is_minimizable: false,
            is_resizable: false,
            app_id: Some("dev.carbonmine.gpui".into()),
            ..Default::default()
        },
        move |window, cx| {
            let menu = cx.new(|cx: &mut Context<MenuWindow>| {
                let focus = cx.focus_handle();
                focus.focus(window);
                cx.on_release(move |menu, cx| {
                    let selected = menu.selected;
                    let _ = parent.update(cx, move |_, win, cx| {
                        win.activate_window();
                        let _ = owner.update(cx, |launcher, cx| {
                            launcher.popup = None;
                            if let Some(f) = restore {
                                f.focus(win);
                            }
                            cx.notify();
                        });
                        if let Some(command) = selected {
                            win.on_next_frame(move |win, cx| {
                                let _ = owner.update(cx, |launcher, cx| {
                                    launcher.shell_command(command, win, cx)
                                });
                            });
                        }
                    });
                })
                .detach();
                MenuWindow {
                    accessibility: Accessibility::new(window, cx, title.into()),
                    items,
                    focus,
                    cursor: 0,
                    selected: None,
                    pref,
                }
            });
            cx.new(|cx| Root::new(menu, window, cx))
        },
    );
    match opened {
        Ok(handle) => launcher.popup = Some(handle.into()),
        Err(error) => launcher.message = format!("Could not open the {title} menu: {error}"),
    }
    cx.notify();
}
impl Render for MenuWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::read(self.pref, window.appearance());
        let menu = div()
            .size_full()
            .flex()
            .flex_col()
            .p_2()
            .gap_1()
            .bg(p.surface)
            .text_color(p.ink())
            .track_focus(&self.focus)
            .key_context("CarbonCommandMenu")
            .on_action(cx.listener(|s, _: &Previous, _, cx| {
                s.cursor = (s.cursor + s.items.len() - 1) % s.items.len();
                cx.notify();
            }))
            .on_action(cx.listener(|s, _: &Next, _, cx| {
                s.cursor = (s.cursor + 1) % s.items.len();
                cx.notify();
            }))
            .on_action(cx.listener(|s, _: &Accept, w, _| {
                s.selected = Some(s.items[s.cursor].1);
                w.remove_window();
            }))
            .on_action(cx.listener(|_, _: &Dismiss, w, _| w.remove_window()))
            .children(self.items.iter().enumerate().map(|(i, (name, cmd))| {
                let selected = matches!(cmd,Command::Appearance(pref) if *pref==self.pref);
                let treatment = if i == self.cursor {
                    Partition::Selected
                } else {
                    Partition::Neutral
                };
                let text = if selected {
                    format!("✓ {name}")
                } else {
                    name.to_string()
                };
                let command = *cmd;
                let id = ("menu-item", i);
                let button = label(
                    compact_action(id, p, treatment, false, cx).w_full(),
                    text,
                    p,
                    treatment,
                    false,
                )
                .on_click(cx.listener(move |s, _, w, _| {
                    s.selected = Some(command);
                    w.remove_window();
                }));
                let weak = cx.weak_entity();
                self.accessibility.button(
                    button,
                    id.into(),
                    *name,
                    Role::MenuItem,
                    false,
                    matches!(cmd, Command::Appearance(_)).then_some(selected),
                    move |window, cx| {
                        let _ = weak.update(cx, |menu, _| {
                            menu.selected = Some(command);
                            window.remove_window();
                        });
                    },
                )
            }));
        self.accessibility.frame(
            self.accessibility
                .group(menu, "menu", "Launcher commands", Role::Menu),
            false,
        )
    }
}
