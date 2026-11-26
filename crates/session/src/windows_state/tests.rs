// SPDX-License-Identifier: MPL-2.0
use super::*;

fn fixture() -> (tempfile::TempDir, Uuid, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let id = Uuid::new_v4();
    let instance = root.path().join("instances-v1").join(id.to_string());
    fs::create_dir_all(instance.join("game")).unwrap();
    (root, id, instance)
}

fn digest(path: &Path) -> [u8; 32] {
    let object = acl::open(path, false).unwrap();
    Acl::read(&object.file).unwrap().digest()
}

fn without(path: &Path, instance: Uuid, run: Uuid, inherited_only: bool) -> [u8; 32] {
    let object = acl::open(path, false).unwrap();
    let sid = acl::run_sid(&profile_name(instance, run)).unwrap();
    Acl::read(&object.file)
        .unwrap()
        .stripped(&sid, object.directory, inherited_only)
        .unwrap()
        .digest()
}

fn emulate_root_cleanup(lease: &StateLease, run: Uuid) {
    let sid = acl::run_sid(&profile_name(lease.instance, run)).unwrap();
    let root = &lease.state.file;
    Acl::read(root)
        .unwrap()
        .stripped(&sid, true, false)
        .unwrap()
        .write(root)
        .unwrap();
}

#[test]
fn durable_state_receives_and_retires_only_the_run_grant() {
    let (root, id, instance) = fixture();
    let canary = instance.join("game").join("existing-world.bin");
    fs::write(&canary, b"authored progress").unwrap();
    let canary_acl = digest(&canary);
    let foreign_id = Uuid::new_v4();
    fs::create_dir(
        root.path()
            .join("instances-v1")
            .join(foreign_id.to_string()),
    )
    .unwrap();
    let foreign = StateLease::acquire(root.path(), foreign_id).unwrap();
    let foreign_acl = digest(&foreign.state_path);

    let mut lease = StateLease::acquire(root.path(), id).unwrap();
    assert!(
        matches!(StateLease::acquire(root.path(), id), Err(error) if error.kind() == ErrorKind::WouldBlock)
    );
    let previous = digest(&lease.state_path);
    let run = Uuid::new_v4();
    let state = lease.prepare(run).unwrap();
    assert_eq!(lease.unfinished_run().unwrap(), Some(run));
    assert_ne!(digest(&state), previous);
    assert_eq!(without(&state, id, run, false), previous);
    assert_ne!(
        acl::run_sid(&profile_name(id, run)).unwrap(),
        acl::run_sid(&profile_name(foreign_id, run)).unwrap()
    );

    let world = state.join("saves").join("sample");
    fs::create_dir_all(&world).unwrap();
    let level = world.join("level.dat");
    fs::write(&level, b"new world").unwrap();
    let child_after_retire = without(&level, id, run, true);
    assert_ne!(digest(&level), child_after_retire);
    assert!(lease.retire_after_drain(Uuid::new_v4()).is_err());
    assert!(matches!(lease.recover_unarmed(), Err(error) if error.kind() == ErrorKind::WouldBlock));

    lease.retire_after_drain(run).unwrap();
    assert_eq!(digest(&state), previous);
    assert_eq!(digest(&level), child_after_retire);
    assert_eq!(fs::read(&level).unwrap(), b"new world");
    assert_eq!(digest(&canary), canary_acl);
    assert_eq!(fs::read(&canary).unwrap(), b"authored progress");
    assert_eq!(digest(&foreign.state_path), foreign_acl);

    let next = Uuid::new_v4();
    assert_ne!(profile_name(id, run), profile_name(id, next));
    lease.prepare(next).unwrap();
    assert_eq!(digest(&level), without(&level, id, run, false));
    assert_ne!(digest(&level), child_after_retire);
    lease.retire_after_drain(next).unwrap();
    assert_eq!(digest(&level), child_after_retire);
}

