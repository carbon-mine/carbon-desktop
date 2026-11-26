use super::{GameFilePlan, PreparedVersion};
use crate::{provider, valid_digest};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

const MAX_OBJECT: u64 = 256 * 1024 * 1024;
const MAX_TOTAL: u64 = 16 * 1024 * 1024 * 1024;
pub(super) const INDEX_LIMIT: u64 = 8 * 1024 * 1024;

#[derive(Deserialize)]
struct Asset {
    hash: String,
    size: u64,
}

fn unique_assets<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, Asset>, D::Error> {
    struct Entries;
    impl<'de> serde::de::Visitor<'de> for Entries {
        type Value = BTreeMap<String, Asset>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a map of unique asset names")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<Self::Value, A::Error> {
            let mut result = BTreeMap::new();
            while let Some((key, value)) = map.next_entry()? {
                if result.len() >= 32768 || result.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom(
                        "duplicate asset name or too many assets",
                    ));
                }
            }
            Ok(result)
        }
    }
    deserializer.deserialize_map(Entries)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct Object {
    pub sha1: String,
    pub size: u64,
    pub url: String,
}
#[derive(Deserialize)]
struct File {
    #[serde(flatten)]
    object: Object,
    path: String,
}
#[derive(Deserialize)]
struct Named {
    #[serde(flatten)]
    object: Object,
    id: String,
}
#[derive(Deserialize)]
struct Version {
    downloads: ClientDownload,
    #[serde(rename = "assetIndex")]
    index: Named,
    libraries: Vec<Library>,
    #[serde(default)]
    logging: BTreeMap<String, Log>,
}
#[derive(Deserialize)]
struct ClientDownload {
    client: Object,
}
#[derive(Deserialize)]
struct Log {
    file: Named,
}
#[derive(Deserialize)]
struct Library {
    downloads: Downloads,
    #[serde(default)]
    rules: Option<Vec<Rule>>,
    #[serde(default)]
    natives: BTreeMap<String, String>,
}
#[derive(Deserialize)]
struct Downloads {
    artifact: Option<File>,
    #[serde(default)]
    classifiers: BTreeMap<String, File>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rule {
    action: String,
    os: Option<Os>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Os {
    name: Option<String>,
    arch: Option<String>,
}

#[derive(Clone, Copy)]
pub(super) struct Platform {
    os: &'static str,
    arch: &'static str,
}
impl Platform {
    pub(super) fn current() -> Result<Self, String> {
        let os = std::env::consts::OS;
        let arch = std::env::consts::ARCH;
        if !matches!(os, "windows" | "linux") || !matches!(arch, "x86_64" | "aarch64" | "x86") {
            return Err("Game file preparation is unsupported on this platform.".into());
        }
        Ok(Self { os, arch })
    }
    #[cfg(test)]
    pub(super) const fn fixture() -> Self {
        Self {
            os: "linux",
            arch: "x86_64",
        }
    }

    pub(super) fn label(self) -> String {
        format!("{}-{}", self.os, self.arch)
    }
    fn allowed(self, rules: Option<&[Rule]>) -> Result<bool, String> {
        let Some(rules) = rules else { return Ok(true) };
        let mut allowed = false;
        for r in rules {
            let permit = match r.action.as_str() {
                "allow" => true,
                "disallow" => false,
                _ => return Err("Unsupported Mojang library rule action.".into()),
            };
            let matches = r.os.as_ref().is_none_or(|os| {
                os.name.as_deref().is_none_or(|n| n == self.os)
                    && os.arch.as_deref().is_none_or(|a| {
                        let arch = match a {
                            "amd64" | "x64" => "x86_64",
                            "arm64" => "aarch64",
                            "i386" | "i686" => "x86",
                            a => a,
                        };
                        arch == self.arch
                    })
            });
            if matches {
                allowed = permit;
            }
        }
        Ok(allowed)
    }
}

fn safe_path(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 512
        && s.split('/').all(|p| {
            let stem = p.split('.').next().unwrap_or("").to_ascii_uppercase();
            let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$")
                || ["COM", "LPT"].iter().any(|prefix| {
                    stem.strip_prefix(prefix)
                        .is_some_and(|n| n.len() == 1 && n.as_bytes()[0].is_ascii_digit())
                });
            !reserved
                && !p.is_empty()
                && p != "."
                && p != ".."
                && !p.ends_with('.')
                && p.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-+".contains(&c))
        })
}

fn check(o: &Object, url: &str) -> Result<(), String> {
    if !valid_digest(&o.sha1) || o.size > MAX_OBJECT || o.url != url {
        return Err(
            "Unsupported Mojang object origin, size or digest. File preparation stopped.".into(),
        );
    }
    Ok(())
}

pub(super) struct Plan {
    pub summary: GameFilePlan,
    pub objects: BTreeMap<String, Object>,
    pub paths: BTreeMap<String, String>,
    pub classpath: Vec<String>,
    pub native_archives: Vec<String>,
    pub client_sha1: String,
    pub asset_index_id: String,
    pub logging_path: Option<String>,
    pub metadata: Vec<u8>,
    pub index: Object,
    seen_paths: HashSet<String>,
    directories: HashSet<String>,
}
impl Plan {
    fn add(&mut self, path: String, object: Object) -> Result<(), String> {
        if self
            .objects
            .get(&object.sha1)
            .is_some_and(|old| old.size != object.size)
        {
            return Err("Conflicting sizes for the same Mojang object digest.".into());
        }
        if let Some(old) = self.paths.get(&path) {
            if old == &object.sha1 {
                return Ok(());
            }
            return Err("Conflicting logical file paths in the Mojang game plan.".into());
        }
        let folded = path.to_ascii_lowercase();
        for (separator, _) in folded.match_indices('/') {
            let prefix = &folded[..separator];
            if self.seen_paths.contains(prefix) {
                return Err("A game file path also names a directory.".into());
            }
            self.directories.insert(prefix.to_owned());
        }
        if self.directories.contains(&folded) || !self.seen_paths.insert(folded) {
            return Err("Case-colliding logical paths in game plan.".into());
        }
        self.paths.insert(path, object.sha1.clone());
        if !self.objects.contains_key(&object.sha1) {
            self.summary.total_bytes = self
                .summary
                .total_bytes
                .checked_add(object.size)
                .ok_or("Game plan is too large.")?;
            if self.summary.total_bytes > MAX_TOTAL || self.objects.len() >= 32768 {
                return Err("Game plan exceeds supported storage or file-count limits.".into());
            }
            self.objects.insert(object.sha1.clone(), object);
            self.summary.unique_files += 1;
        }
        Ok(())
    }

    fn library(&mut self, f: File, native_archive: bool) -> Result<(), String> {
        if !safe_path(&f.path) {
            return Err("Unsupported library path in Mojang metadata.".into());
        }
        check(
            &f.object,
            &format!("https://libraries.minecraft.net/{}", f.path),
        )?;
        let path = format!("libraries/{}", f.path);
        let list = if native_archive {
            &mut self.native_archives
        } else {
            &mut self.classpath
        };
        if !list.contains(&path) {
            list.push(path.clone());
        }
        if !self.paths.contains_key(&path) {
            self.summary.library_files += 1;
        }
        self.add(path, f.object)
    }

    pub(super) fn version(
        info: &PreparedVersion,
        raw: &[u8],
        platform: Platform,
    ) -> Result<Self, String> {
        let v: Version = serde_json::from_slice(raw).map_err(|_| {
            "Unsupported Mojang file metadata or platform rule. No game files were acquired."
        })?;
        let client = v.downloads.client;
        check(
            &client,
            &format!(
                "https://piston-data.mojang.com/v1/objects/{}/client.jar",
                client.sha1
            ),
        )?;
        if v.libraries.len() > 2048
            || !safe_path(&v.index.id)
            || v.index.id.contains('/')
            || v.index.object.size > INDEX_LIMIT
            || !valid_digest(&v.index.object.sha1)
            || !provider::allowed(
                &v.index.object.url,
                &format!("/v1/packages/{}/{}.json", v.index.object.sha1, v.index.id),
            )
        {
            return Err("Unsupported Mojang asset index or library count.".into());
        }
        let mut plan = Self {
            summary: GameFilePlan {
                profile_id: info.profile_id,
                version: info.version.clone(),
                platform: platform.label(),
                metadata_sha1: info.metadata_sha1.clone(),
                library_files: 0,
                asset_objects: 0,
                asset_names: 0,
                logging_files: 0,
                unique_files: 0,
                total_bytes: 0,
            },
            objects: BTreeMap::new(),
            paths: BTreeMap::new(),
            classpath: Vec::new(),
            native_archives: Vec::new(),
            client_sha1: client.sha1.clone(),
            asset_index_id: v.index.id.clone(),
            logging_path: None,
            metadata: raw.to_vec(),
            index: v.index.object.clone(),
            seen_paths: HashSet::new(),
            directories: HashSet::new(),
        };
        plan.add(format!("versions/{0}/{0}.jar", info.version), client)?;
        plan.add(
            format!("assets/indexes/{}.json", v.index.id),
            v.index.object,
        )?;
        if let Some(log) = v
            .logging
            .into_iter()
            .find(|(name, _)| name == "client")
            .map(|(_, v)| v.file)
        {
            if !safe_path(&log.id) || log.id.contains('/') {
                return Err("Unsupported logging configuration filename.".into());
            }
            check(
                &log.object,
                &format!(
                    "https://piston-data.mojang.com/v1/objects/{}/{}",
                    log.object.sha1, log.id
                ),
            )?;
            let path = format!("assets/log_configs/{}", log.id);
            plan.add(path.clone(), log.object)?;
            plan.logging_path = Some(path);
            plan.summary.logging_files = 1;
        }
        for lib in v.libraries {
            if !platform.allowed(lib.rules.as_deref())? {
                continue;
            }
            if let Some(file) = lib.downloads.artifact {
                plan.library(file, false)?;
            }
            if let Some(name) = lib.natives.get(platform.os) {
                let class =
                    name.replace("${arch}", if platform.arch == "x86" { "32" } else { "64" });
                let mut classifiers = lib.downloads.classifiers;
                let file = classifiers
                    .remove(&class)
                    .ok_or("Mojang metadata is missing a required native library classifier.")?;
                plan.library(file, true)?;
            }
        }
        Ok(plan)
    }

    pub(super) fn add_assets(&mut self, raw: &[u8]) -> Result<(), String> {
        #[derive(Deserialize)]
        struct Index {
            #[serde(deserialize_with = "unique_assets")]
            objects: BTreeMap<String, Asset>,
            #[serde(default)]
            r#virtual: bool,
            #[serde(default)]
            map_to_resources: bool,
        }
        let index: Index =
            serde_json::from_slice(raw).map_err(|_| "Invalid Mojang asset index.")?;
        if index.objects.len() > 32768 || index.r#virtual || index.map_to_resources {
            return Err("Unsupported legacy asset layout or asset count.".into());
        }
        self.summary.asset_names = index.objects.len();
        let mut known = HashSet::new();
        for (name, asset) in index.objects {
            // Asset names are lookup keys in the provider index, never host paths.
            if name.is_empty()
                || name.len() > 512
                || name.chars().any(char::is_control)
                || !valid_digest(&asset.hash)
                || asset.size > MAX_OBJECT
            {
                return Err("Unsupported asset descriptor in Mojang index.".into());
            }
            let hash = asset.hash;
            let key = format!("{}/{}", &hash[..2], hash);
            if known.insert(hash.clone()) {
                self.summary.asset_objects += 1;
            }
            let o = Object {
                sha1: hash,
                size: asset.size,
                url: format!("https://resources.download.minecraft.net/{key}"),
            };
            self.add(format!("assets/objects/{key}"), o)?;
        }
        Ok(())
    }
}
