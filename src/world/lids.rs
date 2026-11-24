//! v0.18 Track 9: chest lids and barrel tops (Java 26.3
//! `ChestLidController`, `ContainerOpenersCounter`, `ChestBlockEntity`,
//! `EnderChestBlockEntity` and `BarrelBlockEntity`).
//!
//! Chests, trapped chests, copper chests and ender chests swing their lid
//! open by 0.1 a tick while anyone has them open and shut it by 0.1 a tick
//! after the last viewer leaves; the renderer eases the angle with
//! `1 - (1 - openness)^3`. A barrel shows its open top while in use. The
//! first viewer arriving and the last leaving play the container's open and
//! close sounds (volume 0.5, pitch 0.9 to 1.0, `BLOCKS`): a chest at its
//! centre, a double chest once between its halves, a barrel at its open
//! face. Shulker boxes keep their own lids (`shulker_box`).
//!
//! Only containers in use or still moving are tracked, so closed ones cost
//! nothing per tick or per frame. A container in this table is left out of
//! its chunk mesh and drawn as a block-entity model instead; entering and
//! leaving the table remeshes that chunk once. Lids aren't saved: like
//! Java's, every container loads shut.

use glam::{DVec3, IVec3, Vec3};
use rustc_hash::{FxHashMap, FxHashSet};

use super::World;
use super::block::{Block, Facing};
use super::copper::Stage;
use super::double_chest::{ChestType, Kind};

/// The barrel's open top (texture layer 1290, Track 9's reservation
/// 1290–1294; the other four stay free).
pub const BARREL_OPEN_TEX: u16 = 1290;

/// `ChestLidController.tickLid`'s step per tick.
pub const LID_STEP: f32 = 0.1;

/// `ChestLidController`: how far open a lid is now and a tick ago.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Lid {
    pub should_be_open: bool,
    pub openness: f32,
    pub o_openness: f32,
}

impl Lid {
    /// `ChestLidController.tickLid`.
    pub fn tick(&mut self) {
        self.o_openness = self.openness;
        if !self.should_be_open && self.openness > 0.0 {
            self.openness = (self.openness - LID_STEP).max(0.0);
        } else if self.should_be_open && self.openness < 1.0 {
            self.openness = (self.openness + LID_STEP).min(1.0);
        }
    }

    /// `getOpenness(partialTick)`.
    pub fn at(&self, alpha: f32) -> f32 {
        self.o_openness + (self.openness - self.o_openness) * alpha.clamp(0.0, 1.0)
    }

    /// Shut and still: nothing left to animate.
    fn settled(&self) -> bool {
        !self.should_be_open && self.openness == 0.0 && self.o_openness == 0.0
    }
}

/// `ChestRenderer.submit`: the lid's angle eases out, `1 - (1 - t)^3`.
pub fn eased(openness: f32) -> f32 {
    let closed = 1.0 - openness;
    1.0 - closed * closed * closed
}

/// What kind of container a lid or sound belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Container {
    /// Chests and trapped chests (`SoundEvents.CHEST_OPEN`/`CLOSE`).
    Chest,
    /// A copper chest at its stage (`block.copper_chest*.open`/`close`).
    CopperChest(Stage),
    EnderChest,
    Barrel,
}

impl Container {
    pub fn of(b: Block) -> Option<Container> {
        if let Some(s) = super::double_chest::state(b) {
            return Some(match s.kind {
                Kind::Plain | Kind::Trapped => Container::Chest,
                Kind::Copper(stage) | Kind::WaxedCopper(stage) => Container::CopperChest(stage),
            });
        }
        if super::utilities::ender_chest_of(b).is_some() {
            return Some(Container::EnderChest);
        }
        (b.base() == Block::BARREL).then_some(Container::Barrel)
    }

    /// Whether the container has a lid that swings (all but barrels).
    pub fn has_lid(self) -> bool {
        self != Container::Barrel
    }
}

/// An open or close sound to play.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Cue {
    pub pos: DVec3,
    pub container: Container,
    pub open: bool,
}

