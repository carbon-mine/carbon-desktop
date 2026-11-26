// SPDX-License-Identifier: MPL-2.0
use super::{PlanError, Platform};
use carbon_profiles::Profile;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    path::{Path, PathBuf},
};

pub(super) const MAIN_CLASS: &str = "net.minecraft.client.main.Main";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdentitySlot {
    PlayerName,
    PlayerUuid,
    SessionAccess,
    ClientId,
    XboxUserId,
}

/// Identity slots are deliberately unresolved and carry no credential values.
#[derive(Eq, PartialEq)]
pub enum GameArgument {
    Value(OsString),
    /// Resolve through the supervisor's private state lease.
    GameDirectory,
    /// Resolve to the leased content's assets directory inside the game boundary.
    AssetsDirectory,
    Identity(IdentitySlot),
}

#[derive(Eq, PartialEq)]
pub enum JvmArgument {
    Value(OsString),
    /// Resolved only against a host-verified, immutable native-code generation.
    ReadOnlyNativeSearch {
        subdirectory: Option<&'static str>,
    },
    /// The manifest requests native materialization. No writable destination can satisfy
    /// this requirement at admission. A host-controlled preparer must qualify it first.
    UnresolvedNativeExtraction {
        property: &'static str,
        subdirectory: Option<&'static str>,
    },
}

pub(super) struct Material<'a> {
    pub asset_index: &'a str,
    pub classpath: &'a [PathBuf],
    pub log: Option<&'a Path>,
}

