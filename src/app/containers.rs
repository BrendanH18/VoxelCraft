//! Opening chest screens. Clicks and shift-click quick moves are screen
//! transactions (`screen::Screen`). v0.18 Track 9: lids and their sounds
//! follow the containers' viewers (`World::set_container_viewers`), so every
//! input path and LAN player opens them alike; see [`Game::play_lid_sounds`].

use glam::{DVec3, IVec3};

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

    /// v0.18 Track 9: the containers' open and close sounds since the last
    /// tick (`CHEST_OPEN`/`CLOSE` and the ender chest, copper chest and
    /// barrel forms: volume 0.5, pitch 0.9 to 1.0, `BLOCKS`).
    pub(super) fn play_lid_sounds(&mut self) {
        use crate::audio::sounds::{ContainerCue as C, Sound};
        use crate::world::copper::Stage;
        use crate::world::lids::Container;
        for cue in self.world.take_lid_cues() {
            let (open, close) = match cue.container {
                Container::Chest => (C::ChestOpen, C::ChestClose),
                Container::EnderChest => (C::EnderChestOpen, C::EnderChestClose),
                Container::Barrel => (C::BarrelOpen, C::BarrelClose),
                // `CopperChestBlock`: unaffected and exposed chests share sounds.
                Container::CopperChest(Stage::Weathered) => (C::CopperChestWeatheredOpen, C::CopperChestWeatheredClose),
                Container::CopperChest(Stage::Oxidized) => (C::CopperChestOxidizedOpen, C::CopperChestOxidizedClose),
                Container::CopperChest(_) => (C::CopperChestOpen, C::CopperChestClose),
            };
            let sound = Sound::Container(if cue.open { open } else { close });
            self.audio.play(sound, Some(cue.pos), 0.5, (0.9, 1.0));
        }
    }

    /// v1.0 final track F3: the bells shaking after a ring, drawn as
    /// block-entity models with the body swung (their chunk mesh leaves
    /// them out).
    pub(super) fn bell_models(&self, alpha: f64) -> impl Iterator<Item = crate::render::BlockModel> + '_ {
        self.world.bell_shakes().filter_map(move |(cell, shake)| {
            let block = self.world.get_block(cell)?;
            let centre = cell.as_dvec3() + DVec3::splat(0.5);
            Some(crate::render::BlockModel {
                min: cell.as_dvec3(),
                size: 1.0,
                block,
                sky_light: crate::entity::sky_light(&self.world, centre),
                block_light: self.torch_light(centre),
                yaw: 0.0,
                icon: None,
                orient: None,
                tint: [255; 4],
                glint: false,
                translucent: false,
                mesh: None,
                transform: None,
                patterns: None,
                lid: None,
                swing: Some(shake.swing(alpha as f32)),
            })
        })
    }

    /// v0.18 Track 9: the containers in use or with a lid still moving,
    /// drawn as block-entity models (their chunk mesh leaves them out).
    pub(super) fn lid_models(&self, alpha: f64) -> impl Iterator<Item = crate::render::BlockModel> + '_ {
        self.world.lidded().filter_map(move |(cell, lid)| {
            let block = self.world.get_block(cell)?;
            let centre = cell.as_dvec3() + DVec3::splat(0.5);
            Some(crate::render::BlockModel {
                min: cell.as_dvec3(),
                size: 1.0,
                block,
                sky_light: crate::entity::sky_light(&self.world, centre),
                block_light: self.torch_light(centre),
                yaw: 0.0,
                icon: None,
                orient: None,
                tint: [255; 4],
                glint: false,
                translucent: false,
                mesh: None,
                transform: None,
                patterns: None,
                lid: Some(lid.at(alpha as f32)),
                swing: None,
            })
        })
    }
}
