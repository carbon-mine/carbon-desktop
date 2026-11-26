"""Exercise launcher setup, profiles and recovery with a browser-local Tauri bridge."""

import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from threading import Thread

from playwright.sync_api import expect, sync_playwright


BRIDGE = """
(() => {
  const id = '0ddfed53-76d4-47f4-ab31-8cd392aa4a03';
  const addedId = 'a1cb6fd0-42cb-475a-a198-177a28cc4c15';
  const profiles = [{id, name:'Personal world', version:'1.21.1',
    memoryMiB:2048, windowWidth:1280, windowHeight:720},
    {id:'d8221151-316b-41d2-b3ce-3db6072ac886', name:'Legacy creative', version:'1.20.4',
      memoryMiB:2048, windowWidth:1280, windowHeight:720}];
  const sample = {id:'sample', name:'Forest', generation:'A', phase:'idle',
    healthy:true, setting:0, saveRevision:0, review:null};
  const library = {instances:[sample], metrics:{sourceBytes:81920,
    reusedBytes:0, objectCount:4, objectBytes:81920,
    allocatedObjectBytes:81920, activeLogicalBytes:81920}, message:'Fixture action saved.'};
  const handlers = [];
  const listeners = {};
  const state = {
    failedLoad: '__FAIL_SCOPE__',
    failedAction: '',
    requests: [],
    installReject: null,
    clientPicks: 0,
    emit(event, payload) { handlers[listeners[event]]({event, payload}); }
  };
  const report = {
    profileId:id, version:'1.21.1', releaseType:'release', javaMajor:21,
    clientBytes:27117745, assetIndexBytes:434760, libraryCount:45, metadataSha1:'test-digest',
    origin:'network', runtime:{state:'missing', version:null, source:null},
    managedJava:{status:'available', major:21, release:'21.0.9+10', downloadBytes:5000},
    clientFile:{status:'missing'},
    blockedReasons:['Minecraft game installation and protected game launch are pending.', 'Microsoft sign-in is pending approval.']
  };
  window.reviewTest = state;
  window.__TAURI_INTERNALS__ = {
    transformCallback(callback) { return handlers.push(callback) - 1; },
    async invoke(command, args) {
      state.requests.push([command, args]);
      if (command === 'plugin:event|listen') {
        listeners[args.event] = args.handler;
        return args.handler;
      }
      if (command.endsWith('_library')) {
        if (command === state.failedLoad) {
          state.failedLoad = '';
          throw 'The review test could not read this library.';
        }
        return structuredClone(command === 'fixture_library' ? library : profiles);
      }
      if (command === 'inspect_vanilla_profile') {
        if (args.offline) throw 'No verified Minecraft metadata in the cache.';
        if (state.failedAction === command) throw 'Could not verify the cached version metadata.';
        return structuredClone(report);
      }
      if (command === 'select_profile_java') return null;
      if (command === 'install_profile_java') {
        if (!state.installReject) {
          return new Promise((_, reject) => {
            state.installReject = reject;
            state.emit('java-progress', { profileId:args.profileId, receivedBytes:2048, totalBytes:4096 });
          });
        }
        report.runtime = {state:'compatible', version:'21.0.9+10', source:'Managed Java'};
        report.managedJava = {status:'ready', major:21, release:'21.0.9+10'};
        return structuredClone(report);
      }
      if (command === 'cancel_java_install') {
        state.installReject('Java preparation was cancelled. You can retry from the profile.');
      }
      if (command === 'import_vanilla_client') {
        if (state.clientPicks++ === 0) return null;
        report.clientFile = {status:'verified'};
        if (state.failedAction === command) throw 'The client JAR was saved but setup could not be refreshed.';
        return structuredClone(report);
      }
      if (command === 'fixture_action') {
        sample.phase = 'review';
        sample.review = {added:['B'], removed:['A'], retained:['unchanged']};
        if (state.failedAction === command) throw 'The response was lost after the fixture update.';
        return structuredClone(library);
      }
      if (command === 'personal_action') {
        const {kind, ...settings} = args.action;
        if (kind === 'create') profiles.push({id:addedId, ...settings});
        if (state.failedAction === command) throw 'The response was lost after the personal update.';
        return structuredClone(profiles);
      }
      if (command === 'review_pack') return null;
      throw new Error('Unexpected command ' + command);
    }
  };
})()
"""


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, *_):
        return


