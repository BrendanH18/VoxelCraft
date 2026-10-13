//! Reference-backed block tags used by the shared mining transaction.
use crate::world::block::{Block, STATE_CAPACITY};
use std::sync::OnceLock;
include!(concat!(env!("OUT_DIR"), "/mining_tags.rs"));

/// Java's ice support tag or an actual liquid block. Waterlogged plants are
/// not liquids for this rule. Resolve names once; actions only read a bounded table.
pub fn ice_support(block: Block) -> bool {
    static SUPPORT: OnceLock<[bool; STATE_CAPACITY]> = OnceLock::new();
    let support = SUPPORT.get_or_init(|| {
        std::array::from_fn(|id| {
            let base = Block(id as u16).base();
            let name = match base.name() {
                // Legacy display labels for the same existing reference blocks.
                "grass" | "snowy grass" => "grass block",
                "snow" => "snow block",
                "wool" => "white wool",
                "lit redstone ore" => "redstone ore",
                "lit furnace" => "furnace",
                name => name,
            };
            ICE_SUPPORT_NAMES.binary_search(&name).is_ok()
        })
    });
    block.is_fluid() || support.get(usize::from(block.0)).copied().unwrap_or(false)
}
