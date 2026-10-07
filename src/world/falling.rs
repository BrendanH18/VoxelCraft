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
}

/// Whether a falling block passes through (and lands over) this block.
pub fn can_fall_into(b: Block) -> bool {
    b == Block::AIR || b.is_fluid() || b.kind() == RenderKind::Cross
}

impl World {
    /// Reacts to the block at `p` changing: wakes fluids, drops the block
    /// if it is an unsupported gravity block, and pops off or drops whatever
    /// rested on it, all the way up.
    pub(super) fn settle(&mut self, mut p: IVec3) {
        self.drop_unhung_ladders(p);
        loop {
            self.wake_fluids(p);
            let Some(b) = self.get_block(p) else { return };
            if b.has_gravity() && self.get_block(p - IVec3::Y).is_some_and(can_fall_into) {
                self.edit(p, Block::AIR, true);
                self.falling.push(FallingBlock { block: b, pos: p.as_dvec3(), previous_pos: p.as_dvec3(), speed: 0.0 });
                continue; // `p` is air now: look at what was on top of it
            }
            let above = p + IVec3::Y;
            match self.get_block(above) {
                Some(a) if !a.can_stay_on(b) => {
                    // A plant or torch that just lost its support pops off.
                    self.edit(above, Block::AIR, true);
                    self.spill_block(above, a);
                    p = above;
                }
                Some(a) if a.has_gravity() && can_fall_into(b) => p = above,
                _ => return,
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
            f.speed = (f.speed + GRAVITY * dt).min(TERMINAL_SPEED);
            let target = f.pos.y - f.speed * dt;
            let cell = f.pos.floor().as_ivec3();
            // Check every cell the bottom face is in or passes into, so fast
            // blocks never tunnel through a floor, and a block falling right
            // behind another stacks on it once that one lands.
            let mut y = cell.y;
            while y as f64 + 1.0 > target {
                match self.get_block(IVec3::new(cell.x, y, cell.z)) {
                    Some(b) if can_fall_into(b) && y >= 0 => y -= 1,
                    // Unloaded below: wait for the chunk rather than vanish.
                    None => return true,
                    Some(_) => {
                        landed.push((IVec3::new(cell.x, y + 1, cell.z), f.block));
                        return false;
                    }
                }
            }
            f.pos.y = target;
            true
        });
        self.falling = falling;
        for (p, block) in landed {
            // Something may have been built in the way since; then it's lost.
            if self.get_block(p).is_some_and(can_fall_into) {
                self.edit(p, block, true);
                self.settle(p);
            }
        }
    }

    /// Destroys the blocks in a ragged sphere around `center` (radius about
    /// `0.9 * power`, like a Minecraft blast in open stone). Bedrock,
    /// obsidian, ancient debris, Netherite blocks and fluids resist. Each
    /// destroyed block drops with a chance of `1 / power`, like Minecraft.
    /// Chunks are remeshed on the workers;
    /// what rested on the blasted blocks falls or pops off. Returns how many
    /// blocks were destroyed.
    pub fn explode(&mut self, center: DVec3, power: f64) -> usize {
        let radius = power * 0.9 + 0.5;
        let c = center.floor().as_ivec3();
        let r = radius.ceil() as i32;
        let seed = self.generator.seed ^ 0xB1A5;
        let mut removed = Vec::new();
        for dy in -r..=r {
            for dz in -r..=r {
                for dx in -r..=r {
                    let p = c + IVec3::new(dx, dy, dz);
                    let dist = (p.as_dvec3() + DVec3::splat(0.5) - center).length();
                    if dist > radius * (0.7 + 0.3 * super::noise::hash_f(p.x, p.y, p.z, seed) as f64) {
                        continue;
                    }
                    let Some(b) = self.get_block(p) else { continue };
                    let resists = matches!(
                        b,
                        Block::AIR | Block::BEDROCK | Block::OBSIDIAN | Block::ANCIENT_DEBRIS | Block::NETHERITE_BLOCK
                    ) || b.is_fluid();
                    if !resists && self.edit(p, Block::AIR, false) {
                        removed.push(p);
                        if b == Block::TNT {
                            self.primed_tnt.push((p, true));
                        } else if (super::noise::hash_f(p.x, p.y, p.z, seed ^ 0xD20F) as f64) < 1.0 / power {
                            self.spill_block(p, b);
                        }
                    }
                }
            }
        }
        for &p in &removed {
            self.settle(p);
        }
        self.update_block_light();
        removed.len()
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
