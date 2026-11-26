# Desktop CI

The [Desktop CI workflow](../.github/workflows/desktop-ci.yml) builds and tests GPUI and the shared Rust services for pull requests targeting `main` and pushes to `main`. **Desktop CI Gate** is the single branch protection check. It requires every native platform job to succeed. [Standard GitHub-hosted runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)

| Platform | Native runner | Rust target |
| --- | --- | --- |
| Windows x64 | `windows-2025` | `x86_64-pc-windows-msvc` |
| Windows ARM64 | `windows-11-arm` | `aarch64-pc-windows-msvc` |
| Linux x64 | `ubuntu-24.04` | `x86_64-unknown-linux-gnu` |
| Linux ARM64 | `ubuntu-24.04-arm` | `aarch64-unknown-linux-gnu` |
| macOS Intel | `macos-15-intel` | `x86_64-apple-darwin` |
| macOS Apple Silicon | `macos-15` | `aarch64-apple-darwin` |

Each job verifies the runner and Rust host architecture, the design seed and Cargo.lock. It builds GPUI, runs the workspace tests and checks formatting and strict Clippy. Windows jobs install a matching fixture JDK. macOS uses Xcode with the Metal compiler. Linux installs the native window, font, Vulkan and desktop service libraries listed in the workflow.

## Local checks

```sh
python design-system/verify_seed.py
cargo fmt --all -- --check
cargo build --locked -p carbon-desktop-gpui
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo run --locked -p carbon-desktop-gpui -- --comparison-review
```

Use the disposable library for visual and keyboard checks. Native play, OS credential stores and interactive accessibility require their own verification. The [GPUI guide](gpui.md) documents host setup and development runs.

## Before merging

Reviewed pull requests enter the `main` merge queue after their individual six-platform checks pass. GitHub reruns the same workflow on each candidate combined with the current base and the changes ahead of it. The queue requires all candidates to pass and squash-merges them in order. Check a removed candidate's merge-group run for failures or conflicts before requeuing. [GitHub merge queue](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue)

Review the final diff and resolve findings on the pull request. A change to player-facing behavior requires a UI and interaction review. Exercise the affected journey in a disposable library at wide and narrow sizes. Inspect layout hierarchy, spacing, typography, active and focus states, native dialogs, keyboard navigation and error recovery. Check supported light and dark appearances where the change affects rendering. Record the build, affected edition, surfaces actually inspected and any unverified platform behavior. Attach representative screenshots when the UI changes. For a backend-only PR, identify which user-visible behavior was checked or why the change has no UI impact.

Address actionable human, automated and independent review findings before merge. Run the required checks on the updated branch. The development workflow builds and tests without publishing packages. Release publication is a separate owner decision.
