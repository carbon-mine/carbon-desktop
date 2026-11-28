//! Per-install public client configuration. This file contains no credentials.
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::Path,
};
use uuid::Uuid;

const FILE: &str = "account-client.json";
const LIMIT: u64 = 1024;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct CustomClient {
    client_id: Uuid,
}

/// Read the opt-in public client ID. A missing, malformed, oversized or nil
/// value is treated as no configuration.
pub(super) fn load(root: &Path) -> Option<Uuid> {
    let path = root.join(FILE);
    let info = fs::symlink_metadata(&path).ok()?;
    if !info.is_file() || info.len() > LIMIT {
        return None;
    }
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > LIMIT {
        return None;
    }
    serde_json::from_slice::<CustomClient>(&bytes)
        .ok()
        .map(|custom| custom.client_id)
        .filter(|client_id| !client_id.is_nil())
}

/// Publish only the public application ID. This atomically replaces the small
/// app-owned setting and never writes account credentials.
pub(super) fn store(root: &Path, client_id: Uuid) -> Result<(), ()> {
    if client_id.is_nil() {
        return Err(());
    }
    fs::create_dir_all(root).map_err(|_| ())?;
    let mut staged = tempfile::NamedTempFile::new_in(root).map_err(|_| ())?;
    serde_json::to_writer(&mut staged, &CustomClient { client_id }).map_err(|_| ())?;
    staged.flush().map_err(|_| ())?;
    staged.as_file().sync_all().map_err(|_| ())?;
    staged.persist(root.join(FILE)).map_err(|_| ())?;
    Ok(())
}

/// Remove the opt-in setting without touching saved accounts or vault entries.
pub(super) fn clear(root: &Path) -> Result<(), ()> {
    match fs::remove_file(root.join(FILE)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(()),
    }
}
