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

/// `VXC3` files hold state ids from 4096 up (`STATE_CAPACITY` 16,384), so
/// builds from before that refuse them; `VXC2` has the same records. Saves
/// stay `VXC2` while every id is below [`V2_CAPACITY`], so older builds can
/// still open worlds that use no new states.
const MAGIC: &[u8; 4] = b"VXC3";
const V2_MAGIC: &[u8; 4] = b"VXC2";
const V2_CAPACITY: u16 = 4096;
const LEGACY_MAGIC: &[u8; 4] = b"VXC1";

/// The `level.txt` key holding the game version that last saved the world
/// (Cargo's form, `0.11.0`), added in v0.11. Worlds saved before it have no
/// such key; the world list shows their version as unknown.
pub const VERSION_KEY: &str = "version";

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

    /// Claims this save for one process (Java's `session.lock`): the returned file holds an
    /// exclusive lock until dropped, or until the process exits, even after a crash.
    pub fn lock(&self) -> io::Result<fs::File> {
        fs::create_dir_all(&self.dir)?;
        let file =
            fs::OpenOptions::new().create(true).truncate(false).write(true).open(self.dir.join("session.lock"))?;
        match file.try_lock() {
            Ok(()) => Ok(file),
            Err(fs::TryLockError::WouldBlock) => {
                Err(io::Error::new(io::ErrorKind::WouldBlock, "this world is open in another VoxelCraft window"))
            }
            Err(fs::TryLockError::Error(e)) => Err(e),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn exists(&self) -> bool {
        self.dir.join("level.txt").exists()
    }

    /// Whether another process holds this save's session lock. Unlike
    /// [`Storage::lock`] it never creates the lock file.
    pub fn locked_elsewhere(&self) -> bool {
        let Ok(file) = fs::OpenOptions::new().write(true).open(self.dir.join("session.lock")) else { return false };
        matches!(file.try_lock(), Err(fs::TryLockError::WouldBlock))
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
        buf.extend_from_slice(V2_MAGIC);
        buf.extend_from_slice(&(chunks.len() as u32).to_le_bytes());
        let mut high_ids = false;
        for (pos, data) in chunks {
            for c in pos.to_array() {
                buf.extend_from_slice(&c.to_le_bytes());
            }
            let rle = encode(data);
            high_ids |= rle.as_chunks::<4>().0.iter().any(|run| u16::from_le_bytes([run[2], run[3]]) >= V2_CAPACITY);
            buf.extend_from_slice(&(rle.len() as u32).to_le_bytes());
            buf.extend_from_slice(&rle);
        }

        if high_ids {
            buf[..4].copy_from_slice(MAGIC);
        }

        // Write to temp files then rename so a crash never leaves a torn save.
        let tmp = self.dir.join("chunks.bin.tmp");
        fs::File::create(&tmp)?.write_all(&buf)?;
        self.keep_unknown_format()?;
        fs::rename(&tmp, self.dir.join("chunks.bin"))?;
        self.save_level(level)
    }

    /// Moves a `chunks.bin` of a format this build can't read (a newer
    /// build's) to `chunks.bin.unknown-<unix seconds>` instead of letting a
    /// save replace it. Loading such a file fails and play continues on
    /// regenerated terrain, so without this the first save would destroy it.
    fn keep_unknown_format(&self) -> io::Result<()> {
        let path = self.dir.join("chunks.bin");
        let mut header = Vec::with_capacity(4);
        match fs::File::open(&path) {
            Ok(f) => f.take(4).read_to_end(&mut header)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        };
        if [&MAGIC[..], V2_MAGIC, LEGACY_MAGIC].contains(&header.as_slice()) {
            return Ok(());
        }
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let to = self.unused_unknown_name(secs);
        log::error!("keeping {} in an unknown format as {}", path.display(), to.display());
        fs::rename(path, to)
    }

    /// `chunks.bin.unknown-<secs>`, or with `-1`, `-2`… appended if that is
    /// taken, so an earlier kept file is never replaced. The world's lock
    /// keeps other processes from racing for the name.
    fn unused_unknown_name(&self, secs: u64) -> PathBuf {
        let base = format!("chunks.bin.unknown-{secs}");
        std::iter::once(base.clone())
            .chain((1..).map(|n| format!("{base}-{n}")))
            .map(|name| self.dir.join(name))
            .find(|p| !p.exists())
            .expect("unbounded names")
    }

    /// Writes only `level.txt`, leaving the stored chunks alone. The game
    /// saves through here, so it records this version as the last to save.
    pub fn save_level(&self, level: &LevelInfo) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let mut text = format!("seed={}\n", level.seed);
        if let Some((p, yaw, pitch)) = level.player {
            text += &format!("player={},{},{},{},{}\n", p.x, p.y, p.z, yaw, pitch);
        }
        for (k, v) in level.props.iter().filter(|(k, _)| *k != VERSION_KEY) {
            text += &format!("{k}={v}\n");
        }
        text += &format!("{VERSION_KEY}={}\n", env!("CARGO_PKG_VERSION"));
        self.write_level(&text)
    }

    /// Sets `key=value` lines in `level.txt` without otherwise touching it:
    /// other lines, their order and the saving version stay as they were,
    /// as Java's rename leaves the world's version alone.
    pub fn set_props(&self, changes: &[(&str, &str)]) -> io::Result<()> {
        let old = fs::read_to_string(self.dir.join("level.txt"))?;
        let mut pending: Vec<(&str, &str)> = changes.to_vec();
        let mut text = String::with_capacity(old.len() + 64);
        for line in old.lines() {
            let key = line.split_once('=').map(|(k, _)| k);
            match pending.iter().position(|(k, _)| Some(*k) == key) {
                Some(i) => {
                    let (k, v) = pending.remove(i);
                    text += &format!("{k}={v}\n");
                }
                None => {
                    text += line;
                    text.push('\n');
                }
            }
        }
        for (k, v) in pending {
            text += &format!("{k}={v}\n");
        }
        self.write_level(&text)
    }

    fn write_level(&self, text: &str) -> io::Result<()> {
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
        if buf.len() < 8 || ![&MAGIC[..], V2_MAGIC, LEGACY_MAGIC].contains(&&buf[0..4]) {
            return Err(bad());
        }
        let legacy = &buf[0..4] == LEGACY_MAGIC;
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
            map.insert(IVec3::new(x, y, z), Arc::new(decode(rle, legacy).ok_or_else(bad)?));
        }
        Ok(map)
    }
}

