//! Gravity blocks (sand, gravel). When nothing holds one up it leaves the
//! grid and falls as a free block, then lands back into the grid on the
//! first cell it can't fall into. Plants and fluids don't hold it up; it
//! replaces them when it lands.

use glam::{DVec3, IVec3};

use super::World;
use super::block::{Block, Facing, RenderKind, Shaped};

/// Blocks per second squared (Minecraft's 0.04 per tick², with drag).
const GRAVITY: f64 = 32.0;
const TERMINAL_SPEED: f64 = 40.0;

#[derive(Clone, Copy, Debug)]
pub struct FallingBlock {
    pub block: Block,
    /// Minimum corner; x and z stay on the grid.
    pub pos: DVec3,
    pub previous_pos: DVec3,
    speed: f64,
    /// Damage per block fallen to whatever it lands on
    /// (`FallingBlockEntity.setHurtsEntities`; only a stalactite's tip).
    pub(super) hurt: f32,
    /// Where it started falling, for the fall distance.
    pub(super) from_y: f64,
}

impl FallingBlock {
    pub(super) fn new(block: Block, pos: DVec3, speed: f64) -> Self {
        FallingBlock { block, pos, previous_pos: pos, speed, hurt: 0.0, from_y: pos.y }
    }
}

/// Whether a falling block passes through (and lands over) this block.
pub fn can_fall_into(b: Block) -> bool {
    b == Block::AIR || b.is_fluid() || b.kind() == RenderKind::Cross
}

impl World {
    /// `FallingBlockEntity.fall`: the cell keeps its fluid, the entity is dry.
    pub(super) fn start_scaffolding_fall(&mut self, p: IVec3, block: Block, wet: bool) {
        self.set_block(p, if wet { Block::WATER } else { Block::AIR });
        self.falling.push(FallingBlock::new(block, p.as_dvec3(), 0.0));
    }

    /// Optional dimension property. Removed source cells and these in-flight
    /// blocks are saved in the same world snapshot, preventing loss mid-fall.
    pub fn falling_to_string(&self) -> String {
        let mut entries = vec!["v1".to_string()];
        let mut stored: Vec<_> = self.stored_falling.iter().collect();
        stored.sort_unstable_by_key(|(p, _)| (p.x, p.y, p.z));
        entries.extend(self.falling.iter().chain(stored.into_iter().flat_map(|(_, blocks)| blocks)).map(|f| {
            let entry = format!("{},{},{},{},{}", f.block.0, f.pos.x, f.pos.y, f.pos.z, f.speed);
            // A stalactite's tip also keeps its damage and start height.
            if f.hurt > 0.0 { format!("{entry},{},{}", f.hurt, f.from_y) } else { entry }
        }));
        entries.join("|")
    }

    /// Moves the falling blocks of chunk `pos`, which is unloading, into
    /// storage until the chunk loads again.
    pub(super) fn store_falling(&mut self, pos: IVec3) {
        if self.falling.is_empty() {
            return;
        }
        let mut kept = Vec::with_capacity(self.falling.len());
        for f in std::mem::take(&mut self.falling) {
            if self.entity_chunk(f.pos) == pos {
                self.stored_falling.entry(pos).or_default().push(f);
            } else {
                kept.push(f);
            }
        }
        self.falling = kept;
    }

    /// Falling blocks waiting in unloaded chunks.
    pub fn stored_falling_blocks(&self) -> usize {
        self.stored_falling.values().map(Vec::len).sum()
    }

    pub fn load_falling(&mut self, text: &str) {
        let mut entries = text.split('|');
        if entries.next() != Some("v1") {
            return;
        }
        self.falling.clear();
        self.stored_falling.clear();
        for entry in entries {
            let fields: Vec<_> = entry.split(',').collect();
            let parsed = (|| {
                let (&[block, x, y, z, speed], tip) = fields.split_at_checked(5)? else {
                    return None;
                };
                let block = Block(block.parse().ok()?);
                let pos = DVec3::new(x.parse().ok()?, y.parse().ok()?, z.parse().ok()?);
                let speed: f64 = speed.parse().ok()?;
                let mut f = FallingBlock::new(block, pos, speed);
                if let &[hurt, from_y] = tip {
                    (f.hurt, f.from_y) = (hurt.parse().ok()?, from_y.parse().ok()?);
                } else if !tip.is_empty() {
                    return None;
                }
                ((block.has_gravity()
                    || super::scaffolding::is_scaffolding(block)
                    || super::overworld_blocks::dripstone_of(block.canonical()).is_some())
                    && pos.is_finite()
                    && speed.is_finite()
                    && f.hurt.is_finite()
                    && f.from_y.is_finite()
                    && (0.0..=TERMINAL_SPEED).contains(&speed))
                .then_some(f)
            })();
            if let Some(block) = parsed {
                // Blocks of chunks that aren't loaded wait in storage.
                let chunk = self.entity_chunk(block.pos);
                if self.chunk_loaded(chunk) {
                    self.falling.push(block);
                } else {
                    self.stored_falling.entry(chunk).or_default().push(block);
                }
            }
        }
    }

