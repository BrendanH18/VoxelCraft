//! Shaped blocks in play: doors two blocks tall, ladders hung on walls, and
//! opening and closing doors and fence gates.

use glam::IVec3;

use crate::audio::sounds::Sound;
use crate::world::block::{Facing, Shaped};

use super::Game;

impl Game {
    /// Right-click on a door or fence gate opens or closes it (both halves
    /// of a door) for a player looking along `forward`. Returns whether it did.
    pub(super) fn toggle_door(&mut self, pos: IVec3, forward: glam::Vec3) -> bool {
        let Some(b) = self.world.get_block(pos) else { return false };
        if matches!(
            voxelcraft::world::redstone_blocks::component(b),
            Some(voxelcraft::world::redstone_blocks::Component::IronDoor { .. })
        ) {
            return true;
        }
        let open = match b.shaped() {
            Some(Shaped::Door { mut facing, mut open, upper }) => {
                let other = if upper { pos - IVec3::Y } else { pos + IVec3::Y };
                let partner = self.world.get_block(other).filter(|o| o.is_door());
                // A wood door's upper half does not store facing, so the
                // lower half is the one that flips.
                if upper
                    && let Some(lower) = partner
                    && let Some(Shaped::Door { facing: f, open: o, upper: false }) = lower.shaped()
                {
                    facing = f;
                    open = o;
                }
                self.world.set_block(pos, b.toggled(facing));
                if let Some(o) = partner {
                    self.world.set_block(other, o.toggled(facing));
                }
                !open
            }
            Some(Shaped::Gate { facing, open }) => {
                // A gate swings away from whoever opens it.
                let toward = Facing::toward(forward);
                let facing = if !open && toward.along_x() == facing.along_x() { toward } else { facing };
                self.world.set_block(pos, b.toggled(facing));
                !open
            }
            _ => return false,
        };
        self.audio.play(Sound::Door(open), Some(pos.as_dvec3() + 0.5), 0.8, (0.9, 1.1));
        true
    }
}
