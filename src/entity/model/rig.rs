//! v1.0 gap Track 7c: Java's model part hierarchies (`PartDefinition`
//! trees), posed as `ModelPart` poses them and flattened into this game's
//! [`Parts`]. A model is a table of [`Bone`]s, parents first, each with its
//! `PartPose` offset and rotation in Java's model space (pixels, y down from
//! 24 above the feet, -z forward) and its boxes in this game's axes, written
//! with [`jbox`] from the numbers of Java's `addBox`. The model's animation
//! changes a [`Rig`]'s offsets and rotations as Java's `setupAnim` changes
//! the parts' `x`, `y`, `z`, `xRot`, `yRot` and `zRot`; [`Rig::parts`] then
//! composes each child with its parent (`ModelPart.translateAndRotate`:
//! offset, then `rotationZYX`) and maps Java's space onto this game's by
//! the half turn about X that [`Part::on_java_side`] describes: Java's
//! right (-x) stays the mob's right (-X), y and z change sign, and the y
//! and z turns with them. Nothing from Java's geometry files or textures
//! is used; the numbers are proportions and pivots.

use super::*;

/// A [`Bone`] with no parent: its offset is from the model's origin, 24
/// pixels above the feet.
pub(super) const ROOT: u8 = u8::MAX;

/// The most bones a rig holds.
pub(super) const MAX_BONES: usize = 24;

/// The model's own root part (`createPartLookup`'s "root"), which some
/// keyframe animations move: every [`ROOT`] bone hangs from it, and it
/// pivots 24 pixels above the feet.
pub(super) const MODEL_ROOT: usize = MAX_BONES;

/// One `PartDefinition`: its boxes (this game's axes, relative to its
/// pivot), parent and `PartPose`.
#[derive(Clone, Copy)]
pub(super) struct Bone {
    pub boxes: &'static [Cuboid],
    pub parent: u8,
    /// `PartPose`'s offset and rotation (radians), in Java's axes.
    pub at: [f32; 3],
    pub rot: [f32; 3],
}

/// A bone with its offset and rotation.
pub(super) const fn bone(boxes: &'static [Cuboid], parent: u8, at: [f32; 3], rot: [f32; 3]) -> Bone {
    Bone { boxes, parent, at, rot }
}

/// Java's `addBox(x, y, z, w, h, d)` turned onto this game's axes: x kept,
/// y and z reversed.
pub(super) const fn jbox(o: [f32; 3], s: [f32; 3], color: Rgb, noise: u8) -> Cuboid {
    cube([o[0], -(o[1] + s[1]), -(o[2] + s[2])], [o[0] + s[0], -o[1], -o[2]], color, noise)
}

/// [`jbox`] with a `CubeDeformation` of `g` on every side.
pub(super) const fn jboxg(o: [f32; 3], s: [f32; 3], g: f32, color: Rgb, noise: u8) -> Cuboid {
    cube([o[0] - g, -(o[1] + s[1]) - g, -(o[2] + s[2]) - g], [o[0] + s[0] + g, -o[1] + g, -o[2] + g], color, noise)
}

/// A detail drawn flat on a Java box's face: `jbox` with the face's depth
/// given as Java's z of the face (`front`) and a thickness out of it.
/// Detail boxes (eyes, noses, markings) are this game's own.
pub(super) const fn face(x: [f32; 2], y: [f32; 2], front: f32, depth: f32, color: Rgb) -> Cuboid {
    cube([x[0], -y[1], -front], [x[1], -y[0], -front + depth], color, 0)
}

/// Java's `rotationZYX(zRot, yRot, xRot)` in this game's axes.
pub(super) fn java_quat([x, y, z]: [f32; 3]) -> Quat {
    Quat::from_rotation_z(-z) * Quat::from_rotation_y(-y) * Quat::from_rotation_x(x)
}

/// A model being posed: every bone's `x`, `y`, `z` and rotations, reset to
/// its `PartPose` (`ModelPart.resetPose`) and changed by the animation.
pub(super) struct Rig {
    bones: &'static [Bone],
    pub at: [[f32; 3]; MAX_BONES + 1],
    pub rot: [[f32; 3]; MAX_BONES + 1],
    /// `ModelPart.visible`.
    pub shown: [bool; MAX_BONES],
}

impl Rig {
    pub fn new(bones: &'static [Bone]) -> Self {
        debug_assert!(bones.len() <= MAX_BONES);
        let mut r =
            Self { bones, at: [[0.0; 3]; MAX_BONES + 1], rot: [[0.0; 3]; MAX_BONES + 1], shown: [true; MAX_BONES] };
        for (i, b) in bones.iter().enumerate() {
            debug_assert!(b.parent == ROOT || (b.parent as usize) < i, "parents first");
            (r.at[i], r.rot[i]) = (b.at, b.rot);
        }
        r
    }

