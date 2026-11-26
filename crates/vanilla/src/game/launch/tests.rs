// SPDX-License-Identifier: MPL-2.0
use super::{
    arguments::{self, Material, MAIN_CLASS},
    *,
};
use crate::{game::plan::Plan, verified, Origin, Pin};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    time::Instant,
};

fn profile(version: VanillaVersion) -> Profile {
    Profile {
        id: Uuid::from_u128(0xb031a91d_0dd0_4d14_812b_333102819a66),
        name: "Synthetic review".into(),
        version,
        memory_mib: 2048,
        window_width: 1280,
        window_height: 720,
        imported_pack: None,
    }
}

fn manifest() -> Value {
    json!({
        "id": "26.3", "type": "release", "mainClass": MAIN_CLASS, "javaVersion": { "majorVersion": 25 },
        "arguments": {
            "game": [
                "--username", "${auth_player_name}", "--uuid", "${auth_uuid}",
                "--accessToken", "${auth_access_token}", "--version", "${version_name}",
                "--gameDir", "${game_directory}", "--assetsDir", "${assets_root}",
                "--assetIndex", "${assets_index_name}",
                {"rules": [{"action":"allow","features":{"is_quick_play_multiplayer":true}}],
                 "value":["--quickPlayMultiplayer","${quickPlayMultiplayer}"]},
                {"rules": [{"action":"allow","features":{"has_custom_resolution":true}}],
                 "value":["--width","${resolution_width}","--height","${resolution_height}"]}
            ],
            "jvm": [
                {"rules":[{"action":"allow"},{"action":"disallow","os":{"name":"linux"}}],
                 "value":"-XX:HeapDumpPath=MojangTricksIntelDriversForPerformance_javaw.exe_minecraft.exe.heapdump"},
                "-Djava.library.path=${natives_directory}/java", "-cp", "${classpath}"
            ]
        }
    })
}

fn fixture_parts(
    raw: &Value,
    platform: &str,
    java_major: u16,
) -> Result<arguments::Arguments, PlanError> {
    let root = Path::new("/Synthetic Review");
    let classpath = [
        root.join("libraries/first.jar"),
        root.join("libraries/second.jar"),
        root.join("versions/26.3/26.3.jar"),
    ];
    arguments::build(
        &serde_json::to_vec(raw).unwrap(),
        Platform::launch_platform(platform).unwrap(),
        &profile(VanillaVersion::Release26_3),
        java_major,
        Material {
            game: &root.join("game"),
            assets: &root.join("content/assets"),
            asset_index: "55",
            classpath: &classpath,
            log: None,
        },
    )
}

#[test]
fn rule_order_platform_and_classpath_are_deterministic() {
    let raw = manifest();
    let windows = fixture_parts(&raw, "windows-x86_64", 25).unwrap();
    let linux = fixture_parts(&raw, "linux-x86_64", 25).unwrap();
    let repeat = fixture_parts(&raw, "windows-x86_64", 25).unwrap();
    assert!(windows.jvm == repeat.jvm && windows.game == repeat.game);
    assert_eq!(windows.jvm.len(), linux.jvm.len() + 1);
    assert!(windows.game == linux.game);
    for (parts, separator) in [(&windows, ';'), (&linux, ':')] {
        assert!(matches!(&parts.jvm[0], JvmArgument::Value(v) if v == "-Xmx2048M"));
        let classpath = parts
            .jvm
            .windows(2)
            .find_map(|pair| match pair {
                [JvmArgument::Value(flag), JvmArgument::Value(paths)] if flag == "-cp" => {
                    paths.to_str()
                }
                _ => None,
            })
            .unwrap();
        let entries: Vec<_> = classpath.split(separator).map(Path::new).collect();
        assert_eq!(
            entries
                .iter()
                .map(|p| p.file_name().unwrap())
                .collect::<Vec<_>>(),
            ["first.jar", "second.jar", "26.3.jar"]
        );
        assert!(classpath.contains("Synthetic Review"));
        assert!(parts.jvm.iter().any(|arg| matches!(
            arg,
            JvmArgument::NativeDirectory {
                property: "-Djava.library.path",
                subdirectory: Some("java")
            }
        )));
        assert!(parts
            .game
            .iter()
            .any(|arg| matches!(arg, GameArgument::Identity(IdentitySlot::SessionAccess))));
        assert!(!parts
            .game
            .iter()
            .any(|arg| matches!(arg, GameArgument::Value(v) if v == "--quickPlayMultiplayer")));
        assert!(parts
            .game
            .iter()
            .any(|arg| matches!(arg, GameArgument::Value(v) if v == OsStr::new("1280"))));
    }
}

