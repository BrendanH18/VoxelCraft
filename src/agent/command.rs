//! Command syntax, coordinate validation and completion, separate from execution.
use crate::{
    item::Item,
    player::{MoveInput, PITCH_LIMIT, Player},
    rules::{GameMode, GameRules, RuleValue},
    simulation::{
        difficulty::Difficulty,
        effects::{self, Effect},
        survival::Vitals,
    },
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
    /// Swaps the selected hotbar stack with the off-hand (Java's `F`).
    SwapHands,
    /// Holds use with a shield (main hand, else off-hand) for this many ticks.
    Block(u32),
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
    /// `sign x y z front|back ["line", ...]`: one side's text, as the sign
    /// edit screen's Done sends it (`ServerboundSignUpdatePacket`).
    Sign(IVec3, bool, [String; 4]),
    Furnace(bool, usize),
    Drop,
    Respawn,
    Leave,
    Give(Item, u8),
    Clear,
    Kill,
    Summon(crate::entity::MobKind, PositionSpec),
    SummonLightning(PositionSpec),
    Mode(GameMode),
    /// Java's `/tp`: a position, then optionally a rotation or a `facing`
    /// target.
    Teleport(PositionSpec, Option<Turn>),
    /// Java's `/rotate` (added in 1.21.2).
    Rotate(Turn),
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
    "teleport",
    "rotate",
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
    "furnace",
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

    /// Java's `Vec3Argument`: a whole absolute x or z names the block's
    /// centre, so it gains 0.5 (`WorldCoordinate.parseDouble` with
    /// `centerCorrect`). Y and `~` offsets stay as typed.
    pub fn parse_centered(x: &str, y: &str, z: &str) -> Result<Self, String> {
        let center = |text: &str, c: Coordinate| match c {
            Coordinate::Absolute(v) if !text.contains('.') => Coordinate::Absolute(v + 0.5),
            c => c,
        };
        let Self([cx, cy, cz]) = Self::parse(x, y, z)?;
        Ok(Self([center(x, cx), cy, center(z, cz)]))
    }
}

/// Java's `EntityAnchorArgument`: which point of an entity to face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    Feet,
    Eyes,
}

/// How `/tp` and `/rotate` turn the player: Java's `rotation` argument
/// (yaw then pitch, in Java degrees) or a `facing` target (`LookAt`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Turn {
    Rotation {
        yaw: Coordinate,
        pitch: Coordinate,
    },
    /// `facing <x y z>`. `~` counts from where the command ran, before any
    /// teleport, because Java resolves arguments first.
    FacingPos(PositionSpec),
    /// `facing entity @s|@p [eyes|feet]`. The selectors the parser knows
    /// only reach the player, so the target is the player itself.
    FacingSelf(Anchor),
}

/// Java's `Mth.wrapDegrees`: an angle in [-180, 180).
fn wrap_degrees(angle: f64) -> f64 {
    let a = angle % 360.0;
    if a >= 180.0 {
        a - 360.0
    } else if a < -180.0 {
        a + 360.0
    } else {
        a
    }
}

/// Java's yaw and pitch in degrees from VoxelCraft's radians. VoxelCraft yaw
/// 0 faces +X and Java yaw 0 faces +Z (south); VoxelCraft pitch is positive
/// looking up, Java's is negative.
pub fn java_rotation(yaw: f32, pitch: f32) -> (f64, f64) {
    (wrap_degrees(f64::from(yaw).to_degrees() - 90.0), -f64::from(pitch).to_degrees())
}

/// VoxelCraft's yaw and pitch in radians from Java degrees. The pitch keeps
/// to the camera's limit.
pub fn from_java_rotation(yaw: f64, pitch: f64) -> (f32, f32) {
    // Wrap in f64 first: a huge finite yaw would become infinite as f32.
    let yaw = (wrap_degrees(yaw + 90.0).to_radians() as f32).rem_euclid(std::f32::consts::TAU);
    (yaw, (-pitch.to_radians() as f32).clamp(-PITCH_LIMIT, PITCH_LIMIT))
}

