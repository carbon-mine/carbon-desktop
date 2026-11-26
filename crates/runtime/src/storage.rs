// SPDX-License-Identifier: MPL-2.0
use crate::RuntimeError;
use std::{
    fs::{self, File, Metadata, OpenOptions, TryLockError},
    io,
    path::{Component, Path},
};

pub(crate) fn is_link(meta: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0 // FILE_ATTRIBUTE_REPARSE_POINT
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}

pub(crate) fn check_root(root: &Path) -> Result<bool, RuntimeError> {
    if !root.is_absolute() || root.components().any(|c| c == Component::ParentDir) {
        return Err(RuntimeError::UnsafeStorage);
    }
    #[cfg(windows)]
    {
        use std::path::Prefix;
        if !matches!(
            root.components().next(),
            Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
        ) {
            return Err(RuntimeError::UnsafeStorage);
        }
    }
    let mut root_exists = true;
    for ancestor in root.ancestors() {
        // A Windows drive/UNC prefix alone does not name a directory.
        if !ancestor.has_root() {
            continue;
        }
        match fs::symlink_metadata(ancestor) {
            Ok(meta) if meta.is_dir() && !is_link(&meta) => (),
            Ok(_) => return Err(RuntimeError::UnsafeStorage),
            Err(e) if e.kind() == io::ErrorKind::NotFound => root_exists = false,
            Err(e) => return Err(e.into()),
        }
    }
    Ok(root_exists)
}

pub(crate) fn ensure_root(root: &Path) -> Result<(), RuntimeError> {
    if !check_root(root)? {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(root)?;
        if !check_root(root)? {
            return Err(RuntimeError::UnsafeStorage);
        }
    }
    Ok(())
}

pub(crate) fn open_regular(path: &Path, may_create: bool) -> Result<File, RuntimeError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() && !is_link(&meta) => (),
        Ok(_) => return Err(RuntimeError::UnsafeStorage),
        Err(e) if e.kind() == io::ErrorKind::NotFound && may_create => (),
        Err(e) => return Err(e.into()),
    }
    let mut options = OpenOptions::new();
    options.read(true).write(may_create).create(may_create);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .mode(0o600);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let file = options.open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() || is_link(&meta) || !one_link(&file, &meta)? {
        return Err(RuntimeError::UnsafeStorage);
    }
    Ok(file)
}

fn one_link(_file: &File, _meta: &Metadata) -> io::Result<bool> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(_meta.nlink() == 1)
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // The live file handle and initialized writable structure outlive the call.
        if unsafe { GetFileInformationByHandle(_file.as_raw_handle(), &mut info) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(info.nNumberOfLinks == 1)
    }
    #[cfg(not(any(unix, windows)))]
    {
        Ok(true)
    }
}

pub(crate) fn lock(root: &Path, major: u16, write: bool) -> Result<File, RuntimeError> {
    let file = open_regular(&root.join(format!("java-{major}.lock")), write)?;
    let acquired = if write {
        file.try_lock()
    } else {
        file.try_lock_shared()
    };
    acquired.map_err(|e| match e {
        TryLockError::WouldBlock => RuntimeError::Busy,
        TryLockError::Error(e) => e.into(),
    })?;
    Ok(file)
}

pub(crate) fn remove_stale_stages(root: &Path, prefix: &str) -> Result<(), RuntimeError> {
    for item in fs::read_dir(root)? {
        let item = item?;
        if item.file_name().to_str().is_some_and(|name| {
            name.strip_prefix(prefix)
                .is_some_and(|id| id.len() == 12 && id.bytes().all(|c| c.is_ascii_alphanumeric()))
        }) {
            let meta = fs::symlink_metadata(item.path())?;
            if meta.is_dir() && !is_link(&meta) {
                fs::remove_dir_all(item.path())?;
            }
        }
    }
    Ok(())
}

pub(crate) fn sync_dir(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
