//! Status effects (Java's `MobEffect`s) on players: what potions, and later
//! beacons and food, give. Each has a level (amplifier 0 is level I) and a
//! duration counted in game ticks. Pure logic like the rest of survival;
//! `Vitals` holds the active effects so the host, agents and pad players
//! all have them.

/// Game ticks per second.
const TPS: f32 = 20.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub enum Effect {
    Speed,
    Slowness,
    Strength,
    Weakness,
    InstantHealth,
    InstantDamage,
    Regeneration,
    Poison,
    FireResistance,
    NightVision,
    WaterBreathing,
    JumpBoost,
    SlowFalling,
    Wither,
    Hunger,
}

impl Effect {
    pub const ALL: [Effect; 15] = [
        Effect::Speed,
        Effect::Slowness,
        Effect::Strength,
        Effect::Weakness,
        Effect::InstantHealth,
        Effect::InstantDamage,
        Effect::Regeneration,
        Effect::Poison,
        Effect::FireResistance,
        Effect::NightVision,
        Effect::WaterBreathing,
        Effect::JumpBoost,
        Effect::SlowFalling,
        Effect::Wither,
        Effect::Hunger,
    ];

    /// Java's id, as `/effect` and saves use it.
    pub fn id(self) -> &'static str {
        match self {
            Effect::Speed => "speed",
            Effect::Slowness => "slowness",
            Effect::Strength => "strength",
            Effect::Weakness => "weakness",
            Effect::InstantHealth => "instant_health",
            Effect::InstantDamage => "instant_damage",
            Effect::Regeneration => "regeneration",
            Effect::Poison => "poison",
            Effect::FireResistance => "fire_resistance",
            Effect::NightVision => "night_vision",
            Effect::WaterBreathing => "water_breathing",
            Effect::JumpBoost => "jump_boost",
            Effect::SlowFalling => "slow_falling",
            Effect::Wither => "wither",
            Effect::Hunger => "hunger",
        }
    }

    pub fn from_id(id: &str) -> Option<Effect> {
        let id = id.strip_prefix("minecraft:").unwrap_or(id);
        Effect::ALL.into_iter().find(|e| e.id() == id)
    }

    /// Display name, e.g. "Fire Resistance".
    pub fn name(self) -> &'static str {
        match self {
            Effect::Speed => "Speed",
            Effect::Slowness => "Slowness",
            Effect::Strength => "Strength",
            Effect::Weakness => "Weakness",
            Effect::InstantHealth => "Instant Health",
            Effect::InstantDamage => "Instant Damage",
            Effect::Regeneration => "Regeneration",
            Effect::Poison => "Poison",
            Effect::FireResistance => "Fire Resistance",
            Effect::NightVision => "Night Vision",
            Effect::WaterBreathing => "Water Breathing",
            Effect::JumpBoost => "Jump Boost",
            Effect::SlowFalling => "Slow Falling",
            Effect::Wither => "Wither",
            Effect::Hunger => "Hunger",
        }
    }

    /// Java's effect colour (potion liquid and particles).
    pub fn colour(self) -> [u8; 3] {
        let c = match self {
            Effect::Speed => 0x33EBFF,
            Effect::Slowness => 0x8BAFE0,
            Effect::Strength => 0xFFC700,
            Effect::Weakness => 0x484D48,
            Effect::InstantHealth => 0xF82423,
            Effect::InstantDamage => 0xA9656A,
            Effect::Regeneration => 0xCD5CAB,
            Effect::Poison => 0x87A363,
            Effect::FireResistance => 0xFF9900,
            Effect::NightVision => 0xC2FF66,
            Effect::WaterBreathing => 0x98DAC0,
            Effect::JumpBoost => 0xFDFF84,
            Effect::SlowFalling => 0xF3CFB9,
            Effect::Wither => 0x352A27,
            Effect::Hunger => 0x587653,
        };
        [(c >> 16) as u8, (c >> 8) as u8, c as u8]
    }

    /// Applied once, when given, rather than over time.
    pub fn is_instant(self) -> bool {
        matches!(self, Effect::InstantHealth | Effect::InstantDamage)
    }

    /// Harmful effects show red in tooltips.
    pub fn is_harmful(self) -> bool {
        matches!(
            self,
            Effect::Slowness
                | Effect::Weakness
                | Effect::InstantDamage
                | Effect::Poison
                | Effect::Wither
                | Effect::Hunger
        )
    }
}

