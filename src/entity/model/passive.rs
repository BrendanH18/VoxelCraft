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
    if m.kind != MobKind::Mooshroom {
        return;
    }
    let brown = m.mooshroom.as_ref().is_some_and(|s| s.brown);
    if c.color == COW_HIDE || c.color == CAP {
        c.color = if brown { [133, 102, 73] } else { [178, 43, 36] };
    }
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
