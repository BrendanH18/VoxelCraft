//! Java 26.3 `ThrownTrident`, an `AbstractArrow`, ticked at Java's 20 Hz
//! with its numbers: 2.5 blocks a tick from a player (1.6 from a drowned),
//! 0.05 gravity, 0.99 drag in air and water, 8 damage plus Impaling, a
//! reversed bounce off whatever it hits, sticking in blocks, Loyalty's
//! return, Channeling's lightning, breaking pointed dripstone faster than
//! 0.6 blocks a tick, pickup rules and the 1200-tick despawn. Tridents are
//! saved with the dimension's entities (`tridents`), like Java.

use glam::{DVec3, IVec3, Vec3};
use serde_json::{Value, json};

use super::{EntityEvent, Mob, MobKind, MobSound, MobWorld, PlayerId, Rng, Target};
use crate::inventory::Stack;
use crate::weapons::{Cue, trident as tr};
use crate::world::block::Block;

/// Who threw it (`Projectile.owner`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    Player(PlayerId),
    /// A mob, by [`Mob::uid`].
    Mob(u32),
}

/// `AbstractArrow.Pickup`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pickup {
    Disallowed,
    Allowed,
    CreativeOnly,
}

impl Pickup {
    fn id(self) -> u8 {
        self as u8
    }
    fn from_id(id: u64) -> Self {
        match id {
            1 => Self::Allowed,
            2 => Self::CreativeOnly,
            _ => Self::Disallowed,
        }
    }
}

/// The half-width and height of the trident's box (`EntityTypes.TRIDENT`, 0.5 × 0.5).
const HALF: f64 = 0.25;
const HEIGHT: f64 = 0.5;
/// `ProjectileUtil.getEntityHitResult`'s default pick margin.
const PICK_MARGIN: f64 = 0.3;

#[derive(Clone, Debug)]
pub struct Trident {
    /// `pickupItemStack`: the thrown trident with its wear and enchantments.
    pub stack: Stack,
    pub pos: DVec3,
    pub previous_pos: DVec3,
    /// `deltaMovement`, blocks per tick.
    pub vel: DVec3,
    pub owner: Option<Owner>,
    pub pickup: Pickup,
    pub in_ground: bool,
    in_ground_time: u32,
    life: u32,
    pub shake: u8,
    dealt_damage: bool,
    /// Returning with Loyalty: flies through blocks toward its owner.
    pub no_physics: bool,
    /// `clientSideReturnTridentTickCount` (the return sound plays once).
    returning: u32,
    /// The breeze that turned it back (Java makes it the owner, so it can't
    /// hit that breeze again); not saved.
    deflector: Option<u32>,
    /// `lastState`: the block it is stuck in.
    last_block: Option<Block>,
    /// Pointing direction for rendering, kept once stuck.
    pub dir: Vec3,
    remainder: f64,
    /// v1.0 gap Track 8b: `remainingFireTicks`. Fire and lava set it alight
    /// (8 and 15 seconds), so does a perched dragon's scales (a second,
    /// `AbstractDragonSittingPhase.onHurt`), and water and rain put it out.
    /// A burning trident lights candles and campfires and primes TNT it
    /// hits; `ThrownTrident.onHitEntity` sets nothing alight. Saved (`Fire`).
    pub fire: u16,
    /// v1.0 gap Track 8b: `yRot` and `xRot`, eased toward the motion each
    /// tick and turned on a bounce, as an arrow's ([`super::projectile`]);
    /// `None` until a tick in flight sets them.
    rot: Option<(f32, f32)>,
}

/// Something a trident's tick asks the entity update to do afterwards.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Effect {
    /// Channeling: summon lightning here.
    Lightning(DVec3),
    /// A trident whose owner is gone drops its item here.
    Drop(Stack, DVec3),
}

impl Trident {
    fn new(stack: Stack, pos: DVec3, vel: DVec3, owner: Option<Owner>, pickup: Pickup) -> Self {
        Self {
            stack: Stack { count: 1, ..stack },
            pos,
            previous_pos: pos,
            vel,
            owner,
            pickup,
            in_ground: false,
            in_ground_time: 0,
            life: 0,
            shake: 0,
            dealt_damage: false,
            no_physics: false,
            returning: 0,
            deflector: None,
            last_block: None,
            dir: vel.normalize_or(DVec3::X).as_vec3(),
            remainder: 0.0,
            fire: 0,
            rot: None,
        }
    }

    /// `TridentItem.releaseUsing`: thrown by a player (see [`Throw`]).
    /// Creative throws can only be picked up in Creative.
    pub fn thrown(stack: Stack, throw: Throw, rng: &mut Rng) -> Self {
        let aim = tr::rotation_vector(throw.y_rot, throw.x_rot);
        let carry = throw.carry;
        let vel = shoot(aim, tr::SHOOT_POWER, 1.0, rng)
            + DVec3::new(carry.x, if throw.on_ground { 0.0 } else { carry.y }, carry.z);
        let pickup = if throw.creative { Pickup::CreativeOnly } else { Pickup::Allowed };
        Self::new(stack, throw.eye - DVec3::Y * f64::from(0.1f32), vel, Some(Owner::Player(throw.owner)), pickup)
    }

    /// `Drowned.performRangedAttack`: aimed at `target` (a third of the way
    /// up its box) from the drowned's eye, aiming 20% of the horizontal
    /// distance high, with the difficulty's uncertainty. Not recoverable.
    pub fn from_drowned(stack: Stack, uid: u32, eye: DVec3, target: DVec3, uncertainty: f32, rng: &mut Rng) -> Self {
        let start = eye - DVec3::Y * f64::from(0.1f32);
        let d = target - start;
        let horizontal = (d.x * d.x + d.z * d.z).sqrt();
        let aim = DVec3::new(d.x, d.y + horizontal * f64::from(0.2f32), d.z);
        let vel = shoot(aim, tr::DROWNED_SHOOT_POWER, uncertainty, rng);
        Self::new(stack, start, vel, Some(Owner::Mob(uid)), Pickup::Disallowed)
    }

    /// Whether the trident shows the enchantment glint (`ID_FOIL`).
    /// The vibration source for this trident's game events (its owner).
    pub fn event_source(&self) -> crate::world::vibration::EventSource {
        let (player, mob) = match self.owner {
            Some(Owner::Player(id)) => (Some(id), None),
            Some(Owner::Mob(uid)) => (None, Some(uid)),
            None => (None, None),
        };
        super::sculk_events::Sculk::projectile_source(player, mob)
    }

    pub fn foil(&self) -> bool {
        !self.stack.enchants.is_empty()
    }

