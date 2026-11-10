# Streaming biome queries and the GEN-07 corpus — 2026-11-09

[Roadmap gate](../roadmap.md#stabilisation-gate-before-v017) · [Requirements](../requirements.md) · [Previous profile](2026-11-09-entity-frame-time.md)

The owner approved this stabilisation scope on 2026-11-09: remove terrain
column recomputation from streaming gameplay queries without changing spawning,
extend GEN-07 to ancient cities and trial chambers, and gather Terra runtime
and M5 evidence. No v0.17 content or release tag is part of this change.
PERF-02, PERF-03 and GEN-07 remain Partial; the release gate has not passed.

## Implementation

The generation workers' 512-entry column cache holds density fields as well
as heights. Structure searches and streaming can evict a loaded column. A
subsequent `MobWorld::cave_biome` query used to reconstruct all 32 × 32 density
columns on the simulation thread to recover one height.

The existing foliage worker job now also returns the original, unadapted
Overworld terrain heights. `World` retains them with the loaded column and
releases them when its last chunk unloads: 2 KiB per loaded Overworld column,
with no extra allocation for the Nether or End. Gameplay biome and snow-golem
temperature queries use that height with the same climate sample, cave-depth
rules and biome selection as the generator. Modified terrain heightmaps are
not used: digging or building must not move biome boundaries. Pending worker
results and unloaded positions keep the old generator query, so frontier
queries cannot change spawning decisions. Spawning RNG, attempts, caps,
structure overrides and generated terrain are unchanged. Nothing new is saved.

Regression coverage compares the actual worker-to-world path with generator
queries in generations 1, 2 and 3, at negative coordinates, chunk edges and
lush, dripstone and deep-dark depth boundaries. It replaces the loaded surface,
clears the density cache and checks both biome equality and that gameplay
does not repopulate that cache. Unload releases the heights and restores the
fallback. Nether and End queries retain their previous path.

## Ancient cities and trial chambers

`src/world/terrain/order_tests.rs` now includes negative-coordinate ancient
cities (162 chunks) and trial chambers (30 chunks) crossing storage chunk
boundaries. The full generation-3 corpus has 984 chunks, compared block for
block and feature for feature in row, shuffled, parallel and cache-evicting
orders. Both structure caches participate in eviction. Assertions require
ordinary and ice-box city chests, trial loot, spawners and both normal and
ominous vault blocks, so empty structure coverage cannot pass.

The isolated implementation's cloud run at `e959a540` passed the complete
default/headless chain and the seven focused GEN-07 tests. The generation-3
four-order run took 26.73 s; the focused suite took 37.44 s. Generations 1
and 2 retain their golden outputs. This is one seed and simulated cache
eviction, not proof of Java layout parity or looted-container persistence.

## M5 comparison

GPT-6.1 Terra built the clean baseline `2c8c173e` and changed build `ceeb7d7`
offline with locked dependencies, cleaning only the crate and regenerating
build-script asset paths for each. The changed runtime fingerprint is
`1b2676f4cbcdaebfc7654c091613208bd43068981e5ee08c8f99f1a163461976`.
Later test-constructor fixes affect test code only, not the measured gameplay.

Three streaming repeats per build, 10 s warm-up and 30 s sampled, 1920 × 1080,
256-block view, 160-block simulation distance, enhanced graphics, offscreen.
The Apple M5 was on AC power and unlocked; other desktop workloads were
active. **Every run is ineligible for acceptance.** The idle-machine
condition could not be established, so no windowed gate or eight-scene suite
was attempted. Results and per-frame samples:
[baseline summary](../../benchmarks/results/2026-11-09-2c8c173-m5-streaming-offscreen/summary.json),
[baseline frames](../../benchmarks/results/2026-11-09-2c8c173-m5-streaming-offscreen/frame-times.json.gz),
[changed summary](../../benchmarks/results/2026-11-09-ceeb7d7-m5-streaming-offscreen/summary.json),
[changed frames](../../benchmarks/results/2026-11-09-ceeb7d7-m5-streaming-offscreen/frame-times.json.gz).

| Build / repeat | Average FPS | 1% low FPS | p99 ms | Worst ms | Frames >50 ms | Ticks | Mean tick phase in slowest 1%, ms |
|---|---:|---:|---:|---:|---:|---:|---:|
| Baseline 1 | 471.6 | 55.0 | 11.611 | 596.402 | 2 | 593 | 13.293 |
| Baseline 2 | 493.3 | 55.3 | 11.930 | 603.193 | 2 | 593 | 13.224 |
| Baseline 3 | 484.2 | 69.5 | 12.405 | 22.031 | 0 | 600 | 13.526 |
| Changed 1 | 562.4 | 118.1 | 3.635 | 599.480 | 1 | 593 | 3.046 |
| Changed 2 | 544.2 | 196.2 | 4.157 | 14.819 | 0 | 600 | 2.722 |
| Changed 3 | 568.9 | 200.3 | 3.683 | 14.445 | 0 | 600 | 2.980 |

The short comparison shows lower p99 and tick cost, not a passing performance
gate. Baseline repeats 1/2 and changed repeat 1 also missed tick-count
tolerance (600 expected). Both builds have unexplained roughly 600 ms
outliers; their cause was not established. The 1% lows remain below 240 FPS.
Valid, idle, windowed three-repeat scene runs are still owed.

## Runtime inspection

GPT-6.1 Terra wrapped the changed executable with `scripts/test-app.sh` and
used a separate ignored data directory. Through native computer use it created
a survival world, moved to the shoreline, opened inventory and pause menus,
saved, quit and reopened at the same shoreline. It reopened again and held
forward for 25 s into the ocean; underwater terrain was visible and health
fell. This was limited agent play, not the owner's acceptance or a spawning
test.

Terra also visually inspected generation-3 seed-42 scripted captures: the
ancient city at `136,-38,186,-45,-15` showed the reinforced frame, sculk,
walls and chests; the trial chamber at `265,-30,105,-45,-15` showed tuff
rooms, copper lights and spawner/vault blocks. Logs showed no errors or
panics. No definite new gameplay bug was identified. Local captures, logs,
machine-state evidence, isolated saves and binaries remain under ignored
`target/stabilise-runtime/`.

Not verified: biome-dependent spawning through ordinary play, structure
encounters, the rest of the v0.14–v0.16 hands-on scope, owner play, or the
performance gate. This change supplies regression and comparison evidence
without promoting those acceptance claims.
