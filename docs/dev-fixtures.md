# Development preview fixture libraries

`carbon-desktop-gpui` can seed a disposable, offline library for native UI testing. The command
uses the ordinary profile, launch-target, icon, Home-order and fixture-store operations, so a newly
modeled field keeps its existing validation. The declarations live in
[`crates/gpui-launcher/src/dev_fixtures/catalog.rs`](../crates/gpui-launcher/src/dev_fixtures/catalog.rs);
add a row there and the matching table row below when a new UI state needs coverage.

## Run

```powershell
# Varied default library on the ordinary Home surface.
.\target\debug\carbon-desktop-gpui.exe --seed-fixtures default

# Reopen the same root without reseeding.
.\target\debug\carbon-desktop-gpui.exe --fixture-root "$env:LOCALAPPDATA\CarbonDesktopGpui\dev-fixtures\default"

# Empty library.
.\target\debug\carbon-desktop-gpui.exe --seed-fixtures empty

# 100-profile comparison library, opened in Library.
.\target\debug\carbon-desktop-gpui.exe --seed-fixtures large

# Review tools with a missing shared fixture object, opened in Library.
.\target\debug\carbon-desktop-gpui.exe --seed-fixtures faults

# Explicit disposable root, useful for scripts and separate evidence.
.\target\debug\carbon-desktop-gpui.exe --seed-fixtures default `
  --fixture-root D:\carbon-dev-fixtures\default `
  --ready-file D:\carbon-dev-fixtures\ready.txt

# Seed without opening a window (for scripts and CI).
.\target\debug\carbon-desktop-gpui.exe --seed-fixtures default --seed-only

# Replace only a root that carries the development-fixture marker.
.\target\debug\carbon-desktop-gpui.exe --seed-fixtures default --reset-fixtures
```

The default roots are `%LOCALAPPDATA%\CarbonDesktopGpui\dev-fixtures\<set>` on Windows and
`$XDG_DATA_HOME/carbon-desktop-gpui/dev-fixtures/<set>` (or `~/.local/share/...`) on Linux. They are
siblings of the ordinary `library-v1` root, never that root. `--fixture-root` must be absolute.
Ordinary fixture previews start in Worlds. The legacy `--comparison-review`,
`--temporary-review` and `CARBON_README_SCREENSHOT=1` capture paths explicitly select
Instances so their established instance-first automation remains deterministic.
The command is offline: it creates profiles, target preferences, inert save-folder markers, Home
ordering and local Review tools only. It refuses a normal per-user data root, any ancestor or
descendant of one (including a path spelled through a symlink or junction), a relative path, a path
containing `..`, and a non-empty directory without its marker. `--fixture-root` only reopens a root
with a completed marker; an unseeded or incomplete root is reported with the seed or reset command
to run. Repeating the same set is a no-op. A different set or a partial seed is refused unless
`--reset-fixtures` is explicit, and a reset refuses a root that is itself a
link or reparse point while links above the root are resolved before the
ownership comparison. The large set reuses the established 100-profile comparison sample and its
stable IDs.

## Set map

### `default` profile rows

| Key | Instance | UI state exercised |
| --- | --- | --- |
| `vanilla-survival` | Vanilla survival, 26.3, automatic icon | Ordinary modern vanilla row, selected first, Home hero with no target |
| `alex-a` | Alex, 1.21.1, Red fox | Duplicate-like short name, custom built-in icon, local-world target selected |
| `alex-b` | Alex, 1.20.4, Pond frog | Second identical display name, legacy version, small 1024 MiB / 854 × 480 settings |
| `unicode-snapshot` | 苔むす森・夜間採集, 24w14a, Mooncap | Unicode name, snapshot label, recent ordering |
| `long-name` | 80-character name, 1.20.1, automatic icon | Long-name truncation in the rail, hero and Home tile |
| `pack-qualified` | Pine Hills pack, 1.21.1, Pine | Local pack with no loader or pending file; Modpack label and no pack blocker |
| `pack-pending` | Fabric dev pack, 1.21.1, Field pack | Local pack with a loader and three pending files; Pack requirements blocker |
| `bound-server` | Bound server · survival, 26.3, Beacon | Bound server address; Multiplayer panel; Home readiness “Saved server” |
| `bound-ipv6` | Local IPv6 server, 1.21.1, automatic beacon | Bracketed IPv6 address normalization and display |
| `targets-empty` | Targets · none saved, 26.3, Compass | Empty launch-target inventory |
| `targets-mixed` | Targets · mixed, 1.20.1, Amethyst | Two local worlds and two saved destinations; server selected; pinned |
| `targets-stale-label` | Targets · stale label, 1.20.4, Alpine peak | One world plus a labeled world whose folder was removed; missing-label recovery |

