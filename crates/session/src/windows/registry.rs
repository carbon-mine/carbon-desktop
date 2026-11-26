// SPDX-License-Identifier: MPL-2.0
use super::{check_dir, ensure_dir, open_regular, read_small, sandbox};
use crate::reservation::has_pending_run;
use std::{
    fs::{self, File, TryLockError},
    io::{self, ErrorKind, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

const RETIRE_LIMIT: usize = 4;
const MARKER: &str = "owner-v2";
const OWNER_HEADER: &str = "carbon-desktop-sample-session-v2";
const LEGACY_MARKER: &str = "owner-v1";
const OWNER_LOCK: &str = "owner.lock";

pub(super) struct SessionLease {
    registry: PathBuf,
    path: PathBuf,
    id: Uuid,
    instance: Uuid,
    guard: Option<File>,
}

impl SessionLease {
    pub(super) fn begin(saves: &Path, instance: Uuid, id: Uuid) -> io::Result<Self> {
        let registry = saves.join("transient");
        ensure_dir(&registry)?;
        let _gate = gate(&registry)?;
        recover(&registry)?;
        let path = registry.join(id.to_string());
        fs::create_dir(&path)?;
        let mut marker = File::create_new(path.join(MARKER))?;
        marker.write_all(&owner_bytes(id, instance))?;
        marker.sync_all()?;
        let guard = open_regular(&path.join(OWNER_LOCK), true)?;
        guard.lock()?;
        Ok(Self {
            registry,
            path,
            id,
            instance,
            guard: Some(guard),
        })
    }

    pub(super) fn stage(&self) -> PathBuf {
        self.path.join("stage")
    }

    pub(super) fn profile_name(&self, peer: bool) -> String {
        profile_name(self.id, peer)
    }
}

impl Drop for SessionLease {
    fn drop(&mut self) {
        let result = gate(&self.registry).and_then(|_gate| {
            if !can_retire(&self.registry, self.instance)? {
                return Ok(());
            }
            let guard = self.guard.take().expect("active transient lease");
            retire(&self.path, self.id, guard)
        });
        if let Err(error) = result {
            eprintln!(
                "Protected test cleanup will retry on a later session ({:?}).",
                error.kind()
            );
        }
    }
}

fn gate(registry: &Path) -> io::Result<File> {
    check_dir(registry)?;
    let file = open_regular(&registry.join("registry.lock"), true)?;
    file.lock()?;
    Ok(file)
}

fn profile_name(id: Uuid, peer: bool) -> String {
    let role = if peer { "peer" } else { "window" };
    format!("CarbonDesktop.Sample.{}.{role}", id.simple())
}

fn owner_bytes(id: Uuid, instance: Uuid) -> Vec<u8> {
    format!("{OWNER_HEADER}\n{id}\n{instance}\n").into_bytes()
}

fn owner_instance(bytes: &[u8], run: Uuid) -> Option<Uuid> {
    let instance = std::str::from_utf8(bytes)
        .ok()?
        .lines()
        .nth(2)
        .and_then(|field| Uuid::parse_str(field).ok())?;
    (instance.get_version() == Some(uuid::Version::Random) && bytes == owner_bytes(run, instance))
        .then_some(instance)
}

fn can_retire(registry: &Path, instance: Uuid) -> io::Result<bool> {
    let saves = registry
        .parent()
        .ok_or_else(|| io::Error::other("The transient registry has no save root"))?;
    let profile = saves.join(instance.to_string());
    check_dir(&profile)?;
    // A free owner lock proves the host is gone. A pending record makes its
    // process boundary unknown, so retain the transient registration and tree.
    Ok(!has_pending_run(&profile)?)
}

fn legacy_bytes(id: Uuid) -> Vec<u8> {
    format!("carbon-desktop-sample-session-v1\n{id}\n").into_bytes()
}

fn unresolved_legacy_run(path: &Path, id: Uuid) -> bool {
    let expected = legacy_bytes(id);
    if !matches!(
        read_small(&path.join(LEGACY_MARKER), expected.len()),
        Ok(Some(bytes)) if bytes == expected
    ) {
        return false;
    }
    // Older records do not identify the instance. The live owner's instance
    // lock handles contention; a lost owner provides no boundary evidence.
    let Ok(guard) = open_regular(&path.join(OWNER_LOCK), false) else {
        return true;
    };
    !matches!(guard.try_lock(), Err(TryLockError::WouldBlock))
}

fn recover(registry: &Path) -> io::Result<()> {
    let mut retired = 0;
    for item in fs::read_dir(registry)? {
        let item = item?;
        let name = item.file_name();
        let Some(name) = name.to_str() else { continue };
        let Ok(id) = Uuid::parse_str(name) else {
            continue;
        };
        if id.get_version() != Some(uuid::Version::Random) || name != id.to_string() {
            continue;
        }
        let path = item.path();
        if check_dir(&path).is_err() {
            continue;
        }
        if unresolved_legacy_run(&path, id) {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "A previous sample has an unresolved legacy run",
            ));
        }
        if retired == RETIRE_LIMIT {
            continue;
        }
        let Ok(Some(record)) = read_small(&path.join(MARKER), owner_bytes(id, id).len()) else {
            continue;
        };
        let Some(instance) = owner_instance(&record, id) else {
            continue;
        };
        let Ok(guard) = open_regular(&path.join(OWNER_LOCK), true) else {
            continue;
        };
        match guard.try_lock() {
            Ok(()) => (),
            Err(TryLockError::WouldBlock | TryLockError::Error(_)) => continue,
        }
        if !matches!(can_retire(registry, instance), Ok(true)) {
            continue;
        }
        if let Err(error) = retire(&path, id, guard) {
            eprintln!(
                "Protected test orphan cleanup will retry ({:?}).",
                error.kind()
            );
        }
        retired += 1;
    }
    Ok(())
}