def open_case(browser, address, failure=""):
    page = browser.new_page(viewport={"width": 1040, "height": 760})
    page.add_init_script(BRIDGE.replace("__FAIL_SCOPE__", failure))
    page.goto(address)
    return page


def test_independent_initial_loads(browser, address):
    page = open_case(browser, address, "personal_library")
    expect(page.get_by_role("alert")).to_contain_text("Personal profiles need attention")
    expect(page.get_by_role("button", name="New instance")).to_be_disabled()
    page.get_by_role("button", name="Review tools").click()
    expect(page.get_by_role("button", name="Review update", exact=True)).to_be_enabled()
    page.get_by_role("button", name="Review update", exact=True).click()
    expect(page.get_by_role("button", name="Stage update")).to_be_enabled()
    page.get_by_role("button", name="Library", exact=True).click()
    page.get_by_role("button", name="Reload personal profiles").click()
    expect(page.get_by_role("button", name="New instance")).to_be_enabled()
    page.get_by_role("button", name="Review tools").click()
    expect(page.get_by_role("button", name="Stage update")).to_be_enabled()
    page.close()

    page = open_case(browser, address, "fixture_library")
    expect(page.get_by_role("heading", name="Personal world")).to_be_visible()
    assert not page.evaluate("reviewTest.requests.some(([name]) => name === 'fixture_library')")
    page.get_by_role("button", name="Review tools").click()
    expect(page.get_by_role("alert")).to_contain_text("Fixture library needs attention")
    expect(page.get_by_role("button", name="Install Forest sample")).to_have_count(0)
    expect(page.get_by_role("button", name="New instance")).to_be_enabled()
    expect(page.get_by_role("button", name="Preview pack")).to_be_enabled()
    page.get_by_role("button", name="Reload library").click()
    expect(page.get_by_role("button", name="Forest Generation A")).to_be_visible()
    expect(page.get_by_role("button", name="New instance")).to_be_enabled()
    page.close()


def test_namespace_reload_after_mutation(browser, address):
    page = open_case(browser, address)
    page.get_by_role("button", name="Review tools").click()
    expect(page.get_by_role("button", name="Review update", exact=True)).to_be_enabled()
    page.evaluate("reviewTest.failedAction = 'fixture_action'")
    page.get_by_role("button", name="Review update", exact=True).click()
    expect(page.get_by_role("alert")).to_contain_text("Reload the fixture library")
    expect(page.get_by_role("button", name="Review update", exact=True)).to_be_disabled()
    expect(page.get_by_role("button", name="New instance")).to_be_enabled()
    page.get_by_role("button", name="Reload library").click()
    expect(page.get_by_role("button", name="Stage update")).to_be_enabled()

    page.evaluate("reviewTest.failedAction = 'personal_action'")
    page.get_by_role("button", name="Library", exact=True).click()
    page.get_by_role("button", name="New instance").click()
    page.locator("#profile-name").fill("Saved before reply")
    page.get_by_role("button", name="Create profile", exact=True).click()
    expect(page.get_by_role("alert")).to_contain_text("Reload personal profiles")
    expect(page.get_by_role("button", name="New instance")).to_be_disabled()
    page.get_by_role("button", name="Review tools").click()
    expect(page.get_by_role("button", name="Stage update")).to_be_enabled()
    page.get_by_role("button", name="Library", exact=True).click()
    page.get_by_role("button", name="Reload personal profiles").click()
    expect(page.get_by_role("button", name="Saved before reply")).to_be_visible()
    expect(page.get_by_role("button", name="New instance")).to_be_enabled()
    page.close()


def test_profile_name_length(browser, address):
    page = open_case(browser, address)
    page.get_by_role("button", name="New instance").click()
    dialog = page.get_by_role("dialog")
    name = dialog.locator("#profile-name")
    memory = dialog.locator("#profile-memory")
    save = dialog.get_by_role("button", name="Create profile", exact=True)
    name.fill("🌲" * 81)
    expect(name).to_have_attribute("aria-invalid", "true")
    expect(save).to_be_disabled()
    name.fill("🌲" * 80)
    expect(name).to_have_attribute("aria-invalid", "false")
    expect(memory).to_have_value("2048")
    expect(save).to_be_enabled()
    save.click()
    expect(page.get_by_role("heading", name="🌲" * 80)).to_be_focused()
    page.get_by_role("tab", name="Settings").click()
    page.get_by_role("button", name="Clone profile").click()
    expect(dialog.locator("#profile-name")).to_have_value("🌲" * 75 + " copy")
    page.keyboard.press("Escape")
    expect(page.get_by_role("button", name="Clone profile")).to_be_focused()
    page.close()


