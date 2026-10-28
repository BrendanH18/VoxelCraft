//! Every registered block's drops against Java 26.3's block loot tables
//! (`tests/fixtures/java/26.3/block_loot.json`, written by
//! `tools/extract_java_block_loot_fixtures.py`).
//!
//! Each VoxelCraft state is mapped to its Java block and the state values its
//! table names, then broken through `World::spill_with_item` (the mining drop
//! path) with each scenario's tool: bare hand, a plain tool, shears, Silk Touch
//! and Fortune I-III, and caught in a radius-4 explosion with decay
//! (`World::spill_exploded`). Fixed drops must match exactly; random ones are sampled
//! with the world's seeded RNG and checked against the exact chance, mean and
//! range the extractor computed. Java blocks VoxelCraft doesn't register are
//! listed, not failed; intentional differences are allow-listed below.
mod block_loot;

use block_loot::{Audit, audit};

#[test]
fn every_registered_block_drops_what_its_java_loot_table_gives() {
    let Audit { compared, matched, unregistered, allowed, failures, voxelcraft_only, .. } = audit();
    eprintln!(
        "compared {compared} Java blocks: {matched} match, {} allow-listed, {} fail; {} unregistered: {unregistered:?}",
        allowed.len(),
        failures.len(),
        unregistered.len()
    );
    for (block, reason) in &allowed {
        eprintln!("allowed {block}: {reason}");
    }
    assert!(compared > 900, "the registered families must be covered, compared {compared}");
    assert!(voxelcraft_only.is_empty(), "states with no Java block or allow-list entry: {voxelcraft_only:?}");
    assert!(failures.is_empty(), "{} blocks differ from Java:\n{}", failures.len(), failures.join("\n"));
}

mod structures {
    //! Two-block structures through the real mining path: Java drops their
    //! loot once, from the half its table names.
    use glam::{DVec3, IVec3};
    use std::sync::Arc;
    use voxelcraft::{
        entity::{Entities, PlayerId},
        inventory::Stack,
        item::Item,
        rules::GameRules,
        session::{PlayerSession, mining::Outcome},
        world::{World, block::Block, chunk::ChunkData, overworld_blocks as ob, terrain::Generator},
    };
    const AT: IVec3 = IVec3::new(7, 146, 4);

    fn scene(seed: u64) -> (World, PlayerSession, Entities) {
        let mut w = World::new_remote(Arc::new(Generator::new(seed)), 2);
        w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
        w.set_block(IVec3::new(4, 144, 4), Block::STONE);
        let mut s = PlayerSession::new(PlayerId(3), DVec3::new(4.5, 145., 4.5));
        s.player.on_ground = true;
        s.player.pitch = -0.05;
        (w, s, Entities::new(7))
    }

    fn mine(w: &mut World, s: &mut PlayerSession, e: &mut Entities) {
        assert_eq!(s.block_target(w).map(|(p, _)| p), Some(AT));
        for _ in 0..2000 {
            if !matches!(s.tick_mining(0, w, e, &GameRules::default()).unwrap(), Outcome::Hit { .. }) {
                return;
            }
        }
        panic!("dig did not finish");
    }

    #[test]
    fn either_bed_half_drops_one_bed_from_the_head() {
        for foot_targeted in [true, false] {
            let (mut w, mut s, mut e) = scene(1);
            let (target, other) =
                if foot_targeted { (Block::BED_FOOT, Block::BED_HEAD) } else { (Block::BED_HEAD, Block::BED_FOOT) };
            w.set_block(AT, target);
            w.set_block(AT + IVec3::Z, other);
            mine(&mut w, &mut s, &mut e);
            let head = if foot_targeted { AT + IVec3::Z } else { AT };
            assert_eq!(w.drops, [(head, Stack::new(Item::BED, 1))], "foot targeted: {foot_targeted}");
            assert_eq!((w.get_block(AT), w.get_block(AT + IVec3::Z)), (Some(Block::AIR), Some(Block::AIR)));
        }
    }

    /// Breaking the upper half of tall grass or a large fern rolls its table
    /// once; the lower half's `location_check` then fails, so it adds nothing.
    #[test]
    fn upper_tall_grass_and_large_fern_roll_their_loot_once() {
        for (lower, plant) in [(ob::DOUBLE_TALL_GRASS, Block::TALL_GRASS), (ob::LARGE_FERN, Block::FERN)] {
            let mut seeded = 0;
            let trials = 400;
            for trial in 0..trials {
                for shears in [true, false] {
                    let (mut w, mut s, mut e) = scene(trial);
                    w.set_block(AT - IVec3::Y * 2, Block::DIRT);
                    w.before_overworld_place(AT - IVec3::Y, lower);
                    w.set_block(AT - IVec3::Y, lower);
                    assert_eq!(w.get_block(AT), Some(Block(lower.0 + 1)));
                    if shears {
                        s.inventory.slots[0] = Some(Stack::new(Item::SHEARS, 1));
                    }
                    mine(&mut w, &mut s, &mut e);
                    assert_eq!((w.get_block(AT), w.get_block(AT - IVec3::Y)), (Some(Block::AIR), Some(Block::AIR)));
                    let drops: Vec<_> = w.drops.iter().map(|(_, s)| (s.item, s.count)).collect();
                    if shears {
                        assert_eq!(drops, [(Item::from_block(plant), 2)], "{}", lower.name());
                    } else {
                        assert!(drops.is_empty() || drops == [(Item::WHEAT_SEEDS, 1)], "{}: {drops:?}", lower.name());
                        seeded += drops.len();
                    }
                }
            }
            // Java: one 1-in-8 roll per plant.
            let share = seeded as f64 / f64::from(trials as u32);
            assert!((share - 0.125).abs() < 5.0 * (0.125f64 * 0.875 / 400.0).sqrt(), "{}: {share}", lower.name());
        }
    }
}
