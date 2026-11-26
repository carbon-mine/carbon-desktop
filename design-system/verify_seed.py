"""Check the byte-level provenance of the inherited Carbon Desktop foundations."""

import hashlib
import json
from pathlib import Path


def main():
    root = Path(__file__).resolve().parent
    lock = json.loads((root / "source-lock.json").read_text(encoding="utf-8"))
    groups = [lock["panel"]["files"]]
    groups.extend(package["files"] for package in lock["fontsource"].values())
    expected_paths = set()
    failures = []

    for group in groups:
        for relative_path, source in group.items():
            path = root / relative_path
            expected_paths.add(relative_path)
            if not path.is_file():
                failures.append(f"Missing file: {relative_path}")
            elif hashlib.sha256(path.read_bytes()).hexdigest() != source["sha256"]:
                failures.append(f"Different SHA-256: {relative_path}")

    actual_paths = {
        path.relative_to(root).as_posix()
        for path in (root / "inherited").rglob("*")
        if path.is_file()
    }
    failures.extend(f"Unmapped file: {path}" for path in sorted(actual_paths - expected_paths))
    if failures:
        raise SystemExit("\n".join(failures))

    print(f"Verified {len(expected_paths)} inherited files from Panel {lock['panel']['commit']}.")


if __name__ == "__main__":
    main()
