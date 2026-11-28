// SPDX-License-Identifier: MPL-2.0
use carbon_account::desktop::{AccountState, AccountSummary, AccountView, Accounts};
#[cfg(feature = "dev-account-simulation")]
use carbon_account::simulation::{EntitlementState, Persona, SimulationPhase};
use gpui::Context;
use std::{path::Path, time::Duration};
use uuid::Uuid;

pub struct AccountPanel {
    service: Accounts,
    pub snapshot: AccountView,
    pub browser_error: Option<String>,
    pub launcher_error: Option<String>,
    pub launcher_opening: bool,
    dialog_visible: bool,
    monitoring: bool,
    client_configured: bool,
    client_pending_restart: bool,
}

impl AccountPanel {
    pub fn new(root: &Path) -> Self {
        Self::with_service(Accounts::new(root))
    }

    /// The real panel and lifecycle with the deterministic developer provider.
    #[cfg(feature = "dev-account-simulation")]
    pub fn new_simulated(root: &Path, simulation: carbon_account::simulation::Simulation) -> Self {
        Self::with_service(Accounts::simulated(root, simulation))
    }

    fn with_service(service: Accounts) -> Self {
        let snapshot = service.snapshot();
        let client_configured = service.custom_client_configured();
        let client_pending_restart = service.custom_client_pending_restart();
        Self {
            service,
            snapshot,
            browser_error: None,
            launcher_error: None,
            launcher_opening: false,
            dialog_visible: false,
            monitoring: false,
            client_configured,
            client_pending_restart,
        }
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let snapshot = self.service.snapshot();
        let client_configured = self.service.custom_client_configured();
        let client_pending_restart = self.service.custom_client_pending_restart();
        let snapshot_changed = self.snapshot != snapshot;
        let client_changed = self.client_configured != client_configured
            || self.client_pending_restart != client_pending_restart;
        self.snapshot = snapshot;
        self.client_configured = client_configured;
        self.client_pending_restart = client_pending_restart;
        if snapshot_changed {
            self.browser_error = None;
            self.launcher_error = None;
        }
        if snapshot_changed || client_changed {
            cx.notify();
        }
        if !self.monitoring && self.needs_poll() {
            self.monitoring = true;
            let executor = cx.background_executor().clone();
            cx.spawn(async move |panel, cx| {
                loop {
                    executor.timer(Duration::from_millis(250)).await;
                    if !matches!(
                        panel.update(cx, |panel, cx| {
                            panel.refresh(cx);
                            panel.monitoring = panel.needs_poll();
                            panel.monitoring
                        }),
                        Ok(true)
                    ) {
                        break;
                    }
                }
            })
            .detach();
        }
    }

    pub fn dialog_opened(&mut self, cx: &mut Context<Self>) {
        self.dialog_visible = true;
        self.refresh(cx);
    }

    pub fn dialog_closed(&mut self, cx: &mut Context<Self>) {
        self.dialog_visible = false;
        if self.service.snapshot().cancellable {
            self.service.cancel();
        }
        self.refresh(cx);
    }

    fn needs_poll(&self) -> bool {
        self.dialog_visible || self.pending()
    }

    pub fn begin(&mut self, cx: &mut Context<Self>) {
        if self.can_add() {
            self.service.begin();
            self.refresh(cx);
        }
    }

    pub fn switch(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if self.can_check()
            && self.snapshot.active_account != Some(id)
            && self.account(id).is_some()
        {
            self.service.switch(id);
            self.refresh(cx);
        }
    }

    pub fn refresh_selected(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if self.can_check() && self.snapshot.active_account == Some(id) {
            self.service.refresh_selected();
            self.refresh(cx);
        }
    }

    pub fn sign_out(&mut self, id: Uuid, cx: &mut Context<Self>) {
        if self.can_sign_out(id) {
            self.service.sign_out_account(id);
            self.refresh(cx);
        }
    }

