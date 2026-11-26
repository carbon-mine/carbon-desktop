use super::{
    cache, ensure_selection, resolve, transfer, GameFileProgress, Object, Plan, Platform, Receipt,
    Store,
};
use carbon_profiles::Profile;
use serde::Serialize;
use sha1::{Digest, Sha1};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

const MAX_VIEW_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const MAX_VIEW_FILES: usize = 32_768;
const VIEW_BINDING: &str = ".carbon-view.json";
const CANCELLED: &str = "Private game preparation was cancelled. Retry to prepare this profile.";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceViewSummary {
    pub profile_id: Uuid,
    pub version: String,
    pub platform: String,
    pub metadata_sha1: String,
    pub files: usize,
    pub total_bytes: u64,
    pub copied_bytes: u64,
    pub reused_bytes: u64,
}

/// Host-resolved paths. Keep these in the trusted native process.
#[derive(Debug)]
pub struct InstancePaths {
    game_dir: PathBuf,
    content_dir: PathBuf,
    client_jar: PathBuf,
    class_path: Vec<PathBuf>,
    assets_dir: PathBuf,
    asset_index_id: String,
    logging_config: Option<PathBuf>,
    libraries_dir: PathBuf,
    natives_dir: PathBuf,
    version_metadata: PathBuf,
}

impl InstancePaths {
    pub fn game_dir(&self) -> &Path {
        &self.game_dir
    }

    pub fn content_dir(&self) -> &Path {
        &self.content_dir
    }

    pub fn client_jar(&self) -> &Path {
        &self.client_jar
    }

    pub fn class_path(&self) -> &[PathBuf] {
        &self.class_path
    }

    pub fn assets_dir(&self) -> &Path {
        &self.assets_dir
    }

    pub fn asset_index_id(&self) -> &str {
        &self.asset_index_id
    }

    pub fn logging_config(&self) -> Option<&Path> {
        self.logging_config.as_deref()
    }

    pub fn libraries_dir(&self) -> &Path {
        &self.libraries_dir
    }

    pub fn natives_dir(&self) -> &Path {
        &self.natives_dir
    }

    pub fn version_metadata(&self) -> &Path {
        &self.version_metadata
    }
}

/// Holds the profile's view lease. Keep it alive while consuming these paths.
#[derive(Debug)]
pub struct PreparedInstanceView {
    _lease: File,
    paths: InstancePaths,
    summary: InstanceViewSummary,
    root: PathBuf,
    plan: Plan,
}

impl PreparedInstanceView {
    pub fn paths(&self) -> &InstancePaths {
        &self.paths
    }

    pub fn summary(&self) -> &InstanceViewSummary {
        &self.summary
    }