    pub fn loyalty(&self) -> u8 {
        tr::loyalty(self.stack.active_enchants())
    }

    fn box_at(pos: DVec3) -> (DVec3, DVec3) {
        (pos - DVec3::new(HALF, 0.0, HALF), pos + DVec3::new(HALF, HEIGHT, HALF))
    }

    /// Ticks for `dt` seconds of the entity update; `false` once discarded.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn update<W: MobWorld + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        mobs: &mut [Mob],
        players: &[Target],
        pvp: bool,
        thundering: bool,
        boss: &super::dragon::Boxes,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
        effects: &mut Vec<Effect>,
    ) -> bool {
        self.remainder += dt.max(0.0) * 20.0;
        let ticks = (self.remainder + 1e-9) as u32;
        self.remainder = (self.remainder - f64::from(ticks)).max(0.0);
        for _ in 0..ticks {
            if !self.tick(world, mobs, players, pvp, thundering, boss, rng, events, effects) {
                return false;
            }
        }
        self.pos.y > -128.0
    }

    /// Eye, alive and box width of the owner, if it can still be returned to
    /// (`isAcceptibleReturnOwner`): alive and not a spectator.
    fn owner_state(&self, mobs: &[Mob], players: &[Target]) -> Option<(DVec3, bool, f64)> {
        match self.owner? {
            Owner::Player(id) => players
                .iter()
                .find(|t| t.id == id && t.alive && !t.spectator)
                .map(|t| (t.eye(), true, t.shape.half_width * 2.0)),
            Owner::Mob(uid) => mobs.iter().find(|m| m.uid == uid && m.alive()).map(|m| {
                let s = m.shape();
                (m.look_eye(), false, s.half_width * 2.0)
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn tick<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        mobs: &mut [Mob],
        players: &[Target],
        pvp: bool,
        thundering: bool,
        boss: &super::dragon::Boxes,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
        effects: &mut Vec<Effect>,
    ) -> bool {
        // `ThrownTrident.tick`.
        if self.in_ground_time > 4 {
            self.dealt_damage = true;
        }
        let loyalty = self.loyalty();
        if loyalty > 0 && (self.dealt_damage || self.no_physics) && self.owner.is_some() {
            match self.owner_state(mobs, players) {
                None => {
                    if self.pickup == Pickup::Allowed {
                        effects.push(Effect::Drop(self.stack, self.pos + DVec3::Y * 0.1));
                    }
                    return false;
                }
                Some((eye, player, width)) => {
                    if !player && self.pos.distance(eye) < width + 1.0 {
                        return false;
                    }
                    self.no_physics = true;
                    let to = eye - self.pos;
                    self.pos.y += to.y * 0.015 * f64::from(loyalty);
                    self.vel = self.vel * 0.95 + to.normalize_or_zero() * (0.05 * f64::from(loyalty));
                    if self.returning == 0 {
                        events.push(EntityEvent::Sound { sound: MobSound::Weapon(Cue::TridentReturn), pos: self.pos });
                    }
                    self.returning += 1;
                }
            }
        }
        // `AbstractArrow.tick`.
        let physics = !self.no_physics;
        let cell = self.pos.floor().as_ivec3();
        let here = world.block(cell);
        if physics && here.is_some_and(|b| b != Block::AIR) && inside_collision(world, cell, self.pos) {
            self.vel = DVec3::ZERO;
            self.in_ground = true;
        }
        self.shake = self.shake.saturating_sub(1);
        // v1.0 gap Track 8b: `isInWaterOrRain` puts the fire out.
        if super::projectile::in_water_or_rain(world, self.pos, HALF, HEIGHT) {
            self.fire = 0;
        }
        if self.in_ground && physics {
            // `applyEffectsFromBlocks` in the ground.
            self.touch_blocks(world, self.pos, self.pos);
            if self.last_block != here && should_fall(world, self.pos) {
                self.in_ground = false;
                self.vel *= DVec3::new(
                    f64::from(rng.next_f32() * 0.2),
                    f64::from(rng.next_f32() * 0.2),
                    f64::from(rng.next_f32() * 0.2),
                );
                self.life = 0;
            } else if (self.pickup != Pickup::Allowed || loyalty == 0) && {
                self.life += 1;
                self.life >= tr::DESPAWN_TICKS
            } {
                return false;
            }
            self.in_ground_time += 1;
            return true;
        }
        self.in_ground_time = 0;
        let in_water = here.is_some_and(|b| b.holds_water());
        if in_water {
            self.vel *= tr::WATER_INERTIA;
        }
        // v1.0 gap Track 8b: the look eases a fifth of the way toward the
        // motion (`lerpRotation`; a returning trident's yaw looks back
        // along it, its pitch doesn't).
        if self.vel.length_squared() > 1e-12 {
            let (mut yaw, pitch) = super::projectile::rotation_of(self.vel);
            if !physics {
                yaw = super::projectile::rotation_of(-self.vel).0;
            }
            let (y0, p0) = self.rot.unwrap_or((yaw, pitch));
            let rot = (super::projectile::lerp_rotation(y0, yaw), super::projectile::lerp_rotation(p0, pitch));
            self.rot = Some(rot);
            self.dir = super::projectile::pointing(rot);
        }
        if physics {
            if !self.step_move_and_hit(world, mobs, players, pvp, thundering, boss, rng, events, effects) {
                return false;
            }
        } else {
            self.pos += self.vel;
        }
        if !in_water {
            self.vel *= tr::AIR_DRAG;
        }
        if physics && !self.in_ground {
            self.vel.y -= tr::GRAVITY;
        }
        // `Entity.baseTick` (in flight only) burns the fire down.
        self.fire = self.fire.saturating_sub(1);
        true
    }

    /// v1.0 gap Track 8b: `applyEffectsFromBlocks` for the box moving
    /// `from` to `to` (see [`super::projectile::Touched`]), then rain.
    fn touch_blocks<W: MobWorld + ?Sized>(&mut self, world: &W, from: DVec3, to: DVec3) {
        super::projectile::touched_blocks(world, from, to, HALF, HEIGHT).apply(&mut self.fire);
        if super::projectile::in_rain(world, to, HEIGHT) {
            self.fire = 0;
        }
    }

    /// Whether it is on fire (`isOnFire`).
    pub fn burning(&self) -> bool {
        self.fire > 0
    }

    /// `ProjectileDeflection.REVERSE`'s turn: the yaw by 170 to 190°.
    fn turn_back(&mut self, rng: &mut Rng) {
        let (yaw, pitch) = self.rot.unwrap_or_else(|| super::projectile::rotation_of(self.dir.as_dvec3()));
        let rot = (yaw + super::projectile::reverse_turn(rng), pitch);
        self.rot = Some(rot);
        self.dir = super::projectile::pointing(rot);
    }

    #[allow(clippy::too_many_arguments)]
    fn step_move_and_hit<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        mobs: &mut [Mob],
        players: &[Target],
        pvp: bool,
        thundering: bool,
        boss: &super::dragon::Boxes,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
        effects: &mut Vec<Effect>,
    ) -> bool {
        let from = self.pos;
        let block = clip(world, from, self.vel);
        let reach = block.map_or(1.0, |(t, ..)| t);
        let step = self.vel * reach;
        if !self.dealt_damage
            && let Some((target, t)) = self.find_hit(from, step, mobs, players, pvp, boss)
        {
            self.pos = from + step * t;
            self.touch_blocks(world, from, self.pos);
            // `Breeze.deflection`: `ProjectileDeflection.REVERSE` before any damage.
            if let Hit::Mob(i) = target
                && mobs[i].kind == MobKind::Breeze
            {
                self.vel *= -0.5;
                self.turn_back(rng);
                self.deflector = Some(mobs[i].uid);
                events.push(EntityEvent::Sound {
                    sound: MobSound::Breeze(super::breeze::Cue::Deflect),
                    pos: mobs[i].pos,
                });
                return true;
            }
            self.hit_entity(world, target, mobs, thundering, boss, rng, events, effects);
            return true;
        }
        self.pos = from + step;
        self.touch_blocks(world, from, self.pos);
        if let Some((_, cell, _)) = block {
            self.hit_block(world, cell, thundering, events, effects);
        }
        true
    }

    /// The nearest mob or player the move from `from` along `step` crosses
    /// (`AbstractArrow.findHitEntity`, boxes grown by 0.3), and how far along.
    fn find_hit(
        &self,
        from: DVec3,
        step: DVec3,
        mobs: &[Mob],
        players: &[Target],
        pvp: bool,
        boss: &super::dragon::Boxes,
    ) -> Option<(Hit, f64)> {
        let grow = DVec3::splat(PICK_MARGIN);
        let ray = |min: DVec3, max: DVec3| {
            let (min, max) = (min - grow, max + grow);
            if from.cmpge(min).all() && from.cmple(max).all() {
                Some(0.0)
            } else {
                crate::physics::ray_aabb(from, step, min, max).filter(|t| *t <= 1.0)
            }
        };
        let mut best: Option<(Hit, f64)> = None;
        for (i, m) in mobs.iter().enumerate() {
            if !m.alive() || self.owner == Some(Owner::Mob(m.uid)) || self.deflector == Some(m.uid) {
                continue;
            }
            let (min, max) = m.aabb();
            if let Some(t) = ray(min, max)
                && best.is_none_or(|(_, b)| t < b)
            {
                best = Some((Hit::Mob(i), t));
            }
        }
        let player_owned = matches!(self.owner, Some(Owner::Player(_)));
        for p in players {
            if !p.targetable || !p.alive || self.owner == Some(Owner::Player(p.id)) || (player_owned && !pvp) {
                continue;
            }
            let (min, max) = p.shape.aabb(p.pos);
            if let Some(t) = ray(min, max)
                && best.is_none_or(|(_, b)| t < b)
            {
                best = Some((Hit::Player(p.id, p.pos), t));
            }
        }
        // v1.0 gap Track 4c: End crystals and dragon parts.
        for &(hit, min, max) in &boss.list {
            if let Some(t) = ray(min, max)
                && best.is_none_or(|(_, b)| t < b)
            {
                best = Some((Hit::Boss(hit), t));
            }
        }
        best
    }

    #[allow(clippy::too_many_arguments)]
    fn hit_entity<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        target: Hit,
        mobs: &mut [Mob],
        thundering: bool,
        boss: &super::dragon::Boxes,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
        effects: &mut Vec<Effect>,
    ) {
        let enchants = self.stack.active_enchants();
        let channeling = enchants.has(crate::enchant::Enchantment::Channeling) && thundering;
        self.dealt_damage = true;
        let motion = self.vel;
        let owner_player = match self.owner {
            Some(Owner::Player(id)) => Some(id),
            _ => None,
        };
        match target {
            Hit::Mob(i) => {
                let mob = &mut mobs[i];
                let damage = tr::BASE_DAMAGE + crate::weapons::impaling_bonus(enchants, mob.kind);
                // `EnderMan.hurtServer` dodges every projectile, and (v1.0
                // gap Track 8c) `projectileReceivesSideEffectsOnHit(false)`
                // is false for it: no hit sound and no bounce, the trident
                // flies on (having dealt its damage, it hits nothing more).
                if super::projectile::dodges_projectiles(mob) {
                    mob.teleport_pending = true;
                    return;
                }
                let hurt = if super::projectile::refuses_arrow(mob, damage) {
                    false
                } else {
                    let before = mob.health;
                    if let Some(owner) = owner_player {
                        mob.player_hit_by(owner);
                    }
                    let killed = mob.damage_projectile(damage, &[crate::knockback::Push::projectile(motion)], rng);
                    let hurt = killed || mob.health < before;
                    if hurt {
                        events.push(EntityEvent::MobShot {
                            kind: mob.kind,
                            pos: mob.pos,
                            killed: !mob.alive(),
                            burning: mob.burning,
                            player_kill: owner_player.is_some(),
                            owner: owner_player,
                        });
                    }
                    hurt
                };
                if hurt && channeling && world.exposed(mob.pos.floor().as_ivec3()) {
                    effects.push(Effect::Lightning(mob.pos));
                    events.push(EntityEvent::Sound { sound: MobSound::Weapon(Cue::TridentThunder), pos: mob.pos });
                }
            }
            Hit::Player(id, pos) => {
                let damage = tr::BASE_DAMAGE;
                let cause = if owner_player.is_some() {
                    crate::simulation::damage::Cause::PLAYER_TRIDENT
                } else {
                    crate::simulation::damage::Cause::DROWNED_TRIDENT
                };
                events.push(EntityEvent::PlayerHit {
                    player: id,
                    damage,
                    knockback: (DVec3::new(motion.x, 0.0, motion.z).normalize_or_zero() * 8.0 + DVec3::Y * 8.0)
                        .as_vec3(),
                    java: Some(crate::knockback::Hit::arrow(motion, 0)),
                    cause,
                });
                if let Some(Owner::Mob(attacker)) = self.owner {
                    events.push(EntityEvent::OwnerHurt { player: id, attacker });
                }
                if channeling && world.exposed(pos.floor().as_ivec3()) {
                    effects.push(Effect::Lightning(pos));
                    events.push(EntityEvent::Sound { sound: MobSound::Weapon(Cue::TridentThunder), pos });
                }
            }
            // `ThrownTrident.onHitEntity`: `trident(this, owner)` for 8 (no
            // Impaling bonus: a dragon isn't `#sensitive_to_impaling`). It
            // breaks a crystal; a dragon takes it only from a player, and
            // not perched (it's an `AbstractArrow`) or dying. Whatever
            // happens, it bounces back below (`projectileReceivesSideEffectsOnHit`).
            // Channeling never strikes: the End has no thunder.
            Hit::Boss(hit) => {
                let blow = super::dragon::Blow::arrow(tr::BASE_DAMAGE, owner_player.is_some());
                // v1.0 gap Track 8b: a perched dragon's scales set it alight
                // for a second (`igniteForSeconds(1)`: it's an `AbstractArrow`).
                if boss.scorches(hit, blow) {
                    self.fire = self.fire.max(20);
                }
                let blow = match self.owner {
                    Some(Owner::Mob(uid)) => blow.blamed_on(mobs.iter().find(|m| m.uid == uid).map(|m| m.kind)),
                    _ => blow,
                };
                events.push(EntityEvent::DragonStruck { hit, blow, by: owner_player });
            }
        }
        // `projectileReceivesSideEffectsOnHit`: the hit sound and a reversed
        // bounce (`deflect(REVERSE, .., (0.02, 0.2, 0.02))`), turned about.
        events.push(EntityEvent::Sound { sound: MobSound::Weapon(Cue::TridentHit), pos: self.pos });
        self.vel *= DVec3::new(-0.01, -0.1, -0.01);
        self.turn_back(rng);
    }

    /// `AbstractArrow.onHitBlock` after `BlockState.onProjectileHit`.
    fn hit_block<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        cell: IVec3,
        thundering: bool,
        events: &mut Vec<EntityEvent>,
        effects: &mut Vec<Effect>,
    ) {
        let block = world.block(cell);
        // `SpeleothemBlock.onProjectileHit`: a trident faster than 0.6 breaks
        // pointed dripstone and (v0.16 Track H) sulfur spikes.
        if self.vel.length() > 0.6 && block.is_some_and(|b| crate::world::speleothem::of(b).is_some()) {
            let mob_owned = matches!(self.owner, Some(Owner::Mob(_)));
            events.push(EntityEvent::TridentBrokeDripstone { cell, mob_owned });
        }
        // Channeling's `hit_block`: lightning on a sky-lit lightning rod.
        if thundering
            && self.stack.active_enchants().has(crate::enchant::Enchantment::Channeling)
            && block.is_some_and(|b| crate::world::lightning::rod(b).is_some())
            && world.exposed(cell + IVec3::Y)
        {
            let at = cell.as_dvec3() + DVec3::new(0.5, 1.0, 0.5);
            effects.push(Effect::Lightning(at));
            events.push(EntityEvent::Sound { sound: MobSound::Weapon(Cue::TridentThunder), pos: at });
        }
        // v1.0 gap Track 8b: a burning trident's `onProjectileHit` lights a
        // candle or campfire, or primes TNT (`isOnFire`).
        if self.burning() && block.is_some_and(super::projectile::lights) {
            events.push(EntityEvent::ProjectileBlockHit {
                cell,
                pos: self.pos,
                normal: IVec3::ZERO,
                arrow: true,
                burning: true,
                mob_owned: matches!(self.owner, Some(Owner::Mob(_))),
            });
        }
        self.last_block = block;
        let sign = DVec3::new(self.vel.x.signum(), self.vel.y.signum(), self.vel.z.signum());
        let sign = DVec3::select(self.vel.cmpeq(DVec3::ZERO), DVec3::ZERO, sign);
        self.pos -= sign * f64::from(0.05f32);
        self.vel = DVec3::ZERO;
        events.push(EntityEvent::Sound { sound: MobSound::Weapon(Cue::TridentHitGround), pos: self.pos });
        self.in_ground = true;
        self.shake = tr::SHAKE_TICKS;
    }

    /// `AbstractArrow.playerTouch` for player `id` at `feet` with box `shape`:
    /// whether the trident can be taken now, and the item it gives (a
    /// Creative-only pickup gives nothing).
    pub fn touch(
        &self,
        id: PlayerId,
        feet: DVec3,
        shape: crate::physics::Shape,
        creative: bool,
    ) -> Option<Option<Stack>> {
        let (pmin, pmax) = shape.aabb(feet);
        let (pmin, pmax) = (pmin - DVec3::new(1.0, 0.5, 1.0), pmax + DVec3::new(1.0, 0.5, 1.0));
        let (min, max) = Self::box_at(self.pos);
        let touching = min.cmplt(pmax).all() && max.cmpgt(pmin).all();
        let owner = match self.owner {
            Some(Owner::Player(owner)) => Some(owner),
            _ => None,
        };
        // `ThrownTrident.playerTouch`: only its owner, or anyone once ownerless.
        let owned = owner == Some(id);
        let ownerless = self.owner.is_none();
        if !touching || !(owned || ownerless) || !(self.in_ground || self.no_physics) || self.shake > 0 {
            return None;
        }
        match self.pickup {
            Pickup::Allowed => Some(Some(self.stack)),
            Pickup::CreativeOnly if creative => Some(None),
            // `ThrownTrident.tryPickup`: a returning trident always goes back to its owner.
            _ if self.no_physics && owned => Some(Some(self.stack)),
            _ => None,
        }
    }

    pub fn save(&self) -> Value {
        let owner = match self.owner {
            Some(Owner::Player(id)) => json!({"player": id.0}),
            Some(Owner::Mob(uid)) => json!({"mob": uid}),
            None => Value::Null,
        };
        json!({
            "item": crate::inventory::stack_to_string(Some(self.stack)),
            "pos": self.pos.to_array(),
            "motion": self.vel.to_array(),
            "owner": owner,
            "pickup": self.pickup.id(),
            "in_ground": self.in_ground,
            "in_ground_time": self.in_ground_time,
            "life": self.life,
            "shake": self.shake,
            "dealt_damage": self.dealt_damage,
            "no_physics": self.no_physics,
            "returning": self.returning,
            "in_block": self.last_block.map(|b| b.0),
            "dir": self.dir.to_array(),
            "fire": self.fire,
        })
    }

    pub fn load(v: &Value) -> Option<Self> {
        let stack = crate::inventory::stack_from_str(v["item"].as_str()?).flatten()?;
        if stack.item != crate::weapons::TRIDENT {
            return None;
        }
        let vec = |key: &str| -> Option<DVec3> {
            let a = v[key].as_array()?;
            let p = DVec3::new(a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?);
            p.is_finite().then_some(p)
        };
        let pos = vec("pos")?;
        let owner = if let Some(id) = v["owner"]["player"].as_u64() {
            Some(Owner::Player(PlayerId(u32::try_from(id).ok()?)))
        } else {
            v["owner"]["mob"].as_u64().and_then(|u| u32::try_from(u).ok()).map(Owner::Mob)
        };
        let mut t = Self::new(
            stack,
            pos,
            vec("motion").unwrap_or(DVec3::ZERO),
            owner,
            Pickup::from_id(v["pickup"].as_u64().unwrap_or(0)),
        );
        let small = |key: &str, max: u64| v[key].as_u64().unwrap_or(0).min(max);
        t.in_ground = v["in_ground"].as_bool().unwrap_or(false);
        t.in_ground_time = small("in_ground_time", u64::from(u32::MAX)) as u32;
        t.life = small("life", u64::from(tr::DESPAWN_TICKS)) as u32;
        t.shake = small("shake", u64::from(tr::SHAKE_TICKS)) as u8;
        t.dealt_damage = v["dealt_damage"].as_bool().unwrap_or(false);
        t.no_physics = v["no_physics"].as_bool().unwrap_or(false);
        t.returning = small("returning", u64::from(u32::MAX)) as u32;
        t.last_block = v["in_block"].as_u64().and_then(|b| u16::try_from(b).ok()).map(Block);
        if let Some(d) = vec("dir") {
            t.dir = d.normalize_or(DVec3::X).as_vec3();
        }
        t.fire = small("fire", u64::from(u16::MAX)) as u16;
        Some(t)
    }

    /// A LAN client's copy: drawn only.
    pub(super) fn replica(pos: DVec3, previous_pos: DVec3, dir: Vec3, foil: bool) -> Self {
        let mut stack = Stack::new(crate::weapons::TRIDENT, 1);
        if foil {
            stack.enchants = stack.enchants.with(crate::enchant::Enchantment::Loyalty, 1);
        }
        let mut t = Self::new(stack, pos, DVec3::ZERO, None, Pickup::Disallowed);
        t.previous_pos = previous_pos;
        t.dir = dir;
        t
    }
}