    /// A LAN client's copy of the host's falling blocks: drawn only.
    pub fn set_falling_replica(&mut self, blocks: impl IntoIterator<Item = (Block, DVec3, DVec3)>) {
        self.stored_falling.clear();
        self.falling = blocks
            .into_iter()
            .take(256)
            .map(|(block, pos, previous_pos)| FallingBlock { previous_pos, ..FallingBlock::new(block, pos, 0.0) })
            .collect();
    }

    /// Reacts to the block at `p` changing: wakes fluids, drops the block
    /// if it is an unsupported gravity block, and pops off or drops whatever
    /// rested on it, all the way up.
    pub(super) fn settle(&mut self, mut p: IVec3) {
        self.update_sponges(p);
        self.drop_unsupported_signs(p);
        self.drop_unhung_ladders(p);
        self.drop_unhung_lantern(p);
        self.drop_unsupported_copper(p);
        self.drop_unhung_colour_blocks(p);
        self.solidify_nearby_powder(p);
        self.update_nether_vines(p);
        self.update_overworld_plants(p);
        self.update_utilities(p);
        loop {
            self.wake_fluids(p);
            let Some(b) = self.get_block(p) else { return };
            if b.has_gravity() && self.get_block(p - IVec3::Y).is_some_and(can_fall_into) {
                self.edit(p, Block::AIR, true);
                self.falling.push(FallingBlock::new(b, p.as_dvec3(), 0.0));
                continue; // `p` is air now: look at what was on top of it
            }
            let above = p + IVec3::Y;
            match self.get_block(above) {
                Some(a) if !a.can_stay_on(b) => {
                    // A plant or torch that just lost its support pops off;
                    // kelp and seagrass leave their water behind.
                    self.edit(above, if a.is_waterlogged() { Block::WATER } else { Block::AIR }, true);
                    self.spill_block(above, a);
                    p = above;
                }
                Some(a) if a.has_gravity() && can_fall_into(b) => p = above,
                _ => return,
            }
        }
    }

    fn touches_water(&self, p: IVec3) -> bool {
        [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z]
            .into_iter()
            .any(|d| self.get_block(p + d).is_some_and(Block::holds_water))
    }

    fn solidify_powder(&self, p: IVec3, block: Block) -> Block {
        match block.concrete_powder_color() {
            Some(c) if self.get_block(p).is_some_and(Block::is_water) || self.touches_water(p) => Block::concrete(c),
            _ => block,
        }
    }

    fn solidify_nearby_powder(&mut self, p: IVec3) {
        for d in [IVec3::ZERO, IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z] {
            let at = p + d;
            if let Some(c) = self.get_block(at).and_then(Block::concrete_powder_color)
                && self.touches_water(at)
                && !self.get_block(at - IVec3::Y).is_some_and(can_fall_into)
            {
                self.edit(at, Block::concrete(c), true);
            }
        }
    }

    /// Ladders hung on `p` fall off once it's no longer a solid wall.
    fn drop_unhung_ladders(&mut self, p: IVec3) {
        if self.get_block(p).is_none_or(|b| b.is_opaque()) {
            return;
        }
        for f in Facing::ALL {
            let at = p + f.offset();
            if let Some(ladder) = self.get_block(at)
                && ladder.shaped() == Some(Shaped::Ladder(f))
            {
                self.edit(at, Block::AIR, true);
                self.spill_block(at, ladder);
            }
        }
    }

    /// `LanternBlock.updateShape`: a hanging lantern under `p` breaks when
    /// `p` can no longer hold it (`canSupportCenter` on its underside),
    /// leaving its water if waterlogged.
    fn drop_unhung_lantern(&mut self, p: IVec3) {
        let below = p - IVec3::Y;
        let Some(lantern) = self.get_block(below) else { return };
        if !super::mount_states::lantern_of(lantern).is_some_and(|l| l.hanging)
            || self.get_block(p).is_some_and(|b| super::mount_states::supports_center(b, IVec3::NEG_Y))
        {
            return;
        }
        self.edit(below, if lantern.is_waterlogged() { Block::WATER } else { Block::AIR }, true);
        self.spill_block(below, lantern);
    }

