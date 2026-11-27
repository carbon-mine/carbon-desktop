// SPDX-License-Identifier: MPL-2.0
//! A labelled window for the repository-owned isolation fixture. No Minecraft launches.
use serde::Serialize;
use std::{fmt, io, path::Path};
use uuid::Uuid;

#[cfg(any(test, all(windows, target_arch = "x86_64")))]
mod reservation;
#[cfg(all(windows, target_arch = "x86_64"))]
mod windows;

#[cfg(all(windows, target_arch = "x86_64"))]
pub mod windows_state;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionReport {
    pub sample_count: u32,
    pub saved: bool,
}

#[derive(Debug)]
pub enum SessionError {
    Unsupported,
    RuntimeNotReady,
    Busy,
    RecoveryRequired,
    InvalidProfile,
    Stopped,
    Data(io::Error),
    Isolation(io::Error),
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unsupported => "This build does not include the protected Java test window.",
            Self::RuntimeNotReady => "Prepare the matching managed Java before opening the protected test window.",
            Self::Busy => "This profile already has a protected test or Java preparation in progress.",
            Self::RecoveryRequired => "A previous protected test could not confirm its processes stopped. This profile cannot start another protected test until recovery confirms it.",
            Self::InvalidProfile => "The profile cannot store a disposable test save.",
            Self::Stopped => "The protected test was stopped before it finished. Its disposable save was not published.",
            Self::Data(_) => "Could not read or write the disposable test save. Check local app storage.",
            Self::Isolation(_) => "Carbon could not complete the protected Java test. No unprotected session was opened.",
        })
    }
}

impl std::error::Error for SessionError {}

/// Whether the selected build contains the fixed fixture and a native isolation backend.
pub const fn supported() -> bool {
    #[cfg(all(windows, target_arch = "x86_64"))]
    return windows::supported();
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    false
}

/// Run the fixed local window to completion for an existing app-owned profile.
/// This blocking call accepts no executable, user-controlled directory or command arguments.
pub fn run(root: &Path, profile_id: Uuid, java_major: u16) -> Result<SessionReport, SessionError> {
    if profile_id.get_version() != Some(uuid::Version::Random) {
        return Err(SessionError::InvalidProfile);
    }
    if !supported() {
        return Err(SessionError::Unsupported);
    }
    #[cfg(all(windows, target_arch = "x86_64"))]
    return windows::run(root, profile_id, java_major, false);
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        let _ = (root, java_major);
        Err(SessionError::Unsupported)
    }
}

/// A redacted phase for one protected run. The run UUID names the reservation,
/// the pending record and the OS boundary; it carries no path or identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RunPhase {
    Starting,
    Running,
    Stopping,
    Draining,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunState {
    pub run: Uuid,
    pub instance: Uuid,
    pub phase: RunPhase,
}

#[derive(Debug)]
pub enum RunControlError {
    Unsupported,
    /// No live run with that ID in this process. An earlier stop may already have
    /// finished, so callers treat this the same as an already stopped run.
    UnknownRun,
    Boundary(io::Error),
}

impl fmt::Display for RunControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => f.write_str("This build cannot control a protected run."),
            Self::UnknownRun => f.write_str("No live protected run has that run ID."),
            Self::Boundary(error) => write!(
                f,
                "The protected run's process boundary could not be stopped: {error}"
            ),
        }
    }
}

impl std::error::Error for RunControlError {}

/// Runs this build can currently inspect or stop. Empty when this build has no
/// protected sample backend or nothing is live.
pub fn active_runs() -> Vec<RunState> {
    #[cfg(all(windows, target_arch = "x86_64"))]
    return windows::active_runs();
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    Vec::new()
}

/// Redacted state for one live run, or `None` once its owner has finished.
pub fn inspect(run: Uuid) -> Option<RunState> {
    #[cfg(all(windows, target_arch = "x86_64"))]
    return windows::inspect(run);
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        let _ = run;
        None
    }
}

/// Request one run's own process tree to stop. The run UUID must already be
/// visible through [`active_runs`], so a rejected or unknown ID changes nothing.
/// Repeating the request while the run drains reports the same state.
pub fn stop(run: Uuid) -> Result<RunState, RunControlError> {
    if !supported() {
        return Err(RunControlError::Unsupported);
    }
    #[cfg(all(windows, target_arch = "x86_64"))]
    return windows::stop(run);
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        let _ = run;
        Err(RunControlError::Unsupported)
    }
}
