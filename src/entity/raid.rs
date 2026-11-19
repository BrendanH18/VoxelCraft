//! v0.15 Track E: raids, pillager patrols and the village's part in them,
//! after Java 26.3 `Raid`, `Raids`, `Raider`, `PatrollingMonster`,
//! `PatrolSpawner`, `BadOmenMobEffect`, `RaidOmenMobEffect`,
//! `BellBlockEntity`, `GiveGiftToHero` and `Villager.updateSpecialPrices`.
//!
//! A dimension keeps its raids in [`Raids`] (saved as the dimension's
//! `raids` property with Java's `Raids` fields). Membership lives on the
//! raiders, as in Java: each raider's [`super::illager::Raider`] names its
//! raid and wave, and a raid counts its members from the mob list, so a
//! save holds the raid state once and each raider's membership with it.
//! The raid tick runs before mobs move; it refreshes every raider's unsaved
//! `raid_active`, `celebrating` and goal inputs, which `mob::raider` reads.
//! Every number is checked against `tests/fixtures/java/26.3/raids.json`.
use glam::{DVec3, IVec3};
use rustc_hash::FxHashSet;
use serde_json::{Value, json};

use super::equipment::Slot;
use super::{Ctx, Entities, EntityEvent, MobKind, MobSound, MobWorld, PlayerId};
use crate::inventory::Stack;
use crate::item::Item;
use crate::simulation::difficulty::Difficulty;
use crate::simulation::effects::Effect;

// ---------------------------------------------------------------- Raid

/// `Raid.DEFAULT_PRE_RAID_TICKS`: the countdown before each wave.
pub const PRE_RAID_TICKS: i32 = 300;
/// `RAID_TIMEOUT_TICKS`: a raid stops after this long active.
pub const RAID_TIMEOUT_TICKS: i64 = 48_000;
/// `NUM_SPAWN_ATTEMPTS`: failed wave placements before the raid stops.
pub const NUM_SPAWN_ATTEMPTS: i32 = 5;
/// `POST_RAID_TICK_LIMIT`: ticks after the last raider falls before victory.
pub const POST_RAID_TICK_LIMIT: i32 = 40;
/// `MAX_NO_ACTION_TIME`: idle raiders can't join, and drift out of the raid.
pub const MAX_NO_ACTION_TIME: i32 = 2400;
/// `MAX_CELEBRATION_TICKS`: how long a won or lost raid lingers.
pub const MAX_CELEBRATION_TICKS: i32 = 600;
/// `OUTSIDE_RAID_BOUNDS_TIMEOUT`: checks outside the village before a
/// raider leaves its raid.
pub const OUTSIDE_RAID_BOUNDS_TIMEOUT: i32 = 30;
/// `DEFAULT_MAX_RAID_OMEN_LEVEL`.
pub const MAX_RAID_OMEN_LEVEL: i32 = 5;
/// `LOW_MOB_THRESHOLD`: the bar names the raiders left at or below this.
pub const LOW_MOB_THRESHOLD: i32 = 2;
/// `HERO_OF_THE_VILLAGE_DURATION`.
pub const HERO_OF_THE_VILLAGE_TICKS: u32 = 48_000;
/// `VALID_RAID_RADIUS_SQR` (`ServerLevel.getRaidAt`).
pub const VALID_RAID_RADIUS_SQR: i64 = 9216;
/// `RAID_REMOVAL_THRESHOLD_SQR`: raiders this far from the centre leave.
pub const RAID_REMOVAL_THRESHOLD_SQR: i64 = 12_544;
/// `ALLOW_SPAWNING_WITHIN_VILLAGE_SECONDS_THRESHOLD`.
pub const VILLAGE_SPAWN_SECONDS: i32 = 7;
/// `SECTION_RADIUS_FOR_FINDING_NEW_VILLAGE_CENTER`.
pub const CENTER_SECTION_RADIUS: i32 = 2;
/// `Raid.updateRaiders`: a raider's `tickCount` before the idle checks
/// apply (a literal in the method).
pub const RAIDER_SETTLE_TICKS: u32 = 600;
/// `Raid.getNumGroups` for Peaceful, Easy, Normal and Hard.
pub const GROUPS: [i32; 4] = [0, 3, 5, 7];
/// `Raid.getEnchantOdds` by raid omen level (0 below 2).
pub const ENCHANT_ODDS: [f32; 6] = [0.0, 0.0, 0.1, 0.25, 0.5, 0.75];
/// `Raid.findRandomSpawnPos`: distance `0.22 * seconds - 0.24` rings of 32
/// blocks, `PI / 8` apart, jittered by `nextInt(3) * floor(distance)`.
pub const SPAWN_DISTANCE_SCALE: f32 = 0.22;
pub const SPAWN_DISTANCE_OFFSET: f32 = 0.24;
pub const SPAWN_RING_RADIUS: f32 = 32.0;
pub const SPAWN_ANGLE_DIVISOR: f32 = 8.0;
pub const SPAWN_JITTER_BOUND: u32 = 3;
pub const SPAWN_MAX_HEIGHT_DIFFERENCE: i32 = 96;
pub const SPAWN_LOADED_MARGIN: i32 = 10;
/// `Raid.tick`: tries per wave search before a wave is due, and when due.
pub const EARLY_SPAWN_TRIES: i32 = 8;
pub const DUE_SPAWN_TRIES: i32 = 20;
/// `Raid.setLeader` and `PatrollingMonster.finalizeSpawn`: a captain's
/// banner always drops.
pub const CAPTAIN_BANNER_DROP_CHANCE: f32 = 2.0;
/// `Raids.createOrExtendRaid`: the occupied village POIs averaged for the
/// raid centre.
pub const POI_RADIUS: i32 = 64;
/// `Raider.updateNoActionTime`.
pub const RAIDER_NO_ACTION_STEP: i32 = 2;
/// `RaiderMoveThroughVillageGoal(1.05, 1)` and its 48-block home search.
pub const VILLAGE_WALK_SPEED: f64 = 1.05;
pub const VILLAGE_HOME_RADIUS: i32 = 48;
/// `HoldGroundAttackGoal(10.0)` for pillagers and vindicators.
pub const HOLD_GROUND_RADIUS: f64 = 10.0;
/// `ObtainRaidLeaderBannerGoal`: picked up within this distance, walked to at 1.15.
pub const BANNER_PICKUP_DISTANCE: f64 = 1.414;
pub const BANNER_WALK_SPEED: f64 = 1.15;

/// `Raid.RaiderType`, in Java's order (the order waves spawn in).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RaiderType {
    Vindicator,
    Evoker,
    Pillager,
    Witch,
    Ravager,
}

impl RaiderType {
    pub const ALL: [RaiderType; 5] =
        [RaiderType::Vindicator, RaiderType::Evoker, RaiderType::Pillager, RaiderType::Witch, RaiderType::Ravager];

    pub fn kind(self) -> MobKind {
        match self {
            RaiderType::Vindicator => MobKind::Vindicator,
            RaiderType::Evoker => MobKind::Evoker,
            RaiderType::Pillager => MobKind::Pillager,
            RaiderType::Witch => MobKind::Witch,
            RaiderType::Ravager => MobKind::Ravager,
        }
    }

    /// `spawnsPerWaveBeforeBonus`, indexed by wave (0 unused).
    pub fn spawns(self) -> [i32; 8] {
        match self {
            RaiderType::Vindicator => [0, 0, 2, 0, 1, 4, 2, 5],
            RaiderType::Evoker => [0, 0, 0, 0, 0, 1, 1, 2],
            RaiderType::Pillager => [0, 4, 3, 3, 4, 4, 4, 2],
            RaiderType::Witch => [0, 0, 0, 0, 3, 0, 0, 1],
            RaiderType::Ravager => [0, 0, 0, 1, 0, 1, 0, 2],
        }
    }
}

/// `Raid.getNumGroups`.
pub fn num_groups(difficulty: Difficulty) -> i32 {
    GROUPS[difficulty as usize]
}

/// `Raid.getEnchantOdds`.
pub fn enchant_odds(omen_level: i32) -> f32 {
    ENCHANT_ODDS.get(omen_level.clamp(0, 5) as usize).copied().unwrap_or(0.0)
}

/// `Raid.getPotentialBonusSpawns`: the extra raiders a wave may roll on top
/// of the table, `nextInt(bonus + 1)` when the type's bonus is positive.
pub fn bonus_spawns(
    kind: RaiderType,
    wave: i32,
    difficulty: Difficulty,
    bonus_wave: bool,
    rng: &mut super::Rng,
) -> i32 {
    let easy = difficulty == Difficulty::Easy;
    let normal = difficulty == Difficulty::Normal;
    let bonus = match kind {
        RaiderType::Witch => {
            if easy || wave <= 2 || wave == 4 {
                return 0;
            }
            1
        }
        RaiderType::Vindicator | RaiderType::Pillager => {
            if easy {
                rng.next_int(2) as i32
            } else if normal {
                1
            } else {
                2
            }
        }
        RaiderType::Ravager => i32::from(!easy && bonus_wave),
        RaiderType::Evoker => return 0,
    };
    if bonus > 0 { rng.next_int(bonus as u32 + 1) as i32 } else { 0 }
}

/// `Raid.RaidStatus`, saved by its serialized name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Ongoing,
    Victory,
    Loss,
    Stopped,
}

impl Status {
    pub const ALL: [Status; 4] = [Status::Ongoing, Status::Victory, Status::Loss, Status::Stopped];

    pub fn name(self) -> &'static str {
        match self {
            Status::Ongoing => "ongoing",
            Status::Victory => "victory",
            Status::Loss => "loss",
            Status::Stopped => "stopped",
        }
    }

    pub fn from_name(name: &str) -> Option<Status> {
        Status::ALL.into_iter().find(|s| s.name() == name)
    }
}

/// What the raid bar says (`event.minecraft.raid` and its variants).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BarTitle {
    #[default]
    Raid,
    /// `Raid - Raiders Remaining: n`.
    Remaining(i32),
    /// `event.minecraft.raid.victory.full`.
    Victory,
    /// `event.minecraft.raid.defeat.full`.
    Defeat,
}

impl BarTitle {
    pub fn text(self) -> String {
        match self {
            BarTitle::Raid => "Raid".into(),
            BarTitle::Remaining(n) => format!("Raid - Raiders Remaining: {n}"),
            BarTitle::Victory => "Raid - Victory".into(),
            BarTitle::Defeat => "Raid - Defeat".into(),
        }
    }

