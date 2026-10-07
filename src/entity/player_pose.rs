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
    pub const STANDING_HEIGHT: f64 = 1.8;
    pub const CROUCH_HEIGHT: f64 = 1.5;
    pub const SWIM_HEIGHT: f64 = 0.6;
    pub const HALF_WIDTH: f64 = 0.3;

    pub const STANDING_EYE: f64 = 1.62;
    pub const CROUCH_EYE: f64 = 1.27;
    pub const SWIM_EYE: f64 = 0.4;

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

pub fn fits_at<W: BlockSource + ?Sized>(world: &W, pos: DVec3, pose: PlayerPose) -> bool {
    !physics::overlaps_solid(world, pos, pose.shape())
}

/// Java `Player.updatePlayerPose` when the swimming box fits at the feet.
pub fn resolve_pose<W: BlockSource + ?Sized>(player: &Player, world: &W, shift: bool) -> PlayerPose {
    if !fits_at(world, player.pos, PlayerPose::Swimming) {
        return player.pose;
    }
    let desired = if player.swimming {
        PlayerPose::Swimming
    } else if shift && !player.flying {
        PlayerPose::Crouching
    } else {
        PlayerPose::Standing
    };
    if fits_at(world, player.pos, desired) {
        desired
    } else if fits_at(world, player.pos, PlayerPose::Crouching) {
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
        assert!((PlayerPose::Standing.shape().height - 1.8).abs() < 1e-9);
        assert!((PlayerPose::Crouching.shape().height - 1.5).abs() < 1e-9);
        assert!((PlayerPose::Swimming.shape().height - 0.6).abs() < 1e-9);
        assert!((PlayerPose::Crouching.eye_height() - 1.27).abs() < 1e-9);
        assert!((PlayerPose::Swimming.eye_height() - 0.4).abs() < 1e-9);
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
