//! Procedural Java-like equine proportions, coats, markings and equipment.
use super::{Cuboid, Mob, MobKind, Parts, age_ticks, cube, motion, part, walk_cos};
use glam::{Quat, Vec3};
use std::f32::consts::{FRAC_PI_4, PI};
const COLORS: [[u8; 3]; 9] = [
    [236, 232, 218],
    [208, 177, 126],
    [159, 104, 57],
    [108, 66, 38],
    [40, 35, 31],
    [132, 126, 116],
    [72, 55, 43],
    [222, 218, 200],
    [94, 124, 68],
];
const fn bodies() -> [[Cuboid; 2]; 9] {
    let mut out = [[cube([0.; 3], [0.; 3], [0; 3], 0); 2]; 9];
    let mut i = 0;
    while i < 9 {
        out[i] = [
            cube([-5., 11., -10.], [5., 21., 9.], COLORS[i], 22),
            cube([-1., 20., -9.], [1., 22., 10.], [44, 36, 26], 20),
        ];
        i += 1;
    }
    out
}
const BODIES: [[Cuboid; 2]; 9] = bodies();
/// v1.0 gap Track 7b: the tail on its own joint at the top of the rump
/// (the same box, in the same place at rest).
const TAIL_AT: [f32; 3] = [0., 21., -9.];
const TAIL: &[Cuboid] = &[cube([-1.5, -7., -7.], [1.5, 0., 0.], [48, 38, 28], 20)];
const SKELETON_TAIL_AT: [f32; 3] = [0., 20., -9.];
const SKELETON_TAIL: &[Cuboid] = &[cube([-1., -6., -7.], [1., 0., 0.], [222, 218, 200], 12)];
const fn heads() -> [[Cuboid; 7]; 9] {
    let mut out = [[cube([0.; 3], [0.; 3], [0; 3], 0); 7]; 9];
    let mut i = 0;
    while i < 9 {
        out[i] = [
            cube([-2.5, -1., -1.], [2.5, 7., 4.], COLORS[i], 18),
            cube([-2.5, 4., 2.], [2.5, 9., 9.], COLORS[i], 18),
            cube([-2., 7., 1.], [-1., 11., 3.], COLORS[i], 14),
            cube([1., 7., 1.], [2., 11., 3.], COLORS[i], 14),
            cube([-2.6, 7., 5.], [2.6, 8., 6.], [30, 26, 22], 0),
            // v0.18 Track 6: nostrils on the muzzle.
            cube([-1.8, 5., 9.], [-0.8, 6., 9.05], [30, 26, 22], 0),
            cube([0.8, 5., 9.], [1.8, 6., 9.05], [30, 26, 22], 0),
        ];
        i += 1;
    }
    out
}
const HEADS: [[Cuboid; 7]; 9] = heads();
const fn legs() -> [[Cuboid; 2]; 9] {
    let mut out = [[cube([0.; 3], [0.; 3], [0; 3], 0); 2]; 9];
    let mut i = 0;
    while i < 9 {
        out[i] = [
            cube([-1.6, -11., -1.6], [1.6, 0., 1.6], COLORS[i], 18),
            cube([-1.8, -12., -1.8], [1.8, -9., 1.8], [50, 44, 38], 12),
        ];
        i += 1;
    }
    out
}
const LEGS: [[Cuboid; 2]; 9] = legs();
const EARS: &[Cuboid] =
    &[cube([-2., 10., 1.], [-1., 13., 3.], [110, 100, 86], 16), cube([1., 10., 1.], [2., 13., 3.], [110, 100, 86], 16)];
const CHESTS: &[Cuboid] =
    &[cube([-9., 10., -9.], [-5., 18., 0.], [130, 86, 40], 20), cube([5., 10., -9.], [9., 18., 0.], [130, 86, 40], 20)];
