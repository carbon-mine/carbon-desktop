# Independent review of the local fixture candidate

The Windows review on 26 November 2026 covered the public Rust store entry points, Tauri command registration and capability file, the Svelte action path, and the two fixed filesystem probes. All runtime checks used synthetic inputs. Minecraft Play remains unavailable for every fixture instance. The evidence below applies to the reviewed source revisions.

## Findings and verification

| Finding | Observation | Disposition |
| --- | --- | --- |
| A second activation retained an obsolete pre-B backup | Through the public store API, activate B, roll back, change A's save and setting, then activate B again. Initially the current values were 1 and 1 while the backup still held 0 and 0. | Fixed. The store validates the existing backup and atomically writes the current player state before each B activation. The repeated-activation regression now observes the latest A edits. |
| An edit could succeed while returning an ordinary failure | An unreadable setting in the repeat profile made an edit in the sample profile return an error after its setting changed from 1 to 2. Retrying the action could apply a second edit. | Fixed for this case. The store preflights the entire snapshot under the library lock. A regression also injects a read failure after success and checks that the error explicitly reports the completed action and requests a reload. The revised view blocks further mutations after an ambiguous error and provides a Reload library action. |
| Repair of a staged target was unreachable from the main action | With only B's staged object corrupted, A is healthy. The backend refused activation and advised repair. The original Svelte condition offered repair only when A was unhealthy. | Fixed in source. The view now exposes Repair for a healthy active generation, including a direct repair action in the error panel after target verification fails. The store regression verifies repair followed by activation. |

The synthetic account fixture previously reaccepted the positive fixture outcome if the supplied clock was moved back after expiry. It now records the greatest observed time and rejects earlier evaluations across account selection changes. The post-expiry regression passed. All outcomes still deny live play. [Account fixture](account.md)

The exclusive library lock serializes app writers. The content lease protects the selected generation while a cooperative reader holds it. Healthy objects are checked against a pinned size and SHA-256. The index selects the active generation. Staging preserves A, and activation refuses a damaged target or a conflicting reader. A B-only save blocks rollback to A. The data store assumes that only trusted app processes control its root. Reparse-point and link checks at individual paths do not establish a sandbox for arbitrary concurrent local writers.

Windows unit and subprocess checks exercise interrupted staging, simulated storage exhaustion, process termination on either side of active-index publication, shared-object repair, and independent private state. Hard power loss and a full physical disk were untested. A crash between object publication and index publication can leave unreferenced verified objects in the cache. The persisted source counter can miss the initial write in that interval. [Fixture storage and measurement boundaries](storage.md)

## Process and session boundaries

| Surface | Verified evidence | Remaining qualification |
| --- | --- | --- |
| Windows AppContainer | On native x64 Windows build 26300, the host verified the requested process SID before resume. In each direction a parent and child read and wrote their own fixture and observed permission denied on a sibling marker. The host verified both result files. The source supplies no inherited handles, requests zero capabilities and includes a kill-on-close job in the initial process attributes. | The parent and child additionally check membership in a job. A null job handle tests membership in any job. The specific job identity and kill-on-close termination have no independent observation here. Network policy, sensitive host resources, a real loader and game process, graphics, audio, input and session handoff require their own checks. |
| Windows Java fixture | A development script compiled the repository-owned observation class with annotation processing disabled. The AppContainer parent launched Temurin 25.0.5.1+2. The class read its own input, received access denied on the sibling input, and created, showed and disposed a displayable AWT Frame. The host verified the result markers. | The JDK compilation occurred on the host. The contained program used a fixed class. This checks an AWT window peer and file access only. |
| Linux namespace probe | An ordinary WSL2 user ran two Bubblewrap views. Each child read the shared fixture, could not overwrite it, and wrote a private marker. The parent's host temp path, host Windows mount and home directory were absent in the child view. Kernel `6.6.87.2-microsoft-standard-WSL2`, ext4 temp storage, Bubblewrap 0.9.0. | Native desktop launch, native Linux runtime/storage selection and live gameplay require an authorized native Linux session. |
| Tauri and Play | The generated local `main` capability grants four named commands for reading the library, changing fixture state, checking Play and previewing a pack. The Rust side owns the root and accepts a closed action and instance vocabulary. The Svelte Play action calls readiness only. All three IDs return unavailable in the Rust test. The CSP limits script sources to self and connections to the Tauri IPC transports. | Static bridge review and direct Rust tests do not exercise every runtime-origin denial. No account provider, game credential, scoped launch ticket, OAuth callback or Minecraft executable passed through the reviewed bridge. |
| Pack preview | The fourth command has no path argument from the webview. A parented native dialog supplies a selected file. Rust reads a bounded archive and index, returning display labels and counts only. There is no extraction, URL fetch, installation or launch operation. | A file picker can access a user-mounted network filesystem. Preview metadata supplies no authenticity or pack-playability evidence. |
| Managed updates | The local A/B fixture exercises recovery, reuse and save preservation. An eligible A can retain a healthy selection during interruption. | The fixture contains no game launch, signed release envelope, independently hosted registry or versioned wire contract. External interoperability and entitled gameplay await their respective inputs. |

