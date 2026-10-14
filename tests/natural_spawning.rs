//! Java 26.3 natural spawning and despawning (`NaturalSpawner`,
//! `MobCategory`, `Mob.checkDespawn`) through the shared entity update:
//! monsters fill the category cap of 70 from chunks out to 128 blocks, and
//! despawnable mobs go at once beyond 128 blocks and at random beyond 32
//! once idle for 600 ticks.
use glam::{DVec3, IVec3};
use voxelcraft::{
    entity::{Ctx, Entities, MobKind, MobWorld, PlayerId, Target, category::MobCategory},
    physics::BlockSource,
    simulation::lighting::SpawnLight,
    world::{block::Block, terrain::Dimension},
};

/// A dark stone plain: stone below y = 64, air above, every column loaded,
/// with 1x1 pits two deep at x = 20 (every other z from 0 to 6).
struct Plain;

impl BlockSource for Plain {
    fn block(&self, p: IVec3) -> Option<Block> {
        let pit = p.x == 20 && (0..=6).contains(&p.z) && p.z % 2 == 0 && (62..64).contains(&p.y);
        Some(if p.y < 64 && !pit { Block::STONE } else { Block::AIR })
    }
}

impl MobWorld for Plain {
    fn loaded(&self, _: IVec3) -> bool {
        true
    }
    fn surface(&self, _: i32, _: i32) -> Option<i32> {
        Some(63)
    }
    fn exposed(&self, _: IVec3) -> bool {
        false
    }
    fn sky_light_level(&self, _: IVec3) -> Option<u8> {
        Some(0)
    }
}

fn night(spawning: bool) -> Ctx {
    Ctx {
        spawn_light: SpawnLight::NIGHT,
        // Not targetable, so nothing chases the player and spoils positions.
        players: vec![Target::new(PlayerId::HOST, DVec3::new(0.5, 64.0, 0.5), false)],
        daylight: 0.0,
        spawning,
        raining: false,
        dimension: Dimension::Overworld,
    }
}

#[test]
fn monsters_fill_the_category_cap_from_chunks_out_to_128_blocks() {
    let ctx = night(true);
    let mut e = Entities::new(21);
    let mut beyond_64 = 0;
    for _ in 0..20 * 40 {
        e.ensure_mob_ids();
        let old: std::collections::HashSet<u32> = e.mobs.iter().map(|m| m.uid).collect();
        e.update(0.05, &Plain, &ctx);
        e.ensure_mob_ids();
        for m in e.mobs.iter().filter(|m| !old.contains(&m.uid)) {
            let d = m.pos.distance(ctx.players[0].pos);
            assert!(d > 24.0 && d <= 128.0, "{:?} spawned {d:.1} blocks away", m.kind);
            beyond_64 += (d > 64.0) as u32;
        }
    }
    let monsters = e.mobs.iter().filter(|m| m.kind.category() == MobCategory::Monster).count();
    assert!((60..=73).contains(&monsters), "{monsters} monsters");
    assert!(beyond_64 > 0, "the spawn disc reaches past the old 64-block ring");
}

#[test]
fn far_mobs_despawn_at_once_and_idle_ones_at_random() {
    let ctx = night(false);
    let mut e = Entities::new(5);
    e.spawn(MobKind::Zombie, DVec3::new(140.5, 64.0, 0.5));
    for i in 0..16 {
        e.spawn(MobKind::Skeleton, DVec3::new(60.5, 64.0, i as f64 * 2.0));
    }
    // Kept in pits so they can't stroll beyond 32 blocks.
    for i in 0..4 {
        e.spawn(MobKind::Creeper, DVec3::new(20.5, 62.0, i as f64 * 2.0 + 0.5));
    }
    e.update(0.05, &Plain, &ctx);
    assert!(e.mobs.iter().all(|m| m.kind != MobKind::Zombie), "beyond 128 blocks it goes at once");
    // 30 s idle, then 1 in 800 a tick: after 150 s about 5% remain.
    for _ in 0..20 * 150 {
        e.update(0.05, &Plain, &ctx);
    }
    let skeletons = e.mobs.iter().filter(|m| m.kind == MobKind::Skeleton).count();
    let creepers = e.mobs.iter().filter(|m| m.kind == MobKind::Creeper).count();
    assert!(skeletons <= 4, "{skeletons} idle skeletons 60 blocks away survived");
    assert_eq!(creepers, 4, "nothing despawns within 32 blocks");
}

#[test]
fn persistent_mobs_never_despawn() {
    let ctx = night(false);
    let mut e = Entities::new(6);
    e.spawn(MobKind::Zombie, DVec3::new(200.5, 64.0, 0.5));
    e.mobs[0].persistent = true;
    e.spawn(MobKind::Cow, DVec3::new(300.5, 64.0, 0.5));
    for _ in 0..20 * 60 {
        e.update(0.05, &Plain, &ctx);
    }
    assert_eq!(e.mobs.len(), 2);
}
