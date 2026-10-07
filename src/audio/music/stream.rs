//! Background renderer and a bounded single-producer/single-consumer ring.
//! Atomic sample storage avoids unsafe code. Callback reads never allocate,
//! wait, send messages, log or take locks; underruns decay to silence.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use super::{Action, Composition, Context, Manager, MusicInfo, RATE, Situation};
use crate::world::structure::Rng;

const CAPACITY: usize = 4096;
const BLOCK: usize = 512;

struct Shared {
    samples: Box<[AtomicU64]>,
    head: AtomicUsize,
    tail: AtomicUsize,
    running: AtomicBool,
    desired: AtomicU32,
    gain: AtomicU32,
}

impl Shared {
    fn new() -> Self {
        Self {
            samples: (0..CAPACITY).map(|_| AtomicU64::new(0)).collect(),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            running: AtomicBool::new(true),
            desired: AtomicU32::new(Situation::Menu as u32),
            gain: AtomicU32::new(1.0f32.to_bits()),
        }
    }
}

pub struct MusicStream {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl MusicStream {
    /// One stream per shared audio output, independent of the number of ears.
    pub fn new(seed: u64) -> (Self, MusicReader) {
        let shared = Arc::new(Shared::new());
        let render = shared.clone();
        let worker = std::thread::Builder::new().name("music".into()).spawn(move || produce(render, seed));
        let worker = match worker {
            Ok(worker) => Some(worker),
            Err(e) => {
                log::warn!("could not start music renderer: {e}");
                None
            }
        };
        let reader = MusicReader::new(shared.clone());
        (Self { shared, worker }, reader)
    }

    pub fn set_context(&self, context: Context) {
        self.set_situation(context.select(None));
    }
    pub fn set_situation(&self, situation: Situation) {
        self.shared.desired.store(situation as u32, Ordering::Relaxed);
    }

    /// MusicInfo's situational gain (e.g. a silent biome), separate from the
    /// user's Music category. Zero fades out and stops the current track.
    pub fn set_info(&self, info: MusicInfo) {
        self.set_gain(info.volume);
        self.shared.desired.store(info.music.map_or(255, |s| s as u32), Ordering::Release);
    }

