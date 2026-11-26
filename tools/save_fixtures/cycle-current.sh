#!/usr/bin/env bash
# cycle-current.sh: UX-04a's start, save, quit and reopen of this build on Linux (v1.0 Track 1).
#
#   scripts/boat-check.sh -- 'bash tools/save_fixtures/cycle-current.sh'
#
# Runs in the worktree's Boat sandbox: installs Xvfb and Mesa's software Vulkan the first
# time, builds the game and the agent client with the `ci` profile, runs make-fixture.sh on
# this tree (a new world built and saved by closing the window, then reopened and closed
# again), turns the run into a fixture with collect.py and runs tests/save_compat.rs's
# ignored live-run test on it. Linux has no release package, so this is the source build;
# installing, the app bundle and the Windows installer stay with the owner (docs/saves.md).
set -euo pipefail
cd "$(dirname "$0")/../.."
pkgs="xvfb xauth mesa-vulkan-drivers libvulkan1 libx11-6 libxcursor1 libxrandr2 libxi6 libxkbcommon-x11-0 xdotool"
missing=$(for p in $pkgs; do dpkg-query -W -f='${Status}' "$p" 2>/dev/null | grep -q "install ok installed" || echo "$p"; done)
if [ -n "$missing" ]; then
    sudo apt-get update -qq --allow-releaseinfo-change || true
    # shellcheck disable=SC2086
    sudo DEBIAN_FRONTEND=noninteractive NEEDRESTART_SUSPEND=1 apt-get install -y -qq $missing >/dev/null
fi
cargo build --profile ci --locked --bin voxelcraft --bin voxelcraft-agent
out=$HOME/save-cycle
rm -rf "$out"
VC_FIXTURE_SRC=$PWD VC_FIXTURE_TARGET=$CARGO_TARGET_DIR VC_FIXTURE_OUT=$out \
    bash tools/save_fixtures/make-fixture.sh current 93 4244
python3 tools/save_fixtures/collect.py --dest "$out/fixtures" "$out/current"
grep -h 'exit\|chest' "$out/current/agent.log" | grep -v '^{'
VC_SAVE_FIXTURES=$out/fixtures cargo test --profile ci --locked --test save_compat -- --ignored --nocapture live_run
