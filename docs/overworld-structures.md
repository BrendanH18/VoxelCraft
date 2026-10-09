# Overworld structures (v0.6 work in progress)

The generator now builds desert pyramids, jungle temples, swamp huts,
igloos, pillager outposts, shipwrecks, ocean ruins and ocean monuments.
Existing villages, monster rooms, mineshafts and strongholds remain in
place. Structures use original layouts with Minecraft's recognizable
materials and major rooms; these are not copies of Mojang's templates.

## Placement and generation

Placement uses Java's 48-bit random source, region seeding constants,
16-block placement chunks, salts, spacing and separation. Monuments use
triangular spread. Outposts use the separate legacy one-in-five frequency
roll and exclude potential village placement chunks within ten chunks.
The engine still stores and meshes 32-block chunks.

| Structure | Spacing / separation (Java chunks) | Salt | Biomes |
|---|---:|---:|---|
| Desert pyramid | 32 / 8 | 14357617 | Desert |
| Jungle temple | 32 / 8 | 14357619 | Jungle, bamboo jungle |
| Swamp hut | 32 / 8 | 14357620 | Swamp |
| Igloo | 32 / 8 | 14357618 | Snowy plains, snowy taiga, snowy slopes |
| Pillager outpost | 32 / 8 | 165745296 | Plains, desert, savanna, taiga, snowy plains and mountain biomes |
| Shipwreck | 24 / 4 | 165745295 | Oceans and beaches |
| Ocean ruin | 20 / 8 | 14357621 | Oceans; sandstone in warm seas, stone brick in cold seas |
| Ocean monument | 32 / 5 | 10387313 | Deep ocean, with a surrounding ocean/river biome check |

Each layout is built deterministically, rotated, sorted by height and
cached. Generation paints only the blocks inside the requested chunk.
Discovery bounds cover all rotations, including the long shipwreck hull.
A bounded cache evicts one entry at capacity; parallel fills recheck for
an existing entry before eviction. Clearing the cache produces identical
blocks and loot seeds. There are no new block or item IDs in this feature.

Biome selection, terrain alignment and layouts belong to VoxelCraft's
terrain generator, so a seed does not produce Minecraft's complete world.

## Exploration and loot

`/locate structure` accepts `desert_pyramid`, `jungle_pyramid`, `swamp_hut`,
`igloo`, `pillager_outpost`, `shipwreck`, `ocean_ruin` and `monument`.
Aliases include `jungle_temple`, `witch_hut`, `outpost` and `ocean_monument`;
the `minecraft:` prefix is accepted. Console completion lists the main
names. Searching is restricted to the Overworld and the specified radius,
and accounts for nearer candidates across region boundaries.

Pyramids contain four treasure chests and a pressure-plate TNT trap.
Jungle temples contain two treasure chests and a tripwire arrow trap.
Igloos have a bed, crafting table and furnace; half have a basement with
an accessible ladder, brewing stand and a chest containing a golden apple.
Outposts have a climbable lookout tower and a loot chest. Shipwrecks have
supply, treasure and map-room chests; ruins have small and large chest
variants. Monuments contain flooded halls, two wings, a central tower,
sea lanterns, optional sponge rooms and an eight-block gold core.

Supported loot entries preserve Java's pool weights, roll ranges, counts,
and random enchantment/damage flags. Separate pools stay separate.
Unavailable entries retain their original weight as an empty result:
maps, horse armor, saddles, armor trim templates, enchanted golden apples,
experience bottles, goat horns and suspicious stew are still pending.
Heart of the sea belongs in buried treasure and is not substituted into
shipwreck loot. The shared loot helper uses VoxelCraft's seeded layout RNG
and enchantment selection, rather than Java's exact loot random sequences
and inventory stack splitting.

Igloo furnaces and brewing stands register their functional state on
chunk load and preserve saved inventories. Generated dispensers keep their
one or two arrow stacks in the nine usable slots. Chest/dispenser inventories register when their chunk
loads; saved contents take precedence so emptying a container remains
permanent through regeneration. Normal storage, UI, agents and gamepads
use the existing container paths.

## Remaining parity work

- Aquatic mobs, guardians/elder guardians and their monument spawning,
  combat, Mining Fatigue and drops.
- Pillagers, outpost population, cages and ancillary pieces; raids.
- Hut witch/cat residents and structure-specific spawning bounds.
- Igloo villager/zombie-villager residents, the weakness potion and variable
  basement depth; the engine's bed rendering has no orientation states.
- Jungle temple lever/piston puzzle and Minecraft's full trap layout.
- Shipwreck template variants, all floor orientations, buried treasure
  and exploration maps; `ocean_ruin` currently combines warm/cold variants.
- Ocean ruin clusters and archaeology; exact monument room graphs and
  block-for-block Java structure templates.

## References

- [Java 1.21.5 structure set data extracted from the game](https://mcasset.cloud/1.21.5/data/minecraft/worldgen/structure_set/_all.json).
- [Java 1.21.5 chest loot data extracted from the game](https://github.com/misode/mcmeta/tree/1.21.5-data/data/minecraft/loot_table/chests).
- [Placement/frequency reference implementation](https://github.com/misode/deepslate/blob/main/src/worldgen/structure/StructurePlacement.ts).
