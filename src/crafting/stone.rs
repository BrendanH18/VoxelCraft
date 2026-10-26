//! v0.12 stone and minerals recipes (BLD-01b), transcribed from Java 26.3's
//! `data/minecraft/recipe` (client-26.3.jar): crafting, the retired forms'
//! removal and the stonecutter recipes whose result is one of this track's
//! blocks. The stonecutter screen arrives in v0.17; its recipes are data here.
//! Smelting lives in [`crate::world::stone_blocks::smelt`].

use super::{Ingredient, Recipe, b, shaped, shapeless};
use crate::item::Item;
use crate::world::{block::Block, forms, slab_stair_states as ss, stone_blocks as s};

const STAIRS: &[&str] = &["#  ", "## ", "###"];
const SLAB: &[&str] = &["###"];
const WALL: &[&str] = &["###", "###"];
const SQUARE: &[&str] = &["##", "##"];
const COLUMN: &[&str] = &["#", "#"];
const NINE: &[&str] = &["###", "###", "###"];

const QUARTZ_KINDS: Ingredient = &[b(s::CHISELED_QUARTZ_BLOCK), b(s::QUARTZ_BLOCK), b(s::QUARTZ_PILLAR)];
const SANDSTONE_SLAB_INPUTS: Ingredient = &[b(Block::SANDSTONE), b(s::CHISELED_SANDSTONE)];
const SANDSTONE_STAIRS_INPUTS: Ingredient = &[b(Block::SANDSTONE), b(s::CHISELED_SANDSTONE), b(s::CUT_SANDSTONE)];
const RED_SANDSTONE_SLAB_INPUTS: Ingredient = &[b(s::RED_SANDSTONE), b(s::CHISELED_RED_SANDSTONE)];
const RED_SANDSTONE_STAIRS_INPUTS: Ingredient =
    &[b(s::RED_SANDSTONE), b(s::CHISELED_RED_SANDSTONE), b(s::CUT_RED_SANDSTONE)];

/// The block each family's stairs and slab are crafted from, where Java
/// accepts more than the family's own block.
const fn shape_inputs(family: u16) -> (Ingredient, Ingredient) {
    match family {
        4 => (QUARTZ_KINDS, QUARTZ_KINDS),
        6 => (RED_SANDSTONE_STAIRS_INPUTS, RED_SANDSTONE_SLAB_INPUTS),
        _ => (&[], &[]),
    }
}

