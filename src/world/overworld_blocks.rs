//! Append-only Overworld terrain, cave, ocean and plant blocks added for the
//! 1.18+ world (v0.6), ids 1900..=2099.
//!
//! | Ids | Blocks |
//! |---|---|
//! | 1900..=1913 | podzol, coarse/rooted dirt, mycelium, mud family, moss, pale moss, blue ice, snow layer, carpets, dripstone |
//! | 1914..=1923 | pointed dripstone: up then down, each tip, tip merge, frustum, middle, base |
//! | 1924..=1934 | amethyst block, budding amethyst, buds and clusters up (4) then down (4), smooth basalt |
//! | 1940..=1953 | lush cave plants: azaleas, cave vines, spore blossom, hanging roots, dripleaves |
//! | 1960..=1979 | ocean plants and blocks: kelp, seagrass, sea pickles, prismarine, sponges, turtle eggs |
//! | 1980..=2009 | coral blocks, coral and coral fans, live then dead, five colours each |
//! | 2010..=2031 | dark oak, mangrove, cherry and pale oak leaves and saplings; pale oak wood; roots; bamboo; petals; hanging moss |
//! | 2032..=2076 | flowers, double plants, lily pad, vines, sweet berries, cocoa, mushroom blocks |
//!
//! Aquatic plants (kelp, seagrass, sea pickles, live coral and fans) are
//! always waterlogged: they hold a water source in their cell, render a
//! water surface around their cross, and leave water behind when broken.

use glam::IVec3;

use super::block::{Block, Facing, RenderKind};
use crate::item::{Item, ToolKind};

pub const FIRST: u16 = 1900;
pub const LAST: u16 = 2076;

/// First texture layer of this module (see `render::overworld_textures`).
pub const TEX: u16 = 1450;
/// One past the last texture layer used.
pub const TEX_END: u16 = TEX + t::COUNT;

/// Texture layers, relative to [`TEX`] once added.
pub mod t {
    use super::TEX;
    pub const PODZOL_TOP: u16 = TEX;
    pub const PODZOL_SIDE: u16 = TEX + 1;
    pub const COARSE_DIRT: u16 = TEX + 2;
    pub const ROOTED_DIRT: u16 = TEX + 3;
    pub const MYCELIUM_TOP: u16 = TEX + 4;
    pub const MYCELIUM_SIDE: u16 = TEX + 5;
    pub const MUD: u16 = TEX + 6;
    pub const PACKED_MUD: u16 = TEX + 7;
    pub const MUD_BRICKS: u16 = TEX + 8;
    pub const MOSS: u16 = TEX + 9;
    pub const PALE_MOSS: u16 = TEX + 10;
    pub const BLUE_ICE: u16 = TEX + 11;
    pub const DRIPSTONE: u16 = TEX + 12;
    /// Pointed dripstone, up then down: tip, tip merge, frustum, middle, base.
    pub const POINTED: u16 = TEX + 13;
    pub const AMETHYST: u16 = TEX + 23;
    pub const BUDDING_AMETHYST: u16 = TEX + 24;
    /// Small, medium and large buds and the cluster, up then down.
    pub const BUDS: u16 = TEX + 25;
    pub const SMOOTH_BASALT: u16 = TEX + 33;
    pub const AZALEA_TOP: u16 = TEX + 34;
    pub const AZALEA_SIDE: u16 = TEX + 35;
    pub const FLOWERING_AZALEA_TOP: u16 = TEX + 36;
    pub const FLOWERING_AZALEA_SIDE: u16 = TEX + 37;
    pub const AZALEA_LEAVES: u16 = TEX + 38;
    pub const FLOWERING_AZALEA_LEAVES: u16 = TEX + 39;
    /// Cave vines head, head with berries, plant, plant with berries.
    pub const CAVE_VINES: u16 = TEX + 40;
    pub const SPORE_BLOSSOM: u16 = TEX + 44;
    pub const HANGING_ROOTS: u16 = TEX + 45;
    pub const BIG_DRIPLEAF_TOP: u16 = TEX + 46;
    pub const BIG_DRIPLEAF_SIDE: u16 = TEX + 47;
    pub const BIG_DRIPLEAF_STEM: u16 = TEX + 48;
    pub const SMALL_DRIPLEAF: u16 = TEX + 49;
    pub const SMALL_DRIPLEAF_TOP: u16 = TEX + 50;
    pub const KELP: u16 = TEX + 51;
    pub const KELP_PLANT: u16 = TEX + 52;
    pub const SEAGRASS: u16 = TEX + 53;
    pub const TALL_SEAGRASS_BOTTOM: u16 = TEX + 54;
    pub const TALL_SEAGRASS_TOP: u16 = TEX + 55;
    /// One to four sea pickles.
    pub const SEA_PICKLES: u16 = TEX + 56;
    pub const DRIED_KELP_SIDE: u16 = TEX + 60;
    pub const DRIED_KELP_TOP: u16 = TEX + 61;
    pub const PRISMARINE: u16 = TEX + 62;
    pub const PRISMARINE_BRICKS: u16 = TEX + 63;
    pub const DARK_PRISMARINE: u16 = TEX + 64;
    pub const SEA_LANTERN: u16 = TEX + 65;
    pub const SPONGE: u16 = TEX + 66;
    pub const WET_SPONGE: u16 = TEX + 67;
    /// One to four turtle eggs.
    pub const TURTLE_EGGS: u16 = TEX + 68;
    /// Coral blocks, dead coral blocks, coral, dead coral, fans, dead fans:
    /// five colours each (tube, brain, bubble, fire, horn).
    pub const CORAL: u16 = TEX + 72;
    pub const DARK_OAK_LEAVES: u16 = TEX + 102;
    pub const MANGROVE_LEAVES: u16 = TEX + 103;
    pub const CHERRY_LEAVES: u16 = TEX + 104;
    pub const PALE_OAK_LEAVES: u16 = TEX + 105;
    pub const DARK_OAK_SAPLING: u16 = TEX + 106;
    pub const MANGROVE_PROPAGULE: u16 = TEX + 107;
    pub const CHERRY_SAPLING: u16 = TEX + 108;
    pub const PALE_OAK_SAPLING: u16 = TEX + 109;
    pub const PALE_OAK_LOG_SIDE: u16 = TEX + 110;
    pub const PALE_OAK_LOG_TOP: u16 = TEX + 111;
    pub const PALE_OAK_PLANKS: u16 = TEX + 112;
    pub const MANGROVE_ROOTS_SIDE: u16 = TEX + 113;
    pub const MANGROVE_ROOTS_TOP: u16 = TEX + 114;
    pub const MUDDY_ROOTS_SIDE: u16 = TEX + 115;
    pub const MUDDY_ROOTS_TOP: u16 = TEX + 116;
    pub const BAMBOO: u16 = TEX + 117;
    pub const BAMBOO_SMALL_LEAVES: u16 = TEX + 118;
    pub const BAMBOO_LARGE_LEAVES: u16 = TEX + 119;
    pub const BAMBOO_SAPLING: u16 = TEX + 120;
    pub const BAMBOO_BLOCK_SIDE: u16 = TEX + 121;
    pub const BAMBOO_BLOCK_TOP: u16 = TEX + 122;
    pub const BAMBOO_PLANKS: u16 = TEX + 123;
    pub const BAMBOO_MOSAIC: u16 = TEX + 124;
    pub const PINK_PETALS: u16 = TEX + 125;
    pub const PALE_HANGING_MOSS_TIP: u16 = TEX + 126;
    pub const PALE_HANGING_MOSS: u16 = TEX + 127;
    /// Cornflower, lily of the valley, oxeye daisy, azure bluet, allium,
    /// then red, orange, white and pink tulips.
    pub const FLOWERS: u16 = TEX + 128;
    /// Sunflower, lilac, rose bush, peony, large fern, tall grass: bottom
    /// then top half each.
    pub const DOUBLE: u16 = TEX + 137;
    pub const LILY_PAD: u16 = TEX + 149;
    pub const VINE: u16 = TEX + 150;
    /// Sweet berry bush ages 0..=3.
    pub const BERRY_BUSH: u16 = TEX + 151;
    /// Cocoa pods ages 0..=2.
    pub const COCOA: u16 = TEX + 155;
    pub const RED_MUSHROOM_BLOCK: u16 = TEX + 158;
    pub const BROWN_MUSHROOM_BLOCK: u16 = TEX + 159;
    pub const MUSHROOM_STEM: u16 = TEX + 160;
    pub const SNOW_LAYER: u16 = TEX + 161;
    pub const COUNT: u16 = 162;
}

