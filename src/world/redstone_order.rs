//! Java 26.3 block update order: the `CollectingNeighborUpdater` stack,
//! `Level.setBlock` flags and side effects, `LevelTicks` collection, and
//! `ServerLevel` block events. Sources and differences: docs/redstone.md
//! "Update order" and docs/verification/2026-11-23-v0.17-update-order.md.
use std::collections::VecDeque;

use glam::IVec3;
use rustc_hash::{FxHashMap, FxHashSet};

use super::World;
use super::block::Block;

const DOWN: IVec3 = IVec3::NEG_Y;
const UP: IVec3 = IVec3::Y;
const NORTH: IVec3 = IVec3::NEG_Z;
const SOUTH: IVec3 = IVec3::Z;
const WEST: IVec3 = IVec3::NEG_X;
const EAST: IVec3 = IVec3::X;

/// `NeighborUpdater.UPDATE_ORDER`, used by `Level.updateNeighborsAt`.
pub const UPDATE_ORDER: [IVec3; 6] = [WEST, EAST, DOWN, UP, NORTH, SOUTH];
/// `BlockBehaviour.UPDATE_SHAPE_ORDER`, used by `updateNeighbourShapes`.
pub const UPDATE_SHAPE_ORDER: [IVec3; 6] = [WEST, EAST, NORTH, SOUTH, DOWN, UP];
/// `Direction.values()`.
pub const DIRECTIONS: [IVec3; 6] = [DOWN, UP, NORTH, SOUTH, WEST, EAST];
/// `Direction.Plane.HORIZONTAL`.
pub const HORIZONTAL: [IVec3; 4] = [NORTH, EAST, SOUTH, WEST];
/// `Direction.Plane.VERTICAL`.
pub const VERTICAL: [IVec3; 2] = [UP, DOWN];
/// `MinecraftServer.getMaxChainedNeighborUpdates` (the single-player value
/// and the dedicated server's `max-chained-neighbor-updates` default).
pub const MAX_CHAINED_NEIGHBOR_UPDATES: usize = 1_000_000;
/// `ServerLevel.tick`: `blockTicks.tick(time, 65536, ...)`.
pub const MAX_SCHEDULED_TICKS: usize = 65_536;

/// `Block.UPDATE_*` flags as `Level.setBlock` reads them, plus one
/// VoxelCraft-only bit.
pub mod flags {
    pub const UPDATE_NEIGHBORS: u16 = 1;
    pub const UPDATE_CLIENTS: u16 = 2;
    pub const UPDATE_KNOWN_SHAPE: u16 = 16;
    pub const UPDATE_SUPPRESS_DROPS: u16 = 32;
    pub const UPDATE_MOVE_BY_PISTON: u16 = 64;
    pub const UPDATE_SKIP_ON_PLACE: u16 = 512;
    /// `Block.UPDATE_ALL` (`setBlockAndUpdate`).
    pub const UPDATE_ALL: u16 = UPDATE_NEIGHBORS | UPDATE_CLIENTS;
    /// Set by plain `World::edit`, the path every caller that is not a
    /// ported Java block behaviour uses. It adds the compatibility
    /// notifications for strong signal sources whose own Java update calls
    /// are not ported (`World::compat_strong_updates`).
    pub const COMPAT: u16 = 1 << 15;
}

/// `ScheduledTick` priorities (`TickPriority.getValue`).
pub mod priority {
    pub const EXTREMELY_HIGH: i8 = -3;
    pub const VERY_HIGH: i8 = -2;
    pub const HIGH: i8 = -1;
    pub const NORMAL: i8 = 0;
}

