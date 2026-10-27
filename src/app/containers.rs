//! Opening chest screens. Clicks and shift-click quick moves are screen
//! transactions (`screen::Screen`).

use glam::{DVec3, IVec3};

use crate::audio::sounds::{Material, Sound};

use super::{Container, Game};

impl Game {
    pub(super) fn open_container_count(&self) -> usize {
        match self.body.container {
            Container::Chest(p) => self.world.container_slots(p),
            Container::Minecart(id) => self.mobs.entities.vehicle_slots(id).map_or(0, |s| s.len()),
            _ => 0,
        }
    }

    /// Right-click on a chest: its 27 slots above the inventory.
    pub(super) fn open_chest(&mut self, pos: IVec3) {
        if self.inventory_open || self.world.chest(pos).is_none() {
            return;
        }
        self.body.container = Container::Chest(pos);
        self.toggle_inventory();
        self.world.chest_opened(pos);
        self.chest_sound(pos, 0.65);
        if self.world.get_block(pos).is_some_and(crate::entity::nether::guarded_by_piglins) {
            self.piglins_notice(true);
        }
    }

    /// Java's `angerNearbyPiglins`: piglins around the player turn on them
    /// for opening a chest (only the ones that see it) or breaking gold.
    pub(super) fn piglins_notice(&mut self, only_if_seen: bool) {
        let (actor, at) = (self.session.id, self.session.player.pos);
        self.mobs.entities.piglins_notice(actor, at, only_if_seen, &self.world);
    }

    pub(super) fn chest_sound(&mut self, pos: IVec3, pitch: f32) {
        let at = pos.as_dvec3() + DVec3::splat(0.5);
        self.audio.play(Sound::Place(Material::Wood), Some(at), 0.7, (pitch, pitch + 0.08));
    }
}
