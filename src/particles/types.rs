//! Java particle providers and bounded multi-tick emitters.

use super::{Motion, Particle, Pool, Request, Setting, Texture};
use crate::entity::Rng;
use crate::world::{World, block::Block};
use glam::{DVec3, IVec3};
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Smoke,
    Flame,
    Crit,
    MagicCrit,
    Poof,
    Lava,
    LavaDrip,
    Splash,
    Bubble,
    Explosion,
    Portal,
    Glyph,
    Effect,
    DragonBreath,
    Heart,
    Angry,
}

impl Kind {
    pub fn sprite(self) -> u16 {
        match self {
            Self::Smoke | Self::Poof => 0,
            Self::Flame | Self::Lava => 1,
            Self::Crit | Self::MagicCrit => 2,
            Self::Bubble => 3,
            Self::Splash => 4,
            Self::Portal => 5,
            Self::Glyph => 6,
            Self::Heart => 7,
            Self::Angry => 8,
            Self::Effect => 9,
            Self::Explosion => 10,
            Self::DragonBreath => 11,
            Self::LavaDrip => 12,
        }
    }
}

/// Velocity is blocks/tick. `spread` distributes particles in a box around
/// the origin. Direct combat/death effects bypass the options filter.
#[derive(Clone, Copy, Debug)]
pub struct Burst {
    pub kind: Kind,
    pub pos: DVec3,
    pub velocity: DVec3,
    pub spread: DVec3,
    pub count: u16,
    pub color: Option<[f32; 4]>,
    pub forced: bool,
    pub important: bool,
}

impl Burst {
    pub fn new(kind: Kind, pos: DVec3, count: u16) -> Self {
        Self {
            kind,
            pos,
            velocity: DVec3::ZERO,
            spread: DVec3::ZERO,
            count,
            color: None,
            forced: false,
            important: false,
        }
    }
}

struct Tracking {
    burst: Burst,
    age: u8,
}

struct Emitter {
    pos: DVec3,
    age: u8,
}

pub struct System {
    pub pool: Pool,
    rng: Rng,
    explosions: VecDeque<Emitter>,
    tracking: VecDeque<Tracking>,
}

impl System {
    pub fn new(seed: u64) -> Self {
        Self {
            pool: Pool::default(),
            rng: Rng::new(seed ^ 0x7061_7274_6963_6c65),
            explosions: VecDeque::with_capacity(256),
            tracking: VecDeque::with_capacity(256),
        }
    }

    pub fn clear(&mut self) {
        self.pool.clear();
        self.explosions.clear();
        self.tracking.clear();
    }

    /// Java's ordinary particle filter: All, 2/3 Decreased, zero Minimal.
    /// Important particles get a 1/10 rescue on Minimal, then the 2/3 filter.
    pub fn accept(&mut self, setting: Setting, forced: bool, important: bool) -> bool {
        if forced {
            return true;
        }
        match setting {
            Setting::All => true,
            Setting::Decreased => self.rng.next_f32() >= 1.0 / 3.0,
            Setting::Minimal => important && self.rng.chance(0.1) && self.rng.next_f32() >= 1.0 / 3.0,
        }
    }

    pub fn request(&mut self, request: Request, setting: Setting, world: &World) {
        match request {
            Request::Particle(p) => self.pool.spawn(p),
            Request::Break { cell, block } => self.break_block(cell, block, world),
            Request::Hit { cell, block, face } => self.hit_block(cell, block, face, world),
            Request::Burst(b) => self.burst(b, setting),
            Request::Tracking(burst) => {
                if self.tracking.len() == 256 {
                    self.tracking.pop_front();
                }
                self.tracking.push_back(Tracking { burst, age: 0 });
            }
            Request::Explosion { pos, large } => {
                // Explosion types override the normal options limiter in Java.
                if large {
                    if self.explosions.len() == 256 {
                        self.explosions.pop_front();
                    }
                    self.explosions.push_back(Emitter { pos, age: 0 });
                } else {
                    let mut b = Burst::new(Kind::Explosion, pos, 1);
                    b.forced = true;
                    self.burst(b, setting);
                }
            }
        }
    }

