//! v1.0 gap Track 2g: original procedural stand-ins for Java 26.3's goat
//! long-jump sounds (`entity.goat.long_jump`, `entity.goat.screaming.long_jump`
//! and the landing's `entity.goat.step`). Nothing is sampled: the jumps are
//! the shared voice model's grunt in the goat's bleating vowel, the landing
//! a hoof's damped knock over a scuff of noise.

use super::dsp::{self, Biquad, Mode, Rng, add_mode, mix_into, noise, samples};
use super::voices::{FLAT, Utterance, lerp, swell, utter};
use voxelcraft::entity::animals::long_jump::Cue;

/// The goat's vowel: an open, nasal "eh".
const GOAT: [(f32, f32, f32); 3] = [(700.0, 4.0, 1.0), (1650.0, 5.0, 0.55), (2700.0, 6.0, 0.25)];

/// A short effort grunt with the push-off's scrape: higher and harsher for
/// the screaming goat.
fn jump(rng: &mut Rng, screaming: bool) -> Vec<f32> {
    let secs = if screaming { 0.32 } else { 0.22 };
    let base = if screaming { rng.range(520.0, 600.0) } else { rng.range(230.0, 270.0) };
    let mut out = vec![0.0; samples(secs + 0.08)];
    let grunt = utter(
        rng,
        &Utterance {
            secs,
            f0: &|t| lerp(base * 1.15, base * 0.85, t / secs),
            env: &|t| swell(t, 0.015, secs * 0.4, secs),
            formants: GOAT,
            shift: FLAT,
            jitter: if screaming { 0.22 } else { 0.12 },
            breath: if screaming { 1.2 } else { 0.7 },
        },
    );
    mix_into(&mut out, &grunt, 0.7, samples(0.02));
    let mut scrape = noise(rng, samples(0.08), |t| (-(t / 0.025)).exp());
    Biquad::bandpass(1800.0, 0.9).run(&mut scrape);
    mix_into(&mut out, &scrape, 0.5, 0);
    dsp::finish(out, if screaming { 0.6 } else { 0.5 })
}

/// A cloven hoof landing: a dull knock and a short scuff.
fn step(rng: &mut Rng) -> Vec<f32> {
    let mut out = vec![0.0; samples(0.2)];
    let freq = rng.range(420.0, 520.0);
    add_mode(&mut out, 0, Mode { freq, amp: 0.6, tau: 0.03, glide: -0.2, glide_tau: 0.02 });
    add_mode(&mut out, samples(0.01), Mode { freq: freq * 2.7, amp: 0.25, tau: 0.015, glide: 0.0, glide_tau: 0.01 });
    let mut scuff = noise(rng, samples(0.1), |t| (-(t / 0.03)).exp());
    Biquad::bandpass(2400.0, 1.0).run(&mut scuff);
    mix_into(&mut out, &scuff, 0.35, samples(0.005));
    dsp::finish(out, 0.45)
}

pub(super) fn render(cue: Cue, rng: &mut Rng) -> Vec<f32> {
    match cue {
        Cue::LongJump => jump(rng, false),
        Cue::ScreamingLongJump => jump(rng, true),
        Cue::Step => step(rng),
    }
}
