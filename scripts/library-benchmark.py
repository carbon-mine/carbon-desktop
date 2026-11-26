#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Measure a headless renderer and the local read-only pack preview."""
import argparse
import hashlib
import json
import math
import mimetypes
import os
import platform
import shutil
import subprocess
import tomllib
from pathlib import Path
from urllib.parse import unquote, urlsplit
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo


ROOT = Path(__file__).resolve().parents[1]
DIST = ROOT / "dist"
OUTPUT = ROOT / ".artifacts" / "library-benchmark"


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def percentile(values, quantile):
    return round(sorted(values)[math.ceil(len(values) * quantile) - 1], 3)


def stats(values):
    return {
        "n": len(values),
        "p50Ms": percentile(values, 0.50),
        "p95Ms": percentile(values, 0.95),
        "maxMs": round(max(values), 3),
    }


def archive(name, entries, descriptors):
    hashes = {method: hashlib.new(method, b"x").hexdigest() for method in ("sha1", "sha512")}
    index = {
        "formatVersion": 1,
        "game": "minecraft",
        "name": name,
        "versionId": "benchmark-v1",
        "dependencies": {"minecraft": "1.21.1"},
        "files": [
            {"path": f"mods/{i:04}.jar", "fileSize": 1, "hashes": hashes,
             "downloads": [f"https://example.invalid/{i:04}"]}
            for i in range(descriptors)
        ],
    }
    index_bytes = json.dumps(index, separators=(",", ":")).encode()
    path = OUTPUT / f"{name}.mrpack"
    with ZipFile(path, "w") as pack:
        pack.writestr(ZipInfo("modrinth.index.json"), index_bytes, compress_type=ZIP_DEFLATED)
        for i in range(entries - 1):
            pack.writestr(ZipInfo(f"overrides/config/{i:04}.txt"), b"x")
    return path, {
        "entries": entries,
        "descriptors": descriptors,
        "indexBytes": len(index_bytes),
        "archiveBytes": path.stat().st_size,
        "sha256": sha256(path),
    }


def measure_archives():
    inputs = [
        archive("supported", 4096, 512),
        archive("large-entry-count", 5000, 0),
        archive("large-descriptor-count", 1, 5000),
    ]
    locked = tomllib.loads((ROOT / "Cargo.lock").read_text(encoding="utf-8"))["package"]
    versions = {package["name"]: package["version"] for package in locked}
    helper = OUTPUT / "pack-helper"
    helper.mkdir(exist_ok=True)
    manifest = helper / "Cargo.toml"
    manifest.write_text(
        '[package]\nname = "carbon-preview-benchmark"\nversion = "0.0.0"\nedition = "2021"\n'
        'publish = false\n\n[workspace]\n\n[[bin]]\nname = "carbon-preview-benchmark"\n'
        'path = "../../../scripts/library-benchmark-pack.rs"\n\n[dependencies]\n'
        f'serde = {{ version = "={versions["serde"]}", features = ["derive"] }}\n'
        f'serde_json = "={versions["serde_json"]}"\n'
        f'url = "={versions["url"]}"\n'
        f'zip = {{ version = "={versions["zip"]}", default-features = false, '
        'features = ["deflate-flate2-zlib-rs"] }\n'
        f'libc = "={versions["libc"]}"\n',
        encoding="utf-8",
    )
    shutil.copyfile(ROOT / "Cargo.lock", helper / "Cargo.lock")
    common = ["cargo", "run", "--release", "--offline", "--manifest-path", str(manifest),
              "--target-dir", str(OUTPUT / "target")]
    # Cargo removes unused workspace packages and adds the temporary harness to its lockfile.
    result = subprocess.run([*common, "--", *(str(path) for path, _ in inputs)],
                            cwd=ROOT, check=True, capture_output=True, text=True)
    by_name = {path.stem: details for path, details in inputs}
    return {
        "cases": [
            {**report, **by_name.get(report["case"], {}), **stats(report["milliseconds"])}
            for report in map(json.loads, result.stdout.splitlines())
        ],
        "helperLockSha256": sha256(helper / "Cargo.lock"),
        "parserSha256": sha256(ROOT / "src-tauri" / "src" / "pack.rs"),
        "rustc": subprocess.check_output(["rustc", "-V"], text=True).strip(),
    }


def measure_renderer(executable):
    from playwright.sync_api import sync_playwright

    def route_dist(route):
        request = urlsplit(route.request.url)
        if request.netloc != "benchmark.invalid":
            route.abort()
            return
        target = (DIST / unquote(request.path.lstrip("/") or "index.html")).resolve()
        if not target.is_relative_to(DIST) or not target.is_file():
            route.abort()
            return
        media_type = mimetypes.guess_type(target)[0] or "application/octet-stream"
        if target.suffix == ".js":
            media_type = "application/javascript"
        route.fulfill(path=str(target), content_type=media_type)

    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True, executable_path=executable)
        try:
            context = browser.new_context(viewport={"width": 1040, "height": 760}, device_scale_factor=1)
            context.route("**/*", route_dist)
            page = context.new_page()
            page.add_init_script(path=ROOT / "scripts" / "library-benchmark.js")
            page.goto("http://benchmark.invalid/")
            page.wait_for_function("document.querySelectorAll('.instance-entry').length === 100")
            measured = page.evaluate("window.runLibraryBenchmark()")
            assert measured["personalRows"] == 100 and measured["fixtureRows"] == 3
            return {
                "browser": browser.version,
                "viewportCssPixels": [1040, 760],
                "deviceScale": 1,
                "distSha256": {
                    path.relative_to(DIST).as_posix(): sha256(path)
                    for path in sorted(DIST.rglob("*")) if path.is_file()
                },
                "measured": measured,
                "percentiles": {name: stats(values) for name, values in measured.items()
                                if name.endswith("Ms")},
            }
        finally:
            browser.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--browser-executable", type=Path, help="Path to an installed Chromium or Edge")
    parser.add_argument("--only", choices=["archive", "renderer"], help="Measure a single surface")
    args = parser.parse_args()
    OUTPUT.mkdir(parents=True, exist_ok=True)
    result = {"platform": platform.platform(), "logicalProcessors": os.cpu_count()}
    if args.only != "renderer":
        result["archive"] = measure_archives()
    if args.only != "archive":
        result["renderer"] = measure_renderer(args.browser_executable)
    destination = OUTPUT / f"{args.only or 'all'}.json"
    destination.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(destination)
    print(json.dumps({
        "archive": [{key: value for key, value in case.items() if key != "milliseconds"}
                    for case in result.get("archive", {}).get("cases", [])],
        "renderer": result.get("renderer", {}).get("percentiles", {}),
    }, indent=2))


if __name__ == "__main__":
    main()
