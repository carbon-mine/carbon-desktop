#[cfg(any(windows, target_os = "linux"))]
use crate::account::Dialog as AccountDialog;
use crate::Library;
#[cfg(any(windows, target_os = "linux"))]
use carbon_profiles::{Profile, ProfileAction, ServerAddress, VanillaVersion};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use tauri::{State, WebviewWindow};
use uuid::Uuid;

static THEME_PREFERENCE: AtomicUsize = AtomicUsize::new(0);

pub fn set_theme_preference(preference: usize) {
    THEME_PREFERENCE.store(preference, Ordering::Relaxed);
}

#[cfg(any(windows, target_os = "linux"))]
fn dark_theme(owner: &WebviewWindow) -> bool {
    match THEME_PREFERENCE.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => matches!(owner.theme(), Ok(tauri::Theme::Dark)),
    }
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

#[cfg(any(windows, target_os = "linux"))]
pub fn show_accounts(owner: &WebviewWindow, dialog: AccountDialog) -> Result<(), String> {
    #[cfg(windows)]
    return windows::show_accounts(owner, dialog, dark_theme(owner));
    #[cfg(target_os = "linux")]
    return linux::show_accounts(owner, dialog, dark_theme(owner));
}

#[cfg(any(windows, target_os = "linux"))]
pub async fn edit_server(
    window: WebviewWindow,
    initial: Option<ServerAddress>,
) -> Result<Option<Option<ServerAddress>>, String> {
    let dark = dark_theme(&window);
    let owner = window.clone();
    let (send, mut receive) = tauri::async_runtime::channel(1);
    window
        .run_on_main_thread(move || {
            #[cfg(windows)]
            let value = windows::edit_server(&owner, initial.as_ref(), dark);
            #[cfg(target_os = "linux")]
            let value = linux::edit_server(&owner, initial.as_ref(), dark);
            let _ = send.try_send(value);
        })
        .map_err(|_| "Could not display the native server editor.")?;
    receive
        .recv()
        .await
        .ok_or("The server editor closed unexpectedly.")?
}

#[cfg(any(windows, target_os = "linux"))]
pub async fn copy_server(window: WebviewWindow, address: ServerAddress) -> Result<(), String> {
    #[cfg(windows)]
    let owner = window.clone();
    let (send, mut receive) = tauri::async_runtime::channel(1);
    window
        .run_on_main_thread(move || {
            #[cfg(windows)]
            let _ = send.try_send(windows::copy_text(&owner, address.as_str()));
            #[cfg(target_os = "linux")]
            {
                let clipboard = gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD);
                clipboard.set_text(address.as_str());
                clipboard.request_text(move |_, text| {
                    let result = if text == Some(address.as_str()) {
                        Ok(())
                    } else {
                        Err(
                            "The clipboard could not confirm the address. Try copying it again."
                                .into(),
                        )
                    };
                    let _ = send.try_send(result);
                });
            }
        })
        .map_err(|_| "Could not open the system clipboard.")?;
    receive
        .recv()
        .await
        .ok_or("The system clipboard is unavailable.")?
}

#[derive(Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    Create,
    Edit,
    Clone,
}

#[cfg(any(windows, target_os = "linux"))]
impl Mode {
    fn title(self) -> &'static str {
        match self {
            Self::Create => "New vanilla instance",
            Self::Edit => "Edit instance settings",
            Self::Clone => "Clone instance",
        }
    }
    fn accept(self) -> &'static str {
        match self {
            Self::Create => "Create instance",
            Self::Edit => "Save changes",
            Self::Clone => "Create copy",
        }
    }
}

#[cfg(any(windows, target_os = "linux"))]
struct Form {
    mode: Mode,
    initial: Profile,
}

