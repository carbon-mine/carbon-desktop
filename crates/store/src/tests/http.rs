//! Test-only loopback HTTP fixture. Production has no network supplier.
use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

const LARGE: [Blob; 6] = [
    Blob {
        kib: 629146,
        digest: "645fcdc7157df763eba5eedb0f9b8c606b5202c3b6e5e567a613fcabc242a8a8",
        ..ALL[0]
    },
    Blob {
        kib: 157287,
        digest: "c0858024de348b4f979ea6ccaaebdecafeb7bcab41b02c354cc742f908ca5bcf",
        ..ALL[1]
    },
    Blob {
        kib: 209715,
        digest: "a1832845e7d3eed02f6e7a4cceabbb3608c12e27323b7656ddd0e3555bd479e2",
        ..ALL[2]
    },
    Blob {
        kib: 209715,
        digest: "ad592e81e1e484c5469a764d470eba71ce53bd3b21c221e32df18b34f348f8c9",
        ..ALL[3]
    },
    Blob {
        kib: 52428,
        digest: "dff60f378d358d72b120753d8917220198f726469a65caaabaf4f73182d2e3e5",
        ..ALL[4]
    },
    Blob {
        kib: 52428,
        digest: "359de2e2211626dce5d043de1e8d335b9d61399551f67336f824e2ba272174f1",
        ..ALL[5]
    },
];
#[derive(Default)]
struct Counts {
    requests: AtomicU64,
    body: AtomicU64,
    stop: AtomicBool,
    truncate_next: AtomicBool,
}
struct Origin {
    addr: SocketAddr,
    counts: Arc<Counts>,
    worker: Option<JoinHandle<()>>,
}
impl Origin {
    fn start(catalog: &'static [Blob; 6]) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let counts = Arc::new(Counts::default());
        let reports = Arc::clone(&counts);
        let worker = thread::spawn(move || {
            for stream in listener.incoming() {
                if reports.stop.load(Ordering::SeqCst) {
                    break;
                }
                serve(stream.unwrap(), catalog, &reports).unwrap();
            }
        });
        Self {
            addr,
            counts,
            worker: Some(worker),
        }
    }
    fn get(&self, blob: &Blob) -> io::Result<Vec<u8>> {
        let mut stream = TcpStream::connect_timeout(&self.addr, Duration::from_secs(5))?;
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        write!(
            stream,
            "GET /objects/{} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            blob.digest, self.addr
        )?;
        let mut reader = BufReader::new(stream);
        for expected in [
            "HTTP/1.1 200 OK\r\n".to_owned(),
            format!("Content-Length: {}\r\n", blob.len()),
            "Connection: close\r\n".to_owned(),
            "\r\n".to_owned(),
        ] {
            let mut line = String::new();
            (&mut reader).take(1024).read_line(&mut line)?;
            if line != expected {
                return Err(io::Error::other("Unexpected fixture HTTP response"));
            }
        }
        let mut body = vec![0; blob.len() as usize];
        reader.read_exact(&mut body)?;
        if reader.read(&mut [0; 1])? != 0 {
            return Err(io::Error::other("Long fixture HTTP body"));
        }
        Ok(body)
    }
}
impl Drop for Origin {
    fn drop(&mut self) {
        self.counts.stop.store(true, Ordering::SeqCst);
        // Wake the idle accept loop without requesting an artifact.
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_secs(2));
        if let Some(worker) = self.worker.take() {
            let result = worker.join();
            if !thread::panicking() {
                result.unwrap();
            }
        }
    }
}
fn serve(mut stream: TcpStream, catalog: &[Blob; 6], counts: &Counts) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    let mut request = BufReader::new(&stream).take(1024);
    let mut line = String::new();
    request.read_line(&mut line)?;
    let selected = catalog
        .iter()
        .position(|b| line == format!("GET /objects/{} HTTP/1.1\r\n", b.digest));
    loop {
        line.clear();
        if request.read_line(&mut line)? == 0 {
            return Err(io::Error::other("Incomplete fixture request"));
        }
        if line == "\r\n" {
            break;
        }
    }
    let Some(index) = selected else {
        return stream.write_all(
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
    };
    let length = catalog[index].len();
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n"
    )?;
    let tile = ALL[index].source();
    let mut remaining = if counts.truncate_next.swap(false, Ordering::SeqCst) {
        // Deliberately close halfway through an otherwise correctly sized response.
        length as usize / 2
    } else {
        length as usize
    };
    while remaining != 0 {
        let size = remaining.min(tile.len());
        stream.write_all(&tile[..size])?;
        counts.body.fetch_add(size as u64, Ordering::SeqCst);
        remaining -= size;
    }
    counts.requests.fetch_add(1, Ordering::SeqCst);
    Ok(())
}
fn measure(catalog: &'static [Blob; 6]) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let origin = Origin::start(catalog);
    let source = |blob: &Blob| origin.get(blob).map_err(|e| e.to_string());
    let act = |action| {
        apply_locked(Store::open_fixture(root, catalog, &source).unwrap(), action).unwrap()
    };
    let total = catalog[0].len() + catalog[1].len() + catalog[2].len() + catalog[4].len();
    let overlap = catalog[3].len();
    let change = catalog[5].len();
    let mut previous = (0, 0, 0, 0);
    let mut cold_allocation = None;
    for (name, action, requests, transfer, reuse, logical, cache) in [
        ("cold", CreateSample, 4, total, 0, total, total),
        ("repeat", CreateRepeat, 0, 0, total, 2 * total, total),
        (
            "overlap",
            CreateOverlap,
            1,
            overlap,
            total - overlap,
            3 * total,
            total + overlap,
        ),
        (
            "review",
            Action::Review { instance: Sample },
            0,
            0,
            0,
            3 * total,
            total + overlap,
        ),
        (
            "stage",
            Stage { instance: Sample },
            1,
            change,
            total - change,
            3 * total,
            total + overlap + change,
        ),
        (
            "activate",
            Activate { instance: Sample },
            0,
            0,
            0,
            3 * total,
            total + overlap + change,
        ),
    ] {
        let snapshot = act(action);
        let m = snapshot.metrics;
        if matches!(action, CreateSample) {
            cold_allocation = m.allocated_object_bytes;
        } else if matches!(action, CreateRepeat) {
            assert_eq!(m.allocated_object_bytes, cold_allocation);
        }
        let server_count = origin.counts.requests.load(Ordering::SeqCst);
        let server_bytes = origin.counts.body.load(Ordering::SeqCst);
        assert_eq!(server_count - previous.0, requests);
        assert_eq!(server_bytes - previous.1, transfer);
        assert_eq!(m.source_bytes - previous.2, transfer);
        assert_eq!(m.reused_bytes - previous.3, reuse);
        assert_eq!((m.active_logical_bytes, m.object_bytes), (logical, cache));
        println!("{name}: GETs={requests} HTTP-body={transfer} reuse={reuse} active={logical} objects={cache} allocated={:?}",m.allocated_object_bytes);
        previous = (server_count, server_bytes, m.source_bytes, m.reused_bytes);
    }
    let edited = act(EditSave { instance: Sample });
    assert_eq!(view(&edited, Sample).save_revision, 1);
    assert_eq!(view(&edited, Repeat).save_revision, 0);
    assert_eq!(view(&edited, Overlap).save_revision, 0);
    let edited = act(EditSetting { instance: Overlap });
    assert_eq!(view(&edited, Sample).setting, 0);
    assert_eq!(view(&edited, Repeat).setting, 0);
    assert_eq!(view(&edited, Overlap).setting, 1);
}
#[test]
fn loopback_http_cold_repeat_overlap_update() {
    measure(&ALL);
}

