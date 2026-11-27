# Development handoff

The current Windows development candidate opens personal instances in a split desktop workspace. A saved instance can check official Minecraft metadata, prepare a verified Java runtime and import a matching client JAR. See the [setup decision](adr/0002-standalone-setup.md), [vanilla verification](vanilla.md) and [runtime qualification](runtime.md). The original fixture candidate is documented below. See [contributor guidance](../AGENTS.md) and the [kickoff](../../carbonpanel/docs/plans/carbon-desktop-implementation-kickoff.md). Native play requires both a qualified Carbon runner and an approved entitled session.

## Current standalone setup candidate

The Windows x64 Java sample now checks its private job's active process count before reporting completion. The native regression test starts two independent process trees with fixed test programs. Both parents exit while their children remain alive. Stopping the first job twice leaves the second child running, and allowing the second child to finish drains its job. A lost launcher's released `pending-run-v2` record is retired only after the named run job is absent or empty; a live job keeps the instance fenced. Run `cargo test --locked -p carbon-session` on Windows x64 to repeat the check. The Java sample retains its one-process limit. Production game supervision and interactive close handling for real runs remain open. [Windows job accounting](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_basic_accounting_information)

Local app commit `f1bf9a2` includes the profile compatibility checkpoint `a5e6551` and content preparation checkpoint `fe039f0`. These are unsigned development packages using the Rust debug profile and production Svelte bundle. From the checkout, launch the Windows review copy with an independent temporary library:

```powershell
& '.\.artifacts\review\f1bf9a2\Carbon Desktop.exe' --temporary-review --sample-vanilla --seed-review
```

The initial **Vanilla survival** instance uses Minecraft 26.3. **Check setup** fetches its public version metadata. **Prepare managed Java** downloads the pinned Temurin runtime and checks its files. **Choose Java folder** reads the selected installation's declared platform and version. **Import client JAR** accepts a selected file only after its size and SHA-1 match the pinned version metadata. A real game window, entitled sign-in, world saves and online joins remain pending.

| Review artifact under `.artifacts/review/f1bf9a2/` | SHA-256 |
| --- | --- |
| `Carbon Desktop.exe` | `36c00b2bc853bad198dc8644ee0830af0672ecd605cf0ef784d5e7ea593d12be` |
| `Carbon Desktop_0.0.0_x64-setup.exe` | `5828777ee7cbb6e4637d6fdbfe97be0712abeb229b12f779d45011c2cf7d4278` |
| `Carbon Desktop_0.0.0_amd64.deb` | `8d0955ccb62a55e426e17b08eb0ca7487f9c36945efac7c1bcde20481dab9b95` |

The locked Windows workspace test run passed 45 tests. The live provider checks separately prepared three Minecraft versions on Windows and three pinned Temurin majors each on Windows and WSL2. Svelte diagnostics, Rust formatting, strict Clippy, 25-file design seed verification and the built renderer's Edge checks passed. Windows AppContainer parent/child checks and the WSL2 namespace probes passed on their fixed programs. The scoped native bridge and interrupted mutation review is documented in [vanilla preparation](vanilla.md). The [renderer timings](performance.md) qualify the split library independently of the packaged UI walkthrough.

The Linux build matched its before/after source manifests. Its observed executable SHA-256 was `7f2878bee6507636f250e4d30c2be835bdd459b7391885ef7d0600b4a967b13f`. The Debian package declares GTK, WebKitGTK, OpenSSL 3 and certificate roots. In an ordinary-user WSL2/Xvfb run its 1040 × 760 window showed the personal instance, native menus and Setup action. The renderer had a nested PID namespace and active seccomp filter. The subsystem capture is in `.artifacts/linux-build/window-check-3Fw58Qrc/carbon-window.png`. Physical Linux desktop, game, GPU, input, audio and assistive technology acceptance still require an authorized native host.

The native Windows UI Automation walkthrough created Vanilla survival, checked Mojang metadata, prepared Java 25.0.5.1+2, and observed the compatible runtime in the application. Cancelling the client picker preserved the report. Selecting a wrong-size local JAR produced a specific error and a fresh setup check restored the report. The run also edited and cloned settings, found the narrow library using Ctrl+F, resumed and rolled back the interrupted fixture update, installed exact and overlapping fixtures and previewed a local pack through the native picker. Window captures live in `.artifacts/windows-setup-review/`. In this warm run, first use was interactive after 0.71 seconds and sampled private memory peaked at 284.1 MiB across the launcher tree. Separate checks on the presentation copy exercised the native File and View commands and left Vanilla survival selected for the owner.

## Svelte/Tauri account checkpoint

The account toolbar and native dialogs use the same Rust account service as GPUI. The account registry and vault slots follow each edition's app root. See [account boundaries and review commands](account.md#sveltetauri-account-window).

The credential-free Windows debug executable built successfully. The locked launcher and account test run passed 35 tests. The interactive vault test remains opt-in. Strict Clippy, the Svelte build and type check, and the 25-file design seed verification passed. An unprivileged WSL2/Xvfb test on GTK 3.24.41 and WebKitGTK 2.52.6 opened the account dialog attached to the launcher window, displayed the missing-registration message, dismissed it with Escape, and opened it again from the toolbar. Native Windows dialog interaction, native Linux desktop and assistive technology checks remain open. Live sign-in and entitlement verification need Carbon's approved registration and an entitled test account.

