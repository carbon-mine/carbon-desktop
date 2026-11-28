# Carbon Desktop GPUI edition

This branch is the native Rust-rendered alternative for the frontend comparison. It uses the same Carbon profile, runtime, vanilla game file, fixture store and protected Windows sample-session crates as the standalone candidate. Both editions keep their live data in separate roots.

## Initial window size

The first launcher window uses a preferred GPUI layout of 1368 × 808 logical pixels. On Windows the native menu occupies 20 pixels of that layout at 96 DPI, leaving a 1368 × 788 client. It shrinks to fit a smaller display. The main launcher keeps this size. You can move, minimize and restore it. A valid saved position survives restart; the launcher ignores saved user dimensions and maximized state from older builds. Native child windows keep their own sizing policies. The Minecraft game window has its own resolution setting.

The owner selected the client size of an observed, unmaximized official launcher as the reference. On Windows build 26300 at 96 DPI, launcher v3.44.1-2.6.2 (package 2.6.2.0 x64) measured 1368 × 788 client, 1370 × 820 visible DWM frame and 1384 × 827 outer Win32 frame. The owner’s 1371 × 821 screenshot corroborates the visible frame. The comparison used a running signed-in official launcher. Its factory first-run placement remains unverified. The previous fresh Carbon window measured 1040 × 740 client, 1042 × 792 visible and 1056 × 799 outer. These figures distinguish the native menu and decoration sizes from the launcher’s logical sizing input.

On a disposable Windows desktop at 96 DPI, the revised Carbon window measured 1368 × 788 client, 1370 × 840 visible and 1384 × 847 outer. The native menu accounts for the 20-pixel difference between the two launchers’ outer heights. The fixed-window follow-up retained a saved position when a legacy 620 × 490 maximized record was seeded. The main HWND omitted thick-frame and Maximize styles; a caption double-click, F11 and native size/maximize commands left its bounds unchanged. Move and minimize/restore worked.

On X11 the launcher finds its own toplevel by `_NET_WM_PID` and `WM_CLASS` and writes ICCCM `WM_NORMAL_HINTS` with equal minimum and maximum sizes. It does not use GPUI 0.2.2's raw window handle, which is unimplemented for X11. Wayland compositors are not covered and keep their own policy.

## Build and run

On Windows, use the MSVC Rust toolchain, Windows SDK and a JDK that supplies `javac` when compiling the protected sandbox fixture. From this checkout:

```powershell
cargo build --locked -p carbon-desktop-gpui -j 3
cargo test --locked -p carbon-desktop-gpui -j 3
.\target\debug\carbon-desktop-gpui.exe
```

The ordinary Windows dev build opens GPUI's HLSL source at runtime from the builder's Cargo registry. For a Windows review executable with embedded shaders, set `GPUI_FXC_PATH` to the Windows SDK's `x64\fxc.exe` if GPUI cannot discover it, then run:

```powershell
cargo --config 'profile.dev.package.gpui.debug-assertions=false' --config 'profile.dev.package.gpui-component.debug-assertions=false' build --locked -p carbon-desktop-gpui -j 3
```

Both overrides are required because the component kit's debug inspector uses GPUI's debug-only API. This builds the review binary in `target\debug` with precompiled shader bytecode. The current Windows handoff is `.artifacts/gpui-comparison/carbon-desktop-gpui-parity.exe` from `0867142`, SHA-256 `703b49817ae999ed96f8231c7b0799521eea0ec969f7ec3d08a897bd82c948c3`. PE inspection verified Windows GUI subsystem 2, all 16 shader blobs and the absence of a runtime HLSL compiler import. Pass `--comparison-review` to open a disposable 100-instance library. The ordinary launch opens the separate persistent GPUI root. The executable is unsigned. Clean-machine launch remains open.

On a Linux graphical session with the GPUI development libraries and graphics drivers installed:

```bash
cargo build --locked -p carbon-desktop-gpui -j 3
cargo test --locked -p carbon-desktop-gpui -j 3
./target/debug/carbon-desktop-gpui --comparison-review
```

The current Linux ELF at `.artifacts/gpui-comparison/parity-linux/carbon-desktop-gpui-parity-linux-0867142` is the locked offline debug build from the same source as the Windows handoff. Its SHA-256 is `4f9903c68e80437198a693686033cc79b61f0dc2bba2dfe8975be3ba00ecddc1`. All four GPUI tests passed against that Linux source, and the exact ELF passed a WSL2 AT-SPI action walkthrough.

