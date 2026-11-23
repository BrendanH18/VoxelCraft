//! Java 26.3 `MovingPistonBlock` and `PistonMovingBlockEntity`: a cell in
//! motion keeps its moved state, direction, extending and source flags and
//! progress, ticks after block events, and lands (`tick`) or is completed
//! at once (`finalTick`).
use glam::{DVec3, IVec3};

use super::super::{
    World,
    block::Block,
    redstone_blocks::{self as r, Component},
};
use super::reaction::is_piston;

/// `PistonMovingBlockEntity.tick` adds this much progress per game tick
/// (`TICKS_TO_EXTEND` is 2).
pub const STEP: f32 = 0.5;
/// `getCollisionRelatedBlockState`: a retracting head turns short past this.
pub const SHORT_HEAD: f32 = 0.25;
/// `PistonBaseBlock.checkIfExtend`: below this progress an extension
/// interrupted by a falling edge drops its block (event 2).
pub const SPIT_PROGRESS: f32 = 0.5;

/// One `PistonMovingBlockEntity`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Moving {
    /// `movedState`.
    pub moved: Block,
    /// `direction`: the piston's facing (also the `facing` of the cell's
    /// `moving_piston` state).
    pub facing: u8,
    pub extending: bool,
    /// `isSourcePiston`: a piston's arm or retracting base.
    pub source: bool,
    pub progress: f32,
    pub progress_o: f32,
    /// `lastTicked` on the redstone clock.
    pub last_ticked: u64,
    /// Ticker order: Java ticks block entities in the order they were added.
    pub order: u64,
}

impl Moving {
    pub fn direction(&self) -> IVec3 {
        r::direction(self.facing)
    }

    /// `getMovementDirection`.
    pub fn movement(&self) -> IVec3 {
        if self.extending { self.direction() } else { -self.direction() }
    }

    /// `getExtendedProgress`: how far the moved block sits from its cell.
    pub fn extended_progress(&self, progress: f32) -> f32 {
        if self.extending { progress - 1.0 } else { 1.0 - progress }
    }

    /// Whether this is a retracting piston base (its head slides home).
    pub fn retracting_base(&self) -> bool {
        !self.extending && self.source && is_piston(self.moved)
    }

    fn sticky_piston(&self) -> bool {
        matches!(
            r::component(self.moved),
            Some(Component::Piston { sticky: true, .. } | Component::PistonHead { sticky: true, .. })
        )
    }

    /// The `type` of the cell's `moving_piston` state: a source takes its
    /// piston's type; pushed and pulled blocks are `default`.
    pub fn sticky_type(&self) -> bool {
        self.source && self.sticky_piston()
    }

    /// `getCollisionRelatedBlockState` (which pushes entities): a retracting
    /// base pushes with its head; head shortness is not modelled.
    pub fn push_state(&self) -> Block {
        if self.retracting_base() { r::piston_head(self.facing, self.sticky_piston()) } else { self.moved }
    }

    /// The block drawn in motion.
    pub fn drawn(&self) -> Block {
        self.push_state()
    }

    /// `getCollisionShape` in world space, excluding the moving part when
    /// `noclip` is its movement (`NOCLIP` while entities are being pushed).
    pub fn collision(&self, p: IVec3, noclip: Option<IVec3>, f: &mut dyn FnMut(DVec3, DVec3)) {
        let lo = p.as_dvec3();
        if self.retracting_base() {
            let extended = r::piston(self.facing, self.sticky_piston(), true);
            crate::physics::block_collision_boxes(extended, |a, b| f(lo + a, lo + b));
        }
        if self.progress < 1.0 && noclip == Some(self.movement()) {
            return;
        }
        let state = if self.source { r::piston_head(self.facing, self.sticky_piston()) } else { self.moved };
        let off = lo + self.direction().as_dvec3() * f64::from(self.extended_progress(self.progress));
        crate::physics::block_collision_boxes(state, |a, b| f(off + a, off + b));
    }
}

impl World {
    /// The moving block entity at `p`, if any.
    pub fn moving_piston(&self, p: IVec3) -> Option<Moving> {
        self.pistons.moving.get(&p).copied()
    }