    /// A compact code for LAN snapshots: 0 raid, 1 victory, 2 defeat,
    /// `10 + n` for raiders remaining.
    pub fn code(self) -> i32 {
        match self {
            BarTitle::Raid => 0,
            BarTitle::Victory => 1,
            BarTitle::Defeat => 2,
            BarTitle::Remaining(n) => 10 + n.max(0),
        }
    }

    pub fn from_code(code: i64) -> BarTitle {
        match code {
            1 => BarTitle::Victory,
            2 => BarTitle::Defeat,
            n if n >= 10 => BarTitle::Remaining((n - 10).min(1000) as i32),
            _ => BarTitle::Raid,
        }
    }
}

/// The raid's boss bar (`ServerBossEvent`, red, `NOTCHED_10`), as a player
/// in the raid sees it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bar {
    pub progress: f32,
    pub title: BarTitle,
}

/// One raid (`Raid`). Fields after `heroes` are not saved, as in Java.
#[derive(Clone, Debug, PartialEq)]
pub struct Raid {
    pub id: u32,
    pub started: bool,
    pub active: bool,
    pub ticks_active: i64,
    pub omen_level: i32,
    pub groups_spawned: i32,
    pub cooldown: i32,
    pub post_raid_ticks: i32,
    pub total_health: f32,
    pub num_groups: i32,
    pub status: Status,
    pub center: IVec3,
    /// `heroesOfTheVillage`: players who killed one of its raiders.
    pub heroes: Vec<PlayerId>,
    pub celebration_ticks: i32,
    pub wave_spawn_pos: Option<IVec3>,
    pub bar: Bar,
    /// Whether the bar shows (`setVisible`).
    pub bar_visible: bool,
    /// The players the bar is shown to (`raidEvent.getPlayers`).
    pub bar_players: Vec<PlayerId>,
}

impl Raid {
    /// `new Raid(center, difficulty)`.
    pub fn new(id: u32, center: IVec3, difficulty: Difficulty) -> Raid {
        Raid {
            id,
            started: false,
            active: true,
            ticks_active: 0,
            omen_level: 0,
            groups_spawned: 0,
            cooldown: PRE_RAID_TICKS,
            post_raid_ticks: 0,
            total_health: 0.0,
            num_groups: num_groups(difficulty),
            status: Status::Ongoing,
            center,
            heroes: Vec::new(),
            celebration_ticks: 0,
            wave_spawn_pos: None,
            bar: Bar::default(),
            bar_visible: true,
            bar_players: Vec::new(),
        }
    }

    pub fn is_over(&self) -> bool {
        matches!(self.status, Status::Victory | Status::Loss)
    }
    pub fn is_stopped(&self) -> bool {
        self.status == Status::Stopped
    }
    pub fn is_victory(&self) -> bool {
        self.status == Status::Victory
    }
    pub fn is_loss(&self) -> bool {
        self.status == Status::Loss
    }
    pub fn first_wave_spawned(&self) -> bool {
        self.groups_spawned > 0
    }
    fn has_bonus_wave(&self) -> bool {
        self.omen_level > 1
    }
    fn spawned_bonus_wave(&self) -> bool {
        self.groups_spawned > self.num_groups
    }
    fn final_wave(&self) -> bool {
        self.groups_spawned == self.num_groups
    }
    /// `hasMoreWaves`.
    pub fn has_more_waves(&self) -> bool {
        if self.has_bonus_wave() { !self.spawned_bonus_wave() } else { !self.final_wave() }
    }
    /// `shouldSpawnBonusGroup` given the raiders alive.
    fn bonus_due(&self, alive: i32) -> bool {
        self.final_wave() && alive == 0 && self.has_bonus_wave()
    }
    /// `shouldSpawnGroup` given the raiders alive.
    fn group_due(&self, alive: i32) -> bool {
        self.cooldown == 0 && (self.groups_spawned < self.num_groups || self.bonus_due(alive)) && alive == 0
    }
    /// `isBetweenWaves` given the raiders alive.
    pub fn between_waves(&self, alive: i32) -> bool {
        self.first_wave_spawned() && alive == 0 && self.cooldown > 0
    }
    /// `absorbRaidOmen` for a Raid Omen of this amplifier. Returns whether
    /// this is the trigger (`Stats.RAID_TRIGGER` before the first wave).
    pub fn absorb_omen(&mut self, amplifier: u8) -> bool {
        self.omen_level = (self.omen_level + i32::from(amplifier) + 1).clamp(0, MAX_RAID_OMEN_LEVEL);
        !self.first_wave_spawned()
    }
    /// The default spawns of `kind` in `wave` (`getDefaultNumSpawns`).
    pub fn default_spawns(&self, kind: RaiderType, wave: i32, bonus_wave: bool) -> i32 {
        let w = if bonus_wave { self.num_groups } else { wave };
        kind.spawns().get(w.clamp(0, 7) as usize).copied().unwrap_or(0)
    }

    /// Java's `MAP_CODEC` fields, snake case, with the id (`RaidWithId`).
    pub fn save(&self) -> Value {
        json!({
            "id": self.id,
            "started": self.started,
            "active": self.active,
            "ticks_active": self.ticks_active,
            "raid_omen_level": self.omen_level,
            "groups_spawned": self.groups_spawned,
            "cooldown_ticks": self.cooldown,
            "post_raid_ticks": self.post_raid_ticks,
            "total_health": self.total_health,
            "group_count": self.num_groups,
            "status": self.status.name(),
            "center": self.center.to_array(),
            "heroes_of_the_village": self.heroes.iter().map(|p| p.0).collect::<Vec<_>>(),
        })
    }

    /// Reads [`Raid::save`]; `None` for a malformed record.
    pub fn load(v: &Value) -> Option<Raid> {
        let int = |k: &str| v[k].as_i64();
        let center = v["center"].as_array().and_then(|a| {
            let c = |i: usize| a.get(i).and_then(Value::as_i64).and_then(|n| i32::try_from(n).ok());
            Some(IVec3::new(c(0)?, c(1)?, c(2)?))
        })?;
        let small = |k: &str, max: i64| int(k).map(|n| n.clamp(0, max) as i32);
        Some(Raid {
            id: u32::try_from(int("id")?).ok()?,
            started: v["started"] == true,
            active: v["active"] == true,
            ticks_active: int("ticks_active")?.clamp(0, RAID_TIMEOUT_TICKS),
            omen_level: small("raid_omen_level", i64::from(MAX_RAID_OMEN_LEVEL))?,
            groups_spawned: small("groups_spawned", 100)?,
            cooldown: small("cooldown_ticks", i64::from(PRE_RAID_TICKS))?,
            post_raid_ticks: small("post_raid_ticks", i64::from(POST_RAID_TICK_LIMIT))?,
            total_health: v["total_health"].as_f64().map(|h| h.clamp(0.0, 1.0e6) as f32)?,
            num_groups: small("group_count", 7)?,
            status: Status::from_name(v["status"].as_str()?)?,
            center,
            heroes: v["heroes_of_the_village"]
                .as_array()
                .map(|a| a.iter().filter_map(|p| p.as_u64()).map(|p| PlayerId(p as u32)).collect())
                .unwrap_or_default(),
            celebration_ticks: 0,
            wave_spawn_pos: None,
            bar: Bar::default(),
            bar_visible: true,
            bar_players: Vec::new(),
        })
    }
}

// ---------------------------------------------------------------- omens

/// `BadOmenMobEffect.applyEffectTick`: the Raid Omen it becomes.
pub const RAID_OMEN_TICKS: u32 = 600;
/// `RaidOmenMobEffect.shouldApplyEffectTickThisTick`: the raid starts when
/// one tick is left.
pub const RAID_OMEN_FIRES_AT: u32 = 1;

// ---------------------------------------------------------------- patrols

/// `PatrolSpawner.tick`: every 12000 + `nextInt(1200)` ticks, one try in 5.
pub const PATROL_INTERVAL: i32 = 12_000;
pub const PATROL_INTERVAL_SPREAD: u32 = 1200;
pub const PATROL_CHANCE: u32 = 5;
/// No patrol near a village (`isCloseToVillage(pos, 2)`).
pub const PATROL_VILLAGE_SECTIONS: i32 = 2;
/// The leader spawns 24 + `nextInt(24)` blocks away on each axis.
pub const PATROL_MIN_OFFSET: i32 = 24;
pub const PATROL_OFFSET_SPREAD: u32 = 24;
pub const PATROL_LOADED_MARGIN: i32 = 10;
/// Members step `nextInt(5) - nextInt(5)` from the last.
pub const PATROL_MEMBER_STEP: u32 = 5;
/// The `early_game` timeline: no patrols before this overworld clock tick.
pub const PATROL_START_TICKS: i64 = 120_000;
/// `PatrollingMonster.finalizeSpawn`: a naturally spawned raider leads.
pub const CAPTAIN_CHANCE: f32 = 0.06;
/// `checkPatrollingMonsterSpawnRules`: block light above this forbids.
pub const PATROL_MAX_BLOCK_LIGHT: u8 = 8;
/// `PatrollingMonster.removeWhenFarAway` for patrolling mobs.
pub const PATROL_DESPAWN_SQR: f64 = 16_384.0;
/// `findPatrolTarget`: -500 + `nextInt(1000)` on each axis.
pub const PATROL_TARGET_OFFSET: i32 = -500;
pub const PATROL_TARGET_SPREAD: u32 = 1000;
/// `LongDistancePatrolGoal(0.7, 0.595)`.
pub const PATROL_SPEED: f64 = 0.7;
pub const PATROL_LEADER_SPEED: f64 = 0.595;
pub const PATROL_ARRIVE: f64 = 10.0;
pub const PATROL_TURN_DEGREES: f32 = 90.0;
pub const PATROL_TURN_SCALE: f64 = 0.4;
pub const PATROL_STEP: f64 = 10.0;
pub const PATROL_COMPANION_RADIUS: f64 = 16.0;
pub const PATROL_WANDER_OFFSET: i32 = -8;
pub const PATROL_WANDER_SPREAD: u32 = 16;
pub const PATROL_FAILED_COOLDOWN: u32 = 200;

// ---------------------------------------------------------------- bells

