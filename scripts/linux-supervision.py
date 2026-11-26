#!/usr/bin/python3
# SPDX-License-Identifier: MPL-2.0
"""CDESK-22 Linux process-tree qualification for a fixed, disposable fixture."""

import os
from pathlib import Path
import selectors
import signal
import stat
import subprocess
import sys
import time


BWRAP = "/usr/bin/bwrap"
PYTHON = "/usr/bin/python3"
GRACE_SECONDS = 0.20
WAIT_SECONDS = 5


def emit(event):
    os.write(1, (event + "\n").encode("ascii"))


def fixture(ready, release):
    # The root and child exit independently of the grandchild. Only the grandchild
    # retains stdout until its actual exit, even if namespace init is killed.
    os.close(0)
    if os.fork() == 0 and os.fork() == 0:
        signal.signal(signal.SIGTERM, signal.SIG_IGN)
        os.write(ready, b"R")
        os.read(release, 1)
    os._exit(0)


def init():
    # Scope kill(-1) to this new PID namespace. Never send a broad host signal.
    if os.getpid() != 1 or os.stat("/proc/1").st_ino != os.stat("/proc/self").st_ino:
        raise RuntimeError("Trusted init must be PID 1 with its own procfs")
    emit("namespace-ready")
    if os.read(0, 1) != b"A":
        emit("drained-before-admission")
        return

    ready, ready_writer = os.pipe()
    release_reader, release = os.pipe()
    root = os.fork()
    if root == 0:
        os.close(ready)
        os.close(release)
        fixture(ready_writer, release_reader)
    os.close(ready_writer)
    os.close(release_reader)
    deadline = time.monotonic() + WAIT_SECONDS * 2
    stopping = None
    outcome = "natural"
    root_exited = False
    descendant_ready = False
    announced = False

    with selectors.DefaultSelector() as events:
        events.register(0, selectors.EVENT_READ)
        events.register(ready, selectors.EVENT_READ)
        while time.monotonic() < deadline:
            try:
                while (child := os.waitpid(-1, os.WNOHANG)[0]) != 0:
                    root_exited |= child == root
            except ChildProcessError:
                emit("drained-" + outcome)
                return

            if not announced and root_exited and descendant_ready:
                emit("root-exited-descendant-live")
                announced = True

            for key, _ in events.select(0.01):
                if key.fd == ready:
                    descendant_ready = os.read(ready, 1) == b"R"
                    events.unregister(ready)
                    continue
                command = os.read(0, 1)
                if not command:
                    events.unregister(0)
                if command == b"R" and announced and stopping is None:
                    os.write(release, b"R")
                if command in (b"S", b"") and stopping is None:
                    outcome = "graceful"
                    stopping = time.monotonic() + GRACE_SECONDS
                    signal_tree(signal.SIGTERM)

            if stopping is not None and time.monotonic() >= stopping:
                outcome = "forced"
                signal_tree(signal.SIGKILL)

    # Exiting PID 1 triggers kernel teardown. No drain event is issued on timeout.
    raise RuntimeError("Timed out observing fixture drain")


def signal_tree(sig):
    try:
        os.kill(-1, sig)
    except ProcessLookupError:
        # A process may exit between waitpid(WNOHANG) and signal delivery.
        pass


def spawn(script, stdout=subprocess.PIPE):
    if os.getuid() == 0 or os.getuid() != os.geteuid() or os.getgid() != os.getegid():
        raise RuntimeError("Run as an ordinary user")
    for path in (BWRAP, PYTHON):
        info = os.stat(path)
        if info.st_uid != 0 or info.st_mode & (stat.S_ISUID | stat.S_ISGID | 0o022):
            raise RuntimeError("Expected a system-owned, non-setuid executable: " + path)

    return subprocess.Popen(
        [
            BWRAP, "--unshare-user", "--unshare-pid", "--unshare-ipc", "--unshare-net",
            "--unshare-uts", "--disable-userns", "--as-pid-1", "--die-with-parent",
            "--new-session", "--cap-drop", "ALL", "--clearenv", "--ro-bind", "/usr", "/usr",
            "--symlink", "usr/lib", "/lib", "--symlink", "usr/lib64", "/lib64",
            "--proc", "/proc", "--ro-bind", script, "/fixture.py", "--chdir", "/",
            "--", PYTHON, "-I", "/fixture.py", "--init",
        ],
        stdin=subprocess.PIPE,
        stdout=stdout,
        env={"LANG": "C"},
        bufsize=0,
    )


