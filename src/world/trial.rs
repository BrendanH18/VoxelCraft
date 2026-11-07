//! v0.16 Track C: trial spawners, vaults and their items (Java 26.3
//! `TrialSpawnerBlock`, `VaultBlock`, `TrialSpawner`, `VaultBlockEntity`).
//! See the dated verification record for sources and differences.
//!
//! Block states 15488–15743 belong to this track:
//! - 15488–15499: trial spawner, `15488 + state + 6 * ominous` with Java's
//!   `trial_spawner_state` order (inactive, waiting for players, active,
//!   waiting for reward ejection, ejecting reward, cooldown).
//! - 15504–15535: vault, `15504 + facing + 4 * vault_state + 16 * ominous`,
//!   facing in [`Facing`]'s order (south, north, east, west) and Java's
//!   `vault_state` order (inactive, active, unlocking, ejecting).
//!
//! Items 1470–1479: trial key, ominous trial key, breeze rod, wind charge,
//! ominous bottles of amplifier 0–4 and the trial explorer map.
use std::collections::{BTreeMap, BTreeSet};

use glam::IVec3;
use serde_json::{Value, json};

use super::block::{Block, Facing, RenderKind};
use crate::inventory::Stack;
use crate::item::{Item, ItemInfo, ItemKind, Sprite};

pub mod spawner;
#[cfg(test)]
mod tests;
pub mod vault;

pub const SPAWNER_FIRST: u16 = 15488;
pub const SPAWNER_LAST: u16 = SPAWNER_FIRST + 11;
pub const VAULT_FIRST: u16 = 15504;
pub const VAULT_LAST: u16 = VAULT_FIRST + 31;
/// The track's reservation; unused states stay unregistered.
pub const RESERVED_LAST: u16 = 15743;

pub const TRIAL_SPAWNER: Block = Block(SPAWNER_FIRST);
pub const VAULT: Block = Block(VAULT_FIRST);
pub const OMINOUS_VAULT: Block = Block(VAULT_FIRST + 16);

/// Texture layers 3760–3855: twelve spawner layers, fourteen vault
/// layers, then the trial explorer map icon.
pub const TEX: u16 = 3760;
const SPAWNER_TEX: u16 = TEX;
const VAULT_TEX: u16 = TEX + 12;
pub const MAP_TEX: u16 = TEX + 26;
pub const TEX_END: u16 = TEX + 27;

pub const TRIAL_KEY: Item = Item(1470);
pub const OMINOUS_TRIAL_KEY: Item = Item(1471);
pub const BREEZE_ROD: Item = Item(1472);
pub const WIND_CHARGE: Item = Item(1473);
/// Ominous bottles of amplifier 0–4 (Java's `ominous_bottle_amplifier`
/// component; one item per amplifier here).
pub const OMINOUS_BOTTLE: Item = Item(1474);
/// Java's `buried_trial_chambers_map` explorer map, sold by journeyman
/// cartographers.
pub const TRIAL_CHAMBERS_MAP: Item = Item(1479);
pub const ITEM_FIRST: u16 = 1470;
pub const ITEM_END: u16 = 1480;

/// `OminousBottleAmplifier.EFFECT_DURATION`: Bad Omen ticks from a bottle.
pub const BAD_OMEN_TICKS: u32 = 120_000;

/// Java's `TrialSpawnerState`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub enum SpawnerState {
    Inactive,
    WaitingForPlayers,
    Active,
    WaitingForRewardEjection,
    EjectingReward,
    Cooldown,
}

impl SpawnerState {
    pub const ALL: [SpawnerState; 6] = [
        SpawnerState::Inactive,
        SpawnerState::WaitingForPlayers,
        SpawnerState::Active,
        SpawnerState::WaitingForRewardEjection,
        SpawnerState::EjectingReward,
        SpawnerState::Cooldown,
    ];

