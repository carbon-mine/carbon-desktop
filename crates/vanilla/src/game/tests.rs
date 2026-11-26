use super::*;
use carbon_profiles::{ProfileAction, VanillaVersion};
use serde_json::{json, Value};
use std::{collections::HashMap, fs, io::Cursor};

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha1::digest(bytes))
}
fn file(bytes: &[u8], name: &str, kind: &str) -> Value {
    let hash = digest(bytes);
    let url = match kind {
        "library" => format!("https://libraries.minecraft.net/{name}"),
        "index" => format!("https://piston-meta.mojang.com/v1/packages/{hash}/{name}.json"),
        _ => format!("https://piston-data.mojang.com/v1/objects/{hash}/{name}"),
    };
    json!({"id":name,"path":name,"size":bytes.len(),"sha1":hash,"url":url})
}
struct Fixture {
    temp: tempfile::TempDir,
    profile: Uuid,
    info: PreparedVersion,
    doc: Value,
    index: Vec<u8>,
    bytes: HashMap<String, Vec<u8>>,
}
impl Fixture {
    fn new() -> Self {
        Self::with_legacy_natives(true)
    }
    fn with_legacy_natives(legacy: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let profile = create(root, VanillaVersion::Release1_20_4);
        let platform = Platform::current().unwrap();
        let os = platform.label().split('-').next().unwrap().to_owned();
        let asset = b"shared asset".to_vec();
        let index=serde_json::to_vec(&json!({"objects":{"first":{"hash":digest(&asset),"size":asset.len()},"alias":{"hash":digest(&asset),"size":asset.len()}}})).unwrap();
        let client = b"test client".to_vec();
        let lib = b"selected library".to_vec();
        let native = b"native classifier".to_vec();
        let log = b"logging".to_vec();
        let mut doc = json!({"id":"1.20.4","type":"release","javaVersion":{"majorVersion":17},"downloads":{"client":file(&client,"client.jar","data")},
            "assetIndex":file(&index,"12","index"),"logging":{"client":{"file":file(&log,"logging.xml","data")}},
            "libraries":[{"downloads":{"artifact":file(&lib,"a/base.jar","library")}},{"rules":[{"action":"allow","os":{"name":os}}],
                "downloads":{"artifact":file(&lib,"b/alias.jar","library"),"classifiers":{"native-64":file(&native,"c/native.jar","library"), "native-32":file(&native,"c/native.jar","library")}},
                "natives":{os:"native-${arch}"}},{"rules":[{"action":"allow","os":{"name":"osx"}}],"downloads":{"artifact":file(b"other","osx/apple.jar","library")}}]});
        if !legacy {
            doc["libraries"][1]
                .as_object_mut()
                .unwrap()
                .remove("natives");
            doc["libraries"][2] =
                json!({"downloads":{"artifact":file(&native,"c/native.jar","library")}});
        }
        let raw = serde_json::to_vec(&doc).unwrap();
        let sha = digest(&raw);
        let listing=serde_json::to_vec(&json!({"versions":[{"id":"1.20.4","type":"release","sha1":sha,"url":format!("https://piston-meta.mojang.com/v1/packages/{sha}/1.20.4.json")}]})).unwrap();
        let info = crate::prepare_with(root, profile, false, |_, p, _| {
            Ok(if p == crate::LIST_PATH {
                listing.clone()
            } else {
                raw.clone()
            })
        })
        .unwrap();
        let bytes = [client, lib, native, log, asset, index.clone()]
            .into_iter()
            .map(|b| (digest(&b), b))
            .collect();
        Self {
            temp,
            profile,
            info,
            doc,
            index,
            bytes,
        }
    }
    fn store_plan(&self) -> (Store, Plan) {
        let root = self.temp.path();
        let cache = cache::Cache::open(root).unwrap();
        let store = Store::open(root, cache.client_dir().unwrap()).unwrap();
        drop(cache);
        let plan = Plan::version(
            &self.info,
            &serde_json::to_vec(&self.doc).unwrap(),
            Platform::current().unwrap(),
        )
        .unwrap();
        cache::publish(
            &store.object(&plan.index, &plan.client_sha1).unwrap(),
            &self.index,
        )
        .unwrap();
        resolve(root, self.profile, true, Platform::current().unwrap()).unwrap()
    }
    fn fetch(&self, o: &Object) -> Result<Box<dyn Read + Send>, String> {
        Ok(Box::new(Cursor::new(self.bytes[&o.sha1].clone())))
    }
}

