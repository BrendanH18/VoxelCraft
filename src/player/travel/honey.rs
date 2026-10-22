//! Java 26.3 HoneyBlock.isSlidingDown / doSlideMovement.
use super::Player;
use glam::{DVec3, IVec3};

impl Player {
    pub(super) fn honey_slide(&mut self, cell: IVec3, dm: &mut DVec3) {
        let old_y = dm.y / f64::from(0.98_f32) + 0.08;
        let overlap = 0.4375 + f64::from((self.collision_shape().half_width * 2.0) as f32 / 2.0);
        if self.on_ground || self.pos.y > cell.y as f64 + 0.9375 - 1.0e-7 || old_y >= -0.08 {
            return;
        }
        if (cell.x as f64 + 0.5 - self.pos.x).abs() + 1.0e-7 <= overlap
            && (cell.z as f64 + 0.5 - self.pos.z).abs() + 1.0e-7 <= overlap
        {
            return;
        }
        if old_y < -0.13 {
            let scale = -0.05 / old_y;
            dm.x *= scale;
            dm.z *= scale;
        }
        dm.y = (-0.05 - 0.08) * f64::from(0.98_f32);
        self.motion.fall_distance = 0.0;
        self.honey_sliding = true;
    }

    /// The supporting cell, including honey's 1/16 inset top.
    pub fn on_honey<W: crate::physics::BlockSource + ?Sized>(&self, world: &W) -> bool {
        self.on_ground && world.block(self.on_pos(world, 0.2)) == Some(crate::world::block::Block::HONEY_BLOCK)
    }
}
