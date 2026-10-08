//! Append-only Nether biome blocks, ids 1600..=1793.
//!
//! | Ids | Blocks |
//! |---|---|
//! | 1600..=1601 | crimson and warped nylium |
//! | 1602..=1613 | crimson, warped and their stripped stems, three axis states each (Y, X, Z) |
//! | 1614..=1617 | crimson, warped and stripped hyphae (bark on every face, so one state) |
//! | 1618..=1622 | crimson and warped planks, nether and warped wart blocks, shroomlight |
//! | 1623..=1627 | crimson and warped fungus and roots, nether sprouts |
//! | 1628..=1633 | soul soil, soul fire, soul torch, bone block (three axes) |
//! | 1634..=1687 | weeping vines (ages 0..=25, then the plant), twisting vines likewise |
//! | 1688..=1733 | crimson and warped stairs, slab, fence, gate and door (`world::forms` woods 7 and 8) |
//! | 1734..=1793 | crimson then warped trapdoor (16), button (12) and pressure plate (2) |
//!
//! The trapdoors, buttons and plates mirror the oak redstone states: their
//! `redstone_blocks::Component` is the oak twin's, so every redstone rule
//! applies unchanged, and [`keep_wood`] puts a changed state back on the
//! right wood.

use glam::IVec3;

use super::block::{Block, RenderKind, tex};
use crate::item::{Item, ToolKind};

pub const CRIMSON_NYLIUM: Block = Block(1600);
pub const WARPED_NYLIUM: Block = Block(1601);
pub const CRIMSON_STEM: Block = Block(1602);
pub const WARPED_STEM: Block = Block(1605);
pub const STRIPPED_CRIMSON_STEM: Block = Block(1608);
pub const STRIPPED_WARPED_STEM: Block = Block(1611);
pub const CRIMSON_HYPHAE: Block = Block(1614);
pub const WARPED_HYPHAE: Block = Block(1615);
pub const STRIPPED_CRIMSON_HYPHAE: Block = Block(1616);
pub const STRIPPED_WARPED_HYPHAE: Block = Block(1617);
pub const CRIMSON_PLANKS: Block = Block(1618);
pub const WARPED_PLANKS: Block = Block(1619);
pub const NETHER_WART_BLOCK: Block = Block(1620);
pub const WARPED_WART_BLOCK: Block = Block(1621);
pub const SHROOMLIGHT: Block = Block(1622);
pub const CRIMSON_FUNGUS: Block = Block(1623);
pub const WARPED_FUNGUS: Block = Block(1624);
pub const CRIMSON_ROOTS: Block = Block(1625);
pub const WARPED_ROOTS: Block = Block(1626);
pub const NETHER_SPROUTS: Block = Block(1627);
pub const SOUL_SOIL: Block = Block(1628);
pub const SOUL_FIRE: Block = Block(1629);
pub const SOUL_TORCH: Block = Block(1630);
pub const BONE_BLOCK: Block = Block(1631);
/// Age 0; ages 0..=25 follow, then [`WEEPING_VINES_PLANT`].
pub const WEEPING_VINES: Block = Block(1634);
pub const WEEPING_VINES_PLANT: Block = Block(1660);
pub const TWISTING_VINES: Block = Block(1661);
pub const TWISTING_VINES_PLANT: Block = Block(1687);
/// Java's `GrowingPlantHeadBlock.MAX_AGE`.
pub const VINE_MAX_AGE: u8 = 25;
/// First id of the crimson and warped wood shapes (`world::forms`).
pub const WOOD_ORIGIN: u16 = 1688;
const SWITCH_ORIGIN: u16 = 1734;
const SWITCH_STRIDE: u16 = 30;
pub const LAST: u16 = 1793;

/// The two Nether woods, their nylium and their plants.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum NetherWood {
    Crimson,
    Warped,
}

impl NetherWood {
    pub const ALL: [NetherWood; 2] = [NetherWood::Crimson, NetherWood::Warped];

    const fn i(self) -> u16 {
        self as u16
    }

