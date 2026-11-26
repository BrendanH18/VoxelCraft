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
bastion_remnant` works in the Nether. Once a bastion near a player has
loaded, its pieces get persistent piglins, brutes and (in the stables)
hoglins, once per bastion; see [Nether mobs](nether-mobs.md#bastion-residents).
Magma cube spawners remain absent. Like Java, bastions skip regions whose
start chunk centre lies in a basalt delta (see [Nether biomes](nether-biomes.md)).

## Generation 6: Java's full pool catalogue (v1.0)

Worlds made from v1.0 on (generation 6) build bastions from Java 26.3's
whole `bastion/*` catalogue under fidelity exception FE-02: all 60 pools
with Java's names, weights, fallbacks and processor lists, and an original
template for each of the 167 locations they name, at Java's size, with
Java's jigsaw connectors where Java's templates hold them, and exactly
Java's count of chests (with their loot tables), the treasure room's magma
cube spawner, gold blocks, gilded blackstone, lava and residents.
`src/world/jigsaw/bastion_v6.txt` holds those facts and the pools;
`src/world/bastion/catalogue.rs` draws the templates by kind: ramparts and
walls are blackstone masses faced with polished bricks, with rooms, slits
and a crenellated walk; housing units have brick floors, walls with
windows and a stair; the hoglin stables are railed pens and stair halls;
the treasure room has a lava basin with the spawner, gold-and-treasure
pedestals, beams, corners and walkways; the bridge is an arched span on
legs before a gate with an original piglin face. The jigsaw engine
assembles them with Java's settings (start pool `bastion/starts`, size 6,
80 blocks from the centre, start height 33). Each piece runs its pool
element's processor list as Java's `RuleProcessor` does, including the
high rampart's `axis_aligned_linear_pos` test, which removes blocks more
often the higher they stand (up to 5% 100 blocks up). Residents come from
the placed `bastion/mobs/*` templates: brutes with golden axes, piglins
with golden swords or crossbows, hoglins. Generations 3 to 5 keep the
compact original graph below, so their bastions don't change.
`structure_catalogue_tests` checks the catalogue against
`tests/fixtures/java/26.3/bastion_catalogue.json` both ways
([record](verification/2026-11-25-v1.0-track-08-generation.md)).

Loot uses the four Java 1.21.1 pools and preserves weights/counts for
implemented items, including gear enchantments and durability. Unsupported
items (spectral arrows, snout trims, Pigstep, golden apples, crimson flora
and others) retain their weights as empty rolls; crying obsidian and
crossbows now drop. Treasure templates are guaranteed; bridge, stable and generic
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
text is original; Java's End Poem is not copied. Start or the jump button on
any connected controller skips them, like Escape and Space. Gap: split-screen
gamepad seats and CLI agents share the host's dimension and never take the exit
portal themselves, so they have no credits flag of their own; per-player
dimensions follow 1.0 with multiplayer.
