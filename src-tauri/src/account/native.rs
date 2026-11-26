// SPDX-License-Identifier: MPL-2.0
use carbon_account::desktop::{AccountState, AccountView, Accounts};
use std::sync::atomic::{AtomicBool, Ordering};
use uuid::Uuid;

pub(super) struct DialogGuard<'a>(&'a AtomicBool);

impl<'a> DialogGuard<'a> {
    pub(super) fn acquire(open: &'a AtomicBool) -> Option<Self> {
        open.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self(open))
    }
}

impl Drop for DialogGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Add,
    Switch(Uuid),
    Refresh(Uuid),
    AskSignOut(Uuid),
    SignOut(Uuid),
    Browser,
    CopyCode,
    CancelWork,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Control(pub &'static str, pub Action);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Controls {
    pub main: Option<Control>,
    pub verify: Option<Control>,
    pub sign_out: Option<Control>,
    pub copy: Option<Control>,
    pub cancel: Option<Control>,
}

impl Controls {
    pub fn slots(&self) -> [Option<Control>; 5] {
        [
            self.main,
            self.verify,
            self.sign_out,
            self.copy,
            self.cancel,
        ]
    }

    fn contains(&self, action: Action) -> bool {
        self.slots()
            .into_iter()
            .flatten()
            .any(|control| control.1 == action)
    }
}

pub struct Dialog {
    service: Accounts,
    pub view: AccountView,
    pub selected: Option<Uuid>,
    confirmation: Option<Uuid>,
    feedback: Option<String>,
    started_here: bool,
}

impl Dialog {
    pub(super) fn new(service: Accounts) -> Self {
        let view = service.snapshot();
        let selected = default_selection(&view);
        Self {
            service,
            view,
            selected,
            confirmation: None,
            feedback: None,
            started_here: false,
        }
    }

    pub fn refresh(&mut self) -> bool {
        let view = self.service.snapshot();
        if view == self.view {
            return false;
        }
        if self.view.active_account != view.active_account
            || !view
                .accounts
                .iter()
                .any(|row| Some(row.id) == self.selected)
        {
            self.selected = default_selection(&view);
        }
        if self.view.user_code != view.user_code || self.view.state != view.state {
            self.feedback = None;
        }
        self.view = view;
        if !self.pending() {
            self.started_here = false;
        }
        if self.confirmation.is_some_and(|id| !self.can_sign_out(id)) {
            self.confirmation = None;
        }
        true
    }

    pub fn select(&mut self, id: Uuid) {
        if self.confirmation.is_none() && self.view.accounts.iter().any(|row| row.id == id) {
            self.selected = Some(id);
        }
    }

