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

    /// Default Java 26.3 block interaction range, including Creative's modifier.
    pub const fn block_reach(self) -> f64 {
        if self.is_creative() { 5.0 } else { 4.5 }
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

/// How much of one Java rule VoxelCraft enforces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Support {
    /// Enforced everywhere the rule applies to systems that exist.
    Implemented,
    /// Enforced for some of the systems Java gates; `docs/game-rules.md`
    /// lists what is not.
    Partial,
    /// Governs a system VoxelCraft does not have yet; the release that adds it.
    Missing(&'static str),
    /// Server, command-block or multiplayer behaviour with no single-player
    /// effect; the reason.
    NotApplicable(&'static str),
}

impl Support {
    /// Whether the rule can be read and set.
    pub const fn enforced(self) -> bool {
        matches!(self, Self::Implemented | Self::Partial)
    }

    /// Label used by the inventory table in `docs/game-rules.md`.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Implemented => "Implemented",
            Self::Partial => "Partial",
            Self::Missing(_) => "Missing",
            Self::NotApplicable(_) => "Not applicable",
        }
    }
}

/// One rule of Java 26.3's `GameRules` registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleDef {
    /// Registry id without the `minecraft:` namespace, as `/gamerule` takes it.
    pub id: &'static str,
    /// The camelCase name used before 26.1 and by VoxelCraft saves; accepted
    /// as an alias everywhere and used as the saved key.
    pub legacy: &'static str,
    kind: RuleType,
    default: RuleValue,
    pub support: Support,
}

const fn boolean(id: &'static str, legacy: &'static str, default: bool, support: Support) -> RuleDef {
    RuleDef { id, legacy, kind: RuleType::Bool, default: RuleValue::Bool(default), support }
}

const fn integer(
    id: &'static str,
    legacy: &'static str,
    default: i32,
    min: i32,
    max: i32,
    support: Support,
) -> RuleDef {
    RuleDef { id, legacy, kind: RuleType::Int { min, max }, default: RuleValue::Int(default), support }
}

impl RuleDef {
    pub const fn default_value(&self) -> RuleValue {
        self.default
    }

    pub const fn is_int(&self) -> bool {
        matches!(self.kind, RuleType::Int { .. })
    }

    /// Inclusive bounds of an integer rule.
    pub const fn range(&self) -> Option<(i32, i32)> {
        match self.kind {
            RuleType::Int { min, max } => Some((min, max)),
            RuleType::Bool => None,
        }
    }
}

use Support::{Implemented as I, Missing as M, NotApplicable as N, Partial as P};

/// Every rule registered by Java 26.3's `GameRules` class, in registration
/// order, with Java's type, default and range (read from the class
/// initializer), the legacy name from `GameRuleRegistryFix`, and what
/// VoxelCraft enforces. `docs/game-rules.md` mirrors this table and a test
/// keeps them identical.
pub const JAVA_RULES: [RuleDef; 59] = [
    boolean("advance_time", "doDaylightCycle", true, I),
    boolean("advance_weather", "doWeatherCycle", true, I),
    boolean("allow_entering_nether_using_portals", "allowEnteringNetherUsingPortals", true, I),
    boolean("block_drops", "doTileDrops", true, P),
    boolean("block_explosion_drop_decay", "blockExplosionDropDecay", true, I),
    boolean("command_blocks_work", "enableCommandBlocks", true, M("unscheduled")),
    boolean("command_block_output", "commandBlockOutput", true, M("unscheduled")),
    boolean("drowning_damage", "drowningDamage", true, I),
    boolean("elytra_movement_check", "disableElytraMovementCheck", true, N("server anti-cheat")),
    boolean("ender_pearls_vanish_on_death", "enderPearlsVanishOnDeath", true, I),
    boolean("entity_drops", "doEntityDrops", true, P),
    boolean("fall_damage", "fallDamage", true, I),
    boolean("fire_damage", "fireDamage", true, I),
    integer("fire_spread_radius_around_player", "fireSpreadRadiusAroundPlayer", 128, -1, i32::MAX, I),
    boolean("forgive_dead_players", "forgiveDeadPlayers", true, M("v0.17")),
    boolean("freeze_damage", "freezeDamage", true, I),
    boolean("global_sound_events", "globalSoundEvents", true, N("broadcast range only")),
    boolean("immediate_respawn", "doImmediateRespawn", false, I),
    boolean("keep_inventory", "keepInventory", false, I),
    boolean("lava_source_conversion", "lavaSourceConversion", false, I),
    boolean("limited_crafting", "doLimitedCrafting", false, M("v0.20")),
    boolean("locator_bar", "locatorBar", true, M("unscheduled")),
    boolean("log_admin_commands", "logAdminCommands", true, N("server log")),
    integer("max_block_modifications", "commandModificationBlockLimit", 32768, 1, i32::MAX, N("no fill or clone")),
    integer("max_command_forks", "maxCommandForkCount", 65536, 0, i32::MAX, N("no functions")),
    integer("max_command_sequence_length", "maxCommandChainLength", 65536, 0, i32::MAX, N("no functions")),
    integer("max_entity_cramming", "maxEntityCramming", 24, 0, i32::MAX, M("v0.19")),
    integer("max_minecart_speed", "minecartMaxSpeed", 8, 1, 1000, N("experimental minecart rewrite")),
    integer("max_snow_accumulation_height", "snowAccumulationHeight", 1, 0, 8, M("v0.15")),
    boolean("mob_drops", "doMobLoot", true, I),
    boolean("mob_explosion_drop_decay", "mobExplosionDropDecay", true, I),
    boolean("mob_griefing", "mobGriefing", true, P),
    boolean("natural_health_regeneration", "naturalRegeneration", true, I),
    boolean("player_movement_check", "disablePlayerMovementCheck", true, N("server anti-cheat")),
    integer("players_nether_portal_creative_delay", "playersNetherPortalCreativeDelay", 0, 0, i32::MAX, I),
    integer("players_nether_portal_default_delay", "playersNetherPortalDefaultDelay", 80, 0, i32::MAX, I),
    integer("players_sleeping_percentage", "playersSleepingPercentage", 100, 0, i32::MAX, I),
    boolean("projectiles_can_break_blocks", "projectilesCanBreakBlocks", true, M("v0.12")),
    boolean("pvp", "pvp", true, M("v1.x")),
    boolean("raids", "disableRaids", true, M("v0.17")),
    integer("random_tick_speed", "randomTickSpeed", 3, 0, i32::MAX, I),
    boolean("reduced_debug_info", "reducedDebugInfo", false, M("v0.20")),
    integer("respawn_radius", "spawnRadius", 10, 0, i32::MAX, I),
    boolean("send_command_feedback", "sendCommandFeedback", true, I),
    boolean("show_advancement_messages", "announceAdvancements", true, N("no advancements")),
    boolean("show_death_messages", "showDeathMessages", true, P),
    boolean("spawner_blocks_work", "spawnerBlocksEnabled", true, I),
    boolean("spawn_mobs", "doMobSpawning", true, I),
    boolean("spawn_monsters", "spawnMonsters", true, I),
    boolean("spawn_patrols", "doPatrolSpawning", true, M("v0.17")),
    boolean("spawn_phantoms", "doInsomnia", true, M("v0.15")),
    boolean("spawn_wandering_traders", "doTraderSpawning", true, I),
    boolean("spawn_wardens", "doWardenSpawning", true, M("v0.16")),
    boolean("spectators_generate_chunks", "spectatorsGenerateChunks", true, M("v0.13")),
    boolean("spread_vines", "doVinesSpread", true, M("v0.13")),
    boolean("tnt_explodes", "tntExplodes", true, I),
    boolean("tnt_explosion_drop_decay", "tntExplosionDropDecay", false, I),
    boolean("universal_anger", "universalAnger", false, M("v1.x")),
    boolean("water_source_conversion", "waterSourceConversion", true, I),
];

