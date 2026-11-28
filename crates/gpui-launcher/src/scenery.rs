// SPDX-License-Identifier: MPL-2.0
mod motion;
mod package;
mod review_video;
use gpui::{App, Context, ObjectFit, RenderImage, Window, div, img, prelude::*, rgb};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[cfg(windows)]
mod hi_res {
    use std::sync::Arc;
    use std::time::Duration;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Threading::{
        CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, CreateEventW, CreateWaitableTimerExW, INFINITE,
        SetEvent, SetWaitableTimer, TIMER_ALL_ACCESS, WaitForMultipleObjects,
    };

    struct OwnedHandle(HANDLE);
    unsafe impl Send for OwnedHandle {}
    unsafe impl Sync for OwnedHandle {}

    impl OwnedHandle {
        fn raw(&self) -> HANDLE {
            self.0
        }
    }

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    pub(super) struct Interrupt {
        tx: async_channel::Sender<()>,
        event: Option<Arc<OwnedHandle>>,
    }

    impl Interrupt {
        pub(super) fn pair() -> (Self, CancelRx) {
            let (tx, rx) = async_channel::bounded::<()>(1);
            let event = unsafe { CreateEventW(None, true, false, None) }
                .ok()
                .map(|h| Arc::new(OwnedHandle(h)));
            let handle_for_rx = event.clone();
            (
                Self { tx, event },
                CancelRx {
                    rx,
                    event: handle_for_rx,
                },
            )
        }
        fn raw_ref(&self) -> Option<HANDLE> {
            self.event.as_ref().map(|e| e.raw())
        }
    }

    impl Drop for Interrupt {
        fn drop(&mut self) {
            let _ = self.tx.try_send(());
            if let Some(event) = self.raw_ref() {
                unsafe {
                    let _ = SetEvent(event);
                }
            }
        }
    }

    pub(super) struct CancelRx {
        rx: async_channel::Receiver<()>,
        event: Option<Arc<OwnedHandle>>,
    }

    pub(super) async fn sleep_or_cancel(
        duration: Duration,
        cancel: CancelRx,
        executor: &gpui::BackgroundExecutor,
    ) {
        if let Some(event) = cancel.event.clone()
            && let Ok(timer) = unsafe {
                CreateWaitableTimerExW(
                    None,
                    None,
                    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                    TIMER_ALL_ACCESS.0,
                )
            }
            .map(OwnedHandle)
        {
            let rx_fallback = cancel.rx;
            let ok = executor
                .spawn(async move {
                    let due_100ns =
                        -((duration.as_nanos() / 100).max(1).min(i64::MAX as u128) as i64);
                    let set =
                        unsafe { SetWaitableTimer(timer.raw(), &due_100ns, 0, None, None, false) };
                    if set.is_err() {
                        return false;
                    }
                    unsafe {
                        let _ =
                            WaitForMultipleObjects(&[timer.raw(), event.raw()], false, INFINITE);
                    }
                    true
                })
                .await;
            if ok {
                return;
            }
            let _ = futures::future::select(
                Box::pin(executor.timer(duration)),
                Box::pin(rx_fallback.recv()),
            )
            .await;
            return;
        }
        let _ = futures::future::select(
            Box::pin(executor.timer(duration)),
            Box::pin(cancel.rx.recv()),
        )
        .await;
    }
}

#[cfg(not(windows))]
mod hi_res {
    use std::time::Duration;

    pub(super) struct Interrupt(async_channel::Sender<()>);

    impl Interrupt {
        pub(super) fn pair() -> (Self, CancelRx) {
            let (tx, rx) = async_channel::bounded::<()>(1);
            (Self(tx), CancelRx(rx))
        }
    }

    impl Drop for Interrupt {
        fn drop(&mut self) {
            let _ = self.0.try_send(());
        }
    }

    pub(super) struct CancelRx(async_channel::Receiver<()>);

    pub(super) async fn sleep_or_cancel(
        duration: Duration,
        cancel: CancelRx,
        executor: &gpui::BackgroundExecutor,
    ) {
        let _ = futures::future::select(
            Box::pin(executor.timer(duration)),
            Box::pin(cancel.0.recv()),
        )
        .await;
    }
}

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
    /// Short world source attribution string without shader/mod capture details.
    pub fn source_credit(self) -> &'static str {
        self.0
            .attribution
            .split(". Rendered in ")
            .next()
            .unwrap_or(self.0.attribution)
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

