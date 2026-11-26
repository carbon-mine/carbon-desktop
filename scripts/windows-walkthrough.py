"""Run the native UIA walkthrough in a new disposable library. Requires pywinauto and Pillow."""
import argparse
import ctypes
import re
from pathlib import Path
import threading
import tempfile
import psutil
import subprocess
import time

import win32gui
import win32ui
import win32process
from PIL import Image
from pywinauto import Desktop
from pywinauto.keyboard import send_keys

ROOT = Path(__file__).resolve().parents[1]
EXE = ROOT / "target/debug/carbon-desktop.exe"
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--prepare-java", action="store_true", help="Download the pinned runtime in the disposable profile")
args = parser.parse_args()
OUT = ROOT / ".artifacts/windows-setup-review"
OUT.mkdir(parents=True, exist_ok=True)
proc = subprocess.Popen([str(EXE), "--temporary-review"])
start = time.perf_counter()
sampling_done = threading.Event()
memory_samples = []

def sample_memory():
    while not sampling_done.is_set():
        try:
            app = psutil.Process(proc.pid)
            processes = [app, *app.children(recursive=True)]
            total = 0
            for item in processes:
                try:
                    total += item.memory_info().private
                except (psutil.NoSuchProcess, psutil.AccessDenied):
                    pass
            memory_samples.append(total)
        except psutil.NoSuchProcess:
            return
        sampling_done.wait(.2)

sampler = threading.Thread(target=sample_memory, daemon=True)
sampler.start()
desktop = Desktop(backend="uia")
def owned_window():
    matches = []
    def visit(hwnd, unused):
        if (win32process.GetWindowThreadProcessId(hwnd)[1] == proc.pid
                and win32gui.IsWindowVisible(hwnd)
                and win32gui.GetWindowText(hwnd) == "Carbon Desktop"):
            matches.append(hwnd)
    win32gui.EnumWindows(visit, None)
    return matches[0] if matches else None

deadline = time.monotonic() + 25
hwnd = owned_window()
while not hwnd and time.monotonic() < deadline:
    time.sleep(.1)
    hwnd = owned_window()
assert hwnd, "Native review window did not open"
window = desktop.window(handle=hwnd)


def button(name, timeout=15):
    control = window.child_window(title_re=r"^\s*" + re.escape(name) + r"\s*$", control_type="Button")
    control.wait("exists enabled", timeout=timeout)
    return control.wrapper_object()


def press(name):
    button(name).invoke()


def text_seen(fragment, timeout=10):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if any(fragment.lower() in c.window_text().lower() for c in window.descendants()):
            return
        time.sleep(.15)
    raise AssertionError(f"Missing UI text: {fragment}")


def capture(name):
    time.sleep(.4)
    hwnd = window.handle
    left, top, right, bottom = win32gui.GetWindowRect(hwnd)
    w, h = right - left, bottom - top
    hdc = win32gui.GetWindowDC(hwnd)
    dc = win32ui.CreateDCFromHandle(hdc)
    bitmap_dc = dc.CreateCompatibleDC()
    bitmap = win32ui.CreateBitmap()
    bitmap.CreateCompatibleBitmap(dc, w, h)
    previous = bitmap_dc.SelectObject(bitmap)
    try:
        assert ctypes.windll.user32.PrintWindow(hwnd, bitmap_dc.GetSafeHdc(), 2)
        image = Image.frombytes("RGB", (w,h), bitmap.GetBitmapBits(True), "raw", "BGRX")
        image.save(OUT / (name + ".png"))
    finally:
        bitmap_dc.SelectObject(previous)
        bitmap_dc.DeleteDC()
        win32gui.ReleaseDC(hwnd, hdc)
        win32gui.DeleteObject(bitmap.GetHandle())


def resize(w, h):
    left, top = window.rectangle().left, window.rectangle().top
    ctypes.windll.user32.SetWindowPos(window.handle, 0, max(8, left), max(8, top), w, h, 0x0004)
    time.sleep(.35)


def select_tab(name):
    tab = window.child_window(auto_id="tab-" + name, control_type="TabItem")
    tab.wait("visible enabled", timeout=10)
    tab.set_focus()
    assert win32gui.GetForegroundWindow() == hwnd
    send_keys("{SPACE}")


def native_dialog(title):
    dialog = window.child_window(title=title, control_type="Window")
    if not dialog.exists(timeout=1):
        dialog = desktop.window(title=title, process=proc.pid, control_type="Window")
    dialog.wait("visible", timeout=10)
    return dialog


