# Streaming stalls — 2026-10-17

[Roadmap](../roadmap.md) · [M5 baseline](2026-10-16-m5-baseline.md) · [Requirement inventory](../requirements.md)

Scope: the periodic streaming stalls found by the
[2026-10-16 M5 baseline](2026-10-16-m5-baseline.md) (bursts of about eight
frames over 50 ms every 32 blocks travelled), on `t3/roadmap-next-stage-v0-8`
after `2b17c34`. Measured on the M5 MacBook Pro with a release build carrying
line tables (`CARGO_PROFILE_RELEASE_DEBUG=line-tables-only`).

## Cause

`sample` attached for 30 s to the streaming scene (seed 12345, rd 8,
1920 × 1080, from 55 s into the run) put 6,214 of the main thread's 16,424
samples in `World::update_players`, 5,786 of them under
`World::insert_chunk` → `Generator::structure_features` →
`Dungeons::features` → `Dungeons::near`, which ran `natural_block` and cave
carving to lay out monster rooms. A further 351 were geode fluid ticks.

Workers cache each 32-block column's dungeon rooms when they paint a chunk.
A chunk above the terrain returns from generation at once, as uniform air,
without painting, so when a new column's first arrival was such a chunk the
main thread laid out the rooms of up to nine columns itself, once per row of
new columns.

## Change

Workers now build a `GeneratedChunk` (`src/world/mod.rs`): the generated data
plus the generator-only follow-up that `insert_chunk` used to do on the main
thread (surface decoration, `structure_features` and geode fluid ticks). The
main thread only registers the block entities whose blocks are present. The
results are the same functions of seed and chunk position, in the same
order, so generated worlds and their block entities are unchanged. Saved
(modified) chunks take the old path.

## Results

| Run | Main-thread samples in `update_players` | Frames > 50 ms | Notes |
|---|---|---|---|
| Before, 30 s sampled | 6,214 of 16,424 | — | `sample` attached |
| After, 30 s sampled | 254 of 15,517 | — | `sample` attached; the rest is mostly `acquire_texture` |
| After, 120 s unsampled | — | 1 (worst 910 ms) | **Invalid run**: 8,276 unpresented frames, the window was not frontmost |

The baseline had 451–453 frames over 50 ms per 180 s streaming run. The
unsampled run above is not acceptance evidence: its window was covered, so
presentation was throttled (p50 9.9 ms), and the single 910 ms frame is not
yet explained. `cargo fmt`, `cargo clippy --release --all-targets -- -D warnings`
and `cargo test --release` pass (1,192 tests), as do the headless
(`--no-default-features`) clippy and tests (1,104).

## Remaining

- A valid streaming re-run on the reference machine (idle, window visible,
  `caffeinate -dis python3 tools/benchmark_scenes.py --scenes streaming`),
  compared with the baseline, and a check of whether the 910 ms frame recurs.
- Streaming and hitch budgets (§6) are still unset, so the scene cannot be
  judged against one yet.