/// One `CollectingNeighborUpdater.NeighborUpdates` entry.
#[derive(Clone, Copy, Debug)]
enum Update {
    /// `SimpleNeighborUpdate` / `FullNeighborUpdate`: `neighborChanged` at `pos`.
    Neighbor { pos: IVec3 },
    /// `MultiNeighborUpdate`: `neighborChanged` around `source` in
    /// `UPDATE_ORDER`, skipping one direction.
    Multi { source: IVec3, skip: Option<IVec3>, idx: u8 },
    /// `ShapeUpdate`: `updateShape` at `pos` for its neighbour at `pos + toward`.
    Shape { pos: IVec3, toward: IVec3 },
}

/// A queued `BlockEventData`: position, block family, and the two
/// integer parameters `triggerEvent` receives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlockEvent {
    pub pos: IVec3,
    pub block: Block,
    pub a: i32,
    pub b: i32,
}

/// One executed update, recorded only while [`World::trace_redstone`] is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Traced {
    /// `neighborChanged` ran at a position holding this block.
    Neighbor(IVec3, Block),
    /// `updateShape` ran at a position holding this block.
    Shape(IVec3, Block),
    /// A scheduled tick ran at a position holding this block.
    Tick(IVec3, Block),
    /// A block event was triggered at a position holding this block.
    Event(IVec3, Block),
}

#[derive(Default)]
pub(super) struct UpdateOrder {
    stack: Vec<Update>,
    added: Vec<Update>,
    count: usize,
    /// Updates and ticks executed since the counter was last reset.
    pub(super) executed: usize,
    /// Scheduled ticks collected for the current tick, in run order.
    pub(super) collected: Vec<(IVec3, u16)>,
    next: usize,
    /// `LevelTicks.toRunThisTickSet`: collected ticks that have not run yet.
    pub(super) this_tick: FxHashMap<IVec3, u16>,
    pub(super) events: VecDeque<BlockEvent>,
    queued_events: FxHashSet<BlockEvent>,
    pub(super) trace: Option<Vec<Traced>>,
    /// A tick that `track_redstone` removed with its block (the observer's
    /// `affectNeighborsAfterRemoval` reads `hasScheduledTick`).
    pub(super) removed_tick: Option<IVec3>,
}

/// A collected scheduled tick: trigger tick, priority, sub-tick order,
/// position and block family.
pub(super) type DueTick = (u64, i8, u64, IVec3, u16);

/// `Vec3i.hashCode`.
fn java_hash(p: IVec3) -> i32 {
    p.y.wrapping_add(p.z.wrapping_mul(31)).wrapping_mul(31).wrapping_add(p.x)
}

/// Iteration order of a default-capacity `java.util.HashSet` holding `cells`
/// (fewer than 13, so the 16-bucket table never resizes): bucket index of
/// the spread hash, then insertion order within a bucket.
pub fn java_hash_set_order<const N: usize>(cells: [IVec3; N]) -> [IVec3; N] {
    debug_assert!(N <= 12);
    let mut out = cells;
    // A stable sort keeps insertion order within one bucket.
    out.sort_by_key(|&p| java_hash_bucket(p));
    out
}

/// The bucket of `p` in a 16-bucket `java.util.HashMap` or `HashSet` (at
/// most 12 entries): `Vec3i.hashCode` spread by `HashMap.hash`. A stable
/// sort by it, from insertion order, is the iteration order.
pub fn java_hash_bucket(p: IVec3) -> u32 {
    let h = java_hash(p) as u32;
    (h ^ (h >> 16)) & 15
}

/// Java's `Direction.get3DDataValue` for a unit offset.
pub fn java_direction_value(d: IVec3) -> i32 {
    DIRECTIONS.iter().position(|&x| x == d).map_or(-1, |i| i as i32)
}

impl World {
    /// Record every executed update, scheduled tick and block event from now
    /// on (for order fixtures and debugging); `false` stops and clears it.
    pub fn trace_redstone(&mut self, on: bool) {
        self.redstone.order.trace = on.then(Vec::new);
    }

    /// Take the recorded trace, leaving recording on.
    pub fn take_redstone_trace(&mut self) -> Vec<Traced> {
        self.redstone.order.trace.as_mut().map(std::mem::take).unwrap_or_default()
    }

