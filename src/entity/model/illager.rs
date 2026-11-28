//! v0.15 Track A: original box models for the illagers, the vex, the
//! ravager and evoker fangs. The illagers share one robed body; their arms
//! follow `AbstractIllager.IllagerArmPose`. No Mojang geometry or textures.
use super::*;
use crate::entity::illager::{ArmPose, Fang};

const SKIN: Rgb = [146, 152, 148];
const NOSE: Rgb = [128, 134, 130];
const BROW: Rgb = [52, 50, 48];
const EYE: Rgb = [42, 110, 72];

// v0.18 Track 6: eye whites, a thin mouth and a robe placket (the skin and
// cloth take the default pattern).
const EYE_WHITE: Rgb = [214, 212, 204];
const fn head(eye: Rgb) -> [Cuboid; 9] {
    [
        cube([-4.0, 0.0, -4.0], [4.0, 9.0, 4.0], SKIN, 20),
        cube([-1.0, 1.0, 4.0], [1.0, 5.0, 6.0], NOSE, 16),
        cube([-3.5, 5.0, 4.0], [3.5, 6.0, 4.15], BROW, 0),
        cube([-3.0, 4.0, 4.0], [-2.0, 5.0, 4.1], EYE_WHITE, 0),
        cube([-2.0, 4.0, 4.0], [-1.0, 5.0, 4.1], eye, 0),
        cube([1.0, 4.0, 4.0], [2.0, 5.0, 4.1], eye, 0),
        cube([2.0, 4.0, 4.0], [3.0, 5.0, 4.1], EYE_WHITE, 0),
        cube([-1.5, 0.4, 4.0], [1.5, 0.8, 4.05], [86, 90, 88], 0),
        cube([-4.05, 7.5, -4.05], [4.05, 9.0, 4.05], [64, 60, 58], 10),
    ]
}
const HEAD: [Cuboid; 9] = head(EYE);
const HEAD_ILLUSIONER: [Cuboid; 9] = head([60, 90, 170]);

const fn robe(cloth: Rgb, trim: Rgb, belt: Rgb) -> [Cuboid; 5] {
    [
        cube([-4.0, 12.0, -3.0], [4.0, 24.0, 3.0], cloth, 26),
        cube([-4.2, 4.0, -3.2], [4.2, 12.0, 3.2], cloth, 26),
        cube([-4.25, 4.0, 3.2], [4.25, 5.0, 3.3], trim, 8),
        cube([-4.1, 13.0, -3.1], [4.1, 14.0, 3.1], belt, 8),
        cube([-0.75, 5.0, 3.2], [0.75, 12.0, 3.25], trim, 8),
    ]
}
const VINDICATOR_BODY: [Cuboid; 5] = robe([62, 66, 72], [40, 42, 46], [84, 58, 40]);
const EVOKER_BODY: [Cuboid; 5] = robe([34, 32, 36], [198, 162, 60], [150, 120, 50]);
const ILLUSIONER_BODY: [Cuboid; 5] = robe([46, 76, 140], [180, 190, 210], [70, 60, 50]);
const PILLAGER_BODY: [Cuboid; 5] = robe([96, 84, 76], [62, 56, 52], [70, 52, 40]);

const LEG: &[Cuboid] = &[cube([-2.0, -12.0, -2.0], [2.0, 0.0, 2.0], [54, 50, 46], 24)];
const ARM: &[Cuboid] = &[cube([-2.0, -10.0, -2.0], [2.0, 2.0, 2.0], SKIN, 18)];
const SLEEVE: &[Cuboid] = &[cube([-2.1, -5.0, -2.1], [2.1, 2.1, 2.1], [70, 66, 62], 18)];
/// Arms folded across the chest (`CROSSED`): one bar and two forearms.
const CROSSED: &[Cuboid] = &[
    cube([-4.0, -2.0, -2.0], [4.0, 2.0, 2.0], [80, 74, 70], 18),
    cube([-8.0, -2.0, -2.0], [-4.0, 6.0, 2.0], [80, 74, 70], 18),
    cube([4.0, -2.0, -2.0], [8.0, 6.0, 2.0], [80, 74, 70], 18),
];

