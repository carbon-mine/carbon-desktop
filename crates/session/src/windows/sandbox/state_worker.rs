// SPDX-License-Identifier: MPL-2.0
//! Fixed, headless witness for the Windows state-lease ACL.
use super::*;
use crate::windows_state::{profile_name, StateLease};
use std::{fs, io::Write};
use uuid::Uuid;

const CODE: &str = "validation-code";
const WORKER: &str = "state-lease-worker.exe";
const STATE: &str = "protected-state-v1";
const PEER: Uuid = Uuid::from_u128(0xe3d23f07_62b7_406f_95e4_005409b460e5);
const RETIREMENT_WITNESSED: i32 = 23;

fn fixture_paths() -> (PathBuf, PathBuf) {
    assert_ne!(
        token_data(unsafe { GetCurrentProcess() }, TokenIsAppContainer).unwrap()[0],
        0
    );
    let exe = std::env::current_exe().unwrap();
    let code = exe.parent().unwrap();
    assert_eq!(exe.file_name().unwrap(), WORKER);
    assert_eq!(code.file_name().unwrap(), CODE);
    (code.to_owned(), code.parent().unwrap().to_owned())
}

fn require_denied(path: &Path) {
    assert_eq!(
        fs::read(path).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    assert_eq!(
        fs::OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
}

#[test]
#[ignore = "this child entry point runs only from the staged sandbox fixture"]
fn worker() {
    let (code, instance) = fixture_paths();
    let state = instance.join(STATE);
    let counter = state.join("sample.dat");
    let created = state.join("worker-created.dat");
    let value = fs::read(&counter).unwrap();
    let next = match value.as_slice() {
        b"0" => b"1",
        b"1" => b"2",
        _ => panic!("unexpected initial state fixture"),
    };
    fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&counter)
        .unwrap()
        .write_all(next)
        .unwrap();
    if next == b"1" {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&created)
            .unwrap()
            .write_all(b"created by AppContainer")
            .unwrap();
    } else {
        assert_eq!(fs::read(&created).unwrap(), b"created by AppContainer");
    }

    require_denied(&instance.join("game").join("sibling-canary"));
    require_denied(&peer_canary(&instance));
    let host_record = instance.join(".protected-state-host-v1").join("run.json");
    assert_eq!(
        fs::read(host_record).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    let runtime = code.join("runtime-canary");
    assert_eq!(fs::read(&runtime).unwrap(), b"verified runtime canary");
    assert_eq!(
        fs::OpenOptions::new()
            .write(true)
            .open(&runtime)
            .unwrap_err()
            .kind(),
        io::ErrorKind::PermissionDenied
    );
}

fn peer_canary(instance: &Path) -> PathBuf {
    instance
        .parent()
        .unwrap()
        .join(PEER.to_string())
        .join(STATE)
        .join("peer-canary")
}

#[test]
#[ignore = "this child entry point runs only from the staged sandbox fixture"]
fn retired_worker() {
    let (_, instance) = fixture_paths();
    let state = instance.join(STATE);
    require_denied(&state.join("sample.dat"));
    require_denied(&state.join("worker-created.dat"));
    // The test binary otherwise exits successfully when a filter finds zero tests.
    std::process::exit(RETIREMENT_WITNESSED);
}

struct Registration {
    name: Option<String>,
    profile: Profile,
}

impl Registration {
    fn new(name: String) -> io::Result<Self> {
        let profile = Profile::new(&name)?;
        Ok(Self {
            name: Some(name),
            profile,
        })
    }

    fn close(mut self) -> io::Result<()> {
        if let Some(name) = &self.name {
            delete_profile(name)?;
            self.name.take();
        }
        Ok(())
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        if let Some(name) = &self.name {
            let _ = delete_profile(name);
        }
    }
}

fn run_worker(profile: &Profile, code: &Path, state: &Path, retired: bool) -> io::Result<()> {
    let exe = code.join(WORKER);
    let mut command = quoted_command(&[
        exe.as_os_str(),
        OsStr::new("--ignored"),
        OsStr::new("--exact"),
        OsStr::new(if retired {
            "windows::sandbox::state_worker::retired_worker"
        } else {
            "windows::sandbox::state_worker::worker"
        }),
    ])?;
    let working = if retired { code } else { state };
    let environment = environment(working)?;
    let application = wide(exe);
    let directory = wide(working);
    // Handles and attribute storage remain live for the native process calls.
    unsafe {
        let job = Handle(CreateJobObjectW(null(), null()), None);
        win(i32::from(!job.0.is_null()))?;
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
        limits.BasicLimitInformation.ActiveProcessLimit = 1;
        win(SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            std::ptr::from_ref(&limits).cast(),
            size_of_val(&limits) as u32,
        ))?;
        let security = SECURITY_CAPABILITIES {
            AppContainerSid: profile.sid,
            ..Default::default()
        };
        let mut attributes = Attributes::new(2)?;
        attributes.set(PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, &security)?;
        attributes.set(PROC_THREAD_ATTRIBUTE_JOB_LIST, &job.0)?;
        let mut startup = STARTUPINFOEXW::default();
        startup.StartupInfo.cb = size_of_val(&startup) as u32;
        startup.lpAttributeList = attributes.backing.as_mut_ptr().cast();
        let mut info = PROCESS_INFORMATION::default();
        win(CreateProcessW(
            application.as_ptr(),
            command.as_mut_ptr(),
            null(),
            null(),
            0,
            CREATE_SUSPENDED
                | EXTENDED_STARTUPINFO_PRESENT
                | CREATE_UNICODE_ENVIRONMENT
                | CREATE_NO_WINDOW,
            environment.as_ptr().cast(),
            directory.as_ptr(),
            &startup.StartupInfo,
            &mut info,
        ))?;
        let process = Handle(info.hProcess, None);
        let thread = Handle(info.hThread, None);
        let outcome = (|| {
            check_token(process.0, profile.sid)?;
            let mut assigned = 0;
            win(IsProcessInJob(process.0, job.0, &mut assigned))?;
            if assigned == 0 || ResumeThread(thread.0) == u32::MAX {
                return Err(Error::other(
                    "The sandbox worker could not start in its job.",
                ));
            }
            if WaitForSingleObject(process.0, 15_000) != WAIT_OBJECT_0 {
                return Err(Error::other("The sandbox worker timed out."));
            }
            let mut exit = 0;
            win(GetExitCodeProcess(process.0, &mut exit))?;
            let expected = if retired {
                RETIREMENT_WITNESSED as u32
            } else {
                0
            };
            if exit != expected {
                return Err(Error::other(format!("The fixed worker exited {exit:#x}.")));
            }
            Ok(())
        })();
        if let Err(error) = outcome {
            TerminateProcess(process.0, 1);
            if WaitForSingleObject(process.0, 5_000) != WAIT_OBJECT_0 {
                return Err(Error::other(format!(
                    "Could not confirm fixture teardown: {error}"
                )));
            }
            return Err(error);
        }
    }
    Ok(())
}