    fn record(&mut self, entry: impl FnOnce(&World) -> Traced) {
        if self.redstone.order.trace.is_some() {
            let traced = entry(self);
            if let Some(trace) = &mut self.redstone.order.trace {
                trace.push(traced);
            }
        }
    }

    /// `CollectingNeighborUpdater.addAndRun`: the outermost call runs the
    /// stack to completion; nested calls join the current layer.
    fn add_and_run(&mut self, update: Update) {
        let order = &mut self.redstone.order;
        let running = order.count > 0;
        let too_many = order.count >= MAX_CHAINED_NEIGHBOR_UPDATES;
        order.count += 1;
        if !too_many {
            if running {
                order.added.push(update);
            } else {
                order.stack.push(update);
            }
        } else if order.count - 1 == MAX_CHAINED_NEIGHBOR_UPDATES {
            log::error!(
                "Too many chained neighbor updates. Skipping the rest. First skipped position: {:?}",
                match update {
                    Update::Neighbor { pos } | Update::Shape { pos, .. } => pos,
                    Update::Multi { source, .. } => source,
                }
            );
        }
        if !running {
            self.run_updates();
        }
    }

    /// `CollectingNeighborUpdater.runUpdates`: depth first, each entry's
    /// later steps waiting until everything it added has run.
    fn run_updates(&mut self) {
        loop {
            let order = &mut self.redstone.order;
            if order.stack.is_empty() && order.added.is_empty() {
                break;
            }
            while let Some(update) = order.added.pop() {
                order.stack.push(update);
            }
            while let Some(top) = self.redstone.order.stack.last_mut() {
                let (step, more) = match top {
                    Update::Neighbor { pos } => (Some((*pos, None)), false),
                    Update::Shape { pos, toward } => (Some((*pos, Some(*toward))), false),
                    Update::Multi { source, skip, idx } => {
                        let d = UPDATE_ORDER[*idx as usize];
                        *idx += 1;
                        if (*idx as usize) < UPDATE_ORDER.len() && Some(UPDATE_ORDER[*idx as usize]) == *skip {
                            *idx += 1;
                        }
                        (Some((*source + d, None)), (*idx as usize) < UPDATE_ORDER.len())
                    }
                };
                if let Some((pos, toward)) = step {
                    self.redstone.order.executed += 1;
                    match toward {
                        None => self.run_neighbor_changed(pos),
                        Some(toward) => self.run_shape_update(pos, toward),
                    }
                }
                if !more {
                    self.redstone.order.stack.pop();
                    break;
                }
                if !self.redstone.order.added.is_empty() {
                    break;
                }
            }
        }
        let order = &mut self.redstone.order;
        order.stack.clear();
        order.added.clear();
        order.count = 0;
    }

    fn run_neighbor_changed(&mut self, p: IVec3) {
        let Some(b) = self.get_block(p) else { return };
        self.record(|_| Traced::Neighbor(p, b));
        if !super::redstone::participates(b) {
            return;
        }
        // Unloaded or non-ticking cells keep their update until they tick.
        if !self.is_block_ticking(p) {
            self.park_redstone_update(p);
            return;
        }
        self.redstone_update(p);
    }

    fn run_shape_update(&mut self, p: IVec3, toward: IVec3) {
        let Some(b) = self.get_block(p) else { return };
        self.record(|_| Traced::Shape(p, b));
        self.redstone_shape_update(p, b, toward);
    }

    /// Run `f` as one batch: updates it causes wait until it returns, as if
    /// it ran inside an update. For VoxelCraft behaviour that edits several
    /// cells and must not see its own half-done edits; ported Java behaviour
    /// runs unbatched, as Java's does.
    pub(super) fn batched(&mut self, f: impl FnOnce(&mut World)) {
        if self.redstone.order.count > 0 {
            return f(self);
        }
        self.redstone.order.count = 1;
        f(self);
        self.run_updates();
    }