pub const PODZOL: Block = Block(1900);
pub const COARSE_DIRT: Block = Block(1901);
pub const ROOTED_DIRT: Block = Block(1902);
pub const MYCELIUM: Block = Block(1903);
pub const MUD: Block = Block(1904);
pub const PACKED_MUD: Block = Block(1905);
pub const MUD_BRICKS: Block = Block(1906);
pub const MOSS_BLOCK: Block = Block(1907);
pub const PALE_MOSS_BLOCK: Block = Block(1908);
pub const BLUE_ICE: Block = Block(1909);
pub const SNOW_LAYER: Block = Block(1910);
pub const MOSS_CARPET: Block = Block(1911);
pub const PALE_MOSS_CARPET: Block = Block(1912);
pub const DRIPSTONE_BLOCK: Block = Block(1913);
pub const AMETHYST_BLOCK: Block = Block(1924);
pub const BUDDING_AMETHYST: Block = Block(1925);
pub const SMOOTH_BASALT: Block = Block(1934);
pub const AZALEA: Block = Block(1940);
pub const FLOWERING_AZALEA: Block = Block(1941);
pub const AZALEA_LEAVES: Block = Block(1942);
pub const FLOWERING_AZALEA_LEAVES: Block = Block(1943);
pub const CAVE_VINES: Block = Block(1944);
pub const CAVE_VINES_LIT: Block = Block(1945);
pub const CAVE_VINES_PLANT: Block = Block(1946);
pub const CAVE_VINES_PLANT_LIT: Block = Block(1947);
pub const SPORE_BLOSSOM: Block = Block(1948);
pub const HANGING_ROOTS: Block = Block(1949);
pub const BIG_DRIPLEAF: Block = Block(1950);
pub const BIG_DRIPLEAF_STEM: Block = Block(1951);
pub const SMALL_DRIPLEAF: Block = Block(1952);
pub const SMALL_DRIPLEAF_TOP: Block = Block(1953);
pub const KELP: Block = Block(1960);
pub const KELP_PLANT: Block = Block(1961);
pub const SEAGRASS: Block = Block(1962);
pub const TALL_SEAGRASS: Block = Block(1963);
pub const TALL_SEAGRASS_TOP: Block = Block(1964);
pub const SEA_PICKLE: Block = Block(1965);
pub const DRIED_KELP_BLOCK: Block = Block(1969);
pub const PRISMARINE: Block = Block(1970);
pub const PRISMARINE_BRICKS: Block = Block(1971);
pub const DARK_PRISMARINE: Block = Block(1972);
pub const SEA_LANTERN: Block = Block(1973);
pub const SPONGE: Block = Block(1974);
pub const WET_SPONGE: Block = Block(1975);
pub const TURTLE_EGG: Block = Block(1976);
pub const DARK_OAK_LEAVES: Block = Block(2010);
pub const MANGROVE_LEAVES: Block = Block(2011);
pub const CHERRY_LEAVES: Block = Block(2012);
pub const PALE_OAK_LEAVES: Block = Block(2013);
pub const DARK_OAK_SAPLING: Block = Block(2014);
pub const MANGROVE_PROPAGULE: Block = Block(2015);
pub const CHERRY_SAPLING: Block = Block(2016);
pub const PALE_OAK_SAPLING: Block = Block(2017);
pub const PALE_OAK_LOG: Block = Block(2018);
pub const PALE_OAK_PLANKS: Block = Block(2019);
pub const MANGROVE_ROOTS: Block = Block(2020);
pub const MUDDY_MANGROVE_ROOTS: Block = Block(2021);
pub const BAMBOO: Block = Block(2022);
pub const BAMBOO_SMALL_LEAVES: Block = Block(2023);
pub const BAMBOO_LARGE_LEAVES: Block = Block(2024);
pub const BAMBOO_SAPLING: Block = Block(2025);
pub const BAMBOO_BLOCK: Block = Block(2026);
pub const BAMBOO_PLANKS: Block = Block(2027);
pub const BAMBOO_MOSAIC: Block = Block(2028);
pub const PINK_PETALS: Block = Block(2029);
pub const PALE_HANGING_MOSS_TIP: Block = Block(2030);
pub const PALE_HANGING_MOSS: Block = Block(2031);
pub const CORNFLOWER: Block = Block(2032);
pub const LILY_OF_THE_VALLEY: Block = Block(2033);
pub const OXEYE_DAISY: Block = Block(2034);
pub const AZURE_BLUET: Block = Block(2035);
pub const ALLIUM: Block = Block(2036);
pub const RED_TULIP: Block = Block(2037);
pub const ORANGE_TULIP: Block = Block(2038);
pub const WHITE_TULIP: Block = Block(2039);
pub const PINK_TULIP: Block = Block(2040);
/// Lower halves; the upper half of each is the next id.
pub const SUNFLOWER: Block = Block(2041);
pub const LILAC: Block = Block(2043);
pub const ROSE_BUSH: Block = Block(2045);
pub const PEONY: Block = Block(2047);
pub const LARGE_FERN: Block = Block(2049);
pub const DOUBLE_TALL_GRASS: Block = Block(2051);
pub const LILY_PAD: Block = Block(2053);
/// Vines on the wall to their south; north, east and west follow (`Facing::ALL`).
pub const VINE: Block = Block(2054);
pub const SWEET_BERRY_BUSH: Block = Block(2058);
/// Age 0 on a log to the south; 12 states: age * 4 + the log's side (`Facing::ALL`).
pub const COCOA: Block = Block(2062);
pub const RED_MUSHROOM_BLOCK: Block = Block(2074);
pub const BROWN_MUSHROOM_BLOCK: Block = Block(2075);
pub const MUSHROOM_STEM: Block = Block(2076);

