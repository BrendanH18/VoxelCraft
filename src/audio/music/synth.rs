//! Sparse diatonic scores with nearest-note voice leading and recurring
//! motifs. Wavetable oscillators avoid transcendental work per sample.

use std::f32::consts::TAU;

use super::{RATE, Situation};
use crate::world::structure::Rng;

const TABLE: usize = 2048;
const VOICES: usize = 24;

#[derive(Clone, Copy)]
struct Note {
    at: usize,
    midi: i32,
    length: f32,
    gain: f32,
    pan: f32,
    pad: bool,
}

#[derive(Clone, Copy, Default)]
struct Voice {
    phase: [f32; 3],
    step: [f32; 3],
    env: f32,
    decay: f32,
    age: usize,
    duration: usize,
    attack: usize,
    release: usize,
    gain: [f32; 2],
    pad: bool,
    active: bool,
}

impl Voice {
    fn new(n: Note) -> Self {
        let hz = 440.0 * 2.0f32.powf((n.midi as f32 - 69.0) / 12.0);
        let pan = n.pan.clamp(-0.6, 0.6);
        Self {
            phase: [0.0; 3],
            step: [hz, hz * 2.002, hz * 3.006].map(|f| f * TABLE as f32 / RATE as f32),
            env: 1.0,
            decay: if n.pad { 1.0 } else { (-1.0 / (2.2 * RATE as f32)).exp() },
            age: 0,
            duration: (n.length * RATE as f32) as usize,
            attack: ((if n.pad { 2.5 } else { 0.025 }) * RATE as f32) as usize,
            release: ((if n.pad { 3.0 } else { 0.8 }) * RATE as f32) as usize,
            gain: [(0.5 * (1.0 - pan)).sqrt() * n.gain, (0.5 * (1.0 + pan)).sqrt() * n.gain],
            pad: n.pad,
            active: true,
        }
    }

    fn sample(&mut self, table: &[f32; TABLE + 1]) -> [f32; 2] {
        let mut tone = 0.0;
        for i in 0..3 {
            let p = self.phase[i];
            let idx = p as usize;
            let s = table[idx] + (table[idx + 1] - table[idx]) * (p - idx as f32);
            let brightness = if self.pad { 1.0 } else { self.env };
            tone += s * if i == 0 {
                1.0
            } else if i == 1 {
                0.18 * brightness
            } else {
                0.045 * brightness * brightness
            };
            self.phase[i] += self.step[i];
            if self.phase[i] >= TABLE as f32 {
                self.phase[i] -= TABLE as f32;
            }
        }
        // Smooth cubic onset/offset: both the level and its derivative are
        // zero at the boundaries, including voice retirement.
        let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
        let a = smooth((self.age as f32 / self.attack as f32).min(1.0));
        let r = smooth((self.duration.saturating_sub(self.age) as f32 / self.release as f32).min(1.0));
        let breath = if self.pad { 0.9 + 0.1 * table[(self.age / 173) % TABLE] } else { 1.0 };
        let s = tone * self.env * a * r * breath;
        self.env *= self.decay;
        self.age += 1;
        self.active = self.age < self.duration;
        self.gain.map(|g| s * g)
    }
}

struct Comb {
    buf: Vec<f32>,
    pos: usize,
    damp: f32,
}

impl Comb {
    fn new(length: usize) -> Self {
        Self { buf: vec![0.0; length], pos: 0, damp: 0.0 }
    }
    fn sample(&mut self, x: f32) -> f32 {
        let y = self.buf[self.pos];
        self.damp += 0.22 * (y - self.damp);
        self.buf[self.pos] = x + self.damp * 0.79;
        self.pos += 1;
        if self.pos == self.buf.len() {
            self.pos = 0;
        }
        y
    }
}

struct Allpass {
    buf: Vec<f32>,
    pos: usize,
}
impl Allpass {
    fn new(length: usize) -> Self {
        Self { buf: vec![0.0; length], pos: 0 }
    }
    fn sample(&mut self, x: f32) -> f32 {
        let delayed = self.buf[self.pos];
        let y = delayed - x * 0.5;
        self.buf[self.pos] = x + y * 0.5;
        self.pos += 1;
        if self.pos == self.buf.len() {
            self.pos = 0;
        }
        y
    }
}