    pub const fn nylium(self) -> Block {
        Block(1600 + self.i())
    }
    pub const fn stem(self) -> Block {
        Block(1602 + self.i() * 3)
    }
    pub const fn stripped_stem(self) -> Block {
        Block(1608 + self.i() * 3)
    }
    pub const fn hyphae(self) -> Block {
        Block(1614 + self.i())
    }
    pub const fn stripped_hyphae(self) -> Block {
        Block(1616 + self.i())
    }
    pub const fn planks(self) -> Block {
        Block(1618 + self.i())
    }
    /// The wart block on this wood's huge fungi.
    pub const fn wart(self) -> Block {
        Block(1620 + self.i())
    }
    pub const fn fungus(self) -> Block {
        Block(1623 + self.i())
    }
    pub const fn roots(self) -> Block {
        Block(1625 + self.i())
    }
    /// `world::forms` wood index of this wood's stairs, slab, fence, gate and door.
    pub const fn form_index(self) -> u16 {
        7 + self.i()
    }
    pub const fn trapdoor(self) -> Block {
        Block(SWITCH_ORIGIN + self.i() * SWITCH_STRIDE)
    }
    pub const fn button(self) -> Block {
        Block(SWITCH_ORIGIN + self.i() * SWITCH_STRIDE + 16)
    }
    pub const fn pressure_plate(self) -> Block {
        Block(SWITCH_ORIGIN + self.i() * SWITCH_STRIDE + 28)
    }

    /// The wood a nylium, stem, hyphae, planks, wart block, fungus or roots belongs to.
    pub fn of(b: Block) -> Option<NetherWood> {
        Self::ALL.into_iter().find(|w| {
            [w.nylium(), w.stem().base(), w.stripped_stem().base(), w.hyphae(), w.stripped_hyphae(), w.planks()]
                .contains(&b.base())
                || [w.wart(), w.fungus(), w.roots()].contains(&b)
        })
    }
}

/// Weeping vines hang down from ceilings; twisting vines climb up from floors.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Vine {
    Weeping,
    Twisting,
}

impl Vine {
    pub const ALL: [Vine; 2] = [Vine::Weeping, Vine::Twisting];

    pub const fn head(self, age: u8) -> Block {
        let age = if age > VINE_MAX_AGE { VINE_MAX_AGE } else { age };
        Block(
            match self {
                Vine::Weeping => WEEPING_VINES.0,
                Vine::Twisting => TWISTING_VINES.0,
            } + age as u16,
        )
    }

    pub const fn plant(self) -> Block {
        match self {
            Vine::Weeping => WEEPING_VINES_PLANT,
            Vine::Twisting => TWISTING_VINES_PLANT,
        }
    }

    /// The direction the vine grows in.
    pub const fn grows(self) -> IVec3 {
        match self {
            Vine::Weeping => IVec3::NEG_Y,
            Vine::Twisting => IVec3::Y,
        }
    }

    /// The vine kind and the head's age (`None` for the plant part).
    pub const fn of(b: Block) -> Option<(Vine, Option<u8>)> {
        match b.0 {
            1634..=1659 => Some((Vine::Weeping, Some((b.0 - 1634) as u8))),
            1660 => Some((Vine::Weeping, None)),
            1661..=1686 => Some((Vine::Twisting, Some((b.0 - 1661) as u8))),
            1687 => Some((Vine::Twisting, None)),
            _ => None,
        }
    }
}

/// Stems and bone blocks: Y, X and Z axis states.
const fn axial(id: u16) -> Option<u16> {
    match id {
        1602..=1613 => Some(1602 + (id - 1602) / 3 * 3),
        1631..=1633 => Some(1631),
        _ => None,
    }
}

/// Textures for an axis state: `end` on the two faces the axis points out of.
const fn pillar(id: u16, side: u16, end: u16) -> [u16; 6] {
    match (id
        - match axial(id) {
            Some(b) => b,
            None => id,
        })
        % 3
    {
        1 => [end, end, side, side, side, side],
        2 => [side, side, side, side, end, end],
        _ => [side, side, end, end, side, side],
    }
}

