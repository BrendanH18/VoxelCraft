//! Tree and huge plant shapes, shared by world generation and saplings.
//!
//! Each shape hands its blocks to `put` relative to `ground` (the soil
//! under the trunk), leaves before logs, so overlapping trees resolve the
//! same way in any chunk. `v` picks the variant. Every shape stays within
//! [`REACH`] blocks of its trunk and [`TOP`] blocks above the ground.

use glam::IVec3;

use super::block::{Block, Wood};
use super::overworld_blocks as ob;

/// How far leaves and roots can extend from the trunk.
pub const REACH: i32 = 7;
/// How far above the ground the tallest tree reaches.
pub const TOP: i32 = 32;

pub type Put<'a> = &'a mut dyn FnMut(IVec3, Block);

const DIRS: [IVec3; 4] = [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z];

/// A tree of `wood` grown from a sapling: blocks handed to `put`, leaves
/// before logs. `v` picks the variant.
pub fn tree(wood: Wood, ground: IVec3, v: u32, put: Put) {
    match wood {
        // Java's oak sapling grows a fancy oak one time in ten.
        Wood::Oak if v.is_multiple_of(10) => fancy_oak(ground, v, put),
        Wood::Oak => oak(ground, v, put),
        Wood::Spruce => spruce(ground, v, put),
        Wood::Birch => birch(ground, v, put),
        Wood::Jungle => jungle(ground, v, put),
        Wood::Acacia => acacia(ground, v, put),
        Wood::DarkOak => dark_oak(ground, v, put),
        Wood::Mangrove => mangrove(ground, v, put),
        Wood::Cherry => cherry(ground, v, put),
        Wood::PaleOak => pale_oak(ground, v, put),
    }
}

/// The classic round-topped tree: a trunk of `height` with two wide
/// leaf layers (radius `wide`) under two narrow ones.
fn round_tree(ground: IVec3, height: i32, wood: Wood, wide: i32, v: u32, put: Put) {
    let top = ground.y + height;
    for dy in -2..=1 {
        let r: i32 = if dy >= 0 { 1 } else { wide };
        for dz in -r..=r {
            for dx in -r..=r {
                // Trim corners randomly for a less boxy canopy.
                let corner = dx.abs() == r && dz.abs() == r;
                if corner && (dy == 1 || r > 2 || (v >> ((dx + dz * 3 + dy * 7) & 15)) & 1 == 0) {
                    continue;
                }
                put(IVec3::new(ground.x + dx, top + dy, ground.z + dz), wood.leaves());
            }
        }
    }
    for y in ground.y + 1..top {
        put(IVec3::new(ground.x, y, ground.z), wood.log());
    }
}

pub fn oak(ground: IVec3, v: u32, put: Put) {
    round_tree(ground, 4 + (v % 3) as i32, Wood::Oak, 2, v, put);
}

/// Taller and slimmer than an oak, with white bark.
pub fn birch(ground: IVec3, v: u32, put: Put) {
    round_tree(ground, 5 + (v % 3) as i32, Wood::Birch, 2, v, put);
}

/// A squat oak with a broad, drooping canopy.
pub fn swamp_oak(ground: IVec3, v: u32, put: Put) {
    round_tree(ground, 5 + (v % 3) as i32, Wood::Oak, 3, v, put);
}

/// A tall, thin jungle tree (what a single jungle sapling grows into).
pub fn jungle(ground: IVec3, v: u32, put: Put) {
    round_tree(ground, 7 + (v % 5) as i32, Wood::Jungle, 2, v, put);
}

/// A jungle floor shrub: one log under a mound of oak leaves.
pub fn jungle_bush(ground: IVec3, v: u32, put: Put) {
    for dy in 1..=2i32 {
        let r = 3 - dy;
        for dz in -r..=r {
            for dx in -r..=r {
                if dx.abs() == r && dz.abs() == r && (v >> ((dx + dz * 5) & 15)) & 1 == 0 {
                    continue;
                }
                put(ground + IVec3::new(dx, dy, dz), Block::LEAVES);
            }
        }
    }
    put(ground + IVec3::Y, Block::JUNGLE_LOG);
}

