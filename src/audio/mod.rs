//! Procedural sound: every sound is synthesized in code at startup (see
//! [`sounds`]) and mixed on the audio device's callback thread (see
//! [`mixer`]). The game talks to the mixer only through [`Audio`], which
//! sends small `Copy` commands over a bounded lock-free channel.
//!
//! Synthesis and device setup happen on a background thread, so startup is
//! never blocked. Without an output device the game runs silently.
//!
//! v1.0 Track 3 (owner, 2026-11-25): the blocks' own sounds ([`Bank::lazy`],
//! 44 MB of the 150 MB bank) are synthesized on first use instead of at
//! startup. The first play of one goes to that background thread, which
//! renders its buffers and then hands the play to the mixer; the device
//! callback never synthesizes. Walking onto a block asks for its sounds
//! ahead of time ([`Audio::prefetch_block`]).

pub mod block_sounds;
mod breeze;
mod crafter;
mod dsp;
mod events;
mod export;
mod flight;
mod illager;
mod minecart;
pub mod mixer;
mod raid;
mod sniffer;
pub mod sounds;
mod source;
pub mod subtitles;
mod sulfur_cube;
mod voices;
mod warden;
mod weapons;
mod wither;

use std::sync::Arc;
use std::time::Instant;

use crossbeam_channel::{Sender, TrySendError};
use glam::{DVec3, IVec3};

use crate::player::{HALF_WIDTH, Player};
use crate::world::World;
use crate::world::block::Block;
use crate::world::terrain::SEA_LEVEL;
use dsp::Rng;
pub use export::{export_music, export_sounds};
use mixer::{Command, Mixer};
pub use sounds::{Bank, BlockCue, Sound};
pub use source::SoundSource;
use voxelcraft::music::{Context, MusicReader, MusicStream};

/// Horizontal distance between footsteps, in blocks (Minecraft's ~1.67).
const STRIDE: f64 = 1.67;
/// Distance swum between strokes.
const SWIM_STROKE: f64 = 2.4;
/// Interval between hit sounds while mining a block: Java's
/// `MultiPlayerGameMode.continueDestroyBlock` plays one every fourth tick.
const HIT_INTERVAL: f64 = 0.2;

/// A sound copied for LAN clients: (sound, position or `None` for the host's own, gain, pitch range).
pub type Tapped = (Sound, Option<DVec3>, f32, (f32, f32), SoundSource);

/// v1.0 Track 3: a lazily synthesized sound for the audio thread to
/// render, and the mixer command to send once it has (`None`: prefetch).
pub(crate) struct Synth {
    sound: Sound,
    play: Option<Command>,
}

pub struct Audio {
    /// While a LAN world has players, every played sound is copied here for them:
    /// (sound, position or `None` for the host's own, gain, pitch range).
    pub tap: Option<Vec<Tapped>>,
    tx: Option<Sender<Command>>,
    /// The bank the mixer plays from; lazy sounds fill in as they're used.
    bank: Arc<Bank>,
    /// First uses of lazy sounds, to the audio thread.
    synth: Option<Sender<Synth>>,
    /// Lazy sounds already asked for ahead of time, by `Sound::key`.
    prefetched: Vec<bool>,
    music: MusicStream,
    /// Dropping this ends the audio thread (and closes the device).
    _stop: Option<Sender<()>>,
    muted: bool,
    volume: f32,
    rng: Rng,
    // Player-state tracking for footsteps, landings, jumps and water.
    was_on_ground: bool,
    was_in_water: bool,
    /// Last muffle sent: the share of listeners with their head underwater.
    muffle: f32,
    prev_vel_y: f64,
    /// The player's feet last frame: steps and strokes count the distance
    /// actually moved (velocity is kept after friction, as Java's is).
    prev_pos: Option<glam::DVec3>,
    stride: f64,
    swim: f64,
    /// The block underfoot, whose step sound footsteps play.
    ground: Option<Block>,
    ambience_timer: f64,
    /// Eyes of the listeners deep in caves, where drips play.
    caves: Vec<DVec3>,
    drip_timer: f64,
    hit_timer: f64,
    /// v0.18 Track 9: Closed Captions, and the captions while it is on.
    show_subtitles: bool,
    subtitles: subtitles::Subtitles,
    /// The listener's eye, where sounds with no position play.
    listener: DVec3,
    /// When this `Audio` started: captions time their sounds from it.
    clock: Instant,
}

