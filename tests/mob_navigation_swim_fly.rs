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
