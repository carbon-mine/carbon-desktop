// SPDX-License-Identifier: MPL-2.0
use super::*;
use crate::icons::Icon;
use carbon_profiles::icons::{IconDraft, catalog};
use gpui::{ScrollAnchor, ScrollHandle};

const TILE_WIDTH: f32 = 101.;
const TILE_GAP: f32 = 8.;

struct Choice {
    focus: FocusHandle,
    anchor: ScrollAnchor,
}

pub(super) struct Picker {
    pub(super) draft: IconDraft,
    pub(super) scroll: ScrollHandle,
    choices: Vec<Choice>,
}

impl Picker {
    pub(super) fn new(profile: &Profile, window: &mut Window, cx: &mut Context<Popup>) -> Self {
        let draft = IconDraft::new(profile);
        let scroll = ScrollHandle::new();
        let choices: Vec<_> = catalog()
            .iter()
            .map(|_| Choice {
                focus: cx.focus_handle(),
                anchor: ScrollAnchor::for_handle(scroll.clone()),
            })
            .collect();
        if let Some(index) = catalog()
            .iter()
            .position(|icon| icon.id == draft.preview().id)
        {
            choices[index].focus.focus(window);
            choices[index].anchor.scroll_to(window, cx);
        }
        Self {
            draft,
            scroll,
            choices,
        }
    }
}

impl Popup {
    pub(super) fn icon_content(&self, picker: &Picker, cx: &Context<Self>) -> gpui::Div {
        let draft = &picker.draft;
        let p = self.palette;
        let preview = draft.preview();
        let description = format!("Preview: {}", preview.label);
        let mut choices = div().flex().flex_wrap().gap(px(TILE_GAP));
        for (index, icon) in catalog().iter().enumerate() {
            let id = gpui::SharedString::from(format!("choice-{}", icon.id));
            let selected = draft.selected_index() == index + 1;
            let focus = picker.choices[index]
                .focus
                .clone()
                .tab_stop(icon.id == preview.id);
            let tile = div()
                .id(id.clone())
                .track_focus(&focus)
                .anchor_scroll(Some(picker.choices[index].anchor.clone()))
                .w(px(TILE_WIDTH))
                .h(px(54.))
                .flex()
                .flex_col()
                .gap_1()
                .items_center()
                .justify_center()
                .border_1()
                .border_color(p.stroke)
                .bg(p.layer)
                .when(selected, |tile| {
                    tile.bg(p.selected).border_2().border_color(p.accent)
                })
                .hover(|style| style.bg(p.selected))
                .focus(|style| style.border_2().border_color(p.accent))
                .on_click(
                    cx.listener(move |this, _, window, cx| this.select_sprite(index, window, cx)),
                )
                .on_key_down(
                    cx.listener(move |this, event: &gpui::KeyDownEvent, window, cx| {
                        let total = catalog().len();
                        let columns = (((f32::from(window.viewport_size().width) - 40. + TILE_GAP)
                            / (TILE_WIDTH + TILE_GAP))
                            .floor() as usize)
                            .clamp(1, total);
                        let next = match event.keystroke.key.as_str() {
                            "left" => (index + total - 1) % total,
                            "right" => (index + 1) % total,
                            "up" => {
                                if index < columns {
                                    index + ((total - 1 - index) / columns) * columns
                                } else {
                                    index - columns
                                }
                            }
                            "down" => {
                                if index + columns < total {
                                    index + columns
                                } else {
                                    index % columns
                                }
                            }
                            "home" => 0,
                            "end" => total - 1,
                            "space" => index,
                            _ => return,
                        };
                        this.select_sprite(next, window, cx);
                        if let Some(picker) = &this.icon {
                            picker.choices[next].anchor.scroll_to(window, cx);
                        }
                        cx.stop_propagation();
                    }),
                )
                .child(Icon::Builtin(icon).render(px(24.), p.ink))
                .child(div().text_sm().child(icon.label));
            let weak = cx.weak_entity();
            let semantic = self
                .accessibility
                .button(
                    tile,
                    id.into(),
                    icon.label,
                    Role::RadioButton,
                    false,
                    Some(selected),
                    move |window, cx| {
                        let _ = weak.update(cx, |this, cx| this.select_sprite(index, window, cx));
                    },
                )
                .with_focus(focus);
            choices = choices.child(semantic);
        }
        let automatic = draft.selected_index() == 0;
        let reset = self.control(
            label(
                compact_action("default-icon", p, Partition::Neutral, false, cx),
                "Use default",
                p,
                Partition::Neutral,
                false,
            ),
            "default-icon",
            "Use default",
            Role::Button,
            false,
            None,
            cx,
            |this, _, cx| {
                if let Some(picker) = &mut this.icon {
                    picker.draft.choose(0);
                    cx.notify();
                }
            },
        );
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .size(px(46.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(p.layer)
                            .child(Icon::Builtin(preview).render(px(36.), p.ink)),
                    )
                    .child(self.accessibility.text(
                        div().font_semibold().child(description.clone()),
                        "icon-preview",
                        description,
                        Role::Label,
                    ))
                    .when(automatic, |row| {
                        let text = if draft.unavailable_choice() {
                            "Saved icon unavailable"
                        } else {
                            "Default"
                        };
                        row.child(self.accessibility.text(
                            div().text_sm().text_color(p.muted).child(text),
                            "icon-default",
                            text,
                            Role::Label,
                        ))
                    }),
            )
            .child(self.accessibility.group(
                choices,
                "icon-choices",
                "Built-in instance icons",
                Role::RadioGroup,
            ))
            .child(div().flex().child(reset))
    }

    fn select_sprite(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(picker) = &mut self.icon {
            picker.draft.choose(index + 1);
            picker.choices[index].focus.focus(window);
            cx.notify();
        }
    }

    pub(super) fn save_icon(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(picker) = &self.icon else {
            return;
        };
        let draft = &picker.draft;
        if !draft.changed() {
            return;
        }
        match draft.save(&self.root_path) {
            Ok(saved) => {
                let _ = self.owner.update(cx, |launcher, cx| {
                    if let Some(row) = launcher.profiles.iter_mut().find(|p| p.id == saved.id) {
                        row.icon_id = saved.icon_id;
                    }
                    cx.notify();
                });
                window.remove_window();
            }
            Err(error) => {
                self.error = Some(error);
                cx.notify();
            }
        }
    }
}
