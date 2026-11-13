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
    Saturation,
    /// Appended (v0.16): the warden's and shriekers' pulsing gloom.
    Darkness,
    /// v0.16 Track C (appended): drinking an ominous bottle; near a trial
    /// spawner it becomes [`Effect::TrialOmen`]. Raid Omen waits for v0.17.
    BadOmen,
    TrialOmen,
    /// Appended: saved by Java id; existing effect mask bits stay stable.
    Invisibility,
    /// Appended (v0.14 EFX-REG-01): the eleven effects Java registers that
    /// the game lacked, in this order, which their icon layers follow.
    /// Registered so `/effect`, saves and the HUD know them; what each one
    /// does arrives with its own slice.
    HealthBoost,
    Glowing,
    Levitation,
    ConduitPower,
    HeroOfTheVillage,
    RaidOmen,
    WindCharged,
    Weaving,
    Oozing,
    Infested,
    BreathOfTheNautilus,
}

/// Java's `MobEffectCategory`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Category {
    Beneficial,
    Harmful,
    Neutral,
}

/// Java's `AttributeModifier.Operation`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Operation {
    AddValue,
    AddMultipliedBase,
    AddMultipliedTotal,
}

/// One of an effect's attribute modifiers, from `MobEffects`: per level,
/// `amount * (amplifier + 1)` (`MobEffect.AttributeTemplate.create`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Modifier {
    /// The attribute's Java id, e.g. `movement_speed`.
    pub attribute: &'static str,
    /// The modifier's Java id, e.g. `minecraft:effect.speed`.
    pub id: &'static str,
    pub amount: f64,
    pub operation: Operation,
}

const fn modifier(attribute: &'static str, id: &'static str, amount: f64, operation: Operation) -> Modifier {
    Modifier { attribute, id, amount, operation }
}