/// Java's `Entity.lookAt`: the yaw and pitch in degrees that look from
/// `from` to `to`.
pub fn java_look_at(from: DVec3, to: DVec3) -> (f64, f64) {
    let d = to - from;
    let pitch = wrap_degrees(-d.y.atan2(d.x.hypot(d.z)).to_degrees());
    let yaw = wrap_degrees(d.z.atan2(d.x).to_degrees() - 90.0);
    (yaw, pitch.clamp(-90.0, 90.0))
}

impl Turn {
    /// The player's Java yaw and pitch after the turn, in degrees. `origin`
    /// is where the command ran; `player` already stands where it ends up.
    /// `/tp` wraps a rotation's change into [-180, 180) before adding it
    /// (`TeleportCommand.performTeleport`); `/rotate` does not.
    fn java_angles(self, origin: DVec3, player: &Player, wrap: bool) -> Result<(f64, f64), String> {
        // Java's command source looks from its feet unless `execute
        // anchored eyes` changes the anchor, which VoxelCraft lacks.
        let from = player.pos;
        match self {
            Turn::Rotation { yaw, pitch } => {
                let (now_yaw, now_pitch) = java_rotation(player.yaw, player.pitch);
                let angle = |c: Coordinate, now: f64| {
                    let (base, change) = match c {
                        Coordinate::Absolute(v) => (0.0, v),
                        Coordinate::Relative(v) => (now, v),
                    };
                    base + if wrap { wrap_degrees(change) } else { change }
                };
                // `Entity.setXRot` keeps the pitch within 90 degrees.
                Ok((angle(yaw, now_yaw), (angle(pitch, now_pitch) % 360.0).clamp(-90.0, 90.0)))
            }
            Turn::FacingPos(spec) => Ok(java_look_at(from, spec.resolve(origin)?)),
            Turn::FacingSelf(Anchor::Feet) => Ok(java_look_at(from, player.pos)),
            Turn::FacingSelf(Anchor::Eyes) => Ok(java_look_at(from, player.eye())),
        }
    }
}

/// Java's `/tp`: moves the player to `pos`, then applies `turn`. Nothing
/// changes when the position or the turn is invalid.
pub fn teleport(player: &mut Player, pos: PositionSpec, turn: Option<Turn>) -> Result<(), String> {
    let origin = player.pos;
    let to = pos.resolve(origin)?;
    player.pos = to;
    if let Some(turn) = turn {
        match turn.java_angles(origin, player, true) {
            Ok((yaw, pitch)) => (player.yaw, player.pitch) = from_java_rotation(yaw, pitch),
            Err(e) => {
                player.pos = origin;
                return Err(e);
            }
        }
    }
    Ok(())
}

/// Java's `/rotate`: turns the player where it stands.
pub fn rotate(player: &mut Player, turn: Turn) -> Result<(), String> {
    let (yaw, pitch) = turn.java_angles(player.pos, player, false)?;
    (player.yaw, player.pitch) = from_java_rotation(yaw, pitch);
    Ok(())
}

/// The rotation or `facing` words after `/tp`'s position or `/rotate`'s
/// target.
fn parse_turn(words: &[&str]) -> Result<Option<Turn>, String> {
    let usage = || "use: yaw pitch | facing x y z | facing entity @s|@p [eyes|feet]".to_string();
    Ok(Some(match words {
        [] => return Ok(None),
        ["facing", "entity", target, anchor @ ..] => {
            if !matches!(*target, "@s" | "@p") {
                return Err("facing entity accepts @s or @p".into());
            }
            Turn::FacingSelf(match anchor {
                [] | ["feet"] => Anchor::Feet,
                ["eyes"] => Anchor::Eyes,
                _ => return Err(usage()),
            })
        }
        ["facing", x, y, z] => Turn::FacingPos(PositionSpec::parse_centered(x, y, z)?),
        ["facing", ..] => return Err(usage()),
        [yaw, pitch] => Turn::Rotation { yaw: Coordinate::parse(yaw)?, pitch: Coordinate::parse(pitch)? },
        _ => return Err(usage()),
    }))
}

