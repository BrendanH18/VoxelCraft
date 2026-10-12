//! Client presentation for shared bucket and bottle actions.
use crate::audio::{
    Audio,
    sounds::{Material, Sound},
};
use crate::item::Item;
use crate::world::block::Block;
use voxelcraft::survival_items::bucket::{self, Effect};

use super::{Game, REACH};

impl Game {
    pub(super) fn holding_bucket(&self) -> bool {
        self.held_item().is_some_and(|item| item != Item::GLASS_BOTTLE && bucket::handles(item))
    }

    /// Snowballs, eggs and the fishing rod act once per click, like buckets.
    pub(super) fn holding_throwable(&self) -> bool {
        matches!(self.held_item(), Some(Item::SNOWBALL | Item::EGG | Item::FISHING_ROD))
    }

    pub(super) fn use_bucket(&mut self) -> bool {
        let Some(effect) = bucket::use_bucket(
            &self.player,
            &mut self.inventory,
            self.actions.selected,
            self.mode.is_creative(),
            &mut self.world,
            &mut self.mobs.entities,
            REACH,
        ) else {
            return false;
        };
        feedback(&mut self.audio, effect);
        true
    }
}

pub(super) fn feedback(audio: &mut Audio, effect: Effect) {
    let stone = Sound::Place(Material::Stone);
    let (sound, at, gain, pitch) = match effect {
        Effect::Filled { at, fluid } => (if fluid == Block::WATER { Sound::Swim } else { stone }, at, 0.8, (0.8, 0.9)),
        Effect::Poured { at, fluid } => {
            (if fluid == Block::WATER { Sound::Splash } else { stone }, at, 0.6, (0.9, 1.0))
        }
        Effect::Evaporated { at } => (Sound::Fuse, at, 0.6, (1.6, 1.8)),
        Effect::BottleFilled { at } => (Sound::Swim, at, 0.6, (1.3, 1.5)),
        Effect::Creature => return,
    };
    audio.play(sound, Some(at.as_dvec3()), gain, pitch);
}
