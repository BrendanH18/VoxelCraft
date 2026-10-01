//! The sound set: materials, sound ids and the procedural recipe for each.
//!
//! Every sound is synthesized from noise, damped sinusoids, envelopes and
//! filters at [`RATE`], with a few seeded variants per sound so repeated
//! footsteps and breaks don't sound identical.

use std::f32::consts::TAU;

use super::dsp::{self, Biquad, Mode, OnePole, RATE, Rng, add_mode, crackle, mix_into, noise, samples};
use crate::world::block::Block;

/// Sound category of a block.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Material {
    Stone,
    Wood,
    Dirt,
    Grass,
    Gravel,
    Sand,
    Snow,
    Leaves,
    Glass,
    Water,
}

impl Material {
    pub const ALL: [Material; 10] = [
        Material::Stone,
        Material::Wood,
        Material::Dirt,
        Material::Grass,
        Material::Gravel,
        Material::Sand,
        Material::Snow,
        Material::Leaves,
        Material::Glass,
        Material::Water,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Material::Stone => "stone",
            Material::Wood => "wood",
            Material::Dirt => "dirt",
            Material::Grass => "grass",
            Material::Gravel => "gravel",
            Material::Sand => "sand",
            Material::Snow => "snow",
            Material::Leaves => "leaves",
            Material::Glass => "glass",
            Material::Water => "water",
        }
    }
}

