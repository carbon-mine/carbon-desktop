// SPDX-License-Identifier: MPL-2.0
use super::data::size_label;
use gpui::{Context, Div, FontFeatures, Window, div, prelude::*, px, relative};
use std::time::{Duration, Instant};

/// A backend callback measurement. File and byte totals belong to this phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transfer {
    pub phase: &'static str,
    pub files: Option<(u64, u64)>,
    pub bytes: (u64, u64),
}

impl Transfer {
    fn same_series(self, other: Self) -> bool {
        self.phase == other.phase
            && self.files.map(|(_, total)| total) == other.files.map(|(_, total)| total)
            && self.bytes.1 == other.bytes.1
            && self.bytes.0 >= other.bytes.0
            && self
                .files
                .zip(other.files)
                .is_none_or(|(new, old)| new.0 >= old.0)
    }

    pub fn fraction(self) -> Option<f64> {
        let (done, total) = self.files.unwrap_or(self.bytes);
        (total > 0).then(|| done.min(total) as f64 / total as f64)
    }

    pub fn counts(self) -> String {
        let bytes = format!(
            "{} / {}",
            size_label(self.bytes.0),
            size_label(self.bytes.1)
        );
        match self.files {
            Some((done, total)) => format!("{done} / {total} files · {bytes}"),
            None => bytes,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Update {
    Busy(&'static str),
    Transfer(Transfer),
}

impl From<&'static str> for Update {
    fn from(value: &'static str) -> Self {
        Self::Busy(value)
    }
}

impl Update {
    pub fn label(self) -> &'static str {
        match self {
            Self::Busy(label) => label,
            Self::Transfer(value) => value.phase,
        }
    }
}

const SETTLE: Duration = Duration::from_millis(180);
const BUSY_MOTION: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq)]
struct Sample {
    value: Transfer,
    fraction: Option<f64>,
}

impl From<Transfer> for Sample {
    fn from(value: Transfer) -> Self {
        Self {
            value,
            fraction: value.fraction(),
        }
    }
}

/// The launcher samples worker updates independently of the display refresh rate.
/// This child requests frames only while its own visible values are moving.
pub struct Meter {
    pub target: Update,
    from: Option<Sample>,
    changed: Instant,
    reduced: bool,
    digits: FontFeatures,
}

impl Meter {
    pub fn new(now: Instant, reduced: bool) -> Self {
        Self {
            target: Update::Busy("Starting…"),
            from: None,
            changed: now,
            reduced,
            digits: FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)])),
        }
    }

    pub fn retarget(&mut self, update: Update, reduced: bool, now: Instant) -> bool {
        if self.target == update && self.reduced == reduced {
            return false;
        }
        let current = self.sample(now).0;
        self.from = match (current, update) {
            (Some(old), Update::Transfer(new))
                if !reduced
                    && !self.reduced
                    && matches!(self.target, Update::Transfer(last) if new.same_series(last))
                    && new.fraction() != Some(1.0) =>
            {
                Some(old)
            }
            _ => None,
        };
        self.target = update;
        self.changed = now;
        self.reduced = reduced;
        true
    }

    fn sample(&self, now: Instant) -> (Option<Sample>, bool) {
        let age = now.saturating_duration_since(self.changed);
        let Update::Transfer(mut value) = self.target else {
            return (None, !self.reduced && age < BUSY_MOTION);
        };
        let Some(old) = self.from.filter(|_| age < SETTLE && !self.reduced) else {
            return (Some(value.into()), false);
        };
        let t = age.as_secs_f64() / SETTLE.as_secs_f64();
        let ease = 1.0 - (1.0 - t).powi(3);
        let count = |start: u64, end: u64| {
            start
                .saturating_add(((end - start) as f64 * ease) as u64)
                .min(end)
        };
        let fraction = old
            .fraction
            .zip(value.fraction())
            .map(|(start, end)| start + (end - start) * ease);
        value.bytes.0 = count(old.value.bytes.0, value.bytes.0);
        if let (Some((start, _)), Some((end, _))) = (old.value.files, &mut value.files) {
            *end = count(start, *end);
        }
        (Some(Sample { value, fraction }), true)
    }
}

