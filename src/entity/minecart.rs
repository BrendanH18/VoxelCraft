//! Minecarts: Java's per-tick rail motion (straight, sloped and curved
//! tracks, 8 m/s cap, friction, derailing) plus chest, hopper and TNT carts.
//!
//! Velocity is stored in blocks per game tick, matching `AbstractMinecart`.
//! One [`Minecart::step`] is one 50 ms tick.

use glam::{DVec3, IVec3};

use crate::inventory::{Stack, stack_from_str, stack_to_string};
use crate::item::Item;
use crate::physics::{self, Shape};
use crate::world::World;
use crate::world::block::{Block, RailShape};
use crate::world::rails::{self, RailKind};

/// Java's rideable minecart: 0.98 wide, 0.7 tall.
pub const SHAPE: Shape = Shape::new(0.49, 0.7);
/// Blocks per tick. 0.4 × 20 = 8 m/s.
const MAX_SPEED: f64 = 0.4;
const SLOPE: f64 = 0.0078125;
const SEAT: f64 = 0.1875;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CartKind {
    Rideable,
    Chest,
    Hopper,
    Tnt,
    Boat { wood: u8, chest: bool },
}

impl CartKind {
    pub fn boat(self) -> Option<(u8, bool)> {
        if let Self::Boat { wood, chest } = self { Some((wood, chest)) } else { None }
    }
    fn code(self) -> u8 {
        match self {
            Self::Rideable => 0,
            Self::Chest => 1,
            Self::Hopper => 2,
            Self::Tnt => 3,
            Self::Boat { wood, chest } => 4 + wood + if chest { 10 } else { 0 },
        }
    }
    pub fn seats(self) -> usize {
        match self {
            Self::Rideable => 1,
            Self::Boat { chest, .. } => {
                if chest {
                    1
                } else {
                    2
                }
            }
            _ => 0,
        }
    }
    fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            0 => Self::Rideable,
            1 => Self::Chest,
            2 => Self::Hopper,
            3 => Self::Tnt,
            4..=23 => Self::Boat { wood: (v - 4) % 10, chest: v >= 14 },
            _ => return None,
        })
    }

    pub fn item(self) -> Item {
        match self {
            Self::Rideable => Item::MINECART,
            Self::Chest => Item::CHEST_MINECART,
            Self::Hopper => Item::HOPPER_MINECART,
            Self::Tnt => Item::TNT_MINECART,
            Self::Boat { wood, chest } => Item::boat(wood, chest),
        }
    }

    pub fn from_item(item: Item) -> Option<Self> {
        if (960..980).contains(&item.0) {
            return Some(Self::Boat { wood: ((item.0 - 960) % 10) as u8, chest: item.0 >= 970 });
        }
        Some(match item {
            Item::MINECART => Self::Rideable,
            Item::CHEST_MINECART => Self::Chest,
            Item::HOPPER_MINECART => Self::Hopper,
            Item::TNT_MINECART => Self::Tnt,
            _ => return None,
        })
    }

    fn slots(self) -> usize {
        match self {
            Self::Chest | Self::Boat { chest: true, .. } => 27,
            Self::Hopper => 5,
            _ => 0,
        }
    }
}

pub struct Minecart {
    pub id: u32,
    pub kind: CartKind,
    /// Bottom centre.
    pub pos: DVec3,
    pub previous_pos: DVec3,
    /// Blocks per tick.
    pub vel: DVec3,
    pub yaw: f32,
    flipped: bool,
    on_ground: bool,
    /// Player riding this cart, if any. Mobs store the cart id themselves.
    pub rider: Option<super::PlayerId>,
    pub second_rider: Option<super::PlayerId>,
    pub paddle: f32,
    underwater_ticks: u16,
    pub slots: [Option<Stack>; 27],
    /// TNT fuse in game ticks. `None` until an activator rail ignites it.
    pub fuse: Option<u16>,
    /// Hopper cart on a powered activator rail.
    pub disabled: bool,
    /// Set while the cart sat on a powered activator last tick (rising edge).
    activator: bool,
    /// Punch damage; breaks above 40 and decays by 1 each tick (Java).
    damage: f32,
    /// Ticks until the hopper cart may transfer again.
    pub cooldown: u8,
    /// Powered activator rail this tick: drop the rider.
    pub eject: bool,
    fall_distance: f64,
    blast_speed: f64,
}

impl Minecart {
    pub fn new(id: u32, kind: CartKind, pos: DVec3) -> Self {
        Self {
            id,
            kind,
            pos,
            previous_pos: pos,
            vel: DVec3::ZERO,
            yaw: 0.0,
            flipped: false,
            on_ground: false,
            rider: None,
            second_rider: None,
            paddle: 0.0,
            underwater_ticks: 0,
            slots: [None; 27],
            fuse: None,
            disabled: false,
            activator: false,
            damage: 0.0,
            cooldown: 0,
            eject: false,
            fall_distance: 0.0,
            blast_speed: 0.0,
        }
    }

    pub fn aabb(&self) -> (DVec3, DVec3) {
        self.shape().aabb(self.pos)
    }

    /// Feet of a passenger. The camera follows the rider, so this is the view.
    pub fn seat(&self) -> DVec3 {
        self.pos + DVec3::Y * SEAT
    }

    pub fn shape(&self) -> Shape {
        if self.kind.boat().is_some() { Shape::new(0.6875, 0.5625) } else { SHAPE }
    }
    pub fn seat_for(&self, player: super::PlayerId) -> Option<DVec3> {
        let index = if self.rider == Some(player) {
            0
        } else if self.second_rider == Some(player) {
            1
        } else {
            return None;
        };
        Some(self.passenger_seat(index))
    }
    pub fn passenger_seat(&self, index: usize) -> DVec3 {
        if self.kind.boat().is_some() {
            self.pos
                + DVec3::Y * -0.1
                + DVec3::new(self.yaw.cos() as f64, 0.0, self.yaw.sin() as f64) * if index == 0 { 0.2 } else { -0.6 }
        } else {
            self.seat()
        }
    }
    pub fn occupied(&self) -> bool {
        self.rider.is_some() || self.second_rider.is_some()
    }

    pub fn slot_count(&self) -> usize {
        self.kind.slots()
    }

    /// Java container comparator output for a chest or hopper cart.
    pub fn comparator_signal(&self) -> u8 {
        let n = self.slot_count();
        if n == 0 {
            return 0;
        }
        let mut fullness = 0.0f64;
        let mut nonempty = false;
        for stack in self.slots[..n].iter().flatten() {
            fullness += stack.count as f64 / stack.item.max_stack() as f64;
            nonempty = true;
        }
        ((fullness / n as f64 * 14.0).floor() as u8 + u8::from(nonempty)).min(15)
    }

