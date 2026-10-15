//! Mobs walk Java 26.3 ground paths (`WalkNodeEvaluator` + `PathFinder`)
//! through the shared entity update: a zombie works round a wall it can't
//! jump to reach a player, keeps off a lava moat's edge, and wandering cows
//! and a following wolf don't take a drop they can't survive.
use std::collections::HashMap;

use glam::{DVec3, IVec3};
use voxelcraft::{
    entity::{Ctx, Entities, MobKind, MobWorld, PlayerId, Target},
    physics::BlockSource,
    simulation::lighting::SpawnLight,
    world::{block::Block, terrain::Dimension},
};

/// Stone below y = 1, air above, with explicit overrides.
#[derive(Default)]
struct Scene(HashMap<IVec3, Block>);

impl Scene {
    fn set(&mut self, p: IVec3, b: Block) {
        self.0.insert(p, b);
    }
}

impl BlockSource for Scene {
    fn block(&self, p: IVec3) -> Option<Block> {
        Some(self.0.get(&p).copied().unwrap_or(if p.y < 1 { Block::STONE } else { Block::AIR }))
    }
}

impl MobWorld for Scene {
    fn loaded(&self, _: IVec3) -> bool {
        true
    }
    fn surface(&self, _: i32, _: i32) -> Option<i32> {
        Some(0)
    }
    /// Roofed: no sunburn, so undead chase by "day" without catching fire.
    fn exposed(&self, _: IVec3) -> bool {
        false
    }
}

fn night(player: DVec3) -> Ctx {
    Ctx {
        spawn_light: SpawnLight::NIGHT,
        players: vec![Target::new(PlayerId::HOST, player, true)],
        daylight: 0.0,
        spawning: false,
        raining: false,
        dimension: Dimension::Overworld,
    }
}

fn hits(events: &[voxelcraft::entity::EntityEvent]) -> usize {
    events.iter().filter(|e| matches!(e, voxelcraft::entity::EntityEvent::PlayerHit { .. })).count()
}

#[test]
fn a_zombie_walks_round_a_wall_to_reach_the_player() {
    let mut scene = Scene::default();
    // A three-high wall from z = -6 to 6 between the zombie and the player.
    for z in -6..=6 {
        for y in 1..=3 {
            scene.set(IVec3::new(4, y, z), Block::STONE);
        }
    }
    let player = DVec3::new(8.5, 1.0, 0.5);
    let ctx = night(player);
    let mut e = Entities::new(7);
    e.spawn(MobKind::Zombie, DVec3::new(0.5, 1.0, 0.5));
    let mut reached = None;
    for tick in 0..20 * 30 {
        let events = e.update(0.05, &scene, &ctx);
        if hits(&events) > 0 {
            reached = Some(tick);
            break;
        }
    }
    let zombie = &e.mobs[0];
    assert!(reached.is_some(), "the zombie never reached the player; it ended at {:?}", zombie.pos);
    // It went round the end of the wall rather than through it.
    assert!(zombie.pos.x > 4.0, "{:?}", zombie.pos);
}

#[test]
fn a_zombie_does_not_walk_into_lava_to_reach_the_player() {
    let mut scene = Scene::default();
    // A lava moat (with a stone rim below) all round the player.
    for x in -6i32..=6 {
        for z in -6i32..=6 {
            let ring = x.abs().max(z.abs());
            if (3..=4).contains(&ring) {
                scene.set(IVec3::new(x + 20, 0, z), Block::LAVA);
            }
        }
    }
    let ctx = night(DVec3::new(20.5, 1.0, 0.5));
    let mut e = Entities::new(9);
    e.spawn(MobKind::Zombie, DVec3::new(10.5, 1.0, 0.5));
    for _ in 0..20 * 20 {
        e.update(0.05, &scene, &ctx);
        let z = &e.mobs[0];
        let cell = z.pos.floor().as_ivec3();
        assert!(
            !scene.block(cell - IVec3::Y).is_some_and(|b| b.is_lava()) && !z.burning,
            "the zombie stepped into the lava at {:?}",
            z.pos
        );
    }
    // It waits at the closest safe spot: the moat's outer edge.
    let z = e.mobs[0].pos;
    assert!((z.x - 20.5).abs() > 4.0 || (z.z - 0.5).abs() > 4.0, "{z:?}");
}

