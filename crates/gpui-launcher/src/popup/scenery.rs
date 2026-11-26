// SPDX-License-Identifier: MPL-2.0
use super::*;
use crate::scenery::Landscape;

impl Popup {
    pub(super) fn scene_choices(&self, current: Landscape, cx: &Context<Self>) -> gpui::Div {
        let p = self.palette;
        let owner = self.owner.upgrade();
        let manual_world = owner.as_ref().and_then(|owner| owner.read(cx).artwork.world.clone());
        let mut choices = div().flex().flex_col().gap_1();
        for scene in Landscape::ALL.iter().copied() {
            let selected = current == scene && manual_world.is_none();
            let treatment = if selected { Partition::Selected } else { Partition::Neutral };
            let id = scene.id();
            choices = choices.child(self.control(
                label(compact_action(id, p, treatment, false, cx).justify_start(), scene.name(), p, treatment, false),
                id, scene.name(), Role::RadioButton, false, Some(selected), cx,
                move |this, window, cx| {
                    let _ = this.owner.update(cx, |launcher, cx| {
                        launcher.artwork.world = None;
                        launcher.artwork.playback.select(scene, &launcher.root.path);
                        cx.notify();
                    });
                    this.dismiss(window, cx);
                },
            ));
        }
        if let Some(owner) = owner {
            let launcher = owner.read(cx);
            let enabled = launcher.artwork.playback.personal_worlds;
            let toggle = if enabled { "Exclude local world maps" } else { "Include local world maps" };
            let world_choices = launcher.home_world_choices();
            choices = choices.child(self.control(
                label(compact_action("personal-art-opt-in", p, Partition::Neutral, false, cx), toggle, p, Partition::Neutral, false),
                "personal-art-opt-in", toggle, Role::Button, false, None, cx, move |this, _, cx| {
                    let _ = this.owner.update(cx, |launcher, cx| {
                        launcher.artwork.playback.set_personal_worlds(!enabled, &launcher.root.path);
                        if enabled { launcher.artwork.world = None; }
                        cx.notify();
                    });
                    cx.notify();
                }
            ));
            if enabled && world_choices.is_empty() {
                choices = choices.child(div().py_1().text_sm().text_color(p.muted).child("No recorded local world has a drawable map yet."));
            }
            for (index, (key, name)) in world_choices.into_iter().enumerate() {
                let id = ("personal-map", index);
                let selected = manual_world.as_ref() == Some(&key);
                choices = choices.child(self.control(
                    label(compact_action(id, p, Partition::Neutral, false, cx), name.clone(), p, Partition::Neutral, false),
                    id, &name, Role::RadioButton, false, Some(selected), cx, move |this, window, cx| {
                        let _ = this.owner.update(cx, |launcher, cx| {
                            launcher.artwork.world = Some(key.clone());
                            launcher.artwork.playback.automatic = false;
                            cx.notify();
                        });
                        this.dismiss(window, cx);
                    }
                ));
            }
        }
        choices
    }
}