    /// `Level.updateNeighborsAt`: `neighborChanged` on all six neighbours.
    pub(super) fn update_neighbors_at(&mut self, p: IVec3) {
        self.add_and_run(Update::Multi { source: p, skip: None, idx: 0 });
    }

    /// `Level.updateNeighborsAtExceptFromFacing`.
    pub(super) fn update_neighbors_at_except(&mut self, p: IVec3, skip: IVec3) {
        let idx = (UPDATE_ORDER[0] == skip) as u8;
        self.add_and_run(Update::Multi { source: p, skip: Some(skip), idx });
    }

    /// `Level.neighborChanged`: one `neighborChanged` at `p`.
    pub(super) fn neighbor_changed_at(&mut self, p: IVec3) {
        self.add_and_run(Update::Neighbor { pos: p });
    }

    /// `Level.neighborShapeChanged`: `updateShape` at `p` because its
    /// neighbour at `p + toward` changed.
    pub(super) fn neighbor_shape_changed(&mut self, p: IVec3, toward: IVec3) {
        self.add_and_run(Update::Shape { pos: p, toward });
    }

    /// `Level.updateNeighbourForOutputSignal`: comparators beside `p`, or
    /// one conductor beyond it, hear about a changed analog output.
    pub(super) fn update_neighbour_for_output_signal(&mut self, p: IVec3) {
        for d in HORIZONTAL {
            let q = p + d;
            let Some(b) = self.get_block(q) else { continue };
            if super::redstone::is_comparator(b) {
                self.neighbor_changed_at(q);
            } else if super::redstone_blocks::conductor(b)
                && self.get_block(q + d).is_some_and(super::redstone::is_comparator)
            {
                self.neighbor_changed_at(q + d);
            }
        }
    }

    /// `Block.updateOrDestroy` flags: `Level.setBlock` for redstone
    /// behaviour that Java ports exactly. `World::edit` is this with
    /// `UPDATE_ALL | COMPAT`.
    pub(crate) fn set_block_flags(&mut self, p: IVec3, block: Block, flags: u16) -> bool {
        self.edit_flags(p, block, flags)
    }

    /// The `Level.setBlock` side effects after the cell holds `new`:
    /// `affectNeighborsAfterRemoval`, `onPlace`, neighbour updates, the
    /// comparator output notification and shape updates, in Java's order.
    pub(super) fn redstone_block_changed(&mut self, p: IVec3, old: Block, new: Block, flags: u16) {
        use flags::*;
        let moved = flags & UPDATE_MOVE_BY_PISTON != 0;
        let changed_block = super::redstone::family(old) != super::redstone::family(new);
        if (changed_block || new.is_rail()) && (flags & UPDATE_NEIGHBORS != 0 || moved) {
            self.affect_neighbors_after_removal(p, old, moved);
            // `Containers.updateNeighboursAfterDestroy`: a removed block with
            // an analog output tells comparators beside it and beyond one
            // conductor.
            if changed_block && !moved && super::redstone_catalogue::role(old).analog {
                self.update_neighbour_for_output_signal(p);
            }
        }
        self.redstone.order.removed_tick = None;
        if self.get_block(p) != Some(new) {
            return;
        }
        if flags & UPDATE_SKIP_ON_PLACE == 0 {
            self.redstone_on_place(p, old, new);
        }
        // `Level.setBlock` continues only if `onPlace` left the state alone.
        if self.get_block(p) != Some(new) {
            return;
        }
        if flags & UPDATE_NEIGHBORS != 0 {
            self.update_neighbors_at(p);
            if super::redstone::has_analog_output(self, p, new) {
                self.update_neighbour_for_output_signal(p);
            }
        }
        if flags & UPDATE_KNOWN_SHAPE == 0 {
            self.wire_indirect_shapes(p, old);
            for d in UPDATE_SHAPE_ORDER {
                self.neighbor_shape_changed(p + d, -d);
            }
            self.wire_indirect_shapes(p, new);
        }
        if flags & COMPAT != 0 {
            self.compat_strong_updates(p, old, new);
        }
    }

