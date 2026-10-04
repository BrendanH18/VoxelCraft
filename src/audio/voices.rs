//! Creature voices and combat sounds: mob calls, hurt cries and death
//! sounds, the player's "oof" and the thud of a melee hit.
//!
//! Voiced sounds use a tiny source-filter model: a band-limited sawtooth
//! "glottis" with a pitch contour, jitter and breath noise, run through
//! three parallel formant band-passes (the vowel). Skeletons and spiders
//! are made of clicks and hisses instead.

use super::dsp::{self, Biquad, Mode, Rng, add_mode, crackle, mix_into, noise, samples};

/// Who is making the sound.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Voice {
    Pig,
    Cow,
    Sheep,
    Chicken,
    Zombie,
    Skeleton,
    Creeper,
    Spider,
    Enderman,
}

/// What kind of sound a voice makes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Call {
    /// Idle noises (oinks, moos, groans).
    Ambient,
    Hurt,
    Death,
}

impl Voice {
    pub const ALL: [Voice; 9] = [
        Voice::Pig,
        Voice::Cow,
        Voice::Sheep,
        Voice::Chicken,
        Voice::Zombie,
        Voice::Skeleton,
        Voice::Creeper,
        Voice::Spider,
        Voice::Enderman,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Voice::Pig => "pig",
            Voice::Cow => "cow",
            Voice::Sheep => "sheep",
            Voice::Chicken => "chicken",
            Voice::Zombie => "zombie",
            Voice::Skeleton => "skeleton",
            Voice::Creeper => "creeper",
            Voice::Spider => "spider",
            Voice::Enderman => "enderman",
        }
    }
}

impl Call {
    pub const ALL: [Call; 3] = [Call::Ambient, Call::Hurt, Call::Death];

    pub fn name(self) -> &'static str {
        match self {
            Call::Ambient => "ambient",
            Call::Hurt => "hurt",
            Call::Death => "death",
        }
    }
}

/// A voiced utterance: `f0(t)` is the pitch, `env(t)` the loudness,
/// `formants` the vowel as (frequency, Q, gain), scaled over time by
/// `shift(t)` (1 = unchanged). `breath` mixes in aspiration noise.
struct Utterance<'a> {
    secs: f32,
    f0: &'a dyn Fn(f32) -> f32,
    env: &'a dyn Fn(f32) -> f32,
    formants: [(f32, f32, f32); 3],
    shift: &'a dyn Fn(f32) -> f32,
    jitter: f32,
    breath: f32,
}

fn utter(rng: &mut Rng, u: &Utterance) -> Vec<f32> {
    let len = samples(u.secs);
    let mut filters: Vec<Biquad> = u.formants.iter().map(|&(f, q, _)| Biquad::bandpass(f, q)).collect();
    let mut phase = 0.0f32;
    let mut wobble = 0.0f32;
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let t = i as f32 / dsp::RATE;
        if i % 64 == 0 {
            let s = (u.shift)(t);
            for (filter, &(f, q, _)) in filters.iter_mut().zip(&u.formants) {
                filter.retune(Biquad::bandpass(f * s, q));
            }
            // Slowly wandering pitch error: rough, animal-like voices.
            wobble = wobble * 0.7 + rng.bi() * u.jitter;
        }
        let f = ((u.f0)(t) * (1.0 + wobble)).max(20.0);
        phase = (phase + f / dsp::RATE).fract();
        // Sawtooth softened with a polynomial ramp near the reset (cheap
        // anti-aliasing, enough for these low pitches).
        let dt = f / dsp::RATE;
        let mut saw = 2.0 * phase - 1.0;
        if phase < dt {
            let x = phase / dt;
            saw -= x + x - x * x - 1.0;
        } else if phase > 1.0 - dt {
            let x = (phase - 1.0) / dt;
            saw -= x * x + x + x + 1.0;
        }
        let src = saw + rng.bi() * u.breath;
        let y: f32 = filters.iter_mut().zip(&u.formants).map(|(filter, &(_, _, g))| filter.process(src) * g).sum();
        out.push(y * (u.env)(t));
    }
    out
}

