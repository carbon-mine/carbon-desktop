// SPDX-License-Identifier: MPL-2.0
use super::Popup;
use crate::account_panel::AccountPanel;
use crate::theme::{Partition, compact_action, label};
use accesskit::Role;
use carbon_account::desktop::{AccountState, AccountSummary};
use gpui::{
    ClipboardItem, Context, Div, ElementId, Entity, SharedString, Window, div, prelude::*, px,
};
use gpui_component::StyledExt;
use uuid::Uuid;

pub(super) struct AccountDialog {
    pub panel: Entity<AccountPanel>,
    confirm_sign_out: Option<Uuid>,
}

impl AccountDialog {
    pub(super) fn new(panel: Entity<AccountPanel>, cx: &mut Context<Popup>) -> Self {
        cx.observe(&panel, |_, _, cx| cx.notify()).detach();
        panel.update(cx, AccountPanel::dialog_opened);
        Self {
            panel,
            confirm_sign_out: None,
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
        } else {
            match view.state {
                AccountState::Unavailable => "Microsoft sign-in unavailable",
                AccountState::SignedOut => "Sign in with Microsoft",
                AccountState::Starting => "Updating Minecraft accounts…",
                AccountState::AwaitingBrowser => "Continue in your browser",
                AccountState::SignedIn => "Minecraft accounts",
                AccountState::Error => "Microsoft sign-in needs attention",
            }
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
        if !view.accounts.is_empty() {
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
                    if let Some(dialog) = &mut this.account {
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

    pub(super) fn account_actions(
        &self,
        account: &AccountDialog,
        bar: Div,
        cx: &Context<Self>,
    ) -> Div {
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
                if !view.accounts.is_empty() {
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
        let p = self.palette;
        let id = id.into();
        let treatment = if primary {
            Partition::Primary
        } else {
            Partition::Neutral
        };
        self.control(
            label(
                compact_action(id.clone(), p, treatment, false, cx).min_w(px(72.)),
                name,
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
            action,
        )
    }
}