    /// `RedStoneWireBlock.updateIndirectNeighbourShapes` for `state` at `p`.
    pub(super) fn wire_indirect_shapes(&mut self, p: IVec3, state: Block) {
        if !super::redstone::is_wire(state) {
            return;
        }
        let sides = super::redstone_blocks::connections(|d| {
            if d == IVec3::ZERO { state } else { self.get_block(p + d).unwrap_or(Block::AIR) }
        });
        for d in HORIZONTAL {
            let f = super::block::Facing::from_offset(d).expect("horizontal");
            if sides[f as usize] == 0 || self.get_block(p + d).is_some_and(super::redstone::is_wire) {
                continue;
            }
            for q in [p + d + DOWN, p + d + UP] {
                if self.get_block(q).is_some_and(super::redstone::is_wire) {
                    self.neighbor_shape_changed(q, -d);
                }
            }
        }
    }

    /// `Level.blockEvent` on the server: queue `triggerEvent` for the end
    /// of the tick. An identical queued event is not added twice.
    pub fn queue_block_event(&mut self, pos: IVec3, block: Block, a: i32, b: i32) {
        let event = BlockEvent { pos, block: super::redstone::family(block), a, b };
        if self.redstone.order.queued_events.insert(event) {
            self.redstone.order.events.push_back(event);
        }
    }

