//! Framework-neutral instance inspection and selection of verified Java runtimes.
mod java;
use carbon_profiles::{JavaChoice, VanillaVersion};
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
    let blocked_reasons = vec![
        "Microsoft sign-in is waiting for Carbon’s application registration.",
        "The protected Minecraft game runner is still in development.",
    ];
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
