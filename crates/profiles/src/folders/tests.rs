/* SPDX-License-Identifier: MPL-2.0 */
use super::*;
use crate::{change, list, seed_review_library, ProfileAction, VanillaVersion};
use std::fs;
use std::path::{Path, PathBuf};

fn create(root: &Path, name: &str) -> Uuid {
    change(
        root,
        ProfileAction::Create {
            name: name.to_owned(),
            version: VanillaVersion::Release26_3,
            memory_mib: 2048,
            window_width: 1280,
            window_height: 720,
        },
    )
    .unwrap()
    .last()
    .unwrap()
    .id
}

fn folder_file(root: &Path) -> PathBuf {
    root.join("personal").join(FOLDERS)
}

fn profiles_file(root: &Path) -> PathBuf {
    root.join("personal/profiles.json")
}

fn save_root(root: &Path, id: Uuid) -> PathBuf {
    root.join("instances-v1")
        .join(id.to_string())
        .join("game/saves")
}

fn add_world(saves: &Path, name: &str) {
    let world = saves.join(name);
    fs::create_dir_all(&world).unwrap();
    fs::write(world.join("level.dat"), b"synthetic level").unwrap();
}

fn instance(id: Uuid) -> FolderMember {
    FolderMember::Instance { id }
}

fn world(instance: Uuid, directory: &str) -> FolderMember {
    FolderMember::World {
        instance,
        directory: directory.to_owned(),
    }
}

fn create_folder(
    root: &Path,
    revision: Option<Uuid>,
    scope: FolderScope,
    label: &str,
) -> FolderSnapshot {
    change_folders(
        root,
        revision,
        FolderAction::Create {
            scope,
            label: label.to_owned(),
        },
    )
    .unwrap()
}

fn folder_id(snapshot: &FolderSnapshot, label: &str, scope: FolderScope) -> Uuid {
    snapshot
        .folders
        .iter()
        .find(|folder| folder.label == label && folder.scope == scope)
        .unwrap()
        .id
}

fn member_resolutions(snapshot: &FolderSnapshot, id: Uuid) -> Vec<MemberResolution> {
    snapshot
        .folders
        .iter()
        .find(|folder| folder.id == id)
        .unwrap()
        .members
        .iter()
        .map(|entry| entry.resolution)
        .collect()
}

#[test]
fn empty_library_starts_flat_and_creates_the_folder_file_only_on_write() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    assert_eq!(
        folders(&root).unwrap(),
        FolderSnapshot {
            revision: None,
            folders: Vec::new(),
        }
    );
    assert!(!folder_file(&root).exists());

    let first = create_folder(&root, None, FolderScope::Instances, "Favorites");
    assert_eq!(first.folders.len(), 1);
    assert!(first.revision.is_some());
    assert!(folder_file(&root).exists());
    let second = create_folder(&root, first.revision, FolderScope::Worlds, "Survival");
    assert_eq!(second.folders.len(), 2);
    // A restart-equivalent read returns the exact folders and order.
    assert_eq!(folders(&root).unwrap(), second);
}

