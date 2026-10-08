//! Device-independent player commands and authoritative agent sessions.
//! Agents submit input, never positions or inventory edits (unless cheats are enabled).

use glam::{DVec3, IVec3};
use serde_json::{Value, json};

use crate::crafting;
use crate::entity::Entities;
use crate::inventory::{Inventory, Stack};
use crate::item::Item;
use crate::mining;
use crate::player::{MoveInput, Player};
use crate::rules::{GameMode, GameRules, RuleValue};
use crate::simulation::difficulty::Difficulty;
use crate::simulation::effects::Effect;
use crate::simulation::survival::{self, Vitals};
use crate::simulation::{self, TICK_SECONDS};
use crate::world::terrain::Biome;
use crate::world::{
    World,
    block::{Block, RenderKind},
    terrain::Dimension,
};

pub const HELP: &str = "observe [0..2] | catalog [query] | players | look yaw pitch | move forward right ticks [jump sprint sneak] | wait ticks | mine ticks | eat (or drink) | sleep | place (throws a selected ender pearl, splash potion or eye of ender, or puts the eye in a targeted End portal frame) | attack | select 1..9 | fly on/off | craft item | chest take/put slot | enchanting 1..3 (an aimed enchanting table's offer for the held item) | anvil 1..9 (combine the held stack with that hotbar slot on an aimed anvil) | smithing (upgrade held diamond gear using a template and ingot) | drop | respawn | leave. Cheats: give [@s|@p] item [count], clear, kill, summon mob [x y z], gamemode mode [@s|@p], tp [~] x y z, spawnpoint [x y z], setblock x y z block, time set/add/query, weather clear/rain/thunder, xp|experience add/set/query, effect give/clear, enchant name [level], say message. Host console only: difficulty, gamerule, seed, setworldspawn, locate structure|biome, dimension overworld/nether/end.";

/// Something an agent did that players nearby should hear.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Broke(IVec3, Block),
    Placed(IVec3, Block),
    Chew,
    /// Absorbed an experience orb; with the level-up chime volume when it
    /// reached a multiple of five levels.
    Xp(Option<f32>),
    /// Released an eye of ender from here.
    EyeThrown(DVec3),
    /// Put an eye in the End portal frame here; whether that opened the portal.
    FrameFilled(IVec3, bool),
}

/// Unheard events kept per agent (a host without audio never drains them).
const MAX_EVENTS: usize = 64;

/// Ticks to eat one food item (1.6 s).
pub const EAT_TICKS: u32 = 32;

