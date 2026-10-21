//! Java 26.3 `SimpleWaterloggedBlock`: states with a `waterlogged` property
//! come in dry/wet pairs, each its own state id. Blocks that always hold
//! water (kelp, seagrass, live coral) are not pairs; see
//! [`super::overworld_blocks::waterlogged`].
//!
//! Each v0.9 family module reports its own pairs; this module owns the
//! remaining waterloggable families (BLD-04f, ids 8192–8703) and the
//! precomputed lookup.

use super::block::STATE_CAPACITY;

/// The other state of `id`'s waterlogged pair, and whether `id` is the wet one.
pub const fn pair(id: u16) -> Option<(u16, bool)> {
    if let Some(p) = super::slab_stair_states::water_pair(id) {
        return Some(p);
    }
    if let Some(p) = super::mount_states::water_pair(id) {
        return Some(p);
    }
    family_pair(id)
}

/// Pairs of the waterloggable families this module owns (fences, panes,
/// walls, bars, chains, trapdoors, rails, lanterns, ladders and so on).
const fn family_pair(_id: u16) -> Option<(u16, bool)> {
    None
}

static WET: [bool; STATE_CAPACITY] = {
    let mut arr = [false; STATE_CAPACITY];
    let mut i = 0;
    while i < STATE_CAPACITY {
        arr[i] = matches!(pair(i as u16), Some((_, true)));
        i += 1;
    }
    arr
};

/// `id` is the wet state of a waterlogged pair.
#[inline(always)]
pub fn wet(id: u16) -> bool {
    WET.get(id as usize).copied().unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::Block;

    #[test]
    fn pairs_are_symmetric_registered_and_not_always_wet() {
        for id in 0..STATE_CAPACITY as u16 {
            let Some((other, is_wet)) = pair(id) else { continue };
            assert_eq!(pair(other), Some((id, !is_wet)), "{id} and {other} must pair both ways");
            assert_ne!(Block(id).name(), "unknown", "{id} is paired but unregistered");
            assert!(!crate::world::overworld_blocks::waterlogged(id), "{id} is always wet; it can't be a pair");
            assert_eq!(wet(id), is_wet);
            assert_eq!(Block(id).with_water(is_wet), Block(id));
            assert_eq!(Block(id).with_water(!is_wet), Block(other));
        }
    }
}