/// Java's coral colours, in id order.
pub const CORAL_NAMES: [&str; 5] = ["tube", "brain", "bubble", "fire", "horn"];

/// A coral family member: `kind` 0 block, 1 plant, 2 fan.
pub const fn coral(kind: u16, colour: u16, dead: bool) -> Block {
    Block(1980 + kind * 10 + dead as u16 * 5 + colour)
}

/// `(kind, colour, dead)` of a coral block, plant or fan.
pub const fn coral_of(b: Block) -> Option<(u16, u16, bool)> {
    if b.0 < 1980 || b.0 > 2009 {
        return None;
    }
    let i = b.0 - 1980;
    Some((i / 10, i % 5, i % 10 >= 5))
}

/// Pointed dripstone thickness, tip first (Java's `DripstoneThickness`;
/// the plain tip is first so the item names the default state).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Thickness {
    Tip,
    TipMerge,
    Frustum,
    Middle,
    Base,
}

pub const fn pointed_dripstone(down: bool, thickness: Thickness) -> Block {
    Block(1914 + down as u16 * 5 + thickness as u16)
}

/// Whether a pointed dripstone points down, and how thick it is here.
pub const fn dripstone_of(b: Block) -> Option<(bool, Thickness)> {
    if b.0 < 1914 || b.0 > 1923 {
        return None;
    }
    let i = b.0 - 1914;
    let t = match i % 5 {
        0 => Thickness::Tip,
        1 => Thickness::TipMerge,
        2 => Thickness::Frustum,
        3 => Thickness::Middle,
        _ => Thickness::Base,
    };
    Some((i >= 5, t))
}

/// An amethyst bud (`size` 0 small, 1 medium, 2 large) or cluster (3),
/// growing up from a floor or down from a ceiling.
pub const fn amethyst_bud(size: u16, down: bool) -> Block {
    Block(1926 + down as u16 * 4 + size)
}

pub const fn bud_of(b: Block) -> Option<(u16, bool)> {
    if b.0 < 1926 || b.0 > 1933 {
        return None;
    }
    Some(((b.0 - 1926) % 4, b.0 >= 1930))
}

pub const fn sea_pickles(count: u8) -> Block {
    Block(
        1964 + if count == 0 {
            1
        } else if count > 4 {
            4
        } else {
            count as u16
        },
    )
}

pub const fn pickle_count(b: Block) -> Option<u8> {
    if b.0 >= 1965 && b.0 <= 1968 { Some((b.0 - 1964) as u8) } else { None }
}

pub const fn turtle_eggs(count: u8) -> Block {
    Block(
        1975 + if count == 0 {
            1
        } else if count > 4 {
            4
        } else {
            count as u16
        },
    )
}

pub const fn egg_count(b: Block) -> Option<u8> {
    if b.0 >= 1976 && b.0 <= 1979 {
        Some((b.0 - 1975) as u8)
    } else if b.0 >= 2150 && b.0 <= 2157 {
        Some(((b.0 - 2150) % 4 + 1) as u8)
    } else {
        None
    }
}

pub const fn vine(wall: Facing) -> Block {
    Block(2054 + wall as u16)
}

/// The wall a vine hangs on.
pub fn vine_wall(b: Block) -> Option<Facing> {
    (2054..=2057).contains(&b.0).then(|| Facing::ALL[(b.0 - 2054) as usize])
}

pub const fn berry_bush(age: u8) -> Block {
    Block(2058 + if age > 3 { 3 } else { age as u16 })
}

pub const fn berry_age(b: Block) -> Option<u8> {
    if b.0 >= 2058 && b.0 <= 2061 { Some((b.0 - 2058) as u8) } else { None }
}

/// A cocoa pod of `age` on the log to its `log` side.
pub const fn cocoa(age: u8, log: Facing) -> Block {
    Block(2062 + if age > 2 { 2 } else { age as u16 } * 4 + log as u16)
}

pub fn cocoa_of(b: Block) -> Option<(u8, Facing)> {
    (2062..=2073).contains(&b.0).then(|| (((b.0 - 2062) / 4) as u8, Facing::ALL[((b.0 - 2062) % 4) as usize]))
}

/// The two halves of a double plant: (lower, upper).
pub const DOUBLE_PLANTS: [Block; 6] = [SUNFLOWER, LILAC, ROSE_BUSH, PEONY, LARGE_FERN, DOUBLE_TALL_GRASS];

/// v0.7 cracked egg states, assigned block ids 2150..=2157.
pub const fn egg_stage(b: Block) -> Option<u8> {
    if b.0 >= 1976 && b.0 <= 1979 {
        Some(0)
    } else if b.0 >= 2150 && b.0 <= 2157 {
        Some(((b.0 - 2150) / 4 + 1) as u8)
    } else {
        None
    }
}
pub const fn turtle_eggs_stage(count: u8, stage: u8) -> Block {
    let count = if count < 1 {
        1
    } else if count > 4 {
        4
    } else {
        count
    };
    if stage == 0 {
        turtle_eggs(count)
    } else {
        Block(2150 + (if stage > 2 { 2 } else { stage } as u16 - 1) * 4 + count as u16 - 1)
    }
}
/// Lower half of a double plant (itself for the lower half), and whether `b` is the upper half.
pub const fn double_of(b: Block) -> Option<(Block, bool)> {
    if b.0 < 2041 || b.0 > 2052 {
        return None;
    }
    let i = b.0 - 2041;
    Some((Block(2041 + i / 2 * 2), i % 2 == 1))
}

/// Blocks that hold a water source in their cell.
#[inline]
pub const fn waterlogged(id: u16) -> bool {
    matches!(id, 1960..=1968 | 1990..=1994 | 2000..=2004)
}

const fn all(t: u16) -> [u16; 6] {
    [t; 6]
}

const fn column(side: u16, top: u16, bottom: u16) -> [u16; 6] {
    [side, side, top, bottom, side, side]
}

