<!-- SPDX-License-Identifier: MPL-2.0 -->

# Linux development package

The Linux review package uses the checked frontend in `dist/` and builds the x86-64 GNU/Linux debug executable with static web assets. It is an unsigned development artifact. The package includes the Carbon executable and launch metadata. Run it on a compatible desktop with GTK and WebKitGTK runtime dependencies. Linux build-host evidence and desktop qualification are tracked separately.

## Build on x86-64 Linux

Use cargo, rustc, a C toolchain, pkg-config, dpkg-deb, Bun and the GTK 3, WebKitGTK 4.1, Wayland and xdo development files. The Tauri prerequisites describe distribution-specific packages. The script accepts prebuilt static frontend output, so the same `dist/` can also be built on a Windows review host. Run these two commands where Bun and the checkout dependencies are installed. [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) · [Debian packaging](https://v2.tauri.app/distribute/debian/)

```sh
bun install --frozen-lockfile
bun run build
```

From the same checkout on an x86-64 Linux build host:

```sh
bash scripts/linux-package.sh
```

The command runs locked Cargo resolution and the Tauri Debian bundler with the debug profile. It prints executable and package hashes and Debian control metadata. Check `.artifacts/linux-build/source-before.sha256` for the hashed Rust, UI, design, lockfile and compiled frontend inputs. Keep its matching `source-after.sha256` with the build evidence.

## Ubuntu 24.04 WSL2 build without system package changes

The build host had rustc 1.96.0 and Node. GTK 3, WebKitGTK 4.1 and Wayland pkg-config data were missing. The apt solver used the host's existing Ubuntu Noble indexes. Downloaded Debian archives were unpacked under the ignored Desktop build path for linker and pkg-config use. Native tools, ordinary WSL package state and user profiles were preserved.

On an Ubuntu Noble x86-64 build host with usable apt indexes, resolve and extract the same class of dependencies:

```sh
build="$PWD/.artifacts/linux-build"
mkdir -p "$build/debs" "$build/sdk"
LC_ALL=C apt-get -s --no-install-recommends install libgtk-3-dev libwebkit2gtk-4.1-dev \
  libwayland-dev libxdo-dev libayatana-appindicator3-dev > "$build/apt-plan.txt"
awk '$1 == "Inst" { print $2 }' "$build/apt-plan.txt" > "$build/apt-packages.txt"
(cd "$build/debs" && xargs -r -P 8 -n 10 apt-get download < ../apt-packages.txt)
for archive in "$build"/debs/*.deb; do
  dpkg-deb --fsys-tarfile "$archive" | tar -xf - -C "$build/sdk" \
    --exclude='./usr/share/doc' --exclude='./usr/share/man' \
    --exclude='./usr/share/icons' --exclude='./usr/share/locale' \
    --exclude='./usr/share/help' --exclude='./usr/share/themes' \
    --exclude='./usr/share/gtk-doc' --exclude='./usr/share/gir-1.0'
done
```

The apt plan leaves already-installed runtime libraries on the host. When that leaves a development symlink dangling in the SDK, link the missing SONAME to the existing host library of the same name. Record these paths in the build evidence. Run this command in Bash.

```bash
sdk_lib="$build/sdk/usr/lib/x86_64-linux-gnu"
find "$sdk_lib" -maxdepth 1 -xtype l -print0 | while IFS= read -r -d '' link; do
  target=$(readlink "$link")
  case "$target" in */*) continue ;; esac
  host_lib="/usr/lib/x86_64-linux-gnu/$target"
  if [[ -f "$host_lib" && ! -e "$sdk_lib/$target" && ! -L "$sdk_lib/$target" ]]; then
    ln -s "$host_lib" "$sdk_lib/$target"
    printf '%s\n' "$host_lib"
  fi
done > "$build/host-soname-links.txt"
```

When the checkout already has compatible Linux frontend dependencies, the test can use the normal bundler path. Set `CARBON_LINUX_SDK="$build/sdk"` for the script. If the checkout's `node_modules` belongs to the Windows frontend build, obtain the separate Linux Tauri CLI pinned in `bun.lock`. Store it alongside the SDK to keep the Windows toolchain intact. Pin both tarballs and compare the SHA-512 fingerprints with the lockfile before extracting.

```sh
mkdir -p "$build/cli/node_modules/@tauri-apps/cli-linux-x64-gnu"
curl -fLsS https://registry.npmjs.org/@tauri-apps/cli/-/cli-2.12.6.tgz -o "$build/cli.tgz"
curl -fLsS https://registry.npmjs.org/@tauri-apps/cli-linux-x64-gnu/-/cli-linux-x64-gnu-2.12.6.tgz \
  -o "$build/cli-platform.tgz"
for archive in "$build/cli.tgz" "$build/cli-platform.tgz"; do
  openssl dgst -sha512 -binary "$archive" | openssl base64 -A
  printf '\n'
done
```

After confirming those two fingerprints match the corresponding `@tauri-apps/cli` and `@tauri-apps/cli-linux-x64-gnu` lock entries:

```sh
tar -xzf "$build/cli.tgz" --strip-components=1 -C "$build/cli"
tar -xzf "$build/cli-platform.tgz" --strip-components=1 \
  -C "$build/cli/node_modules/@tauri-apps/cli-linux-x64-gnu"
node "$build/cli/tauri.js" --version
CARBON_LINUX_SDK="$build/sdk" CARBON_LINUX_TAURI_CLI="$build/cli/tauri.js" \
  bash scripts/linux-package.sh
```

The observed local SDK reported GTK 3.24.41, WebKitGTK and JavaScriptCoreGTK 2.52.6, and Wayland 1.22.0. The local build evidence records the 227 downloaded archives in `.artifacts/linux-build/sdk-debs.sha256` and 68 host-library resolutions in `.artifacts/linux-build/host-soname-links.txt`.

## Personal setup package

Building the HTTPS adapters on Linux also requires the OpenSSL development headers (`libssl-dev` on the Ubuntu build host). The bundled application links to OpenSSL 3 and uses the system certificate roots. The setup candidate's Debian control file declares `libssl3t64 | libssl3`, `ca-certificates`, WebKitGTK and GTK. The [active handoff](handoff.md#current-standalone-setup-candidate) identifies its executable and package hashes.

## 26 November 2026 original profile candidate

Built on Ubuntu 24.04.4 x86-64 under WSL2, kernel `6.6.87.2-microsoft-standard-WSL2`, GCC 13.3, rustc 1.96.0, Node 24.19.0 and Tauri CLI 2.12.6. The application source matches commit `693fc7452ba995b8d1aa5899faaace6ad22735e2`. The identical 88-entry before/after source and frontend manifests have SHA-256 `5b60ee64755b943d9fc155aa1e6a11c41cc4ddd8953a9d453984120ff5a463a7`. Build and inspection output is in `.artifacts/linux-build/package-candidate.log`.

Both paths below are relative to `.artifacts/linux-build/target/x86_64-unknown-linux-gnu/debug/`.

| Artifact | Size | SHA-256 |
| --- | --- | --- |
| `carbon-desktop` | 24,147,032 bytes | `17ca29cfaf82547ff9d00a5921df494dc63e712218071616300dece556021b53` |
| `bundle/deb/Carbon Desktop_0.0.0_amd64.deb` | 6,779,490 bytes | `fc4d0f8dd1e5195d9f1459f97aac174f165b9376365ffc89c404af6a14bb1522` |

ELF and Debian inspection verified x86-64, matching binaries inside and outside the package, a desktop entry and an icon. The ELF directly requires GLIBC 2.39 and has no runtime search path. All shared libraries resolved on the WSL build host with the local SDK in its inspection environment. The package declares dependencies on `libwebkit2gtk-4.1-0` and `libgtk-3-0`. For this build, use an Ubuntu 24.04 x86-64 desktop or another confirmed compatible desktop with host-managed runtime dependencies.

On that desktop, run the executable directly for a disposable seeded review:

```sh
.artifacts/linux-build/target/x86_64-unknown-linux-gnu/debug/carbon-desktop \
  --temporary-review --seed-review
```

Linux package installation, rendering, picker, assistive technology and gameplay still need native desktop evidence. The WSL result verifies compilation, linkage and archive contents. Follow the [candidate handoff](handoff.md) for data-preserving review instructions.
