//! v1.0 gap Track 7c: adult models rebuilt on Java's part trees so their
//! keyframe animations move each part as Java's do: the rabbit
//! (`AdultRabbitModel`, `RabbitAnimation`) and the bat (`BatModel`,
//! `BatAnimation`). Proportions and pivots are Java's; colours and details
//! are this game's own, as before.

use std::f32::consts::FRAC_PI_8;

use super::keyframes::{Anim, Pos, Rot, Scale, c, ch, l};
use super::rig::{Bone, ROOT, Rig, bone, face, jbox};
use super::*;

/// Ticks since an animation state started, with the partial tick.
fn since(m: &Mob, start: Option<u32>, alpha: f32) -> Option<f32> {
    start.map(|s| m.age_in_ticks(alpha) - s as f32)
}

// ---------------------------------------------------------------- rabbit

#[allow(dead_code)]
pub(super) mod rabbit {
    use super::*;

    pub const BODY: usize = 0;
    pub const TAIL: usize = 1;
    pub const HEAD: usize = 2;
    pub const LEFT_EAR: usize = 3;
    pub const RIGHT_EAR: usize = 4;
    pub const FRONTLEGS: usize = 5;
    pub const RIGHT_FRONT_LEG: usize = 6;
    pub const LEFT_FRONT_LEG: usize = 7;
    pub const BACKLEGS: usize = 8;
    pub const RIGHT_HIND_LEG: usize = 9;
    pub const LEFT_HIND_LEG: usize = 11;

    include!("keyframe_defs/rabbit.rs");

    /// The coat (`animals::colour` paints each variant) and the eyes (red
    /// on white rabbits).
    const FUR: Rgb = [158, 123, 88];
    const EAR: &[Cuboid] = &[
        jbox([-1.0, -4.2929, -0.1213], [2.0, 5.0, 1.0], FUR, 18),
        face([-0.6, 0.6], [-3.9, -0.4], -0.1213, 0.05, [220, 164, 161]),
    ];
    const HAUNCH: &[Cuboid] = &[jbox([-1.0, 0.0, -5.0], [2.0, 1.0, 6.0], FUR, 24)];
    static BONES: &[Bone] = &[
        bone(&[jbox([-4.0, -6.0, -9.0], [8.0, 6.0, 10.0], FUR, 24)], ROOT, [0.0, 23.0, 4.0], [-FRAC_PI_8, 0.0, 0.0]),
        bone(
            &[jbox([-2.0, -3.0084, -1.0125], [4.0, 4.0, 4.0], [236, 225, 202], 12)],
            0,
            [0.0, -4.9916, 0.0125],
            [0.0; 3],
        ),
        bone(
            &[
                jbox([-2.5, -3.0, -4.0], [5.0, 5.0, 5.0], FUR, 22),
                // Eyes with glints, a pink nose, a mouth and whiskers.
                face([-2.3, -1.1], [-1.6, -0.4], -4.0, 0.1, [30, 28, 26]),
                face([1.1, 2.3], [-1.6, -0.4], -4.0, 0.1, [30, 28, 26]),
                face([-1.85, -1.5], [-1.5, -1.15], -4.1, 0.05, [222, 218, 210]),
                face([1.5, 1.85], [-1.5, -1.15], -4.1, 0.05, [222, 218, 210]),
                face([-0.6, 0.6], [0.0, 0.7], -4.0, 0.1, [218, 170, 166]),
                face([-0.4, 0.4], [1.1, 1.4], -4.0, 0.05, [120, 90, 84]),
                face([-2.5, -1.2], [0.7, 0.8], -4.0, 0.08, [214, 210, 200]),
                face([1.2, 2.5], [0.7, 0.8], -4.0, 0.08, [214, 210, 200]),
            ],
            0,
            [0.0, -5.2929, -8.1213],
            [FRAC_PI_8, 0.0, 0.0],
        ),
        bone(EAR, HEAD as u8, [1.5, -3.7071, -0.8787], [0.0; 3]),
        bone(EAR, HEAD as u8, [-1.5, -3.7071, -0.8787], [0.0; 3]),
        bone(&[], 0, [0.0, -1.5349, -6.3108], [0.0; 3]),
        bone(
            &[jbox([-0.9, -1.0, -0.9], [2.0, 4.0, 2.0], FUR, 22)],
            FRONTLEGS as u8,
            [-2.0, 1.9239, 0.3827],
            [FRAC_PI_8, 0.0, 0.0],
        ),
        bone(
            &[jbox([-1.0, -1.0, -1.0], [2.0, 4.0, 2.0], FUR, 22)],
            FRONTLEGS as u8,
            [2.0, 1.9239, 0.4827],
            [FRAC_PI_8, 0.0, 0.0],
        ),
        bone(&[], ROOT, [0.0, 23.0, 4.0], [0.0; 3]),
        bone(&[], BACKLEGS as u8, [-3.0, 0.5, 0.0], [0.0; 3]),
        bone(HAUNCH, RIGHT_HIND_LEG as u8, [0.0, -0.5, 0.0], [0.0, FRAC_PI_8, 0.0]),
        bone(&[], BACKLEGS as u8, [3.0, 0.5, 0.0], [0.0; 3]),
        bone(HAUNCH, LEFT_HIND_LEG as u8, [0.0, -0.5, 0.0], [0.0, -FRAC_PI_8, 0.0]),
    ];

