# Carbon Desktop contributors

Carbon Desktop targets Windows, Linux, and macOS on x64 and ARM64. GPUI is the primary interface. The secondary Svelte/Tauri edition uses the same Rust services and its own application data root. Keep install, account, execution and storage decisions in those services. Expose named operations to each interface.

Work on a branch and open a pull request against main. Build the Svelte frontend before checking the Rust workspace so that Tauri embeds current assets. Use the pinned Bun and Cargo lockfiles, verify the local design seed, and fix CI failures on the PR branch. The required aggregate check is **Desktop CI Gate**. Use a separate approval for release publication.

Treat pack files, installers and game processes as untrusted. Preserve private saves, settings and backups. Keep personal libraries, credentials, local machine configuration, game downloads and build outputs out of Git. Include only synthetic fixtures and deliberately selected, reviewed screenshots. Distinguish tests, WSL checks and native gameplay when reporting support.

See the [build runbook](docs/implementation.md), [GPUI guide](docs/gpui.md), [architecture](docs/adr/0001-desktop-boundaries.md), [design guide](design-system/README.md) and [launcher matrix](docs/discovery/README.md). Historical verification notes may reference ignored local evidence directories and earlier local commits. Reproduce those checks from the documented inputs.