## Repeat the checks

From the Desktop repository root, with the locked toolchain dependencies available, run these separately.

```sh
cargo test --locked --workspace
cargo run --locked -p carbon-isolation-probe
sh scripts/linux-fixture.sh
```

The first command passed 12 store tests (including the subprocess helper), five account tests, two pack preview tests and one Play-denial test after the two storage fixes on native Windows, using Rust 1.99.1 and NTFS. The second command is the native Windows experiment. The third command is the Linux experiment and needs an ordinary Linux user. Both printed a pass. In PowerShell, with a JDK on PATH, run `./scripts/windows-java-fixture.ps1` for the Java variant. It passed both the Java and parent/child checks in the independent run. The final Svelte check, `bun run check`, reported zero warnings or errors.

## Reviewed source identities

The implementation was still in the working tree during review. Initial store observations correspond to SHA-256 `bf0b5851c16d45db184c69ba20aa574263f063dfaf142f2e2fb6a189223c0d89` for `crates/store/src/lib.rs`. Verify the following post-fix source identities with `git hash-object -- <path>`. Only the named files and behavior above were reviewed.

| Paths, in order | Git blob IDs, in order |
| --- | --- |
| `crates/store/src/{lib,tests,disk,fixtures}.rs` | `914b5e6492502d401bb3a5363f4e22e8def7e944`, `b056afbf9c9541b5a7e74d003a8ab4648d4a157b`, `2a8683d413750fa86b76b04255f63e04da9a68b8`, `1409e5957d9334792eaf885b9245c66105ecef3e` |
| `crates/isolation-probe/src/{windows,main}.rs`, `scripts/linux-fixture.sh` | `72f6a2d7d8b855f37385fe742be5e20e8ffa6eed`, `5a0fce42ab2a90a54c61277cb2035c4fe39ef89a`, `826a72a17876235afed2456a824e7848a8c828cb` |
| `src-tauri/src/main.rs`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/review.json` | `fe0b2b1923651e0b8114cef2fdadef84a52cd02d`, `72cce6a0019ed5296f3482309c1736ca1a3cbf3d`, `ee4a783ee0533fd256d25f703f4f4f11d960ad40`, `f6a9c4cf6379564a56597472e6d970af9b3583f8` |
| `src/App.svelte`, `src/library.ts` | `7c7525410d82c98536e8548ab7bb0ffafa067e81`, `4fcca9dc02f7ed70323340116d3941ff84b34fde` |
| `src-tauri/src/pack.rs`, `crates/account/src/lib.rs` | `4853ccb30a5ada85468d52f17c75592cc8333937`, `9935ee6fc6f8f355d1229007541fa1b96a891270` |
| `crates/isolation-probe/FixtureProcessor.java`, `scripts/windows-java-fixture.ps1` | `51ee5624edc0d30bd64e48b5433f648c2084e906`, `b3fecb969c52a099aed13ff2db922ddcee6223f4` |

## Incremental review of personal profiles and HTTP fixture

This addendum reviews working-tree changes since `870dc7deababcf3d0394b2ff149d1fa14e31c252`.
It covers the six-command bridge, personal profile persistence and renderer,
and the new fixture source seam. The four-command bridge entry above
describes the earlier review snapshot.

| Finding | Disposition |
| --- | --- |
| **P2 · A failed library load discarded the other library's successful response.** The initial renderer awaited fixture and personal data as a group. Either failure kept the healthy result out of the UI. The shared uncertainty flag also blocked unrelated mutations. | Fixed. Each namespace loads independently, retains its last successful view, offers a scoped reload and disables only its own actions when its state is unavailable or unconfirmed. The preview remains independently available. Browser regressions cover both startup failures and lost replies after committed operations in both namespaces. |
| **P3 · Profile names had different character limits in the renderer and core.** The input capped names at 80 UTF-16 units while the core allowed 80 Unicode scalar values. The old clone suggestion could also exceed the limit. | Fixed. The renderer counts Unicode scalars after trimming and associates a visible validation message with the field. The proposed clone name leaves space for its suffix. The browser regression accepts 80 supplementary characters, refuses 81, and confirms clone dialog focus return. |

- **Bridge and shared root.** The native shell supplies the same app-owned
  directory to both cores. The command manifest, generated permissions,
  local main-window capability and handler list agree on six commands. The
  two new commands return typed personal metadata and take a closed profile
  action vocabulary. Neither takes a filesystem path, provider endpoint,
  session credential or executable argument. Personal editing writes only
  the locked `personal/` index and its adjacent staging files. Fixture player
  data stays under named fixture keys and separate locks. A direct public-API
  run initialized both cores concurrently in one disposable NTFS root,
  created same-name personal profiles, edited one clone's memory and changed
  only the repeat fixture's save. The original profile and Forest sample save
  retained their previous values on reopen. This confirms persistence and
  namespacing, and does not establish process isolation for future games.
- **Fixture transport.** Only the test module binds an IPv4 loopback port.
  The actual bridge opens the built-in fixture catalogue and never accepts
  caller-supplied URLs. The default HTTP tests checked request and byte
  deltas, profile-specific mutable data and recovery from a short response
  during staging. A verified active A and its save survived before B could
  be resumed and activated. The 1 GiB benchmark is opt-in and was excluded
  from this independent run. [Recorded benchmark](storage.md#loopback-http-transfer-and-1-gib-workload)
- **Verification.** Native Windows Rust 1.99.1 passed seven profile tests
  and 14 store tests. `bun run check` passed with zero errors and warnings.
  The isolated production renderer build and Edge regression passed using
  a browser-local bridge. The browser test exercises the renderer contract.
  Native runtime origin-denial checks, live provider traffic, real world
  directories, native Linux desktop and real gameplay have separate gates.

Repeat the renderer check with Playwright and Edge installed. On a host
with Playwright's bundled Chromium, omit the channel argument.

```text
cargo test --locked -p carbon-profiles -p carbon-store
bun run check
bun run build --outDir .artifacts/renderer-test
python scripts/test-profile-renderer.py --channel msedge
```

| Incremental source paths, in order | Reviewed Git blob IDs, in order |
| --- | --- |
| `crates/profiles/src/{lib,storage,tests}.rs` | `d5b38049317fc66bfeb43ebc2180f4fd4b2d5aaf`, `771658ed1c22279bf0a0fb684a3985d54191c5ce`, `16b655e2bf7d14da32ed8682841a7c79d141f632` |
| `crates/store/src/{lib,fixtures,tests/http}.rs` | `f91eb680bb23688e3009ffec5b41f50f6b248fc0`, `8dc669def04f85ef43a2e56157d638680c4109fa`, `ad2aaf24eac3aa94bfe75813431866d7b2c7cd31` |
| `src/App.svelte`, `src/ProfileEditor.svelte`, `src/personal.ts`, `scripts/test-profile-renderer.py` | `57653b115fc836dd69996c37efd2a759309d58b8`, `2e7cadaa408404067cde0415362be6f1a962a173`, `b44bc53750a2360e9dce3df3abb77b652f23e70b`, `8f85ef4c0693046730082c79c3b857c479dc9059` |
| `src-tauri/src/main.rs`, `src-tauri/build.rs`, `src-tauri/capabilities/review.json` | `d4a73620a0e730be48a1b70c771cdc30e43c24d9`, `221f001b015842990410d0505be724e869619c94`, `185130c55ffc8fbfbaaa3c4401e761e22a997a33` |
