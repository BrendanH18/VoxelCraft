//! v1.0 gap Track 7e: the fox as Java 26.3's `FoxModel` poses it. The
//! adult stands on `AdultFoxModel`'s parts (an 8 x 6 x 6 head with its ears
//! and snout, the 6 x 11 x 6 body turned to lie along the fox with the
//! tail hanging off its end, 6-pixel legs); the kit keeps Track 7c's
//! `BabyFoxModel` rig. Both run `FoxModel.setupAnim` on Track 2e's fox
//! state (`Mob::fox`): the interested head roll, crouching (`crouchAmount`,
//! with its wiggle), sleeping (legs hidden, curled with the head turned
//! back), sitting and pouncing, each with the adult's or the kit's own
//! overrides, and `FoxRenderer.setupRotations` pitches the whole fox by its
//! look while it pounces or lies faceplanted. A sleeping fox closes its
//! eyes (`fox_sleep`). Colours and details are this game's own.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, FRAC_PI_6, PI};

use super::rig::{Bone, ROOT, Rig, bone, face, jbox, jboxg};
use super::*;

/// The coat (snow foxes recolour it, `animals::colour`), pale fur and the
/// dark socks and ears.
pub(super) const RED: Rgb = [201, 108, 47];
pub(super) const PALE: Rgb = [243, 229, 207];
pub(super) const SOCK: Rgb = [65, 44, 34];
/// The eyes and their glints, which close while it sleeps ([`colour`]).
pub(super) const EYE: Rgb = [28, 26, 23];
pub(super) const GLINT: Rgb = [226, 214, 196];

/// The parts `FoxModel.setupAnim` moves.
pub(super) struct Ids {
    pub head: usize,
    pub body: usize,
    /// Right hind, left hind, right front, left front.
    pub legs: [usize; 4],
    pub tail: usize,
}

/// `FoxRenderState`'s fox flags and eased values.
#[derive(Clone, Copy, Default)]
pub(super) struct State {
    pub sitting: bool,
    pub crouching: bool,
    pub sleeping: bool,
    pub pouncing: bool,
    pub faceplanted: bool,
    pub crouch_amount: f32,
    pub head_roll: f32,
}

/// The state `FoxRenderer.extractRenderState` reads.
pub(super) fn state(m: &Mob, alpha: f32) -> State {
    let a = m.animal.as_deref();
    let f = m.fox.as_deref();
    State {
        sitting: f.is_some_and(|f| f.sitting) || a.is_some_and(|a| a.sit_pose),
        crouching: f.is_some_and(|f| f.crouching),
        sleeping: a.is_some_and(|a| a.sleeping),
        pouncing: f.is_some_and(|f| f.pouncing),
        faceplanted: f.is_some_and(|f| f.faceplanted),
        crouch_amount: f.map_or(0.0, |f| f.crouch(alpha)),
        head_roll: f.map_or(0.0, |f| f.head_roll(alpha)),
    }
}

