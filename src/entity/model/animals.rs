//! Original box models, in sixteenths of a block. Shared animation state is
//! read directly, keeping the entity pass allocation-free per mob.
use super::*;
pub(super) const FUR: [u8; 3] = [186, 185, 178];
pub(super) const COLLAR: [u8; 3] = [255, 0, 254];
/// A wolf's mane and muzzle, recoloured from its coat (see [`colour`]).
pub(super) const MANE: [u8; 3] = [171, 171, 167];
pub(super) const MUZZLE: [u8; 3] = [211, 209, 199];
// v0.18 Track 6: fur texture, a dark nose, eye glints and inner ears.
const WOLF_BODY: &[Cuboid] = &[
    cube([-3.0, 6.0, -6.0], [3.0, 12.0, 5.0], FUR, 24).with(Pattern::Fur),
    cube([-3.5, 6.0, 1.0], [3.5, 13.0, 6.0], [171, 171, 167], 26).with(Pattern::Fur),
];
const WOLF_HEAD: &[Cuboid] = &[
    cube([-3.0, -2.0, -1.0], [3.0, 4.0, 5.0], FUR, 18).with(Pattern::Fur),
    cube([-1.5, -2.0, 5.0], [1.5, 1.0, 8.0], [211, 209, 199], 16).with(Pattern::Fur),
    cube([-1.0, -1.0, 8.0], [1.0, 0.5, 8.1], [35, 33, 32], 0),
    cube([-1.0, -2.0, 8.0], [1.0, -1.6, 8.05], [90, 80, 76], 0),
    cube([-3.0, 4.0, 0.0], [-1.0, 7.0, 2.0], FUR, 20).with(Pattern::Fur),
    cube([1.0, 4.0, 0.0], [3.0, 7.0, 2.0], FUR, 20).with(Pattern::Fur),
    cube([-2.5, 4.5, 2.0], [-1.5, 6.5, 2.05], [96, 78, 74], 0),
    cube([1.5, 4.5, 2.0], [2.5, 6.5, 2.05], [96, 78, 74], 0),
    cube([-2.6, 1.0, 5.0], [-1.5, 2.2, 5.1], [30, 28, 25], 0),
    cube([1.5, 1.0, 5.0], [2.6, 2.2, 5.1], [30, 28, 25], 0),
    cube([-2.2, 1.7, 5.1], [-1.8, 2.1, 5.15], [220, 220, 210], 0),
    cube([1.8, 1.7, 5.1], [2.2, 2.1, 5.15], [220, 220, 210], 0),
];
const WOLF_LEG: &[Cuboid] = &[cube([-1.0, -6.0, -1.0], [1.0, 0.0, 1.0], FUR, 18).with(Pattern::Fur)];
const WOLF_TAIL: &[Cuboid] = &[cube([-1.0, -1.0, -7.0], [1.0, 1.0, 0.0], FUR, 20).with(Pattern::Fur)];
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
    // v0.18 Track 6: white cheeks, dark inner ears and eye glints.
    cube([-3.6, -2.0, 1.5], [-3.5, 0.0, 4.5], [243, 236, 222], 0),
    cube([3.5, -2.0, 1.5], [3.6, 0.0, 4.5], [243, 236, 222], 0),
    cube([-2.6, 4.5, 2.0], [-1.4, 6.5, 2.05], [40, 28, 22], 0),
    cube([1.4, 4.5, 2.0], [2.6, 6.5, 2.05], [40, 28, 22], 0),
    cube([-2.0, 1.6, 5.1], [-1.7, 1.9, 5.15], [226, 214, 196], 0),
    cube([2.0, 1.6, 5.1], [2.3, 1.9, 5.15], [226, 214, 196], 0),
];
const FOX_LEG: &[Cuboid] = &[cube([-0.8, -4.0, -0.8], [0.8, 0.0, 0.8], [65, 44, 34], 16)];
const FOX_TAIL: &[Cuboid] = &[
    cube([-2.0, -2.0, -7.0], [2.0, 2.0, 0.0], [201, 108, 47], 24),
    cube([-2.0, -2.0, -10.0], [2.0, 2.0, -7.0], [242, 234, 218], 16),
];
/// The rabbit's body and tail; v1.0 gap Track 7b split its head off onto
/// its own pivot (`RABBIT_HEAD_AT`), the same boxes in the same places.
const RABBIT: &[Cuboid] = &[
    cube([-2.0, 2.0, -3.0], [2.0, 6.0, 3.0], [158, 123, 88], 24),
    cube([-1.0, 3.0, -4.5], [1.0, 5.0, -3.0], [236, 225, 202], 12),
];
const RABBIT_HEAD_AT: [f32; 3] = [0.0, 5.0, 2.0];
const RABBIT_HEAD: &[Cuboid] = &[
    cube([-2.0, -1.0, 0.0], [2.0, 3.0, 4.0], [173, 143, 106], 22),
    cube([-2.0, 3.0, 0.0], [-0.6, 10.0, 1.2], [169, 137, 103], 18),
    cube([0.6, 3.0, 0.0], [2.0, 10.0, 1.2], [169, 137, 103], 18),
    cube([-1.8, 3.5, 1.2], [-0.8, 9.0, 1.3], [220, 164, 161], 0),
    cube([0.8, 3.5, 1.2], [1.8, 9.0, 1.3], [220, 164, 161], 0),
    cube([-2.1, 1.0, 2.0], [-2.0, 2.0, 3.0], [30, 28, 26], 0),
    cube([2.0, 1.0, 2.0], [2.1, 2.0, 3.0], [30, 28, 26], 0),
    cube([-1.0, 0.0, 4.0], [1.0, 1.0, 4.1], [218, 170, 166], 0),
    // v0.18 Track 6: eye glints, a mouth and whiskers.
    cube([-2.15, 1.6, 2.6], [-2.1, 1.9, 2.9], [222, 218, 210], 0),
    cube([2.1, 1.6, 2.6], [2.15, 1.9, 2.9], [222, 218, 210], 0),
    cube([-0.5, -0.5, 4.0], [0.5, -0.2, 4.05], [120, 90, 84], 0),
    cube([-2.6, 0.2, 3.4], [-2.0, 0.3, 3.6], [214, 210, 200], 0),
    cube([2.0, 0.2, 3.4], [2.6, 0.3, 3.6], [214, 210, 200], 0),
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
    // v0.18 Track 6: amber eyes with a goat's bar pupils, nostrils, a mouth.
    cube([-2.65, 1.0, 4.0], [-2.6, 2.0, 5.5], [196, 160, 64], 0),
    cube([2.6, 1.0, 4.0], [2.65, 2.0, 5.5], [196, 160, 64], 0),
    cube([-2.7, 1.35, 4.2], [-2.65, 1.65, 5.3], [28, 26, 22], 0),
    cube([2.65, 1.35, 4.2], [2.7, 1.65, 5.3], [28, 26, 22], 0),
    cube([-1.5, -2.0, 7.0], [-0.6, -1.3, 7.05], [96, 86, 74], 0),
    cube([0.6, -2.0, 7.0], [1.5, -1.3, 7.05], [96, 86, 74], 0),
    cube([-1.5, -2.9, 7.0], [1.5, -2.6, 7.05], [140, 130, 114], 0),
];
const GOAT_HORN: &[Cuboid] = &[cube([-0.6, 0.0, -0.6], [0.6, 6.0, 0.6], [110, 106, 95], 15)];
const GOAT_LEG: &[Cuboid] = &[
    cube([-1.0, -7.0, -1.0], [1.0, 0.0, 1.0], [224, 219, 201], 20),
    cube([-1.0, -8.0, -1.0], [1.0, -7.0, 1.0], [81, 74, 65], 12),
];
/// The parrot's body; v1.0 gap Track 7b split its head, tail and legs off
/// onto their own pivots, the same boxes in the same places.
const PARROT: &[Cuboid] = &[cube([-1.5, 3.0, -2.0], [1.5, 10.0, 2.0], [221, 48, 43], 20)];
const PARROT_HEAD_AT: [f32; 3] = [0.0, 9.0, 1.0];
const PARROT_HEAD: &[Cuboid] = &[
    cube([-2.0, 0.0, -2.0], [2.0, 5.0, 2.0], [221, 48, 43], 16),
    cube([-1.0, 0.0, 2.0], [1.0, 2.0, 4.0], [82, 77, 67], 15),
    cube([-2.1, 2.0, 0.0], [-2.0, 3.0, 1.0], [27, 26, 24], 0),
    cube([2.0, 2.0, 0.0], [2.1, 3.0, 1.0], [27, 26, 24], 0),
    cube([-0.5, 5.0, -1.0], [0.5, 8.0, 0.0], [242, 211, 45], 0),
    // v0.18 Track 6: pale eye rings and a darker lower beak.
    cube([-2.05, 1.7, -0.3], [-2.0, 3.3, 1.3], [236, 232, 222], 0),
    cube([2.0, 1.7, -0.3], [2.05, 3.3, 1.3], [236, 232, 222], 0),
    cube([-1.0, 0.0, 2.0], [1.0, 0.6, 4.05], [44, 42, 38], 0),
];
const PARROT_TAIL_AT: [f32; 3] = [0.0, 3.0, -2.0];
const PARROT_TAIL: &[Cuboid] = &[cube([-1.0, -1.0, -4.0], [1.0, 1.0, 0.0], [61, 82, 185], 18)];
const PARROT_LEGS_AT: [f32; 3] = [0.0, 3.0, -0.5];
const PARROT_LEGS: &[Cuboid] = &[cube([-1.0, -3.0, -0.5], [1.0, 0.0, 0.5], [79, 76, 69], 15)];
const PARROT_WING: &[Cuboid] = &[cube([0.0, -6.0, -1.0], [1.0, 0.0, 2.0], [61, 82, 185], 18)];
pub(super) fn pose(m: &Mob, time: f32, alpha: f32) -> Parts {
    let a = m.animal.as_deref();
    // v1.0 gap Track 7: `WolfModel`, `AdultFoxModel` and `GoatModel` (a
    // `QuadrupedModel`) trot, `cos(pos * 0.6662) * 1.4 * speed` with each
    // diagonal pair together. (The rabbit hops; its feet keep this swing.)
    let swing = walk_cos(m) * motion::LEG_SWING;
    // Left front, right front, left hind, right hind.
    let trot = [1.0, -1.0, -1.0, 1.0];
    let head = Quat::from_rotation_y(-m.head_yaw) * Quat::from_rotation_x(-m.head_pitch);
    let rx = Quat::from_rotation_x;
    match m.kind {
        MobKind::Wolf | MobKind::Fox => {
            let wolf = m.kind == MobKind::Wolf;
            let sitting = a.is_some_and(|a| a.sit_pose);
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
                    rx(if sitting || sleeping { 1.2 } else { swing * trot[i] }),
                ));
            }
            let tail_pitch = if wolf { 1.0 - m.health / m.max_health() * 1.2 } else { -0.25 };
            // A wolf wags its tail with the walk unless it is angry.
            let wag = if wolf && m.ai != Ai::Chase { swing } else { 0.0 };
            p.push(part(
                if wolf { WOLF_TAIL } else { FOX_TAIL },
                [0.0, if wolf { 10.0 } else { 7.0 } + y, -6.0],
                Quat::from_rotation_y(wag) * rx(tail_pitch),
            ));
            if wolf {
                for armor in super::passive::wolf_armor(m, [0.0, y, 0.0], body_rot).iter().copied() {
                    p.push(armor);
                }
            }
            if wolf && a.is_some_and(|a| a.owner.is_some()) {
                p.push(part(WOLF_COLLAR, [0.0; 3], Quat::IDENTITY));
            }
            p
        }
        MobKind::Rabbit => rabbit(m, alpha, head, swing),
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
                p.push(part(GOAT_LEG, [x, 8.0, z], rx(swing * trot[i])));
            }
            let horns = if m.baby { 0 } else { a.map_or(2, |a| a.horns) };
            for i in 0..horns {
                p.push(part(GOAT_HORN, [if i == 0 { -1.5 } else { 1.5 }, 19.0, 5.0], rx(-0.35)));
            }
            p
        }
        MobKind::Parrot => parrot(m, time, alpha, head),
        _ => Parts::new(),
    }
}
/// v1.0 gap Track 7b: `ParrotModel.setupAnim` in its pose: dancing
/// (`PARTY`: everything but the legs circling a pixel round each tick of
/// `ageInTicks`, the head rocking), sitting, flying (legs tucked) or
/// standing. Standing and flying, the body bobs `0.3 * flapAngle` and the
/// wings open by `flapAngle` (`Parrot.calculateFlapping`'s state) from 5
/// degrees, and the tail swings with the walk.
fn parrot(m: &Mob, time: f32, alpha: f32, head: Quat) -> Parts {
    let a = m.animal.as_deref();
    let fa = super::flap_angle(m, alpha);
    let age = age_ticks(time);
    let party = a.is_some_and(|a| a.dancing);
    let sitting = !party && a.is_some_and(|a| a.sit_pose);
    let flying = !party && !sitting && !m.on_ground;
    let (rx, rz) = (Quat::from_rotation_x, Quat::from_rotation_z);
    // Java's `y` runs down: its `+= bob` is a drop here.
    let (mut shift, mut head_rot, mut tail, mut legs, mut wing, mut legs_y) = (Vec3::ZERO, head, 0.0, 0.0, 0.0873, 0.0);
    if party {
        shift = Vec3::new(age.cos(), -age.sin(), 0.0);
        head_rot = rz(-age.sin() * 0.4);
        wing += fa;
    } else if sitting {
        shift.y = -1.0;
        legs_y = -1.0;
        tail = PI / 6.0;
        legs = 1.0;
    } else {
        shift.y = -fa * 0.3;
        legs_y = shift.y;
        tail = walk_cos(m) * 0.3;
        wing += fa;
        if flying {
            legs = PI * 2.0 / 9.0;
        }
    }
    let at = |p: [f32; 3]| (Vec3::from_array(p) + shift).to_array();
    parts![
        part(PARROT, at([0.0; 3]), Quat::IDENTITY),
        part(PARROT_HEAD, at(PARROT_HEAD_AT), head_rot),
        part(PARROT_TAIL, at(PARROT_TAIL_AT), rx(tail)),
        part(PARROT_LEGS, [PARROT_LEGS_AT[0], PARROT_LEGS_AT[1] + legs_y, PARROT_LEGS_AT[2]], rx(legs)),
        // The right wing on -X, each opening outward (`zRot` -+ for Java's
        // left and right, turned to this model's sides).
        part(PARROT_WING, at([-1.5, 10.0, 0.0]), rz(-wing)),
        part(PARROT_WING, at([1.5, 10.0, 0.0]), rz(wing))
    ]
}

