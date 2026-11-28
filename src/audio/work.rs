//! v1.0 gap Track 2h: original procedural stand-ins for the villagers' work
//! sounds (`VillagerProfession.workSound`, `entity.villager.work_*`, played
//! by `WorkAtPoi.start` as the villager's `NEUTRAL` sound) and the
//! composter's (`block.composter.fill`, `fill_success`, `empty` and
//! `ready`, `BLOCKS`). Nothing is sampled: each is a few damped modes and
//! filtered noise bursts in the style of the other workstation sounds.

use super::dsp::{self, Biquad, Mode, Rng, add_mode, crackle, mix_into, noise, samples};
use voxelcraft::entity::villager::Profession;
use voxelcraft::world::composter::Cue;

/// A short burst of noise through a band-pass at `freq`, `secs` long.
fn swish(rng: &mut Rng, secs: f32, freq: f32, q: f32, attack: f32) -> Vec<f32> {
    let mut burst = noise(rng, samples(secs), |t| dsp::ad(t, attack, secs * 0.35));
    Biquad::bandpass(freq, q).run(&mut burst);
    burst
}

/// A struck object: a damped fundamental and its overtone, with a tick.
fn knock(rng: &mut Rng, out: &mut [f32], at: f32, freq: f32, tau: f32, ring: f32) {
    let start = samples(at);
    add_mode(out, start, Mode { freq, amp: 0.7, tau, glide: 0.95, glide_tau: 0.02 });
    add_mode(out, start, Mode { freq: freq * ring, amp: 0.25, tau: tau * 0.6, glide: 1.0, glide_tau: 0.02 });
    let mut tick = noise(rng, samples(0.015), |t| (1.0 - t / 0.015).max(0.0));
    Biquad::highpass(2200.0, 0.7).run(&mut tick);
    mix_into(out, &tick, 0.3, start);
}

/// The composter's wet, crumbly squelch; `success` adds a layer's settling
/// crackle, `empty` is a long scoop and `ready` a soft rising bloop.
pub(super) fn composter(cue: Cue, rng: &mut Rng) -> Vec<f32> {
    let mut out = vec![0.0; samples(0.6)];
    match cue {
        Cue::Fill | Cue::FillSuccess => {
            let squelch = {
                let f = rng.range(380.0, 460.0);
                swish(rng, 0.22, f, 1.4, 0.01)
            };
            mix_into(&mut out, &squelch, 0.8, 0);
            let mut crumbs = crackle(rng, samples(0.2), (0.6, 1.6), |t| 300.0 * (1.0 - t / 0.2).max(0.0));
            Biquad::bandpass(1500.0, 1.2).run(&mut crumbs);
            mix_into(&mut out, &crumbs, 0.35, samples(0.02));
            if cue == Cue::FillSuccess {
                let mut settle = crackle(rng, samples(0.3), (1.0, 2.5), |t| 500.0 * (1.0 - t / 0.3).max(0.0));
                Biquad::bandpass(900.0, 1.0).run(&mut settle);
                mix_into(&mut out, &settle, 0.5, samples(0.08));
                add_mode(
                    &mut out,
                    samples(0.05),
                    Mode { freq: 140.0, amp: 0.4, tau: 0.08, glide: 0.8, glide_tau: 0.05 },
                );
            }
            dsp::finish(out, if cue == Cue::FillSuccess { 0.6 } else { 0.5 })
        }
        Cue::Empty => {
            let scoop = {
                let f = rng.range(600.0, 700.0);
                swish(rng, 0.45, f, 0.9, 0.06)
            };
            mix_into(&mut out, &scoop, 0.8, 0);
            let mut grit = crackle(rng, samples(0.4), (0.8, 2.0), |t| 400.0 * (1.0 - t / 0.4).max(0.0));
            Biquad::bandpass(2000.0, 1.0).run(&mut grit);
            mix_into(&mut out, &grit, 0.3, samples(0.03));
            dsp::finish(out, 0.5)
        }
        Cue::Ready => {
            add_mode(&mut out, 0, Mode { freq: 220.0, amp: 0.7, tau: 0.12, glide: 1.6, glide_tau: 0.06 });
            let squelch = swish(rng, 0.18, 500.0, 2.0, 0.01);
            mix_into(&mut out, &squelch, 0.5, samples(0.02));
            dsp::finish(out, 0.55)
        }
    }
}

