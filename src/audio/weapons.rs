//! Original procedural stand-ins for the v0.16 weapon sound events
//! (`item.mace.smash_*`, `item.trident.*`, `item.spear*`,
//! `entity.wind_charge.wind_burst`, `entity.drowned.shoot`). Nothing is
//! sampled: thuds are damped modes, whooshes band-passed noise sweeps.

use super::dsp::{self, Biquad, Mode, Rng, add_mode, mix_into, noise, samples};
use crate::weapons::Cue;

/// A swept band of noise: a whoosh rising or falling from `f0` to `f1`.
fn whoosh(out: &mut [f32], rng: &mut Rng, at: f32, len: f32, f0: f32, f1: f32, gain: f32) {
    let n = samples(len);
    let raw = noise(rng, n, |t| dsp::ad(t, len * 0.3, len * 0.35));
    let mut swept = vec![0.0; n];
    let steps = 16;
    for k in 0..steps {
        let (a, b) = (k * n / steps, ((k + 1) * n / steps).min(n));
        let fc = f0 + (f1 - f0) * k as f32 / steps as f32;
        let mut part = raw[a..b].to_vec();
        Biquad::bandpass(fc, 1.4).run(&mut part);
        swept[a..b].copy_from_slice(&part);
    }
    mix_into(out, &swept, gain, samples(at));
}

/// A heavy struck body: a low thump with a metallic ring above it.
fn thud(out: &mut [f32], at: usize, freq: f32, tau: f32, ring: f32) {
    add_mode(out, at, Mode { freq, amp: 0.9, tau, glide: 0.6, glide_tau: tau * 0.5 });
    add_mode(out, at, Mode { freq: freq * 2.7, amp: 0.25 * ring, tau: tau * 1.4, glide: 1.0, glide_tau: 1.0 });
    add_mode(out, at, Mode { freq: freq * 5.3, amp: 0.12 * ring, tau: tau * 2.0, glide: 1.0, glide_tau: 1.0 });
}

/// A short bright tick of metal on metal.
fn clink(out: &mut [f32], at: usize, freq: f32) {
    for (ratio, amp) in [(1.0, 0.5), (2.76, 0.3), (5.4, 0.15)] {
        add_mode(out, at, Mode { freq: freq * ratio, amp, tau: 0.05, glide: 1.0, glide_tau: 1.0 });
    }
}

