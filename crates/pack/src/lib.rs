//! Review and import a bounded client-side subset of Modrinth pack format v1.
use carbon_profiles::{ImportedPack, Profile, VanillaVersion};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::OpenOptions,
    io::{Cursor, Read, Write},
    path::Path,
};
use zip::ZipArchive;

const MAX_ARCHIVE: u64 = 16 * 1024 * 1024;
const MAX_INDEX: u64 = 256 * 1024;
const MAX_ENTRIES: usize = 4096;
const INDEX: &str = "modrinth.index.json";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Index {
    format_version: u32,
    game: String,
    version_id: String,
    name: String,
    dependencies: BTreeMap<String, String>,
    files: Vec<Artifact>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Artifact {
    path: String,
    file_size: u64,
    hashes: BTreeMap<String, String>,
    downloads: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
}
#[derive(Debug)]
pub struct PackReview {
    pub import_id: uuid::Uuid,
    pub name: String,
    pub game_version: String,
    pub sha256: String,
    pub description: String,
    pub loader: Option<String>,
    pub pending_files: usize,
    pub overrides: usize,
}

struct Pack {
    review: PackReview,
    version: VanillaVersion,
    index: Vec<u8>,
    overrides: BTreeMap<String, usize>,
}

fn valid_path(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 240
        && name.is_ascii()
        && name.split('/').all(|part| {
            let stem = part
                .split('.')
                .next()
                .unwrap_or_default()
                .trim_end()
                .to_ascii_uppercase();
            let device = matches!(
                stem.as_str(),
                "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$" | "CONIN$" | "CONOUT$"
            ) || ["COM", "LPT"].iter().any(|prefix| {
                stem.strip_prefix(prefix)
                    .is_some_and(|n| n.len() == 1 && n.as_bytes()[0].is_ascii_digit())
            });
            !part.is_empty()
                && !part.ends_with(['.', ' '])
                && !device
                && part
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._+- ()[]".contains(&c))
        })
}

fn label(s: &str) -> bool {
    !s.trim().is_empty()
        && s.len() <= 128
        && !s.chars().any(|c| {
            c.is_control() || matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
}

fn read_file(path: &Path) -> Result<Vec<u8>, String> {
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("mrpack"))
    {
        return Err("Choose a .mrpack file.".into());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::{
            os::windows::fs::OpenOptionsExt,
            path::{Component, Prefix},
        };
        if !matches!(path.components().next(), Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)))
        {
            return Err("Choose a file on a local drive.".into());
        }
        options.custom_flags(0x00200000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let file = options
        .open(path)
        .map_err(|_| "Could not open the selected local pack.")?;
    let meta = file.metadata().map_err(|_| "Could not read pack size.")?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err("Choose a regular file rather than a link.".into());
        }
    }
    if !meta.is_file() || meta.len() > MAX_ARCHIVE {
        return Err("Import requires a regular file up to 16 MiB.".into());
    }
    let mut data = Vec::new();
    file.take(MAX_ARCHIVE + 1)
        .read_to_end(&mut data)
        .map_err(|_| "Could not read the selected pack.")?;
    Ok(data)
}

