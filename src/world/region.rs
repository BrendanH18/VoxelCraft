//! Region files: each dimension's chunks on disk, after Java 26.3's
//! `RegionFile`, `RegionFileStorage` and `IOWorker` (v1.0 Track 3b).
//!
//! A dimension folder's `region/` holds one file per region, `r.<x>.<z>.vxr`,
//! covering 16 x 16 columns of the game's 32-block chunks: 512 x 512 blocks,
//! the same ground a Java region covers with 32 x 32 of its 16-block chunks.
//! A region has a slot for every chunk of its columns, 16 rows of them (512
//! blocks of height, more than any dimension has). As in Java's
//! `RegionFile`, the file is a table of where each chunk's record starts and
//! how long it is, followed by the records in 4096-byte sectors, so one
//! chunk can be rewritten without touching the others: a new record goes
//! into free sectors (or at the end), then its table entry points at it,
//! then its old sectors are free again. A record cut off or overwritten is
//! detected by its checksum, and that chunk regenerates.
//!
//! The layout of version 1:
//!
//! | Bytes | What |
//! | --- | --- |
//! | 0–7 | `VXREGION` |
//! | 8–11 | the file version, 1 (little-endian, as every number here) |
//! | 12–4095 | zero |
//! | 4096–36863 | 4096 table entries of 8 bytes: the record's first sector (`u32`, 0 for none) and its length in bytes (`u32`). Entry `(row * 16 + z) * 16 + x` holds the chunk at local column `x`, `z` and row `row`, each the chunk coordinate modulo 16 |
//! | 36864 on | records, each starting on a sector boundary and padded to the next |
//!
//! A record is `VXK1`, the payload's length (`u32`), its FNV-1a 64-bit
//! checksum (`u64`) and the payload ([`encode`] documents it).
//!
//! Records are written on this store's own I/O thread (Java's `IOWorker`):
//! the game queues a [`Patch`] for a chunk and goes on. Patches to the same
//! chunk merge while they wait, and reads see every queued patch, so a chunk
//! that unloads and loads again at once reads what it just wrote. The queue
//! is bounded ([`MAX_PENDING`]): past it the game waits for the disk.

use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;

use glam::{IVec2, IVec3};
use rustc_hash::{FxHashMap, FxHashSet};

use super::chunk::ChunkData;

/// The folder inside a dimension's folder that holds its region files.
pub const REGION_DIR: &str = "region";
/// Columns along each side of a region.
pub const REGION_COLUMNS: i32 = 16;
/// Chunk rows a region has slots for.
pub const REGION_ROWS: i32 = 16;
const SLOTS: usize = (REGION_COLUMNS * REGION_COLUMNS * REGION_ROWS) as usize;
/// Java's `RegionFile.SECTOR_BYTES`.
pub const SECTOR: u64 = 4096;
/// The magic sector and the 32 KiB table.
const HEADER_SECTORS: u64 = 1 + (SLOTS as u64 * 8).div_ceil(SECTOR);
const FILE_MAGIC: &[u8; 8] = b"VXREGION";
pub const FILE_VERSION: u32 = 1;
const RECORD_MAGIC: &[u8; 4] = b"VXK1";
/// The payload version [`encode`] writes.
pub const RECORD_VERSION: u8 = 1;
/// Chunks whose patches may wait for the I/O thread before queueing more
/// waits for it.
pub const MAX_PENDING: usize = 4096;
/// Region files kept open (Java's `RegionFileStorage.MAX_CACHE_SIZE` is 256
/// files of a quarter the ground).
const OPEN_FILES: usize = 64;

/// One `key=value` list: a block entity system's text or an entity batch.
pub type Props = Vec<(String, String)>;

/// Everything a chunk keeps on disk.
#[derive(Clone, Default)]
pub struct ChunkRecord {
    /// The blocks, for a chunk the player changed; `None` regenerates them.
    pub blocks: Option<Arc<ChunkData>>,
    /// Its block entities, as the level file's keys hold them (one entry
    /// per system with anything in this chunk).
    pub state: Props,
    /// Scheduled fluid ticks: water, lava and bubble-column cells.
    pub water: Vec<IVec3>,
    pub lava: Vec<IVec3>,
    pub bubbles: Vec<IVec3>,
    /// Scheduled fire ticks: the cell and the game ticks until it is due.
    pub fires: Vec<(IVec3, u32)>,
    /// Its entities, in batches of the level file's entity keys: one batch
    /// per time entities were stored here, until the chunk next loads.
    pub entities: Vec<Props>,
}

impl ChunkRecord {
    /// Whether there is nothing to keep, so the chunk needs no record.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_none()
            && self.state.is_empty()
            && self.water.is_empty()
            && self.lava.is_empty()
            && self.bubbles.is_empty()
            && self.fires.is_empty()
            && self.entities.iter().all(Vec::is_empty)
    }
}

/// The non-block part of a record that a chunk's block entity and tick
/// state replaces together.
#[derive(Clone, Default)]
pub struct StateParts {
    pub state: Props,
    pub water: Vec<IVec3>,
    pub lava: Vec<IVec3>,
    pub bubbles: Vec<IVec3>,
    pub fires: Vec<(IVec3, u32)>,
}

impl StateParts {
    pub fn is_empty(&self) -> bool {
        self.state.is_empty()
            && self.water.is_empty()
            && self.lava.is_empty()
            && self.bubbles.is_empty()
            && self.fires.is_empty()
    }
}

/// A change to one chunk's record. Fields left `None` keep what the record
/// has; `append` adds entity batches after the (kept or replaced) ones.
/// `generation` is the level file save the change belongs to (see
/// [`RegionStore::chunks_written_after`]); 0 stamps nothing.
#[derive(Clone, Default)]
pub struct Patch {
    pub blocks: Option<Option<Arc<ChunkData>>>,
    pub state: Option<StateParts>,
    pub entities: Option<Vec<Props>>,
    pub append: Vec<Props>,
    pub generation: u64,
}

