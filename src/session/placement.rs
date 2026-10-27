//! Validated block placement; input/presentation and other item uses stay in adapters.
use super::PlayerSession;
use crate::item::Item;
use crate::{
    entity::PlayerId,
    inventory::HOTBAR_SLOTS,
    player::Player,
    world::{
        World,
        block::{Block, Facing, RenderKind},
        colour_blocks::{self as cb, candle, straw_bed},
        overworld_blocks as ob, slab_stair_states as ss,
        terrain::Dimension,
        waterlog,
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

/// What Java's `BlockPlaceContext` gives `getStateForPlacement`: where the
/// player clicked and how they were facing. Family placement rules
/// (slab halves, stair halves, door hinges, button faces, waterlogging)
/// read it instead of re-deriving the raycast.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaceContext {
    /// The clicked cell and the block in it.
    pub clicked: IVec3,
    pub clicked_block: Block,
    /// Outward normal of the clicked face (Java's `getClickedFace`).
    pub face: IVec3,
    /// The click point relative to the clicked cell's minimum corner, each
    /// axis in 0..=1 (Java's `getClickLocation` minus the clicked position).
    pub click: DVec3,
    /// The cell the block goes into: the clicked cell when it is replaceable
    /// or the held slab merges into it, otherwise the cell across `face`.
    pub at: IVec3,
    /// The horizontal direction the player looks along (Java's
    /// `getHorizontalDirection`). Opposite to the legacy `Facing::toward`,
    /// which gives the front facing back at the player.
    pub facing: Facing,
    /// The player's unit look vector (Java's `getNearestLookingDirections`).
    pub look: glam::Vec3,
    pub sneaking: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Placed { player: PlayerId, cell: IVec3, block: Block, kind: Kind },
    WaterEvaporated { player: PlayerId, cell: IVec3 },
}

impl PlayerSession {
    /// The placement context for `held` against `target`, whose default
    /// destination (after replaceable and snow rules) is `at`.
    fn place_context(&self, target: crate::world::BlockHit, clicked: Block, held: Item, at: IVec3) -> PlaceContext {
        let (hit, normal) = (target.cell, target.normal);
        let click = (target.point - hit.as_dvec3()).clamp(DVec3::ZERO, DVec3::ONE);
        // `SlabBlock.canBeReplaced` with `replacingClickedOnBlock`: a single
        // slab of the held kind clicked on its open side takes the other half.
        let merges_slab = held.places().is_some_and(|r| ss::slab_takes_click(clicked, r, normal, click.y));
        // `CandleBlock.canBeReplaced`: a candle of the held kind takes another.
        let merges_slab =
            merges_slab || held.places().is_some_and(|r| candle::takes_another(clicked, r, self.player.sneaking));
        PlaceContext {
            clicked: hit,
            clicked_block: clicked,
            face: normal,
            click,
            at: if merges_slab { hit } else { at },
            facing: Facing::toward(glam::Vec3::new(self.player.yaw.cos(), 0., self.player.yaw.sin())).opposite(),
            look: self.player.forward(),
            sneaking: self.player.sneaking,
        }
    }

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
        let target = if held.item == Item::SCAFFOLDING {
            world.raycast_scaffolding(self.player.eye(), self.player.forward().as_dvec3(), self.block_reach())
        } else {
            self.block_hit(world)
        }
        .ok_or("no block within reach")?;
        let (hit, normal) = (target.cell, target.normal);
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
        let mut ctx = self.place_context(target, clicked, held.item, at);
        if held.item == Item::SCAFFOLDING {
            let inside = self.player.eye().floor().as_ivec3() == hit;
            // A ray beginning inside returns the face opposite its dominant look
            // axis (`Direction.getApproximateNearest`: ties go to Y, then Z).
            if ctx.face == IVec3::ZERO {
                let look = self.player.forward();
                let axis = if look.x.abs() > look.y.abs() && look.x.abs() > look.z.abs() {
                    0
                } else if look.y.abs() >= look.z.abs() {
                    1
                } else {
                    2
                };
                ctx.face[axis] = if look[axis] > 0.0 { -1 } else { 1 };
            }
            ctx.at = crate::world::scaffolding::placement_target(
                |p| world.get_block(p),
                &ctx,
                inside,
                world.min_y(),
                world.max_y(),
            )
            .ok_or("scaffolding cannot extend here")?;
        }
        let at = ctx.at;
        let blocked =
            |cell| self.player.intersects_block(cell) || others.iter().any(|&p| Player::new(p).intersects_block(cell));
        let free = |world: &World, cell: IVec3| {
            cell.y >= world.min_y()
                && cell.y < world.max_y()
                && world.is_loaded(cell)
                && world.get_block(cell).is_some_and(Block::is_replaceable)
        };
        let supported = |world: &World, cell| {
            world.get_block(cell - IVec3::Y).is_some_and(|b| crate::world::mount_states::face_sturdy(b, IVec3::Y))
        };
        // Java's `isUnobstructed`: the placed state's collision shape against players.
        let obstructed = |world: &World, cell: IVec3, block: Block| {
            let boxes = collision_boxes(world, cell, block);
            boxes.iter().any(|&(lo, hi)| {
                self.player.intersects_box(lo, hi) || others.iter().any(|&p| Player::new(p).intersects_box(lo, hi))
            })
        };
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
        } else if held.item.places() == Some(straw_bed::ITEM_BLOCK) {
            // `AbstractBedBlock.getStateForPlacement` and `setPlacedBy`: the
            // head goes the way the player looks; beds need no support.
            let toward = facing.opposite();
            let head = at + toward.offset();
            if !free(world, at) || !free(world, head) || blocked(at) || blocked(head) {
                return Err("bed does not fit");
            }
            (at, straw_bed::half(toward, false), Some((head, straw_bed::half(toward, true))), Kind::Bed)
        } else if let Some([lower, upper]) = crate::content::placement::door_halves(held.item, facing) {
            // `DoorBlock.getHinge`: neighbouring doors and solid blocks, then the click.
            let lower = if crate::world::mount_states::door_hinge_right(&ctx, |c| world.get_block(c)) {
                crate::world::mount_states::right_of(lower).unwrap_or(lower)
            } else {
                lower
            };
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
            // Java `DoorBlock.getStateForPlacement`: either half receiving
            // a neighbour signal sets OPEN and POWERED before the first tick.
            let powered = world.redstone_power(at) > 0 || world.redstone_power(up) > 0;
            let (lower, upper) = if powered { (lower.toggled(facing), upper.toggled(facing)) } else { (lower, upper) };
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
            } else if world.get_block(at).is_some_and(|b| candle::takes_another(b, raw, self.player.sneaking)) {
                // `CandleBlock.getStateForPlacement` on its own kind: one more candle.
                let more = candle::placed(raw, world.get_block(at).unwrap_or(Block::AIR));
                if obstructed(world, at, more) {
                    return Err("placement intersects a player");
                }
                (at, more, None, Kind::Block)
            } else if world.get_block(at).is_some_and(|b| ss::slab_takes_placement(b, raw)) {
                // `SlabBlock.getStateForPlacement` on a slab of the same kind:
                // the double slab, never waterlogged.
                let double = ss::placed(raw, world.get_block(at).unwrap_or(Block::AIR), normal, ctx.click.y, |_| None);
                if obstructed(world, at, double) {
                    return Err("placement intersects a player");
                }
                (at, double, None, Kind::CombinedSlab)
            } else {
                let mut raw = raw;
                if crate::world::scaffolding::is_scaffolding(raw) {
                    raw = crate::world::scaffolding::placed(|p| world.get_block(p), at, false);
                }
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
                    // Levers, buttons, grindstones, lanterns and redstone torches
                    // follow Java's nearest-looking-direction rules.
                    let block = if let Some(placed) = crate::world::wood::placed(raw, &ctx, |c| world.get_block(c)) {
                        // Signs, shelves and chiseled bookshelves (`world::wood`).
                        placed.ok_or("block cannot survive here")?
                    } else if let Some(banner) = cb::placed(raw, &ctx, |c| world.get_block(c)) {
                        banner.ok_or("block cannot survive here")?
                    } else if let Some(mounted) = crate::world::copper::placed(raw, &ctx, |c| world.get_block(c))
                        .or_else(|| crate::world::mount_states::placed(raw, &ctx, |c| world.get_block(c)))
                    {
                        mounted.ok_or("block cannot survive here")?
                    } else {
                        let block = ob::placed(raw, normal).unwrap_or(raw);
                        let block = crate::world::village_blocks::placed(
                            crate::world::nether_blocks::placed(block, normal),
                            normal,
                        )
                        .with_facing(facing);
                        let block =
                            crate::world::redstone_blocks::placed_with_look(block, normal, self.player.forward());
                        let block = crate::world::village_blocks::placed_with_look(block, self.player.forward());
                        // Slab type and stairs half and corner shape (Java's
                        // `getStateForPlacement` of `SlabBlock` and `StairBlock`).
                        ss::placed(block, world.get_block(at).unwrap_or(Block::AIR), normal, ctx.click.y, |f| {
                            world.get_block(at + f.offset())
                        })
                    };
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
                    // Partial shapes (and signs and banners, which have no
                    // collision) check their own collision (`isUnobstructed`).
                    let shaped_half = ss::stairs(block).is_some()
                        || ss::slab(block).is_some()
                        || crate::world::wood::owns(block.0)
                        || cb::owns(block.0);
                    if (block.is_solid() && if shaped_half { obstructed(world, at, block) } else { blocked(at) })
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
        // Every `SimpleWaterloggedBlock.getStateForPlacement`: waterlogged
        // exactly when the final cell holds a water source.
        let watered =
            |world: &World, at: IVec3, b: Block| b.with_water(world.get_block(at).is_some_and(waterlog::holds_source));
        block = watered(world, cell, block);
        let partner = partner.map(|(at, b)| (at, watered(world, at, b)));
        block = crate::world::wall_states::reshaped(block, |d| world.get_block(cell + d).unwrap_or(Block::AIR));
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
        if kind == Kind::Door {
            world.finish_door_placement(cell);
        }
        if self.mode.is_survival() {
            self.inventory.take_one(slot);
        }
        Ok(Outcome::Placed { player: self.id, cell, block, kind })
    }
}

