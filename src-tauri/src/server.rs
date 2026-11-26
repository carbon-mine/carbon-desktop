// SPDX-License-Identifier: MPL-2.0
use crate::Library;
use carbon_profiles::{Profile, ServerAddress};
use serde::Serialize;
use tauri::{State, WebviewWindow};
use uuid::Uuid;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LibraryProfile {
    #[serde(flatten)]
    profile: Profile,
    #[serde(skip_serializing_if = "Option::is_none")]
    server_join_message: Option<&'static str>,
}

impl From<Profile> for LibraryProfile {
    fn from(profile: Profile) -> Self {
        let server_join_message = profile
            .server_address
            .as_ref()
            .map(|_| profile.server_join_readiness().message());
        Self {
            profile,
            server_join_message,
        }
    }
}

#[cfg(any(windows, target_os = "linux"))]
#[tauri::command]
pub async fn edit_server_dialog(
    window: WebviewWindow,
    root: State<'_, Library>,
    profile_id: Uuid,
    expected: Option<ServerAddress>,
) -> Result<Option<LibraryProfile>, String> {
    let root = root.0.clone();
    let source = root.clone();
    let profile = tauri::async_runtime::spawn_blocking(move || {
        carbon_profiles::server_profile(&source, profile_id)
    })
    .await
    .map_err(|_| "Could not read this instance's server address.")??;
    if profile.server_address != expected {
        return Err("The server address changed. Reload this instance and try again.".into());
    }
    let Some(address) = crate::dialogs::edit_server(window, profile.server_address).await? else {
        return Ok(None);
    };
    tauri::async_runtime::spawn_blocking(move || {
        carbon_profiles::set_server_address(&root, profile_id, expected.as_ref(), address)
            .map(|profile| Some(profile.into()))
    })
    .await
    .map_err(|_| "Saving the server address was interrupted. Reload this instance.")?
}

#[cfg(any(windows, target_os = "linux"))]
#[tauri::command]
pub async fn copy_server_address(
    window: WebviewWindow,
    root: State<'_, Library>,
    profile_id: Uuid,
    expected: ServerAddress,
) -> Result<(), String> {
    let root = root.0.clone();
    let address = tauri::async_runtime::spawn_blocking(move || {
        let address = carbon_profiles::server_profile(&root, profile_id)?.server_address;
        address
            .filter(|address| *address == expected)
            .ok_or_else(|| {
                "The server address changed. Reload this instance and try again.".to_owned()
            })
    })
    .await
    .map_err(|_| "Could not read this instance's server address.")??;
    crate::dialogs::copy_server(window, address).await
}

#[cfg(not(any(windows, target_os = "linux")))]
#[tauri::command]
pub async fn edit_server_dialog(
    window: WebviewWindow,
    root: State<'_, Library>,
    profile_id: Uuid,
    expected: Option<ServerAddress>,
) -> Result<Option<LibraryProfile>, String> {
    let _ = (window, root, profile_id, expected);
    Err("The native server editor is unavailable on this platform.".into())
}

#[cfg(not(any(windows, target_os = "linux")))]
#[tauri::command]
pub async fn copy_server_address(
    window: WebviewWindow,
    root: State<'_, Library>,
    profile_id: Uuid,
    expected: ServerAddress,
) -> Result<(), String> {
    let _ = (window, root, profile_id, expected);
    Err("Select and copy the displayed address. Clipboard integration is unavailable on this platform.".into())
}
