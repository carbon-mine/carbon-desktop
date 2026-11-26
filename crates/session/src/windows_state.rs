// SPDX-License-Identifier: MPL-2.0
//! Windows x64 host-only durable player state. This module cannot start a process.
mod acl;

use acl::{Acl, Identity, Object};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions, TryLockError},
    io::{self, Error, ErrorKind, Read},
    os::windows::fs::OpenOptionsExt,
    path::{Component, Path, PathBuf, Prefix},
};
use uuid::{Uuid, Version};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
};

const STATE: &str = "protected-state-v1";
const CONTROL: &str = ".protected-state-host-v1";
const OWNER: &str = "owner.json";
const JOURNAL: &str = "run.json";
const OBJECT_LIMIT: usize = 1024;
const DEPTH_LIMIT: usize = 16;
const JOURNAL_LIMIT: u64 = 1024 * 1024;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    instance: Uuid,
    identity: Identity,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    instance: Uuid,
    state: Identity,
    run: Uuid,
    sid: Vec<u8>,
    // True only after every grant was observed and before the caller can launch.
    // Only trusted confirmation of drain may return it to false.
    armed: bool,
    targets: Vec<Target>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Target {
    path: Vec<String>,
    identity: Identity,
    directory: bool,
    before: [u8; 32],
    after: [u8; 32],
}

struct Entry {
    object: Object,
    acl: Acl,
}

/// A trusted supervisor keeps this lease until its OS process boundary drains.
/// Dropping the lease leaves its journal and the player directory in place.
pub struct StateLease {
    instance: Uuid,
    state_path: PathBuf,
    control_path: PathBuf,
    state: Object,
    _control: Object,
    _ancestors: Vec<Object>,
    _lock: File,
}

/// The launcher and supervisor must use this exact owned name when they create
/// the per-run AppContainer. This helper does not register or retire profiles.
pub fn profile_name(instance: Uuid, run: Uuid) -> String {
    let mut hash = Sha256::new();
    hash.update(b"CarbonDesktop.Game.v1");
    hash.update(instance.as_bytes());
    hash.update(run.as_bytes());
    let digest = hash.finalize();
    let label = Uuid::from_slice(&digest[..16]).expect("a 16-byte digest prefix");
    format!("CarbonDesktop.Game.{}", label.simple())
}

