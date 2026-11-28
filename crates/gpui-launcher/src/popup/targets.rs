/* SPDX-License-Identifier: MPL-2.0 */
use super::*;
use carbon_profiles::{
    InstanceTargets, TargetAction, TargetKey, change_instance_targets, instance_targets,
};
use gpui::{Div, ElementId, SharedString};

pub(super) struct TargetDialog {
    instance: Uuid,
    snapshot: InstanceTargets,
    form: Option<TargetForm>,
    message: Option<&'static str>,
}

struct TargetForm {
    revision: Option<Uuid>,
    name: Entity<InputState>,
    kind: EditKind,
}

enum EditKind {
    Destination(Option<Uuid>, Entity<InputState>),
    WorldLabel(String),
}

impl TargetDialog {
    pub(super) fn new(instance: Uuid, snapshot: InstanceTargets) -> Self {
        Self {
            instance,
            snapshot,
            form: None,
            message: None,
        }
    }

    pub(super) fn editing(&self) -> bool {
        self.form.is_some()
    }

    pub(super) fn cancel_edit(&mut self) -> bool {
        self.form.take().is_some()
    }
}

impl Popup {
    fn edit_destination(&mut self, id: Option<Uuid>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(targets) = &mut self.targets else {
            return;
        };
        if targets.editing() {
            return;
        }
        let (name, address) = if let Some(id) = id {
            let Some(server) = targets
                .snapshot
                .destinations
                .iter()
                .find(|server| server.id == id)
            else {
                return;
            };
            (server.name.clone(), server.address.to_string())
        } else {
            (String::new(), String::new())
        };
        let name = cx.new(|cx| InputState::new(window, cx).default_value(name));
        let address = cx.new(|cx| InputState::new(window, cx).default_value(address));
        for input in [&name, &address] {
            cx.subscribe_in(input, window, |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.save_target(window, cx);
                }
            })
            .detach();
        }
        name.update(cx, |input, cx| input.focus(window, cx));
        targets.form = Some(TargetForm {
            revision: targets.snapshot.revision,
            name,
            kind: EditKind::Destination(id, address),
        });
        targets.message = None;
        self.error = None;
        cx.notify();
    }

    fn label_world(&mut self, directory: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(targets) = &mut self.targets else {
            return;
        };
        if targets.editing() {
            return;
        }
        let key = TargetKey::World(directory.to_owned());
        let Some(world) = targets
            .snapshot
            .worlds
            .iter()
            .find(|world| world.key == key)
        else {
            return;
        };
        let name = cx.new(|cx| {
            InputState::new(window, cx).default_value(if world.name == directory {
                String::new()
            } else {
                world.name.clone()
            })
        });
        cx.subscribe_in(&name, window, |this, _, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.save_target(window, cx);
            }
        })
        .detach();
        name.update(cx, |input, cx| input.focus(window, cx));
        targets.form = Some(TargetForm {
            revision: targets.snapshot.revision,
            name,
            kind: EditKind::WorldLabel(directory.to_owned()),
        });
        targets.message = None;
        self.error = None;
        cx.notify();
    }

    pub(super) fn save_target(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self
            .targets
            .as_ref()
            .and_then(|targets| targets.form.as_ref())
        else {
            return;
        };
        let name = form.name.read(cx).value();
        let result = match &form.kind {
            EditKind::Destination(id, field) => carbon_profiles::validate_target_label(&name)
                .and_then(|name| {
                    ServerAddress::parse(field.read(cx).value().as_str()).map(|address| {
                        (
                            TargetAction::SaveDestination {
                                id: *id,
                                name: name.to_owned(),
                                address,
                            },
                            "Destination saved.",
                        )
                    })
                }),
            EditKind::WorldLabel(directory) => {
                let label = if name.trim().is_empty() {
                    Ok(None)
                } else {
                    carbon_profiles::validate_target_label(&name)
                        .map(|label| Some(label.to_owned()))
                };
                label.map(|label| {
                    (
                        TargetAction::LabelWorld {
                            directory: directory.clone(),
                            label,
                        },
                        "Save label updated.",
                    )
                })
            }
        };
        match result {
            Ok((action, message)) => {
                self.change_targets(form.revision, action, message, window, cx)
            }
            Err(error) => {
                self.error = Some(error);
                cx.notify();
            }
        }
    }

    fn change_targets(
        &mut self,
        revision: Option<Uuid>,
        action: TargetAction,
        message: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = &mut self.targets else {
            return;
        };
        if dialog.editing()
            && matches!(
                action,
                TargetAction::Select(_)
                    | TargetAction::RemoveDestination(_)
                    | TargetAction::ForgetMissingLabel(_)
            )
        {
            return;
        }
        self.error = None;
        dialog.message = None;
        let removing = matches!(&action, TargetAction::RemoveDestination(_));
        match change_instance_targets(&self.root_path, dialog.instance, revision, action) {
            Ok(snapshot) => {
                dialog.snapshot = snapshot;
                dialog.message = Some(message);
                if dialog.cancel_edit() || removing {
                    self.focus.focus(window);
                }
            }
            Err(error) if dialog.editing() => {
                // Retain the inputs and the revision on which the draft was based. A later
                // save must never silently use a newer revision for the same draft.
                self.error = Some(format!(
                    "{error} Your draft is still here. Cancel edit to review the saved targets."
                ));
            }
            Err(error) => {
                self.error = Some(error);
                if let Ok(snapshot) = instance_targets(&self.root_path, dialog.instance) {
                    dialog.snapshot = snapshot;
                }
            }
        }
        cx.notify();
    }

    pub(super) fn refresh_targets(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = &mut self.targets else {
            return;
        };
        dialog.message = None;
        match instance_targets(&self.root_path, dialog.instance) {
            Ok(snapshot) => {
                dialog.snapshot = snapshot;
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
        cx.notify();
    }

    fn copy_destination(&mut self, id: Uuid, expected: &ServerAddress, cx: &mut Context<Self>) {
        if self.targets.as_ref().is_none_or(TargetDialog::editing) {
            return;
        }
        self.refresh_targets(cx);
        let Some(dialog) = &mut self.targets else {
            return;
        };
        if self.error.is_some() {
            return;
        }
        if !dialog
            .snapshot
            .destinations
            .iter()
            .any(|server| server.id == id && &server.address == expected)
        {
            self.error =
                Some("This destination changed. Review the current address before copying.".into());
            cx.notify();
            return;
        }
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(expected.to_string()));
        dialog.message = Some("Server address copy requested.");
        cx.notify();
    }

    pub(super) fn target_content(
        &self,
        dialog: &TargetDialog,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let p = self.palette;
        let col = div().flex().flex_col().gap_2();
        if let Some(form) = &dialog.form {
            let input = |name, state: &Entity<InputState>| {
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_sm().child(name))
                    .child(self.accessibility.input(
                        Input::new(state).small().bordered(true),
                        name,
                        Role::TextInput,
                        state,
                        false,
                        cx,
                    ))
            };
            return match &form.kind {
                EditKind::Destination(_, address) => col
                    .child(input("Destination name", &form.name))
                    .child(input("Server address", address))
                    .child(div().text_sm().text_color(p.muted)
                        .child("Use a hostname or IP address, with an optional port.")),
                EditKind::WorldLabel(directory) => col
                    .child(self.accessibility.text(
                        div().w_full().min_w_0().text_sm().whitespace_normal()
                            .child(format!("Folder: {directory}")),
                        "edit-world-folder",
                        format!("Folder: {directory}"),
                        Role::Label,
                    ))
                    .child(input("World label", &form.name))
                    .child(div().text_sm().text_color(p.muted)
                        .child("Leave the label blank to show the folder name. This only changes the launcher label.")),
            };
        }

        let revision = dialog.snapshot.revision;
        let list = || div().flex().flex_col().gap_2();
        let world_list = list()
            .children(dialog.snapshot.worlds.iter().filter_map(|world| {
                let TargetKey::World(directory) = &world.key else {
                    return None;
                };
                let detail = (world.name != *directory).then(|| format!("Folder: {directory}"));
                let spoken = option_name(&world.name, detail.as_deref());
                let key = directory.clone();
                Some(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .min_w_0()
                        .child(div().flex_1().min_w_0().child(self.target_option(
                            SharedString::from(format!("world:{directory}")),
                            &world.name,
                            detail.as_deref(),
                            Some(world.key.clone()),
                            dialog,
                            cx,
                        )))
                        .child(self.target_command(
                            SharedString::from(format!("label:{directory}")),
                            "Label…",
                            &format!("Label save {spoken}"),
                            false,
                            cx,
                            move |this, window, cx| this.label_world(&key, window, cx),
                        )),
                )
            }))
            .when_some(dialog.snapshot.world_issue.as_ref(), |list, issue| {
                list.child(self.accessibility.text(
                    div().text_sm().child(issue.clone()),
                    "world-scan-issue",
                    issue.clone(),
                    Role::Status,
                ))
            })
            .when(
                dialog.snapshot.worlds.is_empty() && dialog.snapshot.world_issue.is_none(),
                |list| {
                    list.child(
                        div()
                            .text_sm()
                            .text_color(p.muted)
                            .child("No local saves found."),
                    )
                },
            );
        let missing = &dialog.snapshot.missing_labels;
        let world_list = world_list.when(!missing.is_empty(), |list| {
            list.child(self.accessibility.text(
                div().font_semibold().child("Labels for missing saves"),
                "missing-labels-heading",
                "Labels for missing saves",
                Role::Heading,
            ))
            .children(missing.iter().map(|(directory, label)| {
                let detail = format!("Folder: {directory}");
                let spoken = option_name(label, Some(&detail));
                let key = directory.clone();
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap_1()
                    .child(
                        self.accessibility.text(
                            div()
                                .w_full()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(div().w_full().text_sm().child(label.clone()))
                                .child(div().w_full().text_xs().text_color(p.muted).child(detail)),
                            format!("missing-label:{directory}"),
                            spoken.clone(),
                            Role::Label,
                        ),
                    )
                    .child(self.target_command(
                        SharedString::from(format!("forget-label:{directory}")),
                        "Forget label",
                        &format!("Forget label {spoken}"),
                        false,
                        cx,
                        move |this, window, cx| {
                            this.change_targets(
                                revision,
                                TargetAction::ForgetMissingLabel(key.clone()),
                                "Missing save label removed.",
                                window,
                                cx,
                            )
                        },
                    ))
            }))
        });
        let server_list = list()
            .children(
                dialog
                    .snapshot
                    .destinations
                    .iter()
                    .enumerate()
                    .map(|(index, server)| {
                        let id = server.id;
                        let expected = server.address.clone();
                        let duplicate = dialog.snapshot.destinations.iter().any(|other| {
                            other.id != id
                                && other.name == server.name
                                && other.address == server.address
                        });
                        let detail = if duplicate {
                            format!("Entry {} · Address: {}", index + 1, server.address)
                        } else {
                            format!("Address: {}", server.address)
                        };
                        let spoken = option_name(&server.name, Some(&detail));
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .min_w_0()
                            .border_b_1()
                            .border_color(p.stroke)
                            .pb_2()
                            .child(self.target_option(
                                SharedString::from(format!("destination-{id}")),
                                &server.name,
                                Some(&detail),
                                Some(TargetKey::Server(id)),
                                dialog,
                                cx,
                            ))
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap_2()
                                    .child(self.target_command(
                                        SharedString::from(format!("edit-{id}")),
                                        "Edit",
                                        &format!("Edit {spoken}"),
                                        false,
                                        cx,
                                        move |this, window, cx| {
                                            this.edit_destination(Some(id), window, cx)
                                        },
                                    ))
                                    .child(self.target_command(
                                        SharedString::from(format!("copy-{id}")),
                                        "Copy address",
                                        &format!("Copy address for {spoken}"),
                                        false,
                                        cx,
                                        move |this, _, cx| this.copy_destination(id, &expected, cx),
                                    ))
                                    .child(self.target_command(
                                        SharedString::from(format!("remove-{id}")),
                                        "Remove",
                                        &format!("Remove {spoken}"),
                                        false,
                                        cx,
                                        move |this, window, cx| {
                                            this.change_targets(
                                                revision,
                                                TargetAction::RemoveDestination(id),
                                                "Destination removed.",
                                                window,
                                                cx,
                                            )
                                        },
                                    )),
                            )
                    }),
            )
            .when(dialog.snapshot.destinations.is_empty(), |list| {
                list.child(
                    div()
                        .text_sm()
                        .text_color(p.muted)
                        .child("No saved destinations."),
                )
            });
        col.child(div().text_sm().text_color(p.muted).child(
            "Save a world or server for this instance. Starting Minecraft with a chosen target is unavailable in this build."
        ))
        .when_some(dialog.message, |col, message| {
            col.child(self.accessibility.text(
                div().text_sm().text_color(p.muted).child(message),
                "target-status", message, Role::Status,
            ))
        })
        .child(self.target_option("general-target", "No specific target", None, None, dialog, cx))
        .child(self.accessibility.text(
            div().font_semibold().pt_2().child("Local saves"), "worlds-heading", "Local saves", Role::Heading,
        ))
        .child(world_list)
        .child(self.accessibility.text(
            div().font_semibold().pt_2().child("Saved destinations"), "destinations-heading", "Saved destinations", Role::Heading,
        ))
        .child(server_list)
    }

    pub(super) fn target_actions(
        &self,
        dialog: &TargetDialog,
        bar: Div,
        cx: &Context<Self>,
    ) -> Div {
        if let Some(form) = &dialog.form {
            let name = match form.kind {
                EditKind::Destination(..) => "Save destination",
                EditKind::WorldLabel(_) => "Save label",
            };
            return bar.child(self.target_command(
                "save-target",
                name,
                name,
                true,
                cx,
                Self::save_target,
            ));
        }
        bar.child(self.target_command(
            "refresh-targets",
            "Refresh",
            "Refresh launch targets",
            false,
            cx,
            |this, _, cx| this.refresh_targets(cx),
        ))
        .child(self.target_command(
            "add-destination",
            "Add destination",
            "Add destination",
            true,
            cx,
            |this, window, cx| this.edit_destination(None, window, cx),
        ))
    }

    fn target_option(
        &self,
        id: impl Into<ElementId> + Clone,
        name: &str,
        detail: Option<&str>,
        key: Option<TargetKey>,
        dialog: &TargetDialog,
        cx: &Context<Self>,
    ) -> super::super::accessibility::Semantic {
        let p = self.palette;
        let selected = dialog.snapshot.selected == key;
        let treatment = if selected {
            Partition::Selected
        } else {
            Partition::Neutral
        };
        let revision = dialog.snapshot.revision;
        let spoken = option_name(name, detail);
        let label = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .items_start()
            .child(
                div()
                    .w_full()
                    .text_sm()
                    .truncate()
                    .child(format!("{} {name}", if selected { "●" } else { "○" })),
            )
            .when_some(detail, |label, detail| {
                label.child(
                    div()
                        .w_full()
                        .text_xs()
                        .whitespace_normal()
                        .text_color(p.muted)
                        .child(detail.to_owned()),
                )
            });
        self.control(
            compact_action(id.clone(), p, treatment, false, cx)
                .justify_start()
                .w_full()
                .h_auto()
                .min_h(px(32.))
                .overflow_hidden()
                .py_1()
                .border_color(if selected { p.accent } else { p.stroke })
                .child(label),
            id,
            &spoken,
            Role::RadioButton,
            false,
            Some(selected),
            cx,
            move |this, window, cx| {
                this.change_targets(
                    revision,
                    TargetAction::Select(key.clone()),
                    "Target choice saved.",
                    window,
                    cx,
                )
            },
        )
    }

    fn target_command(
        &self,
        id: impl Into<ElementId> + Clone,
        label_text: &'static str,
        name: &str,
        primary: bool,
        cx: &Context<Self>,
        f: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> super::super::accessibility::Semantic {
        let p = self.palette;
        let treatment = if primary {
            Partition::Primary
        } else {
            Partition::Neutral
        };
        self.control(
            label(
                compact_action(id.clone(), p, treatment, false, cx),
                label_text,
                p,
                treatment,
                false,
            ),
            id,
            name,
            Role::Button,
            false,
            None,
            cx,
            f,
        )
    }
}

fn option_name(name: &str, detail: Option<&str>) -> String {
    match detail {
        Some(detail) => format!("{name}\n{detail}"),
        None => name.to_owned(),
    }
}