fn inspect(data: &[u8]) -> Result<Pack, String> {
    if data.len() as u64 > MAX_ARCHIVE {
        return Err("Pack exceeds the 16 MiB limit.".into());
    }
    let mut zip = ZipArchive::new(Cursor::new(data)).map_err(|_| "Invalid pack ZIP directory.")?;
    if zip.len() > MAX_ENTRIES
        || zip.has_duplicate_names()
        || zip.offset() != 0
        || zip
            .has_overlapping_entries()
            .map_err(|_| "Invalid archive entries.")?
    {
        return Err("Pack contains duplicate, overlapping or unsupported archive entries.".into());
    }
    let mut names = BTreeSet::new();
    let mut common = BTreeMap::new();
    let mut client = BTreeMap::new();
    for i in 0..zip.len() {
        let entry = zip
            .by_index_raw(i)
            .map_err(|_| "Could not inspect an archive entry.")?;
        let raw = std::str::from_utf8(entry.name_raw()).map_err(|_| "Unsupported archive name.")?;
        let name = raw.strip_suffix('/').unwrap_or(raw);
        let mode = entry.unix_mode().unwrap_or_default() & 0o170000;
        if !valid_path(name)
            || !names.insert(name.to_ascii_lowercase())
            || entry.encrypted()
            || !matches!(mode, 0 | 0o040000 | 0o100000)
        {
            return Err("Unsupported archive path, collision, link or encrypted entry.".into());
        }
        if entry.is_file() {
            let target = name
                .strip_prefix("client-overrides/")
                .map(|path| (path, &mut client))
                .or_else(|| {
                    name.strip_prefix("overrides/")
                        .map(|path| (path, &mut common))
                });
            if let Some((path, group)) = target {
                check_game_path(path)?;
                group.insert(path.to_owned(), i);
            }
        }
    }
    common.extend(client);
    let mut expanded = 0u64;
    let mut tree = BTreeMap::new();
    for (path, position) in &common {
        let entry = zip
            .by_index_raw(*position)
            .map_err(|_| "Could not inspect override.")?;
        expanded = expanded
            .checked_add(entry.size())
            .ok_or("Pack expansion exceeds limits.")?;
        if expanded > 64 * 1024 * 1024 {
            return Err("Client overrides exceed the 64 MiB unpacked limit.".into());
        }
        register_path(&mut tree, path)?;
    }
    let entry = zip
        .by_name(INDEX)
        .map_err(|_| "Missing or unreadable modrinth.index.json.")?;
    if entry.size() > MAX_INDEX {
        return Err("Pack index exceeds the 256 KiB limit.".into());
    }
    let mut index_data = Vec::new();
    entry
        .take(MAX_INDEX + 1)
        .read_to_end(&mut index_data)
        .map_err(|_| "Could not read the complete pack index.")?;
    if index_data.len() as u64 > MAX_INDEX {
        return Err("Pack index exceeds the 256 KiB limit.".into());
    }
    let index: Index =
        serde_json::from_slice(&index_data).map_err(|_| "Invalid Modrinth pack index fields.")?;
    if index.format_version != 1
        || index.game != "minecraft"
        || !label(&index.name)
        || !label(&index.version_id)
        || index.files.len() > MAX_ENTRIES
    {
        return Err("Unsupported pack version, game, name or file count.".into());
    }
    let game_version = index
        .dependencies
        .get("minecraft")
        .filter(|v| label(v))
        .ok_or("Missing Minecraft version.")?
        .clone();
    let mut paths = BTreeSet::new();
    let mut total = 0u64;
    let mut pending = 0;
    for f in &index.files {
        let hashes_ok = [("sha1", 40), ("sha512", 128)].iter().all(|(key, len)| {
            f.hashes
                .get(*key)
                .is_some_and(|h| h.len() == *len && h.bytes().all(|b| b.is_ascii_hexdigit()))
        });
        let urls_ok = !f.downloads.is_empty()
            && f.downloads.iter().all(|u| {
                url::Url::parse(u).is_ok_and(|v| {
                    v.scheme() == "https"
                        && v.host().is_some()
                        && v.username().is_empty()
                        && v.password().is_none()
                })
            });
        if !valid_path(&f.path)
            || !paths.insert(f.path.to_ascii_lowercase())
            || !hashes_ok
            || !urls_ok
        {
            return Err("Invalid or duplicate file descriptor in the pack index.".into());
        }
        if !f.env.iter().all(|(k, v)| {
            matches!(k.as_str(), "client" | "server")
                && matches!(v.as_str(), "required" | "optional" | "unsupported")
        }) {
            return Err("Unknown pack environment requirement.".into());
        }
        if f.env.get("client").map(String::as_str) == Some("unsupported") {
            continue;
        }
        check_game_path(&f.path)?;
        if !common.contains_key(&f.path) {
            register_path(&mut tree, &f.path)?;
            pending += 1;
        }
        total = total
            .checked_add(f.file_size)
            .ok_or("Declared download size is too large.")?;
        if total > (1u64 << 40) {
            return Err("Declared payload is outside the pack limits.".into());
        }
    }
    let version = VanillaVersion::ALL
        .into_iter()
        .find(|v| v.id() == game_version)
        .ok_or_else(|| {
            format!(
                "Minecraft {game_version} is outside this launcher's supported version catalogue."
            )
        })?;
    if index.name.trim() != index.name || index.name.chars().count() > 80 {
        return Err("The pack name must fit an 80-character instance name.".into());
    }
    let mut loader = None;
    for (kind, value) in &index.dependencies {
        if kind == "minecraft" {
            continue;
        }
        if !matches!(
            kind.as_str(),
            "forge" | "neoforge" | "fabric-loader" | "quilt-loader"
        ) || !label(value)
            || loader.is_some()
        {
            return Err("Unsupported or ambiguous pack loader requirement.".into());
        }
        loader = Some(format!("{kind} {value}"));
    }
    let overrides = common.len();
    let mut description = format!(
        "{name}\nMinecraft {game_version} · Pack {pack_version}\nLoader: {loader_name}\n\
         Client overrides to copy: {overrides}\nListed client files awaiting acquisition: {pending}",
        name = index.name,
        pack_version = index.version_id,
        loader_name = loader.as_deref().unwrap_or("Vanilla"),
    );
    if loader.is_some() {
        description.push_str("\nLoader installation is pending.");
    }
    Ok(Pack {
        review: PackReview {
            import_id: uuid::Uuid::new_v4(),
            name: index.name,
            game_version,
            sha256: hex::encode(Sha256::digest(data)),
            description,
            loader,
            pending_files: pending,
            overrides,
        },
        version,
        index: index_data,
        overrides: common,
    })
}