pub(super) fn render(cue: Cue, rng: &mut Rng) -> Vec<f32> {
    match cue {
        Cue::SmashAir => {
            let mut out = vec![0.0; samples(0.6)];
            whoosh(&mut out, rng, 0.0, 0.25, 1800.0, 600.0, 0.6);
            thud(&mut out, samples(0.18), rng.range(150.0, 170.0), 0.09, 0.8);
            dsp::finish(out, 0.6)
        }
        Cue::SmashGround | Cue::SmashHeavy => {
            let heavy = cue == Cue::SmashHeavy;
            let mut out = vec![0.0; samples(if heavy { 1.4 } else { 0.9 })];
            thud(
                &mut out,
                0,
                rng.range(55.0, 65.0) * if heavy { 0.8 } else { 1.0 },
                if heavy { 0.3 } else { 0.18 },
                0.6,
            );
            let mut rubble = noise(rng, samples(0.7), |t| dsp::ad(t, 0.005, if heavy { 0.25 } else { 0.12 }));
            Biquad::lowpass(900.0, 0.8).run(&mut rubble);
            mix_into(&mut out, &rubble, 0.7, samples(0.01));
            dsp::finish(out, if heavy { 0.8 } else { 0.65 })
        }
        Cue::WindBurst => {
            let mut out = vec![0.0; samples(1.0)];
            whoosh(&mut out, rng, 0.0, 0.8, 300.0, 2400.0, 0.9);
            add_mode(&mut out, 0, Mode { freq: 70.0, amp: 0.6, tau: 0.12, glide: 0.5, glide_tau: 0.1 });
            dsp::finish(out, 0.6)
        }
        Cue::TridentThrow => {
            let mut out = vec![0.0; samples(0.5)];
            whoosh(&mut out, rng, 0.0, 0.4, 2600.0, 900.0, 0.8);
            dsp::finish(out, 0.5)
        }
        Cue::TridentHit => {
            let mut out = vec![0.0; samples(0.4)];
            clink(&mut out, 0, rng.range(1300.0, 1500.0));
            let mut flesh = noise(rng, samples(0.15), |t| dsp::ad(t, 0.002, 0.03));
            Biquad::lowpass(1200.0, 0.7).run(&mut flesh);
            mix_into(&mut out, &flesh, 0.8, 0);
            dsp::finish(out, 0.55)
        }
        Cue::TridentHitGround => {
            let mut out = vec![0.0; samples(0.5)];
            clink(&mut out, 0, rng.range(900.0, 1100.0));
            thud(&mut out, 0, rng.range(120.0, 140.0), 0.05, 0.3);
            dsp::finish(out, 0.5)
        }
        Cue::TridentReturn => {
            let mut out = vec![0.0; samples(1.2)];
            for (i, f) in [880.0f32, 1175.0, 1480.0].into_iter().enumerate() {
                add_mode(
                    &mut out,
                    samples(i as f32 * 0.12),
                    Mode { freq: f, amp: 0.3, tau: 0.35, glide: 1.0, glide_tau: 1.0 },
                );
            }
            whoosh(&mut out, rng, 0.0, 1.0, 500.0, 1500.0, 0.3);
            dsp::finish(out, 0.4)
        }
        Cue::Riptide1 | Cue::Riptide2 | Cue::Riptide3 => {
            let level = match cue {
                Cue::Riptide1 => 1.0,
                Cue::Riptide2 => 1.3,
                _ => 1.6,
            };
            let mut out = vec![0.0; samples(1.0)];
            let mut water = noise(rng, samples(0.9), |t| dsp::ad(t, 0.05, 0.3));
            Biquad::bandpass(700.0 * level, 0.9).run(&mut water);
            mix_into(&mut out, &water, 0.9, 0);
            whoosh(&mut out, rng, 0.05, 0.7, 400.0 * level, 2200.0 * level, 0.7);
            dsp::finish(out, 0.6)
        }
        Cue::TridentThunder => {
            let mut out = vec![0.0; samples(2.0)];
            let mut crack = noise(rng, samples(1.8), |t| dsp::ad(t, 0.004, 0.5));
            Biquad::lowpass(500.0, 0.7).run(&mut crack);
            mix_into(&mut out, &crack, 1.0, 0);
            clink(&mut out, 0, 2400.0);
            dsp::finish(out, 0.8)
        }
        Cue::SpearUse | Cue::SpearWoodUse => {
            let mut out = vec![0.0; samples(0.4)];
            let f = if cue == Cue::SpearWoodUse { 400.0 } else { 900.0 };
            whoosh(&mut out, rng, 0.0, 0.3, f, f * 1.8, 0.5);
            dsp::finish(out, 0.35)
        }
        Cue::SpearHit | Cue::SpearWoodHit => {
            let mut out = vec![0.0; samples(0.35)];
            if cue == Cue::SpearWoodHit {
                thud(&mut out, 0, rng.range(210.0, 240.0), 0.05, 0.2);
            } else {
                clink(&mut out, 0, rng.range(1600.0, 1800.0));
                thud(&mut out, 0, rng.range(160.0, 180.0), 0.04, 0.2);
            }
            dsp::finish(out, 0.55)
        }
        Cue::SpearAttack | Cue::SpearWoodAttack => {
            let mut out = vec![0.0; samples(0.3)];
            let f = if cue == Cue::SpearWoodAttack { 700.0 } else { 1400.0 };
            whoosh(&mut out, rng, 0.0, 0.18, f * 1.6, f, 0.6);
            dsp::finish(out, 0.4)
        }
        Cue::Lunge1 | Cue::Lunge2 | Cue::Lunge3 => {
            let level = match cue {
                Cue::Lunge1 => 1.0,
                Cue::Lunge2 => 1.25,
                _ => 1.5,
            };
            let mut out = vec![0.0; samples(0.5)];
            whoosh(&mut out, rng, 0.0, 0.35, 500.0 * level, 1600.0 * level, 0.8);
            dsp::finish(out, 0.45)
        }
        Cue::DrownedShoot => {
            let mut out = vec![0.0; samples(0.6)];
            whoosh(&mut out, rng, 0.0, 0.4, 1600.0, 500.0, 0.6);
            let mut gurgle = noise(rng, samples(0.3), |t| dsp::ad(t, 0.02, 0.1));
            Biquad::bandpass(350.0, 2.0).run(&mut gurgle);
            mix_into(&mut out, &gurgle, 0.6, 0);
            dsp::finish(out, 0.45)
        }
    }
}
