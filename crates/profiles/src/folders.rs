/* SPDX-License-Identifier: MPL-2.0 */
//! Logical folders for the instance and world library.
//!
//! A folder is app-owned metadata in `personal/folders.json`. Filing an item
//! never renames a profile, world directory or save, copies world data or
//! changes launch preferences. Removing a folder only returns its members to
//! the ungrouped list.
use crate::{
    storage,
    targets::{save_presence, valid_label, SavePresence},
    validate_target_label, Library,
};
use serde::{de, Deserialize, Deserializer, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::Read,
    path::Path,
};
use uuid::{Uuid, Version};

const FOLDERS: &str = "folders.json";
const FOLDERS_NEXT: &str = "folders.next";
const MAX_FOLDERS_PER_SCOPE: usize = 128;
const MAX_MEMBERS_PER_FOLDER: usize = 4096;
const MAX_FOLDERS_BYTES: u64 = 4 * 1024 * 1024;
const INVALID: &str = "Saved library folders are unreadable. Preserve the library.";
const MISSING_FOLDER: &str = "This folder is no longer available. Refresh the library.";

/// Folders are separate for the two library modes and never mix members.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FolderScope {
    Instances,
    Worlds,
}

/// A typed reference to an existing library item. A world uses the shared
/// composite identity: backing instance UUID plus the exact save directory.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FolderMember {
    Instance { id: Uuid },
    World { instance: Uuid, directory: String },
}

/// Serde's internally tagged enums ignore unknown fields, so a member is read
/// through this strict shape before it can reach validation.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMember {
    kind: String,
    #[serde(default)]
    id: Option<Uuid>,
    #[serde(default)]
    instance: Option<Uuid>,
    #[serde(default)]
    directory: Option<String>,
}

impl<'de> Deserialize<'de> for FolderMember {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = RawMember::deserialize(deserializer)?;
        match (raw.kind.as_str(), raw.id, raw.instance, raw.directory) {
            ("instance", Some(id), None, None) => Ok(Self::Instance { id }),
            ("world", None, Some(instance), Some(directory)) => Ok(Self::World {
                instance,
                directory,
            }),
            ("instance" | "world", ..) => {
                Err(de::Error::custom("Folder members have unexpected fields."))
            }
            (kind, ..) => Err(de::Error::unknown_variant(kind, &["instance", "world"])),
        }
    }
}

impl FolderMember {
    fn fits(&self, scope: FolderScope) -> bool {
        matches!(
            (self, scope),
            (Self::Instance { .. }, FolderScope::Instances)
                | (Self::World { .. }, FolderScope::Worlds)
        )
    }

    fn valid(&self) -> bool {
        match self {
            Self::Instance { id } => id.get_version() == Some(Version::Random),
            Self::World {
                instance,
                directory,
            } => {
                instance.get_version() == Some(Version::Random) && valid_world_directory(directory)
            }
        }
    }
}

