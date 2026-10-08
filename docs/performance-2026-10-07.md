# Performance regression investigation — 2026-10-07

Apple M5, release build, seed 12345, `--bench --rd 8`. The single-thread
volume has 392 chunks; 66 interior dense chunks are meshed. Values below
are medians of seven fresh-process runs, interleaved in a different order
for each round. Historical commits were built in the detached
`/tmp/vc-bisect` worktree; its source was unchanged and the worktree was
removed afterward. Saved binaries allowed measurement without rebuilding.
Other worktrees were building on the same host, so small differences
between unchanged headless paths should be treated as timing noise.

The initial unmodified `25c4c8a` run reproduced the reported regression:
0.387 ms/chunk generation, 1.329 ms/dense chunk light+mesh, 149,146 quads.
The repeat-run comparison below uses that saved binary as “before”.

## Before and after

| Phase | Before fix (`25c4c8a`) | After fix |
|---|---:|---:|
| Generation, ms/chunk | 0.353 | 0.226 |
| Light+mesh, ms/dense chunk | 1.420 | 0.725 |
| Light+mesh, ns/emitted quad | 628.378 | 320.827 |
| Streaming rd=8, seconds | 0.410 | 0.190 |
| Emitted quads | 149,146 | 149,146 |
| GPU bytes/dense chunk | 27,117 | 27,117 |

Generation is below the 0.25 ms target. The old `0c44148` median was
0.202 ms generation and 1.047 ms light+mesh, or 596.8 ns/quad.
The optimized 320.8 ns/quad is 46.2% cheaper than that old per-quad cost,
and also below the target calculated from the owner's faster 0.83 ms old
measurement (about 1.176 ms at the current quad count, including 10%).
Streaming retains 2,344 loaded chunks and 1,576 mesh uploads.

## Commit attribution

These checkpoints are a merge graph, not one linear series: materials
commits branch from `859ba11`; `c4809bb` combines them with mineshafts.

| Checkpoint | Change included | Generate ms/chunk | Light+mesh ms/dense | Quads |
|---|---|---:|---:|---:|
| `0c44148` | Old baseline | 0.202 | 1.047 | 115,781 |
| `fa3722e` | Music | 0.207 | 1.022 | 115,781 |
| `96a88ff` | Monster rooms | 0.241 | 0.971 | 115,781 |
| `85ca419` | Smithing | 0.259 | 0.978 | 115,781 |
| `39e596b` | Game rules | 0.245 | 0.976 | 115,781 |
| `a278351` | u16 block IDs / palette storage | 0.245 | 1.003 | 115,781 |
| `144151a` | Player model / cameras | 0.236 | 1.019 | 115,781 |
| `859ba11` | Particles | 0.244 | 0.988 | 115,781 |
| `01dbcee` | Mineshafts / poses | 0.250 | 1.082 | 144,791 |
| `d88abef` | New ore veins (materials branch) | 0.262 | 0.976 | 115,645 |
| `3a2d5d9` | Rock variants / deepslate | 0.323 | 1.014 | 115,990 |
| `6890a36` | Wood / stone shapes and below-neighbour support | 0.352 | 1.401 | 115,990 |
| `c4809bb` | Merge materials and mineshafts | 0.325 | 1.366 | 145,005 |
| `25c4c8a` | Java underground height mapping | 0.350 | 1.575 | 149,146 |

- Music, smithing, rules, player rendering and particles do not change this
  headless scene's geometry. The u16/palette checkpoint stays close to the
  preceding meshing cost; it is not the large regression.
- Monster rooms add generation/validation work (roughly 0.03–0.04 ms/chunk
  here). Their source columns already have a bounded cache. This seed has
  no additional room quads in the measured interior.
- Mineshafts add 29,010 quads compared with the preceding geometry: rooms,
  corridor supports, rails and cobwebs are real new geometry. The materials
  merge and height mapping bring the total to 149,146, another 4,355 quads.
  Separate stone/ore textures also prevent some greedy merges.
- `3a2d5d9` → `6890a36` keeps exactly 115,990 quads but increases meshing
  from 1.014 to 1.401 ms. `Block::emission()` called `base()` / `oriented()`
  on every lighting-region cell; those decoders now check both stone and
  wood forms. There are roughly 15.7 million such cells in this run. This is
  the clear per-block code regression. The new `below` read is confined to
  shaped cells and does not explain the volume-wide emission cost.
