// SPDX-License-Identifier: MPL-2.0
//! Deterministic, offline development-preview libraries.
//!
//! The command seeds a dedicated root, never the ordinary per-user library. It
//! uses the ordinary profile, launch-target, icon and fixture-store operations
//! so a newly modeled field keeps its existing validation. Re-running a seeded
//! set is a no-op; `--reset-fixtures` deletes only a root carrying this
//! module's marker before seeding it again.
mod catalog;

use crate::home::HomeOrder;
use anyhow::{Context as _, Result, bail};
use carbon_profiles::{
    ImportedPack, ProfileAction, ServerAddress, TargetAction, TargetKey, change,
    change_instance_targets, icons, import_instance, instance_targets, set_server_address,
};
use catalog::{DEFAULT_HOME, DEFAULT_PROFILES, ProfileRow, Selection};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::ErrorKind,
    path::{Component, Path, PathBuf},
};
use uuid::Uuid;

const MARKER_FILE: &str = "dev-preview-fixtures.json";
const MARKER_VERSION: u32 = 1;
const MAX_MARKER_BYTES: u64 = 16 * 1024;
/// Inert marker written beside each synthetic save folder. The launch-target
/// inventory only requires a regular `level.dat`; Carbon never claims the save
/// is playable and no world entry or map capture is fabricated.
const WORLD_MARKER: &[u8] = b"Carbon development fixture marker. This is not a playable save.\n";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Set {
    Default,
    Empty,
    Large,
    Faults,
}

impl Set {
    pub const ALL: [Self; 4] = [Self::Default, Self::Empty, Self::Large, Self::Faults];

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "default" => Ok(Self::Default),
            "empty" => Ok(Self::Empty),
            "large" => Ok(Self::Large),
            "faults" => Ok(Self::Faults),
            _ => bail!(
                "Unknown fixture set '{value}'. Choose {}.",
                Self::ALL.map(Self::as_str).join(", ")
            ),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Empty => "empty",
            Self::Large => "large",
            Self::Faults => "faults",
        }
    }

    /// The large and fault sets open directly in Library; the others start on
    /// Home. Each surface is the quickest view of that set's intended state.
    pub const fn starts_in_library(self) -> bool {
        matches!(self, Self::Large | Self::Faults)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Marker {
    version: u32,
    pub(crate) set: Set,
    pub(crate) rows: Vec<String>,
    pub(crate) store_instances: usize,
    /// False only while a seed is in progress. A failed seed leaves this marker
    /// so `--reset-fixtures` can recover the dedicated root without guessing.
    #[serde(default = "marker_complete")]
    complete: bool,
}

fn marker_complete() -> bool {
    true
}

impl Marker {
    fn validate(&self) -> Result<()> {
        if self.version != MARKER_VERSION {
            bail!(
                "The development fixture root uses an unsupported marker version. Use a new root."
            );
        }
        Ok(())
    }
}

/// The dedicated root for a named set. It is a sibling of the ordinary GPUI
/// persistent library, never that library or one of its ancestors.
pub(crate) fn default_root(set: Set) -> Result<PathBuf> {
    let base = data_home()?.join("dev-fixtures");
    Ok(base.join(set.as_str()))
}

#[cfg(windows)]
fn data_home() -> Result<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("CarbonDesktopGpui"))
        .filter(|path| path.is_absolute())
        .context("LOCALAPPDATA is unavailable for the development fixture root")
}

#[cfg(not(windows))]
fn data_home() -> Result<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|path| path.join(".local/share"))
        })
        .map(|path| path.join("carbon-desktop-gpui"))
        .context(
            "A local application data directory is unavailable for the development fixture root",
        )
}

/// Ordinary roots that own a player's persistent library. A fixture seed must
/// never replace, descend into or become an ancestor of one of these.
fn ordinary_roots() -> Result<Vec<PathBuf>> {
    let home = data_home()?;
    [
        home.join("library-v1"),
        home.parent()
            .context("The local application data directory has no parent")?
            .join("dev.carbonmine.desktop.review")
            .join("fixture-library-v1"),
    ]
    .into_iter()
    .map(|path| resolve(&path))
    .collect()
}

