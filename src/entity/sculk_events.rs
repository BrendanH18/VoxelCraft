//! The entity side of sculk and vibrations (v0.16 track A): mobs' steps,
//! swims, landings, damage and deaths become game events
//! (`Entity.move`, `checkFallDamage`, `LivingEntity.hurtServer`, `die`),
//! mobs standing on sensors and shriekers trigger them (`stepOn`), and a
//! catalyst near a death takes its experience
//! ([`crate::world::World::sculk_catalyst_death`]).

use glam::{DVec3, IVec3};
use rustc_hash::FxHashMap;

use super::{Entities, MobKind};
use crate::world::World;
use crate::world::block::Block;
use crate::world::vibration::{EntityRef, EventSource, GameEvent};

/// Per-mob movement bookkeeping (Java's `moveDist`, `nextStep` and the fall
/// it is in), not saved.
#[derive(Clone, Copy, Debug, Default)]
struct Watch {
    dying: bool,
    /// `wasTouchingWater`, for `doWaterSplashEffect`.
    in_water: bool,
    /// A creeper's fuse was lit (`swellDir > 0`), or a witch was drinking.
    fusing: bool,
    /// The creeper's swell last tick: `prime_fuse` needs it at 0.
    fuse: f32,
    drinking: bool,
    hurt: f32,
    move_dist: f32,
    next_step: f32,
    peak: Option<f64>,
    /// v1.0 Track 9: the states whose changes Java posts as game events.
    /// The armadillo's pose (`rollUp`, `rollOut`).
    armadillo: u8,
    /// A camel sitting (`sitDown`, `standUp`) and its dash cooldown
    /// (`handleStartJump`); a nautilus dashing.
    sitting: bool,
    dash_cooldown: u16,
    dashing: bool,
    /// A sulfur cube's fuse is lit (`primeTime`).
    primed: bool,
    /// A creaking's `isActive`, `canMove` and twitch (`hurtServer`).
    creaking: (bool, bool, f32),
    /// The item in each equipment slot (`onEquipItem`).
    equipment: [Option<crate::item::Item>; 8],
    /// The vehicle this mob rides (`startRiding`).
    riding: Option<u32>,
    /// The mob's bounce count (`Mob::bounces`).
    bounces: u32,
    /// Ticks seen, for the per-type flap rhythms (`isFlapping`).
    ticks: u32,
    /// `flyDist`, `nextFlap` and `flapSpeed` (chickens and parrots).
    fly_dist: f32,
    next_flap: f32,
    flap_speed: f32,
}

/// v1.0 Track 9: the per-mob states [`Watch`] compares from tick to tick.
fn equipment_items(m: &super::Mob) -> [Option<crate::item::Item>; 8] {
    super::equipment::Slot::ALL.map(|slot| m.equipment.get(slot).map(|s| s.item))
}

/// `Phantom.TICKS_PER_FLAP` (`ceil(24.166098)`), `Vex.TICKS_PER_FLAP`
/// (`ceil(5π/4)`) and `Bee.TICKS_PER_FLAP` (`ceil(1.4959966)`).
const PHANTOM_FLAP_TICKS: u32 = 25;
const VEX_FLAP_TICKS: u32 = 4;
const BEE_FLAP_TICKS: u32 = 2;

#[derive(Default)]
pub struct Sculk {
    /// Loaded catalysts, refreshed from the world every update.
    pub catalysts: Vec<IVec3>,
    /// Deaths a catalyst takes: where, and the experience they would drop.
    pub deaths: Vec<(DVec3, u32)>,
    /// Game events posted where no world is at hand (eating, shooting,
    /// arrows landing), delivered at the next entity update.
    pub events: Vec<(DVec3, GameEvent, EventSource, Option<Block>)>,
    watch: FxHashMap<u32, Watch>,
}

impl Sculk {
    /// Queues a game event for the world (bounded).
    pub fn post(&mut self, pos: DVec3, event: GameEvent, source: EventSource, affected: Option<Block>) {
        if self.events.len() < 256 {
            self.events.push((pos, event, source, affected));
        }
    }

    /// The source of a projectile shot by a player or mob.
    pub fn projectile_source(owner: Option<super::PlayerId>, shooter: Option<u32>) -> EventSource {
        EventSource::projectile(owner.map(EntityRef::Player).or(shooter.map(EntityRef::Mob)))
    }

    /// `Projectile.tick`'s first tick: `projectile_shoot` where it starts,
    /// from its owner.
    pub fn shot(&mut self, pos: DVec3, source: EventSource) {
        self.post(pos, GameEvent::ProjectileShoot, source, None);
    }

