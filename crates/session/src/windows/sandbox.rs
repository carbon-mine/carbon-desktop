// SPDX-License-Identifier: MPL-2.0
// Process and low-box helpers follow the repository-owned M1 isolation probe.
mod job;
mod runs;

pub(super) use runs::RunGuard;

use std::{
    ffi::{OsStr, OsString},
    io::{self, Error},
    mem::{size_of, size_of_val},
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    sync::Arc,
};
use uuid::Uuid;
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::ConvertSidToStringSidW, Isolation::*, *},
    Storage::FileSystem::{FILE_ALL_ACCESS, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ},
    System::{Com::CoTaskMemFree, JobObjects::*, Threading::*},
};

#[cfg(test)]
mod state_worker;

fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain([0]).collect()
}
#[track_caller]
fn win(ok: i32) -> io::Result<()> {
    if ok == 0 {
        Err(Error::other(format!(
            "{} at {}",
            Error::last_os_error(),
            std::panic::Location::caller()
        )))
    } else {
        Ok(())
    }
}
fn hr(value: i32) -> io::Result<()> {
    if value < 0 {
        Err(Error::other(format!("Windows HRESULT {value:#010x}")))
    } else {
        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
// A Win32 handle is valid for the whole process, and the job and process calls
// that borrow it are documented as thread-safe. The run directory shares one
// duplicated job handle between the owning runner and a stop request.
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}
pub(super) struct Profile {
    pub(super) sid: PSID,
}
impl Drop for Profile {
    fn drop(&mut self) {
        unsafe {
            FreeSid(self.sid);
        }
    }
}

unsafe fn os_string(ptr: *const u16) -> OsString {
    let mut len = 0;
    while *ptr.add(len) != 0 {
        len += 1;
    }
    OsString::from_wide(std::slice::from_raw_parts(ptr, len))
}
impl Profile {
    pub(super) fn new(label: &str) -> io::Result<Self> {
        let name = wide(label);
        let mut sid = null_mut();
        unsafe {
            hr(CreateAppContainerProfile(
                name.as_ptr(),
                name.as_ptr(),
                name.as_ptr(),
                null(),
                0,
                &mut sid,
            ))?;
        }
        Ok(Self { sid })
    }
    pub(super) fn folder(&self) -> io::Result<PathBuf> {
        unsafe {
            let mut string_sid = null_mut();
            win(ConvertSidToStringSidW(self.sid, &mut string_sid))?;
            let mut path = null_mut();
            let result = GetAppContainerFolderPath(string_sid, &mut path);
            LocalFree(string_sid.cast());
            hr(result)?;
            let folder = PathBuf::from(os_string(path));
            CoTaskMemFree(path.cast());
            Ok(folder)
        }
    }
}

pub(super) fn delete_profile(name: &str) -> io::Result<()> {
    let name = wide(name);
    // Windows also returns success for a profile that was never registered.
    hr(unsafe { DeleteAppContainerProfile(name.as_ptr()) })
}

struct Attributes {
    backing: Vec<usize>,
}
impl Attributes {
    fn new(count: u32) -> io::Result<Self> {
        let mut bytes = 0;
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), count, 0, &mut bytes);
        }
        if bytes == 0 {
            return Err(Error::other(format!(
                "{} at {}",
                Error::last_os_error(),
                std::panic::Location::caller()
            )));
        }
        let mut backing = vec![0usize; bytes.div_ceil(size_of::<usize>())];
        unsafe {
            win(InitializeProcThreadAttributeList(
                backing.as_mut_ptr().cast(),
                count,
                0,
                &mut bytes,
            ))?;
        }
        Ok(Self { backing })
    }
    fn set<T>(&mut self, id: u32, value: &T) -> io::Result<()> {
        unsafe {
            win(UpdateProcThreadAttribute(
                self.backing.as_mut_ptr().cast(),
                0,
                id as usize,
                std::ptr::from_ref(value).cast(),
                size_of_val(value),
                null_mut(),
                null(),
            ))
        }
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe {
            DeleteProcThreadAttributeList(self.backing.as_mut_ptr().cast());
        }
    }
}

/// The launcher's own user SID, used to build a query-only job DACL.
pub(super) fn current_user() -> io::Result<Vec<usize>> {
    token_data(unsafe { GetCurrentProcess() }, TokenUser)
}

