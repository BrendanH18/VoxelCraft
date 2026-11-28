//! v1.0 gap Track 7c: Java 26.3's own baby models for the kinds Track 7b
//! left drawing their adults halved: every `Baby*Model` (and the snifflet's
//! `SniffletModel` and the baby nautilus's `NautilusModel.createBabyBodyLayer`)
//! that `LayerDefinitions` registers, drawn at full scale by
//! `AgeableMobRenderer`. Each is a [`Rig`]: Java's part tree with its pivots,
//! rotations and box proportions, and this game's own colours, patterns and
//! details (eyes with glints, noses, markings) in the adults' palettes. They
//! animate with their Java models' `setupAnim` (the adult class's, which the
//! baby extends, with the baby's overrides) and, where Java's babies run
//! keyframe animations of their own, those keyframes
//! ([`super::keyframe_defs`]). No Mojang geometry file, texture or UV is used.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_3, FRAC_PI_4, FRAC_PI_6, FRAC_PI_8, PI};

use super::keyframes::{Anim, Pos, Rot, Scale, c, ch, l};
use super::rig::{Bone, MODEL_ROOT, ROOT, Rig, bone, face, jbox, jboxg};
use super::*;

/// `LivingEntityRenderState.ageScale` for a baby (`getAgeScale`).
const AGE_SCALE: f32 = 0.5;

/// Ticks since an animation state started, with the partial tick.
fn since(m: &Mob, start: Option<u32>, alpha: f32) -> Option<f32> {
    start.map(|s| m.age_in_ticks(alpha) - s as f32)
}

/// `Mth.rotLerp` for radians.
fn rot_lerp(a: f32, from: f32, to: f32) -> f32 {
    from + a * ((to - from + PI).rem_euclid(TAU) - PI)
}

const EYE: Rgb = [28, 26, 24];
const GLINT: Rgb = [228, 226, 218];

// ---------------------------------------------------------------- hoglin

pub(super) mod hoglin {
    use super::*;

    pub const HEAD: usize = 1;
    pub const RIGHT_EAR: usize = 2;
    pub const LEFT_EAR: usize = 3;
    pub const RIGHT_HIND_LEG: usize = 4;
    pub const LEFT_HIND_LEG: usize = 5;
    pub const RIGHT_FRONT_LEG: usize = 6;
    pub const LEFT_FRONT_LEG: usize = 7;

    const TUSK: Rgb = [240, 232, 206];
    const HOOF: Rgb = [70, 50, 40];

    /// `BabyHoglinModel` in one palette: fur, crest, snout and eyes.
    macro_rules! hoglin {
        ($bones:ident, $fur:expr, $mane:expr, $snout:expr, $eye:expr) => {
            pub static $bones: &[Bone] = &{
                const FUR: Rgb = $fur;
                const BODY_BOXES: &[Cuboid] = &[
                    jboxg([-4.0, -14.0, -7.0], [8.0, 8.0, 14.0], 0.02, FUR, 34).with(Pattern::Bristle),
                    // Java's crest is a plane of bristles; a thin box here.
                    jbox([-0.5, -18.0, -8.0], [1.0, 6.0, 11.0], $mane, 40).with(Pattern::Bristle),
                ];
                const HEAD_BOXES: &[Cuboid] = &[
                    jbox([-5.0, -2.2605, -10.547], [10.0, 4.0, 12.0], FUR, 34).with(Pattern::Bristle),
                    jbox([-7.0, -4.0981, -8.4879], [2.0, 5.0, 2.0], TUSK, 6),
                    jbox([5.0, -4.0981, -8.4879], [2.0, 5.0, 2.0], TUSK, 6),
                    face([-3.0, 3.0], [-1.6, 1.4], -10.547, 0.3, $snout),
                    face([-2.2, -0.9], [-0.9, 0.3], -10.847, 0.05, DARK),
                    face([0.9, 2.2], [-0.9, 0.3], -10.847, 0.05, DARK),
                    jbox([-5.1, -1.6, -8.4], [0.1, 1.2, 1.6], $eye, 0),
                    jbox([5.0, -1.6, -8.4], [0.1, 1.2, 1.6], $eye, 0),
                    jbox([-5.15, -1.5, -8.3], [0.05, 0.4, 0.4], GLINT, 0),
                    jbox([5.1, -1.5, -8.3], [0.05, 0.4, 0.4], GLINT, 0),
                ];
                const RIGHT_EAR_BOXES: &[Cuboid] = &[jbox([-5.1, -0.5, -2.0], [6.0, 1.0, 4.0], FUR, 34)];
                const LEFT_EAR_BOXES: &[Cuboid] = &[jbox([-0.9, -0.5, -2.0], [6.0, 1.0, 4.0], FUR, 34)];
                const LEG: &[Cuboid] = &[
                    jbox([-1.5, 0.0, -1.5], [3.0, 6.0, 3.0], FUR, 34).with(Pattern::Bristle),
                    jbox([-1.55, 4.8, -1.55], [3.1, 1.2, 3.1], HOOF, 12),
                ];
                [
                    bone(BODY_BOXES, ROOT, [0.0, 24.0, 0.0], [0.0; 3]),
                    bone(HEAD_BOXES, ROOT, [0.0, 13.0, -7.0], [0.8727, 0.0, 0.0]),
                    bone(RIGHT_EAR_BOXES, HEAD as u8, [-5.0, -1.0, -1.5], [0.0, 0.0, -0.8727]),
                    bone(LEFT_EAR_BOXES, HEAD as u8, [5.0, -1.0, -1.5], [0.0, 0.0, 0.8727]),
                    bone(LEG, ROOT, [-2.5, 18.0, 4.5], [0.0; 3]),
                    bone(LEG, ROOT, [2.5, 18.0, 4.5], [0.0; 3]),
                    bone(LEG, ROOT, [-2.5, 18.0, -4.5], [0.0; 3]),
                    bone(LEG, ROOT, [2.5, 18.0, -4.5], [0.0; 3]),
                ]
            };
        };
    }
    hoglin!(HOGLIN, [188, 122, 98], [92, 56, 36], [214, 150, 132], DARK);
    hoglin!(ZOGLIN, [224, 176, 170], [236, 228, 216], [196, 108, 112], [200, 36, 40]);

    /// `HoglinModel.setupAnim` with `BabyHoglinModel.animateHeadbutt`: the
    /// ears flop with the walk, the head looks round and butts up over the
    /// 10-tick attack (rising 2.5 pixels), the legs swing `cos(pos) * 1.2`.
    pub fn pose(m: &Mob) -> Parts {
        let mut r = Rig::new(if m.kind == MobKind::Zoglin { ZOGLIN } else { HOGLIN });
        let (pos, speed) = (m.limb_phase, m.limb_amp);
        r.z_rot(RIGHT_EAR, -PI * 2.0 / 9.0 - speed * pos.sin());
        r.z_rot(LEFT_EAR, PI * 2.0 / 9.0 + speed * pos.sin());
        r.y_rot(HEAD, m.head_yaw);
        let ticks = (m.attack_anim / motion::TICK).round();
        let butt = 1.0 - (10.0 - 2.0 * ticks).abs() / 10.0;
        r.x_rot(HEAD, 0.872_664_63 + (-PI / 9.0 - 0.872_664_63) * butt);
        r.at[HEAD][1] += butt * 2.5;
        let leg = pos.cos() * 1.2 * speed;
        r.x_rot(RIGHT_FRONT_LEG, leg);
        r.x_rot(LEFT_FRONT_LEG, -leg);
        r.x_rot(RIGHT_HIND_LEG, -leg);
        r.x_rot(LEFT_HIND_LEG, leg);
        r.parts()
    }
}

// ---------------------------------------------------------------- goat

pub(super) mod goat {
    use super::*;

    const LEFT_HIND_LEG: usize = 0;
    const RIGHT_HIND_LEG: usize = 1;
    const RIGHT_FRONT_LEG: usize = 2;
    const LEFT_FRONT_LEG: usize = 3;
    const HEAD: usize = 5;
    const RIGHT_HORN: usize = 6;
    const LEFT_HORN: usize = 7;

    const COAT: Rgb = [224, 219, 201];
    const FACE: Rgb = [232, 227, 211];
    const HORN: Rgb = [110, 106, 95];
    const HOOF: Rgb = [81, 74, 65];
    const AMBER: Rgb = [196, 160, 64];
    const LEG: &[Cuboid] = &[
        jbox([-1.0, -0.5, -1.0], [2.0, 5.0, 2.0], COAT, 20).with(Pattern::Wool),
        jbox([-1.05, 3.7, -1.05], [2.1, 0.8, 2.1], HOOF, 12),
    ];
    static BONES: &[Bone] = &[
        bone(LEG, ROOT, [1.5, 19.5, 3.0], [0.0; 3]),
        bone(LEG, ROOT, [-1.5, 19.5, 3.0], [0.0; 3]),
        bone(LEG, ROOT, [-1.5, 19.5, -2.0], [0.0; 3]),
        bone(LEG, ROOT, [1.5, 19.5, -2.0], [0.0; 3]),
        bone(
            &[jbox([-3.0, -2.3, -4.5], [6.0, 5.0, 9.0], COAT, 28).with(Pattern::Wool)],
            ROOT,
            [0.0, 17.8, 0.0],
            [0.0; 3],
        ),
        // `head` and `HeadMain` are the same box; one is drawn.
        bone(
            &[
                jbox([-2.0, -3.8126, -5.1548], [4.0, 4.0, 6.0], FACE, 18).with(Pattern::Wool),
                // Amber eyes with a goat's bar pupils, nostrils and a tuft
                // under the chin.
                jbox([-2.08, -2.9, -4.2], [0.08, 1.0, 1.4], AMBER, 0),
                jbox([2.0, -2.9, -4.2], [0.08, 1.0, 1.4], AMBER, 0),
                jbox([-2.12, -2.55, -4.1], [0.04, 0.3, 1.2], EYE, 0),
                jbox([2.08, -2.55, -4.1], [0.04, 0.3, 1.2], EYE, 0),
                face([-1.3, -0.5], [-1.0, -0.4], -5.1548, 0.05, [96, 86, 74]),
                face([0.5, 1.3], [-1.0, -0.4], -5.1548, 0.05, [96, 86, 74]),
                jbox([-0.6, 0.1874, -5.0], [1.2, 1.0, 1.2], [190, 181, 161], 20),
            ],
            ROOT,
            [0.0, 15.5, -3.0],
            [0.4363, 0.0, 0.0],
        ),
        bone(
            &[jbox([0.0, -4.5, 0.0], [1.0, 2.0, 1.0], HORN, 15)],
            HEAD as u8,
            [-1.5, -1.5, -1.0],
            [-PI / 8.0, 0.0, 0.0],
        ),
        bone(
            &[jbox([2.0, -4.5, 0.0], [1.0, 2.0, 1.0], HORN, 15)],
            HEAD as u8,
            [-1.5, -1.5, -1.0],
            [-PI / 8.0, 0.0, 0.0],
        ),
        bone(
            &[jbox([-2.0, -0.5, -0.5], [2.0, 1.0, 1.0], [207, 194, 180], 14)],
            HEAD as u8,
            [-1.7, -2.3126, 0.1452],
            [0.0, -FRAC_PI_6, 0.0],
        ),
        bone(
            &[jbox([0.0, -0.5, -0.5], [2.0, 1.0, 1.0], [207, 194, 180], 14)],
            HEAD as u8,
            [1.7, -2.3126, 0.1452],
            [0.0, FRAC_PI_6, 0.0],
        ),
    ];

    /// `GoatModel` (a `QuadrupedModel`) with `BabyGoatModel`'s head held
    /// 22.5 degrees down unless it rams (`getRammingXHeadRot`, up to 30
    /// degrees down); a horn shows while the goat has it.
    pub fn pose(m: &Mob) -> Parts {
        let mut r = Rig::new(BONES);
        r.look(HEAD, m);
        r.quadruped_walk(m, [RIGHT_HIND_LEG, LEFT_HIND_LEG, RIGHT_FRONT_LEG, LEFT_FRONT_LEG], motion::LEG_SWING);
        let a = m.animal.as_deref();
        let ramming = a.map_or(0.0, |a| a.ramming);
        r.x_rot(HEAD, if ramming > 0.0 { ramming.min(1.0) * 30f32.to_radians() } else { PI / 8.0 });
        let horns = a.map_or(2, |a| a.horns);
        r.shown[RIGHT_HORN] = horns >= 1;
        r.shown[LEFT_HORN] = horns >= 2;
        r.parts()
    }
}

// ---------------------------------------------------------------- polar bear

pub(super) mod polar_bear {
    use super::*;

    const FUR: Rgb = [227, 225, 208];
    const LEG: &[Cuboid] = &[
        jbox([-1.5, -0.5, -1.5], [3.0, 3.0, 3.0], FUR, 18).with(Pattern::Fur),
        jbox([-1.45, 2.3, -1.6], [2.9, 0.2, 0.3], [87, 81, 66], 0),
    ];
    static BONES: &[Bone] = &[
        bone(
            &[jbox([-4.0, -3.5, -6.0], [8.0, 7.0, 12.0], FUR, 20).with(Pattern::Fur)],
            ROOT,
            [0.0, 17.5, 0.0],
            [0.0; 3],
        ),
        bone(
            &[
                jbox([-3.0, -2.625, -4.25], [6.0, 5.0, 4.0], FUR, 18).with(Pattern::Fur),
                jbox([-2.0, 0.375, -6.25], [4.0, 2.0, 2.0], [189, 186, 168], 16),
                jbox([-4.0, -3.625, -2.75], [2.0, 2.0, 1.0], FUR, 18),
                jbox([2.0, -3.625, -2.75], [2.0, 2.0, 1.0], FUR, 18),
                // A black nose, eyes with glints and pale inner ears.
                face([-0.9, 0.9], [0.4, 1.3], -6.25, 0.1, [43, 44, 39]),
                face([-2.4, -1.2], [-1.0, 0.0], -4.25, 0.1, [31, 33, 28]),
                face([1.2, 2.4], [-1.0, 0.0], -4.25, 0.1, [31, 33, 28]),
                face([-2.05, -1.65], [-0.9, -0.5], -4.35, 0.05, GLINT),
                face([1.65, 2.05], [-0.9, -0.5], -4.35, 0.05, GLINT),
                face([-3.6, -2.4], [-3.3, -2.1], -2.75, 0.05, [170, 160, 140]),
                face([2.4, 3.6], [-3.3, -2.1], -2.75, 0.05, [170, 160, 140]),
            ],
            ROOT,
            [0.0, 18.625, -5.75],
            [0.0; 3],
        ),
        bone(LEG, ROOT, [-2.5, 21.5, 4.5], [0.0; 3]),
        bone(LEG, ROOT, [2.5, 21.5, 4.5], [0.0; 3]),
        bone(LEG, ROOT, [-2.5, 21.5, -4.5], [0.0; 3]),
        bone(LEG, ROOT, [2.5, 21.5, -4.5], [0.0; 3]),
    ];