fn absolute(path: &Path) -> Result<PathBuf> {
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        bail!(
            "--fixture-root must not contain '..'; use a dedicated absolute directory without parent traversal."
        );
    }
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .context("The current directory is unavailable")?
            .join(path))
    }
}

/// Resolve symlinks and junctions in the longest existing prefix while leaving
/// a not-yet-created dedicated suffix in the same parent namespace. The caller
/// must reject `..` first so lexical and filesystem resolution cannot diverge.
fn resolve(path: &Path) -> Result<PathBuf> {
    let absolute = absolute(path)?;
    let mut cursor = absolute.as_path();
    let mut suffix = Vec::new();
    loop {
        match fs::symlink_metadata(cursor) {
            Ok(_) => break,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                let name = cursor.file_name().with_context(|| {
                    format!(
                        "Could not resolve the fixture root {}; a filesystem root has no existing prefix.",
                        absolute.display()
                    )
                })?;
                suffix.push(name.to_os_string());
                cursor = cursor.parent().with_context(|| {
                    format!("Could not resolve the fixture root {}", absolute.display())
                })?;
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("Could not inspect the fixture root {}", absolute.display())
                });
            }
        }
    }
    let mut resolved = fs::canonicalize(cursor)
        .with_context(|| format!("Could not resolve the fixture root {}", absolute.display()))?;
    for part in suffix.into_iter().rev() {
        resolved.push(part);
    }
    Ok(resolved)
}

/// Whether the root path itself is a link or reparse point. Links in existing
/// parent components are resolved before the ownership comparison instead of
/// being refused: macOS reaches ordinary temporary directories through a
/// `/var` symlink, and the resolved path is what a reset would operate on.
fn linked_root(path: &Path) -> Result<bool> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(false);
    };
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Ok(true);
        }
    }
    Ok(metadata.is_symlink())
}

pub(crate) struct Guarded {
    pub(crate) path: PathBuf,
    linked: bool,
}

impl Guarded {
    /// A reset deletes a tree, so refuse a root that is itself a link or
    /// reparse point even when it resolves outside the ordinary library.
    pub(crate) fn allow_reset(&self) -> Result<()> {
        if self.linked {
            bail!(
                "Refusing to reset {} because the root is a symlink or reparse point. Choose a plain dedicated directory.",
                self.path.display()
            );
        }
        Ok(())
    }
}

/// Refuse a normal per-user library, one of its ancestors, a path inside it, a
/// filesystem root, a relative path and any unresolved `..` component. Existing
/// symlinks and junctions are resolved before the comparison, so an alternate
/// spelling cannot reach the ordinary library.
pub(crate) fn guard(root: &Path) -> Result<Guarded> {
    if !root.is_absolute() {
        bail!("--fixture-root must be an absolute path so its owner is unambiguous.");
    }
    let supplied = absolute(root)?;
    let linked = linked_root(&supplied)?;
    let root = resolve(&supplied)?;
    if root.parent().is_none() {
        bail!("--fixture-root must name a dedicated directory, not a filesystem root.");
    }
    for ordinary in ordinary_roots()? {
        if root == ordinary || root.starts_with(&ordinary) || ordinary.starts_with(&root) {
            bail!(
                "Refusing to use a normal per-user data root for development fixtures: {}. Choose a dedicated directory such as {}.",
                root.display(),
                default_root(Set::Default)?.display()
            );
        }
    }
    Ok(Guarded { path: root, linked })
}

fn read_marker(root: &Path) -> Result<Option<Marker>> {
    let path = root.join(MARKER_FILE);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| {
                format!("Could not inspect the fixture marker at {}", path.display())
            });
        }
    };
    if !metadata.is_file() || metadata.is_symlink() || metadata.len() > MAX_MARKER_BYTES {
        bail!(
            "The development fixture marker at {} is not a small regular file. Preserve the directory.",
            path.display()
        );
    }
    let bytes = fs::read(&path)
        .with_context(|| format!("Could not read the fixture marker at {}", path.display()))?;
    let marker: Marker = serde_json::from_slice(&bytes).with_context(|| {
        format!(
            "The development fixture marker at {} is unreadable. Preserve the directory.",
            path.display()
        )
    })?;
    marker.validate()?;
    Ok(Some(marker))
}

