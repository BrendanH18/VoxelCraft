//! Compact, animated original models authored in sixteenths of a block.
use super::*;
const FISH: &[Cuboid] = &[
    cube([-2.0, 1.0, -5.0], [2.0, 5.0, 5.0], [181, 155, 113], 22),
    cube([-2.1, 4.0, 3.0], [-2.0, 5.0, 4.0], [25, 25, 22], 0),
    cube([2.0, 4.0, 3.0], [2.1, 5.0, 4.0], [25, 25, 22], 0),
    cube([-0.2, 5.0, -2.0], [0.2, 7.0, 1.0], [132, 107, 67], 18),
    // v0.18 Track 6: gill lines, a lateral line and a paler belly.
    cube([-2.05, 1.5, 2.0], [2.05, 4.6, 2.3], [150, 124, 88], 0),
    cube([-2.05, 2.8, -4.5], [2.05, 3.1, 2.0], [150, 124, 88], 0),
    cube([-1.8, 0.95, -4.5], [1.8, 1.0, 4.5], [222, 206, 170], 0),
];
const SALMON: &[Cuboid] = &[
    cube([-2.0, 1.0, -7.0], [2.0, 5.0, 7.0], [149, 69, 55], 22),
    cube([-2.0, 5.0, -7.0], [2.0, 6.0, 4.0], [89, 104, 64], 20),
    cube([-2.1, 3.0, 5.0], [-2.0, 4.0, 6.0], [25, 25, 22], 0),
    cube([2.0, 3.0, 5.0], [2.1, 4.0, 6.0], [25, 25, 22], 0),
    // v0.18 Track 6: gill lines and dark spots along the back.
    cube([-2.05, 1.5, 3.5], [2.05, 5.05, 3.8], [112, 50, 40], 0),
    cube([-2.05, 4.0, -4.0], [-2.0, 4.6, -3.2], [52, 46, 38], 0),
    cube([-2.05, 4.2, 0.0], [-2.0, 4.8, 0.8], [52, 46, 38], 0),
    cube([2.0, 4.0, -2.0], [2.05, 4.6, -1.2], [52, 46, 38], 0),
    cube([2.0, 4.2, 1.6], [2.05, 4.8, 2.4], [52, 46, 38], 0),
];
const TROPICAL: &[Cuboid] = &[
    cube([-1.5, 0.5, -3.0], [1.5, 5.5, 4.0], [245, 155, 45], 10),
    cube([-1.55, 0.5, -0.5], [1.55, 5.5, 0.5], [245, 240, 210], 0),
    cube([-0.2, 5.5, -2.5], [0.2, 7.0, 3.0], [242, 174, 65], 0),
    cube([-1.6, 3.5, 2.0], [-1.5, 4.5, 3.0], [22, 22, 22], 0),
    cube([1.5, 3.5, 2.0], [1.6, 4.5, 3.0], [22, 22, 22], 0),
    // v0.18 Track 6: a second band toward the tail and eye glints.
    cube([-1.55, 0.5, -2.4], [1.55, 5.5, -1.8], [245, 240, 210], 0),
    cube([-1.65, 4.0, 2.5], [-1.6, 4.3, 2.8], [236, 236, 226], 0),
    cube([1.6, 4.0, 2.5], [1.65, 4.3, 2.8], [236, 236, 226], 0),
];
const FISH_TAIL: &[Cuboid] = &[cube([-0.2, -2.5, -3.0], [0.2, 2.5, 0.0], [135, 120, 88], 12)];
const PUFFER: &[Cuboid] = &[
    cube([-3.0, 0.0, -3.0], [3.0, 6.0, 3.0], [215, 179, 76], 28),
    cube([-3.0, 0.0, -3.0], [3.0, 2.0, 3.0], [221, 212, 154], 15),
    cube([-2.0, 3.5, 3.0], [-0.8, 5.0, 3.1], [20, 25, 28], 0),
    cube([0.8, 3.5, 3.0], [2.0, 5.0, 3.1], [20, 25, 28], 0),
    // v0.18 Track 6: a pursed mouth and brown spots on top.
    cube([-0.8, 1.6, 3.0], [0.8, 2.4, 3.1], [120, 82, 40], 0),
    cube([-2.0, 6.0, -1.5], [-1.0, 6.05, -0.5], [150, 110, 46], 0),
    cube([0.6, 6.0, 0.4], [1.6, 6.05, 1.4], [150, 110, 46], 0),
    cube([-0.4, 6.0, -2.6], [0.6, 6.05, -1.6], [150, 110, 46], 0),
];
const SPIKE: &[Cuboid] = &[cube([-0.5, -0.5, 0.0], [0.5, 0.5, 3.0], [236, 190, 86], 0)];
const SQUID: &[Cuboid] = &[
    cube([-4.0, 5.0, -4.0], [4.0, 13.0, 4.0], [49, 71, 80], 22),
    cube([-3.0, 6.0, 4.0], [-1.0, 8.0, 4.1], [232, 232, 232], 0),
    cube([1.0, 6.0, 4.0], [3.0, 8.0, 4.1], [232, 232, 232], 0),
    cube([-2.0, 6.0, 4.1], [-1.0, 7.0, 4.2], [15, 20, 23], 0),
    cube([1.0, 6.0, 4.1], [2.0, 7.0, 4.2], [15, 20, 23], 0),
    // v0.18 Track 6: paler stripes down the mantle and a beak.
    cube([-4.05, 9.0, -1.0], [4.05, 13.05, 0.0], SQUID_STRIPE, 0),
    cube([-1.0, 9.0, -4.05], [0.0, 13.05, 4.05], SQUID_STRIPE, 0),
    cube([-0.6, 5.0, 4.0], [0.6, 5.6, 4.05], [30, 34, 36], 0),
];
/// The mantle stripes; the glow squid recolours them.
pub(super) const SQUID_STRIPE: Rgb = [70, 96, 106];
const TENTACLE: &[Cuboid] = &[cube([-0.7, -7.0, -0.7], [0.7, 0.0, 0.7], [49, 71, 80], 16)];
const DOLPHIN: &[Cuboid] = &[
    cube([-4.0, 1.0, -9.0], [4.0, 7.0, 8.0], [116, 143, 153], 16),
    cube([-2.0, 2.0, 8.0], [2.0, 4.0, 13.0], [136, 158, 164], 14),
    cube([-0.5, 7.0, -4.0], [0.5, 11.0, 1.0], [98, 130, 145], 12),
    cube([-8.0, 1.0, -3.0], [8.0, 2.0, 0.0], [101, 132, 144], 14),
    cube([-4.1, 4.0, 5.0], [-4.0, 5.0, 6.0], [20, 25, 29], 0),
    cube([4.0, 4.0, 5.0], [4.1, 5.0, 6.0], [20, 25, 29], 0),
    // v0.18 Track 6: a smiling mouth line, a blowhole and a paler belly.
    cube([-2.05, 2.5, 9.0], [2.05, 2.8, 13.05], [80, 100, 108], 0),
    cube([-0.5, 7.0, 2.0], [0.5, 7.05, 3.0], [70, 96, 106], 0),
    cube([-3.5, 0.95, -7.0], [3.5, 1.0, 7.0], [176, 196, 200], 0),
];
const FLUKE: &[Cuboid] = &[cube([-5.0, -0.5, -5.0], [5.0, 0.5, 0.0], [112, 140, 150], 12)];
/// v1.0 gap Track 3: an original baby dolphin on `BabyDolphinModel`'s
/// proportions (a 6 × 5 × 8 body, a 6 × 5 × 4 head with a 2-pixel nose, a
/// 4 × 3 × 7 tail and an 8-wide fluke), paler than the adult.
const BABY_DOLPHIN: &[Cuboid] = &[
    cube([-3.0, 0.0, -4.0], [3.0, 5.0, 4.0], [128, 155, 165], 14),
    cube([-3.0, -0.5, 4.0], [3.0, 4.5, 8.0], [140, 164, 172], 12),
    cube([-1.0, 1.0, 8.0], [1.0, 3.0, 10.0], [150, 172, 178], 10),
    cube([-2.0, 1.0, -11.0], [2.0, 4.0, -4.0], [128, 155, 165], 12),
    cube([-0.5, 4.5, -2.0], [0.5, 7.0, 1.5], [110, 140, 152], 10),
    cube([-6.0, 0.5, -1.0], [6.0, 1.3, 1.5], [116, 146, 156], 10),
    cube([-3.05, 2.8, 6.0], [-3.0, 3.6, 6.8], [20, 25, 29], 0),
    cube([3.0, 2.8, 6.0], [3.05, 3.6, 6.8], [20, 25, 29], 0),
    cube([-2.5, -0.05, -3.5], [2.5, 0.0, 3.5], [186, 204, 208], 0),
];
const BABY_FLUKE: &[Cuboid] = &[cube([-4.0, -0.5, -4.0], [4.0, 0.5, 0.0], [124, 150, 160], 10)];
const AXOLOTL: &[Cuboid] = &[
    cube([-3.0, 1.0, -5.0], [3.0, 4.0, 3.0], [237, 160, 184], 14),
    cube([-4.0, 0.5, 2.0], [4.0, 5.0, 7.0], [243, 179, 198], 14),
    cube([-3.0, 3.0, 7.0], [-2.0, 4.0, 7.1], [22, 20, 29], 0),
    cube([2.0, 3.0, 7.0], [3.0, 4.0, 7.1], [22, 20, 29], 0),
    cube([-6.0, 2.0, 3.0], [-4.0, 4.0, 5.0], [197, 83, 132], 0),
    cube([4.0, 2.0, 3.0], [6.0, 4.0, 5.0], [197, 83, 132], 0),
    cube([-4.0, 0.0, -4.0], [4.0, 1.0, -2.0], [235, 151, 183], 12),
    cube([-4.0, 0.0, 0.0], [4.0, 1.0, 2.0], [235, 151, 183], 12),
    // v0.18 Track 6: a smile, eye glints and a frill over the head.
    cube([-1.6, 1.4, 7.0], [1.6, 1.7, 7.05], [150, 92, 112], 0),
    cube([-2.7, 3.6, 7.1], [-2.4, 3.9, 7.15], [236, 230, 236], 0),
    cube([2.4, 3.6, 7.1], [2.7, 3.9, 7.15], [236, 230, 236], 0),
    cube([-2.0, 5.0, 3.0], [2.0, 5.6, 4.0], [197, 83, 132], 0),
];
/// The axolotls' variant body colours (lucy, wild, gold, cyan, blue), in
/// the order of `aquatic` variant numbers.
pub(super) const AXOLOTL_BODIES: [Rgb; 5] =
    [[237, 160, 184], [139, 104, 81], [225, 186, 66], [184, 217, 221], [89, 101, 184]];