    /// Snap render interpolation when the offline game pauses.
    pub fn snapshot_falling_positions(&mut self) {
        for f in &mut self.falling {
            f.previous_pos = f.pos;
        }
    }

    /// Moves falling blocks and lands those that hit something.
    pub fn tick_falling(&mut self, dt: f64) {
        let mut landed = Vec::new();
        let mut falling = std::mem::take(&mut self.falling);
        falling.retain_mut(|f| {
            f.previous_pos = f.pos;
            if !self.is_entity_ticking(f.pos.floor().as_ivec3()) {
                return true;
            }
            f.speed = (f.speed + GRAVITY * dt).min(TERMINAL_SPEED);
            let target = f.pos.y - f.speed * dt;
            let cell = f.pos.floor().as_ivec3();
            // Check every cell the bottom face is in or passes into, so fast
            // blocks never tunnel through a floor, and a block falling right
            // behind another stacks on it once that one lands.
            let mut y = cell.y;
            while y as f64 + 1.0 > target {
                match self.get_block(IVec3::new(cell.x, y, cell.z)) {
                    Some(b) if can_fall_into(b) && y >= self.min_y() => {
                        if f.block.concrete_powder_color().is_some() && b.is_water() {
                            landed.push((IVec3::new(cell.x, y, cell.z), *f));
                            return false;
                        }
                        y -= 1;
                    }
                    // Unloaded below: wait for the chunk rather than vanish.
                    None => return true,
                    Some(_) => {
                        landed.push((IVec3::new(cell.x, y + 1, cell.z), *f));
                        return false;
                    }
                }
            }
            f.pos.y = target;
            true
        });
        self.falling = falling;
        self.clear_stalactite_hits();
        for (p, fell) in landed {
            let block = fell.block;
            // Pointed dripstone breaks where it lands (`onBrokenAfterFall`).
            if self.stalactite_landed(p, &fell) {
                continue;
            }
            // FallingBlockEntity only places scaffolding when canSurvive succeeds.
            // An unloaded support neighbour isn't proof it can't: place it
            // and let its scheduled tick decide once the chunk loads.
            let scaffold = super::scaffolding::is_scaffolding(block);
            let unknown = [IVec3::NEG_Y, IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z]
                .into_iter()
                .any(|d| self.get_block(p + d).is_none());
            if scaffold && !unknown && super::scaffolding::distance(|q| self.get_block(q), p) == 7 {
                self.spill_block(p, block);
                continue;
            }
            if scaffold && unknown {
                self.schedule_redstone(p, 1, 0);
            }
            // Something may have been built in the way since; then it's lost.
            if self.get_block(p).is_some_and(can_fall_into) {
                self.edit(p, self.solidify_powder(p, block), true);
                self.settle(p);
            }
        }
    }

    /// A Java blast of `power` at `center` that breaks blocks
    /// ([`World::exploded_cells`] then [`World::detonate`]): each drop
    /// survives by its loot table's explosion rules. Chunks are remeshed on
    /// the workers; what rested on the blasted blocks falls or pops off.
    /// Returns how many blocks were destroyed. Hurt entities first with
    /// [`crate::entity::Entities::explode`].
    pub fn explode(&mut self, center: DVec3, power: f64) -> usize {
        self.explode_with_drops(center, power, true)
    }

    /// As [`World::explode`], with block drops controlled by `drop_blocks`.
    pub fn explode_with_drops(&mut self, center: DVec3, power: f64, drop_blocks: bool) -> usize {
        self.explode_with_decay(center, power, drop_blocks, true)
    }

    /// As [`World::explode_with_drops`]. With `decay` (Java's
    /// `DESTROY_WITH_DECAY`) drops are rolled with the explosion radius;
    /// without it (`DESTROY`, which TNT uses by default) every drop stays.
    pub fn explode_with_decay(&mut self, center: DVec3, power: f64, drop_blocks: bool, decay: bool) -> usize {
        let cells = self.exploded_cells(center, power as f32);
        self.detonate(&cells, power as f32, drop_blocks, decay, false)
    }

    /// Removes many blocks at once without drops (the dragon smashing
    /// through), remeshing on the workers and relighting once.
    pub fn break_blocks(&mut self, cells: &[IVec3]) {
        let removed: Vec<IVec3> = cells.iter().copied().filter(|&p| self.edit(p, Block::AIR, false)).collect();
        for &p in &removed {
            self.settle(p);
        }
        self.update_block_light();
    }

    pub fn falling_blocks(&self) -> &[FallingBlock] {
        &self.falling
    }
}
