use super::*;
use serde_json::{json, Value};
use std::{fs, process::Command};

fn create(name: &str) -> Value {
    json!({"kind":"create", "name":name, "version":"26.3", "memoryMiB":2048, "windowWidth":1280, "windowHeight":720})
}

#[test]
fn server_address_parses_only_supported_host_and_port_forms() {
    for (input, expected) in [
        (" Play.Example.org:25565 ", "play.example.org:25565"),
        ("localhost", "localhost"),
        ("play.Example.ORG.", "play.example.org."),
        ("192.0.2.40", "192.0.2.40"),
        ("192.0.2.40:65535", "192.0.2.40:65535"),
        ("[2001:0DB8:0000:0000::3]:025565", "[2001:db8::3]:25565"),
        ("[::1]", "[::1]"),
    ] {
        let address = ServerAddress::parse(input).unwrap();
        assert_eq!(address.as_str(), expected);
        assert_eq!(serde_json::to_value(address).unwrap(), expected);
    }
    for input in [
        "",
        "https://server.example",
        "server.example/path",
        "example.net?query",
        "example.net#world",
        "user@server.example",
        "a..example",
        "-a.example",
        "server_example",
        "1.2.3.999",
        "server.example:65536",
        "server.example:0",
        "server.example:",
        "server.example:+80",
        "2001:db8::1",
        "[::1]junk",
        "[fe80::1%eth0]",
        "server\n.example",
        "\tserver.example",
        "ｅxample.org",
    ] {
        assert!(ServerAddress::parse(input).is_err(), "{input:?}");
    }
    assert!(ServerAddress::parse(&format!("{}.example", "a".repeat(64))).is_err());
    assert!(ServerAddress::parse_optional(" ").unwrap().is_none());
}

#[test]
fn server_binding_updates_only_one_instance_and_preserves_old_profiles_and_imports() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let (first, save) = existing_player_files(root);
    let first = first[0].clone();
    assert!(!fs::read_to_string(root.join("personal/profiles.json"))
        .unwrap()
        .contains("serverAddress"));
    assert_eq!(
        server_profile(root, first.id)
            .unwrap()
            .server_join_readiness(),
        ServerJoinReadiness::NoServer
    );
    let second = apply(root, create("Other profile")).unwrap().remove(1);
    let address = ServerAddress::parse("test.example:25565").unwrap();
    let other_address = ServerAddress::parse("[2001:db8::1]").unwrap();
    set_server_address(root, first.id, None, Some(address.clone())).unwrap();
    set_server_address(root, second.id, None, Some(other_address.clone())).unwrap();
    assert!(set_server_address(root, first.id, None, None).is_err());

    let mut edited = first.clone();
    edited.memory_mib = 4096;
    let saved = change(root, ProfileAction::Edit(edited)).unwrap().remove(0);
    assert_eq!(saved.server_address.as_ref(), Some(&address));
    assert_eq!(saved.memory_mib, 4096);
    let clone = change(
        root,
        ProfileAction::Clone {
            id: first.id,
            name: "Independent".into(),
        },
    )
    .unwrap()
    .pop()
    .unwrap();
    assert!(clone.server_address.is_none());
    assert_eq!(
        saved.server_join_readiness(),
        ServerJoinReadiness::ProtectedLaunchUnavailable
    );

    let imported = import_instance(
        root,
        Uuid::new_v4(),
        VanillaVersion::Release1_21_1,
        review_pack(),
        |stage| {
            fs::create_dir(stage.join("game")).unwrap();
            fs::write(stage.join("game/sample.txt"), b"pack canary").unwrap();
            Ok(())
        },
    )
    .unwrap();
    assert!(imported.server_address.is_none());
    let bound = set_server_address(root, imported.id, None, Some(address.clone())).unwrap();
    assert_eq!(
        bound.server_join_readiness(),
        ServerJoinReadiness::UnqualifiedPack
    );
    let retried = import_instance(root, imported.id, imported.version, review_pack(), |_| {
        panic!("repeat import should be read-only")
    })
    .unwrap();
    assert_eq!(retried, bound);
    assert_eq!(retried.version, VanillaVersion::Release1_21_1);
    assert_eq!(retried.imported_pack, Some(review_pack()));
    assert!(change(
        root,
        ProfileAction::Clone {
            id: imported.id,
            name: "Do not clone".into()
        }
    )
    .is_err());
    assert_eq!(
        fs::read(
            root.join("instances-v1")
                .join(imported.id.to_string())
                .join("game/sample.txt")
        )
        .unwrap(),
        b"pack canary"
    );

    let cleared = set_server_address(root, first.id, Some(&address), None).unwrap();
    assert!(cleared.server_address.is_none());
    assert_eq!(cleared.memory_mib, 4096);
    assert_eq!(
        server_profile(root, second.id).unwrap().server_address,
        Some(other_address)
    );
    assert_eq!(
        server_profile(root, imported.id).unwrap().server_address,
        Some(address)
    );
    assert_eq!(fs::read(save).unwrap(), b"player's save");
}

