#!/bin/bash
set -euo pipefail

image="${1:?Usage: smoke-test.sh <dmg>}"
mount="$(mktemp -d "${TMPDIR:-/tmp}/voxelcraft-mount.XXXXXX")"
attached=false
cleanup() {
    if $attached; then hdiutil detach "$mount"; fi
    rmdir "$mount"
}
trap cleanup EXIT
hdiutil attach -readonly -nobrowse -mountpoint "$mount" "$image"
attached=true
app="$mount/VoxelCraft.app"
plutil -lint "$app/Contents/Info.plist"
codesign --verify --strict "$app"
test "$(readlink "$mount/Applications")" = /Applications
test -f "$mount/READ ME FIRST.txt"
test -f "$app/Contents/Resources/THIRD-PARTY-LICENSES.html"
test "$(/usr/libexec/PlistBuddy -c 'Print CFBundleExecutable' "$app/Contents/Info.plist")" = voxelcraft
"$app/Contents/MacOS/voxelcraft" --version
codesign --verify --strict "$app/Contents/MacOS/voxelcraft-agent"
# Capture first: grep -q closing the pipe early would abort the client.
agent_help="$("$app/Contents/MacOS/voxelcraft-agent" --help)"
grep -q -- --connect <<<"$agent_help"
# Run from a read-only install volume; this must not try writing alongside the app.
(cd "$mount" && "$app/Contents/MacOS/voxelcraft" --bench --rd 2)
# Reject dependencies on build-machine/Homebrew paths.
for exe in voxelcraft voxelcraft-agent; do
    if otool -L "$app/Contents/MacOS/$exe" | tail -n +2 | awk '{print $1}' | grep -Ev '^(/usr/lib/|/System/Library/)' ; then
        echo "Unexpected non-system dylib dependency in $exe" >&2
        exit 1
    fi
done
echo "Disk image, app bundle, integrity signatures and executable launches (game and agent client) passed."