/// A giant jungle tree: a 2x2 trunk up to 28 blocks tall under a wide
/// dome, with a couple of leafy side branches.
pub fn mega_jungle(ground: IVec3, v: u32, put: Put) {
    let height = 18 + (v % 10) as i32;
    let top = ground.y + height;
    let leaves = Wood::Jungle.leaves();
    for (dy, r) in [(-2, 4.5f32), (-1, 4.2), (0, 3.4), (1, 2.3)] {
        for dz in -4..=5 {
            for dx in -4..=5 {
                let (cx, cz) = (dx as f32 - 0.5, dz as f32 - 0.5);
                if cx * cx + cz * cz <= r * r {
                    put(IVec3::new(ground.x + dx, top + dy, ground.z + dz), leaves);
                }
            }
        }
    }
    let mut logs = Vec::new();
    for i in 0..2u32 {
        let y = top - 6 - i as i32 * 5 - ((v >> (4 + i * 2)) & 3) as i32;
        let dir = DIRS[((v >> (10 + i * 2)) & 3) as usize];
        // Start from the trunk block on that side.
        let start = IVec3::new(ground.x + (dir.x > 0) as i32, y, ground.z + (dir.z > 0) as i32);
        let end = start + dir * 2 + IVec3::Y;
        logs.extend([start + dir, start + dir * 2, end]);
        for dz in -1..=1 {
            for dx in -1..=1 {
                for dy in 0..=1 {
                    if dy == 1 && dx != 0 && dz != 0 {
                        continue;
                    }
                    put(end + IVec3::new(dx, dy, dz), leaves);
                }
            }
        }
    }
    for y in ground.y - 1..top {
        for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            put(IVec3::new(ground.x + dx, y, ground.z + dz), Wood::Jungle.log());
        }
    }
    for p in logs {
        put(p, Wood::Jungle.log());
    }
}

/// A savanna acacia: a trunk that leans off to one side and ends in a
/// flat, umbrella-like canopy, sometimes with a second smaller branch.
pub fn acacia(ground: IVec3, v: u32, put: Put) {
    let (log, leaves) = (Wood::Acacia.log(), Wood::Acacia.leaves());
    let rise = 2 + (v % 3) as i32;
    let dir = DIRS[((v >> 3) & 3) as usize];
    let lean = 1 + ((v >> 5) & 1) as i32;
    let mut logs = Vec::new();
    let mut p = ground;
    for _ in 0..rise {
        p += IVec3::Y;
        logs.push(p);
    }
    let fork = p;
    for _ in 0..lean {
        p += dir + IVec3::Y;
        logs.push(p);
    }
    let canopy = |c: IVec3, wide: i32, put: Put| {
        for dz in -wide..=wide {
            for dx in -wide..=wide {
                if dx.abs() + dz.abs() <= wide + 1 && !(dx.abs() == wide && dz.abs() == wide) {
                    put(c + IVec3::new(dx, 1, dz), leaves);
                }
                if dx.abs() <= 1 && dz.abs() <= 1 && wide > 2 {
                    put(c + IVec3::new(dx, 2, dz), leaves);
                }
            }
        }
    };
    canopy(p, 3, put);
    if (v >> 7) & 1 == 1 {
        // A second branch the other way, with its own small canopy.
        let other = -dir;
        let mut q = fork - IVec3::Y;
        for _ in 0..2 {
            q += other + IVec3::Y;
            logs.push(q);
        }
        canopy(q, 2, put);
    }
    for p in logs {
        put(p, log);
    }
}

/// A spruce standing on `ground`: a cone of alternating wide and narrow
/// leaf rings.
pub fn spruce(ground: IVec3, v: u32, put: Put) {
    let height = 6 + (v % 4) as i32;
    let top = ground.y + height;
    let leaves = Wood::Spruce.leaves();
    put(IVec3::new(ground.x, top + 1, ground.z), leaves);
    for i in 0..height - 2 {
        let y = top - i;
        let r = match i {
            0 => 0,
            _ if i % 2 == 1 => 1,
            _ => (i / 2).min(3) - (i / 6),
        };
        for dz in -r..=r {
            for dx in -r..=r {
                if r > 1 && dx.abs() == r && dz.abs() == r {
                    continue;
                }
                put(IVec3::new(ground.x + dx, y, ground.z + dz), leaves);
            }
        }
    }
    for y in ground.y + 1..top {
        put(IVec3::new(ground.x, y, ground.z), Wood::Spruce.log());
    }
}

pub fn cactus(ground: IVec3, v: u32, put: Put) {
    for y in 1..=1 + (v % 3) as i32 {
        put(ground + IVec3::new(0, y, 0), Block::CACTUS);
    }
}