try:
    window.wait("visible", timeout=25)
    window.set_focus()
    button("New instance")
    print(f"Empty library interactive in {time.perf_counter()-start:.2f}s", flush=True)
    capture("first-use")
    press("New instance")
    name_input = window.child_window(auto_id="profile-name", control_type="Edit")
    name_input.wait("visible")
    name_input.set_edit_text("Vanilla survival")
    press("Create profile")
    button("Check setup")
    capture("library")
    press("Check setup")
    button("Choose Java folder", timeout=60)
    text_seen("Java 25")
    capture("setup")
    if args.prepare_java:
        press("Prepare managed Java")
        button("Verify managed Java", timeout=300)
        text_seen("Carbon-managed Temurin")
        capture("java-prepared")
    press("Import client JAR")
    dialog = native_dialog("Select the Minecraft 26.3 client JAR")
    dialog.child_window(auto_id="2", control_type="Button").invoke()
    button("Import client JAR")
    with tempfile.TemporaryDirectory(prefix="carbon-jar-review-") as files:
        invalid_jar = Path(files) / "invalid.jar"
        invalid_jar.write_bytes(b"Carbon review fixture with an incorrect size")
        press("Import client JAR")
        dialog = native_dialog("Select the Minecraft 26.3 client JAR")
        filename = dialog.child_window(auto_id="1148", control_type="ComboBox").child_window(control_type="Edit")
        filename.set_edit_text(str(invalid_jar))
        dialog.child_window(auto_id="1", control_type="Button").invoke()
        text_seen("wrong size")
        press("Check setup")
        button("Import client JAR", timeout=60)
    select_tab("Settings")
    press("Edit profile")
    memory = window.child_window(auto_id="profile-memory", control_type="Spinner")
    memory.wait("visible enabled", timeout=10)
    memory.iface_range_value.SetValue(3072)
    assert int(memory.iface_range_value.CurrentValue) == 3072
    press("Save changes")
    text_seen("3072 MiB")
    press("Clone profile")
    name_input.set_edit_text("Independent copy")
    press("Create copy")
    text_seen("Independent copy")
    resize(560, 740)
    back = window.child_window(title_re=".*Back to Library", control_type="Button")
    if back.exists(timeout=1) and back.is_visible():
        back.invoke()
    capture("narrow-library")
    window.set_focus()
    assert win32gui.GetForegroundWindow() == hwnd
    send_keys("^f")
    text_seen("Find an instance")
    search = window.child_window(auto_id="search", control_type="Edit")
    deadline = time.monotonic() + 3
    while not search.has_keyboard_focus() and time.monotonic() < deadline:
        time.sleep(.1)
    assert search.has_keyboard_focus(), "Ctrl+F did not focus the native webview's search field"
    send_keys("Vanilla{TAB}{ENTER}")
    back.wait("visible")
    capture("narrow-instance")
    back.invoke()
    resize(1040, 760)
    press("Review tools")
    press("Install Forest sample")
    press("Check Play requirements")
    text_seen("Live Minecraft entitlement is unavailable")
    press("Review update")
    press("Simulate interruption")
    button("Resume update")
    capture("interrupted")
    press("Resume update")
    press("Activate B")
    press("Roll back to A")
    select_tab("Storage")
    press("Install exact repeat")
    text_seen("Forest repeat")
    select_tab("Storage")
    press("Install Coast sample")
    text_seen("Coast sample")
    press("Preview pack")
    dialog = native_dialog("Preview a local Minecraft pack")
    filename = dialog.child_window(auto_id="1148", control_type="ComboBox").child_window(control_type="Edit")
    filename.set_edit_text(str(ROOT / ".artifacts/samples/forest-preview.mrpack"))
    filename.set_focus()
    assert win32gui.GetForegroundWindow() == dialog.handle
    send_keys("{ENTER}")
    text_seen("Forest preview fixture")
    capture("pack-preview")
    press("Close preview")
    print("PASS personal setup, native pickers, settings, clone, narrow library, keyboard find, and fixture recovery/reuse.", flush=True)
except Exception:
    capture("failure")
    print("Current controls:", [(c.element_info.control_type, c.window_text()[:160])
          for c in window.descendants() if c.window_text()][-45:], flush=True)
    raise
finally:
    sampling_done.set()
    sampler.join(timeout=1)
    if memory_samples:
        print(f"Peak sampled launcher-tree private bytes: {max(memory_samples) / 1048576:.1f} MiB", flush=True)
    # This PID belongs to the --temporary-review process created above.
    for modal in Desktop(backend="win32").windows(process=proc.pid, class_name="#32770"):
        win32gui.PostMessage(modal.handle, 0x10, 0, 0)
    if win32gui.IsWindow(hwnd):
        win32gui.PostMessage(hwnd, 0x10, 0, 0)
    try:
        proc.wait(timeout=8)
    except subprocess.TimeoutExpired:
        proc.terminate()
        proc.wait(timeout=8)
