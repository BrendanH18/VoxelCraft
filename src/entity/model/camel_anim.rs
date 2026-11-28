//! v1.0 gap Track 3b: Java 26.3's camel keyframe animations
//! (`CamelAnimation.CAMEL_SIT`, `CAMEL_SIT_POSE`, `CAMEL_STANDUP` and
//! `CAMEL_DASH`), sampled as `KeyframeAnimation.apply` does: the keyframe
//! before the time (the first one at or after it, less one), the next one,
//! and the next one's interpolation (`LINEAR` or `CATMULLROM`); a
//! non-looping animation past its end holds its last keyframe.
//!
//! Values stay in Java's units as `CamelAnimation` writes them: rotations
//! in degrees (`degreeVec`) and positions in pixels (`posVec`, y up). The
//! camel model ([`super::mounts::camel`]) turns them into its own axes.
//! The idle animation (ears and tail) and the walk stay with the model's
//! own motion; `CamelBabyAnimation`'s calf keyframes aren't separate (a
//! calf draws the adult model scaled down).

/// The camel model's animated parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bone {
    Body,
    Head,
    Tail,
    LeftFrontLeg,
    RightFrontLeg,
    LeftHindLeg,
    RightHindLeg,
}

const BONES: usize = 7;

#[derive(Clone, Copy)]
enum Lerp {
    Linear,
    Catmull,
}

#[derive(Clone, Copy)]
struct Key {
    t: f32,
    v: [f32; 3],
    lerp: Lerp,
}

const fn l(t: f32, v: [f32; 3]) -> Key {
    Key { t, v, lerp: Lerp::Linear }
}
const fn c(t: f32, v: [f32; 3]) -> Key {
    Key { t, v, lerp: Lerp::Catmull }
}

/// One `AnimationChannel`: a bone's rotation (`ROTATION`, degrees) or
/// position (`POSITION`, pixels).
struct Channel {
    bone: Bone,
    position: bool,
    keys: &'static [Key],
}

const fn rot(bone: Bone, keys: &'static [Key]) -> Channel {
    Channel { bone, position: false, keys }
}
const fn pos(bone: Bone, keys: &'static [Key]) -> Channel {
    Channel { bone, position: true, keys }
}

/// An `AnimationDefinition`.
pub struct Anim {
    length: f32,
    looping: bool,
    channels: &'static [Channel],
}

use Bone::*;

const Z: [f32; 3] = [0.0; 3];

