//! `--export-sounds`: writes every synthesized sound to a WAV file and prints
//! sample statistics, so the sound set can be inspected without a device.

use std::io::Write;
use std::path::Path;
use std::time::Instant;

use super::dsp::{Biquad, RATE};
use super::sounds::{Bank, Sound};

/// Statistics of one buffer.
#[derive(Debug, Clone, Copy)]
pub struct Stats {
    pub secs: f32,
    pub peak: f32,
    pub rms: f32,
    pub dc: f32,
    /// Largest |sample| among the first and last 8 samples (click check).
    pub edge: f32,
    /// Rough brightness: zero-crossing rate converted to Hz.
    pub zcr_hz: f32,
    /// Share of energy (%) below 250 Hz, 250-2k, 2k-6k and above 6 kHz,
    /// measured with a 4-band Butterworth filter bank.
    pub bands: [f32; 4],
}

fn band_energies(buf: &[f32]) -> [f32; 4] {
    const Q: f32 = std::f32::consts::FRAC_1_SQRT_2;
    let chains: [Vec<Biquad>; 4] = [
        vec![Biquad::lowpass(250.0, Q), Biquad::lowpass(250.0, Q)],
        vec![
            Biquad::highpass(250.0, Q),
            Biquad::highpass(250.0, Q),
            Biquad::lowpass(2000.0, Q),
            Biquad::lowpass(2000.0, Q),
        ],
        vec![
            Biquad::highpass(2000.0, Q),
            Biquad::highpass(2000.0, Q),
            Biquad::lowpass(6000.0, Q),
            Biquad::lowpass(6000.0, Q),
        ],
        vec![Biquad::highpass(6000.0, Q), Biquad::highpass(6000.0, Q)],
    ];
    let e = chains.map(|mut chain| {
        buf.iter()
            .map(|&x| {
                let y = chain.iter_mut().fold(x, |v, f| f.process(v));
                y * y
            })
            .sum::<f32>()
    });
    let total = e.iter().sum::<f32>().max(1e-12);
    e.map(|v| v / total * 100.0)
}

pub fn stats(buf: &[f32]) -> Stats {
    let n = buf.len().max(1) as f32;
    let peak = buf.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let rms = (buf.iter().map(|s| s * s).sum::<f32>() / n).sqrt();
    let dc = buf.iter().sum::<f32>() / n;
    let k = buf.len().min(8);
    let edge = buf[..k].iter().chain(&buf[buf.len() - k..]).fold(0.0f32, |m, s| m.max(s.abs()));
    let crossings = buf.windows(2).filter(|w| (w[0] >= 0.0) != (w[1] >= 0.0)).count() as f32;
    Stats {
        secs: buf.len() as f32 / RATE,
        peak,
        rms,
        dc,
        edge,
        zcr_hz: crossings / n * RATE / 2.0,
        bands: band_energies(buf),
    }
}

/// 16-bit mono PCM WAV.
pub fn wav_bytes(buf: &[f32]) -> Vec<u8> {
    let data_len = (buf.len() * 2) as u32;
    let rate = RATE as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in buf {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
    }
    out
}

pub fn export_sounds(dir: &str) -> std::io::Result<()> {
    let start = Instant::now();
    let bank = Bank::synthesize();
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    std::fs::create_dir_all(dir)?;
    let mut stdout = std::io::stdout().lock();
    writeln!(
        stdout,
        "{:<16} {:>3} {:>6} {:>6} {:>6} {:>8} {:>7} {:>7}  band energy % (<250 / 250-2k / 2k-6k / >6k)",
        "sound", "var", "secs", "peak", "rms", "dc", "edge", "zcr Hz"
    )?;
    for sound in Sound::all() {
        let (first, count) = bank.variants(sound);
        for v in 0..count {
            let buf = &bank.buffers[first + v];
            let loop_tag = if sound.is_loop() { "_loop" } else { "" };
            let path = Path::new(dir).join(format!("{}{loop_tag}_{v}.wav", sound.name()));
            std::fs::write(&path, wav_bytes(buf))?;
            let s = stats(buf);
            writeln!(
                stdout,
                "{:<16} {:>3} {:>6.3} {:>6.3} {:>6.3} {:>8.5} {:>7.4} {:>7.0}  {:>4.0} {:>4.0} {:>4.0} {:>4.0}",
                sound.name(),
                v,
                s.secs,
                s.peak,
                s.rms,
                s.dc,
                s.edge,
                s.zcr_hz,
                s.bands[0],
                s.bands[1],
                s.bands[2],
                s.bands[3]
            )?;
        }
    }
    writeln!(
        stdout,
        "synthesized {} buffers ({:.1} s of audio) in {ms:.1} ms; wrote WAVs to {dir}",
        bank.buffers.len(),
        bank.total_samples() as f32 / RATE
    )?;
    Ok(())
}

