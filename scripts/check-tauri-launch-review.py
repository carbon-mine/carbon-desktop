# SPDX-License-Identifier: MPL-2.0
"""Check the explicit Svelte input review and stale-result handling."""
import argparse
import re
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from threading import Thread

from playwright.sync_api import expect, sync_playwright

PROFILE = "0ddfed53-76d4-47f4-ab31-8cd392aa4a03"
OTHER = "3cbf08c1-c5d3-4456-89c5-0bf3db2bff9c"
MESSAGE = "Review found verified inputs for Minecraft 26.3 with managed Java 25."
BRIDGE = """
([profileId, otherId, message]) => {
  const profile = {id: profileId, name: 'Launch review fixture', version: '26.3',
    memoryMiB: 2048, windowWidth: 1280, windowHeight: 720};
  const other = {...profile, id: otherId, name: 'Other review fixture'};
  const metadataSha1 = 'a'.repeat(40);
  window.commands = [];
  window.__TAURI_INTERNALS__ = {
    transformCallback() { return 1; },
    async invoke(command, args) {
      window.commands.push({command, args});
      if (command === 'personal_library') return [profile, other];
      if (command === 'inspect_vanilla_profile') {
        const installed = args.profileId === profileId && profile.version === '26.3';
        return {profileId: args.profileId, version: args.profileId === profileId ? profile.version : other.version,
          releaseType: 'release', javaMajor: 25, clientBytes: 1, assetIndexBytes: 1, libraryCount: 1,
          metadataSha1, origin: 'cache',
          runtime: {state: 'compatible', version: '25', source: 'Managed'},
          managedJava: {status: 'ready', major: 25}, clientFile: {status: installed ? 'verified' : 'missing'},
          gameFilesCached: installed, gameFilesPrepared: installed, sessionSupported: false, blockedReasons: []};
      }
      if (command === 'edit_profile_dialog') {
        if (window.changeVersion) profile.version = '1.21.1';
        else profile.memoryMiB += 512;
        return {profiles: [profile, other], selectedId: profileId};
      }
      if (command === 'prepare_game_files') {
        return new Promise(resolve => {
          window.completeFileVerification = () => resolve({profileId, version: profile.version,
            platform: 'windows-x86_64', metadataSha1, uniqueFiles: 1, totalBytes: 1, fetchedBytes: 0, reusedBytes: 1});
        });
      }
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
                page.add_init_script(f"({BRIDGE})(['{PROFILE}', '{OTHER}', '{MESSAGE}'])")
                page.goto(f"http://127.0.0.1:{server.server_port}/")
                play = page.get_by_role("button", name="Play", exact=True)
                play.click()
                expect(page.get_by_role("heading", name="Play requirements")).to_be_focused()
                assert all(entry["command"] != "review_installed_launch_inputs"
                           for entry in page.evaluate("window.commands"))
                review = page.get_by_role("button", name="Review installed launch inputs", exact=True)
                review.click()
                expect(page.get_by_role("region", name="Current instance task")).to_be_focused()
                progress = "Verifying installed game files and managed Java…"
                expect(page.get_by_role("status").get_by_text(progress, exact=True)).to_be_visible()
                expect(page.get_by_role("complementary", name="Activity").get_by_text(progress, exact=True)).to_be_visible()
                expect(review).to_be_disabled()
                verify = page.get_by_role("button", name="Verify game files", exact=True)
                expect(verify).to_be_disabled()
                page.get_by_role("tab", name="Settings").click()
                edit = page.get_by_role("button", name="Edit profile", exact=True)
                expect(edit).to_be_disabled()
                page.get_by_role("tab", name="Overview").click()
                commands = page.evaluate("window.commands")
                reviews = [entry["args"] for entry in commands
                           if entry["command"] == "review_installed_launch_inputs"]
                assert reviews == [{"profileId": PROFILE}], reviews
                assert all(entry["command"] not in ["run_sandbox_session", "prepare_game_files", "edit_profile_dialog"]
                           for entry in commands)
                page.evaluate("window.completeReview()")
                expect(page.get_by_text(MESSAGE, exact=True)).to_be_visible()
                review.click()
                page.locator(f"#instance-{OTHER}").click()
                play.click()
                expect(review).to_have_count(0)
                expect(page.get_by_text("A launch input review is in progress.", exact=True)).to_be_visible()
                expect(page.get_by_text("Prepare this instance to review its installed launch inputs.", exact=True)).to_be_visible()
                page.evaluate("window.completeReview()")
                expect(page.get_by_text("A launch input review is in progress.", exact=True)).to_have_count(0)
                result = page.get_by_text(MESSAGE, exact=True)
                expect(result).to_have_count(0)
                expect(page.get_by_text(re.compile("protected Minecraft launch", re.IGNORECASE))).to_be_visible()

                page.locator(f"#instance-{PROFILE}").click()
                for change in ["verify", "edit-settings", "edit-version"]:
                    review.click()
                    page.evaluate("window.completeReview()")
                    expect(result).to_be_visible()
                    if change == "verify":
                        verify.click()
                        expect(result).to_have_count(0)
                        page.evaluate("window.completeFileVerification()")
                        expect(review).to_be_enabled()
                    else:
                        page.evaluate(f"window.changeVersion = {str(change == 'edit-version').lower()}")
                        page.get_by_role("tab", name="Settings").click()
                        edit.click()
                        play.click()
                        expect(result).to_have_count(0)
                        if change == "edit-version":
                            expect(review).to_have_count(0)
                        else:
                            expect(review).to_be_enabled()
                browser.close()
        finally:
            server.shutdown()
    print("Launch input renderer bridge check passed")


if __name__ == "__main__":
    main()