/// A stable save directory name, never a path or a display label.
fn valid_world_directory(directory: &str) -> bool {
    valid_label(directory, 255)
        && directory != "."
        && directory != ".."
        && !directory.contains(['/', '\\'])
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Folder {
    pub id: Uuid,
    pub label: String,
    pub scope: FolderScope,
    pub members: Vec<FolderMember>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MemberResolution {
    Resolved,
    /// A partial save scan cannot prove that a world reference is missing.
    Unknown,
    Missing,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FolderEntry {
    pub member: FolderMember,
    pub resolution: MemberResolution,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FolderView {
    pub id: Uuid,
    pub label: String,
    pub scope: FolderScope,
    pub members: Vec<FolderEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FolderSnapshot {
    pub revision: Option<Uuid>,
    pub folders: Vec<FolderView>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum FolderAction {
    Create {
        scope: FolderScope,
        label: String,
    },
    Rename {
        id: Uuid,
        label: String,
    },
    /// Move a folder within its own scope. `index` is the destination position.
    Reorder {
        id: Uuid,
        index: usize,
    },
    /// Delete only the folder record. Members become ungrouped.
    Remove {
        id: Uuid,
    },
    /// File or move items into one folder. An item leaves its previous folder.
    File {
        id: Uuid,
        members: Vec<FolderMember>,
    },
    /// Return items to the ungrouped list without deleting anything.
    Unfile {
        id: Uuid,
        members: Vec<FolderMember>,
    },
    /// Remove references that a complete scan confirmed missing.
    PruneMissing {
        id: Uuid,
        members: Vec<FolderMember>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FolderIndex {
    version: u32,
    #[serde(default)]
    revision: Option<Uuid>,
    folders: Vec<Folder>,
}

impl Default for FolderIndex {
    fn default() -> Self {
        Self {
            version: 1,
            revision: None,
            folders: Vec::new(),
        }
    }
}

fn normalized_label(label: &str) -> Result<String, String> {
    validate_target_label(label)
        .map(str::to_owned)
        .map_err(|_| {
            "Enter a folder name from 1 to 80 characters without control characters.".into()
        })
}

fn duplicate_label(label: &str) -> String {
    format!("A folder named \"{label}\" already exists in this mode. Choose a different name.")
}

impl FolderIndex {
    fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err(
                "The saved folder library version is unsupported. Preserve the library.".into(),
            );
        }
        if self
            .revision
            .is_some_and(|id| id.get_version() != Some(Version::Random))
        {
            return Err(INVALID.into());
        }
        let mut ids = HashSet::new();
        let mut labels = HashSet::new();
        let mut filed = HashSet::new();
        let mut counts: HashMap<FolderScope, usize> = HashMap::new();
        for folder in &self.folders {
            if folder.id.get_version() != Some(Version::Random)
                || !ids.insert(folder.id)
                || validate_target_label(&folder.label).ok() != Some(folder.label.as_str())
                || !labels.insert((folder.scope, folder.label.to_lowercase()))
                || folder.members.len() > MAX_MEMBERS_PER_FOLDER
            {
                return Err(INVALID.into());
            }
            let count = counts.entry(folder.scope).or_insert(0);
            *count += 1;
            if *count > MAX_FOLDERS_PER_SCOPE {
                return Err(INVALID.into());
            }
            let mut local = HashSet::new();
            for member in &folder.members {
                if !member.valid()
                    || !member.fits(folder.scope)
                    || !local.insert(member.clone())
                    || !filed.insert((folder.scope, member.clone()))
                {
                    return Err(INVALID.into());
                }
            }
        }
        Ok(())
    }

    fn position(&self, id: Uuid) -> Result<usize, String> {
        self.folders
            .iter()
            .position(|folder| folder.id == id)
            .ok_or_else(|| MISSING_FOLDER.into())
    }

    fn label_taken(&self, scope: FolderScope, label: &str, except: Option<Uuid>) -> bool {
        let label = label.to_lowercase();
        self.folders.iter().any(|folder| {
            folder.scope == scope
                && Some(folder.id) != except
                && folder.label.to_lowercase() == label
        })
    }

    fn snapshot(&self, root: &Path, library: &Library) -> FolderSnapshot {
        let mut resolver = Resolver::new(root, library);
        FolderSnapshot {
            revision: self.revision,
            folders: self
                .folders
                .iter()
                .map(|folder| FolderView {
                    id: folder.id,
                    label: folder.label.clone(),
                    scope: folder.scope,
                    members: folder
                        .members
                        .iter()
                        .map(|member| FolderEntry {
                            member: member.clone(),
                            resolution: resolver.resolve(member),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    fn apply(
        &mut self,
        root: &Path,
        library: &Library,
        action: FolderAction,
    ) -> Result<(), String> {
        match action {
            FolderAction::Create { scope, label } => {
                let label = normalized_label(&label)?;
                if self.label_taken(scope, &label, None) {
                    return Err(duplicate_label(&label));
                }
                if self
                    .folders
                    .iter()
                    .filter(|folder| folder.scope == scope)
                    .count()
                    >= MAX_FOLDERS_PER_SCOPE
                {
                    return Err("This mode already has 128 folders.".into());
                }
                self.folders.push(Folder {
                    id: Uuid::new_v4(),
                    label,
                    scope,
                    members: Vec::new(),
                });
            }
            FolderAction::Rename { id, label } => {
                let label = normalized_label(&label)?;
                let position = self.position(id)?;
                if self.label_taken(self.folders[position].scope, &label, Some(id)) {
                    return Err(duplicate_label(&label));
                }
                self.folders[position].label = label;
            }
            FolderAction::Reorder { id, index } => {
                let position = self.position(id)?;
                let scope = self.folders[position].scope;
                let mut ordered: Vec<Folder> = self
                    .folders
                    .iter()
                    .filter(|folder| folder.scope == scope)
                    .cloned()
                    .collect();
                if index >= ordered.len() {
                    return Err("Choose a position within this mode's folders.".into());
                }
                let current = ordered
                    .iter()
                    .position(|folder| folder.id == id)
                    .ok_or(MISSING_FOLDER)?;
                let moved = ordered.remove(current);
                ordered.insert(index, moved);
                let mut ordered = ordered.into_iter();
                for folder in self
                    .folders
                    .iter_mut()
                    .filter(|folder| folder.scope == scope)
                {
                    *folder = ordered.next().ok_or(INVALID)?;
                }
            }
            FolderAction::Remove { id } => {
                let position = self.position(id)?;
                self.folders.remove(position);
            }
            FolderAction::File { id, members } => {
                let members = requested_members(members)?;
                let position = self.position(id)?;
                let scope = self.folders[position].scope;
                if members.iter().any(|member| !member.fits(scope)) {
                    return Err("This item belongs to a different library mode.".into());
                }
                let mut resolver = Resolver::new(root, library);
                if members
                    .iter()
                    .any(|member| resolver.resolve(member) != MemberResolution::Resolved)
                {
                    return Err(
                        "This item is no longer available. Refresh the library before filing it."
                            .into(),
                    );
                }
                let requested: HashSet<FolderMember> = members.iter().cloned().collect();
                let retained = self.folders[position]
                    .members
                    .iter()
                    .filter(|member| !requested.contains(*member))
                    .count();
                if retained + members.len() > MAX_MEMBERS_PER_FOLDER {
                    return Err("This folder already has 4096 items.".into());
                }
                for folder in self
                    .folders
                    .iter_mut()
                    .filter(|folder| folder.scope == scope)
                {
                    folder.members.retain(|member| !requested.contains(member));
                }
                // The target folder was filtered above but never removed.
                self.folders[position].members.extend(members);
            }
            FolderAction::Unfile { id, members } => {
                let members = requested_members(members)?;
                let position = self.position(id)?;
                if members
                    .iter()
                    .any(|member| !self.folders[position].members.contains(member))
                {
                    return Err("This item is not in that folder. Refresh the library.".into());
                }
                self.folders[position]
                    .members
                    .retain(|member| !members.contains(member));
            }
            FolderAction::PruneMissing { id, members } => {
                let members = requested_members(members)?;
                let position = self.position(id)?;
                if members
                    .iter()
                    .any(|member| !self.folders[position].members.contains(member))
                {
                    return Err("This item is not in that folder. Refresh the library.".into());
                }
                let mut resolver = Resolver::new(root, library);
                let mut removable = Vec::new();
                for member in &members {
                    match resolver.resolve(member) {
                        MemberResolution::Missing => removable.push(member.clone()),
                        MemberResolution::Unknown => {
                            return Err("Local saves could not be checked completely. Refresh the library before removing a missing item.".into());
                        }
                        MemberResolution::Resolved => {
                            return Err("This item is available again. Refresh the library before removing it.".into());
                        }
                    }
                }
                self.folders[position]
                    .members
                    .retain(|member| !removable.contains(member));
            }
        }
        Ok(())
    }
}

fn requested_members(members: Vec<FolderMember>) -> Result<Vec<FolderMember>, String> {
    if members.is_empty() {
        return Err("Choose at least one item.".into());
    }
    let mut seen = HashSet::new();
    if members
        .iter()
        .any(|member| !member.valid() || !seen.insert(member.clone()))
    {
        return Err("Choose each item once per change.".into());
    }
    Ok(members)
}

struct Resolver<'a> {
    root: &'a Path,
    library: &'a Library,
    scans: HashMap<Uuid, SavePresence>,
}

impl<'a> Resolver<'a> {
    fn new(root: &'a Path, library: &'a Library) -> Self {
        Self {
            root,
            library,
            scans: HashMap::new(),
        }
    }

    fn resolve(&mut self, member: &FolderMember) -> MemberResolution {
        match member {
            FolderMember::Instance { id } => {
                if self
                    .library
                    .profiles
                    .iter()
                    .any(|profile| profile.id == *id)
                {
                    MemberResolution::Resolved
                } else {
                    MemberResolution::Missing
                }
            }
            FolderMember::World {
                instance,
                directory,
            } => {
                if !self
                    .library
                    .profiles
                    .iter()
                    .any(|profile| profile.id == *instance)
                {
                    return MemberResolution::Missing;
                }
                let scan = self
                    .scans
                    .entry(*instance)
                    .or_insert_with(|| save_presence(self.root, *instance));
                if !scan.complete {
                    MemberResolution::Unknown
                } else if scan.worlds.contains(directory) {
                    MemberResolution::Resolved
                } else if scan.present.contains(directory) {
                    // The entry exists but is not a usable world: linked,
                    // special, incomplete or over the scan limit.
                    MemberResolution::Unknown
                } else {
                    MemberResolution::Missing
                }
            }
        }
    }
}

fn read_index(path: &Path) -> Result<FolderIndex, String> {
    if !storage::regular_file(path)? {
        return Ok(FolderIndex::default());
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| INVALID)?
        .take(MAX_FOLDERS_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| INVALID)?;
    if bytes.len() as u64 > MAX_FOLDERS_BYTES {
        return Err(
            "Saved library folders exceed the supported size. Preserve the library.".into(),
        );
    }
    let index: FolderIndex = serde_json::from_slice(&bytes).map_err(|_| INVALID)?;
    index.validate()?;
    Ok(index)
}

/// Read the logical folders and resolve every reference without changing anything.
pub fn folders(root: &Path) -> Result<FolderSnapshot, String> {
    let (_lock, library) = storage::open(root)?;
    let index = read_index(&root.join("personal").join(FOLDERS))?;
    Ok(index.snapshot(root, &library))
}

/// Change folder metadata at the revision the caller observed.
pub fn change_folders(
    root: &Path,
    expected_revision: Option<Uuid>,
    action: FolderAction,
) -> Result<FolderSnapshot, String> {
    let (_lock, library) = storage::open(root)?;
    let directory = root.join("personal");
    let mut index = read_index(&directory.join(FOLDERS))?;
    if index.revision != expected_revision {
        return Err("Folders changed in another window. Refresh the library and try again.".into());
    }
    let before = index.folders.clone();
    index.apply(root, &library, action)?;
    if index.folders == before {
        return Ok(index.snapshot(root, &library));
    }
    index.revision = Some(Uuid::new_v4());
    index.validate()?;
    let bytes = serde_json::to_vec(&index).map_err(|_| "Could not encode library folders.")?;
    storage::publish(&directory, FOLDERS, FOLDERS_NEXT, &bytes)?;
    Ok(index.snapshot(root, &library))
}

#[cfg(test)]
mod tests;
