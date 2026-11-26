// SPDX-License-Identifier: MPL-2.0
use crate::{
    artifact::{Artifact, Format},
    storage, RuntimeError,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, File},
    io::{Cursor, Read, Write},
    path::Path,
};

const MAX_ENTRIES: usize = 4096;
const MAX_EXPANDED: u64 = 768 * 1024 * 1024;
const INVALID: RuntimeError = RuntimeError::Archive("The Java archive has an unsupported layout.");
const LIMIT: RuntimeError = RuntimeError::Archive("The Java archive exceeds the supported limits.");
const UNSAFE_PATH: RuntimeError =
    RuntimeError::Archive("The Java archive contains an unsafe or conflicting path.");
const UNSUPPORTED_ENTRY: RuntimeError =
    RuntimeError::Archive("The Java archive contains an unsupported file or link.");

type Checkpoint<'a> = dyn FnMut() -> Result<(), RuntimeError> + 'a;

#[derive(Clone)]
struct FileRecord {
    bytes: u64,
    executable: bool,
    hash: [u8; 32],
}

struct Node {
    path: String,
    file: Option<FileRecord>,
}

struct Tree<'a> {
    archive_root: String,
    home: &'a Path,
    extract: bool,
    entries: BTreeMap<String, Node>,
    explicit: HashSet<String>,
    links: Vec<(String, String)>,
    expanded: u64,
    check: &'a mut Checkpoint<'a>,
}

pub(crate) fn check_or_extract(
    spec: &Artifact,
    bytes: &[u8],
    home: &Path,
    extract: bool,
    check: &mut Checkpoint<'_>,
) -> Result<(), RuntimeError> {
    if !storage::check_root(home)? {
        return Err(RuntimeError::Integrity);
    }
    let mut tree = Tree {
        archive_root: format!("jdk-{}", spec.release),
        home,
        extract,
        entries: BTreeMap::new(),
        explicit: HashSet::new(),
        links: Vec::new(),
        expanded: 0,
        check,
    };
    match spec.format {
        Format::Zip => tree.zip(bytes)?,
        Format::TarGz => tree.tar(bytes)?,
    }
    tree.complete_links()?;
    tree.expect_file(spec.executable(), spec.format == Format::TarGz)?;
    tree.expect_file(spec.compiler(), spec.format == Format::TarGz)?;
    tree.expect_file("release", false)?;
    tree.expect_file("lib/modules", false)?;
    tree.expect_file("legal/java.base/LICENSE", false)?;
    tree.expect_file("legal/java.base/ASSEMBLY_EXCEPTION", false)?;
    let checked = tree.verify_dir(home, "")?;
    if checked != tree.entries.len() {
        return Err(RuntimeError::Integrity);
    }
    Ok(())
}

