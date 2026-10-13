//! Shaped blocks in play: doors two blocks tall, ladders hung on walls, and
//! opening and closing doors and fence gates.

use glam::IVec3;

use crate::audio::sounds::Sound;

use super::Game;

impl Game {
    /// Opens or closes a door (both halves) or fence gate through the shared
    /// rule, with its sound. Returns whether it toggled.
    pub(super) fn toggle_door(&mut self, pos: IVec3, forward: glam::Vec3) -> bool {
        let Some(open) = voxelcraft::session::interact::toggle_door(&mut self.world, pos, forward) else {
            return false;
        };
        self.audio.play(Sound::Door(open), Some(pos.as_dvec3() + 0.5), 0.8, (0.9, 1.1));
        true
    }
}
