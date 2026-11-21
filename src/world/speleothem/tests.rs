use super::*;
use crate::world::terrain::Generator;
use std::sync::Arc;

fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
    w.load_test_column(glam::IVec2::ZERO, 0);
    w
}

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/wave1_leftovers.json")).unwrap()
}

#[test]
fn constants_match_the_jar() {
    let f = &fixture()["speleothems"];
    assert_eq!(f["growth_probability_per_random_tick"].as_f64().unwrap() as f32, GROWTH_CHANCE);
    assert_eq!(f["sulfur_spike_max_growing_length"], SPIKE_MAX_GROWTH_LENGTH);
    assert_eq!(f["max_stalagmite_search_range_when_growing"], STALAGMITE_SEARCH);
    assert_eq!(f["tag"], serde_json::json!(["pointed_dripstone", "sulfur_spike"]));
    // Only pointed dripstone hurts what lands on it.
    assert_eq!(f["fall_on_declared_by"], serde_json::json!(["PointedDripstoneBlock"]));
}

#[test]
fn thickness_follows_calculate_speleothem_thickness() {
    use Thickness::*;
    let k = Kind::SulfurSpike;
    let up = |t| g::spike(false, t, false);
    let down = |t| g::spike(true, t, false);
    // Nothing in front: a tip.
    assert_eq!(thickness(k, false, true, Block::AIR, Block::STONE), Tip);
    // An opposite tip of the same block: merged unless told not to.
    assert_eq!(thickness(k, false, true, down(Tip), Block::STONE), TipMerge);
    assert_eq!(thickness(k, false, false, down(Tip), Block::STONE), Tip);
    assert_eq!(thickness(k, false, false, down(TipMerge), Block::STONE), TipMerge);
    // An opposite dripstone tip isn't the same block.
    assert_eq!(thickness(k, false, true, ob::pointed_dripstone(true, Tip), Block::STONE), Tip);
    // Behind a tip: frustum; further back: middle, or base at the root.
    assert_eq!(thickness(k, false, true, up(Tip), Block::STONE), Frustum);
    assert_eq!(thickness(k, false, true, up(Frustum), up(Base)), Middle);
    assert_eq!(thickness(k, false, true, up(Frustum), Block::STONE), Base);
}

#[test]
fn neighbours_reshape_a_growing_column() {
    let mut w = world();
    let floor = IVec3::new(4, 0, 4);
    let up = |t| g::spike(false, t, false);
    for y in 1..=3 {
        w.set_block(floor + IVec3::Y * y, up(Thickness::Tip));
    }
    assert_eq!(w.get_block(floor + IVec3::Y), Some(up(Thickness::Base)));
    assert_eq!(w.get_block(floor + IVec3::Y * 2), Some(up(Thickness::Frustum)));
    assert_eq!(w.get_block(floor + IVec3::Y * 3), Some(up(Thickness::Tip)));
    // Breaking the tip makes the frustum the tip.
    w.set_block(floor + IVec3::Y * 3, Block::AIR);
    assert_eq!(w.get_block(floor + IVec3::Y * 2), Some(up(Thickness::Tip)));
    assert_eq!(w.get_block(floor + IVec3::Y), Some(up(Thickness::Frustum)));
    // Pointed dripstone shares the rule.
    let drip = |t| ob::pointed_dripstone(false, t);
    let at = IVec3::new(8, 1, 8);
    w.set_block(at, drip(Thickness::Tip));
    w.set_block(at + IVec3::Y, drip(Thickness::Tip));
    assert_eq!(w.get_block(at), Some(drip(Thickness::Frustum)));
}

#[test]
fn placing_a_spike_takes_its_neighbours_shape() {
    let get = |p: IVec3| match p.y {
        0 => Block::STONE,
        2 => g::spike(true, Thickness::Tip, false),
        3 => Block::STONE,
        _ => Block::AIR,
    };
    let at = IVec3::new(0, 1, 0);
    assert_eq!(placed_spike(at, false, false, get), Some(g::spike(false, Thickness::TipMerge, false)));
    assert_eq!(placed_spike(at, false, true, get), Some(g::spike(false, Thickness::Tip, false)));
    // Nothing to stand on or hang from.
    assert_eq!(placed_spike(at, false, false, |_| Block::AIR), None);
}

/// Hanging spikes under sulfur grow down or raise a stalagmite under them.
#[test]
fn hanging_spikes_under_sulfur_grow() {
    let mut w = world();
    let roof = IVec3::new(4, 12, 4);
    w.set_block(roof, g::SULFUR);
    w.set_block(roof - IVec3::Y, g::spike(true, Thickness::Tip, false));
    let start = roof - IVec3::Y;
    let mut grew = false;
    for _ in 0..20_000 {
        w.speleothem_random_tick(start, w.get_block(start).unwrap());
        let below = w.get_block(start - IVec3::Y).and_then(of);
        let floor = w.get_block(IVec3::new(4, 1, 4)).and_then(of);
        if below.is_some() || floor.is_some() {
            grew = true;
            break;
        }
    }
    assert!(grew, "a spike under sulfur grows within 20 000 random ticks");
    // `findTip` looks one block past the start (`getMaxGrowthLength` 2), so
    // a hanging run stops at three.
    for _ in 0..20_000 {
        let b = w.get_block(start).unwrap();
        w.speleothem_random_tick(start, b);
    }
    let run = (1..=11).take_while(|i| w.get_block(roof - IVec3::Y * *i).and_then(of).is_some_and(|s| s.down)).count();
    assert!(run <= 3, "hanging run of {run}");
    // Without sulfur above nothing grows.
    let mut w = world();
    w.set_block(roof, Block::STONE);
    w.set_block(start, g::spike(true, Thickness::Tip, false));
    for _ in 0..20_000 {
        w.speleothem_random_tick(start, w.get_block(start).unwrap());
    }
    assert_eq!(w.get_block(start - IVec3::Y), Some(Block::AIR));
    assert!(w.get_block(IVec3::new(4, 1, 4)).and_then(of).is_none());
}
