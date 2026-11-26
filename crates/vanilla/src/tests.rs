use super::*;
use carbon_profiles::{ProfileAction, VanillaVersion};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

fn create(root: &Path, version: VanillaVersion) -> Uuid {
    carbon_profiles::change(
        root,
        ProfileAction::Create {
            name: "Vanilla preparation".into(),
            version,
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

fn document(id: &str, kind: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "id":id, "type":kind, "javaVersion":{"component":"java-runtime-gamma","majorVersion":17},
        "downloads":{"client":{"size":24445539,"sha1":"fd19469fed4a4b4c15b2d5133985f0e3e7816a8a","url":"https://piston-data.mojang.com/v1/objects/fd19469fed4a4b4c15b2d5133985f0e3e7816a8a/client.jar"}},
        "assetIndex":{"id":"12","size":438086,"sha1":"52cf7c68811a1743536f7b01630ae8d069800e4d","totalSize":659597921},
        "libraries":[{"name":"example:library:1"}],"arguments":{"game":["--demo"]}
    })).unwrap()
}
fn entry(id: &str, kind: &str, raw: &[u8]) -> Value {
    let sha1 = hex::encode(Sha1::digest(raw));
    json!({"id":id,"type":kind,"sha1":sha1,"url":format!("https://piston-meta.mojang.com/v1/packages/{sha1}/{id}.json")})
}
fn response(raw: &[u8], entries: &[Value], path: &str) -> Result<Vec<u8>, String> {
    if path == LIST_PATH {
        Ok(serde_json::to_vec(&json!({"versions":entries})).unwrap())
    } else {
        Ok(raw.to_vec())
    }
}
fn object(root: &Path, sha1: &str) -> PathBuf {
    root.join("vanilla-metadata-v1/objects").join(sha1)
}