#[test]
fn recovery_accepts_a_partially_applied_unarmed_journal() {
    let (root, id, _) = fixture();
    let mut lease = StateLease::acquire(root.path(), id).unwrap();
    let object = lease.state_path.join("options.txt");
    fs::write(&object, b"selected").unwrap();
    let before = digest(&object);
    let run = Uuid::new_v4();
    lease.prepare(run).unwrap();
    let mut journal = lease.journal().unwrap().unwrap();
    journal.armed = false;
    write_json(&lease.control_path, JOURNAL, &journal).unwrap();
    emulate_root_cleanup(&lease, run);
    drop(lease);

    let mut lease = StateLease::acquire(root.path(), id).unwrap();
    lease.recover_unarmed().unwrap();
    assert_eq!(lease.unfinished_run().unwrap(), None);
    assert_eq!(digest(&object), before);
    assert_eq!(fs::read(&object).unwrap(), b"selected");
}

#[test]
fn armed_recovery_and_concurrent_foreign_permissions_fail_closed() {
    let (root, id, _) = fixture();
    let mut lease = StateLease::acquire(root.path(), id).unwrap();
    let file = lease.state_path.join("protected.txt");
    fs::write(&file, b"keep").unwrap();
    let run = Uuid::new_v4();
    lease.prepare(run).unwrap();
    let base = lease.state_path.clone();
    drop(lease);

    let mut lease = StateLease::acquire(root.path(), id).unwrap();
    assert!(lease.prepare(Uuid::new_v4()).is_err());
    assert_eq!(lease.unfinished_run().unwrap(), Some(run));

    let root_after = digest(&base);
    let object = acl::open(&file, true).unwrap();
    let original = Acl::read(&object.file).unwrap();
    let different_sid = acl::run_sid(&profile_name(id, Uuid::new_v4())).unwrap();
    let edited = original.granted(&different_sid, false).unwrap();
    edited.write(&object.file).unwrap();
    let edited_hash = digest(&file);
    assert!(lease.retire_after_drain(run).is_err());
    assert_eq!(digest(&file), edited_hash);
    assert_eq!(digest(&base), root_after);
    assert_eq!(fs::read(&file).unwrap(), b"keep");
    assert_eq!(lease.unfinished_run().unwrap(), Some(run));
    emulate_root_cleanup(&lease, run);
    original.write(&object.file).unwrap();
    drop(object);
    lease.recover_unarmed().unwrap();
}

#[test]
fn hard_link_and_root_substitution_never_touch_foreign_data() {
    let (root, id, instance) = fixture();
    let mut lease = StateLease::acquire(root.path(), id).unwrap();
    let foreign = instance.join("game").join("canary.txt");
    fs::write(&foreign, b"outside").unwrap();
    let original = digest(&foreign);
    let link = lease.state_path.join("alias.txt");
    fs::hard_link(&foreign, &link).unwrap();
    assert!(lease.prepare(Uuid::new_v4()).is_err());
    assert_eq!(lease.unfinished_run().unwrap(), None);
    fs::remove_file(&link).unwrap();
    assert_eq!(digest(&foreign), original);

    let state = lease.state_path.clone();
    drop(lease);
    let moved = instance.join("renamed");
    fs::rename(&state, &moved).unwrap();
    acl::create_low_directory(&state).unwrap();
    assert!(StateLease::acquire(root.path(), id).is_err());
    assert_eq!(fs::read(&foreign).unwrap(), b"outside");
}

#[test]
fn substituted_journal_sid_is_refused_without_mutating_permissions() {
    let (root, id, _) = fixture();
    let mut lease = StateLease::acquire(root.path(), id).unwrap();
    let run = Uuid::new_v4();
    let state = lease.prepare(run).unwrap();
    let granted = digest(&state);
    let mut journal = lease.journal().unwrap().unwrap();
    let original_sid = journal.sid.clone();
    journal.sid = acl::run_sid(&profile_name(id, Uuid::new_v4())).unwrap();
    write_json(&lease.control_path, JOURNAL, &journal).unwrap();
    assert!(lease.retire_after_drain(run).is_err());
    assert_eq!(digest(&state), granted);
    journal.sid = original_sid;
    write_json(&lease.control_path, JOURNAL, &journal).unwrap();
    lease.retire_after_drain(run).unwrap();
}

