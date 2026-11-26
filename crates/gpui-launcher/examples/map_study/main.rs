#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
// SPDX-License-Identifier: MPL-2.0
// This example is a standalone design study with original synthetic imagery.
#[allow(dead_code)]
#[path = "../../src/accessibility.rs"]
mod accessibility;
#[cfg(test)]
mod checks;
mod fixtures;
#[allow(dead_code)]
#[path = "../../src/icons.rs"]
mod icons;
mod terrain;

use accessibility::{Accessibility, Semantic};
use accesskit::Role;
use fixtures::{DIMENSIONS, INSTANCES, Selection};
use gpui::{
    AnyElement, App, AppContext, Application, Bounds, Context, Div, FocusHandle, KeyDownEvent,
    MouseButton, Pixels, Point, SharedString, TitlebarOptions, Window, WindowBounds, WindowOptions,
    div, prelude::*, px, rgb, size,
};
use gpui_component::StyledExt;
use std::{cell::RefCell, collections::HashMap};
use terrain::{Camera, Coverage, Terrain};

const BG: u32 = 0x1b1c1e;
const PANEL: u32 = 0x242527;
const EDGE: u32 = 0x414345;
const MUTED: u32 = 0xb8b9ba;
const INK: u32 = 0xf0f0ed;

#[derive(Clone, Copy, Hash, Eq, PartialEq, Debug)]
enum Command {
    Library,
    Map,
    Expand(usize),
    Fullscreen,
    Detach,
    Close,
    Instance(usize),
    World(usize),
    Dimension(usize),
    ZoomIn,
    ZoomOut,
    Recenter,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Page {
    Library,
    Map,
}

struct Study {
    access: Accessibility,
    terrain: Terrain,
    page: Page,
    selection: Selection,
    camera: Camera,
    last_pointer: Option<Point<Pixels>>,
    drag_scale: std::rc::Rc<std::cell::Cell<f32>>,
    detached: bool,
    return_to: Option<(Page, Command)>,
    focus: RefCell<HashMap<Command, FocusHandle>>,
    stage_focus: FocusHandle,
    root_focus: FocusHandle,
}
fn row() -> Div {
    div().flex().items_center()
}
fn col() -> Div {
    div().flex().flex_col()
}
fn fine(s: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(12.))
        .text_color(rgb(MUTED))
        .child(s.into())
}

fn open(
    terrain: Terrain,
    detached: bool,
    selection: Selection,
    camera: Camera,
    cx: &mut App,
) -> gpui::Result<()> {
    let title = if detached {
        format!("{} · Carbon Map", selection.title())
    } else {
        "Carbon Desktop · Map study".into()
    };
    let bounds = Bounds::centered(
        None,
        size(
            px(if detached { 820. } else { 1080. }),
            px(if detached { 610. } else { 760. }),
        ),
        cx,
    );
    cx.open_window(
        WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some(title.clone().into()),
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(380.), px(440.))),
            show: !cfg!(windows),
            focus: !cfg!(windows),
            ..Default::default()
        },
        move |window, cx| {
            let view = cx.new(|cx| {
                let root_focus = cx.focus_handle();
                let stage_focus = cx.focus_handle().tab_stop(true);
                if detached {
                    stage_focus.focus(window)
                } else {
                    root_focus.focus(window)
                }
                Study {
                    access: Accessibility::new(
                        window,
                        cx,
                        if detached { "World map".into() } else { title },
                    ),
                    terrain,
                    page: if detached { Page::Map } else { Page::Library },
                    selection,
                    camera,
                    detached,
                    return_to: None,
                    focus: RefCell::new(HashMap::new()),
                    stage_focus,
                    root_focus,
                    last_pointer: None,
                    drag_scale: Default::default(),
                }
            });
            #[cfg(windows)]
            window.activate_window();
            view
        },
    )?;
    Ok(())
}
impl Study {
    fn handle(&self, command: Command, cx: &App) -> FocusHandle {
        self.focus
            .borrow_mut()
            .entry(command)
            .or_insert_with(|| cx.focus_handle().tab_stop(true))
            .clone()
    }

