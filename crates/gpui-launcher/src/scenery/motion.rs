// SPDX-License-Identifier: MPL-2.0
//! In-process decode of packaged Home scenery motion.
//!
//! Windows uses the operating system Media Foundation decoder, so an installer
//! ships no codec library of its own. Platforms without a qualified native
//! decoder keep the bundled frame packs until one is verified there.
use super::package::Movie;
use gpui::RenderImage;
use std::sync::Arc;

pub(super) struct Reader(mf::Reader);

// Media Foundation readers are created and used inside a multi-threaded
// apartment, so the source reader may move between executor threads.
unsafe impl Send for Reader {}

/// Returns the frame at `step` on the packaged movie's continuous timeline.
/// Consecutive steps reuse one decoder; a rewind or a different view reopens it.
pub(super) fn frame(
    previous: Option<Reader>,
    movie: &Movie,
    step: usize,
    target: Option<(u32, u32)>,
) -> Result<(Reader, Arc<RenderImage>), String> {
    let mut reader = match previous {
        Some(Reader(reader)) if reader.matches(movie) => reader,
        _ => mf::Reader::open(movie, step, target)?,
    };
    reader.set_target(target);
    // A stale in-flight decode can leave the reader up to three frames ahead of
    // the resumed step; present the next frame rather than rewinding.
    if reader.next() > step + 3 {
        reader = mf::Reader::open(movie, step, target)?;
    }
    let mut image = None;
    // Present the newest frame and skip at most two stale slots, matching the
    // bundled frame-pacing budget after a stall.
    for _ in 0..(step + 1).saturating_sub(reader.next()).clamp(1, 3) {
        image = Some(reader.read()?);
    }
    Ok((
        Reader(reader),
        image.ok_or("Packaged scenery produced no frame")?,
    ))
}

#[cfg(windows)]
mod mf {
    use super::super::package::Movie;
    use gpui::RenderImage;
    use std::{
        ffi::c_void,
        path::{Path, PathBuf},
        sync::{
            Arc, Condvar, Mutex, OnceLock,
            atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
            mpsc,
        },
        thread::JoinHandle,
        time::Duration,
    };
    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::Media::MediaFoundation::{
        IMFAttributes, IMFMediaType, IMFSample, IMFSourceReader, MF_MT_DEFAULT_STRIDE,
        MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS,
        MF_SOURCE_READER_ALL_STREAMS, MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING,
        MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_SOURCE_READERF_ENDOFSTREAM,
        MF_SOURCE_READERF_ERROR, MF_VERSION, MFMediaType_Video, MFSTARTUP_LITE, MFVideoFormat_NV12,
        MFVideoFormat_RGB32,
    };
    use windows::Win32::System::Com::{
        COINIT_MULTITHREADED, CoInitializeEx, StructuredStorage::PROPVARIANT,
    };
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    use windows::Win32::System::Variant::VT_I8;
    use windows::core::{GUID, HRESULT, HSTRING, Interface, PCSTR, PCWSTR, w};

    const MAX_EDGE: u32 = 4096;

    #[derive(Clone, Copy)]
    enum PixelFormat {
        Nv12,
        Rgb32,
    }

    /// Media Foundation is resolved at runtime instead of through static
    /// imports so a Windows N image without the Media Feature Pack still
    /// starts the launcher and falls back to bundled artwork.
    struct Api {
        startup: unsafe extern "system" fn(u32, u32) -> HRESULT,
        create_attributes: unsafe extern "system" fn(*mut Option<IMFAttributes>, u32) -> HRESULT,
        create_media_type: unsafe extern "system" fn(*mut Option<IMFMediaType>) -> HRESULT,
        create_reader_from_url: unsafe extern "system" fn(
            PCWSTR,
            *const c_void,
            *mut Option<IMFSourceReader>,
        ) -> HRESULT,
    }