impl Patch {
    /// The whole record, replacing whatever is on disk.
    pub fn replace(record: ChunkRecord) -> Self {
        let ChunkRecord { blocks, state, water, lava, bubbles, fires, entities } = record;
        Self {
            blocks: Some(blocks),
            state: Some(StateParts { state, water, lava, bubbles, fires }),
            entities: Some(entities),
            append: Vec::new(),
            generation: 0,
        }
    }

    /// Whether this patch sets every part, so the record on disk isn't read.
    fn is_total(&self) -> bool {
        self.blocks.is_some() && self.state.is_some() && self.entities.is_some()
    }

    /// `later` applied after this one.
    fn then(&mut self, later: Patch) {
        self.generation = self.generation.max(later.generation);
        if later.blocks.is_some() {
            self.blocks = later.blocks;
        }
        if later.state.is_some() {
            self.state = later.state;
        }
        if later.entities.is_some() {
            self.entities = later.entities;
            self.append = later.append;
        } else {
            self.append.extend(later.append);
        }
    }

    fn apply(&self, record: &mut ChunkRecord) {
        if let Some(blocks) = &self.blocks {
            record.blocks = blocks.clone();
        }
        if let Some(s) = &self.state {
            record.state = s.state.clone();
            record.water = s.water.clone();
            record.lava = s.lava.clone();
            record.bubbles = s.bubbles.clone();
            record.fires = s.fires.clone();
        }
        if let Some(e) = &self.entities {
            record.entities = e.clone();
        }
        record.entities.extend(self.append.iter().cloned());
    }
}

/// The region holding chunk `pos`.
pub fn region_of(pos: IVec3) -> IVec2 {
    IVec2::new(pos.x.div_euclid(REGION_COLUMNS), pos.z.div_euclid(REGION_COLUMNS))
}

/// Chunk `pos`'s table entry in its region.
pub fn slot_of(pos: IVec3) -> usize {
    let (x, z, y) = (pos.x.rem_euclid(REGION_COLUMNS), pos.z.rem_euclid(REGION_COLUMNS), pos.y.rem_euclid(REGION_ROWS));
    ((y * REGION_COLUMNS + z) * REGION_COLUMNS + x) as usize
}

/// The region file's name, `r.<x>.<z>.vxr`.
pub fn file_name(region: IVec2) -> String {
    format!("r.{}.{}.vxr", region.x, region.y)
}

/// The region a file name names, only in the form [`file_name`] writes
/// (`r.00.0.vxr` or `r.+1.0.vxr` would name an existing file's region, and
/// moving them aside would move the other one).
fn parse_file_name(name: &str) -> Option<IVec2> {
    let rest = name.strip_prefix("r.")?.strip_suffix(".vxr")?;
    let (x, z) = rest.split_once('.')?;
    let region = IVec2::new(x.parse().ok()?, z.parse().ok()?);
    (file_name(region) == name).then_some(region)
}

