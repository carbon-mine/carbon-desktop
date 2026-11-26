// SPDX-License-Identifier: MPL-2.0
//! Native Home destination: first screen, shelf, ordering and quick switcher.
use super::view::{col, row};
use super::*;
use accesskit::Role;
use carbon_profiles::{Profile, TargetKey};
use gpui::SharedString;
use gpui::{AnyElement, div, img, px};
use gpui_component::StyledExt;
use icons::Icon;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::Path};
use theme::{Palette, Partition, compact_action};

const ORDER_FILE: &str = "home-order.json";
/// Two capped UUID lists need about 10 KiB, so the shared 4 KiB preference cap
/// would discard a full order and silently clear pins and recents.
const ORDER_FILE_LIMIT: u64 = 32 * 1024;
const ORDER_VERSION: u32 = 1;
const MAX_PINNED: usize = 128;
const MAX_RECENT: usize = 128;

/// Pinned profiles come first, then most recently opened profiles, then the
/// rest of the saved library. The file is written atomically beside the other
/// shell preferences and never contains a filesystem path.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct HomeOrder {
    #[serde(default = "order_version")]
    version: u32,
    #[serde(default)]
    pinned: Vec<Uuid>,
    #[serde(default)]
    recent: Vec<Uuid>,
}

const fn order_version() -> u32 {
    ORDER_VERSION
}

impl Default for HomeOrder {
    fn default() -> Self {
        Self {
            version: ORDER_VERSION,
            pinned: Vec::new(),
            recent: Vec::new(),
        }
    }
}

impl HomeOrder {
    pub(crate) fn load(root: &Path) -> Self {
        let mut order =
            shell::read_at_most::<Self>(root, ORDER_FILE, ORDER_FILE_LIMIT).unwrap_or_default();
        if order.version != ORDER_VERSION {
            return Self::default();
        }
        order.sanitize();
        order
    }

    pub(crate) fn save(&self, root: &Path) -> std::io::Result<()> {
        shell::write(root, ORDER_FILE, self)
    }

    pub(crate) fn is_pinned(&self, id: Uuid) -> bool {
        self.pinned.contains(&id)
    }

    pub(crate) fn toggle_pin(&mut self, id: Uuid) -> bool {
        if let Some(index) = self.pinned.iter().position(|candidate| *candidate == id) {
            self.pinned.remove(index);
            false
        } else {
            self.pinned.push(id);
            self.pinned.truncate(MAX_PINNED);
            true
        }
    }

    pub(crate) fn touch_recent(&mut self, id: Uuid) -> bool {
        let previous = self.recent.clone();
        self.recent.retain(|candidate| *candidate != id);
        self.recent.insert(0, id);
        self.recent.truncate(MAX_RECENT);
        self.recent != previous
    }

    pub(crate) fn retain_profiles(&mut self, profiles: &[Profile]) {
        let known: HashSet<_> = profiles.iter().map(|profile| profile.id).collect();
        self.pinned.retain(|id| known.contains(id));
        self.recent.retain(|id| known.contains(id));
    }

    pub(crate) fn ordered_ids(&self, profiles: &[Profile]) -> Vec<Uuid> {
        let known: HashSet<_> = profiles.iter().map(|profile| profile.id).collect();
        let mut seen = HashSet::new();
        let mut ordered = Vec::with_capacity(profiles.len());
        for id in self.pinned.iter().chain(self.recent.iter()) {
            if known.contains(id) && seen.insert(*id) {
                ordered.push(*id);
            }
        }
        for profile in profiles {
            if seen.insert(profile.id) {
                ordered.push(profile.id);
            }
        }
        ordered
    }

    fn sanitize(&mut self) {
        fn unique(ids: &mut Vec<Uuid>, limit: usize) {
            let mut seen = HashSet::new();
            ids.retain(|id| seen.insert(*id));
            ids.truncate(limit);
        }
        unique(&mut self.pinned, MAX_PINNED);
        unique(&mut self.recent, MAX_RECENT);
    }
}

impl Launcher {
    pub(super) fn surface_tabs(&self, p: Palette, cx: &Context<Self>) -> AnyElement {
        row()
            .gap_1()
            .child(self.surface_tab(p, Surface::Home, "Home", cx))
            .child(self.surface_tab(p, Surface::Library, "Library", cx))
            .into_any_element()
    }

