//! Item frames, glow item frames, paintings and armor stands (Java 26.3's
//! `ItemFrame`, `GlowItemFrame`, `Painting` and `ArmorStand`): entities
//! placed from items, kept with the dimension's other entities and in
//! per-chunk storage, and saved under the level's `decor` property.
//!
//! Frames and paintings hang on the face of a block (`HangingEntity`): each
//! records the cell in front of the face (Java's `pos`) and the direction out
//! of the wall, and every 100 ticks checks that its wall still holds it
//! (`BlockAttachedEntity.tick`). Armor stands stand on the ground, fall when
//! nothing holds them up, and wear the four armor pieces.
//! Sources and differences: `docs/verification/2026-10-25-v0.12-utilities.md`.

use glam::{DVec3, IVec3, Quat, Vec3};

use super::{Entities, ItemEntity, model::EntityVertex};
use crate::inventory::{Inventory, Stack, stack_from_str, stack_to_string};
use crate::item::{ArmorMaterial, ArmorPiece, Item};
use crate::physics::{self, BlockSource, Shape};
use crate::world::block::Block;

/// What kind of decoration an entity is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Frame {
        glow: bool,
    },
    /// A painting of the variant at this index of [`PAINTINGS`].
    Painting(u8),
    Stand,
}

impl Kind {
    /// The item that places this decoration and that breaking it drops.
    pub fn item(self) -> Item {
        match self {
            Kind::Frame { glow: false } => Item::ITEM_FRAME,
            Kind::Frame { glow: true } => Item::GLOW_ITEM_FRAME,
            Kind::Painting(_) => Item::PAINTING,
            Kind::Stand => Item::ARMOR_STAND,
        }
    }

    pub fn from_item(item: Item) -> Option<Kind> {
        Some(match item {
            Item::ITEM_FRAME => Kind::Frame { glow: false },
            Item::GLOW_ITEM_FRAME => Kind::Frame { glow: true },
            Item::PAINTING => Kind::Painting(0),
            Item::ARMOR_STAND => Kind::Stand,
            _ => return None,
        })
    }
}

/// Java 26.3's painting variants (`data/minecraft/painting_variant`): name,
/// width and height in blocks, and whether the `placeable` tag lets a
/// painting item pick it. The art drawn for each is VoxelCraft's own.
pub const PAINTINGS: [(&str, u8, u8, bool); 51] = [
    ("alban", 1, 1, true),
    ("aztec", 1, 1, true),
    ("aztec2", 1, 1, true),
    ("backyard", 3, 4, true),
    ("baroque", 2, 2, true),
    ("bomb", 1, 1, true),
    ("bouquet", 3, 3, true),
    ("burning_skull", 4, 4, true),
    ("bust", 2, 2, true),
    ("cavebird", 3, 3, true),
    ("changing", 4, 2, true),
    ("cotan", 3, 3, true),
    ("courbet", 2, 1, true),
    ("creebet", 2, 1, true),
    ("dennis", 3, 3, true),
    ("donkey_kong", 4, 3, true),
    ("earth", 2, 2, false),
    ("endboss", 3, 3, true),
    ("fern", 3, 3, true),
    ("fighters", 4, 2, true),
    ("finding", 4, 2, true),
    ("fire", 2, 2, false),
    ("graham", 1, 2, true),
    ("humble", 2, 2, true),
    ("kebab", 1, 1, true),
    ("lowmist", 4, 2, true),
    ("match", 2, 2, true),
    ("meditative", 1, 1, true),
    ("orb", 4, 4, true),
    ("owlemons", 3, 3, true),
    ("passage", 4, 2, true),
    ("pigscene", 4, 4, true),
    ("plant", 1, 1, true),
    ("pointer", 4, 4, true),
    ("pond", 3, 4, true),
    ("pool", 2, 1, true),
    ("prairie_ride", 1, 2, true),
    ("sea", 2, 1, true),
    ("skeleton", 4, 3, true),
    ("skull_and_roses", 2, 2, true),
    ("stage", 2, 2, true),
    ("sunflowers", 3, 3, true),
    ("sunset", 2, 1, true),
    ("tides", 3, 3, true),
    ("unpacked", 4, 4, true),
    ("void", 2, 2, true),
    ("wanderer", 1, 2, true),
    ("wasteland", 1, 1, true),
    ("water", 2, 2, false),
    ("wind", 2, 2, false),
    ("wither", 2, 2, true),
];

/// `ItemFrame.NUM_ROTATIONS`.
pub const ROTATIONS: u8 = 8;
/// `BlockAttachedEntity.tick` checks its support every 100 ticks.
const CHECK_TICKS: u8 = 100;
/// `ArmorStand.hurtServer`: a second hit within 5 ticks breaks it.
const BREAK_WINDOW: u64 = 5;
/// The armor stand's size (`EntityType.ARMOR_STAND`: 0.5 x 1.975).
pub const STAND_SHAPE: Shape = Shape { half_width: 0.25, height: 1.975 };
/// Ticks an armor stand wobbles after a first hit (`ArmorStand.handleEntityEvent(32)`).
const WOBBLE_TICKS: u8 = 5;