fn token_data(process: HANDLE, class: TOKEN_INFORMATION_CLASS) -> io::Result<Vec<usize>> {
    unsafe {
        let mut token = null_mut();
        win(OpenProcessToken(process, TOKEN_QUERY, &mut token))?;
        let token = Handle(token);
        let mut bytes = 0;
        GetTokenInformation(token.0, class, null_mut(), 0, &mut bytes);
        if bytes == 0 {
            return Err(Error::other(format!(
                "{} at {}",
                Error::last_os_error(),
                std::panic::Location::caller()
            )));
        }
        let mut data = vec![0usize; (bytes as usize).div_ceil(size_of::<usize>())];
        win(GetTokenInformation(
            token.0,
            class,
            data.as_mut_ptr().cast(),
            bytes,
            &mut bytes,
        ))?;
        Ok(data)
    }
}

fn check_token(process: HANDLE, sid: PSID) -> io::Result<()> {
    let identity = token_data(process, TokenAppContainerSid)?;
    let capabilities = token_data(process, TokenCapabilities)?;
    unsafe {
        let observed =
            (*identity.as_ptr().cast::<TOKEN_APPCONTAINER_INFORMATION>()).TokenAppContainer;
        // GroupCount is the first field, including when there are no capability groups.
        let group_count = *capabilities.as_ptr().cast::<u32>();
        if observed.is_null() || EqualSid(observed, sid) == 0 || group_count != 0 {
            return Err(Error::other("Unexpected sandbox identity or capabilities"));
        }
    }
    Ok(())
}

struct LocalBlock(*mut core::ffi::c_void);
impl Drop for LocalBlock {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}

pub(super) fn grant_read_execute(root: &Path, profile: &Profile) -> io::Result<()> {
    use windows_sys::Win32::Security::Authorization::*;
    let name = wide(root);
    unsafe {
        let owner_data = token_data(GetCurrentProcess(), TokenUser)?;
        let owner = (*owner_data.as_ptr().cast::<TOKEN_USER>()).User.Sid;
        let entry = |sid: PSID, permissions| EXPLICIT_ACCESS_W {
            grfAccessPermissions: permissions,
            grfAccessMode: GRANT_ACCESS,
            grfInheritance: SUB_CONTAINERS_AND_OBJECTS_INHERIT,
            Trustee: TRUSTEE_W {
                TrusteeForm: TRUSTEE_IS_SID,
                TrusteeType: TRUSTEE_IS_UNKNOWN,
                ptstrName: sid.cast(),
                ..Default::default()
            },
        };
        let entries = [
            entry(owner, FILE_ALL_ACCESS),
            entry(profile.sid, FILE_GENERIC_READ | FILE_GENERIC_EXECUTE),
        ];
        let mut acl = null_mut();
        let status = SetEntriesInAclW(entries.len() as u32, entries.as_ptr(), null(), &mut acl);
        if status != 0 {
            return Err(Error::from_raw_os_error(status as i32));
        }
        let _acl = LocalBlock(acl.cast());
        let status = SetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            acl,
            null_mut(),
        );
        if status != 0 {
            return Err(Error::from_raw_os_error(status as i32));
        }
    }
    Ok(())
}

fn quoted_command(args: &[&OsStr]) -> io::Result<Vec<u16>> {
    let mut value = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        let units: Vec<_> = arg.encode_wide().collect();
        if units.iter().any(|&c| matches!(c, 0 | 34)) || units.last() == Some(&(b'\\' as u16)) {
            return Err(Error::other("Unsupported Windows fixture argument"));
        }
        if i != 0 {
            value.push(b' ' as u16);
        }
        value.push(b'"' as u16);
        value.extend(units);
        value.push(b'"' as u16);
    }
    value.push(0);
    Ok(value)
}

fn environment(own: &Path) -> io::Result<Vec<u16>> {
    let mut directory = [0u16; 260];
    let len = unsafe {
        windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW(
            directory.as_mut_ptr(),
            directory.len() as u32,
        )
    };
    if len == 0 || len as usize >= directory.len() {
        return Err(Error::other("Windows directory unavailable"));
    }
    let appdata = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| Error::other("Windows local app data unavailable"))?;
    let mut result = Vec::new();
    let system = OsString::from_wide(&directory[..len as usize]);
    for (name, val) in [
        ("LOCALAPPDATA=", appdata.as_os_str()),
        ("SystemRoot=", system.as_os_str()),
        ("TEMP=", own.as_os_str()),
        ("TMP=", own.as_os_str()),
    ] {
        result.extend(OsStr::new(name).encode_wide());
        result.extend(wide(val));
    }
    result.push(0);
    Ok(result)
}

