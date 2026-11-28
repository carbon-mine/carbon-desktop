//! App-owned library mode and read-only world projection.
//!
//! The instance profile remains the owner of its game directory and saves. A
//! projected world is identified by the backing instance UUID and the exact
//! save-directory name. Nothing in this module moves, copies, deletes or
//! reinterprets user data.
use crate::{storage, Profile};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};
use uuid::Uuid;

const MODE_FILE: &str = "library-mode.json";
const MODE_NEXT: &str = "library-mode.next";
const MAX_MODE_BYTES: u64 = 1024;

/// Presentation mode for the library. The default is world-first.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LibraryMode {
    #[default]
    Worlds,
    Instances,
}

impl LibraryMode {
    pub const ALL: [Self; 2] = [Self::Worlds, Self::Instances];
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredMode {
    version: u32,
    mode: LibraryMode,
}

/// Stable identity of a projected world. The directory is the exact save
/// directory name, not a display label and not a path outside `game/saves`.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorldId {
    pub instance: Uuid,
    pub directory: String,
}

impl WorldId {
    pub fn new(instance: Uuid, directory: impl Into<String>) -> Self {
        Self {
            instance,
            directory: directory.into(),
        }
    }

    /// A stable, non-filesystem key suitable for GPUI element IDs and logs.
    pub fn key(&self) -> String {
        format!("{}:{}", self.instance, self.directory)
    }
}

/// Ownership and source of the backing instance. `Managed` is reserved for a
/// future explicit world-owned record; this slice never creates one.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WorldBacking {
    Local,
    Imported,
    ServerBound,
    Managed,
}

impl WorldBacking {
    fn classify(profile: &Profile) -> Self {
        if profile.server_address.is_some() {
            Self::ServerBound
        } else if profile.imported_pack.is_some() {
            Self::Imported
        } else {
            Self::Local
        }
    }
}

