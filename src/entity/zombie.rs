//! Zombie reinforcements, leader zombies and random spawn attributes, as
//! Java 26.3 `Zombie.hurtServer`, `Zombie.handleAttributes`,
//! `Zombie.randomizeReinforcementsChance` (zero for `ZombifiedPiglin`) and
//! `Mob.finalizeSpawn`'s follow-range bonus do them. The constants are
//! checked against `tests/fixtures/java/26.3/zombie_attributes.json`, which
//! `tools/extract_java_zombie_attribute_fixtures.py` reads from the bytecode.
//!
//! The game has no general attribute system, so the rolled modifiers live in
//! [`Attrs`] and the few attributes they touch (`spawn_reinforcements`,
//! `max_health`, `follow_range`, `knockback_resistance`) are computed from
//! them with Java's operation order and ranges.
use super::{Ctx, Entities, Mob, MobKind, MobWorld, PlayerId, Rng, monster_spawns};
use crate::{physics, simulation::difficulty::Difficulty, world::terrain::Dimension};
use glam::{DVec3, IVec3};
use serde_json::{Value, json};

/// `Zombie.REINFORCEMENT_ATTEMPTS`.
pub const REINFORCEMENT_ATTEMPTS: u32 = 50;
/// `Zombie.REINFORCEMENT_RANGE_MIN` and `REINFORCEMENT_RANGE_MAX`: each axis
/// offset is `Mth.nextInt(7, 40) * Mth.nextInt(-1, 1)`.
pub const REINFORCEMENT_RANGE_MIN: i32 = 7;
pub const REINFORCEMENT_RANGE_MAX: i32 = 40;
/// `hasNearbyAlivePlayer(x, y, z, 7.0)` refuses a spot.
pub const REINFORCEMENT_PLAYER_CLEARANCE: f64 = 7.0;
/// `reinforcement_caller_charge`: the caller's cumulative `ADD_VALUE`.
pub const CALLER_CHARGE: f64 = -0.05;
/// `ZOMBIE_REINFORCEMENT_CALLEE_CHARGE` (`-0.05F` widened to a double).
pub const CALLEE_CHARGE: f64 = -0.05f32 as f64;
/// `randomizeReinforcementsChance`: base `nextDouble() * 0.1F`.
pub const REINFORCEMENT_BASE_SCALE: f64 = 0.1f32 as f64;
/// `handleAttributes`: knockback resistance `+ nextDouble() * 0.05F`.
pub const KNOCKBACK_BONUS_SCALE: f64 = 0.05f32 as f64;
/// `handleAttributes`: `nextDouble() * 1.5 * special`, kept above 1.
pub const FOLLOW_BONUS_SCALE: f64 = 1.5;
pub const FOLLOW_BONUS_THRESHOLD: f64 = 1.0;
/// `Zombie.ZOMBIE_LEADER_CHANCE`, times the special multiplier.
pub const LEADER_CHANCE: f32 = 0.05;
/// Leader `spawn_reinforcements` bonus `nextDouble() * 0.25 + 0.5`.
pub const LEADER_REINFORCEMENT_SCALE: f64 = 0.25;
pub const LEADER_REINFORCEMENT_OFFSET: f64 = 0.5;
/// Leader `max_health` `ADD_MULTIPLIED_TOTAL` of `nextDouble() * 3.0 + 1.0`.
pub const LEADER_HEALTH_SCALE: f64 = 3.0;
pub const LEADER_HEALTH_OFFSET: f64 = 1.0;
/// `Zombie.BREAK_DOOR_CHANCE`, times the special multiplier.
pub const BREAK_DOOR_CHANCE: f32 = 0.1;
/// `Mob.finalizeSpawn`: follow range `ADD_MULTIPLIED_BASE triangle(0, 0.11485)`.
pub const RANDOM_SPAWN_BONUS_SPREAD: f64 = 0.11485000000000001;
/// `Mob.createMobAttributes` and `Zombie.createAttributes` follow ranges.
pub const MOB_FOLLOW_RANGE: f64 = 16.0;
pub const ZOMBIE_FOLLOW_RANGE: f64 = 35.0;
/// `RangedAttribute` bounds of the attributes this module computes.
pub const SPAWN_REINFORCEMENTS_RANGE: (f64, f64) = (0.0, 1.0);
pub const MAX_HEALTH_RANGE: (f64, f64) = (1.0, 1024.0);
pub const FOLLOW_RANGE_RANGE: (f64, f64) = (0.0, 2048.0);
pub const KNOCKBACK_RESISTANCE_RANGE: (f64, f64) = (-2.0, 1.0);

