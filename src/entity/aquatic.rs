//! Water creatures and structure residents. Timers are in seconds; bucket
//! components and one-time population markers survive save/load.
use super::mob::Ai;
use super::{Ctx, Entities, EntityEvent, Mob, MobKind, MobWorld, Rng};
#[cfg(test)]
use crate::world::{
    temples::Kind,
    terrain::{Biome, SEA_LEVEL},
};
use crate::{
    inventory::{Inventory, Stack},
    item::Item,
    physics,
    simulation::{difficulty::Difficulty, effects::Effect},
    world::{World, block::Block, terrain::Dimension},
};
use glam::{DVec3, IVec3};

#[derive(Clone, Debug)]
pub struct State {
    pub variant: u8,
    pub puff: u8,
    pub beam: Option<DVec3>,
    pub charge: f32,
    target: Option<super::PlayerId>,
    prey: Option<(u32, DVec3)>,
    sense_timer: f32,
    pub(super) play_dead: f32,
    air: f32,
    dry: f32,
    damage_tick: f32,
    effect_timer: f32,
    contact_timer: f32,
    /// Squid: `movementVector` (blocks a tick) and the tentacle cycle.
    pub(super) vector: DVec3,
    pub(super) tentacle: f32,
    pub(super) tentacle_speed: f32,
    /// Schooling fish: the leader followed (uid and position), and the
    /// school this fish leads, itself included (`schoolSize`).
    pub(super) leader: Option<(u32, DVec3)>,
    pub(super) school: u8,
    /// `FollowFlockLeaderGoal.nextStartTick` and `timeToRecalcPath`, in
    /// selector updates.
    pub(super) flock_start: i32,
    pub(super) flock_recalc: i32,
}
impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}
impl State {
    pub fn new() -> Self {
        Self {
            target: None,
            prey: None,
            sense_timer: 0.0,
            play_dead: 0.0,
            variant: 0,
            puff: 0,
            beam: None,
            charge: 0.0,
            air: 240.0,
            dry: 0.0,
            damage_tick: 0.0,
            effect_timer: 0.0,
            contact_timer: 0.0,
            vector: DVec3::ZERO,
            tentacle: 0.0,
            tentacle_speed: 0.1,
            leader: None,
            school: 1,
            flock_start: 0,
            flock_recalc: 0,
        }
    }
}
pub(super) fn on_spawn(m: &mut Mob, rng: &mut Rng) {
    if let Some(a) = &mut m.aquatic {
        a.variant = rng.next_int(if m.kind == MobKind::Axolotl { 4 } else { 12 }) as u8;
    }
}
fn wet<W: physics::BlockSource + ?Sized>(w: &W, p: DVec3) -> bool {
    w.block(p.floor().as_ivec3()).is_some_and(Block::holds_water)
}

pub(super) fn environment<W: MobWorld + ?Sized>(m: &mut Mob, dt: f32, w: &W) {
    if !m.alive() {
        return;
    }
    let Some(a) = &mut m.aquatic else { return };
    let submerged = wet(w, m.pos + DVec3::Y * (m.kind.shape().height * 0.8));
    let dolphin = m.kind == MobKind::Dolphin;
    let axolotl = m.kind == MobKind::Axolotl;
    let guardian = matches!(m.kind, MobKind::Guardian | MobKind::ElderGuardian);
    if guardian {
        return;
    }
    if dolphin {
        a.air = if submerged { a.air - dt } else { 240.0 };
        a.dry = if wet(w, m.pos) || w.rains_on(m.pos.floor().as_ivec3()) { 0.0 } else { a.dry + dt };
    } else {
        a.dry = if submerged || (axolotl && w.rains_on(m.pos.floor().as_ivec3())) { 0.0 } else { a.dry + dt };
    }
    let suffocating =
        if dolphin { a.air <= 0.0 || a.dry >= 120.0 } else { a.dry >= if axolotl { 300.0 } else { 15.0 } };
    if suffocating {
        a.damage_tick += dt;
        if a.damage_tick >= 1.0 {
            m.health -= m.effects.absorb(2.0 * m.effects.resistance_factor());
            a.damage_tick %= 1.0;
        }
    } else {
        a.damage_tick = 0.0;
    }
}

/// Effects and attacks that run whatever the mob is doing: the elder
/// guardian's Mining Fatigue, the guardian beam, the pufferfish's puffing
/// and stings, and the dolphin's grace. Returns `true` while the mob must
/// hold still: a guardian aiming its beam (`GuardianAttackGoal` stops its
/// navigation) or an axolotl playing dead.
pub(super) fn upkeep<W: MobWorld + ?Sized>(
    m: &mut Mob,
    dt: f32,
    w: &W,
    ctx: &Ctx,
    events: &mut Vec<EntityEvent>,
) -> bool {
    let shape = m.shape();
    let eye = m.pos + DVec3::Y * shape.height * 0.5;
    let a = m.aquatic.as_mut().unwrap();
    a.effect_timer -= dt;
    a.contact_timer -= dt;
    let guardian = matches!(m.kind, MobKind::Guardian | MobKind::ElderGuardian);
    if m.kind == MobKind::ElderGuardian && a.effect_timer <= 0.0 {
        a.effect_timer = 60.0;
        for t in ctx.players.iter().filter(|t| t.targetable && t.alive && t.pos.distance_squared(m.pos) < 2500.0) {
            events.push(EntityEvent::PlayerEffect {
                player: t.id,
                effect: Effect::MiningFatigue,
                amplifier: 2,
                ticks: 6000,
            });
        }
    }
    if guardian {
        let target = ctx
            .players
            .iter()
            .filter(|t| t.targetable && t.alive)
            .filter(|t| {
                let d = t.pos.distance_squared(m.pos);
                d < 256.0
                    && (m.kind == MobKind::ElderGuardian || d > 9.0)
                    && super::mob::line_of_sight(w, eye, t.pos + DVec3::Y)
            })
            .min_by(|l, r| l.pos.distance_squared(m.pos).total_cmp(&r.pos.distance_squared(m.pos)));
        if let Some(t) = target {
            let to = t.pos + DVec3::Y;
            if a.target != Some(t.id) {
                a.charge = -0.5;
            }
            a.target = Some(t.id);
            a.beam = Some(to);
            a.charge += dt;
            m.yaw = ((to.z - eye.z) as f32).atan2((to.x - eye.x) as f32);
            let duration = if m.kind == MobKind::ElderGuardian { 3.0 } else { 4.0 };
            if a.charge >= duration {
                let magic = if m.difficulty == Difficulty::Hard { 3.0 } else { 1.0 };
                events.push(EntityEvent::PlayerBeam {
                    player: t.id,
                    physical: if m.kind == MobKind::ElderGuardian { 8.0 } else { 6.0 },
                    magic,
                });
                a.charge = -1.0;
            }
            m.stop_navigation();
            return true;
        }
        a.target = None;
        a.beam = None;
        a.charge = 0.0;
    }
    if m.kind == MobKind::Pufferfish {
        let near = ctx.players.iter().any(|t| t.targetable && t.alive && t.pos.distance_squared(m.pos) < 4.0);
        a.charge = if near { (a.charge + dt).min(2.0) } else { (a.charge - dt * 0.3).max(0.0) };
        a.puff = if a.charge > 0.5 { 2 } else { u8::from(a.charge > 0.0) };
        if a.puff > 0 && a.contact_timer <= 0.0 {
            for t in ctx.players.iter().filter(|t| t.targetable && t.alive && t.pos.distance_squared(m.pos) < 1.0) {
                events.push(EntityEvent::PlayerSting {
                    player: t.id,
                    damage: 1.0 + a.puff as f32,
                    cause: "was stung by a pufferfish",
                    poison: 60 * a.puff as u32,
                });
                a.contact_timer = 1.0;
            }
        }
    }
    if m.kind == MobKind::Dolphin && a.effect_timer <= 0.0 {
        a.effect_timer = 1.0;
        for t in ctx.players.iter().filter(|t| t.alive && t.pos.distance_squared(m.pos) < 81.0 && wet(w, t.pos)) {
            events.push(EntityEvent::PlayerEffect {
                player: t.id,
                effect: Effect::DolphinsGrace,
                amplifier: 0,
                ticks: 100,
            });
        }
    }
    if m.kind == MobKind::Axolotl && a.play_dead > 0.0 {
        a.play_dead -= dt;
        m.health = (m.health + dt * 0.4).min(m.kind.max_health());
        m.stop_navigation();
        return true;
    }
    false
}