## Roots and inputs

The [vanilla launch adapter checkpoint](discovery/launch-adapter.md) adds one trusted Rust planner for 26.3 and 24w14a. The pinned corpus passed on the Windows host and under WSL2, and the WSL2 argument pass recorded no helper, socket or write attempts. The Play gate remains in place. Both shells still need the approved account decision and trusted runner for admission.

The verified Desktop root was an empty sibling of the saved Panel checkout. Local Git was initialized here on branch `main`. The saved design source is Panel commit `38cfd2263d20ac97bc9051be461201598ca932ad`. Several product documents are untracked in that checkout. Their SHA-256 identities below identify the working copies used for this build.

| Saved Panel input | SHA-256 |
|---|---|
| Product requirements | e74566e828e7d4ffc42e12621c5e1fb2f66a3582dca16d3036dd6de1a73d4023 |
| Implementation kickoff | f8e9cb5528448ba4d3c03c4bc2c332e483ee7f01ffc772dedb73110cea8032d6 |
| Launcher reuse | 7a09b5ad9ccfabbc1242457f7f16a470c154e1f82169c6d20a42e43577f5617c |
| Registry and client feasibility | c1be713a9971ce58f4390267ac03b110b9277b0ec8bab8baf0a08af4baffad52 |
| Client isolation | 3c5b8cf00f8913036db15ba1bf6ab0bbc2cb343fd2206112e28008a46078f8e2 |
| Registry product proposal | 57ef6be9d809086bd08fdb63341fe5c4674a01960cfbbb9eb1fc4723d9cfc60a |

## What to run

Open the [build and run guide](implementation.md) for the executable, packaging commands, native picker fixture, walkthrough and data-preserving cleanup. The Desktop root is the sibling named `carbon-desktop` beside the product inputs in `carbonpanel`. The original checkout was empty. All implementation checkpoints are local on `main`.

| Ordered slice | Input and owner | Result and acceptance |
|---|---|---|
| M0 scope, references and layout | [Launcher comparison and briefs](discovery/README.md), [interactive layouts](design/layout-options.html), [owner decision](owner-review.md) | Owner selected A. The Desktop-owned [seed](../design-system/README.md) pins 25 Panel files. Discretionary grouping, finer cloning and shortcuts require separate decisions. |
| M1 native trust and sessions | [Boundary decision](adr/0001-desktop-boundaries.md), [account fixture](account.md), [independent review](critical-review.md) | Two Windows AppContainer identities and a contained Java fixture passed scoped checks. WSL2 filesystem probes passed. The app has no game execution or reusable credential path. |
| S1 personal profile authoring | [Profile core](profiles.md), native Library and profile form | Create, edit, clone, search and reopen local settings. Memory and window bounds, ID stability and independent edits have tests. Pinned versions are configuration choices. Real installation and gameplay require independent qualification. |
| S2 content preview and reuse | [Picker walkthrough](implementation.md), [transaction and benchmark evidence](storage.md) | Inspect a selected Modrinth v1 manifest. Local fixture installation verifies and reuses identical objects across profiles. The 1 GiB loopback workload measures actual HTTP body and filesystem allocation. Import materialization, provider search, export, real modpack play and real world backup/restore remain open. |
| C1 local recovery model | Owner-supplied recovery direction and app-owned A/B inputs | Interruption leaves A selected and verified. Retry stages B, verified activation preserves sample player data, and a B-only save refuses incompatible rollback. The sync owner supplies authority, signed fixtures and independent registry before interoperability can be tested. |
| Development review | [Windows walkthrough](implementation.md), [Linux package](linux-build.md), [review log](owner-review.md) | Development packages were built for Windows x64 and Linux x86-64. Native Windows app observations and WSL-built Linux artifacts have distinct evidence. Owner acceptance applies only to explicitly reviewed tasks. |

### Ownership and rerun boundaries

Rust in `crates/profiles` owns vanilla configuration. Rust in `crates/store` owns immutable fixture objects and player/backup transactions. `crates/account` owns synthetic entitlement decisions. `crates/isolation-probe` and `scripts/linux-fixture.sh` own the fixed native and subsystem experiments. `src-tauri` owns data-root selection, read-only pack parsing, native menus, system pickers and the scoped commands for the library, fixtures and personal setup. `src` presents the owner's chosen layout; platform commands stay out of `design-system`. These paths are verified by the commands in [the runbook](implementation.md) and each linked packet. Positive and negative checks, source versions and approval dependencies are in [the feature briefs](discovery/README.md).

### Remaining gates

