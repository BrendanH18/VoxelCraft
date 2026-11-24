//! Redstone signals, neighbour reactions and scheduled component ticks at 20 Hz.
//! Java's update order lives in `redstone_order`; references and the
//! remaining differences: docs/redstone.md.
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

use glam::IVec3;
use rustc_hash::{FxHashMap, FxHashSet};

use super::World;
use super::block::{Block, Facing, Shaped};
use super::chunk::CHUNK_SIZE_I;
use super::nether_biome_blocks::keep_wood;
use super::rails;
use super::redstone_blocks::{self as r, Component, conductor};
use super::redstone_order::{self as order, DIRECTIONS, HORIZONTAL, VERTICAL, flags, priority};
use super::vibration::{EventSource, GameEvent};

/// `block_open` or `block_close` for a door, gate or trapdoor's new state.
fn open_event(open: bool) -> GameEvent {
    if open { GameEvent::BlockOpen } else { GameEvent::BlockClose }
}

/// The block a scheduled tick is keyed by: the redstone, rail or gadget
/// family, or the plain base block (bells, composters).
pub(super) fn family(b: Block) -> Block {
    if let Some(copper) = super::copper::unstaged(b) {
        return copper;
    }
    r::base(b)
        .or_else(|| rails::family(b))
        .or_else(|| {
            (super::gadgets::is_hook(b) || super::gadgets::is_tripwire(b)).then(|| super::gadgets::base(b)).flatten()
        })
        .unwrap_or(b.base())
}

/// Blocks with a redstone `neighborChanged` reaction.
pub(super) fn participates(b: Block) -> bool {
    r::component(b).is_some()
        || b.is_door()
        || b.is_gate()
        || b.is_rail()
        || super::gadgets::participates(b)
        || b == Block::TNT
        || b.base() == Block::BELL
        || super::copper::bulb(b).is_some()
        || super::wood::redstone_participant(b)
        || super::utilities::dripleaf_of(b).is_some()
        || super::crafter::owns(b.0)
}

pub(super) fn is_wire(b: Block) -> bool {
    matches!(r::component(b), Some(Component::Wire(_)))
}

pub(super) fn is_comparator(b: Block) -> bool {
    matches!(r::component(b), Some(Component::Comparator { .. }))
}

fn is_diode(b: Block) -> bool {
    matches!(r::component(b), Some(Component::Repeater { .. } | Component::Comparator { .. }))
}

/// Java's `hasAnalogOutputSignal`: blocks a comparator reads.
pub(super) fn has_analog_output(w: &World, p: IVec3, b: Block) -> bool {
    b != Block::AIR && w.container_signal(p).is_some()
}

/// The six sides, in the order signal reads take them (any order gives
/// the same maximum).
const SIDES: [IVec3; 6] = [IVec3::Y, IVec3::NEG_Y, IVec3::NEG_Z, IVec3::Z, IVec3::NEG_X, IVec3::X];

/// The neighbour a strong source attached to a block powers through it, for
/// sources whose own Java update calls are not ported (`flags::COMPAT`).
fn compat_strong_side(b: Block) -> Option<IVec3> {
    if let Some((facing, _, _)) = super::gadgets::hook_state(b) {
        return Some(-facing.offset());
    }
    if let Some(c) = super::lightning::rod(b) {
        return Some(-super::copper::rod_direction(c.local));
    }
    (rails::kind(b) == Some(rails::RailKind::Detector)
        || super::workstations::lectern(b.0).is_some()
        || super::utilities::trapped_chest_of(b).is_some()
        || super::cave_blocks::any_sensor(b).is_some())
    .then_some(IVec3::NEG_Y)
}

type ScheduledTick = Reverse<(u64, i8, u64, [i32; 3], u16)>;

#[derive(Default)]
pub(super) struct RedstoneState {
    pub(super) contacts: FxHashMap<IVec3, super::redstone_contacts::Contacts>,
    pub(super) tick: u64,
    sequence: u64,
    /// Components to re-evaluate once: chunk-load refreshes and saved queues.
    updates: VecDeque<IVec3>,
    queued: FxHashSet<IVec3>,
    pending: FxHashMap<IVec3, (u64, u16, u64)>,
    scheduled: BinaryHeap<ScheduledTick>,
    /// Due work in unloaded chunks. Woken only by a chunk load, never retried each tick.
    suspended: FxHashMap<IVec3, Vec<ScheduledTick>>,
    sleeping_updates: FxHashMap<IVec3, Vec<IVec3>>,
    /// Saved `redstone_changed` notifications whose chunk hasn't loaded yet.
    sleeping_changes: FxHashMap<IVec3, Vec<IVec3>>,
    /// Pack relative scheduled delays while a storage chunk is unloaded.
    unloaded_at: FxHashMap<IVec3, u64>,
    /// Detector-rail comparator output, filled by the minecart tick.
    pub(super) cart_signal: FxHashMap<IVec3, u8>,
    /// Item frame comparator output by the frame's cell and facing, filled
    /// by the entity tick.
    frame_signal: FxHashMap<(IVec3, IVec3), u8>,
    pub(super) trip_pulse: FxHashMap<IVec3, u64>,
    /// `RedstoneTorchBlock.RECENT_TOGGLES`, by torch.
    toggles: FxHashMap<IVec3, VecDeque<u64>>,
    comparator: FxHashMap<IVec3, u8>,
    /// Tracks powered edges for manually operable doors and gates.
    powered: FxHashSet<IVec3>,
    pub last_updates: usize,
    day_time: f64,
    /// Java's `Level.skyDarken`, shared with spawning.
    sky_darken: u8,
    /// Java's `visual/sun_angle` in degrees.
    sun_angle: f32,
    /// Java's neighbour update stack, tick collection and block events.
    pub(super) order: order::UpdateOrder,
    /// Deferred `redstone_changed` notifications, in arrival order.
    changed: Vec<IVec3>,
    changed_set: FxHashSet<IVec3>,
}

impl World {
    /// Re-evaluate a component once on the next tick (chunk loads, saved queues).
    fn queue_refresh(&mut self, p: IVec3) {
        if self.get_block(p).is_some_and(participates) && self.redstone.queued.insert(p) {
            self.redstone.updates.push_back(p);
        }
    }

    pub(super) fn park_redstone_update(&mut self, p: IVec3) {
        self.redstone.sleeping_updates.entry(super::chunk::chunk_of(p)).or_default().push(p);
    }

    /// Something at `p` changed its signal or analog output outside a ported
    /// Java block behaviour (containers, sensors, finished piston moves): the
    /// block re-evaluates itself, its neighbours hear `neighborChanged`, the
    /// neighbours of the block it strongly powers too, and comparators read
    /// its output (`updateNeighbourForOutputSignal`).
    ///
    /// Callers notify before they finish mutating (`chest_mut` hands out the
    /// contents after notifying), so the notification runs at the start of
    /// the next tick, before the clock advances: comparators still schedule
    /// their tick for the same game tick as a synchronous Java notification.
    pub fn redstone_changed(&mut self, p: IVec3) {
        if self.redstone.changed_set.insert(p) {
            self.redstone.changed.push(p);
        }
    }

    /// Run the notifications `redstone_changed` deferred.
    pub(super) fn drain_redstone_changes(&mut self) {
        if self.redstone.changed.is_empty() {
            return;
        }
        let changed = std::mem::take(&mut self.redstone.changed);
        self.redstone.changed_set.clear();
        for &p in &changed {
            self.neighbor_changed_at(p);
            self.update_neighbors_at(p);
            if let Some(d) = self.get_block(p).and_then(compat_strong_side) {
                self.update_neighbors_at(p + d);
            }
            self.update_neighbour_for_output_signal(p);
        }
        let mut changed = changed;
        changed.clear();
        if self.redstone.changed.is_empty() {
            self.redstone.changed = changed;
        }
    }

    pub(super) fn track_redstone(&mut self, p: IVec3, old: Block, new: Block) {
        if old == new {
            return;
        }
        if family(old) != family(new) {
            // The observer's `affectNeighborsAfterRemoval` asks whether a tick was pending.
            self.redstone.order.removed_tick = self.redstone.pending.remove(&p).map(|_| p);
            self.redstone.comparator.remove(&p);
            self.redstone.toggles.remove(&p);
            self.redstone.powered.remove(&p);
        }
        self.track_tripwire(p, old, new);
    }

    pub(super) fn load_redstone_chunk(&mut self, c: IVec3) {
        if let Some(at) = self.redstone.unloaded_at.remove(&c) {
            let elapsed = self.redstone.tick.saturating_sub(at);
            let mut scheduled = std::mem::take(&mut self.redstone.scheduled).into_vec();
            for Reverse((due, _, seq, xyz, family)) in &mut scheduled {
                let p = IVec3::from_array(*xyz);
                if super::chunk::chunk_of(p) == c && self.redstone.pending.get(&p) == Some(&(*due, *family, *seq)) {
                    *due = due.saturating_add(elapsed);
                    self.redstone.pending.insert(p, (*due, *family, *seq));
                }
            }
            self.redstone.scheduled = scheduled.into();
            if let Some(ticks) = self.redstone.suspended.get_mut(&c) {
                for Reverse((due, _, seq, xyz, family)) in ticks {
                    let p = IVec3::from_array(*xyz);
                    if self.redstone.pending.get(&p) == Some(&(*due, *family, *seq)) {
                        *due = due.saturating_add(elapsed);
                        self.redstone.pending.insert(p, (*due, *family, *seq));
                    }
                }
            }
        }
        if let Some(updates) = self.redstone.sleeping_updates.remove(&c) {
            self.redstone.updates.extend(updates);
        }
        for p in self.redstone.sleeping_changes.remove(&c).unwrap_or_default() {
            self.redstone_changed(p);
        }
        if let Some(ticks) = self.redstone.suspended.remove(&c) {
            self.redstone.scheduled.extend(ticks);
        }
        // Modified chunks restore the block-entity-like state below.
        let Some(slot) = self.chunks.get(&c) else { return };
        if !slot.modified {
            return;
        }
        let data = slot.data.clone();
        let mut i = 0;
        data.for_each_block(|b| {
            let local =
                IVec3::new(i % CHUNK_SIZE_I, i / (CHUNK_SIZE_I * CHUNK_SIZE_I), i / CHUNK_SIZE_I % CHUNK_SIZE_I);
            if super::lightning::rod(b).is_some() {
                self.track_lightning_rod(c * CHUNK_SIZE_I + local, Block::AIR, b);
            }
            // Java re-evaluates nothing when a chunk loads: a BUD stays
            // untriggered and a burned-out torch stays out. Only a powered
            // observer whose old save lost its scheduled tick gets one, so
            // it can still fall.
            if matches!(r::component(b), Some(Component::Observer { on: true, .. })) {
                let p = c * CHUNK_SIZE_I + local;
                if !self.redstone.pending.contains_key(&p) {
                    self.schedule_redstone(p, 2, priority::NORMAL);
                }
            }
            if super::sniffer_egg::owns(b.0) {
                let p = c * CHUNK_SIZE_I + local;
                if !self.redstone.pending.contains_key(&p) {
                    self.schedule_sniffer_egg(p);
                }
            }
            // v0.15 Track D: frosted ice in a chunk whose save carried no
            // tick (Java saves the tick; a lost one would freeze the ice).
            if super::frosted_ice::owns(b.0) {
                let p = c * CHUNK_SIZE_I + local;
                if !self.redstone.pending.contains_key(&p) {
                    self.schedule_redstone(p, 20, 0);
                }
            }
            if b == super::frog_blocks::FROGSPAWN {
                let local =
                    IVec3::new(i % CHUNK_SIZE_I, i / (CHUNK_SIZE_I * CHUNK_SIZE_I), i / CHUNK_SIZE_I % CHUNK_SIZE_I);
                let p = c * CHUNK_SIZE_I + local;
                if !self.redstone.pending.contains_key(&p) {
                    let delay = super::frog_blocks::HATCH_MIN
                        + self.roll() % (super::frog_blocks::HATCH_MAX - super::frog_blocks::HATCH_MIN);
                    self.schedule_redstone(p, delay, 0);
                }
            }
            if super::composter::level(b) == Some(7) {
                let local =
                    IVec3::new(i % CHUNK_SIZE_I, i / (CHUNK_SIZE_I * CHUNK_SIZE_I), i / CHUNK_SIZE_I % CHUNK_SIZE_I);
                self.schedule_redstone(c * CHUNK_SIZE_I + local, 20, 0);
            }
            i += 1;
        });
    }

    pub(super) fn schedule_redstone(&mut self, p: IVec3, delay: u64, priority: i8) {
        if self.redstone.pending.contains_key(&p) {
            return;
        }
        let Some(b) = self.get_block(p).map(family) else { return };
        self.redstone.sequence += 1;
        let sequence = self.redstone.sequence;
        let due = self.redstone.tick + delay;
        self.redstone.pending.insert(p, (due, b.0, sequence));
        self.redstone.scheduled.push(Reverse((due, priority, sequence, p.to_array(), b.0)));
    }