/// Name, render kind and face textures of every id.
pub const fn registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    use RenderKind::*;
    const DIRT: u16 = super::block::tex::DIRT;
    Some(match id {
        1900 => ("podzol", Opaque, column(t::PODZOL_SIDE, t::PODZOL_TOP, DIRT)),
        1901 => ("coarse dirt", Opaque, all(t::COARSE_DIRT)),
        1902 => ("rooted dirt", Opaque, all(t::ROOTED_DIRT)),
        1903 => ("mycelium", Opaque, column(t::MYCELIUM_SIDE, t::MYCELIUM_TOP, DIRT)),
        1904 => ("mud", Opaque, all(t::MUD)),
        1905 => ("packed mud", Opaque, all(t::PACKED_MUD)),
        1906 => ("mud bricks", Opaque, all(t::MUD_BRICKS)),
        1907 => ("moss block", Opaque, all(t::MOSS)),
        1908 => ("pale moss block", Opaque, all(t::PALE_MOSS)),
        1909 => ("blue ice", Opaque, all(t::BLUE_ICE)),
        1910 => ("snow layer", Cutout, all(t::SNOW_LAYER)),
        1911 => ("moss carpet", Cutout, all(t::MOSS)),
        1912 => ("pale moss carpet", Cutout, all(t::PALE_MOSS)),
        1913 => ("dripstone block", Opaque, all(t::DRIPSTONE)),
        1914..=1923 => ("pointed dripstone", Cross, all(t::POINTED + (id - 1914))),
        1924 => ("block of amethyst", Opaque, all(t::AMETHYST)),
        1925 => ("budding amethyst", Opaque, all(t::BUDDING_AMETHYST)),
        1926..=1933 => {
            const NAMES: [&str; 4] =
                ["small amethyst bud", "medium amethyst bud", "large amethyst bud", "amethyst cluster"];
            (NAMES[((id - 1926) % 4) as usize], Cross, all(t::BUDS + (id - 1926)))
        }
        1934 => ("smooth basalt", Opaque, all(t::SMOOTH_BASALT)),
        1940 => ("azalea", Cutout, column(t::AZALEA_SIDE, t::AZALEA_TOP, t::AZALEA_SIDE)),
        1941 => ("flowering azalea", Cutout, column(t::FLOWERING_AZALEA_SIDE, t::FLOWERING_AZALEA_TOP, t::AZALEA_SIDE)),
        1942 => ("azalea leaves", Cutout, all(t::AZALEA_LEAVES)),
        1943 => ("flowering azalea leaves", Cutout, all(t::FLOWERING_AZALEA_LEAVES)),
        1944 => ("cave vines", Cross, all(t::CAVE_VINES)),
        1945 => ("cave vines", Cross, all(t::CAVE_VINES + 1)),
        1946 => ("cave vines plant", Cross, all(t::CAVE_VINES + 2)),
        1947 => ("cave vines plant", Cross, all(t::CAVE_VINES + 3)),
        1948 => ("spore blossom", Cross, all(t::SPORE_BLOSSOM)),
        1949 => ("hanging roots", Cross, all(t::HANGING_ROOTS)),
        1950 => ("big dripleaf", Shaped, column(t::BIG_DRIPLEAF_SIDE, t::BIG_DRIPLEAF_TOP, t::BIG_DRIPLEAF_SIDE)),
        1951 => ("big dripleaf stem", Cross, all(t::BIG_DRIPLEAF_STEM)),
        1952 => ("small dripleaf", Cross, all(t::SMALL_DRIPLEAF)),
        1953 => ("small dripleaf top", Cross, all(t::SMALL_DRIPLEAF_TOP)),
        1960 => ("kelp", Cross, all(t::KELP)),
        1961 => ("kelp plant", Cross, all(t::KELP_PLANT)),
        1962 => ("seagrass", Cross, all(t::SEAGRASS)),
        1963 => ("tall seagrass", Cross, all(t::TALL_SEAGRASS_BOTTOM)),
        1964 => ("tall seagrass top", Cross, all(t::TALL_SEAGRASS_TOP)),
        1965..=1968 => ("sea pickle", Cross, all(t::SEA_PICKLES + (id - 1965))),
        1969 => ("dried kelp block", Opaque, column(t::DRIED_KELP_SIDE, t::DRIED_KELP_TOP, t::DRIED_KELP_TOP)),
        1970 => ("prismarine", Opaque, all(t::PRISMARINE)),
        1971 => ("prismarine bricks", Opaque, all(t::PRISMARINE_BRICKS)),
        1972 => ("dark prismarine", Opaque, all(t::DARK_PRISMARINE)),
        1973 => ("sea lantern", Opaque, all(t::SEA_LANTERN)),
        1974 => ("sponge", Opaque, all(t::SPONGE)),
        1975 => ("wet sponge", Opaque, all(t::WET_SPONGE)),
        1976..=1979 => ("turtle egg", Shaped, all(1750)),
        2150..=2157 => ("turtle egg", Shaped, all(1751 + (id - 2150) / 4)),
        1980..=2009 => {
            let i = id - 1980;
            let (kind, dead, colour) = (i / 10, i % 10 >= 5, (i % 5) as usize);
            let names: [[&str; 5]; 6] = [
                ["tube coral block", "brain coral block", "bubble coral block", "fire coral block", "horn coral block"],
                [
                    "dead tube coral block",
                    "dead brain coral block",
                    "dead bubble coral block",
                    "dead fire coral block",
                    "dead horn coral block",
                ],
                ["tube coral", "brain coral", "bubble coral", "fire coral", "horn coral"],
                ["dead tube coral", "dead brain coral", "dead bubble coral", "dead fire coral", "dead horn coral"],
                ["tube coral fan", "brain coral fan", "bubble coral fan", "fire coral fan", "horn coral fan"],
                [
                    "dead tube coral fan",
                    "dead brain coral fan",
                    "dead bubble coral fan",
                    "dead fire coral fan",
                    "dead horn coral fan",
                ],
            ];
            let row = (kind * 2 + dead as u16) as usize;
            (names[row][colour], if kind == 0 { Opaque } else { Cross }, all(t::CORAL + i))
        }
        2010 => ("dark oak leaves", Cutout, all(t::DARK_OAK_LEAVES)),
        2011 => ("mangrove leaves", Cutout, all(t::MANGROVE_LEAVES)),
        2012 => ("cherry leaves", Cutout, all(t::CHERRY_LEAVES)),
        2013 => ("pale oak leaves", Cutout, all(t::PALE_OAK_LEAVES)),
        2014 => ("dark oak sapling", Cross, all(t::DARK_OAK_SAPLING)),
        2015 => ("mangrove propagule", Cross, all(t::MANGROVE_PROPAGULE)),
        2016 => ("cherry sapling", Cross, all(t::CHERRY_SAPLING)),
        2017 => ("pale oak sapling", Cross, all(t::PALE_OAK_SAPLING)),
        2018 => ("pale oak log", Opaque, column(t::PALE_OAK_LOG_SIDE, t::PALE_OAK_LOG_TOP, t::PALE_OAK_LOG_TOP)),
        2019 => ("pale oak planks", Opaque, all(t::PALE_OAK_PLANKS)),
        2020 => {
            ("mangrove roots", Cutout, column(t::MANGROVE_ROOTS_SIDE, t::MANGROVE_ROOTS_TOP, t::MANGROVE_ROOTS_TOP))
        }
        2021 => ("muddy mangrove roots", Opaque, column(t::MUDDY_ROOTS_SIDE, t::MUDDY_ROOTS_TOP, t::MUDDY_ROOTS_TOP)),
        2022 => ("bamboo", Shaped, all(t::BAMBOO)),
        2023 => ("bamboo", Shaped, all(t::BAMBOO_SMALL_LEAVES)),
        2024 => ("bamboo", Shaped, all(t::BAMBOO_LARGE_LEAVES)),
        2025 => ("bamboo sapling", Cross, all(t::BAMBOO_SAPLING)),
        2026 => ("block of bamboo", Opaque, column(t::BAMBOO_BLOCK_SIDE, t::BAMBOO_BLOCK_TOP, t::BAMBOO_BLOCK_TOP)),
        2027 => ("bamboo planks", Opaque, all(t::BAMBOO_PLANKS)),
        2028 => ("bamboo mosaic", Opaque, all(t::BAMBOO_MOSAIC)),
        2029 => ("pink petals", Cutout, all(t::PINK_PETALS)),
        2030 => ("pale hanging moss", Cross, all(t::PALE_HANGING_MOSS_TIP)),
        2031 => ("pale hanging moss plant", Cross, all(t::PALE_HANGING_MOSS)),
        2032..=2040 => {
            const NAMES: [&str; 9] = [
                "cornflower",
                "lily of the valley",
                "oxeye daisy",
                "azure bluet",
                "allium",
                "red tulip",
                "orange tulip",
                "white tulip",
                "pink tulip",
            ];
            (NAMES[(id - 2032) as usize], Cross, all(t::FLOWERS + (id - 2032)))
        }
        2041..=2052 => {
            const NAMES: [&str; 6] = ["sunflower", "lilac", "rose bush", "peony", "large fern", "tall grass plant"];
            const UPPER: [&str; 6] =
                ["sunflower top", "lilac top", "rose bush top", "peony top", "large fern top", "tall grass plant top"];
            let i = id - 2041;
            let name = if i.is_multiple_of(2) { NAMES[(i / 2) as usize] } else { UPPER[(i / 2) as usize] };
            (name, Cross, all(t::DOUBLE + i))
        }
        2053 => ("lily pad", Cutout, all(t::LILY_PAD)),
        2054..=2057 => ("vines", Shaped, all(t::VINE)),
        2058..=2061 => ("sweet berry bush", Cross, all(t::BERRY_BUSH + (id - 2058))),
        2062..=2073 => ("cocoa", Shaped, all(t::COCOA + (id - 2062) / 4)),
        2074 => ("red mushroom block", Opaque, all(t::RED_MUSHROOM_BLOCK)),
        2075 => ("brown mushroom block", Opaque, all(t::BROWN_MUSHROOM_BLOCK)),
        2076 => ("mushroom stem", Opaque, all(t::MUSHROOM_STEM)),
        _ => return None,
    })
}

