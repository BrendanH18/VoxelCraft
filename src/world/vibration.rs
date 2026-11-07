//! Java 26.3 game events and the vibration system (MOB-04, v0.16 track A).
//!
//! Anything that happens in a world (a step, a placed block, an opened
//! chest, a death) is posted as a [`GameEvent`] at a position with an
//! [`EventSource`]: [`World::emit_game_event`] or the free
//! [`emit_game_event`]. Listeners within their radius hear it:
//!
//! - sculk sensors (radius 8), calibrated sculk sensors (16) and sculk
//!   shriekers (8), the block listeners of [`super::sculk`];
//! - wardens (16, at their eyes), which the entity simulation drains from
//!   [`World::take_entity_game_events`] each tick;
//! - sculk catalysts, which only hear deaths
//!   ([`World::sculk_catalyst_death`]).
//!
//! A vibration listener ([`VibrationData`]) follows `VibrationSystem`:
//! `Listener.handleGameEvent` rejects an event while a vibration travels,
//! when the event isn't in the listener's tag, from a spectator, from a
//! source stepping carefully for the `ignore_vibrations_sneaking` events,
//! from a source that dampens vibrations, or on a block in
//! `dampens_vibrations`, and when wool occludes all six nudged rays.
//! Accepted events become candidates of a `VibrationSelector`: per game
//! tick the nearest wins, then the higher frequency. A candidate is chosen
//! on a later tick, travels `floor(distance)` ticks and is received.
//!
//! Sources: `docs/verification/2026-11-06-v0.16-sculk-warden.md`.

use glam::{DVec3, IVec3};
use serde_json::{Value, json};

use super::World;
use super::block::Block;
use crate::entity::PlayerId;

/// Java's game events (`GameEvent`), in registry order, with the fifteen
/// `resonate_N` events folded into [`GameEvent::Resonate`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GameEvent {
    BlockActivate,
    BlockAttach,
    BlockChange,
    BlockClose,
    BlockDeactivate,
    BlockDestroy,
    BlockDetach,
    BlockOpen,
    BlockPlace,
    Bounce,
    ContainerClose,
    ContainerOpen,
    Drink,
    Eat,
    ElytraGlide,
    EntityDamage,
    EntityDie,
    EntityDismount,
    EntityInteract,
    EntityMount,
    EntityPlace,
    EntityAction,
    Equip,
    Explode,
    Flap,
    FluidPickup,
    FluidPlace,
    HitGround,
    InstrumentPlay,
    ItemInteractFinish,
    ItemInteractStart,
    JukeboxPlay,
    JukeboxStopPlay,
    LightningStrike,
    NoteBlockPlay,
    PrimeFuse,
    ProjectileLand,
    ProjectileShoot,
    SculkSensorTendrilsClicking,
    Shear,
    Shriek,
    Splash,
    Step,
    Swim,
    Teleport,
    Unequip,
    /// `resonate_1` to `resonate_15`: an amethyst block next to an activated
    /// sensor re-emits the sensor's frequency.
    Resonate(u8),
}

impl GameEvent {
    /// Every event except the resonances, in registry order.
    pub const PLAIN: [GameEvent; 46] = [
        GameEvent::BlockActivate,
        GameEvent::BlockAttach,
        GameEvent::BlockChange,
        GameEvent::BlockClose,
        GameEvent::BlockDeactivate,
        GameEvent::BlockDestroy,
        GameEvent::BlockDetach,
        GameEvent::BlockOpen,
        GameEvent::BlockPlace,
        GameEvent::Bounce,
        GameEvent::ContainerClose,
        GameEvent::ContainerOpen,
        GameEvent::Drink,
        GameEvent::Eat,
        GameEvent::ElytraGlide,
        GameEvent::EntityDamage,
        GameEvent::EntityDie,
        GameEvent::EntityDismount,
        GameEvent::EntityInteract,
        GameEvent::EntityMount,
        GameEvent::EntityPlace,
        GameEvent::EntityAction,
        GameEvent::Equip,
        GameEvent::Explode,
        GameEvent::Flap,
        GameEvent::FluidPickup,
        GameEvent::FluidPlace,
        GameEvent::HitGround,
        GameEvent::InstrumentPlay,
        GameEvent::ItemInteractFinish,
        GameEvent::ItemInteractStart,
        GameEvent::JukeboxPlay,
        GameEvent::JukeboxStopPlay,
        GameEvent::LightningStrike,
        GameEvent::NoteBlockPlay,
        GameEvent::PrimeFuse,
        GameEvent::ProjectileLand,
        GameEvent::ProjectileShoot,
        GameEvent::SculkSensorTendrilsClicking,
        GameEvent::Shear,
        GameEvent::Shriek,
        GameEvent::Splash,
        GameEvent::Step,
        GameEvent::Swim,
        GameEvent::Teleport,
        GameEvent::Unequip,
    ];

