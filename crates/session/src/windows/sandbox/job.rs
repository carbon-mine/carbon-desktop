// SPDX-License-Identifier: MPL-2.0
use super::{current_user, wide, win, Handle};
#[cfg(test)]
use std::path::Path;
use std::{
    io,
    mem::{size_of, size_of_val},
    ptr::null_mut,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;
use windows_sys::{
    core::BOOL,
    Win32::{
        Foundation::{
            GetLastError, ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, HWND, LPARAM,
        },
        Security::{
            AddAccessAllowedAce, GetLengthSid, InitializeAcl, InitializeSecurityDescriptor,
            SetSecurityDescriptorDacl, ACCESS_ALLOWED_ACE, ACL, ACL_REVISION, SECURITY_ATTRIBUTES,
            SECURITY_DESCRIPTOR, TOKEN_USER,
        },
        System::{
            JobObjects::*,
            SystemServices::JOB_OBJECT_QUERY,
            Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
        },
        UI::WindowsAndMessaging::{
            EnumWindows, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_CLOSE,
        },
    },
};

pub(super) const DRAIN_TIMEOUT: Duration = Duration::from_secs(10);

/// A durable OS identity for one protected run's process tree. The launcher is
/// the only process that names the job; a lost owner's job is destroyed by
/// `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, so the name's absence proves drain.
fn job_name(run: Uuid) -> Vec<u16> {
    wide(format!("Local\\CarbonDesktop.Sample.Job.{}", run.simple()))
}

pub(super) fn create(process_limit: u32, run: Uuid) -> io::Result<Arc<Handle>> {
    let name = job_name(run);
    // The name is the recovery witness, so only this user may query it. The
    // creating handle keeps full control; other same-user processes cannot
    // reconfigure or terminate the job through a fresh open.
    let user = current_user()?;
    let sid = unsafe { (*user.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    let sid_length = unsafe { GetLengthSid(sid) } as usize;
    let mut acl = vec![
        0u8;
        size_of::<ACL>() + size_of::<ACCESS_ALLOWED_ACE>() - size_of::<u32>()
            + sid_length
    ];
    let acl_pointer = acl.as_mut_ptr().cast::<ACL>();
    let mut descriptor = SECURITY_DESCRIPTOR::default();
    let descriptor_pointer = std::ptr::from_mut(&mut descriptor);
    unsafe {
        win(InitializeAcl(acl_pointer, acl.len() as u32, ACL_REVISION))?;
        win(AddAccessAllowedAce(
            acl_pointer,
            ACL_REVISION,
            JOB_OBJECT_QUERY,
            sid,
        ))?;
        win(InitializeSecurityDescriptor(descriptor_pointer.cast(), 1))?;
        win(SetSecurityDescriptorDacl(
            descriptor_pointer.cast(),
            1,
            acl_pointer,
            0,
        ))?;
    }
    let attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor_pointer.cast(),
        bInheritHandle: 0,
    };
    let handle = unsafe { CreateJobObjectW(&attributes, name.as_ptr()) };
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    let job = Arc::new(Handle(handle));
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

/// The live process IDs the OS reports for this run's job. A closed handle is
/// never used to name a process; membership comes from the job itself.
fn member_pids(job: &Handle) -> io::Result<Vec<u32>> {
    let mut capacity = 16usize;
    loop {
        let header = size_of::<JOBOBJECT_BASIC_PROCESS_ID_LIST>() - size_of::<usize>();
        let bytes = header + capacity * size_of::<usize>();
        let mut buffer = vec![0usize; bytes.div_ceil(size_of::<usize>())];
        let mut needed = 0u32;
        let queried = unsafe {
            QueryInformationJobObject(
                job.0,
                JobObjectBasicProcessIdList,
                buffer.as_mut_ptr().cast(),
                (buffer.len() * size_of::<usize>()) as u32,
                &mut needed,
            )
        };
        if queried == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_MORE_DATA as i32) {
                // `needed` is a byte count, not a number of entries.
                let required = (needed as usize)
                    .saturating_sub(header)
                    .div_ceil(size_of::<usize>());
                capacity = required.max(capacity * 2);
                continue;
            }
            return Err(error);
        }
        // The layout starts with two counts followed by the variable process list.
        let assigned = unsafe { *buffer.as_ptr().cast::<u32>() } as usize;
        let listed = unsafe { *buffer.as_ptr().cast::<u32>().add(1) } as usize;
        if assigned > capacity {
            capacity = assigned;
            continue;
        }
        let ids = unsafe {
            std::slice::from_raw_parts(
                buffer.as_ptr().cast::<u8>().add(header).cast::<usize>(),
                listed.min(capacity),
            )
        };
        return Ok(ids.iter().map(|id| *id as u32).collect());
    }
}

struct CloseTargets<'a> {
    job: &'a Handle,
    pids: &'a [u32],
    posted: usize,
}

