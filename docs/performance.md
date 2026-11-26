<!-- SPDX-License-Identifier: MPL-2.0 -->

# Library and pack-preview benchmarks

26 November 2026. Measurements use candidate source `693fc7452ba995b8d1aa5899faaace6ad22735e2` and its built Svelte distribution on Windows 11 Pro build 26300 with an Intel Core i9-12900HK and 20 logical processors. The renderer ran in headless Edge 158.0.4370.33. The archive harness ran the Rust parser in a release build using rustc 1.99.1. Each workload used warm assets or filesystem cache. Percentiles use the nearest-rank method.

## Renderer feedback

The real `dist/` sits behind a Playwright request route that fulfills local assets and aborts other requests. A test-only Tauri bridge provides 97 personal authoring records across five version labels and three local fixtures. Forest awaits review and Coast has an interrupted update. Search exercises all profiles, matches by version, the two Forest rows, an empty result and clearing the query. Selection alternates personal and fixture rows and checks the new heading, selected-row state and focus.

The timer runs from events injected in the renderer to the first animation frame that sees the expected DOM and layout. It measures renderer frame response. Native OS input dispatch, actual pixel presentation, backend IPC, disk, cold startup and screen-reader response need separate tests. The viewport was 1040 by 760 CSS pixels at scale 1. One warm-up round was discarded.

| Action | Samples | p50 | p95 |
| --- | ---: | ---: | ---: |
| Search feedback over 100 records | 60 | 6.1 ms | 6.4 ms |
| Selection and heading focus | 24 | 6.0 ms | 6.1 ms |
| Cancel to visible Working feedback | 12 | 6.0 ms | 6.2 ms |
| Cancel to completed update status with a mock 120 ms response delay | 12 | 128.7 ms | 130.8 ms |

The mock recorded two concurrent initial library reads. The two activity states stayed visible during search. The cancel event closed one review and returned its status after the synthetic delay. The candidate serializes UI mutations. Concurrent native jobs and end-to-end cancellation remain unqualified.

## Split workspace follow-up

The subsequent split workspace opens 100 personal rows in Library and loads three recovery rows when Review tools opens. A repeat of the same headless Edge harness measured p95 search at 6.3 ms over 60 events, personal selection at 6.1 ms over 12 events, and fixture cancel feedback at 6.1 ms over 12 events. Completed cancellation took 127.8 ms p95 including the simulated 120 ms response. The harness checks selected-row state and heading focus. These measurements apply to renderer feedback in the new layout. The earlier table describes the original combined library.

## Local archive parser

The harness calls the actual [read-only preview parser](../src-tauri/src/pack.rs) with generated local `.mrpack` files. The measured duration includes opening, reading, checking and serializing the result, with a warm-up call per input. The last row measures wall time for two parser calls released by the same thread barrier. The test uses no native picker, installer, downloader or unpacking step.

| Fixture | ZIP entries | Listed descriptors | Archive bytes | Samples | p50 | p95 | Outcome |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| Allowed workload | 4,096 | 512 | 522,966 | 30 | 4.255 ms | 5.421 ms | Preview reports 512 listed files and 4,095 overrides |
| Entry count over limit | 5,000 | 0 | 635,122 | 30 | 1.765 ms | 2.289 ms | Rejected in the ZIP directory check |
| Descriptor workload over index byte limit | 1 | 5,000 | 23,572 | 30 | 0.078 ms | 0.196 ms | Rejected at the 256 KiB uncompressed index limit. The generated index is 1,420,145 bytes. |
| Two concurrent allowed previews | 4,096 each | 512 each | 522,966 each | 30 pairs | 5.120 ms | 6.077 ms | Both calls report the expected metadata |

The 5,000-item results measure refusal. This build caps the archive at 16 MiB, its index at 256 KiB, and both ZIP entries and descriptors at 4,096. Actual descriptor capacity also depends on the serialized index size. Larger archive acceptance awaits a separate limit and performance review.

## Reproduce

Build the frontend with `bun run build` first. Use Python 3.11 or newer, Playwright and an installed Chromium-based browser. The archive run needs Cargo and the repository's locked crates in its local cache. The helper copies the lock into the ignored output directory, removes unrelated workspace entries through offline resolution, and imports the checked-in parser unchanged.

```powershell
python scripts/library-benchmark.py --browser-executable 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe'
```

Add `--only renderer` or `--only archive` to repeat one surface. The command writes raw timings, hashes, generated archives and the temporary helper under `.artifacts/library-benchmark/`. Complete results are in `all.json`. The test starts no foreground app and uses no launcher profile data. The separate [storage benchmark](storage.md) measures verified bytes and allocation. Native package inspection is in the [Linux build handoff](linux-build.md).
