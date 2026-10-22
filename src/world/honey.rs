//! HoneyBlock.maybeDoSlideEffects and fallOn feedback, called once after player movement.
use super::World;
use crate::{particles::Request, player::Player};

impl World {
    pub(crate) fn honey_contact_effects(&mut self, player: &Player) {
        if player.honey_landed {
            self.particles.push(Request::Honey { pos: player.pos, count: 10, sound: true });
        } else if player.honey_sliding {
            let sound = self.roll().is_multiple_of(5);
            let count = if self.roll().is_multiple_of(5) { 5 } else { 0 };
            if sound || count != 0 {
                self.particles.push(Request::Honey { pos: player.pos, count, sound });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::world::block::Block;
    use crate::world::mount_states::{collision_is_full, face_sturdy, supports_center};
    use glam::IVec3;

    #[test]
    fn honey_supports_only_from_its_underside_centre() {
        // Java's support shape is the [1,0,1]..[15,15,15] collision box.
        for face in [IVec3::Y, IVec3::NEG_Y, IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z] {
            assert!(!face_sturdy(Block::HONEY_BLOCK, face), "{face}");
            assert_eq!(supports_center(Block::HONEY_BLOCK, face), face == IVec3::NEG_Y, "{face}");
        }
        assert!(!collision_is_full(Block::HONEY_BLOCK));
    }
}
