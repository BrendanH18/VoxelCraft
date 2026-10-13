//! Procedural Java-like equine proportions, coats, markings and equipment.
use super::{Cuboid, Mob, MobKind, Parts, cube, part};
use crate::item::Item;
use glam::Quat;
const COLORS: [[u8; 3]; 7] =
    [[236, 232, 218], [208, 177, 126], [159, 104, 57], [108, 66, 38], [40, 35, 31], [132, 126, 116], [72, 55, 43]];
const fn bodies() -> [[Cuboid; 3]; 7] {
    let mut out = [[cube([0.; 3], [0.; 3], [0; 3], 0); 3]; 7];
    let mut i = 0;
    while i < 7 {
        out[i] = [
            cube([-5., 11., -10.], [5., 21., 9.], COLORS[i], 22),
            cube([-1.5, 14., -16.], [1.5, 21., -9.], [48, 38, 28], 20),
            cube([-1., 20., -9.], [1., 22., 10.], [44, 36, 26], 20),
        ];
        i += 1;
    }
    out
}
const BODIES: [[Cuboid; 3]; 7] = bodies();
const fn heads() -> [[Cuboid; 5]; 7] {
    let mut out = [[cube([0.; 3], [0.; 3], [0; 3], 0); 5]; 7];
    let mut i = 0;
    while i < 7 {
        out[i] = [
            cube([-2.5, -1., -1.], [2.5, 7., 4.], COLORS[i], 18),
            cube([-2.5, 4., 2.], [2.5, 9., 9.], COLORS[i], 18),
            cube([-2., 7., 1.], [-1., 11., 3.], COLORS[i], 14),
            cube([1., 7., 1.], [2., 11., 3.], COLORS[i], 14),
            cube([-2.6, 7., 5.], [2.6, 8., 6.], [30, 26, 22], 0),
        ];
        i += 1;
    }
    out
}
const HEADS: [[Cuboid; 5]; 7] = heads();
const fn legs() -> [[Cuboid; 2]; 7] {
    let mut out = [[cube([0.; 3], [0.; 3], [0; 3], 0); 2]; 7];
    let mut i = 0;
    while i < 7 {
        out[i] = [
            cube([-1.6, -11., -1.6], [1.6, 0., 1.6], COLORS[i], 18),
            cube([-1.8, -12., -1.8], [1.8, -9., 1.8], [50, 44, 38], 12),
        ];
        i += 1;
    }
    out
}
const LEGS: [[Cuboid; 2]; 7] = legs();
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
const fn armors() -> [[Cuboid; 3]; 4] {
    let colors = [[145, 93, 55], [210, 210, 215], [235, 194, 54], [60, 220, 210]];
    let mut out = [[cube([0.; 3], [0.; 3], [0; 3], 0); 3]; 4];
    let mut i = 0;
    while i < 4 {
        out[i] = [
            cube([-5.2, 11., -10.2], [-5., 20., 9.2], colors[i], 8),
            cube([5., 11., -10.2], [5.2, 20., 9.2], colors[i], 8),
            cube([-5.2, 21., -10.2], [5.2, 21.2, 9.2], colors[i], 8),
        ];
        i += 1;
    }
    out
}
const ARMOR: [[Cuboid; 3]; 4] = armors();
pub(super) fn pose(m: &Mob) -> Parts {
    let s = m.mount.as_ref().unwrap();
    let color = if m.kind == MobKind::Horse { s.variant as usize } else { 5 };
    let stride = m.limb_phase.sin() * m.limb_amp * 0.8;
    let head_y = match m.kind {
        MobKind::Donkey => 13.,
        MobKind::Mule => 14.,
        _ => 15.,
    };
    let head = Quat::from_rotation_y(-m.head_yaw) * Quat::from_rotation_x(-0.35 - m.head_pitch);
    let mut p = Parts::new();
    p.push(part(&BODIES[color], [0.; 3], Quat::IDENTITY));
    for (x, z, phase) in [(-3.5, 7., stride), (3.5, 7., -stride), (-3.5, -8., -stride), (3.5, -8., stride)] {
        p.push(part(&LEGS[color], [x, 12., z], Quat::from_rotation_x(phase)));
    }
    p.push(part(&HEADS[color], [0., head_y, 6.], head));
    if m.kind != MobKind::Horse {
        p.push(part(EARS, [0., head_y, 6.], head));
    }
    if s.marking > 0 && m.kind == MobKind::Horse {
        p.push(part(&MARKINGS[s.marking as usize - 1], [0.; 3], Quat::IDENTITY));
    }
    if s.chest {
        p.push(part(CHESTS, [0.; 3], Quat::IDENTITY));
    }
    equipment(m, &mut p);
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
        let index = match stack.item {
            Item::LEATHER_HORSE_ARMOR => 0,
            Item::IRON_HORSE_ARMOR => 1,
            Item::GOLDEN_HORSE_ARMOR => 2,
            Item::DIAMOND_HORSE_ARMOR => 3,
            _ => return,
        };
        p.push(part(&ARMOR[index], [0.; 3], Quat::IDENTITY));
    }
}