/// An item frame, painting or armor stand.
#[derive(Clone, Debug, PartialEq)]
pub struct Decor {
    pub kind: Kind,
    /// Hanging decorations: the cell in front of the face they hang on.
    pub cell: IVec3,
    /// Hanging decorations: out of the wall (frames any of six ways,
    /// paintings horizontal).
    pub facing: IVec3,
    /// Armor stands: the feet.
    pub pos: DVec3,
    pub previous_pos: DVec3,
    vel_y: f64,
    /// Armor stands: Java's yaw in degrees (0 faces south, +Z).
    pub yaw: f32,
    /// What an item frame holds, turned `rotation` eighths of a turn.
    pub item: Option<Stack>,
    pub rotation: u8,
    /// An armor stand's equipment, indexed by [`ArmorPiece`].
    pub armor: [Option<Stack>; 4],
    check: u8,
    last_hit: Option<u64>,
    /// Ticks of wobble left after a first hit on an armor stand.
    pub wobble: u8,
    pub sky_light: f32,
    pub block_light: f32,
}

impl Decor {
    fn new(kind: Kind, cell: IVec3, facing: IVec3) -> Self {
        let pos = cell.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
        Decor {
            kind,
            cell,
            facing,
            pos,
            previous_pos: pos,
            vel_y: 0.0,
            yaw: 0.0,
            item: None,
            rotation: 0,
            armor: [None; 4],
            check: 0,
            last_hit: None,
            wobble: 0,
            sky_light: 1.0,
            block_light: 0.0,
        }
    }

    pub fn is_hanging(&self) -> bool {
        self.kind != Kind::Stand
    }

    /// Width and height of the hanging shape in blocks.
    fn size(&self) -> (f64, f64) {
        match self.kind {
            Kind::Painting(v) => {
                let (_, w, h, _) = PAINTINGS[v as usize % PAINTINGS.len()];
                (w as f64, h as f64)
            }
            _ => (0.75, 0.75),
        }
    }

    /// The bounding box (Java's `calculateBoundingBox`, and the pop box).
    pub fn aabb(&self) -> (DVec3, DVec3) {
        if self.kind == Kind::Stand {
            return STAND_SHAPE.aabb(self.pos);
        }
        let f = self.facing.as_dvec3();
        let mut centre = self.cell.as_dvec3() + DVec3::splat(0.5) - f * 0.46875;
        let (w, h) = self.size();
        let mut half = DVec3::splat(0.0);
        if let Kind::Painting(_) = self.kind {
            // `offsetForPaintingSize`: even sizes sit half a block over,
            // counterclockwise and up.
            let ccw = ccw(self.facing).as_dvec3();
            centre += ccw * offset(w) + DVec3::Y * offset(h);
            half += ccw.abs() * (w / 2.0) + DVec3::Y * (h / 2.0);
        } else {
            half = (DVec3::ONE - f.abs()) * (w / 2.0);
        }
        half += f.abs() * 0.03125;
        (centre - half, centre + half)
    }

    /// `HangingEntity.survives` (frames: `ItemFrame.survives`): nothing
    /// solid in the way, every block behind solid (or, for a frame on a side
    /// wall, a repeater or comparator), and no overlap with another
    /// decoration facing the same way (paintings: nor another painting).
    pub fn survives(&self, world: &impl BlockSource, others: &[Decor]) -> bool {
        if self.kind == Kind::Stand {
            return true;
        }
        let (min, max) = self.aabb();
        if physics::box_overlaps_solid(world, min, max) {
            return false;
        }
        let back = self.facing.as_dvec3() * -0.5;
        let (lo, hi) = ((min + back + DVec3::splat(1e-7)).floor(), (max + back - DVec3::splat(1e-7)).floor());
        let frame = matches!(self.kind, Kind::Frame { .. });
        for y in lo.y as i32..=hi.y as i32 {
            for z in lo.z as i32..=hi.z as i32 {
                for x in lo.x as i32..=hi.x as i32 {
                    let Some(b) = world.block(IVec3::new(x, y, z)) else { return false };
                    let diode = frame && self.facing.y == 0 && is_diode(b);
                    if !b.is_solid() && !diode {
                        return false;
                    }
                }
            }
        }
        // `canCoexist`: frames tolerate other kinds facing elsewhere.
        !others.iter().any(|o| {
            o.is_hanging()
                && !std::ptr::eq(o, self)
                && (o.facing == self.facing || (!frame && discriminant(o.kind) == discriminant(self.kind)))
                && overlaps(o.aabb(), (min, max))
        })
    }

    fn serialize(&self) -> String {
        let (c, f) = (self.cell, self.facing);
        match self.kind {
            Kind::Frame { glow } => format!(
                "F,{},{},{},{},{},{},{},{},{}",
                glow as u8,
                c.x,
                c.y,
                c.z,
                f.x,
                f.y,
                f.z,
                self.rotation,
                stack_to_string(self.item)
            ),
            Kind::Painting(v) => {
                format!("P,{},{},{},{},{},{}", c.x, c.y, c.z, f.x, f.z, PAINTINGS[v as usize % PAINTINGS.len()].0)
            }
            Kind::Stand => {
                let p = self.pos;
                let a = self.armor.map(stack_to_string);
                format!("S,{:.3},{:.3},{:.3},{},{},{},{},{}", p.x, p.y, p.z, self.yaw, a[0], a[1], a[2], a[3])
            }
        }
    }

