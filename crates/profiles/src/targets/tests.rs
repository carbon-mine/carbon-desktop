/* SPDX-License-Identifier: MPL-2.0 */
use super::*;
use crate::{change, list, set_server_address, ProfileAction, VanillaVersion};

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
    .into_iter()
    .find(|profile| profile.name == name)
    .unwrap()
    .id
}

fn add(name: &str, address: &str) -> TargetAction {
    TargetAction::SaveDestination {
        id: None,
        name: name.to_owned(),
        address: ServerAddress::parse(address).unwrap(),
    }
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

fn preferences_path(root: &Path, id: Uuid) -> PathBuf {
    root.join("personal").join(format!("targets-{id}.json"))
}

fn world_key(name: &str) -> TargetKey {
    TargetKey::World(name.to_owned())
}

fn world_label(directory: &str, label: Option<&str>) -> TargetAction {
    TargetAction::LabelWorld {
        directory: directory.to_owned(),
        label: label.map(str::to_owned),
    }
}

#[test]
fn mixed_targets_and_selection_remain_private_across_reloads_and_binding_changes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let first = create(&root, "Personal");
    let second = create(&root, "Other");
    let saves = root
        .join("instances-v1")
        .join(first.to_string())
        .join("game/saves");
    for name in ["Cave", "Home"] {
        let world = saves.join(name);
        fs::create_dir_all(&world).unwrap();
        fs::write(world.join("level.dat"), "Player canary").unwrap();
    }
    fs::create_dir(saves.join("incomplete")).unwrap();
    let library = fs::read(root.join("personal/profiles.json")).unwrap();
    let initial = instance_targets(&root, first).unwrap();
    assert_eq!(initial.worlds.len(), 2);
    assert!(!root
        .join("personal")
        .join(format!("targets-{first}.json"))
        .exists());

    let cave_key = initial.worlds[0].key.clone();
    let first_entry = change_instance_targets(
        &root,
        first,
        initial.revision,
        add("Cave", "one.example.net"),
    )
    .unwrap();
    let both = change_instance_targets(
        &root,
        first,
        first_entry.revision,
        add("Cave", "two.example.net:25566"),
    )
    .unwrap();
    assert_eq!(both.worlds[0].name, both.destinations[0].name);
    assert_eq!(both.destinations[0].name, both.destinations[1].name);
    let first_server = both.destinations[0].id;
    let second_server = both.destinations[1].id;
    assert_ne!(first_server, second_server);
    // A selection that looked unchanged in a stale chooser must still check the revision.
    assert!(change_instance_targets(
        &root,
        first,
        first_entry.revision,
        TargetAction::Select(None)
    )
    .is_err());
    let confirmed =
        change_instance_targets(&root, first, both.revision, TargetAction::Select(None)).unwrap();
    assert_eq!(confirmed.revision, both.revision);

    assert!(change_instance_targets(
        &root,
        first,
        first_entry.revision,
        add("Stale", "example.net")
    )
    .is_err());
    assert!(change_instance_targets(
        &root,
        second,
        None,
        TargetAction::Select(Some(TargetKey::Server(first_server)))
    )
    .is_err());

    let selected = change_instance_targets(
        &root,
        first,
        both.revision,
        TargetAction::Select(Some(cave_key.clone())),
    )
    .unwrap();
    assert_eq!(selected.selected, Some(cave_key));
    let mut selected = selected;
    for directory in ["Cave", "Home"] {
        selected = change_instance_targets(
            &root,
            first,
            selected.revision,
            TargetAction::LabelWorld {
                directory: directory.into(),
                label: Some("Same name".into()),
            },
        )
        .unwrap();
    }
    assert_eq!(selected.worlds[0].name, selected.worlds[1].name);
    assert_ne!(selected.worlds[0].key, selected.worlds[1].key);
    assert_eq!(selected.selected, Some(TargetKey::World("Cave".into())));
    let reopened = instance_targets(&root, first).unwrap();
    assert_eq!(reopened.worlds, selected.worlds);
    assert_eq!(reopened.selected, selected.selected);
    assert_eq!(
        fs::read(root.join("personal/profiles.json")).unwrap(),
        library
    );
    assert_eq!(
        fs::read(saves.join("Cave/level.dat")).unwrap(),
        b"Player canary"
    );

    let binding = ServerAddress::parse("bound.example.net").unwrap();
    set_server_address(&root, first, None, Some(binding.clone())).unwrap();
    assert!(instance_targets(&root, first).is_err());
    assert!(change_instance_targets(
        &root,
        first,
        selected.revision,
        add("Denied", "example.net")
    )
    .is_err());
    set_server_address(&root, first, Some(&binding), None).unwrap();
    assert_eq!(
        instance_targets(&root, first).unwrap().selected,
        selected.selected
    );

    let changed = change_instance_targets(
        &root,
        first,
        selected.revision,
        TargetAction::SaveDestination {
            id: Some(first_server),
            name: "Changed label".into(),
            address: ServerAddress::parse("one.example.net").unwrap(),
        },
    )
    .unwrap();
    assert_eq!(changed.destinations[0].id, first_server);
    assert_eq!(changed.destinations[1].id, second_server);
    let chosen = change_instance_targets(
        &root,
        first,
        changed.revision,
        TargetAction::Select(Some(TargetKey::Server(first_server))),
    )
    .unwrap();
    let removed = change_instance_targets(
        &root,
        first,
        chosen.revision,
        TargetAction::RemoveDestination(first_server),
    )
    .unwrap();
    assert_eq!(removed.selected, None);
    assert_eq!(removed.destinations[0].id, second_server);

    change(
        &root,
        ProfileAction::Clone {
            id: first,
            name: "Copy".into(),
        },
    )
    .unwrap();
    let clone = list(&root)
        .unwrap()
        .into_iter()
        .find(|p| p.name == "Copy")
        .unwrap()
        .id;
    assert!(instance_targets(&root, second)
        .unwrap()
        .destinations
        .is_empty());
    let cloned = instance_targets(&root, clone).unwrap();
    assert!(cloned.worlds.is_empty());
    assert!(cloned.destinations.is_empty());
    assert_eq!(
        instance_targets(&root, first).unwrap().destinations,
        removed.destinations
    );
}