/// `RandomStrollGoal.canUse` on the selector's full update: not idle for
/// 100 ticks, and a one in `reducedTickDelay(interval)` roll.
fn stroll_roll(m: &Mob, rng: &mut Rng, interval: u32) -> bool {
    m.idle < 5.0 && rng.next_int(interval.div_ceil(2)) == 0
}

/// Walk to `cell` (`moveTo(x, y, z, speed)`, within a block) and mark the
/// stroll as running.
fn start_stroll<W: MobWorld + ?Sized>(m: &mut Mob, w: &W, cell: IVec3, speed: f64) {
    if m.walk_to(w, cell, 1, speed, false) {
        m.ai = Ai::Wander;
        m.ai_timer = 30.0;
        m.path_move = true;
    }
}

/// The movement goals of swimmers with no running table goal, steering by
/// navigation into their `MoveControl` (`mob::swim`):
///
/// - fish: `PanicGoal` (1.25), `FishSwimGoal` (`RandomSwimmingGoal`, 1.0
///   every 40 ticks) and, for schooling fish, `FollowFlockLeaderGoal`;
/// - dolphins: `BreathAirGoal` below 140 ticks of air, then
///   `RandomSwimmingGoal` (1.0 every 10 ticks);
/// - guardians: `RandomStrollGoal` (1.0 every 80 ticks);
/// - squid: `SquidRandomMovementGoal` and `SquidFleeGoal`;
/// - axolotls: the brain's attack (0.6 in water, 0.15 on land) and its idle
///   `RandomStroll.swim(0.5)` or `stroll(0.15)` whenever it has no walk target.
///
/// Returns the ground steering for axolotls on land.
pub(super) fn steer<W: MobWorld + ?Sized>(
    m: &mut Mob,
    dt: f32,
    w: &W,
    ctx: &Ctx,
    rng: &mut Rng,
    events: &mut Vec<EntityEvent>,
) -> (Option<DVec3>, f64) {
    // Kinds without a goal table count their own selector ticks.
    if super::mob::goals::table(m.kind).is_empty() {
        m.goals.ticks = m.goals.ticks.wrapping_add(1);
    }
    let t = m.goals.ticks.wrapping_sub(1);
    let full = t.wrapping_add(m.uid).is_multiple_of(2) || t <= 1;
    let speed = |m: &Mob, modifier: f64| m.goal_speed(modifier);
    match m.kind {
        MobKind::Squid | MobKind::GlowSquid => {
            squid(m, w, ctx, rng, full);
            return (None, 0.0);
        }
        MobKind::Axolotl => return axolotl(m, w, ctx, rng, events),
        _ => {}
    }
    if m.kind == MobKind::Dolphin {
        // `BreathAirGoal`: air below 140 ticks; it can't be interrupted.
        let breathe = m.aquatic.as_ref().is_some_and(|a| a.air < 7.0);
        if breathe {
            if !m.move_ctl.breathing {
                m.stop_navigation();
                m.ai = Ai::Idle;
                m.path_move = false;
            }
            m.move_ctl.breathing = true;
            let goal = air_above(w, m.pos);
            let s = speed(m, 1.0);
            m.walk_to(w, goal + IVec3::Y, 1, s, false);
            return (m.follow_path(w, s, false), s);
        }
        m.move_ctl.breathing = false;
    }
    if m.kind.is_fish() && m.ai == Ai::Panic {
        // `PanicGoal`: a fresh spot five blocks out each time the last is
        // reached, until the hit is forgotten and the path is done.
        m.ai_timer -= dt;
        if m.ai_timer <= 0.0 && m.nav.is_done() {
            m.ai = Ai::Idle;
            m.path_move = false;
            m.stop_navigation();
        } else {
            let s = m.panic_speed();
            if m.ai_timer > 0.0
                && m.nav.is_done()
                && let Some(p) = m.default_random_pos(w, ctx, rng, 5, 4)
            {
                m.walk_to(w, p, 1, s, false);
            }
            return (m.follow_path(w, s, false), s);
        }
    } else if !m.kind.is_fish() && m.ai == Ai::Panic {
        m.ai = Ai::Idle;
    }
    let following = m.aquatic.as_ref().and_then(|a| a.leader);
    if m.ai == Ai::Wander && m.path_move {
        let s = m.stroll_speed();
        match m.follow_path(w, s, false) {
            Some(dir) if m.ai_timer > 0.0 => {
                m.ai_timer -= dt;
                if following.is_none() {
                    return (Some(dir), s);
                }
            }
            _ => {
                m.ai = Ai::Idle;
                m.path_move = false;
                m.stop_navigation();
            }
        }
    }
    // `FollowFlockLeaderGoal` (no flags: it runs beside the swim goal): path
    // to the leader every `adjustedTickDelay(10)`.
    if let Some((_, leader)) = following {
        let s = speed(m, 1.0);
        if full {
            let a = m.aquatic.as_mut().unwrap();
            a.flock_recalc -= 1;
            if a.flock_recalc <= 0 {
                a.flock_recalc = 5;
                m.walk_to(w, leader.floor().as_ivec3(), 1, s, false);
            }
        }
        return (m.follow_path(w, s, false), s);
    }
    if m.ai == Ai::Idle && full {
        let (interval, swimmable) = match m.kind {
            MobKind::Dolphin => (10, true),
            MobKind::Guardian | MobKind::ElderGuardian => (80, false),
            _ => (40, true),
        };
        if stroll_roll(m, rng, interval) {
            let target =
                if swimmable { m.swimmable_pos(w, ctx, rng, 10, 7) } else { m.default_random_pos(w, ctx, rng, 10, 7) };
            if let Some(p) = target {
                let s = m.stroll_speed();
                start_stroll(m, w, p, s);
                return (m.follow_path(w, s, false), s);
            }
        }
    }
    (None, 0.0)
}