/// Name, render kind and face textures of ids 1600..=1687.
pub const fn registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    use RenderKind::*;
    const fn all(t: u16) -> [u16; 6] {
        [t; 6]
    }
    const fn column(side: u16, top: u16, bottom: u16) -> [u16; 6] {
        [side, side, top, bottom, side, side]
    }
    Some(match id {
        1600 => ("crimson nylium", Opaque, column(tex::CRIMSON_NYLIUM_SIDE, tex::CRIMSON_NYLIUM_TOP, tex::NETHERRACK)),
        1601 => ("warped nylium", Opaque, column(tex::WARPED_NYLIUM_SIDE, tex::WARPED_NYLIUM_TOP, tex::NETHERRACK)),
        1602..=1613 => {
            const NAMES: [&str; 4] = ["crimson stem", "warped stem", "stripped crimson stem", "stripped warped stem"];
            let kind = (id - 1602) / 3;
            (NAMES[kind as usize], Opaque, pillar(id, tex::NETHER_STEM_SIDE + kind, tex::NETHER_STEM_TOP + kind))
        }
        1614..=1617 => {
            const NAMES: [&str; 4] =
                ["crimson hyphae", "warped hyphae", "stripped crimson hyphae", "stripped warped hyphae"];
            let kind = id - 1614;
            (NAMES[kind as usize], Opaque, all(tex::NETHER_STEM_SIDE + kind))
        }
        1618 => ("crimson planks", Opaque, all(tex::CRIMSON_PLANKS)),
        1619 => ("warped planks", Opaque, all(tex::WARPED_PLANKS)),
        1620 => ("nether wart block", Opaque, all(tex::NETHER_WART_BLOCK)),
        1621 => ("warped wart block", Opaque, all(tex::WARPED_WART_BLOCK)),
        1622 => ("shroomlight", Opaque, all(tex::SHROOMLIGHT)),
        1623 => ("crimson fungus", Cross, all(tex::CRIMSON_FUNGUS)),
        1624 => ("warped fungus", Cross, all(tex::WARPED_FUNGUS)),
        1625 => ("crimson roots", Cross, all(tex::CRIMSON_ROOTS)),
        1626 => ("warped roots", Cross, all(tex::WARPED_ROOTS)),
        1627 => ("nether sprouts", Cross, all(tex::NETHER_SPROUTS)),
        1628 => ("soul soil", Opaque, all(tex::SOUL_SOIL)),
        // Animated on the GPU like fire (see `chunk.wgsl`).
        1629 => ("soul fire", Cross, all(tex::SOUL_FIRE_0)),
        1630 => ("soul torch", Cross, all(tex::SOUL_TORCH)),
        1631..=1633 => ("bone block", Opaque, pillar(id, tex::BONE_BLOCK_SIDE, tex::BONE_BLOCK_TOP)),
        1634..=1659 => ("weeping vines", Cross, all(tex::NETHER_VINES)),
        1660 => ("weeping vines plant", Cross, all(tex::NETHER_VINES + 1)),
        1661..=1686 => ("twisting vines", Cross, all(tex::NETHER_VINES + 2)),
        1687 => ("twisting vines plant", Cross, all(tex::NETHER_VINES + 3)),
        _ => return None,
    })
}

/// The block without its axis or vine age (`None` if it has neither).
pub const fn base(id: u16) -> Option<Block> {
    if let Some(b) = axial(id) {
        return Some(Block(b));
    }
    match id {
        1634..=1659 => Some(WEEPING_VINES),
        1661..=1686 => Some(TWISTING_VINES),
        _ => None,
    }
}

/// Stems and bone blocks take the axis of the clicked face, like logs in Java.
pub fn axis_placed(block: Block, normal: IVec3) -> Option<Block> {
    let base = axial(block.0)?;
    Some(Block(
        base + if normal.x != 0 {
            1
        } else if normal.z != 0 {
            2
        } else {
            0
        },
    ))
}

/// Java places a vine head with a random age below the maximum.
pub fn placed_vine(block: Block, roll: u64) -> Block {
    match Vine::of(block) {
        Some((vine, Some(_))) => vine.head((roll % VINE_MAX_AGE as u64) as u8),
        _ => block,
    }
}

/// Light emission (Java: shroomlight 15, soul fire and soul torch 10).
pub const fn emission(id: u16) -> u8 {
    match id {
        1622 => 15,
        1629 | 1630 => 10,
        _ => 0,
    }
}

