//! v1.0 final Track F4: goat horns as Java 26.3's `InstrumentItem`.
//!
//! VoxelCraft keeps a horn's instrument in its item ID: `Item::GOAT_HORN`
//! plus the instrument's index in Java's `Instruments` registry order
//! (ponder, sing, seek, feel; then the screaming goats' admire, call,
//! yearn and dream). Every one of Java's eight instruments has the same
//! numbers (`Instruments.bootstrap`): a 7-second use and cooldown, a range
//! of 256 blocks and no durability damage.

use crate::item::Item;

/// `Instruments.GOAT_HORN_DURATION` (`useDuration`), in ticks: the use time
/// and the cooldown `InstrumentItem.use` adds (`Mth.floor(7.0 * 20)`).
pub const USE_TICKS: u32 = 140;
/// `Instruments.GOAT_HORN_RANGE_BLOCKS`: `play`'s volume is `range / 16`.
pub const RANGE: f32 = 256.0;
/// Java's eight instruments.
pub const INSTRUMENTS: u8 = 8;

/// The instrument a goat horn item plays, by Java registry index.
pub fn instrument(item: Item) -> Option<u8> {
    let i = item.0.checked_sub(Item::GOAT_HORN.0)?;
    (i < u16::from(INSTRUMENTS)).then_some(i as u8)
}

/// The cooldown in seconds.
pub fn cooldown_seconds() -> f32 {
    USE_TICKS as f32 * crate::simulation::TICK_SECONDS as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eight_instruments_follow_the_horn_item() {
        assert_eq!(instrument(Item::GOAT_HORN), Some(0));
        assert_eq!(instrument(Item(Item::GOAT_HORN.0 + 7)), Some(7));
        assert_eq!(instrument(Item(Item::GOAT_HORN.0 + 8)), None);
        assert_eq!(instrument(Item(Item::GOAT_HORN.0 - 1)), None);
        assert!((cooldown_seconds() - 7.0).abs() < 1e-5);
    }
}