/// `BreathAirGoal.findAirPosition`: the first cell giving air in the
/// mob's column and the four beside it, up to 8 blocks up; else 8 up.
fn air_above<W: MobWorld + ?Sized>(w: &W, pos: DVec3) -> IVec3 {
    let base = pos.floor().as_ivec3();
    for step in [IVec3::ZERO, IVec3::NEG_Z, IVec3::X, IVec3::Z, IVec3::NEG_X] {
        for dy in 0..=8 {
            let p = base + step + IVec3::Y * dy;
            if w.block(p).is_some_and(|b| !b.is_fluid() && !b.is_waterlogged() && !b.is_solid()) {
                return p;
            }
        }
    }
    (pos + DVec3::Y * 8.0).floor().as_ivec3()
}

/// `SquidRandomMovementGoal` on full updates and `SquidFleeGoal` every
/// tick. The attacker is the nearest player for five seconds after a player
/// hit (`getLastHurtByMob` lasts 100 ticks).
fn squid<W: MobWorld + ?Sized>(m: &mut Mob, w: &W, ctx: &Ctx, rng: &mut Rng, full: bool) {
    let in_water = m.in_water;
    let idle = m.idle;
    let pos = m.pos;
    let attacker = (m.player_hit_left > 0.0)
        .then(|| {
            ctx.players
                .iter()
                .filter(|t| t.alive)
                .min_by(|l, r| l.pos.distance_squared(pos).total_cmp(&r.pos.distance_squared(pos)))
                .map(|t| t.pos)
        })
        .flatten();
    let a = m.aquatic.as_mut().unwrap();
    if full {
        if idle > 5.0 {
            a.vector = DVec3::ZERO;
        } else if rng.next_int(25) == 0 || !in_water || a.vector.length_squared() <= 1e-5 {
            let angle = rng.next_f32() * std::f32::consts::TAU;
            a.vector = DVec3::new(
                (angle.cos() * 0.2) as f64,
                (-0.1 + rng.next_f32() * 0.2) as f64,
                (angle.sin() * 0.2) as f64,
            );
        }
    }
    if let Some(from) = attacker.filter(|f| in_water && f.distance_squared(pos) < 100.0) {
        let mut flee = pos - from;
        let probe = (pos + flee).floor().as_ivec3();
        let block = w.block(probe);
        let air = block == Some(Block::AIR);
        if air || block.is_some_and(Block::holds_water) {
            let len = flee.length();
            if len > 0.0 {
                // Java normalizes into a discarded vector: only the scale applies.
                let mut avoid = 3.0;
                if len > 5.0 {
                    avoid -= (len - 5.0) / 5.0;
                }
                if avoid > 0.0 {
                    flee *= avoid;
                }
            }
            if air {
                flee.y = 0.0;
            }
            a.vector = flee / 20.0;
        }
    }
}

/// The axolotl brain's movement: `SetWalkTargetFromAttackTargetIfTargetOutOfReach`
/// and `MeleeAttack(20)` against prey, else an idle swim or stroll.
fn axolotl<W: MobWorld + ?Sized>(
    m: &mut Mob,
    w: &W,
    ctx: &Ctx,
    rng: &mut Rng,
    events: &mut Vec<EntityEvent>,
) -> (Option<DVec3>, f64) {
    let in_water = m.in_water;
    if let Some((uid, pos)) = m.aquatic.as_ref().and_then(|a| a.prey).filter(|(_, pos)| wet(w, *pos)) {
        let delta = pos - m.pos;
        if delta.length_squared() < 1.5 && m.attack_cooldown <= 0.0 {
            m.attack_cooldown = 1.0;
            events.push(EntityEvent::MobHit {
                target: uid,
                attacker: m.uid,
                damage: 2.0,
                knockback: delta.normalize_or_zero() * 0.3,
            });
        }
        let s = m.goal_speed(0.6);
        return (m.pursue(w, pos, s, true, true, rng), s);
    }
    // `RandomStroll.swim(0.5)` in water, `stroll(0.15)` on land, whenever
    // there is no walk target (`MoveToTargetSink` erases it on arrival).
    let s = m.goal_speed(0.5);
    if m.nav.is_done() {
        let target = if in_water {
            m.swim_stroll_pos(w, ctx, rng).map(|p| p.floor().as_ivec3())
        } else {
            m.land_random_pos(w, ctx, rng)
        };
        if let Some(p) = target {
            m.walk_to(w, p, 0, s, false);
        }
    }
    (m.follow_path(w, s, false), s)
}

pub(super) fn pillager<W: MobWorld + ?Sized>(
    m: &mut Mob,
    _dt: f32,
    w: &W,
    ctx: &Ctx,
    events: &mut Vec<EntityEvent>,
) -> (Option<DVec3>, f64) {
    let Some(t) = ctx.nearest_target(m.pos) else { return (None, 0.0) };
    let delta = t.pos - m.pos;
    if delta.length_squared() > 1024.0 {
        return (None, 0.0);
    }
    let from = m.pos + DVec3::Y * 1.55;
    let visible = super::mob::line_of_sight(w, from, t.pos + DVec3::Y);
    m.charged = visible && delta.length_squared() < 225.0;
    if m.charged && m.attack_cooldown <= 0.0 {
        events.push(EntityEvent::Shoot {
            from,
            target: t.pos + DVec3::Y,
            cause: "was shot by a pillager",
            shooter: m.uid,
        });
        m.attack_cooldown = 2.5;
        m.attack_anim = 0.4;
    }
    let dir = (delta * DVec3::new(1.0, 0.0, 1.0)).normalize_or_zero();
    m.yaw = (dir.z as f32).atan2(dir.x as f32);
    (if m.charged { None } else { Some(dir) }, 2.4)
}

