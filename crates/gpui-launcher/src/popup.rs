/* SPDX-License-Identifier: MPL-2.0 */
mod account;

use super::theme::{Partition, compact_action, label, partition, partition_strip};
use super::{Launcher, account_panel::AccountPanel};
use accesskit::Role;
use account::AccountDialog;
use carbon_pack::PackReview;
use carbon_profiles::{Profile, ProfileAction, ServerAddress, VanillaVersion};
use gpui::{
    AnyWindowHandle, Bounds, Context, Entity, FocusHandle, TitlebarOptions, WeakEntity, Window,
    WindowBounds, WindowDecorations, WindowOptions, actions, div, prelude::*, px, size,
};
use gpui_component::{
    Root, Sizable, StyledExt,
    input::{Input, InputEvent, InputState},
};
use std::{cell::Cell, path::PathBuf, rc::Rc};

actions!(popup, [Close, Save]);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Create,
    Edit,
    Clone,
}

pub enum Kind {
    Profile(Mode, Option<Profile>),
    Server(Profile),
    ImportPack(PathBuf, PackReview),
    Account(Entity<AccountPanel>),
    Notice {
        title: &'static str,
        message: &'static str,
    },
}

struct Form {
    mode: Mode,
    original: Option<Profile>,
    name: Entity<InputState>,
    memory: Entity<InputState>,
    width: Entity<InputState>,
    height: Entity<InputState>,
    version: VanillaVersion,
}

struct ServerForm {
    original: Profile,
    input: Entity<InputState>,
}

struct Popup {
    accessibility: super::accessibility::Accessibility,
    parent: AnyWindowHandle,
    owner: WeakEntity<Launcher>,
    root_path: PathBuf,
    palette: super::theme::Palette,
    focus: FocusHandle,
    form: Option<Form>,
    server: Option<ServerForm>,
    pack: Option<(PathBuf, PackReview)>,
    notice: Option<&'static str>,
    account: Option<AccountDialog>,
    importing: bool,
    error: Option<String>,
    native_owner: Option<isize>,
}

fn title(kind: &Kind) -> &'static str {
    match kind {
        Kind::Profile(Mode::Create, _) => "New vanilla instance",
        Kind::Profile(Mode::Edit, _) => "Edit instance",
        Kind::Profile(Mode::Clone, _) => "Clone instance",
        Kind::Server(_) => "Edit server address",
        Kind::ImportPack(..) => "Import modpack",
        Kind::Account(_) => "Minecraft account",
        Kind::Notice { title, .. } => title,
    }
}

pub fn open(launcher: &mut Launcher, kind: Kind, window: &mut Window, cx: &mut Context<Launcher>) {
    let label = title(&kind);
    let parent = window.window_handle();
    let owner = cx.weak_entity();
    let restore_focus = window.focused(cx);
    let root_path = launcher.root.path.clone();
    let palette = super::theme::Palette::read(launcher.preference, window.appearance());
    let parent_hwnd = native_handle(window);
    let attached = Rc::new(Cell::new(false));
    let result = attached.clone();
    let width = px(480.);
    let (height, minimum_height) = match &kind {
        Kind::Profile(..) => (417., 340.),
        Kind::Server(..) => (262., 220.),
        Kind::ImportPack(..) => (425., 340.),
        Kind::Account(panel) if panel.read(cx).unavailable() => (244., 214.),
        Kind::Account(_) => (430., 280.),
        Kind::Notice { .. } => (190., 156.),
    };
    let bounds = Bounds::centered_at(window.bounds().center(), size(width, px(height)));
    let outcome = cx.open_window(
        WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some(label.into()),
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(330.), px(minimum_height))),
            #[cfg(target_os = "linux")]
            kind: gpui::WindowKind::Floating,
            window_decorations: Some(WindowDecorations::Server),
            app_id: Some("dev.carbonmine.gpui".into()),
            is_minimizable: false,
            show: !cfg!(windows),
            focus: !cfg!(windows),
            ..Default::default()
        },
        move |window, cx| {
            set_app_icon(window);
            super::shell::apply_frame(window, palette.dark);
            let native_owner = attach(parent_hwnd, window);
            attached.set(native_owner.is_ok());
            if native_owner.is_err() {
                window.remove_window();
            }
            let popup = cx.new(|cx| {
                Popup::new(
                    kind,
                    parent,
                    owner,
                    root_path,
                    palette,
                    native_owner.ok().flatten(),
                    restore_focus,
                    window,
                    cx,
                )
            });
            let weak = popup.downgrade();
            window.on_window_should_close(cx, move |_, cx| {
                weak.upgrade().is_none_or(|popup| !popup.read(cx).importing)
            });
            let root = cx.new(|cx| Root::new(popup, window, cx));
            #[cfg(windows)]
            if attached.get() {
                window.activate_window();
            }
            root
        },
    );
    match outcome {
        Ok(popup) if result.get() => {
            launcher.popup = Some(popup.into());
        }
        _ => {
            launcher.message =
                "Could not open an owned native window. Retry from the library.".into();
        }
    }
    cx.notify();
}

