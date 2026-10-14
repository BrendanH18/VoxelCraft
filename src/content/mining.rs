//! Reference-backed block tags used by the shared mining transaction.
use crate::item::ToolKind;
use crate::world::block::{Block, STATE_CAPACITY};
use std::sync::OnceLock;
include!(concat!(env!("OUT_DIR"), "/mining_tags.rs"));

/// The reference name of an existing block, where VoxelCraft's display label
/// differs. Unlisted names are already the reference name or have no
/// reference counterpart.
fn reference_name(base: Block) -> &'static str {
    match base.name() {
        "grass" | "snowy grass" => "grass block",
        "snow" => "snow block",
        "snow layer" => "snow",
        "wool" => "white wool",
        "lit redstone ore" => "redstone ore",
        "lit furnace" => "furnace",
        "vines" => "vine",
        "hay bale" => "hay block",
        "wet farmland" => "farmland",
        "jack o'lantern" => "jack o lantern",
        "lapis lazuli ore" => "lapis ore",
        "deepslate lapis lazuli ore" => "deepslate lapis ore",
        "block of amethyst" => "amethyst block",
        "block of bamboo" => "bamboo block",
        "block of copper" => "copper block",
        "block of emerald" => "emerald block",
        "block of gold" => "gold block",
        "block of iron" => "iron block",
        "block of lapis lazuli" => "lapis block",
        "block of netherite" => "netherite block",
        "block of raw copper" => "raw copper block",
        "block of raw gold" => "raw gold block",
        "block of raw iron" => "raw iron block",
        "block of redstone" => "redstone block",
        name => name,
    }
}

/// Java's ice support tag or an actual liquid block. Waterlogged plants are
/// not liquids for this rule. Resolve names once; actions only read a bounded table.
pub fn ice_support(block: Block) -> bool {
    static SUPPORT: OnceLock<[bool; STATE_CAPACITY]> = OnceLock::new();
    let support = SUPPORT.get_or_init(|| {
        std::array::from_fn(|id| ICE_SUPPORT_NAMES.binary_search(&reference_name(Block(id as u16).base())).is_ok())
    });
    block.is_fluid() || support.get(usize::from(block.0)).copied().unwrap_or(false)
}

const KNOWN: u16 = 1 << 0;
const PICKAXE: u16 = 1 << 1;
const AXE: u16 = 1 << 2;
const SHOVEL: u16 = 1 << 3;
const HOE: u16 = 1 << 4;
const SWORD_EFFICIENT_BIT: u16 = 1 << 5;
const SWORD_INSTANT_BIT: u16 = 1 << 6;
const SHEARS_EXTREME_BIT: u16 = 1 << 7;
const SHEARS_MAJOR_BIT: u16 = 1 << 8;
const SHEARS_MINOR_BIT: u16 = 1 << 9;

/// Per-state Tool component inputs, resolved once from the reference tags.
fn tool_flags(block: Block) -> u16 {
    static FLAGS: OnceLock<[u16; STATE_CAPACITY]> = OnceLock::new();
    let flags = FLAGS.get_or_init(|| {
        std::array::from_fn(|id| {
            let name = reference_name(Block(id as u16).base());
            let has = |names: &[&str]| names.binary_search(&name).is_ok();
            let tagged = [
                (PICKAXE, MINEABLE_PICKAXE),
                (AXE, MINEABLE_AXE),
                (SHOVEL, MINEABLE_SHOVEL),
                (HOE, MINEABLE_HOE),
                (SWORD_EFFICIENT_BIT, SWORD_EFFICIENT),
                (SWORD_INSTANT_BIT, SWORD_INSTANTLY_MINES),
                (SHEARS_EXTREME_BIT, SHEARS_EXTREME),
                (SHEARS_MAJOR_BIT, SHEARS_MAJOR),
                (SHEARS_MINOR_BIT, SHEARS_MINOR),
            ];
            let mut flags = tagged.iter().filter(|(_, names)| has(names)).fold(0, |all, (bit, _)| all | bit);
            if flags != 0 || has(TOOL_RULE_BLOCKS) {
                flags |= KNOWN;
            }
            flags
        })
    });
    flags.get(usize::from(block.0)).copied().unwrap_or(0)
}

/// Whether Java's `mineable/<kind>` tag holds `block`, which is what makes a
/// tool of that kind dig it at tier speed. Blocks with no reference
/// counterpart (VoxelCraft-only variants) keep the engine's own table.
pub fn mineable_by(block: Block, kind: ToolKind) -> bool {
    let flags = tool_flags(block);
    if flags & KNOWN == 0 {
        return block.best_tool() == Some(kind);
    }
    flags
        & match kind {
            ToolKind::Pickaxe => PICKAXE,
            ToolKind::Axe => AXE,
            ToolKind::Shovel => SHOVEL,
            ToolKind::Hoe => HOE,
            ToolKind::Sword => 0,
        }
        != 0
}

/// The speed Java's sword Tool rules give `block`: cobweb 15 (and drops),
/// `sword_instantly_mines` is instant and `sword_efficient` is 1.5.
pub fn sword_speed(block: Block) -> Option<f32> {
    let flags = tool_flags(block);
    if block == Block::COBWEB {
        Some(15.0)
    } else if flags & SWORD_INSTANT_BIT != 0 {
        Some(f32::MAX)
    } else if flags & SWORD_EFFICIENT_BIT != 0 {
        Some(1.5)
    } else {
        None
    }
}

/// The speed Java's shears Tool rules give `block`: cobweb and leaves 15,
/// wool 5, vines and glow lichen 2.
pub fn shears_speed(block: Block) -> Option<f32> {
    let flags = tool_flags(block);
    if flags & KNOWN == 0 {
        // VoxelCraft-only variants: leaves and wool shapes follow their family.
        return if block.is_leaves() { Some(15.0) } else { block.wool_color().map(|_| 5.0) };
    }
    if block == Block::COBWEB || flags & SHEARS_EXTREME_BIT != 0 {
        Some(15.0)
    } else if flags & SHEARS_MAJOR_BIT != 0 {
        Some(5.0)
    } else if flags & SHEARS_MINOR_BIT != 0 {
        Some(2.0)
    } else {
        None
    }
}