const SADDLE: &[Cuboid] = &[
    cube([-5.3, 20.7, -4.], [5.3, 22., 5.], [132, 72, 36], 18),
    cube([-6., 13., -1.], [-5.5, 21., 2.], [160, 146, 130], 8),
    cube([5.5, 13., -1.], [6., 21., 2.], [160, 146, 130], 8),
];
const PIG_SADDLE: &[Cuboid] = &[cube([-4.5, 13.8, -5.], [4.5, 15., 5.], [132, 72, 36], 16)];
const STRIDER_SADDLE: &[Cuboid] = &[cube([-6.3, 26., -6.3], [6.3, 27.5, 6.3], [132, 72, 36], 16)];
const MARKINGS: [[Cuboid; 2]; 4] = [
    [
        cube([-5.1, 13., -5.], [-5., 21., 5.], [240, 236, 224], 6),
        cube([5., 13., -5.], [5.1, 21., 5.], [240, 236, 224], 6),
    ],
    [
        cube([-5.1, 16., -10.], [-5., 21., 9.], [240, 236, 224], 12),
        cube([5., 16., -10.], [5.1, 21., 9.], [240, 236, 224], 12),
    ],
    [
        cube([-5.1, 16., -5.], [-5., 19., -1.], [240, 236, 224], 30),
        cube([5., 12., 3.], [5.1, 15., 8.], [240, 236, 224], 30),
    ],
    [cube([-5.1, 16., -5.], [-5., 19., -1.], [36, 30, 24], 30), cube([5., 12., 3.], [5.1, 15., 8.], [36, 30, 24], 30)],
];
/// Sentinel recoloured from the leather horse armor's `dyed_color` (else
/// `#A06540`) before emitting vertices, as `equipment/leather.json`'s
/// dyeable `horse_body` layer is tinted.
const LEATHER_HORSE: [u8; 3] = [1, 2, 4];
/// Leather, iron, gold, diamond, then copper and netherite (v0.14).
const fn armors() -> [[Cuboid; 3]; 6] {
    let colors = [LEATHER_HORSE, [210, 210, 215], [235, 194, 54], [60, 220, 210], [196, 112, 80], [80, 72, 78]];
    let mut out = [[cube([0.; 3], [0.; 3], [0; 3], 0); 3]; 6];
    let mut i = 0;
    while i < 6 {
        out[i] = [
            cube([-5.2, 11., -10.2], [-5., 20., 9.2], colors[i], 8),
            cube([5., 11., -10.2], [5.2, 20., 9.2], colors[i], 8),
            cube([-5.2, 21., -10.2], [5.2, 21.2, 9.2], colors[i], 8),
        ];
        i += 1;
    }
    out
}
const ARMOR: [[Cuboid; 3]; 6] = armors();
/// `AbstractEquineModel`'s body pivot (Java's (0, 11, 5)), which rearing
/// turns the body about.
const REAR_PIVOT: Vec3 = Vec3::new(0., 13., -5.);