pub(super) struct LaunchObservation {
    pub(super) smoke: bool,
    pub(super) run: Uuid,
    pub(super) instance: Uuid,
    pub(super) boundary_empty: bool,
    pub(super) stopped: bool,
}

fn stopped_error() -> Error {
    Error::new(
        io::ErrorKind::Interrupted,
        "The protected run was stopped before it finished",
    )
}

/// Terminates a suspended root that must not be resumed, drains its job and
/// waits for the process handle. Every step is attempted so a failure still
/// reports the observed boundary rather than assuming it is empty.
pub(super) fn stop_suspended(process: HANDLE, job: &Handle) -> io::Result<()> {
    let terminated = unsafe { win(TerminateProcess(process, 1)) };
    let drained = job::stop_and_drain(job);
    match unsafe { WaitForSingleObject(process, job::DRAIN_TIMEOUT.as_millis() as u32) } {
        WAIT_OBJECT_0 => (),
        WAIT_TIMEOUT => {
            terminated?;
            drained?;
            return Err(Error::new(
                io::ErrorKind::TimedOut,
                "The stopped session did not exit",
            ));
        }
        _ => {
            terminated?;
            drained?;
            return Err(Error::last_os_error());
        }
    }
    terminated?;
    drained?;
    Ok(())
}

