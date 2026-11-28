//! v0.16 Track C: the Wither (Java 26.3 `WitherBoss`), its skulls
//! (`WitherSkull` on `AbstractHurtingProjectile`), its summoning pattern
//! (`WitherSkullBlock`'s `BlockPattern`) and the Nether star.
//!
//! The Entities pass ([`Entities::wither_tick`]) runs the Wither's server
//! AI once a tick (`customServerAiStep` and its goals): the charge-up while
//! invulnerable, target choice for the three heads, the skulls they fire,
//! breaking blocks after a hurt and the slow regeneration. The mob update
//! ([`Mob::wither_motion`]) runs `aiStep`'s motion and the flight.
//! See the dated verification record for the differences from Java.
use glam::{DVec3, IVec3, Vec3};
use serde_json::{Value, json};

use super::{Ctx, Entities, EntityEvent, Mob, MobKind, MobSound, MobWorld, PlayerId, Rng};
use crate::item::Item;
use crate::simulation::damage::{Cause, DamageType};
use crate::simulation::difficulty::Difficulty;
use crate::world::block::Block;

/// The Wither's Nether star (item 1690, Track C's reservation).
pub const NETHER_STAR: Item = Item(1690);
/// `WitherBoss.createAttributes`.
pub const MAX_HEALTH: f32 = 300.0;
pub const ARMOR: u32 = 4;
pub const FOLLOW_RANGE: f64 = 40.0;
pub const FLYING_SPEED: f64 = 0.6;
/// `INVULNERABLE_TICKS`.
pub const INVULNERABLE_TICKS: i32 = 220;
/// `xpReward`.
pub const EXPERIENCE: u32 = 50;
/// `TARGETING_CONDITIONS`' range for the side heads.
pub const HEAD_RANGE: f64 = 20.0;
/// `RangedAttackGoal(this, 1.0, 40, 20.0)`.
pub const ATTACK_INTERVAL: i32 = 40;
pub const ATTACK_RADIUS: f64 = 20.0;
/// `WitherSkull.onHitEntity`: 8 from a living owner (who heals 5 on a
/// kill), 5 magic without one.
pub const SKULL_DAMAGE: f32 = 8.0;
pub const SKULL_MAGIC_DAMAGE: f32 = 5.0;
pub const SKULL_HEAL: f32 = 5.0;
/// The charge-up's blast and a skull's.
pub const SPAWN_POWER: f32 = 7.0;
pub const SKULL_POWER: f32 = 1.0;
/// `ItemEntity.setExtendedLifetime`: the Nether star starts at age -6000.
pub const STAR_EXTRA_SECS: f32 = 300.0;
/// `LivingEntity` forgets its last attacker 100 ticks after the hurt: a mob
/// dying of the Wither effect inside that grows a wither rose.
pub const KILL_CREDIT_SECS: f32 = 5.0;
/// `EntityTypes.WITHER`'s client tracking range (10 chunks): players this
/// close see the boss bar.
pub const BOSS_BAR_RANGE: f64 = 160.0;

pub const SKULL_CAUSE: Cause = Cause::new(DamageType::WitherSkull, "was shot by a skull from Wither");
pub const BLAST_CAUSE: Cause = Cause::new(DamageType::PlayerExplosion, "was blown up by Wither");

/// The Wither's level events, played to everyone (`globalLevelEvent` 1023)
/// or where they happen (1022, 1024).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cue {
    /// 1023 `WITHER_SPAWN`, heard across the dimension.
    Spawn,
    /// 1024 `WITHER_SHOOT`.
    Shoot,
    /// 1022 `WITHER_BREAK_BLOCK`.
    BreakBlocks,
}

impl Cue {
    pub const ALL: [Cue; 3] = [Cue::Spawn, Cue::Shoot, Cue::BreakBlocks];
}

/// Who a head aims at (`DATA_TARGET_A..C` hold entity ids).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Foe {
    Player(PlayerId),
    Mob(u32),
}

/// What the Wither remembers and decides each tick.
#[derive(Clone, Debug)]
pub struct State {
    /// `Invul`: ticks of the charge-up left.
    pub invul: i32,
    /// Head 0 (the main target) and the two side heads' targets.
    pub targets: [Option<Foe>; 3],
    next_head_update: [i32; 2],
    idle_head_updates: [i32; 2],
    destroy_blocks_tick: i32,
    tick_count: i32,
    /// `RangedAttackGoal.attackTime` and `seeTime`.
    attack_time: i32,
    see_time: i32,
    /// The main target's feet this tick, for the motion (`aiStep`).
    pub(super) target_at: Option<DVec3>,
    /// The flight goal's wanted position (`FlyingMoveControl`).
    pub(super) wanted: Option<DVec3>,
    /// Where each side head looks (Java yaw and pitch, degrees).
    pub head_rot: [(f32, f32); 2],
    /// Simulated seconds not yet spent on whole ticks.
    time: f64,
    motion_time: f64,
    /// v0.16 Track H: the target selector's running goal.
    target_goal: TargetGoal,
    /// `getLastHurtByMob` (player or mob), the tick it hurt, and how many
    /// accepted hurts there were (`lastHurtByMobTimestamp`) against the
    /// count `HurtByTargetGoal` last started on.
    last_hurt_by: Option<Foe>,
    last_hurt_tick: i32,
    hurt_stamp: u32,
    hurt_goal_stamp: u32,
    /// `RangedAttackGoal` and `WaterAvoidingRandomFlyingGoal` running.
    ranged: bool,
    strolling: bool,
}

/// Which target goal runs (`HurtByTargetGoal` at priority 1,
/// `NearestAttackableTargetGoal` at 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum TargetGoal {
    #[default]
    None,
    HurtBy,
    Nearest,
}

/// `LivingEntity.baseTick` forgets the last attacker after 100 ticks.
const HURT_MEMORY: i32 = 100;
/// `WaterAvoidingRandomFlyingGoal`: `RandomStrollGoal`'s default interval
/// 120, halved by `reducedTickDelay` (goals are checked every other tick).
pub const STROLL_INTERVAL: u32 = 60;
/// `HoverRandomPos.getPos(mob, 8, 7, view, PI/2, 3, 1)` and the fallback
/// `AirAndWaterRandomPos.getPos(mob, 8, 4, -2, view, PI/2)`.
pub const HOVER: (i32, i32, i32, i32) = (8, 7, 3, 1);
pub const AIR_AND_WATER: (i32, i32, i32) = (8, 4, -2);

impl Default for State {
    fn default() -> Self {
        State {
            invul: 0,
            targets: [None; 3],
            next_head_update: [0; 2],
            idle_head_updates: [0; 2],
            destroy_blocks_tick: 0,
            tick_count: 0,
            attack_time: -1,
            see_time: 0,
            target_at: None,
            wanted: None,
            head_rot: [(0.0, 0.0); 2],
            time: 0.0,
            motion_time: 0.0,
            target_goal: TargetGoal::None,
            last_hurt_by: None,
            last_hurt_tick: 0,
            hurt_stamp: 0,
            hurt_goal_stamp: 0,
            ranged: false,
            strolling: false,
        }
    }
}

impl State {
    /// `addAdditionalSaveData`: only `Invul`.
    pub fn save(&self) -> Value {
        json!({"Invul": self.invul})
    }

