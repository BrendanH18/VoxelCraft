//! v0.16 Track A: shulkers and shulker bullets (MOB-04b). Java 26.3
//! references: `Shulker` (`createAttributes`, `tick`, `canStayAt`,
//! `findAttachableSurface`, `teleportSomewhere`, `hurtServer`,
//! `hitByShulkerBullet`, `setRawPeekAmount`, `updatePeekAmount` and the
//! goals `ShulkerAttackGoal`, `ShulkerPeekGoal`, `ShulkerNearestAttackGoal`
//! and `HurtByTargetGoal`), `ShulkerBullet` (`selectNextMoveDirection`,
//! `tick`, `onHitEntity`, `onHitBlock`, `hurtServer`), and the loot table
//! `entities/shulker`. Constants are checked against
//! `tests/fixtures/java/26.3/end_cities.json`.
//!
//! A shulker never moves on its own: it sits in its block, stuck to the
//! face `attach`, opens its lid to peek or to shoot, and is armored while
//! shut. Shulkers come only from End city and ship `Sentry` markers
//! ([`Entities::populate_end_cities`]), from duplication, and from commands.
use glam::{DVec3, IVec3};
use serde_json::{Value, json};

use super::{Ctx, Entities, EntityEvent, Mob, MobKind, MobSound, MobWorld, PlayerId};
use crate::color::DyeColor;
use crate::simulation::damage::{Cause, DamageType};
use crate::simulation::difficulty::Difficulty;
use crate::world::block::Block;
use crate::world::end_blocks::Dir;

/// `Shulker.createAttributes`: max health 30.
pub const MAX_HEALTH: f32 = 30.0;
/// `xpReward`.
pub const EXPERIENCE: u32 = 5;
/// `COVERED_ARMOR_MODIFIER`: armor while the lid is shut.
pub const COVERED_ARMOR: u32 = 20;
/// `PEEK_PER_TICK`.
pub const PEEK_PER_TICK: f32 = 0.05;
/// `MAX_TELEPORT_DISTANCE`, `OTHER_SHULKER_SCAN_RADIUS`, `OTHER_SHULKER_LIMIT`.
pub const MAX_TELEPORT_DISTANCE: i32 = 8;
pub const OTHER_SHULKER_SCAN_RADIUS: f64 = 8.0;
pub const OTHER_SHULKER_LIMIT: f32 = 5.0;
/// `Mob.createMobAttributes`' follow range.
pub const FOLLOW_RANGE: f64 = 16.0;
/// `ShulkerBullet.SPEED` and its hit: 4 damage and 200 ticks of Levitation.
pub const BULLET_SPEED: f64 = 0.15;
pub const BULLET_DAMAGE: f32 = 4.0;
pub const LEVITATION_TICKS: u32 = 200;
/// `ShulkerBullet.getDefaultGravity` when its target is gone.
pub const BULLET_GRAVITY: f64 = 0.04;
/// `DamageSources.mobProjectile`.
pub const BULLET_CAUSE: Cause = Cause::new(DamageType::MobProjectile, "was shot by a shulker");
const TICK: f64 = 0.05;

/// The sounds a shulker makes besides its voice, played with the nearest
/// shared sounds (`SHULKER_OPEN`, `CLOSE`, `SHOOT`, `TELEPORT`,
/// `SHULKER_BULLET_HIT` and `HURT`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cue {
    Open,
    Close,
    Shoot,
    Teleport,
    BulletHit,
    BulletHurt,
}

/// A shulker's state beside the shared mob record.
#[derive(Clone, Debug)]
pub struct State {
    /// `DATA_ATTACH_FACE_ID`: the face of its cell it clings to.
    pub attach: Dir,
    /// `DATA_PEEK_ID`: how far the lid is asked to open, 0–100.
    pub peek: u8,
    /// `currentPeekAmount` and its value a tick before, 0..1.
    pub amount: f32,
    pub amount_before: f32,
    /// `DATA_COLOR_ID`: a dye colour, or none (16) for the default shell.
    pub color: Option<DyeColor>,
    /// The player it is after (`getTarget`).
    pub target: Option<PlayerId>,
    attacking: bool,
    attack_time: i32,
    peeking: bool,
    peek_time: i32,
    /// An accepted hit to react to on the next tick (`hurtServer`'s
    /// teleport and duplication).
    pub(crate) hurt: bool,
    pub(crate) bullet_hit: bool,
    carry: f64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            attach: Dir::Down,
            peek: 0,
            amount: 0.0,
            amount_before: 0.0,
            color: None,
            target: None,
            attacking: false,
            attack_time: 0,
            peeking: false,
            peek_time: 0,
            hurt: false,
            bullet_hit: false,
            carry: 0.0,
        }
    }
}