impl Tree<'_> {
    fn zip(&mut self, bytes: &[u8]) -> Result<(), RuntimeError> {
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| INVALID)?;
        if zip.len() > MAX_ENTRIES {
            return Err(LIMIT);
        }
        if zip.offset() != 0
            || zip.has_duplicate_names()
            || zip.has_overlapping_entries().map_err(|_| INVALID)?
        {
            return Err(INVALID);
        }
        for i in 0..zip.len() {
            (self.check)()?;
            let mut visit = zip.by_index(i).map_err(|_| INVALID)?;
            let raw = std::str::from_utf8(visit.name_raw()).map_err(|_| UNSAFE_PATH)?;
            let dir = visit.is_dir();
            let mode = visit.unix_mode().unwrap_or(0);
            let kind = mode & 0o170000;
            if visit.encrypted()
                || mode & 0o7000 != 0
                || !(kind == 0 || dir && kind == 0o040000 || !dir && kind == 0o100000)
            {
                return Err(UNSUPPORTED_ENTRY);
            }
            let path = self.register(raw)?;
            if dir && visit.size() == 0 {
                self.directory(&path)?;
            } else if dir || path.is_empty() {
                return Err(INVALID);
            } else {
                self.file(&path, visit.size(), mode & 0o111 != 0, &mut visit)?;
            }
        }
        Ok(())
    }

    fn tar(&mut self, bytes: &[u8]) -> Result<(), RuntimeError> {
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
        for item in archive.entries().map_err(|_| INVALID)? {
            (self.check)()?;
            let mut visit = item.map_err(|_| INVALID)?;
            let raw = visit.path_bytes();
            let path = self.register(std::str::from_utf8(&raw).map_err(|_| UNSAFE_PATH)?)?;
            let kind = visit.header().entry_type();
            if kind.is_dir() && visit.size() == 0 {
                self.directory(&path)?;
            } else if kind.is_file() && !path.is_empty() {
                let mode = visit.header().mode().map_err(|_| INVALID)?;
                if mode & 0o7000 != 0 {
                    return Err(UNSUPPORTED_ENTRY);
                }
                self.file(&path, visit.size(), mode & 0o111 != 0, &mut visit)?;
            } else if kind.is_symlink() && visit.size() == 0 {
                let link = visit.link_name_bytes().ok_or(UNSUPPORTED_ENTRY)?;
                let link = std::str::from_utf8(&link).map_err(|_| UNSUPPORTED_ENTRY)?;
                let target = legal_link(&path, link).ok_or(UNSUPPORTED_ENTRY)?;
                // Claim the path now so a later entry cannot descend through it.
                self.insert_file(&path, None)?;
                self.links.push((path, target));
            } else {
                return Err(UNSUPPORTED_ENTRY);
            }
        }
        Ok(())
    }

    fn register(&mut self, raw: &str) -> Result<String, RuntimeError> {
        if raw.len() > 240 || self.explicit.len() >= MAX_ENTRIES {
            return Err(LIMIT);
        }
        let raw = raw.strip_suffix('/').unwrap_or(raw);
        let path = if raw == self.archive_root {
            ""
        } else {
            raw.strip_prefix(self.archive_root.as_str())
                .and_then(|path| path.strip_prefix('/'))
                .filter(|path| path.split('/').all(safe_component))
                .ok_or(UNSAFE_PATH)?
        };
        if !self.explicit.insert(path.to_ascii_lowercase()) {
            return Err(UNSAFE_PATH);
        }
        Ok(path.to_owned())
    }

    fn directory(&mut self, path: &str) -> Result<(), RuntimeError> {
        if path.is_empty() {
            return Ok(());
        }
        self.claim_directory(path)?;
        if self.extract {
            fs::create_dir_all(self.home.join(path))?;
        }
        Ok(())
    }

    fn claim_directory(&mut self, path: &str) -> Result<(), RuntimeError> {
        let key = path.to_ascii_lowercase();
        if let Some(node) = self.entries.get(&key) {
            if node.path == path
                && node.file.is_none()
                && !self.links.iter().any(|(p, _)| p == path)
            {
                return Ok(());
            }
            return Err(UNSAFE_PATH);
        }
        if let Some((parent, _)) = path.rsplit_once('/') {
            self.claim_directory(parent)?;
        }
        if self.entries.len() >= MAX_ENTRIES {
            return Err(LIMIT);
        }
        self.entries.insert(
            key,
            Node {
                path: path.to_owned(),
                file: None,
            },
        );
        Ok(())
    }

    fn insert_file(&mut self, path: &str, record: Option<FileRecord>) -> Result<(), RuntimeError> {
        if let Some((parent, _)) = path.rsplit_once('/') {
            self.directory(parent)?;
        }
        if self.entries.len() >= MAX_ENTRIES {
            return Err(LIMIT);
        }
        let key = path.to_ascii_lowercase();
        if self.entries.contains_key(&key) {
            return Err(UNSAFE_PATH);
        }
        self.entries.insert(
            key,
            Node {
                path: path.to_owned(),
                file: record,
            },
        );
        Ok(())
    }

    fn file(
        &mut self,
        path: &str,
        size: u64,
        executable: bool,
        reader: &mut dyn Read,
    ) -> Result<(), RuntimeError> {
        self.account_bytes(size)?;
        self.insert_file(path, None)?;
        let file = self
            .extract
            .then(|| File::create_new(self.home.join(path)))
            .transpose()?;
        let hash = check_file(reader, size, file, executable, self.check)?;
        self.entries
            .get_mut(&path.to_ascii_lowercase())
            .ok_or(INVALID)?
            .file = Some(FileRecord {
            bytes: size,
            executable,
            hash,
        });
        Ok(())
    }

    fn account_bytes(&mut self, size: u64) -> Result<(), RuntimeError> {
        self.expanded = self
            .expanded
            .checked_add(size)
            .filter(|sum| *sum <= MAX_EXPANDED)
            .ok_or(LIMIT)?;
        Ok(())
    }

    fn complete_links(&mut self) -> Result<(), RuntimeError> {
        for (path, target) in std::mem::take(&mut self.links) {
            (self.check)()?;
            let record = self
                .entries
                .get(&target.to_ascii_lowercase())
                .filter(|node| node.path == target)
                .and_then(|node| node.file.clone())
                .filter(|record| !record.executable && record.bytes <= 256 * 1024)
                .ok_or(UNSUPPORTED_ENTRY)?;
            self.account_bytes(record.bytes)?;
            if self.extract {
                let mut original = storage::open_regular(&self.home.join(target), false)?;
                let file = File::create_new(self.home.join(&path))?;
                let hash = check_file(&mut original, record.bytes, Some(file), false, self.check)?;
                if hash != record.hash {
                    return Err(RuntimeError::Integrity);
                }
            }
            self.entries
                .get_mut(&path.to_ascii_lowercase())
                .ok_or(INVALID)?
                .file = Some(record);
        }
        Ok(())
    }

    fn expect_file(&self, path: &str, executable: bool) -> Result<(), RuntimeError> {
        self.entries
            .get(&path.to_ascii_lowercase())
            .filter(|node| node.path == path)
            .and_then(|node| node.file.as_ref())
            .filter(|file| file.bytes > 0 && (!executable || file.executable))
            .map(|_| ())
            .ok_or(INVALID)
    }

    fn verify_dir(&mut self, dir: &Path, prefix: &str) -> Result<usize, RuntimeError> {
        let mut checked = 0;
        for item in fs::read_dir(dir)? {
            (self.check)()?;
            let item = item?;
            let name = item.file_name();
            let name = name.to_str().ok_or(RuntimeError::Integrity)?;
            let rel = format!("{prefix}{name}");
            let expected = self
                .entries
                .get(&rel.to_ascii_lowercase())
                .filter(|node| node.path == rel)
                .ok_or(RuntimeError::Integrity)?;
            let path = item.path();
            let meta = fs::symlink_metadata(&path)?;
            if storage::is_link(&meta) {
                return Err(RuntimeError::Integrity);
            }
            checked += 1;
            if let Some(record) = expected.file.clone() {
                let mut file = storage::open_regular(&path, false)?;
                let meta = file.metadata()?;
                if meta.len() != record.bytes
                    || !matches_mode(&meta, record.executable)
                    || check_file(&mut file, record.bytes, None, false, self.check)? != record.hash
                {
                    return Err(RuntimeError::Integrity);
                }
            } else if meta.is_dir() {
                checked += self.verify_dir(&path, &format!("{rel}/"))?;
            } else {
                return Err(RuntimeError::Integrity);
            }
        }
        if self.extract {
            storage::sync_dir(dir)?;
        }
        Ok(checked)
    }
}

