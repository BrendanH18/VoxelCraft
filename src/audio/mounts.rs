//! v1.0 gap Track 3b: original procedural stand-ins for Java 26.3's camel
//! and camel husk pose and dash sounds (`entity.camel.sit`, `.stand`,
//! `.dash`, `.dash_ready` and the husk's) and the mounts' angry calls
//! (`entity.horse.angry`, `entity.baby_horse.angry`, `entity.donkey.angry`,
//! `entity.mule.angry`, `entity.zombie_horse.angry`, `entity.llama.angry`).
//! Nothing is sampled: each is the shared voice model (a band-limited
//! glottal buzz through three formants) over filtered noise and damped
//! modes, as the other creature sounds are.

use super::dsp::{self, Biquad, Mode, Rng, add_mode, mix_into, noise, samples};
use super::voices::{FLAT, Utterance, lerp, swell, utter};
use crate::entity::mounts::Cue;

/// A soft thump of a heavy body or knee meeting sand: a low damped mode
/// under a short burst of low-passed noise.
fn thump(out: &mut [f32], rng: &mut Rng, at: f32, freq: f32, gain: f32) {
    let start = samples(at);
    add_mode(out, start, Mode { freq, amp: 0.7 * gain, tau: 0.09, glide: -0.3, glide_tau: 0.08 });
    let mut sand = noise(rng, samples(0.12), |t| (-(t / 0.03)).exp() * gain);
    Biquad::lowpass(700.0, 0.7).run(&mut sand);
    mix_into(out, &sand, 0.5, start);
}

/// A breathy grunt from a long throat; `dry` thins the voice and adds
/// rasp for the camel husk.
fn grunt(rng: &mut Rng, secs: f32, from: f32, to: f32, dry: bool) -> Vec<f32> {
    let formants = if dry {
        [(380.0, 2.5, 1.0), (900.0, 3.5, 0.45), (2100.0, 4.0, 0.3)]
    } else {
        [(460.0, 3.5, 1.0), (980.0, 4.5, 0.5), (2300.0, 5.0, 0.2)]
    };
    utter(
        rng,
        &Utterance {
            secs,
            f0: &|t| lerp(from, to, t / secs),
            env: &|t| swell(t, 0.05, secs * 0.5, secs),
            formants,
            shift: FLAT,
            jitter: if dry { 0.25 } else { 0.12 },
            breath: if dry { 1.4 } else { 0.7 },
        },
    )
}

/// A sharp exhale through the nostrils.
fn snort(out: &mut [f32], rng: &mut Rng, at: f32, len: f32, fc: f32, gain: f32) {
    let mut n = noise(rng, samples(len), |t| (t / (len * 0.15)).min(1.0) * (-(t / (len * 0.35))).exp());
    Biquad::bandpass(fc, 1.4).run(&mut n);
    mix_into(out, &n, gain, samples(at));
}

/// The camel lowers itself: a groan as the front kneels, then the knees
/// and the body settling onto the ground.
fn sit(rng: &mut Rng, dry: bool) -> Vec<f32> {
    let mut out = vec![0.0; samples(1.2)];
    let base = rng.range(95.0, 110.0) * if dry { 0.85 } else { 1.0 };
    let g = grunt(rng, 0.5, base * 1.1, base * 0.8, dry);
    mix_into(&mut out, &g, 0.8, 0);
    let (knees, body) = (rng.range(70.0, 80.0), rng.range(55.0, 65.0));
    thump(&mut out, rng, 0.45, knees, 0.8);
    thump(&mut out, rng, 0.8, body, 1.0);
    dsp::finish(out, 0.55)
}

/// The camel heaves itself up: a straining, rising grunt, a shuffle of
/// legs and a final settle.
fn stand(rng: &mut Rng, dry: bool) -> Vec<f32> {
    let mut out = vec![0.0; samples(1.3)];
    let base = rng.range(90.0, 105.0) * if dry { 0.85 } else { 1.0 };
    let g = grunt(rng, 0.7, base * 0.85, base * 1.3, dry);
    mix_into(&mut out, &g, 0.8, 0);
    for (at, f) in [(0.35, 85.0), (0.7, 75.0), (1.0, 90.0)] {
        let f = f * rng.range(0.95, 1.05);
        thump(&mut out, rng, at, f, 0.6);
    }
    dsp::finish(out, 0.5)
}

/// The dash: a forceful snort and a scramble of hooves pushing off.
fn dash(rng: &mut Rng, dry: bool) -> Vec<f32> {
    let mut out = vec![0.0; samples(0.7)];
    snort(&mut out, rng, 0.0, 0.35, if dry { 900.0 } else { 1300.0 }, 1.0);
    for k in 0..4 {
        let f = rng.range(85.0, 110.0);
        thump(&mut out, rng, 0.08 + k as f32 * 0.09, f, 0.7);
    }
    dsp::finish(out, 0.55)
}