    /// The `facing` and sticky `type` of the `moving_piston` state at `p`,
    /// from its block entity (the chunk stores one state, 1470).
    pub fn moving_piston_state(&self, p: IVec3) -> Option<(u8, bool)> {
        (self.get_block(p) == Some(r::MOVING))
            .then(|| self.moving_piston(p))
            .flatten()
            .map(|m| (m.facing, m.sticky_type()))
    }

    /// Places a `moving_piston` cell with its block entity. The entity
    /// exists before the edit, so a head watching this cell sees its base.
    /// `flags` are the `Level.setBlock` flags Java places the cell with.
    pub(super) fn add_moving(&mut self, p: IVec3, moved: Block, facing: u8, extending: bool, source: bool, flags: u16) {
        let order = self.pistons.sequence;
        self.pistons.sequence += 1;
        self.pistons.moving.insert(
            p,
            Moving {
                moved,
                facing,
                extending,
                source,
                progress: 0.0,
                progress_o: 0.0,
                last_ticked: self.redstone.tick.wrapping_sub(1),
                order,
            },
        );
        if !self.edit_exact_flags(p, r::MOVING, flags) {
            self.pistons.moving.remove(&p);
        }
    }

    /// Java's `tickBlockEntities` for moving pistons, after block events
    /// (`tick_redstone` and `simulation::tick_world_rules` call it). Each
    /// block lands the tick after it reaches full progress; landing
    /// neighbour updates run at once, as Java's do inside the ticker. It
    /// also ends Java's `handlingTick`.
    pub fn tick_moving_pistons(&mut self) {
        self.pistons.handling_tick = false;
        self.pistons.pushes.clear();
        self.pistons.push_boxes.clear();
        if self.pistons.moving.is_empty() {
            return;
        }
        let mut order = std::mem::take(&mut self.pistons.order_scratch);
        order.clear();
        order
            .extend(self.pistons.moving.iter().filter(|(p, _)| self.is_block_ticking(**p)).map(|(&p, m)| (m.order, p)));
        order.sort_unstable_by_key(|&(o, _)| o);
        let now = self.redstone.tick;
        for &(_, p) in &order {
            let Some(m) = self.pistons.moving.get_mut(&p) else { continue };
            m.last_ticked = now;
            m.progress_o = m.progress;
            if m.progress_o >= 1.0 {
                let m = self.pistons.moving.remove(&p).unwrap();
                if self.get_block(p) == Some(r::MOVING) {
                    let landed = self.landing_state(p, &m, true);
                    let flags =
                        if landed == Block::AIR { super::events::flags::ALL } else { super::events::flags::PLACED };
                    self.edit_exact_flags(p, landed, flags);
                    self.landed_shape_updates(p, landed);
                    self.neighbor_changed_at(p);
                    self.settle(p);
                }
            } else {
                let m = *m;
                let next = m.progress + STEP;
                self.record_piston_pushes(p, &m, next);
                if let Some(m) = self.pistons.moving.get_mut(&p) {
                    m.progress = next.min(1.0);
                }
            }
        }
        self.pistons.order_scratch = order;
    }

    /// What a moving cell becomes: `updateFromNeighbourShapes` of the moved
    /// state. A head whose base is gone can't survive and lands as air, and
    /// `tick` lands a waterlogged block dry (`finalTick` does not).
    fn landing_state(&self, p: IVec3, m: &Moving, dry: bool) -> Block {
        if let Some(Component::PistonHead { facing, sticky }) = r::component(m.moved)
            && !self.head_survives(p, facing, sticky)
        {
            return Block::AIR;
        }
        let block = if dry { m.moved.with_water(false) } else { m.moved };
        self.shaped_by_neighbours(p, block)
    }

    /// The rest of `Block.updateFromNeighbourShapes` for a landed block: its
    /// own `updateShape` from each side, so an observer facing a cell that
    /// changed while it moved starts its pulse and an attached block without
    /// support breaks (VoxelCraft runs it once the cell holds the block).
    fn landed_shape_updates(&mut self, p: IVec3, landed: Block) {
        for d in super::super::redstone_order::UPDATE_SHAPE_ORDER {
            if self.get_block(p) != Some(landed) {
                return;
            }
            self.redstone_shape_update(p, landed, d);
        }
    }

