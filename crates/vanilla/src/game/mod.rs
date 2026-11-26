pub mod launch;
mod plan;
#[cfg(test)]
mod tests;
mod transfer;
mod view;
use super::{cache, prepare, release_type, selected, PreparedVersion};
#[cfg(test)]
use plan::INDEX_LIMIT;
use plan::{Object, Plan, Platform};
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::{
    fs::{File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
};
use uuid::Uuid;
pub use view::{
    has_instance_view, lease_installed_view, prepare_instance_view, InstancePaths,
    InstanceViewSummary, PreparedInstanceView,
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameFilePlan {
    pub profile_id: Uuid,
    pub version: String,
    pub platform: String,
    pub metadata_sha1: String,
    pub library_files: usize,
    pub asset_objects: usize,
    pub asset_names: usize,
    pub logging_files: usize,
    pub unique_files: usize,
    pub total_bytes: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameFileProgress {
    pub completed_files: usize,
    pub total_files: usize,
    pub verified_bytes: u64,
    pub total_bytes: u64,
    pub phase: &'static str,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameFilesPrepared {
    pub profile_id: Uuid,
    pub version: String,
    pub platform: String,
    pub metadata_sha1: String,
    pub unique_files: usize,
    pub total_bytes: u64,
    pub fetched_bytes: u64,
    pub reused_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_view: Option<InstanceViewSummary>,
}

struct Store {
    dir: PathBuf,
    clients: PathBuf,
}
impl Store {
    fn open(root: &Path, clients: PathBuf) -> Result<Self, String> {
        let dir = root.join("game-files-v1");
        cache::directory(&dir)?;
        for name in ["objects", "generations", "active"] {
            cache::directory(&dir.join(name))?;
        }
        Ok(Self { dir, clients })
    }
    fn object(&self, object: &Object, client: &str) -> Result<PathBuf, String> {
        if object.sha1 == client {
            return Ok(self.clients.join(&object.sha1));
        }
        let shard = self.dir.join("objects").join(&object.sha1[..2]);
        cache::directory(&shard)?;
        Ok(shard.join(&object.sha1))
    }
    fn lock(&self) -> Result<File, String> {
        let path = self.dir.join("prepare.lock");
        cache::regular(&path)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .truncate(false)
            .create(true)
            .open(path)
            .map_err(|_| "Could not open game preparation lock.")?;
        lock.try_lock()
            .map_err(|_| "Another game file operation is running. Check Activity and retry.")?;
        Ok(lock)
    }
    fn generation(&self, platform: &str, pin: &str) -> PathBuf {
        self.dir
            .join("generations")
            .join(format!("{platform}-{pin}.json"))
    }
    fn receipt(&self, id: Uuid) -> PathBuf {
        self.dir.join("active").join(format!("{id}.json"))
    }

    fn has_receipt(&self, expected: &Receipt) -> Result<bool, String> {
        let Some(raw) = cache::read(&self.receipt(expected.profile_id), 4096)? else {
            return Ok(false);
        };
        if serde_json::from_slice::<Receipt>(&raw).ok().as_ref() != Some(expected) {
            return Ok(false);
        }
        cache::regular(&self.generation(&expected.platform, &expected.metadata_sha1))
    }
}

#[derive(Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Receipt {
    format: u32,
    profile_id: Uuid,
    version: String,
    metadata_sha1: String,
    platform: String,
}
impl Receipt {
    fn new(id: Uuid, version: &str, pin: &str, platform: &str) -> Self {
        Self {
            format: 1,
            profile_id: id,
            version: version.to_owned(),
            metadata_sha1: pin.to_owned(),
            platform: platform.to_owned(),
        }
    }
}

/// Observe only the matching activation record. An explicit offline preparation rehashes content.
pub fn has_game_install(root: &Path, profile_id: Uuid) -> Result<bool, String> {
    let Some((store, expected)) = current_receipt(root, profile_id)? else {
        return Ok(false);
    };
    store.has_receipt(&expected)
}

fn current_receipt(root: &Path, profile_id: Uuid) -> Result<Option<(Store, Receipt)>, String> {
    let version = selected(root, profile_id)?;
    let cache = cache::Cache::open(root)?;
    let Some(pin) = cache.pin(version.id(), release_type(version))? else {
        return Ok(None);
    };
    let store = Store::open(root, cache.client_dir()?)?;
    let expected = Receipt::new(
        profile_id,
        version.id(),
        &pin.sha1,
        &Platform::current()?.label(),
    );
    Ok(Some((store, expected)))
}

fn resolve(
    root: &Path,
    id: Uuid,
    offline: bool,
    platform: Platform,
) -> Result<(Store, Plan), String> {
    let info = prepare(root, id, offline)?;
    let cache = cache::Cache::open(root)?;
    let raw = cache.raw_version(&info)?;
    let store = Store::open(root, cache.client_dir()?)?;
    drop(cache);
    let mut plan = Plan::version(&info, &raw, platform)?;
    let path = store.object(&plan.index, &plan.client_sha1)?;
    let cached = cache::read(&path, plan.index.size)?.filter(|bytes| {
        bytes.len() as u64 == plan.index.size && hex::encode(Sha1::digest(bytes)) == plan.index.sha1
    });
    if let Some(bytes) = cached {
        plan.add_assets(&bytes)?;
    } else {
        if offline {
            return Err("A verified asset index is missing or damaged. Review the game file plan online and retry.".into());
        }
        let uri = url::Url::parse(&plan.index.url).map_err(|_| "Invalid asset index URL.")?;
        let bytes = crate::provider::fetch(&plan.index.url, uri.path(), plan.index.size)?;
        if bytes.len() as u64 != plan.index.size
            || hex::encode(Sha1::digest(&bytes)) != plan.index.sha1
        {
            return Err(
                "Mojang's asset index failed its expected size or SHA-1. Retry later.".into(),
            );
        }
        plan.add_assets(&bytes)?;
        cache::publish(&path, &bytes)?;
    }
    ensure_selection(root, &plan.summary)?;
    Ok((store, plan))
}

fn ensure_selection(root: &Path, plan: &GameFilePlan) -> Result<(), String> {
    let version = selected(root, plan.profile_id)?;
    if version.id() == plan.version
        && cache::Cache::open(root)?
            .pin(version.id(), release_type(version))?
            .is_some_and(|p| p.sha1 == plan.metadata_sha1)
    {
        Ok(())
    } else {
        Err(
            "This profile or its version metadata changed. Review its current game file plan."
                .into(),
        )
    }
}

pub fn game_file_plan(
    root: &Path,
    profile_id: Uuid,
    offline: bool,
) -> Result<GameFilePlan, String> {
    Ok(resolve(root, profile_id, offline, Platform::current()?)?
        .1
        .summary)
}

pub fn prepare_game_files(
    root: &Path,
    profile_id: Uuid,
    offline: bool,
    on_progress: impl FnMut(GameFileProgress) -> bool + Send,
) -> Result<GameFilesPrepared, String> {
    let (store, plan) = resolve(root, profile_id, offline, Platform::current()?)?;
    run_plan(root, &store, &plan, offline, on_progress)
}

/// Acquire only the provider version reviewed by the user. A stale pin cannot begin a game payload transfer.
pub fn prepare_reviewed_game_files(
    root: &Path,
    profile_id: Uuid,
    offline: bool,
    metadata_sha1: &str,
    on_progress: impl FnMut(GameFileProgress) -> bool + Send,
) -> Result<GameFilesPrepared, String> {
    let (store, plan) = resolve_reviewed(root, profile_id, metadata_sha1)?;
    run_plan(root, &store, &plan, offline, on_progress)
}

/// Verify or acquire the reviewed cache, then activate the private profile content.
/// Only metadata and counts leave this API. Use prepare_instance_view for leased paths.
pub fn prepare_reviewed_instance(
    root: &Path,
    profile_id: Uuid,
    offline: bool,
    metadata_sha1: &str,
    mut on_progress: impl FnMut(GameFileProgress) -> bool + Send,
) -> Result<GameFilesPrepared, String> {
    let (store, plan) = resolve_reviewed(root, profile_id, metadata_sha1)?;
    view::supports_layout(&plan)?;
    let mut files = run_plan(root, &store, &plan, offline, &mut on_progress)?;
    let view = view::materialize(root, &store, &plan, metadata_sha1, on_progress)?;
    files.instance_view = Some(view.summary().clone());
    Ok(files)
}

fn resolve_reviewed(
    root: &Path,
    profile_id: Uuid,
    metadata_sha1: &str,
) -> Result<(Store, Plan), String> {
    if !crate::valid_digest(metadata_sha1) {
        return Err("Invalid reviewed version pin. Review the game file plan again.".into());
    }
    // A reviewed plan already has both verified JSON documents locally. A missing or damaged
    // document must be resolved in a new plan review; the install cannot silently refresh it.
    let (store, plan) = resolve(root, profile_id, true, Platform::current()?)?;
    if plan.summary.metadata_sha1 != metadata_sha1 {
        return Err(
            "The reviewed Minecraft version changed. Review the game file plan again.".into(),
        );
    }
    Ok((store, plan))
}

fn run_plan(
    root: &Path,
    store: &Store,
    plan: &Plan,
    offline: bool,
    on_progress: impl FnMut(GameFileProgress) -> bool + Send,
) -> Result<GameFilesPrepared, String> {
    // No HTTP client or request is needed for an offline verification.
    let http = if offline {
        None
    } else {
        Some(transfer::client()?)
    };
    prepare_with(root, store, plan, offline, on_progress, |o| {
        let client = http
            .as_ref()
            .ok_or("Offline game file preparation cannot access the network.")?;
        transfer::fetch(client, o)
    })
}

fn prepare_with(
    root: &Path,
    store: &Store,
    plan: &Plan,
    offline: bool,
    on_progress: impl FnMut(GameFileProgress) -> bool + Send,
    fetch: impl Fn(&Object) -> Result<Box<dyn Read + Send>, String> + Sync,
) -> Result<GameFilesPrepared, String> {
    let _guard = store.lock()?;
    let (fetched, reused) = transfer::objects(store, plan, offline, on_progress, fetch)?;
    ensure_selection(root, &plan.summary)?;
    let s = &plan.summary;
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Generation<'a> {
        format: u32,
        version: &'a str,
        metadata_sha1: &'a str,
        platform: &'a str,
        client_sha1: &'a str,
        classpath: &'a [String],
        native_archives: &'a [String],
        paths: &'a std::collections::BTreeMap<String, String>,
    }
    let manifest = serde_json::to_vec(&Generation {
        format: 1,
        version: &s.version,
        metadata_sha1: &s.metadata_sha1,
        platform: &s.platform,
        client_sha1: &plan.client_sha1,
        classpath: &plan.classpath,
        native_archives: &plan.native_archives,
        paths: &plan.paths,
    })
    .map_err(|_| "Could not encode the verified game file map.")?;
    cache::publish(&store.generation(&s.platform, &s.metadata_sha1), &manifest)?;
    let receipt = Receipt::new(s.profile_id, &s.version, &s.metadata_sha1, &s.platform);
    cache::publish(
        &store.receipt(s.profile_id),
        &serde_json::to_vec(&receipt).map_err(|_| "Could not encode game file activation.")?,
    )?;
    Ok(GameFilesPrepared {
        profile_id: s.profile_id,
        version: s.version.clone(),
        platform: s.platform.clone(),
        metadata_sha1: s.metadata_sha1.clone(),
        unique_files: s.unique_files,
        total_bytes: s.total_bytes,
        fetched_bytes: fetched,
        reused_bytes: reused,
        instance_view: None,
    })
}
