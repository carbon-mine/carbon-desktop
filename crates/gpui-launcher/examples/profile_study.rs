// SPDX-License-Identifier: MPL-2.0
#[path = "../../../profile-study/profile.rs"]
mod profile;
use gpui::{prelude::*, *};
use gpui_component::{
    Root,
    input::{Input, InputEvent, InputState},
};
use profile::{PROFILES, Profile};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Lookup,
    Me,
}

struct Study {
    page: Page,
    account: Option<Profile>,
    result: Option<Profile>,
    message: String,
    query: Entity<InputState>,
    focus: FocusHandle,
    yaw: f32,
    pitch: f32,
    fixed: bool,
    drag: Option<Point<Pixels>>,
    cached: Option<(String, i32, i32, Arc<RenderImage>)>,
    connection: usize,
    seen: Vec<Profile>,
}

impl Study {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Java name or UUID"));
        cx.subscribe_in(&query, window, |this, input, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.search(input.read(cx).value().as_ref(), cx);
            }
        })
        .detach();
        Self {
            page: Page::Lookup,
            account: Some(PROFILES[0]),
            result: Some(PROFILES[1]),
            message: String::new(),
            query,
            focus: cx.focus_handle().tab_stop(true),
            yaw: -20.,
            pitch: 4.,
            fixed: false,
            drag: None,
            cached: None,
            connection: 0,
            seen: vec![PROFILES[1]],
        }
    }
    fn search(&mut self, query: &str, cx: &mut Context<Self>) {
        if self.connection != 0 {
            let reason = if self.connection == 1 {
                "Offline"
            } else {
                "Rate limited"
            };
            self.result = self.seen.iter().copied().find(|p| {
                p.name.eq_ignore_ascii_case(query.trim())
                    || p.id.eq_ignore_ascii_case(query.trim())
                    || p.id.replace('-', "").eq_ignore_ascii_case(query.trim())
            });
            self.message = format!(
                "{reason} - {}",
                if self.result.is_some() {
                    "last known fixture profile"
                } else {
                    "no cached profile"
                }
            );
            self.reset();
            cx.notify();
            return;
        }
        match profile::lookup(query) {
            Ok(profile) => {
                if !self.seen.contains(&profile) {
                    self.seen.push(profile);
                }
                self.result = Some(profile);
                self.message.clear();
            }
            Err(err) => {
                self.result = None;
                self.message = match err {
                    profile::LookupError::Invalid => "Enter an exact Java name or UUID.",
                    profile::LookupError::Missing => "Player not found.",
                }
                .into();
            }
        }
        self.reset();
        cx.notify();
    }
    fn reset(&mut self) {
        self.yaw = -20.;
        self.pitch = 4.;
        self.drag = None
    }
    fn current(&self) -> Option<Profile> {
        if self.page == Page::Lookup {
            self.result
        } else {
            self.account
        }
    }
    fn portrait(&mut self, skin: &str) -> Arc<RenderImage> {
        let (yaw, pitch) = if self.fixed {
            (0, 0)
        } else {
            (self.yaw.round() as i32, self.pitch.round() as i32)
        };
        if let Some((name, y, p, img)) = &self.cached
            && name == skin
            && (*y, *p) == (yaw, pitch)
        {
            return img.clone();
        }
        let mut bytes = profile::bust_rgba(skin, yaw as f32, pitch as f32, 256);
        for pixel in bytes.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        let pixels = image::RgbaImage::from_raw(256, 256, bytes).expect("fixture bust dimensions");
        let frame = Arc::new(RenderImage::new(vec![image::Frame::new(pixels)]));
        self.cached = Some((skin.into(), yaw, pitch, frame.clone()));
        frame
    }
    fn button(
        &self,
        id: &'static str,
        label: &'static str,
        selected: bool,
        f: impl Fn(&mut Study, &mut Window, &mut Context<Study>) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id(id)
            .px_3()
            .py_1()
            .rounded_sm()
            .bg(rgb(if selected { 0x4a4b4d } else { 0x303133 }))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(0x48494a)))
            .on_click(cx.listener(move |s, _, w, cx| f(s, w, cx)))
            .child(label)
    }
}