/// FNV-1a, 64-bit.
fn checksum(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn put_props(out: &mut Vec<u8>, props: &Props) {
    out.extend_from_slice(&(props.len() as u32).to_le_bytes());
    for (k, v) in props {
        put_str(out, k);
        put_str(out, v);
    }
}

fn put_cells(out: &mut Vec<u8>, cells: &[IVec3]) {
    out.extend_from_slice(&(cells.len() as u32).to_le_bytes());
    for c in cells {
        for v in c.to_array() {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
}

/// A record's payload, version 1:
///
/// - `u8` version (1), then the chunk's position as three `i32`s;
/// - `u8` 1 if blocks follow, then `u32` length and the blocks as the
///   `chunks.bin` `VXC4` runs (`u16` count, `u16` block state id), or 0;
/// - the block entity systems: `u32` count, then each key and text as a
///   `u32` length and UTF-8 bytes;
/// - water, lava and bubble-column cells: each a `u32` count of three `i32`s;
/// - fire ticks: `u32` count, then three `i32`s and a `u32` delay each;
/// - entity batches: `u32` count, then each as the block entity list is.
pub fn encode(pos: IVec3, record: &ChunkRecord) -> Vec<u8> {
    let mut out = Vec::with_capacity(256);
    out.push(RECORD_VERSION);
    for v in pos.to_array() {
        out.extend_from_slice(&v.to_le_bytes());
    }
    match &record.blocks {
        Some(blocks) => {
            out.push(1);
            let runs = super::storage::encode(blocks);
            out.extend_from_slice(&(runs.len() as u32).to_le_bytes());
            out.extend_from_slice(&runs);
        }
        None => out.push(0),
    }
    put_props(&mut out, &record.state);
    put_cells(&mut out, &record.water);
    put_cells(&mut out, &record.lava);
    put_cells(&mut out, &record.bubbles);
    out.extend_from_slice(&(record.fires.len() as u32).to_le_bytes());
    for (c, delay) in &record.fires {
        for v in c.to_array() {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&delay.to_le_bytes());
    }
    out.extend_from_slice(&(record.entities.len() as u32).to_le_bytes());
    for batch in &record.entities {
        put_props(&mut out, batch);
    }
    out
}

struct Reader<'a> {
    buf: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.buf.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn len(&mut self) -> Option<usize> {
        let n = self.u32()? as usize;
        // No list is longer than the bytes left to hold it.
        (n <= self.buf.len() - self.at).then_some(n)
    }
    fn str(&mut self) -> Option<String> {
        let n = self.len()?;
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }
    fn props(&mut self) -> Option<Props> {
        let n = self.len()?;
        (0..n).map(|_| Some((self.str()?, self.str()?))).collect()
    }
    fn cell(&mut self) -> Option<IVec3> {
        Some(IVec3::new(self.i32()?, self.i32()?, self.i32()?))
    }
    fn cells(&mut self) -> Option<Vec<IVec3>> {
        let n = self.len()?;
        (0..n).map(|_| self.cell()).collect()
    }
}

/// Reads [`encode`]'s payload for the chunk at `pos`.
pub fn decode(pos: IVec3, payload: &[u8]) -> Result<ChunkRecord, &'static str> {
    let mut r = Reader { buf: payload, at: 0 };
    let version = r.u8().ok_or("it is empty")?;
    if version != RECORD_VERSION {
        return Err("it is from a newer version");
    }
    let bad = "a chunk record is malformed";
    if r.cell().ok_or(bad)? != pos {
        return Err("a chunk record is in the wrong place");
    }
    let blocks = match r.u8().ok_or(bad)? {
        0 => None,
        1 => {
            let n = r.len().ok_or(bad)?;
            let runs = r.take(n).ok_or(bad)?;
            Some(Arc::new(super::storage::decode(runs, false).ok_or(bad)?))
        }
        _ => return Err(bad),
    };
    let state = r.props().ok_or(bad)?;
    let water = r.cells().ok_or(bad)?;
    let lava = r.cells().ok_or(bad)?;
    let bubbles = r.cells().ok_or(bad)?;
    let n = r.len().ok_or(bad)?;
    let fires = (0..n).map(|_| Some((r.cell()?, r.u32()?))).collect::<Option<Vec<_>>>().ok_or(bad)?;
    let n = r.len().ok_or(bad)?;
    let entities = (0..n).map(|_| r.props()).collect::<Option<Vec<_>>>().ok_or(bad)?;
    if r.at != payload.len() {
        return Err(bad);
    }
    Ok(ChunkRecord { blocks, state, water, lava, bubbles, fires, entities })
}

/// What opening a damaged region file found.
#[derive(Clone, Debug)]
pub struct RegionDamage {
    pub file: String,
    pub reason: &'static str,
    /// Chunks the table listed, and how many of them still read.
    pub saved: usize,
    pub kept: usize,
    /// The untouched copy, `r.<x>.<z>.vxr.damaged-<secs>`.
    pub kept_as: Option<PathBuf>,
}

impl RegionDamage {
    /// The notice the player reads.
    pub fn notice(&self, dimension: &str) -> String {
        let kept = self.kept_as.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
        if self.saved == 0 {
            // The whole file: unreadable, or nothing in its table.
            return format!(
                "The {dimension}'s saved chunks in {} couldn't be read ({}), so they regenerate.{}",
                self.file,
                self.reason,
                kept.map(|k| format!(" The file was kept as {k}.")).unwrap_or_default()
            );
        }
        format!(
            "Some of the {dimension}'s saved chunks in {} couldn't be read ({}): {} of {} were kept and the rest regenerate.{}",
            self.file,
            self.reason,
            self.kept,
            self.saved,
            kept.map(|k| format!(" The damaged file was kept as {k}.")).unwrap_or_default()
        )
    }
}

/// What opening a file dropped from its table: why, the chunks it listed and
/// the chunks still readable.
type TableDamage = (&'static str, usize, usize);

/// One open region file (Java's `RegionFile`).
struct RegionFile {
    file: File,
    /// Each slot's first sector and length in bytes (0: no record).
    table: Box<[(u32, u32)]>,
    /// Which sectors are in use.
    used: Vec<bool>,
    /// The table on disk differs from `table` beyond single entries
    /// (damage was dropped), so the next write rewrites all of it.
    rewrite_header: bool,
    /// The newest level file save any record here belongs to (bytes
    /// 12–19 of the header; 0 in files written before it was kept).
    generation: u64,
}

fn sectors(bytes: u64) -> u64 {
    bytes.div_ceil(SECTOR)
}

impl RegionFile {
    /// Opens `path`, creating it if `create`. Entries that point outside the
    /// file or over another record are dropped and reported.
    fn open(path: &Path, create: bool) -> io::Result<Option<(Self, Option<TableDamage>)>> {
        let file = match OpenOptions::new().read(true).write(true).open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound && create => {
                let mut f = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(path)?;
                let mut header = vec![0u8; (HEADER_SECTORS * SECTOR) as usize];
                header[..8].copy_from_slice(FILE_MAGIC);
                header[8..12].copy_from_slice(&FILE_VERSION.to_le_bytes());
                f.write_all(&header)?;
                let used = vec![true; HEADER_SECTORS as usize];
                return Ok(Some((
                    Self { file: f, table: vec![(0, 0); SLOTS].into(), used, rewrite_header: false, generation: 0 },
                    None,
                )));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let mut file = file;
        let len = file.metadata()?.len();
        let mut header = Vec::with_capacity((HEADER_SECTORS * SECTOR) as usize);
        (&mut file).take(HEADER_SECTORS * SECTOR).read_to_end(&mut header)?;
        let mut reason = None;
        if header.len() < 12 || &header[..8] != FILE_MAGIC {
            reason = Some(if header.len() < 12 { "it is cut off" } else { "its header is unreadable" });
        } else if u32::from_le_bytes(header[8..12].try_into().unwrap()) != FILE_VERSION {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "a region file is from a newer version"));
        }
        if header.len() < (HEADER_SECTORS * SECTOR) as usize {
            reason = Some("it is cut off");
        }
        let total_sectors = sectors(len).max(HEADER_SECTORS);
        let mut used = vec![false; total_sectors as usize];
        used[..HEADER_SECTORS as usize].fill(true);
        let mut table: Box<[(u32, u32)]> = vec![(0, 0); SLOTS].into();
        let (mut saved, mut kept) = (0, 0);
        for (i, entry) in table.iter_mut().enumerate() {
            let at = SECTOR as usize + i * 8;
            let Some(bytes) = header.get(at..at + 8) else {
                break;
            };
            let off = u32::from_le_bytes(bytes[..4].try_into().unwrap());
            let size = u32::from_le_bytes(bytes[4..].try_into().unwrap());
            if size == 0 {
                continue;
            }
            saved += 1;
            let first = u64::from(off);
            let count = sectors(u64::from(size));
            let fits = first >= HEADER_SECTORS && first * SECTOR + u64::from(size) <= len;
            let free = fits && used[first as usize..(first + count) as usize].iter().all(|u| !u);
            if !free {
                reason.get_or_insert(if fits { "two records overlap" } else { "it is cut off" });
                continue;
            }
            used[first as usize..(first + count) as usize].fill(true);
            *entry = (off, size);
            kept += 1;
        }
        let generation = header
            .get(12..20)
            .filter(|_| &header[..8] == FILE_MAGIC)
            .map_or(0, |b| u64::from_le_bytes(b.try_into().unwrap()));
        let damage = reason.map(|r| (r, saved, kept));
        Ok(Some((Self { file, table, used, rewrite_header: damage.is_some(), generation }, damage)))
    }

    /// The payload of `slot`'s record, `Ok(None)` if it has none and an
    /// error if it doesn't read.
    fn read(&mut self, slot: usize) -> io::Result<Option<Vec<u8>>> {
        let (off, size) = self.table[slot];
        if size == 0 {
            return Ok(None);
        }
        let bad = |why: &str| io::Error::new(io::ErrorKind::InvalidData, why.to_string());
        self.file.seek(SeekFrom::Start(u64::from(off) * SECTOR))?;
        let mut frame = vec![0u8; size as usize];
        self.file.read_exact(&mut frame).map_err(|_| bad("it is cut off"))?;
        if frame.len() < 16 || &frame[..4] != RECORD_MAGIC {
            return Err(bad("a chunk record is malformed"));
        }
        let n = u32::from_le_bytes(frame[4..8].try_into().unwrap()) as usize;
        let sum = u64::from_le_bytes(frame[8..16].try_into().unwrap());
        let payload = frame.split_off(16);
        if payload.len() != n || checksum(&payload) != sum {
            return Err(bad("a chunk record is damaged"));
        }
        Ok(Some(payload))
    }

    /// Writes `slot`'s record (`None` removes it), Java's `RegionFile.write`:
    /// the new record goes into free sectors, then the table entry moves
    /// to it, then the old sectors are free.
    fn write(&mut self, slot: usize, payload: Option<&[u8]>) -> io::Result<()> {
        let (old_off, old_size) = self.table[slot];
        let new = match payload {
            None if old_size == 0 => return Ok(()),
            None => (0, 0),
            Some(payload) => {
                let mut frame = Vec::with_capacity(16 + payload.len());
                frame.extend_from_slice(RECORD_MAGIC);
                frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
                frame.extend_from_slice(&checksum(payload).to_le_bytes());
                frame.extend_from_slice(payload);
                let size = frame.len() as u64;
                let count = sectors(size) as usize;
                frame.resize(count * SECTOR as usize, 0);
                let first = self.allocate(count);
                self.file.seek(SeekFrom::Start(first as u64 * SECTOR))?;
                self.file.write_all(&frame)?;
                let size = u32::try_from(size).map_err(|_| io::Error::other("a chunk record is over 4 GiB"))?;
                (first as u32, size)
            }
        };
        self.table[slot] = new;
        if !self.rewrite_header {
            let mut entry = [0u8; 8];
            entry[..4].copy_from_slice(&new.0.to_le_bytes());
            entry[4..].copy_from_slice(&new.1.to_le_bytes());
            let written =
                self.file.seek(SeekFrom::Start(SECTOR + slot as u64 * 8)).and_then(|_| self.file.write_all(&entry));
            if let Err(e) = written {
                // The record didn't land: the slot keeps its old record (a
                // retry must not build on the new one), and the whole table
                // is written again with the next write.
                self.table[slot] = (old_off, old_size);
                if new.1 > 0 {
                    let (first, count) = (new.0 as usize, sectors(u64::from(new.1)) as usize);
                    self.used[first..first + count].fill(false);
                }
                self.rewrite_header = true;
                return Err(e);
            }
        }
        if old_size > 0 {
            let first = old_off as usize;
            let count = sectors(u64::from(old_size)) as usize;
            self.used[first..first + count].fill(false);
        }
        Ok(())
    }

    /// The first run of `count` free sectors, extending the file if none.
    fn allocate(&mut self, count: usize) -> usize {
        let mut run = 0;
        for i in HEADER_SECTORS as usize..self.used.len() {
            if self.used[i] {
                run = 0;
                continue;
            }
            run += 1;
            if run == count {
                let first = i + 1 - count;
                self.used[first..=i].fill(true);
                return first;
            }
        }
        // Reuse a free tail, then grow.
        let first = self.used.len() - run;
        self.used.resize(first + count, true);
        self.used[first..].fill(true);
        first
    }

    /// Records that a record of level file save `generation` is about to
    /// be written here, before it is (so a record on disk is never newer
    /// than the header says).
    fn mark(&mut self, generation: u64) -> io::Result<()> {
        if generation <= self.generation {
            return Ok(());
        }
        self.file.seek(SeekFrom::Start(12))?;
        self.file.write_all(&generation.to_le_bytes())?;
        self.generation = generation;
        Ok(())
    }

    /// Rewrites the whole header after damage was dropped from the table.
    fn finish(&mut self) -> io::Result<()> {
        if !self.rewrite_header {
            return Ok(());
        }
        let mut header = vec![0u8; (HEADER_SECTORS * SECTOR) as usize];
        header[..8].copy_from_slice(FILE_MAGIC);
        header[8..12].copy_from_slice(&FILE_VERSION.to_le_bytes());
        header[12..20].copy_from_slice(&self.generation.to_le_bytes());
        for (i, (off, size)) in self.table.iter().enumerate() {
            let at = SECTOR as usize + i * 8;
            header[at..at + 4].copy_from_slice(&off.to_le_bytes());
            header[at + 4..at + 8].copy_from_slice(&size.to_le_bytes());
        }
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&header)?;
        self.rewrite_header = false;
        Ok(())
    }
}

