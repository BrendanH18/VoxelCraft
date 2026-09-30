//! Block registry: ids, render classification and per-face texture layers.
//!
//! Block properties live in a 256-entry static table so hot loops (meshing,
//! physics) resolve them with a single indexed load instead of a `match`.

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
    pub const COUNT: u32 = 27;
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
    pub fn is_solid(self) -> bool {
        self.info().solid
    }

    pub fn name(self) -> &'static str {
        self.info().name
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
        _ => ("unknown", Invisible, all(0)),
    };
    BlockInfo {
        name,
        kind,
        solid: matches!(kind, Opaque | Cutout),
        self_cull: id == 5 || id == 10,
        tex,
    }
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

static OPAQUE: [bool; 256] = {
    let mut arr = [false; 256];
    let mut i = 0;
    while i < 256 {
        arr[i] = matches!(INFO[i].kind, RenderKind::Opaque);
        i += 1;
    }
    arr
};
