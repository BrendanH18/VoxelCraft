//! v0.12 colour track recipes (Java 26.3 `data/minecraft/recipe`): wool and
//! concrete stairs and slabs, banners, dyed candles and the straw bed.
//!
//! The plain candle's recipe (string over honeycomb) waits for honeycomb,
//! which has no survival source until bees (v0.15).

use super::{Ingredient, Recipe, STICK, b, shaped, shapeless};
use crate::color::DyeColor;
use crate::item::Item;

use crate::world::colour_blocks::{self as cb, banner, candle, straw_bed};

pub(super) fn add(r: &mut Vec<Recipe>) {
    static BASES: [[Item; 1]; cb::SHAPE_FAMILIES] = {
        let mut out = [[Item(0); 1]; cb::SHAPE_FAMILIES];
        let mut i = 0;
        while i < cb::SHAPE_FAMILIES {
            out[i][0] = Item::from_block(cb::family_base(i));
            i += 1;
        }
        out
    };
    static DYES: [[Item; 1]; 16] = super::color_ingredients(0);
    const PLAIN_CANDLE: Ingredient = &[Item::from_block(candle::candle(0, 1, false, false))];
    for (i, base) in BASES.iter().enumerate() {
        // `white_wool_stairs.json`, `white_concrete_slab.json`, ...
        r.push(shaped(&["#  ", "## ", "###"], &[('#', base)], b(cb::family_stairs(i)), 4));
        r.push(shaped(&["###"], &[('#', base)], b(cb::family_slab(i)), 6));
    }
    for (i, c) in DyeColor::ALL.into_iter().enumerate() {
        // `white_banner.json`: six wool over a stick.
        r.push(shaped(&["###", "###", " | "], &[('#', &BASES[i]), ('|', STICK)], b(banner::standing(c as u8, 0)), 1));
        // `white_candle.json`: a plain candle and a dye.
        r.push(shapeless(&[PLAIN_CANDLE, &DYES[i]], b(candle::candle(1 + c as u8, 1, false, false)), 1));
    }
    // `white_cushion.json`: three wool slabs; `dye_white_cushion.json`:
    // any other cushion and a dye.
    static SLABS: [[Item; 1]; 16] = {
        let mut out = [[Item(0); 1]; 16];
        let mut i = 0;
        while i < 16 {
            out[i][0] = Item::from_block(cb::family_slab(i));
            i += 1;
        }
        out
    };
    static OTHER_CUSHIONS: [[Item; 15]; 16] = {
        let mut out = [[Item(0); 15]; 16];
        let mut i = 0;
        while i < 16 {
            let mut j = 0;
            while j < 15 {
                out[i][j] = Item(crate::item::CUSHION_ITEM + (j + (j >= i) as usize) as u16);
                j += 1;
            }
            i += 1;
        }
        out
    };
    for (i, c) in DyeColor::ALL.into_iter().enumerate() {
        r.push(shaped(&["###"], &[('#', &SLABS[i])], Item::cushion(c), 1));
        r.push(shapeless(&[&DYES[i], &OTHER_CUSHIONS[i]], Item::cushion(c), 1));
    }
    // `dye_white_wool_slab.json`, `dye_white_wool_stairs.json`: any other
    // colour's wool slab or stairs and a dye.
    static OTHER_WOOL_SLABS: [[Item; 15]; 16] = other_wool_shapes(false);
    static OTHER_WOOL_STAIRS: [[Item; 15]; 16] = other_wool_shapes(true);
    for i in 0..16 {
        r.push(shapeless(&[&DYES[i], &OTHER_WOOL_SLABS[i]], b(cb::family_slab(i)), 1));
        r.push(shapeless(&[&DYES[i], &OTHER_WOOL_STAIRS[i]], b(cb::family_stairs(i)), 1));
    }
    // `straw_bed.json`: three hay bales make four.
    r.push(shaped(&["XXX"], &[('X', super::HAY)], b(straw_bed::ITEM_BLOCK), 4));
}

/// Java's stonecutting recipes for this track's blocks (input, output,
/// count): each concrete makes one stairs or two slabs. The stonecutter's
/// screen arrives in v0.17; this is its data.
pub fn stonecutting() -> Vec<(Item, Item, u8)> {
    (16..cb::SHAPE_FAMILIES)
        .flat_map(|i| {
            let base = Item::from_block(cb::family_base(i));
            [(base, Item::from_block(cb::family_stairs(i)), 1), (base, Item::from_block(cb::family_slab(i)), 2)]
        })
        .collect()
}

/// For each colour, the wool slabs (or stairs) of the other fifteen colours.
const fn other_wool_shapes(stairs: bool) -> [[Item; 15]; 16] {
    let mut out = [[Item(0); 15]; 16];
    let mut i = 0;
    while i < 16 {
        let mut j = 0;
        while j < 15 {
            let family = j + (j >= i) as usize;
            let block = if stairs { cb::family_stairs(family) } else { cb::family_slab(family) };
            out[i][j] = Item::from_block(block);
            j += 1;
        }
        i += 1;
    }
    out
}