    /// `TrialSpawnerState.lightLevel`.
    pub const fn light(self) -> u8 {
        match self {
            SpawnerState::Inactive | SpawnerState::Cooldown => 0,
            SpawnerState::WaitingForPlayers => 4,
            _ => 8,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            SpawnerState::Inactive => "inactive",
            SpawnerState::WaitingForPlayers => "waiting_for_players",
            SpawnerState::Active => "active",
            SpawnerState::WaitingForRewardEjection => "waiting_for_reward_ejection",
            SpawnerState::EjectingReward => "ejecting_reward",
            SpawnerState::Cooldown => "cooldown",
        }
    }
}

/// Java's `VaultState`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VaultState {
    Inactive,
    Active,
    Unlocking,
    Ejecting,
}

impl VaultState {
    pub const ALL: [VaultState; 4] =
        [VaultState::Inactive, VaultState::Active, VaultState::Unlocking, VaultState::Ejecting];

    /// `VaultState.LightLevel`: half lit (6) while inactive, lit (12) otherwise.
    pub const fn light(self) -> u8 {
        match self {
            VaultState::Inactive => 6,
            _ => 12,
        }
    }
}

pub const fn owns(id: u16) -> bool {
    (id >= SPAWNER_FIRST && id <= SPAWNER_LAST) || (id >= VAULT_FIRST && id <= VAULT_LAST)
}

pub const fn spawner_block(state: SpawnerState, ominous: bool) -> Block {
    Block(SPAWNER_FIRST + state as u16 + 6 * ominous as u16)
}

/// State and ominous flag of a trial spawner block.
pub fn spawner(b: Block) -> Option<(SpawnerState, bool)> {
    let i = b.0.checked_sub(SPAWNER_FIRST).filter(|&i| i < 12)?;
    Some((SpawnerState::ALL[(i % 6) as usize], i >= 6))
}

pub const fn vault_block(facing: Facing, state: VaultState, ominous: bool) -> Block {
    Block(VAULT_FIRST + facing as u16 + 4 * state as u16 + 16 * ominous as u16)
}

/// Facing, state and ominous flag of a vault block.
pub fn vault(b: Block) -> Option<(Facing, VaultState, bool)> {
    let i = b.0.checked_sub(VAULT_FIRST).filter(|&i| i < 32)?;
    let facing = [Facing::South, Facing::North, Facing::East, Facing::West][(i % 4) as usize];
    Some((facing, VaultState::ALL[(i / 4 % 4) as usize], i >= 16))
}

pub const fn emission(id: u16) -> u8 {
    if id >= SPAWNER_FIRST && id <= SPAWNER_LAST {
        let s = (id - SPAWNER_FIRST) % 6;
        return match s {
            0 | 5 => 0,
            1 => 4,
            _ => 8,
        };
    }
    if id >= VAULT_FIRST && id <= VAULT_LAST {
        return if ((id - VAULT_FIRST) / 4).is_multiple_of(4) { 6 } else { 12 };
    }
    0
}

/// Every registered block's name, render kind and face layers (east,
/// west, top, bottom, south, north).
pub const fn registry(id: u16) -> (&'static str, RenderKind, [u16; 6]) {
    if id >= SPAWNER_FIRST && id <= SPAWNER_LAST {
        let i = id - SPAWNER_FIRST;
        let o = if i >= 6 { 6 } else { 0 };
        let lit = matches!(i % 6, 1..=4);
        let side = SPAWNER_TEX + o + if lit { 1 } else { 0 };
        let top = SPAWNER_TEX
            + o
            + match i % 6 {
                4 => 4,
                1..=3 => 3,
                _ => 2,
            };
        return ("trial spawner", RenderKind::Cutout, [side, side, top, SPAWNER_TEX + o + 5, side, side]);
    }
    if id >= VAULT_FIRST && id <= VAULT_LAST {
        let i = id - VAULT_FIRST;
        let o = if i >= 16 { 7 } else { 0 };
        let state = i / 4 % 4;
        let front = VAULT_TEX
            + o
            + match state {
                0 => 0,
                1 | 2 => 1,
                _ => 2,
            };
        let side = VAULT_TEX + o + if state == 0 { 3 } else { 4 };
        let mut t = [side, side, VAULT_TEX + o + 5, VAULT_TEX + o + 6, side, side];
        // South, north, east, west in `Facing` order; faces east, west,
        // top, bottom, south, north.
        let face = match i % 4 {
            0 => 4,
            1 => 5,
            2 => 0,
            _ => 1,
        };
        t[face] = front;
        // Java names both "vault"; `Block::from_name("ominous vault")`
        // finds the ominous states for templates and commands.
        return ("vault", RenderKind::Cutout, t);
    }
    ("unknown", RenderKind::Invisible, [0; 6])
}

