//! v1.0 Track 7 (FOO-04a): boats, chest boats, rafts and chest rafts tick
//! by tick as Java 26.3's `AbstractBoat` does (read from the 26.3 jar):
//!
//! - `getStatus`: under water (a water surface above the box's top, flowing
//!   or still), in water (the bottom below a surface), on land (the mean
//!   `Block.getFriction` of the blocks whose collision meets a thin slab
//!   under the box, lily pads left out) or in the air.
//! - `floatBoat`: gravity 0.04 and the status's friction (0.9 in water,
//!   0.45 under still water, 0.9 under flowing water with a 7.0E-4 sink,
//!   the land friction, 0.9 in the air; Java's initial 0.05 is never read),
//!   buoyancy toward the water level, and the snap onto the surface the
//!   tick it lands in water from the air.
//! - `controlBoat`: the rider's keys. Left and right turn by one degree a
//!   tick (kept by `deltaRotation`, which decays with the friction); up
//!   adds 0.04 forward, down takes 0.005, and turning alone adds 0.005.
//! - `Entity.baseTick`'s water current (the normalised flow times 0.014),
//!   `Entity.move` with no step height and Java's axis order, slime and bed
//!   bounces, the block speed factor, and `checkFallDamage`, which only
//!   resets the fall on landing.
//! - Bubble columns through `applyEffectsFromBlocks`, which a boat runs
//!   twice a tick: a column cell under air marks the boat above it
//!   (`onAboveBubbleColumn`), and after 60 ticks there it is thrown up (0.6)
//!   or dragged down (-0.7) and emptied; a cell under water pushes it as it
//!   pushes any entity (`onInsideBubbleColumn`).
//! - Sixty ticks under water (`outOfControlTicks`) eject the passengers.
//! - Passengers sit `rideHeight` up (a third of the height; 0.8888889 of
//!   it on a raft) less their own vehicle attachment (a player's 0.6), 0.2
//!   ahead and 0.6 behind when two ride (animals 0.2 further forward), a
//!   lone passenger at the middle (0.15 forward in a chest boat).
//! - Paddles turn π/8 a tick while rowing and splash or scrape at π/4.

use glam::{DVec3, IVec3};

use super::Minecart;
use crate::knockback::{mth_cos, mth_sin};
use crate::physics::{self, BlockSource, Collider};
use crate::player::fluid::{self, FluidCell};
use crate::world::block::{Block, Fluid};

/// `AbstractBoat.getDefaultGravity`.
pub const GRAVITY: f64 = 0.04;
/// `TIME_TO_EJECT`, and the bubble column's delay (`BUBBLE_TIME`).
pub const TIME_TO_EJECT: f32 = 60.0;
pub const BUBBLE_TIME: i32 = 60;
/// `PADDLE_SPEED` and `PADDLE_SOUND_TIME`.
pub const PADDLE_SPEED: f32 = std::f32::consts::PI / 8.0;
pub const PADDLE_SOUND_TIME: f32 = std::f32::consts::PI / 4.0;
/// `controlBoat`'s pushes.
pub const FORWARD: f32 = 0.04;
pub const BACKWARD: f32 = 0.005;
pub const TURNING: f32 = 0.005;
/// `EntityTypes` boat dimensions: 1.375 wide, 0.5625 tall.
pub const WIDTH: f64 = 1.375;
pub const HEIGHT: f64 = 0.5625;
/// `Avatar.DEFAULT_VEHICLE_ATTACHMENT`: a player's feet sit this far below
/// the vehicle's attachment point.
pub const PLAYER_ATTACHMENT: f64 = 0.6;

/// `AbstractBoat.Status`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    InWater,
    UnderWater,
    UnderFlowingWater,
    OnLand,
    #[default]
    InAir,
}

/// A rider's movement keys (`AbstractBoat.setInput`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Keys {
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
}

impl Keys {
    /// The keys behind a movement input (`ClientInput.keyPresses`): forward
    /// and strafe, each -1..=1.
    pub fn from_move(forward: f64, right: f64) -> Self {
        Self { left: right < 0.0, right: right > 0.0, up: forward > 0.0, down: forward < 0.0 }
    }
}

