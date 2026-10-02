#!/bin/bash
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"
target="${1:-aarch64-apple-darwin}"
case "$target" in
    aarch64-apple-darwin) arch="apple-silicon" ;;
    x86_64-apple-darwin) arch="intel" ;;
    *) echo "Unsupported macOS target: $target" >&2; exit 1 ;;
esac
version="$(cargo metadata --no-deps --format-version 1 --locked | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "voxelcraft"))')"
binary="target/$target/release/voxelcraft"
agent="target/$target/release/voxelcraft-agent"
if [ ! -f "$binary" ] || [ ! -f "$agent" ]; then
    echo "Build first: cargo build --release --locked --target $target" >&2
    exit 1
fi
if [ ! -f packaging/THIRD-PARTY-LICENSES.html ]; then
    echo "Generate third-party notices with cargo about (see docs/releases.md)" >&2
    exit 1
fi
stage="$(mktemp -d "${TMPDIR:-/tmp}/voxelcraft-package.XXXXXX")"
trap 'rm -rf "$stage"' EXIT
app="$stage/VoxelCraft.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" dist
cp "$binary" "$app/Contents/MacOS/voxelcraft"
# The command-line client for hosted players ships beside the game.
cp "$agent" "$app/Contents/MacOS/voxelcraft-agent"
chmod 755 "$app/Contents/MacOS/voxelcraft" "$app/Contents/MacOS/voxelcraft-agent"
cp packaging/icons/VoxelCraft.icns "$app/Contents/Resources/"
cp packaging/INSTALL.txt packaging/LICENSE.txt packaging/THIRD-PARTY-LICENSES.html LICENSE-MIT LICENSE-APACHE "$app/Contents/Resources/"
cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
    <key>CFBundleName</key><string>VoxelCraft</string>
    <key>CFBundleDisplayName</key><string>VoxelCraft</string>
    <key>CFBundleIdentifier</key><string>io.github.brendanh18.voxelcraft</string>
    <key>CFBundleExecutable</key><string>voxelcraft</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>CFBundleIconFile</key><string>VoxelCraft.icns</string>
    <key>LSMinimumSystemVersion</key><string>13.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSHumanReadableCopyright</key><string>VoxelCraft contributors. MIT OR Apache-2.0.</string>
</dict></plist>
EOF
plutil -lint "$app/Contents/Info.plist"
# No Developer ID or notarization. An ad-hoc signature supplies the local code
# integrity required by Apple Silicon; it does not identify a trusted publisher.
# Nested code is signed before the bundle that seals it.
codesign --force --sign - "$app/Contents/MacOS/voxelcraft-agent"
codesign --force --sign - "$app"
codesign --verify --strict "$app"
cp packaging/INSTALL.txt "$stage/READ ME FIRST.txt"
cp packaging/THIRD-PARTY-LICENSES.html "$stage/"
ln -s /Applications "$stage/Applications"
output="$repo/dist/VoxelCraft-$version-macos-$arch.dmg"
hdiutil create -volname "VoxelCraft $version" -srcfolder "$stage" -ov -format UDZO "$output"
hdiutil verify "$output"
echo "Built $output"
