//! Block registry: ids, render classification and per-face texture layers.
//!
//! Block properties live in a 256-entry static table so hot loops (meshing,
//! physics) resolve them with a single indexed load instead of a `match`.

use crate::item::{Item, ToolKind};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[repr(transparent)]
pub struct Block(pub u8);

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
    pub tex: [u8; 6],
}

/// Texture array layers. Must match the generators in `render::textures`.
pub mod tex {
    pub const STONE: u8 = 0;
    pub const DIRT: u8 = 1;
    pub const GRASS_TOP: u8 = 2;
    pub const GRASS_SIDE: u8 = 3;
    pub const SAND: u8 = 4;
    pub const WATER: u8 = 5;
    pub const LOG_SIDE: u8 = 6;
    pub const LOG_TOP: u8 = 7;
    pub const LEAVES: u8 = 8;
    pub const PLANKS: u8 = 9;
    pub const COBBLESTONE: u8 = 10;
    pub const GLASS: u8 = 11;
    pub const BEDROCK: u8 = 12;
    pub const GRAVEL: u8 = 13;
    pub const SNOW: u8 = 14;
    pub const SNOWY_GRASS_SIDE: u8 = 15;
    pub const COAL_ORE: u8 = 16;
    pub const IRON_ORE: u8 = 17;
    pub const GOLD_ORE: u8 = 18;
    pub const DIAMOND_ORE: u8 = 19;
    pub const CACTUS_SIDE: u8 = 20;
    pub const CACTUS_TOP: u8 = 21;
    pub const BRICKS: u8 = 22;
    pub const SANDSTONE_SIDE: u8 = 23;
    pub const SANDSTONE_TOP: u8 = 24;
    pub const GLOWSTONE: u8 = 25;
    pub const SPRUCE_LEAVES: u8 = 26;
    // HUD icons.
    pub const HEART_FULL: u8 = 27;
    pub const HEART_HALF: u8 = 28;
    pub const HEART_EMPTY: u8 = 29;
    pub const BUBBLE: u8 = 30;
    /// Block-breaking crack overlays, stages 0..10.
    pub const CRACK_0: u8 = 31;
    pub const CRACK_STAGES: u8 = 10;
    // Cross-shaped plants and torches.
    pub const TALL_GRASS: u8 = 41;
    pub const DANDELION: u8 = 42;
    pub const POPPY: u8 = 43;
    pub const DEAD_BUSH: u8 = 44;
    pub const TORCH: u8 = 45;
    pub const LAVA: u8 = 46;
    pub const OBSIDIAN: u8 = 47;
    pub const WOOL: u8 = 48;
    pub const TABLE_TOP: u8 = 49;
    pub const TABLE_SIDE: u8 = 50;
    pub const FURNACE_FRONT: u8 = 51;
    pub const FURNACE_LIT: u8 = 52;
    pub const FURNACE_TOP: u8 = 53;
    // Hunger bar icons.
    pub const FOOD_FULL: u8 = 54;
    pub const FOOD_HALF: u8 = 55;
    pub const FOOD_EMPTY: u8 = 56;
    pub const FURNACE_SIDE: u8 = 57;
    pub const CHEST_TOP: u8 = 58;
    pub const CHEST_SIDE: u8 = 59;
    pub const CHEST_FRONT: u8 = 60;
    pub const FARMLAND: u8 = 61;
    pub const WET_FARMLAND: u8 = 62;
    /// Wheat growth stages 0..8.
    pub const WHEAT_0: u8 = 63;
    pub const OAK_SAPLING: u8 = 71;
    pub const SPRUCE_SAPLING: u8 = 72;
    pub const BED_TOP_FOOT: u8 = 73;
    pub const BED_TOP_HEAD: u8 = 74;
    pub const BED_SIDE_FOOT: u8 = 75;
    pub const BED_SIDE_HEAD: u8 = 76;
    pub const NETHERRACK: u8 = 77;
    pub const SOUL_SAND: u8 = 78;
    pub const QUARTZ_ORE: u8 = 79;
    pub const NETHER_BRICKS: u8 = 80;
    pub const PORTAL: u8 = 81;
    pub const TNT_SIDE: u8 = 82;
    pub const TNT_TOP: u8 = 83;
    pub const TNT_BOTTOM: u8 = 84;
    pub const LADDER: u8 = 85;
    pub const DOOR_TOP: u8 = 86;
    pub const DOOR_BOTTOM: u8 = 87;
    /// The player's arm in first person.
    pub const SKIN: u8 = 88;
    /// Flat item icons (see `item::Item::icon_layer`), up to `ITEM_COUNT` of them.
    pub const ITEM_0: u8 = 96;
    pub const ITEM_COUNT: u8 = 64;
    // More block textures, after the item icons.
    pub const SPRUCE_LOG_SIDE: u8 = 160;
    pub const SPRUCE_LOG_TOP: u8 = 161;
    pub const BIRCH_LOG_SIDE: u8 = 162;
    pub const BIRCH_LOG_TOP: u8 = 163;
    pub const JUNGLE_LOG_SIDE: u8 = 164;
    pub const JUNGLE_LOG_TOP: u8 = 165;
    pub const ACACIA_LOG_SIDE: u8 = 166;
    pub const ACACIA_LOG_TOP: u8 = 167;
    pub const BIRCH_LEAVES: u8 = 168;
    pub const JUNGLE_LEAVES: u8 = 169;
    pub const ACACIA_LEAVES: u8 = 170;
    pub const SPRUCE_PLANKS: u8 = 171;
    pub const BIRCH_PLANKS: u8 = 172;
    pub const JUNGLE_PLANKS: u8 = 173;
    pub const ACACIA_PLANKS: u8 = 174;
    pub const BIRCH_SAPLING: u8 = 175;
    pub const JUNGLE_SAPLING: u8 = 176;
    pub const ACACIA_SAPLING: u8 = 177;
    pub const RED_SAND: u8 = 178;
    /// Plain terracotta, then the six dyed colours (see `Block::TERRACOTTA`).
    pub const TERRACOTTA: u8 = 179;
    pub const CLAY: u8 = 186;
    pub const SUGAR_CANE: u8 = 187;
    pub const PUMPKIN_SIDE: u8 = 188;
    pub const PUMPKIN_TOP: u8 = 189;
    pub const MELON_SIDE: u8 = 190;
    pub const MELON_TOP: u8 = 191;
    pub const FERN: u8 = 192;
    pub const BLUE_ORCHID: u8 = 193;
    pub const ICE: u8 = 194;
    /// Biome-coloured copies of [`FOLIAGE`] textures: for each foliage
    /// group from 1 (see `terrain::Biome::foliage`), one layer per entry.
    pub const FOLIAGE_0: u8 = 195;
    pub const FOLIAGE: [u8; 5] = [GRASS_TOP, GRASS_SIDE, LEAVES, TALL_GRASS, FERN];
    pub const FOLIAGE_GROUPS: u8 = 5;
    /// More item icons, once the first `ITEM_COUNT` are used up.
    pub const ITEM_MORE_0: u8 = FOLIAGE_0 + (FOLIAGE_GROUPS - 1) * FOLIAGE.len() as u8;
    pub const ITEM_MORE_COUNT: u8 = 32;
    pub const COUNT: u32 = ITEM_MORE_0 as u32 + ITEM_MORE_COUNT as u32;
    // Layers are stored in a byte.
    const _: () = assert!(COUNT <= 256);

