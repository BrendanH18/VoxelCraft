//! v1.0 gap Track 8c: the flame Java's client draws on a burning entity.
//!
//! `EntityRenderDispatcher.submit` submits a flame for every entity whose
//! `displayFireAnimation()` is set (`isOnFire() && !isSpectator()`), and
//! `FlameFeatureRenderer.prepare` draws it: square quads facing the camera
//! (turned about the vertical axis only, `Mth.rotationAroundAxis(Y_AXIS,
//! camera.orientation)`), stacked up the entity's height, all scaled by
//! 1.4 times its bounding box's width. Each quad is 1 wide (`r` = 0.5) and
//! 1.4 tall, 0.45 above the last, 0.9 times as wide and 0.03 further back;
//! the stack starts 0.3 toward the camera (less 0.02 per whole unit of
//! scaled height). The textures alternate `fire_0` and `fire_1`, mirrored
//! from the third quad on (`ss / 2 % 2`), in block light 15 with the
//! entity's own sky light, as `entityCutoutCull` on the block atlas.
//!
//! VoxelCraft's `fire_0` is `tex::FIRE_0`, seven frames at ten a second, as
//! the fire block draws it; v1.0 final track F3: the `fire_1` quads draw the
//! second texture (`tex::FIRE_1`, packed one frame a colour channel, as
//! material 7 with the channel in the layer's bits 12-13).

use super::*;
use crate::world::block::tex;

/// `FlameFeatureRenderer`'s flame for an entity at `origin` (camera-relative,
/// the bottom centre of its box) whose bounding box is `width` by `height`
/// blocks, seen along `forward`, in sky light `sky` (0..255) at `time`
/// seconds.
pub(crate) fn flame(
    out: &mut EntityMesh,
    origin: Vec3,
    (width, height): (f32, f32),
    forward: Vec3,
    sky: u8,
    time: f32,
) {
    let s = width * 1.4;
    if s <= 0.0 {
        return;
    }
    // The camera's yaw alone: local +Z points back at the viewer, +X to
    // their right.
    let back = -Vec3::new(forward.x, 0.0, forward.z).normalize_or(Vec3::Z);
    let right = Vec3::Y.cross(back);
    let mut h = height / s;
    let push = 0.3 - h.trunc() * 0.02;
    let frame = (time * 10.0).max(0.0) as u32 % u32::from(tex::FIRE_FRAMES);
    let (mut r, mut yo, mut zo) = (0.5f32, 0.0f32, 0.0f32);
    let mut ss = 0u32;
    while h > 0.0 {
        let (material, layer) = flame_texture(ss, frame);
        // `fire_0` and `fire_1` as they are for the first two quads, then
        // mirrored for two, and so on.
        let (left_u, right_u) = if (ss / 2).is_multiple_of(2) { (0.0, 1.0) } else { (1.0, 0.0) };
        let at = |x: f32, y: f32| origin + (right * x + Vec3::Y * (y - yo) + back * (zo + push)) * s;
        let corners = [
            (at(-r, 0.0), [left_u, 1.0]),
            (at(r, 0.0), [right_u, 1.0]),
            (at(r, 1.4), [right_u, 0.0]),
            (at(-r, 1.4), [left_u, 0.0]),
        ];
        for i in [0, 1, 2, 0, 2, 3] {
            let (pos, uv) = corners[i];
            out.push(EntityVertex {
                pos: pos.to_array(),
                uv,
                color: [255, 255, 255, 0],
                // Sky light, full shade (Java's normal is straight up), no
                // hurt tint, not emissive.
                light: [sky, 255, 0, 0],
                // Block light 15 (`LightCoordsUtil.withBlock(.., 15)`), the
                // block atlas (material 2) at the fire's layer.
                torch: [255, material, layer as u8, (layer >> 8) as u8],
            });
        }
        h -= 0.45;
        yo -= 0.45;
        r *= 0.9;
        zo -= 0.03;
        ss += 1;
    }
}