/// The one place blocks are mapped to sound materials.
pub fn material(block: Block) -> Material {
    match block {
        Block::LOG | Block::PLANKS => Material::Wood,
        Block::DIRT => Material::Dirt,
        Block::GRASS | Block::CACTUS | Block::TALL_GRASS | Block::DANDELION | Block::POPPY | Block::DEAD_BUSH => {
            Material::Grass
        }
        Block::TORCH => Material::Wood,
        Block::GRAVEL => Material::Gravel,
        Block::SAND => Material::Sand,
        Block::SNOW | Block::SNOWY_GRASS => Material::Snow,
        Block::LEAVES | Block::SPRUCE_LEAVES => Material::Leaves,
        Block::GLASS | Block::GLOWSTONE => Material::Glass,
        b if b.is_fluid() => Material::Water,
        // Stone, cobblestone, ores, bricks, sandstone, bedrock and unknowns.
        _ => Material::Stone,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sound {
    Break(Material),
    Place(Material),
    Step(Material),
    /// Heavy landing thud after a fall (played with the material step).
    Land,
    /// Entering water.
    Splash,
    /// Swimming stroke.
    Swim,
    /// Inventory UI click.
    Click,
    /// Cave water drip.
    Drip,
    /// Outdoor wind (seamless loop).
    Wind,
    /// Underground room tone (seamless loop).
    Cave,
}

const M: usize = Material::ALL.len();

impl Sound {
    pub const COUNT: usize = 3 * M + 7;

    /// Dense index in `0..COUNT`.
    pub fn key(self) -> usize {
        match self {
            Sound::Break(m) => m as usize,
            Sound::Place(m) => M + m as usize,
            Sound::Step(m) => 2 * M + m as usize,
            Sound::Land => 3 * M,
            Sound::Splash => 3 * M + 1,
            Sound::Swim => 3 * M + 2,
            Sound::Click => 3 * M + 3,
            Sound::Drip => 3 * M + 4,
            Sound::Wind => 3 * M + 5,
            Sound::Cave => 3 * M + 6,
        }
    }

    pub fn all() -> impl Iterator<Item = Sound> {
        let per_material = Material::ALL.into_iter().flat_map(|m| [Sound::Break(m), Sound::Place(m), Sound::Step(m)]);
        per_material.chain([
            Sound::Land,
            Sound::Splash,
            Sound::Swim,
            Sound::Click,
            Sound::Drip,
            Sound::Wind,
            Sound::Cave,
        ])
    }

    pub fn name(self) -> String {
        match self {
            Sound::Break(m) => format!("break_{}", m.name()),
            Sound::Place(m) => format!("place_{}", m.name()),
            Sound::Step(m) => format!("step_{}", m.name()),
            Sound::Land => "land".into(),
            Sound::Splash => "splash".into(),
            Sound::Swim => "swim".into(),
            Sound::Click => "click".into(),
            Sound::Drip => "drip".into(),
            Sound::Wind => "wind".into(),
            Sound::Cave => "cave".into(),
        }
    }

    pub fn is_loop(self) -> bool {
        matches!(self, Sound::Wind | Sound::Cave)
    }

    pub fn variants(self) -> u32 {
        match self {
            Sound::Step(_) => 4,
            Sound::Break(_) | Sound::Place(_) | Sound::Swim | Sound::Drip => 3,
            Sound::Land | Sound::Splash => 2,
            Sound::Click | Sound::Wind | Sound::Cave => 1,
        }
    }

    /// Synthesizes one variant.
    pub fn render(self, variant: u32) -> Vec<f32> {
        let mut rng = Rng::new((self.key() as u64) << 8 | variant as u64);
        match self {
            Sound::Break(m) => break_sound(m, &mut rng),
            Sound::Place(m) => place_sound(m, &mut rng),
            Sound::Step(m) => step_sound(m, &mut rng),
            Sound::Land => land(&mut rng),
            Sound::Splash => splash(&mut rng, 1.0),
            Sound::Swim => swim(&mut rng),
            Sound::Click => click(),
            Sound::Drip => drip(&mut rng),
            Sound::Wind => wind(&mut rng),
            Sound::Cave => cave(&mut rng),
        }
    }
}

/// All sounds, synthesized once and shared with the mixer.
pub struct Bank {
    pub buffers: Vec<Vec<f32>>,
    /// Per `Sound::key`: first buffer index and variant count.
    ranges: [(u16, u16); Sound::COUNT],
}

impl Bank {
    pub fn synthesize() -> Bank {
        let sounds: Vec<Sound> = Sound::all().collect();
        // Sounds are independent: render them across a few threads.
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, 4);
        let rendered: Vec<Vec<Vec<f32>>> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..threads)
                .map(|t| {
                    let sounds = &sounds;
                    s.spawn(move || {
                        sounds
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| i % threads == t)
                            .map(|(_, &snd)| (0..snd.variants()).map(|v| snd.render(v)).collect::<Vec<_>>())
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            let per_thread: Vec<Vec<Vec<Vec<f32>>>> =
                handles.into_iter().map(|h| h.join().unwrap_or_default()).collect();
            let mut iters: Vec<_> = per_thread.into_iter().map(|v| v.into_iter()).collect();
            (0..sounds.len()).map(|i| iters[i % threads].next().unwrap_or_default()).collect()
        });
        let mut bank = Bank { buffers: Vec::new(), ranges: [(0, 0); Sound::COUNT] };
        for (snd, variants) in sounds.iter().zip(rendered) {
            bank.ranges[snd.key()] = (bank.buffers.len() as u16, variants.len() as u16);
            bank.buffers.extend(variants);
        }
        bank
    }

    /// A bank where every sound is the same single buffer (mixer tests).
    #[cfg(test)]
    pub fn uniform(buf: Vec<f32>) -> Bank {
        Bank { buffers: vec![buf], ranges: [(0, 1); Sound::COUNT] }
    }

    pub fn variants(&self, sound: Sound) -> (usize, usize) {
        let (start, count) = self.ranges[sound.key()];
        (start as usize, count as usize)
    }

    pub fn total_samples(&self) -> usize {
        self.buffers.iter().map(Vec::len).sum()
    }
}

// --- Recipes ------------------------------------------------------------------

/// Parameters of the granular "crunch" family (stone, dirt, gravel...).
struct Crunch {
    /// High-pass / low-pass corners of the grain noise.
    lo: f32,
    hi: f32,
    /// Grain rate at the start, events per second.
    density: f32,
    grain_ms: (f32, f32),
    /// Resonant body band (centre, Q) mixed with the raw grains.
    body: (f32, f32),
    /// Low thump frequency and level (0 disables).
    thump: (f32, f32),
    /// Continuous noise bed under the grains.
    hiss: f32,
    /// Slow amplitude flutter (rustling), 0..1.
    flutter: f32,
}

fn crunch_params(m: Material) -> Crunch {
    match m {
        Material::Stone => Crunch {
            lo: 450.0,
            hi: 6000.0,
            density: 900.0,
            grain_ms: (0.8, 3.5),
            body: (1900.0, 1.4),
            thump: (115.0, 0.55),
            hiss: 0.12,
            flutter: 0.0,
        },
        Material::Gravel => Crunch {
            lo: 280.0,
            hi: 5200.0,
            density: 1600.0,
            grain_ms: (1.0, 5.0),
            body: (1300.0, 0.8),
            thump: (85.0, 0.3),
            hiss: 0.25,
            flutter: 0.0,
        },
        Material::Dirt => Crunch {
            lo: 70.0,
            hi: 1500.0,
            density: 750.0,
            grain_ms: (2.0, 8.0),
            body: (380.0, 0.9),
            thump: (90.0, 0.5),
            hiss: 0.3,
            flutter: 0.0,
        },
        Material::Grass => Crunch {
            lo: 220.0,
            hi: 3800.0,
            density: 900.0,
            grain_ms: (2.0, 7.0),
            body: (1000.0, 0.6),
            thump: (95.0, 0.3),
            hiss: 0.45,
            flutter: 0.3,
        },
        Material::Sand => Crunch {
            lo: 500.0,
            hi: 4500.0,
            density: 2600.0,
            grain_ms: (0.5, 2.0),
            body: (2200.0, 0.5),
            thump: (0.0, 0.0),
            hiss: 0.6,
            flutter: 0.15,
        },
        Material::Snow => Crunch {
            lo: 160.0,
            hi: 2200.0,
            density: 1900.0,
            grain_ms: (1.0, 3.0),
            body: (750.0, 1.0),
            thump: (65.0, 0.2),
            hiss: 0.35,
            flutter: 0.0,
        },
        Material::Leaves => Crunch {
            lo: 600.0,
            hi: 5000.0,
            density: 1300.0,
            grain_ms: (3.0, 12.0),
            body: (2400.0, 0.5),
            thump: (0.0, 0.0),
            hiss: 0.5,
            flutter: 0.8,
        },
        // Glass footsteps: a bright, light stone tap.
        Material::Glass => Crunch {
            lo: 900.0,
            hi: 9000.0,
            density: 700.0,
            grain_ms: (0.5, 2.0),
            body: (2800.0, 1.5),
            thump: (140.0, 0.35),
            hiss: 0.05,
            flutter: 0.0,
        },
        // Wood and water use their own recipes; neutral fallback.
        Material::Wood | Material::Water => crunch_params(Material::Dirt),
    }
}

/// Granular crunch lasting `secs`, with grain density decaying over `tau`.
fn crunch(p: &Crunch, rng: &mut Rng, secs: f32, tau: f32, thump_gain: f32) -> Vec<f32> {
    let len = samples(secs);
    // A few clusters (sub-impacts) make breaks sound like crumbling rather
    // than a single burst.
    let clusters: Vec<(f32, f32)> =
        (0..3).map(|i| (i as f32 * rng.range(0.03, 0.07) * (secs / 0.3), rng.range(0.5, 1.0))).collect();
    let density = |t: f32| {
        let mut d = 0.0;
        for &(t0, a) in &clusters {
            if t >= t0 {
                d += a * (-(t - t0) / tau).exp();
            }
        }
        p.density * d.min(1.3)
    };
    let mut grains = crackle(rng, len, p.grain_ms, density);
    Biquad::highpass(p.lo, 0.7).run(&mut grains);
    Biquad::lowpass(p.hi, 0.7).run(&mut grains);
    let mut body = grains.clone();
    Biquad::bandpass(p.body.0, p.body.1).run(&mut body);
    let mut out: Vec<f32> = grains.iter().zip(&body).map(|(g, b)| g * 0.6 + b * 1.4).collect();

    if p.hiss > 0.0 {
        let flut_f = rng.range(9.0, 16.0);
        let flutter = p.flutter;
        let mut bed = noise(rng, len, |t| {
            let f = 1.0 - flutter * (0.5 + 0.5 * (TAU * flut_f * t).sin());
            dsp::ad(t, 0.004, tau * 1.1) * f
        });
        Biquad::highpass(p.lo, 0.7).run(&mut bed);
        Biquad::lowpass(p.hi * 0.8, 0.7).run(&mut bed);
        mix_into(&mut out, &bed, p.hiss * 0.35, 0);
    }
    if p.thump.1 > 0.0 {
        let f = p.thump.0 * rng.range(0.9, 1.1);
        let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs())).max(0.05);
        add_mode(
            &mut out,
            0,
            Mode { freq: f * 1.6, amp: peak * p.thump.1 * thump_gain, tau: 0.03, glide: 1.0 / 1.6, glide_tau: 0.012 },
        );
    }
    out
}

