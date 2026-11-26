/* SPDX-License-Identifier: MPL-2.0 */
//! Native Home quick switcher for instances and their saved destinations.
use super::*;
use crate::TargetsState;
use crate::accessibility;
use crate::icons::Icon;
use crate::theme::Palette;
use carbon_profiles::{InstanceTargets, Profile, TargetKey};
use gpui::{FocusHandle, ScrollHandle, SharedString, Subscription};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq)]
struct InstanceRow {
    id: Uuid,
    name: String,
    kind: String,
    icon: &'static str,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct TargetList {
    revision: Option<Uuid>,
    world_issue: Option<String>,
    worlds: Vec<(String, String)>,
    servers: Vec<(Uuid, String, String)>,
    selected: Option<TargetKey>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum TargetsSnapshot {
    #[default]
    None,
    Bound,
    Loading,
    Ready(TargetList),
    Error(String),
}

/// The popup never reads live filesystem state of its own. This is a small,
/// comparable copy of the owner's current Home data used for rendering.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HomeSnapshot {
    profiles: Vec<InstanceRow>,
    selected: Option<Uuid>,
    targets: TargetsSnapshot,
    issue: Option<String>,
}

impl HomeSnapshot {
    pub(super) fn read(launcher: &Launcher) -> Self {
        let by_id: HashMap<_, _> = launcher
            .profiles
            .iter()
            .map(|profile| (profile.id, profile))
            .collect();
        let profiles = launcher
            .home_order
            .ordered_ids(&launcher.profiles)
            .into_iter()
            .filter_map(|id| by_id.get(&id).map(|profile| InstanceRow::new(profile)))
            .collect();
        let selected_profile = launcher.selected_profile();
        let targets = match (launcher.personal, selected_profile) {
            (None, _) | (Some(_), None) => TargetsSnapshot::None,
            (Some(_), Some(profile)) if profile.server_address.is_some() => TargetsSnapshot::Bound,
            (Some(id), Some(_)) => match launcher.targets.get(&id) {
                Some(TargetsState::Ready(snapshot)) => {
                    TargetsSnapshot::Ready(TargetList::from(snapshot))
                }
                Some(TargetsState::Error(error)) => TargetsSnapshot::Error(error.clone()),
                Some(TargetsState::Loading(_)) | None => TargetsSnapshot::Loading,
            },
        };
        Self {
            profiles,
            selected: launcher
                .personal
                .filter(|id| launcher.profiles.iter().any(|profile| profile.id == *id)),
            targets,
            issue: launcher.profile_issue.clone(),
        }
    }
}

impl InstanceRow {
    fn new(profile: &Profile) -> Self {
        let icon = carbon_profiles::icons::displayed(profile);
        let mut kind = match &profile.imported_pack {
            Some(pack) => match &pack.loader {
                Some(loader) => format!("Minecraft {} · {loader}", profile.version.id()),
                None => format!("Minecraft {} · Modpack", profile.version.id()),
            },
            None => format!("Minecraft {} · Vanilla", profile.version.id()),
        };
        if profile.server_address.is_some() {
            kind.push_str(" · Saved server");
        }
        Self {
            id: profile.id,
            name: profile.name.clone(),
            kind,
            icon: icon.id,
        }
    }
}

impl From<&InstanceTargets> for TargetList {
    fn from(snapshot: &InstanceTargets) -> Self {
        Self {
            revision: snapshot.revision,
            world_issue: snapshot.world_issue.clone(),
            worlds: snapshot
                .worlds
                .iter()
                .filter_map(|world| match &world.key {
                    TargetKey::World(folder) => Some((folder.clone(), world.name.clone())),
                    TargetKey::Server(_) => None,
                })
                .collect(),
            servers: snapshot
                .destinations
                .iter()
                .map(|server| (server.id, server.name.clone(), server.address.to_string()))
                .collect(),
            selected: snapshot.selected.clone(),
        }
    }
}

pub(super) struct HomeTargets {
    snapshot: HomeSnapshot,
    pub(super) scroll: ScrollHandle,
    close_focus: FocusHandle,
    instance_focus: HashMap<Uuid, FocusHandle>,
    target_focus: HashMap<String, FocusHandle>,
    message: Option<String>,
    committed: bool,
    _changes: Option<Subscription>,
}