    /// `PolarBearModel` (a `QuadrupedModel`); cubs never rear (only adults
    /// stand to attack).
    pub fn pose(m: &Mob) -> Parts {
        let mut r = Rig::new(BONES);
        r.look(1, m);
        r.quadruped_walk(m, [2, 3, 4, 5], motion::LEG_SWING);
        r.parts()
    }
}

// ---------------------------------------------------------------- panda

pub(super) mod panda {
    use super::*;

    const BODY: usize = 0;
    const HEAD: usize = 1;
    const RIGHT_HIND_LEG: usize = 2;
    const LEFT_HIND_LEG: usize = 3;
    const RIGHT_FRONT_LEG: usize = 4;
    const LEFT_FRONT_LEG: usize = 5;

    const WHITE_FUR: Rgb = [235, 232, 219];
    /// The black fur; brown pandas recolour it (`passive::colour`).
    const BLACK_FUR: Rgb = [35, 33, 32];
    const LEG: &[Cuboid] = &[jbox([-1.5, 0.0, -1.5], [3.0, 2.0, 3.0], BLACK_FUR, 14).with(Pattern::Fur)];
    static BONES: &[Bone] = &[
        bone(
            &[
                jbox([-4.5, -3.5, -5.5], [9.0, 7.0, 11.0], WHITE_FUR, 14).with(Pattern::Fur),
                // The black band over the shoulders.
                jbox([-4.6, -3.6, -4.0], [9.2, 7.2, 3.5], BLACK_FUR, 15).with(Pattern::Fur),
            ],
            ROOT,
            [0.0, 18.5, 2.5],
            [0.0; 3],
        ),
        bone(
            &[
                jbox([-3.5, -3.0, -5.0], [7.0, 6.0, 5.0], WHITE_FUR, 12).with(Pattern::Fur),
                jbox([-2.0, 1.0, -6.0], [4.0, 2.0, 1.0], [211, 207, 189], 12),
                jbox([-4.5, -4.0, -3.5], [3.0, 3.0, 1.0], BLACK_FUR, 12),
                jbox([1.5, -4.0, -3.5], [3.0, 3.0, 1.0], BLACK_FUR, 12),
                // Black eye patches with eyes and glints, and a black nose.
                face([-3.0, -0.8], [-1.6, 0.6], -5.0, 0.1, BLACK_FUR),
                face([0.8, 3.0], [-1.6, 0.6], -5.0, 0.1, BLACK_FUR),
                face([-2.3, -1.5], [-1.0, -0.2], -5.1, 0.05, [219, 216, 198]),
                face([1.5, 2.3], [-1.0, -0.2], -5.1, 0.05, [219, 216, 198]),
                face([-0.9, 0.9], [1.1, 1.8], -6.0, 0.1, BLACK_FUR),
            ],
            ROOT,
            [0.0, 19.0, -3.0],
            [0.0; 3],
        ),
        bone(LEG, ROOT, [-3.0, 22.0, 6.5], [0.0; 3]),
        bone(LEG, ROOT, [3.0, 22.0, 6.5], [0.0; 3]),
        bone(LEG, ROOT, [-3.0, 22.0, -1.5], [0.0; 3]),
        bone(LEG, ROOT, [3.0, 22.0, -1.5], [0.0; 3]),
    ];

    /// `PandaModel` (a `QuadrupedModel`) with `BabyPandaModel.animateSitting`:
    /// sitting up, eating (the head bobbing over the paws), scared,
    /// sneezing, unhappy and lying on its back; cubs don't roll
    /// (`PandaRenderer` gives them no `rollAmount`).
    pub fn pose(m: &Mob, time: f32) -> Parts {
        let mut r = Rig::new(BONES);
        let age = age_ticks(time);
        r.look(HEAD, m);
        r.quadruped_walk(m, [RIGHT_HIND_LEG, LEFT_HIND_LEG, RIGHT_FRONT_LEG, LEFT_FRONT_LEG], motion::LEG_SWING);
        let Some(s) = m.panda.as_ref() else { return r.parts() };
        if s.unhappy > 0 {
            r.y_rot(HEAD, 0.35 * (0.6 * age).sin());
            r.z_rot(HEAD, 0.35 * (0.6 * age).sin());
            r.x_rot(RIGHT_FRONT_LEG, -0.75 * (0.3 * age).sin());
            r.x_rot(LEFT_FRONT_LEG, 0.75 * (0.3 * age).sin());
        }
        if s.sneezing > 0 {
            let t = f32::from(s.sneezing);
            if t < 15.0 {
                r.x_rot(HEAD, -FRAC_PI_4 * t / 14.0);
            } else if t < 20.0 {
                r.x_rot(HEAD, -FRAC_PI_4 + FRAC_PI_4 * ((t - 15.0) / 5.0).floor());
            }
        }
        if s.sitting {
            r.x_rot(BODY, PI / 18.0);
            r.at[BODY][2] = -1.5;
            r.at[HEAD][2] = -11.5;
            r.at[HEAD][1] = 17.5;
            r.at[RIGHT_FRONT_LEG][2] = -5.0;
            r.at[LEFT_FRONT_LEG][2] = -5.0;
            r.at[RIGHT_HIND_LEG][2] = 3.0;
            r.at[LEFT_HIND_LEG][2] = 3.0;
            r.z_rot(RIGHT_FRONT_LEG, -0.270_796_42);
            r.z_rot(LEFT_FRONT_LEG, 0.270_796_42);
            r.z_rot(RIGHT_HIND_LEG, 0.570_796_4);
            r.z_rot(LEFT_HIND_LEG, -0.570_796_4);
            if s.eating > 0 {
                r.x_rot(HEAD, FRAC_PI_2 + 0.2 * (age * 0.6).sin());
                r.x_rot(RIGHT_FRONT_LEG, -0.4 - 0.2 * (age * 0.6).sin());
                r.x_rot(LEFT_FRONT_LEG, -0.4 - 0.2 * (age * 0.6).sin());
            }
            if s.scared {
                r.x_rot(HEAD, 2.170_796_4);
                r.x_rot(RIGHT_FRONT_LEG, -0.9);
                r.x_rot(LEFT_FRONT_LEG, -0.9);
            }
        }
        if s.on_back {
            r.x_rot(RIGHT_HIND_LEG, -0.6 * (age * 0.15).sin());
            r.x_rot(LEFT_HIND_LEG, 0.6 * (age * 0.15).sin());
            r.x_rot(RIGHT_FRONT_LEG, 0.3 * (age * 0.25).sin());
            r.x_rot(LEFT_FRONT_LEG, -0.3 * (age * 0.25).sin());
            r.x_rot(HEAD, FRAC_PI_2);
        }
        let mut p = r.parts();
        if s.on_back {
            // `PandaRenderer.setupRotations`: on its back, turned over.
            p.turn_about(Vec3::new(0.0, 3.5, 0.0), Quat::from_rotation_x(PI));
        }
        p
    }
}

// ---------------------------------------------------------------- llama

pub(super) mod llama {
    use super::*;

    const HEAD: usize = 0;

    /// The coat; wild llamas recolour it (`mounts::colour`), a trader
    /// llama's cria stays this cream.
    const FUR: Rgb = [221, 207, 178];
    static BONES: &[Bone] = &[
        bone(
            &[
                jbox([-3.0, -9.0, -4.0], [6.0, 11.0, 4.0], FUR, 20).with(Pattern::Wool),
                jbox([-1.5, -7.0, -7.0], [3.0, 3.0, 3.0], FUR, 18).with(Pattern::Wool),
                jbox([0.5, -11.0, -3.0], [2.0, 2.0, 2.0], FUR, 18),
                jbox([-2.5, -11.0, -3.0], [2.0, 2.0, 2.0], FUR, 18),
                // Eyes on the face, nostrils and a split lip on the snout.
                face([-2.6, -1.4], [-7.4, -6.2], -4.0, 0.1, EYE),
                face([1.4, 2.6], [-7.4, -6.2], -4.0, 0.1, EYE),
                face([-2.25, -1.85], [-7.3, -6.9], -4.1, 0.05, GLINT),
                face([1.85, 2.25], [-7.3, -6.9], -4.1, 0.05, GLINT),
                face([-1.2, -0.4], [-6.6, -6.0], -7.0, 0.05, [96, 80, 64]),
                face([0.4, 1.2], [-6.6, -6.0], -7.0, 0.05, [96, 80, 64]),
                face([-0.15, 0.15], [-5.0, -4.2], -7.0, 0.05, [150, 120, 96]),
            ],
            ROOT,
            [0.0, 12.0, -4.0],
            [0.0; 3],
        ),
        bone(
            &[jbox([-1.4, -0.5, -1.5], [3.0, 8.0, 3.0], FUR, 20).with(Pattern::Wool)],
            ROOT,
            [-2.5, 16.5, 4.5],
            [0.0; 3],
        ),
        bone(
            &[jbox([-1.6, -0.5, -1.5], [3.0, 8.0, 3.0], FUR, 20).with(Pattern::Wool)],
            ROOT,
            [2.5, 16.5, 4.5],
            [0.0; 3],
        ),
        bone(
            &[jbox([-1.4, -0.5, -1.5], [3.0, 8.0, 3.0], FUR, 20).with(Pattern::Wool)],
            ROOT,
            [-2.5, 16.5, -3.5],
            [0.0; 3],
        ),
        bone(
            &[jbox([-1.6, -0.5, -1.5], [3.0, 8.0, 3.0], FUR, 20).with(Pattern::Wool)],
            ROOT,
            [2.5, 16.5, -3.5],
            [0.0; 3],
        ),
        bone(
            &[jbox([-4.0, -3.0, -8.5], [8.0, 6.0, 13.0], FUR, 20).with(Pattern::Wool)],
            ROOT,
            [0.0, 14.0, 2.5],
            [0.0; 3],
        ),
    ];

    /// `LlamaModel`: the head looks; the legs as a quadruped's. Crias carry
    /// no chests or carpets (`hasChest` and the decor layer are adults').
    pub fn pose(m: &Mob) -> Parts {
        let mut r = Rig::new(BONES);
        r.look(HEAD, m);
        r.quadruped_walk(m, [1, 2, 3, 4], motion::LEG_SWING);
        r.parts()
    }
}

// ---------------------------------------------------------------- camel

#[allow(dead_code)]
pub(super) mod camel {
    use super::*;

    pub const BODY: usize = 0;
    pub const TAIL: usize = 1;
    pub const HEAD: usize = 2;
    pub const RIGHT_EAR: usize = 3;
    pub const LEFT_EAR: usize = 4;
    pub const RIGHT_FRONT_LEG: usize = 5;
    pub const LEFT_FRONT_LEG: usize = 6;
    pub const LEFT_HIND_LEG: usize = 7;
    pub const RIGHT_HIND_LEG: usize = 8;

    include!("keyframe_defs/camel_baby.rs");

    const SAND: Rgb = [219, 169, 95];
    const LEG: &[Cuboid] = &[
        jbox([-1.5, -0.5, -1.5], [3.0, 13.0, 3.0], SAND, 16),
        jbox([-1.55, 10.5, -1.55], [3.1, 2.0, 3.1], [130, 95, 55], 10),
    ];
    static BONES: &[Bone] = &[
        bone(
            &[
                jbox([-4.5, -4.0, -8.0], [9.0, 8.0, 16.0], SAND, 18),
                // The calf's soft hump.
                jbox([-3.0, -5.0, -3.0], [6.0, 1.0, 6.0], [205, 152, 80], 18),
            ],
            ROOT,
            [0.0, 7.0, 0.0],
            [0.0; 3],
        ),
        bone(
            &[jbox([-1.5, -0.5, -0.25], [3.0, 9.0, 0.5], [160, 115, 60], 10)],
            BODY as u8,
            [0.0, -1.5, 8.05],
            [0.0; 3],
        ),
        bone(
            &[
                jbox([-2.5, -3.0, -7.5], [5.0, 5.0, 7.0], SAND, 16),
                jbox([-2.5, -12.0, -7.5], [5.0, 9.0, 5.0], SAND, 16),
                jbox([-2.5, -12.0, -10.5], [5.0, 4.0, 3.0], SAND, 16),
                // Eyes, nostrils and the split lip.
                jbox([-2.6, -11.6, -7.0], [0.1, 1.0, 1.2], EYE, 0),
                jbox([2.5, -11.6, -7.0], [0.1, 1.0, 1.2], EYE, 0),
                face([-1.8, -0.8], [-11.6, -11.0], -10.5, 0.05, [96, 66, 36]),
                face([0.8, 1.8], [-11.6, -11.0], -10.5, 0.05, [96, 66, 36]),
                face([-0.15, 0.15], [-10.4, -8.0], -10.5, 0.05, [150, 108, 60]),
            ],
            BODY as u8,
            [0.0, 1.0, -7.5],
            [0.0; 3],
        ),
        bone(
            &[jbox([-3.0, -0.5, -1.0], [3.0, 1.0, 2.0], [205, 152, 80], 10)],
            HEAD as u8,
            [-2.5, -11.0, -4.0],
            [0.0; 3],
        ),
        bone(&[jbox([0.0, -0.5, -1.0], [3.0, 1.0, 2.0], [205, 152, 80], 10)], HEAD as u8, [2.5, -11.0, -4.0], [0.0; 3]),
        bone(LEG, ROOT, [-3.0, 11.5, -5.5], [0.0; 3]),
        bone(LEG, ROOT, [3.0, 11.5, -5.5], [0.0; 3]),
        bone(LEG, ROOT, [3.0, 11.5, 5.5], [0.0; 3]),
        bone(LEG, ROOT, [-3.0, 11.5, 5.5], [0.0; 3]),
    ];

