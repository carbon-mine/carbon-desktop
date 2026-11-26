// SPDX-License-Identifier: MPL-2.0
use gpui::{App, Context, ObjectFit, RenderImage, Window, div, img, prelude::*, rgb};
use std::{sync::Arc, time::Duration};

struct Scene {
    id: &'static str,
    family: &'static str,
    name: &'static str,
    attribution: &'static str,
    poster: &'static [u8],
    motion: Option<&'static [u8]>,
    fps: u8,
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
    fn index(&self, step: usize) -> usize {
        let cycle = (self.count - 1) * 2;
        let phase = step % cycle;
        phase.min(cycle - phase)
    }
}

fn decode(jpeg: &[u8]) -> Option<Arc<RenderImage>> {
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

pub struct SceneView {
    landscape: Landscape,
    image: Option<Arc<RenderImage>>,
    step: usize,
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
        self.generation = self.generation.wrapping_add(1);
        self.landscape = landscape;
        self.motion_failed = false;
        self.poster_failed = false;
        self.step = 0;
        retire(self.image.take(), cx);
        self.schedule(cx);
        cx.notify();
    }
    pub fn set_playing(&mut self, playing: bool, cx: &mut Context<Self>) {
        if self.playing == playing {
            return;
        }
        self.generation = self.generation.wrapping_add(1);
        self.playing = playing;
        self.schedule(cx);
    }
    fn schedule(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.poster_failed {
            return;
        }
        let scene = self.landscape.0;
        let poster = self.image.is_none();
        let (bytes, delay) = if poster {
            (scene.poster, Duration::ZERO)
        } else {
            if !self.playing || self.motion_failed {
                return;
            }
            let Some(pack) = scene.motion else { return };
            let frame = Frames::open(pack).and_then(|f| f.at(f.index(self.step.wrapping_add(1))));
            let Some(bytes) = frame else {
                self.fallback(cx);
                return;
            };
            (bytes, Duration::from_secs_f64(1. / f64::from(scene.fps)))
        };
        let generation = self.generation;
        self.busy = true;
        cx.spawn(async move |this, cx| {
            if !delay.is_zero() {
                cx.background_executor().timer(delay).await;
            }
            let current = this
                .read_with(cx, |view, _| view.generation == generation)
                .unwrap_or(false);
            let decoded = if current {
                cx.background_executor()
                    .spawn(async move { decode(bytes) })
                    .await
            } else {
                None
            };
            let _ = this.update(cx, |view, cx| {
                view.busy = false;
                if view.generation == generation {
                    if let Some(image) = decoded {
                        retire(view.image.replace(image), cx);
                        if !poster {
                            view.step = view.step.wrapping_add(1)
                        }
                        cx.notify();
                    } else if poster {
                        view.poster_failed = true;
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
        retire(self.image.take(), cx);
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
        assert!(decode(extra.0.poster).is_some());
        let frames = Frames::open(extra.0.motion.unwrap()).unwrap();
        assert!(decode(frames.at(frames.index(2)).unwrap()).is_some());
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
                assert_eq!(frames.index(frames.count), frames.count - 2);
                assert_eq!(frames.index(2 * (frames.count - 1)), 0);
                for i in 0..frames.count {
                    assert!(frames.at(i).unwrap().starts_with(&[0xff, 0xd8]));
                }
                assert!(decode(frames.at(frames.count - 1).unwrap()).is_some());
            }
        }
    }
}
