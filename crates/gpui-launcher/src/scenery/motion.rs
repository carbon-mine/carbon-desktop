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
) -> Result<(Reader, Arc<RenderImage>), String> {
    let mut reader = match previous {
        Some(Reader(reader)) if reader.matches(movie) => reader,
        _ => mf::Reader::open(movie, step)?,
    };
    // A stale in-flight decode can leave the reader up to three frames ahead of
    // the resumed step; present the next frame rather than rewinding.
    if reader.next() > step + 3 {
        reader = mf::Reader::open(movie, step)?;
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
        sync::{Arc, OnceLock},
    };
    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::Media::MediaFoundation::{
        IMFAttributes, IMFMediaType, IMFSample, IMFSourceReader, MF_MT_DEFAULT_STRIDE,
        MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_SOURCE_READER_ALL_STREAMS,
        MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, MF_SOURCE_READER_FIRST_VIDEO_STREAM,
        MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READERF_ERROR, MF_VERSION, MFMediaType_Video,
        MFSTARTUP_LITE, MFVideoFormat_RGB32,
    };
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    use windows::core::{HRESULT, HSTRING, Interface, PCSTR, PCWSTR, w};

    const MAX_EDGE: u32 = 4096;

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

    pub(super) struct Reader {
        path: PathBuf,
        width: u32,
        height: u32,
        stride: i32,
        frames: usize,
        next: usize,
        reader: IMFSourceReader,
    }

    impl Reader {
        pub(super) fn open(movie: &Movie, step: usize) -> Result<Self, String> {
            let path = movie.motion().to_owned();
            start()?;
            let (reader, width, height, stride) = source(&path).map_err(|error| {
                format!("Media Foundation cannot open {}: {error}", path.display())
            })?;
            let mut open = Self {
                path,
                width,
                height,
                stride,
                frames: movie.frames as usize,
                next: 0,
                reader,
            };
            for _ in 0..step % open.frames {
                open.read()?;
            }
            open.next = step;
            Ok(open)
        }

        pub(super) fn matches(&self, movie: &Movie) -> bool {
            movie.motion() == self.path
        }

        pub(super) fn next(&self) -> usize {
            self.next
        }

        pub(super) fn read(&mut self) -> Result<Arc<RenderImage>, String> {
            let image = match self.sample()? {
                Some(image) => image,
                None => {
                    self.restart()?;
                    self.sample()?
                        .ok_or("Packaged scenery ended without a frame")?
                }
            };
            self.next += 1;
            Ok(image)
        }

        fn sample(&mut self) -> Result<Option<Arc<RenderImage>>, String> {
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
                if let Some(sample) = sample.as_ref() {
                    return self.present(sample).map(Some);
                }
                if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                    return Ok(None);
                }
            }
            Err("Media Foundation produced no frame".into())
        }

        fn restart(&mut self) -> Result<(), String> {
            let (reader, width, height, stride) = source(&self.path).map_err(|error| {
                format!(
                    "Media Foundation cannot reopen {}: {error}",
                    self.path.display()
                )
            })?;
            self.reader = reader;
            self.width = width;
            self.height = height;
            self.stride = stride;
            Ok(())
        }

        fn present(&self, sample: &IMFSample) -> Result<Arc<RenderImage>, String> {
            let buffer = unsafe { sample.ConvertToContiguousBuffer() }
                .map_err(|error| format!("Media Foundation buffer failed: {error}"))?;
            let mut data = std::ptr::null_mut();
            let mut length = 0;
            unsafe {
                buffer
                    .Lock(&mut data, None, Some(&mut length))
                    .map_err(|error| format!("Media Foundation lock failed: {error}"))?;
            }
            let copied = copy(data, length, self.width, self.height, self.stride);
            unsafe {
                let _ = buffer.Unlock();
            }
            copied.map(|image| Arc::new(RenderImage::new([image::Frame::new(image)])))
        }
    }

    fn source(path: &Path) -> Result<(IMFSourceReader, u32, u32, i32), String> {
        let api = api()?;
        let wide = HSTRING::from(path.to_string_lossy().as_ref());
        unsafe {
            let mut attributes: Option<IMFAttributes> = None;
            created((api.create_attributes)(&mut attributes, 1))?;
            let attributes =
                attributes.ok_or_else(|| "Media Foundation returned no attributes".to_string())?;
            setup(attributes.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1))?;
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
            setup(media.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32))?;
            setup(reader.SetCurrentMediaType(
                MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32,
                None,
                &media,
            ))?;
            // The video processor converts to RGB32 at the encoded size; it does
            // not scale, so the package's movie declares the presented size.
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
            let stride = current
                .GetUINT32(&MF_MT_DEFAULT_STRIDE)
                .map_or((width * 4) as i32, |stride| stride as i32);
            Ok((reader, width, height, stride))
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
        let mut image = image::RgbaImage::new(width, height as u32);
        let raw: &mut [u8] = image.as_mut();
        if stride > 0 && pitch == row {
            raw[..row * height].copy_from_slice(&source[..row * height]);
        } else {
            for y in 0..height {
                let source_row = if stride < 0 { height - 1 - y } else { y };
                let start = source_row * pitch;
                let target: &mut [u8] = &mut raw[y * row..][..row];
                target.copy_from_slice(&source[start..start + row]);
            }
        }
        // RGB32 leaves the fourth byte unused; GPUI uploads all four.
        let words = unsafe {
            std::slice::from_raw_parts_mut(raw.as_mut_ptr().cast::<u64>(), raw.len() / 8)
        };
        for word in words.iter_mut() {
            *word |= 0xFF00_0000_FF00_0000;
        }
        Ok(image)
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
        pub(super) fn open(movie: &Movie, step: usize) -> Result<Self, String> {
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
        let (reader, first) = frame(None, &movie, 0).unwrap();
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

        let (reader, later) = frame(Some(reader), &movie, 15).unwrap();
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
        let (_, direct) = frame(None, &movie, 10).unwrap();
        let (mut reader, _) = frame(None, &movie, 0).unwrap();
        let mut last = None;
        for step in 1..=30 {
            let (next, image) = frame(Some(reader), &movie, step).unwrap();
            reader = next;
            if step == 10 {
                assert_eq!(image.as_bytes(0), direct.as_bytes(0));
            }
            last = Some(image);
        }
        assert_eq!(last.unwrap().as_bytes(0), first.as_bytes(0));
    }
}