/// A player's throw: from `eye` with Java view angles `y_rot`/`x_rot`
/// (degrees), `carry` the player's movement this tick (blocks per tick, its
/// `y` ignored on the ground).
#[derive(Clone, Copy, Debug)]
pub struct Throw {
    pub owner: PlayerId,
    pub eye: DVec3,
    pub y_rot: f32,
    pub x_rot: f32,
    pub carry: DVec3,
    pub on_ground: bool,
    pub creative: bool,
}

/// What a trident's move ran into.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Hit {
    Mob(usize),
    Player(PlayerId, DVec3),
    /// v1.0 gap Track 4c: an End crystal or a dragon part.
    Boss(super::dragon::Hit),
}

/// `Projectile.getMovementToShoot`: the normalized aim with a triangular
/// spread of `0.0172275 * uncertainty` per axis, times `power`.
fn shoot(aim: DVec3, power: f32, uncertainty: f32, rng: &mut Rng) -> DVec3 {
    let spread = tr::SPREAD * f64::from(uncertainty);
    let n = aim.normalize_or(DVec3::X);
    let jitter = DVec3::new(rng.triangle(0.0, spread), rng.triangle(0.0, spread), rng.triangle(0.0, spread));
    (n + jitter) * f64::from(power)
}

/// Whether `pos` lies inside a collision box of the block at `cell`.
fn inside_collision<W: MobWorld + ?Sized>(world: &W, cell: IVec3, pos: DVec3) -> bool {
    let mut out = Vec::new();
    crate::physics::colliders(world, cell.as_dvec3(), cell.as_dvec3() + DVec3::ONE, &mut out);
    out.iter().any(|c| c.cell == cell && pos.cmpge(c.min).all() && pos.cmple(c.max).all())
}

