# Lighting and spawning review fixes — 2026-10-13

Follow-up to the [gameplay skylight and cave spawning record](2026-10-13-lighting-spawning.md),
from the review of that work against the pinned Java 26.3 server artifact
(SHA-1 `33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`). Nothing here is Verified;
ordinary-player acceptance is still open.

## Fixes

- **Spawn-point exclusion.** `NaturalSpawner.isRightDistanceToPlayerAndSpawnPoint`
  rejects positions within 24 blocks of the respawn block centre in the respawn
  dimension. The Overworld monster cycle and legacy natural paths apply it; the
  extractor reads the constant from bytecode.
- **Thunder and rain thresholds.** `Level.isThundering` is
  `getThunderLevel(1) > 0.9` (the thunder level is scaled by the rain level);
  `isRaining` is `getRainLevel(1) > 0.2`. Weather fades a separate storm level;
  only the Overworld applies clock and weather darkening.
- **Daylight detectors.** Detectors read stored skylight minus the shared
  `skyDarken`, scaled by `Mth.cos` of the `day.json` sun angle pulled 20% toward
  noon, using the reference sine table. Dimensions without the day timeline use
  angle 0: End detectors see their sky and inverted Nether detectors output 15.
- **Spawn start cell.** `NaturalSpawner.spawnCategoryForPosition` returns when the
  start cell `isRedstoneConductor`. The predicate's default is
  `BlockStateBase.isCollisionShapeFullBlock`; `Blocks` overrides it at 26 call
  sites with `never` (glass, ice, leaves, pistons, observers, TNT and others,
  plus the stained glass, copper grate and copper bulb families) or `always`
  (soul sand, mud). The previous check used "opaque and full collision", which
  wrongly let soul sand and mud start packs and wrongly stopped them in a few
  non-conducting full blocks.
- **LAN replica skylight.** A replica used to discard and recompute the whole
  column's skylight for every host edit packet. Batches of up to 64 cells now go
  through the same light, heightmap and remesh bookkeeping as a local edit,
  without simulation side effects; larger batches keep the snapshot relight.
  `replica_edits_relight_incrementally_like_a_fresh_snapshot` checks that a
  100-cell roof with a glass gap and a torch, then a reopened cell, give the
  same sky and block light as installing the final chunk fresh.
- **Extractor claims.** The extractor docstring now separates data-JSON facts,
  bytecode-checked facts (instruction forms and constants it requires) and
  hand-reproduced facts (rule strings, family expansions, the float arithmetic
  in sky-darkening and detector cases, and the scene model). The bytecode checks
  confirm the listed constants and call shapes exist; they do not decompile or
  execute the Java methods. The previous record's phrase "checks relevant
  bytecode forms" should be read with that limit.

The extractor now also records each dimension's `visual/sky_light_factor`
(Overworld 1.0, Nether and End 0.0) and the conductor overrides. Regenerating
the fixture from the pinned artifact is byte-identical.

## Measured skylight cost

`cargo test --release --lib skylight_cost -- --ignored --nocapture`, a 5x5
flat-column replica (stone to y 63) on the development Mac, median of 15 runs:

| Work | Median |
|---|---|
| Whole-column relight from a snapshot | 0.110 ms |
| 1 roof cell: snapshot / incremental | 0.099 / 0.005 ms |
| 4 cells | 0.073 / 0.008 ms |
| 16 cells | 0.075 / 0.022 ms |
| 64 cells | 0.092 / 0.066 ms |
| 256 cells | 0.128 / 0.228 ms |
| 1024 cells | 0.259 / 0.927 ms |

The crossover near 100 cells sets the 64-cell limit. Flat columns are the
cheapest case for a snapshot relight (uniform chunks), so real terrain should
favor the incremental path more. These are microbenchmarks, not frame-time
evidence; representative scene measurement waits for a valid M5 baseline.

## Remaining differences

- **End visual sky.** The End's `visual/sky_light_factor` is 0.0, but it has
  `ambient_light` 0.25 and `visual/ambient_light_color` `#3f473f`. The renderer
  approximates End lighting as skylight times 0.65 because the shaders have no
  ambient term; setting the factor alone would render the islands lit only by
  block light. A faithful change needs an ambient lightmap term (Nether
  `ambient_light` 0.1 as well) and visual review.
- **Collision shapes.** Soul sand and mud collide with Java's 14/16-high column
  (pinned in `tests/fixtures/java/26.3/building.json`; soul sand already used it
  and mud now does, as tested through the physics step). Spawn conduction
  follows the registered overrides, so it does not affect the start-cell check.
- Conductor overrides naming blocks VoxelCraft has not registered are skipped.

## Validation

`cargo fmt`, strict release Clippy and release tests: **958 passed, zero failed,
four ignored** (the timing measurement is one). Strict headless Clippy and tests:
**874 passed, zero failed, three ignored**. The desktop LAN regression harness
passed **13/13** on one Mac; it exercises host/client edits but does not isolate
the incremental relight path, which the unit test above covers.

## Proposed roadmap updates

Applied in the same change: the PLY/MOB lighting notes record these fixes and
remaining differences; the gameplay-skylight entry links this report.