#[test]
fn comparison_roots_start_identically_and_existing_profiles_are_preserved() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    for root in [first.path(), second.path()] {
        seed_review_library(root, true).unwrap();
    }
    let profiles = list(first.path()).unwrap();
    assert_eq!(profiles.len(), 100);
    assert_eq!(profiles, list(second.path()).unwrap());
    let updated = apply(first.path(), create("My instance")).unwrap();
    seed_review_library(first.path(), false).unwrap();
    assert_eq!(updated, list(first.path()).unwrap());
    assert_eq!(profiles, list(second.path()).unwrap());
}

#[test]
fn a_native_editor_cannot_overwrite_a_newer_saved_revision() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let opened = apply(root, create("Forest")).unwrap().remove(0);
    let mut newer = opened.clone();
    newer.memory_mib = 4096;
    change_if_current(root, ProfileAction::Edit(newer.clone()), Some(&opened)).unwrap();
    assert!(change_if_current(root, ProfileAction::Edit(opened.clone()), Some(&opened)).is_err());
    assert_eq!(list(root).unwrap(), vec![newer]);
}

#[test]
fn java_selection_survives_reopen_and_clones_independently() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let first = apply(root, create("Forest")).unwrap().remove(0);
    assert_eq!(java_choice(root, first.id).unwrap(), (first.version, None));
    let custom = JavaChoice::Directory(root.join("my-java"));
    select_java(root, first.id, first.version, custom.clone()).unwrap();
    let cloned = apply(root, json!({"kind":"clone","id":first.id,"name":"Second"}))
        .unwrap()
        .remove(1);
    assert_eq!(
        java_choice(root, cloned.id).unwrap().1,
        Some(custom.clone())
    );
    select_java(root, cloned.id, cloned.version, JavaChoice::Managed).unwrap();
    assert_eq!(java_choice(root, first.id).unwrap().1, Some(custom));
    assert!(select_java(
        root,
        first.id,
        VanillaVersion::Release1_20_4,
        JavaChoice::Managed
    )
    .is_err());
    let index = fs::read_to_string(root.join("personal/profiles.json")).unwrap();
    assert!(!index.contains("my-java"));
}

fn apply(root: &Path, json: Value) -> Result<Vec<Profile>, String> {
    change(
        root,
        serde_json::from_value(json).map_err(|e| e.to_string())?,
    )
}

#[test]
fn creates_edits_and_clones_without_sharing_settings_or_using_names_as_paths() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    assert!(list(&root).unwrap().is_empty());
    let first = apply(&root, create("  自分 ../世界  ")).unwrap().remove(0);
    assert_eq!(first.name, "自分 ../世界");
    let sibling = apply(&root, json!({"kind":"clone","id":first.id,"name":"Copy"}))
        .unwrap()
        .remove(1);
    assert_ne!(sibling.id, first.id);
    let mut edit = serde_json::to_value(&sibling).unwrap();
    edit["kind"] = json!("edit");
    edit["name"] = json!("Renamed");
    edit["memoryMiB"] = json!(3072);
    edit["windowHeight"] = json!(1080);
    edit["version"] = json!("1.20.4");
    let updated = apply(&root, edit).unwrap();
    assert_eq!(updated[0], first);
    assert_eq!(updated[1].id, sibling.id);
    assert_eq!(updated[1].name, "Renamed");
    assert_eq!(updated[1].memory_mib, 3072);
    assert_eq!(updated[1].window_height, 1080);
    assert_eq!(list(&root).unwrap(), updated);
    assert!(!temp.path().join("世界").exists());
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    assert_eq!(fs::read_dir(root.join("personal")).unwrap().count(), 3);
}

#[test]
fn invalid_fields_names_options_and_unknown_ids_preserve_the_last_index() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let profiles = apply(root, create("Keep")).unwrap();
    let path = root.join("personal/profiles.json");
    let original = fs::read(&path).unwrap();
    for (field, value) in [
        ("memoryMiB", json!(511)),
        ("memoryMiB", json!(16385)),
        ("windowWidth", json!(319)),
        ("windowWidth", json!(7681)),
        ("windowHeight", json!(239)),
        ("windowHeight", json!(4321)),
        ("name", json!("  ")),
        ("name", json!("A\nB")),
        ("name", json!("x\u{202e}y")),
        ("name", json!("日".repeat(81))),
        ("version", json!("latest")),
        ("jvmArgs", json!("-Xmx2G")),
        ("path", json!("chosen-folder")),
        ("id", json!(profiles[0].id)),
    ] {
        let mut action = create("Try");
        action[field] = value;
        assert!(apply(root, action).is_err(), "{field}");
    }
    assert!(apply(
        root,
        json!({"kind":"clone","id":Uuid::new_v4(),"name":"Copy"})
    )
    .is_err());
    assert!(apply(root, json!({"kind":"clone","id":profiles[0].id,"name":" "})).is_err());
    assert_eq!(fs::read(path).unwrap(), original);
}