impl StateLease {
    /// Use the trusted app library root and an existing instance identity.
    /// This never follows aliases or adopts an unmarked player directory.
    pub fn acquire(root: &Path, instance: Uuid) -> io::Result<Self> {
        random(instance)?;
        if !matches!(root.components().next(), Some(Component::Prefix(prefix))
            if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)))
            || !root.is_absolute()
            || root
                .components()
                .any(|item| matches!(item, Component::ParentDir | Component::CurDir))
        {
            return Err(Error::other(
                "Protected state requires a local absolute library path.",
            ));
        }
        let instance_path = root.join("instances-v1").join(instance.to_string());
        let mut ancestors = Vec::new();
        let mut paths: Vec<_> = instance_path
            .ancestors()
            .filter(|path| path.has_root())
            .collect();
        paths.reverse();
        for path in paths {
            ancestors.push(directory(path, false)?);
        }
        acl::require_ntfs(&ancestors.last().expect("resolved instance directory").file)?;

        let control_path = instance_path.join(CONTROL);
        match fs::create_dir(&control_path) {
            Ok(()) => {}
            Err(err) if err.kind() == ErrorKind::AlreadyExists => {}
            Err(err) => return Err(err),
        }
        let control = directory(&control_path, false)?;
        Acl::read(&control.file)?.require_private_package_acl()?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(control_path.join("lease.lock"))?;
        let lock = acl::object(lock)?;
        if lock.directory {
            return Err(Error::other(
                "The protected state lease is not a regular file.",
            ));
        }
        lock.file.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => Error::new(
                ErrorKind::WouldBlock,
                "Another operation owns protected state for this instance.",
            ),
            TryLockError::Error(error) => error,
        })?;

        let state_path = instance_path.join(STATE);
        let owner_path = control_path.join(OWNER);
        let registered: Option<Owner> = read_json(&owner_path)?;
        if registered.is_none() {
            // An unexpected existing directory cannot be claimed even if no journal was found.
            acl::create_low_directory(&state_path)?;
        }
        let state = directory(&state_path, true)?;
        acl::require_low(&state.file)?;
        if let Some(owner) = registered {
            if owner.instance != instance || owner.identity != state.identity {
                return Err(Error::other(
                    "The protected state directory no longer has its registered identity.",
                ));
            }
        } else {
            write_json(
                &control_path,
                OWNER,
                &Owner {
                    instance,
                    identity: state.identity,
                },
            )?;
        }

        Ok(Self {
            instance,
            state_path,
            control_path,
            state,
            _control: control,
            _ancestors: ancestors,
            _lock: lock.file,
        })
    }

    /// The recorded run, if any. A released file lock alone never proves drain.
    pub fn unfinished_run(&self) -> io::Result<Option<Uuid>> {
        Ok(self.journal()?.map(|journal| journal.run))
    }

    /// Undo a crash before a grant was exposed, or after durable drain was recorded.
    pub fn recover_unarmed(&mut self) -> io::Result<()> {
        if let Some(journal) = self.journal()? {
            if journal.armed {
                return Err(Error::new(
                    ErrorKind::WouldBlock,
                    "The prior protected process boundary must be verified empty.",
                ));
            }
            self.retire(journal)?;
        }
        Ok(())
    }

    /// Journals and grants writable player-state access for one fresh run SID,
    /// then returns the scoped path. Never point classpaths or native search
    /// paths at this directory.
    pub fn prepare(&mut self, run: Uuid) -> io::Result<PathBuf> {
        random(run)?;
        self.recover_unarmed()?;
        let entries = self.tree()?;
        let mut journal = Journal {
            instance: self.instance,
            state: self.state.identity,
            run,
            sid: acl::run_sid(&profile_name(self.instance, run))?,
            armed: false,
            targets: Vec::new(),
        };
        let mut writes = Vec::new();
        for (path, entry) in entries {
            entry.acl.require_private_package_acl()?;
            let next = entry.acl.granted(&journal.sid, entry.object.directory)?;
            journal.targets.push(Target {
                path,
                identity: entry.object.identity,
                directory: entry.object.directory,
                before: entry.acl.digest(),
                after: next.digest(),
            });
            writes.push((entry.object.file, next));
        }
        write_json(&self.control_path, JOURNAL, &journal)?;
        // Add bottom-up so no object is rewritten while its parent already
        // contains inheritable grants for this run.
        for ((file, next), target) in writes.iter().zip(&journal.targets).rev() {
            if Acl::read(file)?.digest() != target.before {
                return Err(Error::other(
                    "Protected state permissions changed before preparation.",
                ));
            }
            next.write(file)?;
            if Acl::read(file)?.digest() != target.after {
                return Err(Error::other(
                    "Windows did not apply the exact protected state grant.",
                ));
            }
        }
        journal.armed = true;
        write_json(&self.control_path, JOURNAL, &journal)?;
        Ok(self.state_path.clone())
    }

    /// The supervisor calls this only after confirming that every member of the
    /// exact run's OS boundary has terminated. No drain proof is inferred here.
    pub fn retire_after_drain(&mut self, run: Uuid) -> io::Result<()> {
        let Some(mut journal) = self.journal()? else {
            return Ok(());
        };
        if journal.run != run {
            return Err(Error::other(
                "The protected state journal is for a different run.",
            ));
        }
        journal.armed = false;
        write_json(&self.control_path, JOURNAL, &journal)?;
        self.retire(journal)
    }

    fn journal(&self) -> io::Result<Option<Journal>> {
        let journal: Option<Journal> = read_json(&self.control_path.join(JOURNAL))?;
        if let Some(journal) = &journal {
            random(journal.run)?;
            if journal.instance != self.instance
                || journal.state != self.state.identity
                || journal.sid != acl::run_sid(&profile_name(self.instance, journal.run))?
                || journal.targets.len() > OBJECT_LIMIT
                || journal.targets.first().is_none_or(|target| {
                    !target.path.is_empty()
                        || target.identity != self.state.identity
                        || !target.directory
                })
            {
                return Err(Error::other(
                    "The protected state journal does not match this instance.",
                ));
            }
        }
        Ok(journal)
    }

    fn retire(&self, journal: Journal) -> io::Result<()> {
        let mut current = self.tree()?;
        let mut changes = Vec::new();
        for target in journal.targets {
            // Journal paths are compared against paths enumerated below the open root. They are
            // never joined back into an NTFS access path and cannot reach a foreign object.
            let entry = current
                .remove(&target.path)
                .ok_or_else(|| Error::other("A journaled state object is missing."))?;
            if target.identity != entry.object.identity
                || target.directory != entry.object.directory
            {
                return Err(Error::other("A journaled state object's identity changed."));
            }
            let observed = entry.acl.digest();
            if observed == target.before {
                continue;
            }
            let stripped = entry
                .acl
                .stripped(&journal.sid, entry.object.directory, false)?;
            if observed != target.after || stripped.digest() != target.before {
                return Err(Error::other(
                    "Protected state permissions changed. Review the conflict before retrying.",
                ));
            }
            changes.push((entry.object.file, observed, stripped));
        }
        // Objects created during the run can inherit its grant. Remove only the
        // exact inherited run-specific shapes. Never restore another ACL here.
        for entry in current.into_values() {
            let observed = entry.acl.digest();
            let stripped = entry
                .acl
                .stripped(&journal.sid, entry.object.directory, true)?;
            if stripped.digest() != observed {
                changes.push((entry.object.file, observed, stripped));
            }
        }
        for (file, observed, acl) in changes {
            if Acl::read(&file)?.digest() != observed {
                return Err(Error::other(
                    "Protected state permissions changed before recovery.",
                ));
            }
            acl.write(&file)?;
            if Acl::read(&file)?.digest() != acl.digest() {
                return Err(Error::other(
                    "Windows did not remove the exact protected state grant.",
                ));
            }
        }
        fs::remove_file(self.control_path.join(JOURNAL))
    }

    fn tree(&self) -> io::Result<BTreeMap<Vec<String>, Entry>> {
        let root = acl::object(self.state.file.try_clone()?)?;
        if root.identity != self.state.identity {
            return Err(Error::other("Protected state root identity changed."));
        }
        let mut tree = BTreeMap::new();
        let mut seen = BTreeSet::new();
        walk(&self.state_path, Vec::new(), root, &mut tree, &mut seen)?;
        Ok(tree)
    }
}