    pub fn open_browser(&mut self, cx: &mut Context<Self>) {
        if matches!(self.snapshot.state, AccountState::AwaitingBrowser) {
            self.browser_error = self.service.open_browser().err();
            cx.notify();
        }
    }

    pub fn open_minecraft_launcher(&mut self, cx: &mut Context<Self>) {
        if self.launcher_opening {
            return;
        }
        self.launcher_opening = true;
        self.launcher_error = None;
        cx.notify();

        let launch = cx
            .background_executor()
            .spawn(async { Self::open_minecraft_launcher_action() });
        cx.spawn(async move |panel, cx| {
            let result = launch.await;
            let _ = panel.update(cx, |panel, cx| {
                panel.launcher_opening = false;
                panel.launcher_error = result.err();
                cx.notify();
            });
        })
        .detach();
    }

    fn open_minecraft_launcher_action() -> Result<(), String> {
        #[cfg(windows)]
        if crate::official_launcher::activate_official_minecraft_launcher().is_ok() {
            return Ok(());
        }
        carbon_account::desktop::open_minecraft_download()
    }

    pub fn custom_client_allowed(&self) -> bool {
        self.service.custom_client_allowed()
    }

    pub fn custom_client_active(&self) -> bool {
        self.service.custom_client_active()
    }

    pub fn custom_client_configured(&self) -> bool {
        self.client_configured
    }

    /// True when this panel runs the opt-in developer provider.
    pub fn is_simulated(&self) -> bool {
        self.service.is_simulated()
    }

    #[cfg(feature = "dev-account-simulation")]
    pub fn simulation_persona(&self) -> Persona {
        self.service
            .simulation()
            .map(|simulation| simulation.next_persona())
            .unwrap_or(Persona::Forest)
    }

    #[cfg(feature = "dev-account-simulation")]
    pub fn simulation_phase(&self) -> SimulationPhase {
        self.service
            .simulation()
            .map(|simulation| simulation.phase())
            .unwrap_or(SimulationPhase::Idle)
    }

    #[cfg(feature = "dev-account-simulation")]
    pub fn simulation_entitlement(&self, persona: Persona) -> EntitlementState {
        self.service
            .simulation()
            .map(|simulation| simulation.entitlement(persona))
            .unwrap_or(EntitlementState::Granted)
    }

    #[cfg(feature = "dev-account-simulation")]
    pub fn set_simulation_persona(&mut self, persona: Persona, cx: &mut Context<Self>) {
        if let Some(simulation) = self.service.simulation() {
            simulation.set_next_persona(persona);
        }
        cx.notify();
    }

    #[cfg(feature = "dev-account-simulation")]
    pub fn simulation_approve(&mut self, cx: &mut Context<Self>) -> bool {
        let approved = self
            .service
            .simulation()
            .is_some_and(|simulation| simulation.approve());
        self.refresh(cx);
        approved
    }

    #[cfg(feature = "dev-account-simulation")]
    pub fn simulation_decline(&mut self, cx: &mut Context<Self>) -> bool {
        let declined = self
            .service
            .simulation()
            .is_some_and(|simulation| simulation.decline());
        self.refresh(cx);
        declined
    }

    #[cfg(feature = "dev-account-simulation")]
    pub fn simulation_expire(&mut self, cx: &mut Context<Self>) -> bool {
        let expired = self
            .service
            .simulation()
            .is_some_and(|simulation| simulation.expire());
        self.refresh(cx);
        expired
    }

    /// Change the next persona's entitlement. When that persona is the active
    /// account, refresh through the ordinary verification job immediately.
    #[cfg(feature = "dev-account-simulation")]
    pub fn simulation_set_entitlement(
        &mut self,
        entitlement: EntitlementState,
        cx: &mut Context<Self>,
    ) {
        let Some(persona) = self
            .service
            .simulation()
            .map(|simulation| simulation.next_persona())
        else {
            return;
        };
        if let Some(simulation) = self.service.simulation() {
            simulation.set_entitlement(persona, entitlement);
        }
        if self
            .snapshot
            .active_account
            .and_then(Persona::from_profile_id)
            == Some(persona)
            && self.can_check()
        {
            self.service.refresh_selected();
        }
        self.refresh(cx);
    }