#[cfg(unix)]
#[test]
fn launcher_never_selects_linked_or_removed_worlds() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let id = create(&root, "Local saves");
    let external = root.join("external");
    fs::create_dir(&external).unwrap();
    fs::write(external.join("level.dat"), "External canary").unwrap();

    let saves = root
        .join("instances-v1")
        .join(id.to_string())
        .join("game/saves");
    fs::create_dir_all(saves.join("World")).unwrap();
    fs::write(saves.join("World/level.dat"), "Private save").unwrap();
    symlink(&external, saves.join("Link")).unwrap();
    fs::create_dir(saves.join("LinkedFile")).unwrap();
    symlink(
        external.join("level.dat"),
        saves.join("LinkedFile/level.dat"),
    )
    .unwrap();
    let targets = instance_targets(&root, id).unwrap();
    assert_eq!(targets.worlds.len(), 1);
    let key = targets.worlds[0].key.clone();
    let saved = change_instance_targets(
        &root,
        id,
        targets.revision,
        TargetAction::Select(Some(key.clone())),
    )
    .unwrap();
    fs::rename(saves.join("World"), saves.join("Renamed")).unwrap();
    assert_eq!(instance_targets(&root, id).unwrap().selected, None);
    assert!(
        change_instance_targets(&root, id, saved.revision, TargetAction::Select(Some(key)))
            .is_err()
    );
    assert_eq!(
        fs::read(external.join("level.dat")).unwrap(),
        b"External canary"
    );
}

#[test]
fn fresh_pack_import_has_an_independent_inventory_and_retry_keeps_preference() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let original_id = Uuid::new_v4();
    let copy_id = Uuid::new_v4();
    let pack = crate::ImportedPack {
        name: "Shared pack".into(),
        loader: None,
        pending_files: 0,
    };
    let import = |id| {
        crate::import_instance(
            &root,
            id,
            VanillaVersion::Release26_3,
            pack.clone(),
            |instance| {
                let world = instance.join("game/saves/Starter");
                fs::create_dir_all(&world).map_err(|error| error.to_string())?;
                fs::write(world.join("level.dat"), "Unchanged starter")
                    .map_err(|error| error.to_string())
            },
        )
        .unwrap()
    };
    import(original_id);
    let original = instance_targets(&root, original_id).unwrap();
    assert_eq!(original.worlds.len(), 1);
    let selected = change_instance_targets(
        &root,
        original_id,
        original.revision,
        TargetAction::Select(Some(original.worlds[0].key.clone())),
    )
    .unwrap();

    import(original_id);
    assert_eq!(
        instance_targets(&root, original_id).unwrap().selected,
        selected.selected
    );
    import(copy_id);
    let copy = instance_targets(&root, copy_id).unwrap();
    assert_eq!(copy.worlds[0].name, "Starter");
    assert!(copy.selected.is_none());
    assert!(copy.destinations.is_empty());
}

