//! Creaking heart states and block-entity storage. Java 26.3 reference:
//! CreakingHeartBlock/BlockEntity; see the dated verification record.
use super::{
    World,
    block::{Block, RenderKind},
    chunk::{CHUNK_SIZE, CHUNK_SIZE_I, ChunkData},
};
use glam::IVec3;
use serde_json::{Value, json};
pub const FIRST: u16 = 14592;
pub const HEART: Block = Block(FIRST);
pub const CLUMP_FIRST: u16 = 14656;
pub const RESIN_CLUMP: Block = Block(CLUMP_FIRST + 1);
pub const RESIN_BLOCK: Block = Block(14784);
pub const RESIN_BRICKS: Block = Block(14785);
pub const CHISELED: Block = Block(14786);
pub const STAIRS: Block = Block(14792);
pub const SLAB: Block = Block(14872);
pub const WALL: Block = Block(14880);
pub const SHAPES: super::slab_stair_states::Family =
    super::slab_stair_states::Family { base: RESIN_BRICKS, stairs: STAIRS, slab: SLAB };
pub const fn wall_local(id: u16) -> Option<u16> {
    if id >= WALL.0 && id < WALL.0 + 4 { Some(id - WALL.0) } else { None }
}
pub const TEX: u16 = 3360;
pub const TEX_END: u16 = TEX + 13;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    Uprooted,
    Dormant,
    Awake,
}
pub const fn block(axis: u16, activity: Activity, natural: bool) -> Block {
    Block(FIRST + axis * 6 + activity as u16 * 2 + natural as u16)
}
pub fn heart(b: Block) -> Option<(u16, Activity, bool)> {
    let i = b.0.checked_sub(FIRST)?;
    if i >= 18 {
        return None;
    }
    Some((i / 6, [Activity::Uprooted, Activity::Dormant, Activity::Awake][(i % 6 / 2) as usize], i % 2 != 0))
}
pub const fn owns(id: u16) -> bool {
    (id >= FIRST && id < FIRST + 18)
        || (id >= RESIN_BLOCK.0 && id <= CHISELED.0)
        || (id >= STAIRS.0 && id < STAIRS.0 + 80)
        || (id >= SLAB.0 && id < SLAB.0 + 5)
        || (id >= WALL.0 && id < WALL.0 + 4)
}
pub const fn registry(id: u16) -> (&'static str, RenderKind, [u16; 6]) {
    if id >= FIRST && id < FIRST + 18 {
        let axis = (id - FIRST) / 6;
        let t = TEX + (id - FIRST) % 6 / 2 * 3;
        let tex = match axis {
            1 => [t + 1, t + 1, t + 2, t + 2, t + 2, t + 2],
            2 => [t + 2, t + 2, t + 2, t + 2, t + 1, t + 1],
            _ => [t, t, t + 1, t + 1, t, t],
        };
        return ("creaking heart", RenderKind::Opaque, tex);
    }
    if id >= STAIRS.0 && id < STAIRS.0 + 80 {
        return ("resin brick stairs", RenderKind::Shaped, [TEX + 11; 6]);
    }
    if id >= SLAB.0 && id < SLAB.0 + 5 {
        return (
            "resin brick slab",
            if id == SLAB.0 + 4 {
                RenderKind::Opaque
            } else if id <= SLAB.0 + 1 {
                RenderKind::Cutout
            } else {
                RenderKind::Shaped
            },
            [TEX + 11; 6],
        );
    }
    if id >= WALL.0 && id < WALL.0 + 4 {
        return ("resin brick wall", RenderKind::Shaped, [TEX + 11; 6]);
    }
    match id {
        14784 => ("block of resin", RenderKind::Opaque, [TEX + 10; 6]),
        14785 => ("resin bricks", RenderKind::Opaque, [TEX + 11; 6]),
        14786 => ("chiseled resin bricks", RenderKind::Opaque, [TEX + 12; 6]),
        _ => ("unknown", RenderKind::Invisible, [0; 6]),
    }
}
pub fn palette_ids() -> impl Iterator<Item = u16> {
    [HEART.0, RESIN_CLUMP.0, RESIN_BLOCK.0, RESIN_BRICKS.0, CHISELED.0, STAIRS.0, SLAB.0, WALL.0].into_iter()
}
pub fn placed(b: Block, normal: IVec3) -> Block {
    heart(b).map_or(b, |(_, a, n)| {
        block(
            if normal.x != 0 {
                1
            } else if normal.z != 0 {
                2
            } else {
                0
            },
            a,
            n,
        )
    })
}
pub fn pale_log(b: Block) -> bool {
    b.base() == super::overworld_blocks::PALE_OAK_LOG
        || matches!(b.name(), "pale oak wood" | "stripped pale oak log" | "stripped pale oak wood")
}
pub fn required_logs(b: Block, p: IVec3, get: impl Fn(IVec3) -> Option<Block>) -> bool {
    let Some((axis, _, _)) = heart(b) else { return false };
    let d = [IVec3::Y, IVec3::X, IVec3::Z][axis as usize];
    [p + d, p - d].into_iter().all(|q| get(q).is_some_and(|n| pale_log(n) && super::wood::axis(n) == Some(axis)))
}
#[derive(Clone, Debug, Default)]
pub struct Heart {
    pub protector: Option<u32>,
    pub signal: u8,
    pub timer: f64,
    pub emitter: f64,
    pub grace: f64,
}
impl World {
    pub(super) fn track_creaking_heart(&mut self, p: IVec3, b: Block) {
        if heart(b).is_some() {
            self.creaking_active.insert(p.to_array());
            self.creaking_hearts.entry(p.to_array()).or_default();
        } else {
            self.creaking_hearts.remove(&p.to_array());
            self.creaking_active.remove(&p.to_array());
        }
    }
    pub(super) fn register_creaking_hearts(&mut self, pos: IVec3, data: &ChunkData) {
        self.creaking_active.retain(|p| super::chunk::chunk_of(IVec3::from_array(*p)) != pos);
        let any = match data {
            ChunkData::Uniform(b) => heart(*b).is_some(),
            ChunkData::Bytes(_) => false,
            ChunkData::Paletted { palette, len, .. } => palette[..*len as usize].iter().any(|&b| heart(b).is_some()),
            ChunkData::Dense(v) => v.iter().any(|&b| heart(b).is_some()),
        };
        if !any {
            return;
        }
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    let b = data.get(x, y, z);
                    if heart(b).is_some() {
                        self.creaking_active
                            .insert((pos * CHUNK_SIZE_I + IVec3::new(x as i32, y as i32, z as i32)).to_array());
                        self.creaking_hearts
                            .entry((pos * CHUNK_SIZE_I + IVec3::new(x as i32, y as i32, z as i32)).to_array())
                            .or_default();
                    }
                }
            }
        }
    }
    pub fn creaking_hearts_to_string(&self) -> String {
        json!(self.creaking_hearts.iter().map(|(p, h)| json!([p, h.protector])).collect::<Vec<_>>()).to_string()
    }
    pub fn load_creaking_hearts(&mut self, text: &str) {
        let Ok(v) = serde_json::from_str::<Value>(text) else { return };
        if let Some(a) = v.as_array() {
            for v in a {
                let Some(p) = v[0].as_array().filter(|a| a.len() == 3) else { continue };
                let coords: Option<Vec<_>> = p.iter().map(|n| n.as_i64().and_then(|n| i32::try_from(n).ok())).collect();
                let Some(p) = coords else { continue };
                self.creaking_hearts.insert(
                    [p[0], p[1], p[2]],
                    Heart {
                        protector: v[1].as_u64().and_then(|n| u32::try_from(n).ok()),
                        grace: 1.5,
                        ..Default::default()
                    },
                );
            }
        }
    }
}
/// Procedural bark, glowing fissures, resin droplets and masonry, all original.
pub fn pixel(layer: u16, x: usize, y: usize) -> [u8; 4] {
    let n = ((x * 17 + y * 31 + x * y * 13) % 19) as u8;
    let i = layer - TEX;
    if i == 9 {
        return if (x * 7 + y * 11) % 17 < 8 { [212 + n, 94 + n, 22, 255] } else { [0; 4] };
    }
    if i >= 10 {
        let mortar = i >= 11 && (y.is_multiple_of(8) || (x + if y % 16 < 8 { 0 } else { 8 }).is_multiple_of(16));
        let cut =
            i == 12 && ((3..=12).contains(&x) && (3..=12).contains(&y)) && (x == 3 || x == 12 || y == 3 || y == 12);
        return if mortar || cut { [124, 59, 24, 255] } else { [194 + n, 84 + n, 24 + n / 2, 255] };
    }
    let (x, y) = if i % 3 == 2 { (y, x) } else { (x, y) };
    let crack = (x * 3 + y / 4) % 11 == 0;
    if i >= 3 && crack {
        if i >= 6 { [245, 132 + n, 32, 255] } else { [129, 76, 40, 255] }
    } else {
        let a = if i % 3 == 1 { (x.abs_diff(8).max(y.abs_diff(8)) % 3) * 10 } else { ((x * 5 + y / 5) % 7) * 5 };
        [102 + n + a as u8, 101 + n + a as u8, 88 + n + a as u8, 255]
    }
}