    /// One punch. Creative breaks immediately. Returns the dropped item when it breaks.
    pub fn hurt(&mut self, creative: bool) -> Option<Item> {
        if creative {
            return Some(self.kind.item());
        }
        self.damage += 10.0;
        (self.damage > 40.0).then(|| self.kind.item())
    }

    pub fn step(&mut self, world: &World, passenger: DVec3, mob_passenger: bool) {
        if !world.is_loaded(self.pos.floor().as_ivec3()) {
            return;
        }
        self.damage = (self.damage - 1.0).max(0.0);
        self.eject = false;
        self.cooldown = self.cooldown.saturating_sub(1);
        if let Some(fuse) = &mut self.fuse {
            *fuse = fuse.saturating_sub(1);
        }
        if self.kind == CartKind::Tnt
            && self.fuse.is_none()
            && physics::touches_block(world, self.pos, SHAPE, |b| b.is_fire() || b.is_lava())
        {
            self.fuse = Some(20);
        }
        if self.kind.boat().is_some() {
            self.step_boat(world, passenger);
            return;
        }
        self.vel.y -= 0.04;
        let on_rail = rail_cell(world, self.pos);
        if let Some((cell, block)) = on_rail {
            self.fall_distance = 0.0;
            self.move_along(world, cell, block, passenger, mob_passenger);
            self.on_ground = false;
        } else {
            self.come_off(world);
            self.activator = false;
        }
        let dx = self.previous_pos.x - self.pos.x;
        let dz = self.previous_pos.z - self.pos.z;
        if dx * dx + dz * dz > 0.001 {
            let mut yaw = dz.atan2(dx) as f32;
            if self.flipped {
                yaw += std::f32::consts::PI;
            }
            let turn = wrap_radians(yaw - self.yaw);
            if turn < -2.967 || turn >= 2.967 {
                yaw += std::f32::consts::PI;
                self.flipped = !self.flipped;
            }
            self.yaw = yaw;
        }
    }

    fn step_boat(&mut self, world: &World, passenger: DVec3) {
        let shape = self.shape();
        let cell = self.pos.floor().as_ivec3();
        let water = physics::touches_block(world, self.pos, shape, |b| b.is_water());
        let submerged =
            world.get_block((self.pos + DVec3::Y * shape.height).floor().as_ivec3()).is_some_and(|b| b.is_water());
        let ground = world.get_block(cell - IVec3::Y).unwrap_or(Block::AIR);
        let friction = if water {
            0.9
        } else if self.on_ground {
            if ground == crate::world::overworld_blocks::BLUE_ICE {
                0.989
            } else if ground == Block::ICE || ground == crate::world::gadgets::PACKED_ICE {
                0.98
            } else {
                0.3
            }
        } else {
            0.9
        };
        self.vel.x *= friction;
        self.vel.z *= friction;
        self.vel.y -= 0.04;
        if water {
            self.fall_distance = 0.0;
            let drop = world.get_block(cell).filter(|b| b.is_water()).map_or(0.0, |b| b.fluid_drop() as f64 / 16.0);
            let surface = cell.y as f64 + 1.0 - drop;
            self.vel.y = (self.vel.y * 0.75 + (surface - self.pos.y - 0.15) * 0.1).clamp(-0.04, 0.1);
            self.underwater_ticks = if submerged { self.underwater_ticks.saturating_add(1) } else { 0 };
            if submerged {
                self.vel.y = (self.vel.y + 0.01) * 0.95;
                if self.underwater_ticks >= 60 {
                    self.rider = None;
                    self.second_rider = None;
                }
            }
        }
        let input = passenger.with_y(0.0);
        if input.length_squared() > 1e-8 {
            let direction = input.normalize();
            let target = direction.z.atan2(direction.x) as f32;
            self.yaw += wrap_radians(target - self.yaw).clamp(-0.1, 0.1);
            let forward = DVec3::new(self.yaw.cos() as f64, 0.0, self.yaw.sin() as f64);
            self.vel += forward * if water || !self.on_ground { 0.04 } else { 0.02 };
            self.paddle += std::f32::consts::PI / 8.0;
        }
        let from = self.pos.y;
        let mut velocity = self.vel * 20.0;
        let hit = physics::move_box(world, &mut self.pos, &mut velocity, self.vel, shape);
        self.vel = velocity / 20.0;
        self.on_ground = hit.on_ground;
        if !water {
            self.fall_distance += (from - self.pos.y).max(0.0);
        }
        if hit.on_ground {
            if self.fall_distance > 3.0 {
                self.damage = 100.0;
            }
            self.fall_distance = 0.0;
        }
    }