    pub(super) fn plan(&self) -> &Plan {
        &self.plan
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    pub(super) fn verify_for_launch(&self) -> Result<(), String> {
        let s = &self.summary;
        let expected = Receipt::new(s.profile_id, &s.version, &s.metadata_sha1, &s.platform);
        let binding = serde_json::to_vec(&expected).map_err(|_| "Invalid view binding.")?;
        let active = self.root.join("game-files-v1").join("active");
        let versions = self.root.join("vanilla-metadata-v1").join("versions");
        for dir in [
            &self.root,
            &self.root.join("game-files-v1"),
            &self.root.join("vanilla-metadata-v1"),
            &self.root.join("instances-v1"),
            &active,
            &versions,
            self.paths
                .content_dir
                .parent()
                .ok_or("Invalid view directory.")?,
            &self.paths.game_dir,
        ] {
            if !ordinary_dir(dir)? {
                return Err("The verified game directories are unavailable.".into());
            }
        }
        let receipt = cache::read(&active.join(format!("{}.json", s.profile_id)), 4096)?;
        let selected = carbon_profiles::existing_profile(&self.root, s.profile_id)?
            .ok_or("The selected profile is unavailable.")?;
        let current_pin = installed_pin(&self.root, &selected)?;
        if receipt.and_then(|raw| serde_json::from_slice::<Receipt>(&raw).ok()) != Some(expected)
            || current_pin.is_none_or(|pin| pin.sha1 != s.metadata_sha1)
            || selected.version.id() != s.version
            || selected.imported_pack.is_some()
        {
            return Err("The selected game version or activation has changed.".into());
        }
        let layout = Layout::new(&self.plan, &binding)?;
        let mut progress = Observer {
            callback: |_| true,
            files: 0,
            bytes: 0,
            total_files: layout.files.len(),
            total_bytes: layout.total_bytes,
        };
        if !verified_content(&self.paths.content_dir, &layout, &mut progress)? {
            return Err("The leased private game files need verification or repair.".into());
        }
        Ok(())
    }
}

/// Offline operation. Rebuilds the plan from verified metadata and copies or rehashes the
/// private content. The caller passes the app-owned library root and previously reviewed pin.
pub fn prepare_instance_view(
    root: &Path,
    profile_id: Uuid,
    expected_metadata_sha1: &str,
    on_progress: impl FnMut(GameFileProgress) -> bool,
) -> Result<PreparedInstanceView, String> {
    if !crate::valid_digest(expected_metadata_sha1) {
        return Err("Invalid reviewed version pin. Review the profile setup again.".into());
    }
    let (store, plan) = resolve(root, profile_id, true, Platform::current()?)?;
    materialize(root, &store, &plan, expected_metadata_sha1, on_progress)
}

fn installed_pin(root: &Path, profile: &Profile) -> Result<Option<crate::Pin>, String> {
    let cache = root.join("vanilla-metadata-v1");
    let versions = cache.join("versions");
    if !ordinary_dir(&cache)? || !ordinary_dir(&versions)? {
        return Ok(None);
    }
    let id = profile.version.id();
    Ok(cache::read(&versions.join(format!("{id}.json")), 4096)?
        .and_then(|raw| serde_json::from_slice::<crate::Pin>(&raw).ok())
        .filter(|pin| pin.valid_for(id, crate::release_type(profile.version))))
}

/// Lease and verify the activated private view without repairing or acquiring files.
pub fn lease_installed_view(
    root: &Path,
    profile: &Profile,
) -> Result<PreparedInstanceView, String> {
    let platform = Platform::current()?;
    if profile.imported_pack.is_some() || Platform::launch_platform(&platform.label()).is_none() {
        return Err("This instance has no supported private vanilla view.".into());
    }
    let instances = root.join("instances-v1");
    let owner = instances.join(profile.id.to_string());
    let content = owner.join("content");
    for path in [root, &instances, &owner, &content] {
        if !ordinary_dir(path)? {
            return Err("Prepare this instance's private game files first.".into());
        }
    }
    let lock = owner.join("view.lock");
    if !cache::regular(&lock)? {
        return Err("The private game files have no preparation lease.".into());
    }
    let lease = File::open(lock).map_err(|_| "Could not read the private game lease.")?;
    lease
        .try_lock_shared()
        .map_err(|_| "Private game files are in use.")?;
    let pin = installed_pin(root, profile)?.ok_or("The installed version pin is unavailable.")?;
    let id = profile.version.id();
    let versions = content.join("versions");
    let version = versions.join(id);
    let assets = content.join("assets");
    let indexes = assets.join("indexes");
    for path in [&versions, &version, &assets, &indexes] {
        if !ordinary_dir(path)? {
            return Err("The installed version metadata is incomplete.".into());
        }
    }
    let metadata = cache::read(&version.join(format!("{id}.json")), crate::VERSION_LIMIT)?
        .ok_or("The installed version metadata is unavailable.")?;
    let info = crate::verified(&metadata, &pin, profile.id, crate::Origin::Cache)?;
    let mut plan = Plan::version(&info, &metadata, platform)?;
    supports_layout(&plan)?;
    let index = cache::read(
        &indexes.join(format!("{}.json", plan.asset_index_id)),
        plan.index.size,
    )?
    .filter(|raw| {
        raw.len() as u64 == plan.index.size && hex::encode(Sha1::digest(raw)) == plan.index.sha1
    })
    .ok_or("The installed asset index needs repair.")?;
    plan.add_assets(&index)?;
    let binding = Receipt::new(profile.id, id, &pin.sha1, &platform.label());
    let raw_binding = serde_json::to_vec(&binding).map_err(|_| "Invalid view binding.")?;
    let layout = Layout::new(&plan, &raw_binding)?;
    let held = held_view(
        root,
        &plan,
        &layout,
        lease,
        content,
        owner.join("game"),
        true,
    );
    held.verify_for_launch()?;
    Ok(held)
}

enum Source<'a> {
    Object(&'a Object),
    Bytes(&'a [u8]),
}

impl Source<'_> {
    fn size(&self) -> u64 {
        match self {
            Self::Object(object) => object.size,
            Self::Bytes(bytes) => bytes.len() as u64,
        }
    }
}

struct Layout<'a> {
    files: BTreeMap<String, Source<'a>>,
    dirs: BTreeSet<String>,
    total_bytes: u64,
}