/// Hollow wooden knock: a few inharmonic damped modes excited by a click.
fn knock(buf: &mut [f32], start: usize, rng: &mut Rng, amp: f32, damp: f32) {
    let f0 = rng.range(230.0, 320.0);
    for (ratio, a, tau) in [(1.0, 1.0, 0.07), (2.32, 0.55, 0.045), (3.93, 0.3, 0.028), (5.61, 0.16, 0.016)] {
        let f = f0 * ratio * rng.range(0.97, 1.03);
        add_mode(buf, start, Mode { freq: f, amp: amp * a, tau: tau * damp, glide: 1.0, glide_tau: 1.0 });
    }
    // Stick-on-wood click.
    let mut click = noise(rng, samples(0.012), |t| (-t / 0.0015).exp());
    Biquad::bandpass(2400.0, 0.9).run(&mut click);
    mix_into(buf, &click, amp * 0.9, start);
}

fn wood(rng: &mut Rng, kind: Kind) -> Vec<f32> {
    match kind {
        Kind::Break => {
            let mut out = vec![0.0; samples(0.4)];
            knock(&mut out, 0, rng, 1.0, 1.0);
            let second = samples(rng.range(0.05, 0.09));
            knock(&mut out, second, rng, 0.55, 0.8);
            // Splintering crackle.
            let mut sp = crackle(rng, out.len(), (0.5, 2.5), |t| 900.0 * (-t / 0.07).exp());
            Biquad::bandpass(2200.0, 0.8).run(&mut sp);
            mix_into(&mut out, &sp, 0.7, 0);
            dsp::finish(out, 0.8)
        }
        Kind::Place => {
            let mut out = vec![0.0; samples(0.25)];
            knock(&mut out, 0, rng, 1.0, 1.0);
            add_mode(&mut out, 0, Mode { freq: 130.0, amp: 0.5, tau: 0.035, glide: 95.0 / 130.0, glide_tau: 0.01 });
            dsp::finish(out, 0.7)
        }
        Kind::Step => {
            let mut out = vec![0.0; samples(0.14)];
            knock(&mut out, 0, rng, 1.0, 0.55);
            Biquad::lowpass(2200.0, 0.7).run(&mut out);
            dsp::finish(out, 0.42)
        }
    }
}

