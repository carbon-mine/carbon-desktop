// SPDX-License-Identifier: MPL-2.0
#[path = "../src-tauri/src/pack.rs"]
mod pack;

use std::{env, path::Path, sync::Barrier, thread, time::Instant};

const REPEATS: usize = 30;

fn inspect(path: &Path) -> String {
    match pack::review_file(path) {
        Ok(review) => serde_json::to_string(&review).expect("serializable preview"),
        Err(error) => error,
    }
}

fn timed(path: &Path, expected: &str) -> f64 {
    let start = Instant::now();
    let result = inspect(path);
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(result, expected);
    ms
}

fn main() {
    let files: Vec<_> = env::args_os().skip(1).collect();
    assert_eq!(files.len(), 3, "pass the supported, entries and descriptors cases");
    let cases: Vec<_> = files
        .iter()
        .map(|name| {
            let path = Path::new(name);
            (path, inspect(path))
        })
        .collect();
    let accepted: serde_json::Value = serde_json::from_str(&cases[0].1).expect("accepted case");
    assert_eq!(accepted["overrideCount"], 4095);
    assert_eq!(accepted["fileCount"], 512);
    assert_eq!(
        cases[1].1,
        "Pack contains duplicate, overlapping or unsupported archive entries."
    );
    assert_eq!(cases[2].1, "Pack index exceeds the 256 KiB preview limit.");
    for (path, result) in &cases {
        let samples: Vec<_> = (0..REPEATS).map(|_| timed(path, result)).collect();
        println!("{}", serde_json::json!({
            "case": path.file_stem().unwrap().to_str().unwrap(),
            "result": result,
            "milliseconds": samples,
        }));
    }
    let (path, result) = &cases[0];
    let pairs: Vec<f64> = (0..REPEATS)
        .map(|_| {
            let ready = Barrier::new(3);
            thread::scope(|scope| {
                let worker = || {
                    ready.wait();
                    timed(path, result)
                };
                let first = scope.spawn(worker);
                let second = scope.spawn(worker);
                let start = Instant::now();
                ready.wait();
                first.join().expect("first review");
                second.join().expect("second review");
                start.elapsed().as_secs_f64() * 1000.0
            })
        })
        .collect();
    println!("{}", serde_json::json!({
        "case": "two-parallel-supported-previews",
        "milliseconds": pairs,
    }));
}
