//! Potions: Java's potion types (water, the three bases and every effect
//! potion whose effect exists), each with its effect, level and duration.
//! Every type is its own item (see `Item::potion`); drinking one applies
//! its effect and leaves a glass bottle.

use crate::simulation::effects::Effect;
use crate::simulation::survival::Vitals;

/// A potion type, by index into [`POTIONS`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Potion(pub u8);

pub struct PotionInfo {
    /// Java's id (`long_swiftness`).
    pub id: &'static str,
    /// Item name; long and strong variants say so, as ids can't be typed
    /// otherwise.
    pub name: &'static str,
    /// Effect, amplifier and duration in ticks (instant effects: 1).
    pub effect: Option<(Effect, u8, u32)>,
    pub secondary_effect: Option<(Effect, u8, u32)>,
}

const fn p(id: &'static str, name: &'static str, effect: Option<(Effect, u8, u32)>) -> PotionInfo {
    PotionInfo { id, name, effect, secondary_effect: None }
}

/// Java's durations (ticks): 3:00 / 8:00 for most, 1:30 / 4:00 for
/// slowness, weakness and slow falling, 0:45 / 1:30 for poison and
/// regeneration; strong variants are level II for half as long or less.
pub static POTIONS: [PotionInfo; 39] = [
    p("water", "water bottle", None),
    p("mundane", "mundane potion", None),
    p("thick", "thick potion", None),
    p("awkward", "awkward potion", None),
    p("night_vision", "potion of night vision", Some((Effect::NightVision, 0, 3600))),
    p("long_night_vision", "long potion of night vision", Some((Effect::NightVision, 0, 9600))),
    p("fire_resistance", "potion of fire resistance", Some((Effect::FireResistance, 0, 3600))),
    p("long_fire_resistance", "long potion of fire resistance", Some((Effect::FireResistance, 0, 9600))),
    p("swiftness", "potion of swiftness", Some((Effect::Speed, 0, 3600))),
    p("long_swiftness", "long potion of swiftness", Some((Effect::Speed, 0, 9600))),
    p("strong_swiftness", "strong potion of swiftness", Some((Effect::Speed, 1, 1800))),
    p("slowness", "potion of slowness", Some((Effect::Slowness, 0, 1800))),
    p("long_slowness", "long potion of slowness", Some((Effect::Slowness, 0, 4800))),
    p("strong_slowness", "strong potion of slowness", Some((Effect::Slowness, 3, 400))),
    p("water_breathing", "potion of water breathing", Some((Effect::WaterBreathing, 0, 3600))),
    p("long_water_breathing", "long potion of water breathing", Some((Effect::WaterBreathing, 0, 9600))),
    p("healing", "potion of healing", Some((Effect::InstantHealth, 0, 1))),
    p("strong_healing", "strong potion of healing", Some((Effect::InstantHealth, 1, 1))),
    p("harming", "potion of harming", Some((Effect::InstantDamage, 0, 1))),
    p("strong_harming", "strong potion of harming", Some((Effect::InstantDamage, 1, 1))),
    p("poison", "potion of poison", Some((Effect::Poison, 0, 900))),
    p("long_poison", "long potion of poison", Some((Effect::Poison, 0, 1800))),
    p("strong_poison", "strong potion of poison", Some((Effect::Poison, 1, 432))),
    p("regeneration", "potion of regeneration", Some((Effect::Regeneration, 0, 900))),
    p("long_regeneration", "long potion of regeneration", Some((Effect::Regeneration, 0, 1800))),
    p("strong_regeneration", "strong potion of regeneration", Some((Effect::Regeneration, 1, 450))),
    p("strength", "potion of strength", Some((Effect::Strength, 0, 3600))),
    p("long_strength", "long potion of strength", Some((Effect::Strength, 0, 9600))),
    p("strong_strength", "strong potion of strength", Some((Effect::Strength, 1, 1800))),
    p("weakness", "potion of weakness", Some((Effect::Weakness, 0, 1800))),
    p("long_weakness", "long potion of weakness", Some((Effect::Weakness, 0, 4800))),
    p("leaping", "potion of leaping", Some((Effect::JumpBoost, 0, 3600))),
    p("long_leaping", "long potion of leaping", Some((Effect::JumpBoost, 0, 9600))),
    p("strong_leaping", "strong potion of leaping", Some((Effect::JumpBoost, 1, 1800))),
    p("slow_falling", "potion of slow falling", Some((Effect::SlowFalling, 0, 1800))),
    p("long_slow_falling", "long potion of slow falling", Some((Effect::SlowFalling, 0, 4800))),
    PotionInfo {
        id: "turtle_master",
        name: "potion of the turtle master",
        effect: Some((Effect::Slowness, 3, 400)),
        secondary_effect: Some((Effect::Resistance, 2, 400)),
    },
    PotionInfo {
        id: "long_turtle_master",
        name: "long potion of the turtle master",
        effect: Some((Effect::Slowness, 3, 800)),
        secondary_effect: Some((Effect::Resistance, 2, 800)),
    },
    PotionInfo {
        id: "strong_turtle_master",
        name: "strong potion of the turtle master",
        effect: Some((Effect::Slowness, 5, 400)),
        secondary_effect: Some((Effect::Resistance, 3, 400)),
    },
];