    fn surface_tab(
        &self,
        p: Palette,
        surface: Surface,
        name: &'static str,
        cx: &Context<Self>,
    ) -> accessibility::Semantic {
        let selected = self.surface == surface;
        let id = SharedString::from(format!("surface-{name}"));
        let button = compact_action(
            id.clone(),
            p,
            if selected {
                Partition::Selected
            } else {
                Partition::Neutral
            },
            false,
            cx,
        )
        .min_w(px(72.))
        .justify_center()
        .child(name);
        self.control(
            button,
            id,
            name,
            Role::Tab,
            false,
            Some(selected),
            cx,
            move |this, window, cx| {
                let origin = SharedString::from(format!("surface-{name}"));
                this.show_surface_from(surface, Some(origin.clone().into()), window, cx);
                this.accessibility.focus_button_after_paint(origin);
            },
        )
    }

    /// Home is the ordinary first screen. It uses the selected personal
    /// profile, the real saved destination index and the shared account state.
    pub(super) fn home_view(
        &self,
        p: Palette,
        window: &mut Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let compact = is_narrow(window) || f32::from(window.viewport_size().width) < 760.;
        let shelf_height = home_shelf_height(window);
        let home = col()
            .id("home")
            .flex_1()
            .min_h_0()
            .bg(p.surface)
            .track_focus(&self.home_focus)
            .child(self.home_hero(p, compact, window.viewport_size().width < px(620.), cx))
            .child(self.home_shelf(p, compact, shelf_height, cx));
        self.accessibility
            .focus_group(home, "home", "Home", Role::Main, &self.home_focus)
            .into_any_element()
    }