    pub fn heading(&self) -> &'static str {
        if self.confirmation.is_some() {
            return "Sign out of Minecraft?";
        }
        match self.view.state {
            AccountState::Unavailable => "Microsoft sign-in unavailable",
            AccountState::SignedOut if self.view.accounts.is_empty() => "Sign in with Microsoft",
            AccountState::SignedOut | AccountState::SignedIn => "Minecraft accounts",
            AccountState::Starting => "Updating Minecraft accounts…",
            AccountState::AwaitingBrowser => "Continue in your browser",
            AccountState::Error => "Microsoft sign-in needs attention",
        }
    }

    pub fn message(&self) -> String {
        if let Some(account) = self
            .confirmation
            .and_then(|id| self.view.accounts.iter().find(|row| row.id == id))
        {
            return format!(
                "Sign out of {} on this device? Carbon Desktop will clear its credentials. Your instances and saved games stay here.",
                account.name
            );
        }
        self.feedback
            .as_deref()
            .unwrap_or(&self.view.message)
            .to_owned()
    }

    pub fn code(&self) -> Option<&str> {
        self.view.user_code.as_deref()
    }

    pub fn rows(&self) -> Vec<(Uuid, String)> {
        if self.confirmation.is_some() {
            return Vec::new();
        }
        self.view
            .accounts
            .iter()
            .map(|row| {
                let status = match (self.view.active_account == Some(row.id), row.verified) {
                    (true, true) => "Active · Verified",
                    (true, false) => "Active · Check required",
                    (false, _) => "Saved",
                };
                (row.id, format!("{} ({status})", row.name))
            })
            .collect()
    }

    pub fn confirming(&self) -> bool {
        self.confirmation.is_some()
    }

    fn pending(&self) -> bool {
        matches!(
            self.view.state,
            AccountState::Starting | AccountState::AwaitingBrowser
        )
    }

    fn can_sign_out(&self, id: Uuid) -> bool {
        !self.pending()
            && self.view.accounts.iter().any(|row| row.id == id)
            && self.view.removal_retry.is_none_or(|retry| retry == id)
    }

    pub fn controls(&self) -> Controls {
        let mut controls = Controls {
            main: None,
            verify: None,
            sign_out: None,
            copy: None,
            cancel: None,
        };
        if let Some(id) = self.confirmation {
            if self.can_sign_out(id) {
                controls.main = Some(Control("Sign out", Action::SignOut(id)));
            }
            return controls;
        }
        if self.view.state == AccountState::AwaitingBrowser && self.code().is_some() {
            controls.main = Some(Control("Open Microsoft browser", Action::Browser));
            controls.copy = Some(Control("Copy code", Action::CopyCode));
        }
        if self.view.cancellable {
            controls.cancel = Some(Control("Cancel operation", Action::CancelWork));
        }
        if self.pending() {
            return controls;
        }
        if self.view.state != AccountState::Unavailable && self.view.removal_retry.is_none() {
            let label = if !self.view.accounts.is_empty() {
                "Add Microsoft account"
            } else if self.view.state == AccountState::Error {
                "Try again"
            } else {
                "Sign in with Microsoft"
            };
            controls.main = Some(Control(label, Action::Add));
            controls.verify = self.selected.map(|id| {
                if self.view.active_account == Some(id) {
                    Control("Refresh account", Action::Refresh(id))
                } else {
                    Control("Switch account", Action::Switch(id))
                }
            });
        }
        if let Some(id) = self.selected.filter(|id| self.can_sign_out(*id)) {
            let label = if self.view.removal_retry == Some(id) {
                "Retry sign out…"
            } else {
                "Sign out…"
            };
            controls.sign_out = Some(Control(label, Action::AskSignOut(id)));
        }
        controls
    }

    pub fn perform(&mut self, action: Action, copy: impl FnOnce(&str) -> bool) {
        let previous_code = if action == Action::CopyCode {
            self.view.user_code.clone()
        } else {
            None
        };
        self.refresh();
        if !self.controls().contains(action) {
            return;
        }
        match action {
            Action::Add => self.service.begin(),
            Action::Switch(id) => self.service.switch(id),
            Action::Refresh(_) => self.service.refresh_selected(),
            Action::AskSignOut(id) => self.confirmation = Some(id),
            Action::SignOut(id) => {
                self.confirmation = None;
                self.service.sign_out_account(id);
            }
            Action::Browser => self.feedback = self.service.open_browser().err(),
            Action::CopyCode => {
                if let Some(code) = self
                    .code()
                    .filter(|code| Some(*code) == previous_code.as_deref())
                {
                    self.feedback = Some(if copy(code) {
                        "Code copied. Enter it in the Microsoft browser page.".into()
                    } else {
                        "The code could not be copied. Enter it in the Microsoft browser page."
                            .into()
                    });
                }
            }
            Action::CancelWork => self.service.cancel(),
        }
        if matches!(action, Action::Add | Action::Switch(_) | Action::Refresh(_)) {
            self.started_here = true;
        }
        self.refresh();
    }

    /// Returns true when the native dialog can close. Escape first backs out of sign-out.
    pub fn dismiss(&mut self) -> bool {
        self.confirmation.take().is_none()
    }
}

fn default_selection(view: &AccountView) -> Option<Uuid> {
    view.removal_retry
        .or(view.active_account)
        .or_else(|| view.accounts.first().map(|row| row.id))
}