impl Popup {
    #[allow(clippy::too_many_arguments)]
    fn new(
        kind: Kind,
        parent: AnyWindowHandle,
        owner: WeakEntity<Launcher>,
        root_path: PathBuf,
        palette: super::theme::Palette,
        native_owner: Option<isize>,
        restore_focus: Option<FocusHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let accessibility =
            super::accessibility::Accessibility::new(window, cx, title(&kind).into());
        let focus = cx.focus_handle();
        let mut server = None;
        let (form, pack, notice, account) = match kind {
            Kind::Profile(mode, original) => {
                let name = original
                    .as_ref()
                    .map(|profile| {
                        if mode == Mode::Clone {
                            format!(
                                "{} copy",
                                profile.name.chars().take(75).collect::<String>().trim_end()
                            )
                        } else {
                            profile.name.clone()
                        }
                    })
                    .unwrap_or_default();
                let memory = original.as_ref().map_or(2048, |p| p.memory_mib);
                let width = original.as_ref().map_or(1280, |p| p.window_width);
                let height = original.as_ref().map_or(720, |p| p.window_height);
                let version = original
                    .as_ref()
                    .map_or(VanillaVersion::Release26_3, |p| p.version);
                let name = cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder("Instance name")
                        .default_value(name)
                });
                let memory =
                    cx.new(|cx| InputState::new(window, cx).default_value(memory.to_string()));
                let width =
                    cx.new(|cx| InputState::new(window, cx).default_value(width.to_string()));
                let height =
                    cx.new(|cx| InputState::new(window, cx).default_value(height.to_string()));
                for input in [&name, &memory, &width, &height] {
                    cx.subscribe_in(input, window, |this, _, event: &InputEvent, window, cx| {
                        if matches!(event, InputEvent::PressEnter { .. }) {
                            this.commit(window, cx);
                        }
                    })
                    .detach();
                }
                name.update(cx, |input, cx| input.focus(window, cx));
                (
                    Some(Form {
                        mode,
                        original,
                        name,
                        memory,
                        width,
                        height,
                        version,
                    }),
                    None,
                    None,
                    None,
                )
            }
            Kind::Server(original) => {
                let address = original
                    .server_address
                    .as_ref()
                    .map_or("", ServerAddress::as_str);
                let input = cx.new(|cx| {
                    InputState::new(window, cx)
                        .placeholder("Hostname or IP address")
                        .default_value(address.to_owned())
                });
                cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.commit(window, cx);
                    }
                })
                .detach();
                input.update(cx, |input, cx| input.focus(window, cx));
                server = Some(ServerForm { original, input });
                (None, None, None, None)
            }
            Kind::ImportPack(path, review) => {
                focus.focus(window);
                (None, Some((path, review)), None, None)
            }
            Kind::Account(panel) => {
                focus.focus(window);
                (None, None, None, Some(AccountDialog::new(panel, cx)))
            }
            Kind::Notice { message, .. } => {
                focus.focus(window);
                (None, None, Some(message), None)
            }
        };
        cx.on_release(move |this, cx| {
            if let Some(account) = &this.account {
                account.panel.update(cx, AccountPanel::dialog_closed);
            }
            let _ = this.owner.update(cx, |launcher, cx| {
                launcher.popup = None;
                cx.notify();
            });
            let _ = this.parent.update(cx, |_, window, _| {
                if native_handle(window) == this.native_owner {
                    enable(this.native_owner);
                }
                window.activate_window();
                if let Some(focus) = &restore_focus {
                    focus.focus(window);
                }
            });
        })
        .detach();
        Self {
            accessibility,
            parent,
            owner,
            root_path,
            palette,
            focus,
            form,
            server,
            pack,
            notice,
            account,
            importing: false,
            error: None,
            native_owner,
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Keep the dialog button and its accessibility binding in sync."
    )]
    fn control(
        &self,
        element: gpui_component::button::Button,
        id: impl Into<gpui::ElementId>,
        name: &str,
        role: Role,
        disabled: bool,
        selected: Option<bool>,
        cx: &Context<Self>,
        f: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> super::accessibility::Semantic {
        let f = Rc::new(f);
        let click = f.clone();
        let button =
            element.on_click(cx.listener(move |this, _, window, cx| click(this, window, cx)));
        let weak = cx.weak_entity();
        self.accessibility.button(
            button,
            id.into(),
            name,
            role,
            disabled,
            selected,
            move |window, cx| {
                let _ = weak.update(cx, |this, cx| f(this, window, cx));
            },
        )
    }

    fn commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(form) = &self.server {
            let result = ServerAddress::parse_optional(form.input.read(cx).value().as_str())
                .and_then(|address| {
                    carbon_profiles::set_server_address(
                        &self.root_path,
                        form.original.id,
                        form.original.server_address.as_ref(),
                        address,
                    )
                });
            match result {
                Ok(profile) => {
                    self.server = None;
                    let _ = self.owner.update(cx, |launcher, cx| {
                        launcher.server_saved(profile, cx);
                    });
                    window.remove_window();
                }
                Err(error) => {
                    self.error = Some(error);
                    cx.notify();
                }
            }
            return;
        }
        let Some(form) = &self.form else {
            return;
        };
        let name = form.name.read(cx).value().to_string();
        let read = |input: &Entity<InputState>, field: &str| {
            input
                .read(cx)
                .value()
                .trim()
                .parse::<u32>()
                .map_err(|_| format!("{field} needs a whole number."))
        };
        let action = if form.mode == Mode::Clone {
            Ok(ProfileAction::Clone {
                id: form.original.as_ref().expect("clone source").id,
                name,
            })
        } else {
            let preferences = || {
                Ok((
                    read(&form.memory, "Maximum memory")?,
                    read(&form.width, "Window width")?,
                    read(&form.height, "Window height")?,
                ))
            };
            preferences().map(
                |(memory_mib, window_width, window_height)| match form.mode {
                    Mode::Edit => ProfileAction::Edit(Profile {
                        id: form.original.as_ref().expect("edit source").id,
                        imported_pack: form
                            .original
                            .as_ref()
                            .and_then(|profile| profile.imported_pack.clone()),
                        server_address: form
                            .original
                            .as_ref()
                            .and_then(|profile| profile.server_address.clone()),
                        name,
                        version: form.version,
                        memory_mib,
                        window_width,
                        window_height,
                    }),
                    _ => ProfileAction::Create {
                        name,
                        version: form.version,
                        memory_mib,
                        window_width,
                        window_height,
                    },
                },
            )
        };
        let result = action.and_then(|action| {
            carbon_profiles::change_if_current(&self.root_path, action, form.original.as_ref())
        });
        match result {
            Ok(profiles) => {
                let selected = match form.mode {
                    Mode::Edit => form.original.as_ref(),
                    _ => profiles.last(),
                }
                .map(|profile| profile.id);
                if let Some(id) = selected {
                    let _ = self.owner.update(cx, move |launcher, cx| {
                        launcher.set_profiles(profiles, id, cx)
                    });
                    window.remove_window();
                }
            }
            Err(error) => {
                self.error = Some(error);
                cx.notify();
            }
        }
    }
    fn import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.importing {
            return;
        }
        let Some((source, review)) = &self.pack else {
            return;
        };
        self.importing = true;
        self.error = None;
        let root = self.root_path.clone();
        let source = source.clone();
        let digest = review.sha256.clone();
        let import_id = review.import_id;
        let _ = self.owner.update(cx, |launcher, cx| {
            launcher.picker_open = true;
            cx.notify();
        });
        let work = cx.background_executor().spawn(async move {
            carbon_pack::import_reviewed(&root, &source, &digest, import_id)
                .map(|profile| (profile, carbon_profiles::list(&root)))
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.importing = false;
                let _ = this.owner.update(cx, |launcher, cx| {
                    launcher.picker_open = false;
                    cx.notify();
                });
                match result {
                    Ok((profile, profiles)) => {
                        let _ = this.owner.update(cx, |launcher, cx| {
                            launcher.pack_imported(profile, profiles, cx);
                            cx.notify();
                        });
                        let _ = this.parent.update(cx, |_, parent, cx| {
                            let _ = this.owner.update(cx, |launcher, cx| {
                                launcher
                                    .search
                                    .update(cx, |input, cx| input.set_value("", parent, cx));
                                launcher.focus_detail(parent);
                            });
                        });
                        window.remove_window();
                    }
                    Err(error) => {
                        this.error = Some(error);
                        cx.notify();
                    }
                }
            });
        })
        .detach();
        cx.notify();
    }
}

