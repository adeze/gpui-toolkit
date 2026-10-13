#!/usr/bin/env python3
"""Verify every full Spinorama view and colored 3D geometry in a real browser.

Uses captured KEF R3/asr API responses for reproducible offline rendering.
Pass --live-api to additionally exercise the site's same-origin proxy.
"""

import argparse
import gzip
import json
from pathlib import Path
from urllib.parse import unquote, urlsplit

from PIL import Image, ImageChops
from qa_wasm_screenshot import BrowserSession

VIEWS = ["cea-2034", "horizontal-spl", "vertical-spl", "contour-plot",
         "polar-directivity", "3d-surface", "sphere-plot", "polar-contour"]
FIXTURE = Path(__file__).resolve().parents[1] / "crates/gpui-spinorama/tests/fixtures/kef-r3-asr-browser.json.gz"


def colored_pixels(path):
    with Image.open(path).convert("RGB") as image:
        # Exclude the shell, selects, titles and colored control buttons.
        region = image.crop((30, 280, image.width - 80, image.height - 20))
        return sum(max(p) - min(p) > 25 for p in region.getdata())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:8090/spinorama/")
    parser.add_argument("--output", type=Path, default=Path("target/qa/spinorama-full"))
    parser.add_argument("--live-api", action="store_true")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    fixture = json.loads(gzip.decompress(FIXTURE.read_bytes()))
    failures = []
    with BrowserSession(viewport=(1280, 900)) as session:
        page = session._browser.new_page(viewport={"width": 1280, "height": 900})
        errors = []
        page.on("pageerror", lambda error: errors.append(str(error)))

        def respond(route):
            path = unquote(urlsplit(route.request.url).path)
            if path.endswith("/speakers"):
                value = ["KEF R3"]
            elif path.endswith("/versions"):
                value = ["asr"]
            else:
                value = fixture.get(path.rsplit("/", 1)[-1])
            route.fulfill(status=200 if value is not None else 404,
                          content_type="application/json", body=json.dumps(value))

        if not args.live_api:
            page.route("**/api/spinorama/v1/**", respond)
        for view in VIEWS:
            page.goto(f"{args.url}?section={view}&theme=dark", wait_until="domcontentloaded")
            page.wait_for_selector("html[data-spinorama-state='loaded']", timeout=45000)
            page.wait_for_timeout(600)
            path = args.output / f"{view}.png"
            page.screenshot(path=str(path))
            colored = colored_pixels(path)
            minimum = 10000 if view in ("3d-surface", "sphere-plot") else 250
            if colored < minimum:
                failures.append(f"{view}: only {colored} colored chart pixels (requires {minimum})")
            if view in ("3d-surface", "sphere-plot"):
                page.mouse.move(650, 550)
                page.mouse.down()
                page.mouse.move(800, 470, steps=10)
                page.mouse.up()
                page.wait_for_timeout(600)
                rotated = args.output / f"{view}-rotated.png"
                page.screenshot(path=str(rotated))
                if colored_pixels(rotated) < 10000:
                    failures.append(f"{view}: mesh disappeared after rotation")
                with Image.open(path) as before, Image.open(rotated) as after:
                    diff = ImageChops.difference(before.convert("RGB"), after.convert("RGB"))
                    changed = sum(max(p) > 15 for p in diff.crop((30, 280, 1200, 880)).getdata())
                if changed < 1000:
                    failures.append(f"{view}: rotation changed only {changed} pixels")
                if view == "sphere-plot":
                    # Drag the log-frequency slider well away from its bass endpoint.
                    page.mouse.move(104, 260)
                    page.mouse.down()
                    page.mouse.move(360, 260, steps=10)
                    page.mouse.up()
                    page.wait_for_timeout(600)
                    frequency = args.output / "sphere-frequency.png"
                    page.screenshot(path=str(frequency))
                    with Image.open(rotated) as before, Image.open(frequency) as after:
                        diff = ImageChops.difference(before.convert("RGB"), after.convert("RGB"))
                        changed = sum(max(p) > 15 for p in diff.crop((30, 303, 1200, 880)).getdata())
                    if changed < 1000:
                        failures.append(f"sphere frequency changed only {changed} geometry pixels")
            print(f"{view}: {colored} colored chart pixels", flush=True)
        page.set_viewport_size({"width": 390, "height": 844})
        page.goto(f"{args.url}?section=polar-directivity&theme=light&style=material3", wait_until="domcontentloaded")
        page.wait_for_selector("html[data-spinorama-state='loaded']", timeout=45000)
        page.wait_for_timeout(600)
        mobile = args.output / "mobile-polar.png"
        page.screenshot(path=str(mobile))
        if colored_pixels(mobile) < 250:
            failures.append("Mobile polar view has no visible colored chart")
        failures.extend(errors)
        page.close()
    if failures:
        for failure in failures:
            print(f"FAIL: {failure}")
        return 1
    print("Full Spinorama browser checks passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