/// The kinds whose Java classes extend `Zombie` and so carry the
/// `spawn_reinforcements` attribute: zombies, husks, drowned, zombie
/// villagers and zombified piglins.
pub fn calls_reinforcements(kind: MobKind) -> bool {
    kind.is_zombie() || kind == MobKind::ZombifiedPiglin
}

/// Java's `RandomSource.nextDouble`: 53 random bits.
fn next_double(rng: &mut Rng) -> f64 {
    (rng.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
}

/// The attribute modifiers a mob rolled when it spawned, and the
/// reinforcement charges it has collected. A conversion that changes the
/// kind in place leaves them stale ([`Attrs::kind`] no longer matches), and
/// stale values read as none: Java's `convertTo` makes a fresh entity.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Attrs {
    /// The kind these belong to.
    pub kind: Option<MobKind>,
    /// `random_spawn_bonus` on follow range (`ADD_MULTIPLIED_BASE`).
    pub follow_spawn_bonus: f64,
    /// `zombie_random_spawn_bonus` on follow range (`ADD_MULTIPLIED_TOTAL`, 0 when absent).
    pub zombie_follow_bonus: f64,
    /// `random_spawn_bonus` on knockback resistance (`ADD_VALUE`).
    pub knockback_bonus: f64,
    /// `spawn_reinforcements` base value.
    pub reinforcement_base: f64,
    /// `reinforcement_caller_charge` (cumulative, never positive).
    pub caller_charge: f64,
    /// `reinforcement_callee_charge` is present.
    pub callee_charge: bool,
    /// `leader_zombie_bonus` on `spawn_reinforcements` (0 unless a leader).
    pub leader_reinforcement: f64,
    /// `leader_zombie_bonus` on max health (`ADD_MULTIPLIED_TOTAL`, 0 unless a leader).
    pub leader_health: f64,
    /// Java's `CanBreakDoors` (saved; the door-breaking goal is not built yet).
    pub can_break_doors: bool,
}

impl Attrs {
    pub fn for_kind(kind: MobKind) -> Self {
        Self { kind: Some(kind), ..Default::default() }
    }

    pub(crate) fn save(&self) -> Value {
        json!({
            "kind": self.kind.map(MobKind::name),
            "follow_spawn_bonus": self.follow_spawn_bonus,
            "zombie_follow_bonus": self.zombie_follow_bonus,
            "knockback_bonus": self.knockback_bonus,
            "reinforcement_base": self.reinforcement_base,
            "caller_charge": self.caller_charge,
            "callee_charge": self.callee_charge,
            "leader_reinforcement": self.leader_reinforcement,
            "leader_health": self.leader_health,
            "can_break_doors": self.can_break_doors,
        })
    }

    /// Reads a saved record; anything missing, non-finite or out of the
    /// range Java could roll falls back to none.
    pub(crate) fn load(v: &Value) -> Self {
        let Some(kind) = v["kind"].as_str().and_then(MobKind::from_name) else { return Self::default() };
        let num = |key: &str, lo: f64, hi: f64| v[key].as_f64().filter(|x| x.is_finite() && (lo..=hi).contains(x));
        let leader = num("leader_health", LEADER_HEALTH_OFFSET, LEADER_HEALTH_OFFSET + LEADER_HEALTH_SCALE);
        Self {
            kind: Some(kind),
            follow_spawn_bonus: num("follow_spawn_bonus", -RANDOM_SPAWN_BONUS_SPREAD, RANDOM_SPAWN_BONUS_SPREAD)
                .unwrap_or(0.0),
            zombie_follow_bonus: num("zombie_follow_bonus", FOLLOW_BONUS_THRESHOLD, FOLLOW_BONUS_SCALE)
                .filter(|x| *x > FOLLOW_BONUS_THRESHOLD)
                .unwrap_or(0.0),
            knockback_bonus: num("knockback_bonus", 0.0, KNOCKBACK_BONUS_SCALE).unwrap_or(0.0),
            reinforcement_base: num("reinforcement_base", 0.0, REINFORCEMENT_BASE_SCALE).unwrap_or(0.0),
            caller_charge: num("caller_charge", -1e6, 0.0).unwrap_or(0.0),
            callee_charge: v["callee_charge"] == true,
            leader_reinforcement: num(
                "leader_reinforcement",
                LEADER_REINFORCEMENT_OFFSET,
                LEADER_REINFORCEMENT_OFFSET + LEADER_REINFORCEMENT_SCALE,
            )
            .unwrap_or(0.0),
            leader_health: leader.unwrap_or(0.0),
            can_break_doors: v["can_break_doors"] == true,
        }
    }
}