/// `BellBlockEntity`: resonance starts 5 ticks into a ring when raiders are
/// within 32 blocks and ends 40 ticks later, giving raiders within 48
/// blocks Glowing for 60 ticks.
pub const BELL_TICKS_BEFORE_RESONATION: u32 = 5;
pub const BELL_MAX_RESONATION_TICKS: u32 = 40;
pub const BELL_GLOW_TICKS: u32 = 60;
pub const BELL_HEAR_RADIUS: f64 = 32.0;
pub const BELL_HIGHLIGHT_RADIUS: f64 = 48.0;
pub const BELL_SEARCH_RADIUS: f64 = 48.0;
pub const BELL_DURATION: u32 = 50;

// ---------------------------------------------------------------- heroes

/// `Villager.updateSpecialPrices`: Hero of the Village takes
/// `floor((0.3 + 0.0625 * amplifier) * base count)`, at least 1, off each offer.
pub const HERO_BASE_DISCOUNT: f32 = 0.3;
pub const HERO_DISCOUNT_PER_LEVEL: f32 = 0.0625;

/// The Hero of the Village part of the villager price calculation
/// ([`super::villager::special_price_diff`]): how many items Hero of the
/// Village at `amplifier` takes off an offer whose base first cost is
/// `base_count` (0 without the effect).
pub fn hero_price_cut(base_count: u8, amplifier: Option<u8>) -> i32 {
    let Some(amp) = amplifier else { return 0 };
    let modifier = f64::from(HERO_BASE_DISCOUNT + HERO_DISCOUNT_PER_LEVEL * f32::from(amp));
    ((modifier * f64::from(base_count)).floor() as i32).max(1)
}

/// `GiveGiftToHero`: 600 ticks before the first gift, then 600 +
/// `nextInt(6001)`; thrown within 5 blocks, 20 ticks after turning; the
/// behaviour times out after 100 ticks; walks at 0.5.
pub const GIFT_FIRST_WAIT: i32 = 600;
pub const GIFT_MIN_WAIT: i32 = 600;
pub const GIFT_MAX_WAIT: i32 = 6600;
pub const GIFT_THROW_DISTANCE: f64 = 5.0;
pub const GIFT_TURN_DELAY: i64 = 20;
pub const GIFT_TIMEOUT: i64 = 100;
pub const GIFT_WALK_SPEED: f64 = 0.5;
/// `PlayerSensor`'s 16-block range (`Sensor.TARGET_CONDITIONS`).
pub const GIFT_SIGHT: f64 = 16.0;

/// One weighted entry of a gift table: Java item name, weight, count range
/// and the tipped arrow's potion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GiftEntry {
    pub item: &'static str,
    pub weight: u32,
    pub count: (u8, u8),
    pub potion: Option<&'static str>,
}

const fn one(item: &'static str) -> GiftEntry {
    GiftEntry { item, weight: 1, count: (1, 1), potion: None }
}
const fn tipped(potion: &'static str) -> GiftEntry {
    GiftEntry { item: "tipped_arrow", weight: 1, count: (0, 1), potion: Some(potion) }
}

/// The `gameplay/hero_of_the_village/*_gift` loot tables (one roll each).
/// A gift table as a constant slice.
macro_rules! table {
    ($($e:expr),* $(,)?) => {{
        const T: &[GiftEntry] = &[$($e),*];
        T
    }};
}

pub fn gift_table(name: &str) -> &'static [GiftEntry] {
    match name {
        "armorer" => {
            table![
                one("chainmail_helmet"),
                one("chainmail_chestplate"),
                one("chainmail_leggings"),
                one("chainmail_boots")
            ]
        }
        "baby" => table![one("poppy")],
        "butcher" => table![
            one("cooked_rabbit"),
            one("cooked_chicken"),
            one("cooked_porkchop"),
            one("cooked_beef"),
            one("cooked_mutton"),
        ],
        "cartographer" => table![one("map"), one("paper")],
        "cleric" => table![one("redstone"), one("lapis_lazuli")],
        "farmer" => table![one("bread"), one("pumpkin_pie"), one("cookie")],
        "fisherman" => table![one("cod"), one("salmon")],
        "fletcher" => table![
            GiftEntry { item: "arrow", weight: 26, count: (1, 1), potion: None },
            tipped("swiftness"),
            tipped("slowness"),
            tipped("strength"),
            tipped("healing"),
            tipped("harming"),
            tipped("leaping"),
            tipped("regeneration"),
            tipped("fire_resistance"),
            tipped("water_breathing"),
            tipped("invisibility"),
            tipped("night_vision"),
            tipped("weakness"),
            tipped("poison"),
        ],
        "leatherworker" => table![one("leather")],
        "librarian" => table![one("book")],
        "mason" => table![one("clay")],
        "shepherd" => table![
            one("white_wool"),
            one("orange_wool"),
            one("magenta_wool"),
            one("light_blue_wool"),
            one("yellow_wool"),
            one("lime_wool"),
            one("pink_wool"),
            one("gray_wool"),
            one("light_gray_wool"),
            one("cyan_wool"),
            one("purple_wool"),
            one("blue_wool"),
            one("brown_wool"),
            one("green_wool"),
            one("red_wool"),
            one("black_wool"),
        ],
        "toolsmith" => table![one("stone_pickaxe"), one("stone_axe"), one("stone_hoe"), one("stone_shovel")],
        "weaponsmith" => table![one("stone_axe"), one("golden_axe"), one("iron_axe")],
        "unemployed" => table![one("wheat_seeds")],
        _ => &[],
    }
}

/// `GiveGiftToHero.getLootTableToThrow`: babies give `baby_gift`; the
/// professions their own table; anyone else `unemployed_gift`.
pub fn gift_table_name(profession: super::villager::Profession, baby: bool) -> &'static str {
    use super::villager::Profession as P;
    if baby {
        return "baby";
    }
    match profession {
        P::Armorer => "armorer",
        P::Butcher => "butcher",
        P::Cartographer => "cartographer",
        P::Cleric => "cleric",
        P::Farmer => "farmer",
        P::Fisherman => "fisherman",
        P::Fletcher => "fletcher",
        P::Leatherworker => "leatherworker",
        P::Librarian => "librarian",
        P::Mason => "mason",
        P::Shepherd => "shepherd",
        P::Toolsmith => "toolsmith",
        P::Weaponsmith => "weaponsmith",
        P::None | P::Nitwit => "unemployed",
    }
}

/// One roll of a gift table: the weighted pick, then its count.
pub fn roll_gift(table: &[GiftEntry], rng: &mut super::Rng) -> Option<Stack> {
    let total: u32 = table.iter().map(|e| e.weight).sum();
    if total == 0 {
        return None;
    }
    let mut pick = rng.next_int(total);
    let entry = table.iter().find(|e| {
        if pick < e.weight {
            true
        } else {
            pick -= e.weight;
            false
        }
    })?;
    let count = entry.count.0 + rng.next_int(u32::from(entry.count.1 - entry.count.0) + 1) as u8;
    if count == 0 {
        return None;
    }
    if let Some(potion) = entry.potion {
        return crate::potion::Potion::from_id(potion).map(|p| Stack::tipped_arrow(p, count));
    }
    crate::java_registry::item(entry.item, "").map(|item| Stack::new(item, count))
}

// ---------------------------------------------------------------- villages

/// Sections holding an occupied village POI (`PoiTypeTags.VILLAGE` with
/// `Occupancy.IS_OCCUPIED`): a claimed bed or job site, or a bell near one.
#[derive(Clone, Debug, Default)]
pub struct VillageSections {
    sections: FxHashSet<IVec3>,
    /// Claimed and unclaimed beds (`PoiTypes.HOME`), for raiders walking
    /// through the village and villagers hiding.
    pub homes: Vec<IVec3>,
    built: Option<i64>,
}

impl VillageSections {
    fn section(p: IVec3) -> IVec3 {
        p.div_euclid(IVec3::splat(16))
    }

    /// `PoiManager.sectionsToVillage`: the Chebyshev section distance to the
    /// nearest village section, at most 7.
    pub fn sections_to_village(&self, pos: IVec3) -> i32 {
        let s = Self::section(pos);
        if self.sections.is_empty() {
            return 7;
        }
        for r in 0..=6i32 {
            for dx in -r..=r {
                for dy in -r..=r {
                    for dz in -r..=r {
                        if dx.abs().max(dy.abs()).max(dz.abs()) == r
                            && self.sections.contains(&(s + IVec3::new(dx, dy, dz)))
                        {
                            return r;
                        }
                    }
                }
            }
        }
        7
    }

    /// `ServerLevel.isCloseToVillage`.
    pub fn close_to_village(&self, pos: IVec3, sections: i32) -> bool {
        sections <= 6 && self.sections_to_village(pos) <= sections
    }

    /// `ServerLevel.isVillage`: within one section of a village section.
    pub fn is_village(&self, pos: IVec3) -> bool {
        self.close_to_village(pos, 1)
    }

    /// Marks `p`'s section as holding an occupied village POI (tests and
    /// the rebuild).
    pub fn insert(&mut self, p: IVec3) {
        self.sections.insert(Self::section(p));
    }
}

// ---------------------------------------------------------------- manager

/// Raid sounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    /// `RAID_HORN`, 13 blocks from the player toward the wave.
    Horn,
    /// `BELL_RESONATE`.
    BellResonate,
    /// `VILLAGER_CELEBRATE` of a villager whose village won.
    VillagerCelebrate,
}

impl Cue {
    pub const ALL: [Cue; 3] = [Cue::Horn, Cue::BellResonate, Cue::VillagerCelebrate];

    /// Java's sound event name.
    pub fn key(self) -> &'static str {
        match self {
            Cue::Horn => "event.raid.horn",
            Cue::BellResonate => "block.bell.resonate",
            Cue::VillagerCelebrate => "entity.villager.celebrate",
        }
    }
}

/// What the game applies to players after a raid tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    /// `Raid.tick` on victory: Hero of the Village (amplifier `level - 1`),
    /// `Stats.RAID_WIN` and the advancement for this player.
    Hero { player: PlayerId, amplifier: u8 },
    /// `absorbRaidOmen` before the first wave: `Stats.RAID_TRIGGER`.
    Triggered { player: PlayerId },
    /// The raid horn for this player at this position.
    Horn { player: PlayerId, at: DVec3 },
}

/// A ringing bell's resonance (`BellBlockEntity`): the raiders it may light.
#[derive(Clone, Debug, PartialEq)]
struct Resonance {
    pos: IVec3,
    ticks: u32,
    resonating: Option<u32>,
    nearby: Vec<u32>,
}

