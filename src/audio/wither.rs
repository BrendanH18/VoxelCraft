//! Original procedural stand-ins for v0.16 Track C's sound events: the
//! Wither's arrival (`entity.wither.spawn`), shots (`entity.wither.shoot`)
//! and block breaking (`entity.wither.break_block`), and the beacon's
//! `block.beacon.activate`, `ambient`, `power_select` and `deactivate`.
//! Nothing is sampled: roars are noise through moving band-passes, the
//! beacon's hum a few slowly beating sine partials.

use super::dsp::{self, Biquad, Mode, Rng, add_mode, crackle, mix_into, noise, samples};
use crate::entity::wither::Cue;
use crate::world::beacon::Cue as BeaconCue;

/// A roar: noise through a band-pass that sweeps from `from` to `to` Hz.
fn roar(out: &mut [f32], rng: &mut Rng, len: f32, from: f32, to: f32, gain: f32) {
    let n = samples(len);
    let raw = noise(rng, n, |t| (t / 0.08).min(1.0) * (1.0 - t / len).max(0.0).powf(0.7));
    // Sweep in short blocks so the filter follows the pitch.
    let block = 256;
    let mut filtered = Vec::with_capacity(n);
    for (i, chunk) in raw.chunks(block).enumerate() {
        let t = (i * block) as f32 / n as f32;
        let mut c = chunk.to_vec();
        Biquad::bandpass(from + (to - from) * t, 2.5).run(&mut c);
        filtered.extend(c);
    }
    mix_into(out, &filtered, gain, 0);
}

pub(super) fn render(cue: Cue, rng: &mut Rng) -> Vec<f32> {
    match cue {
        Cue::Spawn => {
            // A long, rising-then-falling three-throated howl over a rumble.
            let mut out = vec![0.0; samples(4.0)];
            roar(&mut out, rng, 3.8, 180.0, 520.0, 0.8);
            roar(&mut out, rng, 3.6, 260.0, 140.0, 0.6);
            let mut rumble = noise(rng, samples(4.0), |t| (1.0 - t / 4.0).max(0.0));
            Biquad::lowpass(120.0, 0.9).run(&mut rumble);
            mix_into(&mut out, &rumble, 1.2, 0);
            dsp::finish(out, 0.7)
        }
        Cue::Shoot => {
            // A short hollow cough with a puff of air.
            let mut out = vec![0.0; samples(0.5)];
            roar(&mut out, rng, 0.35, 420.0, 260.0, 0.9);
            add_mode(&mut out, 0, Mode { freq: 110.0, amp: 0.5, tau: 0.08, glide: 0.6, glide_tau: 0.05 });
            dsp::finish(out, 0.6)
        }
        Cue::BreakBlocks => {
            // Stone giving way: a heavy thud and a spray of crackle.
            let mut out = vec![0.0; samples(1.0)];
            add_mode(&mut out, 0, Mode { freq: 70.0, amp: 0.9, tau: 0.25, glide: 0.7, glide_tau: 0.1 });
            let mut c = crackle(rng, samples(0.9), (5.0, 25.0), |t| 220.0 * (1.0 - t / 0.9).max(0.0));
            Biquad::lowpass(2400.0, 0.7).run(&mut c);
            mix_into(&mut out, &c, 0.7, 0);
            dsp::finish(out, 0.65)
        }
    }
}

pub(super) fn render_beacon(cue: BeaconCue, rng: &mut Rng) -> Vec<f32> {
    // A glassy chord whose partials beat slowly against each other.
    let hum = |out: &mut [f32], base: f32, glide: f32, gain: f32, tau: f32| {
        for (ratio, amp) in [(1.0, 0.5), (1.5, 0.3), (2.0, 0.22), (3.01, 0.1)] {
            add_mode(out, 0, Mode { freq: base * ratio, amp: amp * gain, tau, glide, glide_tau: tau * 0.6 });
            add_mode(
                out,
                0,
                Mode { freq: base * ratio * 1.004, amp: amp * gain * 0.6, tau, glide, glide_tau: tau * 0.6 },
            );
        }
    };
    let base = 220.0 * rng.range(0.99, 1.01);
    match cue {
        BeaconCue::Activate => {
            let mut out = vec![0.0; samples(2.5)];
            hum(&mut out, base * 0.7, 1.45, 0.9, 1.2);
            dsp::finish(out, 0.6)
        }
        BeaconCue::Deactivate => {
            let mut out = vec![0.0; samples(2.5)];
            hum(&mut out, base, 0.6, 0.9, 1.0);
            dsp::finish(out, 0.6)
        }
        BeaconCue::Ambient => {
            let mut out = vec![0.0; samples(3.0)];
            hum(&mut out, base, 1.0, 0.6, 2.0);
            dsp::finish(out, 0.45)
        }
        BeaconCue::PowerSelect => {
            let mut out = vec![0.0; samples(1.4)];
            hum(&mut out, base * 1.5, 1.2, 0.8, 0.5);
            dsp::finish(out, 0.6)
        }
    }
}
