//! Real-time mixer. Runs inside the audio device callback: drains game
//! commands from a bounded lock-free channel, mixes up to [`MAX_VOICES`]
//! resampled, panned and distance-attenuated voices plus two ambient loops,
//! then applies the underwater low-pass, master gain and a peak limiter.
//!
//! Nothing here allocates or locks after construction: voices index into the
//! shared, immutable [`Bank`], and all scratch space is fixed-size.

use std::sync::Arc;

use crossbeam_channel::Receiver;

use super::dsp::{OnePole, RATE};
use super::sounds::{Bank, Sound};

pub const MAX_VOICES: usize = 32;
/// Frames mixed per inner block; gains ramp linearly across a block.
const BLOCK: usize = 256;
/// Distance (blocks) within which positional sounds play at full volume.
pub const NEAR: f32 = 2.0;
/// Distance at which positional sounds become inaudible.
pub const FAR: f32 = 24.0;
/// Largest left/right pan, so nothing is ever fully in one ear.
const MAX_PAN: f32 = 0.75;
const LIMIT: f32 = 0.9;

#[derive(Clone, Copy, Debug)]
pub enum Command {
    Play {
        sound: Sound,
        /// Random number used to pick the variant.
        variant: u32,
        gain: f32,
        pitch: f32,
        /// World position, or `None` for sounds at the listener (UI, own
        /// footsteps).
        pos: Option<[f32; 3]>,
    },
    Listener {
        pos: [f32; 3],
        yaw: f32,
    },
    Master(f32),
    /// Low-pass the whole mix (head underwater).
    Muffle(bool),
    /// Target levels of the wind and cave loops, 0..1.
    Ambience {
        wind: f32,
        cave: f32,
    },
}

#[derive(Clone, Copy)]
struct Voice {
    buf: u16,
    pos: f64,
    step: f64,
    gain: f32,
    spatial: Option<[f32; 3]>,
    /// Gains applied at the end of the last block (ramp start).
    cur: (f32, f32),
}

#[derive(Clone, Copy, Default)]
struct Loop {
    buf: u16,
    pos: f64,
    gain: f32,
    target: f32,
}

/// Distance attenuation: 1 within [`NEAR`], smoothly to 0 at [`FAR`].
pub fn attenuation(dist: f32) -> f32 {
    let x = ((dist - NEAR) / (FAR - NEAR)).clamp(0.0, 1.0);
    (1.0 - x) * (1.0 - x)
}

/// Left/right gains for a source at `rel` (source minus listener) heard by a
/// listener facing `yaw`. Centre is (1, 1); total power stays constant.
pub fn pan_gains(rel: [f32; 3], yaw: f32) -> (f32, f32) {
    let (s, c) = yaw.sin_cos();
    // Must match `Player::forward`/strafe: forward (cos, sin), right (-sin, cos).
    let (x, z) = (rel[0], rel[2]);
    let side = -s * x + c * z;
    let front = c * x + s * z;
    let horiz = (x * x + z * z).sqrt();
    // Sources within a block of the head (or straight above/below) stay centred.
    let pan = (side / horiz.max(1.0)).clamp(-1.0, 1.0) * MAX_PAN;
    // Slightly quieter behind the listener as a front/back cue.
    let behind = if horiz > 1e-3 { 0.9 + 0.1 * (front / horiz) } else { 1.0 };
    ((1.0 - pan).sqrt() * behind, (1.0 + pan).sqrt() * behind)
}

/// Current left/right gains of a voice for the given listener.
fn voice_gains(v: &Voice, listener: [f32; 3], yaw: f32) -> (f32, f32) {
    let Some(p) = v.spatial else { return (v.gain, v.gain) };
    let rel = [p[0] - listener[0], p[1] - listener[1], p[2] - listener[2]];
    let dist = (rel[0] * rel[0] + rel[1] * rel[1] + rel[2] * rel[2]).sqrt();
    let a = attenuation(dist) * v.gain;
    let (l, r) = pan_gains(rel, yaw);
    (l * a, r * a)
}

pub struct Mixer {
    bank: Arc<Bank>,
    rx: Receiver<Command>,
    out_rate: f32,
    voices: [Option<Voice>; MAX_VOICES],
    last_variant: [u16; Sound::COUNT],
    listener: [f32; 3],
    yaw: f32,
    master: f32,
    master_target: f32,
    muffle: f32,
    muffle_target: f32,
    lp: [OnePole; 4],
    limiter: f32,
    ambient: [Loop; 2],
    left: [f32; BLOCK],
    right: [f32; BLOCK],
}