    /// Texture layer of item icon `index` (see `item::sprite_for_layer`).
    pub const fn item_layer(index: u8) -> u8 {
        if index < ITEM_COUNT { ITEM_0 + index } else { ITEM_MORE_0 + index - ITEM_COUNT }
    }

    /// The item icon index drawn on `layer`, if it holds one.
    pub fn item_index(layer: u8) -> Option<u8> {
        if (ITEM_0..ITEM_0 + ITEM_COUNT).contains(&layer) {
            Some(layer - ITEM_0)
        } else if (ITEM_MORE_0 as u32..COUNT).contains(&(layer as u32)) {
            Some(layer - ITEM_MORE_0 + ITEM_COUNT)
        } else {
            None
        }
    }

    /// The layer to draw `layer` with in a column of foliage `group`:
    /// grass and oak leaves take on the colour of the biome.
    #[inline]
    pub fn tinted(layer: u8, group: u8) -> u8 {
        if group == 0 {
            return layer;
        }
        match FOLIAGE.iter().position(|&l| l == layer) {
            Some(i) => FOLIAGE_0 + (group - 1) * FOLIAGE.len() as u8 + i as u8,
            None => layer,
        }
    }

    /// The plain layer and foliage group a tinted layer was made from.
    pub fn untinted(layer: u8) -> Option<(u8, u8)> {
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

    pub const fn flowing_water(level: u8) -> Block {
        Block(23 + level)
    }

    pub const fn flowing_lava(level: u8) -> Block {
        Block(37 + level)
    }

    /// Wheat crops at growth `stage` 0..=7 (7 is ripe).
    pub const fn wheat(stage: u8) -> Block {
        Block(59 + stage)
    }

    /// Growth stage of a wheat crop.
    pub fn crop_stage(self) -> Option<u8> {
        (59..=66).contains(&self.0).then(|| self.0 - 59)
    }

    /// A fence gate facing `facing`.
    pub const fn gate(facing: Facing, open: bool) -> Block {
        Block(141 + open as u8 * 4 + facing as u8)
    }

    /// One half of a door facing `facing`.
    pub const fn door(facing: Facing, open: bool, upper: bool) -> Block {
        Block(149 + upper as u8 * 8 + open as u8 * 4 + facing as u8)
    }

    /// The stairs cut from `base` (one of [`Block::SLAB_BASES`]), facing south.
    pub fn stairs_of(base: Block) -> Option<Block> {
        Self::SLAB_BASES.iter().position(|&b| b == base).map(|i| Block(112 + i as u8 * 4))
    }

    /// The full block stairs were cut from.
    pub fn stairs_base(self) -> Option<Block> {
        (112..=135).contains(&self.0).then(|| Self::SLAB_BASES[(self.0 as usize - 112) / 4])
    }

    /// What kind of shaped block this is, with its state.
    pub fn shaped(self) -> Option<Shaped> {
        let f = |i: u8| Facing::ALL[i as usize % 4];
        Some(match self.0 {
            112..=135 => Shaped::Stairs(f(self.0 - 112)),
            136 => Shaped::Fence,
            137..=140 => Shaped::Ladder(f(self.0 - 137)),
            141..=148 => Shaped::Gate { facing: f(self.0 - 141), open: self.0 >= 145 },
            149..=164 => {
                let i = self.0 - 149;
                Shaped::Door { facing: f(i), open: i % 8 >= 4, upper: i >= 8 }
            }
            _ => return None,
        })
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
            Some(Shaped::Gate { open, .. }) => Block::gate(facing, !open),
            Some(Shaped::Door { open, upper, .. }) => Block::door(facing, !open, upper),
            _ => self,
        }
    }