    pub fn set_gain(&self, gain: f32) {
        self.shared.gain.store(gain.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }
}

impl Drop for MusicStream {
    fn drop(&mut self) {
        self.shared.running.store(false, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub struct MusicReader {
    shared: Arc<Shared>,
    tail: usize,
    head: usize,
    a: [f32; 2],
    b: [f32; 2],
    last: [f32; 2],
    blend: f32,
    phase: f64,
}

impl MusicReader {
    fn new(shared: Arc<Shared>) -> Self {
        Self { shared, tail: 0, head: 0, a: [0.0; 2], b: [0.0; 2], last: [0.0; 2], blend: 0.0, phase: 0.0 }
    }

    fn pop(&mut self) -> [f32; 2] {
        if self.tail == self.head {
            self.head = self.shared.head.load(Ordering::Acquire);
        }
        if self.tail == self.head {
            self.blend = 0.0;
            self.last = self.last.map(|s| s * 0.995);
        } else {
            let bits = self.shared.samples[self.tail % CAPACITY].load(Ordering::Relaxed);
            let sample = [f32::from_bits(bits as u32), f32::from_bits((bits >> 32) as u32)];
            self.tail = self.tail.wrapping_add(1);
            self.shared.tail.store(self.tail, Ordering::Release);
            // 10 ms recovery ramp also suppresses clicks after an underrun.
            self.blend = (self.blend + 1.0 / (0.01 * RATE as f32)).min(1.0);
            for (last, next) in self.last.iter_mut().zip(sample) {
                *last += (next - *last) * self.blend;
            }
        }
        self.last
    }

    /// Interpolates the 24 kHz stream to the device rate, in fixed storage.
    pub fn sample(&mut self, output_rate: f32) -> [f32; 2] {
        let t = self.phase as f32;
        let out = [self.a[0] + (self.b[0] - self.a[0]) * t, self.a[1] + (self.b[1] - self.a[1]) * t];
        self.phase += RATE as f64 / output_rate.max(8_000.0) as f64;
        while self.phase >= 1.0 {
            self.a = self.b;
            self.b = self.pop();
            self.phase -= 1.0;
        }
        out
    }
}

impl Drop for MusicReader {
    fn drop(&mut self) {
        // Device creation may fail. Stop rendering/waking when there is no
        // consumer, without making the callback responsible for joining.
        self.shared.running.store(false, Ordering::Relaxed);
    }
}

fn produce(shared: Arc<Shared>, seed: u64) {
    let mut manager = Manager::new(seed);
    let mut rng = Rng(seed ^ 0x004D_5553_4943);
    let mut piece: Option<Composition> = None;
    let mut tail_piece: Option<Composition> = None;
    let mut fade = 0usize;
    let mut head = 0usize;
    let mut tick = 0usize;
    let mut gain_start = 1.0f32;
    let mut block = [[0.0; 2]; BLOCK];
    let mut tail = [[0.0; 2]; BLOCK];
    let mut last_variant = u8::MAX;
    while shared.running.load(Ordering::Relaxed) {
        let occupied = head.wrapping_sub(shared.tail.load(Ordering::Acquire));
        let n = (CAPACITY - occupied).min(BLOCK).min(RATE as usize / 20 - tick);
        if n == 0 {
            std::thread::sleep(Duration::from_millis(3));
            continue;
        }
        if tick == 0 {
            let id = shared.desired.load(Ordering::Acquire) as u8;
            let mut desired = (id != 255).then(|| Situation::from_id(id));
            // Java retains the underwater selection until that track ends,
            // even after surfacing. Dimension and screen changes still win.
            if manager.current == Some(Situation::Underwater) && desired.is_some_and(Situation::overworld) {
                desired = Some(Situation::Underwater);
            }
            let target = f32::from_bits(shared.gain.load(Ordering::Relaxed));
            gain_start = manager.gain;
            let finished = manager.current.is_some() && piece.as_ref().is_none_or(Composition::finished);
            match manager.tick_info(MusicInfo { music: desired, volume: target }, finished) {
                Action::Stop => {
                    tail_piece = piece.take();
                    fade = RATE as usize;
                }
                Action::Start(s) | Action::Replace(s) => {
                    if piece.is_some() {
                        tail_piece = piece.take();
                        fade = RATE as usize;
                    }
                    let mut variant = rng.below(3) as u8;
                    if variant == last_variant {
                        variant = (variant + 1) % 3;
                    }
                    last_variant = variant;
                    piece = Some(Composition::new(s, rng.next_u64(), variant));
                }
                Action::None => {
                    if finished {
                        piece = None;
                    }
                }
            }
        }
        block[..n].fill([0.0; 2]);
        if let Some(piece) = &mut piece {
            piece.render(&mut block[..n]);
        }
        if let Some(piece) = &mut tail_piece {
            piece.render(&mut tail[..n]);
            for (out, t) in block[..n].iter_mut().zip(&tail[..n]) {
                let g = fade as f32 / RATE as f32;
                for ch in 0..2 {
                    out[ch] += t[ch] * g;
                }
                fade = fade.saturating_sub(1);
            }
            if fade == 0 {
                tail_piece = None;
            }
        }
        for (i, frame) in block[..n].iter().enumerate() {
            let t = (tick + i + 1) as f32 / (RATE / 20) as f32;
            let gain = gain_start + (manager.gain - gain_start) * t;
            let l = (frame[0] * gain).to_bits() as u64;
            let r = (frame[1] * gain).to_bits() as u64;
            shared.samples[head % CAPACITY].store(l | (r << 32), Ordering::Relaxed);
            head = head.wrapping_add(1);
        }
        shared.head.store(head, Ordering::Release);
        tick = (tick + n) % (RATE as usize / 20);
    }
}

/// Java 1.21.4+ MusicInfo fade: 3% approach downwards, clamped additive
/// increase upwards. Called at 20 Hz and interpolated over output samples.
pub(super) fn fade_gain(current: f32, target: f32) -> f32 {
    if current > target {
        let next = 0.03 * target + 0.97 * current;
        if (next - target).abs() < 1e-4 { target } else { next }
    } else if current < target {
        (current + current.clamp(0.0005, 0.005)).min(target)
    } else {
        current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_preserves_stereo_wrap_and_smooths_underrun() {
        let shared = Arc::new(Shared::new());
        let mut reader = MusicReader::new(shared.clone());
        for cycle in 0..3 {
            for i in 0..CAPACITY {
                shared.samples[i]
                    .store(0.1f32.to_bits() as u64 | ((-0.2f32).to_bits() as u64) << 32, Ordering::Relaxed);
            }
            shared.head.store((cycle + 1) * CAPACITY, Ordering::Release);
            for _ in 0..CAPACITY {
                reader.pop();
            }
            assert_eq!(reader.last, [0.1, -0.2]);
            assert_eq!(shared.tail.load(Ordering::Acquire), (cycle + 1) * CAPACITY);
        }
        let a = reader.pop();
        assert!((a[0] - 0.1).abs() < 0.001);
        for _ in 0..4000 {
            reader.pop();
        }
        assert!(reader.last[0].abs() < 1e-8);
    }

    #[test]
    fn fades_are_monotonic_and_reach_silence() {
        let mut g = 1.0;
        for _ in 0..400 {
            let next = fade_gain(g, 0.0);
            assert!(next <= g);
            g = next;
        }
        assert_eq!(g, 0.0);
        for _ in 0..400 {
            let next = fade_gain(g, 1.0);
            assert!(next >= g && next <= 1.0);
            g = next;
        }
        assert!(g > 0.8);
    }
}
