//! The End fight, after Java's `EndDragonFight`, `EnderDragon` and its
//! phases: End crystals burn on the pillars and heal the dragon, which
//! circles them, strafes players with fireballs, perches on the exit portal
//! to breathe fire, and on death rises in light, drops its experience and
//! opens the exit portal.
//!
//! The dragon runs on whole 20 Hz ticks with Java's per-tick numbers. Its
//! yaw uses Java's convention (degrees, heading `(sin, -cos)`), and like
//! Java it has no collision: it smashes through everything but the End's
//! own blocks.

use glam::{DVec3, IVec3};

use super::{Ctx, EntityEvent, MobSound, PlayerId, Rng, Target};
use crate::physics::{self, BlockSource};
use crate::world::block::Block;
use crate::world::end::{EndGen, Pillar};

pub const MAX_HEALTH: f32 = 200.0;
/// Experience for the first kill, and for any later one.
pub const FIRST_XP: u32 = 12_000;
pub const LATER_XP: u32 = 500;
/// An End crystal's blast.
pub const CRYSTAL_POWER: f32 = 6.0;
/// Players within this distance of the island centre see the boss bar
/// (Java's fight area).
pub const ARENA: f64 = 192.0;
const TICK: f64 = 0.05;
const FRAC_PI_12: f64 = std::f64::consts::PI / 12.0;
/// Ticks the death takes, rising and glowing.
pub const DEATH_TICKS: u32 = 200;
/// Where a new dragon appears.
const SPAWN: DVec3 = DVec3::new(0.0, 110.0, 0.0);
/// Crystals heal a dragon this close.
const HEAL_RANGE: f64 = 32.0;

/// One hit box of the dragon (Java's `EnderDragonPart`s).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Head,
    Neck,
    Body,
    Tail(u8),
    Wing(u8),
}

impl Part {
    /// Width and height in blocks.
    fn size(self) -> (f64, f64) {
        match self {
            Part::Head => (1.0, 1.0),
            Part::Neck => (3.0, 3.0),
            Part::Body => (5.0, 3.0),
            Part::Tail(_) => (2.0, 2.0),
            Part::Wing(_) => (4.0, 2.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Circling the pillars.
    HoldingPattern,
    /// Heading for a player to shoot a fireball at.
    Strafe(PlayerId),
    /// Flying in toward the exit portal.
    LandingApproach,
    Landing,
    /// Perched, breathing a cloud of fire in front.
    SittingFlaming,
    /// Perched, looking around for someone close.
    SittingScanning,
    /// Perched, roaring at someone in front.
    SittingAttacking,
    Takeoff,
    /// Diving at where a player stood.
    Charging,
    Dying,
}

impl Phase {
    pub fn sitting(self) -> bool {
        matches!(self, Phase::SittingFlaming | Phase::SittingScanning | Phase::SittingAttacking)
    }
}

pub struct Dragon {
    pub pos: DVec3,
    pub previous_pos: DVec3,
    /// Blocks per tick.
    vel: DVec3,
    /// Degrees, Java's convention: the dragon faces `(sin, -cos)`.
    pub yaw: f32,
    pub previous_yaw: f32,
    /// Java's `yRotA`: turning speed.
    yaw_speed: f32,
    pub health: f32,
    pub phase: Phase,
    /// Ticks in this phase.
    phase_ticks: u32,
    target: Option<DVec3>,
    /// Waypoints still to fly through, nearest first.
    path: Vec<DVec3>,
    clockwise: bool,
    /// Ticks spent aiming at the strafed player.
    fireball_charge: u32,
    /// Flame bursts since landing (it takes off after four).
    flame_count: u32,
    sitting_damage: f32,
    /// Index of the crystal healing it (drawn as a beam).
    pub healer: Option<usize>,
    /// Seconds of the red hurt flash left.
    pub hurt: f32,
    /// Damage taken during the current immunity window (Java's
    /// `invulnerableTime`): only harder hits do the difference.
    immune: (u32, f32),
    pub death_ticks: u32,
    /// Wing beat, in turns; `previous_flap` for interpolation.
    pub flap: f32,
    pub previous_flap: f32,
    /// Yaw and height over the last 64 ticks, newest at `history_at`.
    history: [(f32, f64); 64],
    history_at: usize,
    /// Hit boxes, refreshed each tick.
    parts: [(Part, DVec3, DVec3); 8],
    /// Ticks until the next growl.
    growl: u32,
}

/// A crystal on a pillar.
pub struct Crystal {
    /// Feet, centred.
    pub pos: DVec3,
    /// Ticks alive, for the spin and bob.
    pub age: u32,
    /// Which pillar it stands on, for the save.
    pillar: usize,
}

impl Crystal {
    /// The crystal's 2x2x2 box.
    pub fn aabb(&self) -> (DVec3, DVec3) {
        (self.pos - DVec3::new(1.0, 0.0, 1.0), self.pos + DVec3::new(1.0, 2.0, 1.0))
    }

    pub fn center(&self) -> DVec3 {
        self.pos + DVec3::Y
    }
}

/// A dragon fireball (Java's `DragonFireball`): speeds up like a blaze's and
/// bursts into a lingering cloud of breath.
pub struct DragonFireball {
    pub pos: DVec3,
    pub previous_pos: DVec3,
    /// Blocks per tick.
    vel: DVec3,
    dir: DVec3,
    age: u32,
}

/// A spreading cloud of dragon's breath (Java's `AreaEffectCloud` with
/// instant damage, applied at half strength).
pub struct BreathCloud {
    pub pos: DVec3,
    pub radius: f32,
    grow: f32,
    pub age: u32,
    pub duration: u32,
    damage: f32,
    /// Ticks until each player in it can be hurt again.
    cooldowns: Vec<(PlayerId, u32)>,
}

/// The fight in one End.
pub struct Fight {
    pub dragon: Option<Dragon>,
    pub crystals: Vec<Crystal>,
    pub fireballs: Vec<DragonFireball>,
    pub clouds: Vec<BreathCloud>,
    /// The dragon of this End has died at least once.
    pub previously_killed: bool,
    /// The exit portal's centre.
    pub podium: IVec3,
    nodes: [DVec3; 24],
    /// Crystals waiting to blow up (hit last tick), and who hit them.
    detonating: Vec<(usize, Option<PlayerId>)>,
    /// Game time not yet run as whole ticks.
    pending: f64,
    /// The dragon died but the exit portal hasn't opened yet (saved
    /// mid-death).
    unopened: bool,
}

impl Fight {
    /// A fresh fight: a dragon and a crystal on every pillar.
    pub fn new(end: &EndGen) -> Self {
        let mut fight = Self::empty(end);
        fight.dragon = Some(Dragon::new(SPAWN));
        fight.crystals = end.pillars().iter().enumerate().map(|(i, p)| crystal_on(i, p)).collect();
        fight
    }

    fn empty(end: &EndGen) -> Self {
        // Java's path nodes: twelve around the pillars, eight inside them,
        // four close in, each a little above the ground.
        let nodes = std::array::from_fn(|i| {
            let (radius, angle, lift) = match i {
                0..12 => (60.0, FRAC_PI_12 * i as f64, 5),
                12..20 => (40.0, std::f64::consts::FRAC_PI_8 * (i - 12) as f64, 15),
                _ => (20.0, std::f64::consts::FRAC_PI_4 * (i - 20) as f64, 5),
            };
            let a = 2.0 * (-std::f64::consts::PI + angle);
            let (x, z) = ((radius * a.cos()).floor() as i32, (radius * a.sin()).floor() as i32);
            let ground = end.column(x, z).map_or(60, |(top, _)| top + 1);
            DVec3::new(x as f64 + 0.5, (ground + lift).max(10) as f64, z as f64 + 0.5)
        });
        Self {
            dragon: None,
            crystals: Vec::new(),
            fireballs: Vec::new(),
            clouds: Vec::new(),
            previously_killed: false,
            podium: end.podium(),
            nodes,
            detonating: Vec::new(),
            pending: 0.0,
            unopened: false,
        }
    }

    /// Restores a saved fight (see [`Fight::serialize`]); a missing or
    /// unreadable one starts fresh.
    pub fn load(end: &EndGen, text: Option<&str>) -> Self {
        let Some(text) = text else { return Self::new(end) };
        let fields: Vec<&str> = text.split(',').collect();
        let [alive, previously, health, mask] = fields[..] else { return Self::new(end) };
        let mut fight = Self::empty(end);
        fight.previously_killed = previously == "1";
        // Saved mid-death: open the portal once its chunk is back.
        fight.unopened = alive == "2";
        if alive == "1" {
            let mut dragon = Dragon::new(SPAWN);
            dragon.health = health.parse::<f32>().unwrap_or(MAX_HEALTH).clamp(1.0, MAX_HEALTH);
            fight.dragon = Some(dragon);
        }
        let mask: u32 = mask.parse().unwrap_or(0);
        fight.crystals = end
            .pillars()
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1 << i) != 0)
            .map(|(i, p)| crystal_on(i, p))
            .collect();
        fight
    }

    /// `state,previously_killed,health,crystal mask`, where the state is 0
    /// for dead, 1 alive and 2 dying (or dead with the portal still shut).
    pub fn serialize(&self) -> String {
        let state = match &self.dragon {
            Some(d) if d.phase == Phase::Dying => 2,
            Some(_) => 1,
            None if self.unopened => 2,
            None => 0,
        };
        let health = self.dragon.as_ref().filter(|d| d.phase != Phase::Dying).map_or(0.0, |d| d.health);
        let mask: u32 = self.crystals.iter().map(|c| 1 << c.pillar).sum();
        format!("{state},{},{health},{mask}", self.previously_killed as u8)
    }

    /// The boss bar: the dragon's health fraction, if a living dragon is
    /// near `player`.
    pub fn boss_bar(&self, player: DVec3) -> Option<f32> {
        let d = self.dragon.as_ref()?;
        (player.with_y(0.0).length() < ARENA).then_some(d.health.max(0.0) / MAX_HEALTH)
    }

    /// Advances the fight by `dt` game seconds.
    pub(super) fn update<W: BlockSource + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) {
        self.pending += dt;
        while self.pending >= TICK - 1e-9 {
            self.pending -= TICK;
            self.tick(world, ctx, rng, events);
        }
    }

