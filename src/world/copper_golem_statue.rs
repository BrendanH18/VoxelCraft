//! Copper golem statues, pinned to Java 26.3. State IDs 14336..=14591:
//! `FIRST + (((waxed * 4 + stage) * 4 + pose) * 2 + wet) * 4 + facing`.
//! No entity dependency: resurrection and world block-entity names belong to
//! the session integrator. See the dated verification record for that contract.
use super::{
    World,
    block::{Block, Facing, RenderKind},
    copper::Stage,
    shape::{Box16, Boxes},
};
use crate::inventory::{Stack, StackName};
use glam::IVec3;

pub const FIRST: u16 = 14336;
pub const END: u16 = 14592;
pub const TEXTURE_FIRST: u16 = 3328;
pub const TEXTURE_END: u16 = TEXTURE_FIRST + 12;
const _: () = assert!(TEXTURE_END <= 3360);

/// Java declaration order and serialized values; comparator output is ordinal + 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Pose {
    Standing,
    Sitting,
    Running,
    Star,
}
impl Pose {
    pub const ALL: [Self; 4] = [Self::Standing, Self::Sitting, Self::Running, Self::Star];
    /// Java BY_ID uses ZERO for out-of-bounds values.
    pub const fn from_id(id: u32) -> Self {
        if id < 4 { Self::ALL[id as usize] } else { Self::Standing }
    }
    pub const fn name(self) -> &'static str {
        ["standing", "sitting", "running", "star"][self as usize]
    }
    pub const fn next(self) -> Self {
        Self::from_id(self as u32 + 1)
    }
    pub const fn comparator_output(self) -> u8 {
        self as u8 + 1
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Statue {
    pub stage: Stage,
    pub waxed: bool,
    pub facing: Facing,
    pub pose: Pose,
    pub waterlogged: bool,
}
impl Statue {
    pub const fn new(stage: Stage, waxed: bool) -> Self {
        Self { stage, waxed, facing: Facing::North, pose: Pose::Standing, waterlogged: false }
    }
    pub const fn cycle_pose(self) -> Self {
        Self { pose: self.pose.next(), ..self }
    }
    pub const fn wax(self) -> Option<Self> {
        if self.waxed { None } else { Some(Self { waxed: true, ..self }) }
    }
    /// Axe priority: remove wax, otherwise one oxidation stage. An unaffected,
    /// unwaxed statue returns None: the parent must attempt resurrection instead.
    pub const fn scrape(self) -> Option<Self> {
        if self.waxed {
            return Some(Self { waxed: false, ..self });
        }
        match self.stage.previous() {
            Some(stage) => Some(Self { stage, ..self }),
            None => None,
        }
    }
    /// Java's BLOCK_STATE item component carries pose only, not facing/water.
    /// This item's existing bounded entity_data payload holds the pose ordinal;
    /// StackName holds CUSTOM_NAME. World names are supplied by the parent.
    pub fn item_stack(self, name: StackName) -> Stack {
        Stack { name, entity_data: self.pose as u32, ..Stack::new(item_block(self.stage, self.waxed), 1) }
    }
}

pub const fn owns(id: u16) -> bool {
    id >= FIRST && id < END
}
pub const fn decode(b: Block) -> Option<Statue> {
    if !owns(b.0) {
        return None;
    }
    let n = b.0 - FIRST;
    Some(Statue {
        stage: Stage::ALL[(n / 32 % 4) as usize],
        waxed: n >= 128,
        facing: Facing::ALL[(n % 4) as usize],
        pose: Pose::ALL[(n / 8 % 4) as usize],
        waterlogged: n & 4 != 0,
    })
}
pub const fn block(s: Statue) -> Block {
    Block(
        FIRST
            + (((s.waxed as u16 * 4 + s.stage as u16) * 4 + s.pose as u16) * 2 + s.waterlogged as u16) * 4
            + s.facing as u16,
    )
}
pub const fn item_block(stage: Stage, waxed: bool) -> Block {
    block(Statue::new(stage, waxed))
}
pub fn palette_ids() -> impl Iterator<Item = u16> {
    [false, true].into_iter().flat_map(|waxed| Stage::ALL.map(move |stage| item_block(stage, waxed).0))
}
/// A block model for a pose-bearing stack, usable by inventory/held/dropped
/// item renderers; also usable by the parent's remaining entity-held path.
pub fn item_model(stack: Stack) -> Option<Block> {
    let s = stack.item.block().and_then(decode)?;
    Some(block(Statue { pose: Pose::from_id(stack.entity_data), ..s }))
}
/// Read the BLOCK_STATE pose component when placing an item. Facing and water
/// are determined by the placement context, not inherited from the item.
pub fn item_pose(stack: Stack) -> Option<Pose> {
    stack.item.block().and_then(decode).map(|_| Pose::from_id(stack.entity_data))
}
pub const fn water_pair(id: u16) -> Option<(u16, bool)> {
    if owns(id) { Some((id ^ 4, (id - FIRST) & 4 != 0)) } else { None }
}
pub const fn name(s: Statue) -> &'static str {
    const NAMES: [[&str; 4]; 2] = [
        [
            "copper golem statue",
            "exposed copper golem statue",
            "weathered copper golem statue",
            "oxidized copper golem statue",
        ],
        [
            "waxed copper golem statue",
            "waxed exposed copper golem statue",
            "waxed weathered copper golem statue",
            "waxed oxidized copper golem statue",
        ],
    ];
    NAMES[s.waxed as usize][s.stage as usize]
}
pub const fn registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    let Some(s) = decode(Block(id)) else { return None };
    let base = TEXTURE_FIRST + s.stage as u16 * 3;
    let mut tex = [base; 6];
    tex[2] = base + 2;
    let front = match s.facing {
        Facing::East => 0,
        Facing::West => 1,
        Facing::South => 4,
        Facing::North => 5,
    };
    tex[front] = base + 1;
    Some((name(s), RenderKind::Shaped, tex))
}
pub fn comparator_output(b: Block) -> Option<u8> {
    decode(b).map(|s| s.pose.comparator_output())
}
pub const fn ages(b: Block) -> bool {
    matches!(decode(b), Some(Statue { waxed: false, stage: Stage::Unaffected | Stage::Exposed | Stage::Weathered, .. }))
}

