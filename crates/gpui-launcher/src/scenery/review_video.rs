// SPDX-License-Identifier: MPL-2.0
//! Local H.264 experiment. Only an explicit launcher invocation selects the
//! decoder executable and trusted artwork directory. Neither is read from an instance.
use gpui::RenderImage;
use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Child, ChildStdout, Command, Stdio},
    sync::{Arc, Mutex, OnceLock},
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

#[derive(Default, Clone)]
pub(super) struct Cancellation(Arc<Mutex<DecoderState>>);

#[derive(Default)]
struct DecoderState {
    cancelled: bool,
    child: Option<Child>,
}

impl Cancellation {
    pub(super) fn cancel(&self) {
        let mut state = self.0.lock().unwrap();
        state.cancelled = true;
        if let Some(mut child) = state.child.take() {
            let _ = child.kill();
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
    }
}

pub(super) struct Reader {
    cancellation: Cancellation,
    pipe: ChildStdout,
    next: usize,
}

impl Reader {
    fn start(
        source: &Source,
        file: &Path,
        step: usize,
        cancellation: Cancellation,
    ) -> io::Result<Self> {
        Self::from_child(
            Self::command(source, file, step).spawn()?,
            step,
            cancellation,
        )
    }

    fn command(source: &Source, file: &Path, step: usize) -> Command {
        let mut command = Command::new(&source.decoder);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        command
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
                "-i",
            ])
            .arg(file)
            // Skip frames on the continuous output timeline. Seeking the input
            // changes where FFmpeg restarts it at subsequent loop boundaries.
            .arg("-ss")
            .arg(format!("{:.9}", (step % CYCLE) as f64 / FPS as f64))
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
            .stderr(Stdio::null());
        command
    }

    fn from_child(mut child: Child, step: usize, cancellation: Cancellation) -> io::Result<Self> {
        let pipe = child.stdout.take().expect("piped video output");
        let mut state = cancellation.0.lock().unwrap();
        if state.cancelled {
            drop(state);
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::from(io::ErrorKind::Interrupted));
        }
        state.child = Some(child);
        drop(state);
        Ok(Self {
            cancellation,
            pipe,
            next: step,
        })
    }
    pub(super) fn frame(
        previous: Option<Self>,
        cancellation: Cancellation,
        source: &Source,
        id: &str,
        step: usize,
    ) -> io::Result<(Self, Arc<RenderImage>)> {
        // Keep the decoder warm through stalls. Skip at most two images on a
        // late tick and rebase the clock to avoid a backlog or repeated seeks.
        let mut reader = match previous {
            Some(reader) => reader,
            _ => Self::start(
                source,
                &source.directory.join(format!("{id}.mp4")),
                step,
                cancellation,
            )?,
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
        self.cancellation.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, sync::mpsc, time::Duration};

    #[test]
    fn looping_seek_is_applied_to_output_after_the_movie_input() {
        let source = Source {
            decoder: PathBuf::from("ffmpeg"),
            directory: PathBuf::new(),
        };
        let command = Reader::command(&source, Path::new("scene.mp4"), CYCLE + 105);
        let args: Vec<_> = command.get_args().collect();
        let input = args.iter().position(|arg| *arg == "-i").unwrap();
        let seek = args.iter().position(|arg| *arg == "-ss").unwrap();
        assert_eq!(args[input + 1], "scene.mp4");
        assert!(seek > input + 1, "input seeks change the loop boundary");
        assert_eq!(args[seek + 1], "3.500000000");
    }

    #[test]
    #[ignore = "child process used by cancellation_unblocks_a_partial_frame"]
    fn hold_partial_frame_for_decoder_cancellation() {
        if std::env::var_os("CARBON_TEST_PARTIAL_FRAME_CHILD").is_none() {
            return;
        }
        // The sentinel confirms this helper ran, followed by less than one frame.
        let mut out = std::io::stdout().lock();
        out.write_all(&[0, 1, 2, 3]).unwrap();
        out.flush().unwrap();
        std::thread::sleep(Duration::from_secs(15));
    }

    #[test]
    fn cancellation_unblocks_a_partial_frame() {
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "hold_partial_frame_for_decoder_cancellation",
                "--ignored",
                "--nocapture",
            ])
            .env("CARBON_TEST_PARTIAL_FRAME_CHILD", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let cancellation = Cancellation::default();
        let mut reader = Reader::from_child(child, 0, cancellation.clone()).unwrap();
        let (ready, started) = mpsc::channel();
        let (done, finished) = mpsc::channel();
        std::thread::spawn(move || {
            let mut byte = [0];
            loop {
                reader.pipe.read_exact(&mut byte).unwrap();
                if byte == [0] {
                    break;
                }
            }
            ready.send(()).unwrap();
            let mut frame = [0; 64];
            done.send(reader.pipe.read_exact(&mut frame)).unwrap();
        });
        let did_start = started.recv_timeout(Duration::from_secs(10));
        cancellation.cancel();
        did_start.expect("the decoder helper must emit its sentinel");
        let result = finished
            .recv_timeout(Duration::from_secs(5))
            .expect("cancellation must interrupt a partial frame before the helper exits");
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
    }
}