/// A leafy blob: an ellipsoid of `radius` (and half `height`) around
/// `centre`, with ragged edges picked by `v`.
fn blob(centre: IVec3, radius: f32, height: f32, leaves: Block, v: u32, put: Put) {
    let r = radius.ceil() as i32;
    let h = height.ceil() as i32;
    for dy in -h..=h {
        for dz in -r..=r {
            for dx in -r..=r {
                let d = (dx * dx + dz * dz) as f32 / (radius * radius) + (dy * dy) as f32 / (height * height).max(0.5);
                let edge = (v.wrapping_mul(0x9E37_79B9) >> ((dx * 3 + dz * 5 + dy * 7) & 31)) & 3 == 0;
                if d <= 1.0 && !(d > 0.7 && edge) {
                    put(centre + IVec3::new(dx, dy, dz), leaves);
                }
            }
        }
    }
}

/// A straight run of logs from `from` to `to` (inclusive), Bresenham-style.
fn branch(from: IVec3, to: IVec3, log: Block, put: Put) {
    let d = to - from;
    let steps = d.abs().max_element().max(1);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let p = from.as_vec3() + d.as_vec3() * t;
        put(p.round().as_ivec3(), log);
    }
}

/// Java's fancy (big) oak: a tall trunk with branches ending in round leaf clusters.
pub fn fancy_oak(ground: IVec3, v: u32, put: Put) {
    let height = 7 + (v % 6) as i32;
    let top = ground + IVec3::Y * height;
    let mut logs = Vec::new();
    let mut clusters = vec![top];
    for i in 0..3 + (v >> 4) % 3 {
        let y = height / 2 + ((v >> (6 + i * 3)) % (height as u32 / 2)) as i32;
        let angle = ((v >> (i * 5)) % 360) as f32 * std::f32::consts::PI / 180.0 + i as f32 * 2.1;
        let len = 2.0 + ((v >> (i * 4 + 1)) % 3) as f32;
        let end = ground
            + IVec3::new((angle.cos() * len).round() as i32, y + len as i32 / 2, (angle.sin() * len).round() as i32);
        logs.push((ground + IVec3::Y * y, end));
        clusters.push(end);
    }
    for c in &clusters {
        blob(*c + IVec3::Y, 2.6, 1.8, Block::LEAVES, v, put);
    }
    for (a, b) in logs {
        branch(a, b, Block::LOG, put);
    }
    for y in 1..height {
        put(ground + IVec3::Y * y, Block::LOG);
    }
}

/// A tall birch (old growth birch forests): 10 to 14 logs.
pub fn tall_birch(ground: IVec3, v: u32, put: Put) {
    round_tree(ground, 10 + (v % 5) as i32, Wood::Birch, 2, v, put);
}

/// The 2x2 trunk of a dark or pale oak, leaning a little near the top.
fn thick_trunk(ground: IVec3, height: i32, log: Block, v: u32, put: Put) -> IVec3 {
    let lean = DIRS[(v >> 3) as usize % 4];
    let bend_at = height - 2 - ((v >> 5) % 2) as i32;
    let mut top = ground;
    for y in 1..=height {
        let shift = if y > bend_at && (v >> 7) & 1 == 1 { lean } else { IVec3::ZERO };
        for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            put(ground + shift + IVec3::new(dx, y, dz), log);
        }
        top = ground + shift + IVec3::Y * y;
    }
    top
}

/// A dark oak: a 2x2 trunk under a wide, flat, dense canopy.
pub fn dark_oak(ground: IVec3, v: u32, put: Put) {
    canopy_oak(ground, v, Block::DARK_OAK_LOG, ob::DARK_OAK_LEAVES, false, put);
}

/// A pale oak: built like a dark oak, with pale leaves and moss hanging under them.
pub fn pale_oak(ground: IVec3, v: u32, put: Put) {
    canopy_oak(ground, v, ob::PALE_OAK_LOG, ob::PALE_OAK_LEAVES, true, put);
}

