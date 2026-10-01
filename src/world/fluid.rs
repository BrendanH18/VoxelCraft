//! Water and lava flow, simulated as a cellular automaton in the style of
//! Minecraft.
//!
//! Only fluid that has been disturbed is simulated: edits wake nearby
//! fluid, and every change wakes its neighbours for the next tick. Each tick
//! reads the world, collects proposed changes, then applies them all at
//! once, so the result doesn't depend on update order. Chunks touched by
//! flow are remeshed on the worker threads.
//!
//! Rules (for each fluid on its own; lava ticks six times slower):
//! - Fluid falls into air (or weaker flowing fluid of its kind) below it.
//! - Fluid that can't fall spreads sideways, losing one level per block
//!   (sources and landed falling fluid spread at level 1, down to level 7
//!   for water and 3 for lava). Like Minecraft, it prefers directions
//!   leading to the nearest drop within 4 blocks.
//! - Flowing fluid that loses its feeder dries up one level per tick.
//! - Flowing water with two or more source neighbours on a solid or source
//!   floor becomes a source (infinite water). Lava has no such rule.
//! - Plants and torches in the way are washed away.
//! - Lava touching water hardens: a source into obsidian, flowing lava into
//!   cobblestone; lava pouring onto water turns the water into stone.

use glam::IVec3;
use rustc_hash::{FxHashMap, FxHashSet};

use super::World;
use super::block::{Block, Fluid};

const TICK: f64 = 0.25;
/// Lava moves once every this many water ticks (1.5 s, like Minecraft).
const LAVA_EVERY: u32 = 6;
const MAX_UPDATES_PER_TICK: usize = 4096;
const DROP_SEARCH: u32 = 4;
const HORIZONTAL: [IVec3; 4] = [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z];
const ALL_DIRS: [IVec3; 6] = [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z];

#[derive(Default)]
pub(super) struct FluidState {
    /// Cells to update on the next water / lava tick.
    pending: FxHashSet<IVec3>,
    lava_pending: FxHashSet<IVec3>,
    timer: f64,
    ticks: u32,
    /// Duration of the most recent tick, for the debug screen.
    pub last_tick_ms: f64,
}

/// Relative strength when two proposals target the same cell; blocks
/// formed by lava meeting water beat any flow.
fn strength(b: Block) -> u8 {
    match b {
        Block::WATER | Block::LAVA => 10,
        Block::FALLING_WATER | Block::FALLING_LAVA => 9,
        b if b.is_fluid() => 8 - b.fluid_level().unwrap(),
        Block::AIR => 0,
        _ => 20,
    }
}

/// Plants and torches: water flows into them and destroys them.
fn washes_away(b: Block) -> bool {
    b.kind() == super::block::RenderKind::Cross
}

/// Flowing (not source, not falling) fluid of the given kind.
fn is_flowing(b: Block, fluid: Fluid) -> bool {
    b.fluid() == Some(fluid) && b != fluid.source() && b != fluid.falling()
}

impl World {
    /// Schedules the fluid at and around `p` for simulation. Lava is only
    /// scheduled when some is there, so its slow tick doesn't hold up
    /// `World::is_idle` after unrelated edits.
    pub fn wake_fluids(&mut self, p: IVec3) {
        let cells = std::iter::once(p).chain(ALL_DIRS.map(|o| p + o));
        let lava = cells.clone().any(|q| self.get_block(q).is_some_and(|b| b.is_lava()));
        for q in cells {
            self.fluids.pending.insert(q);
            if lava {
                self.fluids.lava_pending.insert(q);
            }
        }
    }

    pub fn active_fluids(&self) -> usize {
        self.fluids.pending.len().max(self.fluids.lava_pending.len())
    }