/// The `ss`th flame quad's material and layer at animation frame `frame`:
/// `fire_0` (the block atlas, material 2) for even quads, `fire_1` (material
/// 7, its packed layer and channel) for odd ones.
pub(crate) fn flame_texture(ss: u32, frame: u32) -> (u8, u16) {
    if ss.is_multiple_of(2) {
        (2, tex::FIRE_0 + frame as u16)
    } else {
        (7, (tex::FIRE_1 + (frame / 3) as u16) | ((frame % 3) as u16) << 12)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An arrow's or trident's 0.5 by 0.5 box: scale 0.7, two quads (the
    /// scaled height 0.71 takes 0.45 twice to run out), the first 0.98
    /// blocks tall and 0.7 wide, 0.21 toward the camera, the second 0.45
    /// higher up the scale, narrower and 0.021 behind it, and both facing
    /// the camera.
    #[test]
    fn a_burning_arrow_gets_two_camera_facing_quads() {
        let mut out = EntityMesh::flat();
        let forward = Vec3::new(1.0, -0.5, 0.0);
        flame(&mut out, Vec3::ZERO, (0.5, 0.5), forward, 200, 0.0);
        assert_eq!(out.len(), 12, "two quads");
        let quad = |q: usize| &out[q * 6..q * 6 + 6];
        let xs = |q: usize| quad(q).iter().map(|v| v.pos[0]).collect::<Vec<_>>();
        // Facing a camera looking along +X, both lie across X, toward it.
        assert!(xs(0).iter().all(|&x| (x + 0.21).abs() < 1e-5), "{:?}", xs(0));
        assert!(xs(1).iter().all(|&x| (x + 0.21 - 0.021).abs() < 1e-5), "{:?}", xs(1));
        let ys = |q: usize| quad(q).iter().map(|v| v.pos[1]);
        let (lo, hi) = (ys(0).fold(f32::MAX, f32::min), ys(0).fold(f32::MIN, f32::max));
        assert!(lo.abs() < 1e-5 && (hi - 0.98).abs() < 1e-5, "{lo}..{hi}");
        let lo1 = ys(1).fold(f32::MAX, f32::min);
        assert!((lo1 - 0.45 * 0.7).abs() < 1e-5, "{lo1}");
        let zs = |q: usize| quad(q).iter().map(|v| v.pos[2]).fold(f32::MIN, f32::max);
        assert!((zs(0) - 0.35).abs() < 1e-5 && (zs(1) - 0.35 * 0.9).abs() < 1e-5, "{} {}", zs(0), zs(1));
        // The fire texture in block light 15 with the entity's sky light.
        for v in out.iter() {
            assert_eq!((v.torch[0], v.light[0]), (255, 200));
        }
        // `fire_0` then `fire_1` (v1.0 final track F3: its own texture).
        let layer = |q: usize| u16::from(quad(q)[0].torch[2]) | (u16::from(quad(q)[0].torch[3]) << 8);
        assert_eq!((quad(0)[0].torch[1], layer(0)), (2, tex::FIRE_0));
        assert_eq!((quad(1)[0].torch[1], layer(1)), (7, tex::FIRE_1));
    }

    /// v1.0 gap Track 8d: a burning mob carries the flame over its box (a
    /// zombie's 0.6 by 1.95: six quads), and nothing else of it is drawn
    /// in the fire texture (the rising cubes are gone).
    #[test]
    fn a_burning_mob_carries_the_flame() {
        let fire = |burning: bool| {
            let mut m = Mob::new(MobKind::Zombie, DVec3::ZERO, 0.0);
            m.burning = burning;
            let mut out = EntityMesh::flat();
            build(std::slice::from_ref(&m), DVec3::new(0.0, 1.0, -4.0), Vec3::Z, 64.0, 1.0, &mut out);
            out.iter()
                .filter(|v| {
                    let layer = u16::from(v.torch[2]) | (u16::from(v.torch[3]) << 8);
                    (v.torch[1] == 2 && (tex::FIRE_0..tex::FIRE_0 + u16::from(tex::FIRE_FRAMES)).contains(&layer))
                        || (v.torch[1] == 7 && (tex::FIRE_1..tex::FIRE_1_END).contains(&(layer & 4095)))
                })
                .count()
        };
        assert_eq!((fire(false), fire(true)), (0, 6 * 6));
    }

    /// A taller box stacks more quads: 1.4 / 0.45, rounded up, for a box
    /// twice as tall as wide.
    #[test]
    fn taller_entities_stack_more_quads() {
        let mut out = EntityMesh::flat();
        flame(&mut out, Vec3::ZERO, (0.6, 1.8), Vec3::Z, 255, 1.0);
        // h = 1.8 / 0.84 = 2.14: 2.14, 1.69, 1.24, 0.79, 0.34.
        assert_eq!(out.len(), 5 * 6);
    }
}
