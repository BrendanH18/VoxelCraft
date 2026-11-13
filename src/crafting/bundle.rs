//! v0.17 bundles (Java 26.3 `data/minecraft/recipe`): `bundle.json`, string
//! over leather; `white_bundle.json` and the other colours, the
//! `crafting_transmute` recipes that dye any `#bundles` item with one dye
//! and keep its components. The grid's real result comes from
//! [`crate::content::bundle::transmute`]; these entries give the recipe
//! guide and the CLI's `craft` their ingredient lists.

use super::{Ingredient, Recipe, STRING, color_ingredients, shaped, shapeless};
use crate::color::DyeColor;
use crate::content::bundle::{ITEM_FIRST, dyed};
use crate::item::Item;

/// Java's `#bundles`: the plain bundle, then `DyeColor` order.
static BUNDLES: [Item; 17] = {
    let mut out = [Item(0); 17];
    let mut i = 0;
    while i < 17 {
        out[i] = Item(ITEM_FIRST + i as u16);
        i += 1;
    }
    out
};

pub(super) fn add(r: &mut Vec<Recipe>) {
    static DYES: [[Item; 1]; 16] = color_ingredients(0);
    const LEATHER: Ingredient = &[Item::LEATHER];
    r.push(shaped(&["-", "#"], &[('-', STRING), ('#', LEATHER)], Item(ITEM_FIRST), 1));
    for (i, c) in DyeColor::ALL.into_iter().enumerate() {
        r.push(shapeless(&[&BUNDLES, &DYES[i]], dyed(c), 1));
    }
}