pub(super) fn pose(m: &Mob, time: f32, alpha: f32) -> Parts {
    let s = m.mount.as_ref().unwrap();
    let color = match m.kind {
        MobKind::SkeletonHorse => 7,
        MobKind::ZombieHorse => 8,
        MobKind::Horse => s.variant as usize,
        _ => 5,
    };
    // v1.0 gap Track 7: `AbstractEquineModel`: the front legs swing 0.8
    // and the hind legs 0.5 of `cos(pos * 0.6662 + pi) * speed` (a fifth
    // as fast in water), and above speed 0.2 the head nods.
    let (pos, speed) = (m.limb_phase, m.limb_amp);
    let water = if m.in_water { 0.2 } else { 1.0 };
    let leg = (water * pos * motion::WALK_FREQ + PI).cos() * speed;
    let nod = if speed > 0.2 { (pos * 0.8).cos() * 0.15 * speed } else { 0.0 };
    let head_y = match m.kind {
        MobKind::Donkey => 13.,
        MobKind::Mule => 14.,
        _ => 15.,
    };
    // v1.0 gap Track 7b: rearing (`standAnim`, Track 3b's
    // `mounts::State::stand_anim`) turns the body up 45 degrees about its pivot, carrying the
    // head, which lifts 15 degrees less, and the front legs, which paw on
    // `cos(ageInTicks * 0.6 + pi)` from 60 degrees forward, while the hind
    // legs plant at 15 degrees; the walk fades out as it rises.
    let age = age_ticks(time);
    let stand = s.stand_anim(alpha);
    let walk = 1.0 - stand;
    let rear = Quat::from_rotation_x(-FRAC_PI_4 * stand);
    let paw = (age * 0.6 + PI).cos();
    let head = Quat::from_rotation_x(PI / 6.0 * stand)
        * Quat::from_rotation_y(-m.head_yaw)
        * Quat::from_rotation_x(-0.35 - m.head_pitch + nod * walk);
    let mut p = Parts::new();
    p.push(part(
        if m.kind == MobKind::SkeletonHorse { SKELETON_BODY } else { &BODIES[color] },
        [0.; 3],
        Quat::IDENTITY,
    ));
    // The front legs turn with the body; their pawing angle is absolute,
    // so the body's turn comes back off it.
    for (x, phase, sign) in [(-3.5, leg * 0.8, 1.0), (3.5, -leg * 0.8, -1.0)] {
        let lift = (-PI / 3.0 + sign * paw) * stand + phase * walk;
        p.push(part(&LEGS[color], [x, 12., 7.], Quat::from_rotation_x(FRAC_PI_4 * stand + lift)));
    }
    p.push(part(&HEADS[color], [0., head_y, 6.], head));
    // `tail.xRot` gains `0.75 * speed` and the tail sits lower and further
    // back with the walk; while `tailCounter` runs (8 ticks, one tick in
    // 200) it swishes `cos(ageInTicks * 0.7)` (Java's `yRot` on this
    // model's sides).
    let swish = if m.anim.tail_counter > 0 { -(age * 0.7).cos() } else { 0.0 };
    let (tail, tail_at) =
        if m.kind == MobKind::SkeletonHorse { (SKELETON_TAIL, SKELETON_TAIL_AT) } else { (TAIL, TAIL_AT) };
    let scale = if m.is_baby() { 0.5 } else { 1.0 };
    p.push(part(
        tail,
        [tail_at[0], tail_at[1] - speed * scale, tail_at[2] - 2.0 * speed * scale],
        Quat::from_rotation_y(swish) * Quat::from_rotation_x(0.75 * speed),
    ));
    if matches!(m.kind, MobKind::Donkey | MobKind::Mule) {
        p.push(part(EARS, [0., head_y, 6.], head));
    }
    if s.marking > 0 && m.kind == MobKind::Horse {
        p.push(part(&MARKINGS[s.marking as usize - 1], [0.; 3], Quat::IDENTITY));
    }
    if s.chest {
        p.push(part(CHESTS, [0.; 3], Quat::IDENTITY));
    }
    equipment(m, &mut p);
    if stand > 0.0 {
        p.turn_about(REAR_PIVOT, rear);
    }
    // The hind legs stay planted, angled back a little while rearing.
    for (x, phase) in [(-3.5, -leg * 0.5), (3.5, leg * 0.5)] {
        p.push(part(&LEGS[color], [x, 12., -8.], Quat::from_rotation_x(PI / 12.0 * stand + phase * walk)));
    }
    p
}
pub(super) fn equipment(m: &Mob, p: &mut Parts) {
    let Some(s) = m.mount.as_ref() else { return };
    if s.saddled() {
        p.push(part(
            match m.kind {
                MobKind::Pig => PIG_SADDLE,
                MobKind::Strider => STRIDER_SADDLE,
                _ => SADDLE,
            },
            [0.; 3],
            Quat::IDENTITY,
        ));
    }
    if let Some(stack) = s.slots[1] {
        use crate::item::ArmorMaterial;
        let index = match stack.item.body_armor() {
            Some((crate::equipment::BodyArmor::Horse, material)) => match material {
                ArmorMaterial::Leather => 0,
                ArmorMaterial::Iron => 1,
                ArmorMaterial::Gold => 2,
                ArmorMaterial::Diamond => 3,
                ArmorMaterial::Copper => 4,
                ArmorMaterial::Netherite => 5,
                _ => return,
            },
            _ => return,
        };
        p.push(part(&ARMOR[index], [0.; 3], Quat::IDENTITY));
    }
}