impl Audio {
    /// Starts the audio thread. `volume` is the master volume (0..1).
    pub fn new(muted: bool, volume: f32) -> Self {
        let volume = volume.clamp(0.0, 1.0);
        let (tx, rx) = crossbeam_channel::bounded::<Command>(256);
        let (synth_tx, synth_rx) = crossbeam_channel::bounded::<Synth>(256);
        let (stop_tx, stop_rx) = crossbeam_channel::bounded::<()>(0);
        let seed =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);
        let (music, reader) = MusicStream::new(seed);
        let master = if muted { 0.0 } else { volume };
        // Read the block sound types now rather than at the first footstep.
        block_sounds::warm();
        let bank = Arc::new(Bank::empty());
        let thread_bank = Arc::clone(&bank);
        let mixer_tx = tx.clone();
        let spawned = std::thread::Builder::new().name("audio".into()).spawn(move || {
            let start = Instant::now();
            thread_bank.render_startup();
            log::info!(
                "synthesized {} sounds ({} buffers, {:.1} s of audio; blocks' own on first use) in {:.1} ms",
                Sound::COUNT,
                thread_bank.buffers.len(),
                thread_bank.total_samples() as f32 / dsp::RATE,
                start.elapsed().as_secs_f64() * 1000.0
            );
            match device::open(Arc::clone(&thread_bank), rx, master, reader) {
                Ok(stream) => {
                    // Serve first uses until the game drops its `Audio`.
                    serve(&thread_bank, &synth_rx, &mixer_tx, &stop_rx);
                    drop(stream);
                }
                Err(e) => log::warn!("audio disabled, running silently: {e}"),
            }
        });
        let (tx, synth, stop) = match spawned {
            Ok(_) => (Some(tx), Some(synth_tx), Some(stop_tx)),
            Err(e) => {
                log::warn!("audio disabled, could not start audio thread: {e}");
                (None, None, None)
            }
        };

