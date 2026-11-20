//! Original procedural stand-ins for v0.16 Track B's sound events: shulker
//! box lids, firework launches, blasts and twinkles, the elytra's equip
//! rustle and its glide wind. Nothing is sampled.

use super::SoundSource;
use super::dsp::{self, Biquad, Mode, Rng, add_mode, crackle, noise, samples};
use voxelcraft::content::fireworks::Cue;

/// `SoundSource` of each event as Java plays it: shulker boxes in `BLOCKS`,
/// rockets in `AMBIENT` (the dispenser's shot in `BLOCKS`), elytra in
/// `PLAYERS`.
pub(super) fn source(cue: Cue) -> SoundSource {
    match cue {
        Cue::ShulkerOpen | Cue::ShulkerClose | Cue::Shoot => SoundSource::Blocks,
        Cue::EquipElytra | Cue::ElytraFlying => SoundSource::Players,
        _ => SoundSource::Ambient,
    }
}

/// A short hollow knock with a gliding body: the lid lifting (`rise`) or
/// settling.
fn lid(rng: &mut Rng, rise: bool) -> Vec<f32> {
    let mut out = vec![0.0; samples(0.45)];
    let (from, to) = if rise { (190.0, 320.0) } else { (300.0, 170.0) };
    add_mode(
        &mut out,
        0,
        Mode { freq: to * rng.range(0.97, 1.03), amp: 0.6, tau: 0.09, glide: from / to, glide_tau: 0.12 },
    );
    add_mode(&mut out, samples(0.01), Mode { freq: to * 2.1, amp: 0.2, tau: 0.05, glide: 1.0, glide_tau: 0.05 });
    let mut scrape = noise(rng, out.len(), |t| 0.25 * dsp::ad(t, 0.02, 0.08));
    Biquad::bandpass(if rise { 900.0 } else { 600.0 }, 1.2).run(&mut scrape);
    dsp::mix_into(&mut out, &scrape, 1.0, 0);
    dsp::finish(out, 0.55)
}

/// A firework burst: a sharp crack over a boomy tail; `large` lower and
/// longer, `far` duller and later-sounding.
fn blast(rng: &mut Rng, large: bool, far: bool) -> Vec<f32> {
    let len = samples(if large { 2.2 } else { 1.4 });
    let tau = if large { 0.45 } else { 0.28 };
    let mut out = noise(rng, len, |t| dsp::ad(t, 0.002, tau));
    Biquad::lowpass(if far { 700.0 } else { 2600.0 }, 0.7).run(&mut out);
    let boom = if large { 55.0 } else { 80.0 };
    add_mode(&mut out, 0, Mode { freq: boom, amp: 0.9, tau: tau * 1.4, glide: 1.4, glide_tau: 0.05 });
    if !far {
        let mut crack = noise(rng, samples(0.05), |t| dsp::ad(t, 0.0005, 0.01));
        Biquad::highpass(2500.0, 0.8).run(&mut crack);
        dsp::mix_into(&mut out, &crack, 0.8, 0);
    }
    dsp::finish(out, if far { 0.5 } else { 0.85 })
}

pub(super) fn render(cue: Cue, rng: &mut Rng) -> Vec<f32> {
    match cue {
        Cue::ShulkerOpen => lid(rng, true),
        Cue::ShulkerClose => lid(rng, false),
        Cue::Launch | Cue::Shoot => {
            // A rising hiss as the fuse catches and the rocket leaves.
            let secs = if cue == Cue::Launch { 1.1 } else { 0.5 };
            let len = samples(secs);
            let mut out = noise(rng, len, |t| (t / 0.08).min(1.0) * (1.0 - t / secs).max(0.0).powf(1.5));
            let mut filter = Biquad::bandpass(800.0, 1.4);
            for (i, s) in out.iter_mut().enumerate() {
                if i % 64 == 0 {
                    let t = i as f32 / dsp::RATE;
                    filter.retune(Biquad::bandpass(800.0 + 3200.0 * (t / secs), 1.4));
                }
                *s = filter.process(*s);
            }
            dsp::finish(out, 0.6)
        }
        Cue::Blast => blast(rng, false, false),
        Cue::BlastFar => blast(rng, false, true),
        Cue::LargeBlast => blast(rng, true, false),
        Cue::LargeBlastFar => blast(rng, true, true),
        Cue::Twinkle | Cue::TwinkleFar => {
            // Sparse bright crackles fading out.
            let len = samples(1.6);
            let mut out = crackle(rng, len, (3.0, 12.0), |t| 90.0 * (1.0 - t / 1.6).max(0.0));
            Biquad::highpass(if cue == Cue::Twinkle { 1800.0 } else { 900.0 }, 0.7).run(&mut out);
            if cue == Cue::TwinkleFar {
                Biquad::lowpass(2500.0, 0.7).run(&mut out);
            }
            dsp::finish(out, if cue == Cue::Twinkle { 0.6 } else { 0.4 })
        }
        Cue::EquipElytra => {
            // A leathery flap: two quick filtered rustles.
            let mut out = vec![0.0; samples(0.45)];
            for (start, gain) in [(0.0, 1.0), (0.14, 0.7)] {
                let mut flap = noise(rng, samples(0.2), |t| dsp::ad(t, 0.01, 0.05));
                Biquad::bandpass(rng.range(700.0, 1100.0), 0.9).run(&mut flap);
                dsp::mix_into(&mut out, &flap, gain, samples(start));
            }
            dsp::finish(out, 0.5)
        }
        Cue::ElytraFlying => {
            // A stretch of rushing air, swelling and fading so stretches
            // overlap smoothly.
            let len = samples(0.9);
            let mut out = noise(rng, len, |t| (std::f32::consts::PI * t / 0.9).sin());
            Biquad::lowpass(1100.0, 0.6).run(&mut out);
            Biquad::highpass(120.0, 0.7).run(&mut out);
            dsp::finish(out, 0.5)
        }
    }
}
