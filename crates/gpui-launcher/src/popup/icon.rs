// SPDX-License-Identifier: MPL-2.0
use super::*;
use crate::icons::Icon;
use carbon_profiles::icons::{IconDraft, catalog};

impl Popup {
    pub(super) fn icon_content(&self, draft: &IconDraft, cx: &Context<Self>) -> gpui::Div {
        let p = self.palette;
        let preview = draft.preview();
        let description = format!("Preview: {}", preview.label);
        let mut choices = div().flex().flex_wrap().gap_2();
        for (index, icon) in catalog().iter().enumerate() {
            let index = index + 1;
            let id = gpui::SharedString::from(format!("choice-{}", icon.id));
            let selected = draft.selected_index() == index;
            let treatment = if selected {
                Partition::Selected
            } else {
                Partition::Neutral
            };
            let button = compact_action(id.clone(), p, treatment, false, cx)
                .w(px(95.))
                .h(px(46.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_1()
                        .child(Icon::Builtin(icon).render(px(16.), p.ink))
                        .child(icon.label.clone()),
                );
            choices = choices.child(self.control(
                button,
                id,
                &icon.label,
                Role::RadioButton,
                false,
                Some(selected),
                cx,
                move |this, _, cx| this.choose_icon(index, cx),
            ));
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
            |this, _, cx| this.choose_icon(0, cx),
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
                            .child(Icon::Builtin(preview).render(px(30.), p.ink)),
                    )
                    .child(self.accessibility.text(
                        div().font_semibold().child(description.clone()),
                        "icon-preview",
                        description,
                        Role::Label,
                    ))
                    .when(automatic, |row| {
                        row.child(div().text_sm().text_color(p.muted).child("Default"))
                    }),
            )
            .child(choices)
            .child(div().flex().child(reset))
    }

    fn choose_icon(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(draft) = &mut self.icon {
            draft.choose(index);
            cx.notify();
        }
    }

    pub(super) fn save_icon(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = &self.icon else {
            return;
        };
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