On Ubuntu x86-64 with glibc 2.39, system XCB and XKB libraries, and a Vulkan-capable desktop, copy the current artifact onto the Linux filesystem, run `chmod +x carbon-desktop-gpui-parity-linux-0867142`, then `./carbon-desktop-gpui-parity-linux-0867142 --comparison-review`. The app also uses system fontconfig and TLS libraries. The WSL checks used an isolated Openbox/Xvfb session and Mesa lavapipe.

Run `--seed-review --temporary-review` for one personal vanilla profile and the Forest fixture. Run `--comparison-review` for a fresh disposable library containing the same 100 profiles and Forest as the Svelte candidate. The seed uses stable IDs and instance settings. Docker, the main Carbon Desktop data root, and existing Minecraft data are unnecessary.

For a reusable development preview, run `--seed-fixtures default`, `empty`, `large` or `faults` against the current executable. This seeds a dedicated offline root with varied instances, launch targets and Review tools states, refuses a normal per-user library and reopens with `--fixture-root`. See [development preview fixture libraries](dev-fixtures.md) for the row-to-UI map and truthfulness boundaries.

The ordinary persistent root is `%LOCALAPPDATA%\CarbonDesktopGpui\library-v1` on Windows or `$XDG_DATA_HOME/carbon-desktop-gpui/library-v1` on Linux, falling back to `~/.local/share/carbon-desktop-gpui/library-v1`. Launch with no flags to reopen ordinary data. For a persistent test, point the launched process's data environment at a new directory and seed once.

Pass `--ready-file <fresh absolute path>` to write `CARBON_LIBRARY_READY <milliseconds>` after the first scheduled frame with the populated library and work area. The process also prints that marker to stdout. This is an application-side timing signal. Use an actual on-screen image to qualify paint and a matched external interval when comparing frontends.

## Library and operations

- The searchable library opens a selected instance in a split workspace with Overview and Settings. One task region in Overview shows the next action, progress, or ready state. Setup diagnostics discloses metadata and client integrity checks, custom Java selection and managed JDK preparation. Narrow windows switch to one pane with Back to library and Escape support. Ctrl+F focuses search, Enter selects the first filtered instance, Ctrl+N opens a new instance, Ctrl+O opens native Import pack, Ctrl+W closes the launcher, and F6 focuses Activity. The selected workspace tab supports Left, Right, Home and End. See the [workspace design and behavior](design/gpui-workspace.md).
- Logical folders group Instances and Worlds separately. Folder metadata lives in `personal/folders.json`; filing, renaming, moving and removing never change a profile, world directory, save or launch preference. See [library folders](library-folders.md).
- Profile creation, editing and cloning validate through the shared profile store. Clone creates a separate vanilla profile identity. The selected instance reviews the required full managed JDK and official Mojang file plan before fetching. Its combined operation binds both stages to the reviewed metadata digest, exposes byte/file progress and cancellation, and checks the stored plan offline when verifying. **Prepare from cache** migrates a verified cache receipt into an instance-private game folder before the app marks the game files prepared.
- Play opens setup or account readiness. Microsoft sign-in and entitled Minecraft game execution remain pending. The Windows-only sandbox test uses the same protected sample-session core and has sample saves only. The GPUI edition exposes the action when the managed runtime is ready.
- Import pack reviews a local `.mrpack` in an owned native window, then creates a separate instance and materializes its reviewed overrides on explicit confirmation. Repeating the same confirmation retains the same import identity. Choosing the archive again deliberately creates an independent copy. Remote dependencies and required loaders are reported as pending. Settings exposes the private game folder. Fixture work supports content loss and corruption simulation, repair and generation changes while a personal transfer continues. Activity returns to the owning task.

## Kit, appearance and windowing