impl Render for Study {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut sidebar = div()
            .flex()
            .flex_col()
            .w(px(180.))
            .flex_shrink_0()
            .p_3()
            .gap_2()
            .bg(rgb(0x202123))
            .child(div().text_lg().mb_5().child("Carbon Desktop"));
        for (page, label) in [(Page::Lookup, "Player Lookup"), (Page::Me, "My Profile")] {
            sidebar = sidebar.child(self.button(
                label,
                label,
                self.page == page,
                move |s, _, cx| {
                    s.page = page;
                    s.reset();
                    cx.notify()
                },
                cx,
            ));
        }
        sidebar = sidebar.child(div().flex_1()).child(
            div()
                .text_xs()
                .text_color(rgb(0xbcbdbd))
                .child("Fixture accounts"),
        );
        for p in PROFILES {
            sidebar = sidebar.child(self.button(
                p.name,
                p.name,
                self.account == Some(p),
                move |s, _, cx| {
                    s.account = Some(p);
                    if s.page == Page::Me {
                        s.reset();
                    }
                    cx.notify()
                },
                cx,
            ));
        }
        sidebar = sidebar.child(self.button(
            "signout",
            "Sign out",
            false,
            |s, _, cx| {
                s.account = None;
                cx.notify()
            },
            cx,
        ));
        let connection = ["Online", "Offline", "Rate limited"][self.connection];
        sidebar = sidebar.child(self.button(
            "connection",
            connection,
            false,
            |s, _, cx| {
                s.connection = (s.connection + 1) % 3;
                cx.notify()
            },
            cx,
        ));
        let mut main = div()
            .flex_1()
            .min_w_0()
            .p_6()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(0xc1c1c1))
                    .child("Minecraft Java Edition"),
            )
            .child(div().text_3xl().font_weight(FontWeight::SEMIBOLD).child(
                if self.page == Page::Me {
                    "My Profile"
                } else {
                    "Player Lookup"
                },
            ));
        if self.page == Page::Lookup {
            main = main.child(
                div()
                    .flex()
                    .gap_2()
                    .items_center()
                    .child(div().flex_1().child(Input::new(&self.query)))
                    .child(self.button(
                        "search",
                        "Search",
                        true,
                        |s, _, cx| {
                            let q = s.query.read(cx).value().to_string();
                            s.search(&q, cx)
                        },
                        cx,
                    )),
            );
        }
        if let Some(p) = self.current() {
            let mut stage = div()
                .id("bust")
                .track_focus(&self.focus)
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .h(px(260.))
                .bg(rgb(0x2d2f2e))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|s, e: &MouseDownEvent, w, cx| {
                        w.focus(&s.focus);
                        s.drag = Some(e.position);
                        cx.notify();
                    }),
                )
                .on_mouse_move(cx.listener(|s, e: &MouseMoveEvent, _, cx| {
                    if e.pressed_button != Some(MouseButton::Left) {
                        s.drag = None;
                        return;
                    }
                    if let Some(last) = s.drag.replace(e.position)
                        && !s.fixed
                    {
                        s.yaw = (s.yaw + f32::from(e.position.x - last.x) * 0.8) % 360.;
                        s.pitch =
                            (s.pitch - f32::from(e.position.y - last.y) * 0.5).clamp(-55., 55.);
                        cx.notify();
                    }
                }))
                .on_key_down(cx.listener(|s, e: &KeyDownEvent, _, cx| {
                    if s.fixed {
                        return;
                    }
                    match e.keystroke.key.as_str() {
                        "left" => s.yaw -= 15.,
                        "right" => s.yaw += 15.,
                        "up" => s.pitch = (s.pitch + 10.).min(55.),
                        "down" => s.pitch = (s.pitch - 10.).max(-55.),
                        "home" => s.reset(),
                        _ => return,
                    }
                    cx.stop_propagation();
                    cx.notify()
                }));
            if let Some(skin) = p.skin {
                stage = stage.child(
                    img(ImageSource::Render(self.portrait(skin)))
                        .w(px(256.))
                        .h(px(256.)),
                );
            } else {
                stage = stage.child("Skin unavailable");
            }
            main = main
                .child(stage)
                .child(
                    div()
                        .text_2xl()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(p.name),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(p.id)
                        .child(self.button(
                            "copy",
                            "Copy UUID",
                            false,
                            move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(p.id.into()))
                            },
                            cx,
                        )),
                );
            if p.skin.is_some() {
                main = main
                    .child(
                        div()
                            .text_sm()
                            .child(if p.slim { "Slim arms" } else { "Classic arms" }),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(self.button(
                                "reset",
                                "Reset view",
                                false,
                                |s, _, cx| {
                                    s.reset();
                                    cx.notify()
                                },
                                cx,
                            ))
                            .child(self.button(
                                "fixed",
                                "Static portrait",
                                self.fixed,
                                |s, _, cx| {
                                    s.fixed = !s.fixed;
                                    cx.notify()
                                },
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0xb5b6b5))
                            .child("Drag to orbit, or focus the portrait and use the arrow keys."),
                    );
            }
        } else if self.page == Page::Me {
            main = main.child("Sign in to see your profile").child(self.button(
                "signin",
                "Sign in with a fixture",
                true,
                |s, _, cx| {
                    s.account = Some(PROFILES[0]);
                    cx.notify()
                },
                cx,
            ));
        }
        if self.page == Page::Lookup && !self.message.is_empty() {
            main = main.child(self.message.clone());
        }
        div()
            .size_full()
            .flex()
            .bg(rgb(0x191a1b))
            .text_color(rgb(0xf0f0ef))
            .child(sidebar)
            .child(main)
    }
}
fn main() {
    Application::new().run(|cx: &mut App| {
        gpui_component::init(cx);
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit()
            }
        })
        .detach();
        cx.open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("Carbon Desktop · Java profiles".into()),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(880.), px(670.)),
                    cx,
                ))),
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(|cx| Study::new(window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .expect("open profile study");
    });
}
