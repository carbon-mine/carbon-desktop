use crate::Library;
use carbon_setup::{expected_choice, report, required_java, SetupReport};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| "Setup was interrupted. Reopen the instance and check again.".to_owned())?
}

#[tauri::command]
pub async fn inspect_vanilla_profile(
    root: State<'_, Library>,
    profile_id: Uuid,
    offline: bool,
) -> Result<SetupReport, String> {
    let path = root.0.clone();
    blocking(move || carbon_setup::inspect(&path, profile_id, offline)).await
}

#[tauri::command]
pub async fn review_installed_launch_inputs(
    root: State<'_, Library>,
    profile_id: Uuid,
) -> Result<carbon_setup::LaunchInputReview, String> {
    let path = root.0.clone();
    blocking(move || {
        Ok(carbon_setup::review_installed_launch_inputs(
            &path, profile_id,
        ))
    })
    .await
}

#[tauri::command]
pub async fn select_profile_java(
    window: tauri::WebviewWindow,
    root: State<'_, Library>,
    profile_id: Uuid,
) -> Result<Option<SetupReport>, String> {
    let path = root.0.clone();
    let before = path.clone();
    let meta = blocking(move || carbon_vanilla::prepare(&before, profile_id, true)).await?;
    let chosen = rfd::AsyncFileDialog::new()
        .set_parent(&window)
        .set_title("Choose an installed Java home")
        .pick_folder()
        .await;
    let Some(folder) = chosen else {
        return Ok(None);
    };
    let home = PathBuf::from(folder.path());
    blocking(move || carbon_setup::select_java_home(&path, meta, home).map(Some)).await
}

#[tauri::command]
pub async fn import_vanilla_client(
    window: tauri::WebviewWindow,
    root: State<'_, Library>,
    profile_id: Uuid,
) -> Result<Option<SetupReport>, String> {
    let path = root.0.clone();
    let before = path.clone();
    let meta = blocking(move || carbon_vanilla::prepare(&before, profile_id, true)).await?;
    let chosen = rfd::AsyncFileDialog::new()
        .set_parent(&window)
        .set_title(format!("Select the Minecraft {} client JAR", meta.version))
        .add_filter("Minecraft client", &["jar"])
        .pick_file()
        .await;
    let Some(file) = chosen else {
        return Ok(None);
    };
    let selected = file.path().to_owned();
    blocking(move || {
        expected_choice(&path, &meta)?;
        let handle = carbon_vanilla::open_client_file(&selected)?;
        carbon_vanilla::import_client(&path, profile_id, handle)?;
        report(&path, meta).map(Some).map_err(|reason| {
            format!("Client JAR imported. Recheck setup to refresh this view. {reason}")
        })
    })
    .await
}

#[derive(Clone, Default)]
pub struct SetupJobs(Arc<Mutex<Option<Install>>>);
#[derive(Clone, Copy, PartialEq)]
enum Work {
    Java,
    Game,
    Session,
}
struct Install {
    kind: Work,
    profile: Uuid,
    cancelled: Arc<AtomicBool>,
}
struct Reservation {
    jobs: SetupJobs,
    cancelled: Arc<AtomicBool>,
}
impl SetupJobs {
    fn begin(&self, profile: Uuid, kind: Work) -> Result<Reservation, String> {
        let mut slot = self
            .0
            .lock()
            .map_err(|_| "Restart Carbon Desktop to reset the setup service.")?;
        if slot.is_some() {
            return Err("An instance operation is already running. Check Activity.".into());
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        *slot = Some(Install {
            kind,
            profile,
            cancelled: cancelled.clone(),
        });
        Ok(Reservation {
            jobs: self.clone(),
            cancelled,
        })
    }
    fn cancel(&self, id: Uuid, kind: Work) -> Result<(), String> {
        let slot = self
            .0
            .lock()
            .map_err(|_| "The Java installation service is unavailable.")?;
        if let Some(job) = slot.as_ref().filter(|j| j.profile == id && j.kind == kind) {
            job.cancelled.store(true, Ordering::Release);
        }
        Ok(())
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.jobs.0.lock() {
            *slot = None;
        }
    }
}

#[cfg(windows)]
pub fn guard_test_window(window: &tauri::WebviewWindow, jobs: SetupJobs) {
    let showing = Arc::new(AtomicBool::new(false));
    let parent = window.clone();
    window.on_window_event(move |event| {
        let tauri::WindowEvent::CloseRequested { api, .. } = event else {return};
        if jobs.0.lock().is_ok_and(|slot|slot.as_ref().is_some_and(|job|job.kind==Work::Session)) {
            api.prevent_close();
            if !showing.swap(true,Ordering::AcqRel) {
                let owner = parent.clone();
                let showing = showing.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = crate::dialogs::notice(owner, "Protected test window is open",
                        "Finish the protected Java test before closing Carbon Desktop. In the test window, choose Save and close or Close without saving.".into()).await;
                    showing.store(false,Ordering::Release);
                });
            }
        }
    });
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    profile_id: Uuid,
    received_bytes: u64,
    total_bytes: u64,
}