fn body_of(kind: MobKind) -> &'static [Cuboid] {
    match kind {
        MobKind::Evoker => &EVOKER_BODY,
        MobKind::Illusioner => &ILLUSIONER_BODY,
        MobKind::Pillager => &PILLAGER_BODY,
        _ => &VINDICATOR_BODY,
    }
}

/// An illager in the arm pose its state asks for. The parts follow the
/// humanoid order (body, legs, arms, head) so armor and held items attach.
/// v1.0 gap Track 7: `IllagerModel.setupAnim`: arms swing with the walk and
/// legs at half the usual stride, or the riding pose on a ravager; then
/// the arm pose (an armed attacker swings its weapon down, an unarmed one
/// raises its arms like a zombie).
pub(super) fn pose(m: &Mob, head_rot: Quat, time: f32, alpha: f32) -> Parts {
    let rx = Quat::from_rotation_x;
    let s = m.illager.as_deref();
    let arm_pose = s.map_or(ArmPose::Crossed, |s| s.arm_pose(m.kind, m.main_hand().map(|h| h.item)));
    let age = age_ticks(time);
    // Java's `head.yRot` (Track 7b: on Java's sides, see `Part::on_java_side`).
    let (hx, hy) = (head_x(m), m.head_yaw);
    let riding = m.riding.is_some();
    let mut h = Humanoid::walk(m, 0.5);
    let (mut right_leg, mut left_leg) =
        (Limb { x: h.right_leg, ..Limb::default() }, Limb { x: h.left_leg, ..Limb::default() });
    if riding {
        h.right_arm = Limb { x: -PI / 5.0, ..Limb::default() };
        h.left_arm = h.right_arm;
        right_leg = Limb { x: -1.413_716_7, y: PI / 10.0, z: 0.078_539_82 };
        left_leg = Limb { x: -1.413_716_7, y: -PI / 10.0, z: -0.078_539_82 };
    }
    let swing = m.swing.anim;
    match arm_pose {
        ArmPose::Crossed | ArmPose::Neutral => {}
        ArmPose::Attacking if m.main_hand().is_none() => {
            let (x, y) = motion::zombie_arms(true, true, swing);
            h.right_arm = Limb { x, y: -y, z: 0.0 };
            h.left_arm = Limb { x, y, z: 0.0 };
            h.bob(age);
        }
        ArmPose::Attacking => {
            let (weapon, other) = motion::weapon_down(swing, age);
            h.right_arm = Limb { x: weapon, y: PI / 20.0, z: 0.0 };
            h.left_arm = Limb { x: other, y: -PI / 20.0, z: 0.0 };
            h.bob(age);
        }
        ArmPose::Spellcasting => {
            let wave = (age * 0.6662).cos() * 0.25;
            h.right_arm = Limb { x: wave, y: 0.0, z: PI * 0.75 };
            h.left_arm = Limb { x: wave, y: 0.0, z: -PI * 0.75 };
        }
        ArmPose::BowAndArrow => {
            h.right_arm = Limb { x: -FRAC_PI_2 + hx, y: -0.1 + hy, z: h.right_arm.z };
            h.left_arm = Limb { x: -0.942_477_9 + hx, y: hy - 0.4, z: FRAC_PI_2 };
        }
        ArmPose::CrossbowHold => {
            h.right_arm = Limb { x: -FRAC_PI_2 + hx + 0.1, y: -0.3 + hy, z: h.right_arm.z };
            h.left_arm = Limb { x: -1.5 + hx, y: 0.6 + hy, z: h.left_arm.z };
        }
        ArmPose::CrossbowCharge => {
            // v1.0 gap Track 7b: `AnimationUtils.animateCrossbowCharge`:
            // the pulling arm draws back from 0.4 to 0.85 and up to level
            // over the charge (`ticksUsingItem` over the crossbow's charge
            // duration, the goal's use timer with the partial tick).
            let quick =
                m.main_hand().map_or(0, |c| c.active_enchants().level(crate::enchant::Enchantment::QuickCharge));
            let max = crate::entity::illager::charge_ticks(quick).max(1) as f32;
            let used = s.map_or(0.0, |s| s.use_ticks as f32 + alpha).clamp(0.0, max) / max;
            h.right_arm = Limb { x: -0.970_796_35, y: -0.8, z: h.right_arm.z };
            h.left_arm = Limb {
                x: -0.970_796_35 + (-FRAC_PI_2 + 0.970_796_35) * used,
                y: 0.4 + (0.85 - 0.4) * used,
                z: h.left_arm.z,
            };
        }
        ArmPose::Celebrating => {
            let wave = (age * 0.6662).cos() * 0.05;
            h.right_arm = Limb { x: wave, y: 0.0, z: 2.670_354 };
            h.left_arm = Limb { x: wave, y: 0.0, z: -PI * 0.75 };
        }
    }
    let (right, left) = (h.right_arm.quat(), h.left_arm.quat());
    let crossed = arm_pose == ArmPose::Crossed;
    let head_boxes: &'static [Cuboid] = if m.kind == MobKind::Illusioner { &HEAD_ILLUSIONER } else { &HEAD };
    let mut out = parts![
        part(body_of(m.kind), [0.0; 3], Quat::IDENTITY),
        part(LEG, [-2.0, 12.0, 0.0], left_leg.quat()),
        part(LEG, [2.0, 12.0, 0.0], right_leg.quat()),
        part(if crossed { &[] } else { ARM }, [-5.0, 22.0, 0.0], left),
        part(if crossed { &[] } else { ARM }, [5.0, 22.0, 0.0], right),
        part(head_boxes, [0.0, 24.0, 0.0], head_rot),
    ];
    if crossed {
        out.push(part(CROSSED, [0.0, 20.0, 1.5], rx(-0.75)));
    } else {
        out.push(part(SLEEVE, [-5.0, 22.0, 0.0], left));
        out.push(part(SLEEVE, [5.0, 22.0, 0.0], right));
    }
    // v1.0 gap Track 7b: everything but the head on Java's sides.
    out.java_side(0..5);
    let len = out.len();
    out.java_side(6..len);
    out
}