    pub fn tick_fluids(&mut self, dt: f64) {
        self.fluids.timer = (self.fluids.timer + dt).min(TICK * 2.0);
        if self.fluids.timer < TICK {
            return;
        }
        self.fluids.timer -= TICK;
        self.fluids.ticks = self.fluids.ticks.wrapping_add(1);
        let lava_turn = self.fluids.ticks.is_multiple_of(LAVA_EVERY);
        if self.fluids.pending.is_empty() && !(lava_turn && !self.fluids.lava_pending.is_empty()) {
            return;
        }
        let started = std::time::Instant::now();

        let take = |pending: &mut FxHashSet<IVec3>| -> Vec<IVec3> {
            if pending.len() <= MAX_UPDATES_PER_TICK {
                return std::mem::take(pending).into_iter().collect();
            }
            let batch: Vec<IVec3> = pending.iter().take(MAX_UPDATES_PER_TICK).copied().collect();
            for p in &batch {
                pending.remove(p);
            }
            batch
        };
        let mut changes: FxHashMap<IVec3, Block> = FxHashMap::default();
        for p in take(&mut self.fluids.pending) {
            self.fluid_step(p, Fluid::Water, &mut changes);
        }
        if lava_turn {
            for p in take(&mut self.fluids.lava_pending) {
                self.fluid_step(p, Fluid::Lava, &mut changes);
            }
        }
        let changed = changes.len();
        for (p, b) in changes {
            let Some(old) = self.get_block(p).filter(|&cur| cur != b) else { continue };
            if self.edit(p, b, false) {
                // Water washes plants and torches away as items; lava burns them.
                if washes_away(old) && b.is_water() {
                    self.spill_block(p, old);
                }
                self.wake_fluids(p);
            }
        }
        self.fluids.last_tick_ms = started.elapsed().as_secs_f64() * 1000.0;
        log::debug!("fluid tick: {changed} changes in {:.2} ms", self.fluids.last_tick_ms);
    }

    pub fn fluid_tick_ms(&self) -> f64 {
        self.fluids.last_tick_ms
    }

    fn propose(changes: &mut FxHashMap<IVec3, Block>, p: IVec3, b: Block) {
        changes
            .entry(p)
            .and_modify(|cur| {
                if strength(b) > strength(*cur) {
                    *cur = b;
                }
            })
            .or_insert(b);
    }

    /// Unloaded cells read as solid so fluid never flows into them.
    fn cell(&self, p: IVec3) -> Block {
        self.get_block(p).unwrap_or(Block::STONE)
    }

    /// Fluid can fall from `p` if the cell below is air, a plant, or weaker
    /// flowing fluid of the same kind.
    fn can_fall(&self, p: IVec3, fluid: Fluid) -> bool {
        let below = self.cell(p - IVec3::Y);
        below == Block::AIR || is_flowing(below, fluid) || washes_away(below)
    }

    /// The level the fluid `b` at `p` pushes into its horizontal neighbours,
    /// or `None` if it doesn't spread sideways (it's another fluid, it's
    /// falling instead, or it's flowing fluid resting on more of itself).
    fn spread_level(&self, p: IVec3, b: Block, fluid: Fluid) -> Option<u8> {
        if b.fluid() != Some(fluid) || self.can_fall(p, fluid) {
            return None;
        }
        if b != fluid.source() && self.cell(p - IVec3::Y).fluid() == Some(fluid) {
            return None;
        }
        let level = b.fluid_level()? + 1;
        (level <= fluid.max_level()).then_some(level)
    }

