// SPDX-License-Identifier: MPL-2.0
//! Worker-facing helper for Home background scenes.
//!
//! Home deliberately has no built-in landscape art or playback loop. A separate
//! worker owns scene assets and animation; this module is the small, dependency
//! free seam between that worker and the native Home surface. It also compiles
//! on its own from `scripts/home-scene-helper.rs`, so the worker can exercise
//! the contract without booting GPUI.
#![allow(dead_code, reason = "The integration seam is completed by the scene worker.")]

/// Version of the scene-control contract. Bump only with a worker handshake.
pub const CONTRACT_VERSION: u32 = 1;

/// A scene announced by the worker. `source` is a worker-owned local image or
/// frame URI. Home never invents a fallback image when the list is empty.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SceneDescriptor {
    pub id: String,
    pub label: String,
    pub source: String,
    pub animated: bool,
}

/// One worker-provided frame that Home can paint. The source remains owned by
/// the worker; Home does not fetch it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HomeFrame {
    pub source: String,
    pub width: u32,
    pub height: u32,
}

impl HomeFrame {
    pub fn new(source: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            source: source.into(),
            width,
            height,
        }
    }
}

/// The only background values Home understands. `Unavailable` draws the
/// approved neutral surface instead of a generated landscape.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HomeBackground {
    Frame(HomeFrame),
    Unavailable,
}

/// Controls Home exposes to the worker. They are deliberately transport
/// agnostic so the worker can use a channel, a process pipe or a local socket.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SceneControl {
    Select(String),
    Previous,
    Next,
    SetMotion(bool),
    Restart,
}

/// State the worker keeps alongside its playback loop.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SceneState {
    pub active: Option<String>,
    pub motion: bool,
    pub revision: u64,
}

impl Default for SceneState {
    fn default() -> Self {
        Self {
            active: None,
            motion: false,
            revision: 0,
        }
    }
}

/// What Home should do after a control is applied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SceneOutcome {
    /// The worker acknowledged the control and supplied the new state.
    Applied(SceneState),
    /// The control names a scene the worker no longer has.
    UnknownScene { id: String },
    /// No scene worker is connected. Home stays on its neutral background.
    Unavailable,
}

/// Implemented by the separate worker bridge. A native adapter can wrap a
/// channel or asset service; the standalone helper uses a no-asset worker so a
/// missing service is never confused with fictional artwork.
pub trait SceneWorker {
    fn scenes(&self) -> Vec<SceneDescriptor>;
    fn background(&self, state: &SceneState) -> Option<HomeFrame>;
}

/// A worker bridge that has not been connected yet. It supplies no image.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableWorker;

impl SceneWorker for UnavailableWorker {
    fn scenes(&self) -> Vec<SceneDescriptor> {
        Vec::new()
    }

    fn background(&self, _: &SceneState) -> Option<HomeFrame> {
        None
    }
}

/// Minimal controller Home can hold. It validates controls against the
/// worker-provided scene list and leaves the worker responsible for decoding
/// or animating assets.
pub struct SceneController<W: SceneWorker> {
    worker: W,
    scenes: Vec<SceneDescriptor>,
    state: SceneState,
}

impl<W: SceneWorker> SceneController<W> {
    pub fn new(worker: W) -> Self {
        let scenes = worker.scenes();
        let active = scenes.first().map(|scene| scene.id.clone());
        Self {
            worker,
            scenes,
            state: SceneState {
                active,
                ..SceneState::default()
            },
        }
    }

    pub fn state(&self) -> &SceneState {
        &self.state
    }

    pub fn scenes(&self) -> &[SceneDescriptor] {
        &self.scenes
    }

    pub fn refresh_scenes(&mut self) {
        self.scenes = self.worker.scenes();
        if !self
            .scenes
            .iter()
            .any(|scene| Some(&scene.id) == self.state.active.as_ref())
        {
            self.state.active = self.scenes.first().map(|scene| scene.id.clone());
        }
    }

    pub fn apply(&mut self, control: SceneControl) -> SceneOutcome {
        match control {
            SceneControl::Select(id) => {
                if !self.scenes.iter().any(|scene| scene.id == id) {
                    return SceneOutcome::UnknownScene { id };
                }
                self.state.active = Some(id);
            }
            SceneControl::Previous => self.step(-1),
            SceneControl::Next => self.step(1),
            SceneControl::SetMotion(motion) => self.state.motion = motion,
            SceneControl::Restart => {}
        }
        self.state.revision = self.state.revision.saturating_add(1);
        SceneOutcome::Applied(self.state.clone())
    }

    pub fn background(&self) -> HomeBackground {
        self.worker
            .background(&self.state)
            .map_or(HomeBackground::Unavailable, HomeBackground::Frame)
    }

    fn step(&mut self, delta: isize) {
        if self.scenes.is_empty() {
            self.state.active = None;
            return;
        }
        let current = self
            .state
            .active
            .as_ref()
            .and_then(|active| self.scenes.iter().position(|scene| &scene.id == active));
        let next = match (current, delta.is_negative()) {
            (Some(index), true) => (index + self.scenes.len() - 1) % self.scenes.len(),
            (Some(index), false) => (index + 1) % self.scenes.len(),
            (None, true) => self.scenes.len() - 1,
            (None, false) => 0,
        };
        self.state.active = Some(self.scenes[next].id.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture;

    impl SceneWorker for Fixture {
        fn scenes(&self) -> Vec<SceneDescriptor> {
            vec![
                SceneDescriptor {
                    id: "one".into(),
                    label: "One".into(),
                    source: "worker/one.png".into(),
                    animated: false,
                },
                SceneDescriptor {
                    id: "two".into(),
                    label: "Two".into(),
                    source: "worker/two.png".into(),
                    animated: true,
                },
            ]
        }

        fn background(&self, state: &SceneState) -> Option<HomeFrame> {
            state.active.as_ref().map(|_| HomeFrame::new("worker/active", 1, 1))
        }
    }

    #[test]
    fn controls_wrap_and_unknown_scenes_do_not_mutate_state() {
        let mut controller = SceneController::new(Fixture);
        assert_eq!(controller.state().active.as_deref(), Some("one"));
        assert!(matches!(
            controller.apply(SceneControl::Previous),
            SceneOutcome::Applied(SceneState { active: Some(id), .. }) if id == "two"
        ));
        assert!(matches!(
            controller.apply(SceneControl::Next),
            SceneOutcome::Applied(SceneState { active: Some(id), .. }) if id == "one"
        ));
        let revision = controller.state().revision;
        assert!(matches!(
            controller.apply(SceneControl::Select("missing".into())),
            SceneOutcome::UnknownScene { id } if id == "missing"
        ));
        assert_eq!(controller.state().revision, revision);
        assert_eq!(controller.state().active.as_deref(), Some("one"));
    }

    #[test]
    fn an_unconnected_worker_never_returns_a_landscape() {
        let mut controller = SceneController::new(UnavailableWorker);
        assert_eq!(controller.background(), HomeBackground::Unavailable);
        assert!(matches!(
            controller.apply(SceneControl::Next),
            SceneOutcome::Applied(SceneState { active: None, .. })
        ));
        assert_eq!(controller.background(), HomeBackground::Unavailable);
    }
}
