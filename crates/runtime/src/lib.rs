// SPDX-License-Identifier: MPL-2.0
//! On-demand preparation of pinned Eclipse Temurin runtimes. This crate never launches Java.
mod archive;
mod artifact;
mod storage;

#[cfg(test)]
mod tests;

use artifact::{artifact, Artifact};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fmt, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

const MAX_ARCHIVE: u64 = 256 * 1024 * 1024;
type Progress<'a> = dyn FnMut(u64, u64) -> bool + 'a;

#[derive(Debug, Serialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RuntimeStatus {
    Unsupported {
        major: u16,
    },
    Available {
        major: u16,
        release: &'static str,
        download_bytes: u64,
    },
    RepairNeeded {
        major: u16,
        release: &'static str,
        download_bytes: u64,
    },
    Ready {
        major: u16,
        release: &'static str,
        download_bytes: u64,
        #[serde(skip)]
        java_home: PathBuf,
    },
}

impl RuntimeStatus {
    /// The private verified runtime directory. Keep this path on the trusted side of IPC.
    pub fn java_home(&self) -> Option<&Path> {
        match self {
            Self::Ready { java_home, .. } => Some(java_home),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum RuntimeError {
    Busy,
    Cancelled,
    UnsafeStorage,
    Storage(io::Error),
    Download(reqwest::Error),
    SourceRejected,
    Integrity,
    Unavailable,
    Archive(&'static str),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Busy => "Another preparation is using this Java cache. Try again shortly.",
            Self::Cancelled => "Java preparation was cancelled. You can retry from the profile.",
            Self::UnsafeStorage => "Java storage is not a private, regular directory. Check its location and retry.",
            Self::Storage(_) => "Could not read or write the Java cache. Check free space and permissions, then retry.",
            Self::Download(_) => "Could not reach the approved Java download. Check your connection and retry.",
            Self::SourceRejected => "The Java download destination or response is unsupported. Retry when the approved source is available.",
            Self::Integrity => "The Java archive or cached files failed verification. Retry preparation.",
            Self::Unavailable => "Prepare an app-managed Java runtime for this version and platform.",
            Self::Archive(reason) => reason,
        })
    }
}

impl std::error::Error for RuntimeError {}

impl From<io::Error> for RuntimeError {
    fn from(error: io::Error) -> Self {
        Self::Storage(error)
    }
}

/// Holds a shared lease on an independently verified app-managed Java installation.
/// This value and its paths stay on the trusted launcher side.
pub struct RuntimeLease {
    _lease: fs::File,
    major: u16,
    spec: &'static Artifact,
    generation: PathBuf,
    executable: PathBuf,
}

impl RuntimeLease {
    pub fn major(&self) -> u16 {
        self.major
    }

    pub fn release(&self) -> &'static str {
        self.spec.release
    }

    pub fn archive_sha256(&self) -> &'static str {
        self.spec.sha256
    }

    pub fn java_executable(&self) -> &Path {
        &self.executable
    }

    pub fn belongs_to(&self, root: &Path) -> bool {
        self.generation.parent() == Some(root)
    }

    /// Read-only integrity check for a trusted admission boundary.
    pub fn verify_current(&self) -> Result<(), RuntimeError> {
        verify_cache(&self.generation, self.spec, &mut |_, _| true)
    }
}

/// Acquire existing verified Java files with a shared lease. No probing or downloading.
pub fn lease_verified(root: &Path, major: u16) -> Result<RuntimeLease, RuntimeError> {
    let spec = artifact(major).ok_or(RuntimeError::Unavailable)?;
    lease_pinned(root, major, spec)
}

fn lease_pinned(
    root: &Path,
    major: u16,
    spec: &'static Artifact,
) -> Result<RuntimeLease, RuntimeError> {
    let generation = root.join(cache_name(major, spec));
    if !storage::check_root(root)? || !exists(&generation)? {
        return Err(RuntimeError::Unavailable);
    }
    let lease = RuntimeLease {
        _lease: storage::lock(root, major, false)?,
        major,
        spec,
        executable: generation.join("home").join(spec.executable()),
        generation,
    };
    lease.verify_current()?;
    Ok(lease)
}

/// Inspect the pinned cache. This call is read-only and never accesses the network.
pub fn inspect(root: &Path, java_major: u16) -> Result<RuntimeStatus, RuntimeError> {
    let Some(spec) = artifact(java_major) else {
        return Ok(RuntimeStatus::Unsupported { major: java_major });
    };
    inspect_supported(root, java_major, spec)
}

