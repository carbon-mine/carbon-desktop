# Carbon Desktop layout review

Open [the interactive layout options](layout-options.html) locally in a desktop
browser. The three options use the same mock library and install states. They
work offline. The owner selected **A · Split library** in the kickoff review.
The [Desktop design system](../../design-system/README.md) records the seed
revision, licenses and local usage.

## Decision and tradeoffs

| Direction | Library and primary action | Interruptions | Tradeoff |
| --- | --- | --- | --- |
| **A · Split library (accepted)** | Searchable list beside detail on wide windows. One pane at a time on narrow windows. Select an instance, then act in the named detail pane. | Activity remains visible from either pane. Open an activity entry to return to its instance. | Fast instance switching. The list and detail both need enough room for zoom and long labels. |
| B · Library grid | Catalogue tiles present version and status. Open a detail route to act. One column when space runs out. | Each tile and detail carries its own status and recovery path. | Broad visual scan with pack imagery. Another instance's failure is easier to miss while in detail. |
| C · Play home | Home features the last played instance with immediate Play. A dedicated Library is the next destination for switching packs. | Activity is a top-level destination with a count in its label. | Fast return to a familiar pack. Adds a route for multi-instance work and needs a clear first-run fallback. |

Use platform window decorations. The Carbon tokens, square geometry, marks and
IBM Plex are inherited. The split library, selected row, task strip, state-based
primary action, review flow and narrow navigation are Desktop choices.
The grid and Home remain review illustrations.

The owner asked for the launcher to feel like a desktop app and to continue
through the original standalone setup. The split library now has a compact
command bar, a tabbed instance workspace and a persistent Activity strip.
The native window owns File and View menus. The Library shows personal
instances and opens on personal profile creation. Review tools opens the
synthetic fixture library on demand. In narrow windows, Back restores the
selected row or search. The native shortcut for Find instance can return
directly to the list from the detail pane.

## Workflow and interaction notes

| Player task | Sequence and expected feedback | Focus and screen-reader intent |
| --- | --- | --- |
| First launch | New instance → name, version, memory and window preferences → Create profile → Overview. Review tools offers Install Forest sample for testing. | On successful create, focus the new instance heading and announce the save through Activity. |
| Reopen or switch | Search or choose a row → instance heading and Overview. Overview can show verified metadata from a local cache. Setup holds requirements and preparation. Settings holds edit and clone. | Use Tab through controls, then Enter or Space. Left and Right move between tabs and Home or End jumps to the first or last tab. The selected row exposes its current state. |
| Prepare an instance | Check setup → inspect official version metadata → choose a compatible Java folder or prepare managed Java → import or verify the version's client JAR. Setup shows exact launch prerequisites. | Focus moves into Setup when called from Overview. Activity exposes progress and returns to the correct profile. A cancelled picker preserves the report. Java transfer progress is measurable and cancellation remains request-based. Game libraries, assets, entitlement and protected launch remain pending. |
| Inspect a file | Preview pack → native file picker → read and validate local manifest → named review dialog with metadata and counts. Cancel keeps the current view. | Focus Close preview in the dialog. Escape and Close return to the invoking command. An invalid archive receives visible actionable feedback. |
| Review a release | Review tools → Updates → Review update → inspect added, removed and reused content → Stage or Cancel. Show the active generation separately from the staged generation. | The stage change is announced once. If the invoking button disappears, move to the next valid primary action. Preserve focus on surviving actions. |
| Interrupt and recover | Start or stage a task → switch to another instance → find its Activity entry → Resume or Cancel. When a staged generation fails verification, name that failure and provide Repair while the current healthy generation is still active. | Focus returns to the named instance from Activity. Expose phase in text as well as any tag color. A sampled byte count belongs in progress output, while the polite region announces phase changes and recovery. |
| Uncertain operation result | Present the error and Reload library. Disable mutations until a fresh snapshot confirms state. An authoritative pre-write rejection can still offer its safe corrective action. | Focus the error heading and keep Reload keyboard reachable. Re-read the active phase before offering the next action. |
| Resize and preference | Inspect 1,200 px, 560 px and 320 CSS px in the sketch. In the packaged app, check the configured minimum window and 200% scale. Theme options are Light, Dark and System. | Review zoom and long names for content reflow. Preserve standard native shortcuts and check that header, scroll region and Activity leave focused elements visible. |

## Launcher observations

Reviewed 26 November 2026 by the Desktop UX workstream. Versions below identify
the linked evidence. Client behavior comes from first-party descriptions and
released-source references. Native walkthrough is a separate check.