/// `AbstractBoat`'s per-tick state. Java saves none of it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct State {
    pub status: Status,
    pub old_status: Status,
    water_level: f64,
    land_friction: f32,
    /// Degrees a tick.
    pub delta_rotation: f32,
    last_yd: f64,
    pub out_of_control: f32,
    pub bubble_time: i32,
    above_bubble: bool,
    bubble_down: bool,
    /// Left and right paddle angles, and whether each rows.
    pub paddles: [f32; 2],
    pub rowing: [bool; 2],
    /// Last tick's collisions.
    pub horizontal: bool,
    /// A paddle stroke this tick: in water (true) or on land (false).
    pub stroke: Option<bool>,
    /// The bubble column threw the boat up or dragged it down this tick.
    pub bubble_burst: Option<bool>,
}

/// Java's `FluidState.getHeight` for the water in `p`, if any.
fn water_at<W: BlockSource + ?Sized>(world: &W, p: IVec3) -> Option<(FluidCell, f32)> {
    let cell = world.block(p).and_then(fluid::fluid_at).filter(|c| c.fluid == Fluid::Water)?;
    Some((cell, fluid::height(world, p, cell)))
}

impl Minecart {
    /// Java yaw (degrees, `(-sin, cos)` forward) of this boat.
    pub fn java_yaw(&self) -> f32 {
        self.yaw.to_degrees() - 90.0
    }

    fn set_java_yaw(&mut self, yaw: f32) {
        self.yaw = (yaw + 90.0).to_radians();
    }

    /// `AbstractBoat.rideHeight`: a raft seats higher.
    pub fn ride_height(&self) -> f64 {
        let raft = self.kind.boat().is_some_and(|(wood, _)| wood == 9);
        if raft { HEIGHT * 0.8888889f32 as f64 } else { HEIGHT / 3.0 }
    }

    /// `getPassengerAttachmentPoint` for passenger `index` of `count`
    /// (`animal` for an `Animal`), relative to the boat: up `rideHeight`,
    /// forward by the seat's offset.
    pub fn boat_attachment(&self, index: usize, count: usize, animal: bool) -> DVec3 {
        let chest = self.kind.boat().is_some_and(|(_, chest)| chest);
        let mut offset = if chest { 0.15f32 } else { 0.0 };
        if count > 1 {
            offset = if index == 0 { 0.2 } else { -0.6 };
            if animal {
                offset += 0.2;
            }
        }
        // `new Vec3(0, rideHeight, offset).yRot(-yRot)`: along the boat's forward.
        let a = -self.java_yaw().to_radians();
        let (s, c) = (mth_sin(a as f64) as f64, mth_cos(a as f64) as f64);
        DVec3::new(offset as f64 * s, self.ride_height(), offset as f64 * c)
    }

    fn get_status<W: BlockSource + ?Sized>(&mut self, world: &W) -> Status {
        if let Some(under) = self.underwater_status(world) {
            self.boat.water_level = self.pos.y + HEIGHT;
            return under;
        }
        if self.check_in_water(world) {
            return Status::InWater;
        }
        let friction = self.ground_friction(world);
        if friction > 0.0 {
            self.boat.land_friction = friction;
            Status::OnLand
        } else {
            Status::InAir
        }
    }

