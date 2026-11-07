//! Sculk block entities (MOB-04, v0.16 track A): sculk sensors, calibrated
//! sculk sensors, sculk shriekers and sculk catalysts, after Java 26.3's
//! `SculkSensorBlock(Entity)`, `CalibratedSculkSensorBlock(Entity)`,
//! `SculkShriekerBlock(Entity)`, `SculkCatalystBlock(Entity)` and
//! `WardenSpawnTracker`.
//!
//! Each listener cell keeps a [`SculkEntity`] (its vibration data, a
//! sensor's power and last frequency, a shrieker's warning level, a
//! catalyst's charge cursors), saved with the dimension and kept across
//! chunk unloads. Loaded listeners are indexed by chunk; only listeners with
//! work (a vibration on its way, charge to spread) tick.
//!
//! Shrieker warnings need every nearby player's [`WardenSpawnTracker`], and
//! summoning needs the entity simulation, so the world queues them:
//! [`World::resolve_shrieks`] (the game, with its players) and
//! [`World::take_shrieker_responses`] (the entity simulation).
//!
//! Differences from Java are listed in
//! `docs/verification/2026-11-06-v0.16-sculk-warden.md`.

use std::collections::{BTreeMap, BTreeSet};

use glam::{DVec3, IVec3};
use rustc_hash::{FxHashMap, FxHashSet};
use serde_json::{Value, json};

use super::World;
use super::block::Block;
use super::cave_blocks::{self as cb, Phase};
use super::chunk::{CHUNK_SIZE, CHUNK_SIZE_I, ChunkData, chunk_of};
use super::sculk_spread::Cursor;
use super::vibration::{
    EventSource, GameEvent, VibrationData, VibrationInfo, block_distance, occluded, redstone_strength, valid_vibration,
};
use crate::entity::PlayerId;

/// Posted events kept for entity listeners between two entity ticks.
pub(crate) const MAX_ENTITY_EVENTS: usize = 512;
/// `SculkSensorBlock.ACTIVE_TICKS` and the calibrated sensor's override.
pub const SENSOR_ACTIVE_TICKS: u64 = 30;
pub const CALIBRATED_ACTIVE_TICKS: u64 = 10;
/// `SculkSensorBlock.COOLDOWN_TICKS`.
pub const SENSOR_COOLDOWN_TICKS: u64 = 10;
/// `SculkShriekerBlockEntity.SHRIEKING_TICKS`.
pub const SHRIEKING_TICKS: u64 = 90;
/// `SculkCatalystBlockEntity.CatalystListener.PULSE_TICKS`.
pub const BLOOM_TICKS: u64 = 8;
/// Listener radii: sensor and shrieker 8, calibrated sensor 16, catalyst 8.
pub const SENSOR_RADIUS: i32 = 8;
pub const CALIBRATED_RADIUS: i32 = 16;
pub const SHRIEKER_RADIUS: i32 = 8;
pub const CATALYST_RADIUS: i32 = 8;

/// The kind of sculk listener a block is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListenerKind {
    Sensor,
    Calibrated,
    Shrieker,
    Catalyst,
}

impl ListenerKind {
    pub const fn radius(self) -> i32 {
        match self {
            ListenerKind::Sensor => SENSOR_RADIUS,
            ListenerKind::Calibrated => CALIBRATED_RADIUS,
            ListenerKind::Shrieker => SHRIEKER_RADIUS,
            ListenerKind::Catalyst => CATALYST_RADIUS,
        }
    }
}

/// Which listener `b` is, if any.
pub fn listener_kind(b: Block) -> Option<ListenerKind> {
    if cb::sensor_of(b).is_some() {
        Some(ListenerKind::Sensor)
    } else if cb::calibrated_of(b).is_some() {
        Some(ListenerKind::Calibrated)
    } else if cb::shrieker_of(b).is_some() {
        Some(ListenerKind::Shrieker)
    } else if cb::catalyst_bloom(b).is_some() {
        Some(ListenerKind::Catalyst)
    } else {
        None
    }
}

/// One listener's block-entity data.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SculkEntity {
    /// Sensors and shriekers: the `listener` compound.
    pub vibration: VibrationData,
    /// Sensors: redstone power (Java's `power` state property).
    pub power: u8,
    /// Sensors: `last_vibration_frequency`, read by comparators while active.
    pub last_frequency: u8,
    /// Shriekers: `warning_level` of the last shriek.
    pub warning: u8,
    /// Catalysts: the `SculkSpreader` charge cursors (at most 32).
    pub cursors: Vec<Cursor>,
}