    fn deserialize(text: &str) -> Option<Decor> {
        let f: Vec<&str> = text.split(',').collect();
        let int = |i: usize| f.get(i).and_then(|s| s.parse::<i32>().ok());
        let ivec = |i: usize| Some(IVec3::new(int(i)?, int(i + 1)?, int(i + 2)?));
        match *f.first()? {
            "F" if f.len() == 10 => {
                let facing = ivec(5)?;
                if !is_unit(facing) {
                    return None;
                }
                let mut d = Decor::new(Kind::Frame { glow: int(1)? != 0 }, ivec(2)?, facing);
                d.rotation = (int(8)?.rem_euclid(ROTATIONS as i32)) as u8;
                d.item = stack_from_str(f[9])?;
                Some(d)
            }
            "P" if f.len() == 7 => {
                let facing = IVec3::new(int(4)?, 0, int(5)?);
                let v = PAINTINGS.iter().position(|p| p.0 == f[6])?;
                let cell = ivec(1)?;
                is_unit(facing).then(|| Decor::new(Kind::Painting(v as u8), cell, facing))
            }
            "S" if f.len() == 9 => {
                let num = |i: usize| f.get(i).and_then(|s| s.parse::<f64>().ok()).filter(|v| v.is_finite());
                let pos = DVec3::new(num(1)?, num(2)?, num(3)?);
                let mut d = Decor::new(Kind::Stand, pos.floor().as_ivec3(), IVec3::Y);
                (d.pos, d.previous_pos, d.yaw) = (pos, pos, num(4)? as f32);
                for i in 0..4 {
                    // An unreadable slot is lost on its own, not the whole stand.
                    d.armor[i] = stack_from_str(f[5 + i]).flatten();
                }
                Some(d)
            }
            _ => None,
        }
    }
}

fn discriminant(k: Kind) -> u8 {
    match k {
        Kind::Frame { .. } => 0,
        Kind::Painting(_) => 1,
        Kind::Stand => 2,
    }
}

fn is_unit(v: IVec3) -> bool {
    v.abs().element_sum() == 1
}

fn overlaps(a: (DVec3, DVec3), b: (DVec3, DVec3)) -> bool {
    a.0.cmplt(b.1).all() && a.1.cmpgt(b.0).all()
}

/// `Direction.getCounterClockWise` about the vertical axis.
fn ccw(f: IVec3) -> IVec3 {
    IVec3::new(f.z, 0, -f.x)
}

/// `Painting.offsetForPaintingSize`.
fn offset(size: f64) -> f64 {
    if size as i32 % 2 == 0 { 0.5 } else { 0.0 }
}

/// `DiodeBlock.isDiode`: repeaters and comparators.
fn is_diode(b: Block) -> bool {
    use crate::world::redstone_blocks::{Component, component};
    matches!(component(b), Some(Component::Repeater { .. } | Component::Comparator { .. }))
}

/// The armor stand slot an item goes to (`getEquipmentSlotForItem`):
/// armor its piece, a carved pumpkin the head; `None` for anything else
/// (the hands, which item-placed stands don't show).
pub fn stand_slot(item: Item) -> Option<ArmorPiece> {
    if let Some((piece, _)) = item.as_armor() {
        return Some(piece);
    }
    (item.block().map(Block::base) == Some(Block::CARVED_PUMPKIN)).then_some(ArmorPiece::Helmet)
}

/// `ArmorStand.getClickedSlot` for a stand of normal size: the slot whose
/// height band `y` (above the feet) falls in, among the filled ones.
pub fn clicked_slot(armor: &[Option<Stack>; 4], y: f64) -> Option<ArmorPiece> {
    let has = |p: ArmorPiece| armor[p as usize].is_some();
    if (0.1..0.55).contains(&y) && has(ArmorPiece::Boots) {
        Some(ArmorPiece::Boots)
    } else if (0.9..1.6).contains(&y) && has(ArmorPiece::Chestplate) {
        Some(ArmorPiece::Chestplate)
    } else if (0.4..1.2).contains(&y) && has(ArmorPiece::Leggings) {
        Some(ArmorPiece::Leggings)
    } else if y >= 1.6 && has(ArmorPiece::Helmet) {
        Some(ArmorPiece::Helmet)
    } else {
        None
    }
}

/// What a use or hit on a decoration did, for adapters' sounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Placed(Kind),
    /// An item went into a frame (`ITEM_FRAME_ADD_ITEM`).
    ItemAdded,
    ItemRotated,
    ItemRemoved,
    Broken(Kind),
    /// A first hit on an armor stand.
    Wobbled,
    /// An armor stand took or gave a piece.
    Equipped,
    /// The decoration took the click but nothing happened (Java's `FAIL`).
    Refused,
}