#[cfg(test)]
pub(super) fn spawn_spot<W: MobWorld + ?Sized>(
    w: &W,
    kind: MobKind,
    x: i32,
    z: i32,
    center_y: i32,
    rng: &mut Rng,
) -> Option<DVec3> {
    let underground =
        matches!(kind, MobKind::GlowSquid | MobKind::Axolotl) || (kind == MobKind::TropicalFish && center_y < 40);
    let (lo, hi) = if underground {
        ((center_y - 12).max(-63), (center_y + 12).min(if kind == MobKind::GlowSquid { 29 } else { 62 }))
    } else if kind == MobKind::Guardian {
        (39, 62)
    } else {
        (50, SEA_LEVEL - 1)
    };
    if lo > hi {
        return None;
    }
    let y = lo + rng.next_int((hi - lo + 1) as u32) as i32;
    let p = IVec3::new(x, y, z);
    let pos = p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
    if !w.loaded(p)
        || !wet(w, pos)
        || !wet(w, pos + DVec3::Y * kind.shape().height)
        || physics::overlaps_solid(w, pos, kind.shape())
    {
        return None;
    }
    match kind {
        MobKind::Guardian
            if !w.structures_near(p, 0).iter().any(|b| b.kind == Kind::Monument && b.spawns.contains(p)) =>
        {
            return None;
        }
        MobKind::GlowSquid
            if w.block_light(p) != 0
                || super::sky_light(w, pos) > 0.0
                || !(1..=5).any(|dy| {
                    w.block(p - IVec3::Y * dy).is_some_and(|b| matches!(b, Block::STONE | Block::DEEPSLATE))
                }) =>
        {
            return None;
        }
        MobKind::Axolotl
            if w.cave_biome(p) != Biome::LushCaves
                || !(1..=5).any(|dy| w.block(p - IVec3::Y * dy) == Some(Block::CLAY)) =>
        {
            return None;
        }
        MobKind::TropicalFish if underground && w.cave_biome(p) != Biome::LushCaves => return None,
        _ => {}
    }
    Some(pos)
}

/// `getMaxSchoolSize`: salmon 5, other schooling fish their spawn cluster of 8.
fn max_school(kind: MobKind) -> u8 {
    if kind == MobKind::Salmon { 5 } else { 8 }
}

pub fn bucket_kind(item: Item) -> Option<MobKind> {
    Some(match item {
        Item::COD_BUCKET => MobKind::Cod,
        Item::SALMON_BUCKET => MobKind::Salmon,
        Item::TROPICAL_FISH_BUCKET => MobKind::TropicalFish,
        Item::PUFFERFISH_BUCKET => MobKind::Pufferfish,
        Item::AXOLOTL_BUCKET => MobKind::Axolotl,
        _ => return None,
    })
}
fn bucket_item(kind: MobKind) -> Option<Item> {
    Some(match kind {
        MobKind::Cod => Item::COD_BUCKET,
        MobKind::Salmon => Item::SALMON_BUCKET,
        MobKind::TropicalFish => Item::TROPICAL_FISH_BUCKET,
        MobKind::Pufferfish => Item::PUFFERFISH_BUCKET,
        MobKind::Axolotl => Item::AXOLOTL_BUCKET,
        _ => return None,
    })
}

/// Java's baby turtle growing time: 20 minutes.
pub const TURTLE_GROW_SECS: f32 = 1200.0;

impl Entities {
    /// Baby turtles grow up after twenty minutes and shed a scute.
    #[cfg(test)]
    pub(super) fn grow_turtles(&mut self, dt: f32) {
        self.grow_turtles_matching(dt, |_| true);
    }
    pub(super) fn grow_loaded_turtles<W: MobWorld + ?Sized>(&mut self, dt: f32, w: &W, _ctx: &Ctx) {
        self.grow_turtles_matching(dt, |m| w.entity_ticking(m.pos.floor().as_ivec3()));
    }
    fn grow_turtles_matching(&mut self, dt: f32, mut active: impl FnMut(&Mob) -> bool) {
        let mut scutes = Vec::new();
        for m in &mut self.mobs {
            if m.kind == MobKind::Turtle && m.baby && m.alive() && active(m) {
                m.grow += dt;
                m.age = -((TURTLE_GROW_SECS - m.grow).max(0.0) * 20.0) as i32;
                if m.grow >= TURTLE_GROW_SECS {
                    m.baby = false;
                    m.age = 0;
                    scutes.push(m.pos);
                }
            }
        }
        for pos in scutes {
            self.drop_from_block(Stack::new(Item::TURTLE_SCUTE, 1), pos.floor().as_ivec3());
        }
    }

    /// Turtles hatching from eggs at `cell`.
    pub fn hatch_turtles(&mut self, cell: glam::IVec3, count: u8) {
        for i in 0..count {
            let pos = cell.as_dvec3() + DVec3::new(0.3 + i as f64 * 0.15, 0.0, 0.5);
            self.spawn(MobKind::Turtle, pos);
            let m = self.mobs.last_mut().unwrap();
            m.baby = true;
            m.age = -24000;
            m.persistent = true;
        }
    }

