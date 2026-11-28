//! v1.0 gap track 6b: items Java 26.3 draws in 3D from a model the game's
//! block boxes can't express: the sixteen beds (`block/<colour>_bed_head`
//! with the foot model one block behind it, a `composite` item model), the
//! sixteen banners and the shield (`special` renderers, with their banner
//! patterns), the small dripleaf (`item/small_dripleaf` over
//! `block/small_dripleaf_top`) and the spore blossom (`block/spore_blossom`).
//! The game drew them as flat sprites.
//!
//! Each is a [`Mesh`]: quads in the item model's space (a block is 0..1,
//! before Java's `translate(-0.5)`) and the display transforms of its base
//! model, read from the 26.3 client's `models/`. [`Context::matrix`] is
//! Java's `ItemTransform.apply` followed by that `translate(-0.5)`, so a
//! slot, a dropped item, a framed one and the first-person hand place the
//! quads exactly as `ItemRenderer` does. Textures are the game's own (the
//! bed halves' faces, the banner's cloth and pole, planks and iron for the
//! shield) and four original layers for the two plants ([`TEX`]).

use glam::{Quat, Vec3};

use crate::color::DyeColor;
use crate::inventory::Stack;
use crate::item::Item;
use crate::world::block::{Block, tex};
use crate::world::colour_blocks::banner::{self, Banner, BannerMount, Quad};

pub use super::item_display::{BANNER, BED, BLOCK, Context, Display, SHIELD, SMALL_DRIPLEAF, Transform};

/// An item's 3D model.
#[derive(Clone, Debug)]
pub struct Mesh {
    pub quads: Vec<Quad>,
    pub display: Display,
}

impl Mesh {
    /// The quads placed for `context`, normals turned with them.
    pub fn placed(&self, context: Context) -> impl Iterator<Item = Quad> + '_ {
        let m = context.matrix(&self.display);
        self.quads.iter().map(move |q| Quad {
            corners: q.corners.map(|c| m.transform_point3(c)),
            normal: m.transform_vector3(q.normal).normalize_or_zero(),
            ..*q
        })
    }

    /// The lowest point of the placed model (`ItemStackRenderState.getModelBoundingBox`'s
    /// minimum y), which a dropped item hovers 1/16 above.
    pub fn lowest(&self, context: Context) -> f32 {
        self.placed(context).flat_map(|q| q.corners).map(|c| c.y).fold(f32::MAX, f32::min)
    }
}

/// The 3D model of `stack`, for the items listed in this module.
pub fn mesh(stack: &Stack) -> Option<Mesh> {
    if let Some(colour) = stack.item.bed_color() {
        return Some(Mesh { quads: bed(colour), display: BED });
    }
    if let Some(colour) = crate::content::banner_patterns::banner_color(stack.item) {
        let b = Banner { colour: colour as u8, mount: BannerMount::Ground(0) };
        // `BannerSpecialRenderer`: the standing banner with its flag at
        // angle 0, at the item's `transformation` (the world's 2/3 scale
        // and (0.5, 0, 0.5)), its patterns over the cloth.
        let mut quads = banner::quads(b, 0.0);
        quads.extend(banner::pattern_quads(b, 0.0, &crate::content::banner_patterns::patterns(stack).layers));
        return Some(Mesh { quads, display: BANNER });
    }
    if stack.item == Item::SHIELD {
        return Some(Mesh { quads: shield(stack), display: SHIELD });
    }
    match stack.item.block() {
        Some(b) if b == crate::world::overworld_blocks::SMALL_DRIPLEAF => {
            Some(Mesh { quads: small_dripleaf(), display: SMALL_DRIPLEAF })
        }
        Some(b) if b == crate::world::overworld_blocks::SPORE_BLOSSOM => {
            Some(Mesh { quads: spore_blossom(), display: BLOCK })
        }
        _ => None,
    }
}

/// Whether `item` has a [`mesh`] (without building it).
pub fn has_mesh(item: Item) -> bool {
    item.bed_color().is_some()
        || crate::content::banner_patterns::banner_color(item).is_some()
        || item == Item::SHIELD
        || matches!(
            item.block(),
            Some(b) if b == crate::world::overworld_blocks::SMALL_DRIPLEAF
                || b == crate::world::overworld_blocks::SPORE_BLOSSOM
        )
}