/// Short high glass ping: two inharmonic partials.
fn ping(buf: &mut [f32], start: usize, f: f32, amp: f32, tau: f32) {
    add_mode(buf, start, Mode { freq: f, amp, tau, glide: 1.0, glide_tau: 1.0 });
    add_mode(buf, start, Mode { freq: f * 2.76, amp: amp * 0.45, tau: tau * 0.6, glide: 1.0, glide_tau: 1.0 });
}

fn glass(rng: &mut Rng, kind: Kind) -> Vec<f32> {
    match kind {
        Kind::Break => {
            let len = samples(0.75);
            // Initial shatter: bright noise burst plus a dense crackle.
            let mut out = noise(rng, len, |t| (-t / 0.018).exp());
            let mut cr = crackle(rng, len, (0.3, 1.5), |t| 3500.0 * (-t / 0.05).exp());
            mix_into(&mut out, &cr, 0.8, 0);
            Biquad::highpass(1800.0, 0.7).run(&mut out);
            cr.clear();
            // Falling shards tinkling.
            let shards = 34;
            for _ in 0..shards {
                let t = (-rng.f32().max(1e-4).ln() * 0.12).min(0.6);
                let amp = rng.range(0.15, 0.5) * (-t / 0.35).exp();
                ping(&mut out, samples(t), rng.range(2600.0, 6800.0), amp, rng.range(0.015, 0.05));
            }
            Biquad::lowpass(11000.0, 0.7).run(&mut out);
            dsp::finish(out, 0.75)
        }
        Kind::Place => {
            let mut out = vec![0.0; samples(0.35)];
            let f = rng.range(1900.0, 2500.0);
            for (ratio, a, tau) in [(1.0, 1.0, 0.07), (2.41, 0.5, 0.045), (4.13, 0.25, 0.025)] {
                add_mode(&mut out, 0, Mode { freq: f * ratio, amp: a, tau, glide: 1.0, glide_tau: 1.0 });
            }
            let mut tick = noise(rng, samples(0.01), |t| (-t / 0.0012).exp());
            Biquad::highpass(2500.0, 0.7).run(&mut tick);
            mix_into(&mut out, &tick, 0.8, 0);
            add_mode(&mut out, 0, Mode { freq: 190.0, amp: 0.4, tau: 0.02, glide: 130.0 / 190.0, glide_tau: 0.01 });
            dsp::finish(out, 0.5)
        }
        Kind::Step => {
            let out = crunch(&crunch_params(Material::Glass), rng, 0.1, 0.02, 1.0);
            dsp::finish(out, 0.4)
        }
    }
}