/// The block family's canonical state (items and comparisons).
pub fn base(b: Block) -> Option<Block> {
    if spawner(b).is_some() {
        Some(TRIAL_SPAWNER)
    } else if vault(b).is_some() {
        Some(VAULT)
    } else {
        None
    }
}

pub fn with_facing(b: Block, facing: Facing) -> Option<Block> {
    vault(b).map(|(_, s, o)| vault_block(facing, s, o))
}

pub fn palette_ids() -> impl Iterator<Item = u16> {
    [TRIAL_SPAWNER.0, VAULT.0].into_iter()
}

/// Item names, kinds and icons.
pub fn item_info(item: Item) -> Option<ItemInfo> {
    let info = |name, sprite| ItemInfo { name, kind: ItemKind::Material, max_stack: 64, sprite };
    Some(match item.0 {
        1470 => info("trial key", Sprite::TrialKey(false)),
        1471 => info("ominous trial key", Sprite::TrialKey(true)),
        1472 => info("breeze rod", Sprite::BreezeRod),
        1473 => info("wind charge", Sprite::WindCharge),
        1474..=1478 => {
            const NAMES: [&str; 5] =
                ["ominous bottle", "ominous bottle II", "ominous bottle III", "ominous bottle IV", "ominous bottle V"];
            info(NAMES[(item.0 - 1474) as usize], Sprite::OminousBottle((item.0 - 1474) as u8))
        }
        1479 => info("trial explorer map", Sprite::Map(true)),
        _ => return None,
    })
}

/// The Bad Omen amplifier an ominous bottle gives.
pub fn ominous_amplifier(item: Item) -> Option<u8> {
    (1474..=1478).contains(&item.0).then(|| (item.0 - 1474) as u8)
}

pub const fn ominous_bottle(amplifier: u8) -> Item {
    Item(OMINOUS_BOTTLE.0 + if amplifier > 4 { 4 } else { amplifier as u16 })
}

/// Java registry names resolved for loot tables.
pub fn java_item(name: &str) -> Option<Item> {
    Some(match name {
        "trial_key" => TRIAL_KEY,
        "ominous_trial_key" => OMINOUS_TRIAL_KEY,
        "breeze_rod" => BREEZE_ROD,
        "wind_charge" => WIND_CHARGE,
        "ominous_bottle" => OMINOUS_BOTTLE,
        "buried_trial_chambers_map" => TRIAL_CHAMBERS_MAP,
        "trial_spawner" => Item::from_block(TRIAL_SPAWNER),
        "vault" => Item::from_block(VAULT),
        _ => return None,
    })
}