impl Entities {
    /// The nearest decoration the ray hits within `reach`, with the distance.
    pub fn decor_ray(&self, eye: DVec3, dir: DVec3, reach: f64) -> Option<(usize, f64)> {
        let dir = dir.normalize_or_zero();
        if dir == DVec3::ZERO {
            return None;
        }
        self.decor
            .iter()
            .enumerate()
            .filter_map(|(i, d)| {
                let (min, max) = d.aabb();
                physics::ray_aabb(eye, dir, min, max).filter(|t| *t <= reach).map(|t| (i, t))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// `HangingEntityItem.useOn` and `ArmorStandItem.useOn`: places the
    /// decoration `item` makes against the face `normal` of `pos`. `players`
    /// are the feet of every player (they block armor stands). `None` when
    /// `item` places nothing; `Some(None)` when it can't go there.
    #[allow(clippy::too_many_arguments)]
    pub fn place_decor<W: BlockSource + ?Sized>(
        &mut self,
        world: &W,
        item: Item,
        pos: IVec3,
        normal: IVec3,
        player_yaw: f32,
        players: &[DVec3],
    ) -> Option<Option<Effect>> {
        let kind = Kind::from_item(item)?;
        if !is_unit(normal) {
            return Some(None);
        }
        let world = &Wrapped(world);
        if kind == Kind::Stand {
            if normal == IVec3::NEG_Y {
                return Some(None);
            }
            // `BlockPlaceContext`: a replaceable block clicked is replaced.
            let cell = if world.block(pos).is_some_and(Block::is_replaceable) { pos } else { pos + normal };
            let feet = cell.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
            let (min, max) = STAND_SHAPE.aabb(feet);
            let player = crate::player::SHAPE;
            let crowded = players.iter().any(|&p| overlaps(player.aabb(p), (min, max)))
                || self.mobs.iter().any(|m| m.alive() && overlaps(m.aabb(), (min, max)))
                || self.decor.iter().any(|d| overlaps(d.aabb(), (min, max)));
            if physics::box_overlaps_solid(world, min, max) || crowded {
                return Some(None);
            }
            let mut d = Decor::new(Kind::Stand, cell, IVec3::Y);
            // `Mth.floor((wrapDegrees(rotation - 180) + 22.5) / 45) * 45`.
            let wrapped = (player_yaw - 180.0 + 180.0).rem_euclid(360.0) - 180.0;
            d.yaw = ((wrapped + 22.5) / 45.0).floor() * 45.0;
            self.decor.push(d);
            return Some(Some(Effect::Placed(kind)));
        }
        // Paintings hang only on walls (`HangingEntity.setDirection`).
        if matches!(kind, Kind::Painting(_)) && normal.y != 0 {
            return Some(None);
        }
        let cell = pos + normal;
        if let Kind::Painting(_) = kind {
            // `Painting.create`: of the placeable variants that survive
            // here, the largest by area, chosen at random.
            let fits: Vec<(u8, u32)> = PAINTINGS
                .iter()
                .enumerate()
                .filter(|(_, p)| p.3)
                .map(|(i, p)| (i as u8, p.1 as u32 * p.2 as u32))
                .filter(|&(i, _)| Decor::new(Kind::Painting(i), cell, normal).survives(world, &self.decor))
                .collect();
            let Some(best) = fits.iter().map(|f| f.1).max() else { return Some(None) };
            let choices: Vec<u8> = fits.iter().filter(|f| f.1 == best).map(|f| f.0).collect();
            let v = choices[self.rng.next_int(choices.len() as u32) as usize];
            self.decor.push(Decor::new(Kind::Painting(v), cell, normal));
            return Some(Some(Effect::Placed(Kind::Painting(v))));
        }
        let d = Decor::new(kind, cell, normal);
        if !d.survives(world, &self.decor) {
            return Some(None);
        }
        self.decor.push(d);
        Some(Some(Effect::Placed(kind)))
    }

    /// Right-clicking a decoration (`ItemFrame.interact`,
    /// `ArmorStand.interactAt`) with the stack in `slot`. `None` when the
    /// ray hits none or the click passes.
    pub fn use_decor(
        &mut self,
        eye: DVec3,
        dir: DVec3,
        reach: f64,
        inventory: &mut Inventory,
        slot: usize,
        creative: bool,
    ) -> Option<(DVec3, Effect)> {
        let (i, t) = self.decor_ray(eye, dir, reach)?;
        let at = eye + dir.normalize_or_zero() * t;
        let held = inventory.get(slot);
        let d = &mut self.decor[i];
        match d.kind {
            Kind::Frame { .. } => {
                if d.item.is_some() {
                    d.rotation = (d.rotation + 1) % ROTATIONS;
                    return Some((at, Effect::ItemRotated));
                }
                let stack = held?;
                d.item = Some(Stack { count: 1, ..stack });
                if !creative {
                    inventory.take_one(slot);
                }
                Some((at, Effect::ItemAdded))
            }
            Kind::Painting(_) => None,
            Kind::Stand => {
                // `getClickedSlot` empty-handed, the item's own slot otherwise.
                let piece = match held {
                    None => clicked_slot(&d.armor, at.y - d.pos.y)?,
                    // Item-placed stands have no arms to hold anything else.
                    Some(s) => match stand_slot(s.item) {
                        Some(piece) => piece,
                        None => return Some((at, Effect::Refused)),
                    },
                };
                let worn = d.armor[piece as usize];
                // `ArmorStand.swapItem`.
                match held {
                    Some(s) if creative && worn.is_none() => d.armor[piece as usize] = Some(Stack { count: 1, ..s }),
                    Some(s) if s.count > 1 => {
                        if worn.is_some() {
                            return Some((at, Effect::Refused));
                        }
                        d.armor[piece as usize] = Some(Stack { count: 1, ..s });
                        inventory.take_one(slot);
                    }
                    _ => {
                        d.armor[piece as usize] = held;
                        inventory.slots[slot] = worn;
                    }
                }
                Some((at, Effect::Equipped))
            }
        }
    }

    /// Punching the decoration under the crosshair (`ItemFrame.hurtServer`,
    /// `HangingEntity`'s break, `ArmorStand.hurtServer`). Creative players
    /// break without drops.
    pub fn hurt_decor(&mut self, eye: DVec3, dir: DVec3, reach: f64, creative: bool) -> Option<(DVec3, Effect)> {
        let (i, t) = self.decor_ray(eye, dir, reach)?;
        let at = eye + dir.normalize_or_zero() * t;
        let now = self.decor_clock;
        let drops = !creative && self.entity_drops;
        let d = &mut self.decor[i];
        match d.kind {
            Kind::Frame { .. } if d.item.is_some() => {
                // The framed item pops out; the frame stays.
                let stack = d.item.take();
                let spot = drop_spot(d);
                if let Some(s) = stack.filter(|_| drops) {
                    self.items.push(ItemEntity::new(s, spot, DVec3::ZERO, 0.5, &mut self.rng));
                }
                Some((at, Effect::ItemRemoved))
            }
            Kind::Stand if !creative && d.last_hit.is_none_or(|last| now.saturating_sub(last) > BREAK_WINDOW) => {
                d.last_hit = Some(now);
                d.wobble = WOBBLE_TICKS;
                Some((at, Effect::Wobbled))
            }
            kind => {
                self.break_decor(i, drops, drops);
                Some((at, Effect::Broken(kind)))
            }
        }
    }

    /// Removes decoration `i`, dropping its item (`drop_self`) and what it
    /// holds or wears (`drop_contents`).
    fn break_decor(&mut self, i: usize, drop_self: bool, drop_contents: bool) {
        let d = self.decor.swap_remove(i);
        let spot = drop_spot(&d);
        let mut out: Vec<Stack> = Vec::new();
        if drop_self {
            out.push(Stack::new(d.kind.item(), 1));
        }
        if drop_contents {
            out.extend(d.item);
            out.extend(d.armor.iter().flatten().copied());
        }
        for s in out {
            self.items.push(ItemEntity::new(s, spot, DVec3::ZERO, 0.5, &mut self.rng));
        }
    }

    /// One tick of every decoration in a ticking chunk: hanging ones check
    /// their wall every 100 ticks and drop their item when it's gone;
    /// armor stands fall.
    pub(super) fn tick_decor<W: super::MobWorld + ?Sized>(&mut self, world: &W) {
        self.decor_clock += 1;
        let mut gone = Vec::new();
        for i in 0..self.decor.len() {
            let d = &mut self.decor[i];
            d.previous_pos = d.pos;
            let at = if d.is_hanging() { d.cell } else { d.pos.floor().as_ivec3() };
            if !world.entity_ticking(at) {
                continue;
            }
            d.wobble = d.wobble.saturating_sub(1);
            let probe = if d.is_hanging() { d.cell.as_dvec3() + DVec3::splat(0.5) } else { d.pos + DVec3::Y };
            d.sky_light = super::sky_light(world, probe);
            d.block_light = world.block_light(probe.floor().as_ivec3()) as f32 / 15.0;
            if d.is_hanging() {
                d.check += 1;
                if d.check >= CHECK_TICKS {
                    d.check = 0;
                    if !self.decor[i].survives(&Wrapped(world), &self.decor) {
                        gone.push(i);
                    }
                }
                continue;
            }
            // `LivingEntity.travel`: gravity 0.08 and drag 0.98 a tick.
            d.vel_y = (d.vel_y - 0.08) * 0.98;
            let mut vel = DVec3::Y * d.vel_y;
            let hit = physics::move_box(world, &mut d.pos, &mut vel, DVec3::Y * d.vel_y, STAND_SHAPE);
            if hit.on_ground {
                d.vel_y = 0.0;
            }
            if d.pos.y < -128.0 {
                gone.push(i);
            }
        }
        let drops = self.entity_drops;
        for i in gone.into_iter().rev() {
            self.break_decor(i, drops, drops);
        }
    }

    /// `ArmorStand.hurtServer` for explosions: stands caught in a blast break,
    /// dropping only what they wear; hanging decorations drop their item.
    pub(super) fn blast_decor(&mut self, center: DVec3, power: f32) {
        let drops = self.entity_drops;
        let mut i = self.decor.len();
        while i > 0 {
            i -= 1;
            let (min, max) = self.decor[i].aabb();
            if super::explosion_damage(power, ((min + max) * 0.5).distance(center), 1.0).is_some() {
                let stand = self.decor[i].kind == Kind::Stand;
                self.break_decor(i, drops && !stand, drops);
            }
        }
    }

    /// Every item frame's comparator output (`ItemFrame.getAnalogOutput`:
    /// 0 empty, else the rotation plus one), by its cell and facing.
    pub fn frame_signals(&self) -> impl Iterator<Item = ((IVec3, IVec3), u8)> + '_ {
        self.decor.iter().filter(|d| matches!(d.kind, Kind::Frame { .. })).map(|d| {
            let signal = if d.item.is_some() { d.rotation % 8 + 1 } else { 0 };
            ((d.cell, d.facing), signal)
        })
    }

    /// Falling stalactites that landed this tick hurt the mobs in their cells.
    pub fn stalactite_hits(&mut self, hits: &[(IVec3, f32)]) {
        for &(cell, damage) in hits {
            let cell_box = (cell.as_dvec3(), cell.as_dvec3() + DVec3::ONE);
            for m in &mut self.mobs {
                if overlaps(m.aabb(), cell_box) {
                    m.damage(damage, None, &mut self.rng);
                }
            }
        }
    }

    /// Decorations for the level file, those in unloaded chunks included.
    pub fn decor_to_string(&self) -> String {
        let stored = self.storage.chunks();
        let all = self.decor.iter().chain(stored.iter().flat_map(|c| &c.decor));
        all.map(Decor::serialize).collect::<Vec<_>>().join(";")
    }

    /// Restores [`Entities::decor_to_string`]; malformed entries are skipped.
    pub fn load_decor(&mut self, text: &str) {
        self.decor.extend(text.split(';').filter(|s| !s.is_empty()).filter_map(Decor::deserialize));
    }

    /// Up to `max` loaded decorations within `radius` of `pos`, in the
    /// [`Entities::decor_to_string`] format, for a LAN snapshot.
    pub fn decor_near(&self, pos: DVec3, radius: f64, max: usize) -> String {
        let near = self.decor.iter().filter(|d| d.pos.distance_squared(pos) < radius * radius).take(max);
        near.map(Decor::serialize).collect::<Vec<_>>().join(";")
    }

    /// A LAN client's copy of the host's decorations near it: replaces the
    /// list, keeping each armor stand's last position for smooth motion.
    pub fn replace_decor(&mut self, text: &str) {
        let previous: Vec<(IVec3, DVec3)> =
            self.decor.iter().filter(|d| d.kind == Kind::Stand).map(|d| (d.cell, d.pos)).collect();
        self.decor.clear();
        self.load_decor(text);
        for d in self.decor.iter_mut().filter(|d| d.kind == Kind::Stand) {
            if let Some(&(_, pos)) = previous.iter().find(|(cell, _)| *cell == d.cell) {
                d.previous_pos = pos;
            }
        }
    }
}

/// Java's `spawnAtLocation` for a hanging entity: a little out from the wall.
fn drop_spot(d: &Decor) -> DVec3 {
    if d.is_hanging() {
        let (min, max) = d.aabb();
        (min + max) * 0.5 + d.facing.as_dvec3() * 0.15
    } else {
        d.pos + DVec3::Y * 0.5
    }
}

/// Adapts any block source for the `impl BlockSource` helpers.
struct Wrapped<'a, W: BlockSource + ?Sized>(&'a W);

impl<W: BlockSource + ?Sized> BlockSource for Wrapped<'_, W> {
    fn block(&self, p: IVec3) -> Option<Block> {
        self.0.block(p)
    }
}