    /// The registry name without its namespace.
    pub fn name(self) -> String {
        let s = match self {
            GameEvent::BlockActivate => "block_activate",
            GameEvent::BlockAttach => "block_attach",
            GameEvent::BlockChange => "block_change",
            GameEvent::BlockClose => "block_close",
            GameEvent::BlockDeactivate => "block_deactivate",
            GameEvent::BlockDestroy => "block_destroy",
            GameEvent::BlockDetach => "block_detach",
            GameEvent::BlockOpen => "block_open",
            GameEvent::BlockPlace => "block_place",
            GameEvent::Bounce => "bounce",
            GameEvent::ContainerClose => "container_close",
            GameEvent::ContainerOpen => "container_open",
            GameEvent::Drink => "drink",
            GameEvent::Eat => "eat",
            GameEvent::ElytraGlide => "elytra_glide",
            GameEvent::EntityDamage => "entity_damage",
            GameEvent::EntityDie => "entity_die",
            GameEvent::EntityDismount => "entity_dismount",
            GameEvent::EntityInteract => "entity_interact",
            GameEvent::EntityMount => "entity_mount",
            GameEvent::EntityPlace => "entity_place",
            GameEvent::EntityAction => "entity_action",
            GameEvent::Equip => "equip",
            GameEvent::Explode => "explode",
            GameEvent::Flap => "flap",
            GameEvent::FluidPickup => "fluid_pickup",
            GameEvent::FluidPlace => "fluid_place",
            GameEvent::HitGround => "hit_ground",
            GameEvent::InstrumentPlay => "instrument_play",
            GameEvent::ItemInteractFinish => "item_interact_finish",
            GameEvent::ItemInteractStart => "item_interact_start",
            GameEvent::JukeboxPlay => "jukebox_play",
            GameEvent::JukeboxStopPlay => "jukebox_stop_play",
            GameEvent::LightningStrike => "lightning_strike",
            GameEvent::NoteBlockPlay => "note_block_play",
            GameEvent::PrimeFuse => "prime_fuse",
            GameEvent::ProjectileLand => "projectile_land",
            GameEvent::ProjectileShoot => "projectile_shoot",
            GameEvent::SculkSensorTendrilsClicking => "sculk_sensor_tendrils_clicking",
            GameEvent::Shear => "shear",
            GameEvent::Shriek => "shriek",
            GameEvent::Splash => "splash",
            GameEvent::Step => "step",
            GameEvent::Swim => "swim",
            GameEvent::Teleport => "teleport",
            GameEvent::Unequip => "unequip",
            GameEvent::Resonate(n) => return format!("resonate_{n}"),
        };
        s.to_string()
    }

    /// Looks an event up by registry name (with or without `minecraft:`).
    pub fn from_name(name: &str) -> Option<GameEvent> {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        if let Some(n) = name.strip_prefix("resonate_").and_then(|n| n.parse::<u8>().ok()) {
            return (1..=15).contains(&n).then_some(GameEvent::Resonate(n));
        }
        GameEvent::PLAIN.into_iter().find(|e| e.name() == name)
    }

