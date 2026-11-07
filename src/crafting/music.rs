//! Java 26.3 `recipe/jukebox.json`, `recipe/music_disc_5.json` and
//! `recipe/recovery_compass.json` (v0.16 track B).
use super::{PLANKS, Recipe, b, ingredient, shaped, shapeless};
use crate::{item::Item, world::jukebox as j};
pub(super) fn add(r: &mut Vec<Recipe>) {
    r.push(shaped(&["###", "#X#", "###"], &[('#', PLANKS), ('X', &[Item::DIAMOND])], b(j::JUKEBOX), 1));
    let fragment = ingredient(j::DISC_FRAGMENT_5);
    r.push(shapeless(&[fragment; 9], j::DISC_5, 1));
    r.push(shaped(
        &["SSS", "SCS", "SSS"],
        &[('S', ingredient(j::ECHO_SHARD)), ('C', &[Item::COMPASS])],
        j::RECOVERY_COMPASS,
        1,
    ));
}
