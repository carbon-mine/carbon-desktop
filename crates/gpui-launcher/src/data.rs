/* SPDX-License-Identifier: MPL-2.0 */
use anyhow::{Context as _, Result};

use carbon_runtime::RuntimeStatus;
pub use carbon_setup::{SetupReport as Inspection, inspect};
use carbon_vanilla::{ClientState, GameFilePlan, GameFilesPrepared};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

pub struct DataRoot {
    pub path: PathBuf,
    _temporary: Option<tempfile::TempDir>,
}

impl DataRoot {
    pub fn open(temporary: bool, seed: bool, comparison: bool) -> Result<Self> {
        let session = temporary
            .then(|| {
                tempfile::Builder::new()
                    .prefix("carbon-gpui-review-")
                    .tempdir()
            })
            .transpose()
            .context("Create the disposable review directory")?;
        let path = match &session {
            Some(session) => session.path().join("library"),
            None if cfg!(windows) => std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .context("LOCALAPPDATA is unavailable")?
                .join("CarbonDesktopGpui")
                .join("library-v1"),
            None => std::env::var_os("XDG_DATA_HOME")
                .filter(|p| !p.is_empty())
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .or_else(|| {
                    std::env::var_os("HOME")
                        .map(PathBuf::from)
                        .map(|p| p.join(".local/share"))
                })
                .context("A local application data directory is unavailable")?
                .join("carbon-desktop-gpui")
                .join("library-v1"),
        };
        if seed {
            carbon_profiles::seed_review_library(&path, comparison).map_err(anyhow::Error::msg)?;
            if carbon_store::snapshot(&path)
                .map_err(anyhow::Error::msg)?
                .instances
                .is_empty()
            {
                carbon_store::apply(&path, carbon_store::Action::CreateSample)
                    .map_err(anyhow::Error::msg)?;
            }
        }
        Ok(Self {
            path,
            _temporary: session,
        })
    }
}

pub fn client_verified(inspection: &Inspection) -> bool {
    inspection.client_file.status == ClientState::Verified
}

#[derive(Clone, Default)]
pub struct Activity {
    pub cancel: Arc<AtomicBool>,
    line: Arc<Mutex<Option<String>>>,
    prepared_jdk: Arc<Mutex<Option<(String, RuntimeStatus)>>>,
}

impl Activity {
    pub fn text(&self) -> Option<String> {
        self.line.lock().ok()?.clone()
    }

    pub fn prepared_jdk_release(&self, metadata_sha1: &str) -> Option<&'static str> {
        let prepared = self.prepared_jdk.lock().ok()?;
        match prepared.as_ref()? {
            (digest, RuntimeStatus::Ready { release, .. }) if digest == metadata_sha1 => {
                Some(release)
            }
            _ => None,
        }
    }

    pub fn take_prepared_jdk(&self, metadata_sha1: &str) -> Option<RuntimeStatus> {
        let mut prepared = self.prepared_jdk.lock().ok()?;
        if prepared.as_ref()?.0 == metadata_sha1 {
            return prepared.take().map(|(_, status)| status);
        }
        None
    }

    pub fn progress(&self, text: String) -> bool {
        if let Ok(mut line) = self.line.lock() {
            line.replace(text);
        }
        !self.cancel.load(Ordering::Acquire)
    }
}

pub fn review(path: &Path, profile: uuid::Uuid) -> Result<(Inspection, GameFilePlan), String> {
    let inspection = inspect(path, profile, false)?;
    let plan = carbon_vanilla::game_file_plan(path, profile, false)?;
    if plan.metadata_sha1 != inspection.metadata.metadata_sha1 {
        return Err("Minecraft metadata changed. Review setup again.".into());
    }
    Ok((inspection, plan))
}

pub fn prepare(
    path: &Path,
    profile: uuid::Uuid,
    metadata_sha1: &str,
    offline: bool,
    activity: &Activity,
) -> Result<(GameFilesPrepared, Result<Inspection, String>), String> {
    if !offline {
        let inspection = inspect(path, profile, true)?;
        if inspection.metadata.metadata_sha1 != metadata_sha1 {
            return Err("Minecraft metadata changed. Review setup again.".into());
        }
        if matches!(
            inspection.managed_java,
            RuntimeStatus::Available { .. } | RuntimeStatus::RepairNeeded { .. }
        ) {
            let major = carbon_setup::required_java(&inspection.metadata)?;
            let verified = carbon_setup::prepare_managed_java(
                path,
                profile,
                metadata_sha1,
                |received, total| {
                    activity.progress(format!(
                        "Preparing Java {major} JDK · {} / {}",
                        size_label(received),
                        size_label(total)
                    ))
                },
            )?;
            let status = verified.managed_java;
            if status.java_home().is_some()
                && let Ok(mut prepared) = activity.prepared_jdk.lock()
            {
                prepared.replace((metadata_sha1.to_owned(), status));
            }
            if activity.cancel.load(Ordering::Acquire) {
                return Err("Preparation cancelled after saving the JDK.".into());
            }
        }
    }
    let receipt =
        carbon_vanilla::prepare_reviewed_instance(path, profile, offline, metadata_sha1, |p| {
            let phase = match p.phase {
                "downloading" => "Downloading game files",
                "verifyingView" => "Verifying the game folder",
                "copyingView" => "Preparing the game folder",
                "activatingView" => "Activating the game folder",
                _ => "Verifying game files",
            };
            activity.progress(format!(
                "{phase} · {} / {} files · {} / {}",
                p.completed_files,
                p.total_files,
                size_label(p.verified_bytes),
                size_label(p.total_bytes)
            ))
        })?;
    let refresh = inspect(path, profile, true).and_then(|report| {
        if report.metadata.metadata_sha1 == receipt.metadata_sha1 {
            Ok(report)
        } else {
            Err("Minecraft metadata changed. Review setup again.".into())
        }
    });
    Ok((receipt, refresh))
}

pub fn size_label(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / 1024.0 / 1024.0)
    } else {
        format!("{:.0} KiB", bytes as f64 / 1024.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_jdk_can_be_recovered_only_for_the_reviewed_metadata() {
        let activity = Activity::default();
        activity.prepared_jdk.lock().unwrap().replace((
            "reviewed".into(),
            RuntimeStatus::Ready {
                major: 25,
                release: "test-release",
                download_bytes: 0,
                java_home: PathBuf::from("verified"),
            },
        ));
        activity.cancel.store(true, Ordering::Release);

        assert_eq!(
            activity.prepared_jdk_release("reviewed"),
            Some("test-release")
        );
        assert_eq!(activity.prepared_jdk_release("different"), None);
        assert!(activity.take_prepared_jdk("different").is_none());
        assert!(
            activity
                .take_prepared_jdk("reviewed")
                .is_some_and(|status| status.java_home().is_some())
        );
        assert!(activity.take_prepared_jdk("reviewed").is_none());
    }

    #[test]
    fn disposable_comparison_seed_keeps_personal_and_fixture_roots_together() {
        let first = DataRoot::open(true, true, false).unwrap();
        let second = DataRoot::open(true, false, false).unwrap();
        assert_ne!(first.path, second.path);
        let profiles = carbon_profiles::list(&first.path).unwrap();
        assert_eq!(profiles[0].name, "Vanilla survival");
        assert_eq!(profiles[0].version.id(), "26.3");
        assert_eq!(
            carbon_store::snapshot(&first.path).unwrap().instances.len(),
            1
        );
        assert!(carbon_profiles::list(&second.path).unwrap().is_empty());
        let path = first.path.clone();
        drop(first);
        assert!(!path.exists());
    }
}