#[derive(Deserialize)]
struct ResetHeader {
    version: u64,
    #[allow(dead_code)]
    set: String,
}

/// Authorize a reset for an existing fixture marker even when a newer build
/// wrote fields this binary does not know. The file name and location are
/// specific, and the header still has to name a version and a set.
fn read_reset_header(root: &Path) -> Result<Option<ResetHeader>> {
    let path = root.join(MARKER_FILE);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| {
                format!("Could not inspect the fixture marker at {}", path.display())
            });
        }
    };
    if !metadata.is_file() || metadata.is_symlink() || metadata.len() > MAX_MARKER_BYTES {
        bail!(
            "The development fixture marker at {} is not a small regular file. Preserve the directory.",
            path.display()
        );
    }
    let bytes = fs::read(&path)
        .with_context(|| format!("Could not read the fixture marker at {}", path.display()))?;
    let header: ResetHeader = serde_json::from_slice(&bytes).with_context(|| {
        format!(
            "The development fixture marker at {} is unreadable. Preserve the directory.",
            path.display()
        )
    })?;
    if header.version == 0 || header.set.is_empty() {
        bail!(
            "The development fixture marker at {} is not a recognizable seed record. Preserve the directory.",
            path.display()
        );
    }
    Ok(Some(header))
}

/// Resolve a root that a previous command already seeded. A missing marker and
/// an incomplete seed are different user errors and both require an explicit
/// seed command; neither silently opens an unseeded library.
pub(crate) fn reopen_set(root: &Path) -> Result<Set> {
    let root = guard(root)?.path;
    match read_marker(&root)? {
        Some(marker) if marker.complete => Ok(marker.set),
        Some(_) => bail!(
            "{} contains an incomplete development fixture seed. Run --seed-fixtures <set> --fixture-root {} --reset-fixtures.",
            root.display(),
            root.display()
        ),
        None => bail!(
            "No development fixture marker at {}. Run --seed-fixtures <default|empty|large|faults> --fixture-root {} first.",
            root.display(),
            root.display()
        ),
    }
}

/// Move the root aside before deleting it so a failed recursive delete cannot
/// strand the marker inside a half-removed tree. A retry can seed the now-empty
/// requested path; the quarantine stays clearly named for manual cleanup.
fn remove_seeded_root(root: &Path) -> Result<()> {
    let parent = root
        .parent()
        .context("The fixture root has no parent directory")?;
    let name = root
        .file_name()
        .context("The fixture root has no directory name")?;
    let quarantine = parent.join(format!(
        ".{}-reset-{}",
        name.to_string_lossy(),
        Uuid::new_v4().simple()
    ));
    fs::rename(root, &quarantine)
        .with_context(|| format!("Could not move the fixture root {}", root.display()))?;
    if let Err(error) = fs::remove_dir_all(&quarantine) {
        bail!(
            "The fixture root was moved to {} but cleanup failed: {error}. The requested root is clear; delete that quarantine directory and retry.",
            quarantine.display()
        );
    }
    Ok(())
}