    /// Java `Bucketable.bucketMobPickup`: a water bucket used on a bucketable
    /// creature stores its variant and health in the filled bucket. It is an
    /// entity interaction, so it has no build-permission check. Walls stop the
    /// capture ray; the shared bucket action releases payloads.
    #[allow(clippy::too_many_arguments)]
    pub fn capture_water_creature(
        &mut self,
        world: &World,
        inventory: &mut Inventory,
        slot: usize,
        creative: bool,
        eye: DVec3,
        dir: DVec3,
        reach: f64,
    ) -> bool {
        if inventory.get(slot).is_none_or(|stack| stack.item != Item::WATER_BUCKET) {
            return false;
        }
        let distance =
            world.raycast(eye, dir, reach).map_or(reach, |(p, _)| (p.as_dvec3() + DVec3::splat(0.5)).distance(eye));
        let Some((i, _)) = self.raycast(eye, dir, distance) else { return false };
        let m = &self.mobs[i];
        let Some(item) = bucket_item(m.kind) else { return false };
        let mut filled = Stack::new(item, 1);
        filled.entity_data = 1
            | ((m.aquatic.as_ref().unwrap().variant as u32) << 1)
            | (((m.health.clamp(0.0, 255.0) * 100.0).round() as u32) << 9);
        self.mobs.swap_remove(i);
        if !creative {
            inventory.slots[slot] = Some(filled);
        } else if inventory.add_stack(filled) > 0 {
            self.throw(filled, eye, dir);
        }
        true
    }
    /// Dispensers use the same bucket payload as players.
    pub fn release_water_creature(&mut self, stack: Stack, pos: DVec3) -> bool {
        let Some(kind) = bucket_kind(stack.item) else { return false };
        self.spawn(kind, pos);
        let m = self.mobs.last_mut().unwrap();
        m.persistent = true;
        if stack.entity_data & 1 != 0 {
            m.aquatic.as_mut().unwrap().variant = ((stack.entity_data >> 1) & 255) as u8;
            m.health = ((stack.entity_data >> 9) as f32 / 100.0).clamp(0.01, kind.max_health());
        }
        true
    }
    pub(super) fn aquatic_sense<W: MobWorld + ?Sized>(&mut self, dt: f32, w: &W) {
        if !self.mobs.iter().any(|m| m.aquatic.is_some()) {
            return;
        }
        if self.nether_view.is_empty() {
            self.nether_view.extend(self.mobs.iter().filter(|m| m.alive()).map(|m| super::nether::Seen {
                uid: m.uid,
                kind: m.kind,
                pos: m.pos,
                baby: m.baby,
                huntable: false,
            }));
        }
        for m in &mut self.mobs {
            if !w.entity_ticking(m.pos.floor().as_ivec3()) {
                continue;
            }
            let Some(a) = &mut m.aquatic else { continue };
            a.sense_timer -= dt;
            if a.sense_timer > 0.0 {
                continue;
            }
            a.sense_timer = 0.5;
            a.prey = None;
            if m.kind == MobKind::Axolotl {
                a.prey = self
                    .nether_view
                    .iter()
                    .filter(|v| {
                        v.uid != m.uid
                            && matches!(
                                v.kind,
                                MobKind::Cod
                                    | MobKind::Salmon
                                    | MobKind::TropicalFish
                                    | MobKind::Squid
                                    | MobKind::GlowSquid
                                    | MobKind::Guardian
                                    | MobKind::Drowned
                            )
                            && v.pos.distance_squared(m.pos) < 64.0
                            && wet(w, v.pos)
                    })
                    .min_by(|l, r| l.pos.distance_squared(m.pos).total_cmp(&r.pos.distance_squared(m.pos)))
                    .map(|v| (v.uid, v.pos));
            }
        }
    }
    /// `AbstractSchoolingFish` schools, after the goal searches and before
    /// fish think. Followers track their leader and drop one that died, left
    /// the level or swam past 11 blocks (`FollowFlockLeaderGoal`'s
    /// `canContinueToUse`); each leader's `schoolSize` is recounted from its
    /// followers. On its selector's full update, a fish that neither leads
    /// nor follows counts down `nextStartTick`, then joins a leader with room
    /// within 8 blocks or gathers the loose fish around it (`canUse`).
    pub(super) fn sense_schools(&mut self) {
        let schooling = |k: MobKind| matches!(k, MobKind::Cod | MobKind::Salmon | MobKind::TropicalFish);
        if !self.mobs.iter().any(|m| schooling(m.kind)) {
            return;
        }
        let by_uid: rustc_hash::FxHashMap<u32, usize> = self
            .mobs
            .iter()
            .enumerate()
            .filter(|(_, m)| schooling(m.kind) && m.alive())
            .map(|(j, m)| (m.uid, j))
            .collect();
        let mut counts: rustc_hash::FxHashMap<u32, u8> = Default::default();
        for i in 0..self.mobs.len() {
            let pos = self.mobs[i].pos;
            let Some(a) = self.mobs[i].aquatic.as_deref() else { continue };
            let Some((uid, _)) = a.leader else { continue };
            let leader = by_uid.get(&uid).map(|&j| self.mobs[j].pos).filter(|p| p.distance_squared(pos) <= 121.0);
            let a = self.mobs[i].aquatic.as_mut().unwrap();
            a.leader = leader.map(|p| (uid, p));
            if leader.is_some() {
                *counts.entry(uid).or_default() += 1;
            }
        }
        for m in &mut self.mobs {
            if let Some(a) = &mut m.aquatic {
                a.school = 1 + counts.get(&m.uid).copied().unwrap_or(0);
            }
        }
        for i in 0..self.mobs.len() {
            let m = &self.mobs[i];
            if !schooling(m.kind) || !m.alive() || !m.simulation_active || !m.goals.full(m.uid) {
                continue;
            }
            let a = m.aquatic.as_deref().unwrap();
            if a.school > 1 || a.leader.is_some() {
                continue;
            }
            if a.flock_start > 0 {
                self.mobs[i].aquatic.as_mut().unwrap().flock_start -= 1;
                continue;
            }
            let (kind, pos, shape) = (m.kind, m.pos, m.shape());
            // `reducedTickDelay(200 + nextInt(200) % 20)`.
            let next = (200 + self.rng.next_int(200) % 20).div_ceil(2) as i32;
            self.mobs[i].aquatic.as_mut().unwrap().flock_start = next;
            let max = max_school(kind);
            let mut near = Vec::new();
            self.mob_index.visit(pos, 9.0, |j| {
                let b = &self.mobs[j];
                if b.kind != kind || !b.alive() {
                    return;
                }
                let d = b.pos - pos;
                let across = 8.0 + shape.half_width + b.shape().half_width;
                let a = b.aquatic.as_deref().unwrap();
                if d.x.abs() < across
                    && d.z.abs() < across
                    && b.pos.y < pos.y + shape.height + 8.0
                    && b.pos.y + b.shape().height > pos.y - 8.0
                    && (a.school > 1 && a.school < max || a.leader.is_none())
                {
                    near.push(j);
                }
            });
            near.sort_unstable();
            let followed = |j: usize| self.mobs[j].aquatic.as_deref().unwrap();
            let leader =
                near.iter().copied().find(|&j| followed(j).school > 1 && followed(j).school < max).unwrap_or(i);
            // `addFollowers`: `limit(max - schoolSize)` before dropping the leader.
            let room = max.saturating_sub(followed(leader).school) as usize;
            let joining: Vec<usize> = near
                .iter()
                .copied()
                .filter(|&j| followed(j).leader.is_none())
                .take(room)
                .filter(|&j| j != leader)
                .collect();
            let (luid, lpos) = (self.mobs[leader].uid, self.mobs[leader].pos);
            for j in joining {
                let a = self.mobs[j].aquatic.as_mut().unwrap();
                a.leader = Some((luid, lpos));
                a.flock_recalc = 0;
                self.mobs[leader].aquatic.as_mut().unwrap().school += 1;
            }
        }
    }

