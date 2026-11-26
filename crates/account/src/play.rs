//! Redacted readiness and process-local native launch admission. No provider credentials cross it.
use serde::Serialize;
use std::{
    fmt,
    sync::{Arc, Weak},
    time::{Instant, SystemTime},
};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlayMode {
    OfflineSinglePlayer,
    OnlineMultiplayer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlayBlocker {
    RegistrationRequired,
    AccountChangePending,
    AccountRemovalPending,
    AccountSelectionRequired,
    AccountVerificationRequired,
    EntitlementDenied,
    OfflinePolicyUnavailable,
    OnlineSessionUnavailable,
    MultiplayerPermissionRequired,
    EvidenceExpired,
    EvidenceMismatch,
    ClockUntrusted,
    InstallationRequired,
    PrivateSavesRequired,
    IsolationRequired,
    LaunchStateChanged,
}

impl fmt::Display for PlayBlocker {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RegistrationRequired => "Play requires Carbon's approved Microsoft application registration.",
            Self::AccountChangePending => "Wait for the current Microsoft account change to finish.",
            Self::AccountRemovalPending => "Finish removing the selected Microsoft account before trying to play.",
            Self::AccountSelectionRequired => "Select your Microsoft account for Minecraft: Java Edition.",
            Self::AccountVerificationRequired => "Verify the selected account's Java Edition entitlement.",
            Self::EntitlementDenied => "Minecraft Services did not confirm the selected account has Java Edition.",
            Self::OfflinePolicyUnavailable => "Offline play is unavailable until Carbon can verify a provider-authorized offline entitlement.",
            Self::OnlineSessionUnavailable => "Online play is unavailable until Carbon can establish a safely scoped multiplayer session.",
            Self::MultiplayerPermissionRequired => "The selected account does not have verified multiplayer permission.",
            Self::EvidenceExpired => "The selected account's play authorization expired. Verify it again.",
            Self::EvidenceMismatch => "The authorization belongs to a different account, application, or play mode.",
            Self::ClockUntrusted => "Carbon could not verify the authorization against the current time.",
            Self::InstallationRequired => "Prepare and verify the instance's game files before playing.",
            Self::PrivateSavesRequired => "Carbon could not verify this instance's private save directory.",
            Self::IsolationRequired => "The required protected network boundary could not be established.",
            Self::LaunchStateChanged => "The account or instance changed. Review Play again.",
        })
    }
}

impl std::error::Error for PlayBlocker {}

/// Identity from the protected instance lease. The native runner must retain the
/// corresponding lease through admission and keep verification out of renderer IPC.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct Installation {
    pub instance: Uuid,
    pub activation_digest: [u8; 32],
    pub private_saves_verified: bool,
    pub network: NetworkBoundary,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum NetworkBoundary {
    OfflineEgressDenied,
    ProviderScopedOnline,
    Unverified,
}

#[derive(Clone, Copy)]
pub(super) struct Clock {
    pub wall: SystemTime,
    pub monotonic: Instant,
}

impl Clock {
    pub(super) fn now() -> Self {
        Self {
            wall: SystemTime::now(),
            monotonic: Instant::now(),
        }
    }
}

/// In-memory normalized proof from a trusted issuer. This checkpoint has no
/// provider-authorized issuer and no persisted format or parser for such proofs.
pub(super) struct Evidence {
    pub issuer_session: Weak<()>,
    pub profile: Uuid,
    pub registration: Uuid,
    pub mode: PlayMode,
    pub policy_digest: [u8; 32],
    pub observed: Clock,
    pub expires_wall: SystemTime,
    pub expires_monotonic: Instant,
    pub subscription_ends: Option<SystemTime>,
    pub multiplayer_permitted: bool,
}

#[derive(Clone, Copy)]
pub(super) struct Selection<'a> {
    pub owner: &'a Arc<()>,
    pub revision: u64,
    pub registration: Option<Uuid>,
    pub profile: Option<Uuid>,
    pub online_verified: bool,
    pub entitlement_denied: bool,
    pub pending: bool,
    pub removal_pending: bool,
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct Binding {
    profile: Uuid,
    registration: Uuid,
    revision: u64,
    mode: PlayMode,
    policy_digest: [u8; 32],
    installation: Installation,
}

/// Single-use trusted-host decision, scoped to the service lifetime. The native
/// runner must obtain it afresh and must never send it to a renderer or child process.
pub struct LaunchDecision {
    owner: Weak<()>,
    binding: Binding,
    issued: Clock,
    expires_wall: SystemTime,
    expires_monotonic: Instant,
}

impl LaunchDecision {
    pub fn profile_id(&self) -> Uuid {
        self.binding.profile
    }

    pub fn account_revision(&self) -> u64 {
        self.binding.revision
    }

    pub fn mode(&self) -> PlayMode {
        self.binding.mode
    }
}