/// Fast presence check for the library UI. Only prepare/inspect verifies the complete JDK.
pub fn installed(root: &Path, major: u16) -> Result<RuntimeStatus, RuntimeError> {
    let Some(spec) = artifact(major) else {
        return Ok(RuntimeStatus::Unsupported { major });
    };
    let generation = root.join(cache_name(major, spec));
    if !storage::check_root(root)? || !exists(&generation)? {
        return Ok(RuntimeStatus::Available {
            major,
            release: spec.release,
            download_bytes: spec.archive_bytes,
        });
    }
    let _guard = storage::lock(root, major, false)?;
    let home = generation.join("home");
    if storage::check_root(&home)?
        && storage::open_regular(&generation.join("source.archive"), false)
            .and_then(|f| f.metadata().map_err(RuntimeError::from))
            .is_ok_and(|m| m.len() == spec.archive_bytes)
        && ["release", spec.executable(), spec.compiler()]
            .iter()
            .all(|file| storage::open_regular(&home.join(file), false).is_ok())
    {
        Ok(ready(major, spec, &generation))
    } else {
        Ok(RuntimeStatus::RepairNeeded {
            major,
            release: spec.release,
            download_bytes: spec.archive_bytes,
        })
    }
}

fn inspect_supported(
    root: &Path,
    major: u16,
    spec: &Artifact,
) -> Result<RuntimeStatus, RuntimeError> {
    let generation = root.join(cache_name(major, spec));
    if !storage::check_root(root)? || !exists(&generation)? {
        return Ok(RuntimeStatus::Available {
            major,
            release: spec.release,
            download_bytes: spec.archive_bytes,
        });
    }
    let _guard = storage::lock(root, major, false)?;
    match verify_cache(&generation, spec, &mut |_, _| true) {
        Ok(()) => Ok(ready(major, spec, &generation)),
        Err(_) => Ok(RuntimeStatus::RepairNeeded {
            major,
            release: spec.release,
            download_bytes: spec.archive_bytes,
        }),
    }
}

/// Prepare the pinned native runtime without launching it.
pub fn prepare(root: &Path, java_major: u16) -> Result<RuntimeStatus, RuntimeError> {
    prepare_with(root, java_major, |_, _| true)
}

/// Report downloaded and total bytes. Returning false cancels before publication.
/// After transfer keeps reporting the total while verifying and staging files.
pub fn prepare_with(
    root: &Path,
    java_major: u16,
    mut on_progress: impl FnMut(u64, u64) -> bool,
) -> Result<RuntimeStatus, RuntimeError> {
    let Some(spec) = artifact(java_major) else {
        return Ok(RuntimeStatus::Unsupported { major: java_major });
    };
    prepare_supported(root, java_major, spec, &mut on_progress, download)
}

