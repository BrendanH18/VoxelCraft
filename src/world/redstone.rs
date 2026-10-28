//! Changed-block redstone propagation and scheduled component ticks at 20 Hz.
//! Java references and intentional ordering differences: docs/redstone.md.
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

/// The block a scheduled tick is keyed by: the redstone, rail or gadget
/// family, or the plain base block (bells, composters).
fn family(b: Block) -> Block {
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

const SIDES: [IVec3; 6] = [IVec3::Y, IVec3::NEG_Y, IVec3::NEG_Z, IVec3::Z, IVec3::NEG_X, IVec3::X];
/// A pathological oscillator cannot monopolize a game tick. Work carries over.
const MAX_UPDATES: usize = 65_536;

type ScheduledTick = Reverse<(u64, i8, u64, [i32; 3], u16)>;

#[derive(Default)]
pub(super) struct RedstoneState {
    pub(super) contacts: FxHashMap<IVec3, super::redstone_contacts::Contacts>,
    pub(super) tick: u64,
    sequence: u64,
    updates: VecDeque<IVec3>,
    queued: FxHashSet<IVec3>,
    pending: FxHashMap<IVec3, (u64, u16, u64)>,
    scheduled: BinaryHeap<ScheduledTick>,
    /// Due work in unloaded chunks. Woken only by a chunk load, never retried each tick.
    suspended: FxHashMap<IVec3, Vec<ScheduledTick>>,
    sleeping_updates: FxHashMap<IVec3, Vec<IVec3>>,
    /// Pack relative scheduled delays while a storage chunk is unloaded.
    unloaded_at: FxHashMap<IVec3, u64>,
    /// Detector-rail comparator output, filled by the minecart tick.
    pub(super) cart_signal: FxHashMap<IVec3, u8>,
    /// Item frame comparator output by the frame's cell and facing, filled
    /// by the entity tick.
    frame_signal: FxHashMap<(IVec3, IVec3), u8>,
    pub(super) trip_pulse: FxHashMap<IVec3, u64>,
    toggles: FxHashMap<IVec3, VecDeque<u64>>,
    burnout: FxHashMap<IVec3, u64>,
    comparator: FxHashMap<IVec3, u8>,
    /// Tracks powered edges for manually operable doors and gates.
    powered: FxHashSet<IVec3>,
    pub last_updates: usize,
    day_time: f64,
    /// Java's `Level.skyDarken`, shared with spawning.
    sky_darken: u8,
    /// Java's `visual/sun_angle` in degrees.
    sun_angle: f32,
}

impl World {
    fn queue_redstone(&mut self, p: IVec3) {
        if self.get_block(p).is_some_and(|b| {
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
        }) && self.redstone.queued.insert(p)
        {
            self.redstone.updates.push_back(p);
        }
    }

    /// Includes neighbours of a potentially conducting block, and stair-step
    /// wire neighbours. Fixed fanout, no recursive neighbour notifications.
    pub fn redstone_changed(&mut self, p: IVec3) {
        self.queue_redstone(p);
        for d in SIDES {
            self.queue_redstone(p + d);
            for e in SIDES {
                self.queue_redstone(p + d + e);
            }
        }
    }

    pub(super) fn track_redstone(&mut self, p: IVec3, old: Block, new: Block) {
        if old == new {
            return;
        }
        if family(old) != family(new) {
            self.redstone.pending.remove(&p);
            self.redstone.comparator.remove(&p);
            self.redstone.burnout.remove(&p);
            self.redstone.toggles.remove(&p);
            self.redstone.powered.remove(&p);
        }
        self.track_tripwire(p, old, new);
        self.redstone_changed(p);
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
        if let Some(ticks) = self.redstone.suspended.remove(&c) {
            self.redstone.scheduled.extend(ticks);
        }
        // Saved state needs its connections refreshed once when loaded, including
        // adjacent chunks. Generation never places redstone components.
        let Some(slot) = self.chunks.get(&c) else { return };
        if !slot.modified {
            return;
        }
        let data = slot.data.clone();
        let mut i = 0;
        data.for_each_block(|b| {
            if r::component(b).is_some()
                || b.is_door()
                || b.is_gate()
                || b.is_rail()
                || super::gadgets::participates(b)
                || b.base() == Block::BELL
                || super::copper::bulb(b).is_some()
            {
                let local =
                    IVec3::new(i % CHUNK_SIZE_I, i / (CHUNK_SIZE_I * CHUNK_SIZE_I), i / CHUNK_SIZE_I % CHUNK_SIZE_I);
                self.redstone_changed(c * CHUNK_SIZE_I + local);
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
                let q = p + d;
                if repeaters_only
                    && !matches!(
                        self.get_block(q).and_then(r::component),
                        Some(Component::Repeater { .. } | Component::Comparator { .. })
                    )
                {
                    0
                } else {
                    self.emitted_signal(q, -d, true, true)
                }
            })
            .max()
            .unwrap_or(0)
    }

    pub fn repeater_locked(&self, p: IVec3) -> bool {
        matches!(self.get_block(p).and_then(r::component), Some(Component::Repeater { facing, .. }) if self.diode_side(p, facing, true) > 0)
    }

    fn comparator_target(&self, p: IVec3, facing: Facing, subtract: bool) -> u8 {
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
        let side = self.diode_side(p, facing, false);
        if subtract {
            input.saturating_sub(side)
        } else if input >= side {
            input
        } else {
            0
        }
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
        if let Some(signal) = self.get_block(p).and_then(super::copper::bulb_signal) {
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
        if let Some(c) = self.chest(p) {
            let count = self.container_slots(p);
            return Some(strength(c.slots[..count].iter().copied(), count));
        }
        if let Some(f) = self.furnace(p) {
            return Some(strength([f.input, f.fuel, f.output].into_iter(), 3));
        }
        if let Some(b) = self.brewing_stand(p) {
            return Some(strength(b.bottles.into_iter().chain([b.ingredient, b.fuel]), 5));
        }
        None
    }

    fn redstone_update(&mut self, p: IVec3) {
        let Some(b) = self.get_block(p) else { return };
        if b.is_rail() {
            return self.rail_update(p, b);
        }
        if super::gadgets::is_note(b) {
            return self.note_neighbour(p, b);
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
        // `updateShape`: a lever, button or torch whose attached block no
        // longer has a sturdy face pops off; so does a diode or plate whose
        // support no longer holds it (`canSurvive`, as placement checks).
        let support = match r::component(b) {
            Some(Component::Lever { mount, .. } | Component::Button { mount, .. } | Component::Torch { mount, .. }) => {
                Some((r::support(mount), true))
            }
            Some(Component::Repeater { .. } | Component::Comparator { .. } | Component::Plate { .. }) => {
                Some((IVec3::NEG_Y, false))
            }
            _ => None,
        };
        if support.is_some_and(|(d, sturdy)| {
            self.get_block(p + d).is_some()
                && if sturdy {
                    !self.redstone_supported(p, b)
                } else {
                    self.get_block(p + d).is_some_and(|below| !b.can_stay_on(below))
                }
        }) {
            self.spill_block(p, b);
            self.edit(p, Block::AIR, false);
            return;
        }
        match r::component(b) {
            Some(
                Component::Piston { .. }
                | Component::Observer { .. }
                | Component::Dispenser { .. }
                | Component::Hopper { .. },
            ) => self.automation_update(p, b),
            Some(Component::Wire(power)) => {
                if self.get_block(p - IVec3::Y).is_some_and(|below| !b.can_stay_on(below)) {
                    self.spill_block(p, b);
                    self.edit(p, Block::AIR, false);
                } else {
                    let target = self.wire_target(p);
                    if power != target {
                        self.edit(p, if b.0 >= 1453 { r::dot(target) } else { r::wire(target) }, false);
                    }
                }
            }
            Some(Component::Torch { mount, lit }) => {
                if self.redstone.burnout.get(&p).is_some_and(|&end| end > self.redstone.tick) {
                    return;
                }
                if lit == (self.signal_from(p + r::support(mount), -r::support(mount), true) > 0) {
                    self.schedule_redstone(p, 2, 0);
                }
            }
            Some(Component::Repeater { facing, delay, on }) => {
                if !self.repeater_locked(p) && on != (self.diode_input(p, facing) > 0) {
                    let priority = if on { -2 } else { -1 };
                    self.schedule_redstone(p, delay as u64 * 2, priority);
                }
            }
            Some(Component::Comparator { facing, subtract, .. }) => {
                if self.redstone.comparator.get(&p).copied().unwrap_or(0) != self.comparator_target(p, facing, subtract)
                {
                    self.schedule_redstone(p, 2, 0);
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
                let powered = self.redstone_power(p) > 0;
                if !on && powered {
                    self.edit(p, Block(1211), false);
                } else if on && !powered {
                    self.schedule_redstone(p, 4, 0);
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
                        if let Some((facing, open)) = state
                            && open != powered
                        {
                            self.edit(lower, block.toggled(facing), false);
                            if b.is_door()
                                && let Some(upper) = self.get_block(lower + IVec3::Y).filter(|b| b.is_door_upper())
                            {
                                self.edit(lower + IVec3::Y, upper.toggled(facing), false);
                            }
                        }
                    }
                }
            }
            _ if super::wood::redstone_participant(b) => self.shelf_power(p),
            _ if b == Block::TNT && self.rules.tnt_explodes && self.redstone_power(p) > 0 => {
                self.edit(p, Block::AIR, false);
                self.primed_tnt.push((p, false));
            }
            _ => {}
        }
    }

    fn redstone_scheduled_tick(&mut self, p: IVec3) {
        let Some(b) = self.get_block(p) else { return };
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
                let powered = self.signal_from(p + r::support(mount), -r::support(mount), true) > 0;
                let burned = self.redstone.burnout.get(&p).is_some_and(|&end| end > self.redstone.tick);
                if lit && powered {
                    self.edit(p, r::torch(mount, false), false);
                    let history = self.redstone.toggles.entry(p).or_default();
                    while history.front().is_some_and(|&t| self.redstone.tick - t > 60) {
                        history.pop_front();
                    }
                    history.push_back(self.redstone.tick);
                    if history.len() >= 8 {
                        self.redstone.burnout.insert(p, self.redstone.tick + 160);
                        self.schedule_redstone(p, 160, 0);
                    }
                } else if !lit && !powered && !burned {
                    self.edit(p, r::torch(mount, true), false);
                }
            }
            Some(Component::Repeater { facing, delay, on }) if !self.repeater_locked(p) => {
                let input = self.diode_input(p, facing) > 0;
                if on && !input {
                    self.edit(p, r::repeater(facing, delay, false), false);
                } else if !on {
                    self.edit(p, r::repeater(facing, delay, true), false);
                    if !input {
                        self.schedule_redstone(p, delay as u64 * 2, -2);
                    }
                }
            }
            Some(Component::Comparator { facing, subtract, .. }) => {
                let output = self.comparator_target(p, facing, subtract);
                self.redstone.comparator.insert(p, output);
                self.edit(p, r::comparator(facing, subtract, output > 0), false);
                self.redstone_changed(p);
            }
            Some(Component::Button { mount, wood, on: true }) => {
                if wood && self.redstone.contacts.get(&p).is_some_and(|c| c.arrows) {
                    self.schedule_redstone(p, 30, 0);
                } else {
                    self.edit(p, keep_wood(b, r::button(mount, false, wood)), false);
                }
            }
            Some(Component::Plate { kind, .. }) => {
                let power = super::redstone_contacts::plate_power(
                    kind,
                    self.redstone.contacts.get(&p).copied().unwrap_or_default(),
                );
                if b != keep_wood(b, r::plate(kind, power)) {
                    self.edit(p, keep_wood(b, r::plate(kind, power)), false);
                }
                if power > 0 {
                    self.schedule_redstone(p, if kind < 2 { 20 } else { 10 }, 0);
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
                self.edit(p, r::LAMP, false);
            }
            _ => {}
        }
    }

    pub(super) fn unload_redstone_chunk(&mut self, c: IVec3) {
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

    /// One authoritative 50 ms game step. Neighbour work settles before and
    /// after due scheduled ticks, with stable insertion order for equal priority.
    /// No scanning of loaded blocks or allocation on an idle tick.
    pub fn tick_redstone(&mut self) {
        self.redstone.tick += 1;
        for (&p, due) in &mut self.automation.cooldown {
            if !self.activity.contains(p, true) || !self.chunks.contains_key(&super::chunk::chunk_of(p)) {
                *due = due.saturating_add(1);
            }
        }
        self.finish_piston_moves();
        self.redstone.last_updates = 0;
        self.drain_redstone_updates();
        while self.redstone.last_updates < MAX_UPDATES {
            let Some(&Reverse((due, priority, sequence, xyz, family))) = self.redstone.scheduled.peek() else { break };
            if due > self.redstone.tick {
                break;
            }
            self.redstone.scheduled.pop();
            let p = IVec3::from_array(xyz);
            if self.redstone.pending.get(&p) != Some(&(due, family, sequence)) {
                continue;
            }
            self.redstone.last_updates += 1;
            if !self.is_block_ticking(p) {
                self.redstone
                    .suspended
                    .entry(super::chunk::chunk_of(p))
                    .or_default()
                    .push(Reverse((due, priority, sequence, xyz, family)));
                continue;
            }
            self.redstone.pending.remove(&p);
            if self.get_block(p).map(self::family) == Some(Block(family)) {
                self.redstone_scheduled_tick(p);
                self.drain_redstone_updates();
            }
        }
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
                for (q, upper) in [(lower, false), (lower + IVec3::Y, true)] {
                    if let Some(block) = self.get_block(q)
                        && let Some(Component::IronDoor { facing, open, .. }) = r::component(block)
                        && open != on
                    {
                        let door = super::mount_states::keep_hinge(block, r::iron_door(facing, on, upper));
                        self.edit(q, door, false);
                    }
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
                    self.edit(p, keep_wood(b, r::trapdoor(facing, on, top, iron)), false);
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
        let deep = b.base() == Block::DEEPSLATE_REDSTONE_ORE;
        if !deep && b.base() != Block::REDSTONE_ORE {
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

    fn drain_redstone_updates(&mut self) {
        while self.redstone.last_updates < MAX_UPDATES {
            let Some(p) = self.redstone.updates.pop_front() else { break };
            self.redstone.last_updates += 1;
            if !self.is_block_ticking(p) {
                self.redstone.sleeping_updates.entry(super::chunk::chunk_of(p)).or_default().push(p);
                continue;
            }
            self.redstone.queued.remove(&p);
            self.redstone_update(p);
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
        let Some(b) = self.get_block(p) else { return false };
        if super::gadgets::is_note(b) {
            self.cycle_note(p, b);
            return true;
        }
        if b.base() == Block::BELL {
            return self.ring_bell(p);
        }
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
            Some(Component::Lever { mount, on }) => r::lever(mount, !on),
            Some(Component::Button { mount, on: false, wood }) => {
                self.schedule_redstone(p, if wood { 30 } else { 20 }, 0);
                keep_wood(b, r::button(mount, true, wood))
            }
            Some(Component::Button { on: true, .. }) => return true,
            Some(Component::Repeater { facing, delay, on }) => r::repeater(facing, delay % 4 + 1, on),
            Some(Component::Comparator { facing, subtract, on }) => r::comparator(facing, !subtract, on),
            _ => return false,
        };
        self.set_block(p, next)
    }

    /// Versioned dimension property. Pending ticks use relative deadlines;
    /// block ids already persist wire and device states in VXC2 chunks.
    pub fn redstone_to_string(&self) -> String {
        let mut entries = vec![format!("v1,{},{}", self.redstone.tick, self.redstone.sequence)];
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
        for (p, end) in &self.redstone.burnout {
            if *end > self.redstone.tick {
                entries.push(format!("b,{},{},{},{}", p.x, p.y, p.z, end - self.redstone.tick));
            }
        }
        for (p, history) in &self.redstone.toggles {
            for t in history {
                if self.redstone.tick - t <= 60 {
                    entries.push(format!("h,{},{},{},{}", p.x, p.y, p.z, self.redstone.tick - t));
                }
            }
        }
        for p in &self.redstone.powered {
            entries.push(format!("p,{},{},{}", p.x, p.y, p.z));
        }
        for (&p, &due) in &self.redstone.trip_pulse {
            if due > self.redstone.tick {
                entries.push(format!("w,{},{},{},{}", p.x, p.y, p.z, due - self.redstone.tick));
            }
        }
        entries.join("|")
    }

    pub fn load_redstone(&mut self, text: &str) {
        let mut entries = text.split('|');
        let Some(header) = entries.next() else { return };
        let Some(rest) = header.strip_prefix("v1,") else { return };
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
                ("b", &[delay]) if delay > 0 => {
                    self.redstone.burnout.insert(p, tick.saturating_add(delay as u64));
                }
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
        for chest in [Block::CHEST, block(Family::Chest, Stage::Exposed, 0)] {
            w.set_block(AT, chest);
            assert_eq!(w.redstone_power(AT + IVec3::X), 0, "{chest:?} doesn't conduct");
        }
    }

    #[test]
    fn powered_wire_strongly_powers_its_support_and_pointed_solid() {
        let mut w = world();
        supported(&mut w, AT, r::wire(9));
        put(&mut w, AT + IVec3::X, Block::STONE);
        assert_eq!(w.redstone_power(AT - IVec3::Y + IVec3::Z), 9);
        assert_eq!(w.redstone_power(AT + IVec3::X * 2), 9);
        assert_eq!(w.redstone_power(AT + IVec3::Y), 0);
    }

    #[test]
    fn torch_inverts_after_two_game_ticks_and_burns_out_after_eight_toggles() {
        let mut w = world();
        supported(&mut w, AT, r::TORCH);
        for n in 0..8 {
            put(&mut w, AT - IVec3::Y + IVec3::X, r::lever(3, true));
            ticks(&mut w, 2);
            assert!(on(&w, AT));
            ticks(&mut w, 1);
            assert!(!on(&w, AT));
            put(&mut w, AT - IVec3::Y + IVec3::X, r::lever(3, false));
            ticks(&mut w, 3);
            assert_eq!(on(&w, AT), n < 7);
        }
        assert!(w.redstone.burnout.contains_key(&AT));
        ticks(&mut w, 156);
        assert!(!on(&w, AT));
        ticks(&mut w, 1);
        assert!(on(&w, AT));
    }

    #[test]
    fn repeaters_delay_lock_and_stretch_short_pulses() {
        let mut w = world();
        supported(&mut w, AT, r::repeater(Facing::East, 3, false));
        put(&mut w, AT - IVec3::X, r::REDSTONE_BLOCK);
        ticks(&mut w, 6);
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
        put(&mut w, AT + IVec3::Z, r::wire(1));
        assert_eq!(w.comparator_target(AT, Facing::East, false), 1);
        assert_eq!(w.comparator_target(AT, Facing::East, true), 0);
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
        ticks(&mut w, 4);
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
    fn ticks_cancel_on_replacement_and_round_trip_with_burnout_and_queues() {
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
        ticks(&mut restored, 6);
        assert!(!on(&restored, AT));
        ticks(&mut restored, 1);
        assert!(on(&restored, AT));
        put(&mut w, AT, r::LAMP);
        ticks(&mut w, 10);
        assert_eq!(w.get_block(AT), Some(Block(1211)));
        assert!(!w.redstone.pending.contains_key(&AT));
        w.redstone.burnout.insert(AT, w.redstone.tick + 120);
        w.redstone.toggles.entry(AT).or_default().push_back(w.redstone.tick - 2);
        let save = w.redstone_to_string();
        restored.load_redstone(&save);
        assert_eq!(restored.redstone.burnout, w.redstone.burnout);
        assert_eq!(restored.redstone.toggles, w.redstone.toggles);
        restored.load_redstone("broken");
        assert_eq!(restored.redstone.burnout, w.redstone.burnout);
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
        put(&mut w, IVec3::new(64, 146, 64), r::REDSTONE_BLOCK);
        let start = std::time::Instant::now();
        ticks(&mut w, 1);
        eprintln!("16384-wire grid settle: {:?}, {} updates", start.elapsed(), w.redstone_updates_last_tick());
        let start = std::time::Instant::now();
        for _ in 0..1000 {
            std::hint::black_box(&mut w).tick_redstone();
        }
        eprintln!("idle grid: {:?}/tick", start.elapsed() / 1000);
        put(&mut w, IVec3::new(64, 146, 64), Block::AIR);
        let start = std::time::Instant::now();
        ticks(&mut w, 1);
        eprintln!("source removal: {:?}, {} updates", start.elapsed(), w.redstone_updates_last_tick());
    }
}