impl SculkEntity {
    fn idle(&self) -> bool {
        self.vibration.is_idle() && self.cursors.is_empty()
    }

    fn save(&self) -> Value {
        json!({"listener":self.vibration.save(),"power":self.power,"last_vibration_frequency":self.last_frequency,
            "warning_level":self.warning,"cursors":self.cursors.iter().map(Cursor::save).collect::<Vec<_>>()})
    }

    fn load(v: &Value) -> Self {
        Self {
            vibration: VibrationData::load(&v["listener"]),
            power: v["power"].as_u64().unwrap_or(0).min(15) as u8,
            last_frequency: v["last_vibration_frequency"].as_u64().unwrap_or(0).min(15) as u8,
            warning: v["warning_level"].as_u64().unwrap_or(0).min(4) as u8,
            cursors: v["cursors"]
                .as_array()
                .map_or(Vec::new(), |a| a.iter().filter_map(Cursor::load).take(32).collect()),
        }
    }
}

/// A sound a sculk block makes, for the game to play.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SculkSound {
    /// `SCULK_CLICKING` (pitch 0.8–1.0).
    Clicking,
    /// `SCULK_CLICKING_STOP`.
    ClickingStop,
    /// `AMETHYST_BLOCK_RESONATE` at the frequency's note pitch.
    Resonate(f32),
    /// `SCULK_CATALYST_BLOOM` (volume 2, pitch 0.6–1.0).
    Bloom,
    /// `SCULK_BLOCK_SPREAD`.
    Spread,
    /// A sensor or shrieker grown by a catalyst (its place sound).
    Grow,
    /// Level event 3007: `SCULK_SHRIEKER_SHRIEK`.
    Shriek,
    /// The warden's distant reply to a shriek that summoned nothing
    /// (`WARDEN_NEARBY_CLOSE`, `_CLOSER`, `_CLOSEST`, `WARDEN_LISTENING_ANGRY`
    /// for warning levels 1 to 4), volume 5.
    WardenReply(u8),
}

/// The synthesized sounds of the sculk blocks ([`SculkSound`] without its
/// pitch or level).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SculkCue {
    Clicking,
    ClickingStop,
    Resonate,
    Bloom,
    Spread,
    Grow,
    Shriek,
}

impl SculkCue {
    pub const ALL: [SculkCue; 7] = [
        SculkCue::Clicking,
        SculkCue::ClickingStop,
        SculkCue::Resonate,
        SculkCue::Bloom,
        SculkCue::Spread,
        SculkCue::Grow,
        SculkCue::Shriek,
    ];
}

/// A shrieker that finished shrieking and may answer (Java
/// `SculkShriekerBlockEntity.tryRespond`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShriekerResponse {
    pub pos: IVec3,
    pub warning: u8,
}

/// The world's sculk state.
#[derive(Default)]
pub(crate) struct SculkState {
    pub(crate) entities: BTreeMap<[i32; 3], SculkEntity>,
    /// Loaded listener cells by chunk.
    index: FxHashMap<IVec3, Vec<IVec3>>,
    /// Listeners with work to tick.
    busy: BTreeSet<[i32; 3]>,
    /// Loaded catalysts.
    catalysts: FxHashSet<IVec3>,
    pub(crate) entity_events: Vec<super::vibration::Posted>,
    pub(crate) entity_listeners: bool,
    pub(crate) sounds: Vec<(DVec3, SculkSound)>,
    shriek_triggers: Vec<(IVec3, PlayerId)>,
    responses: Vec<ShriekerResponse>,
    /// Each player's step bookkeeping (Java's `moveDist`, `nextStep` and
    /// the fall in progress), not saved.
    players: FxHashMap<PlayerId, (f32, f32, Option<f64>)>,
}

/// `SculkSensorBlock.RESONANCE_PITCH_BEND`: `NoteBlock.getPitchFromNote`
/// of `{0, 0, 2, 4, 6, 7, 9, 10, 12, 14, 15, 18, 19, 21, 22, 24}`.
pub fn resonance_pitch(frequency: u8) -> f32 {
    const NOTES: [i32; 16] = [0, 0, 2, 4, 6, 7, 9, 10, 12, 14, 15, 18, 19, 21, 22, 24];
    2f32.powf((NOTES[usize::from(frequency.min(15))] - 12) as f32 / 12.0)
}

fn center(p: IVec3) -> DVec3 {
    p.as_dvec3() + DVec3::splat(0.5)
}

