//! Java 26.3 `DriedGhastBlock` (v0.15 Track C): a 10×10×10 husk of a ghast
//! with `hydration` 0–3, `facing` and `waterlogged`. Waterlogged, each
//! scheduled tick (5000 game ticks after a random tick finds none pending)
//! raises the hydration; at 3 the next one removes the block and hatches a
//! ghastling facing the block's way. A dry block loses a level per tick.
//! `Blocks.DRIED_GHAST`: instant to break, no explosion resistance, drops
//! itself when it survives an explosion; not pathfindable.
//!
//! States 12224–12255 (Track C's reservation):
//! `FIRST + ((hydration * 4 + facing) * 2 + waterlogged)` with `Facing::ALL`
//! order. Texture layers 3968–3979, original art.
use super::{
    World,
    block::{Block, Facing, RenderKind},
    shape::{Box16, Boxes},
};
use glam::IVec3;

pub const FIRST: u16 = 12224;
pub const LAST: u16 = FIRST + 31;
/// The item's state: dry, facing south, hydration 0 (Java's default faces
/// north; the placement turns it toward the player either way).
pub const DRIED_GHAST: Block = Block(FIRST);
pub const TEX: u16 = 3968;
pub const TEX_END: u16 = TEX + 12;
const _: () = assert!(FIRST >= 12224 && LAST <= 12255 && TEX >= 3968 && TEX_END <= 4000);
/// `MAX_HYDRATION_LEVEL` and `HYDRATION_TICK_DELAY`.
pub const MAX_HYDRATION: u8 = 3;
pub const HYDRATION_TICK_DELAY: u64 = 5000;

pub const fn owns(id: u16) -> bool {
    id >= FIRST && id <= LAST
}

/// A dried ghast state's hydration, facing and water.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct State {
    pub hydration: u8,
    pub facing: Facing,
    pub waterlogged: bool,
}

pub const fn decode(b: Block) -> Option<State> {
    if !owns(b.0) {
        return None;
    }
    let n = b.0 - FIRST;
    Some(State { hydration: (n / 8) as u8, facing: Facing::ALL[(n / 2 % 4) as usize], waterlogged: n % 2 == 1 })
}

pub const fn block(s: State) -> Block {
    Block(FIRST + ((s.hydration as u16 * 4 + s.facing as u16) * 2 + s.waterlogged as u16))
}

/// The other half of the waterlogged pair, and whether `id` is the wet one.
pub const fn water_pair(id: u16) -> Option<(u16, bool)> {
    if owns(id) { Some((id ^ 1, (id - FIRST) % 2 == 1)) } else { None }
}

/// Side, top and bottom layers per hydration level.
pub const fn registry(id: u16) -> (&'static str, RenderKind, [u16; 6]) {
    let h = (id - FIRST) / 8;
    let base = TEX + h * 3;
    ("dried ghast", RenderKind::Shaped, [base, base, base + 1, base + 2, base, base])
}

/// `Block.column(10.0, 10.0, 0.0, 10.0)`: shape, outline and collision.
pub fn boxes() -> Boxes {
    Boxes::from_box(Box16 { min: [3, 0, 3], max: [13, 10, 13] })
}

/// The ghastling's yaw (VoxelCraft radians) for the block's facing:
/// `Direction.getYRot` (south 0°, west 90°, north 180°, east 270°).
pub fn ghastling_yaw(facing: Facing) -> f32 {
    let yrot: f32 = match facing {
        Facing::South => 0.0,
        Facing::West => 90.0,
        Facing::North => 180.0,
        Facing::East => 270.0,
    };
    (yrot + 90.0).to_radians()
}

/// Original texture: a shrivelled grey-brown skin with closed eyes, which
/// pales and plumps as it hydrates. Layer `base` is the side (the face
/// shows on every side), `base + 1` the top, `base + 2` the bottom; only
/// the middle 10×10 shows.
pub fn pixel(layer: u16, x: usize, y: usize) -> [u8; 4] {
    let i = layer - TEX;
    let (hydration, face) = ((i / 3) as i32, i % 3);
    let (x, y) = (x as i32, y as i32);
    let wrinkle = ((x * 5 + y * 3 + hydration * 7) % 9 < 2 - hydration.min(2) / 2) as i32;
    let tone = 96 + hydration * 34 - wrinkle * 22;
    let c = match face {
        0 if (y == 8 || y == 9) && (x == 5 || x == 6 || x == 9 || x == 10) => [52, 48, 50],
        0 if y == 11 && (6..=9).contains(&x) && hydration >= 2 => [196, 118, 128],
        2 => [tone - 30, tone - 34, tone - 36],
        _ => [tone + 8, tone, tone - 10],
    };
    [c[0].clamp(0, 255) as u8, c[1].clamp(0, 255) as u8, c[2].clamp(0, 255) as u8, 255]
}

impl World {
    /// `DriedGhastBlock.randomTick`: a waterlogged or damp block with no
    /// tick pending schedules one 5000 ticks away.
    pub(super) fn dried_ghast_random_tick(&mut self, p: IVec3, b: Block) {
        let Some(s) = decode(b) else { return };
        // `schedule_redstone` keeps a tick already pending (`hasScheduledTick`).
        if s.waterlogged || s.hydration > 0 {
            self.schedule_redstone(p, HYDRATION_TICK_DELAY, 0);
        }
    }

    /// `DriedGhastBlock.tick`: hydrate in water (hatching at the top
    /// level), dry out otherwise.
    pub(super) fn dried_ghast_tick(&mut self, p: IVec3) {
        let Some(s) = self.get_block(p).and_then(decode) else { return };
        if s.waterlogged {
            if s.hydration < MAX_HYDRATION {
                self.edit(p, block(State { hydration: s.hydration + 1, ..s }), false);
                self.dried_ghast_events.push(Hydration::Grew(p));
            } else {
                // `removeBlock(pos, false)` leaves the water it held.
                self.edit(p, Block::WATER, false);
                self.dried_ghast_events.push(Hydration::Hatched(p, ghastling_yaw(s.facing)));
            }
        } else if s.hydration > 0 {
            self.edit(p, block(State { hydration: s.hydration - 1, ..s }), false);
        }
    }
}

/// What a dried ghast did this tick, for the entity side to play and spawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hydration {
    /// `DRIED_GHAST_TRANSITION`: one level wetter.
    Grew(IVec3),
    /// `spawnGhastling`: the block is gone and a ghastling appears here,
    /// with this yaw.
    Hatched(IVec3, f32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_round_trip_and_pair_with_water() {
        let mut seen = std::collections::HashSet::new();
        for hydration in 0..=MAX_HYDRATION {
            for facing in Facing::ALL {
                for waterlogged in [false, true] {
                    let s = State { hydration, facing, waterlogged };
                    let b = block(s);
                    assert!(owns(b.0));
                    assert_eq!(decode(b), Some(s));
                    assert!(seen.insert(b.0));
                    let (other, wet) = water_pair(b.0).unwrap();
                    assert_eq!(wet, waterlogged);
                    assert_eq!(decode(Block(other)), Some(State { waterlogged: !waterlogged, ..s }));
                }
            }
        }
        // hydration × facing × waterlogged fills the 32 reserved states.
        assert_eq!(seen.len(), 32);
        assert_eq!((FIRST, LAST), (12224, 12255));
    }

    #[test]
    fn the_shape_is_a_ten_pixel_column() {
        let b = boxes();
        assert_eq!(b.as_slice(), [Box16 { min: [3, 0, 3], max: [13, 10, 13] }]);
    }
}
