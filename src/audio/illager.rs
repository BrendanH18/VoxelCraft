//! Original procedural stand-ins for v0.15 Track A's sound events: spells
//! preparing and casting, evoker fangs, the vex's charge, the ravager's
//! bite, stun and roar, the crossbow and the raiders' celebrations. Nothing
//! is sampled.

use super::dsp::{self, Biquad, Mode, Rng, add_mode, mix_into, noise, samples};
use crate::entity::illager::{Cue, Spell};

/// A rising, shimmering chord of damped modes (a spell gathering).
fn chime(out: &mut [f32], at: f32, base: f32, tau: f32, glide: f32, gain: f32) {
    for (ratio, amp) in [(1.0, 0.45), (1.5, 0.3), (2.0, 0.2), (3.02, 0.1)] {
        add_mode(out, samples(at), Mode { freq: base * ratio, amp: amp * gain, tau, glide, glide_tau: tau * 0.8 });
    }
}

/// Band-passed noise with an attack-decay envelope.
fn burst(out: &mut [f32], rng: &mut Rng, at: f32, len: f32, fc: f32, q: f32, gain: f32) {
    let mut n = noise(rng, samples(len), |t| (t / 0.01).min(1.0) * (1.0 - t / len).max(0.0).powi(2));
    Biquad::bandpass(fc, q).run(&mut n);
    mix_into(out, &n, gain, samples(at));
}

pub(super) fn render(cue: Cue, rng: &mut Rng) -> Vec<f32> {
    let mut out = vec![0.0; samples(1.4)];
    let jitter = rng.range(0.95, 1.05);
    match cue {
        Cue::Prepare(spell) => {
            let base = match spell {
                Spell::SummonVex => 330.0,
                Spell::Fangs => 220.0,
                Spell::Wololo => 280.0,
                Spell::Disappear => 390.0,
                _ => 180.0,
            } * jitter;
            chime(&mut out, 0.0, base, 0.5, 1.6, 0.8);
            burst(&mut out, rng, 0.0, 0.6, base * 6.0, 3.0, 0.25);
        }
        Cue::Cast => {
            chime(&mut out, 0.0, 520.0 * jitter, 0.3, 0.7, 0.9);
            burst(&mut out, rng, 0.0, 0.3, 3000.0, 1.5, 0.4);
        }
        Cue::Fangs => {
            // A snap of jaws: two quick wooden knocks over a crunch.
            for (at, freq) in [(0.0, 420.0), (0.06, 300.0)] {
                add_mode(
                    &mut out,
                    samples(at),
                    Mode { freq: freq * jitter, amp: 0.7, tau: 0.05, glide: 0.8, glide_tau: 0.05 },
                );
            }
            burst(&mut out, rng, 0.0, 0.18, 1800.0, 0.9, 0.7);
            out.truncate(samples(0.4));
        }
        Cue::VexCharge => {
            let mut n = noise(rng, samples(0.5), |t| (t / 0.5).powi(2) * (1.0 - t / 0.5).max(0.0) * 4.0);
            Biquad::bandpass(2400.0 * jitter, 2.5).run(&mut n);
            mix_into(&mut out, &n, 0.8, 0);
            chime(&mut out, 0.0, 900.0 * jitter, 0.35, 1.8, 0.4);
            out.truncate(samples(0.6));
        }
        Cue::RavagerAttack => {
            burst(&mut out, rng, 0.0, 0.25, 260.0, 0.8, 1.0);
            add_mode(&mut out, 0, Mode { freq: 70.0 * jitter, amp: 0.8, tau: 0.12, glide: 0.7, glide_tau: 0.1 });
            out.truncate(samples(0.4));
        }
        Cue::Stunned => {
            for i in 0..4 {
                let at = i as f32 * 0.12;
                add_mode(
                    &mut out,
                    samples(at),
                    Mode { freq: (900.0 - 120.0 * i as f32) * jitter, amp: 0.4, tau: 0.1, glide: 1.0, glide_tau: 0.1 },
                );
            }
            out.truncate(samples(0.7));
        }
        Cue::Roar => {
            let mut n = noise(rng, out.len(), |t| (t / 0.08).min(1.0) * (1.0 - t / 1.3).max(0.0));
            Biquad::lowpass(500.0, 0.8).run(&mut n);
            mix_into(&mut out, &n, 1.0, 0);
            add_mode(&mut out, 0, Mode { freq: 62.0 * jitter, amp: 0.9, tau: 0.7, glide: 0.8, glide_tau: 0.6 });
        }
        Cue::CrossbowLoaded => {
            add_mode(&mut out, 0, Mode { freq: 1500.0 * jitter, amp: 0.5, tau: 0.04, glide: 1.0, glide_tau: 0.04 });
            add_mode(
                &mut out,
                samples(0.05),
                Mode { freq: 950.0 * jitter, amp: 0.6, tau: 0.06, glide: 1.0, glide_tau: 0.06 },
            );
            out.truncate(samples(0.3));
        }
        Cue::CrossbowShoot => {
            burst(&mut out, rng, 0.0, 0.2, 1100.0, 0.9, 0.9);
            add_mode(&mut out, 0, Mode { freq: 180.0 * jitter, amp: 0.6, tau: 0.08, glide: 0.6, glide_tau: 0.08 });
            out.truncate(samples(0.35));
        }
        Cue::DoorAttack => {
            // A heavy blow on planks: a low wooden knock with a dull thud.
            for (freq, amp, tau) in [(110.0, 0.9, 0.09), (235.0, 0.5, 0.06), (520.0, 0.25, 0.04)] {
                add_mode(&mut out, 0, Mode { freq: freq * jitter, amp, tau, glide: 0.92, glide_tau: 0.08 });
            }
            burst(&mut out, rng, 0.0, 0.16, 700.0, 0.8, 0.8);
            out.truncate(samples(0.45));
        }
        Cue::DoorBreak => {
            // Splintering wood: a crack, then a rattle of falling boards.
            burst(&mut out, rng, 0.0, 0.12, 2200.0, 0.7, 1.0);
            add_mode(&mut out, 0, Mode { freq: 95.0 * jitter, amp: 0.9, tau: 0.15, glide: 0.85, glide_tau: 0.12 });
            for i in 0..5 {
                let at = 0.08 + i as f32 * 0.07 + rng.range(0.0, 0.03);
                burst(&mut out, rng, at, 0.09, 900.0 + 260.0 * i as f32, 1.6, 0.45);
                add_mode(
                    &mut out,
                    samples(at),
                    Mode {
                        freq: (300.0 + 70.0 * i as f32) * jitter,
                        amp: 0.25,
                        tau: 0.05,
                        glide: 1.0,
                        glide_tau: 0.05,
                    },
                );
            }
            out.truncate(samples(0.8));
        }
        Cue::Celebrate(_) => {
            // A short, gleeful "hah-hah".
            for i in 0..3 {
                let at = i as f32 * 0.14;
                burst(&mut out, rng, at, 0.1, 700.0 + 80.0 * i as f32, 3.0, 0.6);
                add_mode(
                    &mut out,
                    samples(at),
                    Mode { freq: 160.0 * jitter, amp: 0.4, tau: 0.06, glide: 1.1, glide_tau: 0.06 },
                );
            }
            out.truncate(samples(0.6));
        }
    }
    dsp::finish(out, 0.55)
}