    /// `RabbitModel.setupAnim`: the head looks unless the idle tilt plays;
    /// `RabbitAnimation.HOP` runs from the tick the rabbit leaves the
    /// ground and `IDLE_HEAD_TILT` every 180 to 220 idle ticks (Track 7b's
    /// states), each keyframe on its own part.
    pub fn pose(m: &Mob, alpha: f32) -> Parts {
        let mut r = Rig::new(BONES);
        let tilt = since(m, m.anim.tilt_start, alpha).filter(|&t| t < IDLE_HEAD_TILT.length * 20.0);
        if tilt.is_none() {
            r.look(HEAD, m);
        }
        if let Some(t) = since(m, m.anim.hop_start, alpha) {
            HOP.apply(&mut r, t);
        }
        if let Some(t) = tilt {
            IDLE_HEAD_TILT.apply(&mut r, t);
        }
        r.parts()
    }
}

// ---------------------------------------------------------------- bat

pub(super) mod bat {
    use super::*;

    pub const BODY: usize = 0;
    pub const HEAD: usize = 1;
    pub const RIGHT_WING: usize = 4;
    pub const RIGHT_WING_TIP: usize = 5;
    pub const LEFT_WING: usize = 6;
    pub const LEFT_WING_TIP: usize = 7;
    pub const FEET: usize = 8;

    include!("keyframe_defs/bat.rs");

    const FUR: Rgb = [83, 60, 48];
    const MEMBRANE: Rgb = [79, 51, 43];
    const EAR: Rgb = [128, 87, 70];
    static BONES: &[Bone] = &[
        bone(&[jbox([-1.5, 0.0, -1.0], [3.0, 5.0, 2.0], FUR, 32)], ROOT, [0.0, 17.0, 0.0], [0.0; 3]),
        bone(
            &[
                jbox([-2.0, -3.0, -1.0], [4.0, 3.0, 2.0], [101, 75, 57], 28),
                // Eyes with glints, a snout and two small fangs.
                face([-1.6, -0.7], [-2.2, -1.3], -1.0, 0.1, [24, 20, 17]),
                face([0.7, 1.6], [-2.2, -1.3], -1.0, 0.1, [24, 20, 17]),
                face([-1.4, -1.1], [-2.2, -1.9], -1.1, 0.05, [214, 208, 196]),
                face([1.1, 1.4], [-2.2, -1.9], -1.1, 0.05, [214, 208, 196]),
                face([-0.6, 0.6], [-1.2, -0.4], -1.0, 0.5, [176, 115, 102]),
                face([-0.55, -0.25], [-0.4, 0.0], -1.0, 0.05, [236, 232, 222]),
                face([0.25, 0.55], [-0.4, 0.0], -1.0, 0.05, [236, 232, 222]),
            ],
            ROOT,
            [0.0, 17.0, 0.0],
            [0.0; 3],
        ),
        bone(
            &[
                jbox([-2.5, -4.0, -0.15], [3.0, 5.0, 0.3], EAR, 20),
                face([-2.0, -0.2], [-3.4, 0.4], -0.15, 0.05, [150, 96, 86]),
            ],
            HEAD as u8,
            [-1.5, -2.0, 0.0],
            [0.0; 3],
        ),
        bone(
            &[
                jbox([-0.1, -3.0, -0.15], [3.0, 5.0, 0.3], EAR, 20),
                face([0.3, 2.4], [-2.4, 1.4], -0.15, 0.05, [150, 96, 86]),
            ],
            HEAD as u8,
            [1.1, -3.0, 0.0],
            [0.0; 3],
        ),
        bone(&[jbox([-2.0, -2.0, -0.15], [2.0, 7.0, 0.3], MEMBRANE, 25)], BODY as u8, [-1.5, 0.0, 0.0], [0.0; 3]),
        bone(
            &[
                jbox([-6.0, -2.0, -0.15], [6.0, 8.0, 0.3], [95, 61, 51], 30),
                jbox([-6.0, -2.1, -0.2], [6.0, 0.6, 0.4], [134, 91, 69], 18),
            ],
            RIGHT_WING as u8,
            [-2.0, 0.0, 0.0],
            [0.0; 3],
        ),
        bone(&[jbox([0.0, -2.0, -0.15], [2.0, 7.0, 0.3], MEMBRANE, 25)], BODY as u8, [1.5, 0.0, 0.0], [0.0; 3]),
        bone(
            &[
                jbox([0.0, -2.0, -0.15], [6.0, 8.0, 0.3], [95, 61, 51], 30),
                jbox([0.0, -2.1, -0.2], [6.0, 0.6, 0.4], [134, 91, 69], 18),
            ],
            LEFT_WING as u8,
            [2.0, 0.0, 0.0],
            [0.0; 3],
        ),
        bone(&[jbox([-1.5, 0.0, -0.15], [3.0, 2.0, 0.3], [55, 43, 35], 18)], BODY as u8, [0.0, 5.0, 0.0], [0.0; 3]),
    ];