    /// `getWaterLevelAbove`: the top of the water column the boat is in.
    fn water_level_above<W: BlockSource + ?Sized>(&self, world: &W) -> f32 {
        let (min, max) = self.aabb();
        let (x0, x1) = (min.x.floor() as i32, max.x.ceil() as i32);
        let (y0, y1) = (max.y.floor() as i32, (max.y - self.boat.last_yd).ceil() as i32);
        let (z0, z1) = (min.z.floor() as i32, max.z.ceil() as i32);
        'rows: for y in y0..y1 {
            let mut height = 0.0f32;
            for x in x0..x1 {
                for z in z0..z1 {
                    if let Some((_, h)) = water_at(world, IVec3::new(x, y, z)) {
                        height = height.max(h);
                    }
                    if height >= 1.0 {
                        continue 'rows;
                    }
                }
            }
            if height < 1.0 {
                return y as f32 + height;
            }
        }
        y1 as f32 + 1.0
    }

    /// `getGroundFriction`: the mean friction of the blocks under the boat.
    fn ground_friction<W: BlockSource + ?Sized>(&self, world: &W) -> f32 {
        let (min, max) = self.aabb();
        let (lo, hi) = (DVec3::new(min.x, min.y - 0.001, min.z), DVec3::new(max.x, min.y, max.z));
        let mut colliders: Vec<Collider> = Vec::new();
        physics::colliders(world, lo, hi, &mut colliders);
        let (mut friction, mut count) = (0.0f32, 0u32);
        let mut seen: Vec<IVec3> = Vec::new();
        for c in &colliders {
            if seen.contains(&c.cell) || !(c.min.cmplt(hi).all() && c.max.cmpgt(lo).all()) {
                continue;
            }
            let block = world.block(c.cell);
            if block == Some(crate::world::overworld_blocks::LILY_PAD) {
                continue;
            }
            seen.push(c.cell);
            friction += crate::player::friction(block);
            count += 1;
        }
        // Java divides by zero here (NaN), which no status test passes.
        if count == 0 { 0.0 } else { friction / count as f32 }
    }

    /// `checkInWater`: whether the bottom is below a water surface, keeping
    /// the highest surface as the water level.
    fn check_in_water<W: BlockSource + ?Sized>(&mut self, world: &W) -> bool {
        let (min, max) = self.aabb();
        let (x0, x1) = (min.x.floor() as i32, max.x.ceil() as i32);
        let (y0, y1) = (min.y.floor() as i32, (min.y + 0.001).ceil() as i32);
        let (z0, z1) = (min.z.floor() as i32, max.z.ceil() as i32);
        let mut in_water = false;
        self.boat.water_level = -f64::MAX;
        for x in x0..x1 {
            for y in y0..y1 {
                for z in z0..z1 {
                    if let Some((_, h)) = water_at(world, IVec3::new(x, y, z)) {
                        let height = y as f32 + h;
                        self.boat.water_level = self.boat.water_level.max(height as f64);
                        in_water |= min.y < height as f64;
                    }
                }
            }
        }
        in_water
    }

    /// `isUnderwater`: water above the box's top, flowing or still.
    fn underwater_status<W: BlockSource + ?Sized>(&self, world: &W) -> Option<Status> {
        let (min, max) = self.aabb();
        let top = max.y + 0.001;
        let (x0, x1) = (min.x.floor() as i32, max.x.ceil() as i32);
        let (y0, y1) = (max.y.floor() as i32, top.ceil() as i32);
        let (z0, z1) = (min.z.floor() as i32, max.z.ceil() as i32);
        let mut under = false;
        for x in x0..x1 {
            for y in y0..y1 {
                for z in z0..z1 {
                    if let Some((cell, h)) = water_at(world, IVec3::new(x, y, z))
                        && top < y as f64 + h as f64
                    {
                        if !cell.source {
                            return Some(Status::UnderFlowingWater);
                        }
                        under = true;
                    }
                }
            }
        }
        under.then_some(Status::UnderWater)
    }

    /// `floatBoat`.
    fn float_boat<W: BlockSource + ?Sized>(&mut self, world: &W, player: bool) {
        let mut vspeed = -GRAVITY;
        let mut buoyancy = 0.0;
        let b = self.boat;
        if b.old_status == Status::InAir && b.status != Status::InAir && b.status != Status::OnLand {
            // Landed in water: snap onto the surface if there's room.
            self.boat.water_level = self.pos.y + HEIGHT;
            let target = self.water_level_above(world) as f64 - HEIGHT + 0.101;
            let (min, max) = self.shape().aabb(DVec3::new(self.pos.x, target, self.pos.z));
            if physics::no_collision_context(world, min, max, None) {
                self.pos.y = target;
                self.vel.y = 0.0;
                self.boat.last_yd = 0.0;
            }
            self.boat.status = Status::InWater;
            return;
        }
        let inv_friction: f32 = match b.status {
            Status::InWater => {
                buoyancy = (b.water_level - self.pos.y) / HEIGHT;
                0.9
            }
            Status::UnderFlowingWater => {
                vspeed = -7.0E-4;
                0.9
            }
            Status::UnderWater => {
                buoyancy = 0.01f32 as f64;
                0.45
            }
            Status::InAir => 0.9,
            Status::OnLand => {
                if player {
                    self.boat.land_friction /= 2.0;
                }
                b.land_friction
            }
        };
        let f = inv_friction as f64;
        self.vel = DVec3::new(self.vel.x * f, self.vel.y + vspeed, self.vel.z * f);
        self.boat.delta_rotation *= inv_friction;
        if buoyancy > 0.0 {
            self.vel.y = (self.vel.y + buoyancy * (GRAVITY / 0.65)) * 0.75;
        }
    }

    /// `controlBoat` with the controlling player's keys.
    fn control_boat(&mut self, keys: Keys) {
        let mut acceleration = 0.0f32;
        if keys.left {
            self.boat.delta_rotation -= 1.0;
        }
        if keys.right {
            self.boat.delta_rotation += 1.0;
        }
        if keys.right != keys.left && !keys.up && !keys.down {
            acceleration += TURNING;
        }
        let yaw = self.java_yaw() + self.boat.delta_rotation;
        self.set_java_yaw(yaw);
        if keys.up {
            acceleration += FORWARD;
        }
        if keys.down {
            acceleration -= BACKWARD;
        }
        let r = yaw * (std::f32::consts::PI / 180.0);
        self.vel.x += (mth_sin(-r as f64) * acceleration) as f64;
        self.vel.z += (mth_cos(r as f64) * acceleration) as f64;
        self.boat.rowing = [keys.right && !keys.left || keys.up, keys.left && !keys.right || keys.up];
    }

    /// `Entity.move(SELF, deltaMovement)` for a boat: no step height, Java's
    /// axis order, then bounces, the speed factor and `checkFallDamage`.
    fn move_boat<W: BlockSource + ?Sized>(&mut self, world: &W) {
        let delta = self.vel;
        let (min, max) = self.aabb();
        let mut colliders: Vec<Collider> = Vec::new();
        physics::colliders(world, min.min(min + delta), max.max(max + delta), &mut colliders);
        let moved = physics::collide_with(&colliders, min, max, delta);
        self.pos += moved;
        let (x, z) = (moved.x != delta.x, moved.z != delta.z);
        let vertical = moved.y != delta.y;
        self.boat.horizontal = x || z;
        self.on_ground = vertical && delta.y < 0.0;
        if x {
            self.vel.x = 0.0;
        }
        if z {
            self.vel.z = 0.0;
        }
        // `getOnPosLegacy`: the block 0.2 under the feet.
        let below = world.block((self.pos - DVec3::Y * 0.2).floor().as_ivec3());
        if vertical {
            // `Block.updateEntityMovementAfterFallOn`: slime and beds bounce
            // a falling entity that isn't living at 0.8 of their restitution.
            let bounce = crate::player::bounciness(below) * 0.8;
            self.vel.y = if self.on_ground && delta.y < 0.0 && bounce > 0.0 { -delta.y * bounce } else { 0.0 };
        }
        // `checkFallDamage` (which sees the movement before the collision
        // zeroed it), then the speed factor of the block under the boat.
        self.boat.last_yd = delta.y;
        if self.on_ground {
            self.fall_distance = 0.0;
        } else if moved.y < 0.0 && !world.block(self.pos.floor().as_ivec3() - IVec3::Y).is_some_and(|b| b.is_water()) {
            self.fall_distance -= moved.y;
        }
        let here = world.block(self.pos.floor().as_ivec3());
        let factor = if here.is_some_and(|b| b.is_water()) || here.is_some_and(crate::world::bubble::is_column) {
            1.0
        } else {
            let at = here.filter(|_| crate::player::travel::speed_factor(here) != 1.0);
            let under = world.block((self.pos - DVec3::Y * 0.500001).floor().as_ivec3());
            crate::player::travel::speed_factor(at.or(under))
        } as f64;
        self.vel.x *= factor;
        self.vel.z *= factor;
    }

    /// `applyEffectsFromBlocks`' bubble columns for the cells the boat is in.
    fn bubble_effects<W: BlockSource + ?Sized>(&mut self, world: &W) {
        let (min, max) = self.aabb();
        let eps = DVec3::splat(1.0e-5);
        let (lo, hi) = ((min + eps).floor().as_ivec3(), (max - eps).floor().as_ivec3());
        for y in lo.y..=hi.y {
            for z in lo.z..=hi.z {
                for x in lo.x..=hi.x {
                    let p = IVec3::new(x, y, z);
                    let Some(b) = world.block(p).filter(|&b| crate::world::bubble::is_column(b)) else { continue };
                    let down = crate::world::bubble::drags(b);
                    if world.block(p + IVec3::Y) == Some(Block::AIR) {
                        // `AbstractBoat.onAboveBubbleColumn`.
                        self.boat.above_bubble = true;
                        self.boat.bubble_down = down;
                        if self.boat.bubble_time == 0 {
                            self.boat.bubble_time = BUBBLE_TIME;
                        }
                    } else {
                        // `Entity.handleOnInsideBubbleColumn`.
                        self.vel.y = if down { (self.vel.y - 0.03).max(-0.3) } else { (self.vel.y + 0.06).min(0.7) };
                        self.fall_distance = 0.0;
                    }
                }
            }
        }
    }

    /// `tickBubbleColumn` on the server.
    fn tick_bubble_column(&mut self) {
        if !self.boat.above_bubble {
            self.boat.bubble_time = 0;
        }
        if self.boat.bubble_time > 0 {
            self.boat.bubble_time -= 1;
            let diff = BUBBLE_TIME - self.boat.bubble_time - 1;
            if diff > 0 && self.boat.bubble_time == 0 {
                if self.boat.bubble_down {
                    self.rider = None;
                    self.second_rider = None;
                    self.eject = true;
                    self.vel.y -= 0.7;
                } else {
                    self.vel.y = 0.6;
                }
                self.boat.bubble_burst = Some(self.boat.bubble_down);
            }
            self.boat.above_bubble = false;
        }
    }

    /// One `AbstractBoat.tick`. `keys` are the controlling player's (Java's
    /// controller is the first passenger when it is a player).
    pub(super) fn step_boat<W: BlockSource + ?Sized>(&mut self, world: &W, keys: Option<Keys>) {
        self.boat.stroke = None;
        self.boat.bubble_burst = None;
        self.boat.old_status = self.boat.status;
        self.boat.status = self.get_status(world);
        let under = matches!(self.boat.status, Status::UnderWater | Status::UnderFlowingWater);
        self.boat.out_of_control = if under { self.boat.out_of_control + 1.0 } else { 0.0 };
        self.underwater_ticks = self.boat.out_of_control as u16;
        if self.boat.out_of_control >= TIME_TO_EJECT {
            self.rider = None;
            self.second_rider = None;
            self.eject = true;
        }
        // `Entity.baseTick`: water resets the fall, and currents push.
        let (min, max) = self.aabb();
        let contact = fluid::contact(world, min, max, self.pos + DVec3::Y * HEIGHT, true);
        if contact.in_water() {
            self.fall_distance = 0.0;
        }
        self.vel += contact.entity_current_impulse(self.vel, false);
        let keys = keys.filter(|_| self.rider.is_some());
        if keys.is_none() {
            self.boat.rowing = [false, false];
        }
        self.float_boat(world, keys.is_some());
        if let Some(keys) = keys {
            self.control_boat(keys);
        }
        self.move_boat(world);
        // `applyEffectsFromBlocks`, twice.
        self.bubble_effects(world);
        self.bubble_effects(world);
        self.tick_bubble_column();
        // The paddles, and a stroke sound as one passes π/4.
        let stroke = match self.boat.status {
            Status::InWater | Status::UnderWater | Status::UnderFlowingWater => Some(true),
            Status::OnLand => Some(false),
            Status::InAir => None,
        };
        for i in 0..2 {
            if self.boat.rowing[i] {
                let p = self.boat.paddles[i];
                if p % std::f32::consts::TAU <= PADDLE_SOUND_TIME
                    && (p + PADDLE_SPEED) % std::f32::consts::TAU >= PADDLE_SOUND_TIME
                    && stroke.is_some()
                {
                    self.boat.stroke = stroke;
                }
                self.boat.paddles[i] = p + PADDLE_SPEED;
            } else {
                self.boat.paddles[i] = 0.0;
            }
        }
        self.paddle = self.boat.paddles[0].max(self.boat.paddles[1]);
    }

    /// `positionRider` / `clampRotation` for a player: the boat's turn
    /// carries the rider's yaw (radians, the game's convention) round with
    /// it, which then stays within 105° of the boat's heading.
    pub fn turn_boat_rider(&self, yaw: f32) -> f32 {
        let java = yaw.to_degrees() - 90.0 + self.boat.delta_rotation;
        let body = self.java_yaw();
        let delta = wrap_degrees(java - body);
        let clamped = delta.clamp(-105.0, 105.0);
        (java + clamped - delta + 90.0).to_radians()
    }
}