/// Any part of a crimson or warped tree, or something made from its wood.
pub fn is_nether_wood(b: Block) -> bool {
    matches!(b.base().0, 1602..=1619)
        || super::forms::wood_form(b.0).is_some_and(|f| wood_form_index(f) >= 7)
        || switch_oak(b.0).is_some()
}

const fn wood_form_index(form: super::forms::WoodForm) -> u16 {
    use super::forms::WoodForm::*;
    match form {
        Stairs { index, .. } | Slab { index } | Fence { index } | Gate { index, .. } | Door { index, .. } => index,
    }
}

/// Mining properties of the plain blocks (`material` is the block a slab,
/// stair, fence, gate or door mines like, so wood shapes resolve to planks).
/// `(hardness, best tool, pickaxe harvest level)`, as in Java's block
/// properties and `mineable/*` tags.
pub fn mining(material: Block) -> Option<(f32, Option<ToolKind>, Option<u8>)> {
    Some(match material.base().0 {
        1600 | 1601 => (0.4, Some(ToolKind::Pickaxe), Some(0)),
        1602..=1619 => (2.0, Some(ToolKind::Axe), None),
        1620..=1622 => (1.0, Some(ToolKind::Hoe), None),
        1623..=1627 | 1629 | 1630 | 1634..=1687 => (0.0, None, None),
        1628 => (0.5, Some(ToolKind::Shovel), None),
        1631 => (2.0, Some(ToolKind::Pickaxe), Some(0)),
        _ => return None,
    })
}

/// What breaking the block drops (`Some(None)`: nothing, or only by chance
/// in `World::spill_mined`); `None` for blocks this module doesn't own.
pub fn drop(b: Block) -> Option<Option<Item>> {
    Some(match b.base() {
        // Nylium needs silk touch to keep its moss.
        CRIMSON_NYLIUM | WARPED_NYLIUM => Some(Block::NETHERRACK.into()),
        // Sprouts need shears; vines drop by chance (see `vine_drop_chance`).
        NETHER_SPROUTS | SOUL_FIRE => None,
        v if Vine::of(v).is_some() => None,
        b if (1600..=1633).contains(&b.0) => Some(b.into()),
        _ => return None,
    })
}

/// Java's weeping/twisting vines loot: the head item 33% of the time, more
/// with fortune (`[0.33, 0.55, 0.77, 1.0]`); shears and silk touch always
/// drop it (see `sheared_drop`).
pub fn vine_drop_chance(b: Block, fortune: u8) -> Option<(Item, f32)> {
    let (vine, _) = Vine::of(b)?;
    Some((vine.head(0).into(), [0.33, 0.55, 0.77, 1.0][fortune.min(3) as usize]))
}

/// Blocks that drop themselves when cut with shears (or silk touch).
pub fn sheared_drop(b: Block) -> Option<Item> {
    match Vine::of(b) {
        Some((vine, _)) => Some(vine.head(0).into()),
        None => (b == NETHER_SPROUTS).then(|| NETHER_SPROUTS.into()),
    }
}

/// Silk touch keeps nylium (and anything shears would keep).
pub fn silk_drop(b: Block) -> Option<Item> {
    match b {
        CRIMSON_NYLIUM | WARPED_NYLIUM => Some(b.into()),
        _ => sheared_drop(b),
    }
}

/// Java's `BlockTags.NYLIUM`.
pub fn is_nylium(b: Block) -> bool {
    b == CRIMSON_NYLIUM || b == WARPED_NYLIUM
}

/// Java's `SOUL_FIRE_BASE_BLOCKS` (soul fire stays lit only on these).
pub fn soul_fire_base(b: Block) -> bool {
    b == Block::SOUL_SAND || b == SOUL_SOIL
}