    /// `CamelModel` with `BabyCamelModel`'s keyframes (`CamelBabyAnimation`):
    /// the head turns within 30 degrees and pitches from -25 to 45
    /// (`applyHeadRotation`), the walk keyframes run on the walk
    /// (`applyWalk(pos, speed, 2, 2.5)`), and sitting, standing up and the
    /// dash play on the pose clock Track 3 keeps.
    pub fn pose(m: &Mob, alpha: f32) -> Parts {
        let mut r = Rig::new(BONES);
        let s = m.mount.as_ref();
        let mut pitch = (-m.head_pitch.to_degrees()).clamp(-25.0, 45.0);
        if let Some(cooldown) = s.map(|s| s.dash_cooldown).filter(|&c| c > 0) {
            pitch = (pitch + 45.0 * f32::from(cooldown) / 55.0).clamp(-25.0, 70.0);
        }
        r.x_rot(HEAD, pitch.to_radians());
        r.y_rot(HEAD, m.head_yaw.to_degrees().clamp(-30.0, 30.0).to_radians());
        let mut walking = true;
        if let Some(s) = s {
            let t = s.pose_time as f32 + alpha;
            if s.camel_sitting {
                walking = false;
                if s.pose_time < crate::entity::mounts::camel::SITDOWN_TICKS {
                    CAMEL_BABY_SIT.apply(&mut r, t);
                } else {
                    CAMEL_BABY_SIT_POSE.apply(&mut r, t);
                }
            } else {
                if s.dashing {
                    walking = false;
                    CAMEL_BABY_DASH.apply(&mut r, f32::from(s.dash_ticks) + alpha);
                }
                if s.in_pose_transition() {
                    CAMEL_BABY_STANDUP.apply(&mut r, t);
                }
            }
        }
        if walking {
            CAMEL_BABY_WALK.apply_walk(&mut r, m.limb_phase, m.limb_amp, 2.0, 2.5);
        }
        r.parts()
    }
}

// ---------------------------------------------------------------- fox

pub(super) mod fox {
    use super::*;

    pub const HEAD: usize = 0;
    pub const RIGHT_HIND_LEG: usize = 1;
    pub const LEFT_HIND_LEG: usize = 2;
    pub const RIGHT_FRONT_LEG: usize = 3;
    pub const LEFT_FRONT_LEG: usize = 4;
    pub const BODY: usize = 5;
    pub const TAIL: usize = 6;

    include!("keyframe_defs/fox_baby.rs");

    /// The coat; snow foxes recolour it (`animals::colour`).
    const RED: Rgb = [201, 108, 47];
    const PALE: Rgb = [243, 229, 207];
    const SOCK: Rgb = [65, 44, 34];
    const LEG: &[Cuboid] = &[jbox([-1.0, 0.0, -1.0], [2.0, 2.0, 2.0], SOCK, 16)];
    static BONES: &[Bone] = &[
        bone(
            &[
                jbox([-3.0, -2.125, -5.125], [6.0, 5.0, 5.0], RED, 20).with(Pattern::Fur),
                jbox([-1.0, 0.875, -7.125], [2.0, 2.0, 2.0], PALE, 15),
                jbox([-3.0, -4.125, -4.125], [2.0, 2.0, 1.0], SOCK, 16),
                jbox([1.0, -4.125, -4.125], [2.0, 2.0, 1.0], SOCK, 16),
                // White cheeks, a black nose, eyes with glints.
                face([-3.0, -1.2], [0.9, 2.875], -5.125, 0.1, PALE),
                face([1.2, 3.0], [0.9, 2.875], -5.125, 0.1, PALE),
                face([-0.6, 0.6], [0.9, 1.6], -7.125, 0.1, [35, 31, 29]),
                face([-2.5, -1.3], [-0.6, 0.3], -5.125, 0.1, [28, 26, 23]),
                face([1.3, 2.5], [-0.6, 0.3], -5.125, 0.1, [28, 26, 23]),
                face([-1.75, -1.4], [-0.55, -0.2], -5.225, 0.05, [226, 214, 196]),
                face([1.4, 1.75], [-0.55, -0.2], -5.225, 0.05, [226, 214, 196]),
            ],
            ROOT,
            [0.0, 18.125, 0.125],
            [0.0; 3],
        ),
        bone(LEG, ROOT, [-1.5, 22.0, 4.0], [0.0; 3]),
        bone(LEG, ROOT, [1.5, 22.0, 4.0], [0.0; 3]),
        bone(LEG, ROOT, [-1.5, 22.0, 0.0], [0.0; 3]),
        bone(LEG, ROOT, [1.5, 22.0, 0.0], [0.0; 3]),
        bone(
            &[
                jbox([-2.5, -2.0, -3.0], [5.0, 4.0, 6.0], RED, 24).with(Pattern::Fur),
                jbox([-2.0, 1.9, -2.5], [4.0, 0.2, 4.0], [239, 229, 209], 12),
            ],
            ROOT,
            [0.0, 20.0, 2.0],
            [0.0; 3],
        ),
        bone(
            &[
                jbox([-1.5, -1.48, -1.0], [3.0, 3.0, 4.0], RED, 24).with(Pattern::Fur),
                jbox([-1.5, -1.48, 3.0], [3.0, 3.0, 2.0], [242, 234, 218], 16),
            ],
            BODY as u8,
            [0.0, -0.5, 3.0],
            [0.0; 3],
        ),
    ];

    /// `FoxModel` with `BabyFoxModel`'s poses: the walk keyframes
    /// (`FOX_BABY_WALK`, `applyWalk(pos, speed, 1, 2.5)`), the head looking
    /// (tilted while it begs), sitting up and curled asleep (legs hidden).
    pub fn pose(m: &Mob, time: f32) -> Parts {
        let mut r = Rig::new(BONES);
        let a = m.animal.as_deref();
        let (sitting, sleeping) = (a.is_some_and(|a| a.sit_pose), a.is_some_and(|a| a.sleeping));
        if a.is_some_and(|a| a.begging) {
            r.z_rot(HEAD, 0.11 * PI);
        }
        FOX_BABY_WALK.apply_walk(&mut r, m.limb_phase, m.limb_amp, 1.0, 2.5);
        if sleeping {
            r.rot[BODY] = [-PI / 18.0, 0.0, -FRAC_PI_2];
            r.at[BODY][0] -= 1.0;
            r.at[BODY][1] += 1.0;
            r.at[BODY][2] -= 1.0;
            r.x_rot(TAIL, -2.181_661_6);
            r.at[TAIL][0] -= 0.7;
            r.at[TAIL][1] += 0.9;
            r.at[TAIL][2] += 0.6;
            r.at[HEAD][0] -= 2.0;
            r.at[HEAD][1] += 2.8;
            r.at[HEAD][2] -= 4.0;
            for leg in [RIGHT_HIND_LEG, LEFT_HIND_LEG, RIGHT_FRONT_LEG, LEFT_FRONT_LEG] {
                r.shown[leg] = false;
            }
            r.rot[HEAD] = [0.0, -PI * 2.0 / 3.0, (age_ticks(time) * 0.027).cos() / 22.0];
        } else {
            if sitting {
                r.x_rot(BODY, -0.959_931);
                r.at[BODY][2] -= 4.5 * AGE_SCALE;
                r.at[BODY][1] += 3.0 * AGE_SCALE;
                r.at[TAIL][1] -= 0.6;
                r.at[TAIL][2] -= 2.0 * AGE_SCALE;
                r.x_rot(TAIL, 0.959_931_14);
                r.at[HEAD][1] -= 0.75;
                r.x_rot(RIGHT_FRONT_LEG, -PI / 12.0);
                r.x_rot(LEFT_FRONT_LEG, -PI / 12.0);
                r.at[RIGHT_FRONT_LEG][2] -= 1.0;
                r.at[LEFT_FRONT_LEG][2] -= 1.0;
                r.at[RIGHT_HIND_LEG][2] -= 3.75;
                r.at[LEFT_HIND_LEG][2] -= 3.75;
            }
            r.x_rot(HEAD, head_x(m));
            r.y_rot(HEAD, m.head_yaw);
        }
        r.parts()
    }
}

// ---------------------------------------------------------------- cat, ocelot

pub(super) mod feline {
    use super::*;

    const HEAD: usize = 0;
    const LEFT_FRONT_LEG: usize = 1;
    const RIGHT_FRONT_LEG: usize = 2;
    const LEFT_HIND_LEG: usize = 3;
    const BODY: usize = 4;
    const RIGHT_HIND_LEG: usize = 5;
    const TAIL: usize = 6;
    const COLLAR: usize = 7;

    /// `BabyFelineModel` in one coat: the cat's (`passive::colour` paints
    /// each variant) or the ocelot's.
    macro_rules! feline {
        ($bones:ident, $fur:expr, $face:expr, $dark:expr, $eye:expr) => {
            pub static $bones: &[Bone] = &{
                const FUR: Rgb = $fur;
                const LEG: &[Cuboid] = &[jbox([-0.5, 0.0, -1.0], [1.0, 2.0, 2.0], FUR, 16)];
                [
                    bone(
                        &[
                            jbox([-2.5, -3.0, -2.875], [5.0, 4.0, 4.0], FUR, 15),
                            jbox([-2.0, -4.0, -0.875], [1.0, 1.0, 2.0], $dark, 12),
                            jbox([1.0, -4.0, -0.875], [1.0, 1.0, 2.0], $dark, 12),
                            jbox([-1.5, -1.0, -3.875], [3.0, 2.0, 1.0], $face, 10),
                            // Eyes with slit pupils and a pink nose.
                            face([-2.2, -1.0], [-2.2, -1.2], -2.875, 0.1, $eye),
                            face([1.0, 2.2], [-2.2, -1.2], -2.875, 0.1, $eye),
                            face([-1.75, -1.45], [-2.2, -1.2], -2.975, 0.05, [20, 22, 16]),
                            face([1.45, 1.75], [-2.2, -1.2], -2.975, 0.05, [20, 22, 16]),
                            face([-0.5, 0.5], [-1.0, -0.4], -3.875, 0.05, [214, 140, 150]),
                        ],
                        ROOT,
                        [0.0, 20.0, -3.125],
                        [0.0; 3],
                    ),
                    bone(LEG, ROOT, [1.0, 22.0, -1.5], [0.0; 3]),
                    bone(LEG, ROOT, [-1.0, 22.0, -1.5], [0.0; 3]),
                    bone(LEG, ROOT, [1.0, 22.0, 2.5], [0.0; 3]),
                    bone(&[jbox([-2.0, -1.5, -3.5], [4.0, 3.0, 7.0], FUR, 18)], ROOT, [0.0, 20.5, 0.5], [0.0; 3]),
                    bone(LEG, ROOT, [-1.0, 22.0, 2.5], [0.0; 3]),
                    bone(
                        &[
                            jbox([-0.5, -0.107, 0.0849], [1.0, 1.0, 5.0], FUR, 18),
                            jbox([-0.55, -0.157, 3.6], [1.1, 1.1, 0.8], $dark, 4),
                        ],
                        ROOT,
                        [0.0, 19.107, 3.9151],
                        [-0.567_232, 0.0, 0.0],
                    ),
                    // `CAT_BABY_COLLAR`: the collar round the neck.
                    bone(
                        &[jbox([-2.1, -1.6, -3.6], [4.2, 3.2, 1.0], super::super::animals::COLLAR, 0)],
                        BODY as u8,
                        [0.0; 3],
                        [0.0; 3],
                    ),
                ]
            };
        };
    }
    feline!(CAT, [27, 27, 29], [30, 29, 31], [24, 24, 26], [167, 186, 62]);
    feline!(OCELOT, [195, 151, 77], [228, 213, 175], [61, 43, 30], [101, 157, 65]);

    /// `BabyFelineModel.setupAnim`: the head looks; standing, the legs walk
    /// `cos(pos * 0.6662) * speed` (sprinting, each pair 0.3 apart);
    /// crouching lowers the body, head and tail; sitting tips the body up.
    pub fn pose(m: &Mob) -> Parts {
        let cat = m.kind == MobKind::Cat;
        let mut r = Rig::new(if cat { CAT } else { OCELOT });
        let (crouching, sprinting) = m.ocelot.as_ref().map_or((false, false), |s| (s.crouching, s.sprinting));
        let sitting = cat && m.animal.as_ref().is_some_and(|a| a.sit_pose);
        r.shown[COLLAR] = cat && m.animal.as_ref().is_some_and(|a| a.owner.is_some());
        if crouching {
            r.at[BODY][1] += AGE_SCALE;
            r.at[HEAD][1] += 2.0 * AGE_SCALE;
            r.at[TAIL][1] += AGE_SCALE;
            r.x_rot(TAIL, FRAC_PI_2);
        } else if sprinting {
            r.x_rot(TAIL, FRAC_PI_2);
        }
        r.look(HEAD, m);
        let a = m.limb_phase * motion::WALK_FREQ;
        let leg = |offset: f32| (a + offset).cos() * m.limb_amp;
        if !sitting {
            let [lh, rh, lf, rf] = if sprinting {
                [leg(0.0), leg(0.3), leg(PI + 0.3), leg(PI)]
            } else {
                [leg(0.0), leg(PI), leg(PI), leg(0.0)]
            };
            r.x_rot(LEFT_HIND_LEG, lh);
            r.x_rot(RIGHT_HIND_LEG, rh);
            r.x_rot(LEFT_FRONT_LEG, lf);
            r.x_rot(RIGHT_FRONT_LEG, rf);
        } else {
            r.rot[BODY][0] += -0.436_332_32;
            r.at[BODY][1] += 1.0;
            r.at[HEAD][2] += 0.75;
            r.rot[TAIL][0] += 0.545_415_4;
            r.at[TAIL][1] += 4.0;
            r.at[TAIL][2] -= 0.9;
            r.at[LEFT_HIND_LEG][2] -= 0.9;
            r.at[RIGHT_HIND_LEG][2] -= 0.9;
        }
        r.parts()
    }
}

