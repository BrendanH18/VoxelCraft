//! Block registry: ids, render classification and per-face texture layers.
//!
//! Block properties live in a 4096-entry static table so hot loops (meshing,
//! physics) resolve them with a single indexed load instead of a `match`.

use crate::item::{Item, ToolKind};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[repr(transparent)]
pub struct Block(pub u16);

/// Registry capacity; state IDs are append-only and must be below this bound.
pub const STATE_CAPACITY: usize = 4096;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RenderKind {
    /// Never rendered (air).
    Invisible,
    /// Fully opaque cube: hides neighbouring faces and casts ambient occlusion.
    Opaque,
    /// Alpha-tested (leaves, glass). Rendered in the opaque pass with `discard`.
    Cutout,
    /// Alpha-blended (water). Rendered last, sorted back to front.
    Translucent,
    /// Two crossed diagonal planes (plants, torches): alpha-tested, drawn
    /// from both sides, never hides or occludes neighbours.
    Cross,
    /// Built from a few boxes smaller than the cell (stairs, fences, doors;
    /// see `world::shape`). Never hides or occludes neighbours.
    Shaped,
}

#[derive(Clone, Copy, Debug)]
pub struct BlockInfo {
    pub name: &'static str,
    pub kind: RenderKind,
    /// Participates in collision and can be targeted by the crosshair.
    pub solid: bool,
    /// Hide faces between two blocks of this same type (glass, water).
    pub self_cull: bool,
    /// Texture layer per face, ordered +X, -X, +Y, -Y, +Z, -Z.
    pub tex: [u16; 6],
}

/// Texture array layers. Must match the generators in `render::textures`.
pub mod tex {
    pub const STONE: u16 = 0;
    pub const DIRT: u16 = 1;
    pub const GRASS_TOP: u16 = 2;
    pub const GRASS_SIDE: u16 = 3;
    pub const SAND: u16 = 4;
    pub const WATER: u16 = 5;
    pub const LOG_SIDE: u16 = 6;
    pub const LOG_TOP: u16 = 7;
    pub const LEAVES: u16 = 8;
    pub const PLANKS: u16 = 9;
    pub const COBBLESTONE: u16 = 10;
    pub const GLASS: u16 = 11;
    pub const BEDROCK: u16 = 12;
    pub const GRAVEL: u16 = 13;
    pub const SNOW: u16 = 14;
    pub const SNOWY_GRASS_SIDE: u16 = 15;
    pub const COAL_ORE: u16 = 16;
    pub const IRON_ORE: u16 = 17;
    pub const GOLD_ORE: u16 = 18;
    pub const DIAMOND_ORE: u16 = 19;
    pub const CACTUS_SIDE: u16 = 20;
    pub const CACTUS_TOP: u16 = 21;
    pub const BRICKS: u16 = 22;
    pub const SANDSTONE_SIDE: u16 = 23;
    pub const SANDSTONE_TOP: u16 = 24;
    pub const GLOWSTONE: u16 = 25;
    pub const SPRUCE_LEAVES: u16 = 26;
    // HUD icons.
    pub const HEART_FULL: u16 = 27;
    pub const HEART_HALF: u16 = 28;
    pub const HEART_EMPTY: u16 = 29;
    pub const BUBBLE: u16 = 30;
    /// Block-breaking crack overlays, stages 0..10.
    pub const CRACK_0: u16 = 31;
    pub const CRACK_STAGES: u8 = 10;
    // Cross-shaped plants and torches.
    pub const TALL_GRASS: u16 = 41;
    pub const DANDELION: u16 = 42;
    pub const POPPY: u16 = 43;
    pub const DEAD_BUSH: u16 = 44;
    pub const TORCH: u16 = 45;
    pub const LAVA: u16 = 46;
    pub const OBSIDIAN: u16 = 47;
    pub const WOOL: u16 = 48;
    pub const TABLE_TOP: u16 = 49;
    pub const TABLE_SIDE: u16 = 50;
    pub const FURNACE_FRONT: u16 = 51;
    pub const FURNACE_LIT: u16 = 52;
    pub const FURNACE_TOP: u16 = 53;
    // Hunger bar icons.
    pub const FOOD_FULL: u16 = 54;
    pub const FOOD_HALF: u16 = 55;
    pub const FOOD_EMPTY: u16 = 56;
    pub const FURNACE_SIDE: u16 = 57;
    pub const CHEST_TOP: u16 = 58;
    pub const CHEST_SIDE: u16 = 59;
    pub const CHEST_FRONT: u16 = 60;
    pub const FARMLAND: u16 = 61;
    pub const WET_FARMLAND: u16 = 62;
    /// Wheat growth stages 0..8.
    pub const WHEAT_0: u16 = 63;
    pub const OAK_SAPLING: u16 = 71;
    pub const SPRUCE_SAPLING: u16 = 72;
    pub const BED_TOP_FOOT: u16 = 73;
    pub const BED_TOP_HEAD: u16 = 74;
    pub const BED_SIDE_FOOT: u16 = 75;
    pub const BED_SIDE_HEAD: u16 = 76;
    pub const NETHERRACK: u16 = 77;
    pub const SOUL_SAND: u16 = 78;
    pub const QUARTZ_ORE: u16 = 79;
    pub const NETHER_BRICKS: u16 = 80;
    pub const PORTAL: u16 = 81;
    pub const TNT_SIDE: u16 = 82;
    pub const TNT_TOP: u16 = 83;
    pub const TNT_BOTTOM: u16 = 84;
    pub const LADDER: u16 = 85;
    pub const DOOR_TOP: u16 = 86;
    pub const DOOR_BOTTOM: u16 = 87;
    /// The player's arm in first person.
    pub const SKIN: u16 = 88;
    /// Seven flame animation frames.
    pub const FIRE_0: u16 = 89;
    pub const FIRE_FRAMES: u8 = 7;
    pub const SPRUCE_LOG_SIDE: u16 = 96;
    pub const SPRUCE_LOG_TOP: u16 = 97;
    pub const BIRCH_LOG_SIDE: u16 = 98;
    pub const BIRCH_LOG_TOP: u16 = 99;
    pub const JUNGLE_LOG_SIDE: u16 = 100;
    pub const JUNGLE_LOG_TOP: u16 = 101;
    pub const ACACIA_LOG_SIDE: u16 = 102;
    pub const ACACIA_LOG_TOP: u16 = 103;
    pub const BIRCH_LEAVES: u16 = 104;
    pub const JUNGLE_LEAVES: u16 = 105;
    pub const ACACIA_LEAVES: u16 = 106;
    pub const SPRUCE_PLANKS: u16 = 107;
    pub const BIRCH_PLANKS: u16 = 108;
    pub const JUNGLE_PLANKS: u16 = 109;
    pub const ACACIA_PLANKS: u16 = 110;
    pub const BIRCH_SAPLING: u16 = 111;
    pub const JUNGLE_SAPLING: u16 = 112;
    pub const ACACIA_SAPLING: u16 = 113;
    pub const RED_SAND: u16 = 114;
    /// Plain terracotta, then the six dyed colours (see `Block::TERRACOTTA`).
    pub const TERRACOTTA: u16 = 115;
    pub const CLAY: u16 = 122;
    pub const SUGAR_CANE: u16 = 123;
    pub const PUMPKIN_SIDE: u16 = 124;
    pub const PUMPKIN_TOP: u16 = 125;
    pub const MELON_SIDE: u16 = 126;
    pub const MELON_TOP: u16 = 127;
    pub const FERN: u16 = 128;
    pub const BLUE_ORCHID: u16 = 129;
    pub const ICE: u16 = 130;
    /// Biome-coloured copies of [`FOLIAGE`] textures: for each foliage
    /// group from 1 (see `terrain::Biome::foliage`), one layer per entry.
    pub const FOLIAGE_0: u16 = 131;
    pub const FOLIAGE: [u16; 5] = [GRASS_TOP, GRASS_SIDE, LEAVES, TALL_GRASS, FERN];
    pub const FOLIAGE_GROUPS: u8 = 5;
    pub const END_STONE: u16 = FOLIAGE_0 + (FOLIAGE_GROUPS as u16 - 1) * FOLIAGE.len() as u16;
    pub const SPAWNER: u16 = END_STONE + 1;
    /// Nether wart's three looks: ages 0-1, age 2 and ripe (age 3).
    pub const NETHER_WART_0: u16 = SPAWNER + 1;
    /// The brewing stand's sides and top: its rod down the middle columns
    /// and its stone plates along the bottom (shaped boxes sample a texture
    /// by their position in the cell).
    pub const BREWING_SIDE: u16 = NETHER_WART_0 + 3;
    pub const BREWING_TOP: u16 = BREWING_SIDE + 1;
    /// Stone bricks: plain, mossy and cracked.
    pub const STONE_BRICKS: u16 = BREWING_TOP + 1;
    pub const MOSSY_STONE_BRICKS: u16 = STONE_BRICKS + 1;
    pub const CRACKED_STONE_BRICKS: u16 = STONE_BRICKS + 2;
    pub const IRON_BARS: u16 = STONE_BRICKS + 3;
    pub const BOOKSHELF: u16 = IRON_BARS + 1;
    pub const COBWEB: u16 = BOOKSHELF + 1;
    /// End portal frames: top and side, then the same with an eye of ender.
    pub const FRAME_TOP: u16 = COBWEB + 1;
    pub const FRAME_SIDE: u16 = FRAME_TOP + 1;
    pub const FRAME_EYE_TOP: u16 = FRAME_TOP + 2;
    pub const FRAME_EYE_SIDE: u16 = FRAME_TOP + 3;
    pub const END_PORTAL: u16 = FRAME_TOP + 4;
    pub const DRAGON_EGG: u16 = END_PORTAL + 1;
    pub const LAPIS_ORE: u16 = DRAGON_EGG + 1;
    pub const LAPIS_BLOCK: u16 = LAPIS_ORE + 1;
    /// The enchanting table's red cloth top and its 12-high sides (shaped
    /// boxes sample rows 4..16 of a side).
    pub const ENCHANT_TOP: u16 = LAPIS_BLOCK + 1;
    pub const ENCHANT_SIDE: u16 = ENCHANT_TOP + 1;
    pub const IRON_BLOCK: u16 = ENCHANT_SIDE + 1;
    /// Anvil body, then its top intact, chipped and damaged.
    pub const ANVIL: u16 = IRON_BLOCK + 1;
    pub const ANVIL_TOP: u16 = ANVIL + 1;
    pub const DEBRIS_SIDE: u16 = ANVIL_TOP + 3;
    pub const DEBRIS_TOP: u16 = DEBRIS_SIDE + 1;
    pub const NETHERITE_BLOCK: u16 = DEBRIS_TOP + 1;
    /// The smithing table: an iron top, dark wooden front and back (tongs)
    /// and sides (a hammer), and a plank bottom.
    pub const SMITHING_TOP: u16 = NETHERITE_BLOCK + 1;
    pub const SMITHING_FRONT: u16 = SMITHING_TOP + 1;
    pub const SMITHING_SIDE: u16 = SMITHING_TOP + 2;
    pub const SMITHING_BOTTOM: u16 = SMITHING_TOP + 3;
    pub const MOSSY_COBBLESTONE: u16 = SMITHING_BOTTOM + 1;
    pub const COPPER_ORE: u16 = MOSSY_COBBLESTONE + 1;
    pub const REDSTONE_ORE: u16 = COPPER_ORE + 1;
    pub const EMERALD_ORE: u16 = REDSTONE_ORE + 1;
    pub const RAW_IRON_BLOCK: u16 = EMERALD_ORE + 1;
    pub const RAW_GOLD_BLOCK: u16 = RAW_IRON_BLOCK + 1;
    pub const RAW_COPPER_BLOCK: u16 = RAW_GOLD_BLOCK + 1;
    pub const COPPER_BLOCK: u16 = RAW_COPPER_BLOCK + 1;
    pub const EMERALD_BLOCK: u16 = COPPER_BLOCK + 1;
    pub const GRANITE: u16 = EMERALD_BLOCK + 1;
    pub const POLISHED_GRANITE: u16 = GRANITE + 1;
    pub const DIORITE: u16 = POLISHED_GRANITE + 1;
    pub const POLISHED_DIORITE: u16 = DIORITE + 1;
    pub const ANDESITE: u16 = POLISHED_DIORITE + 1;
    pub const POLISHED_ANDESITE: u16 = ANDESITE + 1;
    pub const TUFF: u16 = POLISHED_ANDESITE + 1;
    pub const CALCITE: u16 = TUFF + 1;
    pub const SMOOTH_STONE: u16 = CALCITE + 1;
    pub const DEEPSLATE: u16 = SMOOTH_STONE + 1;
    pub const COBBLED_DEEPSLATE: u16 = DEEPSLATE + 1;
    pub const POLISHED_DEEPSLATE: u16 = COBBLED_DEEPSLATE + 1;
    pub const DEEPSLATE_COAL_ORE: u16 = POLISHED_DEEPSLATE + 1;
    pub const DEEPSLATE_IRON_ORE: u16 = DEEPSLATE_COAL_ORE + 1;
    pub const DEEPSLATE_COPPER_ORE: u16 = DEEPSLATE_IRON_ORE + 1;
    pub const DEEPSLATE_GOLD_ORE: u16 = DEEPSLATE_COPPER_ORE + 1;
    pub const DEEPSLATE_REDSTONE_ORE: u16 = DEEPSLATE_GOLD_ORE + 1;
    pub const DEEPSLATE_EMERALD_ORE: u16 = DEEPSLATE_REDSTONE_ORE + 1;
    pub const DEEPSLATE_LAPIS_ORE: u16 = DEEPSLATE_EMERALD_ORE + 1;
    pub const DEEPSLATE_DIAMOND_ORE: u16 = DEEPSLATE_LAPIS_ORE + 1;
    pub const DARK_OAK_LOG_SIDE: u16 = DEEPSLATE_DIAMOND_ORE + 1;
    pub const DARK_OAK_LOG_TOP: u16 = DARK_OAK_LOG_SIDE + 1;
    pub const MANGROVE_LOG_SIDE: u16 = DARK_OAK_LOG_TOP + 1;
    pub const MANGROVE_LOG_TOP: u16 = MANGROVE_LOG_SIDE + 1;
    pub const CHERRY_LOG_SIDE: u16 = MANGROVE_LOG_TOP + 1;
    pub const CHERRY_LOG_TOP: u16 = CHERRY_LOG_SIDE + 1;
    pub const DARK_OAK_PLANKS: u16 = CHERRY_LOG_TOP + 1;
    pub const MANGROVE_PLANKS: u16 = DARK_OAK_PLANKS + 1;
    pub const CHERRY_PLANKS: u16 = MANGROVE_PLANKS + 1;
    pub const DARK_OAK_DOOR_BOTTOM: u16 = CHERRY_PLANKS + 1;
    pub const DARK_OAK_DOOR_TOP: u16 = DARK_OAK_DOOR_BOTTOM + 1;
    pub const SPRUCE_DOOR_BOTTOM: u16 = DARK_OAK_DOOR_TOP + 1;
    pub const SPRUCE_DOOR_TOP: u16 = SPRUCE_DOOR_BOTTOM + 1;
    pub const BIRCH_DOOR_BOTTOM: u16 = SPRUCE_DOOR_TOP + 1;
    pub const BIRCH_DOOR_TOP: u16 = BIRCH_DOOR_BOTTOM + 1;
    pub const JUNGLE_DOOR_BOTTOM: u16 = BIRCH_DOOR_TOP + 1;
    pub const JUNGLE_DOOR_TOP: u16 = JUNGLE_DOOR_BOTTOM + 1;
    pub const ACACIA_DOOR_BOTTOM: u16 = JUNGLE_DOOR_TOP + 1;
    pub const ACACIA_DOOR_TOP: u16 = ACACIA_DOOR_BOTTOM + 1;
    pub const MANGROVE_DOOR_BOTTOM: u16 = ACACIA_DOOR_TOP + 1;
    pub const MANGROVE_DOOR_TOP: u16 = MANGROVE_DOOR_BOTTOM + 1;
    pub const CHERRY_DOOR_BOTTOM: u16 = MANGROVE_DOOR_TOP + 1;
    pub const CHERRY_DOOR_TOP: u16 = CHERRY_DOOR_BOTTOM + 1;
    pub const RAIL: u16 = CHERRY_DOOR_TOP + 1;
    pub const RAIL_EW: u16 = RAIL + 1;
    pub const RAIL_SE: u16 = RAIL + 2;
    pub const RAIL_SW: u16 = RAIL + 3;
    pub const RAIL_NW: u16 = RAIL + 4;
    pub const RAIL_NE: u16 = RAIL + 5;
    pub const BROWN_MUSHROOM: u16 = RAIL_NE + 1;
    pub const RED_MUSHROOM: u16 = BROWN_MUSHROOM + 1;
    pub const COUNT: u32 = RED_MUSHROOM as u32 + 1;
    /// Eleven bits in the 12-byte quad record; eight portable 256-layer GPU pages.
    pub const CAPACITY: u32 = 2048;
    pub const PAGE_LAYERS: u32 = 256;
    pub const PAGES: usize = (CAPACITY / PAGE_LAYERS) as usize;
    const _: () = assert!(COUNT <= CAPACITY);

