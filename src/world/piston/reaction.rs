//! Java 26.3 `PushReaction` for VoxelCraft block states, and
//! `PistonBaseBlock.isPushable`. `tests/pistons.rs` compares the effective
//! class of every Java block VoxelCraft registers with the jar
//! (`tests/fixtures/java/26.3/pistons.json`).
use glam::IVec3;

use super::super::{
    World,
    block::{Block, RenderKind},
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

/// The block's Java `PushReaction`.
pub fn push_reaction(b: Block) -> PushReaction {
    use super::super::*;
    let b = b.canonical();
    if b == Block::AIR {
        return PushReaction::PushPull;
    }
    // Fluids are `POPPED` (`LiquidBlock` properties).
    if b.is_fluid() {
        return PushReaction::Popped;
    }
    if matches!(
        b,
        Block::OBSIDIAN
            | Block::CRYING_OBSIDIAN
            | Block::NETHERITE_BLOCK
            | Block::END_PORTAL_FRAME
            | Block::NETHER_PORTAL
            | Block::END_PORTAL
            | Block::END_GATEWAY
    ) || jukebox::piston_immovable(b)
        || matches!(r::component(b), Some(Component::PistonHead { .. } | Component::Moving))
    {
        return PushReaction::Immoveable;
    }
    // Glazed terracotta can only be pushed.
    if b.glazed_color().is_some() {
        return PushReaction::Push;
    }
    if shulker_box::owns(b.0)
        || b.kind() == RenderKind::Cross
        || b.is_door()
        || mount_states::is_lantern(b.0)
        || copper::pops_when_pushed(b)
        || copper_golem_statue::owns(b.0)
        || b == frog_blocks::FROGSPAWN
        || b == utilities::FLOWER_POT
        || utilities::potted_plant(b).is_some()
        || utilities::decorated_pot_of(b).is_some()
        || scaffolding::is_scaffolding(b)
        || colour_blocks::pops_on_push(b)
        || skulls::owns(b.0)
        || b.is_fire()
        || matches!(
            r::component(b),
            Some(
                Component::Wire(_)
                    | Component::Lever { .. }
                    | Component::Button { .. }
                    | Component::Torch { .. }
                    | Component::Repeater { .. }
                    | Component::Comparator { .. }
                    | Component::Plate { .. }
                    | Component::IronDoor { .. }
            )
        )
    {
        return PushReaction::Popped;
    }
    PushReaction::PushPull
}

/// Whether the block is an `EntityBlock` in Java (`hasBlockEntity`).
pub fn has_block_entity(b: Block) -> bool {
    use super::super::*;
    let b = b.canonical();
    chest::is_chest(b)
        || furnace::is_furnace(b)
        || b.base() == Block::BREWING_STAND
        || b == Block::SPAWNER
        || b == Block::ENCHANTING_TABLE
        || colour_blocks::banner::decode(b.0).is_some()
        || wood::has_entity(b)
        || utilities::ender_chest_of(b).is_some()
        || utilities::campfire_of(b).is_some()
        || b == beacon::BEACON
        || conduit::owns(b.0)
        || shulker_box::owns(b.0)
        || matches!(
            r::component(b),
            Some(
                Component::Daylight { .. }
                    | Component::Dispenser { .. }
                    | Component::Hopper { .. }
                    | Component::Comparator { .. }
                    | Component::Moving
            )
        )
}

/// The class `PistonBaseBlock.isPushable` gives a block (ignoring position
/// and direction; pistons are movable only while retracted).
pub fn piston_class(b: Block) -> PistonClass {
    if let Some(Component::Piston { extended, .. }) = r::component(b) {
        return if extended { PistonClass::Immovable } else { PistonClass::Move };
    }
    if !b.hardness().is_finite() {
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
