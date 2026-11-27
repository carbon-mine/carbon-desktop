// SPDX-License-Identifier: MPL-2.0
mod registry;
mod sandbox;

use crate::{reservation::RunReservation, SessionError, SessionReport};
use registry::SessionLease;
use sandbox::Profile;
use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io::{self, Error, ErrorKind, Read, Write},
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Path, PathBuf},
};
use uuid::Uuid;
use windows_sys::Win32::{
    Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION},
    System::{RemoteDesktop::ProcessIdToSessionId, Threading::GetCurrentProcessId},
};

const CLASS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/FixtureSession.class"));
const LISTENER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/FixtureSession$1.class"));
const ACTION: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/FixtureSession$IoAction.class"));
const MAX_COUNT: u32 = 1_000_000;

pub(super) const fn supported() -> bool {
    !CLASS.is_empty() && !LISTENER.is_empty() && !ACTION.is_empty()
}

pub(super) fn run(
    root: &Path,
    id: Uuid,
    major: u16,
    smoke: bool,
) -> Result<SessionReport, SessionError> {
    if !supported() {
        return Err(SessionError::Unsupported);
    }
    if !matches!(major, 17 | 21 | 25) {
        return Err(SessionError::RuntimeNotReady);
    }
    // Shared with managed Java preparation. Keep the read lease through the
    // snapshot so a cooperating installer cannot replace its source mid-copy.
    let java_guard = open_regular(&root.join(format!("java-{major}.lock")), false)
        .map_err(|_| SessionError::RuntimeNotReady)?;
    java_guard.try_lock_shared().map_err(lock_error)?;
    let runtime = carbon_runtime::inspect(root, major).map_err(|error| match error {
        carbon_runtime::RuntimeError::Busy => SessionError::Busy,
        _ => SessionError::RuntimeNotReady,
    })?;
    let home = runtime.java_home().ok_or(SessionError::RuntimeNotReady)?;

    let saves = root.join("sample-sessions");
    ensure_dir(&saves).map_err(SessionError::Data)?;
    let save_dir = saves.join(id.to_string());
    ensure_dir(&save_dir).map_err(SessionError::Data)?;
    let profile_guard =
        open_regular(&save_dir.join("sample.lock"), true).map_err(SessionError::Data)?;
    let reservation = RunReservation::acquire(&save_dir, profile_guard)?;
    let saved_count = save_dir.join("count");
    let initial = read_count(&saved_count)
        .map_err(SessionError::Data)?
        .unwrap_or(0);

    let lease = SessionLease::begin(&saves, id, reservation.id()).map_err(|error| {
        if error.kind() == ErrorKind::WouldBlock {
            SessionError::RecoveryRequired
        } else {
            SessionError::Isolation(error)
        }
    })?;
    let profile = Profile::new(&lease.profile_name(false)).map_err(SessionError::Isolation)?;
    let neighbor = Profile::new(&lease.profile_name(true)).map_err(SessionError::Isolation)?;
    let own = local_state(&profile).map_err(SessionError::Isolation)?;
    let foreign = local_state(&neighbor)
        .map_err(SessionError::Isolation)?
        .join("private-witness");
    write_new(&own.join("initial-count"), initial.to_string().as_bytes())
        .map_err(SessionError::Isolation)?;
    write_new(&foreign, b"peer witness").map_err(SessionError::Isolation)?;

    let staged = lease.stage();
    fs::create_dir(&staged).map_err(SessionError::Isolation)?;
    sandbox::grant_read_execute(&staged, &profile).map_err(SessionError::Isolation)?;
    let java = staged.join("java");
    copy_tree(home, &java).map_err(SessionError::Isolation)?;
    drop(java_guard);

    let payload = staged.join("classes");
    fs::create_dir(&payload).map_err(SessionError::Isolation)?;
    for (name, bytes) in [
        ("FixtureSession.class", CLASS),
        ("FixtureSession$1.class", LISTENER),
        ("FixtureSession$IoAction.class", ACTION),
    ] {
        write_new(&payload.join(name), bytes).map_err(SessionError::Isolation)?;
    }
    let witness = staged.join("readonly-witness");
    write_new(&witness, b"protected fixture").map_err(SessionError::Isolation)?;
    reservation.record_admission().map_err(SessionError::Data)?;
    let mut observation = sandbox::LaunchObservation {
        smoke,
        run: reservation.id(),
        boundary_empty: true,
    };
    let result = sandbox::launch(
        &profile,
        &java.join("bin/java.exe"),
        &own,
        &payload,
        &foreign,
        &witness,
        &mut observation,
    );
    if !observation.boundary_empty {
        return Err(SessionError::RecoveryRequired);
    }
    reservation.confirm_empty(|path| open_regular(path, false))?;
    result.map_err(SessionError::Isolation)?;
    if read_small(&own.join("window-shown"), 7)
        .map_err(SessionError::Isolation)?
        .as_deref()
        != Some(b"checked")
        || read_small(&foreign, 12)
            .map_err(SessionError::Isolation)?
            .as_deref()
            != Some(b"peer witness")
        || read_small(&witness, 17)
            .map_err(SessionError::Isolation)?
            .as_deref()
            != Some(b"protected fixture")
    {
        return Err(SessionError::Isolation(Error::other(
            "Required isolation and window witnesses were not verified",
        )));
    }
    let next = read_count(&own.join("next-count")).map_err(SessionError::Isolation)?;
    if let Some(count) = next {
        let mut file = tempfile::NamedTempFile::new_in(&save_dir).map_err(SessionError::Data)?;
        file.write_all(count.to_string().as_bytes())
            .map_err(SessionError::Data)?;
        file.as_file().sync_all().map_err(SessionError::Data)?;
        file.persist(&saved_count)
            .map_err(|e| SessionError::Data(e.error))?;
    }
    Ok(SessionReport {
        sample_count: next.unwrap_or(initial),
        saved: next.is_some(),
    })
}