/// Every framed item for the renderer's free-standing block models: its
/// centre, orientation (model +Z out of the wall), stack, and sky and
/// block light (a glow frame's item is fully lit).
pub fn framed_items(decor: &[Decor]) -> impl Iterator<Item = (DVec3, Quat, Stack, f32, f32)> + '_ {
    decor.iter().filter_map(|d| {
        let stack = d.item?;
        let Kind::Frame { glow } = d.kind else { return None };
        let f = d.facing.as_vec3();
        let centre = d.cell.as_dvec3() + DVec3::splat(0.5) - d.facing.as_dvec3() * (0.5 - 0.0625 - 0.03);
        // The icon's front (+Z) turns to face out of the wall, then the
        // item turns about that axis by its rotation.
        let face = wall_rotation(d.facing);
        let spin = Quat::from_axis_angle(f, -(d.rotation as f32) * std::f32::consts::FRAC_PI_4);
        let light = if glow { (1.0, 1.0) } else { (d.sky_light, d.block_light) };
        Some((centre, spin * face, stack, light.0, light.1))
    })
}

/// Camera-relative triangles for every decoration within `max_dist`.
pub(super) fn build(decor: &[Decor], camera: DVec3, max_dist: f32, alpha: f64, out: &mut Vec<EntityVertex>) {
    for d in decor {
        let pos =
            if d.is_hanging() { d.cell.as_dvec3() + DVec3::splat(0.5) } else { d.previous_pos.lerp(d.pos, alpha) };
        let rel = (pos - camera).as_vec3();
        if rel.length_squared() > max_dist * max_dist {
            continue;
        }
        let light = [(d.sky_light.clamp(0.0, 1.0) * 255.0) as u8, 0, 0, 0];
        let torch = (d.block_light.clamp(0.0, 1.0) * 255.0) as u8;
        match d.kind {
            Kind::Frame { glow } => build_frame(d, rel, glow, (light, torch), out),
            Kind::Painting(v) => build_painting(d, rel, v, (light, torch), out),
            Kind::Stand => build_stand(d, rel, (light, torch), out),
        }
    }
}