/// `Mob.finalizeSpawn`'s follow-range bonus and, for the zombie family,
/// `Zombie.finalizeSpawn`'s door roll and `handleAttributes`. `heal_leader`
/// is false for `CONVERSION`, `LOAD` and `DIMENSION_TRAVEL`.
pub(super) fn finalize_spawn(mob: &mut Mob, special: f32, heal_leader: bool, rng: &mut Rng) {
    mob.attrs = Attrs::for_kind(mob.kind);
    mob.attrs.follow_spawn_bonus = RANDOM_SPAWN_BONUS_SPREAD * (next_double(rng) - next_double(rng));
    if calls_reinforcements(mob.kind) {
        mob.attrs.can_break_doors = rng.next_f32() < special * BREAK_DOOR_CHANCE;
        mob.handle_zombie_attributes(special, heal_leader, rng);
    }
}

impl Mob {
    /// The rolled modifiers, or none when they belong to a kind this mob
    /// has since converted from.
    pub fn attributes(&self) -> Attrs {
        if self.attrs.kind == Some(self.kind) { self.attrs } else { Attrs::default() }
    }

    fn attrs_mut(&mut self) -> &mut Attrs {
        if self.attrs.kind != Some(self.kind) {
            self.attrs = Attrs::for_kind(self.kind);
        }
        &mut self.attrs
    }

    /// `Zombie.handleAttributes(difficultyModifier, reason)`, in Java's roll order.
    pub(super) fn handle_zombie_attributes(&mut self, special: f32, heal_leader: bool, rng: &mut Rng) {
        let piglin = self.kind == MobKind::ZombifiedPiglin;
        let a = self.attrs_mut();
        a.reinforcement_base = if piglin { 0.0 } else { next_double(rng) * REINFORCEMENT_BASE_SCALE };
        a.knockback_bonus = next_double(rng) * KNOCKBACK_BONUS_SCALE;
        let follow = next_double(rng) * FOLLOW_BONUS_SCALE * f64::from(special);
        if follow > FOLLOW_BONUS_THRESHOLD {
            a.zombie_follow_bonus = follow;
        }
        if rng.next_f32() < special * LEADER_CHANCE {
            a.leader_reinforcement = next_double(rng) * LEADER_REINFORCEMENT_SCALE + LEADER_REINFORCEMENT_OFFSET;
            a.leader_health = next_double(rng) * LEADER_HEALTH_SCALE + LEADER_HEALTH_OFFSET;
            a.can_break_doors = true;
            if heal_leader {
                self.health = self.max_health();
            }
        }
    }

    /// A leader zombie (`leader_zombie_bonus` present).
    pub fn is_leader(&self) -> bool {
        self.attributes().leader_health > 0.0
    }

    /// `getAttributeValue(SPAWN_REINFORCEMENTS_CHANCE)`.
    pub fn reinforcement_chance(&self) -> f64 {
        if !calls_reinforcements(self.kind) {
            return 0.0;
        }
        let a = self.attributes();
        let callee = if a.callee_charge { CALLEE_CHARGE } else { 0.0 };
        let (lo, hi) = SPAWN_REINFORCEMENTS_RANGE;
        (a.reinforcement_base + a.caller_charge + callee + a.leader_reinforcement).clamp(lo, hi)
    }

    /// `getAttributeValue(FOLLOW_RANGE)`: the kind's base, then the spawn
    /// bonus on the base, then the zombie bonus on the total.
    pub fn follow_range(&self) -> f64 {
        let a = self.attributes();
        let base = self.kind.follow_range_base();
        let (lo, hi) = FOLLOW_RANGE_RANGE;
        ((base + base * a.follow_spawn_bonus) * (1.0 + a.zombie_follow_bonus)).clamp(lo, hi)
    }