    /// Item icons live in a texture array of their own (see
    /// `item::sprite_for_layer`); renderers address icon `index` as layer
    /// `ITEM_BASE + index`, after every block layer.
    pub const ITEM_BASE: u16 = CAPACITY as u16;

    pub const fn item_layer(index: u16) -> u16 {
        ITEM_BASE + index
    }

    /// The item icon index drawn on `layer`, if it holds one.
    pub fn item_index(layer: u16) -> Option<u16> {
        layer.checked_sub(ITEM_BASE)
    }

    /// The layer to draw `layer` with in a column of foliage `group`:
    /// grass and oak leaves take on the colour of the biome.
    #[inline]
    pub fn tinted(layer: u16, group: u8) -> u16 {
        if group == 0 {
            return layer;
        }
        match FOLIAGE.iter().position(|&l| l == layer) {
            Some(i) => FOLIAGE_0 + (group as u16 - 1) * FOLIAGE.len() as u16 + i as u16,
            None => layer,
        }
    }

    /// The plain layer and foliage group a tinted layer was made from.
    pub fn untinted(layer: u16) -> Option<(u16, u8)> {
        let i = layer.checked_sub(FOLIAGE_0)? as usize;
        (i < (FOLIAGE_GROUPS as usize - 1) * FOLIAGE.len())
            .then(|| (FOLIAGE[i % FOLIAGE.len()], (i / FOLIAGE.len()) as u8 + 1))
    }
}

impl Block {
    pub const AIR: Block = Block(0);
    pub const STONE: Block = Block(1);
    pub const DIRT: Block = Block(2);
    pub const GRASS: Block = Block(3);
    pub const SAND: Block = Block(4);
    pub const WATER: Block = Block(5);
    pub const LOG: Block = Block(6);
    pub const LEAVES: Block = Block(7);
    pub const PLANKS: Block = Block(8);
    pub const COBBLESTONE: Block = Block(9);
    pub const GLASS: Block = Block(10);
    pub const BEDROCK: Block = Block(11);
    pub const GRAVEL: Block = Block(12);
    pub const BROWN_MUSHROOM: Block = Block(520);
    pub const RED_MUSHROOM: Block = Block(521);
    pub fn is_mushroom(self) -> bool {
        matches!(self.0, 520 | 521)
    }

    pub const SNOW: Block = Block(13);
    pub const SNOWY_GRASS: Block = Block(14);
    pub const COAL_ORE: Block = Block(15);
    pub const IRON_ORE: Block = Block(16);
    pub const GOLD_ORE: Block = Block(17);
    pub const DIAMOND_ORE: Block = Block(18);
    pub const CACTUS: Block = Block(19);
    pub const BRICKS: Block = Block(20);
    pub const SANDSTONE: Block = Block(21);
    pub const GLOWSTONE: Block = Block(22);
    pub const SPRUCE_LEAVES: Block = Block(23);
    /// Flowing water levels 1 (strongest) to 7 are ids 24..=30.
    pub const FALLING_WATER: Block = Block(31);
    pub const TALL_GRASS: Block = Block(32);
    pub const DANDELION: Block = Block(33);
    pub const POPPY: Block = Block(34);
    pub const DEAD_BUSH: Block = Block(35);
    pub const TORCH: Block = Block(36);
    pub const LAVA: Block = Block(37);
    /// Flowing lava levels 1 (strongest) to 3 are ids 38..=40.
    pub const FALLING_LAVA: Block = Block(41);
    pub const OBSIDIAN: Block = Block(42);
    pub const WOOL: Block = Block(43);
    pub const CRAFTING_TABLE: Block = Block(44);
    pub const FURNACE: Block = Block(45);
    /// A burning furnace: glows, and breaks into a plain furnace.
    pub const LIT_FURNACE: Block = Block(46);
    // Furnaces facing north, east and west are ids 47..=49, lit ones 50..=52
    // (see `Block::with_facing`); the ids above face south.
    /// 27 slots of storage (see `world::chest`); facing south, then north,
    /// east and west up to id 56.
    pub const CHEST: Block = Block(53);
    /// Tilled soil for crops; wet when water is within 4 blocks.
    pub const FARMLAND: Block = Block(57);
    pub const WET_FARMLAND: Block = Block(58);
    // Wheat crops at growth stages 0..=7 are ids 59..=66 (see `Block::wheat`).
    pub const OAK_SAPLING: Block = Block(67);
    pub const SPRUCE_SAPLING: Block = Block(68);
    pub const SPRUCE_LOG: Block = Block(69);
    pub const BIRCH_LOG: Block = Block(70);
    pub const JUNGLE_LOG: Block = Block(71);
    pub const ACACIA_LOG: Block = Block(72);
    pub const BIRCH_LEAVES: Block = Block(73);
    pub const JUNGLE_LEAVES: Block = Block(74);
    pub const ACACIA_LEAVES: Block = Block(75);
    pub const SPRUCE_PLANKS: Block = Block(76);
    pub const BIRCH_PLANKS: Block = Block(77);
    pub const JUNGLE_PLANKS: Block = Block(78);
    pub const ACACIA_PLANKS: Block = Block(79);
    pub const BIRCH_SAPLING: Block = Block(80);
    pub const JUNGLE_SAPLING: Block = Block(81);
    pub const ACACIA_SAPLING: Block = Block(82);
    pub const RED_SAND: Block = Block(83);
    /// Plain terracotta; the dyed colours of badlands strata follow it up
    /// to id 90 (see [`Block::terracotta`]).
    pub const TERRACOTTA: Block = Block(84);
    pub const CLAY: Block = Block(91);
    pub const SUGAR_CANE: Block = Block(92);
    pub const PUMPKIN: Block = Block(93);
    pub const MELON: Block = Block(94);
    pub const FERN: Block = Block(95);
    pub const BLUE_ORCHID: Block = Block(96);
    pub const ICE: Block = Block(97);
    /// The two halves of a bed (see `Item::BED`), 9/16 of a block tall.
    pub const BED_FOOT: Block = Block(98);
    pub const BED_HEAD: Block = Block(99);
    pub const NETHERRACK: Block = Block(100);
    /// Slows whatever walks on it, which sinks in by 2/16 of a block.
    pub const SOUL_SAND: Block = Block(101);
    pub const QUARTZ_ORE: Block = Block(102);
    pub const NETHER_BRICKS: Block = Block(103);
    /// Fills a lit obsidian frame (see `world::portal`); standing in it
    /// takes you between the overworld and the Nether.
    pub const NETHER_PORTAL: Block = Block(104);
    /// Lit with flint and steel (or set off by a nearby blast), it blows up
    /// four seconds later (see `entity::tnt`).
    pub const TNT: Block = Block(105);
    /// Half-height slabs of stone, cobblestone, oak planks, sandstone,
    /// bricks and nether bricks, ids 106..=111 (see [`Block::slab_of`]).
    pub const STONE_SLAB: Block = Block(106);
    /// Stairs of each slab material, four facings each, ids 112..=135 (see
    /// [`Block::stairs_of`]). Their low step faces the way they face.
    pub const STONE_STAIRS: Block = Block(112);
    /// Joins up with neighbouring fences, gates and full blocks; 1.5 blocks
    /// tall to anything trying to jump it.
    pub const OAK_FENCE: Block = Block(136);
    /// Climbable; ids 137..=140 face south, north, east and west, away from
    /// the wall they hang on.
    pub const LADDER: Block = Block(137);
    /// Ids 141..=148: closed facing south, north, east and west, then open.
    pub const FENCE_GATE: Block = Block(141);
    /// The lower half of a closed door facing south. Ids 149..=164: lower
    /// then upper half, each closed then open, four facings apiece (see
    /// [`Block::door`]). The panel sits on the side the door faces.
    pub const OAK_DOOR: Block = Block(149);
    /// Fire ages 0..=15, saved as ids 165..=180. All ages share geometry.
    pub const FIRE: Block = Block(165);
    pub const END_STONE: Block = Block(181);
    /// Fences of nether brick, which don't join wooden ones.
    pub const NETHER_BRICK_FENCE: Block = Block(182);
    /// A monster spawner cage (see `World::spawner`).
    pub const SPAWNER: Block = Block(183);
    /// Nether wart at ages 0..=3, ids 184..=187 (see [`Block::nether_wart`]).
    /// Grows on soul sand, in any light.
    pub const NETHER_WART: Block = Block(184);
    /// Brews potions (see `world::brewing`).
    pub const BREWING_STAND: Block = Block(188);
    /// Stone brick stairs, ids 189..=192, and slab, 193: the seventh slab
    /// material (see [`Block::SLAB_BASES`]).
    pub const STONE_BRICK_STAIRS: Block = Block(189);
    pub const STONE_BRICK_SLAB: Block = Block(193);
    pub const STONE_BRICKS: Block = Block(194);
    pub const MOSSY_STONE_BRICKS: Block = Block(195);
    pub const CRACKED_STONE_BRICKS: Block = Block(196);
    /// Joins neighbouring bars, panes and full blocks like Java's panes.
    pub const IRON_BARS: Block = Block(197);
    pub const BOOKSHELF: Block = Block(198);
    /// Slows anything inside it to a crawl.
    pub const COBWEB: Block = Block(199);
    /// End portal frames facing south, north, east and west (ids 200..=203),
    /// then holding an eye of ender (204..=207). Twelve facing inward, all
    /// with eyes, open an End portal (see `world::end_portal`).
    pub const END_PORTAL_FRAME: Block = Block(200);
    /// Takes whatever falls in to the End.
    pub const END_PORTAL: Block = Block(208);
    /// Left on the exit portal by the first dragon killed; it teleports away
    /// when hit or used.
    pub const DRAGON_EGG: Block = Block(209);
    /// Spawned around the central island for each dragon killed; it leads
    /// out to the outer islands and back.
    pub const END_GATEWAY: Block = Block(210);
    pub const LAPIS_ORE: Block = Block(211);
    pub const LAPIS_BLOCK: Block = Block(212);
    /// Enchants gear for levels and lapis (see `crate::enchant`); nearby
    /// bookshelves raise its offers.
    pub const ENCHANTING_TABLE: Block = Block(213);
    pub const IRON_BLOCK: Block = Block(214);
    /// Anvils, chipped anvils and damaged anvils (ids 215..=220), each with
    /// the top running along z, then along x (see [`Block::anvil`]).
    pub const ANVIL: Block = Block(215);
    pub const CHIPPED_ANVIL: Block = Block(217);
    pub const DAMAGED_ANVIL: Block = Block(219);
    pub const ANCIENT_DEBRIS: Block = Block(221);
    pub const NETHERITE_BLOCK: Block = Block(222);
    /// Upgrades diamond gear to Netherite (see `crate::smithing`).
    pub const SMITHING_TABLE: Block = Block(223);
    pub const MOSSY_COBBLESTONE: Block = Block(224);
    pub const COPPER_ORE: Block = Block(225);
    pub const REDSTONE_ORE: Block = Block(226);
    pub const EMERALD_ORE: Block = Block(227);
    pub const RAW_IRON_BLOCK: Block = Block(228);
    pub const RAW_GOLD_BLOCK: Block = Block(229);
    pub const RAW_COPPER_BLOCK: Block = Block(230);
    pub const COPPER_BLOCK: Block = Block(231);
    pub const EMERALD_BLOCK: Block = Block(232);
    pub const GRANITE: Block = Block(233);
    pub const POLISHED_GRANITE: Block = Block(234);
    pub const DIORITE: Block = Block(235);
    pub const POLISHED_DIORITE: Block = Block(236);
    pub const ANDESITE: Block = Block(237);
    pub const POLISHED_ANDESITE: Block = Block(238);
    pub const TUFF: Block = Block(239);
    pub const CALCITE: Block = Block(240);
    /// Smelted from stone. Not placed by worldgen.
    pub const SMOOTH_STONE: Block = Block(241);
    /// Replaces stone below the shifted deepslate line (Java y = 0).
    pub const DEEPSLATE: Block = Block(242);
    pub const COBBLED_DEEPSLATE: Block = Block(243);
    pub const POLISHED_DEEPSLATE: Block = Block(244);
    pub const DEEPSLATE_COAL_ORE: Block = Block(245);
    pub const DEEPSLATE_IRON_ORE: Block = Block(246);
    pub const DEEPSLATE_COPPER_ORE: Block = Block(247);
    pub const DEEPSLATE_GOLD_ORE: Block = Block(248);
    pub const DEEPSLATE_REDSTONE_ORE: Block = Block(249);
    pub const DEEPSLATE_EMERALD_ORE: Block = Block(250);
    pub const DEEPSLATE_LAPIS_ORE: Block = Block(251);
    pub const DEEPSLATE_DIAMOND_ORE: Block = Block(252);
    pub const DARK_OAK_LOG: Block = Block(333);
    pub const MANGROVE_LOG: Block = Block(334);
    pub const CHERRY_LOG: Block = Block(335);
    pub const DARK_OAK_PLANKS: Block = Block(336);
    pub const MANGROVE_PLANKS: Block = Block(337);
    pub const CHERRY_PLANKS: Block = Block(338);
    /// Cobblestone already has stairs and a slab; only the wall is new.
    pub const COBBLESTONE_WALL: Block = Block(331);
    pub const STONE_BRICK_WALL: Block = Block(332);

