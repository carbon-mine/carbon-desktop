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
use carbon_account::simulation::{ROOT_MARKER_FILE, Simulation, mark_root, root_is_marked};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(crate) const ACCOUNT_DIR: &str = "dev-account-simulation";

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
    mark_root(&account_root)
        .map_err(anyhow::Error::msg)
        .with_context(|| {
            format!(
                "Validate the developer account marker at {}",
                account_root.join(ROOT_MARKER_FILE).display()
            )
        })?;
    if !root_is_marked(&account_root) {
        bail!("The developer account root marker could not be confirmed.");
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
        assert!(session.account_root.join(ROOT_MARKER_FILE).is_file());
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
        let marker = session.account_root.join(ROOT_MARKER_FILE);
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