    /// `Projectile.onHit`'s `projectile_land`: a block hit posts at the
    /// struck block's centre naming it (wool dampens it); anything else
    /// (an entity) at the projectile. `heading` is the way it was flying.
    pub fn land<W: crate::physics::BlockSource + ?Sized>(
        &mut self,
        world: &W,
        pos: DVec3,
        heading: DVec3,
        source: EventSource,
    ) {
        let ahead = (pos + heading.normalize_or_zero() * 0.05).floor().as_ivec3();
        let struck = [ahead, pos.floor().as_ivec3()]
            .into_iter()
            .find_map(|c| world.block(c).filter(|b| b.is_solid()).map(|b| (c, b)));
        match struck {
            Some((cell, block)) => {
                let center = cell.as_dvec3() + DVec3::splat(0.5);
                self.post(center, GameEvent::ProjectileLand, source, Some(block));
            }
            None => self.post(pos, GameEvent::ProjectileLand, source, None),
        }
    }

    /// A loaded catalyst within its 8-block listener radius of `pos`.
    pub fn catalyst_near(&self, pos: DVec3) -> bool {
        let cell = pos.floor().as_ivec3();
        self.catalysts.iter().any(|&c| (c - cell).length_squared() <= 64)
    }
}

/// `Entity.getOnPos`: the block 0.2 below the feet.
pub fn on_pos(feet: DVec3) -> IVec3 {
    (feet - DVec3::Y * 0.2).floor().as_ivec3()
}

impl Entities {
    /// v1.0 Track 9: queues `event` at mob `index`'s position, from it
    /// (`Entity.gameEvent(event)`).
    pub(crate) fn post_mob_event(&mut self, index: usize, event: GameEvent) {
        if let Some(m) = self.mobs.get(index) {
            let (pos, source) = (m.pos, super::warden::mob_source(m));
            self.sculk.post(pos, event, source, None);
        }
    }

    /// Gives catalysts the deaths waiting since the last update; experience
    /// no catalyst took after all drops as usual.
    pub fn settle_sculk_deaths(&mut self, w: &mut World) {
        for (pos, xp) in std::mem::take(&mut self.sculk.deaths) {
            if !w.sculk_catalyst_death(pos, xp) && xp > 0 {
                self.spawn_xp(pos, xp);
            }
        }
    }

    /// Before the entity update: the world's catalysts and pending deaths.
    pub(super) fn sculk_before(&mut self, w: &mut World) {
        self.sculk.catalysts = w.sculk_catalysts();
        self.settle_sculk_deaths(w);
        self.flush_sculk_events(w);
    }

    /// Delivers the queued game events to the world.
    pub fn flush_sculk_events(&mut self, w: &mut World) {
        for (pos, event, source, affected) in std::mem::take(&mut self.sculk.events) {
            w.emit_game_event(pos, event, source, affected);
        }
    }

