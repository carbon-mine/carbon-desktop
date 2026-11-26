#[cfg(any(windows, target_os = "linux"))]
use crate::account::Dialog as AccountDialog;
use crate::Library;
use carbon_profiles::{icons::IconDraft, Profile};
#[cfg(any(windows, target_os = "linux"))]
use carbon_profiles::{ProfileAction, VanillaVersion};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use tauri::{State, WebviewWindow};
use uuid::Uuid;

static THEME_PREFERENCE: AtomicUsize = AtomicUsize::new(0);

pub fn set_theme_preference(preference: usize) {
    THEME_PREFERENCE.store(preference, Ordering::Relaxed);
}

fn dark_theme(owner: &WebviewWindow) -> bool {
    match THEME_PREFERENCE.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => matches!(owner.theme(), Ok(tauri::Theme::Dark)),
    }
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod mac_icon;
#[cfg(windows)]
mod windows;

#[cfg(any(windows, target_os = "linux"))]
pub fn show_accounts(owner: &WebviewWindow, dialog: AccountDialog) -> Result<(), String> {
    #[cfg(windows)]
    return windows::show_accounts(owner, dialog, dark_theme(owner));
    #[cfg(target_os = "linux")]
    return linux::show_accounts(owner, dialog, dark_theme(owner));
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
                icon_id: None,
                imported_pack: None,
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
            icon_id: self.initial.icon_id.clone(),
            imported_pack: self.initial.imported_pack.clone(),
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
    profiles: Vec<Profile>,
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
            profiles,
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

#[tauri::command]
pub async fn choose_instance_icon(
    window: WebviewWindow,
    root: State<'_, Library>,
    profile_id: Uuid,
) -> Result<Option<Profile>, String> {
    let root = root.0.clone();
    let source = root.clone();
    let original = tauri::async_runtime::spawn_blocking(move || {
        carbon_profiles::list(&source)?
            .into_iter()
            .find(|profile| profile.id == profile_id)
            .ok_or_else(|| "This instance is unavailable. Reload the library.".to_string())
    })
    .await
    .map_err(|_| "Could not open the instance icon picker.")??;
    let draft = IconDraft::new(&original);
    let owner = window.clone();
    let dark = dark_theme(&window);
    let (send, mut receive) = tauri::async_runtime::channel(1);
    window
        .run_on_main_thread(move || {
            #[cfg(windows)]
            let result = windows::show_icon(&owner, &draft, dark);
            #[cfg(target_os = "linux")]
            let result = linux::show_icon(&owner, &draft, dark);
            #[cfg(target_os = "macos")]
            let result = mac_icon::show_icon(&owner, &draft, dark);
            let _ = send.try_send(result);
        })
        .map_err(|_| "Could not show the native icon picker.")?;
    let Some(draft) = receive
        .recv()
        .await
        .ok_or("The icon picker closed unexpectedly.")??
    else {
        return Ok(None);
    };
    tauri::async_runtime::spawn_blocking(move || draft.save(&root).map(Some))
        .await
        .map_err(|_| "Saving the instance icon was interrupted.")?
}