        Self {
            tap: None,
            tx,
            bank,
            synth,
            prefetched: vec![false; Sound::COUNT],
            music,
            _stop: stop,
            muted,
            volume,
            rng: Rng::new(seed),
            was_on_ground: true,
            was_in_water: false,
            muffle: 0.0,
            prev_vel_y: 0.0,
            prev_pos: None,
            stride: 0.0,
            swim: 0.0,
            ground: None,
            ambience_timer: 0.0,
            caves: Vec::new(),
            drip_timer: 5.0,
            hit_timer: 0.0,
            show_subtitles: false,
            subtitles: Default::default(),
            listener: DVec3::ZERO,
            clock: Instant::now(),
        }
    }

    fn send(&mut self, cmd: Command) {
        if let Some(tx) = &self.tx
            && let Err(TrySendError::Disconnected(_)) = tx.try_send(cmd)
        {
            // The audio thread gave up (no device): stop sending.
            self.tx = None;
        }
        // A full queue just drops the command: sounds are best-effort.
    }

    /// Plays `sound` at a world position (or at the listener if `None`),
    /// with a random pitch in `pitch`.
    pub fn play(&mut self, sound: Sound, pos: Option<DVec3>, gain: f32, pitch: (f32, f32)) {
        self.play_in(sound.source(), sound, pos, gain, pitch);
    }

    /// Like [`Audio::play`], for a sound Java plays in a different category
    /// than the sound's own (see [`Sound::source`]), such as a footstep
    /// standing in for a hoe tilling soil (`BLOCKS`).
    pub fn play_in(&mut self, source: SoundSource, sound: Sound, pos: Option<DVec3>, gain: f32, pitch: (f32, f32)) {
        if let Some(tap) = &mut self.tap
            && tap.len() < 512
        {
            tap.push((sound, pos, gain, pitch, source));
        }
        if self.show_subtitles && !self.muted && gain > 0.0 {
            let time = self.clock.elapsed().as_secs_f64();
            self.subtitles.play(sound, pos.unwrap_or(self.listener), pos.is_none(), gain, time);
        }
        if self.muted || self.tx.is_none() {
            return;
        }
        let variant = self.rng.next_u32();
        let pitch = self.rng.range(pitch.0, pitch.1);
        let pos = pos.map(|p| p.as_vec3().to_array());
        let play = Command::Play { sound, variant, gain, pitch, source, pos };
        if Bank::lazy(sound) && !self.bank.is_ready(sound) {
            // First use: the audio thread renders it, then plays it.
            self.request(Synth { sound, play: Some(play) });
        } else {
            self.send(play);
        }
    }

    /// Hands a lazy sound to the audio thread.
    fn request(&mut self, synth: Synth) {
        if let Some(tx) = &self.synth
            && let Err(TrySendError::Disconnected(_)) = tx.try_send(synth)
        {
            self.synth = None;
        }
    }

    /// v1.0 Track 3: asks the audio thread for `block`'s step, break and
    /// place sounds ahead of their first use (when the player walks onto
    /// it, or first hears it), so those uses don't wait for synthesis.
    fn prefetch_block(&mut self, block: Block) {
        if self.muted || self.synth.is_none() {
            return;
        }
        for cue in [BlockCue::Step, BlockCue::Break, BlockCue::Place] {
            let Some((sound, ..)) = block_sounds::play(block, cue) else { continue };
            if Bank::lazy(sound) && !self.prefetched[sound.key()] && !self.bank.is_ready(sound) {
                self.prefetched[sound.key()] = true;
                self.request(Synth { sound, play: None });
            }
        }
    }

    fn at_block(pos: IVec3) -> Option<DVec3> {
        Some(pos.as_dvec3() + DVec3::splat(0.5))
    }

    /// v0.18 Track 9: one of a block's own sounds ([`block_sounds::play`])
    /// at `pos`, `gain` times Java's volume for it, in `source`.
    fn block_sound(&mut self, block: Block, cue: BlockCue, pos: Option<DVec3>, gain: f32, source: SoundSource) {
        // A block heard once (a step, a mining hit) is soon heard breaking
        // or being placed: ask for those now.
        self.prefetch_block(block);
        if let Some((sound, volume, pitch)) = block_sounds::play(block, cue) {
            self.play_in(source, sound, pos, volume * gain, (pitch, pitch));
        }
    }

    pub fn block_break(&mut self, block: Block, pos: IVec3) {
        self.block_sound(block, BlockCue::Break, Self::at_block(pos), 1.0, SoundSource::Blocks);
    }

    pub fn block_place(&mut self, block: Block, pos: IVec3) {
        self.block_sound(block, BlockCue::Place, Self::at_block(pos), 1.0, SoundSource::Blocks);
    }

    /// Call every frame while a block is being mined: its hit sound every
    /// fourth tick, in `BLOCKS`.
    pub fn block_hit(&mut self, block: Block, pos: IVec3, dt: f64) {
        self.hit_timer -= dt;
        if self.hit_timer <= 0.0 {
            self.hit_timer = HIT_INTERVAL;
            self.block_sound(block, BlockCue::Hit, Self::at_block(pos), 1.0, SoundSource::Blocks);
        }
    }

    /// v0.18 Track 9: turns Closed Captions on or off (captions on screen
    /// go with it).
    pub fn set_subtitles(&mut self, on: bool) {
        if !on {
            self.subtitles.clear();
        }
        self.show_subtitles = on;
    }

    /// The captions to draw now for a listener at `ear` looking along
    /// `forward` (`right` to its right), oldest first.
    pub fn subtitles(&mut self, ear: DVec3, forward: DVec3, right: DVec3) -> Vec<subtitles::Shown> {
        if !self.show_subtitles || self.subtitles.is_empty() {
            return Vec::new();
        }
        let time = self.clock.elapsed().as_secs_f64();
        self.subtitles.shown(ear, forward, right, time)
    }

    /// Fades out the world's ambience and lifts underwater muffling (back
    /// to the title screen).
    pub fn leave_world(&mut self) {
        self.subtitles.clear();
        self.music.set_context(Context::default());
        self.muffle = 0.0;
        self.send(Command::Muffle(0.0));
        self.send(Command::Ambience { wind: 0.0, cave: 0.0, rain: 0.0 });
    }

    pub fn ui_click(&mut self) {
        self.play(Sound::Click, None, 0.8, (0.97, 1.03));
    }

    /// Sets a category's volume (0..1). Master takes effect when not muted;
    /// every other category is multiplied by it. Playing sounds follow at once.
    pub fn set_volume(&mut self, source: SoundSource, volume: f32) {
        let volume = if volume.is_nan() { 0.0 } else { volume.clamp(0.0, 1.0) };
        if source == SoundSource::Master {
            self.volume = volume;
            if self.muted {
                return;
            }
        }
        self.send(Command::Volume(source, volume));
    }

    /// Sets every category from `volume` (the player's options).
    pub fn set_volumes(&mut self, volume: impl Fn(SoundSource) -> f32) {
        for source in SoundSource::ALL {
            self.set_volume(source, volume(source));
        }
    }

    /// Update once per frame even while the simulation is paused. The host
    /// chooses the shared stream; underwater music requires an ocean biome.
    pub fn update_music(&self, player: &Player, world: &World, creative: bool, dragon: bool, credits: bool) {
        let column = world.generator.column(player.pos.x.floor() as i32, player.pos.z.floor() as i32);
        self.music.set_context(Context {
            title: false,
            credits,
            dimension: world.generator.dimension,
            creative,
            underwater: player.head_in_water(world),
            biome: column.biome,
            nether: world
                .generator
                .nether_biome_at(player.pos.floor().as_ivec3())
                .map_or(voxelcraft::music::Situation::NetherWastes, voxelcraft::music::Situation::nether),
            dragon,
        });
    }

    /// Toggles mute; returns whether sound is now muted.
    pub fn toggle_mute(&mut self) -> bool {
        self.muted = !self.muted;
        let master = if self.muted { 0.0 } else { self.volume };
        self.send(Command::Volume(SoundSource::Master, master));
        self.muted
    }

    /// The block underfoot's step (`cue` Step) or fall sound at the
    /// listener, `gain` times Java's volume.
    fn step(&mut self, cue: BlockCue, gain: f32) {
        if let Some(block) = self.ground {
            self.block_sound(block, cue, None, gain, SoundSource::Players);
        }
    }

    /// Another player's footsteps: `stride` accumulates the distance they've
    /// walked on the ground, and a step sounds at their feet every stride.
    pub fn other_steps(&mut self, player: &Player, world: &World, walked: f64, stride: &mut f64) {
        if !player.on_ground || player.in_water || player.flying {
            *stride = STRIDE * 0.6;
            return;
        }
        *stride += walked;
        if *stride < STRIDE {
            return;
        }
        *stride -= STRIDE;
        if let Some(b) = ground_block(player, world) {
            let gain = if player.sneaking { 0.2 / 0.55 } else { 1.0 };
            self.block_sound(b, BlockCue::Step, Some(player.pos), gain, SoundSource::Players);
        }
    }

    /// Per-frame update from the player's state: listener position,
    /// footsteps, jumps, landings, water entry, underwater muffling and
    /// ambience. `rain` is how hard it's raining where the player stands
    /// (0..1). `others` are the split-screen players and the rain at each:
    /// sounds near any of them are heard as if by the nearest ear (beyond the
    /// first few are ignored), and they share the speakers' ambience and
    /// muffling.
    pub fn update(&mut self, player: &Player, world: &World, rain: f32, others: &[(&Player, f32)], dt: f64) {
        if self.tx.is_none() {
            return;
        }
        let eye = player.eye();
        self.listener = eye;
        let pos = eye.as_vec3().to_array();
        self.send(Command::Listener { pos, yaw: player.yaw });
        let mut ears = [[0.0; 4]; mixer::MAX_OTHERS];
        for (o, (p, _)) in ears.iter_mut().zip(others) {
            let e = p.eye().as_vec3();
            *o = [e.x, e.y, e.z, p.yaw];
        }
        let count = others.len().min(mixer::MAX_OTHERS) as u8;
        self.send(Command::Others { count, ears });

        let listeners = || std::iter::once((player, rain)).chain(others.iter().copied());
        // Fully muffled only when everyone is underwater, so one diver
        // doesn't deafen the others.
        let wet = listeners().filter(|(p, _)| p.head_in_water(world)).count();
        let muffle = wet as f32 / (1 + others.len()) as f32;
        if muffle != self.muffle {
            self.muffle = muffle;
            self.send(Command::Muffle(muffle));
        }

        let speed = player.vel.x.hypot(player.vel.z);
        // A jump of more than a few blocks in one frame is a teleport, not a walk.
        let moved = self.prev_pos.map_or(glam::DVec3::ZERO, |p| player.pos - p);
        let moved = if moved.length_squared() > 16.0 { glam::DVec3::ZERO } else { moved };
        self.prev_pos = Some(player.pos);
        if player.on_ground && !player.in_water {
            if let Some(b) = ground_block(player, world) {
                if self.ground != Some(b) {
                    self.prefetch_block(b);
                }
                self.ground = Some(b);
            }
            if !self.was_on_ground {
                // Landed: the impact speed is last frame's fall velocity.
                // A hard landing plays the block's fall sound
                // (`playBlockFallSound`), a soft one a step.
                let impact = -self.prev_vel_y;
                if impact > 12.0 {
                    let g = ((impact - 12.0) / 16.0).clamp(0.35, 1.0) as f32;
                    self.play(Sound::Land, None, g, (0.9, 1.05));
                    self.step(BlockCue::Fall, 1.0);
                } else if impact > 4.0 {
                    self.step(BlockCue::Step, 0.45 / 0.55);
                }
                self.stride = 0.0;
            } else if speed > 0.5 {
                self.stride += moved.with_y(0.0).length();
                if self.stride >= STRIDE {
                    self.stride -= STRIDE;
                    // Sneaking is near silent.
                    self.step(BlockCue::Step, if player.sneaking { 0.2 / 0.55 } else { 1.0 });
                }
            } else {
                // Standing still: the first step comes soon after moving.
                self.stride = STRIDE * 0.6;
            }
        }
        if self.was_on_ground && !player.on_ground && player.vel.y > 4.0 && !player.in_water {
            self.step(BlockCue::Step, 0.35 / 0.55); // jump push-off
        }

        if player.in_water && !self.was_in_water && !player.flying {
            let impact = (-self.prev_vel_y).max(0.0);
            if impact > 2.5 {
                let g = (0.35 + impact / 20.0).min(1.0) as f32;
                self.play(Sound::Splash, None, g, (0.9, 1.1));
            } else {
                self.play(Sound::Swim, None, 0.35, (0.9, 1.1));
            }
            self.swim = 0.0;
        } else if player.in_water && !player.flying {
            let v = player.vel.length();
            if v > 1.0 {
                self.swim += moved.length();
                if self.swim >= SWIM_STROKE {
                    self.swim = 0.0;
                    self.play(Sound::Swim, None, 0.3, (0.85, 1.15));
                }
            }
        }

        self.was_on_ground = player.on_ground;
        self.was_in_water = player.in_water;
        self.prev_vel_y = player.vel.y;

        // Ambience: re-evaluated twice a second, faded by the mixer. Each
        // loop plays at the loudest level any listener would hear.
        self.ambience_timer -= dt;
        if self.ambience_timer <= 0.0 {
            self.ambience_timer = 0.5;
            self.caves.clear();
            let (mut wind, mut cave, mut rain) = (0.0f32, 0.0f32, 0.0f32);
            for (p, rain_here) in listeners() {
                let eye = p.eye();
                let underwater = p.head_in_water(world);
                let (w, c, covered) = ambience(eye, world);
                if c > 0.5 {
                    self.caves.push(eye);
                }
                wind = wind.max(if underwater { w * 0.3 } else { w });
                cave = cave.max(c);
                // Rain drums on the roof when sheltered and fades out deep underground.
                let r = rain_here
                    * (1.0 - c)
                    * if underwater {
                        0.2
                    } else if covered {
                        0.4
                    } else {
                        1.0
                    };
                rain = rain.max(r);
            }
            self.send(Command::Ambience { wind, cave, rain });
            // Java's rain plays `weather.rain` again and again around the
            // player; the loop here refreshes its caption instead.
            if rain > 0.05 && self.show_subtitles && !self.muted {
                let time = self.clock.elapsed().as_secs_f64();
                self.subtitles.play(Sound::Rain, self.listener, true, rain, time);
            }
        }
        if !self.caves.is_empty() {
            self.drip_timer -= dt;
            if self.drip_timer <= 0.0 {
                self.drip_timer = self.rng.range(4.0, 14.0) as f64;
                let eye = self.caves[self.rng.next_u32() as usize % self.caves.len()];
                let off = DVec3::new(
                    self.rng.range(-8.0, 8.0) as f64,
                    self.rng.range(1.0, 5.0) as f64,
                    self.rng.range(-8.0, 8.0) as f64,
                );
                self.play(Sound::Drip, Some(eye + off), 0.7, (0.85, 1.15));
            }
        }
    }
}