fn canopy_oak(ground: IVec3, v: u32, log: Block, leaves: Block, moss: bool, put: Put) {
    let height = 6 + (v % 3) as i32 + ((v >> 2) % 2) as i32;
    // Leaves first (logs then overwrite them), then the trunk.
    let mut logs = Vec::new();
    thick_trunk(ground, height, log, v, &mut |p, b| logs.push((p, b)));
    let top = logs.last().map_or(ground + IVec3::Y * height, |&(p, _)| p);
    let centre = IVec3::new(top.x, top.y, top.z) - IVec3::new(1, 0, 1);
    let mut placed = Vec::new();
    for dy in -1..=1 {
        let r = if dy == 1 { 2 } else { 3 + (dy == 0) as i32 };
        for dz in -r..=r + 1 {
            for dx in -r..=r + 1 {
                let (cx, cz) = (dx as f32 - 0.5, dz as f32 - 0.5);
                let d = cx * cx + cz * cz;
                let edge = (v >> ((dx * 7 + dz * 3 + dy) & 31)) & 1 == 0;
                if d <= (r as f32 + 0.6).powi(2) && !(d > (r as f32 - 0.4).powi(2) && edge) {
                    let p = centre + IVec3::new(dx + 1, dy, dz + 1);
                    put(p, leaves);
                    placed.push(p);
                }
            }
        }
    }
    if moss {
        for p in &placed {
            let roll = super::noise::hash3(p.x, p.y, p.z, u64::from(v) ^ 0x4D05) as u32;
            if p.y == centre.y - 1 && roll.is_multiple_of(7) {
                let len = 1 + (roll >> 8) % 3;
                for d in 1..=len as i32 {
                    let tip = d == len as i32;
                    put(*p - IVec3::Y * d, if tip { ob::PALE_HANGING_MOSS_TIP } else { ob::PALE_HANGING_MOSS });
                }
            }
        }
    }
    for (p, b) in logs {
        put(p, b);
    }
}

/// A mangrove: a trunk raised on arching roots, a loose leafy crown and
/// propagules hanging under it. Roots reach down into mud or water.
pub fn mangrove(ground: IVec3, v: u32, put: Put) {
    let lift = 1 + (v % 3) as i32;
    let height = 4 + ((v >> 2) % 5) as i32;
    let base = ground + IVec3::Y * lift;
    let top = base + IVec3::Y * height;
    blob(top, 2.8, 2.0, ob::MANGROVE_LEAVES, v, put);
    blob(
        top + IVec3::new(((v >> 5) % 3) as i32 - 1, -1, ((v >> 7) % 3) as i32 - 1),
        2.2,
        1.4,
        ob::MANGROVE_LEAVES,
        v >> 3,
        put,
    );
    // Propagules dangle from the bottom of the crown.
    for i in 0..3u32 {
        let off = IVec3::new(((v >> (9 + i * 2)) % 5) as i32 - 2, -3, ((v >> (10 + i * 2)) % 5) as i32 - 2);
        put(top + off, ob::MANGROVE_PROPAGULE);
    }
    // Roots: arches out from the trunk base, down to the ground and below.
    for (i, d) in DIRS.into_iter().enumerate() {
        if (v >> (16 + i)) & 1 == 0 && i > 1 {
            continue;
        }
        let out = 1 + ((v >> (20 + i * 2)) % 2) as i32;
        let mut p = base;
        for _ in 0..out {
            p += d;
            put(p, ob::MANGROVE_ROOTS);
        }
        while p.y > ground.y - 2 {
            p -= IVec3::Y;
            put(p, ob::MANGROVE_ROOTS);
        }
    }
    for y in 1..lift + height {
        put(ground + IVec3::Y * y, Block::MANGROVE_LOG);
    }
}

/// A cherry: a short trunk splitting into two or three branches, each
/// under a broad pink canopy.
pub fn cherry(ground: IVec3, v: u32, put: Put) {
    let height = 4 + (v % 3) as i32;
    let top = ground + IVec3::Y * height;
    let branches = 2 + (v >> 3) % 2;
    let mut ends = Vec::new();
    for i in 0..branches {
        let d = DIRS[((v >> (5 + i * 2)) as usize + i as usize) % 4];
        let len = 2 + ((v >> (9 + i)) % 2) as i32;
        let end = top - IVec3::Y * (1 + i as i32 % 2) + d * len + IVec3::Y * (len + 1);
        ends.push((top - IVec3::Y * (1 + i as i32 % 2), end));
    }
    for &(_, end) in &ends {
        blob(end + IVec3::Y, 3.6, 1.9, ob::CHERRY_LEAVES, v, put);
        // Leaves trail down from the canopy's rim.
        for k in 0..4 {
            let off = IVec3::new(((v >> k) % 7) as i32 - 3, -1, ((v >> (k + 3)) % 7) as i32 - 3);
            put(end + off, ob::CHERRY_LEAVES);
        }
    }
    for (from, end) in ends {
        branch(from, end, Block::CHERRY_LOG, put);
    }
    for y in 1..height {
        put(ground + IVec3::Y * y, Block::CHERRY_LOG);
    }
}