impl State {
    /// `isClosed`.
    pub fn closed(&self) -> bool {
        self.peek == 0
    }

    /// The armor the shell gives: 20 while shut.
    pub fn armor(&self) -> u32 {
        if self.closed() { COVERED_ARMOR } else { 0 }
    }

    /// `AttachFace` (Java's direction id), `Peek` and `Color` (16 for none).
    pub fn save(&self) -> Value {
        json!({"attach_face": self.attach as u8, "peek": self.peek, "color": self.color.map_or(16, |c| c as u8)})
    }

    pub fn load(v: &Value) -> Self {
        let attach = v["attach_face"].as_u64().and_then(|i| Dir::ALL.get(i as usize).copied()).unwrap_or(Dir::Down);
        let peek = v["peek"].as_u64().map_or(0, |p| p.min(100) as u8);
        let color = v["color"].as_u64().and_then(|c| DyeColor::ALL.get(c as usize).copied());
        let amount = f32::from(peek) * 0.01;
        Self { attach, peek, color, amount, amount_before: amount, ..Self::default() }
    }

    /// What a LAN client draws.
    pub fn look(&self) -> Value {
        json!({"attach_face": self.attach as u8, "amount": self.amount, "color": self.color.map_or(16, |c| c as u8)})
    }

    pub fn apply_look(&mut self, v: &Value) {
        let loaded = Self::load(v);
        self.attach = loaded.attach;
        self.color = loaded.color;
        self.amount_before = self.amount;
        self.amount = v["amount"].as_f64().map_or(0.0, |a| a.clamp(0.0, 1.0) as f32);
    }

    /// `getPhysicalPeek`: how far the shell has risen, 0..1.
    pub fn physical_peek(&self) -> f32 {
        physical_peek(self.amount)
    }
}

/// `Shulker.getPhysicalPeek`.
pub fn physical_peek(amount: f32) -> f32 {
    0.5 - (std::f32::consts::PI * (0.5 + amount)).sin() * 0.5
}

/// The cell a shulker stands in (`Shulker.setPos` keeps it at the bottom
/// centre of a block).
pub fn cell(pos: DVec3) -> IVec3 {
    IVec3::new(pos.x.floor() as i32, (pos.y + 0.5).floor() as i32, pos.z.floor() as i32)
}

fn centre(cell: IVec3) -> DVec3 {
    DVec3::new(f64::from(cell.x) + 0.5, f64::from(cell.y), f64::from(cell.z) + 0.5)
}

/// `Shulker.canStayAt`: the cell is empty, the block on `face` is sturdy
/// toward it, and the fully opened shell (a block out of the face) has room.
pub fn can_stay_at<W: MobWorld + ?Sized>(w: &W, at: IVec3, face: Dir) -> bool {
    if w.block(at).is_none_or(|b| b != Block::AIR) {
        return false;
    }
    // `loadedAndEntityCanStandOnFace`: the support's face toward the cell is sturdy.
    let support = w.block(at + face.offset());
    if !support.is_some_and(|b| crate::world::mount_states::face_sturdy(b, face.opposite().offset())) {
        return false;
    }
    !w.block(at + face.opposite().offset()).is_some_and(|b| b.is_solid())
}

/// `findAttachableSurface`: the first face, in `Direction` order, it can
/// stay on.
pub fn attachable_face<W: MobWorld + ?Sized>(w: &W, at: IVec3) -> Option<Dir> {
    Dir::ALL.into_iter().find(|&d| can_stay_at(w, at, d))
}

impl Mob {
    /// A shulker's state.
    pub fn shulker(&self) -> Option<&State> {
        self.shulker.as_deref()
    }
}

/// One shulker bullet (`ShulkerBullet`): it homes on its target one axis
/// at a time, and a hit deals 4 and Levitation for 10 seconds.
#[derive(Clone, Debug, PartialEq)]
pub struct Bullet {
    pub pos: DVec3,
    /// Position at the start of the last simulation step, for rendering.
    pub previous_pos: DVec3,
    vel: DVec3,
    /// The shooting shulker's id (never hit by its own bullet).
    owner: u32,
    target: Option<PlayerId>,
    dir: Option<Dir>,
    steps: i32,
    delta: DVec3,
    carry: f64,
    age: u32,
}

/// Seconds a bullet may fly before it is dropped (it has no lifetime in
/// Java, but a target-less bullet falls until it hits something).
const BULLET_MAX_AGE: u32 = 20 * 60;