#[test]
fn metadata_is_shared_across_profiles_and_reverified_on_each_cached_read() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let first = create(root, VanillaVersion::Release1_20_4);
    let second = create(root, VanillaVersion::Release1_20_4);
    let raw = document("1.20.4", "release");
    let pins = [entry("1.20.4", "release", &raw)];
    let mut paths = vec![];
    let prepared = prepare_with(root, first, false, |_, path, _| {
        paths.push(path.to_owned());
        response(&raw, &pins, path)
    })
    .unwrap();
    assert_eq!(paths.len(), 2);
    assert_eq!(prepared.origin, Origin::Network);
    assert_eq!(
        (
            prepared.java_major,
            prepared.client_bytes,
            prepared.asset_index_bytes,
            prepared.library_count
        ),
        (17, 24445539, 438086, 1)
    );
    assert_eq!(
        fs::read(object(root, &prepared.metadata_sha1)).unwrap(),
        raw
    );
    let cached = prepare_with(root, second, true, |_, _, _| {
        panic!("offline network request")
    })
    .unwrap();
    assert_eq!((cached.origin, cached.profile_id), (Origin::Cache, second));
    let mut corrupt = raw.clone();
    corrupt[0] ^= 1;
    fs::write(object(root, &prepared.metadata_sha1), corrupt).unwrap();
    assert!(prepare_with(root, first, true, |_, _, _| panic!("offline repair")).is_err());
    assert_eq!(
        prepare_with(root, first, false, |_, p, _| response(&raw, &pins, p))
            .unwrap()
            .origin,
        Origin::Network
    );
    assert_eq!(
        prepare_with(root, first, false, |_, _, _| panic!("cache should satisfy"))
            .unwrap()
            .origin,
        Origin::Cache
    );
    assert_eq!(
        fs::read_dir(root.join("vanilla-metadata-v1/objects"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn invalid_manifest_entries_and_damaged_payload_never_publish_a_binding() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let profile = create(root, VanillaVersion::Release1_20_4);
    let good = document("1.20.4", "release");
    let pin = entry("1.20.4", "release", &good);
    let mut external = pin.clone();
    external["url"] = json!("https://example.org/version.json");
    let mut bad_digest = pin.clone();
    bad_digest["sha1"] = json!("../data");
    let mut wrong_kind = pin.clone();
    wrong_kind["type"] = json!("snapshot");
    let wrong_id = document("1.21.1", "release");
    let wrong_type = document("1.20.4", "snapshot");
    for (pins, body) in [
        (vec![], good.clone()),
        (vec![pin.clone(), pin.clone()], good.clone()),
        (vec![external], good.clone()),
        (vec![bad_digest], good.clone()),
        (vec![wrong_kind], good.clone()),
        (vec![pin.clone()], b"damaged".to_vec()),
        (vec![entry("1.20.4", "release", &wrong_id)], wrong_id),
        (vec![entry("1.20.4", "release", &wrong_type)], wrong_type),
    ] {
        assert!(prepare_with(root, profile, false, |_, p, _| response(&body, &pins, p)).is_err());
        assert!(!root
            .join("vanilla-metadata-v1/versions/1.20.4.json")
            .exists());
    }
    assert!(prepare_with(root, profile, false, |_, _, limit| Ok(vec![
        b' ';
        limit as usize
            + 1
    ]))
    .is_err());
    assert!(prepare_with(root, Uuid::new_v4(), false, |_, _, _| panic!(
        "unknown profile contacted provider"
    ))
    .is_err());
    assert!(prepare_with(root, profile, true, |_, _, _| panic!(
        "cold offline request"
    ))
    .is_err());
}

#[test]
fn cache_binding_remains_version_specific_during_an_edit() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let profile = create(root, VanillaVersion::Release1_20_4);
    let raw = document("1.20.4", "release");
    let pin = [entry("1.20.4", "release", &raw)];
    let result = prepare_with(root, profile, false, |_, path, _| {
        if path != LIST_PATH {
            let mut current = carbon_profiles::list(root).unwrap().remove(0);
            current.version = VanillaVersion::Snapshot24w14a;
            carbon_profiles::change(root, ProfileAction::Edit(current)).unwrap();
        }
        response(&raw, &pin, path)
    });
    assert!(result.unwrap_err().contains("changed version"));
    assert!(prepare_with(root, profile, true, |_, _, _| panic!()).is_err());
}

#[test]
fn missing_content_is_repaired_and_unexpected_files_are_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let profile = create(root, VanillaVersion::Snapshot24w14a);
    let raw = document("24w14a", "snapshot");
    let pins = [entry("24w14a", "snapshot", &raw)];
    let prepared = prepare_with(root, profile, false, |_, p, _| response(&raw, &pins, p)).unwrap();
    let obj = object(root, &prepared.metadata_sha1);
    fs::remove_file(&obj).unwrap();
    assert!(prepare_with(root, profile, true, |_, _, _| panic!()).is_err());
    assert_eq!(
        prepare_with(root, profile, false, |_, p, _| response(&raw, &pins, p))
            .unwrap()
            .origin,
        Origin::Network
    );
    fs::remove_file(&obj).unwrap();
    fs::create_dir(&obj).unwrap();
    assert!(prepare_with(root, profile, false, |_, _, _| panic!(
        "special files must fail closed"
    ))
    .is_err());
    assert!(obj.is_dir());
}

#[test]
#[ignore = "live Mojang HTTPS; downloads public version JSON only"]
fn live_mojang_metadata_and_offline_reopen() {
    let temp = tempfile::tempdir().unwrap();
    for version in [
        VanillaVersion::Release1_20_4,
        VanillaVersion::Release26_3,
        VanillaVersion::Snapshot24w14a,
    ] {
        let id = create(temp.path(), version);
        let result = prepare(temp.path(), id, false).unwrap();
        assert_eq!(result.origin, Origin::Network);
        println!(
            "{} {} Java {} client {} asset index {} listed libraries {} SHA1 {}",
            result.version,
            result.release_type,
            result.java_major,
            result.client_bytes,
            result.asset_index_bytes,
            result.library_count,
            result.metadata_sha1
        );
        let cached = prepare(temp.path(), id, true).unwrap();
        assert_eq!(
            (cached.origin, cached.metadata_sha1),
            (Origin::Cache, result.metadata_sha1)
        );
    }
}

#[test]
fn overlapping_preparations_download_one_pair_of_documents() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let first = create(root, VanillaVersion::Release1_20_4);
    let second = create(root, VanillaVersion::Release1_20_4);
    let raw = document("1.20.4", "release");
    let pins = [entry("1.20.4", "release", &raw)];
    let fetches = AtomicUsize::new(0);
    let run = |profile| {
        prepare_with(root, profile, false, |_, path, _| {
            fetches.fetch_add(1, Ordering::SeqCst);
            response(&raw, &pins, path)
        })
        .unwrap()
        .origin
    };
    let (a, b) = std::thread::scope(|scope| {
        let worker = scope.spawn(|| run(first));
        let b = run(second);
        (worker.join().unwrap(), b)
    });
    assert_ne!(a, b);
    assert_eq!(fetches.load(Ordering::SeqCst), 2);
}