/// A villager's `GiveGiftToHero` run.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Gift {
    wait: i32,
    started: Option<i64>,
}

/// A dimension's raids (`Raids`) with the patrol spawner, the village
/// section cache and the bell resonances.
#[derive(Clone, Debug)]
pub struct Raids {
    pub raids: Vec<Raid>,
    pub next_id: u32,
    pub tick: i64,
    /// The `raids` game rule (Java's `disableRaids` inverted).
    pub enabled: bool,
    /// The `spawn_patrols` game rule.
    pub patrols: bool,
    /// `PatrolSpawner.nextTick` (not saved, as in Java).
    pub patrol_next: i32,
    carry: f64,
    pub villages: VillageSections,
    bells: Vec<Resonance>,
    gifts: rustc_hash::FxHashMap<u64, Gift>,
    /// Players with Hero of the Village and its amplifier, set by the game
    /// each tick; villagers read it for prices and gifts.
    pub heroes: Vec<(PlayerId, u8)>,
    pub outcomes: Vec<Outcome>,
    /// Each raid's id, centre and activity before this tick's step.
    spots: Vec<(u32, IVec3, bool)>,
}

impl Default for Raids {
    fn default() -> Self {
        Raids {
            raids: Vec::new(),
            next_id: 1,
            tick: 0,
            enabled: true,
            patrols: true,
            patrol_next: 0,
            carry: 0.0,
            villages: VillageSections::default(),
            bells: Vec::new(),
            gifts: Default::default(),
            heroes: Vec::new(),
            outcomes: Vec::new(),
            spots: Vec::new(),
        }
    }
}

impl Raids {
    pub fn get(&self, id: u32) -> Option<&Raid> {
        self.raids.iter().find(|r| r.id == id)
    }

    /// `Raids.getNearbyRaid(pos, 9216)` (`ServerLevel.getRaidAt`): the
    /// nearest active raid whose centre is within 96 blocks.
    pub fn raid_at(&self, pos: IVec3) -> Option<&Raid> {
        let mut best: Option<(&Raid, i64)> = None;
        for r in &self.raids {
            let d = dist_sqr(r.center, pos);
            if r.active && d < best.map_or(VALID_RAID_RADIUS_SQR, |b| b.1) {
                best = Some((r, d));
            }
        }
        best.map(|b| b.0)
    }

    /// The bar `player` at `pos` sees: the raid it is shown by.
    pub fn bar_for(&self, player: PlayerId) -> Option<Bar> {
        self.raids.iter().find(|r| r.bar_visible && r.bar_players.contains(&player)).map(|r| r.bar)
    }

    /// The `raids` property: Java's `Raids` codec (`raids`, `next_id`, `tick`).
    pub fn save(&self) -> String {
        json!({
            "raids": self.raids.iter().map(Raid::save).collect::<Vec<_>>(),
            "next_id": self.next_id,
            "tick": self.tick,
        })
        .to_string()
    }

    pub fn load(&mut self, text: &str) {
        let Ok(v) = serde_json::from_str::<Value>(text) else { return };
        self.raids = v["raids"].as_array().map(|a| a.iter().filter_map(Raid::load).collect()).unwrap_or_default();
        let top = self.raids.iter().map(|r| r.id).max().unwrap_or(0);
        self.next_id = v["next_id"].as_u64().map_or(1, |n| n.min(u64::from(u32::MAX - 1)) as u32).max(top);
        self.tick = v["tick"].as_i64().unwrap_or(0).max(0);
    }
}

/// `Vec3i.distSqr`.
fn dist_sqr(a: IVec3, b: IVec3) -> i64 {
    let d = (a - b).as_i64vec3();
    d.x * d.x + d.y * d.y + d.z * d.z
}

/// The section centre (`SectionPos.center`).
fn section_center(s: IVec3) -> IVec3 {
    s * 16 + IVec3::splat(8)
}

/// A naturally spawned raider's captaincy (`PatrollingMonster.finalizeSpawn`
/// for reasons other than patrols, raids and structures): a 6% roll for
/// those that can lead, then the banner on its head.
pub fn natural_captain(m: &mut super::Mob, rng: &mut super::Rng) {
    if can_lead(m.kind) && rng.next_f32() < CAPTAIN_CHANCE {
        make_captain(m);
    }
}

/// `PatrollingMonster.canBeLeader`: not ravagers or witches.
pub fn can_lead(kind: MobKind) -> bool {
    super::illager::is_raider(kind) && !matches!(kind, MobKind::Ravager | MobKind::Witch)
}

/// `setPatrolLeader(true)` and the ominous banner on the head, which
/// always drops.
pub fn make_captain(m: &mut super::Mob) {
    if let Some(s) = &mut m.illager {
        s.raider.patrol_leader = true;
        s.raider.patrolling = true;
    }
    m.equipment.set(Slot::Head, Some(crate::content::banner_patterns::ominous_banner()));
    m.equipment.set_drop_chance(Slot::Head, CAPTAIN_BANNER_DROP_CHANCE);
}

/// `Raider.isCaptain`: the ominous banner on the head of a patrol leader.
pub fn is_captain(m: &super::Mob) -> bool {
    m.illager.as_ref().is_some_and(|s| s.raider.patrol_leader)
        && m.equipment.get(Slot::Head).is_some_and(crate::content::banner_patterns::is_ominous)
}

/// The pillager's captain pool (`entities/pillager`): an ominous bottle of
/// amplifier `uniform(0, 4)`. The only 26.3 entity table that names
/// captains.
pub fn captain_loot(kind: MobKind, rng: &mut super::Rng) -> Option<Stack> {
    (kind == MobKind::Pillager).then(|| Stack::new(crate::world::trial::ominous_bottle(rng.next_int(5) as u8), 1))
}

/// `Vindicator.applyRaidBuffs` and `Pillager.applyRaidBuffs`: the weapon a
/// wave's raider gets, enchanted by the `raid/*` providers when
/// `nextFloat() <= getEnchantOdds()`.
pub fn apply_raid_buffs(m: &mut super::Mob, wave: i32, omen_level: i32, rng: &mut super::Rng) {
    use crate::enchant::Enchantment;
    use crate::item::{Tier, ToolKind};
    let normal = num_groups(Difficulty::Normal);
    let easy = num_groups(Difficulty::Easy);
    match m.kind {
        MobKind::Vindicator => {
            let mut axe = Stack::new(Item::tool(ToolKind::Axe, Tier::Iron), 1);
            if rng.next_f32() <= enchant_odds(omen_level) {
                axe.enchants.set(Enchantment::Sharpness, if wave > normal { 2 } else { 1 });
            }
            m.equipment.set(Slot::MainHand, Some(axe));
        }
        MobKind::Pillager if rng.next_f32() <= enchant_odds(omen_level) => {
            let level = if wave > normal {
                2
            } else if wave > easy {
                1
            } else {
                return;
            };
            let mut crossbow = Stack::new(Item::CROSSBOW, 1);
            crossbow.enchants.set(Enchantment::QuickCharge, level);
            m.equipment.set(Slot::MainHand, Some(crossbow));
        }
        _ => {}
    }
}

impl Entities {
    /// The raid at `pos` (`ServerLevel.getRaidAt`).
    pub fn raid_at(&self, pos: IVec3) -> Option<&Raid> {
        self.raids.raid_at(pos)
    }

    /// `ServerLevel.isVillage` from the section cache (rebuilt every second).
    pub fn is_village<W: MobWorld + ?Sized>(&mut self, world: &W, pos: IVec3) -> bool {
        self.refresh_villages(world, false);
        self.raids.villages.is_village(pos)
    }

    /// Rebuilds the occupied village sections from the POIs and the
    /// villagers' claims: Java counts a POI section while any of its
    /// village POIs is occupied. Beds and job sites count when a villager
    /// claims them (stored villagers too, as Java's POI tickets outlive the
    /// entity's chunk); bells, whose meeting claims VoxelCraft doesn't
    /// track, count within 48 blocks of a claimed POI.
    pub(super) fn refresh_villages<W: MobWorld + ?Sized>(&mut self, world: &W, force: bool) {
        let now = self.raids.tick;
        if !force && self.raids.villages.built.is_some_and(|t| now - t < 20 && t <= now) {
            return;
        }
        // Loaded villagers' claims; stored villagers' are looked up per
        // loaded POI in the storage's claim indexes, so the cost doesn't
        // grow with the explored save.
        let mut claimed: FxHashSet<IVec3> = FxHashSet::default();
        for m in &self.mobs {
            if m.kind == MobKind::Villager
                && m.alive()
                && let Some(v) = &m.villager
            {
                claimed.extend(v.home);
                claimed.extend(v.job);
            }
        }
        let mut v = VillageSections { built: Some(now), ..Default::default() };
        let mut bells = Vec::new();
        let storage = &self.storage;
        world.village_pois(&mut |p, b| {
            if b.is_bed_head() {
                v.homes.push(p);
            }
            if claimed.contains(&p) || storage.claims_bed(p) || storage.claims_job(p) {
                claimed.insert(p);
                v.insert(p);
            } else if b.base() == crate::world::block::Block::BELL {
                bells.push(p);
            }
        });
        for p in bells {
            if claimed.iter().any(|c| c.as_dvec3().distance_squared(p.as_dvec3()) <= 48.0 * 48.0)
                || self.storage.claimed_near(p, 48.0)
            {
                v.insert(p);
            }
        }
        v.homes.sort_unstable_by_key(|p| p.to_array());
        self.raids.villages = v;
    }

