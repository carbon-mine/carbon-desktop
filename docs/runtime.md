<!-- SPDX-License-Identifier: MPL-2.0 -->

# Managed Java preparation

Carbon can prepare an Eclipse Temurin HotSpot JDK on demand for Java 17, 21 or 25 on Windows x86-64 and GNU/Linux x86-64. Review install derives the required JDK major and shows its download size alongside Minecraft game files. Install instance prepares the missing kit. Independent instances reuse the same app-owned directory for that major and pin. Setup derives the Java major from verified vanilla version metadata. Carbon downloads the pinned JDK into its per-user application storage. Existing Java folders, process environment, Minecraft account data and system installation remain untouched.

## Provider and license

The JDK comes directly from Eclipse Adoptium's Temurin release repositories. Eclipse identifies OpenJDK source licensing as GPLv2 with the Classpath and Assembly Exceptions and provides component-specific notices with each binary. Inspection of the pinned Windows and Linux archives found the Java base GPL v2 license and OpenJDK Assembly Exception under `legal/java.base/`. Carbon preserves the package's notice tree, including its linked notices, when it installs the selected archive. The app distributes the pin metadata and installer logic. The user retrieves the runtime bytes from Adoptium's release host. [Eclipse project license listing](https://projects.eclipse.org/projects/adoptium.temurin) · [Adoptium licensing notes](https://adoptium.net/about/)

| Java | Pinned Temurin release | Windows ZIP | Linux tar.gz |
| --- | --- | ---: | ---: |
| 17 | [17.0.21.1+2](https://github.com/adoptium/temurin17-binaries/releases/tag/jdk-17.0.21.1%2B2) | 191,150,540 bytes | 193,630,735 bytes |
| 21 | [21.0.13.1+2](https://github.com/adoptium/temurin21-binaries/releases/tag/jdk-21.0.13.1%2B2) | 205,447,663 bytes | 207,869,899 bytes |
| 25 | [25.0.5.1+2](https://github.com/adoptium/temurin25-binaries/releases/tag/jdk-25.0.5.1%2B2) | 141,295,462 bytes | 141,362,230 bytes |

The [pin table](../crates/runtime/src/artifact.rs) records the exact release URLs, byte lengths and archive SHA-256 values. On 26 November 2026, all six release downloads matched the Adoptium API inventory and each release's adjacent `.sha256.txt` file. Carbon has no floating latest-release resolver. To update a pin, obtain the matching JDK archive and checksum for each OS, compare byte counts and SHA-256 with the versioned provider metadata, review the published archive layout and component notices, and rerun the six installation and cache verification paths. [Adoptium API schema](https://api.adoptium.net/q/swagger-ui/)

## Trust, storage and bridge

- The native caller supplies the app-owned local root and the selected major. Pack data and the webview supply no path, URL, checksum, executable or arguments to the installer. The only supported redirect hosts are GitHub and its approved HTTPS release asset hosts, with no URL credentials or nonstandard port. The client disables inherited proxy configuration and uses platform TLS. Connection timeout is 15 seconds and request timeout is five minutes.
- Rust compares the complete archive byte length and SHA-256 to the pin before archive parsing. Preparation uses a private, same-volume stage and refuses paths with traversal, unsupported characters, Windows device names, file/directory aliases, or case collisions. It also refuses unsupported ZIP entries, archive hardlinks and special files. The archive cap is 256 MiB, with at most 4,096 materialized nodes and 768 MiB of file data. The Windows pins contain regular files and directories. The Linux pins contain only legal-notice symlinks from a module under `legal/` to the same filename in `legal/java.base/`. Carbon copies those expected non-executable notices as separate regular files and refuses all other links.
- A per-major interprocess lock protects prep and cache verification. A candidate retains `source.archive` and an extracted `home/`. Carbon fsyncs prepared files, checks the complete staged tree, then renames the candidate to the pin-derived generation directory. Unix also syncs directory metadata. Later checks rehash the retained archive and compare the entire installed runtime with it, including file contents, exact names, file/link types, extra or missing nodes and Unix execute mode. A failed check returns repair state. A complete cache hit requires no network. Interrupted stages remain private and eligible for scoped cleanup on retry. A failed or cancelled repair leaves the previous install untouched until activation.
- `inspect(root, major)` is read-only and does not contact the network. `prepare(root, major)` prepares or revalidates the cache. `prepare_with(root, major, callback)` additionally reports downloaded/total bytes and checks cancellation between chunks, throughout verification and extraction, and immediately before publishing. Cancellation is cooperative. An in-flight transport read may take until its timeout to return. The caller may rate-limit UI updates. `RuntimeStatus` serializes `status`, `major`, and applicable `release` and `downloadBytes` fields. The JDK includes both the runtime and compiler. An archive missing the platform's java or javac executable is rejected. The ready Java home is Rust-only. The Library uses installed(root, major) for a fast presence check. Full integrity verification occurs during preparation, explicit JDK verification, and before the protected test uses the kit. Starting Java and qualifying a game session belong to the isolated execution path.

## Verify

```sh
cargo test --locked -p carbon-runtime
cargo test --locked -p carbon-runtime live_native_temurin_preparation -- --ignored --nocapture
```

The first command uses inert fixtures to cover verified offline reuse, mutation and extra-file detection, repair, integrity failure, cancellation, conflicting installers, interrupted-stage cleanup, strict notice copying and unsafe archive paths. The second command downloads all three native pins into a disposable per-user directory and verifies the executable file, release metadata, and a cache hit for each. The tests never execute the runtimes. Run the second command on both supported OSes. A WSL result covers Linux-format extraction, file modes and cache checks under that environment. Native Linux desktop and Java launch require separate observations.

On 26 November 2026, the six controlled tests passed on Windows 11 x86-64 with an NTFS test directory and Ubuntu 24.04 x86-64 under WSL2 with a Linux-local test directory. Live download, extraction and cache re-verification passed for all three pins on both systems. The debug test took 148.94 seconds for the three Windows preparations and 142.37 seconds for the three WSL2 preparations, including download and full re-verification. These are single runs and include network time.
