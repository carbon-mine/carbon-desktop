#!/usr/bin/env python3
"""Exercise live GPUI controls through an existing private AT-SPI desktop session."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi, GLib

UNAVAILABLE_ACCOUNT = (
    "Minecraft account sign-in is not configured. "
    "Use account setup or the Minecraft Launcher."
)
LAUNCHER_FALLBACK = (
    "Opens the installed Minecraft Launcher on Windows when available, "
    "or the official download page."
)


def descendants(element):
    yield element
    for index in range(element.get_child_count()):
        child = element.get_child_at_index(index)
        if child is not None:
            yield from descendants(child)


def describe(root, path):
    nodes = [
        {"role": node.get_role_name(), "name": node.get_name(),
         "interfaces": node.get_interfaces()}
        for node in descendants(root)
    ]
    path.write_text(json.dumps(nodes, ensure_ascii=False, indent=2), encoding="utf-8")
    return len(nodes)


def run(executable, evidence, activate_x11, expect_unavailable_account=False):
    evidence.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="carbon-atspi-fixture-") as directory:
        private = Path(directory)
        output = (evidence / "linux-app.log").open("w", encoding="utf-8")
        app = subprocess.Popen(
            [executable, "--comparison-review", "--ready-file", str(private / "ready")],
            env={**os.environ, "TMPDIR": directory},
            stdout=output,
            stderr=subprocess.STDOUT,
        )

        def wait(read, detail, seconds=15):
            deadline = time.monotonic() + seconds
            while time.monotonic() < deadline:
                if app.poll() is not None:
                    raise RuntimeError(f"GPUI exited with {app.returncode}. Inspect linux-app.log.")
                try:
                    context = GLib.MainContext.default()
                    while context.pending():
                        context.iteration(False)
                    result = read()
                    if result:
                        return result
                except GLib.Error:
                    pass  # A committed render can replace a platform element between calls.
                time.sleep(0.12)
            raise RuntimeError(detail)

        def window(name):
            for item in descendants(Atspi.get_desktop(0)):
                if item.get_role_name() == "frame" and item.get_name() == name:
                    return item
            return None

        def foreground(title):
            if not activate_x11:
                return
            result = subprocess.run(
                ["xdotool", "search", "--sync", "--onlyvisible", "--all", "--pid", str(app.pid), "--name", title],
                check=True, capture_output=True, text=True, timeout=15,
            )
            identifier = result.stdout.splitlines()[0]
            subprocess.run(["xdotool", "windowactivate", "--sync", identifier], check=True, timeout=10)
            subprocess.run(["xdotool", "windowfocus", "--sync", identifier], check=True, timeout=10)
            # Wake the idle offscreen X11 loop without changing app or input state.
            subprocess.run(["xdotool", "key", "Shift_L"], check=True, timeout=10)

        def named(root, name, role=None):
            return next((element for element in descendants(root)
                         if element.get_name() == name
                         and (role is None or element.get_role_name() == role)), None)

        def activate(control):
            action = control.get_action_iface()
            assert action is not None and action.get_n_actions() > 0
            assert action.do_action(0)

        def set_text(control, value):
            editable = control.get_editable_text_iface()
            assert editable is not None, control.get_interfaces()
            assert editable.set_text_contents(value)
            wait(lambda: text(control) == value, f"Text update to {value!r} did not reach GPUI.")

        def text(control):
            control.clear_cache()
            readable = control.get_text_iface()
            return Atspi.Text.get_text(readable, 0, readable.get_character_count())

        def has_state(control, state):
            control.clear_cache()
            return control.get_state_set().contains(state)

        missing_focus = []

        def verify_focus(control, label):
            try:
                wait(lambda: has_state(control, Atspi.StateType.FOCUSED), label, seconds=3)
            except RuntimeError:
                if app.poll() is not None:
                    raise
                missing_focus.append(label)

        try:
            main = wait(lambda: window("Carbon Desktop"), "Carbon GPUI did not register a frame.")
            foreground("Carbon Desktop")
            if expect_unavailable_account:
                account = wait(lambda: named(main, "Minecraft account", "push button"),
                               "Missing neutral account button for an unconfigured build.")
                assert named(main, "Sign in with Microsoft", "push button") is None
                activate(account)
                dialog = wait(lambda: window("Minecraft account"), "Account frame missing.")
                foreground("Minecraft account")
                wait(lambda: named(dialog, "Minecraft account", "heading"),
                     "Account dialog did not identify itself as the account window.")
                wait(lambda: named(dialog, UNAVAILABLE_ACCOUNT),
                     "Account dialog did not explain unavailable sign-in.")
                assert named(dialog, "Sign in with Microsoft", "push button") is None
                wait(lambda: named(dialog, LAUNCHER_FALLBACK),
                     "Account dialog did not describe the launcher fallback.")
                assert named(dialog, "Open Minecraft Launcher", "push button") is not None
                assert named(dialog, "Open Microsoft browser") is None
                close = wait(lambda: named(dialog, "Close", "push button"),
                             "Unavailable account dialog has no Close action.")
                activate(close)
                wait(lambda: window("Minecraft account") is None, "Account dialog did not close.")
                foreground("Carbon Desktop")
                print("PASS unconfigured account button, explanation, "
                      "launcher fallback, and Close.")
            search = wait(lambda: named(main, "Find an instance"), "Missing actual GPUI search.")
            main_count = describe(main, evidence / "linux-main-tree.json")
            assert main_count >= 100
            focus = search.get_component_iface()
            assert focus is not None and focus.grab_focus()
            set_text(search, "099")
            verify_focus(search, "Search")
            option = wait(lambda: named(main, "Review instance 099, Vanilla 24w14a"),
                          "Filtered profile missing.")
            activate(option)
            wait(lambda: named(main, "Review instance 099", "heading"),
                 "AT-SPI selection did not update the real profile detail.")
            assert has_state(option, Atspi.StateType.SELECTED)

            new = wait(lambda: named(main, "New instance") or named(main, "+  New instance"),
                       "New instance command missing.")
            activate(new)
            popup = wait(lambda: window("New vanilla instance"), "Create frame missing.")
            foreground("New vanilla instance")
            editor = wait(lambda: named(popup, "Instance name"), "Editable profile name missing.")
            popup_count = describe(popup, evidence / "linux-dialog-tree.json")
            for field in ("Maximum memory (MiB)", "Window width", "Window height"):
                assert text(named(popup, field))
            default_version = named(popup, "26.3")
            assert default_version.get_component_iface().grab_focus()
            verify_focus(default_version, "Minecraft version choice")
            save = named(popup, "Create instance")
            activate(save)
            wait(lambda: any(item.get_name().startswith("Could not save this instance")
                             for item in descendants(popup)),
                 "Rejected Create did not expose an alert.")
            set_text(editor, "AccessKit verification fixture")
            activate(save)
            wait(lambda: window("New vanilla instance") is None, "Valid form did not close.")
            wait(lambda: named(main, "AccessKit verification fixture", "heading"),
                 "Created profile did not appear in the real detail view.")
            profile_file = next(private.glob("carbon-gpui-review-*/library/personal/profiles.json"))
            matches = [p for p in json.loads(profile_file.read_text())["profiles"]
                       if p["name"] == "AccessKit verification fixture"]
            assert len(matches) == 1
            print(f"AT-SPI main nodes={main_count} dialog nodes={popup_count}. "
                  "Search value, filter, selection, dialog, validation and persistence verified.")
            if missing_focus:
                raise RuntimeError(f"AT-SPI focused state did not reach: {', '.join(missing_focus)}")
            print("PASS AT-SPI search and version-choice focus.")
        finally:
            app.terminate()
            app.wait(timeout=8)
            output.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    parser.add_argument("evidence", type=Path)
    parser.add_argument("--activate-x11", action="store_true", help="Activate the disposable X11 fixture with xdotool")
    parser.add_argument("--expect-unavailable-account", action="store_true",
                        help="Check the account window in a build without a Microsoft registration")
    args = parser.parse_args()
    Atspi.init()
    run(args.executable.resolve(), args.evidence.resolve(), args.activate_x11,
        args.expect_unavailable_account)