    /// `Raids.createOrExtendRaid` for `player`, whose Raid Omen of
    /// `amplifier` fired with its omen position `at`. Returns the raid id.
    pub fn create_or_extend_raid<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        player: PlayerId,
        at: IVec3,
        amplifier: u8,
        can_start: bool,
    ) -> Option<u32> {
        if !self.raids.enabled || !can_start {
            return None;
        }
        self.refresh_villages(world, true);
        // The occupied village POIs within 64 blocks (a cube, as
        // `getInRange` filters by distance): their average is the centre.
        let mut claimed: FxHashSet<IVec3> = FxHashSet::default();
        for m in self.mobs.iter().chain(self.storage.mobs()) {
            if m.kind == MobKind::Villager
                && m.alive()
                && let Some(v) = &m.villager
            {
                claimed.extend(v.home);
                claimed.extend(v.job);
            }
        }
        let mut sum = DVec3::ZERO;
        let mut count = 0;
        let r2 = f64::from(POI_RADIUS * POI_RADIUS);
        world.village_pois(&mut |p, _| {
            if claimed.contains(&p) && p.as_dvec3().distance_squared(at.as_dvec3()) <= r2 {
                sum += p.as_dvec3();
                count += 1;
            }
        });
        let center = if count > 0 { (sum / f64::from(count)).floor().as_ivec3() } else { at };
        let difficulty = self.difficulty;
        let index = match self
            .raids
            .raids
            .iter()
            .position(|r| r.active && dist_sqr(r.center, center) < VALID_RAID_RADIUS_SQR)
        {
            // `getRaidAt` picks the nearest; ties are rare enough that the
            // first within range is kept when there are several.
            Some(_) => {
                let id = self.raids.raid_at(center).map(|r| r.id).unwrap();
                self.raids.raids.iter().position(|r| r.id == id).unwrap()
            }
            None => {
                // `getUniqueId` is `++nextId`.
                self.raids.next_id = self.raids.next_id.saturating_add(1);
                self.raids.raids.push(Raid::new(self.raids.next_id, center, difficulty));
                self.raids.raids.len() - 1
            }
        };
        let raid = &mut self.raids.raids[index];
        if (!raid.started || raid.omen_level < MAX_RAID_OMEN_LEVEL) && raid.absorb_omen(amplifier) {
            self.raids.outcomes.push(Outcome::Triggered { player });
        }
        Some(raid.id)
    }

    /// Every raid, patrol, bell and gift upkeep for `dt` seconds, before
    /// mobs move.
    pub(super) fn raid_tick<W: MobWorld + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        ctx: &Ctx,
        events: &mut Vec<EntityEvent>,
    ) {
        self.raids.carry += dt.max(0.0);
        while self.raids.carry >= 0.05 - 1e-9 {
            self.raids.carry -= 0.05;
            self.raids.tick += 1;
            self.raid_game_tick(world, ctx, events);
        }
    }

    fn raid_game_tick<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx, events: &mut Vec<EntityEvent>) {
        self.share_heroes();
        let has_raiders = self.mobs.iter().any(|m| m.illager.is_some());
        if self.raids.raids.is_empty() && !has_raiders && self.raids.bells.is_empty() && self.raids.heroes.is_empty() {
            self.patrol_tick(world, ctx);
            return;
        }
        self.ensure_mob_ids();
        if !self.raids.raids.is_empty() || has_raiders {
            self.refresh_villages(world, false);
        }
        self.raider_deaths();
        // `Raids.tick`.
        self.raids.spots = self.raids.raids.iter().map(|r| (r.id, r.center, r.active)).collect();
        let mut raids = std::mem::take(&mut self.raids.raids);
        raids.retain_mut(|raid| {
            if !self.raids.enabled {
                self.stop_raid(raid);
            }
            if raid.is_stopped() {
                self.leave_raid(raid.id);
                return false;
            }
            self.step_raid(raid, world, ctx, events);
            true
        });
        // Raids created during the step (none today) would land after these.
        raids.append(&mut self.raids.raids);
        self.raids.raids = raids;
        self.raider_upkeep(world);
        self.patrol_tick(world, ctx);
        self.bell_tick(events);
        self.gift_tick(world, ctx);
        self.villagers_in_raids(events);
    }

    /// Copies the heroes into each villager for its prices.
    fn share_heroes(&mut self) {
        for m in &mut self.mobs {
            if let Some(v) = &mut m.villager
                && v.heroes != self.raids.heroes
            {
                v.heroes.clone_from(&self.raids.heroes);
            }
        }
    }

    /// `Raid.stop`.
    fn stop_raid(&mut self, raid: &mut Raid) {
        raid.active = false;
        raid.bar_players.clear();
        raid.status = Status::Stopped;
    }

    /// Clears membership of a removed raid.
    fn leave_raid(&mut self, id: u32) {
        for m in &mut self.mobs {
            if let Some(s) = &mut m.illager
                && s.raider.raid == Some(id)
            {
                s.raider.raid = None;
                s.raider.raid_active = false;
                s.raider.celebrating = false;
            }
        }
    }

    /// The raid's living members: `(index, health)`.
    fn members(&self, id: u32) -> impl Iterator<Item = (usize, f32)> + '_ {
        self.mobs.iter().enumerate().filter_map(move |(i, m)| {
            (m.alive() && m.illager.as_ref().is_some_and(|s| s.raider.raid == Some(id))).then_some((i, m.health))
        })
    }

    /// `getTotalRaidersAlive`.
    pub fn raiders_alive(&self, id: u32) -> i32 {
        self.members(id).count() as i32
    }

    /// `getHealthOfLivingRaiders`.
    pub fn raider_health(&self, id: u32) -> f32 {
        self.members(id).map(|(_, h)| h).sum()
    }

    /// `Raider.die` for raiders that died since the last tick: the wave's
    /// leader goes, a player killer becomes a hero, and it leaves the raid
    /// without taking its health off the total. (The captain's bottle drops
    /// with its loot, in `drop_loot`.)
    fn raider_deaths(&mut self) {
        let mut heroes = Vec::new();
        for m in &mut self.mobs {
            if m.alive() {
                continue;
            }
            let Some(s) = &mut m.illager else { continue };
            if s.raider.dead_seen {
                continue;
            }
            s.raider.dead_seen = true;
            let Some(id) = s.raider.raid.take() else { continue };
            s.raider.raid_active = false;
            if let Some(p) = s.raider.last_player.filter(|_| m.player_hit_left > 0.0) {
                heroes.push((id, p));
            }
        }
        for (id, player) in heroes {
            if let Some(r) = self.raids.raids.iter_mut().find(|r| r.id == id)
                && !r.heroes.contains(&player)
            {
                r.heroes.push(player);
            }
        }
    }

    /// `Raid.tick` for one raid.
    fn step_raid<W: MobWorld + ?Sized>(
        &mut self,
        raid: &mut Raid,
        world: &W,
        ctx: &Ctx,
        _events: &mut Vec<EntityEvent>,
    ) {
        if raid.status == Status::Ongoing {
            let was_active = raid.active;
            raid.active = world.loaded(raid.center);
            if self.difficulty == Difficulty::Peaceful {
                self.stop_raid(raid);
                return;
            }
            if was_active != raid.active {
                raid.bar_visible = raid.active;
            }
            if !raid.active {
                return;
            }
            if !self.raids.villages.is_village(raid.center) {
                self.move_center_to_village(raid);
            }
            if !self.raids.villages.is_village(raid.center) {
                if raid.groups_spawned > 0 {
                    raid.status = Status::Loss;
                } else {
                    self.stop_raid(raid);
                }
            }
            raid.ticks_active += 1;
            if raid.ticks_active >= RAID_TIMEOUT_TICKS {
                self.stop_raid(raid);
                return;
            }
            let alive = self.raiders_alive(raid.id);
            if alive == 0 && raid.has_more_waves() {
                if raid.cooldown <= 0 {
                    if raid.cooldown == 0 && raid.groups_spawned > 0 {
                        raid.cooldown = PRE_RAID_TICKS;
                        raid.bar.title = BarTitle::Raid;
                        return;
                    }
                } else {
                    let cached = raid.wave_spawn_pos.is_some();
                    let mut search = !cached && raid.cooldown % 5 == 0;
                    if raid.wave_spawn_pos.is_some_and(|p| !world.entity_ticking(p)) {
                        search = true;
                    }
                    if search {
                        raid.wave_spawn_pos = self.find_spawn_pos(raid, world, EARLY_SPAWN_TRIES);
                    }
                    if raid.cooldown == PRE_RAID_TICKS || raid.cooldown % 20 == 0 {
                        self.update_bar_players(raid, ctx);
                    }
                    raid.cooldown -= 1;
                    raid.bar.progress =
                        ((PRE_RAID_TICKS - raid.cooldown) as f32 / PRE_RAID_TICKS as f32).clamp(0.0, 1.0);
                }
            }
            if raid.ticks_active % 20 == 0 {
                self.update_bar_players(raid, ctx);
                self.update_raiders(raid, world);
                raid.bar.title =
                    if alive > 0 && alive <= LOW_MOB_THRESHOLD { BarTitle::Remaining(alive) } else { BarTitle::Raid };
            }
            let mut horn = false;
            let mut attempts = 0;
            while raid.group_due(self.raiders_alive(raid.id)) {
                let pos = raid.wave_spawn_pos.or_else(|| self.find_spawn_pos(raid, world, DUE_SPAWN_TRIES));
                if let Some(pos) = pos {
                    raid.started = true;
                    self.spawn_group(raid, pos, world);
                    if !horn {
                        self.raid_horn(raid, pos, ctx);
                        horn = true;
                    }
                } else {
                    attempts += 1;
                }
                if attempts > NUM_SPAWN_ATTEMPTS {
                    self.stop_raid(raid);
                    break;
                }
            }
            if raid.started && !raid.has_more_waves() && alive == 0 {
                if raid.post_raid_ticks < POST_RAID_TICK_LIMIT {
                    raid.post_raid_ticks += 1;
                } else {
                    raid.status = Status::Victory;
                    let amplifier = (raid.omen_level - 1).clamp(0, 255) as u8;
                    for &player in &raid.heroes {
                        if ctx.players.iter().any(|t| t.id == player && t.alive && !t.spectator) {
                            self.raids.outcomes.push(Outcome::Hero { player, amplifier });
                        }
                    }
                }
            }
            // `Raider.hurtServer` and every membership change refresh the
            // bar from the raiders' health; between waves the countdown
            // fills it instead.
            if alive > 0 {
                self.update_bar_health(raid);
            }
        } else if raid.is_over() {
            raid.celebration_ticks += 1;
            if raid.celebration_ticks >= MAX_CELEBRATION_TICKS {
                self.stop_raid(raid);
                return;
            }
            if raid.celebration_ticks % 20 == 0 {
                self.update_bar_players(raid, ctx);
                raid.bar_visible = true;
                if raid.is_victory() {
                    raid.bar.progress = 0.0;
                    raid.bar.title = BarTitle::Victory;
                } else {
                    raid.bar.title = BarTitle::Defeat;
                }
            }
        }
    }

    /// `updateBossbar`.
    fn update_bar_health(&self, raid: &mut Raid) {
        let health = self.raider_health(raid.id);
        raid.bar.progress = if raid.total_health > 0.0 { (health / raid.total_health).clamp(0.0, 1.0) } else { 0.0 };
    }

    /// `updatePlayers`: living players whose nearest raid is this one. The
    /// raids are out of `self.raids` while they step, so this reads the
    /// snapshot taken before the step, with this raid's own centre.
    fn update_bar_players(&self, raid: &mut Raid, ctx: &Ctx) {
        let nearest = |pos: IVec3| {
            let mut best: Option<(u32, i64)> = None;
            for &(id, center, active) in &self.raids.spots {
                let center = if id == raid.id { raid.center } else { center };
                let active = if id == raid.id { raid.active } else { active };
                let d = dist_sqr(center, pos);
                if active && d < best.map_or(VALID_RAID_RADIUS_SQR, |b| b.1) {
                    best = Some((id, d));
                }
            }
            best.map(|b| b.0)
        };
        raid.bar_players = ctx
            .players
            .iter()
            .filter(|t| t.alive)
            .filter(|t| nearest(t.pos.floor().as_ivec3()) == Some(raid.id))
            .map(|t| t.id)
            .collect();
    }

    /// `moveRaidCenterToNearbyVillageSection`.
    fn move_center_to_village(&self, raid: &mut Raid) {
        let s = raid.center.div_euclid(IVec3::splat(16));
        let r = CENTER_SECTION_RADIUS;
        let mut best: Option<(IVec3, i64)> = None;
        for dx in -r..=r {
            for dy in -r..=r {
                for dz in -r..=r {
                    let c = section_center(s + IVec3::new(dx, dy, dz));
                    if self.raids.villages.is_village(c) {
                        let d = dist_sqr(c, raid.center);
                        if best.is_none_or(|b| d < b.1) {
                            best = Some((c, d));
                        }
                    }
                }
            }
        }
        if let Some((c, _)) = best {
            raid.center = c;
        }
    }

    /// `Raid.updateRaiders`: members too far away, or idle outside the
    /// village for 30 checks, leave.
    fn update_raiders<W: MobWorld + ?Sized>(&mut self, raid: &mut Raid, world: &W) {
        let _ = world;
        let mut removed = false;
        for i in 0..self.mobs.len() {
            let pos = self.mobs[i].pos.floor().as_ivec3();
            let idle = no_action_ticks(&self.mobs[i]);
            let village = self.raids.villages.is_village(pos);
            let m = &mut self.mobs[i];
            if !m.alive() {
                continue;
            }
            let Some(s) = &mut m.illager else { continue };
            if s.raider.raid != Some(raid.id) {
                continue;
            }
            let mut leave = dist_sqr(raid.center, pos) >= RAID_REMOVAL_THRESHOLD_SQR;
            if !leave && s.ticks > RAIDER_SETTLE_TICKS {
                if !village && idle > MAX_NO_ACTION_TIME {
                    s.raider.ticks_outside += 1;
                }
                leave = s.raider.ticks_outside >= OUTSIDE_RAID_BOUNDS_TIMEOUT;
            }
            if leave {
                // `removeFromRaid(.., true)`: its health leaves the total.
                s.raider.raid = None;
                s.raider.raid_active = false;
                raid.total_health -= m.health;
                removed = true;
            }
        }
        if removed {
            self.update_bar_health(raid);
        }
    }

    /// `findRandomSpawnPos(maxTries)`.
    fn find_spawn_pos<W: MobWorld + ?Sized>(&mut self, raid: &Raid, world: &W, tries: i32) -> Option<IVec3> {
        let seconds = raid.cooldown / 20;
        let how_far = SPAWN_DISTANCE_SCALE * seconds as f32 - SPAWN_DISTANCE_OFFSET;
        let start = self.rng.next_f32() * std::f32::consts::TAU;
        for i in 0..tries {
            let angle = start + std::f32::consts::PI * i as f32 / SPAWN_ANGLE_DIVISOR;
            let jitter = how_far.floor() as i32;
            let x = raid.center.x
                + (angle.cos() * SPAWN_RING_RADIUS * how_far).floor() as i32
                + self.rng.next_int(SPAWN_JITTER_BOUND) as i32 * jitter;
            let z = raid.center.z
                + (angle.sin() * SPAWN_RING_RADIUS * how_far).floor() as i32
                + self.rng.next_int(SPAWN_JITTER_BOUND) as i32 * jitter;
            // `WORLD_SURFACE`: the first air above the highest block.
            let Some(top) = world.world_surface(x, z) else { continue };
            let y = top + 1;
            if (y - raid.center.y).abs() > SPAWN_MAX_HEIGHT_DIFFERENCE {
                continue;
            }
            let pos = IVec3::new(x, y, z);
            if self.raids.villages.is_village(pos) && seconds > VILLAGE_SPAWN_SECONDS {
                continue;
            }
            let m = SPAWN_LOADED_MARGIN;
            let loaded =
                [-m, 0, m].iter().all(|&dx| [-m, 0, m].iter().all(|&dz| world.loaded(pos + IVec3::new(dx, 0, dz))));
            if loaded && world.entity_ticking(pos) && ravager_spot(world, pos) {
                return Some(pos);
            }
        }
        None
    }

    /// `Raid.spawnGroup`.
    fn spawn_group<W: MobWorld + ?Sized>(&mut self, raid: &mut Raid, pos: IVec3, world: &W) {
        let _ = world;
        let wave = raid.groups_spawned + 1;
        raid.total_health = 0.0;
        let bonus_wave = raid.bonus_due(self.raiders_alive(raid.id));
        let difficulty = self.difficulty;
        let mut leader_set = false;
        for kind in RaiderType::ALL {
            let count = raid.default_spawns(kind, wave, bonus_wave)
                + bonus_spawns(kind, wave, difficulty, bonus_wave, &mut self.rng);
            let mut ravagers = 0;
            for _ in 0..count {
                let Some(i) = self.spawn_raider(kind.kind(), raid, wave, pos) else { break };
                if !leader_set && can_lead(kind.kind()) {
                    make_captain(&mut self.mobs[i]);
                    leader_set = true;
                }
                if kind == RaiderType::Ravager {
                    let rider = if wave == num_groups(Difficulty::Normal) {
                        Some(MobKind::Pillager)
                    } else if wave >= num_groups(Difficulty::Hard) {
                        Some(if ravagers == 0 { MobKind::Evoker } else { MobKind::Vindicator })
                    } else {
                        None
                    };
                    ravagers += 1;
                    if let Some(rider) = rider {
                        let vehicle = self.mobs[i].uid;
                        if let Some(r) = self.spawn_raider(rider, raid, wave, pos) {
                            self.mobs[r].pos = pos.as_dvec3();
                            let uid = self.mobs[r].uid;
                            self.mount_ravager(uid, vehicle);
                        }
                    }
                }
            }
        }
        raid.wave_spawn_pos = None;
        raid.groups_spawned += 1;
        self.update_bar_health(raid);
    }

    /// `EntityType.create(EVENT)` then `joinRaid(.., pos, false)`: placed at
    /// the spawn's centre a block up, `finalizeSpawn`, the raid buffs, on the
    /// ground. Returns its index.
    fn spawn_raider(&mut self, kind: MobKind, raid: &mut Raid, wave: i32, pos: IVec3) -> Option<usize> {
        let at = pos.as_dvec3() + DVec3::new(0.5, 1.0, 0.5);
        self.raids_event_spawn = true;
        self.spawn(kind, at);
        self.raids_event_spawn = false;
        let i = self.mobs.len() - 1;
        self.ensure_mob_ids();
        let omen = raid.omen_level;
        let m = &mut self.mobs[i];
        let s = m.illager.as_mut()?;
        s.raider.raid = Some(raid.id);
        s.raider.wave = wave;
        s.raider.can_join_raid = true;
        s.raider.ticks_outside = 0;
        s.raider.raid_active = true;
        m.on_ground = true;
        apply_raid_buffs(&mut self.mobs[i], wave, omen, &mut self.rng);
        raid.total_health += self.mobs[i].health;
        Some(i)
    }

    /// `Raid.playSound`: the horn for every player within 64 blocks of the
    /// wave or in the raid, 13 blocks from them toward it.
    fn raid_horn(&mut self, raid: &Raid, pos: IVec3, ctx: &Ctx) {
        let spot = pos.as_dvec3() + DVec3::splat(0.5);
        for t in &ctx.players {
            let flat = (spot - t.pos).with_y(0.0);
            let d = flat.length();
            if d <= 64.0 || raid.bar_players.contains(&t.id) {
                let toward = if d > 0.0 { flat / d * 13.0 } else { DVec3::ZERO };
                let at = DVec3::new(t.pos.x + toward.x, t.pos.y, t.pos.z + toward.z);
                self.raids.outcomes.push(Outcome::Horn { player: t.id, at });
            }
        }
    }

    /// Per tick for every raider: `raid_active` and `celebrating` from its
    /// raid, the goal inputs `mob::raider` reads, joining raids nearby
    /// (`Raider.aiStep` every 20 ticks and `PathfindToRaidGoal`'s
    /// recruiting), and the patrols' companions.
    fn raider_upkeep<W: MobWorld + ?Sized>(&mut self, world: &W) {
        let _ = world;
        let tick = self.raids.tick;
        let joins: Vec<(usize, u32, i32)> = if tick % 20 == 0 && !self.raids.raids.is_empty() {
            let mut out = Vec::new();
            for (i, m) in self.mobs.iter().enumerate() {
                let Some(s) = &m.illager else { continue };
                if !m.alive()
                    || !super::illager::is_raider(m.kind)
                    || s.raider.raid.is_some()
                    || !s.raider.can_join_raid
                    || no_action_ticks(m) > MAX_NO_ACTION_TIME
                {
                    continue;
                }
                let at = m.pos.floor().as_ivec3();
                let near = self.raids.raid_at(at).or_else(|| {
                    // `PathfindToRaidGoal.recruitNearby`: a member within 16.
                    self.mobs
                        .iter()
                        .filter(|o| o.alive() && (o.pos - m.pos).abs().max_element() <= 16.0)
                        .filter_map(|o| o.illager.as_ref()?.raider.raid)
                        .find_map(|id| self.raids.get(id).filter(|r| r.active))
                });
                if let Some(r) = near {
                    out.push((i, r.id, r.groups_spawned));
                }
            }
            out
        } else {
            Vec::new()
        };
        for (i, id, wave) in joins {
            let health = self.mobs[i].health;
            if let Some(s) = &mut self.mobs[i].illager {
                s.raider.raid = Some(id);
                s.raider.wave = wave;
                s.raider.can_join_raid = true;
                s.raider.ticks_outside = 0;
            }
            if let Some(r) = self.raids.raids.iter_mut().find(|r| r.id == id) {
                r.total_health += health;
            }
        }
        // Leaders by raid and wave, for the banner goal.
        let mut leaders: Vec<(u32, i32)> = Vec::new();
        for m in &self.mobs {
            if let Some(s) = &m.illager
                && m.alive()
                && s.raider.patrol_leader
                && let Some(id) = s.raider.raid
            {
                leaders.push((id, s.raider.wave));
            }
        }
        let banners: Vec<DVec3> = self
            .items
            .iter()
            .filter(|it| it.pickup_delay <= 0.0 && crate::content::banner_patterns::is_ominous(&it.stack))
            .map(|it| it.pos)
            .collect();
        let homes = &self.raids.villages.homes;
        for m in &mut self.mobs {
            let pos = m.pos.floor().as_ivec3();
            let in_village = self.raids.villages.is_village(pos);
            let kind = m.kind;
            let Some(s) = &mut m.illager else { continue };
            let raid = s.raider.raid.and_then(|id| self.raids.raids.iter().find(|r| r.id == id));
            let r = &mut s.raider;
            r.raid_active = raid.is_some_and(|r| r.active);
            r.celebrating = raid.is_some_and(|r| r.is_loss());
            r.raid_center = raid.filter(|r| r.active && !r.is_over()).map(|r| r.center);
            r.in_village = in_village;
            // `RaiderMoveThroughVillageGoal`: a random home within 48 not
            // among the last three visited.
            if r.raid_center.is_some() {
                if r.village_target.is_none_or(|t| !homes.contains(&t)) && tick % 20 == 0 {
                    let near: Vec<IVec3> = homes
                        .iter()
                        .copied()
                        .filter(|h| (*h - pos).abs().max_element() <= VILLAGE_HOME_RADIUS)
                        .filter(|h| !r.visited.contains(h))
                        .collect();
                    r.village_target =
                        (!near.is_empty()).then(|| near[(r.wave as usize + pos.x as usize) % near.len()]);
                }
            } else {
                r.village_target = None;
            }
            // `ObtainRaidLeaderBannerGoal`: no leader alive for its wave.
            r.banner_target = None;
            if r.raid_active
                && raid.is_some_and(|x| !x.is_over())
                && can_lead(kind)
                && !leaders.contains(&(r.raid.unwrap_or(0), r.wave))
            {
                let range = super::illager::follow_range(kind);
                r.banner_target = banners
                    .iter()
                    .copied()
                    .filter(|b| (b.x - m.pos.x).abs() <= range && (b.z - m.pos.z).abs() <= range)
                    .filter(|b| (b.y - m.pos.y).abs() <= 8.0)
                    .min_by(|a, b| a.distance_squared(m.pos).total_cmp(&b.distance_squared(m.pos)));
            }
        }
        self.patrol_companions();
        self.pick_up_banners();
    }

    /// `ObtainRaidLeaderBannerGoal.tick` and `Raider.pickUpItem`: a raider
    /// within 1.414 blocks of the banner it is after takes it and leads.
    fn pick_up_banners(&mut self) {
        for i in 0..self.mobs.len() {
            let Some(target) = self.mobs[i].illager.as_ref().and_then(|s| s.raider.banner_target) else { continue };
            let pos = self.mobs[i].pos;
            if pos.distance(target) >= BANNER_PICKUP_DISTANCE {
                continue;
            }
            let Some(j) = self.items.iter().position(|it| {
                it.pos == target && crate::content::banner_patterns::is_ominous(&it.stack) && it.pickup_delay <= 0.0
            }) else {
                continue;
            };
            let item = self.items.remove(j);
            let m = &mut self.mobs[i];
            if let Some(old) = m.equipment.get(Slot::Head).copied()
                && (self.rng.next_f32() - 0.1).max(0.0) < m.equipment.drop_chance(Slot::Head)
            {
                self.items.push(super::ItemEntity::new(old, pos, DVec3::ZERO, 0.5, &mut self.rng));
            }
            let m = &mut self.mobs[i];
            m.equipment.set(Slot::Head, Some(item.stack));
            if let Some(s) = &mut m.illager {
                s.raider.patrol_leader = true;
                s.raider.patrolling = true;
                s.raider.banner_target = None;
            }
        }
    }

    /// `LongDistancePatrolGoal.findPatrolCompanions` for every patroller,
    /// and the leader's chosen step handed to its companions.
    fn patrol_companions(&mut self) {
        let patrollers: Vec<(usize, DVec3, bool, Option<IVec3>, bool)> = self
            .mobs
            .iter()
            .enumerate()
            .filter_map(|(i, m)| {
                let s = m.illager.as_ref()?;
                (m.alive() && super::illager::is_raider(m.kind)).then_some((
                    i,
                    m.pos,
                    s.raider.patrol_leader,
                    s.raider.patrol_step,
                    // `Raider.canJoinPatrol`: not in an active raid.
                    !s.raider.raid_active,
                ))
            })
            .collect();
        if patrollers.is_empty() {
            return;
        }
        let r = PATROL_COMPANION_RADIUS;
        for &(i, pos, _, _, _) in &patrollers {
            let companions = patrollers
                .iter()
                .any(|&(j, p, _, _, joins)| j != i && joins && (p - pos).abs().max_element() <= r + 0.6);
            if let Some(s) = &mut self.mobs[i].illager {
                s.raider.companions = companions;
            }
        }
        for &(i, pos, leader, step, _) in &patrollers {
            let (true, Some(step)) = (leader, step) else { continue };
            for &(j, p, _, _, joins) in &patrollers {
                if j != i
                    && joins
                    && (p - pos).abs().max_element() <= r + 0.6
                    && let Some(s) = &mut self.mobs[j].illager
                    && !s.raider.patrol_leader
                {
                    s.raider.patrol_target = Some(step);
                    s.raider.patrolling = true;
                }
            }
            if let Some(s) = &mut self.mobs[i].illager {
                s.raider.patrol_step = None;
            }
        }
    }

    /// `PatrolSpawner.tick` (`spawnEnemies` is the monster spawning rule
    /// with Peaceful excluded).
    fn patrol_tick<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx) {
        if !self.monster_spawning
            || !ctx.spawning
            || self.difficulty == Difficulty::Peaceful
            || !self.raids.patrols
            || ctx.dimension != crate::world::terrain::Dimension::Overworld
        {
            return;
        }
        self.raids.patrol_next -= 1;
        if self.raids.patrol_next > 0 {
            return;
        }
        self.raids.patrol_next += PATROL_INTERVAL + self.rng.next_int(PATROL_INTERVAL_SPREAD) as i32;
        // `isBrightOutside`: a sky darkening below 4.
        if ctx.spawn_light.sky_darken >= 4 || self.rng.next_int(PATROL_CHANCE) != 0 {
            return;
        }
        if ctx.players.is_empty() {
            return;
        }
        let player = ctx.players[self.rng.next_int(ctx.players.len() as u32) as usize];
        if player.spectator {
            return;
        }
        self.refresh_villages(world, false);
        let feet = player.pos.floor().as_ivec3();
        if self.raids.villages.close_to_village(feet, PATROL_VILLAGE_SECTIONS) {
            return;
        }
        let sign = |rng: &mut super::Rng| if rng.next_int(2) == 0 { -1 } else { 1 };
        let x = (PATROL_MIN_OFFSET + self.rng.next_int(PATROL_OFFSET_SPREAD) as i32) * sign(&mut self.rng);
        let z = (PATROL_MIN_OFFSET + self.rng.next_int(PATROL_OFFSET_SPREAD) as i32) * sign(&mut self.rng);
        let mut spawn = feet + IVec3::new(x, 0, z);
        let m = PATROL_LOADED_MARGIN;
        if ![-m, 0, m].iter().all(|&dx| [-m, 0, m].iter().all(|&dz| world.loaded(spawn + IVec3::new(dx, 0, dz)))) {
            return;
        }
        // `CAN_PILLAGER_PATROL_SPAWN`: the `early_game` timeline and the
        // mushroom fields.
        if self.clock_time < PATROL_START_TICKS
            || world.biome(spawn.x, spawn.z) == crate::world::terrain::Biome::MushroomFields
        {
            return;
        }
        let effective = self.difficulty.effective(self.clock_time, 0, self.moon_brightness);
        let size = effective.ceil() as i32 + 1;
        for i in 0..size {
            let Some(top) = world.surface(spawn.x, spawn.z) else { break };
            spawn.y = top + 1;
            let ok = self.spawn_patrol_member(world, spawn, i == 0);
            if i == 0 && !ok {
                break;
            }
            spawn.x += self.rng.next_int(PATROL_MEMBER_STEP) as i32 - self.rng.next_int(PATROL_MEMBER_STEP) as i32;
            spawn.z += self.rng.next_int(PATROL_MEMBER_STEP) as i32 - self.rng.next_int(PATROL_MEMBER_STEP) as i32;
        }
    }

    /// `PatrolSpawner.spawnPatrolMember`.
    fn spawn_patrol_member<W: MobWorld + ?Sized>(&mut self, world: &W, pos: IVec3, leader: bool) -> bool {
        let (Some(at), Some(below), Some(above)) =
            (world.block(pos), world.block(pos - IVec3::Y), world.block(pos + IVec3::Y))
        else {
            return false;
        };
        use super::monster_spawns::{valid_empty_cell, valid_spawn_floor};
        // `isValidEmptySpawnBlock` for the cell, then the patrol rules:
        // block light at most 8 and `checkAnyLightMonsterSpawnRules`
        // (`ON_GROUND`'s floor and Peaceful).
        if !valid_empty_cell(at, MobKind::Pillager)
            || world.block_light(pos) > PATROL_MAX_BLOCK_LIGHT
            || !valid_spawn_floor(below, MobKind::Pillager)
            || !valid_empty_cell(above, MobKind::Pillager)
        {
            return false;
        }
        self.raids_event_spawn = true;
        self.spawn(MobKind::Pillager, pos.as_dvec3());
        self.raids_event_spawn = false;
        let i = self.mobs.len() - 1;
        let m = &mut self.mobs[i];
        if leader {
            make_captain(m);
            let target = pos
                + IVec3::new(
                    PATROL_TARGET_OFFSET + self.rng.next_int(PATROL_TARGET_SPREAD) as i32,
                    0,
                    PATROL_TARGET_OFFSET + self.rng.next_int(PATROL_TARGET_SPREAD) as i32,
                );
            if let Some(s) = &mut self.mobs[i].illager {
                s.raider.patrol_target = Some(target);
            }
        }
        if let Some(s) = &mut self.mobs[i].illager {
            s.raider.patrolling = true;
        }
        true
    }

    /// `BellBlockEntity`: a ring sends villagers within 32 home (already in
    /// [`Entities::ring_bell`]) and, with raiders within 32 five ticks in,
    /// resonates; 40 ticks later raiders within 48 glow for 60 ticks.
    pub(super) fn bell_rang(&mut self, pos: IVec3) {
        let center = pos.as_dvec3() + DVec3::splat(0.5);
        let reach = BELL_SEARCH_RADIUS + 0.5;
        let nearby: Vec<u32> = self
            .mobs
            .iter()
            .filter(|m| m.alive() && (m.pos - center).abs().max_element() <= reach)
            .map(|m| m.uid)
            .collect();
        if let Some(b) = self.raids.bells.iter_mut().find(|b| b.pos == pos) {
            b.ticks = 0;
            if b.resonating.is_none() {
                b.nearby = nearby;
            }
        } else {
            self.raids.bells.push(Resonance { pos, ticks: 0, resonating: None, nearby });
        }
    }

    fn bell_tick(&mut self, events: &mut Vec<EntityEvent>) {
        if self.raids.bells.is_empty() {
            return;
        }
        let mut bells = std::mem::take(&mut self.raids.bells);
        bells.retain_mut(|b| {
            b.ticks += 1;
            let center = b.pos.as_dvec3() + DVec3::splat(0.5);
            let mobs = &self.mobs;
            let raider_within = |r: f64| -> Vec<u32> {
                b.nearby
                    .iter()
                    .filter_map(|&u| mobs.iter().find(|m| m.uid == u))
                    .filter(|m| {
                        m.alive() && super::illager::is_raider(m.kind) && m.pos.distance_squared(center) < r * r
                    })
                    .map(|m| m.uid)
                    .collect()
            };
            if b.resonating.is_none() && b.ticks >= BELL_TICKS_BEFORE_RESONATION {
                if !raider_within(BELL_HEAR_RADIUS).is_empty() {
                    b.resonating = Some(0);
                    events.push(EntityEvent::Sound { sound: MobSound::Raid(Cue::BellResonate), pos: center });
                } else if b.ticks >= BELL_DURATION {
                    return false;
                }
            }
            if let Some(t) = &mut b.resonating {
                if *t < BELL_MAX_RESONATION_TICKS {
                    *t += 1;
                } else {
                    let glow = raider_within(BELL_HIGHLIGHT_RADIUS);
                    for m in &mut self.mobs {
                        if glow.contains(&m.uid) {
                            m.effects.add(Effect::Glowing, 0, BELL_GLOW_TICKS);
                        }
                    }
                    return false;
                }
            }
            true
        });
        self.raids.bells = bells;
    }

    /// `GiveGiftToHero` for every villager that can see a hero.
    fn gift_tick<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx) {
        if self.raids.heroes.is_empty() {
            if !self.raids.gifts.is_empty() {
                self.raids.gifts.clear();
            }
            return;
        }
        let now = self.raids.tick;
        let heroes: Vec<(PlayerId, DVec3)> = ctx
            .players
            .iter()
            .filter(|t| !t.spectator && t.alive && self.raids.heroes.iter().any(|h| h.0 == t.id))
            .map(|t| (t.id, t.pos))
            .collect();
        let mut throws = Vec::new();
        for i in 0..self.mobs.len() {
            let m = &self.mobs[i];
            if m.kind != MobKind::Villager || !m.alive() || !m.simulation_active {
                continue;
            }
            let Some(v) = &m.villager else { continue };
            let raided = self.raids.raid_at(m.pos.floor().as_ivec3()).is_some();
            let busy = v.sleeping || v.trading || v.fleeing || v.bell_hide > 0.0 || raided;
            let eye = m.pos + DVec3::Y * (m.shape().height * 0.85);
            // `NEAREST_VISIBLE_PLAYER`: the nearest player within 16 in
            // sight, if it is a hero.
            let nearest = ctx
                .players
                .iter()
                .filter(|t| !t.spectator && t.pos.distance_squared(m.pos) < GIFT_SIGHT * GIFT_SIGHT)
                .filter(|t| super::mob::line_of_sight(world, eye, t.pos + DVec3::Y * crate::player::EYE_HEIGHT))
                .min_by(|a, b| a.pos.distance_squared(m.pos).total_cmp(&b.pos.distance_squared(m.pos)));
            let hero = nearest.and_then(|t| heroes.iter().find(|h| h.0 == t.id)).copied();
            let id = v.id;
            let gift = self.raids.gifts.entry(id).or_insert(Gift { wait: GIFT_FIRST_WAIT, started: None });
            match (gift.started, hero) {
                (Some(start), Some((_, at))) if !busy && now - start <= GIFT_TIMEOUT => {
                    let close = m.pos.floor().distance(at.floor()) < GIFT_THROW_DISTANCE;
                    if close {
                        if now - start > GIFT_TURN_DELAY {
                            throws.push((i, at));
                            gift.started = None;
                            gift.wait =
                                GIFT_MIN_WAIT + self.rng.next_int((GIFT_MAX_WAIT - GIFT_MIN_WAIT + 1) as u32) as i32;
                        }
                    } else if let Some(v) = &mut self.mobs[i].villager {
                        v.goal = Some(at);
                    }
                }
                (Some(_), _) => {
                    gift.started = None;
                    gift.wait = GIFT_MIN_WAIT + self.rng.next_int((GIFT_MAX_WAIT - GIFT_MIN_WAIT + 1) as u32) as i32;
                }
                (None, Some(_)) if !busy => {
                    if gift.wait > 0 {
                        gift.wait -= 1;
                    } else {
                        gift.started = Some(now);
                    }
                }
                _ => {}
            }
        }
        for (i, at) in throws {
            let m = &self.mobs[i];
            let Some(v) = &m.villager else { continue };
            let table = gift_table(gift_table_name(v.profession, m.baby));
            let from = m.pos + DVec3::Y * (m.shape().height * 0.85 - 0.3);
            let dir = (at - m.pos).normalize_or_zero() * (0.3 * 20.0);
            if let Some(stack) = roll_gift(table, &mut self.rng) {
                self.items.push(super::ItemEntity::new(stack, from, dir, 0.5, &mut self.rng));
            }
        }
        // Forget villagers that are gone.
        if now % 200 == 0 {
            let ids: FxHashSet<u64> = self.mobs.iter().filter_map(|m| m.villager.as_ref().map(|v| v.id)).collect();
            self.raids.gifts.retain(|id, _| ids.contains(id));
        }
    }

    /// Villagers in a raided village (`SetRaidStatus` and
    /// `VillagerGoalPackages.getRaidPackage`): once a wave is out (not
    /// between waves) the `RAID` activity sends them to shelter at a home
    /// (`LocateHidingPlace`) while the raid is undecided; after a victory
    /// they celebrate (`CelebrateVillagersSurvivedRaid`'s 1 in 100 cheer).
    pub(super) fn villagers_in_raids(&mut self, events: &mut Vec<EntityEvent>) {
        if self.raids.raids.is_empty() {
            return;
        }
        let alive: Vec<(u32, i32)> = self.raids.raids.iter().map(|r| (r.id, self.raiders_alive(r.id))).collect();
        for i in 0..self.mobs.len() {
            let m = &self.mobs[i];
            if m.kind != MobKind::Villager || !m.alive() || !m.simulation_active {
                continue;
            }
            let Some(raid) = self.raids.raid_at(m.pos.floor().as_ivec3()) else { continue };
            let left = alive.iter().find(|a| a.0 == raid.id).map_or(0, |a| a.1);
            if !raid.first_wave_spawned() || raid.between_waves(left) {
                continue;
            }
            let (victory, over) = (raid.is_victory(), raid.is_over());
            if victory {
                if self.rng.next_int(100) == 0 {
                    let pos = self.mobs[i].pos;
                    events.push(EntityEvent::Sound { sound: MobSound::Raid(Cue::VillagerCelebrate), pos });
                }
                continue;
            }
            if over {
                continue;
            }
            let m = &mut self.mobs[i];
            let Some(v) = &mut m.villager else { continue };
            if v.bell_hide < 1.0 {
                v.bell_hide = 1.0;
                v.sleeping = false;
                v.trading = false;
                if v.goal.is_none() {
                    v.goal = v.home.map(|p| p.as_dvec3() + DVec3::new(0.5, 0.6, 0.5));
                }
            }
        }
    }
}

