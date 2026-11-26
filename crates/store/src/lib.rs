//! A local, fixed-input fixture store for Carbon Desktop. No game or provider access.
mod disk;
mod fixtures;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use disk::{atomic_write, directory, existing_file, lockfile, read_json, write_json};
use fixtures::{Blob, ALL};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum InstanceId {
    Sample,
    Repeat,
    Overlap,
}

impl InstanceId {
    fn key(self) -> &'static str {
        match self {
            Self::Sample => "sample",
            Self::Repeat => "repeat",
            Self::Overlap => "overlap",
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Sample => "Forest sample",
            Self::Repeat => "Forest repeat",
            Self::Overlap => "Coast sample",
        }
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Generation {
    A,
    B,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Idle,
    Review,
    Interrupted,
    Staged,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Action {
    CreateSample,
    CreateRepeat,
    CreateOverlap,
    Review { instance: InstanceId },
    Stage { instance: InstanceId },
    Cancel { instance: InstanceId },
    Interrupt { instance: InstanceId },
    Resume { instance: InstanceId },
    Activate { instance: InstanceId },
    Rollback { instance: InstanceId },
    Repair { instance: InstanceId },
    SimulateMissing { instance: InstanceId },
    SimulateCorruption { instance: InstanceId },
    EditSetting { instance: InstanceId },
    EditSave { instance: InstanceId },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub instances: Vec<InstanceView>,
    pub metrics: Metrics,
    pub message: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceView {
    pub id: InstanceId,
    pub name: &'static str,
    pub generation: Generation,
    pub phase: Phase,
    pub healthy: bool,
    pub setting: u32,
    pub save_revision: u32,
    pub review: Option<Review>,
}
#[derive(Debug, Serialize)]
pub struct Review {
    pub added: Vec<&'static str>,
    pub removed: Vec<&'static str>,
    pub retained: Vec<&'static str>,
}
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metrics {
    pub source_bytes: u64,
    pub reused_bytes: u64,
    pub object_bytes: u64,
    pub object_count: u32,
    pub active_logical_bytes: u64,
    pub allocated_object_bytes: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Library {
    version: u8,
    instances: BTreeMap<InstanceId, Profile>,
    source_bytes: u64,
    reused_bytes: u64,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    generation: Generation,
    phase: Phase,
    prepared: usize,
}
impl Default for Profile {
    fn default() -> Self {
        Self {
            generation: Generation::A,
            phase: Phase::Idle,
            prepared: 0,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Save {
    revision: u32,
    requires_b: bool,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Backup {
    world: Save,
    setting: u32,
}

type Supplier<'a> = dyn Fn(&Blob) -> Result<Vec<u8>, String> + 'a;

struct Store<'a> {
    catalog: &'static [Blob; 6],
    supply: &'a Supplier<'a>,
    root: &'a Path,
    _exclusive: File,
    library: Library,
}

impl<'a> Store<'a> {
    fn open(root: &'a Path) -> Result<Self, String> {
        Self::open_fixture(root, &ALL, &|blob| Ok(blob.source()))
    }

    fn open_fixture(
        root: &'a Path,
        catalog: &'static [Blob; 6],
        supply: &'a Supplier<'a>,
    ) -> Result<Self, String> {
        // The shell owns the data-root selection. Every path below root is crate-owned.
        if let Some(parent) = root.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| disk::error("Could not create fixture parent", e))?;
        }
        directory(root)?;
        let lock = lockfile(&root.join("library.lock"))?;
        lock.lock()
            .map_err(|e| disk::error("Could not lock fixture library", e))?;
        for name in ["objects", "players", "backups"] {
            directory(&root.join(name))?;
        }
        let index = root.join("library.json");
        let library: Library = if existing_file(&index)?.is_some() {
            read_json(&index)?
        } else {
            for name in ["objects", "players", "backups"] {
                let mut entries = fs::read_dir(root.join(name))
                    .map_err(|e| disk::error("Could not inspect fixture library", e))?;
                if entries.next().is_some() {
                    return Err("The fixture library index is missing beside existing data. Restore the index before continuing.".into());
                }
            }
            let fresh = Library {
                version: 1,
                instances: BTreeMap::new(),
                source_bytes: 0,
                reused_bytes: 0,
            };
            write_json(&index, &fresh)?;
            fresh
        };
        if library.version != 1
            || (!library.instances.is_empty()
                && !library.instances.contains_key(&InstanceId::Sample))
            || library.instances.values().any(|p| match p.phase {
                Phase::Idle => p.prepared != 0,
                Phase::Review => p.prepared != 0 || p.generation != Generation::A,
                Phase::Interrupted => p.prepared > 4 || p.generation != Generation::A,
                Phase::Staged => p.prepared != 4 || p.generation != Generation::A,
            })
        {
            return Err("Unknown or inconsistent fixture library version or update state.".into());
        }
        Ok(Self {
            catalog,
            supply,
            root,
            _exclusive: lock,
            library,
        })
    }

    fn selection(&self, id: InstanceId, generation: Generation) -> [&'static Blob; 4] {
        let weather = if generation == Generation::A { 4 } else { 5 };
        let biome = if id == InstanceId::Overlap { 3 } else { 2 };
        // Prepare the changed input first so interrupted staging can resume after a real miss.
        [
            &self.catalog[weather],
            &self.catalog[0],
            &self.catalog[1],
            &self.catalog[biome],
        ]
    }

    fn commit(&self) -> Result<(), String> {
        write_json(&self.root.join("library.json"), &self.library)
    }

    fn profile(&self, id: InstanceId) -> Result<Profile, String> {
        self.library
            .instances
            .get(&id)
            .copied()
            .ok_or_else(|| "Create this fixture instance first.".into())
    }

    fn player(&self, id: InstanceId) -> Result<PathBuf, String> {
        let path = self.root.join("players").join(id.key());
        disk::check_directory(&path)?;
        Ok(path)
    }
    fn object(&self, blob: &Blob) -> PathBuf {
        self.root.join("objects").join(blob.digest)
    }

    fn valid(&self, blob: &Blob) -> Result<bool, String> {
        let path = self.object(blob);
        let Some(meta) = existing_file(&path)? else {
            return Ok(false);
        };
        if meta.len() != blob.len() {
            return Ok(false);
        }
        let file = File::open(&path)
            .map_err(|e| disk::error("Could not read cached fixture object", e))?;
        let mut reader = file.take(blob.len() + 1);
        let mut hash = Sha256::new();
        let mut block = [0; 32 * 1024];
        loop {
            let used = reader
                .read(&mut block)
                .map_err(|e| disk::error("Could not verify cached fixture object", e))?;
            if used == 0 {
                break;
            }
            hash.update(&block[..used]);
        }
        Ok(hex::encode(hash.finalize()) == blob.digest)
    }

    fn require_no_readers_using(&self, blob: &Blob) -> Result<(), String> {
        // The library lock prevents new leases until the selected write finishes.
        for (id, p) in &self.library.instances {
            if self
                .selection(*id, p.generation)
                .iter()
                .any(|entry| entry.digest == blob.digest)
            {
                let lock = lockfile(&self.player(*id)?.join("content.lock"))?;
                lock.try_lock().map_err(|_| {
                    "A fixture instance is using this content. Close its reader and retry."
                })?;
            }
        }
        Ok(())
    }

    fn ensure(&mut self, blob: &Blob) -> Result<(), String> {
        if self.valid(blob)? {
            self.library.reused_bytes = self
                .library
                .reused_bytes
                .checked_add(blob.len())
                .ok_or("Fixture reuse counter is full.")?;
        } else {
            self.require_no_readers_using(blob)?;
            let bytes = (self.supply)(blob)?;
            if bytes.len() as u64 != blob.len()
                || hex::encode(Sha256::digest(&bytes)) != blob.digest
            {
                return Err(format!(
                    "Built-in fixture {} failed verification.",
                    blob.name
                ));
            }
            atomic_write(&self.object(blob), &bytes)?;
            self.library.source_bytes = self
                .library
                .source_bytes
                .checked_add(blob.len())
                .ok_or("Fixture source counter is full.")?;
        }
        Ok(())
    }

    fn healthy(&self, id: InstanceId, generation: Generation) -> Result<bool, String> {
        for blob in self.selection(id, generation) {
            if !self.valid(blob)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn create(&mut self, id: InstanceId) -> Result<&'static str, String> {
        if self.library.instances.contains_key(&id) {
            return Ok("This fixture already exists.");
        }
        if id != InstanceId::Sample {
            self.profile(InstanceId::Sample)?;
        }
        let player = self.root.join("players").join(id.key());
        directory(&player)?;
        for blob in self.selection(id, Generation::A) {
            self.ensure(blob)?;
        }
        let settings = player.join("settings.json");
        if existing_file(&settings)?.is_none() {
            write_json(&settings, &0_u32)?;
        }
        let _: u32 = read_json(&settings)?;
        let world = player.join("sample-save.json");
        if existing_file(&world)?.is_none() {
            write_json(&world, &Save::default())?;
        }
        let _: Save = read_json(&world)?;
        self.library.instances.insert(id, Profile::default());
        self.commit()?;
        Ok("Created an independent local fixture instance.")
    }

    fn update(&mut self, id: InstanceId, p: Profile) -> Result<(), String> {
        self.library.instances.insert(id, p);
        self.commit()
    }

    fn stage(
        &mut self,
        id: InstanceId,
        mut p: Profile,
        interrupt: bool,
    ) -> Result<&'static str, String> {
        if !self.healthy(id, Generation::A)? {
            return Err("Repair active generation A before staging its replacement.".into());
        }
        p.phase = Phase::Interrupted;
        self.update(id, p)?;
        let selected = self.selection(id, Generation::B);
        for (index, blob) in selected.iter().enumerate() {
            if index < p.prepared {
                if !self.valid(blob)? {
                    self.ensure(blob)?;
                    self.commit()?;
                }
                continue;
            }
            self.ensure(blob)?;
            p.prepared = index + 1;
            self.update(id, p)?;
            if interrupt {
                return Ok("Staging interrupted after a verified object. Resume or cancel.");
            }
        }
        if interrupt {
            return Ok("The staged update is paused. Resume or cancel.");
        }
        p.phase = Phase::Staged;
        self.update(id, p)?;
        Ok("Generation B is verified and staged. Generation A is still active.")
    }

    fn protect_backup(&self, id: InstanceId, player: &Path) -> Result<(), String> {
        let path = self
            .root
            .join("backups")
            .join(format!("{}-before-b.json", id.key()));
        if existing_file(&path)?.is_some() {
            let _: Backup = read_json(&path)?;
        }
        let backup = Backup {
            world: read_json(&player.join("sample-save.json"))?,
            setting: read_json(&player.join("settings.json"))?,
        };
        write_json(&path, &backup)
    }

    fn change(&mut self, action: Action) -> Result<&'static str, String> {
        use Action::*;
        let id = match action {
            CreateSample => return self.create(InstanceId::Sample),
            CreateRepeat => return self.create(InstanceId::Repeat),
            CreateOverlap => return self.create(InstanceId::Overlap),
            Review { instance }
            | Stage { instance }
            | Cancel { instance }
            | Interrupt { instance }
            | Resume { instance }
            | Activate { instance }
            | Rollback { instance }
            | Repair { instance }
            | SimulateMissing { instance }
            | SimulateCorruption { instance }
            | EditSetting { instance }
            | EditSave { instance } => instance,
        };
        let mut p = self.profile(id)?;
        let player = self.player(id)?;
        match action {
            Review { .. } if p.generation == Generation::A && p.phase == Phase::Idle => {
                p.phase = Phase::Review;
                self.update(id, p)?;
                Ok("Review the local fixture changes before staging.")
            }
            Stage { .. } if p.phase == Phase::Review => self.stage(id, p, false),
            Resume { .. } if p.phase == Phase::Interrupted => self.stage(id, p, false),
            Interrupt { .. } if matches!(p.phase, Phase::Review | Phase::Staged) => {
                self.stage(id, p, true)
            }
            Cancel { .. } if p.phase != Phase::Idle => {
                p.phase = Phase::Idle;
                p.prepared = 0;
                self.update(id, p)?;
                Ok("Update cancelled. Verified fixture objects remain available for reuse.")
            }
            Activate { .. } | Rollback { .. } => {
                let next = match action {
                    Activate { .. } if p.phase == Phase::Staged => Generation::B,
                    Rollback { .. } if p.phase == Phase::Idle && p.generation == Generation::B => {
                        Generation::A
                    }
                    _ => {
                        return Err(
                            "Stage an update before activation, or activate B before rollback."
                                .into(),
                        )
                    }
                };
                let lease = lockfile(&player.join("content.lock"))?;
                lease.try_lock().map_err(|_| {
                    "Activation is waiting for this instance's content reader to exit."
                })?;
                if next == Generation::B && !self.healthy(id, Generation::A)? {
                    return Err("Repair generation A before activating B so a complete rollback remains available.".into());
                }
                if !self.healthy(id, next)? {
                    return Err(
                        "The target generation failed verification. Repair the instance and retry."
                            .into(),
                    );
                }
                let save: Save = read_json(&player.join("sample-save.json"))?;
                if next == Generation::A && save.requires_b {
                    return Err("This save was edited under generation B. Keep B active to preserve compatibility.".into());
                }
                if next == Generation::B {
                    self.protect_backup(id, &player)?;
                }
                self.update(
                    id,
                    Profile {
                        generation: next,
                        ..Profile::default()
                    },
                )?;
                Ok(if next == Generation::B {
                    "Generation B is active."
                } else {
                    "Generation A is active."
                })
            }
            Repair { .. } => {
                // A remains the rollback selection after B activates. Distinct objects are visited once.
                for blob in self.catalog {
                    let needed = self
                        .selection(id, p.generation)
                        .iter()
                        .any(|b| b.digest == blob.digest)
                        || (p.generation == Generation::B
                            && self
                                .selection(id, Generation::A)
                                .iter()
                                .any(|b| b.digest == blob.digest))
                        || (p.phase == Phase::Staged
                            && self
                                .selection(id, Generation::B)
                                .iter()
                                .any(|b| b.digest == blob.digest));
                    if needed && !self.valid(blob)? {
                        self.ensure(blob)?;
                    }
                }
                self.commit()?;
                Ok("Verified and repaired the selected fixture content.")
            }
            SimulateMissing { .. } | SimulateCorruption { .. } => {
                let blob = self.selection(id, p.generation)[0];
                self.require_no_readers_using(blob)?;
                let object = self.object(blob);
                if matches!(action, SimulateMissing { .. }) {
                    if existing_file(&object)?.is_some() {
                        fs::remove_file(&object)
                            .map_err(|e| disk::error("Could not remove fixture object", e))?;
                    }
                } else {
                    let mut damaged = blob.source();
                    damaged[0] ^= 1;
                    atomic_write(&object, &damaged)?;
                }
                Ok("A shared local fixture object needs repair. Player files are intact.")
            }
            EditSetting { .. } => {
                let path = player.join("settings.json");
                let value: u32 = read_json(&path)?;
                write_json(
                    &path,
                    &value
                        .checked_add(1)
                        .ok_or("Sample setting limit reached.")?,
                )?;
                Ok("Changed this instance's sample setting.")
            }
            EditSave { .. } => {
                let path = player.join("sample-save.json");
                let mut save: Save = read_json(&path)?;
                save.revision = save
                    .revision
                    .checked_add(1)
                    .ok_or("Sample save revision limit reached.")?;
                save.requires_b |= p.generation == Generation::B;
                write_json(&path, &save)?;
                Ok("Edited this instance's sample save.")
            }
            _ => Err(
                "This action is unavailable in the current fixture phase. Refresh the library."
                    .into(),
            ),
        }
    }

    fn snapshot(&self, message: &str) -> Result<Snapshot, String> {
        let mut instances = Vec::new();
        let mut metrics = Metrics {
            source_bytes: self.library.source_bytes,
            reused_bytes: self.library.reused_bytes,
            allocated_object_bytes: Some(0),
            ..Metrics::default()
        };
        for (id, p) in &self.library.instances {
            let player = self.player(*id)?;
            let save: Save = read_json(&player.join("sample-save.json"))?;
            let objects = self.selection(*id, p.generation);
            metrics.active_logical_bytes += objects.iter().map(|b| b.len()).sum::<u64>();
            let review = (p.phase != Phase::Idle).then(|| Review {
                added: vec![self.catalog[5].name],
                removed: vec![self.catalog[4].name],
                retained: objects[1..].iter().map(|b| b.name).collect(),
            });
            instances.push(InstanceView {
                id: *id,
                name: id.name(),
                generation: p.generation,
                phase: p.phase,
                healthy: self.healthy(*id, p.generation)?,
                setting: read_json(&player.join("settings.json"))?,
                save_revision: save.revision,
                review,
            });
        }
        for blob in self.catalog {
            let path = self.object(blob);
            if let Some(meta) = existing_file(&path)? {
                metrics.object_count += 1;
                metrics.object_bytes += meta.len();
                metrics.allocated_object_bytes = metrics
                    .allocated_object_bytes
                    .and_then(|total| disk::allocation(&path, &meta).map(|bytes| total + bytes));
            }
        }
        Ok(Snapshot {
            instances,
            metrics,
            message: message.into(),
        })
    }
}

/// Acquire the current snapshot, checking each selected object against its expected digest.
pub fn snapshot(root: &Path) -> Result<Snapshot, String> {
    Store::open(root)?.snapshot("Local fixture library.")
}

/// Apply one fixed-input operation under the app's cross-process fixture library lock.
pub fn apply(root: &Path, action: Action) -> Result<Snapshot, String> {
    apply_locked(Store::open(root)?, action)
}

fn apply_locked(mut store: Store<'_>, action: Action) -> Result<Snapshot, String> {
    // The return value includes every player. Diagnose existing sibling errors before a mutation.
    store.snapshot("")?;
    let message = store.change(action)?;
    #[cfg(test)]
    tests::fault_before_result_refresh(store.root);
    store.snapshot(message).map_err(|error| format!(
        "The fixture action completed, but the library could not be refreshed. Reopen it before retrying. {error}"
    ))
}

/// A reader holds this lease while using a selected fixture generation. Dropping it releases the OS lock.
pub struct ContentLease {
    _lock: File,
}

pub fn acquire_content_lease(root: &Path, id: InstanceId) -> Result<ContentLease, String> {
    let store = Store::open(root)?;
    let p = store.profile(id)?;
    if !store.healthy(id, p.generation)? {
        return Err("Repair the selected fixture content first.".into());
    }
    let lock = lockfile(&store.player(id)?.join("content.lock"))?;
    lock.lock_shared()
        .map_err(|e| disk::error("Could not lock fixture generation", e))?;
    Ok(ContentLease { _lock: lock })
}