/// The block a state's item, drop and rules use: dripstone and buds point
/// up as tips, vines and cocoa face north, upper halves become lower ones,
/// plants become their head or youngest stage.
pub const fn base(id: u16) -> Option<Block> {
    Some(match id {
        1914..=1923 => pointed_dripstone(false, Thickness::Tip),
        1930..=1933 => Block(id - 4),
        1945 => CAVE_VINES,
        1946 | 1947 => CAVE_VINES,
        1951 => BIG_DRIPLEAF,
        1953 => SMALL_DRIPLEAF,
        1961 => KELP,
        1964 => TALL_SEAGRASS,
        1966..=1968 => SEA_PICKLE,
        1977..=1979 | 2150..=2157 => TURTLE_EGG,
        2023 | 2024 => BAMBOO,
        2031 => PALE_HANGING_MOSS_TIP,
        2042 | 2044 | 2046 | 2048 | 2050 | 2052 => Block(id - 1),
        2055..=2057 => VINE,
        2059..=2061 => SWEET_BERRY_BUSH,
        2063..=2073 => COCOA,
        _ => return None,
    })
}

/// Light emission (Java: sea lantern 15, glow berries 14, sea pickles
/// 6/9/12/15 by count, amethyst buds 1/2/4 and clusters 5).
pub const fn emission(id: u16) -> u8 {
    match id {
        1973 => 15,
        1945 | 1947 => 14,
        1965..=1968 => 3 + 3 * (id - 1964) as u8,
        1926 | 1930 => 1,
        1927 | 1931 => 2,
        1928 | 1932 => 4,
        1929 | 1933 => 5,
        _ => 0,
    }
}

/// `(hardness, best tool, pickaxe harvest level)` per Java's block
/// properties and `mineable/*` tags.
pub fn mining(b: Block) -> Option<(f32, Option<ToolKind>, Option<u8>)> {
    use ToolKind::*;
    let id = b.base().0;
    if !(FIRST..=LAST).contains(&id) {
        return None;
    }
    Some(match id {
        1900 | 1901 | 1902 | 1904 => (0.5, Some(Shovel), None),
        1903 => (0.6, Some(Shovel), None),
        1905 => (1.0, Some(Pickaxe), None),
        1906 => (1.5, Some(Pickaxe), Some(0)),
        1907 | 1908 => (0.1, Some(Hoe), None),
        1909 => (2.8, Some(Pickaxe), None),
        1910 => (0.1, Some(Shovel), Some(0)),
        1911 | 1912 => (0.1, Some(Hoe), None),
        1913 => (1.5, Some(Pickaxe), Some(0)),
        1914..=1923 => (1.5, Some(Pickaxe), Some(0)),
        1924 | 1925 => (1.5, Some(Pickaxe), Some(0)),
        1926..=1933 => (1.5, Some(Pickaxe), None),
        1934 => (1.25, Some(Pickaxe), Some(0)),
        1940 | 1941 | 1944..=1949 | 1951..=1968 | 1976..=1979 => (0.0, None, None),
        1942 | 1943 => (0.2, Some(Hoe), None),
        1950 => (0.1, Some(Axe), None),
        1969 => (0.5, Some(Hoe), None),
        1970..=1972 => (1.5, Some(Pickaxe), Some(0)),
        1973 => (0.3, None, None),
        1974 | 1975 => (0.6, Some(Hoe), None),
        1980..=1989 => (1.5, Some(Pickaxe), Some(0)),
        1990..=2009 => (0.0, None, None),
        2010..=2013 => (0.2, Some(Hoe), None),
        2014..=2017 | 2025 | 2029..=2053 | 2058..=2061 => (0.0, None, None),
        2018 | 2019 | 2026..=2028 => (2.0, Some(Axe), None),
        2020 => (0.7, Some(Axe), None),
        2021 => (0.7, Some(Shovel), None),
        2022..=2024 => (1.0, Some(Axe), None),
        2054..=2057 => (0.2, Some(Axe), None),
        2062..=2073 => (0.2, Some(Axe), None),
        2074..=2076 => (0.2, Some(Axe), None),
        _ => return None,
    })
}