    /// Signal leaving `p` toward its neighbour at `p + direction`.
    /// Strong output energizes a conductor; weak output activates devices only.
    fn emitted_signal(&self, p: IVec3, direction: IVec3, strong: bool, wires: bool) -> u8 {
        let Some(b) = self.get_block(p) else { return 0 };
        if let Some(power) = self.sculk_signal(p, b, direction, strong) {
            return power;
        }
        if let Some(power) = Self::lectern_signal(b, direction, strong) {
            return power;
        }
        // `JukeboxBlock`: a weak 15 to every side while a song plays.
        if super::jukebox::is_jukebox(b) {
            return if strong { 0 } else { self.jukebox_signal(p) };
        }
        if let Some(power) = super::lightning::rod_signal(b, direction, strong) {
            return power;
        }
        if let Some(power) = self.trapped_signal(p, b, direction, strong) {
            return power;
        }
        if rails::is_powered(b) && rails::kind(b) == Some(rails::RailKind::Detector) {
            return if !strong || direction == IVec3::NEG_Y { 15 } else { 0 };
        }
        if let Some((facing, _, on)) = super::gadgets::hook_state(b) {
            return if on && (!strong || direction == facing.opposite().offset()) { 15 } else { 0 };
        }
        match r::component(b) {
            Some(Component::Plate { power, .. }) => {
                if !strong || direction == IVec3::NEG_Y {
                    power
                } else {
                    0
                }
            }
            Some(Component::Daylight { power, .. } | Component::Target(power)) => {
                if strong {
                    0
                } else {
                    power
                }
            }
            Some(Component::Source) => {
                if strong {
                    0
                } else {
                    15
                }
            }
            Some(Component::Lever { mount, on } | Component::Button { mount, on, .. }) => {
                if on && (!strong || direction == r::support(mount)) { 15 } else { 0 }
            }
            Some(Component::Torch { mount, lit }) => {
                if lit && direction != r::support(mount) && (!strong || direction == IVec3::Y) { 15 } else { 0 }
            }
            Some(Component::Wire(power)) if wires => {
                if direction == IVec3::NEG_Y
                    || (Facing::from_offset(direction).is_some_and(|f| self.wire_connections(p)[f as usize] != 0))
                {
                    power
                } else {
                    0
                }
            }
            Some(Component::Observer { facing, on }) => {
                if on && direction == -r::direction(facing) {
                    15
                } else {
                    0
                }
            }
            Some(Component::Repeater { facing, on, .. }) => {
                if on && direction == facing.offset() {
                    15
                } else {
                    0
                }
            }
            Some(Component::Comparator { facing, .. }) if direction == facing.offset() => {
                self.redstone.comparator.get(&p).copied().unwrap_or(0)
            }
            _ => 0,
        }
    }

    pub(super) fn signal_from(&self, p: IVec3, toward: IVec3, wires: bool) -> u8 {
        let direct = self.emitted_signal(p, toward, false, wires);
        if !self.get_block(p).is_some_and(conductor) {
            return direct;
        }
        SIDES.into_iter().map(|d| self.emitted_signal(p + d, -d, true, wires)).max().unwrap_or(0).max(direct)
    }

    fn best_signal(&self, p: IVec3, wires: bool) -> u8 {
        SIDES.into_iter().map(|d| self.signal_from(p + d, -d, wires)).max().unwrap_or(0)
    }

    pub fn redstone_power(&self, p: IVec3) -> u8 {
        self.best_signal(p, true)
    }

    /// 0 absent, 1 flat connection, 2 wire climbing a solid side.
    /// Java completes isolated/single-ended wires to crosses/straight lines.
    pub fn wire_connections(&self, p: IVec3) -> [u8; 4] {
        r::connections(|d| self.get_block(p + d).unwrap_or(Block::AIR))
    }

    fn wire_target(&self, p: IVec3) -> u8 {
        let external = self.best_signal(p, false);
        if external == 15 {
            return 15;
        }
        let mut adjacent = 0;
        for f in Facing::ALL {
            let q = p + f.offset();
            let n = self.get_block(q).unwrap_or(Block::AIR);
            let step = if conductor(n) {
                if self.get_block(p + IVec3::Y).is_some_and(conductor) { None } else { Some(q + IVec3::Y) }
            } else {
                Some(q - IVec3::Y)
            };
            for at in std::iter::once(q).chain(step) {
                if let Some(Component::Wire(power)) = self.get_block(at).and_then(r::component) {
                    adjacent = adjacent.max(power);
                }
            }
        }
        external.max(adjacent.saturating_sub(1))
    }

    fn diode_input(&self, p: IVec3, facing: Facing) -> u8 {
        let q = p - facing.offset();
        self.signal_from(q, facing.offset(), true).max(match self.get_block(q).and_then(r::component) {
            Some(Component::Wire(power)) => power,
            _ => 0,
        })
    }

    fn diode_side(&self, p: IVec3, facing: Facing, repeaters_only: bool) -> u8 {
        let side = facing.clockwise().offset();
        [side, -side]
            .into_iter()
            .map(|d| {
                // `SignalGetter.getControlInputSignal`: diodes only for a
                // repeater; otherwise a redstone block gives 15, wire its
                // power, and other sources their direct signal.
                let q = p + d;
                match self.get_block(q).and_then(r::component) {
                    Some(Component::Repeater { .. } | Component::Comparator { .. }) => {
                        self.emitted_signal(q, -d, true, true)
                    }
                    _ if repeaters_only => 0,
                    Some(Component::Source) => 15,
                    Some(Component::Wire(power)) => power,
                    _ => self.emitted_signal(q, -d, true, true),
                }
            })
            .max()
            .unwrap_or(0)
    }

    pub fn repeater_locked(&self, p: IVec3) -> bool {
        matches!(self.get_block(p).and_then(r::component), Some(Component::Repeater { facing, .. }) if self.diode_side(p, facing, true) > 0)
    }

    /// `ComparatorBlock.calculateOutputSignal`.
    fn comparator_target(&self, p: IVec3, facing: Facing, subtract: bool) -> u8 {
        let input = self.comparator_input(p, facing);
        let side = self.diode_side(p, facing, false);
        if subtract {
            input.saturating_sub(side)
        } else if input >= side {
            input
        } else {
            0
        }
    }

    /// `ComparatorBlock.shouldTurnOn`.
    fn comparator_turns_on(&self, p: IVec3, facing: Facing, subtract: bool) -> bool {
        let input = self.comparator_input(p, facing);
        let side = self.diode_side(p, facing, false);
        input > 0 && (input > side || (input == side && !subtract))
    }

    /// `ComparatorBlock.getInputSignal`.
    fn comparator_input(&self, p: IVec3, facing: Facing) -> u8 {
        let mut input = self.diode_input(p, facing);
        let rear = p - facing.offset();
        // Shelves and chiseled bookshelves read by side (`world::wood`).
        let signal = |q: IVec3| self.wood_comparator_signal(q, p - q).or_else(|| self.container_signal(q));
        if let Some(value) = signal(rear) {
            input = value;
        } else if input < 15 && self.get_block(rear).is_some_and(conductor) {
            // `ComparatorBlock.getInputSignal`: through a conductor, the larger
            // of the block behind it and an item frame hung on its far side.
            let far = rear - facing.offset();
            let block = self.wood_comparator_signal(far, facing.offset()).or_else(|| self.container_signal(far));
            let frame = self.redstone.frame_signal.get(&(far, -facing.offset())).copied();
            if let Some(value) = block.max(frame) {
                input = value;
            }
        }
        input
    }

    /// Refresh comparator readings, waking the circuit only when contents or occupancy change.
    pub fn refresh_cart_signals(&mut self, signals: impl IntoIterator<Item = (IVec3, u8)>) {
        let previous = std::mem::take(&mut self.redstone.cart_signal);
        let mut next = FxHashMap::default();
        for (p, signal) in signals {
            next.entry(p).or_insert(signal);
        }
        for (&p, &signal) in &next {
            if previous.get(&p) != Some(&signal) {
                self.redstone_changed(p);
            }
        }
        for p in previous.keys() {
            if !next.contains_key(p) {
                self.redstone_changed(*p);
            }
        }
        self.redstone.cart_signal = next;
    }

    /// Refresh item frame readings (`ItemFrame.getAnalogOutput`, keyed by
    /// the frame's cell and facing), waking comparators only on a change.
    pub fn refresh_frame_signals(&mut self, signals: impl IntoIterator<Item = ((IVec3, IVec3), u8)>) {
        let next: FxHashMap<(IVec3, IVec3), u8> = signals.into_iter().collect();
        if next == self.redstone.frame_signal {
            return;
        }
        let previous = std::mem::replace(&mut self.redstone.frame_signal, next);
        let changed: Vec<IVec3> = previous
            .iter()
            .filter(|&(k, v)| self.redstone.frame_signal.get(k) != Some(v))
            .chain(self.redstone.frame_signal.iter().filter(|&(k, v)| previous.get(k) != Some(v)))
            .map(|(&(cell, _), _)| cell)
            .collect();
        for cell in changed {
            self.redstone_changed(cell);
        }
    }

    pub fn container_signal(&self, p: IVec3) -> Option<u8> {
        if let Some(signal) = self.get_block(p).and_then(|b| self.sculk_comparator(p, b)) {
            return Some(signal);
        }
        if self.get_block(p).is_some_and(super::jukebox::is_jukebox) {
            return Some(self.jukebox_comparator(p));
        }
        if let Some(signal) = self.lectern_comparator(p) {
            return Some(signal);
        }
        if self.get_block(p).is_some_and(|b| super::creaking::heart(b).is_some()) {
            return Some(self.creaking_hearts.get(&p.to_array()).map_or(0, |h| h.signal));
        }
        if let Some((_, _, honey)) = self.get_block(p).and_then(super::bees::hive) {
            return Some(honey);
        }
        // Comparators read nothing from an ender chest (no `hasAnalogOutputSignal`).
        if self.lent_chest() == Some(p)
            || self.get_block(p).is_some_and(|b| super::utilities::ender_chest_of(b).is_some())
        {
            return None;
        }
        if let Some(b) = self.get_block(p)
            && rails::kind(b) == Some(rails::RailKind::Detector)
        {
            return Some(if rails::is_powered(b) {
                self.redstone.cart_signal.get(&p).copied().unwrap_or(0)
            } else {
                0
            });
        }
        if let Some(level) = self.get_block(p).and_then(super::composter::level) {
            return Some(level);
        }
        if let Some(signal) = self.get_block(p).and_then(super::copper_golem_statue::comparator_output) {
            return Some(signal);
        }
        if let Some(signal) = self.get_block(p).and_then(super::copper::bulb_signal) {
            return Some(signal);
        }
        // v0.17 Track C1: `CrafterBlockEntity.getRedstoneSignal`.
        if let Some(signal) = self.crafter_signal(p) {
            return Some(signal);
        }
        // v0.17 Track E2: the state-only analog outputs (Java 26.3
        // `getAnalogOutputSignal` of each block; `redstone_components.json`).
        if let Some(signal) = self.get_block(p).and_then(super::redstone_catalogue::state_analog_output) {
            return Some(signal);
        }
        fn strength(slots: impl Iterator<Item = Option<crate::inventory::Stack>>, count: usize) -> u8 {
            let mut fullness = 0.0f64;
            let mut nonempty = false;
            for stack in slots.flatten() {
                fullness += stack.count as f64 / stack.item.max_stack() as f64;
                nonempty = true;
            }
            ((fullness / count as f64 * 14.0).floor() as u8 + nonempty as u8).min(15)
        }
        if self.chest(p).is_some() {
            // `ChestBlock.getAnalogOutputSignal` reads no container when blocked.
            if self.get_block(p).and_then(super::double_chest::state).is_some() && !self.chest_opens(p) {
                return Some(0);
            }
            let count = self.container_slots(p);
            return Some(strength((0..count).map(|i| self.chest_slot(p, i)), count));
        }
        if let Some(f) = self.furnace(p) {
            return Some(strength([f.input, f.fuel, f.output].into_iter(), 3));
        }
        if let Some(b) = self.brewing_stand(p) {
            return Some(strength(b.bottles.into_iter().chain([b.ingredient, b.fuel]), 5));
        }
        // `DecoratedPotBlock`: its one-slot container's fullness.
        if self.get_block(p).is_some_and(|b| super::utilities::decorated_pot_of(b).is_some()) {
            return Some(strength(std::iter::once(self.decorated_pot_item(p)), 1));
        }
        None
    }