    /// `PistonMovingBlockEntity.finalTick`: completes a move at once; a
    /// source's cell becomes air.
    pub(super) fn final_tick(&mut self, p: IVec3) {
        let Some(m) = self.moving_piston(p) else { return };
        if m.progress_o >= 1.0 {
            return;
        }
        self.pistons.moving.remove(&p);
        if self.get_block(p) == Some(r::MOVING) {
            let landed = if m.source { Block::AIR } else { self.landing_state(p, &m, false) };
            // `setBlockAndUpdate`, then `neighborChanged` at the cell.
            self.edit_exact_flags(p, landed, super::events::flags::ALL);
            if !m.source {
                self.landed_shape_updates(p, landed);
            }
            self.neighbor_changed_at(p);
        }
    }

    /// `MovingPistonBlock.getDrops`: a moving cell drops its moved block.
    pub(crate) fn moving_piston_drop(&self, p: IVec3) -> Option<Block> {
        self.moving_piston(p).map(|m| m.moved)
    }

    /// Calls `f` with every moving block collision box that may reach into
    /// `min..max`, offset by its progress (it can cross into a neighbouring
    /// cell). `noclip` is Java's `NOCLIP` direction while pistons push.
    pub(crate) fn piston_collision_boxes(
        &self,
        min: DVec3,
        max: DVec3,
        noclip: Option<IVec3>,
        f: &mut dyn FnMut(IVec3, DVec3, DVec3),
    ) {
        if self.pistons.moving.is_empty() {
            return;
        }
        let lo = (min - DVec3::splat(2.0)).floor().as_ivec3();
        let hi = (max + DVec3::splat(1.0)).floor().as_ivec3();
        for (&p, m) in &self.pistons.moving {
            if p.cmplt(lo).any() || p.cmpgt(hi).any() || self.get_block(p) != Some(r::MOVING) {
                continue;
            }
            m.collision(p, noclip, &mut |a, b| f(p, a, b));
        }
    }