/// Original procedural textures: dark tuff-and-copper cages for the
/// spawner, copper-banded strongboxes for the vault; ominous variants
/// are teal-tinted.
pub fn pixel(layer: u16, x: usize, y: usize) -> [u8; 4] {
    let n = ((x * 13 + y * 29 + x * y * 7) % 17) as u8;
    if layer == MAP_TEX {
        return map_pixel(x, y);
    }
    if layer >= VAULT_TEX {
        let i = layer - VAULT_TEX;
        let ominous = i >= 7;
        let kind = i % 7;
        let accent = if ominous { [52, 160, 170] } else { [214, 140, 72] };
        let lit = matches!(kind, 1 | 2 | 4);
        let frame = x == 0 || y == 0 || x == 15 || y == 15 || x == 1 || y == 1 || x == 14 || y == 14;
        let band = y == 5 || y == 10;
        let stone = [70 + n, 74 + n, 78 + n];
        let c = match kind {
            // Fronts: a keyhole window, glowing when active or ejecting.
            0..=2 => {
                let window = (5..=10).contains(&x) && (4..=11).contains(&y);
                let hole = (7..=8).contains(&x) && (6..=9).contains(&y);
                if frame {
                    accent
                } else if hole {
                    if lit { [250, 230, 160] } else { [30, 28, 32] }
                } else if window {
                    if kind == 2 {
                        [255, 220, 120]
                    } else if lit {
                        [accent[0] / 2 + 100, accent[1] / 2 + 90, 80]
                    } else {
                        [44, 42, 48]
                    }
                } else {
                    stone
                }
            }
            3 | 4 => {
                if frame || band {
                    accent
                } else if lit && (x + y).is_multiple_of(6) {
                    [accent[0] / 2 + 110, accent[1] / 2 + 100, accent[2] / 2 + 60]
                } else {
                    stone
                }
            }
            // Top and bottom: riveted plates.
            _ => {
                if frame {
                    accent
                } else if (x % 5 == 2) && (y % 5 == 2) {
                    [180, 170, 150]
                } else {
                    [62 + n, 64 + n, 70 + n]
                }
            }
        };
        return [c[0], c[1], c[2], 255];
    }
    let i = layer - SPAWNER_TEX;
    let ominous = i >= 6;
    let kind = i % 6;
    let flame = if ominous { [70, 200, 210] } else { [240, 150, 50] };
    let bars = x.is_multiple_of(4) || y.is_multiple_of(4) || x == 15 || y == 15;
    let edge = x == 0 || y == 0 || x == 15 || y == 15;
    let tuff = [88 + n, 90 + n, 84 + n];
    let c = match kind {
        // Sides, dark and lit: a tuff cage with copper corners.
        0 | 1 => {
            if edge {
                [150 + n, 96, 64]
            } else if bars {
                tuff
            } else if kind == 1 && (y + x / 3) % 5 < 2 {
                flame
            } else {
                return [0, 0, 0, 0];
            }
        }
        // Tops: inactive, active, ejecting (an open shutter).
        2..=4 => {
            let shutter = (5..=10).contains(&x) && (5..=10).contains(&y);
            if edge {
                [150 + n, 96, 64]
            } else if shutter {
                match kind {
                    2 => [52, 52, 56],
                    3 => flame,
                    _ => [20, 18, 22],
                }
            } else {
                tuff
            }
        }
        _ => {
            if edge {
                [150 + n, 96, 64]
            } else {
                [72 + n, 74 + n, 70 + n]
            }
        }
    };
    [c[0], c[1], c[2], 255]
}

/// An original folded-map icon with a trial-key glyph.
fn map_pixel(x: usize, y: usize) -> [u8; 4] {
    if !(1..=14).contains(&x) || !(2..=13).contains(&y) {
        return [0, 0, 0, 0];
    }
    let fold = x == 5 || x == 10;
    let mark = (6..=9).contains(&x) && (6..=9).contains(&y) && (x + y).is_multiple_of(2);
    let c = if mark {
        [180, 110, 60]
    } else if fold {
        [190, 180, 150]
    } else {
        [222, 212, 178]
    };
    [c[0], c[1], c[2], 255]
}

/// Trial spawners and vaults by position, kept while their chunk is
/// unloaded (Java block entities saved with the chunk).
#[derive(Default, Debug)]
pub struct State {
    pub spawners: BTreeMap<[i32; 3], spawner::TrialSpawner>,
    pub vaults: BTreeMap<[i32; 3], vault::Vault>,
    /// Positions written by world generation in this session, so a
    /// chunk that generates again keeps its saved entries.
    pub generated: BTreeSet<[i32; 3]>,
}