    /// The stone-ore form of a deepslate ore, or `self` for everything else.
    /// Drops, fortune and smelting follow the stone ore.
    pub fn as_stone_ore(self) -> Block {
        match self {
            Block::DEEPSLATE_COAL_ORE => Block::COAL_ORE,
            Block::DEEPSLATE_IRON_ORE => Block::IRON_ORE,
            Block::DEEPSLATE_COPPER_ORE => Block::COPPER_ORE,
            Block::DEEPSLATE_GOLD_ORE => Block::GOLD_ORE,
            Block::DEEPSLATE_REDSTONE_ORE => Block::REDSTONE_ORE,
            Block::DEEPSLATE_EMERALD_ORE => Block::EMERALD_ORE,
            Block::DEEPSLATE_LAPIS_ORE => Block::LAPIS_ORE,
            Block::DEEPSLATE_DIAMOND_ORE => Block::DIAMOND_ORE,
            other => other,
        }
    }

    pub fn is_deepslate_ore(self) -> bool {
        (Block::DEEPSLATE_COAL_ORE.0..=Block::DEEPSLATE_DIAMOND_ORE.0).contains(&self.0)
    }

    /// Rails, ids 500..=509: Java `RailShape` north_south, east_west,
    /// ascending east/west/north/south, then south_east, south_west,
    /// north_west, north_east. All render flat (no slope mesh).
    pub const RAIL: Block = Block(500);

    pub const fn fire(age: u8) -> Block {
        Block(165 + if age > 15 { 15 } else { age as u16 })
    }

    pub fn fire_age(self) -> Option<u8> {
        (165..=180).contains(&self.0).then(|| (self.0 - 165) as u8)
    }

    #[inline]
    pub fn is_fire(self) -> bool {
        (165..=180).contains(&self.0)
    }

    /// Minecraft's (encouragement, consumption) fire odds. Wooden doors,
    /// ladders and containers can kindle lava fires but aren't consumed.
    pub fn fire_odds(self) -> (u8, u8) {
        match self.material() {
            b if b.is_log() => (5, 5),
            b if b.is_planks() => (5, 20),
            b if b.is_leaves() || b == Block::WOOL => (30, 60),
            Block::TNT => (15, 100),
            Block::TALL_GRASS
            | Block::FERN
            | Block::DEAD_BUSH
            | Block::DANDELION
            | Block::POPPY
            | Block::BLUE_ORCHID => (60, 100),
            b if b.is_mushroom() => (60, 100),
            b if b.is_sapling() => (60, 100),
            _ => (0, 0),
        }
    }

    pub fn ignited_by_lava(self) -> bool {
        self.fire_odds().0 > 0
            || self.is_bed()
            || matches!(
                self.base(),
                Block::OAK_DOOR | Block::LADDER | Block::CRAFTING_TABLE | Block::CHEST | Block::SMITHING_TABLE
            )
    }

    /// A complete top face can support fire, even on glass.
    pub fn supports_fire(self) -> bool {
        self.is_solid() && self.top_drop() == 0 && self.kind() != RenderKind::Shaped
    }

    pub const fn flowing_water(level: u8) -> Block {
        Block(23 + level as u16)
    }

    pub const fn flowing_lava(level: u8) -> Block {
        Block(37 + level as u16)
    }

    /// Wheat crops at growth `stage` 0..=7 (7 is ripe).
    pub const fn wheat(stage: u8) -> Block {
        Block(59 + stage as u16)
    }

    /// Growth stage of a wheat crop.
    pub fn crop_stage(self) -> Option<u8> {
        (59..=66).contains(&self.0).then(|| (self.0 - 59) as u8)
    }

    /// Nether wart at `age` 0..=3 (3 is ripe).
    pub const fn nether_wart(age: u8) -> Block {
        Block(184 + if age > 3 { 3 } else { age as u16 })
    }

    /// Age of a nether wart crop.
    pub fn wart_age(self) -> Option<u8> {
        (184..=187).contains(&self.0).then(|| (self.0 - 184) as u8)
    }

    /// A fence gate facing `facing`.
    pub const fn gate(facing: Facing, open: bool) -> Block {
        Block(141 + open as u16 * 4 + facing as u16)
    }

    /// One half of a door facing `facing`.
    pub const fn door(facing: Facing, open: bool, upper: bool) -> Block {
        Block(149 + upper as u16 * 8 + open as u16 * 4 + facing as u16)
    }

    /// The stairs cut from `base` (one of [`Block::SLAB_BASES`]), facing south.
    pub fn stairs_of(base: Block) -> Option<Block> {
        Self::SLAB_BASES.iter().position(|&b| b == base).map(stairs_id)
    }

    /// The full block stairs were cut from.
    pub fn stairs_base(self) -> Option<Block> {
        if let Some(super::forms::StoneForm::Stairs { index, .. }) = super::forms::stone_form(self.0) {
            return Some(super::forms::stone_base(index));
        }
        if let Some(super::forms::WoodForm::Stairs { index, .. }) = super::forms::wood_form(self.0) {
            return Some(super::forms::wood_planks(index));
        }
        stairs_index(self.0).map(|(i, _)| Self::SLAB_BASES[i])
    }

    /// The full block a wall was built from.
    pub fn wall_base(self) -> Option<Block> {
        match super::forms::stone_form(self.0) {
            Some(super::forms::StoneForm::Wall { index }) => Some(super::forms::wall_material(index)),
            _ => None,
        }
    }

    /// What kind of shaped block this is, with its state.
    pub fn shaped(self) -> Option<Shaped> {
        if let Some(shaped) = super::forms::as_shaped(self.0) {
            return Some(shaped);
        }
        let f = |i: u16| Facing::ALL[i as usize % 4];
        Some(match self.0 {
            #[cfg(test)]
            4094 => Shaped::Stairs(Facing::South),
            112..=135 | 189..=192 => Shaped::Stairs(f(stairs_index(self.0).unwrap().1)),
            136 | 182 => Shaped::Fence,
            188 => Shaped::BrewingStand,
            197 => Shaped::Pane,
            200..=207 => Shaped::Frame { facing: f(self.0 - 200), eye: self.0 >= 204 },
            208 => Shaped::EndPortal,
            209 => Shaped::DragonEgg,
            213 => Shaped::EnchantingTable,
            215..=220 => Shaped::Anvil { along_x: (self.0 - 215) % 2 == 1 },
            137..=140 => Shaped::Ladder(f(self.0 - 137)),
            141..=148 => Shaped::Gate { facing: f(self.0 - 141), open: self.0 >= 145 },
            149..=164 => {
                let i = self.0 - 149;
                Shaped::Door { facing: f(i), open: i % 8 >= 4, upper: i >= 8 }
            }
            500..=509 => Shaped::Rail,
            _ => return None,
        })
    }

    pub fn is_anvil(self) -> bool {
        matches!(self.shaped(), Some(Shaped::Anvil { .. }))
    }

    pub fn is_rail(self) -> bool {
        (500..=509).contains(&self.0)
    }

    pub const fn rail(shape: RailShape) -> Block {
        Block(500 + shape as u16)
    }

    pub fn rail_shape(self) -> Option<RailShape> {
        (500..=509).contains(&self.0).then(|| RailShape::ALL[(self.0 - 500) as usize])
    }

    /// The next worse anvil (`None` once a damaged one breaks), keeping
    /// its direction.
    pub fn anvil_damaged(self) -> Option<Block> {
        (self.is_anvil() && self.0 < 219).then(|| Block(self.0 + 2))
    }

    pub fn is_ladder(self) -> bool {
        matches!(self.shaped(), Some(Shaped::Ladder(_)))
    }

    pub fn is_door(self) -> bool {
        matches!(self.shaped(), Some(Shaped::Door { .. }))
    }

    pub fn is_door_upper(self) -> bool {
        matches!(self.shaped(), Some(Shaped::Door { upper: true, .. }))
    }

    pub fn is_gate(self) -> bool {
        matches!(self.shaped(), Some(Shaped::Gate { .. }))
    }

    /// The other state of a door half or gate (open <-> closed), facing
    /// `facing`.
    pub fn toggled(self, facing: Facing) -> Block {
        match self.shaped() {
            Some(Shaped::Gate { open, .. }) => match super::forms::gate_index(self) {
                Some(index) => super::forms::wood_id(index, 6 + (!open as u16) * 4 + facing as u16),
                None => Block::gate(facing, !open),
            },
            Some(Shaped::Door { open, upper, .. }) => match super::forms::wood_form(self.0) {
                Some(super::forms::WoodForm::Door { index, upper: true, .. }) => super::forms::wood_id(index, 22),
                Some(super::forms::WoodForm::Door { index, .. }) => {
                    super::forms::wood_id(index, 14 + (!open as u16) * 4 + facing as u16)
                }
                _ => Block::door(facing, !open, upper),
            },
            _ => self,
        }
    }

    /// Drawn as a flat sprite in inventories and when dropped.
    pub fn flat_icon(self) -> bool {
        self.kind() == RenderKind::Cross || self.is_ladder() || self.is_rail()
    }

    /// Badlands terracotta: 0 plain, then orange, yellow, red, brown, white
    /// and light grey.
    pub const fn terracotta(colour: u8) -> Block {
        Block(84 + colour as u16)
    }

    /// The kind of wood a log, leaves, planks or sapling block is made of.
    pub fn wood(self) -> Option<Wood> {
        Wood::ALL.into_iter().find(|w| [w.log(), w.leaves(), w.planks(), w.sapling()].contains(&self))
    }

    pub fn is_log(self) -> bool {
        matches!(self.0, 6 | 69..=72 | 333..=335)
    }

    pub fn is_leaves(self) -> bool {
        matches!(self.0, 7 | 23 | 73..=75)
    }

    pub fn is_planks(self) -> bool {
        matches!(self.0, 8 | 76..=79 | 336..=338)
    }

    pub fn is_sapling(self) -> bool {
        matches!(self.0, 67 | 68 | 80..=82)
    }

    pub fn is_farmland(self) -> bool {
        self == Block::FARMLAND || self == Block::WET_FARMLAND
    }

    /// The block items, recipes and rules use for an oriented block (a
    /// furnace or chest facing any way), and the way it faces.
    pub fn oriented(self) -> Option<(Block, Facing)> {
        if let Some(super::forms::StoneForm::Stairs { index, facing }) = super::forms::stone_form(self.0) {
            return Some((super::forms::stone_id(index, 0), facing));
        }
        match super::forms::wood_form(self.0) {
            Some(super::forms::WoodForm::Stairs { index, facing }) => {
                return Some((super::forms::wood_id(index, 0), facing));
            }
            Some(super::forms::WoodForm::Gate { index, facing, .. }) => {
                return Some((super::forms::wood_id(index, 6), facing));
            }
            Some(super::forms::WoodForm::Door { index, facing, .. }) => {
                return Some((super::forms::wood_id(index, 14), facing));
            }
            _ => {}
        }
        let f = |i: u16| Facing::ALL[i as usize];
        match self.0 {
            45 => Some((Block::FURNACE, Facing::South)),
            46 => Some((Block::LIT_FURNACE, Facing::South)),
            47..=49 => Some((Block::FURNACE, f(self.0 - 46))),
            50..=52 => Some((Block::LIT_FURNACE, f(self.0 - 49))),
            53..=56 => Some((Block::CHEST, f(self.0 - 53))),
            112..=135 | 189..=192 => {
                let (i, facing) = stairs_index(self.0).unwrap();
                Some((stairs_id(i), f(facing)))
            }
            200..=207 => Some((Block::END_PORTAL_FRAME, f((self.0 - 200) % 4))),
            137..=140 => Some((Block::LADDER, f(self.0 - 137))),
            141..=148 => Some((Block::FENCE_GATE, f((self.0 - 141) % 4))),
            149..=164 => Some((Block::OAK_DOOR, f((self.0 - 149) % 4))),
            215..=220 => {
                let along_x = (self.0 - 215) % 2 == 1;
                Some((Block(self.0 - along_x as u16), if along_x { Facing::East } else { Facing::South }))
            }
            _ => None,
        }
    }

    /// This block without its orientation (itself if it has none).
    pub fn base(self) -> Block {
        if self.is_rail() { Block::RAIL } else { self.oriented().map_or(self, |(b, _)| b) }
    }

    /// The same block facing `facing` (unchanged if it has no front).
    pub fn with_facing(self, facing: Facing) -> Block {
        let i = facing as u16;
        match self.base() {
            // Frames keep their eye.
            Block::END_PORTAL_FRAME => Block(200 + i + if self.0 >= 204 { 4 } else { 0 }),
            // Anvils turn broadside to whoever places them (Java's facing
            // is the placer's clockwise), so the top runs across their view.
            b @ (Block::ANVIL | Block::CHIPPED_ANVIL | Block::DAMAGED_ANVIL) => Block(b.0 + !facing.along_x() as u16),
            b if i == 0 => b,
            Block::FURNACE => Block(46 + i),
            Block::LIT_FURNACE => Block(49 + i),
            Block::CHEST => Block(53 + i),
            Block::LADDER => Block(137 + i),
            Block::FENCE_GATE => Block::gate(facing, false),
            b if let Some(index) = super::forms::gate_index(b) => super::forms::wood_id(index, 6 + i),
            b if b.stairs_base().is_some() => Block(b.0 + i),
            _ => self,
        }
    }

    /// Index of a live state. Ids are kept below [`STATE_CAPACITY`], so the
    /// load is unchecked: a bounds check would not be a single indexed load.
    #[inline(always)]
    fn slot(self) -> usize {
        let i = self.0 as usize;
        debug_assert!(i < STATE_CAPACITY);
        i
    }

