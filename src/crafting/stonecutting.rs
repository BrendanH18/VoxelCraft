//! Every stonecutter recipe as data (Java 26.3's `*_stonecutting` recipes).
//! The stonecutter's screen arrives in v0.17. The v0.12 tracks keep their
//! tables next to their crafting recipes; this module holds the families
//! that existed before them and gathers everything.

use crate::item::Item;
use crate::world::{block::Block, forms, nether_blocks, overworld_blocks as ob};

/// A stonecutter output: the block itself or a shape cut from it.
#[derive(Clone, Copy)]
enum Cut {
    Full(Block),
    Stairs(Block),
    Slab(Block),
    Wall(Block),
}

impl Cut {
    fn block(self) -> Block {
        let (base, local) = match self {
            Cut::Full(b) => return b,
            Cut::Stairs(b) => (b, 0),
            Cut::Slab(b) => (b, 4),
            Cut::Wall(b) => (b, 5),
        };
        match (base, local) {
            (Block::COBBLESTONE, 5) => Some(Block::COBBLESTONE_WALL),
            (Block::STONE_BRICKS, 5) => Some(Block::STONE_BRICK_WALL),
            (_, 0) => Block::stairs_of(base),
            (_, 4) => Block::slab_of(base),
            _ => nether_blocks::shape_of(base, 5),
        }
        .or_else(|| (0..19).find(|&i| forms::stone_base(i) == base).map(|i| forms::stone_id(i, local)))
        .unwrap_or_else(|| panic!("no shape {local} of {}", base.name()))
    }
}

use Cut::{Full, Slab, Stairs, Wall};