#[test]
fn resolves_os_rules_aliases_classifiers_and_activation_map() {
    let f = Fixture::new();
    let root = f.temp.path();
    let (store, plan) = f.store_plan();
    let s = &plan.summary;
    assert_eq!(
        (
            s.library_files,
            s.asset_objects,
            s.asset_names,
            s.logging_files,
            s.unique_files
        ),
        (3, 1, 2, 1, 6)
    );
    assert_eq!(
        plan.classpath,
        ["libraries/a/base.jar", "libraries/b/alias.jar"]
    );
    assert_eq!(plan.native_archives, ["libraries/c/native.jar"]);
    assert!(!plan.paths.contains_key("libraries/osx/apple.jar"));
    assert!(!has_game_install(root, f.profile).unwrap());
    assert!(
        prepare_reviewed_game_files(root, f.profile, false, &digest(b"old plan"), |_| panic!(
            "stale pin reached a payload operation"
        ))
        .unwrap_err()
        .contains("reviewed")
    );
    let prepared = prepare_with(root, &store, &plan, false, |_| true, |o| f.fetch(o)).unwrap();
    assert_eq!(
        prepared.fetched_bytes + prepared.reused_bytes,
        s.total_bytes
    );
    assert_eq!(prepared.reused_bytes, plan.index.size);
    assert!(has_game_install(root, f.profile).unwrap());
    let repeat = prepare_with(
        root,
        &store,
        &plan,
        true,
        |_| true,
        |_| panic!("offline contacted source"),
    )
    .unwrap();
    assert_eq!(
        (repeat.fetched_bytes, repeat.reused_bytes),
        (0, s.total_bytes)
    );
    let clone = carbon_profiles::change(
        root,
        ProfileAction::Clone {
            id: f.profile,
            name: "Independent".into(),
        },
    )
    .unwrap()
    .last()
    .unwrap()
    .id;
    assert!(!has_game_install(root, clone).unwrap());
    let clone_result = prepare_game_files(root, clone, true, |_| true).unwrap();
    assert_eq!(
        (clone_result.fetched_bytes, clone_result.reused_bytes),
        (0, s.total_bytes)
    );
    assert!(has_game_install(root, clone).unwrap());
}

#[test]
fn cancellation_source_failure_or_changed_version_cannot_publish_the_profile() {
    let f = Fixture::new();
    let root = f.temp.path();
    let (store, plan) = f.store_plan();
    assert!(
        prepare_with(root, &store, &plan, true, |_| true, |_| panic!())
            .unwrap_err()
            .contains("online")
    );
    let error = prepare_with(
        root,
        &store,
        &plan,
        false,
        |p| p.completed_files < 2,
        |o| f.fetch(o),
    )
    .unwrap_err();
    assert!(error.contains("cancelled"));
    assert!(!has_game_install(root, f.profile).unwrap());
    let selected = plan
        .objects
        .values()
        .find(|o| o.sha1 == plan.client_sha1)
        .unwrap();
    let object = store.object(selected, &plan.client_sha1).unwrap();
    // Ensure the next acquisition exercises the bad response, even if cancellation got this far.
    fs::write(&object, b"damaged").unwrap();
    assert!(prepare_with(
        root,
        &store,
        &plan,
        false,
        |_| true,
        |o| {
            if o.sha1 == plan.client_sha1 {
                Ok(Box::new(Cursor::new(vec![b'!'; o.size as usize])))
            } else {
                f.fetch(o)
            }
        }
    )
    .unwrap_err()
    .contains("SHA-1"));
    assert_eq!(fs::read(&object).unwrap(), b"damaged");
    assert!(!has_game_install(root, f.profile).unwrap());
    let interrupted = object.with_file_name(format!(".{}.game-part", selected.sha1));
    fs::write(&interrupted, b"interrupted download").unwrap();
    let prepared = prepare_with(root, &store, &plan, false, |_| true, |o| f.fetch(o)).unwrap();
    assert!(!interrupted.exists());
    assert!(prepared.reused_bytes > 0);
    let saved = fs::read(store.receipt(f.profile)).unwrap();
    fs::write(&object, vec![b'!'; selected.size as usize]).unwrap();
    assert!(prepare_with(root, &store, &plan, true, |_| true, |_| panic!()).is_err());
    assert_eq!(fs::read(store.receipt(f.profile)).unwrap(), saved);
    let repaired = prepare_with(root, &store, &plan, false, |_| true, |o| f.fetch(o)).unwrap();
    assert_eq!(repaired.fetched_bytes, selected.size);
    let err = prepare_with(
        root,
        &store,
        &plan,
        true,
        |p| {
            if p.phase == "activating" {
                let mut profile = carbon_profiles::list(root).unwrap().remove(0);
                profile.version = VanillaVersion::Snapshot24w14a;
                carbon_profiles::change(root, ProfileAction::Edit(profile)).unwrap();
            }
            true
        },
        |_| panic!(),
    )
    .unwrap_err();
    assert!(err.contains("changed"));
    assert!(!has_game_install(root, f.profile).unwrap());
    assert_eq!(fs::read(store.receipt(f.profile)).unwrap(), saved);
}

