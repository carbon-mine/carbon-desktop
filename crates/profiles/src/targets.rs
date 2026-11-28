/* SPDX-License-Identifier: MPL-2.0 */
//! Passive launch-target inventory and per-instance preferences.
use crate::{storage, ServerAddress};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
};
use uuid::{Uuid, Version};

const MAX_DESTINATIONS: usize = 64;
const MAX_WORLD_LABELS: usize = 64;
const MAX_WORLD_ENTRIES: usize = 4096;
const MAX_PREFERENCE_BYTES: u64 = 256 * 1024;
const SCAN_INCOMPLETE: &str =
    "Some local saves could not be checked safely. Refresh before choosing a local save.";
const SCAN_OVER_LIMIT: &str = "This instance has too many save directory entries to list safely.";

/// A save directory under this instance's game/saves, or an independent launcher destination.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "camelCase")]
pub enum TargetKey {
    World(String),
    Server(Uuid),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalWorld {
    pub key: TargetKey,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedDestination {
    pub id: Uuid,
    pub name: String,
    pub address: ServerAddress,
}

#[derive(Debug)]
pub struct InstanceTargets {
    pub revision: Option<Uuid>,
    pub worlds: Vec<LocalWorld>,
    pub destinations: Vec<SavedDestination>,
    pub selected: Option<TargetKey>,
    /// Set when local save entries could not all be checked. Save targets then stay locked
    /// until a later read completes, while destinations remain usable.
    pub world_issue: Option<String>,
    /// Custom labels whose folders were confirmed absent by a complete scan.
    pub missing_labels: BTreeMap<String, String>,
}

pub enum TargetAction {
    Select(Option<TargetKey>),
    SaveDestination {
        id: Option<Uuid>,
        name: String,
        address: ServerAddress,
    },
    RemoveDestination(Uuid),
    LabelWorld {
        directory: String,
        label: Option<String>,
    },
    /// Remove a label only if the folder is still confirmed missing. This is the
    /// stale-cleanup-safe counterpart to `LabelWorld { label: None }`.
    ForgetMissingLabel(String),
}

#[derive(Clone, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct Preferences {
    revision: Option<Uuid>,
    selected: Option<TargetKey>,
    destinations: Vec<SavedDestination>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    world_labels: BTreeMap<String, String>,
}

pub(crate) fn valid_label(label: &str, max: usize) -> bool {
    (1..=max).contains(&label.chars().count())
        && !label.chars().any(|ch| ch.is_control() || matches!(ch,
            '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
        ))
}

/// Validate and normalize a label without changing saved preferences.
pub fn validate_target_label(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if !valid_label(name, 80) {
        return Err("Enter a name from 1 to 80 characters without control characters.".into());
    }
    Ok(name)
}

fn inventory(root: &Path, id: Uuid) -> WorldScan {
    let instances = root.join("instances-v1");
    let instance = instances.join(id.to_string());
    let game = instance.join("game");
    let saves = game.join("saves");
    for directory in [&instances, &instance, &game, &saves] {
        match inspect(directory) {
            Ok(EntryKind::Directory) => {}
            Ok(EntryKind::Absent) => return WorldScan::empty(),
            Ok(_) => return WorldScan::incomplete(Vec::new(), HashSet::new(), SCAN_INCOMPLETE),
            Err(issue) => return WorldScan::incomplete(Vec::new(), HashSet::new(), &issue),
        }
    }
    let entries = match fs::read_dir(&saves) {
        Ok(entries) => entries,
        Err(_) => return WorldScan::incomplete(Vec::new(), HashSet::new(), SCAN_INCOMPLETE),
    };
    let mut worlds = Vec::new();
    let mut present = HashSet::new();
    for (position, entry) in entries.enumerate() {
        if position == MAX_WORLD_ENTRIES {
            return WorldScan::incomplete(worlds, present, SCAN_OVER_LIMIT);
        }
        let Ok(entry) = entry else {
            return WorldScan::incomplete(worlds, present, SCAN_INCOMPLETE);
        };
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        // Observed names stay present even when they are later rejected, so an
        // unsafe entry is never reported as a missing folder or silently replaced.
        present.insert(name.clone());
        match inspect(&entry.path()) {
            Ok(EntryKind::Directory) => {}
            Ok(_) => continue,
            Err(issue) => return WorldScan::incomplete(worlds, present, &issue),
        }
        if !valid_label(&name, 255) {
            continue;
        }
        match inspect(&entry.path().join("level.dat")) {
            Ok(EntryKind::File) => worlds.push(LocalWorld {
                key: TargetKey::World(name.clone()),
                name,
            }),
            // An absent or ordinary non-file level.dat is an incomplete save, not a scan
            // failure. Links and special files are rejected without being followed.
            Ok(_) => {}
            Err(issue) => return WorldScan::incomplete(worlds, present, &issue),
        }
    }
    WorldScan::complete(worlds, present)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EntryKind {
    Absent,
    Directory,
    File,
    Other,
}

/// Classify one path without following links. An unreadable path is an error rather
/// than an empty entry, so callers can keep save targets locked instead of guessing.
fn inspect(path: &Path) -> Result<EntryKind, String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if !storage::ordinary(&meta) => Ok(EntryKind::Other),
        Ok(meta) if meta.is_dir() => Ok(EntryKind::Directory),
        Ok(meta) if meta.is_file() => Ok(EntryKind::File),
        Ok(_) => Ok(EntryKind::Other),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(EntryKind::Absent),
        Err(_) => Err(SCAN_INCOMPLETE.into()),
    }
}