impl Render for Meter {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_component::ActiveTheme;
        let now = Instant::now();
        let (sample, moving) = self.sample(now);
        if moving {
            window.request_animation_frame();
        }
        if let Some(value) = sample {
            let track = div().flex().h(px(3.)).w_full().bg(cx.theme().border);
            let mut body = div().flex().flex_col().gap_1().min_w_0().w_full();
            if let Some(fraction) = value.fraction {
                body = body.child(
                    track.child(
                        div()
                            .h_full()
                            .w(relative(fraction as f32))
                            .bg(cx.theme().foreground),
                    ),
                );
            }
            let mut numbers = div().text_xs().text_color(cx.theme().muted_foreground);
            numbers
                .text_style()
                .get_or_insert_with(Default::default)
                .font_features = Some(self.digits.clone());
            return body.child(numbers.child(value.value.counts()));
        }
        busy_dots(now.saturating_duration_since(self.changed), moving, cx)
    }
}

fn busy_dots(age: Duration, moving: bool, cx: &Context<Meter>) -> Div {
    use gpui_component::ActiveTheme;
    let mut row = div().flex().items_center().gap(px(3.)).h(px(12.));
    for i in 0..3 {
        let wave = (age.as_secs_f32() / 1.2 - i as f32 / 5.) * std::f32::consts::TAU;
        let opacity = if moving {
            0.65 + 0.35 * wave.sin()
        } else {
            0.7
        };
        row = row.child(
            div()
                .size(px(4.))
                .rounded_full()
                .bg(cx.theme().muted_foreground)
                .opacity(opacity),
        );
    }
    row
}

impl super::Launcher {
    pub(super) fn reduced_motion(&self) -> bool {
        #[cfg(windows)]
        {
            use windows::Win32::UI::WindowsAndMessaging::{
                SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW,
            };
            let mut enabled = 0i32;
            // SPI_GETCLIENTAREAANIMATION writes one Win32 BOOL to the provided address.
            unsafe {
                let _ = SystemParametersInfoW(
                    SPI_GETCLIENTAREAANIMATION,
                    0,
                    Some(std::ptr::from_mut(&mut enabled).cast()),
                    Default::default(),
                );
            }
            enabled == 0
        }
        #[cfg(target_os = "macos")]
        {
            objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
        }
        #[cfg(target_os = "linux")]
        {
            self.reduced_motion
        }
    }
}