#[tauri::command]
pub fn cancel_java_install(jobs: State<'_, SetupJobs>, profile_id: Uuid) -> Result<(), String> {
    jobs.cancel(profile_id, Work::Java)
}

#[tauri::command]
pub async fn install_profile_java(
    app: AppHandle,
    root: State<'_, Library>,
    jobs: State<'_, SetupJobs>,
    profile_id: Uuid,
    metadata_sha1: String,
) -> Result<SetupReport, String> {
    let path = root.0.clone();
    let job = jobs.begin(profile_id, Work::Java)?;
    blocking(move || {
        let mut last: Option<(Instant, u64)> = None;
        carbon_setup::prepare_managed_java(&path, profile_id, &metadata_sha1, |received, total| {
            let now = Instant::now();
            if last.is_none_or(|(stamp, prev)| {
                prev != received
                    && (received == total
                        || now.duration_since(stamp) >= Duration::from_millis(150))
            }) {
                let _ = app.emit_to(
                    "main",
                    "java-progress",
                    Progress {
                        profile_id,
                        received_bytes: received,
                        total_bytes: total,
                    },
                );
                last = Some((now, received));
            }
            !job.cancelled.load(Ordering::Acquire)
        })
    })
    .await
}

#[tauri::command]
pub async fn plan_game_files(
    root: State<'_, Library>,
    profile_id: Uuid,
    offline: bool,
) -> Result<carbon_vanilla::GameFilePlan, String> {
    let path = root.0.clone();
    blocking(move || carbon_vanilla::game_file_plan(&path, profile_id, offline)).await
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GameProgress {
    profile_id: Uuid,
    #[serde(flatten)]
    progress: carbon_vanilla::GameFileProgress,
}

#[tauri::command]
pub async fn prepare_game_files(
    app: AppHandle,
    root: State<'_, Library>,
    jobs: State<'_, SetupJobs>,
    profile_id: Uuid,
    offline: bool,
    metadata_sha1: String,
) -> Result<carbon_vanilla::GameFilesPrepared, String> {
    let path = root.0.clone();
    let job = jobs.begin(profile_id, Work::Game)?;
    blocking(move || {
        let mut last: Option<Instant> = None;
        carbon_vanilla::prepare_reviewed_instance(
            &path,
            profile_id,
            offline,
            &metadata_sha1,
            |progress| {
                let now = Instant::now();
                if last.is_none_or(|t| now.duration_since(t) >= Duration::from_millis(150))
                    || progress.completed_files == progress.total_files
                {
                    let _ = app.emit_to(
                        "main",
                        "game-files-progress",
                        GameProgress {
                            profile_id,
                            progress,
                        },
                    );
                    last = Some(now);
                }
                !job.cancelled.load(Ordering::Acquire)
            },
        )
    })
    .await
}

#[tauri::command]
pub fn cancel_game_files(jobs: State<'_, SetupJobs>, profile_id: Uuid) -> Result<(), String> {
    jobs.cancel(profile_id, Work::Game)
}

#[tauri::command]
pub async fn run_sandbox_session(
    root: State<'_, Library>,
    jobs: State<'_, SetupJobs>,
    profile_id: Uuid,
) -> Result<carbon_session::SessionReport, String> {
    let path = root.0.clone();
    let job = jobs.begin(profile_id, Work::Session)?;
    blocking(move || {
        let _job = job;
        let meta = carbon_vanilla::prepare(&path, profile_id, true)?;
        expected_choice(&path, &meta)?;
        carbon_session::run(&path, profile_id, required_java(&meta)?).map_err(|e| e.to_string())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_owning_profile_cancels_and_completion_releases_the_slot() {
        let jobs = SetupJobs::default();
        let profile = Uuid::new_v4();
        let job = jobs.begin(profile, Work::Java).unwrap();
        assert!(jobs.begin(Uuid::new_v4(), Work::Game).is_err());
        jobs.cancel(Uuid::new_v4(), Work::Java).unwrap();
        assert!(!job.cancelled.load(Ordering::Acquire));
        jobs.cancel(profile, Work::Game).unwrap();
        assert!(!job.cancelled.load(Ordering::Acquire));
        jobs.cancel(profile, Work::Java).unwrap();
        assert!(job.cancelled.load(Ordering::Acquire));
        drop(job);
        assert!(jobs.begin(profile, Work::Java).is_ok());
    }
}
