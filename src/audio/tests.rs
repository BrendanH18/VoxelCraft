use std::sync::{Arc, OnceLock};

use super::dsp::{self, RATE};
use super::export::{stats, wav_bytes};
use super::mixer::{Command, FAR, MAX_OTHERS, MAX_VOICES, Mixer, NEAR, attenuation, pan_gains};
use super::sounds::{Call, Voice};
use super::*;
use crate::world::gadgets::Instrument;

fn bank() -> Arc<Bank> {
    static BANK: OnceLock<Arc<Bank>> = OnceLock::new();
    BANK.get_or_init(|| Arc::new(Bank::synthesize())).clone()
}

#[test]
fn every_sound_renders_clean_buffers() {
    let bank = bank();
    for sound in Sound::all() {
        let (first, count) = bank.variants(sound);
        assert_eq!(count as u32, sound.variants(), "{}", sound.name());
        for v in 0..count {
            let buf = &bank.buffers[first + v];
            let name = format!("{}#{v}", sound.name());
            assert!(buf.len() > samples_ms(20), "{name} too short: {}", buf.len());
            assert!(buf.len() < samples_ms(13_000), "{name} too long");
            assert!(buf.iter().all(|s| s.is_finite() && s.abs() <= 1.0), "{name} out of range");
            let s = stats(buf);
            assert!(s.peak > 0.1 && s.peak <= 0.95, "{name} peak {}", s.peak);
            assert!(s.rms > 0.005, "{name} nearly silent");
            assert!(s.dc.abs() < 0.01, "{name} DC offset {}", s.dc);
            if sound.is_loop() {
                // Seamless: the wrap-around step is no bigger than typical steps.
                let jump = (buf[0] - buf[buf.len() - 1]).abs();
                let typical = buf.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
                assert!(jump <= typical, "{name} loop seam {jump} > {typical}");
            } else {
                // Faded at both ends: no clicks at buffer boundaries.
                assert!(s.edge < 0.01, "{name} edge {}", s.edge);
            }
        }
    }
}

fn samples_ms(ms: u32) -> usize {
    (ms as f32 / 1000.0 * RATE) as usize
}

#[test]
fn materials_have_distinct_character() {
    let bank = bank();
    let brightness = |s: Sound| {
        let (first, count) = bank.variants(s);
        (0..count).map(|v| stats(&bank.buffers[first + v]).zcr_hz).sum::<f32>() / count as f32
    };
    // Stone is crunchier/brighter than dirt; glass shatter is the brightest break.
    assert!(brightness(Sound::Break(Material::Stone)) > brightness(Sound::Break(Material::Dirt)) * 1.5);
    assert!(brightness(Sound::Break(Material::Glass)) > brightness(Sound::Break(Material::Stone)));
    // Landing thud is low.
    assert!(brightness(Sound::Land) < 500.0);
    // Steps are shorter than breaks.
    for m in Material::ALL {
        let len = |s: Sound| bank.buffers[bank.variants(s).0].len();
        assert!(len(Sound::Step(m)) < len(Sound::Break(m)), "{m:?}");
    }
}

#[test]
fn block_materials() {
    assert_eq!(material(Block::STONE), Material::Stone);
    assert_eq!(material(Block::DIAMOND_ORE), Material::Stone);
    assert_eq!(material(Block::PLANKS), Material::Wood);
    assert_eq!(material(Block::GRASS), Material::Grass);
    assert_eq!(material(Block::SNOWY_GRASS), Material::Snow);
    assert_eq!(material(Block::SPRUCE_LEAVES), Material::Leaves);
    assert_eq!(material(Block::GLASS), Material::Glass);
    assert_eq!(material(Block::flowing_water(3)), Material::Water);
    assert_eq!(material(Block::GRAVEL), Material::Gravel);
    assert_eq!(material(Block::SAND), Material::Sand);
    // Every sound key is unique and dense.
    let mut keys: Vec<usize> = Sound::all().map(Sound::key).collect();
    keys.sort();
    assert_eq!(keys, (0..Sound::COUNT).collect::<Vec<_>>());
}