/// `Mth.wrapDegrees`.
fn wrap_degrees(a: f32) -> f32 {
    let w = a % 360.0;
    if w >= 180.0 {
        w - 360.0
    } else if w < -180.0 {
        w + 360.0
    } else {
        w
    }
}

#[cfg(test)]
mod tests {
    //! Tick-by-tick checks against `AbstractBoat`'s equations (Java 26.3):
    //! the constants are the jar's, and each expected value is worked out
    //! from Java's formulas, not from this implementation.
    use super::*;
    use crate::entity::PlayerId;
    use crate::entity::minecart::CartKind;
    use rustc_hash::FxHashMap;

    /// Horizontal layers: `block` from `lo` up to (not including) `hi`.
    struct Layers {
        layers: Vec<(i32, i32, Block)>,
        cells: FxHashMap<IVec3, Block>,
    }

    impl Layers {
        fn new(layers: &[(i32, i32, Block)]) -> Self {
            Self { layers: layers.to_vec(), cells: FxHashMap::default() }
        }
    }

    impl BlockSource for Layers {
        fn block(&self, p: IVec3) -> Option<Block> {
            if let Some(&b) = self.cells.get(&p) {
                return Some(b);
            }
            Some(self.layers.iter().find(|&&(lo, hi, _)| (lo..hi).contains(&p.y)).map_or(Block::AIR, |l| l.2))
        }
    }

