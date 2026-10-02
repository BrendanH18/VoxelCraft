//! Buckets: scoop up a water or lava source and pour it out elsewhere.

use crate::audio::sounds::{Material, Sound};
use crate::inventory::Stack;
use crate::item::Item;
use crate::world::block::Block;

use super::{Game, GameMode, REACH};

impl Game {
    pub(super) fn holding_bucket(&self) -> bool {
        matches!(self.held_item(), Some(Item::BUCKET | Item::WATER_BUCKET | Item::LAVA_BUCKET))
    }

    /// Right-click with a bucket. Returns whether the bucket was used.
    pub(super) fn use_bucket(&mut self) -> bool {
        match self.held_item() {
            Some(Item::BUCKET) => self.fill_bucket(),
            Some(Item::WATER_BUCKET) => self.empty_bucket(Block::WATER),
            Some(Item::LAVA_BUCKET) => self.empty_bucket(Block::LAVA),
            _ => false,
        }
    }

    /// Takes the source block under the crosshair into the held bucket.
    fn fill_bucket(&mut self) -> bool {
        let hit = self.world.raycast_sources(self.player.eye(), self.player.forward().as_dvec3(), REACH);
        let Some((pos, _)) = hit else { return false };
        let filled = match self.world.get_block(pos) {
            Some(Block::WATER) => Item::WATER_BUCKET,
            Some(Block::LAVA) => Item::LAVA_BUCKET,
            _ => return false,
        };
        self.world.set_block(pos, Block::AIR);
        let sound = if filled == Item::WATER_BUCKET { Sound::Swim } else { Sound::Place(Material::Stone) };
        self.audio.play(sound, Some(pos.as_dvec3()), 0.8, (0.8, 0.9));
        // Creative keeps its empty bucket.
        if self.mode == GameMode::Survival {
            let slot = self.actions.selected;
            if self.inventory.slots[slot].is_some_and(|s| s.count == 1) {
                self.inventory.slots[slot] = Some(Stack::new(filled, 1));
            } else {
                self.inventory.take_one(slot);
                let left = self.inventory.add(filled, 1);
                if left > 0 {
                    let (eye, dir) = (self.player.eye(), self.player.forward().as_dvec3());
                    self.mobs.entities.throw(Stack::new(filled, left), eye, dir);
                }
            }
        }
        true
    }

    /// Pours the held bucket's fluid against the face under the crosshair.
    fn empty_bucket(&mut self, fluid: Block) -> bool {
        let Some((pos, normal)) = self.target() else { return false };
        let replace = self.world.get_block(pos).is_some_and(|b| b.is_replaceable());
        let at = if replace { pos } else { pos + normal };
        if !self.world.get_block(at).is_some_and(|b| b.is_replaceable()) {
            return false;
        }
        if fluid == Block::WATER && self.dimension == crate::world::terrain::Dimension::Nether {
            // Water boils away in the Nether.
            self.audio.play(Sound::Fuse, Some(at.as_dvec3()), 0.6, (1.6, 1.8));
        } else {
            self.world.set_block(at, fluid);
            let sound = if fluid == Block::WATER { Sound::Splash } else { Sound::Place(Material::Stone) };
            self.audio.play(sound, Some(at.as_dvec3()), 0.6, (0.9, 1.0));
        }
        if self.mode == GameMode::Survival {
            self.inventory.slots[self.actions.selected] = Some(Stack::new(Item::BUCKET, 1));
        }
        true
    }
}
