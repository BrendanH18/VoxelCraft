//! Procedural sound: every sound is synthesized in code at startup (see
//! [`sounds`]) and mixed on the audio device's callback thread (see
//! [`mixer`]). The game talks to the mixer only through [`Audio`], which
//! sends small `Copy` commands over a bounded lock-free channel.
//!
//! Synthesis and device setup happen on a background thread, so startup is
//! never blocked. Without an output device the game runs silently.

mod dsp;
mod export;
pub mod mixer;
pub mod sounds;
mod voices;

use std::sync::Arc;
use std::time::Instant;

use crossbeam_channel::{Sender, TrySendError};
use glam::{DVec3, IVec3};

use crate::player::{HALF_WIDTH, Player};
use crate::world::World;
use crate::world::block::Block;
use crate::world::terrain::SEA_LEVEL;
use dsp::Rng;
pub use export::export_sounds;
use mixer::{Command, Mixer};
pub use sounds::{Bank, Material, Sound, material};

/// Horizontal distance between footsteps, in blocks (Minecraft's ~1.67).
const STRIDE: f64 = 1.67;
/// Distance swum between strokes.
const SWIM_STROKE: f64 = 2.4;
/// Interval between hit sounds while mining a block.
const HIT_INTERVAL: f64 = 0.22;

pub struct Audio {
    tx: Option<Sender<Command>>,
    /// Dropping this ends the audio thread (and closes the device).
    _stop: Option<Sender<()>>,
    muted: bool,
    volume: f32,
    rng: Rng,
    // Player-state tracking for footsteps, landings, jumps and water.
    was_on_ground: bool,
    was_in_water: bool,
    was_underwater: bool,
    prev_vel_y: f64,
    stride: f64,
    swim: f64,
    ground: Material,
    ambience_timer: f64,
    cave: f32,
    drip_timer: f64,
    hit_timer: f64,
}

impl Audio {
    /// Starts the audio thread. `volume` is the master volume (0..1).
    pub fn new(muted: bool, volume: f32) -> Self {
        let volume = volume.clamp(0.0, 1.0);
        let (tx, rx) = crossbeam_channel::bounded::<Command>(256);
        let (stop_tx, stop_rx) = crossbeam_channel::bounded::<()>(0);
        let master = if muted { 0.0 } else { volume };
        let spawned = std::thread::Builder::new().name("audio".into()).spawn(move || {
            let start = Instant::now();
            let bank = Arc::new(Bank::synthesize());
            log::info!(
                "synthesized {} sounds ({} buffers, {:.1} s of audio) in {:.1} ms",
                Sound::COUNT,
                bank.buffers.len(),
                bank.total_samples() as f32 / dsp::RATE,
                start.elapsed().as_secs_f64() * 1000.0
            );
            match device::open(bank, rx, master) {
                Ok(stream) => {
                    // Park until the game drops its `Audio`.
                    let _ = stop_rx.recv();
                    drop(stream);
                }
                Err(e) => log::warn!("audio disabled, running silently: {e}"),
            }
        });
        let (tx, stop) = match spawned {
            Ok(_) => (Some(tx), Some(stop_tx)),
            Err(e) => {
                log::warn!("audio disabled, could not start audio thread: {e}");
                (None, None)
            }
        };
        let seed =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);
        Self {
            tx,
            _stop: stop,
            muted,
            volume,
            rng: Rng::new(seed),
            was_on_ground: true,
            was_in_water: false,
            was_underwater: false,
            prev_vel_y: 0.0,
            stride: 0.0,
            swim: 0.0,
            ground: Material::Grass,
            ambience_timer: 0.0,
            cave: 0.0,
            drip_timer: 5.0,
            hit_timer: 0.0,
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
        if self.muted || self.tx.is_none() {
            return;
        }
        let variant = self.rng.next_u32();
        let pitch = self.rng.range(pitch.0, pitch.1);
        let pos = pos.map(|p| p.as_vec3().to_array());
        self.send(Command::Play { sound, variant, gain, pitch, pos });
    }

    fn at_block(pos: IVec3) -> Option<DVec3> {
        Some(pos.as_dvec3() + DVec3::splat(0.5))
    }

    pub fn block_break(&mut self, block: Block, pos: IVec3) {
        self.play(Sound::Break(material(block)), Self::at_block(pos), 1.0, (0.85, 1.0));
    }

    pub fn block_place(&mut self, block: Block, pos: IVec3) {
        self.play(Sound::Place(material(block)), Self::at_block(pos), 0.9, (0.85, 1.0));
    }

    /// Call every frame while a block is being mined: plays soft, low hits.
    pub fn block_hit(&mut self, block: Block, pos: IVec3, dt: f64) {
        self.hit_timer -= dt;
        if self.hit_timer <= 0.0 {
            self.hit_timer = HIT_INTERVAL;
            self.play(Sound::Step(material(block)), Self::at_block(pos), 0.45, (0.7, 0.8));
        }
    }