    fn activate(&mut self, cmd: Command, w: &mut Window, cx: &mut Context<Self>) {
        match cmd {
            Command::Library => self.page = Page::Library,
            Command::Map => {
                self.page = Page::Map;
                self.stage_focus.focus(w);
            }
            Command::Expand(i) => {
                self.selection = Selection::last(i);
                self.camera = self.selection.camera();
                self.return_to = Some((Page::Library, cmd));
                self.page = Page::Map;
                if !w.is_fullscreen() {
                    w.toggle_fullscreen();
                }
                self.stage_focus.focus(w);
            }
            Command::Fullscreen => {
                self.return_to = Some((self.page, cmd));
                if !w.is_fullscreen() {
                    w.toggle_fullscreen();
                }
                self.stage_focus.focus(w);
            }
            Command::Close => {
                if let Some((page, opener)) = self.return_to.take() {
                    if w.is_fullscreen() {
                        w.toggle_fullscreen();
                    }
                    self.page = page;
                    self.handle(opener, cx).focus(w);
                } else if self.detached {
                    w.remove_window();
                    return;
                } else {
                    self.page = Page::Library;
                    self.handle(Command::Map, cx).focus(w);
                }
            }
            Command::Detach => {
                if let Err(err) = open(self.terrain.clone(), true, self.selection, self.camera, cx)
                {
                    eprintln!("Could not open separate map: {err}");
                }
            }
            Command::Instance(i) => {
                self.selection = Selection::last(i);
                self.camera = self.selection.camera();
            }
            Command::World(i) => {
                self.selection.world = i;
                self.selection.dimension = INSTANCES[self.selection.instance].last_dimension;
                self.camera = self.selection.camera();
            }
            Command::Dimension(i) => {
                self.selection.dimension = i;
                self.camera = self.selection.camera();
            }
            Command::ZoomIn => self.camera.magnify(1.25),
            Command::ZoomOut => self.camera.magnify(0.8),
            Command::Recenter => self.camera = self.selection.camera(),
        }
        if self.detached {
            w.set_window_title(&format!("{} · Carbon Map", self.selection.title()));
        }
        self.last_pointer = None;
        cx.notify();
    }

    fn control(
        &self,
        cmd: Command,
        text: &str,
        label: &str,
        selected: bool,
        cx: &Context<Self>,
    ) -> Semantic {
        let focus = self.handle(cmd, cx);
        let id: SharedString = format!("{cmd:?}").into();
        let button = row()
            .id(id.clone())
            .track_focus(&focus)
            .tab_stop(true)
            .min_h(px(31.))
            .px_2p5()
            .justify_center()
            .rounded(px(3.))
            .border_1()
            .border_color(rgb(if selected { 0x848689 } else { EDGE }))
            .bg(rgb(if selected { 0x444648 } else { PANEL }))
            .cursor_pointer()
            .text_size(px(12.))
            .font_medium()
            .child(text.to_owned())
            .hover(|s| s.bg(rgb(0x393b3d)))
            .focus(|s| s.border_color(rgb(INK)))
            .on_click(cx.listener(move |this, _, w, cx| this.activate(cmd, w, cx)))
            .on_key_down(cx.listener(move |this, e: &KeyDownEvent, w, cx| {
                if matches!(e.keystroke.key.as_str(), "enter" | "space") {
                    cx.stop_propagation();
                    this.activate(cmd, w, cx);
                }
            }));
        let weak = cx.weak_entity();
        let choice = matches!(
            cmd,
            Command::Instance(_)
                | Command::World(_)
                | Command::Dimension(_)
                | Command::Library
                | Command::Map
        );
        self.access
            .button(
                button,
                id.into(),
                label,
                if choice {
                    Role::RadioButton
                } else {
                    Role::Button
                },
                false,
                choice.then_some(selected),
                move |w, cx| {
                    let _ = weak.update(cx, |this, cx| this.activate(cmd, w, cx));
                },
            )
            .with_focus(focus)
    }