    /// Max health from the kind's base and the leader bonus.
    pub(super) fn leader_max_health(&self, base: f32) -> f32 {
        let (lo, hi) = MAX_HEALTH_RANGE;
        (f64::from(base) * (1.0 + self.attributes().leader_health)).clamp(lo, hi) as f32
    }

    /// The zombie's target as `NearestAttackableTargetGoal` and a
    /// reinforcement's `setTarget` leave it: the player it was sent after
    /// while that player stays in follow range, else the nearest detectable
    /// player in follow range.
    pub(super) fn zombie_target<'a>(&mut self, ctx: &'a Ctx) -> Option<&'a super::Target> {
        let range = self.follow_range();
        if let Some(id) = self.reinforce_target {
            let kept = ctx
                .players
                .iter()
                .find(|t| t.id == id && t.targetable && t.alive && t.pos.distance_squared(self.pos) <= range * range);
            if kept.is_some() {
                return kept;
            }
            self.reinforce_target = None;
        }
        ctx.nearest_detectable_target(self.pos, range)
    }
}

/// Whom a hurt zombie calls help against.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Foe {
    Player(PlayerId),
    /// A villager, golem or turtle it is hunting; reinforcements come
    /// without a player target.
    Other,
}

impl Entities {
    /// `Zombie.hurtServer`'s reinforcement call for every zombie hurt since
    /// the last update.
    pub(super) fn call_reinforcements<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx) {
        let mut i = 0;
        while i < self.mobs.len() {
            if let Some(attacker) = self.mobs[i].reinforce_call.take() {
                self.reinforce(i, attacker, world, ctx);
            }
            i += 1;
        }
    }

    /// One reinforcement call by mob `index`, hurt by `attacker` if a
    /// player's melee is known. Returns whether a reinforcement spawned.
    fn reinforce<W: MobWorld + ?Sized>(
        &mut self,
        index: usize,
        attacker: Option<PlayerId>,
        world: &W,
        ctx: &Ctx,
    ) -> bool {
        let m = &self.mobs[index];
        if !calls_reinforcements(m.kind) {
            return false;
        }
        // `getTarget()`, else a living attacker.
        let range = m.follow_range();
        let in_range = |t: &&super::Target| t.targetable && t.alive && t.pos.distance_squared(m.pos) <= range * range;
        let current =
            m.reinforce_target.filter(|id| ctx.players.iter().filter(in_range).any(|t| t.id == *id)).or_else(|| {
                (m.ai == super::mob::Ai::Chase)
                    .then(|| ctx.nearest_detectable_target(m.pos, range))
                    .flatten()
                    .map(|t| t.id)
            });
        let player_hit = (m.player_hit_left > 0.0)
            .then(|| {
                ctx.players
                    .iter()
                    .filter(|t| t.targetable && t.alive)
                    .min_by(|a, b| a.pos.distance_squared(m.pos).total_cmp(&b.pos.distance_squared(m.pos)))
                    .map(|t| t.id)
            })
            .flatten();
        let foe = match current.or(attacker).or(player_hit) {
            Some(id) => Foe::Player(id),
            None if m.hunt.is_some() => Foe::Other,
            None => return false,
        };
        if self.difficulty != Difficulty::Hard {
            return false;
        }
        if f64::from(self.rng.next_f32()) >= m.reinforcement_chance() {
            return false;
        }
        if !(ctx.spawning && self.monster_spawning) {
            return false;
        }
        let kind = m.kind;
        let origin = m.pos.floor().as_ivec3();
        let shape = kind.shape();
        let rules =
            monster_spawns::SpawnRules { dimension: ctx.dimension, light: ctx.spawn_light, moon: self.moon_brightness };
        let span = (REINFORCEMENT_RANGE_MAX - REINFORCEMENT_RANGE_MIN + 1) as u32;
        for _ in 0..REINFORCEMENT_ATTEMPTS {
            let mut offset = || {
                let size = REINFORCEMENT_RANGE_MIN + self.rng.next_int(span) as i32;
                size * (self.rng.next_int(3) as i32 - 1)
            };
            let (dx, dy, dz) = (offset(), offset(), offset());
            let cell = origin + IVec3::new(dx, dy, dz);
            if !spot_ok(world, kind, cell, rules, &mut self.rng) {
                continue;
            }
            // `setPos(x, y, z)`: the block corner, not its centre.
            let pos = cell.as_dvec3();
            let clearance = REINFORCEMENT_PLAYER_CLEARANCE * REINFORCEMENT_PLAYER_CLEARANCE;
            if ctx.players.iter().any(|t| t.alive && !t.spectator && t.pos.distance_squared(pos) < clearance) {
                continue;
            }
            if self.obstructed(ctx, pos, shape)
                || physics::overlaps_solid(world, pos, shape)
                || (kind != MobKind::Drowned
                    && physics::touches_block(world, pos, shape, |b| b.is_fluid() || b.is_waterlogged()))
            {
                continue;
            }
            self.spawn(kind, pos);
            let new = self.mobs.len() - 1;
            let added = &mut self.mobs[new];
            added.persistent = false;
            added.attrs_mut().callee_charge = true;
            if let Foe::Player(id) = foe {
                added.reinforce_target = Some(id);
            }
            self.mobs[index].attrs_mut().caller_charge += CALLER_CHARGE;
            return true;
        }
        false
    }

    /// `Level.isUnobstructed`: no living mob or player box overlaps.
    fn obstructed(&self, ctx: &Ctx, pos: DVec3, shape: physics::Shape) -> bool {
        let (lo, hi) = shape.aabb(pos);
        let overlaps = |(a, b): (DVec3, DVec3)| a.cmplt(hi).all() && lo.cmplt(b).all();
        self.mobs.iter().any(|m| m.alive() && overlaps(m.aabb()))
            || ctx.players.iter().any(|t| t.alive && !t.spectator && overlaps(t.shape.aabb(t.pos)))
    }
}