/// `IllagerRenderer`'s item layer: the vindicator shows its weapon only
/// when aggressive, the illusioner its bow when casting or aggressive, the
/// pillager always; the evoker holds nothing.
pub(super) fn shows_item(m: &Mob) -> bool {
    let Some(s) = m.illager.as_deref() else { return false };
    match m.kind {
        MobKind::Vindicator => s.aggressive,
        MobKind::Illusioner => s.aggressive || s.casting_spell(),
        MobKind::Pillager | MobKind::Vex => true,
        _ => false,
    }
}

const VEX_SKIN: Rgb = [196, 210, 228];
const VEX_HEAD: &[Cuboid] = &[
    cube([-2.5, 0.0, -2.5], [2.5, 5.0, 2.5], VEX_SKIN, 14),
    cube([-1.8, 2.0, 2.5], [-0.6, 3.0, 2.6], [30, 60, 110], 0),
    cube([0.6, 2.0, 2.5], [1.8, 3.0, 2.6], [30, 60, 110], 0),
    // v0.18 Track 6: angry brows and a mouth.
    cube([-1.8, 3.2, 2.5], [-1.2, 3.6, 2.6], [70, 84, 120], 0),
    cube([-1.2, 3.0, 2.5], [-0.6, 3.4, 2.6], [70, 84, 120], 0),
    cube([0.6, 3.0, 2.5], [1.2, 3.4, 2.6], [70, 84, 120], 0),
    cube([1.2, 3.2, 2.5], [1.8, 3.6, 2.6], [70, 84, 120], 0),
    cube([-1.0, 0.8, 2.5], [1.0, 1.2, 2.55], [90, 104, 140], 0),
];
const VEX_BODY: &[Cuboid] = &[
    cube([-1.5, 3.0, -1.0], [1.5, 7.5, 1.0], VEX_SKIN, 14),
    cube([-1.0, 0.0, -0.7], [1.0, 3.0, 0.7], [170, 186, 210], 14),
];
const VEX_ARM: &[Cuboid] = &[cube([-0.75, -5.0, -0.75], [0.75, 0.0, 0.75], VEX_SKIN, 12)];
const VEX_WING_L: &[Cuboid] = &[cube([0.0, -3.0, -0.1], [5.0, 2.0, 0.1], [226, 236, 244], 6)];
const VEX_WING_R: &[Cuboid] = &[cube([-5.0, -3.0, -0.1], [0.0, 2.0, 0.1], [226, 236, 244], 6)];