/// Open region files, at most [`OPEN_FILES`] (Java's `RegionFileStorage`).
#[derive(Default)]
struct Files {
    open: FxHashMap<IVec2, RegionFile>,
    order: VecDeque<IVec2>,
    /// Regions whose damage has been copied aside this session.
    kept: FxHashSet<IVec2>,
    /// Region files that couldn't be opened and stay where they are (a
    /// read-only store, or one that couldn't move them aside): their chunks
    /// regenerate and nothing is written to them.
    broken: FxHashSet<IVec2>,
    /// False for a read-only store: nothing is copied or rewritten.
    writable: bool,
}

struct Queue {
    pending: FxHashMap<IVec3, Patch>,
    /// The patches the I/O thread is writing, until each is on disk.
    writing: FxHashMap<IVec3, Arc<Patch>>,
    /// Patches whose write failed. They are older than any pending patch
    /// of their chunk and are written again with the next batch; until
    /// then no level file is written.
    failed: FxHashMap<IVec3, Patch>,
    /// Work to run once everything queued before it is on disk (the level
    /// file, which must never name chunks whose records aren't written).
    after: Vec<Box<dyn FnOnce() -> io::Result<()> + Send>>,
    busy: bool,
    shutdown: bool,
    errors: Vec<String>,
    notices: Vec<String>,
    written: u64,
    bytes: u64,
}

