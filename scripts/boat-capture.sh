#!/usr/bin/env bash
# Render screenshots in this worktree's Boat sandbox with a software GPU and copy them back.
#
#   scripts/boat-capture.sh                       every scene in tools/capture_scenes.json
#   scripts/boat-capture.sh --scene NAME ...      only these scenes or groups (repeatable)
#   scripts/boat-capture.sh --name NAME -- ARGS   one capture of the game's own arguments
#                                                 (--screenshot and --data-dir are added)
#   --out DIR                                     where the PNGs go (default
#                                                 target/captures/<commit>[-dirty])
#
# It runs through scripts/boat-check.sh, so it uses the same sandbox, slot (VC_BOAT_SLOT),
# lock and warm `ci` build, and the sandbox stops when the capture ends. In the sandbox it
# installs Xvfb and Mesa's software Vulkan driver (lavapipe) the first time, builds the game
# with the `ci` profile, starts one X server and runs tools/capture_scenes.py. The PNGs come
# back as a tar on the command's standard output; nothing else is written there.
#
# Software rendering shows appearance only, never performance: frame times, FPS and timings
# from these runs mean nothing. See docs/development.md#screenshots-on-boat.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
out="" name="" scenes=() game=()
while [ $# -gt 0 ]; do
    case "$1" in
        --out) out=$2; shift ;;
        --scene) scenes+=("$2"); shift ;;
        --name) name=$2; shift ;;
        --) shift; game=("$@"); break ;;
        -h|--help) sed -n '2,19p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "boat-capture: unknown option $1 (try --help)" >&2; exit 2 ;;
    esac
    shift
done
if [ ${#game[@]} -gt 0 ] && [ -z "$name" ]; then
    echo "boat-capture: give the capture a --name" >&2; exit 2
fi
if [ -n "$name" ] && [ ${#game[@]} -eq 0 ]; then
    echo "boat-capture: --name needs the game's arguments after --" >&2; exit 2
fi
case "$name" in *[!A-Za-z0-9_.-]*) echo "boat-capture: --name may use letters, digits, ., - and _" >&2; exit 2 ;; esac
commit=$(git rev-parse --short=12 HEAD)
[ -z "$(git status --porcelain)" ] || commit="$commit-dirty"
out=${out:-target/captures/$commit}
mkdir -p "$out"

# The capture tool's arguments, quoted for the sandbox's shell.
tool=(--commit "$commit")
if [ -n "$name" ]; then
    tool+=(--adhoc "$name" -- "${game[@]}")
else
    for scene in ${scenes[@]+"${scenes[@]}"}; do tool+=(--scene "$scene"); done
fi
quoted=$(printf '%q ' "${tool[@]}")

# shellcheck disable=SC2016
remote='status=0
{
    pkgs="xvfb xauth mesa-vulkan-drivers libvulkan1 libx11-6 libxcursor1 libxrandr2 libxi6 libxkbcommon-x11-0"
    missing=$(for p in $pkgs; do dpkg-query -W -f="\${Status}" "$p" 2>/dev/null | grep -q "install ok installed" || echo "$p"; done)
    if [ -n "$missing" ]; then
        sudo apt-get update -qq --allow-releaseinfo-change || true
        sudo DEBIAN_FRONTEND=noninteractive NEEDRESTART_SUSPEND=1 apt-get install -y -qq $missing >/dev/null
    fi
    cargo build --profile ci --locked --bin voxelcraft
    out=$HOME/captures
    rm -rf "$out"; mkdir -p "$out"
    # One virtual screen larger than any capture; without a window manager each window
    # gets exactly the size it asks for.
    # A server left by a killed run; the bracket keeps pkill from matching this script.
    pkill -f "[X]vfb :91" 2>/dev/null || true
    Xvfb :91 -screen 0 3840x2160x24 -nolisten tcp >/dev/null 2>&1 &
    xvfb=$!
    trap "kill $xvfb 2>/dev/null || true" EXIT
    sleep 1
    # Dates in the world list are shown in UTC.
    export DISPLAY=:91 TZ=UTC
    renderer="Mesa $(dpkg-query -W -f="\${Version}" mesa-vulkan-drivers) lavapipe (software Vulkan) under Xvfb"
    python3 tools/capture_scenes.py run --binary "$CARGO_TARGET_DIR/ci/voxelcraft" --out "$out" \
        --renderer "$renderer" '"$quoted"' || status=$?
} 1>&2
tar -C "$HOME/captures" -cf - . | base64 -w0
exit $status'

tmp=$(mktemp "${TMPDIR:-/tmp}/boat-capture.XXXXXX")
trap 'rm -f "$tmp"' EXIT
status=0
scripts/boat-check.sh -- "$remote" >"$tmp" || status=$?
if [ -s "$tmp" ]; then
    base64 -d <"$tmp" | tar -x -C "$out"
    echo "boat-capture: $(find "$out" -name '*.png' | wc -l | tr -d ' ') PNGs in $out (manifest.json lists each capture's settings)" >&2
else
    echo "boat-capture: no captures came back" >&2
fi
exit "$status"