/// The marker colour of the gills and head frill, recoloured per variant.
pub(super) const AXOLOTL_GILL: Rgb = [197, 83, 132];
/// v0.18 Track 6: each variant's gills, a distinct colour brighter and more
/// saturated than its body: hot pink on the pink axolotl, rose on the wild
/// brown one, deep orange on the gold, pink on the cyan, violet on the blue.
pub(super) const AXOLOTL_GILLS: [Rgb; 5] =
    [[222, 70, 136], [218, 120, 124], [244, 120, 30], [232, 140, 200], [176, 98, 222]];
const AXO_TAIL: &[Cuboid] = &[cube([-0.5, 0.0, -6.0], [0.5, 4.0, 0.0], [217, 130, 171], 14)];
const GUARDIAN: &[Cuboid] = &[
    cube([-6.0, 0.0, -6.0], [6.0, 13.6, 6.0], [105, 138, 129], 35),
    cube([-4.0, 3.0, 6.0], [4.0, 9.0, 6.2], [214, 212, 177], 12),
    cube([-1.5, 4.0, 6.2], [1.5, 7.0, 6.4], [164, 60, 48], 0),
    cube([-0.6, 4.5, 6.4], [0.6, 6.5, 6.5], [26, 27, 24], 0),
    // v0.18 Track 6: plate seams round the body and a lid over the eye.
    cube([-6.05, 10.0, -6.05], [6.05, 10.5, 6.05], [70, 96, 90], 0),
    cube([-6.05, 2.0, -6.05], [6.05, 2.5, 6.05], [70, 96, 90], 0),
    cube([-4.0, 9.0, 6.0], [4.0, 9.6, 6.25], [214, 150, 80], 0),
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
    // v0.18 Track 6: slit pupils, a pink nose, a pale muzzle and
    // forehead stripes.
    cube([-1.9, 7.0, 9.1], [-1.6, 8.0, 9.15], [20, 22, 16], 0),
    cube([1.6, 7.0, 9.1], [1.9, 8.0, 9.15], [20, 22, 16], 0),
    cube([-0.5, 6.0, 9.0], [0.5, 6.6, 9.05], [214, 140, 150], 0),
    cube([-1.5, 5.0, 9.0], [1.5, 6.0, 9.02], [150, 140, 130], 0),
    cube([-1.5, 10.0, 5.5], [-0.8, 10.05, 8.0], [20, 20, 22], 0),
    cube([0.8, 10.0, 5.5], [1.5, 10.05, 8.0], [20, 20, 22], 0),
];
const CAT_COLLAR: &[Cuboid] = &[cube([-2.6, 4.0, 3.0], [2.6, 7.0, 4.0], [255, 0, 254], 0)];
const CAT_LEG: &[Cuboid] = &[cube([-0.8, -3.0, -0.8], [0.8, 0.0, 0.8], [27, 27, 29], 16)];
const CAT_TAIL: &[Cuboid] = &[cube([-0.6, 0.0, -8.0], [0.6, 1.2, 0.0], [27, 27, 29], 18)];
const TURTLE_SHELL: &[Cuboid] = &[
    cube([-7.0, 2.0, -8.0], [7.0, 6.0, 8.0], [70, 110, 62], 26),
    cube([-6.0, 6.0, -6.0], [6.0, 8.0, 6.0], [86, 126, 74], 22),
    cube([-7.0, 0.0, -8.0], [7.0, 2.0, 8.0], [214, 204, 150], 12),
    // v0.18 Track 6: seams between the shell's plates.
    cube([-0.25, 8.0, -6.0], [0.25, 8.05, 6.0], [52, 84, 46], 0),
    cube([-6.0, 8.0, -0.25], [6.0, 8.05, 0.25], [52, 84, 46], 0),
    cube([-7.05, 4.0, -8.05], [7.05, 4.4, 8.05], [56, 92, 50], 0),
];
const TURTLE_HEAD: &[Cuboid] = &[
    cube([-2.5, 0.0, 0.0], [2.5, 4.0, 5.0], [102, 168, 96], 16),
    cube([-2.6, 2.0, 3.0], [-2.5, 3.0, 4.0], [22, 22, 22], 0),
    cube([2.5, 2.0, 3.0], [2.6, 3.0, 4.0], [22, 22, 22], 0),
    // v0.18 Track 6: a beak line and eye glints.
    cube([-2.55, 0.8, 3.5], [2.55, 1.1, 5.05], [52, 86, 48], 0),
    cube([-2.65, 2.6, 3.6], [-2.6, 2.9, 3.9], [226, 226, 216], 0),
    cube([2.6, 2.6, 3.6], [2.65, 2.9, 3.9], [226, 226, 216], 0),
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
        MobKind::Dolphin if m.baby || m.age < 0 => parts![
            part(BABY_DOLPHIN, [0.0; 3], Quat::IDENTITY),
            part(BABY_FLUKE, [0.0, 2.5, -11.0], Quat::from_rotation_x((time * 5.0).sin() * 0.3))
        ],
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
        MobKind::Cat => {
            let mut p = parts![
                part(CAT, [0.0; 3], Quat::IDENTITY),
                part(CAT_TAIL, [0.0, 5.0, -6.0], tail),
                part(CAT_LEG, [-2.0, 3.0, 3.0], Quat::from_rotation_x(m.limb_phase.sin() * 0.5)),
                part(CAT_LEG, [2.0, 3.0, 3.0], Quat::from_rotation_x(-m.limb_phase.sin() * 0.5)),
                part(CAT_LEG, [-2.0, 3.0, -4.0], Quat::IDENTITY),
                part(CAT_LEG, [2.0, 3.0, -4.0], Quat::IDENTITY)
            ];
            if m.animal.as_ref().is_some_and(|a| a.owner.is_some()) {
                p.push(part(CAT_COLLAR, [0.0; 3], Quat::IDENTITY));
            }
            p
        }
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
pub(super) fn beam(m: &Mob, camera: DVec3, out: &mut EntityMesh) {
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
