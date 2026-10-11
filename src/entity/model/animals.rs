//! Original box models, in sixteenths of a block. Shared animation state is
//! read directly, keeping the entity pass allocation-free per mob.
use super::*;
const FUR: [u8; 3] = [186, 185, 178];
const COLLAR: [u8; 3] = [255, 0, 254];
const WOLF_BODY: &[Cuboid] = &[
    cube([-3.0, 6.0, -6.0], [3.0, 12.0, 5.0], FUR, 24),
    cube([-3.5, 6.0, 1.0], [3.5, 13.0, 6.0], [171, 171, 167], 26),
];
const WOLF_HEAD: &[Cuboid] = &[
    cube([-3.0, -2.0, -1.0], [3.0, 4.0, 5.0], FUR, 18),
    cube([-1.5, -2.0, 5.0], [1.5, 1.0, 8.0], [211, 209, 199], 16),
    cube([-1.0, -1.0, 8.0], [1.0, 0.5, 8.1], [35, 33, 32], 0),
    cube([-3.0, 4.0, 0.0], [-1.0, 7.0, 2.0], FUR, 20),
    cube([1.0, 4.0, 0.0], [3.0, 7.0, 2.0], FUR, 20),
    cube([-2.6, 1.0, 5.0], [-1.5, 2.2, 5.1], [30, 28, 25], 0),
    cube([1.5, 1.0, 5.0], [2.6, 2.2, 5.1], [30, 28, 25], 0),
];
const WOLF_LEG: &[Cuboid] = &[cube([-1.0, -6.0, -1.0], [1.0, 0.0, 1.0], FUR, 18)];
const WOLF_TAIL: &[Cuboid] = &[cube([-1.0, -1.0, -7.0], [1.0, 1.0, 0.0], FUR, 20)];
const WOLF_COLLAR: &[Cuboid] = &[cube([-3.6, 7.0, 3.0], [3.6, 12.0, 4.5], COLLAR, 0)];
const FOX_BODY: &[Cuboid] = &[
    cube([-3.0, 4.0, -6.0], [3.0, 10.0, 6.0], [201, 108, 47], 24),
    cube([-3.0, 4.0, 3.0], [3.0, 7.0, 6.1], [239, 229, 209], 12),
];
const FOX_HEAD: &[Cuboid] = &[
    cube([-3.5, -2.0, -1.0], [3.5, 4.0, 5.0], [201, 108, 47], 20),
    cube([-2.0, -2.0, 5.0], [2.0, 0.0, 8.0], [243, 229, 207], 15),
    cube([-1.0, -1.0, 8.0], [1.0, 0.0, 8.2], [35, 31, 29], 0),
    cube([-3.0, 4.0, 0.0], [-1.0, 7.0, 2.0], [65, 44, 34], 16),
    cube([1.0, 4.0, 0.0], [3.0, 7.0, 2.0], [65, 44, 34], 16),
    cube([-3.0, 1.0, 5.0], [-1.5, 2.0, 5.1], [28, 26, 23], 0),
    cube([1.5, 1.0, 5.0], [3.0, 2.0, 5.1], [28, 26, 23], 0),
];
const FOX_LEG: &[Cuboid] = &[cube([-0.8, -4.0, -0.8], [0.8, 0.0, 0.8], [65, 44, 34], 16)];
const FOX_TAIL: &[Cuboid] = &[
    cube([-2.0, -2.0, -7.0], [2.0, 2.0, 0.0], [201, 108, 47], 24),
    cube([-2.0, -2.0, -10.0], [2.0, 2.0, -7.0], [242, 234, 218], 16),
];
const RABBIT: &[Cuboid] = &[
    cube([-2.0, 2.0, -3.0], [2.0, 6.0, 3.0], [158, 123, 88], 24),
    cube([-2.0, 4.0, 2.0], [2.0, 8.0, 6.0], [173, 143, 106], 22),
    cube([-2.0, 8.0, 2.0], [-0.6, 15.0, 3.2], [169, 137, 103], 18),
    cube([0.6, 8.0, 2.0], [2.0, 15.0, 3.2], [169, 137, 103], 18),
    cube([-1.8, 8.5, 3.2], [-0.8, 14.0, 3.3], [220, 164, 161], 0),
    cube([0.8, 8.5, 3.2], [1.8, 14.0, 3.3], [220, 164, 161], 0),
    cube([-2.1, 6.0, 4.0], [-2.0, 7.0, 5.0], [30, 28, 26], 0),
    cube([2.0, 6.0, 4.0], [2.1, 7.0, 5.0], [30, 28, 26], 0),
    cube([-1.0, 5.0, 6.0], [1.0, 6.0, 6.1], [218, 170, 166], 0),
    cube([-1.0, 3.0, -4.5], [1.0, 5.0, -3.0], [236, 225, 202], 12),
];
const RABBIT_FOOT: &[Cuboid] = &[cube([-1.0, -1.0, -3.0], [1.0, 1.0, 1.0], [158, 123, 88], 24)];
const GOAT_BODY: &[Cuboid] = &[cube([-4.0, 8.0, -7.0], [4.0, 17.0, 6.0], [224, 219, 201], 28)];
const GOAT_HEAD: &[Cuboid] = &[
    cube([-2.5, -3.0, 0.0], [2.5, 4.0, 7.0], [232, 227, 211], 18),
    cube([-2.0, -5.0, 3.0], [2.0, -3.0, 6.0], [190, 181, 161], 20),
    cube([-5.0, 1.0, 1.0], [-2.5, 2.0, 4.0], [207, 194, 180], 14),
    cube([2.5, 1.0, 1.0], [5.0, 2.0, 4.0], [207, 194, 180], 14),
    cube([-2.6, 1.0, 4.0], [-2.5, 2.0, 5.5], [28, 26, 22], 0),
    cube([2.5, 1.0, 4.0], [2.6, 2.0, 5.5], [28, 26, 22], 0),
];
const GOAT_HORN: &[Cuboid] = &[cube([-0.6, 0.0, -0.6], [0.6, 6.0, 0.6], [110, 106, 95], 15)];
const GOAT_LEG: &[Cuboid] = &[
    cube([-1.0, -7.0, -1.0], [1.0, 0.0, 1.0], [224, 219, 201], 20),
    cube([-1.0, -8.0, -1.0], [1.0, -7.0, 1.0], [81, 74, 65], 12),
];
const PARROT: &[Cuboid] = &[
    cube([-1.5, 3.0, -2.0], [1.5, 10.0, 2.0], [221, 48, 43], 20),
    cube([-2.0, 9.0, -1.0], [2.0, 14.0, 3.0], [221, 48, 43], 16),
    cube([-1.0, 9.0, 3.0], [1.0, 11.0, 5.0], [82, 77, 67], 15),
    cube([-2.1, 11.0, 1.0], [-2.0, 12.0, 2.0], [27, 26, 24], 0),
    cube([2.0, 11.0, 1.0], [2.1, 12.0, 2.0], [27, 26, 24], 0),
    cube([-0.5, 14.0, 0.0], [0.5, 17.0, 1.0], [242, 211, 45], 0),
    cube([-1.0, 0.0, -1.0], [1.0, 3.0, 0.0], [79, 76, 69], 15),
    cube([-1.0, 2.0, -6.0], [1.0, 4.0, -2.0], [61, 82, 185], 18),
];
const PARROT_WING: &[Cuboid] = &[cube([0.0, -6.0, -1.0], [1.0, 0.0, 2.0], [61, 82, 185], 18)];
pub(super) fn pose(m: &Mob, time: f32) -> Parts {
    let a = m.animal.as_deref();
    let swing = m.limb_phase.sin() * m.limb_amp * 0.7;
    let head = Quat::from_rotation_y(-m.head_yaw) * Quat::from_rotation_x(-m.head_pitch);
    let rx = Quat::from_rotation_x;
    match m.kind {
        MobKind::Wolf | MobKind::Fox => {
            let wolf = m.kind == MobKind::Wolf;
            let sitting = a.is_some_and(|a| a.sitting);
            let sleeping = a.is_some_and(|a| a.sleeping);
            let body_rot = if sitting { rx(-0.7) } else { Quat::IDENTITY };
            let y = if sleeping { -3.0 } else { 0.0 };
            let mut p = parts![
                part(if wolf { WOLF_BODY } else { FOX_BODY }, [0.0, y, 0.0], body_rot),
                part(
                    if wolf { WOLF_HEAD } else { FOX_HEAD },
                    [0.0, if wolf { 10.0 } else { 8.0 } + y, 6.0],
                    head * Quat::from_rotation_z(if a.is_some_and(|a| a.begging) { 0.25 } else { 0.0 })
                ),
            ];
            for (i, x, z) in [(0, -2.0, 4.0), (1, 2.0, 4.0), (2, -2.0, -4.0), (3, 2.0, -4.0)] {
                p.push(part(
                    if wolf { WOLF_LEG } else { FOX_LEG },
                    [x, if wolf { 6.0 } else { 4.0 } + y, z],
                    rx(if sitting || sleeping {
                        1.2
                    } else if i % 2 == 0 {
                        swing
                    } else {
                        -swing
                    }),
                ));
            }
            let tail_pitch = if wolf { 1.0 - m.health / m.max_health() * 1.2 } else { -0.25 };
            p.push(part(
                if wolf { WOLF_TAIL } else { FOX_TAIL },
                [0.0, if wolf { 10.0 } else { 7.0 } + y, -6.0],
                rx(tail_pitch) * Quat::from_rotation_y((time * 3.0).sin() * 0.08),
            ));
            if wolf && a.is_some_and(|a| a.owner.is_some()) {
                p.push(part(WOLF_COLLAR, [0.0; 3], Quat::IDENTITY));
            }
            p
        }
        MobKind::Rabbit => parts![
            part(RABBIT, [0.0; 3], head),
            part(RABBIT_FOOT, [-2.0, 1.0, -2.0], rx(swing)),
            part(RABBIT_FOOT, [2.0, 1.0, -2.0], rx(swing))
        ],
        MobKind::Goat => {
            let mut p = parts![
                part(GOAT_BODY, [0.0; 3], Quat::IDENTITY),
                part(
                    GOAT_HEAD,
                    [0.0, 15.0, 5.0],
                    head * rx(if a.is_some_and(|a| a.ramming > 0.0) { -0.7 } else { 0.0 })
                )
            ];
            for (i, x, z) in [(0, -3.0, 4.0), (1, 3.0, 4.0), (2, -3.0, -5.0), (3, 3.0, -5.0)] {
                p.push(part(GOAT_LEG, [x, 8.0, z], rx(if i % 2 == 0 { swing } else { -swing })));
            }
            let horns = if m.baby { 0 } else { a.map_or(2, |a| a.horns) };
            for i in 0..horns {
                p.push(part(GOAT_HORN, [if i == 0 { -1.5 } else { 1.5 }, 19.0, 5.0], rx(-0.35)));
            }
            p
        }
        MobKind::Parrot => {
            let dance = if a.is_some_and(|a| a.dancing) { (time * 12.0).sin() * 0.2 } else { 0.0 };
            let flap = if m.on_ground { 0.1 } else { (time * 20.0).sin() * 0.8 };
            parts![
                part(PARROT, [0.0; 3], Quat::from_rotation_z(dance)),
                part(PARROT_WING, [-1.5, 10.0, 0.0], Quat::from_rotation_z(-flap)),
                part(PARROT_WING, [1.5, 10.0, 0.0], Quat::from_rotation_z(flap))
            ]
        }
        _ => Parts::new(),
    }
}
pub(super) fn colour(m: &Mob, c: &mut Cuboid) {
    let Some(a) = m.animal.as_ref() else { return };
    if m.kind == MobKind::Wolf && c.color == FUR {
        c.color = [
            [186, 185, 178],
            [161, 133, 103],
            [213, 221, 225],
            [202, 197, 173],
            [61, 61, 57],
            [119, 79, 54],
            [145, 83, 53],
            [177, 141, 83],
            [186, 150, 93],
        ][a.variant as usize % 9];
    }
    if c.color == COLLAR {
        c.color = a.collar.rgb();
    }
    if m.kind == MobKind::Fox && a.variant == 1 && c.color == [201, 108, 47] {
        c.color = [231, 228, 222];
    }
    if m.kind == MobKind::Parrot && c.color == [221, 48, 43] {
        c.color =
            [[221, 48, 43], [59, 105, 218], [63, 165, 81], [71, 176, 202], [165, 165, 165]][a.variant as usize % 5];
    }
    if m.kind == MobKind::Rabbit && c.color == [158, 123, 88] {
        c.color = [[158, 123, 88], [228, 224, 218], [49, 43, 39], [183, 158, 127], [142, 115, 85], [183, 164, 134]]
            [a.variant as usize % 6];
    }
}

pub(super) fn model_scale(m: &Mob) -> Vec3 {
    Vec3::new(
        1.0,
        match m.kind {
            MobKind::Wolf => 0.8,
            MobKind::Fox => 0.7467,
            MobKind::Rabbit => 0.5333,
            MobKind::Goat => 0.84,
            MobKind::Parrot => 0.8471,
            _ => 1.0,
        },
        1.0,
    )
}