class Fixture:
    def __init__(self, process):
        self.process = process
        self.pending = b""

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.process.stdin.close()
        try:
            self.process.wait(WAIT_SECONDS)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(WAIT_SECONDS)
        finally:
            self.process.stdout.close()

    def command(self, command):
        self.process.stdin.write(command)

    def expect(self, event, *alternatives):
        wanted = tuple(value.encode("ascii") for value in (event, *alternatives))
        deadline = time.monotonic() + WAIT_SECONDS
        with selectors.DefaultSelector() as events:
            events.register(self.process.stdout, selectors.EVENT_READ)
            while b"\n" not in self.pending:
                if not events.select(max(0, deadline - time.monotonic())):
                    raise RuntimeError("Timed out waiting for " + event)
                chunk = os.read(self.process.stdout.fileno(), 4096)
                if not chunk:
                    if b"" not in wanted or self.pending:
                        raise RuntimeError("Fixture closed before " + event)
                    return ""
                self.pending += chunk
        line, self.pending = self.pending.split(b"\n", 1)
        if line not in wanted:
            raise RuntimeError(f"Expected {wanted!r}, received {line!r}")
        return line.decode("ascii")

    def finished(self):
        self.expect("")
        if self.process.wait(WAIT_SECONDS) != 0:
            raise RuntimeError("Trusted fixture exited unsuccessfully")


def self_test(script):
    with Fixture(spawn(script)) as cancelled:
        cancelled.expect("namespace-ready")
        cancelled.command(b"S")
        cancelled.expect("drained-before-admission")
        cancelled.finished()

    with Fixture(spawn(script)) as first, Fixture(spawn(script)) as second:
        for run in (first, second):
            run.expect("namespace-ready")
            run.command(b"A")
            run.expect("root-exited-descendant-live")
        started = time.monotonic()
        first.command(b"SS")
        first.expect("drained-forced")
        first.finished()
        forced_ms = round((time.monotonic() - started) * 1000)
        second.command(b"R")
        second.expect("drained-natural")
        second.finished()

    owner_death(script, admit=False)
    owner_ms = owner_death(script, admit=True)
    print("PASS admission cancellation and loss of owner before admission")
    print(f"PASS repeated stop drained an uncooperative descendant in {forced_ms} ms")
    print("PASS the second tree remained alive and drained naturally")
    print(f"PASS owner death closed the descendant witness in {owner_ms} ms")


def owner_death(script, admit):
    # A separate observer holds the read end and can witness EOF without the
    # owner. EOF supplies no graceful outcome or application save evidence.
    with Fixture(subprocess.Popen(
        [PYTHON, "-I", script, "--owner"],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, bufsize=0, env={"LANG": "C"},
    )) as orphan:
        orphan.expect("namespace-ready")
        if admit:
            orphan.command(b"A")
            orphan.expect("root-exited-descendant-live")
        started = time.monotonic()
        orphan.process.kill()
        if orphan.process.wait(WAIT_SECONDS) != -signal.SIGKILL:
            raise RuntimeError("The external owner did not die by SIGKILL")
        if admit:
            orphan.expect("")
        elif orphan.expect("", "drained-before-admission"):
            orphan.expect("")
        return round((time.monotonic() - started) * 1000)


if __name__ == "__main__":
    script = str(Path(__file__).resolve())
    if sys.argv[1:] == ["--init"]:
        init()
    elif sys.argv[1:] == ["--owner"]:
        owner = spawn(script, stdout=None)
        try:
            if os.read(0, 1) == b"A":
                owner.stdin.write(b"A")
            else:
                owner.stdin.close()
            owner.wait(WAIT_SECONDS * 2)
        finally:
            owner.stdin.close()
    elif not sys.argv[1:]:
        try:
            self_test(script)
        except (OSError, RuntimeError, subprocess.TimeoutExpired) as error:
            sys.exit(f"Linux supervision fixture unavailable or incomplete: {error}")
    else:
        sys.exit("Usage: /usr/bin/python3 scripts/linux-supervision.py")