#[test]
fn saved_folder_json_uses_stable_typed_kinds_and_scopes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let id = create(&root, "Typed");
    add_world(&save_root(&root, id), "Home");
    let snapshot = create_folder(&root, None, FolderScope::Instances, "Local");
    let local = folder_id(&snapshot, "Local", FolderScope::Instances);
    let world_folder = create_folder(&root, snapshot.revision, FolderScope::Worlds, "Adventure");
    let adventure = folder_id(&world_folder, "Adventure", FolderScope::Worlds);
    let filled = change_folders(
        &root,
        world_folder.revision,
        FolderAction::File {
            id: local,
            members: vec![instance(id)],
        },
    )
    .unwrap();
    let filled = change_folders(
        &root,
        filled.revision,
        FolderAction::File {
            id: adventure,
            members: vec![world(id, "Home")],
        },
    )
    .unwrap();

    let stored: serde_json::Value =
        serde_json::from_slice(&fs::read(folder_file(&root)).unwrap()).unwrap();
    assert_eq!(stored["version"], 1);
    assert!(stored["revision"].is_string());
    let folders = stored["folders"].as_array().unwrap();
    assert_eq!(folders.len(), 2);
    let local_json = folders
        .iter()
        .find(|folder| folder["label"] == "Local")
        .unwrap();
    assert_eq!(local_json["scope"], "instances");
    assert_eq!(local_json["members"][0]["kind"], "instance");
    assert_eq!(local_json["members"][0]["id"], id.to_string());
    let adventure_json = folders
        .iter()
        .find(|folder| folder["label"] == "Adventure")
        .unwrap();
    assert_eq!(adventure_json["scope"], "worlds");
    assert_eq!(adventure_json["members"][0]["kind"], "world");
    assert_eq!(adventure_json["members"][0]["instance"], id.to_string());
    assert_eq!(adventure_json["members"][0]["directory"], "Home");
    let adventure_view = filled
        .folders
        .iter()
        .find(|folder| folder.id == adventure)
        .unwrap();
    assert_eq!(
        serde_json::to_value(adventure_view.members[0].resolution).unwrap(),
        "resolved"
    );
}

#[test]
fn rename_move_and_remove_only_change_the_folder_file_and_never_instance_data() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let first = create(&root, "Same name");
    let second = create(&root, "Same name");
    let third = create(&root, "Other");
    add_world(&save_root(&root, first), "Cave");
    let canary = save_root(&root, first).join("Cave/level.dat");
    let canary_before = fs::read(&canary).unwrap();
    let profiles_before = fs::read(profiles_file(&root)).unwrap();

    let snapshot = create_folder(&root, None, FolderScope::Instances, "Weekend");
    let weekend = folder_id(&snapshot, "Weekend", FolderScope::Instances);
    let filled = change_folders(
        &root,
        snapshot.revision,
        FolderAction::File {
            id: weekend,
            members: vec![instance(first), instance(second), instance(third)],
        },
    )
    .unwrap();
    assert_eq!(
        member_resolutions(&filled, weekend),
        vec![MemberResolution::Resolved; 3]
    );
    let renamed = change_folders(
        &root,
        filled.revision,
        FolderAction::Rename {
            id: weekend,
            label: "Weeknights".into(),
        },
    )
    .unwrap();
    assert_eq!(renamed.folders[0].label, "Weeknights");
    assert_eq!(renamed.folders[0].members.len(), 3);

    let second_folder = create_folder(&root, renamed.revision, FolderScope::Instances, "Archive");
    let archive = folder_id(&second_folder, "Archive", FolderScope::Instances);
    let reordered = change_folders(
        &root,
        second_folder.revision,
        FolderAction::Reorder {
            id: archive,
            index: 0,
        },
    )
    .unwrap();
    assert_eq!(
        reordered
            .folders
            .iter()
            .map(|folder| folder.label.as_str())
            .collect::<Vec<_>>(),
        vec!["Archive", "Weeknights"]
    );

    let removed = change_folders(
        &root,
        reordered.revision,
        FolderAction::Remove { id: weekend },
    )
    .unwrap();
    assert_eq!(removed.folders.len(), 1);
    assert_eq!(removed.folders[0].label, "Archive");
    assert_eq!(list(&root).unwrap().len(), 3);
    assert_eq!(fs::read(profiles_file(&root)).unwrap(), profiles_before);
    assert_eq!(fs::read(&canary).unwrap(), canary_before);
    assert_eq!(fs::read_dir(save_root(&root, first)).unwrap().count(), 1);
}