    pub(super) fn redstone_update(&mut self, p: IVec3) {
        let Some(b) = self.get_block(p) else { return };
        if b.is_rail() {
            return self.rail_update(p, b);
        }
        if super::gadgets::is_note(b) {
            return self.note_neighbour(p, b);
        }
        // v0.16 Track C: piglin and dragon heads follow the signal.
        if let Some(s) = super::skulls::decode(b) {
            return self.skull_neighbour(p, s);
        }
        if super::gadgets::is_hook(b) {
            if let Some((facing, _, _)) = super::gadgets::hook_state(b)
                && self.get_block(p - facing.offset()).is_some_and(|s| !s.supports_fire())
            {
                self.spill_block(p, b);
                self.edit(p, Block::AIR, false);
                return;
            }
            return self.connect_hook(p);
        }
        if super::gadgets::is_tripwire(b) {
            return;
        }
        if super::copper::bulb(b).is_some() {
            return self.copper_bulb_update(p, b);
        }
        // v0.17 Track C1: `CrafterBlock.neighborChanged`.
        if super::crafter::owns(b.0) {
            return self.crafter_update(p, b);
        }
        if super::utilities::dripleaf_of(b).is_some() {
            return self.dripleaf_neighbor_changed(p);
        }
        if b.base() == Block::BELL {
            let powered = self.redstone_power(p) > 0;
            let was = self.redstone.powered.contains(&p);
            if powered {
                self.redstone.powered.insert(p);
            } else {
                self.redstone.powered.remove(&p);
            }
            if powered && !was {
                self.ring_bell(p);
            }
            return;
        }
        // `DiodeBlock.neighborChanged`: an unsupported diode drops and
        // notifies all its neighbours' neighbours.
        if is_diode(b) && self.get_block(p - IVec3::Y).is_some_and(|below| !b.can_stay_on(below)) {
            self.spill_block(p, b);
            self.set_block_flags(p, Block::AIR, flags::UPDATE_ALL);
            for d in DIRECTIONS {
                self.update_neighbors_at(p + d);
            }
            return;
        }
        match r::component(b) {
            Some(
                Component::Piston { .. }
                | Component::Observer { .. }
                | Component::Dispenser { .. }
                | Component::Hopper { .. },
            ) => self.automation_update(p, b),
            Some(Component::Wire(_)) => {
                // `RedStoneWireBlock.neighborChanged`: survive or drop.
                if self.get_block(p - IVec3::Y).is_some_and(|below| !b.can_stay_on(below)) {
                    self.spill_block(p, b);
                    self.set_block_flags(p, Block::AIR, flags::UPDATE_ALL);
                } else {
                    self.wire_update_power(p, b);
                }
            }
            Some(Component::Torch { mount, lit }) => {
                if lit == (self.signal_from(p + r::support(mount), -r::support(mount), true) > 0)
                    && !self.will_tick_this_tick(p)
                {
                    self.schedule_redstone(p, 2, priority::NORMAL);
                }
            }
            Some(Component::Repeater { facing, delay, on }) => {
                // `DiodeBlock.checkTickOnNeighbor`.
                if !self.repeater_locked(p) && on != (self.diode_input(p, facing) > 0) && !self.will_tick_this_tick(p) {
                    let priority = if self.diode_prioritized(p, facing) {
                        priority::EXTREMELY_HIGH
                    } else if on {
                        priority::VERY_HIGH
                    } else {
                        priority::HIGH
                    };
                    self.schedule_redstone(p, delay as u64 * 2, priority);
                }
            }
            Some(Component::Comparator { facing, subtract, on }) => {
                // `ComparatorBlock.checkTickOnNeighbor`.
                if !self.will_tick_this_tick(p) {
                    let output = self.comparator_target(p, facing, subtract);
                    let stored = self.redstone.comparator.get(&p).copied().unwrap_or(0);
                    if output != stored || on != self.comparator_turns_on(p, facing, subtract) {
                        let priority =
                            if self.diode_prioritized(p, facing) { priority::HIGH } else { priority::NORMAL };
                        self.schedule_redstone(p, 2, priority);
                    }
                }
            }
            Some(Component::Daylight { .. }) => {
                self.schedule_redstone(p, 20, 0);
            }
            Some(Component::Plate { kind, power }) if power > 0 => {
                self.schedule_redstone(p, if kind < 2 { 20 } else { 10 }, 0);
            }
            Some(Component::IronDoor { .. } | Component::Trapdoor { .. }) => {
                self.redstone_open(p, b);
            }
            Some(Component::Lamp(on)) => {
                // `RedstoneLampBlock.neighborChanged`.
                let powered = self.redstone_power(p) > 0;
                if !on && powered {
                    self.set_block_flags(p, Block(1211), flags::UPDATE_CLIENTS);
                } else if on && !powered {
                    self.schedule_redstone(p, 4, priority::NORMAL);
                }
            }
            _ if b.is_door() || b.is_gate() => {
                let lower = if b.is_door_upper() { p - IVec3::Y } else { p };
                let powered =
                    self.redstone_power(lower) > 0 || (b.is_door() && self.redstone_power(lower + IVec3::Y) > 0);
                let was = self.redstone.powered.contains(&lower);
                if was != powered {
                    if powered {
                        self.redstone.powered.insert(lower);
                    } else {
                        self.redstone.powered.remove(&lower);
                    }
                    if let Some(block) = self.get_block(lower) {
                        let state = match block.shaped() {
                            Some(Shaped::Door { facing, open, .. } | Shaped::Gate { facing, open }) => {
                                Some((facing, open))
                            }
                            _ => None,
                        };
                        // `DoorBlock`/`FenceGateBlock.neighborChanged` set with
                        // `UPDATE_CLIENTS`; the other half follows its shape update.
                        if let Some((facing, open)) = state
                            && open != powered
                        {
                            self.set_block_flags(lower, block.toggled(facing), flags::UPDATE_CLIENTS);
                            if b.is_door()
                                && let Some(upper) = self.get_block(lower + IVec3::Y).filter(|b| b.is_door_upper())
                            {
                                self.set_block_flags(lower + IVec3::Y, upper.toggled(facing), flags::UPDATE_CLIENTS);
                            }
                            // `DoorBlock` / `FenceGateBlock.neighborChanged`.
                            self.emit_block_event(lower, open_event(powered), EventSource::NONE, None);
                        }
                    }
                }
            }
            _ if super::wood::redstone_participant(b) => self.shelf_power(p),
            _ if b == Block::TNT && self.rules.tnt_explodes && self.redstone_power(p) > 0 => {
                self.edit(p, Block::AIR, false);
                self.primed_tnt.push((p, false));
                // `TntBlock.prime` with no igniter.
                self.emit_block_event(p, GameEvent::PrimeFuse, EventSource::NONE, None);
            }
            _ => {}
        }
    }

    /// A scheduled tick. Ported Java ticks run as Java's do, each of their
    /// updates a complete run; the other blocks' ticks run as one batch.
    pub(super) fn redstone_scheduled_tick(&mut self, p: IVec3) {
        let Some(b) = self.get_block(p) else { return };
        if matches!(
            r::component(b),
            Some(
                Component::Torch { .. }
                    | Component::Repeater { .. }
                    | Component::Comparator { .. }
                    | Component::Button { .. }
                    | Component::Plate { .. }
                    | Component::Lamp(_)
                    | Component::Observer { .. }
            )
        ) {
            self.scheduled_tick_dispatch(p, b);
        } else {
            self.batched(|w| w.scheduled_tick_dispatch(p, b));
        }
    }

    fn scheduled_tick_dispatch(&mut self, p: IVec3, b: Block) {
        if self.sculk_scheduled_tick(p, b) {
            return;
        }
        if super::sniffer_egg::owns(b.0) {
            return self.hatch_sniffer_egg(p);
        }
        // v0.17 Track C1: `CrafterBlock.tick` crafts.
        if super::crafter::owns(b.0) {
            return self.crafter_tick(p, b);
        }
        if self.end_lectern_pulse(p) {
            return;
        }
        if super::frosted_ice::owns(b.0) {
            return self.frosted_ice_tick(p);
        }
        if super::dried_ghast::owns(b.0) {
            return self.dried_ghast_tick(p);
        }
        if super::end_blocks::is_chorus(b) {
            return self.chorus_tick(p);
        }
        if super::archaeology::owns(b.0) {
            return self.archaeology_tick(p);
        }
        if super::lightning::rod(b).is_some() {
            return self.reset_lightning_rod(p, b);
        }
        if b == super::frog_blocks::FROGSPAWN {
            return self.hatch_frogspawn(p);
        }
        if matches!(b, super::eyeblossom::OPEN | super::eyeblossom::CLOSED) {
            self.tick_eyeblossom(p, b);
            return;
        }
        if super::scaffolding::is_scaffolding(b) {
            return self.tick_scaffolding(p);
        }
        if super::gadgets::is_hook(b) {
            return self.hook_tick(p);
        }
        if super::gadgets::is_tripwire(b) {
            return self.tripwire_tick(p);
        }
        if rails::family(b).is_some() {
            return self.detector_tick(p, b);
        }
        if super::composter::level(b).is_some() {
            self.finish_compost(p);
            return;
        }
        if super::overworld_flora::live_coral_plant(b) {
            return self.coral_death_tick(p, b);
        }
        match r::component(b) {
            Some(
                Component::Piston { .. }
                | Component::Observer { .. }
                | Component::Dispenser { .. }
                | Component::Hopper { .. },
            ) => self.automation_tick(p, b),
            Some(Component::Torch { mount, lit }) => {
                // `RedstoneTorchBlock.tick` with its `RECENT_TOGGLES` window.
                let powered = self.signal_from(p + r::support(mount), -r::support(mount), true) > 0;
                let now = self.redstone.tick;
                let history = self.redstone.toggles.entry(p).or_default();
                while history.front().is_some_and(|&t| now - t > 60) {
                    history.pop_front();
                }
                if lit {
                    if powered {
                        self.set_block_flags(p, r::torch(mount, false), flags::UPDATE_ALL);
                        let history = self.redstone.toggles.entry(p).or_default();
                        history.push_back(now);
                        if history.len() >= 8 {
                            self.schedule_redstone(p, 160, priority::NORMAL);
                        }
                    }
                } else if !powered && self.redstone.toggles.get(&p).is_none_or(|h| h.len() < 8) {
                    self.set_block_flags(p, r::torch(mount, true), flags::UPDATE_ALL);
                }
                if self.redstone.toggles.get(&p).is_some_and(VecDeque::is_empty) {
                    self.redstone.toggles.remove(&p);
                }
            }
            Some(Component::Repeater { facing, delay, on }) if !self.repeater_locked(p) => {
                // `DiodeBlock.tick`.
                let input = self.diode_input(p, facing) > 0;
                if on && !input {
                    self.set_block_flags(p, r::repeater(facing, delay, false), flags::UPDATE_CLIENTS);
                } else if !on {
                    self.set_block_flags(p, r::repeater(facing, delay, true), flags::UPDATE_CLIENTS);
                    if !input {
                        self.schedule_redstone(p, delay as u64 * 2, priority::VERY_HIGH);
                    }
                }
            }
            Some(Component::Comparator { facing, subtract, on }) => self.comparator_refresh(p, facing, subtract, on),
            Some(Component::Button { mount, wood, on: true }) => {
                // `ButtonBlock.checkPressed`.
                let pressed = wood && self.redstone.contacts.get(&p).is_some_and(|c| c.arrows);
                if !pressed {
                    self.set_block_flags(p, keep_wood(b, r::button(mount, false, wood)), flags::UPDATE_ALL);
                    self.update_attached_neighbours(p, mount);
                    // `ButtonBlock.checkPressed` releasing: no arrow, no source.
                    self.emit_block_event(p, GameEvent::BlockDeactivate, EventSource::NONE, None);
                } else {
                    self.schedule_redstone(p, 30, priority::NORMAL);
                }
            }
            Some(Component::Plate { kind, power: old }) => {
                let power = super::redstone_contacts::plate_power(
                    kind,
                    self.redstone.contacts.get(&p).copied().unwrap_or_default(),
                );
                self.set_plate(p, b, kind, power);
                // `BasePressurePlateBlock.checkPressed(null, ...)` from its tick.
                if old > 0 && power == 0 {
                    self.emit_block_event(p, GameEvent::BlockDeactivate, EventSource::NONE, None);
                }
                if power > 0 {
                    self.schedule_redstone(p, if kind < 2 { 20 } else { 10 }, priority::NORMAL);
                }
            }
            Some(Component::Target(_)) => {
                self.edit(p, r::TARGET, false);
            }
            // Older saves scheduled the former fixed expiry; Java extinguishes on a random tick.
            Some(Component::GlowingOre(deep)) => {
                self.edit(p, if deep { Block::DEEPSLATE_REDSTONE_ORE } else { Block::REDSTONE_ORE }, false);
            }
            Some(Component::Daylight { power, inverted }) => {
                // Unknown skylight beside unloaded terrain keeps the output.
                if let Some(value) = self.daylight_power(p, inverted)
                    && value != power
                {
                    self.edit(p, r::daylight(value, inverted), false);
                }
                self.schedule_redstone(p, 20, 0);
            }
            Some(Component::Lamp(true)) if self.redstone_power(p) == 0 => {
                self.set_block_flags(p, r::LAMP, flags::UPDATE_CLIENTS);
            }
            _ => {}
        }
    }

    pub(super) fn unload_redstone_chunk(&mut self, c: IVec3) {
        self.unload_lightning_chunk(c);
        if self.redstone.pending.keys().any(|&p| super::chunk::chunk_of(p) == c) {
            self.redstone.unloaded_at.entry(c).or_insert(self.redstone.tick);
        }
    }