The default set pins `targets-mixed` and `bound-server`, then orders `unicode-snapshot` and `alex-a`
as recent instances. Home therefore shows pinned rows first, then recents, then the remaining
library.

### `default` Review tools rows

| Fixture | Seeded state | UI state exercised |
| --- | --- | --- |
| Forest sample | Interrupted after one verified generation-B object | Resume/Cancel, partial staging, active generation A |
| Forest repeat | Healthy independent copy | Installed state and a second instance |
| Coast sample | Generation B verified and staged | Activate B, rollback remains available |

The `faults` set instead removes one shared fixture object so the Review tools primary action is
Repair. Removal is shared by design: the fixture store reports each instance that depends on the
missing object, and repairing restores every dependent instance.

### Other sets

| Set | Contents | UI state exercised |
| --- | --- | --- |
| `empty` | Empty profile store and empty fixture store | First-use Home, empty Library, empty Review tools |
| `large` | The 100-profile comparison sample and Forest sample | Rail scrolling, Ctrl+F filter, `099` selection, Library entry |
| `faults` | Empty profile store and a Forest sample with one missing shared object | Repair action and unhealthy fixture status |

## Truthfulness boundaries

The fixtures never fabricate a state the app cannot truthfully read locally:

- Every instance is **Setup required** until real metadata, game files and a managed or selected
  Java runtime are verified. There is no seeded “ready” or installed instance.
- A synthetic save folder contains only a small `level.dat` marker. It exercises the passive target
  inventory and stale-label recovery; it is not a playable world, and Carbon claims no world entry.
  World-map previews stay `NoCapture`, so “no map coverage” remains the honest state.
- Local packs describe only launcher-owned import metadata (`loader`, `pending_files`). No archive,
  descriptor, override or remote dependency is downloaded or claimed.
- Bound servers and saved destinations are normalized addresses only. No sign-in, entitlement,
  remote listing, join or remote success is simulated.
- The icon rows use built-in catalog IDs; custom icon files are not modeled by the current profile
  schema.
- Interrupted pack-import journals, verified install receipts, account eligibility and allocation
  metrics are not seeded because they require evidence from their owning services.
- The three Review tools rows are the fixture store's existing states; the seeder does not add a new
  fixture-store schema.

## Verification

The fixture seeder is covered by unit tests in
[`crates/gpui-launcher/src/dev_fixtures/mod.rs`](../crates/gpui-launcher/src/dev_fixtures/mod.rs):
the ordinary stores validate every row, reseeding is a no-op, reset requires the marker, the empty
and large sets are deterministic, and ordinary per-user roots and their ancestors are refused.
Native checks for this change are recorded in the pull request and checkpoint. Run:

```sh
python design-system/verify_seed.py
cargo fmt --all -- --check
cargo test --locked -p carbon-desktop-gpui
cargo clippy --locked --workspace --all-targets -- -D warnings
```

### Native verification, 27 November 2026

The Windows x64 debug executable built from this change seeded and opened all four roots on the
development host. Native UI Automation and `PrintWindow` captures covered: Home with two pinned
instances followed by two recent instances and 12 total; Library with the long, duplicate-like,
Unicode, pack, server and target rows; the mixed target detail with two worlds and two saved
destinations; the Launch targets dialog with a selected server; Review tools with interrupted,
installed and awaiting-activation rows; the bound-server Multiplayer panel; the missing-label
recovery dialog; the `faults` Repair state; the empty first-use Home; and the 100-row Library
before and after filtering to `099`. `--seed-only`, a repeat seed, a different-set refusal, reopen
without reseeding and refusal of `library-v1`, its parent and a nested path were also exercised.
The final pass additionally checked an unseeded `--fixture-root` refusal, an incomplete-seed
refusal followed by `--reset-fixtures`, an unsupported marker version reset, a junction to a fixture
root (reset refused) and a junction to the ordinary library parent (seed refused).
Screenshots and UIA trees stay outside Git in the worker's scratch evidence directory. Every
instance correctly remained **Setup required**; no ready or installed state was claimed.
