// SPDX-License-Identifier: MPL-2.0
//! Bounded opacity settle for navigation and selection changes.
//!
//! GPUI 0.2.2's `AnimationExt` starts its clock inside element state and has no
//! retarget, cancellation or reduced-motion input. Launcher navigation is
//! explicit state, so it keeps one sample here instead. The treatment follows
//! Windows Fluent's "Bare Minimum" opacity timing: linear and 83 ms. It never
//! hides content, stages a duplicate tree or changes layout, focus or hit
//! testing.

use std::time::{Duration, Instant};

/// Windows Fluent's "Bare Minimum" opacity timing.
pub(crate) const SETTLE: Duration = Duration::from_millis(83);
/// How long a snapped repeat suppresses new cues. Holding a tab key or
/// arrowing through rows repeats faster than one settle; every repeat inside
/// this window stays instant and extends the window.
const REPEAT_QUIET: Duration = Duration::from_millis(250);
/// The softest incoming content becomes. Content is always readable and
/// interactive, so there is no empty staging frame.
const MIN_OPACITY: f32 = 0.6;

#[derive(Clone, Copy, Debug)]
struct Fade {
    started: Instant,
}

/// One bounded settle per surface. Callers begin it only for an explicit
/// navigation or selection change, never for data arrival or theme swaps.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Settle {
    fade: Option<Fade>,
    quiet_until: Option<Instant>,
}

impl Settle {
    pub(crate) const fn new() -> Self {
        Self {
            fade: None,
            quiet_until: None,
        }
    }

    /// Whether a sample is still moving at `now`.
    pub(crate) fn active(&self, now: Instant) -> bool {
        self.fade
            .is_some_and(|fade| now.saturating_duration_since(fade.started) < SETTLE)
    }

    /// Whether a recent repeat is still suppressing new cues.
    fn quiet(&self, now: Instant) -> bool {
        self.quiet_until.is_some_and(|until| now < until)
    }

    /// Begin a settle after an explicit navigation or selection change.
    /// Returns whether a cue started. Reduced motion snaps. A repeat inside
    /// an active settle, or during the quiet window it opens, snaps and
    /// extends that window, so a held key stays instant instead of
    /// alternating between fades.
    pub(crate) fn begin(&mut self, now: Instant, reduced: bool) -> bool {
        if reduced {
            self.fade = None;
            self.quiet_until = None;
            return false;
        }
        if self.active(now) || self.quiet(now) {
            self.fade = None;
            self.quiet_until = Some(now + REPEAT_QUIET);
            return false;
        }
        self.fade = Some(Fade { started: now });
        self.quiet_until = None;
        true
    }

    /// Opacity for this paint and whether another frame is required.
    pub(crate) fn sample(&self, now: Instant, reduced: bool) -> (f32, bool) {
        if reduced {
            return (1.0, false);
        }
        let Some(fade) = self.fade else {
            return (1.0, false);
        };
        let elapsed = now.saturating_duration_since(fade.started);
        if elapsed >= SETTLE {
            return (1.0, false);
        }
        let progress = elapsed.as_secs_f32() / SETTLE.as_secs_f32();
        (MIN_OPACITY + (1.0 - MIN_OPACITY) * progress, true)
    }

    /// Finish immediately when another change supersedes this cue.
    pub(crate) fn snap(&mut self) {
        self.fade = None;
        self.quiet_until = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settles_linearly_and_stops_requesting_frames() {
        let start = Instant::now();
        let mut settle = Settle::new();
        assert_eq!(settle.sample(start, false), (1.0, false));
        assert!(settle.begin(start, false));
        assert_eq!(settle.sample(start, false), (MIN_OPACITY, true));
        let middle = settle.sample(start + SETTLE / 2, false);
        assert!(middle.0 > MIN_OPACITY && middle.0 < 1.0);
        assert!(middle.1);
        assert_eq!(settle.sample(start + SETTLE, false), (1.0, false));
        assert!(!settle.active(start + SETTLE));
    }

    #[test]
    fn reduced_motion_never_starts_or_requests_frames() {
        let start = Instant::now();
        let mut settle = Settle::new();
        assert!(!settle.begin(start, true));
        assert!(!settle.active(start));
        assert_eq!(settle.sample(start, true), (1.0, false));
    }

    #[test]
    fn repeat_inside_settle_snaps_instead_of_restarting() {
        let start = Instant::now();
        let mut settle = Settle::new();
        assert!(settle.begin(start, false));
        let repeat = start + SETTLE / 2;
        assert!(settle.active(repeat));
        assert!(!settle.begin(repeat, false));
        assert_eq!(settle.sample(repeat, false), (1.0, false));
        // A deliberate change after the quiet window settles again.
        let later = repeat + REPEAT_QUIET + Duration::from_millis(1);
        assert!(settle.begin(later, false));
        assert!(settle.active(later));
    }

    #[test]
    fn held_key_repeats_stay_instant_and_extend_the_quiet_window() {
        let start = Instant::now();
        let mut settle = Settle::new();
        assert!(settle.begin(start, false));
        // A 30 Hz key repeat starts inside the first settle and must never
        // alternate back into a fade while the key is held.
        let mut now = start;
        for _ in 0..12 {
            now += Duration::from_millis(33);
            assert!(!settle.begin(now, false));
            assert_eq!(settle.sample(now, false), (1.0, false));
            assert!(!settle.active(now));
        }
        // Releasing the key leaves the last repeat's quiet window in force,
        // and the next deliberate change after it settles normally.
        let later = now + REPEAT_QUIET + Duration::from_millis(1);
        assert!(settle.begin(later, false));
        assert!(settle.active(later));
    }

    #[test]
    fn snap_finishes_a_cue() {
        let start = Instant::now();
        let mut settle = Settle::new();
        assert!(settle.begin(start, false));
        settle.snap();
        assert!(!settle.active(start));
        assert_eq!(settle.sample(start, false), (1.0, false));
    }
}
