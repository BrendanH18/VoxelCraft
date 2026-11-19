//! Drawing helpers shared by the generation 4 catalogue painters (v0.15
//! Track G): a frame that turns a piece so its front faces a given way, face
//! cells for connectors, and cached block names.

use std::sync::Mutex;

use glam::IVec3;
use rustc_hash::FxHashMap;

use super::super::block::{Block, Facing};
use super::catalogue::{Conn, Draft, PieceSpec};

/// A block by name, looked up once (`Block::from_name` scans every state).
/// A name VoxelCraft lacks gives stone and is listed by [`missing_names`],
/// which the catalogue tests require to be empty.
pub fn named(name: &str) -> Block {
    static CACHE: Mutex<Option<FxHashMap<String, Option<Block>>>> = Mutex::new(None);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let map = cache.get_or_insert_with(FxHashMap::default);
    let b = *map.entry(name.to_string()).or_insert_with(|| Block::from_name(name));
    b.unwrap_or_else(|| {
        MISSING.lock().unwrap_or_else(|e| e.into_inner()).push(name.to_string());
        Block::STONE
    })
}

static MISSING: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Names [`named`] could not find, sorted, without repeats.
pub fn missing_names() -> Vec<String> {
    let mut v = MISSING.lock().unwrap_or_else(|e| e.into_inner()).clone();
    v.sort();
    v.dedup();
    v
}

/// Quarter turns clockwise from north.
pub fn turns(f: Facing) -> u8 {
    match f {
        Facing::North => 0,
        Facing::East => 1,
        Facing::South => 2,
        Facing::West => 3,
    }
}

/// A piece seen from its front: `u` runs across the front (west to east
/// when the front faces north), `v` from the front row (0) to the back, `y`
/// up. Local north is out of the front.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub front: Facing,
    pub size: IVec3,
}

impl Frame {
    pub fn new(front: Facing, size: IVec3) -> Frame {
        Frame { front, size }
    }

    /// Width across the front.
    pub fn w(&self) -> i32 {
        if self.front.along_x() { self.size.z } else { self.size.x }
    }

    /// Depth from the front.
    pub fn d(&self) -> i32 {
        if self.front.along_x() { self.size.x } else { self.size.z }
    }

    pub fn h(&self) -> i32 {
        self.size.y
    }

    /// The template cell at local `(u, y, v)`.
    pub fn at(&self, u: i32, y: i32, v: i32) -> IVec3 {
        let s = self.size;
        match self.front {
            Facing::North => IVec3::new(u, y, v),
            Facing::East => IVec3::new(s.x - 1 - v, y, u),
            Facing::South => IVec3::new(s.x - 1 - u, y, s.z - 1 - v),
            Facing::West => IVec3::new(v, y, s.z - 1 - u),
        }
    }

    /// A local facing (north = out of the front) in template directions.
    pub fn face(&self, local: Facing) -> Facing {
        (0..turns(self.front)).fold(local, |f, _| f.clockwise())
    }

    /// The local facing of a template facing.
    pub fn local(&self, f: Facing) -> Facing {
        (0..(4 - turns(self.front)) % 4).fold(f, |f, _| f.clockwise())
    }
}

/// The length of a template's face.
pub fn face_len(size: IVec3, f: Facing) -> i32 {
    if f.along_x() { size.z } else { size.x }
}

/// The cell on face `f` at `t` along it (west to east, or north to south)
/// and height `y`.
pub fn face_cell(size: IVec3, f: Facing, t: i32, y: i32) -> IVec3 {
    match f {
        Facing::North => IVec3::new(t, y, 0),
        Facing::South => IVec3::new(t, y, size.z - 1),
        Facing::West => IVec3::new(0, y, t),
        Facing::East => IVec3::new(size.x - 1, y, t),
    }
}

/// `k` positions spread evenly along a face of length `len`, the first one
/// (when `k` is 1) at the middle.
pub fn spread(len: i32, k: usize) -> Vec<i32> {
    if len <= 0 {
        return vec![0; k];
    }
    let k = k as i32;
    (0..k).map(|i| ((2 * i + 1) * len / (2 * k)).clamp(0, len - 1)).collect()
}

/// The connectors of `conns` that satisfy `f`, grouped by template face.
pub fn by_face(conns: &[Conn], f: impl Fn(&Conn) -> bool) -> Vec<(Facing, Vec<&Conn>)> {
    let mut out: Vec<(Facing, Vec<&Conn>)> = Vec::new();
    for c in conns.iter().filter(|c| c.side() && f(c)) {
        let facing = c.facing.unwrap();
        match out.iter_mut().find(|(g, _)| *g == facing) {
            Some((_, v)) => v.push(c),
            None => out.push((facing, vec![c])),
        }
    }
    out
}

/// A small deterministic hash of a string and numbers (template variety).
pub fn mix(name: &str, a: i32, b: i32) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for byte in name.bytes().chain(a.to_le_bytes()).chain(b.to_le_bytes()) {
        h ^= byte as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h ^ (h >> 29)
}

/// Places every connector of `spec` the painter didn't, at a sensible
/// default: side connectors spread along their face at `side_y`, up
/// connectors on free floor cells, down connectors at the bottom centre.
pub fn place_rest(spec: &PieceSpec, d: &mut Draft, done: &[usize], side_y: i32, floor: &mut Vec<IVec3>) {
    let rest: Vec<&Conn> = spec.conns.iter().enumerate().filter(|(i, _)| !done.contains(i)).map(|(_, c)| c).collect();
    let owned: Vec<Conn> = rest.iter().map(|c| (*c).clone()).collect();
    for (f, cs) in by_face(&owned, |_| true) {
        let len = face_len(spec.size, f);
        for (c, t) in cs.iter().zip(spread(len, cs.len())) {
            d.connect(face_cell(spec.size, f, t, side_y.min(spec.size.y - 1)), c);
        }
    }
    let centre = IVec3::new(spec.size.x / 2, 0, spec.size.z / 2);
    for c in owned.iter().filter(|c| !c.side()) {
        if c.down() {
            d.connect(centre, c);
        } else {
            let at = if floor.is_empty() { centre } else { floor.remove(0) };
            d.connect(at, c);
        }
    }
}