    /// After the entity update: what each mob did this tick, as game events.
    pub(super) fn mob_game_events(&mut self, w: &mut World) {
        self.ensure_mob_ids();
        let mut alive = rustc_hash::FxHashSet::default();
        for m in &self.mobs {
            alive.insert(m.uid);
            let source = super::warden::mob_source(m);
            let vehicle = m.riding.and_then(|id| self.vehicle_pos(id));
            let first = !self.sculk.watch.contains_key(&m.uid);
            let watch = self.sculk.watch.entry(m.uid).or_insert(Watch {
                dying: m.dying.is_some(),
                in_water: m.in_water,
                fusing: m.swelling,
                fuse: m.fuse,
                drinking: m.drinking_potion(),
                hurt: m.hurt,
                next_step: 1.0,
                armadillo: m.armadillo.as_ref().map_or(0, |a| a.pose),
                sitting: m.mount.as_ref().is_some_and(|s| s.sit_ticks > 0),
                dash_cooldown: m.mount.as_ref().map_or(0, |s| s.dash_cooldown),
                dashing: m.nautilus.as_ref().is_some_and(|n| n.dashing),
                primed: m.sulfur_cube.as_ref().is_some_and(|c| c.primed()),
                creaking: m.creaking.as_ref().map_or((false, true, 0.0), |c| (c.active, c.can_move, c.hit_cooldown)),
                equipment: equipment_items(m),
                riding: m.riding,
                bounces: m.bounces,
                ..Watch::default()
            });
            watch.ticks = watch.ticks.wrapping_add(1);
            if !first {
                if m.dying.is_some() && !watch.dying {
                    w.emit_game_event(m.pos, GameEvent::EntityDie, source, None);
                } else if m.alive() && m.hurt > watch.hurt + 1e-4 {
                    w.emit_game_event(m.pos, GameEvent::EntityDamage, source, None);
                }
            }
            watch.dying = m.dying.is_some();
            watch.hurt = m.hurt;
            // `Entity.updateFluidInteraction`: entering water splashes.
            if !first && m.alive() && m.in_water && !watch.in_water {
                w.emit_game_event(m.pos, GameEvent::Splash, source, None);
            }
            watch.in_water = m.in_water;
            // `Creeper.tick`: the swell starting from nothing posts
            // `prime_fuse` (not one a fall left).
            let fusing = m.swelling;
            if !first && m.alive() && fusing && !watch.fusing && watch.fuse == 0.0 {
                w.emit_game_event(m.pos, GameEvent::PrimeFuse, source, None);
            }
            watch.fusing = fusing;
            watch.fuse = m.fuse;
            // `Witch.aiStep`: finishing a potion posts `drink`.
            let drinking = m.drinking_potion();
            if !first && m.alive() && watch.drinking && !drinking {
                w.emit_game_event(m.pos, GameEvent::Drink, source, None);
            }
            watch.drinking = drinking;
            if !first && m.alive() {
                mob_actions(w, m, source, watch, vehicle);
            }
            if !m.alive() || !m.simulation_active || m.riding.is_some() {
                continue;
            }
            let below = on_pos(m.pos);
            let support = w.get_block(below).filter(|&b| b != Block::AIR);
            // `Entity.applyMovementEmissionAndPlaySound`.
            let moved = m.pos - m.previous_pos;
            watch.move_dist += (moved.with_y(0.0).length() * 0.6) as f32;
            let fly = moved.length() as f32 * 0.6;
            watch.fly_dist += fly;
            if matches!(m.kind, MobKind::Chicken | MobKind::Parrot) {
                // `Chicken.aiStep` / `Parrot.calculateFlapping`.
                let lift = if m.on_ground || m.riding.is_some() { -1.0 } else { 4.0 };
                watch.flap_speed = (watch.flap_speed + lift * 0.3).clamp(0.0, 1.0);
            }
            if watch.move_dist > watch.next_step
                && let Some(block) = support
            {
                let next =
                    if m.kind == MobKind::Warden { watch.move_dist + 0.55 } else { watch.move_dist.floor() + 1.0 };
                if m.on_ground && !m.in_water {
                    watch.next_step = next;
                    w.emit_game_event(m.pos, GameEvent::Step, source, Some(block));
                } else if m.in_water {
                    watch.next_step = next;
                    w.emit_game_event(m.pos, GameEvent::Swim, source, None);
                }
            } else if support.is_none() && flapping(m, watch) {
                // v1.0 Track 9: `Entity.processFlappingMovement` over air.
                w.emit_game_event(m.pos, GameEvent::Flap, source, None);
            }
            // `checkFallDamage`: landing after any fall.
            if m.on_ground {
                if watch.peak.is_some_and(|peak| peak > m.pos.y + 1e-6) && !m.in_water {
                    w.emit_game_event(m.pos, GameEvent::HitGround, source, support);
                }
                watch.peak = None;
            } else if !m.in_water {
                watch.peak = Some(watch.peak.map_or(m.previous_pos.y.max(m.pos.y), |p| p.max(m.pos.y)));
            } else {
                watch.peak = None;
            }
            if m.on_ground {
                w.sculk_step_on(below, m.pos, source, m.kind == MobKind::Warden);
            }
        }
        self.sculk.watch.retain(|uid, _| alive.contains(uid));
    }
}

/// v1.0 Track 9: `isFlapping` (and `onFlap`) for the kinds that override
/// it: bats awake every 10 ticks, bees in the air every 2, vexes every 4,
/// phantoms every 25 from their own offset (`getId() * 3`), allays whenever
/// airborne, and chickens and parrots by flown distance.
fn flapping(m: &super::Mob, watch: &mut Watch) -> bool {
    match m.kind {
        MobKind::Bat => m.bat.as_ref().is_none_or(|b| !b.resting) && watch.ticks.is_multiple_of(10),
        MobKind::Bee => !m.on_ground && watch.ticks.is_multiple_of(BEE_FLAP_TICKS),
        MobKind::Vex => watch.ticks.is_multiple_of(VEX_FLAP_TICKS),
        MobKind::Phantom => (m.uid.wrapping_mul(3).wrapping_add(watch.ticks)).is_multiple_of(PHANTOM_FLAP_TICKS),
        MobKind::Allay => !m.on_ground,
        MobKind::Chicken | MobKind::Parrot if watch.fly_dist > watch.next_flap => {
            watch.next_flap = watch.fly_dist + watch.flap_speed / 2.0;
            true
        }
        _ => false,
    }
}

