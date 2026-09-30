//! Small DSP toolkit used by the sound synthesizer: a deterministic RNG,
//! biquad and one-pole filters, and buffer post-processing (DC removal,
//! fades, normalisation, seamless loops).

use std::f32::consts::{PI, TAU};

/// Sample rate every sound is synthesized at. The mixer resamples to the
/// device rate on playback.
pub const RATE: f32 = 48_000.0;

/// Converts seconds to a sample count.
pub fn samples(secs: f32) -> usize {
    (secs * RATE) as usize
}

/// Per-sample decay multiplier for an exponential with time constant `tau`
/// seconds.
pub fn decay_coef(tau: f32) -> f32 {
    (-1.0 / (tau * RATE)).exp()
}

/// xorshift64* — deterministic so every run synthesizes the same sounds.
#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        // splitmix64 scramble so small consecutive seeds diverge.
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        Self((z ^ (z >> 31)) | 1)
    }

    pub fn next_u32(&mut self) -> u32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }

    /// Uniform in [0, 1).
    pub fn f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Uniform in [-1, 1).
    pub fn bi(&mut self) -> f32 {
        self.f32() * 2.0 - 1.0
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }
}

/// RBJ-cookbook biquad, transposed direct form II.
#[derive(Clone, Copy)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn from_coefs(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> Self {
        Self { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0, z1: 0.0, z2: 0.0 }
    }

    fn omega(fc: f32, q: f32) -> (f32, f32, f32) {
        let w = TAU * fc.clamp(10.0, RATE * 0.45) / RATE;
        let (s, c) = w.sin_cos();
        (s, c, s / (2.0 * q))
    }

    pub fn lowpass(fc: f32, q: f32) -> Self {
        let (_, c, alpha) = Self::omega(fc, q);
        Self::from_coefs((1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0, 1.0 + alpha, -2.0 * c, 1.0 - alpha)
    }

    pub fn highpass(fc: f32, q: f32) -> Self {
        let (_, c, alpha) = Self::omega(fc, q);
        Self::from_coefs((1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0, 1.0 + alpha, -2.0 * c, 1.0 - alpha)
    }

    /// Band-pass with 0 dB gain at the centre frequency.
    pub fn bandpass(fc: f32, q: f32) -> Self {
        let (_, c, alpha) = Self::omega(fc, q);
        Self::from_coefs(alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * c, 1.0 - alpha)
    }

    /// Takes `other`'s coefficients while keeping this filter's state.
    pub fn retune(&mut self, other: Biquad) {
        *self = Biquad { z1: self.z1, z2: self.z2, ..other };
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }

    pub fn run(mut self, buf: &mut [f32]) {
        for s in buf {
            *s = self.process(*s);
        }
    }
}

/// One-pole low-pass (6 dB/octave), cheap enough to retune per sample.
#[derive(Clone, Copy, Default)]
pub struct OnePole {
    pub a: f32,
    pub z: f32,
}

impl OnePole {
    pub fn coef(fc: f32, rate: f32) -> f32 {
        1.0 - (-TAU * fc / rate).exp()
    }

    pub fn new(fc: f32) -> Self {
        Self { a: Self::coef(fc, RATE), z: 0.0 }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        self.z += self.a * (x - self.z);
        self.z
    }
}

/// A damped sine (one resonant mode). The frequency starts at `freq` and
/// glides exponentially (time constant `glide_tau`) to `freq * glide`.
pub struct Mode {
    pub freq: f32,
    pub amp: f32,
    pub tau: f32,
    pub glide: f32,
    pub glide_tau: f32,
}

/// Adds `m` to `buf` starting at sample `start`.
pub fn add_mode(buf: &mut [f32], start: usize, m: Mode) {
    let len = ((m.tau * 7.0 * RATE) as usize).min(buf.len().saturating_sub(start));
    let k = decay_coef(m.tau);
    let kg = decay_coef(m.glide_tau.max(1e-4));
    let attack = (0.0006 * RATE) as usize; // 0.6 ms: kills the step-onset click
    let (mut env, mut g, mut phase) = (m.amp, 1.0f32, 0.0f32);
    for (i, s) in buf[start..start + len].iter_mut().enumerate() {
        let f = m.freq * (m.glide + (1.0 - m.glide) * g);
        phase = (phase + TAU * f / RATE) % TAU;
        let a = if i < attack { i as f32 / attack as f32 } else { 1.0 };
        *s += phase.sin() * env * a;
        env *= k;
        g *= kg;
    }
}

/// Random-impulse "crackle": at a time-varying rate `density(t)` (events per
/// second) a grain of white noise starts whose energy decays over
/// `grain_ms`. Filtered afterwards, this is the basis of every crunchy sound.
pub fn crackle(rng: &mut Rng, len: usize, grain_ms: (f32, f32), density: impl Fn(f32) -> f32) -> Vec<f32> {
    let mut out = vec![0.0; len];
    let (mut e, mut k) = (0.0f32, 0.0f32);
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / RATE;
        if rng.f32() < density(t) / RATE {
            e = e.max(rng.range(0.25, 1.0));
            k = decay_coef(rng.range(grain_ms.0, grain_ms.1) / 1000.0);
        }
        *s = rng.bi() * e;
        e *= k;
    }
    out
}