impl<'a> Layout<'a> {
    fn new(plan: &'a Plan, binding: &'a [u8]) -> Result<Self, String> {
        let mut files: BTreeMap<_, _> = plan
            .paths
            .iter()
            .map(|(path, digest)| (path.clone(), Source::Object(&plan.objects[digest])))
            .collect();
        files.insert(metadata_path(plan), Source::Bytes(&plan.metadata));
        files.insert(VIEW_BINDING.into(), Source::Bytes(binding));
        let mut dirs = BTreeSet::from_iter(
            [
                "assets",
                "assets/indexes",
                "assets/objects",
                "libraries",
                "natives",
            ]
            .into_iter()
            .map(str::to_owned),
        );
        let mut total_bytes = 0;
        for (path, source) in &files {
            total_bytes += source.size();
            dirs.extend(path.match_indices('/').map(|(i, _)| path[..i].to_owned()));
        }
        if total_bytes > MAX_VIEW_BYTES || files.len() > MAX_VIEW_FILES {
            return Err(
                "Private vanilla content exceeds supported storage or file-count limits.".into(),
            );
        }
        Ok(Self {
            files,
            dirs,
            total_bytes,
        })
    }

    fn has_expected_entries(&self, base: &Path, prefix: &str) -> Result<bool, String> {
        let dir = base.join(prefix);
        for entry in fs::read_dir(dir).map_err(|_| "Could not inspect private game content.")? {
            let entry = entry.map_err(|_| "Could not inspect an entry in private game content.")?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                return Ok(false);
            };
            let relative = if prefix.is_empty() {
                name.to_owned()
            } else {
                format!("{prefix}/{name}")
            };
            let meta = fs::symlink_metadata(entry.path())
                .map_err(|_| "Could not inspect an entry in private game content.")?;
            if !cache::ordinary(&meta) {
                return Err("Private game content contains a link or reparse point. Inspect the app's storage.".into());
            }
            if meta.is_dir() && self.dirs.contains(&relative) {
                if !self.has_expected_entries(base, &relative)? {
                    return Ok(false);
                }
            } else if !meta.is_file() || !self.files.contains_key(&relative) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

fn metadata_path(plan: &Plan) -> String {
    format!("versions/{0}/{0}.json", plan.summary.version)
}

fn ordinary_dir(path: &Path) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() && cache::ordinary(&meta) => Ok(true),
        Ok(_) => Err("Expected an ordinary directory in private game storage. Links and special files are refused.".into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err("Could not inspect private game storage.".into()),
    }
}

fn remove_owned_dir(path: &Path) -> Result<(), String> {
    if ordinary_dir(path)? {
        fs::remove_dir_all(path).map_err(|_| {
            "Could not clear old private content. Check storage permissions and retry."
        })?;
    }
    Ok(())
}

fn regular_lock(path: &Path) -> Result<File, String> {
    cache::regular(path)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|_| "Could not open the private game preparation lock.")?;
    file.try_lock()
        .map_err(|_| "This profile's game directory is in use. Close its session and retry.")?;
    Ok(file)
}

struct Observer<F> {
    callback: F,
    files: usize,
    bytes: u64,
    total_files: usize,
    total_bytes: u64,
}

impl<F: FnMut(GameFileProgress) -> bool> Observer<F> {
    fn notify(&mut self, phase: &'static str) -> Result<(), String> {
        if (self.callback)(GameFileProgress {
            completed_files: self.files,
            total_files: self.total_files,
            verified_bytes: self.bytes,
            total_bytes: self.total_bytes,
            phase,
        }) {
            Ok(())
        } else {
            Err(CANCELLED.into())
        }
    }

    fn complete(&mut self, source: &Source<'_>, phase: &'static str) -> Result<(), String> {
        self.files += 1;
        self.bytes += source.size();
        self.notify(phase)
    }
}