pub enum Command {
    Help,
    Observe(i32),
    Players,
    Catalog(String),
    Look(f32, f32),
    Run(MoveInput, u32, bool),
    Eat,
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
            ],
            partial,
        )?,
        "locate" if stem.len() == 2 && stem[1] == "biome" => {
            let names: Vec<&str> = Biome::ALL.iter().map(|b| b.name()).collect();
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
            ["locate", "biome", name] => Self::LocateBiome(Biome::from_name(name).ok_or_else(bad)?),
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

pub struct Agent {
    pub player: Player,
    pub previous_pos: DVec3,
    pub inventory: Inventory,
    /// Transient enchanting, anvil or smithing inputs for a controller player.
    /// Saved as returned inventory and dropped on death with the other gear.
    pub work: [Option<Stack>; 3],
    pub vitals: Vitals,
    /// The host's stable ID for this player (owner of its thrown pearls).
    pub id: crate::entity::PlayerId,
    /// Per-player Java game mode. `creative` remains as a compatibility
    /// mirror for old saves and controller code.
    pub mode: GameMode,
    pub creative: bool,
    pub selected: usize,
    pub remaining: u32,
    /// Arm swings so far (placing, attacking, mining), for animation.
    pub swings: u32,
    /// Sounds to play, drained by the host.
    pub events: Vec<Event>,
    /// Seconds in bed (the host fades the view and skips the night once
    /// everyone has slept long enough). Acting or being hurt gets up.
    pub sleeping: Option<f32>,
    /// Foot of the Overworld bed last used, where a dead agent respawns.
    pub spawn_bed: Option<IVec3>,
    /// Exact command-set Overworld spawn point, replacing a bed spawn.
    pub spawn_point: Option<IVec3>,
    input: MoveInput,
    mining: bool,
    /// Holding "use" eats held food; `bite` counts the ticks chewed.
    eating: bool,
    bite: u32,
    /// Accumulated fraction broken, using the speed at each tick.
    breaking: Option<(IVec3, Block, f64)>,
    cooldown: f64,
    /// The world's `keepInventory` rule, refreshed every tick so deaths
    /// outside `tick_rules` (mobs, pearls, `kill`) honour it too.
    keep_inventory: bool,
}

impl Agent {
    /// Creates an idle survival player with empty inventory at `pos`.
    pub fn new(pos: DVec3) -> Self {
        Self {
            player: Player::new(pos),
            previous_pos: pos,
            inventory: Inventory::default(),
            work: [None; 3],
            vitals: Vitals::default(),
            id: crate::entity::PlayerId::default(),
            mode: GameMode::Survival,
            creative: false,
            selected: 0,
            remaining: 0,
            swings: 0,
            events: Vec::new(),
            sleeping: None,
            spawn_bed: None,
            spawn_point: None,
            input: MoveInput::default(),
            mining: false,
            eating: false,
            bite: 0,
            breaking: None,
            cooldown: 0.0,
            keep_inventory: false,
        }
    }

    pub fn target(&self, world: &World) -> Option<(IVec3, IVec3)> {
        world.raycast(self.player.eye(), self.player.forward().as_dvec3(), 6.0)
    }

    /// Changes abilities together so command, host and controller paths agree.
    pub fn set_mode(&mut self, mode: GameMode) {
        self.mode = mode;
        self.creative = mode.is_creative();
        self.player.can_fly = mode.can_fly();
        self.player.noclip = mode == GameMode::Spectator;
        self.player.flying = mode == GameMode::Spectator || (self.player.flying && self.player.can_fly);
        if !self.player.can_fly {
            self.player.flying = false;
        }
    }

    /// Hardcore death has already dropped inventory and experience; revive
    /// only as a non-interacting spectator.
    pub fn hardcore_spectate(&mut self) {
        if self.vitals.is_dead() {
            self.vitals.respawn();
            self.set_mode(GameMode::Spectator);
            self.remaining = 0;
            self.sleeping = None;
        }
    }

    /// Execute immediate actions on the authority; timed inputs complete after real game ticks.
    /// Container transfers and crafting validate capacity before committing either side.
    pub fn execute(
        &mut self,
        command: Command,
        world: &mut World,
        entities: &mut Entities,
        others: &[DVec3],
    ) -> Result<(), String> {
        if self.vitals.is_dead() && !matches!(command, Command::Respawn | Command::Observe(_) | Command::Help) {
            return Err("player is dead; respawn first".into());
        }
        if self.mode == GameMode::Spectator
            && !matches!(
                command,
                Command::Help
                    | Command::Observe(_)
                    | Command::Players
                    | Command::Catalog(_)
                    | Command::Look(..)
                    | Command::Run(..)
                    | Command::Fly(..)
                    | Command::Mode(..)
                    | Command::Teleport(..)
                    | Command::SpawnPoint(..)
                    | Command::Summon(..)
                    | Command::Clear
                    | Command::Kill
                    | Command::Say(..)
                    | Command::Leave
            )
        {
            return Err("spectators cannot interact".into());
        }
        let resting = match &command {
            Command::Observe(_)
            | Command::Help
            | Command::Players
            | Command::Catalog(_)
            | Command::Sleep
            | Command::XpQuery => true,
            Command::Run(input, _, mine) => !mine && input.forward == 0.0 && input.right == 0.0 && !input.jump,
            _ => false,
        };
        if !resting {
            self.sleeping = None;
        }
        match command {
            Command::Run(input, n, mine) => {
                if mine && !self.mode.can_build() {
                    return Err("this game mode cannot break blocks".into());
                }
                if mine
                    && self
                        .target(world)
                        .is_some_and(|(p, _)| world.get_block(p).is_some_and(|b| b.is_door() || b.is_bed()))
                {
                    return Err("multi-cell blocks require the desktop mining action".into());
                }
                self.input = input;
                self.remaining = n;
                self.mining = mine;
                self.eating = false;
                self.breaking = None;
            }
            Command::Eat => {
                let held = self.inventory.get(self.selected).ok_or("selected slot empty")?;
                if !held.item.is_drink() {
                    held.item.food().ok_or("selected item is not food or a potion")?;
                    if self.creative || !self.vitals.hunger.can_eat() {
                        return Err("not hungry".into());
                    }
                }
                self.input = MoveInput::default();
                self.remaining = EAT_TICKS;
                self.mining = false;
                self.eating = true;
                self.bite = 0;
            }
            Command::Look(yaw, pitch) => {
                self.player.yaw = yaw.to_radians();
                self.player.pitch = pitch.to_radians();
                self.breaking = None;
            }
            Command::Select(slot) => {
                self.selected = slot;
                self.breaking = None;
                self.bite = 0;
            }
            Command::Fly(on) => {
                if on && !self.mode.can_fly() {
                    return Err("flight requires Creative or Spectator mode".into());
                }
                self.player.flying = on;
                self.player.vel = DVec3::ZERO;
            }
            Command::Enchant(e, level) => {
                let slot = &mut self.inventory.slots[self.selected];
                *slot = Some(crate::enchant::command(*slot, e, level)?);
            }
            Command::Give(item, count) => {
                let mut inv = self.inventory.clone();
                if inv.add(item, count) > 0 {
                    return Err("inventory full".into());
                }
                self.inventory = inv;
            }
            Command::Clear => {
                self.inventory.take_all();
                self.work.fill(None);
            }
            Command::Kill => {
                if !self.vitals.is_dead() {
                    self.vitals.damage(f32::MAX, "was killed", false);
                    self.drop_everything(entities);
                    self.remaining = 0;
                }
            }
            Command::Summon(kind, pos) => {
                entities.spawn(kind, pos.resolve(self.player.pos)?);
            }
            Command::Xp(change) => {
                if let Some(chime) = change.apply(&mut self.vitals.xp) {
                    self.emit(Event::Xp(Some(chime)));
                }
            }
            Command::Effect(change) => {
                let (damage, _) = change.apply(&mut self.vitals);
                self.damage(damage, survival::CAUSE_MAGIC);
            }
            Command::Mode(mode) => self.set_mode(mode),
            Command::Teleport(pos) => {
                let pos = pos.resolve(self.player.pos)?;
                self.player.pos = pos;
                self.previous_pos = pos;
                self.player.vel = DVec3::ZERO;
                self.vitals.reset_fall();
            }
            Command::SpawnPoint(pos) => {
                let pos = pos.resolve(self.player.pos)?.floor().as_ivec3();
                self.spawn_bed = None;
                self.spawn_point = Some(pos);
            }
            Command::SetBlock(pos, block) => {
                if !world.set_block(pos, block) {
                    return Err("block is unchanged or unloaded".into());
                }
            }
            Command::Place if self.inventory.get(self.selected).is_some_and(|s| s.item == Item::FISHING_ROD) => {
                if !self.mode.can_interact() {
                    return Err("spectators cannot use items".into());
                }
                if self.cooldown > 0.0 {
                    return Err("action cooling down".into());
                }
                crate::survival_items::use_rod(
                    &self.player,
                    &mut self.inventory,
                    self.selected,
                    self.creative,
                    self.id,
                    entities,
                );
                self.cooldown = 0.22;
                self.swings += 1;
            }
            Command::Place
                if self
                    .inventory
                    .get(self.selected)
                    .is_some_and(|s| s.item == Item::SNOWBALL || s.item == Item::EGG) =>
            {
                if !self.mode.can_interact() {
                    return Err("spectators cannot use items".into());
                }
                if self.cooldown > 0.0 {
                    return Err("action cooling down".into());
                }
                crate::survival_items::throw_held(
                    &self.player,
                    &mut self.inventory,
                    self.selected,
                    self.creative,
                    self.id,
                    entities,
                );
                self.cooldown = 0.22;
                self.swings += 1;
            }
            Command::Place if self.inventory.get(self.selected).is_some_and(|s| s.item == Item::ENDER_PEARL) => {
                if !self.mode.can_interact() {
                    return Err("spectators cannot use items".into());
                }
                if self.vitals.pearl_cooldown > 0.0 {
                    return Err("ender pearl cooling down".into());
                }
                self.vitals.pearl_cooldown = crate::entity::pearl::COOLDOWN;
                let p = &self.player;
                let carry = if p.on_ground { p.vel.with_y(0.0) } else { p.vel };
                entities.throw_pearl(self.id, p.eye(), p.forward().as_dvec3(), carry);
                self.swings += 1;
                if !self.creative {
                    self.inventory.take_one(self.selected);
                }
            }
            Command::Place
                if self.inventory.get(self.selected).is_some_and(|s| s.item.as_splash_potion().is_some()) =>
            {
                if !self.mode.can_interact() {
                    return Err("spectators cannot use items".into());
                }
                let potion = self.inventory.get(self.selected).and_then(|s| s.item.as_splash_potion()).unwrap();
                let p = &self.player;
                let carry = if p.on_ground { p.vel.with_y(0.0) } else { p.vel };
                entities.throw_potion(self.id, potion, p.eye(), p.forward().as_dvec3(), carry);
                self.swings += 1;
                if !self.creative {
                    self.inventory.take_one(self.selected);
                }
            }
            Command::Place if self.inventory.get(self.selected).is_some_and(|s| s.item == Item::EYE_OF_ENDER) => {
                if !self.mode.can_interact() {
                    return Err("spectators cannot use items".into());
                }
                if self.cooldown > 0.0 {
                    return Err("action cooling down".into());
                }
                let frame = self
                    .target(world)
                    .map(|(pos, _)| pos)
                    .filter(|&pos| world.get_block(pos).is_some_and(|b| b.base() == Block::END_PORTAL_FRAME));
                if let Some(pos) = frame {
                    let opened =
                        world.insert_eye(pos).ok_or("that frame already holds an eye, or its ring isn't loaded")?;
                    self.emit(Event::FrameFilled(pos, opened));
                } else {
                    if world.generator.dimension != crate::world::terrain::Dimension::Overworld {
                        return Err("eyes of ender only find strongholds in the overworld".into());
                    }
                    let from = self.player.pos + DVec3::Y * (crate::player::SHAPE.height * 0.5);
                    let target = world.generator.strongholds.nearest(from.floor().as_ivec3()).ok_or("no stronghold")?;
                    entities.release_eye(from, target.as_dvec3());
                    self.emit(Event::EyeThrown(from));
                }
                self.cooldown = 0.22;
                self.swings += 1;
                if !self.creative {
                    self.inventory.take_one(self.selected);
                }
            }
            Command::Place
                if crate::survival_items::use_mob(
                    &self.player,
                    &mut self.inventory,
                    self.selected,
                    self.creative,
                    world,
                    entities,
                ) =>
            {
                self.cooldown = 0.22;
                self.swings += 1;
            }
            Command::Place
                if self
                    .target(world)
                    .is_some_and(|(pos, _)| world.get_block(pos).is_some_and(|b| b.cake_bites().is_some())) =>
            {
                let (pos, _) = self.target(world).unwrap();
                if !crate::survival_items::bite_cake(world, pos, &mut self.vitals, self.creative) {
                    return Err("not hungry enough to eat cake".into());
                }
                self.swings += 1;
            }
            Command::Place if self.inventory.get(self.selected).is_some_and(|s| s.item.bed_color().is_some()) => {
                if !self.mode.can_build() {
                    return Err("this game mode cannot place beds".into());
                }
                if self.cooldown > 0.0 {
                    return Err("action cooling down".into());
                }
                let (pos, normal) = self.target(world).ok_or("no block within reach")?;
                let at = if world.get_block(pos).is_some_and(|b| b.is_replaceable()) { pos } else { pos + normal };
                let direction = crate::world::block::Facing::toward(self.player.forward()).opposite().offset();
                let head = at + direction;
                if self.player.intersects_block(at)
                    || self.player.intersects_block(head)
                    || others.iter().any(|&p| {
                        crate::player::Player::new(p).intersects_block(at)
                            || crate::player::Player::new(p).intersects_block(head)
                    })
                {
                    return Err("bed intersects a player".into());
                }
                let color = self.inventory.get(self.selected).unwrap().item.bed_color().unwrap();
                if !world.place_colored_bed(at, direction, color) {
                    return Err("bed does not fit".into());
                }
                if !self.creative {
                    self.inventory.take_one(self.selected);
                }
                self.cooldown = 0.22;
                self.swings += 1;
            }
            Command::Place => {
                if !self.mode.can_build() {
                    return Err("this game mode cannot place blocks".into());
                }
                if self.cooldown > 0.0 {
                    return Err("action cooling down".into());
                }
                let (pos, normal) = self.target(world).ok_or("no block within reach")?;
                let held = self.inventory.get(self.selected).ok_or("selected slot empty")?;
                let block = held.item.places().ok_or("selected item cannot be placed")?;
                // Complex multi-cell placements use client gameplay until the shared action boundary is extracted.
                if block.is_door()
                    || block.is_bed()
                    || block.is_ladder()
                    || block.kind() == RenderKind::Invisible
                    || block.is_fire()
                {
                    return Err("this block requires the desktop placement action".into());
                }
                let block =
                    crate::world::village_blocks::placed(crate::world::nether_blocks::placed(block, normal), normal);
                let at = pos + normal;
                if !world.get_block(at).is_some_and(|b| b == Block::AIR || b.is_water() || b.is_lava()) {
                    return Err("destination occupied or unloaded".into());
                }
                if !world.get_block(at - IVec3::Y).is_some_and(|below| block.can_stay_on(below))
                    || (block.is_mushroom() && !world.mushroom_survives(at))
                {
                    return Err("block cannot survive here".into());
                }
                if (block.is_solid() && self.player.intersects_block(at))
                    || others.iter().any(|&p| Player::new(p).intersects_block(at))
                {
                    return Err("placement intersects a player".into());
                }
                if !world.set_block(at, block) {
                    return Err("placement failed".into());
                }
                self.emit(Event::Placed(at, block));
                self.cooldown = 0.22;
                self.swings += 1;
                if !self.creative {
                    self.inventory.take_one(self.selected);
                }
            }
            Command::Attack => {
                if self.cooldown > 0.0 {
                    return Err("action cooling down".into());
                }
                let distance = self
                    .target(world)
                    .map_or(6.0, |(p, _)| self.player.eye().distance(p.as_dvec3() + DVec3::splat(0.5)).min(6.0));
                let (eye, dir) = (self.player.eye(), self.player.forward().as_dvec3());
                let stack = self.inventory.get(self.selected);
                let held = stack.map(|s| s.item);
                let bonus = self.vitals.effects.attack_bonus();
                if entities.large_fireball(eye, dir, distance).is_some() {
                    entities.punch_fireball(eye, dir, distance);
                } else if let Some((hit, t)) = entities.fight_raycast(eye, dir, distance) {
                    let enchants = stack.map_or(Default::default(), |s| s.active_enchants());
                    let enchant = crate::enchant::damage_bonus(enchants, crate::enchant::Creature::Other);
                    let damage = (mining::attack_damage(held) + bonus).max(0.0) + enchant;
                    if entities.strike(hit, damage, self.id) && enchant > 0.0 {
                        let mut burst =
                            crate::particles::Burst::new(crate::particles::Kind::MagicCrit, eye + dir * t.min(4.0), 16);
                        burst.spread = DVec3::splat(0.4);
                        entities.particles.push(crate::particles::Request::Tracking(burst));
                    }
                } else {
                    let (i, _) = entities.raycast(eye, dir, distance).ok_or("no mob within reach")?;
                    let sprint = self.movement_input().sprint && (self.creative || self.vitals.hunger.can_sprint());
                    let sweep = (self.player.on_ground && !sprint).then_some(self.player.pos);
                    entities.melee(i, dir, stack, bonus, false, sweep);
                }
                if !self.creative
                    && let Some(held) = held
                {
                    self.inventory.wear(self.selected, mining::wear(held, true));
                }
                self.cooldown = mining::attack_cooldown(held);
                self.swings += 1;
            }
            Command::Craft(item) => self.craft(item, world)?,
            Command::Chest(take, slot) => {
                let (pos, _) = self.target(world).ok_or("no chest within reach")?;
                let chest = world.chest_mut(pos).ok_or("target is not a chest")?;
                if take {
                    let stack = chest.slots[slot].ok_or("chest slot empty")?;
                    let left = self.inventory.add_stack(stack);
                    chest.slots[slot] = (left > 0).then_some(Stack { count: left, ..stack });
                    if left == stack.count {
                        return Err("inventory full".into());
                    }
                } else {
                    let stack = self.inventory.get(self.selected).ok_or("selected slot empty")?;
                    let dest = &mut chest.slots[slot];
                    let room = match dest {
                        None => stack.max(),
                        Some(s) if s.stacks_with(&stack) => s.max() - s.count,
                        _ => 0,
                    };
                    let n = room.min(stack.count);
                    if n == 0 {
                        return Err("chest slot full or incompatible".into());
                    }
                    *dest = Some(Stack { count: dest.map_or(0, |s| s.count) + n, ..stack });
                    self.inventory.slots[self.selected] =
                        (stack.count > n).then_some(Stack { count: stack.count - n, ..stack });
                }
            }
            Command::Enchanting(i) => self.enchant_at_table(i, world)?,
            Command::Anvil(slot) => {
                let (pos, _) = self.target(world).ok_or("no anvil within reach")?;
                if !world.get_block(pos).is_some_and(Block::is_anvil) {
                    return Err("target is not an anvil".into());
                }
                if slot == self.selected {
                    return Err("pick a different slot for the second item".into());
                }
                let left = self.inventory.get(self.selected).ok_or("selected slot empty")?;
                let right = self.inventory.get(slot);
                let r = crate::enchant::anvil_any_cost(left, right, self.creative).ok_or("those don't combine")?;
                if !self.creative && r.cost >= crate::enchant::TOO_EXPENSIVE {
                    return Err("too expensive".into());
                }
                if !self.creative && self.vitals.xp.level < r.cost {
                    return Err(format!("needs level {}", r.cost));
                }
                self.inventory.slots[self.selected] = Some(r.output);
                self.inventory.slots[slot] = match (r.uses, right) {
                    (Some(n), Some(s)) if s.count > n => Some(Stack { count: s.count - n, ..s }),
                    _ => None,
                };
                if !self.creative {
                    self.vitals.xp.add_levels(-(r.cost as i64));
                    crate::enchant::wear_anvil(world, pos);
                }
            }
            Command::Grindstone(slot) => {
                let (pos, _) = self.target(world).ok_or("no grindstone within reach")?;
                if world.get_block(pos).is_none_or(|b| b.base() != Block::GRINDSTONE) {
                    return Err("target is not a grindstone".into());
                }
                if slot == Some(self.selected) {
                    return Err("pick a different second slot".into());
                }
                let a = self.inventory.get(self.selected);
                let b = slot.and_then(|i| self.inventory.get(i));
                let result = crate::grindstone::result(a, b).ok_or("those don't grind")?;
                let xp = crate::grindstone::xp(a, b, crate::enchant::roll());
                self.inventory.slots[self.selected] = Some(result.output);
                if let Some(i) = slot {
                    self.inventory.slots[i] = None;
                }
                entities.spawn_xp(pos.as_dvec3() + DVec3::splat(0.5), xp);
            }
            Command::Smithing => {
                let (pos, _) = self.target(world).ok_or("no smithing table within reach")?;
                if world.get_block(pos) != Some(Block::SMITHING_TABLE) {
                    return Err("target is not a smithing table".into());
                }
                let base = self.inventory.get(self.selected).ok_or("selected slot empty")?;
                let template_slot = self.inventory.find(Item::NETHERITE_UPGRADE).ok_or("no upgrade template")?;
                let addition_slot = self.inventory.find(Item::NETHERITE_INGOT).ok_or("no Netherite ingot")?;
                let result = crate::smithing::upgrade(
                    self.inventory.get(template_slot).unwrap(),
                    base,
                    self.inventory.get(addition_slot).unwrap(),
                )
                .ok_or("selected item is not diamond gear")?;
                self.inventory.take_one(template_slot);
                self.inventory.take_one(addition_slot);
                self.inventory.slots[self.selected] = Some(result);
            }
            Command::Drop => {
                let stack = self.inventory.slots[self.selected].take().ok_or("selected slot empty")?;
                entities.throw(stack, self.player.eye(), self.player.forward().as_dvec3());
            }
            Command::Respawn => {
                if !self.vitals.is_dead() {
                    return Err("player is alive".into());
                }
                let overworld = world.generator.dimension == Dimension::Overworld;
                let bed = self
                    .spawn_bed
                    .filter(|&b| overworld && world.get_block(b).is_some_and(|b| b.is_bed() && !b.is_bed_head()));
                if overworld && bed.is_none() {
                    self.spawn_bed = None; // broken: forget it
                }
                let at = match (self.spawn_point.filter(|_| overworld), bed) {
                    (Some(p), _) => p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5),
                    (None, Some(b)) => {
                        DVec3::new(b.x as f64 + 0.5, b.y as f64 + Block::BED_FOOT.height(), b.z as f64 + 0.5)
                    }
                    (None, None) => world.generator.find_spawn().as_dvec3() + DVec3::new(0.5, 0.0, 0.5),
                };
                self.player = Player::new(at);
                self.player.can_fly = self.creative || self.mode.can_fly();
                self.player.noclip = self.mode == GameMode::Spectator;
                self.vitals = Vitals::default();
            }
            Command::Observe(_) | Command::Help | Command::XpQuery | Command::Say(_) => {}
            _ => return Err("command requires host console".into()),
        }
        Ok(())
    }

    fn craft(&mut self, item: Item, world: &World) -> Result<(), String> {
        let at_table = self.at_crafting_table(world);
        let inventory = crafting::recipes()
            .iter()
            .filter(|r| r.result.item == item)
            .find_map(|r| self.crafted(r, at_table))
            .ok_or("missing ingredients, inventory space or targeted crafting table")?;
        self.inventory = inventory;
        Ok(())
    }

    fn at_crafting_table(&self, world: &World) -> bool {
        self.target(world).is_some_and(|(p, _)| world.get_block(p).is_some_and(|b| b.base() == Block::CRAFTING_TABLE))
    }

    /// The inventory after crafting `recipe` once, if the ingredients and
    /// room are there (3x3 recipes need a targeted crafting table).
    fn crafted(&self, recipe: &crafting::Recipe, at_table: bool) -> Option<Inventory> {
        let grid = recipe.preview();
        if grid.size == 3 && !at_table {
            return None;
        }
        let mut inventory = self.inventory.clone();
        for i in 0..grid.cells.len() {
            if grid.cells[i].is_none() {
                continue;
            }
            let options = recipe.alternatives(i).unwrap();
            let slot = inventory.slots.iter().position(|s| s.is_some_and(|s| options.contains(&s.item)))?;
            let used = inventory.get(slot)?.item;
            inventory.take_one(slot);
            if let Some(rest) = used.remainder()
                && inventory.add(rest, 1) != 0
            {
                return None;
            }
        }
        (inventory.add_stack(recipe.result) == 0).then_some(inventory)
    }

    /// Every result that `craft` would make right now, once each, in recipe
    /// book order.
    pub fn craftable(&self, world: &World) -> Vec<Stack> {
        let at_table = self.at_crafting_table(world);
        let mut out: Vec<Stack> = Vec::new();
        for recipe in crafting::recipes() {
            if !out.iter().any(|s| s.item == recipe.result.item) && self.crafted(recipe, at_table).is_some() {
                out.push(recipe.result);
            }
        }
        out
    }

    /// Advance physics/survival/mining once. All sessions tick before the shared world systems.
    pub fn tick(&mut self, world: &mut World, entities: &mut Entities) {
        self.tick_difficulty(world, entities, crate::simulation::difficulty::Difficulty::Normal);
    }

    /// [`Agent::tick`] using the host world's difficulty.
    pub fn tick_difficulty(
        &mut self,
        world: &mut World,
        entities: &mut Entities,
        difficulty: crate::simulation::difficulty::Difficulty,
    ) {
        self.tick_rules(world, entities, difficulty, &crate::rules::GameRules::default());
    }

    /// [`Agent::tick_difficulty`] with natural regeneration toggled.
    pub fn tick_rules(
        &mut self,
        world: &mut World,
        entities: &mut Entities,
        difficulty: crate::simulation::difficulty::Difficulty,
        rules: &crate::rules::GameRules,
    ) {
        self.previous_pos = self.player.pos;
        self.keep_inventory = rules.bool("keepInventory");
        // Movement alone returning early is not enough: survival, mining,
        // pickups and timed commands must also wait for local terrain.
        let feet = self.player.pos.floor().as_ivec3();
        if !world.is_loaded(feet) || !world.is_loaded(feet - IVec3::Y) {
            return;
        }
        let holding_rod = self.inventory.get(self.selected).is_some_and(|s| s.item == Item::FISHING_ROD);
        if self.vitals.is_dead() || !holding_rod {
            entities.drop_bobber(self.id);
        }
        self.cooldown = (self.cooldown - TICK_SECONDS).max(0.0);
        if self.vitals.is_dead() {
            self.remaining = 0;
            self.sleeping = None;
            return;
        }
        let mut input = self.movement_input();
        input.sprint &= self.creative || self.mode.invulnerable() || self.vitals.hunger.can_sprint();
        let before = self.player.pos;
        self.player.apply_effects(&self.vitals.effects);
        self.player
            .wear_boots(crate::enchant::armor_level(&self.inventory.armor, crate::enchant::Enchantment::DepthStrider));
        self.player.update(TICK_SECONDS, input, world);
        crate::particles::water_entry(&self.player, before, world);
        let moved = (self.player.pos - before).with_y(0.0).length();
        let env = simulation::survival::Env {
            respiration: crate::enchant::armor_level(&self.inventory.armor, crate::enchant::Enchantment::Respiration),
            frost_walker: crate::enchant::armor_level(&self.inventory.armor, crate::enchant::Enchantment::FrostWalker)
                > 0,
            ..simulation::player_environment(&self.player, world, input, moved)
        };
        let hurts = self.vitals.tick_rules(
            TICK_SECONDS as f32,
            &env,
            self.creative || self.mode.invulnerable(),
            difficulty,
            rules.bool("naturalRegeneration"),
        );
        for (damage, cause) in [
            (hurts.fall * rules.bool("fallDamage") as u8 as f32, "hit the ground too hard"),
            (hurts.drown * rules.bool("drowningDamage") as u8 as f32, "drowned"),
            (hurts.lava * rules.bool("fireDamage") as u8 as f32, "tried to swim in lava"),
            ((hurts.fire + hurts.burn) * rules.bool("fireDamage") as u8 as f32, "burned to death"),
            (hurts.starve, "starved to death"),
        ] {
            if damage > 0.0 && self.damage(damage, cause) > 0.0 {
                self.sleeping = None;
            }
        }
        if self.vitals.is_dead() {
            self.drop_everything(entities);
            self.remaining = 0;
            return;
        }
        if self.remaining > 0
            && self.mining
            && self.cooldown <= 0.0
            && let Some((pos, face)) = self.target(world)
            && let Some(block) = world.get_block(pos)
        {
            let held = self.inventory.get(self.selected).map(|s| s.item);
            let digger = self.digger(world);
            let progress = self.breaking.filter(|(p, b, _)| *p == pos && *b == block).map_or(0.0, |(_, _, n)| n)
                + TICK_SECONDS / mining::dig_time(block, digger).max(1e-3) as f64;
            self.breaking = Some((pos, block, progress));
            if !self.creative && progress < 1.0 {
                world.particles.push(crate::particles::Request::Hit { cell: pos, block, face });
            }
            self.swings += 1;
            if block != Block::BEDROCK && !block.is_door() && (self.creative || progress >= 1.0) {
                if block.is_bed() {
                    world.break_bed_partner(pos, block, !self.creative && rules.bool("doTileDrops"));
                }
                world.set_block(pos, Block::AIR);
                self.emit(Event::Broke(pos, block));
                if !self.creative {
                    if mining::can_harvest(block, held) {
                        let tool = digger.held.map_or(Default::default(), |s| s.active_enchants());
                        if rules.bool("doTileDrops") {
                            world.spill_with_item(pos, block, digger.held);
                            entities.drop_mined_xp(block, pos, tool);
                        }
                    }
                    if let Some(held) = held {
                        self.inventory.wear(self.selected, mining::wear(held, false));
                    }
                    self.vitals.hunger.exhaust(simulation::survival::EXHAUST_MINE);
                }
                self.breaking = None;
                self.cooldown = 0.15;
            }
        } else {
            self.breaking = None;
        }
        self.chew(entities);
        self.remaining = self.remaining.saturating_sub(1);
        entities.items.retain_mut(|item| {
            if item.pickup_delay > 0.0 || !item.touches_player(self.player.pos) {
                return true;
            }
            item.stack.count = self.inventory.add_stack(item.stack);
            item.stack.count > 0
        });
        if let Some(chime) = crate::entity::orb::absorb(
            &mut entities.orbs,
            self.player.pos,
            &mut self.vitals.xp,
            &mut self.inventory,
            self.selected,
        ) {
            self.emit(Event::Xp(chime));
        }
    }

    /// Arrives where this agent's ender pearl landed, taking the landing's
    /// 5 damage. Returns whether it teleported (dead or asleep agents don't).
    pub fn pearl_teleport(&mut self, pos: DVec3, entities: &mut Entities) -> bool {
        if self.vitals.is_dead() || self.sleeping.is_some() {
            return false;
        }
        self.player.pos = pos;
        self.previous_pos = pos;
        self.player.vel = DVec3::ZERO;
        self.vitals.reset_fall();
        self.damage(crate::entity::pearl::DAMAGE, "fell from a high place");
        if self.vitals.is_dead() {
            self.drop_everything(entities);
            self.remaining = 0;
        }
        true
    }

    /// A dead survival agent's inventory and some of its experience spill
    /// where it died, unless `keepInventory` is on.
    fn drop_everything(&mut self, entities: &mut Entities) {
        if self.keep_inventory {
            return;
        }
        let stacks = self.inventory.take_all().into_iter().chain(self.work.iter_mut().filter_map(Option::take));
        for stack in stacks.filter(|s| !s.active_enchants().has(crate::enchant::Enchantment::VanishingCurse)) {
            entities.scatter(stack, self.player.pos);
        }
        let xp = self.vitals.xp.die();
        entities.spawn_xp(self.player.pos, xp);
    }

    /// One tick of eating: a bite finishes after [`EAT_TICKS`] of holding
    /// the same food, like Java. Switching slots restarts it via `select`.
    fn chew(&mut self, entities: &mut Entities) {
        let held = self.inventory.get(self.selected).map(|s| s.item);
        let milk = held == Some(Item::MILK_BUCKET);
        let potion = held.and_then(Item::as_potion);
        let food = held.and_then(|i| i.food()).filter(|_| !self.creative && self.vitals.hunger.can_eat());
        if self.remaining == 0 || !self.eating || (!milk && potion.is_none() && food.is_none()) {
            self.bite = 0;
            return;
        }
        self.bite += 1;
        // Chewing sounds four times a second.
        if self.bite % 5 == 1 {
            self.emit(Event::Chew);
        }
        if self.bite >= EAT_TICKS {
            self.bite = 0;
            if milk {
                self.vitals.effects.clear();
                crate::survival_items::exchange(
                    &mut self.inventory,
                    self.selected,
                    Item::BUCKET,
                    self.creative,
                    entities,
                    &self.player,
                );
            } else if let Some(potion) = potion {
                let damage = potion.drink(&mut self.vitals);
                self.damage(damage, survival::CAUSE_MAGIC);
                if !self.creative {
                    self.inventory.slots[self.selected] = Some(Stack::new(Item::GLASS_BOTTLE, 1));
                }
            } else if let Some((hunger, saturation)) = food {
                if let Some(remainder) = held.and_then(Item::remainder) {
                    crate::survival_items::exchange(
                        &mut self.inventory,
                        self.selected,
                        remainder,
                        false,
                        entities,
                        &self.player,
                    );
                } else {
                    self.inventory.take_one(self.selected);
                }
                self.vitals.hunger.eat(hunger, saturation);
                if let Some((effect, amp, ticks)) = held.and_then(|item| item.food_effect_roll(entities.roll())) {
                    self.vitals.apply_effect(effect, amp, ticks);
                }
            }
            // A timed `eat` command stops after one bite.
            if self.remaining <= 1 {
                self.eating = false;
            }
        }
    }

    fn emit(&mut self, event: Event) {
        if self.events.len() < MAX_EVENTS {
            self.events.push(event);
        }
    }

    /// Fraction of the current bite chewed, for the HUD.
    pub fn eating(&self) -> f32 {
        self.bite as f32 / EAT_TICKS as f32
    }

    /// The targeted enchanting table and its offers for the held item.
    fn table_offers(&self, world: &World) -> Option<(IVec3, [crate::enchant::Offer; 3])> {
        let (pos, _) = self.target(world)?;
        let held = self.inventory.get(self.selected)?;
        (world.get_block(pos) == Some(Block::ENCHANTING_TABLE) && crate::enchant::table_accepts(held)).then(|| {
            (pos, crate::enchant::offers(self.vitals.xp.seed, held.item, crate::enchant::bookshelves(world, pos)))
        })
    }

    /// Java's enchanting table for an agent: one of the held item gets
    /// offer `i`, for `i + 1` levels and lapis (none in creative).
    fn enchant_at_table(&mut self, i: usize, world: &World) -> Result<(), String> {
        let (_, offers) =
            self.table_offers(world).ok_or("not aiming at an enchanting table with an enchantable item")?;
        let offer = offers[i];
        let held = self.inventory.get(self.selected).ok_or("selected slot empty")?;
        let lapis = self
            .inventory
            .slots
            .iter()
            .flatten()
            .filter(|s| s.item == Item::LAPIS_LAZULI)
            .map(|s| s.count as usize)
            .sum::<usize>();
        let level = self.vitals.xp.level;
        if offer.cost == 0 {
            return Err("no offer in that slot".into());
        }
        if !self.creative && (lapis <= i || level < offer.cost || level <= i as u32) {
            return Err(format!("needs {} lapis and level {}", i + 1, offer.cost));
        }
        let mut out = Stack { count: 1, ..held };
        if out.item == Item::BOOK {
            out.item = Item::ENCHANTED_BOOK;
        }
        for (e, l) in crate::enchant::offer_enchants(self.vitals.xp.seed, held.item, i, offer.cost) {
            out.enchants.set(e, l);
        }
        let mut inv = self.inventory.clone();
        inv.take_one(self.selected);
        if !self.creative {
            for _ in 0..=i {
                let slot = inv.find(Item::LAPIS_LAZULI).ok_or("no lapis")?;
                inv.take_one(slot);
            }
        }
        if inv.slots[self.selected].is_none() {
            inv.slots[self.selected] = Some(out);
        } else if inv.add_stack(out) > 0 {
            return Err("inventory full".into());
        }
        if !self.creative {
            self.vitals.xp.add_levels(-(i as i64 + 1));
        }
        self.inventory = inv;
        self.vitals.xp.seed = (crate::enchant::roll() * u32::MAX as f32) as u32 as i32;
        Ok(())
    }

    /// Damage through protection enchantments (armor points aside).
    fn damage(&mut self, amount: f32, cause: &str) -> f32 {
        let amount = crate::enchant::protect(amount, &self.inventory.armor, cause);
        let taken = self.vitals.damage(amount, cause, self.creative || self.mode.invulnerable());
        if taken > 0.0 {
            self.player.animation.hurt_direction = 0.0;
        }
        taken
    }

    /// Armored damage from a mob, arrow or explosion. Knockback only lands
    /// with damage (hurt immunity also stops repeated shoves), and a survival
    /// agent killed this way drops everything. Returns the damage taken.
    pub fn hurt(&mut self, amount: f32, cause: &str, knockback: DVec3, entities: &mut Entities) -> f32 {
        let inv = &self.inventory;
        let reduced = simulation::survival::armor_reduce(amount, inv.armor_points(), inv.armor_toughness());
        let taken = self.damage(reduced, cause);
        if taken <= 0.0 {
            return 0.0;
        }
        self.player.hurt_from(knockback);
        self.player.vel += simulation::survival::knockback_taken(knockback, self.inventory.knockback_resistance());
        self.inventory.wear_armor(amount);
        self.sleeping = None;
        if self.vitals.is_dead() {
            self.drop_everything(entities);
            self.remaining = 0;
        }
        taken
    }

    /// Held device input for the next tick (a local controller). Unlike a
    /// timed `move` or `mine`, this keeps mining progress on the same block.
    pub fn hold(&mut self, input: MoveInput, mining: bool, using: bool) {
        self.input = input;
        self.mining = mining;
        self.eating = using;
        self.remaining = 1;
    }

    /// The block being mined and the fraction broken (for crack overlays).
    pub fn breaking(&self, world: &World) -> Option<(IVec3, f32)> {
        let (pos, block, progress) = self.breaking?;
        if world.get_block(pos)? != block {
            return None;
        }
        Some((pos, (progress as f32).min(1.0)))
    }

    /// Held movement for the current command or controller tick.
    pub fn movement_input(&self) -> MoveInput {
        if self.remaining > 0 { self.input } else { MoveInput::default() }
    }

    /// What this agent mines with, and where it stands (Java's penalties).
    fn digger(&self, world: &World) -> mining::Digger {
        mining::Digger {
            held: self.inventory.get(self.selected),
            helmet: self.inventory.armor[0].map_or(Default::default(), |s| s.enchants),
            eyes_in_water: self.player.head_in_water(world),
            on_ground: self.player.on_ground || self.player.flying,
        }
    }

    /// Whether hostile mobs may attack this agent.
    pub fn targetable(&self) -> bool {
        !self.creative && self.mode.targetable() && !self.vitals.is_dead()
    }

    /// Structured observation includes loaded status, target, inventory and optional nearby cells.
    pub fn observe(&self, world: &World, radius: i32) -> Value {
        let center = self.player.pos.floor().as_ivec3();
        let mut blocks = Vec::new();
        if radius > 0 {
            for y in -radius..=radius {
                for z in -radius..=radius {
                    for x in -radius..=radius {
                        let p = center + IVec3::new(x, y, z);
                        blocks.push(json!([p.x, p.y, p.z, world.get_block(p).map(|b| b.name())]));
                    }
                }
            }
        }
        let inventory: Vec<_> = self
            .inventory
            .slots
            .iter()
            .enumerate()
            .filter_map(|(slot, s)| {
                s.map(|s| {
                    let enchants: Vec<String> = s.enchants.lines().into_iter().map(|(l, _)| l).collect();
                    json!({"slot":slot+1,"item":s.item.name(),"display_name":s.display_name(),"count":s.count,"damage":s.damage,"enchantments":enchants})
                })
            })
            .collect();
        let offers = self
            .table_offers(world)
            .map(|(_, o)| o.map(|o| json!({"cost":o.cost,"clue":o.clue.map(|(e, l)| e.describe(l))})).to_vec());
        let target = self.target(world).map(|(p, n)| {
            json!({"position":p.to_array(),"face":n.to_array(),"block":world.get_block(p).map(|b|b.name()),"enchanting_offers":offers})
        });
        json!({"position":self.player.pos.to_array(),"yaw":self.player.yaw.to_degrees(),"pitch":self.player.pitch.to_degrees(),"loaded":world.is_loaded(center),"dimension":world.generator.dimension.name(),"health":self.vitals.health,"food":self.vitals.hunger.food,"level":self.vitals.xp.level,"xp_progress":self.vitals.xp.progress(),"dead":self.vitals.is_dead(),"mode":self.mode.name(),"creative":self.creative,"flying":self.player.flying,"sleeping":self.sleeping.is_some(),"spawn_bed":self.spawn_bed.map(|p|p.to_array()),"spawn_point":self.spawn_point.map(|p|p.to_array()),"selected":self.selected+1,"inventory":inventory,"target":target,"blocks":blocks})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{chunk::ChunkData, terrain::Generator};
    use std::sync::Arc;
    fn world() -> World {
        let mut saved = rustc_hash::FxHashMap::default();
        saved.insert(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
        let mut world = World::new_headless(Arc::new(Generator::new(1)), saved, 2);
        for _ in 0..10000 {
            world.update(DVec3::new(1.0, 150.0, 1.0));
            if world.is_loaded(IVec3::new(1, 150, 1)) {
                return world;
            }
            std::thread::yield_now();
        }
        panic!("world failed to load");
    }
    #[test]
    fn milk_clears_effects_at_full_hunger_and_stew_returns_a_bowl() {
        use crate::simulation::effects::Effect;
        let mut w = world();
        let mut e = Entities::new(6);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        w.set_block(IVec3::new(1, 149, 1), Block::STONE);
        a.inventory.slots[0] = Some(Stack::new(Item::MILK_BUCKET, 1));
        a.vitals.apply_effect(Effect::Poison, 0, 600);
        a.vitals.apply_effect(Effect::Speed, 1, 600);
        a.execute(Command::Eat, &mut w, &mut e, &[]).unwrap();
        for _ in 0..EAT_TICKS {
            a.tick(&mut w, &mut e);
        }
        assert!(!a.vitals.effects.has(Effect::Poison));
        assert!(!a.vitals.effects.has(Effect::Speed));
        assert_eq!(a.inventory.get(0).unwrap().item, Item::BUCKET);
        a.inventory.slots[0] = Some(Stack::new(Item::MUSHROOM_STEW, 1));
        a.vitals.hunger.food = 10.0;
        a.execute(Command::Eat, &mut w, &mut e, &[]).unwrap();
        for _ in 0..EAT_TICKS {
            a.tick(&mut w, &mut e);
        }
        assert_eq!(a.inventory.get(0).unwrap().item, Item::BOWL);
        assert_eq!(a.vitals.hunger.food, 16.0);
    }

    #[test]
    fn replacing_a_targeted_block_resets_mining_progress() {
        let mut world = world();
        let at = IVec3::new(3, 151, 1);
        world.set_block(IVec3::new(1, 149, 1), Block::STONE);
        world.set_block(at, Block::STONE);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        a.breaking = Some((at, Block::DIRT, 0.99));
        assert!(a.breaking(&world).is_none(), "old cracks disappear as soon as the block changes");
        a.hold(MoveInput::default(), true, false);
        a.tick(&mut world, &mut Entities::new(1));
        assert_eq!(world.get_block(at), Some(Block::STONE));
        let expected = (TICK_SECONDS / mining::dig_time(Block::STONE, a.digger(&world)) as f64) as f32;
        assert_eq!(a.breaking(&world), Some((at, expected)));
    }

    #[test]
    fn sprint_attacks_do_not_sweep_nearby_mobs() {
        let mut world = world();
        for sprint in [false, true] {
            let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
            a.player.on_ground = true;
            a.inventory.slots[0] =
                Some(Stack::new(Item::tool(crate::item::ToolKind::Sword, crate::item::Tier::Wood), 1));
            a.hold(MoveInput { forward: 1.0, sprint, ..Default::default() }, false, false);
            let mut e = Entities::new(1);
            e.spawn(crate::entity::MobKind::Zombie, DVec3::new(3.0, 150.0, 1.5));
            e.spawn(crate::entity::MobKind::Zombie, DVec3::new(3.0, 150.0, 2.3));
            a.execute(Command::Attack, &mut world, &mut e, &[]).unwrap();
            assert!(e.mobs[0].health < crate::entity::MobKind::Zombie.max_health());
            assert_eq!(e.mobs[1].health, crate::entity::MobKind::Zombie.max_health() - if sprint { 0.0 } else { 1.0 });
        }
    }

    #[test]
    fn mining_speed_changes_only_affect_future_progress() {
        use crate::enchant::Enchantment;
        let mut world = world();
        for x in 0..5 {
            for z in 0..3 {
                world.set_block(IVec3::new(x, 149, z), Block::STONE);
            }
        }
        let at = IVec3::new(3, 151, 1);
        world.set_block(at, Block::STONE);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        a.inventory.slots[0] = Some(Stack::new(Item::tool(crate::item::ToolKind::Pickaxe, crate::item::Tier::Wood), 1));
        let mut entities = Entities::new(1);
        for _ in 0..3 {
            a.hold(MoveInput::default(), true, false);
            a.tick(&mut world, &mut entities);
        }
        let before = a.breaking(&world).unwrap();
        assert_eq!(before.0, at);
        a.inventory.slots[0].as_mut().unwrap().enchants =
            crate::enchant::Enchants::NONE.with(Enchantment::Efficiency, 5);
        assert_eq!(a.breaking(&world).unwrap(), before, "changing tools cannot change existing progress");
        a.hold(MoveInput::default(), true, false);
        a.tick(&mut world, &mut entities);
        let after = a.breaking(&world).unwrap();
        let expected = before.1 + (TICK_SECONDS / mining::dig_time(Block::STONE, a.digger(&world)) as f64) as f32;
        assert!((after.1 - expected).abs() < 1e-6);
        assert_eq!(world.get_block(at), Some(Block::STONE), "a faster tool does not retroactively finish the dig");
    }

    #[test]
    fn depth_strider_bonus_is_halved_off_the_ground() {
        let mut world = world();
        world.set_block(IVec3::new(1, 149, 1), Block::STONE);
        world.set_block(IVec3::new(1, 150, 1), Block::WATER);
        let speed = |world: &World, grounded, level| {
            let mut p = Player::new(DVec3::new(1.5, 150.0, 1.5));
            p.on_ground = grounded;
            p.wear_boots(level);
            p.update(1.0 / 120.0, MoveInput { forward: 1.0, ..Default::default() }, world);
            p.vel.x
        };
        let grounded_bonus = speed(&world, true, 3) - speed(&world, true, 0);
        let airborne_bonus = speed(&world, false, 3) - speed(&world, false, 0);
        assert!(grounded_bonus > 0.0);
        assert!((airborne_bonus * 2.0 - grounded_bonus).abs() < 1e-9);
        world.set_block(IVec3::new(1, 150, 1), Block::LAVA);
        assert_eq!(speed(&world, true, 3), speed(&world, true, 0));
    }

    #[test]
    fn hurt_applies_armor_knockback_immunity_and_death_drops() {
        let mut entities = Entities::new(1);
        let mut bare = Agent::new(DVec3::new(0.5, 150.0, 0.5));
        let mut armored = Agent::new(DVec3::new(0.5, 150.0, 0.5));
        let chest = Item::armor(crate::item::ArmorPiece::Chestplate, crate::item::ArmorMaterial::Iron);
        armored.inventory.armor[1] = Some(Stack::new(chest, 1));
        let push = DVec3::new(6.0, 5.0, 0.0);
        assert_eq!(bare.hurt(4.0, "was slain by a zombie", push, &mut entities), 4.0);
        assert_eq!(bare.player.vel, push);
        assert!(armored.hurt(4.0, "was slain by a zombie", push, &mut entities) < 4.0, "armor absorbs some");
        assert!(armored.inventory.armor[1].unwrap().damage > 0, "armor wears");
        // Hurt immunity: an equal hit right after doesn't land or shove.
        assert_eq!(bare.hurt(4.0, "was slain by a zombie", push, &mut entities), 0.0);
        assert_eq!(bare.player.vel, push);
        // Full Netherite: toughness and 0.4 knockback resistance.
        let mut netherite = Agent::new(DVec3::new(0.5, 150.0, 0.5));
        for piece in crate::item::ArmorPiece::ALL {
            let item = Item::armor(piece, crate::item::ArmorMaterial::Netherite);
            netherite.inventory.armor[piece as usize] = Some(Stack::new(item, 1));
        }
        let taken = netherite.hurt(20.0, "was slain by a zombie", push, &mut entities);
        assert!((taken - 20.0 * 9.0 / 25.0).abs() < 1e-4, "{taken}");
        assert!((netherite.player.vel - push * 0.6).length() < 1e-6);

        let mut creative = Agent::new(DVec3::ZERO);
        creative.creative = true;
        assert!(!creative.targetable());
        assert_eq!(creative.hurt(30.0, "was blown up by a creeper", push, &mut entities), 0.0);

        bare.inventory.add(Item::DIAMOND, 2);
        bare.vitals.health = 1.0;
        bare.vitals.reset_fall();
        bare.hurt(30.0, "was blown up by a creeper", DVec3::ZERO, &mut entities);
        assert!(bare.vitals.is_dead() && !bare.targetable());
        assert_eq!(bare.vitals.death.as_deref(), Some("was blown up by a creeper"));
        assert!(bare.inventory.get(0).is_none());
        assert_eq!(entities.items.iter().map(|i| i.stack.count).sum::<u8>(), 2);
    }
    #[test]
    fn every_death_path_follows_keep_inventory() {
        let mut world = world();
        let mut entities = Entities::new(1);
        // `kill` drops everything, like other deaths.
        let mut killed = Agent::new(DVec3::new(0.5, 150.0, 0.5));
        killed.inventory.add(Item::DIAMOND, 3);
        killed.execute(Command::Kill, &mut world, &mut entities, &[]).unwrap();
        assert!(killed.vitals.is_dead() && killed.inventory.get(0).is_none());
        assert_eq!(entities.items.iter().map(|i| i.stack.count).sum::<u8>(), 3);

        // With keepInventory, mob damage and `kill` keep items and experience.
        let mut rules = crate::rules::GameRules::default();
        rules.set("keepInventory", "true").unwrap();
        let mut kept = Agent::new(DVec3::new(0.5, 150.0, 0.5));
        kept.tick_rules(&mut world, &mut entities, Default::default(), &rules);
        kept.inventory.add(Item::DIAMOND, 2);
        kept.vitals.xp.add_points(50);
        let xp = kept.vitals.xp;
        kept.hurt(100.0, "was slain by a zombie", DVec3::ZERO, &mut entities);
        assert!(kept.vitals.is_dead());
        assert!(kept.inventory.get(0).is_some(), "keepInventory keeps the hotbar");
        assert_eq!(kept.vitals.xp, xp, "keepInventory keeps experience");
        assert_eq!(entities.items.iter().map(|i| i.stack.count).sum::<u8>(), 3, "nothing new dropped");
    }

    #[test]
    fn drinking_a_potion_applies_it_and_leaves_a_bottle() {
        use crate::potion::Potion;
        let mut world = world();
        world.set_block(IVec3::new(1, 149, 1), Block::STONE);
        let mut entities = Entities::new(1);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        // Not hungry, but potions drink anyway.
        a.inventory.add(Item::potion(Potion::from_id("swiftness").unwrap()), 1);
        a.execute(Command::Eat, &mut world, &mut entities, &[]).unwrap();
        for _ in 0..EAT_TICKS {
            a.tick(&mut world, &mut entities);
        }
        assert_eq!(a.vitals.effects.get(Effect::Speed).map(|e| e.amplifier), Some(0));
        assert_eq!(a.inventory.get(0).map(|s| s.item), Some(Item::GLASS_BOTTLE));
    }

    #[test]
    fn eating_takes_a_full_bite_and_held_use_keeps_chewing() {
        let mut world = world();
        world.set_block(IVec3::new(1, 149, 1), Block::STONE);
        let mut entities = Entities::new(1);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        assert!(a.execute(Command::Eat, &mut world, &mut entities, &[]).is_err(), "nothing held");
        a.inventory.add(Item::BREAD, 2);
        assert!(a.execute(Command::Eat, &mut world, &mut entities, &[]).is_err(), "not hungry");
        a.vitals.hunger = crate::simulation::survival::Hunger::restore(10.0, 0.0, 0.0);
        a.execute(Command::Eat, &mut world, &mut entities, &[]).unwrap();
        for _ in 0..EAT_TICKS - 1 {
            a.tick(&mut world, &mut entities);
        }
        assert_eq!(a.inventory.get(0).unwrap().count, 2, "bite unfinished");
        a.tick(&mut world, &mut entities);
        assert_eq!(a.inventory.get(0).unwrap().count, 1);
        assert_eq!(a.vitals.hunger.food, 15.0);
        assert_eq!(a.remaining, 0);
        // A controller holding use chews until released; letting go restarts the bite.
        for _ in 0..EAT_TICKS / 2 {
            a.hold(MoveInput::default(), false, true);
            a.tick(&mut world, &mut entities);
        }
        a.hold(MoveInput::default(), false, false);
        a.tick(&mut world, &mut entities);
        assert_eq!(a.eating(), 0.0);
        for _ in 0..EAT_TICKS {
            a.hold(MoveInput::default(), false, true);
            a.tick(&mut world, &mut entities);
        }
        assert!(a.inventory.get(0).is_none());
        assert_eq!(a.vitals.hunger.food, 20.0);
    }
    #[test]
    fn resting_keeps_an_agent_in_bed_and_acting_or_hurt_wakes_it() {
        let mut world = world();
        let mut entities = Entities::new(1);
        let mut agent = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        let mut run = |agent: &mut Agent, text: &str| {
            agent.execute(Command::parse(text).unwrap(), &mut world, &mut entities, &[]).unwrap();
        };
        assert!(matches!(Command::parse("sleep"), Ok(Command::Sleep)));
        agent.sleeping = Some(1.0);
        for rest in ["wait 5", "observe", "help", "move 0 0 3 sneak"] {
            run(&mut agent, rest);
            assert!(agent.sleeping.is_some(), "{rest} keeps sleeping");
        }
        run(&mut agent, "select 2");
        assert!(agent.sleeping.is_none(), "acting gets up");
        agent.sleeping = Some(1.0);
        run(&mut agent, "move 1 0 3");
        assert!(agent.sleeping.is_none(), "walking gets up");
        agent.sleeping = Some(1.0);
        agent.hurt(1.0, "test", DVec3::ZERO, &mut entities);
        assert!(agent.sleeping.is_none(), "being hurt wakes");
    }

    #[test]
    fn respawns_at_its_bed_while_the_bed_stands() {
        let mut world = world();
        let mut entities = Entities::new(1);
        let bed = IVec3::new(1, 150, 1);
        world.set_block(bed - IVec3::Y, Block::STONE);
        world.set_block(bed, Block::BED_FOOT);
        let mut agent = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        agent.spawn_bed = Some(bed);
        let mut die_and_respawn = |agent: &mut Agent, world: &mut World| {
            agent.vitals.damage(100.0, "test", false);
            agent.execute(Command::Respawn, world, &mut entities, &[]).unwrap();
        };
        die_and_respawn(&mut agent, &mut world);
        assert_eq!(agent.player.pos, DVec3::new(1.5, 150.0 + Block::BED_FOOT.height(), 1.5));
        world.set_block(bed, Block::AIR);
        die_and_respawn(&mut agent, &mut world);
        assert_eq!(agent.spawn_bed, None, "a broken bed is forgotten");
        assert_ne!(agent.player.pos.floor().as_ivec3(), bed);
    }

    #[test]
    fn parsing_bounds() {
        for s in [
            "tp NaN 0 0",
            "look inf 0",
            "move 2 0 1",
            "wait 0",
            "mine 201",
            "select 0",
            "time NaN",
            "give dirt 0",
            "observe 3",
            "place extra",
            "effect give nope",
            "effect give speed 0",
            "effect give speed 10 300",
            "effect clear speed extra",
        ] {
            assert!(Command::parse(s).is_err(), "{s}");
        }
        assert!(matches!(
            Command::parse("effect give minecraft:strength 90 1").unwrap(),
            Command::Effect(EffectChange::Give(Effect::Strength, 90, 1))
        ));
        assert!(matches!(
            Command::parse("effect give regeneration").unwrap(),
            Command::Effect(EffectChange::Give(Effect::Regeneration, 30, 0))
        ));
        assert!(matches!(Command::parse("effect clear").unwrap(), Command::Effect(EffectChange::Clear(None))));
        let mut vitals = Vitals::default();
        vitals.health = 10.0;
        assert_eq!(EffectChange::Give(Effect::InstantDamage, 1, 0).apply(&mut vitals).0, 6.0);
        EffectChange::Give(Effect::InstantHealth, 1, 1).apply(&mut vitals);
        assert_eq!(vitals.health, 18.0);
        assert!(matches!(Command::parse("/give dirt 64").unwrap(), Command::Give(_, 64)));
        assert!(matches!(
            Command::parse("/enchant silk_touch").unwrap(),
            Command::Enchant(crate::enchant::Enchantment::SilkTouch, 1)
        ));
        assert!(Command::parse("/enchant sharpness 0").is_err() && Command::parse("/enchant speed").is_err());
        let xp = |text: &str| match Command::parse(text) {
            Ok(Command::Xp(c)) => Some((c.set, c.amount, c.levels)),
            _ => None,
        };
        assert_eq!(xp("/xp add 30 levels"), Some((false, 30, true)));
        assert_eq!(xp("xp add -5"), Some((false, -5, false)));
        assert_eq!(xp("xp set 3 points"), Some((true, 3, false)));
        assert!(xp("xp set -1 levels").is_none() && xp("xp add 5 hearts").is_none() && xp("xp add").is_none());
        assert!(matches!(Command::parse("xp query"), Ok(Command::XpQuery)));
        assert!(Command::parse("xp add 1").unwrap().cheat());
        assert!(matches!(Command::parse("gamerule keepInventory").unwrap(), Command::GameRule { .. }));
        assert!(matches!(Command::parse("time query daytime").unwrap(), Command::TimeQuery(TimeQuery::Daytime)));
        assert!(matches!(Command::parse("weather thunder").unwrap(), Command::Weather(WeatherKind::Thunder)));
        assert!(matches!(Command::parse("locate biome plains").unwrap(), Command::LocateBiome(Biome::Plains)));
        assert!(tab_complete("/gamemode surv").unwrap().starts_with("/gamemode survival"));
    }

    #[test]
    fn adventure_and_spectator_enforce_player_abilities() {
        let mut world = world();
        let mut entities = Entities::new(1);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        a.execute(Command::Mode(GameMode::Adventure), &mut world, &mut entities, &[]).unwrap();
        assert!(a.execute(Command::parse("mine 1").unwrap(), &mut world, &mut entities, &[]).is_err());
        assert!(a.targetable());

        a.execute(Command::Mode(GameMode::Spectator), &mut world, &mut entities, &[]).unwrap();
        assert!(a.player.flying && a.player.noclip && !a.targetable());
        assert!(a.execute(Command::Place, &mut world, &mut entities, &[]).is_err());
        assert_eq!(a.hurt(20.0, "test", DVec3::ZERO, &mut entities), 0.0);
    }

    #[test]
    fn hardcore_death_becomes_spectator() {
        let mut a = Agent::new(DVec3::ZERO);
        a.vitals.damage(100.0, "test", false);
        a.hardcore_spectate();
        assert!(!a.vitals.is_dead());
        assert_eq!(a.mode, GameMode::Spectator);
    }

    #[test]
    fn independent_players_and_atomic_crafting() {
        let mut world = world();
        let mut entities = Entities::new(1);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        let b = Agent::new(DVec3::new(4.5, 150.0, 1.5));
        a.inventory.add(Block::LOG, 1);
        a.execute(Command::Craft(Item::from_block(Block::PLANKS)), &mut world, &mut entities, &[]).unwrap();
        assert_eq!(a.inventory.get(0).unwrap().count, 4);
        let before = a.inventory.clone();
        assert!(
            a.execute(
                Command::Craft(Item::tool(crate::item::ToolKind::Pickaxe, crate::item::Tier::Diamond)),
                &mut world,
                &mut entities,
                &[]
            )
            .is_err()
        );
        assert_eq!(a.inventory, before);
        assert!(b.inventory.get(0).is_none());
        a.execute(Command::Mode(GameMode::Creative), &mut world, &mut entities, &[]).unwrap();
        a.execute(Command::Fly(true), &mut world, &mut entities, &[]).unwrap();
        a.execute(Command::parse("move 1 0 20").unwrap(), &mut world, &mut entities, &[]).unwrap();
        for _ in 0..20 {
            a.tick(&mut world, &mut entities);
        }
        assert!(a.player.pos.x > 8.0);
        assert_eq!(a.remaining, 0);
        assert_eq!(b.player.pos.x, 4.5);
    }
    #[test]
    fn death_drops_work_inputs_and_destroys_vanishing_gear() {
        let mut a = Agent::new(DVec3::ZERO);
        let sword = Item::tool(crate::item::ToolKind::Sword, crate::item::Tier::Iron);
        a.work[0] = Some(Stack {
            enchants: crate::enchant::Enchants::NONE.with(crate::enchant::Enchantment::VanishingCurse, 1),
            ..Stack::new(sword, 1)
        });
        a.work[1] = Some(Stack::new(Item::DIAMOND, 2));
        let mut entities = Entities::new(1);
        a.hurt(30.0, "was slain by a zombie", DVec3::ZERO, &mut entities);
        assert!(a.vitals.is_dead());
        assert_eq!(a.work, [None; 3]);
        assert_eq!(entities.items.len(), 1);
        assert_eq!(entities.items[0].stack, Stack::new(Item::DIAMOND, 2));
    }

    #[test]
    fn enchanting_can_use_the_slot_freed_by_lapis() {
        let mut world = world();
        world.set_block(IVec3::new(4, 151, 1), Block::ENCHANTING_TABLE);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        a.inventory.slots.fill(Some(Stack::new(Item::STICK, 64)));
        a.inventory.slots[0] = Some(Stack::new(Item::BOOK, 2));
        a.inventory.slots[1] = Some(Stack::new(Item::LAPIS_LAZULI, 1));
        a.vitals.xp.add_levels(30);
        a.enchant_at_table(0, &world).unwrap();
        assert_eq!(a.inventory.get(0), Some(Stack::new(Item::BOOK, 1)));
        let book = a.inventory.get(1).unwrap();
        assert_eq!(book.item, Item::ENCHANTED_BOOK);
        assert!(!book.enchants.is_empty());
        assert_eq!(a.vitals.xp.level, 29);
    }

    #[test]
    fn agents_enchant_at_a_table_for_levels_and_lapis() {
        let mut world = world();
        let mut entities = Entities::new(1);
        let pos = IVec3::new(4, 151, 1);
        world.set_block(pos, Block::ENCHANTING_TABLE);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        let pick = Item::tool(crate::item::ToolKind::Pickaxe, crate::item::Tier::Iron);
        a.inventory.slots[0] = Some(Stack::new(pick, 1));
        a.inventory.slots[1] = Some(Stack::new(Item::LAPIS_LAZULI, 2));
        let offers = a.observe(&world, 0)["target"]["enchanting_offers"].clone();
        assert_eq!(offers.as_array().map(Vec::len), Some(3), "{offers}");
        let cost = offers[1]["cost"].as_u64().unwrap() as u32;
        assert!(a.execute(Command::Enchanting(1), &mut world, &mut entities, &[]).is_err(), "no levels yet");
        a.vitals.xp.add_levels(cost as i64);
        let seed = a.vitals.xp.seed;
        a.execute(Command::Enchanting(1), &mut world, &mut entities, &[]).unwrap();
        let held = a.inventory.get(0).unwrap();
        assert!(!held.enchants.is_empty());
        assert_eq!(a.vitals.xp.level, cost - 2, "the second offer costs two levels");
        assert_eq!(a.inventory.get(1), None, "and two lapis");
        assert_ne!(a.vitals.xp.seed, seed, "a new seed rolls new offers");
        assert!(a.execute(Command::Enchanting(0), &mut world, &mut entities, &[]).is_err(), "already enchanted");
    }

    #[test]
    fn agents_combine_on_an_anvil() {
        use crate::enchant::{Enchantment, Enchants};
        let mut world = world();
        let mut entities = Entities::new(1);
        world.set_block(IVec3::new(4, 150, 1), Block::STONE);
        world.set_block(IVec3::new(4, 151, 1), Block::ANVIL);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        let sword = Item::tool(crate::item::ToolKind::Sword, crate::item::Tier::Iron);
        let book = Enchants::NONE.with(Enchantment::Sharpness, 2);
        a.inventory.slots[0] = Some(Stack::new(sword, 1));
        a.inventory.slots[1] = Some(Stack { enchants: book, ..Stack::new(Item::ENCHANTED_BOOK, 1) });
        let short = a.execute(Command::Anvil(1), &mut world, &mut entities, &[]);
        assert_eq!(short, Err("needs level 2".to_string()));
        a.vitals.xp.add_levels(3);
        a.execute(Command::Anvil(1), &mut world, &mut entities, &[]).unwrap();
        assert_eq!(a.inventory.get(0).unwrap().enchants.level(Enchantment::Sharpness), 2);
        assert_eq!(a.inventory.get(1), None, "the book is used up");
        assert_eq!(a.vitals.xp.level, 1);
    }

    #[test]
    fn agents_upgrade_held_diamond_gear_at_a_smithing_table() {
        use crate::enchant::{Enchantment, Enchants};
        use crate::item::{Tier, ToolKind};
        let mut world = world();
        let mut entities = Entities::new(1);
        world.set_block(IVec3::new(4, 151, 1), Block::SMITHING_TABLE);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        let base = Stack {
            damage: 75,
            enchants: Enchants::NONE.with(Enchantment::Efficiency, 4),
            repair_cost: 7,
            ..Stack::new(Item::tool(ToolKind::Pickaxe, Tier::Diamond), 1).with_name("Deep Delver").unwrap()
        };
        a.inventory.slots[0] = Some(base);
        a.inventory.slots[1] = Some(Stack::new(Item::NETHERITE_UPGRADE, 2));
        a.inventory.slots[2] = Some(Stack::new(Item::NETHERITE_INGOT, 3));
        a.execute(Command::Smithing, &mut world, &mut entities, &[]).unwrap();
        let result = a.inventory.get(0).unwrap();
        assert_eq!(result.item, Item::tool(ToolKind::Pickaxe, Tier::Netherite));
        assert_eq!((result.damage, result.enchants, result.repair_cost), (75, base.enchants, 7));
        assert_eq!(result.display_name(), "Deep Delver");
        assert_eq!(a.inventory.get(1).unwrap().count, 1);
        assert_eq!(a.inventory.get(2).unwrap().count, 2);
        assert!(
            a.execute(Command::Smithing, &mut world, &mut entities, &[]).is_err(),
            "Netherite cannot upgrade again"
        );
    }

    #[test]
    fn shared_chest_transfer_cannot_duplicate_items() {
        let mut world = world();
        let mut entities = Entities::new(1);
        let pos = IVec3::new(4, 151, 1);
        world.set_block(pos, Block::CHEST);
        world.chest_mut(pos).unwrap().slots[0] = Some(Stack::new(Item::DIAMOND, 1));
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        let mut b = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        a.execute(Command::Chest(true, 0), &mut world, &mut entities, &[]).unwrap();
        assert!(b.execute(Command::Chest(true, 0), &mut world, &mut entities, &[]).is_err());
        assert_eq!(a.inventory.get(0), Some(Stack::new(Item::DIAMOND, 1)));
        assert!(world.chest(pos).unwrap().slots[0].is_none());
        assert!(b.inventory.slots.iter().all(Option::is_none));
        a.execute(Command::Chest(false, 0), &mut world, &mut entities, &[]).unwrap();
        assert!(a.inventory.get(0).is_none());
        assert_eq!(world.chest(pos).unwrap().slots[0], Some(Stack::new(Item::DIAMOND, 1)));
        a.inventory.slots.fill(Some(Stack::new(Item::STICK, 64)));
        assert!(a.execute(Command::Chest(true, 0), &mut world, &mut entities, &[]).is_err());
        assert_eq!(world.chest(pos).unwrap().slots[0], Some(Stack::new(Item::DIAMOND, 1)));
    }

    #[test]
    fn placement_validates_reach_occupancy_and_spends_once() {
        let mut world = world();
        let mut entities = Entities::new(1);
        let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        a.inventory.add(Block::STONE, 2);
        world.set_block(IVec3::new(4, 151, 1), Block::STONE);
        let blocker = DVec3::new(3.5, 150.0, 1.5);
        assert!(a.execute(Command::Place, &mut world, &mut entities, &[blocker]).is_err());
        assert_eq!(a.inventory.get(0).unwrap().count, 2);
        assert_eq!(a.swings, 0, "failed placements don't swing");
        a.execute(Command::Place, &mut world, &mut entities, &[]).unwrap();
        assert_eq!(a.inventory.get(0).unwrap().count, 1);
        assert_eq!(a.swings, 1);
        assert_eq!(a.events, [Event::Placed(IVec3::new(3, 151, 1), Block::STONE)]);
        assert_eq!(world.get_block(IVec3::new(3, 151, 1)), Some(Block::STONE));
        assert!(a.execute(Command::Place, &mut world, &mut entities, &[]).is_err());
        assert_eq!(a.inventory.get(0).unwrap().count, 1);
    }
}
