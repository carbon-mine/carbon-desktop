//! Fetch and verify public Minecraft vanilla version metadata for a saved profile.
mod cache;
mod client;
mod game;
mod provider;
pub use client::{client_status, import_client, open_client_file, ClientState, ClientStatus};
pub use game::launch;
pub use game::{
    game_file_plan, has_game_install, has_instance_view, prepare_game_files, prepare_instance_view,
    prepare_reviewed_game_files, prepare_reviewed_instance, GameFilePlan, GameFileProgress,
    GameFilesPrepared, InstancePaths, InstanceViewSummary, PreparedInstanceView,
};
#[cfg(test)]
mod tests;

use carbon_profiles::VanillaVersion;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::path::Path;
use uuid::Uuid;

const LIST_PATH: &str = "/mc/game/version_manifest_v2.json";
const LIST_HOST: &str = "piston-meta.mojang.com";
const LIST_LIMIT: u64 = 2 * 1024 * 1024;
const VERSION_LIMIT: u64 = 1024 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedVersion {
    pub profile_id: Uuid,
    pub version: String,
    pub release_type: String,
    pub java_major: u32,
    pub client_bytes: u64,
    #[serde(skip)]
    client_sha1: String,
    pub asset_index_bytes: u64,
    pub library_count: usize,
    pub metadata_sha1: String,
    pub origin: Origin,
}

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Origin {
    Network,
    Cache,
}

#[derive(Debug, Deserialize, Serialize)]
struct Pin {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    url: String,
    sha1: String,
}

fn valid_digest(s: &str) -> bool {
    s.len() == 40
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

impl Pin {
    fn path(&self) -> String {
        format!("/v1/packages/{}/{}.json", self.sha1, self.id)
    }
    fn valid_for(&self, id: &str, kind: &str) -> bool {
        self.id == id
            && self.kind == kind
            && valid_digest(&self.sha1)
            && provider::allowed(&self.url, &self.path())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    java_version: Java,
    downloads: Downloads,
    asset_index: Descriptor,
    libraries: Vec<serde::de::IgnoredAny>,
}
#[derive(Deserialize)]
struct Java {
    #[serde(rename = "majorVersion")]
    major: u32,
}
#[derive(Deserialize)]
struct Descriptor {
    size: u64,
    sha1: String,
}
#[derive(Deserialize)]
struct Downloads {
    client: Descriptor,
}

fn verified(
    raw: &[u8],
    pin: &Pin,
    profile_id: Uuid,
    origin: Origin,
) -> Result<PreparedVersion, String> {
    if raw.len() as u64 > VERSION_LIMIT || hex::encode(Sha1::digest(raw)) != pin.sha1 {
        return Err(
            "The version metadata failed Mojang's SHA-1 check. Retry preparation online.".into(),
        );
    }
    let metadata: Metadata = serde_json::from_slice(raw).map_err(|_| {
        "The verified version JSON is unsupported. Retry when updated metadata is available."
    })?;
    if metadata.id != pin.id
        || metadata.kind != pin.kind
        || metadata.java_version.major == 0
        || metadata.downloads.client.size == 0
        || metadata.asset_index.size == 0
        || !valid_digest(&metadata.downloads.client.sha1)
        || !valid_digest(&metadata.asset_index.sha1)
    {
        return Err(
            "The verified version metadata contains inconsistent version or download information."
                .into(),
        );
    }
    Ok(PreparedVersion {
        profile_id,
        version: metadata.id,
        release_type: metadata.kind,
        java_major: metadata.java_version.major,
        client_bytes: metadata.downloads.client.size,
        client_sha1: metadata.downloads.client.sha1,
        asset_index_bytes: metadata.asset_index.size,
        library_count: metadata.libraries.len(),
        metadata_sha1: pin.sha1.clone(),
        origin,
    })
}

fn selected(root: &Path, id: Uuid) -> Result<VanillaVersion, String> {
    let profile = carbon_profiles::list(root)?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or("Select an existing personal instance first.")?;
    if profile
        .imported_pack
        .as_ref()
        .is_some_and(|pack| pack.loader.is_some() || pack.pending_files > 0)
    {
        return Err("This pack still needs its loader or listed client files. Vanilla preparation cannot complete this instance.".into());
    }
    Ok(profile.version)
}

fn release_type(version: VanillaVersion) -> &'static str {
    if version == VanillaVersion::Snapshot24w14a {
        "snapshot"
    } else {
        "release"
    }
}

/// Resolve the version solely from the saved native profile. Offline mode reads only local data.
pub fn prepare(root: &Path, profile_id: Uuid, offline: bool) -> Result<PreparedVersion, String> {
    prepare_with(root, profile_id, offline, provider::fetch)
}

fn prepare_with(
    root: &Path,
    profile_id: Uuid,
    offline: bool,
    mut fetch: impl FnMut(&str, &str, u64) -> Result<Vec<u8>, String>,
) -> Result<PreparedVersion, String> {
    let version = selected(root, profile_id)?;
    let id = version.id();
    let kind = release_type(version);
    let store = cache::Cache::open(root)?;
    let prepared = if let Some(cached) = store.read(id, kind, profile_id)? {
        cached
    } else {
        if offline {
            return Err("Verified version metadata is missing or damaged. Prepare this profile online once, then retry offline.".into());
        }
        #[derive(Deserialize)]
        struct Listing {
            versions: Vec<Pin>,
        }
        let listing_raw = fetch(
            &format!("https://{LIST_HOST}{LIST_PATH}"),
            LIST_PATH,
            LIST_LIMIT,
        )?;
        if listing_raw.len() as u64 > LIST_LIMIT {
            return Err("Mojang's version list exceeds the supported size.".into());
        }
        let listing: Listing = serde_json::from_slice(&listing_raw)
            .map_err(|_| "Could not read Mojang's version list. Retry preparation later.")?;
        let mut matches = listing.versions.into_iter().filter(|entry| entry.id == id);
        let pin = matches.next().ok_or("Mojang's version list does not contain the selected version. Choose another version or retry later.")?;
        if matches.next().is_some() || !pin.valid_for(id, kind) {
            return Err("Mojang's version entry has an unexpected identity, digest or metadata source. Preparation stopped.".into());
        }
        let raw = fetch(&pin.url, &pin.path(), VERSION_LIMIT)?;
        let prepared = verified(&raw, &pin, profile_id, Origin::Network)?;
        store.publish(&pin, &raw)?;
        prepared
    };
    if selected(root, profile_id)? != version {
        return Err(
            "This profile changed version during preparation. Prepare its current version.".into(),
        );
    }
    Ok(prepared)
}
