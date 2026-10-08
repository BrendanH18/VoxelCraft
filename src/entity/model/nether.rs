//! Box models for the newer Nether mobs, in the same pixel space as the
//! rest of `model`. Humanoids keep the body, legs, arms, head part order so
//! worn armor lines up (`humanoid_armor`).

use glam::Quat;

use super::{Cuboid, DARK, GOLD, Mob, MobKind, PI, Parts, Rgb, WHITE, cube, part};
use crate::entity::mob::Ai;
use crate::entity::nether::Weapon;

const SKIN: Rgb = [232, 160, 150];
const SNOUT: Rgb = [242, 178, 168];
const LEATHER: Rgb = [118, 80, 44];
const LEATHER_DARK: Rgb = [84, 56, 32];
const TUSK: Rgb = [240, 232, 206];

const BODY: &[Cuboid] = &[
    cube([-4.0, 12.0, -2.0], [4.0, 24.0, 2.0], SKIN, 36),
    // Leather jerkin, belt with a gold buckle, and loincloth.
    cube([-4.1, 17.0, -2.1], [4.1, 24.1, 2.1], LEATHER, 28),
    cube([-4.15, 12.0, -2.15], [4.15, 13.5, 2.15], LEATHER_DARK, 20),
    cube([-1.0, 12.2, 2.15], [1.0, 13.3, 2.25], GOLD, 10),
    cube([-2.5, 9.0, 2.1], [2.5, 12.0, 2.3], LEATHER, 28),
];
const LEG: &[Cuboid] = &[
    cube([-2.0, -12.0, -2.0], [2.0, 0.0, 2.0], SKIN, 36),
    cube([-2.05, -12.0, -2.05], [2.05, -8.0, 2.05], LEATHER_DARK, 24),
];
const ARM: &[Cuboid] = &[cube([-2.0, -10.0, -2.0], [2.0, 2.0, 2.0], SKIN, 36)];
const SWORD_ARM: &[Cuboid] = &[
    cube([-2.0, -10.0, -2.0], [2.0, 2.0, 2.0], SKIN, 36),
    cube([-0.5, -10.0, 1.0], [0.5, -9.0, 3.0], [100, 70, 30], 10),
    cube([-1.5, -10.0, 3.0], [1.5, -9.0, 3.5], GOLD, 10),
    cube([-0.5, -10.0, 3.5], [0.5, -9.0, 11.0], GOLD, 10),
];
const CROSSBOW_ARM: &[Cuboid] = &[
    cube([-2.0, -10.0, -2.0], [2.0, 2.0, 2.0], SKIN, 36),
    // Stock along the forearm, limbs across its far end.
    cube([-0.75, -10.5, -1.0], [0.75, -9.0, 8.0], [137, 103, 39], 14),
    cube([-5.0, -10.25, 6.5], [5.0, -9.25, 7.5], [96, 96, 104], 10),
    cube([-4.5, -10.0, 4.0], [4.5, -9.5, 4.3], [225, 225, 225], 0),
];
/// Off-hand arm holding up an admired gold ingot.
const GOLD_ARM: &[Cuboid] =
    &[cube([-2.0, -10.0, -2.0], [2.0, 2.0, 2.0], SKIN, 36), cube([-1.5, -11.5, -1.0], [1.5, -10.0, 3.0], GOLD, 12)];