    /// `VibrationSystem.VIBRATION_FREQUENCY_FOR_EVENT` (0: not a vibration
    /// frequency; sensors ignore it).
    pub const fn frequency(self) -> u8 {
        match self {
            GameEvent::Step | GameEvent::Swim | GameEvent::Flap => 1,
            GameEvent::ProjectileLand | GameEvent::HitGround | GameEvent::Splash | GameEvent::Bounce => 2,
            GameEvent::ItemInteractFinish | GameEvent::ProjectileShoot | GameEvent::InstrumentPlay => 3,
            GameEvent::EntityAction | GameEvent::ElytraGlide | GameEvent::Unequip => 4,
            GameEvent::EntityDismount | GameEvent::Equip => 5,
            GameEvent::EntityInteract | GameEvent::Shear | GameEvent::EntityMount => 6,
            GameEvent::EntityDamage => 7,
            GameEvent::Drink | GameEvent::Eat => 8,
            GameEvent::ContainerClose | GameEvent::BlockClose | GameEvent::BlockDeactivate | GameEvent::BlockDetach => {
                9
            }
            GameEvent::ContainerOpen
            | GameEvent::BlockOpen
            | GameEvent::BlockActivate
            | GameEvent::BlockAttach
            | GameEvent::PrimeFuse
            | GameEvent::NoteBlockPlay => 10,
            GameEvent::BlockChange => 11,
            GameEvent::BlockDestroy | GameEvent::FluidPickup => 12,
            GameEvent::BlockPlace | GameEvent::FluidPlace => 13,
            GameEvent::EntityPlace | GameEvent::LightningStrike | GameEvent::Teleport => 14,
            GameEvent::EntityDie | GameEvent::Explode => 15,
            GameEvent::Resonate(n) => n,
            _ => 0,
        }
    }

    /// `GameEvent.notificationRadius`: 16, except the jukebox's 10 and the
    /// shriek's 32.
    pub const fn radius(self) -> i32 {
        match self {
            GameEvent::JukeboxPlay | GameEvent::JukeboxStopPlay => 10,
            GameEvent::Shriek => 32,
            _ => 16,
        }
    }

    /// The `#minecraft:vibrations` tag, which sculk sensors hear.
    pub const fn is_vibration(self) -> bool {
        !matches!(
            self,
            GameEvent::ItemInteractStart
                | GameEvent::JukeboxPlay
                | GameEvent::JukeboxStopPlay
                | GameEvent::SculkSensorTendrilsClicking
                | GameEvent::Shriek
        )
    }

    /// `#minecraft:warden_can_listen`: the vibrations without `flap`, plus
    /// `shriek` and the sensor's tendril clicks.
    pub const fn warden_listens(self) -> bool {
        match self {
            GameEvent::Flap => false,
            GameEvent::Shriek | GameEvent::SculkSensorTendrilsClicking => true,
            e => e.is_vibration(),
        }
    }

    /// `#minecraft:shrieker_can_listen`.
    pub const fn shrieker_listens(self) -> bool {
        matches!(self, GameEvent::SculkSensorTendrilsClicking)
    }

    /// `#minecraft:ignore_vibrations_sneaking`.
    pub const fn sneaking_ignored(self) -> bool {
        matches!(
            self,
            GameEvent::HitGround
                | GameEvent::ProjectileShoot
                | GameEvent::Step
                | GameEvent::Swim
                | GameEvent::ItemInteractStart
                | GameEvent::ItemInteractFinish
        )
    }
}

/// An entity a game event or vibration names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntityRef {
    Player(PlayerId),
    /// A mob by its `Mob::uid`.
    Mob(u32),
}

impl EntityRef {
    fn save(self) -> Value {
        match self {
            EntityRef::Player(p) => json!(["player", p.0]),
            EntityRef::Mob(uid) => json!(["mob", uid]),
        }
    }

    fn load(v: &Value) -> Option<Self> {
        let id = u32::try_from(v[1].as_u64()?).ok()?;
        match v[0].as_str()? {
            "player" => Some(EntityRef::Player(PlayerId(id))),
            "mob" => Some(EntityRef::Mob(id)),
            _ => None,
        }
    }
}

/// `GameEvent.Context`'s source entity, reduced to the facts listeners read.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EventSource {
    /// The entity behind the event (Java `sourceEntity`).
    pub entity: Option<EntityRef>,
    /// The player `SculkShriekerBlockEntity.tryGetPlayer` finds: the
    /// entity itself, a vehicle's controlling player, a projectile's or a
    /// dropped item's owner.
    pub player: Option<PlayerId>,
    /// A projectile's owner (`VibrationInfo.projectileOwner`).
    pub projectile_owner: Option<EntityRef>,
    /// `Entity.isSteppingCarefully` (a player holding sneak).
    pub sneaking: bool,
    pub spectator: bool,
    /// `Entity.dampensVibrations`: wardens, and dropped wool.
    pub dampens: bool,
    /// The source is a living entity a warden may not target
    /// (`Warden.canTargetEntity` false: creative players, other wardens,
    /// dead entities), so wardens ignore its vibrations.
    pub untargetable: bool,
}