/// Llama coat colours: creamy, white, brown and gray.
const LLAMA_COATS: [[u8; 3]; 4] = [[221, 207, 178], [233, 233, 228], [128, 92, 60], [130, 128, 124]];
/// The trader llama's head and legs are drawn in this colour; wild llamas recolour it.
const LLAMA_FUR: [u8; 3] = [221, 207, 178];
/// Sentinel recoloured from the equipped carpet before emitting vertices.
const CARPET: [u8; 3] = [1, 2, 3];
const LY: f32 = super::trader_model::LLAMA_Y;
const LLAMA_BODY: &[Cuboid] = &[cube([-4.5, 14.0 * LY, -7.0], [4.5, 23.0 * LY, 7.0], LLAMA_FUR, 20)];
static LLAMA_DECOR: &[Cuboid] = &[
    cube([-4.7, 21.0 * LY, -7.2], [4.7, 23.3 * LY, 7.2], CARPET, 10),
    cube([-4.8, 15.5 * LY, -5.0], [-4.6, 21.0 * LY, 5.0], CARPET, 10),
    cube([4.6, 15.5 * LY, -5.0], [4.8, 21.0 * LY, 5.0], CARPET, 10),
];
const LLAMA_CHESTS: &[Cuboid] = &[
    cube([-7.5, 15.0 * LY, -6.0], [-4.5, 21.0 * LY, 2.0], [130, 86, 40], 20),
    cube([4.5, 15.0 * LY, -6.0], [7.5, 21.0 * LY, 2.0], [130, 86, 40], 20),
];
pub(super) fn llama(m: &Mob) -> Parts {
    use super::trader_model::{LLAMA_HEAD, LLAMA_LEG};
    let s = m.mount.as_ref();
    // `LlamaModel`: the quadruped's `cos(pos * 0.6662) * 1.4 * speed`.
    let swing = walk_cos(m) * motion::LEG_SWING;
    let rx = Quat::from_rotation_x;
    let head = Quat::from_rotation_y(-m.head_yaw) * rx(-m.head_pitch);
    let mut p = Parts::new();
    p.push(part(LLAMA_BODY, [0.; 3], Quat::IDENTITY));
    p.push(part(LLAMA_HEAD, [0., 20. * LY, 5.], head));
    for (x, z, phase) in [(-3., 5., swing), (3., 5., -swing), (-3., -5., -swing), (3., -5., swing)] {
        p.push(part(LLAMA_LEG, [x, 14. * LY, z], rx(phase)));
    }
    if s.is_some_and(|s| s.slots[1].is_some()) {
        p.push(part(LLAMA_DECOR, [0.; 3], Quat::IDENTITY));
    }
    if s.is_some_and(|s| s.chest) {
        p.push(part(LLAMA_CHESTS, [0.; 3], Quat::IDENTITY));
    }
    p
}

const SAND: [u8; 3] = [219, 169, 95];
/// v1.0 gap Track 3b: the camel's parts follow Java's `AdultCamelModel`
/// hierarchy so its keyframes can move them: the body pivots where Java's
/// does (20 pixels up, 9.5 back) and carries the head, the tail and the
/// saddle; the legs pivot 23 pixels up, as Java's. The boxes are where they
/// were (the look Track 7 approved), given from those pivots.
const CAMEL_BODY_PIVOT: Vec3 = Vec3::new(0.0, 20.0, -9.5);
const CAMEL_BODY: &[Cuboid] =
    &[cube([-7.5, 0., -3.5], [7.5, 10., 22.5], SAND, 18), cube([-4.5, 10., 3.5], [4.5, 15., 13.5], [205, 152, 80], 18)];