/// `RabbitAnimation.HOP`'s and `IDLE_HEAD_TILT`'s lengths in seconds.
const RABBIT_HOP: f32 = 0.75;
const RABBIT_TILT: f32 = 4.0;

/// v1.0 gap Track 7b: `RabbitModel`: the hop plays on Java's clock, from
/// the tick `Rabbit.setupAnimationStates` starts it (as the rabbit leaves
/// the ground) round its 0.75 s loop, until it lands; now and then, at
/// rest, the head tilts for 4 s (`IDLE_HEAD_TILT`), not following the look
/// meanwhile. This game's own drawn motion: the body pitches forward and
/// back over the middle of the hop, the head holding level, the hind feet
/// kicking back; the tilt rocks the head to one side and back.
fn rabbit(m: &Mob, alpha: f32, head: Quat, swing: f32) -> Parts {
    let (rx, rz) = (Quat::from_rotation_x, Quat::from_rotation_z);
    let bump = |t: f32, from: f32, to: f32| {
        if t <= from || t >= to { 0.0 } else { ((t - from) / (to - from) * PI).sin() }
    };
    let (pitch, kick) = m
        .loop_time(m.anim.hop_start, RABBIT_HOP, alpha)
        .map_or((0.0, 0.0), |t| (0.55 * bump(t, 0.17, 1.0), 1.6 * bump(t, 0.17, 0.89)));
    let look = match m.loop_time(m.anim.tilt_start, RABBIT_TILT, alpha) {
        Some(t) if m.anim.tilt_start.is_some_and(|s| m.age_in_ticks(alpha) - (s as f32) < RABBIT_TILT * 20.0) => {
            rz(0.4 * (t * TAU).sin() * bump(t, 0.0, 1.0))
        }
        Some(_) => Quat::IDENTITY,
        None => head,
    };
    let body = rx(pitch);
    let head_at = (body * Vec3::from_array(RABBIT_HEAD_AT)).to_array();
    parts![
        part(RABBIT, [0.0; 3], body),
        part(RABBIT_HEAD, head_at, body * rx(-pitch) * look),
        part(RABBIT_FOOT, [-2.0, 1.0, -2.0], rx(swing + kick)),
        part(RABBIT_FOOT, [2.0, 1.0, -2.0], rx(swing + kick))
    ]
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
    // v0.18 Track 6: a wolf's mane and muzzle follow its coat.
    if m.kind == MobKind::Wolf && matches!(c.color, [171, 171, 167] | [211, 209, 199]) {
        let coat: [u8; 3] = [
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
        let k = if c.color == [171, 171, 167] { 0.88 } else { 1.14 };
        let k = if a.variant % 9 == 4 { k + 0.3 } else { k };
        c.color = coat.map(|v| (f32::from(v) * k).min(255.0) as u8);
    }
    // White rabbits have red eyes.
    if m.kind == MobKind::Rabbit && a.variant % 6 == 1 && c.color == [30, 28, 26] {
        c.color = [176, 40, 44];
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
