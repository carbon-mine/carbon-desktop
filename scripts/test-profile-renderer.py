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
    dialogName: null,
    importedPack: null,
    emit(event, payload) { handlers[listeners[event]]({event, payload}); }
  };
  const report = {
    profileId:id, version:'1.21.1', releaseType:'release', javaMajor:21,
    clientBytes:27117745, assetIndexBytes:434760, libraryCount:45, metadataSha1:'test-digest',
    origin:'network', runtime:{state:'missing', version:null, source:null},
    managedJava:{status:'available', major:21, release:'21.0.9+10', downloadBytes:5000},
    clientFile:{status:'missing'}, gameFilesPrepared:false, gameFilesCached:false, sessionSupported:false,
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
        if (args.offline && (!report.gameFilesPrepared || args.profileId !== id)) throw 'No verified Minecraft metadata in the cache.';
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
        return;
      }
      if (command === 'plan_game_files' || command === 'prepare_game_files') {
        const files = {profileId:id, version:report.version, platform:'windows-x64',
          metadataSha1:report.metadataSha1, uniqueFiles:7, totalBytes:8192};
        if (command === 'plan_game_files') return {...files, libraryFiles:2, assetObjects:3, assetNames:3, loggingFiles:0};
        if (args.metadataSha1 !== files.metadataSha1) throw 'Unexpected download revision.';
        report.gameFilesPrepared = true;
        report.gameFilesCached = true;
        report.clientFile = {status:'verified'};
        return {...files, fetchedBytes:args.offline ? 0 : 8192, reusedBytes:args.offline ? 8192 : 0};
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
      if (command === 'edit_profile_dialog') {
        if (state.dialogName === null) return null;
        const created = {...profiles[0], id:'a1cb6fd0-42cb-475a-a198-177a28cc4c15', name:state.dialogName};
        profiles.push(created);
        state.dialogName = null;
        if (state.failedAction === command) throw 'The response was lost after the personal update.';
        return {profiles:structuredClone(profiles), selectedId:created.id};
      }
      if (command === 'import_pack') {
        const imported = state.importedPack;
        if (!imported) return null;
        state.importedPack = null;
        profiles.push(imported);
        return {id: imported.id};
      }
      if (command === 'apply_desktop_theme') return null;
      if (command === 'minecraft_account_status') return 'unavailable';
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

    page.evaluate("reviewTest.failedAction = 'edit_profile_dialog'; reviewTest.dialogName = 'Saved before reply'")
    page.get_by_role("button", name="Library", exact=True).click()
    page.get_by_role("button", name="New instance").click()
    expect(page.get_by_role("alert")).to_contain_text("Reload personal profiles")
    expect(page.get_by_role("button", name="New instance")).to_be_disabled()
    page.get_by_role("button", name="Review tools").click()
    expect(page.get_by_role("button", name="Stage update")).to_be_enabled()
    page.get_by_role("button", name="Library", exact=True).click()
    page.get_by_role("button", name="Reload personal profiles").click()
    expect(page.get_by_role("button", name="Saved before reply")).to_be_visible()
    expect(page.get_by_role("button", name="New instance")).to_be_enabled()
    page.close()


def test_profile_dialog_return(browser, address):
    page = open_case(browser, address)
    # The native form owns validation. The WebView consumes its typed result.
    name = "🌲" * 80
    page.evaluate("(name) => {reviewTest.dialogName = name}", name)
    page.get_by_role("button", name="New instance").click()
    expect(page.get_by_role("heading", name=name, exact=True)).to_be_focused()
    page.get_by_role("tab", name="Settings").click()
    clone = page.get_by_role("button", name="Clone profile")
    clone.click()
    page.wait_for_function("reviewTest.requests.some(([cmd, args]) => cmd === 'edit_profile_dialog' && args.mode === 'clone' && args.profileId === 'a1cb6fd0-42cb-475a-a198-177a28cc4c15')")
    expect(clone).to_be_focused()
    page.close()


def test_pack_import(browser, address):
    page = open_case(browser, address)
    expect(page.get_by_role("heading", name="Personal world")).to_be_visible()
    imported = {
        "id": "3f1b6f6e-6f6e-4a1c-9c0e-2f7a1e2d3c4b",
        "name": "Forest pack",
        "version": "1.21.1",
        "memoryMiB": 2048,
        "windowWidth": 1280,
        "windowHeight": 720,
        "importedPack": {"name": "Forest", "loader": None, "pendingFiles": 1},
    }
    page.evaluate("(profile) => {reviewTest.importedPack = profile}", imported)
    page.keyboard.press("Control+o")
    expect(page.get_by_role("heading", name="Forest pack", exact=True)).to_be_focused()
    row = page.get_by_role("button", name="Forest pack Imported pack 1.21.1")
    expect(row).to_have_attribute("aria-current", "true")
    expect(page.get_by_role("heading", name="Pack preparation pending")).to_be_visible()
    page.close()


def test_install(browser, address):
    page = open_case(browser, address)
    review = page.get_by_role("button", name="Review install")
    expect(review).to_be_enabled()
    assert page.evaluate("reviewTest.requests.some(([cmd, args]) => cmd === 'inspect_vanilla_profile' && args.offline)")
    review.click()
    expect(page.get_by_role("heading", name="Install review")).to_be_focused()
    install = page.get_by_role("button", name="Install instance")
    install.click()
    progress = page.get_by_role("progressbar", name="Managed Java download")
    expect(progress).to_have_attribute("value", "2048")
    page.get_by_role("button", name="Legacy creative Vanilla").click()
    expect(progress).to_have_count(0)
    page.get_by_role("list", name="Local instances").get_by_role("button", name="Personal world Vanilla").click()
    expect(progress).to_have_attribute("value", "2048")
    page.get_by_role("button", name="Cancel Java preparation").click()
    expect(page.get_by_role("status")).to_contain_text("Java preparation was cancelled")
    assert not page.evaluate("reviewTest.requests.some(([cmd]) => cmd === 'prepare_game_files')")
    install.click()
    expect(page.get_by_role("heading", name="Game files ready")).to_be_visible()
    page.get_by_role("button", name="Verify game files").click()
    expect(page.get_by_role("status")).to_contain_text("7 game files verified")
    assert page.evaluate("reviewTest.requests.some(([cmd, args]) => cmd === 'prepare_game_files' && args.offline)")
    page.get_by_role("button", name="Play", exact=True).click()
    expect(page.get_by_role("heading", name="Play requirements")).to_be_focused()
    page.close()


def test_diagnostic_recovery(browser, address):
    page = open_case(browser, address)
    page.get_by_text("Advanced setup and diagnostics", exact=True).click()
    check = page.get_by_role("button", name="Check setup", exact=True)
    check.click()
    expect(page.get_by_text("Verified · release", exact=True)).to_be_visible()
    page.get_by_role("button", name="Choose Java folder").click()
    expect(page.get_by_role("status")).to_contain_text("Java selection cancelled")
    import_client = page.get_by_role("button", name="Import client JAR")
    import_client.click()
    expect(page.get_by_role("status")).to_contain_text("Client JAR selection cancelled")
    import_client.click()
    expect(page.get_by_role("status")).to_contain_text("Client JAR verified")
    page.evaluate("reviewTest.failedAction = 'import_vanilla_client'")
    verify = page.get_by_role("button", name="Verify client JAR")
    verify.click()
    expect(page.get_by_role("status")).to_contain_text("setup could not be refreshed")
    expect(verify).to_have_count(0)
    check.click()
    expect(verify).to_be_enabled()
    page.evaluate("reviewTest.failedAction = 'inspect_vanilla_profile'")
    page.get_by_role("button", name="Recheck setup").click()
    expect(page.get_by_role("status")).to_contain_text("Could not verify the cached version metadata")
    expect(page.get_by_text("Verified · release", exact=True)).to_have_count(0)
    page.evaluate("reviewTest.failedAction = ''")
    check.click()
    expect(page.get_by_text("Verified · release", exact=True)).to_be_visible()
    page.close()


def test_transfer_motion(browser, address):
    page = open_case(browser, address)
    page.clock.install(time="2030-01-01T00:00:00Z")
    page.get_by_role("button", name="Review install").click()
    page.get_by_role("button", name="Install instance").click()
    meter = page.get_by_role("progressbar", name="Managed Java download")
    counter = page.locator(".transfer .technical")
    expect(meter).to_have_attribute("value", "2048")
    expect(counter).to_have_text("2 KiB / 4 KiB")
    page.clock.pause_at("2030-01-01T00:10:00Z")

    def receive(count):
        page.evaluate("""count => reviewTest.emit('java-progress', {
            profileId:'0ddfed53-76d4-47f4-ab31-8cd392aa4a03', receivedBytes:count, totalBytes:4096
        })""", count)

    receive(3584)
    expect(meter).to_have_attribute("aria-valuetext", "3.5 KiB / 4 KiB")
    page.clock.run_for(48)
    intermediate = float(counter.inner_text().split()[0])
    assert 2 < intermediate < 3.5, intermediate
    page.clock.run_for(200)
    expect(counter).to_have_text("3.5 KiB / 4 KiB")

    receive(3840)
    page.emulate_media(reduced_motion="reduce")
    page.clock.run_for(16)
    expect(counter).to_have_text("3.8 KiB / 4 KiB")
    receive(1024)
    expect(counter).to_have_text("1 KiB / 4 KiB")
    page.clock.run_for(200)
    expect(counter).to_have_text("1 KiB / 4 KiB")

    receive(4096)
    busy = page.get_by_role("progressbar", name="Verifying and preparing Java…")
    expect(busy).to_have_count(1)
    assert busy.get_attribute("value") is None
    assert page.locator(".busy span").evaluate("el => getComputedStyle(el).animationName") == "none"
    page.emulate_media(reduced_motion="no-preference")
    motion_seconds = page.locator(".busy span").evaluate("""el => {
        const style = getComputedStyle(el);
        return parseFloat(style.animationDuration) * Number(style.animationIterationCount);
    }""")
    assert 0 < motion_seconds <= 5
    page.close()


def test_file_progress_resets_on_new_phase(browser, address):
    page = open_case(browser, address)
    page.clock.install(time="2030-01-01T00:00:00Z")
    page.evaluate("""() => {
        const bridge = window.__TAURI_INTERNALS__;
        const invoke = bridge.invoke.bind(bridge);
        reviewTest.installReject = () => {};
        bridge.invoke = (cmd, args) => {
            if (cmd !== 'prepare_game_files') return invoke(cmd, args);
            reviewTest.files = (phase, completedFiles) => reviewTest.emit('game-files-progress', {
                profileId:args.profileId, phase, completedFiles, totalFiles:100,
                verifiedBytes:completedFiles * 1024, totalBytes:102400
            });
            reviewTest.files('downloading', 10);
            return new Promise(() => {});
        };
    }""")
    page.get_by_role("button", name="Review install").click()
    page.get_by_role("button", name="Install instance").click()
    counter = page.locator(".transfer .technical")
    meter = page.get_by_role("progressbar", name="Minecraft game files")
    expect(counter).to_have_text("10 / 100 files · 10 KiB / 100 KiB")
    page.clock.pause_at("2030-01-01T00:10:00Z")
    page.evaluate("reviewTest.files('downloading', 70)")
    expect(meter).to_have_attribute("value", "70")
    page.clock.run_for(48)
    assert 10 < int(counter.inner_text().split()[0]) < 70
    page.evaluate("reviewTest.files('copyingView', 5)")
    expect(counter).to_have_text("5 / 100 files · 5 KiB / 100 KiB")
    page.clock.run_for(200)
    expect(counter).to_have_text("5 / 100 files · 5 KiB / 100 KiB")
    page.evaluate("reviewTest.files('copyingView', 100)")
    expect(counter).to_have_text("100 / 100 files · 100 KiB / 100 KiB")
    page.close()


def test_navigation(browser, address, captures):
    page = open_case(browser, address)
    overview = page.get_by_role("tab", name="Overview")
    overview.focus()
    page.keyboard.press("ArrowRight")
    settings = page.get_by_role("tab", name="Settings")
    expect(settings).to_be_focused()
    page.keyboard.press("Control+o")
    page.wait_for_function("reviewTest.requests.some(([cmd]) => cmd === 'import_pack')")
    expect(settings).to_be_focused()
    overview.click()
    if captures:
        captures.mkdir(parents=True, exist_ok=True)
    for width, height in [(1040, 760), (380, 540)]:
        page.set_viewport_size({"width": width, "height": height})
        row = page.get_by_role("button", name="Personal world Vanilla")
        if width == 380 and row.is_visible():
            row.click()
        for theme in ["light", "dark"]:
            page.evaluate("(theme) => reviewTest.emit('desktop-menu', 'theme-' + theme)", theme)
            expect(page.locator("html")).to_have_attribute("data-carbon-theme", theme)
            expect(page.get_by_role("tabpanel", name="Overview")).to_be_visible()
            assert page.locator("html").evaluate("(el) => el.scrollWidth === el.clientWidth")
            if captures:
                overview.focus()
                page.mouse.move(1, 1)
                page.screenshot(path=captures / f"{width}-{theme}-overview.png")
        if width == 380:
            page.get_by_role("button", name="Back to Library").click()
            expect(row).to_be_focused()
    page.keyboard.press("Control+f")
    expect(page.get_by_role("searchbox", name="Find an instance")).to_be_focused()
    page.keyboard.press("Control+n")
    page.wait_for_function("reviewTest.requests.some(([cmd, args]) => cmd === 'edit_profile_dialog' && args.mode === 'create' && args.profileId === null)")
    expect(page.get_by_role("button", name="New instance")).to_be_focused()
    page.keyboard.press("F6")
    expect(page.get_by_role("heading", name="Activity")).to_be_focused()
    page.evaluate("""() => {
      window.dispatchEvent(new KeyboardEvent('keydown', {key:'o', ctrlKey:true, cancelable:true}));
      reviewTest.emit('desktop-menu', 'import-pack');
    }""")
    assert page.evaluate("reviewTest.requests.filter(([cmd]) => cmd === 'import_pack').length") == 2
    page.close()


def test_appearance_keeps_search_selection(browser, address):
    page = open_case(browser, address)
    page.keyboard.press("Control+f")
    search = page.get_by_role("searchbox", name="Find an instance")
    expect(search).to_be_focused()
    page.keyboard.type("Personal")
    page.keyboard.press("Shift+ArrowLeft")
    for theme in ("dark", "light"):
        page.evaluate("(theme) => reviewTest.emit('desktop-menu', 'theme-' + theme)", theme)
        expect(page.locator("html")).to_have_attribute("data-carbon-theme", theme)
        page.wait_for_function("""theme => reviewTest.requests
            .filter(([cmd]) => cmd === 'apply_desktop_theme').at(-1)?.[1].preference === theme""",
            arg=theme)
        expect(search).to_be_focused()
        expect(search).to_have_value("Personal")
        assert search.evaluate("el => [el.selectionStart, el.selectionEnd]") == [7, 8]
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
                test_profile_dialog_return(browser, address)
                test_pack_import(browser, address)
                test_install(browser, address)
                test_transfer_motion(browser, address)
                test_file_progress_resets_on_new_phase(browser, address)
                test_diagnostic_recovery(browser, address)
                test_navigation(browser, address, args.captures)
                test_appearance_keeps_search_selection(browser, address)
                print("Passed renderer checks for install, recovery, native-dialog requests, keyboard focus and reflow.")
        finally:
            server.shutdown()


if __name__ == "__main__":
    main()