/// `SpawnPlacements.isSpawnPositionOk` and `checkSpawnRules` for a
/// `REINFORCEMENT` of `kind` at `cell`: on the ground for zombies, husks,
/// zombie villagers and zombified piglins (`ON_GROUND`), in water for the
/// drowned (`IN_WATER`, then `checkDrownedSpawnRules`, which skips its
/// depth and biome rolls for reinforcements).
fn spot_ok<W: MobWorld + ?Sized>(
    world: &W,
    kind: MobKind,
    cell: IVec3,
    rules: monster_spawns::SpawnRules,
    rng: &mut Rng,
) -> bool {
    if !world.loaded(cell) {
        return false;
    }
    let (Some(below), Some(at), Some(above)) =
        (world.block(cell - IVec3::Y), world.block(cell), world.block(cell + IVec3::Y))
    else {
        return false;
    };
    if kind == MobKind::Drowned {
        return at.holds_water()
            && !monster_spawns::is_redstone_conductor(above)
            && below.holds_water()
            && dark_enough(world, cell, rules, rng);
    }
    if !monster_spawns::valid_spawn_floor(below, kind)
        || !monster_spawns::valid_empty_cell(at, kind)
        || !monster_spawns::valid_empty_cell(above, kind)
    {
        return false;
    }
    match kind {
        // `checkZombifiedPiglinSpawnRules`: any light, not on nether wart.
        MobKind::ZombifiedPiglin => below.name() != "nether wart block",
        // `checkSurfaceMonstersSpawnRules`: dark, and under open sky.
        MobKind::Husk => dark_enough(world, cell, rules, rng) && world.sky_light_level(cell) == Some(15),
        // `checkMonsterSpawnRules`.
        _ => dark_enough(world, cell, rules, rng),
    }
}

/// `Monster.isDarkEnoughToSpawn` with each dimension's spawn light, as the
/// natural spawner applies it.
fn dark_enough<W: MobWorld + ?Sized>(world: &W, cell: IVec3, rules: monster_spawns::SpawnRules, rng: &mut Rng) -> bool {
    match rules.dimension {
        Dimension::Overworld => monster_spawns::dark_enough(world, cell, rules.light.monster_darken, rng),
        Dimension::Nether => world.block_light(cell) <= 7,
        _ => {
            world.block_light(cell) == 0
                && world.sky_light_level(cell).is_some_and(|sky| u32::from(sky) <= rng.next_int(32))
        }
    }
}

#[cfg(test)]
mod tests;
