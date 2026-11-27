//! Box models for the newer Nether mobs, in the same pixel space as the
//! rest of `model`. Humanoids keep the body, legs, arms, head part order so
//! worn armor lines up (`humanoid_armor`).

use glam::Quat;

use super::{
    Cuboid, DARK, FRAC_PI_2, GOLD, Humanoid, Limb, Mob, MobKind, PI, Parts, Rgb, WHITE, age_ticks, cube, head_x,
    motion, part,
};
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
pub(super) const SWORD_ARM: &[Cuboid] = &[
    cube([-2.0, -10.0, -2.0], [2.0, 2.0, 2.0], SKIN, 36),
    cube([-0.5, -10.0, 1.0], [0.5, -9.0, 3.0], [100, 70, 30], 10),
    cube([-1.5, -10.0, 3.0], [1.5, -9.0, 3.5], GOLD, 10),
    cube([-0.5, -10.0, 3.5], [0.5, -9.0, 11.0], GOLD, 10),
];
pub(super) const CROSSBOW_ARM: &[Cuboid] = &[
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
    // v0.18 Track 6: brows, and a mouth along the foot of the snout.
    cube([-4.0, 5.2, 4.0], [-2.0, 5.7, 4.05], [176, 110, 104], 0),
    cube([2.0, 5.2, 4.0], [4.0, 5.7, 4.05], [176, 110, 104], 0),
    cube([-1.8, 0.3, 5.0], [1.8, 0.6, 5.05], [150, 90, 84], 0),
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
pub(super) const AXE_ARM: &[Cuboid] = &[
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
            // v0.18 Track 6: eye glints.
            cube([-4.5, 22.0, 8.1], [-3.5, 23.0, 8.15], [236, 226, 214], 0),
            cube([3.5, 22.0, 8.1], [4.5, 23.0, 8.15], [236, 226, 214], 0),
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

/// v1.0 gap Track 7: `AbstractPiglinModel`'s ears: they flop with the
/// walk and an idle drift, out from Java's 30 degrees (5 for babies).
fn piglin_ears(m: &Mob, age: f32) -> (f32, f32) {
    let rest = if m.is_baby() { 5.0f32 } else { 30.0 }.to_radians();
    let freq = age * 0.1 + m.limb_phase * 0.5;
    let amp = 0.08 + m.limb_amp * 0.4;
    (rest + freq.cos() * amp, -rest - (freq * 1.2).cos() * amp)
}

/// Animated parts for the mobs this module draws.
pub(super) fn pose(m: &Mob, time: f32) -> Parts {
    let head = Quat::from_rotation_y(-m.head_yaw) * Quat::from_rotation_x(-m.head_pitch);
    let rx = Quat::from_rotation_x;
    let age = age_ticks(time);
    let (hx, hy) = (head_x(m), -m.head_yaw);
    match m.kind {
        MobKind::Piglin => {
            // `PiglinModel` over a humanoid's walk: an item held out
            // (`ArmPose.ITEM`), a melee weapon held high while fighting and
            // swung down to strike, a crossbow held or drawn, gold admired
            // in the off hand, and the celebration dance.
            let n = m.nether.as_deref();
            let weapon = n.map_or(Weapon::None, |n| n.weapon);
            let admiring = n.is_some_and(|n| n.is_admiring());
            let dancing = n.is_some_and(|n| n.dancing);
            let fighting = m.ai == Ai::Chase;
            let melee = matches!(weapon, Weapon::GoldenSword | Weapon::GoldenAxe);
            let mut h = Humanoid::walk(m, 1.0);
            if weapon != Weapon::None {
                h.right_arm.x = h.right_arm.x * 0.5 - PI / 10.0;
                h.right_arm.y = 0.0;
            }
            let attacking = fighting && melee;
            if attacking && m.swing.anim > 0.0 {
                let (weapon_x, other_x) = motion::weapon_down(m.swing.anim, age);
                h.right_arm = Limb { x: weapon_x, y: PI / 20.0, z: 0.0 };
                h.left_arm = Limb { x: other_x, y: -PI / 20.0, z: 0.0 };
            } else if weapon != Weapon::Crossbow || !fighting {
                h.attack(m.swing.anim, hx);
            }
            h.bob(age);
            let (mut right_ear, mut left_ear) = piglin_ears(m, age);
            let mut head_rot = head;
            let (mut head_at, mut body_y, mut arm_y) = ([0.0, 24.0, 0.0], 0.0, [0.0, 0.0]);
            if dancing {
                let d = age / 60.0;
                right_ear = PI / 6.0 + (d * 30.0).sin() * 10f32.to_radians();
                left_ear = -PI / 6.0 - (d * 30.0).cos() * 10f32.to_radians();
                head_at = [-(d * 10.0).sin(), 24.0 - ((d * 40.0).sin() + 0.4), 0.0];
                h.right_arm.z = (70.0f32 + (d * 40.0).cos() * 10.0).to_radians();
                h.left_arm.z = -h.right_arm.z;
                arm_y = [-((d * 40.0).sin() * 0.5 - 0.5), -((d * 40.0).sin() * 0.5 + 0.5)];
                body_y = -(d * 40.0).sin() * 0.35;
            } else if attacking && m.swing.anim == 0.0 {
                h.right_arm.x = -1.8;
            } else if weapon == Weapon::Crossbow && fighting {
                h.right_arm = Limb { x: -FRAC_PI_2 + hx + 0.1, y: -0.3 + hy, z: h.right_arm.z };
                h.left_arm = Limb { x: -1.5 + hx, y: 0.6 + hy, z: h.left_arm.z };
            } else if admiring {
                head_rot = rx(0.5);
                h.left_arm.y = 0.5;
                h.left_arm.x = -0.9;
            }
            let right = match weapon {
                Weapon::GoldenSword => SWORD_ARM,
                Weapon::GoldenAxe => AXE_ARM,
                Weapon::Crossbow => CROSSBOW_ARM,
                Weapon::None => ARM,
            };
            let left = if admiring && !dancing { GOLD_ARM } else { ARM };
            let mut parts = Parts::new();
            parts.push(part(BODY, [0.0, body_y, 0.0], h.body()));
            parts.push(part(LEG, [-2.0, 12.0, 0.0], rx(h.left_leg)));
            parts.push(part(LEG, [2.0, 12.0, 0.0], rx(h.right_leg)));
            let [lx, ly, lz] = h.arm_at([-6.0, 22.0, 0.0]);
            parts.push(part(left, [lx, ly + arm_y[1], lz], h.left_arm.quat()));
            let [rx_, ry_, rz_] = h.arm_at([6.0, 22.0, 0.0]);
            parts.push(part(right, [rx_, ry_ + arm_y[0], rz_], h.right_arm.quat()));
            parts.push(part(HEAD, head_at, head_rot));
            let ear = |side: f32| head_rot * glam::Vec3::new(5.0 * side, 6.0, 0.0);
            let e = ear(-1.0);
            parts.push(part(
                EAR,
                [head_at[0] + e.x, head_at[1] + e.y, e.z],
                head_rot * Quat::from_rotation_z(left_ear),
            ));
            let e = ear(1.0);
            parts.push(part(
                EAR,
                [head_at[0] + e.x, head_at[1] + e.y, e.z],
                head_rot * Quat::from_rotation_z(right_ear),
            ));
            parts
        }
        MobKind::PiglinBrute => {
            // `PiglinModel` with its axe: held out walking, held high while
            // fighting and swung down to strike.
            let fighting = m.ai == Ai::Chase;
            let mut h = Humanoid::walk(m, 1.0);
            h.right_arm.x = h.right_arm.x * 0.5 - PI / 10.0;
            h.right_arm.y = 0.0;
            if fighting && m.swing.anim > 0.0 {
                let (weapon_x, other_x) = motion::weapon_down(m.swing.anim, age);
                h.right_arm = Limb { x: weapon_x, y: PI / 20.0, z: 0.0 };
                h.left_arm = Limb { x: other_x, y: -PI / 20.0, z: 0.0 };
            } else {
                h.attack(m.swing.anim, hx);
            }
            h.bob(age);
            if fighting && m.swing.anim == 0.0 {
                h.right_arm.x = -1.8;
            }
            let (right_ear, left_ear) = piglin_ears(m, age);
            let mut parts = Parts::new();
            parts.push(part(BRUTE_BODY, [0.0; 3], h.body()));
            parts.push(part(BRUTE_LEG, [-2.0, 12.0, 0.0], rx(h.left_leg)));
            parts.push(part(BRUTE_LEG, [2.0, 12.0, 0.0], rx(h.right_leg)));
            parts.push(part(ARM, h.arm_at([-6.0, 22.0, 0.0]), h.left_arm.quat()));
            parts.push(part(AXE_ARM, h.arm_at([6.0, 22.0, 0.0]), h.right_arm.quat()));
            parts.push(part(HEAD, [0.0, 24.0, 0.0], head));
            for (side, flop) in [(-1.0f32, left_ear), (1.0, right_ear)] {
                let e = head * glam::Vec3::new(5.0 * side, 6.0, 0.0);
                parts.push(part(EAR, [e.x, 24.0 + e.y, e.z], head * Quat::from_rotation_z(flop)));
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
            // `HoglinModel`: legs swing `cos(pos) * 1.2 * speed` (no 0.6662);
            // the head hangs 50 degrees down and butts up to 20 degrees
            // above level and back over the 10-tick attack. It looks
            // round but never up or down.
            let ticks = (m.attack_anim / motion::TICK).round();
            let butt = 1.0 - (10.0 - 2.0 * ticks).abs() / 10.0;
            let head_x = 0.872_664_63 + (-PI / 9.0 - 0.872_664_63) * butt;
            let leg = m.limb_phase.cos() * 1.2 * m.limb_amp;
            let mut parts = Parts::new();
            parts.push(part(body, [0.0; 3], Quat::IDENTITY));
            parts.push(part(front, [-4.5, 12.0, 8.0], rx(-leg)));
            parts.push(part(front, [4.5, 12.0, 8.0], rx(leg)));
            parts.push(part(back, [-4.5, 12.0, -9.0], rx(leg)));
            parts.push(part(back, [4.5, 12.0, -9.0], rx(-leg)));
            parts.push(part(head_box, [0.0, 15.0, 12.0], Quat::from_rotation_y(-m.head_yaw) * rx(head_x)));
            if zoglin {
                parts.push(part(ZOGLIN_ROT, [0.0; 3], Quat::IDENTITY));
            }
            parts
        }
        MobKind::Strider => {
            let cold = m.nether.as_ref().is_some_and(|n| n.cold);
            let (body, leg) = if cold { (COLD_STRIDER_BODY, COLD_STRIDER_LEG) } else { (STRIDER_BODY, STRIDER_LEG) };
            // `StriderModel`: the speed it animates with is capped at 0.25;
            // the body rocks and bobs at 1.5 times the walk position and
            // the legs swing, splay and lift at half that. A cold strider
            // shivers.
            let (pos, speed) = (m.limb_phase, m.limb_amp.min(0.25));
            let half = pos * 0.75;
            let rock = 0.4 * (pos * 1.5).sin() * speed;
            let bob = 4.0 * (pos * 1.5).cos() * speed;
            let shiver = if cold { (time * 45.0).sin() * 0.04 } else { 0.0 };
            let leg_rot = |sign: f32| {
                Limb { x: sign * half.sin() * 2.0 * speed, y: 0.0, z: sign * PI / 18.0 * half.cos() * speed }.quat()
            };
            let lift = |sign: f32| sign * 4.0 * half.sin() * speed;
            let mut parts = Parts::new();
            // Unridden, the whole body looks where the strider looks.
            let ridden = m.mount.as_ref().is_some_and(|s| s.rider.is_some());
            let (bx, by) = if ridden { (0.0, 0.0) } else { (hx, hy) };
            parts.push(part(body, [0.0, bob, 0.0], Limb { x: bx, y: by, z: shiver + rock }.quat()));
            parts.push(part(leg, [-4.0, 14.0 + lift(1.0), 0.0], leg_rot(1.0)));
            parts.push(part(leg, [4.0, 14.0 + lift(-1.0), 0.0], leg_rot(-1.0)));
            parts
        }
        _ => Parts::new(),
    }
}
