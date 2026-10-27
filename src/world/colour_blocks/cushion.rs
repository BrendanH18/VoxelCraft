//! Cushions: Java 26.3 `Cushion` (a `BlockAttachedEntity` players sit on)
//! and `CushionItem`. They ride the vehicle system as
//! `CartKind::Cushion`; this module holds their placement and survival
//! rules.

use glam::{DVec3, IVec3};

use super::super::World;
use super::super::block::{Block, Facing};
use super::super::shape;
use crate::physics::Shape;

/// `EntityTypes.CUSHION`: 1 wide, 0.25 tall.
pub const SHAPE: Shape = Shape::new(0.5, 0.25);
/// Java's 1/64-block slice under (and at the bottom of) a cushion.
const SLICE: f64 = 0.015625;

/// A rider's feet: the cushion's passenger attachment (its height) less a
/// player's vehicle attachment (`Avatar.DEFAULT_VEHICLE_ATTACHMENT`, 0.6).
pub fn seat(pos: DVec3) -> DVec3 {
    pos + DVec3::Y * (SHAPE.height - 0.6)
}

fn cell_bounds(world: &World, p: IVec3, collision: bool) -> Vec<(DVec3, DVec3)> {
    let Some(b) = world.get_block(p) else { return Vec::new() };
    if b == Block::AIR || b.is_fluid() {
        return Vec::new();
    }
    let neighbour = |f: Facing| world.get_block(p + f.offset()).unwrap_or(Block::AIR);
    let below = world.get_block(p - IVec3::Y).unwrap_or(Block::AIR);
    let boxes = if collision {
        shape::collision_or_cube(b, neighbour, below, shape::Above::NONE)
    } else {
        shape::outline(b, neighbour, below)
    };
    let v = |c: [u8; 3]| DVec3::new(c[0] as f64, c[1] as f64, c[2] as f64) / 16.0;
    boxes.as_slice().iter().map(|bx| (p.as_dvec3() + v(bx.min), p.as_dvec3() + v(bx.max))).collect()
}

fn intersects(a: (DVec3, DVec3), b: (DVec3, DVec3)) -> bool {
    a.0.cmplt(b.1).all() && a.1.cmpgt(b.0).all()
}

/// `Cushion.hasAnchorBelow`: some block's shape (as a whole box) reaches
/// the 1/64-block slice under the cushion's bottom.
fn anchored(world: &World, pos: DVec3) -> bool {
    let (min, max) = SHAPE.aabb(pos);
    let anchor = (DVec3::new(min.x, min.y - SLICE, min.z), DVec3::new(max.x - 1e-9, min.y, max.z - 1e-9));
    let lo = DVec3::new(anchor.0.x, anchor.0.y - 0.125, anchor.0.z).floor().as_ivec3();
    let hi = anchor.1.floor().as_ivec3();
    (lo.y..=hi.y).any(|y| {
        (lo.z..=hi.z).any(|z| {
            (lo.x..=hi.x).any(|x| {
                let bounds = cell_bounds(world, IVec3::new(x, y, z), false);
                let Some(first) = bounds.first().copied() else { return false };
                let whole = bounds.iter().fold(first, |a, b| (a.0.min(b.0), a.1.max(b.1)));
                intersects(whole, anchor)
            })
        })
    })
}

/// `Cushion.isCoveredBySuffocatingBlocks`: every cell it occupies is a
/// suffocating (full, opaque) block.
fn covered(world: &World, pos: DVec3) -> bool {
    let (min, max) = SHAPE.aabb(pos);
    let (lo, hi) = ((min + DVec3::splat(1e-7)).floor().as_ivec3(), (max - DVec3::splat(1e-7)).floor().as_ivec3());
    (lo.y..=hi.y).all(|y| {
        (lo.z..=hi.z).all(|z| (lo.x..=hi.x).all(|x| world.get_block(IVec3::new(x, y, z)).is_none_or(|b| b.is_opaque())))
    })
}

