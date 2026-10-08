//! Saved world and player rules shared by graphical and headless sessions.

use std::fmt;

/// Java Edition's four per-player game modes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameMode {
    #[default]
    Survival,
    Creative,
    Adventure,
    Spectator,
}

impl GameMode {
    pub const ALL: [Self; 4] = [Self::Survival, Self::Creative, Self::Adventure, Self::Spectator];

    /// Lowercase Java name used by saves and commands.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Survival => "survival",
            Self::Creative => "creative",
            Self::Adventure => "adventure",
            Self::Spectator => "spectator",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.name() == name.to_ascii_lowercase())
    }

    /// Creative inventory and instant building.
    pub const fn is_creative(self) -> bool {
        matches!(self, Self::Creative)
    }

    /// Health, hunger, drops, durability and experience behave as Survival.
    pub const fn is_survival(self) -> bool {
        matches!(self, Self::Survival | Self::Adventure)
    }

    /// Creative and Spectator players cannot be damaged.
    pub const fn invulnerable(self) -> bool {
        matches!(self, Self::Creative | Self::Spectator)
    }

    pub const fn can_fly(self) -> bool {
        matches!(self, Self::Creative | Self::Spectator)
    }

    /// Adventure needs CanDestroy/CanPlaceOn data, which stacks do not yet
    /// carry, so it cannot alter blocks.
    pub const fn can_build(self) -> bool {
        matches!(self, Self::Survival | Self::Creative)
    }

    /// Spectators cannot interact with blocks, entities or inventories.
    pub const fn can_interact(self) -> bool {
        !matches!(self, Self::Spectator)
    }

    /// Hostile mobs cannot see Creative or Spectator players.
    pub const fn targetable(self) -> bool {
        self.is_survival()
    }
}

impl fmt::Display for GameMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Survival => "Survival",
            Self::Creative => "Creative",
            Self::Adventure => "Adventure",
            Self::Spectator => "Spectator",
        })
    }
}

/// The type of a Java gamerule exposed through `/gamerule`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleValue {
    Bool(bool),
    Int(i32),
}

impl fmt::Display for RuleValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(value) => value.fmt(f),
            Self::Int(value) => value.fmt(f),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RuleType {
    Bool,
    Int { min: i32, max: i32 },
}

/// Metadata for one supported Java gamerule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleDef {
    pub name: &'static str,
    kind: RuleType,
    default: RuleValue,
}

const fn boolean(name: &'static str, default: bool) -> RuleDef {
    RuleDef { name, kind: RuleType::Bool, default: RuleValue::Bool(default) }
}

const fn integer(name: &'static str, default: i32, min: i32, max: i32) -> RuleDef {
    RuleDef { name, kind: RuleType::Int { min, max }, default: RuleValue::Int(default) }
}

/// Rules the current engine can enforce, with Java names and defaults.
pub const GAME_RULES: [RuleDef; 17] = [
    boolean("keepInventory", false),
    boolean("doDaylightCycle", true),
    boolean("doWeatherCycle", true),
    boolean("doMobSpawning", true),
    boolean("mobGriefing", true),
    boolean("doFireTick", true),
    boolean("naturalRegeneration", true),
    boolean("doImmediateRespawn", false),
    boolean("showDeathMessages", true),
    integer("randomTickSpeed", 3, 0, i32::MAX),
    boolean("doMobLoot", true),
    boolean("doTileDrops", true),
    boolean("fallDamage", true),
    boolean("fireDamage", true),
    boolean("drowningDamage", true),
    integer("playersSleepingPercentage", 100, 0, i32::MAX),
    boolean("doTraderSpawning", true),
];

/// Typed, saved gamerule state. Private storage keeps all mutation going
/// through the registry's type and range checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameRules {
    values: [RuleValue; GAME_RULES.len()],
    spawn_radius: i32,
}

impl Default for GameRules {
    fn default() -> Self {
        Self { values: GAME_RULES.map(|rule| rule.default), spawn_radius: 10 }
    }
}

