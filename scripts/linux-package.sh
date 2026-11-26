#!/usr/bin/env bash
set -euo pipefail

cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
root=$(pwd -P)
build="$root/.artifacts/linux-build"
target=x86_64-unknown-linux-gnu

if [[ $(uname -s) != Linux || $(uname -m) != x86_64 ]]; then
  printf 'Build this package on x86-64 Linux.\n' >&2
  exit 1
fi
if [[ ! -f dist/index.html ]]; then
  printf 'Build the frontend first with bun run build.\n' >&2
  exit 1
fi

mkdir -p "$build"
export CARGO_TARGET_DIR="$build/target"
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_INCREMENTAL=0
if [[ -n ${CARBON_LINUX_SDK:-} ]]; then
  sdk=$(cd -- "$CARBON_LINUX_SDK" && pwd -P)
  sdk_lib="$sdk/usr/lib/x86_64-linux-gnu"
  export PKG_CONFIG_PATH="$sdk_lib/pkgconfig:$sdk/usr/share/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
  export PKG_CONFIG_SYSROOT_DIR="$sdk"
  export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-C link-arg=-Wl,-rpath-link,$sdk_lib"
fi

fingerprint() {
  local files=(Cargo.toml Cargo.lock bun.lock package.json index.html tsconfig.json vite.config.ts
    crates design-system src src-tauri dist)
  find "${files[@]}" -type f ! -path 'src-tauri/gen/*' -print0 |
    sort -z | xargs -0 sha256sum
}

pkg-config --modversion gtk+-3.0 webkit2gtk-4.1 javascriptcoregtk-4.1 wayland-client
fingerprint > "$build/source-before.sha256"
cargo build --locked --target "$target" -p carbon-desktop

bundle_flags=(bundle --debug --target "$target" --bundles deb --ci --no-sign --no-binary-patching)
if [[ -n ${CARBON_LINUX_TAURI_CLI:-} ]]; then
  node "$CARBON_LINUX_TAURI_CLI" "${bundle_flags[@]}"
else
  bun run tauri "${bundle_flags[@]}"
fi

fingerprint > "$build/source-after.sha256"
cmp "$build/source-before.sha256" "$build/source-after.sha256"
output="$CARGO_TARGET_DIR/$target/debug"
sha256sum "$output/carbon-desktop" "$output/bundle/deb/"*.deb
dpkg-deb --info "$output/bundle/deb/"*.deb