/// Seed a set if its root is absent, empty or already carries this set's
/// marker. A different set, an unmarked non-empty directory and a failed
/// partial seed are refused instead of being overwritten. The returned path is
/// the resolved root used for every subsequent read and write.
pub(crate) fn prepare(root: &Path, set: Set, reset: bool) -> Result<(PathBuf, Marker)> {
    let guarded = guard(root)?;
    let root = guarded.path.clone();
    if reset {
        if root.exists() {
            guarded.allow_reset()?;
            match read_reset_header(&root)? {
                Some(_) => remove_seeded_root(&root)?,
                None => bail!(
                    "Refusing to reset {}. It has no development fixture marker, so it may contain user data.",
                    root.display()
                ),
            }
        }
    } else if root.exists() {
        if let Some(marker) = read_marker(&root)? {
            if !marker.complete {
                bail!(
                    "{} contains an incomplete development fixture seed. Use --reset-fixtures to retry it.",
                    root.display()
                );
            }
            if marker.set == set {
                return Ok((root, marker));
            }
            bail!(
                "{} already contains the '{}' fixture set. Use --reset-fixtures to replace it with '{}'.",
                root.display(),
                marker.set.as_str(),
                set.as_str()
            );
        }
        let mut entries = fs::read_dir(&root)
            .with_context(|| format!("Could not inspect the fixture root {}", root.display()))?;
        if entries.next().is_some() {
            bail!(
                "Refusing to replace a non-empty directory without a development fixture marker: {}.",
                root.display()
            );
        }
    }

    fs::create_dir_all(&root)
        .with_context(|| format!("Could not create the fixture root {}", root.display()))?;
    // Publish an incomplete marker before the ordinary stores create anything,
    // so a failed seed is distinguishable from an unowned directory and can be
    // replaced explicitly with --reset-fixtures.
    shell_write(
        &root,
        &Marker {
            version: MARKER_VERSION,
            set,
            rows: Vec::new(),
            store_instances: 0,
            complete: false,
        },
    )?;
    let mut marker = match set {
        Set::Default => seed_default(&root)?,
        Set::Empty => seed_empty(&root)?,
        Set::Large => seed_large(&root)?,
        Set::Faults => seed_faults(&root)?,
    };
    marker.complete = true;
    shell_write(&root, &marker)?;
    Ok((root, marker))
}

fn shell_write(root: &Path, marker: &Marker) -> Result<()> {
    let mut staged = tempfile::NamedTempFile::new_in(root)
        .context("Could not stage the development fixture marker")?;
    serde_json::to_writer(&mut staged, marker).context("Could not encode the fixture marker")?;
    std::io::Write::flush(&mut staged).context("Could not flush the fixture marker")?;
    staged
        .persist(root.join(MARKER_FILE))
        .map_err(|error| error.error)
        .context("Could not publish the development fixture marker")?;
    Ok(())
}

fn seed_empty(root: &Path) -> Result<Marker> {
    // Initialize both ordinary stores so a tester sees an empty library rather
    // than a missing directory, and a second seed remains a no-op.
    carbon_profiles::list(root).map_err(anyhow::Error::msg)?;
    carbon_store::snapshot(root).map_err(anyhow::Error::msg)?;
    Ok(Marker {
        version: MARKER_VERSION,
        set: Set::Empty,
        rows: Vec::new(),
        store_instances: 0,
        complete: true,
    })
}

fn seed_large(root: &Path) -> Result<Marker> {
    // Reuse the established comparison sample: 100 stable IDs, the same names
    // and the same five rotating version labels as the other frontend.
    carbon_profiles::seed_review_library(root, true).map_err(anyhow::Error::msg)?;
    carbon_store::apply(root, carbon_store::Action::CreateSample).map_err(anyhow::Error::msg)?;
    Ok(Marker {
        version: MARKER_VERSION,
        set: Set::Large,
        rows: vec!["comparison-100".into()],
        store_instances: 1,
        complete: true,
    })
}

fn seed_faults(root: &Path) -> Result<Marker> {
    use carbon_store::{Action, InstanceId};
    carbon_profiles::list(root).map_err(anyhow::Error::msg)?;
    for action in [
        Action::CreateSample,
        Action::SimulateMissing {
            instance: InstanceId::Sample,
        },
    ] {
        carbon_store::apply(root, action).map_err(anyhow::Error::msg)?;
    }
    Ok(Marker {
        version: MARKER_VERSION,
        set: Set::Faults,
        rows: vec!["forest-missing-object".into()],
        store_instances: 1,
        complete: true,
    })
}