/// The tail, from its root on the body's back.
const CAMEL_TAIL: &[Cuboid] = &[cube([-1., -6., -0.5], [1., 0., 0.5], [160, 115, 60], 10)];
const CAMEL_TAIL_ROOT: Vec3 = Vec3::new(0.0, 8.0, -4.0);
/// The head's pivot on the body (23 pixels up, 12 forward, at rest).
const CAMEL_HEAD_ROOT: Vec3 = Vec3::new(0.0, 3.0, 21.5);
const CAMEL_LEG: &[Cuboid] = &[
    cube([-1.5, -22., -1.5], [1.5, -2., 1.5], SAND, 16),
    cube([-1.8, -23., -1.8], [1.8, -20., 1.8], [130, 95, 55], 10),
];
const CAMEL_HEAD: &[Cuboid] = &[
    cube([-3., -2., -2.], [3., 13., 4.], SAND, 16),
    cube([-3.5, 9., 0.], [3.5, 15., 12.], SAND, 16),
    cube([-3.6, 12., 6.], [3.6, 13., 7.], [40, 32, 24], 0),
    cube([-4.5, 13., 1.], [-3.5, 14., 3.], [205, 152, 80], 10),
    cube([3.5, 13., 1.], [4.5, 14., 3.], [205, 152, 80], 10),
    // v0.18 Track 6: nostrils, a split lip and a darker shaggy brow.
    cube([-2.2, 13., 12.], [-1., 14., 12.05], [96, 66, 36], 0),
    cube([1., 13., 12.], [2.2, 14., 12.05], [96, 66, 36], 0),
    cube([-0.2, 9.5, 12.], [0.2, 12., 12.05], [150, 108, 60], 0),
    cube([-3.55, 13.2, 5.5], [3.55, 13.8, 7.6], [176, 126, 66], 0),
];
const CAMEL_SADDLE: &[Cuboid] = &[
    cube([-5., 15., 2.5], [5., 16.2, 14.5], [132, 72, 36], 16),
    cube([-7.7, 4., 7.5], [-7.5, 15., 10.5], [160, 146, 130], 8),
    cube([7.5, 4., 7.5], [7.7, 15., 10.5], [160, 146, 130], 8),
];

/// A Java part rotation in degrees (`xRot`, `yRot`, `zRot`, applied
/// `rotationZYX`) in the model's axes (+Y up, +Z forward): x keeps its
/// sense, y and z turn the other way.
fn java_rot([x, y, z]: [f32; 3]) -> Quat {
    Quat::from_rotation_z(-z.to_radians())
        * Quat::from_rotation_y(-y.to_radians())
        * Quat::from_rotation_x(x.to_radians())
}

/// A keyframe position (`posVec`, pixels, y up) in the model's axes.
fn java_pos([x, y, z]: [f32; 3]) -> Vec3 {
    Vec3::new(x, y, -z)
}