    pub fn tick(&mut self, world: &World, setting: Setting) {
        self.pool.tick(world);
        let n = self.tracking.len();
        for _ in 0..n {
            let mut emitter = self.tracking.pop_front().unwrap();
            for _ in 0..16 {
                let direction = random_vec(&mut self.rng) * 2.0 - DVec3::ONE;
                if direction.length_squared() > 1.0 || !self.accept(setting, false, false) {
                    continue;
                }
                let p = self.make(
                    emitter.burst.kind,
                    emitter.burst.pos + direction * emitter.burst.spread,
                    direction + DVec3::Y * 0.2,
                );
                self.pool.spawn(p);
            }
            emitter.age += 1;
            emitter.burst.pos += emitter.burst.velocity;
            if emitter.age < 3 {
                self.tracking.push_back(emitter);
            }
        }
        let n = self.explosions.len();
        for _ in 0..n {
            let mut e = self.explosions.pop_front().unwrap();
            for _ in 0..6 {
                let pos = e.pos + (random_vec(&mut self.rng) - random_vec(&mut self.rng)) * 4.0;
                let mut p = self.make(Kind::Explosion, pos, DVec3::ZERO);
                p.size = 2.0 * (1.0 - e.age as f32 / 16.0);
                self.pool.spawn(p);
            }
            e.age += 1;
            if e.age < 8 {
                self.explosions.push_back(e);
            }
        }
        // Lava pops emit their own smoke; collect into a bounded stack scratch
        // so spawning cannot invalidate iteration or allocate each tick.
        let mut smoke = [(DVec3::ZERO, DVec3::ZERO); 256];
        let mut count = 0;
        for p in self.pool.iter() {
            if matches!(p.texture, Texture::Sprite(1))
                && p.gravity == 0.75
                && count < smoke.len()
                && self.rng.next_f32() > p.age as f32 / p.lifetime as f32
            {
                smoke[count] = (p.pos, p.velocity);
                count += 1;
            }
        }
        for &(pos, velocity) in &smoke[..count] {
            let mut b = Burst::new(Kind::Smoke, pos, 1);
            b.velocity = velocity;
            self.burst(b, setting);
        }
    }

    pub fn burst(&mut self, b: Burst, setting: Setting) {
        for _ in 0..b.count.min(super::CAPACITY as u16) {
            if !self.accept(setting, b.forced, b.important) {
                continue;
            }
            let pos = b.pos + (random_vec(&mut self.rng) * 2.0 - DVec3::ONE) * b.spread;
            let mut p = self.make(b.kind, pos, b.velocity);
            if let Some(color) = b.color {
                p.color = color;
            }
            self.pool.spawn(p);
        }
    }