    /// `BatModel.setupAnim`: resting, the head turns with the look; the
    /// flying or resting keyframes (`BatAnimation`) run from the tick
    /// `Bat.setupAnimationStates` started them (Track 7b's state), each on
    /// its own part: resting, the body and head hang upside down.
    pub fn pose(m: &Mob, alpha: f32) -> Parts {
        let mut r = Rig::new(BONES);
        let resting = m.bat.as_ref().is_some_and(|s| s.resting);
        let t = since(m, m.anim.bat_start, alpha).unwrap_or(0.0);
        if resting {
            r.y_rot(HEAD, m.head_yaw);
            BAT_RESTING.apply(&mut r, t);
        } else {
            BAT_FLYING.apply(&mut r, t);
        }
        r.parts()
    }
}

#[cfg(test)]
mod tests {
    use super::super::pose;
    use super::*;
    use glam::DVec3;

    fn walking(kind: MobKind) -> Mob {
        let mut m = Mob::new(kind, DVec3::ZERO, 0.0);
        (m.limb_phase, m.limb_amp) = (0.0, 0.5);
        m
    }

    /// The x turn of the part whose pivot is nearest `at`.
    fn turn_at(p: &Parts, at: Vec3) -> f32 {
        let part = p.iter().min_by(|a, b| a.pivot.distance(at).total_cmp(&b.pivot.distance(at))).unwrap();
        part.rot.to_euler(glam::EulerRot::XYZ).0
    }

    /// v1.0 gap Track 7c: `QuadrupedModel`'s right hind leg (Java's right,
    /// this model's -X) swings forward on the walk's cosine, its left front
    /// with it, the other two against them.
    #[test]
    fn quadrupeds_walk_on_java_s_sides() {
        for (kind, front, hind) in [
            (MobKind::Pig, Vec3::new(-3.0, 6.0, 5.0), Vec3::new(-3.0, 6.0, -5.0)),
            (MobKind::Cow, Vec3::new(-4.0, 12.0, 6.0), Vec3::new(-4.0, 12.0, -6.0)),
            (MobKind::Sheep, Vec3::new(-3.0, 12.0, 5.0), Vec3::new(-3.0, 12.0, -5.0)),
        ] {
            let p = pose(&walking(kind), 0.0, 1.0);
            // Java's positive `xRot` swings a hanging leg back; at the
            // cosine's peak the right hind leg is back and the right front
            // forward.
            assert!(turn_at(&p, hind) > 0.1 && turn_at(&p, front) < -0.1, "{kind:?}");
            let left_hind = Vec3::new(-hind.x, hind.y, hind.z);
            assert!(turn_at(&p, left_hind) < -0.1, "{kind:?}");
        }
    }