    /// The stored opt-in differs from the registration this process is using.
    pub fn custom_client_pending_restart(&self) -> bool {
        self.client_pending_restart
    }

    /// Registration edits must not race an in-flight account operation.
    pub fn client_setting_editable(&self) -> bool {
        !self.pending()
    }

    /// Plain copy for the paused account actions while a restart is required.
    pub fn restart_message(&self) -> &'static str {
        if self.client_configured {
            "Account actions wait for a restart. Restart Carbon Desktop to apply the saved public client ID."
        } else {
            "Account actions wait for a restart. Restart Carbon Desktop to finish removing the per-install public client ID."
        }
    }

    pub fn set_custom_client(&mut self, value: &str) -> Result<(), String> {
        let result = self.service.set_custom_client(value);
        if result.is_ok() {
            self.sync_client_setting();
        }
        result
    }

    pub fn clear_custom_client(&mut self) -> Result<(), String> {
        let result = self.service.clear_custom_client();
        if result.is_ok() {
            self.sync_client_setting();
        }
        result
    }

    fn sync_client_setting(&mut self) {
        self.client_configured = self.service.custom_client_configured();
        self.client_pending_restart = self.service.custom_client_pending_restart();
    }

    pub fn signed_in(&self) -> bool {
        self.active_account().is_some_and(|active| active.verified)
    }

    pub fn account(&self, id: Uuid) -> Option<&AccountSummary> {
        self.snapshot
            .accounts
            .iter()
            .find(|account| account.id == id)
    }

    pub fn active_account(&self) -> Option<&AccountSummary> {
        self.snapshot.active_account.and_then(|id| self.account(id))
    }

    pub fn pending(&self) -> bool {
        matches!(
            self.snapshot.state,
            AccountState::Starting | AccountState::AwaitingBrowser
        )
    }

    pub fn unavailable(&self) -> bool {
        self.snapshot.state == AccountState::Unavailable
    }

    pub fn toolbar_label(&self) -> &'static str {
        if self.is_simulated() {
            "Test account"
        } else if self.unavailable() || !self.snapshot.accounts.is_empty() || self.pending() {
            "Minecraft account"
        } else {
            "Sign in"
        }
    }

    pub fn can_add(&self) -> bool {
        !self.client_pending_restart
            && self.snapshot.removal_retry.is_none()
            && matches!(
                self.snapshot.state,
                AccountState::SignedOut | AccountState::SignedIn | AccountState::Error
            )
    }

    pub fn can_check(&self) -> bool {
        !self.client_pending_restart
            && !self.pending()
            && !self.unavailable()
            && self.snapshot.removal_retry.is_none()
    }

    pub fn can_sign_out(&self, id: Uuid) -> bool {
        !self.client_pending_restart
            && !self.pending()
            && self.account(id).is_some()
            && self.snapshot.removal_retry.is_none_or(|retry| retry == id)
    }

    pub fn play_hint(&self) -> &'static str {
        if self.is_simulated() {
            if self.signed_in() {
                "This test account is signed in. Protected game launch is unavailable in this build."
            } else if self.active_account().is_some() {
                "Verify the active test account. Protected game launch is unavailable in this build."
            } else {
                "Sign in to check a test account. Protected game launch is unavailable in this build."
            }
        } else if self.client_pending_restart {
            "Restart Carbon Desktop before using Minecraft accounts. Protected game launch is unavailable in this build."
        } else if self.unavailable() {
            "Minecraft account sign-in is not configured. Open account setup or use the Minecraft Launcher."
        } else if self.signed_in() {
            "This account is signed in. Protected game launch is unavailable in this build."
        } else if self.active_account().is_some() {
            "Verify the active Minecraft account. Protected game launch is unavailable in this build."
        } else {
            "Sign in to check this Minecraft account. Protected game launch is unavailable in this build."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "dev-account-simulation")]
    #[test]
    fn a_simulated_panel_is_marked_as_a_test_account_and_offers_no_client_setting() {
        let root = tempfile::tempdir().unwrap();
        let panel =
            AccountPanel::new_simulated(root.path(), carbon_account::simulation::Simulation::new());
        assert!(panel.is_simulated());
        assert!(!panel.custom_client_allowed());
        assert!(panel.play_hint().contains("test account"));
        assert!(!panel.play_hint().contains("Signed in"));
    }

    #[test]
    fn active_account_identity_controls_verification_and_toolbar() {
        let root = tempfile::tempdir().unwrap();
        let mut panel = AccountPanel::new(root.path());
        let active = Uuid::new_v4();
        let other = Uuid::new_v4();
        panel.snapshot.accounts = vec![
            AccountSummary {
                id: active,
                name: "Forest".into(),
                verified: false,
            },
            AccountSummary {
                id: other,
                name: "Coast".into(),
                verified: true,
            },
        ];
        panel.snapshot.active_account = Some(active);
        panel.snapshot.state = AccountState::Error;
        assert_eq!(panel.active_account().unwrap().name, "Forest");
        assert!(
            !panel.signed_in(),
            "another saved account cannot verify the active identity"
        );
        assert_eq!(panel.toolbar_label(), "Minecraft account");
        assert!(panel.can_add());
        assert!(panel.can_check());
        assert!(!panel.needs_poll());

        panel.snapshot.active_account = Some(other);
        panel.snapshot.state = AccountState::Starting;
        assert!(panel.signed_in());
        assert!(panel.pending());
        assert!(
            panel.needs_poll(),
            "closing the dialog must not hide the eventual completion"
        );
        assert!(!panel.can_add());
        assert!(!panel.can_check());

        panel.snapshot.state = AccountState::Error;
        panel.snapshot.removal_retry = Some(active);
        assert!(
            !panel.can_add(),
            "an Add command must never stand in for a removal retry"
        );
        assert!(!panel.can_check());
        assert!(panel.can_sign_out(active));
        assert!(!panel.can_sign_out(other));

        panel.snapshot.state = AccountState::Unavailable;
        assert!(!panel.can_check());
        assert_eq!(panel.toolbar_label(), "Minecraft account");
        assert!(
            panel
                .play_hint()
                .starts_with("Minecraft account sign-in is not configured")
        );
        panel.snapshot.active_account = Some(Uuid::new_v4());
        assert!(panel.active_account().is_none());
        assert!(!panel.signed_in());
    }

    #[test]
    fn a_saved_client_change_pauses_account_mutations_until_restart() {
        let root = tempfile::tempdir().unwrap();
        let mut panel = AccountPanel::new(root.path());
        let account = Uuid::new_v4();
        panel.snapshot.accounts = vec![AccountSummary {
            id: account,
            name: "Forest".into(),
            verified: true,
        }];
        panel.snapshot.active_account = Some(account);
        panel.snapshot.state = AccountState::SignedIn;
        assert!(panel.can_add());
        assert!(panel.can_check());
        assert!(panel.can_sign_out(account));

        panel.client_configured = true;
        panel.client_pending_restart = true;
        assert!(
            !panel.can_add(),
            "an Add command cannot run under the previous registration"
        );
        assert!(!panel.can_check());
        assert!(!panel.can_sign_out(account));
        assert!(panel.custom_client_pending_restart());
        assert!(panel.play_hint().starts_with("Restart Carbon Desktop"));
        assert!(panel.restart_message().contains("apply the saved"));

        panel.snapshot.state = AccountState::Unavailable;
        assert!(panel.play_hint().starts_with("Restart Carbon Desktop"));

        panel.client_pending_restart = false;
        panel.snapshot.state = AccountState::AwaitingBrowser;
        assert!(
            !panel.client_setting_editable(),
            "a registration edit must not race an account operation"
        );
        assert!(!panel.can_check());
    }
}
