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
    /// Flat item icons (see `item::Item::icon_layer`), up to 64 of them.
    pub const ITEM_0: u8 = 96;
    pub const COUNT: u32 = 160;
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

    pub const fn flowing_water(level: u8) -> Block {
        Block(23 + level)
    }

    pub const fn flowing_lava(level: u8) -> Block {
        Block(37 + level)
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

    /// What breaking this block yields in survival.
    pub fn drop(self) -> Option<Item> {
        match self {
            Block::STONE => Some(Block::COBBLESTONE.into()),
            Block::GRASS | Block::SNOWY_GRASS => Some(Block::DIRT.into()),
            Block::COAL_ORE => Some(Item::COAL),
            Block::DIAMOND_ORE => Some(Item::DIAMOND),
            Block::DEAD_BUSH => Some(Item::STICK),
            Block::LIT_FURNACE => Some(Block::FURNACE.into()),
            Block::LEAVES | Block::SPRUCE_LEAVES | Block::GLASS | Block::BEDROCK | Block::TALL_GRASS => None,
            b if b.is_fluid() || b == Block::AIR => None,
            b => Some(b.into()),
        }
    }

    /// Minecraft's hardness: mining takes 1.5x this many seconds when the
    /// held item can harvest the block and 5x when it can't, divided by the
    /// tool's speed (see `crate::mining`). Infinite for unbreakable blocks.
    pub fn hardness(self) -> f32 {
        match self {
            b if b.kind() == RenderKind::Cross => 0.0,
            Block::LEAVES | Block::SPRUCE_LEAVES | Block::SNOW => 0.2,
            Block::GLASS | Block::GLOWSTONE => 0.3,
            Block::CACTUS => 0.4,
            Block::DIRT | Block::SAND => 0.5,
            Block::GRASS | Block::SNOWY_GRASS | Block::GRAVEL => 0.6,
            Block::SANDSTONE | Block::WOOL => 0.8,
            Block::STONE => 1.5,
            Block::LOG | Block::PLANKS | Block::COBBLESTONE | Block::BRICKS => 2.0,
            Block::CRAFTING_TABLE => 2.5,
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
        match self {
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
            | Block::LIT_FURNACE => Some(ToolKind::Pickaxe),
            Block::DIRT | Block::GRASS | Block::SNOWY_GRASS | Block::SAND | Block::GRAVEL | Block::SNOW => {
                Some(ToolKind::Shovel)
            }
            Block::LOG | Block::PLANKS | Block::CRAFTING_TABLE => Some(ToolKind::Axe),
            _ => None,
        }
    }

    /// Pickaxe harvest level needed for any drop (0 wood or gold, 1 stone,
    /// 2 iron, 3 diamond); `None` if a bare hand will do.
    pub fn harvest_level(self) -> Option<u8> {
        match self {
            Block::STONE
            | Block::COBBLESTONE
            | Block::BRICKS
            | Block::SANDSTONE
            | Block::COAL_ORE
            | Block::FURNACE
            | Block::LIT_FURNACE => Some(0),
            Block::IRON_ORE => Some(1),
            Block::GOLD_ORE | Block::DIAMOND_ORE => Some(2),
            Block::OBSIDIAN => Some(3),
            _ => None,
        }
    }

    /// Every block a creative player can pick from.
    pub fn creative_palette() -> impl Iterator<Item = Block> {
        (1..=23u8).chain(32..=37).chain(42..=45).map(Block)
    }

    /// Blocks that placing another block overwrites (air, fluids, grass).
    pub fn is_replaceable(self) -> bool {
        self == Block::AIR || self.is_fluid() || self == Block::TALL_GRASS || self == Block::DEAD_BUSH
    }

    /// Whether the crosshair can select this block (anything visible but fluids).
    #[inline(always)]
    pub fn is_targetable(self) -> bool {
        self.kind() != RenderKind::Invisible && !self.is_fluid()
    }

    /// Sand and gravel fall when nothing holds them up.
    pub fn has_gravity(self) -> bool {
        matches!(self, Block::SAND | Block::GRAVEL)
    }

    /// Whether this block can rest on `below`. Plants need soil and torches
    /// a full block; everything else stays put.
    pub fn can_stay_on(self, below: Block) -> bool {
        match self {
            Block::TALL_GRASS | Block::DANDELION | Block::POPPY => {
                matches!(below, Block::GRASS | Block::DIRT | Block::SNOWY_GRASS)
            }
            Block::DEAD_BUSH => matches!(below, Block::SAND | Block::DIRT | Block::GRASS),
            Block::TORCH => below.is_opaque(),
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

    /// Light level emitted by this block.
    #[inline(always)]
    pub fn emission(self) -> u8 {
        match self {
            Block::GLOWSTONE => 15,
            Block::TORCH => 14,
            Block::LIT_FURNACE => 13,
            b if b.is_lava() => 15,
            _ => 0,
        }
    }
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

const fn make(id: u8) -> BlockInfo {
    use RenderKind::*;
    let (name, kind, tex) = match id {
        0 => ("air", Invisible, all(0)),
        1 => ("stone", Opaque, all(tex::STONE)),
        2 => ("dirt", Opaque, all(tex::DIRT)),
        3 => ("grass", Opaque, column(tex::GRASS_SIDE, tex::GRASS_TOP, tex::DIRT)),
        4 => ("sand", Opaque, all(tex::SAND)),
        5 => ("water", Translucent, all(tex::WATER)),
        6 => ("log", Opaque, column(tex::LOG_SIDE, tex::LOG_TOP, tex::LOG_TOP)),
        7 => ("leaves", Cutout, all(tex::LEAVES)),
        8 => ("planks", Opaque, all(tex::PLANKS)),
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
        45 => ("furnace", Opaque, column(tex::FURNACE_FRONT, tex::FURNACE_TOP, tex::FURNACE_TOP)),
        46 => ("lit furnace", Opaque, column(tex::FURNACE_LIT, tex::FURNACE_TOP, tex::FURNACE_TOP)),
        _ => ("unknown", Invisible, all(0)),
    };
    BlockInfo { name, kind, solid: matches!(kind, Opaque | Cutout), self_cull: id == 5 || id == 10, tex }
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
            RenderKind::Invisible | RenderKind::Cross => 0,
            _ if i == 10 => 0, // glass
            _ => 1,            // leaves, water: dim light passing through
        };
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
}
