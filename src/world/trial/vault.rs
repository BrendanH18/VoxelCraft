//! Vault block entities (Java 26.3 `VaultBlockEntity`, `VaultServerData`,
//! `VaultSharedData`, `VaultConfig`): per-player rewards unlocked with
//! trial keys. The block state carries `vault_state` and `ominous`;
//! ticking with players and item ejection runs in `entity::trial`.
use glam::IVec3;
use serde_json::{Value, json};

use super::{OMINOUS_TRIAL_KEY, TRIAL_KEY, VaultState, stack_from_json, stack_json, vault, vault_block};
use crate::inventory::Stack;

/// `VaultConfig.DEFAULT` activation and deactivation ranges.
pub const ACTIVATION_RANGE: f64 = 4.0;
pub const DEACTIVATION_RANGE: f64 = 4.5;
/// `VaultBlockEntity.Server` constants.
pub const UNLOCKING_DELAY: i64 = 14;
pub const DISPLAY_CYCLE: i64 = 20;
pub const INSERT_FAIL_SOUND_BUFFER: i64 = 15;
/// `VaultState` constants.
pub const UPDATE_PLAYERS_RATE: i64 = 20;
pub const DELAY_BETWEEN_EJECTIONS: i64 = 20;
pub const DELAY_AFTER_LAST_EJECTION: i64 = 20;
pub const DELAY_BEFORE_FIRST_EJECTION: i64 = 20;
/// `VaultServerData.MAX_REWARD_PLAYERS`.
pub const MAX_REWARD_PLAYERS: usize = 128;

/// The two `VaultConfig`s trial chambers use (`trial_chambers/reward/*`).
pub fn loot_table(ominous: bool) -> &'static str {
    if ominous { "trial_chambers/reward_ominous" } else { "trial_chambers/reward" }
}

pub fn key_item(ominous: bool) -> crate::item::Item {
    if ominous { OMINOUS_TRIAL_KEY } else { TRIAL_KEY }
}

/// Server and shared vault data.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Vault {
    /// `rewardedPlayers`, oldest first.
    pub rewarded: Vec<u32>,
    pub resumes_at: i64,
    pub items_to_eject: Vec<Stack>,
    pub total_ejections: u32,
    pub connected: Vec<u32>,
    pub display: Option<Stack>,
    pub last_insert_fail: i64,
}

impl Vault {
    /// `VaultServerData.addToRewardedPlayers`: the oldest falls out past 128.
    pub fn reward(&mut self, player: u32) {
        if !self.rewarded.contains(&player) {
            self.rewarded.push(player);
        }
        while self.rewarded.len() > MAX_REWARD_PLAYERS {
            self.rewarded.remove(0);
        }
    }

    /// `ejectionProgress`: 0 at the first item, 1 at the last.
    pub fn ejection_progress(&self) -> f32 {
        if self.total_ejections == 1 {
            1.0
        } else {
            1.0 - (self.items_to_eject.len() as f32 / self.total_ejections.max(1) as f32)
        }
    }

    pub fn save(&self) -> Value {
        json!({
            "rewarded_players": self.rewarded,
            "state_updating_resumes_at": self.resumes_at,
            "items_to_eject": self.items_to_eject.iter().map(stack_json).collect::<Vec<_>>(),
            "total_ejections_needed": self.total_ejections,
            "connected_players": self.connected,
            "display_item": self.display.as_ref().map(stack_json),
            "last_insert_fail_timestamp": self.last_insert_fail,
        })
    }

    pub fn load(v: &Value) -> Option<Self> {
        if !v.is_object() {
            return None;
        }
        let ids = |v: &Value| -> Vec<u32> {
            v.as_array().into_iter().flatten().filter_map(|n| n.as_u64().and_then(|n| u32::try_from(n).ok())).collect()
        };
        Some(Self {
            rewarded: ids(&v["rewarded_players"]),
            resumes_at: v["state_updating_resumes_at"].as_i64().unwrap_or(0),
            items_to_eject: v["items_to_eject"].as_array().into_iter().flatten().filter_map(stack_from_json).collect(),
            total_ejections: v["total_ejections_needed"].as_u64().and_then(|n| u32::try_from(n).ok()).unwrap_or(0),
            connected: ids(&v["connected_players"]),
            display: stack_from_json(&v["display_item"]),
            last_insert_fail: v["last_insert_fail_timestamp"].as_i64().unwrap_or(0),
        })
    }
}

/// What using an item on a vault did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VaultUse {
    /// Not a vault, or not active: the click passes on.
    Pass,
    /// `VAULT_INSERT_ITEM_FAIL` (wrong item) or, with `rewarded`,
    /// `VAULT_REJECT_REWARDED_PLAYER`; `sound` is false inside the
    /// 15-tick buffer.
    Rejected { rewarded: bool, sound: bool },
    /// The key went in: take this many of it (unless Creative).
    Unlocked { consumed: u8 },
}

impl super::super::World {
    /// `VaultBlockEntity.Server.tryInsertKey` for `player` holding `held`.
    pub fn use_vault(&mut self, p: IVec3, player: u32, held: Option<Stack>) -> VaultUse {
        let Some(block) = self.get_block(p) else { return VaultUse::Pass };
        let Some((facing, state, ominous)) = vault(block) else { return VaultUse::Pass };
        // `canEjectReward`: an active vault with a loot table.
        if state != VaultState::Active {
            return VaultUse::Pass;
        }
        let now = self.maps.game_ticks as i64;
        let Some(v) = self.trial.vaults.get_mut(&p.to_array()) else { return VaultUse::Pass };
        let fail = |v: &mut Vault, rewarded: bool| {
            // `playInsertFailSound`: at most once per 15 ticks.
            let sound = now >= v.last_insert_fail + INSERT_FAIL_SOUND_BUFFER;
            if sound {
                v.last_insert_fail = now;
            }
            VaultUse::Rejected { rewarded, sound }
        };
        let key = key_item(ominous);
        // `isValidToInsert`: the same item with the default components and
        // at least the configured count (one).
        let valid = held.is_some_and(|s| s.item == key && s.count >= 1 && s.enchants.is_empty());
        if !valid {
            return fail(v, false);
        }
        if v.rewarded.contains(&player) {
            return fail(v, true);
        }
        let items = self.loot_stacks(loot_table(ominous), p);
        if items.is_empty() {
            return VaultUse::Pass;
        }
        let v = self.trial.vaults.get_mut(&p.to_array()).expect("vault entry");
        // `unlock`: queue the items, show the first, pause 14 ticks.
        v.total_ejections = items.len() as u32;
        v.display = items.last().copied();
        v.items_to_eject = items;
        v.resumes_at = now + UNLOCKING_DELAY;
        v.reward(player);
        // `updateConnectedPlayersWithinRange` follows; the next tick refreshes it.
        v.connected.retain(|&c| c != player);
        self.edit(p, vault_block(facing, VaultState::Unlocking, ominous), false);
        VaultUse::Unlocked { consumed: 1 }
    }
}