    #[inline(always)]
    pub fn info(self) -> &'static BlockInfo {
        // SAFETY: `slot` is < STATE_CAPACITY for every id the game constructs.
        unsafe { INFO.get_unchecked(self.slot()) }
    }

    #[inline(always)]
    pub fn kind(self) -> RenderKind {
        self.info().kind
    }

    #[inline(always)]
    pub fn is_opaque(self) -> bool {
        // SAFETY: same bound as `info`.
        unsafe { *OPAQUE.get_unchecked(self.slot()) }
    }

    #[inline(always)]
    pub fn is_water(self) -> bool {
        self.0 == 5 || (24..=31).contains(&self.0)
    }

    /// Water flow level: 0 for sources and falling water, 1..=7 for flowing.
    pub fn water_level(self) -> Option<u8> {
        match self.0 {
            5 | 31 => Some(0),
            24..=30 => Some((self.0 - 23) as u8),
            _ => None,
        }
    }

    #[inline(always)]
    pub fn is_lava(self) -> bool {
        (37..=41).contains(&self.0)
    }

    #[inline(always)]
    pub fn fluid(self) -> Option<Fluid> {
        match self.0 {
            5 | 24..=31 => Some(Fluid::Water),
            37..=41 => Some(Fluid::Lava),
            _ => None,
        }
    }

    #[inline(always)]
    pub fn is_fluid(self) -> bool {
        self.fluid().is_some()
    }

    /// Flow level of any fluid: 0 for sources and falling fluid, then
    /// 1..=`max_level` for flowing.
    pub fn fluid_level(self) -> Option<u8> {
        match self.0 {
            5 | 31 | 37 | 41 => Some(0),
            24..=30 => Some((self.0 - 23) as u8),
            38..=40 => Some((self.0 - 37) as u8),
            _ => None,
        }
    }

    /// How far (in 1/16 block) this fluid's surface sits below the top of
    /// its cell when no fluid of the same kind is above it. Lava's three
    /// levels drop like water's levels 2, 4 and 6.
    pub fn fluid_drop(self) -> u8 {
        match self.0 {
            5 | 37 => 2,
            24..=30 => 2 + ((self.0 - 23) * 12 / 7) as u8,
            38..=40 => 2 + ((self.0 - 37) * 24 / 7) as u8,
            _ => 0,
        }
    }

    /// The full blocks slabs are cut from, in slab id order.
    pub const SLAB_BASES: [Block; 7] = [
        Block::STONE,
        Block::COBBLESTONE,
        Block::PLANKS,
        Block::SANDSTONE,
        Block::BRICKS,
        Block::NETHER_BRICKS,
        Block::STONE_BRICKS,
    ];

    /// The slab cut from `base`, if there is one.
    pub fn slab_of(base: Block) -> Option<Block> {
        Self::SLAB_BASES.iter().position(|&b| b == base).map(slab_id)
    }

    /// The full block a slab was cut from (two stacked slabs make it).
    pub fn slab_base(self) -> Option<Block> {
        if let Some(super::forms::StoneForm::Slab { index }) = super::forms::stone_form(self.0) {
            return Some(super::forms::stone_base(index));
        }
        if let Some(super::forms::WoodForm::Slab { index }) = super::forms::wood_form(self.0) {
            return Some(super::forms::wood_planks(index));
        }
        match self.0 {
            106..=111 => Some(Self::SLAB_BASES[self.0 as usize - 106]),
            193 => Some(Block::STONE_BRICKS),
            _ => None,
        }
    }

    pub fn is_slab(self) -> bool {
        self.slab_base().is_some()
    }

    /// What mining rules treat this block as: a slab mines like its full
    /// block, an oriented block like its plain form.
    fn material(self) -> Block {
        match self.base() {
            Block::OAK_FENCE | Block::FENCE_GATE => Block::PLANKS,
            Block::NETHER_BRICK_FENCE => Block::NETHER_BRICKS,
            b => {
                b.slab_base().or(b.stairs_base()).or(b.wall_base()).or_else(|| super::forms::planks_of(b)).unwrap_or(b)
            }
        }
    }

    pub fn is_bed(self) -> bool {
        matches!(self, Block::BED_FOOT | Block::BED_HEAD)
    }

    /// How far (in 1/16 block) the top of this block sits below the top of
    /// its cell; 0 for full blocks.
    pub fn top_drop(self) -> u8 {
        match self {
            b if b.is_bed() => 7,
            b if b.is_slab() => 8,
            _ => 0,
        }
    }

    /// Height of the block's collision box.
    pub fn height(self) -> f64 {
        // Soul sand looks like a full block, but you sink into it a little.
        let sink = if self == Block::SOUL_SAND { 2.0 } else { 0.0 };
        1.0 - (self.top_drop() as f64 + sink) / 16.0
    }

    /// What breaking this block yields in survival.
    pub fn drop(self) -> Option<Item> {
        match self.base().as_stone_ore() {
            Block::STONE => Some(Block::COBBLESTONE.into()),
            Block::DEEPSLATE => Some(Block::COBBLED_DEEPSLATE.into()),
            Block::GRASS | Block::SNOWY_GRASS => Some(Block::DIRT.into()),
            Block::COAL_ORE => Some(Item::COAL),
            Block::IRON_ORE => Some(Item::RAW_IRON),
            Block::GOLD_ORE => Some(Item::RAW_GOLD),
            Block::DIAMOND_ORE => Some(Item::DIAMOND),
            Block::EMERALD_ORE => Some(Item::EMERALD),
            // Copper (2-5) and redstone (4-5) vary; see `World::spill_mined`.
            Block::COPPER_ORE | Block::REDSTONE_ORE => None,
            // Lapis ore drops 4-9 (see `World::spill_mined`).
            Block::LAPIS_ORE => None,
            Block::DEAD_BUSH => Some(Item::STICK),
            Block::LIT_FURNACE => Some(Block::FURNACE.into()),
            Block::FARMLAND | Block::WET_FARMLAND => Some(Block::DIRT.into()),
            b if b.crop_stage() == Some(7) => Some(Item::WHEAT),
            b if b.crop_stage().is_some() => Some(Item::WHEAT_SEEDS),
            b if b.wart_age().is_some() => Some(Item::NETHER_WART),
            Block::CLAY => Some(Item::CLAY_BALL),
            Block::MELON => Some(Item::MELON_SLICE),
            // The foot drops the bed; breaking either half breaks both.
            Block::BED_FOOT => Some(Item::BED),
            Block::BED_HEAD => None,
            // Only the lower half of a door drops it.
            Block::OAK_DOOR => (!self.is_door_upper()).then_some(Item::OAK_DOOR),
            b if super::forms::planks_of(b).is_some() && b.is_door() => {
                (!self.is_door_upper()).then(|| Item::from_block(b))
            }
            Block::QUARTZ_ORE => Some(Item::NETHER_QUARTZ),
            // Glowstone breaks into dust (see `World::spill_block`).
            Block::GLOWSTONE | Block::NETHER_PORTAL | Block::SPAWNER => None,
            Block::COBWEB => Some(Item::STRING),
            // Bookshelves drop three books (see `World::spill_block`).
            Block::BOOKSHELF | Block::END_PORTAL_FRAME | Block::END_PORTAL | Block::END_GATEWAY => None,
            b if b.is_leaves() => None,
            Block::GLASS | Block::BEDROCK | Block::TALL_GRASS | Block::FERN | Block::ICE => None,
            b if b.is_fluid() || b.is_fire() || b == Block::AIR => None,
            b => Some(b.into()),
        }
    }

    /// Minecraft's hardness: mining takes 1.5x this many seconds when the
    /// held item can harvest the block and 5x when it can't, divided by the
    /// tool's speed (see `crate::mining`). Infinite for unbreakable blocks.
    pub fn hardness(self) -> f32 {
        if self.is_door() {
            return 3.0;
        }
        match self.material() {
            Block::COBWEB => 4.0,
            b if b.kind() == RenderKind::Cross => 0.0,
            Block::TNT => 0.0,
            b if b.is_leaves() => 0.2,
            Block::SNOW => 0.2,
            Block::GLASS | Block::GLOWSTONE => 0.3,
            Block::CACTUS | Block::NETHERRACK => 0.4,
            Block::SOUL_SAND => 0.5,
            Block::NETHER_BRICKS => 2.0,
            Block::BREWING_STAND => 0.5,
            Block::STONE_BRICKS | Block::MOSSY_STONE_BRICKS | Block::CRACKED_STONE_BRICKS => 1.5,
            Block::IRON_BARS => 5.0,
            Block::BOOKSHELF => 1.5,
            Block::END_PORTAL_FRAME | Block::END_PORTAL | Block::END_GATEWAY => f32::INFINITY,
            Block::DRAGON_EGG => 3.0,
            Block::ANCIENT_DEBRIS => 30.0,
            Block::NETHERITE_BLOCK => 50.0,
            Block::QUARTZ_ORE | Block::END_STONE | Block::LAPIS_ORE | Block::LAPIS_BLOCK => 3.0,
            Block::ENCHANTING_TABLE
            | Block::IRON_BLOCK
            | Block::ANVIL
            | Block::CHIPPED_ANVIL
            | Block::DAMAGED_ANVIL => 5.0,
            Block::NETHER_PORTAL => f32::INFINITY,
            Block::DIRT | Block::SAND | Block::RED_SAND | Block::ICE => 0.5,
            Block::GRASS | Block::SNOWY_GRASS | Block::GRAVEL | Block::FARMLAND | Block::WET_FARMLAND | Block::CLAY => {
                0.6
            }
            Block::SANDSTONE | Block::WOOL => 0.8,
            Block::RAIL => 0.7,
            Block::BED_FOOT | Block::BED_HEAD => 0.2,
            Block::LADDER => 0.4,
            Block::OAK_DOOR => 3.0,
            Block::PUMPKIN | Block::MELON => 1.0,
            b if b.terracotta_colour().is_some() => 1.25,
            Block::STONE => 1.5,
            b if b.is_log() || b.is_planks() => 2.0,
            Block::COBBLESTONE | Block::MOSSY_COBBLESTONE | Block::BRICKS => 2.0,
            Block::CRAFTING_TABLE | Block::CHEST | Block::SMITHING_TABLE => 2.5,
            Block::COAL_ORE
            | Block::IRON_ORE
            | Block::GOLD_ORE
            | Block::DIAMOND_ORE
            | Block::COPPER_ORE
            | Block::REDSTONE_ORE
            | Block::EMERALD_ORE
            | Block::COPPER_BLOCK => 3.0,
            Block::RAW_IRON_BLOCK | Block::RAW_GOLD_BLOCK | Block::RAW_COPPER_BLOCK | Block::EMERALD_BLOCK => 5.0,
            Block::CALCITE => 0.75,
            Block::GRANITE
            | Block::POLISHED_GRANITE
            | Block::DIORITE
            | Block::POLISHED_DIORITE
            | Block::ANDESITE
            | Block::POLISHED_ANDESITE
            | Block::TUFF => 1.5,
            Block::SMOOTH_STONE => 2.0,
            Block::DEEPSLATE => 3.0,
            Block::COBBLED_DEEPSLATE | Block::POLISHED_DEEPSLATE => 3.5,
            b if b.is_deepslate_ore() => 4.5,
            Block::FURNACE | Block::LIT_FURNACE => 3.5,
            Block::SPAWNER => 5.0,
            Block::OBSIDIAN => 50.0,
            Block::BEDROCK | Block::AIR => f32::INFINITY,
            b if b.is_fluid() => f32::INFINITY,
            _ => 1.0,
        }
    }

    /// The tool kind that mines this block faster.
    pub fn best_tool(self) -> Option<ToolKind> {
        match self.material() {
            Block::STONE
            | Block::COBBLESTONE
            | Block::MOSSY_COBBLESTONE
            | Block::BRICKS
            | Block::SANDSTONE
            | Block::COAL_ORE
            | Block::IRON_ORE
            | Block::GOLD_ORE
            | Block::DIAMOND_ORE
            | Block::COPPER_ORE
            | Block::REDSTONE_ORE
            | Block::EMERALD_ORE
            | Block::RAW_IRON_BLOCK
            | Block::RAW_GOLD_BLOCK
            | Block::RAW_COPPER_BLOCK
            | Block::COPPER_BLOCK
            | Block::EMERALD_BLOCK
            | Block::OBSIDIAN
            | Block::FURNACE
            | Block::LIT_FURNACE
            | Block::NETHERRACK
            | Block::QUARTZ_ORE
            | Block::END_STONE
            | Block::NETHER_BRICKS
            | Block::SPAWNER
            | Block::BREWING_STAND
            | Block::STONE_BRICKS
            | Block::MOSSY_STONE_BRICKS
            | Block::CRACKED_STONE_BRICKS
            | Block::IRON_BARS
            | Block::LAPIS_ORE
            | Block::LAPIS_BLOCK
            | Block::ANCIENT_DEBRIS
            | Block::NETHERITE_BLOCK
            | Block::ENCHANTING_TABLE
            | Block::IRON_BLOCK
            | Block::ANVIL
            | Block::CHIPPED_ANVIL
            | Block::DAMAGED_ANVIL
            | Block::ICE
            | Block::GRANITE
            | Block::POLISHED_GRANITE
            | Block::DIORITE
            | Block::POLISHED_DIORITE
            | Block::ANDESITE
            | Block::POLISHED_ANDESITE
            | Block::TUFF
            | Block::CALCITE
            | Block::SMOOTH_STONE
            | Block::DEEPSLATE
            | Block::COBBLED_DEEPSLATE
            | Block::POLISHED_DEEPSLATE
            | Block::RAIL => Some(ToolKind::Pickaxe),
            b if b.is_deepslate_ore() => Some(ToolKind::Pickaxe),
            Block::COBWEB => Some(ToolKind::Sword),
            Block::BOOKSHELF => Some(ToolKind::Axe),
            b if b.terracotta_colour().is_some() => Some(ToolKind::Pickaxe),
            Block::DIRT
            | Block::GRASS
            | Block::SNOWY_GRASS
            | Block::SAND
            | Block::GRAVEL
            | Block::SNOW
            | Block::FARMLAND
            | Block::WET_FARMLAND
            | Block::RED_SAND
            | Block::SOUL_SAND
            | Block::CLAY => Some(ToolKind::Shovel),
            b if b.is_log() || b.is_planks() => Some(ToolKind::Axe),
            Block::CRAFTING_TABLE
            | Block::CHEST
            | Block::PUMPKIN
            | Block::MELON
            | Block::LADDER
            | Block::OAK_DOOR
            | Block::SMITHING_TABLE => Some(ToolKind::Axe),
            _ => None,
        }
    }

    /// Pickaxe harvest level needed for any drop (0 wood or gold, 1 stone,
    /// 2 iron, 3 diamond); `None` if a bare hand will do.
    pub fn harvest_level(self) -> Option<u8> {
        match self.material().as_stone_ore() {
            Block::STONE
            | Block::COBBLESTONE
            | Block::MOSSY_COBBLESTONE
            | Block::BRICKS
            | Block::SANDSTONE
            | Block::COAL_ORE
            | Block::FURNACE
            | Block::LIT_FURNACE
            | Block::NETHERRACK
            | Block::QUARTZ_ORE
            | Block::END_STONE
            | Block::SPAWNER
            | Block::BREWING_STAND
            | Block::STONE_BRICKS
            | Block::MOSSY_STONE_BRICKS
            | Block::CRACKED_STONE_BRICKS
            | Block::IRON_BARS
            | Block::COBWEB
            | Block::ENCHANTING_TABLE
            | Block::ANVIL
            | Block::CHIPPED_ANVIL
            | Block::DAMAGED_ANVIL
            | Block::NETHER_BRICKS
            | Block::GRANITE
            | Block::POLISHED_GRANITE
            | Block::DIORITE
            | Block::POLISHED_DIORITE
            | Block::ANDESITE
            | Block::POLISHED_ANDESITE
            | Block::TUFF
            | Block::CALCITE
            | Block::SMOOTH_STONE
            | Block::DEEPSLATE
            | Block::COBBLED_DEEPSLATE
            | Block::POLISHED_DEEPSLATE => Some(0),
            b if b.terracotta_colour().is_some() => Some(0),
            Block::IRON_ORE
            | Block::COPPER_ORE
            | Block::LAPIS_ORE
            | Block::LAPIS_BLOCK
            | Block::IRON_BLOCK
            | Block::RAW_IRON_BLOCK
            | Block::RAW_COPPER_BLOCK
            | Block::COPPER_BLOCK => Some(1),
            Block::GOLD_ORE
            | Block::DIAMOND_ORE
            | Block::REDSTONE_ORE
            | Block::EMERALD_ORE
            | Block::RAW_GOLD_BLOCK
            | Block::EMERALD_BLOCK => Some(2),
            Block::OBSIDIAN | Block::ANCIENT_DEBRIS | Block::NETHERITE_BLOCK => Some(3),
            _ => None,
        }
    }

    /// Every block a creative player can pick from.
    pub fn creative_palette() -> impl Iterator<Item = Block> {
        (1..=23u16)
            .chain(32..=37)
            .chain(42..=45)
            .chain([53, 57, 67, 68])
            .chain(69..=97)
            .chain(100..=103)
            .chain(105..=111)
            .chain((112..=132).step_by(4))
            .chain([
                136, 137, 141, 182, 188, 189, 193, 194, 195, 196, 197, 198, 199, 200, 209, 211, 212, 213, 214, 215,
                217, 219, 221, 222, 223, 224, 500,
            ])
            .chain([520, 521])
            .chain(225..=252)
            .chain(super::forms::palette_ids())
            .map(Block)
    }

    /// Blocks that placing another block overwrites (air, fluids, grass).
    pub fn is_replaceable(self) -> bool {
        self == Block::AIR
            || self.is_fluid()
            || self.is_fire()
            || self == Block::TALL_GRASS
            || self == Block::DEAD_BUSH
            || self == Block::FERN
    }

    /// Colour index of a terracotta block (see [`Block::terracotta`]).
    pub fn terracotta_colour(self) -> Option<u8> {
        (84..=90).contains(&self.0).then(|| (self.0 - 84) as u8)
    }

    /// Whether the crosshair can select this block (anything visible but fluids).
    #[inline(always)]
    pub fn is_targetable(self) -> bool {
        self.kind() != RenderKind::Invisible && !self.is_fluid() && self != Block::NETHER_PORTAL
    }

    /// Sand, gravel and the dragon egg fall when nothing holds them up.
    pub fn has_gravity(self) -> bool {
        matches!(self, Block::SAND | Block::RED_SAND | Block::GRAVEL | Block::DRAGON_EGG) || self.is_anvil()
    }

    /// Whether this block can rest on `below`. Plants need soil and torches
    /// a full block; everything else stays put.
    pub fn can_stay_on(self, below: Block) -> bool {
        match self {
            b if b.is_mushroom() => below.is_opaque(),
            Block::TALL_GRASS | Block::DANDELION | Block::POPPY | Block::FERN | Block::BLUE_ORCHID => {
                matches!(below, Block::GRASS | Block::DIRT | Block::SNOWY_GRASS)
            }
            Block::DEAD_BUSH => {
                matches!(below, Block::SAND | Block::RED_SAND | Block::DIRT | Block::GRASS)
                    || below.terracotta_colour().is_some()
            }
            // Sugar cane also needs water next to its lowest block; see
            // `World::cane_has_water`.
            Block::SUGAR_CANE => {
                matches!(below, Block::SUGAR_CANE | Block::GRASS | Block::DIRT | Block::SAND | Block::RED_SAND)
            }
            Block::TORCH => below.is_opaque(),
            b if b.is_rail() => below.is_opaque(),
            b if b.is_door() => {
                if b.is_door_upper() {
                    below.is_door() && !below.is_door_upper()
                } else {
                    below.is_opaque()
                }
            }
            b if b.is_sapling() => {
                matches!(below, Block::GRASS | Block::DIRT | Block::SNOWY_GRASS) || below.is_farmland()
            }
            b if b.crop_stage().is_some() => below.is_farmland(),
            b if b.wart_age().is_some() => below == Block::SOUL_SAND,
            _ => true,
        }
    }

    #[inline(always)]
    pub fn is_solid(self) -> bool {
        self.info().solid
    }

    pub fn name(self) -> &'static str {
        self.info().name
    }

    /// Looks a block up by name (spaces or underscores).
    pub fn from_name(name: &str) -> Option<Block> {
        let name = name.replace('_', " ");
        (0..STATE_CAPACITY as u16)
            .map(Block)
            .find(|b| b.kind() != RenderKind::Invisible && b.name() == name)
            .or((name == "air").then_some(Block::AIR))
    }

    /// Light opacity: propagation loses at least one level per step,
    /// or this value if larger. 15 fully blocks light.
    #[inline(always)]
    pub fn light_opacity(self) -> u8 {
        // SAFETY: same bound as `info`.
        unsafe { *LIGHT_OPACITY.get_unchecked(self.slot()) }
    }

    /// Blocks light but doesn't fill its cell (slabs, stairs): lit like
    /// the brightest cell beside or above it.
    #[inline(always)]
    pub fn borrows_light(self) -> bool {
        // SAFETY: same bound as `info`.
        unsafe { *BORROWS_LIGHT.get_unchecked(self.slot()) }
    }

    /// Light level emitted by this block.
    #[inline(always)]
    pub fn emission(self) -> u8 {
        match self.base() {
            Block::GLOWSTONE => 15,
            Block::TORCH => 14,
            Block::LIT_FURNACE => 13,
            Block::NETHER_PORTAL => 11,
            Block::END_PORTAL | Block::END_GATEWAY => 15,
            // Java: all frame states glow faintly, with or without an eye.
            Block::END_PORTAL_FRAME | Block::DRAGON_EGG => 1,
            Block::ENCHANTING_TABLE => 7,
            b if b.is_lava() => 15,
            b if b.is_fire() => 15,
            _ => 0,
        }
    }
}