#[cfg(target_os = "linux")]
pub fn observe_linux_motion(cx: &mut Context<super::Launcher>) {
    use ashpd::desktop::settings::Settings;
    use futures::StreamExt;
    const APPEARANCE: &str = "org.freedesktop.appearance";
    const GNOME: &str = "org.gnome.desktop.interface";
    cx.spawn(async move |this, cx| {
        let Ok(settings) = Settings::new().await else {
            return;
        };
        let mut changes = settings.receive_setting_changed().await.ok().map(Box::pin);
        loop {
            let reduced = match settings.read::<u32>(APPEARANCE, "reduced-motion").await {
                Ok(value) => value == 1,
                Err(_) => !settings
                    .read::<bool>(GNOME, "enable-animations")
                    .await
                    .unwrap_or(false),
            };
            if this
                .update(cx, |this, cx| {
                    if this.reduced_motion != reduced {
                        this.reduced_motion = reduced;
                        cx.notify();
                    }
                })
                .is_err()
            {
                break;
            }
            let Some(stream) = changes.as_mut() else {
                break;
            };
            loop {
                let Some(event) = stream.next().await else {
                    return;
                };
                if matches!(
                    (event.namespace(), event.key()),
                    (APPEARANCE, "reduced-motion") | (GNOME, "enable-animations")
                ) {
                    break;
                }
            }
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(done: u64) -> Transfer {
        Transfer {
            phase: "Verifying files",
            files: Some((done, 100)),
            bytes: (done * 1000, 100_000),
        }
    }

    #[test]
    fn interrupted_animation_continues_from_the_visible_sample() {
        let start = Instant::now();
        let mut meter = Meter::new(start, false);
        meter.retarget(Update::Transfer(files(0)), false, start);
        meter.retarget(Update::Transfer(files(40)), false, start);
        let middle = start + SETTLE / 2;
        let sample = meter.sample(middle).0.unwrap();
        assert!(sample.value.files.unwrap().0 > 0 && sample.value.files.unwrap().0 < 40);
        assert!(sample.value.bytes.0 > 0 && sample.value.bytes.0 < 40_000);
        assert!(!meter.retarget(Update::Transfer(files(40)), false, middle));
        meter.retarget(Update::Transfer(files(80)), false, middle);
        assert_eq!(meter.sample(middle), (Some(sample), true));
        assert_eq!(
            meter.sample(middle + SETTLE),
            (Some(files(80).into()), false)
        );
    }

    #[test]
    fn new_series_corrections_reduced_motion_and_completion_snap() {
        let now = Instant::now();
        let mut meter = Meter::new(now, false);
        meter.retarget(Update::Transfer(files(10)), false, now);
        meter.retarget(Update::Transfer(files(80)), false, now);
        // This correction is above the current visual value but below the previous real value.
        meter.retarget(Update::Transfer(files(30)), false, now);
        assert_eq!(meter.sample(now), (Some(files(30).into()), false));
        for next in [
            Transfer {
                phase: "Copying",
                ..files(60)
            },
            Transfer {
                files: Some((2, 300)),
                ..files(1)
            },
        ] {
            meter.retarget(Update::Transfer(next), false, now);
            assert_eq!(meter.sample(now), (Some(next.into()), false));
        }
        meter.retarget(Update::Transfer(files(88)), false, now);
        meter.retarget(Update::Transfer(files(90)), false, now);
        meter.retarget(Update::Transfer(files(90)), true, now);
        assert_eq!(meter.sample(now), (Some(files(90).into()), false));
        meter.retarget(Update::Transfer(files(91)), false, now);
        meter.retarget(Update::Transfer(files(100)), false, now);
        assert_eq!(meter.sample(now), (Some(files(100).into()), false));
    }

    #[test]
    fn rail_moves_continuously_between_two_discrete_files() {
        let start = Instant::now();
        let mut meter = Meter::new(start, false);
        let sample = |n| Transfer {
            files: Some((n, 2)),
            ..files(n)
        };
        meter.retarget(Update::Transfer(sample(0)), false, start);
        meter.retarget(Update::Transfer(sample(1)), false, start);
        let middle = meter.sample(start + SETTLE / 2).0.unwrap();
        assert_eq!(middle.value.files, Some((0, 2)));
        assert!(middle.fraction.unwrap() > 0. && middle.fraction.unwrap() < 0.5);
        assert_eq!(meter.target, Update::Transfer(sample(1)));
    }

    #[test]
    fn busy_motion_is_bounded_and_does_not_retain_transfer_amounts() {
        let start = Instant::now();
        let mut meter = Meter::new(start, false);
        assert_eq!(meter.sample(start), (None, true));
        assert_eq!(meter.sample(start + BUSY_MOTION), (None, false));
        meter.retarget(Update::Transfer(files(12)), false, start);
        meter.retarget(Update::Busy("Activating"), true, start);
        assert_eq!(meter.sample(start), (None, false));
        assert_eq!(
            Transfer {
                files: None,
                bytes: (0, 0),
                ..files(0)
            }
            .fraction(),
            None
        );
    }
}
