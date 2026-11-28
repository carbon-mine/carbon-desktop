// SPDX-License-Identifier: MPL-2.0
//! Native Home destination: first screen, shelf, ordering and quick switcher.
use super::view::{col, row};
use super::*;
use accesskit::Role;
use carbon_profiles::{Profile, TargetKey};
use gpui::SharedString;
use gpui::{
    AnyElement, Bounds, BoxShadow, ContentMask, Corners, DevicePixels, ObjectFit, Pixels,
    RenderImage, Rgba, canvas, div, img, linear_color_stop, linear_gradient, point, px, rgba, size,
};
use gpui_component::StyledExt;
use icons::Icon;
use serde::{Deserialize, Serialize};
use std::{cell::Cell, collections::HashSet, path::Path, rc::Rc, sync::Arc};
use theme::{Palette, Partition, compact_action, glass_action};

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
    #[expect(
        clippy::too_many_arguments,
        reason = "Mirrors Launcher::command with Home floating pill contrast."
    )]
    fn home_command<F>(
        &self,
        p: Palette,
        id: impl Into<gpui::ElementId>,
        label: impl Into<SharedString>,
        primary: bool,
        disabled: bool,
        cx: &Context<Self>,
        action: F,
    ) -> accessibility::Semantic
    where
        F: Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    {
        let treatment = if primary {
            Partition::Primary
        } else {
            Partition::Neutral
        };
        let id = id.into();
        let label = label.into();
        let button = if primary {
            theme::label(
                compact_action(id.clone(), p, treatment, disabled, cx),
                label.clone(),
                p,
                treatment,
                disabled,
            )
        } else {
            home_pill_button(id.clone(), false, disabled, cx).child(
                div()
                    .text_color(if disabled {
                        rgba(0xffffff7a)
                    } else {
                        rgba(0xf6f7f9ff)
                    })
                    .child(label.clone()),
            )
        };
        self.control(
            button,
            id,
            label.as_ref(),
            Role::Button,
            disabled,
            None,
            cx,
            action,
        )
    }

    pub(super) fn surface_tabs(&self, p: Palette, cx: &Context<Self>) -> AnyElement {
        row()
            .gap_1()
            .child(self.surface_tab(p, Surface::Home, "Home", cx))
            .child(self.surface_tab(p, Surface::Library, "Library", cx))
            .into_any_element()
    }

    fn surface_tab(
        &self,
        _p: Palette,
        surface: Surface,
        name: &'static str,
        cx: &Context<Self>,
    ) -> accessibility::Semantic {
        let selected = self.surface == surface;
        let id = SharedString::from(format!("surface-{name}"));
        let button = home_pill_button(id.clone(), selected, false, cx)
            .min_w(px(72.))
            .justify_center()
            .child(name);
        self.control(
            button,
            id,
            name,
            Role::Tab,
            self.popup.is_some() || self.picker_open,
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
        let background = match self.home_world_image() {
            Some(image) => img(image)
                .size_full()
                .object_fit(gpui::ObjectFit::Cover)
                .into_any_element(),
            None => self.artwork.view.clone().into_any_element(),
        };
        let home_bounds = Rc::new(Cell::new(Bounds::default()));
        let record_bounds = home_bounds.clone();
        let (world_blur, scene_view) = self.live_blur_source();
        let backdrop = BlurBackdrop {
            home_bounds,
            world_blur,
            scene_view,
        };
        let home = col()
            .id("home")
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .bg(p.surface)
            .track_focus(&self.home_focus)
            .child(
                canvas(
                    move |bounds, _, _| record_bounds.set(bounds),
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .right_0(),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .child(background),
            )
            .child(self.home_hero(
                p,
                compact,
                window.viewport_size().width < px(620.),
                backdrop.clone(),
                cx,
            ))
            .child(self.home_shelf(p, compact, shelf_height, backdrop, cx));
        self.accessibility
            .focus_group(home, "home", "Home", Role::Main, &self.home_focus)
            .into_any_element()
    }

    fn home_hero(
        &self,
        p: Palette,
        compact: bool,
        narrow_header: bool,
        backdrop: BlurBackdrop,
        cx: &Context<Self>,
    ) -> AnyElement {
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
                    bar.child(self.home_command(
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
        let scenery_button = self.home_command(
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
            .child(self.home_command(
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
            .child(self.home_command(
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
        let account = self.home_command(
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
        let shade_height: f32 = if compact { 124. } else { 146. };
        let header_shade = [
            (shade_height, rgba(0x0d11146e)),
            ((shade_height * 0.68).round(), rgba(0x0d111473)),
        ]
        .map(|(height, top)| {
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .h(px(height))
                .bg(linear_gradient(
                    180.,
                    linear_color_stop(top, 0.0),
                    linear_color_stop(rgba(0x0d111400), 1.0),
                ))
        });
        let bottom_band = if compact {
            col()
                .w_full()
                .when_some(self.home_credit(true, cx), |el, credit| {
                    el.child(row().w_full().justify_end().child(credit))
                })
                .child(self.home_launchbox(p, compact, backdrop, cx))
                .into_any_element()
        } else {
            row()
                .w_full()
                .items_end()
                .justify_between()
                .gap_4()
                .child(self.home_launchbox(p, compact, backdrop, cx))
                .children(self.home_credit(false, cx))
                .into_any_element()
        };
        col()
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .children(header_shade)
            .child(
                col()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .id("home-hero-scroll")
                    .overflow_y_scroll()
                    .child(heading)
                    .child(div().flex_1().min_h(px(8.)))
                    .child(bottom_band),
            )
            .into_any_element()
    }

    fn home_credit(&self, compact: bool, cx: &Context<Self>) -> Option<AnyElement> {
        if self.home_world_image().is_some() {
            return None;
        }
        // The painted image can lag a new selection until its poster decodes.
        let credit = self
            .artwork
            .view
            .read(cx)
            .shown_landscape()
            .unwrap_or(self.artwork.playback.scene)
            .source_credit();
        let pill = div()
            .flex_initial()
            .min_w_0()
            .max_w(px(if compact { 360. } else { 700. }))
            .mb(px(if compact { 6. } else { 10. }))
            .mr(home_inset(compact))
            .px(px(7.))
            .py(px(3.))
            .bg(rgba(0x0f121799))
            .border_1()
            .border_color(rgba(0xffffff24))
            .text_size(px(10.5))
            .text_color(rgba(0xe5e8eed9))
            .text_ellipsis()
            .child(credit);
        Some(
            self.accessibility
                .text(pill, "home-scenery-credit", credit, Role::Label)
                .into_any_element(),
        )
    }

    fn home_launchbox(
        &self,
        p: Palette,
        compact: bool,
        backdrop: BlurBackdrop,
        cx: &Context<Self>,
    ) -> AnyElement {
        let glass = ShelfGlass::for_palette(p);
        let (key_alpha, ambient_alpha) = if p.dark {
            (0x00000024, 0x00000038)
        } else {
            (0x00000010, 0x00000018)
        };
        let panel = div()
            .relative()
            .flex_none()
            .overflow_hidden()
            .mb(px(if compact { 12. } else { 26. }))
            .mx(home_inset(compact))
            .border_1()
            .border_color(glass.card_border)
            .shadow(vec![
                BoxShadow {
                    color: rgba(ambient_alpha).into(),
                    offset: point(px(0.), px(12.)),
                    blur_radius: px(32.),
                    spread_radius: px(0.),
                },
                BoxShadow {
                    color: rgba(key_alpha).into(),
                    offset: point(px(0.), px(2.)),
                    blur_radius: px(6.),
                    spread_radius: px(0.),
                },
            ])
            .child(backdrop.layer())
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .bg(glass.card_tint),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(px(1.))
                    .bg(glass.card_sheen),
            )
            .child(
                col()
                    .relative()
                    .px_4()
                    .py(px(if compact { 10. } else { 16. }))
                    .gap(px(if compact { 8. } else { 10. }))
                    .child(self.home_launchbox_content(p, glass, cx)),
            );
        panel
            .when(!compact, |panel| panel.w(px(384.)))
            .into_any_element()
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Mirrors Launcher::home_command with Home frosted card button contrast."
    )]
    fn home_launchbox_command<F>(
        &self,
        p: Palette,
        glass: ShelfGlass,
        id: impl Into<gpui::ElementId>,
        label: impl Into<SharedString>,
        primary: bool,
        disabled: bool,
        cx: &Context<Self>,
        action: F,
    ) -> accessibility::Semantic
    where
        F: Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    {
        let treatment = if primary {
            Partition::Primary
        } else {
            Partition::Neutral
        };
        let id = id.into();
        let label = label.into();
        let base = if primary {
            compact_action(id.clone(), p, treatment, disabled, cx)
        } else {
            glass_action(id.clone(), p, treatment, disabled, cx).when(!disabled, |button| {
                button.border_color(glass.card_control_border)
            })
        };
        let button = theme::label(base, label.clone(), p, treatment, disabled);
        self.control(
            button,
            id,
            label.as_ref(),
            Role::Button,
            disabled,
            None,
            cx,
            action,
        )
    }

    fn home_launchbox_content(
        &self,
        p: Palette,
        glass: ShelfGlass,
        cx: &Context<Self>,
    ) -> gpui::Div {
        if self.library_mode == LibraryMode::Worlds {
            let world = self.selected_world().cloned().or_else(|| {
                self.worlds
                    .as_ref()
                    .and_then(WorldLibrary::recommended)
                    .cloned()
            });
            if let Some(world) = world {
                return self.home_world_launchbox(world, p, glass, cx);
            }
        }
        let Some(profile) = self.selected_profile().cloned() else {
            return col()
                .gap(px(10.))
                .child(
                    col()
                        .gap(px(4.))
                        .child(
                            div()
                                .text_xs()
                                .font_medium()
                                .text_color(glass.eyebrow)
                                .child("Welcome to Carbon Desktop"),
                        )
                        .child(
                            div()
                                .text_size(px(19.))
                                .font_semibold()
                                .child("Set up your first instance"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(glass.meta)
                                .child("Start a vanilla profile or import a pack."),
                        ),
                )
                .child(div().h(px(1.)).w_full().bg(glass.divider))
                .child(
                    row()
                        .flex_wrap()
                        .gap_2()
                        .child(self.home_launchbox_command(
                            p,
                            glass,
                            "home-first-create",
                            "Create instance",
                            true,
                            self.popup.is_some()
                                || self.picker_open
                                || self.profile_issue.is_some(),
                            cx,
                            |this, window, cx| this.editor(popup::Mode::Create, window, cx),
                        ))
                        .child(self.home_launchbox_command(
                            p,
                            glass,
                            "home-first-import",
                            "Import pack",
                            false,
                            self.popup.is_some() || self.picker_open,
                            cx,
                            |this, window, cx| this.pick(Picker::Pack, window, cx),
                        ))
                        .child(self.home_launchbox_command(
                            p,
                            glass,
                            "home-open-library",
                            "Open library",
                            false,
                            self.popup.is_some() || self.picker_open,
                            cx,
                            |this, window, cx| this.open_library(window, cx),
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
        let ready = readiness.starts_with("Setup ready");
        let busy = self.popup.is_some() || self.picker_open;
        let picker = glass_action("home-pick", p, Partition::Neutral, busy, cx)
            .w_full()
            .h(px(40.))
            .when(!busy, |button| {
                button.border_color(glass.card_control_border)
            })
            .justify_start()
            .gap_2()
            .pl_2()
            .pr(px(34.))
            .child(
                Icon::Builtin(carbon_profiles::icons::displayed(&profile)).render(px(22.), p.ink),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(18.))
                    .font_semibold()
                    .truncate()
                    .child(pick_text),
            );
        col()
            .gap(px(10.))
            .child(
                col()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(glass.eyebrow)
                            .child(eyebrow),
                    )
                    .child(
                        div()
                            .relative()
                            .w_full()
                            .child(self.control(
                                picker,
                                "home-pick",
                                &pick_label,
                                Role::Button,
                                busy,
                                None,
                                cx,
                                |this, window, cx| this.open_home_switcher(window, cx),
                            ))
                            .child(
                                div()
                                    .absolute()
                                    .right(px(12.))
                                    .top(px(12.))
                                    .size(px(16.))
                                    .child(Icon::ChevronDown.render(
                                        px(16.),
                                        if busy { p.muted } else { glass.eyebrow },
                                    )),
                            ),
                    )
                    .child(div().text_xs().text_color(glass.meta).child(metadata)),
            )
            .child(div().h(px(1.)).w_full().bg(glass.divider))
            .child(
                row()
                    .items_center()
                    .gap_2()
                    .child(div().flex_none().size(px(6.)).bg(if ready {
                        p.accent
                    } else {
                        glass.status_muted
                    }))
                    .child(self.accessibility.text(
                        div().text_xs().text_color(glass.meta).child(readiness),
                        "home-readiness",
                        readiness,
                        Role::Label,
                    )),
            )
            .child(
                row()
                    .flex_wrap()
                    .gap_2()
                    .child(self.home_launchbox_command(
                        p,
                        glass,
                        "home-open",
                        "Open instance",
                        true,
                        busy,
                        cx,
                        |this, window, cx| this.open_selected_from_home(window, cx),
                    ))
                    .child(self.home_launchbox_command(
                        p,
                        glass,
                        "home-pin",
                        if pinned {
                            "Unpin instance"
                        } else {
                            "Pin instance"
                        },
                        false,
                        busy,
                        cx,
                        |this, _, cx| this.toggle_selected_pin(cx),
                    ))
                    .child(self.home_launchbox_command(
                        p,
                        glass,
                        "home-open-library",
                        "Open library",
                        false,
                        busy,
                        cx,
                        |this, window, cx| this.open_library(window, cx),
                    )),
            )
    }

    fn home_world_launchbox(
        &self,
        world: LibraryWorld,
        p: Palette,
        glass: ShelfGlass,
        cx: &Context<Self>,
    ) -> gpui::Div {
        let source = match world.backing {
            carbon_profiles::WorldBacking::Local => "Local",
            carbon_profiles::WorldBacking::Imported => "Imported",
            carbon_profiles::WorldBacking::ServerBound => "Server-bound",
            carbon_profiles::WorldBacking::Managed => "Managed",
        };
        let selected = self.selected_world.as_ref() == Some(&world.id);
        let eyebrow = if selected {
            "Selected world"
        } else {
            "Recommended world"
        };
        let metadata = format!(
            "{} · {} · Minecraft {}{}",
            world.instance_name,
            source,
            world.version,
            world
                .loader
                .as_ref()
                .map_or_else(String::new, |loader| format!(" · {loader}"))
        );
        let icon = self
            .profiles
            .iter()
            .find(|profile| profile.id == world.id.instance)
            .map_or(Icon::Instance, |profile| {
                Icon::Builtin(carbon_profiles::icons::displayed(profile))
            });
        let busy = self.popup.is_some() || self.picker_open;
        let readiness = world.issue.as_ref().map_or(
            "Direct world launch is unavailable until protected launch is qualified.".to_owned(),
            |issue| format!("Save scan needs attention. {issue}"),
        );
        col()
            .gap(px(10.))
            .child(
                col()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .text_color(glass.eyebrow)
                            .child(eyebrow),
                    )
                    .child(
                        row()
                            .w_full()
                            .min_w_0()
                            .gap_2()
                            .child(icon.render(px(22.), p.ink))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(px(18.))
                                    .font_semibold()
                                    .truncate()
                                    .child(world.display_name.clone()),
                            ),
                    )
                    .child(div().text_xs().text_color(glass.meta).child(metadata)),
            )
            .child(div().h(px(1.)).w_full().bg(glass.divider))
            .child(
                row()
                    .items_center()
                    .gap_2()
                    .child(div().flex_none().size(px(6.)).bg(glass.status_muted))
                    .child(
                        self.accessibility.text(
                            div()
                                .text_xs()
                                .text_color(glass.meta)
                                .child(readiness.clone()),
                            "home-world-readiness",
                            readiness,
                            Role::Label,
                        ),
                    ),
            )
            .child(
                row()
                    .flex_wrap()
                    .gap_2()
                    .child(self.home_launchbox_command(
                        p,
                        glass,
                        "home-world-open",
                        "Open world",
                        true,
                        busy,
                        cx,
                        |this, window, cx| this.open_selected_world_from_home(window, cx),
                    ))
                    .child(self.home_launchbox_command(
                        p,
                        glass,
                        "home-world-library",
                        "All worlds",
                        false,
                        busy,
                        cx,
                        |this, window, cx| this.open_library(window, cx),
                    )),
            )
    }

    fn home_shelf(
        &self,
        p: Palette,
        compact: bool,
        height: f32,
        backdrop: BlurBackdrop,
        cx: &Context<Self>,
    ) -> AnyElement {
        let glass = ShelfGlass::for_palette(p);
        let ordered = self.home_order.ordered_ids(&self.profiles);
        let count = self.profiles.len();
        let visible = if compact { 2 } else { 4 };
        let mut tiles = row().w_full().gap_2();
        for id in ordered.iter().take(visible) {
            let Some(profile) = self.profiles.iter().find(|profile| profile.id == *id) else {
                continue;
            };
            tiles = tiles.child(self.home_tile(profile, p, glass, compact, cx));
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
            .relative()
            .flex_none()
            .h(px(height))
            .w_full()
            .overflow_hidden()
            .justify_center()
            .gap_2()
            .px(home_inset(compact))
            .py(px(10.))
            .border_t_1()
            .border_color(glass.top_border)
            .child(backdrop.layer())
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .bg(glass.tint),
            )
            .child(
                row().w_full().child(
                    row()
                        .gap_2()
                        .min_w_0()
                        .child(
                            div()
                                .text_size(px(if compact { 16. } else { 19. }))
                                .font_semibold()
                                .child("Recent Instances"),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(p.muted)
                                .child(instance_count(count)),
                        ),
                ),
            )
            .child(body)
            .into_any_element()
    }

    fn home_tile(
        &self,
        profile: &Profile,
        p: Palette,
        glass: ShelfGlass,
        compact: bool,
        cx: &Context<Self>,
    ) -> accessibility::Semantic {
        let id = profile.id;
        let selected = self.personal == Some(id);
        let kind = profile_kind(profile);
        let label = format!("{}, {kind}", profile.name);
        let button = glass_action(
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
                    glass,
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
            self.popup.is_some() || self.picker_open,
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

    fn open_selected_world_from_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(world) = self.selected_world().cloned().or_else(|| {
            self.worlds
                .as_ref()
                .and_then(WorldLibrary::recommended)
                .cloned()
        }) else {
            self.open_library(window, cx);
            return;
        };
        let id = world.id.clone();
        self.show_surface_from(Surface::Library, Some("home-world-open".into()), window, cx);
        self.select_world(id, cx);
        self.show_detail = true;
        self.focus_detail(window);
        cx.notify();
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

    /// Select an instance from an explicit user action. Background reloads and
    /// save completions use `choose_personal` directly so data arrival never
    /// starts a transition.
    pub(super) fn select_personal(&mut self, id: Uuid, cx: &mut Context<Self>) {
        let changes_detail = self.personal != Some(id) || self.review;
        self.choose_personal(id, cx);
        if changes_detail {
            self.begin_detail_settle();
        }
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
        // The body is already live content; the settle only softens its first
        // paint. An inner pane must not stack a second fade on the same change.
        self.navigation_settle
            .begin(Instant::now(), self.reduced_motion());
        self.detail_settle.snap();
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
        let changes_surface = self.surface != entry.surface;
        let changes_detail = self.tab != entry.tab
            || self.personal != entry.personal
            || self.review != entry.review
            || self.fixture != entry.fixture;
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
        if changes_surface {
            self.navigation_settle
                .begin(Instant::now(), self.reduced_motion());
            self.detail_settle.snap();
        } else if changes_detail {
            self.begin_detail_settle();
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

/// Shared Home frosted-glass backdrop painter aligned to the full Home bounds.
#[derive(Clone)]
struct BlurBackdrop {
    home_bounds: Rc<Cell<Bounds<Pixels>>>,
    world_blur: Option<Arc<RenderImage>>,
    scene_view: gpui::Entity<scenery::SceneView>,
}

impl BlurBackdrop {
    fn layer(&self) -> AnyElement {
        let home_bounds = self.home_bounds.clone();
        let world_blur = self.world_blur.clone();
        let scene_view = self.scene_view.clone();
        canvas(
            |_, _, _| {},
            move |panel_bounds, _, window, cx| {
                let Some(blurred) = world_blur
                    .as_ref()
                    .cloned()
                    .or_else(|| scene_view.read(cx).blurred_image())
                else {
                    return;
                };
                let full = home_bounds.get();
                if f32::from(full.size.width) <= 0. || f32::from(full.size.height) <= 0. {
                    return;
                }
                let raw_size = blurred.size(0);
                let inner_w = u32::from(raw_size.width).saturating_sub(2).max(1);
                let inner_h = u32::from(raw_size.height).saturating_sub(2).max(1);
                let cover = ObjectFit::Cover.get_bounds(
                    full,
                    size(DevicePixels(inner_w as i32), DevicePixels(inner_h as i32)),
                );
                let texel_w = cover.size.width / inner_w as f32;
                let texel_h = cover.size.height / inner_h as f32;
                let padded = Bounds {
                    origin: point(cover.origin.x - texel_w, cover.origin.y - texel_h),
                    size: size(
                        cover.size.width + texel_w * 2.,
                        cover.size.height + texel_h * 2.,
                    ),
                };
                window.with_content_mask(
                    Some(ContentMask {
                        bounds: panel_bounds,
                    }),
                    |window| {
                        let _ = window.paint_image(padded, Corners::default(), blurred, 0, false);
                    },
                );
            },
        )
        .absolute()
        .top_0()
        .bottom_0()
        .left_0()
        .right_0()
        .into_any_element()
    }
}

/// Uniform frosted-glass surface tint, rim, and typography tokens for Home.
#[derive(Clone, Copy)]
struct ShelfGlass {
    tint: Rgba,
    top_border: Rgba,
    mark_fill: Rgba,
    mark_border: Rgba,
    card_tint: Rgba,
    card_border: Rgba,
    card_sheen: Rgba,
    card_control_border: Rgba,
    divider: Rgba,
    eyebrow: Rgba,
    meta: Rgba,
    status_muted: Rgba,
}

impl ShelfGlass {
    fn for_palette(p: Palette) -> Self {
        if p.dark {
            Self {
                // Opaque enough that the muted instance count keeps 4.5:1
                // contrast even over the brightest blurred backdrop.
                tint: rgba(0x141518cc),
                top_border: rgba(0xffffff38),
                mark_fill: rgba(0x10111470),
                mark_border: rgba(0xffffff24),
                card_tint: rgba(0x0f1217a6),
                card_border: rgba(0xffffff38),
                card_sheen: rgba(0xffffff2c),
                card_control_border: rgba(0xffffff34),
                divider: rgba(0xffffff1a),
                eyebrow: rgba(0xd0d4dcff),
                meta: rgba(0xbec3ccff),
                status_muted: rgba(0xffffff5c),
            }
        } else {
            Self {
                tint: rgba(0xf3f4f8b8),
                top_border: rgba(0x00000024),
                mark_fill: rgba(0xffffff8a),
                mark_border: rgba(0x00000024),
                card_tint: rgba(0xf4f5f8cc),
                card_border: rgba(0x00000024),
                card_sheen: rgba(0xffffffb0),
                card_control_border: rgba(0x00000028),
                divider: rgba(0x00000014),
                eyebrow: rgba(0x4b5058ff),
                meta: rgba(0x52565eff),
                status_muted: rgba(0x00000038),
            }
        }
    }
}

fn home_pill_button(
    id: impl Into<gpui::ElementId>,
    selected: bool,
    disabled: bool,
    cx: &App,
) -> gpui_component::button::Button {
    use gpui_component::{
        Disableable, Selectable, Sizable,
        button::{Button, ButtonCustomVariant, ButtonVariants},
    };
    let (bg, hover, active, border, ink) = if selected {
        (
            rgba(0xf4f5f7ee),
            rgba(0xffffffff),
            rgba(0xdde0e4ff),
            rgba(0xffffffff),
            rgba(0x14161aff),
        )
    } else if disabled {
        (
            rgba(0x10131880),
            rgba(0x10131880),
            rgba(0x10131880),
            rgba(0xffffff22),
            rgba(0x9ca3afff),
        )
    } else {
        (
            rgba(0x0f1217b8),
            rgba(0x1b2028db),
            rgba(0x272d38eb),
            rgba(0xffffff42),
            rgba(0xf6f7f9ff),
        )
    };
    let variant = ButtonCustomVariant::new(cx)
        .color(bg.into())
        .foreground(ink.into())
        .hover(hover.into())
        .active(active.into());
    Button::new(id)
        .custom(variant)
        .selected(selected)
        .disabled(disabled)
        .small()
        .rounded(px(0.))
        .h(px(32.))
        .border_1()
        .border_color(border)
        .text_color(ink)
}

fn home_mark(p: Palette, glass: ShelfGlass, icon: Icon) -> gpui::Div {
    div()
        .flex_none()
        .size(px(28.))
        .bg(glass.mark_fill)
        .border_1()
        .border_color(glass.mark_border)
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
        let order = HomeOrder {
            pinned: (0..MAX_PINNED).map(|_| Uuid::new_v4()).collect(),
            recent: (0..MAX_RECENT).map(|_| Uuid::new_v4()).collect(),
            ..HomeOrder::default()
        };
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