/// Id of the slab of [`Block::SLAB_BASES`] entry `i`.
const fn slab_id(i: usize) -> Block {
    if i < 6 { Block(106 + i as u16) } else { Block::STONE_BRICK_SLAB }
}

/// Id of the south-facing stairs of [`Block::SLAB_BASES`] entry `i`.
const fn stairs_id(i: usize) -> Block {
    if i < 6 { Block(112 + i as u16 * 4) } else { Block::STONE_BRICK_STAIRS }
}

/// The slab material index and facing index of a stairs id.
const fn stairs_index(id: u16) -> Option<(usize, u16)> {
    match id {
        112..=135 => Some(((id as usize - 112) / 4, (id - 112) % 4)),
        189..=192 => Some((6, id - 189)),
        _ => None,
    }
}

/// A kind of tree: its log, leaves, planks and sapling blocks.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Wood {
    Oak,
    Spruce,
    Birch,
    Jungle,
    Acacia,
}

impl Wood {
    pub const ALL: [Wood; 5] = [Wood::Oak, Wood::Spruce, Wood::Birch, Wood::Jungle, Wood::Acacia];

    pub const fn log(self) -> Block {
        match self {
            Wood::Oak => Block::LOG,
            Wood::Spruce => Block::SPRUCE_LOG,
            Wood::Birch => Block::BIRCH_LOG,
            Wood::Jungle => Block::JUNGLE_LOG,
            Wood::Acacia => Block::ACACIA_LOG,
        }
    }

    pub const fn leaves(self) -> Block {
        match self {
            Wood::Oak => Block::LEAVES,
            Wood::Spruce => Block::SPRUCE_LEAVES,
            Wood::Birch => Block::BIRCH_LEAVES,
            Wood::Jungle => Block::JUNGLE_LEAVES,
            Wood::Acacia => Block::ACACIA_LEAVES,
        }
    }

    pub const fn planks(self) -> Block {
        match self {
            Wood::Oak => Block::PLANKS,
            Wood::Spruce => Block::SPRUCE_PLANKS,
            Wood::Birch => Block::BIRCH_PLANKS,
            Wood::Jungle => Block::JUNGLE_PLANKS,
            Wood::Acacia => Block::ACACIA_PLANKS,
        }
    }

    pub const fn sapling(self) -> Block {
        match self {
            Wood::Oak => Block::OAK_SAPLING,
            Wood::Spruce => Block::SPRUCE_SAPLING,
            Wood::Birch => Block::BIRCH_SAPLING,
            Wood::Jungle => Block::JUNGLE_SAPLING,
            Wood::Acacia => Block::ACACIA_SAPLING,
        }
    }
}

/// Which way the front of a furnace or chest faces.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Facing {
    /// +Z
    South,
    /// -Z
    North,
    /// +X
    East,
    /// -X
    West,
}

impl Facing {
    pub const ALL: [Facing; 4] = [Facing::South, Facing::North, Facing::East, Facing::West];

    /// Unit step out of the side this faces.
    pub const fn offset(self) -> glam::IVec3 {
        match self {
            Facing::South => glam::IVec3::Z,
            Facing::North => glam::IVec3::NEG_Z,
            Facing::East => glam::IVec3::X,
            Facing::West => glam::IVec3::NEG_X,
        }
    }

    pub const fn opposite(self) -> Facing {
        match self {
            Facing::South => Facing::North,
            Facing::North => Facing::South,
            Facing::East => Facing::West,
            Facing::West => Facing::East,
        }
    }

    /// A quarter turn clockwise seen from above.
    pub const fn clockwise(self) -> Facing {
        match self {
            Facing::South => Facing::West,
            Facing::West => Facing::North,
            Facing::North => Facing::East,
            Facing::East => Facing::South,
        }
    }

    /// Whether this faces along X (east or west).
    pub const fn along_x(self) -> bool {
        matches!(self, Facing::East | Facing::West)
    }

    /// The facing pointing along a horizontal unit offset.
    pub fn from_offset(d: glam::IVec3) -> Option<Facing> {
        Facing::ALL.into_iter().find(|f| f.offset() == d)
    }

    /// Index into [`BlockInfo::tex`] (+X, -X, +Y, -Y, +Z, -Z).
    pub const fn face(self) -> usize {
        match self {
            Facing::East => 0,
            Facing::West => 1,
            Facing::South => 4,
            Facing::North => 5,
        }
    }

    /// The front a block placed by someone looking along `forward` gets:
    /// facing back at them, like Minecraft.
    pub fn toward(forward: glam::Vec3) -> Facing {
        if forward.x.abs() > forward.z.abs() {
            if forward.x > 0.0 { Facing::West } else { Facing::East }
        } else if forward.z > 0.0 {
            Facing::North
        } else {
            Facing::South
        }
    }
}

/// The state of a [`RenderKind::Shaped`] block (see `world::shape`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shaped {
    /// The low step faces this way; the tall half is behind it.
    Stairs(Facing),
    /// Three stone plates and a rod.
    BrewingStand,
    /// Iron bars: a post with arms out to what they join.
    Pane,
    /// An End portal frame, 13/16 tall, with the eye on top once filled.
    Frame {
        facing: Facing,
        eye: bool,
    },
    /// The End portal's surface, 3/4 up its cell.
    EndPortal,
    /// Stacked boxes rounding to a point.
    DragonEgg,
    /// A 12/16-high table.
    EnchantingTable,
    /// Java's anvil: a base, a neck and a top running along x or z.
    Anvil {
        along_x: bool,
    },
    Fence,
    /// Faces away from the wall it hangs on.
    Ladder(Facing),
    /// Faces whoever placed (or last opened) it; it runs across that way.
    Gate {
        facing: Facing,
        open: bool,
    },
    /// Closed, the panel lies along the side it faces.
    Door {
        facing: Facing,
        open: bool,
        upper: bool,
    },
    /// A cobblestone-style wall: a post and arms out to what it joins.
    Wall,
    /// A 2/16-high rail; Java's slope states still render flat.
    Rail,
}

/// Java's `RailShape` for vanilla rails (block ids 500..=509).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u16)]
pub enum RailShape {
    NorthSouth = 0,
    EastWest = 1,
    AscendingEast = 2,
    AscendingWest = 3,
    AscendingNorth = 4,
    AscendingSouth = 5,
    SouthEast = 6,
    SouthWest = 7,
    NorthWest = 8,
    NorthEast = 9,
}

impl RailShape {
    pub const ALL: [RailShape; 10] = [
        Self::NorthSouth,
        Self::EastWest,
        Self::AscendingEast,
        Self::AscendingWest,
        Self::AscendingNorth,
        Self::AscendingSouth,
        Self::SouthEast,
        Self::SouthWest,
        Self::NorthWest,
        Self::NorthEast,
    ];
}

/// A block type that flows: each has a source, a falling form and
/// `max_level` flowing levels.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fluid {
    Water,
    Lava,
}

impl Fluid {
    pub const fn source(self) -> Block {
        match self {
            Fluid::Water => Block::WATER,
            Fluid::Lava => Block::LAVA,
        }
    }

    pub const fn falling(self) -> Block {
        match self {
            Fluid::Water => Block::FALLING_WATER,
            Fluid::Lava => Block::FALLING_LAVA,
        }
    }

    pub const fn flowing(self, level: u8) -> Block {
        match self {
            Fluid::Water => Block::flowing_water(level),
            Fluid::Lava => Block::flowing_lava(level),
        }
    }

    /// Weakest flowing level: how far the fluid spreads over flat ground.
    pub const fn max_level(self) -> u8 {
        match self {
            Fluid::Water => 7,
            Fluid::Lava => 3,
        }
    }
}

