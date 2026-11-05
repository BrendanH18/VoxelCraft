//! Original procedural wildlife geometry; no reference models or textures.
use super::*;
const CAP: Rgb = [181, 43, 36];
const MUSHROOM: &[Cuboid] = &[
    cube([-0.7, 0.0, -0.7], [0.7, 3.5, 0.7], [224, 213, 182], 24),
    cube([-3.0, 3.0, -3.0], [3.0, 4.5, 3.0], CAP, 45),
    cube([-2.0, 4.5, -2.0], [2.0, 5.5, 2.0], CAP, 40),
    cube([-2.5, 4.5, 0.0], [-1.4, 4.6, 1.4], [243, 224, 194], 12),
    cube([0.7, 5.5, -1.3], [1.6, 5.6, -0.3], [243, 224, 194], 12),
];
pub(super) fn mooshroom(m: &Mob) -> Parts {
    let swing = m.limb_phase.sin() * m.limb_amp * 0.9;
    let head = Quat::from_rotation_y(-m.head_yaw) * Quat::from_rotation_x(-m.head_pitch);
    let rx = Quat::from_rotation_x;
    let mut p = parts![
        part(COW_BODY, [0.0; 3], Quat::IDENTITY),
        part(COW_LEG, [-4.0, 12.0, 6.0], rx(swing)),
        part(COW_LEG, [4.0, 12.0, 6.0], rx(-swing)),
        part(COW_LEG, [-4.0, 12.0, -6.0], rx(-swing)),
        part(COW_LEG, [4.0, 12.0, -6.0], rx(swing)),
        part(COW_HEAD, [0.0, 18.0, 9.0], head),
    ];
    if !m.baby && m.age >= 0 {
        p.push(part(MUSHROOM, [-2.8, 22.0, -5.0], Quat::from_rotation_y(0.4)));
        p.push(part(MUSHROOM, [2.5, 22.0, 2.8], Quat::from_rotation_y(-0.6)));
        p.push(part(MUSHROOM, [0.0, 23.0, 10.0], head));
    }
    p
}
pub(super) fn colour(m: &Mob, c: &mut Cuboid) {
    if m.kind == MobKind::Frog && c.color == FROG_SKIN {
        c.color = match m.frog.as_ref().map_or(0, |s| s.variant) {
            1 => [83, 143, 69],
            2 => [209, 204, 181],
            _ => FROG_SKIN,
        };
    }
    if m.kind != MobKind::Mooshroom {
        return;
    }
    let brown = m.mooshroom.as_ref().is_some_and(|s| s.brown);
    if c.color == COW_HIDE || c.color == CAP {
        c.color = if brown { [133, 102, 73] } else { [178, 43, 36] };
    }
}

const FROG_SKIN: Rgb = [192, 112, 54];
const FROG_BODY: &[Cuboid] = &[
    cube([-3.4, 1.0, -3.0], [3.4, 5.0, 3.5], FROG_SKIN, 25),
    cube([-3.5, 4.0, -0.8], [3.5, 6.3, 4.3], FROG_SKIN, 20),
    cube([-3.5, 6.0, 1.8], [-1.6, 8.0, 3.8], FROG_SKIN, 18),
    cube([1.6, 6.0, 1.8], [3.5, 8.0, 3.8], FROG_SKIN, 18),
    cube([-3.2, 6.3, 3.8], [-1.8, 7.6, 3.9], [233, 205, 99], 0),
    cube([1.8, 6.3, 3.8], [3.2, 7.6, 3.9], [233, 205, 99], 0),
    cube([-2.8, 6.5, 3.9], [-2.2, 7.4, 4.0], [21, 29, 20], 0),
    cube([2.2, 6.5, 3.9], [2.8, 7.4, 4.0], [21, 29, 20], 0),
    cube([-2.8, 2.3, 3.5], [2.8, 4.0, 4.6], [226, 208, 155], 15),
];
const FROG_LEG: &[Cuboid] =
    &[cube([-0.7, 0.0, -1.5], [0.7, 2.2, 1.5], FROG_SKIN, 20), cube([-0.8, 0.0, 0.5], [0.8, 0.6, 2.0], FROG_SKIN, 14)];
