# World, lighting and structure defect fixes — 2026-10-13

[Defect inventory](2026-10-13-v0.7-defect-inventory.md) · [Lighting and spawning](2026-10-13-lighting-spawning.md)

Branch `work/fix-w5-world` (worker 5 of the v0.7.0 defect fixes). Reference:
official Java 26.3 server, SHA-1 `33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`.
Ordinary-player acceptance is outstanding for the behavior recorded here. Each section
names the Java source read (classes were disassembled with `javap`; data JSON read
directly), what changed, and what stays different.

## Growth uses the gameplay light arrays

Java sources: `CropBlock.randomTick` and `hasSufficientLight` (`getRawBrightness(pos, 0)`),
`SaplingBlock.randomTick` (`getMaxLocalRawBrightness(pos.above())`),
`SweetBerryBushBlock.randomTick` and `BambooStalkBlock.randomTick`
(`getRawBrightness(pos.above(), 0) >= 9`), `SpreadingSnowyBlock.randomTick`,
`canStayAlive` and `canPropagate` (grass and mycelium).

- Crops: stored skylight or block light of 9 in their own cell, at any hour. Open
  sky counts at night; a roof leaves only light that propagated under it.
- Saplings: the cell above, with the time-of-day and weather `skyDarken` subtracted,
  so they stop growing at night and in storms but keep growing under lamps.
- Sweet berries now test the cell above (they tested their own); bamboo already did.
- Grass and mycelium share Java's spreader: covered (light-opaque or fluid above)
  turns to dirt; otherwise brightness 9 above is required (night and storms stop
  spreading) before four attempts at dirt in x/z ±1, y -3..+1 that could stay alive
  and have no water above. Mycelium previously ignored light and never died.
- Cells whose skylight is unknown (borders of partly loaded columns) fall back to
  block light only.

Not changed: Java also uproots crops whose light drops below 8 (`CropBlock.canSurvive`);
VoxelCraft does not model that survival check. Snow layers (1-layer snow keeps
grass alive) are not modelled because VoxelCraft has no layered snow.
Mushroom survival (a separate inventory item) still uses the heightmap approximation.

Tests: `world::growth::tests::{covered_crops_need_nine_block_light_and_saplings_sample_above,
crops_read_stored_skylight_at_night_and_saplings_the_darkened_value,
berries_and_bamboo_test_the_cell_above_with_stored_skylight,
grass_spreads_only_with_brightness_nine_above_and_dies_when_covered}`.

## Igloo basement depth

`IglooPieces.addPieces` (bytecode): with `nextDouble() < 0.5`, `depth = nextInt(8) + 4`
three-block ladder pieces are stacked above the laboratory, so its floor is
`depth * 3` blocks below the igloo floor (12 to 33). The generator now draws that depth;
existing seeds' basement igloos change depth (7 in 8 become deeper), nothing else does.
Test: `world::temples::tests::igloo_basement_depth_follows_java_piece_stacking`.
The bed orientation half of the inventory item is not fixable without bed facing
states (new block IDs and models), so it is left open.

## Smaller items

- **Peaceful** (commit `95781fd`): see [nether mobs](../nether-mobs.md). Java 26.3
  discards piglin brutes, hoglins and zoglins (`notInPeaceful()`), not piglins.
- **`/locate biome`** (Nether): already equivalent to Java's 3D search because the Nether
  noise router sets `y_scale` 0 for temperature and vegetation. Pinned in
  `lighting_spawning.json` (`nether_climate`) with a regression test.
- **Metal GPU memory**: see [bench fixes](2026-10-13-review-bench-fixes.md#metal-memory-telemetry-v070-fix).

## Entity and particle skylight

`entity::sky_light` (used for mobs, items, the hand and particles) now returns the
propagated skylight of the cell; the previous heightmap estimate remains as the fallback
where `World::sky_light` is `None`. Particle docs updated.

## Nether spawn costs

Java sources: biome `natural_mob_spawns` `spawn_costs` (soul sand valley: enderman,
ghast, skeleton, strider at charge 0.7, budget 0.15; warped forest: enderman at 1.0,
0.12; every other biome none), `NaturalSpawner$SpawnState.canSpawn/afterSpawn`,
`PotentialCalculator.getPotentialEnergyChange` (sum of `charge / sqrt(distSqr)`, infinite at
distance zero). See [Nether biomes](../nether-biomes.md#spawn-costs). Test:
`entity::tests::spawn_costs_space_out_soul_sand_valley_and_warped_forest_mobs`.

Spawn-cost values are now independently extracted into `world_defects.json`;
`spawn_cost_tables_match_java_26_3` covers all implemented Nether mob types
in all five biomes. Fortress pack members apply the same check as leaders.

## Day and weather timing

Java sources: `ServerLevel.advanceWeatherCycle`, `RAIN_DELAY` (12000-180000 ticks),
`RAIN_DURATION` (12000-24000), `THUNDER_DELAY` (12000-180000), `THUNDER_DURATION` (3600-15600),
levels moving 0.01 per tick; the day timeline is 24000 ticks.

- `DAY_LENGTH` is 1200 s (20 minutes). The saved `time` is the day *fraction*, so old
  worlds keep their time of day with no migration; saved weather timers are seconds and
  stay valid.
- Rain and thunder cycle on independent timers with the ranges above (the old model was
  one toggle scaled by the ten-minute day); fades take 5 s instead of 10. The level file
  entry gains a fourth field (thunder seconds); older entries get a fresh thunder spell.
- Not modelled: `/weather` durations, the `advance_weather` gamerule, and `clearWeatherTime`.

Tests: `simulation::weather::tests::{spells_alternate_and_fade,
durations_and_fades_follow_the_reference_tick_ranges, rain_and_thunder_cycle_on_independent_timers}`.

Spell durations are sampled in inclusive integer ticks. Timer expiration flips
the corresponding flag; its next duration is sampled on the next update, as in
`ServerLevel.advanceWeatherCycle`. Weather timers retain their public seconds
interface and are quantized to ticks when advanced.

## Ambient light in the Nether and End

Sources: the dimension type JSON (`ambient_light` 0.1 Nether, 0.25 End; environment
`visual/ambient_light_color` `#0a0a0a`, `#302821`, `#3f473f`; `visual/sky_light_factor`
1, 0, 0), pinned in `lighting_spawning.json`. The lightmap shader is client-only and is
not in the server jar, so the way the colour combines with sky and block light
follows the documented client behavior (ambient colour added to the sky and block
terms, clamped), not a disassembly.

- The chunk, block-model, entity and particle shaders take `visual/ambient_light_color`
  through a new `Globals.ambient` uniform (w = skyless dimension). In the Nether and End the
  light is `clamp(ambient + sky + block)`. The Overworld keeps its existing path.
- The End and Nether sky factor is 0 (was a 0.65 / 0.42 stand-in "daylight"), so the End's
  sky-lit islands are no longer rendered as 65% sunlight.
- Not changed: gameplay skylight propagation (the End still propagates skylight).

**Unverified visually**: `--screenshot` smoke runs in the Nether and End compiled the shaders and
rendered plausible, non-black terrain, but the reference
brightness of Nether and End terrain still needs a side-by-side look.
Test: `world::terrain::tests::dimension_light_attributes_match_the_reference_dimension_types`.