#[test]
fn attenuation_curve() {
    assert_eq!(attenuation(0.0), 1.0);
    assert_eq!(attenuation(NEAR), 1.0);
    assert_eq!(attenuation(FAR), 0.0);
    assert_eq!(attenuation(FAR * 2.0), 0.0);
    let mut prev = 1.0;
    for i in 0..100 {
        let a = attenuation(NEAR + (FAR - NEAR) * i as f32 / 100.0);
        assert!(a <= prev && a >= 0.0);
        prev = a;
    }
}

#[test]
fn panning() {
    // Facing +X (yaw 0): right is +Z, as in `Player`.
    let (l, r) = pan_gains([0.0, 0.0, 5.0], 0.0);
    assert!(r > l * 2.0, "source on the right: {l} {r}");
    let (l, r) = pan_gains([0.0, 0.0, -5.0], 0.0);
    assert!(l > r * 2.0, "source on the left: {l} {r}");
    let (l, r) = pan_gains([5.0, 0.0, 0.0], 0.0);
    assert!((l - 1.0).abs() < 1e-5 && (r - 1.0).abs() < 1e-5, "straight ahead is centred");
    let (l, r) = pan_gains([0.0, 4.0, 0.0], 1.0);
    assert_eq!((l, r), (1.0, 1.0), "straight above is centred");
    // Turning 90° to the right (yaw +π/2 faces +Z) puts a +Z source ahead.
    let (l, r) = pan_gains([0.0, 0.0, 5.0], std::f32::consts::FRAC_PI_2);
    assert!((l - r).abs() < 1e-4);
    // Behind is slightly quieter than in front; power is otherwise constant.
    let front = pan_gains([5.0, 0.0, 0.0], 0.0);
    let back = pan_gains([-5.0, 0.0, 0.0], 0.0);
    assert!(back.0 < front.0);
    let (l, r) = pan_gains([3.0, 0.0, 4.0], 0.0);
    let behind = 0.9 + 0.1 * 0.6;
    assert!(((l * l + r * r) / (behind * behind) - 2.0).abs() < 1e-4);
}

fn test_mixer(buf: Vec<f32>) -> (Mixer, crossbeam_channel::Sender<Command>) {
    let (tx, rx) = crossbeam_channel::bounded(1024);
    (Mixer::new(Arc::new(Bank::uniform(buf)), rx, RATE, 1.0), tx)
}

fn play(gain: f32, pos: Option<[f32; 3]>) -> Command {
    play_in(SoundSource::Ui, gain, pos)
}

fn play_in(source: SoundSource, gain: f32, pos: Option<[f32; 3]>) -> Command {
    Command::Play { sound: Sound::Click, variant: 0, gain, pitch: 1.0, source, pos }
}

#[test]
fn mixer_plays_voice_unchanged() {
    let buf: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
    let (mut mixer, tx) = test_mixer(buf.clone());
    tx.send(play(0.8, None)).unwrap();
    let mut out = vec![0.0; 2 * 1200];
    mixer.render(&mut out, 2);
    for i in 0..999 {
        assert!((out[2 * i] - buf[i] * 0.8).abs() < 1e-5, "frame {i}");
        assert_eq!(out[2 * i], out[2 * i + 1]);
    }
    assert!(out[2 * 1100..].iter().all(|&s| s == 0.0), "voice ended");
    assert_eq!(mixer.active_voices(), 0);
}

#[test]
fn mixer_resamples_and_mixes_channels() {
    let buf = vec![0.25; 4800];
    // Device at 24 kHz: a 48 kHz buffer plays at double step, half the frames.
    let (tx, rx) = crossbeam_channel::bounded(16);
    let mut mixer = Mixer::new(Arc::new(Bank::uniform(buf)), rx, RATE / 2.0, 1.0);
    tx.send(play(1.0, None)).unwrap();
    tx.send(play(1.0, None)).unwrap();
    let mut out = vec![0.0; 3000];
    mixer.render(&mut out, 1); // mono
    assert!((out[10] - 0.5).abs() < 1e-5, "two voices sum: {}", out[10]);
    assert!(out[2390] > 0.4 && out[2410] == 0.0, "half as many frames at half the rate");
}

#[test]
fn mixer_limits_and_caps_voices() {
    let buf = vec![0.9; 48_000];
    let (mut mixer, tx) = test_mixer(buf);
    for _ in 0..100 {
        tx.send(play(1.0, None)).unwrap();
    }
    let mut out = vec![0.0; 2 * 4096];
    mixer.render(&mut out, 2);
    assert_eq!(mixer.active_voices(), MAX_VOICES);
    assert!(out.iter().all(|s| s.abs() <= 1.0 && s.is_finite()));
    assert!(out[out.len() - 1] > 0.8, "limited but still loud: {}", out[out.len() - 1]);
}

