/* SPDX-License-Identifier: MPL-2.0 */
//! Passive launch-target inventory and per-instance preferences.
use crate::{storage, ServerAddress};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};
use uuid::{Uuid, Version};

const MAX_DESTINATIONS: usize = 64;
const MAX_WORLD_LABELS: usize = 64;
const MAX_WORLD_ENTRIES: usize = 4096;
const MAX_PREFERENCE_BYTES: u64 = 256 * 1024;

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

fn valid_label(label: &str, max: usize) -> bool {
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

fn inventory(root: &Path, id: Uuid) -> Result<Vec<LocalWorld>, String> {
    let instances = root.join("instances-v1");
    let instance = instances.join(id.to_string());
    let game = instance.join("game");
    let saves = game.join("saves");
    for directory in [&instances, &instance, &game, &saves] {
        if !storage::ordinary_directory(directory)? {
            return Ok(Vec::new());
        }
    }
    let mut worlds = Vec::new();
    for (position, entry) in fs::read_dir(saves)
        .map_err(|_| "Could not read local saves.")?
        .enumerate()
    {
        if position == MAX_WORLD_ENTRIES {
            return Err("This instance has too many save directory entries to list safely.".into());
        }
        let entry = entry.map_err(|_| "Could not read a local save entry.")?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if valid_label(&name, 255)
            && matches!(storage::ordinary_directory(&entry.path()), Ok(true))
            && matches!(
                storage::regular_file(&entry.path().join("level.dat")),
                Ok(true)
            )
        {
            worlds.push(LocalWorld {
                key: TargetKey::World(name.clone()),
                name,
            });
        }
    }
    worlds.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(worlds)
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
    fn contains(&self, key: &TargetKey, worlds: &[LocalWorld]) -> bool {
        match key {
            TargetKey::World(_) => worlds.iter().any(|world| world.key == *key),
            TargetKey::Server(id) => self.destinations.iter().any(|server| server.id == *id),
        }
    }

    fn snapshot(self, mut worlds: Vec<LocalWorld>) -> InstanceTargets {
        let selected = self
            .selected
            .as_ref()
            .filter(|key| self.contains(key, &worlds))
            .cloned();
        for world in &mut worlds {
            let TargetKey::World(directory) = &world.key else {
                continue;
            };
            if let Some(label) = self.world_labels.get(directory) {
                world.name.clone_from(label);
            }
        }
        InstanceTargets {
            revision: self.revision,
            worlds,
            destinations: self.destinations,
            selected,
        }
    }

    fn apply(&mut self, action: TargetAction, worlds: &[LocalWorld]) -> Result<(), String> {
        match action {
            TargetAction::Select(key) => {
                if key.as_ref().is_some_and(|key| !self.contains(key, worlds)) {
                    return Err("This target is no longer available. Refresh the list.".into());
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
            TargetAction::LabelWorld { directory, label } => {
                if !self.contains(&TargetKey::World(directory.clone()), worlds) {
                    return Err("This local save is no longer available. Refresh the list.".into());
                }
                match label {
                    Some(label) => {
                        let label = validate_target_label(&label)?.to_owned();
                        if !self.world_labels.contains_key(&directory)
                            && self.world_labels.len() >= MAX_WORLD_LABELS
                        {
                            return Err("This instance already has 64 custom save labels.".into());
                        }
                        self.world_labels.insert(directory, label);
                    }
                    None => {
                        self.world_labels.remove(&directory);
                    }
                }
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
    worlds: Vec<LocalWorld>,
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
        worlds: inventory(root, id)?,
    })
}

/// List an instance's verified directory entries and saved destinations without contacting servers.
pub fn instance_targets(root: &Path, id: Uuid) -> Result<InstanceTargets, String> {
    let (_lock, library) = storage::open(root)?;
    let LoadedTargets { saved, worlds, .. } = load(root, id, &library)?;
    Ok(saved.snapshot(worlds))
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
        worlds,
    } = load(root, id, &library)?;
    if saved.revision != expected_revision {
        return Err(
            "Launch targets changed in another window. Refresh the list and try again.".into(),
        );
    }
    let previous = saved.clone();
    saved.apply(action, &worlds)?;
    if saved == previous {
        return Ok(saved.snapshot(worlds));
    }
    saved.revision = Some(Uuid::new_v4());
    let bytes = serde_json::to_vec(&saved).map_err(|_| "Could not encode launch targets.")?;
    storage::publish(
        &directory,
        &index_name,
        &format!("targets-{id}.next"),
        &bytes,
    )?;
    Ok(saved.snapshot(worlds))
}

#[cfg(test)]
mod tests;
