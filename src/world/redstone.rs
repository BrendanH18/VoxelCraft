//! Changed-block redstone propagation and scheduled component ticks at 20 Hz.
//! Java references and intentional ordering differences: docs/redstone.md.
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

use glam::IVec3;
use rustc_hash::{FxHashMap, FxHashSet};

use super::World;
use super::block::{Block, Facing, Shaped};
use super::chunk::CHUNK_SIZE_I;
use super::rails;
use super::redstone_blocks::{self as r, Component};

fn family(b: Block) -> Option<Block> {
    r::base(b).or_else(|| rails::family(b))
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
    /// Detector-rail comparator output, filled by the minecart tick.
    pub(super) cart_signal: FxHashMap<IVec3, u8>,
    toggles: FxHashMap<IVec3, VecDeque<u64>>,
    burnout: FxHashMap<IVec3, u64>,
    comparator: FxHashMap<IVec3, u8>,
    /// Tracks powered edges for manually operable doors and gates.
    powered: FxHashSet<IVec3>,
    pub last_updates: usize,
    day_time: f64,
    sky_darken: u8,
}

fn conductor(b: Block) -> bool {
    b.is_opaque()
}

impl World {
    fn queue_redstone(&mut self, p: IVec3) {
        if self
            .get_block(p)
            .is_some_and(|b| r::component(b).is_some() || b.is_door() || b.is_gate() || b.is_rail() || b == Block::TNT)
            && self.redstone.queued.insert(p)
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
        self.redstone_changed(p);
    }

    pub(super) fn load_redstone_chunk(&mut self, c: IVec3) {
        // Saved state needs its connections refreshed once when loaded, including
        // adjacent chunks. Generation never places redstone components.
        let Some(slot) = self.chunks.get(&c) else { return };
        if !slot.modified {
            return;
        }
        let data = slot.data.clone();
        let mut i = 0;
        data.for_each_block(|b| {
            if r::component(b).is_some() || b.is_door() || b.is_gate() || b.is_rail() {
                let local =
                    IVec3::new(i % CHUNK_SIZE_I, i / (CHUNK_SIZE_I * CHUNK_SIZE_I), i / CHUNK_SIZE_I % CHUNK_SIZE_I);
                self.redstone_changed(c * CHUNK_SIZE_I + local);
            }
            i += 1;
        });
    }