/// `CAMEL_SIT`: two seconds, front knees first, then the hind legs.
pub const SIT: Anim = Anim {
    length: 2.0,
    looping: false,
    channels: &[
        rot(Body, &[l(0.0, Z), l(1.3, [30.0, 0.0, 0.0]), l(1.8, [24.0, 0.0, 0.0]), l(2.0, Z)]),
        pos(Body, &[l(0.0, Z), l(1.3, [0.0, 0.0, 1.0]), l(1.8, [0.0, -6.0, 1.0]), l(2.0, [0.0, -19.9, 0.0])]),
        rot(
            RightFrontLeg,
            &[l(0.0, Z), l(1.0, [-30.0, 0.0, 0.0]), l(1.5, [-30.0, 0.0, 0.0]), l(2.0, [-90.0, 10.0, 0.0])],
        ),
        pos(
            RightFrontLeg,
            &[
                l(0.0, Z),
                l(1.0, [0.0, -2.0, 11.0]),
                l(1.5, [0.0, -2.0, 11.0]),
                l(1.7, [0.0, -8.4, 11.4]),
                l(2.0, [0.0, -20.6, 12.0]),
            ],
        ),
        rot(
            LeftFrontLeg,
            &[l(0.0, Z), l(1.0, [-30.0, 0.0, 0.0]), l(1.5, [-30.0, 0.0, 0.0]), l(2.0, [-90.0, -10.0, 0.0])],
        ),
        pos(
            LeftFrontLeg,
            &[
                l(0.0, Z),
                l(1.0, [0.0, -2.0, 11.0]),
                l(1.5, [0.0, -2.0, 11.0]),
                l(1.7, [0.0, -8.4, 11.4]),
                l(2.0, [0.0, -20.6, 12.0]),
            ],
        ),
        rot(
            LeftHindLeg,
            &[
                l(0.0, Z),
                l(0.5, Z),
                l(1.5, [-10.0, 0.0, 0.0]),
                l(1.7, [-15.0, -3.0, 0.0]),
                l(1.9, [-65.0, -9.0, 0.0]),
                l(2.0, [-90.0, -15.0, 0.0]),
            ],
        ),
        pos(
            LeftHindLeg,
            &[
                l(0.0, Z),
                l(0.5, Z),
                l(1.5, [0.0, 0.0, 1.0]),
                l(1.7, [1.0, -0.62, 0.25]),
                l(1.9, [0.5, -11.25, 2.5]),
                l(2.0, [1.0, -20.5, 5.0]),
            ],
        ),
        rot(
            RightHindLeg,
            &[
                l(0.0, Z),
                l(0.5, Z),
                l(1.5, [-10.0, 0.0, 0.0]),
                l(1.7, [-15.0, 3.0, 0.0]),
                l(1.9, [-65.0, 9.0, 0.0]),
                l(2.0, [-90.0, 15.0, 0.0]),
            ],
        ),
        pos(
            RightHindLeg,
            &[
                l(0.0, Z),
                l(0.5, Z),
                l(1.5, [0.0, 0.0, 1.0]),
                l(1.7, [-1.0, -0.62, 0.25]),
                l(1.9, [-0.5, -11.25, 2.5]),
                l(2.0, [-1.0, -20.5, 5.0]),
            ],
        ),
        rot(Head, &[l(0.0, Z), l(0.7, [-27.5, 0.0, 0.0]), l(1.5, [-21.25, 0.0, 0.0]), l(2.0, Z)]),
        rot(
            Tail,
            &[l(0.0, [5.0, 0.0, 0.0]), l(1.7, [5.0, 0.0, 0.0]), l(1.9, [80.0, 0.0, 0.0]), l(2.0, [50.0, 0.0, 0.0])],
        ),
    ],
};

/// `CAMEL_SIT_POSE`: the seated pose, held.
pub const SIT_POSE: Anim = Anim {
    length: 1.0,
    looping: true,
    channels: &[
        pos(Body, &[l(0.0, [0.0, -19.9, 0.0])]),
        rot(RightFrontLeg, &[l(0.0, [-90.0, 10.0, 0.0])]),
        pos(RightFrontLeg, &[l(0.0, [0.0, -20.6, 12.0])]),
        rot(LeftFrontLeg, &[l(0.0, [-90.0, -10.0, 0.0])]),
        pos(LeftFrontLeg, &[l(0.0, [0.0, -20.6, 12.0])]),
        rot(LeftHindLeg, &[l(0.0, [-90.0, -15.0, 0.0])]),
        pos(LeftHindLeg, &[l(0.0, [1.0, -20.5, 5.0])]),
        rot(RightHindLeg, &[l(0.0, [-90.0, 15.0, 0.0])]),
        pos(RightHindLeg, &[l(0.0, [-1.0, -20.5, 5.0])]),
        rot(Tail, &[l(0.0, [50.0, 0.0, 0.0])]),
    ],
};