fn seed_default(root: &Path) -> Result<Marker> {
    let mut ids = std::collections::BTreeMap::new();
    for row in DEFAULT_PROFILES {
        let id = create_profile(root, row)?;
        ids.insert(row.key, id);
    }
    for row in DEFAULT_PROFILES {
        let id = ids[row.key];
        finish_profile(root, row, id)?;
    }
    seed_store(root)?;
    seed_home(root, &ids)?;
    Ok(Marker {
        version: MARKER_VERSION,
        set: Set::Default,
        rows: DEFAULT_PROFILES
            .iter()
            .map(|row| row.key.to_owned())
            .collect(),
        store_instances: 3,
        complete: true,
    })
}

fn create_profile(root: &Path, row: &ProfileRow) -> Result<Uuid> {
    let id = match &row.pack {
        Some(pack) => {
            let mut profile = import_instance(
                root,
                Uuid::new_v4(),
                row.version,
                ImportedPack {
                    name: row.name.to_owned(),
                    loader: pack.loader.map(str::to_owned),
                    pending_files: pack.pending_files,
                },
                |_| Ok(()),
            )
            .map_err(anyhow::Error::msg)?;
            // Import sets its own defaults. Apply the catalog row through the
            // ordinary edit operation, which preserves the pack identity.
            profile.memory_mib = row.memory_mib;
            profile.window_width = row.window.0;
            profile.window_height = row.window.1;
            change(root, ProfileAction::Edit(profile.clone())).map_err(anyhow::Error::msg)?;
            profile.id
        }
        None => {
            let profiles = change(
                root,
                ProfileAction::Create {
                    name: row.name.to_owned(),
                    version: row.version,
                    memory_mib: row.memory_mib,
                    window_width: row.window.0,
                    window_height: row.window.1,
                },
            )
            .map_err(anyhow::Error::msg)?;
            profiles
                .last()
                .context("The ordinary profile store did not publish the new fixture row")?
                .id
        }
    };
    Ok(id)
}

fn finish_profile(root: &Path, row: &ProfileRow, id: Uuid) -> Result<()> {
    if let Some(icon) = row.icon {
        icons::set_icon(root, id, None, Some(icon)).map_err(anyhow::Error::msg)?;
    }
    if let Some(server) = row.server {
        let address = ServerAddress::parse(server).map_err(anyhow::Error::msg)?;
        set_server_address(root, id, None, Some(address)).map_err(anyhow::Error::msg)?;
    }
    for world in row
        .worlds
        .iter()
        .chain(row.missing_worlds.iter().map(|(name, _)| name))
    {
        write_world(root, id, world)?;
    }
    // A dedicated-server instance deliberately owns its saved address instead
    // of the launch-target inventory; the ordinary target reader refuses it.
    if row.server.is_some() {
        return Ok(());
    }
    let mut snapshot = instance_targets(root, id).map_err(anyhow::Error::msg)?;
    for (name, address) in row.destinations {
        let address = ServerAddress::parse(address).map_err(anyhow::Error::msg)?;
        snapshot = change_instance_targets(
            root,
            id,
            snapshot.revision,
            TargetAction::SaveDestination {
                id: None,
                name: (*name).to_owned(),
                address,
            },
        )
        .map_err(anyhow::Error::msg)?;
    }
    for (world, label) in row.missing_worlds {
        snapshot = change_instance_targets(
            root,
            id,
            snapshot.revision,
            TargetAction::LabelWorld {
                directory: (*world).to_owned(),
                label: Some((*label).to_owned()),
            },
        )
        .map_err(anyhow::Error::msg)?;
    }
    if let Some(selection) = selection_key(row, &snapshot) {
        change_instance_targets(
            root,
            id,
            snapshot.revision,
            TargetAction::Select(Some(selection)),
        )
        .map_err(anyhow::Error::msg)?;
    }
    // Remove only after the label is saved so the stale-label fixture is the
    // ordinary "preference remains, folder is gone" state the UI reports.
    for (world, _) in row.missing_worlds {
        let path = save_root(root, id).join(world);
        fs::remove_dir_all(&path)
            .with_context(|| format!("Could not retire the labeled save {}", path.display()))?;
    }
    Ok(())
}

