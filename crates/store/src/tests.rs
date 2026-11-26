mod http;

use super::*;
use std::cell::Cell;
use std::io;
use std::process::Command;
use tempfile::TempDir;
use Action::*;
use InstanceId::{Overlap, Repeat, Sample};

const KIB: u64 = 1024;

fn root() -> TempDir {
    let target = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/fixture-tests");
    fs::create_dir_all(&target).unwrap();
    tempfile::tempdir_in(target).unwrap()
}
fn a(root: &Path, action: Action) -> Snapshot {
    apply(root, action).unwrap()
}
fn view(snapshot: &Snapshot, id: InstanceId) -> &InstanceView {
    snapshot.instances.iter().find(|p| p.id == id).unwrap()
}
fn staged(root: &Path) {
    a(root, CreateSample);
    a(root, Action::Review { instance: Sample });
    a(root, Stage { instance: Sample });
}

#[test]
fn fixture_workload_measures_local_reads_and_unique_storage() {
    let work = root();
    let root = work.path();
    assert!(snapshot(root).unwrap().instances.is_empty());
    assert!(apply(root, CreateRepeat).is_err());
    let mut prev = (0, 0);
    for (name, action, source, reuse, objects, logical) in [
        ("cold", CreateSample, 80, 0, 80, 80),
        ("exact repeat", CreateRepeat, 0, 80, 80, 160),
        ("overlap", CreateOverlap, 16, 64, 96, 240),
        ("review", Action::Review { instance: Sample }, 0, 0, 96, 240),
        ("staged update", Stage { instance: Sample }, 4, 76, 100, 240),
        ("activated", Activate { instance: Sample }, 0, 0, 100, 240),
    ] {
        let m = a(root, action).metrics;
        assert_eq!(
            (m.source_bytes - prev.0, m.reused_bytes - prev.1),
            (source * KIB, reuse * KIB)
        );
        assert_eq!(
            (m.object_bytes, m.active_logical_bytes),
            (objects * KIB, logical * KIB)
        );
        println!("{name}: sourceDelta={} reuseDelta={} cacheLogical={} cacheAllocation={:?} activeLogical={}",
            m.source_bytes - prev.0, m.reused_bytes-prev.1, m.object_bytes,m.allocated_object_bytes,m.active_logical_bytes);
        prev = (m.source_bytes, m.reused_bytes);
    }
}