/// Java `WardenSpawnTracker`: a player's warning level (0–4), the ticks
/// since the last warning (a level drops every 12000) and the 200-tick
/// cooldown after each warning. Saved with the player.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WardenSpawnTracker {
    pub ticks_since_last_warning: u32,
    pub warning_level: u8,
    pub cooldown_ticks: u32,
}

impl WardenSpawnTracker {
    pub const MAX_WARNING_LEVEL: u8 = 4;

    /// One game tick (`ServerPlayer.tick`).
    pub fn tick(&mut self) {
        if self.ticks_since_last_warning >= 12000 {
            self.warning_level = self.warning_level.saturating_sub(1);
            self.ticks_since_last_warning = 0;
        } else {
            self.ticks_since_last_warning += 1;
        }
        self.cooldown_ticks = self.cooldown_ticks.saturating_sub(1);
    }

    pub fn on_cooldown(&self) -> bool {
        self.cooldown_ticks > 0
    }

    fn increase(&mut self) {
        if !self.on_cooldown() {
            self.ticks_since_last_warning = 0;
            self.cooldown_ticks = 200;
            self.warning_level = (self.warning_level + 1).min(Self::MAX_WARNING_LEVEL);
        }
    }

    pub fn save(&self) -> Value {
        json!({"ticks_since_last_warning":self.ticks_since_last_warning,"warning_level":self.warning_level,
            "cooldown_ticks":self.cooldown_ticks})
    }

    pub fn load(v: &Value) -> Self {
        Self {
            ticks_since_last_warning: v["ticks_since_last_warning"].as_u64().unwrap_or(0).min(12000) as u32,
            warning_level: v["warning_level"].as_u64().unwrap_or(0).min(4) as u8,
            cooldown_ticks: v["cooldown_ticks"].as_u64().unwrap_or(0).min(200) as u32,
        }
    }
}

/// A player as shrieker warnings see them.
pub struct ShriekPlayer<'a> {
    pub id: PlayerId,
    pub pos: DVec3,
    pub alive: bool,
    pub spectator: bool,
    /// A creative player, whom wardens ignore.
    pub untargetable: bool,
    pub tracker: &'a mut WardenSpawnTracker,
}

impl World {
    /// Sounds sculk blocks made since the last call, for the game to play.
    pub fn take_sculk_sounds(&mut self) -> Vec<(DVec3, SculkSound)> {
        std::mem::take(&mut self.sculk.sounds)
    }

    /// Shriekers whose shriek ended with a warning level to answer.
    pub fn take_shrieker_responses(&mut self) -> Vec<ShriekerResponse> {
        std::mem::take(&mut self.sculk.responses)
    }

    /// The block entity data at `p`, if it is a listener.
    pub fn sculk_entity(&self, p: IVec3) -> Option<&SculkEntity> {
        self.sculk.entities.get(&p.to_array())
    }

    fn sculk_entity_mut(&mut self, p: IVec3) -> &mut SculkEntity {
        self.sculk.entities.entry(p.to_array()).or_default()
    }

    /// Records a listener added or removed by an edit.
    pub(super) fn track_sculk(&mut self, p: IVec3, old: Block, new: Block) {
        let (was, now) = (listener_kind(old), listener_kind(new));
        if was == now {
            return;
        }
        // `SculkShriekerBlockEntity.preRemoveSideEffects`: a shrieker
        // removed mid-shriek answers at once.
        if was == Some(ListenerKind::Shrieker)
            && cb::shrieker_of(old).is_some_and(|(shrieking, ..)| shrieking)
            && let Some(e) = self.sculk.entities.get(&p.to_array())
        {
            let warning = e.warning;
            if self.shrieker_can_respond(old) && warning > 0 {
                self.sculk.responses.push(ShriekerResponse { pos: p, warning });
            }
        }
        let c = chunk_of(p);
        self.sculk.catalysts.remove(&p);
        if now == Some(ListenerKind::Catalyst) {
            self.sculk.catalysts.insert(p);
        }
        if was.is_some() {
            self.sculk.entities.remove(&p.to_array());
            self.sculk.busy.remove(&p.to_array());
            if let Some(cells) = self.sculk.index.get_mut(&c) {
                cells.retain(|&q| q != p);
            }
        }
        if now.is_some() {
            self.sculk.entities.insert(p.to_array(), SculkEntity::default());
            self.sculk.index.entry(c).or_default().push(p);
        }
    }

