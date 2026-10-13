//! Client presentation for shared bucket and bottle actions.
use crate::audio::{
    Audio,
    sounds::{Material, Sound},
};
use crate::world::block::Block;
use voxelcraft::survival_items::bucket::Effect;

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