#[test]
fn snapshot_and_actions_use_the_documented_wire_contract() {
    let work = root();
    let value = serde_json::to_value(a(
        work.path(),
        serde_json::from_str(r#"{"kind":"createSample"}"#).unwrap(),
    ))
    .unwrap();
    assert_eq!(value["instances"][0]["id"], "sample");
    assert_eq!(value["instances"][0]["generation"], "A");
    assert!(value["instances"][0]["healthy"].as_bool().unwrap());
    assert!(value["instances"][0]["saveRevision"].is_number());
    assert!(value["metrics"]["objectBytes"].is_number());
    assert!(
        serde_json::from_str::<Action>(r#"{"kind":"review","instance":"../../worlds"}"#).is_err()
    );
    assert!(serde_json::from_str::<Action>(
        r#"{"kind":"review","instance":"sample","path":"/tmp/out"}"#
    )
    .is_err());
    assert!(serde_json::from_str::<Action>(r#"{"kind":"run","command":"demo"}"#).is_err());
}

#[test]
fn review_cancel_and_resume_leave_generation_and_player_data_intact() {
    let work = root();
    let root = work.path();
    a(root, CreateSample);
    a(root, CreateOverlap);
    a(root, EditSave { instance: Sample });
    a(root, EditSetting { instance: Sample });
    let review = a(root, Action::Review { instance: Sample });
    assert_eq!(
        view(&review, Sample).review.as_ref().unwrap().added,
        ["Rain B"]
    );
    assert!(apply(root, Activate { instance: Sample }).is_err());
    a(root, Interrupt { instance: Sample });
    let restart = snapshot(root).unwrap();
    assert_eq!(view(&restart, Sample).phase, Phase::Interrupted);
    assert_eq!(view(&restart, Sample).generation, Generation::A);
    a(root, Cancel { instance: Sample });
    assert!(apply(root, Resume { instance: Sample }).is_err());
    a(root, Action::Review { instance: Sample });
    a(root, Interrupt { instance: Sample });
    a(root, Resume { instance: Sample });
    let active = a(root, Activate { instance: Sample });
    assert_eq!(view(&active, Sample).generation, Generation::B);
    assert_eq!(
        (
            view(&active, Sample).save_revision,
            view(&active, Sample).setting
        ),
        (1, 1)
    );
    assert_eq!(
        (
            view(&active, Overlap).save_revision,
            view(&active, Overlap).setting
        ),
        (0, 0)
    );
    assert_eq!(view(&active, Overlap).generation, Generation::A);
    let rolled = a(root, Rollback { instance: Sample });
    assert_eq!(view(&rolled, Sample).generation, Generation::A);
    a(root, EditSave { instance: Sample });
    a(root, EditSetting { instance: Sample });
    a(root, Action::Review { instance: Sample });
    a(root, Stage { instance: Sample });
    a(root, Activate { instance: Sample });
    a(root, EditSave { instance: Sample });
    assert!(apply(root, Rollback { instance: Sample })
        .unwrap_err()
        .contains("save was edited"));
    assert_eq!(view(&snapshot(root).unwrap(), Sample).save_revision, 3);
    let backup: Backup = read_json(&root.join("backups/sample-before-b.json")).unwrap();
    assert_eq!((backup.world.revision, backup.setting), (2, 2));
}

#[test]
fn missing_and_same_length_corruption_are_detected_and_repaired() {
    let work = root();
    let root = work.path();
    a(root, CreateSample);
    a(root, CreateRepeat);
    a(root, EditSave { instance: Repeat });
    for action in [
        SimulateMissing { instance: Sample },
        SimulateCorruption { instance: Sample },
    ] {
        let damaged = a(root, action);
        assert!(damaged.instances.iter().all(|p| !p.healthy));
        let repaired = a(root, Repair { instance: Repeat });
        assert!(repaired.instances.iter().all(|p| p.healthy));
        assert_eq!(
            repaired.metrics.source_bytes - damaged.metrics.source_bytes,
            4 * KIB
        );
        assert_eq!(view(&repaired, Repeat).save_revision, 1);
    }
    a(root, Action::Review { instance: Sample });
    a(root, Stage { instance: Sample });
    fs::write(root.join("objects").join(ALL[5].digest), b"partial").unwrap();
    assert!(apply(root, Activate { instance: Sample }).is_err());
    assert_eq!(
        view(&snapshot(root).unwrap(), Sample).generation,
        Generation::A
    );
    a(root, Repair { instance: Sample });
    a(root, SimulateMissing { instance: Sample });
    assert!(apply(root, Activate { instance: Sample })
        .unwrap_err()
        .contains("rollback"));
    a(root, Repair { instance: Sample });
    a(root, Activate { instance: Sample });
}

#[test]
fn content_reader_defers_activation_and_shared_damage() {
    let work = root();
    let root = work.path();
    staged(root);
    let lease = acquire_content_lease(root, Sample).unwrap();
    assert!(apply(root, Activate { instance: Sample })
        .unwrap_err()
        .contains("reader"));
    assert_eq!(view(&snapshot(root).unwrap(), Sample).phase, Phase::Staged);
    a(root, CreateRepeat);
    assert!(apply(root, SimulateMissing { instance: Repeat }).is_err());
    drop(lease);
    a(root, Activate { instance: Sample });
}

#[test]
fn concurrent_instances_and_player_edits_use_one_store_lock() {
    let work = root();
    let root = work.path();
    a(root, CreateSample);
    std::thread::scope(|s| {
        s.spawn(|| a(root, CreateRepeat));
        s.spawn(|| a(root, CreateOverlap));
        for _ in 0..6 {
            s.spawn(|| a(root, EditSetting { instance: Sample }));
        }
    });
    let after = snapshot(root).unwrap();
    assert_eq!(after.instances.len(), 3);
    assert_eq!(view(&after, Sample).setting, 6);
    assert_eq!(after.metrics.source_bytes, 96 * KIB);
    assert_eq!(after.metrics.object_count, 5);
}

#[derive(Clone, Copy, PartialEq)]
enum Fault {
    None,
    Full,
    BeforeActivation,
    AfterActivation,
    DuringStage,
    AfterAction,
}
thread_local! {static FAULT: Cell<Fault> = const { Cell::new(Fault::None) };}

fn active_publication(path: &Path, bytes: &[u8]) -> bool {
    path.ends_with("library.json")
        && serde_json::from_slice::<Library>(bytes)
            .ok()
            .and_then(|l| l.instances.get(&Sample).copied())
            .is_some_and(|p| p.generation == Generation::B)
}
pub(crate) fn fault_before_publish(path: &Path, bytes: &[u8]) -> io::Result<()> {
    match FAULT.get() {
        Fault::Full if path.ends_with(ALL[5].digest) => {
            Err(io::Error::from(io::ErrorKind::StorageFull))
        }
        Fault::BeforeActivation if active_publication(path, bytes) => std::process::exit(23),
        _ => Ok(()),
    }
}
pub(crate) fn fault_after_publish(path: &Path, bytes: &[u8]) {
    match FAULT.get() {
        Fault::AfterActivation if active_publication(path, bytes) => std::process::exit(23),
        Fault::DuringStage if path.ends_with(ALL[5].digest) => std::process::exit(23),
        _ => {}
    }
}

#[test]
fn simulated_full_disk_keeps_old_state_and_allows_retry() {
    let work = root();
    let root = work.path();
    a(root, CreateSample);
    a(root, Action::Review { instance: Sample });
    FAULT.set(Fault::Full);
    let error = apply(root, Stage { instance: Sample }).unwrap_err();
    FAULT.set(Fault::None);
    assert!(error.contains("Could not publish fixture file"));
    let old = snapshot(root).unwrap();
    assert_eq!(view(&old, Sample).generation, Generation::A);
    assert_eq!(view(&old, Sample).phase, Phase::Interrupted);
    assert!(view(&old, Sample).healthy);
    a(root, Resume { instance: Sample });
    a(root, Activate { instance: Sample });
}

#[test]
fn subprocess_crash_helper() {
    let Ok(path) = std::env::var("CARBON_STORE_TEST_DIR") else {
        return;
    };
    let fault = match std::env::var("CARBON_STORE_TEST_POINT").unwrap().as_str() {
        "stage" => Fault::DuringStage,
        "before" => Fault::BeforeActivation,
        "after" => Fault::AfterActivation,
        _ => panic!("unexpected test point"),
    };
    FAULT.set(fault);
    let action = if fault == Fault::DuringStage {
        Stage { instance: Sample }
    } else {
        Activate { instance: Sample }
    };
    let _ = a(Path::new(&path), action);
    panic!("fixture process did not stop at the expected publication");
}
#[test]
fn abrupt_process_exit_during_stage_and_activation_reopens_complete_generation() {
    for point in ["stage", "before", "after"] {
        let work = root();
        let root = work.path();
        a(root, CreateSample);
        a(root, EditSave { instance: Sample });
        a(root, Action::Review { instance: Sample });
        if point != "stage" {
            a(root, Stage { instance: Sample });
        }
        let result = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tests::subprocess_crash_helper"])
            .env("CARBON_STORE_TEST_DIR", root)
            .env("CARBON_STORE_TEST_POINT", point)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(23));
        let reopened = snapshot(root).unwrap();
        let profile = view(&reopened, Sample);
        assert!(profile.healthy);
        assert_eq!(profile.save_revision, 1);
        if point == "after" {
            assert_eq!(
                (profile.generation, profile.phase),
                (Generation::B, Phase::Idle)
            );
        } else {
            assert_eq!(profile.generation, Generation::A);
            if point == "stage" {
                a(root, Resume { instance: Sample });
            } else {
                assert_eq!(profile.phase, Phase::Staged);
            }
            a(root, Activate { instance: Sample });
        }
    }
}

#[test]
fn malformed_metadata_and_missing_player_data_are_preserved() {
    let work = root();
    let root = work.path();
    a(root, CreateSample);
    let settings = root.join("players/sample/settings.json");
    fs::write(&settings, b"player-data").unwrap();
    assert!(apply(root, EditSetting { instance: Sample }).is_err());
    assert_eq!(fs::read(&settings).unwrap(), b"player-data");
    fs::remove_file(&settings).unwrap();
    assert!(snapshot(root).is_err());
    assert!(!settings.exists());
    let index = root.join("library.json");
    fs::write(&index, b"unexpected").unwrap();
    assert!(snapshot(root).is_err());
    assert_eq!(fs::read(&index).unwrap(), b"unexpected");
    fs::remove_file(&index).unwrap();
    assert!(snapshot(root).is_err());
    assert!(!index.exists());
}

#[test]
fn preexisting_nonfile_object_is_refused() {
    let work = root();
    let root = work.path();
    snapshot(root).unwrap();
    let bad = root.join("objects").join(ALL[0].digest);
    fs::create_dir(&bad).unwrap();
    assert!(apply(root, CreateSample).is_err());
    assert!(bad.is_dir());
}

#[cfg(unix)]
#[test]
fn symlink_fixture_file_cannot_redirect_write() {
    use std::os::unix::fs::symlink;
    let work = root();
    let root = work.path();
    a(root, CreateSample);
    let target = root.join("players/sample/settings.json");
    let other = root.join("outside-fixture.txt");
    fs::write(&other, b"unrelated").unwrap();
    fs::remove_file(&target).unwrap();
    symlink(&other, &target).unwrap();
    assert!(apply(root, EditSetting { instance: Sample }).is_err());
    assert_eq!(fs::read(other).unwrap(), b"unrelated");
}

pub(crate) fn fault_before_result_refresh(root: &Path) {
    if FAULT.get() == Fault::AfterAction {
        fs::write(
            root.join("players/repeat/settings.json"),
            b"external failure",
        )
        .unwrap();
    }
}

#[test]
fn unreadable_sibling_blocks_mutation_and_postcommit_refresh_is_distinguished() {
    let temp = root();
    let root = temp.path();
    a(root, CreateSample);
    a(root, CreateRepeat);
    let damaged = root.join("players/repeat/settings.json");
    fs::write(&damaged, b"unreadable").unwrap();
    assert!(apply(root, EditSave { instance: Sample }).is_err());
    let world = root.join("players/sample/sample-save.json");
    assert_eq!(read_json::<Save>(&world).unwrap().revision, 0);
    fs::write(&damaged, b"0").unwrap();
    FAULT.set(Fault::AfterAction);
    let result = apply(root, EditSave { instance: Sample });
    FAULT.set(Fault::None);
    assert!(result.unwrap_err().contains("action completed"));
    assert_eq!(read_json::<Save>(&world).unwrap().revision, 1);
    fs::write(&damaged, b"0").unwrap();
    assert_eq!(view(&snapshot(root).unwrap(), Sample).save_revision, 1);
}
