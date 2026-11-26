// SPDX-License-Identifier: MPL-2.0
//! Pure planning for reviewed vanilla content. Launch admission belongs to the trusted runner.
mod arguments;

use super::{plan::Platform, PreparedInstanceView};
use carbon_profiles::{Profile, VanillaVersion};
use carbon_runtime::RuntimeLease;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{fmt, path::Path};
use uuid::Uuid;

pub use arguments::{GameArgument, IdentitySlot, JvmArgument};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LaunchMode {
    OfflineSinglePlayer,
    OnlineMultiplayer,
}

/// Trusted selection reference. It represents the identity and evidence revision only.
/// The account service still has to authorize the requested mode at admission.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct AccountReference {
    profile: Uuid,
    revision: u64,
}

impl AccountReference {
    pub fn new(profile: Uuid, revision: u64) -> Result<Self, PlanError> {
        if profile.is_nil() || revision == 0 {
            return Err(PlanError::AccountChanged);
        }
        Ok(Self { profile, revision })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlanError {
    UnsupportedPlatform,
    UnsupportedVersion,
    UnsupportedLegacy,
    UnsupportedRule,
    MalformedManifest,
    UnexpectedMainClass,
    UnsupportedArgument,
    MissingMaterial,
    IncompatibleRuntime,
    InvalidProfile,
    AccountChanged,
    GenerationChanged,
    ContentChanged,
    RuntimeChanged,
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UnsupportedPlatform => "Vanilla launch planning is unavailable on this platform.",
            Self::UnsupportedVersion => {
                "This Minecraft version is outside the launch planning corpus."
            }
            Self::UnsupportedLegacy => {
                "Legacy or inherited launch metadata requires its own adapter."
            }
            Self::UnsupportedRule => "The manifest requests an unsupported launch argument rule.",
            Self::MalformedManifest => "The verified manifest has an invalid launch section.",
            Self::UnexpectedMainClass => "The manifest requests an unexpected entry point.",
            Self::UnsupportedArgument => "The manifest requests an unsupported launch argument.",
            Self::MissingMaterial => "The manifest and verified private view disagree.",
            Self::IncompatibleRuntime => {
                "The verified app-managed Java runtime does not match the selected version."
            }
            Self::InvalidProfile => "The instance settings do not match the verified view.",
            Self::AccountChanged => "The selected Minecraft account or its evidence has changed.",
            Self::GenerationChanged => {
                "The active game generation no longer matches the launch plan."
            }
            Self::ContentChanged => "The leased game content changed or needs repair.",
            Self::RuntimeChanged => "The leased Java runtime changed or needs repair.",
        })
    }
}

impl std::error::Error for PlanError {}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchSummary {
    pub version: &'static str,
    pub platform: String,
    pub mode: LaunchMode,
    pub main_class: &'static str,
    pub java_major: u16,
    pub java_release: &'static str,
    pub classpath_entries: usize,
    pub jvm_arguments: usize,
    pub game_arguments: usize,
    pub identity_slots: usize,
}

/// Trusted launch plan with borrowed leases. The account reference, paths and arguments are
/// intentionally excluded from serialization. A plan has no execution or authorization API.
pub struct LaunchPlan<'a> {
    view: &'a PreparedInstanceView,
    runtime: &'a RuntimeLease,
    account: AccountReference,
    mode: LaunchMode,
    profile: Profile,
    activation_digest: [u8; 32],
    jvm: Vec<JvmArgument>,
    game: Vec<GameArgument>,
}

impl LaunchPlan<'_> {
    pub fn instance(&self) -> Uuid {
        self.profile.id
    }

    pub fn activation_digest(&self) -> [u8; 32] {
        self.activation_digest
    }

    pub fn runtime_archive_sha256(&self) -> &'static str {
        self.runtime.archive_sha256()
    }

    pub fn mode(&self) -> LaunchMode {
        self.mode
    }

    pub fn executable(&self) -> &Path {
        self.runtime.java_executable()
    }

    pub fn assets_directory(&self) -> &Path {
        self.view.paths().assets_dir()
    }

    pub fn main_class(&self) -> &'static str {
        arguments::MAIN_CLASS
    }

    pub fn jvm_arguments(&self) -> &[JvmArgument] {
        &self.jvm
    }

    pub fn game_arguments(&self) -> &[GameArgument] {
        &self.game
    }

    pub fn summary(&self) -> LaunchSummary {
        LaunchSummary {
            version: self.profile.version.id(),
            platform: self.view.summary().platform.clone(),
            mode: self.mode,
            main_class: self.main_class(),
            java_major: self.runtime.major(),
            java_release: self.runtime.release(),
            classpath_entries: self.view.paths().class_path().len(),
            jvm_arguments: self.jvm.len(),
            game_arguments: self.game.len(),
            identity_slots: self
                .game
                .iter()
                .filter(|arg| matches!(arg, GameArgument::Identity(_)))
                .count(),
        }
    }

    /// Recheck with fresh trusted selection and generation state under supervisor ownership.
    /// The runner separately validates the account service's decision immediately before spawn.
    pub fn recheck(
        &self,
        profile: &Profile,
        active: [u8; 32],
        account: AccountReference,
    ) -> Result<(), PlanError> {
        matches_binding(
            &self.profile,
            profile,
            self.activation_digest,
            active,
            self.account,
            account,
        )?;
        self.view
            .verify_for_launch()
            .map_err(|_| PlanError::ContentChanged)?;
        self.runtime
            .verify_current()
            .map_err(|_| PlanError::RuntimeChanged)
    }
}