    /// Drawn as a flat sprite in inventories and when dropped.
    pub fn flat_icon(self) -> bool {
        self.kind() == RenderKind::Cross || self.is_ladder()
    }

    /// Badlands terracotta: 0 plain, then orange, yellow, red, brown, white
    /// and light grey.
    pub const fn terracotta(colour: u8) -> Block {
        Block(84 + colour)
    }

    /// The kind of wood a log, leaves, planks or sapling block is made of.
    pub fn wood(self) -> Option<Wood> {
        Wood::ALL.into_iter().find(|w| [w.log(), w.leaves(), w.planks(), w.sapling()].contains(&self))
    }

    pub fn is_log(self) -> bool {
        matches!(self.0, 6 | 69..=72)
    }

    pub fn is_leaves(self) -> bool {
        matches!(self.0, 7 | 23 | 73..=75)
    }

    pub fn is_planks(self) -> bool {
        matches!(self.0, 8 | 76..=79)
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
        let f = |i: u8| Facing::ALL[i as usize];
        match self.0 {
            45 => Some((Block::FURNACE, Facing::South)),
            46 => Some((Block::LIT_FURNACE, Facing::South)),
            47..=49 => Some((Block::FURNACE, f(self.0 - 46))),
            50..=52 => Some((Block::LIT_FURNACE, f(self.0 - 49))),
            53..=56 => Some((Block::CHEST, f(self.0 - 53))),
            112..=135 => Some((Block((self.0 - 112) / 4 * 4 + 112), f((self.0 - 112) % 4))),
            137..=140 => Some((Block::LADDER, f(self.0 - 137))),
            141..=148 => Some((Block::FENCE_GATE, f((self.0 - 141) % 4))),
            149..=164 => Some((Block::OAK_DOOR, f((self.0 - 149) % 4))),
            _ => None,
        }
    }

    /// This block without its orientation (itself if it has none).
    pub fn base(self) -> Block {
        self.oriented().map_or(self, |(b, _)| b)
    }

