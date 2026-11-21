//! v0.16 Track D recipes (Java 26.3 `data/minecraft/recipe`): the poplar
//! family (`poplar_planks.json`, `poplar_wood.json`, `stripped_poplar_wood.json`,
//! `poplar_stairs.json`, `poplar_slab.json`, `poplar_fence.json`,
//! `poplar_fence_gate.json`, `poplar_door.json`; the trapdoor, button,
//! plate, signs, shelf and boats come with their shared wood recipes), the
//! cinnabar and sulfur families with their stonecutting, `potent_sulfur.json`,
//! `sulfur_from_sulfur_spikes.json`, `golden_dandelion.json`,
//! `yellow_dye_from_golden_dandelion.json` and
//! `rabbit_stew_from_shelf_mushroom.json`.

use super::{Recipe, STICK, b, ingredient, shaped, shapeless};
use crate::item::Item;
use crate::world::block::Block;
use crate::world::gen5_blocks as g;
use crate::world::wood::poplar as p;

pub(super) fn add(r: &mut Vec<Recipe>) {
    r.push(shapeless(&[&p::LOG_TAG], b(p::PLANKS), 4));
    r.push(shaped(&["##", "##"], &[('#', ingredient(b(p::LOG)))], b(p::WOOD), 3));
    r.push(shaped(&["##", "##"], &[('#', ingredient(b(p::STRIPPED_LOG)))], b(p::STRIPPED_WOOD), 3));
    let planks = ingredient(b(p::PLANKS));
    r.push(shaped(&["#  ", "## ", "###"], &[('#', planks)], b(p::STAIRS), 4));
    r.push(shaped(&["###"], &[('#', planks)], b(p::SLAB), 6));
    r.push(shaped(&["#s#", "#s#"], &[('#', planks), ('s', STICK)], b(p::FENCE), 3));
    r.push(shaped(&["s#s", "s#s"], &[('#', planks), ('s', STICK)], b(p::GATE), 1));
    r.push(shaped(&["##", "##", "##"], &[('#', planks)], b(p::DOOR), 3));
    // Cinnabar and sulfur: polished from four of the stone, bricks from
    // four polished, chiseled from two slabs; stairs, slabs and walls of
    // the stone, polished and bricks.
    for stone in [0u16, 3] {
        let base = g::STONE_BASES[stone as usize];
        let polished = g::STONE_BASES[stone as usize + 1];
        let bricks = g::STONE_BASES[stone as usize + 2];
        let chiseled = Block(base.0 + 3);
        r.push(shaped(&["SS", "SS"], &[('S', ingredient(b(base)))], b(polished), 4));
        r.push(shaped(&["##", "##"], &[('#', ingredient(b(polished)))], b(bricks), 4));
        r.push(shaped(&["#", "#"], &[('#', ingredient(b(g::slab(stone))))], b(chiseled), 1));
        for f in stone..stone + 3 {
            let mat = ingredient(b(g::STONE_BASES[f as usize]));
            r.push(shaped(&["#  ", "## ", "###"], &[('#', mat)], b(g::stairs(f)), 4));
            r.push(shaped(&["###"], &[('#', mat)], b(g::slab(f)), 6));
            r.push(shaped(&["###", "###"], &[('#', mat)], b(g::wall(f)), 6));
        }
    }
    r.push(shapeless(&[ingredient(b(g::SULFUR)); 9], b(g::POTENT_SULFUR), 1));
    r.push(shaped(&["##", "##"], &[('#', ingredient(b(g::SULFUR_SPIKE)))], b(g::SULFUR), 1));
    r.push(shaped(
        &["###", "#I#", "###"],
        &[('#', &[Item::GOLD_NUGGET]), ('I', ingredient(b(Block::DANDELION)))],
        b(g::GOLDEN_DANDELION),
        1,
    ));
    r.push(shapeless(&[ingredient(b(g::GOLDEN_DANDELION))], crate::color::DyeColor::Yellow.dye(), 1));
    r.push(shapeless(
        &[
            &[Item::BAKED_POTATO],
            &[Item::COOKED_RABBIT],
            &[Item::BOWL],
            &[Item::CARROT],
            ingredient(b(g::SHELF_MUSHROOM)),
        ],
        Item::RABBIT_STEW,
        1,
    ));
}

/// Java 26.3's cinnabar and sulfur stonecutting recipes: each stone cuts
/// into its polished form, bricks, chiseled block and every shape of
/// itself and of the forms below it (polished cuts into bricks and their
/// shapes; bricks into their own shapes).
pub fn stonecutting() -> Vec<(Item, Item, u8)> {
    let mut out = Vec::new();
    for stone in [0u16, 3] {
        for from in stone..stone + 3 {
            let input = b(g::STONE_BASES[from as usize]);
            if from == stone {
                out.push((input, b(Block(g::STONE_BASES[stone as usize].0 + 3)), 1));
            }
            for to in from..stone + 3 {
                if to != from {
                    out.push((input, b(g::STONE_BASES[to as usize]), 1));
                }
                out.push((input, b(g::stairs(to)), 1));
                out.push((input, b(g::slab(to)), 2));
                out.push((input, b(g::wall(to)), 1));
            }
        }
    }
    out
}
