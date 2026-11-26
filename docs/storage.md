# Local fixture storage

Carbon Desktop's first storage slice lets a reviewer install independent samples, change a sample save or setting, stage an update, interrupt it and repair a damaged object. Every input belongs to the application. Each object is inert, deterministically generated data. The sample save is a JSON revision counter. No provider, real mod, Minecraft world, registry, account or game process participates.

## On-disk contract

The shell supplies a fixed `fixture-library-v1` directory inside the app-local data directory. The webview supplies an action and one of three instance IDs. It has no path, file contents, URL, executable or command argument input. The Rust entry points are `carbon_store::snapshot` and `carbon_store::apply`. The Tauri commands are `fixture_library` and `fixture_action`.

| Location inside the fixture root | Owner and contents |
| --- | --- |
| `library.json` | Rust-owned, versioned index. Selects each instance's complete generation, update phase and progress. Records fixture-source and verified-reuse counters. |
| `objects/<sha256>` | One file per digest with fixed expected size and SHA-256. Active, staged and rollback selections reference these bytes. The fixture has six possible objects and no eviction operation. |
| `players/<id>/settings.json` and `sample-save.json` | Independent player files initialized for a new sample. Install, update and repair preserve existing values. An edit increments one selected instance's setting or save revision. |
| `backups/<id>-before-b.json` | Retains the player's setting and sample save from the most recent activation attempt after B passes verification. The store refreshes the backup atomically on each activation, including one after a compatible rollback. A compatible rollback switches immutable content and keeps the current player files. Editing a save under B raises its minimum compatible generation and blocks a return to A. |
| `library.lock` and `players/<id>/content.lock` | OS file locks serialize app operations and defer activation while a content reader holds a lease. Process exit releases the locks. |
| `.fixture-write.tmp` beside a published file | Bounded same-directory scratch space, reused under the library lock. A later write in that directory discards a leftover regular scratch file. |

A private application root and cooperative app processes form the trust boundary. Every object and instance path component comes from this crate. The store rejects existing symlinks, Windows reparse points and special files at the checked paths. New Unix directories request mode 0700. An external writer able to replace a directory between path inspection and opening is outside this fixture's model. Production gameplay needs an OS-enforced read-only content grant and a protected cache and backup root before any untrusted process runs.

## Fixture v1

The generator in [fixtures.rs](../crates/store/src/fixtures.rs) uses six fixed seeds, lengths and expected SHA-256 digests. A contains Rain A (4 KiB), Terrain (48 KiB), Lighting (12 KiB) and Forest (16 KiB). An exact repeat references the same four digests. The overlapping selection replaces Forest with Coast (16 KiB). B replaces Rain A with Rain B (4 KiB). Source generation occurs on a miss or damaged object, and the store verifies its output before publication. A cache hit checks length and digest before reuse.

A profile selects objects by reference. No per-instance writable hardlinks, mounted runtime view or game-visible installation exists in this slice. All three identities have separate player roots. An intentional damage action changes the selected generation's shared weather object. Other instances that reference it also report a repair need. Repair checks the active selection, a fully staged update and the available rollback selection. It replaces only missing or corrupt immutable data.

## Transaction and recovery

1. **Review** lists the added, removed and retained items. **Stage** journals the interrupted state before writing content, then publishes verified objects one at a time and records the completed position. An explicit **Interrupt** in review stops after the first verified object. An interrupt of a fully staged update pauses before activation.
2. **Resume** rechecks the journaled prefix and verifies the remaining objects. **Cancel** discards the pending selection. Up to six verified objects remain available in the cache across all samples.
3. **Activate** checks for a live content reader, re-verifies A for rollback and every target object, refreshes the pre-B player backup and publishes the library index with one same-volume rename. **Rollback** verifies the retained A selection and the current save's compatibility before changing that same index.
4. Files are written to a new sibling scratch file, flushed with `sync_all`, and renamed into place. The Unix implementation also syncs the parent directory. Process exits before and after publication are covered below. Sudden power loss, remote/network filesystems and interference from unrelated same-user processes require separate qualification.
5. Before mutating an instance, the store reads the full library view so damage to a sibling's player file is reported before writing. An unexpected read failure after a completed action reports that the change has already taken effect and asks the player to reopen the library before retrying. An unreadable index or malformed player file produces an error. The store refuses to initialize a missing index beside existing profiles, cache or backups. Repair operates only on immutable objects. Content-reader leases are exercised by tests. A future game supervisor must hold the lease for the full process tree's lifetime.

## Measurements and verification

Run from the Desktop checkout. Transaction tests create isolated, disposable fixture roots under `target/fixture-tests`. The HTTP workload uses a fresh directory on the default temporary volume.

```text
cargo test --locked -p carbon-store
cargo test --locked -p carbon-store fixture_workload_measures_local_reads_and_unique_storage -- --nocapture
cargo clippy --locked -p carbon-store --all-targets -- -D warnings
```

Windows 11 Pro x64 build 26300, Rust 1.99.1, Core i9-12900HK and NTFS with 4096-byte allocation units produced the following measurements. A WSL2 run on kernel 6.6.87.2 with Rust 1.96.0, using the same Windows-mounted volume, measured the same values. Numbers below are KiB (1024 bytes).