    fn home_hero(
        &self,
        p: Palette,
        compact: bool,
        narrow_header: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let background = match self.home_world_image() {
            Some(image) => img(image)
                .size_full()
                .object_fit(gpui::ObjectFit::Cover)
                .into_any_element(),
            None => self.artwork.view.clone().into_any_element(),
        };
        let ink = Palette::read(Preference::Dark, gpui::WindowAppearance::Dark);
        let identity = row().gap_2().child(icons::brand_mark(true, px(28.))).child(
            div()
                .text_size(px(19.))
                .font_semibold()
                .text_color(gpui::rgb(0xf4f4f4))
                .child("Carbon Desktop"),
        );
        let busy = self.popup.is_some() || self.picker_open;
        let navigation =
            row()
                .gap_1()
                .child(self.surface_tabs(ink, cx))
                .when(!self.history.is_empty(), |bar| {
                    bar.child(self.command(
                        ink,
                        "home-back",
                        "Back",
                        false,
                        busy,
                        cx,
                        |this, window, cx| {
                            this.history_back(window, cx);
                        },
                    ))
                });
        let paused = self.artwork.playback.paused;
        let scenery_button = self.command(
            ink,
            "home-scenery",
            "Scenery",
            false,
            busy,
            cx,
            |launcher, window, cx| {
                let scene = popup::ScenePicker::snapshot(launcher);
                popup::open(launcher, popup::Kind::Scenery(scene), window, cx);
            },
        );
        let playback_controls = row()
            .gap_1()
            .child(self.command(
                ink,
                "home-motion",
                if paused { "Resume" } else { "Pause" },
                false,
                busy,
                cx,
                |launcher, _, cx| {
                    launcher
                        .artwork
                        .playback
                        .set_paused(!launcher.artwork.playback.paused, &launcher.root.path);
                    cx.notify();
                },
            ))
            .child(self.command(
                ink,
                "home-shuffle",
                "Shuffle",
                false,
                busy,
                cx,
                |launcher, _, cx| {
                    launcher.rotate_home_scene();
                    cx.notify();
                },
            ));
        let account = self.command(
            ink,
            "home-account",
            self.account.read(cx).toolbar_label(),
            false,
            busy,
            cx,
            |launcher, window, cx| launcher.open_account(window, cx),
        );
        let inset = home_inset(compact);
        let mut scenery_button = Some(scenery_button);
        let utilities = row()
            .gap_1()
            .children(if narrow_header {
                None
            } else {
                scenery_button.take()
            })
            .when(!narrow_header, |r| r.child(playback_controls))
            .child(
                div()
                    .pl(if narrow_header { px(0.) } else { px(8.) })
                    .child(account),
            );
        let heading = col()
            .flex_none()
            .mt(px(if compact { 12. } else { 26. }))
            .mx(inset)
            .gap(px(if compact { 16. } else { 20. }))
            .child(
                row()
                    .justify_between()
                    .gap_2()
                    .child(identity)
                    .child(utilities),
            )
            .child(
                row()
                    .justify_between()
                    .child(navigation)
                    .children(scenery_button),
            );
        col()
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .child(background),
            )
            .child(
                img("brand/home-header-shade.png")
                    .absolute()
                    .top_0()
                    .left_0()
                    .w_full()
                    .h(px(if compact { 124. } else { 146. }))
                    .object_fit(gpui::ObjectFit::Fill),
            )
            .child(
                col()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .id("home-hero-scroll")
                    .overflow_y_scroll()
                    .child(heading)
                    .child(div().flex_1().min_h(px(8.)))
                    .child(self.home_launchbox(p, compact, cx)),
            )
            .into_any_element()
    }

    fn home_launchbox(&self, p: Palette, compact: bool, cx: &Context<Self>) -> AnyElement {
        let panel = col()
            .flex_none()
            .mb(px(if compact { 12. } else { 26. }))
            .mx(home_inset(compact))
            .bg(p.layer)
            .border_1()
            .border_color(p.stroke)
            .px_4()
            .py(px(if compact { 8. } else { 16. }))
            .gap_2();
        panel
            .when(!compact, |panel| panel.w(px(330.)))
            .child(self.home_launchbox_content(p, cx))
            .into_any_element()
    }

    fn home_launchbox_content(&self, p: Palette, cx: &Context<Self>) -> gpui::Div {
        let Some(profile) = self.selected_profile().cloned() else {
            return col()
                .gap_2()
                .child(
                    div()
                        .text_sm()
                        .text_color(p.muted)
                        .child("Welcome to Carbon Desktop"),
                )
                .child(
                    div()
                        .text_size(px(20.))
                        .font_semibold()
                        .child("Set up your first instance"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(p.muted)
                        .child("Start a vanilla profile or import a pack."),
                )
                .child(
                    row()
                        .flex_wrap()
                        .gap_2()
                        .child(self.command(
                            p,
                            "home-first-create",
                            "Create instance",
                            true,
                            self.popup.is_some() || self.profile_issue.is_some(),
                            cx,
                            |this, window, cx| this.editor(popup::Mode::Create, window, cx),
                        ))
                        .child(self.command(
                            p,
                            "home-first-import",
                            "Import pack",
                            false,
                            self.popup.is_some() || self.picker_open,
                            cx,
                            |this, window, cx| this.pick(Picker::Pack, window, cx),
                        )),
                );
        };
        let target = self.selected_target(profile.id);
        let pick_text = target
            .as_ref()
            .map_or_else(|| profile.name.clone(), |(name, _)| name.clone());
        let kind = target
            .as_ref()
            .map_or_else(|| profile_kind(&profile), |(_, kind)| (*kind).to_owned());
        let eyebrow = if target.is_some() {
            "Play destination"
        } else {
            "Selected instance"
        };
        let pick_label = format!("Choose an instance or destination. Current: {pick_text}");
        let metadata = if target.is_some() {
            format!("{} · {kind}", profile.name)
        } else {
            kind
        };
        let pinned = self.home_order.is_pinned(profile.id);
        let readiness = self.home_readiness(&profile, cx);
        let button = compact_action("home-pick", p, Partition::Neutral, false, cx)
            .w_full()
            .h(px(42.))
            .justify_start()
            .gap_2()
            .px_2()
            .child(
                Icon::Builtin(carbon_profiles::icons::displayed(&profile)).render(px(24.), p.ink),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(19.))
                    .font_semibold()
                    .truncate()
                    .child(pick_text),
            )
            .child(div().text_color(p.muted).child("\u{25be}"));
        col()
            .gap_2()
            .child(div().text_sm().text_color(p.muted).child(eyebrow))
            .child(self.control(
                button,
                "home-pick",
                &pick_label,
                Role::Button,
                self.popup.is_some(),
                None,
                cx,
                |this, window, cx| this.open_home_switcher(window, cx),
            ))
            .child(div().text_sm().text_color(p.muted).child(metadata))
            .child(self.accessibility.text(
                div().text_sm().text_color(p.muted).child(readiness),
                "home-readiness",
                readiness,
                Role::Label,
            ))
            .child(
                row()
                    .flex_wrap()
                    .gap_2()
                    .child(self.command(
                        p,
                        "home-open",
                        "Open instance",
                        true,
                        self.popup.is_some() || self.picker_open,
                        cx,
                        |this, window, cx| this.open_selected_from_home(window, cx),
                    ))
                    .child(self.command(
                        p,
                        "home-pin",
                        if pinned {
                            "Unpin instance"
                        } else {
                            "Pin instance"
                        },
                        false,
                        self.popup.is_some(),
                        cx,
                        |this, _, cx| this.toggle_selected_pin(cx),
                    )),
            )
    }

    fn home_shelf(&self, p: Palette, compact: bool, height: f32, cx: &Context<Self>) -> AnyElement {
        let ordered = self.home_order.ordered_ids(&self.profiles);
        let count = self.profiles.len();
        let visible = if compact { 2 } else { 4 };
        let mut tiles = row().w_full().gap_2();
        for id in ordered.iter().take(visible) {
            let Some(profile) = self.profiles.iter().find(|profile| profile.id == *id) else {
                continue;
            };
            tiles = tiles.child(self.home_tile(profile, p, compact, cx));
        }
        let body = if ordered.is_empty() {
            col()
                .flex_1()
                .items_start()
                .child(
                    div()
                        .text_sm()
                        .text_color(p.muted)
                        .child("Instances you create or import will appear here."),
                )
                .into_any_element()
        } else {
            tiles.into_any_element()
        };
        col()
            .flex_none()
            .h(px(height))
            .w_full()
            .justify_center()
            .gap_2()
            .px(home_inset(compact))
            .py(px(10.))
            .bg(p.surface)
            .border_t_1()
            .border_color(p.stroke)
            .child(
                row()
                    .w_full()
                    .justify_between()
                    .child(
                        row()
                            .gap_2()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(if compact { 16. } else { 19. }))
                                    .font_semibold()
                                    .child("Your library"),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(p.muted)
                                    .child(instance_count(count)),
                            ),
                    )
                    .child(self.command(
                        p,
                        "home-open-library",
                        "Open library",
                        false,
                        self.popup.is_some(),
                        cx,
                        |this, window, cx| this.open_library(window, cx),
                    )),
            )
            .child(body)
            .into_any_element()
    }

    fn home_tile(
        &self,
        profile: &Profile,
        p: Palette,
        compact: bool,
        cx: &Context<Self>,
    ) -> accessibility::Semantic {
        let id = profile.id;
        let selected = self.personal == Some(id);
        let kind = profile_kind(profile);
        let label = format!("{}, {kind}", profile.name);
        let button = compact_action(
            SharedString::from(format!("home-shelf-{id}")),
            p,
            if selected {
                Partition::Selected
            } else {
                Partition::Neutral
            },
            false,
            cx,
        )
        .flex_1()
        .min_w(px(0.))
        .h(px(if compact { 50. } else { 62. }))
        .justify_start()
        .px_2()
        .when(selected, |button| {
            button.border_l_2().border_color(p.accent)
        })
        .child(
            row()
                .gap_2()
                .min_w_0()
                .child(home_mark(
                    p,
                    Icon::Builtin(carbon_profiles::icons::displayed(profile)),
                ))
                .child(
                    col()
                        .min_w_0()
                        .items_start()
                        .child(
                            div()
                                .w_full()
                                .truncate()
                                .font_semibold()
                                .child(profile.name.clone()),
                        )
                        .child(
                            div()
                                .w_full()
                                .truncate()
                                .text_xs()
                                .text_color(p.muted)
                                .child(kind),
                        ),
                ),
        );
        self.control(
            button,
            SharedString::from(format!("home-shelf-{id}")),
            &label,
            Role::ListBoxOption,
            false,
            Some(selected),
            cx,
            move |this, window, cx| {
                this.open_profile_from_home(
                    id,
                    SharedString::from(format!("home-shelf-{id}")).into(),
                    window,
                    cx,
                )
            },
        )
    }

    fn open_home_switcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.popup.is_some() || self.picker_open {
            return;
        }
        let Some(id) = self.personal else {
            return;
        };
        self.load_targets(id, cx);
        let kind = popup::home_picker(self);
        popup::open(self, kind, window, cx);
    }

    pub(super) fn home_select_instance(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if self.profiles.iter().any(|p| p.id == id) {
            self.select_personal(id, cx);
            cx.notify();
        }
    }

    fn open_selected_from_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.personal {
            self.open_profile_from_home(id, "home-open".into(), window, cx);
        }
    }

    fn open_profile_from_home(
        &mut self,
        id: Uuid,
        origin: gpui::ElementId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_surface_from(Surface::Library, Some(origin), window, cx);
        self.select_personal(id, cx);
        self.tab = Tab::Overview;
        self.show_detail = true;
        self.focus_detail(window);
        cx.notify();
    }

    fn open_library(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_surface_from(
            Surface::Library,
            Some("home-open-library".into()),
            window,
            cx,
        );
        self.focus_detail(window);
        cx.notify();
    }

    fn toggle_selected_pin(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.personal else {
            return;
        };
        let pinned = self.home_order.toggle_pin(id);
        self.persist_home_order();
        self.message = if pinned {
            "Instance pinned to Home.".to_owned()
        } else {
            "Instance unpinned.".to_owned()
        };
        cx.notify();
    }

    pub(super) fn remember_recent(&mut self, id: Uuid) {
        if self.home_order.touch_recent(id) {
            self.persist_home_order();
        }
    }

    pub(super) fn select_personal(&mut self, id: Uuid, cx: &mut Context<Self>) {
        self.choose_personal(id, cx);
        self.remember_recent(id);
    }

    fn persist_home_order(&mut self) {
        if let Err(error) = self.home_order.save(&self.root.path) {
            self.message = format!("Could not save Home ordering: {error}");
        }
    }

    /// Change the top-level destination and record a Back entry. Repeated
    /// visits to the current destination do not add history noise.
    pub(super) fn show_surface(
        &mut self,
        surface: Surface,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.show_surface_from(surface, None, window, cx)
    }

    fn show_surface_from(
        &mut self,
        surface: Surface,
        origin: Option<gpui::ElementId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.surface == surface {
            return false;
        }
        self.record_history(origin);
        if self.surface == Surface::Library {
            self.save_library_scroll();
        }
        self.surface = surface;
        if surface == Surface::Library {
            self.restore_library_scroll();
        }
        cx.notify();
        window.refresh();
        true
    }

    /// Return through the navigation history after the existing compact-detail
    /// Escape behavior has had a chance to handle the key.
    pub(super) fn history_back(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(entry) = self.history.pop() else {
            return false;
        };
        if self.surface == Surface::Library {
            self.save_library_scroll();
        }
        self.surface = entry.surface;
        self.review = entry.review;
        self.personal = entry.personal;
        self.fixture = entry.fixture;
        self.tab = entry.tab;
        self.show_detail = entry.show_detail;
        self.search_back_to_detail = false;
        if entry.surface == Surface::Library {
            self.restore_library_scroll();
            if let Some(id) = self.personal
                && !self.targets.contains_key(&id)
            {
                self.load_targets(id, cx);
            }
        }
        let fallback = if entry.surface == Surface::Home {
            self.home_focus.clone()
        } else {
            self.detail_focus.clone()
        };
        fallback.focus(window);
        if let Some(origin) = entry.origin {
            self.accessibility.focus_button_after_paint(origin);
        }
        cx.notify();
        true
    }

    fn record_history(&mut self, origin: Option<gpui::ElementId>) {
        let entry = HistoryEntry {
            origin,
            surface: self.surface,
            review: self.review,
            personal: self.personal,
            fixture: self.fixture,
            tab: self.tab,
            show_detail: self.show_detail,
        };
        if self.history.last() != Some(&entry) {
            self.history.push(entry);
            if self.history.len() > 32 {
                self.history.remove(0);
            }
        }
    }

    fn save_library_scroll(&mut self) {
        self.library_scroll_offset = self.library_scroll.offset();
        self.detail_scroll_offset = self.detail_scroll.offset();
    }

    fn restore_library_scroll(&self) {
        self.library_scroll.set_offset(self.library_scroll_offset);
        self.detail_scroll.set_offset(self.detail_scroll_offset);
    }

    fn selected_target(&self, id: Uuid) -> Option<(String, &'static str)> {
        let TargetsState::Ready(snapshot) = self.targets.get(&id)? else {
            return None;
        };
        match snapshot.selected.as_ref()? {
            TargetKey::World(folder) => snapshot
                .worlds
                .iter()
                .find(|world| matches!(&world.key, TargetKey::World(name) if name == folder))
                .map(|world| (world.name.clone(), "Local world")),
            TargetKey::Server(server) => snapshot
                .destinations
                .iter()
                .find(|destination| destination.id == *server)
                .map(|destination| (destination.name.clone(), "Saved server")),
        }
    }

    fn home_readiness(&self, profile: &Profile, cx: &Context<Self>) -> &'static str {
        let setup = self.setup.get(&profile.id);
        let settled = setup.is_some_and(Setup::game_files_prepared);
        let jdk_ready = setup
            .and_then(|setup| setup.inspection.as_ref())
            .is_some_and(|inspection| inspection.managed_java.java_home().is_some());
        let account = self.account.read(cx);
        if profile.server_address.is_some() {
            "Saved server · joining is not available"
        } else if pack_has_requirements(profile) {
            "Pack requirements"
        } else if settled && !jdk_ready {
            "Java required"
        } else if settled && account.unavailable() {
            "Sign-in unavailable"
        } else if settled && account.signed_in() {
            "Setup ready · protected launch is unavailable"
        } else if settled && account.active_account().is_some() {
            "Account check required"
        } else if settled {
            "Account required"
        } else {
            "Setup required"
        }
    }
}