#[test]
fn argument_and_manifest_failures_preserve_their_typed_denials() {
    let run = |raw| fixture_parts(&raw, "windows-x86_64", 25).err().unwrap();
    let mut raw = manifest();
    raw["minecraftArguments"] = json!("--username ${auth_player_name}");
    assert_eq!(run(raw), PlanError::UnsupportedLegacy);

    for invalid in [
        "-javaagent:unreviewed.jar",
        "@args.txt",
        "-Djava.library.path=${natives_directory}/../unreviewed",
        "-cp=extra.jar",
    ] {
        let mut raw = manifest();
        raw["arguments"]["jvm"]
            .as_array_mut()
            .unwrap()
            .push(json!(invalid));
        assert_eq!(run(raw), PlanError::UnsupportedArgument);
    }
    let mut raw = manifest();
    raw["arguments"]["game"][1] = json!("${unreviewed}");
    assert_eq!(run(raw), PlanError::UnsupportedArgument);
    let mut raw = manifest();
    raw["arguments"]["game"][14]["rules"][0]["features"] = json!({"unknown": false});
    assert_eq!(run(raw), PlanError::UnsupportedRule);
    let mut raw = manifest();
    raw["mainClass"] = json!("some.unreviewed.Main");
    assert_eq!(run(raw), PlanError::UnexpectedMainClass);
    let mut raw = manifest();
    raw["arguments"]["jvm"][1] = json!("-Djna.tmpdir=${natives_directory}");
    assert_eq!(run(raw), PlanError::MissingMaterial);
    assert_eq!(
        fixture_parts(&manifest(), "linux-x86_64", 21).err(),
        Some(PlanError::IncompatibleRuntime)
    );
    assert!(Platform::launch_platform("linux-aarch64").is_none());
}

#[test]
fn changed_account_generation_or_profile_fails_the_final_binding_check() {
    let profile = profile(VanillaVersion::Release26_3);
    let selected = AccountReference::new(Uuid::from_u128(1), 5).unwrap();
    let newer = AccountReference::new(Uuid::from_u128(1), 7).unwrap();
    assert_eq!(
        matches_binding(&profile, &profile, [1; 32], [1; 32], selected, newer),
        Err(PlanError::AccountChanged)
    );
    assert_eq!(
        matches_binding(&profile, &profile, [1; 32], [2; 32], selected, selected),
        Err(PlanError::GenerationChanged)
    );
    let mut modified = profile.clone();
    modified.memory_mib *= 2;
    assert_eq!(
        matches_binding(&profile, &modified, [1; 32], [1; 32], selected, selected),
        Err(PlanError::InvalidProfile)
    );
    modified = profile.clone();
    modified.name = "Display rename".into();
    assert!(matches_binding(&profile, &modified, [1; 32], [1; 32], selected, selected).is_ok());
}