/// Java's `SpeleothemBlock` shape at `cell`, if `b` is pointed dripstone
/// or a sulfur spike.
fn dripstone_box(b: Block, cell: IVec3) -> Option<(DVec3, DVec3)> {
    use crate::world::overworld_blocks::Thickness;
    let s = crate::world::speleothem::of(b)?;
    let (down, thickness) = (s.down, s.thickness);
    let (width, lo, hi) = match thickness {
        Thickness::Tip if down => (6.0, 5.0, 16.0),
        Thickness::Tip => (6.0, 0.0, 11.0),
        Thickness::TipMerge => (6.0, 0.0, 16.0),
        Thickness::Frustum => (8.0, 0.0, 16.0),
        Thickness::Middle => (10.0, 0.0, 16.0),
        Thickness::Base => (12.0, 0.0, 16.0),
    };
    let o = cell.as_dvec3();
    let half = width / 32.0;
    Some((o + DVec3::new(0.5 - half, lo / 16.0, 0.5 - half), o + DVec3::new(0.5 + half, hi / 16.0, 0.5 + half)))
}

/// `AbstractArrow.shouldFall`: nothing solid within 0.06 of the point.
fn should_fall<W: MobWorld + ?Sized>(world: &W, pos: DVec3) -> bool {
    crate::physics::no_collision(world, pos - DVec3::splat(0.06), pos + DVec3::splat(0.06))
}

