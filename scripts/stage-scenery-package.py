#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Stage an offline Home scenery package beside the launcher.

The compiled catalog stays authoritative for view identity. This script copies
one H.264 movie and one poster per catalog view into ``<output>/scenery`` and
writes the manifest the launcher reads at startup. It never renders, encodes or
downloads artwork; the inputs must already exist locally.

Example:

    python scripts/stage-scenery-package.py \
        --source D:/renders/2560x1440 \
        --output target/debug

Then launch the built launcher, which finds ``scenery/`` beside its executable.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CATALOG = ROOT / "design-system" / "landscapes" / "manifest.json"


def catalog_ids() -> list[str]:
    manifest = json.loads(CATALOG.read_text(encoding="utf-8"))
    return [view["id"] for view in manifest["views"]]


def digest(path: Path) -> tuple[str, int]:
    sha = hashlib.sha256()
    size = 0
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            sha.update(block)
            size += len(block)
    return sha.hexdigest(), size


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, type=Path,
                        help="directory holding <id>.mp4 and <id>.jpg per catalog view")
    parser.add_argument("--output", required=True, type=Path,
                        help="launcher directory; the package is written to <output>/scenery")
    parser.add_argument("--fps", type=int, default=30)
    parser.add_argument("--frames", type=int, default=240,
                        help="frames per loop, used to resume a paused view")
    args = parser.parse_args()

    if not 1 <= args.fps <= 60 or not 1 <= args.frames <= 3600:
        parser.error("--fps must be 1-60 and --frames 1-3600")

    ids = catalog_ids()
    missing = [name for view in ids for name in (f"{view}.mp4", f"{view}.jpg")
               if not (args.source / name).is_file()]
    if missing:
        parser.error(f"missing source artwork: {', '.join(sorted(missing))}")

    package = args.output / "scenery"
    package.mkdir(parents=True, exist_ok=True)
    views = []
    total = 0
    for view in ids:
        for suffix in (".jpg", ".mp4"):
            name = f"{view}{suffix}"
            target = package / name
            shutil.copyfile(args.source / name, target)
            sha, size = digest(target)
            total += size
            print(f"{name:28} {size:>10} bytes  {sha}")
        views.append({
            "id": view,
            "poster": f"{view}.jpg",
            "motion": f"{view}.mp4",
            "fps": args.fps,
            "frames": args.frames,
        })

    manifest = package / "manifest.json"
    manifest.write_text(json.dumps({"version": 1, "views": views}, indent=2) + "\n",
                        encoding="utf-8")
    print(f"\n{len(views)} views, {total} bytes of artwork -> {manifest}")
    print("Bundled frame packs stay compiled in as the fallback for platforms without a native decoder.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