#[test]
#[ignore = "registers disposable AppContainer profiles for a headless native check"]
fn appcontainer_writes_and_reopens_only_scoped_player_state() {
    let library = tempfile::tempdir().unwrap();
    let instance = Uuid::new_v4();
    let base = library
        .path()
        .join("instances-v1")
        .join(instance.to_string());
    let game = base.join("game");
    fs::create_dir_all(&game).unwrap();
    let sibling = game.join("sibling-canary");
    fs::write(&sibling, b"existing player state").unwrap();
    fs::create_dir(base.parent().unwrap().join(PEER.to_string())).unwrap();
    let _peer_lease = StateLease::acquire(library.path(), PEER).unwrap();
    let peer = peer_canary(&base);
    fs::write(&peer, b"separate instance state").unwrap();

    let mut lease = StateLease::acquire(library.path(), instance).unwrap();
    let temporary = Uuid::new_v4();
    let state = lease.prepare(temporary).unwrap();
    let counter = state.join("sample.dat");
    fs::write(&counter, b"0").unwrap();
    lease.retire_after_drain(temporary).unwrap();
    drop(lease);

    for expected in [b"1", b"2"] {
        let stage = base.join(CODE);
        fs::create_dir(&stage).unwrap();
        let run = Uuid::new_v4();
        let profile = Registration::new(profile_name(instance, run)).unwrap();
        grant_read_execute(&stage, &profile.profile).unwrap();
        fs::copy(std::env::current_exe().unwrap(), stage.join(WORKER)).unwrap();
        let runtime = stage.join("runtime-canary");
        fs::write(&runtime, b"verified runtime canary").unwrap();
        let mut lease = StateLease::acquire(library.path(), instance).unwrap();
        assert_eq!(lease.prepare(run).unwrap(), state);
        run_worker(&profile.profile, &stage, &state, false).unwrap();
        lease.retire_after_drain(run).unwrap();
        assert!(lease.unfinished_run().unwrap().is_none());
        run_worker(&profile.profile, &stage, &state, true).unwrap();
        assert_eq!(fs::read(&counter).unwrap(), expected);
        assert_eq!(fs::read(&sibling).unwrap(), b"existing player state");
        assert_eq!(fs::read(&peer).unwrap(), b"separate instance state");
        assert_eq!(fs::read(&runtime).unwrap(), b"verified runtime canary");
        assert_eq!(
            fs::read(state.join("worker-created.dat")).unwrap(),
            b"created by AppContainer"
        );
        profile.close().unwrap();
        fs::remove_dir_all(&stage).unwrap();
    }
}