    pub(super) fn refresh_redstone_activity(&mut self) {
        let mut suspended = std::mem::take(&mut self.redstone.suspended);
        for ticks in suspended.values_mut() {
            ticks.retain(|&tick @ Reverse((_, _, _, xyz, _))| {
                if self.is_block_ticking(IVec3::from_array(xyz)) {
                    self.redstone.scheduled.push(tick);
                    false
                } else {
                    true
                }
            });
        }
        suspended.retain(|_, ticks| !ticks.is_empty());
        self.redstone.suspended = suspended;
        let mut sleeping = std::mem::take(&mut self.redstone.sleeping_updates);
        for updates in sleeping.values_mut() {
            updates.retain(|&p| {
                if self.is_block_ticking(p) {
                    self.redstone.updates.push_back(p);
                    false
                } else {
                    true
                }
            });
        }
        sleeping.retain(|_, updates| !updates.is_empty());
        self.redstone.sleeping_updates = sleeping;
    }

    /// One authoritative 50 ms game step: the scheduled block ticks, then the
    /// block events. `simulation::tick_world_rules` runs the two parts apart,
    /// with the block events after the random ticks as in `ServerLevel.tick`.
    pub fn tick_redstone(&mut self) {
        self.tick_block_ticks();
        self.run_block_events();
        // v0.17 Track B: the moving-piston block entities, then Track C2's
        // hoppers (both `tickBlockEntities`).
        self.tick_moving_pistons();
        self.tick_hoppers();
    }

    /// `ServerLevel.tick`'s `blockTicks.tick(time, 65536, tickBlock)`: advance
    /// the clock, collect every due tick (up to the cap) in Java's order, then
    /// run them; ticks scheduled while they run wait for a later tick.
    /// Neighbour updates run synchronously inside each tick. No scanning of
    /// loaded blocks or allocation on an idle tick.
    pub fn tick_block_ticks(&mut self) {
        self.redstone.order.executed = 0;
        // Deferred notifications and refreshes belong to the time they were
        // made: they run before the clock advances.
        self.drain_redstone_changes();
        self.drain_redstone_refreshes();
        self.redstone.tick += 1;
        // Java's `handlingTick` spans block ticks and block events.
        self.pistons.handling_tick = true;
        let mut due_ticks = Vec::new();
        while due_ticks.len() < order::MAX_SCHEDULED_TICKS {
            let Some(&Reverse((due, priority, sequence, xyz, family))) = self.redstone.scheduled.peek() else { break };
            if due > self.redstone.tick {
                break;
            }
            self.redstone.scheduled.pop();
            let p = IVec3::from_array(xyz);
            if self.redstone.pending.get(&p) != Some(&(due, family, sequence)) {
                continue;
            }
            if !self.is_block_ticking(p) {
                self.redstone
                    .suspended
                    .entry(super::chunk::chunk_of(p))
                    .or_default()
                    .push(Reverse((due, priority, sequence, xyz, family)));
                continue;
            }
            // `LevelChunkTicks.poll`: a collected tick no longer counts as scheduled.
            self.redstone.pending.remove(&p);
            due_ticks.push((due, priority, sequence, p, family));
        }
        if !due_ticks.is_empty() {
            let mut collected = std::mem::take(&mut self.redstone.order.collected);
            World::order_collected(&mut due_ticks, &mut collected);
            for &(p, family) in &collected {
                self.redstone.order.this_tick.insert(p, family);
            }
            self.redstone.order.collected = collected;
            self.run_collected_ticks();
        }
        // v0.17 Track C1: crafter block entities tick after the scheduled ticks.
        self.tick_crafters();
        self.redstone.last_updates = self.redstone.order.executed;
    }

    pub(super) fn turtle_hatching_time(&self) -> bool {
        (0.9..=0.94).contains(&self.redstone.day_time)
    }
    /// Clock inputs for detectors and hatching: the day fraction, Java's
    /// shared `skyDarken` and the `visual/sun_angle` in degrees.
    pub fn set_redstone_daylight(&mut self, time: f64, sky_darken: u8, sun_angle: f32) {
        self.redstone.day_time = time.rem_euclid(1.0);
        self.redstone.sky_darken = sky_darken.min(15);
        self.redstone.sun_angle = sun_angle;
    }

    /// Java's `Level.skyDarken` as last supplied by the clock.
    pub(super) fn sky_darken(&self) -> u8 {
        self.redstone.sky_darken
    }

    /// Java's `DaylightDetectorBlock.updateSignalStrength` on the gameplay
    /// skylight at the detector; `None` while that light is unknown.
    fn daylight_power(&self, p: IVec3, inverted: bool) -> Option<u8> {
        let effective = self.sky_light(p)?.saturating_sub(self.redstone.sky_darken);
        Some(crate::simulation::lighting::daylight_detector_power(effective, self.redstone.sun_angle, inverted))
    }

    fn redstone_open(&mut self, p: IVec3, b: Block) {
        match r::component(b) {
            Some(Component::IronDoor { upper, .. }) => {
                let lower = if upper { p - IVec3::Y } else { p };
                let on = self.redstone_power(lower) > 0 || self.redstone_power(lower + IVec3::Y) > 0;
                let mut changed = false;
                for (q, upper) in [(lower, false), (lower + IVec3::Y, true)] {
                    if let Some(block) = self.get_block(q)
                        && let Some(Component::IronDoor { facing, open, .. }) = r::component(block)
                        && open != on
                    {
                        let door = super::mount_states::keep_hinge(block, r::iron_door(facing, on, upper));
                        self.set_block_flags(q, door, flags::UPDATE_CLIENTS);
                        changed = true;
                    }
                }
                // `DoorBlock.neighborChanged`: one event for the pair.
                if changed {
                    self.emit_block_event(lower, open_event(on), EventSource::NONE, None);
                }
            }
            Some(Component::Trapdoor { facing, open, top, iron }) => {
                let on = self.redstone_power(p) > 0;
                let was = self.redstone.powered.contains(&p);
                if on {
                    self.redstone.powered.insert(p);
                } else {
                    self.redstone.powered.remove(&p);
                }
                if (iron || was != on) && open != on {
                    self.set_block_flags(p, keep_wood(b, r::trapdoor(facing, on, top, iron)), flags::UPDATE_CLIENTS);
                    // `TrapDoorBlock.playSound` from `neighborChanged`.
                    self.emit_block_event(p, open_event(on), EventSource::NONE, None);
                }
            }
            _ => {}
        }
    }

    pub(super) fn extinguish_ore(&mut self, p: IVec3, b: Block) {
        if let Some(Component::GlowingOre(deep)) = r::component(b) {
            self.edit(p, if deep { Block::DEEPSLATE_REDSTONE_ORE } else { Block::REDSTONE_ORE }, false);
        }
    }

    pub fn touch_redstone_ore(&mut self, p: IVec3) -> bool {
        let Some(b) = self.get_block(p) else { return false };
        let base = b.base();
        let deep = base == Block::DEEPSLATE_REDSTONE_ORE;
        if !deep && base != Block::REDSTONE_ORE {
            return false;
        }
        if !matches!(r::component(b), Some(Component::GlowingOre(_))) {
            self.edit(p, Block(if deep { 1436 } else { 1435 }), false);
        }
        true
    }

    pub fn hit_redstone_target(&mut self, p: IVec3, hit: glam::DVec3, normal: IVec3, arrow: bool) -> bool {
        if !matches!(self.get_block(p).and_then(r::component), Some(Component::Target(_))) {
            return false;
        }
        if self.redstone.pending.contains_key(&p) {
            return true;
        }
        let local = hit - p.as_dvec3() - glam::DVec3::splat(0.5);
        let distance = if normal.x != 0 {
            local.y.abs().max(local.z.abs())
        } else if normal.y != 0 {
            local.x.abs().max(local.z.abs())
        } else {
            local.x.abs().max(local.y.abs())
        };
        let power = (15.0 * ((0.5 - distance) * 2.0).clamp(0.0, 1.0)).ceil().max(1.0) as u8;
        self.edit(p, r::target(power), false);
        self.schedule_redstone(p, if arrow { 20 } else { 8 }, 0);
        true
    }

    pub fn redstone_supported(&self, p: IVec3, b: Block) -> bool {
        if let Some((facing, _, _)) = super::gadgets::hook_state(b) {
            return self.get_block(p - facing.offset()).is_some_and(Block::supports_fire);
        }
        let (support, torch_floor) = match r::component(b) {
            Some(Component::Torch { mount, .. }) => (r::support(mount), mount == 0),
            Some(Component::Lever { mount, .. } | Component::Button { mount, .. }) => (r::support(mount), false),
            Some(
                Component::Wire(_)
                | Component::Repeater { .. }
                | Component::Comparator { .. }
                | Component::Plate { .. },
            ) => (IVec3::NEG_Y, false),
            _ if matches!(
                super::nether_biome_blocks::Vine::of(b),
                Some((super::nether_biome_blocks::Vine::Weeping, _))
            ) =>
            {
                return self.get_block(p + IVec3::Y).is_some_and(super::nether_biome_blocks::hangs_from);
            }
            _ => return true,
        };
        // `canSupportCenter` for a standing torch, `isFaceSturdy` otherwise.
        self.get_block(p + support).is_some_and(|b| {
            if torch_floor {
                super::mount_states::supports_center(b, -support)
            } else {
                super::mount_states::face_sturdy(b, -support)
            }
        })
    }

    /// The saved POWERED edge state for a newly placed door. Called after
    /// both halves exist, before any tick can mistake placement for a new
    /// rising edge (including after a player manually closes a powered door).
    pub(crate) fn finish_door_placement(&mut self, p: IVec3) {
        if self.redstone_power(p) > 0 || self.redstone_power(p + IVec3::Y) > 0 {
            self.redstone.powered.insert(p);
        } else {
            self.redstone.powered.remove(&p);
        }
    }

    /// Shared two-cell iron door placement for all input sources.
    pub fn place_iron_door(&mut self, p: IVec3, facing: Facing) -> bool {
        if ![p, p + IVec3::Y].into_iter().all(|q| self.get_block(q).is_some_and(Block::is_replaceable))
            || !self.get_block(p - IVec3::Y).is_some_and(Block::is_opaque)
        {
            return false;
        }
        let powered = self.redstone_power(p) > 0 || self.redstone_power(p + IVec3::Y) > 0;
        self.set_block(p, r::iron_door(facing, powered, false));
        let placed = self.set_block(p + IVec3::Y, r::iron_door(facing, powered, true));
        self.finish_door_placement(p);
        placed
    }

    /// Chunk-load refreshes and saved queues: each component hears one
    /// `neighborChanged`.
    fn drain_redstone_refreshes(&mut self) {
        while let Some(p) = self.redstone.updates.pop_front() {
            if !self.is_block_ticking(p) {
                self.park_redstone_update(p);
                continue;
            }
            self.redstone.queued.remove(&p);
            self.neighbor_changed_at(p);
        }
    }

    pub fn redstone_updates_last_tick(&self) -> usize {
        self.redstone.last_updates
    }

    /// Device-independent right click; callers handle reach and build permission.
    /// Whether a hand use at `p` acts in Java before the held item's own use:
    /// note blocks, bells, levers, buttons, wooden trapdoors, daylight
    /// detectors, repeaters, comparators and toggleable wire. Iron doors and
    /// trapdoors pass (`useWithoutItem` returns PASS without a redstone signal).
    pub fn redstone_usable(&self, p: IVec3) -> bool {
        let Some(b) = self.get_block(p) else { return false };
        if super::gadgets::is_note(b) || b.base() == Block::BELL {
            return true;
        }
        match r::component(b) {
            Some(Component::Wire(_)) => matches!(self.wire_connections(p), [1, 1, 1, 1] | [0, 0, 0, 0]),
            Some(Component::Trapdoor { iron, .. }) => !iron,
            Some(
                Component::Daylight { .. }
                | Component::Lever { .. }
                | Component::Button { .. }
                | Component::Repeater { .. }
                | Component::Comparator { .. },
            ) => true,
            _ => false,
        }
    }

    pub fn use_redstone(&mut self, p: IVec3) -> bool {
        self.use_redstone_by(p, EventSource::NONE)
    }

    /// [`World::use_redstone`] by `source`, posting what Java's use posts:
    /// `note_block_play` (`NoteBlock.playNote`), `block_change` for a bell
    /// (`attemptToRing`) or daylight detector, `block_activate` or
    /// `block_deactivate` for a lever (`pull`) and `block_activate` for a
    /// button (`press`), `block_open` or `block_close` for a trapdoor
    /// (`playSound`). Repeaters, comparators and wire post nothing.
    pub fn use_redstone_by(&mut self, p: IVec3, source: EventSource) -> bool {
        let Some(b) = self.get_block(p) else { return false };
        if super::gadgets::is_note(b) {
            self.cycle_note_by(p, b, source);
            return true;
        }
        if b.base() == Block::BELL {
            return self.ring_bell_by(p, source);
        }
        let event = match r::component(b) {
            Some(Component::Trapdoor { open, iron: false, .. }) => Some(open_event(!open)),
            Some(Component::Daylight { .. }) => Some(GameEvent::BlockChange),
            Some(Component::Lever { on, .. }) => {
                Some(if on { GameEvent::BlockDeactivate } else { GameEvent::BlockActivate })
            }
            Some(Component::Button { on: false, .. }) => Some(GameEvent::BlockActivate),
            _ => None,
        };
        let used = self.use_redstone_inner(p, b);
        if used && let Some(event) = event {
            // The daylight detector names its new state (`Context.of(player, newState)`).
            let affected = (event == GameEvent::BlockChange).then(|| self.get_block(p)).flatten();
            self.emit_block_event(p, event, source, affected);
        }
        used
    }