    fn tile(&self, i: usize, compact: bool, cx: &Context<Self>) -> Div {
        let selection = Selection::last(i);
        let instance = &INSTANCES[i];
        let coverage = selection.coverage();
        let expand = || {
            self.control(
                Command::Expand(i),
                "Expand",
                &format!("Expand map for {}", instance.name),
                false,
                cx,
            )
        };
        let info = col()
            .p_3()
            .gap_1()
            .child(
                self.access.text(
                    div()
                        .font_semibold()
                        .text_size(px(19.))
                        .child(instance.name),
                    format!("name-{i}"),
                    instance.name,
                    Role::Heading,
                ),
            )
            .child(self.access.text(
                fine(instance.detail),
                format!("version-{i}"),
                instance.detail,
                Role::Label,
            ))
            .child(
                row()
                    .gap_2()
                    .mt_1()
                    .border_t_1()
                    .border_color(rgb(EDGE))
                    .pt_2()
                    .child(self.access.text(
                        div().text_size(px(13.)).child(selection.title()),
                        format!("world-{i}"),
                        selection.title(),
                        Role::Label,
                    ))
                    .child(self.access.text(
                        fine(format!("\u{00b7} {}", DIMENSIONS[selection.dimension])),
                        format!("dimension-{i}"),
                        DIMENSIONS[selection.dimension],
                        Role::Label,
                    )),
            );
        if compact {
            return row()
                .w_full()
                .h(px(108.))
                .flex_shrink_0()
                .px_3()
                .gap_1()
                .border_1()
                .border_color(rgb(EDGE))
                .rounded(px(5.))
                .bg(rgb(PANEL))
                .child(icons::Icon::Instance.render(px(20.), rgb(MUTED)))
                .child(info.flex_1().min_w_0())
                .child(expand());
        }
        let preview = div()
            .relative()
            .w_full()
            .h(px(126.))
            .overflow_hidden()
            .bg(rgb(0x2f3234))
            .when(coverage != Coverage::Empty, |stage| {
                stage.child(self.terrain.surface(
                    coverage,
                    selection.dimension,
                    selection.camera(),
                    selection.position(),
                ))
            })
            .when(coverage == Coverage::Empty, |stage| {
                stage.child(
                    row()
                        .size_full()
                        .gap_2()
                        .px_4()
                        .child(icons::Icon::Instance.render(px(22.), rgb(MUTED)))
                        .child(fine("No saved terrain")),
                )
            })
            .child(div().absolute().top(px(8.)).right(px(8.)).child(expand()));
        col()
            .min_w_0()
            .h(px(244.))
            .overflow_hidden()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(EDGE))
            .rounded(px(5.))
            .child(preview)
            .child(info)
    }

    fn library(&self, width: f32, cx: &Context<Self>) -> AnyElement {
        let narrow = width < 740.;
        let mut cards = col().flex_shrink_0().gap_3();
        if narrow {
            for i in 0..INSTANCES.len() {
                cards = cards.child(self.tile(i, width < 445., cx).w_full().flex_shrink_0());
            }
        } else {
            for first in [0, 2] {
                cards = cards.child(
                    row()
                        .items_start()
                        .gap_3()
                        .child(self.tile(first, false, cx).flex_1())
                        .child(self.tile(first + 1, false, cx).flex_1()),
                );
            }
        }
        col()
            .id("library-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_4()
            .gap_3()
            .child(
                col()
                    .gap_1()
                    .child(
                        self.access.text(
                            div()
                                .text_size(px(28.))
                                .font_semibold()
                                .child("Instance library"),
                            "library-heading",
                            "Instance library",
                            Role::Heading,
                        ),
                    )
                    .child(fine("Local worlds and saved servers")),
            )
            .child(cards)
            .into_any_element()
    }

    fn group(&self, heading: &'static str, items: impl IntoIterator<Item = Semantic>) -> Div {
        row()
            .gap_1()
            .flex_wrap()
            .child(fine(heading))
            .children(items)
    }

    fn viewer(&self, narrow: bool, cx: &Context<Self>) -> Div {
        let s = self.selection;
        let coverage = s.coverage();
        let mut actions = row().gap_1().flex_wrap();
        if self.return_to.is_none() {
            actions = actions.child(self.control(
                Command::Fullscreen,
                "Fullscreen",
                "View map fullscreen",
                false,
                cx,
            ));
        }
        if !self.detached {
            actions = actions.child(self.control(
                Command::Detach,
                "Separate window",
                "Open this map in a separate window",
                false,
                cx,
            ));
        }
        if self.return_to.is_some() || self.detached {
            actions = actions.child(self.control(Command::Close, "Close", "Close map", false, cx));
        }
        let header = row()
            .justify_between()
            .gap_2()
            .when(narrow, |h| h.flex_col().items_start())
            .child(
                col().gap_1().child(fine(INSTANCES[s.instance].name)).child(
                    self.access.text(
                        div()
                            .font_semibold()
                            .text_size(px(if narrow { 22. } else { 27. }))
                            .child(s.title()),
                        "world-title",
                        s.title(),
                        Role::Heading,
                    ),
                ),
            )
            .child(actions);
        let instances = INSTANCES.iter().enumerate().map(|(i, v)| {
            self.control(
                Command::Instance(i),
                v.name,
                &format!("Choose instance {}", v.name),
                i == s.instance,
                cx,
            )
        });
        let worlds = INSTANCES[s.instance]
            .worlds
            .iter()
            .enumerate()
            .map(|(i, v)| {
                self.control(
                    Command::World(i),
                    v,
                    &format!("Choose world {v}"),
                    i == s.world,
                    cx,
                )
            });
        let dims = DIMENSIONS[..s.dimensions()]
            .iter()
            .enumerate()
            .map(|(i, v)| self.control(Command::Dimension(i), v, v, i == s.dimension, cx));
        let selectors = col()
            .gap_2()
            .child(self.group("Instance", instances))
            .child(
                row()
                    .flex_wrap()
                    .gap_2()
                    .child(self.group("World", worlds))
                    .child(self.group("Dimension", dims)),
            );
        let mut stage = div()
            .id("terrain")
            .relative()
            .overflow_hidden()
            .w_full()
            .flex_1()
            .min_h_0()
            .track_focus(&self.stage_focus)
            .tab_stop(true)
            .border_1()
            .border_color(rgb(EDGE))
            .bg(rgb(0x292c2e))
            .focus(|v| v.border_color(rgb(0xa6a8a9)));
        if coverage != Coverage::Empty {
            let scale = self.drag_scale.clone();
            stage = stage
                .cursor_move()
                .child(
                    self.terrain
                        .surface(coverage, s.dimension, self.camera, s.position()),
                )
                .child(
                    gpui::canvas(
                        move |bounds, _, _| {
                            scale.set(
                                512. / f32::from(bounds.size.width.max(bounds.size.height)).max(1.),
                            )
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset_0(),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, e: &gpui::MouseDownEvent, w, _| {
                        this.stage_focus.focus(w);
                        this.last_pointer = Some(e.position);
                    }),
                )
                .on_mouse_move(cx.listener(|this, e: &gpui::MouseMoveEvent, w, cx| {
                    if e.dragging() {
                        if let Some(start) = this.last_pointer.replace(e.position) {
                            let gain = this.drag_scale.get() / this.camera.zoom;
                            this.camera.pan(
                                f32::from(start.x - e.position.x) * gain,
                                f32::from(start.y - e.position.y) * gain,
                            );
                            w.refresh();
                            cx.notify();
                        }
                    } else {
                        this.last_pointer = None;
                    }
                }))
                .on_scroll_wheel(cx.listener(|this, e: &gpui::ScrollWheelEvent, _, cx| {
                    let delta = f32::from(e.delta.pixel_delta(px(18.)).y);
                    this.camera.magnify((delta * 0.005).clamp(-0.5, 0.5).exp());
                    cx.notify();
                }));
        } else {
            stage = stage.child(
                col()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .child(icons::Icon::Instance.render(px(34.), rgb(MUTED)))
                    .child(div().font_semibold().child("No saved terrain"))
                    .child(fine("No readable tiles for this world and dimension.")),
            );
        }
        let mut footer = row()
            .gap_2()
            .justify_between()
            .flex_wrap()
            .child(self.access.text(
                fine(coverage.label()),
                "coverage",
                coverage.label(),
                Role::Label,
            ));
        if coverage != Coverage::Empty {
            let position = format!(
                "X {:.0}   Z {:.0}   ·   {:.0}%",
                self.camera.x,
                self.camera.z,
                self.camera.zoom * 100.
            );
            footer = footer.child(
                row()
                    .gap_2()
                    .flex_wrap()
                    .child(self.access.text(
                        fine(position.clone()),
                        "position",
                        position,
                        Role::Label,
                    ))
                    .child(self.control(Command::ZoomOut, "−", "Zoom out", false, cx))
                    .child(self.control(Command::ZoomIn, "+", "Zoom in", false, cx))
                    .child(self.control(Command::Recenter, "Center", "Recenter map", false, cx)),
            );
        }
        col()
            .min_h_0()
            .flex_1()
            .p_3()
            .gap_2()
            .child(header)
            .child(selectors)
            .child(self.access.focus_group(
                stage,
                "terrain-stage",
                if coverage == Coverage::Empty {
                    "Map viewport. No saved terrain."
                } else {
                    "Map viewport. Arrow keys pan. Plus and minus zoom."
                },
                Role::Image,
                &self.stage_focus,
            ))
            .child(footer)
    }
}

impl Render for Study {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let width = f32::from(w.viewport_size().width);
        let shell = col()
            .size_full()
            .bg(rgb(BG))
            .text_color(rgb(INK))
            .font_family(if cfg!(windows) {
                "Segoe UI"
            } else {
                ".SystemUIFont"
            })
            .text_size(px(14.))
            .track_focus(&self.root_focus)
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, w, cx| {
                match e.keystroke.key.as_str() {
                    "escape" if this.page == Page::Map => this.activate(Command::Close, w, cx),
                    "tab" => {
                        if e.keystroke.modifiers.shift {
                            w.focus_prev()
                        } else {
                            w.focus_next()
                        }
                    }
                    key if this.stage_focus.is_focused(w)
                        && this.selection.coverage() != Coverage::Empty =>
                    {
                        let step = 24. / this.camera.zoom;
                        match key {
                            "left" => this.camera.pan(-step, 0.),
                            "right" => this.camera.pan(step, 0.),
                            "up" => this.camera.pan(0., -step),
                            "down" => this.camera.pan(0., step),
                            "+" | "=" => this.camera.magnify(1.25),
                            "-" => this.camera.magnify(0.8),
                            _ => return,
                        }
                    }
                    _ => return,
                }
                cx.stop_propagation();
                cx.notify();
            }))
            .when(!self.detached && self.return_to.is_none(), |v| {
                v.child(
                    row()
                        .gap_2()
                        .px_3()
                        .py_2()
                        .bg(rgb(PANEL))
                        .border_b_1()
                        .border_color(rgb(EDGE))
                        .child(icons::Icon::Instance.render(px(18.), rgb(INK)))
                        .child(div().font_semibold().mr_2().child("Carbon Desktop"))
                        .child(self.control(
                            Command::Library,
                            "Library",
                            "Instance library",
                            self.page == Page::Library,
                            cx,
                        ))
                        .child(self.control(
                            Command::Map,
                            "Map",
                            "Map workspace",
                            self.page == Page::Map,
                            cx,
                        )),
                )
            })
            .child(match self.page {
                Page::Library => self.library(width, cx),
                Page::Map => self.viewer(width < 720., cx).into_any_element(),
            })
            .child(
                row()
                    .px_3()
                    .py_1()
                    .border_t_1()
                    .border_color(rgb(EDGE))
                    .child(fine("Map study · synthetic terrain")),
            );
        self.access.frame(shell, false)
    }
}

fn main() {
    Application::new().with_assets(icons::Assets).run(|cx| {
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit()
            }
        })
        .detach();
        open(
            Terrain::load(),
            false,
            Selection::default(),
            Selection::default().camera(),
            cx,
        )
        .expect("Open map study");
    })
}