    fn tick<W: BlockSource + ?Sized>(&mut self, world: &W, ctx: &Ctx, rng: &mut Rng, events: &mut Vec<EntityEvent>) {
        for (index, by) in std::mem::take(&mut self.detonating) {
            self.blow_up(index, by, events);
        }
        if self.unopened && world.block(self.podium).is_some() {
            self.unopened = false;
            events.push(EntityEvent::DragonKilled { first: !self.previously_killed });
            self.previously_killed = true;
        }
        for c in &mut self.crystals {
            c.age += 1;
            // Java keeps a fire burning under every crystal in the End.
            let cell = c.pos.floor().as_ivec3();
            if c.age % 20 == 1 && world.block(cell) == Some(Block::AIR) {
                events.push(EntityEvent::IgniteBlock { cell });
            }
        }
        if let Some(mut dragon) = self.dragon.take() {
            let keep = dragon.tick(self, world, ctx, rng, events);
            if keep {
                self.dragon = Some(dragon);
            } else {
                events.push(EntityEvent::DragonKilled { first: !self.previously_killed });
                self.previously_killed = true;
            }
        }
        let mut fireballs = std::mem::take(&mut self.fireballs);
        fireballs.retain_mut(|f| f.tick(world, ctx, &mut self.clouds));
        self.fireballs = fireballs;
        self.clouds.retain_mut(|c| c.tick(ctx, events));
    }

    /// Something hit crystal `index` (melee, an arrow or a blast): it blows
    /// up on the next tick. `by` is the player who did it.
    pub fn hit_crystal(&mut self, index: usize, by: Option<PlayerId>) {
        if !self.detonating.iter().any(|&(i, _)| i == index) {
            self.detonating.push((index, by));
        }
    }

    fn blow_up(&mut self, index: usize, by: Option<PlayerId>, events: &mut Vec<EntityEvent>) {
        if index >= self.crystals.len() {
            return;
        }
        let crystal = self.crystals.swap_remove(index);
        // Keep pending detonations pointing at the same crystals.
        let moved = self.crystals.len();
        for (i, _) in &mut self.detonating {
            if *i == moved {
                *i = index;
            }
        }
        events.push(EntityEvent::Explosion {
            center: crystal.center(),
            power: CRYSTAL_POWER,
            cause: "was blown up by an End crystal",
        });
        if let Some(d) = &mut self.dragon {
            // Destroying the crystal healing it hurts the dragon.
            match d.healer {
                Some(h) if h == index => {
                    d.healer = None;
                    d.hurt_by(Part::Head, 10.0, true);
                }
                Some(h) if h == moved => d.healer = Some(index),
                _ => {}
            }
            // While circling, it turns on whoever did it.
            if let (Phase::HoldingPattern, Some(player)) = (d.phase, by) {
                d.set_phase(Phase::Strafe(player));
            }
        }
    }

