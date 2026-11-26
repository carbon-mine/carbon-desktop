//! Personal vanilla launch preferences. Profile display names never become paths.
mod folder;
pub mod icons;
mod server;
mod storage;
mod targets;
pub use folder::open_game_folder;
pub use server::{
    server_profile, set_server_address, ServerAddress, ServerBindingUpdate, ServerJoinReadiness,
};
pub use targets::{
    change_instance_targets, instance_targets, validate_target_label, InstanceTargets, LocalWorld,
    SavedDestination, TargetAction, TargetKey,
};
#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
use uuid::{Uuid, Version};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum VanillaVersion {
    #[serde(rename = "26.3")]
    Release26_3,
    #[serde(rename = "1.21.1")]
    Release1_21_1,
    #[serde(rename = "1.20.4")]
    Release1_20_4,
    #[serde(rename = "1.20.1")]
    Release1_20_1,
    #[serde(rename = "24w14a")]
    Snapshot24w14a,
}

impl VanillaVersion {
    pub const ALL: [Self; 5] = [
        Self::Release26_3,
        Self::Release1_21_1,
        Self::Release1_20_4,
        Self::Release1_20_1,
        Self::Snapshot24w14a,
    ];
    pub fn id(self) -> &'static str {
        match self {
            Self::Release26_3 => "26.3",
            Self::Release1_21_1 => "1.21.1",
            Self::Release1_20_4 => "1.20.4",
            Self::Release1_20_1 => "1.20.1",
            Self::Snapshot24w14a => "24w14a",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportedPack {
    pub name: String,
    pub loader: Option<String>,
    pub pending_files: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Profile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported_pack: Option<ImportedPack>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_address: Option<ServerAddress>,
    pub id: Uuid,
    pub name: String,
    pub version: VanillaVersion,
    #[serde(rename = "memoryMiB")]
    pub memory_mib: u32,
    pub window_width: u32,
    pub window_height: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ProfileAction {
    Create {
        name: String,
        version: VanillaVersion,
        #[serde(rename = "memoryMiB")]
        memory_mib: u32,
        window_width: u32,
        window_height: u32,
    },
    Edit(Profile),
    Clone {
        id: Uuid,
        name: String,
    },
}

fn normalized_name(name: &str) -> Result<&str, String> {
    if name.chars().any(|c| c.is_control() || matches!(c,
        '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
    )) {
        return Err("Choose a profile name without control or directional formatting characters.".into());
    }
    let name = name.trim();
    if !(1..=80).contains(&name.chars().count()) {
        return Err("Use a profile name of 1 to 80 characters.".into());
    }
    Ok(name)
}

impl Profile {
    pub fn normalize_input(&mut self) -> Result<(), String> {
        self.name = normalized_name(&self.name)?.to_owned();
        self.validate()
    }

    fn validate(&self) -> Result<(), String> {
        if self.id.get_version() != Some(Version::Random)
            || normalized_name(&self.name)? != self.name
        {
            return Err(
                "Invalid saved profile identity or name. Preserve the profile library.".into(),
            );
        }
        if !(512..=16384).contains(&self.memory_mib) {
            return Err("Choose 512 to 16384 MiB of memory.".into());
        }
        if !(320..=7680).contains(&self.window_width) || !(240..=4320).contains(&self.window_height)
        {
            return Err("Choose a window width of 320 to 7680 and height of 240 to 4320.".into());
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Library {
    version: u32,
    profiles: Vec<Profile>,
}

impl Library {
    fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err(
                "The personal profile library version is unsupported. Preserve the library.".into(),
            );
        }
        let mut seen = HashSet::new();
        for profile in &self.profiles {
            profile.validate()?;
            if !seen.insert(profile.id) {
                return Err(
                    "The personal library has duplicate profile IDs. Preserve the library.".into(),
                );
            }
        }
        Ok(())
    }

    fn position(&self, id: Uuid) -> Result<usize, String> {
        self.profiles
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| {
                "This personal profile is unavailable. Reload the profile library.".into()
            })
    }

    fn change(&mut self, action: ProfileAction) -> Result<(), String> {
        let mut profile = match action {
            ProfileAction::Create {
                name,
                version,
                memory_mib,
                window_width,
                window_height,
            } => Profile {
                icon_id: None,
                imported_pack: None,
                server_address: None,
                id: Uuid::new_v4(),
                name,
                version,
                memory_mib,
                window_width,
                window_height,
            },
            ProfileAction::Clone { id, .. }
                if self.profiles[self.position(id)?].imported_pack.is_some() =>
            {
                return Err("Import the same pack again to make an independent copy of its original contents.".into());
            }
            ProfileAction::Clone { id, name } => Profile {
                id: Uuid::new_v4(),
                name,
                server_address: None,
                ..self.profiles[self.position(id)?].clone()
            },
            ProfileAction::Edit(mut edited) => {
                let index = self.position(edited.id)?;
                let original = &self.profiles[index];
                if original.imported_pack.is_some() && edited.version != original.version {
                    return Err(
                        "The pack defines its Minecraft version. Import another pack to change it."
                            .into(),
                    );
                }
                edited.icon_id.clone_from(&original.icon_id);
                edited.imported_pack.clone_from(&original.imported_pack);
                edited.server_address.clone_from(&original.server_address);
                edited.normalize_input()?;
                self.profiles[index] = edited;
                return Ok(());
            }
        };
        profile.normalize_input()?;
        if self.profiles.iter().any(|p| p.id == profile.id) {
            return Err("Could not allocate a unique profile ID. Retry.".into());
        }
        self.profiles.push(profile);
        Ok(())
    }
}

pub fn list(root: &Path) -> Result<Vec<Profile>, String> {
    let (_lock, library) = storage::open(root)?;
    Ok(library.profiles)
}

/// Read an initialized profile snapshot while leaving preparation and recovery to setup.
pub fn existing_profile(root: &Path, id: Uuid) -> Result<Option<Profile>, String> {
    Ok(storage::existing(root)?.and_then(|library| {
        library
            .profiles
            .into_iter()
            .find(|profile| profile.id == id)
    }))
}

/// Populate only an empty review library. The comparison sample is identical across frontends.
pub fn seed_review_library(root: &Path, comparison: bool) -> Result<(), String> {
    let (_lock, mut library) = storage::open(root)?;
    if !library.profiles.is_empty() {
        return Ok(());
    }
    let count = if comparison { 100 } else { 1 };
    for index in 0..count {
        let id = if comparison {
            Uuid::from_u128(0xb79dbad7_d038_46ce_b5a4_000000000000 + index as u128)
        } else {
            Uuid::new_v4()
        };
        library.profiles.push(Profile {
            icon_id: None,
            imported_pack: None,
            server_address: None,
            id,
            name: if index == 0 {
                "Vanilla survival".into()
            } else {
                format!("Review instance {index:03}")
            },
            version: VanillaVersion::ALL[index % VanillaVersion::ALL.len()],
            memory_mib: 2048,
            window_width: 1280,
            window_height: 720,
        });
    }
    library.validate()?;
    storage::save(&root.join("personal"), &library)
}

pub fn change(root: &Path, action: ProfileAction) -> Result<Vec<Profile>, String> {
    change_if_current(root, action, None)
}

/// A native editor can remain open while another launcher edits the same profile.
pub fn change_if_current(
    root: &Path,
    action: ProfileAction,
    original: Option<&Profile>,
) -> Result<Vec<Profile>, String> {
    let (_lock, mut library) = storage::open(root)?;
    if let Some(profile) = original {
        if library.profiles.get(library.position(profile.id)?) != Some(profile) {
            return Err("This instance changed in another window. Reopen its settings to use the latest values.".into());
        }
    }
    let source = match &action {
        ProfileAction::Clone { id, .. } => Some(*id),
        _ => None,
    };
    library.change(action)?;
    let dir = root.join("personal");
    if let Some(source) = source {
        let mut choices = storage::java_choices(&dir)?;
        if let Some(choice) = choices.get(&source).cloned() {
            let clone = library.profiles.last().expect("a new clone was appended");
            choices.insert(clone.id, choice);
            // Publish preferences before exposing the newly allocated profile ID.
            storage::save_java_choices(&dir, &choices)?;
        }
    }
    storage::save(&dir, &library)?;
    Ok(library.profiles)
}

/// Create the private instance contents before publishing the profile in the library.
pub fn import_instance(
    root: &Path,
    id: Uuid,
    version: VanillaVersion,
    pack: ImportedPack,
    populate: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<Profile, String> {
    let (_lock, mut library) = storage::open(root)?;
    if let Some(existing) = library.profiles.iter().find(|profile| profile.id == id) {
        if existing.version == version && existing.imported_pack.as_ref() == Some(&pack) {
            return Ok(existing.clone());
        }
        return Err("This import identity already belongs to another instance.".into());
    }
    let profile = Profile {
        id,
        name: pack.name.clone(),
        icon_id: None,
        imported_pack: Some(pack),
        server_address: None,
        version,
        memory_mib: 2048,
        window_width: 1280,
        window_height: 720,
    };
    profile.validate()?;
    if let Err(error) = storage::import(root, &mut library, &profile, populate) {
        // An index or directory sync can fail after the durable content has been renamed.
        // Give the journal a chance to finish the original import under the same ID.
        drop(_lock);
        return match list(root) {
            Ok(profiles) => profiles
                .into_iter()
                .find(|item| item.id == profile.id && item.imported_pack == profile.imported_pack)
                .ok_or(error),
            Err(_) => Err(error),
        };
    }
    Ok(profile)
}

/// Set only by the native folder picker or the managed runtime installer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum JavaChoice {
    Directory(PathBuf),
    Managed,
}

pub fn java_choice(root: &Path, id: Uuid) -> Result<(VanillaVersion, Option<JavaChoice>), String> {
    let (_lock, library) = storage::open(root)?;
    let version = library.profiles[library.position(id)?].version;
    let mut choices = storage::java_choices(&root.join("personal"))?;
    Ok((version, choices.remove(&id)))
}

pub fn select_java(
    root: &Path,
    id: Uuid,
    expected: VanillaVersion,
    choice: JavaChoice,
) -> Result<(), String> {
    let (_lock, library) = storage::open(root)?;
    if library.profiles[library.position(id)?].version != expected {
        return Err("The profile's Minecraft version changed. Check setup again.".into());
    }
    if matches!(&choice, JavaChoice::Directory(path) if !path.is_absolute()) {
        return Err("Choose an installed Java directory.".into());
    }
    let dir = root.join("personal");
    let mut choices = storage::java_choices(&dir)?;
    let ids: HashSet<_> = library.profiles.iter().map(|profile| profile.id).collect();
    choices.retain(|id, _| ids.contains(id));
    choices.insert(id, choice);
    storage::save_java_choices(&dir, &choices)
}
