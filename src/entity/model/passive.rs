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
    if c.color == WOLF_PLATE
        && let Some(color) = m.wolf_armor.as_ref().and_then(|s| s.armor).and_then(crate::entity::wolf_armor::color)
    {
        c.color = color;
    }

    if m.panda.as_ref().is_some_and(|s| s.variant() == crate::entity::panda::Gene::Brown) && c.color == [35, 33, 32] {
        c.color = [111, 75, 45];
    }
    if m.kind == MobKind::Cat {
        let variant = m.animal.as_ref().map_or(0, |a| a.variant) as usize % 11;
        // Original palette over the established cuboids; old all-black stays zero.
        if variant > 0 && matches!(c.color, [30, 29, 31] | [27, 27, 29] | [24, 24, 26]) {
            let palette: [[u8; 3]; 11] = [
                [27, 27, 29],
                [150, 120, 83],
                [27, 27, 29],
                [201, 116, 48],
                [190, 172, 145],
                [112, 125, 139],
                [219, 198, 159],
                [222, 203, 170],
                [202, 185, 159],
                [231, 230, 221],
                [154, 154, 148],
            ];
            let mut color = palette[variant];
            if matches!(variant, 2 | 6 | 10) && c.color == [30, 29, 31] {
                color = [223, 218, 204];
            }
            if matches!(variant, 4 | 8) && c.color != [30, 29, 31] {
                color = [85, 69, 59];
            }
            let shade = i16::from(c.color[0]) - 27;
            c.color = color.map(|v| (i16::from(v) + shade).clamp(0, 255) as u8);
        }
    }
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
pub(super) fn tongue(m: &Mob, camera: DVec3, out: &mut EntityMesh, light: ([u8; 4], u8)) {
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

const BEAR_FUR: Rgb = [227, 225, 208];
const BEAR_BODY: &[Cuboid] = &[cube([-7.0, -1.0, -4.0], [7.0, 11.0, 18.0], BEAR_FUR, 20)];
const BEAR_HEAD: &[Cuboid] = &[
    cube([-5.5, -3.0, -1.0], [5.5, 4.4, 10.0], BEAR_FUR, 18),
    cube([-5.2, 4.4, 1.0], [-2.6, 6.4, 4.0], BEAR_FUR, 18),
    cube([2.6, 4.4, 1.0], [5.2, 6.4, 4.0], BEAR_FUR, 18),
    cube([-3.7, -3.0, 8.0], [3.7, 0.5, 14.0], [189, 186, 168], 16),
    cube([-2.1, -0.5, 13.7], [2.1, 1.0, 14.1], [43, 44, 39], 0),
    cube([-4.8, 1.4, 9.8], [-3.2, 2.6, 10.1], [31, 33, 28], 0),
    cube([3.2, 1.4, 9.8], [4.8, 2.6, 10.1], [31, 33, 28], 0),
];
const BEAR_LEG: &[Cuboid] = &[
    cube([-2.3, -8.0, -2.3], [2.3, 0.0, 2.3], BEAR_FUR, 18),
    cube([-2.2, -8.0, 1.9], [2.2, -7.2, 2.4], [87, 81, 66], 12),
];
pub(super) fn polar_bear(m: &Mob) -> Parts {
    let stand = m.polar_bear.as_ref().map_or(0.0, |s| s.animation / 6.0);
    let rot = Quat::from_rotation_x(-1.2 * stand);
    let hip = Vec3::new(0.0, 9.0, -8.0);
    let at = |v: [f32; 3]| (hip + rot * (Vec3::from_array(v) - hip)).to_array();
    let swing = m.limb_phase.sin() * m.limb_amp * (1.0 - stand) * 0.7;
    parts![
        part(BEAR_BODY, hip.to_array(), rot),
        part(
            BEAR_HEAD,
            at([0.0, 16.0, 11.0]),
            rot * Quat::from_rotation_y(-m.head_yaw) * Quat::from_rotation_x(-m.head_pitch)
        ),
        part(BEAR_LEG, [-5.0, 8.0, -8.0], Quat::from_rotation_x(swing)),
        part(BEAR_LEG, [5.0, 8.0, -8.0], Quat::from_rotation_x(-swing)),
        part(BEAR_LEG, at([-5.0, 8.0, 8.0]), rot * Quat::from_rotation_x(-swing)),
        part(BEAR_LEG, at([5.0, 8.0, 8.0]), rot * Quat::from_rotation_x(swing))
    ]
}

const PANDA_BODY: &[Cuboid] = &[
    cube([-7.0, 0.0, -9.0], [7.0, 13.0, 8.0], [235, 232, 219], 14),
    cube([-7.2, 0.0, 1.5], [7.2, 13.0, 7.0], [35, 33, 32], 15),
    cube([-1.5, 3.0, -10.5], [1.5, 6.0, -9.0], [235, 232, 219], 12),
];
const PANDA_HEAD: &[Cuboid] = &[
    cube([-6.0, -3.0, -2.0], [6.0, 6.0, 7.0], [235, 232, 219], 12),
    cube([-6.0, 5.0, -0.5], [-3.0, 8.0, 2.5], [35, 33, 32], 12),
    cube([3.0, 5.0, -0.5], [6.0, 8.0, 2.5], [35, 33, 32], 12),
    cube([-5.2, 0.0, 7.0], [-1.7, 3.8, 7.3], [35, 33, 32], 4),
    cube([1.7, 0.0, 7.0], [5.2, 3.8, 7.3], [35, 33, 32], 4),
    cube([-3.2, 1.5, 7.3], [-2.1, 2.5, 7.5], [219, 216, 198], 0),
    cube([2.1, 1.5, 7.3], [3.2, 2.5, 7.5], [219, 216, 198], 0),
    cube([-2.8, -2.8, 6.5], [2.8, -0.3, 9.0], [211, 207, 189], 12),
    cube([-1.5, -1.0, 8.8], [1.5, 0.2, 9.2], [35, 33, 32], 0),
];
const PANDA_LEG: &[Cuboid] = &[cube([-2.7, -7.0, -2.7], [2.7, 0.0, 2.7], [35, 33, 32], 14)];
const PANDA_CAKE: &[Cuboid] = &[
    cube([-3.0, -2.0, -3.0], [3.0, 0.0, 3.0], [147, 101, 61], 12),
    cube([-3.0, 0.0, -3.0], [3.0, 0.7, 3.0], [237, 228, 207], 10),
    cube([-1.0, 0.7, -1.0], [0.0, 1.0, 0.0], [159, 43, 37], 0),
];
const PANDA_BAMBOO: &[Cuboid] = &[
    cube([-0.6, -5.0, -0.6], [0.6, 5.0, 0.6], [112, 146, 51], 16),
    cube([-0.8, 0.0, -0.8], [0.8, 0.6, 0.8], [69, 105, 34], 8),
];
pub(super) fn panda(m: &Mob, time: f32) -> Parts {
    let s = m.panda.as_ref().unwrap();
    let sit = if s.sitting { 1.0 } else { 0.0 };
    let rot = if s.rolling > 0 {
        Quat::from_rotation_x(-f32::from(s.rolling) * std::f32::consts::TAU / 32.0)
    } else {
        Quat::from_rotation_x(if s.on_back { PI } else { -1.15 * sit })
    };
    let center = Vec3::new(0.0, 10.0, 0.0);
    let at = |p: [f32; 3]| (center + rot * (Vec3::from_array(p) - center)).to_array();
    let swing = m.limb_phase.sin() * m.limb_amp * 0.65;
    let head = Quat::from_rotation_y(-m.head_yaw)
        * Quat::from_rotation_x(
            -m.head_pitch
                + if s.eating > 0 {
                    (time * 12.0).sin() * 0.1
                } else if s.scared {
                    (time * 21.0).sin() * 0.04
                } else if s.sneezing > 0 {
                    -f32::from(s.sneezing) / 100.0
                } else {
                    0.0
                },
        );
    let mut p = parts![part(PANDA_BODY, at([0.0, 7.0, 0.0]), rot), part(PANDA_HEAD, at([0.0, 13.0, 8.0]), rot * head)];
    for (x, z, phase) in [(-5.0, -6.0, swing), (5.0, -6.0, -swing), (-5.0, 5.0, -swing), (5.0, 5.0, swing)] {
        p.push(part(
            PANDA_LEG,
            at([x, 7.0, z]),
            rot * Quat::from_rotation_x(if s.sitting && z > 0.0 { -1.0 } else { phase }),
        ));
    }
    if m.animal.as_ref().is_some_and(|a| a.mouth.is_some()) {
        p.push(part(
            if m.animal.as_ref().and_then(|a| a.mouth).is_some_and(|h| h.item == crate::item::Item::CAKE) {
                PANDA_CAKE
            } else {
                PANDA_BAMBOO
            },
            at([0.0, 6.0, 9.0]),
            rot * Quat::from_rotation_z(0.65),
        ));
    }
    p
}

const ARMADILLO_SHELL: &[Cuboid] = &[
    cube([-4.7, 3.0, -5.3], [4.7, 8.3, 3.5], [181, 132, 117], 20),
    cube([-3.8, 8.3, -4.6], [3.8, 10.1, 2.8], [195, 145, 126], 18),
    cube([-5.0, 3.2, -3.2], [5.0, 8.4, -2.5], [136, 94, 84], 15),
    cube([-5.0, 3.2, -0.8], [5.0, 8.4, -0.1], [136, 94, 84], 15),
    cube([-4.8, 3.2, 1.6], [4.8, 8.4, 2.3], [136, 94, 84], 15),
];
const ARMADILLO_HEAD: &[Cuboid] = &[
    cube([-2.6, 0.0, -1.0], [2.6, 4.6, 3.4], [190, 140, 119], 18),
    cube([-1.8, 0.5, 3.4], [1.8, 2.8, 6.0], [169, 116, 99], 15),
    cube([-1.8, 4.6, -0.3], [-0.5, 7.0, 0.6], [176, 126, 108], 16),
    cube([0.5, 4.6, -0.3], [1.8, 7.0, 0.6], [176, 126, 108], 16),
    cube([-2.7, 2.6, 2.0], [-2.5, 3.5, 3.0], [27, 24, 21], 0),
    cube([2.5, 2.6, 2.0], [2.7, 3.5, 3.0], [27, 24, 21], 0),
];
const ARMADILLO_LEG: &[Cuboid] = &[
    cube([-1.1, -3.0, -1.1], [1.1, 0.0, 1.1], [152, 102, 89], 16),
    cube([-1.2, -3.0, 0.5], [1.2, -2.4, 1.7], [210, 186, 153], 10),
];
const ARMADILLO_TAIL: &[Cuboid] = &[cube([-0.7, 0.0, -6.5], [0.7, 1.0, 0.0], [167, 113, 97], 18)];
const ARMADILLO_BALL: &[Cuboid] = &[
    cube([-4.8, 1.0, -4.8], [4.8, 9.0, 4.8], [174, 122, 108], 18),
    cube([-3.4, 0.0, -3.4], [3.4, 10.3, 3.4], [190, 139, 120], 18),
    cube([-4.9, 1.3, -2.4], [4.9, 9.2, -1.8], [135, 91, 81], 12),
    cube([-4.9, 1.3, 0.4], [4.9, 9.2, 1.0], [135, 91, 81], 12),
    cube([-4.9, 1.3, 3.2], [4.9, 9.2, 3.8], [135, 91, 81], 12),
];
pub(super) fn armadillo(m: &Mob) -> Parts {
    let s = m.armadillo.as_ref().unwrap();
    if s.hide() {
        return parts![part(ARMADILLO_BALL, [0.0; 3], Quat::IDENTITY)];
    }
    let swing = m.limb_phase.sin() * m.limb_amp * 0.7;
    let mut p = parts![
        part(ARMADILLO_SHELL, [0.0; 3], Quat::IDENTITY),
        part(
            ARMADILLO_HEAD,
            [0.0, 1.0, 3.4],
            Quat::from_rotation_y(-m.head_yaw) * Quat::from_rotation_x(-m.head_pitch)
        ),
        part(ARMADILLO_TAIL, [0.0, 3.0, -5.0], Quat::from_rotation_x(-0.15))
    ];
    for (x, z, phase) in [(-3.2, -3.0, swing), (3.2, -3.0, -swing), (-3.2, 2.2, -swing), (3.2, 2.2, swing)] {
        p.push(part(ARMADILLO_LEG, [x, 3.0, z], Quat::from_rotation_x(phase)));
    }
    p
}

const WOLF_PLATE: Rgb = [210, 170, 153];
const WOLF_BODY_ARMOR: &[Cuboid] = &[
    cube([-3.5, 7.0, -5.8], [3.5, 12.6, 4.0], WOLF_PLATE, 16),
    cube([-3.8, 6.0, 1.0], [3.8, 13.6, 5.5], WOLF_PLATE, 16),
    cube([-3.6, 7.0, -3.6], [3.6, 13.0, -2.7], [138, 98, 87], 8),
    cube([-3.9, 7.0, 2.4], [3.9, 14.0, 3.1], [138, 98, 87], 8),
];
const WOLF_ARMOR_CRACKS: &[Cuboid] = &[
    cube([-3.65, 8.0, -4.8], [-3.55, 11.6, -4.3], [67, 44, 40], 0),
    cube([3.55, 8.0, -1.0], [3.65, 12.3, -0.5], [67, 44, 40], 0),
    cube([-1.6, 12.7, -5.0], [-1.0, 12.8, 0.0], [67, 44, 40], 0),
];
pub(super) fn wolf_armor(m: &Mob, at: [f32; 3], rot: Quat) -> Parts {
    let Some(a) = m.wolf_armor.as_ref().and_then(|s| s.armor) else {
        return Parts::new();
    };
    let mut p = parts![part(WOLF_BODY_ARMOR, at, rot)];
    let cracks = crate::entity::wolf_armor::cracks(a.damage) as usize;
    if cracks > 0 {
        p.push(part(&WOLF_ARMOR_CRACKS[..cracks], at, rot));
    }
    p
}

// Original golden spotted pelt, narrow muzzle and ringed tail.
const OCELOT_BODY: &[Cuboid] = &[
    cube([-3.0, 5.0, -7.0], [3.0, 10.0, 5.0], [184, 139, 65], 15),
    cube([-2.5, 5.0, -5.5], [2.5, 5.15, 4.0], [224, 204, 153], 12),
    cube([-3.1, 6.5, -5.5], [-2.95, 8.0, -4.2], [53, 40, 29], 3),
    cube([-3.1, 8.0, -2.7], [-2.95, 9.2, -1.5], [53, 40, 29], 3),
    cube([-3.1, 6.0, 0.3], [-2.95, 7.5, 1.6], [53, 40, 29], 3),
    cube([-3.1, 8.3, 2.6], [-2.95, 9.4, 4.0], [53, 40, 29], 3),
    cube([2.95, 6.0, -4.7], [3.1, 7.3, -3.3], [53, 40, 29], 3),
    cube([2.95, 8.1, -1.8], [3.1, 9.3, -0.4], [53, 40, 29], 3),
    cube([2.95, 6.3, 1.3], [3.1, 7.7, 2.6], [53, 40, 29], 3),
    cube([-1.8, 9.95, -4.3], [-0.5, 10.1, -3.1], [53, 40, 29], 3),
    cube([0.5, 9.95, -0.7], [1.8, 10.1, 0.6], [53, 40, 29], 3),
];
const OCELOT_HEAD: &[Cuboid] = &[
    cube([-2.5, -2.0, -1.0], [2.5, 1.5, 3.0], [195, 151, 77], 14),
    cube([-2.4, 1.2, -0.3], [-1.0, 2.3, 1.0], [61, 43, 30], 6),
    cube([1.0, 1.2, -0.3], [2.4, 2.3, 1.0], [61, 43, 30], 6),
    cube([-1.8, -1.6, 2.95], [1.8, -0.4, 3.8], [228, 213, 175], 10),
    cube([-0.6, -0.6, 3.75], [0.6, 0.1, 3.9], [91, 56, 46], 2),
    cube([-2.1, 0.0, 2.95], [-1.0, 0.8, 3.1], [101, 157, 65], 1),
    cube([1.0, 0.0, 2.95], [2.1, 0.8, 3.1], [101, 157, 65], 1),
];
const OCELOT_LEG: &[Cuboid] = &[
    cube([-1.0, -5.0, -1.0], [1.0, 0.0, 1.0], [184, 139, 65], 13),
    cube([-1.05, -4.5, -1.05], [1.05, -3.8, 1.05], [64, 46, 29], 4),
];
const OCELOT_TAIL: &[Cuboid] = &[
    cube([-0.65, 0.0, -9.0], [0.65, 1.3, 0.0], [184, 139, 65], 13),
    cube([-0.7, -0.05, -9.05], [0.7, 1.35, -7.7], [53, 40, 29], 4),
    cube([-0.7, -0.05, -5.6], [0.7, 1.35, -4.7], [53, 40, 29], 4),
    cube([-0.7, -0.05, -2.7], [0.7, 1.35, -1.9], [53, 40, 29], 4),
];
pub(super) fn ocelot(m: &Mob) -> Parts {
    let s = m.ocelot.as_ref().unwrap();
    let low = if s.crouching { -1.3 } else { 0.0 };
    let swing = (m.limb_phase * if s.sprinting { 1.6 } else { 1.0 }).sin() * m.limb_amp * 0.9;
    let rx = Quat::from_rotation_x;
    let head = Quat::from_rotation_y(-m.head_yaw) * rx(-m.head_pitch);
    parts![
        part(OCELOT_BODY, [0.0, low, 0.0], Quat::IDENTITY),
        part(OCELOT_HEAD, [0.0, 9.0 + low, 6.0], head),
        part(OCELOT_LEG, [-2.0, 5.0, 4.0], rx(swing)),
        part(OCELOT_LEG, [2.0, 5.0, 4.0], rx(-swing)),
        part(OCELOT_LEG, [-2.0, 5.0, -5.0], rx(-swing)),
        part(OCELOT_LEG, [2.0, 5.0, -5.0], rx(swing)),
        part(OCELOT_TAIL, [0.0, 7.0 + low, -7.0], rx(if s.sprinting { -0.4 } else { 0.15 }))
    ]
}