    pub(super) fn schedule_redstone(&mut self, p: IVec3, delay: u64, priority: i8) {
        if self.redstone.pending.contains_key(&p) {
            return;
        }
        let Some(b) = self.get_block(p).and_then(family) else { return };
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
        if rails::is_powered(b) && rails::kind(b) == Some(rails::RailKind::Detector) {
            return if !strong || direction == IVec3::NEG_Y { 15 } else { 0 };
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
        if let Some(value) = self.container_signal(rear) {
            input = value;
        } else if input < 15
            && self.get_block(rear).is_some_and(conductor)
            && let Some(value) = self.container_signal(rear - facing.offset())
        {
            input = value;
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
            next.entry(p).and_modify(|v: &mut u8| *v = (*v).max(signal)).or_insert(signal);
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

    pub fn container_signal(&self, p: IVec3) -> Option<u8> {
        if self.get_block(p).is_some_and(|b| rails::kind(b) == Some(rails::RailKind::Detector)) {
            return Some(self.redstone.cart_signal.get(&p).copied().unwrap_or(0));
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
        let support = match r::component(b) {
            Some(Component::Lever { mount, .. } | Component::Button { mount, .. } | Component::Torch { mount, .. }) => {
                Some(r::support(mount))
            }
            Some(Component::Repeater { .. } | Component::Comparator { .. } | Component::Plate { .. }) => {
                Some(IVec3::NEG_Y)
            }
            _ => None,
        };
        if support.is_some_and(|d| self.get_block(p + d).is_some_and(|b| !b.is_solid())) {
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
                if self.get_block(p - IVec3::Y).is_some_and(|b| !b.is_solid()) {
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
            _ if b == Block::TNT && self.redstone_power(p) > 0 => {
                self.edit(p, Block::AIR, false);
                self.primed_tnt.push((p, false));
            }
            _ => {}
        }
    }

    fn redstone_scheduled_tick(&mut self, p: IVec3) {
        let Some(b) = self.get_block(p) else { return };
        if rails::family(b).is_some() {
            return self.detector_tick(p, b);
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
                    self.edit(p, r::button(mount, false, wood), false);
                }
            }
            Some(Component::Plate { kind, .. }) => {
                let power = super::redstone_contacts::plate_power(
                    kind,
                    self.redstone.contacts.get(&p).copied().unwrap_or_default(),
                );
                if b != r::plate(kind, power) {
                    self.edit(p, r::plate(kind, power), false);
                }
                if power > 0 {
                    self.schedule_redstone(p, if kind < 2 { 20 } else { 10 }, 0);
                }
            }
            Some(Component::Target(_)) => {
                self.edit(p, r::TARGET, false);
            }
            Some(Component::GlowingOre(deep)) => {
                self.edit(p, if deep { Block::DEEPSLATE_REDSTONE_ORE } else { Block::REDSTONE_ORE }, false);
            }
            Some(Component::Daylight { power, inverted }) => {
                let value = self.daylight_power(p, inverted);
                if value != power {
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

    /// One authoritative 50 ms game step. Neighbour work settles before and
    /// after due scheduled ticks, with stable insertion order for equal priority.
    /// No scanning of loaded blocks or allocation on an idle tick.
    pub fn tick_redstone(&mut self) {
        self.redstone.tick += 1;
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
            self.redstone.pending.remove(&p);
            if self.get_block(p).is_none() {
                let next = self.redstone.tick + 1;
                self.redstone.pending.insert(p, (next, family, sequence));
                self.redstone.scheduled.push(Reverse((next, priority, sequence, xyz, family)));
            } else if self.get_block(p).and_then(self::family) == Some(Block(family)) {
                self.redstone.last_updates += 1;
                self.redstone_scheduled_tick(p);
                self.drain_redstone_updates();
            }
        }
    }

    pub fn set_redstone_daylight(&mut self, time: f64, sky_darken: u8) {
        self.redstone.day_time = time.rem_euclid(1.0);
        self.redstone.sky_darken = sky_darken.min(15);
    }

    fn daylight_power(&self, p: IVec3, inverted: bool) -> u8 {
        if !self.generator.dimension.has_sky() {
            return 0;
        }
        let sky = if self.sky_exposed(p) { 15u8.saturating_sub(self.redstone.sky_darken) } else { 0 };
        if inverted {
            return 15 - sky;
        }
        let angle = (self.redstone.day_time - 0.25).rem_euclid(1.0) * std::f64::consts::TAU;
        let target = if angle < std::f64::consts::PI { 0.0 } else { std::f64::consts::TAU };
        (sky as f64 * (angle + (target - angle) * 0.2).cos()).round().clamp(0.0, 15.0) as u8
    }

    fn redstone_open(&mut self, p: IVec3, b: Block) {
        match r::component(b) {
            Some(Component::IronDoor { upper, .. }) => {
                let lower = if upper { p - IVec3::Y } else { p };
                let on = self.redstone_power(lower) > 0 || self.redstone_power(lower + IVec3::Y) > 0;
                for (q, upper) in [(lower, false), (lower + IVec3::Y, true)] {
                    if let Some(Component::IronDoor { facing, open, .. }) = self.get_block(q).and_then(r::component)
                        && open != on
                    {
                        self.edit(q, r::iron_door(facing, on, upper), false);
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
                    self.edit(p, r::trapdoor(facing, on, top, iron), false);
                }
            }
            _ => {}
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
        self.schedule_redstone(p, 30, 0);
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
        let support = match r::component(b) {
            Some(Component::Torch { mount, .. } | Component::Lever { mount, .. } | Component::Button { mount, .. }) => {
                r::support(mount)
            }
            Some(
                Component::Wire(_)
                | Component::Repeater { .. }
                | Component::Comparator { .. }
                | Component::Plate { .. },
            ) => IVec3::NEG_Y,
            _ => return true,
        };
        self.get_block(p + support)
            .is_some_and(|b| b.is_opaque() || b == Block::GLASS || b.stained_glass_color().is_some())
    }

    /// Shared two-cell iron door placement for all input sources.
    pub fn place_iron_door(&mut self, p: IVec3, facing: Facing) -> bool {
        if ![p, p + IVec3::Y].into_iter().all(|q| self.get_block(q).is_some_and(Block::is_replaceable))
            || !self.get_block(p - IVec3::Y).is_some_and(Block::is_opaque)
        {
            return false;
        }
        self.set_block(p, r::iron_door(facing, false, false));
        self.set_block(p + IVec3::Y, r::iron_door(facing, false, true))
    }

    fn drain_redstone_updates(&mut self) {
        while self.redstone.last_updates < MAX_UPDATES {
            let Some(p) = self.redstone.updates.pop_front() else { break };
            self.redstone.queued.remove(&p);
            self.redstone.last_updates += 1;
            self.redstone_update(p);
        }
    }

    pub fn redstone_updates_last_tick(&self) -> usize {
        self.redstone.last_updates
    }

    /// Device-independent right click; callers handle reach and build permission.
    pub fn use_redstone(&mut self, p: IVec3) -> bool {
        let Some(b) = self.get_block(p) else { return false };
        let next = match r::component(b) {
            Some(Component::Wire(power)) => {
                if self.wire_connections(p) == [1; 4] || self.wire_connections(p) == [0; 4] {
                    if b.0 >= 1453 { r::wire(power) } else { r::dot(power) }
                } else {
                    return false;
                }
            }
            Some(Component::Trapdoor { facing, open, top, iron: false }) => r::trapdoor(facing, !open, top, false),
            Some(Component::Trapdoor { iron: true, .. } | Component::IronDoor { .. }) => return true,
            Some(Component::Daylight { power, inverted }) => r::daylight(power, !inverted),
            Some(Component::Lever { mount, on }) => r::lever(mount, !on),
            Some(Component::Button { mount, on: false, wood }) => {
                self.schedule_redstone(p, if wood { 30 } else { 20 }, 0);
                r::button(mount, true, wood)
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
                due.saturating_sub(self.redstone.tick),
                priority,
                seq,
                family
            ));
        }
        for p in &self.redstone.updates {
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
                        && ((1100..=1499).contains(&family) || matches!(family, 226 | 249))
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
                ("p", []) => {
                    self.redstone.powered.insert(p);
                }
                _ => {}
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
        let mut w = world();
        put(&mut w, AT, r::DAYLIGHT);
        w.set_redstone_daylight(0.25, 0);
        ticks(&mut w, 21);
        assert_eq!(w.get_block(AT), Some(r::daylight(15, false)));
        w.set_redstone_daylight(0.75, 11);
        ticks(&mut w, 20);
        assert_eq!(w.get_block(AT), Some(r::daylight(0, false)));
        assert!(w.use_redstone(AT));
        ticks(&mut w, 20);
        assert_eq!(w.get_block(AT), Some(r::daylight(11, true)));
        put(&mut w, AT, Block::DEEPSLATE_REDSTONE_ORE);
        assert!(w.touch_redstone_ore(AT));
        let glowing = w.get_block(AT).unwrap();
        assert_eq!(glowing.emission(), 9);
        assert_eq!(glowing.harvest_level(), Some(2));
        assert_eq!(glowing.as_stone_ore(), Block::REDSTONE_ORE);
        ticks(&mut w, 30);
        assert_eq!(w.get_block(AT), Some(Block::DEEPSLATE_REDSTONE_ORE));
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
