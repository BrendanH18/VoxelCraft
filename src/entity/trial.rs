//! Trial spawners and vaults at work (Java 26.3 `TrialSpawner.tickServer`,
//! `TrialSpawnerState.tickAndGetNext`, `TrialSpawnerStateData`,
//! `VaultBlockEntity.Server.tick`, `VaultState.tickAndGetNext`) and the
//! ominous item spawner. The block entities live in `world::trial`; this
//! side owns the mobs a spawner makes and the items it ejects.
use glam::{DVec3, IVec3};

use super::{Entities, EntityEvent, MobSound, MobWorld, PlayerId, Rng};
use crate::inventory::Stack;
use crate::world::World;
use crate::world::trial::spawner::{self as ts, TrialSpawner};
use crate::world::trial::vault::{self as tv, Vault};
use crate::world::trial::{SpawnerState, VaultState, spawner, spawner_block, vault, vault_block};

type Cue = super::breeze::Cue;

/// A player as trial spawners and vaults see them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrialPlayer {
    pub id: PlayerId,
    pub pos: DVec3,
    pub eye: DVec3,
    /// `PlayerDetector.NO_CREATIVE_PLAYERS`: alive, not Creative or Spectator.
    pub counts: bool,
    pub bad_omen: Option<u8>,
    pub trial_omen: bool,
}

/// Gamerules and world settings a spawner reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrialRules {
    /// `spawner_blocks_work`.
    pub spawner_blocks: bool,
    /// `spawn_monsters` (Java's `SPAWN_MOBS` check in `canSpawnInLevel`).
    pub spawn_mobs: bool,
    pub peaceful: bool,
}

/// What a tick did that the game applies to players.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrialOutcome {
    /// `transformBadOmenIntoTrialOmen`: remove Bad Omen, give Trial Omen
    /// for this many ticks.
    TrialOmen { player: PlayerId, ticks: u32 },
}

/// An ominous item spawner (Java `OminousItemSpawner`): hovers with its
/// item for 60–119 ticks, then drops or fires it.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemSpawner {
    pub pos: DVec3,
    pub item: Stack,
    pub age: u32,
    pub lifetime: u32,
}

impl ItemSpawner {
    pub fn save(&self) -> serde_json::Value {
        serde_json::json!({"pos": self.pos.to_array(), "item": crate::world::trial::stack_json(&self.item), "age": self.age, "lifetime": self.lifetime})
    }
    pub fn load(v: &serde_json::Value) -> Option<Self> {
        let p = v["pos"].as_array()?;
        let pos = DVec3::new(p.first()?.as_f64()?, p.get(1)?.as_f64()?, p.get(2)?.as_f64()?);
        Some(Self {
            pos,
            item: crate::world::trial::stack_from_json(&v["item"])?,
            age: v["age"].as_u64().unwrap_or(0) as u32,
            lifetime: v["lifetime"].as_u64().unwrap_or(60).clamp(1, 120) as u32,
        })
    }
}

/// `loot::Random` over the entity RNG, for equipment and dispensing rolls.
struct LootRng<'a>(&'a mut Rng);
impl crate::world::loot::Random for LootRng<'_> {
    fn bounded(&mut self, n: u32) -> u32 {
        self.0.next_int(n.max(1))
    }
    fn boolean(&mut self) -> bool {
        self.0.next_int(2) == 1
    }
    fn float(&mut self) -> f32 {
        self.0.next_f32()
    }
}

/// `TrialSpawner.inLineOfSight` from the spawner's centre: its own cell
/// doesn't block the view, so the line starts where it leaves the cell.
fn sight(world: &World, center: DVec3, to: DVec3) -> bool {
    let d = to - center;
    if d.length_squared() < 0.76 {
        return true;
    }
    super::clear_line(world, center + d.normalize() * 0.87, to)
}

