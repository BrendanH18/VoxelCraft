//! Java 26.3's copper crafting and stonecutting recipes (`data/minecraft/recipe`,
//! the unwaxed `*copper*` and `lightning_rod` files) for `world::copper`.
//! Waxed recipes come with honeycomb in v0.15.

use super::{Recipe, b, ingredient, shaped};
use crate::item::Item;
use crate::world::copper::{Family, Stage, block};

/// Every stage's cut copper, its stairs, slab and chiseled form, grate and
/// bulb, then the unaffected-only door, trapdoor, bars, chain, lantern,
/// chest, lightning rod and torch.
pub(super) fn add_recipes(r: &mut Vec<Recipe>) {
    let item = |f: Family, s: Stage| b(block(f, s, 0));
    for s in Stage::ALL {
        let copper = ingredient(item(Family::Block, s));
        let cut = ingredient(item(Family::Cut, s));
        let slab = ingredient(item(Family::Slab, s));
        r.push(shaped(&["##", "##"], &[('#', copper)], item(Family::Cut, s), 4));
        r.push(shaped(&["#  ", "## ", "###"], &[('#', cut)], item(Family::Stairs, s), 4));
        r.push(shaped(&["###"], &[('#', cut)], item(Family::Slab, s), 6));
        r.push(shaped(&["#", "#"], &[('#', slab)], item(Family::Chiseled, s), 1));
        r.push(shaped(&[" M ", "M M", " M "], &[('M', copper)], item(Family::Grate, s), 4));
        r.push(shaped(
            &[" C ", "CBC", " R "],
            &[('C', copper), ('B', &[Item::BLAZE_ROD]), ('R', &[Item::REDSTONE])],
            item(Family::Bulb, s),
            4,
        ));
    }
    let u = Stage::Unaffected;
    let ingot: super::Ingredient = &[Item::COPPER_INGOT];
    let nugget: super::Ingredient = &[Item::COPPER_NUGGET];
    r.push(shaped(&["##", "##", "##"], &[('#', ingot)], item(Family::Door, u), 3));
    r.push(shaped(&["##", "##"], &[('#', ingot)], item(Family::Trapdoor, u), 1));
    r.push(shaped(&["###", "###"], &[('#', ingot)], item(Family::Bars, u), 16));
    r.push(shaped(&["N", "I", "N"], &[('N', nugget), ('I', ingot)], item(Family::Chain, u), 1));
    let torch = ingredient(item(Family::Torch, u));
    r.push(shaped(&["XXX", "X#X", "XXX"], &[('X', nugget), ('#', torch)], item(Family::Lantern, u), 1));
    let chest = ingredient(b(crate::world::block::Block::CHEST));
    r.push(shaped(&["###", "#X#", "###"], &[('#', ingot), ('X', chest)], item(Family::Chest, u), 1));
    r.push(shaped(&["#", "#", "#"], &[('#', ingot)], item(Family::Rod, u), 1));
    r.push(shaped(
        &["C", "X", "#"],
        &[('C', nugget), ('X', super::FUEL_LUMP), ('#', super::STICK)],
        item(Family::Torch, u),
        4,
    ));
}

/// Java's copper `*_stonecutting` recipes as (input, output, count), for the
/// stonecutter's screen (v0.17): each stage's block cuts into cut copper,
/// its slab, stairs, chiseled copper and grate, and cut copper into the
/// slab, stairs and chiseled copper.
pub fn stonecutting() -> Vec<(Item, Item, u8)> {
    let item = |f: Family, s: Stage| b(block(f, s, 0));
    Stage::ALL
        .into_iter()
        .flat_map(|s| {
            let (copper, cut) = (item(Family::Block, s), item(Family::Cut, s));
            [
                (copper, cut, 4),
                (copper, item(Family::Slab, s), 8),
                (copper, item(Family::Stairs, s), 4),
                (copper, item(Family::Chiseled, s), 4),
                (copper, item(Family::Grate, s), 4),
                (cut, item(Family::Slab, s), 2),
                (cut, item(Family::Stairs, s), 1),
                (cut, item(Family::Chiseled, s), 1),
            ]
        })
        .collect()
}