type Light = ([u8; 4], u8);

/// The rotation taking model +Z (out of the wall) to `facing`, keeping
/// model +Y up on walls.
fn wall_rotation(facing: IVec3) -> Quat {
    match (facing.x, facing.y, facing.z) {
        (_, 1, _) => Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
        (_, -1, _) => Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        _ => Quat::from_rotation_y((facing.x as f32).atan2(facing.z as f32)),
    }
}

fn build_frame(d: &Decor, rel: Vec3, glow: bool, light: Light, out: &mut Vec<EntityVertex>) {
    use super::model::{cube, push_cuboid};
    let rot = wall_rotation(d.facing);
    // Model space: pixels, the wall at z = -8, the cell centre at 0.
    let xf = |p: Vec3| rel + rot * (p / 16.0);
    let (wood, back) = if glow { ([214, 170, 92], [74, 150, 140]) } else { ([150, 104, 60], [118, 86, 56]) };
    let light = if glow { ([255, 0, 0, 255], 255) } else { light };
    push_cuboid(out, &cube([-5., -5., -8.], [5., 5., -7.5], back, 18), &xf, rot, light, ([0.; 3], 0.), 3.0);
    for (min, max) in [
        ([-6., -6., -8.], [6., -5., -7.]),
        ([-6., 5., -8.], [6., 6., -7.]),
        ([-6., -5., -8.], [-5., 5., -7.]),
        ([5., -5., -8.], [6., 5., -7.]),
    ] {
        push_cuboid(out, &cube(min, max, wood, 24), &xf, rot, light, ([0.; 3], 0.), 5.0);
    }
}