    fn move_along(&mut self, world: &World, cell: IVec3, block: Block, passenger: DVec3, mob_passenger: bool) {
        let shape = block.rail_shape().unwrap_or(RailShape::NorthSouth);
        let kind = rails::kind(block);
        let powered = kind == Some(RailKind::Powered) && rails::is_powered(block);
        let mut brake = kind == Some(RailKind::Powered) && !rails::is_powered(block);
        let activator = kind == Some(RailKind::Activator) && rails::is_powered(block);
        if self.kind == CartKind::Hopper && kind == Some(RailKind::Activator) {
            self.disabled = activator;
        }
        if activator && (self.rider.is_some() || mob_passenger) {
            self.eject = true;
        }
        if self.kind == CartKind::Tnt && activator && !self.activator && self.fuse.is_none() {
            self.fuse = Some(80);
        }
        self.activator = activator;

        let mut y = cell.y as f64;
        match shape {
            RailShape::AscendingEast => {
                self.vel.x -= SLOPE;
                y += 1.0;
            }
            RailShape::AscendingWest => {
                self.vel.x += SLOPE;
                y += 1.0;
            }
            RailShape::AscendingNorth => {
                self.vel.z += SLOPE;
                y += 1.0;
            }
            RailShape::AscendingSouth => {
                self.vel.z -= SLOPE;
                y += 1.0;
            }
            _ => {}
        }
        let (e0, e1) = shape.exits();
        let mut dx = (e1.x - e0.x) as f64;
        let mut dz = (e1.z - e0.z) as f64;
        let span = (dx * dx + dz * dz).sqrt().max(1e-6);
        if self.vel.x * dx + self.vel.z * dz < 0.0 {
            dx = -dx;
            dz = -dz;
        }
        let horiz = self.vel.x.hypot(self.vel.z).min(2.0);
        self.vel.x = horiz * dx / span;
        self.vel.z = horiz * dz / span;
        let pushing = passenger.x * passenger.x + passenger.z * passenger.z;
        if pushing > 1.0e-4 && self.vel.x * self.vel.x + self.vel.z * self.vel.z < 0.01 {
            self.vel.x += passenger.x * 0.1;
            self.vel.z += passenger.z * 0.1;
            brake = false;
        }
        if brake {
            let speed = self.vel.x.hypot(self.vel.z);
            if speed < 0.03 {
                self.vel = DVec3::ZERO;
            } else {
                self.vel.x *= 0.5;
                self.vel.z *= 0.5;
                self.vel.y = 0.0;
            }
        }
        let (x0, z0, lx, lz) = segment(cell, e0, e1);
        let t = if lx == 0.0 {
            self.pos.z - cell.z as f64
        } else if lz == 0.0 {
            self.pos.x - cell.x as f64
        } else {
            ((self.pos.x - x0) * lx + (self.pos.z - z0) * lz) * 2.0
        };
        let before = rail_point(world, self.pos);
        self.pos.x = x0 + lx * t;
        self.pos.y = y;
        self.pos.z = z0 + lz * t;
        let scale = if self.rider.is_some() || mob_passenger { 0.75 } else { 1.0 };
        let delta = DVec3::new(
            (scale * self.vel.x).clamp(-MAX_SPEED, MAX_SPEED),
            0.0,
            (scale * self.vel.z).clamp(-MAX_SPEED, MAX_SPEED),
        );
        let mut velocity = self.vel * 20.0;
        let speed = self.vel.x.hypot(self.vel.z);
        let hit = physics::move_box(world, &mut self.pos, &mut velocity, delta, SHAPE);
        if self.kind == CartKind::Tnt && hit.horizontal && speed >= 0.1 {
            self.fuse = Some(0);
            self.blast_speed = speed;
        }
        self.vel = velocity / 20.0;
        let fx = self.pos.x.floor() as i32 - cell.x;
        let fz = self.pos.z.floor() as i32 - cell.z;
        if e0.y != 0 && fx == e0.x && fz == e0.z {
            self.pos.y += e0.y as f64;
        } else if e1.y != 0 && fx == e1.x && fz == e1.z {
            self.pos.y += e1.y as f64;
        }
        let drag = if self.slot_count() > 0 {
            0.98 + (15 - self.comparator_signal()) as f64 * 0.001
        } else if self.rider.is_some() || mob_passenger {
            0.997
        } else {
            0.96
        };
        self.vel.x *= drag;
        self.vel.y = 0.0;
        self.vel.z *= drag;
        if let Some(snapped) = rail_point(world, self.pos) {
            if let Some(from) = before {
                let dh = (from.y - snapped.y) * 0.05;
                let speed = self.vel.x.hypot(self.vel.z);
                if speed > 0.0 {
                    let scale = (speed + dh) / speed;
                    self.vel.x *= scale;
                    self.vel.z *= scale;
                }
            }
            self.pos.y = snapped.y;
        }
        let nx = self.pos.x.floor() as i32;
        let nz = self.pos.z.floor() as i32;
        if nx != cell.x || nz != cell.z {
            let speed = self.vel.x.hypot(self.vel.z);
            self.vel.x = speed * (nx - cell.x) as f64;
            self.vel.z = speed * (nz - cell.z) as f64;
        }
        if powered {
            let speed = self.vel.x.hypot(self.vel.z);
            if speed > 0.01 {
                self.vel.x += self.vel.x / speed * 0.06;
                self.vel.z += self.vel.z / speed * 0.06;
            } else if matches!(shape, RailShape::EastWest | RailShape::NorthSouth) {
                let (vx, vz) = nudge(world, cell, shape);
                self.vel.x = vx;
                self.vel.z = vz;
            }
        }
    }

    fn come_off(&mut self, world: &World) {
        self.vel.x = self.vel.x.clamp(-MAX_SPEED, MAX_SPEED);
        self.vel.z = self.vel.z.clamp(-MAX_SPEED, MAX_SPEED);
        if self.on_ground {
            self.vel *= 0.5;
        }
        let mut vel = self.vel * 20.0;
        let from_y = self.pos.y;
        let speed = self.vel.x.hypot(self.vel.z);
        let hit = physics::move_box(world, &mut self.pos, &mut vel, self.vel, SHAPE);
        self.fall_distance += (from_y - self.pos.y).max(0.0);
        if self.kind == CartKind::Tnt && (hit.horizontal && speed >= 0.1 || hit.on_ground && self.fall_distance >= 3.0)
        {
            self.fuse = Some(0);
            self.blast_speed = if hit.on_ground { self.fall_distance / 10.0 } else { speed };
        }
        if hit.on_ground {
            self.fall_distance = 0.0;
        }
        self.vel = vel / 20.0;
        self.on_ground = hit.on_ground;
        if !hit.on_ground {
            self.vel *= 0.95;
        }
        if self.pos.y < -64.0 {
            self.damage = 100.0;
        }
    }