/// Confirms that the process that owns a matched window is still the member the
/// job reported, so a recycled PID cannot redirect the close request.
fn still_member(job: &Handle, pid: u32) -> bool {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return false;
    }
    let process = Handle(process);
    let mut member = 0;
    unsafe { IsProcessInJob(process.0, job.0, &mut member) != 0 && member != 0 }
}

unsafe extern "system" fn close_owned_window(window: HWND, parameter: LPARAM) -> BOOL {
    let targets = &mut *(parameter as *mut CloseTargets<'_>);
    let mut pid = 0u32;
    GetWindowThreadProcessId(window, &mut pid);
    if pid != 0
        && targets.pids.contains(&pid)
        && IsWindowVisible(window) != 0
        && still_member(targets.job, pid)
        && PostMessageW(window, WM_CLOSE, 0, 0) != 0
    {
        targets.posted += 1;
    }
    1
}

/// Asks every visible top-level window owned by a member of this run to close.
/// This is the OS-level graceful request a game can turn into its own save and
/// exit. Windowless members simply receive no request, so the caller still
/// bounds how long it waits before forcing the tree down.
pub(super) fn request_close(job: &Handle) -> io::Result<usize> {
    let pids = member_pids(job)?;
    if pids.is_empty() {
        return Ok(0);
    }
    let mut targets = CloseTargets {
        job,
        pids: &pids,
        posted: 0,
    };
    let parameter = (&mut targets as *mut CloseTargets<'_>) as LPARAM;
    if unsafe { EnumWindows(Some(close_owned_window), parameter) } == 0 {
        let error = io::Error::last_os_error();
        return Err(match error.raw_os_error() {
            Some(0) | None => io::Error::other("Window enumeration failed"),
            Some(_) => error,
        });
    }
    Ok(targets.posted)
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

/// Terminates every member of one run's job. Repeating the call on an already
/// empty job succeeds, so a repeated stop request has no additional effect.
pub(super) fn terminate(job: &Handle) -> io::Result<()> {
    unsafe { win(TerminateJobObject(job.0, 1)) }
}

pub(super) fn stop_and_drain(job: &Handle) -> io::Result<()> {
    terminate(job)?;
    wait_empty(job, DRAIN_TIMEOUT)
}

#[cfg(test)]
const FIXTURE: &str = "windows::sandbox::job::tests::process_tree_fixture";
#[cfg(test)]
const ROOT: &str = "CARBON_JOB_TEST_TEMP";
#[cfg(test)]
const ROLE: &str = "CARBON_JOB_TEST_ROLE";
#[cfg(test)]
const NO_WINDOW: i32 = 73;
#[cfg(test)]
static IGNORE_CLOSE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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

/// A real top-level window owned by the fixture process. The message loop ends
/// when the window is closed, so an OS close request is observable as exit.
#[cfg(test)]
unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use std::sync::atomic::Ordering;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, DestroyWindow, PostQuitMessage, WM_DESTROY,
    };
    match message {
        WM_CLOSE => {
            if IGNORE_CLOSE.load(Ordering::Relaxed) {
                0
            } else {
                DestroyWindow(window);
                0
            }
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(window, message, wparam, lparam),
    }
}

#[cfg(test)]
fn window_fixture(root: &Path, stubborn: bool) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use std::sync::atomic::Ordering;
    use windows_sys::Win32::{
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DispatchMessageW, GetMessageW, RegisterClassW, TranslateMessage, MSG,
            WNDCLASSW, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
        },
    };
    IGNORE_CLOSE.store(stubborn, Ordering::Relaxed);
    let class: Vec<u16> = std::ffi::OsStr::new("CarbonJobClosableFixture")
        .encode_wide()
        .chain([0])
        .collect();
    let title: Vec<u16> = std::ffi::OsStr::new("Carbon job close fixture")
        .encode_wide()
        .chain([0])
        .collect();
    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let registration = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        lpszClassName: class.as_ptr(),
        ..Default::default()
    };
    if unsafe { RegisterClassW(&registration) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let window = unsafe {
        CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            0,
            0,
            320,
            200,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        )
    };
    if window.is_null() {
        return Err(io::Error::last_os_error());
    }
    std::fs::write(root.join("ready"), [])?;
    let mut message = MSG::default();
    while unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) } > 0 {
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}