impl EventSource {
    /// No source entity (redstone, a falling stalactite).
    pub const NONE: EventSource = EventSource {
        entity: None,
        player: None,
        projectile_owner: None,
        sneaking: false,
        spectator: false,
        dampens: false,
        untargetable: false,
    };

    /// A player's own action.
    pub fn player(id: PlayerId, sneaking: bool) -> Self {
        Self { entity: Some(EntityRef::Player(id)), player: Some(id), sneaking, ..Self::NONE }
    }

    /// A mob's own action.
    pub fn mob(uid: u32) -> Self {
        Self { entity: Some(EntityRef::Mob(uid)), ..Self::NONE }
    }

    /// A projectile (not itself tracked) shot by `owner`.
    pub fn projectile(owner: Option<EntityRef>) -> Self {
        let player = match owner {
            Some(EntityRef::Player(p)) => Some(p),
            _ => None,
        };
        Self { projectile_owner: owner, player, ..Self::NONE }
    }

    fn save(&self) -> Value {
        json!({"entity":self.entity.map(EntityRef::save),"player":self.player.map(|p|p.0),
            "owner":self.projectile_owner.map(EntityRef::save)})
    }

    fn load(v: &Value) -> Self {
        Self {
            entity: EntityRef::load(&v["entity"]),
            player: v["player"].as_u64().and_then(|p| u32::try_from(p).ok()).map(PlayerId),
            projectile_owner: EntityRef::load(&v["owner"]),
            ..Self::NONE
        }
    }
}

/// One posted event, as entity listeners receive it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Posted {
    pub event: GameEvent,
    pub pos: DVec3,
    pub source: EventSource,
    /// `GameEvent.Context.affectedState`.
    pub affected: Option<Block>,
    /// The game tick it was posted on.
    pub tick: u64,
}

/// `VibrationInfo`: a vibration on its way to a listener.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VibrationInfo {
    pub event: GameEvent,
    /// Exact distance from the event to the listener.
    pub distance: f32,
    pub pos: DVec3,
    pub source: EventSource,
}

impl VibrationInfo {
    fn save(&self) -> Value {
        json!({"event":self.event.name(),"distance":self.distance,"pos":self.pos.to_array(),"source":self.source.save()})
    }

    fn load(v: &Value) -> Option<Self> {
        let pos: [f64; 3] = serde_json::from_value(v["pos"].clone()).ok()?;
        let pos = DVec3::from_array(pos);
        Some(Self {
            event: GameEvent::from_name(v["event"].as_str()?)?,
            distance: v["distance"].as_f64().filter(|d| d.is_finite() && *d >= 0.0)? as f32,
            pos: pos.is_finite().then_some(pos)?,
            source: EventSource::load(&v["source"]),
        })
    }
}

/// `VibrationSystem.Data` with its `VibrationSelector`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VibrationData {
    /// The vibration travelling to the listener.
    pub current: Option<VibrationInfo>,
    /// Ticks left until it arrives (`event_delay`).
    pub travel: u32,
    /// The candidate chosen so far and the tick it was heard on.
    pub selector: Option<(VibrationInfo, u64)>,
}

impl VibrationData {
    pub fn is_idle(&self) -> bool {
        self.current.is_none() && self.selector.is_none()
    }

    /// `VibrationSelector.addCandidate`: the first candidate of a tick, or a
    /// nearer one heard on the same tick, or an equally near one of higher
    /// frequency.
    pub fn add_candidate(&mut self, info: VibrationInfo, tick: u64) {
        let replace = match &self.selector {
            None => true,
            Some((old, at)) => {
                *at == tick
                    && (info.distance < old.distance
                        || info.distance == old.distance && info.event.frequency() > old.event.frequency())
            }
        };
        if replace {
            self.selector = Some((info, tick));
        }
    }

    /// `VibrationSelector.chosenCandidate`: a candidate from an earlier tick.
    pub fn chosen(&self, tick: u64) -> Option<VibrationInfo> {
        self.selector.filter(|(_, at)| *at < tick).map(|(i, _)| i)
    }

