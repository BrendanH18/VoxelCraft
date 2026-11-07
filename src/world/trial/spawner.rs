//! Trial spawner configurations (Java 26.3 `data/minecraft/trial_spawner/
//! trial_chamber/**`, defaults from `TrialSpawnerConfig$Builder`) and the
//! saved block-entity state (`TrialSpawnerStateData`). The state machine
//! itself runs in `entity::trial`, which owns the mobs it spawns.
use serde_json::{Value, json};

use crate::entity::MobKind;

/// `TrialSpawner.DEFAULT_TARGET_COOLDOWN_LENGTH`: 30 minutes.
pub const COOLDOWN_TICKS: i64 = 36_000;
/// `TrialSpawner.DEFAULT_PLAYER_SCAN_RANGE`.
pub const PLAYER_RANGE: i32 = 14;
/// `TrialSpawner.MAX_MOB_TRACKING_DISTANCE`.
pub const MOB_TRACKING_DISTANCE: i32 = 47;
/// `TrialSpawnerStateData.DELAY_BETWEEN_PLAYER_SCANS`.
pub const SCAN_INTERVAL: i64 = 20;
/// `TrialSpawner.DETECT_PLAYER_SPAWN_BUFFER`.
pub const DETECT_SPAWN_BUFFER: i64 = 40;
/// `TrialSpawnerStateData.TRIAL_OMEN_PER_BAD_OMEN_LEVEL`.
pub const TRIAL_OMEN_PER_LEVEL: u32 = 18_000;
/// `TrialSpawnerState.DELAY_BEFORE_EJECT_AFTER_KILLING_LAST_MOB`.
pub const EJECT_DELAY: f32 = 40.0;
/// `TrialSpawnerState.TIME_BETWEEN_EACH_EJECTION`.
pub const TIME_BETWEEN_EJECTIONS: i32 = 30;
/// `TrialSpawnerConfig.ticksBetweenItemSpawners`.
pub const TICKS_BETWEEN_ITEM_SPAWNERS: i64 = 160;

/// One `spawn_potentials` entry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Potential {
    pub kind: MobKind,
    /// `IsBaby: 1` (the baby-zombie spawner).
    pub baby: bool,
    /// The slime `Size` tag plus one (Java's `setSize(Size + 1)`).
    pub size: Option<u8>,
    /// Java runs `finalizeSpawn` only when the entity tag holds just its id.
    pub finalize: bool,
    /// `equipment.loot_table`, with every slot drop chance 0.
    pub equipment: Option<&'static str>,
    pub weight: u32,
}

/// One `TrialSpawnerConfig`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    pub spawn_range: i32,
    pub total_mobs: f32,
    pub simultaneous_mobs: f32,
    pub total_mobs_added_per_player: f32,
    pub simultaneous_mobs_added_per_player: f32,
    pub ticks_between_spawn: i64,
    pub potentials: &'static [Potential],
    /// `loot_tables_to_eject`, by weight.
    pub loot: &'static [(&'static str, u32)],
    pub items_to_drop_when_ominous: &'static str,
}

impl Config {
    /// `calculateTargetTotalMobs`.
    pub fn target_total(&self, additional_players: u32) -> u32 {
        (self.total_mobs + self.total_mobs_added_per_player * additional_players as f32).floor() as u32
    }
    /// `calculateTargetSimultaneousMobs`.
    pub fn target_simultaneous(&self, additional_players: u32) -> u32 {
        (self.simultaneous_mobs + self.simultaneous_mobs_added_per_player * additional_players as f32).floor() as u32
    }
}

const NORMAL_LOOT: &[(&str, u32)] = &[("spawners/trial_chamber/consumables", 1), ("spawners/trial_chamber/key", 1)];
const OMINOUS_LOOT: &[(&str, u32)] =
    &[("spawners/ominous/trial_chamber/key", 3), ("spawners/ominous/trial_chamber/consumables", 7)];
const ITEMS_WHEN_OMINOUS: &str = "spawners/trial_chamber/items_to_drop_when_ominous";
const MELEE: Option<&str> = Some("equipment/trial_chamber_melee");
const RANGED: Option<&str> = Some("equipment/trial_chamber_ranged");

/// `TrialSpawnerConfig.DEFAULT` values.
const DEFAULT: Config = Config {
    spawn_range: 4,
    total_mobs: 6.0,
    simultaneous_mobs: 2.0,
    total_mobs_added_per_player: 2.0,
    simultaneous_mobs_added_per_player: 1.0,
    ticks_between_spawn: 40,
    potentials: &[],
    loot: NORMAL_LOOT,
    items_to_drop_when_ominous: ITEMS_WHEN_OMINOUS,
};

const fn mob(kind: MobKind, equipment: Option<&'static str>) -> Potential {
    Potential { kind, baby: false, size: None, finalize: true, equipment, weight: 1 }
}