    /// Indexes the listeners of a chunk that just loaded.
    pub(super) fn register_sculk(&mut self, pos: IVec3, data: &ChunkData) {
        self.sculk.index.remove(&pos);
        self.sculk.catalysts.retain(|q| chunk_of(*q) != pos);
        let any = match data {
            ChunkData::Uniform(b) => listener_kind(*b).is_some(),
            ChunkData::Bytes(_) => false,
            ChunkData::Paletted { palette, len, .. } => {
                palette[..*len as usize].iter().any(|&b| listener_kind(b).is_some())
            }
            ChunkData::Dense(v) => v.iter().any(|&b| listener_kind(b).is_some()),
        };
        if !any {
            return;
        }
        let mut cells = Vec::new();
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let kind = listener_kind(data.get(x, y, z));
                    if kind.is_some() {
                        let p = pos * CHUNK_SIZE_I + IVec3::new(x as i32, y as i32, z as i32);
                        if kind == Some(ListenerKind::Catalyst) {
                            self.sculk.catalysts.insert(p);
                        }
                        let e = self.sculk.entities.entry(p.to_array()).or_default();
                        if !e.idle() {
                            self.sculk.busy.insert(p.to_array());
                        }
                        cells.push(p);
                    }
                }
            }
        }
        self.sculk.index.insert(pos, cells);
    }

    /// Drops a chunk's listeners from the index; data with something in it
    /// stays for when the chunk loads again.
    pub(super) fn unload_sculk(&mut self, pos: IVec3) {
        self.sculk.catalysts.retain(|q| chunk_of(*q) != pos);
        if let Some(cells) = self.sculk.index.remove(&pos) {
            for p in cells {
                self.sculk.busy.remove(&p.to_array());
                if self.sculk.entities.get(&p.to_array()).is_some_and(|e| *e == SculkEntity::default()) {
                    self.sculk.entities.remove(&p.to_array());
                }
            }
        }
    }

    /// Delivers a posted event to the block listeners in range
    /// (`VibrationSystem.Listener.handleGameEvent`).
    pub(super) fn sculk_hear(
        &mut self,
        pos: DVec3,
        event: GameEvent,
        source: EventSource,
        affected: Option<Block>,
        tick: u64,
    ) {
        if self.sculk.index.is_empty() {
            return;
        }
        let cell = pos.floor().as_ivec3();
        let (lo, hi) =
            (chunk_of(cell - IVec3::splat(CALIBRATED_RADIUS)), chunk_of(cell + IVec3::splat(CALIBRATED_RADIUS)));
        let mut heard = Vec::new();
        for cx in lo.x..=hi.x {
            for cy in lo.y..=hi.y {
                for cz in lo.z..=hi.z {
                    if let Some(cells) = self.sculk.index.get(&IVec3::new(cx, cy, cz)) {
                        heard.extend(
                            cells
                                .iter()
                                .copied()
                                .filter(|q| (*q - cell).length_squared() <= CALIBRATED_RADIUS * CALIBRATED_RADIUS),
                        );
                    }
                }
            }
        }
        for q in heard {
            let Some(b) = self.get_block(q) else { continue };
            let Some(kind) = listener_kind(b) else { continue };
            if kind == ListenerKind::Catalyst || (q - cell).length_squared() > kind.radius() * kind.radius() {
                continue;
            }
            if self.sculk.entities.get(&q.to_array()).is_some_and(|e| e.vibration.current.is_some()) {
                continue;
            }
            let listens = |e: GameEvent| match kind {
                ListenerKind::Shrieker => e.shrieker_listens(),
                _ => e.is_vibration(),
            };
            if !valid_vibration(event, &source, affected, listens)
                || !self.sculk_can_receive(q, b, kind, cell, event, &source)
            {
                continue;
            }
            let dest = center(q);
            if occluded(|c| self.get_block(c), pos, dest) {
                continue;
            }
            let info = VibrationInfo { event, distance: pos.distance(dest) as f32, pos, source };
            self.sculk_entity_mut(q).vibration.add_candidate(info, tick);
            self.sculk.busy.insert(q.to_array());
        }
    }

    /// `VibrationSystem.User.canReceiveVibration` of each block listener.
    fn sculk_can_receive(
        &self,
        q: IVec3,
        b: Block,
        kind: ListenerKind,
        from: IVec3,
        event: GameEvent,
        source: &EventSource,
    ) -> bool {
        match kind {
            ListenerKind::Shrieker => {
                !cb::shrieker_of(b).is_some_and(|(shrieking, ..)| shrieking) && source.player.is_some()
            }
            ListenerKind::Calibrated => {
                let filter = self.calibrated_input(q, b);
                (filter == 0 || event.frequency() == filter) && self.sensor_can_receive(q, b, from, event)
            }
            ListenerKind::Sensor => self.sensor_can_receive(q, b, from, event),
            ListenerKind::Catalyst => false,
        }
    }

    fn sensor_can_receive(&self, q: IVec3, b: Block, from: IVec3, event: GameEvent) -> bool {
        if from == q && matches!(event, GameEvent::BlockDestroy | GameEvent::BlockPlace) {
            return false;
        }
        event.frequency() != 0 && cb::any_sensor(b).is_some_and(|(phase, ..)| phase == Phase::Inactive)
    }

    /// `CalibratedSculkSensorBlockEntity.VibrationUser.getBackSignal`: the
    /// redstone signal into the side opposite its facing.
    pub fn calibrated_input(&self, q: IVec3, b: Block) -> u8 {
        let Some((facing, ..)) = cb::calibrated_of(b) else { return 0 };
        let back = q - facing.offset();
        self.signal_from(back, facing.offset(), true)
    }

    /// One game tick of every listener with work.
    pub fn tick_sculk(&mut self) {
        if self.sculk.busy.is_empty() || self.remote {
            return;
        }
        let tick = self.redstone.tick;
        let busy: Vec<[i32; 3]> = self.sculk.busy.iter().copied().collect();
        for key in busy {
            let p = IVec3::from_array(key);
            if !self.is_block_ticking(p) {
                continue;
            }
            let Some(kind) = self.get_block(p).and_then(listener_kind) else {
                self.sculk.busy.remove(&key);
                continue;
            };
            if kind == ListenerKind::Catalyst {
                self.update_sculk_cursors(p);
            } else {
                let Some(mut e) = self.sculk.entities.remove(&key) else {
                    self.sculk.busy.remove(&key);
                    continue;
                };
                let arrived = e.vibration.tick(tick, |d| d.floor().max(0.0) as u32, true);
                self.sculk.entities.insert(key, e);
                if let Some(info) = arrived {
                    self.sculk_receive(p, kind, info);
                }
            }
            if self.sculk.entities.get(&key).is_none_or(SculkEntity::idle) {
                self.sculk.busy.remove(&key);
            }
        }
    }

    /// `VibrationSystem.User.onReceiveVibration` of the block listeners.
    fn sculk_receive(&mut self, p: IVec3, kind: ListenerKind, info: VibrationInfo) {
        let Some(b) = self.get_block(p) else { return };
        match kind {
            ListenerKind::Sensor | ListenerKind::Calibrated => {
                if !cb::any_sensor(b).is_some_and(|(phase, ..)| phase == Phase::Inactive) {
                    return;
                }
                let frequency = info.event.frequency();
                let distance = block_distance(info.pos, center(p));
                let power = redstone_strength(distance, kind.radius());
                self.activate_sensor(p, b, power, frequency, info.source);
            }
            ListenerKind::Shrieker => {
                let player = match info.source.projectile_owner {
                    Some(super::vibration::EntityRef::Player(owner)) => Some(owner),
                    _ => info.source.player,
                };
                if let Some(player) = player {
                    self.try_shriek(p, player);
                }
            }
            ListenerKind::Catalyst => {}
        }
    }

    /// `SculkSensorBlock.activate`.
    fn activate_sensor(&mut self, p: IVec3, b: Block, power: u8, frequency: u8, source: EventSource) {
        let Some((_, wet, calibrated)) = cb::any_sensor(b) else { return };
        {
            let e = self.sculk_entity_mut(p);
            e.last_frequency = frequency;
            e.power = power;
        }
        self.edit(p, cb::with_phase(b, Phase::Active), false);
        self.schedule_redstone(p, if calibrated { CALIBRATED_ACTIVE_TICKS } else { SENSOR_ACTIVE_TICKS }, 0);
        self.redstone_changed(p);
        self.redstone_changed(p - IVec3::Y);
        // `tryResonateVibration`: amethyst blocks beside it re-emit the frequency.
        if (1..=15).contains(&frequency) {
            for d in cb::DIRS {
                let q = p + d;
                if let Some(amethyst) = self.get_block(q).filter(|&n| n == super::overworld_blocks::AMETHYST_BLOCK) {
                    self.emit_game_event(center(q), GameEvent::Resonate(frequency), source, Some(amethyst));
                    self.sculk.sounds.push((center(q), SculkSound::Resonate(resonance_pitch(frequency))));
                }
            }
        }
        self.emit_game_event(center(p), GameEvent::SculkSensorTendrilsClicking, source, None);
        if !wet {
            self.sculk.sounds.push((center(p), SculkSound::Clicking));
        }
    }

    /// The scheduled tick of a sensor (`SculkSensorBlock.tick`), shrieker
    /// (`SculkShriekerBlock.tick`) or blooming catalyst
    /// (`SculkCatalystBlock.tick`).
    pub(super) fn sculk_scheduled_tick(&mut self, p: IVec3, b: Block) -> bool {
        if let Some((phase, wet, _)) = cb::any_sensor(b) {
            match phase {
                Phase::Active => {
                    // `deactivate`: cooldown, no power, wake the neighbours.
                    self.sculk_entity_mut(p).power = 0;
                    self.edit(p, cb::with_phase(b, Phase::Cooldown), false);
                    self.schedule_redstone(p, SENSOR_COOLDOWN_TICKS, 0);
                    self.redstone_changed(p);
                    self.redstone_changed(p - IVec3::Y);
                }
                Phase::Cooldown => {
                    self.edit(p, cb::with_phase(b, Phase::Inactive), false);
                    if !wet {
                        self.sculk.sounds.push((center(p), SculkSound::ClickingStop));
                    }
                }
                Phase::Inactive => {}
            }
            return true;
        }
        if let Some((shrieking, can_summon, wet)) = cb::shrieker_of(b) {
            if shrieking {
                self.edit(p, cb::sculk_shrieker(false, can_summon, wet), false);
                let warning = self.sculk_entity(p).map_or(0, |e| e.warning);
                if self.shrieker_can_respond(b) && warning > 0 {
                    self.sculk.responses.push(ShriekerResponse { pos: p, warning });
                }
            }
            return true;
        }
        if cb::catalyst_bloom(b) == Some(true) {
            self.edit(p, cb::sculk_catalyst(false), false);
            return true;
        }
        false
    }

    /// The comparator reading of a sensor: its last frequency while active.
    pub(super) fn sculk_comparator(&self, p: IVec3, b: Block) -> Option<u8> {
        let (phase, ..) = cb::any_sensor(b)?;
        Some(if phase == Phase::Active { self.sculk_entity(p).map_or(0, |e| e.last_frequency) } else { 0 })
    }

    /// Redstone a sensor sends toward `p + direction`: weak power to every
    /// side (a calibrated sensor's input side excepted), strong power only
    /// down into the block below (`getDirectSignal` toward `UP`).
    pub(super) fn sculk_signal(&self, p: IVec3, b: Block, direction: IVec3, strong: bool) -> Option<u8> {
        cb::any_sensor(b)?;
        let power = self.sculk_entity(p).map_or(0, |e| e.power);
        if let Some((facing, ..)) = cb::calibrated_of(b)
            && direction == -facing.offset()
        {
            return Some(0);
        }
        Some(if !strong || direction == IVec3::NEG_Y { power } else { 0 })
    }

    /// `SculkShriekerBlockEntity.canRespond`.
    fn shrieker_can_respond(&self, b: Block) -> bool {
        cb::shrieker_of(b).is_some_and(|(_, can_summon, _)| can_summon)
            && self.rules.difficulty != 0
            && self.rules.spawn_wardens
    }

    /// `SculkShriekerBlockEntity.tryShriek` for `player`: a shrieker that
    /// can't summon shrieks at once; one that can waits for the game to
    /// check the nearby players' warnings ([`World::resolve_shrieks`]).
    pub fn try_shriek(&mut self, p: IVec3, player: PlayerId) {
        let Some(b) = self.get_block(p) else { return };
        if !cb::shrieker_of(b).is_some_and(|(shrieking, ..)| !shrieking) {
            return;
        }
        self.sculk_entity_mut(p).warning = 0;
        if self.shrieker_can_respond(b) {
            if !self.sculk.shriek_triggers.contains(&(p, player)) {
                self.sculk.shriek_triggers.push((p, player));
            }
        } else {
            self.shriek(p, EventSource::player(player, false));
        }
    }

    /// `SculkShriekerBlockEntity.shriek`.
    fn shriek(&mut self, p: IVec3, source: EventSource) {
        let Some((false, can_summon, wet)) = self.get_block(p).and_then(cb::shrieker_of) else { return };
        self.edit(p, cb::sculk_shrieker(true, can_summon, wet), false);
        self.schedule_redstone(p, SHRIEKING_TICKS, 0);
        self.sculk.sounds.push((center(p), SculkSound::Shriek));
        self.particles.push(crate::particles::Request::Burst({
            let mut b = crate::particles::Burst::new(crate::particles::Kind::Effect, center(p) + DVec3::Y * 0.5, 6);
            b.spread = DVec3::new(0.2, 0.1, 0.2);
            b.velocity = DVec3::new(0.0, 0.08, 0.0);
            b.color = Some([0.24, 0.75, 0.78, 1.0]);
            b
        }));
        self.emit_game_event(center(p), GameEvent::Shriek, source, None);
    }

    /// Settles the queued shrieker triggers of summoning shriekers with the
    /// players' warning levels (`WardenSpawnTracker.tryWarn`): no warning
    /// with a warden within the 48-block box around the shrieker or any
    /// player within 16 (and the trigger) on cooldown; otherwise the highest
    /// level rises by one and every one of them takes it.
    pub fn resolve_shrieks(&mut self, players: &mut [ShriekPlayer], warden_near: impl Fn(DVec3) -> bool) {
        for (p, trigger) in std::mem::take(&mut self.sculk.shriek_triggers) {
            let Some(b) = self.get_block(p) else { continue };
            let Some((false, ..)) = cb::shrieker_of(b) else { continue };
            let Some(t) = players.iter().position(|pl| pl.id == trigger) else { continue };
            let source = EventSource {
                untargetable: players[t].untargetable,
                spectator: players[t].spectator,
                ..EventSource::player(trigger, false)
            };
            if !self.shrieker_can_respond(b) {
                self.shriek(p, source);
                continue;
            }
            let origin = center(p);
            if warden_near(origin) {
                continue;
            }
            let mut group: Vec<usize> = (0..players.len())
                .filter(|&i| {
                    let pl = &players[i];
                    !pl.spectator && pl.alive && pl.pos.distance_squared(origin) < 16.0 * 16.0
                })
                .collect();
            if !group.contains(&t) {
                group.push(t);
            }
            if group.iter().any(|&i| players[i].tracker.on_cooldown()) {
                continue;
            }
            // `Stream.max` keeps the first of equal levels.
            let Some(top) = group.iter().copied().fold(None, |best: Option<usize>, i| match best {
                Some(b) if players[b].tracker.warning_level >= players[i].tracker.warning_level => Some(b),
                _ => Some(i),
            }) else {
                continue;
            };
            players[top].tracker.increase();
            let warned = *players[top].tracker;
            for &i in &group {
                *players[i].tracker = warned;
            }
            self.sculk_entity_mut(p).warning = warned.warning_level;
            self.shriek(p, source);
        }
    }

    /// Something stepped on the block at `cell` (Java `stepOn`): an
    /// inactive sensor is forced to hear the step (unless a warden stands
    /// on it), and a player on a shrieker makes it try to shriek.
    pub fn sculk_step_on(&mut self, cell: IVec3, entity_pos: DVec3, source: EventSource, warden: bool) {
        let Some(b) = self.get_block(cell) else { return };
        match listener_kind(b) {
            Some(kind @ (ListenerKind::Sensor | ListenerKind::Calibrated)) if !warden => {
                if !self.sculk_can_receive(cell, b, kind, cell, GameEvent::Step, &EventSource::NONE) {
                    return;
                }
                // `Listener.forceScheduleVibration`.
                let tick = self.redstone.tick;
                let info = VibrationInfo {
                    event: GameEvent::Step,
                    distance: entity_pos.distance(center(cell)) as f32,
                    pos: entity_pos,
                    source,
                };
                self.sculk_entity_mut(cell).vibration.add_candidate(info, tick);
                self.sculk.busy.insert(cell.to_array());
            }
            Some(ListenerKind::Shrieker) => {
                if let Some(player) = source.player {
                    self.try_shriek(cell, player);
                }
            }
            _ => {}
        }
    }

    /// A player just moved from `before` (Java `Entity.move` and
    /// `checkFallDamage`): steps (every 1/0.6 blocks walked on something
    /// other than air), swims and landings become game events, and the
    /// block underfoot hears a step (`stepOn`). Spectators make none.
    pub fn player_moved(
        &mut self,
        id: PlayerId,
        player: &crate::player::Player,
        before: DVec3,
        spectator: bool,
        creative: bool,
    ) {
        if spectator || self.remote {
            self.sculk.players.remove(&id);
            return;
        }
        let source = EventSource { untargetable: creative, ..EventSource::player(id, player.sneaking) };
        let (mut dist, mut next, mut peak) = self.sculk.players.get(&id).copied().unwrap_or((0.0, 1.0, None));
        let moved = player.pos - before;
        let below = (player.pos - DVec3::Y * 0.2).floor().as_ivec3();
        let support = self.get_block(below).filter(|&b| b != Block::AIR);
        dist += if player.climbing { moved.length() * 0.6 } else { moved.with_y(0.0).length() * 0.6 } as f32;
        if dist > next
            && let Some(block) = support
            && player.vehicle.is_none()
        {
            if (player.on_ground || player.climbing) && !player.swimming {
                next = dist.floor() + 1.0;
                self.emit_game_event(player.pos, GameEvent::Step, source, Some(block));
            } else if player.in_water {
                next = dist.floor() + 1.0;
                self.emit_game_event(player.pos, GameEvent::Swim, source, None);
            }
        }
        if player.on_ground {
            if peak.is_some_and(|p| p > player.pos.y + 1e-6) && !player.in_water {
                self.emit_game_event(player.pos, GameEvent::HitGround, source, support);
            }
            peak = None;
            self.sculk_step_on(below, player.pos, source, false);
        } else if player.in_water || player.flying || player.climbing || player.vehicle.is_some() {
            peak = None;
        } else {
            peak = Some(peak.map_or(before.y.max(player.pos.y), |p: f64| p.max(player.pos.y)));
        }
        self.sculk.players.insert(id, (dist, next, peak));
    }

    /// An entity died at `pos` (Java `CatalystListener.handleGameEvent` for
    /// `entity_die`, delivered nearest first): the nearest catalyst within 8
    /// blocks takes the `xp` it would drop as charge, blooms, and the
    /// experience is consumed. Returns whether a catalyst consumed it.
    pub fn sculk_catalyst_death(&mut self, pos: DVec3, xp: u32) -> bool {
        if self.sculk.catalysts.is_empty() || self.remote {
            return false;
        }
        let cell = pos.floor().as_ivec3();
        let r = CATALYST_RADIUS;
        let best = self
            .sculk
            .catalysts
            .iter()
            .filter(|q| (**q - cell).length_squared() <= r * r)
            .map(|&q| (pos.distance_squared(center(q)), q))
            .min_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.to_array().cmp(&b.1.to_array())));
        let Some((_, q)) = best else { return false };
        if xp > 0 {
            let start = (pos + DVec3::Y * 0.5).floor().as_ivec3();
            let cursors = &mut self.sculk_entity_mut(q).cursors;
            super::sculk_spread::add_cursors(cursors, start, xp as i32);
            self.sculk.busy.insert(q.to_array());
        }
        // `bloom`.
        self.edit(q, cb::sculk_catalyst(true), false);
        self.schedule_redstone(q, BLOOM_TICKS, 0);
        self.sculk.sounds.push((center(q), SculkSound::Bloom));
        // `ParticleTypes.SCULK_SOUL`, two over the top.
        let mut burst = crate::particles::Burst::new(crate::particles::Kind::SoulFlame, center(q) + DVec3::Y * 0.65, 2);
        burst.spread = DVec3::new(0.2, 0.0, 0.2);
        burst.velocity = DVec3::new(0.0, 0.03, 0.0);
        self.particles.push(crate::particles::Request::Burst(burst));
        true
    }

    /// Whether a catalyst within 8 blocks of `pos` would take a death there.
    pub fn sculk_catalyst_near(&self, pos: DVec3) -> bool {
        let cell = pos.floor().as_ivec3();
        self.sculk.catalysts.iter().any(|q| (*q - cell).length_squared() <= CATALYST_RADIUS * CATALYST_RADIUS)
    }

    /// Loaded catalyst cells, so the entity simulation can tell when a
    /// death's experience goes to a catalyst.
    pub fn sculk_catalysts(&self) -> Vec<IVec3> {
        self.sculk.catalysts.iter().copied().collect()
    }

    pub fn sculk_to_string(&self) -> String {
        json!(
            self.sculk
                .entities
                .iter()
                .filter(|(_, e)| **e != SculkEntity::default())
                .map(|(p, e)| json!([p, e.save()]))
                .collect::<Vec<_>>()
        )
        .to_string()
    }

    pub fn load_sculk(&mut self, text: &str) {
        let Ok(v) = serde_json::from_str::<Value>(text) else { return };
        for v in v.as_array().into_iter().flatten() {
            let Ok(p) = serde_json::from_value::<[i32; 3]>(v[0].clone()) else { continue };
            let e = SculkEntity::load(&v[1]);
            let busy = !e.idle();
            self.sculk.entities.insert(p, e);
            if busy
                && self
                    .sculk
                    .index
                    .get(&chunk_of(IVec3::from_array(p)))
                    .is_some_and(|c| c.contains(&IVec3::from_array(p)))
            {
                self.sculk.busy.insert(p);
            }
        }
    }
}

#[cfg(test)]
mod tests;