/// One finite composition. Only construction allocates; `render` has fixed
/// polyphony and bounded DSP cost. The same seed/variant gives identical PCM
/// regardless of block sizes. Variants are three different harmonic forms.
pub struct Composition {
    notes: Vec<Note>,
    next: usize,
    frame: usize,
    length: usize,
    voices: [Voice; VOICES],
    table: Box<[f32; TABLE + 1]>,
    combs: [Comb; 8],
    diffuse: [Allpass; 4],
    dc_x: [f32; 2],
    dc_y: [f32; 2],
}

impl Composition {
    pub fn new(situation: Situation, seed: u64, variant: u8) -> Self {
        let mut rng = Rng(seed ^ ((situation as u64 + 1) * 0x0101_0101) ^ u64::from(variant));
        let dark = matches!(
            situation,
            Situation::NetherWastes
                | Situation::CrimsonForest
                | Situation::WarpedForest
                | Situation::SoulSandValley
                | Situation::BasaltDeltas
                | Situation::End
                | Situation::Dragon
        );
        let scale = if dark { [0, 2, 3, 5, 7, 8, 10] } else { [0, 2, 4, 5, 7, 9, 11] };
        let root = [48, 50, 53, 55, 57][rng.below(5) as usize] - if dark { 12 } else { 0 };
        let (tempo, density, piano_gain, pad_gain) = match situation {
            Situation::Underwater => ((46, 54), 0.52, 0.70, 1.25),
            Situation::End | Situation::WarpedForest => ((44, 52), 0.45, 0.65, 1.20),
            Situation::Dragon => ((62, 70), 0.80, 0.90, 1.05),
            Situation::Creative | Situation::Credits => ((52, 64), 0.88, 1.0, 0.95),
            Situation::NetherWastes
            | Situation::CrimsonForest
            | Situation::SoulSandValley
            | Situation::BasaltDeltas => ((46, 56), 0.60, 0.75, 1.15),
            _ => ((48, 62), 0.75, 1.0, 1.0),
        };
        let beat = 60.0 / rng.range(tempo.0, tempo.1) as f32;
        let bars = if situation == Situation::Credits { 28 } else { rng.range(14, 20) };
        let forms = [[0, 5, 3, 4, 0, 2, 3, 0], [0, 3, 5, 1, 4, 3, 4, 0], [5, 3, 0, 4, 2, 5, 3, 0]];
        let form = forms[variant as usize % forms.len()];
        let motif = [0, rng.range(1, 2) as i32, rng.range(2, 4) as i32, 1];
        let mut notes = Vec::with_capacity(bars as usize * 10);
        let mut previous = [root + 12, root + 16, root + 19];
        let mut melody = root + 24;
        let degree_note = |d: i32| root + d.div_euclid(7) * 12 + scale[d.rem_euclid(7) as usize];
        for bar in 0..bars {
            let degree = form[(bar as usize) % form.len()];
            let at = 2.0 + bar as f32 * 8.0 * beat;
            // A, B, A: middle section changes inversion/register and thins
            // the texture; the motif returns with a varied final cadence.
            let middle = bar >= bars / 3 && bar < bars * 2 / 3;
            let dynamic = (0.75 + rng.unit() as f32 * 0.25) * if middle { 0.8 } else { 1.0 };
            // Choose the complete inversion at once. Independent nearest-note
            // choices can walk the lower voices upwards until no uncrossed
            // top note remains; scoring whole voicings keeps a stable range.
            let chord = [degree_note(degree), degree_note(degree + 2), degree_note(degree + 4)];
            let mut voicing = previous;
            let mut best_cost = i32::MAX;
            for a in (0..=4).map(|oct| chord[0] + oct * 12).filter(|n| (root + 8..=root + 40).contains(n)) {
                for b in (0..=4).map(|oct| chord[1] + oct * 12).filter(|n| *n > a && *n <= root + 40) {
                    for c in (0..=4).map(|oct| chord[2] + oct * 12).filter(|n| *n > b && *n <= root + 40) {
                        let cost = (a - previous[0]).abs() + (b - previous[1]).abs() + (c - previous[2]).abs();
                        if cost < best_cost {
                            best_cost = cost;
                            voicing = [a, b, c];
                        }
                    }
                }
            }
            debug_assert_ne!(best_cost, i32::MAX, "a pad voicing fits in the register");
            previous = voicing;
            for (i, midi) in previous.into_iter().enumerate() {
                notes.push(Note {
                    at: (at * RATE as f32) as usize,
                    midi,
                    length: 8.0 * beat + 3.0,
                    gain: if dark { 0.018 } else { 0.014 } * dynamic * pad_gain,
                    pan: (i as f32 - 1.0) * 0.45,
                    pad: true,
                });
            }
            for (pulse, motif_note) in motif.iter().enumerate() {
                if rng.unit() > (density - if middle { 0.20 } else { 0.0 }) {
                    continue;
                }
                let target = degree_note(degree + motif_note + if middle { 2 } else { 0 });
                let midi = (1..=4)
                    .map(|oct| target + oct * 12)
                    .filter(|&n| (root + 19..=root + 38).contains(&n))
                    .min_by_key(|&n| (n - melody).abs())
                    .unwrap_or(target + 24);
                melody = midi;
                let timing = at + pulse as f32 * 2.0 * beat + rng.unit() as f32 * 0.10;
                notes.push(Note {
                    at: (timing * RATE as f32) as usize,
                    midi,
                    length: rng.range(4, 6) as f32,
                    gain: (0.055 + rng.unit() as f32 * 0.025) * dynamic * piano_gain,
                    pan: rng.unit() as f32 * 0.6 - 0.3,
                    pad: false,
                });
                // An occasional quiet answer below the melody.
                if pulse == 2 && rng.unit() < 0.35 {
                    notes.push(Note {
                        at: ((timing + beat) * RATE as f32) as usize,
                        midi: previous[1],
                        length: 5.0,
                        gain: 0.032 * dynamic * piano_gain,
                        pan: -0.25,
                        pad: false,
                    });
                }
            }
            if bar % 2 == 0 {
                notes.push(Note {
                    at: ((at + 0.14) * RATE as f32) as usize,
                    midi: degree_note(degree),
                    length: 7.0,
                    gain: 0.040 * dynamic * piano_gain,
                    pan: 0.0,
                    pad: false,
                });
            }
        }
        notes.sort_by_key(|n| n.at);
        let length = ((2.0 + bars as f32 * 8.0 * beat + 10.0) * RATE as f32) as usize;
        let mut table = Box::new([0.0; TABLE + 1]);
        for (i, s) in table.iter_mut().enumerate() {
            *s = (TAU * i as f32 / TABLE as f32).sin();
        }
        Self {
            notes,
            next: 0,
            frame: 0,
            length,
            voices: [Voice::default(); VOICES],
            table,
            combs: [713, 809, 887, 953, 739, 827, 907, 977].map(Comb::new),
            diffuse: [127, 41, 139, 47].map(Allpass::new),
            dc_x: [0.0; 2],
            dc_y: [0.0; 2],
        }
    }