pub(super) struct Arguments {
    pub jvm: Vec<JvmArgument>,
    pub game: Vec<GameArgument>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    main_class: String,
    java_version: JavaVersion,
    arguments: Option<Modern>,
    minecraft_arguments: Option<serde::de::IgnoredAny>,
    inherits_from: Option<serde::de::IgnoredAny>,
    #[serde(default)]
    logging: Logging,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JavaVersion {
    major_version: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Modern {
    game: Vec<Conditional>,
    jvm: Vec<Conditional>,
    #[serde(rename = "default-user-jvm")]
    _launcher_preferences: Option<serde::de::IgnoredAny>,
}

#[derive(Deserialize, Default)]
struct Logging {
    client: Option<Log>,
}

#[derive(Deserialize)]
struct Log {
    argument: String,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Conditional {
    Literal(String),
    When(ConditionalGroup),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConditionalGroup {
    rules: Vec<Rule>,
    value: Values,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Values {
    One(String),
    Many(Vec<String>),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rule {
    action: String,
    os: Option<Os>,
    #[serde(default)]
    features: BTreeMap<String, bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Os {
    name: Option<String>,
    arch: Option<String>,
}

impl Rule {
    fn matches(&self, platform: Platform) -> Result<bool, PlanError> {
        let os_matches = self
            .os
            .as_ref()
            .is_none_or(|os| platform.matches_os(os.name.as_deref(), os.arch.as_deref()));
        let mut features_match = true;
        for (name, expected) in &self.features {
            let enabled = match name.as_str() {
                "has_custom_resolution" => true,
                "is_demo_user"
                | "has_quick_plays_support"
                | "is_quick_play_singleplayer"
                | "is_quick_play_multiplayer"
                | "is_quick_play_realms" => false,
                _ => return Err(PlanError::UnsupportedRule),
            };
            features_match &= enabled == *expected;
        }
        Ok(os_matches && features_match)
    }
}

fn select(arguments: &[Conditional], platform: Platform) -> Result<Vec<&str>, PlanError> {
    let mut selected = Vec::new();
    for arg in arguments {
        match arg {
            Conditional::Literal(value) => selected.push(value),
            Conditional::When(group) => {
                let mut enabled = false;
                if group.rules.is_empty() || group.rules.len() > 32 {
                    return Err(PlanError::UnsupportedRule);
                }
                for rule in &group.rules {
                    let allow = match rule.action.as_str() {
                        "allow" => true,
                        "disallow" => false,
                        _ => return Err(PlanError::UnsupportedRule),
                    };
                    if rule.matches(platform)? {
                        enabled = allow;
                    }
                }
                if enabled {
                    match &group.value {
                        Values::One(value) => selected.push(value),
                        Values::Many(values) => selected.extend(values),
                    }
                }
            }
        }
        if selected.len() > 128 {
            return Err(PlanError::MalformedManifest);
        }
    }
    Ok(selected.into_iter().map(String::as_str).collect())
}

fn value(s: impl Into<OsString>) -> JvmArgument {
    JvmArgument::Value(s.into())
}

fn path_property(prefix: &str, path: &Path) -> JvmArgument {
    let mut result = OsString::from(prefix);
    result.push(path);
    value(result)
}

fn join_classpath(paths: &[PathBuf], platform: Platform) -> Result<OsString, PlanError> {
    if paths.len() < 2 {
        return Err(PlanError::MissingMaterial);
    }
    let separator = if platform.windows() { ';' } else { ':' };
    let mut joined = OsString::new();
    for path in paths {
        let text = path
            .to_str()
            .filter(|text| {
                !text.is_empty() && !text.contains(separator) && !text.chars().any(char::is_control)
            })
            .ok_or(PlanError::MissingMaterial)?;
        if !joined.is_empty() {
            joined.push(separator.to_string());
        }
        joined.push(text);
    }
    Ok(joined)
}

fn native_property(token: &str) -> Option<JvmArgument> {
    let (property, template) = token.split_once('=')?;
    let (property, allowed_subdirectory) = match property {
        "-Djava.library.path" => ("-Djava.library.path", "java"),
        "-Djna.tmpdir" => ("-Djna.tmpdir", "jna"),
        "-Dorg.lwjgl.system.SharedLibraryExtractPath" => {
            ("-Dorg.lwjgl.system.SharedLibraryExtractPath", "lwjgl")
        }
        "-Dio.netty.native.workdir" => ("-Dio.netty.native.workdir", "netty"),
        _ => return None,
    };
    let suffix = template.strip_prefix("${natives_directory}")?;
    let subdirectory = if suffix.is_empty() {
        None
    } else if suffix.strip_prefix('/') == Some(allowed_subdirectory) {
        Some(allowed_subdirectory)
    } else {
        return None;
    };
    if property == "-Djava.library.path" {
        Some(JvmArgument::ReadOnlyNativeSearch { subdirectory })
    } else {
        Some(JvmArgument::UnresolvedNativeExtraction {
            property,
            subdirectory,
        })
    }
}

fn jvm_args(
    tokens: Vec<&str>,
    profile: &Profile,
    platform: Platform,
    material: &Material<'_>,
) -> Result<Vec<JvmArgument>, PlanError> {
    let mut result = vec![value(format!("-Xmx{}M", profile.memory_mib))];
    let mut tokens = tokens.into_iter();
    let mut classpath = false;
    let mut native_search = false;
    let mut unique = BTreeSet::new();
    while let Some(token) = tokens.next() {
        let key = token.split_once('=').map_or(token, |(key, _)| key);
        if !unique.insert(key) {
            return Err(PlanError::UnsupportedArgument);
        }
        match token {
            "-cp" => {
                if tokens.next() != Some("${classpath}") {
                    return Err(PlanError::UnsupportedArgument);
                }
                classpath = true;
                result.extend([value(token), value(join_classpath(material.classpath, platform)?)]);
            }
            "--add-exports" => {
                let accepted = "java.base/jdk.internal.misc=ALL-UNNAMED";
                if tokens.next() != Some(accepted) {
                    return Err(PlanError::UnsupportedArgument);
                }
                result.extend([value(token), value(accepted)]);
            }
            "-Dminecraft.launcher.brand=${launcher_name}" => result.push(value("-Dminecraft.launcher.brand=Carbon Desktop")),
            "-Dminecraft.launcher.version=${launcher_version}" => result.push(value(concat!("-Dminecraft.launcher.version=", env!("CARGO_PKG_VERSION")))),
            "-XX:StackShadowPages=32" | "--enable-native-access=ALL-UNNAMED" => result.push(value(token)),
            "-XX:HeapDumpPath=MojangTricksIntelDriversForPerformance_javaw.exe_minecraft.exe.heapdump"
                if platform.windows() => result.push(value(token)),
            _ => {
                let argument = native_property(token).ok_or(PlanError::UnsupportedArgument)?;
                native_search |= key == "-Djava.library.path";
                result.push(argument);
            }
        }
    }
    if !classpath || !native_search {
        return Err(PlanError::MissingMaterial);
    }
    Ok(result)
}

fn game_args(
    tokens: Vec<&str>,
    profile: &Profile,
    material: &Material<'_>,
    version_kind: &str,
) -> Result<Vec<GameArgument>, PlanError> {
    let value = |text: &str| GameArgument::Value(text.into());
    let identity = GameArgument::Identity;
    let (pairs, remainder) = tokens.as_chunks::<2>();
    let mut result = Vec::new();
    let mut flags = BTreeSet::new();
    for [flag, template] in pairs {
        if !flags.insert(*flag) {
            return Err(PlanError::UnsupportedArgument);
        }
        let expanded = match (*flag, *template) {
            ("--username", "${auth_player_name}") => identity(IdentitySlot::PlayerName),
            ("--uuid", "${auth_uuid}") => identity(IdentitySlot::PlayerUuid),
            ("--accessToken", "${auth_access_token}") => identity(IdentitySlot::SessionAccess),
            ("--clientId", "${clientid}") => identity(IdentitySlot::ClientId),
            ("--xuid", "${auth_xuid}") => identity(IdentitySlot::XboxUserId),
            ("--userType", "${user_type}") => value("msa"),
            ("--version", "${version_name}") => value(profile.version.id()),
            ("--versionType", "${version_type}") => value(version_kind),
            ("--gameDir", "${game_directory}") => GameArgument::GameDirectory,
            ("--assetsDir", "${assets_root}") => GameArgument::AssetsDirectory,
            ("--assetIndex", "${assets_index_name}") => value(material.asset_index),
            ("--width", "${resolution_width}") => value(&profile.window_width.to_string()),
            ("--height", "${resolution_height}") => value(&profile.window_height.to_string()),
            _ => return Err(PlanError::UnsupportedArgument),
        };
        result.extend([value(flag), expanded]);
    }
    if !remainder.is_empty() {
        return Err(PlanError::MalformedManifest);
    }
    if [
        "--username",
        "--uuid",
        "--accessToken",
        "--version",
        "--gameDir",
        "--assetsDir",
        "--assetIndex",
    ]
    .into_iter()
    .any(|flag| !flags.contains(flag))
    {
        return Err(PlanError::MissingMaterial);
    }
    Ok(result)
}

pub(super) fn build(
    raw: &[u8],
    platform: Platform,
    profile: &Profile,
    java_major: u16,
    material: Material<'_>,
) -> Result<Arguments, PlanError> {
    let manifest: Manifest =
        serde_json::from_slice(raw).map_err(|_| PlanError::MalformedManifest)?;
    if manifest.inherits_from.is_some() || manifest.minecraft_arguments.is_some() {
        return Err(PlanError::UnsupportedLegacy);
    }
    if manifest.id != profile.version.id() || manifest.kind != crate::release_type(profile.version)
    {
        return Err(PlanError::InvalidProfile);
    }
    if manifest.java_version.major_version != java_major {
        return Err(PlanError::IncompatibleRuntime);
    }
    if manifest.main_class != MAIN_CLASS {
        return Err(PlanError::UnexpectedMainClass);
    }
    let modern = manifest.arguments.ok_or(PlanError::MalformedManifest)?;
    let mut jvm = jvm_args(select(&modern.jvm, platform)?, profile, platform, &material)?;
    match (manifest.logging.client, material.log) {
        (Some(log), Some(path))
            if log.argument == "-Dlog4j.configurationFile=${path}" && log.kind == "log4j2-xml" =>
        {
            jvm.push(path_property("-Dlog4j.configurationFile=", path));
        }
        (None, None) => {}
        _ => return Err(PlanError::MissingMaterial),
    }
    let game = game_args(
        select(&modern.game, platform)?,
        profile,
        &material,
        &manifest.kind,
    )?;
    Ok(Arguments { jvm, game })
}