pub(super) fn camel(m: &Mob) -> Parts {
    use super::camel_anim::{self as anim, Bone};
    let s = m.mount.as_ref();
    // v1.0 gap Track 3b: `Camel.setupAnimationStates` and `CamelModel`:
    // sitting down plays `CAMEL_SIT` for its 40 ticks, then holds
    // `CAMEL_SIT_POSE`; standing up plays `CAMEL_STANDUP` for its 52; a
    // dash loops `CAMEL_DASH` while `DASH` is on.
    let mut off = anim::Offsets::default();
    let mut walking = true;
    if let Some(s) = s {
        let t = s.pose_time as f32;
        if s.camel_sitting {
            walking = false;
            if s.pose_time < crate::entity::mounts::camel::SITDOWN_TICKS {
                anim::SIT.apply(t, &mut off);
            } else {
                anim::SIT_POSE.apply(t, &mut off);
            }
        } else {
            if s.dashing {
                // `updateWalkAnimation`: no walk while the dash plays.
                walking = false;
                anim::DASH.apply(f32::from(s.dash_ticks), &mut off);
            }
            if s.in_pose_transition() {
                anim::STAND_UP.apply(t, &mut off);
            }
        }
    }
    // `CamelModel` plays a keyframe walk; these legs keep its loop and
    // strength (v1.0 gap Track 7).
    let (phase, scale) = motion::keyframe_phase(m.limb_phase, m.limb_amp, motion::CAMEL_WALK);
    let swing = if walking { phase.sin() * 0.7 * scale } else { 0.0 };
    // `applyHeadRotation`: yaw within 30 degrees, pitch -25 to 45 (Java's
    // down is positive), tipped down after a dash by 45 × cooldown / 55.
    let mut pitch = (-m.head_pitch.to_degrees()).clamp(-25.0, 45.0);
    if let Some(cooldown) = s.map(|s| s.dash_cooldown).filter(|&c| c > 0) {
        pitch = (pitch + 45.0 * f32::from(cooldown) / 55.0).clamp(-25.0, 70.0);
    }
    let yaw = m.head_yaw.to_degrees().clamp(-30.0, 30.0);
    let head_anim = off.rot(Bone::Head);
    let body_rot = java_rot(off.rot(Bone::Body));
    let body_pivot = CAMEL_BODY_PIVOT + java_pos(off.pos(Bone::Body));
    let on_body = |at: Vec3| (body_pivot + body_rot * at).to_array();
    let head = body_rot * java_rot([pitch + head_anim[0], yaw + head_anim[1], head_anim[2]]);
    let mut p = Parts::new();
    p.push(part(CAMEL_BODY, body_pivot.to_array(), body_rot));
    p.push(part(CAMEL_HEAD, on_body(CAMEL_HEAD_ROOT), head));
    p.push(part(CAMEL_TAIL, on_body(CAMEL_TAIL_ROOT), body_rot * java_rot(off.rot(Bone::Tail))));
    for (bone, x, z, phase) in [
        (Bone::RightFrontLeg, -4.5, 9., swing),
        (Bone::LeftFrontLeg, 4.5, 9., -swing),
        (Bone::RightHindLeg, -4.5, -9., -swing),
        (Bone::LeftHindLeg, 4.5, -9., swing),
    ] {
        let [rx, ry, rz] = off.rot(bone);
        let pivot = Vec3::new(x, 23., z) + java_pos(off.pos(bone));
        let rot = java_rot([rx + phase.to_degrees(), ry, rz]);
        p.push(part(CAMEL_LEG, pivot.to_array(), rot));
    }
    if s.is_some_and(|s| s.saddled()) {
        p.push(part(CAMEL_SADDLE, body_pivot.to_array(), body_rot));
    }
    p
}

/// Llama coats and carpets, and dyed leather horse armor.
pub(super) fn colour(m: &Mob, c: &mut Cuboid) {
    let Some(s) = m.mount.as_ref() else { return };
    if c.color == LEATHER_HORSE {
        c.color = s.slots[1]
            .and_then(|stack| crate::inventory::components::dye_tint(&stack))
            .unwrap_or(crate::inventory::components::LEATHER_COLOR);
        return;
    }
    if m.kind != MobKind::Llama {
        return;
    }
    if c.color == LLAMA_FUR {
        c.color = LLAMA_COATS[s.variant as usize % 4];
    } else if c.color == CARPET {
        c.color = s.slots[1]
            .and_then(|stack| stack.item.block())
            .and_then(|b| b.carpet_color())
            .map_or([200, 200, 200], |d| d.sheep_rgb());
    }
}

