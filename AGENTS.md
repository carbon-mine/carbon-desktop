# Carbon Desktop contributors

Carbon Desktop targets Windows, Linux, and macOS on x64 and ARM64. GPUI is the sole active frontend. The former Svelte/Tauri shell is a historical visual reference. Keep install, account, execution and storage decisions in those services. Expose named operations to GPUI.

Work on a branch and open a pull request against main. Track work in the Desktop Jira project. Build the locked GPUI workspace, verify the design seed and fix CI failures on the PR branch. Obtain independent review, address CodeRabbit findings, verify user-visible changes in the native app and update Jira after the protected merge. The required aggregate check is **Desktop CI Gate**. Use a separate approval for release publication.

Treat pack files, installers and game processes as untrusted. Preserve private saves, settings and backups. Keep personal libraries, credentials, local machine configuration, game downloads and build outputs out of Git. Include only synthetic fixtures and deliberately selected, reviewed screenshots. Distinguish tests, WSL checks and native gameplay when reporting support.

See the [build runbook](docs/implementation.md), [GPUI guide](docs/gpui.md), [architecture](docs/adr/0001-desktop-boundaries.md), [design guide](design-system/README.md) and [launcher matrix](docs/discovery/README.md). Historical verification notes may reference ignored local evidence directories and earlier local commits. Reproduce those checks from the documented inputs.
