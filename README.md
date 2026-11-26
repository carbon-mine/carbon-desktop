<!-- SPDX-License-Identifier: MPL-2.0 -->

<div align="center">
  <h1>Carbon Desktop</h1>
  <p><strong>A native Minecraft Java launcher in development for Windows, Linux, and macOS.</strong></p>
  <p>Organize local instances, review what each version needs, and prepare its game files and Java runtime from one desktop workspace.</p>
  <p>
    <a href="#quick-start">Quick start</a> ·
    <a href="#what-you-can-do">Features</a> ·
    <a href="#project-status">Project status</a> ·
    <a href="LICENSE">MPL 2.0</a>
  </p>
</div>

![Carbon Desktop on Windows with a 100-instance library, a selected Vanilla survival instance, and its Review install action](docs/images/gpui-library.png)

*A disposable library in the GPUI edition. The selected instance shows its Minecraft version, Java requirement, and next setup action.*

## What you can do

- **Organize your instances.** Create, edit, and clone vanilla profiles. Search the local library, set memory and starting window size, and open an instance’s private game folder.
- **Review and prepare an installation.** Review the download size first, then fetch the version’s game files directly from Mojang and install a matching Eclipse Temurin JDK in Carbon’s application storage. Follow progress, cancel, or verify prepared files offline.
- **Import a local modpack.** Review a Modrinth `.mrpack` before creating its own instance. Carbon copies the reviewed overrides and records any loader or dependencies still required.
- **Use a desktop workspace.** The library and selected instance sit side by side in wide windows. At narrow widths, Back to library and Escape return to the instance list. The GPUI edition has native file pickers, keyboard navigation, and System, Light, and Dark appearance settings.

<table>
  <tr>
    <td><img src="docs/images/gpui-narrow-library.png" width="390" alt="A narrow Carbon Desktop window showing search and the populated local instance list"></td>
    <td><img src="docs/images/gpui-narrow-instance.png" width="390" alt="The same narrow window showing the selected Vanilla survival instance, Back to library, and Review install"></td>
  </tr>
  <tr>
    <td align="center">Search and select an instance.</td>
    <td align="center">Review setup in the selected instance.</td>
  </tr>
</table>

## Quick start

### Native GPUI edition

Use a stable Rust toolchain. On Windows, install the MSVC C++ build tools and Windows SDK. A JDK with `javac` is optional at build time and enables the protected Java sample. On Linux, use a graphical session with Vulkan support and install the C toolchain, `pkg-config`, and OpenSSL, fontconfig, FreeType, XCB, and XKB development libraries.

From this repository, run:

```sh
cargo run --locked -p carbon-desktop-gpui -- --comparison-review
```

The comparison flag opens a temporary library with sample profiles. Run without the flag to keep changes in your normal Carbon Desktop library.

Select an instance, then choose **Review install** to inspect its download plan. **Install instance** prepares the reviewed files and the required JDK. **Verify game files** checks the result offline.

### Svelte/Tauri edition

The secondary interface uses the same Rust core and maintains its own local library. Install Bun and the system dependencies for Tauri. [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

```sh
bun install --frozen-lockfile
bun run tauri dev
```

## Project status

Carbon Desktop is a development preview. Vanilla setup and offline verification are available. The Play action leads to setup and account requirements. Protected Minecraft launch, authenticated multiplayer, and full mod-loader installation are still in development. Microsoft sign-in requires an approved application registration.

The project targets x64 and ARM64 on all three operating systems. Native gameplay, account storage, and installer behavior require separate checks for each target.

The [GPUI application](crates/gpui-launcher) is the primary desktop frontend. The [Svelte interface](src) and [Tauri shell](src-tauri) remain available for comparison. Shared Rust code owns [profiles](crates/profiles), [Java runtime preparation](crates/runtime), [vanilla installation](crates/vanilla), and [pack import](crates/pack).

## License

Carbon Desktop source is available under the [Mozilla Public License 2.0](LICENSE). Bundled and inherited material retains its own license and notices. See the [third-party notices](third_party) and [inherited design assets](design-system/inherited).
