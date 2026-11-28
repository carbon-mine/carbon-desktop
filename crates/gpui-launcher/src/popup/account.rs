// SPDX-License-Identifier: MPL-2.0
use super::Popup;
use crate::account_panel::AccountPanel;
use crate::theme::{Partition, compact_action, label};
use accesskit::Role;
use carbon_account::desktop::{AccountState, AccountSummary};
use gpui::{
    ClipboardItem, Context, Div, ElementId, Entity, SharedString, Window, div, prelude::*, px,
};
use gpui_component::{
    Sizable, StyledExt,
    input::{Input, InputEvent, InputState},
};
use uuid::Uuid;

const CLIENT_HELP: &str = "Enter a public client ID you control that Minecraft approved for sign-in from this Carbon Desktop deployment. This is a public application ID, not a client secret or token.";
const CLIENT_HELP_LINES: [&str; 2] = [
    "Enter the public client ID you control that Minecraft approved for sign-in from this deployment.",
    "This is a public application ID, not a client secret or token.",
];
const CLIENT_CONFIRM: &str = "I confirm this public client ID is mine and is approved for Minecraft sign-in from this deployment.";
const CLIENT_CONFIRM_LABEL: &str = "I control this ID and Minecraft approved it.";
const CLIENT_CONFIRMED_LABEL: &str = "Confirmed: this ID is mine and approved.";
const CLIENT_SCOPE: [&str; 2] = [
    "Saved accounts stay with the public client ID they signed in with.",
    "They appear again when Carbon Desktop uses that public client ID.",
];

pub(super) struct AccountDialog {
    pub panel: Entity<AccountPanel>,
    confirm_sign_out: Option<Uuid>,
    client_input: Entity<InputState>,
    client_entry: bool,
    /// The exact entry the user confirmed, so an edited ID needs confirming again.
    client_confirmed: Option<String>,
    client_notice: Option<(String, bool)>,
}

impl AccountDialog {
    pub(super) fn new(
        panel: Entity<AccountPanel>,
        window: &mut Window,
        cx: &mut Context<Popup>,
    ) -> Self {
        cx.observe(&panel, |_, _, cx| cx.notify()).detach();
        panel.update(cx, AccountPanel::dialog_opened);
        let client_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Public client ID (UUID)"));
        cx.subscribe_in(
            &client_input,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => this.save_client_entry(window, cx),
                InputEvent::Change => {
                    if let Some(dialog) = &mut this.account
                        && dialog.client_confirmed.take().is_some()
                    {
                        dialog.client_notice = None;
                        cx.notify();
                    }
                }
                InputEvent::Focus | InputEvent::Blur => {}
            },
        )
        .detach();
        Self {
            panel,
            confirm_sign_out: None,
            client_input,
            client_entry: false,
            client_confirmed: None,
            client_notice: None,
        }
    }

    pub(super) fn confirmation<'a>(&self, cx: &'a Context<Popup>) -> Option<&'a AccountSummary> {
        let panel = self.panel.read(cx);
        self.confirm_sign_out
            .filter(|id| panel.can_sign_out(*id))
            .and_then(|id| panel.account(id))
    }
}