/// v1.0 gap Track 7b: `VexModel.setupAnim`. The arms hang 36 degrees out,
/// swaying on `cos(ageInTicks * 5.5 degrees) * 0.1`, the body leaning 9
/// degrees; charging, the body straightens and the arms reach forward
/// (70 degrees), or the armed one thrusts up and over (210 degrees). The
/// wings beat on `cos(ageInTicks * 45.84 degrees) * 16.2 degrees` (round
/// this model's own rest angle). The sword hand is the right, on -X.
pub(super) fn vex(m: &Mob, head_rot: Quat, time: f32) -> Parts {
    let rx = Quat::from_rotation_x;
    let age = age_ticks(time);
    let charging = m.illager.as_deref().is_some_and(|s| s.charging);
    let armed = m.main_hand().is_some();
    let bob = (age * 5.5f32.to_radians()).cos() * 0.1;
    let flap = (age * 45.836_624f32.to_radians()).cos() * 16.2f32.to_radians();
    let mut right = Limb { z: PI / 5.0 + bob, ..Limb::default() };
    let mut left = Limb { z: -(PI / 5.0 + bob), ..Limb::default() };
    if charging {
        let reach = |x: f32, y: f32, z: f32| Limb { x, y, z };
        if armed {
            right = reach(PI * 7.0 / 6.0, PI / 12.0, -0.471_238_88 - bob);
        } else {
            right = reach(-1.221_730_5, PI / 12.0, -0.471_238_88 - bob);
            left = reach(-1.221_730_5, -PI / 12.0, 0.471_238_88 + bob);
        }
    }
    // The limbs as Java's turned through the origin (left on -X), then
    // moved onto Java's sides like the humanoids'.
    let mut p = parts![
        part(VEX_BODY, [0.0; 3], if charging { Quat::IDENTITY } else { rx(PI / 20.0) }),
        part(&[], [0.0; 3], Quat::IDENTITY),
        part(&[], [0.0; 3], Quat::IDENTITY),
        part(VEX_ARM, [-2.0, 7.0, 0.0], left.quat()),
        part(VEX_ARM, [2.0, 7.0, 0.0], right.quat()),
        part(VEX_HEAD, [0.0, 7.5, 0.0], head_rot),
        part(VEX_WING_L, [0.5, 6.5, -1.0], Quat::from_rotation_y(0.5 + flap)),
        part(VEX_WING_R, [-0.5, 6.5, -1.0], Quat::from_rotation_y(-0.5 - flap)),
    ];
    p.java_side(3..5);
    p
}

