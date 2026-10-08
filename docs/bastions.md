# Nether materials and bastion remnants

Block states 800–834 contain blackstone, polished blackstone, polished and
cracked blackstone bricks, chiseled and gilded blackstone, axial basalt and
polished basalt, magma, gold blocks, axial chains, and stairs/slabs/walls for
the three blackstone building materials. Item IDs 608–610 add magma cream,
iron nuggets and slimeballs; the existing gold nugget keeps its saved ID.
Recipes, harvesting and gilded-blackstone Fortune/Silk Touch drops follow
Java. Magma does one point of fire contact damage without igniting the
player; sneaking, Frost Walker and Fire Resistance protect them. Frost
Walker is appended to saved enchantment bits; freezing water remains a gap.
Slimeballs have no natural mob source yet. Magma cream is available in
bastion loot; basalt and blackstone can be mined from the structures.

Nether complexes use Java's 16-block chunk grid: spacing 27, separation 4,
salt 30084232, and fortress:bastion weights 2:3. Region and selection RNGs
match Java's legacy random spread and large-feature seed algorithms.
Terrain and piece layouts remain VoxelCraft's, so worlds are not copies of
Java seeds. A shared placement function prevents both structures claiming
the same region. Existing modified chunks and saved containers win.

Four hand-authored layouts replace Java's jigsaw assembly: housing around
a wart courtyard, tiered hoglin pens, a lava-filled treasure hall with two
guaranteed upgrade-template chests, and a bridge with a piglin-face rampart.
Blackstone, basalt, gilded blocks, stairs, walls, chains, lava and gold form
the structures. Cached piece frames and clipped painting are shared by
chunk and single-column ore-exposure queries. `/locate structure
bastion_remnant` works in the Nether. No piglins, hoglins, brutes or magma
cube spawners are added; bastions have no generated resident mobs. Like Java, bastions skip regions
whose start chunk centre lies in a basalt delta (see [Nether biomes](nether-biomes.md)).

Loot uses the four Java 1.21.1 pools and preserves weights/counts for
implemented items, including gear enchantments and durability. Unsupported
items (crossbows, spectral arrows, snout trims, Pigstep, golden apples,
crying obsidian, crimson flora and others) retain their weights as empty
rolls. Treasure templates are guaranteed; bridge, stable and generic
chests have a separate 10% template roll. Loot scattering uses the engine's
existing independent RNG rather than Java's loot random-sequence stream.

Sources:
- [Nether-complex placement data](https://raw.githubusercontent.com/InventivetalentDev/minecraft-assets/1.21.1/data/minecraft/worldgen/structure_set/nether_complexes.json)
- [RandomSpreadStructurePlacement](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/levelgen/structure/placement/RandomSpreadStructurePlacement.java)
- [WorldgenRandom](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/levelgen/WorldgenRandom.java)
- [ChunkGenerator weighted selection](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/chunk/ChunkGenerator.java)
- [Treasure loot](https://raw.githubusercontent.com/InventivetalentDev/minecraft-assets/1.21.1/data/minecraft/loot_table/chests/bastion_treasure.json), with `bastion_bridge`, `bastion_hoglin_stable` and `bastion_other` in the same folder.
- [Gilded-blackstone loot](https://raw.githubusercontent.com/InventivetalentDev/minecraft-assets/1.21.1/data/minecraft/loot_table/blocks/gilded_blackstone.json)
- [MagmaBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/MagmaBlock.java)

Generation benchmark: `cargo run --release --no-default-features --example
nether_bench` measures 1,024 chunks (seed 12345, x/z −8..7, y 0..3), five
fresh generator runs. Before bastions: median **0.204 ms/chunk** on Apple M5. After bastions and
shared placement: median **0.187 ms/chunk** (five runs: 0.215, 0.186,
0.187, 0.187, 0.187). The reduction in fortress frequency offsets bastion
painting; this is a fixed-area comparison, not a universal speedup.
Use `bastion_locations` to reproduce screenshots of all four layouts.

## End credits

The first time a player enters the exit portal (saved as `credits_seen` in
the player's level properties), a skippable scrolling credits screen plays
with the music system's credits situation, then the player respawns. The
text is original; Java's End Poem is not copied. Gap: split-screen gamepad
seats and CLI agents don't see or save the credits flag, and gamepads can't
skip them.
