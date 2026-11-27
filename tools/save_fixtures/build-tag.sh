#!/usr/bin/env bash
# build-tag.sh TAG: builds that tag's game and agent client with the ci profile into ~/tgt/TAG,
# from a worktree ~/t/TAG of the clone ~/vc (v1.0 Track 1's save fixtures; Boat only).
set -euo pipefail
tag=$1
cd ~/vc
[ -d ~/t/$tag ] || git worktree add -q ~/t/$tag $tag
cd ~/t/$tag
# Tags before v0.16 have no ci profile; add the one v0.16 introduced (release without LTO).
# This changes only how the tag is optimised, not its code.
if ! grep -q 'profile.ci' Cargo.toml; then
  printf '\n[profile.ci]\ninherits = "release"\nlto = false\ncodegen-units = 16\n' >> Cargo.toml
fi
export CARGO_TARGET_DIR=~/tgt/$tag
cargo +1.100.0 build --profile ci --locked --bin voxelcraft --bin voxelcraft-agent 2>&1 | tail -5
ls -la $CARGO_TARGET_DIR/ci/voxelcraft $CARGO_TARGET_DIR/ci/voxelcraft-agent