fn matches_binding(
    expected: &Profile,
    profile: &Profile,
    generation: [u8; 32],
    active: [u8; 32],
    account: AccountReference,
    selected: AccountReference,
) -> Result<(), PlanError> {
    if selected != account {
        return Err(PlanError::AccountChanged);
    }
    if generation != active {
        return Err(PlanError::GenerationChanged);
    }
    if profile.id != expected.id
        || profile.version != expected.version
        || profile.imported_pack.is_some()
        || profile.memory_mib != expected.memory_mib
        || profile.window_width != expected.window_width
        || profile.window_height != expected.window_height
    {
        return Err(PlanError::InvalidProfile);
    }
    Ok(())
}

fn required_major(version: VanillaVersion) -> Result<u16, PlanError> {
    match version {
        VanillaVersion::Release26_3 => Ok(25),
        VanillaVersion::Snapshot24w14a => Ok(21),
        _ => Err(PlanError::UnsupportedVersion),
    }
}

/// Identity for the exact verified manifest and its ordered artifact map, scoped to this
/// instance and platform. No local paths or account data participate.
pub fn activation_digest(view: &PreparedInstanceView) -> [u8; 32] {
    let plan = view.plan();
    let mut digest = Sha256::new();
    digest.update(b"carbon-vanilla-activation-v1\0");
    digest.update(plan.summary.profile_id.as_bytes());
    digest.update(plan.summary.platform.as_bytes());
    digest.update([0]);
    digest.update(&plan.metadata);
    for (path, sha1) in &plan.paths {
        digest.update([0]);
        digest.update(path.as_bytes());
        digest.update([0]);
        digest.update(sha1.as_bytes());
    }
    digest.finalize().into()
}

/// Construct a plan entirely from already verified inputs. Holding the returned plan keeps
/// both source leases borrowed. The caller supplies the trusted profile and account reference.
pub fn plan_launch<'a>(
    profile: &Profile,
    view: &'a PreparedInstanceView,
    runtime: &'a RuntimeLease,
    mode: LaunchMode,
    account: AccountReference,
) -> Result<LaunchPlan<'a>, PlanError> {
    let summary = view.summary();
    let platform =
        Platform::launch_platform(&summary.platform).ok_or(PlanError::UnsupportedPlatform)?;
    if Platform::current()
        .map_err(|_| PlanError::UnsupportedPlatform)?
        .label()
        != summary.platform
    {
        return Err(PlanError::UnsupportedPlatform);
    }
    let required = required_major(profile.version)?;
    if runtime.major() != required || !runtime.belongs_to(view.root()) {
        return Err(PlanError::IncompatibleRuntime);
    }
    if profile.id != summary.profile_id
        || profile.version.id() != summary.version
        || profile.imported_pack.is_some()
        || profile.clone().normalize_input().is_err()
        || !view.paths().content_dir().is_absolute()
        || !runtime.java_executable().is_absolute()
    {
        return Err(PlanError::InvalidProfile);
    }
    let paths = view.paths();
    let parts = arguments::build(
        &view.plan().metadata,
        platform,
        profile,
        required,
        arguments::Material {
            asset_index: paths.asset_index_id(),
            classpath: paths.class_path(),
            log: paths.logging_config(),
        },
    )?;
    Ok(LaunchPlan {
        view,
        runtime,
        account,
        mode,
        profile: profile.clone(),
        activation_digest: activation_digest(view),
        jvm: parts.jvm,
        game: parts.game,
    })
}

#[cfg(test)]
mod tests;