impl Bullet {
    /// A LAN client's copy: drawn only.
    pub(super) fn replica(pos: DVec3, previous_pos: DVec3) -> Self {
        Self {
            pos,
            previous_pos,
            vel: DVec3::ZERO,
            owner: 0,
            target: None,
            dir: None,
            steps: 0,
            delta: DVec3::ZERO,
            carry: 0.0,
            age: 0,
        }
    }

    /// `ShulkerBullet.selectNextMoveDirection`.
    fn select_next<W: MobWorld + ?Sized>(
        &mut self,
        w: &W,
        avoid: Option<u8>,
        target: Option<DVec3>,
        rng: &mut super::Rng,
    ) {
        let (target_cell, y_offset) = match target {
            None => (self.pos.floor().as_ivec3() - IVec3::Y, 0.5),
            // The player's box is 1.8 high.
            Some(t) => (DVec3::new(t.x, t.y + 0.9, t.z).floor().as_ivec3(), 0.9),
        };
        let mut aim = DVec3::new(
            f64::from(target_cell.x) + 0.5,
            f64::from(target_cell.y) + y_offset,
            f64::from(target_cell.z) + 0.5,
        );
        let mut selection = None;
        let target_centre = target_cell.as_dvec3() + DVec3::splat(0.5);
        if target_centre.distance_squared(self.pos) >= 4.0 {
            let current = self.pos.floor().as_ivec3();
            let empty = |p: IVec3| w.block(p) == Some(Block::AIR);
            let mut options = Vec::new();
            if avoid != Some(0) {
                if current.x < target_cell.x && empty(current + IVec3::X) {
                    options.push(Dir::East);
                } else if current.x > target_cell.x && empty(current - IVec3::X) {
                    options.push(Dir::West);
                }
            }
            if avoid != Some(1) {
                if current.y < target_cell.y && empty(current + IVec3::Y) {
                    options.push(Dir::Up);
                } else if current.y > target_cell.y && empty(current - IVec3::Y) {
                    options.push(Dir::Down);
                }
            }
            if avoid != Some(2) {
                if current.z < target_cell.z && empty(current + IVec3::Z) {
                    options.push(Dir::South);
                } else if current.z > target_cell.z && empty(current - IVec3::Z) {
                    options.push(Dir::North);
                }
            }
            let mut pick = Dir::ALL[rng.next_int(6) as usize];
            if options.is_empty() {
                let mut attempts = 5;
                while !empty(current + pick.offset()) && attempts > 0 {
                    pick = Dir::ALL[rng.next_int(6) as usize];
                    attempts -= 1;
                }
            } else {
                pick = options[rng.next_int(options.len() as u32) as usize];
            }
            aim = self.pos + pick.offset().as_dvec3();
            selection = Some(pick);
        }
        self.dir = selection;
        let d = aim - self.pos;
        let len = d.length();
        self.delta = if len == 0.0 { DVec3::ZERO } else { d / len * BULLET_SPEED };
        self.steps = 10 + rng.next_int(5) as i32 * 10;
    }

    /// `new ShulkerBullet(level, owner, target, invalidStartAxis)`: from the
    /// shulker's box centre, first heading up, then turning toward the
    /// target off the shulker's own axis.
    pub fn new<W: MobWorld + ?Sized>(
        w: &W,
        from: DVec3,
        owner: u32,
        target: PlayerId,
        target_pos: DVec3,
        avoid: u8,
        rng: &mut super::Rng,
    ) -> Self {
        let mut b = Self {
            pos: from,
            previous_pos: from,
            vel: DVec3::ZERO,
            owner,
            target: Some(target),
            dir: Some(Dir::Up),
            steps: 0,
            delta: DVec3::ZERO,
            carry: 0.0,
            age: 0,
        };
        b.select_next(w, Some(avoid), Some(target_pos), rng);
        b
    }
}

fn axis_of(d: Dir) -> u8 {
    match d {
        Dir::West | Dir::East => 0,
        Dir::Down | Dir::Up => 1,
        _ => 2,
    }
}

/// What a bullet ran into this tick.
enum BulletHit {
    Player(PlayerId),
    Mob(usize),
    Block,
    /// v1.0 gap Track 4c: an End crystal or a dragon part.
    Boss(super::dragon::Hit),
}

