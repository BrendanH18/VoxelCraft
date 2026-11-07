//! Original procedural stand-ins for Java's warden (`entity.warden.*`) and
//! sculk block (`block.sculk*`, `block.sculk_shrieker.shriek`,
//! `block.sculk_catalyst.bloom`, `block.amethyst_block.resonate`) sound
//! events. Nothing is sampled: every cue is built from damped modes,
//! filtered noise and crackle.

use super::dsp::{self, Biquad, Mode, Rng, add_mode, crackle, mix_into, noise, samples};
use crate::entity::warden::Cue;
use crate::world::sculk::SculkCue;

/// A deep chest resonance: stacked low modes with a downward glide.
fn growl(out: &mut [f32], at: usize, freq: f32, tau: f32, glide: f32, gain: f32) {
    for (ratio, amp) in [(1.0, 0.7), (1.51, 0.35), (2.02, 0.22), (3.07, 0.1)] {
        add_mode(out, at, Mode { freq: freq * ratio, amp: amp * gain, tau, glide, glide_tau: tau * 1.5 });
    }
}

/// Breath through a wide throat: band-passed noise under an envelope.
fn breath(out: &mut [f32], rng: &mut Rng, at: f32, len: f32, fc: f32, q: f32, gain: f32) {
    let mut n = noise(rng, samples(len), |t| (t / (len * 0.3)).min(1.0) * (1.0 - t / len).max(0.0));
    Biquad::bandpass(fc, q).run(&mut n);
    mix_into(out, &n, gain, samples(at));
}

/// Dry, bony clicks.
fn clicks(out: &mut [f32], rng: &mut Rng, at: f32, count: usize, spacing: f32, fc: f32) {
    for i in 0..count {
        let mut c = crackle(rng, samples(0.03), (0.3, 1.2), |t| 3000.0 * (-t / 0.006).exp());
        Biquad::bandpass(fc * rng.range(0.85, 1.15), 2.5).run(&mut c);
        mix_into(out, &c, 1.0, samples(at + i as f32 * spacing * rng.range(0.8, 1.2)));
    }
}

/// Soil and stone shifting.
fn rumble(out: &mut [f32], rng: &mut Rng, at: f32, len: f32, density: f32) {
    let mut c = crackle(rng, samples(len), (6.0, 25.0), |t| density * (t / 0.2).min(1.0) * (1.0 - t / len).max(0.0));
    Biquad::lowpass(rng.range(420.0, 620.0), 0.7).run(&mut c);
    mix_into(out, &c, 1.0, samples(at));
}