#[test]
fn mixer_master_and_distance() {
    let buf = vec![0.5; 48_000];
    let (mut mixer, tx) = test_mixer(buf);
    tx.send(Command::Listener { pos: [0.0; 3], yaw: 0.0 }).unwrap();
    // Out of earshot: never becomes a voice.
    tx.send(play(1.0, Some([FAR + 1.0, 0.0, 0.0]))).unwrap();
    let mut out = vec![0.0; 2 * 512];
    mixer.render(&mut out, 2);
    assert_eq!(mixer.active_voices(), 0);
    // To the right, 10 blocks away: attenuated and panned right.
    tx.send(play(1.0, Some([0.0, 0.0, 10.0]))).unwrap();
    mixer.render(&mut out, 2);
    let (l, r) = (out[1000], out[1001]);
    let (pl, pr) = pan_gains([0.0, 0.0, 10.0], 0.0);
    let a = attenuation(10.0) * 0.5;
    assert!((l - pl * a).abs() < 1e-4 && (r - pr * a).abs() < 1e-4, "{l} {r}");
    // Muting fades to silence within a few blocks.
    tx.send(Command::Volume(SoundSource::Master, 0.0)).unwrap();
    for _ in 0..20 {
        mixer.render(&mut out, 2);
    }
    assert!(out.iter().all(|s| s.abs() < 1e-3));
}

#[test]
fn split_screen_players_hear_nearby_sounds() {
    let buf = vec![0.5; 48_000];
    let (mut mixer, tx) = test_mixer(buf);
    tx.send(Command::Listener { pos: [0.0; 3], yaw: 0.0 }).unwrap();
    let mut ears = [[0.0; 4]; MAX_OTHERS];
    ears[0] = [500.0, 0.0, 0.0, 0.0];
    tx.send(Command::Others { count: 1, ears }).unwrap();
    // Far from the host but beside the other player: heard at their ear.
    tx.send(play(1.0, Some([500.0, 0.0, 2.0]))).unwrap();
    let mut out = vec![0.0; 2 * 512];
    mixer.render(&mut out, 2);
    assert_eq!(mixer.active_voices(), 1);
    let (pl, pr) = pan_gains([0.0, 0.0, 2.0], 0.0);
    let a = attenuation(2.0) * 0.5;
    assert!((out[1000] - pl * a).abs() < 1e-4 && (out[1001] - pr * a).abs() < 1e-4);
    // Once that player is gone, a new sound there is out of earshot.
    tx.send(Command::Others { count: 0, ears }).unwrap();
    tx.send(play(1.0, Some([500.0, 0.0, 2.0]))).unwrap();
    mixer.render(&mut out, 2);
    assert!(mixer.active_voices() <= 1);
}

#[test]
fn muffle_removes_highs() {
    // Alternating samples = Nyquist tone; the underwater low-pass kills it.
    let buf: Vec<f32> = (0..96_000).map(|i| if i % 2 == 0 { 0.3 } else { -0.3 }).collect();
    let (mut mixer, tx) = test_mixer(buf);
    tx.send(play(1.0, None)).unwrap();
    tx.send(Command::Muffle(1.0)).unwrap();
    let mut out = vec![0.0; 2 * 48_000];
    mixer.render(&mut out, 2);
    let tail = &out[out.len() - 2000..];
    assert!(tail.iter().all(|s| s.abs() < 0.01), "high tone muffled");
}

#[test]
fn partial_muffle_keeps_more_highs() {
    // One of two split-screen players underwater muffles the mix halfway.
    let tone = |muffle: f32| {
        let buf: Vec<f32> =
            (0..96_000).map(|i| 0.3 * (i as f32 * 2000.0 * std::f32::consts::TAU / 48_000.0).sin()).collect();
        let (mut mixer, tx) = test_mixer(buf);
        tx.send(play(1.0, None)).unwrap();
        tx.send(Command::Muffle(muffle)).unwrap();
        let mut out = vec![0.0; 2 * 48_000];
        mixer.render(&mut out, 2);
        let tail = &out[out.len() - 4000..];
        (tail.iter().map(|s| s * s).sum::<f32>() / tail.len() as f32).sqrt()
    };
    let (half, full) = (tone(0.5), tone(1.0));
    assert!(half > 2.0 * full, "half {half} vs full {full}");
}