/// The profession's workstation at work (`entity.villager.work_<job>`).
pub(super) fn work(job: Profession, rng: &mut Rng) -> Vec<f32> {
    use Profession as P;
    let mut out = vec![0.0; samples(0.9)];
    let peak = match job {
        // The blast furnace's roar with a few sparks.
        P::Armorer => {
            let mut roar = noise(rng, samples(0.7), |t| dsp::ad(t, 0.08, 0.3));
            Biquad::lowpass(500.0, 0.7).run(&mut roar);
            mix_into(&mut out, &roar, 0.9, 0);
            let mut sparks = crackle(rng, samples(0.6), (0.5, 1.2), |_| 120.0);
            Biquad::highpass(2500.0, 0.7).run(&mut sparks);
            mix_into(&mut out, &sparks, 0.3, samples(0.05));
            0.5
        }
        // The smoker's sizzle.
        P::Butcher => {
            let mut sizzle = crackle(rng, samples(0.7), (0.3, 0.9), |t| 900.0 * (1.0 - t / 0.7).max(0.0));
            Biquad::highpass(3000.0, 0.7).run(&mut sizzle);
            mix_into(&mut out, &sizzle, 0.8, 0);
            let mut smoke = noise(rng, samples(0.6), |t| dsp::ad(t, 0.1, 0.25));
            Biquad::bandpass(800.0, 0.8).run(&mut smoke);
            mix_into(&mut out, &smoke, 0.3, 0);
            0.45
        }
        // A pen scratching three quick strokes.
        P::Cartographer => {
            for k in 0..3 {
                let stroke = {
                    let f = rng.range(3800.0, 4600.0);
                    swish(rng, 0.12, f, 2.5, 0.02)
                };
                mix_into(&mut out, &stroke, 0.7, samples(0.05 + 0.16 * k as f32));
            }
            0.4
        }
        // The brewing stand's bubbling.
        P::Cleric => {
            for k in 0..5 {
                let at = samples(0.03 + 0.13 * k as f32 + rng.range(0.0, 0.04));
                let freq = rng.range(500.0, 900.0);
                add_mode(&mut out, at, Mode { freq, amp: 0.5, tau: 0.03, glide: 1.8, glide_tau: 0.02 });
            }
            0.45
        }
        // A spadeful of compost.
        P::Farmer => {
            let squelch = {
                let f = rng.range(380.0, 460.0);
                swish(rng, 0.25, f, 1.3, 0.01)
            };
            mix_into(&mut out, &squelch, 0.8, 0);
            let mut crumbs = crackle(rng, samples(0.25), (0.6, 1.6), |t| 350.0 * (1.0 - t / 0.25).max(0.0));
            Biquad::bandpass(1400.0, 1.2).run(&mut crumbs);
            mix_into(&mut out, &crumbs, 0.4, samples(0.03));
            0.5
        }
        // A barrel's lid: a wooden knock and a creak.
        P::Fisherman => {
            knock(rng, &mut out, 0.0, 210.0, 0.07, 2.4);
            let creak = swish(rng, 0.3, 1100.0, 6.0, 0.08);
            mix_into(&mut out, &creak, 0.4, samples(0.08));
            0.5
        }
        // The fletching table: two light wooden taps.
        P::Fletcher => {
            knock(rng, &mut out, 0.0, 620.0, 0.04, 2.9);
            knock(rng, &mut out, 0.14, 560.0, 0.04, 2.9);
            0.45
        }
        // Leather folded over a cauldron's rim: soft flaps.
        P::Leatherworker => {
            for k in 0..2 {
                let flap = {
                    let f = rng.range(700.0, 900.0);
                    swish(rng, 0.14, f, 0.8, 0.005)
                };
                mix_into(&mut out, &flap, 0.8, samples(0.18 * k as f32));
            }
            0.45
        }
        // A lectern's page turning.
        P::Librarian => {
            let page = {
                let f = rng.range(2600.0, 3200.0);
                swish(rng, 0.32, f, 0.9, 0.12)
            };
            mix_into(&mut out, &page, 0.8, 0);
            0.4
        }
        // The stonecutter's grind.
        P::Mason => {
            let mut grind = noise(rng, samples(0.6), |t| dsp::ad(t, 0.04, 0.3));
            Biquad::bandpass(1300.0, 1.5).run(&mut grind);
            mix_into(&mut out, &grind, 0.8, 0);
            let mut chips = crackle(rng, samples(0.5), (0.4, 1.0), |_| 200.0);
            Biquad::highpass(2800.0, 0.7).run(&mut chips);
            mix_into(&mut out, &chips, 0.35, samples(0.03));
            0.5
        }
        // The loom's shuttle: a run of clacks.
        P::Shepherd => {
            for k in 0..3 {
                let f = rng.range(430.0, 520.0);
                knock(rng, &mut out, 0.1 * k as f32, f, 0.035, 3.1);
            }
            0.45
        }
        // A hammer on the smithing table.
        P::Toolsmith => {
            for (k, freq) in [1250.0, 1180.0].into_iter().enumerate() {
                knock(rng, &mut out, 0.22 * k as f32, freq, 0.12, 2.76);
            }
            0.5
        }
        // The grindstone's rasp.
        P::Weaponsmith => {
            let mut rasp = noise(rng, samples(0.6), |t| dsp::ad(t, 0.05, 0.3) * (1.0 + 0.5 * (t * 40.0).sin()));
            Biquad::bandpass(2100.0, 1.8).run(&mut rasp);
            mix_into(&mut out, &rasp, 0.8, 0);
            0.45
        }
        P::None | P::Nitwit => 0.0,
    };
    if peak == 0.0 {
        return vec![0.0; samples(0.05)];
    }
    dsp::finish(out, peak)
}
