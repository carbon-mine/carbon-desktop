"""Capture the named Carbon window only on the script's private Xvfb display."""
import ctypes as c
import os
import signal
import subprocess
import sys
import time
from pathlib import Path

import gi

gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
gi.require_version("GdkX11", "3.0")
from gi.repository import Gdk, GdkX11, Gtk


def carbon_window():
    x11 = c.CDLL("libX11.so.6")
    display, window, pointer = c.c_void_p, c.c_ulong, c.POINTER
    x11.XOpenDisplay.argtypes, x11.XOpenDisplay.restype = [c.c_char_p], display
    x11.XDefaultRootWindow.argtypes, x11.XDefaultRootWindow.restype = [display], window
    x11.XQueryTree.argtypes = [display, window, pointer(window), pointer(window), pointer(pointer(window)), pointer(c.c_uint)]
    x11.XFetchName.argtypes = [display, window, pointer(c.c_void_p)]
    x11.XFree.argtypes = [c.c_void_p]
    x11.XCloseDisplay.argtypes = [display]
    connection = x11.XOpenDisplay(None)
    if not connection:
        raise RuntimeError("No connection to the private X11 display")
    children, parent, root, count = pointer(window)(), window(), window(), c.c_uint()
    try:
        if x11.XQueryTree(connection, x11.XDefaultRootWindow(connection), c.byref(root), c.byref(parent), c.byref(children), c.byref(count)):
            for i in range(count.value):
                name = c.c_void_p()
                if x11.XFetchName(connection, children[i], c.byref(name)) and name.value:
                    try:
                        if c.string_at(name) == b"Carbon Desktop":
                            return children[i]
                    finally:
                        x11.XFree(name)
    finally:
        if children:
            x11.XFree(children)
        x11.XCloseDisplay(connection)


def process_observations(pid):
    # Restrict emitted data to security status for this launcher and its WebKit children.
    processes = {}
    for status in Path("/proc").glob("[0-9]*/status"):
        try:
            fields = dict(line.split(":", 1) for line in status.read_text().splitlines())
            processes[int(status.parent.name)] = fields
        except (OSError, ValueError):
            continue
    launcher_depth = len(processes[pid]["NSpid"].split())
    sandbox_observed = False
    for found, status in processes.items():
        name = status["Name"].strip()
        if found != pid and not name.startswith("WebKit"):
            continue
        ancestor = found
        while ancestor in processes and ancestor != pid:
            ancestor = int(processes[ancestor]["PPid"])
        if ancestor == pid:
            depth = len(status["NSpid"].split())
            filters = status.get("Seccomp_filters", "unknown").strip()
            print(f"{name}: pid namespace depth {depth}, no-new-privileges {status['NoNewPrivs'].strip()}, seccomp {status['Seccomp'].strip()}, filters {filters}")
            if name.startswith("WebKitWebProces") and depth > launcher_depth and status["NoNewPrivs"].strip() == "1" and status["Seccomp"].strip() == "2" and filters.isdigit() and int(filters) > 0:
                sandbox_observed = True
    if not sandbox_observed:
        raise RuntimeError("Expected a nested WebKit web-content process with no-new-privileges and an active seccomp filter.")


def main():
    app, out = sys.argv[1], Path(sys.argv[2])
    Gtk.init([])
    process = subprocess.Popen([app, "--temporary-review", "--sample-vanilla"], start_new_session=True)
    first_content = None
    try:
        for _ in range(30):
            if process.poll() is not None:
                raise RuntimeError(f"Carbon exited with status {process.returncode}")
            time.sleep(0.6)
            xid = carbon_window()
            if xid:
                handle = GdkX11.X11Window.foreign_new_for_display(Gdk.Display.get_default(), xid)
                width, height = handle.get_width(), handle.get_height()
                shot = Gdk.pixbuf_get_from_window(handle, 0, 0, width, height)
                if shot:
                    pixels = shot.get_pixels()
                    colors = {pixels[i:i+3] for i in range(0, len(pixels)-2, 17*shot.get_n_channels())}
                    if len(colors) > 32:
                        if first_content is None:
                            first_content = time.monotonic()
                        if time.monotonic() - first_content < 3:
                            continue
                        shot.savev(str(out/"carbon-window.png"), "png", [], [])
                        process_observations(process.pid)
                        print(f"Captured Carbon Desktop window {width}x{height} ({len(colors)} sampled colors). Inspect the pixels to verify rendered content.")
                        return
        raise RuntimeError("No populated Carbon Desktop window captured within 18 seconds.")
    finally:
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(2)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()


if __name__ == "__main__":
    main()
