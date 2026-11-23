//! Java 26.3 `PushReaction` for VoxelCraft block states, and
//! `PistonBaseBlock.isPushable`. `tests/pistons.rs` compares the effective
//! class of every Java block VoxelCraft registers with the jar
//! (`tests/fixtures/java/26.3/pistons.json`).
use glam::IVec3;

use super::super::{
    World,
    block::Block,
    redstone_blocks::{self as r, Component},
};

/// Java's `PushReaction` (`IGNORE_ENTITY` applies only to entities).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PushReaction {
    /// `PUSH_PULL`: moved both ways, and dragged by slime and honey.
    PushPull,
    /// `PUSH`: pushed, never pulled or dragged (glazed terracotta).
    Push,
    /// `POPPED`: broken and dropped by a push.
    Popped,
    /// `IMMOVEABLE`.
    Immoveable,
}

/// What a piston can do with a block, combining its reaction, its block
/// entity and its destroy time as `PistonBaseBlock.isPushable` does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PistonClass {
    Move,
    PushOnly,
    Destroy,
    Immovable,
}

impl PistonClass {
    pub fn name(self) -> &'static str {
        match self {
            Self::Move => "move",
            Self::PushOnly => "push_only",
            Self::Destroy => "destroy",
            Self::Immovable => "immovable",
        }
    }

    /// The class of a Java block from its `pushReaction`, `EntityBlock`
    /// interface and destroy time (fixture vocabulary).
    pub fn of_java(reaction: &str, block_entity: bool, destroy_time: f64) -> Self {
        if destroy_time == -1.0 {
            return Self::Immovable;
        }
        match reaction {
            "IMMOVEABLE" => Self::Immovable,
            "POPPED" => Self::Destroy,
            _ if block_entity => Self::Immovable,
            "PUSH" => Self::PushOnly,
            _ => Self::Move,
        }
    }
}

/// Slime and honey blocks (`PistonStructureResolver.isSticky`).
pub fn is_sticky(b: Block) -> bool {
    b == super::super::stone_blocks::SLIME_BLOCK || b == Block::HONEY_BLOCK
}

/// `PistonStructureResolver.canStickToEachOther`: one side must be sticky,
/// and honey never holds slime.
pub fn stick_together(a: Block, b: Block) -> bool {
    let slime = super::super::stone_blocks::SLIME_BLOCK;
    if (a == Block::HONEY_BLOCK && b == slime) || (a == slime && b == Block::HONEY_BLOCK) {
        return false;
    }
    is_sticky(a) || is_sticky(b)
}

/// Pistons and sticky pistons, extended or not (`state.is(PISTON)`).
pub fn is_piston(b: Block) -> bool {
    matches!(r::component(b), Some(Component::Piston { .. }))
}

const REACTION: u8 = 3;
const BLOCK_ENTITY: u8 = 4;
const UNBREAKABLE: u8 = 8;

/// Java flags per VoxelCraft state (see [`super::java_table`]), built once.
/// A Java block maps to the states sharing its VoxelCraft name, resolved as
/// `java_registry::block` does: the registry ID (or its alias) first, then
/// the English name.
fn flags(b: Block) -> u8 {
    use crate::world::block::STATE_CAPACITY;
    static TABLE: std::sync::OnceLock<Box<[u8]>> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let names: rustc_hash::FxHashSet<&'static str> = (0..STATE_CAPACITY).map(|i| Block(i as u16).name()).collect();
        let mut by_name: rustc_hash::FxHashMap<&'static str, u8> = Default::default();
        for &(id, english, flags) in super::java_table::JAVA_BLOCKS {
            let primary = crate::java_registry::block_alias(id).map_or_else(|| id.replace('_', " "), str::to_string);
            if let Some(&name) = names.get(primary.as_str()).or_else(|| names.get(english)) {
                by_name.insert(name, flags);
            }
        }
        (0..STATE_CAPACITY)
            .map(|i| {
                let b = Block(i as u16).canonical();
                // Beds are named by their halves.
                if b.is_bed() { 2 } else { by_name.get(b.name()).copied().unwrap_or(0) }
            })
            .collect()
    });
    table.get(b.0 as usize).copied().unwrap_or(0)
}

/// The block's Java `PushReaction` (`PUSH_PULL` for blocks Java doesn't have).
pub fn push_reaction(b: Block) -> PushReaction {
    match flags(b) & REACTION {
        0 => PushReaction::PushPull,
        1 => PushReaction::Push,
        2 => PushReaction::Popped,
        _ => PushReaction::Immoveable,
    }
}

/// Whether the block is an `EntityBlock` in Java (`hasBlockEntity`).
pub fn has_block_entity(b: Block) -> bool {
    flags(b) & BLOCK_ENTITY != 0
}

/// Java destroy time -1, or a VoxelCraft block that can't be broken.
fn unbreakable(b: Block) -> bool {
    flags(b) & UNBREAKABLE != 0 || (!b.is_fluid() && !b.hardness().is_finite())
}

/// The class `PistonBaseBlock.isPushable` gives a block (ignoring position
/// and direction; pistons are movable only while retracted).
pub fn piston_class(b: Block) -> PistonClass {
    if let Some(Component::Piston { extended, .. }) = r::component(b) {
        return if extended { PistonClass::Immovable } else { PistonClass::Move };
    }
    if unbreakable(b) {
        return PistonClass::Immovable;
    }
    match push_reaction(b) {
        PushReaction::Immoveable => PistonClass::Immovable,
        PushReaction::Popped => PistonClass::Destroy,
        _ if has_block_entity(b) => PistonClass::Immovable,
        PushReaction::Push => PistonClass::PushOnly,
        PushReaction::PushPull => PistonClass::Move,
    }
}

impl World {
    /// `PistonBaseBlock.isPushable`: `dir` is the push direction, and
    /// `connection` the direction the block is reached from (a `PUSH` block
    /// moves only when they agree). Unloaded cells never move.
    pub(crate) fn piston_pushable(&self, p: IVec3, dir: IVec3, allow_destroy: bool, connection: IVec3) -> bool {
        if !self.contains_y(p.y) {
            return false;
        }
        let Some(b) = self.get_block(p) else { return false };
        if b == Block::AIR {
            return true;
        }
        if (dir == IVec3::NEG_Y && p.y == self.min_y()) || (dir == IVec3::Y && p.y == self.max_y() - 1) {
            return false;
        }
        match piston_class(b) {
            PistonClass::Move => true,
            PistonClass::PushOnly => dir == connection,
            PistonClass::Destroy => allow_destroy,
            PistonClass::Immovable => false,
        }
    }
}