/// `CAMEL_STANDUP`: 2.6 seconds, hind legs first, the head thrown forward.
pub const STAND_UP: Anim = Anim {
    length: 2.6,
    looping: false,
    channels: &[
        rot(
            Body,
            &[l(0.0, Z), c(0.7, [-17.5, 0.0, 0.0]), c(1.8, [-17.83, 0.0, 0.0]), l(2.3, [-5.83, 0.0, 0.0]), l(2.6, Z)],
        ),
        pos(
            Body,
            &[
                l(0.0, [0.0, -19.9, 0.0]),
                l(0.7, [0.0, -19.9, -3.0]),
                c(1.4, [0.0, -12.76, -4.0]),
                c(1.8, [0.0, -10.1, -4.0]),
                l(2.3, [0.0, -2.9, -2.0]),
                l(2.6, Z),
            ],
        ),
        rot(
            RightFrontLeg,
            &[
                l(0.0, [-90.0, 10.0, 0.0]),
                l(0.5, [-90.0, 10.0, 0.0]),
                l(1.1, [-49.06, 10.0, 0.0]),
                l(1.8, [-22.5, 10.0, 0.0]),
                l(2.3, [-25.0, 10.0, 0.0]),
                l(2.6, Z),
            ],
        ),
        pos(
            RightFrontLeg,
            &[
                l(0.0, [0.0, -20.6, 12.0]),
                l(0.5, [0.0, -20.6, 8.0]),
                l(1.1, [0.0, -7.14, 4.42]),
                l(1.8, [0.0, -1.27, -1.33]),
                l(2.3, [0.0, -1.27, -0.33]),
                l(2.6, Z),
            ],
        ),
        rot(
            LeftFrontLeg,
            &[
                l(0.0, [-90.0, -10.0, 0.0]),
                l(0.5, [-90.0, -10.0, 0.0]),
                l(1.1, [-49.06, -10.0, 0.0]),
                l(1.8, [-22.5, -10.0, 0.0]),
                l(2.3, [-25.0, -10.0, 0.0]),
                l(2.6, Z),
            ],
        ),
        pos(
            LeftFrontLeg,
            &[
                l(0.0, [0.0, -20.6, 12.0]),
                l(0.5, [0.0, -20.6, 8.0]),
                l(1.1, [0.0, -7.14, 4.42]),
                l(1.8, [0.0, -1.27, -1.33]),
                l(2.3, [0.0, -1.27, -0.33]),
                l(2.6, Z),
            ],
        ),
        rot(
            LeftHindLeg,
            &[
                l(0.0, [-90.0, -15.0, 0.0]),
                l(0.3, [-90.0, 0.0, 0.0]),
                l(0.6, [-90.0, 0.0, 0.0]),
                l(1.1, [-60.0, 0.0, 0.0]),
                l(1.9, [35.0, 0.0, 0.0]),
                l(2.2, [30.0, 0.0, 0.0]),
                l(2.6, Z),
            ],
        ),
        pos(
            LeftHindLeg,
            &[
                l(0.0, [1.0, -20.5, 5.0]),
                l(0.3, [-2.0, -20.5, 3.0]),
                l(0.6, [-2.0, -20.5, 3.0]),
                l(1.1, [-2.0, -10.5, 2.0]),
                l(1.5, [-2.0, -0.4, -3.9]),
                l(1.9, [-2.0, -4.3, -9.8]),
                l(2.2, [-1.0, -2.5, -5.0]),
                l(2.6, Z),
            ],
        ),
        rot(
            RightHindLeg,
            &[
                l(0.0, [-90.0, 15.0, 0.0]),
                l(0.3, [-90.0, 0.0, 0.0]),
                l(0.6, [-90.0, 0.0, 0.0]),
                l(1.1, [-60.0, 0.0, 0.0]),
                l(1.9, [35.0, 0.0, 0.0]),
                l(2.2, [30.0, 0.0, 0.0]),
                l(2.6, Z),
            ],
        ),
        pos(
            RightHindLeg,
            &[
                l(0.0, [-1.0, -20.5, 5.0]),
                l(0.3, [2.0, -20.5, 3.0]),
                l(0.6, [2.0, -20.5, 3.0]),
                l(1.1, [2.0, -10.5, 2.0]),
                l(1.5, [2.0, -0.4, -3.9]),
                l(1.9, [2.0, -4.3, -9.8]),
                l(2.2, [1.0, -2.5, -5.0]),
                l(2.6, Z),
            ],
        ),
        rot(Head, &[l(0.0, Z), l(0.3, Z), l(0.8, [55.0, 0.0, 0.0]), l(2.0, [65.0, 0.0, 0.0]), l(2.4, Z)]),
        rot(
            Tail,
            &[
                l(0.0, [50.0, 0.0, 0.0]),
                l(0.4, [55.0, 0.0, 0.0]),
                l(0.9, [55.0, 0.0, 0.0]),
                l(1.5, [17.5, 0.0, 0.0]),
                l(2.6, [5.0, 0.0, 0.0]),
            ],
        ),
    ],
};

