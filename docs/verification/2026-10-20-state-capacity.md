# Block-state capacity raise — 2026-10-20

[Roadmap](../roadmap.md#release-plan) · [Requirement inventory](../requirements.md) · [M0 capacity assessment](2026-10-17-registry-capacity.md) · [ID reservations](../architecture.md#release-id-reservations-v09v017)

Scope: BLD-06's encoding change at the start of v0.9, on
`t3/continue-build-brief` after `5e9f025`. The owner chose (2026-10-20) to
raise `STATE_CAPACITY` and keep one ID per Java state, waterlogged states
included, rather than store waterlogging in a separate chunk layer. That was
option (a) of the [assessment](2026-10-17-registry-capacity.md).

## Change

- `world::block::STATE_CAPACITY` rises from 4096 to 16,384. Block items
  still fit: `BLOCK_ITEM_BASE` (0x8000) + 16,384 = 0xC000, within `u16`, and
  the compile-time assert in `item.rs` holds.
- Chunk files keep the `VXC2` header while every saved state is below 4096
  and switch to `VXC3` once one is not. The records are unchanged (`u16` run,
  `u16` state). `VXC2` and `VXC1` files load as before. Builds from before
  the raise refuse `VXC3` files, and still open worlds that use no new states.
- Saving over a `chunks.bin` whose header this build doesn't know (a newer
  build's) first renames it to `chunks.bin.unknown-<unix seconds>`. Loading
  such a file fails and play continues on regenerated terrain; until now the
  first save then destroyed it. Runs that never save (benchmarks and
  screenshots, which may load a world without its lock) leave it alone.
  Builds before this change still overwrite a `VXC3` file, so don't open a
  world that uses v0.9 states in one.
- The LAN `PROTOCOL` goes from 1 to 2, so a host and client built either side
  of the raise refuse each other at the handshake.
- The test-only registry entries for the high texture-layer meshing test
  move from 4093–4095 to 16,381–16,383, freeing the v0.10 range. The
  automation save parser and the LAN out-of-registry test now use
  `STATE_CAPACITY` instead of a literal 4096.
- ID ranges for v0.9–v0.17 are reserved in the
  [architecture document](../architecture.md#release-id-reservations-v09v017).

## Checks

- `cargo fmt`, `cargo clippy --release --all-targets -- -D warnings` and
  `cargo test --release` pass. So do the headless (`--no-default-features`)
  clippy and tests (1,282); the desktop tests number 1,374.
- New storage cases: a state at `STATE_CAPACITY - 1` round-trips through a
  `VXC3` file; a save whose states are all below 4096 is written as `VXC2`;
  a `VXC2` file holding 4095 loads unchanged; state 16,384 is rejected as
  outside the registry; loading a `VXC9` file leaves it untouched, and the
  next save keeps it intact as `chunks.bin.unknown-…` (once only) before
  writing its own file.
- Headless `--bench --rd 8`, alternating pre-change (`5e9f025`) and changed
  builds three times each. The differences are within run-to-run noise:

| Measure | Before (mean of 3) | After (mean of 3) |
|---|---|---|
| Generation, 1 thread | 0.465 ms/chunk | 0.478 ms/chunk |
| Gameplay light initialisation | 53.3 ms | 51.4 ms |
| Snapshot + mesh, 1 thread | 0.646 ms/chunk | 0.653 ms/chunk |
| Stream rd 8 on 9 workers | 0.51 s | 0.51 s |
| Palette pack | 11.9 µs/chunk | 12.7 µs/chunk (individual runs 10.5–13.9 overlap) |

Not yet checked: the windowed scene suite with the larger static tables.
Run it as part of the next streaming re-run.