#[test]
fn ambience_loops_fade_in() {
    let (tx, rx) = crossbeam_channel::bounded(16);
    let mut mixer = Mixer::new(bank(), rx, RATE, 1.0);
    let mut out = vec![0.0; 2 * 4800];
    mixer.render(&mut out, 2);
    assert!(out.iter().all(|&s| s == 0.0), "silent until requested");
    tx.send(Command::Ambience { wind: 1.0, cave: 0.0, rain: 0.0 }).unwrap();
    for _ in 0..20 {
        mixer.render(&mut out, 2);
    }
    let rms = (out.iter().map(|s| s * s).sum::<f32>() / out.len() as f32).sqrt();
    assert!(rms > 0.03, "wind audible after fading in: {rms}");
}

#[test]
fn wav_header() {
    let bytes = wav_bytes(&[0.0, 1.0, -1.0]);
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..16], b"WAVEfmt ");
    assert_eq!(bytes.len(), 44 + 6);
    assert_eq!(i16::from_le_bytes([bytes[46], bytes[47]]), 32767);
    assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), RATE as u32);
}

#[test]
fn fades_and_finish() {
    let mut rng = dsp::Rng::new(1);
    let buf = dsp::noise(&mut rng, 4800, |_| 1.0);
    let out = dsp::finish(buf, 0.5);
    assert!((stats(&out).peak - 0.5).abs() < 1e-5);
    assert!(out[0].abs() < 1e-6 && out[out.len() - 1].abs() < 1e-3);
}

#[test]
fn synthesis_is_fast() {
    let start = std::time::Instant::now();
    let bank = Bank::synthesize();
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    println!("synthesized {} buffers in {ms:.1} ms", bank.buffers.len());
    assert!(ms < 1000.0, "synthesis took {ms} ms");
}

#[test]
fn mixer_is_cheap() {
    // One second of 48 kHz stereo with every voice busy, positional, plus
    // both ambient loops and the underwater filter.
    let (tx, rx) = crossbeam_channel::bounded(1024);
    let mut mixer = Mixer::new(bank(), rx, RATE, 1.0);
    tx.send(Command::Ambience { wind: 1.0, cave: 1.0, rain: 1.0 }).unwrap();
    tx.send(Command::Muffle(1.0)).unwrap();
    let mut out = vec![0.0; 2 * 512];
    let start = std::time::Instant::now();
    for block in 0..94 {
        for i in 0..MAX_VOICES {
            let pos = Some([i as f32 - 16.0, 0.0, 3.0]);
            let _ = tx.try_send(Command::Play {
                sound: Sound::Break(Material::Glass),
                variant: i as u32,
                gain: 1.0,
                pitch: 1.1,
                source: SoundSource::Blocks,
                pos,
            });
        }
        tx.send(Command::Listener { pos: [0.0, 0.0, block as f32 * 0.01], yaw: 0.3 }).unwrap();
        mixer.render(&mut out, 2);
    }
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    println!("1 s of audio with {MAX_VOICES} voices mixed in {ms:.2} ms");
    assert!(ms < 250.0, "mixing too slow: {ms} ms per second of audio");
}

#[test]
fn every_sound_round_trips_through_its_key() {
    for sound in Sound::all() {
        assert_eq!(Sound::from_key(sound.key()), Some(sound));
    }
    assert_eq!(Sound::from_key(Sound::COUNT), None);
}