/// Java's `noActionTime` for a raider: `idle` counts seconds, and
/// `Raider.updateNoActionTime` adds 2 a tick.
pub fn no_action_ticks(m: &super::Mob) -> i32 {
    (m.idle * 20.0 * RAIDER_NO_ACTION_STEP as f32).min(i32::MAX as f32) as i32
}

/// `RAVAGER_SPAWN_PLACEMENT_TYPE.isSpawnPositionOk` (`ON_GROUND`), or snow
/// below and air in the cell.
fn ravager_spot<W: MobWorld + ?Sized>(world: &W, pos: IVec3) -> bool {
    use super::monster_spawns::{valid_empty_cell, valid_spawn_floor};
    let (Some(at), Some(below), Some(above)) =
        (world.block(pos), world.block(pos - IVec3::Y), world.block(pos + IVec3::Y))
    else {
        return false;
    };
    let snow = below == crate::world::overworld_blocks::SNOW_LAYER
        || (crate::world::overworld_blocks::SNOW_LAYERS_2..=crate::world::overworld_blocks::SNOW_LAYERS_8)
            .contains(&below.0);
    (valid_spawn_floor(below, MobKind::Ravager)
        && valid_empty_cell(at, MobKind::Ravager)
        && valid_empty_cell(above, MobKind::Ravager))
        || (snow && at == crate::world::block::Block::AIR)
}

#[cfg(test)]
mod tests;
