//! v1.0 gap Track 2f: original procedural stand-ins for Java 26.3's fox
//! sounds beyond its ambient, hurt and death calls (`entity.fox.aggro`,
//! `.bite`, `.eat`, `.screech`, `.sleep`, `.sniff` and `.spit`). Nothing is
//! sampled: each is the shared voice model (a band-limited glottal buzz
//! through three formants, the fox's own vowel) over filtered noise and
//! damped modes, as the other creature sounds are.

use super::dsp::{self, Biquad, Mode, Rng, add_mode, mix_into, noise, samples};
use super::voices::{FLAT, Utterance, lerp, swell, utter};
use crate::entity::fox::Cue;

/// The fox's vowel (`voices::animal_voice`'s formants for it).
const FOX: [(f32, f32, f32); 3] = [(950.0, 4.0, 1.0), (1800.0, 5.0, 0.6), (3200.0, 7.0, 0.25)];

/// A voiced call from `from` to `to` Hz over `secs`, with `rasp` breath.
fn call(rng: &mut Rng, secs: f32, from: f32, to: f32, rasp: f32, formants: [(f32, f32, f32); 3]) -> Vec<f32> {
    utter(
        rng,
        &Utterance {
            secs,
            f0: &|t| lerp(from, to, t / secs),
            env: &|t| swell(t, 0.01, secs * 0.5, secs),
            formants,
            shift: FLAT,
            jitter: 0.1,
            breath: rasp,
        },
    )
}

/// A short burst of band-passed noise at `at` seconds: a sniff, a puff of
/// breath or the snap of a jaw.
fn puff(out: &mut [f32], rng: &mut Rng, at: f32, len: f32, fc: f32, q: f32, gain: f32) {
    let mut n = noise(rng, samples(len), |t| (t / (len * 0.2)).min(1.0) * (-(t / (len * 0.35))).exp());
    Biquad::bandpass(fc, q).run(&mut n);
    mix_into(out, &n, gain, samples(at));
}

/// `entity.fox.aggro`: a low, rattling snarl that rises and breaks off.
fn aggro(rng: &mut Rng) -> Vec<f32> {
    let base = rng.range(300.0, 360.0);
    let mut out = call(rng, 0.45, base, base * 1.35, 0.9, [(700.0, 3.0, 1.0), (1500.0, 4.0, 0.6), (2900.0, 5.0, 0.3)]);
    // The growl's flutter.
    for (i, v) in out.iter_mut().enumerate() {
        let t = i as f32 / dsp::RATE;
        *v *= 0.65 + 0.35 * (t * 38.0 * std::f32::consts::TAU).sin().abs();
    }
    dsp::finish(out, 0.5)
}

/// `entity.fox.bite`: a quick yap and the teeth snapping shut.
fn bite(rng: &mut Rng) -> Vec<f32> {
    let mut out = vec![0.0; samples(0.25)];
    let from = rng.range(650.0, 720.0);
    let yap = call(rng, 0.09, from, 520.0, 0.5, FOX);
    mix_into(&mut out, &yap, 0.7, 0);
    add_mode(&mut out, samples(0.07), Mode { freq: 1900.0, amp: 0.5, tau: 0.012, glide: -0.2, glide_tau: 0.01 });
    puff(&mut out, rng, 0.07, 0.04, 3200.0, 1.2, 0.6);
    dsp::finish(out, 0.55)
}

/// `entity.fox.eat`: wet chewing, three or four crunches.
fn eat(rng: &mut Rng) -> Vec<f32> {
    let mut out = vec![0.0; samples(0.6)];
    let chews = 3 + (rng.range(0.0, 1.99) as usize);
    for k in 0..chews {
        let at = 0.03 + k as f32 * rng.range(0.13, 0.16);
        let fc = rng.range(1500.0, 2400.0);
        puff(&mut out, rng, at, 0.07, fc, 1.6, 0.8);
        let freq = rng.range(260.0, 320.0);
        add_mode(&mut out, samples(at), Mode { freq, amp: 0.3, tau: 0.03, glide: 0.0, glide_tau: 0.01 });
    }
    dsp::finish(out, 0.45)
}

