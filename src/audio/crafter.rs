//! Original procedural stand-ins for v0.17 Track C1's crafter sounds
//! (`block.crafter.craft` and `block.crafter.fail`, both played as
//! `BLOCKS` at volume 1 and pitch 1). Nothing is sampled: the craft is a
//! clack of metal plates over a short whirr of gears, the failure a dull
//! double knock with a falling buzz.

use super::dsp::{self, Biquad, Mode, Rng, add_mode, crackle, mix_into, noise, samples};
use crate::world::crafter::Cue;

pub(super) fn render(cue: Cue, rng: &mut Rng) -> Vec<f32> {
    match cue {
        Cue::Craft => {
            let mut out = vec![0.0; samples(0.6)];
            // Gears: a burst of fine crackle through a band-pass.
            let mut gears = crackle(rng, samples(0.3), (1.0, 3.0), |t| 900.0 * (1.0 - t / 0.3).max(0.0));
            Biquad::bandpass(1800.0 * rng.range(0.95, 1.05), 1.8).run(&mut gears);
            mix_into(&mut out, &gears, 0.5, 0);
            // Two plates clacking shut, the second a little lower.
            for (at, freq) in [(0.0, 920.0), (0.11, 700.0)] {
                let start = samples(at);
                add_mode(&mut out, start, Mode { freq, amp: 0.7, tau: 0.05, glide: 0.92, glide_tau: 0.03 });
                add_mode(
                    &mut out,
                    start,
                    Mode { freq: freq * 2.71, amp: 0.25, tau: 0.03, glide: 1.0, glide_tau: 0.03 },
                );
                let mut tick = noise(rng, samples(0.02), |t| (1.0 - t / 0.02).max(0.0));
                Biquad::highpass(2500.0, 0.8).run(&mut tick);
                mix_into(&mut out, &tick, 0.4, start);
            }
            dsp::finish(out, 0.6)
        }
        Cue::Fail => {
            let mut out = vec![0.0; samples(0.45)];
            for at in [0.0, 0.12] {
                add_mode(&mut out, samples(at), Mode { freq: 180.0, amp: 0.8, tau: 0.06, glide: 0.8, glide_tau: 0.04 });
            }
            let mut buzz = noise(rng, samples(0.3), |t| (1.0 - t / 0.3).max(0.0) * 0.6);
            Biquad::bandpass(420.0, 3.0).run(&mut buzz);
            mix_into(&mut out, &buzz, 0.5, samples(0.05));
            dsp::finish(out, 0.5)
        }
    }
}