/// The lids in use or moving.
#[derive(Default)]
pub struct Lids {
    lids: FxHashMap<IVec3, Lid>,
    /// Cells with viewers last tick.
    open: FxHashSet<IVec3>,
    /// Sounds since the game last asked.
    pub cues: Vec<Cue>,
}

/// Facing of a barrel's open top.
fn barrel_facing(b: Block) -> Option<IVec3> {
    match b {
        super::village_blocks::BARREL_UP => Some(IVec3::Y),
        super::village_blocks::BARREL_DOWN => Some(IVec3::NEG_Y),
        b => super::village_blocks::oriented(b.0).filter(|(base, _)| *base == Block::BARREL).map(|(_, f)| f.offset()),
    }
}

impl World {
    /// Who has which container open this tick, from every input path,
    /// players and copper golems alike: lids start opening or closing and
    /// the first viewer's arrival and the last one's leaving sound.
    pub fn set_container_viewers(&mut self, open: &[IVec3]) {
        let mut now: FxHashSet<IVec3> = FxHashSet::default();
        let golems = self.golem_chest_cells();
        for &p in open.iter().chain(&golems) {
            let Some(container) = self.get_block(p).and_then(Container::of) else { continue };
            if matches!(container, Container::Chest | Container::CopperChest(_)) {
                let (first, second) = self.chest_halves(p);
                now.extend(std::iter::once(first).chain(second));
            } else {
                now.insert(p);
            }
        }
        if now == self.lids.open {
            return;
        }
        let old = std::mem::replace(&mut self.lids.open, now);
        let opened: Vec<IVec3> = self.lids.open.difference(&old).copied().collect();
        let closed: Vec<IVec3> = old.difference(&self.lids.open).copied().collect();
        for (cells, open) in [(opened, true), (closed, false)] {
            for p in cells {
                let Some(block) = self.get_block(p) else { continue };
                let Some(container) = Container::of(block) else { continue };
                if let Some(pos) = sound_position(block, p)
                    && self.lids.cues.len() < 64
                {
                    self.lids.cues.push(Cue { pos, container, open });
                }
                let entered = !self.lids.lids.contains_key(&p);
                self.lids.lids.entry(p).or_default().should_be_open = open;
                if entered {
                    self.lid_remesh(p);
                }
            }
        }
    }

    /// Moves every tracked lid one tick (`lidAnimateTick`); shut lids and
    /// containers that are gone are forgotten and drawn in their chunk again.
    pub fn tick_lids(&mut self) {
        if self.lids.lids.is_empty() {
            return;
        }
        for lid in self.lids.lids.values_mut() {
            lid.tick();
        }
        let gone: Vec<IVec3> = self
            .lids
            .lids
            .iter()
            .filter(|(p, l)| l.settled() || !self.get_block(**p).and_then(Container::of).is_some())
            .map(|(p, _)| *p)
            .collect();
        for p in gone {
            self.lids.lids.remove(&p);
            self.lid_remesh(p);
        }
    }

    /// Remeshes the chunk holding `p` now (a lid entering or leaving the
    /// block-entity models), as a block edit would.
    fn lid_remesh(&mut self, p: IVec3) {
        self.remesh_geometry(p);
    }

    /// The lid at `p` between the last two ticks (0 shut).
    pub fn chest_lid(&self, p: IVec3, alpha: f32) -> f32 {
        self.lids.lids.get(&p).map_or(0.0, |l| l.at(alpha))
    }

    /// Whether the barrel at `p` shows its open top (Java's `open` state).
    pub fn barrel_open(&self, p: IVec3) -> bool {
        self.lids.open.contains(&p)
    }