impl Entities {
    /// Per tick: every loaded shulker's `tick` and goals, then the bullets.
    pub(super) fn shulker_upkeep<W: MobWorld + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        ctx: &Ctx,
        events: &mut Vec<EntityEvent>,
    ) {
        if self.shulker_bullets.is_empty() && !self.mobs.iter().any(|m| m.shulker.is_some()) {
            return;
        }
        self.ensure_mob_ids();
        let mut i = 0;
        while i < self.mobs.len() {
            if self.mobs[i].shulker.is_some() && self.mobs[i].alive() && self.mobs[i].simulation_active {
                let s = self.mobs[i].shulker.as_mut().unwrap();
                s.carry += dt.max(0.0);
                let ticks = ((s.carry + 1e-9) / TICK).floor();
                s.carry -= ticks * TICK;
                for _ in 0..ticks as u32 {
                    self.shulker_tick(i, world, ctx, events);
                }
            }
            i += 1;
        }
        self.bullet_upkeep(dt, world, ctx, events);
    }

    /// One game tick of shulker `i`.
    fn shulker_tick<W: MobWorld + ?Sized>(&mut self, i: usize, world: &W, ctx: &Ctx, events: &mut Vec<EntityEvent>) {
        let at = cell(self.mobs[i].pos);
        {
            let m = &mut self.mobs[i];
            m.pos = centre(at);
            m.previous_pos = m.pos;
            m.vel = DVec3::ZERO;
            m.on_ground = true;
        }
        // `tick`: a shulker that can't stay finds a new face or teleports.
        let attach = self.mobs[i].shulker.as_ref().unwrap().attach;
        if self.mobs[i].riding.is_none() && !can_stay_at(world, at, attach) {
            match attachable_face(world, at) {
                Some(face) => self.mobs[i].shulker.as_mut().unwrap().attach = face,
                None => {
                    self.shulker_teleport(i, world, events);
                }
            }
        }
        // `hurtServer`'s reactions to the last accepted hit.
        let (hurt, bullet) = {
            let s = self.mobs[i].shulker.as_mut().unwrap();
            (std::mem::take(&mut s.hurt), std::mem::take(&mut s.bullet_hit))
        };
        if hurt {
            if self.mobs[i].health < MAX_HEALTH * 0.5 && self.rng.next_int(4) == 0 {
                self.shulker_teleport(i, world, events);
            } else if bullet {
                self.hit_by_shulker_bullet(i, world, events);
            }
        }
        self.shulker_goals(i, world, ctx, events);
        // `updatePeekAmount`.
        let s = self.mobs[i].shulker.as_mut().unwrap();
        s.amount_before = s.amount;
        let want = f32::from(s.peek) * 0.01;
        if s.amount > want {
            s.amount = (s.amount - PEEK_PER_TICK).clamp(want, 1.0);
        } else if s.amount < want {
            s.amount = (s.amount + PEEK_PER_TICK).clamp(0.0, want);
        }
    }

    /// The target selector and goal selector of shulker `i`: nearest
    /// player, attack (shoot), peek.
    fn shulker_goals<W: MobWorld + ?Sized>(&mut self, i: usize, world: &W, ctx: &Ctx, events: &mut Vec<EntityEvent>) {
        let peaceful = self.difficulty == Difficulty::Peaceful;
        let pos = self.mobs[i].pos;
        let attach = self.mobs[i].shulker.as_ref().unwrap().attach;
        // `TargetGoal.canContinueToUse`: a target that died, left the
        // follow range or stopped being attackable is dropped.
        let target = self.mobs[i].shulker.as_ref().unwrap().target;
        let target_pos = target.and_then(|id| {
            ctx.players
                .iter()
                .find(|t| t.id == id && t.targetable && t.alive && !t.spectator)
                .map(|t| t.pos)
                .filter(|p| p.distance_squared(pos) <= FOLLOW_RANGE * FOLLOW_RANGE)
        });
        if target.is_some() && (target_pos.is_none() || peaceful) {
            self.mobs[i].shulker.as_mut().unwrap().target = None;
        }
        // `ShulkerNearestAttackGoal`: about once in ten ticks, the nearest
        // attackable player it can see in its 16-block box, only 4 deep
        // along its attach axis.
        if self.mobs[i].shulker.as_ref().unwrap().target.is_none() && !peaceful && self.rng.next_int(10) == 0 {
            let reach = |d: Dir, axis: u8| if axis_of(d) == axis { 4.0 } else { FOLLOW_RANGE };
            let eye = pos + DVec3::Y * 0.5;
            let seen = ctx
                .players
                .iter()
                .filter(|t| t.targetable && t.alive && !t.spectator)
                .filter(|t| {
                    let d = t.pos - pos;
                    d.x.abs() <= reach(attach, 0) + 0.5
                        && d.y.abs() <= reach(attach, 1) + 1.0
                        && d.z.abs() <= reach(attach, 2) + 0.5
                })
                .filter(|t| super::mob::line_of_sight(world, eye, t.pos + DVec3::Y * 1.62))
                .min_by(|a, b| a.pos.distance_squared(pos).total_cmp(&b.pos.distance_squared(pos)))
                .map(|t| t.id);
            self.mobs[i].shulker.as_mut().unwrap().target = seen;
        }
        let target = self.mobs[i].shulker.as_ref().unwrap().target;
        let target_pos = target.and_then(|id| ctx.players.iter().find(|t| t.id == id).map(|t| t.pos));
        // `ShulkerAttackGoal`.
        let attacking = self.mobs[i].shulker.as_ref().unwrap().attacking;
        match (target.zip(target_pos), attacking) {
            (Some(_), false) if !peaceful => {
                let s = self.mobs[i].shulker.as_mut().unwrap();
                s.attacking = true;
                s.peeking = false;
                s.attack_time = 20;
                self.set_peek(i, 100, events);
            }
            (None, true) => {
                self.mobs[i].shulker.as_mut().unwrap().attacking = false;
                self.set_peek(i, 0, events);
            }
            _ => {}
        }
        if let Some((id, tp)) = target.zip(target_pos)
            && self.mobs[i].shulker.as_ref().unwrap().attacking
        {
            // `getLookControl().setLookAt(target)`.
            let d = tp + DVec3::Y * 1.62 - (pos + DVec3::Y * 0.5);
            let m = &mut self.mobs[i];
            m.head_yaw = (d.z.atan2(d.x) as f32) - m.yaw;
            m.head_pitch = (d.y.atan2(d.x.hypot(d.z))) as f32;
            let s = m.shulker.as_mut().unwrap();
            s.attack_time -= 1;
            if tp.distance_squared(pos) < 400.0 {
                if s.attack_time <= 0 {
                    s.attack_time = 20 + self.rng.next_int(10) as i32 * 20 / 2;
                    let from = self.shulker_box_centre(i);
                    let owner = self.mobs[i].uid;
                    let bullet = Bullet::new(world, from, owner, id, tp, axis_of(attach), &mut self.rng);
                    self.shulker_bullets.push(bullet);
                    self.sculk.shot(from, crate::world::vibration::EventSource::mob(owner));
                    events.push(EntityEvent::Sound { sound: MobSound::Shulker(Cue::Shoot), pos: from });
                }
            } else {
                s.target = None;
            }
        }
        // `ShulkerPeekGoal`: with no target, now and then it peeks for one
        // to three seconds (goals run every other tick, so its one-in-20
        // chance comes about once in 40 ticks).
        if self.mobs[i].shulker.as_ref().unwrap().target.is_none() {
            let peeking = self.mobs[i].shulker.as_ref().unwrap().peeking;
            if peeking {
                let s = self.mobs[i].shulker.as_mut().unwrap();
                s.peek_time -= 1;
                if s.peek_time <= 0 {
                    s.peeking = false;
                    self.set_peek(i, 0, events);
                }
            } else if self.rng.next_int(40) == 0 && can_stay_at(world, cell(pos), attach) {
                let time = 20 * (1 + self.rng.next_int(3) as i32);
                let s = self.mobs[i].shulker.as_mut().unwrap();
                s.peeking = true;
                s.peek_time = time;
                self.set_peek(i, 30, events);
            }
        }
    }

    /// `setRawPeekAmount`: the armor modifier and the open or close sound.
    fn set_peek(&mut self, i: usize, amount: u8, events: &mut Vec<EntityEvent>) {
        let m = &mut self.mobs[i];
        let s = m.shulker.as_mut().unwrap();
        if s.peek == amount {
            return;
        }
        s.peek = amount;
        let cue = if amount == 0 { Cue::Close } else { Cue::Open };
        events.push(EntityEvent::Sound { sound: MobSound::Shulker(cue), pos: m.pos + DVec3::Y * 0.5 });
        // v1.0 Track 9: `Shulker.setRawPeekAmount` posts `container_close`
        // when it shuts, otherwise `container_open`.
        let event = if amount == 0 {
            crate::world::vibration::GameEvent::ContainerClose
        } else {
            crate::world::vibration::GameEvent::ContainerOpen
        };
        self.post_mob_event(i, event);
    }

    /// The centre of shulker `i`'s box (where bullets start).
    fn shulker_box_centre(&self, i: usize) -> DVec3 {
        let m = &self.mobs[i];
        let s = m.shulker.as_ref().unwrap();
        let out = s.attach.opposite().offset().as_dvec3();
        m.pos + DVec3::Y * 0.5 + out * f64::from(s.physical_peek()) * 0.5
    }

    /// `teleportSomewhere`: five tries at a random empty cell within 8 on
    /// each axis with a face to cling to (not bedrock,
    /// `#shulker_does_not_teleport_to`). Returns whether it moved.
    fn shulker_teleport<W: MobWorld + ?Sized>(&mut self, i: usize, world: &W, events: &mut Vec<EntityEvent>) -> bool {
        if !self.mobs[i].alive() {
            return false;
        }
        let old = cell(self.mobs[i].pos);
        let span = (MAX_TELEPORT_DISTANCE * 2 + 1) as u32;
        for _ in 0..5 {
            let mut r = || self.rng.next_int(span) as i32 - MAX_TELEPORT_DISTANCE;
            let target = old + IVec3::new(r(), r(), r());
            // `getMinY`, the empty cell and the world border (the loaded area).
            if target.y <= crate::world::chunk::WORLD_MIN_Y
                || world.block(target) != Some(Block::AIR)
                || !world.loaded(target)
            {
                continue;
            }
            let Some(face) = attachable_face(world, target) else { continue };
            if world.block(target + face.offset()) == Some(Block::BEDROCK) {
                continue;
            }
            let from = self.mobs[i].pos;
            // `Shulker.teleportSomewhere`: `teleport` where it left.
            let source = crate::world::vibration::EventSource::mob(self.mobs[i].uid);
            self.sculk.post(from, crate::world::vibration::GameEvent::Teleport, source, None);
            let m = &mut self.mobs[i];
            m.riding = None;
            m.pos = centre(target);
            m.previous_pos = m.pos;
            let s = m.shulker.as_mut().unwrap();
            s.attach = face;
            s.peek = 0;
            s.target = None;
            s.attacking = false;
            s.peeking = false;
            events.push(EntityEvent::Sound { sound: MobSound::Shulker(Cue::Teleport), pos: from });
            return true;
        }
        false
    }

    /// `hitByShulkerBullet`: an open shulker hit by a bullet teleports, and
    /// unless crowded (a 1 in 5 more chance to fail per other shulker
    /// within 8 of where it was) leaves a new shulker of its colour behind.
    fn hit_by_shulker_bullet<W: MobWorld + ?Sized>(&mut self, i: usize, world: &W, events: &mut Vec<EntityEvent>) {
        let (old, closed) = (self.mobs[i].pos, self.mobs[i].shulker.as_ref().unwrap().closed());
        if closed || !self.shulker_teleport(i, world, events) {
            return;
        }
        let r = OTHER_SHULKER_SCAN_RADIUS + 0.5;
        let count = self
            .mobs
            .iter()
            .filter(|m| m.kind == MobKind::Shulker && m.alive())
            .filter(|m| (m.pos - old).abs().cmple(DVec3::new(r, r + 0.5, r)).all())
            .count();
        let failure = (count as f32 - 1.0) / OTHER_SHULKER_LIMIT;
        if self.rng.next_f32() < failure {
            return;
        }
        let color = self.mobs[i].shulker.as_ref().unwrap().color;
        let mut baby = Mob::new(MobKind::Shulker, old, 0.0);
        baby.persistent = true;
        baby.shulker.as_mut().unwrap().color = color;
        self.mobs.push(baby);
    }

    /// Bullets fly whole Java ticks.
    fn bullet_upkeep<W: MobWorld + ?Sized>(&mut self, dt: f64, world: &W, ctx: &Ctx, events: &mut Vec<EntityEvent>) {
        let mut i = 0;
        while i < self.shulker_bullets.len() {
            let b = &mut self.shulker_bullets[i];
            b.previous_pos = b.pos;
            b.carry += dt.max(0.0);
            let ticks = ((b.carry + 1e-9) / TICK).floor();
            b.carry -= ticks * TICK;
            let mut alive = true;
            for _ in 0..ticks as u32 {
                if !self.bullet_tick(i, world, ctx, events) {
                    alive = false;
                    break;
                }
            }
            if alive {
                i += 1;
            } else {
                let b = self.shulker_bullets.swap_remove(i);
                let source = super::sculk_events::Sculk::projectile_source(None, Some(b.owner));
                self.sculk.land(world, b.pos, b.pos - b.previous_pos, source);
                // v1.0 Track 9: `ShulkerBullet.destroy` posts `entity_damage`
                // where it is, from the bullet (named here by its owner).
                self.sculk.post(b.pos, crate::world::vibration::GameEvent::EntityDamage, source, None);
            }
        }
    }

    /// One `ShulkerBullet.tick`; `false` once it hit something.
    fn bullet_tick<W: MobWorld + ?Sized>(
        &mut self,
        i: usize,
        world: &W,
        ctx: &Ctx,
        events: &mut Vec<EntityEvent>,
    ) -> bool {
        if self.difficulty == Difficulty::Peaceful {
            return false;
        }
        let b = self.shulker_bullets[i].clone();
        let target = b.target.and_then(|id| ctx.players.iter().find(|t| t.id == id && t.alive && !t.spectator));
        let mut b = b;
        b.age += 1;
        if b.age > BULLET_MAX_AGE || !world.loaded(b.pos.floor().as_ivec3()) {
            return false;
        }
        match target {
            None => {
                b.target = None;
                b.vel.y -= BULLET_GRAVITY;
            }
            Some(_) => {
                b.delta = b.delta.map(|v| (v * 1.025).clamp(-1.0, 1.0));
                b.vel += (b.delta - b.vel) * 0.2;
            }
        }
        // `ProjectileUtil.getHitResultOnMoveVector`: blocks and entities on
        // the way, in small steps.
        let steps = ((b.vel.length() / 0.1).ceil() as u32).max(1);
        let mut hit = None;
        for k in 1..=steps {
            let p = b.pos + b.vel * (f64::from(k) / f64::from(steps));
            if world.block(p.floor().as_ivec3()).is_some_and(|bl| bl.is_solid()) {
                hit = Some(BulletHit::Block);
                break;
            }
            if let Some(t) = ctx.players.iter().find(|t| t.alive && !t.spectator && t.contains(p)) {
                hit = Some(BulletHit::Player(t.id));
                break;
            }
            if let Some(j) = self.mobs.iter().position(|m| {
                m.alive() && m.uid != b.owner && {
                    let (lo, hi) = m.aabb();
                    p.cmpge(lo - DVec3::splat(0.15)).all() && p.cmple(hi + DVec3::splat(0.15)).all()
                }
            }) {
                hit = Some(BulletHit::Mob(j));
                break;
            }
            if let Some(h) = super::dragon::hit_at(self.fight.as_ref(), &self.wild_dragons, &self.crystals, p) {
                hit = Some(BulletHit::Boss(h));
                break;
            }
        }
        if hit.is_none() {
            b.pos += b.vel;
        }
        self.particles.push(crate::particles::Request::Burst(crate::particles::Burst::new(
            crate::particles::Kind::EndRod,
            b.pos - b.vel + DVec3::Y * 0.15,
            1,
        )));
        if let Some(hit) = hit {
            self.bullet_hit(&b, hit, events);
            return false;
        }
        if let Some(t) = target {
            if b.steps > 0 {
                b.steps -= 1;
                if b.steps == 0 {
                    let avoid = b.dir.map(axis_of);
                    b.select_next(world, avoid, Some(t.pos), &mut self.rng);
                }
            }
            if let Some(d) = b.dir {
                let current = b.pos.floor().as_ivec3();
                let axis = axis_of(d);
                let goal = t.pos.floor().as_ivec3();
                let blocked = world.block(current + d.offset()).is_some_and(|bl| bl.is_solid());
                let aligned = match axis {
                    0 => current.x == goal.x,
                    1 => current.y == goal.y,
                    _ => current.z == goal.z,
                };
                if blocked || aligned {
                    b.select_next(world, Some(axis), Some(t.pos), &mut self.rng);
                }
            }
        }
        self.shulker_bullets[i] = b;
        true
    }

    /// `onHitEntity` and `onHitBlock`.
    fn bullet_hit(&mut self, b: &Bullet, hit: BulletHit, events: &mut Vec<EntityEvent>) {
        match hit {
            BulletHit::Player(player) => {
                let push = DVec3::new(b.vel.x, 0.0, b.vel.z).normalize_or_zero() * 2.0 + DVec3::Y * 2.0;
                events.push(EntityEvent::ArrowEffectsStart { player });
                events.push(EntityEvent::PlayerHit {
                    java: None,
                    player,
                    damage: BULLET_DAMAGE,
                    knockback: push.as_vec3(),
                    cause: BULLET_CAUSE,
                });
                events.push(EntityEvent::PlayerEffect {
                    player,
                    effect: crate::simulation::effects::Effect::Levitation,
                    amplifier: 0,
                    ticks: LEVITATION_TICKS,
                });
                events.push(EntityEvent::ArrowEffectsEnd);
            }
            BulletHit::Mob(j) => {
                let m = &mut self.mobs[j];
                m.hit_cause = Some(BULLET_CAUSE);
                let before = m.health;
                m.damage_pushed(BULLET_DAMAGE, &[], &mut self.rng);
                if m.health < before {
                    m.effects.add(crate::simulation::effects::Effect::Levitation, 0, LEVITATION_TICKS);
                    if let Some(s) = &mut m.shulker {
                        s.bullet_hit = true;
                    }
                }
            }
            // `mobProjectile(this, shulker)` for 4: it breaks a crystal, and
            // lands on a dragon without hurting it (a shulker isn't a
            // player); a part isn't living, so nothing levitates.
            BulletHit::Boss(struck) => {
                if !struck.is_dragon() {
                    // v1.0 gap Track 8b: the crystal's blast names the shulker.
                    let shulker = self.mobs.iter().any(|m| m.uid == b.owner && m.alive()).then_some(MobKind::Shulker);
                    let blow = super::dragon::Blow::projectile(BULLET_DAMAGE, false).blamed_on(shulker);
                    events.push(EntityEvent::DragonStruck { hit: struck, blow, by: None });
                }
            }
            BulletHit::Block => {
                let mut burst = crate::particles::Burst::new(crate::particles::Kind::Explosion, b.pos, 2);
                burst.spread = DVec3::splat(0.2);
                self.particles.push(crate::particles::Request::Burst(burst));
                events.push(EntityEvent::Sound { sound: MobSound::Shulker(Cue::BulletHit), pos: b.pos });
            }
        }
    }

    /// Spawns the shulkers of End city `Sentry` markers near players, once
    /// each (like the other structures' residents), when their cells are
    /// loaded. Java spawns them while the chunk generates; VoxelCraft's
    /// structures populate when a player first comes near.
    pub(super) fn populate_end_cities<W: MobWorld + ?Sized>(&mut self, dt: f32, w: &W, ctx: &Ctx) {
        if ctx.dimension != crate::world::terrain::Dimension::End {
            return;
        }
        self.end_city_timer -= dt;
        if self.end_city_timer > 0.0 {
            return;
        }
        self.end_city_timer = 1.0;
        let mut spawn = Vec::new();
        for t in &ctx.players {
            for p in w.end_city_sentries(t.pos.floor().as_ivec3(), 96) {
                if !w.loaded(p) || self.structures_populated.contains(&p) {
                    continue;
                }
                spawn.push(p);
            }
        }
        for p in spawn {
            if self.structures_populated.insert(p) {
                // `EntityTypes.SHULKER.create(level, STRUCTURE)`: its tick
                // finds a face to cling to.
                let mut m = Mob::new(MobKind::Shulker, centre(p), 0.0);
                m.persistent = true;
                self.mobs.push(m);
            }
        }
        // The ship's `Elytra` marker: an item frame holding an elytra
        // (v0.16 Track B's item), hung once.
        let mut frames = Vec::new();
        for t in &ctx.players {
            for (cell, facing) in w.end_city_elytra_frames(t.pos.floor().as_ivec3(), 96) {
                if w.loaded(cell) && !self.structures_populated.contains(&cell) && !frames.contains(&(cell, facing)) {
                    frames.push((cell, facing));
                }
            }
        }
        for (cell, facing) in frames {
            if self.structures_populated.insert(cell) {
                let elytra = crate::inventory::Stack::new(crate::content::elytra::ELYTRA, 1);
                self.decor.push(super::decor::Decor::structure_frame(cell, facing, elytra));
            }
        }
    }

    /// `Shulker`'s `HurtByTargetGoal().setAlertOthers()`: the shulker a
    /// player hurt targets them, and so does every shulker without a target
    /// within its 16-block follow range (10 up and down).
    pub(super) fn shulker_hurt_by(&mut self, index: usize, player: PlayerId) {
        let Some(pos) = self.mobs.get(index).filter(|m| m.alive()).map(|m| m.pos) else { return };
        for (j, m) in self.mobs.iter_mut().enumerate() {
            let d = m.pos - pos;
            let near = d.x.abs() <= FOLLOW_RANGE && d.z.abs() <= FOLLOW_RANGE && d.y.abs() <= 10.0;
            let alive = m.alive();
            if let Some(s) = &mut m.shulker
                && alive
                && (j == index || near && s.target.is_none())
            {
                s.target = Some(player);
            }
        }
    }
}

#[cfg(test)]
mod tests;