impl Render for Popup {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let body = div()
            .flex()
            .flex_col()
            .size_full()
            .bg(self.palette.surface)
            .text_color(self.palette.ink())
            .track_focus(&self.focus)
            .key_context("CarbonPopup")
            .on_action(cx.listener(|this, _: &Close, window, cx| {
                if !this.importing {
                    this.account_dismiss(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Save, window, cx| {
                if this.form.is_some() || this.server.is_some() {
                    this.commit(window, cx);
                } else if this.notice.is_some() {
                    window.remove_window();
                } else {
                    cx.propagate();
                }
            }));
        let row = |label: &'static str, input: &Entity<InputState>| {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_sm().child(label))
                .child(self.accessibility.input(
                    Input::new(input).small().bordered(true),
                    label,
                    Role::TextInput,
                    input,
                    false,
                    cx,
                ))
        };
        let mut content = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .id("popup-content")
            .overflow_y_scroll()
            .p_5()
            .gap_3();
        if let Some(form) = &self.form {
            content = content.child(row("Instance name", &form.name));
            if form.mode == Mode::Clone {
                content = content
                    .child("Copy this instance’s preferences. Game data and saves stay separate.");
                if form
                    .original
                    .as_ref()
                    .is_some_and(|profile| profile.server_address.is_some())
                {
                    content = content.child("The copy starts without a server address.");
                }
            } else {
                content = content
                    .child("Minecraft version")
                    .when(
                        form.original
                            .as_ref()
                            .is_some_and(|profile| profile.imported_pack.is_some()),
                        |content| {
                            content.child(
                                div()
                                    .text_color(p.muted)
                                    .child(format!("{} · Fixed by the pack", form.version.id())),
                            )
                        },
                    )
                    .when(
                        form.original
                            .as_ref()
                            .is_none_or(|profile| profile.imported_pack.is_none()),
                        |content| {
                            content.child(partition_strip(p).children(VanillaVersion::ALL.map(
                                |version| {
                                    let treatment = if version == form.version {
                                        Partition::Selected
                                    } else {
                                        Partition::Neutral
                                    };
                                    let option = label(
                                        partition(version.id(), p, treatment, false, cx)
                                            .h(px(36.))
                                            .min_w(px(62.))
                                            .px_1(),
                                        version.id(),
                                        p,
                                        treatment,
                                        false,
                                    );
                                    self.control(
                                        option,
                                        version.id(),
                                        version.id(),
                                        Role::RadioButton,
                                        false,
                                        Some(version == form.version),
                                        cx,
                                        move |this, _, cx| {
                                            if let Some(form) = &mut this.form {
                                                form.version = version;
                                            }
                                            cx.notify();
                                        },
                                    )
                                },
                            )))
                        },
                    )
                    .child(row("Maximum memory (MiB)", &form.memory))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_3()
                            .child(row("Window width", &form.width).flex_1().min_w(px(124.)))
                            .child(row("Window height", &form.height).flex_1().min_w(px(124.))),
                    );
            }
        } else if let Some(server) = &self.server {
            content =
                content
                    .child(row("Server address", &server.input))
                    .child(self.accessibility.text(
                    div().text_sm().text_color(p.muted).child(
                        "Example: play.example.org:25565. Clear the field to remove the binding.",
                    ),
                    "server-help",
                    "Example: play.example.org:25565. Clear the field to remove the binding.",
                    Role::Label,
                ));
        } else if let Some((_, review)) = &self.pack {
            content =
                content
                    .child(self.accessibility.text(
                        div().text_lg().font_semibold().child(review.name.clone()),
                        "pack-title",
                        review.name.clone(),
                        Role::Heading,
                    ))
                    .children(review.description.lines().skip(1).enumerate().map(
                        |(position, line)| {
                            self.accessibility.text(
                                div().child(line.to_owned()),
                                format!("pack-detail-{position}"),
                                line,
                                Role::Label,
                            )
                        },
                    ))
                    .child(
                        div()
                            .text_color(p.muted)
                            .child("Import copies these overrides into a separate local instance."),
                    );
        }
        if let Some(account) = &self.account {
            content = content.child(self.account_content(account, cx));
        }
        if let Some(message) = self.notice {
            content = content.child(self.accessibility.text(
                div().child(message),
                "notice-message",
                message,
                Role::Alert,
            ));
        }
        if let Some(error) = &self.error {
            let reason = if self.pack.is_some() {
                format!("Could not import this pack. {error}")
            } else if self.server.is_some() {
                error.clone()
            } else {
                format!("Could not save this instance. {error}")
            };
            content = content.child(
                self.accessibility.text(
                    div()
                        .bg(self.palette.layer)
                        .p_2()
                        .text_color(self.palette.warning)
                        .child(reason.clone()),
                    "validation",
                    reason,
                    Role::Alert,
                ),
            );
        }
        let (dismiss, treatment) = if self.notice.is_some() {
            ("OK", Partition::Primary)
        } else if let Some(account) = &self.account {
            let dismiss = if account.confirmation(cx).is_some() {
                "Keep account"
            } else if account.panel.read(cx).snapshot.cancellable {
                "Cancel"
            } else {
                "Close"
            };
            (dismiss, Partition::Neutral)
        } else {
            ("Cancel", Partition::Neutral)
        };
        let bar = div()
            .flex()
            .flex_wrap()
            .justify_end()
            .gap_2()
            .px_5()
            .py_3()
            .bg(p.layer)
            .border_t_1()
            .border_color(self.palette.stroke)
            .child(self.control(
                label(
                    compact_action("close", p, treatment, self.importing, cx),
                    dismiss,
                    p,
                    treatment,
                    self.importing,
                ),
                "close",
                dismiss,
                Role::Button,
                self.importing,
                None,
                cx,
                |this, window, cx| this.account_dismiss(window, cx),
            ))
            .when(self.form.is_some() || self.server.is_some(), |bar| {
                let name = match self.form.as_ref().map(|form| form.mode) {
                    Some(Mode::Create) => "Create instance",
                    Some(Mode::Clone) => "Create copy",
                    _ => "Save changes",
                };
                bar.child(self.control(
                    label(
                        compact_action("save", p, Partition::Primary, false, cx),
                        name,
                        p,
                        Partition::Primary,
                        false,
                    ),
                    "save",
                    name,
                    Role::Button,
                    false,
                    None,
                    cx,
                    |this, window, cx| this.commit(window, cx),
                ))
            })
            .when(self.pack.is_some(), |bar| {
                let name = if self.importing {
                    "Importing…"
                } else {
                    "Import pack"
                };
                bar.child(self.control(
                    label(
                        compact_action("import", p, Partition::Primary, self.importing, cx),
                        name,
                        p,
                        Partition::Primary,
                        self.importing,
                    ),
                    "import",
                    name,
                    Role::Button,
                    self.importing,
                    None,
                    cx,
                    |this, window, cx| this.import(window, cx),
                ))
            });
        let bar = if let Some(account) = &self.account {
            self.account_actions(account, bar, cx)
        } else {
            bar
        };
        self.accessibility
            .frame(body.child(content).child(bar), false)
    }
}

