"""Exercise the account toolbar and Play requirements in the built Svelte renderer."""
import argparse
import json
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from threading import Thread

from playwright.sync_api import expect, sync_playwright


BRIDGE = r"""
status => {
  const profile = {id: '0ddfed53-76d4-47f4-ab31-8cd392aa4a03', name: 'Test instance',
    version: '26.3', memoryMiB: 2048, windowWidth: 1280, windowHeight: 720};
  window.accountDialogCalls = 0;
  window.__TAURI_INTERNALS__ = {
    transformCallback() { return 1; },
    async invoke(command) {
      switch (command) {
        case 'personal_library': return [profile];
        case 'minecraft_account_status': return status;
        case 'show_minecraft_accounts': window.accountDialogCalls += 1; return;
        default: throw new Error('Unhandled in account renderer test: ' + command);
      }
    }
  };
}
"""


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, *_):
        pass


def verify(browser, address, status, label, explanation):
    page = browser.new_page(viewport={"width": 1040, "height": 760})
    try:
        page.add_init_script(f"({BRIDGE})({json.dumps(status)})")
        page.goto(address)
        toolbar = page.get_by_role("button", name=label, exact=True)
        expect(toolbar).to_be_visible()
        if status == "unsupported":
            expect(toolbar).to_be_disabled()
            assert page.evaluate("window.accountDialogCalls") == 0
        else:
            toolbar.click()
            page.wait_for_function("window.accountDialogCalls === 1")
            expect(toolbar).to_be_focused()
        if status in ("unsupported", "unavailable"):
            expect(page.get_by_role("button", name="Sign in with Microsoft", exact=True)).to_have_count(0)
        page.get_by_role("button", name="Play", exact=True).click()
        expect(page.get_by_role("heading", name="Play requirements")).to_be_focused()
        expect(page.get_by_text(explanation, exact=True)).to_be_visible()
        expect(page.get_by_text("Protected Minecraft launch is still in development.", exact=True)).to_be_visible()
    finally:
        page.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--channel", help="Installed Chromium browser channel, for example msedge")
    args = parser.parse_args()
    dist = Path(__file__).resolve().parents[1] / "dist"
    if not (dist / "index.html").is_file():
        parser.error("run bun run build first")
    handler = partial(QuietHandler, directory=dist)
    with ThreadingHTTPServer(("127.0.0.1", 0), handler) as server:
        Thread(target=server.serve_forever, daemon=True).start()
        try:
            address = f"http://127.0.0.1:{server.server_port}/"
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(channel=args.channel)
                verify(browser, address, "unavailable", "Microsoft account",
                       "Microsoft sign-in is unavailable. Open Microsoft account for details.")
                verify(browser, address, "unsupported", "Microsoft account",
                       "Minecraft account controls are unavailable on this platform.")
                verify(browser, address, "signIn", "Sign in with Microsoft",
                       "Verify your Minecraft account to check Java Edition entitlement.")
                verify(browser, address, "available", "Minecraft account",
                       "Verify your Minecraft account to check Java Edition entitlement.")
                verify(browser, address, "verified", "Minecraft account", "Microsoft account verified.")
                browser.close()
        finally:
            server.shutdown()
    print("Account renderer checks passed")


if __name__ == "__main__":
    main()