fn verified_content(
    content: &Path,
    layout: &Layout<'_>,
    progress: &mut Observer<impl FnMut(GameFileProgress) -> bool>,
) -> Result<bool, String> {
    let phase = "verifyingView";
    progress.notify(phase)?;
    if !ordinary_dir(content)? || !layout.has_expected_entries(content, "")? {
        return Ok(false);
    }
    for relative in &layout.dirs {
        if !ordinary_dir(&content.join(relative))? {
            return Ok(false);
        }
    }
    for (relative, source) in &layout.files {
        let path = content.join(relative);
        if !cache::regular(&path)? {
            return Ok(false);
        }
        let valid = match source {
            Source::Object(object) => transfer::digest(
                File::open(path).map_err(|_| "Could not read a private game file.")?,
                &mut io::sink(),
                object,
                || progress.notify(phase),
            )?,
            Source::Bytes(raw) => cache::read(&path, raw.len() as u64)?.as_deref() == Some(raw),
        };
        if !valid {
            return Ok(false);
        }
        progress.complete(source, phase)?;
    }
    Ok(true)
}

fn build_content(
    staged: &Path,
    store: &Store,
    plan: &Plan,
    layout: &Layout<'_>,
    progress: &mut Observer<impl FnMut(GameFileProgress) -> bool>,
) -> Result<(), String> {
    let phase = "copyingView";
    progress.files = 0;
    progress.bytes = 0;
    progress.notify(phase)?;
    for relative in &layout.dirs {
        cache::directory(&staged.join(relative))?;
    }
    for (relative, source) in &layout.files {
        let mut destination = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(staged.join(relative))
            .map_err(|_| "Could not create a private game file. Check free disk space.")?;
        match source {
            Source::Object(object) => {
                let path = store.object(object, &plan.client_sha1)?;
                let error = || {
                    format!(
                    "Missing or damaged cached game object {}. Prepare game files online, then retry.",
                    object.sha1
                )
                };
                if !cache::regular(&path)? {
                    return Err(error());
                }
                let input = File::open(path).map_err(|_| error())?;
                if !transfer::digest(input, &mut destination, object, || progress.notify(phase))? {
                    return Err(error());
                }
            }
            Source::Bytes(raw) => destination
                .write_all(raw)
                .map_err(|_| "Could not write private version data. Check free disk space.")?,
        }
        destination
            .sync_all()
            .map_err(|_| "Could not sync private content. Check storage and retry.")?;
        progress.complete(source, phase)?;
    }
    for relative in layout.dirs.iter().rev() {
        cache::sync_dir(&staged.join(relative))?;
    }
    cache::sync_dir(staged)
}

fn recover(content: &Path, previous: &Path, staged: &Path) -> Result<(), String> {
    if ordinary_dir(previous)? {
        if ordinary_dir(content)? {
            remove_owned_dir(previous)?;
        } else {
            fs::rename(previous, content)
                .map_err(|_| "Could not restore interrupted private game content.")?;
        }
    }
    remove_owned_dir(staged)?;
    Ok(())
}

fn publish(content: &Path, previous: &Path, staged: &Path, owner: &Path) -> Result<(), String> {
    if ordinary_dir(content)? {
        fs::rename(content, previous)
            .map_err(|_| "Could not set aside the previous private game content.")?;
    }
    if fs::rename(staged, content).is_err() {
        recover(content, previous, staged)?;
        return Err("Could not publish private game content. Check storage and retry.".into());
    }
    cache::sync_dir(owner)?;
    remove_owned_dir(previous)
}

pub(super) fn supports_layout(plan: &Plan) -> Result<(), String> {
    if Platform::launch_platform(&plan.summary.platform).is_none() {
        return Err("Private vanilla game preparation supports Windows and Linux x86-64.".into());
    }
    if !plan.native_archives.is_empty() {
        return Err("This Minecraft version requires legacy native archive extraction. Private launch preparation is unavailable for this plan.".into());
    }
    Ok(())
}

/// Cheap setup status only. An explicit prepare fully verifies every byte again.
pub fn has_instance_view(root: &Path, profile_id: Uuid) -> Result<bool, String> {
    let Some((store, expected)) = super::current_receipt(root, profile_id)? else {
        return Ok(false);
    };
    if !store.has_receipt(&expected)? {
        return Ok(false);
    }
    let mut content = root.to_path_buf();
    for segment in ["instances-v1", &profile_id.to_string(), "content"] {
        content.push(segment);
        if !ordinary_dir(&content)? {
            return Ok(false);
        }
    }
    let Some(raw) = cache::read(&content.join(VIEW_BINDING), 4096)? else {
        return Ok(false);
    };
    if serde_json::from_slice::<Receipt>(&raw).ok().as_ref() != Some(&expected) {
        return Ok(false);
    }
    for segment in ["versions", &expected.version] {
        content.push(segment);
        if !ordinary_dir(&content)? {
            return Ok(false);
        }
    }
    let version_file = content.join(format!("{}.json", expected.version));
    Ok(
        cache::read(&version_file, crate::VERSION_LIMIT)?.is_some_and(|raw| {
            use sha1::{Digest, Sha1};
            hex::encode(Sha1::digest(raw)) == expected.metadata_sha1
        }),
    )
}