/// `FoxModel.setupAnim` on a fresh rig: the walking pose (with the head
/// roll), then crouching, sleeping or sitting, the pounce, the look, the
/// sleeper's turned head and the faceplanted legs' paddle. `walk` is the
/// model's own leg swing (the adult's trot, the kit's keyframes).
pub(super) fn setup(r: &mut Rig, ids: &Ids, s: &State, m: &Mob, time: f32, baby: bool, walk: impl FnOnce(&mut Rig)) {
    let age = age_ticks(time);
    let age_scale = if baby { 0.5 } else { 1.0 };
    let [rh, lh, rf, lf] = ids.legs;
    // `setWalkingPose`.
    r.z_rot(ids.head, s.head_roll);
    walk(r);
    if s.crouching {
        // `setCrouchingPose`, with the adult's or kit's body drop.
        r.rot[ids.body][0] += 0.104_719_76;
        r.at[ids.head][1] += s.crouch_amount * age_scale;
        let wiggle = age.cos() * 0.05;
        r.y_rot(ids.body, wiggle);
        r.z_rot(rh, wiggle);
        r.z_rot(lh, wiggle);
        r.z_rot(rf, wiggle / 2.0);
        r.z_rot(lf, wiggle / 2.0);
        r.at[ids.body][1] += if baby { s.crouch_amount / 6.0 } else { s.crouch_amount };
    } else if s.sleeping {
        for leg in ids.legs {
            r.shown[leg] = false;
        }
        if baby {
            r.rot[ids.body] = [-PI / 18.0, 0.0, -FRAC_PI_2];
            r.at[ids.body][0] -= 1.0;
            r.at[ids.body][1] += 1.0;
            r.at[ids.body][2] -= 1.0;
            r.x_rot(ids.tail, -2.181_661_6);
            r.at[ids.tail][0] -= 0.7;
            r.at[ids.tail][1] += 0.9;
            r.at[ids.tail][2] += 0.6;
            r.at[ids.head][0] -= 2.0;
            r.at[ids.head][1] += 2.8;
            r.at[ids.head][2] -= 4.0;
        } else {
            r.z_rot(ids.body, -FRAC_PI_2);
            r.at[ids.body][1] += 5.0;
            r.x_rot(ids.tail, -PI * 5.0 / 6.0);
            r.at[ids.head][0] += 2.0;
            r.at[ids.head][1] += 2.99;
        }
        r.y_rot(ids.head, -PI * 2.0 / 3.0);
        r.z_rot(ids.head, 0.0);
    } else if s.sitting {
        r.x_rot(ids.head, 0.0);
        r.y_rot(ids.head, 0.0);
        if baby {
            r.x_rot(ids.body, -0.959_931);
            r.at[ids.body][2] -= 4.5 * age_scale;
            r.at[ids.body][1] += 3.0 * age_scale;
            r.at[ids.tail][1] -= 0.6;
            r.at[ids.tail][2] -= 2.0 * age_scale;
            r.x_rot(ids.tail, 0.959_931_14);
            r.at[ids.head][1] -= 0.75;
            r.x_rot(rf, -PI / 12.0);
            r.x_rot(lf, -PI / 12.0);
            r.at[rf][2] -= 1.0;
            r.at[lf][2] -= 1.0;
            r.at[rf][0] += 0.01;
            r.at[lf][0] -= 0.01;
            r.at[rh][2] -= 3.75;
            r.at[lh][2] -= 3.75;
            r.at[rh][0] += 0.01;
            r.at[lh][0] -= 0.01;
        } else {
            r.x_rot(ids.body, FRAC_PI_6);
            r.at[ids.body][1] -= 7.0;
            r.at[ids.body][2] += 3.0;
            r.x_rot(ids.tail, FRAC_PI_4);
            r.at[ids.head][1] -= 6.5;
            r.at[ids.head][2] += 2.75;
            r.x_rot(rf, -PI / 12.0);
            r.x_rot(lf, -PI / 12.0);
            for leg in [rh, lh] {
                r.x_rot(leg, -PI * 5.0 / 12.0);
                r.at[leg][1] += 4.0;
                r.at[leg][2] -= 0.25;
            }
            r.at[ids.tail][2] -= 1.0;
        }
    }
    if s.pouncing && !baby {
        // `AdultFoxModel.setPouncingPose` (the kit's does nothing).
        let crouch = s.crouch_amount / 2.0;
        r.at[ids.body][1] -= crouch;
        r.at[ids.head][1] -= crouch;
    }
    if !s.sleeping && !s.faceplanted && !s.crouching {
        r.x_rot(ids.head, head_x(m));
        r.y_rot(ids.head, m.head_yaw);
    }
    if s.sleeping {
        r.rot[ids.head] = [0.0, -PI * 2.0 / 3.0, (age * 0.027).cos() / 22.0];
    }
    if s.faceplanted {
        // `legMotionPos`: the model's own counter, which gains 0.67 each
        // time it poses a faceplanted fox (v1.0 gap Track 7f).
        let pos = leg_motion(baby) * 0.4662;
        r.x_rot(rh, pos.cos() * 0.1);
        r.x_rot(lh, (pos + PI).cos() * 0.1);
        r.x_rot(rf, (pos + PI).cos() * 0.1);
        r.x_rot(lf, pos.cos() * 0.1);
    }
}