/// Creates a named job that owns one live benign child for reservation tests.
#[cfg(test)]
pub(super) fn test_boundary(run: Uuid, root: &Path) -> io::Result<Arc<Handle>> {
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
            role @ ("windowed" | "windowed-stubborn") => {
                if let Err(error) = window_fixture(root, role == "windowed-stubborn") {
                    fs::write(root.join("no-window"), format!("{:?}", error.kind())).unwrap();
                    std::process::exit(NO_WINDOW);
                }
            }
            _ => panic!("Unknown fixture role"),
        }
    }

    fn surviving_child() -> (tempfile::TempDir, Arc<Handle>) {
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

    /// Admits one fixture process with a real window to a fresh job. Returns
    /// `None` when this host cannot create an addressable window, which is a
    /// host limitation rather than a supervision failure.
    fn window_boundary(
        run: Uuid,
        root: &Path,
        stubborn: bool,
    ) -> Option<(Arc<Handle>, std::process::Child)> {
        let job = create(2, run).unwrap();
        let role = if stubborn {
            "windowed-stubborn"
        } else {
            "windowed"
        };
        let mut child = fixture(root, role).spawn().unwrap();
        if let Err(error) = unsafe { win(AssignProcessToJobObject(job.0, child.as_raw_handle())) } {
            child.kill().ok();
            child.wait().ok();
            panic!("Could not admit the fixture window to the job: {error}");
        }
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if root.join("ready").is_file() {
                return Some((job, child));
            }
            if root.join("no-window").is_file() {
                child.wait().ok();
                eprintln!("This host cannot create an addressable fixture window");
                return None;
            }
            assert!(
                Instant::now() < deadline,
                "Timed out waiting for the fixture window"
            );
            thread::sleep(Duration::from_millis(10));
        }
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
    fn a_suspended_root_is_terminated_before_it_runs() {
        use std::os::windows::process::CommandExt;
        let root = tempfile::tempdir().unwrap();
        let job = create(2, Uuid::new_v4()).unwrap();
        let mut command = fixture(root.path(), "child");
        command.creation_flags(0x0000_0004); // CREATE_SUSPENDED
        let mut child = command.spawn().unwrap();
        if let Err(error) = unsafe { win(AssignProcessToJobObject(job.0, child.as_raw_handle())) } {
            child.kill().ok();
            child.wait().ok();
            panic!("Could not admit the suspended fixture: {error}");
        }
        super::super::stop_suspended(child.as_raw_handle(), &job).unwrap();
        assert_eq!(active_processes(&job).unwrap(), 0);
        assert_eq!(child.wait().unwrap().code(), Some(1));
        // The fixture's first action would have been to write this marker.
        assert!(!root.path().join("ready").exists());
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
    fn a_close_request_reaches_the_run_window() {
        let root = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let Some((job, mut child)) = window_boundary(run, root.path(), false) else {
            return;
        };
        assert_eq!(request_close(&job).unwrap(), 1);
        wait_empty(&job, DRAIN_TIMEOUT).unwrap();
        assert_eq!(child.wait().unwrap().code(), Some(0));
    }

    #[test]
    fn a_window_that_ignores_the_close_survives_until_the_bound() {
        let root = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let Some((job, mut child)) = window_boundary(run, root.path(), true) else {
            return;
        };
        assert_eq!(request_close(&job).unwrap(), 1);
        assert_eq!(
            wait_empty(&job, Duration::from_millis(300))
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
        assert_eq!(active_processes(&job).unwrap(), 1);
        terminate(&job).unwrap();
        wait_empty(&job, DRAIN_TIMEOUT).unwrap();
        child.wait().ok();
    }

    #[test]
    fn a_close_request_skips_windows_outside_the_run() {
        let outsider_root = tempfile::tempdir().unwrap();
        let mut outsider = fixture(outsider_root.path(), "windowed").spawn().unwrap();
        wait_for(&outsider_root.path().join("ready"));
        let root = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let Some((job, mut member)) = window_boundary(run, root.path(), false) else {
            outsider.kill().ok();
            outsider.wait().ok();
            return;
        };
        // Only the run's own window is a member, so only it is asked to close.
        assert_eq!(request_close(&job).unwrap(), 1);
        wait_empty(&job, DRAIN_TIMEOUT).unwrap();
        assert_eq!(member.wait().unwrap().code(), Some(0));
        assert!(
            outsider.try_wait().unwrap().is_none(),
            "a window outside the run must not receive the request"
        );
        outsider.kill().ok();
        outsider.wait().ok();
    }

    #[test]
    fn an_empty_run_receives_no_close_request() {
        let run = Uuid::new_v4();
        let job = create(1, run).unwrap();
        assert_eq!(request_close(&job).unwrap(), 0);
    }

    #[test]
    fn an_existing_job_name_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let first = test_boundary(run, root.path()).unwrap();
        assert!(create(1, run).is_err());
        drop(first);
    }

    #[test]
    fn the_named_job_denies_control_opens() {
        let root = tempfile::tempdir().unwrap();
        let run = Uuid::new_v4();
        let job = test_boundary(run, root.path()).unwrap();
        let name = job_name(run);
        let denied = unsafe {
            OpenJobObjectW(
                windows_sys::Win32::System::SystemServices::JOB_OBJECT_SET_ATTRIBUTES,
                0,
                name.as_ptr(),
            )
        };
        assert!(denied.is_null());
        assert_eq!(
            io::Error::last_os_error().raw_os_error(),
            Some(windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED as i32)
        );
        drop(job);
    }
}