    fn use_redstone_inner(&mut self, p: IVec3, b: Block) -> bool {
        let next = match r::component(b) {
            Some(Component::Wire(power)) => {
                if self.wire_connections(p) == [1; 4] || self.wire_connections(p) == [0; 4] {
                    if b.0 >= 1453 { r::wire(power) } else { r::dot(power) }
                } else {
                    return false;
                }
            }
            Some(Component::Trapdoor { facing, open, top, iron: false }) => {
                keep_wood(b, r::trapdoor(facing, !open, top, false))
            }
            Some(Component::Trapdoor { iron: true, .. } | Component::IronDoor { .. }) => return true,
            Some(Component::Daylight { power, inverted }) => r::daylight(power, !inverted),
            // `LeverBlock.pull`: set and update, then update around the lever and its support.
            Some(Component::Lever { mount, on }) => {
                let changed = self.set_block_flags(p, r::lever(mount, !on), flags::UPDATE_ALL);
                self.update_attached_neighbours(p, mount);
                return changed;
            }
            // `ButtonBlock.press`.
            Some(Component::Button { mount, on: false, wood }) => {
                let changed = self.set_block_flags(p, keep_wood(b, r::button(mount, true, wood)), flags::UPDATE_ALL);
                self.update_attached_neighbours(p, mount);
                self.schedule_redstone(p, if wood { 30 } else { 20 }, priority::NORMAL);
                return changed;
            }
            Some(Component::Button { on: true, .. }) => return true,
            // `RepeaterBlock.useWithoutItem`: `setBlock(..., UPDATE_ALL)`.
            Some(Component::Repeater { facing, delay, on }) => {
                return self.set_block_flags(p, r::repeater(facing, delay % 4 + 1, on), flags::UPDATE_ALL);
            }
            // `ComparatorBlock.useWithoutItem`: `UPDATE_CLIENTS`, then `refreshOutputState`.
            Some(Component::Comparator { facing, subtract, on }) => {
                let changed = self.set_block_flags(p, r::comparator(facing, !subtract, on), flags::UPDATE_CLIENTS);
                if matches!(self.get_block(p).and_then(r::component), Some(Component::Comparator { .. })) {
                    self.comparator_refresh(p, facing, !subtract, on);
                }
                return changed;
            }
            _ => return false,
        };
        self.set_block(p, next)
    }

    /// Versioned dimension property (v2; v1 still loads). Pending ticks use
    /// relative deadlines; block ids already persist wire and device states
    /// in VXC2 chunks.
    pub fn redstone_to_string(&self) -> String {
        let mut entries = vec![format!("v2,{},{}", self.redstone.tick, self.redstone.sequence)];
        let mut pending: Vec<_> = self
            .redstone
            .scheduled
            .iter()
            .chain(self.redstone.suspended.values().flatten())
            .filter(|Reverse((due, _, seq, xyz, family))| {
                self.redstone.pending.get(&IVec3::from_array(*xyz)) == Some(&(*due, *family, *seq))
            })
            .collect();
        pending.sort_unstable();
        for &&Reverse((due, priority, seq, xyz, family)) in &pending {
            entries.push(format!(
                "t,{},{},{},{},{},{},{}",
                xyz[0],
                xyz[1],
                xyz[2],
                due.saturating_sub(
                    self.redstone
                        .unloaded_at
                        .get(&super::chunk::chunk_of(IVec3::from_array(xyz)))
                        .copied()
                        .unwrap_or(self.redstone.tick)
                ),
                priority,
                seq,
                family
            ));
        }
        for p in self.redstone.updates.iter().chain(self.redstone.sleeping_updates.values().flatten()) {
            entries.push(format!("q,{},{},{}", p.x, p.y, p.z));
        }
        for (p, power) in &self.redstone.comparator {
            entries.push(format!("c,{},{},{},{}", p.x, p.y, p.z, power));
        }
        // Deferred `redstone_changed` notifications (a container changed
        // since the last tick), in order, so a save between ticks keeps them.
        for p in self.redstone.changed.iter().chain(self.redstone.sleeping_changes.values().flatten()) {
            entries.push(format!("n,{},{},{}", p.x, p.y, p.z));
        }
        // `RedstoneTorchBlock.RECENT_TOGGLES` lives in memory only, as in
        // Java: a restart forgets it (older saves' `h` entries still load).
        for p in &self.redstone.powered {
            entries.push(format!("p,{},{},{}", p.x, p.y, p.z));
        }
        for (&p, &due) in &self.redstone.trip_pulse {
            if due > self.redstone.tick {
                entries.push(format!("w,{},{},{},{}", p.x, p.y, p.z, due - self.redstone.tick));
            }
        }
        self.block_events_to_string(&mut entries);
        entries.join("|")
    }

    pub fn load_redstone(&mut self, text: &str) {
        let mut entries = text.split('|');
        let Some(header) = entries.next() else { return };
        // v2 (2026-11-23, Java update order) adds block events ("e") and
        // drops the separate torch burnout deadline ("b"): a burned-out torch
        // is its toggle history plus its pending 160-tick tick.
        let Some(rest) = header.strip_prefix("v2,").or_else(|| header.strip_prefix("v1,")) else { return };
        let Some((tick, seq)) =
            rest.split_once(',').and_then(|(a, b)| Some((a.parse::<u64>().ok()?, b.parse::<u64>().ok()?)))
        else {
            return;
        };
        self.redstone = RedstoneState { tick, sequence: seq, ..Default::default() };
        for entry in entries {
            let mut fields = entry.split(',');
            let kind = fields.next().unwrap_or("");
            let Some(xyz) =
                (|| Some([fields.next()?.parse().ok()?, fields.next()?.parse().ok()?, fields.next()?.parse().ok()?]))()
            else {
                continue;
            };
            let p = IVec3::from_array(xyz);
            let values: Vec<i64> = fields.filter_map(|s| s.parse().ok()).collect();
            match (kind, values.as_slice()) {
                ("t", &[delay, priority, seq, family])
                    if delay >= 0
                        && seq >= 0
                        && ((1100..=1599).contains(&family)
                            || matches!(family, 226 | 249 | 904)
                            || u16::try_from(family).is_ok_and(|f| super::lightning::rod(Block(f)).is_some())
                            || family == i64::from(super::frog_blocks::FROGSPAWN.0)
                            || family == i64::from(super::sniffer_egg::EGG.0)
                            || family == i64::from(super::frosted_ice::FIRST.0)
                            || family == i64::from(super::dried_ghast::DRIED_GHAST.0)
                            // v0.17 Track C1: a crafter's pending craft.
                            || family == i64::from(super::crafter::CRAFTER.0)
                            // v0.16 Track A: unsupported chorus breaking.
                            || family == i64::from(super::end_blocks::CHORUS_PLANT.0)
                            || family == i64::from(super::end_blocks::CHORUS_FLOWER.0)
                            // A lectern's page-turn pulse (`LecternBlock.tick`).
                            || family == i64::from(Block::LECTERN.0)
                            || family == i64::from(super::eyeblossom::OPEN.0)
                            || family == i64::from(super::eyeblossom::CLOSED.0)
                            || u16::try_from(family).is_ok_and(super::archaeology::owns)
                            || family == i64::from(super::scaffolding::SCAFFOLDING.0)
                            // A pending coral death (`CoralPlantBlock.tryScheduleDieTick`).
                            || u16::try_from(family)
                                .is_ok_and(|f| super::overworld_flora::live_coral_plant(Block(f))))
                        && (-3..=0).contains(&priority) =>
                {
                    let due = tick.saturating_add(delay as u64);
                    self.redstone.pending.insert(p, (due, family as u16, seq as u64));
                    self.redstone.scheduled.push(Reverse((due, priority as i8, seq as u64, xyz, family as u16)));
                }
                ("q", []) => {
                    if self.redstone.queued.insert(p) {
                        self.redstone.updates.push_back(p);
                    }
                }
                ("c", &[power]) if (0..=15).contains(&power) => {
                    self.redstone.comparator.insert(p, power as u8);
                }
                // A saved notification runs once its chunk is loaded.
                ("n", []) => {
                    let c = super::chunk::chunk_of(p);
                    if self.chunks.contains_key(&c) {
                        self.redstone_changed(p);
                    } else {
                        self.redstone.sleeping_changes.entry(c).or_default().push(p);
                    }
                }
                ("e", values) => self.load_block_event(p, values),
                ("h", &[age]) if (0..=60).contains(&age) && age as u64 <= tick => {
                    self.redstone.toggles.entry(p).or_default().push_back(tick - age as u64);
                }
                ("w", &[delay]) if delay > 0 => {
                    self.redstone.trip_pulse.insert(p, tick.saturating_add(delay as u64));
                }
                ("p", []) => {
                    self.redstone.powered.insert(p);
                }
                _ => {}
            }
        }
        // Saved delays count from when their chunk loads, like a chunk unloaded now.
        let absent: Vec<_> = self
            .redstone
            .pending
            .keys()
            .map(|&p| super::chunk::chunk_of(p))
            .filter(|c| !self.chunks.contains_key(c))
            .collect();
        for c in absent {
            self.redstone.unloaded_at.entry(c).or_insert(tick);
        }
    }
}

/// Ported Java block behaviour that runs inside `Level.setBlock`, and the
/// update calls each redstone block makes when it changes.
impl World {
    /// `DefaultRedstoneWireEvaluator.updatePowerStrength`: set the new power
    /// without neighbour updates, then update around the wire and its six
    /// neighbours in `HashSet` iteration order.
    fn wire_update_power(&mut self, p: IVec3, state: Block) {
        let Some(Component::Wire(power)) = r::component(state) else { return };
        let target = self.wire_target(p);
        if power == target {
            return;
        }
        if self.get_block(p) == Some(state) {
            let next = if state.0 >= 1453 { r::dot(target) } else { r::wire(target) };
            self.set_block_flags(p, next, flags::UPDATE_CLIENTS);
        }
        let cells = order::java_hash_set_order([
            p,
            p + DIRECTIONS[0],
            p + DIRECTIONS[1],
            p + DIRECTIONS[2],
            p + DIRECTIONS[3],
            p + DIRECTIONS[4],
            p + DIRECTIONS[5],
        ]);
        for q in cells {
            self.update_neighbors_at(q);
        }
    }

    /// `RedStoneWireBlock.updateNeighborsOfNeighboringWires`.
    fn update_neighbors_of_neighboring_wires(&mut self, p: IVec3) {
        for d in HORIZONTAL {
            self.check_corner_change_at(p + d);
        }
        for d in HORIZONTAL {
            let target = p + d;
            if self.get_block(target).is_some_and(conductor) {
                self.check_corner_change_at(target + IVec3::Y);
            } else {
                self.check_corner_change_at(target - IVec3::Y);
            }
        }
    }

    /// `RedStoneWireBlock.checkCornerChangeAt`.
    fn check_corner_change_at(&mut self, p: IVec3) {
        if self.get_block(p).is_some_and(is_wire) {
            self.update_neighbors_at(p);
            for d in DIRECTIONS {
                self.update_neighbors_at(p + d);
            }
        }
    }

    /// `DiodeBlock.shouldPrioritize`: the block in front is a diode that
    /// does not face back into this one.
    fn diode_prioritized(&self, p: IVec3, facing: Facing) -> bool {
        match self.get_block(p + facing.offset()).and_then(r::component) {
            Some(Component::Repeater { facing: front, .. } | Component::Comparator { facing: front, .. }) => {
                front != facing.opposite()
            }
            _ => false,
        }
    }

    /// `DiodeBlock`/`ObserverBlock.updateNeighborsInFront` for output toward `out`.
    fn update_neighbors_in_front(&mut self, p: IVec3, out: IVec3) {
        let front = p + out;
        self.neighbor_changed_at(front);
        self.update_neighbors_at_except(front, -out);
    }

    /// `ObserverBlock.updateNeighborsInFront`: output leaves the back face.
    pub(super) fn update_observer_front(&mut self, p: IVec3, facing: u8) {
        self.update_neighbors_in_front(p, -r::direction(facing));
    }

    /// `LeverBlock`/`ButtonBlock.updateNeighbours`.
    fn update_attached_neighbours(&mut self, p: IVec3, mount: u8) {
        self.update_neighbors_at(p);
        self.update_neighbors_at(p + r::support(mount));
    }

    /// `BasePressurePlateBlock.checkPressed`'s state change: set without
    /// neighbour updates, then update around the plate and below it.
    pub(super) fn set_plate(&mut self, p: IVec3, b: Block, kind: u8, power: u8) {
        let next = keep_wood(b, r::plate(kind, power));
        if next != b {
            self.set_block_flags(p, next, flags::UPDATE_CLIENTS);
            self.update_neighbors_at(p);
            self.update_neighbors_at(p - IVec3::Y);
        }
    }

    /// `ComparatorBlock.refreshOutputState`.
    fn comparator_refresh(&mut self, p: IVec3, facing: Facing, subtract: bool, on: bool) {
        let output = self.comparator_target(p, facing, subtract);
        let old = self.redstone.comparator.insert(p, output).unwrap_or(0);
        if old != output || !subtract {
            let should = self.comparator_turns_on(p, facing, subtract);
            if on != should {
                self.set_block_flags(p, r::comparator(facing, subtract, should), flags::UPDATE_CLIENTS);
            }
            self.update_neighbors_in_front(p, facing.offset());
        }
    }