/// `FoxModel.legMotionPos` of the adult's and the kit's model (Java's
/// renderer keeps one of each): a field of the model, not of a fox, that
/// each faceplanted fox posed with it advances by 0.67, so it runs per
/// rendered frame and is shared by every faceplanted fox drawn. Kept as
/// `f32` bits; only the faceplant paddle reads it.
static LEG_MOTION: [std::sync::atomic::AtomicU32; 2] =
    [std::sync::atomic::AtomicU32::new(0), std::sync::atomic::AtomicU32::new(0)];

/// `this.legMotionPos += 0.67F`, returning the new value.
fn leg_motion(baby: bool) -> f32 {
    use std::sync::atomic::Ordering::Relaxed;
    let cell = &LEG_MOTION[usize::from(baby)];
    let old =
        cell.try_update(Relaxed, Relaxed, |bits| Some((f32::from_bits(bits) + 0.67).to_bits())).unwrap_or_default();
    f32::from_bits(old) + 0.67
}

/// v1.0 gap Track 7f: `FoxHeldItemLayer`, the fox's main-hand item in its
/// mouth: at the posed head's pivot (in `FoxRenderer.setupRotations`'
/// pitch), scaled 0.75 for a kit, turned by the head roll, the look's yaw
/// and pitch (the state's, not the head part's), moved to the mouth
/// (`(0.06, 0.27, -0.5)`; asleep `(0.46, 0.26, 0.22)`, the kit's
/// `(0.06, 0.26, -0.5)` and `(0.4, 0.26, 0.15)`), laid flat by a quarter
/// turn about x (and a quarter about z asleep), in the item's `ground`
/// display transform (`HoldingEntityRenderState`).
#[allow(clippy::too_many_arguments)]
pub(super) fn held_item(
    out: &mut EntityMesh,
    m: &Mob,
    stack: crate::inventory::Stack,
    head: &Part,
    origin: Vec3,
    body: Quat,
    scale: f32,
    light: ([u8; 4], u8),
    alpha: f32,
) {
    use glam::Mat4;
    let s = state(m, alpha);
    let baby = m.is_baby();
    let pitch = if s.pouncing || s.faceplanted { Quat::from_rotation_x(head_x(m)) } else { Quat::IDENTITY };
    let offset = match (baby, s.sleeping) {
        (true, true) => Vec3::new(0.4, 0.26, 0.15),
        (true, false) => Vec3::new(0.06, 0.26, -0.5),
        (false, true) => Vec3::new(0.46, 0.26, 0.22),
        (false, false) => Vec3::new(0.06, 0.27, -0.5),
    };
    let mut mouth = Mat4::from_translation(origin)
        * Mat4::from_quat(body)
        * Mat4::from_scale(Vec3::splat(scale / 16.0))
        * Mat4::from_translation(head.pivot)
        * Mat4::from_quat(pitch)
        // Java's model frame: y down, z back, in blocks.
        * Mat4::from_rotation_x(PI)
        * Mat4::from_scale(Vec3::splat(if baby { 16.0 * 0.75 } else { 16.0 }))
        * Mat4::from_rotation_z(s.head_roll)
        * Mat4::from_rotation_y(m.head_yaw)
        * Mat4::from_rotation_x(head_x(m))
        * Mat4::from_translation(offset)
        * Mat4::from_rotation_x(FRAC_PI_2);
    if s.sleeping {
        mouth *= Mat4::from_rotation_z(FRAC_PI_2);
    }
    let display = crate::content::item_display::drawn(&stack);
    out.push_held(HeldItemModel {
        stack,
        transform: mouth * crate::content::item_display::Context::Ground.matrix(&display),
        sky_light: f32::from(light.0[0]) / 255.0,
        block_light: f32::from(light.1) / 255.0,
        block: None,
        icon: None,
        tint: None,
    });
}