impl Mixer {
    pub fn new(bank: Arc<Bank>, rx: Receiver<Command>, out_rate: f32, master: f32) -> Self {
        let loop_buf = |s: Sound| bank.variants(s).0 as u16;
        let ambient = [
            Loop { buf: loop_buf(Sound::Wind), ..Default::default() },
            Loop { buf: loop_buf(Sound::Cave), ..Default::default() },
        ];
        Self {
            bank,
            rx,
            out_rate,
            voices: [None; MAX_VOICES],
            last_variant: [u16::MAX; Sound::COUNT],
            listener: [0.0; 3],
            yaw: 0.0,
            master,
            master_target: master,
            muffle: 0.0,
            muffle_target: 0.0,
            lp: [OnePole::default(); 4],
            limiter: 1.0,
            ambient,
            left: [0.0; BLOCK],
            right: [0.0; BLOCK],
        }
    }

    #[cfg(test)]
    pub fn active_voices(&self) -> usize {
        self.voices.iter().filter(|v| v.is_some()).count()
    }

    /// Fills an interleaved output buffer with `channels` channels.
    pub fn render(&mut self, out: &mut [f32], channels: usize) {
        while let Ok(cmd) = self.rx.try_recv() {
            self.apply(cmd);
        }
        let channels = channels.max(1);
        for chunk in out.chunks_mut(BLOCK * channels) {
            let frames = chunk.len() / channels;
            self.mix_block(frames);
            for (i, frame) in chunk.chunks_exact_mut(channels).enumerate() {
                let (l, r) = (self.left[i], self.right[i]);
                if channels == 1 {
                    frame[0] = 0.5 * (l + r);
                } else {
                    frame[0] = l;
                    frame[1] = r;
                    frame[2..].fill(0.0);
                }
            }
        }
    }

    pub fn apply(&mut self, cmd: Command) {
        match cmd {
            Command::Play { sound, variant, gain, pitch, pos } => self.play(sound, variant, gain, pitch, pos),
            Command::Listener { pos, yaw } => {
                self.listener = pos;
                self.yaw = yaw;
            }
            Command::Master(g) => self.master_target = g.clamp(0.0, 2.0),
            Command::Muffle(on) => self.muffle_target = if on { 1.0 } else { 0.0 },
            Command::Ambience { wind, cave } => {
                self.ambient[0].target = wind.clamp(0.0, 1.0);
                self.ambient[1].target = cave.clamp(0.0, 1.0);
            }
        }
    }

    fn play(&mut self, sound: Sound, variant: u32, gain: f32, pitch: f32, pos: Option<[f32; 3]>) {
        let (start, count) = self.bank.variants(sound);
        if count == 0 || gain.is_nan() || pitch.is_nan() || gain <= 0.0 || pitch <= 0.0 {
            return;
        }
        // Pick a variant, never the same one twice in a row.
        let key = sound.key();
        let mut idx = (variant as usize % count) as u16;
        if count > 1 && idx == self.last_variant[key] {
            idx = (idx + 1) % count as u16;
        }
        self.last_variant[key] = idx;
        let mut voice = Voice {
            buf: start as u16 + idx,
            pos: 0.0,
            step: pitch as f64 * RATE as f64 / self.out_rate as f64,
            gain,
            spatial: pos,
            cur: (0.0, 0.0),
        };
        let g = voice_gains(&voice, self.listener, self.yaw);
        if g.0.max(g.1) < 1e-4 {
            return; // out of earshot
        }
        voice.cur = g;
        let slot = match self.voices.iter().position(Option::is_none) {
            Some(i) => i,
            // Steal the voice with the least remaining loudness.
            None => {
                let remaining = |v: &Voice| {
                    let len = self.bank.buffers[v.buf as usize].len() as f64;
                    (v.cur.0.max(v.cur.1) as f64) * (1.0 - v.pos / len).max(0.0)
                };
                let (i, weakest) = self
                    .voices
                    .iter()
                    .enumerate()
                    .filter_map(|(i, v)| v.as_ref().map(|v| (i, remaining(v))))
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .unwrap_or((0, 0.0));
                if weakest > g.0.max(g.1) as f64 {
                    return; // everything playing is louder than this
                }
                i
            }
        };
        self.voices[slot] = Some(voice);
    }