    /// The nearest crystal or dragon part a ray hits within `max_dist`.
    pub fn raycast(&self, origin: DVec3, dir: DVec3, max_dist: f64) -> Option<(Hit, f64)> {
        let crystals = self.crystals.iter().enumerate().filter_map(|(i, c)| {
            let (min, max) = c.aabb();
            physics::ray_aabb(origin, dir, min, max).map(|t| (Hit::Crystal(i), t))
        });
        let parts = self.dragon.iter().filter(|d| d.phase != Phase::Dying).flat_map(|d| {
            d.parts.iter().filter_map(|&(part, min, max)| {
                physics::ray_aabb(origin, dir, min, max).map(|t| (Hit::Dragon(part), t))
            })
        });
        crystals.chain(parts).filter(|&(_, t)| t <= max_dist).min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// Whatever `p` is inside of.
    pub fn hit_at(&self, p: DVec3) -> Option<Hit> {
        let inside = |(min, max): (DVec3, DVec3)| p.cmpge(min).all() && p.cmple(max).all();
        if let Some(i) = self.crystals.iter().position(|c| inside(c.aabb())) {
            return Some(Hit::Crystal(i));
        }
        let d = self.dragon.as_ref().filter(|d| d.phase != Phase::Dying)?;
        d.parts.iter().find(|&&(_, min, max)| inside((min, max))).map(|&(part, ..)| Hit::Dragon(part))
    }

    /// Applies a player's hit; returns whether it did anything. Arrows
    /// bounce off a perched dragon.
    pub fn strike(&mut self, hit: Hit, damage: f32, by: Option<PlayerId>, arrow: bool) -> bool {
        match hit {
            Hit::Crystal(i) => {
                self.hit_crystal(i, by);
                true
            }
            Hit::Dragon(part) => self.dragon.as_mut().is_some_and(|d| {
                if arrow && d.phase.sitting() {
                    return false;
                }
                d.hurt_by(part, damage, true)
            }),
        }
    }

    /// A blast at `center`: crystals in reach go off too, and the dragon
    /// takes the damage of its nearest part.
    pub fn explode(&mut self, center: DVec3, power: f32) {
        for i in 0..self.crystals.len() {
            if super::explosion_damage(power, self.crystals[i].center().distance(center)).is_some() {
                self.hit_crystal(i, None);
            }
        }
        if let Some(d) = &mut self.dragon {
            let worst = d
                .parts
                .iter()
                .filter_map(|&(part, min, max)| {
                    let (damage, _) = super::explosion_damage(power, ((min + max) / 2.0).distance(center))?;
                    Some((part, damage))
                })
                .max_by(|a, b| part_damage(a.0, a.1).total_cmp(&part_damage(b.0, b.1)));
            if let Some((part, damage)) = worst {
                d.hurt_by(part, damage, true);
            }
        }
    }

    /// Nearest of `nodes[range]` to `p`.
    fn closest_node(&self, p: DVec3, range: std::ops::Range<usize>) -> usize {
        range.min_by(|&a, &b| self.nodes[a].distance_squared(p).total_cmp(&self.nodes[b].distance_squared(p))).unwrap()
    }

    /// The perch: on top of the exit portal's pillar (and egg).
    fn perch(&self, world: &(impl BlockSource + ?Sized)) -> DVec3 {
        let mut top = self.podium + IVec3::Y * 4;
        while world.block(top).is_some_and(|b| b != Block::AIR) && top.y < self.podium.y + 8 {
            top.y += 1;
        }
        top.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)
    }
}

/// What a player's attack hit in the fight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Crystal(usize),
    Dragon(Part),
}

fn crystal_on(pillar: usize, p: &Pillar) -> Crystal {
    Crystal { pos: p.crystal(), age: (pillar as u32 * 37) % 200, pillar }
}

/// Java: hits anywhere but the head do a quarter, plus up to one.
fn part_damage(part: Part, damage: f32) -> f32 {
    if part == Part::Head { damage } else { damage / 4.0 + damage.min(1.0) }
}

fn part_center(d: &Dragon, part: Part) -> DVec3 {
    d.parts.iter().find(|p| p.0 == part).map_or(d.pos, |&(_, min, max)| (min + max) / 2.0)
}

/// Java's `Mth.wrapDegrees`.
fn wrap_degrees(a: f32) -> f32 {
    (a + 180.0).rem_euclid(360.0) - 180.0
}

/// The horizontal unit heading of a Java yaw.
fn heading(yaw: f32) -> DVec3 {
    let r = yaw.to_radians();
    DVec3::new(r.sin() as f64, 0.0, -r.cos() as f64)
}

impl Dragon {
    fn new(pos: DVec3) -> Self {
        let mut d = Self {
            pos,
            previous_pos: pos,
            vel: DVec3::ZERO,
            yaw: 0.0,
            previous_yaw: 0.0,
            yaw_speed: 0.0,
            health: MAX_HEALTH,
            phase: Phase::HoldingPattern,
            phase_ticks: 0,
            target: None,
            path: Vec::new(),
            clockwise: false,
            fireball_charge: 0,
            flame_count: 0,
            sitting_damage: 0.0,
            healer: None,
            hurt: 0.0,
            immune: (0, 0.0),
            death_ticks: 0,
            flap: 0.0,
            previous_flap: 0.0,
            history: [(0.0, pos.y); 64],
            history_at: 0,
            parts: [(Part::Body, pos, pos); 8],
            growl: 100,
        };
        d.place_parts();
        d
    }

    /// Yaw and height `ticks` ago.
    pub fn latency(&self, ticks: usize) -> (f32, f64) {
        self.history[(self.history_at + 64 - ticks.min(63)) % 64]
    }

    pub fn parts(&self) -> &[(Part, DVec3, DVec3); 8] {
        &self.parts
    }

    fn set_phase(&mut self, phase: Phase) {
        if self.phase == Phase::Dying || self.phase == phase {
            return;
        }
        self.phase = phase;
        self.phase_ticks = 0;
        self.target = None;
        self.path.clear();
        self.fireball_charge = 0;
        if phase == Phase::SittingFlaming {
            self.flame_count += 1;
        }
        if phase == Phase::Landing {
            self.flame_count = 0;
        }
    }

