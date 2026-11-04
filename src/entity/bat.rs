//! Java 26.3 Bat. The pinned reference has no Halloween/calendar branch:
//! spawning uses WORLD_SURFACE, base_stone_overworld and raw brightness 0..3.
use super::{Ctx, Mob, MobWorld, Rng, monster_spawns};
use crate::{
    physics,
    world::{block::Block, terrain::Dimension},
};
use glam::{DVec3, IVec3};
#[derive(Clone, Debug)]
pub struct State {
    pub resting: bool,
    pub(super) wake: bool,
    target: Option<IVec3>,
    min_y: i32,
}
impl Default for State {
    fn default() -> Self {
        Self { resting: true, wake: false, target: None, min_y: Dimension::Overworld.min_y() }
    }
}
pub(super) fn spawn_ok<W: MobWorld + ?Sized>(
    world: &W,
    cell: IVec3,
    light: crate::simulation::lighting::SpawnLight,
    rng: &mut Rng,
) -> bool {
    let Some(top) = world.world_surface(cell.x, cell.z) else {
        return false;
    };
    if cell.y > top || rng.next_int(2) != 0 {
        return false;
    }
    let Some(sky) = world.sky_light_level(cell) else {
        return false;
    };
    if sky.saturating_sub(light.sky_darken).max(world.block_light(cell)) > rng.next_int(4) as u8 {
        return false;
    }
    let Some(floor) = world.block(cell - IVec3::Y) else {
        return false;
    };
    let stone = matches!(floor.name(), "stone" | "granite" | "diorite" | "andesite" | "deepslate" | "tuff");
    let pos = cell.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
    stone
        && monster_spawns::valid_spawn_floor(floor, super::MobKind::Bat)
        && world.block(cell).is_some_and(|b| monster_spawns::valid_empty_cell(b, super::MobKind::Bat))
        && world.block(cell + IVec3::Y).is_some_and(|b| monster_spawns::valid_empty_cell(b, super::MobKind::Bat))
        && !physics::overlaps_solid(world, pos, super::MobKind::Bat.shape())
        && !physics::touches_block(world, pos, super::MobKind::Bat.shape(), |b| b.is_fluid() || b.is_waterlogged())
}
impl Mob {
    pub(super) fn bat_sense<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx) {
        if let Some(s) = &mut self.bat {
            s.min_y = ctx.dimension.min_y();
            s.wake = ctx.players.iter().any(|p| {
                p.alive
                    && !p.spectator
                    && p.pos.distance_squared(self.pos) < 16.0
                    && super::mob::line_of_sight(
                        world,
                        self.pos + DVec3::Y * 0.45,
                        p.pos + DVec3::Y * (p.shape.height * 0.85),
                    )
            });
        }
    }
    /// One whole Java tick. Velocities in this engine are blocks/second;
    /// calculations below use Java's blocks/tick, then convert back.
    pub(super) fn bat_tick<W: MobWorld + ?Sized>(&mut self, world: &W, rng: &mut Rng) {
        let alive = self.alive();
        let shape = self.shape();
        let cell = self.pos.floor().as_ivec3();
        let ceiling = world.block(cell + IVec3::Y).is_some_and(monster_spawns::is_redstone_conductor);
        let Some(s) = &mut self.bat else {
            return;
        };
        let mut dm = self.vel * 0.05;
        if alive {
            if s.resting {
                if !ceiling || s.wake {
                    s.resting = false;
                } else if rng.next_int(200) == 0 {
                    self.head_yaw = rng.range(-std::f32::consts::PI, std::f32::consts::PI);
                }
            } else {
                if s.target.is_some_and(|p| world.block(p) != Some(Block::AIR) || p.y <= s.min_y) {
                    s.target = None;
                }
                if s.target.is_none()
                    || rng.next_int(30) == 0
                    || s.target.is_some_and(|p| (p.as_dvec3() + DVec3::splat(0.5)).distance_squared(self.pos) < 4.0)
                {
                    s.target = Some(
                        (self.pos
                            + DVec3::new(
                                (rng.next_int(7) as i32 - rng.next_int(7) as i32) as f64,
                                rng.next_int(6) as f64 - 2.0,
                                (rng.next_int(7) as i32 - rng.next_int(7) as i32) as f64,
                            ))
                        .floor()
                        .as_ivec3(),
                    );
                }
                let target = s.target.unwrap().as_dvec3() + DVec3::new(0.5, 0.1, 0.5);
                let d = target - self.pos;
                dm += (DVec3::new(sign(d.x) * 0.5, sign(d.y) * 0.7, sign(d.z) * 0.5) - dm) * 0.10000000149011612;
                self.yaw = (dm.z as f32).atan2(dm.x as f32);
                if rng.next_int(100) == 0 && ceiling {
                    s.resting = true;
                }
            }
        }
        let water = physics::touches_block(world, self.pos, shape, Block::holds_water);
        let lava = physics::touches_block(world, self.pos, shape, Block::is_lava);
        let resting = s.resting;
        let forward = if alive && !resting { 0.49 } else { 0.0 };
        if water || lava {
            dm += DVec3::new(self.yaw.cos() as f64, 0.0, self.yaw.sin() as f64) * (0.02 * forward);
            let delta = dm;
            let hit = physics::move_box(world, &mut self.pos, &mut dm, delta, shape);
            self.on_ground = hit.on_ground;
            self.blocked = hit.horizontal;
            dm *= if water { 0.8 } else { 0.5 };
            dm.y -= 0.02;
        } else {
            self.travel_in_air(world, &mut dm, DVec3::new(0.0, 0.0, forward));
        }
        // Bat.tick follows the superclass tick: hang at the ceiling, or
        // multiply only the vertical motion by .6. Bats never take fall damage.
        if resting {
            dm = DVec3::ZERO;
            self.pos.y = self.pos.y.floor() + 1.0 - shape.height;
        } else {
            dm.y *= 0.6;
        }
        self.vel = dm * 20.0;
    }
}
fn sign(n: f64) -> f64 {
    if n == 0.0 { 0.0 } else { n.signum() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        entity::{Entities, MobKind, PlayerId, Target, category::MobCategory},
        physics::{BlockSource, test_util::Grid},
        simulation::lighting::SpawnLight,
    };
    struct Cave {
        grid: Grid,
        light: u8,
    }
    impl BlockSource for Cave {
        fn block(&self, p: IVec3) -> Option<Block> {
            self.grid.block(p)
        }
    }
    impl MobWorld for Cave {
        fn loaded(&self, _: IVec3) -> bool {
            true
        }
        fn exposed(&self, _: IVec3) -> bool {
            false
        }
        fn surface(&self, _: i32, _: i32) -> Option<i32> {
            Some(80)
        }
        fn sky_light_level(&self, _: IVec3) -> Option<u8> {
            Some(0)
        }
        fn block_light(&self, _: IVec3) -> u8 {
            self.light
        }
    }
    #[test]
    fn spawn_predicate_uses_26_3_surface_tag_and_light_instead_of_calendar() {
        let mut w = Cave { grid: Grid::flat(70), light: 0 };
        let cell = IVec3::new(0, 70, 0);
        let mut rng = Rng::new(3);
        assert_eq!(MobKind::Bat.category(), MobCategory::Ambient);
        let dark = (0..10000).filter(|_| spawn_ok(&w, cell, SpawnLight::DAY, &mut rng)).count();
        assert!((4700..5300).contains(&dark), "half pass the boolean: {dark}");
        w.light = 3;
        let light = (0..10000).filter(|_| spawn_ok(&w, cell, SpawnLight::DAY, &mut rng)).count();
        assert!((1050..1450).contains(&light), "half * one quarter: {light}");
        w.light = 4;
        assert!(!(0..100).any(|_| spawn_ok(&w, cell, SpawnLight::NIGHT, &mut rng)));
        w.light = 0;
        w.grid.set(cell + IVec3::Y, Block::STONE);
        assert!(
            !(0..100).any(|_| spawn_ok(&w, cell, SpawnLight::DAY, &mut rng)),
            "ON_GROUND needs the head cell empty even for a short bat"
        );
        w.grid.set(cell + IVec3::Y, Block::AIR);
        w.grid.set(cell - IVec3::Y, Block::GRASS);
        assert!(!(0..100).any(|_| spawn_ok(&w, cell, SpawnLight::DAY, &mut rng)));
        w.grid.set(IVec3::new(0, 80, 0), Block::STONE);
        assert!(!spawn_ok(&w, IVec3::new(0, 81, 0), SpawnLight::NIGHT, &mut rng));
    }
    #[test]
    fn hanging_wakes_on_ceiling_loss_near_player_or_damage_and_flight_is_bounded() {
        let mut w = Cave { grid: Grid::flat(-64), light: 0 };
        w.grid.set(IVec3::new(0, 12, 0), Block::STONE);
        let mut m = Mob::new(MobKind::Bat, DVec3::new(0.5, 11.1, 0.5), 0.0);
        let mut rng = Rng::new(2);
        m.bat_tick(&w, &mut rng);
        assert!(m.bat.as_ref().unwrap().resting);
        assert_eq!(m.vel, DVec3::ZERO);
        assert!((m.pos.y - 11.1).abs() < 1e-9);
        let c = Ctx {
            players: vec![Target::new(PlayerId::HOST, DVec3::new(2.0, 10.0, 0.5), false)],
            daylight: 0.0,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
            spawn_light: SpawnLight::NIGHT,
        };
        m.bat_sense(&w, &c);
        m.bat_tick(&w, &mut rng);
        assert!(!m.bat.as_ref().unwrap().resting);
        m.bat.as_mut().unwrap().resting = true;
        m.damage(1.0, None, &mut rng);
        assert!(!m.bat.as_ref().unwrap().resting);
        m.bat.as_mut().unwrap().resting = true;
        w.grid.set(IVec3::new(0, 12, 0), Block::AIR);
        m.bat_tick(&w, &mut rng);
        assert!(!m.bat.as_ref().unwrap().resting);
        let start = m.pos;
        for _ in 0..200 {
            m.bat_tick(&w, &mut rng);
        }
        assert!(m.pos.is_finite() && m.pos.distance(start) > 1.0);
        assert!(m.vel.length() < 20.0, "bounded sign-steering flight: {:?}", m.vel);
        assert_eq!(m.health, 5.0, "falling never hurts a bat");
    }
    #[test]
    fn natural_bats_save_resting_state_and_have_no_loot_or_xp() {
        let mut e = Entities::new(4);
        e.spawn(MobKind::Bat, DVec3::new(0.5, 20.0, 0.5));
        e.mobs[0].persistent = false;
        e.mobs[0].bat.as_mut().unwrap().resting = false;
        let mut loaded = Entities::new(5);
        loaded.load_nether_mobs(&e.nether_mobs_to_string());
        assert_eq!(loaded.mobs.len(), 1);
        assert!(!loaded.mobs[0].bat.as_ref().unwrap().resting);
        assert!(!loaded.mobs[0].persistent);
        let mut remote = Mob::new(MobKind::Bat, DVec3::ZERO, 0.0);
        remote.apply_look(&loaded.mobs[0].look());
        assert!(!remote.bat.unwrap().resting);
        e.drop_loot(MobKind::Bat, DVec3::ZERO);
        assert!(e.items.is_empty() && e.orbs.is_empty());
    }
}
