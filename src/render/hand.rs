//! The first-person hand: the bare arm, a held block turned 45 degrees, or
//! a held item as an extruded sprite, placed with Minecraft's first-person
//! transforms and swing, equip and walking-bob animations.
//!
//! Vertices are built in Minecraft's view frame (x right, y up, -z ahead,
//! in blocks), squeezed sideways so the hand always looks as if drawn with
//! a 70 degree field of view, then turned to the camera. The block-model
//! pass draws them with depth pulled close to the eye, so the hand never
//! sinks into walls.

use std::f32::consts::PI;

use glam::{EulerRot, Mat4, Quat, Vec3};
use rustc_hash::FxHashMap;

use super::block_model::BlockVertex;
use crate::item::Item;
use crate::world::block::tex;
use crate::world::shape;

/// Everything about the hand this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Hand {
    /// What's held (`None`: the bare arm).
    pub item: Option<Item>,
    /// Swing progress, 0..1 (0 when not swinging).
    pub swing: f32,
    /// 0 when raised, 1 when lowered out of view (switching items).
    pub equip: f32,
    /// Eating: 0..1 while chewing, moving the food to the mouth.
    pub eating: f32,
    pub sky_light: f32,
    pub block_light: f32,
}

/// Minecraft draws the hand with this field of view, whatever the setting.
const HAND_FOV: f32 = 70.0 * PI / 180.0;
/// Views narrower than this (side-by-side split-screen) pull the hand in
/// so it stays on screen; wider ones use Minecraft's placement unchanged.
const WIDE_ASPECT: f32 = 16.0 / 9.0;
/// The skin box of the right arm, in 1/16 block, and its pivot.
const ARM_MIN: Vec3 = Vec3::new(-3.0, -2.0, -2.0);
const ARM_MAX: Vec3 = Vec3::new(1.0, 10.0, 2.0);
const ARM_PIVOT: Vec3 = Vec3::new(-5.0, 2.0, 0.0);

/// Which pixels of each flat icon are solid, by texture layer (one bit per
/// pixel, a row per entry), cached as icons come into the hand.
#[derive(Default)]
pub(super) struct SpriteMasks(FxHashMap<u16, [u16; 16]>);

impl SpriteMasks {
    fn get(&mut self, layer: u16) -> [u16; 16] {
        *self.0.entry(layer).or_insert_with(|| {
            std::array::from_fn(|y| {
                (0..16).fold(0u16, |row, x| row | ((super::textures::texel(layer, x, y)[3] >= 128) as u16) << x)
            })
        })
    }
}

fn deg(d: f32) -> f32 {
    d * PI / 180.0
}

fn rot_x(d: f32) -> Mat4 {
    Mat4::from_rotation_x(deg(d))
}

fn rot_y(d: f32) -> Mat4 {
    Mat4::from_rotation_y(deg(d))
}

fn rot_z(d: f32) -> Mat4 {
    Mat4::from_rotation_z(deg(d))
}

fn translate(x: f32, y: f32, z: f32) -> Mat4 {
    Mat4::from_translation(Vec3::new(x, y, z))
}

/// Shading for a face whose normal points along `n` in the view frame:
/// lit from above like terrain faces.
fn shade(n: Vec3) -> f32 {
    let n = n.normalize_or_zero();
    0.8 * n.x * n.x + 0.68 * n.z * n.z + if n.y > 0.0 { 1.0 } else { 0.55 } * n.y * n.y
}

struct Builder<'a> {
    out: &'a mut Vec<BlockVertex>,
    /// View frame -> camera-relative world.
    to_world: Mat4,
    light: (f32, f32),
}