    /// Damage to `part`; `counts` is false for sources that can't hurt the
    /// dragon. Returns whether it took any.
    fn hurt_by(&mut self, part: Part, damage: f32, counts: bool) -> bool {
        if self.phase == Phase::Dying || !counts {
            return false;
        }
        let damage = part_damage(part, damage);
        if damage < 0.01 {
            return false;
        }
        // Java's ten-tick window: a second hit only does what it adds.
        let dealt = if self.immune.0 > 0 {
            if damage <= self.immune.1 {
                return false;
            }
            damage - self.immune.1
        } else {
            self.hurt = 0.5;
            damage
        };
        self.immune = (10, damage);
        let before = self.health;
        self.health = (self.health - dealt).max(0.0);
        if self.health <= 0.0 {
            self.health = 0.0;
            self.phase = Phase::Dying;
            self.phase_ticks = 0;
            return true;
        }
        if self.phase.sitting() {
            self.sitting_damage += before - self.health;
            if self.sitting_damage > 0.25 * MAX_HEALTH {
                self.sitting_damage = 0.0;
                self.set_phase(Phase::Takeoff);
            }
        }
        true
    }

    /// One tick; returns `false` once the death is over.
    fn tick<W: BlockSource + ?Sized>(
        &mut self,
        fight: &mut Fight,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> bool {
        self.previous_pos = self.pos;
        self.previous_yaw = self.yaw;
        self.previous_flap = self.flap;
        self.hurt = (self.hurt - TICK as f32).max(0.0);
        if self.immune.0 > 0 {
            self.immune.0 -= 1;
        }
        self.history_at = (self.history_at + 1) % 64;
        self.history[self.history_at] = (self.yaw, self.pos.y);
        if self.phase == Phase::Dying {
            return self.tick_death(fight, events);
        }
        self.phase_ticks += 1;

        // Wing beats: slower the faster it flies, quicker while climbing.
        let flap = if self.phase.sitting() {
            0.1
        } else {
            let speed = self.vel.with_y(0.0).length() as f32;
            0.2 / (speed * 10.0 + 1.0) * 2f32.powf(self.vel.y as f32)
        };
        let before = self.flap;
        self.flap += flap * 0.5;
        if before.cos_turns() > -0.3 && self.flap.cos_turns() <= -0.3 && !self.phase.sitting() {
            events.push(EntityEvent::Sound { sound: MobSound::DragonFlap, pos: self.pos });
        }
        self.growl = self.growl.saturating_sub(1);
        if self.growl == 0 {
            self.growl = 200 + (rng.next_f32() * 200.0) as u32;
            events.push(EntityEvent::Sound { sound: MobSound::DragonGrowl, pos: self.head() });
        }

        self.think(fight, world, ctx, rng, events);
        self.fly(world);
        self.place_parts();
        self.heal(fight, rng);
        if !self.phase.sitting() {
            self.smash(world, events);
        }
        self.hit_players(ctx, events);
        true
    }

    /// Java's phase logic (`DragonPhaseInstance.doServerTick`).
    fn think<W: BlockSource + ?Sized>(
        &mut self,
        fight: &mut Fight,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) {
        let near = |r: f64, from: DVec3| {
            ctx.players
                .iter()
                .filter(|t| t.targetable && t.pos.distance_squared(from) < r * r)
                .min_by(|a, b| a.pos.distance_squared(from).total_cmp(&b.pos.distance_squared(from)))
                .copied()
        };
        match self.phase {
            Phase::HoldingPattern => {
                if self.target_reached() {
                    self.holding_target(fight, ctx, rng);
                }
            }
            Phase::Strafe(id) => {
                let Some(t) = ctx.players.iter().find(|t| t.id == id && t.targetable).copied() else {
                    self.set_phase(Phase::HoldingPattern);
                    return;
                };
                let dist = (t.pos - self.pos).with_y(0.0).length();
                let lift = (0.4 + dist / 80.0 - 1.0).min(10.0);
                self.target = Some(t.pos + DVec3::Y * lift);
                let aim = (t.pos - self.pos).with_y(0.0).normalize_or_zero();
                let facing = heading(self.yaw);
                let close = t.pos.distance_squared(self.pos) < 64.0 * 64.0;
                if close && super::mob::line_of_sight(world, self.head(), t.pos + DVec3::Y * 0.9) {
                    self.fireball_charge += 1;
                    let angle = facing.dot(aim).clamp(-1.0, 1.0).acos().to_degrees() + 0.5;
                    if self.fireball_charge >= 5 && angle < 10.0 {
                        let from = self.head() - facing;
                        let dir = (t.pos + DVec3::Y * 0.9 - from).normalize_or(facing);
                        fight.fireballs.push(DragonFireball::new(from, dir));
                        events.push(EntityEvent::Sound { sound: MobSound::DragonShoot, pos: from });
                        self.set_phase(Phase::HoldingPattern);
                    }
                } else {
                    self.fireball_charge = self.fireball_charge.saturating_sub(1);
                }
                if self.phase_ticks > 400 {
                    self.set_phase(Phase::HoldingPattern);
                }
            }
            Phase::LandingApproach => {
                if self.path.is_empty() && self.target.is_none() {
                    // In from the side away from the nearest player.
                    let podium = fight.perch(world);
                    let away = near(150.0, podium).map_or(DVec3::X, |t| -t.pos.with_y(0.0).normalize_or(DVec3::NEG_X));
                    let entry = fight.closest_node(DVec3::new(away.x * 40.0, 105.0, away.z * 40.0), 12..24);
                    self.path = vec![podium, fight.nodes[entry]];
                    self.next_waypoint(rng, true);
                } else if self.target_reached() {
                    if self.path.is_empty() {
                        self.set_phase(Phase::Landing);
                    } else {
                        let last = self.path.len() == 1;
                        self.next_waypoint(rng, !last);
                    }
                }
            }
            Phase::Landing => {
                let perch = fight.perch(world);
                self.target = Some(perch);
                if perch.distance_squared(self.pos) < 1.0 || self.phase_ticks > 400 {
                    self.pos = perch;
                    self.vel = DVec3::ZERO;
                    self.set_phase(Phase::SittingFlaming);
                }
            }
            Phase::SittingFlaming => {
                if self.phase_ticks >= 200 {
                    let next = if self.flame_count >= 4 { Phase::Takeoff } else { Phase::SittingScanning };
                    self.set_phase(next);
                } else if self.phase_ticks == 10 {
                    // A cloud of breath in front, on the ground.
                    let ahead = (self.head() - self.pos).with_y(0.0).normalize_or_zero();
                    let mut at = self.head() + ahead * 2.5;
                    let start = at.y;
                    while world.block(at.floor().as_ivec3()) == Some(Block::AIR) {
                        at.y -= 1.0;
                        if at.y < 0.0 {
                            at.y = start;
                            break;
                        }
                    }
                    at.y = at.y.floor() + 1.0;
                    fight.clouds.push(BreathCloud::new(at, 5.0, 0.0, 200, 3.0));
                    events.push(EntityEvent::Sound { sound: MobSound::DragonShoot, pos: at });
                }
            }
            Phase::SittingScanning => {
                if let Some(t) = near(20.0, self.pos) {
                    if self.phase_ticks > 25 {
                        self.set_phase(Phase::SittingAttacking);
                        events.push(EntityEvent::Sound { sound: MobSound::DragonGrowl, pos: self.head() });
                    } else {
                        // Turn to face them.
                        let to = (t.pos - self.head()).with_y(0.0);
                        let facing = heading(self.yaw);
                        let angle = facing.dot(to.normalize_or_zero()).clamp(-1.0, 1.0).acos().to_degrees() + 0.5;
                        if !(0.0..=10.0).contains(&angle) {
                            let want = 180.0 - (to.x.atan2(to.z) as f32).to_degrees();
                            let turn = wrap_degrees(want - self.yaw).clamp(-100.0, 100.0);
                            let len = to.length() as f32 + 1.0;
                            self.yaw_speed = self.yaw_speed * 0.8 + turn * 0.7 / len.min(40.0) / len;
                            self.yaw += self.yaw_speed;
                        }
                    }
                } else if self.phase_ticks >= 100 {
                    match near(150.0, self.pos) {
                        Some(t) => {
                            self.set_phase(Phase::Charging);
                            self.target = Some(t.pos);
                        }
                        None => self.set_phase(Phase::Takeoff),
                    }
                }
            }
            Phase::SittingAttacking => {
                if self.phase_ticks >= 40 {
                    self.set_phase(Phase::SittingFlaming);
                }
            }
            Phase::Takeoff => {
                if self.path.is_empty() && self.target.is_none() {
                    // Out the way it faces, to a node beyond the pillars.
                    let out = heading(self.yaw);
                    let node = fight.closest_node(DVec3::new(out.x * 40.0, 105.0, out.z * 40.0), 0..24);
                    let node = node % 12;
                    self.path = vec![fight.nodes[node]];
                    self.target = Some(self.pos + DVec3::Y * 12.0 + out * 6.0);
                } else if self.target_reached() {
                    self.next_waypoint(rng, true);
                }
                let perch = fight.perch(world);
                if self.pos.distance_squared(perch) > 10.0 * 10.0 && self.phase_ticks > 20 {
                    self.set_phase(Phase::HoldingPattern);
                }
            }
            Phase::Charging => {
                let reached = self.target.is_none_or(|t| {
                    let d = t.distance_squared(self.pos);
                    !(100.0..=22500.0).contains(&d)
                });
                if reached || self.phase_ticks > 200 {
                    self.fireball_charge += 1;
                    if self.fireball_charge >= 10 {
                        self.set_phase(Phase::HoldingPattern);
                    }
                }
            }
            Phase::Dying => {}
        }
    }

    /// Whether the dragon is close to (or impossibly far from) its target.
    fn target_reached(&self) -> bool {
        self.target.is_none_or(|t| !(100.0..=22500.0).contains(&t.distance_squared(self.pos)))
    }

    /// Flies on to the next waypoint, up to 20 blocks above it when `lift`.
    fn next_waypoint(&mut self, rng: &mut Rng, lift: bool) {
        let Some(node) = self.path.pop() else {
            self.target = None;
            return;
        };
        let up = if lift { rng.next_f32() as f64 * 20.0 } else { 0.0 };
        self.target = Some(node + DVec3::Y * up);
    }

    /// Java's `HoldingPatternPhase.findNewTarget`: maybe land or strafe,
    /// otherwise on round the pillars.
    fn holding_target(&mut self, fight: &Fight, ctx: &Ctx, rng: &mut Rng) {
        if self.path.is_empty() && self.target.is_some() {
            let alive = fight.crystals.len() as u32;
            if (rng.next_f32() * (alive + 3) as f32) < 1.0 {
                self.set_phase(Phase::LandingApproach);
                return;
            }
            let center = fight.podium.as_dvec3();
            let player = ctx
                .players
                .iter()
                .filter(|t| t.targetable && t.pos.distance_squared(center) < 150.0 * 150.0)
                .min_by(|a, b| a.pos.distance_squared(center).total_cmp(&b.pos.distance_squared(center)));
            if let Some(t) = player {
                let odds = (t.pos.distance_squared(center) / 512.0).abs() as u32 + 2;
                if (rng.next_f32() * odds as f32) < 1.0 || (rng.next_f32() * (alive + 2) as f32) < 1.0 {
                    self.set_phase(Phase::Strafe(t.id));
                    return;
                }
            }
        }
        if self.path.is_empty() {
            let from = fight.closest_node(self.pos, 0..12);
            let mut to = from as i32;
            if rng.next_f32() < 0.125 {
                self.clockwise = !self.clockwise;
                to += 6;
            }
            to = (to + if self.clockwise { 1 } else { -1 }).rem_euclid(12);
            let to = to as usize;
            self.path.push(fight.nodes[to]);
            // Across the ring, pass over the inner nodes.
            if (to as i32 - from as i32).rem_euclid(12) > 1 && (from as i32 - to as i32).rem_euclid(12) > 1 {
                let mid = (fight.nodes[from] + fight.nodes[to]) / 2.0;
                self.path.push(fight.nodes[fight.closest_node(mid, 12..20)]);
            }
        }
        self.next_waypoint(rng, true);
    }

    /// Fly speed and turn rate of the phase (Java's `getFlySpeed` and
    /// `getTurnSpeed`).
    fn flight(&self) -> (f64, f32) {
        let speed = self.vel.with_y(0.0).length() as f32 + 1.0;
        let default_turn = 0.7 / speed.min(40.0) / speed;
        match self.phase {
            Phase::Landing => (1.5, speed.min(40.0) / speed),
            Phase::Charging => (3.0, default_turn),
            _ => (0.6, default_turn),
        }
    }

    /// Java's `EnderDragon.aiStep` movement toward the target.
    fn fly<W: BlockSource + ?Sized>(&mut self, world: &W) {
        if self.phase.sitting() {
            self.vel = DVec3::ZERO;
            return;
        }
        let Some(target) = self.target else {
            self.vel *= 0.8;
            self.pos += self.vel;
            return;
        };
        let (fly_speed, turn_speed) = self.flight();
        let d = target - self.pos;
        let horizontal = d.with_y(0.0).length();
        let climb = if horizontal > 0.0 { (d.y / horizontal).clamp(-fly_speed, fly_speed) } else { d.y };
        self.vel.y += climb * 0.01;
        self.yaw = wrap_degrees(self.yaw);
        let to_target = d.normalize_or_zero();
        let facing = DVec3::new(heading(self.yaw).x, self.vel.y, heading(self.yaw).z).normalize_or_zero();
        let align = ((facing.dot(to_target) as f32 + 0.5) / 1.5).max(0.0);
        if d.x.abs() > 1e-5 || d.z.abs() > 1e-5 {
            let want = 180.0 - (d.x.atan2(d.z) as f32).to_degrees();
            let turn = wrap_degrees(want - self.yaw).clamp(-50.0, 50.0);
            self.yaw_speed = self.yaw_speed * 0.8 + turn * turn_speed;
            self.yaw += self.yaw_speed * 0.1;
        }
        let near = 2.0 / (d.length_squared() as f32 + 1.0);
        let thrust = 0.06 * (align * near + (1.0 - near)) as f64;
        self.vel += heading(self.yaw) * thrust;
        // Java slows it inside solid blocks it can't break.
        let in_wall = self.parts[..3].iter().any(|&(_, min, max)| {
            let (lo, hi) = (min.floor().as_ivec3(), max.floor().as_ivec3());
            (lo.y..=hi.y).any(|y| {
                (lo.z..=hi.z).any(|z| (lo.x..=hi.x).any(|x| world.block(IVec3::new(x, y, z)).is_some_and(immune_solid)))
            })
        });
        self.pos += if in_wall { self.vel * 0.8 } else { self.vel };
        self.pos.y = self.pos.y.min(crate::world::chunk::WORLD_HEIGHT as f64 + 16.0);
        let along = (self.vel.normalize_or_zero().dot(facing) as f32 + 1.0) / 2.0;
        let keep = 0.8 + 0.15 * along as f64;
        self.vel *= DVec3::new(keep, 0.91, keep);
    }

    /// Head position (centre of its box).
    pub fn head(&self) -> DVec3 {
        part_center(self, Part::Head)
    }

    /// Java's part offsets, from the yaw and recent heights.
    fn place_parts(&mut self) {
        let r = self.yaw.to_radians();
        let (s, c) = (r.sin() as f64, r.cos() as f64);
        let tilt = ((self.latency(5).1 - self.latency(10).1) * 10.0).to_radians();
        let (tc, ts) = (tilt.cos(), tilt.sin());
        let head_y = if self.phase.sitting() { -1.0 } else { self.latency(5).1 - self.latency(0).1 };
        let hr = (self.yaw - self.yaw_speed * 0.01).to_radians();
        let (hs, hc) = (hr.sin() as f64, hr.cos() as f64);
        let base = self.latency(5);
        let tail = |k: usize| {
            let old = self.latency(12 + k * 2);
            let a = (self.yaw + wrap_degrees(old.0 - base.0)).to_radians();
            let (ts2, tc2) = (a.sin() as f64, a.cos() as f64);
            let back = (k + 1) as f64 * 2.0;
            DVec3::new(
                -(s * 1.5 + ts2 * back) * tc,
                old.1 - base.1 - (back + 1.5) * ts + 1.5,
                (c * 1.5 + tc2 * back) * tc,
            )
        };
        let offsets = [
            (Part::Body, DVec3::new(s * 0.5, 0.0, -c * 0.5)),
            (Part::Wing(0), DVec3::new(c * 4.5, 2.0, s * 4.5)),
            (Part::Wing(1), DVec3::new(-c * 4.5, 2.0, -s * 4.5)),
            (Part::Head, DVec3::new(hs * 6.5 * tc, head_y + ts * 6.5, -hc * 6.5 * tc)),
            (Part::Neck, DVec3::new(hs * 5.5 * tc, head_y + ts * 5.5, -hc * 5.5 * tc)),
            (Part::Tail(0), tail(0)),
            (Part::Tail(1), tail(1)),
            (Part::Tail(2), tail(2)),
        ];
        self.parts = offsets.map(|(part, offset)| {
            let (w, h) = part.size();
            let feet = self.pos + offset;
            (part, feet - DVec3::new(w / 2.0, 0.0, w / 2.0), feet + DVec3::new(w / 2.0, h, w / 2.0))
        });
    }

    /// Picks the nearest crystal now and then, and heals a point every ten
    /// ticks while one is in range.
    fn heal(&mut self, fight: &Fight, rng: &mut Rng) {
        if self.healer.is_some_and(|i| i >= fight.crystals.len()) {
            self.healer = None;
        }
        if self.healer.is_some() && self.phase_ticks.is_multiple_of(10) && self.health < MAX_HEALTH {
            self.health = (self.health + 1.0).min(MAX_HEALTH);
        }
        if rng.next_f32() < 0.1 {
            // Java searches its 16-block-long box grown by 32.
            let reach = HEAL_RANGE + 8.0;
            self.healer = fight
                .crystals
                .iter()
                .enumerate()
                .filter(|(_, c)| c.center().distance_squared(self.pos) < reach * reach)
                .min_by(|a, b| {
                    a.1.center().distance_squared(self.pos).total_cmp(&b.1.center().distance_squared(self.pos))
                })
                .map(|(i, _)| i);
        }
    }

    /// Breaks the blocks in its head, neck and body (Java's `checkWalls`).
    fn smash<W: BlockSource + ?Sized>(&self, world: &W, events: &mut Vec<EntityEvent>) {
        let mut broke = false;
        for (_, min, max) in [0, 3, 4].map(|i| self.parts[i]) {
            let (lo, hi) = (min.floor().as_ivec3(), max.floor().as_ivec3());
            for y in lo.y..=hi.y {
                for z in lo.z..=hi.z {
                    for x in lo.x..=hi.x {
                        let cell = IVec3::new(x, y, z);
                        if world.block(cell).is_some_and(breakable) {
                            events.push(EntityEvent::BreakBlock { cell });
                            broke = true;
                        }
                    }
                }
            }
        }
        if broke {
            events.push(EntityEvent::Sound { sound: MobSound::DragonSmash, pos: self.pos });
        }
    }

    /// Wings shove players away (and hurt them in flight); the head and neck
    /// bite.
    fn hit_players(&self, ctx: &Ctx, events: &mut Vec<EntityEvent>) {
        if self.immune.0 > 0 {
            return;
        }
        let body = part_center(self, Part::Body);
        let overlaps = |t: &Target, min: DVec3, max: DVec3| {
            let (pmin, pmax) = crate::player::SHAPE.aabb(t.pos);
            pmin.cmplt(max).all() && pmax.cmpgt(min).all()
        };
        for t in ctx.players.iter().filter(|t| t.targetable) {
            let wing = self.parts[1..3]
                .iter()
                .any(|&(_, min, max)| overlaps(t, min - DVec3::new(4.0, 4.0, 4.0), max + DVec3::new(4.0, 0.0, 4.0)));
            let bite = self.parts[3..5].iter().any(|&(_, min, max)| overlaps(t, min - DVec3::ONE, max + DVec3::ONE));
            if bite {
                events.push(EntityEvent::PlayerHit {
                    player: t.id,
                    damage: 10.0,
                    knockback: glam::Vec3::ZERO,
                    cause: "was slain by the Ender Dragon",
                });
            } else if wing {
                let away = (t.pos - body).with_y(0.0);
                let d2 = away.length_squared().max(0.1);
                // Java pushes 4/d blocks a tick; in blocks a second, capped.
                let push = (away / d2 * 4.0 * 20.0).clamp_length_max(30.0) + DVec3::Y * 4.0;
                let damage = if self.phase.sitting() { 0.0 } else { 5.0 };
                events.push(EntityEvent::PlayerHit {
                    player: t.id,
                    damage,
                    knockback: push.as_vec3(),
                    cause: "was slain by the Ender Dragon",
                });
            }
        }
    }

    /// Rises and spins for ten seconds, spilling experience toward the end.
    fn tick_death(&mut self, fight: &mut Fight, events: &mut Vec<EntityEvent>) -> bool {
        self.death_ticks += 1;
        self.healer = None;
        let xp = if fight.previously_killed { LATER_XP } else { FIRST_XP };
        if self.death_ticks == 1 {
            events.push(EntityEvent::Sound { sound: MobSound::DragonDeath, pos: self.pos });
        }
        if self.death_ticks > 150 && self.death_ticks.is_multiple_of(5) {
            events.push(EntityEvent::DragonXp { pos: self.pos, points: (xp as f32 * 0.08) as u32 });
        }
        self.pos.y += 0.1;
        self.yaw += 20.0;
        self.flap += 0.02;
        self.place_parts();
        if self.death_ticks >= DEATH_TICKS {
            events.push(EntityEvent::DragonXp { pos: self.pos, points: (xp as f32 * 0.2) as u32 });
            return false;
        }
        true
    }
}

/// Blocks the dragon can't break and is slowed by.
fn immune_solid(b: Block) -> bool {
    b.is_solid() && !breakable(b)
}

/// Java's `#dragon_immune` and `#dragon_transparent`: the End's own blocks,
/// bedrock and portals survive; air, fire and fluids aren't in the way.
fn breakable(b: Block) -> bool {
    let base = b.base();
    !(b == Block::AIR
        || b.is_fluid()
        || base == Block::FIRE
        || b == Block::BEDROCK
        || b == Block::OBSIDIAN
        || b == Block::END_STONE
        || b == Block::IRON_BARS
        || base == Block::END_PORTAL_FRAME
        || b == Block::END_PORTAL
        || b == Block::NETHER_PORTAL)
}

impl DragonFireball {
    fn new(from: DVec3, dir: DVec3) -> Self {
        Self { pos: from, previous_pos: from, vel: dir * 0.1, dir, age: 0 }
    }

