// SPDX-License-Identifier: MPL-2.0
use gpui::{App, Context, ObjectFit, RenderImage, Window, div, img, prelude::*, rgb};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

struct Scene {
    id: &'static str,
    family: &'static str,
    name: &'static str,
    attribution: &'static str,
    poster: &'static [u8],
    motion: Option<&'static [u8]>,
    fps: u8,
    forward: bool,
}

#[derive(Clone, Copy)]
pub struct Landscape(&'static Scene);

impl PartialEq for Landscape {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}
impl Eq for Landscape {}

impl Landscape {
    pub const ALL: &'static [Self] = &include!(concat!(env!("OUT_DIR"), "/landscape_catalog.rs"));
    pub fn id(self) -> &'static str {
        self.0.id
    }
    pub fn name(self) -> &'static str {
        self.0.name
    }
    pub fn family(self) -> &'static str {
        self.0.family
    }
    pub fn attribution(self) -> &'static str {
        self.0.attribution
    }
    pub fn from_id(id: &str) -> Option<Self> {
        Self::lookup(Self::ALL, id)
    }
    pub fn shuffle(previous: Option<Self>, seed: u128) -> Self {
        Self::shuffle_in(Self::ALL, previous, seed)
    }
    fn shuffle_in(catalog: &[Self], previous: Option<Self>, seed: u128) -> Self {
        let mut choices: Vec<_> = catalog
            .iter()
            .copied()
            .filter(|view| previous.is_none_or(|old| old.family() != view.family()))
            .collect();
        if choices.is_empty() {
            choices.extend(
                catalog
                    .iter()
                    .copied()
                    .filter(|&view| Some(view) != previous),
            );
        }
        if choices.is_empty() {
            return catalog[0];
        }
        choices[(seed % choices.len() as u128) as usize]
    }
    fn lookup(catalog: &[Self], id: &str) -> Option<Self> {
        catalog.iter().copied().find(|view| view.id() == id)
    }
}

struct Frames<'a> {
    table: &'a [u8],
    images: &'a [u8],
    count: usize,
}
fn word(bytes: &[u8], offset: usize) -> Option<usize> {
    Some(u32::from_le_bytes(bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?) as usize)
}
impl<'a> Frames<'a> {
    fn open(pack: &'a [u8]) -> Option<Self> {
        let count = word(pack, 0)?;
        if !(2..=1024).contains(&count) {
            return None;
        }
        let (header, images) = pack.split_at_checked((count + 2) * 4)?;
        let table = header.get(4..)?;
        if word(table, 0)? != 0 || word(table, count * 4)? != images.len() {
            return None;
        }
        Some(Self {
            table,
            images,
            count,
        })
    }
    fn at(&self, index: usize) -> Option<&'a [u8]> {
        if index >= self.count {
            return None;
        }
        let start = word(self.table, index * 4)?;
        let end = word(self.table, (index + 1) * 4)?;
        (start < end).then(|| self.images.get(start..end)).flatten()
    }
    fn index(&self, step: usize, forward: bool) -> usize {
        if forward {
            return step % self.count;
        }
        let cycle = (self.count - 1) * 2;
        let phase = step % cycle;
        phase.min(cycle - phase)
    }
}

fn decode(jpeg: &[u8]) -> Option<Arc<RenderImage>> {
    use zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions};
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::BGRA);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(jpeg), options);
    decoder.decode_headers().ok()?;
    if decoder.input_colorspace() == Some(ColorSpace::YCbCr) {
        let (width, height) = decoder.dimensions()?;
        let bytes = decoder.decode().ok()?;
        let image =
            image::RgbaImage::from_raw(width.try_into().ok()?, height.try_into().ok()?, bytes)?;
        return Some(Arc::new(RenderImage::new([image::Frame::new(image)])));
    }
    // Preserve the image crate's color handling for grayscale and CMYK posters.
    let mut rgba = image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg)
        .ok()?
        .into_rgba8();
    for pixel in rgba.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new([image::Frame::new(rgba)])))
}

// GPUI omits a window from App::windows during input callbacks. Retire its atlas
// allocation after the callback, when the window is back in the app.
fn retire(image: Option<Arc<RenderImage>>, cx: &mut App) {
    if let Some(image) = image {
        cx.defer(move |app| app.drop_image(image, None));
    }
}

// Keep presentation anchored to the original cadence and skip elapsed slots
// after a stall. Decode at most one successor, regardless of the delay.
fn frame_deadline(due: Instant, now: Instant, period: Duration) -> (Instant, usize) {
    let late = now.saturating_duration_since(due);
    let skipped = late.as_nanos() / period.as_nanos();
    let aligned = if skipped == 0 {
        due
    } else {
        now - Duration::from_nanos((late.as_nanos() % period.as_nanos()) as u64)
    };
    (aligned, (skipped as usize).wrapping_add(1))
}