impl Effect {
    pub const ALL: [Effect; 40] = [
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
        Effect::Saturation,
        Effect::Darkness,
        Effect::BadOmen,
        Effect::TrialOmen,
        Effect::Invisibility,
        Effect::HealthBoost,
        Effect::Glowing,
        Effect::Levitation,
        Effect::ConduitPower,
        Effect::HeroOfTheVillage,
        Effect::RaidOmen,
        Effect::WindCharged,
        Effect::Weaving,
        Effect::Oozing,
        Effect::Infested,
        Effect::BreathOfTheNautilus,
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
            Effect::Saturation => "saturation",
            Effect::Darkness => "darkness",
            Effect::BadOmen => "bad_omen",
            Effect::TrialOmen => "trial_omen",
            Effect::Invisibility => "invisibility",
            Effect::HealthBoost => "health_boost",
            Effect::Glowing => "glowing",
            Effect::Levitation => "levitation",
            Effect::ConduitPower => "conduit_power",
            Effect::HeroOfTheVillage => "hero_of_the_village",
            Effect::RaidOmen => "raid_omen",
            Effect::WindCharged => "wind_charged",
            Effect::Weaving => "weaving",
            Effect::Oozing => "oozing",
            Effect::Infested => "infested",
            Effect::BreathOfTheNautilus => "breath_of_the_nautilus",
        }
    }

    pub fn from_id(id: &str) -> Option<Effect> {
        let id = id.strip_prefix("minecraft:").unwrap_or(id);
        Effect::ALL.into_iter().find(|e| e.id() == id)
    }

    /// Display name, e.g. "Fire Resistance" (en_us `effect.minecraft.*`).
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
            Effect::DolphinsGrace => "Dolphin's Grace",
            Effect::Resistance => "Resistance",
            Effect::Haste => "Haste",
            Effect::Blindness => "Blindness",
            Effect::Nausea => "Nausea",
            Effect::Luck => "Luck",
            Effect::BadLuck => "Bad Luck",
            Effect::Absorption => "Absorption",
            Effect::Saturation => "Saturation",
            Effect::Darkness => "Darkness",
            Effect::BadOmen => "Bad Omen",
            Effect::TrialOmen => "Trial Omen",
            Effect::Invisibility => "Invisibility",
            Effect::HealthBoost => "Health Boost",
            Effect::Glowing => "Glowing",
            Effect::Levitation => "Levitation",
            Effect::ConduitPower => "Conduit Power",
            Effect::HeroOfTheVillage => "Hero of the Village",
            Effect::RaidOmen => "Raid Omen",
            Effect::WindCharged => "Wind Charged",
            Effect::Weaving => "Weaving",
            Effect::Oozing => "Oozing",
            Effect::Infested => "Infested",
            Effect::BreathOfTheNautilus => "Breath of the Nautilus",
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
            Effect::Wither => 0x736156,
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
            Effect::Saturation => 0xF82423,
            Effect::Darkness => 0x292721,
            // MobEffects: BAD_OMEN 0x0B6138, TRIAL_OMEN 0x16A6A6.
            Effect::BadOmen => 0x0B6138,
            Effect::TrialOmen => 0x16A6A6,
            Effect::Invisibility => 0xF6F6F6,
            Effect::HealthBoost => 0xF87D23,
            Effect::Glowing => 0x94A061,
            Effect::Levitation => 0xCEFFFF,
            Effect::ConduitPower => 0x1DC2D1,
            Effect::HeroOfTheVillage => 0x44FF44,
            Effect::RaidOmen => 0xDE4058,
            Effect::WindCharged => 0xBDC9FF,
            Effect::Weaving => 0x78695A,
            Effect::Oozing => 0x99FFA3,
            Effect::Infested => 0x8C9B8C,
            Effect::BreathOfTheNautilus => 0x00FFEE,
        };
        [(c >> 16) as u8, (c >> 8) as u8, c as u8]
    }

    /// Healed or dealt by [`Effects::add`] at once rather than kept: Java's
    /// `HealOrHarmMobEffect`. Saturation is instantaneous in Java too (see
    /// [`Effect::is_instantaneous`]) but is kept and applied every tick, as
    /// Java does when a command or food gives it a duration.
    pub fn is_instant(self) -> bool {
        matches!(self, Effect::InstantHealth | Effect::InstantDamage)
    }

    /// Java's `MobEffect.isInstantaneous`: true on `InstantaneousMobEffect`
    /// (Instant Health, Instant Damage and Saturation). `/effect give`
    /// reads its number as ticks for these, and defaults to one tick.
    pub fn is_instantaneous(self) -> bool {
        matches!(self, Effect::InstantHealth | Effect::InstantDamage | Effect::Saturation)
    }

    /// Java's `MobEffectCategory`, from `MobEffects`.
    pub fn category(self) -> Category {
        match self {
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
            | Effect::Darkness
            | Effect::Levitation
            | Effect::WindCharged
            | Effect::Weaving
            | Effect::Oozing
            | Effect::Infested => Category::Harmful,
            Effect::BadOmen | Effect::TrialOmen | Effect::RaidOmen | Effect::Glowing => Category::Neutral,
            Effect::Speed
            | Effect::Strength
            | Effect::InstantHealth
            | Effect::Regeneration
            | Effect::FireResistance
            | Effect::NightVision
            | Effect::WaterBreathing
            | Effect::JumpBoost
            | Effect::SlowFalling
            | Effect::DolphinsGrace
            | Effect::Resistance
            | Effect::Haste
            | Effect::Luck
            | Effect::Absorption
            | Effect::Saturation
            | Effect::Invisibility
            | Effect::HealthBoost
            | Effect::ConduitPower
            | Effect::HeroOfTheVillage
            | Effect::BreathOfTheNautilus => Category::Beneficial,
        }
    }

    /// Harmful effects show red in potion tooltips
    /// (`MobEffectCategory.getTooltipFormatting`); neutral ones are blue,
    /// like beneficial ones.
    pub fn is_harmful(self) -> bool {
        self.category() == Category::Harmful
    }

    /// Java's `MobEffect.isBeneficial`: only the beneficial category. The HUD
    /// (`Hud.extractEffects`) puts every other effect, neutral ones
    /// included, in its second row.
    pub fn is_beneficial(self) -> bool {
        self.category() == Category::Beneficial
    }

    /// The attribute modifiers `MobEffects` registers on the effect.
    pub fn modifiers(self) -> &'static [Modifier] {
        use Operation::*;
        // Java writes several amounts as float literals, widened to doubles.
        const SPEED: [Modifier; 1] =
            [modifier("movement_speed", "minecraft:effect.speed", 0.2f32 as f64, AddMultipliedTotal)];
        const SLOWNESS: [Modifier; 1] =
            [modifier("movement_speed", "minecraft:effect.slowness", -0.15f32 as f64, AddMultipliedTotal)];
        const HASTE: [Modifier; 1] =
            [modifier("attack_speed", "minecraft:effect.haste", 0.1f32 as f64, AddMultipliedTotal)];
        const FATIGUE: [Modifier; 1] =
            [modifier("attack_speed", "minecraft:effect.mining_fatigue", -0.1f32 as f64, AddMultipliedTotal)];
        const STRENGTH: [Modifier; 1] = [modifier("attack_damage", "minecraft:effect.strength", 3.0, AddValue)];
        const WEAKNESS: [Modifier; 1] = [modifier("attack_damage", "minecraft:effect.weakness", -4.0, AddValue)];
        const JUMP: [Modifier; 1] = [modifier("safe_fall_distance", "minecraft:effect.jump_boost", 1.0, AddValue)];
        const INVISIBILITY: [Modifier; 1] = [modifier(
            "waypoint_transmit_range",
            "minecraft:effect.waypoint_transmit_range_hide",
            -1.0,
            AddMultipliedTotal,
        )];
        const HEALTH_BOOST: [Modifier; 1] = [modifier("max_health", "minecraft:effect.health_boost", 4.0, AddValue)];
        const ABSORPTION: [Modifier; 1] =
            [modifier("max_absorption", "minecraft:effect.absorption", ABSORPTION_PER_LEVEL as f64, AddValue)];
        const LUCK: [Modifier; 1] = [modifier("luck", "minecraft:effect.luck", 1.0, AddValue)];
        const UNLUCK: [Modifier; 1] = [modifier("luck", "minecraft:effect.unluck", -1.0, AddValue)];
        match self {
            Effect::Speed => &SPEED,
            Effect::Slowness => &SLOWNESS,
            Effect::Haste => &HASTE,
            Effect::MiningFatigue => &FATIGUE,
            Effect::Strength => &STRENGTH,
            Effect::Weakness => &WEAKNESS,
            Effect::JumpBoost => &JUMP,
            Effect::Invisibility => &INVISIBILITY,
            Effect::HealthBoost => &HEALTH_BOOST,
            Effect::Absorption => &ABSORPTION,
            Effect::Luck => &LUCK,
            Effect::BadLuck => &UNLUCK,
            _ => &[],
        }
    }

    /// `EffectCommands.computeDurationInTicks`: `/effect give` counts
    /// seconds, 30 by default, except for instantaneous effects, whose
    /// number is ticks and which default to one tick.
    pub fn command_duration_ticks(self, seconds: Option<u32>) -> u32 {
        match (seconds, self.is_instantaneous()) {
            (Some(ticks), true) => ticks,
            (Some(seconds), false) => seconds.saturating_mul(20),
            (None, true) => 1,
            (None, false) => 600,
        }
    }
}