#[test]
fn every_sound_plays_in_a_java_category() {
    use SoundSource::*;
    for sound in Sound::all() {
        let source = sound.source();
        assert!(!matches!(source, Master | Music), "{}: Master and Music are not sound categories", sound.name());
    }
    // Where Java plays the matching sound event.
    let expect = [
        (Sound::Break(Material::Stone), Blocks),
        (Sound::Place(Material::Wood), Blocks),
        (Sound::Door(true), Blocks),
        (Sound::Bell, Blocks),
        (Sound::Explosion, Blocks),
        (Sound::Step(Material::Grass), Players),
        (Sound::Hurt, Players),
        (Sound::Pop, Players),
        (Sound::Orb, Players),
        (Sound::Note(Instrument::ALL[0]), Records),
        (Sound::Rain, Weather),
        (Sound::Wind, Ambient),
        (Sound::Cave, Ambient),
        (Sound::Click, Ui),
        (Sound::Fuse, Hostile),
        (Sound::Scream, Hostile),
        (Sound::DragonGrowl, Hostile),
        (Sound::PortalSpawn, Hostile),
        (Sound::EyeDeath, Neutral),
        (Sound::Mob(Voice::Zombie, Call::Hurt), Hostile),
        (Sound::Mob(Voice::Hoglin, Call::Ambient), Hostile),
        (Sound::Mob(Voice::Cow, Call::Death), Neutral),
        (Sound::Mob(Voice::Villager, Call::Ambient), Neutral),
        (Sound::Mob(Voice::Strider, Call::Ambient), Neutral),
    ];
    for (sound, source) in expect {
        assert_eq!(sound.source(), source, "{}", sound.name());
    }
    // Every category is used by some sound (or is Master/Music).
    for source in SoundSource::ALL.into_iter().skip(2) {
        assert!(Sound::all().any(|s| s.source() == source), "{source:?} has no sounds");
    }
}

#[test]
fn categories_have_java_labels_and_unique_option_keys() {
    let labels: Vec<_> = SoundSource::ALL.iter().map(|s| s.label()).collect();
    assert_eq!(
        labels,
        [
            "Master Volume",
            "Music",
            "Jukebox/Note Blocks",
            "Weather",
            "Blocks",
            "Hostile Mobs",
            "Friendly Mobs",
            "Players",
            "Ambient/Environment",
            "UI"
        ]
    );
    for (i, source) in SoundSource::ALL.into_iter().enumerate() {
        assert_eq!(source.index(), i);
        assert_eq!(SoundSource::from_option_key(source.option_key()), Some(source));
    }
    assert_eq!(SoundSource::from_option_key("volume"), Some(SoundSource::Master));
    assert_eq!(SoundSource::from_option_key("music_volume"), Some(SoundSource::Music));
    assert_eq!(SoundSource::from_option_key("fov"), None);
}

/// A mixer playing one long constant voice in `source`, settled at `volumes`.
fn settled(source: SoundSource, volumes: &[(SoundSource, f32)]) -> (Mixer, crossbeam_channel::Sender<Command>, f32) {
    let (mut mixer, tx) = test_mixer(vec![0.5; 4 * 48_000]);
    for &(s, v) in volumes {
        tx.send(Command::Volume(s, v)).unwrap();
    }
    // Let the glide to the new volumes finish before the voice starts.
    let mut out = vec![0.0; 2 * 4800];
    for _ in 0..4 {
        mixer.render(&mut out, 2);
    }
    tx.send(play_in(source, 0.8, None)).unwrap();
    for _ in 0..4 {
        mixer.render(&mut out, 2);
    }
    (mixer, tx, out[out.len() - 1])
}

#[test]
fn master_and_category_multiply() {
    let level = |volumes: &[(SoundSource, f32)]| settled(SoundSource::Players, volumes).2;
    assert!((level(&[]) - 0.4).abs() < 1e-3, "full volume plays the voice unchanged");
    assert!((level(&[(SoundSource::Master, 0.5)]) - 0.2).abs() < 1e-3);
    assert!((level(&[(SoundSource::Players, 0.5)]) - 0.2).abs() < 1e-3);
    assert!((level(&[(SoundSource::Master, 0.5), (SoundSource::Players, 0.5)]) - 0.1).abs() < 1e-3);
    assert!((level(&[(SoundSource::Master, 0.25), (SoundSource::Players, 0.8)]) - 0.08).abs() < 1e-3);
    // Other categories leave the voice alone.
    let others =
        [(SoundSource::Music, 0.0), (SoundSource::Blocks, 0.0), (SoundSource::Hostile, 0.1), (SoundSource::Ui, 0.0)];
    assert!((level(&others) - 0.4).abs() < 1e-3);
    assert!(level(&[(SoundSource::Players, 0.0)]).abs() < 1e-6);
}

