//! Compact, animated original models authored in sixteenths of a block.
use super::*;
const FISH: &[Cuboid] = &[
    cube([-2.0, 1.0, -5.0], [2.0, 5.0, 5.0], [181, 155, 113], 22),
    cube([-2.1, 4.0, 3.0], [-2.0, 5.0, 4.0], [25, 25, 22], 0),
    cube([2.0, 4.0, 3.0], [2.1, 5.0, 4.0], [25, 25, 22], 0),
    cube([-0.2, 5.0, -2.0], [0.2, 7.0, 1.0], [132, 107, 67], 18),
];
const SALMON: &[Cuboid] = &[
    cube([-2.0, 1.0, -7.0], [2.0, 5.0, 7.0], [149, 69, 55], 22),
    cube([-2.0, 5.0, -7.0], [2.0, 6.0, 4.0], [89, 104, 64], 20),
    cube([-2.1, 3.0, 5.0], [-2.0, 4.0, 6.0], [25, 25, 22], 0),
    cube([2.0, 3.0, 5.0], [2.1, 4.0, 6.0], [25, 25, 22], 0),
];
const TROPICAL: &[Cuboid] = &[
    cube([-1.5, 0.5, -3.0], [1.5, 5.5, 4.0], [245, 155, 45], 10),
    cube([-1.55, 0.5, -0.5], [1.55, 5.5, 0.5], [245, 240, 210], 0),
    cube([-0.2, 5.5, -2.5], [0.2, 7.0, 3.0], [242, 174, 65], 0),
    cube([-1.6, 3.5, 2.0], [-1.5, 4.5, 3.0], [22, 22, 22], 0),
    cube([1.5, 3.5, 2.0], [1.6, 4.5, 3.0], [22, 22, 22], 0),
];
const FISH_TAIL: &[Cuboid] = &[cube([-0.2, -2.5, -3.0], [0.2, 2.5, 0.0], [135, 120, 88], 12)];
const PUFFER: &[Cuboid] = &[
    cube([-3.0, 0.0, -3.0], [3.0, 6.0, 3.0], [215, 179, 76], 28),
    cube([-3.0, 0.0, -3.0], [3.0, 2.0, 3.0], [221, 212, 154], 15),
    cube([-2.0, 3.5, 3.0], [-0.8, 5.0, 3.1], [20, 25, 28], 0),
    cube([0.8, 3.5, 3.0], [2.0, 5.0, 3.1], [20, 25, 28], 0),
];
const SPIKE: &[Cuboid] = &[cube([-0.5, -0.5, 0.0], [0.5, 0.5, 3.0], [236, 190, 86], 0)];
const SQUID: &[Cuboid] = &[
    cube([-4.0, 5.0, -4.0], [4.0, 13.0, 4.0], [49, 71, 80], 22),
    cube([-3.0, 6.0, 4.0], [-1.0, 8.0, 4.1], [232, 232, 232], 0),
    cube([1.0, 6.0, 4.0], [3.0, 8.0, 4.1], [232, 232, 232], 0),
    cube([-2.0, 6.0, 4.1], [-1.0, 7.0, 4.2], [15, 20, 23], 0),
    cube([1.0, 6.0, 4.1], [2.0, 7.0, 4.2], [15, 20, 23], 0),
];
const TENTACLE: &[Cuboid] = &[cube([-0.7, -7.0, -0.7], [0.7, 0.0, 0.7], [49, 71, 80], 16)];
const DOLPHIN: &[Cuboid] = &[
    cube([-4.0, 1.0, -9.0], [4.0, 7.0, 8.0], [116, 143, 153], 16),
    cube([-2.0, 2.0, 8.0], [2.0, 4.0, 13.0], [136, 158, 164], 14),
    cube([-0.5, 7.0, -4.0], [0.5, 11.0, 1.0], [98, 130, 145], 12),
    cube([-8.0, 1.0, -3.0], [8.0, 2.0, 0.0], [101, 132, 144], 14),
    cube([-4.1, 4.0, 5.0], [-4.0, 5.0, 6.0], [20, 25, 29], 0),
    cube([4.0, 4.0, 5.0], [4.1, 5.0, 6.0], [20, 25, 29], 0),
];
const FLUKE: &[Cuboid] = &[cube([-5.0, -0.5, -5.0], [5.0, 0.5, 0.0], [112, 140, 150], 12)];
const AXOLOTL: &[Cuboid] = &[
    cube([-3.0, 1.0, -5.0], [3.0, 4.0, 3.0], [237, 160, 184], 14),
    cube([-4.0, 0.5, 2.0], [4.0, 5.0, 7.0], [243, 179, 198], 14),
    cube([-3.0, 3.0, 7.0], [-2.0, 4.0, 7.1], [22, 20, 29], 0),
    cube([2.0, 3.0, 7.0], [3.0, 4.0, 7.1], [22, 20, 29], 0),
    cube([-6.0, 2.0, 3.0], [-4.0, 4.0, 5.0], [197, 83, 132], 0),
    cube([4.0, 2.0, 3.0], [6.0, 4.0, 5.0], [197, 83, 132], 0),
    cube([-4.0, 0.0, -4.0], [4.0, 1.0, -2.0], [235, 151, 183], 12),
    cube([-4.0, 0.0, 0.0], [4.0, 1.0, 2.0], [235, 151, 183], 12),
];
const AXO_TAIL: &[Cuboid] = &[cube([-0.5, 0.0, -6.0], [0.5, 4.0, 0.0], [217, 130, 171], 14)];
const GUARDIAN: &[Cuboid] = &[
    cube([-6.0, 0.0, -6.0], [6.0, 13.6, 6.0], [105, 138, 129], 35),
    cube([-4.0, 3.0, 6.0], [4.0, 9.0, 6.2], [214, 212, 177], 12),
    cube([-1.5, 4.0, 6.2], [1.5, 7.0, 6.4], [164, 60, 48], 0),
    cube([-0.6, 4.5, 6.4], [0.6, 6.5, 6.5], [26, 27, 24], 0),
];
const GUARD_TAIL: &[Cuboid] = &[
    cube([-1.5, -1.5, -8.0], [1.5, 1.5, 0.0], [95, 126, 113], 25),
    cube([-0.5, -4.0, -9.0], [0.5, 4.0, -7.0], [215, 161, 64], 16),
];
const CAT: &[Cuboid] = &[
    cube([-2.5, 3.0, -6.0], [2.5, 7.0, 5.0], [30, 29, 31], 18),
    cube([-3.0, 5.0, 4.0], [3.0, 10.0, 9.0], [27, 27, 29], 15),
    cube([-3.0, 10.0, 5.0], [-1.5, 12.0, 7.0], [24, 24, 26], 12),
    cube([1.5, 10.0, 5.0], [3.0, 12.0, 7.0], [24, 24, 26], 12),
    cube([-2.5, 7.0, 9.0], [-1.0, 8.0, 9.1], [167, 186, 62], 0),
    cube([1.0, 7.0, 9.0], [2.5, 8.0, 9.1], [167, 186, 62], 0),
];
const CAT_LEG: &[Cuboid] = &[cube([-0.8, -3.0, -0.8], [0.8, 0.0, 0.8], [27, 27, 29], 16)];
const CAT_TAIL: &[Cuboid] = &[cube([-0.6, 0.0, -8.0], [0.6, 1.2, 0.0], [27, 27, 29], 18)];
const PILLAGER_BODY: &[Cuboid] = &[
    cube([-4.0, 12.0, -2.0], [4.0, 24.0, 2.0], [86, 69, 63], 28),
    cube([-4.0, 19.0, 2.0], [4.0, 21.0, 2.1], [158, 151, 140], 20),
];
const PILLAGER_HEAD: &[Cuboid] = &[
    cube([-4.0, 0.0, -4.0], [4.0, 8.0, 4.0], [149, 153, 148], 20),
    cube([-1.0, 1.0, 4.0], [1.0, 5.0, 6.0], [135, 140, 135], 18),
    cube([-3.0, 4.0, 4.0], [-1.0, 5.0, 4.1], [194, 211, 175], 0),
    cube([1.0, 4.0, 4.0], [3.0, 5.0, 4.1], [194, 211, 175], 0),
    cube([-3.0, 5.0, 4.0], [3.0, 6.0, 4.1], [49, 45, 43], 0),
];
const PILLAGER_ARM: &[Cuboid] = &[cube([-7.0, -3.0, -2.0], [7.0, 0.0, 2.0], [103, 91, 85], 22)];
const PILLAGER_LEG: &[Cuboid] = &[cube([-2.0, -12.0, -2.0], [2.0, 0.0, 2.0], [58, 53, 49], 24)];
const CROSSBOW: &[Cuboid] = &[
    cube([-7.0, -0.5, 3.0], [7.0, 0.5, 5.0], [99, 66, 40], 18),
    cube([-1.0, -1.0, -2.0], [1.0, 1.0, 8.0], [111, 78, 42], 18),
    cube([-7.0, -0.1, 1.0], [7.0, 0.1, 1.3], [198, 188, 167], 0),
];
const TURTLE_SHELL: &[Cuboid] = &[
    cube([-7.0, 2.0, -8.0], [7.0, 6.0, 8.0], [70, 110, 62], 26),
    cube([-6.0, 6.0, -6.0], [6.0, 8.0, 6.0], [86, 126, 74], 22),
    cube([-7.0, 0.0, -8.0], [7.0, 2.0, 8.0], [214, 204, 150], 12),
];
const TURTLE_HEAD: &[Cuboid] = &[
    cube([-2.5, 0.0, 0.0], [2.5, 4.0, 5.0], [102, 168, 96], 16),
    cube([-2.6, 2.0, 3.0], [-2.5, 3.0, 4.0], [22, 22, 22], 0),
    cube([2.5, 2.0, 3.0], [2.6, 3.0, 4.0], [22, 22, 22], 0),
];
const TURTLE_FLIPPER: &[Cuboid] = &[cube([-1.0, -1.0, -2.0], [1.0, 1.0, 2.0], [102, 168, 96], 14)];
const TURTLE_SIDE_FLIPPER: &[Cuboid] = &[cube([0.0, -0.5, -2.0], [6.0, 0.5, 2.0], [102, 168, 96], 14)];