def test_setup_and_native_menu(browser, address, captures):
    page = open_case(browser, address)
    expect(page.get_by_role("button", name="Check setup")).to_be_enabled()
    assert page.evaluate("reviewTest.requests.some(([name, args]) => name === 'inspect_vanilla_profile' && args.offline)")
    overview = page.get_by_role("tab", name="Overview")
    overview.focus()
    page.keyboard.press("ArrowRight")
    expect(page.get_by_role("tab", name="Setup")).to_be_focused()
    page.get_by_role("button", name="Check setup").click()
    expect(page.get_by_text("Verified · release", exact=True)).to_be_visible()
    expect(page.get_by_text("Minecraft game installation and protected game launch are pending.")).to_be_visible()
    expect(page.get_by_role("button", name="Play", exact=True)).to_have_count(0)
    page.get_by_role("button", name="Choose Java folder").click()
    expect(page.get_by_role("status")).to_contain_text("Java selection cancelled")
    page.get_by_role("button", name="Prepare managed Java").click()
    expect(page.get_by_role("progressbar", name="Managed Java download")).to_have_attribute("value", "2048")
    expect(page.get_by_role("button", name="Recheck setup")).to_be_disabled()
    page.get_by_role("button", name="Legacy creative Vanilla").click()
    expect(page.get_by_role("heading", name="Legacy creative")).to_be_focused()
    expect(page.get_by_role("progressbar", name="Managed Java download")).to_have_count(0)
    page.get_by_role("button", name="Personal world Preparing managed Java").click()
    expect(page.get_by_role("progressbar", name="Managed Java download")).to_have_attribute("value", "2048")
    page.get_by_role("button", name="Cancel Java preparation").click()
    expect(page.get_by_role("status")).to_contain_text("Java preparation was cancelled")
    expect(page.get_by_role("button", name="Prepare managed Java")).to_be_enabled()
    page.get_by_role("button", name="Prepare managed Java").click()
    expect(page.get_by_text("Compatible Java found · 21.0.9+10")).to_be_visible()
    assert not page.get_by_role("button", name="Verify managed Java").evaluate("el => el.classList.contains('primary')")
    assert page.get_by_role("button", name="Import client JAR").evaluate("el => el.classList.contains('primary')")
    page.get_by_role("button", name="Import client JAR").click()
    expect(page.get_by_role("status")).to_contain_text("Client JAR selection cancelled")
    page.get_by_role("button", name="Import client JAR").click()
    expect(page.get_by_role("button", name="Verify client JAR")).to_be_enabled()
    expect(page.get_by_role("status")).to_contain_text("Client JAR verified")
    page.evaluate("reviewTest.failedAction = 'import_vanilla_client'")
    page.get_by_role("button", name="Verify client JAR").click()
    expect(page.get_by_role("status")).to_contain_text("setup could not be refreshed")
    expect(page.get_by_role("button", name="Verify client JAR")).to_have_count(0)
    page.get_by_role("button", name="Check setup", exact=True).click()
    expect(page.get_by_role("button", name="Verify client JAR")).to_be_enabled()
    page.evaluate("reviewTest.failedAction = 'inspect_vanilla_profile'")
    page.get_by_role("button", name="Recheck setup").click()
    expect(page.get_by_role("status")).to_contain_text("Could not verify the cached version metadata.")
    expect(page.get_by_text("Verified · release", exact=True)).to_have_count(0)
    page.evaluate("reviewTest.failedAction = ''")
    page.get_by_role("button", name="Check setup", exact=True).click()
    expect(page.get_by_text("Verified · release", exact=True)).to_be_visible()
    expect(page.get_by_text("Minecraft game installation and protected game launch are pending.")).to_be_visible()

    page.evaluate("reviewTest.emit('desktop-menu', 'show-activity')")
    expect(page.get_by_role("heading", name="Activity")).to_be_focused()
    page.evaluate("reviewTest.emit('desktop-menu', 'preview-pack')")
    page.wait_for_function("reviewTest.requests.some(([name]) => name === 'review_pack')")
    expect(page.get_by_role("button", name="Preview pack")).to_be_focused()
    page.evaluate("reviewTest.emit('desktop-menu', 'new-profile')")
    expect(page.get_by_role("dialog", name="Create vanilla profile")).to_be_visible()
    page.keyboard.press("Escape")

    if captures:
        captures.mkdir(parents=True, exist_ok=True)
    for width, height in [(1040, 760), (380, 540)]:
        page.set_viewport_size({"width": width, "height": height})
        if width == 380 and page.get_by_role("button", name="Personal world Vanilla").is_visible():
            page.get_by_role("button", name="Personal world Vanilla").click()
        for theme in ["light", "dark"]:
            page.get_by_role("combobox", name="Appearance").select_option(theme)
            expect(page.get_by_role("tabpanel", name="Setup")).to_be_visible()
            expect(page.get_by_role("heading", name="Activity")).to_be_visible()
            assert page.locator("html").evaluate("(el) => el.scrollWidth === el.clientWidth")
            if captures:
                page.get_by_role("tab", name="Setup").focus()
                page.mouse.move(1, 1)
                page.screenshot(path=captures / f"{width}-{theme}-setup.png")
        if width == 380:
            page.get_by_role("button", name="Back to Library").click()
            expect(page.get_by_role("button", name="Personal world Vanilla")).to_be_focused()
    page.evaluate("reviewTest.emit('desktop-menu', 'focus-search')")
    expect(page.get_by_role("searchbox", name="Find an instance")).to_be_focused()
    page.close()


