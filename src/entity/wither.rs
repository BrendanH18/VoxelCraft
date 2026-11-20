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
}

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
    let rows: [&[u8; 3]; 3] = [b"^^^", b"###", b"~#~"];
    let test = |c: u8, b: Option<Block>| match (c, b) {
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
            Foe::Player(id) => ctx
                .players
                .iter()
                .find(|t| t.id == id && t.alive && t.targetable)
                .map(|t| (t.pos, t.shape.height * 0.9)),
            Foe::Mob(uid) => self
                .mobs
                .iter()
                .find(|m| m.uid == uid && m.alive() && !friend(m.kind))
                .map(|m| (m.pos, m.shape().height * 0.85)),
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
                    out.push((t.pos.distance_squared(from), Foe::Player(t.id), t.pos, t.shape.height * 0.9));
                }
            }
        }
        self.mob_index.visit(w.pos, half.x.max(half.y) + 4.0, |j| {
            let m = &self.mobs[j];
            if j != me && m.alive() && !friend(m.kind) && inside(m.pos, m.shape().height) {
                let d2 = m.pos.distance_squared(w.pos);
                if d2 <= range * range {
                    out.push((m.pos.distance_squared(from), Foe::Mob(m.uid), m.pos, m.shape().height * 0.85));
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
        // Targets (`HurtByTargetGoal`, then `NearestAttackableTargetGoal`).
        let main = s.targets[0].and_then(|f| self.foe_at(ctx, f).map(|at| (f, at)));
        let main = match main {
            Some((f, (p, _))) if p.distance_squared(self.mobs[i].pos) <= FOLLOW_RANGE * FOLLOW_RANGE => Some(f),
            _ => {
                let range = DVec3::splat(FOLLOW_RANGE);
                self.wither_candidates(world, ctx, i, range, FOLLOW_RANGE).first().map(|&(f, _)| f)
            }
        };
        let main_at = main.and_then(|f| self.foe_at(ctx, f));
        let w_pos = self.mobs[i].pos;
        let body_yrot = self.mobs[i].yrot();
        let mut shots: Vec<(usize, DVec3, bool)> = Vec::new();
        // `RangedAttackGoal.tick` for head 0.
        {
            let s = self.mobs[i].wither.as_mut().unwrap();
            s.targets[0] = main;
            s.target_at = main_at.map(|(p, _)| p);
            if let Some((p, eye_h)) = main_at {
                let aim = p + DVec3::Y * (eye_h * 0.5);
                let d2 = p.distance_squared(w_pos);
                let sees = super::mob::line_of_sight(world, eye, p + DVec3::Y * eye_h);
                s.see_time = if sees { s.see_time + 1 } else { 0 };
                // Out of range or unseen: fly straight at it (`moveTo`).
                s.wanted = (d2 > ATTACK_RADIUS * ATTACK_RADIUS || s.see_time < 5).then_some(p);
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
            } else {
                s.see_time = 0;
                s.attack_time = -1;
            }
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
        self.wither_skulls.push(Skull::new(from, dir, dangerous, Some(w.uid)));
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

    /// Who hurt a Wither becomes its main target (`HurtByTargetGoal`).
    pub(super) fn wither_hurt_by(&mut self, index: usize, player: PlayerId) {
        if let Some(s) = self.mobs[index].wither.as_mut() {
            s.targets[0] = Some(Foe::Player(player));
        }
    }

    /// Moves the skulls; a hit becomes damage, Wither and a blast.
    pub(super) fn update_wither_skulls<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        ctx: &Ctx,
        events: &mut Vec<EntityEvent>,
    ) {
        if self.wither_skulls.is_empty() {
            return;
        }
        let mut skulls = std::mem::take(&mut self.wither_skulls);
        let difficulty = self.difficulty;
        skulls.retain_mut(|k| {
            if !world.entity_ticking(k.pos.floor().as_ivec3()) {
                return true;
            }
            let owner_alive =
                k.owner.is_none_or(|u| self.mobs.iter().any(|m| m.uid == u && m.kind == MobKind::Wither && m.alive()));
            if !owner_alive {
                return false;
            }
            match k.tick(world, ctx, &self.mobs) {
                None => k.age < SKULL_LIFETIME,
                Some(hit) => {
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
            let eye = m.pos + DVec3::Y * (m.shape().height * 0.85);
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

    /// `WaterAvoidingRandomFlyingGoal` while it has no target: now and then
    /// a spot within 8 blocks ahead and up to 7 up.
    pub(super) fn wither_wander(&mut self, rng: &mut Rng) {
        let Some(s) = self.wither.as_mut() else { return };
        if s.invul > 0 || s.target_at.is_some() || s.wanted.is_some() || rng.next_int(60) != 0 {
            return;
        }
        let yrot = (self.yaw.to_degrees() - 90.0 + rng.range(-90.0, 90.0)).to_radians();
        let r = rng.range(1.0, 8.0) as f64;
        let dir = DVec3::new(-(yrot as f64).sin(), 0.0, (yrot as f64).cos());
        let up = rng.range(-2.0, 7.0) as f64;
        s.wanted = Some(self.pos + dir * r + DVec3::Y * up);
    }
}

// -------------------------------------------------------------------- skulls

/// Ticks a skull flies before it's dropped; Java keeps one until it hits.
const SKULL_LIFETIME: u32 = 1200;
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
    pub fn tick<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx, mobs: &[Mob]) -> Option<Hit> {
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
        }
        None
    }

    /// The skull's direction, for drawing.
    pub fn heading(&self) -> Vec3 {
        self.motion.as_vec3().normalize_or(Vec3::Z)
    }
}

/// The Nether star's item info (`Items.NETHER_STAR`).
pub fn is_nether_star(item: Item) -> bool {
    item == NETHER_STAR
}

#[cfg(test)]
mod tests;
