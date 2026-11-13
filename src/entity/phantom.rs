//! PhantomSpawner and phantom flight, based on the pinned 26.3 classes.
use super::{Ctx, Entities, EntityEvent, Mob, MobKind, MobWorld, PlayerId, Rng};
use crate::{
    physics,
    world::terrain::{Dimension, SEA_LEVEL},
};
use glam::DVec3;
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub struct State {
    pub size: u8,
    pub anchor: DVec3,
    pub swooping: bool,
    pub cat_near: bool,
    target: Option<PlayerId>,
    next_sweep: i32,
    scan: i32,
    radius: f64,
    height: f64,
    angle: f64,
    clockwise: f64,
    point: DVec3,
    speed: f64,
    clock: f64,
    collision: bool,
}
impl State {
    pub fn new(pos: DVec3) -> Self {
        Self {
            size: 0,
            anchor: pos.floor() + DVec3::Y * 5.,
            swooping: false,
            cat_near: false,
            target: None,
            next_sweep: 10,
            scan: 20,
            radius: 10.,
            height: 0.,
            angle: 0.,
            clockwise: 1.,
            point: pos,
            speed: 0.1,
            clock: 0.,
            collision: false,
        }
    }
    pub fn save(&self) -> Value {
        json!({"size":self.size,"anchor":self.anchor.to_array()})
    }
    pub fn load(&mut self, v: &Value) {
        self.size = v["size"].as_u64().unwrap_or(0).min(64) as u8;
        if let Some(a) = v["anchor"].as_array().filter(|a| a.len() == 3) {
            let p = DVec3::new(a[0].as_f64().unwrap_or(0.), a[1].as_f64().unwrap_or(0.), a[2].as_f64().unwrap_or(0.));
            if p.is_finite() {
                self.anchor = p;
            }
        }
    }
    pub(super) fn start_circle(&mut self, rng: &mut Rng) {
        self.radius = rng.range(5., 15.) as f64;
        self.height = rng.range(-4., 5.) as f64;
        self.clockwise = if rng.chance(0.5) { 1. } else { -1. };
        self.next_point();
    }
    fn next_point(&mut self) {
        self.angle += self.clockwise * 15f64.to_radians();
        self.point =
            self.anchor + DVec3::new(self.radius * self.angle.cos(), -4. + self.height, self.radius * self.angle.sin());
    }
}
impl Entities {
    pub(super) fn spawn_phantoms<W: MobWorld + ?Sized>(&mut self, dt: f64, w: &W, c: &Ctx) {
        if !c.spawning || !self.monster_spawning || !self.phantom_spawning || c.dimension != Dimension::Overworld {
            return;
        }
        self.phantom_clock += dt * 20.;
        while self.phantom_clock >= 1. {
            self.phantom_clock -= 1.;
            self.phantom_next -= 1;
            if self.phantom_next > 0 {
                continue;
            }
            self.phantom_next += (60 + self.rng.next_int(60) as i32) * 20;
            if c.spawn_light.sky_darken < 5 {
                continue;
            }
            for t in c.players.iter().filter(|t| !t.spectator) {
                let cell = t.pos.floor().as_ivec3();
                // `Level.canSeeSky`: above the WORLD_SURFACE heightmap, so glass
                // roofs block phantoms although they pass full skylight.
                if cell.y < SEA_LEVEL || !w.world_surface(cell.x, cell.z).is_some_and(|top| cell.y > top) {
                    continue;
                }
                let local = self.difficulty.effective(self.clock_time, 0, self.moon_brightness);
                if local <= self.rng.next_f32() * 3.
                    || self.rng.next_int(t.time_since_rest.clamp(1, i32::MAX as u32)) < 72000
                {
                    continue;
                }
                let p = cell
                    + glam::IVec3::new(
                        self.rng.next_int(21) as i32 - 10,
                        20 + self.rng.next_int(15) as i32,
                        self.rng.next_int(21) as i32 - 10,
                    );
                let Some(b) = w.block(p) else {
                    continue;
                };
                if !super::monster_spawns::valid_empty_cell(b, MobKind::Phantom) {
                    continue;
                }
                let count = 1 + self.rng.next_int(u32::from(self.difficulty.id()) + 1);
                for _ in 0..count {
                    self.spawn(MobKind::Phantom, p.as_dvec3() + DVec3::new(0.5, 0., 0.5));
                }
            }
        }
    }
    pub(super) fn sense_phantoms(&mut self) {
        let cats: Vec<_> = self.mobs.iter().filter(|m| m.kind == MobKind::Cat && m.alive()).map(|m| m.pos).collect();
        for m in &mut self.mobs {
            if let Some(s) = &mut m.phantom {
                s.cat_near = cats.iter().any(|p| (*p - m.pos).abs().max_element() < 16.);
            }
        }
    }
}
impl Mob {
    pub(super) fn phantom_flight<W: MobWorld + ?Sized>(
        &mut self,
        dt: f64,
        w: &W,
        c: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) {
        let mut s = self.phantom.take().unwrap();
        s.clock += dt;
        while s.clock >= 0.05 - 1e-9 {
            s.clock -= 0.05;
            if !self.alive() {
                break;
            }
            let mut target = s.target.and_then(|id| c.players.iter().find(|t| t.id == id && t.alive && t.targetable));
            s.scan -= 1;
            if target.is_none() && s.scan <= 0 {
                s.scan = 60;
                target = c
                    .players
                    .iter()
                    .filter(|t| {
                        t.alive
                            && t.targetable
                            && (t.pos - self.pos).with_y(0.).abs().max_element() < 16.
                            && (t.pos.y - self.pos.y).abs() < 64.
                            && t.pos.distance_squared(self.pos) < t.detection_range(64.).powi(2)
                    })
                    .max_by(|a, b| a.pos.y.total_cmp(&b.pos.y));
                s.target = target.map(|t| t.id);
                if let Some(t) = target {
                    s.anchor = t.pos.floor() + DVec3::Y * (20. + rng.next_int(20) as f64);
                    s.anchor.y = s.anchor.y.max(f64::from(SEA_LEVEL + 1));
                    s.next_sweep = 10;
                    s.swooping = false;
                    s.radius = rng.range(5., 15.) as f64;
                    s.height = rng.range(-4., 5.) as f64;
                    s.clockwise = if rng.chance(0.5) { 1. } else { -1. };
                    s.next_point();
                }
            }
            if target.is_none() {
                s.target = None;
                s.swooping = false;
            }
            if target.is_some() && !s.swooping {
                s.next_sweep -= 1;
                if s.next_sweep <= 0 {
                    s.swooping = true;
                    s.next_sweep = (8 + rng.next_int(4) as i32) * 20;
                }
            }
            if s.swooping && (s.cat_near || s.collision || self.hurt > 0.) {
                s.swooping = false;
                s.target = None;
                target = None;
            }
            if let Some(t) = target.filter(|_| s.swooping) {
                s.point = t.pos + DVec3::Y * t.shape.height * 0.5;
                let scale = 1. + f64::from(s.size) * 0.15;
                let (a, b) = physics::Shape::new(0.45 * scale, 0.5 * scale).aabb(self.pos);
                let (p, q) = t.shape.aabb(t.pos);
                if (b + DVec3::splat(0.2)).cmpgt(p).all() && (a - DVec3::splat(0.2)).cmplt(q).all() {
                    events.push(EntityEvent::PlayerHit {
                        java: None,
                        player: t.id,
                        damage: (6. + f32::from(s.size) + self.effects.attack_bonus()).max(0.),
                        knockback: (t.pos - self.pos).normalize_or_zero().as_vec3() * 4.,
                        cause: crate::simulation::damage::Cause::mob("was slain by a phantom"),
                    });
                    events.push(EntityEvent::OwnerHurt { player: t.id, attacker: self.uid });
                    s.swooping = false;
                }
            } else {
                if rng.next_int(350) == 0 {
                    s.height = rng.range(-4., 5.) as f64;
                }
                if rng.next_int(250) == 0 {
                    s.radius += 1.;
                    if s.radius > 15. {
                        s.radius = 5.;
                        s.clockwise = -s.clockwise;
                    }
                }
                if rng.next_int(450) == 0 {
                    s.angle = rng.range(0., std::f32::consts::TAU) as f64;
                    s.next_point();
                }
                if self.pos.distance_squared(s.point) < 4. || s.collision {
                    s.next_point();
                }
                if s.point.y < self.pos.y
                    && w.block(self.pos.floor().as_ivec3() - glam::IVec3::Y)
                        .is_some_and(|b| b != crate::world::block::Block::AIR)
                {
                    s.height = s.height.max(1.);
                    s.next_point();
                }
                if s.point.y > self.pos.y
                    && w.block(self.pos.floor().as_ivec3() + glam::IVec3::Y)
                        .is_some_and(|b| b != crate::world::block::Block::AIR)
                {
                    s.height = s.height.min(-1.);
                    s.next_point();
                }
            }
            // PhantomMoveControl: four degrees per tick, speed approaches 1.8
            // while aligned and .2 while turning; 20% velocity interpolation.
            if s.collision {
                self.yaw += std::f32::consts::PI;
                s.speed = 0.1;
            }
            let mut d = s.point - self.pos;
            let horizontal = d.with_y(0.).length();
            if horizontal > 1e-5 {
                let f = 1. - (d.y * 0.7).abs() / horizontal;
                d.x *= f;
                d.z *= f;
                let length = d.length();
                let desired = d.z.atan2(d.x) as f32;
                let delta = super::mob::wrap(desired - self.yaw);
                self.yaw += delta.clamp(-4f32.to_radians(), 4f32.to_radians());
                let aligned = delta.abs() < 3f32.to_radians();
                let speed = if aligned { 1.8 } else { 0.2 };
                let step = if aligned { 0.005 * 1.8 / s.speed } else { 0.025 };
                s.speed += (speed - s.speed).clamp(-step, step);
                self.head_pitch = (d.y.atan2(horizontal)) as f32;
                let wanted = DVec3::new(
                    f64::from(self.yaw.cos()) * (d.x / length).abs(),
                    self.head_pitch.sin() as f64 * (d.y / length).abs(),
                    f64::from(self.yaw.sin()) * (d.z / length).abs(),
                ) * s.speed;
                let mut vel = self.vel / 20.;
                vel += (wanted - vel) * 0.2;
                let shape = self.kind.shape();
                let shape = physics::Shape::new(
                    shape.half_width * (1. + f64::from(s.size) * 0.15),
                    shape.height * (1. + f64::from(s.size) * 0.15),
                );
                let delta = vel;
                s.collision = physics::move_box(w, &mut self.pos, &mut vel, delta, shape).horizontal;
                self.vel = vel * 0.91 * 20.;
                self.on_ground = false;
            }
        }
        self.phantom = Some(s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        entity::Target,
        physics::test_util::Grid,
        session::PlayerSession,
        simulation::{difficulty::Difficulty, survival::Env},
    };
    fn context(rest: u32) -> Ctx {
        let mut t = Target::new(PlayerId(7), DVec3::new(0., 80., 0.), true);
        t.time_since_rest = rest;
        Ctx {
            players: vec![t],
            daylight: 0.,
            raining: false,
            spawning: true,
            dimension: Dimension::Overworld,
            spawn_light: crate::simulation::lighting::SpawnLight::NIGHT,
        }
    }
    #[test]
    fn phantom_spawner_obeys_rule_rest_threshold_and_reference_group_sizes() {
        let w = Grid::flat(64);
        let mut e = Entities::new(17);
        e.difficulty = Difficulty::Hard;
        e.clock_time = 2_000_000;
        for rest in [0, 1, 72000] {
            for _ in 0..100 {
                e.phantom_next = 0;
                e.spawn_phantoms(0.05, &w, &context(rest));
                assert!(e.mobs.is_empty());
            }
        }
        let mut max = 0;
        for _ in 0..100 {
            e.phantom_next = 0;
            e.spawn_phantoms(0.05, &w, &context(i32::MAX as u32));
            max = max.max(e.mobs.len());
            assert!(e.mobs.len() <= 4);
            for m in &e.mobs {
                assert!((100.0..=114.0).contains(&m.pos.y));
                assert_eq!(m.kind, MobKind::Phantom);
            }
            e.mobs.clear();
        }
        assert_eq!(max, 4);
        // `canSeeSky` uses WORLD_SURFACE: a glass roof passes skylight but
        // still keeps phantoms away.
        let mut roofed = Grid::flat(64);
        roofed.set(glam::IVec3::new(0, 90, 0), crate::world::block::Block::GLASS);
        for _ in 0..100 {
            e.phantom_next = 0;
            e.spawn_phantoms(0.05, &roofed, &context(i32::MAX as u32));
        }
        assert!(e.mobs.is_empty(), "no phantoms under a glass roof");
        e.phantom_spawning = false;
        e.phantom_next = 300;
        for _ in 0..40 {
            e.spawn_phantoms(0.05, &w, &context(i32::MAX as u32));
        }
        assert_eq!(e.phantom_next, 300);
        assert!(e.mobs.is_empty());
    }
    #[test]
    fn rest_clock_advances_per_tick_and_sleep_resets_it() {
        let mut s = PlayerSession::new(PlayerId(8), DVec3::ZERO);
        for _ in 0..60 {
            s.vitals.tick(0.05, &Env::default(), true);
        }
        assert_eq!(s.vitals.time_since_rest, 60);
        s.begin_sleep(None, DVec3::ZERO);
        assert_eq!(s.vitals.time_since_rest, 0);
    }
    #[test]
    fn phantom_size_anchor_and_membrane_drop_survive_the_shared_paths() {
        let mut e = Entities::new(8);
        e.spawn(MobKind::Phantom, DVec3::new(0., 100., 0.));
        let p = e.mobs[0].phantom.as_mut().unwrap();
        p.size = 7;
        p.anchor = DVec3::new(4., 120., 8.);
        let json = e.nether_mobs_to_string();
        let mut copy = Entities::new(1);
        copy.load_nether_mobs(&json);
        let p = copy.mobs[0].phantom.as_ref().unwrap();
        assert_eq!(p.size, 7);
        assert_eq!(p.anchor, DVec3::new(4., 120., 8.));
        assert!((copy.mobs[0].shape().height - 1.025).abs() < 1e-6);
        let mut drops = 0;
        for _ in 0..40 {
            e.drop_loot(MobKind::Phantom, DVec3::ZERO);
            drops += e.items.iter().filter(|i| i.stack.item == crate::item::Item::PHANTOM_MEMBRANE).count();
            e.items.clear();
        }
        assert!(drops > 0);
    }
    #[test]
    fn swoop_hits_target_then_returns_to_circling() {
        let w = Grid::flat(64);
        let mut m = Mob::new(MobKind::Phantom, DVec3::new(0., 80.7, 0.), 0.);
        let p = m.phantom.as_mut().unwrap();
        p.target = Some(PlayerId(7));
        p.swooping = true;
        let mut ev = Vec::new();
        m.phantom_flight(0.05, &w, &context(90000), &mut Rng::new(3), &mut ev);
        assert!(ev.iter().any(|e| matches!(e, EntityEvent::PlayerHit { player: PlayerId(7), damage: 6., .. })));
        assert!(!m.phantom.as_ref().unwrap().swooping);
    }
}