    /// `updateShape` reactions: observers start a pulse when their watched
    /// neighbour changes; attached switches, torches and plates break when
    /// the change came from their support and it no longer holds them.
    pub(super) fn redstone_shape_update(&mut self, p: IVec3, b: Block, toward: IVec3) {
        match r::component(b) {
            Some(Component::Observer { facing, on: false }) if r::direction(facing) == toward => {
                // `ObserverBlock.startSignal`.
                if !self.redstone.pending.contains_key(&p) {
                    self.schedule_redstone(p, 2, priority::NORMAL);
                }
            }
            Some(Component::Lever { mount, .. } | Component::Button { mount, .. } | Component::Torch { mount, .. })
                if r::support(mount) == toward
                    && self.get_block(p + toward).is_some()
                    && !self.redstone_supported(p, b) =>
            {
                self.destroy_redstone_block(p, b);
            }
            // `PistonHeadBlock.updateShape`: from its base's side, a head
            // that can't survive breaks (v0.17 Track B).
            Some(Component::PistonHead { facing, sticky })
                if toward == -r::direction(facing) && !self.head_survives(p, facing, sticky) =>
            {
                self.destroy_redstone_block(p, b);
            }
            // `BasePressurePlateBlock`/`RedStoneWireBlock.updateShape` toward `DOWN`.
            Some(Component::Plate { .. } | Component::Wire(_))
                if toward == IVec3::NEG_Y
                    && self.get_block(p - IVec3::Y).is_some_and(|below| !b.can_stay_on(below)) =>
            {
                self.destroy_redstone_block(p, b);
            }
            _ => {}
        }
    }

    /// `Level.destroyBlock` with drops.
    fn destroy_redstone_block(&mut self, p: IVec3, b: Block) {
        self.spill_block(p, b);
        self.set_block_flags(p, Block::AIR, flags::UPDATE_ALL);
    }

    /// `BlockBehaviour.affectNeighborsAfterRemoval` for `old`.
    pub(super) fn affect_neighbors_after_removal(&mut self, p: IVec3, old: Block, moved: bool) {
        let removed_tick = self.redstone.order.removed_tick.take() == Some(p);
        match r::component(old) {
            Some(Component::Wire(_)) if !moved => {
                for d in DIRECTIONS {
                    self.update_neighbors_at(p + d);
                }
                self.wire_update_power(p, old);
                self.update_neighbors_of_neighboring_wires(p);
            }
            Some(Component::Torch { .. }) if !moved => {
                for d in DIRECTIONS {
                    self.update_neighbors_at(p + d);
                }
            }
            Some(Component::Repeater { facing, .. } | Component::Comparator { facing, .. }) if !moved => {
                self.update_neighbors_in_front(p, facing.offset());
            }
            Some(Component::Observer { facing, on: true }) if removed_tick => {
                self.update_neighbors_in_front(p, -r::direction(facing));
            }
            Some(Component::Lever { mount, on: true } | Component::Button { mount, on: true, .. }) if !moved => {
                self.update_attached_neighbours(p, mount);
            }
            Some(Component::Plate { power, .. }) if !moved && power > 0 => {
                self.update_neighbors_at(p);
                self.update_neighbors_at(p - IVec3::Y);
            }
            // `PistonHeadBlock.affectNeighborsAfterRemoval`: a removed head
            // destroys the extended piston of its kind behind it, with drops.
            Some(Component::PistonHead { facing, sticky }) => {
                let base = p - r::direction(facing);
                let fitting = r::piston(facing, sticky, true);
                if self.get_block(base) == Some(fitting) {
                    self.destroy_redstone_block(base, fitting);
                }
            }
            _ => {}
        }
    }

    /// `BlockBehaviour.onPlace` for `new`, called for every state change.
    pub(super) fn redstone_on_place(&mut self, p: IVec3, old: Block, new: Block) {
        let changed_block = family(old) != family(new);
        match r::component(new) {
            Some(Component::Wire(_)) if changed_block => {
                self.wire_update_power(p, new);
                for d in VERTICAL {
                    self.update_neighbors_at(p + d);
                }
                self.update_neighbors_of_neighboring_wires(p);
            }
            Some(Component::Torch { .. }) => {
                for d in DIRECTIONS {
                    self.update_neighbors_at(p + d);
                }
            }
            Some(Component::Repeater { facing, .. } | Component::Comparator { facing, .. }) => {
                self.update_neighbors_in_front(p, facing.offset());
            }
            Some(Component::Observer { facing, on: true })
                if changed_block && !self.redstone.pending.contains_key(&p) =>
            {
                self.set_block_flags(p, r::observer(facing, false), flags::UPDATE_CLIENTS | flags::UPDATE_KNOWN_SHAPE);
                self.update_neighbors_in_front(p, -r::direction(facing));
            }
            // `PistonBaseBlock.onPlace`: `checkIfExtend`.
            Some(Component::Piston { .. }) if changed_block => self.automation_update(p, new),
            _ => {}
        }
    }