#[test]
#[ignore = "Transfers and temporarily stores 1.25 GiB on the default temp volume"]
fn loopback_http_one_gib() {
    measure(&LARGE);
}

#[test]
fn truncated_loopback_body_preserves_active_generation_and_can_resume() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let origin = Origin::start(&ALL);
    let source = |blob: &Blob| origin.get(blob).map_err(|e| e.to_string());
    let act = |action| apply_locked(Store::open_fixture(root, &ALL, &source).unwrap(), action);
    act(CreateSample).unwrap();
    act(EditSave { instance: Sample }).unwrap();
    act(Action::Review { instance: Sample }).unwrap();
    origin.counts.truncate_next.store(true, Ordering::SeqCst);
    assert!(act(Stage { instance: Sample }).is_err());
    let reopened = snapshot(root).unwrap();
    let current = view(&reopened, Sample);
    assert_eq!(
        (
            current.generation,
            current.phase,
            current.healthy,
            current.save_revision
        ),
        (Generation::A, Phase::Interrupted, true, 1)
    );
    assert!(!root.join("objects").join(ALL[5].digest).exists());
    act(Resume { instance: Sample }).unwrap();
    let updated = act(Activate { instance: Sample }).unwrap();
    assert_eq!(view(&updated, Sample).generation, Generation::B);
    assert_eq!(view(&updated, Sample).save_revision, 1);
    assert_eq!(
        origin.counts.body.load(Ordering::SeqCst),
        updated.metrics.source_bytes + ALL[5].len() / 2
    );
}
