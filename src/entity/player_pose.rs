//! Java player poses: shared hitbox, eye height and collision resolution
//! (Player.updatePlayerPose / EntityDimensions / getStandingEyeHeight).

use glam::DVec3;

use crate::physics::{self, BlockSource, Shape};
use crate::player::Player;

/// Effective pose after [`resolve_pose`]; crawling reuses [`PlayerPose::Swimming`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PlayerPose {
    #[default]
    Standing,
    Crouching,
    Swimming,
}

impl PlayerPose {
    // `Avatar.POSES` stores sizes and eye heights as floats; the double
    // bounding box and eye position widen them. Preserving that conversion
    // matters at splash-duration rounding boundaries and in tight gaps.
    pub const STANDING_HEIGHT: f64 = 1.8_f32 as f64;
    pub const CROUCH_HEIGHT: f64 = 1.5_f32 as f64;
    pub const SWIM_HEIGHT: f64 = 0.6_f32 as f64;
    pub const HALF_WIDTH: f64 = 0.6_f32 as f64 / 2.0;

    pub const STANDING_EYE: f64 = 1.62_f32 as f64;
    pub const CROUCH_EYE: f64 = 1.27_f32 as f64;
    pub const SWIM_EYE: f64 = 0.4_f32 as f64;

    pub fn height(self) -> f64 {
        match self {
            Self::Standing => Self::STANDING_HEIGHT,
            Self::Crouching => Self::CROUCH_HEIGHT,
            Self::Swimming => Self::SWIM_HEIGHT,
        }
    }

    pub fn eye_height(self) -> f64 {
        match self {
            Self::Standing => Self::STANDING_EYE,
            Self::Crouching => Self::CROUCH_EYE,
            Self::Swimming => Self::SWIM_EYE,
        }
    }

    pub fn shape(self) -> Shape {
        Shape::new(Self::HALF_WIDTH, self.height())
    }

    /// HumanoidModel uses swimAmount while visually swimming (pose, not the sprint flag).
    pub fn visually_swimming(self) -> bool {
        self == Self::Swimming
    }

    pub fn crawling(self, in_water: bool) -> bool {
        self == Self::Swimming && !in_water
    }
}

/// Java `canPlayerFitWithinBlocksAndEntitiesWhen`: the pose's box,
/// deflated by `1.0E-7`, overlaps no block.
pub fn fits_at<W: BlockSource + ?Sized>(world: &W, pos: DVec3, pose: PlayerPose) -> bool {
    let (min, max) = pose.shape().aabb(pos);
    let inset = DVec3::splat(physics::JAVA_EPSILON);
    physics::no_collision(world, min + inset, max - inset)
}

/// Player pose checks retain the original entity context for conditional
/// scaffolding and powder-snow collision shapes.
pub fn fits_player<W: BlockSource + ?Sized>(world: &W, player: &Player, pose: PlayerPose) -> bool {
    let (min, max) = pose.shape().aabb(player.pos);
    let inset = DVec3::splat(physics::JAVA_EPSILON);
    physics::no_collision_context(world, min + inset, max - inset, Some(player.collision_context()))
}

/// Java `Player.updatePlayerPose` when the swimming box fits at the feet.
pub fn resolve_pose<W: BlockSource + ?Sized>(player: &Player, world: &W, shift: bool) -> PlayerPose {
    if player.vehicle.is_some() {
        return PlayerPose::Standing;
    }
    if !fits_player(world, player, PlayerPose::Swimming) {
        return player.pose;
    }
    let desired = if player.swimming {
        PlayerPose::Swimming
    } else if shift && !player.flying {
        PlayerPose::Crouching
    } else {
        PlayerPose::Standing
    };
    if fits_player(world, player, desired) {
        desired
    } else if fits_player(world, player, PlayerPose::Crouching) {
        PlayerPose::Crouching
    } else {
        PlayerPose::Swimming
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::test_util::Grid;
    use crate::world::block::Block;
    use glam::IVec3;

    #[test]
    fn dimensions_match_java_entity_data() {
        assert_eq!(PlayerPose::Standing.shape().height, 1.8_f32 as f64);
        assert_eq!(PlayerPose::Crouching.shape().height, 1.5_f32 as f64);
        assert_eq!(PlayerPose::Swimming.shape().height, 0.6_f32 as f64);
        assert_eq!(PlayerPose::Standing.eye_height(), 1.62_f32 as f64);
        assert_eq!(PlayerPose::Crouching.eye_height(), 1.27_f32 as f64);
        assert_eq!(PlayerPose::Swimming.eye_height(), 0.4_f32 as f64);
    }

    #[test]
    fn one_block_gap_forces_swim_pose() {
        let mut world = Grid::flat(10);
        let base = DVec3::new(0.5, 10.0, 0.5);
        for x in -1..=1 {
            for z in -1..=1 {
                world.set(IVec3::new(x, 11, z), Block::STONE);
            }
        }
        let mut p = Player::new(base);
        p.pose = PlayerPose::Standing;
        p.pose = resolve_pose(&p, &world, false);
        assert_eq!(p.pose, PlayerPose::Swimming);
    }

    #[test]
    fn shift_resolves_to_crouch_when_it_fits() {
        let world = Grid::flat(10);
        let mut p = Player::new(DVec3::new(0.5, 10.0, 0.5));
        p.pose = resolve_pose(&p, &world, true);
        assert_eq!(p.pose, PlayerPose::Crouching);
    }
}