/// Whether `block` can rest on `below` (`None`: not a block of this module).
pub fn can_stay_on(block: Block, below: Block) -> Option<bool> {
    let bush = || matches!(below, Block::GRASS | Block::DIRT | Block::SNOWY_GRASS) || below.is_farmland();
    Some(match block {
        // FungusBlock / RootsBlock / NetherSproutsBlock `mayPlaceOn`.
        CRIMSON_FUNGUS | WARPED_FUNGUS | CRIMSON_ROOTS | WARPED_ROOTS | NETHER_SPROUTS => {
            is_nylium(below) || below == SOUL_SOIL || bush()
        }
        SOUL_FIRE => soul_fire_base(below),
        SOUL_TORCH => below.is_opaque(),
        b => match Vine::of(b) {
            // Twisting vines stand on a sturdy top or more of themselves.
            Some((Vine::Twisting, _)) => sturdy_face(below) || matches!(Vine::of(below), Some((Vine::Twisting, _))),
            // Weeping vines hang from above (see `hangs_from`).
            Some((Vine::Weeping, _)) => true,
            None => return None,
        },
    })
}

// Match the full-block face support used by mounted redstone components.
// Transparent glass is sturdy too; partial shapes need per-face support.
fn sturdy_face(b: Block) -> bool {
    b.is_opaque() || b == Block::GLASS || b.stained_glass_color().is_some()
}

/// Whether weeping vines can hang under `above`: a sturdy bottom face or
/// more weeping vines.
pub fn hangs_from(above: Block) -> bool {
    sturdy_face(above) || matches!(Vine::of(above), Some((Vine::Weeping, _)))
}

/// Roots and sprouts are replaceable, like grass.
pub fn replaceable(b: Block) -> bool {
    matches!(b, CRIMSON_ROOTS | WARPED_ROOTS | NETHER_SPROUTS)
}

/// Java's `BlockTags.CLIMBABLE` members from this module.
pub fn climbable(b: Block) -> bool {
    Vine::of(b).is_some()
}

/// What an axe strips a stem or hyphae into (Java's `AxeItem.STRIPPABLES`),
/// keeping its axis.
pub fn stripped(b: Block) -> Option<Block> {
    match b.0 {
        1602..=1607 => Some(Block(b.0 + 6)),
        1614 | 1615 => Some(Block(b.0 + 2)),
        _ => None,
    }
}

/// The oak redstone state an id mirrors, and its wood (0 crimson, 1 warped).
pub const fn switch_oak(id: u16) -> Option<(Block, u16)> {
    if id < SWITCH_ORIGIN || id > LAST {
        return None;
    }
    let (wood, local) = ((id - SWITCH_ORIGIN) / SWITCH_STRIDE, (id - SWITCH_ORIGIN) % SWITCH_STRIDE);
    Some((
        Block(match local {
            0..=15 => 1437 + local,
            16..=27 => 1140 + local - 16,
            _ => 1215 + local - 28,
        }),
        wood,
    ))
}

/// The `wood` (0 crimson, 1 warped) twin of an oak trapdoor, button or
/// plate state; anything else is returned unchanged.
pub const fn switch_reskin(oak: Block, wood: u16) -> Block {
    let base = SWITCH_ORIGIN + wood * SWITCH_STRIDE;
    match oak.0 {
        1437..=1452 => Block(base + oak.0 - 1437),
        1140..=1151 => Block(base + 16 + oak.0 - 1140),
        1215..=1216 => Block(base + 28 + oak.0 - 1215),
        _ => oak,
    }
}

/// `to` (an oak-twin state computed by redstone rules) on the wood of `from`.
pub const fn keep_wood(from: Block, to: Block) -> Block {
    match switch_oak(from.0) {
        Some((_, wood)) => switch_reskin(to, wood),
        None => to,
    }
}

/// Names and textures of the crimson and warped trapdoors, buttons and plates.
pub const fn switch_registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    let Some((_, wood)) = switch_oak(id) else { return None };
    let local = (id - SWITCH_ORIGIN) % SWITCH_STRIDE;
    let planks = if wood == 0 { tex::CRIMSON_PLANKS } else { tex::WARPED_PLANKS };
    let (names, layer) = match local {
        0..=15 => (["crimson trapdoor", "warped trapdoor"], tex::NETHER_TRAPDOORS + wood),
        16..=27 => (["crimson button", "warped button"], planks),
        _ => (["crimson pressure plate", "warped pressure plate"], planks),
    };
    Some((names[wood as usize], RenderKind::Shaped, [layer; 6]))
}

