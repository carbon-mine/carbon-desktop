// SPDX-License-Identifier: MPL-2.0
use carbon_profiles::world_preview::{Destination, EntryKey, WorldMap};
use super::{Launcher, home_playback::HomePlayback, scenery::{Landscape, SceneView}};
use gpui::{Context, Entity, Task, Window, prelude::*};
use std::{path::Path, time::Instant};

pub(super) struct HomeArtwork {
    pub(super) playback: HomePlayback,
    pub(super) view: Entity<SceneView>,
    pub(super) world: Option<EntryKey>,
    applied: (Landscape, bool),
    rotation: Option<(Instant, Task<()>)>,
}

impl HomeArtwork {
    pub(super) fn new(root: &Path, cx: &mut Context<Launcher>) -> Self {
        let playback = HomePlayback::open(root);
        let view = cx.new(|cx| SceneView::new(playback.scene, cx));
        Self {view, world: None, applied: (playback.scene, false), playback, rotation: None}
    }
}

impl Launcher {
    pub(super) fn home_world_choices(&self) -> Vec<(EntryKey, String)> {
        if !self.artwork.playback.personal_worlds {
            return Vec::new();
        }
        self.profiles.iter().filter_map(|profile| {
            let visit = self.world_previews.latest(profile.id)?;
            if !matches!(visit.key.destination, Destination::LocalWorld(_)) || !matches!(visit.map, WorldMap::Drawable { .. }) {
                return None;
            }
            Some((visit.key.clone(), format!("{} · {} · {}", profile.name, visit.display_name, visit.key.dimension)))
        }).collect()
    }

    pub(super) fn home_world_image(&self) -> Option<std::sync::Arc<gpui::RenderImage>> {
        if !self.artwork.playback.personal_worlds {
            return None;
        }
        let key = self.artwork.world.as_ref()?;
        let visit = self.world_previews.latest(key.instance)?;
        if &visit.key != key {return None}
        match &visit.map {
            WorldMap::Drawable { image, .. } => Some(image.clone()),
            _ => None,
        }
    }

    pub(super) fn rotate_home_scene(&mut self) {
        let worlds = self.home_world_choices().into_iter()
            .filter(|(key,_)| Some(key) != self.artwork.world.as_ref()).collect::<Vec<_>>();
        let seed = uuid::Uuid::new_v4().as_u128();
        let choice = (seed % (Landscape::ALL.len() + worlds.len()) as u128) as usize;
        self.artwork.world = worlds.get(choice).map(|(key,_)|key.clone());
        self.artwork.playback.shuffle(&self.root.path);
    }

    pub(super) fn sync_home_artwork(&mut self, home_visible: bool, window: &mut Window, cx: &mut Context<Self>) {
        let active = home_visible && self.popup.is_none() && window.is_window_active() && !self.reduced_motion();
        let now = Instant::now();
        let mut deadline = self.artwork.playback.deadline(active, now);
        if deadline.is_some_and(|d| d <= now) {
            self.rotate_home_scene();
            deadline = self.artwork.playback.deadline(active, now);
        }
        if self.artwork.world.is_some() && self.home_world_image().is_none() {
            self.artwork.world = None;
        }
        let art = &mut self.artwork;
        let scene = art.playback.scene;
        let playing = active && !art.playback.paused && art.world.is_none();
        if art.applied != (scene, playing) {
            art.view.update(cx, |view, cx| {
                if art.applied.0 != scene {
                    view.set_landscape(scene, cx);
                }
                view.set_playing(playing, cx);
            });
            art.applied = (scene, playing);
        }
        if art.rotation.as_ref().map(|(due, _)| *due) != deadline {
            art.rotation = deadline.map(|due| {
                let timer = cx.background_executor().timer(due.saturating_duration_since(now));
                (due, cx.spawn(async move |this, cx| {
                    timer.await;
                    let _ = this.update(cx, |_, cx| cx.notify());
                }))
            });
        }
    }
}