/// The solid block under the player's feet (centre first, then corners so
/// standing on an edge still finds it).
fn ground_block(player: &Player, world: &World) -> Option<Block> {
    let y = player.pos.y - 0.05;
    let w = HALF_WIDTH - 0.01;
    [(0.0, 0.0), (-w, -w), (w, -w), (-w, w), (w, w)].into_iter().find_map(|(dx, dz)| {
        let p = DVec3::new(player.pos.x + dx, y, player.pos.z + dz).floor().as_ivec3();
        world.get_block(p).filter(|b| b.is_solid())
    })
}

/// Wind and cave ambience levels (0..1) at the listener, and whether
/// something overhead shelters it.
fn ambience(eye: DVec3, world: &World) -> (f32, f32, bool) {
    // The Nether is one huge cave.
    if world.generator.dimension == crate::world::terrain::Dimension::End {
        return (0.0, 0.0, false);
    }
    if world.generator.dimension == crate::world::terrain::Dimension::Nether {
        return (0.0, 1.0, true);
    }
    let p = eye.floor().as_ivec3();
    let covered = (1..=32).any(|dy| world.get_block(p + IVec3::Y * dy).is_some_and(|b| b.is_opaque()));
    let surface = world.generator.column(p.x, p.z).height;
    let depth = (surface - p.y) as f32;
    let cave = if covered { ((depth - 4.0) / 12.0).clamp(0.0, 1.0) } else { 0.0 };
    let altitude = ((eye.y as f32 - SEA_LEVEL as f32) / 80.0).clamp(0.0, 1.0);
    let shelter = if covered { 0.5 } else { 1.0 };
    let wind = (1.0 - cave) * (0.35 + 0.65 * altitude) * shelter;
    (wind, cave, covered)
}

