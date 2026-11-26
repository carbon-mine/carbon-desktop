use super::{cache, GameFileProgress, Object, Plan, Store};
use reqwest::{blocking::Client, redirect::Policy};
use sha1::{Digest, Sha1};
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex,
    },
    time::Duration,
};

pub(super) fn client() -> Result<Client, String> {
    Client::builder()
        .https_only(true)
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(6))
        .timeout(Duration::from_secs(120))
        .pool_max_idle_per_host(8)
        .build()
        .map_err(|_| "Could not initialize the official game download HTTPS client.".into())
}
pub(super) fn fetch(client: &Client, object: &Object) -> Result<Box<dyn Read + Send>, String> {
    // Only exact approved object URLs constructed/checked by Plan enter this function.
    for attempt in 0..2 {
        match client.get(&object.url).send() {
            Ok(response) if response.status().as_u16() == 200 => {
                if response
                    .content_length()
                    .is_some_and(|len| len != object.size)
                {
                    return Err(format!(
                        "Mojang returned an unexpected size for {}. Retry preparation.",
                        object.sha1
                    ));
                }
                return Ok(Box::new(response));
            }
            Ok(response) if attempt == 0 && response.status().is_server_error() => continue,
            Ok(response) => {
                return Err(format!(
                    "Official game download returned HTTP {} for {}. Check access and retry.",
                    response.status().as_u16(),
                    object.sha1
                ))
            }
            Err(_) if attempt == 0 => continue,
            Err(_) => {
                return Err(format!(
                "Could not reach the official game source for {}. Check the connection and retry.",
                object.sha1
            ))
            }
        }
    }
    Err("Could not fetch the game object.".into())
}

pub(super) fn digest(
    reader: impl Read,
    writer: &mut impl Write,
    object: &Object,
    mut tick: impl FnMut() -> Result<(), String>,
) -> Result<bool, String> {
    let mut reader = reader.take(object.size + 1);
    let mut hash = Sha1::new();
    let mut bytes = 0;
    let mut buf = [0u8; 64 * 1024];
    let mut progress_at = 0;
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|_| "Game object read failed. Retry preparation.")?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        if bytes > object.size {
            return Ok(false);
        }
        writer
            .write_all(&buf[..n])
            .map_err(|_| "Could not stage game content. Check free disk space and retry.")?;
        hash.update(&buf[..n]);
        if bytes - progress_at >= 256 * 1024 {
            tick()?;
            progress_at = bytes;
        }
    }
    Ok(bytes == object.size && hex::encode(hash.finalize()) == object.sha1)
}

fn one(
    path: &Path,
    object: &Object,
    offline: bool,
    fetch: &impl Fn(&Object) -> Result<Box<dyn Read + Send>, String>,
    mut tick: impl FnMut(&'static str) -> Result<(), String>,
) -> Result<bool, String> {
    tick("verifying")?;
    if cache::regular(path)? {
        let f = File::open(path).map_err(|_| "Could not open a cached game file.")?;
        if f.metadata()
            .map_err(|_| "Could not inspect a cached game file.")?
            .len()
            == object.size
            && digest(f, &mut io::sink(), object, || tick("verifying"))?
        {
            return Ok(false);
        }
    }
    if offline {
        return Err(format!(
            "Missing or damaged game object {}. Prepare the game files online to repair it.",
            object.sha1
        ));
    }
    tick("downloading")?;
    let input = fetch(object)?;
    let parent = path
        .parent()
        .ok_or("Missing game file staging directory.")?;
    // The native game-preparation lock excludes another writer of this digest's scratch file.
    // A killed process can leave one partial object; the next acquisition discards it.
    let partial_name = format!(".{}.game-part", object.sha1);
    let partial = parent.join(&partial_name);
    if cache::regular(&partial)? {
        fs::remove_file(&partial).map_err(|_| "Could not replace an interrupted game download.")?;
    }
    let mut scratch = tempfile::Builder::new()
        .prefix(&partial_name)
        .rand_bytes(0)
        .tempfile_in(parent)
        .map_err(|_| "Could not stage game content. Check free disk space.")?;
    if !digest(input, &mut scratch, object, || tick("downloading"))? {
        return Err(format!(
            "Game object {} failed its pinned size or SHA-1. Retry preparation.",
            object.sha1
        ));
    }
    tick("downloading")?;
    scratch
        .as_file()
        .sync_all()
        .map_err(|_| "Could not sync staged game content. Check storage and retry.")?;
    scratch
        .persist(path)
        .map_err(|_| "Could not publish verified game content. Check storage and retry.")?;
    cache::sync_dir(parent)?;
    Ok(true)
}

pub(super) fn objects(
    store: &Store,
    plan: &Plan,
    offline: bool,
    notify: impl FnMut(GameFileProgress) -> bool + Send,
    fetch: impl Fn(&Object) -> Result<Box<dyn Read + Send>, String> + Sync,
) -> Result<(u64, u64), String> {
    struct Progress<F> {
        callback: F,
        files: usize,
        verified: u64,
        fetched: u64,
        reused: u64,
    }
    let progress = Mutex::new(Progress {
        callback: notify,
        files: 0,
        verified: 0,
        fetched: 0,
        reused: 0,
    });
    let stopped = AtomicBool::new(false);
    let first_error = Mutex::new(None);
    let next = AtomicUsize::new(0);
    let objects: Vec<_> = plan.objects.values().collect();
    let update = |phase, completed: Option<(u64, bool)>| -> Result<(), String> {
        if stopped.load(Ordering::Acquire) {
            return Err("Game file preparation was cancelled.".into());
        }
        let mut p = progress
            .lock()
            .map_err(|_| "Game file progress was interrupted.")?;
        if let Some((size, downloaded)) = completed {
            p.files += 1;
            p.verified += size;
            if downloaded {
                p.fetched += size;
            } else {
                p.reused += size;
            }
        }
        let event = GameFileProgress {
            phase,
            completed_files: p.files,
            total_files: objects.len(),
            verified_bytes: p.verified,
            total_bytes: plan.summary.total_bytes,
        };
        if !(p.callback)(event) {
            return Err(
                "Game file preparation was cancelled. Verified content can be reused on retry."
                    .into(),
            );
        }
        Ok(())
    };
    update("verifying", None)?;
    std::thread::scope(|scope| {
        for _ in 0..8.min(objects.len()) {
            scope.spawn(|| {
                while !stopped.load(Ordering::Acquire) {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(object) = objects.get(i) else { break };
                    let result = store
                        .object(object, &plan.client_sha1)
                        .and_then(|path| {
                            one(&path, object, offline, &fetch, |phase| update(phase, None))
                        })
                        .and_then(|downloaded| {
                            update("verifying", Some((object.size, downloaded)))
                        });
                    if let Err(error) = result {
                        let mut first = first_error.lock().expect("worker error mutex");
                        if first.is_none() {
                            *first = Some(error);
                        }
                        stopped.store(true, Ordering::Release);
                        break;
                    }
                }
            });
        }
    });
    if let Some(error) = first_error
        .into_inner()
        .map_err(|_| "Game file preparation was interrupted.")?
    {
        return Err(error);
    }
    update("activating", None)?;
    let p = progress
        .into_inner()
        .map_err(|_| "Game file progress was interrupted.")?;
    Ok((p.fetched, p.reused))
}