pub(super) fn materialize(
    root: &Path,
    store: &Store,
    plan: &Plan,
    expected_metadata_sha1: &str,
    on_progress: impl FnMut(GameFileProgress) -> bool,
) -> Result<PreparedInstanceView, String> {
    let s = &plan.summary;
    if s.metadata_sha1 != expected_metadata_sha1 {
        return Err(
            "The reviewed Minecraft version changed. Review the profile setup again.".into(),
        );
    }
    supports_layout(plan)?;
    let receipt = Receipt::new(s.profile_id, &s.version, &s.metadata_sha1, &s.platform);
    let binding = serde_json::to_vec(&receipt)
        .map_err(|_| "Could not encode the private game content binding.")?;
    let layout = Layout::new(plan, &binding)?;
    let _cache_guard = store.lock()?;
    if !store.has_receipt(&receipt)? {
        return Err(
            "Prepare or verify the reviewed game files before making a private game directory."
                .into(),
        );
    }
    ensure_selection(root, s)?;

    let instances = root.join("instances-v1");
    cache::directory(&instances)?;
    let owner = instances.join(s.profile_id.to_string());
    cache::directory(&owner)?;
    let lease = regular_lock(&owner.join("view.lock"))?;

    let content = owner.join("content");
    let staged = owner.join(".content-stage");
    let previous = owner.join(".content-previous");
    recover(&content, &previous, &staged)?;
    let game_dir = owner.join("game");
    cache::directory(&game_dir)?;
    let mut progress = Observer {
        callback: on_progress,
        files: 0,
        bytes: 0,
        total_files: layout.files.len(),
        total_bytes: layout.total_bytes,
    };
    let reused = verified_content(&content, &layout, &mut progress)?;
    if !reused {
        // The lock reserves these fixed sibling names. A stopped process can leave at most
        // one incomplete stage and one previous generation, which recover handles on retry.
        cache::directory(&staged)?;
        let prepared = build_content(&staged, store, plan, &layout, &mut progress)
            .and_then(|_| progress.notify("activatingView"))
            .and_then(|_| ensure_selection(root, s));
        if let Err(error) = prepared {
            remove_owned_dir(&staged)?;
            return Err(error);
        }
        publish(&content, &previous, &staged, &owner)?;
    } else {
        progress.notify("activatingView")?;
    }
    ensure_selection(root, s)?;

    Ok(held_view(
        root, plan, &layout, lease, content, game_dir, reused,
    ))
}

fn held_view(
    root: &Path,
    plan: &Plan,
    layout: &Layout<'_>,
    lease: File,
    content: PathBuf,
    game_dir: PathBuf,
    reused: bool,
) -> PreparedInstanceView {
    let s = &plan.summary;
    let client_jar = content.join(format!("versions/{0}/{0}.jar", s.version));
    let paths = InstancePaths {
        game_dir,
        class_path: plan
            .classpath
            .iter()
            .map(|p| content.join(p))
            .chain(std::iter::once(client_jar.clone()))
            .collect(),
        assets_dir: content.join("assets"),
        asset_index_id: plan.asset_index_id.clone(),
        logging_config: plan.logging_path.as_ref().map(|p| content.join(p)),
        libraries_dir: content.join("libraries"),
        natives_dir: content.join("natives"),
        version_metadata: content.join(metadata_path(plan)),
        client_jar,
        content_dir: content,
    };
    let summary = InstanceViewSummary {
        profile_id: s.profile_id,
        version: s.version.clone(),
        platform: s.platform.clone(),
        metadata_sha1: s.metadata_sha1.clone(),
        files: layout.files.len(),
        total_bytes: layout.total_bytes,
        copied_bytes: if reused { 0 } else { layout.total_bytes },
        reused_bytes: if reused { layout.total_bytes } else { 0 },
    };
    PreparedInstanceView {
        _lease: lease,
        paths,
        summary,
        root: root.to_owned(),
        plan: plan.clone(),
    }
}
