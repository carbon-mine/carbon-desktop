# SPDX-License-Identifier: MPL-2.0
"""Capture the review at native CSS sizes using Edge and disposable profiles."""
from pathlib import Path
import sys
from playwright.sync_api import sync_playwright

root = Path(__file__).resolve().parent
out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)
with sync_playwright() as pw:
    browser = pw.chromium.launch(channel="msedge")
    page = browser.new_page(viewport={"width": 1100, "height": 760}, reduced_motion="reduce")
    errors = []
    page.on("pageerror", lambda e: errors.append(str(e)))
    page.goto((root/"index.html").as_uri())
    page.wait_for_selector(".card")
    assert page.locator(".card").count() == 4
    page.wait_for_timeout(250)
    for variant in ["scenic", "split", "compact", "scenic"]:
        page.locator('[data-v="'+variant+'"]').click()
        page.screenshot(path=out/("library-"+variant+".png"))
    tile = page.locator(".card .expand").first
    tile.focus()
    tile.press("Enter")
    assert page.locator("#worldTitle").inner_text().startswith("Alder Reach")
    page.screenshot(path=out/"expanded.png")
    canvas = page.locator("#largeMap")
    canvas.focus()
    before = page.locator("#mapCoords").inner_text()
    canvas.press("ArrowRight")
    page.wait_for_function("(before)=>document.querySelector('#mapCoords').textContent!==before",arg=before)
    with page.expect_popup() as popup:
        page.locator("#detach").click()
    detached = popup.value
    detached.wait_for_load_state()
    assert detached.locator("#worldSelect").input_value()=="alder-reach"
    page.locator("#dimSelect").select_option("nether")
    assert page.locator("#emptyMap").is_visible()
    assert detached.locator("#dimSelect").input_value()=="overworld"
    detached.screenshot(path=out/"separate.png")
    page.bring_to_front()
    page.locator("#dismiss").click()
    page.wait_for_function("document.activeElement===document.querySelector('.card .expand')")
    tile.press("Enter")
    assert page.locator("#dimSelect").input_value()=="overworld"
    page.keyboard.press("Escape")
    page.wait_for_function("document.querySelector('#expanded').hidden")
    page.wait_for_function("document.activeElement===document.querySelector('.card .expand')")
    page.locator(".card .expand").nth(1).click()
    assert "partial" in page.locator("#coverage").inner_text().lower()
    page.screenshot(path=out/"partial.png")
    page.locator("#dismiss").click()
    page.set_viewport_size({"width":390,"height":760})
    page.screenshot(path=out/"library-narrow.png")
    assert page.locator(".tiny-icon").first.is_visible()
    assert page.evaluate("document.documentElement.scrollWidth <= innerWidth")
    assert not errors, errors
    browser.close()
print("Passed library variants, partial/empty coverage, camera, independent window, focus return and compact width.")