/// The first block collision box the move from `from` by `step` enters:
/// how far along (0..=1), its cell and face normal.
fn clip<W: MobWorld + ?Sized>(world: &W, from: DVec3, step: DVec3) -> Option<(f64, IVec3, IVec3)> {
    let to = from + step;
    let (lo, hi) = (from.min(to) - DVec3::splat(1e-3), from.max(to) + DVec3::splat(1e-3));
    let mut boxes = Vec::new();
    crate::physics::colliders(world, lo, hi, &mut boxes);
    // Pointed dripstone has no collision box in this engine, but Java's
    // `PointedDripstoneBlock` shapes stop projectiles (and are what a
    // trident breaks): columns 6/8/10/12 pixels wide by thickness.
    for y in lo.y.floor() as i32..=hi.y.floor() as i32 {
        for z in lo.z.floor() as i32..=hi.z.floor() as i32 {
            for x in lo.x.floor() as i32..=hi.x.floor() as i32 {
                let cell = IVec3::new(x, y, z);
                if let Some((min, max)) = world.block(cell).and_then(|b| dripstone_box(b, cell)) {
                    boxes.push(crate::physics::Collider { min, max, cell });
                }
            }
        }
    }
    boxes
        .iter()
        .filter_map(|c| {
            if from.cmpgt(c.min).all() && from.cmplt(c.max).all() {
                return None;
            }
            crate::physics::ray_aabb_face(from, step, c.min, c.max)
                .filter(|(t, _)| *t <= 1.0)
                .map(|(t, n)| (t, c.cell, n))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
}

impl super::Entities {
    /// Ticks every thrown trident, then summons Channeling's lightning and
    /// drops the items of tridents whose owner is gone.
    pub(super) fn update_tridents<W: MobWorld + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        ctx: &super::Ctx,
        boss: &super::dragon::Boxes,
        events: &mut Vec<EntityEvent>,
    ) {
        if self.tridents.is_empty() {
            return;
        }
        let mut effects = Vec::new();
        let (mobs, rng, pvp, thundering, sculk) =
            (&mut self.mobs, &mut self.rng, self.pvp, self.thundering, &mut self.sculk);
        self.tridents.retain_mut(|t| {
            if !world.entity_ticking(t.pos.floor().as_ivec3()) {
                return true;
            }
            let was_in_ground = t.in_ground;
            let keep = t.update(dt, world, mobs, &ctx.players, pvp, thundering, boss, rng, events, &mut effects);
            // `Projectile.onHit`: `projectile_land` where it stuck.
            if keep && !was_in_ground && t.in_ground {
                sculk.post(t.pos, crate::world::vibration::GameEvent::ProjectileLand, t.event_source(), None);
            }
            keep
        });
        for effect in effects {
            match effect {
                Effect::Lightning(at) => self.summon_lightning(at, false),
                Effect::Drop(stack, at) => self.scatter(stack, at),
            }
        }
    }

    /// Throws `stack` as a player's trident.
    pub fn throw_trident(&mut self, stack: Stack, throw: Throw) {
        let trident = Trident::thrown(stack, throw, &mut self.rng);
        // `Projectile.tick`'s first tick: `projectile_shoot` from its owner.
        let source = trident.event_source();
        self.sculk.post(trident.pos, crate::world::vibration::GameEvent::ProjectileShoot, source, None);
        self.tridents.push(trident);
    }

    /// Player `id` at `feet` with box `shape` picks up every trident it may
    /// (`AbstractArrow.playerTouch`). `give` puts the item in the inventory
    /// and says whether it fit; a trident that doesn't fit stays. Returns
    /// whether anything was taken.
    pub fn collect_tridents(
        &mut self,
        id: PlayerId,
        feet: DVec3,
        shape: crate::physics::Shape,
        creative: bool,
        mut give: impl FnMut(Stack) -> bool,
    ) -> bool {
        let mut taken = false;
        self.tridents.retain(|t| match t.touch(id, feet, shape, creative) {
            Some(Some(stack)) if give(stack) => {
                taken = true;
                false
            }
            Some(None) => {
                taken = true;
                false
            }
            _ => true,
        });
        taken
    }

    /// `;`-free JSON of every trident, stored ones included.
    pub(super) fn tridents_json(&self) -> String {
        let stored = self.storage.chunks();
        let all: Vec<Value> =
            self.tridents.iter().chain(stored.iter().flat_map(|c| &c.tridents)).map(Trident::save).collect();
        Value::Array(all).to_string()
    }

    pub(super) fn load_tridents_json(&mut self, text: &str) {
        let Ok(Value::Array(list)) = serde_json::from_str::<Value>(text) else { return };
        self.tridents.extend(list.iter().filter_map(Trident::load));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{Ctx, Entities};
    use crate::physics::test_util::Grid;
    use crate::world::terrain::Dimension;

    fn ctx() -> Ctx {
        Ctx {
            players: vec![Target::new(PlayerId::HOST, DVec3::new(0.5, 10.0, 0.5), true)],
            daylight: 1.0,
            spawn_light: crate::simulation::lighting::SpawnLight::DAY,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
        }
    }

    fn flying(stack: Stack, from: DVec3, vel: DVec3) -> Trident {
        Trident::new(stack, from, vel, Some(Owner::Player(PlayerId::HOST)), Pickup::Allowed)
    }

    fn run(e: &mut Entities, world: &Grid, ticks: u32) -> Vec<EntityEvent> {
        (0..ticks).flat_map(|_| e.update(0.05, world, &ctx())).collect()
    }

    /// v1.0 gap Track 4c: `ThrownTrident.onHitEntity` on a dragon part: a
    /// player's trident takes 8 (a quarter plus one on the body), and it
    /// bounces back whatever happens; perched, the dragon takes nothing.
    #[test]
    fn a_trident_strikes_the_dragon_and_bounces_back() {
        for (sitting, lost) in [(false, 3.0), (true, 0.0)] {
            let world = Grid::flat(10);
            let mut e = Entities::new(3);
            e.wild_dragons.spawn(DVec3::new(10.5, 40.0, 0.5), 0.0);
            if sitting {
                e.wild_dragons.dragons[0].dragon.phase = crate::entity::dragon::Phase::SittingScanning;
            }
            let at = e.wild_dragons.dragons[0].dragon.pos;
            let from = DVec3::new(at.x - 8.0, at.y + 1.2, at.z);
            e.tridents.push(flying(Stack::new(crate::weapons::TRIDENT, 1), from, DVec3::X * 2.5));
            let events = run(&mut e, &world, 4);
            let t = &e.tridents[0];
            assert!(t.dealt_damage && t.vel.x < 0.0 && t.vel.x > -0.1, "bounced: {}", t.vel);
            assert!(
                events
                    .iter()
                    .any(|ev| matches!(ev, EntityEvent::Sound { sound: MobSound::Weapon(Cue::TridentHit), .. }))
            );
            let health = e.wild_dragons.dragons[0].dragon.health;
            assert_eq!(crate::entity::dragon::MAX_HEALTH - health, lost, "sitting {sitting}");
        }
    }

    /// v1.0 gap Track 8b: a perched dragon's scales set a trident alight
    /// for a second (`AbstractDragonSittingPhase.onHurt`, it's an
    /// `AbstractArrow`); a flying dragon doesn't. The bounce turns its yaw
    /// 170 to 190°.
    #[test]
    fn a_perched_dragon_sets_a_trident_alight() {
        for sitting in [false, true] {
            let world = Grid::flat(10);
            let mut e = Entities::new(3);
            e.wild_dragons.spawn(DVec3::new(10.5, 40.0, 0.5), 0.0);
            if sitting {
                e.wild_dragons.dragons[0].dragon.phase = crate::entity::dragon::Phase::SittingScanning;
            }
            let at = e.wild_dragons.dragons[0].dragon.pos;
            let from = DVec3::new(at.x - 8.0, at.y + 1.2, at.z);
            e.tridents.push(flying(Stack::new(crate::weapons::TRIDENT, 1), from, DVec3::X * 2.5));
            let mut yaw = None;
            for _ in 0..4 {
                run(&mut e, &world, 1);
                let t = &e.tridents[0];
                if t.dealt_damage {
                    break;
                }
                yaw = t.rot.map(|r| r.0);
            }
            let t = &e.tridents[0];
            assert!(t.dealt_damage, "hit the dragon");
            assert_eq!(t.burning(), sitting, "sitting {sitting}");
            if sitting {
                assert!((18..=20).contains(&t.fire), "{}", t.fire);
            }
            let turned = t.rot.unwrap().0 - yaw.unwrap();
            assert!((170.0..=190.0).contains(&turned), "turned {turned}");
        }
    }

    /// v1.0 gap Track 8b: lava sets a trident alight (15 seconds); a
    /// burning one hitting a campfire reports it (`CampfireBlock.onProjectileHit`)
    /// and keeps its fire through a save; water puts it out.
    #[test]
    fn lava_lights_a_trident_and_water_puts_it_out() {
        use crate::world::block::Facing;
        use crate::world::utilities::{Campfire, campfire};
        let fire = campfire(Campfire { soul: false, lit: false, signal: false, wet: false, facing: Facing::North });
        let mut world = Grid::flat(10);
        world.set(IVec3::new(3, 11, 0), Block::LAVA);
        world.set(IVec3::new(8, 11, 0), fire);
        let mut e = Entities::new(3);
        e.tridents.push(flying(Stack::new(crate::weapons::TRIDENT, 1), DVec3::new(0.5, 11.35, 0.5), DVec3::X * 2.5));
        let mut events = Vec::new();
        for _ in 0..6 {
            events.extend(run(&mut e, &world, 1));
            if e.tridents[0].in_ground {
                break;
            }
        }
        let t = &e.tridents[0];
        assert!(t.in_ground && t.burning() && t.fire > 290, "{} {}", t.in_ground, t.fire);
        assert!(events.iter().any(|ev| matches!(
            ev,
            EntityEvent::ProjectileBlockHit { cell, burning: true, mob_owned: false, .. } if *cell == IVec3::new(8, 11, 0)
        )));
        let back = Trident::load(&t.save()).unwrap();
        assert_eq!(back.fire, t.fire, "saved as Java's `Fire`");
        // Stuck, the fire stays (no `baseTick` in the ground)...
        let before = t.fire;
        run(&mut e, &world, 5);
        assert_eq!(e.tridents[0].fire, before);
        // ...until water reaches it.
        world.set(IVec3::new(7, 11, 0), Block::WATER);
        run(&mut e, &world, 1);
        assert!(!e.tridents[0].burning());
    }

    #[test]
    fn a_breeze_turns_a_trident_back_unhurt() {
        let world = Grid::flat(10);
        let mut e = Entities::new(3);
        e.spawn(MobKind::Breeze, DVec3::new(4.5, 10.0, 0.5));
        e.tridents.push(flying(Stack::new(crate::weapons::TRIDENT, 1), DVec3::new(0.5, 10.6, 0.5), DVec3::X * 1.2));
        let events = run(&mut e, &world, 4);
        assert_eq!(e.mobs[0].health, MobKind::Breeze.max_health());
        assert!(events.iter().any(|ev| matches!(
            ev,
            EntityEvent::Sound { sound: MobSound::Breeze(crate::entity::breeze::Cue::Deflect), .. }
        )));
        assert!(!e.tridents[0].dealt_damage && e.tridents[0].vel.x < 0.0, "deflected, not a hit");
    }

    #[test]
    fn a_breeze_turns_an_arrow_back_unhurt() {
        let world = Grid::flat(10);
        let mut e = Entities::new(3);
        e.spawn(MobKind::Breeze, DVec3::new(4.5, 10.0, 0.5));
        e.arrows.push(crate::entity::Arrow::shot(DVec3::new(2.5, 10.6, 0.5), DVec3::X, 1.0, false));
        let events = run(&mut e, &world, 4);
        assert_eq!(e.mobs[0].health, MobKind::Breeze.max_health());
        assert!(events.iter().any(|ev| matches!(
            ev,
            EntityEvent::Sound { sound: MobSound::Breeze(crate::entity::breeze::Cue::Deflect), .. }
        )));
        assert!(e.arrows[0].velocity().x < 0.0, "it flies back");
    }

    /// v1.0 gap Track 8c: an enderman dodges a trident
    /// (`projectileReceivesSideEffectsOnHit(false)` is false for it): no hit
    /// sound and no bounce, it flies on, hitting nothing more.
    #[test]
    fn a_trident_flies_on_past_a_dodging_enderman() {
        let world = Grid::flat(10);
        let mut e = Entities::new(3);
        e.spawn(MobKind::Enderman, DVec3::new(4.5, 10.0, 0.5));
        e.spawn(MobKind::Cow, DVec3::new(7.5, 10.0, 0.5));
        e.tridents.push(flying(Stack::new(crate::weapons::TRIDENT, 1), DVec3::new(0.5, 10.6, 0.5), DVec3::X * 1.2));
        let events = run(&mut e, &world, 4);
        assert!(
            !events.iter().any(|ev| matches!(ev, EntityEvent::Sound { sound: MobSound::Weapon(Cue::TridentHit), .. }))
        );
        let t = &e.tridents[0];
        assert!(t.dealt_damage && t.vel.x > 0.0 && t.pos.x > 5.0, "flew on: {} at {}", t.vel, t.pos);
        assert!(e.mobs[0].teleport_pending);
        assert_eq!(e.mobs[1].health, MobKind::Cow.max_health(), "it hits nothing more");
    }

    #[test]
    fn a_thrown_trident_hits_for_eight_bounces_back_and_sticks() {
        let world = Grid::flat(10);
        let mut e = Entities::new(3);
        e.spawn(MobKind::Cow, DVec3::new(4.5, 10.0, 0.5));
        e.tridents.push(flying(Stack::new(crate::weapons::TRIDENT, 1), DVec3::new(0.5, 10.6, 0.5), DVec3::X * 1.2));
        let events = run(&mut e, &world, 4);
        assert_eq!(e.mobs[0].health, MobKind::Cow.max_health() - 8.0);
        assert!(
            events.iter().any(|ev| matches!(ev, EntityEvent::Sound { sound: MobSound::Weapon(Cue::TridentHit), .. }))
        );
        assert!(e.tridents[0].dealt_damage && e.tridents[0].vel.x < 0.0, "it bounced back");
        run(&mut e, &world, 60);
        assert!(e.tridents[0].in_ground, "it fell and stuck");
        assert_eq!(e.mobs[0].health, MobKind::Cow.max_health() - 8.0, "only one hit");
        // Its owner picks it up; nobody else may.
        let shape = crate::physics::Shape { half_width: 0.3, height: 1.8 };
        let feet = e.tridents[0].pos;
        assert!(!e.collect_tridents(PlayerId(5), feet, shape, false, |_| true));
        let mut got = None;
        assert!(e.collect_tridents(PlayerId::HOST, feet, shape, false, |s| {
            got = Some(s);
            true
        }));
        assert_eq!(got.map(|s| s.item), Some(crate::weapons::TRIDENT));
        assert!(e.tridents.is_empty());
    }

    #[test]
    fn impaling_adds_to_aquatic_targets() {
        let world = Grid::flat(10);
        let mut e = Entities::new(3);
        e.spawn(MobKind::Turtle, DVec3::new(4.5, 10.0, 0.5));
        let mut stack = Stack::new(crate::weapons::TRIDENT, 1);
        stack.enchants = stack.enchants.with(crate::enchant::Enchantment::Impaling, 1);
        e.tridents.push(flying(stack, DVec3::new(0.5, 10.5, 0.5), DVec3::X * 1.2));
        run(&mut e, &world, 4);
        assert_eq!(e.mobs[0].health, MobKind::Turtle.max_health() - 10.5);
    }

    #[test]
    fn loyalty_flies_back_through_blocks_to_its_owner() {
        let world = Grid::flat(10);
        let mut e = Entities::new(3);
        let mut stack = Stack::new(crate::weapons::TRIDENT, 1);
        stack.enchants = stack.enchants.with(crate::enchant::Enchantment::Loyalty, 3);
        e.tridents.push(flying(stack, DVec3::new(12.5, 10.3, 0.5), DVec3::new(0.0, -0.5, 0.0)));
        run(&mut e, &world, 10);
        let t = &e.tridents[0];
        assert!(t.no_physics, "returning after five ticks in the ground");
        let start = t.pos.distance(DVec3::new(0.5, 11.6, 0.5));
        run(&mut e, &world, 10);
        assert!(e.tridents[0].pos.distance(DVec3::new(0.5, 11.6, 0.5)) < start);
        // The owner catches it on the way back.
        let shape = crate::physics::Shape { half_width: 0.3, height: 1.8 };
        let feet = e.tridents[0].pos;
        assert!(e.collect_tridents(PlayerId::HOST, feet, shape, false, |_| true));
    }

    #[test]
    fn fast_tridents_break_pointed_dripstone() {
        let mut world = Grid::flat(10);
        let tip = IVec3::new(4, 11, 0);
        world.set(
            tip,
            crate::world::overworld_blocks::pointed_dripstone(true, crate::world::overworld_blocks::Thickness::Tip),
        );
        for (speed, breaks) in [(1.5, true), (0.5, false)] {
            let mut e = Entities::new(3);
            e.tridents.push(flying(
                Stack::new(crate::weapons::TRIDENT, 1),
                DVec3::new(0.5, 11.8, 0.5),
                DVec3::X * speed,
            ));
            let events = run(&mut e, &world, 12);
            let broke = events
                .iter()
                .any(|ev| matches!(ev, EntityEvent::TridentBrokeDripstone { cell, mob_owned: false } if *cell == tip));
            assert_eq!(broke, breaks, "speed {speed}");
        }
    }

    #[test]
    fn tridents_and_drowned_hands_save_and_reload() {
        let mut e = Entities::new(3);
        let mut stack = Stack::new(crate::weapons::TRIDENT, 1);
        stack.damage = 17;
        stack.enchants = stack.enchants.with(crate::enchant::Enchantment::Loyalty, 2);
        let mut t = flying(stack, DVec3::new(1.5, 12.0, -3.25), DVec3::new(0.3, -0.2, 0.1));
        t.in_ground = true;
        t.shake = 3;
        t.dealt_damage = true;
        e.tridents.push(t);
        e.spawn(MobKind::Drowned, DVec3::new(0.5, 10.0, 0.5));
        e.mobs[0].equipment.set(super::super::equipment::Slot::MainHand, Some(Stack::new(crate::weapons::TRIDENT, 1)));
        e.mobs[0].persistent = true;
        let mut props = std::collections::BTreeMap::new();
        e.save_props(&mut props);
        let mut back = Entities::new(9);
        back.load_props(&props);
        let t = &back.tridents[0];
        assert_eq!((t.stack.damage, t.loyalty(), t.in_ground, t.shake), (17, 2, true, 3));
        assert_eq!(t.owner, Some(Owner::Player(PlayerId::HOST)));
        assert_eq!(t.pos, DVec3::new(1.5, 12.0, -3.25));
        assert!(t.dealt_damage);
        let drowned = super::super::nether::save_mob(&e.mobs[0]);
        let m = super::super::nether::load_mob(&drowned).unwrap();
        assert!(m.holds_trident());
    }

    #[test]
    fn drowned_with_tridents_throw_them_at_players_in_range() {
        let world = Grid::flat(10);
        let mut e = Entities::new(3);
        e.spawn(MobKind::Drowned, DVec3::new(0.5, 10.0, 0.5));
        e.mobs[0].equipment.set(super::super::equipment::Slot::MainHand, Some(Stack::new(crate::weapons::TRIDENT, 1)));
        let mut c = ctx();
        c.daylight = 0.0;
        c.spawn_light = crate::simulation::lighting::SpawnLight::NIGHT;
        c.players[0].pos = DVec3::new(8.5, 10.0, 0.5);
        let events: Vec<_> = (0..120).flat_map(|_| e.update(0.05, &world, &c)).collect();
        assert!(
            events.iter().any(|ev| matches!(ev, EntityEvent::Sound { sound: MobSound::Weapon(Cue::DrownedShoot), .. })),
            "{events:?}"
        );
        assert!(events.iter().any(|ev| matches!(
            ev,
            EntityEvent::PlayerHit { cause: crate::simulation::damage::Cause::DROWNED_TRIDENT, .. }
        )));
    }
}