    fn make(&mut self, kind: Kind, pos: DVec3, velocity: DVec3) -> Particle {
        let rng = &mut self.rng;
        let mut p = Particle::new(pos, Texture::Sprite(kind.sprite()));
        let base_size = 0.1 * rng.range(1.0, 2.0);
        p.size = base_size;
        p.velocity = java_velocity(rng, velocity);
        p.lifetime = (4.0 / rng.range(0.1, 1.0)) as u16;
        p.style = kind;
        match kind {
            Kind::Crit | Kind::MagicCrit => {
                p.friction = 0.7;
                p.gravity = 0.5;
                p.physics = false;
                p.velocity = p.velocity * 0.1 + velocity * 0.4;
                let c = rng.range(0.6, 0.9);
                p.color = [c, c, c, 1.0];
                if kind == Kind::MagicCrit {
                    p.color[0] *= 0.3;
                    p.color[1] *= 0.8;
                }
                p.size *= 0.75;
                p.lifetime = (6.0 / rng.range(0.6, 1.4)) as u16;
            }
            Kind::Smoke => {
                p.friction = 0.96;
                p.gravity = -0.1;
                p.velocity = p.velocity * 0.1 + velocity;
                let c = rng.range(0.0, 0.3);
                p.color = [c, c, c, 1.0];
                p.size *= 0.75;
                p.lifetime = (8.0 / rng.range(0.2, 1.0)) as u16;
            }
            Kind::Poof => {
                p.friction = 0.9;
                p.gravity = -0.1;
                p.velocity = velocity + (random_vec(rng) * 2.0 - DVec3::ONE) * 0.05;
                let c = rng.range(0.7, 1.0);
                p.color = [c, c, c, 1.0];
                p.size = 0.1 * (rng.next_f32() * rng.next_f32() * 6.0 + 1.0);
                p.lifetime = (16.0 / rng.range(0.2, 1.0)) as u16 + 2;
            }
            Kind::Flame => {
                p.physics = false;
                p.friction = 0.96;
                p.velocity = p.velocity * 0.01 + velocity;
                p.lifetime = (8.0 / rng.range(0.2, 1.0)) as u16 + 4;
            }
            Kind::Lava => {
                p.gravity = 0.75;
                p.friction = 0.999;
                p.emissive = true;
                p.velocity *= 0.8;
                p.velocity.y = rng.range(0.05, 0.45) as f64;
                p.size *= rng.range(0.2, 2.2);
                p.lifetime = (16.0 / rng.range(0.2, 1.0)) as u16;
            }
            Kind::LavaDrip => {
                p.gravity = 1.5;
                p.size = 0.01;
                p.emissive = true;
                p.velocity = DVec3::ZERO;
                p.lifetime = 40;
                p.color = [1.0, 0.3, 0.02, 1.0];
                p.collision_size = 0.01;
            }
            Kind::Splash => {
                p.motion = Motion::Splash;
                p.gravity = 0.04;
                p.collision_size = 0.01;
                p.velocity *= 0.3;
                p.velocity.y = rng.range(0.1, 0.3) as f64;
                if velocity.y == 0.0 && (velocity.x != 0.0 || velocity.z != 0.0) {
                    p.velocity = DVec3::new(velocity.x, 0.1, velocity.z);
                }
                p.color = [0.45, 0.65, 1.0, 1.0];
                p.lifetime = (8.0 / rng.range(0.2, 1.0)) as u16;
            }
            Kind::Bubble => {
                p.motion = Motion::Bubble;
                p.collision_size = 0.02;
                p.friction = 0.85;
                p.size *= rng.range(0.2, 0.8);
                p.velocity = velocity * 0.2 + (random_vec(rng) * 2.0 - DVec3::ONE) * 0.02;
                p.lifetime = (8.0 / rng.range(0.2, 1.0)) as u16;
            }
            Kind::Explosion => {
                p.motion = Motion::Stationary;
                p.physics = false;
                p.emissive = true;
                let c = rng.range(0.4, 1.0);
                p.color = [c, c, c, 1.0];
                p.size = 2.0;
                p.lifetime = 6 + (rng.next_f32() * 4.0) as u16;
            }
            Kind::Portal => {
                p.motion = Motion::Portal;
                p.physics = false;
                p.velocity = velocity;
                p.size = 0.1 * rng.range(0.5, 0.7);
                let c = rng.range(0.4, 1.0);
                p.color = [c * 0.9, c * 0.3, c, 1.0];
                p.lifetime = 40 + (rng.next_f32() * 10.0) as u16;
            }
            Kind::Glyph => {
                p.motion = Motion::Glyph;
                p.physics = false;
                p.velocity = velocity;
                p.pos = pos + velocity;
                p.previous = p.pos;
                p.size = 0.1 * rng.range(0.2, 0.7);
                let c = rng.range(0.4, 1.0);
                p.color = [c * 0.9, c * 0.9, c, 1.0];
                p.lifetime = 30 + (rng.next_f32() * 10.0) as u16;
            }
            Kind::Effect => {
                p.physics = false;
                p.friction = 0.96;
                p.gravity = -0.1;
                p.velocity.y *= 0.2;
                p.velocity.x *= 0.1;
                p.velocity.z *= 0.1;
                p.size *= 0.75;
                p.lifetime = (8.0 / rng.range(0.2, 1.0)) as u16;
            }
            Kind::DragonBreath => {
                p.physics = false;
                p.friction = 0.96;
                p.velocity = velocity;
                p.color = [rng.range(0.7176471, 0.8745098), 0.0, rng.range(0.8235294, 0.9764706), 1.0];
                p.size *= 0.75;
                p.lifetime = (20.0 / rng.range(0.2, 1.0)) as u16;
            }
            Kind::Heart | Kind::Angry => {
                p.velocity *= 0.01;
                p.velocity.y += 0.1;
                p.size *= 0.75;
                p.lifetime = 16;
                p.color = if kind == Kind::Heart { [1.0, 0.1, 0.1, 1.0] } else { [1.0; 4] };
            }
        }
        if matches!(kind, Kind::Crit | Kind::MagicCrit) {
            p.age = 1;
            p.velocity.y -= 0.02;
            p.pos += p.velocity;
            p.velocity *= 0.7;
            p.color[1] *= 0.96;
            p.color[2] *= 0.9;
        }
        p
    }

