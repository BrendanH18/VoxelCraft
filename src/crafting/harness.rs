//! v0.17 harnesses (Java 26.3 `data/minecraft/recipe`): `white_harness.json`
//! and the other colours, three leather over glass, the colour's wool and
//! glass; `dye_white_harness.json` and the others, any other colour's
//! harness and a dye.

use super::{GLASS, Ingredient, Recipe, color_ingredients, shaped, shapeless};
use crate::color::DyeColor;
use crate::content::harness::{ITEM_FIRST, harness};
use crate::item::Item;

pub(super) fn add(r: &mut Vec<Recipe>) {
    static DYES: [[Item; 1]; 16] = color_ingredients(0);
    static WOOLS: [[Item; 1]; 16] = color_ingredients(1);
    // For each colour, the other fifteen harnesses (the recipe leaves out
    // the result's own colour).
    static OTHER_HARNESSES: [[Item; 15]; 16] = {
        let mut out = [[Item(0); 15]; 16];
        let mut i = 0;
        while i < 16 {
            let mut j = 0;
            while j < 15 {
                out[i][j] = Item(ITEM_FIRST + (j + (j >= i) as usize) as u16);
                j += 1;
            }
            i += 1;
        }
        out
    };
    const LEATHER: Ingredient = &[Item::LEATHER];
    for (i, c) in DyeColor::ALL.into_iter().enumerate() {
        r.push(shaped(&["LLL", "G#G"], &[('L', LEATHER), ('G', GLASS), ('#', &WOOLS[i])], harness(c), 1));
        r.push(shapeless(&[&DYES[i], &OTHER_HARNESSES[i]], harness(c), 1));
    }
}