#[test]
fn duplicate_labels_are_case_insensitive_within_a_scope_only() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let snapshot = create_folder(&root, None, FolderScope::Instances, "Favorites");
    assert!(change_folders(
        &root,
        snapshot.revision,
        FolderAction::Create {
            scope: FolderScope::Instances,
            label: "favorites".into(),
        }
    )
    .is_err());
    let both = create_folder(&root, snapshot.revision, FolderScope::Worlds, "Favorites");
    assert_eq!(both.folders.len(), 2);
    let other = create_folder(&root, both.revision, FolderScope::Worlds, "Other");
    assert!(change_folders(
        &root,
        other.revision,
        FolderAction::Rename {
            id: folder_id(&other, "Other", FolderScope::Worlds),
            label: "favorites".into(),
        }
    )
    .is_err());
    // The refused writes left the saved revision unchanged.
    assert_eq!(folders(&root).unwrap(), other);
}

#[test]
fn world_members_use_instance_and_directory_and_move_between_folders() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let id = create(&root, "Local saves");
    add_world(&save_root(&root, id), "Home");
    let home = world(id, "Home");
    let snapshot = create_folder(&root, None, FolderScope::Worlds, "Survival");
    let survival = folder_id(&snapshot, "Survival", FolderScope::Worlds);
    let filled = change_folders(
        &root,
        snapshot.revision,
        FolderAction::File {
            id: survival,
            members: vec![home.clone()],
        },
    )
    .unwrap();
    assert_eq!(
        member_resolutions(&filled, survival),
        vec![MemberResolution::Resolved]
    );

    let other = create_folder(&root, filled.revision, FolderScope::Worlds, "Shared");
    let shared = folder_id(&other, "Shared", FolderScope::Worlds);
    let moved = change_folders(
        &root,
        other.revision,
        FolderAction::File {
            id: shared,
            members: vec![home.clone()],
        },
    )
    .unwrap();
    assert!(moved
        .folders
        .iter()
        .find(|folder| folder.id == survival)
        .unwrap()
        .members
        .is_empty());
    assert_eq!(
        moved
            .folders
            .iter()
            .find(|folder| folder.id == shared)
            .unwrap()
            .members
            .len(),
        1
    );

    let unfiled = change_folders(
        &root,
        moved.revision,
        FolderAction::Unfile {
            id: shared,
            members: vec![home],
        },
    )
    .unwrap();
    assert!(unfiled
        .folders
        .iter()
        .find(|folder| folder.id == shared)
        .unwrap()
        .members
        .is_empty());

    // Instance folders reject world references and vice versa.
    let instances = create_folder(&root, unfiled.revision, FolderScope::Instances, "All");
    let all = folder_id(&instances, "All", FolderScope::Instances);
    assert!(change_folders(
        &root,
        instances.revision,
        FolderAction::File {
            id: all,
            members: vec![world(id, "Home")],
        }
    )
    .is_err());
    assert!(change_folders(
        &root,
        instances.revision,
        FolderAction::File {
            id: survival,
            members: vec![instance(id)],
        }
    )
    .is_err());
}

#[test]
fn missing_instances_stay_referenced_until_an_explicit_prune() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    seed_review_library(&root, true).unwrap();
    let removed = list(&root).unwrap()[0].id;
    let snapshot = create_folder(&root, None, FolderScope::Instances, "Old");
    let old = folder_id(&snapshot, "Old", FolderScope::Instances);
    let filled = change_folders(
        &root,
        snapshot.revision,
        FolderAction::File {
            id: old,
            members: vec![instance(removed)],
        },
    )
    .unwrap();
    assert_eq!(
        member_resolutions(&filled, old),
        vec![MemberResolution::Resolved]
    );

    let path = profiles_file(&root);
    let mut library: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    library["profiles"]
        .as_array_mut()
        .unwrap()
        .retain(|profile| profile["id"] != serde_json::json!(removed.to_string()));
    fs::write(&path, serde_json::to_vec(&library).unwrap()).unwrap();

    let missing = folders(&root).unwrap();
    assert_eq!(
        member_resolutions(&missing, old),
        vec![MemberResolution::Missing]
    );
    // Another window files something else after the missing item was painted.
    let later = create_folder(&root, missing.revision, FolderScope::Instances, "Later");
    // A stale observation cannot prune a reference that changed since paint.
    assert!(change_folders(
        &root,
        missing.revision,
        FolderAction::PruneMissing {
            id: old,
            members: vec![instance(removed)],
        }
    )
    .is_err());
    assert_eq!(
        member_resolutions(&folders(&root).unwrap(), old),
        vec![MemberResolution::Missing]
    );
    let pruned = change_folders(
        &root,
        later.revision,
        FolderAction::PruneMissing {
            id: old,
            members: vec![instance(removed)],
        },
    )
    .unwrap();
    assert!(pruned.folders[0].members.is_empty());
    assert_eq!(list(&root).unwrap().len(), 99);
}