    pub fn load(v: &Value) -> Self {
        State { invul: v["Invul"].as_i64().unwrap_or(0).clamp(0, INVULNERABLE_TICKS as i64) as i32, ..Self::default() }
    }

    /// `hurtServer` after the checks: blocks break 20 ticks on and the
    /// side heads grow restless.
    pub(super) fn hurt(&mut self) {
        if self.destroy_blocks_tick <= 0 {
            self.destroy_blocks_tick = 20;
        }
        for i in &mut self.idle_head_updates {
            *i += 3;
        }
    }
}

/// `isPowered`: at half health or less the armor is up.
pub fn powered(m: &Mob) -> bool {
    m.kind == MobKind::Wither && m.health <= MAX_HEALTH / 2.0
}

/// The boss bar's progress: the charge during the charge-up, then health.
pub fn boss_progress(m: &Mob) -> f32 {
    match &m.wither {
        Some(s) if s.invul > 0 => 1.0 - s.invul as f32 / INVULNERABLE_TICKS as f32,
        _ => (m.health / MAX_HEALTH).clamp(0.0, 1.0),
    }
}

/// `WitherBossRenderer.scale` relative to the full model: shrunk by up to a
/// quarter while charging.
pub fn model_scale(m: &Mob) -> f32 {
    let invul = m.wither.as_ref().map_or(0, |s| s.invul);
    if invul > 0 { 1.0 - invul as f32 / INVULNERABLE_TICKS as f32 * 0.25 } else { 1.0 }
}

/// `WITHER_FRIENDS` (`#undead`): mobs the Wither never targets or is hurt by.
pub fn friend(kind: MobKind) -> bool {
    kind == MobKind::Wither || kind.creature() == crate::enchant::Creature::Undead
}

/// `WitherBoss.canDestroy`: anything but air and `#wither_immune`.
pub fn can_destroy(b: Block) -> bool {
    use crate::world::jukebox::REINFORCED_DEEPSLATE;
    b != Block::AIR
        && !matches!(
            b,
            Block::BEDROCK | Block::END_PORTAL | Block::END_PORTAL_FRAME | Block::END_GATEWAY | REINFORCED_DEEPSLATE
        )
        && b.base() != Block::END_PORTAL_FRAME
        && b != crate::world::redstone_blocks::MOVING
}

// ------------------------------------------------------------------ pattern

/// Java's `Direction.values()` order: down, up, north, south, west, east.
const DIRECTIONS: [IVec3; 6] = [IVec3::NEG_Y, IVec3::Y, IVec3::NEG_Z, IVec3::Z, IVec3::NEG_X, IVec3::X];

/// A found Wither pattern (`BlockPattern.BlockPatternMatch`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PatternMatch {
    pub front_top_left: IVec3,
    pub forwards: IVec3,
    pub up: IVec3,
}

impl PatternMatch {
    /// `getBlock(right, down, forwards)`.
    pub fn cell(&self, right: i32, down: i32, forwards: i32) -> IVec3 {
        let r = self.forwards.cross(self.up);
        self.front_top_left - self.up * down + r * right + self.forwards * forwards
    }
}

/// `WitherSkullBlock.getOrCreateWitherFull`: `^^^`, `###`, `~#~` (skulls,
/// soul sand or soul soil, air) found by `BlockPattern.find` from `origin`.
pub fn find_pattern(get: impl Fn(IVec3) -> Option<Block>, origin: IVec3) -> Option<PatternMatch> {
    find_rows(get, origin, [b"^^^", b"###", b"~#~"])
}

/// v0.16 Track H: `WitherSkullBlock.getOrCreateWitherBase` (`   `, `###`,
/// `~#~`: any top row), which `canSpawnMob` looks for around the cell a
/// dispenser would put a skull in.
pub fn find_base(get: impl Fn(IVec3) -> Option<Block>, origin: IVec3) -> Option<PatternMatch> {
    find_rows(get, origin, [b"   ", b"###", b"~#~"])
}

/// `WitherSkullBlock.canSpawnMob` for a wither skeleton skull a dispenser
/// holds: two blocks over the world's bottom, not in Peaceful, and part of a
/// Wither's base.
pub fn can_spawn_mob(get: impl Fn(IVec3) -> Option<Block>, at: IVec3, min_y: i32, peaceful: bool) -> bool {
    at.y >= min_y + 2 && !peaceful && find_base(get, at).is_some()
}