const FROG_THROAT: &[Cuboid] = &[cube([-2.5, 1.2, 3.7], [2.5, 4.3, 5.2], [234, 215, 158], 14)];
pub(super) fn frog(m: &Mob, time: f32) -> Parts {
    let state = m.frog.as_ref().unwrap();
    let swing = if state.pose == 4 { 0.7 } else { (time * 5.0).sin() * m.limb_amp * 0.35 };
    let mut p = parts![part(FROG_BODY, [0.0; 3], Quat::IDENTITY)];
    for (x, z, sign) in [(-3.2, -2.2, 1.0), (3.2, -2.2, -1.0), (-2.6, 2.2, -1.0), (2.6, 2.2, 1.0)] {
        p.push(part(FROG_LEG, [x, 0.0, z], Quat::from_rotation_x(swing * sign)));
    }
    if state.pose == 3 {
        p.push(part(FROG_THROAT, [0.0; 3], Quat::IDENTITY));
    }
    p
}
const TADPOLE_BODY: &[Cuboid] = &[
    cube([-2.5, 0.8, -1.6], [2.5, 4.8, 2.4], [77, 66, 43], 28),
    cube([-2.5, 3.0, 2.4], [-1.2, 4.2, 2.5], [17, 19, 14], 0),
    cube([1.2, 3.0, 2.4], [2.5, 4.2, 2.5], [17, 19, 14], 0),
];
const TADPOLE_TAIL: &[Cuboid] = &[
    cube([-0.6, 1.1, -6.0], [0.6, 3.2, 0.0], [112, 99, 64], 18),
    cube([-0.25, 0.5, -6.0], [0.25, 3.8, -2.0], [149, 134, 91], 15),
];
pub(super) fn tadpole(m: &Mob, time: f32) -> Parts {
    parts![
        part(TADPOLE_BODY, [0.0; 3], Quat::IDENTITY),
        part(
            TADPOLE_TAIL,
            [0.0, 0.0, -1.5],
            Quat::from_rotation_y((time * 10.0).sin() * if m.in_water { 0.45 } else { 0.1 })
        )
    ]
}
pub(super) fn tongue(m: &Mob, camera: DVec3, out: &mut Vec<EntityVertex>, light: ([u8; 4], u8)) {
    let Some(s) = &m.frog else {
        return;
    };
    if m.kind != MobKind::Frog || !matches!(s.pose, 1 | 2) {
        return;
    }
    let Some((_, target)) = s.tongue else {
        return;
    };
    let from = m.pos + DVec3::new(m.yaw.cos() as f64 * 0.25, 0.22, m.yaw.sin() as f64 * 0.25);
    let delta = (target + DVec3::Y * 0.12 - from).as_vec3();
    let extension = if s.pose == 1 {
        (s.tongue_ticks as f32 / 2.0).clamp(0.0, 1.0)
    } else {
        (11_u16.saturating_sub(s.tongue_ticks) as f32 / 4.0).clamp(0.0, 1.0)
    };
    let length = delta.length().clamp(0.01, 2.5) * extension;
    let rot = Quat::from_rotation_arc(Vec3::Z, delta.normalize_or(Vec3::Z));
    let c = cube([-0.25, -0.25, 0.0], [0.25, 0.25, length * 16.0], [205, 113, 132], 10);
    let from = (from - camera).as_vec3();
    push_cuboid(out, &c, &|v| from + rot * v / 16.0, rot, light, (FIRE, 0.0), 0.0);
}
const BAT_BODY: &[Cuboid] = &[
    cube([-1.8, 0.0, -0.8], [1.8, 3.0, 0.8], [55, 43, 35], 18),
    cube([-2.0, 3.0, -1.5], [2.0, 10.0, 1.5], [83, 60, 48], 32),
    cube([-2.5, 9.0, -1.7], [2.5, 13.0, 2.0], [101, 75, 57], 28),
    cube([-2.5, 12.0, -0.7], [-1.1, 14.4, 0.5], [128, 87, 70], 20),
    cube([1.1, 12.0, -0.7], [2.5, 14.4, 0.5], [128, 87, 70], 20),
    cube([-1.8, 10.5, 2.0], [-0.9, 11.5, 2.1], [24, 20, 17], 0),
    cube([0.9, 10.5, 2.0], [1.8, 11.5, 2.1], [24, 20, 17], 0),
    cube([-0.6, 9.5, 2.0], [0.6, 10.3, 2.6], [176, 115, 102], 16),
];
const BAT_WING: &[Cuboid] = &[
    cube([0.0, -3.0, -0.4], [5.0, 1.0, 0.4], [79, 51, 43], 25),
    cube([5.0, -5.0, -0.3], [10.0, 0.0, 0.3], [95, 61, 51], 30),
    cube([5.0, 0.0, -0.5], [10.0, 0.5, 0.5], [134, 91, 69], 18),
];
pub(super) fn bat(m: &Mob, time: f32) -> Parts {
    let resting = m.bat.as_ref().is_some_and(|s| s.resting);
    let flip = if resting { Quat::from_rotation_z(PI) } else { Quat::IDENTITY };
    let body_y = if resting { 14.4 } else { 0.0 };
    let flap = if resting { 1.3 } else { (time * std::f32::consts::TAU * 2.0).sin() * 0.8 };
    let mut p = parts![part(BAT_BODY, [0.0, body_y, 0.0], flip)];
    for side in [-1.0, 1.0] {
        let pivot = Vec3::new(side * 2.0, 9.0, 0.0);
        let pivot = Vec3::new(0.0, body_y, 0.0) + flip * pivot;
        let mirror = if side < 0.0 { Quat::from_rotation_y(PI) } else { Quat::IDENTITY };
        p.push(part(BAT_WING, pivot.to_array(), flip * mirror * Quat::from_rotation_y(flap)));
    }
    p
}