#[test]
fn platform_rules_apply_the_last_matching_action_in_metadata_order() {
    let f = Fixture::new();
    let platform = Platform::current().unwrap();
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let mut document = f.doc.clone();
    document["libraries"][0]["rules"] = json!([
        {"action":"allow"},
        {"action":"disallow","os":{"name":os,"arch":arch}},
        {"action":"allow","os":{"name":"osx"}},
    ]);
    document["libraries"][1]["rules"] = json!([
        {"action":"disallow"},
        {"action":"allow","os":{"name":os,"arch":arch}},
    ]);
    let plan = Plan::version(&f.info, &serde_json::to_vec(&document).unwrap(), platform).unwrap();
    assert_eq!(plan.classpath, ["libraries/b/alias.jar"]);
    assert_eq!(plan.native_archives, ["libraries/c/native.jar"]);
}

#[test]
fn malformed_source_rules_paths_aliases_and_oversize_are_refused() {
    let f = Fixture::new();
    for (pointer, value) in [
        (
            "/downloads/client/url",
            json!("https://example.org/client.jar"),
        ),
        (
            "/logging/client/file/url",
            json!("https://example.org/log.xml"),
        ),
        (
            "/libraries/0/downloads/artifact/path",
            json!("../local.jar"),
        ),
        (
            "/libraries/0/downloads/artifact/url",
            json!("https://other.example/foreign.jar"),
        ),
        (
            "/libraries/1/rules",
            json!([{"action":"allow","features":{"unimplemented":true}}]),
        ),
        ("/assetIndex/size", json!(INDEX_LIMIT + 1)),
    ] {
        let mut doc = f.doc.clone();
        *doc.pointer_mut(pointer).unwrap() = value;
        assert!(
            Plan::version(
                &f.info,
                &serde_json::to_vec(&doc).unwrap(),
                Platform::current().unwrap()
            )
            .is_err(),
            "{pointer}"
        );
    }
    let raw = serde_json::to_vec(&f.doc).unwrap();
    let mut plan = Plan::version(&f.info, &raw, Platform::current().unwrap()).unwrap();
    let mut wrong: Value = serde_json::from_slice(&f.index).unwrap();
    wrong["objects"]["alias"]["size"] = json!(500);
    assert!(plan
        .add_assets(&serde_json::to_vec(&wrong).unwrap())
        .is_err());
    let mut plan = Plan::version(&f.info, &raw, Platform::current().unwrap()).unwrap();
    let dup = format!(
        r#"{{"objects":{{"a":{{"hash":"{0}","size":1}},"a":{{"hash":"{0}","size":1}}}}}}"#,
        digest(b"a")
    );
    assert!(plan.add_assets(dup.as_bytes()).is_err());
}