fn selection_key(
    row: &ProfileRow,
    snapshot: &carbon_profiles::InstanceTargets,
) -> Option<TargetKey> {
    match row.select {
        Selection::None => None,
        Selection::World(world) => snapshot
            .worlds
            .iter()
            .find(|candidate| matches!(&candidate.key, TargetKey::World(name) if name == world))
            .map(|candidate| candidate.key.clone()),
        Selection::Server(name) => snapshot
            .destinations
            .iter()
            .find(|candidate| candidate.name == name)
            .map(|candidate| TargetKey::Server(candidate.id)),
    }
}

fn save_root(root: &Path, id: Uuid) -> PathBuf {
    root.join("instances-v1")
        .join(id.to_string())
        .join("game")
        .join("saves")
}

fn write_world(root: &Path, id: Uuid, name: &str) -> Result<()> {
    let directory = save_root(root, id).join(name);
    fs::create_dir_all(&directory)
        .with_context(|| format!("Could not create the fixture save {}", directory.display()))?;
    fs::write(directory.join("level.dat"), WORLD_MARKER)
        .with_context(|| format!("Could not mark the fixture save {}", directory.display()))?;
    Ok(())
}

fn seed_store(root: &Path) -> Result<()> {
    use carbon_store::{Action, InstanceId};
    for action in [
        Action::CreateSample,
        Action::Review {
            instance: InstanceId::Sample,
        },
        Action::Interrupt {
            instance: InstanceId::Sample,
        },
        Action::CreateRepeat,
        Action::CreateOverlap,
        Action::Review {
            instance: InstanceId::Overlap,
        },
        Action::Stage {
            instance: InstanceId::Overlap,
        },
    ] {
        carbon_store::apply(root, action).map_err(anyhow::Error::msg)?;
    }
    Ok(())
}

