// SPDX-License-Identifier: MPL-2.0
use crate::SessionError;
use std::{
    fs::{self, File, TryLockError},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

const PENDING: &str = "pending-run-v1";

pub(crate) struct RunReservation {
    directory: PathBuf,
    id: Uuid,
    _guard: File,
}

impl RunReservation {
    /// The lock file is opened and validated by the owning platform.
    pub(crate) fn acquire(directory: &Path, guard: File) -> Result<Self, SessionError> {
        guard.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => SessionError::Busy,
            TryLockError::Error(error) => SessionError::Data(error),
        })?;
        if has_pending_run(directory).map_err(SessionError::Data)? {
            return Err(SessionError::RecoveryRequired);
        }
        Ok(Self {
            directory: directory.to_owned(),
            id: Uuid::new_v4(),
            _guard: guard,
        })
    }

    pub(crate) fn id(&self) -> Uuid {
        self.id
    }

    pub(crate) fn record_admission(&self) -> io::Result<()> {
        let mut record = tempfile::NamedTempFile::new_in(&self.directory)?;
        record.write_all(&self.record_bytes())?;
        record.as_file().sync_all()?;
        record
            .persist_noclobber(self.directory.join(PENDING))
            .map_err(|error| error.error)?;
        Ok(())
    }

    /// The caller must have observed the current run's OS boundary empty.
    /// Dropping the reservation alone does not prove the boundary has drained.
    pub(crate) fn confirm_empty(
        &self,
        open_marker: impl FnOnce(&Path) -> io::Result<File>,
    ) -> Result<(), SessionError> {
        let path = self.directory.join(PENDING);
        let expected = self.record_bytes();
        let mut actual = Vec::new();
        open_marker(&path)
            .and_then(|file| {
                file.take(expected.len() as u64 + 1)
                    .read_to_end(&mut actual)
            })
            .map_err(SessionError::Data)?;
        if actual != expected {
            return Err(SessionError::RecoveryRequired);
        }
        fs::remove_file(path).map_err(SessionError::Data)
    }

    fn record_bytes(&self) -> Vec<u8> {
        format!("carbon-desktop-sample-run-v1\n{}\n", self.id).into_bytes()
    }
}

/// Every present object fences admission, including an invalid or damaged record.
pub(crate) fn has_pending_run(directory: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(directory.join(PENDING)) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, process::Command};

    const DIRECTORY: &str = "CARBON_TEST_RESERVATION_DIRECTORY";
    const OTHER: &str = "CARBON_TEST_RESERVATION_OTHER";
    const OWNER_EXITED: i32 = 71;
    const LEASE_PROBE_OK: i32 = 72;

    fn reserve(directory: &Path) -> Result<RunReservation, SessionError> {
        let guard = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join("sample.lock"))
            .unwrap();
        RunReservation::acquire(directory, guard)
    }

    fn confirm(run: &RunReservation) -> Result<(), SessionError> {
        run.confirm_empty(|path| {
            #[cfg(all(windows, target_arch = "x86_64"))]
            return crate::windows::open_regular(path, false);
            #[cfg(not(all(windows, target_arch = "x86_64")))]
            File::open(path)
        })
    }

    #[test]
    fn lease_probe() {
        let Some(owned) = env::var_os(DIRECTORY) else {
            return;
        };
        let other = PathBuf::from(env::var_os(OTHER).unwrap());
        assert!(matches!(
            reserve(Path::new(&owned)),
            Err(SessionError::Busy)
        ));
        let independent = reserve(&other).unwrap();
        independent.record_admission().unwrap();
        assert!(has_pending_run(&other).unwrap());
        confirm(&independent).unwrap();
        std::process::exit(LEASE_PROBE_OK);
    }

    #[test]
    fn abandon_probe() {
        let Some(owned) = env::var_os(DIRECTORY) else {
            return;
        };
        let run = reserve(Path::new(&owned)).unwrap();
        run.record_admission().unwrap();
        // Simulate host loss without running destructors.
        std::process::exit(OWNER_EXITED);
    }

    #[test]
    fn two_hosts_contend_only_for_the_same_instance() {
        let owned = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let first = reserve(owned.path()).unwrap();
        first.record_admission().unwrap();
        let probe = |test| {
            Command::new(env::current_exe().unwrap())
                .args(["--exact", test])
                .env(DIRECTORY, owned.path())
                .env(OTHER, other.path())
                .status()
                .unwrap()
        };
        assert!(probe("reservation::tests::misspelled_probe").success());
        assert_eq!(
            probe("reservation::tests::lease_probe").code(),
            Some(LEASE_PROBE_OK)
        );
        assert!(has_pending_run(owned.path()).unwrap());
        assert!(!has_pending_run(other.path()).unwrap());
        confirm(&first).unwrap();
        let prior_id = first.id();
        drop(first);
        assert_ne!(reserve(owned.path()).unwrap().id(), prior_id);
    }

    #[test]
    fn losing_the_lock_keeps_the_run_fenced_without_drain_proof() {
        let owned = tempfile::tempdir().unwrap();
        let status = Command::new(env::current_exe().unwrap())
            .args(["--exact", "reservation::tests::abandon_probe"])
            .env(DIRECTORY, owned.path())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(OWNER_EXITED));
        assert!(matches!(
            reserve(owned.path()),
            Err(SessionError::RecoveryRequired)
        ));
        assert!(has_pending_run(owned.path()).unwrap());
    }

    #[test]
    fn only_matching_run_can_clear_its_record() {
        let owned = tempfile::tempdir().unwrap();
        let first = reserve(owned.path()).unwrap();
        first.record_admission().unwrap();
        assert_eq!(
            first.record_admission().unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        let path = owned.path().join(PENDING);
        fs::write(&path, Uuid::new_v4().as_bytes()).unwrap();
        assert!(matches!(
            confirm(&first),
            Err(SessionError::RecoveryRequired)
        ));
        assert!(path.exists());
    }

    #[cfg(all(windows, target_arch = "x86_64"))]
    #[test]
    fn hard_linked_record_is_rejected_by_the_windows_handle_check() {
        let owned = tempfile::tempdir().unwrap();
        let first = reserve(owned.path()).unwrap();
        first.record_admission().unwrap();
        let path = owned.path().join(PENDING);
        let alias = owned.path().join("alias");
        fs::hard_link(&path, &alias).unwrap();
        assert!(matches!(confirm(&first), Err(SessionError::Data(_))));
        assert_eq!(fs::read(&alias).unwrap(), first.record_bytes());
        assert_eq!(fs::read(&path).unwrap(), first.record_bytes());
    }
}