// ---------------------------------------------------------------- rabbit

pub(super) mod rabbit {
    use super::*;

    pub const BODY: usize = 0;
    pub const TAIL: usize = 2;
    pub const HEAD: usize = 4;
    pub const RIGHT_EAR: usize = 5;
    pub const LEFT_EAR: usize = 6;
    pub const FRONTLEGS: usize = 7;
    pub const LEFT_FRONT_LEG: usize = 8;
    pub const RIGHT_FRONT_LEG: usize = 10;
    pub const BACKLEGS: usize = 12;
    pub const LEFT_HIND_LEG: usize = 13;
    pub const RIGHT_HIND_LEG: usize = 15;

    include!("keyframe_defs/rabbit_baby.rs");

    /// The coat (`animals::colour` paints each variant) and the eyes (red
    /// on white rabbits).
    const FUR: Rgb = [158, 123, 88];
    const PAW: &[Cuboid] = &[jbox([-0.5, -1.5, -0.5], [1.0, 3.0, 1.0], FUR, 22)];
    const HAUNCH: &[Cuboid] = &[jbox([-2.0, -0.5, 0.0], [2.0, 1.0, 3.0], FUR, 22)];
    const EAR: &[Cuboid] = &[
        jbox([-1.0, -3.5, -0.5], [2.0, 4.0, 1.0], FUR, 18),
        face([-0.6, 0.6], [-3.1, -0.2], -0.5, 0.05, [220, 164, 161]),
    ];
    static BONES: &[Bone] = &[
        bone(&[], ROOT, [0.0, 23.0, 1.6], [0.0; 3]),
        bone(
            &[jbox([-2.0, -2.0, -3.0], [4.0, 3.0, 6.0], FUR, 24).with(Pattern::Fur)],
            BODY as u8,
            [0.0, -2.0, -1.6],
            [-FRAC_PI_6, 0.0, 0.0],
        ),
        bone(&[], BODY as u8, [0.0, -2.2, 2.0], [0.0; 3]),
        bone(
            &[jbox([-1.4, -2.0268, -1.0177], [3.0, 3.0, 3.0], [236, 225, 202], 12)],
            TAIL as u8,
            [-0.1, 0.0, 0.0],
            [-FRAC_PI_6, 0.0, 0.0],
        ),
        bone(
            &[
                jbox([-2.5, -3.0, -3.0], [5.0, 4.0, 4.0], FUR, 22).with(Pattern::Fur),
                // Eyes with glints, a pink nose and whiskers.
                face([-2.2, -1.0], [-1.9, -0.8], -3.0, 0.1, [30, 28, 26]),
                face([1.0, 2.2], [-1.9, -0.8], -3.0, 0.1, [30, 28, 26]),
                face([-1.75, -1.45], [-1.8, -1.5], -3.1, 0.05, GLINT),
                face([1.45, 1.75], [-1.8, -1.5], -3.1, 0.05, GLINT),
                face([-0.5, 0.5], [-0.4, 0.2], -3.0, 0.1, [218, 170, 166]),
                face([-2.5, -1.4], [0.15, 0.25], -3.0, 0.08, [214, 210, 200]),
                face([1.4, 2.5], [0.15, 0.25], -3.0, 0.08, [214, 210, 200]),
            ],
            BODY as u8,
            [0.0, -5.0, -2.6],
            [0.0; 3],
        ),
        bone(EAR, HEAD as u8, [-1.5, -3.5, -0.5], [0.0; 3]),
        bone(EAR, HEAD as u8, [1.5, -3.5, -0.5], [0.0; 3]),
        bone(&[], BODY as u8, [0.0, -2.5, -2.6], [0.0; 3]),
        bone(&[], FRONTLEGS as u8, [1.0, 1.0, -0.5], [FRAC_PI_8, 0.0, 0.0]),
        bone(PAW, LEFT_FRONT_LEG as u8, [0.0, 1.0, 0.0], [-FRAC_PI_8, 0.0, 0.0]),
        bone(&[], FRONTLEGS as u8, [-1.0, 1.0, -0.5], [FRAC_PI_8, 0.0, 0.0]),
        bone(PAW, RIGHT_FRONT_LEG as u8, [0.0, 1.0, 0.0], [-FRAC_PI_8, 0.0, 0.0]),
        bone(&[], ROOT, [0.0, 23.0, 2.0], [0.0; 3]),
        bone(&[], BACKLEGS as u8, [1.5, 0.5, 0.5], [0.0, PI, 0.0]),
        bone(HAUNCH, LEFT_HIND_LEG as u8, [1.0, 0.0, 0.5], [0.0, -FRAC_PI_4, 0.0]),
        bone(&[], BACKLEGS as u8, [-1.5, 0.5, 0.5], [0.0, PI, 0.0]),
        bone(HAUNCH, RIGHT_HIND_LEG as u8, [0.5, 0.0, -0.9], [0.0, FRAC_PI_4, 0.0]),
    ];

    /// `RabbitModel.setupAnim` on `BabyRabbitModel`'s parts with
    /// `BabyRabbitAnimation`: the head looks unless the idle tilt plays,
    /// the hop's keyframes from the tick it left the ground and the head
    /// tilt's every 180 to 220 idle ticks (Track 7b's states).
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

// ---------------------------------------------------------------- horses

/// A foal's coat and mane, painted by [`colour`] from the adult's colours.
pub(super) const COAT: Rgb = [4, 2, 1];
pub(super) const MANE: Rgb = [4, 1, 2];

pub(super) mod equine {
    use super::*;

    /// The parts `AbstractEquineModel.setupAnim` moves.
    #[derive(Clone, Copy)]
    struct Equine {
        body: usize,
        tail: usize,
        head_parts: usize,
        left_hind_leg: usize,
        right_hind_leg: usize,
        left_front_leg: usize,
        right_front_leg: usize,
        /// `getLegStandingYOffset`, `getLegStandingZOffset`,
        /// `getLegStandAngle`, `getLegStandingXRotOffset`,
        /// `getTailXRotOffset`.
        stand_y: f32,
        stand_z: f32,
        stand_angle: f32,
        stand_x_rot: f32,
        tail_x: f32,
        donkey: bool,
    }

    const HOOF: Rgb = [50, 44, 38];
    const LEG: &[Cuboid] = &[
        jbox([-1.5, -1.0, -1.5], [3.0, 9.0, 3.0], COAT, 18).with(Pattern::Fur),
        jbox([-1.55, 6.5, -1.55], [3.1, 1.5, 3.1], HOOF, 12),
    ];
    /// `BabyHorseModel.createBabyMesh` (horses, skeleton and zombie horses).
    static HORSE: &[Bone] = &[
        bone(
            &[jbox([-4.0, -3.5, -7.0], [8.0, 7.0, 14.0], COAT, 22).with(Pattern::Fur)],
            ROOT,
            [0.0, 12.5, 0.0],
            [0.0; 3],
        ),
        bone(&[jbox([-1.5, -1.5, -1.0], [3.0, 3.0, 8.0], MANE, 20)], 0, [0.0, -1.0, 7.0], [-0.7418, 0.0, 0.0]),
        bone(LEG, ROOT, [2.4, 16.0, 5.4], [0.0; 3]),
        bone(LEG, ROOT, [-2.4, 16.0, 5.4], [0.0; 3]),
        bone(LEG, ROOT, [2.4, 16.0, -5.4], [0.0; 3]),
        bone(LEG, ROOT, [-2.4, 16.0, -5.4], [0.0; 3]),
        bone(
            &[
                jbox([-2.0, -6.0, -2.0], [4.0, 8.0, 4.0], COAT, 18).with(Pattern::Fur),
                // A short mane down the back of the neck.
                jbox([-0.75, -6.5, 1.8], [1.5, 7.5, 0.8], MANE, 16),
            ],
            ROOT,
            [0.0, 10.0, -6.0],
            [0.6109, 0.0, 0.0],
        ),
        bone(
            &[
                jbox([-3.0, -3.9484, -6.705], [6.0, 4.0, 9.0], COAT, 18).with(Pattern::Fur),
                // Eyes with glints on the sides, nostrils on the muzzle.
                jbox([-3.1, -3.3, -3.6], [0.1, 1.0, 1.2], EYE, 0),
                jbox([3.0, -3.3, -3.6], [0.1, 1.0, 1.2], EYE, 0),
                jbox([-3.15, -3.2, -3.5], [0.05, 0.35, 0.35], GLINT, 0),
                jbox([3.1, -3.2, -3.5], [0.05, 0.35, 0.35], GLINT, 0),
                face([-2.2, -1.2], [-2.6, -1.8], -6.705, 0.05, [30, 26, 22]),
                face([1.2, 2.2], [-2.6, -1.8], -6.705, 0.05, [30, 26, 22]),
            ],
            6,
            [0.0, -6.0516, -0.2951],
            [0.0; 3],
        ),
        bone(&[jbox([-1.0, -2.5, -0.8], [2.0, 3.0, 1.0], COAT, 14)], 7, [2.0, -4.2484, 1.9451], [0.0, 0.0, 0.2618]),
        bone(&[jbox([-1.0, -2.5, -0.5], [2.0, 3.0, 1.0], COAT, 14)], 7, [-2.0, -4.2484, 1.645], [0.0, 0.0, -0.2618]),
    ];
    const HORSE_PARTS: Equine = Equine {
        body: 0,
        tail: 1,
        head_parts: 6,
        left_hind_leg: 2,
        right_hind_leg: 3,
        left_front_leg: 4,
        right_front_leg: 5,
        stand_y: 4.0,
        stand_z: 0.0,
        stand_angle: PI / 12.0,
        stand_x_rot: -PI / 3.0,
        tail_x: -FRAC_PI_2,
        donkey: false,
    };

    const DONKEY_LEG: &[Cuboid] = &[
        jbox([-2.5, -1.5, -1.5], [3.0, 8.0, 3.0], COAT, 18).with(Pattern::Fur),
        jbox([-2.55, 5.0, -1.55], [3.1, 1.5, 3.1], HOOF, 12),
    ];
    /// `BabyDonkeyModel.createBabyLayer` (donkeys and mules): the legs and
    /// head hang from the body.
    static DONKEY: &[Bone] = &[
        bone(
            &[jbox([-5.0, -3.0, -7.0], [8.0, 6.0, 14.0], COAT, 22).with(Pattern::Fur)],
            ROOT,
            [1.0, 14.0, 0.0],
            [0.0; 3],
        ),
        bone(&[], 0, [0.0, -1.5, 6.5], [0.0; 3]),
        bone(&[jbox([-2.5, -1.0, -0.5], [3.0, 3.0, 8.0], MANE, 20)], 1, [0.0; 3], [-0.7418, 0.0, 0.0]),
        bone(DONKEY_LEG, 0, [2.25, 3.5, 5.25], [0.0; 3]),
        bone(DONKEY_LEG, 0, [-2.4, 3.5, 5.4], [0.0; 3]),
        bone(DONKEY_LEG, 0, [2.4, 3.5, -5.3], [0.0; 3]),
        bone(DONKEY_LEG, 0, [-2.4, 3.5, -5.4], [0.0; 3]),
        bone(&[], 0, [0.0, -3.0, -5.0], [0.0; 3]),
        bone(
            &[
                jbox([-3.0, -6.0, -3.0], [4.0, 8.0, 4.0], COAT, 18).with(Pattern::Fur),
                jbox([-1.75, -6.5, 0.8], [1.5, 7.5, 0.8], MANE, 16),
            ],
            7,
            [0.0; 3],
            [FRAC_PI_8, 0.0, 0.0],
        ),
        bone(&[], 7, [0.0, -5.0, -3.0], [0.0; 3]),
        bone(
            &[
                jbox([-4.0, -3.6, -8.4], [6.0, 4.0, 9.0], COAT, 18).with(Pattern::Fur),
                jbox([-4.1, -3.0, -5.2], [0.1, 1.0, 1.2], EYE, 0),
                jbox([2.0, -3.0, -5.2], [0.1, 1.0, 1.2], EYE, 0),
                face([-3.2, -2.2], [-2.4, -1.6], -8.4, 0.05, [30, 26, 22]),
                face([0.2, 1.2], [-2.4, -1.6], -8.4, 0.05, [30, 26, 22]),
            ],
            9,
            [0.0, -1.0, 1.0],
            [FRAC_PI_8, 0.0, 0.0],
        ),
        bone(&[jbox([-2.0, -6.5, -0.3], [2.0, 7.0, 1.0], COAT, 14)], 9, [2.0, -3.5, -1.0], [0.48, 0.0, 0.48]),
        bone(&[jbox([-2.0, -6.5, -0.3], [2.0, 7.0, 1.0], COAT, 14)], 9, [-2.0, -3.5, -1.0], [0.48, 0.0, -0.48]),
    ];
    const DONKEY_PARTS: Equine = Equine {
        body: 0,
        tail: 1,
        head_parts: 7,
        left_hind_leg: 3,
        right_hind_leg: 4,
        left_front_leg: 5,
        right_front_leg: 6,
        stand_y: 1.0,
        stand_z: 0.5,
        stand_angle: PI / 3.0,
        stand_x_rot: 0.0,
        tail_x: -FRAC_PI_4,
        donkey: true,
    };