/// What breaking the block drops (`Some(None)`: nothing, or extra drops in
/// `World::spill_mined` such as berries and pickles); `None` for blocks
/// this module doesn't own.
pub fn drop(b: Block) -> Option<Option<Item>> {
    if egg_count(b).is_some() {
        return Some(None);
    }
    if !(FIRST..=LAST).contains(&b.0) {
        return None;
    }
    let base = b.base();
    Some(match b.0 {
        1900 | 1903 => Some(Block::DIRT.into()),
        // Blue ice, budding amethyst, buds and turtle eggs need silk touch.
        1909 | 1925 | 1926..=1928 | 1930..=1932 | 1976..=1979 => None,
        // Snow layers give a snowball; clusters shards; both in `spill_mined`.
        1910 | 1929 | 1933 => None,
        // Cave vines drop glow berries only when lit (see `spill_mined`).
        1944..=1947 => None,
        // Shears-only plants.
        1949 | 1952 | 1953 | 1962..=1964 | 2030 | 2031 | 2049..=2052 | 2054..=2057 => None,
        1951 => Some(BIG_DRIPLEAF.into()),
        // Pickles: one per pickle (see `spill_mined`).
        1965..=1968 => None,
        // Live coral needs silk touch; blocks die into dead ones.
        1980..=1984 => Some(coral(0, b.0 - 1980, true).into()),
        1990..=2009 => None,
        // Leaves drop saplings by chance (see `spill_mined`).
        2010..=2013 | 1942 | 1943 => None,
        2042 | 2044 | 2046 | 2048 => None,
        // Berry bushes and cocoa drop their fruit (see `spill_mined`).
        2058..=2073 => None,
        // Mushroom blocks drop 0-2 mushrooms; stems nothing.
        2074..=2076 => None,
        2023 | 2024 => Some(BAMBOO.into()),
        2025 => Some(BAMBOO.into()),
        _ => Some(base.into()),
    })
}

/// Blocks shears (and silk touch) keep.
pub fn sheared_drop(b: Block) -> Option<Item> {
    match b.0 {
        1949 | 1952 | 1953 | 1962 | 2030 | 2031 | 2054..=2057 => Some(b.base().into()),
        1963 | 1964 => Some(SEAGRASS.into()),
        2049 | 2050 => Some(Block::FERN.into()),
        2051 | 2052 => Some(Block::TALL_GRASS.into()),
        2010..=2013 | 1942 | 1943 => Some(b.into()),
        _ => None,
    }
}

/// What silk touch keeps beyond shears: coral, buds, blue ice, eggs,
/// podzol and mycelium, mushroom blocks.
pub fn silk_drop(b: Block) -> Option<Item> {
    if egg_count(b).is_some() {
        return Some(TURTLE_EGG.into());
    }
    match b.0 {
        1900 | 1903 | 1909 | 1925 | 1926..=1933 | 1976..=1979 | 1990..=2009 | 1980..=1984 | 2074..=2076 => {
            Some(b.base().into())
        }
        1910 => Some(SNOW_LAYER.into()),
        _ => sheared_drop(b),
    }
}

/// Plants, snow and vines that placing a block overwrites.
pub fn replaceable(b: Block) -> bool {
    matches!(b.0, 1910 | 1949 | 1962..=1964 | 2049..=2052 | 2054..=2057 | 2030 | 2031)
}

/// Java's `BlockTags.CLIMBABLE` members from this module.
pub fn climbable(b: Block) -> bool {
    matches!(b.0, 1944..=1947 | 2054..=2057)
}

pub fn is_leaves(b: Block) -> bool {
    matches!(b.0, 1942 | 1943 | 2010..=2013)
}

pub fn is_sapling(b: Block) -> bool {
    matches!(b.0, 2014..=2017)
}

/// Soil for grass-like plants (Java's `BlockTags.DIRT` plus farmland).
pub fn dirt_like(b: Block) -> bool {
    matches!(b, Block::GRASS | Block::DIRT | Block::SNOWY_GRASS)
        || b.is_farmland()
        || matches!(b.0, 1900..=1904 | 1907 | 1908 | 2021)
}

fn sturdy(b: Block) -> bool {
    b.is_opaque() || b == Block::GLASS || b.stained_glass_color().is_some()
}

/// Whether `block` can rest on `below` (`None`: not a block of this module).
/// Hanging plants check the block above instead (see [`hangs_from`]).
pub fn can_stay_on(block: Block, below: Block) -> Option<bool> {
    let id = block.0;
    if egg_count(block).is_some() {
        return Some(true);
    }
    if !(FIRST..=LAST).contains(&id) {
        return None;
    }
    Some(match id {
        1910 => sturdy(below) || below.is_leaves(),
        1911 | 1912 | 2029 => below.is_solid() && below.kind() != RenderKind::Cross,
        1914..=1918 => sturdy(below) || matches!(dripstone_of(below), Some((false, _))),
        1926..=1929 => sturdy(below),
        1940 | 1941 => dirt_like(below) || below == Block::CLAY,
        1950 => {
            below == BIG_DRIPLEAF_STEM
                || dirt_like(below)
                || below == Block::CLAY
                || below == MUD
                || below == MOSS_BLOCK
        }
        1951 => below == BIG_DRIPLEAF_STEM || dirt_like(below) || below == Block::CLAY || below == MUD,
        1952 => dirt_like(below) || below == Block::CLAY || below == MUD,
        1953 => below == SMALL_DRIPLEAF,
        1960 | 1961 => below == KELP_PLANT || (sturdy(below) && below != Block::MAGMA),
        1962 | 1963 | 1965..=1968 | 1990..=1999 | 2000..=2009 => sturdy(below),
        1964 => below == TALL_SEAGRASS,
        1976..=1979 => below == Block::SAND,
        2014 | 2016 | 2017 => dirt_like(below),
        2015 => dirt_like(below) || below == Block::CLAY || below == MUD,
        2022..=2025 => {
            matches!(below.0, 2022..=2025) || dirt_like(below) || below == Block::SAND || below == Block::GRAVEL
        }
        2032..=2040 | 2041 | 2043 | 2045 | 2047 | 2049 | 2051 => dirt_like(below),
        2042 | 2044 | 2046 | 2048 | 2050 | 2052 => below.0 == id - 1,
        2053 => below.is_water(),
        2058..=2061 => dirt_like(below),
        // Wall plants check their wall, ceiling plants the block above.
        _ => true,
    })
}

