# Library modes and Carbon-managed worlds

Status: implementation foundation. Reviewed 29 November 2026. Owner lane:
CDESK-101, with slices CDESK-102 to CDESK-107.

## Decision

Select a **hybrid projection** for the first release slice:

1. The existing instance profile remains the owner of its private game directory,
   settings, entitlement context and saves.
2. A world is projected from exactly one backing instance and one save directory.
   Its stable ID is `(instance UUID, save-directory name)`. Copying a world
   creates a new save directory and therefore a new world ID; the launcher does
   not parse or trust `level.dat` for identity.
3. Worlds mode is a persisted presentation mode. It does not move files, rewrite
   profiles, change IDs, import saves or create a second ownership record.
4. Managed-world records are a future overlay that may reference a projected
   world only after an explicit, transactional adoption operation. They are not
   created by switching modes or by reading a save directory.

This preserves every existing profile, imported and server-bound instance,
default, setting and save. It also gives the world-first UI real data without
pretending that world launch or mutation is available.

## Sources and current evidence

### Mojang Quick Play

Mojang documents Quick Play as a direct destination path. The Java launch
contract uses `--quickPlaySingleplayer <save-folder>`, with related multiplayer
and Realms arguments. A direct singleplayer launch therefore needs the exact
save folder, a validated game installation and a successful game entry; a
launcher preference alone is not enough.

The current Carbon launch planner deliberately disables
`has_quick_plays_support`, `is_quick_play_singleplayer`,
`is_quick_play_multiplayer` and `is_quick_play_realms` and does not accept
`${quickPlaySingleplayer}` or `${quickPlayMultiplayer}` substitutions. Its
tests assert that the multiplayer Quick Play arguments are absent. Direct world
launch is not implemented in this checkout. The UI must say so.

References:

