//! Java 26.3 navigation through the real mob/entity tick, without a renderer.
use std::collections::HashMap;

use glam::{DVec3, IVec3};
use voxelcraft::{
    entity::{Ctx, Entities, MobKind, MobWorld, PlayerId, Target},
    physics::BlockSource,
    simulation::lighting::SpawnLight,
    world::{block::Block, terrain::Dimension},
};

#[derive(Default)]
struct Scene {
    blocks: HashMap<IVec3, Block>,
    water: bool,
    edits: Vec<IVec3>,
}
impl Scene {
    fn set(&mut self, p: IVec3, b: Block) {
        self.blocks.insert(p, b);
        self.edits.push(p);
    }
}
impl BlockSource for Scene {
    fn block(&self, p: IVec3) -> Option<Block> {
        Some(self.blocks.get(&p).copied().unwrap_or(if p.y < 1 {
            Block::STONE
        } else if self.water && p.y < 9 {
            Block::WATER
        } else {
            Block::AIR
        }))
    }
}
impl MobWorld for Scene {
    fn loaded(&self, _: IVec3) -> bool {
        true
    }
    fn surface(&self, _: i32, _: i32) -> Option<i32> {
        Some(0)
    }
    fn exposed(&self, _: IVec3) -> bool {
        false
    }
    fn navigation_updates(&self, since: u64, visit: &mut dyn FnMut(IVec3)) -> (u64, bool) {
        for &p in self.edits.iter().skip(since as usize) {
            visit(p);
        }
        (self.edits.len() as u64, false)
    }
}
fn night(at: DVec3) -> Ctx {
    Ctx {
        spawn_light: SpawnLight::NIGHT,
        players: vec![Target::new(PlayerId::HOST, at, true)],
        daylight: 0.0,
        spawning: false,
        raining: false,
        dimension: Dimension::Overworld,
    }
}

#[test]
fn drowned_swim_up_to_a_player_instead_of_walking_the_bottom() {
    let scene = Scene { water: true, ..Scene::default() };
    let target = DVec3::new(5.5, 6.0, 0.5);
    let mut entities = Entities::new(73);
    entities.spawn(MobKind::Drowned, DVec3::new(0.5, 2.0, 0.5));
    let mut hit = false;
    for _ in 0..300 {
        hit |= entities
            .update(0.05, &scene, &night(target))
            .iter()
            .any(|e| matches!(e, voxelcraft::entity::EntityEvent::PlayerHit { .. }));
    }
    assert!(hit, "drowned did not reach elevated underwater target: {:?}", entities.mobs[0].pos);
    assert!(entities.mobs[0].pos.y > 4.0);
    // Killed mid-chase, the body sinks instead of hanging in the water.
    let y = entities.mobs[0].pos.y;
    entities.mobs[0].dying = Some(0.0);
    for _ in 0..10 {
        entities.update(0.05, &scene, &night(target));
    }
    assert!(entities.mobs[0].pos.y < y - 0.1, "corpse hung at {:?}", entities.mobs[0].pos);
}

#[test]
fn following_parrot_flies_round_a_wall_and_across_a_gap() {
    let mut scene = Scene::default();
    for x in 1..=8 {
        for y in -8..=0 {
            for z in -8..=8 {
                scene.set(IVec3::new(x, y, z), Block::AIR);
            }
        }
    }
    for y in 1..=7 {
        for z in -2..=2 {
            scene.set(IVec3::new(4, y, z), Block::STONE);
        }
    }
    let target = DVec3::new(9.5, 3.0, 0.5);
    let ctx = night(target);
    let mut e = Entities::new(17);
    e.spawn(MobKind::Parrot, DVec3::new(0.5, 4.0, 0.5));
    e.mobs[0].animal.as_mut().unwrap().owner = Some(PlayerId::HOST);
    let mut went_around = false;
    let mut reached = false;
    for _ in 0..600 {
        e.update(0.05, &scene, &ctx);
        let p = e.mobs[0].pos;
        went_around |= p.z.abs() > 2.5 || p.y >= 8.0;
        reached |= p.x > 8.0;
        if reached {
            break;
        }
        assert!(p.y > 0.0, "parrot fell through the gap: {p:?}");
    }
    assert!(went_around && reached, "parrot failed wall/gap route: {:?}", e.mobs[0].pos);
}