/// Creative palette entries.
pub fn palette_ids() -> impl Iterator<Item = u16> {
    [1600, 1601, 1602, 1605, 1608, 1611, 1614, 1615, 1616, 1617, 1618, 1619, 1620, 1621, 1622]
        .into_iter()
        .chain([1623, 1624, 1625, 1626, 1627, 1634, 1661, 1628, 1630, 1631])
        .chain(NetherWood::ALL.into_iter().flat_map(|w| {
            let i = w.form_index();
            [0, 4, 5, 6, 14].map(|local| super::forms::wood_id(i, local).0)
        }))
        .chain(NetherWood::ALL.into_iter().flat_map(|w| [w.trapdoor().0, w.button().0, w.pressure_plate().0]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Tier;
    use crate::mining;
    use crate::world::block::Facing;
    use crate::world::redstone_blocks::{self as r, Component};

    #[test]
    fn ids_stay_in_the_reserved_range_and_names_round_trip() {
        for id in 1600..=LAST {
            let b = Block(id);
            assert_ne!(b.kind(), RenderKind::Invisible, "{id} is registered");
            assert_ne!(b.name(), "unknown");
        }
        assert_eq!(Block(LAST + 1).kind(), RenderKind::Invisible, "1794..=1799 stay free");
        for id in palette_ids() {
            let b = Block(id);
            assert_eq!(Block::from_name(b.name()), Some(b), "{} names its default state", b.name());
            assert_eq!(b.base(), b, "palette holds default states: {}", b.name());
            assert!(Block::creative_palette().any(|p| p == b));
        }
        assert_eq!(Block::from_name("crimson_planks"), Some(CRIMSON_PLANKS));
        assert_eq!(Block::from_name("warped_door"), Some(crate::world::forms::wood_id(8, 14)));
        assert_eq!(Item::from_name("weeping_vines"), Some(Item::from_block(WEEPING_VINES)));
    }

    #[test]
    fn woods_follow_java_mining_light_and_fire_rules() {
        for w in NetherWood::ALL {
            let wood = [w.stem(), w.stripped_stem(), w.hyphae(), w.stripped_hyphae(), w.planks()];
            let shapes = [0, 4, 5, 6, 14].map(|l| crate::world::forms::wood_id(w.form_index(), l));
            for b in wood.into_iter().chain(shapes) {
                assert_eq!(b.best_tool(), Some(ToolKind::Axe), "{}", b.name());
                assert!(mining::can_harvest(b, None), "{}", b.name());
                assert_eq!(b.fire_odds(), (0, 0), "nether wood never burns: {}", b.name());
                assert!(!b.ignited_by_lava(), "{}", b.name());
                assert!(is_nether_wood(b), "{}", b.name());
                assert_eq!(NetherWood::of(b).is_some(), wood.contains(&b));
            }
            assert_eq!(w.planks().hardness(), 2.0);
            assert_eq!(shapes[4].hardness(), 3.0, "doors are 3.0");
            assert_eq!(w.trapdoor().hardness(), 3.0);
            assert_eq!(w.button().hardness(), 0.5);
            assert_eq!(w.pressure_plate().hardness(), 0.5);
            assert_eq!(w.nylium().drop(), Some(Block::NETHERRACK.into()));
            assert_eq!(silk_drop(w.nylium()), Some(w.nylium().into()));
            assert!(!mining::can_harvest(w.nylium(), None), "nylium needs a pickaxe");
            assert!(mining::can_harvest(w.nylium(), Some(Item::tool(ToolKind::Pickaxe, Tier::Wood))));
            assert_eq!(w.wart().best_tool(), Some(ToolKind::Hoe));
            assert_eq!(w.fungus().hardness(), 0.0);
            assert_eq!(NetherWood::of(w.fungus()), Some(w));
        }
        assert_eq!(SHROOMLIGHT.emission(), 15);
        assert_eq!(SOUL_FIRE.emission(), 10);
        assert_eq!(SOUL_TORCH.emission(), 10);
        assert!(SOUL_FIRE.is_fire() && SOUL_FIRE.fire_age().is_none() && SOUL_FIRE.drop().is_none());
        assert_eq!(SOUL_SOIL.best_tool(), Some(ToolKind::Shovel));
        assert!(!mining::can_harvest(BONE_BLOCK, None));
        assert_eq!(BONE_BLOCK.hardness(), 2.0);
        assert!(Block::LOG.fire_odds().0 > 0, "overworld wood still burns");
    }

    #[test]
    fn stems_and_bone_blocks_keep_their_axis() {
        for base in [CRIMSON_STEM, WARPED_STEM, STRIPPED_CRIMSON_STEM, STRIPPED_WARPED_STEM, BONE_BLOCK] {
            for (n, offset) in [(IVec3::Y, 0), (IVec3::X, 1), (IVec3::NEG_Z, 2)] {
                let b = axis_placed(base, n).unwrap();
                assert_eq!(b.0, base.0 + offset);
                assert_eq!((b.base(), b.name(), b.drop()), (base, base.name(), Some(base.into())));
                let side = base.info().tex[0];
                let ends: Vec<usize> = (0..6).filter(|&f| b.info().tex[f] != side).collect();
                assert_eq!(ends, [[2, 3], [0, 1], [4, 5]][offset as usize]);
            }
        }
        assert_eq!(stripped(Block(CRIMSON_STEM.0 + 2)), Some(Block(STRIPPED_CRIMSON_STEM.0 + 2)));
        assert_eq!(stripped(WARPED_HYPHAE), Some(STRIPPED_WARPED_HYPHAE));
        assert_eq!(stripped(STRIPPED_WARPED_STEM), None);
    }

    #[test]
    fn plants_need_nylium_soul_soil_or_dirt_and_vines_their_support() {
        for plant in [CRIMSON_FUNGUS, WARPED_FUNGUS, CRIMSON_ROOTS, WARPED_ROOTS, NETHER_SPROUTS] {
            for ok in [CRIMSON_NYLIUM, WARPED_NYLIUM, SOUL_SOIL, Block::DIRT, Block::GRASS] {
                assert!(plant.can_stay_on(ok), "{} on {}", plant.name(), ok.name());
            }
            for bad in [Block::NETHERRACK, Block::SOUL_SAND, Block::STONE, Block::AIR] {
                assert!(!plant.can_stay_on(bad), "{} on {}", plant.name(), bad.name());
            }
            assert_eq!(plant.kind(), RenderKind::Cross);
        }
        assert!(CRIMSON_ROOTS.is_replaceable() && NETHER_SPROUTS.is_replaceable() && !CRIMSON_FUNGUS.is_replaceable());
        assert!(SOUL_FIRE.can_stay_on(SOUL_SOIL) && SOUL_FIRE.can_stay_on(Block::SOUL_SAND));
        assert!(!SOUL_FIRE.can_stay_on(Block::NETHERRACK));
        assert!(TWISTING_VINES.can_stay_on(Block::NETHERRACK) && TWISTING_VINES.can_stay_on(TWISTING_VINES_PLANT));
        assert!(!TWISTING_VINES.can_stay_on(Block::AIR));
        assert!(hangs_from(Block::NETHERRACK) && hangs_from(WEEPING_VINES_PLANT) && !hangs_from(Block::AIR));
        for v in Vine::ALL {
            assert_eq!(Vine::of(v.head(7)), Some((v, Some(7))));
            assert_eq!(Vine::of(v.plant()), Some((v, None)));
            assert_eq!(v.head(200), v.head(VINE_MAX_AGE));
            assert_eq!(v.head(9).base(), v.head(0));
            assert!(climbable(v.head(3)) && climbable(v.plant()));
            assert_eq!(v.head(3).drop(), None);
            assert_eq!(sheared_drop(v.plant()), Some(v.head(0).into()));
            assert_eq!(vine_drop_chance(v.plant(), 9).map(|(_, p)| p), Some(1.0));
        }
        assert_eq!(sheared_drop(NETHER_SPROUTS), Some(NETHER_SPROUTS.into()));
        assert_eq!(NETHER_SPROUTS.drop(), None);
        assert_eq!(CRIMSON_ROOTS.drop(), Some(CRIMSON_ROOTS.into()));
    }

    #[test]
    fn trapdoors_buttons_and_plates_behave_like_oak_on_their_own_wood() {
        for (i, w) in NetherWood::ALL.into_iter().enumerate() {
            for oak in (1437..=1452).chain(1140..=1151).chain(1215..=1216).map(Block) {
                let twin = switch_reskin(oak, i as u16);
                assert_eq!(switch_oak(twin.0), Some((oak, i as u16)));
                assert_eq!(r::component(twin), r::component(oak));
                assert_eq!(twin.hardness(), oak.hardness());
                assert_eq!(twin.best_tool(), oak.best_tool());
                assert_eq!(twin.drop(), Some(keep_wood(twin, r::base(oak).unwrap()).into()));
                assert!(twin.name().starts_with(["crimson", "warped"][i]), "{}", twin.name());
            }
            let placed = r::placed(w.button(), IVec3::X, Facing::North);
            assert_eq!(switch_oak(placed.0).map(|(_, wood)| wood), Some(i as u16));
            assert!(matches!(r::component(placed), Some(Component::Button { mount: 3, wood: true, .. })));
            let trapdoor = r::placed(w.trapdoor(), IVec3::NEG_Y, Facing::East);
            assert_eq!(r::base(trapdoor), Some(w.trapdoor()));
            assert_eq!(keep_wood(w.pressure_plate(), r::plate(1, 15)).0, w.pressure_plate().0 + 1);
        }
        assert_eq!(keep_wood(r::WOOD_BUTTON, r::button(0, true, true)), r::button(0, true, true));
    }

    #[test]
    fn every_state_survives_a_save_and_reload() {
        use crate::world::chunk::ChunkData;
        use crate::world::storage::{LevelInfo, Storage};
        use std::sync::Arc;
        let dir = std::env::temp_dir().join(format!("voxelcraft-nether-biome-blocks-{}", std::process::id()));
        let storage = Storage::new(&dir);
        let mut chunk = ChunkData::Uniform(Block::NETHERRACK);
        for (i, id) in (1600..=LAST).enumerate() {
            chunk.set(i % 32, i / 32, 7, Block(id));
        }
        let pos = IVec3::new(-3, 2, 9);
        let level = LevelInfo { seed: 7, player: None, props: Default::default() };
        storage.save(&level, &[(pos, Arc::new(chunk))]).unwrap();
        let loaded = storage.load_chunks().unwrap();
        for (i, id) in (1600..=LAST).enumerate() {
            assert_eq!(loaded[&pos].get(i % 32, i / 32, 7), Block(id));
        }
        // Block items keep their ids in inventories too.
        for id in palette_ids() {
            assert_eq!(Item::from_block(Block(id)).block(), Some(Block(id)));
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn nether_doors_and_gates_open_turn_and_drop_like_other_woods() {
        use crate::world::block::Shaped;
        use crate::world::forms::wood_id;
        for w in NetherWood::ALL {
            let i = w.form_index();
            let gate = wood_id(i, 6);
            let east = gate.with_facing(Facing::East);
            assert_eq!(east.shaped(), Some(Shaped::Gate { facing: Facing::East, open: false }));
            let open = east.toggled(Facing::West);
            assert_eq!(open.shaped(), Some(Shaped::Gate { facing: Facing::West, open: true }));
            assert_eq!((open.drop(), open.base()), (Some(gate.into()), gate));
            let door = wood_id(i, 14).with_facing(Facing::North);
            let opened = door.toggled(Facing::North);
            assert_eq!(opened.shaped(), Some(Shaped::Door { facing: Facing::North, open: true, upper: false }));
            assert_eq!(opened.drop(), Some(wood_id(i, 14).into()));
            assert_eq!(wood_id(i, 22).drop(), None, "only the lower half drops");
            assert_eq!(wood_id(i, 4).slab_base(), Some(w.planks()));
            assert_eq!(wood_id(i, 2).stairs_base(), Some(w.planks()));
            assert!(wood_id(i, 4).borrows_light() && wood_id(i, 0).borrows_light());
            assert_eq!(wood_id(i, 5).shaped(), Some(Shaped::Fence));
            assert!(!crate::world::shape::item_shape(wood_id(i, 14)).is_empty());
        }
        // chunk.wgsl animates soul fire from this layer.
        assert_eq!(tex::SOUL_FIRE_0, 1300);
    }
}
