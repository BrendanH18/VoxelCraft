//! Validated block placement; input/presentation and other item uses stay in adapters.
use super::PlayerSession;
use crate::{
    entity::PlayerId,
    inventory::HOTBAR_SLOTS,
    player::Player,
    world::{
        World,
        block::{Block, Facing, RenderKind},
        overworld_blocks as ob,
        terrain::Dimension,
    },
};
use glam::{DVec3, IVec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Block,
    Bed,
    Door,
    Ladder,
    CombinedSlab,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Placed { player: PlayerId, cell: IVec3, block: Block, kind: Kind },
    WaterEvaporated { player: PlayerId, cell: IVec3 },
}

impl PlayerSession {
    /// Place from the selected stack against an engine-computed raycast.
    /// All destination/support/player checks precede edits, spending and RNG use.
    /// Existing block-state conventions are retained; full shape/hinge parity
    /// is a separate registry/geometry task.
    pub fn place_held(&mut self, slot: usize, world: &mut World, others: &[DVec3]) -> Result<Outcome, &'static str> {
        if !self.mode.can_build() || self.vitals.is_dead() || self.sleeping.is_some() {
            return Err("player cannot place now");
        }
        if self.player.vehicle.is_some() {
            return Err("cannot place while riding");
        }
        if slot >= HOTBAR_SLOTS {
            return Err("selected slot out of range");
        }
        let feet = self.player.pos.floor().as_ivec3();
        if !self.player.pos.is_finite()
            || !self.player.yaw.is_finite()
            || !self.player.pitch.is_finite()
            || !world.is_loaded(feet)
            || !world.is_loaded(feet - IVec3::Y)
            || others.iter().any(|p| !p.is_finite())
        {
            return Err("player terrain or view is unavailable");
        }
        let held = self.inventory.get(slot).ok_or("selected slot empty")?;
        if held.count == 0 || held.count > held.item.max_stack() {
            return Err("invalid held stack");
        }
        let (hit, normal) = self.block_target(world).ok_or("no block within reach")?;
        let clicked = world.get_block(hit).ok_or("target is unloaded")?;
        // `SnowLayerBlock.canBeReplaced`: a snow layer item stacks onto a snow
        // block of fewer than eight layers, from its top face or in the cell
        // beside it; another face of a snow block places next to it.
        let snow = ob::snow_layers(clicked).filter(|_| held.item.places() == Some(ob::SNOW_LAYER));
        let at = if snow.is_some() {
            if normal == IVec3::Y && snow < Some(8) { hit } else { hit + normal }
        } else if clicked.is_replaceable() {
            hit
        } else {
            hit + normal
        };
        // Horizontal placement follows yaw even when looking straight down/up.
        let facing = Facing::toward(glam::Vec3::new(self.player.yaw.cos(), 0., self.player.yaw.sin()));
        let blocked =
            |cell| self.player.intersects_block(cell) || others.iter().any(|&p| Player::new(p).intersects_block(cell));
        let free = |world: &World, cell: IVec3| {
            cell.y >= world.min_y()
                && cell.y < world.max_y()
                && world.is_loaded(cell)
                && world.get_block(cell).is_some_and(Block::is_replaceable)
        };
        let supported = |world: &World, cell| world.get_block(cell - IVec3::Y).is_some_and(Block::is_opaque);
        let (cell, mut block, partner, kind) = if let Some(color) = held.item.bed_color() {
            let head = at + facing.opposite().offset();
            if !free(world, at)
                || !free(world, head)
                || !supported(world, at)
                || !supported(world, head)
                || blocked(at)
                || blocked(head)
            {
                return Err("bed does not fit");
            }
            (at, Block::colored_bed(color, false), Some((head, Block::colored_bed(color, true))), Kind::Bed)
        } else if let Some([lower, upper]) = crate::content::placement::door_halves(held.item, facing) {
            let up = at + IVec3::Y;
            if up.y >= world.max_y()
                || !free(world, at)
                || !free(world, up)
                || !supported(world, at)
                || blocked(at)
                || blocked(up)
            {
                return Err("door does not fit");
            }
            (at, lower, Some((up, upper)), Kind::Door)
        } else {
            let raw = held.item.places().ok_or("selected item cannot be placed")?;
            if raw.kind() == RenderKind::Invisible || raw.is_fire() {
                return Err("this block cannot be placed");
            }
            if raw.is_ladder() {
                let direction = Facing::from_offset(normal).ok_or("ladders need a wall")?;
                let cell = hit + normal;
                if !clicked.is_opaque() || !free(world, cell) || blocked(cell) {
                    return Err("ladder does not fit");
                }
                (cell, raw.with_facing(direction), None, Kind::Ladder)
            } else if raw.is_slab() && normal == IVec3::Y && clicked == raw {
                let full = raw.slab_base().ok_or("unsupported slab")?;
                if blocked(hit) {
                    return Err("placement intersects a player");
                }
                (hit, full, None, Kind::CombinedSlab)
            } else {
                let mut raw = raw;
                if raw == ob::SNOW_LAYER
                    && let Some(layers) = world.get_block(at).and_then(ob::snow_layers).filter(|&n| n < 8)
                {
                    // Stacking replaces the existing snow block with one more layer.
                    raw = ob::snow_layer(layers + 1);
                    if blocked(at) && raw.height() > 0.0 {
                        return Err("placement intersects a player");
                    }
                    let below = world.get_block(at - IVec3::Y);
                    if !below.is_some_and(|b| ob::can_stay_on(raw, b).unwrap_or(true)) {
                        return Err("block cannot survive here");
                    }
                    (at, raw, None, Kind::Block)
                } else {
                    let block = ob::placed(raw, normal).unwrap_or(raw);
                    let block = crate::world::village_blocks::placed(
                        crate::world::nether_blocks::placed(block, normal),
                        normal,
                    )
                    .with_facing(facing);
                    let block = crate::world::redstone_blocks::placed_with_look(block, normal, self.player.forward());
                    if !free(world, at) {
                        return Err("destination occupied or unloaded");
                    }
                    let below = world.get_block(at - IVec3::Y);
                    let support = world
                        .overworld_placement_ok(at, block)
                        .unwrap_or_else(|| below.is_some_and(|b| block.can_stay_on(b)))
                        && (block != Block::SUGAR_CANE || below == Some(Block::SUGAR_CANE) || world.cane_has_water(at))
                        && (!block.is_mushroom() || world.mushroom_survives(at));
                    if !support || !world.redstone_supported(at, block) {
                        return Err("block cannot survive here");
                    }
                    let partner = if let Some((lower, false)) = ob::double_of(block) {
                        Some((at + IVec3::Y, Block(lower.0 + 1)))
                    } else if block == ob::TALL_SEAGRASS {
                        Some((at + IVec3::Y, ob::TALL_SEAGRASS_TOP))
                    } else {
                        None
                    };
                    if (block.is_solid() && blocked(at))
                        || partner.is_some_and(|(p, b)| !free(world, p) || (b.is_solid() && blocked(p)))
                    {
                        return Err("placement intersects a player or occupied partner cell");
                    }
                    if block.is_water() && world.generator.dimension == Dimension::Nether {
                        return Ok(Outcome::WaterEvaporated { player: self.id, cell: at });
                    }
                    (at, block, partner, Kind::Block)
                }
            }
        };
        // Complete validation before rolling vine age or changing neighbor/drop rules.
        block = world.placed_vine_age(cell, block)?;
        world.touch_redstone_ore(hit);
        if kind == Kind::Block {
            world.before_overworld_place(cell, block);
        }
        let placed = world.set_block(cell, block);
        debug_assert!(placed, "validated loaded placement must succeed");
        if kind != Kind::Block
            && let Some((pos, partner)) = partner
        {
            let placed = world.set_block(pos, partner);
            debug_assert!(placed, "validated loaded partner must succeed");
        }
        if self.mode.is_survival() {
            self.inventory.take_one(slot);
        }
        Ok(Outcome::Placed { player: self.id, cell, block, kind })
    }
}