/// `BlockPos.asLong`, for the 20-tick scan phase.
fn as_long(p: IVec3) -> i64 {
    ((i64::from(p.x) & 0x3FF_FFFF) << 38) | ((i64::from(p.z) & 0x3FF_FFFF) << 12) | (i64::from(p.y) & 0xFFF)
}

fn weighted<T: Copy>(rng: &mut Rng, items: &[(T, u32)]) -> Option<T> {
    let total: u32 = items.iter().map(|i| i.1).sum();
    if total == 0 {
        return None;
    }
    let mut roll = rng.next_int(total);
    items.iter().find_map(|&(t, w)| {
        if roll < w {
            Some(t)
        } else {
            roll -= w;
            None
        }
    })
}

impl Entities {
    /// One server tick of every loaded trial spawner and vault, then the
    /// ominous item spawners. Returns effects for the game to apply.
    pub fn tick_trial(&mut self, world: &mut World, players: &[TrialPlayer], rules: TrialRules) -> Vec<TrialOutcome> {
        let mut out = Vec::new();
        let now = world.maps.game_ticks.min(i64::MAX as u64) as i64;
        let keys: Vec<[i32; 3]> = world.trial.spawners.keys().copied().collect();
        for key in keys {
            let p = IVec3::from_array(key);
            if !world.is_block_ticking(p) {
                continue;
            }
            let Some((state, ominous)) = world.get_block(p).and_then(spawner) else { continue };
            let Some(mut data) = world.trial.spawners.get(&key).cloned() else { continue };
            let mut ominous = ominous;
            let next = self.tick_spawner(world, p, now, state, &mut ominous, &mut data, players, rules, &mut out);
            world.trial.spawners.insert(key, data);
            let block = spawner_block(next, ominous);
            if world.get_block(p) != Some(block) {
                world.edit(p, block, false);
            }
        }
        let keys: Vec<[i32; 3]> = world.trial.vaults.keys().copied().collect();
        for key in keys {
            let p = IVec3::from_array(key);
            if !world.is_block_ticking(p) {
                continue;
            }
            self.tick_vault(world, p, now, players);
        }
        self.tick_item_spawners(world);
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn tick_spawner(
        &mut self,
        world: &mut World,
        p: IVec3,
        now: i64,
        state: SpawnerState,
        ominous: &mut bool,
        d: &mut TrialSpawner,
        players: &[TrialPlayer],
        rules: TrialRules,
        out: &mut Vec<TrialOutcome>,
    ) -> SpawnerState {
        let center = p.as_dvec3() + DVec3::splat(0.5);
        // `tickServer`: forget mobs that died, unloaded or wandered off.
        let before = d.current_mobs.len();
        let reach = i64::from(ts::MOB_TRACKING_DISTANCE).pow(2);
        d.current_mobs.retain(|uid| {
            self.mobs.iter().any(|m| {
                m.uid == *uid && m.alive() && {
                    let b = m.pos.floor().as_ivec3() - p;
                    i64::from(b.x).pow(2) + i64::from(b.y).pow(2) + i64::from(b.z).pow(2) <= reach
                }
            })
        });
        if d.current_mobs.len() != before {
            d.next_mob_spawns_at = now + d.config(*ominous).ticks_between_spawn;
        }
        let can_spawn = rules.spawner_blocks && !rules.peaceful && rules.spawn_mobs;
        match state {
            SpawnerState::Inactive => {
                if self.next_spawn(d, *ominous).is_some() {
                    SpawnerState::WaitingForPlayers
                } else {
                    state
                }
            }
            SpawnerState::WaitingForPlayers => {
                if !can_spawn {
                    d.reset_statistics();
                    return state;
                }
                if self.next_spawn(d, *ominous).is_none() {
                    return SpawnerState::Inactive;
                }
                self.detect(world, p, now, state, ominous, d, players, out);
                if d.detected.is_empty() { state } else { SpawnerState::Active }
            }
            SpawnerState::Active => {
                if !can_spawn {
                    d.reset_statistics();
                    return SpawnerState::WaitingForPlayers;
                }
                if self.next_spawn(d, *ominous).is_none() {
                    return SpawnerState::Inactive;
                }
                let additional = d.additional_players();
                self.detect(world, p, now, state, ominous, d, players, out);
                if *ominous {
                    self.spawn_ominous_item_spawner(world, p, now, d, players);
                }
                let config = d.config(*ominous);
                if d.total_mobs_spawned >= config.target_total(additional) {
                    if d.current_mobs.is_empty() {
                        d.cooldown_ends_at = now + ts::COOLDOWN_TICKS;
                        d.total_mobs_spawned = 0;
                        d.next_mob_spawns_at = 0;
                        return SpawnerState::WaitingForRewardEjection;
                    }
                } else if now >= d.next_mob_spawns_at
                    && (d.current_mobs.len() as u32) < config.target_simultaneous(additional)
                    && let Some(uid) = self.spawn_trial_mob(world, p, d, *ominous)
                {
                    d.current_mobs.push(uid);
                    d.total_mobs_spawned += 1;
                    d.next_mob_spawns_at = now + config.ticks_between_spawn;
                    d.next_spawn = self.pick_potential(config);
                }
                state
            }
            SpawnerState::WaitingForRewardEjection => {
                // `isReadyToOpenShutter(level, 40, cooldownLength)`.
                if now as f32 >= (d.cooldown_ends_at - ts::COOLDOWN_TICKS) as f32 + ts::EJECT_DELAY {
                    self.cue(Cue::SpawnerOpen, center);
                    SpawnerState::EjectingReward
                } else {
                    state
                }
            }
            SpawnerState::EjectingReward => {
                // `isReadyToEjectItems(level, 30, cooldownLength)`.
                let since = now - (d.cooldown_ends_at - ts::COOLDOWN_TICKS);
                if (since as f32) % ts::TIME_BETWEEN_EJECTIONS as f32 != 0.0 {
                    return state;
                }
                if d.detected.is_empty() {
                    self.cue(Cue::SpawnerClose, center);
                    d.ejecting_loot = None;
                    return SpawnerState::Cooldown;
                }
                let config = d.config(*ominous);
                if d.ejecting_loot.is_none() {
                    let indexed: Vec<(u8, u32)> =
                        config.loot.iter().enumerate().map(|(i, &(_, w))| (i as u8, w)).collect();
                    d.ejecting_loot = weighted(&mut self.rng, &indexed);
                }
                if let Some(table) = d.ejecting_loot.and_then(|i| config.loot.get(i as usize)) {
                    self.eject_reward(world, p, table.0);
                }
                d.detected.remove(0);
                state
            }
            SpawnerState::Cooldown => {
                self.detect(world, p, now, state, ominous, d, players, out);
                if !d.detected.is_empty() {
                    d.total_mobs_spawned = 0;
                    d.next_mob_spawns_at = 0;
                    return SpawnerState::Active;
                }
                if now >= d.cooldown_ends_at {
                    // `removeOminous` and `reset`.
                    *ominous = false;
                    d.reset();
                    return SpawnerState::WaitingForPlayers;
                }
                state
            }
        }
    }

    /// `getOrCreateNextSpawnData`.
    fn next_spawn(&mut self, d: &mut TrialSpawner, ominous: bool) -> Option<ts::Potential> {
        let config = d.config(ominous);
        if d.next_spawn.is_none_or(|i| i as usize >= config.potentials.len()) {
            d.next_spawn = self.pick_potential(config);
        }
        d.next_spawn.and_then(|i| config.potentials.get(i as usize)).copied()
    }

    fn pick_potential(&mut self, config: &ts::Config) -> Option<u8> {
        let indexed: Vec<(u8, u32)> = config.potentials.iter().enumerate().map(|(i, p)| (i as u8, p.weight)).collect();
        weighted(&mut self.rng, &indexed)
    }

    fn cue(&mut self, cue: Cue, pos: DVec3) {
        self.pending_sounds.push(EntityEvent::Sound { sound: MobSound::Breeze(cue), pos });
    }

    /// `TrialSpawnerStateData.tryDetectPlayers`.
    #[allow(clippy::too_many_arguments)]
    fn detect(
        &mut self,
        world: &World,
        p: IVec3,
        now: i64,
        state: SpawnerState,
        ominous: &mut bool,
        d: &mut TrialSpawner,
        players: &[TrialPlayer],
        out: &mut Vec<TrialOutcome>,
    ) {
        if (as_long(p).wrapping_add(now)).rem_euclid(ts::SCAN_INTERVAL) != 0 {
            return;
        }
        if state == SpawnerState::Cooldown && *ominous {
            return;
        }
        let center = p.as_dvec3() + DVec3::splat(0.5);
        let range = i64::from(ts::PLAYER_RANGE);
        let near = |t: &&TrialPlayer| {
            let b = t.pos.floor().as_ivec3() - p;
            t.counts && i64::from(b.x).pow(2) + i64::from(b.y).pow(2) + i64::from(b.z).pow(2) < range * range
        };
        let sighted: Vec<&TrialPlayer> = players.iter().filter(near).filter(|t| sight(world, center, t.eye)).collect();
        let mut became_ominous = false;
        if !*ominous && !sighted.is_empty() {
            // `findPlayerWithOminousEffect`: Trial Omen first, else the last
            // player with Bad Omen.
            let found = sighted
                .iter()
                .find(|t| t.trial_omen)
                .map(|t| (t.id, None))
                .or_else(|| sighted.iter().rev().find_map(|t| t.bad_omen.map(|a| (t.id, Some(a)))));
            if let Some((player, bad)) = found {
                if let Some(amplifier) = bad {
                    out.push(TrialOutcome::TrialOmen {
                        player,
                        ticks: ts::TRIAL_OMEN_PER_LEVEL * (u32::from(amplifier) + 1),
                    });
                    self.cue(Cue::TrialOmen, center);
                }
                self.apply_ominous(p, now, ominous, d);
                became_ominous = true;
            }
        }
        if state == SpawnerState::Cooldown && !became_ominous {
            return;
        }
        let first = d.detected.is_empty();
        let found: Vec<PlayerId> = if first {
            sighted.iter().map(|t| t.id).collect()
        } else {
            players.iter().filter(near).map(|t| t.id).collect()
        };
        let mut added = false;
        for id in found {
            if !d.detected.contains(&id.0) {
                d.detected.push(id.0);
                added = true;
            }
        }
        if added {
            d.next_mob_spawns_at = d.next_mob_spawns_at.max(now + ts::DETECT_SPAWN_BUFFER);
            if !became_ominous {
                self.cue(Cue::SpawnerDetect, center);
                let mut flames = crate::particles::Burst::new(
                    if *ominous { crate::particles::Kind::SoulFlame } else { crate::particles::Kind::Flame },
                    center,
                    (d.detected.len() as u16 * 10).min(60),
                );
                flames.spread = DVec3::splat(0.4);
                self.particles.push(crate::particles::Request::Burst(flames));
            }
        }
    }

    /// `TrialSpawner.applyOminous` and `resetAfterBecomingOminous`.
    fn apply_ominous(&mut self, p: IVec3, now: i64, ominous: &mut bool, d: &mut TrialSpawner) {
        *ominous = true;
        self.cue(Cue::SpawnerOminous, p.as_dvec3() + DVec3::splat(0.5));
        // The current mobs vanish in a puff of flame, keeping their gear.
        let gone = std::mem::take(&mut d.current_mobs);
        for uid in gone {
            if let Some(i) = self.mobs.iter().position(|m| m.uid == uid) {
                let pos = self.mobs[i].pos;
                let mut puff = crate::particles::Burst::new(crate::particles::Kind::Flame, pos + DVec3::Y, 12);
                puff.spread = DVec3::new(0.4, 0.8, 0.4);
                self.particles.push(crate::particles::Request::Burst(puff));
                self.mobs.swap_remove(i);
            }
        }
        let config = d.config(true);
        if !config.potentials.is_empty() {
            d.next_spawn = None;
        }
        d.total_mobs_spawned = 0;
        d.next_mob_spawns_at = now + config.ticks_between_spawn;
        d.cooldown_ends_at = now + ts::TICKS_BETWEEN_ITEM_SPAWNERS;
    }

    /// `TrialSpawner.spawnMob`. Returns the new mob's uid.
    fn spawn_trial_mob(&mut self, world: &World, p: IVec3, d: &mut TrialSpawner, ominous: bool) -> Option<u32> {
        let potential = self.next_spawn(d, ominous)?;
        let config = d.config(ominous);
        let range = f64::from(config.spawn_range);
        let r = &mut self.rng;
        let mut next = || f64::from(r.next_f32());
        let x = f64::from(p.x) + (next() - next()) * range + 0.5;
        let y = f64::from(p.y + r.next_int(3) as i32 - 1);
        let mut next = || f64::from(r.next_f32());
        let z = f64::from(p.z) + (next() - next()) * range + 0.5;
        let pos = DVec3::new(x, y, z);
        let shape = potential.kind.shape();
        if !world.loaded(pos.floor().as_ivec3())
            || crate::physics::overlaps_solid(world, pos, shape)
            || crate::physics::is_fluid_at(world, pos)
        {
            return None;
        }
        let center = p.as_dvec3() + DVec3::splat(0.5);
        if !sight(world, center, pos + DVec3::Y * 0.5) {
            return None;
        }
        let i = self.mobs.len();
        if potential.finalize {
            self.spawn(potential.kind, pos);
        } else {
            let yaw = self.rng.range(0.0, std::f32::consts::TAU);
            let mut mob = super::Mob::new(potential.kind, pos, yaw);
            mob.baby = potential.baby;
            if let Some(size) = potential.size {
                // Java's `Size` tag plus one; sizes above 2 clamp here.
                mob.set_size(size);
            }
            self.mobs.push(mob);
        }
        let m = &mut self.mobs[i];
        m.yaw = self.rng.range(0.0, std::f32::consts::TAU);
        m.persistent = true;
        if let Some(table) = potential.equipment {
            equip(&mut self.mobs[i], table, &mut self.rng);
        }
        self.ensure_mob_ids();
        let uid = self.mobs[i].uid;
        self.cue(Cue::SpawnerSpawn, center);
        let kind = if ominous { crate::particles::Kind::SoulFlame } else { crate::particles::Kind::Flame };
        for at in [center, pos + DVec3::Y * 0.5] {
            let mut flames = crate::particles::Burst::new(kind, at, 20);
            flames.spread = DVec3::splat(0.5);
            self.particles.push(crate::particles::Request::Burst(flames));
        }
        Some(uid)
    }

    /// `TrialSpawner.ejectReward`: each stack pops up out of the top.
    fn eject_reward(&mut self, world: &mut World, p: IVec3, table: &str) {
        let items = world.loot_stacks(table, p);
        let from = p.as_dvec3() + DVec3::new(0.5, 1.2, 0.5);
        for stack in items {
            self.eject_up(stack, from);
        }
        self.cue(Cue::SpawnerEject, from);
    }

    /// `DefaultDispenseItemBehavior.spawnItem(level, stack, 2, UP, pos)`.
    fn eject_up(&mut self, stack: Stack, from: DVec3) {
        let speed = 2.0;
        let r = &mut self.rng;
        let mut tri = |c: f64, dev: f64| c + dev * (f64::from(r.next_f32()) - f64::from(r.next_f32()));
        let up = 0.2;
        let vel = DVec3::new(tri(0.0, 0.0172275 * speed), tri(up, 0.0172275 * speed), tri(0.0, 0.0172275 * speed));
        let item = super::ItemEntity::new(stack, from, vel * 20.0, super::item::PICKUP_DELAY, &mut self.rng);
        self.items.push(item);
    }

    /// `TrialSpawnerState.spawnOminousOminousItemSpawner`.
    fn spawn_ominous_item_spawner(
        &mut self,
        world: &mut World,
        p: IVec3,
        now: i64,
        d: &mut TrialSpawner,
        players: &[TrialPlayer],
    ) {
        if now < d.cooldown_ends_at {
            return;
        }
        // `getDispensingItems`: the ominous drop table, rolled per spawner.
        let config = d.config(true);
        let items =
            crate::world::loot::roll_items(config.items_to_drop_when_ominous, &mut LootRng(&mut self.rng), |_| None);
        let Some(&item) = items.get(self.rng.next_int(items.len().max(1) as u32) as usize) else { return };
        // `selectEntityToSpawnItemAbove`: a random current mob or a random
        // player in range, by a coin flip.
        let range = f64::from(ts::PLAYER_RANGE * ts::PLAYER_RANGE);
        let center = p.as_dvec3() + DVec3::splat(0.5);
        let mobs: Vec<(DVec3, f64)> = d
            .current_mobs
            .iter()
            .filter_map(|uid| self.mobs.iter().find(|m| m.uid == *uid && m.alive()))
            .filter(|m| m.pos.distance_squared(center) <= range)
            .map(|m| (m.pos, m.shape().height))
            .collect();
        let near: Vec<(DVec3, f64)> = players
            .iter()
            .filter(|t| d.detected.contains(&t.id.0) && t.counts && t.pos.distance_squared(center) <= range)
            .map(|t| (t.pos, t.eye.y - t.pos.y + 0.18))
            .collect();
        let pool = if self.rng.next_int(2) == 1 { &mobs } else { &near };
        if pool.is_empty() {
            return;
        }
        let (at, height) = pool[self.rng.next_int(pool.len() as u32) as usize];
        // `calculatePositionAbove`: up to height + 2 + nextInt(4), stopping
        // under a ceiling.
        let up = height + 2.0 + f64::from(self.rng.next_int(4));
        let mut top = at;
        let steps = (up / 0.25).ceil() as i32;
        for k in 1..=steps {
            let q = at + DVec3::Y * (f64::from(k) * 0.25).min(up);
            if world.get_block(q.floor().as_ivec3()).is_some_and(|b| b.is_solid()) {
                break;
            }
            top = q;
        }
        let spot = top.floor().as_ivec3();
        if world.get_block(spot).is_some_and(|b| b.is_solid()) {
            return;
        }
        let pos = spot.as_dvec3() + DVec3::new(0.5, 0.5, 0.5);
        let lifetime = 60 + self.rng.next_int(60);
        self.item_spawners.push(ItemSpawner { pos, item, age: 0, lifetime });
        self.cue(Cue::ItemSpawnerBegin, pos);
        d.cooldown_ends_at = now + ts::TICKS_BETWEEN_ITEM_SPAWNERS;
    }

    /// `OminousItemSpawner.tick`: at the end of its life it fires
    /// projectiles straight down and drops anything else.
    fn tick_item_spawners(&mut self, world: &World) {
        let mut done = Vec::new();
        for (i, s) in self.item_spawners.iter_mut().enumerate() {
            if !world.is_block_ticking(s.pos.floor().as_ivec3()) {
                continue;
            }
            s.age += 1;
            if s.age >= s.lifetime {
                done.push(i);
            }
        }
        for i in done.into_iter().rev() {
            let s = self.item_spawners.swap_remove(i);
            self.cue(Cue::ItemSpawnerDrop, s.pos);
            let down = DVec3::NEG_Y;
            let item = s.item.item;
            if let Some(kind) = super::thrown::Kind::from_item(item) {
                let mut thrown =
                    super::thrown::Thrown::launch(kind, PlayerId::HOST, s.pos, down, DVec3::ZERO, &mut self.rng);
                thrown.owner = None;
                self.thrown.push(thrown);
            } else if item == crate::item::Item::ARROW || item == crate::item::Item::TIPPED_ARROW {
                let mut arrow = super::Arrow::dispensed(s.pos, down * 1.1 * 20.0);
                arrow.potion = s.item.arrow_potion();
                arrow.tipped = arrow.potion.is_some();
                self.arrows.push(arrow);
            } else if item == crate::item::Item::FIRE_CHARGE {
                self.fireballs.push(super::fireball::Fireball::new(s.pos, down));
            } else if let Some(potion) = s.item.lingering_contents() {
                let mut thrown = super::potion::ThrownPotion::new(potion, None, s.pos, down * 10.0);
                thrown.lingering = true;
                self.potions.push(thrown);
            } else {
                for _ in 0..s.item.count {
                    let one = Stack { count: 1, ..s.item };
                    let item =
                        super::ItemEntity::new(one, s.pos, DVec3::ZERO, super::item::PICKUP_DELAY, &mut self.rng);
                    self.items.push(item);
                }
            }
        }
    }

    /// `VaultBlockEntity.Server.tick`.
    fn tick_vault(&mut self, world: &mut World, p: IVec3, now: i64, players: &[TrialPlayer]) {
        let Some((facing, state, ominous)) = world.get_block(p).and_then(vault) else { return };
        let Some(mut v) = world.trial.vaults.get(&p.to_array()).cloned() else { return };
        let center = p.as_dvec3() + DVec3::splat(0.5);
        // `shouldCycleDisplayItem`: every 20 ticks while active.
        if now % tv::DISPLAY_CYCLE == 0 && state == VaultState::Active {
            let items = crate::world::loot::roll_items(tv::loot_table(ominous), &mut LootRng(&mut self.rng), |_| None);
            v.display = items.get(self.rng.next_int(items.len().max(1) as u32) as usize).copied();
        }
        let mut next = state;
        if now >= v.resumes_at {
            next = match state {
                VaultState::Inactive | VaultState::Active => {
                    let range =
                        if state == VaultState::Inactive { tv::ACTIVATION_RANGE } else { tv::DEACTIVATION_RANGE };
                    update_connected(&mut v, now, center, range, players);
                    if v.connected.is_empty() { VaultState::Inactive } else { VaultState::Active }
                }
                VaultState::Unlocking => {
                    v.resumes_at = now + tv::DELAY_BEFORE_FIRST_EJECTION;
                    VaultState::Ejecting
                }
                VaultState::Ejecting => {
                    if v.items_to_eject.is_empty() {
                        v.total_ejections = 0;
                        update_connected(&mut v, now, center, tv::DEACTIVATION_RANGE, players);
                        if v.connected.is_empty() { VaultState::Inactive } else { VaultState::Active }
                    } else {
                        let progress = v.ejection_progress();
                        let stack = v.items_to_eject.pop().unwrap();
                        self.eject_up(stack, p.as_dvec3() + DVec3::new(0.5, 1.2, 0.5));
                        let _ = progress;
                        self.cue(Cue::VaultEject, center);
                        v.display = v.items_to_eject.last().copied();
                        let last = v.items_to_eject.is_empty();
                        v.resumes_at =
                            now + if last { tv::DELAY_AFTER_LAST_EJECTION } else { tv::DELAY_BETWEEN_EJECTIONS };
                        VaultState::Ejecting
                    }
                }
            };
        }
        if next != state {
            // `VaultState.onTransition` sounds and display resets.
            match next {
                VaultState::Active if state == VaultState::Inactive => {
                    self.cue(Cue::VaultActivate, center);
                    if v.display.is_none() {
                        let items = crate::world::loot::roll_items(
                            tv::loot_table(ominous),
                            &mut LootRng(&mut self.rng),
                            |_| None,
                        );
                        v.display = items.first().copied();
                    }
                }
                VaultState::Inactive => {
                    self.cue(Cue::VaultDeactivate, center);
                    v.display = None;
                }
                VaultState::Ejecting => self.cue(Cue::VaultOpen, center),
                _ => {}
            }
            if state == VaultState::Ejecting {
                self.cue(Cue::VaultClose, center);
            }
            world.edit(p, vault_block(facing, next, ominous), false);
        }
        world.trial.vaults.insert(p.to_array(), v);
    }

    /// Ominous item spawners for saves.
    pub fn item_spawners_to_value(&self) -> serde_json::Value {
        serde_json::Value::Array(self.item_spawners.iter().map(ItemSpawner::save).collect())
    }

    pub fn load_item_spawners(&mut self, v: &serde_json::Value) {
        self.item_spawners = v.as_array().into_iter().flatten().filter_map(ItemSpawner::load).collect();
    }
}

/// `VaultSharedData.updateConnectedPlayersWithinRange`, every 20 ticks.
fn update_connected(v: &mut Vault, now: i64, center: DVec3, range: f64, players: &[TrialPlayer]) {
    if now % tv::UPDATE_PLAYERS_RATE != 0 {
        return;
    }
    let p = center.floor().as_ivec3();
    v.connected = players
        .iter()
        .filter(|t| {
            let b = t.pos.floor().as_ivec3() - p;
            t.counts
                && f64::from(b.x).powi(2) + f64::from(b.y).powi(2) + f64::from(b.z).powi(2) < range * range
                && !v.rewarded.contains(&t.id.0)
        })
        .map(|t| t.id.0)
        .collect();
}

/// `Mob.equip(EquipmentTable)` with `equipment/trial_chamber_melee` or
/// `_ranged`, rolled as the tables are: one armor set (chainmail 4, iron 2,
/// diamond 1) whose helmet and chestplate each come with chance 0.5,
/// enchanted Protection, Projectile and Fire Protection IV (drawn as a
/// glint), then the weapon pool. Swords are rolled but not yet carried by
/// zombies here (recorded gap); bows go to skeleton-family mobs. Every
/// slot's drop chance is 0.
fn equip(m: &mut super::Mob, table: &str, rng: &mut Rng) {
    use super::armor::ArmorKind;
    let set = weighted(rng, &[(ArmorKind::Chain, 4), (ArmorKind::Iron, 2), (ArmorKind::Diamond, 1)])
        .unwrap_or(ArmorKind::Chain);
    for slot in 0..2 {
        if rng.next_f32() < 0.5 {
            m.armor[slot] = Some(set);
            m.armor_glint |= 1 << slot;
        }
    }
    use crate::enchant::Enchantment as E;
    if table.ends_with("ranged") {
        let enchant = weighted(rng, &[(None, 2), (Some(E::Power), 1), (Some(E::Punch), 1)]).flatten();
        if m.kind.is_bow_skeleton() {
            let mut bow = Stack::new(crate::item::Item::BOW, 1);
            if let Some(e) = enchant {
                bow.enchants.set(e, 1);
            }
            m.skeleton_bow = Some(Box::new(bow));
        }
    } else {
        let _sword = weighted(rng, &[(0u8, 4), (1, 1), (2, 1), (3, 1)]);
    }
    m.keeps_equipment = true;
}

/// For tests and scenes: the mobs a trial spawner currently tracks.
pub fn tracked(world: &World, p: IVec3) -> Vec<u32> {
    world.trial_spawner(p).map_or(Vec::new(), |s| s.current_mobs.clone())
}

#[cfg(test)]
mod tests;