/// Whether a ceiling plant at a cell can hang under `above` (`None`: not a
/// hanging plant).
pub fn hangs_from(block: Block, above: Block) -> Option<bool> {
    Some(match block.0 {
        1919..=1923 => sturdy(above) || matches!(dripstone_of(above), Some((true, _))),
        1930..=1933 => sturdy(above),
        1944..=1947 => sturdy(above) || matches!(above.0, 1944..=1947),
        1948 | 1949 => sturdy(above),
        2030 | 2031 => sturdy(above) || above.is_leaves() || above == PALE_HANGING_MOSS,
        _ => return None,
    })
}

/// Whether a wall-mounted plant (vines, cocoa) still has its wall.
pub fn wall_ok(block: Block, wall: Block) -> Option<bool> {
    if let Some(_f) = vine_wall(block) {
        return Some(sturdy(wall) || wall.is_leaves() || wall.is_log());
    }
    if cocoa_of(block).is_some() {
        return Some(wall == Block::JUNGLE_LOG);
    }
    None
}

/// The wall direction of a wall-mounted plant.
pub fn wall_of(block: Block) -> Option<Facing> {
    vine_wall(block).or_else(|| cocoa_of(block).map(|(_, f)| f))
}

/// Java's fire odds `(encouragement, flammability)` for flammable members.
pub fn fire_odds(b: Block) -> Option<(u8, u8)> {
    Some(match b.base().0 {
        1942 | 1943 | 2010..=2013 => (30, 60),
        1940 | 1941 | 1944 | 1948 | 1949 | 2014..=2017 | 2029..=2054 | 2058 => (60, 100),
        1907 | 1911 | 1908 | 1912 => (5, 100),
        2018 | 2020 => (5, 5),
        2019 | 2026..=2028 => (5, 20),
        2022 | 1969 => (60, 60),
        _ => return None,
    })
}

/// Logs among these blocks.
pub fn is_log(b: Block) -> bool {
    b == PALE_OAK_LOG
}

pub fn is_planks(b: Block) -> bool {
    matches!(b.0, 2019 | 2027)
}

/// Boxes (in 1/16 block) of the shaped members: vines lie on their wall,
/// big dripleaves are a thin platform, bamboo a post, cocoa a pod on its log.
pub fn boxes(b: Block) -> Option<&'static [super::shape::Box16]> {
    use super::shape::Box16 as B;
    const fn bx(min: [u8; 3], max: [u8; 3]) -> B {
        B { min, max }
    }
    // In `Facing::ALL` order: south, north, east, west walls.
    static VINES: [[B; 1]; 4] = [
        [bx([0, 0, 15], [16, 16, 16])],
        [bx([0, 0, 0], [16, 16, 1])],
        [bx([15, 0, 0], [16, 16, 16])],
        [bx([0, 0, 0], [1, 16, 16])],
    ];
    static DRIPLEAF: [B; 1] = [bx([0, 11, 0], [16, 15, 16])];
    static BAMBOO_POST: [B; 1] = [bx([6, 0, 6], [10, 16, 10])];
    static COCOA_PODS: [[[B; 1]; 4]; 3] = {
        // Age 0 is 4x5x4, age 1 6x7x6, age 2 8x9x8, hanging under the top
        // of the cell against the log.
        const fn pod(w: u8, h: u8, wall: u8) -> B {
            let lo = 8 - w / 2;
            let (top, bottom) = (12, 12 - h);
            match wall {
                0 => bx([lo, bottom, 15 - w], [lo + w, top, 15]),
                1 => bx([lo, bottom, 1], [lo + w, top, 1 + w]),
                2 => bx([15 - w, bottom, lo], [15, top, lo + w]),
                _ => bx([1, bottom, lo], [1 + w, top, lo + w]),
            }
        }
        let mut out = [[[bx([0; 3], [0; 3])]; 4]; 3];
        let mut age = 0;
        while age < 3 {
            let mut f = 0;
            while f < 4 {
                out[age][f] = [pod(4 + age as u8 * 2, 5 + age as u8 * 2, f as u8)];
                f += 1;
            }
            age += 1;
        }
        out
    };
    static EGGS: [B; 4] =
        [bx([3, 0, 3], [8, 7, 8]), bx([9, 0, 8], [14, 7, 13]), bx([3, 0, 9], [8, 7, 14]), bx([9, 0, 2], [14, 7, 7])];
    if let Some(n) = egg_count(b) {
        return Some(&EGGS[..n as usize]);
    }
    Some(match b.0 {
        2054..=2057 => &VINES[(b.0 - 2054) as usize],
        1950 => &DRIPLEAF,
        2022..=2024 => &BAMBOO_POST,
        2062..=2073 => {
            let i = (b.0 - 2062) as usize;
            &COCOA_PODS[i / 4][i % 4]
        }
        _ => return None,
    })
}

/// Collision boxes where they differ from [`boxes`]: vines are passable.
pub fn collision(b: Block) -> Option<&'static [super::shape::Box16]> {
    match b.0 {
        2054..=2057 => Some(&[]),
        _ => boxes(b),
    }
}

/// Height (in 1/16) a low block's top sits under the cell's top.
pub fn top_drop(b: Block) -> Option<u8> {
    Some(match b.0 {
        1910 => 14,
        1911 | 1912 | 2029 | 2053 => 15,
        _ => return None,
    })
}

/// Creative palette entries.
pub fn palette_ids() -> impl Iterator<Item = u16> {
    (1900..=1914)
        .chain([1924, 1925, 1926, 1927, 1928, 1929, 1934])
        .chain([1940, 1941, 1942, 1943, 1944, 1948, 1949, 1950, 1952])
        .chain([1960, 1962, 1963, 1965, 1969, 1970, 1971, 1972, 1973, 1974, 1975, 1976])
        .chain(1980..=2009)
        .chain(2010..=2022)
        .chain([2026, 2027, 2028, 2029, 2030])
        .chain(2032..=2041)
        .chain([2043, 2045, 2047, 2049, 2051, 2053, 2054, 2058, 2062, 2074, 2075, 2076])
}

/// The block an item of this module places when used on `face` of a
/// clicked block, and its orientation (`None` if the item is not one of
/// ours or has no special placement).
pub fn placed(block: Block, normal: IVec3) -> Option<Block> {
    Some(match block.0 {
        1914 => pointed_dripstone(normal.y < 0, Thickness::Tip),
        1926..=1929 if normal.y < 0 => Block(block.0 + 4),
        2054 => {
            let wall = Facing::from_offset(-normal)?;
            vine(wall)
        }
        2062 => cocoa(0, Facing::from_offset(-normal)?),
        _ => return None,
    })
}