/// A lost owner's process boundary is identified by the job name derived from
/// the run record. Other builds keep the previous fail-closed behavior.
pub(super) fn sample_boundary_empty(run: Uuid) -> io::Result<bool> {
    sandbox::boundary_empty(run)
}

/// A job name is scoped to the Windows session that created it, so a recovery
/// witness is only valid for the session recorded in the pending run.
pub(super) fn current_session_id() -> io::Result<u32> {
    let mut session = 0;
    if unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(session)
}

#[cfg(test)]
pub(super) fn test_boundary(run: Uuid, root: &Path) -> io::Result<impl Sized> {
    sandbox::test_boundary(run, root)
}

fn lock_error(error: TryLockError) -> SessionError {
    match error {
        TryLockError::WouldBlock => SessionError::Busy,
        TryLockError::Error(error) => SessionError::Data(error),
    }
}

fn local_state(profile: &Profile) -> io::Result<PathBuf> {
    let folder = profile.folder()?;
    ensure_dir(&folder)?;
    let local = folder.join("LocalState");
    ensure_dir(&local)?;
    Ok(local)
}

fn ensure_dir(path: &Path) -> io::Result<()> {
    match fs::create_dir(path) {
        Ok(()) => (),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => (),
        Err(error) => return Err(error),
    }
    check_dir(path)
}

fn check_dir(path: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir() || meta.file_attributes() & 0x400 != 0 {
        return Err(Error::other("Fixture storage is not a regular directory"));
    }
    Ok(())
}

pub(super) fn open_regular(path: &Path, create: bool) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(create)
        .create(create)
        .truncate(false)
        .custom_flags(0x00200000) // FILE_FLAG_OPEN_REPARSE_POINT
        .open(path)?;
    let meta = file.metadata()?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // The metadata structure remains writable while the live file handle is borrowed.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(Error::last_os_error());
    }
    if !meta.is_file() || meta.file_attributes() & 0x400 != 0 || info.nNumberOfLinks != 1 {
        return Err(Error::other(
            "Fixture storage is not a regular private file",
        ));
    }
    Ok(file)
}

fn read_small(path: &Path, max: usize) -> io::Result<Option<Vec<u8>>> {
    let file = match open_regular(path, false) {
        Ok(file) => file,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut data = Vec::new();
    file.take(max as u64 + 1).read_to_end(&mut data)?;
    if data.len() > max {
        return Err(Error::other("Fixture save exceeds its data bound"));
    }
    Ok(Some(data))
}

fn read_count(path: &Path) -> io::Result<Option<u32>> {
    let Some(data) = read_small(path, 7)? else {
        return Ok(None);
    };
    if data.is_empty() || !data.iter().all(u8::is_ascii_digit) {
        return Err(Error::other("Fixture save is not a valid count"));
    }
    std::str::from_utf8(&data)
        .ok()
        .and_then(|text| text.parse::<u32>().ok())
        .filter(|count| *count <= MAX_COUNT)
        .map(Some)
        .ok_or_else(|| Error::other("Fixture save is outside its allowed range"))
}

fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    File::create_new(path)?.write_all(bytes)
}

fn copy_tree(source: &Path, destination: &Path) -> io::Result<()> {
    check_dir(source)?;
    fs::create_dir(destination)?;
    for item in fs::read_dir(source)? {
        let item = item?;
        let origin = item.path();
        let child = destination.join(item.file_name());
        if fs::symlink_metadata(&origin)?.is_dir() {
            copy_tree(&origin, &child)?;
        } else {
            let mut input = open_regular(&origin, false)?;
            let mut output = File::create_new(child)?;
            io::copy(&mut input, &mut output)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_runtime_refuses_before_creating_sample_state() {
        let root = tempfile::tempdir().unwrap();
        let error = crate::run(root.path(), Uuid::new_v4(), 25).unwrap_err();
        if crate::supported() {
            assert!(matches!(error, SessionError::RuntimeNotReady));
        } else {
            assert!(matches!(error, SessionError::Unsupported));
        }
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }

    #[test]
    fn counts_accept_only_bounded_digits_and_regular_files() {
        let root = tempfile::tempdir().unwrap();
        let number = root.path().join("number");
        assert_eq!(read_count(&number).unwrap(), None);
        for (bytes, expected) in [(b"0".as_slice(), 0), (b"1000000", MAX_COUNT)] {
            fs::write(&number, bytes).unwrap();
            assert_eq!(read_count(&number).unwrap(), Some(expected));
        }
        for invalid in [b"1000001".as_slice(), b"+1", b"1\n", b"12345678", b""] {
            fs::write(&number, invalid).unwrap();
            assert!(read_count(&number).is_err());
        }
        fs::write(&number, b"1").unwrap();
        let alias = root.path().join("alias");
        fs::hard_link(&number, &alias).unwrap();
        assert!(read_count(&number).is_err());
        assert!(read_count(&alias).is_err());
    }

    #[test]
    #[ignore = "downloads a pinned JVM and briefly opens three isolated native windows"]
    fn native_windows_save_reopen_and_profile_separation() {
        let root = tempfile::tempdir().unwrap();
        carbon_runtime::prepare(root.path(), 25).unwrap();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        for (id, expected) in [(a, 1), (a, 2), (b, 1)] {
            let result = run(root.path(), id, 25, true).unwrap();
            assert!(result.saved);
            assert_eq!(result.sample_count, expected);
            println!("verified native peer and scoped access checks; disposable count {expected}");
        }
    }
}