    pub fn heading(&self) -> glam::Vec3 {
        self.dir.as_vec3()
    }

    /// Moves one tick; on hitting a block or player it bursts into a cloud
    /// of breath (on the nearest player within four blocks).
    fn tick<W: BlockSource + ?Sized>(&mut self, world: &W, ctx: &Ctx, clouds: &mut Vec<BreathCloud>) -> bool {
        self.previous_pos = self.pos;
        self.age += 1;
        self.vel = (self.vel + self.dir * 0.1) * 0.95;
        let steps = (self.vel.length() / 0.2).ceil().max(1.0) as u32;
        for _ in 0..steps {
            let next = self.pos + self.vel / steps as f64;
            let hit_block = world.block(next.floor().as_ivec3()).is_some_and(|b| b.is_solid());
            let hit_player = ctx.players.iter().any(|t| t.targetable && t.contains(next));
            if hit_block || hit_player {
                let near = ctx
                    .players
                    .iter()
                    .filter(|t| t.targetable && t.pos.distance_squared(self.pos) < 16.0)
                    .min_by(|a, b| a.pos.distance_squared(self.pos).total_cmp(&b.pos.distance_squared(self.pos)));
                let at = near.map_or(self.pos, |t| t.pos);
                clouds.push(BreathCloud::new(at, 3.0, 4.0 / 600.0, 600, 6.0));
                return false;
            }
            self.pos = next;
        }
        self.age < 600 && self.pos.y > -64.0
    }
}

impl BreathCloud {
    fn new(pos: DVec3, radius: f32, grow: f32, duration: u32, damage: f32) -> Self {
        Self { pos, radius, grow, age: 0, duration, damage, cooldowns: Vec::new() }
    }