#[test]
fn pinned_choices_and_inclusive_setting_limits_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    for (i, version) in ["26.3", "1.21.1", "1.20.4", "1.20.1", "24w14a"]
        .iter()
        .enumerate()
    {
        let mut action = create("Vanilla");
        action["version"] = json!(version);
        let (memory, width, height) = if i % 2 == 0 {
            (512, 320, 240)
        } else {
            (16384, 7680, 4320)
        };
        action["memoryMiB"] = json!(memory);
        action["windowWidth"] = json!(width);
        action["windowHeight"] = json!(height);
        let result = apply(temp.path(), action).unwrap();
        let wire = serde_json::to_value(&result[i]).unwrap();
        assert_eq!(wire["version"], json!(version));
        assert_eq!(wire["memoryMiB"], json!(memory));
        assert_eq!(wire["windowWidth"], json!(width));
        assert_eq!(wire["windowHeight"], json!(height));
    }
}

#[test]
fn missing_invalid_or_unknown_metadata_is_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let profiles = apply(root, create("Original")).unwrap();
    let path = root.join("personal/profiles.json");
    for data in [
        b"incomplete".to_vec(),
        serde_json::to_vec(&json!({"version":2,"profiles":profiles})).unwrap(),
        serde_json::to_vec(&json!({"version":1,"profiles":[profiles[0],profiles[0]]})).unwrap(),
    ] {
        fs::write(&path, &data).unwrap();
        assert!(apply(root, create("Another")).is_err());
        assert_eq!(fs::read(&path).unwrap(), data);
    }
    fs::remove_file(&path).unwrap();
    assert!(list(root).is_err());
    assert!(!path.exists());
}

#[test]
fn interrupted_write_or_blocked_staging_preserves_valid_profiles() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let first = apply(root, create("Original")).unwrap();
    let scratch = root.join("personal/profiles.next");
    fs::write(&scratch, b"partial").unwrap();
    assert_eq!(list(root).unwrap(), first);
    assert_eq!(apply(root, create("Second")).unwrap().len(), 2);
    assert!(!scratch.exists());
    fs::create_dir(&scratch).unwrap();
    assert!(apply(root, create("Third")).is_err());
    assert_eq!(list(root).unwrap().len(), 2);
    assert!(scratch.is_dir());
}

#[test]
fn process_create_helper() {
    let Some(root) = std::env::var_os("CARBON_PROFILE_TEST_LIBRARY") else {
        return;
    };
    apply(Path::new(&root), create("Concurrent")).unwrap();
}

#[test]
fn separate_processes_serialize_first_use_and_keep_every_created_profile() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("new");
    let workers: Vec<_> = (0..4)
        .map(|_| {
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "tests::process_create_helper"])
                .env("CARBON_PROFILE_TEST_LIBRARY", &root)
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap()
        })
        .collect();
    for mut worker in workers {
        assert!(worker.wait().unwrap().success());
    }
    let result = list(&root).unwrap();
    assert_eq!(result.len(), 4);
    assert_eq!(result.iter().map(|p| p.id).collect::<HashSet<_>>().len(), 4);
}

#[cfg(unix)]
#[test]
fn linked_personal_directory_is_refused_and_existing_files_are_preserved() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let marker = other.path().join("keep");
    fs::write(&marker, b"independent").unwrap();
    symlink(other.path(), temp.path().join("personal")).unwrap();
    assert!(apply(temp.path(), create("Sample")).is_err());
    assert_eq!(fs::read(marker).unwrap(), b"independent");
    assert_eq!(fs::read_dir(other.path()).unwrap().count(), 1);
}

fn review_pack() -> ImportedPack {
    ImportedPack {
        name: "Reviewed pack".into(),
        loader: Some("fabric-loader 0.16.10".into()),
        pending_files: 2,
    }
}

#[test]
fn process_import_interruption_helper() {
    let Some(root) = std::env::var_os("CARBON_PROFILE_TEST_IMPORT_ROOT") else {
        return;
    };
    let root = Path::new(&root);
    let boundary = std::env::var("CARBON_PROFILE_TEST_IMPORT_BOUNDARY").unwrap();
    let import = import_instance(
        root,
        Uuid::new_v4(),
        VanillaVersion::Release26_3,
        review_pack(),
        |stage| {
            fs::create_dir(stage.join("game")).unwrap();
            fs::write(stage.join("game/sample.txt"), b"imported bytes").unwrap();
            if boundary == "before-rename" {
                // Exit without unwinding or closing the library lock cleanly.
                std::process::exit(41);
            }
            assert_eq!(boundary, "after-rename");
            // The real import will rename complete contents, then hit the blocked index write.
            fs::create_dir(root.join("personal/profiles.next")).unwrap();
            Ok(())
        },
    );
    assert!(import.unwrap_err().contains("Pack files were copied"));
    std::process::exit(42);
}