/// Downsamples, applies multi-pass frosted Gaussian blur and saturation vibrance,
/// and pads a 1-texel clamped border so bilinear GPU upscaling has no transparent seams.
pub fn blur_backdrop(source: &RenderImage) -> Option<Arc<RenderImage>> {
    let size = source.size(0);
    let sw = u32::from(size.width) as usize;
    let sh = u32::from(size.height) as usize;
    let src = source.as_bytes(0)?;
    if sw == 0 || sh == 0 || src.len() < sw * sh * 4 {
        return None;
    }
    let bw = 120usize.min(sw);
    let bh = ((bw * sh) / sw).max(1);
    let n = bw * bh;
    let (mut b, mut g, mut r) = (vec![0i32; n], vec![0i32; n], vec![0i32; n]);
    for y in 0..bh {
        let sy0 = ((y * 4 + 1) * sh / (bh * 4)).min(sh - 1);
        let sy1 = ((y * 4 + 3) * sh / (bh * 4)).min(sh - 1);
        let (row0, row1) = (sy0 * sw, sy1 * sw);
        for x in 0..bw {
            let sx0 = ((x * 4 + 1) * sw / (bw * 4)).min(sw - 1);
            let sx1 = ((x * 4 + 3) * sw / (bw * 4)).min(sw - 1);
            let (mut sb, mut sg, mut sr) = (0i32, 0i32, 0i32);
            for p in [row0 + sx0, row0 + sx1, row1 + sx0, row1 + sx1] {
                let off = p * 4;
                sb += i32::from(src[off]);
                sg += i32::from(src[off + 1]);
                sr += i32::from(src[off + 2]);
            }
            let out = y * bw + x;
            b[out] = (sb + 2) >> 2;
            g[out] = (sg + 2) >> 2;
            r[out] = (sr + 2) >> 2;
        }
    }
    let (mut tmp_b, mut tmp_g, mut tmp_r) = (vec![0i32; n], vec![0i32; n], vec![0i32; n]);
    for _ in 0..2 {
        box_blur_h(&b, &mut tmp_b, bw, bh, 3);
        box_blur_h(&g, &mut tmp_g, bw, bh, 3);
        box_blur_h(&r, &mut tmp_r, bw, bh, 3);
        box_blur_v(&tmp_b, &mut b, bw, bh, 3);
        box_blur_v(&tmp_g, &mut g, bw, bh, 3);
        box_blur_v(&tmp_r, &mut r, bw, bh, 3);
    }
    let (pw, ph) = (bw + 2, bh + 2);
    let mut out = vec![255u8; pw * ph * 4];
    for y in 0..bh {
        for x in 0..bw {
            let i = y * bw + x;
            let (cb, cg, cr) = (b[i], g[i], r[i]);
            let luma = (218 * cr + 732 * cg + 74 * cb + 512) >> 10;
            let dst = ((y + 1) * pw + (x + 1)) * 4;
            out[dst] = (luma + (((cb - luma) * 145 + 50) / 100)).clamp(0, 255) as u8;
            out[dst + 1] = (luma + (((cg - luma) * 145 + 50) / 100)).clamp(0, 255) as u8;
            out[dst + 2] = (luma + (((cr - luma) * 145 + 50) / 100)).clamp(0, 255) as u8;
        }
    }
    let stride = pw * 4;
    for y in 1..=bh {
        let row = y * stride;
        out.copy_within(row + 4..row + 8, row);
        let last = row + bw * 4;
        out.copy_within(last..last + 4, last + 4);
    }
    out.copy_within(stride..stride * 2, 0);
    out.copy_within(bh * stride..(bh + 1) * stride, (bh + 1) * stride);
    let img = image::RgbaImage::from_raw(pw as u32, ph as u32, out)?;
    Some(Arc::new(RenderImage::new([image::Frame::new(img)])))
}

fn box_blur_h(src: &[i32], dst: &mut [i32], w: usize, h: usize, radius: usize) {
    let div = (radius * 2 + 1) as i32;
    let half = div / 2;
    for y in 0..h {
        let row = y * w;
        let mut sum = src[row] * (radius as i32 + 1);
        for dx in 1..=radius {
            sum += src[row + dx.min(w - 1)];
        }
        for x in 0..w {
            dst[row + x] = (sum + half) / div;
            let left = src[row + x.saturating_sub(radius)];
            let right = src[row + (x + radius + 1).min(w - 1)];
            sum += right - left;
        }
    }
}