const FORE: [f32; 3] = [44.97272, 1.76749, -1.76833];
const FORE_MIRROR: [f32; 3] = [44.97272, -1.76749, 1.76833];
const UP: [f32; 3] = [-90.0, 0.0, 0.0];

/// `CAMEL_DASH`: a half-second gallop, looping while `DASH` is on.
pub const DASH: Anim = Anim {
    length: 0.5,
    looping: true,
    channels: &[
        rot(Body, &[l(0.0, [5.0, 0.0, 0.0]), l(0.5, [5.0, 0.0, 0.0])]),
        rot(
            Tail,
            &[
                c(0.0, [67.5, 0.0, 0.0]),
                c(0.125, [112.5, 0.0, 0.0]),
                c(0.25, [67.5, 0.0, 0.0]),
                c(0.375, [112.5, 0.0, 0.0]),
                c(0.5, [67.5, 0.0, 0.0]),
            ],
        ),
        rot(
            Head,
            &[c(0.0, [10.0, 0.0, 0.0]), c(0.125, Z), c(0.25, [10.0, 0.0, 0.0]), c(0.375, Z), c(0.5, [10.0, 0.0, 0.0])],
        ),
        rot(RightFrontLeg, &[c(0.0, FORE), c(0.125, UP), c(0.25, FORE), c(0.375, UP), c(0.5, FORE)]),
        rot(LeftFrontLeg, &[c(0.0, UP), c(0.125, FORE_MIRROR), c(0.25, UP), c(0.375, FORE_MIRROR), c(0.5, UP)]),
        rot(
            LeftHindLeg,
            &[
                c(0.0, [90.0, 0.0, 0.0]),
                c(0.125, [-45.0, 0.0, 0.0]),
                c(0.25, [90.0, 0.0, 0.0]),
                c(0.375, [-45.0, 0.0, 0.0]),
                c(0.5, [90.0, 0.0, 0.0]),
            ],
        ),
        rot(
            RightHindLeg,
            &[
                c(0.0, [-45.0, 0.0, 0.0]),
                c(0.125, [90.0, 0.0, 0.0]),
                c(0.25, [-45.0, 0.0, 0.0]),
                c(0.375, [90.0, 0.0, 0.0]),
                c(0.5, [-45.0, 0.0, 0.0]),
            ],
        ),
    ],
};

/// The animations' summed offsets per bone (`offsetRotation`, degrees, and
/// `offsetPos`, pixels), as Java adds each applied animation to the part.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Offsets {
    rot: [[f32; 3]; BONES],
    pos: [[f32; 3]; BONES],
}

impl Offsets {
    pub fn rot(&self, bone: Bone) -> [f32; 3] {
        self.rot[bone as usize]
    }
    pub fn pos(&self, bone: Bone) -> [f32; 3] {
        self.pos[bone as usize]
    }
}

/// `Mth.catmullrom`.
fn catmull(a: f32, p0: f32, p1: f32, p2: f32, p3: f32) -> f32 {
    0.5 * (2.0 * p1
        + (p2 - p0) * a
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * a * a
        + (3.0 * p1 - p0 - 3.0 * p2 + p3) * a * a * a)
}

/// `KeyframeAnimation.Entry.apply` at `secs` into the channel.
fn sample(keys: &[Key], secs: f32) -> [f32; 3] {
    let first = keys.iter().position(|k| secs <= k.t).unwrap_or(keys.len());
    let prev = first.saturating_sub(1);
    let next = (prev + 1).min(keys.len() - 1);
    let alpha =
        if next != prev { ((secs - keys[prev].t) / (keys[next].t - keys[prev].t)).clamp(0.0, 1.0) } else { 0.0 };
    let (a, b) = (keys[prev].v, keys[next].v);
    match keys[next].lerp {
        Lerp::Linear => std::array::from_fn(|i| a[i] + (b[i] - a[i]) * alpha),
        Lerp::Catmull => {
            let p0 = keys[prev.saturating_sub(1)].v;
            let p3 = keys[(next + 1).min(keys.len() - 1)].v;
            std::array::from_fn(|i| catmull(alpha, p0[i], a[i], b[i], p3[i]))
        }
    }
}