    pub fn serialize(&self) -> String {
        let slots: Vec<_> = self.slots.iter().map(|s| stack_to_string(*s)).collect();
        format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            self.id,
            self.kind.code(),
            self.pos.x,
            self.pos.y,
            self.pos.z,
            self.vel.x,
            self.vel.y,
            self.vel.z,
            self.yaw,
            u8::from(self.flipped),
            u8::from(self.disabled),
            self.fuse.map(|f| f as i32).unwrap_or(-1),
            self.rider.map(|id| id.0 as i32).unwrap_or(-1),
            slots.join("|"),
            self.second_rider.map(|id| id.0 as i64).unwrap_or(-1),
        )
    }

    pub fn deserialize(text: &str) -> Option<Self> {
        let mut parts = text.split(',');
        let id: u32 = parts.next()?.parse().ok()?;
        let kind = CartKind::from_u8(parts.next()?.parse().ok()?)?;
        let pos = DVec3::new(parts.next()?.parse().ok()?, parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
        let vel = DVec3::new(parts.next()?.parse().ok()?, parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
        if !pos.is_finite() || !vel.is_finite() {
            return None;
        }
        let yaw: f32 = parts.next()?.parse().ok()?;
        if !yaw.is_finite() {
            return None;
        }
        let flipped = parts.next()? == "1";
        let disabled = parts.next()? == "1";
        let fuse: i32 = parts.next()?.parse().ok()?;
        let rider: i32 = parts.next()?.parse().ok()?;
        let slot_text = parts.next()?;
        let second: i64 = parts.next().unwrap_or("-1").parse().ok()?;
        if !(-1..=u32::MAX as i64).contains(&second) || parts.next().is_some() {
            return None;
        }
        let mut cart = Self::new(id, kind, pos);
        cart.second_rider = (second >= 0).then_some(super::PlayerId(second as u32));
        cart.vel = vel;
        cart.yaw = yaw;
        cart.flipped = flipped;
        cart.disabled = disabled;
        if !(-1..=u16::MAX as i32).contains(&fuse) {
            return None;
        }
        cart.fuse = (fuse >= 0).then_some(fuse as u16);
        cart.rider = (rider >= 0).then_some(super::PlayerId(rider as u32));
        if slot_text.split('|').count() != 27 {
            return None;
        }
        for (i, piece) in slot_text.split('|').enumerate() {
            cart.slots[i] = stack_from_str(piece)?;
        }
        Some(cart)
    }
}

fn wrap_radians(v: f32) -> f32 {
    let mut a = v % std::f32::consts::TAU;
    if a < -std::f32::consts::PI {
        a += std::f32::consts::TAU;
    } else if a >= std::f32::consts::PI {
        a -= std::f32::consts::TAU;
    }
    a
}

fn segment(cell: IVec3, e0: IVec3, e1: IVec3) -> (f64, f64, f64, f64) {
    let x0 = cell.x as f64 + 0.5 + e0.x as f64 * 0.5;
    let z0 = cell.z as f64 + 0.5 + e0.z as f64 * 0.5;
    let x1 = cell.x as f64 + 0.5 + e1.x as f64 * 0.5;
    let z1 = cell.z as f64 + 0.5 + e1.z as f64 * 0.5;
    (x0, z0, x1 - x0, z1 - z0)
}

/// Java `getPos`: the point on the rail curve under a world position.
fn rail_point(world: &World, pos: DVec3) -> Option<DVec3> {
    let (cell, block) = rail_cell(world, pos)?;
    let shape = block.rail_shape()?;
    let (e0, e1) = shape.exits();
    let (x0, z0, lx, lz) = segment(cell, e0, e1);
    let mut y0 = cell.y as f64 + 0.0625 + e0.y as f64 * 0.5;
    let y1 = cell.y as f64 + 0.0625 + e1.y as f64 * 0.5;
    let ly = (y1 - y0) * 2.0;
    let t = if lx == 0.0 {
        pos.z - cell.z as f64
    } else if lz == 0.0 {
        pos.x - cell.x as f64
    } else {
        ((pos.x - x0) * lx + (pos.z - z0) * lz) * 2.0
    };
    let mut y = y0 + ly * t;
    if ly < 0.0 {
        y += 1.0;
    } else if ly > 0.0 {
        y += 0.5;
    }
    y0 = y;
    Some(DVec3::new(x0 + lx * t, y0, z0 + lz * t))
}

/// The rail in this cell, or one below when the cart is raised on a slope.
pub fn rail_cell(world: &World, pos: DVec3) -> Option<(IVec3, Block)> {
    let mut cell = pos.floor().as_ivec3();
    if world.get_block(cell - IVec3::Y).is_some_and(|b| b.is_rail()) {
        cell.y -= 1;
    }
    let block = world.get_block(cell)?;
    block.is_rail().then_some((cell, block))
}

fn nudge(world: &World, cell: IVec3, shape: RailShape) -> (f64, f64) {
    let solid = |d: IVec3| world.get_block(cell + d).is_some_and(|b| b.is_opaque());
    match shape {
        RailShape::EastWest if solid(IVec3::NEG_X) => (0.02, 0.0),
        RailShape::EastWest if solid(IVec3::X) => (-0.02, 0.0),
        RailShape::NorthSouth if solid(IVec3::NEG_Z) => (0.0, 0.02),
        RailShape::NorthSouth if solid(IVec3::Z) => (0.0, -0.02),
        _ => (0.0, 0.0),
    }
}

/// Limit entity use/attack reach to the first selected block's actual outline.
pub fn interaction_reach(world: &World, eye: DVec3, dir: DVec3, reach: f64) -> f64 {
    world
        .raycast(eye, dir, reach)
        .and_then(|(p, _)| {
            let (min, max) = world.outline(p);
            physics::ray_aabb(
                eye,
                dir,
                p.as_dvec3() + glam::Vec3::from_array(min).as_dvec3(),
                p.as_dvec3() + glam::Vec3::from_array(max).as_dvec3(),
            )
        })
        .unwrap_or(reach)
        .min(reach)
}

/// Push two carts apart the way `AbstractMinecart.push` does, once per pair.
pub fn push_pair(a: &mut Minecart, b: &mut Minecart) {
    let dx = b.pos.x - a.pos.x;
    let dz = b.pos.z - a.pos.z;
    let dist2 = dx * dx + dz * dz;
    if dist2 < 1.0e-4 {
        return;
    }
    let (min_a, max_a) = a.aabb();
    let (min_b, max_b) = b.aabb();
    if max_a.x + 0.2 < min_b.x || max_b.x + 0.2 < min_a.x || max_a.z + 0.2 < min_b.z || max_b.z + 0.2 < min_a.z {
        return;
    }
    if max_a.y < min_b.y || max_b.y < min_a.y {
        return;
    }
    let dist = dist2.sqrt();
    let scale = (1.0 / dist).min(1.0) * 0.05;
    let (nx, nz) = (dx / dist * scale, dz / dist * scale);
    let mx = (b.vel.x + a.vel.x) * 0.5;
    let mz = (b.vel.z + a.vel.z) * 0.5;
    a.vel.x = a.vel.x * 0.2 + (mx - nx);
    a.vel.z = a.vel.z * 0.2 + (mz - nz);
    b.vel.x = b.vel.x * 0.2 + (mx + nx);
    b.vel.z = b.vel.z * 0.2 + (mz + nz);
}

impl super::Entities {
    /// Using a cart item succeeds only on a rail, including its slope height.
    pub fn place_cart(&mut self, world: &World, kind: CartKind, cell: IVec3) -> Option<u32> {
        if kind.boat().is_some() {
            let block = world.get_block(cell)?;
            let pos = cell.as_dvec3() + DVec3::new(0.5, if block.is_water() { 0.45 } else { 0.0 }, 0.5);
            if physics::overlaps_solid(world, pos, Shape::new(0.6875, 0.5625)) {
                return None;
            }
            return Some(self.spawn_cart(kind, pos));
        }
        let block = world.get_block(cell).filter(|b| b.is_rail())?;
        let height = if block.rail_shape()?.is_ascending() { 0.5 } else { 0.0 };
        Some(self.spawn_cart(kind, cell.as_dvec3() + DVec3::new(0.5, 0.0625 + height, 0.5)))
    }

    pub fn spawn_cart(&mut self, kind: CartKind, pos: DVec3) -> u32 {
        let id = self.next_cart;
        self.next_cart = self.next_cart.saturating_add(1);
        self.minecarts.push(Minecart::new(id, kind, pos));
        id
    }

    pub fn cart_mut(&mut self, id: u32) -> Option<&mut Minecart> {
        self.minecarts.iter_mut().find(|c| c.id == id)
    }

    pub fn cart(&self, id: u32) -> Option<&Minecart> {
        self.minecarts.iter().find(|c| c.id == id)
    }

    /// Mount `player` on the cart under the crosshair. `None` if nothing rideable was hit.
    pub fn mount_cart(&mut self, eye: DVec3, dir: DVec3, reach: f64, player: super::PlayerId) -> Option<u32> {
        let i = self.cart_ray(eye, dir, reach)?;
        let cart = &mut self.minecarts[i];
        let mobs = self.mobs.iter().filter(|m| m.alive() && m.riding == Some(cart.id)).count();
        let players = usize::from(cart.rider.is_some()) + usize::from(cart.second_rider.is_some());
        if players + mobs >= cart.kind.seats() || cart.fuse.is_some() {
            return None;
        }
        if cart.rider.is_none() {
            cart.rider = Some(player);
        } else {
            cart.second_rider = Some(player);
        }
        Some(cart.id)
    }

    /// Chest or hopper cart under the crosshair, with its slot count.
    pub fn cart_container(&self, eye: DVec3, dir: DVec3, reach: f64) -> Option<(u32, usize)> {
        let cart = &self.minecarts[self.cart_ray(eye, dir, reach)?];
        let n = cart.slot_count();
        (n > 0).then_some((cart.id, n))
    }

    pub fn cart_ray(&self, eye: DVec3, dir: DVec3, reach: f64) -> Option<usize> {
        let dir = dir.normalize_or_zero();
        if dir == DVec3::ZERO {
            return None;
        }
        self.minecarts
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                let (min, max) = c.aabb();
                physics::ray_aabb(eye, dir, min, max).filter(|t| *t <= reach).map(|t| (i, t))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    /// Punch the cart under the crosshair. A broken cart becomes its item.
    pub fn hurt_cart(&mut self, eye: DVec3, dir: DVec3, reach: f64, creative: bool) -> bool {
        let Some(i) = self.cart_ray(eye, dir, reach) else { return false };
        if !creative
            && self.minecarts[i].kind == CartKind::Tnt
            && self.minecarts[i].vel.with_y(0.0).length_squared() >= 0.01
        {
            self.minecarts[i].fuse.get_or_insert(20);
            return true;
        }
        if !creative && self.minecarts[i].fuse.is_some() && self.minecarts[i].kind == CartKind::Tnt {
            return true;
        }
        if let Some(item) = self.minecarts[i].hurt(creative) {
            let pos = self.minecarts[i].pos;
            let id = self.minecarts[i].id;
            self.drop_cart(i, item, pos, !creative);
            self.clear_cart(id);
        }
        true
    }

    fn drop_cart(&mut self, index: usize, item: Item, pos: DVec3, drop_item: bool) {
        let cart = self.minecarts.swap_remove(index);
        if cart.kind == CartKind::Tnt && cart.fuse.is_some() {
            return;
        }
        for stack in cart.slots.into_iter().flatten() {
            self.items.push(super::ItemEntity::new(stack, pos + DVec3::Y * 0.2, DVec3::ZERO, 0.5, &mut self.rng));
        }
        if drop_item {
            self.items.push(super::ItemEntity::new(
                Stack::new(item, 1),
                pos + DVec3::Y * 0.2,
                DVec3::ZERO,
                0.5,
                &mut self.rng,
            ));
        }
    }

    fn clear_cart(&mut self, id: u32) {
        for mob in &mut self.mobs {
            if mob.riding == Some(id) {
                mob.riding = None;
            }
        }
    }

    pub fn dismount(&mut self, world: &World, player: super::PlayerId) -> Option<DVec3> {
        let cart = self.minecarts.iter_mut().find(|c| c.seat_for(player).is_some())?;
        if cart.rider == Some(player) {
            cart.rider = cart.second_rider.take();
        } else {
            cart.second_rider = None;
        }
        let side = DVec3::new(cart.yaw.cos() as f64, 0.0, -(cart.yaw.sin() as f64));
        for offset in
            [side * 1.2, -side * 1.2, DVec3::X * 1.2, DVec3::NEG_X * 1.2, DVec3::Z * 1.2, DVec3::NEG_Z * 1.2, DVec3::Y]
        {
            let pos = cart.pos + offset;
            if !physics::overlaps_solid(world, pos, crate::player::SHAPE) {
                return Some(pos);
            }
        }
        Some(cart.pos + DVec3::Y)
    }

    /// Contact push from a walking player or mob; velocity is in blocks/tick.
    pub fn push_carts(&mut self, feet: DVec3, half: f64) {
        for cart in &mut self.minecarts {
            if !near_cart(feet, half, cart.pos) {
                continue;
            }
            let delta = (cart.pos - feet).with_y(0.0);
            let dist = delta.length();
            if dist >= 0.01 {
                cart.vel += delta / dist * (1.0 / dist).min(1.0) * 0.05;
            }
        }
    }

    /// Advance every cart one game tick. `wish` is each rider's blocks-per-tick push.
    pub fn tick_minecarts(&mut self, world: &mut World, wish: &[(super::PlayerId, DVec3)]) {
        if self.minecarts.is_empty() {
            world.refresh_cart_signals([]);
            return;
        }
        for mob in &self.mobs {
            if mob.alive() && mob.riding.is_none() {
                for cart in &mut self.minecarts {
                    if near_cart(mob.pos, mob.shape().half_width, cart.pos) {
                        let delta = (cart.pos - mob.pos).with_y(0.0);
                        let dist = delta.length();
                        if dist >= 0.01 {
                            cart.vel += delta / dist * (1.0 / dist).min(1.0) * 0.05;
                        }
                    }
                }
            }
        }
        let mut signals = Vec::new();
        let n = self.minecarts.len();
        for i in 0..n {
            let passenger = self.minecarts[i].rider.and_then(|id| wish.iter().find(|w| w.0 == id).map(|w| w.1));
            let mob_passenger = self.mobs.iter().any(|m| m.riding == Some(self.minecarts[i].id) && m.alive());
            let id = self.minecarts[i].id;
            self.minecarts[i].previous_pos = self.minecarts[i].pos;
            self.minecarts[i].step(world, passenger.unwrap_or(DVec3::ZERO), mob_passenger);
            if self.minecarts[i].eject {
                self.minecarts[i].rider = None;
                for mob in &mut self.mobs {
                    if mob.riding == Some(id) {
                        mob.riding = None;
                    }
                }
            }
            let cart = &self.minecarts[i];
            if cart.slot_count() > 0 {
                // Detector comparators use the same entity search box as the
                // rail's occupancy test, even when a cart straddles two cells.
                let (min, max) = cart.aabb();
                let lo = min.floor().as_ivec3();
                let hi = max.floor().as_ivec3();
                for y in lo.y..=hi.y {
                    for z in lo.z..=hi.z {
                        for x in lo.x..=hi.x {
                            let p = IVec3::new(x, y, z);
                            if world.get_block(p).and_then(rails::kind) != Some(RailKind::Detector) {
                                continue;
                            }
                            let origin = p.as_dvec3();
                            if min.cmplt(origin + rails::DETECTOR_BOX.1).all()
                                && max.cmpgt(origin + rails::DETECTOR_BOX.0).all()
                            {
                                signals.push((p, cart.comparator_signal()));
                            }
                        }
                    }
                }
            }
            let mut passenger_index =
                usize::from(self.minecarts[i].rider.is_some()) + usize::from(self.minecarts[i].second_rider.is_some());
            for mob in &mut self.mobs {
                if mob.riding == Some(id) && mob.alive() {
                    let seat = self.minecarts[i].passenger_seat(passenger_index);
                    passenger_index += 1;
                    mob.pos = seat;
                    mob.previous_pos = seat;
                    mob.vel = DVec3::ZERO;
                    mob.on_ground = true;
                }
            }
        }
        world.refresh_cart_signals(signals);
        // Boats pick up small nearby mobs even while stationary; chest boats have one seat.
        for cart in &self.minecarts {
            let players = usize::from(cart.rider.is_some()) + usize::from(cart.second_rider.is_some());
            let taken = self.mobs.iter().filter(|m| m.alive() && m.riding == Some(cart.id)).count();
            let free = cart.kind.seats().saturating_sub(players + taken);
            if free == 0
                || cart.fuse.is_some()
                || cart.kind.boat().is_none() && cart.vel.with_y(0.0).length_squared() <= 0.01
            {
                continue;
            }
            for (offset, mob) in self
                .mobs
                .iter_mut()
                .filter(|m| {
                    m.alive()
                        && m.riding.is_none()
                        && m.dying.is_none()
                        && m.shape().half_width < cart.shape().half_width
                        && near_cart(m.pos, m.shape().half_width, cart.pos)
                })
                .take(free)
                .enumerate()
            {
                mob.riding = Some(cart.id);
                mob.persistent = true;
                mob.pos = cart.passenger_seat(players + taken + offset);
                mob.vel = DVec3::ZERO;
            }
        }
        let n = self.minecarts.len();
        for i in 0..n {
            for j in i + 1..n {
                let (left, right) = self.minecarts.split_at_mut(j);
                push_pair(&mut left[i], &mut right[0]);
            }
        }
        // A lit TNT cart explodes and is gone.
        let mut i = 0;
        while i < self.minecarts.len() {
            if self.minecarts[i].kind.boat().is_some() && self.minecarts[i].damage > 40.0 {
                let cart = self.minecarts.swap_remove(i);
                self.clear_cart(cart.id);
                if let Some((wood, _)) = cart.kind.boat() {
                    let plank = if wood == 9 {
                        crate::world::overworld_blocks::BAMBOO_PLANKS
                    } else {
                        crate::world::block::Wood::ALL[wood as usize].planks()
                    };
                    for stack in [Some(Stack::new(plank, 3)), Some(Stack::new(Item::STICK, 2))]
                        .into_iter()
                        .chain(cart.slots)
                        .flatten()
                    {
                        self.items.push(super::ItemEntity::new(stack, cart.pos, DVec3::ZERO, 0.5, &mut self.rng));
                    }
                }
                continue;
            }
            if self.minecarts[i].fuse == Some(0) || self.minecarts[i].damage > 40.0 && self.minecarts[i].pos.y < -64.0 {
                let cart = self.minecarts.swap_remove(i);
                self.clear_cart(cart.id);
                if cart.fuse == Some(0) {
                    let speed = cart.blast_speed.max(cart.vel.with_y(0.0).length()).min(5.0);
                    let power = 4.0 + self.rng.next_f32() * 1.5 * speed as f32;
                    self.tnt.push(super::tnt::PrimedTnt::with_power(cart.pos, power));
                }
                continue;
            }
            i += 1;
        }
    }

    pub(super) fn blast_carts(&mut self, center: DVec3, power: f32) {
        let mut i = 0;
        while i < self.minecarts.len() {
            let cart = &mut self.minecarts[i];
            if let Some((damage, _)) = super::explosion_damage(power, cart.pos.distance(center)) {
                if cart.kind == CartKind::Tnt {
                    if cart.fuse.is_none() {
                        cart.fuse = Some((self.rng.next_int(20) + self.rng.next_int(20)) as u16);
                    }
                } else if damage * 10.0 > 40.0 {
                    let (id, item, pos) = (cart.id, cart.kind.item(), cart.pos);
                    self.drop_cart(i, item, pos, true);
                    self.clear_cart(id);
                    continue;
                }
            }
            i += 1;
        }
    }

    pub fn minecarts_to_string(&self) -> String {
        self.minecarts.iter().map(Minecart::serialize).collect::<Vec<_>>().join(";")
    }

    pub fn load_minecarts(&mut self, text: &str) {
        for piece in text.split(';').filter(|s| !s.is_empty()) {
            if let Some(cart) = Minecart::deserialize(piece) {
                self.next_cart = self.next_cart.max(cart.id.saturating_add(1));
                self.minecarts.push(cart);
            }
        }
    }

    /// Reattach a saved rider once that player exists.
    pub fn claim_rider(&self, player: super::PlayerId) -> Option<u32> {
        self.minecarts.iter().find(|c| c.seat_for(player).is_some()).map(|c| c.id)
    }
}

fn near_cart(feet: DVec3, half: f64, cart: DVec3) -> bool {
    let d = feet - cart;
    d.x.abs() < 0.7 + half && d.z.abs() < 0.7 + half && d.y.abs() < 1.2
}

/// Insert `stack` into a cart inventory. Returns what did not fit.
pub fn insert_slots(slots: &mut [Option<Stack>], mut stack: Stack) -> Option<Stack> {
    for cell in slots.iter_mut() {
        if let Some(have) = cell
            && have.stacks_with(&stack)
        {
            let room = have.max().saturating_sub(have.count);
            let n = room.min(stack.count);
            have.count += n;
            stack.count -= n;
            if stack.count == 0 {
                return None;
            }
        }
    }
    for cell in slots.iter_mut() {
        if cell.is_none() {
            *cell = Some(stack);
            return None;
        }
    }
    Some(stack)
}

/// Take one item out of a cart inventory, if any.
pub fn take_one(slots: &mut [Option<Stack>]) -> Option<Stack> {
    for cell in slots.iter_mut() {
        let Some(have) = cell else { continue };
        let one = Stack { count: 1, ..*have };
        have.count -= 1;
        if have.count == 0 {
            *cell = None;
        }
        return Some(one);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::Block;
    use crate::world::chunk::ChunkData;
    use crate::world::terrain::Generator;
    use std::sync::Arc;

    fn world() -> World {
        let mut w = World::new_headless(Arc::new(Generator::new(3)), Default::default(), 2);
        w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        w
    }

    fn track(w: &mut World, z: i32, n: i32) {
        for x in 0..n {
            let p = IVec3::new(x, 140, z);
            w.set_block(p - IVec3::Y, Block::STONE);
            w.set_block(p, Block::RAIL);
        }
    }

    #[test]
    fn boats_have_two_seats_and_chest_boats_one_with_twenty_seven_slots() {
        for wood in 0..10 {
            for chest in [false, true] {
                let kind = CartKind::Boat { wood, chest };
                assert_eq!(CartKind::from_item(kind.item()), Some(kind));
                assert_eq!(CartKind::from_u8(kind.code()), Some(kind));
                assert_eq!(kind.seats(), if chest { 1 } else { 2 });
                let mut c = Minecart::new(1, kind, DVec3::new(3., 140., 3.));
                c.rider = Some(super::super::PlayerId::HOST);
                if !chest {
                    c.second_rider = Some(super::super::PlayerId(2));
                }
                c.slots[0] = chest.then(|| Stack::new(Item::DIAMOND, 3));
                let loaded = Minecart::deserialize(&c.serialize()).unwrap();
                assert_eq!(loaded.serialize(), c.serialize());
                assert_eq!(c.slot_count(), if chest { 27 } else { 0 });
            }
        }
    }

    #[test]
    fn boat_floats_and_paddles_then_coasts_on_water() {
        let mut w = world();
        for x in 0..16 {
            for z in 0..16 {
                w.set_block(IVec3::new(x, 140, z), Block::WATER);
            }
        }
        let mut c = Minecart::new(1, CartKind::Boat { wood: 0, chest: false }, DVec3::new(3., 140.45, 3.));
        for _ in 0..30 {
            c.step(&w, DVec3::X, false);
        }
        assert!(c.pos.x > 5. && c.pos.y > 140. && c.pos.y < 141.);
        let before = c.vel.x;
        c.step(&w, DVec3::ZERO, false);
        assert!(c.vel.x > 0. && c.vel.x < before);
    }

    #[test]
    fn boat_cannot_mount_third_player_and_dismount_keeps_other_rider() {
        let mut e = super::super::Entities::new(1);
        let mut w = world();
        let id = e.spawn_cart(CartKind::Boat { wood: 0, chest: false }, DVec3::new(3., 140., 3.));
        let eye = DVec3::new(3., 140.3, 1.);
        for player in [super::super::PlayerId(1), super::super::PlayerId(2)] {
            assert_eq!(e.mount_cart(eye, DVec3::Z, 5., player), Some(id));
        }
        assert_eq!(e.mount_cart(eye, DVec3::Z, 5., super::super::PlayerId(3)), None);
        assert!(e.dismount(&w, super::super::PlayerId(1)).is_some());
        assert_eq!(e.cart(id).unwrap().rider, Some(super::super::PlayerId(2)));
        e.tick_minecarts(&mut w, &[]);
    }

    #[test]
    fn straight_rail_keeps_speed_under_the_cap_and_friction_slows_it() {
        let mut w = world();
        track(&mut w, 0, 16);
        w.tick_redstone();
        let mut cart = Minecart::new(1, CartKind::Rideable, DVec3::new(1.5, 140.0, 0.5));
        cart.vel.x = 0.5;
        let before = cart.vel.x;
        cart.step(&w, DVec3::ZERO, false);
        let moved = cart.pos.x - 1.5;
        assert!(moved <= MAX_SPEED + 1e-6, "displacement {moved}");
        assert!(cart.vel.x < before, "friction");
        assert!((cart.pos.z - 0.5).abs() < 1e-6, "snapped to the rail");
        assert!(cart.pos.y > 140.0 && cart.pos.y < 141.0);
    }

    #[test]
    fn a_slope_adds_downhill_speed() {
        let mut w = world();
        // Rail at x=2 climbs east onto the rail at x=3,y=141.
        for x in 0..3 {
            let p = IVec3::new(x, 140, 0);
            w.set_block(p - IVec3::Y, Block::STONE);
            w.set_block(p, Block::RAIL);
        }
        w.set_block(IVec3::new(3, 140, 0), Block::STONE);
        w.set_block(IVec3::new(3, 141, 0) - IVec3::Y, Block::STONE);
        w.set_block(IVec3::new(2, 140, 0), Block::RAIL);
        w.set_block(IVec3::new(3, 141, 0), Block::RAIL);
        w.tick_redstone();
        let mut cart = Minecart::new(1, CartKind::Rideable, DVec3::new(2.5, 140.1, 0.5));
        cart.step(&w, DVec3::ZERO, false);
        assert!(cart.vel.x < 0.0, "the low end is west, vel {}", cart.vel.x);
    }

    #[test]
    fn powered_rail_boosts_and_unpowered_brakes() {
        let mut w = world();
        let p = IVec3::new(4, 140, 0);
        w.set_block(p - IVec3::Y, Block::STONE);
        w.set_block(p - IVec3::Y, crate::world::redstone_blocks::REDSTONE_BLOCK);
        w.set_block(p, rails::special(RailKind::Powered, RailShape::EastWest, false));
        w.tick_redstone();
        assert!(rails::is_powered(w.get_block(p).unwrap()), "redstone block powers the rail");
        let mut cart = Minecart::new(1, CartKind::Rideable, DVec3::new(4.5, 140.05, 0.5));
        cart.vel.x = 0.05;
        cart.step(&w, DVec3::ZERO, false);
        assert!(cart.vel.x > 0.05, "boost {}", cart.vel.x);

        w.set_block(p - IVec3::Y, Block::STONE);
        w.tick_redstone();
        let mut slow = Minecart::new(2, CartKind::Rideable, DVec3::new(4.5, 140.05, 0.5));
        slow.vel.x = 0.2;
        slow.step(&w, DVec3::ZERO, false);
        assert!(slow.vel.x < 0.12, "brake {}", slow.vel.x);
    }

    #[test]
    fn carts_exchange_momentum_and_saves_round_trip() {
        let mut a = Minecart::new(1, CartKind::Rideable, DVec3::new(0.0, 0.0, 0.0));
        let mut b = Minecart::new(2, CartKind::Chest, DVec3::new(0.4, 0.0, 0.0));
        a.vel.x = 0.3;
        a.slots[0] = Some(Stack::new(Item::DIAMOND, 3));
        b.kind = CartKind::Chest;
        push_pair(&mut a, &mut b);
        assert!(b.vel.x > 0.05 && a.vel.x < 0.3);
        let saved = a.serialize();
        let loaded = Minecart::deserialize(&saved).unwrap();
        assert_eq!(loaded.id, 1);
        assert_eq!(loaded.vel.x, a.vel.x);
        assert_eq!(loaded.slots[0], a.slots[0]);
    }

    #[test]
    fn derailed_cart_falls() {
        let w = world();
        let mut cart = Minecart::new(1, CartKind::Rideable, DVec3::new(2.5, 150.0, 2.5));
        let y = cart.pos.y;
        cart.step(&w, DVec3::ZERO, false);
        assert!(cart.pos.y < y);
    }
    #[test]
    fn curves_turn_motion_and_activators_eject_disable_and_prime() {
        let mut w = world();
        let p = IVec3::new(8, 140, 8);
        w.set_block(p - IVec3::Y, Block::STONE);
        w.set_block(p, Block::rail(RailShape::SouthEast));
        let mut c = Minecart::new(1, CartKind::Rideable, p.as_dvec3() + DVec3::new(0.5, 0.1, 0.9));
        c.vel.z = -0.2;
        c.step(&w, DVec3::ZERO, false);
        assert!(c.vel.x > 0.0 && c.vel.z < 0.0);
        w.set_block(p, rails::special(RailKind::Activator, RailShape::NorthSouth, true));
        for kind in [CartKind::Rideable, CartKind::Hopper, CartKind::Tnt] {
            let mut c = Minecart::new(1, kind, p.as_dvec3() + DVec3::new(0.5, 0.1, 0.5));
            c.rider = (kind == CartKind::Rideable).then_some(super::super::PlayerId::HOST);
            c.step(&w, DVec3::ZERO, false);
            match kind {
                CartKind::Rideable => assert!(c.eject),
                CartKind::Hopper => assert!(c.disabled),
                CartKind::Tnt => assert_eq!(c.fuse, Some(80)),
                _ => unreachable!(),
            }
        }
    }
    #[test]
    fn placement_requires_rail_and_variant_saves_preserve_contents_and_rider() {
        let mut w = world();
        let mut e = super::super::Entities::new(3);
        let p = IVec3::new(8, 140, 8);
        assert!(e.place_cart(&w, CartKind::Chest, p).is_none());
        w.set_block(p - IVec3::Y, Block::STONE);
        w.set_block(p, Block::RAIL);
        for kind in [CartKind::Rideable, CartKind::Chest, CartKind::Hopper, CartKind::Tnt] {
            let id = e.place_cart(&w, kind, p).unwrap();
            let cart = e.cart_mut(id).unwrap();
            if cart.slot_count() > 0 {
                cart.slots[0] = Some(Stack::new(Item::DIAMOND, 31).with_name("Cargo").unwrap());
                assert!(cart.comparator_signal() > 0);
            }
            if kind == CartKind::Rideable {
                cart.rider = Some(super::super::PlayerId::HOST);
            }
            if kind == CartKind::Tnt {
                cart.fuse = Some(39);
            }
            if kind == CartKind::Hopper {
                cart.disabled = true;
            }
        }
        let text = e.minecarts_to_string();
        let mut loaded = super::super::Entities::new(3);
        loaded.load_minecarts(&text);
        assert_eq!(loaded.minecarts_to_string(), text);
        assert!(loaded.claim_rider(super::super::PlayerId::HOST).is_some());
        assert!(Minecart::deserialize("garbage").is_none());
    }
    #[test]
    fn walls_block_cart_interactions_and_explosions_prime_tnt() {
        let mut w = world();
        let eye = DVec3::new(4.5, 140.4, 6.5);
        w.set_block(IVec3::new(4, 140, 8), Block::STONE);
        let mut e = super::super::Entities::new(3);
        let id = e.spawn_cart(CartKind::Tnt, DVec3::new(4.5, 140.0, 10.5));
        let reach = interaction_reach(&w, eye, DVec3::Z, 6.0);
        assert!(e.cart_ray(eye, DVec3::Z, reach).is_none());
        e.explode(eye + DVec3::Z * 4.0, 4.0);
        assert!(e.cart(id).unwrap().fuse.is_some());
    }
    #[test]
    fn detector_comparators_find_straddling_containers_and_use_the_first_cart() {
        let mut w = world();
        let p = IVec3::new(8, 140, 8);
        for q in [p, p + IVec3::X] {
            w.set_block(q - IVec3::Y, Block::STONE);
            w.set_block(q, rails::special(RailKind::Detector, RailShape::EastWest, false));
        }
        w.tick_redstone();
        let mut e = super::super::Entities::new(1);
        let pos = p.as_dvec3() + DVec3::new(0.99, 0.0625, 0.5);
        e.minecarts.push(Minecart::new(1, CartKind::Rideable, pos));
        let mut first = Minecart::new(2, CartKind::Chest, pos);
        first.slots[0] = Some(Stack::new(Item::DIAMOND, 1));
        e.minecarts.push(first);
        let mut full = Minecart::new(3, CartKind::Chest, pos);
        full.slots.fill(Some(Stack::new(Item::DIAMOND, 64)));
        e.minecarts.push(full);
        e.tick_minecarts(&mut w, &[]);
        w.redstone_contacts([], &e);
        w.tick_redstone();
        assert_eq!(w.container_signal(p), Some(1));
        assert_eq!(w.container_signal(p + IVec3::X), Some(1));
        // A cart above the detector's 0.8-high search box is not a contact.
        e.minecarts.truncate(1);
        e.minecarts[0].pos.y = p.y as f64 + 0.81;
        w.redstone_contacts([], &e);
        for _ in 0..21 {
            w.tick_redstone();
        }
        assert_eq!(w.container_signal(p), Some(0));
    }

    #[test]
    fn hopper_activation_latches_until_an_unpowered_activator_and_unloaded_fuses_pause() {
        let mut w = world();
        let p = IVec3::new(8, 140, 8);
        w.set_block(p - IVec3::Y, Block::STONE);
        let mut c = Minecart::new(1, CartKind::Hopper, p.as_dvec3() + DVec3::new(0.5, 0.1, 0.5));
        w.set_block(p, rails::special(RailKind::Activator, RailShape::NorthSouth, true));
        c.step(&w, DVec3::ZERO, false);
        assert!(c.disabled);
        w.set_block(p, Block::RAIL);
        c.step(&w, DVec3::ZERO, false);
        assert!(c.disabled);
        w.set_block(p, rails::ACTIVATOR_RAIL);
        c.step(&w, DVec3::ZERO, false);
        assert!(!c.disabled);
        let mut t = Minecart::new(2, CartKind::Tnt, DVec3::new(400.0, 140.0, 400.0));
        t.fuse = Some(30);
        t.step(&w, DVec3::ZERO, false);
        assert_eq!(t.fuse, Some(30));
    }
}