/// v1.0 Track 3: the audio thread's work once the device is open: render
/// each lazy sound on its first use and pass its play on to the mixer,
/// until the game drops its `Audio` (`stop` closes) or stops asking.
pub(crate) fn serve(
    bank: &Bank,
    synth: &crossbeam_channel::Receiver<Synth>,
    mixer: &Sender<Command>,
    stop: &crossbeam_channel::Receiver<()>,
) {
    loop {
        crossbeam_channel::select! {
            recv(stop) -> _ => return,
            recv(synth) -> msg => {
                let Ok(Synth { sound, play }) = msg else { return };
                bank.ensure(sound);
                if let Some(play) = play {
                    // A full queue drops it, as for any other sound.
                    let _ = mixer.try_send(play);
                }
            }
        }
    }
}

/// Output device setup (cpal).
mod device {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{FromSample, SampleFormat, SizedSample, Stream, StreamConfig};

    use super::*;

    pub fn open(
        bank: Arc<Bank>,
        rx: crossbeam_channel::Receiver<Command>,
        master: f32,
        music: MusicReader,
    ) -> Result<Stream, String> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("no audio output device")?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let format = supported.sample_format();
        let config: StreamConfig = supported.into();
        let mixer = Mixer::new(bank, rx, config.sample_rate as f32, master).with_music(music);
        let stream = match format {
            SampleFormat::F32 => build::<f32>(&device, &config, mixer),
            SampleFormat::I16 => build::<i16>(&device, &config, mixer),
            SampleFormat::U16 => build::<u16>(&device, &config, mixer),
            SampleFormat::I32 => build::<i32>(&device, &config, mixer),
            SampleFormat::F64 => build::<f64>(&device, &config, mixer),
            other => return Err(format!("unsupported sample format {other}")),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        log::info!("audio output: {} Hz, {} channels, {format}", config.sample_rate, config.channels);
        Ok(stream)
    }

    fn build<T: SizedSample + FromSample<f32>>(
        device: &cpal::Device,
        config: &StreamConfig,
        mut mixer: Mixer,
    ) -> Result<Stream, String> {
        let channels = (config.channels as usize).clamp(1, 64);
        let mut scratch = [0.0f32; 2048];
        let chunk = scratch.len() / channels * channels;
        device
            .build_output_stream(
                *config,
                move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                    for out in data.chunks_mut(chunk) {
                        let tmp = &mut scratch[..out.len()];
                        mixer.render(tmp, channels);
                        for (d, s) in out.iter_mut().zip(tmp.iter()) {
                            *d = T::from_sample(*s);
                        }
                    }
                },
                |e| log::warn!("audio stream error: {e}"),
                None,
            )
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests;