struct Shared {
    dir: PathBuf,
    queue: Mutex<Queue>,
    work: Condvar,
    done: Condvar,
    files: Mutex<Files>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// A dimension's region storage: its region files and the I/O thread that
/// writes them (Java's `RegionFileStorage` and `IOWorker`).
pub struct RegionStore {
    shared: Arc<Shared>,
    writer: Option<thread::JoinHandle<()>>,
    /// Every chunk with a record when the store opened.
    stored: FxHashSet<IVec3>,
    /// Each region file's newest save generation when the store opened,
    /// for those that have one.
    generations: Vec<(IVec2, u64)>,
}

/// A handle that reads chunks, for the generation workers.
#[derive(Clone)]
pub struct RegionReader(Arc<Shared>);

impl RegionStore {
    /// Opens the region folder `dir` (created when the first record is
    /// written), checks every region file's table and starts the I/O
    /// thread. A damaged file is copied to `<name>.damaged-<secs>` first;
    /// what it lost is returned for the player. A file that can't be opened
    /// at all (unreadable, or from a newer version) is moved aside to that
    /// name instead, and its chunks regenerate; the other regions are
    /// unaffected.
    /// `writable` is false for a run that never writes saves (scripted
    /// captures): it reads records and copies nothing aside.
    pub fn open(dir: impl Into<PathBuf>, writable: bool) -> io::Result<(Self, Vec<RegionDamage>)> {
        let dir = dir.into();
        let mut files = Files { writable, ..Default::default() };
        let mut damage = Vec::new();
        let mut stored = FxHashSet::default();
        let mut generations = Vec::new();
        let mut names: Vec<(IVec2, PathBuf)> = match fs::read_dir(&dir) {
            Ok(entries) => entries
                .filter_map(|e| e.ok())
                .filter_map(|e| Some((parse_file_name(e.file_name().to_str()?)?, e.path())))
                .collect(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        names.sort_unstable_by_key(|(r, _)| (r.x, r.y));
        for (region, path) in names {
            let (file, hurt) = match RegionFile::open(&path, false) {
                Ok(Some(opened)) => opened,
                Ok(None) => continue,
                Err(e) => {
                    log::error!("couldn't open {}: {e}", path.display());
                    let reason = if e.kind() == io::ErrorKind::InvalidData {
                        "it is from a newer version"
                    } else {
                        "it can't be opened"
                    };
                    let kept_as = if writable { move_aside(&dir, region) } else { None };
                    if kept_as.is_none() {
                        files.broken.insert(region);
                    }
                    damage.push(RegionDamage { file: file_name(region), reason, saved: 0, kept: 0, kept_as });
                    continue;
                }
            };
            if file.generation > 0 {
                generations.push((region, file.generation));
            }
            for (slot, &(_, size)) in file.table.iter().enumerate() {
                if size > 0 {
                    stored.insert(slot_pos(region, slot));
                }
            }
            if let Some((reason, saved, kept)) = hurt {
                let kept_as = if writable { keep_damaged(&dir, region) } else { None };
                files.kept.insert(region);
                damage.push(RegionDamage { file: file_name(region), reason, saved, kept, kept_as });
            }
            if files.open.len() < OPEN_FILES {
                files.order.push_back(region);
                files.open.insert(region, file);
            }
        }
        let shared = Arc::new(Shared {
            dir,
            queue: Mutex::new(Queue {
                pending: FxHashMap::default(),
                writing: FxHashMap::default(),
                failed: FxHashMap::default(),
                after: Vec::new(),
                busy: false,
                shutdown: false,
                errors: Vec::new(),
                notices: Vec::new(),
                written: 0,
                bytes: 0,
            }),
            work: Condvar::new(),
            done: Condvar::new(),
            files: Mutex::new(files),
        });
        let worker = shared.clone();
        let writer = thread::Builder::new().name("region-io".into()).spawn(move || write_loop(&worker))?;
        Ok((Self { shared, writer: Some(writer), stored, generations }, damage))
    }

    pub fn dir(&self) -> &Path {
        &self.shared.dir
    }

    pub fn reader(&self) -> RegionReader {
        RegionReader(self.shared.clone())
    }

    /// Chunks that had a record when the store opened (until
    /// [`RegionStore::forget_stored_at_open`]).
    pub fn stored_at_open(&self) -> &FxHashSet<IVec3> {
        &self.stored
    }

    /// Frees [`RegionStore::stored_at_open`] once nothing needs it (a
    /// large world's list is megabytes).
    pub fn forget_stored_at_open(&mut self) {
        self.stored = FxHashSet::default();
    }

    /// Queues `patches`, merging each into what already waits for its
    /// chunk. Waits while [`MAX_PENDING`] chunks are queued.
    pub fn queue(&self, patches: impl IntoIterator<Item = (IVec3, Patch)>) {
        let mut q = lock(&self.shared.queue);
        for (pos, patch) in patches {
            while q.pending.len() >= MAX_PENDING && !q.pending.contains_key(&pos) {
                self.shared.work.notify_one();
                q = self.shared.done.wait(q).unwrap_or_else(|e| e.into_inner());
            }
            match q.pending.get_mut(&pos) {
                Some(p) => p.then(patch),
                None => {
                    q.pending.insert(pos, patch);
                }
            }
        }
        self.shared.work.notify_one();
    }

    /// Runs `work` on the I/O thread once everything queued so far is on
    /// disk. Its error is reported like a failed record write.
    pub fn after_writes(&self, work: impl FnOnce() -> io::Result<()> + Send + 'static) {
        let mut q = lock(&self.shared.queue);
        q.after.push(Box::new(work));
        self.shared.work.notify_one();
    }

    /// Waits until everything queued is on disk, then returns the errors
    /// since the last call, if any.
    pub fn flush(&self) -> io::Result<()> {
        let mut q = lock(&self.shared.queue);
        self.shared.work.notify_one();
        while !q.pending.is_empty() || !q.writing.is_empty() || !q.after.is_empty() || q.busy {
            q = self.shared.done.wait(q).unwrap_or_else(|e| e.into_inner());
        }
        let errors = std::mem::take(&mut q.errors);
        if errors.is_empty() { Ok(()) } else { Err(io::Error::other(errors.join("; "))) }
    }

    /// The newest level file save generation any region file held a record
    /// of when the store opened (0 if none says).
    pub fn generation(&self) -> u64 {
        self.generations.iter().map(|&(_, g)| g).max().unwrap_or(0)
    }

    /// The chunks with a record in a region file that held records of a
    /// save newer than `generation` when the store opened: where a session
    /// that stopped before that save finished wrote after it.
    pub fn chunks_written_after(&self, generation: u64) -> Vec<IVec3> {
        let mut files = lock(&self.shared.files);
        let mut out = Vec::new();
        for &(region, _) in self.generations.iter().filter(|&&(_, g)| g > generation) {
            match files.get(&self.shared.dir, region, false) {
                Ok(Some(file)) => out.extend(
                    file.table.iter().enumerate().filter(|(_, e)| e.1 > 0).map(|(slot, _)| slot_pos(region, slot)),
                ),
                Ok(None) => {}
                Err(e) => log::error!("couldn't reopen {}: {e}", file_name(region)),
            }
        }
        out
    }

    /// Write errors since the last call (each also logged).
    pub fn take_errors(&self) -> Vec<String> {
        std::mem::take(&mut lock(&self.shared.queue).errors)
    }

    /// Damage found reading chunks since the last call, for the player.
    pub fn take_notices(&self) -> Vec<String> {
        std::mem::take(&mut lock(&self.shared.queue).notices)
    }

    /// Chunks waiting to be written, records written and their bytes, for
    /// the soak's diagnostics.
    pub fn stats(&self) -> (usize, u64, u64) {
        let q = lock(&self.shared.queue);
        (q.pending.len() + q.writing.len() + q.failed.len(), q.written, q.bytes)
    }

    /// Reads chunk `pos`'s record now, on this thread.
    pub fn read(&self, pos: IVec3) -> Option<ChunkRecord> {
        self.reader().read(pos)
    }
}

impl Drop for RegionStore {
    /// Writes everything queued, then stops the I/O thread.
    fn drop(&mut self) {
        {
            let mut q = lock(&self.shared.queue);
            q.shutdown = true;
            self.shared.work.notify_all();
        }
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
        for e in self.take_errors() {
            log::error!("region write failed while closing: {e}");
        }
        let lost = lock(&self.shared.queue).failed.len();
        if lost > 0 {
            log::error!("{lost} chunk records couldn't be written before closing");
        }
    }
}

fn slot_pos(region: IVec2, slot: usize) -> IVec3 {
    let slot = slot as i32;
    let (x, z, y) =
        (slot % REGION_COLUMNS, slot / REGION_COLUMNS % REGION_COLUMNS, slot / (REGION_COLUMNS * REGION_COLUMNS));
    // Rows are stored modulo 16; a dimension's rows start at or below 0
    // and span at most 16, so rows 8 and up of the slot mean negative ones
    // only when the dimension goes below 0 (the Overworld's -2 and -1).
    let y = if y >= REGION_ROWS - 2 { y - REGION_ROWS } else { y };
    IVec3::new(region.x * REGION_COLUMNS + x, y, region.y * REGION_COLUMNS + z)
}

fn unix_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// An unused `r.<x>.<z>.vxr.damaged-<secs>` path for a region file's copy.
fn damaged_path(dir: &Path, region: IVec2) -> Option<PathBuf> {
    let base = format!("{}.damaged-{}", file_name(region), unix_secs());
    std::iter::once(base.clone()).chain((1..).map(|n| format!("{base}-{n}"))).map(|n| dir.join(n)).find(|p| !p.exists())
}

/// Moves a region file that can't be opened aside, so its region starts
/// again empty while the rest of the dimension's regions load.
fn move_aside(dir: &Path, region: IVec2) -> Option<PathBuf> {
    let name = file_name(region);
    let to = damaged_path(dir, region)?;
    match fs::rename(dir.join(&name), &to) {
        Ok(()) => Some(to),
        Err(e) => {
            log::error!("couldn't move the unreadable {name} aside: {e}");
            None
        }
    }
}

/// Copies a damaged region file aside, untouched, before anything writes it.
fn keep_damaged(dir: &Path, region: IVec2) -> Option<PathBuf> {
    let name = file_name(region);
    let to = damaged_path(dir, region)?;
    match fs::copy(dir.join(&name), &to) {
        Ok(_) => Some(to),
        Err(e) => {
            log::error!("couldn't keep a copy of the damaged {name}: {e}");
            None
        }
    }
}

impl Files {
    fn get(&mut self, dir: &Path, region: IVec2, create: bool) -> io::Result<Option<&mut RegionFile>> {
        if self.broken.contains(&region) {
            return if create {
                Err(io::Error::other("the file couldn't be read and is kept unchanged"))
            } else {
                Ok(None)
            };
        }
        if !self.open.contains_key(&region) {
            if create {
                fs::create_dir_all(dir)?;
            }
            let Some((file, damage)) = RegionFile::open(&dir.join(file_name(region)), create)? else {
                return Ok(None);
            };
            if damage.is_some() && self.writable && self.kept.insert(region) {
                keep_damaged(dir, region);
            }
            // A file whose table can't be written yet stays open (over the
            // limit if need be), so the next level file still waits for it.
            let mut tries = self.order.len();
            while self.open.len() >= OPEN_FILES && tries > 0 {
                tries -= 1;
                let Some(old) = self.order.pop_front() else { break };
                let Some(mut f) = self.open.remove(&old) else { continue };
                if self.writable
                    && let Err(e) = f.finish()
                {
                    log::error!("couldn't finish {}: {e}; it stays open", file_name(old));
                    self.open.insert(old, f);
                    self.order.push_back(old);
                }
            }
            self.order.push_back(region);
            self.open.insert(region, file);
        }
        Ok(self.open.get_mut(&region))
    }
}

impl RegionReader {
    /// Chunk `pos`'s record, with every queued patch applied. A record that
    /// doesn't read is reported (see [`RegionStore::take_notices`]) and
    /// read as none, so the chunk regenerates, as Java's `ChunkMap` does
    /// with a chunk it fails to read.
    pub fn read(&self, pos: IVec3) -> Option<ChunkRecord> {
        let shared = &*self.0;
        // Files first, then the queue, as the I/O thread takes them, so a
        // patch is either still queued or already on disk, never both.
        let mut files = lock(&shared.files);
        let (writing, pending) = {
            let q = lock(&shared.queue);
            let writing = q.writing.get(&pos).map(|p| (**p).clone()).or_else(|| q.failed.get(&pos).cloned());
            (writing, q.pending.get(&pos).cloned())
        };
        let mut patch = writing.unwrap_or_default();
        if let Some(p) = pending {
            patch.then(p);
        }
        let mut record = ChunkRecord::default();
        if !patch.is_total() {
            let region = region_of(pos);
            match files.get(&shared.dir, region, false) {
                Ok(None) => {}
                Ok(Some(file)) => match file.read(slot_of(pos)) {
                    Ok(None) => {}
                    Ok(Some(payload)) => match decode(pos, &payload) {
                        Ok(r) => record = r,
                        Err(why) => damaged(shared, &mut files, region, pos, why),
                    },
                    Err(e) if e.kind() == io::ErrorKind::InvalidData || e.kind() == io::ErrorKind::UnexpectedEof => {
                        damaged(shared, &mut files, region, pos, &e.to_string())
                    }
                    Err(e) => {
                        log::error!("failed to read chunk {pos} from its region: {e}");
                        lock(&shared.queue)
                            .notices
                            .push(format!("A saved chunk couldn't be read ({e}); it regenerates."));
                    }
                },
                Err(e) => {
                    log::error!("failed to open the region of chunk {pos}: {e}");
                    lock(&shared.queue)
                        .notices
                        .push(format!("A region file couldn't be read ({e}); its chunks regenerate."));
                }
            }
        }
        drop(files);
        patch.apply(&mut record);
        (!record.is_empty()).then_some(record)
    }
}

/// A record that doesn't read: Java's "Failed to read chunk". The region
/// file is copied aside once, the entry is dropped so the chunk can be
/// written again, and the player is told.
fn damaged(shared: &Shared, files: &mut Files, region: IVec2, pos: IVec3, why: &str) {
    log::error!("failed to read chunk {pos} from {}: {why}", file_name(region));
    let kept = if files.writable && files.kept.insert(region) { keep_damaged(&shared.dir, region) } else { None };
    if let Some(f) = files.open.get_mut(&region) {
        let slot = slot_of(pos);
        let (off, size) = f.table[slot];
        if size > 0 {
            let count = sectors(u64::from(size)) as usize;
            let first = off as usize;
            if first + count <= f.used.len() {
                f.used[first..first + count].fill(false);
            }
            f.table[slot] = (0, 0);
            f.rewrite_header = true;
        }
    }
    let kept = kept.and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
    lock(&shared.queue).notices.push(format!(
        "A saved chunk in {} couldn't be read ({why}), so it regenerates.{}",
        file_name(region),
        kept.map(|k| format!(" The damaged file was kept as {k}.")).unwrap_or_default()
    ));
}

/// The I/O thread: takes every queued patch (and every failed one again),
/// writes them region by region, then runs the work waiting for them, but
/// only if every one of them is on disk: a level file never names records
/// that failed to write. Failed patches wait for the next batch.
fn write_loop(shared: &Shared) {
    loop {
        let (batch, after) = {
            let mut q = lock(&shared.queue);
            while q.pending.is_empty() && q.after.is_empty() && !q.shutdown {
                q = shared.work.wait(q).unwrap_or_else(|e| e.into_inner());
            }
            if q.pending.is_empty() && q.after.is_empty() {
                break;
            }
            let mut pending = std::mem::take(&mut q.failed);
            for (pos, patch) in std::mem::take(&mut q.pending) {
                match pending.get_mut(&pos) {
                    Some(older) => older.then(patch),
                    None => {
                        pending.insert(pos, patch);
                    }
                }
            }
            let mut batch: Vec<(IVec3, Arc<Patch>)> =
                pending.into_iter().map(|(p, patch)| (p, Arc::new(patch))).collect();
            batch.sort_unstable_by_key(|(p, _)| (region_of(*p).x, region_of(*p).y, slot_of(*p)));
            q.writing = batch.iter().map(|(p, patch)| (*p, patch.clone())).collect();
            q.busy = true;
            (batch, std::mem::take(&mut q.after))
        };
        // Someone may be waiting for room in the queue.
        shared.done.notify_all();
        let mut start = 0;
        let mut whole = true;
        while start < batch.len() {
            let region = region_of(batch[start].0);
            let end = start + batch[start..].iter().take_while(|(p, _)| region_of(*p) == region).count();
            whole &= write_region(shared, region, &batch[start..end]);
            start = end;
        }
        let mut failed = Vec::new();
        if whole && !after.is_empty() {
            // A table whose rewrite failed earlier must be on disk first.
            let mut files = lock(&shared.files);
            let writable = files.writable;
            for f in files.open.values_mut().filter(|_| writable) {
                if let Err(e) = f.finish() {
                    failed.push(format!("a region file's table: {e}"));
                    whole = false;
                }
            }
        }
        if whole {
            for work in after {
                if let Err(e) = work() {
                    failed.push(e.to_string());
                }
            }
        } else if !after.is_empty() {
            // The level file waits for the next save, which writes the
            // failed records again first.
            log::error!("not writing the level file: some chunk records failed to write");
        }
        let mut q = lock(&shared.queue);
        for e in failed {
            log::error!("save failed: {e}");
            q.errors.push(e);
        }
        q.busy = false;
        shared.done.notify_all();
    }
    // Leave the table complete in every file still open.
    let mut files = lock(&shared.files);
    if !files.writable {
        return;
    }
    for f in files.open.values_mut() {
        if let Err(e) = f.finish() {
            log::error!("failed to finish a region file: {e}");
        }
    }
}

/// Writes one region's patches; whether all of them are on disk. Those that
/// failed go back to the queue's failed patches.
fn write_region(shared: &Shared, region: IVec2, patches: &[(IVec3, Arc<Patch>)]) -> bool {
    let mut files = lock(&shared.files);
    let mut errors = Vec::new();
    let mut failed: FxHashSet<IVec3> = FxHashSet::default();
    let mut bytes = 0;
    let mut written = 0;
    let generation = patches.iter().map(|(_, p)| p.generation).max().unwrap_or(0);
    match files.get(&shared.dir, region, true).and_then(|f| {
        let f = f.expect("region files are created on write");
        f.mark(generation)?;
        Ok(f)
    }) {
        Ok(file) => {
            for (pos, patch) in patches {
                let mut record = ChunkRecord::default();
                if !patch.is_total() {
                    // A partial patch builds on the record on disk; one that
                    // doesn't read is replaced (the reader already said so).
                    if let Ok(Some(payload)) = file.read(slot_of(*pos))
                        && let Ok(r) = decode(*pos, &payload)
                    {
                        record = r;
                    }
                }
                patch.apply(&mut record);
                let payload = (!record.is_empty()).then(|| encode(*pos, &record));
                match file.write(slot_of(*pos), payload.as_deref()) {
                    Ok(()) => {
                        written += 1;
                        bytes += payload.map_or(0, |p| p.len() as u64);
                    }
                    Err(e) => {
                        errors.push(format!("{}: {e}", file_name(region)));
                        failed.insert(*pos);
                    }
                }
            }
            // The table on disk is complete only once the header is (the
            // records are in the table in memory, so they aren't retried).
            if let Err(e) = file.finish() {
                errors.push(format!("{}: {e}", file_name(region)));
            }
        }
        Err(e) => {
            errors.push(format!("{}: {e}", file_name(region)));
            failed.extend(patches.iter().map(|(p, _)| *p));
        }
    }
    let whole = errors.is_empty();
    // Still holding the files: from here a reader finds these on disk, or
    // among the failed patches.
    let mut q = lock(&shared.queue);
    for (pos, patch) in patches {
        q.writing.remove(pos);
        if failed.contains(pos) {
            q.failed.insert(*pos, (**patch).clone());
        }
    }
    q.written += written;
    q.bytes += bytes;
    for e in errors {
        log::error!("region write failed: {e}");
        q.errors.push(e);
    }
    whole
}

#[cfg(test)]
mod tests;
