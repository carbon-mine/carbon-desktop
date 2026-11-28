<!-- SPDX-License-Identifier: MPL-2.0 -->

# README screenshots

The README images come from the real native GPUI launcher, captured on the hosted
Windows runner and published as a static, image-only GitHub Pages site.

## Pipeline

1. `.github/workflows/readme-screenshots.yml` runs on every `pull_request` that
   touches the workflow, the capture script, or this directory, and on every
   completed `Desktop CI` push to `main`.
2. The job builds the locked `carbon-desktop-gpui` package for
   `x86_64-pc-windows-msvc` and runs `scripts/capture-readme-screenshots.ps1`.
   The script seeds the deterministic `default` fixture library into a disposable
   root, forces dark appearance, waits for the real `CARBON_LIBRARY_READY`
   marker, drives the window through UI Automation, and hard-fails on blank,
   uniform, duplicate or unstable captures.
3. Already-published screens:

   | ID | File | View |
   | --- | --- | --- |
   | `library-wide` | `library-wide.png` | wide window, library and selected instance |
   | `library-compact` | `library-compact.png` | compact window, library list |
   | `instance-compact` | `instance-compact.png` | compact window, selected instance |

4. On a trusted `main` event only (never a fork pull request) the job verifies the
   captured commit is still the tip of `main`, writes a path-free public manifest,
   and deploys the images plus `manifest.json` to GitHub Pages. The deploy job
   re-checks the tip before publishing, so a late job cannot overwrite newer
   screenshots.

## Published layout

The site serves stable URLs directly under
`https://carbon-mine.github.io/carbon-desktop/`:

- `library-wide.png`
- `library-compact.png`
- `instance-compact.png`
- `manifest.json` — schema `carbon-readme-screenshots-public/v1`, naming the
  `commit`, `generatedAt`, `appearance` and each screenshot's `id`, `file`,
  `alt`, `width`, `height`, `bytes` and `sha256`.

The Pages artifact intentionally contains only those PNG files and the manifest.
There is no root `index.html`, so `https://carbon-mine.github.io/carbon-desktop/`
returns the Pages 404 page: the site is a stable asset host for the README, not a
browsable gallery, and nothing is indexable beyond the images themselves. Do not
add a landing page or directory listing.

## Verifying a publish

```sh
gh run list --repo carbon-mine/carbon-desktop --workflow "README screenshots" --limit 3
curl -sS https://carbon-mine.github.io/carbon-desktop/manifest.json
```

Then confirm the manifest `commit` is the current `origin/main` tip and that each
`sha256` matches the downloaded image:

```sh
curl -sS https://carbon-mine.github.io/carbon-desktop/library-wide.png | sha256sum
```

Rendered README images are served through GitHub's image proxy, which caches per
URL. If a screenshot ever lags behind the manifest, the tested fallback is to
revert that image reference to the bundled `docs/images/gpui-*.png` copy until
the next deploy; the bundled copies remain in the repository for that purpose.
No cache-purge step is part of the pipeline.

## Adding a screen

Add a `Capture-State` call in `scripts/capture-readme-screenshots.ps1` with a
stable `-Id`, honest `-Alt`, a readiness assertion for the state, and then list
the ID in the workflow's required-ID check and in the table above. Only add
screens that render reliably on the hosted runner.
