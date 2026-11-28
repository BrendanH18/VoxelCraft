//! v1.0 final Track F4: original procedural voices for the mobs that used
//! to borrow another mob's (iron golems, llamas, camels, snow golems,
//! phantoms, silverfish and turtles), and the footsteps Java gives a mob of
//! its own (`entity.<mob>.step`, the camel's sand step and the turtle's
//! shamble). Nothing is sampled: the calls are the shared source-filter
//! voice ([`super::voices::utter`]) in each mob's own vowel and contour, and
//! the steps damped knocks over filtered noise.

use super::dsp::{self, Biquad, Mode, Rng, add_mode, crackle, mix_into, noise, samples};
use super::voices::{Call, FLAT, Utterance, lerp, swell, utter};

pub use voxelcraft::entity::mob_steps::StepCue;

/// A footstep: a damped knock (`thud` Hz) under a scuff of band-passed noise.
fn knock(rng: &mut Rng, secs: f32, thud: (f32, f32), scuff: f32, bright: f32) -> Vec<f32> {
    let thud = rng.range(thud.0, thud.1);
    let mut out = noise(rng, samples(secs), |t| dsp::ad(t, 0.004, secs * 0.25) * scuff);
    Biquad::bandpass(bright, 0.9).run(&mut out);
    add_mode(&mut out, 0, Mode { freq: thud, amp: 1.0, tau: secs * 0.2, glide: 0.7, glide_tau: 0.02 });
    out
}

pub fn render_step(cue: StepCue, rng: &mut Rng) -> Vec<f32> {
    match cue {
        StepCue::IronGolem => {
            // A heavy iron foot: a low thump and the ring of plates.
            let mut out = knock(rng, 0.45, (55.0, 65.0), 0.5, 900.0);
            for (f, a, tau) in [(410.0, 0.35, 0.09), (1130.0, 0.22, 0.06), (2270.0, 0.12, 0.04)] {
                add_mode(
                    &mut out,
                    samples(0.004),
                    Mode { freq: f * rng.range(0.97, 1.03), amp: a, tau, glide: 1.0, glide_tau: 1.0 },
                );
            }
            dsp::finish(out, 0.6)
        }
        StepCue::Llama => dsp::finish(knock(rng, 0.18, (150.0, 180.0), 0.8, 1400.0), 0.5),
        StepCue::Camel => dsp::finish(knock(rng, 0.3, (85.0, 100.0), 0.7, 700.0), 0.55),
        StepCue::CamelSand => {
            let mut out = knock(rng, 0.35, (85.0, 100.0), 0.5, 700.0);
            let mut sand = crackle(rng, samples(0.35), (1.0, 3.0), |t| 2500.0 * dsp::ad(t, 0.01, 0.1));
            Biquad::highpass(2200.0, 0.7).run(&mut sand);
            mix_into(&mut out, &sand, 0.5, 0);
            dsp::finish(out, 0.55)
        }
        StepCue::Silverfish => {
            let mut out = crackle(rng, samples(0.12), (0.3, 1.0), |t| 1800.0 * dsp::ad(t, 0.005, 0.04));
            Biquad::highpass(3500.0, 0.8).run(&mut out);
            dsp::finish(out, 0.4)
        }
        StepCue::Turtle | StepCue::BabyTurtle => {
            // The shell dragged over the ground: a short gritty scrape.
            let baby = cue == StepCue::BabyTurtle;
            let secs = if baby { 0.18 } else { 0.32 };
            let mut out = crackle(rng, samples(secs), (0.5, 2.0), |t| 1400.0 * swell(t, 0.03, secs * 0.5, secs));
            Biquad::bandpass(if baby { 2600.0 } else { 1300.0 }, 1.1).run(&mut out);
            add_mode(
                &mut out,
                0,
                Mode { freq: if baby { 260.0 } else { 140.0 }, amp: 0.4, tau: 0.03, glide: 0.8, glide_tau: 0.02 },
            );
            dsp::finish(out, 0.45)
        }
    }
}

