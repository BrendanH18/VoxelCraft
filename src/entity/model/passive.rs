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