fn home_inset(compact: bool) -> gpui::Pixels {
    px(if compact { 12. } else { 30. })
}

fn home_shelf_height(window: &Window) -> f32 {
    home_shelf_height_for(f32::from(window.viewport_size().height))
}

fn home_shelf_height_for(height: f32) -> f32 {
    if height < 630. {
        96.
    } else {
        (height * 0.22).clamp(110., 166.)
    }
}

fn instance_count(count: usize) -> String {
    if count == 1 {
        "1 instance".to_owned()
    } else {
        format!("{count} instances")
    }
}

fn profile_kind(profile: &Profile) -> String {
    match &profile.imported_pack {
        Some(pack) => match &pack.loader {
            Some(loader) => format!("Minecraft {} · {loader}", profile.version.id()),
            None => format!("Minecraft {} · Modpack", profile.version.id()),
        },
        None => format!("Minecraft {} · Vanilla", profile.version.id()),
    }
}

fn home_mark(p: Palette, icon: Icon) -> gpui::Div {
    div()
        .flex_none()
        .size(px(28.))
        .bg(p.surface)
        .border_1()
        .border_color(p.stroke)
        .flex()
        .items_center()
        .justify_center()
        .child(icon.render(px(17.), p.ink))
}

#[cfg(test)]
mod tests {
    use super::*;
    use carbon_profiles::VanillaVersion;