    /// Spreads and hurts players standing in it, once a second each.
    fn tick(&mut self, ctx: &Ctx, events: &mut Vec<EntityEvent>) -> bool {
        self.age += 1;
        self.radius += self.grow;
        self.cooldowns.retain_mut(|(_, t)| {
            *t -= 1;
            *t > 0
        });
        // Java waits ten ticks before the first dose.
        if self.age > 10 {
            for t in ctx.players.iter().filter(|t| t.targetable) {
                let d = t.pos - self.pos;
                let inside = d.y > -crate::player::HEIGHT && d.y < 0.5 && d.with_y(0.0).length() <= self.radius as f64;
                if inside && !self.cooldowns.iter().any(|&(id, _)| id == t.id) {
                    self.cooldowns.push((t.id, 20));
                    events.push(EntityEvent::PlayerHit {
                        player: t.id,
                        damage: self.damage,
                        knockback: glam::Vec3::ZERO,
                        cause: "was killed by dragon's breath",
                    });
                }
            }
        }
        self.age < self.duration
    }
}

trait CosTurns {
    fn cos_turns(self) -> f32;
}

impl CosTurns for f32 {
    fn cos_turns(self) -> f32 {
        (self * std::f32::consts::TAU).cos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::terrain::Dimension;

    struct Air;
    impl BlockSource for Air {
        fn block(&self, _p: IVec3) -> Option<Block> {
            Some(Block::AIR)
        }
    }

    fn ctx(players: Vec<Target>) -> Ctx {
        Ctx { players, daylight: 0.0, spawning: false, raining: false, dimension: Dimension::End }
    }

    #[test]
    fn a_new_fight_has_a_crystal_on_every_pillar_and_saves() {
        let end = EndGen::new(3);
        let fight = Fight::new(&end);
        assert_eq!(fight.crystals.len(), 10);
        assert_eq!(fight.serialize(), "1,0,200,1023");
        let loaded = Fight::load(&end, Some("1,1,57.5,5"));
        assert_eq!(loaded.crystals.len(), 2);
        assert!(loaded.previously_killed);
        assert_eq!(loaded.dragon.as_ref().unwrap().health, 57.5);
        let dead = Fight::load(&end, Some("0,1,0,0"));
        assert!(dead.dragon.is_none() && dead.crystals.is_empty());

        // Saved while dying: the portal opens (with the egg) on reload.
        let mut dying = Fight::new(&end);
        dying.dragon.as_mut().unwrap().hurt_by(Part::Head, 500.0, true);
        assert_eq!(dying.serialize(), "2,0,0,1023");
        let mut reloaded = Fight::load(&end, Some(&dying.serialize()));
        let mut events = Vec::new();
        reloaded.update(TICK, &Air, &ctx(Vec::new()), &mut Rng::new(1), &mut events);
        assert!(events.contains(&EntityEvent::DragonKilled { first: true }));
        assert_eq!(reloaded.serialize(), "0,1,0,1023");
    }

    #[test]
    fn the_dragon_circles_the_pillars() {
        let end = EndGen::new(5);
        let mut fight = Fight::new(&end);
        let mut rng = Rng::new(1);
        let mut events = Vec::new();
        let mut far = 0;
        for _ in 0..20 * 60 {
            fight.update(TICK, &Air, &ctx(Vec::new()), &mut rng, &mut events);
            let d = fight.dragon.as_ref().unwrap();
            assert!(d.pos.is_finite());
            if d.pos.with_y(0.0).length() > 30.0 {
                far += 1;
            }
        }
        // It spends most of its time out around the pillar ring.
        assert!(far > 20 * 30, "{far}");
    }

    #[test]
    fn head_hits_count_fully_and_body_hits_quarter() {
        let end = EndGen::new(5);
        let mut fight = Fight::new(&end);
        assert!(fight.strike(Hit::Dragon(Part::Head), 8.0, Some(PlayerId::HOST), false));
        assert_eq!(fight.dragon.as_ref().unwrap().health, 192.0);
        // A weaker hit inside the immunity window does nothing.
        assert!(!fight.strike(Hit::Dragon(Part::Body), 8.0, None, false));
        fight.dragon.as_mut().unwrap().immune = (0, 0.0);
        assert!(fight.strike(Hit::Dragon(Part::Wing(0)), 8.0, None, false));
        assert_eq!(fight.dragon.as_ref().unwrap().health, 189.0);
    }

    #[test]
    fn breaking_the_healing_crystal_hurts_the_dragon_and_explodes() {
        let end = EndGen::new(5);
        let mut fight = Fight::new(&end);
        fight.dragon.as_mut().unwrap().healer = Some(4);
        fight.hit_crystal(4, Some(PlayerId::HOST));
        let mut events = Vec::new();
        fight.update(TICK, &Air, &ctx(Vec::new()), &mut Rng::new(1), &mut events);
        assert_eq!(fight.crystals.len(), 9);
        assert!(events.iter().any(|e| matches!(e, EntityEvent::Explosion { power, .. } if *power == CRYSTAL_POWER)));
        assert!(fight.dragon.as_ref().unwrap().health <= 190.0);
    }

    #[test]
    fn death_takes_ten_seconds_and_pays_out_experience() {
        let end = EndGen::new(5);
        let mut fight = Fight::new(&end);
        fight.crystals.clear();
        fight.dragon.as_mut().unwrap().hurt_by(Part::Head, 500.0, true);
        let mut events = Vec::new();
        let mut rng = Rng::new(1);
        for _ in 0..DEATH_TICKS + 5 {
            fight.update(TICK, &Air, &ctx(Vec::new()), &mut rng, &mut events);
        }
        assert!(fight.dragon.is_none());
        assert!(fight.previously_killed);
        let xp: u32 = events
            .iter()
            .filter_map(|e| if let EntityEvent::DragonXp { points, .. } = e { Some(*points) } else { None })
            .sum();
        assert_eq!(xp, 10 * 960 + 2400);
        assert!(events.contains(&EntityEvent::DragonKilled { first: true }));
        assert_eq!(fight.serialize(), "0,1,0,0");
    }

    #[test]
    fn strafing_dragons_spit_fireballs_that_leave_breath() {
        let end = EndGen::new(5);
        let mut fight = Fight::new(&end);
        let player = Target::new(PlayerId::HOST, DVec3::new(0.0, 70.0, 0.0), true);
        let d = fight.dragon.as_mut().unwrap();
        d.pos = DVec3::new(0.0, 90.0, 50.0);
        d.yaw = 180.0;
        d.set_phase(Phase::Strafe(PlayerId::HOST));
        let mut rng = Rng::new(2);
        let mut events = Vec::new();
        let mut shot = false;
        for _ in 0..20 * 30 {
            fight.update(TICK, &Air, &ctx(vec![player]), &mut rng, &mut events);
            shot |= !fight.fireballs.is_empty();
            if !fight.clouds.is_empty() {
                break;
            }
        }
        assert!(shot, "the dragon never fired");
        assert_eq!(fight.clouds.len(), 1);
        assert!(fight.clouds[0].pos.distance(player.pos) < 4.0);
    }

    #[test]
    fn perched_dragons_shrug_off_arrows() {
        let end = EndGen::new(5);
        let mut fight = Fight::new(&end);
        fight.dragon.as_mut().unwrap().phase = Phase::SittingScanning;
        assert!(!fight.strike(Hit::Dragon(Part::Head), 9.0, Some(PlayerId::HOST), true));
        assert!(fight.strike(Hit::Dragon(Part::Head), 9.0, Some(PlayerId::HOST), false));
    }

    #[test]
    fn breath_clouds_hurt_once_a_second() {
        let mut cloud = BreathCloud::new(DVec3::ZERO, 3.0, 0.0, 100, 6.0);
        let players = vec![Target::new(PlayerId::HOST, DVec3::new(1.0, 0.0, 0.0), true)];
        let mut events = Vec::new();
        for _ in 0..40 {
            cloud.tick(&ctx(players.clone()), &mut events);
        }
        assert_eq!(events.len(), 2);
    }
}