    /// Interpolated moving blocks for the free-block renderer, one per
    /// moving cell: a retracting base stays in its cell as an extended base
    /// while its head slides home ([`World::moving_piston_heads`]).
    pub fn moving_piston_blocks(&self, alpha: f64) -> impl Iterator<Item = (Block, DVec3)> + '_ {
        let a = alpha.clamp(0.0, 1.0) as f32;
        self.pistons.moving.iter().map(move |(&p, m)| {
            if m.retracting_base() {
                return (r::piston(m.facing, m.sticky_type(), true), p.as_dvec3());
            }
            let progress = m.progress_o + (m.progress - m.progress_o) * a;
            (m.drawn(), p.as_dvec3() + m.direction().as_dvec3() * f64::from(m.extended_progress(progress)))
        })
    }

    /// The heads of retracting pistons, sliding home.
    pub fn moving_piston_heads(&self, alpha: f64) -> impl Iterator<Item = (Block, DVec3)> + '_ {
        let a = alpha.clamp(0.0, 1.0) as f32;
        self.pistons.moving.iter().filter(|(_, m)| m.retracting_base()).map(move |(&p, m)| {
            let progress = m.progress_o + (m.progress - m.progress_o) * a;
            (m.drawn(), p.as_dvec3() + m.direction().as_dvec3() * f64::from(m.extended_progress(progress)))
        })
    }

    /// Save records: `M` for each moving cell, in ticker order. Queued
    /// piston events are block events, saved with the `redstone` property.
    pub(crate) fn piston_save_entries(&self, out: &mut Vec<String>) {
        let mut cells: Vec<_> = self.pistons.moving.iter().collect();
        cells.sort_unstable_by_key(|(_, m)| m.order);
        for (p, m) in cells {
            out.push(format!(
                "M,{},{},{},{},{},{},{},{},{},{}",
                p.x,
                p.y,
                p.z,
                m.moved.0,
                m.facing,
                m.extending as u8,
                m.source as u8,
                m.progress,
                m.progress_o,
                self.redstone.tick.saturating_sub(m.last_ticked).min(1)
            ));
        }
    }

    /// Loads one `M` record, or migrates a v1 `m` record. Returns
    /// whether the entry was a piston record.
    pub(crate) fn load_piston_entry(&mut self, entry: &str) -> bool {
        let (kind, rest) = match entry.split_once(',') {
            Some((k @ ("M" | "m"), rest)) => (k, rest),
            _ => return false,
        };
        let fields: Vec<&str> = rest.split(',').collect();
        let int = |i: usize| fields.get(i).and_then(|s| s.parse::<i32>().ok());
        let pos = |i: usize| Some(IVec3::new(int(i)?, int(i + 1)?, int(i + 2)?));
        let flag = |i: usize| int(i).filter(|v| (0..=1).contains(v)).map(|v| v == 1);
        let block = |i: usize| {
            fields
                .get(i)
                .and_then(|s| s.parse::<u16>().ok())
                .filter(|&id| (id as usize) < crate::world::block::STATE_CAPACITY)
                .map(Block)
        };
        match kind {
            "M" => {
                let parsed = (|| {
                    let progress: f32 = fields.get(7)?.parse().ok()?;
                    let progress_o: f32 = fields.get(8)?.parse().ok()?;
                    let since = int(9).filter(|v| (0..=1).contains(v))?;
                    (fields.len() == 10 && (0.0..=1.0).contains(&progress) && (0.0..=1.0).contains(&progress_o))
                        .then_some(())?;
                    Some((
                        pos(0)?,
                        block(3)?,
                        int(4).filter(|f| (0..6).contains(f))? as u8,
                        flag(5)?,
                        flag(6)?,
                        progress,
                        progress_o,
                        since,
                    ))
                })();
                if let Some((p, moved, facing, extending, source, progress, progress_o, since)) = parsed {
                    let order = self.pistons.sequence;
                    self.pistons.sequence += 1;
                    let last_ticked = self.redstone.tick.saturating_sub(since as u64);
                    self.pistons.moving.insert(
                        p,
                        Moving { moved, facing, extending, source, progress, progress_o, last_ticked, order },
                    );
                }
            }
            _ => self.migrate_v1_moving(&fields),
        }
        true
    }

    /// A v1 `m` record (`to, block, from, base, age, install`): installed
    /// cells become block entities with their age as progress; a retracting
    /// head (not installed) was drawn only and is dropped, since v1 had
    /// already placed its retracted base.
    fn migrate_v1_moving(&mut self, fields: &[&str]) {
        let Ok(n) = fields.iter().map(|s| s.parse::<i64>()).collect::<Result<Vec<_>, _>>() else { return };
        let &[x, y, z, block, fx, fy, fz, bx, by, bz, age, install] = n.as_slice() else { return };
        if !(0..crate::world::block::STATE_CAPACITY as i64).contains(&block) || !(0..=2).contains(&age) || install != 1
        {
            return;
        }
        if [x, y, z, fx, fy, fz, bx, by, bz].into_iter().any(|v| i32::try_from(v).is_err()) {
            return;
        }
        let v = |a: i64, b: i64, c: i64| IVec3::new(a as i32, b as i32, c as i32);
        let (to, from, base) = (v(x, y, z), v(fx, fy, fz), v(bx, by, bz));
        let dir = (to - base).signum();
        let Some(facing) = (0..6u8).find(|&f| r::direction(f) == dir) else { return };
        let moved = Block(block as u16);
        let progress = age as f32 * STEP;
        let order = self.pistons.sequence;
        self.pistons.sequence += 1;
        self.pistons.moving.insert(
            to,
            Moving {
                moved,
                facing,
                extending: to - from == dir,
                source: matches!(r::component(moved), Some(Component::PistonHead { .. })),
                progress,
                progress_o: (progress - STEP).max(0.0),
                last_ticked: self.redstone.tick.saturating_sub(1),
                order,
            },
        );
    }
}