#[test]
fn summary_is_redacted() {
    let summary = LaunchSummary {
        version: "26.3",
        platform: "windows-x86_64".into(),
        mode: LaunchMode::OfflineSinglePlayer,
        main_class: MAIN_CLASS,
        java_major: 25,
        java_release: "25.0.5.1+2",
        classpath_entries: 3,
        jvm_arguments: 5,
        game_arguments: 18,
        identity_slots: 3,
    };
    let serialized = serde_json::to_value(summary).unwrap();
    assert_eq!(
        serialized,
        json!({
            "version": "26.3", "platform": "windows-x86_64", "mode": "offlineSinglePlayer",
            "mainClass": "net.minecraft.client.main.Main", "javaMajor": 25, "javaRelease": "25.0.5.1+2",
            "classpathEntries": 3, "jvmArguments": 5, "gameArguments": 18, "identitySlots":3
        })
    );
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CorpusEntry {
    version: VanillaVersion,
    sha1: String,
    sha256: String,
    url: String,
    java_major: u16,
    asset_index: String,
    asset_index_sha1: String,
    windows_classpath: usize,
    linux_classpath: usize,
}

#[test]
#[ignore = "requires independently obtained pinned provider metadata in CARBON_LAUNCH_CORPUS_INPUT"]
fn pinned_official_corpus_is_offline_deterministic() {
    let root = PathBuf::from(
        std::env::var_os("CARBON_LAUNCH_CORPUS_INPUT").expect("set the public metadata directory"),
    );
    let corpus: Vec<CorpusEntry> = serde_json::from_str(include_str!(
        "../../../../../fixtures/launch-corpus/vanilla.json"
    ))
    .unwrap();
    for entry in corpus {
        let profile = profile(entry.version);
        let raw = std::fs::read(root.join(format!("{}.json", entry.version.id()))).unwrap();
        assert_eq!(hex::encode(Sha256::digest(&raw)), entry.sha256);
        let pin = Pin {
            id: entry.version.id().into(),
            kind: crate::release_type(entry.version).into(),
            sha1: entry.sha1,
            url: entry.url,
        };
        let info = verified(&raw, &pin, profile.id, Origin::Cache).unwrap();
        for (label, expected) in [
            ("windows-x86_64", entry.windows_classpath),
            ("linux-x86_64", entry.linux_classpath),
        ] {
            let platform = Platform::launch_platform(label).unwrap();
            let plan = Plan::version(&info, &raw, platform).unwrap();
            assert_eq!(plan.asset_index_id, entry.asset_index);
            assert_eq!(plan.index.sha1, entry.asset_index_sha1);
            assert!(
                plan.native_archives.is_empty(),
                "requires legacy native extraction"
            );
            let view = Path::new("/Synthetic Review/content");
            let classpath: Vec<_> = plan
                .classpath
                .iter()
                .map(|p| view.join(p))
                .chain(std::iter::once(
                    view.join(format!("versions/{0}/{0}.jar", entry.version.id())),
                ))
                .collect();
            assert_eq!(classpath.len(), expected);
            let log = plan.logging_path.map(|p| view.join(p));
            let game = view.with_file_name("game");
            let assets = view.join("assets");
            let build = || {
                arguments::build(
                    &raw,
                    platform,
                    &profile,
                    entry.java_major,
                    Material {
                        game: &game,
                        assets: &assets,
                        asset_index: &plan.asset_index_id,
                        classpath: &classpath,
                        log: log.as_deref(),
                    },
                )
                .unwrap()
            };
            let start = Instant::now();
            let cold = build();
            let cold_ms = start.elapsed().as_secs_f64() * 1000.;
            let start = Instant::now();
            for _ in 0..50 {
                let warm = build();
                assert!(cold.jvm == warm.jvm && cold.game == warm.game);
            }
            let warm_ms = start.elapsed().as_secs_f64() * 1000. / 50.;
            println!("{} {label}: {expected} classpath, {} JVM args, {} game args, {cold_ms:.3} ms first, {warm_ms:.3} ms warmed mean", entry.version.id(), cold.jvm.len(), cold.game.len());
        }
    }
    #[cfg(target_os = "linux")]
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        if let Some(peak) = status.lines().find(|line| line.starts_with("VmHWM:")) {
            println!(
                "Debug test process peak RSS: {}",
                peak["VmHWM:".len()..].trim()
            );
        }
    }
}