#[cfg(any(windows, target_os = "linux"))]
impl Form {
    fn new(mode: Mode, original: Option<&Profile>) -> Result<Self, String> {
        let mut initial = match (mode, original) {
            (Mode::Create, None) => Profile {
                id: Uuid::new_v4(),
                name: String::new(),
                version: VanillaVersion::Release26_3,
                memory_mib: 2048,
                window_width: 1280,
                window_height: 720,
                imported_pack: None,
                server_address: None,
            },
            (Mode::Edit | Mode::Clone, Some(p)) => p.clone(),
            _ => return Err("Select a valid instance for this operation.".into()),
        };
        if mode == Mode::Clone {
            let prefix = initial.name.chars().take(75).collect::<String>();
            initial.name = format!("{} copy", prefix.trim_end());
        }
        Ok(Self { mode, initial })
    }
    fn submit(
        &self,
        name: String,
        version: VanillaVersion,
        values: [String; 3],
    ) -> Result<ProfileAction, String> {
        let [memory, width, height] = values;
        let integer = |s: String| {
            s.trim().parse::<u32>().map_err(|_| {
                "Enter a whole number for memory and both window dimensions.".to_owned()
            })
        };
        let mut edited = Profile {
            id: self.initial.id,
            imported_pack: self.initial.imported_pack.clone(),
            server_address: self.initial.server_address.clone(),
            name,
            version,
            memory_mib: integer(memory)?,
            window_width: integer(width)?,
            window_height: integer(height)?,
        };
        edited.normalize_input()?;
        Ok(match self.mode {
            Mode::Clone => ProfileAction::Clone {
                id: edited.id,
                name: edited.name,
            },
            Mode::Edit => ProfileAction::Edit(edited),
            Mode::Create => ProfileAction::Create {
                name: edited.name,
                version: edited.version,
                memory_mib: edited.memory_mib,
                window_width: edited.window_width,
                window_height: edited.window_height,
            },
        })
    }
    fn numbers(&self) -> [String; 3] {
        [
            self.initial.memory_mib,
            self.initial.window_width,
            self.initial.window_height,
        ]
        .map(|n| n.to_string())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Edited {
    profiles: Vec<crate::server::LibraryProfile>,
    selected_id: Uuid,
}

#[cfg(any(windows, target_os = "linux"))]
pub async fn notice(
    owner: WebviewWindow,
    title: &'static str,
    description: String,
) -> Result<(), String> {
    let dark = dark_theme(&owner);
    let window = owner.clone();
    let (send, mut receive) = tauri::async_runtime::channel(1);
    owner
        .run_on_main_thread(move || {
            #[cfg(windows)]
            let result = windows::notice(&window, title, &description, dark);
            #[cfg(target_os = "linux")]
            let result =
                linux::message_dialog(&window, title, &description, None, dark).map(|_| ());
            let _ = send.try_send(result);
        })
        .map_err(|_| "Could not show the native dialog.")?;
    receive
        .recv()
        .await
        .ok_or("The native dialog closed unexpectedly.")?
}

#[cfg(any(windows, target_os = "linux"))]
pub async fn confirm(
    owner: WebviewWindow,
    title: &'static str,
    description: String,
    accept_label: &'static str,
) -> Result<bool, String> {
    let dark = dark_theme(&owner);
    let window = owner.clone();
    let (send, mut receive) = tauri::async_runtime::channel(1);
    owner
        .run_on_main_thread(move || {
            #[cfg(windows)]
            let result = windows::confirm(&window, title, &description, accept_label, dark);
            #[cfg(target_os = "linux")]
            let result =
                linux::message_dialog(&window, title, &description, Some(accept_label), dark);
            let _ = send.try_send(result);
        })
        .map_err(|_| "Could not show the native confirmation.")?;
    receive
        .recv()
        .await
        .ok_or("The native confirmation closed unexpectedly.")?
}

#[cfg(any(windows, target_os = "linux"))]
#[tauri::command]
pub async fn edit_profile_dialog(
    window: WebviewWindow,
    root: State<'_, Library>,
    mode: Mode,
    profile_id: Option<Uuid>,
) -> Result<Option<Edited>, String> {
    let path = root.0.clone();
    let source = path.clone();
    let original = tauri::async_runtime::spawn_blocking(move || match profile_id {
        None => Ok(None),
        Some(id) => carbon_profiles::list(&source)?
            .into_iter()
            .find(|p| p.id == id)
            .map(Some)
            .ok_or_else(|| "This instance is unavailable. Reload your library.".to_owned()),
    })
    .await
    .map_err(|_| "Could not open instance settings.")??;
    let form = Form::new(mode, original.as_ref())?;
    let dark = dark_theme(&window);
    let owner = window.clone();
    let (send, mut receive) = tauri::async_runtime::channel(1);
    window
        .run_on_main_thread(move || {
            #[cfg(windows)]
            let value = windows::show(&owner, &form, dark);
            #[cfg(target_os = "linux")]
            let value = linux::show(&owner, &form, dark);
            let _ = send.try_send(value);
        })
        .map_err(|_| "Could not display the native instance editor.")?;
    let Some(action) = receive
        .recv()
        .await
        .ok_or("The instance editor closed unexpectedly.")??
    else {
        return Ok(None);
    };
    tauri::async_runtime::spawn_blocking(move || {
        let profiles = carbon_profiles::change_if_current(&path, action, original.as_ref())?;
        let selected_id = match mode {
            Mode::Edit => profile_id.ok_or("The edited instance is unavailable.")?,
            _ => {
                profiles
                    .last()
                    .ok_or("Reload the saved instance library.")?
                    .id
            }
        };
        Ok(Some(Edited {
            profiles: profiles.into_iter().map(Into::into).collect(),
            selected_id,
        }))
    })
    .await
    .map_err(|_| "The save was interrupted. Reload the library.")?
}

#[cfg(not(any(windows, target_os = "linux")))]
pub async fn notice(_: WebviewWindow, _: &'static str, _: String) -> Result<(), String> {
    Err("Native dialogs are unavailable on this platform.".into())
}

#[cfg(not(any(windows, target_os = "linux")))]
pub async fn confirm(
    _: WebviewWindow,
    _: &'static str,
    _: String,
    _: &'static str,
) -> Result<bool, String> {
    Err("Native dialogs are unavailable on this platform.".into())
}

#[cfg(not(any(windows, target_os = "linux")))]
#[tauri::command]
pub async fn edit_profile_dialog(
    window: WebviewWindow,
    root: State<'_, Library>,
    mode: Mode,
    profile_id: Option<Uuid>,
) -> Result<Option<Edited>, String> {
    let _ = (window, root, mode, profile_id);
    Err("Native instance editing is unavailable on this platform.".into())
}