const HEAD: &[Cuboid] = &[
    cube([-5.0, 0.0, -4.0], [5.0, 8.0, 4.0], SKIN, 36),
    cube([-2.0, 0.0, 4.0], [2.0, 4.0, 5.0], SNOUT, 14),
    cube([-1.5, 1.5, 5.0], [-0.5, 2.5, 5.1], DARK, 0),
    cube([0.5, 1.5, 5.0], [1.5, 2.5, 5.1], DARK, 0),
    // Tusks either side of the snout.
    cube([-3.0, 0.0, 4.0], [-2.0, 2.0, 5.0], TUSK, 6),
    cube([2.0, 0.0, 4.0], [3.0, 2.0, 5.0], TUSK, 6),
    cube([-4.0, 4.0, 4.0], [-3.0, 5.0, 4.1], WHITE, 0),
    cube([-3.0, 4.0, 4.0], [-2.0, 5.0, 4.1], DARK, 0),
    cube([3.0, 4.0, 4.0], [4.0, 5.0, 4.1], WHITE, 0),
    cube([2.0, 4.0, 4.0], [3.0, 5.0, 4.1], DARK, 0),
];
/// Brutes wear a black tunic trimmed with gold over the same build.
const BRUTE_CLOTH: Rgb = [38, 34, 40];
const BRUTE_BODY: &[Cuboid] = &[
    cube([-4.0, 12.0, -2.0], [4.0, 24.0, 2.0], SKIN, 36),
    cube([-4.1, 14.0, -2.1], [4.1, 24.1, 2.1], BRUTE_CLOTH, 22),
    cube([-4.15, 20.0, 2.1], [4.15, 21.0, 2.2], GOLD, 10),
    cube([-4.15, 12.0, -2.15], [4.15, 14.0, 2.15], LEATHER_DARK, 20),
    cube([-1.0, 12.3, 2.15], [1.0, 13.7, 2.25], GOLD, 10),
    cube([-2.5, 9.0, 2.1], [2.5, 12.0, 2.3], BRUTE_CLOTH, 22),
];
const BRUTE_LEG: &[Cuboid] = &[
    cube([-2.0, -12.0, -2.0], [2.0, 0.0, 2.0], SKIN, 36),
    cube([-2.05, -12.0, -2.05], [2.05, -7.0, 2.05], BRUTE_CLOTH, 22),
];
const AXE_ARM: &[Cuboid] = &[
    cube([-2.0, -10.0, -2.0], [2.0, 2.0, 2.0], SKIN, 36),
    // A golden axe: handle forward from the fist, head on top.
    cube([-0.5, -10.0, -1.0], [0.5, -9.0, 10.0], [137, 103, 39], 10),
    cube([-0.6, -9.0, 6.5], [0.6, -5.0, 10.0], GOLD, 10),
];

/// Hoglin and zoglin boxes in one palette each: body, mane, legs, head.
macro_rules! tusker {
    ($body:ident, $front:ident, $back:ident, $head:ident, $fur:expr, $mane:expr, $snout:expr, $eye:expr) => {
        const $body: &[Cuboid] = &[
            cube([-8.0, 9.0, -13.0], [8.0, 20.0, 13.0], $fur, 34),
            // A bristly crest down the back.
            cube([-1.0, 20.0, -10.0], [1.0, 23.5, 7.0], $mane, 40),
        ];
        const $front: &[Cuboid] = &[
            cube([-3.0, -12.0, -3.0], [3.0, 0.0, 3.0], $fur, 34),
            cube([-3.05, -12.0, -3.05], [3.05, -10.0, 3.05], HOOF, 12),
        ];
        const $back: &[Cuboid] = &[
            cube([-2.5, -12.0, -2.5], [2.5, 0.0, 2.5], $fur, 34),
            cube([-2.55, -12.0, -2.55], [2.55, -10.0, 2.55], HOOF, 12),
        ];
        const $head: &[Cuboid] = &[
            cube([-7.0, -3.0, 0.0], [7.0, 3.0, 16.0], $fur, 34),
            cube([-4.0, -2.0, 16.0], [4.0, 2.0, 16.3], $snout, 14),
            cube([-2.5, -0.5, 16.3], [-1.0, 0.5, 16.4], DARK, 0),
            cube([1.0, -0.5, 16.3], [2.5, 0.5, 16.4], DARK, 0),
            cube([-7.1, 1.0, 9.0], [-6.9, 2.0, 11.0], $eye, 0),
            cube([6.9, 1.0, 9.0], [7.1, 2.0, 11.0], $eye, 0),
            // Tusks curving up from the jaw, and floppy ears.
            cube([-9.0, -2.0, 11.0], [-7.0, 6.0, 13.0], TUSK, 6),
            cube([7.0, -2.0, 11.0], [9.0, 6.0, 13.0], TUSK, 6),
            cube([-11.0, 1.0, 2.0], [-7.0, 2.0, 6.0], $fur, 34),
            cube([7.0, 1.0, 2.0], [11.0, 2.0, 6.0], $fur, 34),
        ];
    };
}
const HOOF: Rgb = [70, 50, 40];
tusker!(HOGLIN_BODY, HOGLIN_FRONT, HOGLIN_BACK, HOGLIN_HEAD, [188, 122, 98], [92, 56, 36], [214, 150, 132], DARK);
tusker!(
    ZOGLIN_BODY,
    ZOGLIN_FRONT,
    ZOGLIN_BACK,
    ZOGLIN_HEAD,
    [224, 176, 170],
    [236, 228, 216],
    [196, 108, 112],
    [200, 36, 40]
);
/// Zoglin flesh showing through the hide.
const ZOGLIN_ROT: &[Cuboid] = &[
    cube([-8.1, 11.0, -6.0], [-7.9, 18.0, 4.0], [150, 70, 76], 30),
    cube([7.9, 13.0, -10.0], [8.1, 19.0, -2.0], [150, 70, 76], 30),
];