/// `FoxRenderer.setupRotations`: a pouncing or faceplanted fox pitched by
/// its look (`rotateDegrees(XP, -xRot)` about its feet), nose up as it
/// leaps and down as it lands.
pub(super) fn pitch(p: &mut Parts, s: &State, m: &Mob) {
    if s.pouncing || s.faceplanted {
        p.turn_about(Vec3::ZERO, Quat::from_rotation_x(head_x(m)));
    }
}

/// `fox_sleep`: a sleeping fox's eyes closed, a dark slit under the fur.
pub(super) fn colour(m: &Mob, c: &mut Cuboid) {
    if m.kind != MobKind::Fox || !m.animal.as_deref().is_some_and(|a| a.sleeping) {
        return;
    }
    if c.color == EYE {
        c.color = [96, 54, 30];
    } else if c.color == GLINT {
        c.color = EYE;
    }
}

// ---------------------------------------------------------------- the adult

const HEAD: usize = 0;
const BODY: usize = 1;
const RIGHT_HIND_LEG: usize = 2;
const LEFT_HIND_LEG: usize = 3;
const RIGHT_FRONT_LEG: usize = 4;
const LEFT_FRONT_LEG: usize = 5;
const TAIL: usize = 6;

/// `AdultFoxModel`'s head with its ears and snout (children at its pivot),
/// and this game's face: white cheeks, eyes with glints, a black nose and
/// dark inner ears.
const ADULT_HEAD: &[Cuboid] = &[
    jbox([-3.0, -2.0, -5.0], [8.0, 6.0, 6.0], RED, 20).with(Pattern::Fur),
    jbox([-3.0, -4.0, -4.0], [2.0, 2.0, 1.0], SOCK, 16),
    jbox([3.0, -4.0, -4.0], [2.0, 2.0, 1.0], SOCK, 16),
    jbox([-1.0, 2.01, -8.0], [4.0, 2.0, 3.0], PALE, 15),
    face([0.0, 2.0], [2.01, 3.0], -8.0, 0.15, [35, 31, 29]),
    face([-3.0, -1.0], [2.0, 4.0], -5.0, 0.1, PALE),
    face([3.0, 5.0], [2.0, 4.0], -5.0, 0.1, PALE),
    face([-2.5, -1.0], [0.0, 1.0], -5.0, 0.1, EYE),
    face([3.0, 4.5], [0.0, 1.0], -5.0, 0.1, EYE),
    face([-1.75, -1.4], [0.1, 0.45], -5.1, 0.05, GLINT),
    face([3.75, 4.1], [0.1, 0.45], -5.1, 0.05, GLINT),
    face([-2.6, -1.4], [-3.7, -2.3], -4.0, 0.05, [40, 28, 22]),
    face([3.4, 4.6], [-3.7, -2.3], -4.0, 0.05, [40, 28, 22]),
];
/// The body, turned a quarter about x so its 11 pixels lie along the fox,
/// with a pale belly on its underside.
const ADULT_BODY: &[Cuboid] = &[
    jbox([-3.0, 3.999, -3.5], [6.0, 11.0, 6.0], RED, 24).with(Pattern::Fur),
    jbox([-2.5, 5.0, -3.6], [5.0, 8.0, 0.2], [239, 229, 209], 12),
];
const ADULT_LEG: &[Cuboid] = &[jboxg([2.0, 0.5, -1.0], [2.0, 6.0, 2.0], 0.001, SOCK, 16)];
/// The tail, its last third white.
const ADULT_TAIL: &[Cuboid] = &[
    jbox([2.0, 0.0, -1.0], [4.0, 6.0, 5.0], RED, 24).with(Pattern::Fur),
    jbox([2.0, 6.0, -1.0], [4.0, 3.0, 5.0], [242, 234, 218], 16).with(Pattern::Fur),
];

static ADULT_BONES: &[Bone] = &[
    bone(ADULT_HEAD, ROOT, [-1.0, 16.5, -3.0], [0.0; 3]),
    bone(ADULT_BODY, ROOT, [0.0, 16.0, -6.0], [FRAC_PI_2, 0.0, 0.0]),
    bone(ADULT_LEG, ROOT, [-5.0, 17.5, 7.0], [0.0; 3]),
    bone(ADULT_LEG, ROOT, [-1.0, 17.5, 7.0], [0.0; 3]),
    bone(ADULT_LEG, ROOT, [-5.0, 17.5, 0.0], [0.0; 3]),
    bone(ADULT_LEG, ROOT, [-1.0, 17.5, 0.0], [0.0; 3]),
    bone(ADULT_TAIL, BODY as u8, [-4.0, 15.0, -1.0], [-0.052_359_88, 0.0, 0.0]),
];

