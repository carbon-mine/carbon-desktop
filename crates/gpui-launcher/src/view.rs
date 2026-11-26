/* SPDX-License-Identifier: MPL-2.0 */
use super::*;
use accesskit::Role;
use carbon_runtime::RuntimeStatus;
use carbon_store::{Generation, Phase};
use gpui::{AnyElement, SharedString, div};
use gpui_component::{
    Sizable, StyledExt,
    button::{Button, ButtonVariants},
    input::Input,
};
use icons::Icon;
use std::rc::Rc;
use theme::{
    Palette, Partition, compact_action, label as control_label, partition, partition_strip,
};

fn col() -> gpui::Div {
    div().flex().flex_col()
}
fn row() -> gpui::Div {
    div().flex().items_center()
}
fn command_row(p: Palette) -> gpui::Div {
    row()
        .w_full()
        .flex_wrap()
        .gap_2()
        .px_5()
        .py_3()
        .border_b_1()
        .border_color(p.stroke)
}

impl Launcher {
    #[expect(
        clippy::too_many_arguments,
        reason = "A command binds styling, availability, and one action."
    )]
    fn command<F>(
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
        let button = control_label(
            compact_action(id.clone(), p, treatment, disabled, cx),
            label.clone(),
            p,
            treatment,
            disabled,
        );
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

    #[expect(
        clippy::too_many_arguments,
        reason = "The toolbar binds its independent icon, text, and command state in one call."
    )]
    fn toolbar_command(
        &self,
        p: Palette,
        id: &'static str,
        text: &'static str,
        icon: Icon,
        disabled: bool,
        cx: &Context<Self>,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> accessibility::Semantic {
        let ink = if disabled { p.muted } else { p.ink };
        let button = compact_action(id, p, Partition::Neutral, disabled, cx).child(
            row()
                .gap_2()
                .text_color(ink)
                .child(icon.render(px(14.), ink))
                .child(text),
        );
        self.control(button, id, text, Role::Button, disabled, None, cx, action)
    }

    #[allow(clippy::too_many_arguments)]
    fn control<F>(
        &self,
        button: Button,
        id: impl Into<gpui::ElementId>,
        label: &str,
        role: Role,
        disabled: bool,
        selected: Option<bool>,
        cx: &Context<Self>,
        action: F,
    ) -> accessibility::Semantic
    where
        F: Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    {
        let action = Rc::new(action);
        let click = action.clone();
        let weak = cx.weak_entity();
        let button = button.on_click(cx.listener(move |this, _, window, cx| {
            click(this, window, cx);
            cx.notify();
        }));
        self.accessibility.button(
            button,
            id.into(),
            label,
            role,
            disabled,
            selected,
            move |window, cx| {
                let _ = weak.update(cx, |this, cx| {
                    action(this, window, cx);
                    cx.notify();
                });
            },
        )
    }

    fn sidebar(&self, p: Palette, narrow: bool, cx: &Context<Self>) -> AnyElement {
        let query = self.search.read(cx).value().trim().to_lowercase();
        let entries: Vec<AnyElement> = if self.review {
            self.fixtures
                .iter()
                .flat_map(|store| &store.instances)
                .filter(|profile| profile.name.to_lowercase().contains(&query))
                .map(|item| {
                    let id = item.id;
                    self.entry(
                        format!("fixture-{id:?}"),
                        item.name.into(),
                        format!(
                            "Generation {:?} · {}",
                            item.generation,
                            fixture_status(item)
                        ),
                        id == self.fixture,
                        Icon::Instance,
                        p,
                        cx,
                        move |this, _, _| this.choose_fixture(id),
                    )
                })
                .collect()
        } else {
            self.profiles
                .iter()
                .filter(|profile| {
                    format!("{} {}", profile.name, profile.version.id())
                        .to_lowercase()
                        .contains(&query)
                })
                .map(|profile| {
                    let id = profile.id;
                    self.entry(
                        format!("profile-{id}"),
                        profile.name.clone(),
                        format!(
                            "{} {}",
                            if profile.imported_pack.is_some() {
                                "Modpack · Minecraft"
                            } else {
                                "Vanilla"
                            },
                            profile.version.id()
                        ),
                        Some(id) == self.personal,
                        if profile.imported_pack.is_some() {
                            Icon::Pack
                        } else {
                            Icon::Instance
                        },
                        p,
                        cx,
                        move |this, _, cx| this.choose_personal(id, cx),
                    )
                })
                .collect()
        };
        let empty = entries.is_empty();
        let count = if self.review {
            self.fixtures
                .as_ref()
                .map_or(0, |items| items.instances.len())
        } else {
            self.profiles.len()
        };
        let sidebar = col()
            .relative()
            .flex_none()
            .h_full()
            .w(if narrow {
                gpui::relative(1.)
            } else {
                px(self.rail_width).into()
            })
            .when(narrow, |rail| rail.min_w(px(0.)))
            .bg(p.frame)
            .border_r_1()
            .border_color(p.stroke)
            .child(
                row()
                    .px_3()
                    .py_3()
                    .justify_between()
                    .font_semibold()
                    .child(if self.review {
                        "Review fixtures"
                    } else {
                        "Local instances"
                    })
                    .child(div().text_xs().text_color(p.muted).child(count.to_string())),
            )
            .child(div().px_3().child(self.accessibility.input(
                Input::new(&self.search).small(),
                "Find an instance",
                Role::SearchInput,
                &self.search,
                false,
                cx,
            )))
            .child(
                self.accessibility.group(
                    col()
                        .id("instance-list")
                        .flex_1()
                        .min_h_0()
                        .px_3()
                        .py_2()
                        .gap(px(2.))
                        .overflow_y_scroll()
                        .children(entries)
                        .when(empty, |list| {
                            list.child(div().px_2().py_2().text_color(p.muted).child(
                                if query.is_empty() {
                                    "No instances"
                                } else {
                                    "No matching instances"
                                },
                            ))
                        }),
                    "instances",
                    if self.review {
                        "Review fixtures"
                    } else {
                        "Local instances"
                    },
                    Role::ListBox,
                ),
            )
            .child(
                self.accessibility.group(
                    partition_strip(p)
                        .border_t_1()
                        .border_color(p.stroke)
                        .child(
                            self.control(
                                partition(
                                    "library-nav",
                                    p,
                                    if !self.review {
                                        Partition::Selected
                                    } else {
                                        Partition::Neutral
                                    },
                                    self.popup.is_some(),
                                    cx,
                                )
                                .min_w_0()
                                .label("Library"),
                                "library-nav",
                                "Library",
                                Role::Button,
                                self.popup.is_some(),
                                None,
                                cx,
                                |this, window, cx| {
                                    this.switch_view(false, window, cx);
                                },
                            ),
                        )
                        .child(
                            self.control(
                                partition(
                                    "fixture-nav",
                                    p,
                                    if self.review {
                                        Partition::Selected
                                    } else {
                                        Partition::Neutral
                                    },
                                    self.popup.is_some(),
                                    cx,
                                )
                                .min_w_0()
                                .label("Review tools"),
                                "fixture-nav",
                                "Review tools",
                                Role::Button,
                                self.popup.is_some(),
                                None,
                                cx,
                                |this, window, cx| {
                                    this.switch_view(true, window, cx);
                                },
                            ),
                        ),
                    "navigation",
                    "Launcher sections",
                    Role::Group,
                ),
            )
            .when(!narrow, |rail| rail.child(self.rail_resizer(p, cx)));
        self.accessibility
            .group(sidebar, "library", "Instance library", Role::Pane)
            .into_any_element()
    }

    /// A bounded splitter at the rail edge. Dragging adjusts the width, and the
    /// focused handle supports arrow keys, Home/End, and Enter to reset.
    fn rail_resizer(&self, p: Palette, cx: &Context<Self>) -> AnyElement {
        let resize = self.rail_resize_focus.clone();
        let handle = div()
            .id("rail-resizer")
            .absolute()
            .top_0()
            .bottom_0()
            .right(px(-3.))
            .w(px(6.))
            .cursor_col_resize()
            .track_focus(&resize)
            .tab_stop(true)
            .hover(|style| style.bg(p.stroke))
            .focus(|style| style.bg(p.muted))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    this.rail_drag = Some(RailDrag {
                        start_x: f32::from(event.position.x),
                        start_width: this.rail_width,
                    });
                    resize.focus(window);
                    cx.notify();
                }),
            )
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                let Some(width) = rail_width_for_key(
                    this.rail_width,
                    &event.keystroke.key,
                    event.keystroke.modifiers.shift,
                ) else {
                    return;
                };
                this.set_rail_width(width);
                cx.stop_propagation();
                cx.notify();
            }));
        self.accessibility
            .splitter(
                handle,
                "rail-resizer",
                "Resize instance library",
                RAIL_MIN_WIDTH,
                RAIL_MAX_WIDTH,
                self.rail_width,
                self.rail_resize_focus.clone(),
            )
            .into_any_element()
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Each rendered library row has distinct text, selection, icon, and action."
    )]
    fn entry<F>(
        &self,
        id: String,
        name: String,
        subtitle: String,
        selected: bool,
        icon: Icon,
        p: Palette,
        cx: &Context<Self>,
        f: F,
    ) -> AnyElement
    where
        F: Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    {
        let id = SharedString::from(id);
        let label = format!("{name}, {subtitle}");
        let button = Button::new(id.clone())
            .ghost()
            .rounded(px(0.))
            .w_full()
            .h(px(45.))
            .justify_start()
            .px_2()
            .border_0()
            .border_l_2()
            .border_color(if selected { p.accent } else { p.frame })
            .when(selected, |button| button.bg(p.selected))
            .child(
                row().gap_2().min_w_0().child(mark(p, icon, false)).child(
                    col()
                        .min_w_0()
                        .items_start()
                        .child(div().truncate().font_semibold().child(name))
                        .child(div().text_xs().text_color(p.muted).child(subtitle)),
                ),
            );
        self.control(
            button,
            id,
            &label,
            Role::ListBoxOption,
            false,
            Some(selected),
            cx,
            move |this, win, cx| {
                f(this, win, cx);
                this.focus_detail(win);
            },
        )
        .into_any_element()
    }

    fn workspace(&self, p: Palette, narrow: bool, cx: &Context<Self>) -> AnyElement {
        let mut pane = col()
            .id("workspace")
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_y_scroll()
            .bg(p.surface)
            .track_focus(&self.detail_focus)
            .tab_stop(false);
        if narrow && self.show_detail {
            pane = pane.child(command_row(p).border_b_1().border_color(p.stroke).child(
                self.command(
                    p,
                    "back",
                    "‹  Back to library",
                    false,
                    false,
                    cx,
                    |this, window, cx| this.find(window, cx),
                ),
            ));
        }
        let issue = if self.review {
            self.fixture_issue.as_ref()
        } else {
            self.profile_issue.as_ref()
        };
        if let Some(issue) = issue {
            pane = pane.child(
                col()
                    .p_3()
                    .mx_3()
                    .mt_3()
                    .gap_2()
                    .bg(p.layer)
                    .child(self.accessibility.text(
                        div().text_color(p.warning).child(issue.clone()),
                        "load-error",
                        issue.clone(),
                        Role::Alert,
                    ))
                    .child(self.command(
                        p,
                        "reload-error",
                        "Reload library",
                        false,
                        if self.review {
                            self.fixture_busy()
                        } else {
                            self.busy()
                        },
                        cx,
                        |this, _, cx| this.reload(cx),
                    )),
            );
        }
        let pane =
            if self.review {
                let detail = self.accessibility.group(
                    self.fixture_detail(p, cx),
                    format!("fixture-{:?}", self.fixture),
                    self.selected_fixture()
                        .map_or("Review fixture", |item| item.name),
                    Role::Group,
                );
                pane.child(detail).into_any_element()
            } else if let Some(profile) = self.selected_profile() {
                let detail = self.accessibility.group(
                    self.personal_detail(profile, p, narrow, cx),
                    format!("profile-{}", profile.id),
                    &profile.name,
                    Role::Group,
                );
                pane.child(detail).into_any_element()
            } else {
                pane.child(
                    col()
                        .p_6()
                        .gap_3()
                        .child(
                            div()
                                .text_xl()
                                .font_semibold()
                                .child("Set up your first instance"),
                        )
                        .child(div().text_color(p.muted).child(
                            "Create a vanilla profile, then review its Java and game files.",
                        ))
                        .child(self.command(
                            p,
                            "empty-new",
                            "New vanilla instance",
                            true,
                            self.profile_issue.is_some(),
                            cx,
                            |this, window, cx| this.editor(popup::Mode::Create, window, cx),
                        )),
                )
                .into_any_element()
            };
        self.accessibility
            .focus_group(
                pane,
                "detail",
                if self.review {
                    "Review workspace"
                } else {
                    "Instance workspace"
                },
                Role::Main,
                &self.detail_focus,
            )
            .into_any_element()
    }

    fn personal_detail(
        &self,
        profile: &Profile,
        p: Palette,
        narrow: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let setup = self.setup.get(&profile.id);
        let inspection = setup.and_then(|state| state.inspection.as_ref());
        let plan = setup.and_then(|state| state.plan.as_ref());
        let receipt = setup
            .and_then(|state| state.receipt.as_ref())
            .filter(|receipt| {
                inspection.is_some_and(|i| {
                    data::client_verified(i) && i.metadata.metadata_sha1 == receipt.metadata_sha1
                })
            });
        let just_prepared_jdk = inspection.and_then(|report| {
            self.task
                .as_ref()
                .filter(|task| task.profile == Some(profile.id))?
                .activity
                .prepared_jdk_release(&report.metadata.metadata_sha1)
        });
        let jdk_ready = just_prepared_jdk.is_some()
            || inspection.is_some_and(|i| i.managed_java.java_home().is_some());
        let jdk = just_prepared_jdk
            .map(|release| format!("Prepared in app storage · {release}"))
            .or_else(|| {
                inspection.map(|report| match &report.managed_java {
                    RuntimeStatus::Ready { release, .. } => {
                        format!("Prepared in app storage · {release}")
                    }
                    RuntimeStatus::Available { .. } => "Download required".to_owned(),
                    RuntimeStatus::RepairNeeded { .. } => "Repair required".to_owned(),
                    RuntimeStatus::Unsupported { .. } => "Unavailable on this platform".to_owned(),
                })
            })
            .unwrap_or_else(|| "Checked during review".to_owned());
        let settled = setup.is_some_and(Setup::game_files_prepared);
        let cache_only =
            !settled && inspection.is_some_and(|i| i.game_files_cached && data::client_verified(i));
        let client_needs_repair = inspection.is_some_and(|i| {
            i.client_file.status == carbon_vanilla::ClientState::RepairNeeded
                || (i.game_files_cached && !data::client_verified(i))
        });
        let inspected = inspection.is_some();
        let install_activity = self
            .task
            .as_ref()
            .filter(|task| task.profile == Some(profile.id));
        let setup_error = setup.and_then(|state| state.error.as_deref());
        let pack_blocked = pack_has_requirements(profile);
        let pack_summary = profile.imported_pack.as_ref().map(|pack| {
            let pending = match pack.pending_files {
                0 => "Local overrides imported".to_owned(),
                1 => "1 remote file pending".to_owned(),
                count => format!("{count} remote files pending"),
            };
            match &pack.loader {
                Some(loader) => format!("{} · Loader {loader} required · {pending}", pack.name),
                None => format!("{} · {pending}", pack.name),
            }
        });
        let can_change = !self.busy() && self.profile_issue.is_none();
        let (task_title, task_description, task_action) = if pack_blocked {
            (
                "Modpack requirements",
                pack_summary.unwrap_or_default(),
                None,
            )
        } else if let Some(task) = install_activity {
            let cancelling = task.activity.cancel.load(Ordering::Acquire);
            (
                task.label,
                if cancelling {
                    "Cancellation requested…".to_owned()
                } else {
                    task.activity
                        .text()
                        .unwrap_or_else(|| "Starting…".to_owned())
                },
                task.can_cancel.then(|| {
                    self.command(
                        p,
                        "cancel-install",
                        "Cancel",
                        false,
                        cancelling,
                        cx,
                        |this, _, _| this.cancel_task(),
                    )
                }),
            )
        } else if let Some(plan) = plan {
            let jdk_plan = inspection
                .map(|report| match report.managed_java {
                    RuntimeStatus::Available { download_bytes, .. }
                    | RuntimeStatus::RepairNeeded { download_bytes, .. } => {
                        format!(" · Temurin JDK up to {}", size_label(download_bytes))
                    }
                    RuntimeStatus::Ready { .. } => " · Managed JDK ready".to_owned(),
                    RuntimeStatus::Unsupported { .. } => " · Managed JDK unavailable".to_owned(),
                })
                .unwrap_or_default();
            (
                "Review installation",
                format!(
                    "Mojang · {} files · up to {}{}",
                    plan.unique_files,
                    size_label(plan.total_bytes),
                    jdk_plan
                ),
                Some(self.command(
                    p,
                    "install-confirm",
                    if setup_error.is_some() {
                        "Retry installation"
                    } else {
                        "Install instance"
                    },
                    true,
                    !can_change,
                    cx,
                    |this, _, cx| this.prepare_files(false, cx),
                )),
            )
        } else if settled {
            (
                "Game files ready",
                if jdk_ready {
                    self.account.read(cx).play_hint().to_owned()
                } else {
                    format!(
                        "A managed JDK is still required. {}",
                        self.account.read(cx).play_hint()
                    )
                },
                Some(self.command(
                    p,
                    "verify",
                    "Verify game files",
                    false,
                    !can_change,
                    cx,
                    |this, _, cx| this.prepare_files(true, cx),
                )),
            )
        } else if cache_only {
            (
                "Game files are cached",
                "Prepare this instance’s private game folder offline.".to_owned(),
                Some(self.command(
                    p,
                    "prepare-from-cache",
                    "Prepare from cache",
                    true,
                    !can_change,
                    cx,
                    |this, _, cx| this.prepare_files(true, cx),
                )),
            )
        } else {
            (
                if setup_error.is_some() || client_needs_repair {
                    "Installation needs attention"
                } else {
                    "Prepare this instance"
                },
                if client_needs_repair {
                    "The Minecraft client file needs repair. Review the download to replace it."
                } else {
                    "Check the required Java version and review the Mojang download before installing."
                }.to_owned(),
                Some(self.command(
                    p,
                    "review-install",
                    if setup_error.is_some() || client_needs_repair {
                        "Review repair"
                    } else {
                        "Review install"
                    },
                    true,
                    !can_change,
                    cx,
                    |this, _, cx| this.review_install(cx),
                )),
            )
        };
        let main = {
            let task = row()
                .id("profile-task")
                .track_focus(&self.task_focus)
                .items_start()
                .min_w_0()
                .gap_4()
                .border_1()
                .border_color(p.stroke)
                .bg(p.layer)
                .focus(|style| style.border_color(p.accent))
                .px_4()
                .py_4()
                .when(narrow, |layout| layout.flex_col())
                .child(
                    col()
                        .min_w_0()
                        .flex_1()
                        .gap_1()
                        .child(self.accessibility.text(
                            div().text_size(px(17.)).font_semibold().child(task_title),
                            "task-heading",
                            task_title,
                            Role::Heading,
                        ))
                        .child(
                            self.accessibility.text(
                                div()
                                    .text_sm()
                                    .text_color(p.muted)
                                    .child(task_description.clone()),
                                "task-description",
                                task_description,
                                if install_activity.is_some() {
                                    Role::Status
                                } else {
                                    Role::Label
                                },
                            ),
                        )
                        .when_some(
                            setup_error.filter(|_| install_activity.is_none()),
                            |task, error| {
                                task.child(
                                    self.accessibility.text(
                                        div()
                                            .pt_1()
                                            .text_sm()
                                            .font_semibold()
                                            .child(error.to_owned()),
                                        "setup-error",
                                        error,
                                        Role::Alert,
                                    ),
                                )
                            },
                        )
                        .when(settled && profile.imported_pack.is_none(), |task| {
                            task.child(row().child(self.command(
                                p,
                                "review-launch-inputs",
                                "Review installed launch inputs",
                                false,
                                !can_change,
                                cx,
                                |this, window, cx| {
                                    this.task_focus.focus(window);
                                    this.review_launch_inputs(cx);
                                },
                            )))
                        })
                        .when(
                            !settled
                                && install_activity.is_none()
                                && profile.imported_pack.is_none(),
                            |task| {
                                let hint =
                                    "Prepare this instance to review its installed launch inputs.";
                                task.child(self.accessibility.text(
                                    div().text_sm().text_color(p.muted).child(hint),
                                    "launch-input-review-hint",
                                    hint,
                                    Role::Label,
                                ))
                            },
                        )
                        .when_some(
                            setup.and_then(|setup| setup.launch_input_review.as_deref()),
                            |task, message| {
                                task.child(
                                    self.accessibility.text(
                                        div()
                                            .text_sm()
                                            .text_color(p.muted)
                                            .child(message.to_owned()),
                                        "launch-input-review-result",
                                        message,
                                        Role::Status,
                                    ),
                                )
                            },
                        ),
                )
                .when_some(task_action, |task, action| task.child(action));
            col()
                .px_5()
                .py_4()
                .gap_4()
                .child(self.accessibility.focus_group(
                    task,
                    "instance-task",
                    "Current instance task",
                    Role::Group,
                    &self.task_focus,
                ))
                .when_some(profile.imported_pack.as_ref(), |panel, pack| {
                    panel.child(
                        col()
                            .child(detail_row("Pack", pack.name.clone(), p))
                            .child(detail_row(
                                "Loader",
                                pack.loader.clone().unwrap_or_else(|| "Vanilla".into()),
                                p,
                            ))
                            .child(detail_row(
                                "Files pending",
                                pack.pending_files.to_string(),
                                p,
                            )),
                    )
                })
                .when(!pack_blocked, |panel| {
                    panel.child(
                        col()
                            .child(
                                div()
                                    .pb_2()
                                    .text_sm()
                                    .font_semibold()
                                    .child("Installation details"),
                            )
                            .child(detail_row(
                                "Minecraft",
                                if inspected {
                                    format!("{} · Metadata verified", profile.version.id())
                                } else {
                                    profile.version.id().into()
                                },
                                p,
                            ))
                            .child(detail_row("Managed JDK", jdk, p))
                            .child(detail_row(
                                "Game files",
                                receipt
                                    .map(|receipt| format!("{} verified", receipt.unique_files))
                                    .unwrap_or_else(|| {
                                        if settled {
                                            "Private game folder ready".to_owned()
                                        } else if cache_only {
                                            "Cached · Instance folder pending".to_owned()
                                        } else {
                                            "Awaiting installation".to_owned()
                                        }
                                    }),
                                p,
                            )),
                    )
                })
                .when(!pack_blocked, |panel| {
                    panel.child(
                        col()
                            .child(row().child(self.command(
                                p,
                                "toggle-diagnostics",
                                if self.diagnostics_open {
                                    "Hide setup diagnostics"
                                } else {
                                    "Setup diagnostics"
                                },
                                false,
                                self.popup.is_some(),
                                cx,
                                |this, _, _| this.diagnostics_open = !this.diagnostics_open,
                            )))
                            .when(self.diagnostics_open, |panel| {
                                panel.child(self.setup_diagnostics(p, inspection, receipt, cx))
                            }),
                    )
                })
        };
        let body: AnyElement = match self.tab {
            Tab::Overview => {
                let side = self.target_stack(profile, p, narrow, cx);
                div()
                    .flex()
                    .items_start()
                    .min_w_0()
                    .when(narrow, |layout| layout.flex_col())
                    .child(main.flex_1().min_w_0())
                    .child(side)
                    .into_any_element()
            }
            Tab::Storage => main.into_any_element(),
            Tab::Settings => col()
                .child(
                    div()
                        .px_5()
                        .py_4()
                        .text_lg()
                        .font_semibold()
                        .child("Instance settings"),
                )
                .child(
                    col().px_5()
                        .child(detail_row("Minecraft version", profile.version.id().into(), p))
                        .child(detail_row("Maximum memory", format!("{} MiB", profile.memory_mib), p))
                        .child(detail_row("Starting window", format!("{} × {}", profile.window_width, profile.window_height), p)),
                )
                .child(
                    command_row(p)
                        .child(self.command(
                            p,
                            "edit-profile",
                            "Edit instance",
                            true,
                            self.busy(),
                            cx,
                            |this, win, cx| this.editor(popup::Mode::Edit, win, cx),
                        ))
                        .when(profile.imported_pack.is_none(), |row| row.child(self.command(
                            p,
                            "clone-profile",
                            "Clone instance",
                            false,
                            self.busy(),
                            cx,
                            |this, win, cx| this.editor(popup::Mode::Clone, win, cx),
                        ))),
                )
                .child(div().px_5().py_3().font_semibold().child("Files"))
                .child(command_row(p).child(self.command(
                    p,
                    "open-game-folder",
                    "Open game folder",
                    false,
                    self.busy(),
                    cx,
                    |this, _, cx| this.open_game_folder(cx),
                ))).when(profile.imported_pack.is_some(), |settings| settings
                    .child(div().px_5().py_3().text_sm().text_color(p.muted).child("The pack defines the Minecraft version. Import it again to create an independent copy."))
                    .child(command_row(p).child(self.command(p, "reimport-pack", "Import pack", false, self.busy(), cx,
                        |this, window, cx| this.pick(Picker::Pack, window, cx)))))
                .into_any_element(),
        };
        let play_status = if profile.server_address.is_some() {
            "Join unavailable"
        } else if pack_blocked {
            "Pack requirements"
        } else if settled && !jdk_ready {
            "Java required"
        } else if settled && self.account.read(cx).unavailable() {
            "Sign-in unavailable"
        } else if settled && self.account.read(cx).signed_in() {
            "Launch unavailable"
        } else if settled && self.account.read(cx).active_account().is_some() {
            "Account check required"
        } else if settled {
            "Account required"
        } else {
            "Setup required"
        };
        let primary_label = Self::primary_label(profile);
        let play = theme::launch_action(primary_label, p, self.popup.is_some(), cx);
        let play = self
            .control(
                play,
                "play",
                primary_label,
                Role::Button,
                self.popup.is_some(),
                None,
                cx,
                Self::activate_primary,
            )
            .with_focus_outline(p.ink());
        col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_5()
                    .py_4()
                    .bg(p.layer)
                    .child(
                        row()
                            .min_w_0()
                            .flex_1()
                            .gap_3()
                            .child(mark(
                                p,
                                if profile.imported_pack.is_some() {
                                    Icon::Pack
                                } else {
                                    Icon::Instance
                                },
                                !narrow,
                            ))
                            .child(
                                col()
                                    .min_w_0()
                                    .flex_1()
                                    .child(
                                        self.accessibility.text(
                                            div()
                                                .text_size(px(if narrow { 22. } else { 25. }))
                                                .line_height(px(if narrow { 26. } else { 29. }))
                                                .font_semibold()
                                                .truncate()
                                                .child(profile.name.clone()),
                                            "profile-title",
                                            profile.name.clone(),
                                            Role::Heading,
                                        ),
                                    )
                                    .child(div().text_sm().text_color(p.muted).child(
                                        match &profile.imported_pack {
                                            Some(pack) => match &pack.loader {
                                                Some(loader) => format!(
                                                    "Minecraft {} · {loader}",
                                                    profile.version.id()
                                                ),
                                                None => format!(
                                                    "Minecraft {} · Modpack",
                                                    profile.version.id()
                                                ),
                                            },
                                            None => format!(
                                                "Minecraft {} · Vanilla",
                                                profile.version.id()
                                            ),
                                        },
                                    )),
                            ),
                    )
                    .child(col().items_center().gap_1().flex_none().child(play).child(
                        self.accessibility.text(
                            div().text_xs().text_color(p.muted).child(play_status),
                            "play-status",
                            play_status,
                            Role::Label,
                        ),
                    )),
            )
            .child(self.workspace_tabs(&[Tab::Overview, Tab::Settings], p, cx))
            .when(profile.server_address.is_none(), |detail| {
                detail.child(
                    row()
                        .px_5()
                        .py_2()
                        .border_b_1()
                        .border_color(p.stroke)
                        .child(self.command(
                            p,
                            "launch-targets",
                            "Targets…",
                            false,
                            self.popup.is_some()
                                || self.picker_open
                                || self.profile_issue.is_some(),
                            cx,
                            Self::open_targets,
                        )),
                )
            })
            .when(
                self.tab == Tab::Overview && profile.server_address.is_some(),
                |detail| detail.child(self.server_details(profile, p, cx)),
            )
            .child(body)
            .when(self.tab == Tab::Settings, |detail| {
                detail.child(self.server_details(profile, p, cx))
            })
            .into_any_element()
    }

    fn server_details(
        &self,
        profile: &carbon_profiles::Profile,
        p: Palette,
        cx: &Context<Self>,
    ) -> super::accessibility::Semantic {
        let enabled = !self.busy() && self.popup.is_none() && self.profile_issue.is_none();
        let address = profile.server_address.as_ref();
        let guidance = profile.server_join_readiness().message();
        let mut panel = col()
            .id("server-details")
            .track_focus(&self.server_focus)
            .px_5()
            .py_4()
            .gap_2()
            .border_t_1()
            .border_color(p.stroke)
            .focus(|panel| panel.border_color(p.accent))
            .child(self.accessibility.text(
                div().font_semibold().child("Multiplayer"),
                "server-heading",
                "Multiplayer",
                Role::Heading,
            ));
        if let Some(address) = address {
            let address = format!("Server address: {address}");
            let pack = profile.imported_pack.as_ref().map_or_else(
                || format!("Local pack: Vanilla {}", profile.version.id()),
                |pack| {
                    format!(
                        "Local pack: {} · Minecraft {}{} · {} pending file{}",
                        pack.name,
                        profile.version.id(),
                        pack.loader
                            .as_ref()
                            .map_or_else(String::new, |loader| format!(" · {loader}")),
                        pack.pending_files,
                        if pack.pending_files == 1 { "" } else { "s" },
                    )
                },
            );
            panel = panel
                .child(self.accessibility.text(
                    div().min_w_0().overflow_hidden().child(address.clone()),
                    "server-address",
                    address,
                    Role::Label,
                ))
                .child(self.accessibility.text(
                    div().text_sm().text_color(p.muted).child(pack.clone()),
                    "server-pack",
                    pack,
                    Role::Label,
                ));
        }
        panel = panel
            .child(self.accessibility.text(
                div().text_sm().text_color(p.muted).child(guidance),
                "server-readiness",
                guidance,
                Role::Label,
            ))
            .when(address.is_some(), |panel| {
                const MANUAL: &str = "Copy the address and paste it in Multiplayer in a compatible Minecraft installation.";
                panel.child(self.accessibility.text(
                    div().text_sm().text_color(p.muted).child(MANUAL),
                    "server-manual",
                    MANUAL,
                    Role::Label,
                ))
            })
            .child(
                row().flex_wrap().gap_2()
                    .when(address.is_some(), |row| row.child(self.command(
                        p, "copy-server", "Copy server address", false, self.popup.is_some(), cx,
                        |this, _, cx| this.copy_server(cx),
                    )))
                    .child(self.command(
                        p,
                        "edit-server-address",
                        if address.is_some() { "Edit server address" } else { "Set server address" },
                        false,
                        !enabled,
                        cx,
                        |this, window, cx| this.server_editor(window, cx),
                    )),
            );
        self.accessibility.focus_group(
            panel,
            "join-requirements",
            "Multiplayer and join requirements",
            Role::Group,
            &self.server_focus,
        )
    }

    /// Read-only overview panels beside the instance detail. The snapshot is
    /// cached in `targets`; rendering never walks the filesystem.
    fn target_stack(
        &self,
        profile: &Profile,
        p: Palette,
        narrow: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        if profile.server_address.is_some() {
            // A dedicated server instance already shows its address above the tabs.
            return div().into_any_element();
        }
        let mut stack = col().flex_none().gap_3().px_3().py_4();
        stack = if narrow {
            stack.w_full()
        } else {
            stack.w(px(264.))
        };
        let panel = match self.targets.get(&profile.id) {
            Some(TargetsState::Ready(snapshot)) => {
                // The native target chooser exposes the complete lists. Keep the
                // overview inexpensive during resizes even for a full saves directory.
                const PREVIEW_ROWS: usize = 3;
                let panel = |title, total, empty, rows: Vec<(String, String, String)>| {
                    let mut panel = side_panel(p, title, Some(total));
                    if total == 0 {
                        panel = panel.child(
                            self.accessibility.text(
                                div()
                                    .px_3()
                                    .py_2()
                                    .text_sm()
                                    .text_color(p.muted)
                                    .child(empty),
                                format!("empty-{title}"),
                                empty,
                                Role::Label,
                            ),
                        );
                    }
                    for (id, name, secondary) in rows {
                        let spoken = if secondary.is_empty() {
                            name.clone()
                        } else {
                            format!("{name} · {secondary}")
                        };
                        panel = panel.child(
                            self.accessibility.text(
                                col()
                                    .w_full()
                                    .min_w_0()
                                    .px_3()
                                    .py_2()
                                    .border_t_1()
                                    .border_color(p.stroke)
                                    .child(div().w_full().truncate().text_sm().child(name))
                                    .when(!secondary.is_empty(), |row| {
                                        row.child(
                                            div()
                                                .w_full()
                                                .truncate()
                                                .text_xs()
                                                .text_color(p.muted)
                                                .child(secondary),
                                        )
                                    }),
                                id,
                                spoken,
                                Role::Label,
                            ),
                        );
                    }
                    if total > PREVIEW_ROWS {
                        let text = format!("{} more in Targets…", total - PREVIEW_ROWS);
                        panel = panel.child(
                            self.accessibility.text(
                                div()
                                    .px_3()
                                    .py_2()
                                    .text_xs()
                                    .text_color(p.muted)
                                    .child(text.clone()),
                                format!("more-{title}"),
                                text,
                                Role::Label,
                            ),
                        );
                    }
                    panel
                };
                let worlds = panel(
                    "Local worlds",
                    snapshot.worlds.len(),
                    "No local worlds yet.",
                    snapshot
                        .worlds
                        .iter()
                        .take(PREVIEW_ROWS)
                        .map(|world| {
                            let folder = match &world.key {
                                carbon_profiles::TargetKey::World(folder) => folder.as_str(),
                                _ => "",
                            };
                            (
                                format!("world-{folder}"),
                                world.name.clone(),
                                if folder == world.name {
                                    String::new()
                                } else {
                                    folder.to_owned()
                                },
                            )
                        })
                        .collect(),
                );
                let destinations = panel(
                    "Saved servers",
                    snapshot.destinations.len(),
                    "No saved servers yet.",
                    snapshot
                        .destinations
                        .iter()
                        .take(PREVIEW_ROWS)
                        .map(|server| {
                            (
                                format!("server-{}", server.id),
                                server.name.clone(),
                                server.address.as_str().to_owned(),
                            )
                        })
                        .collect(),
                );
                col().gap_3().child(worlds).child(destinations)
            }
            Some(TargetsState::Error(error)) => side_panel(p, "Local saves and destinations", None)
                .child(
                    self.accessibility.text(
                        div()
                            .px_3()
                            .py_3()
                            .text_sm()
                            .text_color(p.muted)
                            .child(error.clone()),
                        "targets-error",
                        error.clone(),
                        Role::Alert,
                    ),
                )
                .child(div().px_3().pb_3().child(self.command(
                    p,
                    "retry-targets",
                    "Retry",
                    false,
                    self.popup.is_some(),
                    cx,
                    |this, _, cx| this.refresh_targets(cx),
                ))),
            Some(TargetsState::Loading(_)) | None => {
                side_panel(p, "Local saves and destinations", None).child(
                    self.accessibility.text(
                        div()
                            .px_3()
                            .py_3()
                            .text_sm()
                            .text_color(p.muted)
                            .child("Reading local saves and saved destinations…"),
                        "targets-loading",
                        "Reading local saves and saved destinations",
                        Role::Status,
                    ),
                )
            }
        };
        stack.child(panel).into_any_element()
    }

    fn setup_diagnostics(
        &self,
        p: Palette,
        inspection: Option<&Inspection>,
        receipt: Option<&GameFilesPrepared>,
        cx: &Context<Self>,
    ) -> gpui::Div {
        let enabled = !self.busy() && self.profile_issue.is_none();
        let controls = row().flex_wrap().gap_2().py_3().child(self.command(
            p,
            "check-setup",
            if inspection.is_some() {
                "Recheck setup"
            } else {
                "Check setup"
            },
            false,
            !enabled,
            cx,
            |this, _, cx| this.check_setup(cx),
        ));
        let panel = col().pt_3();
        let Some(report) = inspection else {
            return panel.child(controls);
        };
        let runtime = &report.runtime;
        let mut java = match runtime.state {
            "compatible" => "Compatible".to_owned(),
            "incompatible" => "Incompatible".to_owned(),
            _ => format!("No compatible Java {} found", report.metadata.java_major),
        };
        for value in [runtime.version.as_deref(), runtime.source]
            .into_iter()
            .flatten()
        {
            java.push_str(" · ");
            java.push_str(value);
        }
        let client = match report.client_file.status {
            carbon_vanilla::ClientState::Verified => "Verified",
            carbon_vanilla::ClientState::RepairNeeded => "Repair needed",
            carbon_vanilla::ClientState::Missing => "Missing",
        };
        let managed = match report.managed_java {
            RuntimeStatus::Available {
                release,
                download_bytes,
                ..
            } => Some((
                "Prepare managed JDK",
                "Download required",
                release,
                download_bytes,
            )),
            RuntimeStatus::Ready {
                release,
                download_bytes,
                ..
            } => Some(("Verify managed JDK", "Ready", release, download_bytes)),
            RuntimeStatus::RepairNeeded {
                release,
                download_bytes,
                ..
            } => Some((
                "Repair managed JDK",
                "Repair required",
                release,
                download_bytes,
            )),
            RuntimeStatus::Unsupported { .. } => None,
        };
        let metadata = &report.metadata;
        let provenance = match metadata.origin {
            carbon_vanilla::Origin::Network => "Mojang",
            carbon_vanilla::Origin::Cache => "Local cache",
        };
        panel
            .child(detail_row("Version source", format!("{} · {provenance}", metadata.release_type), p))
            .child(detail_row("Metadata SHA‑1", metadata.metadata_sha1.clone(), p))
            .child(detail_row("Client size", size_label(metadata.client_bytes), p))
            .child(detail_row("Asset index size", size_label(metadata.asset_index_bytes), p))
            .child(detail_row("Library entries", metadata.library_count.to_string(), p))
            .when_some(receipt, |panel, receipt| panel.child(detail_row("Files reused", size_label(receipt.reused_bytes), p)))
            .child(detail_row("Detected Java", java, p))
            .child(detail_row("Client JAR", client.to_owned(), p))
            .when_some(managed, |panel, (_, state, release, bytes)| {
                panel.child(detail_row("Managed JDK", format!("Java {} · {state} · {release}", metadata.java_major), p))
                    .child(detail_row("JDK download", size_label(bytes), p))
            })
            .child(controls
                .child(self.command(p, "choose-java", "Choose Java folder", false, !enabled, cx,
                    |this, window, cx| this.select_java(window, cx)))
                .when_some(managed, |row, (label, _, _, _)| row.child(self.command(p, "prepare-jdk", label, false, !enabled, cx,
                    |this, _, cx| this.prepare_jdk(cx))))
                .child(self.command(p, "import", if data::client_verified(report) { "Verify client JAR" } else { "Import client JAR" },
                    false, !enabled, cx, |this, window, cx| this.pick(Picker::Client, window, cx)))
                .when(report.game_files_prepared && data::client_verified(report), |row| row.child(
                    self.command(p, "review-repair", "Review repair", false, !enabled, cx, |this, _, cx| this.review_install(cx))))
                .when(report.session_supported && report.managed_java.java_home().is_some(), |row| row.child(
                    self.command(p, "sandbox-test", "Open sandbox test", false, !enabled, cx, |this, _, cx| this.launch_sandbox(cx)))))
            .child(div().text_sm().text_color(p.muted).child(if report.session_supported {
                "A local client JAR is optional. The sandbox test opens a separate window and writes only sample saves."
            } else {
                "A local client JAR is optional."
            }))
    }

    fn workspace_tabs(
        &self,
        tabs: &[Tab],
        p: Palette,
        cx: &Context<Self>,
    ) -> accessibility::Semantic {
        let strip = row()
            .flex_wrap()
            .w_full()
            .gap_2()
            .px_5()
            .bg(p.surface)
            .border_b_1()
            .border_color(p.stroke)
            .children(tabs.iter().copied().enumerate().map(|(index, tab)| {
                let selected = self.tab == tab;
                let focus = self.tab_focus[tab.index()].clone().tab_stop(selected);
                let prev = tabs[(index + tabs.len() - 1) % tabs.len()];
                let next = tabs[(index + 1) % tabs.len()];
                let first = tabs[0];
                let last = tabs[tabs.len() - 1];
                let id = SharedString::from(format!("tab-{tab:?}"));
                let name = match (self.review, tab) {
                    (true, Tab::Overview) => "Updates",
                    (true, Tab::Settings) => "Diagnostics",
                    (_, Tab::Overview) => "Overview",
                    (_, Tab::Settings) => "Settings",
                    (_, Tab::Storage) => "Storage",
                }
                .to_owned();
                let item = row()
                    .id(id.clone())
                    .relative()
                    .h(px(36.))
                    .min_w(px(86.))
                    .px_3()
                    .justify_center()
                    .text_sm()
                    .text_color(if selected { p.ink } else { p.muted })
                    .border_2()
                    .border_color(gpui::transparent_black())
                    .when(selected, |item| {
                        item.font_semibold().child(
                            div()
                                .absolute()
                                .bottom_0()
                                .left_1()
                                .right_1()
                                .h(px(2.))
                                .bg(p.accent),
                        )
                    })
                    .when(self.popup.is_none(), |item| {
                        item.track_focus(&focus)
                            .hover(|style| style.bg(p.layer).text_color(p.ink))
                            .focus(|style| style.border_color(p.accent))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.select_tab(tab, window, cx)
                            }))
                            .on_key_down(cx.listener(
                                move |this, event: &gpui::KeyDownEvent, window, cx| {
                                    let target = match event.keystroke.key.as_str() {
                                        "left" => prev,
                                        "right" => next,
                                        "home" => first,
                                        "end" => last,
                                        _ => return,
                                    };
                                    this.select_tab(target, window, cx);
                                    cx.stop_propagation();
                                },
                            ))
                    })
                    .child(name.clone());
                let weak = cx.weak_entity();
                self.accessibility
                    .button(
                        item,
                        id.into(),
                        name,
                        Role::Tab,
                        self.popup.is_some(),
                        Some(selected),
                        move |window, cx| {
                            let _ = weak.update(cx, |this, cx| this.select_tab(tab, window, cx));
                        },
                    )
                    .with_focus(focus)
            }));
        self.accessibility
            .group(strip, "workspace-tabs", "Workspace sections", Role::TabList)
    }
    fn fixture_detail(&self, p: Palette, cx: &Context<Self>) -> AnyElement {
        let disabled = self.fixture_busy() || self.fixture_issue.is_some();
        let Some(item) = self.selected_fixture() else {
            return col()
                .child(
                    div()
                        .px_5()
                        .py_4()
                        .text_lg()
                        .font_semibold()
                        .child("Synthetic review fixtures"),
                )
                .child(self.command(
                    p,
                    "install-forest",
                    "Install Forest sample",
                    true,
                    disabled,
                    cx,
                    |this, _, cx| this.fixture_action(Action::CreateSample, cx),
                ))
                .into_any_element();
        };
        let id = item.id;
        let operation = if !item.healthy {
            ("Repair files", Action::Repair { instance: id })
        } else {
            match (item.phase, item.generation) {
                (Phase::Idle, Generation::A) => ("Review update", Action::Review { instance: id }),
                (Phase::Idle, Generation::B) => {
                    ("Roll back to A", Action::Rollback { instance: id })
                }
                (Phase::Review, _) => ("Stage update", Action::Stage { instance: id }),
                (Phase::Interrupted, _) => ("Resume update", Action::Resume { instance: id }),
                (Phase::Staged, _) => ("Activate B", Action::Activate { instance: id }),
            }
        };
        let commands = command_row(p)
            .child(self.command(
                p,
                "fixture-primary",
                operation.0,
                true,
                disabled,
                cx,
                move |this, _, cx| this.fixture_action(operation.1, cx),
            ))
            .child(self.command(
                p,
                "fixture-play",
                "Check Play requirements",
                false,
                disabled,
                cx,
                |this, _, cx| this.check_fixture_play(cx),
            ))
            .when(item.healthy, |r| {
                r.child(self.command(
                    p,
                    "fixture-repair",
                    "Repair files",
                    false,
                    disabled,
                    cx,
                    move |this, _, cx| this.fixture_action(Action::Repair { instance: id }, cx),
                ))
            })
            .when(item.phase != Phase::Idle, |r| {
                r.child(self.command(
                    p,
                    "fixture-cancel",
                    "Cancel update",
                    false,
                    disabled,
                    cx,
                    move |this, _, cx| this.fixture_action(Action::Cancel { instance: id }, cx),
                ))
            })
            .when(matches!(item.phase, Phase::Review | Phase::Staged), |r| {
                r.child(self.command(
                    p,
                    "fixture-interrupt",
                    "Simulate interruption",
                    false,
                    disabled,
                    cx,
                    move |this, _, cx| this.fixture_action(Action::Interrupt { instance: id }, cx),
                ))
            });
        let review = item.review.as_ref().map(|diff| {
            format!(
                "Add {} · Remove {} · Reuse {}",
                diff.added.join(", "),
                diff.removed.join(", "),
                diff.retained.join(", ")
            )
        });
        let store = self.fixtures.as_ref().expect("fixture has a snapshot");
        let body = match self.tab {
            Tab::Overview => col().px_5().py_4().gap_3()
                .child(div().font_semibold().text_lg().child("Update and recovery"))
                .child(commands)
                .when(self.fixture_play_checked == Some(id), |pane| pane.child(
                    div().p_3().bg(p.layer).child("This sample contains synthetic files and cannot launch Minecraft. You can update or repair it here.")
                ))
                .when_some(review, |pane, review| pane.child(col().gap_2()
                    .child(div().font_semibold().child("Generation A → B"))
                    .child(div().text_sm().text_color(p.muted).child(review)))),
            Tab::Storage => col().px_5().py_4().gap_3()
                .child(div().font_semibold().text_lg().child("Content reuse"))
                .child(div().text_sm().text_color(p.muted).child("Forest Repeat reuses the same objects. Coast shares most of Forest’s immutable bytes. Each has its own save and settings."))
                .child(col()
                    .child(detail_row("Fixture bytes read", size_label(store.metrics.source_bytes), p))
                    .child(detail_row("Bytes reused", size_label(store.metrics.reused_bytes), p))
                    .child(detail_row("Object allocation", store.metrics.allocated_object_bytes.map(size_label).unwrap_or_else(|| "Unavailable".into()), p))
                    .child(detail_row("Active size", size_label(store.metrics.active_logical_bytes), p)))
                .child(row().flex_wrap().gap_2()
                    .child(self.command(p, "install-repeat", "Install exact repeat", false,
                        disabled || store.instances.iter().any(|i| i.id == InstanceId::Repeat), cx,
                        |this, _, cx| this.fixture_action(Action::CreateRepeat, cx)))
                    .child(self.command(p, "install-coast", "Install Coast sample", false,
                        disabled || store.instances.iter().any(|i| i.id == InstanceId::Overlap), cx,
                        |this, _, cx| this.fixture_action(Action::CreateOverlap, cx)))),
            Tab::Settings => col().px_5().py_4().gap_3()
                .child(div().font_semibold().text_lg().child("Private fixture data"))
                .child(col()
                    .child(detail_row("Sample setting", item.setting.to_string(), p))
                    .child(detail_row("Save revision", item.save_revision.to_string(), p)))
                .child(div().text_sm().text_color(p.muted).child("A save edited under B requires B to reopen it."))
                .child(row().flex_wrap().gap_2()
                    .child(self.command(p, "fixture-setting", "Change sample setting", false, disabled, cx,
                        move |this, _, cx| this.fixture_action(Action::EditSetting { instance: id }, cx)))
                    .child(self.command(p, "fixture-save", "Edit sample save", false, disabled, cx,
                        move |this, _, cx| this.fixture_action(Action::EditSave { instance: id }, cx))))
                .child(div().pt_3().font_semibold().child("Fault simulation"))
                .child(div().text_sm().text_color(p.muted).child("These operations affect shared fixture content. Repair restores it for every sample using those bytes."))
                .child(row().flex_wrap().gap_2()
                    .child(self.command(p, "fixture-missing", "Simulate missing file", false, disabled, cx,
                        move |this, _, cx| this.fixture_action(Action::SimulateMissing { instance: id }, cx)))
                    .child(self.command(p, "fixture-corrupt", "Simulate corrupt file", false, disabled, cx,
                        move |this, _, cx| this.fixture_action(Action::SimulateCorruption { instance: id }, cx)))),
        };
        col()
            .child(
                col()
                    .px_5()
                    .py_4()
                    .gap_1()
                    .bg(p.layer)
                    .child(
                        div()
                            .text_size(px(25.))
                            .line_height(px(29.))
                            .font_semibold()
                            .child(item.name),
                    )
                    .child(div().text_sm().text_color(p.muted).child(format!(
                        "Synthetic fixture · Generation {:?} · {}",
                        item.generation,
                        fixture_status(item)
                    ))),
            )
            .child(self.workspace_tabs(&[Tab::Overview, Tab::Storage, Tab::Settings], p, cx))
            .child(body)
            .into_any_element()
    }
}