/// Appends this track's crafting recipes, drops the retired forms' recipes
/// and gives the sandstone stairs and slab Java's alternative inputs.
pub(super) fn add_recipes(r: &mut Vec<Recipe>) {
    let sandstone_slab = Item::from_block(Block::slab_of(Block::SANDSTONE).unwrap());
    let sandstone_stairs = Item::from_block(Block::stairs_of(Block::SANDSTONE).unwrap());
    r.retain(|x| {
        let block = x.result.item.block();
        !block.is_some_and(s::is_retired) && x.result.item != sandstone_slab && x.result.item != sandstone_stairs
    });
    r.push(shaped(SLAB, &[('#', SANDSTONE_SLAB_INPUTS)], sandstone_slab, 6));
    r.push(shaped(STAIRS, &[('#', SANDSTONE_STAIRS_INPUTS)], sandstone_stairs, 4));

    let one = |block: Block| -> Ingredient { super::ingredient(b(block)) };
    let square = |from: Block, to: Block, n: u8| shaped(SQUARE, &[('#', one(from))], b(to), n);
    let column = |from: Block, to: Block, n: u8| shaped(COLUMN, &[('#', one(from))], b(to), n);
    r.push(square(Block::TUFF, s::POLISHED_TUFF, 4));
    r.push(square(s::POLISHED_TUFF, s::TUFF_BRICKS, 4));
    r.push(column(forms::stone_id(7, 4), s::CHISELED_TUFF, 1));
    r.push(column(ss::FAMILIES[ss::LEGACY_COUNT + 1].slab, s::CHISELED_TUFF_BRICKS, 1));
    r.push(square(Block::POLISHED_DEEPSLATE, s::DEEPSLATE_BRICKS, 4));
    r.push(square(s::DEEPSLATE_BRICKS, s::DEEPSLATE_TILES, 4));
    r.push(column(forms::stone_id(11, 4), s::CHISELED_DEEPSLATE, 1));
    r.push(shaped(SQUARE, &[('#', &[Item::NETHER_QUARTZ])], b(s::QUARTZ_BLOCK), 1));
    r.push(square(s::QUARTZ_BLOCK, s::QUARTZ_BRICKS, 4));
    r.push(column(s::QUARTZ_BLOCK, s::QUARTZ_PILLAR, 2));
    r.push(column(s::slab_at(4, 0), s::CHISELED_QUARTZ_BLOCK, 1));
    r.push(square(Block::RED_SAND, s::RED_SANDSTONE, 1));
    r.push(square(s::RED_SANDSTONE, s::CUT_RED_SANDSTONE, 4));
    r.push(column(s::slab_at(6, 0), s::CHISELED_RED_SANDSTONE, 1));
    r.push(square(Block::SANDSTONE, s::CUT_SANDSTONE, 4));
    r.push(column(Block::slab_of(Block::SANDSTONE).unwrap(), s::CHISELED_SANDSTONE, 1));
    r.push(shaped(
        &["NW", "WN"],
        &[('N', &[Item::NETHER_BRICK]), ('W', &[Item::NETHER_WART])],
        b(s::RED_NETHER_BRICKS),
        1,
    ));
    r.push(column(Block::slab_of(Block::NETHER_BRICKS).unwrap(), s::CHISELED_NETHER_BRICKS, 1));
    r.push(square(Block::END_STONE, s::END_STONE_BRICKS, 4));
    r.push(column(Block::STONE_BRICK_SLAB, s::CHISELED_STONE_BRICKS, 1));
    r.push(shaped(NINE, &[('#', &[Item::COAL])], b(s::COAL_BLOCK), 1));
    r.push(shapeless(&[one(s::COAL_BLOCK)], Item::COAL, 9));
    r.push(shaped(NINE, &[('#', &[Item::DIAMOND])], b(s::DIAMOND_BLOCK), 1));
    r.push(shapeless(&[one(s::DIAMOND_BLOCK)], Item::DIAMOND, 9));
    r.push(shaped(
        &[" S ", "SGS", " S "],
        &[('S', &[Item::AMETHYST_SHARD]), ('G', const { &[b(Block::GLASS)] })],
        b(s::TINTED_GLASS),
        2,
    ));
    r.push(shaped(NINE, &[('#', &[Item::SLIME_BALL])], b(s::SLIME_BLOCK), 1));
    r.push(shapeless(&[one(s::SLIME_BLOCK)], Item::SLIME_BALL, 9));

    for family in 0..s::SLAB_FAMILIES {
        let base = s::SHAPES[family as usize].base;
        let (stairs_in, slab_in) = shape_inputs(family);
        let stairs_in = if stairs_in.is_empty() { one(base) } else { stairs_in };
        let slab_in = if slab_in.is_empty() { one(base) } else { slab_in };
        if family < s::STAIR_FAMILIES {
            r.push(shaped(STAIRS, &[('#', stairs_in)], b(s::stairs_at(family, 0)), 4));
        }
        r.push(shaped(SLAB, &[('#', slab_in)], b(s::slab_at(family, 0)), 6));
    }
    for (w, &base) in s::WALLS.iter().enumerate() {
        r.push(shaped(WALL, &[('#', one(base))], b(s::wall(w as u16)), 6));
    }
}

