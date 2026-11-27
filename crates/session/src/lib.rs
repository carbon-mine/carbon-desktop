// SPDX-License-Identifier: MPL-2.0
//! A labelled window for the repository-owned isolation fixture. No Minecraft launches.
use serde::Serialize;
use std::{
    fmt, io,
    path::Path,
    sync::mpsc::{self, RecvTimeoutError},
    time::Duration,
};
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
    /// The owner observed an empty tree and is settling the run. A stop request
    /// that arrives now reports this phase and cannot replace the outcome.
    Finishing,
    /// The owner retired the run. A stale observer can still read this phase
    /// from its own handle; the terminal record is [`RunOutcome`].
    Finished,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunState {
    pub run: Uuid,
    pub instance: Uuid,
    pub phase: RunPhase,
}

/// Why a protected run ended. Every value is redacted: the record carries only
/// the opaque run and instance IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RunEnd {
    /// The owner observed an empty tree and settled the completed sample.
    Completed,
    /// This process asked the run to stop before the owner completed it.
    StopRequested,
    /// The owner retired the run without settling either outcome. The caller
    /// already received the concrete error; this record does not replace it.
    Unsettled,
}

/// The terminal record for one protected run. It is retained in memory for a
/// bounded window after [`RunPhase::Finished`] so a late observer can learn the
/// outcome without reading a path, process ID or account identity.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunOutcome {
    pub run: Uuid,
    pub instance: Uuid,
    pub end: RunEnd,
}

/// One observed change for a subscribed run. State updates arrive in phase
/// order and exactly one `Finished` update ends the stream.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RunUpdate {
    State(RunState),
    Finished(RunOutcome),
}

/// A framework-neutral observer of one protected run. Dropping it only removes
/// this observer; the run continues under its owner.
pub struct RunSubscription {
    receiver: mpsc::Receiver<RunUpdate>,
}

impl RunSubscription {
    /// Wait for the next update. A finished run closes the channel after its one
    /// terminal update, so a disconnected channel means the owner retired.
    pub fn recv_timeout(&self, timeout: Duration) -> Result<RunUpdate, RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }
}

#[derive(Debug)]
pub enum RunControlError {
    Unsupported,
    /// No live run with that ID in this process. An earlier stop may already have
    /// finished, so callers treat this the same as an already stopped run.
    UnknownRun,
}

impl fmt::Display for RunControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => f.write_str("This build cannot control a protected run."),
            Self::UnknownRun => f.write_str("No live protected run has that run ID."),
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

/// Observe one protected run's phase changes and terminal outcome.
///
/// A live run starts with its current state. A run that has already finished
/// yields its retained outcome once, and an ID this process never admitted is
/// rejected. The subscription is one-way: it cannot start, stop or reconfigure
/// a run.
pub fn subscribe(run: Uuid) -> Result<RunSubscription, RunControlError> {
    #[cfg(all(windows, target_arch = "x86_64"))]
    return windows::subscribe(run);
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        let _ = run;
        Err(RunControlError::Unsupported)
    }
}

/// The redacted terminal record for a run that finished in this process and is
/// still inside the bounded retention window, or `None` otherwise.
pub fn outcome(run: Uuid) -> Option<RunOutcome> {
    #[cfg(all(windows, target_arch = "x86_64"))]
    return windows::outcome(run);
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        let _ = run;
        None
    }
}

/// Request one run's own process tree to stop. The run UUID must already be
/// visible through [`active_runs`], so a rejected or unknown ID changes nothing.
///
/// Every visible window the tree owns is asked to close first, which lets a game
/// save and exit normally. A tree that has not drained after the documented
/// close bound is terminated. Repeating the request while the run drains reports
/// the same state and starts no new bound.
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