/// World-space collision boxes `block` would have at `cell`.
fn collision_boxes(world: &World, cell: IVec3, block: Block) -> Vec<(DVec3, DVec3)> {
    let lo = cell.as_dvec3();
    let neighbour = |f: Facing| world.get_block(cell + f.offset()).unwrap_or(Block::AIR);
    let below = world.get_block(cell - IVec3::Y).unwrap_or(Block::AIR);
    let v = |c: [u8; 3]| DVec3::new(c[0] as f64, c[1] as f64, c[2] as f64) / 16.0;
    crate::world::shape::collision_or_cube(block, neighbour, below, crate::world::shape::Above::NONE)
        .as_slice()
        .iter()
        .map(|b| (lo + v(b.min), lo + v(b.max)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BlockHit;

    #[test]
    fn place_context_faces_where_the_player_looks_and_merges_slabs_in_place() {
        let mut session = PlayerSession::new(PlayerId(0), DVec3::ZERO);
        let slab = Block::slab_of(Block::STONE).unwrap();
        let cell = IVec3::new(7, 145, 4);
        let hit = BlockHit { cell, normal: IVec3::Y, point: DVec3::new(7.25, 146.0, 4.75) };
        for (yaw, facing) in [
            (0.0f32, Facing::East),
            (std::f32::consts::FRAC_PI_2, Facing::South),
            (std::f32::consts::PI, Facing::West),
            (-std::f32::consts::FRAC_PI_2, Facing::North),
        ] {
            session.player.yaw = yaw;
            let ctx = session.place_context(hit, slab, Item::from_block(Block::STONE), cell + IVec3::Y);
            assert_eq!(ctx.facing, facing, "yaw {yaw}");
            assert!(ctx.look.dot(ctx.facing.offset().as_vec3()) > 0.99, "look and facing agree at yaw {yaw}");
            assert_eq!(ctx.at, cell + IVec3::Y);
            assert_eq!(ctx.click, DVec3::new(0.25, 1.0, 0.75));
        }
        let merging = session.place_context(hit, slab, Item::from_block(slab), cell + IVec3::Y);
        assert_eq!(merging.at, cell, "a matching slab clicked from above merges into its own cell");
    }
}