pub(super) fn pose(m: &Mob, time: f32) -> Parts {
    let tail = Quat::from_rotation_y((time * 5.0 + m.limb_phase).sin() * 0.3);
    match m.kind {
        MobKind::Cod | MobKind::Salmon | MobKind::TropicalFish => parts![
            part(
                match m.kind {
                    MobKind::Salmon => SALMON,
                    MobKind::TropicalFish => TROPICAL,
                    _ => FISH,
                },
                [0.0; 3],
                Quat::IDENTITY
            ),
            part(FISH_TAIL, [0.0, 3.0, if m.kind == MobKind::Salmon { -7.0 } else { -4.5 }], tail)
        ],
        MobKind::Pufferfish => {
            let mut p = parts![part(PUFFER, [0.0; 3], Quat::IDENTITY)];
            if m.aquatic.as_ref().is_some_and(|a| a.puff > 0) {
                for i in 0..8 {
                    let a = i as f32 * PI * 0.25;
                    p.push(part(SPIKE, [a.sin() * 3.0, 3.0, a.cos() * 3.0], Quat::from_rotation_y(a)));
                }
            }
            p
        }
        MobKind::Squid | MobKind::GlowSquid => {
            let mut p = parts![part(SQUID, [0.0; 3], Quat::IDENTITY)];
            for i in 0..8 {
                let a = i as f32 * PI * 0.25;
                p.push(part(
                    TENTACLE,
                    [a.sin() * 3.0, 6.0, a.cos() * 3.0],
                    Quat::from_rotation_y(a) * Quat::from_rotation_x(0.4 + (time * 2.0).sin() * 0.3),
                ));
            }
            p
        }
        MobKind::Dolphin => parts![
            part(DOLPHIN, [0.0; 3], Quat::IDENTITY),
            part(FLUKE, [0.0, 3.0, -9.0], Quat::from_rotation_x((time * 4.0).sin() * 0.25))
        ],
        MobKind::Axolotl => parts![part(AXOLOTL, [0.0; 3], Quat::IDENTITY), part(AXO_TAIL, [0.0, 1.0, -5.0], tail)],
        MobKind::Guardian | MobKind::ElderGuardian => {
            let mut p = parts![part(GUARDIAN, [0.0; 3], Quat::IDENTITY), part(GUARD_TAIL, [0.0, 5.0, -6.0], tail)];
            for i in 0..8 {
                let a = i as f32 * PI * 0.25;
                let extent = if m.aquatic.as_ref().is_some_and(|a| a.beam.is_some()) { 4.0 } else { 6.0 };
                p.push(part(SPIKE, [a.sin() * extent, 6.0, a.cos() * extent], Quat::from_rotation_y(a)));
            }
            p
        }
        MobKind::Cat => parts![
            part(CAT, [0.0; 3], Quat::IDENTITY),
            part(CAT_TAIL, [0.0, 5.0, -6.0], tail),
            part(CAT_LEG, [-2.0, 3.0, 3.0], Quat::from_rotation_x(m.limb_phase.sin() * 0.5)),
            part(CAT_LEG, [2.0, 3.0, 3.0], Quat::from_rotation_x(-m.limb_phase.sin() * 0.5)),
            part(CAT_LEG, [-2.0, 3.0, -4.0], Quat::IDENTITY),
            part(CAT_LEG, [2.0, 3.0, -4.0], Quat::IDENTITY)
        ],
        MobKind::Pillager => parts![
            part(PILLAGER_BODY, [0.0; 3], Quat::IDENTITY),
            part(PILLAGER_LEG, [-2.0, 12.0, 0.0], Quat::from_rotation_x(m.limb_phase.sin() * m.limb_amp)),
            part(PILLAGER_LEG, [2.0, 12.0, 0.0], Quat::from_rotation_x(-m.limb_phase.sin() * m.limb_amp)),
            part(PILLAGER_HEAD, [0.0, 24.0, 0.0], Quat::IDENTITY),
            part(PILLAGER_ARM, [0.0, 20.0, 0.0], Quat::from_rotation_x(-0.5)),
            part(CROSSBOW, [0.0, 18.0, 5.0], Quat::IDENTITY)
        ],
        MobKind::Turtle => {
            let paddle = if m.in_water { (time * 4.0).sin() * 0.6 } else { m.limb_phase.sin() * m.limb_amp };
            parts![
                part(TURTLE_SHELL, [0.0; 3], Quat::IDENTITY),
                part(TURTLE_HEAD, [0.0, 2.0, 8.0], Quat::IDENTITY),
                part(TURTLE_SIDE_FLIPPER, [6.0, 2.0, 4.0], Quat::from_rotation_y(-0.4 + paddle)),
                part(TURTLE_SIDE_FLIPPER, [-6.0, 2.0, 4.0], Quat::from_rotation_y(PI + 0.4 - paddle)),
                part(TURTLE_FLIPPER, [-4.0, 2.0, -8.0], Quat::from_rotation_y(paddle)),
                part(TURTLE_FLIPPER, [4.0, 2.0, -8.0], Quat::from_rotation_y(-paddle))
            ]
        }
        _ => Parts::new(),
    }
}
pub(super) fn beam(m: &Mob, camera: DVec3, out: &mut Vec<EntityVertex>) {
    let Some(a) = &m.aquatic else { return };
    let Some(end) = a.beam.filter(|_| a.charge > 0.0) else { return };
    let start = m.pos + DVec3::Y * m.shape().height * 0.5;
    let delta = (end - start).as_vec3();
    let length = delta.length();
    if length < 0.01 {
        return;
    }
    let rot = Quat::from_rotation_arc(Vec3::Z, delta / length);
    let origin = (start - camera).as_vec3();
    let c = cube([-0.025, -0.025, 0.0], [0.025, 0.025, length], [220, 110, 65], 0);
    push_cuboid(out, &c, &|p| origin + rot * p, rot, ([255, 0, 0, 255], 255), ([255.0; 3], 0.0), 0.0);
}