/// The six faces of the box `lo`..`hi` (model units), each with `layers`
/// (+X, -X, +Y, -Y, +Z, -Z) and UVs from the face's position within the
/// box's own block (`origin` is that block's corner), as block faces map.
fn cuboid(out: &mut Vec<Quad>, lo: Vec3, hi: Vec3, origin: Vec3, layers: [u16; 6]) {
    for (face, &layer) in layers.iter().enumerate() {
        let (d, positive) = (face / 2, face % 2 == 0);
        let (u, v) = ((d + 1) % 3, (d + 2) % 3);
        let corner = |a: bool, b: bool| {
            let mut p = [0.0f32; 3];
            p[d] = if positive { hi[d] } else { lo[d] };
            p[u] = if a { hi[u] } else { lo[u] };
            p[v] = if b { hi[v] } else { lo[v] };
            Vec3::from_array(p)
        };
        let c = if positive {
            [corner(false, false), corner(true, false), corner(true, true), corner(false, true)]
        } else {
            [corner(false, false), corner(false, true), corner(true, true), corner(true, false)]
        };
        let uv = c.map(|p| crate::mesh::face_uv(face, (p - origin).to_array()));
        let mut normal = Vec3::ZERO;
        normal[d] = if positive { 1.0 } else { -1.0 };
        out.push(Quad { corners: c, uv, layer, normal, tint: None });
    }
}

/// A bed: the head half (the game's head block, 9/16 high as it stands)
/// with the foot one block behind it (+Z), as the item model composes
/// `<colour>_bed_head` and `<colour>_bed_foot` translated by (0, 0, 1).
fn bed(colour: DyeColor) -> Vec<Quad> {
    let mut out = Vec::with_capacity(12);
    for (head, z) in [(true, 0.0), (false, 1.0)] {
        let block = Block::colored_bed(colour, head);
        let h = (16 - block.top_drop()) as f32 / 16.0;
        let origin = Vec3::new(0.0, 0.0, z);
        cuboid(&mut out, origin, Vec3::new(1.0, h, z + 1.0), origin, block.info().tex);
    }
    out
}