#[test]
fn spider_climbs_a_three_high_wall_to_its_unreachable_target() {
    let mut scene = Scene::default();
    // The platform extends well beyond the walking search; climbing is required.
    for x in 3..=12 {
        for z in -30..=30 {
            for y in 1..=3 {
                scene.set(IVec3::new(x, y, z), Block::STONE);
            }
        }
    }
    let ctx = night(DVec3::new(7.5, 4.0, 0.5));
    let mut e = Entities::new(19);
    e.spawn(MobKind::Spider, DVec3::new(0.5, 1.0, 0.5));
    let mut climbed = false;
    for _ in 0..400 {
        e.update(0.05, &scene, &ctx);
        climbed |= e.mobs[0].pos.y >= 3.9 && e.mobs[0].pos.x > 3.0;
    }
    assert!(climbed, "spider did not climb: {:?}", e.mobs[0].pos);
}

#[test]
fn a_new_wall_on_an_active_path_is_recomputed_before_the_stuck_timeout() {
    let mut scene = Scene::default();
    let target = DVec3::new(12.5, 1.0, 0.5);
    let ctx = night(target);
    let mut e = Entities::new(23);
    e.spawn(MobKind::Zombie, DVec3::new(0.5, 1.0, 0.5));
    for _ in 0..25 {
        e.update(0.05, &scene, &ctx);
    }
    assert!(e.mobs[0].pos.x < 5.0);
    for y in 1..=3 {
        for z in -1..=1 {
            scene.set(IVec3::new(5, y, z), Block::STONE);
        }
    }
    let mut went_around = false;
    for _ in 0..70 {
        e.update(0.05, &scene, &ctx);
        went_around |= e.mobs[0].pos.z.abs() > 1.5;
    }
    assert!(went_around, "new wall did not trigger a detour: {:?}", e.mobs[0].pos);
}

#[test]
fn idle_drowned_sink_to_the_bottom_instead_of_hovering() {
    let scene = Scene { water: true, ..Scene::default() };
    // A distant creative player: nothing to chase.
    let mut ctx = night(DVec3::new(500.5, 6.0, 0.5));
    ctx.players[0].targetable = false;
    let mut entities = Entities::new(5);
    entities.spawn(MobKind::Drowned, DVec3::new(0.5, 6.0, 0.5));
    entities.mobs[0].persistent = true;
    // Java sinks at most 0.02 blocks a tick, so five blocks take over 250 ticks.
    for _ in 0..400 {
        entities.update(0.05, &scene, &ctx);
    }
    assert!(entities.mobs[0].pos.y < 2.0, "idle drowned hovered at {:?}", entities.mobs[0].pos);
}

#[test]
fn drowned_swim_up_after_villagers() {
    let scene = Scene { water: true, ..Scene::default() };
    let mut ctx = night(DVec3::new(500.5, 6.0, 0.5));
    ctx.players[0].targetable = false;
    let mut entities = Entities::new(9);
    entities.spawn(MobKind::Drowned, DVec3::new(0.5, 1.0, 0.5));
    entities.spawn(MobKind::Villager, DVec3::new(4.5, 7.0, 0.5));
    entities.mobs[0].persistent = true;
    let mut peak = 0.0f64;
    for _ in 0..200 {
        entities.update(0.05, &scene, &ctx);
        assert_eq!(entities.mobs[0].kind, MobKind::Drowned);
        peak = peak.max(entities.mobs[0].pos.y);
    }
    assert!(peak > 4.0, "drowned hunting a villager stayed on the bottom: {peak}");
}

