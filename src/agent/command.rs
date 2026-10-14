//! Command syntax, coordinate validation and completion, separate from execution.
use crate::{
    item::Item,
    player::MoveInput,
    rules::{GameMode, GameRules, RuleValue},
    simulation::{difficulty::Difficulty, effects::Effect, survival::Vitals},
    world::{
        block::Block,
        terrain::{Biome, Dimension},
    },
};
use glam::{DVec3, IVec3};
pub enum Command {
    Help,
    Observe(i32),
    Players,
    Catalog(String),
    Look(f32, f32),
    Run(MoveInput, u32, bool),
    Eat,
    /// Holds use with a bow or crossbow for this many ticks (Java's draw and
    /// release, or load), or fires a charged crossbow at once.
    Draw(u32),
    Sleep,
    Place,
    Attack,
    Select(usize),
    Fly(bool),
    Craft(Item),
    Chest(bool, usize),
    Drop,
    Respawn,
    Leave,
    Give(Item, u8),
    Clear,
    Kill,
    Summon(crate::entity::MobKind, PositionSpec),
    Mode(GameMode),
    Teleport(PositionSpec),
    SpawnPoint(PositionSpec),
    SetBlock(IVec3, Block),
    Time(f64),
    TimeAdd(i64),
    TimeQuery(TimeQuery),
    Weather(WeatherKind),
    Difficulty(Difficulty),
    GameRule {
        name: String,
        value: Option<String>,
    },
    LocateStructure(String),
    LocateBiome(Biome),
    LocateNetherBiome(crate::world::nether_biome::NetherBiome),
    Seed,
    SetWorldSpawn(PositionSpec),
    Dimension(Dimension),
    /// Java's `/xp`: add or set an amount, counted in levels or points.
    Xp(XpChange),
    XpQuery,
    /// Java's `/effect`.
    Effect(EffectChange),
    /// Java's `/enchant`: enchants the held item.
    Enchant(crate::enchant::Enchantment, u8),
    /// Takes offer 0..3 of the targeted enchanting table for the held item,
    /// paying levels and lapis from the inventory.
    Enchanting(usize),
    /// Combines the held stack with hotbar slot 0..9 on the targeted anvil.
    Anvil(usize),
    Grindstone(Option<usize>),
    Trade(Option<usize>),
    /// Upgrades held diamond gear at the targeted smithing table, consuming
    /// a template and Netherite ingot from the inventory.
    Smithing,
    Say(String),
}

/// `/time query` targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeQuery {
    Daytime,
    Day,
    Gametime,
}

/// `/weather` modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeatherKind {
    Clear,
    Rain,
    Thunder,
}

const COMMAND_NAMES: &[&str] = &[
    "help",
    "give",
    "clear",
    "kill",
    "summon",
    "gamemode",
    "tp",
    "spawnpoint",
    "setworldspawn",
    "setblock",
    "time",
    "weather",
    "difficulty",
    "gamerule",
    "locate",
    "seed",
    "dimension",
    "xp",
    "experience",
    "effect",
    "enchant",
    "say",
    "smithing",
    "grindstone",
    "trade",
    "players",
    "catalog",
];

/// One Java command coordinate: absolute, or relative to the executor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Coordinate {
    Absolute(f64),
    Relative(f64),
}

impl Coordinate {
    fn parse(text: &str) -> Result<Self, String> {
        if let Some(offset) = text.strip_prefix('~') {
            Ok(Self::Relative(if offset.is_empty() { 0.0 } else { number(offset)? }))
        } else {
            Ok(Self::Absolute(number(text)?))
        }
    }

    fn resolve(self, origin: f64) -> f64 {
        match self {
            Self::Absolute(value) => value,
            Self::Relative(offset) => origin + offset,
        }
    }
}

/// Three Java command coordinates with `~` support.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionSpec(pub [Coordinate; 3]);

impl PositionSpec {
    pub fn parse(x: &str, y: &str, z: &str) -> Result<Self, String> {
        Ok(Self([Coordinate::parse(x)?, Coordinate::parse(y)?, Coordinate::parse(z)?]))
    }

    pub fn resolve(self, origin: DVec3) -> Result<DVec3, String> {
        let [x, y, z] = self.0;
        let pos = DVec3::new(x.resolve(origin.x), y.resolve(origin.y), z.resolve(origin.z));
        if !pos.is_finite() || pos.abs().max_element() > 30_000_000.0 {
            return Err("coordinates exceed world bounds".into());
        }
        Ok(pos)
    }
}

