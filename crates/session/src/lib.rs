// SPDX-License-Identifier: MPL-2.0
//! A labelled window for the repository-owned isolation fixture. No Minecraft launches.
use serde::Serialize;
use std::{fmt, io, path::Path};
use uuid::Uuid;

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
    InvalidProfile,
    Data(io::Error),
    Isolation(io::Error),
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unsupported => "This build does not include the protected Java test window.",
            Self::RuntimeNotReady => "Prepare the matching managed Java before opening the protected test window.",
            Self::Busy => "This profile already has a protected test or Java preparation in progress.",
            Self::InvalidProfile => "The profile cannot store a disposable test save.",
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