impl Render for Launcher {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = Palette::read(self.preference, window.appearance());
        if self.applied_dark != Some(p.dark) {
            self.applied_dark = Some(p.dark);
            p.apply(window, cx);
            shell::apply_frame(window, p.dark);
            #[cfg(windows)]
            if let Some(menu) = &self.native_menu {
                menu.apply_appearance(p.dark);
            }
        }
        self.frame_ready(window);
        let narrow = is_narrow(window);
        let account = self.account.read(cx);
        let account_name = account.active_account().map(|active| active.name.clone());
        let have_detail = if self.review {
            self.selected_fixture().is_some()
        } else {
            self.selected_profile().is_some()
        };
        let inline_activity = !self.review
            && self.tab == Tab::Overview
            && (!narrow || self.show_detail)
            && self
                .task
                .as_ref()
                .is_some_and(|task| task.profile.is_some() && task.profile == self.personal);
        let activity_name = self
            .activity_target()
            .and_then(|target| self.activity_name(target));
        let activity = self
            .task
            .as_ref()
            .or(self.fixture_task.as_ref())
            .map(|task| {
                if inline_activity {
                    self.selected_profile().map_or_else(
                        || "Instance task running".to_owned(),
                        |profile| format!("Working on {}", profile.name),
                    )
                } else if task.activity.cancel.load(Ordering::Acquire) {
                    "Cancellation requested…".to_owned()
                } else {
                    task.activity
                        .text()
                        .unwrap_or_else(|| task.label.to_owned())
                }
            })
            .unwrap_or_else(|| self.message.clone());
        let activity = match activity_name {
            Some(name) if !inline_activity => format!("{name} · {activity}"),
            _ => activity,
        };
        let frame = col()
            .id("launcher-frame")
            .size_full()
            .bg(p.frame)
            .text_color(p.ink())
            .text_size(px(14.))
            .font_family(if cfg!(windows) {
                "Segoe UI"
            } else {
                ".SystemUIFont"
            })
            .track_focus(&self.root_focus)
            .key_context("CarbonDesktop")
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                if !event.dragging() {
                    if this.rail_drag.take().is_some() {
                        cx.notify();
                    }
                    return;
                }
                let Some((start_x, start_width)) = this
                    .rail_drag
                    .as_ref()
                    .map(|drag| (drag.start_x, drag.start_width))
                else {
                    return;
                };
                let width = start_width + (f32::from(event.position.x) - start_x);
                if clamp_rail_width(width) != this.rail_width {
                    this.set_rail_width(width);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.rail_drag.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.rail_drag.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            .when(self.popup.is_some(), |owner| {
                owner
                    .capture_any_mouse_down(|_, _, cx| cx.stop_propagation())
                    .capture_any_mouse_up(|_, _, cx| cx.stop_propagation())
            })
            .on_action(cx.listener(|this, _: &shell::CloseLauncher, win, cx| {
                // Share the close guard with native menu commands.
                this.close_from_menu(win, cx)
            }))
            .on_action(cx.listener(|this, _: &shell::OpenFile, win, cx| {
                this.open_shell_menu(shell::Page::File, win, cx)
            }))
            .on_action(cx.listener(|this, _: &shell::OpenView, win, cx| {
                this.open_shell_menu(shell::Page::View, win, cx)
            }))
            .on_action(
                cx.listener(|this, _: &FindInstance, win, cx| this.reveal_library(win, cx, true)),
            )
            .on_action(cx.listener(|this, _: &NewInstance, win, cx| {
                this.editor(popup::Mode::Create, win, cx)
            }))
            .on_action(
                cx.listener(|this, _: &ImportPack, win, cx| this.pick(Picker::Pack, win, cx)),
            )
            .on_action(cx.listener(|this, _: &Back, win, cx| {
                if back_returns_to_detail(
                    this.search_back_to_detail,
                    this.show_detail,
                    is_narrow(win),
                ) {
                    this.search_back_to_detail = false;
                    this.show_detail = true;
                    this.detail_focus.focus(win);
                    cx.notify();
                } else if this.show_detail {
                    this.search_back_to_detail = false;
                    this.find(win, cx);
                } else {
                    this.search_back_to_detail = false;
                }
            }))
            .on_action(
                cx.listener(|this, _: &FocusActivity, win, _| this.activity_focus.focus(win)),
            )
            .child(
                row()
                    .flex_wrap()
                    .px_3()
                    .py_2()
                    .gap_2()
                    .border_b_1()
                    .border_color(p.stroke)
                    .child(self.toolbar_command(
                        p,
                        "new",
                        "New instance",
                        Icon::Add,
                        self.busy(),
                        cx,
                        |this, win, cx| this.editor(popup::Mode::Create, win, cx),
                    ))
                    .when(cfg!(target_os = "linux"), |bar| {
                        bar.child(self.command(
                            p,
                            "file-menu",
                            "File",
                            false,
                            self.popup.is_some(),
                            cx,
                            |this, win, cx| this.open_shell_menu(shell::Page::File, win, cx),
                        ))
                        .child(self.command(
                            p,
                            "view-menu",
                            "View",
                            false,
                            self.popup.is_some(),
                            cx,
                            |this, win, cx| this.open_shell_menu(shell::Page::View, win, cx),
                        ))
                    })
                    .child(div().flex_1())
                    .when_some(account_name.filter(|_| !narrow), |bar, name| {
                        bar.child(
                            self.accessibility.text(
                                div()
                                    .max_w(px(155.))
                                    .truncate()
                                    .text_sm()
                                    .text_color(p.muted)
                                    .child(name.clone()),
                                "active-account-name",
                                name,
                                Role::Label,
                            ),
                        )
                    })
                    .child(self.toolbar_command(
                        p,
                        "microsoft-account",
                        account.toolbar_label(),
                        Icon::Account,
                        self.popup.is_some() || self.picker_open,
                        cx,
                        |this, window, cx| this.open_account(window, cx),
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .when(!narrow || !self.show_detail || !have_detail, |body| {
                        body.child(self.sidebar(p, narrow, cx))
                    })
                    .when(!narrow || self.show_detail, |body| {
                        body.child(self.workspace(p, narrow, cx))
                    }),
            )
            .child(
                self.accessibility.focus_group(
                    div()
                        .flex()
                        .id("activity")
                        .track_focus(&self.activity_focus)
                        .border_t_1()
                        .border_color(p.stroke)
                        .focus(|style| style.border_color(p.accent))
                        .child(
                            row()
                                .px_3()
                                .py_2()
                                .gap_2()
                                .flex_1()
                                .min_w_0()
                                .child(div().text_xs().font_semibold().child("Activity"))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_xs()
                                        .text_color(p.muted)
                                        .child(self.accessibility.text(
                                            div().child(activity.clone()),
                                            "activity-status",
                                            activity,
                                            Role::Status,
                                        )),
                                ),
                        )
                        .when(activity_name.is_some() && !inline_activity, |bar| {
                            bar.child(div().px_2().py_1().child(self.command(
                                p,
                                "activity-target",
                                "Show task",
                                false,
                                self.popup.is_some() || self.picker_open,
                                cx,
                                |this, window, cx| this.show_activity_target(window, cx),
                            )))
                        })
                        .when(
                            !inline_activity
                                && self.task.as_ref().is_some_and(|task| task.can_cancel),
                            |bar| {
                                bar.child(
                                    div()
                                        .flex()
                                        .w(px(124.))
                                        .border_l_1()
                                        .border_color(p.stroke)
                                        .child(self.command(
                                            p,
                                            "cancel-job",
                                            "Cancel",
                                            false,
                                            self.task.as_ref().is_some_and(|task| {
                                                task.activity.cancel.load(Ordering::Acquire)
                                            }),
                                            cx,
                                            |this, _, _| this.cancel_task(),
                                        )),
                                )
                            },
                        ),
                    "activity",
                    "Activity",
                    Role::Group,
                    &self.activity_focus,
                ),
            );
        self.accessibility
            .frame(frame, self.popup.is_some() || self.picker_open)
    }
}

fn mark(p: Palette, icon: Icon, large: bool) -> gpui::Div {
    div()
        .flex_none()
        .size(px(if large { 38. } else { 24. }))
        .when(large, |mark| {
            mark.bg(p.surface).border_1().border_color(p.stroke)
        })
        .flex()
        .items_center()
        .justify_center()
        .child(icon.render(px(if large { 22. } else { 15. }), p.ink))
}

fn detail_row(label: &'static str, status: String, p: Palette) -> gpui::Div {
    row()
        .min_h(px(34.))
        .gap_3()
        .py_1()
        .border_t_1()
        .border_color(p.stroke)
        .text_sm()
        .child(div().w(px(132.)).flex_none().font_semibold().child(label))
        .child(div().min_w_0().flex_1().text_color(p.muted).child(status))
}

fn side_panel(p: Palette, title: &'static str, count: Option<usize>) -> gpui::Div {
    col()
        .w_full()
        .min_w_0()
        .bg(p.surface)
        .border_1()
        .border_color(p.stroke)
        .child(
            row()
                .w_full()
                .min_w_0()
                .gap_3()
                .justify_between()
                .px_3()
                .py_2()
                .bg(p.layer)
                .border_b_1()
                .border_color(p.stroke)
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_sm()
                        .font_semibold()
                        .child(title),
                )
                .when_some(count, |header, count| {
                    header.child(div().text_xs().text_color(p.muted).child(count.to_string()))
                }),
        )
}

fn fixture_status(item: &InstanceView) -> &'static str {
    if !item.healthy {
        return "Repair needed";
    }
    match item.phase {
        Phase::Idle => "Installed",
        Phase::Review => "Awaiting review",
        Phase::Interrupted => "Interrupted",
        Phase::Staged => "Awaiting activation",
    }
}
