# Build, run and review

## Windows development host

Use an x64 MSVC Rust toolchain, Visual Studio C++ build tools with Windows SDK, WebView2 Runtime, Bun and Node. Tested with Rust 1.99.1, Bun 1.4.2 and Node 26.13.1. Java is optional for the separate containment experiment. The lockfiles pin the accepted build inputs. The renderer uses Svelte 5.59.2 and Vite 8.3.8. The native build uses Tauri 2.12.7 with CLI 2.12.6. TypeScript 5.9.3 matched the development checker. The Windows bundle is an unsigned development NSIS installer at version 0.0.0.

Run from the Desktop checkout:

```powershell
bun install --frozen-lockfile
python design-system/verify_seed.py
bun run check
bun run build
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python scripts/make-preview.py
bun run tauri build --debug --bundles nsis
.\target\debug\carbon-desktop.exe --temporary-review --sample-vanilla --seed-review
```

Close an existing executable in `target/debug` before rebuilding it on Windows. The presentation copy can stay open from its separate ignored artifact directory. The development loop is `bun run tauri dev`. For a populated disposable walkthrough use `--temporary-review --sample-vanilla --seed-review`. A fresh personal instance named Vanilla survival opens in Library. Review tools contains the separately seeded Forest fixture. Both use temporary storage for the lifetime of that process. Default startup opens the saved library. Adding `--seed-review` initializes Forest if the fixture collection is empty. Real worlds and account data are never inputs to this demo.

The generated Windows installer lives in `target/debug/bundle/nsis/`. The executable runs directly without installing it. For Linux build prerequisites, the isolated WSL SDK and package checks, see [the Linux runbook](linux-build.md). Recreate the package icons from the local seed with `bun run tauri icon design-system/inherited/brand/mark-light.svg --fit contain -o .artifacts/icons`. The shell uses the generated `icon.ico` and `128x128.png` as input resources.

## Native checks

```powershell
cargo run --locked -p carbon-isolation-probe
.\scripts\windows-java-fixture.ps1
wsl -e bash scripts/linux-fixture.sh
uv run --python 3.12 --no-project --with pywinauto==0.6.9 --with Pillow==13.0.0 --with psutil==7.2.2 -- python scripts/windows-walkthrough.py --prepare-java
```

The WSL command names this review host's distro and ordinary user. On another machine run `sh scripts/linux-fixture.sh` as an ordinary Linux user with bubblewrap. The probe uses a newly allocated temporary directory and removes only its own disposable input. The Windows probe creates fresh OS profiles, supplies a filtered environment, attaches the job at process creation and checks the token before resuming. It checks the fixed parent and child's private writes and cross-instance read denials. The developer script compiles a repository-owned Java helper with the installed JDK, then runs the processor class inside AppContainer. It checked scoped file reads and writes and an AWT native window peer on the review host. Source-file compilation inside this AppContainer failed to close compiler resources; execution of the precompiled fixture passed. Minecraft, real loader processors, GPU throughput, audio and input still require separate native tests.

The UI Automation walkthrough creates a separate temporary library through the normal app entry point, drives the embedded WebView and native picker, and writes captures under `.artifacts/windows-review/`. It requires an unlocked Windows desktop. Keep the keyboard available to the walkthrough during its focus check. UI Automation exposes names, roles and focus. Narrator speech and user acceptance require a separate human walkthrough.

## Pack preview

`python scripts/make-preview.py` reproduces `.artifacts/samples/forest-preview.mrpack` from an owned fixture with one descriptor and one override. In the library choose **Preview pack** and select this file. The dialog shows the pack and game versions, listed payload size and file counts. An invalid ZIP or unsupported index yields a reason, and the player can return to the library. The picker reads the selected file only. The Rust preview checks ZIP layout, names, lengths, entry types, required hashes and HTTPS descriptor syntax. It bounds the archive at 16 MiB and index at 256 KiB. It does not extract entries, request descriptor URLs or authenticate the publisher. [Modrinth's format specification](https://support.modrinth.com/en/articles/8802351-modrinth-modpack-format-mrpack)

## Cleanup and persistence

Close the review window to end the session. Disposable review roots contain only the sample data created for that run. Normal per-user data is under the OS local application data directory for `dev.carbonmine.desktop.review`, in `fixture-library-v1`. That library includes the player's authored personal profiles. Keep the entire normal library when rebuilding or moving the app. To put build outputs aside reversibly, close the affected program and rename `target`, `dist` or `.artifacts` inside this checkout. Move them back before using the recorded artifact paths. Source, fonts, worlds and personal libraries are outside these build directories.
