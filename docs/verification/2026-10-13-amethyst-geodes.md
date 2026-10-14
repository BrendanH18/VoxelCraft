# Amethyst geodes — 2026-10-13

[Roadmap](../roadmap.md) · [Architecture ID notes](../architecture.md#assessment-branch-id-reservations) · [Reference fixtures](../../tests/fixtures/java/26.3/README.md)

Scope: GEN-02 (geodes) and the bud states BLD-03/BLD-04 need for them, based on
`7b049db` on `work/v0.7-assessment`. Implementation and this record accompany
the same commit (branch `work/geodes`).
Reference: official Java Edition 26.3 server SHA-1
`33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`; tests ran on the M5 MacBook Pro.
State: **Implemented, unverified**. Geodes generate with the reference
parameters and algorithm. Java-seed coordinate parity, ordinary player
acceptance and representative performance acceptance are still open.

## Reference source

Extracted from the pinned artifact's inner `server-26.3.jar` (kept under
ignored `target/`, never committed):

- `data/minecraft/worldgen/feature/amethyst_geode.json`: filling air, inner
  amethyst block, alternate inner budding amethyst, middle calcite, outer
  smooth basalt; inner placements small/medium/large bud and cluster;
  `cannot_replace` `#features_cannot_replace`, `invalid_blocks`
  `#geode_invalid_blocks`; crack chance 0.95; outer wall distance uniform
  4–6; `use_alternate_layer0_chance` 0.083; `invalid_blocks_threshold` 1;
  empty `layers`.
- Codec defaults found in the `GeodeFeature`, `GeodeLayerSettings` and
  `GeodeCrackSettings` bytecode: layers 1.7/2.2/3.2/4.2;
  base crack size 2.0; crack point offset 2; distribution points uniform 3–4;
  point offset uniform 1–2; min/max gen offset -16/16; noise multiplier
  0.05; `use_potential_placements_chance` 0.35;
  `placements_require_layer0_alternate` true. The noise is
  `NormalNoise.createParity(-4, [1.0])`, seeded from the world seed.
- `GeodeFeature.place` bytecode: distribution-point validity (air or an
  invalid block counts; more than the threshold rejects), the crack-point
  patterns (`nextInt(4)`, offsets `points*2+1` and y 7/5/1), the per-point
  noise term, threshold order (filling, crack, inner, middle, outer),
  `safeSetBlock`, and bud growth on the first face in `Direction.values()`
  order whose neighbour passes `BuddingAmethystBlock.canClusterGrowAtState`.
- `worldgen/placed_feature/amethyst_geode.json`: `rarity_filter` 24,
  `in_square`, `height_range` uniform `above_bottom` 6 to absolute 30,
  `biome`. Every Overworld biome (including the cave biomes) lists the
  feature in its `local_modifications` step; no Nether/End biome does.
- Tags: `geode_invalid_blocks` = bedrock, water, lava, ice, packed ice, blue
  ice; `features_cannot_replace` = bedrock, spawner, chest, end portal frame,
  reinforced deepslate, trial spawner, vault.

## Delivered behavior

- `src/world/geode.rs` implements the placed and configured feature with the
  values above. Each Java 16×16 chunk rolls the rarity filter; candidate
  geodes are laid out once against pristine terrain
  (`Generator::natural_block` plus the exact bedrock pattern), cached per
  32-block column and clipped into every chunk they touch, in a fixed origin
  order. Load order cannot cut or change a geode.
- Geodes paint after carvers/aquifers and before ores, cave vegetation,
  monster rooms and structures, matching Java's step order. Ores replace
  only stone/deepslate, so they never overwrite geode blocks.
- Buds had only up/down states. Sixteen wall states (2400–2415, south/north/
  east/west × small/medium/large/cluster) were appended and reserved in the
  architecture notes. They share the upright bud's item, hardness, light
  (1/2/4/5), Silk Touch/shard drops and glass sounds, need a sturdy block
  behind them (dropping when it is removed), are placed on wall faces, and
  use Java's outline boxes for collision/targeting.
- Budding amethyst random ticks now follow `BuddingAmethystBlock`: one tick
  in five, one of six random faces starts or grows the bud facing that way
  (previously only up/down).

## Tests and results

New tests (`world::geode::tests`, `world::overworld_blocks::tests`):

- Candidate rate across 9,216 Java chunks is within 0.01 of 1/24; every
  origin lies in y −58..=30; every write stays inside the ±16 box; more than a
  third of candidates survive the validity checks.