    /// `VibrationSystem.Ticker.tick` up to delivery: picks a candidate (its
    /// travel time from `travel_time`), counts the travel down and returns
    /// the vibration once it arrives. `deliverable` is
    /// `requiresAdjacentChunksToBeTicking`'s check; while it fails the
    /// vibration waits.
    pub fn tick(&mut self, tick: u64, travel_time: impl Fn(f32) -> u32, deliverable: bool) -> Option<VibrationInfo> {
        if self.current.is_none()
            && let Some(info) = self.chosen(tick)
        {
            self.current = Some(info);
            self.travel = travel_time(info.distance);
            self.selector = None;
        }
        let info = self.current?;
        self.travel = self.travel.saturating_sub(1);
        if self.travel == 0 && deliverable {
            self.current = None;
            return Some(info);
        }
        None
    }

    pub fn save(&self) -> Value {
        json!({"event":self.current.map(|i|i.save()),"event_delay":self.travel,
            "selector":self.selector.map(|(i,t)|json!({"event":i.save(),"tick":t}))})
    }

    pub fn load(v: &Value) -> Self {
        Self {
            current: VibrationInfo::load(&v["event"]),
            travel: v["event_delay"].as_u64().unwrap_or(0).min(64) as u32,
            selector: VibrationInfo::load(&v["selector"]["event"]).zip(v["selector"]["tick"].as_u64()),
        }
    }
}

/// `VibrationSystem.getRedstoneStrengthForDistance`.
pub fn redstone_strength(distance: f32, radius: i32) -> u8 {
    let scale = 15.0 / f64::from(radius);
    (15 - (scale * f64::from(distance)).floor() as i32).clamp(1, 15) as u8
}

/// `VibrationSystem.Listener.distanceBetweenInBlocks` between the cells
/// holding two positions.
pub fn block_distance(a: DVec3, b: DVec3) -> f32 {
    let d = (a.floor() - b.floor()).as_ivec3();
    (d.length_squared() as f32).sqrt()
}

/// Wool (`#minecraft:occludes_vibration_signals`).
pub fn occludes(b: Block) -> bool {
    b.wool_color().is_some()
}

/// `#minecraft:dampens_vibrations`: wool and carpets (VoxelCraft has no
/// wool slabs or stairs).
pub fn dampens(b: Block) -> bool {
    let b = b.canonical();
    b.wool_color().is_some() || b.carpet_color().is_some()
}

/// `VibrationSystem.Listener.isOccluded`: rays between the two cell
/// centres, each nudged 1e-5 toward one of the six faces; occluded only
/// when every ray meets wool.
pub fn occluded(get: impl Fn(IVec3) -> Option<Block>, from: DVec3, to: DVec3) -> bool {
    let from = from.floor() + DVec3::splat(0.5);
    let to = to.floor() + DVec3::splat(0.5);
    [DVec3::NEG_Y, DVec3::Y, DVec3::NEG_Z, DVec3::Z, DVec3::NEG_X, DVec3::X]
        .into_iter()
        .all(|side| wool_in_line(&get, from + side * f64::from(1e-5_f32), to))
}

/// `Level.isBlockInLine` with an occluding-block predicate: a voxel walk
/// from `from` to `to`. A listener is at most 16 blocks away, so the walk
/// is bounded.
fn wool_in_line(get: &impl Fn(IVec3) -> Option<Block>, from: DVec3, to: DVec3) -> bool {
    let delta = to - from;
    let step = IVec3::from_array(delta.to_array().map(|v| {
        if v > 0. {
            1
        } else if v < 0. {
            -1
        } else {
            0
        }
    }));
    let inv = DVec3::from_array(delta.to_array().map(|v| if v == 0. { f64::INFINITY } else { 1. / v.abs() }));
    let mut cell = from.floor().as_ivec3();
    let end = to.floor().as_ivec3();
    let mut next = DVec3::ZERO;
    for axis in 0..3 {
        next[axis] =
            if step[axis] > 0 { f64::from(cell[axis] + 1) - from[axis] } else { from[axis] - f64::from(cell[axis]) }
                * inv[axis];
    }
    for _ in 0..200 {
        if get(cell).is_some_and(occludes) {
            return true;
        }
        if cell == end {
            return false;
        }
        let axis = if next.x < next.y && next.x < next.z {
            0
        } else if next.y < next.z {
            1
        } else {
            2
        };
        cell[axis] += step[axis];
        next[axis] += inv[axis];
    }
    false
}

