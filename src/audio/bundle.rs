//! Original procedural stand-ins for the four bundle sounds (v1.0 gap
//! track 6; `item.bundle.insert`, `insert_fail`, `remove_one` and
//! `drop_contents`, all played by the player as `PLAYERS`). Nothing is
//! sampled: a leather pouch rustles as a stack slides in, with a soft
//! thump of it landing; one comes out with a rustle and a light pop; a
//! full pouch answers with a dull double pat; and emptying it is a longer
//! shake ending in the thump of the stack falling out.

use super::dsp::{self, Biquad, Mode, Rng, add_mode, crackle, mix_into, noise, samples};
use voxelcraft::content::bundle::Cue;

/// A short leather rustle: fine crackle through a low band-pass.
fn rustle(rng: &mut Rng, secs: f32, centre: f32) -> Vec<f32> {
    let mut out = crackle(rng, samples(secs), (0.6, 2.2), |t| {
        let x = t / secs;
        1400.0 * (x * 6.0).min(1.0) * (1.0 - x).max(0.0)
    });
    let mut hiss = noise(rng, samples(secs), |t| {
        let x = t / secs;
        (x * 8.0).min(1.0) * (1.0 - x).max(0.0).powf(1.5) * 0.35
    });
    mix_into(&mut hiss, &out, 1.0, 0);
    Biquad::bandpass(centre * rng.range(0.92, 1.08), 1.1).run(&mut hiss);
    out = hiss;
    out
}

/// The soft landing of a stack on leather.
fn thump(out: &mut [f32], at: f32, freq: f32, amp: f32) {
    add_mode(out, samples(at), Mode { freq, amp, tau: 0.045, glide: 0.75, glide_tau: 0.03 });
    add_mode(out, samples(at), Mode { freq: freq * 2.3, amp: amp * 0.3, tau: 0.025, glide: 0.85, glide_tau: 0.02 });
}

pub(super) fn render(cue: Cue, rng: &mut Rng) -> Vec<f32> {
    match cue {
        Cue::Insert => {
            let mut out = vec![0.0; samples(0.4)];
            mix_into(&mut out, &rustle(rng, 0.22, 1300.0), 0.8, 0);
            thump(&mut out, 0.16, rng.range(150.0, 175.0), 0.8);
            dsp::finish(out, 0.55)
        }
        Cue::RemoveOne => {
            let mut out = vec![0.0; samples(0.4)];
            mix_into(&mut out, &rustle(rng, 0.2, 1600.0), 0.8, 0);
            // A light, rising pop as the stack comes free.
            let f = rng.range(380.0, 440.0);
            add_mode(&mut out, samples(0.13), Mode { freq: f, amp: 0.45, tau: 0.03, glide: 1.4, glide_tau: 0.02 });
            dsp::finish(out, 0.55)
        }
        Cue::InsertFail => {
            let mut out = vec![0.0; samples(0.35)];
            thump(&mut out, 0.0, 140.0, 0.9);
            thump(&mut out, 0.09, 120.0, 0.7);
            let mut scuff = rustle(rng, 0.12, 900.0);
            for s in &mut scuff {
                *s *= 0.5;
            }
            mix_into(&mut out, &scuff, 0.6, 0);
            dsp::finish(out, 0.5)
        }
        Cue::DropContents => {
            let mut out = vec![0.0; samples(0.6)];
            mix_into(&mut out, &rustle(rng, 0.38, 1100.0), 0.9, 0);
            mix_into(&mut out, &rustle(rng, 0.2, 1700.0), 0.5, samples(0.12));
            thump(&mut out, 0.34, rng.range(110.0, 130.0), 0.9);
            dsp::finish(out, 0.6)
        }
    }
}