    fn profile(id: Uuid) -> Profile {
        Profile {
            icon_id: None,
            imported_pack: None,
            server_address: None,
            id,
            name: id.to_string(),
            version: VanillaVersion::Release26_3,
            memory_mib: 2048,
            window_width: 1280,
            window_height: 720,
        }
    }

    #[test]
    fn a_full_order_round_trips_through_the_home_read_limit() {
        let dir = tempfile::tempdir().unwrap();
        let mut order = HomeOrder::default();
        order.pinned = (0..MAX_PINNED).map(|_| Uuid::new_v4()).collect();
        order.recent = (0..MAX_RECENT).map(|_| Uuid::new_v4()).collect();
        order.save(dir.path()).unwrap();
        let loaded = HomeOrder::load(dir.path());
        assert_eq!(loaded.pinned, order.pinned);
        assert_eq!(loaded.recent, order.recent);
    }

    #[test]
    fn pinned_profiles_precede_recent_then_the_remaining_library() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let d = Uuid::new_v4();
        let profiles = vec![profile(a), profile(b), profile(c), profile(d)];
        let mut order = HomeOrder::default();
        assert!(order.toggle_pin(b));
        assert!(order.touch_recent(c));
        assert!(order.touch_recent(a));
        assert!(order.touch_recent(c));
        assert_eq!(order.ordered_ids(&profiles), vec![b, c, a, d]);
        order.retain_profiles(&profiles[..2]);
        assert_eq!(order.ordered_ids(&profiles[..2]), vec![b, a]);
    }

    #[test]
    fn stale_ids_are_ignored_and_the_recent_file_stays_bounded() {
        let a = Uuid::new_v4();
        let missing = Uuid::new_v4();
        let mut order = HomeOrder::default();
        order.pinned.push(missing);
        order.recent.push(a);
        assert_eq!(order.ordered_ids(&[profile(a)]), vec![a]);
    }

    #[test]
    fn shelf_height_uses_twenty_two_percent_until_the_short_compact_case() {
        assert_eq!(home_shelf_height_for(400.), 96.);
        assert_eq!(home_shelf_height_for(720.), 158.4);
        assert_eq!(home_shelf_height_for(2000.), 166.);
    }
}