impl Anim {
    /// Adds this animation at `ticks` since it started (`getTimeInMillis`,
    /// whole milliseconds) to `out`.
    pub fn apply(&self, ticks: f32, out: &mut Offsets) {
        let secs = (ticks * 50.0) as i64 as f32 / 1000.0;
        let secs = if self.looping { secs % self.length } else { secs };
        for ch in self.channels {
            let v = sample(ch.keys, secs);
            let slot = if ch.position { &mut out.pos[ch.bone as usize] } else { &mut out.rot[ch.bone as usize] };
            for i in 0..3 {
                slot[i] += v[i];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sitting_ends_in_the_sit_pose_and_standing_up_returns_to_rest() {
        let mut end = Offsets::default();
        SIT.apply(40.0, &mut end);
        let mut pose = Offsets::default();
        SIT_POSE.apply(7.0, &mut pose);
        for bone in [Body, LeftFrontLeg, RightFrontLeg, LeftHindLeg, RightHindLeg, Tail] {
            assert_eq!(end.pos(bone), pose.pos(bone), "{bone:?}");
        }
        // The sit ends with the body level again and 19.9 pixels down.
        assert_eq!(end.rot(Body), [0.0, 0.0, 0.0]);
        assert_eq!(end.pos(Body), [0.0, -19.9, 0.0]);
        assert_eq!(end.rot(LeftHindLeg), [-90.0, -15.0, 0.0]);
        // Standing up starts where sitting ends and ends at rest (52 ticks).
        let mut start = Offsets::default();
        STAND_UP.apply(0.0, &mut start);
        assert_eq!(start.pos(Body), [0.0, -19.9, 0.0]);
        let mut done = Offsets::default();
        STAND_UP.apply(52.0, &mut done);
        for bone in [Body, Head, LeftFrontLeg, RightFrontLeg, LeftHindLeg, RightHindLeg] {
            assert_eq!((done.rot(bone), done.pos(bone)), (Z, Z), "{bone:?}");
        }
        // The tail settles at its 5-degree rest (as the idle animation holds it).
        assert_eq!(done.rot(Tail), [5.0, 0.0, 0.0]);
    }

    #[test]
    fn keyframes_interpolate_as_java_does() {
        // Linear: halfway from 0 to 30 degrees at 0.65 s (13 ticks).
        let mut o = Offsets::default();
        SIT.apply(13.0, &mut o);
        assert!((o.rot(Body)[0] - 15.0).abs() < 1e-4, "{:?}", o.rot(Body));
        // Catmull-Rom through the keys, and the dash loops every 10 ticks.
        let (mut a, mut b) = (Offsets::default(), Offsets::default());
        DASH.apply(1.0, &mut a);
        DASH.apply(11.0, &mut b);
        for bone in [Body, Head, Tail, LeftFrontLeg, RightFrontLeg, LeftHindLeg, RightHindLeg] {
            for i in 0..3 {
                assert!((a.rot(bone)[i] - b.rot(bone)[i]).abs() < 1e-3, "{bone:?}");
            }
        }
        // At 0.05 s the tail is 40% of the way from 67.5 to 112.5 on the
        // spline: 0.5 × (2 × 67.5 + 45 × 0.4 + (135 − 337.5 + 450 − 67.5) × 0.16
        // + (202.5 − 67.5 − 337.5 + 67.5) × 0.064).
        let want = 0.5 * (135.0 + 45.0 * 0.4 + 180.0 * 0.16 - 135.0 * 0.064);
        assert!((a.rot(Tail)[0] - want).abs() < 1e-3, "{} vs {want}", a.rot(Tail)[0]);
    }
}
