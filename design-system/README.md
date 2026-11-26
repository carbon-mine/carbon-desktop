# Carbon Desktop design system

Desktop owns this local copy of the Carbon foundations. The seed comes from
the saved [Carbon Panel revision 38cfd2263d20ac97bc9051be461201598ca932ad](https://github.com/athNdev/carbon-panel/tree/38cfd2263d20ac97bc9051be461201598ca932ad).
The [source lock](source-lock.json) gives the full source path and SHA-256 of
each inherited file. Panel's working-tree changes were excluded from the
seed.

## Use the local foundations

Import [tokens.css](inherited/tokens.css) and [workspace.css](workspace.css)
from the launcher entry point. Resolve the brand SVGs as local assets.
The standalone Carbon sketches use [fonts.css](fonts.css) and the inherited,
local IBM Plex fonts. Both surfaces run without a Panel checkout or runtime
fetch for design assets.

- Use `--background`, `--card` and `--popover` for surfaces and their paired
  foreground roles for text. Use `--input` for meaningful control boundaries
  and a visible outline against the actual adjacent surface. Use tag foreground
  and background pairs and an explicit label for states.
- The root defaults to light. Apply `data-carbon-theme="dark"` to the root for
  dark or `data-carbon-theme="light"` for light. For System, resolve the OS
  preference and follow changes while System is selected. Check buttons, text,
  dialogs and focus in both palettes and with forced colors.
- Use `--desktop-ui-font` for launcher text and `--desktop-code-font` for
  technical values. The launcher uses Segoe UI Variable or Segoe UI on
  Windows and a system UI font on Linux. The IBM Plex seed remains available
  for the standalone Carbon examples. Use regular labels and semibold
  headings. Let long labels wrap when space is limited.
- The inherited control sizes are 32, 40, 48 and 64 CSS pixels. Start at 40 with
  square corners and spacing from the tokens. Use a visible border for
  structure and an overlay shadow for a modal. Keep motion proportional and
  respect reduced motion. A focus ring needs visible clearance around its
  control inside scroll regions.
- Use the light SVG against dark surfaces and dark SVG against light ones.
  Keep the original proportions and lower opening. Use empty alt when the
  adjacent wordmark text already identifies the app. The inherited SVG titles
  say Carbon Panel. The paired Carbon Desktop text in the sketch is a local
  usage example. [Panel branding guide](https://github.com/athNdev/carbon-panel/blob/38cfd2263d20ac97bc9051be461201598ca932ad/assets/branding/README.md)

## Inherited files and terms

| Local files | Source and terms |
| --- | --- |
| `inherited/tokens.css` | Exact canonical Panel token CSS. The file declares MPL-2.0. |
| `inherited/components/CarbonButton.svelte` | Exact Panel Svelte 5 source. It has no Panel app imports and declares MPL-2.0. It assumes Panel's Tailwind classes, semantic aliases, `focus-ring`, `motion-press` and reduced-motion setup. Treat it as reference source until a tested Desktop adapter supplies equivalent styling. |
| `inherited/brand/mark-*.svg` | Exact symbol-only master artwork. The assets have no file-level license header. Preserve the Panel marks, source credit and repository notices. Desktop's brand-use authority is the owner's kickoff for this shared identity. |
| `inherited/brand/README.panel.md` | Verbatim Panel artwork provenance and credit. Its repository-relative paths refer to Panel. The editable master and outlined Panel wordmarks stay in the source repository. |
| `inherited/fonts/ibm-plex-sans/` | Fontsource variable Plex Sans 5.3.0. The exact `wght.css` declares six upright WOFF2 subsets covering weights 100–700. Preserve the package's bundled SIL OFL 1.1 notice. |
| `inherited/fonts/ibm-plex-mono/` | Fontsource Plex Mono 5.3.0. Exact Latin upright styles and linked WOFF2/WOFF files for 400, 500 and 600. Preserve the package's bundled SIL OFL 1.1 notice. |
| `inherited/LICENSE.MPL-2.0` and `inherited/LICENSE.DiscoPanel-MIT` | Verbatim Panel repository notices. The DiscoPanel MIT notice applies to inherited material identified by Panel. |

The Panel Git revision pins the two package archives through
`web/carbon-panel/bun.lock`. The archive SHA-512 values matched that lock before
the CSS, font files, and notices were extracted. [Panel license summary](https://github.com/athNdev/carbon-panel/tree/38cfd2263d20ac97bc9051be461201598ca932ad#license)

## Desktop usage examples

Open the [layout review](../docs/design/layout-options.html) offline. It loads
both local font families and the original token file. Its buttons and dialogs
are portable browser sketches. Use real DOM controls in the renderer and attach
native work at the app boundary.

| Pattern | Contract |
| --- | --- |
| Instance action and status | Label the state and use a single primary action for the selected instance. Disable duplicate starts. When the action changes, keep focus on the next valid action or the instance heading. Announce meaningful completion, interruption and resolution once. |
| Review dialog | Name the affected instance and consequences. Place initial focus on the safe action or the heading for a long review. Escape and Cancel return to the invoking control. Use the next valid control if applying the action removed the opener. |
| Task activity | Name the instance and operation phase. Show measured progress when available. Keep recovery available from the affected instance and the chosen activity surface. Announce state changes without repeating byte updates. |

### Native task motion

The GPUI install card and Activity use the same task meter. During a measured
transfer, a three-pixel rail and tabular counters ease from their current
displayed values to the latest worker sample over 180 ms. A new phase, changed
total, retry or terminal value takes effect immediately. File preparation uses
completed files for the rail and also shows verified bytes. During an unmeasured
step three monochrome dots move for up to five seconds, then stay still.

Reduced motion shows measured values immediately and keeps the dots still.
GPUI reads the Windows client animation setting, the macOS accessibility motion
preference and the Linux desktop portal setting (with the GNOME animation
setting as a fallback). Screen readers receive phase changes and can inspect
the measured progress value independently of the visual interpolation.
| Library empty and no matches | Offer a first-create and import path for an empty library. Preserve the query when filtering returns no matches and give a way to broaden it. |

### Desktop launcher workspace

The live [launcher shell](../src/App.svelte),
[workspace foundations](workspace.css) and
[renderer styles](../src/review.css) keep the desktop layout outside the
inherited seed. The command bar fits common actions below the native menu
and caption. A scrollable Library stays beside the instance workspace on
wide windows. Use a distinct instance header, an active tab underline and
a clear primary action. Narrow windows show the Library or the selected
instance. Activity stays outside both scroll regions.

[DesktopIcon.svelte](DesktopIcon.svelte) uses the installed Segoe Fluent
Icons or Segoe MDL2 Assets on Windows when available. It carries original
portable fallback paths for other systems and pairs each icon with visible
text or an accessible control name. No Microsoft font file is bundled.

- Use the `instance-tabs` pattern when an instance has distinct tasks.
  Keep a visible active marker and a programmatic selected state. Arrow
  keys change tabs, Home and End seek the edges, and Tab moves into the
  chosen panel.
- Use `property-list` for settings and readiness values. Keep their terms and
  descriptions semantic. Let long values wrap while keeping the primary
  action and readiness text readable.
- Use `setup-stages` when readiness depends on several prerequisites.
  Put the player decision and expected source, size and outcome near the
  confirmation. Keep diagnostics and technical details available on demand.
  The program should render application dialogs, pickers and theme choices
  using native controls.
- Use `setup-progress` for task-specific progress and an optional cancel
  request. Show measured bytes only when the bridge supplies them. Keep
  phase messages and the result in Activity. For a saved action with an
  uncertain result, reload or recheck the affected state before offering
  another mutation.
- Use a dedicated view for synthetic fixtures. Personal instances open in
  the Library and show real prerequisites through Setup. A prepared runtime
  or verified client JAR does not replace the remaining account and
  installation gates.

The [launcher renderer regression](../scripts/test-profile-renderer.py)
exercises four light/dark and narrow/wide combinations, tabs, menu events,
progress, cancellation and conservative recovery. The
[layout review](../docs/design/README.md) records evidence and native
acceptance limits. The [Windows workspace review](../docs/design/windows-workspace.md)
records Microsoft guidance and before/after evidence.

The [M0 research packet](../docs/design/README.md) records contrast measurements,
keyboard paths, theme checks and the packaged-app verification still required.

## Refresh and verify

Run `python design-system/verify_seed.py` from the Desktop root to verify file
digests and coverage. For an intentional refresh, identify the new full Panel
commit, compare it with the prior revision, and review relevant tokens,
component behavior, brand files and file-level terms. Compare pinned package
integrities before extracting any font updates. Update source mappings and
digests alongside the changed inherited files. Keep Desktop additions outside
`inherited/` and document their intended use. Reopen the affected examples,
review packaged Windows and Linux rendering, and rerun focus, color, scaling
and status checks. Shared upstream changes go through the [Carbon shared
change workflow](https://github.com/athNdev/carbon-panel/blob/38cfd2263d20ac97bc9051be461201598ca932ad/docs/design-system/README.md#change-and-propagation-workflow).