    /// Containers drawn as models instead of in their chunk mesh: in use or
    /// with a lid still moving.
    pub fn lidded(&self) -> impl Iterator<Item = (IVec3, Lid)> + '_ {
        self.lids.lids.iter().map(|(&p, &l)| (p, l))
    }

    /// Cells of the chunk at `cpos` its mesh leaves out (see [`World::lidded`]).
    pub(crate) fn lidded_in_chunk(&self, cpos: IVec3) -> Vec<IVec3> {
        if self.lids.lids.is_empty() {
            return Vec::new();
        }
        self.lids.lids.keys().filter(|p| super::chunk::chunk_of(**p) == cpos).copied().collect()
    }

    /// Lid sounds since the last call.
    pub fn take_lid_cues(&mut self) -> Vec<Cue> {
        std::mem::take(&mut self.lids.cues)
    }

    /// Lids within `radius` of `at`, for a LAN client: `[x, y, z, open,
    /// openness a tick ago, openness]`.
    pub fn lids_near(&self, at: DVec3, radius: f64) -> Vec<serde_json::Value> {
        self.lids
            .lids
            .iter()
            .filter(|(p, _)| (p.as_dvec3() + DVec3::splat(0.5)).distance_squared(at) <= radius * radius)
            .take(256)
            .map(|(p, l)| serde_json::json!([p.x, p.y, p.z, l.should_be_open, l.o_openness, l.openness]))
            .collect()
    }

    /// Applies [`World::lids_near`] on a LAN client, remeshing the chunks
    /// whose containers entered or left the models.
    pub fn apply_lids(&mut self, list: &serde_json::Value) {
        let Some(list) = list.as_array() else { return };
        let mut lids: FxHashMap<IVec3, Lid> = FxHashMap::default();
        for v in list {
            let Some(a) = v.as_array() else { continue };
            let n = |i: usize| a.get(i).and_then(serde_json::Value::as_f64);
            let (Some(x), Some(y), Some(z), Some(old), Some(now)) = (n(0), n(1), n(2), n(4), n(5)) else { continue };
            let open = a.get(3).and_then(serde_json::Value::as_bool).unwrap_or(false);
            lids.insert(
                IVec3::new(x as i32, y as i32, z as i32),
                Lid {
                    should_be_open: open,
                    openness: now.clamp(0.0, 1.0) as f32,
                    o_openness: old.clamp(0.0, 1.0) as f32,
                },
            );
        }
        let changed: Vec<IVec3> = lids
            .keys()
            .filter(|p| !self.lids.lids.contains_key(p))
            .chain(self.lids.lids.keys().filter(|p| !lids.contains_key(p)))
            .copied()
            .collect();
        self.lids.open = lids.iter().filter(|(_, l)| l.should_be_open).map(|(p, _)| *p).collect();
        self.lids.lids = lids;
        for p in changed {
            self.lid_remesh(p);
        }
    }
}

/// Where the open or close sound plays (`ChestBlockEntity.playSound`,
/// `EnderChestBlockEntity`, `BarrelBlockEntity.playSound`): `None` for a
/// double chest's left half, which leaves the sound to its right half.
fn sound_position(b: Block, p: IVec3) -> Option<DVec3> {
    let centre = p.as_dvec3() + DVec3::splat(0.5);
    if let Some(s) = super::double_chest::state(b) {
        return match s.ty {
            ChestType::Single => Some(centre),
            ChestType::Left => None,
            ChestType::Right => {
                Some(centre + super::double_chest::connected(s.facing, ChestType::Right).offset().as_dvec3() * 0.5)
            }
        };
    }
    if let Some(f) = barrel_facing(b) {
        return Some(centre + f.as_dvec3() * 0.5);
    }
    Some(centre)
}

/// A model's box in pixels: lowest and highest corners.
pub type Bounds = ([u8; 3], [u8; 3]);

/// The facing and shape (pixels) of a chest, trapped, copper or ender
/// chest drawn as a model.
pub fn chest_model(b: Block) -> Option<(Facing, Bounds)> {
    if let Some(s) = super::double_chest::state(b) {
        let bx = super::double_chest::shape(b)?;
        return Some((s.facing, (bx.min, bx.max)));
    }
    // `EnderChestBlock.SHAPE`: `Block.column(14, 0, 14)`.
    super::utilities::ender_chest_of(b).map(|(f, _)| (f, ([1, 0, 1], [15, 14, 15])))
}