const FIRE_RADIUS: usize = 13;
const LEGACY_FIRE_TICK: &str = "doFireTick";
const LEGACY_FIRE_AWAY: &str = "allowFireTicksAwayFromPlayer";

fn resolve(name: &str) -> Option<usize> {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    JAVA_RULES.iter().position(|rule| rule.id == name || rule.legacy == name)
}

/// Typed, saved gamerule state. Private storage keeps all mutation going
/// through the registry's type and range checks. Rules are saved under their
/// legacy camelCase names, so older builds read newer saves; every name
/// Java 26.3 accepts (the id, with or without `minecraft:`) loads too.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameRules {
    values: [RuleValue; JAVA_RULES.len()],
}

impl Default for GameRules {
    fn default() -> Self {
        Self { values: JAVA_RULES.map(|rule| rule.default) }
    }
}

impl GameRules {
    /// Ids of the rules the engine enforces, as `/gamerule` completes them.
    pub fn names() -> impl Iterator<Item = &'static str> {
        JAVA_RULES.iter().filter(|rule| rule.support.enforced()).map(|rule| rule.id)
    }

    /// The registry entry for any accepted spelling of an enforced rule.
    pub fn definition(name: &str) -> Option<&'static RuleDef> {
        resolve(name).map(|i| &JAVA_RULES[i]).filter(|rule| rule.support.enforced())
    }

    /// Why `name` cannot be used, for command feedback.
    pub fn unknown(name: &str) -> String {
        match resolve(name).map(|i| JAVA_RULES[i]) {
            Some(rule) => match rule.support {
                Support::Missing(release) => {
                    format!("gamerule {} is not implemented yet (planned: {release})", rule.id)
                }
                Support::NotApplicable(why) => format!("gamerule {} has no single-player effect ({why})", rule.id),
                _ => format!("unknown gamerule: {name}"),
            },
            None => format!("unknown gamerule: {name}"),
        }
    }

    pub fn get(&self, name: &str) -> Option<RuleValue> {
        // Before 26.1 fire ticking was two booleans; a saved or typed
        // `doFireTick` reads as "is fire ticking at all".
        if name == LEGACY_FIRE_TICK {
            return Some(RuleValue::Bool(self.int("fire_spread_radius_around_player") != 0));
        }
        if name == LEGACY_FIRE_AWAY {
            return Some(RuleValue::Bool(self.int("fire_spread_radius_around_player") < 0));
        }
        let i = resolve(name).filter(|&i| JAVA_RULES[i].support.enforced())?;
        Some(self.values[i])
    }

    /// Parses and sets one rule, rejecting the wrong type and out-of-range
    /// integers. Returns its typed value for command feedback.
    pub fn set(&mut self, name: &str, text: &str) -> Result<RuleValue, String> {
        if name == LEGACY_FIRE_TICK || name == LEGACY_FIRE_AWAY {
            let on = parse_bool(name, text)?;
            let radius = self.values[FIRE_RADIUS];
            let RuleValue::Int(current) = radius else { unreachable!("fire radius is an integer") };
            // Java 26.3's `GameRuleRegistryFix`: no ticking is 0, ticking
            // beside players only is 128, and ticking everywhere is -1.
            self.values[FIRE_RADIUS] = RuleValue::Int(if name == LEGACY_FIRE_TICK {
                match (on, current) {
                    (false, _) => 0,
                    (true, 0) => 128,
                    (true, kept) => kept,
                }
            } else {
                match (on, current) {
                    (_, 0) => 0,
                    (true, _) => -1,
                    (false, -1) => 128,
                    (false, kept) => kept,
                }
            });
            return Ok(RuleValue::Bool(on));
        }
        let i = resolve(name).filter(|&i| JAVA_RULES[i].support.enforced()).ok_or_else(|| Self::unknown(name))?;
        let rule = &JAVA_RULES[i];
        let value = match rule.kind {
            RuleType::Bool => RuleValue::Bool(parse_bool(rule.id, text)?),
            RuleType::Int { min, max } => {
                let value = text.parse::<i32>().map_err(|_| format!("{} needs an integer", rule.id))?;
                if !(min..=max).contains(&value) {
                    return Err(format!("{} must be {min}..{max}", rule.id));
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
        JAVA_RULES
            .iter()
            .zip(self.values)
            .filter(|(rule, value)| rule.support.enforced() && *value != rule.default)
            .map(|(rule, value)| format!("{}={value}", rule.legacy))
            .collect::<Vec<_>>()
            .join(";")
    }

    /// Restores valid entries over Java defaults; malformed future or old
    /// entries are ignored independently. Entries may use the saved legacy
    /// names or Java ids, and the two pre-26.1 fire booleans.
    pub fn deserialize(text: &str) -> Self {
        let mut rules = Self::default();
        let entries: Vec<(&str, &str)> = text.split(';').filter_map(|entry| entry.split_once('=')).collect();
        // Fire entries combine, so apply them in `GameRuleRegistryFix` order
        // regardless of where they appear.
        for (name, value) in &entries {
            if *name != LEGACY_FIRE_TICK && *name != LEGACY_FIRE_AWAY {
                let _ = rules.set(name, value);
            }
        }
        for wanted in [LEGACY_FIRE_TICK, LEGACY_FIRE_AWAY] {
            if let Some((_, value)) = entries.iter().rev().find(|(name, _)| *name == wanted) {
                let _ = rules.set(wanted, value);
            }
        }
        rules
    }
}

/// Java's `Level.ExplosionInteraction` for the explosions VoxelCraft has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExplosionSource {
    /// Creepers, ghast fireballs and other mobs: gated by `mob_griefing`.
    Mob,
    /// TNT and TNT minecarts.
    Tnt,
    /// Beds, respawn anchors and end crystals.
    Block,
}

impl ExplosionSource {
    /// Classifies a blast by the death message it carries.
    pub fn of_cause(cause: &str) -> Self {
        if cause.contains("TNT") {
            Self::Tnt
        } else if cause.contains("creeper") || cause.contains("ghast") {
            Self::Mob
        } else {
            Self::Block
        }
    }
}

/// What an explosion does to blocks under the current rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExplosionRules {
    /// Java's `KEEP` interaction (false) or one that destroys blocks.
    pub destroys_blocks: bool,
    /// `DESTROY_WITH_DECAY`: destroyed blocks drop with chance `1 / power`
    /// instead of always.
    pub drop_decay: bool,
}