#[test]
fn clearing_an_unavailable_selection_survives_restoring_the_directory() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let id = create(root, "Restore test");
    let world = root
        .join("instances-v1")
        .join(id.to_string())
        .join("game/saves/Home");
    fs::create_dir_all(&world).unwrap();
    fs::write(world.join("level.dat"), "Player canary").unwrap();
    let key = instance_targets(root, id).unwrap().worlds.remove(0).key;
    let selected =
        change_instance_targets(root, id, None, TargetAction::Select(Some(key))).unwrap();
    let moved = temp.path().join("temporarily moved");
    fs::rename(&world, &moved).unwrap();
    assert!(instance_targets(root, id).unwrap().selected.is_none());
    change_instance_targets(root, id, selected.revision, TargetAction::Select(None)).unwrap();
    fs::rename(moved, &world).unwrap();
    assert!(instance_targets(root, id).unwrap().selected.is_none());
    assert_eq!(fs::read(world.join("level.dat")).unwrap(), b"Player canary");
}

#[test]
fn label_capacity_survives_a_missing_save_until_explicitly_forgotten() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let id = create(&root, "Label capacity");
    let saves = save_root(&root, id);
    for index in 0..MAX_WORLD_LABELS {
        add_world(&saves, &format!("World {index:02}"));
    }
    let mut snapshot = instance_targets(&root, id).unwrap();
    assert_eq!(snapshot.worlds.len(), MAX_WORLD_LABELS);
    assert!(snapshot.world_issue.is_none());
    assert!(snapshot.missing_labels.is_empty());

    for index in 0..MAX_WORLD_LABELS {
        let directory = format!("World {index:02}");
        snapshot = change_instance_targets(
            &root,
            id,
            snapshot.revision,
            world_label(&directory, Some(&format!("Named {index:02}"))),
        )
        .unwrap();
    }
    assert_eq!(snapshot.worlds.len(), MAX_WORLD_LABELS);
    assert!(snapshot.worlds.iter().all(|world| match &world.key {
        TargetKey::World(directory) => world.name != *directory,
        TargetKey::Server(_) => false,
    }));
    assert!(snapshot.missing_labels.is_empty());

    // A folder that disappears keeps its label and its slot.
    fs::remove_dir_all(saves.join("World 00")).unwrap();
    add_world(&saves, "World new");
    let missing = instance_targets(&root, id).unwrap();
    assert_eq!(missing.worlds.len(), MAX_WORLD_LABELS);
    assert_eq!(
        missing.missing_labels.get("World 00").map(String::as_str),
        Some("Named 00")
    );
    assert!(change_instance_targets(
        &root,
        id,
        missing.revision,
        world_label("World new", Some("Named new")),
    )
    .is_err());

    // A save that returns must not lose its label to a stale cleanup request.
    add_world(&saves, "World 00");
    let returned = instance_targets(&root, id).unwrap();
    assert!(returned.missing_labels.is_empty());
    assert!(change_instance_targets(
        &root,
        id,
        missing.revision,
        TargetAction::ForgetMissingLabel("World 00".into()),
    )
    .is_err());
    assert_eq!(
        instance_targets(&root, id)
            .unwrap()
            .worlds
            .into_iter()
            .find(|world| world.key == world_key("World 00"))
            .unwrap()
            .name,
        "Named 00"
    );

    // Once confirmed missing again, the explicit cleanup reclaims the slot.
    fs::remove_dir_all(saves.join("World 00")).unwrap();
    let forgotten = change_instance_targets(
        &root,
        id,
        missing.revision,
        TargetAction::ForgetMissingLabel("World 00".into()),
    )
    .unwrap();
    assert!(!forgotten.missing_labels.contains_key("World 00"));
    let added = change_instance_targets(
        &root,
        id,
        forgotten.revision,
        world_label("World new", Some("Named new")),
    )
    .unwrap();
    assert!(added
        .worlds
        .iter()
        .any(|world| world.key == world_key("World new") && world.name == "Named new"));

    // Removing a label through the explicit label action is also allowed for a
    // confirmed missing world and only then reclaims the slot.
    fs::remove_dir_all(saves.join("World 01")).unwrap();
    let missing_again = instance_targets(&root, id).unwrap();
    assert_eq!(
        missing_again
            .missing_labels
            .get("World 01")
            .map(String::as_str),
        Some("Named 01")
    );
    let reclaimed = change_instance_targets(
        &root,
        id,
        missing_again.revision,
        world_label("World 01", None),
    )
    .unwrap();
    assert!(!reclaimed.missing_labels.contains_key("World 01"));
}