#[test]
fn preexisting_package_access_is_preserved_and_blocks_preparation() {
    let (root, id, _) = fixture();
    let mut lease = StateLease::acquire(root.path(), id).unwrap();
    let foreign = acl::run_sid(&profile_name(id, Uuid::new_v4())).unwrap();
    let scoped = Acl::read(&lease.state.file)
        .unwrap()
        .granted(&foreign, true)
        .unwrap();
    scoped.write(&lease.state.file).unwrap();
    assert!(lease.prepare(Uuid::new_v4()).is_err());
    assert_eq!(lease.unfinished_run().unwrap(), None);
    assert_eq!(digest(&lease.state_path), scoped.digest());
}

#[test]
fn atomic_save_replacement_retains_state_and_blocks_retirement_and_relaunch() {
    let (root, id, instance) = fixture();
    let mut lease = StateLease::acquire(root.path(), id).unwrap();
    let initial_run = Uuid::new_v4();
    let state = lease.prepare(initial_run).unwrap();
    let world = state.join("saves").join("sample");
    fs::create_dir_all(&world).unwrap();
    let level = world.join("level.dat");
    fs::write(&level, b"first save").unwrap();
    lease.retire_after_drain(initial_run).unwrap();

    let run = Uuid::new_v4();
    lease.prepare(run).unwrap();
    let old_identity = acl::open(&level, false).unwrap().identity;
    let staged = world.join("level.dat_new");
    fs::write(&staged, b"replacement save").unwrap();
    let replacement = acl::open(&staged, false).unwrap().identity;
    assert_ne!(old_identity, replacement);
    fs::rename(&staged, &level).unwrap();
    assert_eq!(acl::open(&level, false).unwrap().identity, replacement);

    let canary = instance.join("game").join("untouched.dat");
    fs::write(&canary, b"prior authored state").unwrap();
    let before = [
        digest(&state),
        digest(&world),
        digest(&level),
        digest(&canary),
    ];
    assert!(lease
        .retire_after_drain(run)
        .unwrap_err()
        .to_string()
        .contains("identity changed"));
    drop(lease);
    let mut lease = StateLease::acquire(root.path(), id).unwrap();
    assert_eq!(lease.unfinished_run().unwrap(), Some(run));
    assert!(!lease.journal().unwrap().unwrap().armed);
    assert!(lease.recover_unarmed().is_err());
    assert!(lease.prepare(Uuid::new_v4()).is_err());
    assert_eq!(lease.unfinished_run().unwrap(), Some(run));
    assert_eq!(
        [
            digest(&state),
            digest(&world),
            digest(&level),
            digest(&canary),
        ],
        before
    );
    assert_eq!(fs::read(&level).unwrap(), b"replacement save");
    assert_eq!(fs::read(&canary).unwrap(), b"prior authored state");
    assert!(!staged.exists());
}

#[test]
fn unmarked_state_survives_an_interrupted_initial_enrollment() {
    let (root, id, instance) = fixture();
    let state = instance.join(STATE);
    acl::create_low_directory(&state).unwrap();
    let unfinished = state.join("options.txt");
    fs::write(&unfinished, b"orphan canary").unwrap();
    let before = (digest(&state), digest(&unfinished));

    for _ in 0..2 {
        assert!(StateLease::acquire(root.path(), id).is_err());
        assert!(!instance.join(CONTROL).join(OWNER).exists());
        assert!(!instance.join(CONTROL).join(JOURNAL).exists());
        assert_eq!((digest(&state), digest(&unfinished)), before);
        assert_eq!(fs::read(&unfinished).unwrap(), b"orphan canary");
    }
}