fn safe_component(part: &str) -> bool {
    if part.is_empty()
        || part.ends_with('.')
        || !part
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.+".contains(&c))
    {
        return false;
    }
    let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
    !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !["COM", "LPT"].iter().any(|prefix| {
            stem.strip_prefix(prefix)
                .is_some_and(|n| n.len() == 1 && n.as_bytes()[0].is_ascii_digit())
        })
}

fn legal_link(path: &str, target: &str) -> Option<String> {
    let mut parts = path.split('/');
    if parts.next()? != "legal" || parts.next()? == "java.base" {
        return None;
    }
    let name = parts.next()?;
    if parts.next().is_some()
        || !matches!(
            name,
            "LICENSE" | "ADDITIONAL_LICENSE_INFO" | "ASSEMBLY_EXCEPTION" | "cldr.md"
        )
        || target != format!("../java.base/{name}")
    {
        return None;
    }
    Some(format!("legal/java.base/{name}"))
}

fn check_file(
    reader: &mut dyn Read,
    size: u64,
    mut output: Option<File>,
    executable: bool,
    check: &mut Checkpoint<'_>,
) -> Result<[u8; 32], RuntimeError> {
    let mut hash = Sha256::new();
    let mut buf = [0; 64 * 1024];
    let mut left = size;
    loop {
        check()?;
        let step = (left + 1).min(buf.len() as u64) as usize;
        let count = reader.read(&mut buf[..step])?;
        if count == 0 {
            break;
        }
        left = left
            .checked_sub(count as u64)
            .ok_or(RuntimeError::Integrity)?;
        hash.update(&buf[..count]);
        if let Some(file) = &mut output {
            file.write_all(&buf[..count])?;
        }
    }
    if left != 0 {
        return Err(RuntimeError::Integrity);
    }
    if let Some(file) = output {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(private_mode(executable)))?;
        }
        #[cfg(not(unix))]
        let _ = executable;
        file.sync_all()?;
    }
    Ok(hash.finalize().into())
}

#[cfg(unix)]
fn private_mode(executable: bool) -> u32 {
    if executable {
        0o700
    } else {
        0o600
    }
}

fn matches_mode(_meta: &fs::Metadata, _executable: bool) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        _meta.permissions().mode() & 0o7777 == private_mode(_executable)
    }
    #[cfg(not(unix))]
    {
        true
    }
}
