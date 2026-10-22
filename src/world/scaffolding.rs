//! Java 26.3 `ScaffoldingBlock` states and scheduled stability updates.
//! The existing persisted block-tick queue carries the one-tick updates; no
//! loaded-world scan is needed. IDs 7936..7967 encode distance, bottom, water.
use super::{
    World,
    block::{Block, Facing, RenderKind},
    shape::{Box16, Boxes},
};
use glam::IVec3;

pub const FIRST: u16 = 7936;
pub const SCAFFOLDING: Block = state(7, false, false);
pub const POWDER_SNOW: Block = Block(7968);

pub const fn state(distance: u8, bottom: bool, wet: bool) -> Block {
    Block(FIRST + distance as u16 + ((bottom as u16) << 3) + ((wet as u16) << 4))
}
pub const fn is_scaffolding(b: Block) -> bool {
    b.0 >= FIRST && b.0 < FIRST + 32
}
pub const fn properties(b: Block) -> Option<(u8, bool, bool)> {
    if is_scaffolding(b) {
        let n = b.0 - FIRST;
        Some(((n & 7) as u8, n & 8 != 0, n & 16 != 0))
    } else {
        None
    }
}
pub const fn water_pair(id: u16) -> Option<(u16, bool)> {
    if id >= FIRST && id < FIRST + 32 { Some((id ^ 16, (id - FIRST) & 16 != 0)) } else { None }
}
pub const fn registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    use super::block::tex;
    if is_scaffolding(Block(id)) {
        Some((
            "scaffolding",
            RenderKind::Shaped,
            [
                tex::SCAFFOLD_SIDE,
                tex::SCAFFOLD_SIDE,
                tex::SCAFFOLD_TOP,
                tex::SCAFFOLD_TOP,
                tex::SCAFFOLD_SIDE,
                tex::SCAFFOLD_SIDE,
            ],
        ))
    } else if id == POWDER_SNOW.0 {
        Some(("powder snow", RenderKind::Cutout, [tex::POWDER_SNOW; 6]))
    } else {
        None
    }
}

pub fn distance(get: impl Fn(IVec3) -> Option<Block>, p: IVec3) -> u8 {
    let mut distance = match get(p - IVec3::Y) {
        Some(b) if is_scaffolding(b) => properties(b).unwrap().0,
        Some(b) if super::mount_states::face_sturdy(b, IVec3::Y) => return 0,
        _ => 7,
    };
    for f in Facing::ALL {
        if let Some((d, _, _)) = get(p + f.offset()).and_then(properties) {
            distance = distance.min(d + 1);
            if distance == 1 {
                break;
            }
        }
    }
    distance
}
pub fn placed(get: impl Fn(IVec3) -> Option<Block>, p: IVec3, wet: bool) -> Block {
    let d = distance(&get, p);
    state(d, d > 0 && !get(p - IVec3::Y).is_some_and(is_scaffolding), wet)
}

/// `ScaffoldingBlockItem.updatePlacementContext`. `inside` means the hit
/// started inside its block; vertical extension stops only at build limits.
pub fn placement_target(
    get: impl Fn(IVec3) -> Option<Block>,
    ctx: &crate::session::placement::PlaceContext,
    inside: bool,
    min_y: i32,
    max_y: i32,
) -> Option<IVec3> {
    if !is_scaffolding(ctx.clicked_block) {
        return (distance(&get, ctx.at) < 7).then_some(ctx.at);
    }
    let dir = if ctx.sneaking {
        if inside { -ctx.face } else { ctx.face }
    } else if ctx.face == IVec3::Y {
        ctx.facing.offset()
    } else {
        IVec3::Y
    };
    if dir == IVec3::ZERO {
        return None;
    }
    let mut p = ctx.clicked + dir;
    let mut horizontal = 0;
    while horizontal < 7 && p.y >= min_y && p.y < max_y {
        let b = get(p)?;
        if !is_scaffolding(b) {
            return b.is_replaceable().then_some(p);
        }
        p += dir;
        if dir.y == 0 {
            horizontal += 1;
        }
    }
    None
}

const fn bx(min: [u8; 3], max: [u8; 3]) -> Box16 {
    Box16 { min, max }
}
pub const STABLE: [Box16; 5] = [
    bx([0, 14, 0], [16, 16, 16]),
    bx([0, 0, 0], [2, 16, 2]),
    bx([14, 0, 0], [16, 16, 2]),
    bx([0, 0, 14], [2, 16, 16]),
    bx([14, 0, 14], [16, 16, 16]),
];
pub const BOTTOM: Box16 = bx([0, 0, 0], [16, 2, 16]);
pub fn outline(b: Block) -> Boxes {
    let mut out = Boxes::from_box(STABLE[0]);
    for &b in &STABLE[1..] {
        out.push(b);
    }
    if properties(b).is_some_and(|(_, bottom, _)| bottom) {
        out.push(BOTTOM);
        // Java's four rotated bottom strips are contained in BOTTOM; their
        // union adds no geometry beyond this full 2/16 plate.
    }
    out
}

impl World {
    pub fn powder_snow_fire(&mut self, player: &crate::player::Player, burning: bool) {
        if !(burning || player.in_fire(self) || player.in_lava(self)) || player.noclip {
            return;
        }
        let (min, max) = player.collision_shape().aabb(player.pos);
        let lo = (min + glam::DVec3::splat(1.0e-5)).floor().as_ivec3();
        let hi = (max - glam::DVec3::splat(1.0e-5)).floor().as_ivec3();
        for y in lo.y..=hi.y {
            for z in lo.z..=hi.z {
                for x in lo.x..=hi.x {
                    let p = IVec3::new(x, y, z);
                    if self.get_block(p) == Some(POWDER_SNOW) {
                        self.set_block(p, Block::AIR);
                    }
                }
            }
        }
    }
    pub(super) fn scaffolding_changed(&mut self, p: IVec3) {
        for d in [IVec3::ZERO, IVec3::Y, IVec3::NEG_Y, IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z] {
            if self.get_block(p + d).is_some_and(is_scaffolding) {
                self.schedule_redstone(p + d, 1, 0);
            }
        }
    }
    pub(super) fn tick_scaffolding(&mut self, p: IVec3) {
        let Some(old) = self.get_block(p).filter(|&b| is_scaffolding(b)) else { return };
        let (old_distance, _, wet) = properties(old).unwrap();
        // A chunk border with unavailable neighbours must wait, not destroy a build.
        if [IVec3::NEG_Y, IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z]
            .into_iter()
            .any(|d| self.get_block(p + d).is_none())
        {
            self.schedule_redstone(p, 1, 0);
            return;
        }
        let new = placed(|q| self.get_block(q), p, wet);
        if properties(new).unwrap().0 == 7 {
            if old_distance == 7 {
                self.start_scaffolding_fall(p, new.with_water(false), wet);
            } else {
                self.spill_block(p, old);
                self.set_block(p, Block::AIR);
            }
        } else if new != old {
            self.set_block(p, new);
        }
    }
}

#[cfg(test)]
mod tests;