    /// Queued block events, in run order.
    pub fn block_events(&self) -> impl Iterator<Item = &BlockEvent> + '_ {
        self.redstone.order.events.iter()
    }

    /// `ServerLevel.runBlockEvents`: drain the queue, events added while it
    /// runs included; events in non-ticking cells wait for a later tick.
    pub fn run_block_events(&mut self) {
        if self.redstone.order.events.is_empty() {
            return;
        }
        let mut waiting = Vec::new();
        while let Some(event) = self.redstone.order.events.pop_front() {
            self.redstone.order.queued_events.remove(&event);
            if !self.is_block_ticking(event.pos) {
                waiting.push(event);
                continue;
            }
            // `doBlockEvent`: only if the block is still the one that queued it.
            if let Some(b) = self.get_block(event.pos)
                && super::redstone::family(b) == event.block
            {
                self.record(|_| Traced::Event(event.pos, b));
                self.redstone.order.executed += 1;
                // Pistons are the only answer, and `world::piston` ports
                // `triggerEvent` with Java's flags and updates: no batch.
                self.trigger_block_event(event);
            }
        }
        for event in waiting {
            if self.redstone.order.queued_events.insert(event) {
                self.redstone.order.events.push_back(event);
            }
        }
    }

    /// `BlockBehaviour.triggerEvent` for the queued event's block.
    pub fn trigger_block_event(&mut self, event: BlockEvent) -> bool {
        let Some(b) = self.get_block(event.pos) else { return false };
        if matches!(super::redstone_blocks::component(b), Some(super::redstone_blocks::Component::Piston { .. })) {
            return self.piston_trigger_event(event.pos, b, event.a, event.b);
        }
        false
    }

    /// `LevelTicks.willTickThisTick`.
    pub(super) fn will_tick_this_tick(&self, p: IVec3) -> bool {
        self.get_block(p).is_some_and(|b| self.redstone.order.this_tick.get(&p) == Some(&super::redstone::family(b).0))
    }

    pub(super) fn block_events_to_string(&self, entries: &mut Vec<String>) {
        for e in &self.redstone.order.events {
            entries.push(format!("e,{},{},{},{},{},{}", e.pos.x, e.pos.y, e.pos.z, e.block.0, e.a, e.b));
        }
    }

    pub(super) fn load_block_event(&mut self, p: IVec3, values: &[i64]) {
        if let &[block, a, b] = values
            && let (Ok(block), Ok(a), Ok(b)) = (u16::try_from(block), i32::try_from(a), i32::try_from(b))
        {
            let event = BlockEvent { pos: p, block: Block(block), a, b };
            if self.redstone.order.queued_events.insert(event) {
                self.redstone.order.events.push_back(event);
            }
        }
    }

    /// Run the ticks gathered by `collect_scheduled_ticks`.
    pub(super) fn run_collected_ticks(&mut self) {
        self.redstone.order.next = 0;
        while let Some(&(p, family)) = self.redstone.order.collected.get(self.redstone.order.next) {
            self.redstone.order.next += 1;
            self.redstone.order.this_tick.remove(&p);
            self.redstone.order.executed += 1;
            // `ServerLevel.tickBlock`: only if the cell still holds the block.
            if let Some(b) = self.get_block(p)
                && super::redstone::family(b) == Block(family)
            {
                self.record(|_| Traced::Tick(p, b));
                self.redstone_scheduled_tick(p);
            }
        }
        self.redstone.order.collected.clear();
        self.redstone.order.this_tick.clear();
    }

    /// `LevelTicks.drainContainers`: ticks grouped by chunk column (Java's
    /// `LevelChunkTicks`), each column in `DRAIN_ORDER`, merged by the
    /// `INTRA_TICK_DRAIN_ORDER` (priority, then sub-tick order) of their heads.
    pub(super) fn order_collected(ticks: &mut Vec<DueTick>, out: &mut Vec<(IVec3, u16)>) {
        // Global drain order already orders each column's ticks.
        if ticks.iter().all(|t| t.0 == ticks[0].0) {
            // Equal trigger ticks: the merge is the intra-tick order itself.
            out.extend(ticks.iter().map(|t| (t.3, t.4)));
            ticks.clear();
            return;
        }
        let column = |p: IVec3| (p.x >> 4, p.z >> 4);
        let mut columns: Vec<VecDeque<DueTick>> = Vec::new();
        let mut index: FxHashMap<(i32, i32), usize> = FxHashMap::default();
        for &t in ticks.iter() {
            let i = *index.entry(column(t.3)).or_insert_with(|| {
                columns.push(VecDeque::new());
                columns.len() - 1
            });
            columns[i].push_back(t);
        }
        ticks.clear();
        let intra = |t: &DueTick| (t.1, t.2);
        // Containers whose head is next, by intra-tick order of the head.
        let mut queue: Vec<usize> = (0..columns.len()).collect();
        loop {
            queue.retain(|&i| !columns[i].is_empty());
            let Some((at, _)) = queue.iter().enumerate().min_by_key(|&(_, &i)| intra(&columns[i][0])) else {
                break;
            };
            let top = queue.remove(at);
            let first = columns[top].pop_front().expect("non-empty");
            out.push((first.3, first.4));
            let next_best = queue.iter().map(|&i| intra(&columns[i][0])).min();
            while let Some(head) = columns[top].front() {
                if next_best.is_some_and(|n| intra(head) > n) {
                    break;
                }
                out.push((head.3, head.4));
                columns[top].pop_front();
            }
            if !columns[top].is_empty() {
                queue.push(top);
            }
        }
    }
}

/// v1.0 Track 3: entry counts for the long-session soak (`crate::soak`).
impl UpdateOrder {
    pub(super) fn soak_counts(&self, out: &mut crate::world::diagnostics::Counts) {
        out.extend([
            ("redstone.order.stack", self.stack.len()),
            ("redstone.order.added", self.added.len()),
            ("redstone.order.collected", self.collected.len()),
            ("redstone.order.this_tick", self.this_tick.len()),
            ("redstone.order.events", self.events.len()),
            ("redstone.order.queued_events", self.queued_events.len()),
            ("redstone.order.trace", self.trace.as_ref().map_or(0, Vec::len)),
        ]);
    }
}