#[test]
fn category_volume_changes_reach_playing_sounds() {
    let (mut mixer, tx, loud) = settled(SoundSource::Blocks, &[]);
    assert_eq!(mixer.active_voices(), 1);
    let mut out = vec![0.0; 2 * 4800];
    tx.send(Command::Volume(SoundSource::Blocks, 0.25)).unwrap();
    mixer.render(&mut out, 2);
    mixer.render(&mut out, 2);
    let quiet = out[out.len() - 1];
    assert!((quiet / loud - 0.25).abs() < 0.01, "{loud} -> {quiet}");
    assert_eq!(mixer.active_voices(), 1, "the same voice, no restart");
    // It glides rather than jumping: no step between samples bigger than a
    // small fraction of the change.
    let (mut mixer, tx, loud) = settled(SoundSource::Blocks, &[]);
    tx.send(Command::Volume(SoundSource::Blocks, 0.0)).unwrap();
    let mut out = vec![0.0; 2 * 4800];
    mixer.render(&mut out, 2);
    // Consecutive left-channel samples, starting from the level before the
    // change, so a jump on the first frame counts too.
    let left: Vec<f32> = std::iter::once(loud).chain(out.iter().step_by(2).copied()).collect();
    let step = left.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
    assert!(step < loud * 0.05, "volume change clicks: step {step} for a {loud} voice");
    mixer.render(&mut out, 2);
    mixer.render(&mut out, 2);
    assert!(out.iter().all(|s| s.abs() < 1e-3), "and ends in silence");
}

#[test]
fn a_silent_category_takes_no_voice() {
    let (mut mixer, tx) = test_mixer(vec![0.5; 48_000]);
    tx.send(Command::Volume(SoundSource::Hostile, 0.0)).unwrap();
    let mut out = vec![0.0; 2 * 4800];
    for _ in 0..3 {
        mixer.render(&mut out, 2);
    }
    tx.send(play_in(SoundSource::Hostile, 1.0, None)).unwrap();
    tx.send(play_in(SoundSource::Neutral, 1.0, None)).unwrap();
    mixer.render(&mut out, 2);
    assert_eq!(mixer.active_voices(), 1, "only the audible category plays");
    // Out-of-range and non-finite volumes are clamped or ignored.
    tx.send(Command::Volume(SoundSource::Neutral, f32::NAN)).unwrap();
    tx.send(Command::Volume(SoundSource::Players, 7.0)).unwrap();
    tx.send(Command::Volume(SoundSource::Ui, -3.0)).unwrap();
    mixer.render(&mut out, 2);
    assert!(out.iter().all(|s| s.is_finite()));
}

#[test]
fn ambience_loops_follow_their_own_categories() {
    let energy = |volumes: &[(SoundSource, f32)], wind: f32, rain: f32| {
        let (tx, rx) = crossbeam_channel::bounded(64);
        let mut mixer = Mixer::new(bank(), rx, RATE, 1.0);
        for &(s, v) in volumes {
            tx.send(Command::Volume(s, v)).unwrap();
        }
        tx.send(Command::Ambience { wind, cave: 0.0, rain }).unwrap();
        let mut out = vec![0.0; 2 * 4800];
        let mut total = 0.0;
        // Four seconds, past the loops' fade-in.
        for block in 0..40 {
            mixer.render(&mut out, 2);
            if block >= 30 {
                total += out.iter().map(|s| s * s).sum::<f32>();
            }
        }
        total
    };
    let rain = energy(&[], 0.0, 1.0);
    assert!(rain > 1.0, "rain is audible: {rain}");
    assert!(energy(&[(SoundSource::Weather, 0.0)], 0.0, 1.0) < rain * 1e-4, "Weather silences rain");
    assert!(energy(&[(SoundSource::Ambient, 0.0)], 0.0, 1.0) > rain * 0.99, "Ambient leaves rain alone");
    let wind = energy(&[], 1.0, 0.0);
    assert!(wind > 1.0, "wind is audible: {wind}");
    assert!(energy(&[(SoundSource::Ambient, 0.0)], 1.0, 0.0) < wind * 1e-4, "Ambient silences wind");
    assert!(energy(&[(SoundSource::Weather, 0.0)], 1.0, 0.0) > wind * 0.99, "Weather leaves wind alone");
    let half = energy(&[(SoundSource::Master, 0.5), (SoundSource::Weather, 0.5)], 0.0, 1.0);
    assert!((half / rain - 0.0625).abs() < 0.01, "master x weather: {}", half / rain);
}
