// SPDX-License-Identifier: MPL-2.0
use super::{check_dir, ensure_dir, open_regular, read_small, sandbox};
use std::{
    fs::{self, File, TryLockError},
    io::{self, ErrorKind, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

const SCAN_LIMIT: usize = 64;
const RETIRE_LIMIT: usize = 4;
const MARKER: &str = "owner-v1";
const OWNER_LOCK: &str = "owner.lock";

pub(super) struct SessionLease {
    registry: PathBuf,
    path: PathBuf,
    id: Uuid,
    guard: Option<File>,
}

impl SessionLease {
    pub(super) fn begin(saves: &Path) -> io::Result<Self> {
        let registry = saves.join("transient");
        ensure_dir(&registry)?;
        let _gate = gate(&registry)?;
        recover(&registry)?;
        let id = Uuid::new_v4();
        let path = registry.join(id.to_string());
        fs::create_dir(&path)?;
        let mut marker = File::create_new(path.join(MARKER))?;
        marker.write_all(&owner_bytes(id))?;
        marker.sync_all()?;
        let guard = open_regular(&path.join(OWNER_LOCK), true)?;
        guard.lock()?;
        Ok(Self {
            registry,
            path,
            id,
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

fn owner_bytes(id: Uuid) -> Vec<u8> {
    format!("carbon-desktop-sample-session-v1\n{id}\n").into_bytes()
}

fn recover(registry: &Path) -> io::Result<()> {
    let mut retired = 0;
    for item in fs::read_dir(registry)?.take(SCAN_LIMIT) {
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
        let expected = owner_bytes(id);
        if !matches!(read_small(&path.join(MARKER), expected.len()), Ok(Some(found)) if found == expected)
        {
            continue;
        }
        let Ok(guard) = open_regular(&path.join(OWNER_LOCK), true) else {
            continue;
        };
        match guard.try_lock() {
            Ok(()) => (),
            Err(TryLockError::WouldBlock | TryLockError::Error(_)) => continue,
        }
        if let Err(error) = retire(&path, id, guard) {
            eprintln!(
                "Protected test orphan cleanup will retry ({:?}).",
                error.kind()
            );
        }
        retired += 1;
        if retired == RETIRE_LIMIT {
            break;
        }
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

    #[test]
    fn recovery_retains_live_and_unmarked_records_and_retires_a_leased_orphan() {
        let root = tempfile::tempdir().unwrap();
        let live = SessionLease::begin(root.path()).unwrap();
        fs::create_dir(live.stage()).unwrap();
        let unknown = live.registry.join(Uuid::new_v4().to_string());
        fs::create_dir(&unknown).unwrap();
        fs::write(unknown.join(MARKER), b"not owned").unwrap();

        let id = Uuid::new_v4();
        let path = live.registry.join(id.to_string());
        fs::create_dir(&path).unwrap();
        fs::write(path.join(MARKER), owner_bytes(id)).unwrap();
        let abandoned_guard = open_regular(&path.join(OWNER_LOCK), true).unwrap();
        abandoned_guard.lock().unwrap();
        fs::create_dir(path.join("stage")).unwrap();
        fs::write(path.join("stage/witness"), b"abandoned").unwrap();
        let peer = profile_name(id, true);
        drop(sandbox::Profile::new(&peer).unwrap());
        drop(abandoned_guard);

        let next = SessionLease::begin(root.path()).unwrap();
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
}