struct WorldScan {
    worlds: Vec<LocalWorld>,
    /// Every directory entry observed under a readable saves folder, including
    /// entries rejected as links, special files or incomplete saves.
    present: HashSet<String>,
    issue: Option<String>,
}

impl WorldScan {
    fn empty() -> Self {
        Self::complete(Vec::new(), HashSet::new())
    }

    fn complete(worlds: Vec<LocalWorld>, present: HashSet<String>) -> Self {
        let mut worlds = worlds;
        worlds.sort_by(|a, b| a.name.cmp(&b.name));
        Self {
            worlds,
            present,
            issue: None,
        }
    }

    fn incomplete(worlds: Vec<LocalWorld>, present: HashSet<String>, issue: &str) -> Self {
        let mut worlds = worlds;
        worlds.sort_by(|a, b| a.name.cmp(&b.name));
        Self {
            worlds,
            present,
            issue: Some(issue.into()),
        }
    }
}

/// Observe one instance's saves directory without following links. Folder
/// resolution uses the same scan as the launch-target chooser.
#[derive(Clone, Debug)]
pub(crate) struct SavePresence {
    /// Every observed directory-entry name, including entries that are not
    /// usable worlds. Absence from a complete scan is pruning evidence.
    pub(crate) present: HashSet<String>,
    /// Ordinary save directories with an ordinary `level.dat`. Only these
    /// resolve as a world for filing; observed-only names stay ambiguous.
    pub(crate) worlds: HashSet<String>,
    /// False when at least one entry could not be inspected safely.
    pub(crate) complete: bool,
}

pub(crate) fn save_presence(root: &Path, id: Uuid) -> SavePresence {
    let scan = inventory(root, id);
    let worlds = scan
        .worlds
        .iter()
        .filter_map(|world| match &world.key {
            TargetKey::World(directory) => Some(directory.clone()),
            TargetKey::Server(_) => None,
        })
        .collect();
    SavePresence {
        present: scan.present,
        worlds,
        complete: scan.issue.is_none(),
    }
}

fn world_present(scan: &WorldScan, directory: &str) -> bool {
    scan.worlds
        .iter()
        .any(|world| matches!(&world.key, TargetKey::World(name) if name == directory))
}

