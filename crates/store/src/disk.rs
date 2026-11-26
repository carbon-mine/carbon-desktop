use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;

use serde::{de::DeserializeOwned, Serialize};

pub(crate) fn error(label: &str, e: io::Error) -> String {
    format!("{label}: {e}")
}

fn ordinary(meta: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    !meta.file_type().is_symlink()
}

pub(crate) fn directory(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    let created = {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(path)
    };
    #[cfg(not(unix))]
    let created = fs::create_dir(path);
    match created {
        Ok(()) => sync_parent(path)?,
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(error("Could not create fixture directory", e)),
    }
    check_directory(path)
}

pub(crate) fn check_directory(path: &Path) -> Result<(), String> {
    let meta = fs::symlink_metadata(path).map_err(|e| error("Could not inspect directory", e))?;
    if ordinary(&meta) && meta.is_dir() {
        Ok(())
    } else {
        Err(
            "Fixture directory must be a normal directory. Links and reparse points are refused."
                .into(),
        )
    }
}

pub(crate) fn existing_file(path: &Path) -> Result<Option<Metadata>, String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if ordinary(&meta) && meta.is_file() => Ok(Some(meta)),
        Ok(_) => {
            Err("Fixture path must be a regular file. Links and special files are refused.".into())
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(error("Could not inspect fixture file", e)),
    }
}

pub(crate) fn lockfile(path: &Path) -> Result<File, String> {
    existing_file(path)?;
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| error("Could not open fixture lock", e))
}

fn sync_parent(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        File::open(path.parent().ok_or("Missing fixture parent")?)
            .and_then(|file| file.sync_all())
            .map_err(|e| error("Could not sync fixture directory", e))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    existing_file(path)?;
    let scratch = path.with_file_name(".fixture-write.tmp");
    // The exclusive store lock serializes all writers. Never open a stale scratch file for writing.
    if existing_file(&scratch)?.is_some() {
        fs::remove_file(&scratch).map_err(|e| error("Could not release interrupted write", e))?;
    }
    let result = (|| {
        let mut file = File::create_new(&scratch)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        #[cfg(test)]
        crate::tests::fault_before_publish(path, bytes)?;
        fs::rename(&scratch, path)?;
        #[cfg(test)]
        crate::tests::fault_after_publish(path, bytes);
        Ok(())
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&scratch);
        return Err(error("Could not publish fixture file", e));
    }
    sync_parent(path)
}

pub(crate) fn read_bytes(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|e| error("Could not read fixture file", e))?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| error("Could not read fixture file", e))?;
    if bytes.len() as u64 > limit {
        return Err("Fixture file exceeds its allowed size.".into());
    }
    Ok(bytes)
}

pub(crate) fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    existing_file(path)?.ok_or("A fixture metadata or player file is missing. Preserve the library and recover it from a backup.")?;
    serde_json::from_slice(&read_bytes(path, 16_384)?).map_err(|_| {
        "Fixture metadata or player data is unreadable. Preserve it for recovery.".into()
    })
}

pub(crate) fn write_json(path: &Path, state: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec(state).map_err(|_| "Could not encode fixture state.")?;
    atomic_write(path, &bytes)
}

pub(crate) fn allocation(path: &Path, meta: &Metadata) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let _ = path;
        Some(meta.blocks() * 512)
    }
    #[cfg(windows)]
    {
        use std::mem::{size_of, MaybeUninit};
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            FileStandardInfo, GetFileInformationByHandleEx, FILE_STANDARD_INFO,
        };
        let _ = meta;
        let file = File::open(path).ok()?;
        let mut info = MaybeUninit::<FILE_STANDARD_INFO>::uninit();
        // The OS fills the fixed-size FILE_STANDARD_INFO for this live regular file handle.
        let ok = unsafe {
            GetFileInformationByHandleEx(
                file.as_raw_handle(),
                FileStandardInfo,
                info.as_mut_ptr().cast(),
                size_of::<FILE_STANDARD_INFO>() as u32,
            )
        };
        if ok == 0 {
            return None;
        }
        u64::try_from(unsafe { info.assume_init() }.AllocationSize).ok()
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (path, meta);
        None
    }
}
