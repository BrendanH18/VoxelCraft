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
    MiningFatigue,
    DolphinsGrace,
    Resistance,
    /// Appended: effects persist by Java id, but icon layers follow this order.
    Haste,
    Blindness,
    Nausea,
    Luck,
    BadLuck,
    /// Appended: adds `4 * (amplifier + 1)` to the max-absorption attribute.
    Absorption,
}

impl Effect {
    pub const ALL: [Effect; 24] = [
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
        Effect::MiningFatigue,
        Effect::DolphinsGrace,
        Effect::Resistance,
        Effect::Haste,
        Effect::Blindness,
        Effect::Nausea,
        Effect::Luck,
        Effect::BadLuck,
        Effect::Absorption,
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
            Effect::MiningFatigue => "mining_fatigue",
            Effect::DolphinsGrace => "dolphins_grace",
            Effect::Resistance => "resistance",
            Effect::Haste => "haste",
            Effect::Blindness => "blindness",
            Effect::Nausea => "nausea",
            Effect::Luck => "luck",
            Effect::BadLuck => "unluck",
            Effect::Absorption => "absorption",
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
            Effect::MiningFatigue => "Mining Fatigue",
            Effect::DolphinsGrace => "Dolphin’s Grace",
            Effect::Resistance => "Resistance",
            Effect::Haste => "Haste",
            Effect::Blindness => "Blindness",
            Effect::Nausea => "Nausea",
            Effect::Luck => "Luck",
            Effect::BadLuck => "Bad Luck",
            Effect::Absorption => "Absorption",
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
            Effect::MiningFatigue => 0x4A4217,
            Effect::DolphinsGrace => 0x88A3BE,
            Effect::Resistance => 0x9146F0,
            Effect::Haste => 0xD9C043,
            Effect::Blindness => 0x1F1F23,
            Effect::Nausea => 0x551D4A,
            Effect::Luck => 0x59C106,
            Effect::BadLuck => 0xC0A44D,
            Effect::Absorption => 0x2552A5,
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
                | Effect::MiningFatigue
                | Effect::Blindness
                | Effect::Nausea
                | Effect::BadLuck
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

/// Java's `MobEffectInstance.update` on the chain `[active, hidden...]` of one
/// effect, starting at `at`. A stronger newcomer replaces the instance (which
/// stays hidden if it lasts longer); a weaker, longer one is kept hidden.
fn merge(chain: &mut Vec<Active>, at: usize, new: Active) {
    let cur = chain[at];
    if new.amplifier > cur.amplifier {
        if new.ticks < cur.ticks {
            chain.insert(at + 1, cur);
        }
        chain[at].amplifier = new.amplifier;
        chain[at].ticks = new.ticks;
    } else if cur.ticks < new.ticks {
        if new.amplifier == cur.amplifier {
            chain[at].ticks = new.ticks;
        } else if at + 1 == chain.len() {
            chain.push(new);
        } else {
            merge(chain, at + 1, new);
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effects {
    active: Vec<Active>,
    /// Java's `MobEffectInstance.hiddenEffect` chains: weaker or shorter
    /// instances of an active effect, nearest first, kept in order per effect
    /// and restored when the one above them runs out.
    hidden: Vec<Active>,
    /// Seconds not yet spent on whole ticks.
    carry: f32,
    /// Java's `LivingEntity.absorptionAmount`: extra health spent before
    /// health. It lives with the effects because the only source of its cap
    /// (the max-absorption attribute) is the Absorption effect.
    absorption: f32,
}

/// Java 26.3 `MobEffects.ABSORPTION`'s `ADD_VALUE` modifier on the
/// max-absorption attribute, per level.
const ABSORPTION_PER_LEVEL: f32 = 4.0;

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

    /// Bit set of the active effects, indexed by `Effect as u32`.
    pub fn mask(&self) -> u32 {
        self.active.iter().fold(0, |m, a| m | 1 << a.effect as u32)
    }

    /// Java's `getMaxAbsorption`: the attribute's base is 0 and Absorption adds
    /// `4 * (amplifier + 1)` while active.
    pub fn max_absorption(&self) -> f32 {
        self.levels(Effect::Absorption) as f32 * ABSORPTION_PER_LEVEL
    }

    /// Java's `getAbsorptionAmount`.
    pub fn absorption(&self) -> f32 {
        self.absorption
    }

    /// Java's `setAbsorptionAmount`: clamped to `0..=max_absorption`.
    pub fn set_absorption(&mut self, amount: f32) {
        self.absorption = amount.clamp(0.0, self.max_absorption());
    }

    /// The first half of `actuallyHurt` for damage that already went through
    /// armour, resistance and enchantments: absorption soaks up what it can
    /// and the rest, for health, is returned.
    pub fn absorb(&mut self, damage: f32) -> f32 {
        let remaining = (damage - self.absorption).max(0.0);
        self.set_absorption(self.absorption - (damage - remaining));
        remaining
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
        let new = Active { effect, amplifier, ticks };
        match self.active.iter().position(|a| a.effect == effect) {
            Some(head) => {
                let mut chain = vec![self.active[head]];
                chain.extend(self.hidden.iter().filter(|a| a.effect == effect));
                merge(&mut chain, 0, new);
                self.active[head] = chain[0];
                self.hidden.retain(|a| a.effect != effect);
                self.hidden.extend(chain.into_iter().skip(1));
            }
            None => self.active.push(new),
        }
        self.active.sort_by_key(|a| a.effect);
        if effect == Effect::Absorption {
            // `AbsorptionMobEffect.onEffectStarted`, run for every instance
            // given, even one that only becomes a hidden effect.
            self.set_absorption(self.absorption.max(((u32::from(amplifier) + 1) * 4) as f32));
        }
        self.clamp_absorption();
        Outcome::default()
    }

    /// Java clamps the absorption amount whenever its attribute changes.
    fn clamp_absorption(&mut self) {
        self.set_absorption(self.absorption);
    }

    pub fn remove(&mut self, effect: Effect) -> bool {
        let before = self.active.len();
        self.active.retain(|a| a.effect != effect);
        self.hidden.retain(|a| a.effect != effect);
        self.clamp_absorption();
        self.active.len() != before
    }

    pub fn clear(&mut self) {
        self.active.clear();
        self.hidden.clear();
        self.absorption = 0.0;
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
            // `AbsorptionMobEffect.applyEffectTick` fails once the absorption
            // is spent, which ends the effect (and any hidden instance).
            if self.absorption <= 0.0 {
                self.remove(Effect::Absorption);
            }
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
            for a in self.active.iter_mut().chain(&mut self.hidden) {
                a.ticks = a.ticks.saturating_sub(1);
            }
            // Java's `downgradeToHiddenEffect`: an expired effect gives way to
            // the next hidden instance of itself.
            for a in &mut self.active {
                if a.ticks == 0
                    && let Some(at) = self.hidden.iter().position(|h| h.effect == a.effect)
                {
                    *a = self.hidden.remove(at);
                }
            }
            self.active.retain(|a| a.ticks > 0);
            self.clamp_absorption();
        }
        out
    }

    /// Movement speed multiplier: Speed and Slowness are each an
    /// `ADD_MULTIPLIED_TOTAL` attribute modifier (+0.2 and -0.15 per level, floats
    /// widened to doubles), so their effects multiply (Java's `MobEffects`).
    pub fn speed_factor(&self) -> f64 {
        (1.0 + crate::knockback::SPEED_PER_LEVEL * self.levels(Effect::Speed) as f64).max(0.0)
            * (1.0 + crate::knockback::SLOWNESS_PER_LEVEL * self.levels(Effect::Slowness) as f64).max(0.0)
    }

    /// The Speed and Slowness levels (0 when absent), for `Player.getSpeed`.
    pub fn movement_speed_levels(&self) -> (u32, u32) {
        (self.levels(Effect::Speed), self.levels(Effect::Slowness))
    }

    /// Java's fatigue multipliers for mining speed.
    pub fn resistance_factor(&self) -> f32 {
        self.get(Effect::Resistance).map_or(1.0, |e| (1.0 - 0.2 * (f32::from(e.amplifier) + 1.0)).max(0.0))
    }

    /// `Player.getDestroySpeed`'s Haste multiplier, `1 + (amplifier + 1) * 0.2`
    /// in single precision; exactly 1.0 without Haste. Conduit Power, which
    /// shares the formula, is not registered.
    pub fn dig_speed_factor(&self) -> f32 {
        self.get(Effect::Haste).map_or(1.0, |a| 1.0 + (i32::from(a.amplifier) + 1) as f32 * 0.2)
    }

    /// The Luck attribute the effects add: +1 per Luck level, -1 per Bad Luck level.
    pub fn luck(&self) -> i32 {
        self.levels(Effect::Luck) as i32 - self.levels(Effect::BadLuck) as i32
    }

    /// Java's `Player.isMobilityRestricted`: Blindness stops criticals and sprinting.
    pub fn mobility_restricted(&self) -> bool {
        self.has(Effect::Blindness)
    }

    /// The player's attack-speed attribute multiplier: Haste adds 10% per
    /// level and Mining Fatigue removes 10% per level, each an
    /// `ADD_MULTIPLIED_TOTAL` modifier of `amount * (amplifier + 1)` whose
    /// amount is a float widened to a double, as in `MobEffects`.
    pub fn attack_speed_factor(&self) -> f64 {
        let modifier = |effect, amount: f32| {
            self.get(effect).map_or(1.0, |a| 1.0 + f64::from(amount) * (f64::from(a.amplifier) + 1.0))
        };
        modifier(Effect::Haste, 0.1) * modifier(Effect::MiningFatigue, -0.1)
    }

    pub fn mining_factor(&self) -> f32 {
        self.get(Effect::MiningFatigue).map_or(1.0, |a| 0.3f64.powf(f64::from(a.amplifier) + 1.) as f32)
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

    /// `effect:amplifier:ticks|...` for saves. The active Absorption entry
    /// carries the absorption amount as a fourth field.
    pub fn serialize(&self) -> String {
        let active = self.active.len();
        self.active
            .iter()
            .chain(&self.hidden)
            .enumerate()
            .map(|(i, a)| match a.effect {
                Effect::Absorption if i < active => {
                    format!("{}:{}:{}:{}", a.effect.id(), a.amplifier, a.ticks, self.absorption)
                }
                _ => format!("{}:{}:{}", a.effect.id(), a.amplifier, a.ticks),
            })
            .collect::<Vec<_>>()
            .join("|")
    }

    /// Restores [`Effects::serialize`] output, skipping malformed entries.
    pub fn deserialize(text: &str) -> Self {
        let mut effects = Self::default();
        let mut absorption = None;
        for entry in text.split('|') {
            let mut parts = entry.split(':');
            if let (Some(e), Some(Ok(amp)), Some(Ok(ticks))) =
                (parts.next().and_then(Effect::from_id), parts.next().map(str::parse), parts.next().map(str::parse))
                && ticks > 0
            {
                effects.add(e, amp, ticks);
                if e == Effect::Absorption
                    && let Some(Ok(amount)) = parts.next().map(str::parse::<f32>)
                    && amount.is_finite()
                {
                    absorption = Some(amount);
                }
            }
        }
        if let Some(amount) = absorption {
            effects.set_absorption(amount);
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
        // The modifier amounts are floats widened to doubles (0.2f is 0.20000000298...).
        assert!((e.speed_factor() - 1.4).abs() < 1e-6);
        e.add(Effect::Slowness, 0, 100);
        // Java multiplies the two modifiers: (1 + 2 * 0.2) * (1 - 0.15), not 1 + 0.4 - 0.15.
        assert!((e.speed_factor() - 1.4 * 0.85).abs() < 1e-6);
    }

    /// Java's `MobEffectInstance.hiddenEffect`: a stronger, shorter effect
    /// hides the weaker one, which comes back when the stronger one ends.
    #[test]
    fn a_weaker_longer_effect_returns_after_the_stronger_one_expires() {
        let mut e = Effects::default();
        e.add(Effect::Speed, 0, 600);
        e.add(Effect::Speed, 1, 100);
        assert_eq!(e.get(Effect::Speed).map(|a| (a.amplifier, a.ticks)), Some((1, 100)));
        for _ in 0..99 {
            e.tick(0.05);
        }
        assert_eq!(e.get(Effect::Speed).map(|a| (a.amplifier, a.ticks)), Some((1, 1)));
        e.tick(0.05);
        assert_eq!(e.get(Effect::Speed).map(|a| (a.amplifier, a.ticks)), Some((0, 500)));
        // Saves keep the hidden instance.
        e.add(Effect::Speed, 2, 50);
        let copy = Effects::deserialize(&e.serialize());
        assert_eq!(copy, e);
        e.remove(Effect::Speed);
        assert!(e.is_empty() && e.serialize().is_empty());
        // A stronger, longer effect simply replaces: nothing to restore.
        e.add(Effect::Speed, 0, 100);
        e.add(Effect::Speed, 1, 600);
        for _ in 0..600 {
            e.tick(0.05);
        }
        assert!(e.is_empty());
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

    /// Java 26.3 `AbsorptionMobEffect` and `MobEffects.ABSORPTION`: a 4.0
    /// `ADD_VALUE` modifier per level on `max_absorption`, and
    /// `onEffectStarted` raises the amount to `4 * (amplifier + 1)`.
    #[test]
    fn absorption_starts_at_four_per_level_and_never_exceeds_the_attribute() {
        let mut e = Effects::default();
        assert_eq!((e.absorption(), e.max_absorption()), (0.0, 0.0));
        e.set_absorption(5.0);
        assert_eq!(e.absorption(), 0.0, "no attribute, no absorption");
        e.add(Effect::Absorption, 0, 2400);
        assert_eq!((e.absorption(), e.max_absorption()), (4.0, 4.0));
        e.set_absorption(100.0);
        assert_eq!(e.absorption(), 4.0);
        e.set_absorption(-3.0);
        assert_eq!(e.absorption(), 0.0);
        // Absorption IV (the enchanted golden apple): 16.
        e.add(Effect::Absorption, 3, 2400);
        assert_eq!((e.absorption(), e.max_absorption()), (16.0, 16.0));
        // `Math.max(current, 4 * (1 + amplifier))`: a weaker apple adds nothing.
        e.set_absorption(10.0);
        e.add(Effect::Absorption, 0, 2400);
        assert_eq!(e.absorption(), 10.0);
        assert_eq!(e.get(Effect::Absorption).map(|a| a.amplifier), Some(3));
        assert!(Effect::Absorption.id() == "absorption" && !Effect::Absorption.is_harmful());
        assert_eq!(Effect::Absorption.colour(), [0x25, 0x52, 0xA5], "MobEffects.ABSORPTION colour 2445989");
    }

    /// `LivingEntity.actuallyHurt`: `remaining = max(damage - absorption, 0)`
    /// and absorption loses `damage - remaining`.
    #[test]
    fn absorb_spends_absorption_before_health() {
        let mut e = Effects::default();
        e.add(Effect::Absorption, 1, 2400);
        assert_eq!(e.absorb(3.0), 0.0);
        assert_eq!(e.absorption(), 5.0);
        assert_eq!(e.absorb(5.0), 0.0);
        assert_eq!(e.absorption(), 0.0);
        e.add(Effect::Absorption, 1, 2400);
        assert_eq!(e.absorb(9.5), 1.5);
        assert_eq!(e.absorption(), 0.0);
        assert_eq!(e.absorb(2.0), 2.0, "nothing left to absorb");
    }

    /// `AbsorptionMobEffect.applyEffectTick` returns `absorption > 0`, and a
    /// false tick removes the instance.
    #[test]
    fn the_effect_ends_on_the_tick_after_its_absorption_is_spent() {
        let mut e = Effects::default();
        e.add(Effect::Absorption, 0, 2400);
        e.tick(0.05);
        assert!(e.has(Effect::Absorption));
        e.absorb(4.0);
        assert!(e.has(Effect::Absorption), "still listed until the next tick");
        e.tick(0.05);
        assert!(!e.has(Effect::Absorption) && e.is_empty());
        assert!(e.serialize().is_empty());
    }

    #[test]
    fn expiry_and_removal_clear_the_absorption_amount() {
        let mut e = Effects::default();
        e.add(Effect::Absorption, 0, 40);
        for _ in 0..39 {
            e.tick(0.05);
        }
        assert_eq!(e.absorption(), 4.0);
        e.tick(0.05);
        assert!(e.is_empty());
        assert_eq!(e.absorption(), 0.0, "removing the modifier clamps the amount to the new max of 0");
        // Milk and /effect clear.
        e.add(Effect::Absorption, 3, 2400);
        e.clear();
        assert_eq!((e.absorption(), e.max_absorption()), (0.0, 0.0));
        e.add(Effect::Absorption, 3, 2400);
        assert!(e.remove(Effect::Absorption));
        assert_eq!(e.absorption(), 0.0);
        // A hidden weaker instance resurfacing lowers the cap, and the amount with it.
        e.add(Effect::Absorption, 3, 20);
        e.add(Effect::Absorption, 0, 2400);
        assert_eq!(e.absorption(), 16.0);
        for _ in 0..20 {
            e.tick(0.05);
        }
        assert_eq!(e.get(Effect::Absorption).map(|a| (a.amplifier, a.ticks)), Some((0, 2380)));
        assert_eq!((e.absorption(), e.max_absorption()), (4.0, 4.0));
    }

    /// The amount is saved on the active entry as a fourth field.
    #[test]
    fn absorption_amount_survives_a_save() {
        let mut e = Effects::default();
        e.add(Effect::Absorption, 3, 1800);
        e.add(Effect::Speed, 0, 600);
        e.absorb(4.5);
        assert_eq!(e.absorption(), 11.5);
        let text = e.serialize();
        assert_eq!(text, "speed:0:600|absorption:3:1800:11.5");
        let back = Effects::deserialize(&text);
        assert_eq!(back, e);
        assert_eq!((back.absorption(), back.max_absorption()), (11.5, 16.0));
        // A hidden weaker instance keeps its place and the amount stays on the active one.
        e.add(Effect::Absorption, 0, 2400);
        let back = Effects::deserialize(&e.serialize());
        assert_eq!(back, e);
        // Older text without the amount restores the effect's starting amount; bad amounts clamp or drop.
        assert_eq!(Effects::deserialize("absorption:1:100").absorption(), 8.0);
        assert_eq!(Effects::deserialize("absorption:1:100:99").absorption(), 8.0);
        assert_eq!(Effects::deserialize("absorption:1:100:-4").absorption(), 0.0);
        assert_eq!(Effects::deserialize("absorption:1:100:NaN").absorption(), 8.0);
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
