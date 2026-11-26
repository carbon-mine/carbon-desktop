#!/bin/sh
# A fixed M1 filesystem experiment. Run as a standard Linux user.
set -eu
[ "$(id -u)" -ne 0 ] || { echo "Run as an ordinary user." >&2; exit 1; }
for tool in /usr/bin/bwrap /usr/bin/dash /usr/bin/mktemp; do test -x "$tool"; done
base=$(mktemp -d)
# Only this invocation's private mktemp directory is disposable.
trap 'rm -rf -- "$base"' EXIT
mkdir "$base/one" "$base/two"
printf 'fixture A' > "$base/immutable"
for id in one two; do
  /usr/bin/bwrap --unshare-all --die-with-parent --new-session --cap-drop ALL \
    --clearenv --setenv PATH /usr/bin --ro-bind /usr /usr \
    --symlink usr/lib /lib --symlink usr/lib64 /lib64 \
    --proc /proc --dev /dev --tmpfs /tmp \
    --ro-bind "$base/immutable" /artifact --bind "$base/$id" /instance \
    --chdir /instance /usr/bin/dash -ec '
      printf "parent" > parent
      /usr/bin/dash -ec '\''test "$(cat /artifact)" = "fixture A"
        test "$(cat /instance/parent)" = parent
        if (printf bad > /artifact) 2>/dev/null; then exit 11; fi
        test ! -e "$1"; test ! -e /mnt/c; test ! -e /home
        printf "child" > /instance/child'\'' probe "$1"
    ' probe "$base"
  test "$(cat "$base/$id/parent")" = parent
  test "$(cat "$base/$id/child")" = child
done
test "$(cat "$base/immutable")" = 'fixture A'
echo 'PASS two fresh namespace views, private writes, child writes, shared read-only input and hidden host fixture parent.'
