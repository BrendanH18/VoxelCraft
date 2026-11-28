//! Java 26.3 `PistonMovingBlockEntity.moveCollidedEntities` and
//! `moveStuckEntities`, with `Entity.move(MoverType.PISTON)`.
//!
//! The moving-block tick records what each block sweeps this tick
//! ([`PistonPush`]); every entity collection then runs [`push_body`] on its
//! bodies after the world tick. Java runs the same steps per block entity
//! over the entities it finds, after entities moved; the per-entity order
//! of pushes is the block-entity order in both.
use glam::{DVec3, IVec3};

use super::super::{World, block::Block, stone_blocks::SLIME_BLOCK};
use super::moving::Moving;
use crate::physics::{self, BlockSource, Shape};

/// `PUSH_OFFSET`: how far past a moving shape a pushed entity ends up.
pub const PUSH_OFFSET: f64 = 0.01;
/// `PistonMovingBlockEntity.TICK_MOVEMENT` and `Entity.applyPistonMovementRestriction`:
/// piston movement per axis per game tick.
pub const TICK_MOVEMENT: f64 = 0.51;
/// `moveStuckEntities`: honey carries entities whose feet are below this
/// height above the moving cell.
pub const STICKY_TOP: f64 = 1.500_001_000_000_000_1;

/// What one moving block did to entities this tick.
#[derive(Clone, Debug, PartialEq)]
pub struct PistonPush {
    pub cell: IVec3,
    /// `getMovementDirection`.
    pub movement: IVec3,
    /// `newProgress - progress`.
    pub delta: f64,
    /// The pushing shape's bounds at the old progress, or for a stuck
    /// (honey) record the region whose entities ride along.
    pub min: DVec3,
    pub max: DVec3,
    boxes: std::ops::Range<usize>,
    /// Slime: entities in the swept area take the movement as velocity.
    pub bounce: bool,
    /// A retracting base: entities in its cell slide out its front
    /// (`fixEntityWithinPistonBase`).
    pub fix_base: bool,
    /// A `moveStuckEntities` record.
    pub stuck: bool,
}

/// An entity a piston can move. Velocities are in the entity's own units;
/// `vel_per_tick` converts one block per tick into them (20 for blocks per
/// second).
pub struct PushBody<'a> {
    /// Feet (bottom centre).
    pub pos: &'a mut DVec3,
    pub vel: &'a mut DVec3,
    pub vel_per_tick: f64,
    pub shape: Shape,
    /// `onGround`; `None` derives it from collision under the feet.
    pub on_ground: Option<bool>,
    /// `noPhysics` (spectators): moved without collision.
    pub noclip: bool,
}

/// The world's blocks as Java's `NOCLIP` sees them while a piston pushes:
/// blocks moving the same way don't collide.
struct NoClip<'a> {
    world: &'a World,
    dir: IVec3,
}

impl BlockSource for NoClip<'_> {
    fn block(&self, p: IVec3) -> Option<Block> {
        self.world.get_block(p)
    }

    fn moving_boxes(&self, min: DVec3, max: DVec3, f: &mut dyn FnMut(IVec3, DVec3, DVec3)) {
        self.world.piston_collision_boxes(min, max, Some(self.dir), f);
    }

    fn fast_lava(&self) -> bool {
        self.world.fast_lava()
    }
}

fn intersects(a: (DVec3, DVec3), b: (DVec3, DVec3)) -> bool {
    a.0.cmplt(b.1).all() && a.1.cmpgt(b.0).all()
}

/// `PistonMath.getMovementArea`: the slab a box face sweeps moving `amount`.
fn movement_area(min: DVec3, max: DVec3, dir: IVec3, amount: f64) -> (DVec3, DVec3) {
    let axis = if dir.x != 0 {
        0
    } else if dir.y != 0 {
        1
    } else {
        2
    };
    let step = f64::from(dir[axis]);
    let delta = amount * step;
    let (lo, hi) = (delta.min(0.0), delta.max(0.0));
    let face = if step > 0.0 { max[axis] } else { min[axis] };
    let (mut a, mut b) = (min, max);
    a[axis] = face + lo;
    b[axis] = face + hi;
    (a, b)
}

