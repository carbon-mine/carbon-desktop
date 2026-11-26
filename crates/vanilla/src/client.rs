use super::{cache, release_type, selected, PreparedVersion};
use serde::Serialize;
use sha1::{Digest, Sha1};
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Seek, Write},
    path::Path,
};
use uuid::Uuid;

const MAX_CLIENT_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ClientState {
    Missing,
    Verified,
    RepairNeeded,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientStatus {
    pub profile_id: Uuid,
    pub version: String,
    pub status: ClientState,
    pub expected_bytes: u64,
    pub expected_sha1: String,
}

/// Open only the file selected by the native picker. The webview supplies no pathname.
pub fn open_client_file(path: &Path) -> Result<File, String> {
    if !path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("jar"))
    {
        return Err("Choose the exact .jar file for the selected Minecraft version.".into());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::{
            os::windows::fs::OpenOptionsExt,
            path::{Component, Prefix},
        };
        if !matches!(path.components().next(),Some(Component::Prefix(p)) if matches!(p.kind(),Prefix::Disk(_)|Prefix::VerbatimDisk(_)))
            || !path.is_absolute()
        {
            return Err("Choose a client JAR on a mounted local drive.".into());
        }
        options.custom_flags(0x00200000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let file = options
        .open(path)
        .map_err(|_| "Could not open the selected client JAR. Choose a readable regular file.")?;
    source_metadata(&file)?;
    Ok(file)
}

fn source_metadata(file: &File) -> Result<u64, String> {
    let meta = file
        .metadata()
        .map_err(|_| "Could not inspect the selected client JAR.")?;
    if !meta.is_file() || !cache::ordinary(&meta) || meta.len() > MAX_CLIENT_BYTES {
        return Err("Choose a regular client JAR no larger than 256 MiB. Links and special files are refused.".into());
    }
    Ok(meta.len())
}

fn cached_profile(
    root: &Path,
    profile_id: Uuid,
) -> Result<(cache::Cache, PreparedVersion), String> {
    let version = selected(root, profile_id)?;
    let cache = cache::Cache::open(root)?;
    let metadata = cache.read(version.id(),release_type(version),profile_id)?
        .ok_or("Prepare verified version metadata for this profile before checking or importing its client JAR.")?;
    if metadata.client_bytes > MAX_CLIENT_BYTES {
        return Err(
            "Mojang's declared client size exceeds the supported 256 MiB import limit.".into(),
        );
    }
    Ok((cache, metadata))
}

// Reads at most one byte beyond the pinned length, including if a selected file changes mid-read.
fn copy_and_verify(
    file: &mut File,
    writer: &mut impl Write,
    info: &PreparedVersion,
) -> Result<bool, String> {
    let mut buf = [0u8; 64 * 1024];
    let mut source = file.take(info.client_bytes + 1);
    let mut digest = Sha1::new();
    let mut size = 0;
    loop {
        let count = source
            .read(&mut buf)
            .map_err(|_| "Could not finish reading the client JAR. Check file access and retry.")?;
        if count == 0 {
            break;
        }
        size += count as u64;
        if size > info.client_bytes {
            return Ok(false);
        }
        writer
            .write_all(&buf[..count])
            .map_err(|_| "Could not stage the client JAR. Check free space and retry.")?;
        digest.update(&buf[..count]);
    }
    Ok(size == info.client_bytes && hex::encode(digest.finalize()) == info.client_sha1)
}

fn state(path: &Path, info: &PreparedVersion) -> Result<ClientState, String> {
    if !cache::regular(path)? {
        return Ok(ClientState::Missing);
    }
    let mut file = File::open(path)
        .map_err(|_| "Could not read the cached client JAR. Check its permissions.")?;
    if file
        .metadata()
        .map_err(|_| "Could not inspect the cached client JAR.")?
        .len()
        == info.client_bytes
        && copy_and_verify(&mut file, &mut io::sink(), info)?
    {
        Ok(ClientState::Verified)
    } else {
        Ok(ClientState::RepairNeeded)
    }
}

fn result(root: &Path, info: PreparedVersion, status: ClientState) -> Result<ClientStatus, String> {
    if selected(root, info.profile_id)?.id() != info.version {
        return Err("The profile changed version. Check its current client JAR again.".into());
    }
    Ok(ClientStatus {
        profile_id: info.profile_id,
        version: info.version,
        status,
        expected_bytes: info.client_bytes,
        expected_sha1: info.client_sha1,
    })
}

/// Hash a present object against the saved profile's verified provider metadata.
pub fn client_status(root: &Path, profile_id: Uuid) -> Result<ClientStatus, String> {
    let (cache, info) = cached_profile(root, profile_id)?;
    let dest = cache.client_dir()?.join(&info.client_sha1);
    let status = state(&dest, &info)?;
    result(root, info, status)
}

/// Import a user-selected regular file under the existing cache writer lock. No network or execution.
pub fn import_client(
    root: &Path,
    profile_id: Uuid,
    mut file: File,
) -> Result<ClientStatus, String> {
    let (cache, info) = cached_profile(root, profile_id)?;
    if source_metadata(&file)? != info.client_bytes {
        return Err("The selected client JAR has the wrong size for this version. Choose the matching vanilla client JAR.".into());
    }
    file.rewind()
        .map_err(|_| "Could not read the selected client JAR from its beginning.")?;
    let dir = cache.client_dir()?;
    let dest = dir.join(&info.client_sha1);
    let existing = state(&dest, &info)?;
    if existing == ClientState::Verified {
        if !copy_and_verify(&mut file, &mut io::sink(), &info)? {
            return Err("The selected client JAR failed Mojang's SHA-1 check. The verified cached copy remains available.".into());
        }
    } else {
        let mut stage = tempfile::NamedTempFile::new_in(&dir)
            .map_err(|_| "Could not stage the client JAR. Check free space and retry.")?;
        if !copy_and_verify(&mut file, &mut stage, &info)? {
            return Err("The selected client JAR failed Mojang's SHA-1 check. Choose the exact vanilla client for this version.".into());
        }
        stage
            .as_file()
            .sync_all()
            .map_err(|_| "Could not sync the client JAR. Check free space and retry.")?;
        stage.persist(&dest).map_err(|_| {
            "Could not publish the verified client JAR. Check permissions and retry."
        })?;
        cache::sync_dir(&dir)?;
        if state(&dest, &info)? != ClientState::Verified {
            return Err(
                "The published client JAR failed verification. Import a matching copy again."
                    .into(),
            );
        }
    }
    result(root, info, ClientState::Verified)
}