impl HomeTargets {
    pub(super) fn new(
        snapshot: HomeSnapshot,
        owner: Option<Entity<Launcher>>,
        window: &mut Window,
        cx: &mut Context<Popup>,
    ) -> Self {
        let mut dialog = Self {
            snapshot,
            scroll: ScrollHandle::new(),
            close_focus: cx.focus_handle().tab_stop(true),
            instance_focus: HashMap::new(),
            target_focus: HashMap::new(),
            message: None,
            committed: false,
            _changes: None,
        };
        dialog.reconcile_focus(cx);
        dialog.focus_initial(window);
        dialog._changes = owner.map(|owner| {
            cx.observe(&owner, |this, owner, cx| {
                let snapshot = HomeSnapshot::read(owner.read(cx));
                if let Some(home) = &mut this.home
                    && home.refresh(snapshot, cx)
                {
                    cx.notify();
                }
            })
        });
        dialog
    }

    fn refresh(&mut self, snapshot: HomeSnapshot, cx: &mut Context<Popup>) -> bool {
        if self.snapshot == snapshot {
            return false;
        }
        self.snapshot = snapshot;
        self.reconcile_focus(cx);
        true
    }

    /// Keep one caller-owned focus handle per stable row id so Tab order and
    /// explicit initial focus survive target loading and owner refreshes.
    fn reconcile_focus(&mut self, cx: &mut Context<Popup>) {
        let instances: HashSet<Uuid> = self.snapshot.profiles.iter().map(|row| row.id).collect();
        self.instance_focus.retain(|id, _| instances.contains(id));
        for row in &self.snapshot.profiles {
            self.instance_focus
                .entry(row.id)
                .or_insert_with(|| cx.focus_handle().tab_stop(true));
        }

        let mut targets = HashSet::new();
        if let TargetsSnapshot::Ready(list) = &self.snapshot.targets {
            if list.world_issue.is_none() {
                for (folder, _) in &list.worlds {
                    targets.insert(format!("world:{folder}"));
                }
            }
            for (id, _, _) in &list.servers {
                targets.insert(format!("server:{id}"));
            }
        }
        self.target_focus.retain(|id, _| targets.contains(id));
        for id in targets {
            self.target_focus
                .entry(id)
                .or_insert_with(|| cx.focus_handle().tab_stop(true));
        }
    }

    fn focus_initial(&self, window: &mut Window) {
        let preferred = self
            .snapshot
            .selected
            .and_then(|id| self.instance_focus.get(&id))
            .or_else(|| {
                self.snapshot
                    .profiles
                    .first()
                    .and_then(|row| self.instance_focus.get(&row.id))
            })
            .unwrap_or(&self.close_focus);
        preferred.focus(window);
    }

    fn focus_for_target(&self, key: &TargetKey) -> FocusHandle {
        let id = match key {
            TargetKey::World(folder) => format!("world:{folder}"),
            TargetKey::Server(server) => format!("server:{server}"),
        };
        self.target_focus
            .get(&id)
            .cloned()
            .unwrap_or_else(|| self.close_focus.clone())
    }
}

fn target_present(snapshot: &InstanceTargets, key: &TargetKey) -> bool {
    match key {
        TargetKey::World(_) if snapshot.world_issue.is_some() => false,
        TargetKey::World(folder) => snapshot
            .worlds
            .iter()
            .any(|world| matches!(&world.key, TargetKey::World(saved) if saved == folder)),
        TargetKey::Server(id) => snapshot.destinations.iter().any(|server| server.id == *id),
    }
}

impl Popup {
    pub(super) fn choose_instance(
        &mut self,
        id: Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.home.as_ref().is_some_and(|home| home.committed) {
            return;
        }
        let result = self.owner.update(cx, |launcher, cx| {
            if !launcher.profiles.iter().any(|profile| profile.id == id) {
                return Err("That instance is no longer available. Choose another.".to_owned());
            }
            launcher.home_select_instance(id, cx);
            Ok(())
        });
        match result {
            Ok(Ok(())) => {
                if let Some(home) = &mut self.home {
                    home.committed = true;
                }
                self.dismiss(window, cx);
            }
            Ok(Err(message)) => self.home_error(message, cx),
            Err(_) => self.home_error("Could not update the selected instance.".into(), cx),
        }
    }