impl Builder<'_> {
    /// One quad; `corners` in the view frame, counter-clockwise from outside.
    fn quad(&mut self, corners: [Vec3; 4], uv: [[f32; 2]; 4], layer: u16) {
        let n = (corners[1] - corners[0]).cross(corners[2] - corners[0]);
        let s = shade(n);
        for i in [0, 1, 2, 2, 3, 0] {
            self.out.push(BlockVertex {
                pos: self.to_world.transform_point3(corners[i]).to_array(),
                uv: uv[i],
                layer: layer as u32,
                light: [self.light.0, s, self.light.1],
            });
        }
    }

    /// A box from `lo` to `hi` in model space; `tex` per face (+X, -X, +Y,
    /// -Y, +Z, -Z) and UVs from the model position like block faces.
    fn cuboid(&mut self, m: Mat4, lo: Vec3, hi: Vec3, tex: [u16; 6]) {
        for (face, &layer) in tex.iter().enumerate() {
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
            // As in `block_model`: sides map (horizontal, down), tops (x, z).
            let uv = c.map(|p| match d {
                0 => [p.z, 1.0 - p.y],
                1 => [p.x, p.z],
                _ => [p.x, 1.0 - p.y],
            });
            self.quad(c.map(|p| m.transform_point3(p)), uv, layer);
        }
    }

    /// A flat icon extruded 1/16 deep: front and back faces, plus a strip
    /// along every pixel edge between solid and clear.
    fn sprite(&mut self, m: Mat4, layer: u16, mask: [u16; 16]) {
        let solid = |x: i32, y: i32| (0..16).contains(&x) && (0..16).contains(&y) && mask[y as usize] >> x & 1 == 1;
        let p = |x: f32, y: f32, z: f32| m.transform_point3(Vec3::new(x / 16.0, 1.0 - y / 16.0, z / 16.0));
        let full = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
        self.quad([p(0., 16., 1.), p(16., 16., 1.), p(16., 0., 1.), p(0., 0., 1.)], full, layer);
        let back = [[1.0, 1.0], [0.0, 1.0], [0.0, 0.0], [1.0, 0.0]];
        self.quad([p(16., 16., 0.), p(0., 16., 0.), p(0., 0., 0.), p(16., 0., 0.)], back, layer);
        for y in 0..16 {
            for x in 0..16 {
                if !solid(x, y) {
                    continue;
                }
                let uv = [[(x as f32 + 0.5) / 16.0, (y as f32 + 0.5) / 16.0]; 4];
                let (fx, fy) = (x as f32, y as f32);
                if !solid(x - 1, y) {
                    self.quad([p(fx, fy, 0.), p(fx, fy + 1., 0.), p(fx, fy + 1., 1.), p(fx, fy, 1.)], uv, layer);
                }
                if !solid(x + 1, y) {
                    let x1 = fx + 1.0;
                    self.quad([p(x1, fy, 1.), p(x1, fy + 1., 1.), p(x1, fy + 1., 0.), p(x1, fy, 0.)], uv, layer);
                }
                if !solid(x, y - 1) {
                    self.quad([p(fx, fy, 1.), p(fx + 1., fy, 1.), p(fx + 1., fy, 0.), p(fx, fy, 0.)], uv, layer);
                }
                if !solid(x, y + 1) {
                    let y1 = fy + 1.0;
                    self.quad([p(fx, y1, 0.), p(fx + 1., y1, 0.), p(fx + 1., y1, 1.), p(fx, y1, 1.)], uv, layer);
                }
            }
        }
    }
}