macro_rules! potentials {
    ($($p:expr),*) => { &[$($p),*] };
}

const BREEZE: &[Potential] = potentials!(mob(MobKind::Breeze, None));
const HUSK: &[Potential] = potentials!(mob(MobKind::Husk, None));
const HUSK_EQ: &[Potential] = potentials!(mob(MobKind::Husk, MELEE));
const SPIDER: &[Potential] = potentials!(mob(MobKind::Spider, None));
const ZOMBIE: &[Potential] = potentials!(mob(MobKind::Zombie, None));
const ZOMBIE_EQ: &[Potential] = potentials!(mob(MobKind::Zombie, MELEE));
const BOGGED: &[Potential] = potentials!(mob(MobKind::Bogged, None));
const BOGGED_EQ: &[Potential] = potentials!(mob(MobKind::Bogged, RANGED));
const SKELETON: &[Potential] = potentials!(mob(MobKind::Skeleton, None));
const SKELETON_EQ: &[Potential] = potentials!(mob(MobKind::Skeleton, RANGED));
const STRAY: &[Potential] = potentials!(mob(MobKind::Stray, None));
const STRAY_EQ: &[Potential] = potentials!(mob(MobKind::Stray, RANGED));
const BABY_ZOMBIE: &[Potential] = potentials!(Potential {
    kind: MobKind::Zombie,
    baby: true,
    size: None,
    finalize: false,
    equipment: None,
    weight: 1
});
const BABY_ZOMBIE_EQ: &[Potential] = potentials!(Potential {
    kind: MobKind::Zombie,
    baby: true,
    size: None,
    finalize: false,
    equipment: MELEE,
    weight: 1
});
const CAVE_SPIDER: &[Potential] = potentials!(mob(MobKind::CaveSpider, None));
const SILVERFISH: &[Potential] = potentials!(mob(MobKind::Silverfish, None));
const SLIME: &[Potential] = potentials!(
    Potential { kind: MobKind::Slime, baby: false, size: Some(2), finalize: false, equipment: None, weight: 3 },
    Potential { kind: MobKind::Slime, baby: false, size: Some(3), finalize: false, equipment: None, weight: 1 }
);

const fn normal(sim: f32, sim_add: f32, ticks: i64, potentials: &'static [Potential]) -> Config {
    Config {
        simultaneous_mobs: sim,
        simultaneous_mobs_added_per_player: sim_add,
        ticks_between_spawn: ticks,
        potentials,
        ..DEFAULT
    }
}

const fn ominous(sim: f32, sim_add: f32, ticks: i64, total: f32, potentials: &'static [Potential]) -> Config {
    Config {
        simultaneous_mobs: sim,
        simultaneous_mobs_added_per_player: sim_add,
        ticks_between_spawn: ticks,
        total_mobs: total,
        potentials,
        loot: OMINOUS_LOOT,
        ..DEFAULT
    }
}

/// Every trial chamber spawner configuration: Java name, normal and
/// ominous. Saved by name; the index is only this session's handle.
pub const CONFIGS: [(&str, Config, Config); 14] = [
    (
        "trial_chamber/breeze",
        Config { total_mobs: 2.0, total_mobs_added_per_player: 1.0, ..normal(1.0, 0.5, 20, BREEZE) },
        Config { total_mobs_added_per_player: 1.0, ..ominous(2.0, 0.5, 20, 4.0, BREEZE) },
    ),
    ("trial_chamber/melee/husk", normal(3.0, 0.5, 20, HUSK), ominous(3.0, 0.5, 20, 6.0, HUSK_EQ)),
    ("trial_chamber/melee/spider", normal(3.0, 0.5, 20, SPIDER), ominous(4.0, 0.5, 20, 12.0, SPIDER)),
    ("trial_chamber/melee/zombie", normal(3.0, 0.5, 20, ZOMBIE), ominous(3.0, 0.5, 20, 6.0, ZOMBIE_EQ)),
    ("trial_chamber/ranged/poison_skeleton", normal(3.0, 0.5, 20, BOGGED), ominous(3.0, 0.5, 20, 6.0, BOGGED_EQ)),
    ("trial_chamber/ranged/skeleton", normal(3.0, 0.5, 20, SKELETON), ominous(3.0, 0.5, 20, 6.0, SKELETON_EQ)),
    ("trial_chamber/ranged/stray", normal(3.0, 0.5, 20, STRAY), ominous(3.0, 0.5, 20, 6.0, STRAY_EQ)),
    (
        "trial_chamber/slow_ranged/poison_skeleton",
        normal(4.0, 2.0, 160, BOGGED),
        ominous(4.0, 2.0, 160, 6.0, BOGGED_EQ),
    ),
    ("trial_chamber/slow_ranged/skeleton", normal(4.0, 2.0, 160, SKELETON), ominous(4.0, 2.0, 160, 6.0, SKELETON_EQ)),
    ("trial_chamber/slow_ranged/stray", normal(4.0, 2.0, 160, STRAY), ominous(4.0, 2.0, 160, 6.0, STRAY_EQ)),
    (
        "trial_chamber/small_melee/baby_zombie",
        normal(2.0, 0.5, 20, BABY_ZOMBIE),
        ominous(2.0, 0.5, 20, 6.0, BABY_ZOMBIE_EQ),
    ),
    (
        "trial_chamber/small_melee/cave_spider",
        normal(3.0, 0.5, 20, CAVE_SPIDER),
        ominous(4.0, 0.5, 20, 12.0, CAVE_SPIDER),
    ),
    ("trial_chamber/small_melee/silverfish", normal(3.0, 0.5, 20, SILVERFISH), ominous(4.0, 0.5, 20, 12.0, SILVERFISH)),
    ("trial_chamber/small_melee/slime", normal(3.0, 0.5, 20, SLIME), ominous(4.0, 0.5, 20, 12.0, SLIME)),
];