#[test]
fn over_limit_save_scan_keeps_destination_crud_and_revision_guards() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let id = create(&root, "Busy saves");
    let saves = save_root(&root, id);
    fs::create_dir_all(&saves).unwrap();
    for index in 0..=MAX_WORLD_ENTRIES {
        fs::File::create(saves.join(format!("entry-{index:04}"))).unwrap();
    }
    let start = instance_targets(&root, id).unwrap();
    assert!(start.world_issue.is_some());
    assert!(start.worlds.is_empty());
    assert!(start.missing_labels.is_empty());
    assert!(start.destinations.is_empty());

    let first =
        change_instance_targets(&root, id, start.revision, add("One", "one.example.net")).unwrap();
    let second = change_instance_targets(
        &root,
        id,
        first.revision,
        add("Two", "two.example.net:25566"),
    )
    .unwrap();
    let first_id = second.destinations[0].id;
    let second_id = second.destinations[1].id;
    let chosen = change_instance_targets(
        &root,
        id,
        second.revision,
        TargetAction::Select(Some(TargetKey::Server(first_id))),
    )
    .unwrap();
    assert_eq!(chosen.selected, Some(TargetKey::Server(first_id)));
    let removed = change_instance_targets(
        &root,
        id,
        chosen.revision,
        TargetAction::RemoveDestination(first_id),
    )
    .unwrap();
    assert_eq!(removed.selected, None);
    assert_eq!(removed.destinations[0].id, second_id);
    let remaining = change_instance_targets(
        &root,
        id,
        removed.revision,
        TargetAction::Select(Some(TargetKey::Server(second_id))),
    )
    .unwrap();
    let cleared =
        change_instance_targets(&root, id, remaining.revision, TargetAction::Select(None)).unwrap();
    assert_eq!(cleared.selected, None);

    // Stale revisions still guard destination changes.
    assert!(change_instance_targets(
        &root,
        id,
        second.revision,
        TargetAction::RemoveDestination(second_id),
    )
    .is_err());
    // Save targets stay fail closed while the scan is over the limit.
    assert!(change_instance_targets(
        &root,
        id,
        cleared.revision,
        TargetAction::Select(Some(world_key("entry-0000"))),
    )
    .is_err());
    assert!(change_instance_targets(
        &root,
        id,
        cleared.revision,
        world_label("entry-0000", Some("Hidden")),
    )
    .is_err());
    assert_eq!(fs::read_dir(&saves).unwrap().count(), MAX_WORLD_ENTRIES + 1);
    assert!(instance_targets(&root, id).unwrap().world_issue.is_some());
}

#[test]
fn incomplete_scan_is_distinct_from_empty_and_restores_the_saved_world_choice() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();

    let empty_id = create(&root, "Empty saves");
    fs::create_dir_all(save_root(&root, empty_id)).unwrap();
    let empty = instance_targets(&root, empty_id).unwrap();
    assert!(empty.world_issue.is_none());
    assert!(empty.worlds.is_empty());
    assert!(empty.missing_labels.is_empty());

    let id = create(&root, "Incomplete saves");
    let saves = save_root(&root, id);
    add_world(&saves, "Home");
    let listed = instance_targets(&root, id).unwrap();
    assert!(listed.world_issue.is_none());
    let chosen = change_instance_targets(
        &root,
        id,
        listed.revision,
        TargetAction::Select(Some(world_key("Home"))),
    )
    .unwrap();
    let with_server = change_instance_targets(
        &root,
        id,
        chosen.revision,
        add("Early", "early.example.net"),
    )
    .unwrap();
    let preferences = preferences_path(&root, id);
    let before = fs::read(&preferences).unwrap();

    let backup = root.join("saves-backup");
    fs::rename(&saves, &backup).unwrap();
    fs::write(&saves, b"not a directory").unwrap();

    let broken = instance_targets(&root, id).unwrap();
    assert!(broken.world_issue.is_some());
    assert!(broken.worlds.is_empty());
    assert_eq!(broken.selected, Some(world_key("Home")));
    assert!(broken.missing_labels.is_empty());
    assert_eq!(broken.revision, with_server.revision);
    assert_eq!(broken.destinations.len(), 1);
    // Reads never write.
    assert_eq!(fs::read(&preferences).unwrap(), before);
    assert!(instance_targets(&root, id).unwrap().world_issue.is_some());
    assert_eq!(fs::read(&preferences).unwrap(), before);

    // Save operations fail closed while destinations remain revision guarded.
    assert!(change_instance_targets(
        &root,
        id,
        with_server.revision,
        TargetAction::Select(Some(world_key("Home"))),
    )
    .is_err());
    assert!(change_instance_targets(
        &root,
        id,
        with_server.revision,
        world_label("Home", Some("Mine")),
    )
    .is_err());
    let later = change_instance_targets(
        &root,
        id,
        with_server.revision,
        add("Late", "late.example.net"),
    )
    .unwrap();
    assert_eq!(later.destinations.len(), 2);
    assert_eq!(later.selected, Some(world_key("Home")));
    // The failed save operations did not overwrite the stored world choice.
    let stored: serde_json::Value =
        serde_json::from_slice(&fs::read(&preferences).unwrap()).unwrap();
    assert_eq!(stored["selected"]["id"], "Home");
    assert!(change_instance_targets(
        &root,
        id,
        with_server.revision,
        add("Stale", "stale.example.net"),
    )
    .is_err());

    fs::remove_file(&saves).unwrap();
    fs::rename(&backup, &saves).unwrap();
    let recovered = instance_targets(&root, id).unwrap();
    assert!(recovered.world_issue.is_none());
    assert_eq!(recovered.worlds.len(), 1);
    assert_eq!(recovered.selected, Some(world_key("Home")));
    assert_eq!(recovered.revision, later.revision);
    assert_eq!(recovered.destinations.len(), 2);
}

