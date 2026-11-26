// SPDX-License-Identifier: MPL-2.0
//! Local H.264 experiment. Only an explicit launcher invocation selects the
//! decoder executable and trusted artwork directory. Neither is read from an instance.
use gpui::RenderImage;
use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Child, ChildStdout, Command, Stdio},
    sync::{Arc, OnceLock},
};

const WIDTH: u32 = 1440;
const HEIGHT: u32 = 810;
const FPS: usize = 30;
const CYCLE: usize = 8 * FPS;

pub(super) struct Source {
    decoder: PathBuf,
    directory: PathBuf,
}

impl Source {
    pub(super) fn configured() -> Option<&'static Self> {
        static CONFIG: OnceLock<Option<Source>> = OnceLock::new();
        CONFIG
            .get_or_init(|| {
                let args: Vec<_> = std::env::args_os().collect();
                let path = |key: &str| {
                    let pair = args.windows(2).find(|p| p[0] == key)?;
                    let path = PathBuf::from(&pair[1]);
                    path.is_absolute().then_some(path)
                };
                let decoder = path("--scenery-review-ffmpeg")?;
                let directory = path("--scenery-review-dir")?;
                (decoder.is_file() && directory.is_dir()).then_some(Self { decoder, directory })
            })
            .as_ref()
    }
    pub(super) fn poster(&self, id: &str) -> Option<Arc<RenderImage>> {
        let path = self.directory.join(format!("{id}.jpg"));
        super::decode(&std::fs::read(path).ok()?)
    }
}

pub(super) struct Reader {
    child: Option<Child>,
    pipe: ChildStdout,
    next: usize,
}

impl Reader {
    fn start(source: &Source, file: &Path, step: usize) -> io::Result<Self> {
        let mut command = Command::new(&source.decoder);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        let mut child = command
            .args([
                "-nostdin",
                "-v",
                "error",
                "-protocol_whitelist",
                "file",
                "-f",
                "mov",
                "-stream_loop",
                "-1",
                "-threads",
                "2",
                "-c:v",
                "h264",
                "-ss",
            ])
            .arg(format!("{:.9}", (step % CYCLE) as f64 / FPS as f64))
            .arg("-i")
            .arg(file)
            .args([
                "-map",
                "0:v:0",
                "-an",
                "-sn",
                "-dn",
                "-vf",
                "scale=1440:810:in_color_matrix=bt709:out_range=full",
                "-pix_fmt",
                "bgra",
                "-threads",
                "1",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let pipe = child.stdout.take().expect("piped video output");
        Ok(Self {
            child: Some(child),
            pipe,
            next: step,
        })
    }
    pub(super) fn frame(
        previous: Option<Self>,
        source: &Source,
        id: &str,
        step: usize,
    ) -> io::Result<(Self, Arc<RenderImage>)> {
        // Keep the decoder warm through stalls. Skip at most two images on a
        // late tick and rebase the clock to avoid a backlog or repeated seeks.
        let mut reader = match previous {
            Some(reader) => reader,
            _ => Self::start(source, &source.directory.join(format!("{id}.mp4")), step)?,
        };
        let mut bytes = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
        for _ in 0..=step.wrapping_sub(reader.next).min(2) {
            reader.pipe.read_exact(&mut bytes)?;
        }
        reader.next = step.wrapping_add(1);
        let image = image::RgbaImage::from_raw(WIDTH, HEIGHT, bytes).expect("fixed BGRA frame");
        Ok((
            reader,
            Arc::new(RenderImage::new([image::Frame::new(image)])),
        ))
    }
}
impl Drop for Reader {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
    }
}