    /// `AbstractEquineModel.setupAnim` with each foal model's overrides:
    /// the head (turned within 20 degrees) nods with a fast walk, lowers to
    /// graze (`eatAnim`), shakes as it chews (`mouthAnim`) and lifts as it
    /// rears (`standAnim`); rearing tips the body up 45 degrees, lifts the
    /// front legs pawing on `cos(ageInTicks * 0.6 + pi)` and angles the
    /// hind legs; the legs walk (a fifth as fast in water) and the tail
    /// lifts with the walk and swishes while `tailCounter` runs. The donkey
    /// foal holds its head at a fixed 30 degrees up (`BabyDonkeyModel`).
    pub fn pose(m: &Mob, time: f32, alpha: f32) -> Parts {
        let donkey = matches!(m.kind, MobKind::Donkey | MobKind::Mule);
        let (bones, e) = if donkey { (DONKEY, DONKEY_PARTS) } else { (HORSE, HORSE_PARTS) };
        let mut r = Rig::new(bones);
        let age = age_ticks(time);
        let s = m.mount.as_ref();
        let (eating, standing, feeding) = s.map_or((0.0, 0.0, 0.0), |s| {
            (s.eat_anim(alpha), s.stand_anim(alpha), s.mouth_anim_o + (s.mouth_anim - s.mouth_anim_o) * alpha)
        });
        let yaw = m.head_yaw.to_degrees().clamp(-20.0, 20.0).to_radians();
        let (pos, speed) = (m.limb_phase, m.limb_amp);
        let mut head_x = head_x(m);
        if speed > 0.2 {
            head_x += (pos * 0.8).cos() * 0.15 * speed;
        }
        let i_standing = 1.0 - standing;
        let water = if m.in_water { 0.2 } else { 1.0 };
        let leg = (water * pos * motion::WALK_FREQ + PI).cos();
        let leg_x = leg * 0.8 * speed;
        let rest = 1.0 - standing.max(eating);
        let head_rot = |head_x: f32| {
            let base = rest * (PI / 6.0 + head_x + feeding * age.sin() * 0.05);
            standing * (PI / 12.0 + head_x) + eating * (2.181_661_6 + age.sin() * 0.05) + base
        };
        r.x_rot(e.head_parts, head_rot(head_x));
        r.y_rot(e.head_parts, standing * yaw + rest * yaw);
        // `animateHeadPartsPlacement`.
        if donkey {
            r.at[e.head_parts][1] += (-1.2 - r.at[e.head_parts][1]) * eating;
            r.at[e.head_parts][2] += (-3.6 - r.at[e.head_parts][2]) * standing;
        } else {
            r.at[e.head_parts][1] += -2.0 * standing + (2.0 - -2.0 * standing) * eating;
            r.at[e.head_parts][2] += (-4.0 - r.at[e.head_parts][2]) * standing;
        }
        r.x_rot(e.body, standing * -FRAC_PI_4 + i_standing * r.rot[e.body][0]);
        r.at[e.left_front_leg][1] -= e.stand_y * standing;
        r.at[e.left_front_leg][2] += e.stand_z * standing;
        r.at[e.right_front_leg][1] = r.at[e.left_front_leg][1];
        r.at[e.right_front_leg][2] = r.at[e.left_front_leg][2];
        let bob = (age * 0.6 + PI).cos();
        let stand_angle = e.stand_angle * standing;
        r.x_rot(e.left_hind_leg, stand_angle - leg * 0.5 * speed * i_standing);
        r.x_rot(e.right_hind_leg, stand_angle + leg * 0.5 * speed * i_standing);
        r.x_rot(e.left_front_leg, (e.stand_x_rot + bob) * standing + leg_x * i_standing);
        r.x_rot(e.right_front_leg, (e.stand_x_rot - bob) * standing - leg_x * i_standing);
        if donkey {
            // `offsetLegPositionWhenStanding`, as Java writes it (the right
            // hind leg follows the left's lifted height).
            r.at[e.left_hind_leg][1] += (-0.3 - r.at[e.left_hind_leg][1]) * standing;
            r.at[e.right_hind_leg][1] = r.at[e.left_hind_leg][1] + (-0.3 - r.at[e.left_hind_leg][1]) * standing;
        }
        r.x_rot(e.tail, e.tail_x + PI / 6.0 + speed * 0.75);
        r.at[e.tail][1] += speed * AGE_SCALE;
        r.at[e.tail][2] += speed * 2.0 * AGE_SCALE;
        r.y_rot(e.tail, if m.anim.tail_counter > 0 { (age * 0.7).cos() } else { 0.0 });
        if e.donkey {
            r.x_rot(e.head_parts, head_rot(-30f32.to_radians()));
        }
        r.parts()
    }
}

// ---------------------------------------------------------------- turtle

pub(super) mod turtle {
    use super::*;

    const SKIN: Rgb = [102, 168, 96];
    static BONES: &[Bone] = &[
        bone(
            &[
                jbox([-2.0, -1.0, -2.0], [4.0, 2.0, 4.0], [70, 110, 62], 26).with(Pattern::Mottle),
                jbox([-2.05, 0.4, -2.05], [4.1, 0.65, 4.1], [214, 204, 150], 12),
                jbox([-0.15, -1.05, -1.6], [0.3, 0.05, 3.2], [52, 84, 46], 0),
            ],
            ROOT,
            [0.0, 22.9, 1.0],
            [0.0; 3],
        ),
        bone(
            &[
                jbox([-1.5, -2.0, -3.0], [3.0, 3.0, 3.0], SKIN, 16),
                jbox([-1.6, -1.2, -2.6], [0.1, 0.7, 0.7], EYE, 0),
                jbox([1.5, -1.2, -2.6], [0.1, 0.7, 0.7], EYE, 0),
                face([-1.0, 1.0], [-0.1, 0.15], -3.0, 0.05, [52, 86, 48]),
            ],
            ROOT,
            [0.0, 22.9, -1.0],
            [0.0; 3],
        ),
        bone(&[jbox([-2.0, -0.3, -0.5], [2.0, 0.3, 1.0], SKIN, 14)], ROOT, [-2.0, 23.9, 2.5], [0.0; 3]),
        bone(&[jbox([0.0, -0.3, -0.5], [2.0, 0.3, 1.0], SKIN, 14)], ROOT, [2.0, 23.9, 2.5], [0.0; 3]),
        bone(&[jbox([-2.0, -0.3, -0.5], [2.0, 0.3, 1.0], SKIN, 14)], ROOT, [-2.0, 23.9, -0.5], [0.0; 3]),
        bone(&[jbox([0.0, -0.3, -0.5], [2.0, 0.3, 1.0], SKIN, 14)], ROOT, [2.0, 23.9, -0.5], [0.0; 3]),
    ];

    /// `TurtleModel` (a `QuadrupedModel`): on land the flippers sweep
    /// round at five times the walk position, in water they row.
    pub fn pose(m: &Mob) -> Parts {
        let mut r = Rig::new(BONES);
        let (rh, lh, rf, lf) = (2, 3, 4, 5);
        r.look(1, m);
        r.quadruped_walk(m, [rh, lh, rf, lf], motion::LEG_SWING);
        let (pos, speed) = (m.limb_phase, m.limb_amp);
        if !m.in_water && m.on_ground {
            let (front, hind) = ((pos * 5.0).cos(), (pos * 5.0).cos());
            r.y_rot(rf, -front * 8.0 * speed);
            r.y_rot(lf, front * 8.0 * speed);
            r.y_rot(rh, -hind * 3.0 * speed);
            r.y_rot(lh, hind * 3.0 * speed);
        } else {
            let swing = (pos * motion::WALK_FREQ * 0.6).cos() * 0.5 * speed;
            r.x_rot(rh, swing);
            r.x_rot(lh, -swing);
            r.z_rot(rf, -swing);
            r.z_rot(lf, swing);
        }
        r.parts()
    }
}

// ---------------------------------------------------------------- axolotl

#[allow(dead_code)]
pub(super) mod axolotl {
    use super::*;

    pub const BODY: usize = 1;
    pub const RIGHT_FRONT_LEG: usize = 2;
    pub const RIGHT_HIND_LEG: usize = 3;
    pub const LEFT_FRONT_LEG: usize = 5;
    pub const LEFT_HIND_LEG: usize = 6;
    pub const TAIL: usize = 7;
    pub const HEAD: usize = 8;
    pub const LEFT_GILLS: usize = 9;
    pub const RIGHT_GILLS: usize = 10;
    pub const TOP_GILLS: usize = 11;

    include!("keyframe_defs/axolotl_baby.rs");

    /// The body colours `aquatic`'s variants repaint, and the gills'.
    const SKIN: Rgb = [237, 160, 184];
    const FIN: Rgb = [243, 179, 198];
    const GILL: Rgb = super::super::aquatic::AXOLOTL_GILL;
    const LEG: &[Cuboid] = &[jbox([0.0, -0.15, -0.5], [3.0, 0.3, 1.0], [235, 151, 183], 12)];
    static BONES: &[Bone] = &[
        bone(&[], ROOT, [0.0, 24.0, 0.0], [0.0; 3]),
        bone(
            &[
                jbox([-2.0, -0.75, -2.75], [4.0, 2.0, 6.0], SKIN, 14),
                jbox([-0.15, -1.75, -2.75], [0.3, 1.0, 5.0], FIN, 10),
            ],
            0,
            [0.0, -1.25, 1.75],
            [0.0; 3],
        ),
        bone(&[jbox([-3.0, -0.15, -0.5], [3.0, 0.3, 1.0], [235, 151, 183], 12)], 1, [-2.0, 0.25, -1.25], [0.0; 3]),
        bone(&[], 1, [-2.0, 0.25, 1.75], [0.0, FRAC_PI_2, FRAC_PI_2]),
        bone(LEG, 3, [0.0; 3], [-FRAC_PI_2, 0.0, FRAC_PI_2]),
        bone(LEG, 1, [2.0, 0.25, -1.25], [0.0; 3]),
        bone(LEG, 1, [2.0, 0.25, 1.75], [0.0; 3]),
        bone(&[jbox([-0.15, -1.5, -1.0], [0.3, 3.0, 8.0], FIN, 14)], 1, [0.0, -0.25, 3.25], [0.0; 3]),
        bone(
            &[
                jbox([-3.0, -2.0, -4.0], [6.0, 3.0, 4.0], SKIN, 14),
                // Eyes with glints on the face's corners and a smile.
                face([-2.6, -1.6], [-1.4, -0.4], -4.0, 0.1, [22, 20, 29]),
                face([1.6, 2.6], [-1.4, -0.4], -4.0, 0.1, [22, 20, 29]),
                face([-2.35, -2.05], [-1.3, -1.0], -4.1, 0.05, [236, 230, 236]),
                face([2.05, 2.35], [-1.3, -1.0], -4.1, 0.05, [236, 230, 236]),
                face([-1.2, 1.2], [0.2, 0.45], -4.0, 0.05, [150, 92, 112]),
            ],
            1,
            [0.0, 0.25, -2.75],
            [0.0; 3],
        ),
        bone(&[jbox([0.0, -3.5, -0.15], [3.0, 5.0, 0.3], GILL, 0)], HEAD as u8, [3.0, -0.5, -2.0], [0.0; 3]),
        bone(&[jbox([-3.0, -3.5, -0.15], [3.0, 5.0, 0.3], GILL, 0)], HEAD as u8, [-3.0, -0.5, -2.0], [0.0; 3]),
        bone(&[jbox([-3.0, -3.0, -0.15], [6.0, 3.0, 0.3], GILL, 0)], HEAD as u8, [0.0, -2.0, -2.0], [0.0; 3]),
    ];

    /// `BabyAxolotlModel.setupAnim`: one `BabyAxolotlAnimation` at a time,
    /// as `Axolotl.setupAnimationStates` picks it (playing dead; moving:
    /// swimming, walking on the floor or under water; still: floating,
    /// resting on the floor under water or on land). The walk runs on the
    /// walk (`applyWalk(pos, speed, 15, 30)`) and, as Java's model writes
    /// it, so does the underwater walk's keyframes on the walk's state.
    pub fn pose(m: &Mob, alpha: f32) -> Parts {
        use crate::entity::mob_anim::AxolotlAnim as A;
        let mut r = Rig::new(BONES);
        let t = since(m, m.anim.axolotl_start, alpha).unwrap_or(0.0);
        match m.anim.axolotl {
            A::PlayDead => BABY_AXOLOTL_PLAY_DEAD.apply(&mut r, t),
            A::Swim => BABY_AXOLOTL_SWIM.apply(&mut r, t),
            A::Walk => {
                AXOLOTL_WALK_FLOOR.apply_walk(&mut r, m.limb_phase, m.limb_amp, 15.0, 30.0);
                WALK_FLOOR_UNDERWATER.apply(&mut r, t);
            }
            A::WalkUnderWater => {}
            A::IdleUnderWater => IDLE_UNDERWATER.apply(&mut r, t),
            A::IdleFloorUnderWater => IDLE_FLOOR_UNDERWATER.apply(&mut r, t),
            A::IdleFloor => BABY_AXOLOTL_IDLE_FLOOR.apply(&mut r, t),
        }
        r.parts()
    }
}

// ---------------------------------------------------------------- bee

pub(super) mod bee {
    use super::*;

    const BONE: usize = 0;
    const STINGER: usize = 2;
    const RIGHT_WING: usize = 3;
    const LEFT_WING: usize = 4;
    const FRONT_LEGS: usize = 5;
    const MIDDLE_LEGS: usize = 6;
    const BACK_LEGS: usize = 7;