/// `/effect give` or `/effect clear`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EffectChange {
    /// The effect, its duration in ticks ([`Effect::command_duration_ticks`]
    /// of the typed number, or [`effects::INFINITE`] for `infinite`), its
    /// amplifier and whether it shows particles (`hideParticles` inverted).
    Give(Effect, u32, u8, bool),
    /// Removes one effect, or all of them.
    Clear(Option<Effect>),
}

/// en_us `commands.effect.*`, which `EffectCommands` sends for one target.
pub mod effect_feedback {
    pub const GIVE_SUCCESS: &str = "Applied effect %s to %s";
    pub const GIVE_FAILED: &str =
        "Unable to apply this effect (target is either immune to effects, or has something stronger)";
    pub const CLEAR_EVERYTHING_SUCCESS: &str = "Removed every effect from %s";
    pub const CLEAR_EVERYTHING_FAILED: &str = "Target has no effects to remove";
    pub const CLEAR_SPECIFIC_SUCCESS: &str = "Removed effect %s from %s";
    pub const CLEAR_SPECIFIC_FAILED: &str = "Target doesn't have the requested effect";

    /// Fills the `%s` slots in order.
    pub fn fill(template: &str, args: &[&str]) -> String {
        let mut out = String::new();
        let mut args = args.iter();
        let mut parts = template.split("%s");
        out.push_str(parts.next().unwrap_or_default());
        for part in parts {
            out.push_str(args.next().copied().unwrap_or_default());
            out.push_str(part);
        }
        out
    }
}

impl EffectChange {
    /// Applies the change to the player named `target`, as Java's
    /// `EffectCommands` does: instant damage for the caller to deal and the
    /// success line, or the failure Java throws when nothing changed.
    pub fn apply(self, vitals: &mut Vitals, target: &str) -> Result<(f32, String), String> {
        use effect_feedback::*;
        match self {
            EffectChange::Give(effect, ticks, amp, particles) => {
                let instance = effects::Active::new(effect, amp, ticks.max(1)).with_flags(false, particles, particles);
                let (damage, changed) = vitals.give_effect(instance);
                if !changed {
                    return Err(GIVE_FAILED.into());
                }
                Ok((damage, fill(GIVE_SUCCESS, &[effect.name(), target])))
            }
            EffectChange::Clear(Some(effect)) => {
                if !vitals.effects.remove(effect) {
                    return Err(CLEAR_SPECIFIC_FAILED.into());
                }
                Ok((0.0, fill(CLEAR_SPECIFIC_SUCCESS, &[effect.name(), target])))
            }
            EffectChange::Clear(None) => {
                if vitals.effects.is_empty() {
                    return Err(CLEAR_EVERYTHING_FAILED.into());
                }
                vitals.effects.clear();
                Ok((0.0, fill(CLEAR_EVERYTHING_SUCCESS, &[target])))
            }
        }
    }
}

/// Brigadier's `StringReader.readInt`: digits, `.` and `-` only (no `+`),
/// then `Integer.parseInt`.
fn java_int(text: &str) -> Option<i32> {
    text.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-').then(|| text.parse().ok()).flatten()
}