/// v1.0 Track 9: the mob behaviours Java posts as game events, found by
/// comparing each mob's state with the last tick's. `vehicle` is where the
/// mob's vehicle is, if it rides one.
fn mob_actions(w: &mut World, m: &super::Mob, source: EventSource, watch: &mut Watch, vehicle: Option<DVec3>) {
    let mut events = Vec::new();
    let mut post = |event| events.push(event);
    // `Armadillo.rollUp` (idle to rolling) and `rollOut` (back to idle).
    if let Some(a) = &m.armadillo {
        use super::armadillo::{IDLE, ROLLING};
        if a.pose != watch.armadillo && (a.pose == ROLLING && watch.armadillo == IDLE || a.pose == IDLE) {
            post(GameEvent::EntityAction);
        }
        watch.armadillo = a.pose;
    }
    if let Some(s) = &m.mount {
        // `Camel.sitDown`, `standUp` and `standUpInstantly`.
        let sitting = s.sit_ticks > 0;
        if sitting != watch.sitting && super::mounts::camel_like(m.kind) {
            post(GameEvent::EntityAction);
        }
        watch.sitting = sitting;
        // `Camel.handleStartJump`: the dash starts its cooldown.
        if s.dash_cooldown > watch.dash_cooldown && super::mounts::camel_like(m.kind) {
            post(GameEvent::EntityAction);
        }
        watch.dash_cooldown = s.dash_cooldown;
    }
    // `AbstractNautilus.handleStartJump`.
    if let Some(n) = &m.nautilus {
        if n.dashing && !watch.dashing {
            post(GameEvent::EntityAction);
        }
        watch.dashing = n.dashing;
    }
    // `SulfurCube.primeTime`.
    if let Some(c) = &m.sulfur_cube {
        if c.primed() && !watch.primed {
            post(GameEvent::PrimeFuse);
        }
        watch.primed = c.primed();
    }
    // `Creaking.activate`, `deactivate`, the `canMove` change in `aiStep`,
    // and a protected creaking's twitch when hit (`hurtServer`).
    if let Some(c) = &m.creaking {
        let (active, can_move, twitch) = watch.creaking;
        if c.active != active {
            post(GameEvent::EntityAction);
        }
        if c.can_move != can_move {
            post(GameEvent::EntityAction);
        }
        if c.hit_cooldown > twitch + 1e-4 {
            post(GameEvent::EntityAction);
        }
        watch.creaking = (c.active, c.can_move, c.hit_cooldown);
    }
    // `Turtle.aiStep`: digging its nest every fifth tick of the clutch.
    if m.turtle_digging() && w.get_block(m.pos.floor().as_ivec3() - IVec3::Y).is_some_and(super::mob::is_sand) {
        post(GameEvent::EntityAction);
    }
    // `LivingEntity.onEquipItem`: `equip` for an item with an `EQUIPPABLE`
    // component, `unequip` for anything else (an empty slot, a weapon).
    let equipment = equipment_items(m);
    for (old, new) in watch.equipment.iter().zip(equipment) {
        if *old != new {
            let equippable = new.is_some_and(|i| crate::world::dispensing::equippable(i).is_some());
            post(if equippable { GameEvent::Equip } else { GameEvent::Unequip });
        }
    }
    watch.equipment = equipment;
    for event in events {
        w.emit_game_event(m.pos, event, source, None);
    }
    // `Entity.startRiding`: `entity_mount` at the vehicle, from the rider.
    if m.riding.is_some()
        && m.riding != watch.riding
        && let Some(at) = vehicle
    {
        w.emit_game_event(at, GameEvent::EntityMount, source, None);
    }
    watch.riding = m.riding;
    // `Entity.restituteMovementAfterCollisions`: a slime block or bed threw it up.
    if m.bounces != watch.bounces {
        w.emit_game_event(m.pos, GameEvent::Bounce, source, None);
    }
    watch.bounces = m.bounces;
}

#[cfg(test)]
mod tests;
