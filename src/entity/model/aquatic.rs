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
/// v1.0 gap Track 7c: the pufferfish's blue pectoral fins (`*_blue_fin`).
const PUFFER_FIN_R: &[Cuboid] = &[cube([-2.0, -0.15, -1.0], [0.0, 0.15, 1.0], [70, 144, 190], 6)];
const PUFFER_FIN_L: &[Cuboid] = &[cube([0.0, -0.15, -1.0], [2.0, 0.15, 1.0], [70, 144, 190], 6)];
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
    // v0.18 Track 6: plate seams round the body and a lid over the eye.
    cube([-6.05, 10.0, -6.05], [6.05, 10.5, 6.05], [70, 96, 90], 0),
    cube([-6.05, 2.0, -6.05], [6.05, 2.5, 6.05], [70, 96, 90], 0),
    cube([-4.0, 9.0, 6.0], [4.0, 9.6, 6.25], [214, 150, 80], 0),
];
/// v1.0 gap Track 7c: the iris and pupil on their own part, which
/// [`track_eye`] slides toward what the guardian watches.
pub(super) static GUARD_EYE: &[Cuboid] = &[
    cube([-1.5, 4.0, 6.2], [1.5, 7.0, 6.4], [164, 60, 48], 0),
    cube([-0.6, 4.5, 6.4], [0.6, 6.5, 6.5], [26, 27, 24], 0),
];
/// v1.0 gap Track 7c: `GuardianModel`'s three tail joints (4, 3 and 2
/// pixels thick; 8, 7 and 6 long, the last with its fin) hanging from the
/// body's centre, given from Java's pivots; the first starts a pixel
/// inside the body's back, which is 2 pixels shallower than Java's.
const GUARD_TAIL_COLOUR: Rgb = [95, 126, 113];
static GUARD_TAIL: &[rig::Bone] = &[
    rig::bone(&[rig::jbox([-2.0, 14.0, 5.0], [4.0, 4.0, 8.0], GUARD_TAIL_COLOUR, 25)], rig::ROOT, [0.0; 3], [0.0; 3]),
    rig::bone(&[rig::jbox([0.0, 14.0, 0.0], [3.0, 3.0, 7.0], GUARD_TAIL_COLOUR, 25)], 0, [-1.5, 0.5, 12.0], [0.0; 3]),
    rig::bone(
        &[
            rig::jbox([0.0, 14.0, 0.0], [2.0, 2.0, 6.0], GUARD_TAIL_COLOUR, 25),
            rig::jbox([1.0, 10.5, 3.0], [1.0, 9.0, 9.0], [215, 161, 64], 16),
        ],
        1,
        [0.5, 0.5, 6.0],
        [0.0; 3],
    ),
];
/// v1.0 gap Track 7c: `GuardianModel`'s twelve spikes, one on each edge of
/// the body, as Java's `SPIKE_X/Y/Z` (pixels from the centre, here scaled
/// to this game's body) and `SPIKE_X/Y/Z_ROT` (turns of pi).
const GUARD_SPIKE: &[Cuboid] = &[rig::jbox([-0.75, -3.4, -0.75], [1.5, 6.8, 1.5], [236, 190, 86], 8)];
const SPIKE_AT: [[f32; 3]; 12] = [
    [0.0, -8.0, 8.0],
    [0.0, -8.0, -8.0],
    [8.0, -8.0, 0.0],
    [-8.0, -8.0, 0.0],
    [-8.0, 0.0, -8.0],
    [8.0, 0.0, -8.0],
    [8.0, 0.0, 8.0],
    [-8.0, 0.0, 8.0],
    [0.0, 8.0, 8.0],
    [0.0, 8.0, -8.0],
    [8.0, 8.0, 0.0],
    [-8.0, 8.0, 0.0],
];
const SPIKE_ROT: [[f32; 3]; 12] = [
    [1.75, 0.0, 0.0],
    [0.25, 0.0, 0.0],
    [0.0, 0.0, 0.25],
    [0.0, 0.0, 1.75],
    [0.5, 0.25, 0.0],
    [0.5, 1.75, 0.0],
    [0.5, 1.25, 0.0],
    [0.5, 0.75, 0.0],
    [1.25, 0.0, 0.0],
    [0.75, 0.0, 0.0],
    [0.0, 0.0, 0.75],
    [0.0, 0.0, 1.25],
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

pub(super) fn pose(m: &Mob, time: f32, alpha: f32) -> Parts {
    use crate::entity::mob_anim::lerp;
    let tail = Quat::from_rotation_y((time * 5.0 + m.limb_phase * motion::WALK_FREQ).sin() * 0.3);
    let age = age_ticks(time);
    let (rx, ry, rz) = (Quat::from_rotation_x, Quat::from_rotation_y, Quat::from_rotation_z);
    match m.kind {
        MobKind::Cod | MobKind::Salmon | MobKind::TropicalFish => {
            // v1.0 gap Track 7b: `CodModel`, `SalmonModel` and the tropical
            // fish models swing the tail (the salmon's back half) on
            // `sin(0.6 * ageInTicks)`, harder out of water; their renderers
            // sway the body 4.3 degrees on the same wave and, out of water,
            // lay it on its side, lifted a little (Java's y turns and
            // offsets on this model's sides).
            let salmon = m.kind == MobKind::Salmon;
            let (amp, freq) = match (salmon, m.in_water) {
                (true, false) => (1.3, 1.7),
                (false, false) => (1.5, 1.0),
                _ => (1.0, 1.0),
            };
            let wave = (freq * 0.6 * age).sin();
            let fin = if salmon { 0.25 * amp * wave } else { 0.45 * amp * wave };
            let body_amp = if salmon { amp } else { 1.0 };
            let mut p = parts![
                part(
                    match m.kind {
                        MobKind::Salmon => SALMON,
                        MobKind::TropicalFish => TROPICAL,
                        _ => FISH,
                    },
                    [0.0; 3],
                    Quat::IDENTITY
                ),
                part(FISH_TAIL, [0.0, 3.0, if salmon { -7.0 } else { -4.5 }], ry(fin))
            ];
            let sway = ry((body_amp * 4.3 * wave).to_radians());
            if m.in_water {
                p.transform(Vec3::ZERO, sway);
            } else {
                let lift = if m.kind == MobKind::Cod { Vec3::new(-1.6, 1.6, 1.6) } else { Vec3::new(-3.2, 1.6, 0.0) };
                p.transform(sway * lift, sway * rz(-FRAC_PI_2));
            }
            p
        }
        MobKind::Pufferfish => {
            // `PufferfishRenderer` bobs it `cos(ageInTicks * 0.05) * 0.08`
            // blocks.
            let bob = (age * 0.05).cos() * 0.08 * 16.0;
            // v1.0 gap Track 7c: the pufferfish models' blue fins flap,
            // `zRot` -+0.2 +- 0.4 sin(ageInTicks * 0.2) (Java's right one on
            // -X; z turns change sign).
            let flap = -0.2 + 0.4 * (age * 0.2).sin();
            let mut p = parts![
                part(PUFFER, [0.0, bob, 0.0], Quat::IDENTITY),
                part(PUFFER_FIN_R, [-3.0, 2.0 + bob, 1.5], rz(-flap)),
                part(PUFFER_FIN_L, [3.0, 2.0 + bob, 1.5], rz(flap))
            ];
            if m.aquatic.as_ref().is_some_and(|a| a.puff > 0) {
                for i in 0..8 {
                    let a = i as f32 * PI * 0.25;
                    p.push(part(SPIKE, [a.sin() * 3.0, 3.0 + bob, a.cos() * 3.0], Quat::from_rotation_y(a)));
                }
            }
            p
        }
        MobKind::Squid | MobKind::GlowSquid => {
            // `SquidModel`: every tentacle opens outward by
            // `tentacleAngle`; `SquidRenderer` turns the body about its
            // middle, half a block up, by `xBodyRot` then `zBodyRot`
            // (`Squid.aiStep`'s state).
            let a = &m.anim;
            let open = lerp(alpha, a.o_tentacle_angle, a.tentacle_angle);
            let mut p = parts![part(SQUID, [0.0; 3], Quat::IDENTITY)];
            for i in 0..8 {
                let a = i as f32 * PI * 0.25;
                p.push(part(TENTACLE, [a.sin() * 3.0, 6.0, a.cos() * 3.0], ry(a) * rx(-open)));
            }
            let (x, z) = (lerp(alpha, a.o_x_body_rot, a.x_body_rot), lerp(alpha, a.o_z_body_rot, a.z_body_rot));
            p.turn_about(Vec3::new(0.0, 8.0, 0.0), rx(-x.to_radians()) * ry(z.to_radians()));
            p
        }
        MobKind::Dolphin => {
            // `DolphinModel`: the body pitches with the dolphin and turns
            // with its head; swimming, it rocks on `cos(ageInTicks * 0.3)`
            // and the tail and fluke beat (`isMoving`: it is moving, here
            // its walk speed, which follows the distance moved a tick).
            let moving = m.limb_amp > 1e-3;
            let wave = (age * 0.3).cos();
            let (rock, fluke) = if moving { (-0.05 - 0.05 * wave, -0.3 * wave) } else { (0.0, 0.0) };
            // (v1.0 gap Track 7c: calves draw `BabyDolphinModel`,
            // `baby_rigs::dolphin`.)
            let mut p = parts![part(DOLPHIN, [0.0; 3], Quat::IDENTITY), part(FLUKE, [0.0, 3.0, -9.0], rx(fluke))];
            let pitch = m.move_ctl.pitch.to_radians() + rock;
            p.turn_about(Vec3::new(0.0, 4.0, 0.0), ry(-m.head_yaw) * rx(pitch));
            p
        }
        MobKind::Axolotl => parts![part(AXOLOTL, [0.0; 3], Quat::IDENTITY), part(AXO_TAIL, [0.0, 1.0, -5.0], tail)],
        MobKind::Guardian | MobKind::ElderGuardian => {
            // `GuardianModel`: the tail's three joints swing `sin(tail) *
            // pi` times 0.05, 0.1 and 0.15, each on the last (v1.0 gap
            // Track 7c); the twelve spikes stand out by `1 + cos(ageInTicks
            // * 1.5 + i) / 100` less the withdrawal `(1 - spikes) * 0.55`
            // (`Guardian.aiStep`'s client state: drawn in while it swims,
            // flickering out of water). The eye is placed by `track_eye`.
            let a = &m.anim;
            let swim = lerp(alpha, a.o_tail, a.tail).sin() * PI;
            let withdrawal = (1.0 - lerp(alpha, a.o_spikes, a.spikes)) * 0.55;
            let mut tail = rig::Rig::new(GUARD_TAIL);
            // The tail's centre line 5 pixels up, its turn about the body's.
            tail.at[rig::MODEL_ROOT] = [0.0, 3.0, 0.0];
            for (i, k) in [0.05, 0.1, 0.15].into_iter().enumerate() {
                tail.y_rot(i, swim * k);
            }
            let mut p = parts![part(GUARDIAN, [0.0; 3], Quat::IDENTITY), part(GUARD_EYE, [0.0; 3], Quat::IDENTITY)];
            for t in tail.parts().iter() {
                p.push(*t);
            }
            let (centre, half) = (Vec3::new(0.0, 6.8, 0.0), Vec3::new(6.0, 6.8, 6.0) / 8.0);
            for (i, (at, rot)) in SPIKE_AT.iter().zip(SPIKE_ROT).enumerate() {
                let out = 1.0 + (age * 1.5 + i as f32).cos() * 0.01 - withdrawal;
                // Java's y runs down and its z back.
                let at = centre + Vec3::new(at[0], -at[1], -at[2]) * half * out;
                p.push(part(GUARD_SPIKE, at.to_array(), rig::java_quat(rot.map(|r| r * PI))));
            }
            p
        }
        MobKind::Cat => {
            let mut p = parts![
                part(CAT, [0.0; 3], Quat::IDENTITY),
                part(CAT_TAIL, [0.0, 5.0, -6.0], tail),
                // v1.0 gap Track 7: `AdultFelineModel`, every leg on
                // `cos(pos * 0.6662) * speed`, the left hind leading.
                // (v1.0 gap Track 7c: Java's right legs on -X.)
                part(CAT_LEG, [-2.0, 3.0, 3.0], Quat::from_rotation_x(walk_cos(m))),
                part(CAT_LEG, [2.0, 3.0, 3.0], Quat::from_rotation_x(-walk_cos(m))),
                part(CAT_LEG, [-2.0, 3.0, -4.0], Quat::from_rotation_x(-walk_cos(m))),
                part(CAT_LEG, [2.0, 3.0, -4.0], Quat::from_rotation_x(walk_cos(m)))
            ];
            if m.animal.as_ref().is_some_and(|a| a.owner.is_some()) {
                p.push(part(CAT_COLLAR, [0.0; 3], Quat::IDENTITY));
            }
            p
        }
        MobKind::Turtle => {
            // v1.0 gap Track 7: `TurtleModel`. On land the flippers sweep
            // round at five times the walk position (front 8, hind 3 times
            // the speed); off it they row at `0.6662 * 0.6` of it.
            let (pos, speed) = (m.limb_phase, m.limb_amp);
            let (ry, rx, rz) = (Quat::from_rotation_y, Quat::from_rotation_x, Quat::from_rotation_z);
            let (front, hind, row) = if !m.in_water && m.on_ground {
                let sweep = (pos * 5.0).cos() * speed;
                (-8.0 * sweep, 3.0 * sweep, 0.0)
            } else {
                (0.0, 0.0, (pos * motion::WALK_FREQ * 0.6).cos() * 0.5 * speed)
            };
            parts![
                part(TURTLE_SHELL, [0.0; 3], Quat::IDENTITY),
                part(TURTLE_HEAD, [0.0, 2.0, 8.0], Quat::IDENTITY),
                part(TURTLE_SIDE_FLIPPER, [6.0, 2.0, 4.0], ry(-0.4 + front) * rz(-row)),
                part(TURTLE_SIDE_FLIPPER, [-6.0, 2.0, 4.0], ry(PI + 0.4 - front) * rz(row)),
                part(TURTLE_FLIPPER, [-4.0, 2.0, -8.0], ry(hind) * rx(-row)),
                part(TURTLE_FLIPPER, [4.0, 2.0, -8.0], ry(-hind) * rx(row))
            ]
        }
        _ => Parts::new(),
    }
}
/// v1.0 gap Track 7c: `GuardianModel`'s eye on what the guardian watches
/// (`GuardianRenderer.getEntityToLookAt`: its attack target, else the
/// viewer): up to two pixels toward the side it is on (the square root
/// of the sine of its bearing from the guardian's heading, twice) and a
/// pixel down when it is below the eye.
pub(super) fn track_eye(m: &Mob, camera: DVec3, parts: &mut Parts) {
    if !matches!(m.kind, MobKind::Guardian | MobKind::ElderGuardian) {
        return;
    }
    let Some(eye) = parts.iter().position(|p| std::ptr::eq(p.boxes, GUARD_EYE)) else { return };
    let from = m.pos + DVec3::Y * m.shape().height * 0.5;
    let target = m.aquatic.as_ref().and_then(|a| a.beam).unwrap_or(camera);
    let down = if target.y - from.y > 0.0 { 0.0 } else { 1.0 };
    // `viewVector` flattened, and the bearing turned a quarter round.
    let view = DVec3::new(f64::from(m.yaw.cos()), 0.0, f64::from(m.yaw.sin())) * f64::from(m.head_pitch.cos());
    let d = (from - target).with_y(0.0).normalize_or_zero();
    let dot = (view.x * d.z - view.z * d.x) as f32;
    let side = dot.abs().sqrt() * 2.0 * dot.signum();
    parts.data[eye].pivot = Vec3::new(side, -down, 0.0);
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
