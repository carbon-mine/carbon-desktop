use crate::{Library, Profile};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::{self, Read, Write},
    path::Path,
};

const MAX_INDEX_BYTES: u64 = 4 * 1024 * 1024;
const INDEX: &str = "profiles.json";
const NEXT: &str = "profiles.next";
const LOCK: &str = "profiles.lock";
const IMPORT: &str = "pack-import.json";
const IMPORT_NEXT: &str = "pack-import.next";
const IMPORT_COMPLETE: &str = ".carbon-import-complete";
const MAX_IMPORT_BYTES: u64 = 4096;

fn ordinary(meta: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    !meta.is_symlink()
}

fn regular_file(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(m) if ordinary(&m) && m.is_file() => Ok(true),
        Ok(_) => {
            Err("Expected a regular profile file. Links and special files are refused.".into())
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err("Could not inspect the personal profile library.".into()),
    }
}

fn ordinary_directory(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if ordinary(&meta) && meta.is_dir() => Ok(true),
        Ok(_) => {
            Err("Expected a regular profile directory. Linked directories are refused.".into())
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err("Could not inspect the profile directory.".into()),
    }
}

pub(super) fn sync_directory(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    return File::open(path)?.sync_all();
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

pub(super) fn private_directory(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    let created = {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(path)
    };
    #[cfg(not(unix))]
    let created = fs::create_dir(path);
    match created {
        Ok(()) => sync_directory(path.parent().ok_or("Missing directory parent.")?)
            .map_err(|_| "Could not confirm the new profile directory.")?,
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err("Could not create the personal profile directory.".into()),
    }
    if !ordinary_directory(path)? {
        return Err("The profile directory disappeared during preparation.".into());
    }
    Ok(())
}

pub(super) fn open(root: &Path) -> Result<(File, Library), String> {
    // The native shell supplies this root. Player input never participates in path construction.
    if let Some(parent) = root.parent() {
        fs::create_dir_all(parent).map_err(|_| "Could not create the profile library parent.")?;
    }
    private_directory(root)?;
    let dir = root.join("personal");
    private_directory(&dir)?;
    let lock_path = dir.join(LOCK);
    regular_file(&lock_path)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|_| "Could not open the personal library lock.")?;
    lock.lock()
        .map_err(|_| "Could not lock the personal library.")?;

    let index = dir.join(INDEX);
    let marker = dir.join("initialized");
    let initialized = regular_file(&marker)?;
    let mut library = if regular_file(&index)? {
        let file =
            File::open(&index).map_err(|_| "Could not read the personal profile library.")?;
        let mut bytes = Vec::new();
        file.take(MAX_INDEX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Could not read the personal profile library.")?;
        if bytes.len() as u64 > MAX_INDEX_BYTES {
            return Err(
                "The personal profile library exceeds the supported size. Preserve the library."
                    .into(),
            );
        }
        let library: Library = serde_json::from_slice(&bytes).map_err(|_| {
            "Personal profile metadata is unreadable. Restore the library from a backup.".to_owned()
        })?;
        library.validate()?;
        library
    } else {
        if initialized {
            return Err("The personal profile index is missing. Restore it from a backup.".into());
        }
        for entry in fs::read_dir(&dir).map_err(|_| "Could not inspect the personal directory.")? {
            let name = entry
                .map_err(|_| "Could not inspect personal directory contents.")?
                .file_name();
            if name != LOCK && name != NEXT {
                return Err(
                    "Missing profile index beside existing data. Preserve the directory.".into(),
                );
            }
        }
        let fresh = Library {
            version: 1,
            profiles: vec![],
        };
        save(&dir, &fresh)?;
        fresh
    };
    if !initialized {
        File::create_new(&marker)
            .and_then(|file| file.sync_all())
            .and_then(|_| sync_directory(&dir))
            .map_err(|_| {
                "Could not finalize personal library initialization. Reload it to retry."
            })?;
    }
    recover_import(root, &dir, &mut library)?;
    Ok((lock, library))
}

pub(super) fn save(dir: &Path, library: &Library) -> Result<(), String> {
    let bytes = serde_json::to_vec(library).map_err(|_| "Could not encode personal profiles.")?;
    publish(dir, INDEX, NEXT, &bytes)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackImport {
    format: u32,
    profile: Profile,
}

impl PackImport {
    fn validate(&self) -> Result<(), String> {
        if self.format != 1
            || !self
                .profile
                .imported_pack
                .as_ref()
                .is_some_and(|pack| pack.name == self.profile.name && pack.pending_files <= 4096)
        {
            return Err("Invalid staged pack import record. Preserve the library.".into());
        }
        self.profile.validate()
    }
}

pub(super) fn import(
    root: &Path,
    library: &mut Library,
    profile: &Profile,
    populate: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let dir = root.join("personal");
    let instances = root.join("instances-v1");
    private_directory(&instances)?;
    let id = profile.id.to_string();
    let stage = instances.join(format!(".pack-{id}"));
    let destination = instances.join(&id);
    if library.profiles.iter().any(|old| old.id == profile.id)
        || ordinary_directory(&stage)?
        || ordinary_directory(&destination)?
    {
        return Err("Could not allocate a new instance directory.".into());
    }
    let journal = PackImport {
        format: 1,
        profile: profile.clone(),
    };
    journal.validate()?;
    let bytes =
        serde_json::to_vec(&journal).map_err(|_| "Could not encode the pack import record.")?;
    if bytes.len() as u64 > MAX_IMPORT_BYTES {
        return Err("Pack import metadata exceeds the supported size.".into());
    }
    // Check the prospective index before writing content that recovery must publish later.
    library.profiles.push(profile.clone());
    let size = serde_json::to_vec(library).map(|bytes| bytes.len() as u64);
    library.profiles.pop();
    if size.map_err(|_| "Could not encode the imported profile.")? > MAX_INDEX_BYTES {
        return Err("The personal library is too large to save this change.".into());
    }
    publish(&dir, IMPORT, IMPORT_NEXT, &bytes)?;

    let stage_import = || {
        private_directory(&stage)?;
        populate(&stage)?;
        File::create_new(stage.join(IMPORT_COMPLETE))
            .and_then(|mut file| file.write_all(id.as_bytes()).and_then(|_| file.sync_all()))
            .and_then(|_| sync_directory(&stage))
            .map_err(|_| "Could not finalize the staged pack content.")?;
        fs::rename(&stage, &destination)
            .map_err(|_| "Could not publish the imported instance files.")?;
        Ok::<_, String>(())
    };
    if let Err(error) = stage_import() {
        recover_import(root, &dir, library)?;
        return Err(error);
    }
    recover_import(root, &dir, library).map_err(|error| {
        format!("Pack files were copied. Reload the library to finish the import. {error}")
    })
}

fn recover_import(root: &Path, dir: &Path, library: &mut Library) -> Result<(), String> {
    let journal_path = dir.join(IMPORT);
    if !regular_file(&journal_path)? {
        return Ok(());
    }
    let mut bytes = Vec::new();
    File::open(&journal_path)
        .and_then(|f| f.take(MAX_IMPORT_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|_| "Could not read the interrupted pack import record.")?;
    if bytes.len() as u64 > MAX_IMPORT_BYTES {
        return Err("Pack import record exceeds its size limit. Preserve the library.".into());
    }
    let journal: PackImport = serde_json::from_slice(&bytes)
        .map_err(|_| "Pack import record is unreadable. Preserve the library.")?;
    journal.validate()?;
    let profile = journal.profile;
    let instances = root.join("instances-v1");
    let id = profile.id.to_string();
    let stage = instances.join(format!(".pack-{id}"));
    let destination = instances.join(&id);
    let indexed = library.profiles.iter().find(|item| item.id == profile.id);

    // Preserve ambiguous or unsafe state. Only the recorded staging name can be removed.
    let exists = ordinary_directory(&instances)?;
    let staged = exists && ordinary_directory(&stage)?;
    if exists && ordinary_directory(&destination)? {
        let complete = destination.join(IMPORT_COMPLETE);
        let mut identity = Vec::new();
        if staged
            || indexed.is_some_and(|existing| existing != &profile)
            || !regular_file(&complete)?
            || File::open(&complete)
                .and_then(|file| file.take(37).read_to_end(&mut identity))
                .is_err()
            || identity != id.as_bytes()
        {
            return Err("Interrupted pack import has inconsistent contents. Preserve the instance and library.".into());
        }
        sync_directory(&instances)
            .map_err(|_| "Could not sync the imported instance directory.")?;
        if indexed.is_none() {
            library.profiles.push(profile);
            save(dir, library)?;
        } else {
            // A previous save may have renamed the index just before an interrupted sync.
            // Establish its durability before discarding the record that can reconstruct it.
            sync_directory(dir).map_err(|_| "Could not sync the recovered profile index.")?;
        }
    } else {
        if indexed.is_some() {
            return Err("Imported profile is indexed but its instance directory is missing. Preserve the library.".into());
        }
        if staged {
            fs::remove_dir_all(stage)
                .map_err(|_| "Could not discard the interrupted pack stage.")?;
            sync_directory(&instances).map_err(|_| "Could not sync the discarded pack stage.")?;
        }
    }
    fs::remove_file(journal_path)
        .and_then(|_| sync_directory(dir))
        .map_err(|_| "Could not clear the pack import record. Reload the library to retry.".into())
}

type Choices = std::collections::BTreeMap<uuid::Uuid, crate::JavaChoice>;

pub(super) fn java_choices(dir: &Path) -> Result<Choices, String> {
    let path = dir.join("java.json");
    if !regular_file(&path)? {
        return Ok(Choices::new());
    }
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|f| f.take(MAX_INDEX_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|_| "Could not read the saved Java selections.")?;
    if bytes.len() as u64 > MAX_INDEX_BYTES {
        return Err("Saved Java selections exceed the size limit.".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| {
        "Saved Java selections are unreadable. Restore the library from a backup.".into()
    })
}

pub(super) fn save_java_choices(dir: &Path, choices: &Choices) -> Result<(), String> {
    let bytes = serde_json::to_vec(choices).map_err(|_| "Could not encode Java selections.")?;
    publish(dir, "java.json", "java.next", &bytes)
}

fn publish(dir: &Path, index_name: &str, next_name: &str, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() as u64 > MAX_INDEX_BYTES {
        return Err("The personal library is too large to save this change.".into());
    }
    let index = dir.join(index_name);
    regular_file(&index)?;
    let next = dir.join(next_name);
    if regular_file(&next)? {
        fs::remove_file(&next).map_err(|_| "Could not release the interrupted profile write.")?;
    }
    let mut file = File::create_new(&next).map_err(|_| "Could not stage personal profiles.")?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| {
            "Could not finish writing personal profiles. The previous index remains active."
        })?;
    drop(file);
    fs::rename(next, index)
        .map_err(|_| "Could not publish personal profiles. Reload before retrying.")?;
    sync_directory(dir)
        .map_err(|_| "The profile index was replaced but durability could not be confirmed. Reload before retrying.".into())
}