/// The row of the HUD's effect icons (`Hud.extractEffects`): 0 for
/// beneficial effects, 1 (26 pixels lower) for harmful and neutral ones.
pub fn hud_row(effect: Effect) -> u8 {
    u8::from(!effect.is_beneficial())
}

/// Roman numerals for effect levels (I..X), like Java's tooltips.
pub fn level_name(amplifier: u8) -> String {
    const ROMAN: [&str; 10] = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X"];
    ROMAN.get(amplifier as usize).map_or_else(|| (amplifier as u32 + 1).to_string(), |r| r.to_string())
}

/// Java's `MobEffectUtil.formatDuration` at 20 ticks a second: `∞`
/// (en_us `effect.duration.infinite`) for an infinite effect, else
/// `StringUtil.formatTickDuration`'s `mm:ss`, or `hh:mm:ss` from an hour.
pub fn duration_text(ticks: u32) -> String {
    if ticks == INFINITE {
        return "\u{221e}".into();
    }
    let seconds = ticks / 20;
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 { format!("{hours:02}:{minutes:02}:{seconds:02}") } else { format!("{minutes:02}:{seconds:02}") }
}

/// The name an effect panel beside the inventory shows
/// (`EffectsInInventory.getEffectName`): the level only for amplifiers 1
/// to 9 (`enchantment.level.2` to `.10`).
pub fn list_label(a: &Active) -> String {
    match a.amplifier {
        1..=9 => format!("{} {}", a.effect.name(), level_name(a.amplifier)),
        _ => a.effect.name().to_string(),
    }
}

/// Java's `MobEffectInstance.INFINITE_DURATION` (-1 in Java). It is kept as
/// the largest tick count so every "ticks left" check reads an infinite
/// effect as a long one rather than one about to run out.
pub const INFINITE: u32 = u32::MAX;

