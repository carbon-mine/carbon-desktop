# Personal vanilla profiles

Create a named profile, select a pinned Minecraft version, set the maximum Java memory and the intended game window size, and reopen the library to edit those preferences. Cloning appends an independent copy with a new name and ID. Edits preserve the selected profile's ID and position. Two names may match. These operations author launch preferences. Installation, game execution, the live account and real worlds have separate acceptance gates.

## Profile contract

The native core exposes `carbon_profiles::list(root)` and `carbon_profiles::change(root, action)`. Each returns the complete ordered list after success. The shell chooses the app-owned library root. On a disposable run it supplies its temporary library root. The fixed choices are 26.3, 1.21.1, 1.20.4, 1.20.1 and snapshot 24w14a. These values select a version for a future installation. The selected version and preferences are retained across restarts.

| Field | Accepted input |
| --- | --- |
| `name` | 1–80 Unicode scalar values after trimming whitespace at the ends. Control characters, explicit bidirectional marks/overrides/isolates and Unicode line separators are refused. Names are display text. |
| `id` | An app-generated version 4 UUID. Edit and clone identify the existing profile by ID. Create omits it. |
| `version` | One of the five exact version strings above. |
| `memoryMiB` | Integer from 512 to 16,384 inclusive. |
| `windowWidth`, `windowHeight` | Integer width from 320 to 7,680 and height from 240 to 4,320 inclusive. |

```json
{"kind":"create","name":"My survival world","version":"26.3","memoryMiB":2048,"windowWidth":1280,"windowHeight":720}
```

An edit uses the same fields and the returned ID, with `"kind":"edit"`. A clone takes only `kind`, `id` and the new `name`. The result includes the generated ID and settings, without the action tag. The native API rejects unknown fields. It accepts no filesystem paths, JVM arguments or command strings.

## Library mode and world projection

The native library has two presentation modes. **Worlds** is the default;
**Instances** preserves the instance-first list. The mode is an app-owned
preference in `personal/library-mode.json` and is read with
`carbon_profiles::library_mode(root)` and written with
`carbon_profiles::set_library_mode(root, mode)`. A missing file means Worlds.
Malformed or unsupported data is reported and preserved on reads rather than
silently replaced; an explicit Settings mode choice is the repair action.
Switching modes must not change a profile, target preference, world label, save,
ownership record or entitlement decision.

`carbon_profiles::world_library(root)` projects worlds from the existing save
directories of every saved profile. A projected world has:

- a stable `WorldId { instance, directory }`, where `instance` is the profile
  UUID and `directory` is the exact direct child of `game/saves`;
- a display name from the existing launcher label or the directory name;
- its backing `Local`, `Imported`, `ServerBound` or reserved `Managed` source;
- the backing profile's Minecraft version and optional loader;
- the backing instance's passive selected-target flag;
- an optional scan issue and an explicit `WorldLaunch::Unavailable` state.

The scan reuses the instance-target inventory: direct ordinary directories with
an ordinary `level.dat`, no links or Windows reparse points, bounded entry counts
and no `level.dat` parsing. Duplicate display names stay distinct. A copied save
folder gets a new `WorldId`; a label change does not. Server-bound profiles are
projected for inspection only and retain their existing restricted target and
join behavior. Imported profiles are labeled as imported; their saves remain
inside the imported profile.

The projection is read-only and does not create, copy, delete or launch a world.
Those operations remain blocked until protected launch, confirmed world entry
and a transactional world-operation service are independently qualified. No
managed-world overlay is written by this slice.

## Persistence and recovery

The personal index lives under `personal/profiles.json` inside the library root. Names and IDs stay inside this versioned JSON document. A separate lock file serializes cooperating app processes, including first use. The index is limited to 4 MiB. The writer stages and syncs the replacement in the same directory, then renames it into place. Unix also syncs the directory. An interrupted scratch file is ignored by readers and replaced on the next write. A persistent initialization marker distinguishes a first launch from a missing index.

Invalid, missing or unsupported metadata stops writes. Keep the original personal directory and restore a known backup of the complete directory with the app closed. The crate does not automatically back up data or delete profiles. It never creates a game directory, copies worlds or modifies the fixture store. Future materialization belongs to the shared installation core. The trust boundary is an application-owned root with cooperative writers. The profile library checks its directories and files for links and Windows reparse points.

If an action's result is lost, reload the list before retrying. Every accepted create or clone adds a distinct profile.

The Setup screen stores a chosen Java home or managed-runtime preference in a separate `personal/java.json` index keyed by profile UUID. The same writer lock protects both indexes. Existing profile files open without migration. A clone copies its Java choice before its new profile ID becomes visible. Changing the clone's choice leaves the original independent. A write based on an older game version is rejected. The folder picker and Rust core own local paths. The window receives the source label and checked version. See [vanilla preparation](vanilla.md).

## Verification

Run `cargo test --locked -p carbon-profiles` at the repository root. The suite checks the exact JSON contract, Unicode names and invalid bounds, creation order and stable IDs, clone independence, reopen, corrupted or missing metadata, a partial scratch file, a blocked write and four concurrent first-use processes. Unix adds a linked-directory refusal check. Run `cargo clippy --locked -p carbon-profiles --all-targets -- -D warnings` for Rust lint.

The tests use disposable libraries. Native Windows NTFS and WSL2 Linux temporary storage exercise the respective filesystem implementations. Neither environment supplies evidence of booting a selected Minecraft version, and these process-level checks do not simulate a host power loss.