- Mojang, [Quick Play coming to Java and Bedrock](https://www.minecraft.net/en-us/article/quick-play-coming-java-and-bedrock-edition)
- [Launch argument rules](../../crates/vanilla/src/game/launch/arguments.rs)
- [Quick Play absence test](../../crates/vanilla/src/game/launch/tests.rs)

### Prism Launcher

Prism's primary workflow is instance-first. Add Instance selects a Minecraft
version and loader or imports a pack. An instance's Worlds page manages that
instance's saves. Instance Copy exposes independent choices for saves, game
options, resource packs, screenshots, shader packs, servers and mods; its
advanced copy mode can link files but includes **Don't link saves** so mutable
save data is copied instead.

This confirms that saves are valuable, independently mutable user data and that
copying must be explicit. It does not provide a world-first ownership model that
Carbon can adopt without migration. Carbon can project worlds across instances
while keeping Prism-style instance ownership and copy choices.

References:

- [Create an instance](https://prismlauncher.org/wiki/getting-started/create-instance/)
- [Worlds](https://prismlauncher.org/wiki/help-pages/worlds/)
- [Instance Copy](https://prismlauncher.org/wiki/help-pages/instance-copy/)

### Carbon code and existing tickets

The current native library is instance-first:

- `Profile` owns an app-generated UUID, display name, pinned Minecraft version,
  memory/window preferences and optional imported-pack/server binding.
- Saves are directories under
  `instances-v1/<profile UUID>/game/saves/<directory>` with an ordinary
  `level.dat`.
- `instance_targets` scans only direct, ordinary save directories, refuses links
  and reparse points, applies launcher-only labels and never opens or parses
  `level.dat`. It refuses a dedicated server-bound profile because that profile
  uses its saved server address.
- The profile index is locked and atomically published. A mode preference can
  use the same app-owned root without touching profile records.
- Protected launch, confirmed world entry and safe world copy/delete are not
  available. The sandbox sample is not a real world launch.

Related tickets: CDESK-83 (mixed worlds/destinations), CDESK-84 (last entered
world), CDESK-93 (compact Play/Join targets), CDESK-96 (rail vs Resume layout),
CDESK-31 (instance lifecycle), CDESK-32 (selective clone), CDESK-24 (play/reopen
world), CDESK-46 (browse/transfer), CDESK-47 (backup/restore), CDESK-97 (Home
and Library navigation) and CDESK-108 (navigation motion). The folder-planning
worker owns CDESK-110; this lane links to it, coordinates stable IDs and does
not edit it.

## Model comparison

| Model | Ownership and IDs | Mode switch | Strengths | Costs and risks | Result |
| --- | --- | --- | --- | --- | --- |
| A. Pure instance projection | Profile UUID plus save-directory name. No new records. | Only sidebar, Home recommendation and detail presentation change. | No migration. Existing imported, server-bound and user settings remain authoritative. Copy/rename naturally produces a new key. | A world cannot be managed independently of its instance. Moving a world to another instance changes its ID. Delete and copy remain gated on qualified file operations. | Selected foundation. |
| B. World-owned records create backing instances | New world UUID owns a generated or adopted instance. | Requires migration or duplicate instances; switching back may hide the original profile. | Strong world-first lifecycle and per-world configuration. | High risk of duplicate ownership, saves and entitlement; imported/bound instances need special cases; a failed migration can strand data. Violates the no-forced-migration constraint unless only new worlds use it. | Rejected for the first slice. |
| C. Hybrid projection plus managed-world overlay | Projection is read-only and stable; an explicit future record references it before adoption. | Switch remains presentation-only until adoption. | Keeps existing data safe and leaves a path to Carbon-owned lifecycle. | Two concepts must be documented and kept consistent. Adoption needs compare-and-swap, backup and restart tests. | Selected direction, with no overlay written yet. |

## Selected semantics

### Stable IDs and names

- `WorldId = { instance: UUID, directory: String }`.
- `directory` is the exact direct child of `game/saves`, not a derived display
  name and not a filesystem path.
- Two worlds with the same display name remain distinct. The UI shows the
  backing instance and directory when names collide.
- A custom label changes only the display name. Renaming or copying the
  directory changes the ID. The launcher does not read `level.dat`, so a copied
  save's internal world identity cannot collide with launcher IDs.
- Profiles and worlds keep their IDs when modes change.

### Backing instances and sources

Every world carries its backing profile ID and source:

- **Local**: an ordinary profile created by Carbon.
- **Imported**: a profile with an imported pack. Its saves are local to that
  profile; pack-defined files and loader requirements stay visible.
- **Server-bound**: a profile with a saved server address. Its local saves are
  projected for inspection only; no general target chooser or direct join is
  implied. The existing server-bound behavior remains authoritative.
- **Managed**: reserved for a future explicit managed-world overlay. No current
  profile is silently relabeled.

Version and loader text comes from the backing profile. Compatibility is not
inferred from a folder name. A future copy/create action must require the
destination instance's version and loader to be compatible, or create a
separate explicitly compatible instance.

### Missing, unverified and failed data

- A complete scan with no `level.dat` does not invent a world.
- A partial or unreadable scan reports an issue and keeps any found worlds; it
  does not prune labels or claim that missing entries were deleted.
- A missing prefererence target can be omitted, but its label file is not
  rewritten merely by reading.
- A fresh profile has no worlds until its game folder is actually populated.
- Unverified world entry is not a successful visit. Selection, opening details
  and launching Java do not count.
- Failed copy/create/delete must leave the original and destination in a known
  state. The first slice does not expose these operations.
- Protected restart/concurrency: profile and preference writers use the existing
  app-owned lock and atomic publish. A second process must not observe a
  half-written mode file. A world scan never holds the lock while presenting
  files to the UI.

### Home and mode behavior

- Worlds is the default mode. The Settings switch is always reachable and is a
  preference, not a migration.
- Worlds mode lists projected worlds, recommends a real named world on Home and
  can open the backing instance or world detail. It must not show a fake
  Create/Play/Copy/Delete action.
- Instances mode preserves the current list, create/edit/clone, bound-instance
  and target behavior and can still select and inspect worlds within the
  selected instance.
- Switching modes preserves the selected profile/world identity and all
  configuration. If a selected world is unavailable, the UI shows the empty or
  issue state rather than guessing a replacement.

## Staged acceptance

1. **Mode and persistence (CDESK-102)**: missing file means Worlds; explicit
   Instances round-trips; malformed/unknown data is preserved and reported
   until an explicit Settings choice repairs it; a mode write does not touch
   profile, target or save files; tests cover two writers and restart-equivalent
   reads.
2. **Inventory (CDESK-103)**: projection returns exact backing IDs, duplicate
   names, imported/server-bound sources, partial scans and missing folders;
   tests use synthetic save directories only.
3. **Grounded UI (CDESK-105)**: Settings switch, Worlds sidebar, Home
   recommendation and truthful unavailable/empty states. No account or game
   process. Keyboard focus and accessibility labels are checked.
4. **Managed operations (CDESK-106, blocked)**: create/copy/delete/direct launch
   only after protected launch, world-entry observation, backup and safe
   filesystem transactions are proven. Any public default activation is gated
   here, not by this preview.
5. **Release verification (CDESK-107)**: native Windows and Linux inspection,
   retained profile/world IDs across switches, imported/bound preservation,
   restart and concurrency, draft PR and independent review.

## Native verification

Windows x64 debug build, 29 November 2026, disposable `default` fixture root
outside the ordinary library. The run used UI Automation only; it did not sign
in, launch Java, open the game, or touch a real player library.

- Home opened in Worlds mode and exposed a real recommendation, **Open world**,
  the backing instance action and the explicit direct-launch-unavailable status.
- **Open world** reached Library with `Find a world`, four real fixture worlds
  (`Cove`, `Harbour`, `Highlands`, `Ridge`), distinct backing-instance/source
  subtitles and a selected-world panel.
- **App settings** opened an owned window with Worlds and Instances radio
  choices. Switching to Instances changed the Library to `Find an instance` and
  the 12-profile list; switching back restored `Find a world`.
- The persisted preference ended as `{"version":1,"mode":"worlds"}`. The
  existing comparison UIA walkthrough still passed after explicitly selecting
  Instances for its disposable root.

Remaining native checks: Linux/AT-SPI, light and dark palettes, narrow layout,
screen reader and keyboard-only traversal, imported and server-bound fixture
sources, a partial-scan fixture, and a real restart during a concurrent write.
Gameplay, world creation/copy/delete, direct entry and map evidence remain
outside this slice. Evidence is kept outside Git.

## Implementation boundary in this lane

This checkout implements the mode preference, the read-only world projection and
a reachable Settings/Worlds preview. It does not implement world creation,
copying, deletion, direct launch or a managed-world overlay. The Home change is
limited to the launch card so the recents naming/row styling and navigation
motion owned by other workers are not edited.