    /// Still water up to y = 63 (the top source's surface at 62 + 8/9).
    fn sea() -> Layers {
        Layers::new(&[(i32::MIN, 40, Block::STONE), (40, 63, Block::WATER)])
    }

    fn surface() -> f64 {
        (62.0f32 + 8.0f32 / 9.0f32) as f64
    }

    fn boat(at: DVec3) -> Minecart {
        Minecart::new(1, CartKind::Boat { wood: 0, chest: false }, at)
    }

    fn drive(b: &mut Minecart, world: &Layers, keys: Keys, ticks: usize) {
        b.rider = Some(PlayerId::HOST);
        for _ in 0..ticks {
            b.previous_pos = b.pos;
            b.step_boat(world, Some(keys));
        }
    }

    const UP: Keys = Keys { left: false, right: false, up: true, down: false };

    #[test]
    fn statuses_follow_the_water_and_ground() {
        let world = sea();
        let mut b = boat(DVec3::new(0.5, surface() - 0.2, 0.5));
        assert_eq!(b.get_status(&world), Status::InWater);
        assert!((b.boat.water_level - surface()).abs() < 1e-9);
        b.pos.y = 50.0;
        assert_eq!(b.get_status(&world), Status::UnderWater);
        b.pos.y = 70.0;
        assert_eq!(b.get_status(&world), Status::InAir);
        let land = Layers::new(&[(i32::MIN, 64, Block::STONE)]);
        b.pos.y = 64.0;
        assert_eq!(b.get_status(&land), Status::OnLand);
        // The float mean of the blocks under it (Java sums floats too).
        assert!((b.boat.land_friction - 0.6).abs() < 1e-6);
        // Flowing water over the box: `UNDER_FLOWING_WATER`.
        let mut flowing = sea();
        for x in -2..3 {
            for z in -2..3 {
                flowing.cells.insert(IVec3::new(x, 50, z), Block(24));
            }
        }
        b.pos.y = 50.0;
        assert_eq!(b.get_status(&flowing), Status::UnderFlowingWater);
    }

