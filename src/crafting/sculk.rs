//! Java 26.3 `calibrated_sculk_sensor`: an amethyst shard on top and either
//! side of a sculk sensor (v0.16 track A).
use super::{Recipe, b, ingredient, shaped};
use crate::{item::Item, world::cave_blocks as cb};
pub(super) fn add(r: &mut Vec<Recipe>) {
    r.push(shaped(
        &[" # ", "#X#"],
        &[('#', &[Item::AMETHYST_SHARD]), ('X', ingredient(b(cb::SCULK_SENSOR)))],
        b(cb::CALIBRATED_SCULK_SENSOR),
        1,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calibrated_sensor_recipe_matches_java_shape() {
        let mut r = Vec::new();
        add(&mut r);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].result.item, b(cb::CALIBRATED_SCULK_SENSOR));
        assert_eq!(r[0].result.count, 1);
    }
}