/// `getMovement`: how far `aabb` must travel along `dir` to leave `outside`.
fn movement(outside: (DVec3, DVec3), dir: IVec3, aabb: (DVec3, DVec3)) -> f64 {
    match (dir.x, dir.y, dir.z) {
        (1, _, _) => outside.1.x - aabb.0.x,
        (-1, _, _) => aabb.1.x - outside.0.x,
        (_, -1, _) => aabb.1.y - outside.0.y,
        (_, _, 1) => outside.1.z - aabb.0.z,
        (_, _, -1) => aabb.1.z - outside.0.z,
        _ => outside.1.y - aabb.0.y,
    }
}

impl World {
    /// The entity pushes the last moving-block tick made.
    pub fn piston_pushes(&self) -> &[PistonPush] {
        &self.pistons.pushes
    }

    /// Records `moveCollidedEntities` and `moveStuckEntities` for the block
    /// at `p` advancing to `next` progress.
    pub(super) fn record_piston_pushes(&mut self, p: IVec3, m: &Moving, next: f32) {
        let delta = f64::from(next - m.progress);
        let mv = m.movement();
        let off = p.as_dvec3() + m.direction().as_dvec3() * f64::from(m.extended_progress(m.progress));
        let start = self.pistons.push_boxes.len();
        let boxes = &mut self.pistons.push_boxes;
        physics::block_collision_boxes(m.push_state(), |a, b| boxes.push((off + a, off + b)));
        let end = boxes.len();
        if end > start {
            let (min, max) = boxes[start..end]
                .iter()
                .fold((DVec3::INFINITY, DVec3::NEG_INFINITY), |(lo, hi), &(a, b)| (lo.min(a), hi.max(b)));
            self.pistons.pushes.push(PistonPush {
                cell: p,
                movement: mv,
                delta,
                min,
                max,
                boxes: start..end,
                bounce: m.moved == SLIME_BLOCK,
                fix_base: !m.extending && m.source,
                stuck: false,
            });
        }
        if m.moved == Block::HONEY_BLOCK && mv.y == 0 {
            let mut top = 0.0f64;
            physics::block_collision_boxes(m.moved, |_, b| top = top.max(b.y));
            self.pistons.pushes.push(PistonPush {
                cell: p,
                movement: mv,
                delta,
                min: off + DVec3::new(0.0, top, 0.0),
                max: off + DVec3::new(1.0, STICKY_TOP, 1.0),
                boxes: 0..0,
                bounce: false,
                fix_base: false,
                stuck: true,
            });
        }
    }
}

/// `moveEntityByPiston`: `Entity.move(PISTON)` with `NOCLIP` set to
/// `noclip`, limited to [`TICK_MOVEMENT`] per axis per tick.
fn move_by_piston(world: &World, body: &mut PushBody, noclip: IVec3, v: DVec3, limit: &mut [f64; 3]) {
    let mut v = v;
    if v.length_squared() > 1.0e-7 {
        let axis = if v.x != 0.0 {
            0
        } else if v.y != 0.0 {
            1
        } else {
            2
        };
        let total = (v[axis] + limit[axis]).clamp(-TICK_MOVEMENT, TICK_MOVEMENT);
        let applied = total - limit[axis];
        limit[axis] = total;
        if applied.abs() <= 1.0e-5_f32 as f64 {
            return;
        }
        v = DVec3::ZERO;
        v[axis] = applied;
    }
    if body.noclip {
        *body.pos += v;
        return;
    }
    let (min, max) = body.shape.aabb(*body.pos);
    let mut colliders = Vec::new();
    physics::colliders(&NoClip { world, dir: noclip }, min.min(min + v), max.max(max + v), &mut colliders);
    *body.pos += physics::collide_with(&colliders, min, max, v);
}