/// A barrel's face textures with its open end showing the open top.
pub fn barrel_faces(b: Block) -> Option<[u16; 6]> {
    let f = barrel_facing(b)?;
    let face = match (f.x, f.y, f.z) {
        (1, _, _) => 0,
        (-1, _, _) => 1,
        (_, 1, _) => 2,
        (_, -1, _) => 3,
        (_, _, 1) => 4,
        _ => 5,
    };
    let mut tex = b.info().tex;
    tex[face] = BARREL_OPEN_TEX;
    Some(tex)
}

/// One textured quad of a chest model, in the cell's 0..1 coordinates.
pub type Quad = super::colour_blocks::banner::Quad;

/// `ChestRenderer`'s chest at `openness` (0 shut, 1 open; eased here):
/// the body and the lid, which turns up about its back edge 9 pixels up,
/// drawn with the block's own face textures. `faces` are the block's
/// texture layers (+X, -X, +Y, -Y, +Z, -Z) and `bounds` its shape in pixels.
pub fn chest_quads(facing: Facing, bounds: Bounds, faces: [u16; 6], openness: f32, mut emit: impl FnMut(Quad)) {
    let angle = eased(openness) * std::f32::consts::FRAC_PI_2;
    let back = -facing.offset().as_vec3();
    // The hinge: the back edge of the lid's underside, 9 pixels up.
    let hinge_y = 9.0 / 16.0;
    let (lo, hi) = (bounds.0.map(|c| c as f32 / 16.0), bounds.1.map(|c| c as f32 / 16.0));
    let hinge_back = if back.x > 0.0 {
        hi[0]
    } else if back.x < 0.0 {
        lo[0]
    } else if back.z > 0.0 {
        hi[2]
    } else {
        lo[2]
    };
    // The lid turns about the horizontal axis along the back edge, opening
    // away from the front: a point's front-to-back offset rises.
    let axis = Vec3::Y.cross(back).normalize();
    let rotation = glam::Quat::from_axis_angle(axis, -angle);
    let pivot = {
        let mut p = Vec3::new(0.5, hinge_y, 0.5);
        if back.x != 0.0 {
            p.x = hinge_back;
        } else {
            p.z = hinge_back;
        }
        p
    };
    let parts: [(f32, f32, bool); 2] = [(lo[1], 10.0 / 16.0, false), (hinge_y, hi[1], true)];
    for (y0, y1, lid) in parts {
        let (a, b) = (Vec3::new(lo[0], y0, lo[2]), Vec3::new(hi[0], y1, hi[2]));
        for (face, &layer) in faces.iter().enumerate() {
            let (d, positive) = (face / 2, face % 2 == 0);
            // The body's top and the lid's underside are hidden by each other.
            if d == 1 && ((positive && !lid) || (!positive && lid)) && openness <= 0.0 {
                continue;
            }
            let (u, v) = ((d + 1) % 3, (d + 2) % 3);
            let corner = |s: bool, t: bool| {
                let mut p = Vec3::ZERO;
                p[d] = if positive { b[d] } else { a[d] };
                p[u] = if s { b[u] } else { a[u] };
                p[v] = if t { b[v] } else { a[v] };
                p
            };
            let model = if positive {
                [corner(false, false), corner(true, false), corner(true, true), corner(false, true)]
            } else {
                [corner(false, false), corner(false, true), corner(true, true), corner(true, false)]
            };
            // UVs as the chunk mesh maps them: sides (horizontal, down), tops (x, z).
            let uv = model.map(|p| match d {
                0 => [p.z, 1.0 - p.y],
                1 => [p.x, p.z],
                _ => [p.x, 1.0 - p.y],
            });
            let mut normal = Vec3::ZERO;
            normal[d] = if positive { 1.0 } else { -1.0 };
            let (corners, normal) =
                if lid { (model.map(|p| pivot + rotation * (p - pivot)), rotation * normal) } else { (model, normal) };
            emit(Quad { corners, uv, layer, normal, tint: None });
        }
    }
}

#[cfg(test)]
#[path = "lids/tests.rs"]
mod tests;
