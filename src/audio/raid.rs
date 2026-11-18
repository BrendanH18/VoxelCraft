//! Original procedural stand-ins for v0.15 Track E's sound events: the raid
//! horn (`event.raid.horn`), a bell resonating near raiders
//! (`block.bell.resonate`) and villagers cheering a won raid
//! (`entity.villager.celebrate`). Nothing is sampled.

use super::dsp::{self, Biquad, Mode, Rng, add_mode, noise, samples};
use super::voices::{Call, Voice};
use crate::entity::raid::Cue;

pub(super) fn render(cue: Cue, rng: &mut Rng) -> Vec<f32> {
    match cue {
        Cue::Horn => {
            // A long, low, slightly detuned brass call that swells and fades.
            let len = samples(3.2);
            let mut out = vec![0.0; len];
            let base = 98.0 * rng.range(0.98, 1.02);
            for (i, s) in out.iter_mut().enumerate() {
                let t = i as f32 / dsp::RATE;
                let env = (t / 0.6).min(1.0) * (1.0 - ((t - 2.2) / 1.0).clamp(0.0, 1.0));
                let vibrato = 1.0 + 0.006 * (std::f32::consts::TAU * 4.5 * t).sin();
                let mut v = 0.0;
                for (k, amp) in [(1.0, 0.55), (2.0, 0.3), (3.0, 0.18), (4.0, 0.1), (5.0, 0.05)] {
                    let phase = std::f32::consts::TAU * base * k * vibrato * t;
                    v += amp * (phase.sin() + 0.35 * (phase * 1.003).sin());
                }
                *s = v * env;
            }
            let mut breath = noise(rng, len, |t| 0.06 * (t / 0.6).min(1.0) * (1.0 - t / 3.2).max(0.0));
            Biquad::bandpass(base * 6.0, 1.5).run(&mut breath);
            dsp::mix_into(&mut out, &breath, 1.0, 0);
            Biquad::lowpass(1400.0, 0.7).run(&mut out);
            dsp::finish(out, 0.6)
        }
        Cue::BellResonate => {
            // The bell's partials ringing on, wavering as they fade.
            let mut out = vec![0.0; samples(2.5)];
            let base = 520.0 * rng.range(0.99, 1.01);
            for (ratio, amp, tau) in
                [(0.5, 0.35, 1.6), (1.0, 0.5, 1.4), (1.19, 0.3, 1.1), (1.56, 0.2, 0.9), (2.0, 0.25, 0.8)]
            {
                add_mode(&mut out, 0, Mode { freq: base * ratio, amp, tau, glide: 1.0, glide_tau: tau });
                add_mode(
                    &mut out,
                    0,
                    Mode { freq: base * ratio * 1.004, amp: amp * 0.6, tau, glide: 1.0, glide_tau: tau },
                );
            }
            dsp::finish(out, 0.45)
        }
        Cue::VillagerCelebrate => super::voices::render(Voice::Villager, Call::Ambient, rng),
    }
}