/// The dash is ready again: a short, relaxed double snort.
fn dash_ready(rng: &mut Rng, dry: bool) -> Vec<f32> {
    let mut out = vec![0.0; samples(0.5)];
    let fc = if dry { 800.0 } else { 1100.0 };
    snort(&mut out, rng, 0.0, 0.16, fc, 0.7);
    snort(&mut out, rng, 0.2, 0.2, fc * 0.9, 0.8);
    dsp::finish(out, 0.4)
}

/// An angry whinny: a high, shaking call that climbs and falls; `pitch`
/// scales it (foals higher), `rot` makes it hoarse (the zombie horse).
fn whinny(rng: &mut Rng, pitch: f32, rot: bool) -> Vec<f32> {
    let secs = 0.9;
    let top = 520.0 * pitch;
    let base = 220.0 * pitch;
    utter(
        rng,
        &Utterance {
            secs,
            f0: &|t| {
                let x = t / secs;
                let f = if x < 0.25 { lerp(base, top, x / 0.25) } else { lerp(top, base * 0.8, (x - 0.25) / 0.75) };
                f * (1.0 + 0.18 * (t * 70.0).sin())
            },
            env: &|t| swell(t, 0.03, 0.3, secs) * 0.7,
            formants: if rot {
                [(560.0, 3.0, 0.8), (1300.0, 4.0, 0.5), (2500.0, 4.0, 0.3)]
            } else {
                [(700.0, 4.5, 0.8), (1600.0, 5.0, 0.5), (3000.0, 5.0, 0.2)]
            },
            shift: FLAT,
            jitter: if rot { 0.12 } else { 0.04 },
            breath: if rot { 0.9 } else { 0.15 },
        },
    )
}

/// An angry bray: two harsh alternating notes, the in-breath lower than
/// the out-breath; the mule's is shorter and higher, between horse and
/// donkey.
fn bray(rng: &mut Rng, mule: bool) -> Vec<f32> {
    let mut out = vec![0.0; samples(1.2)];
    let (hi, lo, beats) = if mule { (430.0, 260.0, 2) } else { (380.0, 180.0, 3) };
    for k in 0..beats {
        let (f, secs) = if k % 2 == 0 { (hi, 0.22) } else { (lo, 0.26) };
        let f = f * rng.range(0.96, 1.04);
        let note = utter(
            rng,
            &Utterance {
                secs,
                f0: &|t| f * lerp(1.05, 0.95, t / secs),
                env: &|t| swell(t, 0.02, 0.08, secs),
                formants: [(750.0, 3.5, 1.0), (1500.0, 4.0, 0.6), (2600.0, 4.5, 0.3)],
                shift: FLAT,
                jitter: 0.08,
                breath: 0.5,
            },
        );
        mix_into(&mut out, &note, 0.8, samples(k as f32 * 0.3));
    }
    dsp::finish(out, 0.55)
}

/// An angry llama: a nasal bleat cut off by a snort.
fn llama(rng: &mut Rng) -> Vec<f32> {
    let mut out = vec![0.0; samples(0.8)];
    let base = rng.range(300.0, 340.0);
    let bleat = utter(
        rng,
        &Utterance {
            secs: 0.45,
            f0: &|t| base * (1.0 + 0.1 * (t * 40.0).sin()),
            env: &|t| swell(t, 0.03, 0.15, 0.45),
            formants: [(600.0, 6.0, 1.0), (1800.0, 7.0, 0.6), (2900.0, 7.0, 0.25)],
            shift: FLAT,
            jitter: 0.06,
            breath: 0.4,
        },
    );
    mix_into(&mut out, &bleat, 0.8, 0);
    snort(&mut out, rng, 0.45, 0.25, 1200.0, 0.9);
    dsp::finish(out, 0.5)
}

pub(super) fn render(cue: Cue, rng: &mut Rng) -> Vec<f32> {
    match cue {
        Cue::CamelSit => sit(rng, false),
        Cue::CamelHuskSit => sit(rng, true),
        Cue::CamelStand => stand(rng, false),
        Cue::CamelHuskStand => stand(rng, true),
        Cue::CamelDash => dash(rng, false),
        Cue::CamelHuskDash => dash(rng, true),
        Cue::CamelDashReady => dash_ready(rng, false),
        Cue::CamelHuskDashReady => dash_ready(rng, true),
        Cue::HorseAngry => dsp::finish(whinny(rng, 1.0, false), 0.55),
        Cue::HorseAngryBaby => dsp::finish(whinny(rng, 1.5, false), 0.5),
        Cue::ZombieHorseAngry => dsp::finish(whinny(rng, 0.8, true), 0.55),
        Cue::DonkeyAngry => bray(rng, false),
        Cue::MuleAngry => bray(rng, true),
        Cue::LlamaAngry => llama(rng),
    }
}