/// Java's fixed column(10, 0, 14), independent of facing and pose; also outline.
pub fn collision() -> Boxes {
    Boxes::from_box(Box16 { min: [3, 0, 3], max: [13, 14, 13] })
}

/// Original model: riveted head, projecting nose, rod cap, torso and articulated
/// arms/feet. Every pose is deliberately distinct without importing Java art.
pub fn mesh(s: Statue) -> Boxes {
    let mut out = Boxes::new();
    let mut add = |min, max| out.push(Box16 { min, max }.turned(s.facing));
    let low = s.pose == Pose::Sitting;
    let h = if low { 6 } else { 8 };
    add([4, h, 4], [12, h + 5, 12]);
    add([7, h + 1, 12], [9, h + 3, 14]); // nose faces +Z
    add([7, h + 5, 7], [9, h + 7, 9]); // rod on the head
    add([5, h - 4, 5], [11, h, 11]);
    match s.pose {
        Pose::Standing => {
            add([2, 3, 6], [5, 8, 10]);
            add([11, 3, 6], [14, 8, 10]);
            add([5, 0, 5], [8, 4, 11]);
            add([8, 0, 5], [11, 4, 11]);
        }
        Pose::Sitting => {
            add([2, 2, 7], [5, 6, 11]);
            add([11, 2, 7], [14, 6, 11]);
            add([5, 0, 7], [8, 2, 14]);
            add([8, 0, 7], [11, 2, 14]);
        }
        Pose::Running => {
            add([2, 5, 3], [5, 8, 8]);
            add([11, 5, 9], [14, 8, 14]);
            add([5, 0, 2], [8, 4, 8]);
            add([8, 0, 9], [11, 4, 15]);
        }
        Pose::Star => {
            add([0, 7, 6], [5, 10, 10]);
            add([11, 7, 6], [16, 10, 10]);
            add([2, 0, 5], [6, 4, 11]);
            add([10, 0, 5], [14, 4, 11]);
        }
    }
    out
}