    fn mix_block(&mut self, n: usize) {
        let (left, right) = (&mut self.left[..n], &mut self.right[..n]);
        left.fill(0.0);
        right.fill(0.0);
        let inv_n = 1.0 / n.max(1) as f32;

        // --- One-shot voices ------------------------------------------------
        for slot in 0..MAX_VOICES {
            let Some(mut v) = self.voices[slot] else { continue };
            let target = voice_gains(&v, self.listener, self.yaw);
            let buf = &self.bank.buffers[v.buf as usize];
            let len = buf.len();
            let (dl, dr) = ((target.0 - v.cur.0) * inv_n, (target.1 - v.cur.1) * inv_n);
            let (mut gl, mut gr) = v.cur;
            let mut done = false;
            for i in 0..n {
                let idx = v.pos as usize;
                if idx + 1 >= len {
                    done = true;
                    break;
                }
                let frac = (v.pos - idx as f64) as f32;
                let s = buf[idx] + (buf[idx + 1] - buf[idx]) * frac;
                gl += dl;
                gr += dr;
                left[i] += s * gl;
                right[i] += s * gr;
                v.pos += v.step;
            }
            v.cur = target;
            self.voices[slot] = if done { None } else { Some(v) };
        }

        // --- Ambient loops --------------------------------------------------
        // ~1.5 s time constant so ambience fades rather than switches.
        let k = 1.0 - (-(n as f32) / (1.5 * self.out_rate)).exp();
        let step = RATE as f64 / self.out_rate as f64;
        for amb in self.ambient.iter_mut() {
            let start = amb.gain;
            amb.gain += (amb.target - amb.gain) * k;
            let buf = &self.bank.buffers[amb.buf as usize];
            let len = buf.len() as f64;
            if start.max(amb.gain) < 1e-4 || len < 2.0 {
                amb.pos = (amb.pos + step * n as f64) % len.max(1.0);
                continue;
            }
            let dg = (amb.gain - start) * inv_n;
            let mut g = start;
            for i in 0..n {
                let idx = amb.pos as usize;
                let next = if idx + 1 >= buf.len() { 0 } else { idx + 1 };
                let frac = (amb.pos - idx as f64) as f32;
                let s = (buf[idx] + (buf[next] - buf[idx]) * frac) * g;
                g += dg;
                left[i] += s;
                right[i] += s;
                amb.pos += step;
                if amb.pos >= len {
                    amb.pos -= len;
                }
            }
        }

        // --- Master bus: muffle, volume, limiter -------------------------------
        let k = 1.0 - (-(n as f32) / (0.15 * self.out_rate)).exp();
        let m0 = self.muffle;
        self.muffle += (self.muffle_target - self.muffle) * k;
        if m0.max(self.muffle) > 1e-3 {
            // Cutoff sweeps exponentially from 16 kHz down to 600 Hz.
            let fc = 16_000.0 * (600.0f32 / 16_000.0).powf(self.muffle);
            let a = OnePole::coef(fc.min(self.out_rate * 0.45), self.out_rate);
            for lp in self.lp.iter_mut() {
                lp.a = a;
            }
            let wet_gain = 1.0 + 0.4 * self.muffle; // low-passing loses energy
            for i in 0..n {
                let l = self.lp[0].process(left[i]);
                left[i] = self.lp[1].process(l) * wet_gain;
                let r = self.lp[2].process(right[i]);
                right[i] = self.lp[3].process(r) * wet_gain;
            }
        } else if n > 0 {
            // Park the filters on the dry signal so engaging them is click-free.
            let (l, r) = (left[n - 1], right[n - 1]);
            for (i, lp) in self.lp.iter_mut().enumerate() {
                lp.z = if i < 2 { l } else { r };
            }
        }

        let m_start = self.master;
        self.master += (self.master_target - self.master) * (1.0 - (-(n as f32) / (0.02 * self.out_rate)).exp());
        let dm = (self.master - m_start) * inv_n;
        let release = (-1.0 / (0.12 * self.out_rate)).exp();
        let mut m = m_start;
        for i in 0..n {
            m += dm;
            let (l, r) = (left[i] * m, right[i] * m);
            let peak = l.abs().max(r.abs());
            let want = if peak > LIMIT { LIMIT / peak } else { 1.0 };
            self.limiter = if want < self.limiter { want } else { want + (self.limiter - want) * release };
            left[i] = (l * self.limiter).clamp(-1.0, 1.0);
            right[i] = (r * self.limiter).clamp(-1.0, 1.0);
        }
    }
}