#[cfg(target_os = "windows")]
fn native_handle(window: &Window) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Win32(hwnd) => Some(hwnd.hwnd.get()),
        _ => None,
    }
}

#[cfg(not(target_os = "windows"))]
fn native_handle(_: &Window) -> Option<isize> {
    None
}

#[cfg(target_os = "windows")]
fn attach(parent: Option<isize>, child: &Window) -> Result<Option<isize>, ()> {
    let parent = parent.ok_or(())?;
    let child = native_handle(child).ok_or(())? as *mut core::ffi::c_void;
    // SAFETY: both handles came from live GPUI windows on the UI thread.
    unsafe {
        SetWindowLongPtrW(child, GWLP_HWNDPARENT, parent);
        if GetWindow(child, GW_OWNER) as isize != parent {
            return Err(());
        }
        EnableWindow(parent as _, 0);
    }
    Ok(Some(parent))
}

#[cfg(not(target_os = "windows"))]
fn attach(_: Option<isize>, _: &Window) -> Result<Option<isize>, ()> {
    Ok(None)
}

fn enable(parent: Option<isize>) {
    #[cfg(target_os = "windows")]
    if let Some(parent) = parent {
        // SAFETY: the popup releases its stored owner handle on the GPUI UI thread.
        unsafe {
            EnableWindow(parent as _, 1);
        }
    }
    #[cfg(not(target_os = "windows"))]
    let _ = parent;
}