/// Brigadier's `IntegerArgumentType.integer(min, max)`.
fn java_int_in(text: &str, min: i32, max: i32) -> Option<i32> {
    java_int(text).filter(|n| (min..=max).contains(n))
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
                "trail_ruins",
                "ancient_city",
                "trial_chambers",
                "nether_fossil",
                "ruined_portal",
                "buried_treasure",
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
            let mut names: Vec<String> =
                crate::entity::MobKind::ALL.iter().map(|k| k.name().replace(' ', "_")).collect();
            names.push("lightning_bolt".into());
            let refs: Vec<&str> = names.iter().map(String::as_str).collect();
            complete_options(&refs, partial)?
        }
        "effect" if stem.len() <= 1 => complete_options(&["give", "clear"], partial)?,
        // The effect id, after an optional `@s`/`@p` target.
        "effect"
            if matches!(stem.get(1), Some(&("give" | "clear")))
                && (stem.len() == 2 || stem.len() == 3 && matches!(stem[2], "@s" | "@p")) =>
        {
            let ids: Vec<&str> = Effect::ALL.iter().map(|e| e.id()).collect();
            complete_options(&ids, partial.strip_prefix("minecraft:").unwrap_or(partial))?
        }
        // `/effect give`'s `infinite` literal and `hideParticles` boolean.
        "effect" if stem.get(1) == Some(&"give") => {
            let target = usize::from(matches!(stem.get(2), Some(&("@s" | "@p"))));
            match stem.len() - target {
                3 => complete_options(&["infinite"], partial)?,
                5 => complete_options(&["true", "false"], partial)?,
                _ => return None,
            }
        }
        "xp" | "experience" if stem.len() <= 1 => complete_options(&["add", "set", "query"], partial)?,
        "rotate" if stem.len() == 1 => complete_options(&["@s", "@p"], partial)?,
        "rotate" if stem.len() == 2 => complete_options(&["facing"], partial)?,
        "tp" | "teleport" if stem.len() == 5 && stem[1].starts_with('@') => complete_options(&["facing"], partial)?,
        "tp" | "teleport" | "rotate" if stem.last() == Some(&"facing") => complete_options(&["entity"], partial)?,
        "tp" | "teleport" | "rotate" if stem.last() == Some(&"entity") => complete_options(&["@s", "@p"], partial)?,
        "tp" | "teleport" | "rotate" if stem.len() > 2 && stem[stem.len() - 2] == "entity" => {
            complete_options(&["eyes", "feet"], partial)?
        }
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
    /// `sign x y z front|back` and a JSON array of up to four lines, which
    /// keeps the lines' spaces.
    fn parse_sign(text: &str) -> Option<Self> {
        let mut rest = text.trim().trim_start_matches('/').strip_prefix("sign")?;
        let mut word = || {
            let t = rest.trim_start();
            let end = t.find(char::is_whitespace).unwrap_or(t.len());
            rest = &t[end..];
            Some(&t[..end]).filter(|w| !w.is_empty())
        };
        let mut coordinate = || word()?.parse::<i32>().ok();
        let pos = IVec3::new(coordinate()?, coordinate()?, coordinate()?);
        let front = match word()? {
            "front" => true,
            "back" => false,
            _ => return None,
        };
        let given: Vec<String> = serde_json::from_str(rest.trim()).ok()?;
        if given.len() > 4 {
            return None;
        }
        let mut lines: [String; 4] = Default::default();
        for (line, text) in lines.iter_mut().zip(given) {
            *line = text;
        }
        Some(Self::Sign(pos, front, lines))
    }

    /// Parse slash commands and CLI input identically, rejecting extra arguments and non-finite numbers.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut words: Vec<_> = text.trim().trim_start_matches('/').split_whitespace().collect();
        // Java's `effect give @s …`, `enchant @s …` and `xp add @s …` name a
        // target; the only targets are the executor, so drop it.
        let target_at = match words.as_slice() {
            ["effect", "give" | "clear", ..] | ["xp" | "experience", "add" | "set" | "query", ..] => Some(2),
            ["enchant", ..] => Some(1),
            _ => None,
        };
        if let Some(i) = target_at.filter(|&i| matches!(words.get(i), Some(&("@s" | "@p")))) {
            words.remove(i);
        }
        let bad = || "unknown command or arguments; use /help".to_string();
        if words.first() == Some(&"sign") {
            return Self::parse_sign(text).ok_or_else(|| "use: sign x y z front|back [\"line\", ...]".to_string());
        }
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
                        ..Default::default()
                    },
                    ticks(n)?,
                    false,
                )
            }
            ["eat"] => Self::Eat,
            ["swap"] | ["swaphands"] | ["offhand"] => Self::SwapHands,
            ["block"] => Self::Block(40),
            ["block", n] => Self::Block(ticks(n)?),
            ["draw"] => Self::Draw(25),
            ["draw", n] => Self::Draw(ticks(n)?),
            ["sleep"] => Self::Sleep,
            ["place"] => Self::Place,
            ["attack"] => Self::Attack,
            ["select", n] => Self::Select(n.parse::<usize>().ok().filter(|n| (1..=9).contains(n)).ok_or_else(bad)? - 1),
            ["fly", "on"] => Self::Fly(true),
            ["fly", "off"] => Self::Fly(false),
            ["craft", name] => Self::Craft(item(name)?),
            ["furnace", action @ ("take" | "put"), slot] => Self::Furnace(
                *action == "take",
                match *slot {
                    "input" => 0,
                    "fuel" => 1,
                    "output" => 2,
                    _ => return Err(bad()),
                },
            ),
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
            ["summon", "lightning_bolt" | "minecraft:lightning_bolt"] => {
                Self::SummonLightning(PositionSpec([Coordinate::Relative(0.0); 3]))
            }
            ["summon", "lightning_bolt" | "minecraft:lightning_bolt", x, y, z] => {
                Self::SummonLightning(PositionSpec::parse(x, y, z)?)
            }
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
            // Java's target-less `/tp <location>` takes no rotation.
            ["tp" | "teleport", x, y, z] => Self::Teleport(PositionSpec::parse_centered(x, y, z)?, None),
            ["tp" | "teleport", "@s" | "@p", x, y, z, turn @ ..] => {
                Self::Teleport(PositionSpec::parse_centered(x, y, z)?, parse_turn(turn)?)
            }
            ["rotate", "@s" | "@p", turn @ ..] => Self::Rotate(parse_turn(turn)?.ok_or_else(bad)?),
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
            ["locate", "trail_ruins"] => Self::LocateStructure("trail_ruins".into()),
            ["locate", "ancient_city"] => Self::LocateStructure("ancient_city".into()),
            ["locate", "trial_chambers"] => Self::LocateStructure("trial_chambers".into()),
            ["locate", "nether_fossil"] => Self::LocateStructure("nether_fossil".into()),
            ["locate", "ruined_portal"] => Self::LocateStructure("ruined_portal".into()),
            ["locate", "buried_treasure"] => Self::LocateStructure("buried_treasure".into()),
            ["locate", "structure", name] => Self::LocateStructure(name.to_string()),
            ["locate", "biome", name] => match crate::world::nether_biome::NetherBiome::from_name(name) {
                Some(nether) => Self::LocateNetherBiome(nether),
                None => Self::LocateBiome(Biome::from_name(name).ok_or_else(bad)?),
            },
            ["seed"] => Self::Seed,
            ["dimension", name] => Self::Dimension(Dimension::from_name(name).ok_or_else(bad)?),
            ["xp" | "experience", "query"] | ["xp" | "experience", "query", "points" | "levels"] => Self::XpQuery,
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
            // `EffectCommands`: `<seconds 1..1000000>` or `infinite`, then
            // `<amplifier 0..255>` and `<hideParticles>`. Java's defaults: 30
            // seconds (one tick for an instantaneous effect, whose number
            // counts ticks), level I, particles shown.
            ["effect", "give", name, rest @ ..] if rest.len() <= 3 => {
                let effect = Effect::from_id(name).ok_or_else(bad)?;
                let ticks = match rest.first() {
                    Some(&"infinite") => effects::INFINITE,
                    Some(n) => {
                        effect.command_duration_ticks(Some(java_int_in(n, 1, 1_000_000).ok_or_else(bad)? as u32))
                    }
                    None => effect.command_duration_ticks(None),
                };
                let amp = rest.get(1).map_or(Ok(0), |n| java_int_in(n, 0, 255).map(|n| n as u8).ok_or_else(bad))?;
                let hide = match rest.get(2) {
                    None | Some(&"false") => false,
                    Some(&"true") => true,
                    Some(_) => return Err(bad()),
                };
                Self::Effect(EffectChange::Give(effect, ticks, amp, !hide))
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
                | Self::SummonLightning(..)
                | Self::Mode(..)
                | Self::Teleport(..)
                | Self::Rotate(..)
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

#[cfg(test)]
mod tests;