/// Original exposed spine and ribs, in the same horse-shaped rig.
const SKELETON_BODY: &[Cuboid] = &[
    cube([-1., 19., -10.], [1., 21., 9.], [222, 218, 200], 12),
    cube([-4., 12., -8.], [-3., 20., -7.], [222, 218, 200], 12),
    cube([3., 12., -8.], [4., 20., -7.], [222, 218, 200], 12),
    cube([-4., 12., -3.], [-3., 20., -2.], [222, 218, 200], 12),
    cube([3., 12., -3.], [4., 20., -2.], [222, 218, 200], 12),
    cube([-4., 12., 2.], [-3., 20., 3.], [222, 218, 200], 12),
    cube([3., 12., 2.], [4., 20., 3.], [222, 218, 200], 12),
    cube([-4., 12., 7.], [4., 14., 8.], [222, 218, 200], 12),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{color::DyeColor, entity::Rng, inventory::Stack, item::Item, world::block::Block};

    /// The leather horse armor layer is drawn in the stack's `dyed_color`,
    /// `#A06540` undyed, and survives a save of the horse's slots.
    #[test]
    fn leather_horse_armor_is_drawn_in_its_dyed_colour() {
        use crate::inventory::components::{LEATHER_COLOR, apply_dyes};
        let mut mob = Mob::new(MobKind::Horse, glam::DVec3::ZERO, 0.);
        mob.mount = Some(crate::entity::mounts::State::new(MobKind::Horse, &mut Rng::new(1)).into());
        mob.mount.as_mut().unwrap().tame = true;
        let plain = Stack::new(Item::LEATHER_HORSE_ARMOR, 1);
        for (stack, want) in [
            (plain, LEATHER_COLOR),
            (apply_dyes(plain, &[DyeColor::Lime]).unwrap(), DyeColor::Lime.rgb()),
            (apply_dyes(plain, &[DyeColor::Red, DyeColor::Blue]).unwrap(), [173, 83, 152]),
        ] {
            mob.mount.as_mut().unwrap().slots[1] = Some(stack);
            mob.mount = Some(
                crate::entity::mounts::State::load(MobKind::Horse, &mob.mount.as_ref().unwrap().save()).unwrap().into(),
            );
            let parts = pose(&mob, 0.0, 1.0);
            assert!(parts.iter().any(|p| p.boxes.iter().any(|c| c.color == LEATHER_HORSE)));
            for source in ARMOR[0] {
                let mut emitted = source;
                colour(&mob, &mut emitted);
                assert_eq!(emitted.color, want);
            }
        }
        // Other horse armor keeps its own colour.
        mob.mount.as_mut().unwrap().slots[1] = Some(Stack::new(Item::IRON_HORSE_ARMOR, 1));
        let mut iron = ARMOR[1][0];
        colour(&mob, &mut iron);
        assert_eq!(iron.color, ARMOR[1][0].color);
    }

    #[test]
    fn every_llama_carpet_recolours_the_sentinel_after_equipment_reload() {
        let mut mob = Mob::new(MobKind::Llama, glam::DVec3::ZERO, 0.);
        mob.mount = Some(crate::entity::mounts::State::new(MobKind::Llama, &mut Rng::new(1)).into());
        mob.mount.as_mut().unwrap().tame = true;
        for dye in DyeColor::ALL {
            mob.mount.as_mut().unwrap().slots[1] = Some(Stack::new(Block::carpet(dye), 1));
            mob.mount = Some(
                crate::entity::mounts::State::load(MobKind::Llama, &mob.mount.as_ref().unwrap().save()).unwrap().into(),
            );
            let parts = llama(&mob);
            assert!(parts.iter().any(|p| std::ptr::eq(p.boxes, LLAMA_DECOR)));
            for &source in LLAMA_DECOR {
                let mut emitted = source;
                colour(&mob, &mut emitted);
                assert_eq!(emitted.color, dye.sheep_rgb());
                assert_ne!(emitted.color, CARPET);
            }
        }
    }
}