- Ore/material generation is real new work: on its separate branch,
  generation rises from 0.244 ms at `859ba11` to 0.323 ms at `3a2d5d9`.
  Feature Y-band and origin-reach pruning, cached dungeon/mineshaft starts,
  and stack-backed ore spheres are already present. The repeated surface
  noise offered a larger, safer saving without changing random streams.

## Targeted timers

Neither `samply` nor `cargo-instruments` was installed. Temporary `Instant`
timers accumulated each stage outside its inner loops. These are separate
seven-run medians, in milliseconds per generated chunk or per dense mesh
chunk respectively; medians need not add up to the whole-job median.
All instrumentation was removed from the committed code.

| Generation stage | Before | After |
|---|---:|---:|
| Surface columns / biome noise | 0.1350 | 0.0170 |
| Cave field and terrain fill | 0.0343 | 0.0344 |
| Deepslate paint | 0.0037 | 0.0037 |
| Ore veins | 0.1065 | 0.1051 |
| Trees and plants | 0.0049 | 0.0050 |
| Strongholds | 0.0000 | 0.0000 |
| Monster rooms | 0.0435 | 0.0433 |
| Mineshafts | 0.0007 | 0.0006 |
| Pack chunk storage | 0.0026 | 0.0026 |

| Light+mesh stage | Before | After |
|---|---:|---:|
| Decode neighbourhood into flat region | 0.0428 | 0.0416 |
| Skylight initialization / propagation | 0.0920 | 0.0905 |
| Emission scan / block-light propagation | 0.7300 | 0.0845 |
| Slab / stairs light borrowing | 0.0584 | 0.0573 |
| Greedy and detail meshing | 0.3938 | 0.3944 |

## Changes and output verification

`src/world/block.rs` stores emission in a 4,096-byte state-indexed table,
including every lit furnace facing, fire age, lava level and portal-frame
state. An exhaustive test compares every state with the original
base/orientation implementation. `info()`, opacity and light borrowing were
already indexed loads; neighbourhood storage was already decoded once
into `Region`, so those paths remain unchanged.

`src/world/terrain.rs` reuses immutable height/biome columns across the eight
vertical chunks. The mutex protects only cache lookup/insertion; noise runs
outside it. The cache holds at most 512 columns, about 4 MiB plus headers.
Concurrent misses may calculate the same column twice, but cannot change
its result. Eviction retains any in-flight `Arc` snapshot.

`generated_chunk_hashes_stay_identical` pins FNV-1a hashes captured before
optimization over 395 chunks per case (the benchmark volume plus three
distant chunks), 1,580 chunks total:

| Dimension / seed | Golden block-ID hash |
|---|---|
| Overworld / 12345 | `a0c9372471787228` |
| Overworld / 99 | `337f1929ad1749ff` |
| Nether / 12345 | `17a01163881c778d` |
| End / 12345 | `c5aa25494a635b2b` |

All hashes pass before and after. Quad count and GPU bytes are unchanged.
Screenshot comparisons used the same seed, fixed poses and paused scenes:

- `target/perf/before-paused.png` / `after-paused.png`: surface terrain.
  The static terrain rectangle `(0,200)..(400,900)` has zero changed pixels.
- `target/perf/before-lights.png` / `after-lights.png`: stone platform at
  y=150 with glowstone, torch, two lit furnace facings, enchanting table,
  portal frame and material variants. The floor `(0,500)..(1600,740)` and
  wall `(540,140)..(1060,290)` have zero changed pixels.
- Whole-frame differences are animated water, twinkling stars, FPS and HUD
  animation. Static-region diffs are saved as `target/perf/diff-*-static.png`.

Both screenshots and diff images were inspected. The production mesh,
physics, structure placement, ore random streams and Java height map are
unchanged. The existing compressed height map and simplified structure
algorithms remain the same; this patch introduces no parity changes.

The full gate passed: format; clippy with `-D warnings` for default and
headless features; 471 default-feature tests and 400 headless tests, with
one existing ignored client test. Raw runs, binaries, timing scripts and
screenshots remain under `target/perf/` (untracked).

Java reference: [BlockBehaviour.BlockStateBase](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/state/BlockBehaviour.java)
precomputes `lightEmission` in the state constructor and returns it directly.
[NoiseBasedChunkGenerator](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/levelgen/NoiseBasedChunkGenerator.java)
reuses chunk noise and climate sampling. These support caching immutable
state/noise results; world behavior remains pinned to the pre-fix hashes.