/// `Cushion.isAnchorBuried`: block collision covers all of the 1/64-block
/// slice at the cushion's bottom.
fn buried(world: &World, pos: DVec3) -> bool {
    let (min, max) = SHAPE.aabb(pos);
    let slice = (min + DVec3::splat(1e-7), DVec3::new(max.x, min.y + SLICE, max.z) - DVec3::splat(1e-7));
    let (lo, hi) = (slice.0.floor().as_ivec3(), slice.1.floor().as_ivec3());
    // Cover a 32 × 32 grid of the slice's footprint.
    let mut covered = [[false; 32]; 32];
    for y in lo.y..=hi.y {
        for z in lo.z..=hi.z {
            for x in lo.x..=hi.x {
                for b in cell_bounds(world, IVec3::new(x, y, z), true) {
                    if !(b.0.y <= slice.0.y && b.1.y >= slice.1.y) {
                        continue;
                    }
                    for (i, row) in covered.iter_mut().enumerate() {
                        let cz = min.z + (i as f64 + 0.5) / 32.0;
                        for (j, cell) in row.iter_mut().enumerate() {
                            let cx = min.x + (j as f64 + 0.5) / 32.0;
                            *cell |= (b.0.x..b.1.x).contains(&cx) && (b.0.z..b.1.z).contains(&cz);
                        }
                    }
                }
            }
        }
    }
    covered.iter().flatten().all(|&c| c)
}

/// `Cushion.wouldSurviveAt`.
pub fn survives(world: &World, pos: DVec3) -> bool {
    anchored(world, pos) && !covered(world, pos)
}

/// `Cushion.canBePlacedAt`.
pub fn can_be_placed_at(world: &World, pos: DVec3) -> bool {
    survives(world, pos) && !buried(world, pos)
}

/// `Cushion.tickAtCheckInterval`: fire anywhere in its (slightly shrunk)
/// box (`destroyIfInFire`), or lava in its cell, whose `entityInside`
/// burns it (`Entity.lavaHurt`); any damage breaks a cushion.
pub fn in_fire(world: &World, pos: DVec3) -> bool {
    let (min, max) = SHAPE.aabb(pos);
    let (lo, hi) = ((min + DVec3::splat(1e-7)).floor().as_ivec3(), (max - DVec3::splat(1e-7)).floor().as_ivec3());
    world.get_block(pos.floor().as_ivec3()).is_some_and(Block::is_lava)
        || (lo.y..=hi.y).any(|y| {
            (lo.z..=hi.z)
                .any(|z| (lo.x..=hi.x).any(|x| world.get_block(IVec3::new(x, y, z)).is_some_and(Block::is_fire)))
        })
}

/// Where `CushionItem.useOn` puts a cushion for a click at `point` on the
/// top face of `clicked`: centred in the cell the item would go into, at
/// the click's height. `None` for any other face.
pub fn placement(world: &World, clicked: IVec3, normal: IVec3, point: DVec3) -> Option<DVec3> {
    if normal != IVec3::Y {
        return None;
    }
    let replaceable = world.get_block(clicked).is_some_and(Block::is_replaceable);
    let cell = if replaceable { clicked } else { clicked + normal };
    let pos = DVec3::new(cell.x as f64 + 0.5, point.y, cell.z as f64 + 0.5);
    can_be_placed_at(world, pos).then_some(pos)
}

impl crate::entity::Entities {
    /// Places a cushion of `colour` at `pos`, turned to `yaw`, unless
    /// another cushion is in the way (`CushionItem.useOn`).
    pub fn place_cushion(&mut self, colour: u8, pos: DVec3, yaw: f32) -> Option<u32> {
        use crate::entity::minecart::CartKind;
        let bx = SHAPE.aabb(pos);
        if self.minecarts.iter().any(|c| c.kind.cushion().is_some() && intersects(c.aabb(), bx)) {
            return None;
        }
        let id = self.spawn_cart(CartKind::Cushion { colour }, pos);
        if let Some(c) = self.cart_mut(id) {
            c.yaw = yaw;
        }
        Some(id)
    }
}