/// Java 26.3 stonecutting recipes with this track's results: Java input id,
/// input, Java result id, result and count.
pub const STONECUTTING: &[(&str, Block, &str, Block, u8)] = &[
    ("bricks", Block::BRICKS, "brick_wall", s::wall(10), 1),
    ("cobbled_deepslate", Block::COBBLED_DEEPSLATE, "chiseled_deepslate", s::CHISELED_DEEPSLATE, 1),
    ("deepslate", Block::DEEPSLATE, "chiseled_deepslate", s::CHISELED_DEEPSLATE, 1),
    ("nether_bricks", Block::NETHER_BRICKS, "chiseled_nether_bricks", s::CHISELED_NETHER_BRICKS, 1),
    ("quartz_block", s::QUARTZ_BLOCK, "chiseled_quartz_block", s::CHISELED_QUARTZ_BLOCK, 1),
    ("red_sandstone", s::RED_SANDSTONE, "chiseled_red_sandstone", s::CHISELED_RED_SANDSTONE, 1),
    ("sandstone", Block::SANDSTONE, "chiseled_sandstone", s::CHISELED_SANDSTONE, 1),
    ("stone_bricks", Block::STONE_BRICKS, "chiseled_stone_bricks", s::CHISELED_STONE_BRICKS, 1),
    ("stone", Block::STONE, "chiseled_stone_bricks", s::CHISELED_STONE_BRICKS, 1),
    ("polished_tuff", s::POLISHED_TUFF, "chiseled_tuff_bricks", s::CHISELED_TUFF_BRICKS, 1),
    ("tuff_bricks", s::TUFF_BRICKS, "chiseled_tuff_bricks", s::CHISELED_TUFF_BRICKS, 1),
    ("tuff", Block::TUFF, "chiseled_tuff_bricks", s::CHISELED_TUFF_BRICKS, 1),
    ("tuff", Block::TUFF, "chiseled_tuff", s::CHISELED_TUFF, 1),
    ("red_sandstone", s::RED_SANDSTONE, "cut_red_sandstone", s::CUT_RED_SANDSTONE, 1),
    ("cut_red_sandstone", s::CUT_RED_SANDSTONE, "cut_red_sandstone_slab", s::slab_at(13, 0), 2),
    ("red_sandstone", s::RED_SANDSTONE, "cut_red_sandstone_slab", s::slab_at(13, 0), 2),
    ("sandstone", Block::SANDSTONE, "cut_sandstone", s::CUT_SANDSTONE, 1),
    ("cut_sandstone", s::CUT_SANDSTONE, "cut_sandstone_slab", s::slab_at(12, 0), 2),
    ("sandstone", Block::SANDSTONE, "cut_sandstone_slab", s::slab_at(12, 0), 2),
    ("cobbled_deepslate", Block::COBBLED_DEEPSLATE, "deepslate_brick_slab", s::slab_at(2, 0), 2),
    ("deepslate_bricks", s::DEEPSLATE_BRICKS, "deepslate_brick_slab", s::slab_at(2, 0), 2),
    ("deepslate", Block::DEEPSLATE, "deepslate_brick_slab", s::slab_at(2, 0), 2),
    ("polished_deepslate", Block::POLISHED_DEEPSLATE, "deepslate_brick_slab", s::slab_at(2, 0), 2),
    ("cobbled_deepslate", Block::COBBLED_DEEPSLATE, "deepslate_brick_stairs", s::stairs_at(2, 0), 1),
    ("deepslate_bricks", s::DEEPSLATE_BRICKS, "deepslate_brick_stairs", s::stairs_at(2, 0), 1),
    ("deepslate", Block::DEEPSLATE, "deepslate_brick_stairs", s::stairs_at(2, 0), 1),
    ("polished_deepslate", Block::POLISHED_DEEPSLATE, "deepslate_brick_stairs", s::stairs_at(2, 0), 1),
    ("cobbled_deepslate", Block::COBBLED_DEEPSLATE, "deepslate_brick_wall", s::wall(2), 1),
    ("deepslate_bricks", s::DEEPSLATE_BRICKS, "deepslate_brick_wall", s::wall(2), 1),
    ("deepslate", Block::DEEPSLATE, "deepslate_brick_wall", s::wall(2), 1),
    ("polished_deepslate", Block::POLISHED_DEEPSLATE, "deepslate_brick_wall", s::wall(2), 1),
    ("cobbled_deepslate", Block::COBBLED_DEEPSLATE, "deepslate_bricks", s::DEEPSLATE_BRICKS, 1),
    ("deepslate", Block::DEEPSLATE, "deepslate_bricks", s::DEEPSLATE_BRICKS, 1),
    ("polished_deepslate", Block::POLISHED_DEEPSLATE, "deepslate_bricks", s::DEEPSLATE_BRICKS, 1),
    ("cobbled_deepslate", Block::COBBLED_DEEPSLATE, "deepslate_tile_slab", s::slab_at(3, 0), 2),
    ("deepslate_bricks", s::DEEPSLATE_BRICKS, "deepslate_tile_slab", s::slab_at(3, 0), 2),
    ("deepslate", Block::DEEPSLATE, "deepslate_tile_slab", s::slab_at(3, 0), 2),
    ("deepslate_tiles", s::DEEPSLATE_TILES, "deepslate_tile_slab", s::slab_at(3, 0), 2),
    ("polished_deepslate", Block::POLISHED_DEEPSLATE, "deepslate_tile_slab", s::slab_at(3, 0), 2),
    ("cobbled_deepslate", Block::COBBLED_DEEPSLATE, "deepslate_tile_stairs", s::stairs_at(3, 0), 1),
    ("deepslate_bricks", s::DEEPSLATE_BRICKS, "deepslate_tile_stairs", s::stairs_at(3, 0), 1),
    ("deepslate", Block::DEEPSLATE, "deepslate_tile_stairs", s::stairs_at(3, 0), 1),
    ("deepslate_tiles", s::DEEPSLATE_TILES, "deepslate_tile_stairs", s::stairs_at(3, 0), 1),
    ("polished_deepslate", Block::POLISHED_DEEPSLATE, "deepslate_tile_stairs", s::stairs_at(3, 0), 1),
    ("cobbled_deepslate", Block::COBBLED_DEEPSLATE, "deepslate_tile_wall", s::wall(3), 1),
    ("deepslate_bricks", s::DEEPSLATE_BRICKS, "deepslate_tile_wall", s::wall(3), 1),
    ("deepslate", Block::DEEPSLATE, "deepslate_tile_wall", s::wall(3), 1),
    ("deepslate_tiles", s::DEEPSLATE_TILES, "deepslate_tile_wall", s::wall(3), 1),
    ("polished_deepslate", Block::POLISHED_DEEPSLATE, "deepslate_tile_wall", s::wall(3), 1),
    ("cobbled_deepslate", Block::COBBLED_DEEPSLATE, "deepslate_tiles", s::DEEPSLATE_TILES, 1),
    ("deepslate_bricks", s::DEEPSLATE_BRICKS, "deepslate_tiles", s::DEEPSLATE_TILES, 1),
    ("deepslate", Block::DEEPSLATE, "deepslate_tiles", s::DEEPSLATE_TILES, 1),
    ("polished_deepslate", Block::POLISHED_DEEPSLATE, "deepslate_tiles", s::DEEPSLATE_TILES, 1),
    ("end_stone_bricks", s::END_STONE_BRICKS, "end_stone_brick_slab", s::slab_at(10, 0), 2),
    ("end_stone", Block::END_STONE, "end_stone_brick_slab", s::slab_at(10, 0), 2),
    ("end_stone_bricks", s::END_STONE_BRICKS, "end_stone_brick_stairs", s::stairs_at(10, 0), 1),
    ("end_stone", Block::END_STONE, "end_stone_brick_stairs", s::stairs_at(10, 0), 1),
    ("end_stone_bricks", s::END_STONE_BRICKS, "end_stone_brick_wall", s::wall(6), 1),
    ("end_stone", Block::END_STONE, "end_stone_brick_wall", s::wall(6), 1),
    ("end_stone", Block::END_STONE, "end_stone_bricks", s::END_STONE_BRICKS, 1),
    ("mossy_stone_bricks", Block::MOSSY_STONE_BRICKS, "mossy_stone_brick_slab", s::slab_at(11, 0), 2),
    ("mossy_stone_bricks", Block::MOSSY_STONE_BRICKS, "mossy_stone_brick_stairs", s::stairs_at(11, 0), 1),
    ("mossy_stone_bricks", Block::MOSSY_STONE_BRICKS, "mossy_stone_brick_wall", s::wall(7), 1),
    ("nether_bricks", Block::NETHER_BRICKS, "nether_brick_wall", s::wall(9), 1),
    ("tuff", Block::TUFF, "polished_tuff", s::POLISHED_TUFF, 1),
    ("polished_tuff", s::POLISHED_TUFF, "polished_tuff_slab", s::slab_at(0, 0), 2),
    ("tuff", Block::TUFF, "polished_tuff_slab", s::slab_at(0, 0), 2),
    ("polished_tuff", s::POLISHED_TUFF, "polished_tuff_stairs", s::stairs_at(0, 0), 1),
    ("tuff", Block::TUFF, "polished_tuff_stairs", s::stairs_at(0, 0), 1),
    ("polished_tuff", s::POLISHED_TUFF, "polished_tuff_wall", s::wall(0), 1),
    ("tuff", Block::TUFF, "polished_tuff_wall", s::wall(0), 1),
    ("quartz_block", s::QUARTZ_BLOCK, "quartz_bricks", s::QUARTZ_BRICKS, 1),
    ("quartz_block", s::QUARTZ_BLOCK, "quartz_pillar", s::QUARTZ_PILLAR, 1),
    ("quartz_block", s::QUARTZ_BLOCK, "quartz_slab", s::slab_at(4, 0), 2),
    ("quartz_block", s::QUARTZ_BLOCK, "quartz_stairs", s::stairs_at(4, 0), 1),
    ("red_nether_bricks", s::RED_NETHER_BRICKS, "red_nether_brick_slab", s::slab_at(9, 0), 2),
    ("red_nether_bricks", s::RED_NETHER_BRICKS, "red_nether_brick_stairs", s::stairs_at(9, 0), 1),
    ("red_nether_bricks", s::RED_NETHER_BRICKS, "red_nether_brick_wall", s::wall(5), 1),
    ("red_sandstone", s::RED_SANDSTONE, "red_sandstone_slab", s::slab_at(6, 0), 2),
    ("red_sandstone", s::RED_SANDSTONE, "red_sandstone_stairs", s::stairs_at(6, 0), 1),
    ("red_sandstone", s::RED_SANDSTONE, "red_sandstone_wall", s::wall(4), 1),
    ("sandstone", Block::SANDSTONE, "sandstone_wall", s::wall(8), 1),
    ("smooth_quartz", s::SMOOTH_QUARTZ, "smooth_quartz_slab", s::slab_at(5, 0), 2),
    ("smooth_quartz", s::SMOOTH_QUARTZ, "smooth_quartz_stairs", s::stairs_at(5, 0), 1),
    ("smooth_red_sandstone", s::SMOOTH_RED_SANDSTONE, "smooth_red_sandstone_slab", s::slab_at(7, 0), 2),
    ("smooth_red_sandstone", s::SMOOTH_RED_SANDSTONE, "smooth_red_sandstone_stairs", s::stairs_at(7, 0), 1),
    ("smooth_sandstone", s::SMOOTH_SANDSTONE, "smooth_sandstone_slab", s::slab_at(8, 0), 2),
    ("smooth_sandstone", s::SMOOTH_SANDSTONE, "smooth_sandstone_stairs", s::stairs_at(8, 0), 1),
    ("polished_tuff", s::POLISHED_TUFF, "tuff_brick_slab", s::slab_at(1, 0), 2),
    ("tuff_bricks", s::TUFF_BRICKS, "tuff_brick_slab", s::slab_at(1, 0), 2),
    ("tuff", Block::TUFF, "tuff_brick_slab", s::slab_at(1, 0), 2),
    ("polished_tuff", s::POLISHED_TUFF, "tuff_brick_stairs", s::stairs_at(1, 0), 1),
    ("tuff_bricks", s::TUFF_BRICKS, "tuff_brick_stairs", s::stairs_at(1, 0), 1),
    ("tuff", Block::TUFF, "tuff_brick_stairs", s::stairs_at(1, 0), 1),
    ("polished_tuff", s::POLISHED_TUFF, "tuff_brick_wall", s::wall(1), 1),
    ("tuff_bricks", s::TUFF_BRICKS, "tuff_brick_wall", s::wall(1), 1),
    ("tuff", Block::TUFF, "tuff_brick_wall", s::wall(1), 1),
    ("polished_tuff", s::POLISHED_TUFF, "tuff_bricks", s::TUFF_BRICKS, 1),
    ("tuff", Block::TUFF, "tuff_bricks", s::TUFF_BRICKS, 1),
];

/// What the stonecutter makes from `input` (for v0.17's screen).
pub fn stonecutting(input: Item) -> impl Iterator<Item = (Item, u8)> {
    STONECUTTING.iter().filter(move |r| b(r.1) == input).map(|r| (b(r.3), r.4))
}

#[cfg(test)]
mod tests;
