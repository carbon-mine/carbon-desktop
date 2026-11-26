# SPDX-License-Identifier: MPL-2.0
"""Observe the first usable frame from a GPUI process with a disposable library."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import re

def main(exe):
    with tempfile.TemporaryDirectory(prefix="carbon-native-smoke-") as folder:
        marker = Path(folder) / "ready"
        env = {**os.environ, "TMP": folder, "TEMP": folder, "TMPDIR": folder,
               "GPUI_DISABLE_DIRECT_COMPOSITION": "1"}
        process = subprocess.Popen([exe, "--comparison-review", "--ready-file", str(marker)], env=env)
        try:
            deadline = time.monotonic() + 60
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise RuntimeError(f"Launcher exited with {process.returncode} before the library was usable")
                if marker.exists() and re.fullmatch(r"CARBON_LIBRARY_READY [0-9]+\s*", marker.read_text()):
                    print(marker.read_text().strip())
                    return
                time.sleep(0.1)
            raise TimeoutError("The native launcher did not report a usable library frame in 60 seconds")
        finally:
            if process.poll() is None:
                process.terminate()
            process.wait(timeout=10)

if __name__ == "__main__":
    main(sys.argv[1])