// Support only game-owned content directories. Personal worlds, launcher metadata and
// launch configuration stay outside the archive's write scope.
fn check_game_path(path: &str) -> Result<(), String> {
    if path.split_once('/').is_some_and(|(dir, _)| {
        matches!(
            dir,
            "config" | "mods" | "resourcepacks" | "shaderpacks" | "defaultconfigs" | "kubejs"
        )
    }) {
        Ok(())
    } else {
        Err(format!("Unsupported client content path: {path}"))
    }
}

fn register_path(paths: &mut BTreeMap<String, (String, bool)>, path: &str) -> Result<(), String> {
    let mut prefix = String::new();
    for part in path.split('/') {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(part);
        let entry = (prefix.clone(), prefix == path);
        if let Some(old) = paths.insert(prefix.to_ascii_lowercase(), entry.clone()) {
            if old != entry || entry.1 {
                return Err("Client paths collide as files, directories or case aliases.".into());
            }
        }
    }
    Ok(())
}

pub fn review_file(path: &Path) -> Result<PackReview, String> {
    Ok(inspect(&read_file(path)?)?.review)
}

pub fn import_reviewed(
    root: &Path,
    source: &Path,
    expected_sha256: &str,
    import_id: uuid::Uuid,
) -> Result<Profile, String> {
    let bytes = read_file(source)?;
    let pack = inspect(&bytes)?;
    if pack.review.sha256 != expected_sha256 {
        return Err("The selected pack changed after review. Choose the file again.".into());
    }
    let info = ImportedPack {
        name: pack.review.name,
        loader: pack.review.loader,
        pending_files: pack.review.pending_files,
    };
    carbon_profiles::import_instance(root, import_id, pack.version, info, |stage| {
        let write_error = |_| "Could not write the imported pack. Check free disk space.";
        let mut index = std::fs::File::create_new(stage.join(INDEX)).map_err(write_error)?;
        index
            .write_all(&pack.index)
            .and_then(|_| index.sync_all())
            .map_err(write_error)?;
        let mut receipt =
            std::fs::File::create_new(stage.join("pack.sha256")).map_err(write_error)?;
        writeln!(receipt, "{}", pack.review.sha256)
            .and_then(|_| receipt.sync_all())
            .map_err(write_error)?;
        let game = stage.join("game");
        std::fs::create_dir(&game).map_err(write_error)?;
        let mut zip =
            ZipArchive::new(Cursor::new(bytes)).map_err(|_| "Could not reopen pack contents.")?;
        #[cfg(unix)]
        let mut directories = BTreeSet::from([game.clone()]);
        for (path, position) in pack.overrides {
            let destination = game.join(path);
            std::fs::create_dir_all(destination.parent().ok_or("Invalid override path.")?)
                .map_err(write_error)?;
            #[cfg(unix)]
            directories.extend(
                destination
                    .ancestors()
                    .skip(1)
                    .take_while(|dir| dir.starts_with(&game))
                    .map(Path::to_path_buf),
            );
            let mut input = zip
                .by_index(position)
                .map_err(|_| "Could not open a client override.")?;
            let size = input.size();
            let mut output = std::fs::File::create_new(destination).map_err(write_error)?;
            let copied =
                std::io::copy(&mut input.by_ref().take(size + 1), &mut output).map_err(|_| {
                    "Client override failed its integrity check or could not be written."
                })?;
            if copied != size {
                return Err("Incomplete client override.".into());
            }
            output.sync_all().map_err(write_error)?;
        }
        #[cfg(unix)]
        for dir in directories.iter().rev() {
            std::fs::File::open(dir)
                .and_then(|dir| dir.sync_all())
                .map_err(write_error)?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::{write::SimpleFileOptions, ZipWriter};

    fn archive(index: &str, extra: Option<&str>) -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file(INDEX, SimpleFileOptions::default()).unwrap();
        zip.write_all(index.as_bytes()).unwrap();
        if let Some(name) = extra {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }
    const SAMPLE: &str = include_str!("../../../fixtures/pack-index.json");
    #[test]
    fn review_reports_client_requirements() {
        let p = inspect(&archive(SAMPLE, Some("overrides/config/forest.txt"))).unwrap();
        assert_eq!((p.review.pending_files, p.review.overrides), (1, 1));
    }

    #[test]
    fn review_counts_client_entries_and_bundled_overrides() {
        let mut index: serde_json::Value = serde_json::from_str(SAMPLE).unwrap();
        let files = ["required", "optional", "unsupported"].map(|requirement| {
            let mut entry = index["files"][0].clone();
            entry["path"] = format!("mods/{requirement}.txt").into();
            entry["env"] = serde_json::json!({ "client": requirement, "server": "required" });
            entry
        });
        index["files"] = files.to_vec().into();
        let manifest = index.to_string();
        let plain = inspect(&archive(&manifest, None)).unwrap().review;
        assert_eq!((plain.pending_files, plain.overrides), (2, 0));
        for (file, pending) in [("required", 1), ("optional", 1), ("unsupported", 2)] {
            let path = format!("overrides/mods/{file}.txt");
            let bundled = inspect(&archive(&manifest, Some(&path))).unwrap().review;
            assert_eq!(
                (bundled.pending_files, bundled.overrides),
                (pending, 1),
                "{file}"
            );
        }
    }
    fn client_pack() -> Vec<u8> {
        let mut index: serde_json::Value = serde_json::from_str(SAMPLE).unwrap();
        index["dependencies"]["fabric-loader"] = "0.16.10".into();
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in [
            (INDEX, index.to_string().into_bytes()),
            ("overrides/config/profile.txt", b"shared".to_vec()),
            ("client-overrides/config/profile.txt", b"client".to_vec()),
            ("server-overrides/config/private.txt", b"server".to_vec()),
        ] {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(&bytes).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn imports_reviewed_client_contents_into_independent_persistent_profiles() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("library");
        let source = temp.path().join("fixture.mrpack");
        std::fs::write(&source, client_pack()).unwrap();
        let review = review_file(&source).unwrap();
        assert_eq!(review.overrides, 1);
        assert_eq!(review.pending_files, 1);
        assert_eq!(review.loader.as_deref(), Some("fabric-loader 0.16.10"));
        assert!(review.description.contains('\n'));
        let import = || import_reviewed(&root, &source, &review.sha256, review.import_id).unwrap();
        let a = import();
        let dir = |id: &Profile| root.join("instances-v1").join(id.id.to_string());
        let game_a = dir(&a).join("game");
        assert_eq!(
            std::fs::read(game_a.join("config/profile.txt")).unwrap(),
            b"client"
        );
        assert!(!game_a.join("config/private.txt").exists());
        std::fs::write(game_a.join("config/profile.txt"), b"personal").unwrap();
        let retry = import();
        assert_eq!(a.id, retry.id);
        assert_eq!(carbon_profiles::list(&root).unwrap().len(), 1);
        let second_review = review_file(&source).unwrap();
        let b = import_reviewed(
            &root,
            &source,
            &second_review.sha256,
            second_review.import_id,
        )
        .unwrap();
        assert_ne!(a.id, b.id);
        assert_eq!(
            std::fs::read(dir(&b).join("game/config/profile.txt")).unwrap(),
            b"client"
        );
        assert_eq!(
            std::fs::read(game_a.join("config/profile.txt")).unwrap(),
            b"personal"
        );
        assert_eq!(
            std::fs::read_to_string(dir(&a).join("pack.sha256"))
                .unwrap()
                .trim(),
            review.sha256
        );
        assert_eq!(carbon_profiles::list(&root).unwrap(), vec![a, b]);
    }

    #[test]
    fn changed_archives_or_personal_paths_never_publish_an_instance() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("library");
        let source = temp.path().join("fixture.mrpack");
        std::fs::write(&source, client_pack()).unwrap();
        let review = review_file(&source).unwrap();
        std::fs::write(&source, archive(SAMPLE, None)).unwrap();
        assert!(
            import_reviewed(&root, &source, &review.sha256, review.import_id)
                .unwrap_err()
                .contains("changed")
        );
        for name in [
            "overrides/../config/x",
            "client-overrides/saves/world/level.dat",
            "overrides/options.txt",
        ] {
            assert!(inspect(&archive(SAMPLE, Some(name))).is_err());
        }
        assert!(carbon_profiles::list(&root).unwrap().is_empty());
    }

    #[test]
    fn unsupported_or_ambiguous_packs_fail_before_preview() {
        for index in [
            SAMPLE.replace("mods/forest.txt", "../private"),
            SAMPLE.replace("https://", "file://"),
            SAMPLE.replace("\"formatVersion\": 1", "\"formatVersion\": 2"),
            "{}".to_owned(),
        ] {
            assert!(inspect(&archive(&index, None)).is_err());
        }
        for path in [
            "config/con.txt",
            "../saves",
            "C:/config",
            ".",
            "a/",
            "/z",
            "a//x",
            "x:y",
            "x\\y",
        ] {
            assert!(!valid_path(path));
        }
        assert!(inspect(&archive(SAMPLE, Some("../outside"))).is_err());
        assert!(inspect(&archive(&" ".repeat(MAX_INDEX as usize + 1), None)).is_err());
    }
}