| Project and evidence | Player task and documented path | Design implication and evidence gap |
| --- | --- | --- |
| Official Minecraft Launcher · Minecraft Help, read 26 November · Windows and supported Linux distributions | Java Edition → Installations → New installation → name/version → Create → Play → select the saved installation. The help site places JVM arguments in the installation's advanced options. | Keep version and current instance visible beside Play. Preserve advanced settings within the selected instance. Exact native layout and concurrent task behavior are unverified. [Installation](https://help.minecraft.net/hc/en-us/articles/23907917790093) · [Version workflow](https://help.minecraft.net/hc/en-us/articles/360034754852-Changing-game-versions-) · [JVM settings](https://help.minecraft.net/hc/en-us/articles/39083573916941) |
| Prism Launcher · 11.1.1 release reference and current wiki · Windows and Linux | Add Instance → choose version and loader or pick an import source. Instance Copy supports a name, icon, group and selective data. Settings → Java → Auto-detect or Test. Instance editing exposes Mods. | Surface Create/Import and named instance tools. Selective clone, grouping and Java verification remain proposals. Current wiki may include behavior newer than the release anchor. [Release](https://github.com/PrismLauncher/PrismLauncher/releases/tag/11.1.1) · [Create](https://prismlauncher.org/wiki/getting-started/create-instance/) · [Copy](https://prismlauncher.org/wiki/help-pages/instance-copy/) · [Java](https://prismlauncher.org/wiki/help-pages/java-settings/) · [Mods](https://prismlauncher.org/wiki/help-pages/mods/) |
| Modrinth App · 0.23.5 release reference and July 2026 sharing guide · Windows and Linux | An incoming shared-instance invite leads to preview and acceptance before installation. The guide documents publisher changes, recipient additions and compatibility effects. The 0.22.0 notes document reuse of already-owned files. | Review contents before activation, carry readable differences and retain a path back. Modrinth sharing documents its own service. Carbon's independent signed contract remains a separate integration. Native cancellation and interrupted-update recovery are unverified here. [0.23.5 changelog](https://modrinth.com/news/changelog/app/0.23.5) · [Sharing workflow](https://modrinth.com/news/article/shared-instances/) · [0.22.0 changelog](https://modrinth.com/news/changelog/app/0.22.0) |
| XMCL · project feature catalogue dated 22 November 2026 · Windows and Linux | The catalogue describes library folders, instance settings, installed mod groups, keyboard Quick Action and reports for launch failures or exit codes. | Use searchable instances and keep phase-specific diagnostics reachable. Folder groups, mod groups and command palette require an owner decision. Exact menu order and focus return are unverified. [Feature catalogue](https://xmcl.app/en/features/) |
| Pandora · v6.0.0 release reference · Windows and Linux | The project describes content organization, cross-instance storage reuse, Java management and redacted logs. | Give errors with named phase and consequences. An approved storage metric needs observable disk and network evidence. Current documentation alone leaves task interruption, cancel and recovery UX unknown. [Project features](https://github.com/Moulberry/PandoraLauncher#features) · [Release](https://github.com/Moulberry/PandoraLauncher/releases/tag/v6.0.0) |

Windows Start registered Minecraft Launcher, Prism, Modrinth and Pandora.
Comparison evidence in this packet comes from docs and release references.
The available WSL session reported a Wayland socket and no GNOME or KDE shell.
Packaged native GNOME and KDE captures, real launcher walkthroughs, and
assistive-technology speech remain open.

## Desktop guidance and checks

- **Windows.** Reflow the list/detail when content no longer fits side by
  side. Test a restored and resized window, focus order, app accelerators and
  native picker return. [Responsive design](https://learn.microsoft.com/windows/apps/design/layout/responsive-design) · [Keyboard interactions](https://learn.microsoft.com/en-us/windows/apps/develop/input/keyboard-interactions)
- **GNOME.** Keep one resizable primary window, use inline progress with a
  text phase and leave persistent recovery visible. Test Tab, Shift+Tab,
  Enter, Space and Escape on GNOME and record the decoration and portal in
  use. [Windows](https://developer.gnome.org/hig/patterns/containers/windows.html) · [Adaptiveness](https://developer.gnome.org/hig/guidelines/adaptive.html) · [Keyboard](https://developer.gnome.org/hig/guidelines/keyboard.html) · [Progress](https://developer.gnome.org/hig/patterns/feedback/progress-bars.html)
- **KDE.** Scope actionable errors to the affected instance and keep the
  background flow available. Use the modal for deliberate inspection or
  consent. Test Breeze, window resize, keyboard and the file portal.
  [Input and dialogs](https://develop.kde.org/hig/getting_input/) · [Status messages](https://develop.kde.org/hig/components/assistance/message/)

### WCAG 2.2 targets and measurements

| Criterion | Test and observed result in the sketch |
| --- | --- |
| 1.4.3 Contrast (Minimum) | Token pairs computed with WCAG relative luminance. Body text 18.10:1 light and 16.45:1 dark. Muted text on card 7.10:1 and 6.36:1. White primary button label 5.00:1 in both. Ready, running, interrupted and default tags 5.94:1 through 8.75:1. |
| 1.4.11 Non-text Contrast | Input border on card 4.31:1 light and 3.68:1 dark. Focus ring on card 4.55:1 and 15.13:1. Dark primary fill on dark popover is 2.31:1 and the default input border on that surface is 2.81:1. Both the sketch and the review app use a foreground-colored dialog control boundary. |
| 1.4.10 Reflow | Edge 158.0.4370.33 checked each direction across configured wide, 560 px and 320 px preview widths in light and dark. No horizontal overflow in the populated preset. Native zoom and long translation qualification remain pending. |
| 2.1.1 Keyboard, 2.4.3 Focus Order, 2.4.7 Focus Visible, 2.4.11 Focus Not Obscured (Minimum) | Edge exercised modal opening, initial focus on Keep current, Escape and opener restore, narrow Back and selection restore. Production app exercise requires full keyboard traversal, forced colors and focus clearance at every scroll edge. |
| 2.5.8 Target Size (Minimum) | The sketch uses a 40 px minimum height for buttons and fields. Run a packaged target-size and spacing check for compact activity buttons and native controls. |
| 4.1.2 Name, Role, Value and 4.1.3 Status Messages | Edge found the named dialog and accessible controls, and the live region updated after filtering. Actual Narrator or NVDA and Orca speech is pending on native desktop builds. A progress tick should be available on demand and avoid unsolicited repeated speech. |

[WCAG 2.2 Recommendation](https://www.w3.org/TR/WCAG22/)

### Carbon source audit

| Foundation | Evidence at saved Panel revision | Desktop disposition |
| --- | --- | --- |
| Tokens and typography | Exact light/dark semantic tokens, spacing, geometry and motion. Panel's Bun lock pins Fontsource 5.3.0. Original archive integrities and 25 file hashes verified. | Adopt local CSS and selected font files. Edge loaded the Sans variable and Mono faces from local files and made zero remote requests opening the sketch. |
| Button | Native button or anchor, native disabled for buttons, aria-disabled link handling, loading spinner, Tailwind utility styling. | Carry exact MPL source. The actual Desktop layout uses native HTML/Svelte controls with local style and explicit state changes. Exercise busy focus and duplicate start on the renderer. |
| Search | Clear control and accessible input name. Hard-coded dark background and placeholder; the dark placeholder pair is 3.01:1. | Use labeled native search with visible count and no-results text in the comparison. Expose no results and retain input focus. |
| Modal | Named custom dialog, Escape, Tab wrap and prior focus handling. Header and footer use fixed dark literals. | Use a named native dialog in the webview and qualify focus return on both host engines. |
| Tile and tag | Interactive Tile's synthetic Space handling does not prevent default scrolling. Tag's removable filter uses a generic accessible name and clears its focus outline. | Use a native instance button and static status tags. Any future remove button needs a specific name and visible focus. |
| Shell | Header, routes, user controls and app-specific navigation belong to Panel. | Use app-owned navigation and native desktop decorations. |

[Pinned source tree](https://github.com/athNdev/carbon-panel/tree/38cfd2263d20ac97bc9051be461201598ca932ad/web/carbon-panel/src/lib) · [Shared guide](https://github.com/athNdev/carbon-panel/blob/38cfd2263d20ac97bc9051be461201598ca932ad/docs/design-system/README.md)

## Reproducible review

The built desktop renderer passed a Windows Edge 158.0.4370.33 walkthrough at
1040×760 and 380×540 CSS pixels using deterministic Tauri bridge responses.
The run checked keyboard tab navigation, desktop menu events, Java progress
and cancel followed by retry, client JAR picker cancellation and verification,
report invalidation after failed recheck, scoped recovery, focus restoration
and both themes. Four captures show the
wide and narrow Setup panels in light and dark. No horizontal overflow was
observed at either tested width. See the implementation handoff for native
WebView, dialogs and packaged application evidence.

- In the sketch, choose A and 72 instances. Search for an absent name, then
  clear the query. Open Stone & Steam, open Review update, press Escape and
  check focus. Set Narrow, go Back, and confirm the selected row regains focus.
- Choose C and Empty. Import the sample archive, create it, Install, open
  progress, simulate interruption, resume and simulate completion. Play
  changes the sample state to Running. No account or game is accessed.
- Run `python design-system/verify_seed.py`, `bun run check` and
  `bun run build --outDir .artifacts/renderer-test`. With Playwright and
  Edge installed, run
  `python scripts/test-profile-renderer.py --channel msedge --captures .review/launcher-renderer`.
  For the packaged check, use native File and View commands, inspect Activity,
  and test setup, picker cancel and failure recovery with the approved
  inputs. Capture both widths and record OS, theme, scaling and input method.
  See the [implementation handoff](../handoff.md) for packaged results.
- With Narrator or NVDA on Windows and Orca on desktop Linux, walk Library
  selection, activity, content review, a failed action and modal return. Expect
  the selected heading, named dialog, meaningful changes announced once, and
  no byte-by-byte announcements. Record platform and engine versions with
  each outcome.

Research and better-ui guided evidence gathering and control checks.
The kickoff permits local annotated sketches. The review uses interactive
HTML against the inherited tokens. The sketches were drawn directly for
Carbon and used no competitor UI assets.