#[test]
fn absent_and_special_level_data_are_not_worlds_or_scan_failures() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let id = create(&root, "Level data");
    let saves = save_root(&root, id);
    add_world(&saves, "Good");
    fs::create_dir_all(saves.join("No level.dat")).unwrap();
    fs::create_dir_all(saves.join("Directory level.dat/level.dat")).unwrap();

    let targets = instance_targets(&root, id).unwrap();
    assert!(targets.world_issue.is_none());
    assert_eq!(
        targets
            .worlds
            .iter()
            .map(|world| world.key.clone())
            .collect::<Vec<_>>(),
        vec![world_key("Good")]
    );
    assert!(targets.missing_labels.is_empty());
    // A present folder without usable level data cannot be mistaken for a missing
    // labeled save or labeled as a world.
    assert!(change_instance_targets(
        &root,
        id,
        targets.revision,
        world_label("No level.dat", None),
    )
    .is_err());
    assert!(change_instance_targets(
        &root,
        id,
        targets.revision,
        world_label("No level.dat", Some("Nope")),
    )
    .is_err());

    let file = root.join("plain-file");
    fs::write(&file, b"").unwrap();
    assert_eq!(inspect(&file), Ok(EntryKind::File));
    assert_eq!(inspect(&root.join("absent")), Ok(EntryKind::Absent));
    assert!(inspect(Path::new("invalid\0path")).is_err());
}

#[test]
fn server_changes_and_reads_leave_world_labels_and_saves_untouched() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let id = create(&root, "Labels stay");
    let saves = save_root(&root, id);
    add_world(&saves, "Home");
    add_world(&saves, "Gone");

    let targets = instance_targets(&root, id).unwrap();
    let targets = change_instance_targets(
        &root,
        id,
        targets.revision,
        world_label("Home", Some("My home")),
    )
    .unwrap();
    change_instance_targets(
        &root,
        id,
        targets.revision,
        world_label("Gone", Some("Keep me")),
    )
    .unwrap();
    fs::remove_dir_all(saves.join("Gone")).unwrap();

    let preferences = preferences_path(&root, id);
    let before_read = fs::read(&preferences).unwrap();
    let missing = instance_targets(&root, id).unwrap();
    assert_eq!(
        missing.missing_labels.get("Gone").map(String::as_str),
        Some("Keep me")
    );
    assert_eq!(fs::read(&preferences).unwrap(), before_read);

    let added = change_instance_targets(
        &root,
        id,
        missing.revision,
        add("Server", "server.example.net"),
    )
    .unwrap();
    let server = added.destinations[0].id;
    let selected = change_instance_targets(
        &root,
        id,
        added.revision,
        TargetAction::Select(Some(TargetKey::Server(server))),
    )
    .unwrap();
    assert_eq!(selected.selected, Some(TargetKey::Server(server)));
    assert_eq!(
        selected.missing_labels.get("Gone").map(String::as_str),
        Some("Keep me")
    );
    assert_eq!(
        fs::read(saves.join("Home/level.dat")).unwrap(),
        b"synthetic level"
    );
    assert!(!saves.join("Gone").exists());

    let stored: serde_json::Value =
        serde_json::from_slice(&fs::read(&preferences).unwrap()).unwrap();
    assert_eq!(stored["world_labels"]["Gone"], "Keep me");
    assert_eq!(stored["world_labels"]["Home"], "My home");
}