/// Strider boxes in one skin colour: warm red on lava, purple-grey cold.
macro_rules! strider {
    ($body:ident, $leg:ident, $skin:expr, $dark:expr) => {
        const $body: &[Cuboid] = &[
            cube([-8.0, 14.0, -8.0], [8.0, 27.0, 8.0], $skin, 30),
            // Eyes, a wide mouth and bristles along the top edges.
            cube([-5.0, 21.0, 8.0], [-2.0, 23.0, 8.1], DARK, 0),
            cube([2.0, 21.0, 8.0], [5.0, 23.0, 8.1], DARK, 0),
            cube([-6.0, 17.0, 8.0], [6.0, 18.0, 8.1], $dark, 0),
            cube([-8.5, 26.0, -6.0], [-7.5, 29.0, -5.0], $dark, 10),
            cube([-8.5, 26.0, 0.0], [-7.5, 29.0, 1.0], $dark, 10),
            cube([-8.5, 26.0, 5.0], [-7.5, 29.0, 6.0], $dark, 10),
            cube([7.5, 26.0, -6.0], [8.5, 29.0, -5.0], $dark, 10),
            cube([7.5, 26.0, 0.0], [8.5, 29.0, 1.0], $dark, 10),
            cube([7.5, 26.0, 5.0], [8.5, 29.0, 6.0], $dark, 10),
        ];
        const $leg: &[Cuboid] = &[cube([-2.0, -14.0, -2.0], [2.0, 0.0, 2.0], $skin, 30)];
    };
}
strider!(STRIDER_BODY, STRIDER_LEG, [178, 54, 58], [118, 32, 38]);
strider!(COLD_STRIDER_BODY, COLD_STRIDER_LEG, [126, 98, 126], [82, 60, 86]);

const EAR: &[Cuboid] = &[cube([-0.5, -5.0, -2.0], [0.5, 0.0, 2.0], SKIN, 36)];