    fn choose_target(
        &mut self,
        id: Uuid,
        key: TargetKey,
        rendered_revision: Option<Uuid>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.home.as_ref().is_some_and(|home| home.committed) {
            return;
        }
        let request = Uuid::new_v4();
        let started = self.owner.update(cx, |launcher, cx| {
            if launcher.personal != Some(id)
                || launcher
                    .selected_profile()
                    .is_none_or(|p| p.server_address.is_some())
            {
                return Err("The active instance changed. Choose a destination again.");
            }
            let Some(TargetsState::Ready(snapshot)) = launcher.targets.get(&id) else {
                return Err("Destinations are still loading. Choose again in a moment.");
            };
            if snapshot.revision != rendered_revision || !target_present(snapshot, &key) {
                return Err(
                    "The destinations changed. Review the current choices before selecting.",
                );
            }
            launcher.targets.insert(id, TargetsState::Loading(request));
            cx.notify();
            Ok(())
        });
        match started {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                self.home_error(error.into(), cx);
                return;
            }
            Err(_) => {
                self.home_error("The launcher is unavailable.".into(), cx);
                return;
            }
        }
        if let Some(home) = &mut self.home {
            home.committed = true;
            home.message = None;
        }
        let path = self.root_path.clone();
        let retry_key = key.clone();
        let work = cx.background_executor().spawn(async move {
            match carbon_profiles::change_instance_targets(
                &path,
                id,
                rendered_revision,
                carbon_profiles::TargetAction::Select(Some(key)),
            ) {
                Ok(snapshot) => (Ok(snapshot), None),
                Err(error) => (carbon_profiles::instance_targets(&path, id), Some(error)),
            }
        });
        let owner = self.owner.clone();
        cx.spawn_in(window, async move |popup, cx| {
            let (fresh, error) = work.await;
            let applied = owner
                .update(cx, |launcher, cx| {
                    let current = launcher
                        .targets
                        .get_mut(&id)
                        .is_some_and(|state| state.complete(request, fresh));
                    if current {
                        if launcher.personal == Some(id) {
                            launcher.message = error
                                .clone()
                                .unwrap_or_else(|| "Destination selected.".into());
                        }
                        cx.notify();
                    }
                    current
                })
                .unwrap_or(false);
            // A closed picker is a different entity from a later opening.
            let _ = popup.update_in(cx, |this, window, cx| {
                if let Some(home) = &mut this.home {
                    home.committed = false;
                }
                if let Some(error) = error {
                    if let Some(owner) = this.owner.upgrade()
                        && let Some(home) = &mut this.home
                    {
                        home.refresh(HomeSnapshot::read(owner.read(cx)), cx);
                        home.focus_for_target(&retry_key).focus(window);
                    }
                    this.home_error(error, cx);
                } else if applied {
                    this.dismiss(window, cx);
                } else {
                    this.home_error(
                        "The active instance changed. Review the current choices.".into(),
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    fn home_error(&mut self, message: String, cx: &mut Context<Self>) {
        if let Some(home) = &mut self.home {
            home.message = Some(message);
        }
        cx.notify();
    }

    pub(super) fn home_target_content(
        &self,
        dialog: &HomeTargets,
        cx: &Context<Self>,
    ) -> gpui::Div {
        let p = self.palette;
        let snapshot = &dialog.snapshot;
        let mut content = div().flex().flex_col().gap_2();
        content = content.child(
            self.accessibility.text(
                div()
                    .text_sm()
                    .text_color(p.muted)
                    .child("Choose the instance or destination to use next time you play."),
                "home-targets-intro",
                "Choose the instance or destination to use next time you play.",
                Role::Label,
            ),
        );
        if let Some(issue) = &snapshot.issue {
            content = content.child(
                self.accessibility.text(
                    div()
                        .bg(p.layer)
                        .p_2()
                        .text_sm()
                        .text_color(p.warning)
                        .child(issue.clone()),
                    "home-instance-issue",
                    issue.clone(),
                    Role::Alert,
                ),
            );
        }
        if let Some(message) = &dialog.message {
            content = content.child(
                self.accessibility.text(
                    div()
                        .bg(p.layer)
                        .p_2()
                        .text_sm()
                        .text_color(p.warning)
                        .child(message.clone()),
                    "home-targets-message",
                    message.clone(),
                    Role::Alert,
                ),
            );
        }

        content = content.child(self.home_heading("Instances", "home-instances-heading"));
        if snapshot.profiles.is_empty() {
            content = content.child(self.home_muted(
                "No instances are saved yet. Create or import an instance first.",
                "home-instances-empty",
            ));
        } else {
            let mut list = div().flex().flex_col().gap_1();
            for row in &snapshot.profiles {
                list = list.child(self.home_instance_option(row, dialog, cx));
            }
            content = content.child(list);
        }

        let Some(active) = snapshot
            .selected
            .and_then(|id| snapshot.profiles.iter().find(|row| row.id == id))
        else {
            return content.child(self.home_muted(
                "Select an instance to choose a destination.",
                "home-targets-select",
            ));
        };
        content = content.child(self.home_heading("Destinations", "home-targets-heading"));
        let mut section = div().flex().flex_col().gap_1();
        match &snapshot.targets {
            TargetsSnapshot::Bound => {
                section = section.child(self.home_muted(
                    "This instance uses its saved server address. Destinations are not used.",
                    "home-targets-bound",
                ));
            }
            TargetsSnapshot::Loading | TargetsSnapshot::None => {
                section = section.child(
                    self.accessibility.text(
                        div()
                            .py_1()
                            .text_sm()
                            .text_color(p.muted)
                            .child("Reading local saves and saved destinations…"),
                        "home-targets-loading",
                        "Reading local saves and saved destinations",
                        Role::Status,
                    ),
                );
            }
            TargetsSnapshot::Error(error) => {
                section = section.child(
                    self.accessibility.text(
                        div()
                            .py_1()
                            .text_sm()
                            .text_color(p.warning)
                            .child(format!("{error} Close and reopen the switcher to retry.")),
                        "home-targets-error",
                        error.clone(),
                        Role::Alert,
                    ),
                );
            }
            TargetsSnapshot::Ready(list) => {
                section =
                    section.child(self.home_subheading("Local saves", "home-worlds-heading", p));
                if let Some(issue) = &list.world_issue {
                    section = section.child(
                        self.accessibility.text(
                            div()
                                .py_1()
                                .text_sm()
                                .text_color(p.warning)
                                .child(issue.clone()),
                            "home-worlds-scan-status",
                            issue.clone(),
                            Role::Alert,
                        ),
                    );
                } else if list.worlds.is_empty() {
                    section = section.child(
                        self.home_muted("No local saves for this instance.", "home-worlds-empty"),
                    );
                } else {
                    let mut worlds = div().flex().flex_col().gap_1();
                    for (folder, name) in &list.worlds {
                        let key = TargetKey::World(folder.clone());
                        let selected = list.selected.as_ref() == Some(&key);
                        let detail = (folder != name).then(|| folder.clone());
                        worlds = worlds.child(self.home_target_option(
                            SharedString::from(format!("switcher-world-{folder}")),
                            name.clone(),
                            detail,
                            key,
                            active.id,
                            list.revision,
                            selected,
                            dialog,
                            cx,
                        ));
                    }
                    section = section.child(worlds);
                }

                section =
                    section.child(self.home_subheading("Saved servers", "home-servers-heading", p));
                if list.servers.is_empty() {
                    section =
                        section.child(self.home_muted(
                            "No saved servers for this instance.",
                            "home-servers-empty",
                        ));
                } else {
                    let mut servers = div().flex().flex_col().gap_1();
                    for (id, name, address) in &list.servers {
                        let key = TargetKey::Server(*id);
                        let selected = list.selected.as_ref() == Some(&key);
                        servers = servers.child(self.home_target_option(
                            SharedString::from(format!("switcher-server-{id}")),
                            name.clone(),
                            Some(address.clone()),
                            key,
                            active.id,
                            list.revision,
                            selected,
                            dialog,
                            cx,
                        ));
                    }
                    section = section.child(servers);
                }
            }
        }
        content.child(section)
    }

    fn home_instance_option(
        &self,
        row: &InstanceRow,
        dialog: &HomeTargets,
        cx: &Context<Self>,
    ) -> accessibility::Semantic {
        let p = self.palette;
        let selected = dialog.snapshot.selected == Some(row.id);
        let focus = dialog
            .instance_focus
            .get(&row.id)
            .cloned()
            .unwrap_or_else(|| dialog.close_focus.clone());
        let icon = carbon_profiles::icons::catalog()
            .iter()
            .find(|icon| icon.id == row.icon)
            .unwrap_or(&carbon_profiles::icons::catalog()[0]);
        let visual = div()
            .flex()
            .items_center()
            .gap_2()
            .min_w_0()
            .child(home_mark(p, Icon::Builtin(icon)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .items_start()
                    .child(
                        div()
                            .w_full()
                            .truncate()
                            .font_semibold()
                            .child(row.name.clone()),
                    )
                    .child(
                        div()
                            .w_full()
                            .truncate()
                            .text_xs()
                            .text_color(p.muted)
                            .child(row.kind.clone()),
                    ),
            );
        let spoken = format!(
            "{}, {}{}",
            row.name,
            row.kind,
            if selected { ", current" } else { "" }
        );
        let id = SharedString::from(format!("switcher-instance-{}", row.id));
        let instance = row.id;
        self.home_choice(
            id,
            visual,
            &spoken,
            selected,
            focus,
            cx,
            move |this, window, cx| this.choose_instance(instance, window, cx),
        )
        .within_scroller(&dialog.scroll)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Keep a target's visual row, spoken name, focus and commit together."
    )]
    fn home_target_option(
        &self,
        id: SharedString,
        name: String,
        detail: Option<String>,
        key: TargetKey,
        instance: Uuid,
        revision: Option<Uuid>,
        selected: bool,
        dialog: &HomeTargets,
        cx: &Context<Self>,
    ) -> accessibility::Semantic {
        let p = self.palette;
        let focus = dialog.focus_for_target(&key);
        let mut visual = div()
            .flex()
            .items_center()
            .gap_2()
            .min_w_0()
            .child(home_mark(p, Icon::Instance))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .items_start()
                    .child(div().w_full().truncate().child(name.clone()))
                    .when_some(detail.clone(), |col, detail| {
                        col.child(
                            div()
                                .w_full()
                                .truncate()
                                .text_xs()
                                .text_color(p.muted)
                                .child(detail),
                        )
                    }),
            );
        if selected {
            visual = visual.child(
                div()
                    .flex_none()
                    .text_xs()
                    .text_color(p.muted)
                    .child("Current"),
            );
        }
        let spoken = match &detail {
            Some(detail) => format!(
                "{name}, {detail}{}",
                if selected { ", current" } else { "" }
            ),
            None => format!("{name}{}", if selected { ", current" } else { "" }),
        };
        self.home_choice(
            id,
            visual,
            &spoken,
            selected,
            focus,
            cx,
            move |this, window, cx| this.choose_target(instance, key.clone(), revision, window, cx),
        )
        .within_scroller(&dialog.scroll)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Reuse one accessible native row control for both lists."
    )]
    fn home_choice(
        &self,
        id: SharedString,
        visual: impl IntoElement,
        name: &str,
        selected: bool,
        focus: FocusHandle,
        cx: &Context<Self>,
        f: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> accessibility::Semantic {
        let p = self.palette;
        let f = Rc::new(f);
        let click = f.clone();
        let key = f.clone();
        let weak = cx.weak_entity();
        let row = div()
            .id(id.clone())
            .track_focus(&focus)
            .w_full()
            .min_h(px(42.))
            .px_2()
            .py_1()
            .flex()
            .items_center()
            .gap_2()
            .border_1()
            .border_color(if selected { p.accent } else { p.stroke })
            .bg(if selected { p.selected } else { p.layer })
            .when(selected, |row| row.border_2())
            .hover(|style| style.bg(p.selected))
            .focus(|style| style.border_color(p.accent))
            .on_click(cx.listener(move |this, _, window, cx| click(this, window, cx)))
            .on_key_down(
                cx.listener(move |this, event: &gpui::KeyDownEvent, window, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        key(this, window, cx);
                        cx.stop_propagation();
                    }
                }),
            )
            .child(visual);
        self.accessibility
            .button(
                row,
                id.into(),
                name,
                Role::RadioButton,
                false,
                Some(selected),
                move |window, cx| {
                    let _ = weak.update(cx, |this, cx| f(this, window, cx));
                },
            )
            .with_focus(focus)
    }

    pub(super) fn home_close_control(
        &self,
        dialog: &HomeTargets,
        cx: &Context<Self>,
    ) -> accessibility::Semantic {
        let p = self.palette;
        let id = SharedString::from("close");
        let focus = dialog.close_focus.clone();
        let weak = cx.weak_entity();
        let button = div()
            .id(id.clone())
            .track_focus(&focus)
            .h(px(32.))
            .px_3()
            .flex()
            .items_center()
            .justify_center()
            .border_1()
            .border_color(p.stroke)
            .bg(p.layer)
            .text_color(p.ink())
            .hover(|style| style.bg(p.selected))
            .focus(|style| style.border_color(p.accent))
            .on_click(cx.listener(|this, _, window, cx| this.dismiss(window, cx)))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    this.dismiss(window, cx);
                    cx.stop_propagation();
                }
            }))
            .child("Close");
        self.accessibility
            .button(
                button,
                id.into(),
                "Close",
                Role::Button,
                false,
                None,
                move |window, cx| {
                    let _ = weak.update(cx, |this, cx| this.dismiss(window, cx));
                },
            )
            .with_focus(focus)
    }

    fn home_heading(&self, text: &'static str, id: &'static str) -> accessibility::Semantic {
        self.accessibility.text(
            div().pt_2().font_semibold().child(text),
            id,
            text,
            Role::Heading,
        )
    }

    fn home_subheading(
        &self,
        text: &'static str,
        id: &'static str,
        p: Palette,
    ) -> accessibility::Semantic {
        self.accessibility.text(
            div()
                .pt_1()
                .text_sm()
                .font_semibold()
                .text_color(p.muted)
                .child(text),
            id,
            text,
            Role::Heading,
        )
    }

    fn home_muted(&self, text: &'static str, id: &'static str) -> accessibility::Semantic {
        self.accessibility.text(
            div()
                .py_1()
                .text_sm()
                .text_color(self.palette.muted)
                .child(text),
            id,
            text,
            Role::Label,
        )
    }
}