- Rays from each air core through crack-free directions pass air → amethyst →
  calcite (→ smooth basalt) without reversing; at least 80% reach basalt;
  amethyst–basalt face contacts are under 5% of amethyst–calcite contacts.
  (Layer thickness is under one block at Java's thresholds, so occasional
  basalt gaps are a property of the reference algorithm, not a defect.)
- Every generated bud has budding amethyst directly behind it; all six
  facings and all four sizes occur. The budding share of the inner layer
  is within 0.015 of 0.083.
- Geodes with open distribution points are rejected.
- Two fresh generators produce identical chunks for every chunk a geode spans
  when generated in forward and reverse order; at least 90% of the layout's
  cells appear in the generated chunks; rebuilding the layout reproduces it.
- Bud states round-trip their six facings, base item, light, hardness, drops,
  wall support, box geometry and wall placement; all new states survive the
  chunk save/reload test.
- The Overworld golden-hash test was re-captured. With geode painting
  disabled the old hashes still match, so the bedrock helper refactor
  changed nothing else.

`cargo fmt`, strict release Clippy and release tests passed in desktop and
headless configurations. Desktop: 748 library + 81 executable + 72
integration tests passed, three ignored. Headless: 747 library + 72
integration tests passed, two ignored.

Benchmark (`--bench --rd 8`, same machine, base `7b049db` binary interleaved
with this branch, other agents running concurrently):

| Run | Base generate ms/chunk | Geodes generate ms/chunk | Base stream s | Geodes stream s |
| --- | --- | --- | --- | --- |
| 1 | 0.463 | 0.456 | 0.47 | 0.51 |
| 2 | 0.444 | 0.463 | 0.56 | 0.50 |
| 3 | 0.440 | 0.717* | 0.64 | 0.52 |
| 4 | 0.478 | 0.464 | 0.59 | 0.51 |

\*Run 3 also slowed the untouched Nether generator by 50% (0.319 vs 0.21
ms/chunk), so it is external load. Excluding it, the medians are 0.454 vs
0.463 ms/chunk (about +2%, within run-to-run noise); streaming is unchanged.
A far-cell bound skips noise samples that cannot reach the outer threshold
without changing results (the normalised noise stays below 2; tested).

## Remaining differences from Java

- RNG and noise: decoration seeds are this engine's hashed per-chunk
  splitmix stream, not Java's xoroshiro `WorldgenRandom` with feature-index
  seeds; the noise is this engine's improved-Perlin pair with Java's
  frequency, second-sampler scale and normalisation, not Java's seeded
  `NormalNoise`. A Java seed does not reproduce Java's geode positions or
  shapes. Terrain itself also differs (GEN-01), so bit parity is not
  achievable in this scope.
- Validity and bud checks read pristine terrain: ravines and aquifer-placed
  fluids are not modelled there (cave air from noise caves and lava below
  the lava level are). Java reads the live world, including other features.
- Overlapping geodes: each is laid out against terrain, and overlaps paint in
  origin order. Java's result depends on its chunk decoration order.
- Buds are not waterlogged (no bud waterlogging states exist).
- Wall buds render as their Java outline box textured with the bud sprite
  rather than rotated crossed planes; upright/hanging buds keep crosses and
  have no collision box.
- `features_cannot_replace` members that do not exist yet (reinforced
  deepslate, trial spawner, vault) are absent from the check.
- No ordinary-player exploration, visual capture review or representative
  frame-time scene has been run.

## v0.7.0 defect fix: crack fluid ticks

`GeodeFeature.place` bytecode: after each crack-air `safeSetBlock`, every
`Direction` neighbour with a non-empty fluid state gets
`scheduleTick(pos, fluid, 0)`. Each laid-out geode now records those fluid cells
(`Geode::fluid_ticks`, read from the layout or pristine terrain at the moment the
air is placed). `World::insert_chunk` wakes them when a freshly generated
Overworld chunk is installed, so water and lava beside a crack pour in without a
player edit. Tests: `world::geode::tests::crack_air_beside_fluid_schedules_fluid_ticks`
and `world::tests::installing_a_chunk_wakes_fluid_beside_geode_cracks`.

Still open from this record (not fixed in v0.7.0): Java xoroshiro decoration
seeding/`NormalNoise` and live-world terrain reads (the same chunk-pipeline
rewrite as the other decoration RNG gaps), chunk-decoration overlap order, and
rotated crossed-plane wall bud models (the cross quads are fixed-geometry shader
faces; wall buds need new shader faces that could not be reviewed visually).