    #[test]
    fn a_boat_floats_where_buoyancy_balances_gravity() {
        // `floatBoat` in water: vy = (vy - 0.04 + (level - y) / 0.5625 * 0.04 / 0.65) * 0.75,
        // still where (level - y) / 0.5625 = 0.65.
        let world = sea();
        let mut b = boat(DVec3::new(0.5, surface() - 0.1, 0.5));
        for _ in 0..400 {
            b.step_boat(&world, None);
        }
        assert_eq!(b.boat.status, Status::InWater);
        assert!((b.pos.y - (surface() - 0.65 * HEIGHT)).abs() < 1e-6, "{}", b.pos.y);
    }

    #[test]
    fn a_boat_falling_into_water_snaps_onto_the_surface() {
        let world = sea();
        let mut b = boat(DVec3::new(0.5, 63.3, 0.5));
        let mut snapped = None;
        for _ in 0..20 {
            let before = b.boat.status;
            b.step_boat(&world, None);
            if before == Status::InAir && b.boat.status == Status::InWater && snapped.is_none() {
                snapped = Some(b.pos.y);
            }
        }
        // `getWaterLevelAbove() - getBbHeight() + 0.101`, then not moved
        // that tick (its fall was zeroed). The level is the first row from
        // the box's top that isn't full: with the top still above y = 63
        // that is the empty row 63 itself (Java's quirk), else the top
        // source's 62 + 8/9.
        let snapped = snapped.unwrap();
        let targets = [63.0 - HEIGHT + 0.101, surface() - HEIGHT + 0.101];
        assert!(targets.iter().any(|t| (snapped - t).abs() < 1e-9), "{snapped} vs {targets:?}");
    }