1. Qualify a native Linux desktop with a supported compositor, runtime dependencies and representative hardware. Run the package there, the same keyboard/picker checks, screen reader and graphics/audio/input observations. WSL2 supplies build and bounded filesystem evidence.
2. Supply the Carbon application's approved public-client registration and an entitled test account for official browser sign-in. Qualify protected storage, per-launch broker, runtime selection and game/loader setup. Then open a real standalone world, save and reopen, back up and restore, and join an ordinary online-mode server. Existing launcher identities and account data are outside this build.
3. Qualify a game-visible immutable file view, verify allowed writes, protect other instances/credentials and test native process-tree, route and device boundaries on both desktops. Measure the qualified default filesystem on native Linux, actual peak staging and launcher/JVM footprint separately.
4. Complete authorized catalog access, pack materialization and loader-specific denial paths, actual imports/exports, offline policy and release compatibility. Keep version/loader/provider combinations individually scoped until exercised.
5. Integrate the sync workstream's versioned contract and signed releases A and B from an independent registry. Test signer scope, rotation, withdrawals, revocation, rollback and a matching online server. The recorded local fixture supplies no signature or protocol acceptance.

Review the next iteration against the owner's [hands-on record](owner-review.md). Keep native gameplay, manual acceptance, signed distribution and public release status separate.

## Original November 26 profile candidate

The app source is local commit `693fc7452ba995b8d1aa5899faaace6ad22735e2`. Build both packages with the embedded production Svelte bundle and the Rust debug profile. The review executable was rebuilt from this commit and opened on Windows. Subsequent handoff and measurement files do not change the app. Reproduction paths are relative to this Desktop checkout.

| Artifact | Path | SHA-256 |
|---|---|---|
| Open Windows x64 executable | `.artifacts/review/693fc74/Carbon Desktop.exe` | `83e6c30ec2302acdaf31e280383e8ef8a6477f00a26cc677fd79ca2334f45a7f` |
| Unsigned Windows NSIS package | `target/debug/bundle/nsis/Carbon Desktop_0.0.0_x64-setup.exe` | `42929c471a7971f22bf50bfc99599c554e2894208b13b54320f6825a990bede4` |
| Linux x86-64 executable | `.artifacts/linux-build/target/x86_64-unknown-linux-gnu/debug/carbon-desktop` | `17ca29cfaf82547ff9d00a5921df494dc63e712218071616300dece556021b53` |
| Unsigned Linux Debian package | `.artifacts/linux-build/target/x86_64-unknown-linux-gnu/debug/bundle/deb/Carbon Desktop_0.0.0_amd64.deb` | `fc4d0f8dd1e5195d9f1459f97aac174f165b9376365ffc89c404af6a14bb1522` |

Relaunch the exact Windows review executable from PowerShell:

```powershell
& '.\.artifacts\review\693fc74\Carbon Desktop.exe' --seed-review
```

Windows 11 Pro x64 build 26300, Core i9-12900HK, 64 GiB RAM and NTFS were available. The host reports Intel Iris Xe and NVIDIA RTX 3050 Ti Laptop GPU. No game rendering benchmark was run. The Linux build host was WSL2 Ubuntu 24.04.4 x86-64. The [Linux runbook](linux-build.md) records its compiler, library versions, before/after source manifest and inspected Debian payload.

The integrated Windows Rust suite passed 29 tests. The additional opt-in 1 GiB workload passed on both NTFS and WSL2 ext4. Formatting, Clippy with warnings as errors, frozen frontend install, Svelte diagnostics, embedded production frontend build and 25-file design provenance checks passed. The built renderer's isolated bridge tests passed for independent library failures, uncertain mutation recovery and Unicode profile names. Repeat with `python scripts/test-profile-renderer.py --directory dist --channel msedge` using Playwright and Edge.

The native Windows walkthrough exercised personal create/edit/clone, the real system picker, Play refusal, repeated and overlapping installs, interrupted update/resume/activation/rollback and narrow keyboard navigation. Captures are in `.artifacts/windows-review/`. In one automated run the empty library's install action was accessible after 0.90 seconds. The launcher process tree used 165.0 MiB private bytes after the first sample install and peaked at 210.6 MiB in 200 ms samples. This was a warm development-host run. It excludes a game JVM, and does not establish a cold-start percentile or steady idle use.

The order of tested local checkpoints was `c69188a` (scope and inputs), `870dc7d` (account and storage), `a1d1244` (design seed and direction), `5a4cc0f` (personal profiles), `1973a64` (HTTP and disk reuse) and `693fc74` (integrated native review build).

The [100-record library and archive benchmarks](performance.md) report headless renderer p50/p95 and concurrent parser measurements. The 5,000-entry and 5,000-descriptor samples reached the documented limits and were refused. The [critical review](critical-review.md) identifies the independently tested recovery fixes and the remaining boundaries.

A fresh Git export of the app commit reproduced the frontend JavaScript asset identity, verified all 25 inherited files and passed the locked native Cargo check with its six command permissions regenerated. A WSL2/Xvfb launch also displayed the seeded Forest library through the packaged Linux executable and native bridge. The default offscreen run supplied UI evidence. A separate run requiring the WebKit sandbox rendered the same library with nested PID namespace and seccomp filtering observed on its web-content process. See [the offscreen Linux window check](wsl-window.md) for the capture, process-specific observations and remaining environment limits.
