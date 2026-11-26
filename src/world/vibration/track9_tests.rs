//! v1.0 Track 9: block ticks and block changes Java posts as game events
//! (`docs/verification/2026-11-25-v1.0-track-09-controls-sculk.md`).

use std::sync::Arc;

use glam::{DVec3, IVec3};

use super::{EventSource, GameEvent, Posted};
use crate::entity::PlayerId;
use crate::world::World;
use crate::world::block::{Block, Facing};
use crate::world::chunk::ChunkData;
use crate::world::overworld_blocks as ob;
use crate::world::terrain::Generator;

const P: IVec3 = IVec3::new(8, 145, 8);

fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
    w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
    for x in 0..32 {
        for z in 0..32 {
            w.set_block(IVec3::new(x, P.y - 1, z), Block::STONE);
        }
    }
    w.set_entity_listeners(true);
    w.take_entity_game_events();
    w
}

fn at(w: &mut World, cell: IVec3) -> Vec<Posted> {
    w.take_entity_game_events().into_iter().filter(|p| p.pos.floor().as_ivec3() == cell).collect()
}

fn kinds(posted: &[Posted]) -> Vec<GameEvent> {
    posted.iter().map(|p| p.event).collect()
}

#[test]
fn eyeblossoms_and_dried_ghasts_post_block_change() {
    use crate::world::dried_ghast::{State, block};
    use crate::world::eyeblossom::{CLOSED, OPEN};
    let mut w = world();
    w.eyeblossom_open = Some(true);
    w.set_block(P, CLOSED);
    w.take_entity_game_events();
    assert!(w.tick_eyeblossom(P, CLOSED));
    assert_eq!(w.get_block(P), Some(OPEN));
    assert_eq!(kinds(&at(&mut w, P)), [GameEvent::BlockChange]);
    // Hydrating in water, then drying out.
    let ghast = P + IVec3::X * 3;
    let wet = State { hydration: 0, facing: Facing::North, waterlogged: true };
    w.set_block(ghast, block(wet));
    w.take_entity_game_events();
    w.dried_ghast_tick(ghast);
    let grew = at(&mut w, ghast);
    assert_eq!(kinds(&grew), [GameEvent::BlockChange]);
    assert_eq!(grew[0].source, EventSource::NONE);
    // On dry land (a cell that never held water) it dries out.
    let dry = P + IVec3::X * 6;
    w.set_block(dry, block(State { hydration: 2, waterlogged: false, ..wet }));
    w.take_entity_game_events();
    w.dried_ghast_tick(dry);
    assert_eq!(kinds(&at(&mut w, dry)), [GameEvent::BlockChange]);
    // Dry with nothing to lose: no change, no event.
    w.set_block(dry, block(State { hydration: 0, waterlogged: false, ..wet }));
    w.take_entity_game_events();
    w.dried_ghast_tick(dry);
    assert!(at(&mut w, dry).is_empty());
}

/// `TurtleEggBlock.randomTick` before dawn on sand: `block_change` for a
/// crack, `block_destroy` when they hatch; `SweetBerryBushBlock.randomTick`.
#[test]
fn turtle_eggs_crack_and_hatch_and_berry_bushes_grow_with_events() {
    let mut w = world();
    w.set_redstone_daylight(0.92, 0, 0.0);
    // Raised a block so the sand under the eggs rests on stone.
    let egg = P + IVec3::Y;
    w.set_block(egg - IVec3::Y * 2, Block::STONE);
    w.set_block(egg - IVec3::Y, Block::SAND);
    w.set_block(egg, ob::turtle_eggs_stage(2, 0));
    w.take_entity_game_events();
    assert!(w.tick_overworld_flora(egg, ob::turtle_eggs_stage(2, 0)));
    assert_eq!(kinds(&at(&mut w, egg)), [GameEvent::BlockChange]);
    w.set_block(egg, ob::turtle_eggs_stage(2, 2));
    w.take_entity_game_events();
    w.tick_overworld_flora(egg, ob::turtle_eggs_stage(2, 2));
    assert_eq!(w.get_block(egg), Some(Block::AIR));
    assert_eq!(kinds(&at(&mut w, egg)), [GameEvent::BlockDestroy]);
    // A lit bush grows one tick in five (glowstone beside the cell above
    // gives it light 14; the headless chunk has no skylight yet).
    let bush = P + IVec3::X * 4;
    w.set_block(bush - IVec3::Y, Block::GRASS);
    w.set_block(bush + IVec3::new(1, 1, 0), Block::GLOWSTONE);
    let mut grown = 0;
    for _ in 0..200 {
        w.set_block(bush, ob::berry_bush(0));
        w.take_entity_game_events();
        w.tick_overworld_flora(bush, ob::berry_bush(0));
        if w.get_block(bush) == Some(ob::berry_bush(1)) {
            let posted = at(&mut w, bush);
            assert_eq!(kinds(&posted), [GameEvent::BlockChange]);
            assert_eq!(posted[0].affected, Some(ob::berry_bush(1)));
            grown += 1;
        } else {
            assert!(at(&mut w, bush).is_empty());
        }
    }
    assert!((20..=60).contains(&grown), "{grown}");
}

