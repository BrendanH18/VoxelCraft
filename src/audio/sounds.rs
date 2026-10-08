//! The sound set: materials, sound ids and the procedural recipe for each.
//!
//! Every sound is synthesized from noise, damped sinusoids, envelopes and
//! filters at [`RATE`], with a few seeded variants per sound so repeated
//! footsteps and breaks don't sound identical.

use std::f32::consts::TAU;

use super::dsp::{self, Biquad, Mode, OnePole, RATE, Rng, add_mode, crackle, mix_into, noise, samples};
pub use super::voices::{Call, Voice};
use crate::world::block::Block;
use crate::world::gadgets::Instrument;

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
    match block.base() {
        b if b.is_log()
            || b.is_planks()
            || crate::world::gadgets::is_note(b)
            || block.is_door()
            || block.stairs_base().is_some_and(Block::is_planks)
            || block.slab_base().is_some_and(Block::is_planks)
            || crate::world::forms::planks_of(block).is_some() =>
        {
            Material::Wood
        }
        Block::CRAFTING_TABLE | Block::CHEST | Block::PUMPKIN | Block::MELON | Block::SMITHING_TABLE => Material::Wood,
        Block::DIRT | Block::FARMLAND | Block::WET_FARMLAND => Material::Dirt,
        Block::TORCH => Material::Wood,
        b if b == Block::GRASS || b == Block::CACTUS || b.kind() == crate::world::block::RenderKind::Cross => {
            Material::Grass
        }
        Block::GRAVEL | Block::CLAY => Material::Gravel,
        b if b == Block::SAND || b == Block::RED_SAND || b.concrete_powder_color().is_some() => Material::Sand,
        b if b == Block::SNOW || b == Block::SNOWY_GRASS || b.wool_color().is_some() || b.carpet_color().is_some() => {
            Material::Snow
        }
        b if b.is_leaves() => Material::Leaves,
        b if b == Block::GLASS
            || b == Block::GLOWSTONE
            || b == Block::ICE
            || b.stained_glass_color().is_some()
            || b.is_glass_pane() =>
        {
            Material::Glass
        }
        b if b.is_fluid() => Material::Water,
        // Stone, cobblestone, ores, bricks, sandstone, bedrock and unknowns.
        _ => Material::Stone,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sound {
    Note(Instrument),
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
    /// Creeper blast.
    Explosion,
    /// Creeper fuse hiss.
    Fuse,
    /// Skeleton bow release.
    Bow,
    /// Picking up an item.
    Pop,
    /// The player taking damage.
    Hurt,
    /// A melee blow landing on a mob.
    Hit,
    /// A mob's idle call, hurt cry or death sound.
    Mob(Voice, Call),
    /// Rainfall (seamless loop).
    Rain,
    /// A door or gate opening (creaky hinge) or closing (latch and thud).
    Door(bool),
    /// Absorbing an experience orb: a small glassy ding.
    Orb,
    /// Reaching a multiple of five levels: a rising bell arpeggio.
    LevelUp,
    /// An enderman (or pearl thrower) teleporting: a swooping "vwoop".
    Teleport,
    /// An enderman someone looked at.
    Scream,
    /// A blaze's fireball launching: a roaring whoosh.
    Fireball,
    /// A thrown eye of ender dropping or shattering: a glassy shimmer.
    EyeDeath,
    /// An eye clicking into an End portal frame.
    FrameFill,
    /// An End portal opening: a deep rumble under a swelling chord.
    PortalSpawn,
    /// A beat of the Ender Dragon's wings: a heavy whump of air.
    DragonFlap,
    /// The Ender Dragon's growl.
    DragonGrowl,
    /// The Ender Dragon's long dying roar.
    DragonDeath,
}

const M: usize = Material::ALL.len();
const CALLS: usize = Call::ALL.len();

impl Sound {
    pub const COUNT: usize = 3 * M + 27 + Voice::ALL.len() * CALLS + 16;

    /// Dense index in `0..COUNT`.
    pub fn key(self) -> usize {
        match self {
            Sound::Note(i) => 3 * M + 27 + Voice::ALL.len() * CALLS + i as usize,
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
            Sound::Explosion => 3 * M + 7,
            Sound::Fuse => 3 * M + 8,
            Sound::Bow => 3 * M + 9,
            Sound::Pop => 3 * M + 10,
            Sound::Hurt => 3 * M + 11,
            Sound::Hit => 3 * M + 12,
            Sound::Rain => 3 * M + 13,
            Sound::Door(open) => 3 * M + 14 + open as usize,
            Sound::Orb => 3 * M + 16,
            Sound::LevelUp => 3 * M + 17,
            Sound::Teleport => 3 * M + 18,
            Sound::Scream => 3 * M + 19,
            Sound::Fireball => 3 * M + 20,
            Sound::EyeDeath => 3 * M + 21,
            Sound::FrameFill => 3 * M + 22,
            Sound::PortalSpawn => 3 * M + 23,
            Sound::DragonFlap => 3 * M + 24,
            Sound::DragonGrowl => 3 * M + 25,
            Sound::DragonDeath => 3 * M + 26,
            Sound::Mob(v, c) => 3 * M + 27 + v as usize * CALLS + c as usize,
        }
    }

    /// Every material, gameplay and mob sound synthesized into the audio bank.
    pub fn all() -> impl Iterator<Item = Sound> {
        let per_material = Material::ALL.into_iter().flat_map(|m| [Sound::Break(m), Sound::Place(m), Sound::Step(m)]);
        per_material
            .chain([
                Sound::Land,
                Sound::Splash,
                Sound::Swim,
                Sound::Click,
                Sound::Drip,
                Sound::Wind,
                Sound::Cave,
                Sound::Explosion,
                Sound::Fuse,
                Sound::Bow,
                Sound::Pop,
                Sound::Hurt,
                Sound::Hit,
                Sound::Rain,
                Sound::Door(false),
                Sound::Door(true),
                Sound::Orb,
                Sound::LevelUp,
                Sound::Teleport,
                Sound::Scream,
                Sound::Fireball,
                Sound::EyeDeath,
                Sound::FrameFill,
                Sound::PortalSpawn,
                Sound::DragonFlap,
                Sound::DragonGrowl,
                Sound::DragonDeath,
            ])
            .chain(Voice::ALL.into_iter().flat_map(|v| Call::ALL.map(|c| Sound::Mob(v, c))))
            .chain(Instrument::ALL.map(Sound::Note))
    }

    /// Stable sound name used when exporting or identifying samples.
    pub fn name(self) -> String {
        match self {
            Sound::Note(i) => format!("note_{i:?}").to_ascii_lowercase(),
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
            Sound::Explosion => "explosion".into(),
            Sound::Fuse => "fuse".into(),
            Sound::Bow => "bow".into(),
            Sound::Pop => "pop".into(),
            Sound::Hurt => "hurt".into(),
            Sound::Hit => "hit".into(),
            Sound::Rain => "rain".into(),
            Sound::Door(open) => if open { "door_open" } else { "door_close" }.into(),
            Sound::Orb => "xp_orb".into(),
            Sound::LevelUp => "level_up".into(),
            Sound::Teleport => "teleport".into(),
            Sound::Scream => "enderman_scream".into(),
            Sound::Fireball => "fireball".into(),
            Sound::EyeDeath => "ender_eye_death".into(),
            Sound::FrameFill => "end_portal_frame_fill".into(),
            Sound::PortalSpawn => "end_portal_spawn".into(),
            Sound::DragonFlap => "ender_dragon_flap".into(),
            Sound::DragonGrowl => "ender_dragon_growl".into(),
            Sound::DragonDeath => "ender_dragon_death".into(),
            Sound::Mob(v, c) => format!("{}_{}", v.name(), c.name()),
        }
    }

    pub fn is_loop(self) -> bool {
        matches!(self, Sound::Wind | Sound::Cave | Sound::Rain)
    }

    /// Number of synthesized variations playback can choose from.
    pub fn variants(self) -> u32 {
        match self {
            Sound::Note(_) => 1,
            Sound::Step(_) => 4,
            Sound::Break(_) | Sound::Place(_) | Sound::Swim | Sound::Drip => 3,
            Sound::Land | Sound::Splash | Sound::Explosion | Sound::Bow | Sound::Hurt | Sound::Hit | Sound::Door(_) => {
                2
            }
            Sound::Mob(_, Call::Death) => 1,
            Sound::Mob(..) => 2,
            Sound::Click
            | Sound::Wind
            | Sound::Cave
            | Sound::Fuse
            | Sound::Pop
            | Sound::Rain
            | Sound::Orb
            | Sound::LevelUp
            | Sound::Scream
            | Sound::EyeDeath
            | Sound::PortalSpawn
            | Sound::DragonDeath => 1,
            Sound::Teleport | Sound::Fireball | Sound::FrameFill | Sound::DragonFlap | Sound::DragonGrowl => 2,
        }
    }

    /// Synthesizes one variant.
    pub fn render(self, variant: u32) -> Vec<f32> {
        let mut rng = Rng::new((self.key() as u64) << 8 | variant as u64);
        match self {
            Sound::Note(i) => note_sound(i, &mut rng),
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
            Sound::Explosion => explosion(&mut rng),
            Sound::Fuse => fuse(&mut rng),
            Sound::Bow => bow(&mut rng),
            Sound::Pop => pop(),
            Sound::Hurt => super::voices::player_hurt(&mut rng),
            Sound::Hit => super::voices::hit(&mut rng),
            Sound::Rain => rain(&mut rng),
            Sound::Door(open) => door(&mut rng, open),
            Sound::Orb => orb(),
            Sound::LevelUp => level_up(),
            Sound::Teleport => teleport(&mut rng),
            Sound::Scream => super::voices::scream(&mut rng),
            Sound::Fireball => fireball(&mut rng),
            Sound::EyeDeath => eye_death(&mut rng),
            Sound::FrameFill => frame_fill(&mut rng),
            Sound::PortalSpawn => portal_spawn(&mut rng),
            Sound::DragonFlap => dragon_flap(&mut rng),
            Sound::DragonGrowl => dragon_roar(&mut rng, 1.8, (95.0, 70.0)),
            Sound::DragonDeath => dragon_death(&mut rng),
            Sound::Mob(v, c) => super::voices::render(v, c, &mut rng),
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

fn explosion(rng: &mut Rng) -> Vec<f32> {
    let len = samples(2.2);
    // A sharp crack into a long low rumble: noise through a low-pass whose
    // cutoff falls as the blast decays, plus a sub-bass thump.
    let src = noise(rng, len, |t| dsp::ad(t, 0.004, 0.55) + 0.25 * dsp::ad(t, 0.0, 0.02));
    let mut lp = OnePole::new(2500.0);
    let mut out: Vec<f32> = src
        .iter()
        .enumerate()
        .map(|(i, &x)| {
            let t = i as f32 / RATE;
            lp.a = OnePole::coef(180.0 + 2600.0 * (-t / 0.08).exp(), RATE);
            lp.process(x) * 2.6
        })
        .collect();
    add_mode(&mut out, 0, Mode { freq: rng.range(48.0, 56.0), amp: 1.2, tau: 0.35, glide: 0.7, glide_tau: 0.3 });
    let mut debris = crackle(rng, len, (2.0, 8.0), |t| 260.0 * dsp::ad(t, 0.05, 0.4));
    Biquad::bandpass(1800.0, 0.8).run(&mut debris);
    mix_into(&mut out, &debris, 0.12, 0);
    dsp::finish(out, 0.95)
}

fn fuse(rng: &mut Rng) -> Vec<f32> {
    // A rising hiss for the length of the fuse.
    let len = samples(1.5);
    let mut out =
        noise(rng, len, |t| (t / 0.1).min(1.0) * (0.6 + 0.4 * t / 1.5) * (1.0 - ((t - 1.42) / 0.08).max(0.0)));
    Biquad::highpass(3000.0, 0.7).run(&mut out);
    Biquad::lowpass(9000.0, 0.7).run(&mut out);
    dsp::finish(out, 0.35)
}

fn bow(rng: &mut Rng) -> Vec<f32> {
    // String twang: a short plucked tone with a falling pitch and a whoosh.
    let len = samples(0.35);
    let mut out = vec![0.0; len];
    let f = rng.range(380.0, 440.0);
    add_mode(&mut out, 0, Mode { freq: f, amp: 1.0, tau: 0.06, glide: 0.85, glide_tau: 0.05 });
    add_mode(&mut out, 0, Mode { freq: f * 2.02, amp: 0.35, tau: 0.03, glide: 0.85, glide_tau: 0.05 });
    let mut whoosh = noise(rng, len, |t| dsp::ad(t, 0.02, 0.08));
    Biquad::bandpass(2200.0, 1.2).run(&mut whoosh);
    mix_into(&mut out, &whoosh, 0.3, 0);
    dsp::finish(out, 0.4)
}

fn door(rng: &mut Rng, open: bool) -> Vec<f32> {
    let len = samples(if open { 0.5 } else { 0.3 });
    let mut out = vec![0.0; len];
    // The latch: a short, bright click.
    let mut latch = noise(rng, samples(0.012), |t| dsp::ad(t, 0.0005, 0.002));
    Biquad::bandpass(3200.0, 2.0).run(&mut latch);
    mix_into(&mut out, &latch, 0.5, samples(0.003));
    if open {
        // The hinge creaks: stick-slip pulses that speed up as the door
        // swings, each ringing a narrow wooden band.
        let mut creak = vec![0.0; len];
        let (end, f0) = (0.42, rng.range(70.0, 95.0));
        let mut t = 0.03;
        while t < end {
            creak[samples(t)] = (1.0 - t / end) * rng.range(0.6, 1.0);
            t += rng.range(0.9, 1.1) / (f0 * (1.0 + 1.5 * t / end));
        }
        Biquad::bandpass(rng.range(780.0, 980.0), 6.0).run(&mut creak);
        Biquad::bandpass(rng.range(1500.0, 1800.0), 4.0).run(&mut creak);
        mix_into(&mut out, &creak, 4.0, 0);
    } else {
        // The door meets its frame: a hollow wooden thud.
        let at = samples(0.008);
        add_mode(
            &mut out,
            at,
            Mode { freq: rng.range(105.0, 125.0), amp: 1.0, tau: 0.07, glide: 0.9, glide_tau: 0.05 },
        );
        add_mode(
            &mut out,
            at,
            Mode { freq: rng.range(290.0, 340.0), amp: 0.5, tau: 0.035, glide: 1.0, glide_tau: 1.0 },
        );
        add_mode(
            &mut out,
            at,
            Mode { freq: rng.range(720.0, 820.0), amp: 0.2, tau: 0.015, glide: 1.0, glide_tau: 1.0 },
        );
    }
    dsp::finish(out, 0.5)
}

fn pop() -> Vec<f32> {
    // A quick upward "bloop", like a cork: a sine chirp with a soft second
    // partial (pitch varies per pickup at playback).
    let mut out = vec![0.0; samples(0.09)];
    add_mode(&mut out, 0, Mode { freq: 620.0, amp: 1.0, tau: 0.025, glide: 1.8, glide_tau: 0.02 });
    add_mode(&mut out, 0, Mode { freq: 1240.0, amp: 0.15, tau: 0.012, glide: 1.8, glide_tau: 0.02 });
    dsp::finish(out, 0.3)
}

/// A struck-glass partial set: a strong fundamental, a quieter octave and
/// two fast inharmonic overtones that give the "ting".
fn ding(out: &mut [f32], start: usize, freq: f32, amp: f32, tau: f32) {
    for (ratio, a, t) in [(1.0, 1.0, 1.0), (2.0, 0.35, 0.5), (3.01, 0.12, 0.25), (4.16, 0.06, 0.15)] {
        add_mode(out, start, Mode { freq: freq * ratio, amp: amp * a, tau: tau * t, glide: 1.0, glide_tau: 1.0 });
    }
}

/// Synthesizes the bright chime played when an experience orb is collected.
fn orb() -> Vec<f32> {
    // Java's pickup is a short bright ding; playback varies the pitch.
    let mut out = vec![0.0; samples(0.7)];
    ding(&mut out, 0, 1320.0, 1.0, 0.16);
    dsp::finish(out, 0.3)
}

/// Synthesizes the rising arpeggio for experience level milestones.
fn level_up() -> Vec<f32> {
    // A quick rising major arpeggio that rings out on the top note.
    let mut out = vec![0.0; samples(1.8)];
    for (i, (freq, tau)) in [(1046.5, 0.18), (1318.5, 0.18), (1568.0, 0.2), (2093.0, 0.45)].into_iter().enumerate() {
        ding(&mut out, samples(0.07 * i as f32), freq, 0.8, tau);
    }
    dsp::finish(out, 0.35)
}

/// Synthesizes a thrown eye's end: a soft glassy break that rings down.
fn eye_death(rng: &mut Rng) -> Vec<f32> {
    let mut out = vec![0.0; samples(0.8)];
    for (i, freq) in [1760.0, 1480.0, 1175.0].into_iter().enumerate() {
        ding(&mut out, samples(0.035 * i as f32), freq * rng.range(0.98, 1.02), 0.6, 0.18);
    }
    let mut shards = crackle(rng, samples(0.25), (0.2, 0.9), |t| 900.0 * (-t / 0.06).exp());
    Biquad::highpass(3000.0, 0.7).run(&mut shards);
    mix_into(&mut out, &shards, 0.3, 0);
    dsp::finish(out, 0.3)
}

/// Synthesizes an eye settling into a frame: a stony clunk and a low ring.
fn frame_fill(rng: &mut Rng) -> Vec<f32> {
    let mut out = vec![0.0; samples(1.0)];
    add_mode(&mut out, 0, Mode { freq: rng.range(150.0, 170.0), amp: 1.0, tau: 0.05, glide: 0.85, glide_tau: 0.03 });
    let mut knock = noise(rng, samples(0.05), |t| (-t / 0.008).exp());
    Biquad::bandpass(900.0, 1.5).run(&mut knock);
    mix_into(&mut out, &knock, 0.6, 0);
    ding(&mut out, samples(0.01), rng.range(520.0, 560.0), 0.5, 0.35);
    dsp::finish(out, 0.4)
}

/// Synthesizes an End portal opening: a sub-bass rumble that swells under
/// a slow, detuned minor chord, fading over a few seconds.
fn portal_spawn(rng: &mut Rng) -> Vec<f32> {
    let secs = 4.0;
    let len = samples(secs);
    let env = |t: f32| (t / 0.6).min(1.0) * ((secs - t) / 2.5).clamp(0.0, 1.0);
    let mut out = vec![0.0; len];
    for (freq, amp) in [(55.0, 0.5), (110.0, 0.3), (130.8, 0.22), (164.8, 0.18), (220.0, 0.12)] {
        for detune in [0.996, 1.004] {
            let mut phase = 0.0f32;
            for (i, s) in out.iter_mut().enumerate() {
                let t = i as f32 / dsp::RATE;
                phase = (phase + std::f32::consts::TAU * freq * detune / dsp::RATE) % std::f32::consts::TAU;
                *s += phase.sin() * env(t) * amp * 0.5;
            }
        }
    }
    let rumble = noise(rng, len, env);
    let mut lp = OnePole::new(120.0);
    let rumble: Vec<f32> = rumble.iter().map(|&x| lp.process(x) * 3.0).collect();
    mix_into(&mut out, &rumble, 0.8, 0);
    dsp::finish(out, 0.8)
}

/// Synthesizes a wing beat: low noise swelling and falling through a
/// low-pass, like a sail snapping full.
fn dragon_flap(rng: &mut Rng) -> Vec<f32> {
    let secs = 0.7;
    let len = samples(secs);
    let mut out = noise(rng, len, |t| (t / 0.12).min(1.0).powi(2) * (-(t - 0.12).max(0.0) / 0.12).exp());
    let mut lp = Biquad::lowpass(220.0, 0.9);
    for (i, s) in out.iter_mut().enumerate() {
        if i % 64 == 0 {
            let t = i as f32 / dsp::RATE;
            lp.retune(Biquad::lowpass(160.0 + 260.0 * (-(t - 0.1).abs() / 0.08).exp(), 0.9));
        }
        *s = lp.process(*s) * 4.0;
    }
    dsp::finish(out, 0.6)
}

/// Synthesizes a roar: a buzzing, wavering low voice gliding from
/// `glide.0` to `glide.1` Hz, with throaty noise on top.
fn dragon_roar(rng: &mut Rng, secs: f32, glide: (f32, f32)) -> Vec<f32> {
    let len = samples(secs);
    let env = |t: f32| (t / 0.15).min(1.0) * ((secs - t) / (secs * 0.5)).clamp(0.0, 1.0);
    let mut out = vec![0.0; len];
    let wobble = rng.range(5.0, 8.0);
    for (harmonic, amp) in [(1.0, 0.5), (2.0, 0.35), (3.0, 0.25), (4.0, 0.15), (5.0, 0.12), (7.0, 0.06)] {
        let mut phase = rng.f32() * std::f32::consts::TAU;
        for (i, s) in out.iter_mut().enumerate() {
            let t = i as f32 / dsp::RATE;
            let f = glide.0 + (glide.1 - glide.0) * (t / secs) + (t * wobble * std::f32::consts::TAU).sin() * 4.0;
            phase = (phase + std::f32::consts::TAU * f * harmonic / dsp::RATE) % std::f32::consts::TAU;
            *s += phase.sin() * amp * env(t);
        }
    }
    let mut throat = noise(rng, len, env);
    Biquad::bandpass(520.0, 1.0).run(&mut throat);
    mix_into(&mut out, &throat, 0.9, 0);
    dsp::finish(out, 0.75)
}

/// Synthesizes the death: a long falling roar under a rising shimmer.
fn dragon_death(rng: &mut Rng) -> Vec<f32> {
    let secs = 6.0;
    let mut out = dragon_roar(rng, secs, (110.0, 38.0));
    for (i, freq) in [523.3, 659.3, 784.0, 1046.5].into_iter().enumerate() {
        let start = samples(1.5 + i as f32 * 0.6);
        for detune in [0.997, 1.003] {
            let mut phase = 0.0f32;
            for (j, s) in out[start..].iter_mut().enumerate() {
                let t = j as f32 / dsp::RATE;
                let env = (t / 1.0).min(1.0) * ((secs - 1.5 - i as f32 * 0.6 - t) / 1.5).clamp(0.0, 1.0);
                phase = (phase + std::f32::consts::TAU * freq * detune / dsp::RATE) % std::f32::consts::TAU;
                *s += phase.sin() * env * 0.06;
            }
        }
    }
    dsp::finish(out, 0.8)
}

/// Synthesizes a blaze's launch whoosh with a decaying flame crackle.
fn fireball(rng: &mut Rng) -> Vec<f32> {
    // A roaring whoosh: noise swept down through a band-pass, with a
    // crackle of flame on top.
    let secs = 0.7;
    let len = samples(secs);
    let mut out = noise(rng, len, |t| (t / 0.03).min(1.0) * (-(t / 0.25)).exp());
    let mut band = Biquad::bandpass(1800.0, 1.2);
    for (i, s) in out.iter_mut().enumerate() {
        if i % 64 == 0 {
            let t = i as f32 / dsp::RATE;
            band.retune(Biquad::bandpass(300.0 + 1500.0 * (-t / 0.15).exp(), 1.2));
        }
        *s = band.process(*s);
    }
    let mut crack = crackle(rng, len, (0.3, 1.5), |t| 400.0 * (-t / 0.3).exp());
    Biquad::highpass(2000.0, 0.7).run(&mut crack);
    mix_into(&mut out, &crack, 0.35, 0);
    dsp::finish(out, 0.45)
}

/// Synthesizes the sweeping tones and breathy noise of a teleport.
fn teleport(rng: &mut Rng) -> Vec<f32> {
    // Java's "vwoop": a few detuned tones swooping up then down, with a
    // breathy band of noise riding the same sweep.
    let secs = 0.55;
    let len = samples(secs);
    let peak = rng.range(0.14, 0.2);
    let sweep = |t: f32| {
        if t < peak { 260.0 + 900.0 * (t / peak).powi(2) } else { 1160.0 * (-(t - peak) / 0.12).exp() + 180.0 }
    };
    let env = |t: f32| (t / 0.03).min(1.0) * ((secs - t) / 0.25).clamp(0.0, 1.0);
    let mut out = vec![0.0; len];
    for detune in [1.0, 1.013, 0.987] {
        let mut phase = 0.0f32;
        for (i, s) in out.iter_mut().enumerate() {
            let t = i as f32 / dsp::RATE;
            phase = (phase + std::f32::consts::TAU * sweep(t) * detune / dsp::RATE) % std::f32::consts::TAU;
            *s += phase.sin() * env(t) * 0.33;
        }
    }
    let mut air = noise(rng, len, env);
    let mut band = Biquad::bandpass(1000.0, 3.0);
    for (i, s) in air.iter_mut().enumerate() {
        if i % 64 == 0 {
            band.retune(Biquad::bandpass(sweep(i as f32 / dsp::RATE) * 1.5, 3.0));
        }
        *s = band.process(*s);
    }
    mix_into(&mut out, &air, 0.6, 0);
    dsp::finish(out, 0.4)
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

fn rain(rng: &mut Rng) -> Vec<f32> {
    let loop_secs = 8.0;
    let (n, x) = (samples(loop_secs), samples(0.5));
    // Countless tiny droplet ticks over a soft hiss, with a gentle swell
    // whose period divides the loop length.
    let mut drops = crackle(rng, n + x, (0.3, 1.2), |t| 2400.0 * (0.8 + 0.2 * (TAU * t / loop_secs * 2.0).sin()));
    Biquad::highpass(1800.0, 0.7).run(&mut drops);
    Biquad::lowpass(9000.0, 0.7).run(&mut drops);
    let mut hiss = noise(rng, n + x, |_| 1.0);
    Biquad::bandpass(1400.0, 0.6).run(&mut hiss);
    let mut rumble = noise(rng, n + x, |_| 1.0);
    Biquad::lowpass(220.0, 0.7).run(&mut rumble);
    mix_into(&mut drops, &hiss, 0.5, 0);
    mix_into(&mut drops, &rumble, 0.8, 0);
    dsp::make_loop(drops, n, x, 0.12)
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

/// Original note-block timbres, centered on F# in the instrument's octave.
/// The mixer resamples by 2^((note-12)/12), keeping all 25 pitches exact.
fn note_sound(instrument: Instrument, rng: &mut Rng) -> Vec<f32> {
    use Instrument::*;
    let frequency = match instrument {
        Bass | Didgeridoo => 92.4986,
        Guitar => 184.9972,
        Bell | Chime | Xylophone => 1479.9777,
        Flute | CowBell => 739.9888,
        _ => 369.9944,
    };
    let duration = match instrument {
        Hat => 0.12,
        Snare | BassDrum => 0.25,
        Flute | Didgeridoo => 0.8,
        _ => 1.1,
    };
    let mut out = vec![0.0; samples(duration)];
    let mut low_noise = 0.0;
    for (i, sample) in out.iter_mut().enumerate() {
        let t = i as f32 / RATE;
        let phase = TAU * frequency * t;
        let tone = match instrument {
            Harp => phase.sin() + 0.3 * (phase * 2.0).sin() * (-t * 9.0).exp(),
            Bass => phase.sin() + 0.35 * (phase * 2.0).sin(),
            Snare => 0.8 * rng.bi() + 0.2 * (TAU * 180.0 * t).sin(),
            Hat => {
                let n = rng.bi();
                low_noise += 0.2 * (n - low_noise);
                n - low_noise
            }
            BassDrum => (TAU * (65.0 * t + 2.0 * (1.0 - (-t * 30.0).exp()))).sin(),
            Bell => phase.sin() + 0.45 * (phase * 2.756).sin(),
            Flute => phase.sin() + 0.1 * (phase * 3.0).sin(),
            Chime => phase.sin() + 0.3 * (phase * 4.0).sin(),
            Guitar => phase.sin() + 0.5 * (phase * 2.0).sin() + 0.2 * (phase * 3.0).sin(),
            Xylophone => phase.sin() + 0.55 * (phase * 3.0).sin() * (-t * 20.0).exp(),
            IronXylophone => phase.sin() + 0.4 * (phase * 2.4).sin(),
            CowBell => 0.6 * phase.sin().signum() + 0.4 * (phase * 1.48).sin(),
            Didgeridoo => phase.sin() + 0.5 * (phase * 3.0).sin() + 0.2 * (phase * 5.0).sin(),
            Bit => phase.sin().signum() + 0.15 * (phase * 0.5).sin().signum(),
            Banjo => phase.sin() + 0.7 * (phase * 2.0).sin() * (-t * 12.0).exp() + 0.2 * (phase * 4.0).sin(),
            Pling => phase.sin() + 0.4 * (phase * 2.0).sin() + 0.3 * (phase * 4.0).sin(),
        };
        let decay = match instrument {
            Hat => 40.0,
            Snare | BassDrum => 20.0,
            Xylophone | Banjo => 9.0,
            Flute | Didgeridoo => 2.5,
            _ => 5.0,
        };
        let attack = if matches!(instrument, Flute | Didgeridoo) { (t * 35.0).min(1.0) } else { (t * 500.0).min(1.0) };
        *sample = tone * attack * (-t * decay).exp();
    }
    dsp::finish(out, 0.65)
}
