//! Original procedural stand-ins for v0.17 Track D1's minecart sounds: the
//! rolling rumble heard from a moving cart (`entity.minecart.riding`) and
//! the rider's inside sound (`entity.minecart.inside`, also underwater).
//! Nothing is sampled. Each buffer is a short stretch under a Hann window,
//! so stretches started every half of their length sum to a steady loop
//! (`Game::minecart_sounds`).

use super::dsp::{self, Biquad, Rng, crackle, noise, samples};
use crate::entity::minecart::Cue;

/// Length of one stretch in seconds; the game starts one every half of it.
pub const STRETCH: f32 = 0.5;

/// `sin²` over the stretch: overlapping halves add up to one.
fn hann(t: f32) -> f32 {
    let x = (std::f32::consts::PI * t / STRETCH).sin();
    x * x
}

pub(super) fn render(cue: Cue, rng: &mut Rng) -> Vec<f32> {
    let len = samples(STRETCH);
    // Wheels on steel: a low rumble with a faint grinding band and sparse
    // clicks of rail joints.
    let mut rumble = noise(rng, len, |_| 1.0);
    Biquad::lowpass(if cue == Cue::InsideUnderwater { 160.0 } else { 260.0 }, 0.9).run(&mut rumble);
    Biquad::highpass(45.0, 0.7).run(&mut rumble);
    let mut out = rumble;
    if cue != Cue::InsideUnderwater {
        let mut grind = noise(rng, len, |_| 0.25);
        Biquad::bandpass(if cue == Cue::Rolling { 1400.0 } else { 900.0 }, 2.5).run(&mut grind);
        dsp::mix_into(&mut out, &grind, 1.0, 0);
        let mut clicks = crackle(rng, len, (2.0, 6.0), |_| 10.0);
        Biquad::bandpass(2200.0, 1.5).run(&mut clicks);
        dsp::mix_into(&mut out, &clicks, if cue == Cue::Rolling { 0.4 } else { 0.2 }, 0);
    }
    // The window goes on last, after the filters, so the ends are silent.
    for (i, s) in out.iter_mut().enumerate() {
        *s *= hann(i as f32 / dsp::RATE);
    }
    dsp::finish(out, if cue == Cue::Rolling { 0.6 } else { 0.5 })
}