pub(super) fn launch(
    profile: &Profile,
    java: &Path,
    own: &Path,
    classpath: &Path,
    foreign: &Path,
    read_only: &Path,
    observation: &mut LaunchObservation,
) -> io::Result<RunGuard> {
    let java_path = wide(java);
    let working = wide(own);
    let mut tmp = OsString::from("-Djava.io.tmpdir=");
    tmp.push(own);
    let mut home = OsString::from("-Duser.home=");
    home.push(own);
    let mut native = OsString::from("-Djava.library.path=");
    native.push(
        java.parent()
            .ok_or_else(|| Error::other("Missing Java bin directory"))?,
    );
    let mut command = quoted_command(&[
        java.as_os_str(),
        OsStr::new("-XX:-UsePerfData"),
        OsStr::new("-Xmx128m"),
        &tmp,
        &home,
        &native,
        OsStr::new("-cp"),
        classpath.as_os_str(),
        OsStr::new("FixtureSession"),
        foreign.as_os_str(),
        read_only.as_os_str(),
        OsStr::new(if observation.smoke {
            "smoke"
        } else {
            "interactive"
        }),
    ])?;
    let env = environment(own)?;
    let job = job::create(1, observation.run)?;
    let run = runs::register(observation.run, observation.instance, Arc::clone(&job));
    unsafe {
        let caps = SECURITY_CAPABILITIES {
            AppContainerSid: profile.sid,
            ..Default::default()
        };
        let mut attrs = Attributes::new(2)?;
        attrs.set(PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES, &caps)?;
        attrs.set(PROC_THREAD_ATTRIBUTE_JOB_LIST, &job.0)?;
        let mut startup = STARTUPINFOEXW::default();
        startup.StartupInfo.cb = size_of_val(&startup) as u32;
        startup.lpAttributeList = attrs.backing.as_mut_ptr().cast();
        let mut info = PROCESS_INFORMATION::default();
        win(CreateProcessW(
            java_path.as_ptr(),
            command.as_mut_ptr(),
            null(),
            null(),
            0,
            CREATE_SUSPENDED
                | EXTENDED_STARTUPINFO_PRESENT
                | CREATE_UNICODE_ENVIRONMENT
                | CREATE_NO_WINDOW,
            env.as_ptr().cast(),
            working.as_ptr(),
            &startup.StartupInfo,
            &mut info,
        ))?;
        observation.boundary_empty = false;
        let process = Handle(info.hProcess);
        let thread = Handle(info.hThread);
        if run.stopped() {
            // A stop accepted before process creation must not let the payload
            // run: the suspended root is now a member of that stopped job.
            if let Err(error) = stop_suspended(process.0, &job) {
                // Without a confirmed drain the record must not claim the stop
                // settled, even though the run never reached the caller.
                run.unsettle();
                return Err(error);
            }
            observation.boundary_empty = true;
            observation.stopped = true;
            return Err(stopped_error());
        }
        let verified = check_token(process.0, profile.sid).and_then(|()| {
            let mut in_job = 0;
            win(IsProcessInJob(process.0, job.0, &mut in_job))?;
            if in_job != 0 {
                Ok(())
            } else {
                Err(Error::other(
                    "Process is outside its designated containment job",
                ))
            }
        });
        if let Err(error) = verified {
            // Terminate the suspended process explicitly if any pre-resume check fails.
            let terminated = win(TerminateProcess(process.0, 1));
            let drained = job::stop_and_drain(&job);
            // An unverified root may be outside the job. Termination is asynchronous.
            match WaitForSingleObject(process.0, job::DRAIN_TIMEOUT.as_millis() as u32) {
                WAIT_OBJECT_0 => (),
                WAIT_TIMEOUT => {
                    terminated?;
                    return Err(Error::new(
                        io::ErrorKind::TimedOut,
                        "The rejected session did not exit",
                    ));
                }
                _ => return Err(Error::last_os_error()),
            }
            if let Err(error) = drained {
                run.unsettle();
                return Err(error);
            }
            observation.boundary_empty = true;
            observation.stopped = run.stopped();
            return Err(error);
        }
        if run.stopped() {
            // A stop accepted after validation must still prevent the resume.
            if let Err(error) = stop_suspended(process.0, &job) {
                run.unsettle();
                return Err(error);
            }
            observation.boundary_empty = true;
            observation.stopped = true;
            return Err(stopped_error());
        }
        if ResumeThread(thread.0) == u32::MAX {
            let error = Error::last_os_error();
            if let Err(error) = job::stop_and_drain(&job) {
                run.unsettle();
                return Err(error);
            }
            observation.boundary_empty = true;
            observation.stopped = run.stopped();
            return Err(error);
        }
        run.running();
        let deadline = if observation.smoke {
            30_000
        } else {
            30 * 60_000
        };
        let wait = WaitForSingleObject(process.0, deadline);
        if wait != WAIT_OBJECT_0 {
            if run.stopped() {
                // The stop request terminated the root; drain what remains of its tree.
                if job::wait_empty(&job, job::DRAIN_TIMEOUT).is_ok() {
                    observation.boundary_empty = true;
                } else {
                    run.unsettle();
                }
                observation.stopped = true;
                return Err(stopped_error());
            }
            let error = if wait == WAIT_TIMEOUT {
                Error::new(
                    io::ErrorKind::TimedOut,
                    "The fixed Java session did not exit in time",
                )
            } else {
                Error::last_os_error()
            };
            if let Err(error) = job::stop_and_drain(&job) {
                run.unsettle();
                return Err(error);
            }
            observation.boundary_empty = true;
            return Err(error);
        }
        let drain = job::wait_empty(&job, job::DRAIN_TIMEOUT);
        if let Err(error) = drain {
            if run.stopped() {
                if let Err(error) = job::stop_and_drain(&job) {
                    run.unsettle();
                    return Err(error);
                }
                observation.boundary_empty = true;
                observation.stopped = true;
                return Err(stopped_error());
            }
            if let Err(error) = job::stop_and_drain(&job) {
                run.unsettle();
                return Err(error);
            }
            observation.boundary_empty = true;
            return Err(error);
        }
        if !run.finish() {
            // A stop request claimed the run before the owner observed the empty
            // tree, so the completed sample is not published.
            observation.boundary_empty = true;
            observation.stopped = true;
            return Err(stopped_error());
        }
        observation.boundary_empty = true;
        let mut exit = 0;
        if let Err(error) = win(GetExitCodeProcess(process.0, &mut exit)) {
            // The tree drained but the sample's own result was not verified, so
            // the terminal record must not claim a published completion.
            run.unsettle();
            return Err(error);
        }
        if exit != 0 {
            run.unsettle();
            return Err(Error::other(format!(
                "The fixed Java session exited {exit:#x}"
            )));
        }
    }
    // The caller now owns finalization: it keeps the guard alive through its
    // post-launch checks and only then lets the terminal record settle.
    Ok(run)
}

pub(super) fn boundary_empty(run: Uuid) -> io::Result<bool> {
    job::boundary_empty(run)
}

pub(super) fn active_runs() -> Vec<crate::RunState> {
    runs::active()
}

pub(super) fn inspect_run(run: Uuid) -> Option<crate::RunState> {
    runs::inspect(run)
}

pub(super) fn stop_run(run: Uuid) -> Result<crate::RunState, crate::RunControlError> {
    runs::stop(run)
}

pub(super) fn subscribe(run: Uuid) -> Result<crate::RunSubscription, crate::RunControlError> {
    runs::subscribe(run)
}

pub(super) fn outcome(run: Uuid) -> Option<crate::RunOutcome> {
    runs::outcome(run)
}

#[cfg(test)]
pub(super) fn test_boundary(run: Uuid, root: &Path) -> io::Result<Arc<Handle>> {
    job::test_boundary(run, root)
}
