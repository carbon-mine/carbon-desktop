// SPDX-License-Identifier: MPL-2.0
use carbon_account::desktop::{AccountState, AccountView, Accounts};
use serde::Serialize;
use std::path::Path;
use tauri::{State, WebviewWindow};

#[cfg(any(windows, target_os = "linux"))]
mod native;
#[cfg(any(windows, target_os = "linux"))]
pub use native::{Control, Dialog};
#[cfg(any(windows, target_os = "linux"))]
use std::sync::{atomic::AtomicBool, Arc};

#[derive(Clone)]
pub struct DesktopAccounts {
    service: Accounts,
    #[cfg(any(windows, target_os = "linux"))]
    dialog_open: Arc<AtomicBool>,
}

impl DesktopAccounts {
    pub fn new(root: &Path) -> Self {
        #[cfg(not(any(windows, target_os = "linux")))]
        let _ = root;
        Self {
            #[cfg(any(windows, target_os = "linux"))]
            service: Accounts::new(root),
            #[cfg(not(any(windows, target_os = "linux")))]
            service: Accounts::session_only(),
            #[cfg(any(windows, target_os = "linux"))]
            dialog_open: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AccountStatus {
    Unsupported,
    Unavailable,
    SignIn,
    Available,
    Verified,
}

fn toolbar_status(view: &AccountView, native_window_available: bool) -> AccountStatus {
    if !native_window_available {
        return AccountStatus::Unsupported;
    }
    if view.state == AccountState::Unavailable {
        return AccountStatus::Unavailable;
    }
    if view
        .accounts
        .iter()
        .any(|row| Some(row.id) == view.active_account && row.verified)
    {
        return AccountStatus::Verified;
    }
    if view.accounts.is_empty()
        && matches!(view.state, AccountState::SignedOut | AccountState::Error)
    {
        return AccountStatus::SignIn;
    }
    AccountStatus::Available
}

#[tauri::command]
pub fn minecraft_account_status(accounts: State<'_, DesktopAccounts>) -> AccountStatus {
    toolbar_status(
        &accounts.service.snapshot(),
        cfg!(any(windows, target_os = "linux")),
    )
}

#[tauri::command]
pub async fn show_minecraft_accounts(
    window: WebviewWindow,
    accounts: State<'_, DesktopAccounts>,
) -> Result<(), String> {
    #[cfg(any(windows, target_os = "linux"))]
    {
        let accounts = accounts.inner().clone();
        let owner = window.clone();
        let (send, mut receive) = tauri::async_runtime::channel(1);
        window
            .run_on_main_thread(move || {
                let result = match native::DialogGuard::acquire(&accounts.dialog_open) {
                    Some(_guard) => {
                        crate::dialogs::show_accounts(&owner, Dialog::new(accounts.service))
                    }
                    None => Ok(()),
                };
                let _ = send.try_send(result);
            })
            .map_err(|_| "Could not open the native account window.")?;
        receive
            .recv()
            .await
            .ok_or("The native account window closed unexpectedly.")?
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = (window, accounts);
        Err("Native account windows are unavailable on this platform.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use carbon_account::desktop::AccountSummary;
    use uuid::Uuid;

    #[test]
    fn webview_account_status_redacts_profiles_and_only_verifies_the_active_one() {
        let profile = Uuid::from_u128(1);
        let mut view = AccountView {
            state: AccountState::Unavailable,
            message: "private detail".into(),
            name: Some("Forest".into()),
            user_code: Some("SYNTHETIC".into()),
            accounts: vec![],
            active_account: None,
            cancellable: false,
            removal_retry: None,
        };
        assert_eq!(toolbar_status(&view, false), AccountStatus::Unsupported);
        assert_eq!(toolbar_status(&view, true), AccountStatus::Unavailable);
        view.state = AccountState::SignedOut;
        assert_eq!(toolbar_status(&view, true), AccountStatus::SignIn);
        view.state = AccountState::Starting;
        assert_eq!(toolbar_status(&view, true), AccountStatus::Available);

        view.accounts.push(AccountSummary {
            id: profile,
            name: "Forest".into(),
            verified: true,
        });
        view.active_account = Some(Uuid::from_u128(2));
        assert_eq!(toolbar_status(&view, true), AccountStatus::Available);
        view.active_account = Some(profile);
        assert_eq!(toolbar_status(&view, false), AccountStatus::Unsupported);
        assert_eq!(
            serde_json::to_value(toolbar_status(&view, true)).unwrap(),
            serde_json::json!("verified")
        );
    }

    #[test]
    fn account_bridge_exposes_only_status_and_the_native_window_to_main() {
        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/review.json")).unwrap();
        assert_eq!(capability["windows"], serde_json::json!(["main"]));
        let account_permissions: Vec<_> = capability["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(serde_json::Value::as_str)
            .filter(|permission| permission.contains("account"))
            .collect();
        assert_eq!(
            account_permissions,
            [
                "allow-minecraft-account-status",
                "allow-show-minecraft-accounts"
            ]
        );
    }
}