| Operation on fresh fixture v1 | Source bytes added (KiB) | Verified bytes reused (KiB) | Total logical bytes in unique objects (KiB) | Allocated bytes in unique objects (KiB) | Active logical bytes across profiles (KiB) |
| --- | ---: | ---: | ---: | ---: | ---: |
| Create Forest sample | 80 | 0 | 80 | 80 | 80 |
| Create Forest repeat | 0 | 80 | 80 | 80 | 160 |
| Create Coast sample | 16 | 64 | 96 | 96 | 240 |
| Stage B for Forest sample | 4 | 76 | 100 | 100 | 240 |
| Activate B | 0 | 0 | 100 | 100 | 240 |

The source/reuse counters count bytes attributed to committed fixture operations and staging checkpoints. No network payload is transferred in this path. A process exit after object publication but before progress publication can leave a verified cache hit on retry without a persisted source-byte count for that original write. Disk allocation uses Windows `FILE_STANDARD_INFO.AllocationSize` or Unix `st_blocks × 512`; an unavailable measurement is null. The allocation column counts the immutable objects only. Indexes, player data, backups, scratch files and filesystem overhead are outside it. The largest sampled cache allocation across the staged workload was 100 KiB. Total transient volume usage is unmeasured.

The [store tests](../crates/store/src/tests.rs) cover exact repeat and overlap, review/cancel/resume, independent writes, compatibility refusal, backup preservation, shared digest corruption, repair, reader deferral, parallel commands, malformed paths and a simulated full disk. A real child test process exits during stage and immediately before or after activation publication. Reopening observes a complete old or new generation with its sample save intact. The default Windows suite passed 14 tests including the subprocess helper. The WSL2 suite passed 15, including a symlink refusal test. The large HTTP study runs separately.

To repeat the WSL2 check from a Windows checkout with a provisioned Linux Rust toolchain, enter WSL at this repository and run `CARGO_TARGET_DIR=target/wsl-store cargo test --locked -p carbon-store -- --nocapture`. WSL2 on a Windows mount supplies Linux target and filesystem-adapter evidence. Native Linux default paths, gameplay-speed materialization or private-copy fallback, real provider fetches, production-scale file counts and library sizes, restoring real-world backups, signed updates and host power-loss recovery still require their own runs.

## Loopback HTTP transfer and 1 GiB workload

The [HTTP acceptance fixture](../crates/store/src/tests/http.rs) supplies bytes to the same Rust create, stage, activate and repair-capable store through a private supplier seam. The benchmark starts an HTTP server on an automatically assigned IPv4 loopback port. Its six allowlisted responses tile the small fixture bytes to fixed lengths with independently pinned expected SHA-256 digests. The app has only the built-in supplier. No URL, alternate catalog or supplier can be selected through the application bridge.

The test counts HTTP GETs and response body bytes written by the server, then independently checks the client's committed source/reuse counters. Its health checks hash cached files in blocks. The workload also edits a save in the first instance and a setting in the overlapping instance and verifies the other instances' values. The 80 KiB version runs in the default suite. A second test closes a response halfway through staging, confirms the partial object was discarded and generation A and its save survived, then resumes successfully.

The opt-in 1 GiB workload uses the OS temp volume for fresh, disposable data. On the Windows review host this was the same NTFS volume (4096-byte allocation units) used by the application-local directory. The WSL2 test explicitly selected `/tmp` on its ext4 filesystem (4096-byte blocks). Build artifacts remained in the Desktop checkout. Allow approximately 1.25 GiB of free temporary disk space plus filesystem overhead and at least 1 GiB of free memory. No source corpus is written to disk. The largest individual response is approximately 614 MiB and the test client buffers that response before publication.

| Operation | HTTP GETs | HTTP body bytes | Verified reuse (bytes) | NTFS allocated, all objects (bytes) | WSL2 ext4 allocated, all objects (bytes) |
| --- | ---: | ---: | ---: | ---: | ---: |
| Cold 1 GiB instance | 4 | 1,073,741,824 | 0 | 1,073,745,920 | 1,073,758,208 |
| Exact repeat | 0 | 0 | 1,073,741,824 | 1,073,745,920 | 1,073,758,208 |
| Independent instance with 80% overlap | 1 | 214,748,160 | 858,993,664 | 1,288,495,104 | 1,288,511,488 |
| Stage B, 95% unchanged | 1 | 53,686,272 | 1,020,055,552 | 1,342,181,376 | 1,342,197,760 |
| Activate B | 0 | 0 | 0 | 1,342,181,376 | 1,342,197,760 |

The three active selections total 3,221,225,472 logical bytes. Six verified objects, including the retained A selection, contain 1,342,176,256 logical bytes. Percentages round to the nearest percentage point. The table excludes HTTP headers, transport overhead, player files and transient filesystem allocation. It measures an app-owned source over loopback with four objects per generation. External provider downloads, usage with thousands of small mod files, signed registry interoperability, native Linux desktop storage and game-visible filesystem views need separate qualification.

Run `cargo test --release --locked -p carbon-store loopback_http_one_gib -- --ignored --nocapture` on Windows. From WSL at the same checkout run `CARGO_TARGET_DIR=target/wsl-store TMPDIR=/tmp cargo test --release --locked -p carbon-store loopback_http_one_gib -- --ignored --nocapture`. The Windows and WSL runs printed the byte counts above and exited successfully.

For a fresh UI walkthrough, exit the app and rename the entire `fixture-library-v1` directory to a new sibling name in the application's local data directory. The next run initializes a fresh fixture library. To reopen the original, exit, move the new fixture aside and restore the original name. Preserve the whole directory when moving it, including player data and backups.