#[test]
fn powered_shelves_and_sniffer_eggs_post_their_events() {
    use crate::world::wood::Kind;
    use crate::world::wood::shelves::{Chain, Shelf};
    let mut w = world();
    let shelf = Shelf { kind: Kind::Oak, facing: Facing::North, powered: false, chain: Chain::Unconnected, wet: false };
    w.set_block(P, shelf.block());
    w.take_entity_game_events();
    w.set_block(P + IVec3::X, crate::world::redstone_blocks::REDSTONE_BLOCK);
    w.shelf_power(P);
    let on = at(&mut w, P);
    assert_eq!(kinds(&on), [GameEvent::BlockActivate]);
    // The neighbour update powers it down (`neighborChanged`).
    w.set_block(P + IVec3::X, Block::AIR);
    w.shelf_power(P);
    assert_eq!(kinds(&at(&mut w, P)), [GameEvent::BlockDeactivate]);
    // `SnifferEggBlock.onPlace`.
    let egg = P + IVec3::X * 4;
    w.set_block(egg, crate::world::sniffer_egg::EGG);
    let placed = at(&mut w, egg);
    assert_eq!(kinds(&placed), [GameEvent::BlockPlace]);
    assert_eq!(placed[0].affected, Some(crate::world::sniffer_egg::EGG));
}

#[test]
fn farmland_turning_to_dirt_and_frost_walking_post_from_their_source() {
    let mut w = world();
    w.set_block(P, Block::FARMLAND);
    w.take_entity_game_events();
    let walker = EventSource::player(PlayerId(3), false);
    w.farmland_to_dirt(P, walker);
    assert_eq!(w.get_block(P), Some(Block::DIRT));
    let changed = at(&mut w, P);
    assert_eq!(kinds(&changed), [GameEvent::BlockChange]);
    assert_eq!(changed[0].source.player, Some(PlayerId(3)));
    // Covered farmland reverts, from no one: Track 4 turns it when the
    // block lands on it (`updateShape`), or failing that on its random tick.
    let covered = P + IVec3::X * 3;
    w.set_block(covered, Block::FARMLAND);
    w.take_entity_game_events();
    w.set_block(covered + IVec3::Y, Block::STONE);
    w.random_tick(covered);
    assert_eq!(w.get_block(covered), Some(Block::DIRT));
    let reverted = at(&mut w, covered);
    assert_eq!(kinds(&reverted), [GameEvent::BlockChange]);
    assert_eq!(reverted[0].source, EventSource::NONE);
    // `ReplaceDisk` (Frost Walker): `block_place` from the walker per block.
    let pool = P + IVec3::new(10, 0, 10);
    for dx in -2..=2 {
        for dz in -2..=2 {
            w.set_block(pool + IVec3::new(dx, -1, dz), Block::WATER);
        }
    }
    w.take_entity_game_events();
    let feet = pool.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
    let placed = w.frost_walk_by(feet, 1, |_| false, walker);
    let events = w.take_entity_game_events();
    assert!(placed > 0);
    assert_eq!(events.len(), placed);
    assert!(events.iter().all(|e| e.event == GameEvent::BlockPlace && e.source.player == Some(PlayerId(3))));
}