impl Popup {
    pub(super) fn account_content(&self, account: &AccountDialog, cx: &Context<Self>) -> Div {
        let panel = account.panel.read(cx);
        let view = &panel.snapshot;
        let confirmation = account.confirmation(cx);
        let title = if confirmation.is_some() {
            "Sign out of Minecraft?"
        } else if matches!(view.state, AccountState::AwaitingBrowser) {
            "Continue in your browser"
        } else {
            "Minecraft account"
        };
        let mut content = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.accessibility.text(
                div().text_lg().font_semibold().child(title),
                "account-heading",
                title,
                Role::Heading,
            ));
        if let Some(account) = confirmation {
            let message = format!(
                "Sign out of {} on this device? Its credentials will be cleared from Carbon Desktop. Your instances and saved games stay here.",
                account.name
            );
            return content.child(self.accessibility.text(
                div().text_sm().child(message.clone()),
                "account-signout-consequence",
                message,
                Role::Label,
            ));
        }
        if !view.message.is_empty() {
            content = content.child(
                self.accessibility.text(
                    div()
                        .text_sm()
                        .text_color(self.palette.muted)
                        .child(view.message.clone()),
                    "account-message",
                    view.message.clone(),
                    if matches!(view.state, AccountState::Error) {
                        Role::Alert
                    } else {
                        Role::Status
                    },
                ),
            );
        }
        if panel.custom_client_pending_restart() {
            let restart = panel.restart_message();
            content = content.child(
                self.accessibility.text(
                    div()
                        .text_sm()
                        .text_color(self.palette.muted)
                        .child(restart),
                    "account-client-restart",
                    restart,
                    Role::Status,
                ),
            );
        }
        if matches!(view.state, AccountState::AwaitingBrowser)
            && let Some(code) = &view.user_code
        {
            let copy = code.clone();
            content = content.child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap_3()
                    .child(
                        self.accessibility.text(
                            div()
                                .font_semibold()
                                .px_3()
                                .py_2()
                                .bg(self.palette.layer)
                                .child(code.clone()),
                            "account-device-code",
                            format!("Sign-in code {code}"),
                            Role::Label,
                        ),
                    )
                    .child(self.account_button(
                        "account-copy-code",
                        "Copy code",
                        false,
                        cx,
                        move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()));
                        },
                    )),
            );
        }
        if let Some(error) = &panel.browser_error {
            content = content.child(
                self.accessibility.text(
                    div()
                        .text_sm()
                        .text_color(self.palette.warning)
                        .child(error.clone()),
                    "account-browser-error",
                    error.clone(),
                    Role::Alert,
                ),
            );
        }
        if panel.custom_client_allowed() {
            let p = self.palette;
            if let Some((text, error)) = &account.client_notice {
                content = content.child(
                    self.accessibility.text(
                        div()
                            .text_sm()
                            .text_color(if *error { p.warning } else { p.muted })
                            .child(text.clone()),
                        "account-client-notice",
                        text.clone(),
                        if *error { Role::Alert } else { Role::Status },
                    ),
                );
            }
            if account.client_entry {
                let confirmed = account.client_confirmed.is_some();
                let editable = panel.client_setting_editable();
                content = content
                    .child(
                        self.accessibility.text(
                            div()
                                .flex()
                                .flex_col()
                                .text_sm()
                                .text_color(p.muted)
                                .child(CLIENT_HELP_LINES[0])
                                .child(CLIENT_HELP_LINES[1]),
                            "account-client-help",
                            CLIENT_HELP,
                            Role::Label,
                        ),
                    )
                    .child(self.accessibility.input(
                        Input::new(&account.client_input).small().bordered(true),
                        "Public client ID",
                        Role::TextInput,
                        &account.client_input,
                        !editable,
                        cx,
                    ))
                    .child(self.control(
                        label(
                            compact_action(
                                "account-client-confirm-use",
                                p,
                                if confirmed {
                                    Partition::Selected
                                } else {
                                    Partition::Neutral
                                },
                                false,
                                cx,
                            ),
                            if confirmed {
                                CLIENT_CONFIRMED_LABEL
                            } else {
                                CLIENT_CONFIRM_LABEL
                            },
                            p,
                            if confirmed {
                                Partition::Selected
                            } else {
                                Partition::Neutral
                            },
                            false,
                        ),
                        "account-client-confirm-use",
                        CLIENT_CONFIRM,
                        Role::CheckBox,
                        false,
                        Some(confirmed),
                        cx,
                        |this, _, cx| {
                            if let Some(dialog) = &mut this.account {
                                let value = dialog.client_input.read(cx).value().trim().to_owned();
                                dialog.client_confirmed = if value.is_empty()
                                    || dialog.client_confirmed.as_deref() == Some(value.as_str())
                                {
                                    None
                                } else {
                                    Some(value)
                                };
                                dialog.client_notice = None;
                            }
                            cx.notify();
                        },
                    ));
                let disabled = !confirmed || !editable;
                content = content.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(self.account_button(
                            "account-client-cancel-entry",
                            "Cancel",
                            false,
                            cx,
                            |this, window, cx| this.close_client_entry(window, cx),
                        ))
                        .child(self.control(
                            label(
                                compact_action(
                                    "account-client-save",
                                    p,
                                    Partition::Primary,
                                    disabled,
                                    cx,
                                ),
                                "Use client ID",
                                p,
                                Partition::Primary,
                                disabled,
                            ),
                            "account-client-save",
                            "Use client ID",
                            Role::Button,
                            disabled,
                            None,
                            cx,
                            |this, window, cx| this.save_client_entry(window, cx),
                        )),
                );
            } else {
                let has_setting = panel.custom_client_configured();
                let editable = panel.client_setting_editable();
                if panel.custom_client_active() {
                    content = content.child(
                        self.accessibility.text(
                            div()
                                .text_sm()
                                .text_color(p.muted)
                                .child("Using a per-install public client ID."),
                            "account-client-active",
                            "Using a per-install public client ID.",
                            Role::Status,
                        ),
                    );
                }
                content = content.child(
                    self.accessibility.text(
                        div()
                            .flex()
                            .flex_col()
                            .text_sm()
                            .text_color(p.muted)
                            .child(CLIENT_SCOPE[0])
                            .child(CLIENT_SCOPE[1]),
                        "account-client-scope",
                        format!("{} {}", CLIENT_SCOPE[0], CLIENT_SCOPE[1]),
                        Role::Label,
                    ),
                );
                let (id, name) = if has_setting {
                    ("account-client-change", "Change approved client ID…")
                } else {
                    ("account-client-open", "Use an approved client ID…")
                };
                content = content.child(self.account_button_when(
                    id,
                    name,
                    false,
                    !editable,
                    cx,
                    |this, window, cx| this.open_client_entry(window, cx),
                ));
                if has_setting {
                    content = content.child(self.account_button_when(
                        "account-client-clear",
                        "Clear approved client ID",
                        false,
                        !editable,
                        cx,
                        |this, window, cx| this.clear_client_setting(window, cx),
                    ));
                }
            }
        }
        if let Some(error) = &panel.launcher_error {
            content = content.child(
                self.accessibility.text(
                    div()
                        .text_sm()
                        .text_color(self.palette.warning)
                        .child(error.clone()),
                    "account-launcher-error",
                    error.clone(),
                    Role::Alert,
                ),
            );
        }
        if panel.unavailable() {
            content = content.child(
                self.accessibility.text(
                    div()
                        .text_sm()
                        .text_color(self.palette.muted)
                        .child("Opens the installed Minecraft Launcher on Windows when available, or the official download page."),
                "account-launcher-fallback",
                "Opens the installed Minecraft Launcher on Windows when available, or the official download page.",
                Role::Label,
            ));
        } else if matches!(view.state, AccountState::Error) || view.accounts.is_empty() {
            content = content.child(self.account_button_when(
                "account-open-minecraft-launcher",
                "Open Minecraft Launcher",
                false,
                panel.launcher_opening,
                cx,
                |this, _, cx| this.update_account(cx, AccountPanel::open_minecraft_launcher),
            ));
        }
        if !account.client_entry && !view.accounts.is_empty() {
            let list = div().flex().flex_col().gap_2().children(
                view.accounts
                    .iter()
                    .map(|saved| self.account_row(saved, panel, cx)),
            );
            content = content.child(self.accessibility.group(
                list,
                "saved-accounts",
                "Saved accounts",
                Role::Group,
            ));
        }
        content
    }

    fn account_row(
        &self,
        account: &AccountSummary,
        panel: &AccountPanel,
        cx: &Context<Self>,
    ) -> crate::accessibility::Semantic {
        let p = self.palette;
        let id = account.id;
        let active = panel.snapshot.active_account == Some(id);
        let status = match (active, account.verified) {
            (true, true) => "Active · Verified",
            (true, false) => "Active · Check required",
            (false, _) => "Saved",
        };
        let mut controls = div().flex().flex_wrap().gap_2();
        if panel.can_check() {
            controls = if active {
                controls.child(self.account_button(
                    SharedString::from(format!("account-refresh-{id}")),
                    "Refresh account",
                    false,
                    cx,
                    move |this, window, cx| {
                        this.focus.focus(window);
                        this.update_account(cx, |panel, cx| panel.refresh_selected(id, cx))
                    },
                ))
            } else {
                controls.child(self.account_button(
                    SharedString::from(format!("account-switch-{id}")),
                    "Switch",
                    false,
                    cx,
                    move |this, window, cx| {
                        this.focus.focus(window);
                        this.update_account(cx, |panel, cx| panel.switch(id, cx))
                    },
                ))
            };
        }
        if panel.can_sign_out(id) {
            controls = controls.child(self.account_button(
                SharedString::from(format!("account-signout-{id}")),
                if panel.snapshot.removal_retry == Some(id) {
                    "Retry sign out…"
                } else {
                    "Sign out…"
                },
                false,
                cx,
                move |this, window, cx| {
                    if let Some(dialog) = &mut this.account
                        && !dialog.client_entry
                    {
                        dialog.confirm_sign_out = Some(id);
                        this.focus.focus(window);
                        cx.notify();
                    }
                },
            ));
        }
        let row = div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap_2()
            .p_3()
            .border_1()
            .border_color(p.stroke)
            .bg(if active { p.layer } else { p.surface })
            .child(
                div()
                    .flex_1()
                    .min_w(px(120.))
                    .text_sm()
                    .child(div().font_semibold().child(account.name.clone()))
                    .child(div().text_color(p.muted).child(status)),
            )
            .child(controls);
        self.accessibility.group(
            row,
            format!("saved-account-{id}"),
            format!("{}, {status}", account.name),
            Role::Group,
        )
    }

    fn open_client_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = &self.account else {
            return;
        };
        if !dialog.panel.read(cx).client_setting_editable() {
            return;
        }
        if let Some(dialog) = &mut self.account {
            dialog.client_entry = true;
            dialog.client_confirmed = None;
            dialog.client_notice = None;
            dialog
                .client_input
                .update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    fn close_client_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(dialog) = &mut self.account {
            dialog.client_entry = false;
            dialog.client_confirmed = None;
            dialog
                .client_input
                .update(cx, |input, cx| input.set_value("", window, cx));
            dialog.client_notice = None;
        }
        self.focus.focus(window);
        cx.notify();
    }

    fn save_client_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = &self.account else {
            return;
        };
        if !dialog.panel.read(cx).client_setting_editable() {
            return;
        }
        let value = dialog.client_input.read(cx).value().to_string();
        let entered = value.trim();
        if dialog.client_confirmed.as_deref() != Some(entered) {
            if let Some(dialog) = &mut self.account {
                dialog.client_notice = Some((
                    "Confirm this public client ID again after editing it.".into(),
                    true,
                ));
                dialog
                    .client_input
                    .update(cx, |input, cx| input.focus(window, cx));
            }
            cx.notify();
            return;
        }
        let result = dialog
            .panel
            .update(cx, |panel, _| panel.set_custom_client(entered));
        let mut invalid = false;
        if let Some(dialog) = &mut self.account {
            match result {
                Ok(()) => {
                    dialog.client_entry = false;
                    dialog.client_confirmed = None;
                    dialog
                        .client_input
                        .update(cx, |input, cx| input.set_value("", window, cx));
                    dialog.client_notice = Some(("Public client ID saved.".into(), false));
                }
                Err(error) => {
                    dialog.client_notice = Some((error, true));
                    dialog
                        .client_input
                        .update(cx, |input, cx| input.focus(window, cx));
                    invalid = true;
                }
            }
        }
        if !invalid {
            self.focus.focus(window);
        }
        cx.notify();
    }

    fn clear_client_setting(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = &self.account else {
            return;
        };
        if !dialog.panel.read(cx).client_setting_editable() {
            return;
        }
        let result = dialog
            .panel
            .update(cx, |panel, _| panel.clear_custom_client());
        if let Some(dialog) = &mut self.account {
            match result {
                Ok(()) => {
                    dialog.client_entry = false;
                    dialog.client_confirmed = None;
                    dialog.client_input.update(cx, |input, cx| {
                        input.set_value("", window, cx);
                    });
                    dialog.client_notice =
                        Some(("Per-install public client ID cleared.".into(), false));
                }
                Err(error) => dialog.client_notice = Some((error, true)),
            }
        }
        self.focus.focus(window);
        cx.notify();
    }

    pub(super) fn account_actions(
        &self,
        account: &AccountDialog,
        bar: Div,
        cx: &Context<Self>,
    ) -> Div {
        if account.client_entry {
            return bar;
        }
        let panel = account.panel.read(cx);
        let view = &panel.snapshot;
        if let Some(selected) = account.confirmation(cx) {
            let id = selected.id;
            return bar.child(self.account_button(
                "account-confirm-signout",
                "Sign out",
                true,
                cx,
                move |this, window, cx| {
                    if let Some(dialog) = &mut this.account {
                        dialog.confirm_sign_out = None;
                        dialog.panel.update(cx, |panel, cx| panel.sign_out(id, cx));
                        this.focus.focus(window);
                        cx.notify();
                    }
                },
            ));
        }
        if panel.can_add() {
            return bar.child(self.account_button(
                "account-begin",
                if !account.client_entry && !view.accounts.is_empty() {
                    "Add Microsoft account"
                } else if matches!(view.state, AccountState::Error) {
                    "Try again"
                } else {
                    "Sign in with Microsoft"
                },
                true,
                cx,
                |this, window, cx| {
                    this.focus.focus(window);
                    this.update_account(cx, AccountPanel::begin)
                },
            ));
        }
        if panel.unavailable() {
            return bar.child(self.account_button_when(
                "account-open-minecraft-launcher",
                "Open Minecraft Launcher",
                true,
                panel.launcher_opening,
                cx,
                |this, _, cx| this.update_account(cx, AccountPanel::open_minecraft_launcher),
            ));
        }
        match view.state {
            AccountState::AwaitingBrowser => bar.child(self.account_button(
                "account-open-browser",
                "Open Microsoft browser",
                true,
                cx,
                |this, _, cx| this.update_account(cx, AccountPanel::open_browser),
            )),
            _ => bar,
        }
    }

    pub(super) fn account_dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .account
            .as_ref()
            .is_some_and(|dialog| dialog.client_entry)
        {
            self.close_client_entry(window, cx);
            return;
        }
        if let Some(dialog) = &mut self.account
            && dialog.confirmation(cx).is_some()
        {
            dialog.confirm_sign_out = None;
            self.focus.focus(window);
            cx.notify();
            return;
        }
        window.remove_window();
    }

    fn update_account(
        &mut self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut AccountPanel, &mut Context<AccountPanel>),
    ) {
        if let Some(account) = &self.account {
            account.panel.update(cx, f);
        }
    }

    fn account_button(
        &self,
        id: impl Into<ElementId>,
        name: &'static str,
        primary: bool,
        cx: &Context<Self>,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> impl IntoElement {
        self.account_button_when(id, name, primary, false, cx, action)
    }

    fn account_button_when(
        &self,
        id: impl Into<ElementId>,
        name: &'static str,
        primary: bool,
        disabled: bool,
        cx: &Context<Self>,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> impl IntoElement {
        let p = self.palette;
        let id = id.into();
        let treatment = if primary {
            Partition::Primary
        } else {
            Partition::Neutral
        };
        self.control(
            label(
                compact_action(id.clone(), p, treatment, disabled, cx).min_w(px(72.)),
                name,
                p,
                treatment,
                disabled,
            ),
            id,
            name,
            Role::Button,
            disabled,
            None,
            cx,
            action,
        )
    }
}
