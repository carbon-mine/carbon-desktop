// SPDX-License-Identifier: MPL-2.0
//! Guarded startup for the opt-in developer account simulation.
//!
//! The simulation is only reachable when all of these hold: the binary was
//! compiled with the non-default `dev-account-simulation` feature, it is a
//! debug/test build, the explicit `--dev-account-simulation` flag is present,
//! and the supplied root passes the existing development-fixture ownership
//! checks. No environment variable, stored setting or packaged build can turn
//! it on.

use crate::dev_fixtures;
use anyhow::{Context as _, Result, bail};
use carbon_account::simulation::Simulation;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub(crate) const ACCOUNT_DIR: &str = "dev-account-simulation";
const MARKER_FILE: &str = "dev-account-simulation.json";
const MARKER_VERSION: u32 = 1;
const MARKER_KIND: &str = "carbon-desktop-dev-account-simulation";
const MARKER_LIMIT: u64 = 16 * 1024;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Marker {
    version: u32,
    kind: String,
    client_id: Uuid,
}

pub struct Session {
    pub account_root: PathBuf,
    pub simulation: Simulation,
}

/// Pure startup gate so the runtime/root boundary can be tested without a
/// window or a provider. `built` mirrors the compile-time feature.
pub(crate) fn gate(
    requested: bool,
    built: bool,
    debug_build: bool,
    fixture_root: Option<&Path>,
) -> Result<()> {
    if !requested {
        return Ok(());
    }
    if !built {
        bail!(
            "--dev-account-simulation requires a build with the non-default dev-account-simulation feature."
        );
    }
    if !debug_build {
        bail!("--dev-account-simulation is restricted to a debug or test build.");
    }
    let Some(root) = fixture_root else {
        bail!(
            "--dev-account-simulation requires an explicit isolated --fixture-root; the ordinary per-user data root is never used."
        );
    };
    if !root.is_absolute() {
        bail!("--dev-account-simulation requires an absolute --fixture-root.");
    }
    dev_fixtures::guard(root)?;
    Ok(())
}

pub(crate) fn prepare(root: &Path) -> Result<Session> {
    gate(true, true, cfg!(debug_assertions), Some(root))?;
    let guarded = dev_fixtures::guard(root)?;
    let account_root = guarded.path.join(ACCOUNT_DIR);
    if let Ok(metadata) = fs::symlink_metadata(&account_root)
        && (linked(&metadata) || !metadata.is_dir())
    {
        bail!(
            "The developer account root {} must be a plain directory, not a link or file.",
            account_root.display()
        );
    }
    fs::create_dir_all(&account_root).with_context(|| {
        format!(
            "Create the developer account root {}",
            account_root.display()
        )
    })?;
    let marker_path = account_root.join(MARKER_FILE);
    let marker = Marker {
        version: MARKER_VERSION,
        kind: MARKER_KIND.to_owned(),
        client_id: carbon_account::simulation::CLIENT_ID,
    };
    match fs::symlink_metadata(&marker_path) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.is_symlink() || metadata.len() > MARKER_LIMIT {
                bail!(
                    "The developer account marker at {} is not a small regular file. Preserve the directory.",
                    marker_path.display()
                );
            }
            let bytes = fs::read(&marker_path).with_context(|| {
                format!(
                    "Read the developer account marker at {}",
                    marker_path.display()
                )
            })?;
            let existing: Marker = serde_json::from_slice(&bytes).with_context(|| {
                format!(
                    "The developer account marker at {} is unreadable. Preserve the directory.",
                    marker_path.display()
                )
            })?;
            if existing.version != MARKER_VERSION
                || existing.kind != MARKER_KIND
                || existing.client_id != carbon_account::simulation::CLIENT_ID
            {
                bail!(
                    "The developer account root uses a different or unsupported marker. Choose a dedicated root."
                );
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let bytes = serde_json::to_vec_pretty(&marker)
                .context("Serialize the developer account marker")?;
            fs::write(&marker_path, bytes).with_context(|| {
                format!(
                    "Write the developer account marker at {}",
                    marker_path.display()
                )
            })?;
        }
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "Inspect the developer account marker at {}",
                    marker_path.display()
                )
            });
        }
    }
    Ok(Session {
        account_root,
        simulation: Simulation::new(),
    })
}

fn linked(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    metadata.is_symlink()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_requires_the_feature_debug_build_flag_and_root() {
        assert!(gate(false, false, false, None).is_ok());
        assert!(gate(true, false, true, None).is_err());
        assert!(gate(true, true, false, None).is_err());
        assert!(gate(true, true, true, None).is_err());
        assert!(gate(true, true, true, Some(Path::new("relative-root"))).is_err());
    }

    #[test]
    fn prepare_marks_and_owns_the_simulation_root() {
        let root = tempfile::tempdir().unwrap();
        let session = prepare(root.path()).unwrap();
        assert!(session.account_root.ends_with(ACCOUNT_DIR));
        assert!(session.account_root.join(MARKER_FILE).is_file());
        let reopened = prepare(root.path()).unwrap();
        assert_eq!(reopened.account_root, session.account_root);
        assert!(
            session
                .simulation
                .entitlement(carbon_account::simulation::Persona::Forest)
                == carbon_account::simulation::EntitlementState::Granted
        );
    }

    #[test]
    fn a_non_directory_account_root_is_refused() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join(ACCOUNT_DIR), b"not a directory").unwrap();
        assert!(prepare(root.path()).is_err());
    }

    #[test]
    fn an_unsupported_marker_is_refused_without_deleting_anything() {
        let root = tempfile::tempdir().unwrap();
        let session = prepare(root.path()).unwrap();
        let marker = session.account_root.join(MARKER_FILE);
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&marker).unwrap()).unwrap();
        value["version"] = serde_json::json!(999);
        fs::write(&marker, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(prepare(root.path()).is_err());
        assert!(marker.is_file(), "a refused marker is preserved");
    }

    #[test]
    fn the_ordinary_per_user_root_is_refused() {
        let Some(local) = std::env::var_os("LOCALAPPDATA") else {
            return;
        };
        let ordinary = PathBuf::from(local)
            .join("CarbonDesktopGpui")
            .join("library-v1");
        assert!(gate(true, true, true, Some(&ordinary)).is_err());
    }
}
