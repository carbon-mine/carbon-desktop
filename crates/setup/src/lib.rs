//! Framework-neutral instance inspection and selection of verified Java runtimes.
mod java;
use carbon_profiles::{JavaChoice, VanillaVersion};
use carbon_vanilla::launch::{preview_launch_inputs, LaunchMode, LaunchSummary, PlanError};
pub use java::RuntimeReport;
use serde::Serialize;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupReport {
    #[serde(flatten)]
    pub metadata: carbon_vanilla::PreparedVersion,
    pub runtime: java::RuntimeReport,
    pub managed_java: carbon_runtime::RuntimeStatus,
    pub client_file: carbon_vanilla::ClientStatus,
    pub game_files_cached: bool,
    pub game_files_prepared: bool,
    pub session_supported: bool,
    pub blocked_reasons: Vec<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchInputReview {
    pub summary: Option<LaunchSummary>,
    pub message: String,
}

fn launch_input_unavailable(message: impl Into<String>) -> LaunchInputReview {
    LaunchInputReview {
        summary: None,
        message: message.into(),
    }
}

/// Read and reverify existing app-managed inputs. Both shells call this on explicit review.
pub fn review_installed_launch_inputs(root: &Path, profile_id: Uuid) -> LaunchInputReview {
    if !cfg!(all(
        any(windows, target_os = "linux"),
        target_arch = "x86_64"
    )) {
        return launch_input_unavailable(
            "Vanilla launch input review is unavailable on this platform.",
        );
    }
    let Ok(Some(profile)) = carbon_profiles::existing_profile(root, profile_id) else {
        return launch_input_unavailable(
            "This instance is unavailable. Reload your instance list and retry.",
        );
    };
    if profile.imported_pack.is_some() {
        return launch_input_unavailable("Imported packs require their own launch preparation.");
    }
    let Ok(view) = carbon_vanilla::lease_installed_view(root, &profile) else {
        return launch_input_unavailable(
            "Prepare or verify this instance's private game files before reviewing launch inputs.",
        );
    };
    let summary = match preview_launch_inputs(&profile, &view, LaunchMode::OfflineSinglePlayer) {
        Ok(summary) => summary,
        Err(PlanError::GenerationChanged) => {
            return launch_input_unavailable(
                "The instance or its installed files changed during review. Check the launch inputs again.",
            );
        }
        Err(error) => return launch_input_unavailable(error.to_string()),
    };
    let message = format!(
        "Review found verified inputs for Minecraft {} with managed Java {}.",
        summary.version, summary.java_major
    );
    LaunchInputReview {
        summary: Some(summary),
        message,
    }
}

pub fn expected_choice(
    root: &Path,
    meta: &carbon_vanilla::PreparedVersion,
) -> Result<(VanillaVersion, Option<JavaChoice>), String> {
    let (version, choice) = carbon_profiles::java_choice(root, meta.profile_id)?;
    if version.id() != meta.version {
        return Err("The profile version changed. Reopen the instance and check again.".into());
    }
    Ok((version, choice))
}

pub fn required_java(meta: &carbon_vanilla::PreparedVersion) -> Result<u16, String> {
    u16::try_from(meta.java_major)
        .map_err(|_| "Unsupported Java requirement in the version metadata.".into())
}

pub fn report(root: &Path, meta: carbon_vanilla::PreparedVersion) -> Result<SetupReport, String> {
    let major = required_java(&meta)?;
    let managed_java = carbon_runtime::installed(root, major).map_err(|e| e.to_string())?;
    let (_, choice) = expected_choice(root, &meta)?;
    let runtime = java::inspect(major, choice.as_ref(), managed_java.java_home());
    let client_file = carbon_vanilla::client_status(root, meta.profile_id)?;
    if client_file.version != meta.version {
        return Err("The selected version changed. Recheck setup.".into());
    }
    let mut blocked_reasons = Vec::new();
    if !carbon_account::desktop::sign_in_configured(root) {
        blocked_reasons.push("Minecraft account sign-in is not configured for this build.");
    }
    blocked_reasons.push("Protected Minecraft launch is unavailable in this build.");
    let game_files_cached = carbon_vanilla::has_game_install(root, meta.profile_id)?;
    let game_files_prepared =
        game_files_cached && carbon_vanilla::has_instance_view(root, meta.profile_id)?;
    Ok(SetupReport {
        game_files_cached,
        game_files_prepared,
        session_supported: carbon_session::supported(),
        metadata: meta,
        runtime,
        managed_java,
        client_file,
        blocked_reasons,
    })
}

pub fn inspect(root: &Path, profile_id: Uuid, offline: bool) -> Result<SetupReport, String> {
    report(root, carbon_vanilla::prepare(root, profile_id, offline)?)
}

pub fn select_java_home(
    root: &Path,
    meta: carbon_vanilla::PreparedVersion,
    home: PathBuf,
) -> Result<SetupReport, String> {
    let (version, _) = expected_choice(root, &meta)?;
    let major = required_java(&meta)?;
    let actual = java::inspect_home(&home, major, "Selected Java folder")
        .ok_or("Choose the Java home containing its release metadata and bin directory.")?;
    if actual.state != "compatible" {
        return Err(format!("This instance requires Java {major} for this operating system and CPU. Choose another Java home."));
    }
    carbon_profiles::select_java(root, meta.profile_id, version, JavaChoice::Directory(home))?;
    report(root, meta).map_err(|reason| {
        format!("Java selection saved. Recheck setup to refresh this view. {reason}")
    })
}

pub fn prepare_managed_java(
    root: &Path,
    profile_id: Uuid,
    metadata_sha1: &str,
    on_progress: impl FnMut(u64, u64) -> bool,
) -> Result<SetupReport, String> {
    let meta = carbon_vanilla::prepare(root, profile_id, true)?;
    if meta.metadata_sha1 != metadata_sha1 {
        return Err("The reviewed Minecraft version has changed. Review the installation again before downloading a JDK.".into());
    }
    let (version, _) = expected_choice(root, &meta)?;
    let major = required_java(&meta)?;
    let prepared =
        carbon_runtime::prepare_with(root, major, on_progress).map_err(|e| e.to_string())?;
    if prepared.java_home().is_none() {
        return Err("A managed runtime is unavailable for this Java version and platform.".into());
    }
    carbon_profiles::select_java(root, profile_id, version, JavaChoice::Managed).map_err(
        |reason| {
            format!("Managed Java is prepared. The instance selection could not be saved. {reason}")
        },
    )?;
    report(root, meta).map_err(|reason| {
        format!(
            "Managed Java is prepared and selected. Recheck setup to refresh this view. {reason}"
        )
    })
}

#[cfg(test)]
mod launch_tests {
    use super::*;
    use std::{fs, path::PathBuf};

    #[test]
    fn shared_launch_review_leaves_unprepared_storage_as_it_found_it() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("library");
        let missing = review_installed_launch_inputs(&root, Uuid::nil());
        assert!(missing.summary.is_none());
        assert!(!root.exists());

        carbon_profiles::seed_review_library(&root, false).unwrap();
        let id = carbon_profiles::list(&root).unwrap()[0].id;
        let count_before = fs::read_dir(&root).unwrap().count();
        let unprepared = review_installed_launch_inputs(&root, id);
        assert!(unprepared.summary.is_none());
        assert_eq!(count_before, fs::read_dir(&root).unwrap().count());
        assert!(!root.join("vanilla-metadata-v1").exists());
        assert!(!root.join("instances-v1").exists());
    }

    #[test]
    #[ignore = "requires an isolated copy of the verified 26.3 installation and managed Java"]
    fn shared_launch_review_uses_the_installed_release_and_managed_java_leases() {
        let root = PathBuf::from(
            std::env::var_os("CARBON_LAUNCH_INPUT_REVIEW_ROOT")
                .expect("Set the disposable verified review root"),
        );
        let id: Uuid = std::env::var("CARBON_LAUNCH_INPUT_REVIEW_PROFILE")
            .expect("Set the disposable installed profile ID")
            .parse()
            .unwrap();
        if std::env::var("CARBON_LAUNCH_INPUT_PREPARE_JAVA").as_deref() == Ok("1") {
            let metadata =
                carbon_vanilla::prepare(&root, id, true).expect("Read the installed version pin");
            assert_eq!(metadata.version, "26.3");
            prepare_managed_java(&root, id, &metadata.metadata_sha1, |_, _| true)
                .expect("Prepare the app-managed Java kit in the disposable fixture");
        }
        let owner = root.join("instances-v1").join(id.to_string());
        let save = owner.join("game/saves/preview-canary.dat");
        let receipt = root.join("game-files-v1/active").join(format!("{id}.json"));
        let saved = fs::read(&save).expect("seed the isolated save canary");
        let installed = fs::read(&receipt).expect("copy the verified active receipt");
        let before = std::time::Instant::now();
        let report = review_installed_launch_inputs(&root, id);
        let summary = report
            .summary
            .unwrap_or_else(|| panic!("{}", report.message));
        assert_eq!(summary.version, "26.3");
        assert_eq!(summary.java_major, 25);
        assert_eq!(summary.identity_slots, 5);
        assert_eq!(
            summary.classpath_entries,
            if cfg!(windows) { 75 } else { 67 }
        );
        assert_eq!(fs::read(&save).unwrap(), saved);
        assert_eq!(fs::read(&receipt).unwrap(), installed);
        assert!(!owner.join(".content-stage").exists());
        println!(
            "{} Read-only review completed in {:.2}s.",
            report.message,
            before.elapsed().as_secs_f64()
        );
    }
}
