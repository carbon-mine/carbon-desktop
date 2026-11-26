#![cfg_attr(windows, windows_subsystem = "windows")]

mod dialogs;
mod frame;
mod menu;
mod setup;

use carbon_store::{Action, InstanceId, Snapshot};
use std::path::PathBuf;
use tauri::{Manager, State};

struct Library(PathBuf);

#[tauri::command]
async fn fixture_library(root: State<'_, Library>) -> Result<Snapshot, String> {
    let path = root.0.clone();
    tauri::async_runtime::spawn_blocking(move || carbon_store::snapshot(&path))
        .await
        .map_err(|_| "Reading the fixture library was interrupted.".to_owned())?
}

#[tauri::command]
async fn fixture_action(root: State<'_, Library>, action: Action) -> Result<Snapshot, String> {
    let path = root.0.clone();
    tauri::async_runtime::spawn_blocking(move || carbon_store::apply(&path, action))
        .await
        .map_err(|_| {
            "The fixture operation was interrupted. Reopen the library to recover.".to_owned()
        })?
}

#[tauri::command]
async fn personal_library(
    root: State<'_, Library>,
) -> Result<Vec<carbon_profiles::Profile>, String> {
    let path = root.0.clone();
    tauri::async_runtime::spawn_blocking(move || carbon_profiles::list(&path))
        .await
        .map_err(|_| "Reading personal profiles was interrupted.".to_owned())?
}

#[tauri::command]
async fn open_game_folder(root: State<'_, Library>, profile_id: uuid::Uuid) -> Result<(), String> {
    let path = root.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        carbon_profiles::open_game_folder(&path, profile_id)
    })
    .await
    .map_err(|_| "Opening the game folder was interrupted.".to_owned())?
}

#[derive(serde::Serialize)]
struct Readiness {
    available: bool,
    reason: String,
}

#[tauri::command]
fn play_readiness(instance: InstanceId) -> Readiness {
    match instance {
        InstanceId::Sample | InstanceId::Repeat | InstanceId::Overlap => Readiness {
            available: false,
            reason: format!(
                "{} This sample contains synthetic files. You can update or repair it here.",
                carbon_account::fixture_play_blocker()
            ),
        },
    }
}

#[tauri::command]
async fn import_pack(
    window: tauri::WebviewWindow,
    root: State<'_, Library>,
) -> Result<Option<carbon_profiles::Profile>, String> {
    let chosen = rfd::AsyncFileDialog::new()
        .set_parent(&window)
        .set_title("Import a Minecraft pack")
        .add_filter("Modrinth pack", &["mrpack"])
        .pick_file()
        .await;
    let Some(file) = chosen else {
        return Ok(None);
    };
    let path = file.path().to_path_buf();
    let source = path.clone();
    let checked = tauri::async_runtime::spawn_blocking(move || carbon_pack::review_file(&source))
        .await
        .map_err(|_| "Reviewing the pack was interrupted.")?;
    let review = match checked {
        Ok(review) => review,
        Err(reason) => {
            dialogs::notice(window, "Could not import pack", reason.clone()).await?;
            return Err(reason);
        }
    };
    if !dialogs::confirm(window, "Import pack", review.description, "Import pack").await? {
        return Ok(None);
    }
    let root = root.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        carbon_pack::import_reviewed(&root, &path, &review.sha256, review.import_id)
    })
    .await
    .map_err(|_| "Importing the pack was interrupted.")?
    .map(Some)
}

fn main() {
    tauri::Builder::default()
        .menu(menu::build)
        .on_menu_event(menu::dispatch)
        .manage(setup::SetupJobs::default())
        .setup(|app| {
            let comparison = std::env::args_os().any(|a| a == "--comparison-review");
            let temporary = comparison || std::env::args_os().any(|a| a == "--temporary-review");
            let review_workspace = std::env::args_os().any(|a| a == "--standalone-review");
            let root = if temporary {
                let session = tempfile::Builder::new()
                    .prefix("carbon-desktop-review-")
                    .tempdir()?;
                let root = session.path().join("fixture");
                app.manage(session);
                root
            } else if review_workspace {
                app.path()
                    .app_local_data_dir()?
                    .join("standalone-review-v1")
            } else {
                app.path().app_local_data_dir()?.join("fixture-library-v1")
            };
            if (comparison || std::env::args_os().any(|a| a == "--seed-review"))
                && carbon_store::snapshot(&root)
                    .map_err(std::io::Error::other)?
                    .instances
                    .is_empty()
            {
                carbon_store::apply(&root, Action::CreateSample).map_err(std::io::Error::other)?;
            }
            let sample = temporary && std::env::args_os().any(|a| a == "--sample-vanilla");
            if sample || comparison || review_workspace {
                carbon_profiles::seed_review_library(&root, comparison)
                    .map_err(std::io::Error::other)?;
            }
            #[cfg(windows)]
            if let Some(window) = app.get_webview_window("main") {
                frame::remember(&window, root.join("window-placement.json"));
                setup::guard_test_window(&window, app.state::<setup::SetupJobs>().inner().clone());
            }
            app.manage(Library(root));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            fixture_library,
            frame::apply_desktop_theme,
            fixture_action,
            play_readiness,
            dialogs::edit_profile_dialog,
            personal_library,
            open_game_folder,
            import_pack,
            setup::inspect_vanilla_profile,
            setup::review_installed_launch_inputs,
            setup::import_vanilla_client,
            setup::select_profile_java,
            setup::install_profile_java,
            setup::cancel_java_install,
            setup::plan_game_files,
            setup::prepare_game_files,
            setup::cancel_game_files,
            setup::run_sandbox_session
        ])
        .run(tauri::generate_context!())
        .expect("Carbon Desktop could not start its window");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synthetic_instances_always_refuse_execution() {
        for instance in [InstanceId::Sample, InstanceId::Repeat, InstanceId::Overlap] {
            assert!(!play_readiness(instance).available);
        }
    }
}