/// The damage the player-only rules `fall_damage`, `fire_damage`,
/// `drowning_damage` and `freeze_damage` make a player immune to. Java checks
/// the damage type's tag in `Player.isInvulnerableTo`; VoxelCraft damage
/// carries its death message, so the message selects the tag.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DamageImmunity {
    /// `is_fall`: fall, ender pearl, stalagmite.
    pub fall: bool,
    /// `is_fire`: in fire, campfire, burning, lava, hot floor, blaze fireballs.
    /// A ghast fireball's blast is explosion damage and is not included.
    pub fire: bool,
    /// `is_drowning`.
    pub drowning: bool,
    /// `is_freezing`.
    pub freezing: bool,
}

impl DamageImmunity {
    /// Whether damage with this death message is ignored.
    pub fn blocks(self, cause: &str) -> bool {
        if cause.starts_with("fell") || cause.starts_with("hit the ground") {
            self.fall
        } else if cause.starts_with("burned") || cause.contains("lava") || cause.contains("fireballed by a blaze") {
            self.fire
        } else if cause.starts_with("drowned") {
            self.drowning
        } else if cause.contains("froze") {
            self.freezing
        } else {
            false
        }
    }
}

impl GameRules {
    /// The rules which change how blocks tick, for [`crate::world::World::rules`].
    pub fn world_rules(&self) -> crate::world::WorldRules {
        crate::world::WorldRules {
            tnt_explodes: self.bool("tnt_explodes"),
            water_source_conversion: self.bool("water_source_conversion"),
            lava_source_conversion: self.bool("lava_source_conversion"),
            fire_spread_radius: self.int("fire_spread_radius_around_player"),
            difficulty: crate::world::WorldRules::default().difficulty,
        }
    }

