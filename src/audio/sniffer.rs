//! Original procedural stand-ins for Java's sniffer and sniffer egg sound
//! events (`entity.sniffer.*`, `block.sniffer_egg.*`). Nothing is sampled:
//! each cue is built from filtered noise, crackle grains and damped modes.

use super::dsp::{self, Biquad, Mode, Rng, add_mode, crackle, mix_into, noise, samples};
use crate::entity::sniffer::Cue;

/// One short nasal draw of air: band-passed noise with a quick swell.
fn sniff(out: &mut [f32], rng: &mut Rng, at: f32, len: f32, fc: f32, gain: f32) {
    let mut n = noise(rng, samples(len), |t| (t / (len * 0.35)).min(1.0) * (-(t / (len * 0.4))).exp());
    Biquad::bandpass(fc, 1.6).run(&mut n);
    mix_into(out, &n, gain, samples(at));
}

/// Damp soil scraping: low-passed crackle grains.
fn scrape(out: &mut [f32], rng: &mut Rng, at: f32, len: f32, density: f32) {
    let mut c = crackle(rng, samples(len), (4.0, 18.0), |t| density * (1.0 - t / len).max(0.0));
    Biquad::lowpass(rng.range(900.0, 1300.0), 0.7).run(&mut c);
    mix_into(out, &c, 1.0, samples(at));
}

/// A deep, breathy grunt with a downward glide.
fn grunt(out: &mut [f32], at: usize, freq: f32, tau: f32, glide: f32) {
    for (ratio, amp) in [(1.0, 0.6), (2.03, 0.25), (3.1, 0.1)] {
        add_mode(out, at, Mode { freq: freq * ratio, amp, tau, glide, glide_tau: tau });
    }
}

pub(super) fn render(cue: Cue, rng: &mut Rng) -> Vec<f32> {
    match cue {
        // A long rising inhale that ends in a short snort.
        Cue::Scenting => {
            let mut out = vec![0.; samples(1.1)];
            let mut n = noise(rng, samples(0.85), |t| (t / 0.7).min(1.0).powi(2) * (1.0 - (t - 0.7).max(0.0) / 0.15));
            Biquad::bandpass(rng.range(520.0, 640.0), 1.2).run(&mut n);
            mix_into(&mut out, &n, 0.8, 0);
            grunt(&mut out, samples(0.82), rng.range(70.0, 82.0), 0.09, 0.8);
            dsp::finish(out, 0.5)
        }
        // Three or four quick sniffs.
        Cue::Sniffing => {
            let mut out = vec![0.; samples(0.8)];
            let count = 3 + (rng.f32() * 2.0) as usize;
            for i in 0..count {
                let fc = rng.range(700.0, 980.0);
                sniff(&mut out, rng, i as f32 * 0.16, 0.12, fc, 1.0);
            }
            dsp::finish(out, 0.45)
        }
        // A softer, slower sniff over a low hum while it walks to a scent.
        Cue::Searching => {
            let mut out = vec![0.; samples(0.7)];
            let fc = rng.range(480.0, 620.0);
            sniff(&mut out, rng, 0.0, 0.3, fc, 0.7);
            grunt(&mut out, samples(0.08), rng.range(55.0, 66.0), 0.18, 0.95);
            dsp::finish(out, 0.35)
        }
        // Nose pushing through earth.
        Cue::Digging => {
            let mut out = vec![0.; samples(1.0)];
            scrape(&mut out, rng, 0.0, 0.95, 160.0);
            let fc = rng.range(380.0, 460.0);
            sniff(&mut out, rng, 0.45, 0.25, fc, 0.5);
            dsp::finish(out, 0.45)
        }
        // Lifting out of the hole: a grunt and a shake of loose soil.
        Cue::DiggingStop => {
            let mut out = vec![0.; samples(0.9)];
            grunt(&mut out, 0, rng.range(62.0, 74.0), 0.16, 0.75);
            scrape(&mut out, rng, 0.12, 0.5, 90.0);
            dsp::finish(out, 0.5)
        }
        // A seed popping out of the soil.
        Cue::DropSeed => {
            let mut out = vec![0.; samples(0.3)];
            add_mode(
                &mut out,
                0,
                Mode { freq: rng.range(300.0, 360.0), amp: 1.0, tau: 0.03, glide: 1.6, glide_tau: 0.02 },
            );
            scrape(&mut out, rng, 0.0, 0.12, 120.0);
            dsp::finish(out, 0.4)
        }
        // Two contented rising hums.
        Cue::Happy => {
            let mut out = vec![0.; samples(1.0)];
            let base = rng.range(84.0, 96.0);
            grunt(&mut out, 0, base, 0.14, 1.12);
            grunt(&mut out, samples(0.3), base * 1.26, 0.18, 1.15);
            dsp::finish(out, 0.5)
        }
        // A thin shell crack.
        Cue::EggCrack => {
            let mut out = crackle(rng, samples(0.25), (0.6, 2.5), |t| 900.0 * (-t / 0.05).exp());
            Biquad::highpass(1400.0, 0.7).run(&mut out);
            add_mode(
                &mut out,
                0,
                Mode { freq: rng.range(1700.0, 2100.0), amp: 0.3, tau: 0.02, glide: 1.0, glide_tau: 1.0 },
            );
            dsp::finish(out, 0.45)
        }
        // The shell giving way, then a soft landing.
        Cue::EggHatch => {
            let mut out = vec![0.; samples(0.6)];
            let mut c = crackle(rng, samples(0.3), (0.8, 4.0), |t| 1400.0 * (-t / 0.1).exp());
            Biquad::highpass(900.0, 0.7).run(&mut c);
            mix_into(&mut out, &c, 1.0, 0);
            add_mode(&mut out, samples(0.22), Mode { freq: 120.0, amp: 0.8, tau: 0.06, glide: 0.7, glide_tau: 0.03 });
            dsp::finish(out, 0.5)
        }
        // A heavy egg dropping onto the ground.
        Cue::EggPlop => {
            let mut out = vec![0.; samples(0.35)];
            add_mode(
                &mut out,
                0,
                Mode { freq: rng.range(140.0, 170.0), amp: 1.0, tau: 0.05, glide: 0.55, glide_tau: 0.03 },
            );
            let mut n = noise(rng, samples(0.08), |t| dsp::ad(t, 0.002, 0.02));
            Biquad::lowpass(700.0, 0.7).run(&mut n);
            mix_into(&mut out, &n, 0.5, 0);
            dsp::finish(out, 0.5)
        }
    }
}
