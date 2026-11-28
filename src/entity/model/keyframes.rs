//! v1.0 gap Track 7c: Java's keyframe animations (`AnimationDefinition`,
//! `KeyframeAnimation`) on a [`Rig`]'s bones, sampled as
//! `KeyframeAnimations.animate` samples them (the camel's Track 3b engine in
//! [`super::camel_anim`] does the same for its own bones): the keyframe
//! before the time and the next one, joined by the next one's interpolation
//! (`LINEAR`, or `CATMULLROM` through the neighbours); a looping animation
//! wraps, a finished one holds its last keyframe; every value is scaled by
//! the strength and added to the part (`offsetRotation`, `offsetPos`).
//! Values are as Java's definitions write them: rotations in degrees
//! (`degreeVec`), positions in pixels with y up (`posVec`), scales as
//! factors (`scaleVec`). The tables were turned from the 26.3 client's
//! definitions by a converter that read their numbers only.

use super::rig::Rig;

/// What a channel moves (`AnimationChannel.Targets`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Target {
    Rot,
    Pos,
    /// `SCALE` (v1.0 gap Track 7d: the rig scales its parts).
    Scale,
}
pub(super) use Target::{Pos, Rot, Scale};

#[derive(Clone, Copy, Debug)]
pub(super) struct Key {
    t: f32,
    v: [f32; 3],
    catmull: bool,
}

pub(super) const fn l(t: f32, v: [f32; 3]) -> Key {
    Key { t, v, catmull: false }
}
pub(super) const fn c(t: f32, v: [f32; 3]) -> Key {
    Key { t, v, catmull: true }
}

/// One `AnimationChannel` on a bone of the rig.
pub(super) struct Channel {
    bone: usize,
    target: Target,
    keys: &'static [Key],
}

pub(super) const fn ch(bone: usize, target: Target, keys: &'static [Key]) -> Channel {
    Channel { bone, target, keys }
}

/// An `AnimationDefinition`.
pub(super) struct Anim {
    pub length: f32,
    pub looping: bool,
    pub channels: &'static [Channel],
}

/// `Mth.catmullrom`.
fn catmull(a: f32, p0: f32, p1: f32, p2: f32, p3: f32) -> f32 {
    0.5 * (2.0 * p1
        + (p2 - p0) * a
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * a * a
        + (3.0 * p1 - p0 - 3.0 * p2 + p3) * a * a * a)
}

/// The channel's value `secs` into the animation.
fn sample(keys: &[Key], secs: f32) -> [f32; 3] {
    let first = keys.iter().position(|k| secs <= k.t).unwrap_or(keys.len());
    let prev = first.saturating_sub(1);
    let next = (prev + 1).min(keys.len() - 1);
    let alpha =
        if next != prev { ((secs - keys[prev].t) / (keys[next].t - keys[prev].t)).clamp(0.0, 1.0) } else { 0.0 };
    let (a, b) = (keys[prev].v, keys[next].v);
    if keys[next].catmull {
        let p0 = keys[prev.saturating_sub(1)].v;
        let p3 = keys[(next + 1).min(keys.len() - 1)].v;
        std::array::from_fn(|i| catmull(alpha, p0[i], a[i], b[i], p3[i]))
    } else {
        std::array::from_fn(|i| a[i] + (b[i] - a[i]) * alpha)
    }
}

impl Anim {
    /// `KeyframeAnimations.animate` at `millis` with strength `scale`.
    pub fn at_millis(&self, rig: &mut Rig, millis: i64, scale: f32) {
        let secs = millis as f32 / 1000.0;
        let secs = if self.looping { secs % self.length } else { secs };
        for ch in self.channels {
            let v = sample(ch.keys, secs);
            match ch.target {
                Rot => {
                    for (r, d) in rig.rot[ch.bone].iter_mut().zip(v) {
                        *r += d.to_radians() * scale;
                    }
                }
                Pos => {
                    let p = &mut rig.at[ch.bone];
                    p[0] += v[0] * scale;
                    p[1] -= v[1] * scale;
                    p[2] += v[2] * scale;
                }
                // `scaleVec` stores each factor less one, which
                // `offsetScale` adds to the part's scale.
                Scale => {
                    for (s, d) in rig.scale[ch.bone].iter_mut().zip(v) {
                        *s += (d - 1.0) * scale;
                    }
                }
            }
        }
    }

    /// `KeyframeAnimation.apply(state, ageInTicks)`: `ticks` since the
    /// state started (`AnimationState.getTimeInMillis`, whole
    /// milliseconds).
    pub fn apply(&self, rig: &mut Rig, ticks: f32) {
        self.at_millis(rig, (ticks * 50.0) as i64, 1.0);
    }

    /// `KeyframeAnimation.applyWalk(pos, speed, speedScale, ampScale)`:
    /// the walk position as time and the walk speed as strength.
    pub fn apply_walk(&self, rig: &mut Rig, pos: f32, speed: f32, speed_scale: f32, amp_scale: f32) {
        self.at_millis(rig, (pos * 50.0 * speed_scale) as i64, (speed * amp_scale).min(1.0));
    }
}

#[cfg(test)]
mod tests {
    use super::super::rig::{ROOT, bone};
    use super::*;

    static BONES: &[super::super::rig::Bone] = &[bone(&[], ROOT, [0.0; 3], [0.0; 3])];
    const SWING: Anim = Anim {
        length: 1.0,
        looping: true,
        channels: &[
            ch(0, Rot, &[l(0.0, [0.0, 0.0, 0.0]), l(0.5, [90.0, 0.0, 0.0]), l(1.0, [0.0, 0.0, 0.0])]),
            ch(0, Pos, &[l(0.0, [0.0, 2.0, 0.0])]),
        ],
    };

    /// Halfway to the 90-degree keyframe is 45 degrees, a looping
    /// animation wraps, and `posVec`'s y is up (Java's y is down).
    #[test]
    fn keyframes_interpolate_and_loop_as_java_s() {
        let mut r = Rig::new(BONES);
        SWING.apply(&mut r, 5.0);
        assert!((r.rot[0][0] - 45f32.to_radians()).abs() < 1e-5);
        assert_eq!(r.at[0], [0.0, -2.0, 0.0]);
        let mut r = Rig::new(BONES);
        SWING.apply(&mut r, 25.0);
        assert!((r.rot[0][0] - 45f32.to_radians()).abs() < 1e-5, "1.25 s wraps to 0.25 s");
        let mut r = Rig::new(BONES);
        SWING.apply_walk(&mut r, 10.0, 0.25, 1.0, 2.0);
        // 500 ms in is the 90-degree keyframe, at strength min(0.25 * 2, 1).
        assert!((r.rot[0][0] - 45f32.to_radians()).abs() < 1e-5, "strength min(speed * amp, 1)");
    }

    /// v1.0 gap Track 7d: a `SCALE` channel adds its factor less one,
    /// times the strength, to the part's scale.
    #[test]
    fn scale_keys_stretch_the_part() {
        const GROW: Anim = Anim {
            length: 1.0,
            looping: false,
            channels: &[ch(0, Scale, &[l(0.0, [1.0, 1.0, 1.0]), l(1.0, [1.0, 3.0, 0.5])])],
        };
        let mut r = Rig::new(BONES);
        GROW.apply(&mut r, 10.0);
        assert_eq!(r.scale[0], [1.0, 2.0, 0.75]);
        let mut r = Rig::new(BONES);
        GROW.at_millis(&mut r, 1000, 0.5);
        assert_eq!(r.scale[0], [1.0, 2.0, 0.75]);
    }
}
