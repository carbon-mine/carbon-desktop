// SPDX-License-Identifier: MPL-2.0
use super::{scenery::Landscape, shell};
use serde::{Deserialize, Serialize};
use std::{path::Path, time::{Duration, Instant}};

const FILE: &str = "home-scenery.json";
const INTERVAL: Duration = Duration::from_secs(180);

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Preferences {
    last: String,
    paused: bool,
    personal_worlds: bool,
}

pub(super) struct HomePlayback {
    pub(super) scene: Landscape,
    pub(super) paused: bool,
    pub(super) automatic: bool,
    pub(super) personal_worlds: bool,
    remaining: Duration,
    started: Option<Instant>,
}

fn next_scene(previous: Option<Landscape>, seed: u128) -> Landscape {
    let choices: Vec<_> = Landscape::ALL.iter().copied()
        .filter(|scene| previous.is_none_or(|previous| scene.family() != previous.family()))
        .collect();
    if choices.is_empty() {
        // A valid catalog can contain just one landscape.
        return previous.unwrap_or(Landscape::ALL[0]);
    }
    choices[(seed % choices.len() as u128) as usize]
}

impl HomePlayback {
    pub(super) fn open(root: &Path) -> Self {
        let prefs: Preferences = shell::read(root, FILE).unwrap_or_default();
        let scene = next_scene(Landscape::from_id(&prefs.last), uuid::Uuid::new_v4().as_u128());
        let playback = Self { scene, paused: prefs.paused, personal_worlds: prefs.personal_worlds, automatic: true, remaining: INTERVAL, started: None };
        playback.save(root);
        playback
    }

    fn save(&self, root: &Path) {
        let prefs = Preferences { last: self.scene.id().to_string(), paused: self.paused, personal_worlds: self.personal_worlds };
        if let Err(error) = shell::write(root, FILE, &prefs) {
            eprintln!("Could not save Home scenery preferences: {error}");
        }
    }

    pub(super) fn select(&mut self, scene: Landscape, root: &Path) {
        self.scene = scene;
        self.automatic = false;
        self.reset_clock();
        self.save(root);
    }

    pub(super) fn set_personal_worlds(&mut self, enabled: bool, root: &Path) {
        self.personal_worlds = enabled;
        self.save(root);
    }

    pub(super) fn set_paused(&mut self, paused: bool, root: &Path) {
        self.paused = paused;
        self.save(root);
    }

    pub(super) fn shuffle(&mut self, root: &Path) {
        self.scene = next_scene(Some(self.scene), uuid::Uuid::new_v4().as_u128());
        self.automatic = true;
        self.reset_clock();
        self.save(root);
    }

    fn reset_clock(&mut self) {
        self.started = None;
        self.remaining = INTERVAL;
    }

    /// Advance only while Home is visible and motion is permitted. The caller
    /// owns a single cancellable timer for this deadline.
    pub(super) fn deadline(&mut self, visible_motion: bool, now: Instant) -> Option<Instant> {
        if self.automatic && !self.paused && visible_motion {
            Some(*self.started.get_or_insert(now) + self.remaining)
        } else {
            if let Some(started) = self.started.take() {
                self.remaining = self.remaining.saturating_sub(now.saturating_duration_since(started));
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shuffle_changes_the_landscape_even_if_a_second_angle_exists() {
        for previous in Landscape::ALL.iter().copied() {
            for seed in 0..14 {
                assert_ne!(next_scene(Some(previous), seed).family(), previous.family());
            }
        }
    }

    #[test]
    fn time_hidden_is_excluded_and_manual_selection_stops_shuffle() {
        let root = tempfile::tempdir().unwrap();
        let mut state = HomePlayback::open(root.path());
        let start = Instant::now();
        assert_eq!(state.deadline(true, start), Some(start + INTERVAL));
        assert_eq!(state.deadline(false, start + Duration::from_secs(15)), None);
        let resume = start + Duration::from_secs(900);
        assert_eq!(state.deadline(true, resume), Some(resume + INTERVAL - Duration::from_secs(15)));
        state.select(state.scene, root.path());
        assert_eq!(state.deadline(true, resume), None);
        state.set_paused(true, root.path());
        assert!(HomePlayback::open(root.path()).paused);
    }
}