fn build_painting(d: &Decor, rel: Vec3, v: u8, light: Light, out: &mut Vec<EntityVertex>) {
    use super::model::{cube, push_cuboid};
    let (_, w, h, _) = PAINTINGS[v as usize % PAINTINGS.len()];
    // Work in pixels from the cell centre, straight from the bounding box.
    let centre = d.cell.as_dvec3() + DVec3::splat(0.5);
    let (min, max) = d.aabb();
    let (lo, hi) = (((min - centre) * 16.0).as_vec3(), ((max - centre) * 16.0).as_vec3());
    let xf = |p: Vec3| rel + p / 16.0;
    let f = d.facing.as_vec3();
    // The back board fills the box but its front sixteenth.
    let (mut bmin, mut bmax) = (lo, hi);
    if f.x + f.y + f.z > 0.0 {
        bmax -= f.abs() * 0.5;
    } else {
        bmin += f.abs() * 0.5;
    }
    let board = cube(bmin.to_array(), bmax.to_array(), [104, 78, 50], 20);
    push_cuboid(out, &board, &xf, Quat::IDENTITY, light, ([0.; 3], 0.), 7.0);
    // The art faces out; seen from the front, `right` runs left to right.
    let right = (-f).cross(Vec3::Y);
    let front = (lo + hi) * 0.5 + f * 0.5;
    let corner = front - right * (w as f32 * 8.0) - Vec3::Y * (h as f32 * 8.0);
    let (cw, ch) = (w as usize * 4, h as usize * 4);
    for cy in 0..ch {
        for cx in 0..cw {
            let c = painting_colour(v, cw, ch, cx, cy);
            let a = corner + right * (cx as f32 * 4.0) + Vec3::Y * (cy as f32 * 4.0);
            push_tile(out, &xf, f, [a, a + right * 4.0, a + right * 4.0 + Vec3::Y * 4.0, a + Vec3::Y * 4.0], c, light);
        }
    }
}

/// One flat tile with normal `n` through four corners (in pixels).
fn push_tile(
    out: &mut Vec<EntityVertex>,
    xf: &impl Fn(Vec3) -> Vec3,
    n: Vec3,
    quad: [Vec3; 4],
    c: [u8; 3],
    light: Light,
) {
    let shade = 0.8 * n.x * n.x + 0.68 * n.z * n.z + if n.y > 0.0 { 1.0 } else { 0.55 } * n.y * n.y;
    let mut l = light.0;
    l[1] = (shade * 255.0) as u8;
    for i in [0, 1, 2, 0, 2, 3] {
        let p = quad[i];
        out.push(EntityVertex {
            pos: xf(p).to_array(),
            uv: [p.x + p.z, p.y],
            color: [c[0], c[1], c[2], 10],
            light: l,
            torch: [light.1, 0, 0, 0],
        });
    }
}