/// Java's colour of water and of the effectless bases.
const WATER_COLOUR: [u8; 3] = [0x38, 0x5D, 0xC6];

impl Potion {
    pub const COUNT: usize = POTIONS.len();
    pub const WATER: Potion = Potion(0);
    pub const MUNDANE: Potion = Potion(1);
    pub const THICK: Potion = Potion(2);
    pub const AWKWARD: Potion = Potion(3);

    pub fn info(self) -> &'static PotionInfo {
        &POTIONS[self.0 as usize]
    }

    pub fn from_id(id: &str) -> Option<Potion> {
        let id = id.strip_prefix("minecraft:").unwrap_or(id);
        POTIONS.iter().position(|p| p.id == id).map(|i| Potion(i as u8))
    }

    pub fn all() -> impl Iterator<Item = Potion> {
        (0..Self::COUNT as u8).map(Potion)
    }

    /// Every effect, preserving their reference registration order.
    pub fn effects(self) -> impl Iterator<Item = (Effect, u8, u32)> {
        [self.info().effect, self.info().secondary_effect].into_iter().flatten()
    }

    /// Java's amplifier-weighted colour mixture, or water for an effectless base.
    pub fn colour(self) -> [u8; 3] {
        let mut colour = [0u32; 3];
        let mut weight = 0;
        for (effect, amplifier, _) in self.effects() {
            let w = u32::from(amplifier) + 1;
            weight += w;
            for (sum, c) in colour.iter_mut().zip(effect.colour()) {
                *sum += u32::from(c) * w;
            }
        }
        if weight == 0 { WATER_COLOUR } else { colour.map(|c| (c / weight) as u8) }
    }

    pub fn drink(self, vitals: &mut Vitals) -> f32 {
        self.effects().map(|(effect, amp, ticks)| vitals.apply_effect(effect, amp, ticks)).sum()
    }

    pub fn describe_lines(self) -> Vec<String> {
        use crate::simulation::effects::{duration_text, level_name};
        let mut lines: Vec<_> = self
            .effects()
            .map(|(effect, amp, ticks)| {
                if effect.is_instant() {
                    format!("{} {}", effect.name(), level_name(amp))
                } else {
                    format!("{} {} ({})", effect.name(), level_name(amp), duration_text(ticks))
                }
            })
            .collect();
        if lines.is_empty() {
            lines.push("No Effects".into());
        }
        lines
    }
    /// Backwards-compatible single-line description for CLI feedback.
    pub fn describe(self) -> String {
        self.describe_lines().join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_and_names_are_unique_and_drinking_applies_effects() {
        let ids: std::collections::HashSet<_> = POTIONS.iter().map(|p| p.id).collect();
        let names: std::collections::HashSet<_> = POTIONS.iter().map(|p| p.name).collect();
        assert_eq!((ids.len(), names.len()), (Potion::COUNT, Potion::COUNT));
        let mut v = Vitals::default();
        Potion::from_id("strong_swiftness").unwrap().drink(&mut v);
        assert_eq!(v.effects.get(Effect::Speed).map(|a| (a.amplifier, a.ticks)), Some((1, 1800)));
        assert_eq!(Potion::from_id("minecraft:harming").unwrap().drink(&mut v), 6.0);
        assert_eq!(Potion::WATER.drink(&mut v), 0.0);
        assert_eq!(Potion::from_id("long_poison").unwrap().describe(), "Poison I (1:30)");
        assert_eq!(Potion::WATER.colour(), WATER_COLOUR);
    }
}