fn after_present(step: usize, due: Instant, now: Instant, period: Duration) -> (usize, Instant) {
    // Give each visible image at least half a period after a late decode.
    let (aligned, advance) = frame_deadline(due, now, period);
    (
        step.wrapping_add(advance - 1),
        (aligned + period).max(now + period / 2),
    )
}

pub struct SceneView {
    landscape: Landscape,
    image: Option<Arc<RenderImage>>,
    step: usize,
    poster_pending: bool,
    interrupt: Option<async_channel::Sender<()>>,
    next_due: Option<Instant>,
    playing: bool,
    busy: bool,
    motion_failed: bool,
    poster_failed: bool,
    generation: u64,
}

impl SceneView {
    pub fn new(landscape: Landscape, cx: &mut Context<Self>) -> Self {
        cx.on_release(|view, app| retire(view.image.take(), app))
            .detach();
        let mut view = Self {
            landscape,
            image: None,
            step: 0,
            poster_pending: true,
            interrupt: None,
            next_due: None,
            playing: false,
            busy: false,
            motion_failed: false,
            poster_failed: false,
            generation: 0,
        };
        view.schedule(cx);
        view
    }
    pub fn set_landscape(&mut self, landscape: Landscape, cx: &mut Context<Self>) {
        if self.landscape == landscape {
            return;
        }
        self.interrupt.take();
        self.generation = self.generation.wrapping_add(1);
        self.landscape = landscape;
        self.motion_failed = false;
        self.poster_failed = false;
        self.step = 0;
        self.next_due = None;
        self.poster_pending = true;
        self.schedule(cx);
        cx.notify();
    }
    pub fn set_playing(&mut self, playing: bool, cx: &mut Context<Self>) {
        if self.playing == playing {
            return;
        }
        self.interrupt.take();
        self.generation = self.generation.wrapping_add(1);
        self.playing = playing;
        self.next_due = None;
        self.schedule(cx);
    }
    fn schedule(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.poster_failed {
            return;
        }
        let scene = self.landscape.0;
        let poster = self.poster_pending;
        let now = Instant::now();
        let (bytes, due, step) = if poster {
            (scene.poster, now, 0)
        } else {
            if !self.playing || self.motion_failed {
                return;
            }
            let Some(pack) = scene.motion else { return };
            let period = Duration::from_secs_f64(1. / f64::from(scene.fps));
            let (due, advance) = frame_deadline(self.next_due.unwrap_or(now + period), now, period);
            let step = self.step.wrapping_add(advance);
            let Some(bytes) = Frames::open(pack).and_then(|f| f.at(f.index(step, scene.forward)))
            else {
                self.fallback(cx);
                return;
            };
            (bytes, due, step)
        };
        let generation = self.generation;
        let (interrupt, interrupted) = async_channel::bounded::<()>(1);
        self.interrupt = Some(interrupt);
        self.busy = true;
        cx.spawn(async move |this, cx| {
            let decoded = cx
                .background_executor()
                .spawn(async move { decode(bytes) })
                .await;
            let remaining = due.saturating_duration_since(Instant::now());
            if !remaining.is_zero() {
                futures::future::select(
                    Box::pin(cx.background_executor().timer(remaining)),
                    Box::pin(interrupted.recv()),
                )
                .await;
            }
            let _ = this.update(cx, |view, cx| {
                view.busy = false;
                view.interrupt.take();
                if view.generation == generation {
                    if let Some(image) = decoded {
                        retire(view.image.replace(image), cx);
                        view.poster_pending = false;
                        if !poster {
                            let period = Duration::from_secs_f64(1. / f64::from(scene.fps));
                            let (phase, next) = after_present(step, due, Instant::now(), period);
                            view.step = phase;
                            view.next_due = Some(next);
                        }
                        cx.notify();
                    } else if poster {
                        view.poster_failed = true;
                        retire(view.image.take(), cx);
                        cx.notify();
                    } else {
                        view.fallback(cx);
                    }
                }
                view.schedule(cx);
            });
        })
        .detach();
    }
    fn fallback(&mut self, cx: &mut Context<Self>) {
        self.motion_failed = true;
        self.poster_pending = true;
        self.schedule(cx);
        cx.notify();
    }
}

