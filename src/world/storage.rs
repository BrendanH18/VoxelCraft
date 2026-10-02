//! On-disk persistence for player-modified chunks.
//!
//! Only chunks the player changed are stored; everything else regenerates
//! from the seed. Chunks are run-length encoded into a single file.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use glam::{DVec3, IVec3};
use rustc_hash::FxHashMap;

use super::block::Block;
use super::chunk::{CHUNK_VOLUME, ChunkData};

const MAGIC: &[u8; 4] = b"VXC1";

pub struct LevelInfo {
    pub seed: u64,
    pub player: Option<(DVec3, f32, f32)>,
    /// Other `key=value` lines (game mode, inventory, ...).
    pub props: std::collections::BTreeMap<String, String>,
}

pub struct Storage {
    dir: PathBuf,
}

impl Storage {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn exists(&self) -> bool {
        self.dir.join("level.txt").exists()
    }

    pub fn load_level(&self) -> io::Result<LevelInfo> {
        let text = fs::read_to_string(self.dir.join("level.txt"))?;
        let mut info = LevelInfo { seed: 0, player: None, props: Default::default() };
        for line in text.lines() {
            match line.split_once('=') {
                Some(("seed", v)) => info.seed = v.trim().parse().map_err(|_| io::Error::other("bad seed"))?,
                Some(("player", v)) => {
                    let n: Vec<f64> = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                    if n.len() == 5 {
                        info.player = Some((DVec3::new(n[0], n[1], n[2]), n[3] as f32, n[4] as f32));
                    }
                }
                Some((k, v)) => {
                    info.props.insert(k.to_string(), v.to_string());
                }
                None => {}
            }
        }
        Ok(info)
    }

    pub fn save(&self, level: &LevelInfo, chunks: &[(IVec3, Arc<ChunkData>)]) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;

        let mut buf = Vec::with_capacity(chunks.len() * 256);
        buf.extend_from_slice(MAGIC);
        buf.extend_from_slice(&(chunks.len() as u32).to_le_bytes());
        for (pos, data) in chunks {
            for c in pos.to_array() {
                buf.extend_from_slice(&c.to_le_bytes());
            }
            let rle = encode(data);
            buf.extend_from_slice(&(rle.len() as u32).to_le_bytes());
            buf.extend_from_slice(&rle);
        }

        // Write to temp files then rename so a crash never leaves a torn save.
        let tmp = self.dir.join("chunks.bin.tmp");
        fs::File::create(&tmp)?.write_all(&buf)?;
        fs::rename(&tmp, self.dir.join("chunks.bin"))?;
        self.save_level(level)
    }

    /// Writes only `level.txt`, leaving the stored chunks alone.
    pub fn save_level(&self, level: &LevelInfo) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let mut text = format!("seed={}\n", level.seed);
        if let Some((p, yaw, pitch)) = level.player {
            text += &format!("player={},{},{},{},{}\n", p.x, p.y, p.z, yaw, pitch);
        }
        for (k, v) in &level.props {
            text += &format!("{k}={v}\n");
        }
        let tmp = self.dir.join("level.txt.tmp");
        fs::write(&tmp, text)?;
        fs::rename(&tmp, self.dir.join("level.txt"))
    }

    pub fn load_chunks(&self) -> io::Result<FxHashMap<IVec3, Arc<ChunkData>>> {
        let mut map = FxHashMap::default();
        let mut buf = Vec::new();
        match fs::File::open(self.dir.join("chunks.bin")) {
            Ok(mut f) => f.read_to_end(&mut buf)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(map),
            Err(e) => return Err(e),
        };
        let bad = || io::Error::new(io::ErrorKind::InvalidData, "corrupt chunks.bin");
        if buf.len() < 8 || &buf[0..4] != MAGIC {
            return Err(bad());
        }
        let mut at = 4;
        let u32_at = |at: &mut usize| -> io::Result<u32> {
            let b = buf.get(*at..*at + 4).ok_or_else(bad)?;
            *at += 4;
            Ok(u32::from_le_bytes(b.try_into().unwrap()))
        };
        let count = u32_at(&mut at)?;
        for _ in 0..count {
            let x = u32_at(&mut at)? as i32;
            let y = u32_at(&mut at)? as i32;
            let z = u32_at(&mut at)? as i32;
            let len = u32_at(&mut at)? as usize;
            let rle = buf.get(at..at + len).ok_or_else(bad)?;
            at += len;
            map.insert(IVec3::new(x, y, z), Arc::new(decode(rle).ok_or_else(bad)?));
        }
        Ok(map)
    }
}

/// Encodes as (run length u16 LE, block u8) triples.
fn encode(data: &ChunkData) -> Vec<u8> {
    let mut out = Vec::new();
    let mut run: Option<(Block, u16)> = None;
    data.for_each_block(|b| match &mut run {
        Some((cur, n)) if *cur == b && *n < u16::MAX => *n += 1,
        _ => {
            if let Some((cur, n)) = run {
                out.extend_from_slice(&n.to_le_bytes());
                out.push(cur.0);
            }
            run = Some((b, 1));
        }
    });
    if let Some((cur, n)) = run {
        out.extend_from_slice(&n.to_le_bytes());
        out.push(cur.0);
    }
    out
}

fn decode(rle: &[u8]) -> Option<ChunkData> {
    let mut blocks = ChunkData::new_dense(Block::AIR);
    let mut i = 0;
    for triple in rle.as_chunks::<3>().0 {
        let n = u16::from_le_bytes([triple[0], triple[1]]) as usize;
        blocks.get_mut(i..i + n)?.fill(Block(triple[2]));
        i += n;
    }
    (i == CHUNK_VOLUME).then(|| ChunkData::from_dense(blocks))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rle_roundtrip() {
        let mut c = ChunkData::Uniform(Block::STONE);
        c.set(3, 4, 5, Block::GLASS);
        c.set(31, 31, 31, Block::AIR);
        let d = decode(&encode(&c)).unwrap();
        assert_eq!(d.get(3, 4, 5), Block::GLASS);
        assert_eq!(d.get(31, 31, 31), Block::AIR);
        assert_eq!(d.get(0, 0, 0), Block::STONE);
    }
}
