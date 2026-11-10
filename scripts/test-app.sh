#!/usr/bin/env bash
# Wrap a built game binary in a throwaway macOS app bundle so desktop
# automation (computer use) can see and drive its window. A bare executable
# isn't listed as an app; this bundle is, like the released game's.
#
#   scripts/test-app.sh BINARY [APP]
#
# APP defaults to "target/hands-on/VoxelCraft Test.app" (ignored by git).
# The bundle has its own identifier, so it never collides with an installed
# VoxelCraft, and an ad-hoc signature, as packaging/macos/package.sh gives
# the release. Launch it with an isolated data folder:
#
#   open -n "target/hands-on/VoxelCraft Test.app" --args \
#       --data-dir "$PWD/target/hands-on/data" --no-update-check
#
# The screen must be unlocked: a locked session has no windows to attach to.
# See docs/development.md#hands-on-runs.
set -euo pipefail
[ $# -ge 1 ] || { sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
repo="$(cd "$(dirname "$0")/.." && pwd)"
bin=$1
app=${2:-"$repo/target/hands-on/VoxelCraft Test.app"}
[ -x "$bin" ] || { echo "test-app: $bin is not an executable" >&2; exit 1; }
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$bin" "$app/Contents/MacOS/voxelcraft"
chmod 755 "$app/Contents/MacOS/voxelcraft"
cp "$repo/packaging/icons/VoxelCraft.icns" "$app/Contents/Resources/"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
    <key>CFBundleName</key><string>VoxelCraft Test</string>
    <key>CFBundleDisplayName</key><string>VoxelCraft Test</string>
    <key>CFBundleIdentifier</key><string>io.github.brendanh18.voxelcraft.handson</string>
    <key>CFBundleExecutable</key><string>voxelcraft</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>0.0.0</string>
    <key>CFBundleVersion</key><string>0.0.0</string>
    <key>CFBundleIconFile</key><string>VoxelCraft.icns</string>
    <key>LSMinimumSystemVersion</key><string>13.0</string>
    <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
plutil -lint "$app/Contents/Info.plist" > /dev/null
codesign --force --sign - "$app"
codesign --verify --strict "$app"
echo "test-app: built $app"