impl Render for SceneView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .overflow_hidden()
            .bg(rgb(0x252525))
            .when_some(self.image.clone(), |view, image| {
                view.child(img(image).size_full().object_fit(ObjectFit::Cover))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_decode_skips_stale_frames_and_spaces_the_next_presentation() {
        let due = Instant::now();
        let period = Duration::from_millis(40);
        let late = due + Duration::from_millis(115);
        let (step, next) = after_present(1, due, late, period);
        assert_eq!(step, 3);
        assert_eq!(next, late + period / 2);
        assert_eq!(after_present(1, due, due, period), (1, due + period));
    }

    #[test]
    #[ignore = "timing report for the native CI matrix; run with --ignored --nocapture"]
    fn report_bundled_decode_timing() {
        let mut samples = Vec::new();
        for view in Landscape::ALL {
            let Some(pack) = view.0.motion.and_then(Frames::open) else {
                continue;
            };
            for i in (0..pack.count).step_by((pack.count / 8).max(1)) {
                let bytes = pack.at(i).unwrap();
                let start = Instant::now();
                let image = decode(bytes).unwrap();
                samples.push(start.elapsed().as_micros());
                std::hint::black_box(image);
            }
        }
        assert!(!samples.is_empty());
        samples.sort_unstable();
        println!(
            "Native JPEG decode, {} {}: {} samples, p50 {} us, p95 {} us",
            std::env::consts::OS,
            std::env::consts::ARCH,
            samples.len(),
            samples[samples.len() / 2],
            samples[95 * samples.len() / 100]
        );
    }
    #[test]
    fn optimized_decode_preserves_poster_colors_and_alpha() {
        for scene in Landscape::ALL {
            let mut reference =
                image::load_from_memory_with_format(scene.0.poster, image::ImageFormat::Jpeg)
                    .unwrap()
                    .into_rgba8();
            for pixel in reference.pixels_mut() {
                pixel.0.swap(0, 2);
            }
            assert_eq!(
                decode(scene.0.poster).unwrap().as_bytes(0),
                Some(reference.as_raw().as_slice()),
                "{}",
                scene.id()
            );
        }
    }
    #[test]
    fn prefetch_preserves_the_due_time_and_skips_missed_slots() {
        let due = Instant::now() + Duration::from_secs(1);
        let period = Duration::from_millis(40);
        assert_eq!(frame_deadline(due, due - period, period), (due, 1));
        assert_eq!(
            frame_deadline(due, due + Duration::from_millis(95), period),
            (due + 2 * period, 3)
        );
    }
    #[test]
    fn wind_advances_at_camera_turns_and_at_the_loop_seam() {
        let f = Frames {
            table: &[],
            images: &[],
            count: 4,
        };
        assert_eq!(
            (0..7).map(|i| f.index(i, true)).collect::<Vec<_>>(),
            [0, 1, 2, 3, 0, 1, 2]
        );
        assert_eq!(
            (0..7).map(|i| f.index(i, false)).collect::<Vec<_>>(),
            [0, 1, 2, 3, 2, 1, 0]
        );
    }
    #[test]
    fn adding_a_scenic_only_manifest_entry_populates_the_same_selection_and_playback_catalog() {
        const EXTENDED: &[Landscape] = &include!(concat!(env!("OUT_DIR"), "/landscape_fixture.rs"));
        let extra = Landscape::lookup(EXTENDED, "extra-scenic").unwrap();
        assert_eq!(EXTENDED.len(), Landscape::ALL.len() + 1);
        assert_eq!(
            EXTENDED
                .iter()
                .filter(|v| v.family() == extra.family())
                .count(),
            1
        );
        assert!(Landscape::from_id(extra.id()).is_none());
        assert!((0..EXTENDED.len()).any(|seed| Landscape::shuffle_in(
            EXTENDED,
            Some(EXTENDED[0]),
            seed as u128
        ) == extra));
        assert_eq!(
            Landscape::shuffle_in(&[extra], Some(extra), 0).id(),
            extra.id()
        );
        assert!(decode(extra.0.poster).is_some());
        let frames = Frames::open(extra.0.motion.unwrap()).unwrap();
        assert!(decode(frames.at(frames.index(2, false)).unwrap()).is_some());
        assert_eq!(extra.0.fps, 8);
    }
    #[test]
    fn bundled_frames_decode_and_return_to_the_authored_poster() {
        for &view in Landscape::ALL {
            assert_eq!(Landscape::from_id(view.id()).unwrap().id(), view.id());
            assert!(
                !view.name().is_empty()
                    && !view.family().is_empty()
                    && !view.attribution().is_empty()
            );
            assert!(decode(view.0.poster).is_some());
            if let Some(pack) = view.0.motion {
                let frames = Frames::open(pack).unwrap();
                assert_eq!(frames.at(0).unwrap(), view.0.poster);
                assert_eq!(frames.index(frames.count, false), frames.count - 2);
                assert_eq!(frames.index(2 * (frames.count - 1), false), 0);
                for i in 0..frames.count {
                    assert!(frames.at(i).unwrap().starts_with(&[0xff, 0xd8]));
                }
                assert!(decode(frames.at(frames.count - 1).unwrap()).is_some());
            }
        }
    }
}