/// Rough share of Java's sound groups, for the sound bank.
pub fn is_coral(b: Block) -> bool {
    coral_of(b).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mining;

    #[test]
    fn ids_are_registered_and_names_round_trip() {
        for id in FIRST..=LAST {
            let reserved = matches!(id, 1935..=1939 | 1954..=1959);
            assert_eq!(registry(id).is_some(), !reserved, "{id}");
            if reserved {
                continue;
            }
            let b = Block(id);
            assert_ne!(b.kind(), RenderKind::Invisible, "{id}");
            assert!(mining(b).is_some(), "{} mines", b.name());
        }
        for id in palette_ids() {
            let b = Block(id);
            assert_eq!(b.base(), b, "palette holds default states: {}", b.name());
            assert_eq!(Block::from_name(b.name()), Some(b), "{} looks itself up", b.name());
            assert!(Block::creative_palette().any(|p| p == b), "{}", b.name());
        }
        const { assert!(TEX_END <= 1700) };
    }

    #[test]
    fn helpers_round_trip() {
        for down in [false, true] {
            for t in [Thickness::Tip, Thickness::TipMerge, Thickness::Frustum, Thickness::Middle, Thickness::Base] {
                assert_eq!(dripstone_of(pointed_dripstone(down, t)), Some((down, t)));
            }
            for size in 0..4 {
                assert_eq!(bud_of(amethyst_bud(size, down)), Some((size, down)));
            }
        }
        for n in 1..=4 {
            assert_eq!(pickle_count(sea_pickles(n)), Some(n));
            assert_eq!(egg_count(turtle_eggs(n)), Some(n));
        }
        for f in Facing::ALL {
            assert_eq!(vine_wall(vine(f)), Some(f));
            for age in 0..3 {
                assert_eq!(cocoa_of(cocoa(age, f)), Some((age, f)));
            }
        }
        for kind in 0..3 {
            for colour in 0..5 {
                for dead in [false, true] {
                    assert_eq!(coral_of(coral(kind, colour, dead)), Some((kind, colour, dead)));
                }
            }
        }
        for (i, lower) in DOUBLE_PLANTS.into_iter().enumerate() {
            assert_eq!(double_of(lower), Some((lower, false)), "{i}");
            assert_eq!(double_of(Block(lower.0 + 1)), Some((lower, true)));
            assert_eq!(Block(lower.0 + 1).base(), lower);
        }
    }

    #[test]
    fn aquatic_plants_hold_water_and_glow() {
        for b in [KELP, KELP_PLANT, SEAGRASS, TALL_SEAGRASS, sea_pickles(3), coral(1, 2, false), coral(2, 4, false)] {
            assert!(b.is_waterlogged(), "{}", b.name());
            assert!(b.holds_water(), "{}", b.name());
            assert!(!b.is_water());
        }
        assert!(!coral(1, 2, true).is_waterlogged(), "dead coral stays dry");
        assert_eq!(sea_pickles(4).emission(), 15);
        assert_eq!(sea_pickles(1).emission(), 6);
        assert_eq!(SEA_LANTERN.emission(), 15);
        assert_eq!(CAVE_VINES_LIT.emission(), 14);
        assert_eq!(CAVE_VINES.emission(), 0);
    }

    #[test]
    fn mining_and_drops_follow_java() {
        assert_eq!(PODZOL.drop(), Some(Block::DIRT.into()));
        assert_eq!(silk_drop(PODZOL), Some(PODZOL.into()));
        assert_eq!(coral(0, 3, false).drop(), Some(coral(0, 3, true).into()));
        assert!(!mining::can_harvest(DRIPSTONE_BLOCK, None));
        assert_eq!(BLUE_ICE.drop(), None);
        assert_eq!(BIG_DRIPLEAF_STEM.drop(), Some(BIG_DRIPLEAF.into()));
        assert_eq!(Block(SUNFLOWER.0 + 1).drop(), None, "upper halves drop nothing");
        assert_eq!(SUNFLOWER.drop(), Some(SUNFLOWER.into()));
        assert_eq!(MOSS_BLOCK.best_tool(), Some(ToolKind::Hoe));
        assert!(is_leaves(CHERRY_LEAVES) && CHERRY_LEAVES.is_leaves());
        assert!(PALE_OAK_LOG.is_log() && PALE_OAK_PLANKS.is_planks());
        assert!(DARK_OAK_SAPLING.is_sapling());
        assert!(SEAGRASS.is_replaceable() && SNOW_LAYER.is_replaceable() && !KELP.is_replaceable());
    }

    #[test]
    fn plants_need_their_ground() {
        assert!(AZALEA.can_stay_on(MOSS_BLOCK));
        assert!(!KELP.can_stay_on(Block::MAGMA));
        assert!(KELP.can_stay_on(Block::SAND) && KELP.can_stay_on(KELP_PLANT));
        assert!(LILY_PAD.can_stay_on(Block::WATER) && !LILY_PAD.can_stay_on(Block::DIRT));
        assert!(CORNFLOWER.can_stay_on(PODZOL));
        assert!(Block(SUNFLOWER.0 + 1).can_stay_on(SUNFLOWER));
        assert!(!Block(SUNFLOWER.0 + 1).can_stay_on(Block::GRASS));
        assert_eq!(hangs_from(SPORE_BLOSSOM, Block::STONE), Some(true));
        assert_eq!(hangs_from(CAVE_VINES, Block::AIR), Some(false));
        assert_eq!(wall_ok(cocoa(0, Facing::East), Block::JUNGLE_LOG), Some(true));
        assert_eq!(wall_ok(cocoa(0, Facing::East), Block::LOG), Some(false));
    }

    #[test]
    fn every_state_survives_a_save_and_reload() {
        use crate::world::chunk::ChunkData;
        use crate::world::storage::{LevelInfo, Storage};
        use std::sync::Arc;
        let dir = std::env::temp_dir().join(format!("voxelcraft-overworld-blocks-{}", std::process::id()));
        let storage = Storage::new(&dir);
        let mut chunk = ChunkData::Uniform(Block::STONE);
        for (i, id) in (FIRST..=LAST).enumerate() {
            chunk.set(i % 32, i / 32, 3, Block(id));
        }
        let pos = IVec3::new(2, -2, -7);
        storage
            .save(&LevelInfo { seed: 1, player: None, props: Default::default() }, &[(pos, Arc::new(chunk))])
            .unwrap();
        let loaded = storage.load_chunks().unwrap();
        for (i, id) in (FIRST..=LAST).enumerate() {
            assert_eq!(loaded[&pos].get(i % 32, i / 32, 3), Block(id));
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