const ADULT_IDS: Ids =
    Ids { head: HEAD, body: BODY, legs: [RIGHT_HIND_LEG, LEFT_HIND_LEG, RIGHT_FRONT_LEG, LEFT_FRONT_LEG], tail: TAIL };

/// The adult fox: `AdultFoxModel`'s trot (`cos(pos * 0.6662) * 1.4 *
/// speed`, the right hind and left front leg together) under
/// [`setup`].
pub(super) fn adult(m: &Mob, time: f32, alpha: f32) -> Parts {
    let s = state(m, alpha);
    let mut r = Rig::new(ADULT_BONES);
    setup(&mut r, &ADULT_IDS, &s, m, time, false, |r| {
        r.quadruped_walk(m, [RIGHT_HIND_LEG, LEFT_HIND_LEG, RIGHT_FRONT_LEG, LEFT_FRONT_LEG], motion::LEG_SWING);
    });
    let mut p = r.parts();
    pitch(&mut p, &s, m);
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fox(baby: bool) -> Mob {
        let mut m = Mob::new(MobKind::Fox, DVec3::new(10.0, 64.0, 0.0), 0.0);
        if baby {
            m.baby = true;
            m.age = crate::entity::animals::BABY_AGE;
        }
        m
    }

    fn posed(m: &Mob) -> Parts {
        super::super::pose(m, 0.0, 1.0)
    }

    fn crouch(m: &mut Mob, amount: f32) {
        let f = m.fox.as_mut().unwrap();
        f.crouching = true;
        (f.crouch_amount, f.crouch_amount_o) = (amount, amount);
    }

    /// `FoxModel.setupAnim` with `AdultFoxModel`'s overrides.
    #[test]
    fn the_adult_fox_takes_track_2e_s_poses() {
        let standing = posed(&fox(false));
        assert_eq!(standing.len(), 7);
        // Asleep: the legs hide and the head turns back over the body.
        let mut m = fox(false);
        m.animal.as_mut().unwrap().sleeping = true;
        let p = posed(&m);
        assert_eq!(p.len(), 3);
        assert!(p[HEAD].rot.angle_between(standing[HEAD].rot) > 1.5);
        // Crouched (`crouchAmount` 5): the body and head drop 5 pixels.
        let mut m = fox(false);
        crouch(&mut m, 5.0);
        let p = posed(&m);
        assert!((standing[HEAD].pivot.y - p[HEAD].pivot.y - 5.0).abs() < 1e-3);
        assert!((standing[BODY].pivot.y - p[BODY].pivot.y - 5.0).abs() < 0.3);
        // Interested: the head rolls by `interestedAngle * 0.11 * pi`.
        let mut m = fox(false);
        let f = m.fox.as_mut().unwrap();
        (f.interested_angle, f.interested_angle_o) = (1.0, 1.0);
        let roll = posed(&m)[HEAD].rot.to_euler(glam::EulerRot::ZYX).0;
        assert!((roll.abs() - 0.11 * PI).abs() < 1e-3, "{roll}");
        // Sitting: the body sits up and the hind legs fold forward.
        let mut m = fox(false);
        m.fox.as_mut().unwrap().sitting = true;
        let p = posed(&m);
        assert!(p[BODY].pivot.y > standing[BODY].pivot.y + 6.0);
        assert!(p[RIGHT_HIND_LEG].rot.angle_between(Quat::IDENTITY) > 1.2);
        // Pouncing while rising (looking up): the whole fox pitches nose up
        // about its feet.
        let mut m = fox(false);
        m.head_pitch = 0.5;
        let level = posed(&m);
        m.fox.as_mut().unwrap().pouncing = true;
        let p = posed(&m);
        assert!(p[HEAD].pivot.y > level[HEAD].pivot.y + 0.3, "{} vs {}", p[HEAD].pivot, level[HEAD].pivot);
        // Faceplanted and looking down: nose into the ground.
        let mut m = fox(false);
        m.head_pitch = -0.6;
        let f = m.fox.as_mut().unwrap();
        f.faceplanted = true;
        assert!(posed(&m)[HEAD].pivot.y < standing[HEAD].pivot.y - 1.0);
    }

    /// v1.0 gap Track 7f: `legMotionPos` is the model's, advanced each time
    /// it poses a faceplanted fox, so the paddle moves from one rendered
    /// frame to the next even at one `ageInTicks`.
    #[test]
    fn the_faceplanted_paddle_advances_each_frame() {
        let mut m = fox(false);
        m.fox.as_mut().unwrap().faceplanted = true;
        let legs = |m: &Mob| posed(m)[RIGHT_HIND_LEG].rot;
        let (a, b) = (legs(&m), legs(&m));
        assert!(a.angle_between(b) > 1e-4, "{a} {b}");
        let before = f32::from_bits(LEG_MOTION[0].load(std::sync::atomic::Ordering::Relaxed));
        posed(&fox(false));
        posed(&fox(true));
        // Only faceplanted foxes move the adult model's counter.
        let after = f32::from_bits(LEG_MOTION[0].load(std::sync::atomic::Ordering::Relaxed));
        assert!(after == before || after >= before + 0.67 - 1e-3);
    }

    /// v1.0 gap Track 7f: `FoxHeldItemLayer`, the main-hand item in the
    /// mouth, in front of the head at the snout, for the adult and the kit;
    /// asleep it lies to the side.
    #[test]
    fn foxes_hold_their_item_in_the_mouth() {
        let mesh = |m: &Mob| {
            let mut out = EntityMesh::flat();
            assert_eq!(build(std::slice::from_ref(m), DVec3::new(0.0, 64.0, 0.0), Vec3::X, 100.0, 1.0, &mut out), 1);
            out
        };
        for baby in [false, true] {
            let mut m = fox(baby);
            assert!(mesh(&m).held_items().is_empty());
            let stack = crate::inventory::Stack::new(crate::item::Item::FEATHER, 1);
            m.equipment.set(crate::entity::equipment::Slot::MainHand, Some(stack));
            let out = mesh(&m);
            assert_eq!(out.held_items().len(), 1, "baby {baby}");
            let centre = |out: &EntityMesh| out.held_items()[0].transform.transform_point3(Vec3::splat(0.5));
            let c = centre(&out);
            // Facing +X from x = 10: at the snout, low on the head.
            assert!((10.25..11.0).contains(&c.x), "baby {baby}: {c}");
            assert!((0.05..0.5).contains(&c.y), "baby {baby}: {c}");
            assert!(c.z.abs() < 0.1, "baby {baby}: {c}");
            // The look turns it with the head.
            m.head_yaw = 0.6;
            assert!(centre(&mesh(&m)).z.abs() > 0.1, "baby {baby}");
            m.head_yaw = 0.0;
            m.animal.as_mut().unwrap().sleeping = true;
            assert!(centre(&mesh(&m)).distance(c) > 0.1, "baby {baby}");
        }
    }

    /// The kit's `BabyFoxModel` overrides: a sixth of the crouch, curled up
    /// asleep, its eyes closed.
    #[test]
    fn the_kit_crouches_and_sleeps_with_its_eyes_closed() {
        let standing = posed(&fox(true));
        let mut m = fox(true);
        crouch(&mut m, 6.0);
        let p = posed(&m);
        // The head drops crouchAmount * ageScale, the body a sixth.
        assert!((standing[0].pivot.y - p[0].pivot.y - 3.0).abs() < 1e-3);
        let mut m = fox(true);
        m.animal.as_mut().unwrap().sleeping = true;
        assert_eq!(posed(&m).len(), 3);
        let mut eye = cube([0.0; 3], [1.0; 3], EYE, 0);
        colour(&m, &mut eye);
        assert_ne!(eye.color, EYE);
    }
}