    const YELLOW: Rgb = [225, 170, 40];
    const STRIPE: Rgb = [49, 37, 23];
    const WING: &[Cuboid] = &[jbox([-3.0, -0.1, 0.0], [3.0, 0.2, 3.0], [199, 229, 233], 4)];
    const LEGS: &[Cuboid] = &[jbox([-1.5, 0.0, -0.1], [3.0, 1.0, 0.2], [50, 34, 20], 0)];
    static BONES: &[Bone] = &[
        // Java's two 1 x 2 x 2 blocks on the front: the antennae stubs.
        bone(
            &[
                jbox([1.0, -1.6667, -2.1633], [1.0, 2.0, 2.0], [45, 33, 21], 0),
                jbox([-2.0, -1.6667, -2.1933], [1.0, 2.0, 2.0], [45, 33, 21], 0),
            ],
            ROOT,
            [0.0, 19.6667, -1.8567],
            [0.0; 3],
        ),
        bone(
            &[
                jbox([-2.0, -2.0, -2.5], [4.0, 4.0, 5.0], YELLOW, 20),
                jbox([-2.05, -2.05, -0.5], [4.1, 4.1, 1.0], STRIPE, 10),
                jbox([-2.05, -2.05, 1.3], [4.1, 4.1, 0.8], STRIPE, 10),
                // Big eyes with glints and a smile.
                face([-1.9, -0.6], [-1.2, 0.2], -2.5, 0.1, [27, 24, 25]),
                face([0.6, 1.9], [-1.2, 0.2], -2.5, 0.1, [27, 24, 25]),
                face([-1.6, -1.25], [-1.1, -0.75], -2.6, 0.05, [226, 222, 214]),
                face([1.25, 1.6], [-1.1, -0.75], -2.6, 0.05, [226, 222, 214]),
                face([-0.6, 0.6], [0.9, 1.15], -2.5, 0.05, [96, 62, 28]),
            ],
            BONE as u8,
            [0.0, 1.3333, 2.3567],
            [0.0; 3],
        ),
        bone(&[jbox([-0.15, -0.5, 0.0], [0.3, 1.0, 1.0], [33, 28, 24], 0)], 1, [0.0, 0.5, 2.5], [0.0; 3]),
        bone(WING, BONE as u8, [-1.0, -0.6667, 0.8567], [0.2182, 0.3491, 0.0]),
        bone(
            &[jbox([0.0, -0.1, 0.0], [3.0, 0.2, 3.0], [199, 229, 233], 4)],
            BONE as u8,
            [1.0, -0.6667, 0.8567],
            [0.2182, -0.3491, 0.0],
        ),
        bone(LEGS, BONE as u8, [0.0, 3.3333, 1.8567], [0.0; 3]),
        bone(LEGS, BONE as u8, [0.0, 3.3333, 2.8567], [0.0; 3]),
        bone(LEGS, BONE as u8, [0.0, 3.3333, 3.8567], [0.0; 3]),
    ];

    /// `BeeModel.setupAnim` on `BabyBeeModel`'s parts: flying, the wings
    /// beat `cos(ageInTicks * 120.32 degrees) * 0.15 pi` and the legs tuck;
    /// unless angry it bobs and pitches on `cos(ageInTicks * 0.18)`; an
    /// angry bee at its target tumbles by `rollAmount`; the stinger goes
    /// once used.
    pub fn pose(m: &Mob, time: f32, alpha: f32) -> Parts {
        let mut r = Rig::new(BONES);
        let age = age_ticks(time);
        let flying = !(m.on_ground && m.limb_amp < 1e-3);
        let angry = m.bee.is_some_and(|s| s.anger > 0);
        r.shown[STINGER] = !m.bee.is_some_and(|s| s.stung);
        if flying {
            r.y_rot(RIGHT_WING, 0.0);
            r.z_rot(RIGHT_WING, (age * 120.321_13_f32.to_radians()).cos() * PI * 0.15);
            r.rot[LEFT_WING] = [r.rot[RIGHT_WING][0], 0.0, -r.rot[RIGHT_WING][2]];
            for legs in [FRONT_LEGS, MIDDLE_LEGS, BACK_LEGS] {
                r.x_rot(legs, FRAC_PI_4);
            }
        }
        if !angry && flying {
            let speed = (age * 0.18).cos();
            r.x_rot(BONE, 0.1 + speed * PI * 0.025);
            r.at[BONE][1] -= (age * 0.18).cos() * 0.9;
            r.x_rot(FRONT_LEGS, -speed * PI * 0.1 + PI / 8.0);
            r.x_rot(BACK_LEGS, -speed * PI * 0.05 + FRAC_PI_4);
        }
        let roll = crate::entity::mob_anim::lerp(alpha, m.anim.o_roll, m.anim.roll);
        if roll > 0.0 {
            r.x_rot(BONE, rot_lerp(roll, r.rot[BONE][0], 3.091_592_8));
        }
        r.parts()
    }
}

// ---------------------------------------------------------------- squid

pub(super) mod squid {
    use super::*;

    const SKIN: Rgb = [49, 71, 80];
    const TENTACLE: &[Cuboid] = &[jbox([-1.0, -0.5, -1.0], [2.0, 6.0, 2.0], SKIN, 16)];
    macro_rules! tentacle {
        ($i:expr) => {{
            let a = $i as f32 * PI * 2.0 / 8.0;
            bone(TENTACLE, ROOT, [0.0, 18.5, 0.0], [0.0, -a + FRAC_PI_2, 0.0])
        }};
    }
    /// Tentacle `i` stands at `(cos(a) * 3, 18.5, sin(a) * 3)`, `a` an eighth
    /// of a turn each; the offsets are filled in at pose time.
    static BONES: &[Bone] = &[
        bone(
            &[
                jbox([-4.0, -5.0, -4.0], [8.0, 10.0, 8.0], SKIN, 22),
                // Eyes with dark pupils, the mantle's stripes and a beak.
                face([-3.0, -1.0], [1.0, 3.0], -4.0, 0.1, [232, 232, 232]),
                face([1.0, 3.0], [1.0, 3.0], -4.0, 0.1, [232, 232, 232]),
                face([-2.0, -1.0], [2.0, 3.0], -4.1, 0.1, [15, 20, 23]),
                face([1.0, 2.0], [2.0, 3.0], -4.1, 0.1, [15, 20, 23]),
                jbox([-4.05, -5.05, -0.5], [8.1, 5.0, 1.0], super::super::aquatic::SQUID_STRIPE, 0),
                face([-0.6, 0.6], [4.3, 4.9], -4.0, 0.05, [30, 34, 36]),
            ],
            ROOT,
            [0.0, 13.0, 0.0],
            [0.0; 3],
        ),
        tentacle!(0),
        tentacle!(1),
        tentacle!(2),
        tentacle!(3),
        tentacle!(4),
        tentacle!(5),
        tentacle!(6),
        tentacle!(7),
    ];

    /// `SquidModel`: every tentacle opens by `tentacleAngle`;
    /// `SquidRenderer.setupRotations` turns the baby about a quarter block
    /// up by `xBodyRot` then `zBodyRot`, the model set 0.6 blocks below.
    pub fn pose(m: &Mob, alpha: f32) -> Parts {
        use crate::entity::mob_anim::lerp;
        let mut r = Rig::new(BONES);
        let a = &m.anim;
        let open = lerp(alpha, a.o_tentacle_angle, a.tentacle_angle);
        for i in 0..8 {
            let angle = i as f32 * PI * 2.0 / 8.0;
            r.at[1 + i] = [angle.cos() * 3.0, 18.5, angle.sin() * 3.0];
            r.x_rot(1 + i, open);
        }
        let mut p = r.parts();
        let (x, z) = (lerp(alpha, a.o_x_body_rot, a.x_body_rot), lerp(alpha, a.o_z_body_rot, a.z_body_rot));
        let turn = Quat::from_rotation_x(-x.to_radians()) * Quat::from_rotation_y(z.to_radians());
        p.transform(Vec3::new(0.0, 4.0, 0.0) + turn * Vec3::new(0.0, -9.6, 0.0), turn);
        p
    }
}

// ---------------------------------------------------------------- dolphin

pub(super) mod dolphin {
    use super::*;

    const BODY: usize = 0;
    const TAIL: usize = 5;
    const TAIL_FIN: usize = 6;

    const GREY: Rgb = [128, 155, 165];
    const FIN: Rgb = [116, 146, 156];
    const FLIPPER: &[Cuboid] = &[jbox([-0.5, -1.5, -0.5], [1.0, 3.0, 6.0], FIN, 10)];
    static BONES: &[Bone] = &[
        bone(
            &[
                jbox([-3.0, -2.5, -4.0], [6.0, 5.0, 8.0], GREY, 14),
                // A paler belly.
                face([-2.5, 2.5], [0.5, 2.5], -4.0, 0.05, [186, 204, 208]),
            ],
            ROOT,
            [0.0, 21.5, 0.0],
            [0.0; 3],
        ),
        bone(
            &[
                jbox([-3.0, -3.5, -4.0], [6.0, 5.0, 4.0], [140, 164, 172], 12),
                // Eyes on the sides and the smile along the jaw.
                jbox([-3.05, -1.9, -3.0], [0.05, 0.8, 0.8], [20, 25, 29], 0),
                jbox([3.0, -1.9, -3.0], [0.05, 0.8, 0.8], [20, 25, 29], 0),
                face([-1.5, 1.5], [0.3, 0.55], -4.0, 0.05, [80, 100, 108]),
            ],
            BODY as u8,
            [0.0, 1.0, -4.0],
            [0.0; 3],
        ),
        bone(&[jbox([-1.0, -1.0, -2.0], [2.0, 2.0, 2.0], [150, 172, 178], 10)], 1, [0.0, 0.5, -4.0], [0.0; 3]),
        bone(FLIPPER, BODY as u8, [1.8, 0.85, -2.6], [0.8727, 0.0, 1.7017]),
        bone(FLIPPER, BODY as u8, [-1.8, 0.85, -2.6], [0.8727, 0.0, -1.7017]),
        bone(&[jbox([-2.0, -1.5, 0.0], [4.0, 3.0, 7.0], GREY, 12)], BODY as u8, [0.0, 1.0, 4.0], [0.0; 3]),
        bone(&[jbox([-4.0, -0.5, -1.0], [8.0, 1.0, 4.0], [124, 150, 160], 10)], TAIL as u8, [0.0, 0.0, 6.0], [0.0; 3]),
        bone(
            &[jbox([-0.5, -1.0, 1.0], [1.0, 3.0, 4.0], [110, 140, 152], 10)],
            BODY as u8,
            [0.0, -1.0, -2.7],
            [0.8727, 0.0, 0.0],
        ),
    ];

    /// `DolphinModel` on `BabyDolphinModel`'s parts: the body pitches with
    /// its swimming pitch and turns with its head; moving, it rocks on
    /// `cos(ageInTicks * 0.3)` and the tail and fluke beat.
    pub fn pose(m: &Mob, time: f32) -> Parts {
        let mut r = Rig::new(BONES);
        let age = age_ticks(time);
        r.x_rot(BODY, m.move_ctl.pitch.to_radians());
        r.y_rot(BODY, m.head_yaw);
        if m.limb_amp > 1e-3 {
            r.rot[BODY][0] += -0.05 - 0.05 * (age * 0.3).cos();
            r.x_rot(TAIL, -0.1 * (age * 0.3).cos());
            r.x_rot(TAIL_FIN, -0.2 * (age * 0.3).cos());
        }
        r.parts()
    }
}

// ---------------------------------------------------------------- armadillo

#[allow(dead_code)]
pub(super) mod armadillo {
    use super::*;

    pub const BODY: usize = 0;
    pub const TAIL: usize = 1;
    pub const HEAD: usize = 3;
    pub const RIGHT_EAR: usize = 5;
    pub const LEFT_EAR: usize = 6;
    pub const RIGHT_HIND_LEG: usize = 7;
    pub const LEFT_HIND_LEG: usize = 8;
    pub const RIGHT_FRONT_LEG: usize = 9;
    pub const LEFT_FRONT_LEG: usize = 10;
    pub const CUBE: usize = 11;

    include!("keyframe_defs/armadillo_baby.rs");

    const SHELL: Rgb = [181, 132, 117];
    const BAND: Rgb = [136, 94, 84];
    const SKIN: Rgb = [190, 140, 119];
    const LEG: &[Cuboid] = &[jbox([-1.0, 0.0, -1.0], [2.0, 2.0, 2.0], [152, 102, 89], 16)];
    const EAR: &[Cuboid] = &[jbox([-1.0, -2.0, -0.15], [2.0, 3.0, 0.3], [176, 126, 108], 16)];
    static BONES: &[Bone] = &[
        bone(
            &[
                jboxg([-2.5, -2.0, -3.5], [5.0, 4.0, 7.0], 0.3, SHELL, 20).with(Pattern::Mottle),
                jbox([-2.85, -2.35, -1.3], [5.7, 3.5, 0.5], BAND, 15),
                jbox([-2.85, -2.35, 0.8], [5.7, 3.5, 0.5], BAND, 15),
            ],
            ROOT,
            [0.0, 20.0, 0.5],
            [0.0; 3],
        ),
        bone(&[], 0, [0.0, 0.0, 3.4], [0.0; 3]),
        bone(
            &[jbox([-0.5, -0.5, -2.0], [1.0, 1.0, 4.0], [167, 113, 97], 18)],
            TAIL as u8,
            [0.0, 1.5, 1.0],
            [-FRAC_PI_3, 0.0, 0.0],
        ),
        bone(&[], 0, [0.0, 0.0, -3.2], [0.0; 3]),
        bone(
            &[
                jbox([-1.0, -2.0, -4.0], [2.0, 2.0, 4.0], SKIN, 18),
                jbox([-1.05, -1.6, -3.0], [0.05, 0.5, 0.5], [27, 24, 21], 0),
                jbox([1.0, -1.6, -3.0], [0.05, 0.5, 0.5], [27, 24, 21], 0),
                face([-0.5, 0.5], [-1.2, -0.6], -4.0, 0.05, [92, 60, 52]),
            ],
            HEAD as u8,
            [0.0; 3],
            [0.741_764_9, 0.0, 0.0],
        ),
        bone(EAR, 4, [-1.0, -2.0, -0.3], [-0.4363, -0.1134, 0.0524]),
        bone(EAR, 4, [1.0, -2.0, -0.3], [-0.4363, 0.1134, -0.0524]),
        bone(LEG, ROOT, [-1.5, 22.0, 2.5], [0.0; 3]),
        bone(LEG, ROOT, [1.5, 22.0, 2.5], [0.0; 3]),
        bone(LEG, ROOT, [1.5, 22.0, -1.5], [0.0; 3]),
        bone(LEG, ROOT, [-1.5, 22.0, -1.5], [0.0; 3]),
        bone(
            &[
                jboxg([-3.0, -3.0, -3.0], [6.0, 6.0, 6.0], 0.3, SHELL, 18).with(Pattern::Mottle),
                jbox([-3.35, -3.35, -0.4], [6.7, 6.7, 0.8], BAND, 12),
            ],
            ROOT,
            [0.0, 20.7, 0.5],
            [0.0; 3],
        ),
    ];