const fn all(t: u16) -> [u16; 6] {
    [t; 6]
}

const fn column(side: u16, top: u16, bottom: u16) -> [u16; 6] {
    [side, side, top, bottom, side, side]
}

/// Side textures with `front` on the face `facing` points out of.
const fn fronted(front: u16, side: u16, top: u16, facing: Facing) -> [u16; 6] {
    let mut t = [side, side, top, top, side, side];
    t[facing.face()] = front;
    t
}

const fn make(id: u16) -> BlockInfo {
    use RenderKind::*;
    let (name, kind, tex) = match id {
        #[cfg(test)]
        4095 => ("test high cube", Opaque, all(2047)),
        #[cfg(test)]
        4094 => ("test high stairs", Shaped, all(1536)),
        #[cfg(test)]
        4093 => ("test high cross", Cross, all(1024)),
        0 => ("air", Invisible, all(0)),
        1 => ("stone", Opaque, all(tex::STONE)),
        2 => ("dirt", Opaque, all(tex::DIRT)),
        3 => ("grass", Opaque, column(tex::GRASS_SIDE, tex::GRASS_TOP, tex::DIRT)),
        4 => ("sand", Opaque, all(tex::SAND)),
        5 => ("water", Translucent, all(tex::WATER)),
        6 => ("oak log", Opaque, column(tex::LOG_SIDE, tex::LOG_TOP, tex::LOG_TOP)),
        7 => ("oak leaves", Cutout, all(tex::LEAVES)),
        8 => ("oak planks", Opaque, all(tex::PLANKS)),
        9 => ("cobblestone", Opaque, all(tex::COBBLESTONE)),
        10 => ("glass", Cutout, all(tex::GLASS)),
        11 => ("bedrock", Opaque, all(tex::BEDROCK)),
        12 => ("gravel", Opaque, all(tex::GRAVEL)),
        13 => ("snow", Opaque, all(tex::SNOW)),
        14 => ("snowy grass", Opaque, column(tex::SNOWY_GRASS_SIDE, tex::SNOW, tex::DIRT)),
        15 => ("coal ore", Opaque, all(tex::COAL_ORE)),
        16 => ("iron ore", Opaque, all(tex::IRON_ORE)),
        17 => ("gold ore", Opaque, all(tex::GOLD_ORE)),
        18 => ("diamond ore", Opaque, all(tex::DIAMOND_ORE)),
        19 => ("cactus", Opaque, column(tex::CACTUS_SIDE, tex::CACTUS_TOP, tex::CACTUS_TOP)),
        20 => ("bricks", Opaque, all(tex::BRICKS)),
        21 => ("sandstone", Opaque, column(tex::SANDSTONE_SIDE, tex::SANDSTONE_TOP, tex::SANDSTONE_TOP)),
        22 => ("glowstone", Opaque, all(tex::GLOWSTONE)),
        23 => ("spruce leaves", Cutout, all(tex::SPRUCE_LEAVES)),
        24..=30 => ("flowing water", Translucent, all(tex::WATER)),
        31 => ("falling water", Translucent, all(tex::WATER)),
        32 => ("tall grass", Cross, all(tex::TALL_GRASS)),
        33 => ("dandelion", Cross, all(tex::DANDELION)),
        34 => ("poppy", Cross, all(tex::POPPY)),
        35 => ("dead bush", Cross, all(tex::DEAD_BUSH)),
        36 => ("torch", Cross, all(tex::TORCH)),
        37 => ("lava", Translucent, all(tex::LAVA)),
        38..=40 => ("flowing lava", Translucent, all(tex::LAVA)),
        41 => ("falling lava", Translucent, all(tex::LAVA)),
        42 => ("obsidian", Opaque, all(tex::OBSIDIAN)),
        43 => ("wool", Opaque, all(tex::WOOL)),
        44 => ("crafting table", Opaque, column(tex::TABLE_SIDE, tex::TABLE_TOP, tex::PLANKS)),
        45 | 47..=49 => {
            let f = if id == 45 { Facing::South } else { Facing::ALL[id as usize - 46] };
            ("furnace", Opaque, fronted(tex::FURNACE_FRONT, tex::FURNACE_SIDE, tex::FURNACE_TOP, f))
        }
        46 | 50..=52 => {
            let f = if id == 46 { Facing::South } else { Facing::ALL[id as usize - 49] };
            ("lit furnace", Opaque, fronted(tex::FURNACE_LIT, tex::FURNACE_SIDE, tex::FURNACE_TOP, f))
        }
        53..=56 => {
            let f = Facing::ALL[id as usize - 53];
            ("chest", Opaque, fronted(tex::CHEST_FRONT, tex::CHEST_SIDE, tex::CHEST_TOP, f))
        }
        57 => ("farmland", Opaque, column(tex::DIRT, tex::FARMLAND, tex::DIRT)),
        58 => ("wet farmland", Opaque, column(tex::DIRT, tex::WET_FARMLAND, tex::DIRT)),
        59..=66 => ("wheat crops", Cross, all(tex::WHEAT_0 + (id - 59))),
        67 => ("oak sapling", Cross, all(tex::OAK_SAPLING)),
        68 => ("spruce sapling", Cross, all(tex::SPRUCE_SAPLING)),
        69 => ("spruce log", Opaque, column(tex::SPRUCE_LOG_SIDE, tex::SPRUCE_LOG_TOP, tex::SPRUCE_LOG_TOP)),
        70 => ("birch log", Opaque, column(tex::BIRCH_LOG_SIDE, tex::BIRCH_LOG_TOP, tex::BIRCH_LOG_TOP)),
        71 => ("jungle log", Opaque, column(tex::JUNGLE_LOG_SIDE, tex::JUNGLE_LOG_TOP, tex::JUNGLE_LOG_TOP)),
        72 => ("acacia log", Opaque, column(tex::ACACIA_LOG_SIDE, tex::ACACIA_LOG_TOP, tex::ACACIA_LOG_TOP)),
        73 => ("birch leaves", Cutout, all(tex::BIRCH_LEAVES)),
        74 => ("jungle leaves", Cutout, all(tex::JUNGLE_LEAVES)),
        75 => ("acacia leaves", Cutout, all(tex::ACACIA_LEAVES)),
        76 => ("spruce planks", Opaque, all(tex::SPRUCE_PLANKS)),
        77 => ("birch planks", Opaque, all(tex::BIRCH_PLANKS)),
        78 => ("jungle planks", Opaque, all(tex::JUNGLE_PLANKS)),
        79 => ("acacia planks", Opaque, all(tex::ACACIA_PLANKS)),
        80 => ("birch sapling", Cross, all(tex::BIRCH_SAPLING)),
        81 => ("jungle sapling", Cross, all(tex::JUNGLE_SAPLING)),
        82 => ("acacia sapling", Cross, all(tex::ACACIA_SAPLING)),
        83 => ("red sand", Opaque, all(tex::RED_SAND)),
        84..=90 => {
            const NAMES: [&str; 7] = [
                "terracotta",
                "orange terracotta",
                "yellow terracotta",
                "red terracotta",
                "brown terracotta",
                "white terracotta",
                "light gray terracotta",
            ];
            (NAMES[id as usize - 84], Opaque, all(tex::TERRACOTTA + (id - 84)))
        }
        91 => ("clay", Opaque, all(tex::CLAY)),
        92 => ("sugar cane", Cross, all(tex::SUGAR_CANE)),
        93 => ("pumpkin", Opaque, column(tex::PUMPKIN_SIDE, tex::PUMPKIN_TOP, tex::PUMPKIN_TOP)),
        94 => ("melon", Opaque, column(tex::MELON_SIDE, tex::MELON_TOP, tex::MELON_TOP)),
        95 => ("fern", Cross, all(tex::FERN)),
        96 => ("blue orchid", Cross, all(tex::BLUE_ORCHID)),
        97 => ("ice", Translucent, all(tex::ICE)),
        98 => ("bed foot", Cutout, column(tex::BED_SIDE_FOOT, tex::BED_TOP_FOOT, tex::PLANKS)),
        99 => ("bed head", Cutout, column(tex::BED_SIDE_HEAD, tex::BED_TOP_HEAD, tex::PLANKS)),
        100 => ("netherrack", Opaque, all(tex::NETHERRACK)),
        101 => ("soul sand", Opaque, all(tex::SOUL_SAND)),
        102 => ("nether quartz ore", Opaque, all(tex::QUARTZ_ORE)),
        103 => ("nether bricks", Opaque, all(tex::NETHER_BRICKS)),
        104 => ("nether portal", Translucent, all(tex::PORTAL)),
        105 => ("tnt", Opaque, column(tex::TNT_SIDE, tex::TNT_TOP, tex::TNT_BOTTOM)),
        106..=111 => {
            const NAMES: [&str; 6] =
                ["stone slab", "cobblestone slab", "oak slab", "sandstone slab", "brick slab", "nether brick slab"];
            let base = make(match id {
                106 => 1,
                107 => 9,
                108 => 8,
                109 => 21,
                110 => 20,
                _ => 103,
            });
            // Cutout, not opaque: the faces above and beside a slab show.
            (NAMES[id as usize - 106], Cutout, base.tex)
        }
        112..=135 => {
            const NAMES: [&str; 6] = [
                "stone stairs",
                "cobblestone stairs",
                "oak stairs",
                "sandstone stairs",
                "brick stairs",
                "nether brick stairs",
            ];
            let base = make(match (id - 112) / 4 {
                0 => 1,
                1 => 9,
                2 => 8,
                3 => 21,
                4 => 20,
                _ => 103,
            });
            (NAMES[(id as usize - 112) / 4], Shaped, base.tex)
        }
        136 => ("oak fence", Shaped, all(tex::PLANKS)),
        137..=140 => ("ladder", Shaped, all(tex::LADDER)),
        141..=148 => ("oak fence gate", Shaped, all(tex::PLANKS)),
        149..=156 => ("oak door", Shaped, all(tex::DOOR_BOTTOM)),
        157..=164 => ("oak door", Shaped, all(tex::DOOR_TOP)),
        165..=180 => ("fire", Cross, all(tex::FIRE_0)),
        181 => ("end stone", Opaque, all(tex::END_STONE)),
        182 => ("nether brick fence", Shaped, all(tex::NETHER_BRICKS)),
        183 => ("spawner", Cutout, all(tex::SPAWNER)),
        184..=187 => ("nether wart", Cross, all(tex::NETHER_WART_0 + (id - 184).saturating_sub(1))),
        188 => ("brewing stand", Shaped, column(tex::BREWING_SIDE, tex::BREWING_TOP, tex::BREWING_TOP)),
        189..=192 => ("stone brick stairs", Shaped, all(tex::STONE_BRICKS)),
        193 => ("stone brick slab", Cutout, all(tex::STONE_BRICKS)),
        194 => ("stone bricks", Opaque, all(tex::STONE_BRICKS)),
        195 => ("mossy stone bricks", Opaque, all(tex::MOSSY_STONE_BRICKS)),
        196 => ("cracked stone bricks", Opaque, all(tex::CRACKED_STONE_BRICKS)),
        197 => ("iron bars", Shaped, all(tex::IRON_BARS)),
        198 => ("bookshelf", Opaque, column(tex::BOOKSHELF, tex::PLANKS, tex::PLANKS)),
        199 => ("cobweb", Cross, all(tex::COBWEB)),
        200..=203 => ("end portal frame", Shaped, column(tex::FRAME_SIDE, tex::FRAME_TOP, tex::END_STONE)),
        204..=207 => ("end portal frame", Shaped, column(tex::FRAME_EYE_SIDE, tex::FRAME_EYE_TOP, tex::END_STONE)),
        208 => ("end portal", Shaped, all(tex::END_PORTAL)),
        209 => ("dragon egg", Shaped, all(tex::DRAGON_EGG)),
        210 => ("end gateway", Cutout, all(tex::END_PORTAL)),
        211 => ("lapis lazuli ore", Opaque, all(tex::LAPIS_ORE)),
        212 => ("block of lapis lazuli", Opaque, all(tex::LAPIS_BLOCK)),
        213 => ("enchanting table", Shaped, column(tex::ENCHANT_SIDE, tex::ENCHANT_TOP, tex::OBSIDIAN)),
        214 => ("block of iron", Opaque, all(tex::IRON_BLOCK)),
        215 | 216 => ("anvil", Shaped, column(tex::ANVIL, tex::ANVIL_TOP, tex::ANVIL)),
        217 | 218 => ("chipped anvil", Shaped, column(tex::ANVIL, tex::ANVIL_TOP + 1, tex::ANVIL)),
        219 | 220 => ("damaged anvil", Shaped, column(tex::ANVIL, tex::ANVIL_TOP + 2, tex::ANVIL)),
        221 => ("ancient debris", Opaque, column(tex::DEBRIS_SIDE, tex::DEBRIS_TOP, tex::DEBRIS_TOP)),
        222 => ("block of netherite", Opaque, all(tex::NETHERITE_BLOCK)),
        // Java's model: the tongs front on north and south, the hammer side
        // on east and west.
        223 => {
            let (front, side) = (tex::SMITHING_FRONT, tex::SMITHING_SIDE);
            ("smithing table", Opaque, [side, side, tex::SMITHING_TOP, tex::SMITHING_BOTTOM, front, front])
        }
        224 => ("mossy cobblestone", Opaque, all(tex::MOSSY_COBBLESTONE)),
        225 => ("copper ore", Opaque, all(tex::COPPER_ORE)),
        226 => ("redstone ore", Opaque, all(tex::REDSTONE_ORE)),
        227 => ("emerald ore", Opaque, all(tex::EMERALD_ORE)),
        228 => ("block of raw iron", Opaque, all(tex::RAW_IRON_BLOCK)),
        229 => ("block of raw gold", Opaque, all(tex::RAW_GOLD_BLOCK)),
        230 => ("block of raw copper", Opaque, all(tex::RAW_COPPER_BLOCK)),
        231 => ("block of copper", Opaque, all(tex::COPPER_BLOCK)),
        232 => ("block of emerald", Opaque, all(tex::EMERALD_BLOCK)),
        233 => ("granite", Opaque, all(tex::GRANITE)),
        234 => ("polished granite", Opaque, all(tex::POLISHED_GRANITE)),
        235 => ("diorite", Opaque, all(tex::DIORITE)),
        236 => ("polished diorite", Opaque, all(tex::POLISHED_DIORITE)),
        237 => ("andesite", Opaque, all(tex::ANDESITE)),
        238 => ("polished andesite", Opaque, all(tex::POLISHED_ANDESITE)),
        239 => ("tuff", Opaque, all(tex::TUFF)),
        240 => ("calcite", Opaque, all(tex::CALCITE)),
        241 => ("smooth stone", Opaque, all(tex::SMOOTH_STONE)),
        242 => ("deepslate", Opaque, all(tex::DEEPSLATE)),
        243 => ("cobbled deepslate", Opaque, all(tex::COBBLED_DEEPSLATE)),
        244 => ("polished deepslate", Opaque, all(tex::POLISHED_DEEPSLATE)),
        245 => ("deepslate coal ore", Opaque, all(tex::DEEPSLATE_COAL_ORE)),
        246 => ("deepslate iron ore", Opaque, all(tex::DEEPSLATE_IRON_ORE)),
        247 => ("deepslate copper ore", Opaque, all(tex::DEEPSLATE_COPPER_ORE)),
        248 => ("deepslate gold ore", Opaque, all(tex::DEEPSLATE_GOLD_ORE)),
        249 => ("deepslate redstone ore", Opaque, all(tex::DEEPSLATE_REDSTONE_ORE)),
        250 => ("deepslate emerald ore", Opaque, all(tex::DEEPSLATE_EMERALD_ORE)),
        251 => ("deepslate lapis lazuli ore", Opaque, all(tex::DEEPSLATE_LAPIS_ORE)),
        252 => ("deepslate diamond ore", Opaque, all(tex::DEEPSLATE_DIAMOND_ORE)),
        520 => ("brown mushroom", Cross, all(tex::BROWN_MUSHROOM)),
        521 => ("red mushroom", Cross, all(tex::RED_MUSHROOM)),
        331 => ("cobblestone wall", Shaped, all(tex::COBBLESTONE)),
        332 => ("stone brick wall", Shaped, all(tex::STONE_BRICKS)),
        333 => ("dark oak log", Opaque, column(tex::DARK_OAK_LOG_SIDE, tex::DARK_OAK_LOG_TOP, tex::DARK_OAK_LOG_TOP)),
        334 => ("mangrove log", Opaque, column(tex::MANGROVE_LOG_SIDE, tex::MANGROVE_LOG_TOP, tex::MANGROVE_LOG_TOP)),
        335 => ("cherry log", Opaque, column(tex::CHERRY_LOG_SIDE, tex::CHERRY_LOG_TOP, tex::CHERRY_LOG_TOP)),
        336 => ("dark oak planks", Opaque, all(tex::DARK_OAK_PLANKS)),
        337 => ("mangrove planks", Opaque, all(tex::MANGROVE_PLANKS)),
        338 => ("cherry planks", Opaque, all(tex::CHERRY_PLANKS)),
        id if super::forms::stone_form(id).is_some() => {
            let form = super::forms::stone_form(id).unwrap();
            let base = match form {
                super::forms::StoneForm::Wall { index } => super::forms::wall_material(index),
                super::forms::StoneForm::Stairs { index, .. } | super::forms::StoneForm::Slab { index } => {
                    super::forms::stone_base(index)
                }
            };
            let kind = match form {
                super::forms::StoneForm::Slab { .. } => Cutout,
                _ => Shaped,
            };
            (super::forms::stone_name(form), kind, make(base.0).tex)
        }
        id if super::forms::wood_form(id).is_some() => {
            let form = super::forms::wood_form(id).unwrap();
            let name = super::forms::wood_name(form);
            match form {
                super::forms::WoodForm::Door { index, upper, .. } => {
                    (name, Shaped, all(super::forms::door_tex(index, upper)))
                }
                super::forms::WoodForm::Slab { index } => (name, Cutout, make(super::forms::wood_planks(index).0).tex),
                super::forms::WoodForm::Stairs { index, .. }
                | super::forms::WoodForm::Fence { index }
                | super::forms::WoodForm::Gate { index, .. } => {
                    (name, Shaped, make(super::forms::wood_planks(index).0).tex)
                }
            }
        }
        500 | 504 | 505 => ("rail", Shaped, all(tex::RAIL)),
        501..=503 => ("rail", Shaped, all(tex::RAIL_EW)),
        506 => ("rail", Shaped, all(tex::RAIL_SE)),
        507 => ("rail", Shaped, all(tex::RAIL_SW)),
        508 => ("rail", Shaped, all(tex::RAIL_NW)),
        509 => ("rail", Shaped, all(tex::RAIL_NE)),
        _ => ("unknown", Invisible, all(0)),
    };
    // Ice is see-through like water but solid underfoot; End portals are
    // a surface to fall through.
    let solid = (matches!(kind, Opaque | Cutout | Shaped) || id == 97) && id != 208 && id != 210;
    BlockInfo { name, kind, solid, self_cull: matches!(id, 5 | 10 | 97 | 104), tex }
}