    fn terrain(&mut self, pos: DVec3, origin: DVec3, block: Block, velocity: DVec3) -> Particle {
        let mut p = Particle::new(pos, Texture::Block(block));
        p.origin = origin;
        p.gravity = 1.0;
        p.color = [0.6, 0.6, 0.6, 1.0];
        p.size = 0.05 * self.rng.range(1.0, 2.0);
        p.velocity = java_velocity(&mut self.rng, velocity);
        p.lifetime = (4.0 / self.rng.range(0.1, 1.0)) as u16;
        let u = self.rng.range(0.0, 0.75);
        let v = self.rng.range(0.0, 0.75);
        p.uv = [u + 0.25, v, u, v + 0.25];
        p
    }

    fn break_block(&mut self, cell: IVec3, block: Block, world: &World) {
        if block == Block::AIR || block.is_fluid() || block == Block::NETHER_PORTAL {
            return;
        }
        // Selection boxes preserve stairs/slabs/doors and 64 crumbs for a cube.
        let neighbour = |side: crate::world::block::Facing| world.get_block(cell + side.offset()).unwrap_or(Block::AIR);
        let boxes = if block.kind() == crate::world::block::RenderKind::Shaped {
            crate::world::shape::shape(block, neighbour)
        } else {
            crate::world::shape::Boxes::from_box(crate::world::shape::Box16 {
                min: [0; 3],
                max: [16, (block.height() * 16.0) as u8, 16],
            })
        };
        for bx in boxes.as_slice() {
            let min = DVec3::from_array(bx.min.map(|v| v as f64 / 16.0));
            let size = DVec3::from_array(bx.max.map(|v| v as f64 / 16.0)) - min;
            let counts = (size / 0.25).ceil().max(DVec3::splat(2.0)).as_ivec3();
            for x in 0..counts.x {
                for y in 0..counts.y {
                    for z in 0..counts.z {
                        let fraction = DVec3::new(
                            (x as f64 + 0.5) / counts.x as f64,
                            (y as f64 + 0.5) / counts.y as f64,
                            (z as f64 + 0.5) / counts.z as f64,
                        );
                        let pos = cell.as_dvec3() + min + fraction * size;
                        let p = self.terrain(pos, cell.as_dvec3(), block, fraction - DVec3::splat(0.5));
                        self.pool.spawn(p);
                    }
                }
            }
        }
    }

    fn hit_block(&mut self, cell: IVec3, block: Block, face: IVec3, world: &World) {
        let (min, max) = world.outline(cell);
        let min = DVec3::from_array(min.map(|v| v as f64));
        let max = DVec3::from_array(max.map(|v| v as f64));
        let mut pos =
            cell.as_dvec3() + min + DVec3::splat(0.1) + random_vec(&mut self.rng) * (max - min - DVec3::splat(0.2));
        for axis in 0..3 {
            if face[axis] > 0 {
                pos[axis] = cell[axis] as f64 + max[axis] + 0.1;
            }
            if face[axis] < 0 {
                pos[axis] = cell[axis] as f64 + min[axis] - 0.1;
            }
        }
        let mut p = self.terrain(pos, cell.as_dvec3(), block, DVec3::ZERO);
        p.velocity.x *= 0.2;
        p.velocity.z *= 0.2;
        p.velocity.y = (p.velocity.y - 0.1) * 0.2 + 0.1;
        p.size *= 0.6;
        p.collision_size *= 0.6;
        self.pool.spawn(p);
    }
}

