use std::{
    ffi::{OsStr, OsString},
    fs,
    io::{self, Error, ErrorKind},
    mem::{size_of, size_of_val},
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    process::Command,
    ptr::{null, null_mut},
    time::{SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::ConvertSidToStringSidW, Isolation::*, *},
    System::{Com::CoTaskMemFree, JobObjects::*, Threading::*},
};

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

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
struct Profile {
    name: Vec<u16>,
    sid: PSID,
}
impl Drop for Profile {
    fn drop(&mut self) {
        unsafe {
            FreeSid(self.sid);
            let result = DeleteAppContainerProfile(self.name.as_ptr());
            if result < 0 {
                eprintln!("Transient profile cleanup HRESULT {result:#010x}");
            }
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
    fn new(label: &str) -> io::Result<Self> {
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
        Ok(Self { name, sid })
    }
    fn folder(&self) -> io::Result<PathBuf> {
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

fn check_sid(process: HANDLE, sid: PSID) -> io::Result<()> {
    unsafe {
        let mut token = null_mut();
        win(OpenProcessToken(process, TOKEN_QUERY, &mut token))?;
        let token = Handle(token);
        let mut bytes = 0;
        GetTokenInformation(token.0, TokenAppContainerSid, null_mut(), 0, &mut bytes);
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
            TokenAppContainerSid,
            data.as_mut_ptr().cast(),
            bytes,
            &mut bytes,
        ))?;
        let observed = (*data.as_ptr().cast::<TOKEN_APPCONTAINER_INFORMATION>()).TokenAppContainer;
        if observed.is_null() || EqualSid(observed, sid) == 0 {
            return Err(Error::other("Unexpected sandbox identity"));
        }
        Ok(())
    }
}

fn run_contained(profile: &Profile, own: &Path, other: &Path) -> io::Result<()> {
    let probe = own.join("probe.exe");
    fs::copy(std::env::current_exe()?, &probe)?;
    let file = wide(&probe);
    let cwd = wide(own);
    let mut cmd = wide(format!(
        "\"{}\" parent \"{}\"",
        probe.display(),
        other.display()
    ));
    let mut system_root = [0u16; 260];
    let len = unsafe {
        windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW(
            system_root.as_mut_ptr(),
            260,
        )
    };
    if len == 0 || len >= 260 {
        return Err(Error::other("Windows directory lookup failed"));
    }
    let base = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| Error::other("Windows local app data directory is unavailable"))?;
    let mut env = wide(format!("LOCALAPPDATA={}", base.to_string_lossy()));
    env.extend(wide(format!(
        "SystemRoot={}",
        OsString::from_wide(&system_root[..len as usize]).to_string_lossy()
    )));
    env.push(0);
    unsafe {
        let job = Handle(CreateJobObjectW(null(), null()));
        if job.0.is_null() {
            return Err(Error::other(format!(
                "{} at {}",
                Error::last_os_error(),
                std::panic::Location::caller()
            )));
        }
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        win(SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            std::ptr::from_ref(&limits).cast(),
            size_of_val(&limits) as u32,
        ))?;
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
            file.as_ptr(),
            cmd.as_mut_ptr(),
            null(),
            null(),
            0,
            CREATE_SUSPENDED
                | EXTENDED_STARTUPINFO_PRESENT
                | CREATE_UNICODE_ENVIRONMENT
                | CREATE_NO_WINDOW,
            env.as_ptr().cast(),
            cwd.as_ptr(),
            &startup.StartupInfo,
            &mut info,
        ))?;
        let process = Handle(info.hProcess);
        let thread = Handle(info.hThread);
        check_sid(process.0, profile.sid)?;
        if ResumeThread(thread.0) == u32::MAX {
            return Err(Error::other(format!(
                "{} at {}",
                Error::last_os_error(),
                std::panic::Location::caller()
            )));
        }
        if WaitForSingleObject(process.0, 10000) != WAIT_OBJECT_0 {
            return Err(Error::other("Probe timed out"));
        }
        let mut exit = 0;
        win(GetExitCodeProcess(process.0, &mut exit))?;
        if exit != 0 {
            return Err(Error::other(format!("Probe exited {exit:#x}")));
        }
    }
    if fs::read(own.join("child-result"))? != b"checked"
        || fs::read(own.join("parent-result"))? != b"checked"
    {
        return Err(Error::other("Missing fixture observation"));
    }
    Ok(())
}

