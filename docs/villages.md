# Villages and villagers

Village content uses append-only block states 900–952. All workstation
blocks have original procedural textures. Barrels use the saved 27-slot
chest inventory, shared by host, controller players and CLI agents.
Smokers cook food and blast furnaces smelt ores/raw metals/ancient debris
in five seconds, burning fuel twice as fast (eight items per coal).
Their inventories, progress and XP survive saves, and lit states emit 13.

Grindstones use two inputs and a result in the host/controller screens.
CLI agents use `grindstone [second-hotbar-slot]` while targeting one.
Repair combines remaining durability plus floor(5% maximum durability).
Non-curse enchantments are removed, curses and custom names survive,
uncursed enchanted books become books, and prior work resets according
to the retained curses. XP is uniformly chosen from ceil(sum/2) through
2*ceil(sum/2)-1, where sum is the removed enchantments' minimum cost.
Taking the result consumes inputs once and releases XP orbs.

Craftable: job sites, hay bales and fast furnaces. Bells are village loot /
creative only, as in Java. A shovel makes dirt paths; paths are 15/16 high
and drop dirt. Hay bales support three placement axes.

Current simplifications: composter processing, lectern books, loom banners,
cartography maps, stonecutter UI, wall/ceiling grindstones, vertical barrel
facings, bell ringing and hay-bale fall cushioning are not implemented.
Blast-furnace equipment recycling is not yet implemented.

## Java references

- [GrindstoneMenu](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/inventory/GrindstoneMenu.java)
- [AbstractFurnaceBlockEntity](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/entity/AbstractFurnaceBlockEntity.java)
- [Java 1.21 data](https://github.com/InventivetalentDev/minecraft-assets/tree/1.21/data/minecraft)
- [RandomSpreadStructurePlacement](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/levelgen/structure/placement/RandomSpreadStructurePlacement.java)
- [JigsawStructure](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/levelgen/structure/structures/JigsawStructure.java)
- [Villager](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/entity/npc/Villager.java)

## Generation

`/locate village` or `/locate structure village` locates the nearest valid
start. Only the Overworld generates villages: plains, desert, savanna,
taiga and the generator's snowy biome map directly to the five Java sets.
Placement uses Java's 16-block chunks, 34 spacing, 8 separation,
10387312 salt, linear spread and legacy Java random, including negative
regions. Noise remains VoxelCraft's own, so starts are not vanilla seeds.

A well/meeting point grows streets through oriented connectors, then houses
and farms; branching stops at depth 6 and 80 blocks from the centre. Piece
boxes cannot overlap. Streets follow deterministic terrain columns; rigid
houses sit at their entrances and fill foundations downward. Cliffs/water
are rejected. Each chunk paints only its portion of the cached layout,
including foundations and clearing tree canopies above pieces. Eviction
and generation order do not change layouts.

Palettes use oak/cobble, sandstone, acacia/terracotta, spruce/cobble, and
spruce/snow roofs. Houses contain beds, a job site, a lit doorway and a
chest; farms have wheat/carrot/potato rows, irrigation, hay and composters.
Generated containers register immediately; saved entries override seeded
loot, including empty looted chests. House loot follows Java 1.21 biome
weights, 3–8 rolls and item counts. Unsupported loot remains weighted
empty rolls (berries, some seeds, saddles, signs, blue ice, beetroot soup).

The compact house/street/farm/meeting pools are original geometry inspired
by Java's jigsaw pools. They do not reproduce the full vanilla NBT template
catalogue, pool weights, processor lists, abandoned variants, decorative
pools, terrain beard blending or specialized profession chest tables.
Snow roofs currently use full snow blocks because snow layers are absent.

Baseline on Apple M5, `--bench --rd 4`: 0.228 ms/generated chunk,
0.678 ms/dense chunk for light+mesh. Overlap painting, signed placement,
all five biome sets, seeded loot, cache regeneration and saved-container
precedence have regression coverage.

After village generation, the same idle-system benchmark measured 0.210
ms/generated chunk and 0.663 ms/dense light+mesh chunk (no material
regression; this fixed benchmark volume does not intersect a village).