fn client_fixture(root: &Path, id: Uuid, payload: &[u8]) -> String {
    let sha1 = hex::encode(Sha1::digest(payload));
    let mut doc: Value = serde_json::from_slice(&document("1.20.4", "release")).unwrap();
    doc["downloads"]["client"]["size"] = json!(payload.len());
    doc["downloads"]["client"]["sha1"] = json!(sha1);
    let raw = serde_json::to_vec(&doc).unwrap();
    let pins = [entry("1.20.4", "release", &raw)];
    prepare_with(root, id, false, |_, p, _| response(&raw, &pins, p)).unwrap();
    sha1
}

#[test]
fn source_handle_and_cache_are_verified_on_import_repeat_and_repair() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("app");
    let id = create(&root, VanillaVersion::Release1_20_4);
    let clone = create(&root, VanillaVersion::Release1_20_4);
    let source = temp.path().join("chosen.jar");
    let good = b"owned synthetic client fixture";
    let mut wrong = good.to_vec();
    wrong[0] ^= 1;
    assert!(client_status(&root, id).is_err());
    let sha1 = client_fixture(&root, id, good);
    let cached = root.join("vanilla-metadata-v1/clients").join(&sha1);
    let check = || client_status(&root, id).unwrap().status;
    let import = || import_client(&root, id, open_client_file(&source).unwrap());
    assert_eq!(check(), ClientState::Missing);
    fs::write(&source, &wrong).unwrap();
    assert!(import().is_err());
    assert!(!cached.exists());
    fs::write(&source, good).unwrap();
    assert_eq!(import().unwrap().status, ClientState::Verified);
    assert_eq!(import().unwrap().status, ClientState::Verified);
    assert_eq!(
        client_status(&root, clone).unwrap().status,
        ClientState::Verified
    );
    assert_eq!(fs::read_dir(cached.parent().unwrap()).unwrap().count(), 1);
    fs::write(&source, &wrong).unwrap();
    assert!(import().is_err());
    assert_eq!(fs::read(&cached).unwrap(), good);
    fs::write(&cached, &wrong).unwrap();
    assert_eq!(check(), ClientState::RepairNeeded);
    assert!(import().is_err());
    assert_eq!(fs::read(&cached).unwrap(), wrong);
    fs::write(&source, b"wrong length").unwrap();
    assert!(import().is_err());
    fs::write(&source, good).unwrap();
    let handle = open_client_file(&source).unwrap();
    let result = import_client(&root, id, handle).unwrap();
    let wire = serde_json::to_value(result).unwrap();
    assert_eq!(
        wire,
        json!({"profileId":id,"version":"1.20.4","expectedSha1":sha1,"expectedBytes":good.len(),"status":"verified"})
    );
    assert_eq!(fs::read(&source).unwrap(), good);
    assert_eq!(fs::read(&cached).unwrap(), good);
    fs::remove_file(&cached).unwrap();
    fs::create_dir(&cached).unwrap();
    assert!(client_status(&root, id).is_err());
    assert!(import().is_err());
    assert!(cached.is_dir());
    assert!(open_client_file(&temp.path().join("file.txt")).is_err());
}

#[cfg(unix)]
#[test]
fn picker_refuses_a_symlink_leaf() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let jar = temp.path().join("plain.jar");
    let link = temp.path().join("shortcut.jar");
    fs::write(&jar, b"owned").unwrap();
    symlink(&jar, &link).unwrap();
    assert!(open_client_file(&link).is_err());
    assert_eq!(fs::read(jar).unwrap(), b"owned");
}

#[test]
fn imported_packs_cannot_bypass_loader_and_file_requirements_via_vanilla_commands() {
    let temp = tempfile::tempdir().unwrap();
    for (loader, pending) in [
        (Some("fabric-loader 1".to_owned()), 0),
        (None, 1),
        (None, 0),
    ] {
        let blocked = loader.is_some() || pending > 0;
        let info = carbon_profiles::ImportedPack {
            name: "Imported profile".into(),
            loader,
            pending_files: pending,
        };
        let profile = carbon_profiles::import_instance(
            temp.path(),
            Uuid::new_v4(),
            VanillaVersion::Release1_21_1,
            info,
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(selected(temp.path(), profile.id).is_err(), blocked);
        if blocked {
            assert!(game_file_plan(temp.path(), profile.id, true)
                .unwrap_err()
                .contains("pack"));
        }
    }
}