pub static INFO: [BlockInfo; STATE_CAPACITY] = {
    let mut arr = [make(0); STATE_CAPACITY];
    let mut i = 0;
    while i < STATE_CAPACITY {
        arr[i] = make(i as u16);
        i += 1;
    }
    arr
};

/// Slabs and stairs keep light out the way the original seven do. Walls,
/// fences, gates and doors stay open.
const fn shape_blocks_light(id: u16) -> bool {
    if matches!(id, 106..=135 | 189..=193) {
        return true;
    }
    if let Some(super::forms::StoneForm::Stairs { .. } | super::forms::StoneForm::Slab { .. }) =
        super::forms::stone_form(id)
    {
        return true;
    }
    matches!(
        super::forms::wood_form(id),
        Some(super::forms::WoodForm::Stairs { .. } | super::forms::WoodForm::Slab { .. })
    )
}

static LIGHT_OPACITY: [u8; STATE_CAPACITY] = {
    let mut arr = [15u8; STATE_CAPACITY];
    let mut i = 0;
    while i < STATE_CAPACITY {
        arr[i] = match INFO[i].kind {
            RenderKind::Opaque => 15,
            // Slabs and stairs keep the light out, like Minecraft's (they
            // borrow light from their neighbours instead; see `borrows_light`).
            _ if shape_blocks_light(i as u16) => 15,
            // Mesh bit tests use 4094 as a stairs stand-in at the layer limit.
            #[cfg(test)]
            _ if i == 4094 => 15,
            RenderKind::Invisible | RenderKind::Cross | RenderKind::Shaped => 0,
            _ if matches!(i, 10 | 98 | 99) => 0, // glass, beds
            _ => 1,                              // leaves, water: attenuate skylight too
        };
        i += 1;
    }
    arr
};

static BORROWS_LIGHT: [bool; STATE_CAPACITY] = {
    let mut arr = [false; STATE_CAPACITY];
    let mut i = 0;
    while i < STATE_CAPACITY {
        arr[i] = LIGHT_OPACITY[i] >= 15 && !matches!(INFO[i].kind, RenderKind::Opaque);
        i += 1;
    }
    arr
};

