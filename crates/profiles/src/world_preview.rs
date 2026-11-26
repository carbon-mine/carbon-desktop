// SPDX-License-Identifier: MPL-2.0
//! Session-side evidence for the Library. Only a qualified game-entry observer
//! and a map decoder should feed this store. It is independent of launch choices.

use crate::ServerAddress;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Destination {
    LocalWorld(String),
    Multiplayer(ServerAddress),
}

/// Session-wide increasing sequence assigned by the game-entry observer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntryKey {
    pub instance: Uuid,
    pub sequence: u64,
    pub destination: Destination,
    pub dimension: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Coverage {
    Complete,
    Partial,
}

/// A drawable is supplied by the qualified decoder. For partial coverage the
/// drawable must preserve the uncovered area as transparent pixels.
pub enum WorldMap<Image> {
    NoCapture,
    Preparing,
    Unsupported,
    Unreadable,
    Drawable { coverage: Coverage, image: Image },
}

pub struct WorldVisit<Image> {
    pub key: EntryKey,
    pub display_name: String,
    pub map: WorldMap<Image>,
    map_revision: Option<u64>,
}

pub struct WorldPreviews<Image> {
    visits: HashMap<Uuid, WorldVisit<Image>>,
}

impl<Image> Default for WorldPreviews<Image> {
    fn default() -> Self {
        Self {
            visits: HashMap::new(),
        }
    }
}

impl<Image> WorldPreviews<Image> {
    /// A missing entry means Carbon has no verified observation in this session.
    pub fn latest(&self, instance: Uuid) -> Option<&WorldVisit<Image>> {
        self.visits.get(&instance)
    }

    /// An actual world-entry event, including a dimension switch or in-game join.
    /// The supervisor must order all sessions for a given instance on one clock.
    pub fn record_confirmed_entry(&mut self, key: EntryKey, display_name: String) -> bool {
        if self
            .latest(key.instance)
            .is_some_and(|v| v.key.sequence >= key.sequence)
        {
            return false;
        }
        self.visits.insert(
            key.instance,
            WorldVisit {
                key,
                display_name,
                map: WorldMap::NoCapture,
                map_revision: None,
            },
        );
        true
    }

    /// Reserve revision numbers in request order before starting provider work.
    /// A completion for an older visit or an already superseded request is ignored.
    pub fn update_map(&mut self, key: &EntryKey, revision: u64, map: WorldMap<Image>) -> bool {
        let Some(visit) = self.visits.get_mut(&key.instance) else {
            return false;
        };
        if visit.key != *key {
            return false;
        }
        if let Some(last) = visit.map_revision {
            let completes_current_request =
                matches!(visit.map, WorldMap::Preparing) && !matches!(map, WorldMap::Preparing);
            if revision < last || (revision == last && !completes_current_request) {
                return false;
            }
        }
        visit.map_revision = Some(revision);
        visit.map = map;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(instance: Uuid, sequence: u64, dimension: &str) -> EntryKey {
        EntryKey {
            instance,
            sequence,
            destination: Destination::LocalWorld("Forest".into()),
            dimension: dimension.into(),
        }
    }

    #[test]
    fn captures_and_passive_target_inventory_are_not_entry_evidence() {
        let folder = tempfile::tempdir().unwrap();
        let instance = crate::change(
            folder.path(),
            crate::ProfileAction::Create {
                name: "Map review".into(),
                version: crate::VanillaVersion::Release26_3,
                memory_mib: 2048,
                window_width: 1280,
                window_height: 720,
            },
        )
        .unwrap()[0]
            .id;
        let key = entry(instance, 1, "minecraft:overworld");
        let mut previews = WorldPreviews::default();
        assert!(!previews.update_map(
            &key,
            1,
            WorldMap::Drawable {
                coverage: Coverage::Complete,
                image: "fixture pixels",
            }
        ));
        let saves = folder
            .path()
            .join("instances-v1")
            .join(instance.to_string())
            .join("game/saves/Forest");
        std::fs::create_dir_all(&saves).unwrap();
        std::fs::write(saves.join("level.dat"), b"fixture").unwrap();
        let inventory = crate::instance_targets(folder.path(), instance).unwrap();
        assert_eq!(inventory.worlds.len(), 1);
        let selected = crate::change_instance_targets(
            folder.path(),
            instance,
            inventory.revision,
            crate::TargetAction::Select(Some(inventory.worlds[0].key.clone())),
        )
        .unwrap();
        assert!(selected.selected.is_some());
        assert!(previews.latest(instance).is_none());
    }

    #[test]
    fn scopes_worlds_dimensions_and_server_joins_independently() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let mut previews = WorldPreviews::<()>::default();
        let first = entry(a, 1, "minecraft:overworld");
        let second = entry(a, 2, "minecraft:the_nether");
        let neighbor = entry(b, 1, "minecraft:overworld");
        for key in [&first, &neighbor, &second] {
            assert!(previews.record_confirmed_entry(key.clone(), "Forest".into()));
        }
        assert!(!previews.update_map(
            &first,
            9,
            WorldMap::Drawable {
                coverage: Coverage::Complete,
                image: ()
            }
        ));
        let server = EntryKey {
            destination: Destination::Multiplayer(
                ServerAddress::parse("play.example.org").unwrap(),
            ),
            ..entry(a, 3, "minecraft:overworld")
        };
        assert!(previews.record_confirmed_entry(server.clone(), "Lantern Bay".into()));
        assert!(!previews.record_confirmed_entry(second, "Old save".into()));
        assert_eq!(previews.latest(a).unwrap().key, server);
        assert_eq!(previews.latest(b).unwrap().key, neighbor);
    }

    #[test]
    fn an_old_provider_response_cannot_overwrite_a_newer_request_or_visit() {
        let a = Uuid::new_v4();
        let key = entry(a, 1, "minecraft:overworld");
        let mut previews = WorldPreviews::default();
        assert!(previews.record_confirmed_entry(key.clone(), "Forest".into()));
        assert!(previews.update_map(&key, 1, WorldMap::Preparing));
        assert!(previews.update_map(&key, 2, WorldMap::Preparing));
        assert!(!previews.update_map(&key, 1, WorldMap::Unreadable));
        assert!(previews.update_map(
            &key,
            2,
            WorldMap::Drawable {
                coverage: Coverage::Partial,
                image: 7
            }
        ));
        assert!(!previews.update_map(&key, 2, WorldMap::NoCapture));
        assert!(matches!(
            previews.latest(a).unwrap().map,
            WorldMap::Drawable {
                coverage: Coverage::Partial,
                image: 7
            }
        ));
        let next = entry(a, 2, "minecraft:overworld");
        assert!(previews.record_confirmed_entry(next.clone(), "Forest".into()));
        assert!(!previews.update_map(
            &key,
            3,
            WorldMap::Drawable {
                coverage: Coverage::Complete,
                image: 8
            }
        ));
        assert!(matches!(
            previews.latest(a).unwrap().map,
            WorldMap::NoCapture
        ));
        assert!(previews.update_map(&next, 1, WorldMap::Unsupported));
        assert_eq!(previews.latest(a).unwrap().display_name, "Forest");
    }
}
