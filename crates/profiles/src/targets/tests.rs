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