#[test]
fn wandering_cows_stay_off_a_deadly_drop() {
    let mut scene = Scene::default();
    // A plateau at y = 1 for x < 6; beyond it a 12-block drop.
    for x in 6..30 {
        for z in -30..30 {
            for y in -12..1 {
                scene.set(IVec3::new(x, y, z), Block::AIR);
            }
        }
    }
    let mut ctx = night(DVec3::new(0.5, 1.0, 0.5));
    ctx.players[0].targetable = false;
    ctx.daylight = 1.0;
    ctx.spawn_light = SpawnLight::DAY;
    let mut e = Entities::new(11);
    for z in [-4, 0, 4] {
        e.spawn(MobKind::Cow, DVec3::new(3.5, 1.0, z as f64 + 0.5));
    }
    for _ in 0..20 * 120 {
        e.update(0.05, &scene, &ctx);
    }
    for cow in &e.mobs {
        assert!(cow.pos.y > 0.5 && cow.pos.x < 6.5, "a cow walked off the cliff: {:?}", cow.pos);
    }
    assert!(e.mobs.iter().any(|c| c.pos.distance(DVec3::new(3.5, 1.0, c.pos.z)) > 0.5), "cows stroll about");
}

#[test]
fn a_following_wolf_keeps_the_idle_fall_limit() {
    let mut scene = Scene::default();
    // A plateau at y = 1 for x < 6; beyond it a 7-block drop to the owner.
    for x in 6..30 {
        for z in -30..30 {
            for y in -6..1 {
                scene.set(IVec3::new(x, y, z), Block::AIR);
            }
        }
    }
    // Close enough that the wolf walks rather than teleporting to its owner.
    let mut ctx = night(DVec3::new(9.5, -6.0, 0.5));
    ctx.players[0].targetable = false;
    let mut e = Entities::new(13);
    e.spawn(MobKind::Wolf, DVec3::new(3.5, 1.0, 0.5));
    e.mobs[0].animal.as_mut().unwrap().owner = Some(PlayerId::HOST);
    // A tamed wolf's 40 health: with a target it could risk a 25-block fall.
    e.mobs[0].health = 40.0;
    for _ in 0..20 * 10 {
        e.update(0.05, &scene, &ctx);
        let wolf = &e.mobs[0];
        // `getMaxFallDistance` is 3 without an attack target, however much
        // health the wolf could spare.
        assert!(wolf.pos.y > 0.5, "the wolf jumped down to its owner: {:?}", wolf.pos);
    }
}

/// Representative-terrain check (run with `--ignored --nocapture`): thirty
/// zombies chase a still player across generated Overworld terrain for 30
/// seconds. Reports how many reach the player and the entity tick cost.
#[test]
#[ignore]
fn zombies_chase_across_generated_terrain() {
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use voxelcraft::world::{World, terrain::Generator};
    let seed: u64 = std::env::var("NAV_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(7);
    let mut world = World::new_headless(Arc::new(Generator::new(seed)), Default::default(), 2);
    let at = DVec3::new(0.5, 100.0, 0.5);
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        world.update(at);
        if world.loaded_chunks() > 0 && world.is_idle() {
            break;
        }
        assert!(Instant::now() < deadline, "streaming did not finish");
        std::thread::sleep(Duration::from_millis(1));
    }
    // Open ground only: not tree tops, water or lava.
    // The forest floor under any canopy; not water or lava.
    let ground = |x: i32, z: i32| {
        let mut y = world.surface(x, z)?;
        while world
            .block(IVec3::new(x, y, z))
            .is_some_and(|b| b.is_leaves() || b.is_log() || !b.is_solid() && !b.is_fluid())
        {
            y -= 1;
        }
        let top = world.block(IVec3::new(x, y, z))?;
        (top.is_solid() && !top.is_fluid()).then(|| DVec3::new(x as f64 + 0.5, y as f64 + 1.0, z as f64 + 0.5))
    };
    let Some(player) = (0..48).flat_map(|r| (-r..=r).map(move |d| (d, r))).find_map(|(d, r)| ground(d, r)) else {
        println!("seed {seed}: no dry ground near the origin");
        return;
    };
    let ctx = night(player);
    let mut e = Entities::new(seed);
    for i in 0..30 {
        let a = i as f64 * std::f64::consts::TAU / 30.0;
        let r = 12.0 + (i % 3) as f64 * 4.0;
        let (x, z) = (player.x as i32 + (a.cos() * r) as i32, player.z as i32 + (a.sin() * r) as i32);
        if let Some(p) = ground(x, z) {
            e.spawn(MobKind::Zombie, p);
        }
    }
    let spawned = e.mobs.len();
    let mut times = Vec::new();
    let mut reached = std::collections::HashSet::new();
    for _ in 0..20 * 30 {
        let t = Instant::now();
        e.update(0.05, &world, &ctx);
        times.push(t.elapsed().as_secs_f64() * 1e3);
        for (i, m) in e.mobs.iter().enumerate() {
            if m.pos.distance(player) < 2.5 {
                reached.insert(i);
            }
        }
    }
    times.sort_by(f64::total_cmp);
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    println!(
        "seed {seed}: {}/{spawned} zombies reached the player; entity tick mean {mean:.3} ms, p99 {:.3} ms, max {:.3} ms",
        reached.len(),
        times[times.len() * 99 / 100],
        times.last().unwrap()
    );
}