/// An iron golem: no voice, only metal. Hurt is a clank and a low groan of
/// stressed iron; death the same groan sagging while plates clatter down.
/// Java gives it no ambient call; the ambient buffer is a quiet creak that
/// never plays.
pub(super) fn iron_golem(call: Call, rng: &mut Rng) -> Vec<f32> {
    let secs = match call {
        Call::Ambient => 0.6,
        Call::Hurt => 0.55,
        Call::Death => 1.6,
    };
    let base = rng.range(70.0, 80.0);
    let fall = if call == Call::Death { 0.6 } else { 0.9 };
    let mut out = utter(
        rng,
        &Utterance {
            secs,
            f0: &|t| base * lerp(1.0, fall, t / secs),
            env: &|t| swell(t, 0.02, secs * 0.5, secs),
            formants: [(310.0, 6.0, 1.0), (760.0, 9.0, 0.6), (1900.0, 12.0, 0.35)],
            shift: FLAT,
            jitter: 0.04,
            breath: 0.05,
        },
    );
    let hits: u32 = match call {
        Call::Ambient => 1,
        Call::Hurt => 2,
        Call::Death => 7,
    };
    for h in 0..hits {
        let at = samples(if h == 0 { 0.0 } else { rng.range(0.05, secs * 0.85) });
        let ring = rng.range(0.85, 1.15);
        for (f, a, tau) in [(520.0, 0.8, 0.12), (1340.0, 0.5, 0.08), (2710.0, 0.3, 0.05), (3890.0, 0.15, 0.03)] {
            add_mode(&mut out, at, Mode { freq: f * ring, amp: a, tau, glide: 0.98, glide_tau: 0.1 });
        }
    }
    dsp::finish(out, if call == Call::Ambient { 0.25 } else { 0.6 })
}

/// A llama: a nasal, humming bleat ("mmeh") with a quick waver; the hurt
/// call clipped and high, the death a long sinking hum.
pub(super) fn llama(call: Call, rng: &mut Rng) -> Vec<f32> {
    let (secs, base, fall) = match call {
        Call::Ambient => (rng.range(0.55, 0.75), rng.range(230.0, 270.0), 0.9),
        Call::Hurt => (0.3, rng.range(320.0, 350.0), 0.85),
        Call::Death => (1.0, rng.range(240.0, 260.0), 0.55),
    };
    let rate = rng.range(7.0, 9.0);
    let out = utter(
        rng,
        &Utterance {
            secs,
            f0: &|t| base * (1.0 + 0.05 * (std::f32::consts::TAU * rate * t).sin()) * lerp(1.0, fall, t / secs),
            env: &|t| swell(t, 0.05, secs * 0.45, secs),
            formants: [(350.0, 6.0, 1.0), (1250.0, 6.0, 0.5), (2500.0, 8.0, 0.2)],
            shift: &|t| lerp(0.8, 1.0, t / (secs * 0.4)),
            jitter: 0.03,
            breath: 0.12,
        },
    );
    dsp::finish(out, 0.5)
}

/// A camel: a deep, gurgling grumble with a throaty flutter, lower and
/// wetter than a camel husk's rasp.
pub(super) fn camel(call: Call, rng: &mut Rng) -> Vec<f32> {
    let (secs, base) = match call {
        Call::Ambient => (rng.range(1.0, 1.3), rng.range(85.0, 100.0)),
        Call::Hurt => (0.45, rng.range(130.0, 145.0)),
        Call::Death => (1.4, rng.range(95.0, 105.0)),
    };
    let gurgle = rng.range(11.0, 15.0);
    let out = utter(
        rng,
        &Utterance {
            secs,
            f0: &|t| base * lerp(1.05, if call == Call::Death { 0.6 } else { 0.85 }, t / secs),
            env: &|t| {
                let flutter = 0.7 + 0.3 * (std::f32::consts::TAU * gurgle * t).sin();
                swell(t, 0.08, secs * 0.4, secs) * flutter
            },
            formants: [(520.0, 4.0, 1.0), (1050.0, 5.0, 0.45), (2300.0, 6.0, 0.15)],
            shift: &|t| lerp(0.7, 1.0, t / (secs * 0.3)),
            jitter: 0.12,
            breath: 0.35,
        },
    );
    dsp::finish(out, 0.55)
}