static OPAQUE: [bool; STATE_CAPACITY] = {
    let mut arr = [false; STATE_CAPACITY];
    let mut i = 0;
    while i < STATE_CAPACITY {
        arr[i] = matches!(INFO[i].kind, RenderKind::Opaque);
        i += 1;
    }
    arr
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stronghold_blocks_follow_java_rules() {
        use crate::item::{Tier, ToolKind};
        let sword = Some(Item::tool(ToolKind::Sword, Tier::Wood));
        assert!(crate::mining::can_harvest(Block::COBWEB, sword) && !crate::mining::can_harvest(Block::COBWEB, None));
        assert_eq!(Block::COBWEB.drop(), Some(Item::STRING));
        assert_eq!(Block::COBWEB.hardness(), 4.0);
        assert_eq!(crate::mining::break_time(Block::COBWEB, sword), 0.4);
        assert_eq!(crate::mining::break_time(Block::COBWEB, None), 20.0);
        assert!(!Block::COBWEB.is_solid());
        let eyed = Block(204);
        assert_eq!(eyed.with_facing(Facing::East), Block(206), "turning keeps the eye");
        assert_eq!(Block(201).base(), Block::END_PORTAL_FRAME);
        for id in 200..=207 {
            assert_eq!(Block(id).emission(), 1);
        }
        assert_eq!(Block::END_PORTAL_FRAME.hardness(), f32::INFINITY);
        assert!(!Block::END_PORTAL.is_solid() && Block::END_PORTAL.emission() == 15);
        assert_eq!(Block::slab_of(Block::STONE_BRICKS), Some(Block::STONE_BRICK_SLAB));
        assert_eq!(Block::STONE_BRICK_STAIRS.with_facing(Facing::West).stairs_base(), Some(Block::STONE_BRICKS));
        assert!(crate::world::shape::pane_connects(Block::STONE) && !crate::world::shape::pane_connects(Block::AIR));
    }

    #[test]
    fn block_layers_are_not_item_icons() {
        for index in 0..300 {
            assert_eq!(tex::item_index(tex::item_layer(index)), Some(index));
        }
        for layer in 0..tex::CAPACITY as u16 {
            assert_eq!(tex::item_index(layer), None);
        }
    }

    #[test]
    fn smithing_table_has_its_fixed_id_textures_and_axe_rules() {
        assert_eq!(Block::SMITHING_TABLE, Block(223));
        assert_eq!(Block::from_name("smithing_table"), Some(Block::SMITHING_TABLE));
        assert_eq!(Block::SMITHING_TABLE.name(), "smithing table");
        assert_eq!(Block::COPPER_ORE, Block(225));
        assert_eq!(Block::EMERALD_BLOCK, Block(232));
        assert_eq!(Block::from_name("copper_ore"), Some(Block::COPPER_ORE));
        assert_eq!(Block::from_name("block of raw iron"), Some(Block::RAW_IRON_BLOCK));
        assert_eq!(Block::IRON_ORE.drop(), Some(Item::RAW_IRON));
        assert_eq!(Block::COPPER_ORE.drop(), None);
        assert_eq!(Block::REDSTONE_ORE.harvest_level(), Some(2));
        assert_eq!(Block::COPPER_ORE.harvest_level(), Some(1));
        assert_eq!(Block::EMERALD_BLOCK.hardness(), 5.0);
        assert!(Block::creative_palette().any(|b| b == Block::EMERALD_ORE));
        assert_eq!(Item::from_name("raw_copper"), Some(Item::RAW_COPPER));
        assert_eq!(Item::REDSTONE, Item(365));
        assert_eq!(Block::GRANITE, Block(233));
        assert_eq!(Block::DEEPSLATE_DIAMOND_ORE, Block(252));
        assert_eq!(Block::from_name("polished_andesite"), Some(Block::POLISHED_ANDESITE));
        assert_eq!(Block::DEEPSLATE.drop(), Some(Block::COBBLED_DEEPSLATE.into()));
        assert_eq!(Block::DEEPSLATE_IRON_ORE.drop(), Some(Item::RAW_IRON));
        assert_eq!(Block::DEEPSLATE_IRON_ORE.as_stone_ore(), Block::IRON_ORE);
        assert_eq!(Block::DEEPSLATE_DIAMOND_ORE.harvest_level(), Some(2));
        assert_eq!(Block::DEEPSLATE_COAL_ORE.harvest_level(), Some(0));
        assert_eq!(Block::GRANITE.hardness(), 1.5);
        assert_eq!(Block::CALCITE.hardness(), 0.75);
        assert_eq!(Block::DEEPSLATE.hardness(), 3.0);
        assert_eq!(Block::COBBLED_DEEPSLATE.hardness(), 3.5);
        assert_eq!(Block::DEEPSLATE_IRON_ORE.hardness(), 4.5);
        assert_eq!(Block::GRANITE.best_tool(), Some(ToolKind::Pickaxe));
        assert!(Block::creative_palette().any(|b| b == Block::TUFF));
        assert!(Block::creative_palette().any(|b| b == Block::DEEPSLATE_EMERALD_ORE));
        assert_eq!(Block::DARK_OAK_LOG, Block(333));
        assert_eq!(Block::CHERRY_PLANKS, Block(338));
        assert_eq!(Block(499).name(), "cherry door");
        assert!(Block(499).is_door_upper());
        assert_eq!(crate::world::forms::wood_id(0, 0).stairs_base(), Some(Block::DARK_OAK_PLANKS));
        assert_eq!(crate::world::forms::stone_id(0, 0).stairs_base(), Some(Block::MOSSY_COBBLESTONE));
        assert_eq!(crate::world::forms::stone_id(10, 4).slab_base(), Some(Block::DEEPSLATE));
        assert_eq!(Block::COBBLESTONE_WALL.wall_base(), Some(Block::COBBLESTONE));
        assert_eq!(Block::from_name("cherry_door"), Some(crate::world::forms::wood_id(6, 14)));
        assert_eq!(Item::from_name("spruce_door"), Some(Item::from_block(crate::world::forms::wood_id(1, 14))));
        assert_eq!(Item::from_name("oak_door"), Some(Item::OAK_DOOR));
        assert!(Block::creative_palette().any(|b| b == Block::CHERRY_LOG));
        assert!(Block::creative_palette().any(|b| b == crate::world::forms::wood_id(6, 14)));
        assert!(!Block::creative_palette().any(|b| b == Block(499)));
        let gate = crate::world::forms::wood_id(2, 6);
        assert_eq!(gate.with_facing(Facing::East).shaped(), Some(Shaped::Gate { facing: Facing::East, open: false }));
        assert_eq!(gate.toggled(Facing::South).shaped(), Some(Shaped::Gate { facing: Facing::South, open: true }));
        let deepslate_slab = crate::world::forms::stone_id(10, 4);
        let dark_oak_stairs = crate::world::forms::wood_id(0, 0);
        assert!(deepslate_slab.light_opacity() == 15 && deepslate_slab.borrows_light());
        assert!(dark_oak_stairs.borrows_light() && dark_oak_stairs.best_tool() == Some(ToolKind::Axe));
        assert_eq!(crate::world::forms::wood_id(0, 5).light_opacity(), 0);
        assert_eq!(Block::COBBLESTONE_WALL.light_opacity(), 0);
        assert_eq!(crate::world::forms::wood_id(6, 14).hardness(), 3.0);
        assert_eq!(Block::SMITHING_TABLE.hardness(), 2.5);
        assert_eq!(Block::SMITHING_TABLE.best_tool(), Some(ToolKind::Axe));
        assert_eq!(
            Block::SMITHING_TABLE.info().tex,
            [
                tex::SMITHING_SIDE,
                tex::SMITHING_SIDE,
                tex::SMITHING_TOP,
                tex::SMITHING_BOTTOM,
                tex::SMITHING_FRONT,
                tex::SMITHING_FRONT,
            ]
        );
        assert!(Block::creative_palette().any(|b| b == Block::SMITHING_TABLE));
    }

    #[test]
    fn fire_states_and_materials_follow_minecraft_rules() {
        for age in 0..=15 {
            let fire = Block::fire(age);
            assert_eq!(fire.fire_age(), Some(age));
            assert_eq!(fire.emission(), 15);
            assert_eq!(fire.light_opacity(), 0);
            assert!(fire.is_replaceable() && !fire.is_solid());
            assert_eq!(fire.drop(), None);
        }
        assert_eq!(Block::from_name("fire"), Some(Block::FIRE));
        assert_eq!(Block::fire(255), Block::fire(15));
        assert_eq!(Block::LOG.fire_odds(), (5, 5));
        assert_eq!(Block::WOOL.fire_odds(), (30, 60));
        assert_eq!(Block::TNT.fire_odds(), (15, 100));
        for b in [
            Block::PLANKS,
            Block::SPRUCE_PLANKS,
            Block::slab_of(Block::PLANKS).unwrap(),
            Block::stairs_of(Block::PLANKS).unwrap().with_facing(Facing::West),
            Block::OAK_FENCE,
            Block::gate(Facing::North, true),
        ] {
            assert_eq!(b.fire_odds(), (5, 20), "{}", b.name());
        }
        for b in [Block::OAK_DOOR, Block::LADDER, Block::CHEST, Block::CRAFTING_TABLE, Block::BED_FOOT] {
            assert_eq!(b.fire_odds(), (0, 0), "{} isn't consumed by fire", b.name());
            assert!(b.ignited_by_lava());
        }
        assert!(!Block::STONE.ignited_by_lava());
        assert_eq!(Block::NETHERRACK.fire_odds(), (0, 0));
    }
    #[test]
    fn cross_blocks_are_targetable_but_not_solid() {
        for b in [Block::TALL_GRASS, Block::DANDELION, Block::POPPY, Block::DEAD_BUSH, Block::TORCH] {
            assert_eq!(b.kind(), RenderKind::Cross, "{}", b.name());
            assert!(!b.is_solid() && !b.is_opaque() && b.is_targetable());
            assert_eq!(b.light_opacity(), 0);
            assert_eq!(b.hardness(), 0.0);
            assert!(Block::creative_palette().any(|p| p == b));
        }
        assert!(!Block::WATER.is_targetable() && !Block::AIR.is_targetable());
        assert_eq!(Block::TORCH.emission(), 14);
        assert!(Block::TALL_GRASS.is_replaceable() && !Block::POPPY.is_replaceable());
    }

    #[test]
    fn plants_need_soil_and_torches_a_full_block() {
        assert!(Block::POPPY.can_stay_on(Block::GRASS));
        assert!(!Block::POPPY.can_stay_on(Block::SAND));
        assert!(Block::DEAD_BUSH.can_stay_on(Block::SAND));
        assert!(Block::TORCH.can_stay_on(Block::COBBLESTONE));
        assert!(!Block::TORCH.can_stay_on(Block::GLASS) && !Block::TORCH.can_stay_on(Block::AIR));
        assert!(Block::STONE.can_stay_on(Block::AIR));
    }

    #[test]
    fn oriented_blocks_turn_and_keep_their_rules() {
        for base in [Block::FURNACE, Block::LIT_FURNACE, Block::CHEST] {
            for f in Facing::ALL {
                let b = base.with_facing(f);
                assert_eq!(b.oriented(), Some((base, f)));
                assert_eq!(b.base(), base);
                assert_eq!(b.name(), base.name());
                assert_eq!(b.info().tex[f.face()], base.info().tex[Facing::South.face()], "front on the {f:?} face");
                assert_eq!(
                    (b.hardness(), b.best_tool(), b.emission()),
                    (base.hardness(), base.best_tool(), base.emission())
                );
                assert_eq!(b.with_facing(Facing::South), base);
            }
        }
        assert_eq!(Block::LIT_FURNACE.with_facing(Facing::East).drop(), Some(Block::FURNACE.into()));
        assert_eq!(Block::STONE.with_facing(Facing::East), Block::STONE);
        assert_eq!(Block::from_name("chest"), Some(Block::CHEST));
        // Placed by someone looking east, the front faces west, back at them.
        assert_eq!(Facing::toward(glam::Vec3::new(0.9, -0.3, 0.2)), Facing::West);
        assert_eq!(Facing::toward(glam::Vec3::new(0.1, 0.0, -1.0)), Facing::South);
    }

    #[test]
    fn crops_and_saplings_need_the_right_soil() {
        for stage in 0..8 {
            let wheat = Block::wheat(stage);
            assert_eq!(wheat.crop_stage(), Some(stage));
            assert_eq!(wheat.kind(), RenderKind::Cross);
            assert!(wheat.can_stay_on(Block::WET_FARMLAND) && !wheat.can_stay_on(Block::DIRT));
        }
        assert_eq!(Block::wheat(3).drop(), Some(Item::WHEAT_SEEDS));
        assert_eq!(Block::wheat(7).drop(), Some(Item::WHEAT));
        assert_eq!(Block::FARMLAND.drop(), Some(Block::DIRT.into()));
        assert!(Block::OAK_SAPLING.can_stay_on(Block::GRASS) && !Block::OAK_SAPLING.can_stay_on(Block::SAND));
        assert_eq!(Block::from_name("wheat crops"), Some(Block::wheat(0)));
    }

    #[test]
    fn woods_and_biome_blocks() {
        for wood in Wood::ALL {
            for b in [wood.log(), wood.leaves(), wood.planks(), wood.sapling()] {
                assert_eq!(b.wood(), Some(wood), "{}", b.name());
                assert!(Block::creative_palette().any(|p| p == b), "{}", b.name());
            }
            assert!(wood.log().is_log() && wood.leaves().is_leaves() && wood.planks().is_planks());
            assert!(wood.sapling().is_sapling() && wood.sapling().can_stay_on(Block::GRASS));
            assert_eq!(wood.log().best_tool(), Some(ToolKind::Axe));
            assert_eq!(wood.leaves().drop(), None);
        }
        assert_eq!(Block::from_name("oak log"), Some(Block::LOG));
        assert!(Block::RED_SAND.has_gravity());
        assert!(Block::DEAD_BUSH.can_stay_on(Block::terracotta(3)));
        for c in 0..7 {
            assert_eq!(Block::terracotta(c).terracotta_colour(), Some(c));
            assert_eq!(Block::terracotta(c).harvest_level(), Some(0));
        }
        assert_eq!(Block::CLAY.drop(), Some(Item::CLAY_BALL));
        // Ice: see-through and blended like water, but solid and minable.
        assert!(Block::ICE.is_solid() && !Block::ICE.is_opaque() && Block::ICE.is_targetable());
        assert_eq!(Block::ICE.kind(), RenderKind::Translucent);
        assert_eq!(Block::ICE.drop(), None);
        assert!(Block::SUGAR_CANE.can_stay_on(Block::SAND) && Block::SUGAR_CANE.can_stay_on(Block::SUGAR_CANE));
        assert!(!Block::SUGAR_CANE.can_stay_on(Block::STONE));
    }

    #[test]
    fn foliage_layers_round_trip() {
        for group in 0..tex::FOLIAGE_GROUPS {
            for layer in tex::FOLIAGE {
                let t = tex::tinted(layer, group);
                assert!((t as u32) < tex::COUNT);
                if group == 0 {
                    assert_eq!(t, layer);
                } else {
                    assert_eq!(tex::untinted(t), Some((layer, group)));
                }
            }
        }
        // Only grass and oak leaves change colour.
        assert_eq!(tex::tinted(tex::STONE, 3), tex::STONE);
        assert_eq!(tex::tinted(tex::SPRUCE_LEAVES, 4), tex::SPRUCE_LEAVES);
        assert_eq!(tex::untinted(tex::END_STONE), None);
    }

    #[test]
    fn slabs_are_half_blocks_that_mine_like_their_base() {
        for (i, &base) in Block::SLAB_BASES.iter().enumerate() {
            let slab = Block::slab_of(base).unwrap();
            assert_eq!(slab, slab_id(i));
            assert_eq!(slab.slab_base(), Some(base));
            assert_eq!(slab.height(), 0.5);
            assert!(slab.is_solid() && !slab.is_opaque() && slab.is_targetable());
            assert!(slab.light_opacity() == 15 && slab.borrows_light());
            assert_eq!(
                (slab.hardness(), slab.best_tool(), slab.harvest_level()),
                (base.hardness(), base.best_tool(), base.harvest_level())
            );
            assert_eq!(slab.drop(), Some(slab.into()), "{}", slab.name());
            assert_eq!(slab.info().tex, base.info().tex);
            assert!(Block::creative_palette().any(|p| p == slab));
        }
        assert_eq!(Block::slab_of(Block::DIRT), None);
        assert_eq!(Block::from_name("oak_slab"), Some(Block::slab_of(Block::PLANKS).unwrap()));
    }

    #[test]
    fn fluids_have_sources_levels_and_drops() {
        for fluid in [Fluid::Water, Fluid::Lava] {
            assert_eq!(fluid.source().fluid(), Some(fluid));
            assert_eq!(fluid.falling().fluid_level(), Some(0));
            for l in 1..=fluid.max_level() {
                let b = fluid.flowing(l);
                assert_eq!((b.fluid(), b.fluid_level()), (Some(fluid), Some(l)));
                assert!(b.fluid_drop() > fluid.source().fluid_drop());
                assert!(!b.is_targetable() && b.is_replaceable() && !b.is_solid());
            }
        }
        assert!(Block::LAVA.is_lava() && !Block::LAVA.is_water());
        assert_eq!(Block::flowing_lava(2).emission(), 15);
        assert!(Block::OBSIDIAN.is_opaque() && Block::OBSIDIAN.hardness() > Block::STONE.hardness());
        assert_eq!(Block::LAVA.drop(), None);
    }

    #[test]
    fn shaped_blocks_turn_drop_and_mine_like_their_material() {
        for (i, &base) in Block::SLAB_BASES.iter().enumerate() {
            let stairs = Block::stairs_of(base).unwrap();
            assert_eq!(stairs, stairs_id(i));
            for f in Facing::ALL {
                let turned = stairs.with_facing(f);
                assert_eq!(turned.shaped(), Some(Shaped::Stairs(f)));
                assert_eq!((turned.base(), turned.stairs_base()), (stairs, Some(base)));
                assert_eq!(turned.drop(), Some(stairs.into()));
                assert_eq!((turned.hardness(), turned.harvest_level()), (base.hardness(), base.harvest_level()));
                assert!(turned.borrows_light() && turned.is_solid() && !turned.is_opaque());
            }
            assert!(Block::creative_palette().any(|p| p == stairs));
        }
        for f in Facing::ALL {
            assert_eq!(Block::LADDER.with_facing(f).shaped(), Some(Shaped::Ladder(f)));
            assert_eq!(Block::FENCE_GATE.with_facing(f), Block::gate(f, false));
            for open in [false, true] {
                let gate = Block::gate(f, open);
                assert_eq!(gate.shaped(), Some(Shaped::Gate { facing: f, open }));
                assert_eq!(gate.drop(), Some(Block::FENCE_GATE.into()));
                assert_eq!(gate.toggled(f), Block::gate(f, !open));
                for upper in [false, true] {
                    let door = Block::door(f, open, upper);
                    assert_eq!(door.shaped(), Some(Shaped::Door { facing: f, open, upper }));
                    assert_eq!(door.name(), "oak door");
                    assert_eq!(door.drop(), (!upper).then_some(Item::OAK_DOOR));
                    assert_eq!(door.toggled(f), Block::door(f, !open, upper));
                    assert_eq!(door.light_opacity(), 0);
                }
            }
        }
        // The upper half stands on the lower; the lower on a full block.
        assert!(Block::door(Facing::East, false, true).can_stay_on(Block::door(Facing::East, false, false)));
        assert!(!Block::door(Facing::East, false, true).can_stay_on(Block::AIR));
        assert!(Block::OAK_DOOR.can_stay_on(Block::STONE) && !Block::OAK_DOOR.can_stay_on(Block::GLASS));
        assert_eq!(Block::OAK_FENCE.hardness(), Block::PLANKS.hardness());
        assert_eq!(Block::LADDER.best_tool(), Some(ToolKind::Axe));
        assert!(Block::LADDER.flat_icon() && !Block::OAK_FENCE.flat_icon());
        assert_eq!(Item::from_name("oak_door"), Some(Item::OAK_DOOR));
        assert_eq!(Block::from_name("oak fence"), Some(Block::OAK_FENCE));
    }

    #[test]
    fn anvils_turn_broadside_fall_and_wear_down() {
        use crate::world::block::Facing;
        // Placed facing south or north (the placer looks along z), the top runs along x.
        let placed = Block::ANVIL.with_facing(Facing::South);
        assert_eq!(placed.shaped(), Some(Shaped::Anvil { along_x: true }));
        assert_eq!(Block::ANVIL.with_facing(Facing::East).shaped(), Some(Shaped::Anvil { along_x: false }));
        assert_eq!(placed.base(), Block::ANVIL);
        assert_eq!(placed.drop(), Some(Block::ANVIL.into()));
        assert!(placed.has_gravity() && placed.is_anvil() && !Block::IRON_BLOCK.is_anvil());
        let chipped = placed.anvil_damaged().unwrap();
        assert_eq!((chipped.base(), chipped.shaped()), (Block::CHIPPED_ANVIL, placed.shaped()));
        let damaged = chipped.anvil_damaged().unwrap();
        assert_eq!(damaged.base(), Block::DAMAGED_ANVIL);
        assert_eq!(damaged.anvil_damaged(), None, "a damaged anvil breaks next");
        assert_eq!(Item::from_name("chipped anvil"), Some(Item::from(Block::CHIPPED_ANVIL)));
    }

    #[test]
    fn rails_are_thin_pickaxe_blocks_with_java_shapes() {
        assert_eq!(Block::RAIL, Block(500));
        assert_eq!(Block::from_name("rail"), Some(Block::RAIL));
        assert_eq!(Block::RAIL.hardness(), 0.7);
        assert_eq!(Block::RAIL.best_tool(), Some(ToolKind::Pickaxe));
        assert!(Block::RAIL.can_stay_on(Block::STONE) && !Block::RAIL.can_stay_on(Block::AIR));
        assert!(Block::RAIL.flat_icon() && Block::creative_palette().any(|b| b == Block::RAIL));
        for (i, &shape) in RailShape::ALL.iter().enumerate() {
            let b = Block::rail(shape);
            assert_eq!(b, Block(500 + i as u16));
            assert_eq!(b.rail_shape(), Some(shape));
            assert_eq!(b.base(), Block::RAIL);
            assert_eq!(b.shaped(), Some(Shaped::Rail));
            assert_eq!(b.name(), "rail");
            assert_eq!((b.hardness(), b.best_tool()), (0.7, Some(ToolKind::Pickaxe)));
        }
        let boxes = crate::world::shape::item_shape(Block::RAIL);
        assert_eq!(boxes.as_slice(), &[crate::world::shape::Box16 { min: [0, 0, 0], max: [16, 2, 16] }]);
    }
}
