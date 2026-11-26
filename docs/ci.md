# Desktop CI

The [Desktop CI workflow](../.github/workflows/desktop-ci.yml) builds Svelte/Tauri and GPUI on Windows x64 and Ubuntu 24.04 x64 for pull requests targeting `main` and pushes to `main`. The branch protection check is **Desktop CI Gate**. It runs after the platform jobs and requires both to succeed.

Each job verifies the inherited design seed, installs from `bun.lock`, checks and builds the renderer, builds both native executables with `Cargo.lock`, and runs the workspace tests. The Tauri executable embeds the freshly built `dist/`. CI runs without a Microsoft application registration and leaves opt-in provider and vault tests out of the default suite. Keep graphical walkthroughs, signed packaging and native play qualification in the relevant platform runbook.

## Reproduce locally

Use the Rust, Node and Bun versions in the workflow. CI checks each platform archive against a SHA-256 from the [official Bun release](https://github.com/oven-sh/bun/releases/tag/bun-v1.4.2). On Windows use the x64 MSVC toolchain, the Windows SDK and a JDK with `javac` capable of compiling Java 17. On Ubuntu install the native development packages listed in the workflow. Python 3 runs the design seed verifier. [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

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