fn grounded(world: &World, pos: DVec3, shape: Shape) -> bool {
    let (min, max) = shape.aabb(pos);
    !physics::no_collision(world, min - DVec3::new(0.0, 1.0e-3, 0.0), DVec3::new(max.x, min.y, max.z))
}

/// Applies every push the last moving-block tick recorded to one body.
/// Returns whether anything moved or launched it.
pub fn push_body(world: &World, mut body: PushBody) -> bool {
    let pushes = world.piston_pushes();
    if pushes.is_empty() {
        return false;
    }
    let mut limit = [0.0; 3];
    let mut acted = false;
    for push in pushes {
        let before = *body.pos;
        let entity = body.shape.aabb(*body.pos);
        let mv = push.movement.as_dvec3();
        if push.stuck {
            if !intersects((push.min, push.max), entity) {
                continue;
            }
            if !body.on_ground.unwrap_or_else(|| grounded(world, *body.pos, body.shape)) {
                continue;
            }
            // `isSupportedBy(pos)`: standing on the moving cell's top.
            let supported = (entity.0.y - push.min.y).abs() < 1.0e-3;
            let centre =
                (push.min.x..=push.max.x).contains(&body.pos.x) && (push.min.z..=push.max.z).contains(&body.pos.z);
            if supported || centre {
                move_by_piston(world, &mut body, push.movement, mv * push.delta, &mut limit);
            }
            acted |= *body.pos != before;
            continue;
        }
        let swept = movement_area(push.min, push.max, push.movement, push.delta);
        if !intersects((swept.0.min(push.min), swept.1.max(push.max)), entity) {
            continue;
        }
        if push.bounce {
            let axis = if push.movement.x != 0 {
                0
            } else if push.movement.y != 0 {
                1
            } else {
                2
            };
            body.vel[axis] = mv[axis] * body.vel_per_tick;
            acted = true;
        }
        let mut delta = 0.0f64;
        for &(a, b) in &world.pistons.push_boxes[push.boxes.clone()] {
            let moving = movement_area(a, b, push.movement, push.delta);
            if intersects(moving, entity) {
                delta = delta.max(movement(moving, push.movement, entity));
                if delta >= push.delta {
                    break;
                }
            }
        }
        if delta > 0.0 {
            let delta = delta.min(push.delta) + PUSH_OFFSET;
            move_by_piston(world, &mut body, push.movement, mv * delta, &mut limit);
            if push.fix_base {
                fix_within_base(world, &mut body, push, &mut limit);
            }
        }
        acted |= *body.pos != before;
    }
    acted
}

/// `fixEntityWithinPistonBase`: an entity left inside a retracting base's
/// cell is moved out of its front.
fn fix_within_base(world: &World, body: &mut PushBody, push: &PistonPush, limit: &mut [f64; 3]) {
    let entity = body.shape.aabb(*body.pos);
    let cell = (push.cell.as_dvec3(), push.cell.as_dvec3() + DVec3::ONE);
    if !intersects(entity, cell) {
        return;
    }
    let opposite = -push.movement;
    let inside = (entity.0.max(cell.0), entity.1.min(cell.1));
    let delta = movement(cell, opposite, entity) + PUSH_OFFSET;
    let intersected = movement(cell, opposite, inside) + PUSH_OFFSET;
    if (delta - intersected).abs() < PUSH_OFFSET {
        let delta = delta.min(push.delta) + PUSH_OFFSET;
        move_by_piston(world, body, push.movement, opposite.as_dvec3() * delta, limit);
    }
}

/// [`push_body`] for a player (host, controller or CLI agent). Riders move
/// with their vehicle instead.
pub fn push_player(world: &World, player: &mut crate::player::Player) -> bool {
    if player.vehicle.is_some() {
        return false;
    }
    let shape = player.collision_shape();
    push_body(
        world,
        PushBody {
            pos: &mut player.pos,
            vel: &mut player.vel,
            vel_per_tick: 20.0,
            shape,
            on_ground: Some(player.on_ground),
            noclip: player.noclip,
        },
    )
}
