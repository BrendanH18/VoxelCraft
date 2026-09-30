//! Water flow, simulated as a cellular automaton in the style of Minecraft.
//!
//! Only water that has been disturbed is simulated: edits wake nearby
//! water, and every change wakes its neighbours for the next tick. Each tick
//! reads the world, collects proposed changes, then applies them all at
//! once, so the result doesn't depend on update order. Chunks touched by
//! flow are remeshed on the worker threads.
//!
//! Rules:
//! - Water falls into air (or weaker flowing water) below it.
//! - Water that can't fall spreads sideways, losing one level per block
//!   (sources and landed falling water spread at level 1, down to level 7).
//!   Like Minecraft, it prefers directions leading to the nearest drop
//!   within 4 blocks.
//! - Flowing water that loses its feeder dries up one level per tick.
//! - Flowing water with two or more source neighbours on a solid or source
//!   floor becomes a source (infinite water).

use glam::IVec3;
use rustc_hash::{FxHashMap, FxHashSet};

use super::World;
use super::block::Block;

const TICK: f64 = 0.25;
const MAX_UPDATES_PER_TICK: usize = 4096;
const MAX_LEVEL: u8 = 7;
const DROP_SEARCH: u32 = 4;
const HORIZONTAL: [IVec3; 4] = [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z];
const ALL_DIRS: [IVec3; 6] = [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z];

#[derive(Default)]
pub(super) struct FluidState {
    pending: FxHashSet<IVec3>,
    timer: f64,
    /// Duration of the most recent tick, for the debug screen.
    pub last_tick_ms: f64,
}

/// Relative strength when two proposals target the same cell.
fn strength(b: Block) -> u8 {
    match b {
        Block::WATER => 10,
        Block::FALLING_WATER => 9,
        _ => b.water_level().map_or(0, |l| 8 - l),
    }
}

/// Flowing (not source, not falling) water.
fn is_flowing(b: Block) -> bool {
    b.is_water() && b != Block::WATER && b != Block::FALLING_WATER
}

impl World {
    /// Schedules the water at and around `p` for simulation.
    pub fn wake_fluids(&mut self, p: IVec3) {
        self.fluids.pending.insert(p);
        for o in ALL_DIRS {
            self.fluids.pending.insert(p + o);
        }
    }

    pub fn active_fluids(&self) -> usize {
        self.fluids.pending.len()
    }

    pub fn tick_fluids(&mut self, dt: f64) {
        self.fluids.timer = (self.fluids.timer + dt).min(TICK * 2.0);
        if self.fluids.timer < TICK || self.fluids.pending.is_empty() {
            return;
        }
        self.fluids.timer -= TICK;
        let started = std::time::Instant::now();

        let batch: Vec<IVec3> = if self.fluids.pending.len() <= MAX_UPDATES_PER_TICK {
            std::mem::take(&mut self.fluids.pending).into_iter().collect()
        } else {
            let batch: Vec<IVec3> = self.fluids.pending.iter().take(MAX_UPDATES_PER_TICK).copied().collect();
            for p in &batch {
                self.fluids.pending.remove(p);
            }
            batch
        };

        let mut changes: FxHashMap<IVec3, Block> = FxHashMap::default();
        for p in batch {
            self.fluid_step(p, &mut changes);
        }
        let changed = changes.len();
        for (p, b) in changes {
            if self.get_block(p).is_some_and(|cur| cur != b) && self.edit(p, b, false) {
                self.wake_fluids(p);
            }
        }
        self.fluids.last_tick_ms = started.elapsed().as_secs_f64() * 1000.0;
        log::debug!("water tick: {changed} changes in {:.2} ms", self.fluids.last_tick_ms);
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

    /// Unloaded cells read as solid so water never flows into them.
    fn cell(&self, p: IVec3) -> Block {
        self.get_block(p).unwrap_or(Block::STONE)
    }

    /// Water can fall from `p` if the cell below is air or weaker flowing water.
    fn can_fall(&self, p: IVec3) -> bool {
        let below = self.cell(p - IVec3::Y);
        below == Block::AIR || is_flowing(below)
    }

    /// The level water at `p` pushes into its horizontal neighbours, or
    /// `None` if it doesn't spread sideways (it's falling instead, or it's
    /// flowing water resting on other water).
    fn spread_level(&self, p: IVec3, b: Block) -> Option<u8> {
        if !b.is_water() || self.can_fall(p) {
            return None;
        }
        if b != Block::WATER && self.cell(p - IVec3::Y).is_water() {
            return None;
        }
        let level = if b == Block::WATER || b == Block::FALLING_WATER { 1 } else { b.water_level()? + 1 };
        (level <= MAX_LEVEL).then_some(level)
    }

    fn fluid_step(&self, p: IVec3, changes: &mut FxHashMap<IVec3, Block>) {
        let Some(b) = self.get_block(p) else { return };
        if !b.is_water() {
            return;
        }

        // Non-source water re-derives its state from its surroundings.
        let mut state = b;
        if b != Block::WATER {
            let above = self.cell(p + IVec3::Y);
            let new = if above.is_water() {
                Block::FALLING_WATER
            } else {
                let mut sources = 0;
                let mut best: Option<u8> = None;
                for o in HORIZONTAL {
                    let n = self.cell(p + o);
                    if n == Block::WATER {
                        sources += 1;
                    }
                    if let Some(l) = self.spread_level(p + o, n) {
                        best = Some(best.map_or(l, |b| b.min(l)));
                    }
                }
                let floor = self.cell(p - IVec3::Y);
                if sources >= 2 && (floor == Block::WATER || !floor.is_replaceable()) {
                    Block::WATER
                } else if let Some(l) = best {
                    Block::flowing_water(l)
                } else {
                    Block::AIR
                }
            };
            if new != b {
                Self::propose(changes, p, new);
                state = new;
            }
        }
        if !state.is_water() {
            return;
        }

        // Fall first; water that can fall doesn't spread sideways.
        if self.can_fall(p) {
            Self::propose(changes, p - IVec3::Y, Block::FALLING_WATER);
            return;
        }
        let Some(level) = self.spread_level(p, state) else { return };
        let target = Block::flowing_water(level);
        let open = |q: IVec3| {
            let n = self.cell(q);
            n == Block::AIR || (is_flowing(n) && n.water_level().unwrap() > level)
        };
        let dirs: Vec<IVec3> = HORIZONTAL.into_iter().filter(|&o| open(p + o)).collect();
        let preferred = if dirs.len() > 1 { self.directions_to_drop(p) } else { 0 };
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
    fn directions_to_drop(&self, p: IVec3) -> u8 {
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
            if self.cell(q).is_replaceable() {
                dist[slot(q)] = 1;
                first[slot(q)] = 1 << i;
                frontier.push(q);
            }
        }
        for d in 1..=R as u8 {
            let found = frontier.iter().fold(0u8, |m, &q| if self.can_fall(q) { m | first[slot(q)] } else { m });
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
                        if !self.cell(n).is_replaceable() {
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