fn seed_home(root: &Path, ids: &std::collections::BTreeMap<&'static str, Uuid>) -> Result<()> {
    let mut order = HomeOrder::default();
    for key in DEFAULT_HOME.pinned {
        let id = ids
            .get(key)
            .with_context(|| format!("Home pin references an unknown fixture row '{key}'"))?;
        order.toggle_pin(*id);
    }
    // touch_recent pushes to the front, so record the oldest desired entry first.
    for key in DEFAULT_HOME.recent.iter().rev() {
        let id = ids.get(key).with_context(|| {
            format!("Home recent entry references an unknown fixture row '{key}'")
        })?;
        order.touch_recent(*id);
    }
    order
        .save(root)
        .with_context(|| format!("Could not save Home ordering in {}", root.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("default");
        (temp, root)
    }

    #[test]
    fn default_rows_validate_through_the_ordinary_stores() {
        let (_temp, root) = temp_root();
        let (_, marker) = prepare(&root, Set::Default, false).unwrap();
        assert_eq!(marker.rows.len(), DEFAULT_PROFILES.len());
        assert_eq!(marker.store_instances, 3);
        let profiles = carbon_profiles::list(&root).unwrap();
        assert_eq!(profiles.len(), DEFAULT_PROFILES.len());
        let alex: Vec<_> = profiles
            .iter()
            .filter(|profile| profile.name == "Alex")
            .collect();
        assert_eq!(alex.len(), 2);
        assert!(
            profiles
                .iter()
                .any(|profile| profile.name == "苔むす森・夜間採集")
        );
        assert!(
            profiles
                .iter()
                .any(|profile| profile.server_address.is_some())
        );
        assert!(profiles.iter().any(|profile| {
            profile.imported_pack.as_ref().is_some_and(|pack| {
                pack.loader.as_deref() == Some("fabric") && pack.pending_files == 3
            })
        }));
        assert!(profiles.iter().any(|profile| {
            profile
                .imported_pack
                .as_ref()
                .is_some_and(|pack| pack.loader.is_none() && pack.pending_files == 0)
        }));
        for row in DEFAULT_PROFILES {
            let profile = profiles
                .iter()
                .find(|profile| {
                    let pack_matches = match (&profile.imported_pack, &row.pack) {
                        (None, None) => true,
                        (Some(profile), Some(row)) => {
                            profile.loader.as_deref() == row.loader
                                && profile.pending_files == row.pending_files
                        }
                        _ => false,
                    };
                    profile.name == row.name && profile.version == row.version && pack_matches
                })
                .unwrap_or_else(|| panic!("fixture row {} was not persisted", row.key));
            assert_eq!(profile.memory_mib, row.memory_mib, "{}", row.key);
            assert_eq!(
                (profile.window_width, profile.window_height),
                row.window,
                "{}",
                row.key
            );
        }
        let mixed = profiles
            .iter()
            .find(|profile| profile.name == "Targets · mixed")
            .unwrap();
        let targets = carbon_profiles::instance_targets(&root, mixed.id).unwrap();
        assert_eq!(targets.worlds.len(), 2);
        assert_eq!(targets.destinations.len(), 2);
        assert!(matches!(targets.selected, Some(TargetKey::Server(_))));
        let stale = profiles
            .iter()
            .find(|profile| profile.name == "Targets · stale label")
            .unwrap();
        let targets = carbon_profiles::instance_targets(&root, stale.id).unwrap();
        assert_eq!(targets.missing_labels.len(), 1);
        let store = carbon_store::snapshot(&root).unwrap();
        assert_eq!(store.instances.len(), 3);
        assert!(
            store
                .instances
                .iter()
                .any(|item| item.phase == carbon_store::Phase::Interrupted)
        );
        assert!(
            store
                .instances
                .iter()
                .any(|item| item.phase == carbon_store::Phase::Staged)
        );
        assert!(store.instances.iter().all(|item| item.healthy));
        let order: serde_json::Value = serde_json::from_slice(
            &fs::read(root.join("home-order.json")).expect("Home order file"),
        )
        .expect("Home order JSON");
        assert_eq!(order["pinned"].as_array().unwrap().len(), 2);
        assert_eq!(order["recent"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn reseeding_the_same_set_is_a_no_op_and_a_different_set_is_refused() {
        let (_temp, root) = temp_root();
        let (_, first) = prepare(&root, Set::Default, false).unwrap();
        let profiles = carbon_profiles::list(&root).unwrap();
        let (_, second) = prepare(&root, Set::Default, false).unwrap();
        assert_eq!(first.rows, second.rows);
        assert_eq!(reopen_set(&root).unwrap(), Set::Default);
        assert_eq!(profiles, carbon_profiles::list(&root).unwrap());
        let error = prepare(&root, Set::Empty, false).unwrap_err().to_string();
        assert!(error.contains("--reset-fixtures"));
    }

    #[test]
    fn reset_requires_the_marker_and_then_rebuilds_the_named_set() {
        let (_temp, root) = temp_root();
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("personal-note.txt"), b"unowned").unwrap();
        assert!(prepare(&root, Set::Empty, false).is_err());
        assert!(prepare(&root, Set::Empty, true).is_err());
        assert_eq!(
            fs::read(root.join("personal-note.txt")).unwrap(),
            b"unowned"
        );
        fs::remove_file(root.join("personal-note.txt")).unwrap();

        prepare(&root, Set::Default, false).unwrap();
        shell_write(
            &root,
            &Marker {
                version: MARKER_VERSION,
                set: Set::Default,
                rows: Vec::new(),
                store_instances: 0,
                complete: false,
            },
        )
        .unwrap();
        assert!(prepare(&root, Set::Default, false).is_err());
        assert!(reopen_set(&root).is_err());
        prepare(&root, Set::Empty, true).unwrap();
        assert!(carbon_profiles::list(&root).unwrap().is_empty());
        let marker = read_marker(&root).unwrap().unwrap();
        assert_eq!(marker.set, Set::Empty);

        fs::write(
            root.join(MARKER_FILE),
            br#"{"version":999,"set":"default","future":true}"#,
        )
        .unwrap();
        prepare(&root, Set::Empty, true).unwrap();
        assert!(carbon_profiles::list(&root).unwrap().is_empty());
    }

    #[test]
    fn empty_and_large_sets_are_deterministic_and_repeatable() {
        let (_temp, empty_root) = temp_root();
        assert!(
            prepare(&empty_root, Set::Empty, false)
                .unwrap()
                .1
                .rows
                .is_empty()
        );
        assert!(carbon_profiles::list(&empty_root).unwrap().is_empty());
        assert!(
            carbon_store::snapshot(&empty_root)
                .unwrap()
                .instances
                .is_empty()
        );

        let temp = tempfile::tempdir().unwrap();
        let large_root = temp.path().join("large");
        prepare(&large_root, Set::Large, false).unwrap();
        let profiles = carbon_profiles::list(&large_root).unwrap();
        assert_eq!(profiles.len(), 100);
        assert_eq!(profiles[0].name, "Vanilla survival");
        assert_eq!(
            profiles[0].id,
            Uuid::from_u128(0xb79dbad7_d038_46ce_b5a4_000000000000)
        );
        assert_eq!(profiles[99].name, "Review instance 099");
        assert_eq!(profiles[99].version.id(), "24w14a");
        prepare(&large_root, Set::Large, false).unwrap();
        assert_eq!(profiles, carbon_profiles::list(&large_root).unwrap());

        let fault_root = temp.path().join("faults");
        prepare(&fault_root, Set::Faults, false).unwrap();
        assert!(carbon_profiles::list(&fault_root).unwrap().is_empty());
        let store = carbon_store::snapshot(&fault_root).unwrap();
        assert_eq!(store.instances.len(), 1);
        assert!(!store.instances[0].healthy);
    }

    #[test]
    fn ordinary_roots_and_their_ancestors_are_refused() {
        let ordinary = ordinary_roots().unwrap();
        for root in &ordinary {
            assert!(guard(root).is_err());
            assert!(guard(root.parent().unwrap()).is_err());
            assert!(guard(&root.join("nested")).is_err());
        }
        assert!(guard(Path::new("relative-fixture")).is_err());
        assert!(
            guard(
                &std::env::current_dir()
                    .unwrap()
                    .join("..")
                    .join("fixture-with-parent")
            )
            .is_err()
        );
        assert!(reopen_set(&ordinary[0].join("missing")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_link_above_the_root_is_resolved_and_reset_still_works() {
        let temp = tempfile::tempdir().unwrap();
        let real = temp.path().join("real");
        fs::create_dir_all(&real).unwrap();
        let alias = temp.path().join("alias");
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        let root = alias.join("default");
        // macOS temp roots live below /private/var reached through /var.
        let expected = fs::canonicalize(&real).unwrap().join("default");
        let (resolved, _) = prepare(&root, Set::Empty, false).unwrap();
        assert_eq!(resolved, expected);
        assert!(resolved.exists());
        // Reset replaces the tree and re-seeds the named set through the alias.
        prepare(&root, Set::Empty, true).unwrap();
        assert!(expected.exists());
        let marker = read_marker(&expected).unwrap().unwrap();
        assert_eq!(marker.set, Set::Empty);
        assert!(marker.complete);
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_root_itself_is_refused_for_reset() {
        let temp = tempfile::tempdir().unwrap();
        let real = temp.path().join("real");
        fs::create_dir_all(&real).unwrap();
        prepare(&real, Set::Empty, false).unwrap();
        let alias = temp.path().join("alias");
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        let error = prepare(&alias, Set::Empty, true).unwrap_err().to_string();
        assert!(error.contains("symlink"), "{error}");
        assert!(read_marker(&real).unwrap().is_some());
    }

    #[test]
    fn every_row_is_reachable_by_its_key_and_a_long_name_stays_valid() {
        let keys: std::collections::HashSet<_> =
            DEFAULT_PROFILES.iter().map(|row| row.key).collect();
        assert_eq!(keys.len(), DEFAULT_PROFILES.len());
        for row in DEFAULT_PROFILES {
            assert!(row.name.chars().count() <= 80);
            assert_eq!(row.name.trim(), row.name);
        }
        for key in DEFAULT_HOME.pinned.iter().chain(DEFAULT_HOME.recent) {
            assert!(keys.contains(key));
        }
    }
}
