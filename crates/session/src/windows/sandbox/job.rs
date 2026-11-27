// SPDX-License-Identifier: MPL-2.0
use super::{wide, win, Handle};
#[cfg(test)]
use std::path::Path;
use std::{
    io,
    mem::size_of_val,
    ptr::{null, null_mut},
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;
use windows_sys::Win32::{
    Foundation::{GetLastError, ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND},
    System::{JobObjects::*, SystemServices::JOB_OBJECT_QUERY},
};

pub(super) const DRAIN_TIMEOUT: Duration = Duration::from_secs(10);

/// A durable OS identity for one protected run's process tree. The launcher is
/// the only process that names the job; a lost owner's job is destroyed by
/// `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, so the name's absence proves drain.
fn job_name(run: Uuid) -> Vec<u16> {
    wide(format!("Local\\CarbonDesktop.Sample.Job.{}", run.simple()))
}

pub(super) fn create(process_limit: u32, run: Uuid) -> io::Result<Handle> {
    let name = job_name(run);
    // Null security attributes keep the private handle out of the child.
    let handle = unsafe { CreateJobObjectW(null(), name.as_ptr()) };
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    let job = Handle(handle);
    // An existing name means another process owns this run's boundary. Refuse
    // rather than adopt or reconfigure a job this launcher did not create.
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return Err(io::Error::other(
            "The protected run's OS job object already exists",
        ));
    }
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags =
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
    limits.BasicLimitInformation.ActiveProcessLimit = process_limit;
    unsafe {
        win(SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            std::ptr::from_ref(&limits).cast(),
            size_of_val(&limits) as u32,
        ))?;
    }
    Ok(job)
}

fn active_processes(job: &Handle) -> io::Result<u32> {
    let mut counts = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
    unsafe {
        win(QueryInformationJobObject(
            job.0,
            JobObjectBasicAccountingInformation,
            std::ptr::from_mut(&mut counts).cast(),
            size_of_val(&counts) as u32,
            null_mut(),
        ))?;
    }
    Ok(counts.ActiveProcesses)
}

/// True when no process from this run can still be running: either the named
/// job no longer exists or it exists with no active members.
pub(super) fn boundary_empty(run: Uuid) -> io::Result<bool> {
    let name = job_name(run);
    let handle = unsafe { OpenJobObjectW(JOB_OBJECT_QUERY, 0, name.as_ptr()) };
    if handle.is_null() {
        let error = io::Error::last_os_error();
        return match error.raw_os_error() {
            Some(code) if code == ERROR_FILE_NOT_FOUND as i32 => Ok(true),
            _ => Err(error),
        };
    }
    let job = Handle(handle);
    Ok(active_processes(&job)? == 0)
}

pub(super) fn wait_empty(job: &Handle, timeout: Duration) -> io::Result<()> {
    let start = Instant::now();
    while active_processes(job)? != 0 {
        let remaining = timeout.saturating_sub(start.elapsed());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "The protected process tree has not drained",
            ));
        }
        thread::sleep(remaining.min(Duration::from_millis(25)));
    }
    Ok(())
}

pub(super) fn stop_and_drain(job: &Handle) -> io::Result<()> {
    unsafe { win(TerminateJobObject(job.0, 1))? };
    wait_empty(job, DRAIN_TIMEOUT)
}

#[cfg(test)]
const FIXTURE: &str = "windows::sandbox::job::tests::process_tree_fixture";
#[cfg(test)]
const ROOT: &str = "CARBON_JOB_TEST_TEMP";
#[cfg(test)]
const ROLE: &str = "CARBON_JOB_TEST_ROLE";

#[cfg(test)]
fn fixture(root: &Path, role: &str) -> std::process::Command {
    let mut child = std::process::Command::new(std::env::current_exe().unwrap());
    child
        .args(["--exact", FIXTURE])
        .env(ROOT, root)
        .env(ROLE, role);
    child
}

#[cfg(test)]
fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !path.is_file() {
        assert!(Instant::now() < deadline, "Timed out waiting for fixture");
        thread::sleep(Duration::from_millis(10));
    }
}

/// Creates a named job that owns one live benign child for reservation tests.
#[cfg(test)]
pub(super) fn test_boundary(run: Uuid, root: &Path) -> io::Result<Handle> {
    use std::os::windows::io::AsRawHandle;
    let job = create(2, run)?;
    let mut child = fixture(root, "child").spawn()?;
    if let Err(error) = unsafe { win(AssignProcessToJobObject(job.0, child.as_raw_handle())) } {
        child.kill().ok();
        child.wait().ok();
        return Err(error);
    }
    wait_for(&root.join("ready"));
    Ok(job)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs, os::windows::io::AsRawHandle};
    use windows_sys::Win32::{Foundation::WAIT_OBJECT_0, System::Threading::WaitForSingleObject};

    #[test]
    fn process_tree_fixture() {
        let Some(root) = env::var_os(ROOT) else {
            return;
        };
        let root = Path::new(&root);
        match env::var(ROLE).unwrap().as_str() {
            "parent" => {
                wait_for(&root.join("admitted"));
                // The parent exits while its child is still a member of the OS job.
                drop(fixture(root, "child").spawn().unwrap());
            }
            "child" => {
                fs::write(root.join("ready"), []).unwrap();
                wait_for(&root.join("finish"));
            }
            _ => panic!("Unknown fixture role"),
        }
    }

    fn surviving_child() -> (tempfile::TempDir, Handle) {
        let root = tempfile::tempdir().unwrap();
        let job = create(2, Uuid::new_v4()).unwrap();
        let mut parent = fixture(root.path(), "parent").spawn().unwrap();
        if let Err(error) = unsafe { win(AssignProcessToJobObject(job.0, parent.as_raw_handle())) }
        {
            // The fixture is still waiting for admission outside the job's cleanup boundary.
            parent.kill().unwrap();
            parent.wait().unwrap();
            panic!("Could not admit the fixture parent to the job: {error}");
        }
        fs::write(root.path().join("admitted"), []).unwrap();
        assert_eq!(
            unsafe { WaitForSingleObject(parent.as_raw_handle(), 15_000) },
            WAIT_OBJECT_0
        );
        assert!(parent.wait().unwrap().success());
        wait_for(&root.path().join("ready"));
        assert_eq!(active_processes(&job).unwrap(), 1);
        (root, job)
    }

    #[test]
    fn root_exit_is_distinct_from_job_drain() {
        let (_first_root, first) = surviving_child();
        let (second_root, second) = surviving_child();
        assert_eq!(
            wait_empty(&first, Duration::ZERO).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        stop_and_drain(&first).unwrap();
        stop_and_drain(&first).unwrap();
        assert_eq!(active_processes(&first).unwrap(), 0);
        assert_eq!(active_processes(&second).unwrap(), 1);
        fs::write(second_root.path().join("finish"), []).unwrap();
        wait_empty(&second, DRAIN_TIMEOUT).unwrap();
    }

    #[test]
    fn named_boundary_distinguishes_unknown_live_and_drained() {
        let root = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        // A run that never created a boundary is already empty.
        assert!(boundary_empty(run).unwrap());
        let job = test_boundary(run, root.path()).unwrap();
        assert!(!boundary_empty(run).unwrap());
        assert!(boundary_empty(Uuid::new_v4()).unwrap());
        drop(job);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !boundary_empty(run).unwrap() {
            assert!(Instant::now() < deadline, "The named job did not retire");
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn an_existing_job_name_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let first = test_boundary(run, root.path()).unwrap();
        let error = create(1, run).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Other);
        drop(first);
    }
}
