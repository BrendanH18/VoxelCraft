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

## Slime and drowned spawn rules

Java sources (`javap` of `Slime`, `Drowned`, `SpawnPlacementTypes`, `NaturalSpawner`):

- Slimes: `ON_GROUND` placement, then `checkSlimeSpawnRules`, which has *no*
  `isDarkEnoughToSpawn` test. A swamp or mangrove swamp cell with `50 < y < 70` passes
  when `nextFloat() < surface_slime_spawn_chance` (0.5 at a full moon, 0.375, 0.25,
  0.125, 0 at a new moon) and raw brightness (sky minus `skyDarken`, or block light)
  is at most `nextInt(8)`. Otherwise a slime chunk (the existing Java seed function)
  at `y < 40` passes one attempt in ten.
- Drowned: `IN_WATER` placement (water in the cell, no redstone conductor above),
  then `checkDrownedSpawnRules`: water below too, `isDarkEnoughToSpawn` (the same
  monster light roll), and `nextInt(15) == 0` in river biomes or
  `nextInt(40) == 0` below sea level - 5 elsewhere. `checkSpawnObstruction` is only
  `isUnobstructed`, so the spawn box may be wet.
- Both walk the biome list's packs (slime weight 100 in four, the swamp's extra weight-1
  single slime, drowned weights per biome) through the ordinary cycle; before, they
  rolled position, light and rarity independently outside it.

Not changed: zombie horses, strays, bogged, parched and ocelots are absent mobs, so
their list entries still end a pack (adding them is new content). The 24-64 block ring,
per-kind caps and structure overrides are unchanged.

Tests: `entity::monster_spawns::tests::{swamp_slimes_follow_the_moon_phase_and_light_roll,
slime_chunks_spawn_slimes_below_y_40_one_try_in_ten,
drowned_spawn_in_water_one_try_in_40_when_deep_and_one_in_15_in_rivers,
slimes_and_drowned_belong_to_the_biome_pack_cycle}`.

## Structure and bastion loot

Source: `data/minecraft/loot_table/chests/*` of the pinned server, already extracted into
`world_defects.json`. The engine's tables were the 1.21.5 shapes; they are replaced with
the 26.3 pools (desert pyramid has leather and copper horse armor, no saddle; jungle temple
adds leather and copper horse armor; stone spears replace/extend axes in ruins; shipwreck and
ruin chests roll an extra nautilus-armor pool). `enchant_with_levels` (30 levels, the jungle book)
uses `enchant::enchant_with_levels`; `set_instrument` picks among the first four goat horns of
`#minecraft:regular_goat_horns`. Missing-item entries remain empty at Java's weight. In the bastion
tables the saddle, golden apple, golden carrot, crimson nylium/fungus/roots and bone block
entries that were left empty are now real items.

Loot still uses VoxelCraft's RNG, not Java's loot random sequences (v0.8 item).
Tests: `world::temples::tests::{loot_tables_match_the_java_26_3_fixture,
outpost_horns_are_regular_instruments_and_temple_books_use_thirty_levels}` and
`world::bastion::tests::stable_and_other_chests_hold_saddles_apples_carrots_and_nylium`.

## Nether biome fuzzy zoom

Source: `BiomeManager.getBiome`, `getFiddledDistance`, `getFiddle`,
`LinearCongruentialGenerator.next` (bytecode) and `obfuscateSeed`
(Guava SHA-256 `hashLong`). `world::biome_zoom::BiomeZoom` ports them; a Java
transcription run on a JDK produced the pinned values for two seeds and seven
positions (`fuzzy_quarts_match_the_java_bytecode_transcription`, plus SHA-256
standard vectors). `Generator::nether_biome_at(IVec3)` is the zoomed lookup used by
Nether mob spawning (list, chance and spawn cost), the F3 biome line and music, which
Java reads through `level.getBiome`. Fog and the biome fog blend use `getNoiseBiome`
(plain quarts) in Java and stay on quarts. Not changed: terrain surface and feature gating
still use the plain quart grid (Java's surface builder also goes through the
zoomed manager), because the column grid cache stores one biome per 4×4 column;
ambient particles keep their cached window.
Spawn selection chooses its list before the final height is known, so it
jitters at the player's height. Test: `world::nether_biome::tests::zoomed_biomes_roughen_borders_without_leaving_the_neighbouring_quarts`.

## Jungle temple (piece port)

`JungleTemplePiece.postProcess`, `ScatteredFeaturePiece.updateAverageGroundHeight`,
`StructurePiece.{makeBoundingBox, getWorldX/Y/Z, setOrientation, placeBlock, generateBox, generateAirBox,
createChest, reorient, createDispenser}` and `JungleTemplePiece$MossStoneSelector` were disassembled and the piece
reproduced operation by operation (about 110 box fills and placements, the same order, so later writes
overwrite earlier ones as in Java). The old layout was an original build with the selector backwards (40% mossy).
Directions are written in Java's frame and converted: stairs and repeaters point the other way in this engine
(the engine's stair facing is its low side, its repeater facing the output side). Differences: chiseled stone
bricks are stone bricks, the footprint-wide ground average, and the old random vines are gone (Java's piece has
three fixed ones). Tests: `scattered_pieces_use_javas_orientation_frame`, `jungle_temple_follows_the_java_piece`,
the existing tripwire-fires-dispenser test, and the rotated-build/locate tests.

## Geode crack fluid ticks

See [amethyst geodes](2026-10-13-amethyst-geodes.md#v070-defect-fix-crack-fluid-ticks).
