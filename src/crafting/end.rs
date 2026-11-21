//! v0.16 Track A: the purpur family and End rods (`recipe/purpur_*.json`,
//! `recipe/end_rod.json`). Popped chorus fruit is smelted in
//! `world::furnace`; the stonecutter recipes are listed by `stonecutting`.
use super::{Recipe, b, shaped};
use crate::{content::chorus::POPPED_CHORUS_FRUIT, item::Item, world::end_blocks as e};

pub(super) fn add(r: &mut Vec<Recipe>) {
    const PURPUR: &[Item] = &[b(e::PURPUR_BLOCK), b(e::PURPUR_PILLAR)];
    r.push(shaped(&["FF", "FF"], &[('F', &[POPPED_CHORUS_FRUIT])], b(e::PURPUR_BLOCK), 4));
    r.push(shaped(&["#", "#"], &[('#', const { &[b(e::PURPUR_SLAB)] })], b(e::PURPUR_PILLAR), 1));
    r.push(shaped(&["###"], &[('#', PURPUR)], b(e::PURPUR_SLAB), 6));
    r.push(shaped(&["#  ", "## ", "###"], &[('#', PURPUR)], b(e::PURPUR_STAIRS), 4));
    r.push(shaped(&["/", "#"], &[('/', &[Item::BLAZE_ROD]), ('#', &[POPPED_CHORUS_FRUIT])], b(e::END_ROD), 4));
}

/// `purpur_*_from_purpur_block_stonecutting`: the pillar, two slabs, stairs.
pub fn stonecutting() -> impl Iterator<Item = (Item, Item, u8)> {
    [(e::PURPUR_PILLAR, 1), (e::PURPUR_SLAB, 2), (e::PURPUR_STAIRS, 1)]
        .into_iter()
        .map(|(out, n)| (b(e::PURPUR_BLOCK), b(out), n))
}