/// Offline full-length stereo music, with reproducible seeds and render CPU
/// timings. Stats use an interpolated 48 kHz channel for the existing filter
/// bank; the WAV preserves the native 24 kHz stereo samples.
pub fn export_music(dir: &str, seed: u64) -> std::io::Result<()> {
    use voxelcraft::music::{Composition, RATE as MUSIC_RATE, Situation};
    std::fs::create_dir_all(dir)?;
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "piece seed secs peak rms dc max_delta edge bands%(<250/250-2k/2k-6k/>6k) render_ms cpu%")?;
    for (index, situation) in [
        Situation::Menu,
        Situation::Game,
        Situation::Creative,
        Situation::Underwater,
        Situation::NetherWastes,
        Situation::CrimsonForest,
        Situation::End,
        Situation::Dragon,
        Situation::Credits,
    ]
    .into_iter()
    .enumerate()
    {
        let variant = index as u8 % 3;
        let piece_seed = seed.wrapping_add(index as u64);
        let start = Instant::now();
        let mut piece = Composition::new(situation, piece_seed, variant);
        let setup = start.elapsed();
        let mut frames = vec![[0.0; 2]; piece.frames()];
        let start = Instant::now();
        for block in frames.chunks_mut(512) {
            piece.render(block);
        }
        let render_ms = start.elapsed().as_secs_f64() * 1000.0;
        let secs = frames.len() as f64 / MUSIC_RATE as f64;
        let path = Path::new(dir).join(format!("{}_{variant}_seed{piece_seed}.wav", situation.name()));
        let data_len = (frames.len() * 4) as u32;
        let mut wav = Vec::with_capacity(44 + data_len as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&MUSIC_RATE.to_le_bytes());
        wav.extend_from_slice(&(MUSIC_RATE * 4).to_le_bytes());
        wav.extend_from_slice(&4u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        for frame in &frames {
            for s in frame {
                wav.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
            }
        }
        std::fs::write(path, wav)?;
        for ch in 0..2 {
            let mut channel = Vec::with_capacity(frames.len() * 2);
            for (i, frame) in frames.iter().enumerate() {
                channel.push(frame[ch]);
                channel.push((frame[ch] + frames.get(i + 1).map_or(0.0, |f| f[ch])) * 0.5);
            }
            let s = stats(&channel);
            let delta = frames.windows(2).map(|f| (f[0][ch] - f[1][ch]).abs()).fold(0.0f32, f32::max);
            writeln!(
                stdout,
                "{}_{variant}/{} {piece_seed} {secs:.1} {:.4} {:.4} {:.7} {delta:.5} {:.7} {:.1}/{:.1}/{:.1}/{:.1} {render_ms:.1} {:.3}",
                situation.name(),
                if ch == 0 { "L" } else { "R" },
                s.peak,
                s.rms,
                s.dc,
                s.edge,
                s.bands[0],
                s.bands[1],
                s.bands[2],
                s.bands[3],
                render_ms / (secs * 10.0)
            )?;
            if s.peak >= 0.5 || s.dc.abs() >= 1e-4 || delta >= 0.06 || s.edge >= 1e-4 || s.bands[3] >= 1.0 {
                return Err(std::io::Error::other(format!("music analysis failed: {} channel {ch}", situation.name())));
            }
        }
        writeln!(stdout, "setup {:.3} ms", setup.as_secs_f64() * 1000.0)?;
    }
    writeln!(stdout, "wrote native stereo music WAVs to {dir}")
}
