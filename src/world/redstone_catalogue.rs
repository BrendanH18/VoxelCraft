//! v0.17 Track E2: which blocks take part in redstone, and how, as one
//! cold-path table the Java 26.3 component inventory
//! (`tests/fixtures/java/26.3/redstone_components.json`) is compared with
//! both ways (`tests/redstone_components.rs`).
//!
//! The columns are the ones the extractor derives from each Java class:
//! a signal source (`isSignalSource`), one with strong output on some side
//! (`getDirectSignal`), one a comparator reads (`hasAnalogOutputSignal`),
//! and one that reacts to power (a `SignalGetter` power query). Each
//! predicate here names the same block families the engine dispatches on
//! in `world::redstone` (`emitted_signal`, `container_signal`,
//! `redstone_update`), so a family added to one place and not the other
//! shows up as a fixture mismatch.

use super::block::Block;
use super::redstone_blocks::{self as r, Component};

/// A block's redstone role.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Role {
    /// Emits power in some state (`isSignalSource`).
    pub source: bool,
    /// Strongly powers a conductor in some state (`getDirectSignal`).
    pub strong: bool,
    /// Comparators read it rather than the power behind it
    /// (`hasAnalogOutputSignal`).
    pub analog: bool,
    /// Changes state or acts when its power changes.
    pub reads_power: bool,
}

impl Role {
    pub fn any(self) -> bool {
        self.source || self.strong || self.analog || self.reads_power
    }
}

/// The comparator output of the blocks whose `getAnalogOutputSignal`
/// depends only on their state (Java 26.3): cake `(7 - bites) * 2`, candle
/// cake 14 (`CakeBlock.FULL_CAKE_SIGNAL`), a cauldron's level (lava 3, an
/// empty cauldron 0), a respawn anchor `floor(charges / 4 * 15)` and an end
/// portal frame 15 with an eye.
pub fn state_analog_output(b: Block) -> Option<u8> {
    use super::utilities::Contents;
    if let Some(bites) = b.cake_bites() {
        return Some((7 - bites) * 2);
    }
    if super::colour_blocks::candle::decode_cake(b.0).is_some() {
        return Some(14);
    }
    if let Some(contents) = super::utilities::cauldron_of(b) {
        return Some(match contents {
            Contents::Empty => 0,
            Contents::Water(level) | Contents::PowderSnow(level) => level,
            Contents::Lava => 3,
        });
    }
    if let Some(charges) = super::utilities::anchor_charges(b) {
        return Some((f32::from(charges) / 4.0 * 15.0).floor() as u8);
    }
    if b.base() == Block::END_PORTAL_FRAME {
        return Some(if super::end_portal::has_eye(b) { 15 } else { 0 });
    }
    None
}

/// Emits power in some state, and whether that includes strong power.
fn source(b: Block) -> Option<bool> {
    if super::cave_blocks::any_sensor(b).is_some() {
        return Some(true);
    }
    if super::workstations::lectern(b.0).is_some()
        || super::lightning::rod(b).is_some()
        || super::utilities::trapped_chest_of(b).is_some()
        || super::gadgets::is_hook(b)
        || super::rails::kind(b) == Some(super::rails::RailKind::Detector)
    {
        return Some(true);
    }
    if super::jukebox::is_jukebox(b) {
        return Some(false);
    }
    match r::component(b)? {
        Component::Daylight { .. } | Component::Target(_) | Component::Source => Some(false),
        Component::Plate { .. }
        | Component::Lever { .. }
        | Component::Button { .. }
        | Component::Torch { .. }
        | Component::Wire(_)
        | Component::Observer { .. }
        | Component::Repeater { .. }
        | Component::Comparator { .. } => Some(true),
        _ => None,
    }
}

/// Comparators read the block itself (`World::container_signal` and the
/// shelf readings in `world::wood`).
fn analog(b: Block) -> bool {
    state_analog_output(b).is_some()
        || super::cave_blocks::any_sensor(b).is_some()
        || super::jukebox::is_jukebox(b)
        || super::workstations::lectern(b.0).is_some()
        || super::creaking::heart(b).is_some()
        || super::bees::hive(b).is_some()
        || super::rails::kind(b) == Some(super::rails::RailKind::Detector)
        || super::composter::level(b).is_some()
        || super::copper_golem_statue::comparator_output(b).is_some()
        || super::copper::bulb(b).is_some()
        || (super::chest::is_chest(b) && super::utilities::ender_chest_of(b).is_none())
        || super::furnace::is_furnace(b)
        || b == Block::BREWING_STAND
        || super::utilities::decorated_pot_of(b).is_some()
        || super::wood::shelves::shelf(b).is_some()
        || super::wood::bookshelf::slots(b).is_some()
}

/// Reacts to its power: the families `World::redstone_update` (and the
/// automation and rail updates it hands on to) acts on when powered.
fn reads_power(b: Block) -> bool {
    if b.is_door() || b.is_gate() || b.is_rail() || b == Block::TNT || b.base() == Block::BELL {
        return true;
    }
    if super::gadgets::is_note(b)
        || super::copper::bulb(b).is_some()
        || super::utilities::dripleaf_of(b).is_some()
        || super::skulls::decode(b).is_some()
        || super::wood::redstone_participant(b)
        // v0.17 Track C1: `CrafterBlock.neighborChanged` reads `hasNeighborSignal`.
        || super::crafter::owns(b.0)
    {
        return true;
    }
    matches!(
        r::component(b),
        Some(
            Component::Wire(_)
                | Component::Torch { .. }
                | Component::Repeater { .. }
                | Component::Comparator { .. }
                | Component::Lamp(_)
                | Component::IronDoor { .. }
                | Component::Trapdoor { .. }
                | Component::Piston { .. }
                | Component::Dispenser { .. }
                | Component::Hopper { .. }
        )
    )
}

/// The redstone role of `b`.
pub fn role(b: Block) -> Role {
    let emits = source(b);
    Role { source: emits.is_some(), strong: emits == Some(true), analog: analog(b), reads_power: reads_power(b) }
}