    fn api() -> Result<&'static Api, String> {
        static API: OnceLock<Result<Api, String>> = OnceLock::new();
        API.get_or_init(|| unsafe {
            let platform = LoadLibraryW(w!("mfplat.dll"))
                .map_err(|error| format!("Media Foundation is unavailable ({error})"))?;
            let reader = LoadLibraryW(w!("mfreadwrite.dll"))
                .map_err(|error| format!("Media Foundation reading is unavailable ({error})"))?;
            Ok(Api {
                startup: symbol(platform, b"MFStartup\0")?,
                create_attributes: symbol(platform, b"MFCreateAttributes\0")?,
                create_media_type: symbol(platform, b"MFCreateMediaType\0")?,
                create_reader_from_url: symbol(reader, b"MFCreateSourceReaderFromURL\0")?,
            })
        })
        .as_ref()
        .map_err(Clone::clone)
    }

    unsafe fn symbol<T: Copy>(module: HMODULE, name: &'static [u8]) -> Result<T, String> {
        let address = unsafe { GetProcAddress(module, PCSTR(name.as_ptr())) }
            .ok_or_else(|| format!("Media Foundation entry point {:?} is missing", name))?;
        Ok(unsafe { std::mem::transmute_copy(&address) })
    }

    fn pack_target(target: Option<(u32, u32)>) -> u64 {
        target.map_or(0, |(w, h)| (u64::from(w) << 32) | u64::from(h))
    }

    fn unpack_target(packed: u64) -> Option<(u32, u32)> {
        (packed != 0).then_some(((packed >> 32) as u32, packed as u32))
    }

    /// Computes even dimensions preserving the source aspect ratio for cover display,
    /// avoiding uploading 1440p textures when the fixed viewport is smaller.
    fn cover_dimensions(src_w: u32, src_h: u32, target: Option<(u32, u32)>) -> (u32, u32) {
        let Some((win_w, win_h)) = target.filter(|&(w, h)| w > 0 && h > 0) else {
            return (src_w, src_h);
        };
        let scale = (win_w as f32 / src_w as f32)
            .max(win_h as f32 / src_h as f32)
            .min(1.0);
        if scale >= 0.98 {
            return (src_w, src_h);
        }
        let dst_w = ((src_w as f32 * scale).round() as u32).clamp(64, src_w) & !1;
        let dst_h = ((src_h as f32 * scale).round() as u32).clamp(64, src_h) & !1;
        (dst_w.max(64), dst_h.max(64))
    }

    struct ScaleLut {
        src_w: usize,
        src_h: usize,
        dst_w: usize,
        dst_h: usize,
        sx0: Vec<usize>,
        sx1: Vec<usize>,
        ux: Vec<usize>,
    }

    impl ScaleLut {
        fn new(src_w: usize, src_h: usize, dst_w: usize, dst_h: usize) -> Self {
            let mut sx0 = Vec::with_capacity(dst_w);
            let mut sx1 = Vec::with_capacity(dst_w);
            let mut ux = Vec::with_capacity(dst_w);
            for px in 0..dst_w {
                let x0 = ((px * src_w * 2 + src_w) / (dst_w * 2)).min(src_w - 1);
                sx0.push(x0);
                sx1.push((x0 + 1).min(src_w - 1));
                ux.push((x0 & !1).min(src_w - 2));
            }
            Self {
                src_w,
                src_h,
                dst_w,
                dst_h,
                sx0,
                sx1,
                ux,
            }
        }
    }

    /// Pipelined Media Foundation reader that decodes NV12 on a dedicated background
    /// thread ahead of presentation so UI cadence never stalls on YUV conversion or wraparound.
    pub(super) struct Reader {
        path: PathBuf,
        next: usize,
        target: Arc<AtomicU64>,
        stop: Arc<AtomicBool>,
        rx: Option<mpsc::Receiver<Result<Arc<RenderImage>, String>>>,
        worker: Option<JoinHandle<()>>,
    }

    impl Reader {
        pub(super) fn open(
            movie: &Movie,
            step: usize,
            target_size: Option<(u32, u32)>,
        ) -> Result<Self, String> {
            let path = movie.motion().to_owned();
            let frames = (movie.frames as usize).max(1);
            let target = Arc::new(AtomicU64::new(pack_target(target_size)));
            let stop = Arc::new(AtomicBool::new(false));
            let (tx, rx) = mpsc::sync_channel::<Result<Arc<RenderImage>, String>>(10);
            let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<(), String>>(1);

            let worker_path = path.clone();
            let worker_target = target.clone();
            let worker_stop = stop.clone();
            let skip = step % frames;

            let worker = std::thread::Builder::new()
                .name("carbon-scenic-decode".into())
                .spawn(move || {
                    decode_worker(
                        &worker_path,
                        skip,
                        &worker_target,
                        &worker_stop,
                        ready_tx,
                        tx,
                    );
                })
                .map_err(|error| format!("Cannot start scenery decode worker: {error}"))?;

            match ready_rx.recv() {
                Ok(Ok(())) => Ok(Self {
                    path,
                    next: step,
                    target,
                    stop,
                    rx: Some(rx),
                    worker: Some(worker),
                }),
                Ok(Err(error)) => {
                    stop.store(true, Ordering::Release);
                    drop(rx);
                    let _ = worker.join();
                    Err(error)
                }
                Err(_) => {
                    stop.store(true, Ordering::Release);
                    drop(rx);
                    let _ = worker.join();
                    Err("Scenery decode worker stopped during startup".into())
                }
            }
        }

        pub(super) fn matches(&self, movie: &Movie) -> bool {
            movie.motion() == self.path
        }

        pub(super) fn set_target(&self, target: Option<(u32, u32)>) {
            self.target.store(pack_target(target), Ordering::Relaxed);
        }

        pub(super) fn next(&self) -> usize {
            self.next
        }

        pub(super) fn read(&mut self) -> Result<Arc<RenderImage>, String> {
            let rx = self
                .rx
                .as_ref()
                .ok_or_else(|| "Scenery decoder was already closed".to_string())?;
            let image = rx
                .recv_timeout(Duration::from_secs(3))
                .map_err(|_| "Scenery decode worker timed out or disconnected".to_string())??;
            self.next += 1;
            Ok(image)
        }
    }

    impl Drop for Reader {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            drop(self.rx.take());
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }

    const BANDS: usize = 4;

    struct Nv12Frame<'a> {
        y_plane: &'a [u8],
        uv_plane: &'a [u8],
        pitch: usize,
        width: usize,
        height: usize,
    }

    #[derive(Clone, Copy)]
    struct BandTask {
        seq: u64,
        y_ptr: usize,
        y_len: usize,
        uv_ptr: usize,
        uv_len: usize,
        pitch: usize,
        w: usize,
        h: usize,
        dst_ptr: usize,
        lut_ptr: usize,
    }

    struct RowShared {
        work: Mutex<(bool, Option<BandTask>)>,
        work_cv: Condvar,
        pending: AtomicUsize,
        done_mu: Mutex<()>,
        done_cv: Condvar,
    }

    struct RowPool {
        shared: Arc<RowShared>,
        workers: Vec<JoinHandle<()>>,
    }

    impl RowPool {
        fn new() -> Self {
            let shared = Arc::new(RowShared {
                work: Mutex::new((false, None)),
                work_cv: Condvar::new(),
                pending: AtomicUsize::new(0),
                done_mu: Mutex::new(()),
                done_cv: Condvar::new(),
            });
            let mut workers = Vec::with_capacity(BANDS - 1);
            for tid in 1..BANDS {
                let s = shared.clone();
                workers.push(
                    std::thread::Builder::new()
                        .name("carbon-scenic-row".into())
                        .spawn(move || {
                            let mut seen = 0u64;
                            loop {
                                let task = {
                                    let mut guard = s.work.lock().expect("row pool lock");
                                    loop {
                                        if guard.0 {
                                            return;
                                        }
                                        if let Some(task) = guard.1
                                            && task.seq != seen
                                        {
                                            break task;
                                        }
                                        guard = s.work_cv.wait(guard).expect("row pool wait");
                                    }
                                };
                                seen = task.seq;
                                unsafe { Self::run_slice(&task, tid) };
                                if s.pending.fetch_sub(1, Ordering::AcqRel) == 1 {
                                    let _done = s.done_mu.lock().expect("row done lock");
                                    s.done_cv.notify_one();
                                }
                            }
                        })
                        .expect("spawn scenic row worker"),
                );
            }
            Self { shared, workers }
        }

        unsafe fn run_slice(task: &BandTask, tid: usize) {
            let src = Nv12Frame {
                y_plane: unsafe { std::slice::from_raw_parts(task.y_ptr as *const u8, task.y_len) },
                uv_plane: unsafe {
                    std::slice::from_raw_parts(task.uv_ptr as *const u8, task.uv_len)
                },
                pitch: task.pitch,
                width: task.w,
                height: task.h,
            };
            let lut = (task.lut_ptr != 0).then(|| unsafe { &*(task.lut_ptr as *const ScaleLut) });
            let (dst_w, dst_h) = lut.map_or((task.w, task.h), |l| (l.dst_w, l.dst_h));
            let rows = dst_h.div_ceil(BANDS);
            let y_start = (tid * rows).min(dst_h);
            let y_end = ((tid + 1) * rows).min(dst_h);
            if y_start >= y_end {
                return;
            }
            let row_bytes = dst_w * 4;
            let dst_base = unsafe { (task.dst_ptr as *mut u8).add(y_start * row_bytes) };
            let slice =
                unsafe { std::slice::from_raw_parts_mut(dst_base, (y_end - y_start) * row_bytes) };
            if let Some(lut) = lut {
                nv12_scaled_rows(&src, lut, y_start, slice);
            } else {
                nv12_full_rows(&src, y_start, slice);
            }
        }

        fn dispatch(
            &self,
            seq: &mut u64,
            src: &Nv12Frame<'_>,
            lut: Option<&ScaleLut>,
            dst: &mut [u8],
        ) {
            *seq = seq.wrapping_add(1).max(1);
            let task = BandTask {
                seq: *seq,
                y_ptr: src.y_plane.as_ptr() as usize,
                y_len: src.y_plane.len(),
                uv_ptr: src.uv_plane.as_ptr() as usize,
                uv_len: src.uv_plane.len(),
                pitch: src.pitch,
                w: src.width,
                h: src.height,
                dst_ptr: dst.as_mut_ptr() as usize,
                lut_ptr: lut.map_or(0, |l| l as *const ScaleLut as usize),
            };
            self.shared.pending.store(BANDS - 1, Ordering::Release);
            {
                let mut guard = self.shared.work.lock().expect("row pool lock");
                guard.1 = Some(task);
                self.shared.work_cv.notify_all();
            }
            unsafe { Self::run_slice(&task, 0) };
            let mut done = self.shared.done_mu.lock().expect("row done lock");
            while self.shared.pending.load(Ordering::Acquire) > 0 {
                done = self.shared.done_cv.wait(done).expect("row done wait");
            }
            let mut guard = self.shared.work.lock().expect("row pool clear");
            guard.1 = None;
        }
    }

    impl Drop for RowPool {
        fn drop(&mut self) {
            if let Ok(mut guard) = self.shared.work.lock() {
                guard.0 = true;
                self.shared.work_cv.notify_all();
            }
            for worker in self.workers.drain(..) {
                let _ = worker.join();
            }
        }
    }

    struct Decoder {
        path: PathBuf,
        width: u32,
        height: u32,
        stride: i32,
        format: PixelFormat,
        reader: IMFSourceReader,
        lut: Option<ScaleLut>,
        pool: Option<RowPool>,
        pool_seq: u64,
    }

    impl Decoder {
        fn open(path: &Path) -> Result<Self, String> {
            start()?;
            let (reader, width, height, stride, format) = source(path).map_err(|error| {
                format!("Media Foundation cannot open {}: {error}", path.display())
            })?;
            Ok(Self {
                path: path.to_owned(),
                width,
                height,
                stride,
                format,
                reader,
                lut: None,
                pool: None,
                pool_seq: 0,
            })
        }

        fn skip(&mut self, count: usize) -> Result<(), String> {
            for _ in 0..count {
                if self.next_sample()?.is_none() {
                    self.rewind()?;
                    if self.next_sample()?.is_none() {
                        return Err("Packaged scenery ended while skipping frames".into());
                    }
                }
            }
            Ok(())
        }

        fn read(&mut self, target: Option<(u32, u32)>) -> Result<Arc<RenderImage>, String> {
            let sample = match self.next_sample()? {
                Some(sample) => sample,
                None => {
                    self.rewind()?;
                    self.next_sample()?
                        .ok_or("Packaged scenery ended without a frame")?
                }
            };
            self.present(&sample, target)
        }

        fn next_sample(&mut self) -> Result<Option<IMFSample>, String> {
            // A stream tick carries no sample and is not the end of the movie.
            for _ in 0..8 {
                let mut flags = 0u32;
                let mut sample: Option<IMFSample> = None;
                unsafe {
                    self.reader
                        .ReadSample(
                            MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                            0,
                            None,
                            Some(&mut flags),
                            None,
                            Some(&mut sample),
                        )
                        .map_err(|error| format!("Media Foundation read failed: {error}"))?;
                }
                if flags & MF_SOURCE_READERF_ERROR.0 as u32 != 0 {
                    return Err("Media Foundation reported a decode error".into());
                }
                if sample.is_some() {
                    return Ok(sample);
                }
                if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                    return Ok(None);
                }
            }
            Err("Media Foundation produced no frame".into())
        }

        fn rewind(&mut self) -> Result<(), String> {
            let mut pos = PROPVARIANT::default();
            unsafe {
                let inner = &mut *pos.Anonymous.Anonymous;
                inner.vt = VT_I8;
                inner.Anonymous.hVal = 0;
                if self
                    .reader
                    .Flush(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32)
                    .is_ok()
                    && self
                        .reader
                        .SetCurrentPosition(&GUID::zeroed(), &pos)
                        .is_ok()
                {
                    return Ok(());
                }
            }
            let (reader, width, height, stride, format) = source(&self.path).map_err(|error| {
                format!(
                    "Media Foundation cannot reopen {}: {error}",
                    self.path.display()
                )
            })?;
            self.reader = reader;
            self.width = width;
            self.height = height;
            self.stride = stride;
            self.format = format;
            Ok(())
        }

        fn present(
            &mut self,
            sample: &IMFSample,
            target: Option<(u32, u32)>,
        ) -> Result<Arc<RenderImage>, String> {
            let buffer = unsafe { sample.ConvertToContiguousBuffer() }
                .map_err(|error| format!("Media Foundation buffer failed: {error}"))?;
            let mut data = std::ptr::null_mut();
            let mut length = 0;
            unsafe {
                buffer
                    .Lock(&mut data, None, Some(&mut length))
                    .map_err(|error| format!("Media Foundation lock failed: {error}"))?;
            }
            let copied = match self.format {
                PixelFormat::Nv12 => {
                    let (dst_w, dst_h) = cover_dimensions(self.width, self.height, target);
                    let src_w = self.width as usize;
                    let src_h = self.height as usize;
                    if dst_h >= 256 && self.pool.is_none() {
                        self.pool = Some(RowPool::new());
                    }
                    let src = nv12_planes(data, length, self.width, self.height, self.stride);
                    match src {
                        Ok(src) if dst_w == self.width && dst_h == self.height => {
                            self.lut = None;
                            nv12_full_to_bgra(
                                &src,
                                self.pool.as_ref().zip(Some(&mut self.pool_seq)),
                            )
                        }
                        Ok(src) => {
                            let dw = dst_w as usize;
                            let dh = dst_h as usize;
                            let need_lut = self.lut.as_ref().is_none_or(|l| {
                                l.src_w != src_w
                                    || l.src_h != src_h
                                    || l.dst_w != dw
                                    || l.dst_h != dh
                            });
                            if need_lut {
                                self.lut = Some(ScaleLut::new(src_w, src_h, dw, dh));
                            }
                            let lut = self.lut.as_ref().expect("ScaleLut initialized");
                            nv12_scaled_to_bgra(
                                &src,
                                lut,
                                self.pool.as_ref().zip(Some(&mut self.pool_seq)),
                            )
                        }
                        Err(err) => Err(err),
                    }
                }
                PixelFormat::Rgb32 => copy(data, length, self.width, self.height, self.stride),
            };
            unsafe {
                let _ = buffer.Unlock();
            }
            copied.map(|image| Arc::new(RenderImage::new([image::Frame::new(image)])))
        }
    }

    fn decode_worker(
        path: &Path,
        skip: usize,
        target: &AtomicU64,
        stop: &AtomicBool,
        ready_tx: mpsc::SyncSender<Result<(), String>>,
        tx: mpsc::SyncSender<Result<Arc<RenderImage>, String>>,
    ) {
        let mut decoder = match Decoder::open(path).and_then(|mut d| {
            d.skip(skip)?;
            Ok(d)
        }) {
            Ok(decoder) => {
                let _ = ready_tx.send(Ok(()));
                decoder
            }
            Err(error) => {
                let _ = ready_tx.send(Err(error));
                return;
            }
        };

        while !stop.load(Ordering::Acquire) {
            let item = decoder.read(unpack_target(target.load(Ordering::Relaxed)));
            let failed = item.is_err();
            if tx.send(item).is_err() || failed {
                break;
            }
        }
    }

    fn source(path: &Path) -> Result<(IMFSourceReader, u32, u32, i32, PixelFormat), String> {
        // Prefer direct NV12 output without Media Foundation's CPU Video Processor
        // MFT (~13x faster ReadSample); fall back to RGB32 + Video Processor only if
        // a stream refuses NV12.
        if let Ok(opened) = open_with_format(path, PixelFormat::Nv12) {
            return Ok(opened);
        }
        open_with_format(path, PixelFormat::Rgb32)
    }

    fn open_with_format(
        path: &Path,
        format: PixelFormat,
    ) -> Result<(IMFSourceReader, u32, u32, i32, PixelFormat), String> {
        let api = api()?;
        let wide = HSTRING::from(path.to_string_lossy().as_ref());
        let (subtype, video_processing) = match format {
            PixelFormat::Nv12 => (MFVideoFormat_NV12, 0u32),
            PixelFormat::Rgb32 => (MFVideoFormat_RGB32, 1u32),
        };
        unsafe {
            let mut attributes: Option<IMFAttributes> = None;
            created((api.create_attributes)(&mut attributes, 2))?;
            let attributes =
                attributes.ok_or_else(|| "Media Foundation returned no attributes".to_string())?;
            setup(
                attributes.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, video_processing),
            )?;
            setup(attributes.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1))?;
            let mut reader: Option<IMFSourceReader> = None;
            created((api.create_reader_from_url)(
                PCWSTR(wide.as_ptr()),
                attributes.as_raw(),
                &mut reader,
            ))?;
            let reader = reader.ok_or_else(|| "Media Foundation returned no reader".to_string())?;
            setup(reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false))?;
            setup(reader.SetStreamSelection(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32, true))?;
            let mut media: Option<IMFMediaType> = None;
            created((api.create_media_type)(&mut media))?;
            let media =
                media.ok_or_else(|| "Media Foundation returned no media type".to_string())?;
            setup(media.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video))?;
            setup(media.SetGUID(&MF_MT_SUBTYPE, &subtype))?;
            setup(reader.SetCurrentMediaType(
                MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                None,
                &media,
            ))?;
            let current =
                setup(reader.GetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32))?;
            let size = setup(current.GetUINT64(&MF_MT_FRAME_SIZE))?;
            let (width, height) = ((size >> 32) as u32, size as u32);
            if width < 64
                || height < 64
                || width > MAX_EDGE
                || height > MAX_EDGE
                || width % 2 != 0
                || height % 2 != 0
            {
                return Err(format!(
                    "Media Foundation reported an unsupported {width}x{height} frame"
                ));
            }
            let default_stride = match format {
                PixelFormat::Nv12 => width as i32,
                PixelFormat::Rgb32 => (width * 4) as i32,
            };
            let stride = current
                .GetUINT32(&MF_MT_DEFAULT_STRIDE)
                .map_or(default_stride, |stride| stride as i32);
            Ok((reader, width, height, stride, format))
        }
    }

    fn created(result: HRESULT) -> Result<(), String> {
        result
            .ok()
            .map_err(|error| format!("Media Foundation cannot create the decoder: {error}"))
    }

    fn setup<T>(result: windows::core::Result<T>) -> Result<T, String> {
        result.map_err(|error| format!("Media Foundation rejected the setup: {error}"))
    }

    fn nv12_planes<'a>(
        data: *const u8,
        length: u32,
        width: u32,
        height: u32,
        stride: i32,
    ) -> Result<Nv12Frame<'a>, String> {
        if stride < 0 {
            return Err(format!("Unsupported negative NV12 stride {stride}"));
        }
        let pitch = stride.unsigned_abs() as usize;
        let w = width as usize;
        let h = height as usize;
        let needed = pitch * (h + h / 2);
        if pitch < w || (length as usize) < needed {
            return Err(format!(
                "Unexpected NV12 buffer of {length} bytes for {width}x{height} at stride {stride}"
            ));
        }
        let h16 = h.next_multiple_of(16);
        let h32 = h.next_multiple_of(32);
        // Hardware MFT decoders may align the NV12 luma slice height to 16 or 32
        // scanlines so the UV plane starts at `pitch * plane_h` instead of `pitch * h`.
        let plane_h = if (length as usize) == pitch * (h16 + h16 / 2) {
            h16
        } else if (length as usize) == pitch * (h32 + h32 / 2) {
            h32
        } else {
            h
        };
        let source = unsafe { std::slice::from_raw_parts(data, length as usize) };
        let y_end = pitch * plane_h;
        let uv_end = y_end + pitch * (h / 2);
        Ok(Nv12Frame {
            y_plane: &source[..pitch * h],
            uv_plane: &source[y_end..uv_end],
            pitch,
            width: w,
            height: h,
        })
    }

    fn nv12_full_rows(src: &Nv12Frame<'_>, y_start: usize, dst: &mut [u8]) {
        let w = src.width;
        for (dy, out) in dst.chunks_exact_mut(w * 4).enumerate() {
            let py = y_start + dy;
            let row_y = &src.y_plane[py * src.pitch..][..w];
            let row_uv = &src.uv_plane[(py >> 1) * src.pitch..][..w];
            for (px2, pair) in out.as_chunks_mut::<8>().0.iter_mut().enumerate() {
                let u = i32::from(row_uv[px2 * 2]) - 128;
                let v = i32::from(row_uv[px2 * 2 + 1]) - 128;
                let db = 2163 * u + 512;
                let dg = -218 * u - 546 * v + 512;
                let dr = 1836 * v + 512;
                let cy0 = 1192 * (i32::from(row_y[px2 * 2]) - 16).max(0);
                pair[0] = ((cy0 + db) >> 10).clamp(0, 255) as u8;
                pair[1] = ((cy0 + dg) >> 10).clamp(0, 255) as u8;
                pair[2] = ((cy0 + dr) >> 10).clamp(0, 255) as u8;
                pair[3] = 255;
                let cy1 = 1192 * (i32::from(row_y[px2 * 2 + 1]) - 16).max(0);
                pair[4] = ((cy1 + db) >> 10).clamp(0, 255) as u8;
                pair[5] = ((cy1 + dg) >> 10).clamp(0, 255) as u8;
                pair[6] = ((cy1 + dr) >> 10).clamp(0, 255) as u8;
                pair[7] = 255;
            }
        }
    }

    fn nv12_full_to_bgra(
        src: &Nv12Frame<'_>,
        pool: Option<(&RowPool, &mut u64)>,
    ) -> Result<image::RgbaImage, String> {
        let mut raw = vec![0u8; src.width * src.height * 4];
        if src.height >= 256
            && let Some((pool, seq)) = pool
        {
            pool.dispatch(seq, src, None, &mut raw);
        } else {
            nv12_full_rows(src, 0, &mut raw);
        }
        image::RgbaImage::from_raw(src.width as u32, src.height as u32, raw)
            .ok_or_else(|| "Unexpected NV12 buffer size".to_owned())
    }

    fn nv12_scaled_rows(src: &Nv12Frame<'_>, lut: &ScaleLut, y_start: usize, dst: &mut [u8]) {
        let (w, h) = (src.width, src.height);
        let (dw, dh) = (lut.dst_w, lut.dst_h);
        for (dy, out) in dst.chunks_exact_mut(dw * 4).enumerate() {
            let py = y_start + dy;
            let sy0 = ((py * h * 2 + h) / (dh * 2)).min(h - 1);
            let sy1 = (sy0 + 1).min(h - 1);
            let row_y0 = &src.y_plane[sy0 * src.pitch..][..w];
            let row_y1 = &src.y_plane[sy1 * src.pitch..][..w];
            let row_uv = &src.uv_plane[(sy0 >> 1) * src.pitch..][..w];
            for (px, out_px) in out.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let x0 = lut.sx0[px];
                let x1 = lut.sx1[px];
                let ux = lut.ux[px];
                let y4 = i32::from(row_y0[x0])
                    + i32::from(row_y0[x1])
                    + i32::from(row_y1[x0])
                    + i32::from(row_y1[x1]);
                let cy = 1192 * (((y4 + 2) >> 2) - 16).max(0);
                let u = i32::from(row_uv[ux]) - 128;
                let v = i32::from(row_uv[ux + 1]) - 128;
                out_px[0] = ((cy + 2163 * u + 512) >> 10).clamp(0, 255) as u8;
                out_px[1] = ((cy - 218 * u - 546 * v + 512) >> 10).clamp(0, 255) as u8;
                out_px[2] = ((cy + 1836 * v + 512) >> 10).clamp(0, 255) as u8;
                out_px[3] = 255;
            }
        }
    }

    fn nv12_scaled_to_bgra(
        src: &Nv12Frame<'_>,
        lut: &ScaleLut,
        pool: Option<(&RowPool, &mut u64)>,
    ) -> Result<image::RgbaImage, String> {
        let (dw, dh) = (lut.dst_w, lut.dst_h);
        let mut raw = vec![0u8; dw * dh * 4];
        if dh >= 256
            && let Some((pool, seq)) = pool
        {
            pool.dispatch(seq, src, Some(lut), &mut raw);
        } else {
            nv12_scaled_rows(src, lut, 0, &mut raw);
        }
        image::RgbaImage::from_raw(dw as u32, dh as u32, raw)
            .ok_or_else(|| "Unexpected scaled NV12 buffer size".to_owned())
    }

    /// Media Foundation RGB32 buffers keep the pixel order GPUI uploads. A
    /// negative stride is bottom-up; the fourth byte is unused.
    fn copy(
        data: *const u8,
        length: u32,
        width: u32,
        height: u32,
        stride: i32,
    ) -> Result<image::RgbaImage, String> {
        let row = width as usize * 4;
        let pitch = stride.unsigned_abs() as usize;
        let height = height as usize;
        let needed = (height - 1) * pitch + row;
        if (length as usize) < needed || pitch < row {
            return Err(format!(
                "Unexpected Media Foundation buffer of {length} bytes for {width}x{height} at stride {stride}"
            ));
        }
        let source = unsafe { std::slice::from_raw_parts(data, length as usize) };
        let total = row * height;
        let mut raw = Vec::with_capacity(total);
        if stride > 0 && pitch == row {
            raw.extend_from_slice(&source[..total]);
        } else {
            for y in 0..height {
                let source_row = if stride < 0 { height - 1 - y } else { y };
                let start = source_row * pitch;
                raw.extend_from_slice(&source[start..start + row]);
            }
        }
        // RGB32 leaves the fourth byte unused; GPUI uploads all four.
        let (prefix, words, suffix) = unsafe { raw.align_to_mut::<u64>() };
        for px in prefix.as_chunks_mut::<4>().0 {
            px[3] = 255;
        }
        for word in words {
            *word |= 0xFF00_0000_FF00_0000;
        }
        for px in suffix.as_chunks_mut::<4>().0 {
            px[3] = 255;
        }
        image::RgbaImage::from_raw(width, height as u32, raw)
            .ok_or_else(|| "Unexpected Media Foundation buffer alignment".to_owned())
    }

    fn start() -> Result<(), String> {
        static PLATFORM: OnceLock<Result<(), String>> = OnceLock::new();
        // Decoding runs on the shared background executor, so every call joins
        // the same multi-threaded apartment before using a reader. A thread
        // that is already initialised reports S_FALSE and stays usable.
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if initialized.is_err() {
            return Err(format!("COM initialization failed: {initialized:?}"));
        }
        // The platform is deliberately kept for the process lifetime instead of
        // being shut down while another executor task may still hold a reader.
        PLATFORM
            .get_or_init(|| {
                let api = api()?;
                unsafe { (api.startup)(MF_VERSION, MFSTARTUP_LITE) }
                    .ok()
                    .map_err(|error| format!("Media Foundation cannot start: {error}"))
            })
            .clone()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn bottom_up_buffers_are_flipped_into_top_down_rows() {
            // With a negative stride the buffer starts with the bottom row.
            let mut data = Vec::new();
            data.extend_from_slice(&[1, 2, 3, 0, 4, 5, 6, 0]);
            data.extend_from_slice(&[7, 8, 9, 0, 10, 11, 12, 0]);
            let image = copy(data.as_ptr(), data.len() as u32, 2, 2, -8).unwrap();
            let raw = image.as_raw();
            assert_eq!(&raw[0..4], &[7, 8, 9, 255]);
            assert_eq!(&raw[4..8], &[10, 11, 12, 255]);
            assert_eq!(&raw[8..12], &[1, 2, 3, 255]);
            assert_eq!(&raw[12..16], &[4, 5, 6, 255]);
        }

        #[test]
        #[ignore = "compare RGB32 vs NV12 Media Foundation decode and conversion latency"]
        fn compare_mf_decode_paths() {
            use std::time::Instant;
            use windows::Win32::Media::MediaFoundation::{
                MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, MFVideoFormat_NV12,
            };
            let fixture_mp4 =
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/scenery/movie.mp4");
            start().unwrap();
            let api = api().unwrap();

            for mp4 in [
                PathBuf::from(r"C:\carbon-backdrops-target\debug\scenery\kingdom-keep.mp4"),
                fixture_mp4,
            ] {
                let wide = HSTRING::from(mp4.to_string_lossy().as_ref());
                let mut rgb32_sample0 = Vec::new();
                let mut nv12_sample0 = Vec::new();
                for (label, subtype, vp) in [
                    ("RGB32+VP", MFVideoFormat_RGB32, 1u32),
                    ("NV12(noVP)", MFVideoFormat_NV12, 0u32),
                ] {
                    let t_open = Instant::now();
                    let (reader, w, h, stride) = unsafe {
                        let mut attrs = None;
                        (api.create_attributes)(&mut attrs, 2).ok().unwrap();
                        let attrs = attrs.unwrap();
                        setup(attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, vp))
                            .unwrap();
                        setup(attrs.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1))
                            .unwrap();
                        let mut r = None;
                        created((api.create_reader_from_url)(
                            PCWSTR(wide.as_ptr()),
                            attrs.as_raw(),
                            &mut r,
                        ))
                        .unwrap();
                        let r = r.unwrap();
                        setup(r.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false))
                            .unwrap();
                        setup(r.SetStreamSelection(
                            MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                            true,
                        ))
                        .unwrap();
                        let mut m: Option<IMFMediaType> = None;
                        created((api.create_media_type)(&mut m)).unwrap();
                        let m = m.unwrap();
                        setup(m.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)).unwrap();
                        setup(m.SetGUID(&MF_MT_SUBTYPE, &subtype)).unwrap();
                        setup(r.SetCurrentMediaType(
                            MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                            None,
                            &m,
                        ))
                        .unwrap();
                        let cur = setup(
                            r.GetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32),
                        )
                        .unwrap();
                        let size = setup(cur.GetUINT64(&MF_MT_FRAME_SIZE)).unwrap();
                        let (w, h) = ((size >> 32) as usize, size as u32 as usize);
                        let stride = cur
                            .GetUINT32(&MF_MT_DEFAULT_STRIDE)
                            .map_or(w as i32, |s| s as i32);
                        (r, w, h, stride)
                    };
                    let open_us = t_open.elapsed().as_micros();
                    let mut read_us = Vec::with_capacity(60);
                    let mut lock_us = Vec::with_capacity(60);
                    let mut conv_full_us = Vec::with_capacity(60);
                    let mut conv_win_us = Vec::with_capacity(60);
                    let (tw, th) = (1368usize, 770usize);
                    let mut dst_full = vec![255u8; w * h * 4];
                    let mut dst_win = vec![255u8; tw * th * 4];
                    let t_loop = Instant::now();
                    for frame_idx in 0..60usize {
                        let t0 = Instant::now();
                        let mut flags = 0u32;
                        let mut sample = None;
                        unsafe {
                            reader
                                .ReadSample(
                                    MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                                    0,
                                    None,
                                    Some(&mut flags),
                                    None,
                                    Some(&mut sample),
                                )
                                .unwrap();
                        }
                        let d_read = t0.elapsed().as_micros();
                        let Some(sample) = sample else { continue };
                        let t1 = Instant::now();
                        let buf = unsafe { sample.ConvertToContiguousBuffer().unwrap() };
                        let mut data = std::ptr::null_mut();
                        let mut len = 0;
                        unsafe { buf.Lock(&mut data, None, Some(&mut len)).unwrap() };
                        let src = unsafe { std::slice::from_raw_parts(data, len as usize) };
                        let d_lock = t1.elapsed().as_micros();
                        if vp == 0 {
                            let st = stride.unsigned_abs() as usize;
                            let slice_h = if st > 0 {
                                ((len as usize) * 2) / (3 * st)
                            } else {
                                h
                            };
                            if frame_idx == 0 {
                                println!(
                                    "  [NV12 layout] file={} w={w} h={h} stride={stride} len={len} slice_h={slice_h} uv@h={:?} uv@slice_h={:?}",
                                    mp4.file_name().unwrap().to_string_lossy(),
                                    &src[st * h..][..6],
                                    &src[st * slice_h.min(h.max(slice_h))..][..6],
                                );
                            }
                            let plane_h = slice_h.max(h);
                            let y_plane = &src[..st * h];
                            let uv_plane = &src[st * plane_h..st * plane_h + st * (h / 2)];
                            let threads = 4usize;

                            let t2 = Instant::now();
                            let rows_win = th.div_ceil(threads);
                            std::thread::scope(|s| {
                                for (tid, chunk) in
                                    dst_win.chunks_mut(rows_win * tw * 4).enumerate()
                                {
                                    let y_start = tid * rows_win;
                                    s.spawn(move || {
                                        for (dy, out) in chunk.chunks_exact_mut(tw * 4).enumerate()
                                        {
                                            let py = y_start + dy;
                                            let sy0 = ((py * h * 2 + h) / (th * 2)).min(h - 1);
                                            let sy1 = (sy0 + 1).min(h - 1);
                                            let row_y0 = &y_plane[sy0 * st..][..w];
                                            let row_y1 = &y_plane[sy1 * st..][..w];
                                            let row_uv = &uv_plane[(sy0 >> 1) * st..][..w];
                                            for (px, px_out) in
                                                out.as_chunks_mut::<4>().0.iter_mut().enumerate()
                                            {
                                                let sx0 = ((px * w * 2 + w) / (tw * 2)).min(w - 1);
                                                let sx1 = (sx0 + 1).min(w - 1);
                                                let y4 = i32::from(row_y0[sx0])
                                                    + i32::from(row_y0[sx1])
                                                    + i32::from(row_y1[sx0])
                                                    + i32::from(row_y1[sx1]);
                                                let cy = 1192 * (((y4 + 2) >> 2) - 16).max(0);
                                                let ux = (sx0 & !1).min(w - 2);
                                                let u = i32::from(row_uv[ux]) - 128;
                                                let v = i32::from(row_uv[ux + 1]) - 128;
                                                px_out[0] = ((cy + 2163 * u + 512) >> 10)
                                                    .clamp(0, 255)
                                                    as u8;
                                                px_out[1] = ((cy - 218 * u - 546 * v + 512) >> 10)
                                                    .clamp(0, 255)
                                                    as u8;
                                                px_out[2] = ((cy + 1836 * v + 512) >> 10)
                                                    .clamp(0, 255)
                                                    as u8;
                                            }
                                        }
                                    });
                                }
                            });
                            conv_win_us.push(t2.elapsed().as_micros());

                            let t3 = Instant::now();
                            let rows_full = h.div_ceil(threads);
                            std::thread::scope(|s| {
                                for (tid, chunk) in
                                    dst_full.chunks_mut(rows_full * w * 4).enumerate()
                                {
                                    let y_start = tid * rows_full;
                                    s.spawn(move || {
                                        for (dy, out) in chunk.chunks_exact_mut(w * 4).enumerate() {
                                            let py = y_start + dy;
                                            let row_y = &y_plane[py * st..][..w];
                                            let row_uv = &uv_plane[(py >> 1) * st..][..w];
                                            for (px2, pair) in
                                                out.as_chunks_mut::<8>().0.iter_mut().enumerate()
                                            {
                                                let u = i32::from(row_uv[px2 * 2]) - 128;
                                                let v = i32::from(row_uv[px2 * 2 + 1]) - 128;
                                                let db = 2163 * u + 512;
                                                let dg = -218 * u - 546 * v + 512;
                                                let dr = 1836 * v + 512;
                                                let cy0 =
                                                    1192 * (i32::from(row_y[px2 * 2]) - 16).max(0);
                                                pair[0] = ((cy0 + db) >> 10).clamp(0, 255) as u8;
                                                pair[1] = ((cy0 + dg) >> 10).clamp(0, 255) as u8;
                                                pair[2] = ((cy0 + dr) >> 10).clamp(0, 255) as u8;
                                                let cy1 = 1192
                                                    * (i32::from(row_y[px2 * 2 + 1]) - 16).max(0);
                                                pair[4] = ((cy1 + db) >> 10).clamp(0, 255) as u8;
                                                pair[5] = ((cy1 + dg) >> 10).clamp(0, 255) as u8;
                                                pair[6] = ((cy1 + dr) >> 10).clamp(0, 255) as u8;
                                            }
                                        }
                                    });
                                }
                            });
                            conv_full_us.push(t3.elapsed().as_micros());
                            if frame_idx == 0 {
                                nv12_sample0 = dst_full.clone();
                            }
                        } else {
                            let t3 = Instant::now();
                            dst_full.copy_from_slice(&src[..w * h * 4]);
                            conv_full_us.push(t3.elapsed().as_micros());
                            if frame_idx == 0 {
                                rgb32_sample0 = dst_full.clone();
                            }
                        }
                        unsafe {
                            let _ = buf.Unlock();
                        }
                        read_us.push(d_read);
                        lock_us.push(d_lock);
                    }
                    let loop_ms = t_loop.elapsed().as_millis();
                    std::hint::black_box((&dst_full, &dst_win));
                    let med = |mut v: Vec<u128>| {
                        if v.is_empty() {
                            return 0;
                        }
                        v.sort_unstable();
                        v[v.len() / 2]
                    };
                    println!(
                        "[{label} {}] ({w}x{h} stride={stride}) open={open_us}us total_60f={loop_ms}ms | med ReadSample={}us Lock={}us conv_full={}us conv_win(1368x770)={}us",
                        mp4.file_name().unwrap().to_string_lossy(),
                        med(read_us),
                        med(lock_us),
                        med(conv_full_us),
                        med(conv_win_us)
                    );
                }
                if !rgb32_sample0.is_empty() && !nv12_sample0.is_empty() {
                    let mae: f64 = rgb32_sample0
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .zip(nv12_sample0.as_chunks::<4>().0.iter())
                        .map(|(a, b)| {
                            (a[0] as i32 - b[0] as i32).abs()
                                + (a[1] as i32 - b[1] as i32).abs()
                                + (a[2] as i32 - b[2] as i32).abs()
                        })
                        .sum::<i32>() as f64
                        / ((rgb32_sample0.len() / 4 * 3) as f64);
                    println!(
                        "  Color MAE BT.709 NV12 vs RGB32 ({}) = {mae:.3}",
                        mp4.file_name().unwrap().to_string_lossy()
                    );
                }
            }
        }
    }
}

