# Prepare vanilla game files

The version check resolves a saved personal profile's pinned Minecraft version against Mojang's public version listing. It verifies the version JSON and reports the required Java major, the declared client JAR size, the declared asset-index JSON size and the number of library entries. The game file plan resolves the downloads for the current platform. Library entries can have platform rules and the count does not predict the number of files a particular player needs. [Mojang version listing](https://piston-meta.mojang.com/mc/game/version_manifest_v2.json)

The native entry point is `carbon_vanilla::prepare(root, profile_id, offline)`. The shell supplies the app-owned root. The caller provides a profile UUID and an offline flag. Rust reads the version from the saved profile, validates metadata against it, and rechecks the selection before returning. When another operation changes that version during a fetch, the verified object remains reusable while the caller receives a request to prepare the new selection. [Personal profile contract](profiles.md)

## Retrieval and verification

1. Read the selected version's local pin and content object. Check the pin's ID, release type, expected HTTPS source and digest. Rehash the original JSON bytes and validate the internal ID/type and typed Java and size fields.
2. Return a verified cache hit in either mode. An offline miss or damaged object returns an instruction to prepare online.
3. On an online miss, fetch the public listing and require one matching entry with the expected release or snapshot type. The per-version URL must identify that exact version and SHA-1. Fetch only that JSON and check the original bytes against the listing's SHA-1 before publication.
4. Atomically publish the verified object and then its version pin. Reopening and retrying a missing or corrupted regular cache entry performs verification again. Links and special files stop the operation.

Both metadata requests require TLS validation. The only allowed HTTPS hosts are `piston-meta.mojang.com` and `launchermeta.mojang.com`. The listing path is fixed and the version path takes the form `/v1/packages/<sha1>/<version>.json`. A redirect can switch between those hosts while retaining the approved path. The helper permits up to two redirects per request, bounds a complete resource request to 15 seconds, limits an individual connect attempt to five seconds, and caps the listing and version JSON at 2 MiB and 1 MiB respectively. Version selection uses only the profile and the official listing. Subsequent game planning validates the separate asset and game descriptors below.

## Cache and result

The separate `vanilla-metadata-v1` directory contains one regular file per digest under `objects/` and the last accepted descriptor under `versions/`. An OS lock serializes cooperative writers. Each publication stages a file in the destination directory, syncs it, then replaces the destination atomically. Unix also syncs the parent directory. Interruption before pin publication can leave an unreferenced content object. The durable profile version selects the binding on every read. Multiple profiles pinned to the same version reuse the same object.

The HTTPS listing supplies the expected SHA-1 on first retrieval. Offline verification trusts the previously accepted pin in the app-owned cache. It detects damaged bytes and inconsistent identities. This cache assumes the launcher controls its directory. There is no independent signature or freshness claim for an offline pin. A healthy cached version is reused on a subsequent online prepare.

The result reports `profileId`, `version`, `releaseType`, `javaMajor`, `clientBytes`, `assetIndexBytes`, `libraryCount`, `metadataSha1` and `origin` (`network` or `cache`). This step downloads only the public version JSON documents. Account entitlement, a compatible runtime, installation and a qualified runner have separate acceptance gates.

## Import an existing vanilla client JAR

After preparing a profile, select its exact vanilla client JAR in the native file dialog. The Rust side opens the chosen regular file and passes a read-only handle to the importer. The importer compares its actual size and SHA-1 with the client descriptor in the reverified version metadata. It stages matching bytes under `vanilla-metadata-v1/clients/`, syncs and atomically publishes the file under its digest, then reads and verifies the published object. An existing healthy cache entry can satisfy multiple profiles. On repeat import, both the selected file and that cached object must verify. An invalid selection leaves the existing copy untouched.

The native signatures are `open_client_file(path)`, `import_client(root, profile_id, file)` and `client_status(root, profile_id)`. The file picker supplies the path. The webview receives a profile ID, version, expected byte count, expected SHA-1 and one of three status values.

| Status | Observation |
| --- | --- |
| `missing` | The cache has no object for the pinned client digest. |
| `verified` | A fresh bounded read verified the cached object's exact size and SHA-1. |
| `repairNeeded` | An ordinary cached file has a different size or digest. Select the exact client JAR to repair it. |

A missing metadata pin, unexpected cache file type or file-access failure returns an error. Expected size mismatch is rejected before copying. File reads are capped by the pinned size plus one byte, with a 256 MiB ceiling. This operation imports a single user-selected file. It never downloads the client, opens the JAR as executable code, changes the source file or accesses a world. The full preparation flow below can acquire the client directly from Mojang.

## Native bridge review

The local main-window capability grants named setup commands. The version check receives a saved profile UUID and the offline choice. The JAR import receives a UUID and opens a parented native file picker. Cancelling the picker returns an empty result. The chosen path stays in Rust, and the frontend receives status and descriptor data. The shell and store both check the selected profile version when asynchronous work crosses a picker or preparation step.

Review identified a case where copying a client or selecting Java succeeded but assembling the refreshed setup report could fail. The commands now identify the completed action and ask the player to recheck setup. A failed mutation clears the old report in the view. The Java installation progress event targets the main window and carries a profile ID and byte counts. The job ownership test verifies that cancellation applies only to the matching profile and that completing a job releases the slot.

The setup report continues to carry the account and protected-launch blockers. A selected Java folder's compatibility status describes its declared release metadata and expected binary's presence. It provides no observation of running that binary.

## Review and prepare the complete game file set

Open the saved profile's setup, review the game file plan and choose preparation. The plan fetches verified version and asset-index JSON as necessary. It shows the host platform, applicable library file count, asset names and unique objects, logging configuration count, and total unique files and bytes. Plan totals include the client JAR and asset index. Two asset names or library paths may reference the same content.

The native planning call is `game_file_plan(root, profile_id, offline)`. The shell passes its result's `metadataSha1` to `prepare_reviewed_instance(root, profile_id, offline, metadata_sha1, on_progress)`. The core rebuilds the plan from reverified local JSON and checks that digest before starting a payload operation. A profile change during acquisition prevents the old plan from being recorded for the new selection. The UI receives progress for the requesting profile and can cancel that job. The shell supplies the library root.

| Required content | Approved HTTPS source |
| --- | --- |
| Client JAR and declared client logging configuration | `piston-data.mojang.com`, exact object digest and filename |
| Libraries and declared native classifiers | `libraries.minecraft.net`, exact reviewed library path |
| Asset index | The metadata hosts above, exact digest and index ID |
| Asset objects | `resources.download.minecraft.net`, path derived from the index digest for each asset |

The planner applies Mojang's ordered OS and architecture rules and preserves classpath order. It resolves any required native classifier for the platform. Unsupported rule fields and legacy virtual asset layouts stop preparation. The index is limited to 8 MiB. A game plan is limited to 32,768 unique files, 256 MiB per file and 16 GiB total. Up to eight workers stream and check content concurrently. Payload requests require HTTPS and reject redirects. Each attempt has a six-second connect timeout and a 120-second request timeout. Workers check for cancellation between files and in 256 KiB increments while transferring or hashing a large object. A pending network read can delay cancellation until that read returns or times out.

For each digest the worker verifies the exact length and SHA-1 of existing bytes. An online retry fetches missing or damaged files, stages bytes in the destination directory, then syncs and atomically publishes each verified file. A failed transfer preserves the previous bytes. Interrupted staged downloads are replaced on the next acquisition of that object. The offline path reads every required object during a successful verification and opens no network client. A failure identifies the object to repair online. Progress and result byte counts record completed verified objects. Requests for version metadata and the asset index made while planning are outside the subsequent transfer counters.

Shared game objects live under `game-files-v1/objects/<prefix>/<sha1>`. The client uses the same digest store as the optional file import. After verification the core publishes a logical file map in `game-files-v1/generations/` and then a profile receipt in `game-files-v1/active/`. The file map records the client digest, library classpath order, native archive names and provider logical paths. An OS lock serializes prepare jobs. Other profiles can reuse the same bytes. Cancellation or failed verification leaves already verified objects available for retry. The overview's `has_game_install` checks for a matching receipt and file map. An explicit offline check establishes current byte integrity.

The cache operation covers file acquisition and verification. The private preparation below gives each profile its own logical files. The application packages contain launcher code. Acquired game files stay in the app-owned library.

## Repeat the checks

```sh
cargo test --locked -p carbon-vanilla
cargo test --locked -p carbon-vanilla live_mojang_metadata_and_offline_reopen -- --ignored --nocapture
cargo test --locked -p carbon-vanilla live_public_game_plans -- --ignored --nocapture
cargo clippy --locked -p carbon-vanilla --all-targets -- -D warnings
```

The deterministic tests use a disposable library and an injected metadata source. The import tests use inert locally generated bytes with a test pin. They cover same-length corruption, size mismatch, repeated verification, shared reuse, repair, preservation of existing bytes on a rejected import and refusal to replace a special cache entry. The opt-in live check creates three temporary profiles, prepares their public metadata through the native library, then reopens each offline and compares its digest. It uses no account or credentials. The separate live plan check retrieves the asset index for every offered version and reconstructs each plan offline. Game preparation fixtures exercise aliases, platform rules, native classifiers, stale reviewed pins, source and size rejection, cancellation, interrupted staging, corruption, selective repair and profile changes before activation.

## Native Windows live check

On 26 November 2026 the native Windows library fetched the official listing and immutable version JSON for each selection below. All three reported `network` on first use and `cache` on the offline reopen. The sizes are bytes declared in the verified metadata. The test did not fetch those payloads.

| Version | Type | Required Java | Client bytes | Asset-index bytes | Listed libraries | Verified metadata SHA-1 |
| --- | --- | ---: | ---: | ---: | ---: | --- |
| 1.20.4 | release | 17 | 24,445,539 | 438,086 | 88 | `829b544d7c453c4d86ee106f0ee2eef16ba90b74` |
| 26.3 | release | 25 | 41,483,720 | 597,235 | 114 | `8696584e8d2da5726487597e6c5777480387ec22` |
| 24w14a | snapshot | 21 | 26,340,437 | 446,764 | 96 | `a19842623bd07c282b13f4f651adc4136f7694cf` |

## Game files verified on 26 November 2026

Public metadata planning succeeded on native x86-64 Windows and on the x86-64 Linux build running under WSL2. Every row includes one client JAR, one asset index and one logging configuration. The 1.20.4 index contains 3,813 asset names referencing 3,790 distinct objects.

| Host | Version | Library files | Unique asset objects | Unique files | Total unique bytes |
| --- | --- | ---: | ---: | ---: | ---: |
| Windows | 26.3 | 74 | 5,149 | 5,226 | 617,783,881 |
| Windows | 1.21.1 | 70 | 3,890 | 3,963 | 922,881,420 |
| Windows | 1.20.4 | 64 | 3,790 | 3,857 | 746,755,015 |
| Windows | 1.20.1 | 64 | 3,577 | 3,644 | 738,787,077 |
| Windows | 24w14a | 69 | 3,864 | 3,936 | 754,691,566 |
| WSL2 Linux | 26.3 | 66 | 5,149 | 5,218 | 613,339,726 |
| WSL2 Linux | 1.21.1 | 56 | 3,890 | 3,949 | 918,086,753 |
| WSL2 Linux | 1.20.4 | 52 | 3,790 | 3,845 | 743,530,969 |
| WSL2 Linux | 1.20.1 | 52 | 3,577 | 3,632 | 735,979,239 |
| WSL2 Linux | 24w14a | 55 | 3,864 | 3,922 | 749,896,899 |

The native Windows 26.3 run started with an empty dedicated review library. After the plan fetched the index, preparation fetched 617,186,646 bytes from the official endpoints and reused the 597,235-byte index. Every required file passed exact-size and SHA-1 verification. A second pass verified all 617,783,881 bytes offline with zero fetched bytes. Repeating through the reviewed-pin bridge's core function also verified all 5,226 files offline. The remaining rows cover planning and metadata only. Gameplay, native Linux desktop use and live account entitlement have separate acceptance requirements.

To reproduce the full 26.3 preparation and offline repeat, run the following opt-in test on Windows. It retains content under the checkout's ignored `.artifacts/game-files-review-26-3/` directory. Allow at least 700 MiB of free disk space and network access for a cold run. Running it again reuses verified files.

```sh
cargo test --locked -p carbon-vanilla live_full_official_game_files_and_offline_repeat -- --ignored --nocapture
```

## Prepare a private profile game directory

The native `prepare_reviewed_instance(root, profile_id, offline, metadata_sha1, on_progress)` operation verifies or acquires the reviewed cache and then prepares the profile's private version of that content. A plan review still resolves public metadata and the asset index before a player approves preparation. The offline option checks both the cached files and the private content. The result keeps the existing transfer counts and adds `instanceView` with the profile UUID, version, host platform, metadata SHA-1, private logical file count and copied, reused and total bytes. Plot progress with the counters for the current phase. The view phases are `verifyingView`, `copyingView` and `activatingView`, and their totals are separate from the provider cache totals.

`has_instance_view(root, profile_id)` checks for a matching cache receipt, a private content receipt and the small private version JSON. Use explicit verification for full file integrity. A profile that already had a shared cache receipt can reuse those bytes to make its private directory. The cache APIs `has_game_install` and `prepare_reviewed_game_files` still describe the provider cache.

The trusted native API `prepare_instance_view(root, profile_id, expected_metadata_sha1, on_progress)` checks the prepared cache plan offline and returns the paths plus the summary. Keep the returned value alive while using those paths. Its OS file lease prevents another cooperative operation from replacing that profile's content during use. The shell supplies the app-owned library root. The webview may receive the statistics and identifiers from the summary.

| Private path under `instances-v1/<profile UUID>/` | Contents |
| --- | --- |
| `game/` | Persistent working directory for that profile's saves, settings, logs and other player files. |
| `content/versions/<version>/` | The verified client JAR and original pinned version JSON. |
| `content/libraries/` | Host-selected archives in the logical Maven layout. The native result returns the metadata's library order followed by the client JAR. |
| `content/assets/` | Object digests by prefix, the pinned `indexes/<id>.json` and the reviewed client configuration in `log_configs/`. |
| `content/natives/` | Empty managed native directory for these plans. A later protected runner must provide its own permitted writable native scratch for libraries that extract themselves. |
| `content/.carbon-view.json` | Identity of the profile, pin and platform bound to the private tree. |

Every ordinary private file is copied and verified against its source descriptor on first publication. The private copy has no links to the cache. A reuse verifies every expected file and both local records, rejects linked entries, and checks the directory inventory. To repair ordinary private corruption, copy a fresh tree from the cache. If the selected cached object is missing or damaged, an online game preparation repairs the cache before retrying private activation. A valid private tree can be independently reverified from its stored bytes by the native offline view API.

A same-directory stage contains each verified new tree. The operation checks cancellation during copy and hashing and checks the selected profile and pin immediately before committing. The final tree appears by rename after all files verify. An interrupted replacement can leave one previous tree and one partial tree. The next operation on that profile restores the last complete tree if necessary, clears the partial stage and verifies the recovered result. The directory used for player files is a sibling of the replaceable tree. Staging and repair leave its contents untouched. Keep player files in `game/`; `content/` is replaceable app-owned storage. Private content is bounded to 32,768 files and 16 GiB per profile, including logical aliases and the two small records. Allow additional space for a complete replacement beside an existing tree.

Current private preparation targets Windows and GNU/Linux x86-64. Supported modern LWJGL native JARs belong to the ordinary ordered library classpath. If metadata requires the older extraction-classifier flow, the service reports that the layout is unsupported before beginning combined preparation. It extracts no downloaded archives and neither builds a Java command nor consumes credentials. A protected runner still has to grant the sandbox specific access to the returned game directory and verified files, supply private writable native scratch, and independently resolve launch and account requirements. [LWJGL packaged native libraries](https://github.com/LWJGL/lwjgl3/blob/master/README.md)

### Verify the private view

```sh
cargo test --locked -p carbon-vanilla
cargo test --locked -p carbon-vanilla live_private_26_3_view_from_public_cache -- --ignored --nocapture
```

The second command uses the dedicated public 26.3 cache from the preceding test and opens no network client. It copies the full version files per platform, then reopens the directory, verifies the private bytes and reads an isolated state sentinel. Tests with inert input also exercise a stale pin, missing receipt, profile changes, interrupted staging, cancellation, same-length corruption, failed cache repair, legacy classifiers, and preservation of sample saves and settings. The Unix fixture refuses a symbolic link to an outside file.

All five offered version plans passed private layout validation against public metadata on native Windows x86-64 and WSL2 Linux x86-64. Version 26.3 was also fully materialized and verified from each platform's existing public cache. In both runs the classpath included the host's native JARs, the cache was read offline and a second pass reused all private files while preserving the state sentinel.

| Host | Private files | Private bytes | Copy from cache | Reopen and verify |
| --- | ---: | ---: | ---: | ---: |
| Native Windows | 5,228 | 617,829,523 | 20.8 s | 8.9 s |
| WSL2 Linux | 5,220 | 613,385,366 | 102.7 s | 62.1 s |

These are single observed runs. The unprivileged WSL process read and wrote the review library on the mounted Windows drive. Native Linux desktop and game execution still require separate verification.