/// What a listener accepts (`VibrationSystem.User.isValidVibration`): the
/// event in its tag, a source that isn't a spectator, isn't stepping
/// carefully for the sneak-ignored events and doesn't dampen vibrations,
/// and an affected block that doesn't dampen them.
pub fn valid_vibration(
    event: GameEvent,
    source: &EventSource,
    affected: Option<Block>,
    listens: impl Fn(GameEvent) -> bool,
) -> bool {
    if !listens(event) {
        return false;
    }
    if source.entity.is_some() && (source.spectator || source.sneaking && event.sneaking_ignored() || source.dampens) {
        return false;
    }
    !affected.is_some_and(dampens)
}

/// Posts `event` at `pos` in `world` (Java `Level.gameEvent`). The public
/// entry point for other systems; see [`World::emit_game_event`].
pub fn emit_game_event(world: &mut World, pos: DVec3, event: GameEvent, source: EventSource) {
    world.emit_game_event(pos, event, source, None);
}

impl World {
    /// Posts `event` at `pos` (Java `GameEventDispatcher.post`): every
    /// block listener in range considers it now, and entity listeners
    /// (wardens) on their next tick. `affected` is the block the event
    /// concerns (`GameEvent.Context.affectedState`); vibrations off wool
    /// and carpets are dampened.
    pub fn emit_game_event(&mut self, pos: DVec3, event: GameEvent, source: EventSource, affected: Option<Block>) {
        if self.remote || !pos.is_finite() {
            return;
        }
        let tick = self.redstone.tick;
        if self.sculk.entity_listeners && self.sculk.entity_events.len() < super::sculk::MAX_ENTITY_EVENTS {
            self.sculk.entity_events.push(Posted { event, pos, source, affected, tick });
        }
        self.sculk_hear(pos, event, source, affected, tick);
    }

    /// Events posted since the entity simulation last asked, for entity
    /// listeners. Entity simulations set [`World::set_entity_listeners`]
    /// when they have any; otherwise nothing is kept.
    pub fn take_entity_game_events(&mut self) -> Vec<Posted> {
        std::mem::take(&mut self.sculk.entity_events)
    }

    /// Whether any entity listens (a warden is loaded), so posted events
    /// are kept for [`World::take_entity_game_events`].
    pub fn set_entity_listeners(&mut self, any: bool) {
        self.sculk.entity_listeners = any;
        if !any {
            self.sculk.entity_events.clear();
        }
    }

    /// The game tick the vibration system counts in.
    pub fn game_tick(&self) -> u64 {
        self.redstone.tick
    }