#[test]
fn world_references_survive_renames_and_only_a_complete_scan_can_prune() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let id = create(&root, "Saves");
    let saves = save_root(&root, id);
    add_world(&saves, "Home");
    let home = world(id, "Home");
    let snapshot = create_folder(&root, None, FolderScope::Worlds, "Survival");
    let survival = folder_id(&snapshot, "Survival", FolderScope::Worlds);
    change_folders(
        &root,
        snapshot.revision,
        FolderAction::File {
            id: survival,
            members: vec![home.clone()],
        },
    )
    .unwrap();

    // Renaming the physical directory is outside Carbon. The reference stays,
    // is reported missing, and is not silently dropped.
    fs::rename(saves.join("Home"), saves.join("Away")).unwrap();
    let missing = folders(&root).unwrap();
    assert_eq!(
        member_resolutions(&missing, survival),
        vec![MemberResolution::Missing]
    );
    assert!(change_folders(
        &root,
        missing.revision,
        FolderAction::PruneMissing {
            id: survival,
            members: vec![home.clone()],
        }
    )
    .is_ok());

    // Restoring the reference by re-filing it resolves again.
    let refiled = change_folders(
        &root,
        folders(&root).unwrap().revision,
        FolderAction::File {
            id: survival,
            members: vec![home.clone()],
        },
    );
    assert!(refiled.is_err(), "a missing world cannot be filed");

    fs::rename(saves.join("Away"), saves.join("Home")).unwrap();
    let restored = change_folders(
        &root,
        folders(&root).unwrap().revision,
        FolderAction::File {
            id: survival,
            members: vec![home.clone()],
        },
    )
    .unwrap();
    assert_eq!(
        member_resolutions(&restored, survival),
        vec![MemberResolution::Resolved]
    );

    // An unreadable saves path cannot prove the world is missing.
    fs::remove_dir_all(&saves).unwrap();
    fs::write(&saves, b"not a directory").unwrap();
    let unknown = folders(&root).unwrap();
    assert_eq!(
        member_resolutions(&unknown, survival),
        vec![MemberResolution::Unknown]
    );
    assert!(change_folders(
        &root,
        unknown.revision,
        FolderAction::PruneMissing {
            id: survival,
            members: vec![home],
        }
    )
    .is_err());
    assert_eq!(
        member_resolutions(&folders(&root).unwrap(), survival),
        vec![MemberResolution::Unknown]
    );
}