/// Adds a rising-pitch bubble (Minnaert resonance) at `t0` seconds.
fn bubble(buf: &mut [f32], t0: f32, f0: f32, amp: f32) {
    // Smaller bubbles ring higher and shorter.
    let tau = (0.9 / f0).clamp(0.004, 0.03) * 12.0;
    add_mode(buf, samples(t0), Mode { freq: f0, amp, tau, glide: 1.5, glide_tau: tau * 0.8 });
}

fn splash(rng: &mut Rng, size: f32) -> Vec<f32> {
    let secs = 0.45 + 0.4 * size;
    let len = samples(secs);
    // Body: low-passed noise whose cutoff sweeps down as the water settles.
    let src = noise(rng, len, |t| dsp::ad(t, 0.006, 0.09 + 0.1 * size));
    let mut lp = [OnePole::new(3000.0), OnePole::new(3000.0)];
    let mut out: Vec<f32> = src
        .iter()
        .enumerate()
        .map(|(i, &x)| {
            let t = i as f32 / RATE;
            let fc = 700.0 + 3500.0 * (-t / 0.12).exp();
            let a = OnePole::coef(fc, RATE);
            lp[0].a = a;
            lp[1].a = a;
            let y = lp[0].process(x);
            lp[1].process(y)
        })
        .collect();
    // Spray.
    let mut spray = noise(rng, len, |t| dsp::ad(t, 0.01, 0.1 + 0.08 * size));
    Biquad::highpass(2500.0, 0.7).run(&mut spray);
    Biquad::lowpass(6500.0, 0.7).run(&mut spray);
    mix_into(&mut out, &spray, 0.06, 0);
    let bubbles = (8.0 + 22.0 * size) as usize;
    for _ in 0..bubbles {
        let t = rng.f32().powf(1.6) * (secs - 0.15);
        bubble(&mut out, t, rng.range(450.0, 1700.0), rng.range(0.04, 0.14) * (-t / 0.4).exp());
    }
    dsp::finish(out, 0.4 + 0.35 * size)
}

fn swim(rng: &mut Rng) -> Vec<f32> {
    let len = samples(0.55);
    // A gentle swish: slow-swelling low-passed noise.
    let mut out = noise(rng, len, |t| (t / 0.12).min(1.0).powi(2) * (-(t - 0.12).max(0.0) / 0.13).exp());
    Biquad::lowpass(rng.range(750.0, 1000.0), 0.8).run(&mut out);
    Biquad::highpass(120.0, 0.7).run(&mut out);
    for _ in 0..5 {
        bubble(&mut out, rng.range(0.05, 0.35), rng.range(500.0, 1200.0), rng.range(0.02, 0.05));
    }
    dsp::finish(out, 0.3)
}

fn land(rng: &mut Rng) -> Vec<f32> {
    let len = samples(0.3);
    let mut out = noise(rng, len, |t| dsp::ad(t, 0.002, 0.035));
    Biquad::lowpass(280.0, 0.8).run(&mut out);
    for s in out.iter_mut() {
        *s *= 3.0;
    }
    add_mode(&mut out, 0, Mode { freq: rng.range(105.0, 120.0), amp: 1.0, tau: 0.06, glide: 0.62, glide_tau: 0.025 });
    dsp::finish(out, 0.65)
}

fn click() -> Vec<f32> {
    let mut out = vec![0.0; samples(0.06)];
    // Soft falling blip with a quiet second partial and a tiny tick.
    add_mode(&mut out, 0, Mode { freq: 1100.0, amp: 1.0, tau: 0.009, glide: 0.75, glide_tau: 0.006 });
    add_mode(&mut out, 0, Mode { freq: 2460.0, amp: 0.12, tau: 0.004, glide: 1.0, glide_tau: 1.0 });
    let mut rng = Rng::new(7);
    let mut tick = noise(&mut rng, samples(0.006), |t| (-t / 0.0008).exp());
    Biquad::bandpass(4000.0, 1.0).run(&mut tick);
    mix_into(&mut out, &tick, 0.15, 0);
    dsp::finish(out, 0.35)
}