/// A smooth rise over `a` seconds and fall over the last `r` of `len`.
fn swell(t: f32, a: f32, r: f32, len: f32) -> f32 {
    let up = (t / a).min(1.0);
    let down = ((len - t) / r).clamp(0.0, 1.0);
    up * up * (3.0 - 2.0 * up) * down * down * (3.0 - 2.0 * down)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

const FLAT: &dyn Fn(f32) -> f32 = &|_| 1.0;

pub fn render(voice: Voice, call: Call, rng: &mut Rng) -> Vec<f32> {
    match voice {
        Voice::Pig => pig(call, rng),
        Voice::Cow => cow(call, rng),
        Voice::Sheep => sheep(call, rng),
        Voice::Chicken => chicken(call, rng),
        Voice::Zombie => zombie(call, rng),
        Voice::Skeleton => skeleton(call, rng),
        Voice::Creeper => creeper(call, rng),
        Voice::Spider => spider(call, rng),
        Voice::Enderman => enderman(call, rng),
    }
}

fn pig(call: Call, rng: &mut Rng) -> Vec<f32> {
    let nasal = [(520.0, 5.0, 1.0), (1500.0, 6.0, 0.6), (2700.0, 8.0, 0.25)];
    match call {
        Call::Ambient => {
            // Two or three snorty grunts.
            let mut out = vec![0.0; samples(0.6)];
            let grunts = 2 + rng.next_u32() % 2;
            for g in 0..grunts {
                let base = rng.range(130.0, 170.0);
                let len = rng.range(0.1, 0.15);
                let grunt = utter(
                    rng,
                    &Utterance {
                        secs: len,
                        f0: &|t| base * (1.0 - t * 1.5),
                        env: &|t| swell(t, 0.015, 0.05, len),
                        formants: nasal,
                        shift: FLAT,
                        jitter: 0.12,
                        breath: 0.5,
                    },
                );
                mix_into(&mut out, &grunt, 1.0, samples(g as f32 * 0.17));
            }
            dsp::finish(out, 0.5)
        }
        Call::Hurt | Call::Death => {
            // A squeal: high, rising then falling.
            let (secs, top) = if call == Call::Hurt { (0.32, 650.0) } else { (0.7, 560.0) };
            let start = rng.range(380.0, 440.0);
            let out = utter(
                rng,
                &Utterance {
                    secs,
                    f0: &|t| {
                        let x = t / secs;
                        if x < 0.3 { lerp(start, top, x / 0.3) } else { lerp(top, start * 0.6, (x - 0.3) / 0.7) }
                    },
                    env: &|t| swell(t, 0.02, secs * 0.4, secs),
                    formants: [(950.0, 4.0, 1.0), (2100.0, 5.0, 0.7), (3200.0, 6.0, 0.3)],
                    shift: FLAT,
                    jitter: 0.04,
                    breath: 0.25,
                },
            );
            dsp::finish(out, 0.55)
        }
    }
}

fn cow(call: Call, rng: &mut Rng) -> Vec<f32> {
    let (secs, base, fall) = match call {
        Call::Ambient => (rng.range(1.1, 1.4), rng.range(105.0, 125.0), 0.85),
        Call::Hurt => (0.5, rng.range(150.0, 170.0), 0.8),
        Call::Death => (1.1, rng.range(130.0, 145.0), 0.55),
    };
    // "Mmm-ooo": the formants open up after the closed-mouth start.
    let out = utter(
        rng,
        &Utterance {
            secs,
            f0: &|t| {
                let x = t / secs;
                base * (1.0 + 0.15 * (x * 3.0).min(1.0)) * lerp(1.0, fall, (x - 0.4) / 0.6)
            },
            env: &|t| swell(t, secs * 0.2, secs * 0.35, secs),
            formants: [(420.0, 4.0, 1.0), (850.0, 5.0, 0.55), (2400.0, 7.0, 0.15)],
            shift: &|t| lerp(0.55, 1.0, t / (secs * 0.35)),
            jitter: 0.03,
            breath: 0.15,
        },
    );
    dsp::finish(out, 0.6)
}

fn sheep(call: Call, rng: &mut Rng) -> Vec<f32> {
    let (secs, base) = match call {
        Call::Ambient => (rng.range(0.6, 0.8), rng.range(280.0, 320.0)),
        Call::Hurt => (0.35, rng.range(360.0, 400.0)),
        Call::Death => (0.85, rng.range(300.0, 330.0)),
    };
    let rate = rng.range(16.0, 20.0);
    // "Baa": a bleat with a strong, fast vibrato.
    let out = utter(
        rng,
        &Utterance {
            secs,
            f0: &|t| {
                let droop = if call == Call::Death { lerp(1.0, 0.6, t / secs) } else { 1.0 };
                base * (1.0 + 0.11 * (std::f32::consts::TAU * rate * t).sin()) * droop
            },
            env: &|t| swell(t, 0.04, secs * 0.4, secs),
            formants: [(780.0, 4.0, 1.0), (1650.0, 5.0, 0.6), (2800.0, 7.0, 0.25)],
            shift: &|t| lerp(0.7, 1.0, t / 0.08),
            jitter: 0.02,
            breath: 0.2,
        },
    );
    dsp::finish(out, 0.5)
}

fn chicken(call: Call, rng: &mut Rng) -> Vec<f32> {
    let formants = [(1100.0, 4.0, 1.0), (2300.0, 5.0, 0.6), (3600.0, 6.0, 0.3)];
    match call {
        Call::Ambient => {
            // A few quick clucks, the last one drawn out.
            let clucks = 3 + rng.next_u32() % 3;
            let mut out = vec![0.0; samples(0.12 * clucks as f32 + 0.3)];
            for c in 0..clucks {
                let last = c + 1 == clucks;
                let len = if last { 0.16 } else { 0.06 };
                let base = rng.range(550.0, 700.0) * if last { 1.25 } else { 1.0 };
                let cluck = utter(
                    rng,
                    &Utterance {
                        secs: len,
                        f0: &|t| base * (1.0 + t * 2.0),
                        env: &|t| swell(t, 0.008, len * 0.6, len),
                        formants,
                        shift: FLAT,
                        jitter: 0.05,
                        breath: 0.35,
                    },
                );
                mix_into(&mut out, &cluck, 1.0, samples(c as f32 * 0.11 + rng.range(0.0, 0.02)));
            }
            dsp::finish(out, 0.4)
        }
        Call::Hurt | Call::Death => {
            let secs = if call == Call::Hurt { 0.25 } else { 0.5 };
            let base = rng.range(850.0, 950.0);
            let out = utter(
                rng,
                &Utterance {
                    secs,
                    f0: &|t| base * (1.0 + 0.5 * (t / secs * 4.0).min(1.0) - 0.6 * (t / secs)),
                    env: &|t| swell(t, 0.01, secs * 0.5, secs),
                    formants,
                    shift: FLAT,
                    jitter: 0.1,
                    breath: 0.6,
                },
            );
            dsp::finish(out, 0.45)
        }
    }
}

fn zombie(call: Call, rng: &mut Rng) -> Vec<f32> {
    let (secs, base, fall, jitter) = match call {
        Call::Ambient => (rng.range(1.1, 1.5), rng.range(80.0, 95.0), 0.85, 0.18),
        Call::Hurt => (0.4, rng.range(105.0, 120.0), 0.8, 0.25),
        Call::Death => (1.3, rng.range(95.0, 105.0), 0.5, 0.2),
    };
    let wob = rng.range(2.0, 3.5);
    // A low, rough, breathy groan: "uuurgh".
    let out = utter(
        rng,
        &Utterance {
            secs,
            f0: &|t| base * lerp(1.0, fall, t / secs),
            env: &|t| swell(t, secs * 0.15, secs * 0.4, secs) * (0.8 + 0.2 * (wob * std::f32::consts::TAU * t).sin()),
            formants: [(480.0, 3.0, 1.0), (950.0, 4.0, 0.6), (2300.0, 5.0, 0.2)],
            shift: &|t| 0.85 + 0.15 * (wob * 0.7 * std::f32::consts::TAU * t).sin(),
            jitter,
            breath: 0.9,
        },
    );
    dsp::finish(out, 0.6)
}

/// Dry bone clacks: short bright resonances fired at `rate(t)` per second.
fn clatter(rng: &mut Rng, secs: f32, rate: impl Fn(f32) -> f32) -> Vec<f32> {
    let mut out = vec![0.0; samples(secs + 0.05)];
    let mut t = 0.0;
    while t < secs {
        let f = rng.range(1400.0, 3200.0);
        let amp = rng.range(0.4, 1.0);
        add_mode(&mut out, samples(t), Mode { freq: f, amp, tau: 0.006, glide: 1.0, glide_tau: 1.0 });
        add_mode(&mut out, samples(t), Mode { freq: f * 1.7, amp: amp * 0.4, tau: 0.003, glide: 1.0, glide_tau: 1.0 });
        t += rng.range(0.6, 1.4) / rate(t).max(1.0);
    }
    out
}

fn skeleton(call: Call, rng: &mut Rng) -> Vec<f32> {
    let out = match call {
        Call::Ambient => clatter(rng, 0.45, |t| 30.0 * (1.0 - t / 0.5)),
        Call::Hurt => clatter(rng, 0.25, |_| 45.0),
        // A collapse: fast rattling that slows as the bones settle.
        Call::Death => clatter(rng, 0.9, |t| 50.0 * (-t / 0.35).exp() + 6.0),
    };
    dsp::finish(out, 0.5)
}

fn creeper(call: Call, rng: &mut Rng) -> Vec<f32> {
    // Creepers have no voice: a papery leaf rustle.
    let secs = match call {
        Call::Ambient => 0.3,
        Call::Hurt => 0.25,
        Call::Death => 0.5,
    };
    let mut out = crackle(rng, samples(secs), (1.0, 4.0), |t| 900.0 * dsp::ad(t, 0.02, secs * 0.4));
    Biquad::bandpass(2600.0, 0.9).run(&mut out);
    if call == Call::Hurt {
        // The thump of the hit itself.
        add_mode(&mut out, 0, Mode { freq: 140.0, amp: 0.8, tau: 0.03, glide: 0.7, glide_tau: 0.02 });
    }
    dsp::finish(out, 0.4)
}

fn spider(call: Call, rng: &mut Rng) -> Vec<f32> {
    let (secs, rate, bright) = match call {
        Call::Ambient => (0.6, 22.0, 3200.0),
        Call::Hurt => (0.3, 35.0, 4200.0),
        Call::Death => (0.8, 28.0, 3600.0),
    };
    // A hiss chopped into a chitter.
    let mut out = noise(rng, samples(secs), |t| {
        let chop = 0.5 + 0.5 * (std::f32::consts::TAU * rate * t).sin();
        chop * chop * swell(t, 0.03, secs * 0.4, secs)
    });
    Biquad::bandpass(bright, 1.2).run(&mut out);
    let mut clicks = clatter(rng, secs * 0.8, |_| rate * 0.7);
    Biquad::highpass(1800.0, 0.7).run(&mut clicks);
    mix_into(&mut out, &clicks, 0.3, 0);
    if call == Call::Death {
        Biquad::lowpass(5000.0, 0.7).run(&mut out);
    }
    dsp::finish(out, 0.45)
}

fn enderman(call: Call, rng: &mut Rng) -> Vec<f32> {
    let (secs, base, fall, wob) = match call {
        Call::Ambient => (rng.range(0.7, 1.0), rng.range(70.0, 90.0), 0.7, rng.range(6.0, 9.0)),
        Call::Hurt => (0.45, rng.range(180.0, 220.0), 0.6, 14.0),
        Call::Death => (1.6, rng.range(220.0, 250.0), 0.25, 9.0),
    };
    // A garbled, warbling murmur: fast vibrato and a vowel that sweeps
    // back and forth, swelling in like reversed speech.
    let mut out = utter(
        rng,
        &Utterance {
            secs,
            f0: &|t| base * lerp(1.0, fall, t / secs) * (1.0 + 0.12 * (wob * std::f32::consts::TAU * t).sin()),
            env: &|t| swell(t, secs * 0.6, secs * 0.15, secs),
            formants: [(420.0, 3.0, 1.0), (1100.0, 4.0, 0.7), (2600.0, 5.0, 0.3)],
            shift: &|t| 1.0 + 0.35 * (wob * 0.45 * std::f32::consts::TAU * t).sin(),
            jitter: 0.25,
            breath: 0.6,
        },
    );
    for s in &mut out {
        *s = (*s * 2.5).tanh();
    }
    dsp::finish(out, 0.55)
}

/// An enderman stared at: a loud, rasping shriek.
pub fn scream(rng: &mut Rng) -> Vec<f32> {
    let secs = 1.3;
    let base = rng.range(330.0, 380.0);
    let mut out = utter(
        rng,
        &Utterance {
            secs,
            f0: &|t| base * (1.0 + 0.25 * (t / secs)) * (1.0 + 0.06 * (37.0 * t).sin()),
            env: &|t| swell(t, 0.08, 0.5, secs),
            formants: [(1300.0, 3.0, 1.0), (2700.0, 4.0, 0.8), (4100.0, 5.0, 0.4)],
            shift: &|t| 1.0 + 0.2 * (t * 9.0).sin(),
            jitter: 0.5,
            breath: 1.4,
        },
    );
    for s in &mut out {
        *s = (*s * 4.0).tanh();
    }
    dsp::finish(out, 0.6)
}

/// The player taking damage: a short, punchy "oof".
pub fn player_hurt(rng: &mut Rng) -> Vec<f32> {
    let base = rng.range(200.0, 230.0);
    let mut out = utter(
        rng,
        &Utterance {
            secs: 0.2,
            f0: &|t| base * lerp(1.0, 0.72, t / 0.2),
            env: &|t| dsp::ad(t, 0.006, 0.06),
            formants: [(620.0, 4.0, 1.0), (1050.0, 5.0, 0.6), (2500.0, 6.0, 0.2)],
            shift: &|t| lerp(1.0, 0.75, t / 0.2),
            jitter: 0.05,
            breath: 0.35,
        },
    );
    add_mode(&mut out, 0, Mode { freq: 95.0, amp: 0.5, tau: 0.03, glide: 0.7, glide_tau: 0.02 });
    dsp::finish(out, 0.6)
}

/// A melee blow landing: a dull thud with a bit of slap on top.
pub fn hit(rng: &mut Rng) -> Vec<f32> {
    let len = samples(0.18);
    let mut out = noise(rng, len, |t| dsp::ad(t, 0.001, 0.02));
    Biquad::lowpass(rng.range(500.0, 650.0), 0.8).run(&mut out);
    for s in out.iter_mut() {
        *s *= 2.0;
    }
    add_mode(&mut out, 0, Mode { freq: rng.range(140.0, 170.0), amp: 1.0, tau: 0.035, glide: 0.65, glide_tau: 0.02 });
    let mut slap = noise(rng, samples(0.02), |t| (-t / 0.003).exp());
    Biquad::bandpass(2400.0, 1.0).run(&mut slap);
    mix_into(&mut out, &slap, 0.4, 0);
    dsp::finish(out, 0.6)
}