/// Direct world launch is intentionally unavailable until protected execution
/// and world-entry observation are qualified.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WorldLaunch {
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryWorld {
    pub id: WorldId,
    pub display_name: String,
    pub backing: WorldBacking,
    pub instance_name: String,
    pub version: String,
    pub loader: Option<String>,
    /// The backing instance's passive launch-target preference, when present.
    pub selected: bool,
    /// A local scan issue that does not prove the save is missing.
    pub issue: Option<String>,
    pub launch: WorldLaunch,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldIssue {
    pub instance_id: Uuid,
    pub instance_name: String,
    pub message: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WorldLibrary {
    pub worlds: Vec<LibraryWorld>,
    pub issues: Vec<WorldIssue>,
}

impl WorldLibrary {
    pub fn get(&self, id: &WorldId) -> Option<&LibraryWorld> {
        self.worlds.iter().find(|world| &world.id == id)
    }

    /// Prefer the first fully readable world. A world with a scan issue remains
    /// visible but is not silently recommended for a future direct action.
    pub fn recommended(&self) -> Option<&LibraryWorld> {
        self.worlds
            .iter()
            .find(|world| world.issue.is_none())
            .or_else(|| self.worlds.first())
    }
}

fn mode_path(root: &Path) -> PathBuf {
    root.join("personal").join(MODE_FILE)
}

fn read_mode(root: &Path) -> Result<LibraryMode, String> {
    let path = mode_path(root);
    if !storage::regular_file(&path)? {
        return Ok(LibraryMode::Worlds);
    }
    let mut bytes = Vec::new();
    File::open(&path)
        .map_err(|_| "Could not read the library mode preference.")?
        .take(MAX_MODE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read the library mode preference.")?;
    if bytes.len() as u64 > MAX_MODE_BYTES {
        return Err(
            "The library mode preference is unreadable. Preserve it and reload the library.".into(),
        );
    }
    let stored: StoredMode = serde_json::from_slice(&bytes).map_err(|_| {
        "The library mode preference is unreadable. Preserve it and reload the library."
    })?;
    if stored.version != 1 {
        return Err(
            "The library mode preference version is unsupported. Preserve it and reload the library."
                .into(),
        );
    }
    Ok(stored.mode)
}

/// Read the saved mode. A missing preference means `Worlds`; malformed or
/// unsupported data is reported instead of being overwritten.
pub fn library_mode(root: &Path) -> Result<LibraryMode, String> {
    let (_lock, _library) = storage::open(root)?;
    read_mode(root)
}

/// Persist only the presentation mode. Existing profiles, target preferences,
/// labels and saves are not read or written by this operation beyond acquiring
/// the shared library lock.
pub fn set_library_mode(root: &Path, mode: LibraryMode) -> Result<LibraryMode, String> {
    let (_lock, _library) = storage::open(root)?;
    // An unreadable preference is reported by `library_mode` and preserved
    // until an explicit Settings choice replaces it. Starting the app never
    // rewrites it.
    if read_mode(root).ok() == Some(mode) {
        return Ok(mode);
    }
    let stored = StoredMode { version: 1, mode };
    let bytes =
        serde_json::to_vec(&stored).map_err(|_| "Could not encode the library mode preference.")?;
    storage::publish(&root.join("personal"), MODE_FILE, MODE_NEXT, &bytes)?;
    Ok(mode)
}

/// Project all readable local worlds from every saved profile. The scan is
/// passive: it does not open `level.dat`, follow links or change preferences.
pub fn world_library(root: &Path) -> Result<WorldLibrary, String> {
    let (lock, library) = storage::open(root)?;
    let profiles = library.profiles;
    // The profile list is the consistent snapshot. Release the library lock
    // before walking save directories so a large library does not block a
    // profile or target write for the whole scan.
    drop(lock);
    let mut worlds = Vec::new();
    let mut issues = Vec::new();
    for profile in &profiles {
        match crate::targets::world_snapshot(root, profile.id) {
            Ok(snapshot) => {
                let scan_issue = snapshot.issue;
                if let Some(issue) = &scan_issue {
                    issues.push(WorldIssue {
                        instance_id: profile.id,
                        instance_name: profile.name.clone(),
                        message: issue.clone(),
                    });
                }
                for world in snapshot.worlds {
                    let directory = match &world.key {
                        crate::TargetKey::World(directory) => directory.clone(),
                        crate::TargetKey::Server(_) => continue,
                    };
                    let selected = snapshot.selected.as_ref().is_some_and(|selected| {
                        matches!(selected, crate::TargetKey::World(name) if name == &directory)
                    });
                    worlds.push(LibraryWorld {
                        id: WorldId::new(profile.id, directory),
                        display_name: world.name,
                        backing: WorldBacking::classify(profile),
                        instance_name: profile.name.clone(),
                        version: profile.version.id().to_owned(),
                        loader: profile
                            .imported_pack
                            .as_ref()
                            .and_then(|pack| pack.loader.clone()),
                        selected,
                        issue: scan_issue.clone(),
                        launch: WorldLaunch::Unavailable,
                    });
                }
            }
            Err(message) => issues.push(WorldIssue {
                instance_id: profile.id,
                instance_name: profile.name.clone(),
                message,
            }),
        }
    }
    worlds.sort_by(|left, right| {
        left.display_name
            .to_lowercase()
            .cmp(&right.display_name.to_lowercase())
            .then_with(|| left.instance_name.cmp(&right.instance_name))
            .then_with(|| left.id.cmp(&right.id))
    });
    issues.sort_by(|left, right| {
        left.instance_name
            .cmp(&right.instance_name)
            .then_with(|| left.message.cmp(&right.message))
    });
    Ok(WorldLibrary { worlds, issues })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn root() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn write_world(dir: &Path, name: &str) {
        let save = dir.join("game/saves").join(name);
        fs::create_dir_all(&save).unwrap();
        fs::write(save.join("level.dat"), b"synthetic").unwrap();
    }

    #[test]
    fn mode_defaults_to_worlds_and_round_trips_without_touching_profiles() {
        let temp = root();
        let root = temp.path();
        crate::seed_review_library(root, false).unwrap();
        let before = crate::list(root).unwrap();
        assert_eq!(library_mode(root).unwrap(), LibraryMode::Worlds);
        assert_eq!(
            set_library_mode(root, LibraryMode::Instances).unwrap(),
            LibraryMode::Instances
        );
        assert_eq!(library_mode(root).unwrap(), LibraryMode::Instances);
        assert_eq!(crate::list(root).unwrap(), before);
        let bytes = fs::read(root.join("personal/library-mode.json")).unwrap();
        assert!(bytes.windows(9).any(|window| window == b"instances"));
    }

    #[test]
    fn concurrent_mode_writers_leave_one_valid_preference() {
        let temp = root();
        let root = temp.path();
        crate::seed_review_library(root, false).unwrap();
        std::thread::scope(|scope| {
            for index in 0..8 {
                scope.spawn(move || {
                    let mode = if index % 2 == 0 {
                        LibraryMode::Worlds
                    } else {
                        LibraryMode::Instances
                    };
                    set_library_mode(root, mode).unwrap();
                });
            }
        });
        let mode = library_mode(root).unwrap();
        assert!(matches!(mode, LibraryMode::Worlds | LibraryMode::Instances));
        assert_eq!(crate::list(root).unwrap().len(), 1);
    }

    #[test]
    fn invalid_mode_is_reported_and_preserved_until_an_explicit_choice() {
        let temp = root();
        let root = temp.path();
        crate::seed_review_library(root, false).unwrap();
        let path = root.join("personal/library-mode.json");
        fs::write(&path, b"not a mode").unwrap();
        assert!(library_mode(root).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"not a mode");
        assert_eq!(
            set_library_mode(root, LibraryMode::Instances).unwrap(),
            LibraryMode::Instances
        );
        assert_eq!(library_mode(root).unwrap(), LibraryMode::Instances);
    }

    #[test]
    fn projection_keeps_duplicate_names_and_server_bound_sources_distinct() {
        let temp = root();
        let root = temp.path();
        crate::seed_review_library(root, true).unwrap();
        let profiles = crate::list(root).unwrap();
        let first = profiles[0].id;
        let second = profiles[1].id;
        let bound = profiles[2].id;
        write_world(&root.join("instances-v1").join(first.to_string()), "Shared");
        write_world(
            &root.join("instances-v1").join(second.to_string()),
            "Shared",
        );
        write_world(&root.join("instances-v1").join(bound.to_string()), "Bound");
        write_world(&root.join("instances-v1").join(first.to_string()), "Copied");
        crate::set_server_address(
            root,
            bound,
            None,
            Some(crate::ServerAddress::parse("example.org").unwrap()),
        )
        .unwrap();

        let library = world_library(root).unwrap();
        let shared: Vec<_> = library
            .worlds
            .iter()
            .filter(|world| world.display_name == "Shared")
            .collect();
        assert_eq!(shared.len(), 2);
        assert_ne!(shared[0].id, shared[1].id);
        assert_eq!(shared[0].backing, WorldBacking::Local);
        assert_eq!(shared[1].backing, WorldBacking::Local);
        assert!(library.worlds.iter().any(|world| {
            world.id.instance == bound
                && world.display_name == "Bound"
                && world.backing == WorldBacking::ServerBound
        }));
        assert_eq!(
            library
                .worlds
                .iter()
                .filter(|world| world.id.instance == first)
                .count(),
            2
        );
        assert!(library
            .worlds
            .iter()
            .all(|world| world.launch == WorldLaunch::Unavailable));
    }

    #[test]
    fn backing_sources_are_explicit_and_do_not_relabel_existing_profiles() {
        let temp = root();
        let root = temp.path();
        crate::seed_review_library(root, false).unwrap();
        let mut profile = crate::list(root).unwrap().remove(0);
        assert_eq!(WorldBacking::classify(&profile), WorldBacking::Local);
        profile.imported_pack = Some(crate::ImportedPack {
            name: profile.name.clone(),
            loader: Some("Fabric".into()),
            pending_files: 0,
        });
        assert_eq!(WorldBacking::classify(&profile), WorldBacking::Imported);
        profile.server_address = Some(crate::ServerAddress::parse("example.org").unwrap());
        assert_eq!(WorldBacking::classify(&profile), WorldBacking::ServerBound);
    }

    #[test]
    fn missing_level_data_does_not_invent_a_world() {
        let temp = root();
        let root = temp.path();
        crate::seed_review_library(root, false).unwrap();
        let id = crate::list(root).unwrap()[0].id;
        let saves = root
            .join("instances-v1")
            .join(id.to_string())
            .join("game/saves");
        fs::create_dir_all(saves.join("Not a save")).unwrap();
        let library = world_library(root).unwrap();
        assert!(library.worlds.is_empty());
        assert!(library.issues.is_empty());
    }

    #[test]
    fn world_identity_is_the_exact_directory_even_when_the_label_changes() {
        let temp = root();
        let root = temp.path();
        crate::seed_review_library(root, false).unwrap();
        let profile = crate::list(root).unwrap().remove(0);
        write_world(
            &root.join("instances-v1").join(profile.id.to_string()),
            "Folder",
        );
        let before = world_library(root).unwrap().worlds[0].id.clone();
        assert_eq!(before.directory, "Folder");
        crate::change_instance_targets(
            root,
            profile.id,
            None,
            crate::TargetAction::LabelWorld {
                directory: "Folder".into(),
                label: Some("Display".into()),
            },
        )
        .unwrap();
        let after = world_library(root).unwrap();
        assert_eq!(after.worlds[0].id, before);
        assert_eq!(after.worlds[0].display_name, "Display");
    }
}