/// The hand's triangles, camera-relative, for a camera looking along
/// `forward` with vertical field of view `fov_y` in a view `aspect` wide.
pub(super) fn vertices(
    hand: &Hand,
    forward: Vec3,
    fov_y: f32,
    aspect: f32,
    view_effect: Mat4,
    masks: &mut SpriteMasks,
) -> Vec<BlockVertex> {
    let mut out = Vec::new();
    // View frame -> camera-relative world, squeezed to the hand's FOV.
    let f = forward.normalize();
    let r = f.cross(Vec3::Y).normalize_or(Vec3::X);
    let u = r.cross(f);
    let k = (fov_y / 2.0).tan() / (HAND_FOV / 2.0).tan();
    let to_world = Mat4::from_cols(r.extend(0.0), u.extend(0.0), (-f).extend(0.0), Vec3::ZERO.extend(1.0))
        // The world projection already applies view_effect. Conjugate it
        // around the FOV correction so the hand sees the identical Java bob
        // in its own fixed 70-degree projection, exactly once.
        * view_effect.inverse()
        * Mat4::from_scale(Vec3::new(k, k, 1.0))
        * view_effect;
    let mut b = Builder { out: &mut out, to_world, light: (hand.sky_light, hand.block_light) };

    let s = hand.swing;
    let root = s.sqrt();
    // Narrow views slide the hand toward the centre without squashing it.
    let inward = 0.56 * (1.0 - (aspect / WIDE_ASPECT).clamp(0.0, 1.0));
    let bob = translate(-inward, 0.0, 0.0);

    let Some(item) = hand.item else {
        // The bare arm (Minecraft's `renderPlayerArm`).
        let m =
            bob * translate(
                -0.3 * (root * PI).sin() + 0.64,
                0.4 * (root * 2.0 * PI).sin() - 0.6 - hand.equip * 0.6,
                -0.4 * (s * PI).sin() - 0.72,
            ) * rot_y(45.0)
                * rot_y((root * PI).sin() * 70.0)
                * rot_z((s * s * PI).sin() * -20.0)
                * translate(-1.0, 3.6, 3.5)
                * rot_z(120.0)
                * rot_x(200.0)
                * rot_y(-135.0)
                * translate(5.6, 0.0, 0.0)
                * translate(ARM_PIVOT.x / 16.0, ARM_PIVOT.y / 16.0, ARM_PIVOT.z / 16.0);
        b.cuboid(m, ARM_MIN / 16.0, ARM_MAX / 16.0, [tex::SKIN; 6]);
        return out;
    };

    // Food heads for the mouth and bobs while chewing.
    let e = hand.eating.clamp(0.0, 1.0);
    let chew = if e > 0.0 { (e * 24.0).cos().abs() * 0.08 } else { 0.0 };
    let eat = translate(-0.4 * e.min(0.25) * 4.0 * 0.6, chew + e.min(0.25) * 4.0 * 0.1, 0.0)
        * rot_y(e.min(0.25) * 4.0 * 60.0);

    // Minecraft's `renderArmWithItem`: swing offset, arm placement, swing turn.
    let arm = bob
        * eat
        * translate(-0.4 * (root * PI).sin(), 0.2 * (root * 2.0 * PI).sin(), -0.2 * (s * PI).sin())
        * translate(0.56, -0.52 - hand.equip * 0.6, -0.72)
        * rot_y(45.0 + (s * s * PI).sin() * -20.0)
        * rot_z((root * PI).sin() * -20.0)
        * rot_x((root * PI).sin() * -80.0)
        * rot_y(-45.0);

    let block = item.block().filter(|b| !b.flat_icon());
    match block {
        Some(block) => {
            // Block models: turned 45 degrees and shrunk to 0.4, centred.
            let m = arm * rot_y(45.0) * Mat4::from_scale(Vec3::splat(0.4)) * translate(-0.5, -0.5, -0.5);
            let tex = block.info().tex;
            let mut boxes = shape::item_shape(block);
            if boxes.is_empty() {
                let top = 16 - block.top_drop();
                boxes = shape::Boxes::from_box(shape::Box16 { min: [0; 3], max: [16, top, 16] });
            }
            for bx in boxes.as_slice() {
                let (lo, hi) = (Vec3::from_array(bx.min.map(f32::from)), Vec3::from_array(bx.max.map(f32::from)));
                b.cuboid(m, lo / 16.0, hi / 16.0, tex);
            }
        }
        None => {
            // Flat items: Minecraft's first-person "handheld" transform.
            let layer = match item.block() {
                Some(block) => block.info().tex[0],
                None => item.icon_layer().unwrap_or(tex::ITEM_BASE),
            };
            // Mirrored front to back, so the icon's right (a sword's tip)
            // points away from the eye and the handle sits in the hand.
            let display = translate(1.13 / 16.0, 3.2 / 16.0, 1.13 / 16.0)
                * Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0))
                * Mat4::from_quat(Quat::from_euler(EulerRot::XYZ, 0.0, deg(-90.0), deg(25.0)))
                * Mat4::from_scale(Vec3::splat(0.68))
                * translate(-0.5, -0.5, -0.5 / 16.0);
            b.sprite(arm * display, layer, masks.get(layer));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::Block;

    fn bounds(v: &[BlockVertex]) -> (Vec3, Vec3) {
        v.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), v| {
            let p = Vec3::from_array(v.pos);
            (lo.min(p), hi.max(p))
        })
    }

    #[test]
    fn the_hand_sits_low_right_and_ahead() {
        let mut masks = SpriteMasks::default();
        let forward = Vec3::NEG_Z;
        for item in
            [None, Some(Item::from_block(Block::STONE)), Some(Item::STICK), Some(Item::from_block(Block::TORCH))]
        {
            let hand = Hand { item, sky_light: 1.0, ..Default::default() };
            let v = vertices(&hand, forward, 70f32.to_radians(), WIDE_ASPECT, Mat4::IDENTITY, &mut masks);
            assert!(!v.is_empty() && v.len().is_multiple_of(6), "{item:?}");
            let (lo, hi) = bounds(&v);
            // In front of the eye (looking down -Z), right of and below centre.
            assert!(hi.z < -0.1 && lo.z > -1.5, "{item:?}: {lo} {hi}");
            assert!(hi.x > 0.2 && lo.y < -0.2 && hi.y < 0.4, "{item:?}: {lo} {hi}");
        }
        // Lowered for an item switch, it drops out of view.
        let low = Hand { item: Some(Item::STICK), equip: 1.0, ..Default::default() };
        let (_, hi) = bounds(&vertices(&low, forward, 70f32.to_radians(), WIDE_ASPECT, Mat4::IDENTITY, &mut masks));
        assert!(hi.y < -0.3, "{hi}");
    }

    #[test]
    fn narrow_split_screen_views_keep_the_hand_on_screen() {
        let mut masks = SpriteMasks::default();
        let fov = 70f32.to_radians();
        // Leftmost screen-x of the hand as a fraction of the half-width.
        let left_edge = |aspect: f32, masks: &mut SpriteMasks| {
            let hand = Hand { item: Some(Item::STICK), sky_light: 1.0, ..Default::default() };
            let v = vertices(&hand, Vec3::NEG_Z, fov, aspect, Mat4::IDENTITY, masks);
            v.iter().map(|v| v.pos[0] / -v.pos[2] / ((fov / 2.0).tan() * aspect)).fold(f32::MAX, f32::min)
        };
        let wide = left_edge(WIDE_ASPECT, &mut masks);
        let narrow = left_edge(0.89, &mut masks);
        assert!(wide > 0.0 && wide < 1.0, "{wide}");
        assert!(narrow > 0.0 && narrow < 0.9, "on the right half and on screen: {narrow}");
        // Ultra-wide windows keep Minecraft's placement.
        let mut a = SpriteMasks::default();
        let hand = Hand { item: Some(Item::STICK), ..Default::default() };
        assert_eq!(
            bounds(&vertices(&hand, Vec3::NEG_Z, fov, 2.4, Mat4::IDENTITY, &mut a)),
            bounds(&vertices(&hand, Vec3::NEG_Z, fov, WIDE_ASPECT, Mat4::IDENTITY, &mut a))
        );
    }
    #[test]
    fn camera_bob_is_applied_once_at_the_fixed_hand_fov() {
        let hand = Hand { item: Some(Item::STICK), sky_light: 1.0, ..Default::default() };
        let effect = Mat4::from_translation(Vec3::new(0.04, -0.08, 0.))
            * Mat4::from_rotation_z(0.07)
            * Mat4::from_rotation_x(0.01);
        let mut masks = SpriteMasks::default();
        let reference = vertices(&hand, Vec3::NEG_Z, HAND_FOV, WIDE_ASPECT, Mat4::IDENTITY, &mut masks);
        let projection = |fov| glam::camera::rh::proj::directx::perspective_infinite_reverse(fov, WIDE_ASPECT, 0.05);
        for degrees in [30f32, 70., 110.] {
            let fov = degrees.to_radians();
            let actual = vertices(&hand, Vec3::NEG_Z, fov, WIDE_ASPECT, effect, &mut masks);
            for (a, b) in actual.iter().zip(&reference) {
                let clip_a = projection(fov) * effect * Vec3::from_array(a.pos).extend(1.);
                let clip_b = projection(HAND_FOV) * effect * Vec3::from_array(b.pos).extend(1.);
                assert!((clip_a / clip_a.w).abs_diff_eq(clip_b / clip_b.w, 1e-5));
            }
        }
    }
}