[Zed GPUI 0.2.2](https://crates.io/crates/gpui/0.2.2) owns windowing, layout, rendering, key dispatch and jobs. [Longbridge GPUI Component 0.5.1](https://crates.io/crates/gpui-component/0.5.1) supplies Root, focusable buttons and text inputs. The launcher layouts, state and popup forms live in this branch. The exact direct versions and transitive checksums are in `Cargo.lock`. Upstream Apache-2.0 notices are under `third_party/licenses`. The inherited Linux `xattr` patch retains upstream MIT and Apache-2.0 licenses and changes the removed Linux errno alias only. See [patch notes](../third_party/xattr-0.2.3/PATCH.md).

| Carbon treatment | GPUI mapping |
| --- | --- |
| System, Light, Dark | `Palette::read` chooses frame, workspace, raised layer and matching text/border tones. System follows GPUI platform appearance. |
| Desktop grayscale | Contrast for primary actions, a current-row rail, selected tab rule, and an independent keyboard focus ring. |
| Native typography | Segoe UI on Windows and the GPUI platform UI font on Linux. |
| Native frame | Platform caption controls, and on Windows the existing Carbon icon and File/View menus. The launcher remembers its appearance and usable window position in its isolated library. Windows captions follow the selected appearance. |
| Owned windows | Separate GPUI top-level windows for instance forms, import and the protected sample close alert. Windows sets and checks HWND ownership and disables the owner. Linux uses GPUI Floating windows and supplies the active window identifier to the desktop portal. X11 transients were exercised. A Wayland compositor still needs an independent walkthrough. |
| Accessibility | The live visual controls project roles, names, focus, actions and state through AccessKit Windows UI Automation and Linux AT-SPI. Test harnesses exercise the provider separately from pointer input. |

[Rust source for the palette](../crates/gpui-launcher/src/theme.rs) · [GPUI 0.2.2 X11 window source](https://docs.rs/crate/gpui/0.2.2/source/src/platform/linux/x11/window.rs)

## Windows comparison

The final Windows GPUI executable built from `0867142` measured 489 ms median to the painted library and 87.9 MiB median idle private memory in the same 100-profile review. Wheel input and Ctrl+F search during a real download produced visible list changes in 121 and 95 ms. These are wall-clock observations including capture and automation.

For the earlier review builds, the harness used one rehearsal and three measured warm launches per edition on the same Windows 11 x64 host, using debug builds, the same 100 profiles plus Forest, and a 1040 × 760 client area at 96 DPI.

| Candidate | Visible populated library (ms) | Median startup | Median idle private bytes | Process layout |
| --- | --- | ---: | ---: | --- |
| Svelte/Tauri `ba61993` | 622, 1038, 602 | 622 ms | 173.1 MiB | App plus six WebView2 child processes |
| GPUI `fc70067` | 735, 826, 956 | 826 ms | 90.8 MiB | One process |

The external Windows harness checked for a matching populated region in screenshots every 25 ms. After four seconds idle, it sampled private bytes across the process tree five times at 250 ms intervals. The lead recorded samples and captures under the primary checkout's `.artifacts/frontend-comparison/{tauri-final,gpui-final}/`. These three warm samples show lower idle private memory and higher median startup time for this GPUI build. They leave cold launch and other hardware open.

Both editions remained navigable while downloading the real Mojang file plan after preparing the required JDK. The Windows harness observed wheel input and Ctrl+F / 099 to list-pixel changes after 122 and 77 ms for Svelte, and 124 and 88 ms for GPUI. The screenshots show 316 and 242 verified files out of 5,226 respectively. These are single wall-clock observations including input automation and capture. Evidence is in the primary checkout's `.artifacts/frontend-comparison/{tauri-files,gpui-files}/`.

## Evidence and remaining checks

- The common engine in this GPUI worktree matches Svelte/Tauri through `d108b3d`, including the shared setup report and the recoverable pack import identity. The accompanying baseline renderer also incorporates the Linux packaging-only update `57c3931`. Its Svelte check and build passed. The final Windows workspace run passed 70 tests with seven opt-in tests ignored.
- Windows walkthroughs exercised compatible Java folder selection, standalone managed JDK download and verification, real Minecraft 26.3 installation, independent fixture fault and repair during installation, returning to the same job from Activity, and a reviewed pack import. Captures are in `.artifacts/gpui-functional-qa/`. The current Windows handoff also verified actual File and View labels and matching captions in System, Light and Dark. It blocked launcher close with a Carbon-styled owned notice during the protected Java test. Enter returned to the launcher, **Add one** and **Save and close** raised the private sample count from 1 to 2, and Ctrl+W then closed the launcher normally. [Shell verification](gpui-shell.md)
- WSL X11 checks exercised native editor and menu transients, a real owner-bound GTK portal choice, pack import, durable appearance and placement, and narrow keyboard navigation. On the preceding focus-fixed build Orca announced the selected instance, search field and focused `26.3` version choice. The final `0867142` ELF passed the live AT-SPI action walkthrough, including focused search, text entry, profile selection, empty-name validation, version choice and a persisted profile. The harness observed 125 main-window nodes and 12 editor nodes. [Shell verification](gpui-shell.md) · [Accessibility verification](gpui-accessibility.md)

- The grayscale workspace at `ffa2c98` matches the frozen Svelte candidate `11512372` for the shared engine crates and the Svelte/Tauri sources retained in this checkout. Windows workspace tests passed with 68 tests run and seven opt-in cases skipped. The Svelte check reported zero diagnostics and its renderer built successfully.
- The staged Linux grayscale ELF passed the actual X11 walkthrough on Ubuntu 24.04 WSL at 96 and 144 DPI. It loaded the matched 100-instance library, selected 099 using Ctrl+F and Enter, and restored search using Escape after resizing to the narrow detail. The editor opened as a separate window and cancellation left the full list unchanged. Both light and dark palette captures, dimensions and process logs are under `.artifacts/gpui-comparison/workspace-linux/`. The earlier local Ubuntu package below predates this redesign.

- An actual Windows debug walkthrough at `fc70067` verified creation with the initial text focus and Enter, cancellation with Escape and foreground/owner restoration, actual HWND owner and disabled main while the dialog was open, both light and dark client themes, OS file selection followed by a separately owned manifest preview, Forest interruption and resume through Generation B then rollback, and reopening the created profile after a normal exit and relaunch. The measured executable SHA-256 is `02400b879ddc2da01bb29bbfef6c899d938985d7cb7d861cd481bf73f765d276`. The post-benchmark Windows build at `ee52a36` surfaces completion of the managed JDK stage while the game-file phase is active or when that phase fails. Its executable SHA-256 is `290664510cefca6bf3fad4189a79031304e64489b52ec4f0cf7221a9b0ce4948`.
- The wide layout changed from individual dark buttons in the library and boxed tabs to transparent unselected rows, a selected rail and flat tabs. Actual before images are the lead's initial GPUI library capture under the primary checkout's `.artifacts/frontend-comparison/gpui/`. After images and native QA captures are under this worktree's `.artifacts/gpui-native-qa/`. Key files are `personal-dark.png`, `personal-after-relaunch.png`, `profile-create-dark.png`, `profile-create-light.png`, `modpack-preview.png`, `fixture-interrupted.png`, and `fixture-resumed.png`. A lead-operated comparison also verified search and ordinary library scrolling while the real JDK transfer remained active.
- The earlier GPUI `ee52a36` exposed only window-frame nodes. The current AccessKit implementation adds application and modal nodes with live command callbacks. Use [the Windows UIA script](../scripts/check-gpui-uia.ps1) or [the Linux AT-SPI script](../scripts/check-gpui-atspi.py) in an isolated desktop session. Narrator, Orca speech and Windows High Contrast remain separate qualifications.
- WSL built `ee52a36` offline against the lockfile on Ubuntu 24.04.4 x86-64 with Rust 1.96, then painted the 100-profile library under Xvfb, Openbox and Mesa lavapipe at 96 and 144 DPI. Ctrl+F / 099 / Enter selected Review instance 099, and Ctrl+N opened a separate profile window. Escape closed it with all 100 profiles unchanged. The narrow detail and Back to library were exercised at 444 × 680 at 96 DPI and 666 × 1020 at 144 DPI with light and dark captures. An editor reopened in this pass inherited the dark palette. Openbox showed no `WM_TRANSIENT_FOR` or modal state for the popup. This is X11 evidence. The headless WSL session lacked the native chooser services, and Wayland remains unqualified.
- An unsigned local Ubuntu amd64 package was extracted and its ELF reran the complete 96 DPI suite. `carbon-desktop-gpui_0.0.0+review.ee52a36_amd64.deb` is 20,525,980 bytes with SHA-256 `e0babc9fba1432766ed4e80a1cb6c49abb10607a14f8fa27c33d05dd0b9f7af2`. The extracted ELF matches the built artifact at SHA-256 `9285110e58ab95d1ac2864b17b7e01feaa5d49592cb9d4cd1cac94578c04f249`. This qualifies the extracted app in the test session. Installer integration and distribution remain open.

## Review H.264 playback in Home

Use a trusted local FFmpeg executable with the H.264 decoder and a directory of review renders. For each view in the landscape manifest, supply `<id>.mp4` and `<id>.jpg`. The prototype expects silent eight-second loops at 1440 × 810 and 30 fps, with a BT.709 matrix and limited video range. Export posters from the source render.

```text
carbon-desktop-gpui --temporary-review --seed-review --scenery-review-dir <absolute-artwork-directory> --scenery-review-ffmpeg <absolute-decoder-path>
```

Open **Scenery** to select a view. Check moving terrain details against the poster, then try **Pause**, **Resume** and **Library**. Active Home uses a software decoder and uploads frames through the GPUI image path. Switching away retires the decoder. A missing or unreadable clip falls back to bundled motion. Run without the two scenery flags to compare bundled playback at the same window size.

## Package Home scenery beside the launcher

The compiled catalog and its 960x540 frame packs remain the portable fallback. A `scenery` directory beside the executable can replace a view's poster and add a compressed motion loop that the operating system decodes in-process:

```text
<launcher directory>/
  CarbonDesktop...(.exe)
  scenery/
    manifest.json
    <id>.jpg
    <id>.mp4
```

`manifest.json` is version 1 and lists only views that already exist in the compiled catalog:

```json
{"version":1,"views":[{"id":"riverside","poster":"riverside.jpg","motion":"riverside.mp4","fps":30,"frames":240}]}
```

Each entry needs a local JPEG poster, a local MP4 movie, 1-60 fps and 1-3600 frames per loop. The manifest and each view reject unknown fields. The movie's encoded size is the presented size; the decoder does not scale. A package with an unknown field, an unknown ID, a duplicate, a missing file, a path or an unsafe name is ignored as a whole with one message on standard error, and bundled playback continues. A packaged view replaces both the bundled poster and its frame pack, so no animated artwork is duplicated for it. A failed or unsupported movie falls back to the poster.

Stage an existing render set without re-encoding it:

```sh
python scripts/stage-scenery-package.py --source <artwork-directory> --output <launcher-directory>
```

The script copies `<id>.mp4` and `<id>.jpg` for every catalog view, writes the manifest, prints each file's SHA-256 and reports the total payload. A review build can also read an absolute `--scenery-package <directory>` instead.

Windows loads Media Foundation at runtime and decodes with the source reader in a multi-threaded apartment, so an installer ships no codec library of its own; a Windows N image without the Media Feature Pack keeps bundled playback. Linux and macOS keep bundled playback until an in-process decoder is qualified there. Motion reuses the bundled deadline, one-frame-ahead pacing, cancellation and poster fallback, and keeps a warm decoder across pause and resume. A decode that blocks for more than three seconds is abandoned and the view falls back to its poster, and a packaged view never falls through to the local FFmpeg experiment. The [H.264 review path](#review-h264-playback-in-home) remains a separate local experiment with an external FFmpeg and is never used for a package.

Measured on Windows x64 in a debug build with the seven 2560x1440 CRF 16 loops staged beside the probe: 232,095,289 bytes of artwork (226,751,847 bytes of video, 5,343,442 bytes of posters), 18-29 presented frames per second per view, 167% CPU while playing and 1.5% on a still poster. Software 2K decode alone costs about 34 ms per frame, so 30 fps has no headroom in a debug build; release builds and hardware-accelerated decode remain separate qualifications.

Measured on the same Windows x64 machine in a release build with the same seven loops and a 1368x788 probe window: a 30 ms median gap between presented frames, 22-29 fps averaged over an eight-second window, 146-207% CPU while playing, 2.5% on a still poster, 0.5% while paused and a 30 fps resume. A scene switch presents the new poster 0.12-0.21 s after the previous view's last frame, about 80 ms of which is the measurement command round trip, and its first packaged motion frame 0.2-0.35 s after the poster, while that movie's first 2560x1440 keyframe decodes; opening a packaged movie costs 9-71 ms and steady 2K decode 23-28 ms per frame. An earlier revision of this page reported a 1.8-2.0 s stall on every switch. That interval measured the QA driver's own 0.6 s pause and 0.8 s play sleeps. Without them a switch costs 0.37-0.54 s from the previous view's last frame to the new view's first motion frame, including a command round trip of about 80 ms. A warm-open path would remove the remaining poster hold at a scene change but is not needed to avoid a multi-second stall. A rejected package logs one line and keeps bundled playback. An offline review package is a folder beside the launcher holding the unsigned executable and `scenery/`; Windows installer integration and a production default remain open.

The packaged reader asks Media Foundation for RGB32 with video processing and does not request a D3D manager, so the decode figures above are the operating system software path. A hardware-accelerated or zero-copy design remains a separate qualification; the current review package does not depend on one.

That review folder was also assembled into an unsigned local NSIS installer for install-location and uninstall checks. The installer is 379,054,400 bytes, SHA-256 `2ec3402569a645c0e9cb1328379b684168d553e906baac2598bb899ab0f3b41e`, and installs 439,036,718 bytes in 46 files. A silent install to a disposable directory matched all 42 payload hashes; the installed launcher painted a native window from a clean data root and logged a `Packaged scenery: 7 views ...` line; a normal close exited 0; and a silent uninstall removed the install directory, Start Menu folder and HKCU entry. It is a review artifact only. Install location, signing, updates, uninstall policy, production installer integration, a production default and release publication remain open.