/// Llama coat colours: creamy, white, brown and gray.
const LLAMA_COATS: [[u8; 3]; 4] = [[221, 207, 178], [233, 233, 228], [128, 92, 60], [130, 128, 124]];
/// The trader llama's head and legs are drawn in this colour; wild llamas recolour it.
const LLAMA_FUR: [u8; 3] = [221, 207, 178];
/// Placeholder recoloured to the worn carpet.
const CARPET: [u8; 3] = [1, 2, 3];
const LY: f32 = super::trader_model::LLAMA_Y;
const LLAMA_BODY: &[Cuboid] = &[cube([-4.5, 14.0 * LY, -7.0], [4.5, 23.0 * LY, 7.0], LLAMA_FUR, 20)];
const LLAMA_DECOR: &[Cuboid] = &[
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
    let swing = m.limb_phase.sin() * m.limb_amp * 0.9;
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
const CAMEL_BODY: &[Cuboid] = &[
    cube([-7.5, 20., -13.], [7.5, 30., 13.], SAND, 18),
    cube([-4.5, 30., -6.], [4.5, 35., 4.], [205, 152, 80], 18),
    cube([-1., 22., -14.], [1., 28., -13.], [160, 115, 60], 10),
];
const CAMEL_LEG: &[Cuboid] = &[
    cube([-1.5, -20., -1.5], [1.5, 0., 1.5], SAND, 16),
    cube([-1.8, -21., -1.8], [1.8, -18., 1.8], [130, 95, 55], 10),
];
const CAMEL_HEAD: &[Cuboid] = &[
    cube([-3., -2., -2.], [3., 13., 4.], SAND, 16),
    cube([-3.5, 9., 0.], [3.5, 15., 12.], SAND, 16),
    cube([-3.6, 12., 6.], [3.6, 13., 7.], [40, 32, 24], 0),
    cube([-4.5, 13., 1.], [-3.5, 14., 3.], [205, 152, 80], 10),
    cube([3.5, 13., 1.], [4.5, 14., 3.], [205, 152, 80], 10),
];
const CAMEL_SADDLE: &[Cuboid] = &[
    cube([-5., 35., -7.], [5., 36.2, 5.], [132, 72, 36], 16),
    cube([-7.7, 24., -2.], [-7.5, 35., 1.], [160, 146, 130], 8),
    cube([7.5, 24., -2.], [7.7, 35., 1.], [160, 146, 130], 8),
];
pub(super) fn camel(m: &Mob) -> Parts {
    let s = m.mount.as_ref();
    let sitting = s.is_some_and(|s| s.sit_ticks > 0);
    let swing = m.limb_phase.sin() * m.limb_amp * 0.7;
    let rx = Quat::from_rotation_x;
    let head = Quat::from_rotation_y(-m.head_yaw) * rx(-m.head_pitch);
    // Sitting: the body rests on folded legs.
    let drop = if sitting { -15. } else { 0. };
    let mut p = Parts::new();
    p.push(part(CAMEL_BODY, [0., drop, 0.], Quat::IDENTITY));
    p.push(part(CAMEL_HEAD, [0., 23. + drop, 12.], head));
    for (x, z, phase) in [(-4.5, 9., swing), (4.5, 9., -swing), (-4.5, -9., -swing), (4.5, -9., swing)] {
        if sitting {
            p.push(part(CAMEL_LEG, [x, 2., z + 9.], rx(std::f32::consts::FRAC_PI_2)));
        } else {
            p.push(part(CAMEL_LEG, [x, 21., z], rx(phase)));
        }
    }
    if s.is_some_and(|s| s.saddled()) {
        p.push(part(CAMEL_SADDLE, [0., drop, 0.], Quat::IDENTITY));
    }
    p
}

/// Llama coats and carpets.
pub(super) fn colour(m: &Mob, c: &mut Cuboid) {
    if m.kind != MobKind::Llama {
        return;
    }
    let Some(s) = m.mount.as_ref() else { return };
    if c.color == LLAMA_FUR {
        c.color = LLAMA_COATS[s.variant as usize % 4];
    } else if c.color == CARPET {
        c.color = s.slots[1]
            .and_then(|stack| stack.item.block())
            .and_then(|b| b.carpet_color())
            .map_or([200, 200, 200], |d| d.sheep_rgb());
    }
}