    /// `ServerLevel.explode`'s choice of block interaction: mobs destroy
    /// blocks only with `mob_griefing`, and each source has its own decay rule.
    pub fn explosion(&self, source: ExplosionSource) -> ExplosionRules {
        match source {
            ExplosionSource::Mob => ExplosionRules {
                destroys_blocks: self.bool("mob_griefing"),
                drop_decay: self.bool("mob_explosion_drop_decay"),
            },
            ExplosionSource::Tnt => {
                ExplosionRules { destroys_blocks: true, drop_decay: self.bool("tnt_explosion_drop_decay") }
            }
            ExplosionSource::Block => {
                ExplosionRules { destroys_blocks: true, drop_decay: self.bool("block_explosion_drop_decay") }
            }
        }
    }

    /// Whether a blast with this death message sets fires
    /// (`ServerExplosion.createFire`): beds and respawn anchors always
    /// (`BedBlock`, `RespawnAnchorBlock`), ghast fireballs with
    /// `mob_griefing` (`LargeFireball.onHit`). Creepers, TNT and end
    /// crystals never do.
    pub fn explosion_fire(&self, cause: &str) -> bool {
        cause.contains("[Intentional Game Design]") || cause.contains("ghast") && self.bool("mob_griefing")
    }

    /// `NetherPortalBlock.getPortalTransitionTime`: ticks a player stands in
    /// a Nether portal before it takes them. Invulnerable players (Creative
    /// and Spectator) use their own delay.
    pub fn portal_delay_ticks(&self, invulnerable: bool) -> u32 {
        let rule =
            if invulnerable { "players_nether_portal_creative_delay" } else { "players_nether_portal_default_delay" };
        self.int(rule).max(0) as u32
    }

    /// `ServerLevel.isAllowedToEnterPortal` for a trip from `from`: only
    /// entering the Nether is gated.
    pub fn portal_allows_trip_from(&self, from: crate::world::terrain::Dimension) -> bool {
        from != crate::world::terrain::Dimension::Overworld || self.bool("allow_entering_nether_using_portals")
    }

    /// Whether a death skips the death screen: `immediate_respawn`, except in
    /// Hardcore where the player becomes a spectator instead.
    pub fn respawns_immediately(&self, hardcore: bool) -> bool {
        !hardcore && self.bool("immediate_respawn")
    }

    pub fn damage_immunity(&self) -> DamageImmunity {
        DamageImmunity {
            fall: !self.bool("fall_damage"),
            fire: !self.bool("fire_damage"),
            drowning: !self.bool("drowning_damage"),
            freezing: !self.bool("freeze_damage"),
        }
    }
}

