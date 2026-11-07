//! Original procedural stand-ins for Track C's sound events: the breeze,
//! wind charges, trial spawners, vaults and ominous bottles. Nothing is
//! sampled: gusts are swept band-passed noise, mechanisms damped modes.

use super::dsp::{self, Biquad, Mode, Rng, add_mode, crackle, mix_into, noise, samples};
use crate::entity::breeze::Cue;

/// A gust: noise through a band-pass whose envelope swells and fades.
fn gust(out: &mut [f32], rng: &mut Rng, at: f32, len: f32, fc: f32, q: f32, gain: f32) {
    let mut n = noise(rng, samples(len), |t| {
        let p = t / len;
        (p * std::f32::consts::PI).sin().powf(1.5)
    });
    Biquad::bandpass(fc, q).run(&mut n);
    mix_into(out, &n, gain, samples(at));
}

/// A metallic clank (shutters, keys).
fn clank(out: &mut [f32], at: f32, freq: f32, tau: f32, gain: f32) {
    for (ratio, amp) in [(1.0, 0.5), (2.76, 0.25), (5.4, 0.12)] {
        add_mode(out, samples(at), Mode { freq: freq * ratio, amp: amp * gain, tau, glide: 1.0, glide_tau: tau });
    }
}

pub(super) fn render(cue: Cue, rng: &mut Rng) -> Vec<f32> {
    let mut out = vec![0.0; samples(1.2)];
    match cue {
        Cue::Whirl => {
            let fc = rng.range(500.0, 700.0);
            gust(&mut out, rng, 0.0, 1.0, fc, 0.8, 0.6)
        }
        Cue::Shoot => {
            gust(&mut out, rng, 0.0, 0.35, 1400.0, 1.2, 1.0);
            gust(&mut out, rng, 0.05, 0.5, 600.0, 0.9, 0.6);
        }
        Cue::Inhale | Cue::Charge => {
            let len = if cue == Cue::Inhale { 0.75 } else { 0.5 };
            let mut n = noise(rng, samples(len), |t| (t / len).powi(2));
            Biquad::bandpass(900.0, 1.4).run(&mut n);
            mix_into(&mut out, &n, 0.8, 0);
        }
        Cue::Jump => gust(&mut out, rng, 0.0, 0.45, 350.0, 0.8, 1.0),
        Cue::Land => {
            gust(&mut out, rng, 0.0, 0.3, 250.0, 0.7, 0.8);
            let mut c = crackle(rng, samples(0.2), (6.0, 18.0), |t| 30.0 * (1.0 - t / 0.2).max(0.0));
            Biquad::lowpass(1200.0, 0.7).run(&mut c);
            mix_into(&mut out, &c, 0.5, 0);
        }
        Cue::Deflect => {
            gust(&mut out, rng, 0.0, 0.25, 1800.0, 2.0, 0.8);
            clank(&mut out, 0.0, 1400.0, 0.08, 0.4);
        }
        Cue::Burst => {
            gust(&mut out, rng, 0.0, 0.6, 450.0, 0.6, 1.0);
            gust(&mut out, rng, 0.0, 0.25, 1600.0, 1.0, 0.5);
        }
        Cue::Throw => gust(&mut out, rng, 0.0, 0.3, 1100.0, 1.4, 0.7),
        Cue::SpawnerDetect | Cue::SpawnerOminous => {
            let base = if cue == Cue::SpawnerOminous { 330.0 } else { 440.0 };
            for (i, r) in [1.0, 1.26, 1.5].into_iter().enumerate() {
                add_mode(
                    &mut out,
                    samples(i as f32 * 0.12),
                    Mode { freq: base * r, amp: 0.3, tau: 0.4, glide: 1.0, glide_tau: 0.4 },
                );
            }
        }
        Cue::SpawnerSpawn => {
            let mut c = crackle(rng, samples(0.5), (5.0, 15.0), |t| 40.0 * (1.0 - t / 0.5).max(0.0));
            Biquad::bandpass(1500.0, 0.8).run(&mut c);
            mix_into(&mut out, &c, 0.8, 0);
            gust(&mut out, rng, 0.0, 0.4, 700.0, 1.0, 0.5);
        }
        Cue::SpawnerOpen | Cue::VaultOpen => clank(&mut out, 0.0, 220.0, 0.25, 1.0),
        Cue::SpawnerClose | Cue::VaultClose => clank(&mut out, 0.0, 180.0, 0.2, 1.0),
        Cue::SpawnerEject | Cue::VaultEject => {
            clank(&mut out, 0.0, 600.0, 0.06, 0.6);
            gust(&mut out, rng, 0.0, 0.2, 1200.0, 1.5, 0.4);
        }
        Cue::ItemSpawnerBegin => {
            let mut n = noise(rng, samples(0.8), |t| (t / 0.8).min(1.0));
            Biquad::bandpass(500.0, 3.0).run(&mut n);
            mix_into(&mut out, &n, 0.6, 0);
        }
        Cue::ItemSpawnerDrop => clank(&mut out, 0.0, 900.0, 0.1, 0.5),
        Cue::VaultActivate | Cue::VaultDeactivate => {
            let (a, b) = if cue == Cue::VaultActivate { (300.0, 450.0) } else { (450.0, 300.0) };
            add_mode(&mut out, 0, Mode { freq: a, amp: 0.4, tau: 0.3, glide: b / a, glide_tau: 0.3 });
        }
        Cue::VaultInsert => clank(&mut out, 0.0, 1200.0, 0.12, 0.7),
        Cue::VaultFail | Cue::VaultReject => {
            add_mode(&mut out, 0, Mode { freq: 160.0, amp: 0.5, tau: 0.15, glide: 0.8, glide_tau: 0.15 });
        }
        Cue::OminousBottle => {
            let mut c = crackle(rng, samples(0.3), (8.0, 20.0), |t| 40.0 * (1.0 - t / 0.3).max(0.0));
            Biquad::highpass(2000.0, 0.7).run(&mut c);
            mix_into(&mut out, &c, 0.7, 0);
        }
        Cue::TrialOmen => {
            for (i, f) in [262.0, 311.0, 392.0].into_iter().enumerate() {
                add_mode(
                    &mut out,
                    samples(i as f32 * 0.05),
                    Mode { freq: f, amp: 0.25, tau: 0.6, glide: 1.0, glide_tau: 0.6 },
                );
            }
        }
    }
    dsp::finish(out, 0.5)
}