/// `MobEffectInstance.compareTo`'s cut-off: two effects that both last
/// longer than this are ordered by colour alone.
const COMPARE_CUTOFF: i64 = 32147;

/// `MobEffect.AMBIENT_ALPHA` (`Mth.floor(38.25F)`): the alpha of an ambient
/// effect's swirl colour.
pub const AMBIENT_PARTICLE_ALPHA: u8 = 38;

/// One `MobEffectInstance`: the effect, its level and time left, and the
/// three flags Java carries with it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Active {
    pub effect: Effect,
    /// 0 is level I.
    pub amplifier: u8,
    /// Game ticks left, or [`INFINITE`].
    pub ticks: u32,
    /// Java's `ambient`: given by a beacon or conduit. Its swirls are faint
    /// and rarer, and its HUD icon has the ambient frame and never blinks.
    pub ambient: bool,
    /// Java's `visible` (`show_particles`): whether it adds swirls.
    pub particles: bool,
    /// Java's `showIcon`: whether the HUD shows its icon.
    pub show_icon: bool,
}

impl Active {
    /// Java's `MobEffectInstance(effect, duration, amplifier)`: not ambient,
    /// with particles and an icon.
    pub fn new(effect: Effect, amplifier: u8, ticks: u32) -> Self {
        Self { effect, amplifier, ticks, ambient: false, particles: true, show_icon: true }
    }

    /// Java's six-argument constructor's flags.
    pub fn with_flags(self, ambient: bool, particles: bool, show_icon: bool) -> Self {
        Self { ambient, particles, show_icon, ..self }
    }

    /// Java's `isInfiniteDuration`.
    pub fn is_infinite(&self) -> bool {
        self.ticks == INFINITE
    }

    /// Java's `endsWithin`: never for an infinite effect.
    pub fn ends_within(&self, ticks: u32) -> bool {
        !self.is_infinite() && self.ticks <= ticks
    }

    /// The duration as Java stores it: -1 for infinite.
    pub fn java_duration(&self) -> i64 {
        if self.is_infinite() { -1 } else { i64::from(self.ticks) }
    }

    /// Java's private `isShorterDurationThan`.
    fn is_shorter_than(&self, other: &Active) -> bool {
        !self.is_infinite() && (self.ticks < other.ticks || other.is_infinite())
    }

    /// Java's `MobEffectInstance.compareTo`, the order of the inventory's
    /// effect list (the HUD uses its reverse): non-ambient before ambient,
    /// finite before infinite, then shorter first and then by colour; two
    /// effects that both last over 32147 ticks, or two ambient ones, go by
    /// ambient and colour alone.
    pub fn java_cmp(&self, other: &Active) -> std::cmp::Ordering {
        let colour = |a: &Active| {
            let [r, g, b] = a.effect.colour();
            u32::from_be_bytes([0, r, g, b])
        };
        let (d, o) = (self.java_duration(), other.java_duration());
        if (d <= COMPARE_CUTOFF || o <= COMPARE_CUTOFF) && (!self.ambient || !other.ambient) {
            self.ambient
                .cmp(&other.ambient)
                .then(self.is_infinite().cmp(&other.is_infinite()))
                .then(d.cmp(&o))
                .then(colour(self).cmp(&colour(other)))
        } else {
            self.ambient.cmp(&other.ambient).then(colour(self).cmp(&colour(other)))
        }
    }

    /// The HUD icon's alpha (`Hud.extractEffects`): steady for an ambient
    /// effect or one with over 200 ticks left, then a blink that deepens
    /// over the last ten seconds.
    pub fn hud_alpha(&self) -> f32 {
        if self.ambient || !self.ends_within(200) {
            return 1.0;
        }
        let remaining = self.ticks as i32;
        let used_seconds = 10 - remaining / 20;
        let fade = (remaining as f32 / 10.0 / 5.0 * 0.5).clamp(0.0, 0.5);
        // `Mth.cos` reads Java's 65536-entry sine table.
        let blink = crate::knockback::mth_cos(f64::from(remaining as f32 * std::f32::consts::PI / 5.0));
        (fade + blink * (used_seconds as f32 / 10.0 * 0.25).clamp(0.0, 0.25)).clamp(0.0, 1.0)
    }