/// VoxelCraft's own art for painting `v`: one of a few compositions
/// (landscape, blocks of colour, a figure, waves) in a palette drawn from
/// the variant, on a `w` x `h` grid of cells (`y` up).
pub fn painting_colour(v: u8, w: usize, h: usize, x: usize, y: usize) -> [u8; 3] {
    let seed = v as u32 * 2_654_435_761;
    let hash = |a: u32| {
        let mut s = seed ^ a.wrapping_mul(0x9E37_79B9);
        s ^= s >> 15;
        s = s.wrapping_mul(0x85EB_CA6B);
        s ^= s >> 13;
        s
    };
    let palette = |i: u32| {
        let r = hash(100 + i);
        [(r & 0xFF) as u8 / 2 + 60, (r >> 8 & 0xFF) as u8 / 2 + 50, (r >> 16 & 0xFF) as u8 / 2 + 40]
    };
    let (fx, fy) = ((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
    // A dark rim one cell wide.
    if x == 0 || y == 0 || x + 1 == w || y + 1 == h {
        return [52, 40, 30];
    }
    let grain = (hash(x as u32 * 977 + y as u32 * 131) % 24) as u8;
    let lift = |c: [u8; 3]| c.map(|ch| ch.saturating_add(grain));
    lift(match v % 4 {
        0 => {
            // Landscape: sky, a sun, hills.
            let hill = 0.35 + 0.12 * (fx * 6.0 + v as f32).sin();
            let (sx, sy) = (0.25 + (hash(1) % 50) as f32 / 100.0, 0.75);
            if ((fx - sx) * w as f32).hypot((fy - sy) * h as f32) < 1.6 {
                [236, 210, 120]
            } else if fy < hill {
                palette(1)
            } else {
                let sky = palette(2);
                sky.map(|ch| (ch as f32 * (0.8 + fy * 0.4)).min(255.0) as u8)
            }
        }
        1 => {
            // Blocks of colour divided by dark lines.
            let (cx, cy) = (2 + hash(3) as usize % (w - 2).max(1), 2 + hash(4) as usize % (h - 2).max(1));
            if x == cx || y == cy { [40, 34, 30] } else { palette(((x > cx) as u32) + 2 * ((y > cy) as u32) + 5) }
        }
        2 => {
            // A figure against a dark ground.
            let (dx, dy) = ((fx - 0.5) * 2.2, (fy - 0.45) * 1.6);
            let head = (dx * dx + (dy - 0.45).powi(2) * 3.0) < 0.12;
            if head {
                [214, 170, 140]
            } else if dx.abs() < 0.55 - dy * 0.3 && dy < 0.2 {
                palette(9)
            } else {
                palette(10).map(|ch| ch / 2)
            }
        }
        _ => {
            // Waves of two colours.
            let band = ((fy * 5.0 + (fx * 9.0 + v as f32).sin() * 0.4) as i32).rem_euclid(2);
            if band == 0 { palette(11) } else { palette(12) }
        }
    })
}

fn armor_colour(material: ArmorMaterial) -> [u8; 3] {
    match material {
        ArmorMaterial::Leather => [160, 101, 64],
        ArmorMaterial::Iron => [198, 198, 198],
        ArmorMaterial::Gold => [232, 196, 60],
        ArmorMaterial::Diamond => [92, 219, 213],
        ArmorMaterial::Netherite => [74, 66, 70],
        ArmorMaterial::Turtle => [80, 160, 72],
        ArmorMaterial::Copper => [196, 112, 80],
    }
}

fn build_stand(d: &Decor, rel: Vec3, light: Light, out: &mut Vec<EntityVertex>) {
    use super::model::{cube, push_cuboid};
    let wobble = if d.wobble > 0 { (d.wobble as f32 * 1.7).sin() * 0.15 } else { 0.0 };
    let rot = Quat::from_rotation_y(-d.yaw.to_radians()) * Quat::from_rotation_z(wobble);
    let xf = |p: Vec3| rel + rot * (p / 16.0);
    let wood = [164, 128, 78];
    let stone = [150, 150, 150];
    let parts: [([f32; 3], [f32; 3], [u8; 3]); 7] = [
        ([-6., 0., -6.], [6., 1., 6.], stone),
        ([-3., 1., -1.], [-1., 12., 1.], wood),
        ([1., 1., -1.], [3., 12., 1.], wood),
        ([-4., 12., -1.], [4., 14., 1.], wood),
        ([-1., 14., -1.], [1., 21., 1.], wood),
        ([-6., 21., -1.5], [6., 24., 1.5], wood),
        ([-1., 24., -1.], [1., 31., 1.], wood),
    ];
    for (min, max, c) in parts {
        push_cuboid(out, &cube(min, max, c, 18), &xf, rot, light, ([0.; 3], 0.), 11.0);
    }
    // Worn pieces as shells around the stand.
    for (piece, stack) in ArmorPiece::ALL.iter().zip(&d.armor) {
        let Some(stack) = stack else { continue };
        let c = match stack.item.as_armor() {
            Some((_, m)) => armor_colour(m),
            None => [214, 120, 32],
        };
        let (min, max) = match piece {
            ArmorPiece::Helmet => ([-4.5, 23.5, -4.5], [4.5, 32.5, 4.5]),
            ArmorPiece::Chestplate => ([-4.5, 12.5, -2.5], [4.5, 24.5, 2.5]),
            ArmorPiece::Leggings => ([-4.2, 5.5, -2.2], [4.2, 14.5, 2.2]),
            ArmorPiece::Boots => ([-4.4, 0.8, -2.4], [4.4, 5.0, 2.4]),
        };
        push_cuboid(out, &cube(min, max, c, 14), &xf, rot, light, ([0.; 3], 0.), 13.0);
    }
}

#[cfg(test)]
#[path = "decor_tests.rs"]
mod tests;
