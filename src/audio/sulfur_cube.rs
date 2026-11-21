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

/// v0.16 Track H: original stand-ins for potent sulfur's geyser and gas
/// (`block.potent_sulfur.geyser_*` and `noxious_gas`): hissing noise
/// through moving band-passes over a low boiling rumble.
pub(super) fn render_geyser(cue: crate::world::potent_sulfur::Cue, rng: &mut Rng) -> Vec<f32> {
    use crate::world::potent_sulfur::Cue as G;
    let pitch = rng.range(0.94, 1.06);
    // Steam: high noise swelling in and out.
    let hiss = |rng: &mut Rng, secs: f32, attack: f32, centre: f32, q: f32| {
        let mut out = noise(rng, samples(secs), |t| dsp::ad(t, attack, secs * 0.45));
        Biquad::bandpass(centre * pitch, q).run(&mut out);
        out
    };
    // Boiling: a train of soft low pops.
    let boil = |rng: &mut Rng, secs: f32, pops: usize, gain: f32| {
        let mut out = vec![0.0; samples(secs)];
        for i in 0..pops {
            let at = samples(secs * (i as f32 + rng.range(0.0, 0.8)) / pops as f32);
            let freq = rng.range(70.0, 140.0) * pitch;
            add_mode(
                &mut out,
                at,
                Mode { freq, amp: gain * rng.range(0.5, 1.0), tau: 0.05, glide: 1.6, glide_tau: 0.03 },
            );
        }
        out
    };
    match cue {
        G::EruptionStart | G::ContinuousStart => {
            // A gulp of water giving way to a rising roar of steam.
            let len = if cue == G::EruptionStart { 1.6 } else { 2.2 };
            let mut out = vec![0.0; samples(len)];
            let gulp = boil(rng, 0.35, 4, 0.8);
            mix_into(&mut out, &gulp, 1.0, 0);
            let steam = hiss(rng, len - 0.15, 0.25, 2400.0, 0.8);
            mix_into(&mut out, &steam, 0.9, samples(0.15));
            let mut rumble = noise(rng, samples(len), |t| dsp::ad(t, 0.1, len * 0.5));
            Biquad::lowpass(160.0, 0.9).run(&mut rumble);
            mix_into(&mut out, &rumble, 0.8, 0);
            dsp::finish(out, 0.6)
        }
        G::EruptionActive | G::ContinuousActive => {
            // The plume's steady rush with water slapping back down.
            let len = 2.0;
            let mut out = hiss(rng, len, 0.05, if cue == G::EruptionActive { 1800.0 } else { 1500.0 }, 0.6);
            let splash = boil(rng, len, 9, 0.35);
            mix_into(&mut out, &splash, 1.0, 0);
            dsp::finish(out, 0.45)
        }
        G::NoxiousGas => {
            // A slow bubbling seep with a thin hiss.
            let len = 1.4;
            let mut out = boil(rng, len, 6, 0.5);
            let seep = hiss(rng, len, 0.3, 900.0, 1.5);
            mix_into(&mut out, &seep, 0.35, 0);
            dsp::finish(out, 0.35)
        }
    }
}
