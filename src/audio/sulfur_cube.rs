//! Original procedural stand-ins for v0.16 Track D's sulfur cube sound
//! events (`entity.sulfur_cube.*` and its buckets). Nothing is sampled.

use super::dsp::{self, Biquad, Mode, Rng, add_mode, mix_into, noise, samples};
use crate::entity::sulfur_cube::Cue;

/// A wet thump: filtered noise over a falling body resonance.
fn thump(rng: &mut Rng, secs: f32, cutoff: f32, freq: f32, glide: f32) -> Vec<f32> {
    let mut out = noise(rng, samples(secs), |t| dsp::ad(t, 0.004, secs * 0.3));
    Biquad::lowpass(cutoff, 1.6).run(&mut out);
    add_mode(&mut out, 0, Mode { freq, amp: 0.7, tau: secs * 0.35, glide, glide_tau: secs * 0.2 });
    out
}

pub(super) fn render(cue: Cue, rng: &mut Rng) -> Vec<f32> {
    let pitch = rng.range(0.92, 1.08);
    match cue {
        Cue::Squish => dsp::finish(thump(rng, 0.22, 700.0, 120.0 * pitch, 0.6), 0.5),
        Cue::Bounce => {
            // The block inside knocks against the jelly.
            let mut out = thump(rng, 0.25, 900.0, 150.0 * pitch, 0.8);
            add_mode(
                &mut out,
                samples(0.01),
                Mode { freq: 420.0 * pitch, amp: 0.3, tau: 0.04, glide: 1.0, glide_tau: 0.01 },
            );
            dsp::finish(out, 0.5)
        }
        Cue::Jump => dsp::finish(thump(rng, 0.18, 900.0, 160.0 * pitch, 1.4), 0.45),
        Cue::Absorb => {
            // A slurp that ends in a gulp.
            let len = samples(0.45);
            let mut out = noise(rng, len, |t| 0.6 * dsp::ad(t, 0.05, 0.15));
            Biquad::bandpass(900.0 * pitch, 2.0).run(&mut out);
            let gulp = thump(rng, 0.2, 500.0, 90.0 * pitch, 0.5);
            mix_into(&mut out, &gulp, 1.0, samples(0.22));
            dsp::finish(out, 0.55)
        }
        Cue::Eject => {
            // A pop and a spatter.
            let mut out = thump(rng, 0.15, 1800.0, 260.0 * pitch, 0.5);
            let mut spatter = noise(rng, samples(0.3), |t| 0.4 * dsp::ad(t, 0.002, 0.06));
            Biquad::highpass(1500.0, 0.8).run(&mut spatter);
            mix_into(&mut out, &spatter, 1.0, samples(0.03));
            dsp::finish(out, 0.5)
        }
        Cue::Eat => {
            let mut out = noise(rng, samples(0.3), |t| dsp::ad(t, 0.01, 0.05));
            Biquad::bandpass(1300.0 * pitch, 3.0).run(&mut out);
            dsp::finish(out, 0.45)
        }
        Cue::Hit => dsp::finish(thump(rng, 0.2, 1400.0, 210.0 * pitch, 0.7), 0.55),
        Cue::Push => {
            let mut out = noise(rng, samples(0.35), |t| dsp::ad(t, 0.04, 0.12));
            Biquad::lowpass(450.0, 1.2).run(&mut out);
            dsp::finish(out, 0.4)
        }
        Cue::BucketFill | Cue::BucketEmpty => {
            // A metal bucket ring under a squelch (rising to fill, falling to empty).
            let glide = if cue == Cue::BucketFill { 1.5 } else { 0.6 };
            let mut out = thump(rng, 0.35, 800.0, 130.0 * pitch, glide);
            add_mode(&mut out, 0, Mode { freq: 1250.0 * pitch, amp: 0.2, tau: 0.12, glide: 1.0, glide_tau: 0.05 });
            dsp::finish(out, 0.5)
        }
    }
}