impl super::World {
    /// Keeps trial spawner and vault entries in step with block changes.
    pub(super) fn track_trial(&mut self, p: IVec3, old: Block, new: Block) {
        let key = p.to_array();
        if spawner(new).is_some() {
            if spawner(old).is_none() {
                self.trial.spawners.entry(key).or_default();
            }
        } else if spawner(old).is_some() {
            self.trial.spawners.remove(&key);
        }
        if vault(new).is_some() {
            if vault(old).is_none() {
                self.trial.vaults.entry(key).or_default();
            }
        } else if vault(old).is_some() {
            self.trial.vaults.remove(&key);
        }
    }

    pub fn trial_spawner(&self, p: IVec3) -> Option<&spawner::TrialSpawner> {
        self.trial.spawners.get(&p.to_array())
    }

    pub fn trial_spawner_mut(&mut self, p: IVec3) -> Option<&mut spawner::TrialSpawner> {
        self.trial.spawners.get_mut(&p.to_array())
    }

    pub fn trial_vault(&self, p: IVec3) -> Option<&vault::Vault> {
        self.trial.vaults.get(&p.to_array())
    }

    /// Sets which configuration a trial spawner uses (structure
    /// generation, `--place` scenes and tests). Returns whether one is there.
    pub fn set_trial_spawner_config(&mut self, p: IVec3, config: &str) -> bool {
        let Some(id) = spawner::config_id(config) else { return false };
        let Some(s) = self.trial.spawners.get_mut(&p.to_array()) else { return false };
        s.config = Some(id);
        true
    }

    /// `trial` level property: every trial spawner and vault.
    pub fn trial_to_string(&self) -> String {
        let spawners: Vec<Value> = self.trial.spawners.iter().map(|(p, s)| json!([p, s.save()])).collect();
        let vaults: Vec<Value> = self.trial.vaults.iter().map(|(p, v)| json!([p, v.save()])).collect();
        json!({"version": 1, "spawners": spawners, "vaults": vaults}).to_string()
    }

    pub fn load_trial(&mut self, text: &str) {
        let Ok(v) = serde_json::from_str::<Value>(text) else { return };
        let pos = |v: &Value| -> Option<[i32; 3]> {
            let a = v.as_array().filter(|a| a.len() == 3)?;
            let mut out = [0; 3];
            for (o, n) in out.iter_mut().zip(a) {
                *o = i32::try_from(n.as_i64()?).ok()?;
            }
            Some(out)
        };
        for e in v["spawners"].as_array().into_iter().flatten() {
            if let (Some(p), Some(s)) = (pos(&e[0]), spawner::TrialSpawner::load(&e[1])) {
                self.trial.spawners.insert(p, s);
            }
        }
        for e in v["vaults"].as_array().into_iter().flatten() {
            if let (Some(p), Some(s)) = (pos(&e[0]), vault::Vault::load(&e[1])) {
                self.trial.vaults.insert(p, s);
            }
        }
    }

    /// Installs a generated trial spawner's configuration. Saved entries
    /// win, as for loot chests.
    pub(super) fn register_trial_spawner(&mut self, p: IVec3, config: u8) {
        let key = p.to_array();
        let entry = self.trial.spawners.entry(key).or_default();
        if entry.config.is_none() && !self.trial.generated.contains(&key) {
            entry.config = Some(config);
        }
        self.trial.generated.insert(key);
    }

    pub(super) fn register_trial_vault(&mut self, p: IVec3) {
        self.trial.vaults.entry(p.to_array()).or_default();
    }
}

/// Stack save helpers shared by the block entities.
pub(crate) fn stack_json(s: &Stack) -> Value {
    json!(crate::inventory::stack_to_string(Some(*s)))
}

pub(crate) fn stack_from_json(v: &Value) -> Option<Stack> {
    v.as_str().and_then(|s| crate::inventory::stack_from_str(s).flatten())
}