/// VXC2 runs: (run length u16 LE, block state u16 LE). VXC1 used u8 states.
fn encode(data: &ChunkData) -> Vec<u8> {
    let mut out = Vec::new();
    let mut run: Option<(Block, u16)> = None;
    data.for_each_block(|b| match &mut run {
        Some((cur, n)) if *cur == b && *n < u16::MAX => *n += 1,
        _ => {
            if let Some((cur, n)) = run {
                out.extend_from_slice(&n.to_le_bytes());
                out.extend_from_slice(&cur.0.to_le_bytes());
            }
            run = Some((b, 1));
        }
    });
    if let Some((cur, n)) = run {
        out.extend_from_slice(&n.to_le_bytes());
        out.extend_from_slice(&cur.0.to_le_bytes());
    }
    out
}

fn decode(rle: &[u8], legacy: bool) -> Option<ChunkData> {
    let stride = if legacy { 3 } else { 4 };
    if !rle.len().is_multiple_of(stride) {
        return None;
    }
    let mut blocks = ChunkData::new_dense(Block::AIR);
    let mut i = 0;
    for run in rle.chunks_exact(stride) {
        let n = u16::from_le_bytes([run[0], run[1]]) as usize;
        let id = if legacy { run[2] as u16 } else { u16::from_le_bytes([run[2], run[3]]) };
        if n == 0 || id as usize >= super::block::STATE_CAPACITY {
            return None;
        }
        // v0.4.0's axis-less bone block becomes the upright axis state.
        let block = if Block(id) == super::gadgets::LEGACY_BONE_BLOCK {
            super::nether_biome_blocks::BONE_BLOCK
        } else {
            Block(id)
        };
        blocks.get_mut(i..i + n)?.fill(block);
        i += n;
    }
    (i == CHUNK_VOLUME).then(|| ChunkData::from_dense(blocks))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_save_can_only_be_claimed_once_at_a_time() {
        let dir = std::env::temp_dir().join(format!("vx-lock-{}", std::process::id()));
        let storage = Storage::new(&dir);
        let first = storage.lock().expect("first claim");
        let second = Storage::new(&dir).lock();
        assert_eq!(second.unwrap_err().kind(), io::ErrorKind::WouldBlock);
        drop(first);
        assert!(storage.lock().is_ok(), "released when the game closes");
        let _ = fs::remove_dir_all(dir);
    }
    use super::*;

    #[test]
    fn legacy_runs_preserve_every_byte_id() {
        let mut legacy = Vec::new();
        for id in 0..=255u8 {
            legacy.extend_from_slice(&128u16.to_le_bytes());
            legacy.push(id);
        }
        let chunk = decode(&legacy, true).unwrap();
        let mut i = 0;
        chunk.for_each_block(|b| {
            assert_eq!(b, Block((i / 128) as u16));
            i += 1;
        });
        assert_eq!(i, CHUNK_VOLUME);
        let migrated = decode(&encode(&chunk), false).unwrap();
        for y in 0..32 {
            assert_eq!(migrated.get(31, y, 31), chunk.get(31, y, 31));
        }
    }

    #[test]
    fn v0_4_bone_blocks_load_upright() {
        let mut runs = Vec::new();
        runs.extend_from_slice(&(CHUNK_VOLUME as u16).to_le_bytes());
        runs.extend_from_slice(&super::super::gadgets::LEGACY_BONE_BLOCK.0.to_le_bytes());
        let chunk = decode(&runs, false).unwrap();
        assert_eq!(chunk.get(0, 0, 0), super::super::nether_biome_blocks::BONE_BLOCK);
        assert_eq!(chunk.get(0, 0, 0).info().tex[2], super::super::block::tex::BONE_BLOCK_TOP);
    }

    #[test]
    fn high_ids_roundtrip_and_malformed_runs_are_rejected() {
        let mut c = ChunkData::Uniform(Block(16383));
        c.set(3, 4, 5, Block(256));
        c.set(31, 31, 31, Block(1024));
        let encoded = encode(&c);
        let d = decode(&encoded, false).unwrap();
        assert_eq!(d.get(0, 0, 0), Block(16383));
        assert_eq!(d.get(3, 4, 5), Block(256));
        assert_eq!(d.get(31, 31, 31), Block(1024));
        let mut truncated = encoded.clone();
        truncated.pop();
        assert!(decode(&truncated, false).is_none());
        assert!(decode(&[0, 128, 0, 64], false).is_none(), "state outside registry");
        assert!(decode(&[0, 128, 0, 0, 0, 0, 1, 0], false).is_none(), "zero-length run");
        assert!(decode(&[1, 0, 1], true).is_none(), "underfilled chunk");
        assert!(decode(&[255, 255, 1], true).is_none(), "overfilled chunk");
        assert!(decode(&[0, 128, 1, 7], true).is_none(), "legacy trailing byte");
    }

    #[test]
    fn file_version_migrates_legacy_and_rejects_unknown_versions() {
        let dir = std::env::temp_dir().join(format!("voxelcraft-state-format-{}", std::process::id()));
        let storage = Storage::new(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut buf = Vec::from(&LEGACY_MAGIC[..]);
        buf.extend_from_slice(&1u32.to_le_bytes());
        for c in [-1i32, 4, 2] {
            buf.extend_from_slice(&c.to_le_bytes());
        }
        buf.extend_from_slice(&3u32.to_le_bytes());
        buf.extend_from_slice(&[0, 128, 222]);
        fs::write(dir.join("chunks.bin"), &buf).unwrap();
        let pos = IVec3::new(-1, 4, 2);
        let chunks = storage.load_chunks().unwrap();
        assert_eq!(chunks[&pos].uniform(), Some(Block::NETHERITE_BLOCK));
        let level = LevelInfo { seed: 42, player: None, props: Default::default() };
        let high = Block(super::super::block::STATE_CAPACITY as u16 - 1);
        storage.save(&level, &[(pos, Arc::new(ChunkData::Uniform(high)))]).unwrap();
        let saved = fs::read(dir.join("chunks.bin")).unwrap();
        assert_eq!(&saved[..4], MAGIC, "states from 4096 up need VXC3");
        assert_eq!(storage.load_chunks().unwrap()[&pos].uniform(), Some(high));
        storage.save(&level, &[(pos, Arc::new(ChunkData::Uniform(Block(4095))))]).unwrap();
        assert_eq!(&fs::read(dir.join("chunks.bin")).unwrap()[..4], V2_MAGIC, "older builds keep reading VXC2");
        let mut v2 = saved.clone();
        v2[..4].copy_from_slice(V2_MAGIC);
        let id_at = v2.len() - 2;
        v2[id_at..].copy_from_slice(&4095u16.to_le_bytes());
        fs::write(dir.join("chunks.bin"), &v2).unwrap();
        assert_eq!(storage.load_chunks().unwrap()[&pos].uniform(), Some(Block(4095)), "VXC2 loads unchanged");
        buf[..4].copy_from_slice(b"VXC9");
        fs::write(dir.join("chunks.bin"), &buf).unwrap();
        assert_eq!(storage.load_chunks().err().unwrap().kind(), io::ErrorKind::InvalidData);
        assert_eq!(fs::read(dir.join("chunks.bin")).unwrap(), buf, "loading leaves an unknown format alone");
        storage.save(&level, &[(pos, Arc::new(ChunkData::Uniform(Block::STONE)))]).unwrap();
        let kept: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.unwrap().file_name().into_string().ok())
            .filter(|n| n.starts_with("chunks.bin.unknown-"))
            .collect();
        assert_eq!(kept.len(), 1, "saving keeps a newer build's chunks instead of replacing them");
        assert_eq!(fs::read(dir.join(&kept[0])).unwrap(), buf);
        assert_eq!(storage.load_chunks().unwrap()[&pos].uniform(), Some(Block::STONE));
        storage.save(&level, &[(pos, Arc::new(ChunkData::Uniform(Block::STONE)))]).unwrap();
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 3, "a known format is replaced without another copy");
        // A second kept file in the same second takes a new name.
        fs::write(dir.join("chunks.bin.unknown-7"), b"A").unwrap();
        fs::write(dir.join("chunks.bin.unknown-7-1"), b"B").unwrap();
        assert_eq!(storage.unused_unknown_name(7), dir.join("chunks.bin.unknown-7-2"));
        assert_eq!(storage.unused_unknown_name(8), dir.join("chunks.bin.unknown-8"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn saves_record_their_version_and_edits_keep_it() {
        let dir = std::env::temp_dir().join(format!("voxelcraft-level-version-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let storage = Storage::new(&dir);
        let mut props = std::collections::BTreeMap::new();
        props.insert("name".to_string(), "Old".to_string());
        props.insert(VERSION_KEY.to_string(), "0.1.0".to_string());
        storage.save_level(&LevelInfo { seed: 7, player: None, props }).unwrap();
        let level = storage.load_level().unwrap();
        assert_eq!(level.props[VERSION_KEY], env!("CARGO_PKG_VERSION"), "every save records this version");
        assert_eq!(fs::read_to_string(dir.join("level.txt")).unwrap().matches("version=").count(), 1);
        // A newer build's save, then a rename here: the version stays.
        fs::write(dir.join("level.txt"), "seed=7\nname=Old\nunknown line\nversion=9.0.0\nfuture=1\n").unwrap();
        storage.set_props(&[("name", "New Name"), ("added", "x")]).unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("level.txt")).unwrap(),
            "seed=7\nname=New Name\nunknown line\nversion=9.0.0\nfuture=1\nadded=x\n"
        );
        let level = storage.load_level().unwrap();
        assert_eq!((level.seed, level.props["name"].as_str()), (7, "New Name"));
        // Worlds from before the key load as before.
        fs::write(dir.join("level.txt"), "seed=3\nname=Older\n").unwrap();
        assert!(!storage.load_level().unwrap().props.contains_key(VERSION_KEY));
        assert!(!storage.locked_elsewhere(), "no lock file: nobody has it open");
        let lock = storage.lock().unwrap();
        // This process holds it; a second handle sees it taken, as another process would.
        assert!(storage.locked_elsewhere());
        drop(lock);
        assert!(!storage.locked_elsewhere());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rle_roundtrip() {
        let mut c = ChunkData::Uniform(Block::STONE);
        c.set(3, 4, 5, Block::GLASS);
        c.set(31, 31, 31, Block::AIR);
        let d = decode(&encode(&c), false).unwrap();
        assert_eq!(d.get(3, 4, 5), Block::GLASS);
        assert_eq!(d.get(31, 31, 31), Block::AIR);
        assert_eq!(d.get(0, 0, 0), Block::STONE);
    }
}