fn create(root: &Path, version: VanillaVersion) -> Uuid {
    carbon_profiles::change(
        root,
        ProfileAction::Create {
            name: "Official-source game preparation review".into(),
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

#[test]
#[ignore = "live Mojang version and asset-index JSON only"]
fn live_public_game_plans() {
    let temp = tempfile::tempdir().unwrap();
    for version in VanillaVersion::ALL {
        let id = create(temp.path(), version);
        let (_, plan) = resolve(temp.path(), id, false, Platform::current().unwrap()).unwrap();
        let summary = &plan.summary;
        println!("{}", serde_json::to_string(summary).unwrap());
        assert!(
            summary.library_files > 20
                && summary.asset_objects > 1000
                && summary.logging_files == 1
        );
        if std::env::consts::ARCH == "x86_64" {
            view::supports_layout(&plan).unwrap();
        }
        assert_eq!(
            game_file_plan(temp.path(), id, true).unwrap().total_bytes,
            summary.total_bytes
        );
    }
}

#[test]
#[ignore = "opt-in full official Mojang download and retained review cache"]
fn live_full_official_game_files_and_offline_repeat() {
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let evidence = if cfg!(target_os = "windows") {
        "game-files-review-26-3"
    } else {
        "game-files-review-26-3-linux"
    };
    let root = checkout.join(".artifacts").join(evidence);
    let version = VanillaVersion::Release26_3;
    let id = carbon_profiles::list(&root)
        .unwrap()
        .into_iter()
        .find(|p| p.version == version && p.name == "Official-source game preparation review")
        .map(|p| p.id)
        .unwrap_or_else(|| create(&root, version));
    println!("Dedicated review root {} profile {}", root.display(), id);
    let plan = game_file_plan(&root, id, false).unwrap();
    println!("Plan {}", serde_json::to_string(&plan).unwrap());
    let mut milestone = 0;
    let acquired = prepare_reviewed_game_files(&root, id, false, &plan.metadata_sha1, |p| {
        if p.completed_files >= milestone + 500 {
            println!("Verified {} / {} objects", p.completed_files, p.total_files);
            milestone = p.completed_files;
        }
        true
    })
    .unwrap();
    println!("Prepared {}", serde_json::to_string(&acquired).unwrap());
    assert!(has_game_install(&root, id).unwrap());
    let offline =
        prepare_reviewed_game_files(&root, id, true, &plan.metadata_sha1, |_| true).unwrap();
    println!("Offline {}", serde_json::to_string(&offline).unwrap());
    assert_eq!(
        (offline.fetched_bytes, offline.reused_bytes),
        (0, plan.total_bytes)
    );

    let profiles = carbon_profiles::list(&root).unwrap();
    let original = profiles.iter().find(|p| p.id == id).unwrap().clone();
    let clone_name = "Independent game file reuse review";
    let mut second = match profiles.into_iter().find(|p| p.name == clone_name) {
        Some(profile) => profile,
        None => carbon_profiles::change(
            &root,
            ProfileAction::Clone {
                id,
                name: clone_name.into(),
            },
        )
        .unwrap()
        .pop()
        .unwrap(),
    };
    assert_ne!(second.id, id);
    second.memory_mib = 3072;
    carbon_profiles::change(&root, ProfileAction::Edit(second.clone())).unwrap();
    let reuse =
        prepare_reviewed_game_files(&root, second.id, true, &plan.metadata_sha1, |_| true).unwrap();
    println!(
        "Independent profile {}",
        serde_json::to_string(&reuse).unwrap()
    );
    assert_eq!(
        (reuse.fetched_bytes, reuse.reused_bytes),
        (0, plan.total_bytes)
    );
    assert!(has_game_install(&root, second.id).unwrap());
    assert!(has_game_install(&root, original.id).unwrap());
    assert!(carbon_profiles::list(&root).unwrap().contains(&original));
}

#[test]
fn private_view_repairs_copied_content_and_preserves_user_state() {
    let f = Fixture::with_legacy_natives(false);
    let root = f.temp.path();
    let (store, plan) = f.store_plan();
    let pin = &plan.summary.metadata_sha1;
    let open = || prepare_instance_view(root, f.profile, pin, |_| true);

    assert!(open().unwrap_err().contains("Prepare or verify"));
    assert!(!root.join("instances-v1").exists());
    prepare_with(root, &store, &plan, false, |_| true, |o| f.fetch(o)).unwrap();
    assert!(
        prepare_instance_view(root, f.profile, &digest(b"stale"), |_| panic!())
            .unwrap_err()
            .contains("changed")
    );
    let cancel = prepare_instance_view(root, f.profile, pin, |p| {
        p.phase != "copyingView" || p.completed_files < 2
    });
    assert!(cancel.unwrap_err().contains("cancelled"));
    let owner = root.join("instances-v1").join(f.profile.to_string());
    assert!(!owner.join(".content-stage").exists());
    assert!(!owner.join("content").exists());

    let view = open().unwrap();
    assert_eq!(view.summary().files, plan.paths.len() + 2);
    assert_eq!(view.summary().copied_bytes, view.summary().total_bytes);
    assert_eq!(view.summary().reused_bytes, 0);
    let paths = view.paths();
    let content = paths.content_dir().to_owned();
    let game = paths.game_dir().to_owned();
    let client = paths.client_jar().to_owned();
    assert_eq!(paths.asset_index_id(), "12");
    assert!(paths.assets_dir().join("indexes/12.json").is_file());
    assert_eq!(fs::read(paths.version_metadata()).unwrap(), plan.metadata);
    assert_eq!(
        fs::read(paths.logging_config().unwrap()).unwrap(),
        b"logging"
    );
    assert_eq!(
        paths.class_path(),
        [
            paths.libraries_dir().join("a/base.jar"),
            paths.libraries_dir().join("b/alias.jar"),
            paths.libraries_dir().join("c/native.jar"),
            client.clone(),
        ]
    );
    assert!(paths.natives_dir().read_dir().unwrap().next().is_none());
    assert!(open().unwrap_err().contains("in use"));
    let settings = game.join("options.txt");
    let save = game.join("saves/Private seed/level.dat");
    fs::create_dir_all(save.parent().unwrap()).unwrap();
    fs::write(&settings, b"renderDistance:12").unwrap();
    fs::write(&save, b"player data").unwrap();
    drop(view);

    let reused = open().unwrap();
    assert_eq!(reused.summary().copied_bytes, 0);
    assert_eq!(reused.summary().reused_bytes, reused.summary().total_bytes);
    drop(reused);
    let obj = &plan.objects[&plan.client_sha1];
    let shared = store.object(obj, &plan.client_sha1).unwrap();
    let tampered = vec![b'!'; obj.size as usize];
    fs::write(&client, &tampered).unwrap();
    assert_eq!(
        fs::read(&shared).unwrap(),
        b"test client",
        "must not hard-link shared cache"
    );
    fs::write(&shared, &tampered).unwrap();
    assert!(open().unwrap_err().contains("Prepare game files online"));
    assert_eq!(fs::read(&client).unwrap(), tampered);
    assert!(!owner.join(".content-stage").exists());

    prepare_with(root, &store, &plan, false, |_| true, |o| f.fetch(o)).unwrap();
    let repaired = open().unwrap();
    assert_eq!(
        repaired.summary().copied_bytes,
        repaired.summary().total_bytes
    );
    assert_eq!(fs::read(&client).unwrap(), b"test client");
    drop(repaired);

    // Crash between renames: the last complete tree is restored, not the partial stage.
    fs::rename(&content, owner.join(".content-previous")).unwrap();
    fs::create_dir(owner.join(".content-stage")).unwrap();
    fs::write(owner.join(".content-stage/torn"), b"partial").unwrap();
    let recovered = open().unwrap();
    assert_eq!(
        recovered.summary().reused_bytes,
        recovered.summary().total_bytes
    );
    assert!(!owner.join(".content-previous").exists());
    assert!(!owner.join(".content-stage").exists());
    assert_eq!(fs::read(&settings).unwrap(), b"renderDistance:12");
    assert_eq!(fs::read(&save).unwrap(), b"player data");
    drop(recovered);

    // A profile switched after its copy is rejected instead of publishing an old plan.
    fs::write(&client, &tampered).unwrap();
    let changed = prepare_instance_view(root, f.profile, pin, |p| {
        if p.phase == "activatingView" {
            let mut profile = carbon_profiles::list(root).unwrap().remove(0);
            profile.version = VanillaVersion::Snapshot24w14a;
            carbon_profiles::change(root, ProfileAction::Edit(profile)).unwrap();
        }
        true
    });
    assert!(changed.unwrap_err().contains("changed"));
    assert_eq!(fs::read(&client).unwrap(), tampered);
    assert_eq!(fs::read(&save).unwrap(), b"player data");
}

#[test]
fn private_view_refuses_legacy_native_archives() {
    let f = Fixture::new();
    let root = f.temp.path();
    let (store, plan) = f.store_plan();
    prepare_with(root, &store, &plan, false, |_| true, |o| f.fetch(o)).unwrap();
    assert!(
        prepare_instance_view(root, f.profile, &plan.summary.metadata_sha1, |_| panic!())
            .unwrap_err()
            .contains("legacy native archive extraction")
    );
    assert!(!root.join("instances-v1").exists());
}

#[test]
fn reviewed_prepare_and_verify_include_a_private_view_receipt() {
    let f = Fixture::with_legacy_natives(false);
    let root = f.temp.path();
    let (store, plan) = f.store_plan();
    let pin = &plan.summary.metadata_sha1;
    prepare_with(root, &store, &plan, false, |_| true, |o| f.fetch(o)).unwrap();
    assert!(has_game_install(root, f.profile).unwrap());
    assert!(!has_instance_view(root, f.profile).unwrap());

    let mut phases = Vec::new();
    let result = prepare_reviewed_instance(root, f.profile, true, pin, |p| {
        if phases.last() != Some(&p.phase) {
            phases.push(p.phase);
        }
        true
    })
    .unwrap();
    assert_eq!(
        phases,
        [
            "verifying",
            "activating",
            "verifyingView",
            "copyingView",
            "activatingView"
        ]
    );
    assert_eq!(result.fetched_bytes, 0);
    assert_eq!(result.reused_bytes, plan.summary.total_bytes);
    let json = serde_json::to_value(&result).unwrap();
    let reply = json["instanceView"].as_object().unwrap();
    assert_eq!(reply.len(), 8);
    assert_eq!(reply["version"], "1.20.4");
    assert!(has_instance_view(root, f.profile).unwrap());
    let result = prepare_reviewed_instance(root, f.profile, true, pin, |_| true).unwrap();
    let private = result.instance_view.unwrap();
    assert_eq!(private.reused_bytes, private.total_bytes);
}

#[cfg(unix)]
#[test]
fn private_view_refuses_links_in_managed_content() {
    use std::os::unix::fs::symlink;
    let f = Fixture::with_legacy_natives(false);
    let root = f.temp.path();
    let (store, plan) = f.store_plan();
    prepare_with(root, &store, &plan, false, |_| true, |o| f.fetch(o)).unwrap();
    let open = || prepare_instance_view(root, f.profile, &plan.summary.metadata_sha1, |_| true);
    let prepared = open().unwrap();
    let client = prepared.paths().client_jar().to_owned();
    let sentinel = root.join("outside.txt");
    fs::write(&sentinel, b"outside content").unwrap();
    fs::remove_file(&client).unwrap();
    symlink(&sentinel, &client).unwrap();
    drop(prepared);
    assert!(open().unwrap_err().contains("link or reparse point"));
    assert_eq!(fs::read(&sentinel).unwrap(), b"outside content");
    assert!(fs::symlink_metadata(&client).unwrap().is_symlink());
}

#[test]
#[ignore = "opt-in offline materialization from retained verified public 26.3 test cache"]
fn live_private_26_3_view_from_public_cache() {
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let evidence = if cfg!(target_os = "windows") {
        "game-files-review-26-3"
    } else {
        "game-files-review-26-3-linux"
    };
    let root = checkout.join(".artifacts").join(evidence);
    let profile_id = carbon_profiles::list(&root)
        .unwrap()
        .into_iter()
        .find(|p| p.name == "Official-source game preparation review")
        .expect("Run live_full_official_game_files_and_offline_repeat first")
        .id;
    let plan = game_file_plan(&root, profile_id, true).unwrap();
    assert_eq!(
        plan.metadata_sha1,
        "8696584e8d2da5726487597e6c5777480387ec22"
    );
    let start = std::time::Instant::now();
    let open = || prepare_instance_view(&root, profile_id, &plan.metadata_sha1, |_| true).unwrap();
    let view = open();
    let paths = view.paths();
    assert_eq!(paths.asset_index_id(), "34");
    assert_eq!(paths.class_path().last().unwrap(), paths.client_jar());
    assert_eq!(paths.class_path().len(), plan.library_files + 1);
    assert!(paths.class_path().iter().any(|path| {
        path.file_name()
            .unwrap()
            .to_string_lossy()
            .contains(if cfg!(windows) {
                "-natives-windows.jar"
            } else {
                "-natives-linux.jar"
            })
    }));
    assert!(paths.assets_dir().join("indexes/34.json").is_file());
    assert!(paths.logging_config().unwrap().is_file());
    assert!(paths.version_metadata().is_file());
    assert_eq!(view.summary().files, plan.unique_files + 2);
    let sample = paths.game_dir().join("carbon-private-view-review-sentinel");
    if !sample.exists() {
        fs::write(&sample, b"isolated test state").unwrap();
    }
    println!(
        "First private view {} ({:?})",
        serde_json::to_string(view.summary()).unwrap(),
        start.elapsed()
    );
    drop(view);
    let start = std::time::Instant::now();
    let repeated = open();
    assert_eq!(fs::read(sample).unwrap(), b"isolated test state");
    assert_eq!(
        repeated.summary().reused_bytes,
        repeated.summary().total_bytes
    );
    println!(
        "Offline reuse {} ({:?})",
        serde_json::to_string(repeated.summary()).unwrap(),
        start.elapsed()
    );
}
