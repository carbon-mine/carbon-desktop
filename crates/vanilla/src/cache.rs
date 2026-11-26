use super::{verified, Origin, Pin, PreparedVersion, VERSION_LIMIT};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub(super) fn ordinary(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    !meta.is_symlink()
}

pub(super) fn directory(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    let result = {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(path)
    };
    #[cfg(not(unix))]
    let result = fs::create_dir(path);
    match result {
        Ok(()) => sync_dir(path.parent().ok_or("Missing metadata directory parent.")?)?,
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err("Could not create the vanilla metadata cache directory.".into()),
    }
    let meta = fs::symlink_metadata(path)
        .map_err(|_| "Could not inspect the metadata cache directory.")?;
    if meta.is_dir() && ordinary(&meta) {
        Ok(())
    } else {
        Err("Expected an ordinary metadata cache directory. Linked directories are refused.".into())
    }
}

pub(super) fn regular(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() && ordinary(&meta) => Ok(true),
        Ok(_) => Err(
            "Expected an ordinary metadata cache file. Links and special files are refused.".into(),
        ),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err("Could not inspect the vanilla metadata cache file.".into()),
    }
}

pub(super) fn sync_dir(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Could not confirm cache directory durability. Retry preparation.")?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

pub(super) fn read(path: &Path, limit: u64) -> Result<Option<Vec<u8>>, String> {
    if !regular(path)? {
        return Ok(None);
    }
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|f| f.take(limit + 1).read_to_end(&mut bytes))
        .map_err(|_| "Could not read the metadata cache. Check its permissions and retry.")?;
    Ok((bytes.len() as u64 <= limit).then_some(bytes))
}

pub(super) fn publish(path: &Path, raw: &[u8]) -> Result<(), String> {
    regular(path)?;
    let parent = path.parent().ok_or("Missing metadata cache directory.")?;
    let mut scratch = tempfile::NamedTempFile::new_in(parent)
        .map_err(|_| "Could not stage metadata. Check free space and retry.")?;
    scratch
        .write_all(raw)
        .and_then(|_| scratch.as_file().sync_all())
        .map_err(|_| "Could not finish writing metadata. Check free space and retry.")?;
    scratch
        .persist(path)
        .map_err(|_| "Could not publish verified metadata. Check cache permissions and retry.")?;
    sync_dir(parent)
}

pub(super) struct Cache {
    dir: PathBuf,
    _lock: File,
}

impl Cache {
    pub(super) fn client_dir(&self) -> Result<PathBuf, String> {
        let dir = self.dir.join("clients");
        directory(&dir)?;
        Ok(dir)
    }
    pub(super) fn open(root: &Path) -> Result<Self, String> {
        let dir = root.join("vanilla-metadata-v1");
        directory(&dir)?;
        let lock = dir.join("cache.lock");
        regular(&lock)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock)
            .map_err(|_| "Could not open the vanilla metadata cache lock.")?;
        lock.lock()
            .map_err(|_| "Could not lock the vanilla metadata cache.")?;
        directory(&dir.join("objects"))?;
        directory(&dir.join("versions"))?;
        Ok(Self { dir, _lock: lock })
    }
    fn pointer(&self, id: &str) -> PathBuf {
        self.dir.join("versions").join(format!("{id}.json"))
    }
    fn object(&self, digest: &str) -> PathBuf {
        self.dir.join("objects").join(digest)
    }

    pub(super) fn pin(&self, id: &str, kind: &str) -> Result<Option<Pin>, String> {
        let Some(raw) = read(&self.pointer(id), 4096)? else {
            return Ok(None);
        };
        Ok(serde_json::from_slice::<Pin>(&raw)
            .ok()
            .filter(|p| p.valid_for(id, kind)))
    }

    pub(super) fn raw_version(&self, info: &PreparedVersion) -> Result<Vec<u8>, String> {
        let pin = self
            .pin(&info.version, &info.release_type)?
            .ok_or("Prepare the pinned version metadata first.")?;
        if pin.sha1 != info.metadata_sha1 {
            return Err("The version metadata changed. Review the game file plan again.".into());
        }
        let raw = read(&self.object(&pin.sha1), VERSION_LIMIT)?
            .ok_or("Verified version metadata is missing. Prepare metadata online again.")?;
        verified(&raw, &pin, info.profile_id, Origin::Cache)?;
        Ok(raw)
    }

    pub(super) fn read(
        &self,
        id: &str,
        kind: &str,
        profile_id: Uuid,
    ) -> Result<Option<PreparedVersion>, String> {
        let Some(pin) = self.pin(id, kind)? else {
            return Ok(None);
        };
        let Some(raw) = read(&self.object(&pin.sha1), VERSION_LIMIT)? else {
            return Ok(None);
        };
        Ok(verified(&raw, &pin, profile_id, Origin::Cache).ok())
    }

    pub(super) fn publish(&self, pin: &Pin, raw: &[u8]) -> Result<(), String> {
        let pin_bytes =
            serde_json::to_vec(pin).map_err(|_| "Could not encode the verified Mojang pin.")?;
        publish(&self.object(&pin.sha1), raw)?;
        publish(&self.pointer(&pin.id), &pin_bytes)
    }
}