    /// Notifications `World::edit` adds for strong sources changed outside
    /// their ported Java behaviour: a switch or plate written directly in a
    /// new powered state updates like its own toggle, and sensors whose
    /// update calls are not ported update the block they strongly power.
    pub(super) fn compat_strong_updates(&mut self, p: IVec3, old: Block, new: Block) {
        let same = family(old) == family(new);
        // A newly placed block checks itself once at the start of the next
        // tick (before the clock advances), as the onPlace,
        // setPlacedBy and getStateForPlacement checks of blocks without a
        // ported Java behaviour do (hoppers, rails, TNT, detectors, lamps).
        // Diodes follow `DiodeBlock.setPlacedBy`; wire, torches, observers
        // and pistons have their own `onPlace`.
        if !same {
            match r::component(new) {
                Some(Component::Repeater { facing, on, .. }) if !on && self.diode_input(p, facing) > 0 => {
                    self.schedule_redstone(p, 1, priority::NORMAL);
                }
                Some(Component::Comparator { facing, subtract, on })
                    if !on && self.comparator_turns_on(p, facing, subtract) =>
                {
                    self.schedule_redstone(p, 1, priority::NORMAL);
                }
                // Own `onPlace`, or no placement check in Java.
                Some(
                    Component::Wire(_)
                    | Component::Torch { .. }
                    | Component::Observer { .. }
                    | Component::Piston { .. }
                    | Component::Repeater { .. }
                    | Component::Comparator { .. }
                    | Component::Source
                    | Component::Lever { .. }
                    | Component::Button { .. }
                    | Component::Plate { .. }
                    | Component::Target(_)
                    | Component::GlowingOre(_)
                    | Component::PistonHead { .. }
                    | Component::Moving,
                ) => {}
                _ if participates(new) => self.queue_refresh(p),
                _ => {}
            }
        }
        match (r::component(old), r::component(new)) {
            (_, Some(Component::Lever { mount, on } | Component::Button { mount, on, .. })) => {
                let was = same
                    && matches!(
                        r::component(old),
                        Some(Component::Lever { on: true, .. } | Component::Button { on: true, .. })
                    );
                if on != was {
                    self.update_attached_neighbours(p, mount);
                }
            }
            (_, Some(Component::Plate { power, .. })) => {
                let was = match r::component(old) {
                    Some(Component::Plate { power, .. }) if same => power,
                    _ => 0,
                };
                if (power > 0) != (was > 0) || (power != was && power > 0) {
                    self.update_neighbors_at(p);
                    self.update_neighbors_at(p - IVec3::Y);
                }
            }
            _ => {
                if let Some(d) = compat_strong_side(new).or_else(|| compat_strong_side(old)) {
                    self.update_neighbors_at(p + d);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::Stack;
    use crate::item::Item;
    use crate::world::chunk::ChunkData;
    use crate::world::terrain::Generator;
    use std::sync::Arc;

    const AT: IVec3 = IVec3::new(8, 145, 8);
    fn world() -> World {
        let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
        w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        w
    }
    fn put(w: &mut World, p: IVec3, b: Block) {
        assert!(w.edit(p, b, false));
    }
    fn ticks(w: &mut World, n: usize) {
        for _ in 0..n {
            w.tick_redstone();
        }
    }
    fn on(w: &World, p: IVec3) -> bool {
        matches!(
            w.get_block(p).and_then(r::component),
            Some(Component::Repeater { on: true, .. } | Component::Torch { lit: true, .. } | Component::Lamp(true))
        )
    }
    fn supported(w: &mut World, p: IVec3, b: Block) {
        put(w, p - IVec3::Y, Block::STONE);
        put(w, p, b);
    }

    #[test]
    fn wire_decays_fifteen_steps_and_clears_loops_without_recursion() {
        let mut w = world();
        for i in 0..17 {
            supported(&mut w, AT + IVec3::X * i, r::WIRE);
        }
        put(&mut w, AT - IVec3::X, r::REDSTONE_BLOCK);
        ticks(&mut w, 1);
        for i in 0..17 {
            assert_eq!(w.get_block(AT + IVec3::X * i), Some(r::wire(15u8.saturating_sub(i as u8))));
        }
        put(&mut w, AT - IVec3::X, Block::AIR);
        ticks(&mut w, 1);
        for i in 0..17 {
            assert_eq!(w.get_block(AT + IVec3::X * i), Some(r::WIRE));
        }
        ticks(&mut w, 1);
        assert_eq!(w.redstone_updates_last_tick(), 0, "idle circuits cost no updates");
    }

    #[test]
    fn weak_and_strong_power_do_not_chain_through_conductors() {
        let mut w = world();
        put(&mut w, AT, Block::STONE);
        put(&mut w, AT + IVec3::Y, r::lever(0, true));
        assert_eq!(w.redstone_power(AT + IVec3::X), 15, "lever strongly powers its support");
        put(&mut w, AT + IVec3::X, Block::STONE);
        assert_eq!(w.redstone_power(AT + IVec3::X * 2), 0, "conduction stops after one solid block");
        put(&mut w, AT + IVec3::Y, r::REDSTONE_BLOCK);
        assert_eq!(w.redstone_power(AT + IVec3::X), 0, "redstone block emits only weak power");
        assert_eq!(w.redstone_power(AT + IVec3::Y + IVec3::X), 15);
    }

    #[test]
    fn slime_blocks_conduct_like_solid_blocks() {
        let mut w = world();
        put(&mut w, AT, crate::world::stone_blocks::SLIME_BLOCK);
        put(&mut w, AT + IVec3::Y, r::lever(0, true));
        assert_eq!(w.redstone_power(AT + IVec3::X), 15, "lever strongly powers slime");
        put(&mut w, AT, crate::world::stone_blocks::TINTED_GLASS);
        assert_eq!(w.redstone_power(AT + IVec3::X), 0, "tinted glass never conducts");
        // Chests have an inset collision shape, so they don't conduct either.
        use crate::world::copper::{Family, Stage, block};
        for chest in [
            Block::CHEST,
            block(Family::Chest, Stage::Exposed, 0),
            crate::world::utilities::TRAPPED_CHEST,
            crate::world::utilities::ENDER_CHEST,
        ] {
            w.set_block(AT, chest);
            assert_eq!(w.redstone_power(AT + IVec3::X), 0, "{chest:?} doesn't conduct");
        }
    }

    #[test]
    fn powered_wire_strongly_powers_its_support_and_pointed_solid() {
        let mut w = world();
        // A placed wire takes its power at once (`RedStoneWireBlock.onPlace`),
        // so feed it 9 through six wires from a redstone block.
        for i in 0..7 {
            supported(&mut w, AT - IVec3::X * i, r::WIRE);
        }
        put(&mut w, AT - IVec3::X * 7, r::REDSTONE_BLOCK);
        assert_eq!(w.get_block(AT), Some(r::wire(9)));
        put(&mut w, AT + IVec3::X, Block::STONE);
        assert_eq!(w.redstone_power(AT - IVec3::Y + IVec3::Z), 9);
        assert_eq!(w.redstone_power(AT + IVec3::X * 2), 9);
        assert_eq!(w.redstone_power(AT + IVec3::Y), 0);
    }

    #[test]
    fn torch_inverts_after_two_game_ticks_and_burns_out_after_eight_toggles() {
        let mut w = world();
        supported(&mut w, AT, r::TORCH);
        // Edits between ticks update synchronously, so a torch inverts on
        // the second tick after its input changes, as in Java.
        for n in 0..8 {
            put(&mut w, AT - IVec3::Y + IVec3::X, r::lever(3, true));
            ticks(&mut w, 1);
            assert!(on(&w, AT));
            ticks(&mut w, 1);
            assert!(!on(&w, AT));
            put(&mut w, AT - IVec3::Y + IVec3::X, r::lever(3, false));
            ticks(&mut w, 2);
            assert_eq!(on(&w, AT), n < 7);
        }
        // `RedstoneTorchBlock.tick`: the eighth toggle in 60 ticks schedules
        // a 160-tick tick, which relights it once the toggles age out.
        assert_eq!(w.redstone.toggles[&AT].len(), 8);
        ticks(&mut w, 157);
        assert!(!on(&w, AT));
        ticks(&mut w, 1);
        assert!(on(&w, AT));
    }

    #[test]
    fn repeaters_delay_lock_and_stretch_short_pulses() {
        let mut w = world();
        supported(&mut w, AT, r::repeater(Facing::East, 3, false));
        put(&mut w, AT - IVec3::X, r::REDSTONE_BLOCK);
        // Edits update synchronously: the 6-tick delay counts from the edit.
        ticks(&mut w, 5);
        assert!(!on(&w, AT));
        ticks(&mut w, 1);
        assert!(on(&w, AT));
        supported(&mut w, AT + IVec3::Z, r::repeater(Facing::North, 1, true));
        put(&mut w, AT + IVec3::Z * 2, r::REDSTONE_BLOCK);
        assert!(w.repeater_locked(AT));
        put(&mut w, AT - IVec3::X, Block::AIR);
        ticks(&mut w, 10);
        assert!(on(&w, AT));
        put(&mut w, AT + IVec3::Z, Block::AIR);
        ticks(&mut w, 7);
        assert!(!on(&w, AT));
        put(&mut w, AT - IVec3::X, r::REDSTONE_BLOCK);
        ticks(&mut w, 1);
        put(&mut w, AT - IVec3::X, Block::AIR);
        ticks(&mut w, 6);
        assert!(on(&w, AT), "scheduled rising edges survive short input pulses");
        ticks(&mut w, 6);
        assert!(!on(&w, AT));
    }

    #[test]
    fn unloaded_due_ticks_sleep_until_chunk_load_and_keep_v1_saves() {
        let mut w = world();
        supported(&mut w, AT, r::STONE_BUTTON);
        assert!(w.use_redstone(AT));
        let chunk = super::super::chunk::chunk_of(AT);
        let data = w.chunks[&chunk].data.clone();
        let deadline = w.redstone.pending[&AT];
        w.remove_chunk(chunk);
        ticks(&mut w, 20);
        assert_eq!(w.redstone.pending[&AT], deadline);
        assert!(w.redstone.scheduled.is_empty());
        assert_eq!(w.redstone.suspended[&chunk].len(), 1);
        ticks(&mut w, 100);
        assert_eq!(w.redstone.last_updates, 0, "idle unloaded ticks do no work");
        assert_eq!(w.redstone.pending[&AT], deadline);
        let saved = w.redstone_to_string();
        // Java saves scheduled ticks as delays; unloaded time doesn't count.
        assert!(saved.contains("t,8,145,8,20,0,"), "unloaded work keeps its remaining delay: {saved}");
        let mut restored = world();
        restored.remove_chunk(chunk);
        restored.load_redstone(&saved);
        ticks(&mut restored, 50);
        assert!(restored.redstone.scheduled.is_empty());
        restored.insert_chunk(chunk, data.clone(), true);
        // The restored tick keeps its 20-tick delay from when the chunk loads.
        ticks(&mut restored, 19);
        assert!(restored.redstone.pending.contains_key(&AT), "released early after restore");
        ticks(&mut restored, 1);
        assert_eq!(restored.get_block(AT), Some(r::STONE_BUTTON));
        assert!(!restored.redstone.pending.contains_key(&AT));
        assert!(restored.redstone.suspended.is_empty());
        w.insert_chunk(chunk, data, true);
        // Replacing the button invalidates the suspended tick's family/sequence.
        put(&mut w, AT, r::LAMP);
        ticks(&mut w, 1);
        assert_eq!(w.get_block(AT), Some(r::LAMP));
    }

    #[test]
    fn comparators_compare_subtract_and_read_container_fullness() {
        let mut w = world();
        supported(&mut w, AT, r::comparator(Facing::East, false, false));
        put(&mut w, AT - IVec3::X, Block::CHEST);
        w.chest_mut(AT - IVec3::X).unwrap().slots[0] = Some(Stack::new(Item::DIAMOND, 64));
        assert_eq!(w.container_signal(AT - IVec3::X), Some(1));
        ticks(&mut w, 3);
        assert_eq!(w.redstone_power(AT + IVec3::X), 1);
        // A redstone block on the side reads 15 (`getControlInputSignal`).
        put(&mut w, AT + IVec3::Z, r::REDSTONE_BLOCK);
        assert_eq!(w.comparator_target(AT, Facing::East, false), 0);
        assert_eq!(w.comparator_target(AT, Facing::East, true), 0);
        put(&mut w, AT + IVec3::Z, Block::AIR);
        assert_eq!(w.comparator_target(AT, Facing::East, false), 1);
        w.chest_mut(AT - IVec3::X).unwrap().slots.fill(Some(Stack::new(Item::DIAMOND, 64)));
        assert_eq!(w.container_signal(AT - IVec3::X), Some(15));
    }

    #[test]
    fn comparators_read_an_item_frame_on_the_far_side_of_a_conductor() {
        let mut w = world();
        supported(&mut w, AT, r::comparator(Facing::East, false, false));
        put(&mut w, AT - IVec3::X, Block::STONE);
        let frame = (AT - IVec3::X * 2, IVec3::NEG_X);
        // A frame facing the comparator, or hung elsewhere, doesn't count.
        w.refresh_frame_signals([((frame.0, IVec3::X), 5), ((frame.0, IVec3::Y), 6)]);
        ticks(&mut w, 3);
        assert_eq!(w.redstone_power(AT + IVec3::X), 0);
        // `ItemFrame.getAnalogOutput`: rotation 3 reads 4.
        w.refresh_frame_signals([(frame, 4)]);
        ticks(&mut w, 3);
        assert_eq!(w.redstone_power(AT + IVec3::X), 4);
        // The larger of the frame and a container behind it.
        put(&mut w, frame.0, Block::CHEST);
        w.chest_mut(frame.0).unwrap().slots.fill(Some(Stack::new(Item::DIAMOND, 64)));
        ticks(&mut w, 3);
        assert_eq!(w.redstone_power(AT + IVec3::X), 15);
        put(&mut w, frame.0, Block::AIR);
        w.refresh_frame_signals([]);
        ticks(&mut w, 3);
        assert_eq!(w.redstone_power(AT + IVec3::X), 0, "a removed frame stops the signal");
    }

    #[test]
    fn lamp_delays_turning_off_and_tnt_primes_once() {
        let mut w = world();
        put(&mut w, AT, r::LAMP);
        put(&mut w, AT - IVec3::X, r::REDSTONE_BLOCK);
        ticks(&mut w, 1);
        assert!(on(&w, AT));
        put(&mut w, AT - IVec3::X, Block::AIR);
        // `RedstoneLampBlock`: off four ticks after the edit.
        ticks(&mut w, 3);
        assert!(on(&w, AT));
        ticks(&mut w, 1);
        assert!(!on(&w, AT));
        put(&mut w, AT, Block::TNT);
        put(&mut w, AT - IVec3::X, r::REDSTONE_BLOCK);
        ticks(&mut w, 10);
        assert_eq!(w.primed_tnt, vec![(AT, false)]);
    }

    #[test]
    fn powered_tnt_stays_a_block_without_tnt_explodes() {
        let mut w = world();
        w.rules.tnt_explodes = false;
        put(&mut w, AT, Block::TNT);
        put(&mut w, AT - IVec3::X, r::REDSTONE_BLOCK);
        ticks(&mut w, 10);
        assert!(w.primed_tnt.is_empty());
        assert_eq!(w.get_block(AT), Some(Block::TNT), "Java's `TntBlock.prime` fails, leaving the block");
    }

    #[test]
    fn ticks_cancel_on_replacement_and_round_trip_with_toggles_and_queues() {
        let mut w = world();
        supported(&mut w, AT, r::repeater(Facing::East, 4, false));
        put(&mut w, AT - IVec3::X, r::REDSTONE_BLOCK);
        ticks(&mut w, 2);
        let save = w.redstone_to_string();
        let mut restored = world();
        for (pos, slot) in &w.chunks {
            restored.insert_chunk(*pos, slot.data.clone(), true);
        }
        restored.load_redstone(&save);
        ticks(&mut restored, 5);
        assert!(!on(&restored, AT));
        ticks(&mut restored, 1);
        assert!(on(&restored, AT));
        put(&mut w, AT, r::LAMP);
        ticks(&mut w, 10);
        assert_eq!(w.get_block(AT), Some(Block(1211)));
        assert!(!w.redstone.pending.contains_key(&AT));
        w.redstone.toggles.entry(AT).or_default().push_back(w.redstone.tick - 2);
        let save = w.redstone_to_string();
        assert!(save.starts_with("v2,"), "{save}");
        restored.load_redstone(&save);
        // Java never saves `RECENT_TOGGLES`: a reload starts with no history.
        assert!(restored.redstone.toggles.is_empty());
        restored.redstone.toggles = w.redstone.toggles.clone();
        restored.load_redstone("broken");
        assert_eq!(restored.redstone.toggles, w.redstone.toggles);
        // A v1 save's burnout deadline is dropped; its history and pending tick still load.
        restored.load_redstone("v1,40,3|b,8,145,8,120|h,8,145,8,2");
        assert_eq!(restored.redstone.tick, 40);
        assert_eq!(restored.redstone.toggles[&AT], VecDeque::from([38]));
    }

    #[test]
    fn wire_connects_up_steps_and_does_not_connect_across_a_ceiling() {
        let mut w = world();
        supported(&mut w, AT, r::WIRE);
        supported(&mut w, AT + IVec3::X + IVec3::Y, r::WIRE);
        put(&mut w, AT - IVec3::X, r::REDSTONE_BLOCK);
        ticks(&mut w, 1);
        assert_eq!(w.get_block(AT + IVec3::X + IVec3::Y), Some(r::wire(14)));
        assert_eq!(w.wire_connections(AT)[Facing::East as usize], 2);
        put(&mut w, AT + IVec3::Y, Block::STONE);
        ticks(&mut w, 1);
        assert_eq!(w.get_block(AT + IVec3::X + IVec3::Y), Some(r::WIRE));
    }

    #[test]
    fn buttons_release_on_java_delays_and_arrows_hold_only_wood_buttons() {
        let mut w = world();
        let mut entities = crate::entity::Entities::new(7);
        supported(&mut w, AT, r::STONE_BUTTON);
        assert!(w.use_redstone(AT));
        ticks(&mut w, 19);
        assert!(matches!(w.get_block(AT).and_then(r::component), Some(Component::Button { on: true, .. })));
        ticks(&mut w, 1);
        assert_eq!(w.get_block(AT), Some(r::STONE_BUTTON));
        put(&mut w, AT, r::WOOD_BUTTON);
        // An arrow inside the cell but above the thin button model does not touch it.
        let high = AT.as_dvec3() + glam::DVec3::new(0.5, 0.6, 0.5);
        entities.arrows.push(crate::entity::Arrow::shot(high, glam::DVec3::X, 0.1, false));
        entities.arrows[0].pos = high;
        w.redstone_contacts([], &entities);
        ticks(&mut w, 2);
        assert_eq!(w.get_block(AT), Some(r::WOOD_BUTTON));
        entities.arrows.clear();
        let pos = AT.as_dvec3() + glam::DVec3::new(0.5, 0.04, 0.5);
        entities.arrows.push(crate::entity::Arrow::shot(pos, glam::DVec3::X, 0.1, false));
        entities.arrows[0].pos = pos;
        w.redstone_contacts([], &entities);
        ticks(&mut w, 30);
        assert!(matches!(w.get_block(AT).and_then(r::component), Some(Component::Button { on: true, .. })));
        entities.arrows.clear();
        w.redstone_contacts([], &entities);
        ticks(&mut w, 30);
        assert_eq!(w.get_block(AT), Some(r::WOOD_BUTTON));
        put(&mut w, AT, r::STONE_BUTTON);
        entities.arrows.push(crate::entity::Arrow::shot(pos, glam::DVec3::X, 0.1, false));
        entities.arrows[0].pos = pos;
        w.redstone_contacts([], &entities);
        assert_eq!(w.get_block(AT), Some(r::STONE_BUTTON));
    }

    #[test]
    fn wooden_plates_feel_orbs_tnt_potions_bobbers_and_other_projectiles_but_stone_plates_do_not() {
        use crate::entity::{Entities, PlayerId, Rng, fireball::Fireball, orb::XpOrb, potion::ThrownPotion};
        let mid = AT.as_dvec3() + glam::DVec3::new(0.5, 0.0, 0.5);
        let mut rng = Rng::new(3);
        type AddEntity = Box<dyn Fn(&mut Entities, &mut Rng)>;
        let cases: [(&str, AddEntity); 5] = [
            ("orb", Box::new(move |e, r| e.orbs.push(XpOrb::new(3, mid + glam::DVec3::Y * 0.1, r)))),
            ("tnt", Box::new(move |e, _| e.tnt.push(crate::entity::tnt::PrimedTnt::new(mid, glam::DVec3::ZERO, 4.0)))),
            (
                "potion",
                Box::new(move |e, _| {
                    e.potions.push(ThrownPotion::new(
                        crate::potion::Potion::from_id("harming").unwrap(),
                        None,
                        mid,
                        glam::DVec3::ZERO,
                    ))
                }),
            ),
            (
                "bobber",
                Box::new(move |e, _| {
                    e.cast_bobber(PlayerId::HOST, mid + glam::DVec3::Y * 0.1, glam::DVec3::ZERO, 0, 0);
                    e.bobbers[0].pos = mid + glam::DVec3::Y * 0.1;
                }),
            ),
            (
                "fireball",
                Box::new(move |e, _| e.fireballs.push(Fireball::new(mid + glam::DVec3::Y * 0.1, glam::DVec3::X))),
            ),
        ];
        for (name, add) in &cases {
            let mut entities = Entities::new(7);
            add(&mut entities, &mut rng);
            let mut w = world();
            supported(&mut w, AT, r::WOOD_PLATE);
            w.redstone_contacts([], &entities);
            assert_eq!(w.get_block(AT), Some(r::plate(1, 15)), "wooden plate ignores {name}");
            let mut w = world();
            supported(&mut w, AT, r::STONE_PLATE);
            w.redstone_contacts([], &entities);
            assert_eq!(w.get_block(AT), Some(r::STONE_PLATE), "stone plate counts {name}");
        }
        let reference: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/redstone_contacts.json")).unwrap();
        let orb_width = reference["entity_sizes"]["experience_orb"][0].as_f64().unwrap();
        let touch_edge = reference["touch_max"][0].as_f64().unwrap();
        // Place an orb 0.01 blocks inside/outside the reference's far overlap edge.
        // Java's touch box runs x 1/16..15/16: a 0.5-wide orb centred at 1.15 touches it, at 1.2 it does not.
        let mut entities = Entities::new(7);
        entities.orbs.push(XpOrb::new(
            3,
            AT.as_dvec3() + glam::DVec3::new(touch_edge + orb_width / 2.0 - 0.01, 0.1, 0.5),
            &mut rng,
        ));
        let mut w = world();
        supported(&mut w, AT, r::WOOD_PLATE);
        w.redstone_contacts([], &entities);
        assert_eq!(w.get_block(AT), Some(r::plate(1, 15)));
        entities.orbs[0].pos = AT.as_dvec3() + glam::DVec3::new(touch_edge + orb_width / 2.0 + 0.01, 0.1, 0.5);
        let mut w = world();
        supported(&mut w, AT, r::WOOD_PLATE);
        w.redstone_contacts([], &entities);
        assert_eq!(w.get_block(AT), Some(r::WOOD_PLATE));
    }

    #[test]
    fn pressure_plates_filter_living_entities_and_count_item_entities_not_stack_size() {
        let mut w = world();
        let mut entities = crate::entity::Entities::new(7);
        let mut rng = crate::entity::Rng::new(7);
        let pos = AT.as_dvec3() + glam::DVec3::new(0.5, 0.1, 0.5);
        entities.items.push(crate::entity::ItemEntity::new(
            Stack::new(Item::DIAMOND, 64),
            pos,
            glam::DVec3::ZERO,
            0.0,
            &mut rng,
        ));
        supported(&mut w, AT, r::STONE_PLATE);
        w.redstone_contacts([], &entities);
        assert_eq!(w.get_block(AT), Some(r::STONE_PLATE));
        w.redstone_contacts([(pos, crate::player::SHAPE)], &entities);
        assert_eq!(w.get_block(AT), Some(r::plate(0, 15)));
        w.redstone_contacts([], &entities);
        ticks(&mut w, 20);
        assert_eq!(w.get_block(AT), Some(r::STONE_PLATE));
        put(&mut w, AT, r::WOOD_PLATE);
        w.redstone_contacts([], &entities);
        assert_eq!(w.get_block(AT), Some(r::plate(1, 15)));
        put(&mut w, AT, r::LIGHT_PLATE);
        w.redstone_contacts([], &entities);
        assert_eq!(w.get_block(AT), Some(r::plate(2, 1)));
        for _ in 0..10 {
            entities.items.push(crate::entity::ItemEntity::new(
                Stack::new(Item::DIAMOND, 64),
                pos,
                glam::DVec3::ZERO,
                0.0,
                &mut rng,
            ));
        }
        put(&mut w, AT, r::HEAVY_PLATE);
        w.redstone_contacts([], &entities);
        assert_eq!(w.get_block(AT), Some(r::plate(3, 2)));
        entities.items.clear();
        w.redstone_contacts([], &entities);
        ticks(&mut w, 10);
        assert_eq!(w.get_block(AT), Some(r::HEAVY_PLATE));
    }

    #[test]
    fn iron_doors_and_trapdoors_open_only_with_power_and_close_on_removal() {
        let mut w = world();
        put(&mut w, AT - IVec3::Y, Block::STONE);
        assert!(w.place_iron_door(AT, Facing::East));
        assert!(w.use_redstone(AT));
        assert_eq!(w.get_block(AT), Some(r::iron_door(Facing::East, false, false)));
        put(&mut w, AT - IVec3::X, r::REDSTONE_BLOCK);
        ticks(&mut w, 1);
        assert_eq!(w.get_block(AT), Some(r::iron_door(Facing::East, true, false)));
        assert_eq!(w.get_block(AT + IVec3::Y), Some(r::iron_door(Facing::East, true, true)));
        put(&mut w, AT - IVec3::X, Block::AIR);
        ticks(&mut w, 1);
        assert_eq!(w.get_block(AT), Some(r::iron_door(Facing::East, false, false)));
        put(&mut w, AT, r::IRON_TRAPDOOR);
        assert!(w.use_redstone(AT));
        assert_eq!(w.get_block(AT), Some(r::IRON_TRAPDOOR));
        put(&mut w, AT - IVec3::X, r::REDSTONE_BLOCK);
        ticks(&mut w, 1);
        assert!(matches!(w.get_block(AT).and_then(r::component), Some(Component::Trapdoor { open: true, .. })));
    }

    #[test]
    fn targets_score_face_centres_and_ignore_hits_until_pulse_ends() {
        let mut w = world();
        put(&mut w, AT, r::TARGET);
        assert!(w.hit_redstone_target(AT, AT.as_dvec3() + glam::DVec3::new(0.0, 0.5, 0.5), IVec3::NEG_X, true));
        assert_eq!(w.get_block(AT), Some(r::target(15)));
        w.hit_redstone_target(AT, AT.as_dvec3(), IVec3::NEG_X, false);
        assert_eq!(w.get_block(AT), Some(r::target(15)));
        ticks(&mut w, 19);
        assert_eq!(w.get_block(AT), Some(r::target(15)));
        ticks(&mut w, 1);
        assert_eq!(w.get_block(AT), Some(r::TARGET));
        w.hit_redstone_target(AT, AT.as_dvec3(), IVec3::NEG_X, false);
        assert_eq!(w.get_block(AT), Some(r::target(1)));
        ticks(&mut w, 8);
        assert_eq!(w.get_block(AT), Some(r::TARGET));
    }

    #[test]
    fn daylight_detectors_sample_periodically_and_ore_keeps_mining_rules() {
        use crate::simulation::lighting::{SpawnLight, sun_angle};
        // Detectors read propagated gameplay skylight, which needs lit columns.
        let mut w = super::super::lighting::flat_test_world(
            (-1..=1).flat_map(|x| (-1..=1).map(move |z| glam::IVec2::new(x, z))),
            63,
        );
        let at = IVec3::new(8, 64, 8);
        put(&mut w, at, r::DAYLIGHT);
        w.update_block_light();
        let clock = |w: &mut World, tick: u32| {
            let darken = SpawnLight::at(tick, 0., 0.).sky_darken;
            w.set_redstone_daylight(tick as f64 / 24000.0, darken, sun_angle(tick));
        };
        clock(&mut w, 6000);
        ticks(&mut w, 21);
        assert_eq!(w.get_block(at), Some(r::daylight(15, false)));
        // At dusk the shared darkening is still 0 (sky 15), but the sun angle
        // already halves the output: 15 * cos(0.8 * 77.6 degrees) rounds to 7.
        clock(&mut w, 12000);
        ticks(&mut w, 20);
        assert_eq!(SpawnLight::at(12000, 0., 0.).sky_darken, 0);
        assert_eq!(w.get_block(at), Some(r::daylight(7, false)));
        clock(&mut w, 18000);
        ticks(&mut w, 20);
        assert_eq!(w.get_block(at), Some(r::daylight(0, false)));
        assert!(w.use_redstone(at));
        ticks(&mut w, 20);
        assert_eq!(w.get_block(at), Some(r::daylight(11, true)), "15 - (15 - 11)");
        // A roof removes the sky light the inverted detector compares.
        put(&mut w, at + IVec3::Y * 3, Block::STONE);
        w.update_block_light();
        ticks(&mut w, 20);
        let sky = w.sky_light(at).unwrap();
        assert!(sky < 15, "{sky}");
        assert_eq!(w.get_block(at), Some(r::daylight(15 - sky.saturating_sub(11), true)));

        let mut w = world();
        put(&mut w, AT, Block::DEEPSLATE_REDSTONE_ORE);
        assert!(w.touch_redstone_ore(AT));
        let glowing = w.get_block(AT).unwrap();
        assert_eq!(glowing.emission(), 9);
        assert_eq!(glowing.harvest_level(), Some(2));
        assert_eq!(glowing.as_stone_ore(), Block::REDSTONE_ORE);
        // Java: no fixed expiry, and re-touching a lit ore schedules nothing.
        assert!(w.touch_redstone_ore(AT));
        ticks(&mut w, 100);
        assert_eq!(w.get_block(AT), Some(glowing));
        // `RedStoneOreBlock.randomTick` turns LIT off.
        w.random_tick(AT);
        assert_eq!(w.get_block(AT), Some(Block::DEEPSLATE_REDSTONE_ORE));
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/building.json")).unwrap();
        assert_eq!(fixture["redstone_ore_extinguishes_on_random_tick"], true);
    }

    #[test]
    fn isolated_wire_toggles_dot_without_horizontal_power_and_mounts_have_no_collision() {
        let mut w = world();
        supported(&mut w, AT, r::WIRE);
        assert_eq!(w.wire_connections(AT), [1; 4]);
        assert!(w.use_redstone(AT));
        assert_eq!(w.wire_connections(AT), [0; 4]);
        put(&mut w, AT, r::dot(7));
        assert_eq!(w.redstone_power(AT + IVec3::X), 0);
        assert_eq!(w.redstone_power(AT - IVec3::Y + IVec3::X), 7);
        for b in [r::WIRE, r::LEVER, r::STONE_BUTTON, r::TORCH, r::STONE_PLATE] {
            assert!(super::super::shape::collision(b, |_| Block::AIR, Block::AIR).is_empty());
        }
    }

    #[test]
    fn generic_agent_place_uses_a_lever_with_empty_hands() {
        let mut w = world();
        supported(&mut w, AT, r::LEVER);
        let mut entities = crate::entity::Entities::new(7);
        let mut a = crate::agent::Agent::new(AT.as_dvec3() + glam::DVec3::new(-2.0, 0.0, 0.5));
        a.player.yaw = 0.0;
        a.player.pitch = -0.45;
        assert_eq!(a.target(&w).map(|(p, _)| p), Some(AT));
        a.inventory.slots.fill(None);
        a.execute(crate::agent::Command::Place, &mut w, &mut entities, &[]).unwrap();
        assert_eq!(w.get_block(AT), Some(r::lever(0, true)));
    }

    #[test]
    #[ignore = "manual changed-grid benchmark"]
    fn dust_grid_benchmark() {
        let mut w = world();
        for x in 0..4 {
            for z in 0..4 {
                w.insert_chunk(IVec3::new(x, 4, z), Arc::new(ChunkData::Uniform(Block::AIR)), false);
            }
        }
        for x in 0..128 {
            for z in 0..128 {
                supported(&mut w, IVec3::new(x, 145, z), r::WIRE);
            }
        }
        ticks(&mut w, 2);
        // Neighbour updates run synchronously inside the edit, as in Java.
        let source = IVec3::new(64, 146, 64);
        for round in 0..3 {
            w.redstone.order.executed = 0;
            let start = std::time::Instant::now();
            put(&mut w, source, r::REDSTONE_BLOCK);
            eprintln!(
                "round {round}: 16384-wire grid power-up: {:?}, {} updates",
                start.elapsed(),
                w.redstone.order.executed
            );
            w.redstone.order.executed = 0;
            let start = std::time::Instant::now();
            put(&mut w, source, Block::AIR);
            eprintln!(
                "round {round}: 16384-wire grid power-down: {:?}, {} updates",
                start.elapsed(),
                w.redstone.order.executed
            );
        }
        let start = std::time::Instant::now();
        for _ in 0..1000 {
            std::hint::black_box(&mut w).tick_redstone();
        }
        eprintln!("idle grid: {:?}/tick, {} updates", start.elapsed() / 1000, w.redstone_updates_last_tick());
        // A 16-wire line, the common case.
        let mut w = world();
        for x in 0..16 {
            supported(&mut w, AT + IVec3::X * x, r::WIRE);
        }
        let start = std::time::Instant::now();
        for _ in 0..100 {
            put(&mut w, AT - IVec3::X, r::REDSTONE_BLOCK);
            put(&mut w, AT - IVec3::X, Block::AIR);
        }
        eprintln!("16-wire line on/off: {:?} per toggle pair", start.elapsed() / 100);
    }
}