const HIDE: Rgb = [86, 80, 74];
const RAVAGER_BODY: &[Cuboid] = &[
    cube([-7.0, 14.0, -12.0], [7.0, 29.0, 8.0], HIDE, 30),
    cube([-8.0, 18.0, 2.0], [8.0, 33.0, 13.0], [96, 88, 80], 30),
    cube([-6.0, 13.0, -12.5], [6.0, 16.0, 12.0], [72, 66, 60], 20),
    // v1.0 Track 10: a dark mane along the hump, a scar on its flank.
    cube([-1.5, 33.0, 3.0], [1.5, 33.4, 13.0], [62, 56, 50], 10),
    cube([-8.1, 24.0, 5.0], [-8.0, 25.0, 9.0], [128, 118, 106], 0),
];
const RAVAGER_NECK: &[Cuboid] = &[cube([-5.0, -5.0, 0.0], [5.0, 5.0, 9.0], [90, 84, 78], 26)];
const RAVAGER_HEAD: &[Cuboid] = &[
    cube([-7.0, -4.0, 0.0], [7.0, 10.0, 12.0], [100, 94, 86], 26),
    cube([-2.0, -2.0, 12.0], [2.0, 4.0, 15.0], [70, 64, 58], 18),
    cube([-10.0, 6.0, 3.0], [-7.0, 9.0, 6.0], [214, 204, 180], 10),
    cube([-11.0, 9.0, 3.0], [-8.0, 15.0, 6.0], [214, 204, 180], 10),
    cube([7.0, 6.0, 3.0], [10.0, 9.0, 6.0], [214, 204, 180], 10),
    cube([8.0, 9.0, 3.0], [11.0, 15.0, 6.0], [214, 204, 180], 10),
    cube([-5.0, 5.0, 12.0], [-3.0, 6.0, 12.1], [210, 160, 60], 0),
    cube([3.0, 5.0, 12.0], [5.0, 6.0, 12.1], [210, 160, 60], 0),
    // v1.0 Track 10: slit pupils with a glint, a heavy brow over each eye
    // and nostrils on the nose.
    cube([-4.0, 5.0, 12.1], [-3.5, 6.0, 12.15], [40, 26, 14], 0),
    cube([3.5, 5.0, 12.1], [4.0, 6.0, 12.15], [40, 26, 14], 0),
    cube([-5.0, 5.6, 12.1], [-4.6, 6.0, 12.15], [252, 236, 190], 0),
    cube([4.6, 5.6, 12.1], [5.0, 6.0, 12.15], [252, 236, 190], 0),
    cube([-6.0, 6.0, 12.0], [-2.0, 7.0, 12.6], [70, 64, 58], 8),
    cube([2.0, 6.0, 12.0], [6.0, 7.0, 12.6], [70, 64, 58], 8),
    cube([-1.5, 0.5, 15.0], [-0.5, 1.5, 15.1], [34, 30, 28], 0),
    cube([0.5, 0.5, 15.0], [1.5, 1.5, 15.1], [34, 30, 28], 0),
];
const RAVAGER_JAW: &[Cuboid] = &[
    cube([-6.0, -3.0, 0.0], [6.0, 0.0, 11.0], [76, 70, 64], 20),
    cube([-5.0, 0.0, 9.0], [-4.0, 1.5, 10.0], [230, 224, 210], 0),
    cube([4.0, 0.0, 9.0], [5.0, 1.5, 10.0], [230, 224, 210], 0),
];
const RAVAGER_LEG: &[Cuboid] = &[
    cube([-3.0, -14.0, -3.0], [3.0, 0.0, 3.0], [78, 72, 66], 24),
    cube([-3.2, -14.0, -3.2], [3.2, -12.0, 3.2], [52, 48, 44], 12),
];

