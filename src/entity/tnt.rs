//! Primed TNT: a block-sized entity that falls, slides to a stop, flashes
//! and swells, then explodes when its fuse runs out.

use glam::DVec3;

use super::EntityEvent;
use crate::physics::{self, BlockSource, Shape};

/// Seconds from lighting TNT to the blast (Minecraft's 80 ticks).
pub const FUSE: f32 = 4.0;
/// Blast strength (a creeper is 3).
pub const POWER: f32 = 4.0;
const GRAVITY: f64 = 20.0;
const SHAPE: Shape = Shape::new(0.49, 0.98);

pub struct PrimedTnt {
    /// Bottom centre.
    pub pos: DVec3,
    /// Position at the start of the last simulation step, for rendering.
    pub previous_pos: DVec3,
    vel: DVec3,
    /// Seconds left before it blows.
    pub fuse: f32,
}

impl PrimedTnt {
    /// Create a moving charge with a fuse in seconds and coincident interpolation positions.
    pub fn new(pos: DVec3, vel: DVec3, fuse: f32) -> Self {
        Self { pos, previous_pos: pos, vel, fuse }
    }

    /// Falls and slides; returns `false` (after queueing the blast) once
    /// the fuse has burnt down.
    pub(super) fn update<W: BlockSource + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        events: &mut Vec<EntityEvent>,
    ) -> bool {
        self.fuse -= dt as f32;
        if self.fuse <= 0.0 {
            events.push(EntityEvent::Explosion {
                center: self.pos + DVec3::Y * SHAPE.height / 2.0,
                power: POWER,
                cause: "was blown up by TNT",
                credit_player: false,
            });
            return false;
        }
        self.vel.y -= GRAVITY * dt;
        let delta = self.vel * dt;
        let on_ground = physics::move_box(world, &mut self.pos, &mut self.vel, delta, SHAPE).on_ground;
        if on_ground {
            let k = (1.0 - dt * 8.0).max(0.0);
            self.vel.x *= k;
            self.vel.z *= k;
        }
        self.pos.y > -64.0
    }

    /// Flashing white for the model, every quarter second.
    pub fn flash(&self) -> bool {
        (self.fuse * 4.0) as i32 % 2 == 0
    }

    /// Edge length of the model: it swells just before the blast.
    pub fn size(&self) -> f32 {
        1.0 + (1.0 - self.fuse / 0.4).clamp(0.0, 1.0).powi(2) * 0.2
    }
}