    /// This instance's swirl colour (`MobEffect.createParticleOptions`):
    /// the effect colour, with alpha 38 when ambient.
    pub fn particle_colour(&self) -> [u8; 4] {
        let [r, g, b] = self.effect.colour();
        [r, g, b, if self.ambient { AMBIENT_PARTICLE_ALPHA } else { 255 }]
    }

    /// A `--effect` start option: `id[,seconds|infinite[,amplifier[,flag...]]]`
    /// with flags `ambient`, `hideparticles` and `hideicon`. Seconds default
    /// to 30.
    pub fn parse_option(spec: &str) -> Result<Active, String> {
        let mut parts = spec.split(',');
        let effect = parts.next().and_then(Effect::from_id).ok_or("bad --effect name")?;
        let ticks = match parts.next() {
            Some("infinite") => INFINITE,
            Some(s) => s.parse::<u32>().map_err(|_| "bad --effect seconds")?.saturating_mul(20).clamp(1, INFINITE - 1),
            None => 600,
        };
        let amplifier = parts.next().map_or(Ok(0), str::parse).map_err(|_| "bad --effect amplifier")?;
        let mut active = Active::new(effect, amplifier, ticks);
        for flag in parts {
            match flag {
                "ambient" => active.ambient = true,
                "hideparticles" => active.particles = false,
                "hideicon" => active.show_icon = false,
                _ => return Err(format!("bad --effect flag {flag}")),
            }
        }
        Ok(active)
    }
}

/// Where the HUD draws each effect icon (`Hud.extractEffects`): the effects
/// in reverse [`Active::java_cmp`] order, skipping those whose icon is
/// hidden, each with its column counted from the right edge (from 1) and
/// its row ([`hud_row`]).
pub fn hud_layout(effects: &Effects) -> Vec<(Active, u32, u8)> {
    let (mut beneficial, mut other) = (0, 0);
    let mut sorted: Vec<Active> = effects.iter().copied().collect();
    // `Ordering.natural().reverse().sortedCopy`: a stable sort on the
    // reversed comparator, not a reversed sort.
    sorted.sort_by(|a, b| b.java_cmp(a));
    sorted
        .into_iter()
        .filter(|a| a.show_icon)
        .map(|a| {
            let column = if a.effect.is_beneficial() { &mut beneficial } else { &mut other };
            *column += 1;
            (a, *column, hud_row(a.effect))
        })
        .collect()
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
    /// Saturation replenishes one food and two saturation per level each tick.
    pub food: u32,
}

/// Java's `MobEffectInstance.update` on the chain `[active, hidden...]` of one
/// effect, starting at `at`. A stronger newcomer replaces the instance (which
/// stays hidden if it lasts longer); a weaker, longer one is kept hidden.
/// Infinite beats any finite duration. The instance then takes the
/// newcomer's particle and icon flags, and its ambient flag when that clears
/// it or anything else changed. Returns whether this instance changed (the
/// hidden chain's own result is ignored, as in Java).
fn merge(chain: &mut Vec<Active>, at: usize, new: Active) -> bool {
    let cur = chain[at];
    let mut changed = false;
    if new.amplifier > cur.amplifier {
        if new.is_shorter_than(&cur) {
            chain.insert(at + 1, cur);
        }
        chain[at].amplifier = new.amplifier;
        chain[at].ticks = new.ticks;
        changed = true;
    } else if cur.is_shorter_than(&new) {
        if new.amplifier == cur.amplifier {
            chain[at].ticks = new.ticks;
            changed = true;
        } else if at + 1 == chain.len() {
            chain.push(new);
        } else {
            merge(chain, at + 1, new);
        }
    }
    let this = &mut chain[at];
    if !new.ambient && this.ambient || changed {
        this.ambient = new.ambient;
        changed = true;
    }
    if new.particles != this.particles {
        this.particles = new.particles;
        changed = true;
    }
    if new.show_icon != this.show_icon {
        this.show_icon = new.show_icon;
        changed = true;
    }
    changed
}

#[derive(Clone, Debug, Default)]
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
    /// Ticks run since these effects were created or loaded: the entity's
    /// `tickCount`, which `MobEffectInstance.tickServer` reads instead of the
    /// duration to time an infinite effect's periodic ticks. Java does not
    /// save `tickCount`, so neither saves nor equality include it.
    age: u32,
}

