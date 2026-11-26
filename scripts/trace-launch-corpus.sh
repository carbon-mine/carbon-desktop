#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
set -euo pipefail

if [[ $# != 2 ]]; then
    printf 'Usage: %s INPUT_DIRECTORY TRACE_FILE\n' "$0" >&2
    exit 2
fi
export CARBON_LAUNCH_CORPUS_INPUT
CARBON_LAUNCH_CORPUS_INPUT=$(cd -- "$1" && pwd -P)
trace=$(realpath -m -- "$2")
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."

binary=$(cargo test --locked -p carbon-vanilla --lib --no-run --message-format=json |
    python3 -c 'import json,sys
for line in sys.stdin:
    event = json.loads(line)
    if event.get("profile", {}).get("test") and event.get("executable"):
        print(event["executable"])')
[[ -x $binary ]]
"${STRACE:-strace}" -qq -f -o "$trace" -e trace=%file,%process,%network "$binary" \
    pinned_official_corpus_is_offline_deterministic --ignored --nocapture

control=$(mktemp)
trap 'rm -f -- "$control"' EXIT
"${STRACE:-strace}" -qq -f -o "$trace.control" -e trace=%file,%process,%network python3 -B - "$control" <<'PY'
from pathlib import Path
import socket
import subprocess
import sys

with socket.socket(socket.AF_INET, socket.SOCK_STREAM):
    pass  # Instrument socket creation without connecting to anything.
Path(sys.argv[1]).write_text("trace witness")
subprocess.run(["/usr/bin/true"], check=True)
PY

python3 - "$trace" "$trace.control" <<'PY'
from pathlib import Path
import re
import sys

network = re.compile(r"\b(?:socket|socketpair|connect|bind|listen|accept4?|sendto|sendmsg|recvfrom|recvmsg)\(")
write = re.compile(
    r"\b(?:creat|unlink(?:at)?|rename(?:at2?)?|mkdir(?:at)?|rmdir|link(?:at)?|symlink(?:at)?|"
    r"chmod|fchmodat|chown|fchownat|truncate|utimensat)\("
    r"|\bopen(?:at2?)?\(.*\b(?:O_WRONLY|O_RDWR|O_CREAT|O_TRUNC|O_TMPFILE)\b"
)
def counts(file):
    calls = Path(file).read_text().splitlines()
    execs = sum(bool(re.search(r"\bexecve(?:at)?\(", line)) for line in calls)
    forks = sum(bool(re.search(r"\b(?:v?fork|clone3?)\(", line))
                and "CLONE_THREAD" not in line and "<... " not in line for line in calls)
    attempts = sum(bool(network.search(line)) for line in calls)
    mutations = sum(bool(write.search(line)) for line in calls)
    return execs, forks, attempts, mutations

control_execs, control_helpers, control_network, control_writes = counts(sys.argv[2])
if control_execs < 2 or not (control_helpers and control_network and control_writes):
    sys.exit("Trace control failed to detect each known process, socket and write operation.")
print("Trace control detected the deliberate helper, network socket and file write.")
effect_counts = counts(sys.argv[1])
if effect_counts != (1, 0, 0, 0):
    sys.exit("Unexpected effects: %d exec attempts, %d helpers, %d network calls, %d file mutation calls."
             % effect_counts)
print("One initial test process, zero helper attempts, zero network calls and zero file mutation calls.")
PY