/// `entity.fox.screech`: the vixen's long, hoarse scream at night.
fn screech(rng: &mut Rng) -> Vec<f32> {
    let secs = rng.range(0.8, 1.0);
    let peak = rng.range(1050.0, 1200.0);
    let out = utter(
        rng,
        &Utterance {
            secs,
            f0: &|t| {
                let x = t / secs;
                // Up fast, a wavering hold, then down.
                let rise = (x / 0.15).min(1.0);
                let fall = ((1.0 - x) / 0.4).clamp(0.0, 1.0);
                (420.0 + (peak - 420.0) * rise * (0.6 + 0.4 * fall)) * (1.0 + 0.03 * (t * 31.0).sin())
            },
            env: &|t| swell(t, 0.03, secs * 0.45, secs),
            formants: [(1300.0, 3.0, 1.0), (2400.0, 4.0, 0.7), (3900.0, 6.0, 0.35)],
            shift: &|t| 1.0 + 0.1 * (t * 9.0).sin(),
            jitter: 0.18,
            breath: 1.1,
        },
    );
    dsp::finish(out, 0.6)
}

/// `entity.fox.sleep`: a slow breath in and a soft whistling breath out.
fn sleep(rng: &mut Rng) -> Vec<f32> {
    let mut out = vec![0.0; samples(1.4)];
    let mut inhale = noise(rng, samples(0.6), |t| swell(t, 0.25, 0.2, 0.6) * 0.6);
    Biquad::bandpass(1100.0, 0.9).run(&mut inhale);
    mix_into(&mut out, &inhale, 0.5, 0);
    let mut exhale = noise(rng, samples(0.7), |t| swell(t, 0.1, 0.45, 0.7));
    Biquad::bandpass(700.0, 1.1).run(&mut exhale);
    mix_into(&mut out, &exhale, 0.6, samples(0.65));
    let whistle = call(rng, 0.5, 520.0, 470.0, 2.5, FOX);
    mix_into(&mut out, &whistle, 0.12, samples(0.7));
    dsp::finish(out, 0.25)
}

/// `entity.fox.sniff`: two or three quick sniffs.
fn sniff(rng: &mut Rng) -> Vec<f32> {
    let mut out = vec![0.0; samples(0.4)];
    let sniffs = 2 + (rng.range(0.0, 1.99) as usize);
    for k in 0..sniffs {
        let fc = rng.range(2600.0, 3400.0);
        puff(&mut out, rng, k as f32 * 0.11, 0.07, fc, 2.0, 0.9);
    }
    dsp::finish(out, 0.4)
}

/// `entity.fox.spit`: a short, puffed-out "ptoo".
fn spit(rng: &mut Rng) -> Vec<f32> {
    let mut out = vec![0.0; samples(0.3)];
    puff(&mut out, rng, 0.0, 0.05, 2500.0, 1.0, 0.9);
    let lip = call(rng, 0.12, 380.0, 300.0, 0.8, [(500.0, 3.0, 1.0), (1100.0, 4.0, 0.4), (2600.0, 5.0, 0.2)]);
    mix_into(&mut out, &lip, 0.5, samples(0.03));
    add_mode(&mut out, samples(0.02), Mode { freq: 900.0, amp: 0.25, tau: 0.02, glide: -0.3, glide_tau: 0.02 });
    dsp::finish(out, 0.45)
}

pub(super) fn render(cue: Cue, rng: &mut Rng) -> Vec<f32> {
    match cue {
        Cue::Aggro => aggro(rng),
        Cue::Bite => bite(rng),
        Cue::Eat => eat(rng),
        Cue::Screech => screech(rng),
        Cue::Sleep => sleep(rng),
        Cue::Sniff => sniff(rng),
        Cue::Spit => spit(rng),
    }
}