fn find_rows(get: impl Fn(IVec3) -> Option<Block>, origin: IVec3, rows: [&[u8; 3]; 3]) -> Option<PatternMatch> {
    let test = |c: u8, b: Option<Block>| match (c, b) {
        (b' ', _) => true,
        (b'^', Some(b)) => crate::world::skulls::is_wither_skull(b),
        (b'#', Some(b)) => b == Block::SOUL_SAND || b == crate::world::nether_biome_blocks::SOUL_SOIL,
        (b'~', Some(b)) => b == Block::AIR,
        _ => false,
    };
    // `BlockPos.betweenClosed(origin, origin + 2)`: x fastest, then y, z.
    for dz in 0..3 {
        for dy in 0..3 {
            for dx in 0..3 {
                let at = origin + IVec3::new(dx, dy, dz);
                for forwards in DIRECTIONS {
                    for up in DIRECTIONS {
                        if up == forwards || up == -forwards {
                            continue;
                        }
                        let m = PatternMatch { front_top_left: at, forwards, up };
                        let ok =
                            (0..3).all(|x| (0..3).all(|y| test(rows[y as usize][x as usize], get(m.cell(x, y, 0)))));
                        if ok {
                            return Some(m);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Where the Wither spawns for a match (`getBlock(1, 2, 0)`, plus 0.5,
/// 0.55, 0.5) and its yaw (Java's 0 along an x-forwards pattern, else 90).
pub fn spawn_point(m: &PatternMatch) -> (DVec3, f32) {
    let c = m.cell(1, 2, 0);
    let yrot: f32 = if m.forwards.x != 0 { 0.0 } else { 90.0 };
    (c.as_dvec3() + DVec3::new(0.5, 0.55, 0.5), yrot)
}

/// `WitherSkullBlock.checkSpawn` for each wither skeleton skull placed
/// since the last call: a complete pattern turns into a Wither (not in
/// Peaceful, not below the world).
pub fn finish_withers(world: &mut crate::world::World, entities: &mut Entities, peaceful: bool) {
    for pos in std::mem::take(&mut world.wither_skulls) {
        if peaceful || pos.y < world.min_y() || !world.get_block(pos).is_some_and(crate::world::skulls::is_wither_skull)
        {
            continue;
        }
        let Some(m) = find_pattern(|p| world.get_block(p), pos) else { continue };
        // `CarvedPumpkinBlock.clearPatternBlocks`: the front layer goes,
        // each with its break particles (level event 2001).
        for x in 0..3 {
            for y in 0..3 {
                let cell = m.cell(x, y, 0);
                if let Some(b) = world.get_block(cell).filter(|&b| b != Block::AIR) {
                    world.particles.push(crate::particles::Request::Break { cell, block: b });
                }
                world.set_block(cell, Block::AIR);
            }
        }
        let (feet, yrot) = spawn_point(&m);
        entities.spawn_wither(feet, yrot);
    }
}

impl Entities {
    /// A fresh Wither from its pattern: `makeInvulnerable` (a third of its
    /// health, the 220-tick charge) and the global spawn sound once the
    /// charge is spent.
    pub fn spawn_wither(&mut self, feet: DVec3, yrot: f32) {
        let mut m = Mob::new(MobKind::Wither, feet, 0.0);
        m.set_yrot(yrot);
        m.health = MAX_HEALTH / 3.0;
        m.persistent = true;
        if let Some(s) = &mut m.wither {
            s.invul = INVULNERABLE_TICKS;
        }
        self.mobs.push(m);
        self.ensure_mob_ids();
    }

    /// Where a foe is now: feet, eye height and whether it can still be
    /// attacked (`canAttack`: alive, a targetable player or a non-undead mob).
    fn foe_at(&self, ctx: &Ctx, foe: Foe) -> Option<(DVec3, f64)> {
        match foe {
            Foe::Player(id) => {
                ctx.players.iter().find(|t| t.id == id && t.alive && t.targetable).map(|t| (t.pos, t.eye_height()))
            }
            Foe::Mob(uid) => {
                self.mobs.iter().find(|m| m.uid == uid && m.alive() && !friend(m.kind)).map(|m| (m.pos, m.eye_height()))
            }
        }
    }

    /// Everyone a Wither at `centre` could aim at in the box `half` around
    /// its box, nearest to `from` first, seen from `from`.
    fn wither_candidates<W: MobWorld + ?Sized>(
        &self,
        world: &W,
        ctx: &Ctx,
        me: usize,
        half: DVec3,
        range: f64,
    ) -> Vec<(Foe, DVec3)> {
        let w = &self.mobs[me];
        let (lo, hi) = w.aabb();
        let (lo, hi) = (lo - half, hi + half);
        let from = w.pos + DVec3::Y * WITHER_EYE;
        let inside = |p: DVec3, h: f64| {
            p.x < hi.x && p.x + 0.0 > lo.x && p.y < hi.y && p.y + h > lo.y && p.z < hi.z && p.z > lo.z
        };
        let mut out: Vec<(f64, Foe, DVec3, f64)> = Vec::new();
        for t in &ctx.players {
            if t.alive && t.targetable && !t.spectator && inside(t.pos, t.shape.height) {
                let r = t.detection_range(range);
                let d2 = t.pos.distance_squared(w.pos);
                if d2 <= r * r {
                    out.push((t.pos.distance_squared(from), Foe::Player(t.id), t.pos, t.eye_height()));
                }
            }
        }
        self.mob_index.visit(w.pos, half.x.max(half.y) + 4.0, |j| {
            let m = &self.mobs[j];
            if j != me && m.alive() && !friend(m.kind) && inside(m.pos, m.shape().height) {
                let d2 = m.pos.distance_squared(w.pos);
                if d2 <= range * range {
                    out.push((m.pos.distance_squared(from), Foe::Mob(m.uid), m.pos, m.eye_height()));
                }
            }
        });
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out.into_iter()
            .filter(|&(_, _, p, eye)| super::mob::line_of_sight(world, from, p + DVec3::Y * eye))
            .map(|(_, f, p, _)| (f, p))
            .collect()
    }

    /// `WitherBoss.customServerAiStep` and its goals, once a tick for every
    /// Wither in the simulation; `events` gets its sounds, blasts and
    /// broken blocks, and its skulls join [`Entities::wither_skulls`].
    pub(super) fn wither_tick<W: MobWorld + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        ctx: &Ctx,
        events: &mut Vec<EntityEvent>,
    ) {
        if !self.mobs.iter().any(|m| m.kind == MobKind::Wither) {
            return;
        }
        self.ensure_mob_ids();
        self.mob_index.rebuild(&self.mobs);
        for i in 0..self.mobs.len() {
            if self.mobs[i].kind != MobKind::Wither || !self.mobs[i].simulation_active || !self.mobs[i].alive() {
                continue;
            }
            let Some(s) = self.mobs[i].wither.as_mut() else { continue };
            s.time += dt;
            let tick = super::mob::swim::TICK;
            let ticks = ((s.time + 1e-9) / tick).floor().max(0.0) as u32;
            s.time -= f64::from(ticks) * tick;
            for _ in 0..ticks {
                if !self.mobs[i].alive() {
                    break;
                }
                self.wither_server_tick(i, world, ctx, events);
            }
        }
    }

    fn wither_server_tick<W: MobWorld + ?Sized>(
        &mut self,
        i: usize,
        world: &W,
        ctx: &Ctx,
        events: &mut Vec<EntityEvent>,
    ) {
        let difficulty = self.difficulty;
        let w = &mut self.mobs[i];
        let s = w.wither.as_mut().unwrap();
        s.tick_count += 1;
        // `Mob.checkDespawn` is replaced: no idle despawn.
        w.idle = 0.0;
        // `addEffect` refuses everything.
        if !w.effects.is_empty() {
            w.effects.clear();
        }
        let eye = w.pos + DVec3::Y * WITHER_EYE;
        if s.invul > 0 {
            let left = s.invul - 1;
            if left <= 0 {
                events.push(EntityEvent::WitherBlast { center: eye, power: SPAWN_POWER, dangerous: false });
                events.push(EntityEvent::Sound { sound: MobSound::Wither(Cue::Spawn), pos: w.pos });
            }
            s.invul = left;
            if s.tick_count % 10 == 0 {
                w.health = (w.health + 10.0).min(MAX_HEALTH);
            }
            self.wither_particles(i);
            return;
        }
        // `Mob.serverAiStep`: the selectors choose goals every other tick
        // (`(tickCount + id) % 2 == 0`, and the first tick); running goals
        // that need it tick every tick.
        let select = s.tick_count <= 1 || (s.tick_count as u32).wrapping_add(w.uid).is_multiple_of(2);
        if select {
            self.wither_select_goals(i, world, ctx);
        }
        let main = self.mobs[i].wither.as_ref().unwrap().targets[0];
        let main_at = main.and_then(|f| self.foe_at(ctx, f));
        let w_pos = self.mobs[i].pos;
        let body_yrot = self.mobs[i].yrot();
        let mut shots: Vec<(usize, DVec3, bool)> = Vec::new();
        // `RangedAttackGoal.tick` for head 0, and its `moveTo` or `stop`.
        let mut path_to = None;
        {
            let s = self.mobs[i].wither.as_mut().unwrap();
            s.target_at = main_at.map(|(p, _)| p);
            match main_at.filter(|_| s.ranged) {
                Some((p, eye_h)) => {
                    let aim = p + DVec3::Y * (eye_h * 0.5);
                    let d2 = p.distance_squared(w_pos);
                    let sees = super::mob::line_of_sight(world, eye, p + DVec3::Y * eye_h);
                    s.see_time = if sees { s.see_time + 1 } else { 0 };
                    // Out of range or unseen: fly a path to it (`moveTo`).
                    path_to = Some((d2 > ATTACK_RADIUS * ATTACK_RADIUS || s.see_time < 5).then_some(p));
                    s.attack_time -= 1;
                    if s.attack_time == 0 {
                        if sees {
                            let dangerous = self.rng.next_f32() < 0.001;
                            shots.push((0, aim, dangerous));
                            s.attack_time = ATTACK_INTERVAL;
                        }
                    } else if s.attack_time < 0 {
                        s.attack_time = ATTACK_INTERVAL;
                    }
                }
                None => {
                    s.see_time = 0;
                    s.attack_time = -1;
                }
            }
        }
        self.wither_navigate(i, world, path_to);
        // v1.0 gap Track 2c: `RangedAttackGoal.tick`'s `setLookAt(target,
        // 30, 30)` for the main head; `WitherDoNothingGoal` holds LOOK while
        // it charges up, so its idle looks wait.
        let ranged = self.mobs[i].wither.as_ref().is_some_and(|s| s.ranged);
        let charging = self.mobs[i].wither.as_ref().is_some_and(|s| s.invul > 0);
        if let Some((p, eye_h)) = main_at.filter(|_| ranged) {
            let look = super::mob::head::ATTACK_LOOK;
            self.mobs[i].set_look_at(p + DVec3::Y * eye_h, look, look);
        } else if charging {
            self.mobs[i].hold_look();
        }
        // The side heads.
        for h in 1..3usize {
            let s = self.mobs[i].wither.as_mut().unwrap();
            if s.tick_count < s.next_head_update[h - 1] {
                continue;
            }
            s.next_head_update[h - 1] = s.tick_count + 10 + self.rng.next_int(10) as i32;
            if matches!(difficulty, Difficulty::Normal | Difficulty::Hard) {
                let idle = s.idle_head_updates[h - 1];
                s.idle_head_updates[h - 1] += 1;
                if idle > 15 {
                    let r = |rng: &mut Rng, c: f64, half: f64| c - half + rng.next_f32() as f64 * 2.0 * half;
                    let t = DVec3::new(
                        r(&mut self.rng, w_pos.x, 10.0),
                        r(&mut self.rng, w_pos.y, 5.0),
                        r(&mut self.rng, w_pos.z, 10.0),
                    );
                    shots.push((h, t, true));
                    self.mobs[i].wither.as_mut().unwrap().idle_head_updates[h - 1] = 0;
                }
            }
            let target = self.mobs[i].wither.as_ref().unwrap().targets[h];
            if let Some(foe) = target {
                let ok = self.foe_at(ctx, foe).filter(|&(p, eye_h)| {
                    p.distance_squared(w_pos) <= 900.0 && super::mob::line_of_sight(world, eye, p + DVec3::Y * eye_h)
                });
                let s = self.mobs[i].wither.as_mut().unwrap();
                if let Some((p, eye_h)) = ok {
                    shots.push((h, p + DVec3::Y * (eye_h * 0.5), false));
                    s.next_head_update[h - 1] = s.tick_count + 40 + self.rng.next_int(20) as i32;
                    s.idle_head_updates[h - 1] = 0;
                } else {
                    s.targets[h] = None;
                }
            } else {
                let found = self.wither_candidates(world, ctx, i, DVec3::new(20.0, 8.0, 20.0), HEAD_RANGE);
                if !found.is_empty() {
                    let pick = found[self.rng.next_int(found.len() as u32) as usize].0;
                    self.mobs[i].wither.as_mut().unwrap().targets[h] = Some(pick);
                }
            }
        }
        // Side heads turn toward their targets (for drawing).
        for h in 1..3usize {
            let target = self.mobs[i].wither.as_ref().unwrap().targets[h].and_then(|f| self.foe_at(ctx, f));
            let head = head_pos(w_pos, body_yrot, h);
            let s = self.mobs[i].wither.as_mut().unwrap();
            let (yaw, pitch) = s.head_rot[h - 1];
            s.head_rot[h - 1] = match target {
                Some((p, eye_h)) => {
                    let d = p + DVec3::Y * eye_h - head;
                    let yrot = (d.z.atan2(d.x).to_degrees() as f32) - 90.0;
                    let xrot = -(d.y.atan2(d.x.hypot(d.z)).to_degrees() as f32);
                    (rotlerp(yaw, yrot, 10.0), rotlerp(pitch, xrot, 40.0))
                }
                None => (rotlerp(yaw, body_yrot, 10.0), pitch),
            };
        }
        for (head, target, dangerous) in shots {
            self.wither_shoot(i, head, target, dangerous, events);
        }
        // Blocks around it break 20 ticks after a hurt (`mobGriefing`).
        let s = self.mobs[i].wither.as_mut().unwrap();
        if s.destroy_blocks_tick > 0 {
            s.destroy_blocks_tick -= 1;
            if s.destroy_blocks_tick == 0 {
                let m = &self.mobs[i];
                let shape = m.shape();
                let width = ((shape.half_width * 2.0) / 2.0 + 1.0).floor() as i32;
                let height = shape.height.floor() as i32;
                let b = m.pos.floor().as_ivec3();
                events.push(EntityEvent::WitherBreak {
                    min: IVec3::new(b.x - width, b.y, b.z - width),
                    max: IVec3::new(b.x + width, b.y + height, b.z + width),
                    pos: m.pos,
                });
            }
        }
        let s = self.mobs[i].wither.as_ref().unwrap();
        if s.tick_count % 20 == 0 {
            let w = &mut self.mobs[i];
            w.health = (w.health + 1.0).min(MAX_HEALTH);
        }
        self.wither_particles(i);
    }

    /// `performRangedAttack(head, x, y, z, dangerous)`: a skull from the
    /// head toward the point, with the shoot sound.
    fn wither_shoot(&mut self, i: usize, head: usize, target: DVec3, dangerous: bool, events: &mut Vec<EntityEvent>) {
        let w = &self.mobs[i];
        let from = head_pos(w.pos, w.yrot(), head);
        let dir = (target - from).normalize_or(DVec3::Y);
        events.push(EntityEvent::Sound { sound: MobSound::Wither(Cue::Shoot), pos: w.pos });
        let uid = w.uid;
        self.wither_skulls.push(Skull::new(from, dir, dangerous, Some(uid)));
        self.sculk.shot(from, crate::world::vibration::EventSource::mob(uid));
    }

    /// The Wither's smoke at each head, its armor's motes and the charge's
    /// swirl (`aiStep`'s particles, a few a tick).
    fn wither_particles(&mut self, i: usize) {
        let w = &self.mobs[i];
        let (pos, yrot) = (w.pos, w.yrot());
        let charging = w.wither.as_ref().is_some_and(|s| s.invul > 0);
        let armored = powered(w);
        for h in 0..3 {
            let mut smoke = crate::particles::Burst::new(crate::particles::Kind::Smoke, head_pos(pos, yrot, h), 1);
            smoke.spread = DVec3::splat(0.3);
            self.particles.push(crate::particles::Request::Burst(smoke));
            if armored && self.rng.next_int(4) == 0 {
                let mut b = crate::particles::Burst::new(crate::particles::Kind::Effect, head_pos(pos, yrot, h), 1);
                b.spread = DVec3::splat(0.3);
                b.color = Some([0.7, 0.7, 0.5, 1.0]);
                self.particles.push(crate::particles::Request::Burst(b));
            }
        }
        if charging {
            let mut b = crate::particles::Burst::new(crate::particles::Kind::Effect, pos + DVec3::Y * 1.65, 3);
            b.spread = DVec3::new(1.0, 1.65, 1.0);
            b.color = Some([0.7, 0.7, 0.9, 1.0]);
            self.particles.push(crate::particles::Request::Burst(b));
        }
    }

    /// A player hurt the Wither (`setLastHurtByMob`): its
    /// `HurtByTargetGoal` decides on the next selector tick.
    pub(super) fn wither_hurt_by(&mut self, index: usize, player: PlayerId) {
        self.wither_hurt_by_foe(index, Foe::Player(player));
    }

    /// v0.16 Track H: a mob hurt the Wither (`HurtByTargetGoal` against
    /// mobs).
    pub(super) fn wither_hurt_by_mob(&mut self, index: usize, attacker: u32) {
        self.wither_hurt_by_foe(index, Foe::Mob(attacker));
    }

    fn wither_hurt_by_foe(&mut self, index: usize, foe: Foe) {
        if let Some(s) = self.mobs.get_mut(index).and_then(|m| m.wither.as_mut()) {
            s.last_hurt_by = Some(foe);
            s.last_hurt_tick = s.tick_count;
            s.hurt_stamp = s.hurt_stamp.wrapping_add(1);
        }
    }

    /// `GoalSelector.tick` on a selection tick: the target goals
    /// (`HurtByTargetGoal` 1, `NearestAttackableTargetGoal` 2), then
    /// `RangedAttackGoal` (2) and `WaterAvoidingRandomFlyingGoal` (5), which
    /// share the `MOVE` flag.
    fn wither_select_goals<W: MobWorld + ?Sized>(&mut self, i: usize, world: &W, ctx: &Ctx) {
        let pos = self.mobs[i].pos;
        let s = self.mobs[i].wither.as_ref().unwrap();
        // `TargetGoal.canContinueToUse`: alive, attackable, within range.
        let (mut target, mut goal) = (s.targets[0], s.target_goal);
        let reach = |at: Option<(DVec3, f64)>| at.is_some_and(|(p, _)| p.distance_squared(pos) <= FOLLOW_RANGE.powi(2));
        if !target.is_some_and(|f| reach(self.foe_at(ctx, f))) {
            (target, goal) = (None, TargetGoal::None);
        }
        // `HurtByTargetGoal.canUse`: a newer hurt by someone attackable.
        let fresh = s.tick_count - s.last_hurt_tick <= HURT_MEMORY;
        if goal != TargetGoal::HurtBy
            && s.hurt_stamp != s.hurt_goal_stamp
            && fresh
            && let Some(foe) = s.last_hurt_by.filter(|&f| self.foe_at(ctx, f).is_some())
        {
            let stamp = s.hurt_stamp;
            (target, goal) = (Some(foe), TargetGoal::HurtBy);
            self.mobs[i].wither.as_mut().unwrap().hurt_goal_stamp = stamp;
        }
        // `NearestAttackableTargetGoal.canUse` when no target goal runs.
        if goal == TargetGoal::None {
            let range = DVec3::splat(FOLLOW_RANGE);
            if let Some(&(foe, _)) = self.wither_candidates(world, ctx, i, range, FOLLOW_RANGE).first() {
                (target, goal) = (Some(foe), TargetGoal::Nearest);
            }
        }
        let m = &mut self.mobs[i];
        let nav_done = m.nav.is_done();
        let s = m.wither.as_mut().unwrap();
        (s.targets[0], s.target_goal) = (target, goal);
        // `RangedAttackGoal`: runs while there is a target (or its path).
        if s.ranged && target.is_none() && nav_done {
            s.ranged = false;
            s.see_time = 0;
            s.attack_time = -1;
        }
        if !s.ranged && target.is_some() {
            // It takes `MOVE` from the flight goal, which stops.
            s.ranged = true;
            s.strolling = false;
            m.nav.stop();
        }
        let s = m.wither.as_mut().unwrap();
        if s.strolling && nav_done {
            s.strolling = false;
        }
        // `WaterAvoidingRandomFlyingGoal.canUse`: `nextInt(60) == 0`, then a
        // spot to hover over.
        if !s.ranged && !s.strolling && self.rng.next_int(STROLL_INTERVAL) == 0 {
            let spot = self.mobs[i].hover_spot(world, ctx, &mut self.rng);
            if let Some(spot) = spot {
                let m = &mut self.mobs[i];
                if m.walk_to(world, spot, 1, super::path::ground_speed(FLYING_SPEED), false) {
                    m.wither.as_mut().unwrap().strolling = true;
                }
            }
        }
    }

    /// The Wither's `FlyingPathNavigation`: `RangedAttackGoal`'s `moveTo`
    /// (`Some(Some(target))`) or `stop` (`Some(None)`), then
    /// `PathNavigation.tick` sets the move control's wanted spot.
    fn wither_navigate<W: MobWorld + ?Sized>(&mut self, i: usize, world: &W, path_to: Option<Option<DVec3>>) {
        let m = &mut self.mobs[i];
        match path_to {
            Some(Some(target)) => {
                // `moveTo(entity)`: a path to its block, reach 1.
                if !m.walk_to(world, target.floor().as_ivec3(), 1, super::path::ground_speed(FLYING_SPEED), true) {
                    m.nav.stop();
                }
            }
            Some(None) => m.nav.stop(),
            None => {}
        }
        let wanted = if m.nav.is_done() {
            None
        } else {
            let caps = m.caps(true);
            let mover = m.mover(super::path::ground_speed(FLYING_SPEED));
            m.nav.tick(world, &caps, &mover)
        };
        if let Some(s) = m.wither.as_mut() {
            s.wanted = wanted;
        }
    }

    /// Moves the skulls; a hit becomes damage, Wither and a blast.
    pub(super) fn update_wither_skulls<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        ctx: &Ctx,
        boss: &super::dragon::Boxes,
        events: &mut Vec<EntityEvent>,
    ) {
        if self.wither_skulls.is_empty() {
            return;
        }
        let mut skulls = std::mem::take(&mut self.wither_skulls);
        let difficulty = self.difficulty;
        skulls.retain_mut(|k| {
            // Frozen where entities don't tick, including a restored skull
            // whose chunk hasn't streamed in yet; gone if a ticking cell
            // still has no block.
            let cell = k.pos.floor().as_ivec3();
            if !world.entity_ticking(cell) {
                return true;
            }
            if world.block(cell).is_none() {
                return false;
            }
            // `AbstractHurtingProjectile.tick`: an owner that can't be found
            // (unloaded) keeps the skull; only one found dead discards it.
            let owner_dead = k
                .owner
                .is_some_and(|u| self.mobs.iter().any(|m| m.uid == u && m.kind == MobKind::Wither && !m.alive()));
            if owner_dead {
                return false;
            }
            match k.tick(world, ctx, &self.mobs, boss) {
                None => true,
                Some(hit) => {
                    let source = super::sculk_events::Sculk::projectile_source(None, k.owner);
                    let heading = k.pos - k.previous_pos;
                    self.sculk.land(world, k.pos, heading, source);
                    self.wither_skull_hit(k, hit, difficulty, events);
                    events.push(EntityEvent::WitherBlast { center: k.pos, power: SKULL_POWER, dangerous: k.dangerous });
                    false
                }
            }
        });
        // Skulls fired during the hits stay.
        skulls.append(&mut self.wither_skulls);
        self.wither_skulls = skulls;
    }

    /// `WitherSkull.onHitEntity`.
    fn wither_skull_hit(&mut self, k: &Skull, hit: Hit, difficulty: Difficulty, events: &mut Vec<EntityEvent>) {
        let wither_secs = match difficulty {
            Difficulty::Normal => SKULL_WITHER_SECONDS[0],
            Difficulty::Hard => SKULL_WITHER_SECONDS[1],
            _ => 0,
        };
        let owner = k.owner.and_then(|u| self.mobs.iter().position(|m| m.uid == u));
        match hit {
            Hit::Block => {}
            // `witherSkull(this, wither)` for 8 (magic 5 with no owner): it
            // breaks a crystal, and lands on a dragon without hurting it (a
            // Wither isn't a player); a part isn't living, so no Wither
            // effect. The skull's blast follows, which does hurt it.
            Hit::Boss(struck) => {
                let damage = if owner.is_some() { SKULL_DAMAGE } else { SKULL_MAGIC_DAMAGE };
                if !struck.is_dragon() {
                    // v1.0 gap Track 8b: the crystal's blast names the Wither.
                    let wither = owner.map(|_| super::MobKind::Wither);
                    let blow = super::dragon::Blow::projectile(damage, false).blamed_on(wither);
                    events.push(EntityEvent::DragonStruck { hit: struck, blow, by: None });
                }
            }
            Hit::Player(id) => {
                let damage = if owner.is_some() { SKULL_DAMAGE } else { SKULL_MAGIC_DAMAGE };
                events.push(EntityEvent::ArrowEffectsStart { player: id });
                events.push(EntityEvent::PlayerHit {
                    player: id,
                    damage,
                    knockback: Vec3::ZERO,
                    java: None,
                    cause: if owner.is_some() { SKULL_CAUSE } else { Cause::MAGIC },
                });
                if wither_secs > 0 {
                    events.push(EntityEvent::PlayerEffect {
                        player: id,
                        effect: crate::simulation::effects::Effect::Wither,
                        amplifier: 1,
                        ticks: wither_secs * 20,
                    });
                }
                events.push(EntityEvent::ArrowEffectsEnd);
            }
            Hit::Mob(j) => {
                let damage = if owner.is_some() { SKULL_DAMAGE } else { SKULL_MAGIC_DAMAGE };
                let m = &mut self.mobs[j];
                if m.refuses_hit(damage) {
                    return;
                }
                let killed = m.damage(damage, None, &mut self.rng);
                if owner.is_some() {
                    m.wither_credit = KILL_CREDIT_SECS;
                }
                if !killed && wither_secs > 0 && m.kind != MobKind::WitherSkeleton {
                    m.effects.add(crate::simulation::effects::Effect::Wither, 1, wither_secs * 20);
                }
                if killed {
                    let (kind, pos, burning) = (m.kind, m.pos, m.burning);
                    if let Some(o) = owner {
                        let w = &mut self.mobs[o];
                        w.health = (w.health + SKULL_HEAL).min(MAX_HEALTH);
                        events.push(EntityEvent::WitherRose { pos });
                    }
                    self.drop_loot_with_fire(kind, pos, 0, burning, false);
                }
            }
        }
    }

    /// The Wither's blasts on mobs (`ServerExplosion.hurtEntities`), sparing
    /// the Wither and its friends (`hurtServer` refuses a source whose
    /// entity is a Wither); mobs it kills grow wither roses.
    pub fn wither_blast_mobs<W: crate::physics::BlockSource + ?Sized>(
        &mut self,
        world: &W,
        center: DVec3,
        power: f32,
        breaks_blocks: bool,
        events: &mut Vec<EntityEvent>,
    ) {
        self.blast_drops(world, center, power, breaks_blocks);
        self.blast_carts(world, center, power);
        self.blast_decor(center, power);
        if let Some(fight) = &mut self.fight {
            fight.explode(world, center, power);
        }
        self.wild_dragons.explode(world, center, power);
        self.crystals.explode(center, power);
        let mut killed = Vec::new();
        for m in &mut self.mobs {
            if !m.alive() || m.kind == MobKind::Wither || m.pos.distance(center) > f64::from(power) * 2.0 {
                continue;
            }
            let (min, max) = m.aabb();
            let exposure = crate::world::explosion::seen_percent(world, center, min, max);
            let Some((damage, knockback)) = super::explosion_damage(power, m.pos.distance(center), exposure) else {
                continue;
            };
            let eye = m.look_eye();
            let push = (eye - center).normalize_or_zero() * knockback;
            m.wither_credit = KILL_CREDIT_SECS;
            if m.blast(damage, push, &mut self.rng) {
                killed.push((m.kind, m.pos));
            }
        }
        self.particles.push(crate::particles::Request::Explosion { pos: center, large: power >= 2.0 });
        for (kind, pos) in killed {
            events.push(EntityEvent::WitherRose { pos });
            self.allay_equipment_drops(kind, pos);
            self.copper_equipment_drops(kind, pos);
        }
    }

    /// The Wither's own drops: the Nether star, which lasts twice as long
    /// on the ground (`setExtendedLifetime`) and shrugs off blasts.
    pub(super) fn wither_star(&mut self, pos: DVec3) {
        let mut item = super::item::ItemEntity::new(
            crate::inventory::Stack::new(NETHER_STAR, 1),
            pos,
            DVec3::ZERO,
            0.5,
            &mut self.rng,
        );
        item.age = -STAR_EXTRA_SECS;
        self.items.push(item);
    }

    /// Withers whose boss bar `player` sees, nearest first: progress and
    /// whether the charge-up is still running.
    pub fn wither_bars(&self, player: DVec3) -> Vec<f32> {
        let mut bars: Vec<(f64, f32)> = self
            .mobs
            .iter()
            .filter(|m| {
                m.kind == MobKind::Wither && m.alive() && m.pos.distance_squared(player) < BOSS_BAR_RANGE.powi(2)
            })
            .map(|m| (m.pos.distance_squared(player), boss_progress(m)))
            .collect();
        bars.sort_by(|a, b| a.0.total_cmp(&b.0));
        bars.into_iter().map(|(_, p)| p).collect()
    }
}

/// Feet to the centre head's eyes (`EntityTypes.WITHER` eye height 2.9...
/// the model's middle head at 3.0 blocks).
pub const WITHER_EYE: f64 = 2.975;

/// `getHeadX/Y/Z`: the centre head over the body, the side heads 1.3
/// blocks out on either side at 2.2 blocks.
pub fn head_pos(pos: DVec3, body_yrot: f32, head: usize) -> DVec3 {
    if head == 0 {
        return pos + DVec3::Y * 3.0;
    }
    let angle = ((body_yrot + 180.0 * (head as f32 - 1.0)).to_radians()) as f64;
    DVec3::new(pos.x + angle.cos() * 1.3, pos.y + 2.2, pos.z + angle.sin() * 1.3)
}

/// `WitherBoss.rotlerp`.
fn rotlerp(a: f32, b: f32, max: f32) -> f32 {
    a + super::mob::swim::wrap_degrees(b - a).clamp(-max, max)
}

impl Mob {
    /// One Java tick of `WitherBoss.aiStep`'s motion and the flight, with
    /// `vel` in blocks a second. Called from [`Mob::update`] in place of the
    /// shared steering.
    pub(super) fn wither_motion<W: MobWorld + ?Sized>(&mut self, dt: f64, world: &W) {
        let Some(s) = self.wither.as_mut() else { return };
        s.motion_time += dt;
        while self.wither.as_ref().is_some_and(|s| s.motion_time >= super::mob::swim::TICK - 1e-9) {
            let s = self.wither.as_mut().unwrap();
            s.motion_time -= super::mob::swim::TICK;
            let (target, wanted, charging) = (s.target_at, s.wanted, s.invul > 0);
            let mut dm = self.vel * super::mob::swim::TICK * DVec3::new(1.0, 0.6, 1.0);
            if let Some(t) = target.filter(|_| !charging) {
                let mut yd = dm.y;
                if self.pos.y < t.y || !powered(self) && self.pos.y < t.y + 5.0 {
                    yd = yd.max(0.0);
                    yd += 0.3 - yd * 0.6;
                }
                dm.y = yd;
                let delta = DVec3::new(t.x - self.pos.x, 0.0, t.z - self.pos.z);
                if delta.length_squared() > 9.0 {
                    let n = delta.normalize();
                    dm += DVec3::new(n.x * 0.3 - dm.x * 0.6, 0.0, n.z * 0.3 - dm.z * 0.6);
                }
            }
            if dm.x * dm.x + dm.z * dm.z > 0.05 {
                self.set_yrot((dm.z.atan2(dm.x).to_degrees() as f32) - 90.0);
            }
            // `LivingEntity.aiStep` drops tiny motion.
            for v in [&mut dm.x, &mut dm.y, &mut dm.z] {
                if v.abs() < 0.003 {
                    *v = 0.0;
                }
            }
            // `FlyingMoveControl`: toward the wanted spot without gravity,
            // falling otherwise; `travelInAir` at the flying speed's input.
            let flying = wanted.filter(|w| !charging && w.distance_squared(self.pos) > 2.5e-7);
            let mut input = DVec3::ZERO;
            if let Some(w) = flying {
                let d = w - self.pos;
                let yrot = (d.z.atan2(d.x).to_degrees() as f32) - 90.0;
                let turned = self.yrot() + super::mob::swim::wrap_degrees(yrot - self.yrot()).clamp(-90.0, 90.0);
                self.set_yrot(turned);
                let speed = if self.on_ground { 0.6 } else { FLYING_SPEED };
                input = DVec3::new(0.0, if d.y > 0.0 { speed } else { -speed }, speed);
            }
            let f = if self.on_ground { 0.6 } else { 1.0 };
            let accel = if self.on_ground { FLYING_SPEED * 0.216 / (f * f * f) } else { 0.02 };
            super::mob::swim::move_relative(&mut dm, accel, input, self.yrot());
            self.java_move(world, &mut dm, 0.6);
            let gravity = if flying.is_some() { 0.0 } else { 0.08 };
            dm = DVec3::new(dm.x * f * 0.91, (dm.y - gravity) * 0.98, dm.z * f * 0.91);
            self.vel = dm / super::mob::swim::TICK;
            if self.wither.as_ref().is_some_and(|s| s.wanted.is_some_and(|w| w.distance_squared(self.pos) < 1.0)) {
                self.wither.as_mut().unwrap().wanted = None;
            }
        }
        self.limb_amp = 0.0;
    }

    /// `WaterAvoidingRandomFlyingGoal.getPosition`: `HoverRandomPos` (a
    /// spot over solid ground ahead, one to three blocks up), else
    /// `AirAndWaterRandomPos`; the best-valued of ten tries each
    /// (`RandomPos.generateRandomPos`).
    pub(super) fn hover_spot<W: MobWorld + ?Sized>(&self, world: &W, ctx: &Ctx, rng: &mut Rng) -> Option<IVec3> {
        let view = self.look_dir();
        let (h, v, above_max, above_min) = HOVER;
        let solid = |p: IVec3| world.block(p).is_some_and(Block::is_solid);
        let open = |p: IVec3| {
            let caps = self.caps(false);
            !world.block(p).is_some_and(|b| b.holds_water())
                && caps.malus.get(super::path::static_type(world, p)) == 0.0
        };
        let in_world = |p: IVec3| (crate::world::chunk::WORLD_MIN_Y..crate::world::chunk::WORLD_MAX_Y).contains(&p.y);
        let mut best: Option<(f32, IVec3)> = None;
        for _ in 0..10 {
            let Some(d) = direction_within(rng, f64::from(h), v, 0, view) else { continue };
            let p = (self.pos + d.as_dvec3()).floor().as_ivec3();
            // `LandRandomPos.generateRandomPosTowardDirection`: in the world
            // and a stable destination (solid to stand on) for flight.
            if !in_world(p) || !solid(p) {
                continue;
            }
            // `RandomPos.moveUpToAboveSolid` by 1..=3.
            let above = rng.next_int((above_max - above_min + 1) as u32) as i32 + above_min;
            let mut q = p + IVec3::Y;
            while q.y < crate::world::chunk::WORLD_MAX_Y && solid(q) {
                q.y += 1;
            }
            let first = q.y;
            while q.y < crate::world::chunk::WORLD_MAX_Y && q.y - first < above {
                q.y += 1;
                if solid(q) {
                    q.y -= 1;
                    break;
                }
            }
            if !open(q) {
                continue;
            }
            let value = self.walk_value(world, ctx, q);
            if value > best.map_or(f32::NEG_INFINITY, |(b, _)| b) {
                best = Some((value, q));
            }
        }
        if best.is_some() {
            return best.map(|(_, p)| p);
        }
        let (h, v, flying) = AIR_AND_WATER;
        for _ in 0..10 {
            let Some(d) = direction_within(rng, f64::from(h), v, flying, view) else { continue };
            let mut p = (self.pos + d.as_dvec3()).floor().as_ivec3();
            if !in_world(p) {
                continue;
            }
            // `RandomPos.moveUpOutOfSolid`.
            while p.y < crate::world::chunk::WORLD_MAX_Y && solid(p) {
                p.y += 1;
            }
            let caps = self.caps(false);
            if caps.malus.get(super::path::static_type(world, p)) != 0.0 {
                continue;
            }
            let value = self.walk_value(world, ctx, p);
            if value > best.map_or(f32::NEG_INFINITY, |(b, _)| b) {
                best = Some((value, p));
            }
        }
        best.map(|(_, p)| p)
    }

    /// `getViewVector(0)`'s horizontal part.
    fn look_dir(&self) -> DVec3 {
        let yrot = f64::from(self.yrot()).to_radians();
        DVec3::new(-yrot.sin(), 0.0, yrot.cos())
    }
}

/// `RandomPos.generateRandomDirectionWithinRadians(random, 0, max,
/// vertical, flying, view.x, view.z, PI / 2)`.
fn direction_within(rng: &mut Rng, max: f64, vertical: i32, flying: i32, view: DVec3) -> Option<IVec3> {
    let centre = view.z.atan2(view.x) - f64::from(std::f32::consts::FRAC_PI_2);
    let yrad = centre + f64::from(2.0 * rng.next_f32() - 1.0) * f64::from(std::f32::consts::FRAC_PI_2);
    let dist = f64::from(rng.next_f32()).sqrt() * max * f64::from(std::f32::consts::SQRT_2);
    let (x, z) = (-dist * yrad.sin(), dist * yrad.cos());
    if x.abs() > max || z.abs() > max {
        return None;
    }
    let y = rng.next_int((2 * vertical + 1) as u32) as i32 - vertical + flying;
    Some(IVec3::new(x.floor() as i32, y, z.floor() as i32))
}

// -------------------------------------------------------------------- skulls

/// `AbstractHurtingProjectile.INITAL_ACCELERATION_POWER`, `getInertia`
/// (`WitherSkull`'s 0.73 when dangerous) and `getLiquidInertia`.
pub const ACCELERATION_POWER: f64 = 0.1;
pub const SKULL_INERTIA: f32 = 0.95;
pub const DANGEROUS_INERTIA: f32 = 0.73;
pub const LIQUID_INERTIA: f32 = 0.8;
/// `WitherSkull.getBlockExplosionResistance`: a dangerous skull's cap.
pub const DANGEROUS_RESISTANCE: f32 = 0.8;
/// `onHitEntity`: Wither II for 10 seconds on Normal, 40 on Hard.
pub const SKULL_WITHER_SECONDS: [u32; 2] = [10, 40];

/// What a skull ran into this tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Block,
    Player(PlayerId),
    Mob(usize),
    /// v1.0 gap Track 4c: an End crystal or a dragon part.
    Boss(super::dragon::Hit),
}

/// A flying wither skull (`WitherSkull`): black, or blue and `dangerous`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Skull {
    pub pos: DVec3,
    pub previous_pos: DVec3,
    /// `deltaMovement`, blocks a tick.
    pub motion: DVec3,
    pub dangerous: bool,
    /// The Wither that fired it (its uid).
    pub owner: Option<u32>,
    pub age: u32,
}

impl Skull {
    /// `AbstractHurtingProjectile(type, mob, direction)`: launched at the
    /// acceleration power, 0.1 a tick, along `dir`.
    pub fn new(from: DVec3, dir: DVec3, dangerous: bool, owner: Option<u32>) -> Self {
        Skull {
            pos: from,
            previous_pos: from,
            motion: dir.normalize_or(DVec3::Y) * ACCELERATION_POWER,
            dangerous,
            owner,
            age: 0,
        }
    }

    /// A LAN client's copy, drawn only.
    pub fn replica(pos: DVec3, previous_pos: DVec3, dangerous: bool) -> Self {
        Skull { pos, previous_pos, motion: DVec3::ZERO, dangerous, owner: None, age: 0 }
    }

    /// `getInertia`: 0.73 for a dangerous skull, else 0.95; 0.8 in water.
    fn inertia(&self, in_water: bool) -> f64 {
        f64::from(if in_water {
            LIQUID_INERTIA
        } else if self.dangerous {
            DANGEROUS_INERTIA
        } else {
            SKULL_INERTIA
        })
    }

    /// `AbstractHurtingProjectile.tick`: speed up along the motion, then
    /// move, stopping at the first block, player or mob in the way (its
    /// owner and the Wither's friends excluded, as `canHitEntity`).
    pub fn tick<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        ctx: &Ctx,
        mobs: &[Mob],
        boss: &super::dragon::Boxes,
    ) -> Option<Hit> {
        self.previous_pos = self.pos;
        self.age += 1;
        let in_water = world.block(self.pos.floor().as_ivec3()).is_some_and(Block::holds_water);
        let m = self.motion;
        if m.length_squared() > 0.0 {
            self.motion = (m + m.normalize() * ACCELERATION_POWER) * self.inertia(in_water);
        }
        let delta = self.motion;
        let steps = (delta.length() / 0.2).ceil().max(1.0) as u32;
        for _ in 0..steps {
            let next = self.pos + delta / steps as f64;
            if world.block(next.floor().as_ivec3()).is_some_and(|b| b.is_solid()) {
                return Some(Hit::Block);
            }
            self.pos = next;
            // The skull's 0.3125-block box against bodies.
            const R: f64 = 0.15625;
            if let Some(t) = ctx.players.iter().find(|t| {
                t.alive && !t.spectator && {
                    let (lo, hi) = t.shape.aabb(t.pos);
                    next.cmpgt(lo - R).all() && next.cmplt(hi + R).all()
                }
            }) {
                return Some(Hit::Player(t.id));
            }
            if let Some(j) = mobs.iter().position(|o| {
                o.alive() && Some(o.uid) != self.owner && o.kind != MobKind::Wither && {
                    let (lo, hi) = o.aabb();
                    next.cmpgt(lo - R).all() && next.cmplt(hi + R).all()
                }
            }) {
                return Some(Hit::Mob(j));
            }
            if let Some(hit) = boss.hit_at(next, true) {
                return Some(Hit::Boss(hit));
            }
        }
        None
    }

    /// The skull's direction, for drawing.
    pub fn heading(&self) -> Vec3 {
        self.motion.as_vec3().normalize_or(Vec3::Z)
    }

    /// v0.16 Track H: what a saved skull keeps (`Entity.saveWithoutId`'s
    /// position and motion, `Projectile`'s owner and `WitherSkull`'s
    /// `dangerous`).
    pub fn save(&self) -> Value {
        json!({
            "pos": self.pos.to_array(),
            "motion": self.motion.to_array(),
            "dangerous": self.dangerous,
            "owner": self.owner,
        })
    }

    pub fn load(v: &Value) -> Option<Self> {
        let vec = |key: &str| -> Option<DVec3> {
            let a = v[key].as_array().filter(|a| a.len() == 3)?;
            let xyz: Option<Vec<f64>> = a.iter().map(Value::as_f64).collect();
            xyz.map(|c| DVec3::new(c[0], c[1], c[2])).filter(|p| p.is_finite())
        };
        let pos = vec("pos")?;
        Some(Skull {
            pos,
            previous_pos: pos,
            motion: vec("motion").unwrap_or(DVec3::ZERO),
            dangerous: v["dangerous"].as_bool().unwrap_or(false),
            owner: v["owner"].as_u64().and_then(|u| u32::try_from(u).ok()),
            age: 0,
        })
    }
}

impl Entities {
    /// v0.16 Track H: the skulls in flight, for the level's `wither_skulls`.
    pub(super) fn wither_skulls_json(&self) -> String {
        Value::Array(self.wither_skulls.iter().map(Skull::save).collect()).to_string()
    }

    pub(super) fn load_wither_skulls_json(&mut self, text: &str) {
        let Ok(Value::Array(list)) = serde_json::from_str::<Value>(text) else { return };
        self.wither_skulls.extend(list.iter().filter_map(Skull::load));
    }
}

/// The Nether star's item info (`Items.NETHER_STAR`).
pub fn is_nether_star(item: Item) -> bool {
    item == NETHER_STAR
}

#[cfg(test)]
mod tests;
