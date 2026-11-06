//! SnifferEggBlock: three saved hatch levels and persisted scheduled ticks.
use super::{
    World,
    block::{Block, RenderKind},
    shape::{Box16, Boxes},
};
use glam::IVec3;
pub const EGG: Block = Block(15120);
pub const LAST: u16 = 15122;
pub const TEX: u16 = super::sniffer_plants::t::BASE + 16;
pub const TEXTURE_END: u16 = TEX + 3;
pub const fn owns(id: u16) -> bool {
    id >= EGG.0 && id <= LAST
}
pub const fn registry(id: u16) -> (&'static str, RenderKind, [u16; 6]) {
    ("sniffer egg", RenderKind::Shaped, [TEX + id - EGG.0; 6])
}
pub fn boxes() -> Boxes {
    Boxes::from_box(Box16 { min: [1, 0, 2], max: [15, 16, 14] })
}
pub fn pixel(layer: u16, x: usize, y: usize) -> [u8; 4] {
    let crack = layer - TEX;
    let c = if (crack >= 1 && x == 5 + y % 3) || (crack >= 2 && y == 6 + x % 5) {
        [48, 37, 31]
    } else if (x * 3 + y * 5) % 17 < 5 {
        [67, 111, 59]
    } else {
        [166, 126, 84]
    };
    [c[0], c[1], c[2], 255]
}
/// Java level event 3009 (`ParticleUtils.spawnParticlesOnBlockFaces` with
/// `UniformInt.of(3, 6)`): shell specks on every face of a boosted egg.
const FACES: [IVec3; 6] = [IVec3::NEG_Y, IVec3::Y, IVec3::NEG_Z, IVec3::Z, IVec3::NEG_X, IVec3::X];
impl World {
    pub(super) fn track_sniffer_egg(&mut self, p: IVec3, old: Block, new: Block) {
        if old != new && owns(new.0) {
            // `SnifferEggBlock.onPlace` runs for every hatch-level change.
            if self.schedule_sniffer_egg(p) {
                // A positional count keeps visuals off the gameplay random stream.
                for (i, face) in FACES.into_iter().enumerate() {
                    let n = 3 + ((p.x.wrapping_mul(31) ^ p.y.wrapping_mul(17) ^ p.z.wrapping_mul(7) ^ i as i32) & 3);
                    for _ in 0..n {
                        self.particles.push(crate::particles::Request::Hit { cell: p, block: new, face });
                    }
                }
            }
        }
    }
    /// Schedules the next hatch tick; returns whether moss boosts it.
    pub(super) fn schedule_sniffer_egg(&mut self, p: IVec3) -> bool {
        let boost = self.get_block(p - IVec3::Y) == Some(super::overworld_blocks::MOSS_BLOCK);
        let delay = if boost { 4000 } else { 8000 } + self.roll() % 300;
        self.schedule_redstone(p, delay, 0);
        boost
    }
    pub(super) fn hatch_sniffer_egg(&mut self, p: IVec3) {
        let Some(b) = self.get_block(p).filter(|b| owns(b.0)) else {
            return;
        };
        if b.0 < LAST {
            self.edit(p, Block(b.0 + 1), false);
            self.sniffer_egg_events.push((p, false));
        } else {
            // `destroyBlock(pos, false)`: break effects without drops.
            self.edit(p, Block::AIR, false);
            self.particles.push(crate::particles::Request::Break { cell: p, block: b });
            self.sniffer_egg_events.push((p, true));
        }
    }
}