pub(super) fn missing_evidence(selection: Selection<'_>, mode: PlayMode) -> PlayBlocker {
    use PlayBlocker::*;
    if let Err(reason) = selected(selection) {
        return reason;
    }
    if !selection.online_verified {
        return AccountVerificationRequired;
    }
    match mode {
        PlayMode::OfflineSinglePlayer => OfflinePolicyUnavailable,
        PlayMode::OnlineMultiplayer => OnlineSessionUnavailable,
    }
}

fn selected(selection: Selection<'_>) -> Result<(Uuid, Uuid), PlayBlocker> {
    use PlayBlocker::*;
    let registration = selection.registration.ok_or(RegistrationRequired)?;
    if selection.pending {
        return Err(AccountChangePending);
    }
    let profile = selection.profile.ok_or(AccountSelectionRequired)?;
    if selection.entitlement_denied {
        return Err(EntitlementDenied);
    }
    if selection.removal_pending {
        return Err(AccountRemovalPending);
    }
    Ok((registration, profile))
}

pub(super) fn issue(
    selection: Selection<'_>,
    mode: PlayMode,
    installation: Option<Installation>,
    evidence: Option<&Evidence>,
    now: Clock,
) -> Result<LaunchDecision, PlayBlocker> {
    use PlayBlocker::*;
    let Some(evidence) = evidence else {
        return Err(missing_evidence(selection, mode));
    };
    let (registration, profile) = selected(selection)?;
    if !evidence
        .issuer_session
        .ptr_eq(&Arc::downgrade(selection.owner))
        || evidence.profile != profile
        || evidence.registration != registration
        || evidence.mode != mode
        || evidence.policy_digest == [0; 32]
    {
        return Err(EvidenceMismatch);
    }
    if mode == PlayMode::OnlineMultiplayer {
        if !selection.online_verified {
            return Err(AccountVerificationRequired);
        }
        if !evidence.multiplayer_permitted {
            return Err(MultiplayerPermissionRequired);
        }
    }

    let expires_wall = if let Some(subscription_ends) = evidence.subscription_ends {
        evidence.expires_wall.min(subscription_ends)
    } else {
        evidence.expires_wall
    };
    // A backwards wall-clock adjustment cannot extend the monotonic deadline.
    // Reject a sample older than the observed wall/monotonic evidence anchors.
    if now.wall < evidence.observed.wall || now.monotonic < evidence.observed.monotonic {
        return Err(ClockUntrusted);
    }
    let validity = expires_wall
        .duration_since(evidence.observed.wall)
        .map_err(|_| EvidenceExpired)?;
    let expires_monotonic = evidence
        .observed
        .monotonic
        .checked_add(validity)
        .ok_or(ClockUntrusted)?
        .min(evidence.expires_monotonic);
    if now.wall >= expires_wall || now.monotonic >= expires_monotonic {
        return Err(EvidenceExpired);
    }

    let installation = installation
        .filter(|install| !install.instance.is_nil() && install.activation_digest != [0; 32])
        .ok_or(InstallationRequired)?;
    if !installation.private_saves_verified {
        return Err(PrivateSavesRequired);
    }
    let network_matches = matches!(
        (mode, installation.network),
        (
            PlayMode::OfflineSinglePlayer,
            NetworkBoundary::OfflineEgressDenied
        ) | (
            PlayMode::OnlineMultiplayer,
            NetworkBoundary::ProviderScopedOnline
        )
    );
    if !network_matches {
        return Err(IsolationRequired);
    }

    Ok(LaunchDecision {
        owner: Arc::downgrade(selection.owner),
        binding: Binding {
            profile,
            registration,
            revision: selection.revision,
            mode,
            policy_digest: evidence.policy_digest,
            installation,
        },
        issued: now,
        expires_wall,
        expires_monotonic,
    })
}

pub(super) fn admit<T>(
    decision: LaunchDecision,
    selection: Selection<'_>,
    installation: Option<Installation>,
    evidence: Option<&Evidence>,
    now: Clock,
    start: impl FnOnce() -> T,
) -> Result<T, PlayBlocker> {
    use PlayBlocker::*;
    if !decision.owner.ptr_eq(&Arc::downgrade(selection.owner))
        || selection.revision != decision.binding.revision
        || selection.registration != Some(decision.binding.registration)
        || selection.profile != Some(decision.binding.profile)
        || installation != Some(decision.binding.installation)
        || selection.pending
    {
        return Err(LaunchStateChanged);
    }
    if now.wall < decision.issued.wall || now.monotonic < decision.issued.monotonic {
        return Err(ClockUntrusted);
    }
    if now.wall >= decision.expires_wall || now.monotonic >= decision.expires_monotonic {
        return Err(EvidenceExpired);
    }
    let fresh = issue(
        selection,
        decision.binding.mode,
        installation,
        evidence,
        now,
    )?;
    if fresh.binding != decision.binding {
        return Err(LaunchStateChanged);
    }
    Ok(start())
}

#[cfg(test)]
#[path = "play_tests.rs"]
mod tests;
