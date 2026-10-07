//! Bounded, device-free client particle simulation. Velocities are blocks
//! per Java game tick (20 Hz); rendering interpolates previous/current positions.
//! Gameplay owns requests, never GPU resources. See docs/particles.md.

use std::collections::VecDeque;

use glam::{DVec3, IVec3};

use crate::physics::{self, BlockSource, Shape};
use crate::world::block::Block;

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
    stopped: bool,
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
            stopped: false,
        }
    }

    fn tick<W: BlockSource + ?Sized>(&mut self, world: &W) -> bool {
        self.previous = self.pos;
        if self.age >= self.lifetime {
            return false;
        }
        self.age += 1;
        let t = self.age as f64 / self.lifetime.max(1) as f64;
        match self.motion {
            Motion::Stationary => return true,
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
                        self.velocity.x *= 0.7;
                        self.velocity.z *= 0.7;
                        if self.motion == Motion::Splash {
                            return false;
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
        if matches!(self.texture, Texture::Sprite(_)) && self.style == Kind::Smoke && self.pos.y == self.previous.y {
            self.velocity.x *= 1.1;
            self.velocity.z *= 1.1;
        }
        self.velocity *= self.friction;
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
    Explosion { pos: DVec3, large: bool },
    Break { cell: IVec3, block: Block },
    Hit { cell: IVec3, block: Block, face: IVec3 },
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
    fn bubbles_expire_outside_water() {
        let mut p = Particle::new(DVec3::Y, Texture::Sprite(0));
        p.motion = Motion::Bubble;
        assert!(!p.tick(&Floor));
    }
}