/// Representative Nether check (run with `--ignored --nocapture`): forty
/// striders on generated lava seas, half of them put ashore beside the
/// lava, for 30 seconds. Reports how many stand in lava at the end and the
/// entity tick cost of their lava paths.
#[test]
#[ignore]
fn striders_walk_lava_seas_in_generated_nether() {
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use voxelcraft::world::{
        World,
        terrain::{Dimension, Generator},
    };
    let seed: u64 = std::env::var("NAV_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(7);
    let generator = Arc::new(Generator::for_dimension(seed, Dimension::Nether));
    let mut world = World::new_headless(generator, Default::default(), 2);
    let at = DVec3::new(0.5, 40.0, 0.5);
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        world.update(at);
        if world.loaded_chunks() > 0 && world.is_idle() {
            break;
        }
        assert!(Instant::now() < deadline, "streaming did not finish");
        std::thread::sleep(Duration::from_millis(1));
    }
    let sea = voxelcraft::world::nether::LAVA_SEA;
    let lava = |p: IVec3| world.block(p).is_some_and(|b| b.is_lava());
    let open = |p: IVec3| world.block(p) == Some(Block::AIR) && world.block(p + IVec3::Y) == Some(Block::AIR);
    let mut on_sea = Vec::new();
    let mut ashore = Vec::new();
    for r in 0..40 {
        for d in -r..=r {
            for (x, z) in [(d, r), (d, -r), (r, d), (-r, d)] {
                let top = IVec3::new(x, sea, z);
                if lava(top) && open(top + IVec3::Y) && on_sea.len() < 20 {
                    on_sea.push(DVec3::new(x as f64 + 0.5, sea as f64 + 0.5, z as f64 + 0.5));
                }
                // Dry ground level with the lava's surface, next to lava.
                let ground = IVec3::new(x, sea, z);
                if ashore.len() < 20
                    && world.block(ground).is_some_and(|b| b.is_solid() && !b.is_fluid())
                    && open(ground + IVec3::Y)
                    && [IVec3::X, -IVec3::X, IVec3::Z, -IVec3::Z].iter().any(|&s| lava(ground + s))
                {
                    ashore.push(DVec3::new(x as f64 + 0.5, sea as f64 + 1.0, z as f64 + 0.5));
                }
            }
        }
    }
    let player = DVec3::new(0.5, sea as f64 + 40.0, 0.5);
    let ctx = Ctx {
        spawn_light: SpawnLight::NIGHT,
        players: vec![Target::new(PlayerId::HOST, player, true)],
        daylight: 0.0,
        spawning: false,
        raining: false,
        dimension: Dimension::Nether,
    };
    let mut e = Entities::new(seed);
    for &p in on_sea.iter().chain(&ashore) {
        e.spawn(MobKind::Strider, p);
    }
    let mut times = Vec::new();
    for _ in 0..20 * 30 {
        let t = Instant::now();
        e.update(0.05, &world, &ctx);
        times.push(t.elapsed().as_secs_f64() * 1e3);
    }
    times.sort_by(f64::total_cmp);
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    // Feet in the top lava cell (Java's stable height), or a block above it
    // on the old full-block surface.
    let warm = |p: DVec3| lava(p.floor().as_ivec3()) || lava((p - DVec3::Y * 0.6).floor().as_ivec3());
    let in_lava = e.mobs.iter().filter(|m| m.kind == MobKind::Strider && warm(m.pos)).count();
    println!(
        "seed {seed}: {} on the sea + {} ashore; {in_lava}/{} in lava after 30 s; entity tick mean {mean:.3} ms, p99 {:.3} ms, max {:.3} ms",
        on_sea.len(),
        ashore.len(),
        e.mobs.len(),
        times[times.len() * 99 / 100],
        times.last().unwrap()
    );
}