pub(super) fn render(cue: Cue, rng: &mut Rng) -> Vec<f32> {
    match cue {
        // A slow, wet exhale over a low drone.
        Cue::Ambient => {
            let mut out = vec![0.; samples(1.6)];
            growl(&mut out, 0, rng.range(48.0, 56.0), 0.5, 0.9, 0.8);
            let fc = rng.range(260.0, 340.0);
            breath(&mut out, rng, 0.1, 1.3, fc, 0.9, 0.6);
            dsp::finish(out, 0.5)
        }
        // A restless snarl that rises.
        Cue::Agitated => {
            let mut out = vec![0.; samples(1.2)];
            growl(&mut out, 0, rng.range(58.0, 66.0), 0.35, 1.25, 1.0);
            let fc = rng.range(380.0, 460.0);
            breath(&mut out, rng, 0.0, 1.0, fc, 1.2, 0.7);
            dsp::finish(out, 0.55)
        }
        // A rattling bellow.
        Cue::Angry => {
            let mut out = vec![0.; samples(1.4)];
            for k in 0..4 {
                growl(&mut out, samples(k as f32 * 0.09), rng.range(66.0, 78.0), 0.3, 0.85, 0.7);
            }
            let fc = rng.range(500.0, 600.0);
            breath(&mut out, rng, 0.0, 1.2, fc, 1.0, 0.8);
            dsp::finish(out, 0.6)
        }
        // Tendrils perking up: a faint quiver of clicks.
        Cue::Listening => {
            let mut out = vec![0.; samples(0.8)];
            clicks(&mut out, rng, 0.0, 6, 0.09, 2400.0);
            growl(&mut out, samples(0.1), rng.range(52.0, 60.0), 0.25, 1.05, 0.4);
            dsp::finish(out, 0.4)
        }
        Cue::ListeningAngry => {
            let mut out = vec![0.; samples(1.0)];
            clicks(&mut out, rng, 0.0, 9, 0.07, 2000.0);
            growl(&mut out, samples(0.05), rng.range(60.0, 70.0), 0.35, 1.2, 0.8);
            dsp::finish(out, 0.5)
        }
        // A deep double thump.
        Cue::Heartbeat => {
            let mut out = vec![0.; samples(0.7)];
            for (at, f) in [(0.0, 52.0), (0.22, 44.0)] {
                add_mode(&mut out, samples(at), Mode { freq: f, amp: 1.0, tau: 0.07, glide: 0.6, glide_tau: 0.05 });
                let mut n = noise(rng, samples(0.06), |t| dsp::ad(t, 0.002, 0.015));
                Biquad::lowpass(180.0, 0.7).run(&mut n);
                mix_into(&mut out, &n, 0.4, samples(at));
            }
            dsp::finish(out, 0.6)
        }
        Cue::TendrilClicks => {
            let mut out = vec![0.; samples(0.6)];
            clicks(&mut out, rng, 0.0, 10, 0.045, 2800.0);
            dsp::finish(out, 0.45)
        }
        // A long roar: rising growl, a shuddering peak, falling off.
        Cue::Roar => {
            let mut out = vec![0.; samples(3.0)];
            for k in 0..14 {
                let at = k as f32 * 0.16;
                let f = rng.range(70.0, 92.0) * (1.0 + (k as f32 / 6.0).min(1.0) * 0.25);
                growl(&mut out, samples(at), f, 0.3, 0.9, 0.6);
            }
            breath(&mut out, rng, 0.0, 2.8, 520.0, 0.6, 1.0);
            dsp::finish(out, 0.7)
        }
        // Three deep sniffs.
        Cue::Sniff => {
            let mut out = vec![0.; samples(1.3)];
            for k in 0..3 {
                let fc = rng.range(600.0, 800.0);
                breath(&mut out, rng, k as f32 * 0.35, 0.22, fc, 1.8, 1.0);
            }
            dsp::finish(out, 0.45)
        }
        // Crawling out of the ground.
        Cue::Emerge => {
            let mut out = vec![0.; samples(3.5)];
            rumble(&mut out, rng, 0.0, 3.4, 220.0);
            growl(&mut out, samples(1.8), rng.range(46.0, 54.0), 0.6, 1.2, 0.8);
            dsp::finish(out, 0.6)
        }
        // Burrowing away.
        Cue::Dig => {
            let mut out = vec![0.; samples(3.0)];
            rumble(&mut out, rng, 0.0, 2.9, 260.0);
            growl(&mut out, 0, rng.range(50.0, 58.0), 0.5, 0.7, 0.6);
            dsp::finish(out, 0.6)
        }
        // A heavy blow.
        Cue::AttackImpact => {
            let mut out = vec![0.; samples(0.5)];
            add_mode(
                &mut out,
                0,
                Mode { freq: rng.range(70.0, 85.0), amp: 1.0, tau: 0.08, glide: 0.5, glide_tau: 0.05 },
            );
            let mut n = noise(rng, samples(0.15), |t| dsp::ad(t, 0.001, 0.03));
            Biquad::lowpass(900.0, 0.7).run(&mut n);
            mix_into(&mut out, &n, 0.8, 0);
            dsp::finish(out, 0.7)
        }
        // Drawing in power: a rising whine over a swelling hum.
        Cue::SonicCharge => {
            let mut out = vec![0.; samples(1.7)];
            add_mode(&mut out, 0, Mode { freq: 180.0, amp: 0.5, tau: 0.6, glide: 3.2, glide_tau: 0.8 });
            add_mode(&mut out, 0, Mode { freq: 60.0, amp: 0.7, tau: 0.7, glide: 1.5, glide_tau: 0.9 });
            breath(&mut out, rng, 0.0, 1.6, 900.0, 3.0, 0.5);
            dsp::finish(out, 0.55)
        }
        // The blast: a sharp crack and a ringing sweep.
        Cue::SonicBoom => {
            let mut out = vec![0.; samples(1.5)];
            let mut n = noise(rng, samples(0.4), |t| dsp::ad(t, 0.001, 0.06));
            Biquad::bandpass(1200.0, 0.6).run(&mut n);
            mix_into(&mut out, &n, 1.0, 0);
            add_mode(&mut out, 0, Mode { freq: 520.0, amp: 0.8, tau: 0.25, glide: 0.25, glide_tau: 0.2 });
            add_mode(&mut out, 0, Mode { freq: 45.0, amp: 1.0, tau: 0.35, glide: 0.8, glide_tau: 0.3 });
            dsp::finish(out, 0.75)
        }
        Cue::Hurt => {
            let mut out = vec![0.; samples(0.7)];
            growl(&mut out, 0, rng.range(80.0, 95.0), 0.15, 1.3, 1.0);
            breath(&mut out, rng, 0.0, 0.5, 700.0, 1.4, 0.6);
            dsp::finish(out, 0.55)
        }
        Cue::Death => {
            let mut out = vec![0.; samples(2.4)];
            growl(&mut out, 0, rng.range(64.0, 72.0), 0.9, 0.4, 1.0);
            breath(&mut out, rng, 0.0, 2.2, 360.0, 0.8, 0.8);
            rumble(&mut out, rng, 0.8, 1.5, 120.0);
            dsp::finish(out, 0.6)
        }
        // A heavy, padded footfall.
        Cue::Step => {
            let mut out = vec![0.; samples(0.35)];
            add_mode(
                &mut out,
                0,
                Mode { freq: rng.range(55.0, 65.0), amp: 1.0, tau: 0.06, glide: 0.7, glide_tau: 0.05 },
            );
            let mut n = noise(rng, samples(0.12), |t| dsp::ad(t, 0.003, 0.03));
            Biquad::lowpass(500.0, 0.7).run(&mut n);
            mix_into(&mut out, &n, 0.6, 0);
            dsp::finish(out, 0.5)
        }
        // The distant replies to a shriek, ever closer.
        Cue::NearbyClose | Cue::NearbyCloser | Cue::NearbyClosest => {
            let level = match cue {
                Cue::NearbyClose => 0.0,
                Cue::NearbyCloser => 1.0,
                _ => 2.0,
            };
            let mut out = vec![0.; samples(2.0)];
            growl(&mut out, 0, rng.range(40.0, 46.0) + level * 6.0, 0.5 + level * 0.1, 1.1, 0.6 + level * 0.15);
            let mut n = noise(rng, samples(1.8), |t| (t / 0.6).min(1.0) * (1.0 - t / 1.8).max(0.0));
            Biquad::lowpass(250.0 + level * 120.0, 0.7).run(&mut n);
            mix_into(&mut out, &n, 0.5, 0);
            dsp::finish(out, 0.4 + level * 0.1)
        }
    }
}