/// Java 26.3's stonecutting recipes for the families from before v0.12:
/// input, output and count.
const LEGACY: &[(Block, Cut, u8)] = &[
    (Block::ANDESITE, Slab(Block::ANDESITE), 2),
    (Block::ANDESITE, Stairs(Block::ANDESITE), 1),
    (Block::ANDESITE, Wall(Block::ANDESITE), 1),
    (Block::ANDESITE, Full(Block::POLISHED_ANDESITE), 1),
    (Block::ANDESITE, Slab(Block::POLISHED_ANDESITE), 2),
    (Block::ANDESITE, Stairs(Block::POLISHED_ANDESITE), 1),
    (Block::BASALT, Full(Block::POLISHED_BASALT), 1),
    (Block::BLACKSTONE, Slab(Block::BLACKSTONE), 2),
    (Block::BLACKSTONE, Stairs(Block::BLACKSTONE), 1),
    (Block::BLACKSTONE, Wall(Block::BLACKSTONE), 1),
    (Block::BLACKSTONE, Full(Block::CHISELED_POLISHED_BLACKSTONE), 1),
    (Block::BLACKSTONE, Full(Block::POLISHED_BLACKSTONE), 1),
    (Block::BLACKSTONE, Slab(Block::POLISHED_BLACKSTONE_BRICKS), 2),
    (Block::BLACKSTONE, Stairs(Block::POLISHED_BLACKSTONE_BRICKS), 1),
    (Block::BLACKSTONE, Wall(Block::POLISHED_BLACKSTONE_BRICKS), 1),
    (Block::BLACKSTONE, Full(Block::POLISHED_BLACKSTONE_BRICKS), 1),
    (Block::BLACKSTONE, Slab(Block::POLISHED_BLACKSTONE), 2),
    (Block::BLACKSTONE, Stairs(Block::POLISHED_BLACKSTONE), 1),
    (Block::BLACKSTONE, Wall(Block::POLISHED_BLACKSTONE), 1),
    (Block::BRICKS, Slab(Block::BRICKS), 2),
    (Block::BRICKS, Stairs(Block::BRICKS), 1),
    (Block::COBBLED_DEEPSLATE, Slab(Block::COBBLED_DEEPSLATE), 2),
    (Block::COBBLED_DEEPSLATE, Stairs(Block::COBBLED_DEEPSLATE), 1),
    (Block::COBBLED_DEEPSLATE, Wall(Block::COBBLED_DEEPSLATE), 1),
    (Block::COBBLED_DEEPSLATE, Full(Block::POLISHED_DEEPSLATE), 1),
    (Block::COBBLED_DEEPSLATE, Slab(Block::POLISHED_DEEPSLATE), 2),
    (Block::COBBLED_DEEPSLATE, Stairs(Block::POLISHED_DEEPSLATE), 1),
    (Block::COBBLED_DEEPSLATE, Wall(Block::POLISHED_DEEPSLATE), 1),
    (Block::COBBLESTONE, Slab(Block::COBBLESTONE), 2),
    (Block::COBBLESTONE, Stairs(Block::COBBLESTONE), 1),
    (Block::COBBLESTONE, Wall(Block::COBBLESTONE), 1),
    (ob::DARK_PRISMARINE, Slab(ob::DARK_PRISMARINE), 2),
    (ob::DARK_PRISMARINE, Stairs(ob::DARK_PRISMARINE), 1),
    (Block::DEEPSLATE, Full(Block::COBBLED_DEEPSLATE), 1),
    (Block::DEEPSLATE, Slab(Block::COBBLED_DEEPSLATE), 2),
    (Block::DEEPSLATE, Stairs(Block::COBBLED_DEEPSLATE), 1),
    (Block::DEEPSLATE, Wall(Block::COBBLED_DEEPSLATE), 1),
    (Block::DEEPSLATE, Full(Block::POLISHED_DEEPSLATE), 1),
    (Block::DEEPSLATE, Slab(Block::POLISHED_DEEPSLATE), 2),
    (Block::DEEPSLATE, Stairs(Block::POLISHED_DEEPSLATE), 1),
    (Block::DEEPSLATE, Wall(Block::POLISHED_DEEPSLATE), 1),
    (Block::DIORITE, Slab(Block::DIORITE), 2),
    (Block::DIORITE, Stairs(Block::DIORITE), 1),
    (Block::DIORITE, Wall(Block::DIORITE), 1),
    (Block::DIORITE, Full(Block::POLISHED_DIORITE), 1),
    (Block::DIORITE, Slab(Block::POLISHED_DIORITE), 2),
    (Block::DIORITE, Stairs(Block::POLISHED_DIORITE), 1),
    (Block::GRANITE, Slab(Block::GRANITE), 2),
    (Block::GRANITE, Stairs(Block::GRANITE), 1),
    (Block::GRANITE, Wall(Block::GRANITE), 1),
    (Block::GRANITE, Full(Block::POLISHED_GRANITE), 1),
    (Block::GRANITE, Slab(Block::POLISHED_GRANITE), 2),
    (Block::GRANITE, Stairs(Block::POLISHED_GRANITE), 1),
    (Block::MOSSY_COBBLESTONE, Slab(Block::MOSSY_COBBLESTONE), 2),
    (Block::MOSSY_COBBLESTONE, Stairs(Block::MOSSY_COBBLESTONE), 1),
    (Block::MOSSY_COBBLESTONE, Wall(Block::MOSSY_COBBLESTONE), 1),
    (ob::MUD_BRICKS, Slab(ob::MUD_BRICKS), 2),
    (ob::MUD_BRICKS, Stairs(ob::MUD_BRICKS), 1),
    (ob::MUD_BRICKS, Wall(ob::MUD_BRICKS), 1),
    (Block::NETHER_BRICKS, Slab(Block::NETHER_BRICKS), 2),
    (Block::NETHER_BRICKS, Stairs(Block::NETHER_BRICKS), 1),
    (Block::POLISHED_ANDESITE, Slab(Block::POLISHED_ANDESITE), 2),
    (Block::POLISHED_ANDESITE, Stairs(Block::POLISHED_ANDESITE), 1),
    (Block::POLISHED_BLACKSTONE, Full(Block::CHISELED_POLISHED_BLACKSTONE), 1),
    (Block::POLISHED_BLACKSTONE, Slab(Block::POLISHED_BLACKSTONE_BRICKS), 2),
    (Block::POLISHED_BLACKSTONE, Stairs(Block::POLISHED_BLACKSTONE_BRICKS), 1),
    (Block::POLISHED_BLACKSTONE, Wall(Block::POLISHED_BLACKSTONE_BRICKS), 1),
    (Block::POLISHED_BLACKSTONE, Full(Block::POLISHED_BLACKSTONE_BRICKS), 1),
    (Block::POLISHED_BLACKSTONE, Slab(Block::POLISHED_BLACKSTONE), 2),
    (Block::POLISHED_BLACKSTONE, Stairs(Block::POLISHED_BLACKSTONE), 1),
    (Block::POLISHED_BLACKSTONE, Wall(Block::POLISHED_BLACKSTONE), 1),
    (Block::POLISHED_BLACKSTONE_BRICKS, Slab(Block::POLISHED_BLACKSTONE_BRICKS), 2),
    (Block::POLISHED_BLACKSTONE_BRICKS, Stairs(Block::POLISHED_BLACKSTONE_BRICKS), 1),
    (Block::POLISHED_BLACKSTONE_BRICKS, Wall(Block::POLISHED_BLACKSTONE_BRICKS), 1),
    (Block::POLISHED_DEEPSLATE, Slab(Block::POLISHED_DEEPSLATE), 2),
    (Block::POLISHED_DEEPSLATE, Stairs(Block::POLISHED_DEEPSLATE), 1),
    (Block::POLISHED_DEEPSLATE, Wall(Block::POLISHED_DEEPSLATE), 1),
    (Block::POLISHED_DIORITE, Slab(Block::POLISHED_DIORITE), 2),
    (Block::POLISHED_DIORITE, Stairs(Block::POLISHED_DIORITE), 1),
    (Block::POLISHED_GRANITE, Slab(Block::POLISHED_GRANITE), 2),
    (Block::POLISHED_GRANITE, Stairs(Block::POLISHED_GRANITE), 1),
    (ob::PRISMARINE, Slab(ob::PRISMARINE), 2),
    (ob::PRISMARINE, Stairs(ob::PRISMARINE), 1),
    (ob::PRISMARINE, Wall(ob::PRISMARINE), 1),
    (ob::PRISMARINE_BRICKS, Slab(ob::PRISMARINE_BRICKS), 2),
    (ob::PRISMARINE_BRICKS, Stairs(ob::PRISMARINE_BRICKS), 1),
    (Block::SANDSTONE, Slab(Block::SANDSTONE), 2),
    (Block::SANDSTONE, Stairs(Block::SANDSTONE), 1),
    (Block::SMOOTH_STONE, Slab(Block::SMOOTH_STONE), 2),
    (Block::STONE, Full(Block::COBBLESTONE), 1),
    (Block::STONE, Slab(Block::COBBLESTONE), 2),
    (Block::STONE, Stairs(Block::COBBLESTONE), 1),
    (Block::STONE, Wall(Block::COBBLESTONE), 1),
    (Block::STONE, Slab(Block::STONE_BRICKS), 2),
    (Block::STONE, Stairs(Block::STONE_BRICKS), 1),
    (Block::STONE, Wall(Block::STONE_BRICKS), 1),
    (Block::STONE, Full(Block::STONE_BRICKS), 1),
    (Block::STONE, Slab(Block::STONE), 2),
    (Block::STONE, Stairs(Block::STONE), 1),
    (Block::STONE_BRICKS, Slab(Block::STONE_BRICKS), 2),
    (Block::STONE_BRICKS, Stairs(Block::STONE_BRICKS), 1),
    (Block::STONE_BRICKS, Wall(Block::STONE_BRICKS), 1),
    (Block::TUFF, Slab(Block::TUFF), 2),
    (Block::TUFF, Stairs(Block::TUFF), 1),
    (Block::TUFF, Wall(Block::TUFF), 1),
];

/// Every stonecutter recipe as (input, output, count).
pub fn all() -> Vec<(Item, Item, u8)> {
    let b = Item::from_block;
    let legacy = LEGACY.iter().map(|&(input, cut, n)| (b(input), b(cut.block()), n));
    let stone = super::stone::STONECUTTING.iter().map(|r| (b(r.1), b(r.3), r.4));
    legacy.chain(stone).chain(super::copper::stonecutting()).chain(super::colour::stonecutting()).collect()
}

/// What the stonecutter makes from `input`, in recipe order.
pub fn from(input: Item) -> impl Iterator<Item = (Item, u8)> {
    all().into_iter().filter(move |r| r.0 == input).map(|r| (r.1, r.2))
}
