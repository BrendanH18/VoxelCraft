//! Java 26.3 resin and creaking heart acquisition.
use super::{Recipe, b, ingredient, shaped, shapeless};
use crate::{
    item::Item,
    world::{creaking as c, overworld_blocks as ob},
};
pub(super) fn add(r: &mut Vec<Recipe>) {
    r.push(shaped(&["RRR", "RRR", "RRR"], &[('R', ingredient(b(c::RESIN_CLUMP)))], b(c::RESIN_BLOCK), 1));
    r.push(shapeless(&[ingredient(b(c::RESIN_BLOCK))], b(c::RESIN_CLUMP), 9));
    r.push(shaped(&["RR", "RR"], &[('R', &[Item::RESIN_BRICK])], b(c::RESIN_BRICKS), 1));
    r.push(shaped(&["R  ", "RR ", "RRR"], &[('R', ingredient(b(c::RESIN_BRICKS)))], b(c::STAIRS), 4));
    r.push(shaped(&["RRR"], &[('R', ingredient(b(c::RESIN_BRICKS)))], b(c::SLAB), 6));
    r.push(shaped(&["RRR", "RRR"], &[('R', ingredient(b(c::RESIN_BRICKS)))], b(c::WALL), 6));
    r.push(shaped(&["R", "R"], &[('R', ingredient(b(c::SLAB)))], b(c::CHISELED), 1));
    r.push(shaped(
        &["L", "R", "L"],
        &[('L', ingredient(b(ob::PALE_OAK_LOG))), ('R', ingredient(b(c::RESIN_BLOCK)))],
        b(c::HEART),
        1,
    ));
}
