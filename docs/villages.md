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