fn drip(rng: &mut Rng) -> Vec<f32> {
    let len = samples(1.0);
    let mut dry = vec![0.0; len];
    let f = rng.range(900.0, 1400.0);
    // A drop "plink" is a fast upward chirp.
    add_mode(&mut dry, 0, Mode { freq: f, amp: 1.0, tau: 0.028, glide: 1.9, glide_tau: 0.012 });
    // Cave echoes: a few delayed, darker copies.
    let mut out = dry.clone();
    for (delay, gain, fc) in [(0.083, 0.35, 2500.0), (0.151, 0.25, 1800.0), (0.237, 0.17, 1300.0), (0.331, 0.1, 1000.0)]
    {
        let mut echo = dry.clone();
        Biquad::lowpass(fc, 0.7).run(&mut echo);
        mix_into(&mut out, &echo, gain, samples(delay));
    }
    dsp::finish(out, 0.3)
}

fn wind(rng: &mut Rng) -> Vec<f32> {
    let loop_secs = 12.0;
    let (n, x) = (samples(loop_secs), samples(0.5));
    // Brown-ish noise through a band-pass whose centre and level drift with
    // periods that divide the loop length, so the loop point is inaudible.
    let mut brown = 0.0f32;
    let mut out = Vec::with_capacity(n + x);
    let mut bp = Biquad::bandpass(400.0, 0.9);
    let (p1, p2) = (rng.range(0.0, TAU), rng.range(0.0, TAU));
    for i in 0..n + x {
        let t = i as f32 / RATE;
        let w = TAU * t / loop_secs;
        let gust = 0.55 + 0.25 * (w + p1).sin() + 0.2 * (3.0 * w + p2).sin();
        if i % 64 == 0 {
            let fc = 260.0 + 380.0 * (0.5 + 0.5 * (2.0 * w + p2).sin()) * gust;
            bp.retune(Biquad::bandpass(fc, 0.8));
        }
        brown = brown * 0.985 + rng.bi() * 0.15;
        let white = rng.bi() * 0.35;
        out.push(bp.process(brown + white) * gust);
    }
    dsp::make_loop(out, n, x, 0.12)
}

fn cave(rng: &mut Rng) -> Vec<f32> {
    let loop_secs = 10.0;
    let (n, x) = (samples(loop_secs), samples(0.5));
    let mut brown = 0.0f32;
    let mut lp = Biquad::lowpass(140.0, 0.7);
    let mut room = Biquad::bandpass(95.0, 3.0);
    // Faint low-mid "air" so the room tone survives small speakers.
    let mut air = Biquad::bandpass(450.0, 0.8);
    let p = rng.range(0.0, TAU);
    let out: Vec<f32> = (0..n + x)
        .map(|i| {
            let w = TAU * i as f32 / RATE / loop_secs;
            brown = brown * 0.995 + rng.bi() * 0.1;
            let s = lp.process(brown);
            let a = air.process(rng.bi()) * 2.5;
            (s + room.process(s) * 1.5 + a) * (0.8 + 0.2 * (2.0 * w + p).sin())
        })
        .collect();
    dsp::make_loop(out, n, x, 0.1)
}

#[derive(Clone, Copy)]
enum Kind {
    Break,
    Place,
    Step,
}

fn material_sound(m: Material, rng: &mut Rng, kind: Kind) -> Vec<f32> {
    match m {
        Material::Wood => wood(rng, kind),
        Material::Glass => glass(rng, kind),
        Material::Water => match kind {
            Kind::Break => splash(rng, 0.5),
            Kind::Place => splash(rng, 0.3),
            Kind::Step => swim(rng),
        },
        _ => {
            let p = crunch_params(m);
            match kind {
                Kind::Break => dsp::finish(crunch(&p, rng, 0.38, 0.09, 1.0), 0.8),
                Kind::Place => dsp::finish(crunch(&p, rng, 0.18, 0.035, 1.6), 0.7),
                Kind::Step => dsp::finish(crunch(&p, rng, 0.13, 0.025, 1.0), 0.42),
            }
        }
    }
}

fn break_sound(m: Material, rng: &mut Rng) -> Vec<f32> {
    material_sound(m, rng, Kind::Break)
}

fn place_sound(m: Material, rng: &mut Rng) -> Vec<f32> {
    material_sound(m, rng, Kind::Place)
}

fn step_sound(m: Material, rng: &mut Rng) -> Vec<f32> {
    material_sound(m, rng, Kind::Step)
}
