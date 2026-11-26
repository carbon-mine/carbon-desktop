# Desktop CI

The [Desktop CI workflow](../.github/workflows/desktop-ci.yml) builds and tests Svelte/Tauri and GPUI for pull requests targeting `main` and pushes to `main`. **Desktop CI Gate** is the single branch protection check. It requires every native platform job to succeed. [Standard GitHub-hosted runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)

| Platform | Native runner | Rust target |
| --- | --- | --- |
| Windows x64 | `windows-2025` | `x86_64-pc-windows-msvc` |
| Windows ARM64 | `windows-11-arm` | `aarch64-pc-windows-msvc` |
| Linux x64 | `ubuntu-24.04` | `x86_64-unknown-linux-gnu` |
| Linux ARM64 | `ubuntu-24.04-arm` | `aarch64-unknown-linux-gnu` |
| macOS Intel | `macos-15-intel` | `x86_64-apple-darwin` |
| macOS Apple Silicon | `macos-15` | `aarch64-apple-darwin` |

Each job verifies the runner, Rust host, Node and Bun architectures. It checks the inherited design seed, installs from `bun.lock`, checks and builds the renderer, builds both native executables with `Cargo.lock` and runs the workspace tests, formatting and strict Clippy. Tauri embeds the freshly built `dist/`. CI runs both editions without a Microsoft application registration. Opt-in provider and vault tests, graphical walkthroughs, signed packaging and native play have separate verification.

## Reproduce locally

The Windows x64 job also runs the built Svelte interface in headless Microsoft Edge. A disposable bridge fixture exercises install review, cancellation and retry, offline file verification, native-dialog requests, keyboard focus and narrow-window reflow in both themes. Native dialog contents, GPUI rendering and game execution require their own checks. To repeat this renderer check on Windows x64, build the frontend, install the pinned wheels in a Python virtual environment, and run:

```powershell
python -m pip install --only-binary=:all: --require-hashes -r scripts/renderer-ci-requirements.txt
python scripts/test-profile-renderer.py --directory dist --channel msedge
python scripts/check-tauri-account-renderer.py --channel msedge
```

Use the native Rust, Node and Bun versions in the workflow. Python 3 runs the design seed verifier. CI verifies the SHA-256 for each architecture’s Bun archive. [Bun release](https://github.com/oven-sh/bun/releases/tag/bun-v1.4.2)

- **Windows** requires the MSVC build tools for the host architecture and the Windows SDK. The Windows fixture compiler requires a native JDK capable of `javac --release 17`. CI uses Temurin for x64 and a checksum-pinned Microsoft Build of OpenJDK for ARM64. Tauri also uses WebView2. [Microsoft OpenJDK](https://learn.microsoft.com/en-us/java/openjdk/download) · [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)
- **Ubuntu** requires the development packages in the workflow, including GTK, WebKitGTK, OpenSSL, D-Bus, Wayland, XKB, XCB and Vulkan. Running GPUI interactively also requires a suitable graphical session.
- **macOS** requires full Xcode, the selected macOS SDK, Clang and Metal command-line tools, and CMake. CI uses Xcode 16.4 on both architectures. For local Xcode versions that ship Metal separately, install the Metal toolchain before building GPUI. [Apple Metal toolchain setup](https://developer.apple.com/documentation/xcode/downloading-and-installing-additional-xcode-components)

```text
python design-system/verify_seed.py
bun install --frozen-lockfile
bun run check
bun run build
cargo fmt --all -- --check
cargo build --locked -p carbon-desktop -p carbon-desktop-gpui
cargo test --locked --workspace
cargo clippy --workspace --all-targets --locked -- -D warnings
```

For disposable local review, run `target/debug/carbon-desktop --temporary-review --sample-vanilla --seed-review` or `target/debug/carbon-desktop-gpui --comparison-review`. Use the corresponding `.exe` on Windows. See the [GPUI runbook](gpui.md) for portable Windows shader builds and the [Linux runbook](linux-build.md) for Debian packaging.

## Before merging

Review the final diff and resolve findings on the pull request. A change to player-facing behavior requires a UI and interaction review. Exercise the affected journey in a disposable library at wide and narrow sizes. Inspect layout hierarchy, spacing, typography, active and focus states, native dialogs, keyboard navigation and error recovery. Check supported light and dark appearances where the change affects rendering. Record the build, affected edition, surfaces actually inspected and any unverified platform behavior. Attach representative screenshots when the UI changes. For a backend-only PR, identify which user-visible behavior was checked or why the change has no UI impact.

Address actionable human, automated and independent review findings before merge. Run the required checks on the updated branch. The development workflow builds and tests without publishing packages. Release publication is a separate owner decision.