    /// `ArmadilloModel` on `BabyArmadilloModel`'s parts: walking, the head
    /// looks (yaw within 32.5 degrees, pitch -22.5 to 25) and the baby walk
    /// keyframes run (`applyWalk(pos, speed, 16.5, 2.5)`); rolled up, only
    /// the ball (`cube`) shows, as this game's adult does.
    pub fn pose(m: &Mob) -> Parts {
        let mut r = Rig::new(BONES);
        let hidden = m.armadillo.as_ref().is_some_and(|s| s.hide());
        for shown in &mut r.shown[..CUBE] {
            *shown = !hidden;
        }
        r.shown[CUBE] = hidden;
        if !hidden {
            r.x_rot(HEAD, (-m.head_pitch.to_degrees()).clamp(-22.5, 25.0).to_radians());
            r.y_rot(HEAD, m.head_yaw.to_degrees().clamp(-32.5, 32.5).to_radians());
            ARMADILLO_BABY_WALK.apply_walk(&mut r, m.limb_phase, m.limb_amp, 16.5, 2.5);
        }
        r.parts()
    }
}

// ---------------------------------------------------------------- piglins

pub(super) mod piglin {
    use super::*;

    const BODY: usize = 0;
    const LEFT_LEG: usize = 1;
    const RIGHT_LEG: usize = 2;
    const LEFT_ARM: usize = 3;
    const RIGHT_ARM: usize = 4;
    const HEAD: usize = 5;
    const LEFT_EAR: usize = 6;
    const RIGHT_EAR: usize = 8;

    const SKIN: Rgb = [232, 160, 150];
    const SNOUT: Rgb = [242, 178, 168];
    const ROT: Rgb = [116, 150, 80];
    const GOLD: Rgb = [250, 210, 60];

    /// `BabyPiglinModel` (also the zombified baby's) in one palette: skin,
    /// snout, clothes, the left ear's and the right arm's.
    macro_rules! baby_piglin {
        ($bones:ident, $skin:expr, $snout:expr, $cloth:expr, $left_ear:expr, $arm:expr, $sword:expr) => {
            pub static $bones: &[Bone] = &{
                const LEG: &[Cuboid] = &[
                    jbox([-1.5, 0.0, -1.5], [3.0, 4.0, 3.0], $skin, 36),
                    jbox([-1.55, 2.6, -1.55], [3.1, 1.4, 3.1], $cloth, 24),
                ];
                [
                    bone(
                        &[
                            jbox([-3.0, -3.0, -1.0], [6.0, 5.0, 3.0], $skin, 36),
                            jbox([-3.05, -3.05, -1.05], [6.1, 2.6, 3.1], $cloth, 28),
                            jbox([-3.1, 1.0, -1.1], [6.2, 1.0, 3.2], [84, 56, 32], 20),
                        ],
                        ROOT,
                        [0.0, 18.0, -0.5],
                        [0.0; 3],
                    ),
                    bone(LEG, ROOT, [1.5, 20.0, 0.0], [0.0; 3]),
                    bone(LEG, ROOT, [-1.5, 20.0, 0.0], [0.0; 3]),
                    bone(&[jbox([-1.0, 0.0, -1.5], [2.0, 5.0, 3.0], $skin, 36)], ROOT, [4.0, 15.0, 0.0], [0.0; 3]),
                    bone($arm, ROOT, [-4.0, 15.0, 0.0], [0.0; 3]),
                    bone(
                        &[
                            jbox([-4.5, -6.0, -3.5], [9.0, 6.0, 7.0], $skin, 36),
                            jbox([-1.5, -3.0, -4.5], [3.0, 3.0, 1.0], $snout, 14),
                            face([-1.2, -0.4], [-2.2, -1.2], -4.5, 0.05, DARK),
                            face([0.4, 1.2], [-2.2, -1.2], -4.5, 0.05, DARK),
                            // Big eyes (one white, one dark, as the adult's)
                            // and small tusks.
                            face([-3.6, -2.0], [-4.4, -3.2], -3.5, 0.1, WHITE),
                            face([2.0, 3.6], [-4.4, -3.2], -3.5, 0.1, DARK),
                            face([-2.6, -2.0], [-4.4, -3.6], -3.6, 0.05, DARK),
                            face([-2.6, -2.1], [-1.6, -0.2], -3.5, 0.3, [226, 214, 186]),
                            face([2.1, 2.6], [-1.6, -0.2], -3.5, 0.3, [226, 214, 186]),
                        ],
                        ROOT,
                        [0.0, 15.0, 0.0],
                        [0.0; 3],
                    ),
                    bone(&[], HEAD as u8, [4.2, -4.0, 0.0], [0.0; 3]),
                    bone(
                        &[jbox([-0.5, -3.0, -2.0], [1.0, 6.0, 4.0], $left_ear, 36)],
                        LEFT_EAR as u8,
                        [1.0, 1.75, 0.0],
                        [0.0, 0.0, -0.6109],
                    ),
                    bone(&[], HEAD as u8, [-4.2, -4.0, 0.0], [0.0; 3]),
                    bone(
                        &[jbox([-0.5, -3.0, -2.0], [1.0, 6.0, 4.0], $skin, 36)],
                        RIGHT_EAR as u8,
                        [-1.0, 1.75, 0.0],
                        [0.0, 0.0, 0.6109],
                    ),
                    bone($sword, RIGHT_ARM as u8, [0.0; 3], [0.0; 3]),
                ]
            };
        };
    }
    const ARM: &[Cuboid] = &[jbox([-1.0, 0.0, -1.5], [2.0, 5.0, 3.0], SKIN, 36)];
    const ZOMBIE_ARM: &[Cuboid] = &[jbox([-1.0, 0.0, -1.5], [2.0, 5.0, 3.0], ROT, 40)];
    /// A golden sword in the zombified baby's right fist, pointing forward.
    const SWORD: &[Cuboid] = &[
        jbox([-0.5, 4.0, -2.5], [1.0, 1.0, 2.0], [100, 70, 30], 10),
        jbox([-1.5, 4.0, -3.0], [3.0, 1.0, 0.5], GOLD, 10),
        jbox([-0.5, 4.0, -8.5], [1.0, 1.0, 5.5], GOLD, 10),
    ];
    baby_piglin!(PIGLIN, SKIN, SNOUT, [118, 80, 44], SKIN, ARM, &[]);
    baby_piglin!(ZOMBIFIED, [226, 150, 140], [236, 168, 160], [112, 78, 46], ROT, ZOMBIE_ARM, SWORD);
    baby_piglin!(ZOMBIFIED_UNARMED, [226, 150, 140], [236, 168, 160], [112, 78, 46], ROT, ZOMBIE_ARM, &[]);

    /// The bones' turns from a humanoid's limbs (Java's convention), the
    /// arms' pivots carried round by the body's twist as
    /// `HumanoidModel.setupAttackAnimation` carries them.
    fn humanoid(r: &mut Rig, h: &Humanoid) {
        r.y_rot(BODY, h.body_y);
        r.x_rot(LEFT_LEG, h.left_leg);
        r.x_rot(RIGHT_LEG, h.right_leg);
        r.rot[LEFT_ARM] = [h.left_arm.x, h.left_arm.y, h.left_arm.z];
        r.rot[RIGHT_ARM] = [h.right_arm.x, h.right_arm.y, h.right_arm.z];
        if h.body_y != 0.0 {
            let (s, c) = h.body_y.sin_cos();
            (r.at[RIGHT_ARM][0], r.at[RIGHT_ARM][2]) = (-c * 4.0, s * 4.0);
            (r.at[LEFT_ARM][0], r.at[LEFT_ARM][2]) = (c * 4.0, -s * 4.0);
        }
    }

    /// `PiglinModel` and `ZombifiedPiglinModel` on `BabyPiglinModel`'s
    /// parts, with the babies' 5-degree ears flopping with the walk and an
    /// idle drift (`AbstractPiglinModel`). A piglin cub walks, swings,
    /// dances and admires gold (`ADMIRING_ITEM`: head 0.5 down and level,
    /// the off hand raised); a zombified cub raises its arms
    /// (`animateZombieArms`) with its golden sword.
    pub fn pose(m: &Mob, time: f32) -> Parts {
        let age = age_ticks(time);
        let zombified = m.kind == MobKind::ZombifiedPiglin;
        let armed = m.held.unwrap_or(crate::entity::nether::Weapon::GoldenSword) != crate::entity::nether::Weapon::None;
        let mut r = Rig::new(match (zombified, armed) {
            (false, _) => PIGLIN,
            (true, true) => ZOMBIFIED,
            (true, false) => ZOMBIFIED_UNARMED,
        });
        r.look(HEAD, m);
        let ear = 5f32.to_radians();
        let freq = age * 0.1 + m.limb_phase * 0.5;
        let amp = 0.08 + m.limb_amp * 0.4;
        let mut left_ear = -ear - (freq * 1.2).cos() * amp;
        let mut right_ear = ear + freq.cos() * amp;
        if zombified {
            let h = zombie_limbs(m, time, !armed);
            humanoid(&mut r, &h);
        } else {
            let n = m.nether.as_deref();
            let mut h = Humanoid::walk(m, 1.0);
            h.attack(m.swing.anim, head_x(m));
            h.bob(age);
            if n.is_some_and(|n| n.dancing) {
                let d = age / 60.0;
                right_ear = PI / 6.0 + (d * 30.0).sin() * 10f32.to_radians();
                left_ear = -PI / 6.0 - (d * 30.0).cos() * 10f32.to_radians();
                r.at[HEAD][0] += (d * 10.0).sin();
                r.at[HEAD][1] += (d * 40.0).sin() + 0.4;
                h.right_arm.z = (70.0 + (d * 40.0).cos() * 10.0).to_radians();
                h.left_arm.z = -h.right_arm.z;
                r.at[RIGHT_ARM][1] += (d * 40.0).sin() * 0.5 - 0.5;
                r.at[LEFT_ARM][1] += (d * 40.0).sin() * 0.5 + 0.5;
                r.at[BODY][1] += (d * 40.0).sin() * 0.35;
            } else if n.is_some_and(|n| n.is_admiring()) {
                r.rot[HEAD] = [0.5, 0.0, 0.0];
                h.left_arm.y = 0.5;
                h.left_arm.x = -0.9;
            }
            humanoid(&mut r, &h);
        }
        r.z_rot(LEFT_EAR, left_ear);
        r.z_rot(RIGHT_EAR, right_ear);
        r.parts()
    }
}

// ---------------------------------------------------------------- strider

pub(super) mod strider {
    use super::*;

    const BODY: usize = 0;
    const RIGHT_LEG: usize = 1;
    const LEFT_LEG: usize = 2;

    macro_rules! baby_strider {
        ($bones:ident, $skin:expr, $dark:expr) => {
            pub static $bones: &[Bone] = &{
                const LEG: &[Cuboid] = &[jbox([-1.0, 0.0, -1.0], [2.0, 4.0, 2.0], $skin, 30)];
                const BRISTLE: &[Cuboid] = &[jbox([-3.5, -2.5, -0.15], [7.0, 3.0, 0.3], $dark, 10)];
                [
                    bone(
                        &[
                            jbox([-3.5, -3.75, -4.0], [7.0, 7.0, 8.0], $skin, 30),
                            // Eyes with glints and a wide mouth.
                            face([-2.6, -1.0], [-1.6, -0.6], -4.0, 0.1, DARK),
                            face([1.0, 2.6], [-1.6, -0.6], -4.0, 0.1, DARK),
                            face([-2.2, -1.7], [-1.6, -1.1], -4.1, 0.05, [236, 226, 214]),
                            face([1.7, 2.2], [-1.6, -1.1], -4.1, 0.05, [236, 226, 214]),
                            face([-2.5, 2.5], [0.8, 1.3], -4.0, 0.05, $dark),
                        ],
                        ROOT,
                        [0.0, 16.75, 0.0],
                        [0.0; 3],
                    ),
                    bone(LEG, ROOT, [-1.5, 20.0, 0.0], [0.0; 3]),
                    bone(LEG, ROOT, [1.5, 20.0, 0.0], [0.0; 3]),
                    bone(BRISTLE, BODY as u8, [0.0, -4.25, 2.0], [0.0; 3]),
                    bone(BRISTLE, BODY as u8, [0.0, -4.25, 0.0], [0.0; 3]),
                    bone(BRISTLE, BODY as u8, [0.0, -4.25, -2.0], [0.0; 3]),
                ]
            };
        };
    }
    baby_strider!(WARM, [178, 54, 58], [118, 32, 38]);
    baby_strider!(COLD, [126, 98, 126], [82, 60, 86]);

    /// `StriderModel` with `BabyStriderModel.customAnimations`: the body
    /// looks (unless ridden), rocks and bobs at 1.5 times the walk position
    /// and the legs swing, splay and lift at half that (speed capped at
    /// 0.25); the bristles flow with the walk and drift.
    pub fn pose(m: &Mob, time: f32) -> Parts {
        let cold = m.nether.as_ref().is_some_and(|n| n.cold);
        let mut r = Rig::new(if cold { COLD } else { WARM });
        let (pos, speed) = (m.limb_phase, m.limb_amp.min(0.25));
        let age = age_ticks(time);
        if !m.mount.as_ref().is_some_and(|s| s.rider.is_some()) {
            r.look(BODY, m);
        }
        r.z_rot(BODY, 0.1 * (pos * 1.5).sin() * 4.0 * speed);
        r.x_rot(LEFT_LEG, (pos * 0.75).sin() * 2.0 * speed);
        r.x_rot(RIGHT_LEG, (pos * 0.75 + PI).sin() * 2.0 * speed);
        r.z_rot(LEFT_LEG, PI / 18.0 * (pos * 0.75).cos() * speed);
        r.z_rot(RIGHT_LEG, PI / 18.0 * (pos * 0.75 + PI).cos() * speed);
        r.at[BODY][1] = 17.25 - (pos * 1.5).cos() * 2.0 * speed;
        r.at[LEFT_LEG][1] = 20.0 + 2.0 * (pos * 0.75 + PI).sin() * 2.0 * speed;
        r.at[RIGHT_LEG][1] = 20.0 + 2.0 * (pos * 0.75).sin() * 2.0 * speed;
        let flow = (pos * 1.5 + PI).cos() * speed;
        r.rot[3][0] += flow * 0.6 + 0.1 * (age * 0.4).sin();
        r.rot[4][0] += flow * 1.2 + 0.1 * (age * 0.2).sin();
        r.rot[5][0] += flow * 1.3 + 0.05 * (age * -0.4).sin();
        r.parts()
    }
}

// ---------------------------------------------------------------- zombie villager

pub(super) mod zombie_villager {
    use super::*;