impl World {
    pub fn copper_golem_statue_name(&self, p: IVec3) -> StackName {
        self.copper_golem_names.get(&p).copied().unwrap_or_default()
    }

    /// `CopperGolemStatueBlockEntity` copies CUSTOM_NAME, not mob inventory.
    pub fn name_copper_golem_statue(&mut self, p: IVec3, name: StackName) {
        if self.get_block(p).and_then(decode).is_some() && name.as_str().is_some() {
            self.copper_golem_names.insert(p, name);
        } else {
            self.copper_golem_names.remove(&p);
        }
    }

    pub fn copper_golem_statue_item(&self, p: IVec3) -> Option<Stack> {
        Some(decode(self.get_block(p)?)?.item_stack(self.copper_golem_statue_name(p)))
    }

    pub(super) fn track_copper_golem_statue(&mut self, p: IVec3, new: Block) {
        // All eight statue blocks share the same block entity in Java.
        if !owns(new.0) {
            self.copper_golem_names.remove(&p);
        }
    }

    pub fn copper_golem_statues_to_string(&self) -> String {
        let mut names: Vec<_> =
            self.copper_golem_names.iter().filter_map(|(p, n)| n.as_str().map(|n| (p.to_array(), n))).collect();
        names.sort_unstable_by_key(|(p, _)| *p);
        serde_json::to_string(&names).expect("statue names serialize")
    }

    pub fn load_copper_golem_statues(&mut self, text: &str) {
        let Ok(names) = serde_json::from_str::<Vec<([i32; 3], String)>>(text) else { return };
        for (p, text) in names {
            let p = IVec3::from_array(p);
            if self.contains_y(p.y)
                && let Some(name) = StackName::new(&text).filter(|n| n.as_str().is_some())
            {
                // Level data is restored before every saved chunk is loaded.
                self.copper_golem_names.insert(p, name);
            }
        }
    }

    /// Small component snapshot for a LAN client's local pick-block selection.
    /// Block geometry already travels in the regular chunk stream.
    pub fn copper_golem_statues_near(&self, at: glam::DVec3) -> String {
        let mut near: Vec<_> = self
            .copper_golem_names
            .iter()
            .filter_map(|(p, name)| {
                let distance = p.as_dvec3().distance_squared(at);
                (distance <= 160.0 * 160.0).then(|| name.as_str().map(|n| (distance, p.to_array(), n))).flatten()
            })
            .collect();
        near.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        let names: Vec<_> = near.into_iter().take(128).map(|(_, p, name)| (p, name)).collect();
        serde_json::to_string(&names).expect("statue names serialize")
    }

    pub fn replace_copper_golem_statues(&mut self, text: &str) {
        self.copper_golem_names.clear();
        self.load_copper_golem_statues(text);
    }

    /// Mining removes the block before generic loot runs. Capture its name
    /// first, just as Java keeps the block entity for playerDestroy.
    pub fn mine_copper_golem_statue(&mut self, p: IVec3, drops: bool) -> bool {
        let Some(stack) = self.copper_golem_statue_item(p) else { return false };
        if drops && self.tile_drops {
            self.drops.push((p, stack));
        }
        true
    }

    pub(super) fn tick_copper_golem_statue(&mut self, p: IVec3, b: Block) {
        let Some(s) = decode(b) else { return };
        if ages(b) && self.copper_oxidizes(p, s.stage) {
            self.edit(p, block(Statue { stage: s.stage.next().unwrap(), ..s }), false);
        }
    }
}
#[cfg(test)]
mod tests;