    /// Fades out the world's ambience and lifts underwater muffling (back
    /// to the title screen).
    pub fn leave_world(&mut self) {
        self.was_underwater = false;
        self.send(Command::Muffle(false));
        self.send(Command::Ambience { wind: 0.0, cave: 0.0, rain: 0.0 });
    }

    pub fn ui_click(&mut self) {
        self.play(Sound::Click, None, 0.8, (0.97, 1.03));
    }

    /// Sets the master volume (0..1); takes effect when not muted.
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if !self.muted {
            self.send(Command::Master(self.volume));
        }
    }

    /// Toggles mute; returns whether sound is now muted.
    pub fn toggle_mute(&mut self) -> bool {
        self.muted = !self.muted;
        let master = if self.muted { 0.0 } else { self.volume };
        self.send(Command::Master(master));
        self.muted
    }

    fn step(&mut self, m: Material, gain: f32) {
        self.play(Sound::Step(m), None, gain, (0.9, 1.1));
    }

    /// Per-frame update from the player's state: listener position,
    /// footsteps, jumps, landings, water entry, underwater muffling and
    /// ambience.
    /// `rain` is how hard it's raining where the player stands (0..1).
    pub fn update(&mut self, player: &Player, world: &World, rain: f32, dt: f64) {
        if self.tx.is_none() {
            return;
        }
        let eye = player.eye();
        let pos = eye.as_vec3().to_array();
        self.send(Command::Listener { pos, yaw: player.yaw });

        let underwater = player.head_in_water(world);
        if underwater != self.was_underwater {
            self.was_underwater = underwater;
            self.send(Command::Muffle(underwater));
        }

        let speed = player.vel.x.hypot(player.vel.z);
        if player.on_ground && !player.in_water {
            if let Some(b) = ground_block(player, world) {
                self.ground = material(b);
            }
            if !self.was_on_ground {
                // Landed: the impact speed is last frame's fall velocity.
                let impact = -self.prev_vel_y;
                if impact > 12.0 {
                    let g = ((impact - 12.0) / 16.0).clamp(0.35, 1.0) as f32;
                    self.play(Sound::Land, None, g, (0.9, 1.05));
                    self.step(self.ground, 0.8);
                } else if impact > 4.0 {
                    self.step(self.ground, 0.45);
                }
                self.stride = 0.0;
            } else if speed > 0.5 {
                self.stride += speed * dt;
                if self.stride >= STRIDE {
                    self.stride -= STRIDE;
                    // Sneaking is near silent.
                    self.step(self.ground, if player.sneaking { 0.2 } else { 0.55 });
                }
            } else {
                // Standing still: the first step comes soon after moving.
                self.stride = STRIDE * 0.6;
            }
        }
        if self.was_on_ground && !player.on_ground && player.vel.y > 4.0 && !player.in_water {
            self.step(self.ground, 0.35); // jump push-off
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
                self.swim += v * dt;
                if self.swim >= SWIM_STROKE {
                    self.swim = 0.0;
                    self.play(Sound::Swim, None, 0.3, (0.85, 1.15));
                }
            }
        }

        self.was_on_ground = player.on_ground;
        self.was_in_water = player.in_water;
        self.prev_vel_y = player.vel.y;

        // Ambience: re-evaluated twice a second, faded by the mixer.
        self.ambience_timer -= dt;
        if self.ambience_timer <= 0.0 {
            self.ambience_timer = 0.5;
            let (wind, cave, covered) = ambience(eye, world);
            self.cave = cave;
            let wind = if underwater { wind * 0.3 } else { wind };
            // Rain drums on the roof when sheltered and fades out deep underground.
            let rain = rain
                * (1.0 - cave)
                * if underwater {
                    0.2
                } else if covered {
                    0.4
                } else {
                    1.0
                };
            self.send(Command::Ambience { wind, cave, rain });
        }
        if self.cave > 0.5 {
            self.drip_timer -= dt;
            if self.drip_timer <= 0.0 {
                self.drip_timer = self.rng.range(4.0, 14.0) as f64;
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
    if !world.generator.dimension.has_sky() {
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

/// Output device setup (cpal).
mod device {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{FromSample, SampleFormat, SizedSample, Stream, StreamConfig};

    use super::*;

    pub fn open(bank: Arc<Bank>, rx: crossbeam_channel::Receiver<Command>, master: f32) -> Result<Stream, String> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("no audio output device")?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let format = supported.sample_format();
        let config: StreamConfig = supported.into();
        let mixer = Mixer::new(bank, rx, config.sample_rate as f32, master);
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
