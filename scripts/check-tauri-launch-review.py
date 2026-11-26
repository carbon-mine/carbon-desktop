# SPDX-License-Identifier: MPL-2.0
"""Check that the Svelte Play action requests only the redacted native input review."""
import argparse
import re
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from threading import Thread

from playwright.sync_api import expect, sync_playwright

PROFILE = "0ddfed53-76d4-47f4-ab31-8cd392aa4a03"
MESSAGE = "Installed launch inputs verified for Minecraft 26.3 with managed Java 25."
BRIDGE = """
([profileId, message]) => {
  const profile = {id: profileId, name: 'Launch review fixture', version: '26.3',
    memoryMiB: 2048, windowWidth: 1280, windowHeight: 720};
  window.commands = [];
  window.__TAURI_INTERNALS__ = {
    transformCallback() { return 1; },
    async invoke(command, args) {
      window.commands.push({command, args});
      if (command === 'personal_library') return [profile];
      if (command === 'review_installed_launch_inputs') {
        return new Promise(resolve => {
          window.completeReview = () => resolve({message});
        });
      }
      throw new Error('Unhandled in launch review test: ' + command);
    }
  };
}
"""


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, *_):
        pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--channel", help="Installed Chromium channel, for example msedge")
    args = parser.parse_args()
    dist = Path(__file__).resolve().parents[1] / "dist"
    if not (dist / "index.html").is_file():
        parser.error("run bun run build first")
    with ThreadingHTTPServer(("127.0.0.1", 0), partial(QuietHandler, directory=dist)) as server:
        Thread(target=server.serve_forever, daemon=True).start()
        try:
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(channel=args.channel)
                page = browser.new_page(viewport={"width": 1040, "height": 760})
                page.add_init_script(f"({BRIDGE})(['{PROFILE}', '{MESSAGE}'])")
                page.goto(f"http://127.0.0.1:{server.server_port}/")
                play = page.get_by_role("button", name="Play", exact=True)
                play.click()
                expect(page.get_by_role("heading", name="Play requirements")).to_be_focused()
                expect(page.get_by_text("Checking installed launch inputs…", exact=True)).to_be_visible()
                play.click()
                commands = page.evaluate("window.commands")
                reviews = [entry["args"] for entry in commands
                           if entry["command"] == "review_installed_launch_inputs"]
                assert reviews == [{"profileId": PROFILE}], reviews
                assert all(entry["command"] != "run_sandbox_session" for entry in commands)
                page.evaluate("window.completeReview()")
                expect(page.get_by_text(MESSAGE, exact=True)).to_be_visible()
                expect(page.get_by_text(re.compile("protected Minecraft launch", re.IGNORECASE))).to_be_visible()
                browser.close()
        finally:
            server.shutdown()
    print("Launch input renderer bridge check passed")


if __name__ == "__main__":
    main()