/// `/effect give` (with seconds and amplifier) or `/effect clear`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EffectChange {
    Give(Effect, u32, u8),
    /// Removes one effect, or all of them.
    Clear(Option<Effect>),
}

impl EffectChange {
    /// Applies the change; returns instant damage for the caller to deal
    /// and the feedback line.
    pub fn apply(self, vitals: &mut Vitals) -> (f32, String) {
        match self {
            EffectChange::Give(effect, secs, amp) => {
                let damage = vitals.apply_effect(effect, amp, secs.saturating_mul(20).max(1));
                (damage, format!("applied {} {}", effect.name(), crate::simulation::effects::level_name(amp)))
            }
            EffectChange::Clear(Some(effect)) => {
                let had = vitals.effects.remove(effect);
                (0.0, if had { format!("removed {}", effect.name()) } else { format!("no {}", effect.name()) })
            }
            EffectChange::Clear(None) => {
                vitals.effects.clear();
                (0.0, "removed every effect".into())
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct XpChange {
    pub set: bool,
    pub amount: i64,
    pub levels: bool,
}

impl XpChange {
    /// Applies the change, returning the level-up chime volume if any.
    pub fn apply(self, xp: &mut crate::simulation::experience::Experience) -> Option<f32> {
        match (self.set, self.levels) {
            (false, false) => xp.add_points(self.amount),
            (false, true) => xp.add_levels(self.amount),
            (true, true) => xp.add_levels(self.amount - xp.level as i64),
            (true, false) => {
                // Java only sets points within the current level.
                let cap = crate::simulation::experience::points_to_next(xp.level) as i64;
                xp.points = self.amount.clamp(0, cap - 1) as u32;
                None
            }
        }
    }
}

fn number(text: &str) -> Result<f64, String> {
    text.parse::<f64>().ok().filter(|n| n.is_finite()).ok_or_else(|| format!("invalid finite number: {text}"))
}

fn ticks(text: &str) -> Result<u32, String> {
    text.parse::<u32>().ok().filter(|n| (1..=200).contains(n)).ok_or_else(|| "ticks must be 1..200".into())
}

fn parse_time_fraction(token: &str) -> Result<f64, String> {
    match token {
        "day" => Ok(1_000.0 / 24_000.0),
        "noon" => Ok(0.25),
        "night" => Ok(13_000.0 / 24_000.0),
        "midnight" => Ok(0.75),
        n => Ok((number(n)? / 24_000.0).rem_euclid(1.0)),
    }
}

fn common_prefix<'a>(options: impl IntoIterator<Item = &'a str>) -> String {
    let mut iter = options.into_iter();
    let Some(first) = iter.next() else { return String::new() };
    let mut prefix: Vec<char> = first.chars().collect();
    for word in iter {
        prefix.truncate(prefix.iter().zip(word.chars()).take_while(|(a, b)| *a == b).count());
        if prefix.is_empty() {
            break;
        }
    }
    prefix.into_iter().collect()
}

fn complete_options(options: &[&str], partial: &str) -> Option<String> {
    let mut matches: Vec<&str> = options.iter().copied().filter(|o| o.starts_with(partial)).collect();
    matches.sort_unstable();
    match matches.len() {
        0 => None,
        1 => {
            let word = matches[0];
            Some(if partial == word { format!("{word} ") } else { word.to_string() })
        }
        _ => {
            let pref = common_prefix(matches);
            if pref.len() > partial.len() { Some(pref) } else { None }
        }
    }
}

/// Tab-completes a slash command line for the host console.
pub fn tab_complete(input: &str) -> Option<String> {
    if !input.starts_with('/') {
        return None;
    }
    let rest = input.trim_start_matches('/');
    if rest.is_empty() {
        return Some("/help ".into());
    }
    let trailing = input.ends_with(' ') || input.ends_with('\t');
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    let (stem, partial) = if trailing {
        (&tokens[..], "")
    } else {
        let partial = *tokens.last()?;
        (&tokens[..tokens.len().saturating_sub(1)], partial)
    };
    let completed = match stem.first().copied().unwrap_or("") {
        "" => complete_options(COMMAND_NAMES, partial)?,
        "gamemode" if stem.len() <= 1 => {
            complete_options(&["survival", "creative", "adventure", "spectator"], partial)?
        }
        "difficulty" if stem.len() <= 1 => complete_options(&["peaceful", "easy", "normal", "hard"], partial)?,
        "weather" if stem.len() <= 1 => complete_options(&["clear", "rain", "thunder"], partial)?,
        "time" if stem.len() <= 1 => {
            complete_options(&["set", "add", "query", "day", "noon", "night", "midnight"], partial)?
        }
        "time" if stem.len() == 2 && stem[1] == "query" => complete_options(&["daytime", "day", "gametime"], partial)?,
        "locate" if stem.len() <= 1 => complete_options(&["structure", "biome"], partial)?,
        "locate" if stem.len() == 2 && stem[1] == "structure" => complete_options(
            &[
                "stronghold",
                "fortress",
                "nether_fortress",
                "bastion_remnant",
                "mineshaft",
                "abandoned_mineshaft",
                "village",
                "desert_pyramid",
                "jungle_pyramid",
                "swamp_hut",
                "igloo",
                "pillager_outpost",
                "shipwreck",
                "ocean_ruin",
                "monument",
            ],
            partial,
        )?,
        "locate" if stem.len() == 2 && stem[1] == "biome" => {
            let nether = crate::world::nether_biome::NetherBiome::ALL.map(|b| b.name());
            let names: Vec<&str> = Biome::ALL.iter().map(|b| b.name()).chain(nether).collect();
            complete_options(&names, partial)?
        }
        "gamerule" if stem.len() <= 1 => {
            let names: Vec<&str> = GameRules::names().collect();
            complete_options(&names, partial)?
        }
        "gamerule" if stem.len() == 2 => match GameRules::default().get(stem[1]) {
            Some(RuleValue::Bool(_)) => complete_options(&["true", "false"], partial)?,
            Some(RuleValue::Int(_)) => return None,
            None => return None,
        },
        "summon" if stem.len() <= 1 => {
            let names: Vec<String> = crate::entity::MobKind::ALL.iter().map(|k| k.name().replace(' ', "_")).collect();
            let refs: Vec<&str> = names.iter().map(String::as_str).collect();
            complete_options(&refs, partial)?
        }
        "effect" if stem.len() <= 1 => complete_options(&["give", "clear"], partial)?,
        "xp" | "experience" if stem.len() <= 1 => complete_options(&["add", "set", "query"], partial)?,
        _ if stem.is_empty() => complete_options(COMMAND_NAMES, partial)?,
        _ => return None,
    };
    let mut out = String::from("/");
    for (i, token) in stem.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(token);
    }
    if !stem.is_empty() || trailing {
        out.push(' ');
    }
    out.push_str(&completed);
    Some(out)
}

impl Command {
    /// Parse slash commands and CLI input identically, rejecting extra arguments and non-finite numbers.
    pub fn parse(text: &str) -> Result<Self, String> {
        let words: Vec<_> = text.trim().trim_start_matches('/').split_whitespace().collect();
        let bad = || "unknown command or arguments; use /help".to_string();
        let item = |name: &str| Item::from_name(name).ok_or_else(|| format!("unknown item: {name}"));
        let xyz = |x: &str, y: &str, z: &str| -> Result<DVec3, String> {
            let v = DVec3::new(number(x)?, number(y)?, number(z)?);
            if v.abs().max_element() > 30_000_000.0 {
                return Err("coordinates exceed world bounds".into());
            }
            Ok(v)
        };
        Ok(match words.as_slice() {
            ["help"] => Self::Help,
            ["observe"] => Self::Observe(0),
            ["observe", r] => Self::Observe(r.parse::<i32>().ok().filter(|r| (0..=2).contains(r)).ok_or_else(bad)?),
            ["players"] => Self::Players,
            ["catalog", query @ ..] => Self::Catalog(query.join(" ")),
            ["look", yaw, pitch] => {
                Self::Look(number(yaw)?.rem_euclid(360.0) as f32, number(pitch)?.clamp(-89.0, 89.0) as f32)
            }
            ["wait", n] => Self::Run(MoveInput::default(), ticks(n)?, false),
            ["mine", n] => Self::Run(MoveInput::default(), ticks(n)?, true),
            ["move", f, r, n, flags @ ..] => {
                if flags.iter().any(|f| !matches!(*f, "jump" | "sprint" | "sneak")) {
                    return Err(bad());
                }
                let forward = number(f)?;
                let right = number(r)?;
                if forward.abs() > 1.0 || right.abs() > 1.0 {
                    return Err("movement axes must be -1..1".into());
                }
                Self::Run(
                    MoveInput {
                        forward,
                        right,
                        jump: flags.contains(&"jump"),
                        sprint: flags.contains(&"sprint"),
                        descend: flags.contains(&"sneak"),
                    },
                    ticks(n)?,
                    false,
                )
            }
            ["eat"] => Self::Eat,
            ["draw"] => Self::Draw(25),
            ["draw", n] => Self::Draw(ticks(n)?),
            ["sleep"] => Self::Sleep,
            ["place"] => Self::Place,
            ["attack"] => Self::Attack,
            ["select", n] => Self::Select(n.parse::<usize>().ok().filter(|n| (1..=9).contains(n)).ok_or_else(bad)? - 1),
            ["fly", "on"] => Self::Fly(true),
            ["fly", "off"] => Self::Fly(false),
            ["craft", name] => Self::Craft(item(name)?),
            ["chest", action @ ("take" | "put"), slot] => {
                Self::Chest(*action == "take", slot.parse::<usize>().ok().filter(|n| *n < 27).ok_or_else(bad)?)
            }
            ["drop"] => Self::Drop,
            ["respawn"] => Self::Respawn,
            ["leave"] => Self::Leave,
            ["give", "@s" | "@p", name] => Self::Give(item(name)?, 1),
            ["give", "@s" | "@p", name, n] => {
                Self::Give(item(name)?, n.parse::<u8>().ok().filter(|n| *n > 0).ok_or_else(bad)?)
            }
            ["give", name] => Self::Give(item(name)?, 1),
            ["give", name, n] => Self::Give(item(name)?, n.parse::<u8>().ok().filter(|n| *n > 0).ok_or_else(bad)?),
            ["clear"] | ["clear", "@s" | "@p"] => Self::Clear,
            ["kill"] | ["kill", "@s" | "@p"] => Self::Kill,
            ["summon", name] => Self::Summon(
                crate::entity::MobKind::from_name(name).ok_or_else(bad)?,
                PositionSpec([Coordinate::Relative(0.0), Coordinate::Relative(0.0), Coordinate::Relative(0.0)]),
            ),
            ["summon", name, x, y, z] => {
                Self::Summon(crate::entity::MobKind::from_name(name).ok_or_else(bad)?, PositionSpec::parse(x, y, z)?)
            }
            ["gamemode", name] => Self::Mode(GameMode::from_name(name).ok_or_else(bad)?),
            ["gamemode", name, "@s" | "@p"] => Self::Mode(GameMode::from_name(name).ok_or_else(bad)?),
            ["gamemode", "@s" | "@p", name] => Self::Mode(GameMode::from_name(name).ok_or_else(bad)?),
            ["tp", x, y, z] => Self::Teleport(PositionSpec::parse(x, y, z)?),
            ["tp", "@s" | "@p", x, y, z] => Self::Teleport(PositionSpec::parse(x, y, z)?),
            ["spawnpoint"] | ["spawnpoint", "@s" | "@p"] => Self::SpawnPoint(PositionSpec([
                Coordinate::Relative(0.0),
                Coordinate::Relative(0.0),
                Coordinate::Relative(0.0),
            ])),
            ["spawnpoint", x, y, z] | ["spawnpoint", "@s" | "@p", x, y, z] => {
                Self::SpawnPoint(PositionSpec::parse(x, y, z)?)
            }
            ["setworldspawn"] | ["setworldspawn", "@s" | "@p"] => Self::SetWorldSpawn(PositionSpec([
                Coordinate::Relative(0.0),
                Coordinate::Relative(0.0),
                Coordinate::Relative(0.0),
            ])),
            ["setworldspawn", x, y, z] | ["setworldspawn", "@s" | "@p", x, y, z] => {
                Self::SetWorldSpawn(PositionSpec::parse(x, y, z)?)
            }
            ["setblock", x, y, z, name] => {
                Self::SetBlock(xyz(x, y, z)?.floor().as_ivec3(), Block::from_name(name).ok_or_else(bad)?)
            }
            ["time", "set", t] => Self::Time(parse_time_fraction(t)?),
            ["time", "add", n] => Self::TimeAdd(n.parse::<i64>().map_err(|_| "time add needs an integer".to_string())?),
            ["time", "query", "daytime"] => Self::TimeQuery(TimeQuery::Daytime),
            ["time", "query", "day"] => Self::TimeQuery(TimeQuery::Day),
            ["time", "query", "gametime"] => Self::TimeQuery(TimeQuery::Gametime),
            ["time", "day"] => Self::Time(parse_time_fraction("day")?),
            ["time", "noon"] => Self::Time(parse_time_fraction("noon")?),
            ["time", "night"] => Self::Time(parse_time_fraction("night")?),
            ["time", "midnight"] => Self::Time(parse_time_fraction("midnight")?),
            ["time", n] => Self::Time(parse_time_fraction(n)?),
            ["weather", "clear"] => Self::Weather(WeatherKind::Clear),
            ["weather", "rain"] => Self::Weather(WeatherKind::Rain),
            ["weather", "thunder"] => Self::Weather(WeatherKind::Thunder),
            ["difficulty", name] => Self::Difficulty(Difficulty::from_name(name).ok_or_else(bad)?),
            ["gamerule", name] => Self::GameRule { name: name.to_string(), value: None },
            ["gamerule", name, value] => Self::GameRule { name: name.to_string(), value: Some(value.to_string()) },
            ["locate", "village"] => Self::LocateStructure("village".into()),
            ["locate", "structure", name] => Self::LocateStructure(name.to_string()),
            ["locate", "biome", name] => match crate::world::nether_biome::NetherBiome::from_name(name) {
                Some(nether) => Self::LocateNetherBiome(nether),
                None => Self::LocateBiome(Biome::from_name(name).ok_or_else(bad)?),
            },
            ["seed"] => Self::Seed,
            ["dimension", name] => Self::Dimension(Dimension::from_name(name).ok_or_else(bad)?),
            ["xp" | "experience", "query"] => Self::XpQuery,
            ["xp" | "experience", op @ ("add" | "set"), n, unit @ ..] => {
                let amount = n.parse::<i64>().ok().filter(|n| n.abs() <= 1_000_000).ok_or_else(bad)?;
                let levels = match unit {
                    [] | ["points"] => false,
                    ["levels"] => true,
                    _ => return Err(bad()),
                };
                if *op == "set" && amount < 0 {
                    return Err(bad());
                }
                Self::Xp(XpChange { set: *op == "set", amount, levels })
            }
            ["effect", "clear"] => Self::Effect(EffectChange::Clear(None)),
            ["effect", "clear", name] => {
                Self::Effect(EffectChange::Clear(Some(Effect::from_id(name).ok_or_else(bad)?)))
            }
            ["effect", "give", name, rest @ ..] if rest.len() <= 2 => {
                let effect = Effect::from_id(name).ok_or_else(bad)?;
                // Java's defaults: 30 seconds, level I.
                let secs = rest.first().map_or(Ok(30), |n| {
                    n.parse::<u32>().ok().filter(|n| (1..=1_000_000).contains(n)).ok_or_else(bad)
                })?;
                let amp = rest.get(1).map_or(Ok(0), |n| n.parse::<u8>().map_err(|_| bad()))?;
                Self::Effect(EffectChange::Give(effect, secs, amp))
            }
            ["anvil", n] => Self::Anvil(n.parse::<usize>().ok().filter(|n| (1..=9).contains(n)).ok_or_else(bad)? - 1),
            ["trade"] => Self::Trade(None),
            ["trade", n] => {
                Self::Trade(Some(n.parse::<usize>().ok().filter(|n| (1..=10).contains(n)).ok_or_else(bad)? - 1))
            }
            ["grindstone"] => Self::Grindstone(None),
            ["grindstone", n] => {
                Self::Grindstone(Some(n.parse::<usize>().ok().filter(|n| (1..=9).contains(n)).ok_or_else(bad)? - 1))
            }
            ["smithing"] => Self::Smithing,
            ["enchanting", n] => {
                Self::Enchanting(n.parse::<usize>().ok().filter(|n| (1..=3).contains(n)).ok_or_else(bad)? - 1)
            }
            ["enchant", name, rest @ ..] if rest.len() <= 1 => {
                let e = crate::enchant::Enchantment::from_name(name).ok_or_else(bad)?;
                let level = rest.first().map_or(Ok(1), |n| n.parse::<u8>().ok().filter(|&n| n > 0).ok_or_else(bad))?;
                Self::Enchant(e, level)
            }
            ["say", message @ ..] if !message.is_empty() => Self::Say(message.join(" ")),
            _ => return Err(bad()),
        })
    }

    /// Whether executing this command requires cheats to be enabled.
    pub fn cheat(&self) -> bool {
        matches!(
            self,
            Self::Give(..)
                | Self::Clear
                | Self::Kill
                | Self::Summon(..)
                | Self::Mode(..)
                | Self::Teleport(..)
                | Self::SpawnPoint(..)
                | Self::SetBlock(..)
                | Self::Time(..)
                | Self::Weather(..)
                | Self::TimeAdd(..)
                | Self::Say(..)
                | Self::Dimension(..)
                | Self::Xp(..)
                | Self::Effect(..)
                | Self::Enchant(..)
        )
    }
}