fn walk(
    path: &Path,
    relative: Vec<String>,
    object: Object,
    tree: &mut BTreeMap<Vec<String>, Entry>,
    seen: &mut BTreeSet<Identity>,
) -> io::Result<()> {
    if tree.len() >= OBJECT_LIMIT || relative.len() > DEPTH_LIMIT || !seen.insert(object.identity) {
        return Err(Error::other(
            "Protected state contains an alias or exceeds its inspection bounds.",
        ));
    }
    acl::require_low(&object.file)?;
    let acl = Acl::read(&object.file)?;
    let is_directory = object.directory;
    tree.insert(relative.clone(), Entry { object, acl });
    if is_directory {
        for item in fs::read_dir(path)? {
            let item = item?;
            let name = item
                .file_name()
                .into_string()
                .map_err(|_| Error::other("Protected state contains an unsupported filename."))?;
            let mut child = relative.clone();
            child.push(name);
            let path = item.path();
            let object = acl::open(&path, true)?;
            walk(&path, child, object, tree, seen)?;
        }
    }
    Ok(())
}

fn directory(path: &Path, write: bool) -> io::Result<Object> {
    let object = if write {
        acl::open(path, true)?
    } else {
        acl::pin(path)?
    };
    if !object.directory {
        return Err(Error::other(
            "Expected an app-owned protected state directory.",
        ));
    }
    Ok(object)
}

fn random(id: Uuid) -> io::Result<()> {
    if id.get_version() != Some(Version::Random) {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "Expected a random Carbon instance or run identity.",
        ));
    }
    Ok(())
}

fn read_json<T: DeserializeOwned>(path: &Path) -> io::Result<Option<T>> {
    let object = match acl::open(path, false) {
        Ok(object) => object,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    if object.directory {
        return Err(Error::other(
            "The protected state record is not a regular file.",
        ));
    }
    let mut bytes = Vec::new();
    object
        .file
        .take(JOURNAL_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > JOURNAL_LIMIT {
        return Err(Error::other(
            "The protected state record exceeds its bound.",
        ));
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(Error::other)
}

fn write_json<T: Serialize>(directory: &Path, name: &str, value: &T) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(Error::other)?;
    if bytes.len() as u64 > JOURNAL_LIMIT {
        return Err(Error::other(
            "The protected state record exceeds its bound.",
        ));
    }
    let mut temp = tempfile::NamedTempFile::new_in(directory)?;
    io::Write::write_all(temp.as_file_mut(), &bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(directory.join(name))
        .map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests;
