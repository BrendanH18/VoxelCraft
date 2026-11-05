use super::*;
use crate::world::{
    chunk::ChunkData,
    double_chest::{self as dc, ChestType, Kind},
    terrain::Generator,
};
use std::sync::Arc;
const P: IVec3 = IVec3::new(10, 140, 10);
fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(8)), Default::default(), 2);
    w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
    w.set_block(P - IVec3::Y, Block::STONE);
    w
}
#[test]
fn waxing_stops_random_ticks_and_waxed_neighbours_do_not_delay_oxidation() {
    let mut w = world();
    let plain = block(Family::Block, Stage::Unaffected, 0);
    w.set_block(P, plain);
    assert!(w.wax_copper(P));
    let waxed = w.get_block(P).unwrap();
    for _ in 0..1000 {
        w.tick_copper(P, waxed);
    }
    assert_eq!(w.get_block(P), Some(waxed));
    w.set_block(P + IVec3::X, waxed);
    w.set_block(P, block(Family::Block, Stage::Exposed, 0));
    for _ in 0..10000 {
        let b = w.get_block(P).unwrap();
        w.tick_copper(P, b);
        if state(w.get_block(P).unwrap()).unwrap().stage != Stage::Exposed {
            break;
        }
    }
    assert_ne!(state(w.get_block(P).unwrap()).unwrap().stage, Stage::Exposed);
    assert_eq!(w.get_block(P + IVec3::X), Some(waxed));
}
#[test]
fn door_halves_bulb_power_and_double_chest_contents_keep_their_wax() {
    let mut w = world();
    let [lo, hi] = door_halves(block(Family::Door, Stage::Weathered, 0), Facing::East).unwrap();
    w.set_block(P, right_of(lo).unwrap());
    w.set_block(P + IVec3::Y, hi);
    assert!(w.wax_copper(P + IVec3::Y));
    for p in [P, P + IVec3::Y] {
        assert!(state(w.get_block(p).unwrap()).unwrap().waxed);
    }
    crate::session::interact::toggle_door(&mut w, P, glam::Vec3::X);
    for p in [P, P + IVec3::Y] {
        assert!(state(w.get_block(p).unwrap()).unwrap().waxed);
    }
    assert!(w.scrape_copper(P));
    for p in [P, P + IVec3::Y] {
        assert!(!state(w.get_block(p).unwrap()).unwrap().waxed);
    }
    let bulb = encode(Copper { family: Family::Bulb, stage: Stage::Exposed, local: 0, waxed: true });
    w.set_block(P, bulb);
    w.set_block(P + IVec3::X, super::super::redstone_blocks::REDSTONE_BLOCK);
    for _ in 0..4 {
        w.tick_redstone();
    }
    assert!(state(w.get_block(P).unwrap()).unwrap().waxed);
    assert_eq!(self::bulb(w.get_block(P).unwrap()), Some((Stage::Exposed, true, true)));
    let chest =
        |ty| dc::block(dc::ChestState { kind: Kind::Copper(Stage::Weathered), facing: Facing::North, ty, wet: false });
    w.set_block(P, chest(ChestType::Left));
    w.set_block(P + IVec3::X, chest(ChestType::Right));
    w.set_chest_slot(P, 0, Some(crate::inventory::Stack::new(Item::DIAMOND, 4)));
    assert!(w.wax_copper(P));
    for p in [P, P + IVec3::X] {
        assert_eq!(dc::state(w.get_block(p).unwrap()).unwrap().kind, Kind::WaxedCopper(Stage::Weathered));
    }
    assert_eq!(w.chest_slot(P, 0), Some(crate::inventory::Stack::new(Item::DIAMOND, 4)));
    assert!(w.scrape_copper(P + IVec3::X));
    for p in [P, P + IVec3::X] {
        assert_eq!(dc::state(w.get_block(p).unwrap()).unwrap().kind, Kind::Copper(Stage::Weathered));
    }
    assert_eq!(w.chest_slot(P, 0), Some(crate::inventory::Stack::new(Item::DIAMOND, 4)));
}

#[test]
fn all_waxed_states_survive_chunk_storage() {
    use crate::world::storage::{LevelInfo, Storage};
    let mut w = world();
    let at = |id| {
        let n = (id - WAXED_FIRST) as i32;
        IVec3::new(n % 15 * 2, 140 + n / 225 * 2, (n / 15 % 15) * 2)
    };
    for id in WAXED_FIRST..WAXED_END {
        if decode(id).is_some() {
            w.edit_exact(at(id), Block(id), false);
        }
    }
    let dir = std::env::temp_dir().join(format!("voxelcraft-waxed-copper-{}", std::process::id()));
    let store = Storage::new(&dir);
    store.save(&LevelInfo { seed: 8, player: None, props: Default::default() }, &w.modified_chunks()).unwrap();
    let mut loaded = world();
    for (p, data) in store.load_chunks().unwrap() {
        loaded.insert_chunk(p, data, true);
    }
    for id in WAXED_FIRST..WAXED_END {
        if decode(id).is_some() {
            assert_eq!(loaded.get_block(at(id)), Some(Block(id)));
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn joining_waxed_and_unwaxed_chests_unwaxes_both_before_selecting_the_younger_stage() {
    for (left, right) in [
        (Kind::Copper(Stage::Weathered), Kind::WaxedCopper(Stage::Exposed)),
        (Kind::WaxedCopper(Stage::Weathered), Kind::Copper(Stage::Exposed)),
        (Kind::WaxedCopper(Stage::Weathered), Kind::WaxedCopper(Stage::Exposed)),
    ] {
        let mut w = world();
        let chest = |kind| dc::block(dc::ChestState { kind, facing: Facing::North, ty: ChestType::Single, wet: false });
        w.set_block(P, chest(left));
        let ctx = PlaceContext {
            clicked: P + IVec3::X - IVec3::Y,
            clicked_block: Block::STONE,
            face: IVec3::Y,
            click: DVec3::new(0.5, 1., 0.5),
            at: P + IVec3::X,
            facing: Facing::South,
            look: glam::Vec3::Z,
            sneaking: false,
        };
        let new = dc::placed(chest(right), &ctx, |q| w.get_block(q)).unwrap();
        w.set_block(ctx.at, new);
        let expected = if left.copper().unwrap().1 && right.copper().unwrap().1 {
            Kind::WaxedCopper(Stage::Exposed)
        } else {
            Kind::Copper(Stage::Exposed)
        };
        assert_eq!(dc::state(w.get_block(P).unwrap()).unwrap().kind, expected);
        assert_eq!(dc::state(w.get_block(ctx.at).unwrap()).unwrap().kind, expected);
    }
}
