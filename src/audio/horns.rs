//! v1.0 final Track F4: original procedural goat horn calls
//! (`item.goat_horn.sound.0` to `.7`, one per Java 26.3 instrument) and
//! brushing (`item.brush.brushing.*`). Nothing is sampled. A horn is the
//! shared source-filter voice ([`super::voices::utter`]) in an open, brassy
//! vowel that brightens as it swells, with its own melodic contour, over a
//! breathy edge and a long echo like a call across a valley.

use super::dsp::{self, Biquad, Rng, crackle, mix_into, noise, samples};
use super::voices::{Utterance, lerp, swell, utter};

/// Java's eight instruments (`Instruments`), in their registry order: the
/// regular horns first, the screaming goats' last.
pub const INSTRUMENTS: [&str; 8] = ["ponder", "sing", "seek", "feel", "admire", "call", "yearn", "dream"];

/// One sustained horn note from `from` to `to` seconds of a call.
struct Blast {
    from: f32,
    to: f32,
    f0: fn(f32) -> f32,
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// The eight calls: each is one or two blasts with its own pitch line.
fn blasts(kind: u8) -> &'static [Blast] {
    match kind {
        // Ponder: a low, steady call that sinks a minor third at its end.
        0 => &[Blast { from: 0.0, to: 4.2, f0: |t| 196.0 * lerp(1.0, 0.84, smooth((t - 3.0) / 1.0)) }],
        // Sing: two held notes a major third apart, slurred.
        1 => &[Blast { from: 0.0, to: 3.8, f0: |t| 247.0 * lerp(1.0, 1.26, smooth((t - 1.3) / 0.25)) }],
        // Seek: a long rising glide, as if asking.
        2 => &[Blast { from: 0.0, to: 3.6, f0: |t| 185.0 * lerp(1.0, 1.5, smooth(t / 2.6)) }],
        // Feel: a slow fall with a wide, mournful waver.
        3 => &[Blast {
            from: 0.0,
            to: 4.4,
            f0: |t| 277.0 * lerp(1.0, 0.8, smooth(t / 4.0)) * (1.0 + 0.025 * (std::f32::consts::TAU * 4.5 * t).sin()),
        }],
        // Admire: a bright stab that leaps up and holds.
        4 => &[Blast { from: 0.0, to: 3.4, f0: |t| 330.0 * lerp(0.75, 1.0, smooth(t / 0.18)) }],
        // Call: a short blast, then a long one that rises a fourth.
        5 => &[
            Blast { from: 0.0, to: 0.9, f0: |_| 262.0 },
            Blast { from: 1.15, to: 4.2, f0: |t| 262.0 * lerp(1.0, 1.335, smooth((t - 1.9) / 0.4)) },
        ],
        // Yearn: a long, wandering note that never settles.
        6 => &[Blast {
            from: 0.0,
            to: 4.6,
            f0: |t| {
                233.0
                    * (1.0
                        + 0.06 * (std::f32::consts::TAU * 0.45 * t).sin()
                        + 0.02 * (std::f32::consts::TAU * 1.7 * t).sin())
            },
        }],
        // Dream: high and airy, drifting down.
        _ => &[Blast { from: 0.0, to: 4.0, f0: |t| 415.0 * lerp(1.0, 0.89, smooth(t / 3.6)) }],
    }
}