#[cfg(not(windows))]
mod mf {
    use super::super::package::Movie;
    use gpui::RenderImage;
    use std::{path::PathBuf, sync::Arc};

    /// Keeps the package contract identical on platforms without a qualified
    /// decoder; every read reports that bundled artwork should be used.
    pub(super) struct Reader {
        path: PathBuf,
        frames: usize,
        next: usize,
    }

    impl Reader {
        pub(super) fn open(
            movie: &Movie,
            step: usize,
            _target: Option<(u32, u32)>,
        ) -> Result<Self, String> {
            Ok(Self {
                path: movie.motion().to_owned(),
                frames: movie.frames as usize,
                next: step,
            })
        }
        pub(super) fn matches(&self, movie: &Movie) -> bool {
            movie.motion() == self.path
        }
        pub(super) fn next(&self) -> usize {
            self.next
        }
        pub(super) fn set_target(&mut self, _target: Option<(u32, u32)>) {}
        pub(super) fn read(&mut self) -> Result<Arc<RenderImage>, String> {
            self.next = (self.next + 1) % self.frames.max(1);
            Err(format!(
                "No in-process scenery decoder is qualified on {}",
                std::env::consts::OS
            ))
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn movie() -> Movie {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/scenery");
        Movie::test(root.join("movie.jpg"), root.join("movie.mp4"), 30)
    }

    #[test]
    fn decodes_packaged_movies_in_process_at_the_encoded_size() {
        let movie = movie();
        let (reader, first) = frame(None, &movie, 0, None).unwrap();
        let size = first.size(0);
        assert_eq!((size.width.0, size.height.0), (320, 180));
        let bytes = first.as_bytes(0).unwrap();
        assert_eq!(bytes.len(), 320 * 180 * 4);
        // The authored clip is red on top and blue below.
        let pixel = |x: usize, y: usize| &bytes[(y * 320 + x) * 4..][..4];
        assert!(
            pixel(10, 10)[2] > 180 && pixel(10, 10)[0] < 80,
            "top band {:?}",
            pixel(10, 10)
        );
        assert!(
            pixel(10, 170)[0] > 180 && pixel(10, 170)[2] < 80,
            "bottom band {:?}",
            pixel(10, 170)
        );
        assert!(bytes.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 255));

        let (reader, later) = frame(Some(reader), &movie, 15, None).unwrap();
        let moved = first
            .as_bytes(0)
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .zip(later.as_bytes(0).unwrap().as_chunks::<4>().0)
            .filter(|(before, after)| before != after)
            .count();
        assert!(moved > 500, "the clip must move between frames: {moved}");
        drop(reader);

        // Reopening mid-loop and wrapping both land on the requested frame.
        let (_, direct) = frame(None, &movie, 10, None).unwrap();
        let (mut reader, _) = frame(None, &movie, 0, None).unwrap();
        let mut last = None;
        for step in 1..=30 {
            let (next, image) = frame(Some(reader), &movie, step, None).unwrap();
            reader = next;
            if step == 10 {
                assert_eq!(image.as_bytes(0), direct.as_bytes(0));
            }
            last = Some(image);
        }
        assert_eq!(last.unwrap().as_bytes(0), first.as_bytes(0));
    }

    #[test]
    fn decodes_high_profile_b_frame_movies_across_loop_wrap() {
        let dir = tempfile::tempdir().unwrap();
        let bframe = dir.path().join("bframe.mp4");
        let generated = std::process::Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x180:rate=60:duration=0.5",
                "-c:v",
                "libx264",
                "-profile:v",
                "high",
                "-bf",
                "3",
                "-g",
                "30",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&bframe)
            .status();
        if !matches!(generated, Ok(status) if status.success()) {
            return;
        }
        let movie = Movie::test(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/scenery/movie.jpg"),
            bframe,
            30,
        );
        let (mut reader, first) = frame(None, &movie, 0, None).unwrap();
        let mut last = None;
        for step in 1..=30 {
            let (next, image) = frame(Some(reader), &movie, step, None).unwrap();
            reader = next;
            last = Some(image);
        }
        assert_eq!(last.unwrap().as_bytes(0), first.as_bytes(0));
    }
}