fn read_preferences(path: &Path) -> Result<Preferences, String> {
    if !storage::regular_file(path)? {
        return Ok(Preferences::default());
    }
    const INVALID: &str = "Saved launch targets are unreadable. Preserve the instance data.";
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| INVALID)?
        .take(MAX_PREFERENCE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| INVALID)?;
    if bytes.len() as u64 > MAX_PREFERENCE_BYTES {
        return Err(INVALID.into());
    }
    let saved: Preferences = serde_json::from_slice(&bytes).map_err(|_| INVALID)?;
    let mut ids = HashSet::new();
    if saved
        .revision
        .is_none_or(|id| id.get_version() != Some(Version::Random))
        || saved.destinations.len() > MAX_DESTINATIONS
        || saved.world_labels.len() > MAX_WORLD_LABELS
        || saved.world_labels.iter().any(|(directory, label)| {
            !valid_label(directory, 255)
                || validate_target_label(label).ok() != Some(label.as_str())
        })
        || saved.destinations.iter().any(|server| {
            server.id.get_version() != Some(Version::Random)
                || !ids.insert(server.id)
                || !valid_label(&server.name, 80)
                || server.name.trim() != server.name
        })
    {
        return Err(INVALID.into());
    }
    Ok(saved)
}

impl Preferences {
    fn has_destination(&self, id: Uuid) -> bool {
        self.destinations.iter().any(|server| server.id == id)
    }

    fn snapshot(self, scan: WorldScan) -> InstanceTargets {
        let WorldScan {
            mut worlds,
            present,
            issue,
        } = scan;
        // An incomplete scan cannot disprove a saved preference. All world actions
        // still require a complete inventory at the point of use.
        let selected = self.selected.as_ref().and_then(|key| match key {
            TargetKey::World(_)
                if issue.is_some() || worlds.iter().any(|world| world.key == *key) =>
            {
                Some(key.clone())
            }
            TargetKey::Server(id) if self.has_destination(*id) => Some(key.clone()),
            _ => None,
        });
        for world in &mut worlds {
            let TargetKey::World(directory) = &world.key else {
                continue;
            };
            if let Some(label) = self.world_labels.get(directory) {
                world.name.clone_from(label);
            }
        }
        // Labels are never pruned implicitly. Only a complete scan can establish that
        // a folder is missing, so partial scans report no missing labels.
        let missing_labels = if issue.is_none() {
            self.world_labels
                .iter()
                .filter(|(directory, _)| !present.contains(*directory))
                .map(|(directory, label)| (directory.clone(), label.clone()))
                .collect()
        } else {
            BTreeMap::new()
        };
        InstanceTargets {
            revision: self.revision,
            worlds,
            destinations: self.destinations,
            selected,
            world_issue: issue,
            missing_labels,
        }
    }