    #[test]
    fn paddling_reaches_javas_top_speeds() {
        // v' = v * friction + 0.04 a tick, so the top speed is 0.04 / (1 - friction).
        let cases = [
            (sea(), 0.9f32, 400),
            (Layers::new(&[(i32::MIN, 64, Block::STONE)]), 0.6, 100),
            (Layers::new(&[(i32::MIN, 64, Block::ICE)]), 0.98, 1500),
            (Layers::new(&[(i32::MIN, 64, crate::world::overworld_blocks::BLUE_ICE)]), 0.989, 3000),
        ];
        for (world, friction, ticks) in cases {
            let y = if friction == 0.9 { surface() - 0.65 * HEIGHT } else { 64.0 };
            let mut b = boat(DVec3::new(0.5, y, 0.5));
            // Java yaw 0: forward is +z.
            b.set_java_yaw(0.0);
            drive(&mut b, &world, UP, ticks);
            let top = FORWARD as f64 / (1.0 - friction as f64);
            let speed = b.vel.with_y(0.0).length();
            assert!((speed - top).abs() < 1e-4, "friction {friction}: {speed} vs {top}");
            assert!(b.vel.z > 0.0 && b.vel.x.abs() < 1e-9);
        }
    }

    #[test]
    fn turning_keys_build_a_turn_that_friction_decays() {
        let land = Layers::new(&[(i32::MIN, 64, Block::STONE)]);
        let mut b = boat(DVec3::new(0.5, 64.0, 0.5));
        b.set_java_yaw(0.0);
        let left = Keys { left: true, ..Default::default() };
        // Tick 1: deltaRotation = 0 * 0.6 - 1; tick 2: -1 * 0.6 - 1.
        drive(&mut b, &land, left, 1);
        assert!((b.java_yaw() + 1.0).abs() < 1e-4);
        drive(&mut b, &land, left, 1);
        assert!((b.boat.delta_rotation + 1.6).abs() < 1e-6);
        assert!((b.java_yaw() + 2.6).abs() < 1e-4);
        // Released: -1.6 * 0.6 more.
        drive(&mut b, &land, Keys::default(), 1);
        assert!((b.java_yaw() + 3.56).abs() < 1e-4);
        // Turning alone also pushes 0.005 forward; both paddles idle once released.
        assert_eq!(b.boat.rowing, [false, false]);
        let mut c = boat(DVec3::new(0.5, 64.0, 0.5));
        c.set_java_yaw(0.0);
        drive(&mut c, &land, Keys { right: true, ..Default::default() }, 1);
        let r = c.java_yaw().to_radians();
        assert!((c.vel.with_y(0.0).length() - TURNING as f64).abs() < 1e-6, "{:?} at {r}", c.vel);
        assert_eq!(c.boat.rowing, [true, false], "the right turn rows the left paddle");
    }

    #[test]
    fn sixty_ticks_under_water_eject_the_riders() {
        let world = sea();
        let mut b = boat(DVec3::new(0.5, 50.0, 0.5));
        b.rider = Some(PlayerId::HOST);
        for tick in 1..=60 {
            b.step_boat(&world, None);
            // Keep it under: the buoyancy of 0.01 lifts it slowly.
            b.pos.y = 50.0;
            assert_eq!(b.rider.is_none(), tick >= 60, "tick {tick}");
        }
        assert!(b.eject);
    }