    /// Whether wool occludes every ray from `from` to `to`.
    pub fn vibration_occluded(&self, from: DVec3, to: DVec3) -> bool {
        occluded(|p| self.get_block(p), from, to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequencies_match_java_table() {
        assert_eq!(GameEvent::Step.frequency(), 1);
        assert_eq!(GameEvent::HitGround.frequency(), 2);
        assert_eq!(GameEvent::ProjectileShoot.frequency(), 3);
        assert_eq!(GameEvent::Equip.frequency(), 5);
        assert_eq!(GameEvent::EntityDamage.frequency(), 7);
        assert_eq!(GameEvent::Eat.frequency(), 8);
        assert_eq!(GameEvent::ContainerClose.frequency(), 9);
        assert_eq!(GameEvent::NoteBlockPlay.frequency(), 10);
        assert_eq!(GameEvent::BlockChange.frequency(), 11);
        assert_eq!(GameEvent::BlockDestroy.frequency(), 12);
        assert_eq!(GameEvent::BlockPlace.frequency(), 13);
        assert_eq!(GameEvent::Teleport.frequency(), 14);
        assert_eq!(GameEvent::Explode.frequency(), 15);
        assert_eq!(GameEvent::Resonate(9).frequency(), 9);
        assert_eq!(GameEvent::Shriek.frequency(), 0);
        assert_eq!(GameEvent::ItemInteractStart.frequency(), 0);
        for e in GameEvent::PLAIN {
            assert_eq!(GameEvent::from_name(&e.name()), Some(e));
        }
        assert_eq!(GameEvent::from_name("minecraft:resonate_15"), Some(GameEvent::Resonate(15)));
        assert_eq!(GameEvent::from_name("resonate_16"), None);
    }

    #[test]
    fn tags_match_java_data() {
        assert!(GameEvent::Flap.is_vibration() && !GameEvent::Flap.warden_listens());
        assert!(GameEvent::Shriek.warden_listens() && !GameEvent::Shriek.is_vibration());
        assert!(GameEvent::SculkSensorTendrilsClicking.shrieker_listens());
        assert!(!GameEvent::Step.shrieker_listens());
        assert!(GameEvent::Step.sneaking_ignored() && !GameEvent::BlockPlace.sneaking_ignored());
        assert_eq!(GameEvent::Shriek.radius(), 32);
        assert_eq!(GameEvent::JukeboxPlay.radius(), 10);
    }

    #[test]
    fn selector_prefers_nearer_then_higher_frequency_within_a_tick() {
        let info = |event, distance| VibrationInfo { event, distance, pos: DVec3::ZERO, source: EventSource::NONE };
        let mut d = VibrationData::default();
        d.add_candidate(info(GameEvent::Step, 5.0), 10);
        d.add_candidate(info(GameEvent::BlockPlace, 6.0), 10);
        assert_eq!(d.selector.unwrap().0.event, GameEvent::Step);
        d.add_candidate(info(GameEvent::Explode, 5.0), 10);
        assert_eq!(d.selector.unwrap().0.event, GameEvent::Explode);
        d.add_candidate(info(GameEvent::Step, 1.0), 11);
        assert_eq!(d.selector.unwrap().0.event, GameEvent::Explode, "a later tick never replaces");
        assert!(d.chosen(10).is_none(), "chosen only on a later tick");
        // Travel: floor(5.0) = 5 ticks, delivered on the fifth tick call.
        let mut got = None;
        for t in 11..20 {
            if let Some(i) = d.tick(t, |dist| dist.floor() as u32, true) {
                got = Some((t, i));
                break;
            }
        }
        assert_eq!(got.map(|(t, i)| (t, i.event)), Some((15, GameEvent::Explode)));
        assert!(d.is_idle());
    }

    #[test]
    fn redstone_strength_by_distance() {
        assert_eq!(redstone_strength(0.0, 8), 15);
        assert_eq!(redstone_strength(1.0, 8), 14);
        assert_eq!(redstone_strength(8.0, 8), 1);
        assert_eq!(redstone_strength(16.0, 16), 1);
        assert_eq!(redstone_strength(4.0, 16), 12);
    }

    #[test]
    fn sneaking_spectators_dampening_and_wool() {
        let p = EventSource::player(PlayerId(3), true);
        assert!(!valid_vibration(GameEvent::Step, &p, None, GameEvent::is_vibration));
        assert!(valid_vibration(GameEvent::BlockPlace, &p, None, GameEvent::is_vibration));
        assert!(!valid_vibration(GameEvent::BlockPlace, &p, Some(Block::WOOL), GameEvent::is_vibration));
        let spectator = EventSource { spectator: true, ..EventSource::player(PlayerId(3), false) };
        assert!(!valid_vibration(GameEvent::BlockPlace, &spectator, None, GameEvent::is_vibration));
        let warden = EventSource { dampens: true, ..EventSource::mob(9) };
        assert!(!valid_vibration(GameEvent::Step, &warden, None, GameEvent::is_vibration));
        let wall = |p: IVec3| {
            Some(if p.x == 2 && (-3..=3).contains(&p.y) && (-3..=3).contains(&p.z) { Block::WOOL } else { Block::AIR })
        };
        assert!(occluded(wall, DVec3::new(0.5, 0.5, 0.5), DVec3::new(5.5, 0.5, 0.5)));
        assert!(!occluded(wall, DVec3::new(0.5, 0.5, 0.5), DVec3::new(0.5, 0.5, 5.5)));
    }

    #[test]
    fn data_round_trips() {
        let mut d = VibrationData::default();
        let source = EventSource::player(PlayerId(4), false);
        d.add_candidate(
            VibrationInfo { event: GameEvent::Resonate(3), distance: 2.5, pos: DVec3::new(1.5, 2.0, 3.0), source },
            40,
        );
        d.current =
            Some(VibrationInfo { event: GameEvent::Eat, distance: 7.0, pos: DVec3::ONE, source: EventSource::mob(5) });
        d.travel = 4;
        let back = VibrationData::load(&d.save());
        assert_eq!(back.travel, 4);
        assert_eq!(back.current.unwrap().event, GameEvent::Eat);
        assert_eq!(back.current.unwrap().source.entity, Some(EntityRef::Mob(5)));
        assert_eq!(back.selector.unwrap().0.source.player, Some(PlayerId(4)));
        assert_eq!(back.selector.unwrap().1, 40);
    }
}