    /// The bone's rotation about each axis, Java's `xRot`, `yRot`, `zRot`.
    pub fn x_rot(&mut self, i: usize, v: f32) {
        self.rot[i][0] = v;
    }
    pub fn y_rot(&mut self, i: usize, v: f32) {
        self.rot[i][1] = v;
    }
    pub fn z_rot(&mut self, i: usize, v: f32) {
        self.rot[i][2] = v;
    }

    /// The head's look, `head.xRot = xRot` and `head.yRot = yRot` from the
    /// mob's pitch and head yaw.
    pub fn look(&mut self, i: usize, m: &Mob) {
        self.rot[i][0] = head_x(m);
        self.rot[i][1] = m.head_yaw;
    }

    /// `QuadrupedModel.setupAnim`'s legs on Java's sides: the right hind
    /// and left front leg on `cos(pos * 0.6662) * 1.4 * speed`, the other
    /// two half a cycle later. `legs` is right hind, left hind, right
    /// front, left front.
    pub fn quadruped_walk(&mut self, m: &Mob, [rh, lh, rf, lf]: [usize; 4], amplitude: f32) {
        let swing = walk_cos(m) * amplitude;
        self.rot[rh][0] = swing;
        self.rot[lh][0] = -swing;
        self.rot[rf][0] = -swing;
        self.rot[lf][0] = swing;
    }

    /// Every bone composed with its parents and turned onto this game's
    /// axes; bones without boxes, or hidden, add no part.
    pub fn parts(&self) -> Parts {
        let mut world = [(Vec3::ZERO, Quat::IDENTITY); MAX_BONES];
        let mut shown = [true; MAX_BONES];
        let mut out = Parts::new();
        let (base, base_rot) = self.model_root();
        for (i, b) in self.bones.iter().enumerate() {
            let [x, y, z] = self.at[i];
            let local = java_quat(self.rot[i]);
            let (pivot, rot) = if b.parent == ROOT {
                (base + base_rot * Vec3::new(x, -y, -z), base_rot * local)
            } else {
                let (pp, pr) = world[b.parent as usize];
                shown[i] = shown[b.parent as usize];
                (pp + pr * Vec3::new(x, -y, -z), pr * local)
            };
            world[i] = (pivot, rot);
            shown[i] &= self.shown[i];
            if shown[i] && !b.boxes.is_empty() {
                out.push(Part { boxes: b.boxes, pivot, rot });
            }
        }
        out
    }

    /// Where the model's root part puts Java's origin, and its turn.
    fn model_root(&self) -> (Vec3, Quat) {
        let [x, y, z] = self.at[MODEL_ROOT];
        (Vec3::new(x, 24.0 - y, -z), java_quat(self.rot[MODEL_ROOT]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOX: &[Cuboid] = &[jbox([-1.0, 0.0, -1.0], [2.0, 6.0, 2.0], [0; 3], 0)];

    /// A root bone at Java's y 18 stands 6 pixels up, a child hangs off it,
    /// and Java's x turn keeps its sign while its y turn changes.
    #[test]
    fn bones_compose_onto_this_game_s_axes() {
        static BONES: &[Bone] =
            &[bone(BOX, ROOT, [2.0, 18.0, -3.0], [0.0; 3]), bone(BOX, 0, [0.0, 6.0, 0.0], [0.0; 3])];
        let mut r = Rig::new(BONES);
        let p = r.parts();
        assert_eq!(p[0].pivot, Vec3::new(2.0, 6.0, 3.0));
        assert_eq!(p[1].pivot, Vec3::new(2.0, 0.0, 3.0));
        // The box hangs down from its pivot and a forward (Java -x turn
        // lifts the front) swing moves the foot forward, as in Java.
        assert_eq!(BOX[0].min, [-1.0, -6.0, -1.0]);
        r.x_rot(0, -std::f32::consts::FRAC_PI_2);
        let foot = r.parts()[0].rot * Vec3::new(0.0, -6.0, 0.0);
        assert!((foot - Vec3::new(0.0, 0.0, 6.0)).length() < 1e-4, "{foot}");
        // Java's +y turn (towards the mob's left) is a -Y turn here.
        r.rot[0] = [0.0, 0.5, 0.0];
        assert!((r.parts()[0].rot.to_euler(glam::EulerRot::YXZ).0 + 0.5).abs() < 1e-5);
    }
}