fn home_mark(p: Palette, icon: Icon) -> gpui::Div {
    div()
        .flex_none()
        .size(px(26.))
        .bg(p.surface)
        .border_1()
        .border_color(p.stroke)
        .flex()
        .items_center()
        .justify_center()
        .child(icon.render(px(16.), p.ink))
}

#[cfg(test)]
mod tests {
    use super::*;
    use carbon_profiles::{LocalWorld, SavedDestination, ServerAddress};

    fn snapshot(server: Uuid) -> InstanceTargets {
        InstanceTargets {
            world_issue: None,
            missing_labels: Default::default(),
            revision: None,
            worlds: vec![LocalWorld {
                key: TargetKey::World("alpha".into()),
                name: "Alpha".into(),
            }],
            destinations: vec![SavedDestination {
                id: server,
                name: "SMP".into(),
                address: ServerAddress::parse("play.example.org:25565").unwrap(),
            }],
            selected: None,
        }
    }

    #[test]
    fn an_incomplete_scan_exposes_its_error_and_keeps_servers_selectable() {
        let id = Uuid::new_v4();
        let mut snapshot = snapshot(id);
        snapshot.revision = Some(Uuid::new_v4());
        snapshot.world_issue = Some("Unable to read all saves".into());
        let displayed = TargetList::from(&snapshot);
        assert_eq!(displayed.world_issue, snapshot.world_issue);
        assert_eq!(displayed.revision, snapshot.revision);
        assert!(!target_present(
            &snapshot,
            &TargetKey::World("alpha".into())
        ));
        assert!(target_present(&snapshot, &TargetKey::Server(id)));
    }

    #[test]
    fn a_stale_destination_is_rejected_before_commit() {
        let server = Uuid::new_v4();
        let targets = snapshot(server);
        assert!(target_present(&targets, &TargetKey::World("alpha".into())));
        assert!(target_present(&targets, &TargetKey::Server(server)));
        assert!(!target_present(
            &targets,
            &TargetKey::World("removed".into())
        ));
        assert!(!target_present(
            &targets,
            &TargetKey::Server(Uuid::new_v4())
        ));
    }
}