def test_webview_shortcuts(browser, address):
    page = open_case(browser, address)
    page.set_viewport_size({"width": 380, "height": 540})
    row = page.get_by_role("button", name="Personal world Vanilla")
    search = page.get_by_role("searchbox", name="Find an instance")
    row.click()
    title = page.get_by_role("heading", name="Personal world")
    expect(title).to_be_focused()
    assert title.evaluate("el => el.offsetWidth < el.parentElement.offsetWidth")
    page.keyboard.press("Control+f")
    expect(search).to_be_focused()
    row.click()
    page.get_by_role("button", name="Back to Library").click()
    expect(row).to_be_focused()
    page.keyboard.press("Control+f")
    expect(search).to_be_focused()

    page.keyboard.press("Control+n")
    dialog = page.get_by_role("dialog", name="Create vanilla profile")
    expect(dialog.locator("#profile-name")).to_be_focused()
    page.keyboard.press("Control+f")
    expect(dialog.locator("#profile-name")).to_be_focused()
    page.keyboard.press("Escape")
    expect(page.get_by_role("button", name="New instance")).to_be_focused()
    page.keyboard.press("F6")
    expect(page.get_by_role("heading", name="Activity")).to_be_focused()

    page.keyboard.press("Control+o")
    expect(page.get_by_role("button", name="Preview pack")).to_be_focused()
    assert page.evaluate("reviewTest.requests.filter(([name]) => name === 'review_pack').length") == 1
    page.evaluate("""() => {
      window.dispatchEvent(new KeyboardEvent('keydown', {key:'o', ctrlKey:true, cancelable:true}));
      reviewTest.emit('desktop-menu', 'preview-pack');
    }""")
    assert page.evaluate("reviewTest.requests.filter(([name]) => name === 'review_pack').length") == 2
    page.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, default=Path(".artifacts/renderer-test"))
    parser.add_argument("--channel", help="Playwright Chromium channel, for example msedge")
    parser.add_argument("--captures", type=Path, help="Directory for the four renderer-only captures")
    args = parser.parse_args()
    if not (args.directory / "index.html").is_file():
        parser.error("build the renderer first with bun run build --outDir .artifacts/renderer-test")
    handler = partial(QuietHandler, directory=args.directory.resolve())
    with ThreadingHTTPServer(("127.0.0.1", 0), handler) as server:
        Thread(target=server.serve_forever, daemon=True).start()
        try:
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(channel=args.channel, headless=True)
                address = "http://127.0.0.1:" + str(server.server_port)
                test_independent_initial_loads(browser, address)
                test_namespace_reload_after_mutation(browser, address)
                test_profile_name_length(browser, address)
                test_setup_and_native_menu(browser, address, args.captures)
                test_webview_shortcuts(browser, address)
                print("Passed: setup action priority, Java state, menu and keyboard entry points, reflow, scoped recovery, and Unicode profile names.")
        finally:
            server.shutdown()


if __name__ == "__main__":
    main()