impl PartialEq for Effects {
    fn eq(&self, other: &Self) -> bool {
        self.active == other.active
            && self.hidden == other.hidden
            && self.carry == other.carry
            && self.absorption == other.absorption
    }
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

    /// Bit set of the active effects, indexed by `Effect as u32`. It stays in
    /// memory (saves and LAN carry Java ids), so widening it changes no format.
    pub fn mask(&self) -> u64 {
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

    /// Gives `effect` for `ticks` (or [`INFINITE`]), not ambient, with
    /// particles and an icon: [`Effects::add_instance`].
    pub fn add(&mut self, effect: Effect, amplifier: u8, ticks: u32) -> Outcome {
        self.add_instance(Active::new(effect, amplifier, ticks)).0
    }

    /// Java's `LivingEntity.addEffect`: a new effect is added, an existing
    /// one merges by `MobEffectInstance.update` (a stronger level replaces a
    /// weaker one, the same level only extends the duration, and the flags
    /// follow the newcomer). Returns what an instant effect did, applied at
    /// once, and whether anything changed (`/effect give` fails when not).
    pub fn add_instance(&mut self, new: Active) -> (Outcome, bool) {
        let Active { effect, amplifier, .. } = new;
        if effect.is_instant() {
            let amount = match effect {
                Effect::InstantHealth => 4.0,
                _ => 6.0,
            } * (1u32 << amplifier.min(30)) as f32;
            let out = match effect {
                Effect::InstantHealth => Outcome { heal: amount, ..Outcome::default() },
                _ => Outcome { damage: amount, ..Outcome::default() },
            };
            return (out, true);
        }
        let changed = match self.active.iter().position(|a| a.effect == effect) {
            Some(head) => {
                let mut chain = vec![self.active[head]];
                chain.extend(self.hidden.iter().filter(|a| a.effect == effect));
                let changed = merge(&mut chain, 0, new);
                self.active[head] = chain[0];
                self.hidden.retain(|a| a.effect != effect);
                self.hidden.extend(chain.into_iter().skip(1));
                changed
            }
            None => {
                self.active.push(new);
                true
            }
        };
        self.active.sort_by_key(|a| a.effect);
        if effect == Effect::Absorption {
            // `AbsorptionMobEffect.onEffectStarted`, run for every instance
            // given, even one that only becomes a hidden effect.
            self.set_absorption(self.absorption.max(((u32::from(amplifier) + 1) * 4) as f32));
        }
        self.clamp_absorption();
        (Outcome::default(), changed)
    }

    /// The active effects in Java's `MobEffectInstance.compareTo` order, as
    /// the inventory's effect list shows them.
    pub fn sorted(&self) -> Vec<Active> {
        let mut sorted = self.active.clone();
        sorted.sort_by(Active::java_cmp);
        sorted
    }

    /// The effects that add swirls (`MobEffectInstance::isVisible`).
    pub fn with_particles(&self) -> impl Iterator<Item = &Active> {
        self.active.iter().filter(|a| a.particles)
    }

    /// Java's `LivingEntity.areAllEffectsAmbient`: no visible effect is
    /// non-ambient. Swirls are then five times rarer.
    pub fn all_ambient(&self) -> bool {
        !self.active.iter().any(|a| a.particles && !a.ambient)
    }

    /// The whole chain of `effect`: the active instance, then its hidden
    /// ones nearest first.
    pub fn chain(&self, effect: Effect) -> Vec<Active> {
        self.get(effect).into_iter().chain(self.hidden.iter().filter(|a| a.effect == effect)).copied().collect()
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
    /// (Java's intervals, from the ticks left, or for an infinite effect from
    /// the ticks run), then finite effects count down and run out.
    pub fn tick(&mut self, dt: f32) -> Outcome {
        let mut out = Outcome::default();
        self.carry += dt * TPS;
        let whole = self.carry.floor();
        self.carry -= whole;
        for _ in 0..whole as u32 {
            // `Entity.commonTick` counts the tick before `tickEffects` runs.
            self.age = self.age.wrapping_add(1);
            // `AbsorptionMobEffect.applyEffectTick` fails once the absorption
            // is spent, which ends the effect (and any hidden instance).
            if self.absorption <= 0.0 {
                self.remove(Effect::Absorption);
            }
            let age = self.age;
            for a in &mut self.active {
                // `tickServer`: an infinite effect times its periodic ticks
                // on the entity's free-running `tickCount`.
                let clock = if a.is_infinite() { age } else { a.ticks };
                let every = match a.effect {
                    Effect::Regeneration => 50u32 >> a.amplifier.min(31),
                    Effect::Poison => 25u32 >> a.amplifier.min(31),
                    Effect::Wither => 40u32 >> a.amplifier.min(31),
                    Effect::Saturation => {
                        out.food += u32::from(a.amplifier) + 1;
                        continue;
                    }
                    Effect::Hunger => {
                        out.exhaustion += 0.005 * (a.amplifier as f32 + 1.0);
                        continue;
                    }
                    _ => continue,
                };
                if every == 0 || clock.is_multiple_of(every) {
                    match a.effect {
                        Effect::Regeneration => out.heal += 1.0,
                        Effect::Wither => out.wither += 1.0,
                        _ => out.poison += 1.0,
                    }
                }
            }
            // `tickDownDuration`: `mapDuration` leaves infinite and zero alone.
            for a in self.active.iter_mut().chain(&mut self.hidden) {
                if !a.is_infinite() {
                    a.ticks = a.ticks.saturating_sub(1);
                }
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
    /// in single precision; exactly 1.0 without Haste. Conduit Power shares
    /// the formula in Java; it is registered but its behaviour waits for v0.16.
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

    /// `effect:amplifier:ticks|...` for saves: the active instances, then
    /// the hidden ones, each effect's nearest first. Ticks are `-1` for an
    /// infinite effect. The active Absorption entry adds the absorption
    /// amount as a fourth field. Flags that differ from the defaults follow:
    /// `a` (ambient), `p0` (no particles) and `i0` (no icon).
    pub fn serialize(&self) -> String {
        let active = self.active.len();
        self.active
            .iter()
            .chain(&self.hidden)
            .enumerate()
            .map(|(i, a)| {
                let mut entry = format!("{}:{}:{}", a.effect.id(), a.amplifier, a.java_duration());
                if a.effect == Effect::Absorption && i < active {
                    entry += &format!(":{}", self.absorption);
                }
                for (set, flag) in [(a.ambient, ":a"), (!a.particles, ":p0"), (!a.show_icon, ":i0")] {
                    if set {
                        entry += flag;
                    }
                }
                entry
            })
            .collect::<Vec<_>>()
            .join("|")
    }

    /// Restores [`Effects::serialize`] output, skipping malformed entries.
    /// An effect's first entry is its active instance and later ones are its
    /// hidden chain, restored as they were rather than merged again (merging
    /// would hand the hidden instances' flags to the active one).
    pub fn deserialize(text: &str) -> Self {
        let mut effects = Self::default();
        let mut absorption = None;
        let mut started = 0.0f32;
        for entry in text.split('|') {
            let mut parts = entry.split(':');
            let (Some(effect), Some(Ok(amplifier)), Some(Ok(ticks))) = (
                parts.next().and_then(Effect::from_id),
                parts.next().map(str::parse),
                parts.next().map(str::parse::<i64>),
            ) else {
                continue;
            };
            let hidden = effects.has(effect);
            let ticks = match ticks {
                -1 => INFINITE,
                // A hidden instance under an infinite one counts down to 0
                // and stays there, as in Java.
                0 if hidden => 0,
                1.. if ticks < i64::from(INFINITE) => ticks as u32,
                _ => continue,
            };
            if effect.is_instant() {
                continue;
            }
            let mut a = Active::new(effect, amplifier, ticks);
            for flag in parts {
                match flag {
                    "a" => a.ambient = true,
                    "p0" => a.particles = false,
                    "i0" => a.show_icon = false,
                    amount if effect == Effect::Absorption && !hidden => {
                        absorption = amount.parse::<f32>().ok().filter(|v| v.is_finite()).or(absorption);
                    }
                    _ => {}
                }
            }
            if effect == Effect::Absorption {
                // Saves without the amount restore `onEffectStarted`'s.
                started = started.max(((u32::from(amplifier) + 1) * 4) as f32);
            }
            if hidden {
                effects.hidden.push(a);
            } else {
                effects.active.push(a);
            }
        }
        effects.active.sort_by_key(|a| a.effect);
        effects.set_absorption(absorption.unwrap_or(started));
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
        assert_eq!(duration_text(3 * 60 * 20 + 20 * 5), "03:05");
        assert_eq!(duration_text(INFINITE), "\u{221e}");
    }
}