    /// The same block facing `facing` (unchanged if it has no front).
    pub fn with_facing(self, facing: Facing) -> Block {
        let i = facing as u8;
        match self.base() {
            b if i == 0 => b,
            Block::FURNACE => Block(46 + i),
            Block::LIT_FURNACE => Block(49 + i),
            Block::CHEST => Block(53 + i),
            Block::LADDER => Block(137 + i),
            Block::FENCE_GATE => Block::gate(facing, false),
            b if b.stairs_base().is_some() => Block(b.0 + i),
            _ => self,
        }
    }

    #[inline(always)]
    pub fn info(self) -> &'static BlockInfo {
        &INFO[self.0 as usize]
    }

    #[inline(always)]
    pub fn kind(self) -> RenderKind {
        self.info().kind
    }

    #[inline(always)]
    pub fn is_opaque(self) -> bool {
        OPAQUE[self.0 as usize]
    }

    #[inline(always)]
    pub fn is_water(self) -> bool {
        self.0 == 5 || (24..=31).contains(&self.0)
    }

    /// Water flow level: 0 for sources and falling water, 1..=7 for flowing.
    pub fn water_level(self) -> Option<u8> {
        match self.0 {
            5 | 31 => Some(0),
            24..=30 => Some(self.0 - 23),
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
            24..=30 => Some(self.0 - 23),
            38..=40 => Some(self.0 - 37),
            _ => None,
        }
    }

    /// How far (in 1/16 block) this fluid's surface sits below the top of
    /// its cell when no fluid of the same kind is above it. Lava's three
    /// levels drop like water's levels 2, 4 and 6.
    pub fn fluid_drop(self) -> u8 {
        match self.0 {
            5 | 37 => 2,
            24..=30 => 2 + (self.0 - 23) * 12 / 7,
            38..=40 => 2 + (self.0 - 37) * 24 / 7,
            _ => 0,
        }
    }

    /// The full blocks slabs are cut from, in slab id order.
    pub const SLAB_BASES: [Block; 6] =
        [Block::STONE, Block::COBBLESTONE, Block::PLANKS, Block::SANDSTONE, Block::BRICKS, Block::NETHER_BRICKS];

    /// The slab cut from `base`, if there is one.
    pub fn slab_of(base: Block) -> Option<Block> {
        Self::SLAB_BASES.iter().position(|&b| b == base).map(|i| Block(106 + i as u8))
    }

    /// The full block a slab was cut from (two stacked slabs make it).
    pub fn slab_base(self) -> Option<Block> {
        (106..=111).contains(&self.0).then(|| Self::SLAB_BASES[self.0 as usize - 106])
    }

    pub fn is_slab(self) -> bool {
        self.slab_base().is_some()
    }

    /// What mining rules treat this block as: a slab mines like its full
    /// block, an oriented block like its plain form.
    fn material(self) -> Block {
        match self.base() {
            Block::OAK_FENCE | Block::FENCE_GATE => Block::PLANKS,
            b => b.slab_base().or(b.stairs_base()).unwrap_or(b),
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
        match self.base() {
            Block::STONE => Some(Block::COBBLESTONE.into()),
            Block::GRASS | Block::SNOWY_GRASS => Some(Block::DIRT.into()),
            Block::COAL_ORE => Some(Item::COAL),
            Block::DIAMOND_ORE => Some(Item::DIAMOND),
            Block::DEAD_BUSH => Some(Item::STICK),
            Block::LIT_FURNACE => Some(Block::FURNACE.into()),
            Block::FARMLAND | Block::WET_FARMLAND => Some(Block::DIRT.into()),
            b if b.crop_stage() == Some(7) => Some(Item::WHEAT),
            b if b.crop_stage().is_some() => Some(Item::WHEAT_SEEDS),
            Block::CLAY => Some(Item::CLAY_BALL),
            Block::MELON => Some(Item::MELON_SLICE),
            // The foot drops the bed; breaking either half breaks both.
            Block::BED_FOOT => Some(Item::BED),
            Block::BED_HEAD => None,
            // Only the lower half of a door drops it.
            Block::OAK_DOOR => (!self.is_door_upper()).then_some(Item::OAK_DOOR),
            Block::QUARTZ_ORE => Some(Item::NETHER_QUARTZ),
            // Glowstone breaks into dust (see `World::spill_block`).
            Block::GLOWSTONE | Block::NETHER_PORTAL => None,
            b if b.is_leaves() => None,
            Block::GLASS | Block::BEDROCK | Block::TALL_GRASS | Block::FERN | Block::ICE => None,
            b if b.is_fluid() || b == Block::AIR => None,
            b => Some(b.into()),
        }
    }

    /// Minecraft's hardness: mining takes 1.5x this many seconds when the
    /// held item can harvest the block and 5x when it can't, divided by the
    /// tool's speed (see `crate::mining`). Infinite for unbreakable blocks.
    pub fn hardness(self) -> f32 {
        match self.material() {
            b if b.kind() == RenderKind::Cross => 0.0,
            Block::TNT => 0.0,
            b if b.is_leaves() => 0.2,
            Block::SNOW => 0.2,
            Block::GLASS | Block::GLOWSTONE => 0.3,
            Block::CACTUS | Block::NETHERRACK => 0.4,
            Block::SOUL_SAND => 0.5,
            Block::NETHER_BRICKS => 2.0,
            Block::QUARTZ_ORE => 3.0,
            Block::NETHER_PORTAL => f32::INFINITY,
            Block::DIRT | Block::SAND | Block::RED_SAND | Block::ICE => 0.5,
            Block::GRASS | Block::SNOWY_GRASS | Block::GRAVEL | Block::FARMLAND | Block::WET_FARMLAND | Block::CLAY => {
                0.6
            }
            Block::SANDSTONE | Block::WOOL => 0.8,
            Block::BED_FOOT | Block::BED_HEAD => 0.2,
            Block::LADDER => 0.4,
            Block::OAK_DOOR => 3.0,
            Block::PUMPKIN | Block::MELON => 1.0,
            b if b.terracotta_colour().is_some() => 1.25,
            Block::STONE => 1.5,
            b if b.is_log() || b.is_planks() => 2.0,
            Block::COBBLESTONE | Block::BRICKS => 2.0,
            Block::CRAFTING_TABLE | Block::CHEST => 2.5,
            Block::COAL_ORE | Block::IRON_ORE | Block::GOLD_ORE | Block::DIAMOND_ORE => 3.0,
            Block::FURNACE | Block::LIT_FURNACE => 3.5,
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
            | Block::BRICKS
            | Block::SANDSTONE
            | Block::COAL_ORE
            | Block::IRON_ORE
            | Block::GOLD_ORE
            | Block::DIAMOND_ORE
            | Block::OBSIDIAN
            | Block::FURNACE
            | Block::LIT_FURNACE
            | Block::NETHERRACK
            | Block::QUARTZ_ORE
            | Block::NETHER_BRICKS
            | Block::ICE => Some(ToolKind::Pickaxe),
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
            Block::CRAFTING_TABLE | Block::CHEST | Block::PUMPKIN | Block::MELON | Block::LADDER | Block::OAK_DOOR => {
                Some(ToolKind::Axe)
            }
            _ => None,
        }
    }

    /// Pickaxe harvest level needed for any drop (0 wood or gold, 1 stone,
    /// 2 iron, 3 diamond); `None` if a bare hand will do.
    pub fn harvest_level(self) -> Option<u8> {
        match self.material() {
            Block::STONE
            | Block::COBBLESTONE
            | Block::BRICKS
            | Block::SANDSTONE
            | Block::COAL_ORE
            | Block::FURNACE
            | Block::LIT_FURNACE
            | Block::NETHERRACK
            | Block::QUARTZ_ORE
            | Block::NETHER_BRICKS => Some(0),
            b if b.terracotta_colour().is_some() => Some(0),
            Block::IRON_ORE => Some(1),
            Block::GOLD_ORE | Block::DIAMOND_ORE => Some(2),
            Block::OBSIDIAN => Some(3),
            _ => None,
        }
    }

    /// Every block a creative player can pick from.
    pub fn creative_palette() -> impl Iterator<Item = Block> {
        (1..=23u8)
            .chain(32..=37)
            .chain(42..=45)
            .chain([53, 57, 67, 68])
            .chain(69..=97)
            .chain(100..=103)
            .chain(105..=111)
            .chain((112..=132).step_by(4))
            .chain([136, 137, 141])
            .map(Block)
    }

    /// Blocks that placing another block overwrites (air, fluids, grass).
    pub fn is_replaceable(self) -> bool {
        self == Block::AIR
            || self.is_fluid()
            || self == Block::TALL_GRASS
            || self == Block::DEAD_BUSH
            || self == Block::FERN
    }

    /// Colour index of a terracotta block (see [`Block::terracotta`]).
    pub fn terracotta_colour(self) -> Option<u8> {
        (84..=90).contains(&self.0).then(|| self.0 - 84)
    }

    /// Whether the crosshair can select this block (anything visible but fluids).
    #[inline(always)]
    pub fn is_targetable(self) -> bool {
        self.kind() != RenderKind::Invisible && !self.is_fluid() && self != Block::NETHER_PORTAL
    }

    /// Sand and gravel fall when nothing holds them up.
    pub fn has_gravity(self) -> bool {
        matches!(self, Block::SAND | Block::RED_SAND | Block::GRAVEL)
    }

    /// Whether this block can rest on `below`. Plants need soil and torches
    /// a full block; everything else stays put.
    pub fn can_stay_on(self, below: Block) -> bool {
        match self {
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
        (0..=255u8)
            .map(Block)
            .find(|b| b.kind() != RenderKind::Invisible && b.name() == name)
            .or((name == "air").then_some(Block::AIR))
    }

    /// How much light is lost passing through this block, on top of the
    /// usual 1 per step. 15 fully blocks light.
    #[inline(always)]
    pub fn light_opacity(self) -> u8 {
        LIGHT_OPACITY[self.0 as usize]
    }

    /// Blocks light but doesn't fill its cell (slabs, stairs): lit like
    /// the brightest cell beside or above it.
    #[inline(always)]
    pub fn borrows_light(self) -> bool {
        BORROWS_LIGHT[self.0 as usize]
    }

    /// Light level emitted by this block.
    #[inline(always)]
    pub fn emission(self) -> u8 {
        match self.base() {
            Block::GLOWSTONE => 15,
            Block::TORCH => 14,
            Block::LIT_FURNACE => 13,
            Block::NETHER_PORTAL => 11,
            b if b.is_lava() => 15,
            _ => 0,
        }
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

const fn all(t: u8) -> [u8; 6] {
    [t; 6]
}

const fn column(side: u8, top: u8, bottom: u8) -> [u8; 6] {
    [side, side, top, bottom, side, side]
}

/// Side textures with `front` on the face `facing` points out of.
const fn fronted(front: u8, side: u8, top: u8, facing: Facing) -> [u8; 6] {
    let mut t = [side, side, top, top, side, side];
    t[facing.face()] = front;
    t
}

const fn make(id: u8) -> BlockInfo {
    use RenderKind::*;
    let (name, kind, tex) = match id {
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
        _ => ("unknown", Invisible, all(0)),
    };
    // Ice is see-through like water but solid underfoot.
    let solid = matches!(kind, Opaque | Cutout | Shaped) || id == 97;
    BlockInfo { name, kind, solid, self_cull: matches!(id, 5 | 10 | 97 | 104), tex }
}

pub static INFO: [BlockInfo; 256] = {
    let mut arr = [make(0); 256];
    let mut i = 0;
    while i < 256 {
        arr[i] = make(i as u8);
        i += 1;
    }
    arr
};

static LIGHT_OPACITY: [u8; 256] = {
    let mut arr = [15u8; 256];
    let mut i = 0;
    while i < 256 {
        arr[i] = match INFO[i].kind {
            RenderKind::Opaque => 15,
            // Slabs and stairs keep the light out, like Minecraft's (they
            // borrow light from their neighbours instead; see `borrows_light`).
            _ if matches!(i, 106..=135) => 15,
            RenderKind::Invisible | RenderKind::Cross | RenderKind::Shaped => 0,
            _ if matches!(i, 10 | 98 | 99) => 0, // glass, beds
            _ => 1,                              // leaves, water: dim light passing through
        };
        i += 1;
    }
    arr
};

static BORROWS_LIGHT: [bool; 256] = {
    let mut arr = [false; 256];
    let mut i = 0;
    while i < 256 {
        arr[i] = LIGHT_OPACITY[i] >= 15 && !matches!(INFO[i].kind, RenderKind::Opaque);
        i += 1;
    }
    arr
};

static OPAQUE: [bool; 256] = {
    let mut arr = [false; 256];
    let mut i = 0;
    while i < 256 {
        arr[i] = matches!(INFO[i].kind, RenderKind::Opaque);
        i += 1;
    }
    arr
};

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(tex::untinted(tex::ITEM_0), None);
    }

    #[test]
    fn slabs_are_half_blocks_that_mine_like_their_base() {
        for (i, &base) in Block::SLAB_BASES.iter().enumerate() {
            let slab = Block::slab_of(base).unwrap();
            assert_eq!(slab, Block(Block::STONE_SLAB.0 + i as u8));
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
            assert_eq!(stairs, Block(Block::STONE_STAIRS.0 + 4 * i as u8));
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
}