/// Animated parts for the mobs this module draws.
pub(super) fn pose(m: &Mob, time: f32) -> Parts {
    let swing = m.limb_phase.sin() * m.limb_amp * 0.9;
    let head = Quat::from_rotation_y(-m.head_yaw) * Quat::from_rotation_x(-m.head_pitch);
    let rx = Quat::from_rotation_x;
    match m.kind {
        MobKind::Piglin => {
            let n = m.nether.as_deref();
            let weapon = n.map_or(Weapon::None, |n| n.weapon);
            let admiring = n.is_some_and(|n| n.is_admiring());
            let fighting = m.ai == Ai::Chase;
            let chop = if m.attack_anim > 0.0 { (m.attack_anim / 0.35 * PI).sin() * 0.9 } else { 0.0 };
            let (right, right_rot) = match weapon {
                Weapon::GoldenSword if fighting => (SWORD_ARM, rx(-1.1 + chop)),
                Weapon::GoldenSword => (SWORD_ARM, rx(-0.4 - swing * 0.5)),
                Weapon::GoldenAxe if fighting => (AXE_ARM, rx(-1.1 + chop)),
                Weapon::GoldenAxe => (AXE_ARM, rx(-0.4 - swing * 0.5)),
                // Crossbow held level at the target.
                Weapon::Crossbow if fighting => (CROSSBOW_ARM, rx(-PI / 2.0 - m.head_pitch)),
                Weapon::Crossbow => (CROSSBOW_ARM, rx(-0.3 - swing * 0.5)),
                Weapon::None => (ARM, rx(-swing * 0.6)),
            };
            let (left, left_rot) = if admiring {
                (GOLD_ARM, rx(-1.0))
            } else if weapon == Weapon::Crossbow && fighting {
                (ARM, rx(-PI / 2.0 - m.head_pitch) * Quat::from_rotation_y(0.5))
            } else {
                (ARM, rx(swing * 0.6))
            };
            // Ears flop with the walk.
            let flop = 0.45 + (time * 3.0 + m.limb_phase).sin() * 0.08 * (0.3 + m.limb_amp);
            let mut parts = Parts::new();
            parts.push(part(BODY, [0.0; 3], Quat::IDENTITY));
            parts.push(part(LEG, [-2.0, 12.0, 0.0], rx(swing)));
            parts.push(part(LEG, [2.0, 12.0, 0.0], rx(-swing)));
            parts.push(part(left, [-6.0, 22.0, 0.0], left_rot));
            parts.push(part(right, [6.0, 22.0, 0.0], right_rot));
            parts.push(part(HEAD, [0.0, 24.0, 0.0], head));
            let ear = |side: f32| head * glam::Vec3::new(5.0 * side, 6.0, 0.0);
            let e = ear(-1.0);
            parts.push(part(EAR, [e.x, 24.0 + e.y, e.z], head * Quat::from_rotation_z(-flop)));
            let e = ear(1.0);
            parts.push(part(EAR, [e.x, 24.0 + e.y, e.z], head * Quat::from_rotation_z(flop)));
            parts
        }
        MobKind::PiglinBrute => {
            let chop = if m.attack_anim > 0.0 { (m.attack_anim / 0.35 * PI).sin() * 1.1 } else { 0.0 };
            let axe = if m.ai == Ai::Chase { rx(-1.3 + chop) } else { rx(-0.4 - swing * 0.5) };
            let flop = 0.4 + (time * 3.0 + m.limb_phase).sin() * 0.06 * (0.3 + m.limb_amp);
            let mut parts = Parts::new();
            parts.push(part(BRUTE_BODY, [0.0; 3], Quat::IDENTITY));
            parts.push(part(BRUTE_LEG, [-2.0, 12.0, 0.0], rx(swing)));
            parts.push(part(BRUTE_LEG, [2.0, 12.0, 0.0], rx(-swing)));
            parts.push(part(ARM, [-6.0, 22.0, 0.0], rx(swing * 0.6)));
            parts.push(part(AXE_ARM, [6.0, 22.0, 0.0], axe));
            parts.push(part(HEAD, [0.0, 24.0, 0.0], head));
            for side in [-1.0f32, 1.0] {
                let e = head * glam::Vec3::new(5.0 * side, 6.0, 0.0);
                parts.push(part(EAR, [e.x, 24.0 + e.y, e.z], head * Quat::from_rotation_z(flop * side)));
            }
            parts
        }
        MobKind::Hoglin | MobKind::Zoglin => {
            let zoglin = m.kind == MobKind::Zoglin;
            let (body, front, back, head_box) = if zoglin {
                (ZOGLIN_BODY, ZOGLIN_FRONT, ZOGLIN_BACK, ZOGLIN_HEAD)
            } else {
                (HOGLIN_BODY, HOGLIN_FRONT, HOGLIN_BACK, HOGLIN_HEAD)
            };
            // The head hangs low and tosses upward on an attack.
            let toss = if m.attack_anim > 0.0 { (m.attack_anim / 0.35 * PI).sin() * 1.1 } else { 0.0 };
            let mut parts = Parts::new();
            parts.push(part(body, [0.0; 3], Quat::IDENTITY));
            parts.push(part(front, [-4.5, 12.0, 8.0], rx(swing)));
            parts.push(part(front, [4.5, 12.0, 8.0], rx(-swing)));
            parts.push(part(back, [-4.5, 12.0, -9.0], rx(-swing)));
            parts.push(part(back, [4.5, 12.0, -9.0], rx(swing)));
            parts.push(part(head_box, [0.0, 15.0, 12.0], head * rx(0.6 - toss)));
            if zoglin {
                parts.push(part(ZOGLIN_ROT, [0.0; 3], Quat::IDENTITY));
            }
            parts
        }
        MobKind::Strider => {
            let cold = m.nether.as_ref().is_some_and(|n| n.cold);
            let (body, leg) = if cold { (COLD_STRIDER_BODY, COLD_STRIDER_LEG) } else { (STRIDER_BODY, STRIDER_LEG) };
            // Long, slow strides; a cold strider shivers.
            let stride = swing * 0.7;
            let shiver = if cold { (time * 45.0).sin() * 0.04 } else { 0.0 };
            let mut parts = Parts::new();
            parts.push(part(body, [0.0; 3], Quat::from_rotation_z(shiver)));
            parts.push(part(leg, [-4.0, 14.0, 0.0], rx(stride)));
            parts.push(part(leg, [4.0, 14.0, 0.0], rx(-stride)));
            parts
        }
        _ => Parts::new(),
    }
}