impl GameRules {
    /// Includes `spawnRadius`, kept separately to avoid disturbing save
    /// ordering as rules are added to the registry.
    pub fn names() -> impl Iterator<Item = &'static str> {
        GAME_RULES.iter().map(|rule| rule.name).chain(std::iter::once("spawnRadius"))
    }

    pub fn get(&self, name: &str) -> Option<RuleValue> {
        if name == "spawnRadius" {
            return Some(RuleValue::Int(self.spawn_radius));
        }
        GAME_RULES.iter().position(|rule| rule.name == name).map(|i| self.values[i])
    }

    /// Parses and sets one rule, rejecting the wrong type and out-of-range
    /// integers. Returns its typed value for command feedback.
    pub fn set(&mut self, name: &str, text: &str) -> Result<RuleValue, String> {
        if name == "spawnRadius" {
            let value = text.parse::<i32>().map_err(|_| "spawnRadius needs an integer".to_string())?;
            if value < 0 {
                return Err("spawnRadius must be 0..2147483647".into());
            }
            self.spawn_radius = value;
            return Ok(RuleValue::Int(value));
        }
        let i =
            GAME_RULES.iter().position(|rule| rule.name == name).ok_or_else(|| format!("unknown gamerule: {name}"))?;
        let value = match GAME_RULES[i].kind {
            RuleType::Bool => RuleValue::Bool(match text {
                "true" => true,
                "false" => false,
                _ => return Err(format!("{name} needs true or false")),
            }),
            RuleType::Int { min, max } => {
                let value = text.parse::<i32>().map_err(|_| format!("{name} needs an integer"))?;
                if !(min..=max).contains(&value) {
                    return Err(format!("{name} must be {min}..{max}"));
                }
                RuleValue::Int(value)
            }
        };
        self.values[i] = value;
        Ok(value)
    }

    pub fn bool(&self, name: &str) -> bool {
        matches!(self.get(name), Some(RuleValue::Bool(true)))
    }

    pub fn int(&self, name: &str) -> i32 {
        match self.get(name) {
            Some(RuleValue::Int(value)) => value,
            _ => 0,
        }
    }

    pub fn serialize(&self) -> String {
        Self::names()
            .filter_map(|name| {
                let value = self.get(name)?;
                let default = if name == "spawnRadius" {
                    RuleValue::Int(10)
                } else {
                    GAME_RULES.iter().find(|rule| rule.name == name)?.default
                };
                (value != default).then(|| format!("{name}={value}"))
            })
            .collect::<Vec<_>>()
            .join(";")
    }

    /// Restores valid entries over Java defaults; malformed future or old
    /// entries are ignored independently.
    pub fn deserialize(text: &str) -> Self {
        let mut rules = Self::default();
        for (name, value) in text.split(';').filter_map(|entry| entry.split_once('=')) {
            let _ = rules.set(name, value);
        }
        rules
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_mode_names_and_abilities_match_java() {
        for mode in GameMode::ALL {
            assert_eq!(GameMode::from_name(mode.name()), Some(mode));
        }
        assert!(GameMode::Creative.invulnerable() && GameMode::Creative.can_fly());
        assert!(GameMode::Spectator.invulnerable() && !GameMode::Spectator.can_interact());
        assert!(GameMode::Adventure.is_survival() && !GameMode::Adventure.can_build());
    }

    #[test]
    fn gamerules_have_java_defaults_and_typed_round_trips() {
        let mut rules = GameRules::default();
        assert!(!rules.bool("keepInventory"));
        assert!(rules.bool("doDaylightCycle"));
        assert_eq!(rules.int("randomTickSpeed"), 3);
        assert_eq!(rules.int("playersSleepingPercentage"), 100);
        assert_eq!(rules.int("spawnRadius"), 10);
        rules.set("keepInventory", "true").unwrap();
        rules.set("randomTickSpeed", "12").unwrap();
        rules.set("spawnRadius", "32").unwrap();
        assert_eq!(GameRules::deserialize(&rules.serialize()), rules);
        assert!(rules.set("keepInventory", "1").is_err());
        assert!(rules.set("playersSleepingPercentage", "-1").is_err());
        assert!(rules.set("notARule", "true").is_err());
    }

    #[test]
    fn old_worlds_receive_every_default() {
        assert_eq!(GameRules::deserialize(""), GameRules::default());
        assert_eq!(GameRules::deserialize("futureRule=yes;randomTickSpeed=bad"), GameRules::default());
    }
}