/// An old growth spruce or pine: a 2x2 trunk. Spruces carry a full cone of
/// leaves; pines only a tuft near the top.
pub fn mega_spruce(ground: IVec3, v: u32, pine: bool, put: Put) {
    let height = 13 + (v % 14) as i32;
    let top = ground + IVec3::Y * height;
    let leafy = if pine { 3 + (v >> 4) as i32 % 3 } else { height * 2 / 3 };
    for i in 0..leafy {
        let y = top.y + 1 - i;
        let r = ((i as f32 * 0.35) + 0.6).min(4.5) + if i % 3 == 2 { -0.6 } else { 0.0 };
        let ri = r.ceil() as i32;
        for dz in -ri..=ri + 1 {
            for dx in -ri..=ri + 1 {
                let (cx, cz) = (dx as f32 - 0.5, dz as f32 - 0.5);
                if cx * cx + cz * cz <= r * r {
                    put(IVec3::new(ground.x + dx, y, ground.z + dz), Block::SPRUCE_LEAVES);
                }
            }
        }
    }
    for y in 1..height {
        for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            put(ground + IVec3::new(dx, y, dz), Block::SPRUCE_LOG);
        }
    }
}

/// A pine: a spruce with leaves only around its upper half.
pub fn pine(ground: IVec3, v: u32, put: Put) {
    let height = 7 + (v % 5) as i32;
    let top = ground.y + height;
    put(IVec3::new(ground.x, top + 1, ground.z), Block::SPRUCE_LEAVES);
    for i in 0..4 {
        let r: i32 = [0, 1, 2, 1][i as usize];
        for dz in -r..=r {
            for dx in -r..=r {
                if r == 2 && dx.abs() == 2 && dz.abs() == 2 {
                    continue;
                }
                put(IVec3::new(ground.x + dx, top - i, ground.z + dz), Block::SPRUCE_LEAVES);
            }
        }
    }
    for y in ground.y + 1..top {
        put(IVec3::new(ground.x, y, ground.z), Block::SPRUCE_LOG);
    }
}

/// An azalea tree: a short bending oak trunk under a loose crown of
/// azalea and flowering azalea leaves.
pub fn azalea_tree(ground: IVec3, v: u32, put: Put) {
    let height = 3 + (v % 2) as i32;
    let lean = DIRS[(v >> 2) as usize % 4];
    let mut top = ground;
    let mut logs = Vec::new();
    for y in 1..=height {
        top = ground + IVec3::Y * y + if y > 2 { lean } else { IVec3::ZERO };
        logs.push(top);
    }
    let r = 2;
    for dy in 0..=2 {
        for dz in -r..=r {
            for dx in -r..=r {
                if dx * dx + dz * dz + dy * dy > 6 + (v >> ((dx + dz * 5 + dy * 9) & 31) & 1) as i32 {
                    continue;
                }
                let flowering = (v.rotate_left((dx * 5 + dz * 3 + dy) as u32 & 31)).is_multiple_of(4);
                put(
                    top + IVec3::new(dx, dy, dz),
                    if flowering { ob::FLOWERING_AZALEA_LEAVES } else { ob::AZALEA_LEAVES },
                );
            }
        }
    }
    for p in logs {
        put(p, Block::LOG);
    }
}

/// Java's huge mushrooms: a red dome or a broad flat brown cap on a stem.
pub fn huge_mushroom(ground: IVec3, v: u32, red: bool, put: Put) {
    let height = 4 + (v % 3) as i32 + if red { 0 } else { 1 };
    let top = ground + IVec3::Y * height;
    if red {
        for dy in -3..=0 {
            let r: i32 = if dy == 0 { 1 } else { 2 };
            for dz in -r..=r {
                for dx in -r..=r {
                    let side = dx.abs() == r || dz.abs() == r || dy == 0;
                    if side && !(dx.abs() == r && dz.abs() == r && dy != 0) {
                        put(top + IVec3::new(dx, dy, dz), ob::RED_MUSHROOM_BLOCK);
                    }
                }
            }
        }
    } else {
        for dz in -3i32..=3 {
            for dx in -3i32..=3 {
                if dx.abs() == 3 && dz.abs() == 3 {
                    continue;
                }
                put(top + IVec3::new(dx, 0, dz), ob::BROWN_MUSHROOM_BLOCK);
            }
        }
    }
    for y in 1..height + red as i32 - 1 {
        put(ground + IVec3::Y * y, ob::MUSHROOM_STEM);
    }
}