/// A snow golem: a soft, hollow "hoo" through packed snow, with a crunch.
pub(super) fn snow_golem(call: Call, rng: &mut Rng) -> Vec<f32> {
    let (secs, base) = match call {
        Call::Ambient => (rng.range(0.5, 0.7), rng.range(190.0, 220.0)),
        Call::Hurt => (0.3, rng.range(260.0, 290.0)),
        Call::Death => (0.9, rng.range(200.0, 220.0)),
    };
    let mut out = utter(
        rng,
        &Utterance {
            secs,
            f0: &|t| base * lerp(1.0, if call == Call::Death { 0.55 } else { 0.92 }, t / secs),
            env: &|t| swell(t, 0.06, secs * 0.5, secs),
            formants: [(320.0, 5.0, 1.0), (700.0, 6.0, 0.4), (2200.0, 5.0, 0.1)],
            shift: FLAT,
            jitter: 0.01,
            breath: 0.6,
        },
    );
    let crunch_len = if call == Call::Death { 0.8 } else { 0.25 };
    let mut crunch = crackle(rng, samples(crunch_len), (1.0, 4.0), |t| 900.0 * dsp::ad(t, 0.01, crunch_len * 0.4));
    Biquad::bandpass(2400.0, 0.8).run(&mut crunch);
    mix_into(&mut out, &crunch, if call == Call::Ambient { 0.15 } else { 0.4 }, 0);
    dsp::finish(out, 0.45)
}

/// A phantom: a thin, wavering screech over a dry, papery breath.
pub(super) fn phantom(call: Call, rng: &mut Rng) -> Vec<f32> {
    let (secs, base) = match call {
        Call::Ambient => (rng.range(0.9, 1.2), rng.range(620.0, 700.0)),
        Call::Hurt => (0.35, rng.range(820.0, 880.0)),
        Call::Death => (1.3, rng.range(680.0, 720.0)),
    };
    let rate = rng.range(5.0, 7.0);
    let out = utter(
        rng,
        &Utterance {
            secs,
            f0: &|t| {
                let x = t / secs;
                let arc =
                    if call == Call::Death { lerp(1.1, 0.5, x) } else { 1.0 + 0.15 * (x * std::f32::consts::PI).sin() };
                base * arc * (1.0 + 0.04 * (std::f32::consts::TAU * rate * t).sin())
            },
            env: &|t| swell(t, 0.08, secs * 0.45, secs),
            formants: [(1400.0, 5.0, 1.0), (2600.0, 6.0, 0.7), (4200.0, 7.0, 0.35)],
            shift: FLAT,
            jitter: 0.05,
            breath: 0.9,
        },
    );
    dsp::finish(out, 0.4)
}

/// A silverfish: a dry, scratchy chitter, lower and grittier than an
/// endermite's.
pub(super) fn silverfish(call: Call, rng: &mut Rng) -> Vec<f32> {
    let (secs, rate) = match call {
        Call::Ambient => (rng.range(0.4, 0.55), rng.range(24.0, 30.0)),
        Call::Hurt => (0.25, 40.0),
        Call::Death => (0.75, 30.0),
    };
    let mut out = crackle(rng, samples(secs), (0.4, 1.5), |t| {
        let chop = 0.5 + 0.5 * (std::f32::consts::TAU * rate * t).sin();
        3000.0 * chop * chop * swell(t, 0.02, secs * 0.4, secs)
    });
    Biquad::bandpass(if call == Call::Hurt { 4600.0 } else { 3800.0 }, 1.0).run(&mut out);
    let mut hiss = noise(rng, samples(secs), |t| swell(t, 0.03, secs * 0.5, secs) * 0.25);
    Biquad::highpass(5000.0, 0.7).run(&mut hiss);
    mix_into(&mut out, &hiss, 1.0, 0);
    dsp::finish(out, 0.4)
}

/// A turtle (`baby`: a hatchling): a soft, low creak of a grunt on land,
/// a small squeak when hurt, a long sigh as it dies; a hatchling's are
/// higher and shorter.
pub(super) fn turtle(call: Call, rng: &mut Rng, baby: bool) -> Vec<f32> {
    let lift = if baby { 2.4 } else { 1.0 };
    let (secs, base) = match call {
        Call::Ambient => (rng.range(0.4, 0.6), rng.range(110.0, 130.0)),
        Call::Hurt => (0.25, rng.range(180.0, 200.0)),
        Call::Death => (0.9, rng.range(120.0, 135.0)),
    };
    let secs = if baby { secs * 0.7 } else { secs };
    let out = utter(
        rng,
        &Utterance {
            secs,
            f0: &|t| base * lift * lerp(1.0, if call == Call::Death { 0.6 } else { 0.85 }, t / secs),
            env: &|t| swell(t, 0.03, secs * 0.4, secs),
            formants: [(450.0 * lift.sqrt(), 5.0, 1.0), (1100.0 * lift.sqrt(), 6.0, 0.4), (2600.0, 7.0, 0.15)],
            shift: FLAT,
            jitter: 0.08,
            breath: 0.4,
        },
    );
    dsp::finish(out, 0.45)
}