    fn fluid_step(&self, p: IVec3, fluid: Fluid, changes: &mut FxHashMap<IVec3, Block>) {
        let Some(b) = self.get_block(p) else { return };
        if b.fluid() != Some(fluid) {
            return;
        }
        let source = fluid.source();

        // Lava meeting water hardens.
        if fluid == Fluid::Lava {
            if [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z, IVec3::Y].iter().any(|&o| self.cell(p + o).is_water()) {
                Self::propose(changes, p, if b == source { Block::OBSIDIAN } else { Block::COBBLESTONE });
                return;
            }
            if self.cell(p - IVec3::Y).is_water() {
                Self::propose(changes, p - IVec3::Y, Block::STONE);
                return;
            }
        }

        // Non-source fluid re-derives its state from its surroundings.
        let mut state = b;
        if b != source {
            let above = self.cell(p + IVec3::Y);
            let new = if above.fluid() == Some(fluid) {
                fluid.falling()
            } else {
                let mut sources = 0;
                let mut best: Option<u8> = None;
                for o in HORIZONTAL {
                    let n = self.cell(p + o);
                    if n == source {
                        sources += 1;
                    }
                    if let Some(l) = self.spread_level(p + o, n, fluid) {
                        best = Some(best.map_or(l, |b| b.min(l)));
                    }
                }
                let floor = self.cell(p - IVec3::Y);
                if fluid == Fluid::Water && sources >= 2 && (floor == source || !floor.is_replaceable()) {
                    source
                } else if let Some(l) = best {
                    fluid.flowing(l)
                } else {
                    Block::AIR
                }
            };
            if new != b {
                Self::propose(changes, p, new);
                state = new;
            }
        }
        if state.fluid() != Some(fluid) {
            return;
        }

        // Fall first; fluid that can fall doesn't spread sideways.
        if self.can_fall(p, fluid) {
            Self::propose(changes, p - IVec3::Y, fluid.falling());
            return;
        }
        let Some(level) = self.spread_level(p, state, fluid) else { return };
        let target = fluid.flowing(level);
        let open = |q: IVec3| {
            let n = self.cell(q);
            n == Block::AIR || washes_away(n) || (is_flowing(n, fluid) && n.fluid_level().unwrap() > level)
        };
        let dirs: Vec<IVec3> = HORIZONTAL.into_iter().filter(|&o| open(p + o)).collect();
        let preferred = if dirs.len() > 1 { self.directions_to_drop(p, fluid) } else { 0 };
        for (i, o) in HORIZONTAL.iter().enumerate() {
            let chosen = if preferred == 0 { dirs.contains(o) } else { preferred & (1 << i) != 0 };
            if chosen && open(p + *o) {
                Self::propose(changes, p + *o, target);
            }
        }
    }

    /// Breadth-first search over the horizontal plane (up to `DROP_SEARCH`
    /// steps) for the nearest cells water could fall from. Returns a bitmask
    /// over `HORIZONTAL` of the first steps that lead to one at minimum
    /// distance, or 0 if there is no drop in range.
    fn directions_to_drop(&self, p: IVec3, fluid: Fluid) -> u8 {
        // Diamond of radius DROP_SEARCH: at most 41 cells.
        const R: i32 = DROP_SEARCH as i32;
        const W: usize = (2 * R + 1) as usize;
        let slot = |q: IVec3| ((q.x - p.x + R) as usize) + ((q.z - p.z + R) as usize) * W;
        // Distance from p (u8::MAX unvisited, 0 for p itself and blocked cells)
        // and the set of first steps that reach each cell at that distance.
        let mut dist = [u8::MAX; W * W];
        let mut first = [0u8; W * W];
        dist[slot(p)] = 0;
        let mut frontier: Vec<IVec3> = Vec::with_capacity(16);
        for (i, &o) in HORIZONTAL.iter().enumerate() {
            let q = p + o;
            if self.cell(q).is_replaceable() || washes_away(self.cell(q)) {
                dist[slot(q)] = 1;
                first[slot(q)] = 1 << i;
                frontier.push(q);
            }
        }
        for d in 1..=R as u8 {
            let found = frontier.iter().fold(0u8, |m, &q| if self.can_fall(q, fluid) { m | first[slot(q)] } else { m });
            if found != 0 || d == R as u8 {
                return found;
            }
            let mut next = Vec::with_capacity(frontier.len() * 2);
            for &q in &frontier {
                for o in HORIZONTAL {
                    let n = q + o;
                    if (n.x - p.x).abs() + (n.z - p.z).abs() > R {
                        continue;
                    }
                    let si = slot(n);
                    if dist[si] == u8::MAX {
                        if !(self.cell(n).is_replaceable() || washes_away(self.cell(n))) {
                            dist[si] = 0;
                            continue;
                        }
                        dist[si] = d + 1;
                        next.push(n);
                    }
                    if dist[si] == d + 1 {
                        first[si] |= first[slot(q)];
                    }
                }
            }
            frontier = next;
        }
        0
    }
}