fn box_blur_v(src: &[i32], dst: &mut [i32], w: usize, h: usize, radius: usize) {
    let div = (radius * 2 + 1) as i32;
    let half = div / 2;
    for x in 0..w {
        let mut sum = src[x] * (radius as i32 + 1);
        for dy in 1..=radius {
            sum += src[dy.min(h - 1) * w + x];
        }
        for y in 0..h {
            dst[y * w + x] = (sum + half) / div;
            let top = src[y.saturating_sub(radius) * w + x];
            let bottom = src[(y + radius + 1).min(h - 1) * w + x];
            sum += bottom - top;
        }
    }
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

#[derive(Default)]
struct Decoded {
    image: Option<Arc<RenderImage>>,
    blurred: Option<Arc<RenderImage>>,
    video: Option<review_video::Reader>,
    video_failed: bool,
    movie: Option<motion::Reader>,
    movie_failed: bool,
}

pub struct SceneView {
    landscape: Landscape,
    video: Option<review_video::Reader>,
    decoder_cancel: review_video::Cancellation,
    video_failed: bool,
    movie: Option<motion::Reader>,
    movie_failed: bool,
    image: Option<Arc<RenderImage>>,
    blurred: Option<Arc<RenderImage>>,
    viewport_px: (u32, u32),
    step: usize,
    poster_pending: bool,
    interrupt: Option<hi_res::Interrupt>,
    next_due: Option<Instant>,
    playing: bool,
    busy: bool,
    motion_failed: bool,
    poster_failed: bool,
    generation: u64,
}

impl SceneView {
    pub fn new(landscape: Landscape, initial_viewport: (u32, u32), cx: &mut Context<Self>) -> Self {
        cx.on_release(|view, app| {
            view.decoder_cancel.cancel();
            retire(view.image.take(), app);
            retire(view.blurred.take(), app);
        })
        .detach();
        let mut view = Self {
            landscape,
            video: None,
            decoder_cancel: Default::default(),
            video_failed: false,
            movie: None,
            movie_failed: false,
            image: None,
            blurred: None,
            viewport_px: initial_viewport,
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

    pub fn set_viewport(&mut self, viewport: (u32, u32)) {
        if viewport.0 > 0 && viewport.1 > 0 {
            self.viewport_px = viewport;
        }
    }

    pub fn blurred_image(&self) -> Option<Arc<RenderImage>> {
        self.blurred.clone()
    }
    pub fn set_landscape(&mut self, landscape: Landscape, cx: &mut Context<Self>) {
        if self.landscape == landscape {
            return;
        }
        self.interrupt.take();
        self.generation = self.generation.wrapping_add(1);
        std::mem::take(&mut self.decoder_cancel).cancel();
        self.video.take();
        self.video_failed = false;
        self.movie.take();
        self.movie_failed = false;
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
        std::mem::take(&mut self.decoder_cancel).cancel();
        self.video.take();
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
        // A view that has a packaged movie never falls through to the local
        // FFmpeg experiment, even after a packaged decode failure.
        let packaged = package::discover().and_then(|package| package.view(scene.id));
        let movie = packaged.filter(|_| !self.movie_failed);
        let video_source = if packaged.is_some() {
            None
        } else {
            review_video::Source::configured().filter(|_| !self.video_failed)
        };
        let fps = packaged.map_or_else(
            || {
                if video_source.is_some() {
                    30
                } else {
                    scene.fps
                }
            },
            |movie| movie.fps,
        );
        let (bytes, due, step) = if poster {
            (scene.poster, now, 0)
        } else {
            if !self.playing || self.motion_failed {
                return;
            }
            let Some(pack) = scene.motion else { return };
            let period = Duration::from_secs_f64(1. / f64::from(fps));
            let (due, advance) = frame_deadline(self.next_due.unwrap_or(now + period), now, period);
            let step = self.step.wrapping_add(advance);
            let Some(bytes) = Frames::open(pack).and_then(|f| f.at(f.index(step, scene.forward)))
            else {
                self.fallback(cx);
                return;
            };
            (bytes, due, step)
        };
        let previous_reader = self.video.take();
        let previous_movie = self.movie.take();
        let target_size =
            (self.viewport_px.0 > 0 && self.viewport_px.1 > 0).then_some(self.viewport_px);
        let refresh_blur = poster || step.is_multiple_of(3);
        let decoder_cancel = self.decoder_cancel.clone();
        let generation = self.generation;
        let (interrupt, interrupted) = hi_res::Interrupt::pair();
        self.interrupt = Some(interrupt);
        self.busy = true;
        cx.spawn(async move |this, cx| {
            let timeout_cancel = decoder_cancel.clone();
            let decode_job = cx.background_executor().spawn(async move {
                let mut decoded = Decoded::default();
                if let Some(movie) = movie {
                    if poster {
                        decoded.image = movie.poster().or_else(|| decode(bytes));
                        decoded.blurred = decoded.image.as_deref().and_then(blur_backdrop);
                        return decoded;
                    }
                    match motion::frame(previous_movie, movie, step, target_size) {
                        Ok((reader, image)) => {
                            if refresh_blur {
                                decoded.blurred = blur_backdrop(&image);
                            }
                            decoded.image = Some(image);
                            decoded.movie = Some(reader);
                        }
                        Err(error) => {
                            eprintln!(
                                "Packaged scenery {}: {}; using bundled playback",
                                scene.id, error
                            );
                            decoded.image = decode(bytes);
                            if refresh_blur {
                                decoded.blurred = decoded.image.as_deref().and_then(blur_backdrop);
                            }
                            decoded.movie_failed = true;
                        }
                    }
                    return decoded;
                }
                if let Some(source) = video_source {
                    if poster {
                        decoded.image = source.poster(scene.id).or_else(|| decode(bytes));
                        decoded.blurred = decoded.image.as_deref().and_then(blur_backdrop);
                        return decoded;
                    }
                    match review_video::Reader::frame(
                        previous_reader,
                        decoder_cancel,
                        source,
                        scene.id,
                        step,
                    ) {
                        Ok((reader, image)) => {
                            if refresh_blur {
                                decoded.blurred = blur_backdrop(&image);
                            }
                            decoded.image = Some(image);
                            decoded.video = Some(reader);
                        }
                        Err(err) => {
                            eprintln!(
                                "H.264 scenery {}: {}; using bundled playback",
                                scene.id, err
                            );
                            decoded.image = decode(bytes);
                            if refresh_blur {
                                decoded.blurred = decoded.image.as_deref().and_then(blur_backdrop);
                            }
                            decoded.video_failed = true;
                        }
                    }
                    return decoded;
                }
                decoded.image = decode(bytes);
                if refresh_blur {
                    decoded.blurred = decoded.image.as_deref().and_then(blur_backdrop);
                }
                decoded
            });
            let decoded = if video_source.is_some() && !poster {
                let timeout = cx.background_executor().timer(Duration::from_secs(3));
                match futures::future::select(decode_job, Box::pin(timeout)).await {
                    futures::future::Either::Left((result, _)) => result,
                    futures::future::Either::Right((_, pending)) => {
                        timeout_cancel.cancel();
                        pending.await
                    }
                }
            } else {
                decode_job.await
            };
            let Decoded {
                image: decoded,
                blurred,
                video: reader,
                video_failed,
                movie: movie_reader,
                movie_failed,
            } = decoded;
            let remaining = due.saturating_duration_since(Instant::now());
            if !remaining.is_zero() {
                hi_res::sleep_or_cancel(remaining, interrupted, cx.background_executor()).await;
            }
            let _ = this.update(cx, |view, cx| {
                view.busy = false;
                view.interrupt.take();
                // Keep the warm decoder across pause and resume. A packaged
                // movie has one keyframe per loop, so reopening mid-cycle would
                // decode from the start again.
                view.movie = movie_reader;
                if view.generation == generation {
                    view.video = reader;
                    view.video_failed |= video_failed;
                    view.movie_failed |= movie_failed;
                    if let Some(image) = decoded {
                        retire(view.image.replace(image), cx);
                        if let Some(blur) = blurred {
                            retire(view.blurred.replace(blur), cx);
                        }
                        view.poster_pending = false;
                        if !poster {
                            let period = Duration::from_secs_f64(1. / f64::from(fps));
                            let (phase, next) = after_present(step, due, Instant::now(), period);
                            view.step = phase;
                            view.next_due = Some(next);
                        }
                        cx.notify();
                    } else if poster {
                        view.poster_failed = true;
                        retire(view.image.take(), cx);
                        retire(view.blurred.take(), cx);
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
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let scale = window.scale_factor();
        let viewport = window.viewport_size();
        self.viewport_px = (
            (f32::from(viewport.width) * scale).round().max(0.) as u32,
            (f32::from(viewport.height) * scale).round().max(0.) as u32,
        );
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
    #[ignore = "release benchmark comparing three JPEG color conversion paths"]
    fn compare_bundled_decode_paths() {
        use std::hint::black_box;
        use zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions};

        let mut scenes = Landscape::ALL.to_vec();
        scenes.sort_by_key(|scene| scene.id());
        let mut frames = Vec::new();
        for scene in scenes {
            if let Some(bytes) = scene.0.motion {
                let pack = Frames::open(bytes).unwrap();
                frames.extend([pack.at(0).unwrap(), pack.at(pack.count / 2).unwrap()]);
            }
        }
        assert!(!frames.is_empty());

        let decode_path = |jpeg: &[u8], mode: Option<ColorSpace>| {
            let mut pixels = match mode {
                None => image::load_from_memory_with_format(jpeg, image::ImageFormat::Jpeg)
                    .unwrap()
                    .into_rgba8()
                    .into_raw(),
                Some(color) => zune_jpeg::JpegDecoder::new_with_options(
                    ZCursor::new(jpeg),
                    DecoderOptions::default().jpeg_set_out_colorspace(color),
                )
                .decode()
                .unwrap(),
            };
            if mode != Some(ColorSpace::BGRA) {
                for pixel in pixels.as_chunks_mut::<4>().0 {
                    pixel.swap(0, 2);
                }
            }
            pixels
        };
        let paths = [None, Some(ColorSpace::RGBA), Some(ColorSpace::BGRA)];
        let mut samples = [Vec::new(), Vec::new(), Vec::new()];
        for repetition in 0..=15 {
            for (index, jpeg) in frames.iter().enumerate() {
                for offset in 0..paths.len() {
                    let path = (offset + repetition + index) % paths.len();
                    let start = Instant::now();
                    let pixels = decode_path(black_box(jpeg), paths[path]);
                    let elapsed = start.elapsed().as_micros();
                    if repetition == 0 {
                        assert_eq!(pixels, decode_path(jpeg, None));
                        assert!(
                            pixels
                                .as_chunks::<4>()
                                .0
                                .iter()
                                .all(|pixel| pixel[3] == 255)
                        );
                    } else {
                        samples[path].push(elapsed);
                    }
                    black_box(pixels);
                }
            }
        }
        for (label, mut times) in ["image RGBA + swap", "Zune RGBA + swap", "Zune BGRA"]
            .into_iter()
            .zip(samples)
        {
            times.sort_unstable();
            println!(
                "{label}: {} samples, median {} us, p95 {} us",
                times.len(),
                times[times.len() / 2],
                times[(times.len() * 95).div_ceil(100) - 1]
            );
        }
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
    #[test]
    fn blur_backdrop_smooths_edges_and_pads_clamped_border() {
        let mut raw = vec![0u8; 32 * 16 * 4];
        for y in 0..16 {
            for x in 0..32 {
                let off = (y * 32 + x) * 4;
                if x >= 16 {
                    raw[off..off + 4].copy_from_slice(&[240, 160, 80, 255]);
                } else {
                    raw[off + 3] = 255;
                }
            }
        }
        let img = image::RgbaImage::from_raw(32, 16, raw).unwrap();
        let source = RenderImage::new([image::Frame::new(img)]);
        let blurred = blur_backdrop(&source).unwrap();
        let size = blurred.size(0);
        assert_eq!((u32::from(size.width), u32::from(size.height)), (34, 18));
        let bytes = blurred.as_bytes(0).unwrap();
        let px = |x: usize, y: usize| -> &[u8] {
            let off = (y * 34 + x) * 4;
            &bytes[off..off + 4]
        };
        assert!(bytes.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
        assert_eq!(px(0, 8), px(1, 8));
        assert_eq!(px(33, 8), px(32, 8));
        assert_eq!(px(17, 0), px(17, 1));
        assert_eq!(px(17, 17), px(17, 16));
        assert!(px(17, 8)[0] > 40 && px(17, 8)[0] < 200);
    }
}