/// v1.0 final Track F4: the mobs whose Java events are their own but whose
/// kin's voice was all they had. Each takes its kin's call recipe and makes
/// it its own: transposed, filtered and layered with the mob's own
/// texture (a husk's sand, a drowned's water, a stray's frost, a bogged's
/// moss, a magma cube's sizzle, a skeleton horse's hollow bones).
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct Kin {
    pub base: super::voices::Voice,
    /// Playback rate: above 1 is higher and shorter.
    pub rate: f32,
    /// Low-pass corner (Hz), or 0 for none.
    pub low: f32,
    /// High-pass corner (Hz), or 0 for none.
    pub high: f32,
    /// The texture layered over the call.
    pub layer: Layer,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Layer {
    None,
    /// Dry, fine grit.
    Sand,
    /// Wet bubbling.
    Water,
    /// A thin, glassy shimmer.
    Frost,
    /// A soft, damp rustle.
    Moss,
    /// A hot crackle.
    Sizzle,
    /// A low, hollow resonance.
    Hollow,
    /// A breathy hum.
    Hum,
}

pub(super) fn kin(k: Kin, call: Call, rng: &mut Rng) -> Vec<f32> {
    let src = super::voices::render(k.base, call, rng);
    let len = ((src.len() as f32) / k.rate) as usize;
    let mut out: Vec<f32> = (0..len)
        .map(|i| {
            let x = i as f32 * k.rate;
            let j = x as usize;
            let f = x - j as f32;
            let a = src.get(j).copied().unwrap_or(0.0);
            let b = src.get(j + 1).copied().unwrap_or(0.0);
            a + (b - a) * f
        })
        .collect();
    if k.low > 0.0 {
        Biquad::lowpass(k.low, 0.7).run(&mut out);
    }
    if k.high > 0.0 {
        Biquad::highpass(k.high, 0.7).run(&mut out);
    }
    let secs = out.len() as f32 / dsp::RATE;
    let env = |t: f32| swell(t, 0.02, secs * 0.5, secs);
    let layer = match k.layer {
        Layer::None => None,
        Layer::Sand => {
            let mut l = crackle(rng, out.len(), (0.3, 1.0), |t| 2200.0 * env(t));
            Biquad::highpass(3000.0, 0.7).run(&mut l);
            Some((l, 0.25))
        }
        Layer::Water => {
            let mut l = vec![0.0; out.len()];
            let mut t = 0.0;
            while t < secs {
                let f = rng.range(300.0, 900.0);
                add_mode(
                    &mut l,
                    samples(t),
                    Mode { freq: f, amp: rng.range(0.3, 0.8) * env(t), tau: 0.02, glide: 1.6, glide_tau: 0.015 },
                );
                t += rng.range(0.03, 0.09);
            }
            Some((l, 0.35))
        }
        Layer::Frost => {
            let mut l = noise(rng, out.len(), |t| env(t) * 0.5);
            Biquad::bandpass(6500.0, 4.0).run(&mut l);
            Some((l, 0.3))
        }
        Layer::Moss => {
            let mut l = noise(rng, out.len(), |t| env(t) * 0.5);
            Biquad::lowpass(1200.0, 0.7).run(&mut l);
            Some((l, 0.25))
        }
        Layer::Sizzle => {
            let mut l = crackle(rng, out.len(), (0.2, 0.8), |t| 3500.0 * env(t));
            Biquad::highpass(4000.0, 0.7).run(&mut l);
            Some((l, 0.3))
        }
        Layer::Hollow => {
            let mut l = vec![0.0; out.len()];
            add_mode(
                &mut l,
                0,
                Mode { freq: rng.range(180.0, 220.0), amp: 0.6, tau: secs * 0.3, glide: 0.9, glide_tau: 0.2 },
            );
            Some((l, 0.3))
        }
        Layer::Hum => {
            let f = rng.range(140.0, 170.0);
            let l: Vec<f32> = (0..out.len())
                .map(|i| {
                    let t = i as f32 / dsp::RATE;
                    (std::f32::consts::TAU * f * t).sin() * env(t)
                })
                .collect();
            Some((l, 0.15))
        }
    };
    if let Some((l, gain)) = layer {
        let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs())).max(1e-6);
        mix_into(&mut out, &l, gain * peak, 0);
    }
    dsp::finish(out, 0.5)
}
