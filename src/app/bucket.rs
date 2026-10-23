//! Client presentation for shared bucket and bottle actions.
use crate::audio::{
    Audio, SoundSource,
    sounds::{Material, Sound},
};
use crate::world::block::Block;
use voxelcraft::survival_items::bucket::Effect;

pub(super) fn feedback(audio: &mut Audio, effect: Effect) {
    // Powder snow's `BUCKET_FILL/EMPTY_POWDER_SNOW` use the nearest shared material.
    let solid = |fluid: Block| {
        Sound::Place(if fluid == crate::world::scaffolding::POWDER_SNOW { Material::Snow } else { Material::Stone })
    };
    // Java plays bucket and fire sounds in `BLOCKS` and bottle filling in `NEUTRAL`.
    let mut source = SoundSource::Blocks;
    let (sound, at, gain, pitch) = match effect {
        Effect::Filled { at, fluid } => {
            (if fluid == Block::WATER { Sound::Swim } else { solid(fluid) }, at, 0.8, (0.8, 0.9))
        }
        Effect::Poured { at, fluid } => {
            (if fluid == Block::WATER { Sound::Splash } else { solid(fluid) }, at, 0.6, (0.9, 1.0))
        }
        Effect::Released { at } => (Sound::Splash, at, 0.5, (1.2, 1.35)),
        Effect::Evaporated { at } => (Sound::Fuse, at, 0.6, (1.6, 1.8)),
        Effect::BottleFilled { at } => {
            source = SoundSource::Neutral;
            (Sound::Swim, at, 0.6, (1.3, 1.5))
        }
        Effect::Creature => return,
    };
    audio.play_in(source, sound, Some(at.as_dvec3()), gain, pitch);
}
