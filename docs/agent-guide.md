# Agent engineering guide

[Documentation index](README.md) · [Agent instructions](../AGENTS.md) · [Development tools](development.md)

Project instructions and engineering guidance live in this repository and are
versioned with the code. There is no separate agent-notes checkout to install.
`AGENTS.md` defines workflow; `docs/roadmap.md` defines goals and acceptance.
Task-specific assignments belong in GitHub issues, PR descriptions or the working
conversation. Keep only useful historical briefs in `docs/archive/briefs/`.

## Checks and feature evidence

Before handing work back, run the chain in [AGENTS.md](../AGENTS.md#before-handing-work-back).
CI additionally runs a headless benchmark on Linux, macOS and Windows; see
[the workflow](../.github/workflows/ci.yml). For engine/shared-feature changes,
also check the device-independent configuration:

```sh
cargo clippy --release --no-default-features --all-targets -- -D warnings
cargo test --release --no-default-features
```

Tests validate every WGSL shader with naga. Check changed gameplay against the
roadmap's selected Java reference. Record setup, expected behavior, observed
results and the tested commit. Add meaningful regression cases for defects;
tests that repeat the implementation are not independent reference evidence.

Run save/restart and unload/reload checks for persistent state. Use an isolated
`--data-dir target/playtest`; do not overwrite a player's normal world. Inspect
old saves when IDs or storage change. Do not infer complete gameplay from a
successful constructor, screenshot or build.

## Visual checks

`--screenshot <file>` writes a PNG once the world loads and then exits.
`--pose x,y,z,yaw,pitch` takes angles in degrees. Add `--wait`, `--time`,
`--spawn`, `--give` and `--drop` for a reproducible scene. `--place x,y,z,<name>`
also accepts a raw block-state ID, useful for oriented states.

Build inspection platforms around y=150 to avoid placing the camera inside
terrain. Use `--data-dir target/screenshots` and write captures under `target/`.
In zsh, a scalar variable does not word-split: construct repeated `--place`
arguments with an array or a Python subprocess argument list. Full examples
are in [the development guide](development.md#screenshots-and-scripted-scenes).

## Performance checks

`cargo run --release -- --bench --rd 8` measures generation, meshing and
streaming without opening a GPU window. VoxelCraft uses 32-block chunks, so
`--rd 8` corresponds to a 256-block view distance, Java's 16 chunks.

Historical results live in [architecture](architecture.md#performance-design)
and dated performance reports. They are not the roadmap's 1080p/240 FPS
acceptance result. Compare the same scene, seed, build, resolution, view distance
and hardware; report frame-time distribution, streaming behavior and memory
when relevant. The [restructuring plan](restructuring.md#str-01--reference-fixtures-and-performance-scenes)
defines the planned reproducible suite.

## Engine pointers

- Shaped blocks are boxes in `src/world/shape.rs`, meshed as detail quads with
  the flag in bit 31 of quad word 1.
- Gameplay block light belongs to `World`. Mesh jobs calculate render light
  independently and cannot replace gameplay light. See
  [the simulation boundary](architecture.md#simulation-boundary).
- Slabs and stairs currently use an opaque-cell lighting approximation;
  shape-aware lighting remains a roadmap requirement.
- First-person hand rendering lives in `src/render/hand.rs`; its animation
  lives in `src/app/hand.rs`.
- The Nether has no sky heightmap (`Dimension::has_sky`).
- Consult `src/world/block.rs`, `src/item.rs`, the architecture's
  [ID/storage notes](architecture.md#block-and-texture-ids), and pending branches
  before allocating IDs. Preserve old save meanings and use the conversion APIs.

Update these pointers with the implementation rather than maintaining an
unversioned copy elsewhere.

## Transition for older local worktrees

The initial migration replaced the old external-note symlinks with ordinary
files in the owner's existing worktrees. Older branches temporarily have ignored local copies of
`AGENTS.md`, `CLAUDE.md` and a root `roadmap.md` pointer until this change is
integrated. These copies are transitional, not a second authoritative plan.
The pointer identifies the published documentation branch for missing documents.

After integrating this PR into an older branch, use the tracked instructions and
`docs/roadmap.md`; remove the ignored root pointer. Once no older worktrees need
local copies, remove their temporary entries from `.git/info/exclude`.
Do not copy the obsolete checkout hook into new clones.

Git does not update another checkout's local hooks or excludes when a PR merges.
For any other checkout still using the former setup:

1. Back up its notes, then inspect its checkout hook (including a configured
   `core.hooksPath`, if present). Remove only the external-note linking code;
   preserve unrelated hooks.
2. Replace external `AGENTS.md` and `CLAUDE.md` symlinks with the tracked files
   from this branch. Remove the root `roadmap.md` symlink and use `docs/roadmap.md`.
   Inspect each existing worktree without changing its code or branch.
3. Remove obsolete local exclude entries when no older worktree needs the
   transitional copies. Verify a fresh clone includes the guidance before
   retiring the external directory.