/// The ravager: a heavy beast whose head lunges and jaw drops when it bites
/// or roars, and shakes while stunned.
/// v1.0 gap Track 7: `RavagerModel.setupAnim`: legs swing `0.4 * speed`;
/// a bite thrusts the neck forward up to 12 pixels (a cubed triangle wave
/// over its 10 ticks) and opens the mouth; stunned, the neck droops and
/// sways; a roar opens the mouth wide. Counters run down with the partial
/// tick as `RavagerRenderer` reads them.
pub(super) fn ravager(m: &Mob, head_rot: Quat, alpha: f32) -> Parts {
    let rx = Quat::from_rotation_x;
    let s = m.illager.as_deref();
    let (attack, stun, roar) = s.map_or((0, 0, 0), |s| (s.attack_tick, s.stun_tick, s.roar_tick));
    let left = |t: i32| if t > 0 { t as f32 - alpha } else { 0.0 };
    let (attack, stun) = (left(attack), left(stun));
    let (mut neck_x, mut neck_z, mut neck_rot) = (0.0, 0.0, 0.0);
    let mouth = if attack > 0.0 {
        let scaled = (1.0 + motion::triangle_wave(attack, 10.0)) * 0.5;
        neck_z = 12.0 - scaled * scaled * scaled * 12.0;
        if attack > 5.0 { ((-4.0 + attack) / 4.0).sin() * PI * 0.4 } else { PI / 20.0 * (PI * attack / 10.0).sin() }
    } else if stun > 0.0 {
        neck_rot = 0.219_911_49;
        neck_x = ((stun / 40.0) * 10.0).sin() * 3.0;
        PI * 0.05
    } else if roar > 0 {
        let anim = (20.0 - roar as f32 + alpha) / 20.0;
        FRAC_PI_2 * (anim * PI * 0.25).sin()
    } else {
        PI * 0.01
    };
    let lunge = rx(neck_rot);
    let head = lunge * head_rot;
    let leg = walk_cos(m) * 0.4;
    parts![
        part(RAVAGER_BODY, [0.0; 3], Quat::IDENTITY),
        part(RAVAGER_LEG, [-5.0, 14.0, -8.0], rx(-leg)),
        part(RAVAGER_LEG, [5.0, 14.0, -8.0], rx(leg)),
        part(RAVAGER_LEG, [-5.0, 14.0, 6.0], rx(leg)),
        part(RAVAGER_LEG, [5.0, 14.0, 6.0], rx(-leg)),
        part(RAVAGER_NECK, [neck_x, 24.0, 12.0 + neck_z], lunge),
        part(RAVAGER_HEAD, [neck_x, 22.0, 20.0 + neck_z], head),
        part(RAVAGER_JAW, [neck_x, 18.0, 21.0 + neck_z], head * rx(mouth)),
    ]
}

const FANG_BASE: &[Cuboid] = &[cube([-2.5, 0.0, -2.5], [2.5, 2.0, 2.5], [112, 108, 92], 18)];
const FANG_JAW: &[Cuboid] = &[
    cube([-2.0, 0.0, -2.0], [2.0, 10.0, 0.0], [196, 186, 150], 16),
    cube([-1.5, 10.0, -1.5], [-0.5, 12.0, -0.5], [236, 230, 210], 0),
    cube([0.5, 10.0, -1.5], [1.5, 12.0, -0.5], [236, 230, 210], 0),
];

/// Evoker fangs: two jaws rising out of the ground and snapping shut
/// (`EvokerFangsModel`'s shape, original boxes).
pub fn build_fangs(fangs: &[Fang], camera: DVec3, out: &mut EntityMesh) {
    for f in fangs {
        let p = f.progress();
        if p <= 0.0 {
            continue;
        }
        let rise = ((p * 2.0).min(1.0) * 12.0 - 12.0) / 16.0;
        let snap = if p > 0.9 { (1.0 - p) * 10.0 } else { 1.0 };
        let open = (p * PI).sin() * 0.9 * snap;
        let rel = (f.pos - camera).as_vec3() + Vec3::Y * rise;
        let body = Quat::from_rotation_y(FRAC_PI_2 - f.yaw);
        let light = ([230, 0, 0, 0], 0);
        for (i, (boxes, rot, pivot)) in [
            (FANG_BASE, Quat::IDENTITY, Vec3::ZERO),
            (FANG_JAW, Quat::from_rotation_x(open), Vec3::new(0.0, 2.0, 0.0)),
            (FANG_JAW, Quat::from_rotation_y(PI) * Quat::from_rotation_x(open), Vec3::new(0.0, 2.0, 0.0)),
        ]
        .into_iter()
        .enumerate()
        {
            let r = body * rot;
            for c in boxes {
                push_cuboid(out, c, &|v: Vec3| rel + body * (pivot + rot * v) / 16.0, r, light, (FIRE, 0.0), i as f32);
            }
        }
    }
}