/// White noise shaped by an envelope function of time.
pub fn noise(rng: &mut Rng, len: usize, env: impl Fn(f32) -> f32) -> Vec<f32> {
    (0..len).map(|i| rng.bi() * env(i as f32 / RATE)).collect()
}

/// Attack/decay envelope: linear rise over `attack` then exponential decay.
pub fn ad(t: f32, attack: f32, tau: f32) -> f32 {
    if t < attack { t / attack } else { (-(t - attack) / tau).exp() }
}

pub fn mix_into(dst: &mut [f32], src: &[f32], gain: f32, offset: usize) {
    for (d, s) in dst.iter_mut().skip(offset).zip(src) {
        *d += s * gain;
    }
}

/// Final clean-up shared by every one-shot: DC blocking, trimming trailing
/// silence, short fades at both ends (no clicks at buffer boundaries) and
/// normalisation to `peak`.
pub fn finish(mut buf: Vec<f32>, peak: f32) -> Vec<f32> {
    // DC blocker (~20 Hz high-pass).
    let r = 1.0 - TAU * 20.0 / RATE;
    let (mut x1, mut y1) = (0.0f32, 0.0f32);
    for s in buf.iter_mut() {
        let y = *s - x1 + r * y1;
        x1 = *s;
        y1 = y;
        *s = y;
    }
    let max = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if max <= 0.0 || !max.is_finite() {
        return vec![0.0; samples(0.01)];
    }
    // Trim the tail once it falls ~66 dB below the peak.
    let floor = max * 5e-4;
    let end = buf.iter().rposition(|s| s.abs() > floor).map_or(buf.len(), |i| i + 1);
    buf.truncate((end + samples(0.004)).min(buf.len()));
    fade(&mut buf, samples(0.0015), samples(0.012));
    let max = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let g = peak / max.max(1e-9);
    for s in buf.iter_mut() {
        *s *= g;
    }
    buf
}

/// Raised-cosine fade in over `fade_in` and out over `fade_out` samples.
pub fn fade(buf: &mut [f32], fade_in: usize, fade_out: usize) {
    let n = buf.len();
    for (i, s) in buf.iter_mut().take(fade_in).enumerate() {
        *s *= 0.5 - 0.5 * (PI * i as f32 / fade_in as f32).cos();
    }
    for i in 0..fade_out.min(n) {
        buf[n - 1 - i] *= 0.5 - 0.5 * (PI * i as f32 / fade_out as f32).cos();
    }
}

/// Turns `buf` (which must be `loop_len + xfade` long) into a seamless loop
/// of `loop_len` samples by equal-power crossfading its tail over its head,
/// then removes DC and normalises the RMS to `rms`.
pub fn make_loop(buf: Vec<f32>, loop_len: usize, xfade: usize, rms: f32) -> Vec<f32> {
    let mut out = buf[..loop_len].to_vec();
    for i in 0..xfade {
        let t = i as f32 / xfade as f32 * PI / 2.0;
        out[i] = buf[i] * t.sin() + buf[loop_len + i] * t.cos();
    }
    let mean = out.iter().sum::<f32>() / out.len() as f32;
    let cur = (out.iter().map(|s| (s - mean) * (s - mean)).sum::<f32>() / out.len() as f32).sqrt();
    let g = rms / cur.max(1e-9);
    for s in out.iter_mut() {
        *s = ((*s - mean) * g).clamp(-1.0, 1.0);
    }
    out
}