// These two modes only access fixture markers and run after verifying the calling token.
fn worker(mode: &str, other: &OsStr) -> io::Result<()> {
    unsafe {
        let mut token = null_mut();
        win(OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_QUERY,
            &mut token,
        ))?;
        let token = Handle(token);
        let mut contained = 0u32;
        let mut bytes = 0;
        win(GetTokenInformation(
            token.0,
            TokenIsAppContainer,
            std::ptr::from_mut(&mut contained).cast(),
            4,
            &mut bytes,
        ))?;
        if contained != 1 {
            return Err(Error::other("Probe worker requires an AppContainer"));
        }
    }
    let mut in_job = 0;
    unsafe {
        win(IsProcessInJob(GetCurrentProcess(), null_mut(), &mut in_job))?;
    }
    if in_job == 0 {
        return Err(Error::other("Probe worker is outside a job"));
    }
    let neighbor = Path::new(other).join("private-input");
    if fs::read(&neighbor).err().map(|e| e.kind()) != Some(ErrorKind::PermissionDenied) {
        return Err(Error::other("Expected access denial for sibling input"));
    }
    if fs::read("private-input")? != b"private" {
        return Err(Error::other("Private read failed"));
    }
    if mode == "parent" {
        if Path::new("runtime-path").exists() {
            let runtime = fs::read_to_string("runtime-path")?;
            let status = Command::new(runtime)
                .args([
                    "-XX:-UsePerfData",
                    "-Xmx128m",
                    "-Djava.io.tmpdir=.",
                    "-cp",
                    ".",
                    "FixtureProcessor",
                ])
                .arg(other)
                .stdout(fs::File::create("java-stdout")?)
                .stderr(fs::File::create("java-stderr")?)
                .status()?;
            if !status.success() {
                return Err(Error::other("Java fixture failed"));
            }
        }
        let status = Command::new(std::env::current_exe()?)
            .arg("child")
            .arg(other)
            .status()?;
        if !status.success() {
            return Err(Error::other(format!("Child failed {status}")));
        }
    }
    fs::write(format!("{mode}-result"), b"checked")
}

pub fn run() -> io::Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let java = match args.as_slice() {
        [mode, other] if mode == "parent" || mode == "child" => {
            return worker(mode.to_str().unwrap(), other)
        }
        [] => None,
        [flag, path]
            if flag == "--java-fixture"
                && Path::new(path).is_absolute()
                && Path::new(path)
                    .file_name()
                    .is_some_and(|f| f.eq_ignore_ascii_case("java.exe")) =>
        {
            Some(path)
        }
        _ => return Err(Error::other(
            "Use no arguments, or --java-fixture followed by the absolute installed java.exe path",
        )),
    };
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(Error::other)?
        .as_nanos();
    let prefix = format!("CarbonDesktop.Probe.{}.{nonce:x}", std::process::id());
    let a = Profile::new(&format!("{prefix}.a"))?;
    let b = Profile::new(&format!("{prefix}.b"))?;
    let fa = a.folder()?.join("LocalState");
    let fb = b.folder()?.join("LocalState");
    for path in [&fa, &fb] {
        fs::create_dir_all(path)?;
        fs::write(path.join("private-input"), b"private")?;
    }
    if let Some(java) = java {
        fs::write(
            fa.join("runtime-path"),
            java.to_str()
                .ok_or_else(|| Error::other("Unsupported runtime path encoding"))?,
        )?;
        let compiled = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.artifacts/java-probe/FixtureProcessor.class");
        fs::copy(compiled, fa.join("FixtureProcessor.class")).map_err(|_| {
            Error::other(
                "Build the owned fixture class with scripts/windows-java-fixture.ps1 first",
            )
        })?;
    }
    let attempt = run_contained(&a, &fa, &fb);
    if java.is_some() {
        if attempt.is_err() {
            // Only the fixed, credential-free developer fixture produces this output.
            if let Ok(stderr) = fs::read_to_string(fa.join("java-stderr")) {
                eprintln!("{}", stderr.chars().take(1200).collect::<String>());
            }
        }
        attempt?;
        for marker in ["processor-result", "window-result"] {
            if fs::read(fa.join(marker))? != b"checked" {
                return Err(Error::other("Missing Java observation"));
            }
        }
        println!(
            "PASS Java fixture processor, scoped file checks and creation of an AWT native peer."
        );
    } else {
        attempt?;
    }
    run_contained(&b, &fb, &fa)?;
    println!("PASS two AppContainer identities, token checked before resume, supervised process tree, private writes and cross-instance read denial for parent and child.");
    Ok(())
}