    pub(super) fn populate_structures<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        w: &W,
        ctx: &Ctx,
        difficulty: Difficulty,
    ) {
        if ctx.dimension != Dimension::Overworld {
            return;
        }
        self.structure_timer -= dt;
        if self.structure_timer > 0.0 {
            return;
        }
        self.structure_timer = 1.0;
        for t in &ctx.players {
            for b in w.structures_near(t.pos.floor().as_ivec3(), 80) {
                if self.structures_populated.contains(&b.bounds.min) {
                    continue;
                }
                if b.residents.is_empty() || !b.residents.iter().all(|(_, p)| w.loaded(*p)) {
                    continue;
                }
                // Wait for blocks, not just the chunk addresses, before recording it.
                if b.residents
                    .iter()
                    .any(|(k, p)| physics::overlaps_solid(w, p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5), k.shape()))
                {
                    continue;
                }
                // Residents are recorded one by one, so hostiles skipped on
                // Peaceful still arrive once the difficulty goes up. The
                // structure counts as populated only when all have spawned.
                let mut complete = true;
                for &(kind, p) in &b.residents {
                    if self.structures_populated.contains(&p) {
                        continue;
                    }
                    if difficulty == Difficulty::Peaceful && kind.is_hostile() {
                        complete = false;
                        continue;
                    }
                    self.structures_populated.insert(p);
                    self.spawn(kind, p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
                    let m = self.mobs.last_mut().unwrap();
                    m.persistent = kind != MobKind::Pillager;
                    if kind == MobKind::Cat {
                        m.wool_color = crate::color::DyeColor::Black;
                    }
                }
                if complete {
                    for (_, p) in &b.residents {
                        self.structures_populated.remove(p);
                    }
                    self.structures_populated.insert(b.bounds.min);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        entity::{PlayerId, Target},
        physics::test_util::Grid,
        world::terrain::Generator,
    };
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };
    fn water() -> Grid {
        let mut w = Grid::flat(-64);
        for x in -5..=20 {
            for y in -2..=8 {
                for z in -5..=5 {
                    w.set(IVec3::new(x, y, z), Block::WATER);
                }
            }
        }
        w
    }
    /// A walled pool on a stone floor: water from y = -2 to 8 over
    /// x -10..=30 and z -10..=10.
    fn pool() -> Grid {
        let mut w = Grid::flat(-3);
        for x in -11..=31 {
            for y in -3..=11 {
                for z in -11..=11 {
                    let wall = x == -11 || x == 31 || z == -11 || z == 11 || y == -3;
                    let b = if wall {
                        Block::STONE
                    } else if y <= 8 {
                        Block::WATER
                    } else {
                        Block::AIR
                    };
                    w.set(IVec3::new(x, y, z), b);
                }
            }
        }
        w
    }
    fn ctx(p: DVec3) -> Ctx {
        Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::DAY,
            players: vec![Target::new(PlayerId(7), p, true)],
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
        }
    }
    #[test]
    fn swimming_fish_stay_in_the_water() {
        let w = water();
        let mut m = Mob::new(MobKind::Cod, DVec3::new(0.5, 0.0, 0.5), 0.0);
        let mut r = Rng::new(4);
        let mut e = vec![];
        for _ in 0..400 {
            m.update(0.05, &w, &Ctx { players: vec![], ..ctx(DVec3::ZERO) }, &mut r, &mut e);
            // At most its eye leaves the surface at y = 9.
            assert!(m.pos.y < 9.0 && m.in_water, "{}", m.pos);
        }
        assert_eq!(m.health, 3.0);
        assert!(m.pos.distance(DVec3::new(0.5, 0.0, 0.5)) > 0.5);
    }
    #[test]
    fn fish_suffocate_and_water_resets_the_air_timer() {
        let mut m = Mob::new(MobKind::Cod, DVec3::ZERO, 0.0);
        let dry = Grid::flat(-64);
        environment(&mut m, 15.5, &dry);
        assert_eq!(m.health, 1.0);
        environment(&mut m, 1.0, &water());
        assert_eq!(m.aquatic.as_ref().unwrap().dry, 0.0);
        environment(&mut m, 14.0, &dry);
        assert_eq!(m.health, 1.0);
    }
    #[test]
    fn guardian_beam_charges_breaks_on_occlusion_and_respects_target_identity() {
        let mut w = water();
        let mut m = Mob::new(MobKind::Guardian, DVec3::ZERO, 0.0);
        let c = ctx(DVec3::new(10.0, 0.0, 0.0));
        let mut e = vec![];
        for _ in 0..80 {
            upkeep(&mut m, 0.05, &w, &c, &mut e);
        }
        assert!(!e.iter().any(|e| matches!(e, EntityEvent::PlayerBeam { .. })));
        w.set(IVec3::new(5, 0, 0), Block::STONE);
        w.set(IVec3::new(5, 1, 0), Block::STONE);
        upkeep(&mut m, 0.05, &w, &c, &mut e);
        assert!(m.aquatic.as_ref().unwrap().beam.is_none());
        w.set(IVec3::new(5, 0, 0), Block::WATER);
        w.set(IVec3::new(5, 1, 0), Block::WATER);
        for _ in 0..92 {
            upkeep(&mut m, 0.05, &w, &c, &mut e);
        }
        assert!(
            e.iter().any(
                |e| matches!(e,EntityEvent::PlayerBeam{player,physical,..} if *player==PlayerId(7)&&*physical==6.0)
            )
        );
        assert!(e.iter().any(|e| matches!(e,EntityEvent::PlayerBeam{magic,..} if *magic==1.0)));
        let mut other = c;
        other.players[0].id = PlayerId(8);
        upkeep(&mut m, 0.05, &w, &other, &mut e);
        assert!(m.aquatic.as_ref().unwrap().charge < 0.0);
    }
    #[test]
    fn elders_fatigue_survival_players_even_through_walls_and_ignore_creative() {
        let w = water();
        let mut m = Mob::new(MobKind::ElderGuardian, DVec3::ZERO, 0.0);
        let mut c = ctx(DVec3::new(40.0, 0.0, 0.0));
        c.players.push(Target::new(PlayerId(8), DVec3::Y, false));
        let mut e = vec![];
        upkeep(&mut m, 0.05, &w, &c, &mut e);
        assert_eq!(
            e.iter()
                .filter(|e| matches!(
                    e,
                    EntityEvent::PlayerEffect { effect: Effect::MiningFatigue, amplifier: 2, ticks: 6000, .. }
                ))
                .count(),
            1
        );
        e.clear();
        upkeep(&mut m, 1.0, &w, &c, &mut e);
        assert!(e.is_empty());
    }
    #[test]
    fn pufferfish_stings_and_dolphins_grant_a_separate_water_effect() {
        let w = water();
        let c = ctx(DVec3::new(0.3, 0.0, 0.0));
        let mut e = vec![];
        let mut p = Mob::new(MobKind::Pufferfish, DVec3::ZERO, 0.0);
        upkeep(&mut p, 0.6, &w, &c, &mut e);
        // The Poison rides on the sting: `Pufferfish.playerTouch` applies it only when the sting hurts.
        assert!(e.iter().any(|e| matches!(e, EntityEvent::PlayerSting { poison: 120, .. })));
        assert!(!e.iter().any(|e| matches!(e, EntityEvent::PlayerEffect { effect: Effect::Poison, .. })));
        let mut d = Mob::new(MobKind::Dolphin, DVec3::ZERO, 0.0);
        upkeep(&mut d, 0.05, &w, &c, &mut e);
        assert!(
            e.iter().any(|e| matches!(e, EntityEvent::PlayerEffect { effect: Effect::DolphinsGrace, ticks: 100, .. }))
        );
    }
    #[test]
    fn axolotls_hunt_fish_and_loose_fish_gather_a_school() {
        let w = water();
        let mut e = Entities::new(4);
        for x in [0.5, 3.5, 5.5] {
            e.spawn(MobKind::Cod, DVec3::new(x, 0.0, 0.0));
        }
        e.spawn(MobKind::Axolotl, DVec3::ZERO);
        e.ensure_mob_ids();
        e.nether_sense(0.05, &w, &ctx(DVec3::ZERO));
        e.aquatic_sense(0.05, &w);
        e.sense_goals(&w);
        e.sense_schools();
        // The first loose fish leads; the others within 8 blocks follow it.
        let leader = e.mobs[0].uid;
        assert_eq!(e.mobs[0].aquatic.as_ref().unwrap().school, 3);
        for i in [1, 2] {
            assert_eq!(e.mobs[i].aquatic.as_ref().unwrap().leader.map(|l| l.0), Some(leader));
        }
        // A follower that swims past 11 blocks drops out, and the school shrinks.
        e.mobs[2].pos.x = 20.0;
        e.sense_schools();
        assert!(e.mobs[2].aquatic.as_ref().unwrap().leader.is_none());
        assert_eq!(e.mobs[0].aquatic.as_ref().unwrap().school, 2);
        assert_eq!(e.mobs[3].aquatic.as_ref().unwrap().prey.unwrap().0, e.mobs[0].uid);
        let mut events = vec![];
        steer(&mut e.mobs[3], 0.05, &w, &ctx(DVec3::ZERO), &mut Rng::new(1), &mut events);
        assert!(events.iter().any(|e| matches!(e, EntityEvent::MobHit { damage: 2.0, .. })));
    }

    /// `AvoidEntityGoal(Player, 8, 1.6, 1.4)`: fish swim away from a
    /// survival player within 8 blocks; creative players don't count.
    #[test]
    fn fish_flee_survival_players_but_not_creative_ones() {
        let w = pool();
        for (survival, flees) in [(true, true), (false, false)] {
            let mut e = Entities::new(9);
            e.spawn(MobKind::Pufferfish, DVec3::new(6.5, 3.0, 0.5));
            e.ensure_mob_ids();
            let p = DVec3::new(2.5, 3.0, 0.5);
            let c = Ctx { players: vec![Target::new(PlayerId(7), p, survival)], ..ctx(p) };
            let mut fled = false;
            for _ in 0..100 {
                e.update(0.05, &w, &c);
                fled |= e.mobs[0].goals.avoiding.is_some();
            }
            assert_eq!(fled, flees);
            if flees {
                // About 1.4 blocks a second once up to speed.
                assert!(e.mobs[0].pos.distance(p) > 7.0, "{}", e.mobs[0].pos);
            }
        }
    }

    /// `BreathAirGoal`: below 140 ticks of air a dolphin heads for the air
    /// above it, and the extra stroke speeds it there.
    #[test]
    fn dolphins_short_of_air_swim_up_to_breathe() {
        let w = pool();
        let mut e = Entities::new(9);
        e.spawn(MobKind::Dolphin, DVec3::new(5.5, -1.0, 0.5));
        e.ensure_mob_ids();
        e.mobs[0].aquatic.as_mut().unwrap().air = 5.0;
        let c = Ctx { players: vec![], ..ctx(DVec3::ZERO) };
        let mut breathed = false;
        for _ in 0..40 {
            e.update(0.05, &w, &c);
            breathed |= e.mobs[0].aquatic.as_ref().unwrap().air == 240.0;
        }
        assert!(breathed, "{}", e.mobs[0].pos);
    }

    #[test]
    fn salmon_schools_hold_at_most_five() {
        let w = water();
        let mut e = Entities::new(4);
        for i in 0..7 {
            e.spawn(MobKind::Salmon, DVec3::new(0.5 + i as f64 * 0.5, 0.0, 0.0));
        }
        e.ensure_mob_ids();
        e.sense_goals(&w);
        e.sense_schools();
        // `addFollowers` limits the loose fish to `max - schoolSize` before
        // dropping the leader itself, which is one of them: three join.
        assert_eq!(e.mobs[0].aquatic.as_ref().unwrap().school, 4);
        let loose = e.mobs.iter().filter(|m| m.aquatic.as_ref().unwrap().leader.is_none()).count();
        assert_eq!(loose, 4, "the leader and three fish left over");
        // Later searches never grow a school past five.
        for _ in 0..400 {
            for m in &mut e.mobs {
                m.goals.ticks = m.goals.ticks.wrapping_add(1);
            }
            e.sense_goals(&w);
            e.sense_schools();
            assert!(e.mobs.iter().all(|m| m.aquatic.as_ref().unwrap().school <= 5));
        }
    }

    fn world() -> World {
        let mut w = World::new_headless(Arc::new(Generator::new(1)), Default::default(), 2);
        let end = Instant::now() + Duration::from_secs(20);
        while !w.column_loaded(0, 0) || w.pending_jobs() > 0 {
            w.update(DVec3::new(0.0, 150.0, 0.0));
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(1));
        }
        w
    }
    #[test]
    fn bucket_components_round_trip_and_capture_cannot_pass_through_walls() {
        let mut w = world();
        let eye = DVec3::new(0.5, 151.2, 0.5);
        let mut e = Entities::new(1);
        e.spawn(MobKind::Axolotl, DVec3::new(2.5, 151.0, 0.5));
        e.mobs[0].aquatic.as_mut().unwrap().variant = 3;
        e.mobs[0].health = 6.25;
        let mut inv = Inventory::default();
        inv.slots[0] = Some(Stack::new(Item::WATER_BUCKET, 1));
        w.set_block(IVec3::new(1, 151, 0), Block::STONE);
        assert!(!e.capture_water_creature(&w, &mut inv, 0, false, eye, DVec3::X, 6.0));
        w.set_block(IVec3::new(1, 151, 0), Block::AIR);
        assert!(e.capture_water_creature(&w, &mut inv, 0, false, eye, DVec3::X, 6.0));
        assert!(e.mobs.is_empty());
        let encoded = crate::inventory::stack_to_string(inv.get(0));
        assert_eq!(crate::inventory::stack_from_str(&encoded), Some(inv.get(0)));
        assert!(e.release_water_creature(inv.get(0).unwrap(), DVec3::new(3.5, 151.0, 0.5)));
        let m = &e.mobs[0];
        assert!(m.persistent);
        assert_eq!(m.health, 6.25);
        assert_eq!(m.aquatic.as_ref().unwrap().variant, 3);
        let saved = e.nether_mobs_to_string();
        let mut restored = Entities::new(2);
        restored.load_nether_mobs(&saved);
        assert_eq!(restored.mobs[0].aquatic.as_ref().unwrap().variant, 3);
        assert_eq!(restored.mobs[0].health, 6.25);
    }
    #[test]
    fn guardian_beam_keeps_magic_damage_when_armor_reduces_the_physical_hit() {
        let mut e = Entities::new(1);
        let mut a = crate::agent::Agent::new(DVec3::ZERO);
        a.inventory.armor = [
            Some(Stack::new(Item::armor(crate::item::ArmorPiece::Helmet, crate::item::ArmorMaterial::Diamond), 1)),
            Some(Stack::new(Item::armor(crate::item::ArmorPiece::Chestplate, crate::item::ArmorMaterial::Diamond), 1)),
            Some(Stack::new(Item::armor(crate::item::ArmorPiece::Leggings, crate::item::ArmorMaterial::Diamond), 1)),
            Some(Stack::new(Item::armor(crate::item::ArmorPiece::Boots, crate::item::ArmorMaterial::Diamond), 1)),
        ];
        let expected =
            crate::simulation::survival::armor_reduce(6.0, a.inventory.armor_points(), a.inventory.armor_toughness())
                + 1.0;
        let taken = a.hurt_beam(6.0, 1.0, &mut e);
        assert!((taken - expected).abs() < 1e-5);
        let mut bare = crate::agent::Agent::new(DVec3::ZERO);
        assert_eq!(bare.hurt_beam(6.0, 1.0, &mut e), 7.0);
    }

    #[test]
    fn guardian_spikes_are_fixed_contact_damage_and_credit_the_actual_hitter() {
        let mut e = Entities::new(1);
        e.spawn(MobKind::Guardian, DVec3::ZERO);
        e.melee_for(0, DVec3::X, None, 0.0, false, None, PlayerId(12));
        assert!(
            e.pending_sounds
                .iter()
                .any(|e| matches!(e, EntityEvent::PlayerSting { player: PlayerId(12), damage: 2.0, .. }))
        );
    }

    #[test]
    fn water_creatures_require_water_and_only_monuments_spawn_guardians() {
        let mut r = Rng::new(1);
        let mut w = water();
        assert!(spawn_spot(&w, MobKind::Cod, 0, 0, 60, &mut r).is_none());
        for y in 50..=63 {
            w.set(IVec3::new(0, y, 0), Block::WATER);
        }
        assert!(spawn_spot(&w, MobKind::Cod, 0, 0, 60, &mut r).is_some());
        assert!(spawn_spot(&w, MobKind::Guardian, 0, 0, 60, &mut r).is_none());
    }
    #[test]
    fn persistent_structure_markers_prevent_resurrection_after_reload() {
        let mut e = Entities::new(1);
        e.structures_populated.insert(IVec3::new(-10, 39, 20));
        e.spawn(MobKind::ElderGuardian, DVec3::ZERO);
        e.mobs[0].persistent = true;
        let mut restored = Entities::new(2);
        restored.load_nether_mobs(&e.nether_mobs_to_string());
        assert_eq!(restored.count(MobKind::ElderGuardian), 1);
        assert_eq!(restored.structures_populated, e.structures_populated);
        e.mobs.clear();
        let mut dead = Entities::new(2);
        dead.load_nether_mobs(&e.nether_mobs_to_string());
        assert!(dead.mobs.is_empty());
        assert_eq!(dead.structures_populated, e.structures_populated);
    }

    #[test]
    fn turtles_hatch_as_babies_and_grow_up_shedding_a_scute() {
        let mut e = Entities::new(3);
        e.hatch_turtles(glam::IVec3::new(0, 64, 0), 2);
        assert_eq!(e.mobs.iter().filter(|m| m.kind == MobKind::Turtle && m.baby).count(), 2);
        // Growth survives a save and reload.
        e.grow_turtles(TURTLE_GROW_SECS - 1.0);
        let mut e2 = Entities::new(3);
        e2.load_nether_mobs(&e.nether_mobs_to_string());
        assert!(e2.mobs.iter().all(|m| m.baby && m.grow >= TURTLE_GROW_SECS - 1.0));
        e = e2;
        e.grow_turtles(2.0);
        assert!(e.mobs.iter().all(|m| !m.baby));
        assert_eq!(e.items.iter().filter(|i| i.stack.item == Item::TURTLE_SCUTE).count(), 2);
        assert!(crate::entity::can_spawn_on(MobKind::Turtle, Block::SAND, 1.0));
        assert!(!crate::entity::can_spawn_on(MobKind::Turtle, Block::GRASS, 1.0));
        assert!(MobKind::Turtle.biome_chance(crate::world::terrain::Biome::Beach) > 0.0);
        assert_eq!(MobKind::Turtle.biome_chance(crate::world::terrain::Biome::Plains), 0.0);
    }

    /// Timing for a busy ocean and a Nether sky (`cargo test --release --lib
    /// mob_movement_cost -- --ignored --nocapture`): 186 swimmers in a
    /// 64 x 16 x 64 sea, then 8 ghasts and 8 blazes, 400 ticks each.
    #[test]
    #[ignore]
    fn mob_movement_cost() {
        let mut sea = Grid::flat(-64);
        for x in -32..32 {
            for y in -8..8 {
                for z in -32..32 {
                    sea.set(IVec3::new(x, y, z), Block::WATER);
                }
            }
        }
        let mut e = Entities::new(11);
        let mut r = Rng::new(5);
        for (kind, n) in [
            (MobKind::Cod, 120),
            (MobKind::Salmon, 30),
            (MobKind::Dolphin, 10),
            (MobKind::Guardian, 10),
            (MobKind::Squid, 10),
            (MobKind::Axolotl, 6),
        ] {
            for _ in 0..n {
                e.spawn(
                    kind,
                    DVec3::new(r.range(-28.0, 28.0) as f64, r.range(-6.0, 5.0) as f64, r.range(-28.0, 28.0) as f64),
                );
            }
        }
        let c = Ctx { players: vec![Target::new(PlayerId(7), DVec3::new(0.0, 9.0, 0.0), false)], ..ctx(DVec3::ZERO) };
        let start = std::time::Instant::now();
        for _ in 0..400 {
            e.update(0.05, &sea, &c);
        }
        let swim = start.elapsed().as_secs_f64() * 1000.0 / 400.0;
        let mut sky = Grid::flat(0);
        for x in -40..40 {
            for z in -40..40 {
                sky.set(IVec3::new(x, 60, z), Block::NETHERRACK);
            }
        }
        let mut e = Entities::new(12);
        for i in 0..8 {
            e.spawn(MobKind::Ghast, DVec3::new(i as f64 * 6.0 - 20.0, 30.0, 10.0));
            e.spawn(MobKind::Blaze, DVec3::new(i as f64 * 3.0 - 10.0, 1.0, -10.0));
        }
        let c = Ctx {
            dimension: Dimension::Nether,
            players: vec![Target::new(PlayerId(7), DVec3::new(0.0, 1.0, 0.0), true)],
            ..ctx(DVec3::ZERO)
        };
        let start = std::time::Instant::now();
        for _ in 0..400 {
            e.update(0.05, &sky, &c);
        }
        let fly = start.elapsed().as_secs_f64() * 1000.0 / 400.0;
        eprintln!("mob_movement_cost: swimmers {swim:.3} ms/tick, ghasts and blazes {fly:.3} ms/tick");
    }
}