fn prepare_supported(
    root: &Path,
    major: u16,
    spec: &Artifact,
    progress: &mut Progress<'_>,
    fetch: impl FnOnce(&Artifact, &mut Progress<'_>) -> Result<Vec<u8>, RuntimeError>,
) -> Result<RuntimeStatus, RuntimeError> {
    checkpoint(progress, 0, spec.archive_bytes)?;
    storage::ensure_root(root)?;
    let _guard = storage::lock(root, major, true)?;
    let stage_prefix = format!(".stage-java-{major}-");
    let generation = root.join(cache_name(major, spec));
    let old = exists(&generation)?;
    if old {
        if !storage::check_root(&generation)? {
            return Err(RuntimeError::UnsafeStorage);
        }
        match verify_cache(&generation, spec, progress) {
            Ok(()) => {
                checkpoint(progress, spec.archive_bytes, spec.archive_bytes)?;
                return Ok(ready(major, spec, &generation));
            }
            Err(RuntimeError::Cancelled) => return Err(RuntimeError::Cancelled),
            Err(_) => checkpoint(progress, 0, spec.archive_bytes)?,
        }
    }
    storage::remove_stale_stages(root, &stage_prefix)?;

    let stage = tempfile::Builder::new()
        .prefix(&stage_prefix)
        .rand_bytes(12)
        .tempdir_in(root)?;
    let staged_generation = stage.path().join("candidate");
    fs::create_dir(&staged_generation)?;
    let home = staged_generation.join("home");
    fs::create_dir(&home)?;

    let bytes = fetch(spec, progress)?;
    verify_pin(spec, &bytes, progress)?;
    let archive_path = staged_generation.join("source.archive");
    let mut saved = fs::File::create_new(archive_path)?;
    saved.write_all(&bytes)?;
    saved.sync_all()?;
    drop(saved);
    let mut through_stage = || checkpoint(progress, spec.archive_bytes, spec.archive_bytes);
    archive::check_or_extract(spec, &bytes, &home, true, &mut through_stage)?;
    storage::sync_dir(&staged_generation)?;
    storage::sync_dir(stage.path())?;
    through_stage()?;

    let retired = stage.path().join("retired");
    if old {
        fs::rename(&generation, &retired)?;
    }
    if let Err(error) = fs::rename(&staged_generation, &generation) {
        if old && fs::rename(&retired, &generation).is_err() {
            // Preserve the unverified old copy for diagnosis if filesystem recovery failed.
            let _ = stage.keep();
        }
        return Err(error.into());
    }
    storage::sync_dir(root)?;
    Ok(ready(major, spec, &generation))
}

fn verify_cache(
    generation: &Path,
    spec: &Artifact,
    progress: &mut Progress<'_>,
) -> Result<(), RuntimeError> {
    if !storage::check_root(generation)? {
        return Err(RuntimeError::Integrity);
    }
    let archived = storage::open_regular(&generation.join("source.archive"), false)?;
    if archived.metadata()?.len() != spec.archive_bytes {
        return Err(RuntimeError::Integrity);
    }
    let bytes = read_archive(archived, spec.archive_bytes, &mut |_, total| {
        progress(total, total)
    })?;
    verify_pin(spec, &bytes, progress)?;
    archive::check_or_extract(spec, &bytes, &generation.join("home"), false, &mut || {
        checkpoint(progress, spec.archive_bytes, spec.archive_bytes)
    })
}

fn verify_pin(
    spec: &Artifact,
    bytes: &[u8],
    progress: &mut Progress<'_>,
) -> Result<(), RuntimeError> {
    if bytes.len() as u64 != spec.archive_bytes {
        return Err(RuntimeError::Integrity);
    }
    let mut digest = Sha256::new();
    for chunk in bytes.chunks(512 * 1024) {
        checkpoint(progress, spec.archive_bytes, spec.archive_bytes)?;
        digest.update(chunk);
    }
    if hex::encode(digest.finalize()) != spec.sha256 {
        return Err(RuntimeError::Integrity);
    }
    Ok(())
}

fn download(spec: &Artifact, progress: &mut Progress<'_>) -> Result<Vec<u8>, RuntimeError> {
    let policy = reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() <= 4 && approved_url(attempt.url()) {
            attempt.follow()
        } else {
            attempt.stop()
        }
    });
    let client = reqwest::blocking::Client::builder()
        .https_only(true)
        .redirect(policy)
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .user_agent("Carbon-Desktop-runtime-preparation")
        .no_proxy()
        .build()
        .map_err(download_error)?;
    let response = client.get(spec.url).send().map_err(download_error)?;
    if !approved_url(response.url())
        || !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|len| len != spec.archive_bytes)
    {
        return Err(RuntimeError::SourceRejected);
    }
    read_archive(response, spec.archive_bytes, progress).map_err(|e| match e {
        RuntimeError::Storage(_) => RuntimeError::SourceRejected,
        e => e,
    })
}

fn approved_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && matches!(
            url.host_str(),
            Some(
                "github.com"
                    | "release-assets.githubusercontent.com"
                    | "objects.githubusercontent.com"
            )
        )
}

fn download_error(error: reqwest::Error) -> RuntimeError {
    RuntimeError::Download(error.without_url())
}

fn read_archive(
    mut reader: impl Read,
    size: u64,
    progress: &mut Progress<'_>,
) -> Result<Vec<u8>, RuntimeError> {
    if size == 0 || size > MAX_ARCHIVE {
        return Err(RuntimeError::Integrity);
    }
    let mut data = Vec::new();
    data.try_reserve_exact(size as usize)
        .map_err(|_| io::Error::other("archive allocation"))?;
    let mut buf = [0; 64 * 1024];
    loop {
        checkpoint(progress, data.len() as u64, size)?;
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        if data.len() as u64 + n as u64 > size {
            return Err(RuntimeError::Integrity);
        }
        data.extend_from_slice(&buf[..n]);
    }
    if data.len() as u64 != size {
        return Err(RuntimeError::Integrity);
    }
    Ok(data)
}

fn checkpoint(progress: &mut Progress<'_>, done: u64, total: u64) -> Result<(), RuntimeError> {
    if progress(done, total) {
        Ok(())
    } else {
        Err(RuntimeError::Cancelled)
    }
}

fn cache_name(major: u16, spec: &Artifact) -> String {
    format!("java-{major}-{}", spec.sha256)
}

fn ready(major: u16, spec: &Artifact, generation: &Path) -> RuntimeStatus {
    RuntimeStatus::Ready {
        major,
        release: spec.release,
        download_bytes: spec.archive_bytes,
        java_home: generation.join("home"),
    }
}

fn exists(path: &Path) -> Result<bool, RuntimeError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