    pub fn frames(&self) -> usize {
        self.length
    }
    pub fn finished(&self) -> bool {
        self.frame >= self.length
    }

    pub fn render(&mut self, out: &mut [[f32; 2]]) {
        for frame in out {
            *frame = [0.0; 2];
            if self.finished() {
                continue;
            }
            while self.next < self.notes.len() && self.notes[self.next].at <= self.frame {
                let note = self.notes[self.next];
                // Scores guarantee <24 simultaneous voices. Never steal an
                // active note (which would introduce a discontinuity).
                if let Some(v) = self.voices.iter_mut().find(|v| !v.active) {
                    *v = Voice::new(note);
                }
                self.next += 1;
            }
            for voice in self.voices.iter_mut().filter(|v| v.active) {
                let s = voice.sample(&self.table);
                for ch in 0..2 {
                    frame[ch] += s[ch];
                }
            }
            let send = (frame[0] + frame[1]) * 0.5;
            let fade_in = (self.frame as f32 / (3 * RATE) as f32).min(1.0);
            let fade_out = ((self.length - self.frame - 1) as f32 / (8 * RATE) as f32).min(1.0);
            for (ch, sample) in frame.iter_mut().enumerate() {
                let mut wet = 0.0;
                for comb in &mut self.combs[ch * 4..ch * 4 + 4] {
                    wet += comb.sample(send);
                }
                for ap in &mut self.diffuse[ch * 2..ch * 2 + 2] {
                    wet = ap.sample(wet);
                }
                let x = *sample + wet * 0.12;
                let y = x - self.dc_x[ch] + 0.998 * self.dc_y[ch];
                self.dc_x[ch] = x;
                self.dc_y[ch] = y;
                *sample = y * fade_in * fade_out;
                debug_assert!(sample.abs() < 0.8, "music score exceeded its headroom");
            }
            self.frame += 1;
        }
    }
}
