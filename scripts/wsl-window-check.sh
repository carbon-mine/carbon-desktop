#!/usr/bin/env bash
# Offscreen WSL2 candidate check. The SDK comes from docs/linux-build.md.
set -euo pipefail
cd "$(dirname "$0")/.."
(( EUID != 0 )) || { echo 'Run as an ordinary WSL user.' >&2; exit 1; }
build=$PWD/.artifacts/linux-build
app=$build/target/x86_64-unknown-linux-gnu/debug/carbon-desktop
sdk=$build/sdk/usr
out=$(mktemp -d "$build/window-check-XXXXXXXX")
private=$(mktemp -d)
trap 'rm -rf -- "$private"' EXIT
mkdir -m 700 "$private"/{home,runtime,tmp,schemas,config,data,cache}
lib=$sdk/lib/x86_64-linux-gnu
# WebKitGTK resolves its helper executables at the compiled system path.
# Assemble that path in a disposable mount namespace, preserving host package state.
mkdir "$private/libview" "$private/binview"
python3 - "$private/libview" "$private/binview" <<'PY'
from pathlib import Path
import sys
view = Path(sys.argv[1])
for item in Path("/usr/lib/x86_64-linux-gnu").iterdir():
    if item.name != "webkit2gtk-4.1":
        (view/item.name).symlink_to(Path("/usr/local/lib")/item.name)
(view/"webkit2gtk-4.1").mkdir()
binview = Path(sys.argv[2])
for item in Path("/usr/bin").iterdir():
    if item.name != "xdg-dbus-proxy":
        (binview/item.name).symlink_to(Path("/usr/local/bin")/item.name)
(binview/"xdg-dbus-proxy").touch()
PY
schema_compiler=$sdk/bin/glib-compile-schemas
[[ -x $schema_compiler ]] || schema_compiler=/usr/lib/x86_64-linux-gnu/glib-2.0/glib-compile-schemas
LD_LIBRARY_PATH="$lib" "$schema_compiler" --targetdir="$private/schemas" "$sdk/share/glib-2.0/schemas"
printf 'Evidence directory: %s\n' "$out"
sha256sum "$app" > "$out/executable.sha256"
timeout -k 4 28 env -i PATH=/usr/bin:/bin LC_ALL=C.UTF-8 HOME="$private/home" \
  TMPDIR="$private/tmp" XDG_CONFIG_HOME="$private/config" XDG_CACHE_HOME="$private/cache" \
  XDG_DATA_HOME="$private/data" XDG_RUNTIME_DIR="$private/runtime" GDK_BACKEND=x11 \
  GSETTINGS_BACKEND=memory GSETTINGS_SCHEMA_DIR="$private/schemas" LD_LIBRARY_PATH="$lib" \
  GI_TYPELIB_PATH="$lib/girepository-1.0" WEBKIT_EXEC_PATH="$lib/webkit2gtk-4.1" \
  WEBKIT_FORCE_SANDBOX=1 LIBGL_ALWAYS_SOFTWARE=1 NO_AT_BRIDGE=1 XDG_DATA_DIRS="$sdk/share:/usr/share" \
  xvfb-run -a -s '-screen 0 1280x900x24 -nolisten tcp' dbus-run-session -- \
  bwrap --die-with-parent --unshare-pid --ro-bind / / --proc /proc --dev /dev --bind "$private" "$private" --bind "$out" "$out" \
    --ro-bind /usr/lib/x86_64-linux-gnu /usr/local/lib --ro-bind "$sdk" /usr/local/share \
    --ro-bind "$private/libview" /usr/lib/x86_64-linux-gnu --ro-bind "$lib/webkit2gtk-4.1" /usr/lib/x86_64-linux-gnu/webkit2gtk-4.1 \
    --setenv LD_LIBRARY_PATH /usr/local/share/lib/x86_64-linux-gnu \
    --ro-bind /usr/bin /usr/local/bin --ro-bind "$private/binview" /usr/bin \
    --ro-bind "$sdk/bin/xdg-dbus-proxy" /usr/bin/xdg-dbus-proxy \
  python3 scripts/wsl-window-check.py "$app" "$out" > "$out/process.log" 2>&1 || {
      tail -c 2400 "$out/process.log" >&2
      exit 1
  }
cat "$out/process.log"
