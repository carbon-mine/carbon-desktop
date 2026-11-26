// SPDX-License-Identifier: MPL-2.0
use super::*;
use crate::scenery::Landscape;

pub(crate) struct ScenePicker {
    current: Landscape,
    enabled: bool,
    world: Option<carbon_profiles::world_preview::EntryKey>,
    choices: Vec<(carbon_profiles::world_preview::EntryKey, String)>,
}
impl ScenePicker {
    pub(crate) fn snapshot(launcher: &Launcher) -> Self {
        Self {
            current: launcher.artwork.playback.scene,
            enabled: launcher.artwork.playback.personal_worlds,
            world: launcher.artwork.world.clone(),
            choices: launcher.home_world_choices(),
        }
    }
}
impl Popup {
    pub(super) fn scene_choices(&self, picker: &ScenePicker, cx: &Context<Self>) -> gpui::Div {
        let p = self.palette;
        let manual_world = &picker.world;
        let mut choices = div().flex().flex_col().gap_1();
        for scene in Landscape::ALL.iter().copied() {
            let selected = picker.current == scene && manual_world.is_none();
            let treatment = if selected {
                Partition::Selected
            } else {
                Partition::Neutral
            };
            let id = scene.id();
            choices = choices.child(self.control(
                label(
                    compact_action(id, p, treatment, false, cx).justify_start(),
                    scene.name(),
                    p,
                    treatment,
                    false,
                ),
                id,
                scene.name(),
                Role::RadioButton,
                false,
                Some(selected),
                cx,
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
        {
            let enabled = picker.enabled;
            let toggle = if enabled {
                "Exclude local world maps"
            } else {
                "Include local world maps"
            };
            let world_choices = &picker.choices;
            choices = choices.child(self.control(
                label(
                    compact_action("personal-art-opt-in", p, Partition::Neutral, false, cx),
                    toggle,
                    p,
                    Partition::Neutral,
                    false,
                ),
                "personal-art-opt-in",
                toggle,
                Role::Button,
                false,
                None,
                cx,
                move |this, _, cx| {
                    let snapshot = this.owner.update(cx, |launcher, cx| {
                        launcher
                            .artwork
                            .playback
                            .set_personal_worlds(!enabled, &launcher.root.path);
                        if enabled {
                            launcher.artwork.world = None;
                        }
                        cx.notify();
                        ScenePicker::snapshot(launcher)
                    });
                    if let Ok(snapshot) = snapshot {
                        this.scene = Some(snapshot);
                    }
                    cx.notify();
                },
            ));
            if enabled && world_choices.is_empty() {
                choices = choices.child(
                    div()
                        .py_1()
                        .text_sm()
                        .text_color(p.muted)
                        .child("No recorded local world has a drawable map yet."),
                );
            }
            for (index, (key, name)) in world_choices.iter().enumerate() {
                let id = ("personal-map", index);
                let selected = manual_world.as_ref() == Some(key);
                let key = key.clone();
                choices = choices.child(self.control(
                    label(
                        compact_action(id, p, Partition::Neutral, false, cx),
                        name.clone(),
                        p,
                        Partition::Neutral,
                        false,
                    ),
                    id,
                    name,
                    Role::RadioButton,
                    false,
                    Some(selected),
                    cx,
                    move |this, window, cx| {
                        let _ = this.owner.update(cx, |launcher, cx| {
                            launcher.artwork.world = Some(key.clone());
                            launcher.artwork.playback.automatic = false;
                            cx.notify();
                        });
                        this.dismiss(window, cx);
                    },
                ));
            }
        }
        choices
    }
}
