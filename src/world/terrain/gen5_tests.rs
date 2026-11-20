//! Generation 5 Overworld and Nether content of v0.16 Track D, found on
//! fixed seeds: dappled forests with poplars, leaf litter, red shrubs and
//! shelf mushrooms, and nether gold ore. Generation 4 keeps none of it.

use glam::{IVec2, IVec3};

use super::{Biome, CHUNK_SIZE_I, Dimension, Generation, Generator};
use crate::world::biome_plants as bp;
use crate::world::gen5_blocks as g;
use crate::world::wood::poplar;

#[test]
fn generation_5_grows_dappled_forests_of_poplars() {
    let gen5 = Generator::with_generation(12345, Dimension::Overworld, Generation::V5);
    let at = gen5.nearest_biome(IVec2::ZERO, Biome::DappledForest, 8000).expect("a dappled forest within 8000 blocks");
    let (mut logs, mut leaves, mut shrubs, mut litter, mut shelves) = (0, 0, 0, 0, 0);
    let c = at.div_euclid(IVec3::splat(CHUNK_SIZE_I));
    for dx in -2..=2 {
        for dz in -2..=2 {
            for dy in -1..=1 {
                gen5.generate(c + IVec3::new(dx, dy, dz)).for_each_block(|b| {
                    logs += usize::from(poplar::is_log(b));
                    leaves += usize::from(poplar::is_leaves(b));
                    shrubs += usize::from(b == g::RED_SHRUB);
                    litter += usize::from(bp::base(b.0) == Some(bp::LEAF_LITTER) || b == bp::LEAF_LITTER);
                    shelves += usize::from(g::shelf_of(b).is_some());
                });
            }
        }
    }
    assert!(logs > 20 && leaves > 100, "poplars near {at}: {logs} logs, {leaves} leaves");
    assert!(shrubs > 0, "red shrubs near {at}");
    assert!(litter > 0, "leaf litter near {at}");
    assert!(shelves > 0, "shelf mushrooms near {at}");
    // The same columns of a generation 4 world are another biome.
    let gen4 = Generator::with_generation(12345, Dimension::Overworld, Generation::V4);
    assert_ne!(gen4.column(at.x, at.z).biome, Biome::DappledForest);
    let mut old = 0;
    gen4.generate(c).for_each_block(|b| old += usize::from(poplar::is_log(b) || b == g::RED_SHRUB));
    assert_eq!(old, 0);
}

#[test]
fn generation_5_nether_holds_gold_ore() {
    let count = |generation| {
        let nether = Generator::with_generation(99, Dimension::Nether, generation);
        let mut n = 0;
        for x in -1..=1 {
            for z in -1..=1 {
                for y in 1..7 {
                    nether.generate(IVec3::new(x, y, z)).for_each_block(|b| n += usize::from(b == g::NETHER_GOLD_ORE));
                }
            }
        }
        n
    };
    assert!(count(Generation::V5) > 20, "nether gold ore in generation 5");
    assert_eq!(count(Generation::V4), 0);
}
