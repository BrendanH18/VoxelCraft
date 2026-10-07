//! Bounded, device-free client particle simulation. Velocities are blocks
//! per Java game tick (20 Hz); rendering interpolates previous/current positions.
//! Gameplay owns requests, never GPU resources. See docs/particles.md.

use std::collections::VecDeque;

use glam::{DVec3, IVec3};

use crate::physics::{self, BlockSource, Shape};
use crate::world::block::Block;

mod ambient;
mod types;
pub use types::{Burst, Kind, System};

pub const CAPACITY: usize = 16_384;
const REQUEST_CAPACITY: usize = 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Setting {
    #[default]
    All,
    Decreased,
    Minimal,
}

impl Setting {
    pub fn name(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Decreased => "Decreased",
            Self::Minimal => "Minimal",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::All => Self::Decreased,
            Self::Decreased => Self::Minimal,
            Self::Minimal => Self::All,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Texture {
    /// The renderer resolves the block's particle icon, independently of mesh packing.
    Block(Block),
    Sprite(u16),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Motion {
    #[default]
    Normal,
    Bubble,
    Splash,
    Portal,
    Glyph,
    Stationary,
    DripHang,
}

/// A single particle request. Sizes are billboard half-widths, not collision sizes.
#[derive(Clone, Copy, Debug)]
pub struct Particle {
    pub pos: DVec3,
    pub previous: DVec3,
    pub origin: DVec3,
    pub velocity: DVec3,
    pub texture: Texture,
    /// Normalized sub-UV rectangle (terrain uses a quarter of the block icon).
    pub uv: [f32; 4],
    pub color: [f32; 4],
    pub size: f32,
    pub gravity: f64,
    pub friction: f64,
    pub age: u16,
    pub lifetime: u16,
    pub physics: bool,
    pub collision_size: f64,
    pub emissive: bool,
    pub motion: Motion,
    pub style: Kind,
    pub successor_lifetime: u16,
    pub landing_lifetime: u16,
    stopped: bool,
    on_ground: bool,
    random_state: u64,
}

impl Particle {
    pub fn new(pos: DVec3, texture: Texture) -> Self {
        Self {
            pos,
            previous: pos,
            origin: pos,
            velocity: DVec3::ZERO,
            texture,
            uv: [0.0, 0.0, 1.0, 1.0],
            color: [1.0; 4],
            size: 0.1,
            gravity: 0.0,
            friction: 0.98,
            age: 0,
            lifetime: 20,
            physics: true,
            collision_size: 0.2,
            emissive: false,
            motion: Motion::Normal,
            style: Kind::Smoke,
            successor_lifetime: 64,
            landing_lifetime: 20,
            stopped: false,
            on_ground: false,
            random_state: pos.x.to_bits() ^ pos.z.to_bits() ^ 0x5350_4c41_5348,
        }
    }

    fn tick<W: BlockSource + ?Sized>(&mut self, world: &W) -> bool {
        self.previous = self.pos;
        if self.age >= self.lifetime {
            if self.motion == Motion::DripHang {
                self.motion = Motion::Normal;
                self.age = 0;
                self.lifetime = self.successor_lifetime;
                self.gravity = 1.5;
                self.friction = 0.98;
                self.color = [1.0, 0.2857143, 0.083333336, 1.0];
            } else {
                return false;
            }
        }
        self.age += 1;
        let t = self.age as f64 / self.lifetime.max(1) as f64;
        match self.motion {
            Motion::Stationary => return true,
            Motion::DripHang => {
                self.color = [1.0, 16.0 / (self.age as f32 + 16.0), 4.0 / (self.age as f32 + 8.0), 1.0];
                self.velocity.y -= 0.0012;
            }
            Motion::Portal => {
                self.pos = self.origin + self.velocity * (1.0 + t - 2.0 * t * t) + DVec3::Y * (1.0 - t);
                return true;
            }
            Motion::Glyph => {
                self.pos = self.origin + self.velocity * (1.0 - t) - DVec3::Y * (1.2 * t.powi(4));
                return true;
            }
            Motion::Bubble => self.velocity.y += 0.002,
            Motion::Splash => self.velocity.y -= self.gravity,
            Motion::Normal => self.velocity.y -= 0.04 * self.gravity,
        }
        if self.motion == Motion::Splash && self.on_ground {
            self.random_state = self.random_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            if self.random_state >> 63 == 0 {
                return false;
            }
        }
        if !self.stopped {
            let delta = self.velocity;
            if self.physics {
                // Substeps avoid tunnelling; use the shared shaped-block collision path.
                let steps = (delta.abs().max_element() / 0.1).ceil().clamp(1.0, 32.0) as usize;
                for _ in 0..steps {
                    let hit = physics::move_box(
                        world,
                        &mut self.pos,
                        &mut self.velocity,
                        delta / steps as f64,
                        Shape::new(self.collision_size * 0.5, self.collision_size),
                    );
                    if hit.on_ground || hit.ceiling {
                        self.stopped = true;
                    }
                    if hit.on_ground {
                        self.on_ground = true;
                        self.velocity.x *= 0.7;
                        self.velocity.z *= 0.7;
                        if self.style == Kind::LavaDrip && self.motion == Motion::Normal {
                            self.motion = Motion::Stationary;
                            self.age = 0;
                            self.lifetime = self.landing_lifetime;
                        }
                        if self.motion == Motion::Splash {
                            self.random_state =
                                self.random_state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                            if self.random_state >> 63 == 0 {
                                return false;
                            }
                        }
                    }
                    if self.stopped {
                        break;
                    }
                }
            } else {
                self.pos += delta;
            }
        }
        if matches!(self.texture, Texture::Sprite(_)) && matches!(self.style, Kind::Crit | Kind::MagicCrit) {
            self.color[1] *= 0.96;
            self.color[2] *= 0.9;
        }
        if matches!(self.texture, Texture::Sprite(_))
            && matches!(self.style, Kind::Smoke | Kind::LargeSmoke)
            && self.pos.y == self.previous.y
        {
            self.velocity.x *= 1.1;
            self.velocity.z *= 1.1;
        }
        if self.style == Kind::DragonBreath {
            if self.pos.y == self.previous.y {
                self.velocity.x *= 1.1;
                self.velocity.z *= 1.1;
            }
            self.velocity.x *= self.friction;
            self.velocity.z *= self.friction;
        } else {
            self.velocity *= self.friction;
        }
        if self.style == Kind::LavaDrip && world.block(self.pos.floor().as_ivec3()).is_some_and(Block::is_lava) {
            return false;
        }
        if self.motion == Motion::Bubble && !world.block(self.pos.floor().as_ivec3()).is_some_and(Block::is_water) {
            return false;
        }
        if self.motion == Motion::Splash
            && world
                .block(self.pos.floor().as_ivec3())
                .is_some_and(|b| b.is_fluid() && self.pos.y.fract() < 1.0 - b.fluid_drop() as f64 / 16.0)
        {
            return false;
        }
        true
    }
}

/// Pure simulation actions can enqueue client visuals without knowing a device exists.
#[derive(Clone, Copy, Debug)]
pub enum Request {
    Particle(Particle),
    Burst(Burst),
    Tracking(Burst),
    Explosion {
        pos: DVec3,
        large: bool,
    },
    Break {
        cell: IVec3,
        block: Block,
    },
    Hit {
        cell: IVec3,
        block: Block,
        face: IVec3,
    },
    /// Java level event 2003: the portal ring when an eye of ender shatters.
    EyeBreak {
        pos: DVec3,
    },
}

/// A bounded request mailbox; oldest requests are evicted if the client isn't polling.
pub struct Requests(VecDeque<Request>);

impl Default for Requests {
    fn default() -> Self {
        Self(VecDeque::with_capacity(REQUEST_CAPACITY))
    }
}

impl Requests {
    pub fn push(&mut self, request: Request) {
        if self.0.len() == REQUEST_CAPACITY {
            self.0.pop_front();
        }
        self.0.push_back(request);
    }

    pub fn pop(&mut self) -> Option<Request> {
        self.0.pop_front()
    }

    pub fn drain(&mut self) -> impl Iterator<Item = Request> + '_ {
        self.0.drain(..)
    }
}

/// A FIFO pool matching Java's oldest-particle eviction. All backing storage
/// is reserved at construction; tick, spawn and iteration never allocate.
pub struct Pool {
    particles: VecDeque<Particle>,
}

impl Default for Pool {
    fn default() -> Self {
        Self { particles: VecDeque::with_capacity(CAPACITY) }
    }
}

impl Pool {
    pub fn spawn(&mut self, particle: Particle) {
        if self.particles.len() == CAPACITY {
            self.particles.pop_front();
        }
        self.particles.push_back(particle);
    }

    pub fn clear(&mut self) {
        self.particles.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = &Particle> {
        self.particles.iter()
    }

    pub fn tick<W: BlockSource + ?Sized>(&mut self, world: &W) {
        self.particles.retain_mut(|p| p.tick(world));
    }
}

/// Shared player movement feedback; both the host and headless agents call
/// this after movement. Lava never produces water particles.
pub fn water_entry(player: &crate::player::Player, previous: DVec3, world: &mut crate::world::World) {
    let wet = |pos: DVec3| world.get_block((pos + DVec3::Y * 0.3).floor().as_ivec3()).is_some_and(Block::is_water);
    if !wet(player.pos) || wet(previous) {
        return;
    }
    let cell = (player.pos + DVec3::Y * 0.3).floor().as_ivec3();
    let block = world.get_block(cell).unwrap_or(Block::WATER);
    // Java counts `1 + width * 20` and spreads across the width. The surface
    // is the fluid top (floor(feet)+1 for a source). Bubbles sit just under
    // that plane so an exact integer top is still inside the water block.
    let count = (1.0 + crate::player::HALF_WIDTH * 40.0) as u16;
    let pos = player.pos.with_y(cell.y as f64 + 1.0 - block.fluid_drop() as f64 / 16.0);
    for kind in [Kind::Bubble, Kind::Splash] {
        let mut b = Burst::new(kind, pos, count);
        b.spread = DVec3::new(crate::player::HALF_WIDTH * 2.0, 0.0, crate::player::HALF_WIDTH * 2.0);
        b.velocity = player.vel / 20.0;
        if kind == Kind::Bubble {
            b.pos.y -= 0.1;
            b.velocity.y -= 0.1;
        }
        world.particles.push(Request::Burst(b));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Floor;
    impl BlockSource for Floor {
        fn block(&self, p: IVec3) -> Option<Block> {
            Some(if p.y < 0 { Block::STONE } else { Block::AIR })
        }
    }

    #[test]
    fn java_lifetime_and_gravity() {
        let mut p = Particle::new(DVec3::Y * 4.0, Texture::Sprite(0));
        p.lifetime = 2;
        p.gravity = 1.0;
        p.friction = 0.98;
        assert!(p.tick(&Floor));
        assert!((p.pos.y - 3.96).abs() < 1e-9);
        assert!((p.velocity.y + 0.0392).abs() < 1e-9);
        assert!(p.tick(&Floor));
        assert!(!p.tick(&Floor));
    }

    #[test]
    fn collision_stops_crumbs_on_the_floor() {
        let mut p = Particle::new(DVec3::new(0.5, 0.4, 0.5), Texture::Sprite(0));
        p.gravity = 1.0;
        p.lifetime = 100;
        for _ in 0..20 {
            assert!(p.tick(&Floor));
        }
        assert!(p.stopped);
        assert!(p.pos.y >= 0.0 && p.pos.y < 0.001);
    }

    #[test]
    fn overflow_evicts_the_oldest_without_growing() {
        let mut pool = Pool::default();
        let capacity = pool.particles.capacity();
        for i in 0..CAPACITY + 5 {
            pool.spawn(Particle::new(DVec3::X * i as f64, Texture::Sprite(0)));
        }
        assert_eq!(pool.particles.len(), CAPACITY);
        assert_eq!(pool.particles.front().unwrap().pos.x, 5.0);
        assert_eq!(pool.particles.capacity(), capacity);
    }

    #[test]
    fn requests_are_bounded_in_headless_worlds() {
        let mut requests = Requests::default();
        for _ in 0..REQUEST_CAPACITY + 3 {
            requests.push(Request::Particle(Particle::new(DVec3::ZERO, Texture::Sprite(0))));
        }
        assert_eq!(requests.drain().count(), REQUEST_CAPACITY);
        assert_eq!(requests.drain().count(), 0);
    }

    #[test]
    fn walls_clip_horizontal_motion() {
        struct Wall;
        impl BlockSource for Wall {
            fn block(&self, p: IVec3) -> Option<Block> {
                Some(if p.x >= 1 { Block::STONE } else { Block::AIR })
            }
        }
        let mut p = Particle::new(DVec3::new(0.5, 2.0, 0.5), Texture::Sprite(0));
        p.velocity.x = 0.6;
        assert!(p.tick(&Wall));
        assert!(p.pos.x <= 0.9 && p.pos.x > 0.89);
        assert_eq!(p.velocity.x, 0.0);
    }

    #[test]
    fn lava_drip_hangs_then_falls_and_lands() {
        let mut p = Particle::new(DVec3::new(0.5, 2.0, 0.5), Texture::Sprite(12));
        p.style = Kind::LavaDrip;
        p.motion = Motion::DripHang;
        p.lifetime = 40;
        p.friction = 0.0196;
        p.collision_size = 0.01;
        for _ in 0..40 {
            assert!(p.tick(&Floor));
        }
        assert!(p.pos.y > 1.9);
        assert!(p.tick(&Floor));
        assert_eq!(p.motion, Motion::Normal);
        for _ in 0..12 {
            assert!(p.tick(&Floor));
        }
        assert_eq!(p.motion, Motion::Stationary);
        assert!(p.pos.y < 0.001);
    }

    #[test]
    fn bubbles_expire_outside_water() {
        let mut p = Particle::new(DVec3::Y, Texture::Sprite(0));
        p.motion = Motion::Bubble;
        assert!(!p.tick(&Floor));
    }
}