    #[test]
    fn bubble_columns_throw_up_or_drag_down_after_sixty_ticks() {
        use crate::world::bubble::{DOWN, UP as RISE};
        for (column, down) in [(RISE, false), (DOWN, true)] {
            let mut world = sea();
            for y in 40..63 {
                world.cells.insert(IVec3::new(0, y, 0), column);
            }
            let mut b = boat(DVec3::new(0.5, surface() - 0.65 * HEIGHT, 0.5));
            b.rider = Some(PlayerId::HOST);
            let mut burst = None;
            for tick in 1..=60 {
                b.step_boat(&world, None);
                if let Some(d) = b.boat.bubble_burst {
                    burst = Some((tick, d, b.vel.y));
                }
            }
            let (tick, d, vy) = burst.expect("a burst");
            assert_eq!((tick, d), (60, down));
            if down {
                assert!(b.rider.is_none() && b.eject, "dragged down and emptied");
                assert!(vy < -0.6);
            } else {
                assert_eq!(vy, 0.6);
                assert!(b.rider.is_some());
            }
        }
    }

    #[test]
    fn water_currents_carry_a_boat_downstream() {
        // A flowing (level 1) water layer running toward +x.
        let mut world = Layers::new(&[(i32::MIN, 60, Block::STONE)]);
        for x in -4..6 {
            for z in -3..4 {
                world.cells.insert(IVec3::new(x, 60, z), Block(24 + (x + 4).clamp(0, 6) as u16));
            }
        }
        let mut b = boat(DVec3::new(0.5, 60.4, 0.5));
        for _ in 0..10 {
            b.step_boat(&world, None);
        }
        assert!(b.pos.x > 0.5 && b.vel.x > 0.0, "{:?} {:?}", b.pos, b.vel);
    }

    #[test]
    fn passengers_sit_at_javas_attachment_points() {
        let mut b = boat(DVec3::new(0.0, 64.0, 0.0));
        b.set_java_yaw(0.0);
        // One player: the middle, `height / 3` up less the player's 0.6.
        b.passengers = 1;
        let seat = b.passenger_seat(0);
        assert!((seat - DVec3::new(0.0, 64.0 + HEIGHT / 3.0 - 0.6, 0.0)).length() < 1e-6, "{seat}");
        // Two: 0.2 ahead and 0.6 behind (Java yaw 0 faces +z).
        b.passengers = 2;
        assert!((b.passenger_seat(0).z - 0.2).abs() < 1e-6);
        assert!((b.passenger_seat(1).z + 0.6).abs() < 1e-6);
        // A chest boat's lone seat is 0.15 forward; a raft seats higher.
        let mut c = Minecart::new(2, CartKind::Boat { wood: 0, chest: true }, DVec3::ZERO);
        c.set_java_yaw(0.0);
        c.passengers = 1;
        assert!((c.boat_attachment(0, 1, false).z - 0.15).abs() < 1e-6);
        let raft = Minecart::new(3, CartKind::Boat { wood: 9, chest: false }, DVec3::ZERO);
        assert!((raft.ride_height() - HEIGHT * 0.8888889f32 as f64).abs() < 1e-12);
    }

    #[test]
    fn a_riders_heading_turns_with_the_boat_within_105_degrees() {
        let mut b = boat(DVec3::ZERO);
        b.set_java_yaw(0.0);
        b.boat.delta_rotation = 5.0;
        // Looking straight ahead: carried round by 5°.
        let ahead = (0.0f32 + 90.0).to_radians();
        let turned = b.turn_boat_rider(ahead).to_degrees() - 90.0;
        assert!((turned - 5.0).abs() < 1e-3, "{turned}");
        // Looking 150° to the side: held at 105° from the boat.
        b.boat.delta_rotation = 0.0;
        let side = (150.0f32 + 90.0).to_radians();
        let held = b.turn_boat_rider(side).to_degrees() - 90.0;
        assert!((held - 105.0).abs() < 1e-3, "{held}");
    }

    #[test]
    fn hits_add_ten_times_their_damage_and_break_past_forty() {
        let mut b = boat(DVec3::ZERO);
        // A fist (1): the fifth hit breaks it.
        for hit in 1..=5 {
            assert_eq!(b.hurt_by(false, 1.0).is_some(), hit == 5, "hit {hit}");
        }
        // A diamond sword (7): at once.
        let mut b = boat(DVec3::ZERO);
        assert_eq!(b.hurt_by(false, 7.0), Some(CartKind::Boat { wood: 0, chest: false }.item()));
    }
}