pub fn random_vec(rng: &mut Rng) -> DVec3 {
    DVec3::new(rng.next_f32() as f64, rng.next_f32() as f64, rng.next_f32() as f64)
}

fn java_velocity(rng: &mut Rng, supplied: DVec3) -> DVec3 {
    let v = supplied + (random_vec(rng) * 2.0 - DVec3::ONE) * 0.4;
    let speed = (rng.next_f32() + rng.next_f32() + 1.0) as f64 * 0.15 * 0.4;
    v.normalize_or(DVec3::Y) * speed + DVec3::Y * 0.1
}

#[cfg(test)]
mod tests {
    use super::*;
    fn world() -> World {
        World::new_headless(std::sync::Arc::new(crate::world::terrain::Generator::new(42)), Default::default(), 2)
    }

    #[test]
    fn full_cube_break_has_64_quarter_texture_crumbs() {
        let mut system = System::new(1);
        system.request(Request::Break { cell: IVec3::new(0, 150, 0), block: Block::STONE }, Setting::Minimal, &world());
        assert_eq!(system.pool.iter().count(), 64);
        for p in system.pool.iter() {
            assert_eq!(p.gravity, 1.0);
            assert!((p.uv[0] - p.uv[2] - 0.25).abs() < 1e-6);
            assert!((p.uv[3] - p.uv[1] - 0.25).abs() < 1e-6);
            assert!(p.pos.cmpge(DVec3::new(0.0, 150.0, 0.0)).all());
            assert!((4..=39).contains(&p.lifetime));
        }
    }

    #[test]
    fn settings_match_java_filter_probabilities_and_override() {
        let mut system = System::new(2);
        for _ in 0..100 {
            assert!(system.accept(Setting::All, false, false));
            assert!(!system.accept(Setting::Minimal, false, false));
            assert!(system.accept(Setting::Minimal, true, false));
        }
        let decreased = (0..3000).filter(|_| system.accept(Setting::Decreased, false, false)).count();
        assert!((1850..2150).contains(&decreased), "{decreased}");
        let rescued = (0..3000).filter(|_| system.accept(Setting::Minimal, false, true)).count();
        assert!((140..260).contains(&rescued), "{rescued}");
    }

    #[test]
    fn explosion_emitter_spawns_six_per_tick_for_eight_ticks() {
        let mut system = System::new(3);
        let world = world();
        system.request(Request::Explosion { pos: DVec3::Y * 150.0, large: true }, Setting::Minimal, &world);
        for _ in 0..8 {
            system.tick(&world, Setting::Minimal);
        }
        assert!(system.explosions.is_empty());
        assert!(system.pool.iter().count() <= 48);
        assert!(system.pool.iter().all(|p| p.emissive && p.motion == Motion::Stationary));
    }

    #[test]
    fn crit_emitter_has_three_ticks_and_obeys_minimal() {
        let mut system = System::new(4);
        let world = world();
        system.request(Request::Tracking(Burst::new(Kind::Crit, DVec3::Y * 150.0, 16)), Setting::Minimal, &world);
        for _ in 0..3 {
            system.tick(&world, Setting::Minimal);
        }
        assert!(system.tracking.is_empty());
        assert_eq!(system.pool.iter().count(), 0);
    }

    #[test]
    fn portal_and_glyph_lifetimes_and_paths() {
        let mut system = System::new(5);
        let portal = system.make(Kind::Portal, DVec3::ZERO, DVec3::X);
        assert!((40..=49).contains(&portal.lifetime));
        assert_eq!(portal.motion, Motion::Portal);
        let glyph = system.make(Kind::Glyph, DVec3::ZERO, DVec3::X * 2.0);
        assert!((30..=39).contains(&glyph.lifetime));
        assert_eq!(glyph.pos, DVec3::X * 2.0);
    }
}