    fn apply(&mut self, action: TargetAction, scan: &WorldScan) -> Result<(), String> {
        match action {
            TargetAction::Select(key) => {
                if let Some(key) = &key {
                    match key {
                        TargetKey::World(_) if scan.issue.is_some() => {
                            return Err("Local saves could not be checked completely. Refresh before choosing a local save.".into());
                        }
                        TargetKey::World(directory) if !world_present(scan, directory) => {
                            return Err(
                                "This target is no longer available. Refresh the list.".into()
                            );
                        }
                        TargetKey::Server(id) if !self.has_destination(*id) => {
                            return Err(
                                "This target is no longer available. Refresh the list.".into()
                            );
                        }
                        _ => {}
                    }
                }
                self.selected = key;
            }
            TargetAction::SaveDestination { id, name, address } => {
                let name = validate_target_label(&name)?;
                if let Some(id) = id {
                    let destination = self
                        .destinations
                        .iter_mut()
                        .find(|entry| entry.id == id)
                        .ok_or("This saved destination is no longer available.")?;
                    destination.name = name.to_owned();
                    destination.address = address;
                } else {
                    if self.destinations.len() >= MAX_DESTINATIONS {
                        return Err("This instance already has 64 saved destinations.".into());
                    }
                    self.destinations.push(SavedDestination {
                        id: Uuid::new_v4(),
                        name: name.to_owned(),
                        address,
                    });
                }
            }
            TargetAction::LabelWorld { directory, label } => match label {
                Some(label) => {
                    if scan.issue.is_some() {
                        return Err("Local saves could not be checked completely. Refresh before labeling a local save.".into());
                    }
                    if !world_present(scan, &directory) {
                        return Err(
                            "This local save is no longer available. Refresh the list.".into()
                        );
                    }
                    let label = validate_target_label(&label)?.to_owned();
                    if !self.world_labels.contains_key(&directory)
                        && self.world_labels.len() >= MAX_WORLD_LABELS
                    {
                        return Err("This instance already has 64 custom save labels.".into());
                    }
                    self.world_labels.insert(directory, label);
                }
                None => {
                    if scan.issue.is_some() {
                        return Err("Local saves could not be checked completely. Refresh before changing a local save label.".into());
                    }
                    if !world_present(scan, &directory) && scan.present.contains(&directory) {
                        return Err("This local save is still present. Refresh the list before removing its label.".into());
                    }
                    // This explicit request is the only path that reclaims a label slot
                    // for a missing folder; reads and server changes never do.
                    self.world_labels.remove(&directory);
                }
            },
            TargetAction::ForgetMissingLabel(directory) => {
                if scan.issue.is_some() {
                    return Err("Local saves could not be checked completely. Refresh before removing a local save label.".into());
                }
                if scan.present.contains(&directory) {
                    return Err("This local save is available again. Refresh the list before removing its label.".into());
                }
                self.world_labels.remove(&directory);
            }
            TargetAction::RemoveDestination(id) => {
                let index = self
                    .destinations
                    .iter()
                    .position(|entry| entry.id == id)
                    .ok_or("This saved destination is no longer available.")?;
                self.destinations.remove(index);
                if self.selected == Some(TargetKey::Server(id)) {
                    self.selected = None;
                }
            }
        }
        Ok(())
    }
}

struct LoadedTargets {
    directory: PathBuf,
    index_name: String,
    saved: Preferences,
    scan: WorldScan,
}

fn load(root: &Path, id: Uuid, library: &crate::Library) -> Result<LoadedTargets, String> {
    let profile = &library.profiles[library.position(id)?];
    if profile.server_address.is_some() {
        return Err("This dedicated instance uses its saved server address.".into());
    }
    let directory = root.join("personal");
    let index_name = format!("targets-{id}.json");
    let saved = read_preferences(&directory.join(&index_name))?;
    Ok(LoadedTargets {
        directory,
        index_name,
        saved,
        scan: inventory(root, id),
    })
}

/// List an instance's verified directory entries and saved destinations without contacting servers.
pub fn instance_targets(root: &Path, id: Uuid) -> Result<InstanceTargets, String> {
    let (_lock, library) = storage::open(root)?;
    let LoadedTargets { saved, scan, .. } = load(root, id, &library)?;
    Ok(saved.snapshot(scan))
}

/// Save target preferences only. A revision change requires a refresh before retry.
pub fn change_instance_targets(
    root: &Path,
    id: Uuid,
    expected_revision: Option<Uuid>,
    action: TargetAction,
) -> Result<InstanceTargets, String> {
    let (_lock, library) = storage::open(root)?;
    let LoadedTargets {
        directory,
        index_name,
        mut saved,
        scan,
    } = load(root, id, &library)?;
    if saved.revision != expected_revision {
        return Err(
            "Launch targets changed in another window. Refresh the list and try again.".into(),
        );
    }
    let previous = saved.clone();
    saved.apply(action, &scan)?;
    if saved == previous {
        return Ok(saved.snapshot(scan));
    }
    saved.revision = Some(Uuid::new_v4());
    let bytes = serde_json::to_vec(&saved).map_err(|_| "Could not encode launch targets.")?;
    storage::publish(
        &directory,
        &index_name,
        &format!("targets-{id}.next"),
        &bytes,
    )?;
    Ok(saved.snapshot(scan))
}

#[cfg(test)]
mod tests;
