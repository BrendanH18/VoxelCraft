//! v1.0 final Track F4: the mobs whose `playStepSound` plays a sound of
//! their own instead of the block's (Java 26.3 `IronGolem`, `Llama`,
//! `Camel`, `Silverfish` and `Turtle`), stepping as Java's `Entity.move`
//! does: `moveDist` grows by 0.6 of each horizontal move on the ground, and
//! a step plays each time it passes `nextStep` (`(int) moveDist + 1`), so
//! one every 1.67 blocks.

use super::MobKind;

/// A mob's own footstep sound.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum StepCue {
    /// `entity.iron_golem.step`, volume 1.
    IronGolem,
    /// `entity.llama.step` (llamas and trader llamas), volume 0.15.
    Llama,
    /// `entity.camel.step`, volume 1.
    Camel,
    /// `entity.camel.step_sand`: on `camel_sand_step_sound_blocks`.
    CamelSand,
    /// `entity.silverfish.step`, volume 0.15.
    Silverfish,
    /// `entity.turtle.shamble`, volume 0.15.
    Turtle,
    /// `entity.turtle.shamble_baby`.
    BabyTurtle,
}

impl StepCue {
    pub const ALL: [StepCue; 7] = [
        StepCue::IronGolem,
        StepCue::Llama,
        StepCue::Camel,
        StepCue::CamelSand,
        StepCue::Silverfish,
        StepCue::Turtle,
        StepCue::BabyTurtle,
    ];

    /// The Java 26.3 sound event.
    pub fn event(self) -> &'static str {
        match self {
            StepCue::IronGolem => "entity.iron_golem.step",
            StepCue::Llama => "entity.llama.step",
            StepCue::Camel => "entity.camel.step",
            StepCue::CamelSand => "entity.camel.step_sand",
            StepCue::Silverfish => "entity.silverfish.step",
            StepCue::Turtle => "entity.turtle.shamble",
            StepCue::BabyTurtle => "entity.turtle.shamble_baby",
        }
    }

    /// The volume the mob's `playStepSound` passes.
    pub fn volume(self) -> f32 {
        match self {
            StepCue::IronGolem | StepCue::Camel | StepCue::CamelSand => 1.0,
            StepCue::Llama | StepCue::Silverfish | StepCue::Turtle | StepCue::BabyTurtle => 0.15,
        }
    }

    /// The step `kind` plays (`sand`: the block under it is in
    /// `camel_sand_step_sound_blocks`), or `None` for the block's own.
    pub fn of(kind: MobKind, baby: bool, sand: bool) -> Option<StepCue> {
        Some(match kind {
            MobKind::IronGolem => StepCue::IronGolem,
            MobKind::Llama | MobKind::TraderLlama => StepCue::Llama,
            MobKind::Camel if sand => StepCue::CamelSand,
            MobKind::Camel => StepCue::Camel,
            MobKind::Silverfish => StepCue::Silverfish,
            MobKind::Turtle if baby => StepCue::BabyTurtle,
            MobKind::Turtle => StepCue::Turtle,
            _ => return None,
        })
    }
}

/// `BlockTags.CAMEL_SAND_STEP_SOUND_BLOCKS` (26.3: `#sand`, which is
/// sand, red sand and suspicious sand, and `#concrete_powders`).
pub fn camel_sand(block: crate::world::block::Block) -> bool {
    use crate::world::block::Block;
    matches!(block, Block::SAND | Block::RED_SAND)
        || crate::world::archaeology::decode(block).is_some_and(|(gravel, _)| !gravel)
        || block.concrete_powder_color().is_some()
}

/// Java's `moveDist` and `nextStep` for one mob; true when a step plays.
pub fn advance(move_dist: &mut f32, next_step: &mut f32, horizontal: f32) -> bool {
    *move_dist += horizontal * 0.6;
    if *move_dist > *next_step {
        *next_step = move_dist.floor() + 1.0;
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_every_five_thirds_of_a_block() {
        let (mut dist, mut next) = (0.0, 0.0);
        let steps = (0..990).filter(|_| advance(&mut dist, &mut next, 0.01)).count();
        // 9.9 blocks: moveDist 5.94; a step passing 0, then each whole number.
        assert_eq!(steps, 6);
        assert_eq!(StepCue::of(MobKind::Turtle, true, false), Some(StepCue::BabyTurtle));
        assert_eq!(StepCue::of(MobKind::Camel, false, true), Some(StepCue::CamelSand));
        assert_eq!(StepCue::of(MobKind::Cow, false, false), None);
        assert!(camel_sand(crate::world::block::Block::RED_SAND));
    }
}