impl Drop for Dialog {
    fn drop(&mut self) {
        if self.started_here {
            self.service.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use carbon_account::desktop::AccountSummary;

    fn dialog() -> Dialog {
        let mut dialog = Dialog::new(Accounts::session_only());
        dialog.view = AccountView {
            state: AccountState::SignedIn,
            message: "Minecraft: Java Edition account verified.".into(),
            name: Some("Forest".into()),
            user_code: None,
            accounts: vec![
                AccountSummary {
                    id: Uuid::from_u128(1),
                    name: "Forest".into(),
                    verified: true,
                },
                AccountSummary {
                    id: Uuid::from_u128(2),
                    name: "Coast".into(),
                    verified: false,
                },
            ],
            active_account: Some(Uuid::from_u128(1)),
            cancellable: false,
            removal_retry: None,
        };
        dialog.selected = dialog.view.active_account;
        dialog
    }

    #[test]
    fn native_actions_follow_the_selected_uuid_and_keep_confirmation_bound() {
        let mut dialog = dialog();
        let forest = Uuid::from_u128(1);
        let coast = Uuid::from_u128(2);
        assert!(dialog.controls().contains(Action::Refresh(forest)));
        dialog.select(coast);
        assert!(dialog.controls().contains(Action::Switch(coast)));
        assert!(!dialog.controls().contains(Action::Refresh(forest)));
        assert!(dialog.controls().contains(Action::AskSignOut(coast)));
        assert!(!dialog.controls().contains(Action::SignOut(coast)));
        dialog.confirmation = Some(coast);
        dialog.select(forest);
        assert!(dialog.message().contains("Coast"));
        assert!(dialog.controls().contains(Action::SignOut(coast)));
        assert!(!dialog.controls().contains(Action::SignOut(forest)));
        assert!(!dialog.dismiss());
        assert!(dialog.controls().contains(Action::AskSignOut(coast)));
    }

    #[test]
    fn removal_retry_cannot_turn_into_refresh_add_or_another_removal() {
        let mut dialog = dialog();
        let forest = Uuid::from_u128(1);
        let coast = Uuid::from_u128(2);
        dialog.view.state = AccountState::Error;
        dialog.view.removal_retry = Some(coast);
        assert_eq!(default_selection(&dialog.view), Some(coast));
        assert!(!dialog.controls().contains(Action::AskSignOut(forest)));
        assert!(!dialog.controls().contains(Action::Add));
        assert!(!dialog.controls().contains(Action::Refresh(forest)));
        dialog.select(coast);
        assert_eq!(
            dialog.controls().sign_out,
            Some(Control("Retry sign out…", Action::AskSignOut(coast)))
        );
    }

    #[test]
    fn unconfigured_and_expired_sign_in_do_not_offer_browser_actions() {
        let mut dialog = dialog();
        dialog.view.state = AccountState::Unavailable;
        dialog.view.accounts.clear();
        dialog.selected = None;
        assert_eq!(dialog.heading(), "Microsoft sign-in unavailable");
        assert!(dialog.controls().slots().iter().all(Option::is_none));
        dialog.view.state = AccountState::AwaitingBrowser;
        dialog.view.cancellable = true;
        assert!(dialog.controls().contains(Action::CancelWork));
        assert!(!dialog.controls().contains(Action::CopyCode));
        assert!(!dialog.controls().contains(Action::Browser));
        dialog.view.user_code = Some("SYNTHETIC".into());
        assert!(dialog.controls().contains(Action::CopyCode));
        assert!(dialog.controls().contains(Action::Browser));
    }

    #[test]
    fn native_window_guard_rejects_overlapping_requests_and_reopens_after_drop() {
        let open = AtomicBool::new(false);
        let first = DialogGuard::acquire(&open).unwrap();
        assert!(DialogGuard::acquire(&open).is_none());
        drop(first);
        assert!(DialogGuard::acquire(&open).is_some());
    }
}