pub fn render_horn(kind: u8, rng: &mut Rng) -> Vec<f32> {
    let kind = kind.min(7);
    let calls = blasts(kind);
    let end = calls.iter().fold(0.0f32, |e, b| e.max(b.to));
    let tail = 2.2;
    let mut dry = vec![0.0; samples(end + 0.05)];
    let screaming = kind >= 4;
    for b in calls {
        let secs = b.to - b.from;
        let f0 = b.f0;
        let from = b.from;
        let vibrato = rng.range(4.5, 5.5);
        let tone = utter(
            rng,
            &Utterance {
                secs,
                f0: &|t| f0(t + from) * (1.0 + 0.006 * (std::f32::consts::TAU * vibrato * t).sin()),
                env: &|t| swell(t, 0.25, 0.6, secs),
                // An open "aw" that opens further (brighter) as it swells.
                formants: [(560.0, 2.0, 1.0), (1150.0, 2.5, 0.75), (2500.0, 3.5, if screaming { 0.55 } else { 0.35 })],
                shift: &|t| lerp(0.75, 1.0, smooth(t / 0.5)),
                jitter: 0.004,
                breath: if kind == 7 { 0.55 } else { 0.18 },
            },
        );
        mix_into(&mut dry, &tone, 1.0, samples(from));
    }
    // The player's breath at the mouthpiece.
    let mut air = noise(rng, dry.len(), |t| swell(t, 0.1, 0.5, end) * 0.06);
    Biquad::bandpass(1800.0, 0.8).run(&mut air);
    mix_into(&mut dry, &air, 1.0, 0);
    // A valley's echo: a few darkening repeats far behind the call.
    let mut out = vec![0.0; dry.len() + samples(tail)];
    mix_into(&mut out, &dry, 1.0, 0);
    let mut echo = dry.clone();
    for (delay, gain) in [(0.23, 0.42), (0.41, 0.3), (0.67, 0.2), (0.97, 0.12)] {
        Biquad::lowpass(2600.0, 0.7).run(&mut echo);
        mix_into(&mut out, &echo, gain, samples(delay));
    }
    dsp::finish(out, 0.7)
}

/// The brushing events (`BrushItem.onUseTick`: the block's
/// `BrushableBlock.getBrushSound`, else `BRUSH_GENERIC`; level event 3008
/// plays `getBrushCompletedSound`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum BrushCue {
    Generic,
    Sand,
    Gravel,
    SandComplete,
    GravelComplete,
}

impl BrushCue {
    pub const ALL: [BrushCue; 5] =
        [BrushCue::Generic, BrushCue::Sand, BrushCue::Gravel, BrushCue::SandComplete, BrushCue::GravelComplete];

    pub fn event(self) -> &'static str {
        match self {
            BrushCue::Generic => "item.brush.brushing.generic",
            BrushCue::Sand => "item.brush.brushing.sand",
            BrushCue::Gravel => "item.brush.brushing.gravel",
            BrushCue::SandComplete => "item.brush.brushing.sand.complete",
            BrushCue::GravelComplete => "item.brush.brushing.gravel.complete",
        }
    }
}

pub fn render_brush(cue: BrushCue, rng: &mut Rng) -> Vec<f32> {
    // One stroke: bristles swept across the face, there and back.
    let stroke = |rng: &mut Rng, secs: f32, grit: f32, bright: f32| {
        let mut sweep = noise(rng, samples(secs), |t| {
            let x = t / secs;
            (std::f32::consts::PI * x).sin().powi(2) * 0.5
        });
        Biquad::bandpass(bright, 0.7).run(&mut sweep);
        let mut grains = crackle(rng, samples(secs), (0.3, 1.2), |t| grit * (std::f32::consts::PI * t / secs).sin());
        Biquad::highpass(bright * 0.6, 0.7).run(&mut grains);
        mix_into(&mut sweep, &grains, 0.6, 0);
        sweep
    };
    match cue {
        BrushCue::Generic => dsp::finish(stroke(rng, 0.4, 300.0, 5200.0), 0.35),
        BrushCue::Sand => dsp::finish(stroke(rng, 0.45, 2600.0, 4200.0), 0.4),
        BrushCue::Gravel => dsp::finish(stroke(rng, 0.45, 1200.0, 2600.0), 0.42),
        BrushCue::SandComplete | BrushCue::GravelComplete => {
            // The last grains give way and pour off the find.
            let sand = cue == BrushCue::SandComplete;
            let secs = 0.8;
            let mut pour = crackle(rng, samples(secs), (0.5, if sand { 1.5 } else { 4.0 }), |t| {
                (if sand { 4500.0 } else { 1400.0 }) * dsp::ad(t, 0.02, 0.22)
            });
            Biquad::bandpass(if sand { 3600.0 } else { 1700.0 }, 0.8).run(&mut pour);
            let mut out = stroke(rng, 0.3, if sand { 2600.0 } else { 1200.0 }, if sand { 4200.0 } else { 2600.0 });
            out.resize(samples(secs + 0.2), 0.0);
            mix_into(&mut out, &pour, 0.9, samples(0.12));
            dsp::finish(out, 0.5)
        }
    }
}