fn retire(path: &Path, id: Uuid, guard: File) -> io::Result<()> {
    // Keep the durable marker until both registrations and the protected tree
    // have been retired. Only the derived names and fixed stage are removed.
    sandbox::delete_profile(&profile_name(id, false))?;
    sandbox::delete_profile(&profile_name(id, true))?;
    let stage = path.join("stage");
    match check_dir(&stage) {
        Ok(()) => fs::remove_dir_all(stage)?,
        Err(error) if error.kind() == ErrorKind::NotFound => (),
        Err(error) => return Err(error),
    }
    drop(guard);
    fs::remove_file(path.join(OWNER_LOCK))?;
    fs::remove_file(path.join(MARKER))?;
    fs::remove_dir(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reservation::RunReservation;

    #[test]
    fn recovery_retains_live_and_unmarked_records_and_retires_a_leased_orphan() {
        let root = tempfile::tempdir().unwrap();
        let instance = Uuid::new_v4();
        let instance_dir = root.path().join(instance.to_string());
        ensure_dir(&instance_dir).unwrap();
        let live = SessionLease::begin(root.path(), instance, Uuid::new_v4()).unwrap();
        fs::create_dir(live.stage()).unwrap();
        let unknown = live.registry.join(Uuid::new_v4().to_string());
        fs::create_dir(&unknown).unwrap();
        fs::write(unknown.join(MARKER), b"not owned").unwrap();

        let id = Uuid::new_v4();
        let path = live.registry.join(id.to_string());
        fs::create_dir(&path).unwrap();
        fs::write(path.join(MARKER), owner_bytes(id, instance)).unwrap();
        let abandoned_guard = open_regular(&path.join(OWNER_LOCK), true).unwrap();
        abandoned_guard.lock().unwrap();
        fs::create_dir(path.join("stage")).unwrap();
        fs::write(path.join("stage/witness"), b"abandoned").unwrap();
        let peer = profile_name(id, true);
        drop(sandbox::Profile::new(&peer).unwrap());
        drop(abandoned_guard);

        let next = SessionLease::begin(root.path(), instance, Uuid::new_v4()).unwrap();
        assert!(live.stage().exists());
        assert!(unknown.exists());
        assert!(!path.exists());
        let recreated = sandbox::Profile::new(&peer).unwrap();
        drop(recreated);
        sandbox::delete_profile(&peer).unwrap();
        let next_path = next.path.clone();
        drop(next);
        assert!(!next_path.exists());
    }

    #[test]
    fn unresolved_legacy_owner_fences_admission_without_removing_files() {
        let root = tempfile::tempdir().unwrap();
        let instance = Uuid::new_v4();
        let registry = root.path().join("transient");
        ensure_dir(&root.path().join(instance.to_string())).unwrap();
        ensure_dir(&registry).unwrap();
        let id = Uuid::new_v4();
        let legacy = registry.join(id.to_string());
        ensure_dir(&legacy).unwrap();
        fs::write(legacy.join(LEGACY_MARKER), legacy_bytes(id)).unwrap();
        let legacy_guard = open_regular(&legacy.join(OWNER_LOCK), true).unwrap();
        legacy_guard.lock().unwrap();
        let live = SessionLease::begin(root.path(), instance, Uuid::new_v4()).unwrap();
        let live_path = live.path.clone();

        drop(legacy_guard);
        assert_eq!(
            SessionLease::begin(root.path(), instance, Uuid::new_v4())
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(
            fs::read(legacy.join(LEGACY_MARKER)).unwrap(),
            legacy_bytes(id)
        );
        assert!(live_path.exists());
        drop(live);
        assert!(!live_path.exists());
    }

    #[test]
    fn recovery_retains_admitted_tree_without_drain_proof() {
        let root = tempfile::tempdir().unwrap();
        let instance = Uuid::new_v4();
        let path = root.path().join(instance.to_string());
        ensure_dir(&path).unwrap();
        let guard = open_regular(&path.join("sample.lock"), true).unwrap();
        let run = RunReservation::acquire(&path, guard).unwrap();
        let lease = SessionLease::begin(root.path(), instance, run.id()).unwrap();
        let tree = lease.stage();
        fs::create_dir(&tree).unwrap();
        fs::write(tree.join("witness"), b"pending").unwrap();
        run.record_admission().unwrap();
        drop(lease);
        drop(run);

        let other = Uuid::new_v4();
        ensure_dir(&root.path().join(other.to_string())).unwrap();
        let _next = SessionLease::begin(root.path(), other, Uuid::new_v4()).unwrap();
        assert_eq!(fs::read(tree.join("witness")).unwrap(), b"pending");
        assert!(matches!(
            RunReservation::acquire(
                &path,
                open_regular(&path.join("sample.lock"), true).unwrap()
            ),
            Err(crate::SessionError::RecoveryRequired)
        ));
    }
}