#[test]
fn stale_writes_and_untrusted_folder_files_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let id = create(&root, "One");
    let snapshot = create_folder(&root, None, FolderScope::Instances, "Kept");
    let kept = folder_id(&snapshot, "Kept", FolderScope::Instances);
    let renamed = change_folders(
        &root,
        snapshot.revision,
        FolderAction::Rename {
            id: kept,
            label: "Renamed".into(),
        },
    )
    .unwrap();
    assert!(change_folders(
        &root,
        snapshot.revision,
        FolderAction::Rename {
            id: kept,
            label: "Stale".into(),
        }
    )
    .is_err());
    assert_eq!(folders(&root).unwrap(), renamed);

    // Every untrusted shape preserves the saved bytes and reports an error.
    let good = fs::read(folder_file(&root)).unwrap();
    let cases = [
        b"not json".to_vec(),
        serde_json::to_vec(&serde_json::json!({
            "version": 2,
            "revision": null,
            "folders": []
        }))
        .unwrap(),
        serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "revision": null,
            "folders": [{
                "id": Uuid::new_v4().to_string(),
                "label": "Wrong scope",
                "scope": "worlds",
                "members": [{"kind": "instance", "id": id.to_string()}]
            }]
        }))
        .unwrap(),
        serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "revision": null,
            "folders": [{
                "id": Uuid::new_v4().to_string(),
                "label": "Extra member field",
                "scope": "instances",
                "members": [{"kind": "instance", "id": id.to_string(), "extra": 1}]
            }]
        }))
        .unwrap(),
        serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "revision": null,
            "folders": [{
                "id": Uuid::new_v4().to_string(),
                "label": "Mixed member shape",
                "scope": "worlds",
                "members": [{
                    "kind": "world",
                    "id": id.to_string(),
                    "instance": id.to_string(),
                    "directory": "Home"
                }]
            }]
        }))
        .unwrap(),
        serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "revision": null,
            "folders": [
                {"id": Uuid::new_v4().to_string(), "label": "Same", "scope": "instances", "members": []},
                {"id": Uuid::new_v4().to_string(), "label": "same", "scope": "instances", "members": []}
            ]
        }))
        .unwrap(),
        serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "revision": null,
            "folders": [
                {"id": Uuid::new_v4().to_string(), "label": "A", "scope": "instances", "members": [{"kind":"instance","id":id.to_string()}]},
                {"id": Uuid::new_v4().to_string(), "label": "B", "scope": "instances", "members": [{"kind":"instance","id":id.to_string()}]}
            ]
        }))
        .unwrap(),
        vec![b' '; MAX_FOLDERS_BYTES as usize + 1],
        serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "revision": null,
            "folders": [{
                "id": Uuid::new_v4().to_string(),
                "label": "Bad reference",
                "scope": "worlds",
                "members": [{"kind": "world", "instance": id.to_string(), "directory": "../Escape"}]
            }]
        }))
        .unwrap(),
    ];
    for bytes in cases {
        fs::write(folder_file(&root), &bytes).unwrap();
        assert!(folders(&root).is_err());
        assert!(change_folders(
            &root,
            renamed.revision,
            FolderAction::Create {
                scope: FolderScope::Instances,
                label: "New".into(),
            }
        )
        .is_err());
        assert_eq!(fs::read(folder_file(&root)).unwrap(), bytes);
    }
    fs::write(folder_file(&root), &good).unwrap();
    assert_eq!(folders(&root).unwrap(), renamed);
}

#[test]
fn long_synthetic_library_loads_with_all_references_resolved() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    seed_review_library(&root, true).unwrap();
    let profiles = list(&root).unwrap();
    assert_eq!(profiles.len(), 100);

    let snapshot = create_folder(&root, None, FolderScope::Instances, "All instances");
    let instances = folder_id(&snapshot, "All instances", FolderScope::Instances);
    let filled = change_folders(
        &root,
        snapshot.revision,
        FolderAction::File {
            id: instances,
            members: profiles
                .iter()
                .map(|profile| instance(profile.id))
                .collect(),
        },
    )
    .unwrap();
    assert_eq!(
        member_resolutions(&filled, instances),
        vec![MemberResolution::Resolved; 100]
    );

    let worlds = create_folder(&root, filled.revision, FolderScope::Worlds, "All worlds");
    let all_worlds = folder_id(&worlds, "All worlds", FolderScope::Worlds);
    let mut members = Vec::new();
    for profile in &profiles {
        let saves = save_root(&root, profile.id);
        for index in 0..10 {
            let name = format!("World {index:02}");
            add_world(&saves, &name);
            members.push(world(profile.id, &name));
        }
    }
    assert_eq!(members.len(), 1000);
    let filed = change_folders(
        &root,
        worlds.revision,
        FolderAction::File {
            id: all_worlds,
            members,
        },
    )
    .unwrap();
    assert_eq!(
        member_resolutions(&filed, all_worlds),
        vec![MemberResolution::Resolved; 1000]
    );
    let reloaded = folders(&root).unwrap();
    assert_eq!(reloaded, filed);
    assert_eq!(reloaded.folders.len(), 2);
}