    /// `AbstractEquineModel`: grazing lowers the head 7 pixels and turns it
    /// down toward 125 degrees; rearing lifts the front legs 12 pixels and
    /// carries them 4 back, the head 8 up and 8 back.
    #[test]
    fn a_horse_grazes_and_rears_as_java_places_its_parts() {
        let mut m = Mob::new(MobKind::Horse, DVec3::ZERO, 0.0);
        let head = |m: &Mob| pose(m, 0.0, 1.0)[3];
        let rest = head(&m);
        let s = m.mount.as_mut().unwrap();
        (s.eat_anim, s.eat_anim_o) = (1.0, 1.0);
        let grazing = head(&m);
        assert!((rest.pivot.y - grazing.pivot.y - 7.0).abs() < 1e-4);
        let down = (grazing.rot * Vec3::Z).y - (rest.rot * Vec3::Z).y;
        assert!(down < -0.5, "the muzzle points down: {down}");
        let s = m.mount.as_mut().unwrap();
        (s.eat_anim, s.eat_anim_o, s.stand_anim, s.stand_anim_o) = (0.0, 0.0, 1.0, 1.0);
        let p = pose(&m, 0.0, 1.0);
        assert!((p[1].pivot - Vec3::new(-3.5, 24.0, 3.0)).length() < 1e-4, "{}", p[1].pivot);
        assert!((p[3].pivot - rest.pivot - Vec3::new(0.0, 8.0, -8.0)).length() < 1e-4);
    }

    /// `GuardianModel`'s eye slides toward the side its target is on, and a
    /// pixel down when the target is below.
    #[test]
    fn a_guardian_s_eye_follows_its_target() {
        let m = Mob::new(MobKind::Guardian, DVec3::new(0.0, 64.0, 0.0), 0.0);
        let eye = |camera: DVec3| {
            let mut p = pose(&m, 0.0, 1.0);
            super::super::aquatic::track_eye(&m, camera, &mut p);
            p.iter().find(|p| std::ptr::eq(p.boxes, super::super::aquatic::GUARD_EYE)).unwrap().pivot
        };
        // Facing +X (yaw 0), its left is -Z in the world and +X in the model.
        let left = eye(DVec3::new(0.0, 70.0, -5.0));
        assert!((left - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-4, "{left}");
        let right_below = eye(DVec3::new(0.0, 60.0, 5.0));
        assert!((right_below - Vec3::new(-2.0, -1.0, 0.0)).length() < 1e-4, "{right_below}");
        let ahead = eye(DVec3::new(5.0, 70.0, 0.0));
        assert!(ahead.x.abs() < 1e-3);
    }

    /// `RabbitAnimation.HOP` and `BatAnimation` move Java's parts: mid-hop
    /// the rabbit's hind legs swing, and a resting bat hangs upside down.
    #[test]
    fn rabbits_and_bats_play_their_keyframes_per_part() {
        let mut m = Mob::new(MobKind::Rabbit, DVec3::ZERO, 0.0);
        let still = rabbit::pose(&m, 0.0);
        m.anim.hop_start = Some(0);
        m.tick_count = 6;
        let hop = rabbit::pose(&m, 0.0);
        assert!(still.iter().zip(hop.iter()).any(|(a, b)| a.rot.angle_between(b.rot) > 0.5));
        let mut bat = Mob::new(MobKind::Bat, DVec3::ZERO, 0.0);
        bat.bat.as_mut().unwrap().resting = true;
        bat.anim.bat_start = Some(0);
        let p = bat::pose(&bat, 0.0);
        // The body (Java's y 17 to 22) turned over about its top.
        assert!((p[0].rot * Vec3::Y).y < -0.99);
    }

    /// v1.0 gap Track 7c: a crossbow piglin's pulling arm draws back over
    /// the 25 ticks it loads (`animateCrossbowCharge`).
    #[test]
    fn a_piglin_draws_its_crossbow_over_its_charge() {
        let mut m = Mob::new(MobKind::Piglin, DVec3::ZERO, 0.0);
        m.nether.as_mut().unwrap().weapon = crate::entity::nether::Weapon::Crossbow;
        m.ai = super::super::Ai::Chase;
        let arm = |m: &Mob| pose(m, 0.0, 1.0)[3].rot;
        m.set_crossbow_draw(Some(0.0));
        let start = arm(&m);
        assert_eq!(m.crossbow_draw(), Some(0.0));
        m.set_crossbow_draw(Some(24.0));
        let drawn = arm(&m);
        assert!(start.angle_between(drawn) > 0.4, "{}", start.angle_between(drawn));
        m.set_crossbow_draw(None);
        assert_eq!(m.crossbow_draw(), None);
    }
}