fn parse_bool(name: &str, text: &str) -> Result<bool, String> {
    match text {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("{name} needs true or false")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freeze_damage_is_enforced_and_switchable() {
        // Powder snow freezing (PLY-02c) hurts unless `freeze_damage` is off.
        let mut rules = GameRules::default();
        assert!(rules.bool("freeze_damage") && !rules.damage_immunity().freezing);
        rules.set("freezeDamage", "false").unwrap();
        assert!(rules.damage_immunity().freezing);
    }

    #[test]
    fn game_mode_names_and_abilities_match_java() {
        for mode in GameMode::ALL {
            assert_eq!(GameMode::from_name(mode.name()), Some(mode));
        }
        assert!(GameMode::Creative.invulnerable() && GameMode::Creative.can_fly());
        assert!(GameMode::Spectator.invulnerable() && !GameMode::Spectator.can_interact());
        assert!(GameMode::Adventure.is_survival() && !GameMode::Adventure.can_build());
    }

    /// Java 26.3 `GameRules.<clinit>` read from `server-26.3-classes.jar`:
    /// `(id, default, min, max)`; booleans have no bounds. The jar's
    /// `GameRuleRegistryFix` supplies the legacy names checked below.
    const JAVA_26_3: [(&str, &str, Option<i32>, Option<i32>); 59] = [
        ("advance_time", "true", None, None),
        ("advance_weather", "true", None, None),
        ("allow_entering_nether_using_portals", "true", None, None),
        ("block_drops", "true", None, None),
        ("block_explosion_drop_decay", "true", None, None),
        ("command_blocks_work", "true", None, None),
        ("command_block_output", "true", None, None),
        ("drowning_damage", "true", None, None),
        ("elytra_movement_check", "true", None, None),
        ("ender_pearls_vanish_on_death", "true", None, None),
        ("entity_drops", "true", None, None),
        ("fall_damage", "true", None, None),
        ("fire_damage", "true", None, None),
        ("fire_spread_radius_around_player", "128", Some(-1), Some(i32::MAX)),
        ("forgive_dead_players", "true", None, None),
        ("freeze_damage", "true", None, None),
        ("global_sound_events", "true", None, None),
        ("immediate_respawn", "false", None, None),
        ("keep_inventory", "false", None, None),
        ("lava_source_conversion", "false", None, None),
        ("limited_crafting", "false", None, None),
        ("locator_bar", "true", None, None),
        ("log_admin_commands", "true", None, None),
        ("max_block_modifications", "32768", Some(1), Some(i32::MAX)),
        ("max_command_forks", "65536", Some(0), Some(i32::MAX)),
        ("max_command_sequence_length", "65536", Some(0), Some(i32::MAX)),
        ("max_entity_cramming", "24", Some(0), Some(i32::MAX)),
        ("max_minecart_speed", "8", Some(1), Some(1000)),
        ("max_snow_accumulation_height", "1", Some(0), Some(8)),
        ("mob_drops", "true", None, None),
        ("mob_explosion_drop_decay", "true", None, None),
        ("mob_griefing", "true", None, None),
        ("natural_health_regeneration", "true", None, None),
        ("player_movement_check", "true", None, None),
        ("players_nether_portal_creative_delay", "0", Some(0), Some(i32::MAX)),
        ("players_nether_portal_default_delay", "80", Some(0), Some(i32::MAX)),
        ("players_sleeping_percentage", "100", Some(0), Some(i32::MAX)),
        ("projectiles_can_break_blocks", "true", None, None),
        ("pvp", "true", None, None),
        ("raids", "true", None, None),
        ("random_tick_speed", "3", Some(0), Some(i32::MAX)),
        ("reduced_debug_info", "false", None, None),
        ("respawn_radius", "10", Some(0), Some(i32::MAX)),
        ("send_command_feedback", "true", None, None),
        ("show_advancement_messages", "true", None, None),
        ("show_death_messages", "true", None, None),
        ("spawner_blocks_work", "true", None, None),
        ("spawn_mobs", "true", None, None),
        ("spawn_monsters", "true", None, None),
        ("spawn_patrols", "true", None, None),
        ("spawn_phantoms", "true", None, None),
        ("spawn_wandering_traders", "true", None, None),
        ("spawn_wardens", "true", None, None),
        ("spectators_generate_chunks", "true", None, None),
        ("spread_vines", "true", None, None),
        ("tnt_explodes", "true", None, None),
        ("tnt_explosion_drop_decay", "false", None, None),
        ("universal_anger", "false", None, None),
        ("water_source_conversion", "true", None, None),
    ];

    #[test]
    fn the_inventory_matches_every_java_26_3_rule_in_order() {
        assert_eq!(JAVA_RULES.len(), JAVA_26_3.len());
        for (rule, (id, default, min, max)) in JAVA_RULES.iter().zip(JAVA_26_3) {
            assert_eq!(rule.id, id);
            assert_eq!(rule.default.to_string(), default, "{id} default");
            assert_eq!(rule.range(), min.zip(max), "{id} range");
            assert_eq!(rule.is_int(), min.is_some(), "{id} type");
        }
        let mut ids: Vec<_> = JAVA_RULES.iter().map(|r| r.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), JAVA_RULES.len());
        assert_eq!(JAVA_RULES[FIRE_RADIUS].id, "fire_spread_radius_around_player");
    }

    #[test]
    fn legacy_names_are_the_ones_java_26_3_renames_from() {
        // `GameRuleRegistryFix.makeRule`: legacy field -> registry id. Rules
        // whose legacy and 26.x spellings differ only by id are spelled out;
        // `doFireTick` and `allowFireTicksAwayFromPlayer` are tested separately.
        let renamed = [
            ("doDaylightCycle", "advance_time"),
            ("doWeatherCycle", "advance_weather"),
            ("allowEnteringNetherUsingPortals", "allow_entering_nether_using_portals"),
            ("doTileDrops", "block_drops"),
            ("blockExplosionDropDecay", "block_explosion_drop_decay"),
            ("enableCommandBlocks", "command_blocks_work"),
            ("commandBlockOutput", "command_block_output"),
            ("drowningDamage", "drowning_damage"),
            ("disableElytraMovementCheck", "elytra_movement_check"),
            ("enderPearlsVanishOnDeath", "ender_pearls_vanish_on_death"),
            ("doEntityDrops", "entity_drops"),
            ("fallDamage", "fall_damage"),
            ("fireDamage", "fire_damage"),
            ("forgiveDeadPlayers", "forgive_dead_players"),
            ("freezeDamage", "freeze_damage"),
            ("globalSoundEvents", "global_sound_events"),
            ("doImmediateRespawn", "immediate_respawn"),
            ("keepInventory", "keep_inventory"),
            ("lavaSourceConversion", "lava_source_conversion"),
            ("doLimitedCrafting", "limited_crafting"),
            ("locatorBar", "locator_bar"),
            ("logAdminCommands", "log_admin_commands"),
            ("commandModificationBlockLimit", "max_block_modifications"),
            ("maxCommandForkCount", "max_command_forks"),
            ("maxCommandChainLength", "max_command_sequence_length"),
            ("maxEntityCramming", "max_entity_cramming"),
            ("minecartMaxSpeed", "max_minecart_speed"),
            ("snowAccumulationHeight", "max_snow_accumulation_height"),
            ("doMobLoot", "mob_drops"),
            ("mobExplosionDropDecay", "mob_explosion_drop_decay"),
            ("mobGriefing", "mob_griefing"),
            ("naturalRegeneration", "natural_health_regeneration"),
            ("disablePlayerMovementCheck", "player_movement_check"),
            ("playersNetherPortalCreativeDelay", "players_nether_portal_creative_delay"),
            ("playersNetherPortalDefaultDelay", "players_nether_portal_default_delay"),
            ("playersSleepingPercentage", "players_sleeping_percentage"),
            ("projectilesCanBreakBlocks", "projectiles_can_break_blocks"),
            ("pvp", "pvp"),
            ("disableRaids", "raids"),
            ("randomTickSpeed", "random_tick_speed"),
            ("reducedDebugInfo", "reduced_debug_info"),
            ("spawnRadius", "respawn_radius"),
            ("sendCommandFeedback", "send_command_feedback"),
            ("announceAdvancements", "show_advancement_messages"),
            ("showDeathMessages", "show_death_messages"),
            ("spawnerBlocksEnabled", "spawner_blocks_work"),
            ("doMobSpawning", "spawn_mobs"),
            ("spawnMonsters", "spawn_monsters"),
            ("doPatrolSpawning", "spawn_patrols"),
            ("doInsomnia", "spawn_phantoms"),
            ("doTraderSpawning", "spawn_wandering_traders"),
            ("doWardenSpawning", "spawn_wardens"),
            ("spectatorsGenerateChunks", "spectators_generate_chunks"),
            ("doVinesSpread", "spread_vines"),
            ("tntExplodes", "tnt_explodes"),
            ("tntExplosionDropDecay", "tnt_explosion_drop_decay"),
            ("universalAnger", "universal_anger"),
            ("waterSourceConversion", "water_source_conversion"),
        ];
        for (legacy, id) in renamed {
            let rule = JAVA_RULES.iter().find(|r| r.id == id).unwrap_or_else(|| panic!("{id}"));
            assert_eq!(rule.legacy, legacy, "{id}");
        }
        // Only the VoxelCraft-chosen name for the merged fire rule differs.
        assert_eq!(renamed.len(), JAVA_RULES.len() - 1);
        assert_eq!(JAVA_RULES[FIRE_RADIUS].legacy, "fireSpreadRadiusAroundPlayer");
    }

    #[test]
    fn implemented_rules_have_java_defaults_typed_values_and_every_spelling() {
        for rule in JAVA_RULES.iter().filter(|r| r.support.enforced()) {
            let rules = GameRules::default();
            for spelling in [rule.id.to_string(), format!("minecraft:{}", rule.id), rule.legacy.to_string()] {
                assert_eq!(rules.get(&spelling), Some(rule.default), "{spelling}");
            }
            let (good, bad) = match rule.range() {
                None => ("true", "1"),
                Some(_) => ("7", "seven"),
            };
            let mut rules = GameRules::default();
            if rule.range().is_none_or(|(min, max)| (min..=max).contains(&7)) {
                assert!(rules.set(rule.id, good).is_ok(), "{}", rule.id);
            }
            assert!(rules.set(rule.id, bad).is_err(), "{} rejects the wrong type", rule.id);
        }
        let mut rules = GameRules::default();
        assert!(rules.set("fire_spread_radius_around_player", "-1").is_ok());
        assert!(rules.set("fire_spread_radius_around_player", "-2").is_err());
        assert!(rules.set("random_tick_speed", "-1").is_err());
        assert!(rules.set("respawn_radius", "-1").is_err());
        assert!(rules.set("keep_inventory", "TRUE").is_err(), "Java parses booleans exactly");
    }

    #[test]
    fn gamerules_have_java_defaults_and_typed_round_trips() {
        let mut rules = GameRules::default();
        assert!(!rules.bool("keep_inventory"));
        assert!(rules.bool("advance_time"));
        assert_eq!(rules.int("random_tick_speed"), 3);
        assert_eq!(rules.int("players_sleeping_percentage"), 100);
        assert_eq!(rules.int("respawn_radius"), 10);
        rules.set("keep_inventory", "true").unwrap();
        rules.set("random_tick_speed", "12").unwrap();
        rules.set("respawn_radius", "32").unwrap();
        rules.set("tnt_explosion_drop_decay", "true").unwrap();
        assert_eq!(GameRules::deserialize(&rules.serialize()), rules);
        assert!(rules.set("keep_inventory", "1").is_err());
        assert!(rules.set("players_sleeping_percentage", "-1").is_err());
        assert!(rules.set("notARule", "true").is_err());
    }

    #[test]
    fn saves_use_legacy_names_and_load_old_new_and_namespaced_spellings() {
        let mut rules = GameRules::default();
        rules.set("keep_inventory", "true").unwrap();
        rules.set("advance_time", "false").unwrap();
        rules.set("respawn_radius", "0").unwrap();
        rules.set("spawn_monsters", "false").unwrap();
        // Defaults are omitted; VoxelCraft keeps writing the names its worlds
        // always used, in registry order.
        assert_eq!(rules.serialize(), "doDaylightCycle=false;keepInventory=true;spawnRadius=0;spawnMonsters=false");
        let loaded =
            GameRules::deserialize("doDaylightCycle=false;keepInventory=true;spawnRadius=0;spawnMonsters=false");
        assert_eq!(loaded, rules);
        assert_eq!(
            GameRules::deserialize(
                "minecraft:advance_time=false;minecraft:keep_inventory=true;respawn_radius=0;spawn_monsters=false"
            ),
            rules
        );
        // Worlds from before the later rules existed (17 rules plus spawnRadius).
        let old = "keepInventory=true;doDaylightCycle=false;doWeatherCycle=false;doMobSpawning=false;mobGriefing=false;\
                   doFireTick=false;naturalRegeneration=false;doImmediateRespawn=true;showDeathMessages=false;\
                   randomTickSpeed=9;doMobLoot=false;doTileDrops=false;fallDamage=false;fireDamage=false;\
                   drowningDamage=false;playersSleepingPercentage=50;doTraderSpawning=false;spawnRadius=3";
        let old = GameRules::deserialize(old);
        for (name, value) in [
            ("keep_inventory", true),
            ("advance_time", false),
            ("advance_weather", false),
            ("spawn_mobs", false),
            ("mob_griefing", false),
            ("natural_health_regeneration", false),
            ("immediate_respawn", true),
            ("show_death_messages", false),
            ("mob_drops", false),
            ("block_drops", false),
            ("fall_damage", false),
            ("fire_damage", false),
            ("drowning_damage", false),
            ("spawn_wandering_traders", false),
        ] {
            assert_eq!(old.bool(name), value, "{name}");
        }
        assert_eq!(
            (old.int("random_tick_speed"), old.int("players_sleeping_percentage"), old.int("respawn_radius")),
            (9, 50, 3)
        );
        assert_eq!(old.int("fire_spread_radius_around_player"), 0, "doFireTick=false");
        // The remaining rules keep their Java defaults.
        assert!(old.bool("spawner_blocks_work") && old.bool("tnt_explodes") && old.bool("water_source_conversion"));
    }

    #[test]
    fn pre_26_fire_booleans_become_the_spread_radius_like_the_datafixer() {
        // GameRuleRegistryFix: !doFireTick -> 0; doFireTick && allowFireTicksAwayFromPlayer -> -1; else 128.
        let radius = |text: &str| GameRules::deserialize(text).int("fire_spread_radius_around_player");
        assert_eq!(radius(""), 128);
        assert_eq!(radius("doFireTick=true"), 128);
        assert_eq!(radius("doFireTick=false"), 0);
        assert_eq!(radius("doFireTick=false;allowFireTicksAwayFromPlayer=true"), 0);
        assert_eq!(radius("allowFireTicksAwayFromPlayer=true;doFireTick=true"), -1);
        assert_eq!(radius("allowFireTicksAwayFromPlayer=true"), -1);
        assert_eq!(radius("allowFireTicksAwayFromPlayer=false"), 128);
        assert_eq!(radius("fireSpreadRadiusAroundPlayer=40"), 40);
        let mut rules = GameRules::default();
        rules.set("doFireTick", "false").unwrap();
        assert_eq!((rules.int("fire_spread_radius_around_player"), rules.bool("doFireTick")), (0, false));
        rules.set("doFireTick", "true").unwrap();
        assert_eq!((rules.int("fire_spread_radius_around_player"), rules.bool("doFireTick")), (128, true));
        rules.set("fire_spread_radius_around_player", "-1").unwrap();
        assert!(rules.bool("allowFireTicksAwayFromPlayer"));
        assert_eq!(rules.serialize(), "fireSpreadRadiusAroundPlayer=-1");
    }

    #[test]
    fn rules_the_engine_lacks_explain_why_instead_of_silently_storing_values() {
        let mut rules = GameRules::default();
        assert!(rules.get("raids").is_none() && rules.get("disableRaids").is_none());
        let error = rules.set("raids", "false").unwrap_err();
        assert!(error.contains("not implemented") && error.contains("v0.17"), "{error}");
        assert!(rules.set("elytra_movement_check", "false").unwrap_err().contains("no single-player effect"));
        assert!(GameRules::unknown("nope").contains("unknown gamerule"));
        assert_eq!(rules, GameRules::default());
        assert!(!GameRules::names().any(|name| name == "raids"));
        assert!(GameRules::names().any(|name| name == "keep_inventory"));
    }

    #[test]
    fn old_worlds_receive_every_default() {
        assert_eq!(GameRules::deserialize(""), GameRules::default());
        assert_eq!(GameRules::deserialize("futureRule=yes;randomTickSpeed=bad"), GameRules::default());
        assert_eq!(GameRules::deserialize("raids=false;disableRaids=true;keepInventory"), GameRules::default());
    }

    #[test]
    fn docs_inventory_lists_every_rule_with_its_support_level() {
        let docs = include_str!("../docs/game-rules.md");
        for rule in JAVA_RULES {
            let row = docs
                .lines()
                .find(|line| line.starts_with(&format!("| `{}` |", rule.id)))
                .unwrap_or_else(|| panic!("docs/game-rules.md has no row for {}", rule.id));
            let cells: Vec<_> = row.split('|').map(str::trim).collect();
            assert_eq!(cells[2], format!("`{}`", rule.legacy), "{} legacy name", rule.id);
            assert_eq!(cells[4], format!("`{}`", rule.default), "{} default", rule.id);
            assert_eq!(cells[5], rule.support.label(), "{} status", rule.id);
            let owner = match rule.support {
                Support::Missing(release) => release,
                Support::NotApplicable(_) => "n/a",
                _ => "-",
            };
            assert_eq!(cells[6], owner, "{} owner", rule.id);
        }
    }

    #[test]
    fn player_damage_rules_make_java_tags_harmless() {
        let mut rules = GameRules::default();
        let all = |rules: &GameRules, causes: &[&str]| {
            causes.iter().map(|c| rules.damage_immunity().blocks(c)).collect::<Vec<_>>()
        };
        let fall = ["fell from a high place", "hit the ground too hard"];
        let fire = ["burned to death", "tried to swim in lava", "was fireballed by a blaze"];
        // Defaults: nothing is blocked.
        assert_eq!(all(&rules, &[&fall[..], &fire[..], &["drowned"]].concat()), vec![false; 6]);
        rules.set("fall_damage", "false").unwrap();
        assert_eq!(all(&rules, &fall), [true, true]);
        assert_eq!(all(&rules, &fire), [false; 3]);
        rules.set("fire_damage", "false").unwrap();
        assert_eq!(all(&rules, &fire), [true; 3]);
        // The ghast fireball's blast is explosion damage, not `is_fire`.
        assert!(!rules.damage_immunity().blocks("was fireballed by a ghast"));
        assert_eq!(all(&rules, &["drowned"]), [false]);
        rules.set("drowning_damage", "false").unwrap();
        assert_eq!(all(&rules, &["drowned"]), [true]);
        // Other damage types are unaffected: starvation, mobs, arrows, explosions, magic.
        assert_eq!(
            all(
                &rules,
                &[
                    "starved to death",
                    "was slain by a zombie",
                    "was shot by a skeleton",
                    "was blown up by a creeper",
                    "was killed by magic"
                ]
            ),
            [false; 5]
        );
    }

    #[test]
    fn explosions_choose_block_interaction_and_decay_like_server_level_explode() {
        let rules = GameRules::default();
        let blast = |rules: &GameRules, cause: &str| rules.explosion(ExplosionSource::of_cause(cause));
        // Java defaults: mobs and blocks decay their drops, TNT does not.
        assert_eq!(
            blast(&rules, "was blown up by a creeper"),
            ExplosionRules { destroys_blocks: true, drop_decay: true }
        );
        assert_eq!(
            blast(&rules, "was fireballed by a ghast"),
            ExplosionRules { destroys_blocks: true, drop_decay: true }
        );
        assert_eq!(blast(&rules, "was blown up by TNT"), ExplosionRules { destroys_blocks: true, drop_decay: false });
        assert_eq!(
            blast(&rules, "was killed by [Intentional Game Design]"),
            ExplosionRules { destroys_blocks: true, drop_decay: true }
        );
        assert_eq!(
            blast(&rules, "was blown up by an End crystal"),
            ExplosionRules { destroys_blocks: true, drop_decay: true }
        );
        let mut rules = GameRules::default();
        rules.set("mob_griefing", "false").unwrap();
        assert!(!blast(&rules, "was blown up by a creeper").destroys_blocks);
        assert!(!blast(&rules, "was fireballed by a ghast").destroys_blocks);
        assert!(blast(&rules, "was blown up by TNT").destroys_blocks, "TNT ignores mob_griefing");
        assert!(blast(&rules, "was killed by [Intentional Game Design]").destroys_blocks);
        for (rule, cause) in [
            ("mob_explosion_drop_decay", "was blown up by a creeper"),
            ("block_explosion_drop_decay", "was killed by [Intentional Game Design]"),
        ] {
            rules.set(rule, "false").unwrap();
            assert!(!blast(&rules, cause).drop_decay, "{rule}");
        }
        rules.set("tnt_explosion_drop_decay", "true").unwrap();
        assert!(blast(&rules, "was blown up by TNT").drop_decay);
    }

    #[test]
    fn portal_delays_and_nether_entry_follow_the_rules() {
        use crate::world::terrain::Dimension;
        let mut rules = GameRules::default();
        // Defaults: 80 ticks (4 s) for survival players, none for creative.
        assert_eq!((rules.portal_delay_ticks(false), rules.portal_delay_ticks(true)), (80, 0));
        rules.set("players_nether_portal_default_delay", "20").unwrap();
        rules.set("players_nether_portal_creative_delay", "5").unwrap();
        assert_eq!((rules.portal_delay_ticks(false), rules.portal_delay_ticks(true)), (20, 5));
        assert!(
            rules.portal_allows_trip_from(Dimension::Overworld) && rules.portal_allows_trip_from(Dimension::Nether)
        );
        rules.set("allow_entering_nether_using_portals", "false").unwrap();
        assert!(!rules.portal_allows_trip_from(Dimension::Overworld));
        assert!(rules.portal_allows_trip_from(Dimension::Nether), "leaving the Nether is never blocked");
    }

    #[test]
    fn immediate_respawn_skips_the_death_screen_except_in_hardcore() {
        let mut rules = GameRules::default();
        assert!(!rules.respawns_immediately(false));
        rules.set("immediate_respawn", "true").unwrap();
        assert!(rules.respawns_immediately(false));
        assert!(!rules.respawns_immediately(true));
    }

    #[test]
    fn world_rules_follow_the_game_rules() {
        let mut rules = GameRules::default();
        assert_eq!(rules.world_rules(), crate::world::WorldRules::default());
        rules.set("tnt_explodes", "false").unwrap();
        rules.set("water_source_conversion", "false").unwrap();
        rules.set("lava_source_conversion", "true").unwrap();
        rules.set("fire_spread_radius_around_player", "16").unwrap();
        assert_eq!(
            rules.world_rules(),
            crate::world::WorldRules {
                tnt_explodes: false,
                water_source_conversion: false,
                lava_source_conversion: true,
                fire_spread_radius: 16,
                difficulty: 2,
            }
        );
    }
}
