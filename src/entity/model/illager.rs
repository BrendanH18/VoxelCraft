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
pub(super) fn pose(m: &Mob, swing: f32, head_rot: Quat, time: f32) -> Parts {
    let rx = Quat::from_rotation_x;
    let rz = Quat::from_rotation_z;
    let s = m.illager.as_deref();
    let arm_pose = s.map_or(ArmPose::Crossed, |s| s.arm_pose(m.kind, m.main_hand().map(|h| h.item)));
    let age = time * 20.0;
    let pitch = m.head_pitch;
    let chop = if m.attack_anim > 0.0 { (m.attack_anim / 0.35 * PI).sin() * 0.9 } else { 0.0 };
    let (right, left): (Quat, Quat) = match arm_pose {
        ArmPose::Crossed => (Quat::IDENTITY, Quat::IDENTITY),
        ArmPose::Attacking => (rx(-1.2 + chop - swing * 0.2), rx(-1.0 + swing * 0.2)),
        ArmPose::Spellcasting => {
            let wave = (age * 0.6662).cos() * 0.25;
            (rz(2.36) * rx(wave), rz(-2.36) * rx(-wave))
        }
        ArmPose::BowAndArrow => (rx(-FRAC_PI_2 - pitch), rx(-FRAC_PI_2 - pitch) * Quat::from_rotation_y(-0.5)),
        ArmPose::CrossbowHold => (rx(-FRAC_PI_2 - pitch), rx(-FRAC_PI_2 - pitch) * Quat::from_rotation_y(-0.6)),
        ArmPose::CrossbowCharge => (rx(-0.97), rx(-1.4) * Quat::from_rotation_y(0.4)),
        ArmPose::Celebrating => {
            let wave = (age * 0.6662).cos() * 0.05;
            (rx(-PI + wave) * rz(0.3), rx(-PI - wave) * rz(-0.3))
        }
        ArmPose::Neutral => (rx(swing * 0.6), rx(-swing * 0.6)),
    };
    let crossed = arm_pose == ArmPose::Crossed;
    let head_boxes: &'static [Cuboid] = if m.kind == MobKind::Illusioner { &HEAD_ILLUSIONER } else { &HEAD };
    let mut out = parts![
        part(body_of(m.kind), [0.0; 3], Quat::IDENTITY),
        part(LEG, [-2.0, 12.0, 0.0], rx(swing)),
        part(LEG, [2.0, 12.0, 0.0], rx(-swing)),
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
];
const VEX_BODY: &[Cuboid] = &[
    cube([-1.5, 3.0, -1.0], [1.5, 7.5, 1.0], VEX_SKIN, 14),
    cube([-1.0, 0.0, -0.7], [1.0, 3.0, 0.7], [170, 186, 210], 14),
];
const VEX_ARM: &[Cuboid] = &[cube([-0.75, -5.0, -0.75], [0.75, 0.0, 0.75], VEX_SKIN, 12)];
const VEX_WING_L: &[Cuboid] = &[cube([0.0, -3.0, -0.1], [5.0, 2.0, 0.1], [226, 236, 244], 6)];
const VEX_WING_R: &[Cuboid] = &[cube([-5.0, -3.0, -0.1], [0.0, 2.0, 0.1], [226, 236, 244], 6)];

/// The vex: arms down, or both raised toward its target while charging,
/// wings beating fast.
pub(super) fn vex(m: &Mob, head_rot: Quat, time: f32) -> Parts {
    let rx = Quat::from_rotation_x;
    let charging = m.illager.as_deref().is_some_and(|s| s.charging);
    let flap = (time * 20.0 * 0.8).cos() * 0.6;
    // Arms reach forward, raised high while charging.
    let arm = if charging { rx(-2.2) } else { rx(-1.1) };
    parts![
        part(VEX_BODY, [0.0; 3], if charging { rx(0.5) } else { Quat::IDENTITY }),
        part(&[], [0.0; 3], Quat::IDENTITY),
        part(&[], [0.0; 3], Quat::IDENTITY),
        part(VEX_ARM, [-2.0, 7.0, 0.0], arm),
        part(VEX_ARM, [2.0, 7.0, 0.0], arm),
        part(VEX_HEAD, [0.0, 7.5, 0.0], head_rot),
        part(VEX_WING_L, [0.5, 6.5, -1.0], Quat::from_rotation_y(0.5 + flap)),
        part(VEX_WING_R, [-0.5, 6.5, -1.0], Quat::from_rotation_y(-0.5 - flap)),
    ]
}

const HIDE: Rgb = [86, 80, 74];
const RAVAGER_BODY: &[Cuboid] = &[
    cube([-7.0, 14.0, -12.0], [7.0, 29.0, 8.0], HIDE, 30),
    cube([-8.0, 18.0, 2.0], [8.0, 33.0, 13.0], [96, 88, 80], 30),
    cube([-6.0, 13.0, -12.5], [6.0, 16.0, 12.0], [72, 66, 60], 20),
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
pub(super) fn ravager(m: &Mob, swing: f32, head_rot: Quat, time: f32) -> Parts {
    let rx = Quat::from_rotation_x;
    let s = m.illager.as_deref();
    let (attack, stun, roar) = s.map_or((0, 0, 0), |s| (s.attack_tick, s.stun_tick, s.roar_tick));
    let bite = if attack > 0 { (attack as f32 / 10.0 * PI).sin() } else { 0.0 };
    let shout = if roar > 0 { (roar as f32 / 20.0 * PI).sin() } else { 0.0 };
    let shake = if stun > 0 { (time * 40.0).sin() * 0.15 } else { 0.0 };
    let jaw = rx(0.6 * bite + 0.9 * shout);
    let lunge = rx(-0.3 * bite + 0.2 * shout) * Quat::from_rotation_y(shake);
    let head = head_rot * lunge;
    parts![
        part(RAVAGER_BODY, [0.0; 3], Quat::IDENTITY),
        part(RAVAGER_LEG, [-5.0, 14.0, -8.0], rx(swing * 0.6)),
        part(RAVAGER_LEG, [5.0, 14.0, -8.0], rx(-swing * 0.6)),
        part(RAVAGER_LEG, [-5.0, 14.0, 6.0], rx(-swing * 0.6)),
        part(RAVAGER_LEG, [5.0, 14.0, 6.0], rx(swing * 0.6)),
        part(RAVAGER_NECK, [0.0, 24.0, 12.0], lunge),
        part(RAVAGER_HEAD, [0.0, 22.0, 20.0], head),
        part(RAVAGER_JAW, [0.0, 18.0, 21.0], head * jaw),
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