/// The shield (`ShieldModel` with the item's `(1, -1, -1)` scale): a
/// 12 × 22 × 1 plate of planks with an iron rim, and a 2 × 6 × 6 grip
/// behind it (the game's oak planks: Java's own shield texture is a
/// mid-brown wood, lighter than the dark oak its particle names). A patterned shield shows its base colour's cloth inside
/// the rim with the pattern layers over it, as `ShieldSpecialRenderer`
/// draws `BannerRenderer.submitPatterns` on the plate.
fn shield(stack: &Stack) -> Vec<Quad> {
    let px = |x: f32, y: f32, z: f32| Vec3::new(x, -y, -z) / 16.0;
    let planks = tex::PLANKS;
    let iron = tex::IRON_BLOCK;
    let mut out = Vec::new();
    // Java's boxes, flipped: the plate (-6, -11, -2)..(6, 11, -1) and the
    // grip (-1, -3, -1)..(1, 3, 5).
    let plate = (px(-6.0, 11.0, -1.0), px(6.0, -11.0, -2.0));
    cuboid(&mut out, plate.0, plate.1, Vec3::ZERO, [planks; 6]);
    let grip = (px(-1.0, 3.0, 5.0), px(1.0, -3.0, -1.0));
    cuboid(&mut out, grip.0, grip.1, Vec3::ZERO, [planks; 6]);
    // The front is the face at Java z = -2, model z = +2/16.
    let front = 2.0 / 16.0 + 0.0008;
    let rect = |x0: f32, y0: f32, x1: f32, y1: f32, z: f32| {
        [px(x0, y1, -z * 16.0), px(x1, y1, -z * 16.0), px(x1, y0, -z * 16.0), px(x0, y0, -z * 16.0)]
    };
    let flat = |corners: [Vec3; 4], layer: u16, uv: [[f32; 2]; 4], tint: Option<[u8; 4]>| Quad {
        corners,
        uv,
        layer,
        normal: Vec3::Z,
        tint,
    };
    let spread = |x0: f32, y0: f32, x1: f32, y1: f32| -> [[f32; 2]; 4] {
        // Texel UVs of a block layer, 16 per unit.
        [[x0 / 16.0, y1 / 16.0], [x1 / 16.0, y1 / 16.0], [x1 / 16.0, y0 / 16.0], [x0 / 16.0, y0 / 16.0]]
    };
    // The rim: a pixel of iron round the front's edge.
    for (x0, y0, x1, y1) in
        [(-6.0, -11.0, 6.0, -10.0), (-6.0, 10.0, 6.0, 11.0), (-6.0, -10.0, -5.0, 10.0), (5.0, -10.0, 6.0, 10.0)]
    {
        out.push(flat(
            rect(x0, y0, x1, y1, front),
            iron,
            spread(x0 + 8.0, y0 + 11.0, x1 + 8.0, y1 + 11.0).map(|[u, v]| [u, v.min(1.0)]),
            None,
        ));
    }
    let p = crate::content::banner_patterns::patterns(stack);
    if p.base.is_some() || !p.layers.is_empty() {
        let face = 0.0016;
        let base = p.base.unwrap_or(DyeColor::White);
        let cloth = crate::world::colour_blocks::tex::CLOTH + base as u16;
        out.push(flat(rect(-5.0, -10.0, 5.0, 10.0, front + face), cloth, spread(3.0, 0.0, 13.0, 16.0), None));
        for (k, l) in p.layers.iter().enumerate() {
            let z = front + face * (k + 2) as f32;
            let [r, g, b] = l.color.rgb();
            for half in 0..2u8 {
                let (y0, y1) = if half == 0 { (-10.0, 0.0) } else { (0.0, 10.0) };
                out.push(flat(
                    rect(-5.0, y0, 5.0, y1, z),
                    crate::world::workstations::pattern_layer(l.pattern, half),
                    [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
                    Some([r, g, b, 255]),
                ));
            }
        }
    }
    out
}

/// One face of `block/small_dripleaf_top` or `block/spore_blossom`: the
/// element `lo`..`hi` (sixteenths; flat along one axis) turned `angle`
/// degrees about `axis` (0 x, 1 y, 2 z) through `origin`, both sides.
fn plane(out: &mut Vec<Quad>, lo: [f32; 3], hi: [f32; 3], rotation: Option<([f32; 3], usize, f32)>, layer: u16) {
    let (lo, hi) = (Vec3::from_array(lo) / 16.0, Vec3::from_array(hi) / 16.0);
    let flat = (0..3).find(|&d| lo[d] == hi[d]).unwrap_or(1);
    let (u, v) = ((flat + 1) % 3, (flat + 2) % 3);
    let corner = |a: bool, b: bool| {
        let mut p = lo;
        p[u] = if a { hi[u] } else { lo[u] };
        p[v] = if b { hi[v] } else { lo[v] };
        p
    };
    let corners = [corner(false, false), corner(true, false), corner(true, true), corner(false, true)];
    let turn = |p: Vec3, vector: bool| match rotation {
        Some((origin, axis, angle)) => {
            let q = Quat::from_axis_angle(Vec3::AXES[axis], angle.to_radians());
            if vector { q * p } else { q * (p - Vec3::from_array(origin) / 16.0) + Vec3::from_array(origin) / 16.0 }
        }
        None => p,
    };
    let mut normal = Vec3::ZERO;
    normal[flat] = 1.0;
    let uv = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let corners = corners.map(|c| turn(c, false));
    let normal = turn(normal, true);
    out.push(Quad { corners, uv, layer, normal, tint: None });
    let back = [corners[3], corners[2], corners[1], corners[0]];
    out.push(Quad { corners: back, uv: [uv[3], uv[2], uv[1], uv[0]], layer, normal: -normal, tint: None });
}

/// A thin box's four sides (no top or bottom), `layer` on each.
fn rim(out: &mut Vec<Quad>, lo: [f32; 3], hi: [f32; 3], layer: u16) {
    let mut faces = Vec::new();
    cuboid(&mut faces, Vec3::from_array(lo) / 16.0, Vec3::from_array(hi) / 16.0, Vec3::ZERO, [layer; 6]);
    // Faces 2 and 3 are the top and bottom.
    out.extend(faces.into_iter().enumerate().filter(|&(i, _)| i / 2 != 1).map(|(_, q)| Quad {
        // A 1/16-high side shows a strip of the leaf's edge.
        uv: [[0.0, 0.5], [1.0, 0.5], [1.0, 0.5625], [0.0, 0.5625]],
        ..q
    }));
}

/// `block/small_dripleaf_top`: three leaves at heights 3, 8 and 12 on two
/// crossed stems.
fn small_dripleaf() -> Vec<Quad> {
    let mut out = Vec::new();
    for (lo, hi) in [
        ([8.0, 3.0, 8.0], [15.0, 3.0, 15.0]),
        ([1.0, 8.02, 1.0], [8.0, 8.02, 8.0]),
        ([1.0, 12.02, 8.0], [8.0, 12.02, 15.0]),
    ] {
        plane(&mut out, lo, hi, None, DRIPLEAF_LEAF);
        rim(&mut out, [lo[0], lo[1] - 1.0, lo[2]], hi, DRIPLEAF_LEAF);
    }
    for angle in [45.0, -45.0] {
        plane(&mut out, [4.5, 0.0, 8.0], [11.5, 14.0, 8.0], Some(([8.0, 8.0, 8.0], 1, angle)), DRIPLEAF_STEM);
    }
    out
}

/// A spore blossom petal element: from, to, its rotation's origin, axis and angle.
type Petal = ([f32; 3], [f32; 3], [f32; 3], usize, f32);

/// `block/spore_blossom`: the base under the ceiling and four petals
/// hanging from it at 22.5 degrees.
fn spore_blossom() -> Vec<Quad> {
    let mut out = Vec::new();
    plane(&mut out, [1.0, 15.9, 1.0], [15.0, 15.9, 15.0], None, BLOSSOM_BASE);
    let petals: [Petal; 4] = [
        ([8.0, 15.7, 0.0], [24.0, 15.7, 16.0], [8.0, 16.0, 0.0], 2, -22.5),
        ([-8.0, 15.7, 0.0], [8.0, 15.7, 16.0], [8.0, 16.0, 0.0], 2, 22.5),
        ([0.0, 15.7, 8.0], [16.0, 15.7, 24.0], [0.0, 16.0, 8.0], 0, 22.5),
        ([0.0, 15.7, -8.0], [16.0, 15.7, 8.0], [0.0, 16.0, 8.0], 0, -22.5),
    ];
    for (lo, hi, origin, axis, angle) in petals {
        plane(&mut out, lo, hi, Some((origin, axis, angle)), BLOSSOM_PETAL);
    }
    out
}

/// v1.0 gap track 6b's block texture layers (Track 6's reservation,
/// 4080–4087): the spore blossom's petal and base and the small dripleaf's
/// leaf and stems, for the item models only (the placed plants keep their
/// crossed sprites).
pub const TEX: u16 = 4080;
pub const BLOSSOM_PETAL: u16 = TEX;
pub const BLOSSOM_BASE: u16 = TEX + 1;
pub const DRIPLEAF_LEAF: u16 = TEX + 2;
pub const DRIPLEAF_STEM: u16 = TEX + 3;
pub const TEX_END: u16 = TEX + 4;

/// Original 16 × 16 art for [`TEX`]'s layers: light from the top left as
/// the game's plants are.
pub fn pixel(layer: u16, x: usize, y: usize) -> [u8; 4] {
    let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
    let shade = |c: [u8; 3], f: f32| c.map(|v| (v as f32 * f).clamp(0.0, 255.0) as u8);
    let rgba = |c: [u8; 3]| [c[0], c[1], c[2], 255];
    let grain = crate::world::noise::hash_f(x as i32, y as i32, layer as i32, 0x6B) * 0.12 + 0.94;
    match layer {
        BLOSSOM_PETAL => {
            // A broad petal from the hinge (the top edge, where the plane
            // meets the base) out to a rounded tip, pink with a deeper vein.
            let (dx, t) = ((fx - 8.0).abs(), fy / 16.0);
            let half = 2.0 + 5.0 * (std::f32::consts::PI * t).sin().max(0.0).powf(0.7);
            if dx > half || t > 0.97 {
                return [0; 4];
            }
            let vein = dx < 0.8;
            let c = if vein {
                [196, 70, 132]
            } else if dx > half - 1.0 {
                [214, 96, 158]
            } else {
                [236, 132, 186]
            };
            rgba(shade(c, grain * (1.08 - t * 0.2)))
        }
        BLOSSOM_BASE => {
            // A round green pad with a pink heart.
            let r = ((fx - 8.0).powi(2) + (fy - 8.0).powi(2)).sqrt();
            if r > 7.2 {
                return [0; 4];
            }
            let c = if r < 2.5 {
                [226, 116, 172]
            } else if r > 6.2 {
                [62, 104, 44]
            } else {
                [86, 136, 58]
            };
            rgba(shade(c, grain))
        }
        DRIPLEAF_LEAF => {
            // A rounded leaf, veins fanning from its stem corner.
            let r = ((fx - 8.0).powi(2) + (fy - 8.0).powi(2)).sqrt();
            if r > 7.8 {
                return [0; 4];
            }
            let vein = (x == y && x < 12) || (x == 7 && y > 4) || (y == 7 && x > 4);
            let edge = r > 6.8;
            let c = if vein {
                [74, 124, 40]
            } else if edge {
                [86, 140, 48]
            } else {
                [112, 170, 62]
            };
            rgba(shade(c, grain * (1.06 - (fx + fy) / 64.0)))
        }
        DRIPLEAF_STEM => {
            // Two thin stems rising up the middle of the plane.
            let stem = (x == 7 || x == 8) && y >= 1;
            if !stem {
                return [0; 4];
            }
            rgba(shade([84, 138, 52], grain * if x == 7 { 1.08 } else { 0.86 }))
        }
        _ => [0; 4],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn javas_display_transforms_place_a_block_where_block_block_does() {
        // `block/block`'s GUI transform turns a unit cube into the slot's
        // isometric cube: 0.625 across, its centre at the slot's centre.
        let m = Context::Gui.matrix(&BLOCK);
        assert!(m.transform_point3(Vec3::splat(0.5)).abs_diff_eq(Vec3::ZERO, 1e-6));
        let top = m.transform_vector3(Vec3::Y);
        assert!((top.length() - 0.625).abs() < 1e-5);
    }

    #[test]
    fn every_listed_item_has_a_mesh_that_fits_its_slot() {
        let items = [
            DyeColor::White.bed(),
            DyeColor::Red.bed(),
            Item::SHIELD,
            Item::from_block(crate::world::overworld_blocks::SMALL_DRIPLEAF),
            Item::from_block(crate::world::overworld_blocks::SPORE_BLOSSOM),
        ]
        .into_iter()
        .chain(DyeColor::ALL.map(crate::content::banner_patterns::banner_item));
        for item in items {
            let mesh = mesh(&Stack::new(item, 1)).unwrap_or_else(|| panic!("{item:?} has no mesh"));
            assert!(!mesh.quads.is_empty());
            for q in mesh.placed(Context::Gui) {
                for c in q.corners {
                    // About the slot (Java's GUI space is -0.5..0.5; the
                    // spore blossom's petals reach past it, as in Java).
                    assert!(c.x.abs() <= 0.7 && c.y.abs() <= 0.7, "{item:?} {c}");
                }
            }
            // A dropped one sits on its lowest point, within its ground model.
            assert!(mesh.lowest(Context::Ground).abs() < 0.5, "{item:?}");
        }
        assert!(mesh(&Stack::new(Item::STICK, 1)).is_none());
    }

    #[test]
    fn plant_layers_are_drawn() {
        for layer in TEX..TEX_END {
            let solid = (0..256).filter(|&i| pixel(layer, i % 16, i / 16)[3] > 0).count();
            assert!(solid > 16, "{layer}");
        }
    }
}
