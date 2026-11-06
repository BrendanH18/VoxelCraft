use super::*;
use crate::{
    crafting::Grid,
    inventory::Stack,
    item::Item,
    world::{
        block::Block,
        chunk::ChunkData,
        overworld_blocks as ob, shape, utilities,
        terrain::{Dimension, Generator},
    },
};
use std::sync::Arc;

const FLOOR: i32 = 136;

fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(5)), Default::default(), 2);
    for x in -1..=1 {
        for z in -1..=1 {
            w.insert_chunk(IVec3::new(x, 8, z), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        }
    }
    for x in 0..16 {
        for z in 0..16 {
            w.set_block(IVec3::new(x, FLOOR, z), Block::GRASS);
        }
    }
    w
}

#[test]
fn eyeblossom_states_items_pots_and_recipes() {
    for (b, name) in [(OPEN, "open eyeblossom"), (CLOSED, "closed eyeblossom")] {
        assert_eq!(b.name(), name);
        assert_eq!(Block::from_name(name), Some(b));
        assert!(Block::creative_palette().any(|p| p == b));
        assert!(Item::from_block(b).is_valid());
        assert_eq!(b.hardness(), 0.);
        assert!(!b.is_solid());
        assert!(b.can_stay_on(Block::GRASS) && b.can_stay_on(ob::PALE_MOSS_BLOCK) && !b.can_stay_on(Block::STONE));
        assert_eq!(b.fire_odds(), (60, 100));
        assert_eq!(crate::content::compost::chance(Item::from_block(b)), 0.65);
        assert!(crate::entity::mooshroom::flower_effect(Item::from_block(b)).is_some());
        let pot = utilities::potted(b).unwrap();
        assert_eq!(utilities::potted_plant(pot), Some(b));
        assert!(utilities::is_pot(pot));
        assert_eq!(pot.name(), format!("potted {name}"));
        assert_eq!(pot.hardness(), 0.);
        assert_eq!(pot.drop(), Some(utilities::FLOWER_POT.into()));
        assert_eq!(utilities::extra_drops(pot), Some(Stack::new(b, 1)));
        assert!(!shape::collision(pot, |_| Block::AIR, Block::AIR).as_slice().is_empty());
    }
    let craft = |b: Block| {
        let mut g = Grid::new(2);
        g.cells[0] = Some(Stack::new(b, 1));
        g.result().map(|s| s.item)
    };
    assert_eq!(craft(OPEN), Some(crate::color::DyeColor::Orange.dye()));
    assert_eq!(craft(CLOSED), Some(crate::color::DyeColor::Gray.dye()));
    assert!(open_at(12600) && open_at(23400) && !open_at(23401) && !open_at(12599) && !open_at(6000));
}

#[test]
fn eyeblossoms_follow_the_overworld_clock_and_ripple_through_patches() {
    let mut w = world();
    let flowers: Vec<_> = (2..12).map(|x| IVec3::new(x, FLOOR + 1, 5)).collect();
    for &p in &flowers {
        w.set_block(p, CLOSED);
    }
    // No clock, or another dimension: nothing changes.
    assert!(!w.tick_eyeblossom(flowers[0], CLOSED));
    w.set_bee_clock(Dimension::Nether, 18000);
    assert!(!w.tick_eyeblossom(flowers[0], CLOSED));
    w.set_bee_clock(Dimension::Overworld, 6000);
    assert!(!w.tick_eyeblossom(flowers[0], CLOSED));
    w.set_bee_clock(Dimension::Overworld, 18000);
    assert!(w.tick_eyeblossom(flowers[0], CLOSED));
    assert_eq!(w.get_block(flowers[0]), Some(OPEN));
    // Neighbours within three blocks open on scheduled ticks 5d..=10d later,
    // and each passes it on in turn.
    for _ in 0..4 {
        w.tick_redstone();
    }
    assert_eq!(w.get_block(flowers[1]), Some(CLOSED), "no tick before 5 * distance");
    for _ in 0..400 {
        w.tick_redstone();
    }
    assert!(flowers.iter().all(|&p| w.get_block(p) == Some(OPEN)));
    // Day closes them again the same way.
    w.set_bee_clock(Dimension::Overworld, 23401);
    assert!(w.tick_eyeblossom(flowers[9], OPEN));
    for _ in 0..400 {
        w.tick_redstone();
    }
    assert!(flowers.iter().all(|&p| w.get_block(p) == Some(CLOSED)));
}