/// Roman numerals for effect levels (I..X), like Java's tooltips.
pub fn level_name(amplifier: u8) -> String {
    const ROMAN: [&str; 10] = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X"];
    ROMAN.get(amplifier as usize).map_or_else(|| (amplifier as u32 + 1).to_string(), |r| r.to_string())
}

/// `m:ss` for an effect's remaining ticks.
pub fn duration_text(ticks: u32) -> String {
    let secs = ticks / 20;
    format!("{}:{:02}", secs / 60, secs % 60)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Active {
    pub effect: Effect,
    /// 0 is level I.
    pub amplifier: u8,
    /// Game ticks left.
    pub ticks: u32,
}

/// What instant effects and one tick of the others did.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Outcome {
    pub heal: f32,
    /// Magic damage (instant damage): bypasses armor.
    pub damage: f32,
    /// Poison damage: bypasses armor and never kills.
    pub poison: f32,
    /// Wither damage: bypasses armor and can kill.
    pub wither: f32,
    /// Hunger exhaustion (0.005 per level per tick).
    pub exhaustion: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effects {
    active: Vec<Active>,
    /// Seconds not yet spent on whole ticks.
    carry: f32,
}

impl Effects {
    pub fn iter(&self) -> impl Iterator<Item = &Active> {
        self.active.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }

    pub fn get(&self, effect: Effect) -> Option<&Active> {
        self.active.iter().find(|a| a.effect == effect)
    }

    pub fn has(&self, effect: Effect) -> bool {
        self.get(effect).is_some()
    }

    /// Amplifier + 1 of `effect`, or 0 without it.
    fn levels(&self, effect: Effect) -> u32 {
        self.get(effect).map_or(0, |a| a.amplifier as u32 + 1)
    }

    /// Gives `effect` for `ticks`. Like Java, a stronger level replaces a
    /// weaker one, and the same level only extends the duration. Instant
    /// effects apply at once and return what they did.
    pub fn add(&mut self, effect: Effect, amplifier: u8, ticks: u32) -> Outcome {
        if effect.is_instant() {
            let amount = match effect {
                Effect::InstantHealth => 4.0,
                _ => 6.0,
            } * (1u32 << amplifier.min(30)) as f32;
            return match effect {
                Effect::InstantHealth => Outcome { heal: amount, ..Outcome::default() },
                _ => Outcome { damage: amount, ..Outcome::default() },
            };
        }
        match self.active.iter_mut().find(|a| a.effect == effect) {
            Some(a) if amplifier > a.amplifier || (amplifier == a.amplifier && ticks > a.ticks) => {
                *a = Active { effect, amplifier, ticks };
            }
            Some(_) => {}
            None => self.active.push(Active { effect, amplifier, ticks }),
        }
        self.active.sort_by_key(|a| a.effect);
        Outcome::default()
    }

    pub fn remove(&mut self, effect: Effect) -> bool {
        let before = self.active.len();
        self.active.retain(|a| a.effect != effect);
        self.active.len() != before
    }

    pub fn clear(&mut self) {
        self.active.clear();
    }

    /// Advances `dt` seconds: regeneration heals 1 every 50 >> level ticks
    /// poison hurts 1 every 25 >> level ticks and wither 1 every 40 >> level
    /// (Java's intervals, from the ticks left), then effects run out.
    pub fn tick(&mut self, dt: f32) -> Outcome {
        let mut out = Outcome::default();
        self.carry += dt * TPS;
        let whole = self.carry.floor();
        self.carry -= whole;
        for _ in 0..whole as u32 {
            for a in &mut self.active {
                let every = match a.effect {
                    Effect::Regeneration => 50u32 >> a.amplifier.min(31),
                    Effect::Poison => 25u32 >> a.amplifier.min(31),
                    Effect::Wither => 40u32 >> a.amplifier.min(31),
                    Effect::Hunger => {
                        out.exhaustion += 0.005 * (a.amplifier as f32 + 1.0);
                        continue;
                    }
                    _ => continue,
                };
                if every == 0 || a.ticks.is_multiple_of(every) {
                    match a.effect {
                        Effect::Regeneration => out.heal += 1.0,
                        Effect::Wither => out.wither += 1.0,
                        _ => out.poison += 1.0,
                    }
                }
            }
            for a in &mut self.active {
                a.ticks = a.ticks.saturating_sub(1);
            }
            self.active.retain(|a| a.ticks > 0);
        }
        out
    }

    /// Movement speed multiplier: +20% per level of Speed, -15% per level
    /// of Slowness (Java's attribute modifiers).
    pub fn speed_factor(&self) -> f64 {
        (1.0 + 0.2 * self.levels(Effect::Speed) as f64 - 0.15 * self.levels(Effect::Slowness) as f64).max(0.0)
    }

    /// Extra jump height levels (Jump Boost).
    pub fn jump_boost(&self) -> u32 {
        self.levels(Effect::JumpBoost)
    }

    /// Melee damage added by Strength (+3 per level) and taken away by
    /// Weakness (-4 per level).
    pub fn attack_bonus(&self) -> f32 {
        3.0 * self.levels(Effect::Strength) as f32 - 4.0 * self.levels(Effect::Weakness) as f32
    }

    /// How strongly night vision brightens the view, 0..=1: full, then
    /// flickering over its last 10 seconds like Java's.
    pub fn night_vision(&self, time: f32) -> f32 {
        match self.get(Effect::NightVision) {
            None => 0.0,
            Some(a) if a.ticks > 200 => 1.0,
            Some(a) => 0.7 + (((a.ticks as f32 - time * TPS) * std::f32::consts::PI * 0.2).sin() * 0.3).abs(),
        }
    }

    /// `effect:amplifier:ticks|...` for saves.
    pub fn serialize(&self) -> String {
        self.active
            .iter()
            .map(|a| format!("{}:{}:{}", a.effect.id(), a.amplifier, a.ticks))
            .collect::<Vec<_>>()
            .join("|")
    }

    /// Restores [`Effects::serialize`] output, skipping malformed entries.
    pub fn deserialize(text: &str) -> Self {
        let mut effects = Self::default();
        for entry in text.split('|') {
            let mut parts = entry.split(':');
            if let (Some(e), Some(Ok(amp)), Some(Ok(ticks))) =
                (parts.next().and_then(Effect::from_id), parts.next().map(str::parse), parts.next().map(str::parse))
                && ticks > 0
            {
                effects.add(e, amp, ticks);
            }
        }
        effects
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stronger_levels_replace_and_equal_levels_extend() {
        let mut e = Effects::default();
        e.add(Effect::Speed, 0, 600);
        e.add(Effect::Speed, 0, 100);
        assert_eq!(e.get(Effect::Speed).unwrap().ticks, 600, "shorter same level is ignored");
        e.add(Effect::Speed, 1, 100);
        assert_eq!(e.get(Effect::Speed).map(|a| (a.amplifier, a.ticks)), Some((1, 100)));
        e.add(Effect::Speed, 0, 5000);
        assert_eq!(e.get(Effect::Speed).unwrap().amplifier, 1, "weaker never replaces stronger");
        assert!((e.speed_factor() - 1.4).abs() < 1e-9);
        e.add(Effect::Slowness, 0, 100);
        assert!((e.speed_factor() - 1.25).abs() < 1e-9);
    }

    #[test]
    fn instant_effects_double_per_level() {
        let mut e = Effects::default();
        assert_eq!(e.add(Effect::InstantHealth, 0, 1).heal, 4.0);
        assert_eq!(e.add(Effect::InstantHealth, 1, 1).heal, 8.0);
        assert_eq!(e.add(Effect::InstantDamage, 1, 1).damage, 12.0);
        assert!(e.is_empty());
    }

    #[test]
    fn regeneration_and_poison_follow_java_intervals_then_expire() {
        let mut e = Effects::default();
        // Regeneration I: 45 s heals once per 2.5 s.
        e.add(Effect::Regeneration, 0, 900);
        let healed: f32 = (0..900).map(|_| e.tick(0.05).heal).sum();
        assert_eq!(healed, 18.0);
        assert!(e.is_empty());
        // Poison II: 21.6 s hurts every 12 ticks.
        e.add(Effect::Poison, 1, 432);
        let hurt: f32 = (0..432).map(|_| e.tick(0.05).poison).sum();
        assert_eq!(hurt, 36.0);
        assert!(!e.has(Effect::Poison));
    }

    #[test]
    fn modifiers_and_saves() {
        let mut e = Effects::default();
        e.add(Effect::Strength, 1, 1800);
        e.add(Effect::Weakness, 0, 1800);
        assert_eq!(e.attack_bonus(), 2.0);
        e.add(Effect::NightVision, 0, 150);
        assert!((0.7..=1.0).contains(&e.night_vision(3.0)));
        let back = Effects::deserialize(&e.serialize());
        assert_eq!(back.active, e.active);
        assert_eq!(Effects::deserialize("bogus:1:2|speed:x:3|speed:0:0").active, []);
        assert_eq!(level_name(1), "II");
        assert_eq!(duration_text(3 * 60 * 20 + 20 * 5), "3:05");
    }
}
