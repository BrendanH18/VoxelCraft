# World, lighting and structure defect fixes — 2026-10-13

[Defect inventory](2026-10-13-v0.7-defect-inventory.md) · [Lighting and spawning](2026-10-13-lighting-spawning.md)

Branch `work/fix-w5-world` (worker 5 of the v0.7.0 defect fixes). Reference:
official Java 26.3 server, SHA-1 `33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`.
Nothing here is **Verified**: there was no ordinary-player testing. Each section
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
