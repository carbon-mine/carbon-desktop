// SPDX-License-Identifier: MPL-2.0
use crate::SessionError;
use std::{
    fs::{self, File, TryLockError},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

const PENDING: &str = "pending-run-v2";
const LEGACY_PENDING: &str = "pending-run-v1";
const HEADER: &str = "carbon-desktop-sample-run-v2";
const MAX_RECORD: usize = 96;

struct Pending {
    run: Uuid,
    session: u32,
}

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
        recover_previous(directory).map_err(|error| match error.kind() {
            io::ErrorKind::WouldBlock | io::ErrorKind::InvalidData => {
                SessionError::RecoveryRequired
            }
            _ => SessionError::Data(error),
        })?;
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
        format!("{HEADER}\n{}\n{}\n", self.id, session_id()).into_bytes()
    }
}

/// A lost owner leaves its record behind. A record written by this version
/// names one OS job object and the Windows session that created it, so a
/// missing or empty job in that same session proves the process tree is gone.
/// Older records, other sessions and unrecognized content stay fenced.
fn recover_previous(directory: &Path) -> io::Result<()> {
    if exists(&directory.join(LEGACY_PENDING))? {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "A previous sample has an unresolved legacy run record",
        ));
    }
    let Some(pending) = read_record(directory)? else {
        return Ok(());
    };
    if pending.session != session_id() {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "The previous sample belongs to another Windows session",
        ));
    }
    let empty = boundary_empty(pending.run).map_err(|error| {
        io::Error::new(
            io::ErrorKind::WouldBlock,
            format!("The previous sample's job boundary could not be observed: {error}"),
        )
    })?;
    if !empty {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "The previous sample's process boundary is not confirmed empty",
        ));
    }
    fs::remove_file(directory.join(PENDING))
}

fn read_record(directory: &Path) -> io::Result<Option<Pending>> {
    let path = directory.join(PENDING);
    let file = match open_marker(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take(MAX_RECORD as u64).read_to_end(&mut bytes)?;
    if bytes.len() == MAX_RECORD {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "The sample run record has an unexpected length",
        ));
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "The sample run record is not text",
        )
    })?;
    let mut lines = text.lines();
    let header = lines.next();
    let run = lines.next().and_then(|value| Uuid::parse_str(value).ok());
    let session = lines.next().and_then(|value| value.parse::<u32>().ok());
    let trailing = lines.next();
    match (header, run, session, trailing) {
        (Some(header), Some(run), Some(session), None)
            if header == HEADER
                && run.get_version() == Some(uuid::Version::Random)
                && text == format!("{HEADER}\n{run}\n{session}\n") =>
        {
            Ok(Some(Pending { run, session }))
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "The sample run record is not owned by this build",
        )),
    }
}

#[cfg(all(windows, target_arch = "x86_64"))]
fn open_marker(path: &Path) -> io::Result<File> {
    crate::windows::open_regular(path, false)
}

#[cfg(not(all(windows, target_arch = "x86_64")))]
fn open_marker(path: &Path) -> io::Result<File> {
    File::open(path)
}

#[cfg(all(windows, target_arch = "x86_64"))]
fn boundary_empty(run: Uuid) -> io::Result<bool> {
    crate::windows::sample_boundary_empty(run)
}

/// Only the Windows fixture owns an OS job boundary. Other targets keep the
/// previous fail-closed behavior for tests and unsupported builds.
#[cfg(not(all(windows, target_arch = "x86_64")))]
fn boundary_empty(_run: Uuid) -> io::Result<bool> {
    Ok(false)
}

#[cfg(all(windows, target_arch = "x86_64"))]
fn session_id() -> u32 {
    crate::windows::current_session_id()
}

#[cfg(not(all(windows, target_arch = "x86_64")))]
fn session_id() -> u32 {
    0
}

/// Every present object fences admission, including an invalid or damaged record.
pub(crate) fn has_pending_run(directory: &Path) -> io::Result<bool> {
    Ok(exists(&directory.join(PENDING))? || exists(&directory.join(LEGACY_PENDING))?)
}

fn exists(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
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
        // Simulate host loss without running destructors or creating a boundary.
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
    fn lost_owner_without_a_live_boundary_is_recovered() {
        let owned = tempfile::tempdir().unwrap();
        let status = Command::new(env::current_exe().unwrap())
            .args(["--exact", "reservation::tests::abandon_probe"])
            .env(DIRECTORY, owned.path())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(OWNER_EXITED));
        assert!(has_pending_run(owned.path()).unwrap());
        #[cfg(all(windows, target_arch = "x86_64"))]
        {
            let recovered = reserve(owned.path()).unwrap();
            assert!(!has_pending_run(owned.path()).unwrap());
            recovered.record_admission().unwrap();
            confirm(&recovered).unwrap();
        }
        #[cfg(not(all(windows, target_arch = "x86_64")))]
        assert!(matches!(
            reserve(owned.path()),
            Err(SessionError::RecoveryRequired)
        ));
    }

    #[test]
    fn legacy_and_damaged_records_stay_fenced() {
        let owned = tempfile::tempdir().unwrap();
        fs::write(
            owned.path().join(LEGACY_PENDING),
            format!("carbon-desktop-sample-run-v1\n{}\n", Uuid::new_v4()),
        )
        .unwrap();
        assert!(matches!(
            reserve(owned.path()),
            Err(SessionError::RecoveryRequired)
        ));
        fs::remove_file(owned.path().join(LEGACY_PENDING)).unwrap();
        fs::write(owned.path().join(PENDING), b"not owned").unwrap();
        assert!(matches!(
            reserve(owned.path()),
            Err(SessionError::RecoveryRequired)
        ));
        assert!(owned.path().join(PENDING).exists());
    }

    #[test]
    fn record_from_another_session_stays_fenced() {
        let owned = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let other_session = session_id().wrapping_add(1);
        fs::write(
            owned.path().join(PENDING),
            format!("{HEADER}\n{run}\n{other_session}\n").as_bytes(),
        )
        .unwrap();
        assert!(matches!(
            reserve(owned.path()),
            Err(SessionError::RecoveryRequired)
        ));
        assert!(owned.path().join(PENDING).exists());
    }

    #[cfg(all(windows, target_arch = "x86_64"))]
    #[test]
    fn live_os_boundary_keeps_the_record_fenced() {
        let owned = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let job = crate::windows::test_boundary(run, owned.path()).unwrap();
        fs::write(
            owned.path().join(PENDING),
            format!("{HEADER}\n{run}\n{}\n", session_id()).as_bytes(),
        )
        .unwrap();
        assert!(matches!(
            reserve(owned.path()),
            Err(SessionError::RecoveryRequired)
        ));
        assert!(owned.path().join(PENDING).exists());
        drop(job);
        let recovered = reserve(owned.path()).unwrap();
        assert!(!has_pending_run(owned.path()).unwrap());
        recovered.record_admission().unwrap();
        confirm(&recovered).unwrap();
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