/// Swamp oaks and jungle trees wear vines: hang them from leaves facing
/// open air (`occupied` tells which cells already hold the tree).
pub fn hang_vines(blocks: &[(IVec3, Block)], v: u32, put: Put) {
    let occupied: rustc_hash::FxHashSet<IVec3> = blocks.iter().map(|&(p, _)| p).collect();
    for &(p, b) in blocks {
        if !b.is_leaves() && !b.is_log() {
            continue;
        }
        for (k, f) in super::block::Facing::ALL.into_iter().enumerate() {
            let q = p + f.offset();
            let roll = super::noise::hash3(q.x, q.y, q.z, u64::from(v) ^ (k as u64) << 40) as u32;
            if occupied.contains(&q) || !roll.is_multiple_of(5) {
                continue;
            }
            let len = 1 + (roll >> 8) % if b.is_log() { 2 } else { 4 };
            for d in 0..len as i32 {
                if occupied.contains(&(q - IVec3::Y * d)) {
                    break;
                }
                put(q - IVec3::Y * d, ob::vine(f.opposite()));
            }
        }
    }
}

/// Cocoa pods on the sides of a jungle trunk.
pub fn cocoa_pods(ground: IVec3, height: i32, v: u32, put: Put) {
    for y in 2..height.min(6) {
        for (k, f) in super::block::Facing::ALL.into_iter().enumerate() {
            if (v >> ((y as u32 * 4 + k as u32) % 31)).is_multiple_of(7) {
                put(ground + IVec3::Y * y + f.offset(), ob::cocoa(((v >> 3) % 3) as u8, f.opposite()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tree_has_a_trunk_of_its_wood_under_its_leaves() {
        for wood in Wood::ALL {
            for v in 0..20u32 {
                let mut blocks = Vec::new();
                tree(wood, IVec3::ZERO, v.wrapping_mul(0x9E37_79B9), &mut |p, b| blocks.push((p, b)));
                assert!(blocks.contains(&(IVec3::Y, wood.log())), "{wood:?} trunk starts on the ground");
                assert!(blocks.iter().any(|&(_, b)| b == wood.leaves()), "{wood:?} has leaves");
                for &(p, _) in &blocks {
                    assert!(p.x.abs() <= REACH && p.z.abs() <= REACH && p.y <= TOP, "{wood:?} {p}");
                }
            }
        }
        // Worldgen-only trees also stay within reach.
        for v in 0..50u32 {
            let v = v.wrapping_mul(0x9E37_79B9);
            let shapes: [fn(IVec3, u32, Put); 9] =
                [mega_jungle, jungle_bush, swamp_oak, acacia, fancy_oak, tall_birch, pine, azalea_tree, cactus];
            for grow in shapes {
                let mut blocks = Vec::new();
                grow(IVec3::ZERO, v, &mut |p, b| blocks.push((p, b)));
                assert!(blocks.iter().all(|&(p, _)| p.x.abs() <= REACH && p.z.abs() <= REACH && p.y <= TOP));
            }
            for flag in [false, true] {
                let mut blocks = Vec::new();
                mega_spruce(IVec3::ZERO, v, flag, &mut |p, b| blocks.push((p, b)));
                huge_mushroom(IVec3::ZERO, v, flag, &mut |p, b| blocks.push((p, b)));
                assert!(blocks.iter().all(|&(p, _)| p.x.abs() <= REACH && p.z.abs() <= REACH && p.y <= TOP));
            }
        }
    }

    #[test]
    fn vines_hang_off_leaves_into_open_air() {
        let mut tree_blocks = Vec::new();
        swamp_oak(IVec3::ZERO, 7, &mut |p, b| tree_blocks.push((p, b)));
        let mut vines = Vec::new();
        hang_vines(&tree_blocks, 12345, &mut |p, b| vines.push((p, b)));
        assert!(!vines.is_empty());
        for (p, b) in vines {
            assert!(ob::vine_wall(b).is_some());
            assert!(!tree_blocks.iter().any(|&(q, _)| q == p));
        }
    }
}