pub fn config_id(name: &str) -> Option<u8> {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    CONFIGS.iter().position(|c| c.0 == name).map(|i| i as u8)
}

/// The configuration an unconfigured spawner uses: empty potentials, so
/// it stays inactive (a creative-placed trial spawner).
pub const EMPTY: Config = DEFAULT;

/// `TrialSpawnerStateData` plus the configuration choice.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrialSpawner {
    /// Index into [`CONFIGS`]; `None` for an unconfigured spawner.
    pub config: Option<u8>,
    /// `detectedPlayers`, in the order they were added.
    pub detected: Vec<u32>,
    /// `currentMobs`, by mob uid.
    pub current_mobs: Vec<u32>,
    pub cooldown_ends_at: i64,
    pub next_mob_spawns_at: i64,
    pub total_mobs_spawned: u32,
    /// `nextSpawnData`: an index into the active configuration's potentials.
    pub next_spawn: Option<u8>,
    /// `ejectingLootTable`: an index into the active configuration's loot.
    pub ejecting_loot: Option<u8>,
}

impl TrialSpawner {
    pub fn config(&self, ominous: bool) -> &'static Config {
        match self.config {
            Some(i) if (i as usize) < CONFIGS.len() => {
                let c = &CONFIGS[i as usize];
                if ominous { &c.2 } else { &c.1 }
            }
            _ => &EMPTY,
        }
    }

    pub fn config_name(&self) -> Option<&'static str> {
        self.config.and_then(|i| CONFIGS.get(i as usize)).map(|c| c.0)
    }

    /// `resetStatistics`.
    pub fn reset_statistics(&mut self) {
        self.detected.clear();
        self.total_mobs_spawned = 0;
        self.next_mob_spawns_at = 0;
        self.cooldown_ends_at = 0;
    }

    /// `reset`.
    pub fn reset(&mut self) {
        self.current_mobs.clear();
        self.next_spawn = None;
        self.reset_statistics();
    }

    /// `countAdditionalPlayers`.
    pub fn additional_players(&self) -> u32 {
        (self.detected.len() as u32).saturating_sub(1)
    }

    pub fn save(&self) -> Value {
        json!({
            "config": self.config_name(),
            "registered_players": self.detected,
            "current_mobs": self.current_mobs,
            "cooldown_ends_at": self.cooldown_ends_at,
            "next_mob_spawns_at": self.next_mob_spawns_at,
            "total_mobs_spawned": self.total_mobs_spawned,
            "spawn_data": self.next_spawn,
            "ejecting_loot_table": self.ejecting_loot,
        })
    }

    pub fn load(v: &Value) -> Option<Self> {
        if !v.is_object() {
            return None;
        }
        let ids = |v: &Value| -> Vec<u32> {
            v.as_array().into_iter().flatten().filter_map(|n| n.as_u64().and_then(|n| u32::try_from(n).ok())).collect()
        };
        let small = |v: &Value| v.as_u64().and_then(|n| u8::try_from(n).ok());
        Some(Self {
            config: v["config"].as_str().and_then(config_id),
            detected: ids(&v["registered_players"]),
            current_mobs: ids(&v["current_mobs"]),
            cooldown_ends_at: v["cooldown_ends_at"].as_i64().unwrap_or(0),
            next_mob_spawns_at: v["next_mob_spawns_at"].as_i64().unwrap_or(0),
            total_mobs_spawned: v["total_mobs_spawned"].as_u64().and_then(|n| u32::try_from(n).ok()).unwrap_or(0),
            next_spawn: small(&v["spawn_data"]),
            ejecting_loot: small(&v["ejecting_loot_table"]),
        })
    }
}