    const BODY: usize = 0;
    const LEFT_LEG: usize = 1;
    const RIGHT_LEG: usize = 2;
    const LEFT_ARM: usize = 3;
    const RIGHT_ARM: usize = 4;
    const HEAD: usize = 5;

    const SKIN: Rgb = [92, 148, 78];
    const ROBE: Rgb = [114, 78, 51];
    const ARM: &[Cuboid] = &[
        jbox([-1.0, -0.5, -1.0], [2.0, 2.5, 2.0], ROBE, 30).with(Pattern::Cloth),
        jbox([-1.0, 2.0, -1.0], [2.0, 2.5, 2.0], SKIN, 24).with(Pattern::Rot),
    ];
    const LEG: &[Cuboid] = &[jbox([-1.0, -0.5, -1.0], [2.0, 3.0, 2.0], [68, 49, 36], 28).with(Pattern::Cloth)];
    static BONES: &[Bone] = &[
        bone(
            &[
                jbox([-2.0, -2.75, -1.5], [4.0, 5.0, 3.0], ROBE, 30).with(Pattern::Cloth),
                jboxg([-2.0, -2.75, -1.5], [4.0, 6.0, 3.0], 0.1, ROBE, 30).with(Pattern::Cloth),
                jbox([-2.15, 1.0, -1.65], [4.3, 0.5, 3.3], [66, 44, 28], 10),
            ],
            ROOT,
            [0.0, 18.75, 0.0],
            [0.0; 3],
        ),
        bone(LEG, ROOT, [1.0, 21.5, 0.0], [0.0; 3]),
        bone(LEG, ROOT, [-1.0, 21.5, 0.0], [0.0; 3]),
        bone(ARM, ROOT, [3.0, 15.5, 0.0], [0.0; 3]),
        bone(ARM, ROOT, [-3.0, 15.5, 0.0], [0.0; 3]),
        bone(
            &[
                jbox([-4.0, -8.0, -3.5], [8.0, 8.0, 7.0], SKIN, 24).with(Pattern::Rot),
                // Red eyes under a dark brow and a slack mouth.
                face([-3.0, -1.0], [-4.5, -3.5], -3.5, 0.1, [160, 40, 36]),
                face([1.0, 3.0], [-4.5, -3.5], -3.5, 0.1, [160, 40, 36]),
                face([-3.0, 3.0], [-5.6, -5.0], -3.5, 0.1, [40, 28, 22]),
                face([-1.5, 1.5], [-1.2, -0.8], -3.5, 0.05, [40, 28, 22]),
            ],
            ROOT,
            [0.0, 16.0, 0.0],
            [0.0; 3],
        ),
        bone(&[jbox([-1.0, -1.0, -0.5], [2.0, 2.0, 1.0], [70, 120, 58], 22)], HEAD as u8, [0.0, -1.0, -4.0], [0.0; 3]),
    ];

    /// `ZombieVillagerModel` on `BabyZombieVillagerModel`'s parts: a
    /// humanoid's walk with `animateZombieArms` (raised, higher while
    /// chasing) and the idle bob; it shakes while it is cured.
    pub fn pose(m: &Mob, time: f32) -> Parts {
        let mut r = Rig::new(BONES);
        r.look(HEAD, m);
        let h = zombie_limbs(m, time, true);
        r.y_rot(BODY, h.body_y);
        r.x_rot(LEFT_LEG, h.left_leg);
        r.x_rot(RIGHT_LEG, h.right_leg);
        r.rot[LEFT_ARM] = [h.left_arm.x, h.left_arm.y, h.left_arm.z];
        r.rot[RIGHT_ARM] = [h.right_arm.x, h.right_arm.y, h.right_arm.z];
        let mut p = r.parts();
        if m.convert_left > 0.0 {
            p.transform(Vec3::new((time * 18.0).sin() * 1.5, 0.0, 0.0), Quat::IDENTITY);
        }
        p
    }
}

// ---------------------------------------------------------------- snifflet

pub(super) mod snifflet {
    use super::*;

    const HEAD: usize = 2;

    const HIDE: Rgb = [124, 63, 43];
    const MOSS: Rgb = [70, 108, 42];
    const LEG: &[Cuboid] = &[jbox([-2.0, -1.0, -2.0], [4.0, 5.0, 4.0], [75, 49, 34], 20)];
    /// `SnifferAnimation.BABY_TRANSFORM`'s 1.2 head scale, baked into the
    /// head's boxes and its children's offsets.
    const K: f32 = 1.2;
    static BONES: &[Bone] = &[
        bone(&[], ROOT, [0.0, 24.0, 0.0], [0.0; 3]),
        bone(
            &[
                jbox([-13.0, -14.0, -0.5], [14.0, 15.0, 20.0], HIDE, 30),
                jboxg([-13.0, -14.0, -0.5], [14.0, 4.0, 20.0], 0.25, MOSS, 40),
                jbox([-9.0, -14.6, 5.0], [1.0, 0.6, 1.0], [214, 96, 150], 0),
                jbox([-3.0, -14.6, 12.0], [1.0, 0.6, 1.0], [236, 206, 70], 0),
            ],
            0,
            [6.0, -3.0, -9.5],
            [0.0; 3],
        ),
        bone(
            &[
                jbox([-5.0 * K, -4.25 * K, -7.5 * K], [10.0 * K, 9.0 * K, 9.0 * K], [133, 72, 48], 30),
                jbox([-5.1 * K, -2.0 * K, -5.5 * K], [0.1, 1.2 * K, 1.4 * K], EYE, 0),
                jbox([5.0 * K, -2.0 * K, -5.5 * K], [0.1, 1.2 * K, 1.4 * K], EYE, 0),
                jbox([-5.15 * K, -1.9 * K, -5.4 * K], [0.05, 0.4, 0.4], GLINT, 0),
                jbox([5.1 * K, -1.9 * K, -5.4 * K], [0.05, 0.4, 0.4], GLINT, 0),
            ],
            1,
            [-6.0, -4.75, 0.0],
            [0.0; 3],
        ),
        bone(
            &[jbox([0.0, 0.0, -2.0 * K], [1.0 * K, 11.0 * K, 3.0 * K], [133, 72, 48], 26)],
            HEAD as u8,
            [5.0 * K, -4.25 * K, -1.5 * K],
            [0.0; 3],
        ),
        bone(
            &[jbox([-K, 0.0, -2.0 * K], [1.0 * K, 11.0 * K, 3.0 * K], [133, 72, 48], 26)],
            HEAD as u8,
            [-5.0 * K, -4.25 * K, -1.5 * K],
            [0.0; 3],
        ),
        bone(
            &[
                jbox([-5.0 * K, -3.0 * K, -2.0 * K], [10.0 * K, 3.0 * K, 4.0 * K], [204, 171, 75], 30),
                face([-4.0 * K, -1.5 * K], [-2.2 * K, -K], -2.0 * K, 0.1, [65, 49, 33]),
                face([1.5 * K, 4.0 * K], [-2.2 * K, -K], -2.0 * K, 0.1, [65, 49, 33]),
            ],
            HEAD as u8,
            [0.0, -1.25 * K, -9.5 * K],
            [0.0; 3],
        ),
        bone(
            &[jbox([-5.0 * K, -2.5 * K, -2.0 * K], [10.0 * K, 5.0 * K, 4.0 * K], [190, 156, 66], 30)],
            HEAD as u8,
            [0.0, 1.25 * K, -9.5 * K],
            [0.0; 3],
        ),
        bone(LEG, 0, [-4.0, -4.0, -7.0], [0.0; 3]),
        bone(LEG, 0, [-4.0, -4.0, 0.0], [0.0; 3]),
        bone(LEG, 0, [-4.0, -4.0, 7.0], [0.0; 3]),
        bone(LEG, 0, [4.0, -4.0, -7.0], [0.0; 3]),
        bone(LEG, 0, [4.0, -4.0, 0.0], [0.0; 3]),
        bone(LEG, 0, [4.0, -4.0, 7.0], [0.0; 3]),
    ];

    /// `SnifferModel` on `SniffletModel`'s parts with `BABY_TRANSFORM` (the
    /// head 1.2 times larger, a pixel up and a pixel back): the head looks;
    /// the walk and the digging, sniffing and rising poses move it as this
    /// game's adult sniffer does (its keyframe animations aren't on these
    /// parts yet).
    pub fn pose(m: &Mob, time: f32) -> Parts {
        use crate::entity::sniffer as s;
        let mut r = Rig::new(BONES);
        r.at[HEAD][1] -= 1.0;
        r.at[HEAD][2] += 1.0;
        let state = m.sniffer.as_ref().map_or(0, |s| s.pose);
        let (phase, scale) = motion::keyframe_phase(m.limb_phase, m.limb_amp, motion::SNIFFER_WALK);
        let swing = phase.sin() * 0.6 * scale;
        for (i, k) in [1.0, -1.0, 1.0, -1.0, 1.0, -1.0].into_iter().enumerate() {
            r.x_rot(7 + i, swing * if i % 3 == 1 { -k } else { k });
        }
        let lower = match state {
            s::DIGGING => 3.0,
            s::RISING => 3.0 * (1.0 - m.sniffer.as_ref().map_or(40.0, |s| s.elapsed as f32) / 40.0).clamp(0.0, 1.0),
            _ => 0.0,
        };
        r.at[1][1] += lower;
        r.y_rot(HEAD, m.head_yaw);
        r.x_rot(
            HEAD,
            match state {
                s::DIGGING => 0.4 + (time * 9.0).sin() * 0.1,
                s::SNIFFING => 0.25 + (time * 5.0).sin() * 0.08,
                s::SCENTING => -0.3,
                s::HAPPY => (time * 4.0).sin() * 0.15,
                _ => head_x(m),
            },
        );
        r.parts()
    }
}

// ---------------------------------------------------------------- nautilus

pub(super) mod nautilus {
    use super::*;

    pub const BODY: usize = 2;
    pub const UPPER_MOUTH: usize = 3;
    pub const INNER_MOUTH: usize = 4;
    pub const LOWER_MOUTH: usize = 5;

    include!("keyframe_defs/nautilus.rs");

    const SHELL: Rgb = [222, 204, 172];
    const STRIPE: Rgb = [166, 86, 52];
    const SOFT: Rgb = [196, 132, 116];
    static BONES: &[Bone] = &[
        bone(&[], ROOT, [-0.5, 28.0, -0.5], [0.0; 3]),
        bone(
            &[
                jbox([-6.0, -4.0, -1.0], [7.0, 4.0, 7.0], SHELL, 22).with(Pattern::Mottle),
                jbox([-6.0, 0.0, -1.0], [7.0, 4.0, 9.0], SHELL, 22).with(Pattern::Mottle),
                jbox([-6.1, -3.0, 0.5], [7.2, 0.8, 5.0], STRIPE, 10),
                jbox([-6.1, 1.2, 0.5], [7.2, 0.8, 6.5], STRIPE, 10),
            ],
            0,
            [3.0, -8.0, -2.0],
            [0.0; 3],
        ),
        bone(
            &[
                jbox([-2.5, -3.01, -1.0], [5.0, 4.0, 7.0], SOFT, 18),
                jbox([-2.6, -2.2, 3.0], [0.1, 1.0, 1.0], [24, 24, 30], 0),
                jbox([2.5, -2.2, 3.0], [0.1, 1.0, 1.0], [24, 24, 30], 0),
            ],
            0,
            [0.5, -5.0, 3.0],
            [0.0; 3],
        ),
        bone(&[jbox([-2.5, -1.0, 0.0], [5.0, 2.0, 2.0], SOFT, 16)], BODY as u8, [0.0, -2.01, 3.9], [0.0; 3]),
        bone(&[jbox([-1.5, -1.0, -1.0], [3.0, 2.0, 2.0], [150, 92, 84], 10)], BODY as u8, [0.0, -1.01, 4.9], [0.0; 3]),
        bone(&[jbox([-2.5, -1.0, 0.0], [5.0, 2.0, 2.0], SOFT, 16)], BODY as u8, [0.0, -0.01, 3.9], [0.0; 3]),
    ];

    /// `NautilusModel` on its baby mesh: the body turns toward the look
    /// (within 10 degrees each way) and the swim keyframes run on the walk
    /// position plus a fifth of `ageInTicks` (`applyWalk(.., speed + 0.2,
    /// 2, 3)`). As in Java, the mouth is at the back: a nautilus swims
    /// shell first.
    pub fn pose(m: &Mob, time: f32) -> Parts {
        let mut r = Rig::new(BONES);
        r.y_rot(BODY, m.head_yaw.to_degrees().clamp(-10.0, 10.0).to_radians());
        r.x_rot(BODY, (-m.head_pitch.to_degrees()).clamp(-10.0, 10.0).to_radians());
        SWIMMING.apply_walk(&mut r, m.limb_phase + age_ticks(time) / 5.0, m.limb_amp + 0.2, 2.0, 3.0);
        r.parts()
    }
}

/// The foals' coats and manes, from the adults' colours.
pub(super) fn colour(m: &Mob, c: &mut Cuboid) {
    if c.color != COAT && c.color != MANE {
        return;
    }
    let index = match m.kind {
        MobKind::SkeletonHorse => 7,
        MobKind::ZombieHorse => 8,
        MobKind::Horse => m.mount.as_ref().map_or(0, |s| s.variant as usize),
        _ => 5,
    };
    let coat = super::mounts::COLORS[index % super::mounts::COLORS.len()];
    c.color = if c.color == MANE { coat.map(|v| (f32::from(v) * 0.55) as u8) } else { coat };
}
