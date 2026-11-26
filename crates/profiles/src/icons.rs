// SPDX-License-Identifier: MPL-2.0
//! Original Carbon instance artwork shared by both desktop editions.
use super::{storage, Profile};
use serde::Deserialize;
use std::{path::Path, sync::LazyLock};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct InstanceIcon {
    pub id: String,
    pub label: String,
    pub rows: [u16; 12],
}

pub fn catalog() -> &'static [InstanceIcon] {
    static ICONS: LazyLock<Vec<InstanceIcon>> = LazyLock::new(|| {
        serde_json::from_str(include_str!("../../../design-system/instance-icons.json"))
            .expect("the built-in icon catalog must be valid")
    });
    &ICONS
}

fn known(id: &str) -> Option<&'static InstanceIcon> {
    catalog().iter().find(|icon| icon.id == id)
}

fn automatic(profile: &Profile) -> &'static InstanceIcon {
    let id = if profile.imported_pack.is_some() {
        "pack"
    } else {
        "block"
    };
    known(id).expect("required automatic icon")
}

pub fn displayed(profile: &Profile) -> &'static InstanceIcon {
    profile
        .icon_id
        .as_deref()
        .and_then(known)
        .unwrap_or_else(|| automatic(profile))
}

/// A picker owns its draft until Save. Unknown future IDs survive opening and cancelling the picker.
#[derive(Clone)]
pub struct IconDraft {
    profile_id: Uuid,
    automatic: &'static InstanceIcon,
    original: Option<String>,
    selected: Option<String>,
}

impl IconDraft {
    pub fn new(profile: &Profile) -> Self {
        Self {
            profile_id: profile.id,
            automatic: automatic(profile),
            original: profile.icon_id.clone(),
            selected: profile.icon_id.clone(),
        }
    }

    pub fn selected_index(&self) -> usize {
        catalog()
            .iter()
            .position(|icon| Some(icon.id.as_str()) == self.selected.as_deref())
            .map_or(0, |index| index + 1)
    }

    pub fn choose(&mut self, index: usize) {
        if index == 0 {
            self.selected = None;
        } else if let Some(icon) = catalog().get(index - 1) {
            self.selected = Some(icon.id.clone());
        }
    }

    pub fn preview(&self) -> &'static InstanceIcon {
        self.selected
            .as_deref()
            .and_then(known)
            .unwrap_or(self.automatic)
    }

    pub fn unavailable_choice(&self) -> bool {
        self.selected
            .as_deref()
            .is_some_and(|id| known(id).is_none())
    }

    pub fn changed(&self) -> bool {
        self.original != self.selected
    }

    pub fn save(&self, root: &Path) -> Result<Profile, String> {
        set_icon(
            root,
            self.profile_id,
            self.original.as_deref(),
            self.selected.as_deref(),
        )
    }
}

/// Compare only this preference under the existing library write lock. Preserve newer unrelated edits.
pub fn set_icon(
    root: &Path,
    id: Uuid,
    expected: Option<&str>,
    choice: Option<&str>,
) -> Result<Profile, String> {
    let (_lock, mut library) = storage::open(root)?;
    let index = library.position(id)?;
    let original = &library.profiles[index];
    if original.icon_id.as_deref() != expected {
        return Err("This instance’s icon changed in another window. Reopen the picker to use the latest choice.".into());
    }
    if choice == expected {
        return Ok(original.clone());
    }
    if choice.is_some_and(|id| known(id).is_none()) {
        return Err("Choose an icon from the built-in collection.".into());
    }
    library.profiles[index].icon_id = choice.map(str::to_owned);
    storage::save(&root.join("personal"), &library)?;
    Ok(library.profiles[index].clone())
}