fn leave_import_at_boundary(root: &Path, boundary: &str, code: i32) -> Profile {
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "tests::process_import_interruption_helper"])
        .env("CARBON_PROFILE_TEST_IMPORT_ROOT", root)
        .env("CARBON_PROFILE_TEST_IMPORT_BOUNDARY", boundary)
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(code));
    let bytes = fs::read(root.join("personal/pack-import.json")).unwrap();
    let marker: Value = serde_json::from_slice(&bytes).unwrap();
    serde_json::from_value(marker["profile"].clone()).unwrap()
}

fn existing_player_files(root: &Path) -> (Vec<Profile>, PathBuf) {
    let existing = apply(root, create("Personal world")).unwrap();
    let save = root
        .join("instances-v1")
        .join(existing[0].id.to_string())
        .join("game/saves/My world/level.dat");
    fs::create_dir_all(save.parent().unwrap()).unwrap();
    fs::write(&save, b"player's save").unwrap();
    (existing, save)
}

#[test]
fn crash_before_import_rename_discards_the_partial_stage_and_preserves_saves() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let (existing, save) = existing_player_files(root);
    let interrupted = leave_import_at_boundary(root, "before-rename", 41);
    let instances = root.join("instances-v1");
    let stage = instances.join(format!(".pack-{}", interrupted.id));
    let destination = instances.join(interrupted.id.to_string());
    assert!(stage.join("game/sample.txt").is_file());
    assert!(!destination.exists());

    assert_eq!(list(root).unwrap(), existing);
    assert!(!stage.exists());
    assert!(!destination.exists());
    assert!(fs::read(root.join("personal/pack-import.json")).is_err());
    assert_eq!(fs::read(save).unwrap(), b"player's save");
}

#[test]
fn crash_after_import_rename_recovers_the_same_profile_when_the_index_can_be_saved() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let (mut existing, save) = existing_player_files(root);
    let interrupted = leave_import_at_boundary(root, "after-rename", 42);
    let destination = root.join("instances-v1").join(interrupted.id.to_string());
    let marker = root.join("personal/pack-import.json");
    assert_eq!(
        fs::read(destination.join("game/sample.txt")).unwrap(),
        b"imported bytes"
    );
    assert!(list(root).unwrap_err().contains("regular profile file"));
    assert!(marker.is_file());
    let journal = fs::read(&marker).unwrap();

    fs::remove_dir(root.join("personal/profiles.next")).unwrap();
    existing.push(interrupted);
    assert_eq!(list(root).unwrap(), existing);
    assert!(!marker.exists());
    // An interruption after the index had already landed is also idempotent.
    fs::write(&marker, journal).unwrap();
    assert_eq!(list(root).unwrap(), existing);
    assert!(!marker.exists());
    assert_eq!(fs::read(save).unwrap(), b"player's save");
    assert_eq!(
        fs::read(destination.join("game/sample.txt")).unwrap(),
        b"imported bytes"
    );
}

#[test]
fn ambiguous_completed_import_is_preserved_for_recovery() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let interrupted = leave_import_at_boundary(root, "after-rename", 42);
    let destination = root.join("instances-v1").join(interrupted.id.to_string());
    fs::remove_dir(root.join("personal/profiles.next")).unwrap();
    fs::write(destination.join(".carbon-import-complete"), b"unexpected").unwrap();
    assert!(list(root).unwrap_err().contains("inconsistent contents"));
    assert!(root.join("personal/pack-import.json").is_file());
    assert_eq!(
        fs::read(destination.join("game/sample.txt")).unwrap(),
        b"imported bytes"
    );
}

#[cfg(unix)]
#[test]
fn recovery_refuses_a_link_at_the_staging_path() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let root = temp.path();
    let interrupted = leave_import_at_boundary(root, "before-rename", 41);
    let stage = root
        .join("instances-v1")
        .join(format!(".pack-{}", interrupted.id));
    fs::remove_dir_all(&stage).unwrap();
    fs::write(outside.path().join("keep"), b"outside data").unwrap();
    symlink(outside.path(), &stage).unwrap();
    assert!(list(root).unwrap_err().contains("Linked directories"));
    assert!(fs::symlink_metadata(stage).unwrap().is_symlink());
    assert_eq!(
        fs::read(outside.path().join("keep")).unwrap(),
        b"outside data"
    );
}