#[cfg(target_os = "windows")]
const GWLP_HWNDPARENT: i32 = -8;
#[cfg(target_os = "windows")]
const GW_OWNER: u32 = 4;
#[cfg(target_os = "windows")]
#[link(name = "user32")]
unsafe extern "system" {
    fn SetWindowLongPtrW(window: *mut core::ffi::c_void, index: i32, new: isize) -> isize;
    fn GetWindow(window: *mut core::ffi::c_void, command: u32) -> *mut core::ffi::c_void;
    fn EnableWindow(window: *mut core::ffi::c_void, enable: i32) -> i32;
    fn LoadImageW(
        module: *mut core::ffi::c_void,
        name: *const u16,
        typ: u32,
        width: i32,
        height: i32,
        flags: u32,
    ) -> *mut core::ffi::c_void;
    fn SendMessageW(
        window: *mut core::ffi::c_void,
        message: u32,
        word: usize,
        long: isize,
    ) -> isize;
}

#[cfg(target_os = "windows")]
pub fn set_app_icon(window: &Window) {
    let Some(hwnd) = native_handle(window) else {
        return;
    };
    // SAFETY: the icon resource is embedded by build.rs and the handle is borrowed from the live window.
    unsafe {
        let module = GetModuleHandleW(std::ptr::null());
        let name = 101usize as *const u16;
        for (kind, pixels) in [(0, 16), (1, 32)] {
            let icon = LoadImageW(module, name, 1, pixels, pixels, 0x8000);
            if !icon.is_null() {
                SendMessageW(hwnd as _, 0x80, kind, icon as isize);
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn set_app_icon(_: &Window) {}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut core::ffi::c_void;
}