pub(super) fn render_sculk(cue: SculkCue, rng: &mut Rng) -> Vec<f32> {
    match cue {
        // A sensor's tendrils rattling.
        SculkCue::Clicking => {
            let mut out = vec![0.; samples(0.7)];
            clicks(&mut out, rng, 0.0, 12, 0.05, 3200.0);
            dsp::finish(out, 0.4)
        }
        SculkCue::ClickingStop => {
            let mut out = vec![0.; samples(0.3)];
            clicks(&mut out, rng, 0.0, 3, 0.07, 2600.0);
            dsp::finish(out, 0.3)
        }
        // A bright crystal chime (the playback pitch sets the note).
        SculkCue::Resonate => {
            let mut out = vec![0.; samples(1.4)];
            for (ratio, amp, tau) in [(1.0, 0.8, 0.5), (2.76, 0.3, 0.25), (5.4, 0.15, 0.12)] {
                add_mode(&mut out, 0, Mode { freq: 880.0 * ratio, amp, tau, glide: 1.0, glide_tau: 1.0 });
            }
            dsp::finish(out, 0.4)
        }
        // A soulful whoosh as the catalyst blooms.
        SculkCue::Bloom => {
            let mut out = vec![0.; samples(1.2)];
            let fc = rng.range(700.0, 900.0);
            breath(&mut out, rng, 0.0, 1.1, fc, 1.5, 1.0);
            add_mode(&mut out, samples(0.1), Mode { freq: 220.0, amp: 0.4, tau: 0.3, glide: 1.5, glide_tau: 0.4 });
            dsp::finish(out, 0.45)
        }
        // Wet creeping growth.
        SculkCue::Spread | SculkCue::Grow => {
            let mut out = vec![0.; samples(0.5)];
            let mut c = crackle(rng, samples(0.45), (2.0, 8.0), |t| 400.0 * (1.0 - t / 0.45).max(0.0));
            Biquad::bandpass(rng.range(600.0, 900.0), 1.2).run(&mut c);
            mix_into(&mut out, &c, 1.0, 0);
            if cue == SculkCue::Grow {
                add_mode(&mut out, 0, Mode { freq: 140.0, amp: 0.5, tau: 0.08, glide: 0.8, glide_tau: 0.05 });
            }
            dsp::finish(out, 0.4)
        }
        // A piercing, wavering shriek.
        SculkCue::Shriek => {
            let mut out = vec![0.; samples(2.5)];
            let len = samples(2.4);
            let mut phase = 0.0f32;
            for (i, s) in out.iter_mut().enumerate().take(len) {
                let t = i as f32 / dsp::RATE;
                let f = 900.0 + 260.0 * (t * 9.0).sin() + 140.0 * (t * 23.0).sin();
                phase = (phase + std::f32::consts::TAU * f / dsp::RATE) % std::f32::consts::TAU;
                let env = (t / 0.15).min(1.0) * (1.0 - t / 2.4).max(0.0);
                *s += (phase.sin() + 0.3 * (2.0 * phase).sin()) * env * 0.6;
            }
            breath(&mut out, rng, 0.0, 2.3, 1400.0, 1.0, 0.4);
            dsp::finish(out, 0.55)
        }
    }
}
