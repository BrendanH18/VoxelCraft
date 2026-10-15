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

/// Bright grass surface; generation explicitly reports completed columns.
struct Grass {
    generation: bool,
}
impl BlockSource for Grass {
    fn block(&self, p: IVec3) -> Option<Block> {
        Some(if p.y == 63 {
            Block::GRASS
        } else if p.y < 63 {
            Block::STONE
        } else {
            Block::AIR
        })
    }
}
impl MobWorld for Grass {
    fn loaded(&self, _: IVec3) -> bool {
        true
    }
    fn surface(&self, _: i32, _: i32) -> Option<i32> {
        Some(63)
    }
    fn exposed(&self, p: IVec3) -> bool {
        p.y >= 64
    }
    fn generation_columns(&self, visit: &mut dyn FnMut(glam::IVec2)) {
        if self.generation {
            for z in -8..=8 {
                for x in -8..=8 {
                    visit(glam::IVec2::new(x, z));
                }
            }
        }
    }
}

#[test]
fn creatures_share_the_ten_mob_cap_and_only_spawn_every_400_ticks() {
    let w = Grass { generation: false };
    let c = night(true);
    let mut e = Entities::new(12);
    // The first tick is game time zero. The next 399 have no creature pass.
    e.update(0.05, &w, &c);
    e.mobs.clear();
    for _ in 0..399 {
        e.update(0.05, &w, &c);
    }
    assert!(e.mobs.iter().all(|m| m.kind.category() != MobCategory::Creature));
    for _ in 0..400 * 20 {
        e.update(0.05, &w, &c);
    }
    let creatures: Vec<_> = e.mobs.iter().filter(|m| m.kind.category() == MobCategory::Creature).collect();
    assert!((10..=13).contains(&creatures.len()), "{} creatures", creatures.len());
    assert!(creatures.iter().any(|m| m.pos.distance(c.players[0].pos) > 64.0));
}

#[test]
fn generated_herds_survive_restart_and_cleared_columns_do_not_repopulate() {
    let w = Grass { generation: true };
    let mut c = night(true);
    let mut e = Entities::new(42);
    e.update(0.05, &w, &c);
    let before = e.mobs.iter().filter(|m| m.kind.category() == MobCategory::Creature).count();
    assert!(before > 20, "generation is independent of the ten-creature runtime cap: {before}");
    e.ensure_mob_ids();
    let saved = e.nether_mobs_to_string();
    let mut loaded = Entities::new(42);
    loaded.load_nether_mobs(&saved);
    assert_eq!(loaded.mobs.iter().filter(|m| m.kind.category() == MobCategory::Creature).count(), before);
    c.spawning = false;
    c.players[0].pos.x = 1000.5;
    loaded.update(0.05, &w, &c);
    assert_eq!(loaded.mobs.len(), before, "animals do not distance-despawn");
    loaded.mobs.clear();
    let cleared = loaded.nether_mobs_to_string();
    let mut reloaded = Entities::new(42);
    reloaded.load_nether_mobs(&cleared);
    c.spawning = true;
    c.players[0].pos.x = 0.5;
    reloaded.update(0.05, &w, &c);
    assert!(
        reloaded.mobs.iter().all(|m| m.kind.category() != MobCategory::Creature),
        "saved generation markers prevent resurrection"
    );
}

#[test]
fn spectators_do_not_create_spawn_chunks_or_hold_mobs_nearby() {
    let mut c = night(true);
    c.players[0].spectator = true;
    let mut e = Entities::new(21);
    for _ in 0..40 {
        e.update(0.05, &Plain, &c);
    }
    assert!(e.mobs.is_empty());
    c.players.push(Target::new(PlayerId(2), DVec3::new(1000.5, 64.0, 0.5), false));
    c.spawning = false;
    e.spawn(MobKind::Zombie, DVec3::new(0.5, 64.0, 0.5));
    e.update(0.05, &Plain, &c);
    assert!(e.mobs.is_empty(), "a nearby spectator cannot prevent distance despawning");
}

#[test]
fn saves_without_generation_markers_keep_their_herds_without_doubling_them() {
    let mut old = Entities::new(5);
    old.spawn(MobKind::Cow, DVec3::new(8.5, 64.0, 8.5));
    old.ensure_mob_ids();
    let mut root: serde_json::Value = serde_json::from_str(&old.nether_mobs_to_string()).unwrap();
    root.as_object_mut().unwrap().remove("generation");
    let mut e = Entities::new(5);
    e.load_nether_mobs(&root.to_string());
    let mut c = night(false);
    c.spawning = true;
    e.update(0.05, &Grass { generation: true }, &c);
    let near =
        |m: &&&voxelcraft::entity::Mob| (m.pos.x / 16.0).floor().abs() <= 2.0 && (m.pos.z / 16.0).floor().abs() <= 2.0;
    let creatures: Vec<_> = e.mobs.iter().filter(|m| m.kind.category() == MobCategory::Creature).collect();
    assert_eq!(creatures.iter().filter(near).count(), 1, "the legacy herd's columns stay populated");
    assert!(creatures.len() > 1, "unvisited columns still gain generation herds");
}

/// `Plain`, but only the strip of Java chunks with x = 2 is entity-ticking.
struct Bounded;
impl BlockSource for Bounded {
    fn block(&self, p: IVec3) -> Option<Block> {
        Plain.block(p)
    }
}
impl MobWorld for Bounded {
    fn loaded(&self, _: IVec3) -> bool {
        true
    }
    fn entity_ticking(&self, p: IVec3) -> bool {
        p.x >> 4 == 2
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

#[test]
fn pack_members_stay_in_entity_ticking_chunks() {
    let c = night(true);
    let mut e = Entities::new(8);
    let mut seen = std::collections::HashSet::new();
    for _ in 0..400 {
        e.update(0.05, &Bounded, &c);
        e.ensure_mob_ids();
        // Check where each mob first appears; it may wander across afterwards.
        for m in &e.mobs {
            if seen.insert(m.uid) {
                assert!((m.pos.x.floor() as i32) >> 4 == 2, "a pack walked past the simulation edge: {:?}", m.pos);
            }
        }
    }
    assert!(!seen.is_empty());
}

#[test]
fn generation_herds_ignore_do_mob_spawning_and_follow_the_chunk_seed() {
    let w = Grass { generation: true };
    let c = night(false);
    let herd = |seed| {
        let mut e = Entities::new(seed);
        e.update(0.05, &w, &c);
        let mut cells: Vec<_> = e
            .mobs
            .iter()
            .filter(|m| m.kind.category() == MobCategory::Creature)
            .map(|m| (format!("{:?}", m.kind), m.pos.floor().as_ivec3().to_array()))
            .collect();
        cells.sort();
        cells
    };
    let first = herd(1);
    assert!(!first.is_empty(), "chunk generation spawns herds with doMobSpawning off");
    assert_eq!(first, herd(99), "a column's herd depends on the world seed, not the session's random state");
}
