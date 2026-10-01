//! What the held item does to blocks and mobs: mining speed, whether a
//! block drops anything, tool wear and melee damage. Minecraft's rules.

use crate::item::{Item, Tier, ToolKind};
use crate::world::block::Block;

/// Whether mining `block` with `held` yields its drop.
pub fn can_harvest(block: Block, held: Option<Item>) -> bool {
    let Some(level) = block.harvest_level() else { return true };
    match held.and_then(Item::as_tool) {
        Some((kind, tier)) => Some(kind) == block.best_tool() && tier.level() >= level,
        None => false,
    }
}

/// Seconds to mine `block` holding `held` (infinite if unbreakable).
pub fn break_time(block: Block, held: Option<Item>) -> f32 {
    let speed = match held.and_then(Item::as_tool) {
        Some((kind, tier)) if Some(kind) == block.best_tool() => tier.speed(),
        _ => 1.0,
    };
    let factor = if can_harvest(block, held) { 1.5 } else { 5.0 };
    block.hardness() * factor / speed
}

/// Durability a tool loses for breaking a block (swords wear faster, as
/// they're not meant for it) or for hitting a mob (the reverse).
pub fn wear(held: Item, hitting_mob: bool) -> u16 {
    match (held.as_tool(), hitting_mob) {
        (Some((ToolKind::Sword, _)), false)
        | (Some((ToolKind::Pickaxe | ToolKind::Shovel | ToolKind::Axe, _)), true) => 2,
        (Some(_), _) => 1,
        (None, _) => 0,
    }
}

/// Melee damage of a hit with `held` (a fist does 1).
pub fn attack_damage(held: Option<Item>) -> f32 {
    let Some((kind, tier)) = held.and_then(Item::as_tool) else { return 1.0 };
    let bonus = match tier {
        Tier::Wood | Tier::Gold => 0.0,
        Tier::Stone => 1.0,
        Tier::Iron => 2.0,
        Tier::Diamond => 3.0,
    };
    let base = match kind {
        ToolKind::Sword => 4.0,
        ToolKind::Axe => 3.0,
        ToolKind::Pickaxe => 2.0,
        ToolKind::Shovel => 1.0,
        ToolKind::Hoe => return 1.0,
    };
    base + bonus
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(kind: ToolKind, tier: Tier) -> Option<Item> {
        Some(Item::tool(kind, tier))
    }

    #[test]
    fn ores_need_the_right_pickaxe_tier() {
        assert!(can_harvest(Block::DIRT, None) && can_harvest(Block::LOG, None));
        assert!(!can_harvest(Block::STONE, None), "stone needs a pickaxe");
        assert!(!can_harvest(Block::STONE, tool(ToolKind::Shovel, Tier::Diamond)));
        assert!(can_harvest(Block::STONE, tool(ToolKind::Pickaxe, Tier::Wood)));
        assert!(!can_harvest(Block::IRON_ORE, tool(ToolKind::Pickaxe, Tier::Wood)));
        assert!(can_harvest(Block::IRON_ORE, tool(ToolKind::Pickaxe, Tier::Stone)));
        assert!(!can_harvest(Block::DIAMOND_ORE, tool(ToolKind::Pickaxe, Tier::Gold)), "gold is fast but weak");
        assert!(can_harvest(Block::DIAMOND_ORE, tool(ToolKind::Pickaxe, Tier::Iron)));
        assert!(!can_harvest(Block::OBSIDIAN, tool(ToolKind::Pickaxe, Tier::Iron)));
        assert!(can_harvest(Block::OBSIDIAN, tool(ToolKind::Pickaxe, Tier::Diamond)));
    }

    #[test]
    fn better_tools_mine_faster() {
        // Minecraft's numbers: stone takes 7.5 s by hand, 1.125 s with a
        // wooden pickaxe, 0.56 stone, 0.375 iron, 0.28 diamond, 0.19 gold.
        let by_hand = break_time(Block::STONE, None);
        assert_eq!(by_hand, 7.5);
        let mut last = by_hand;
        for tier in [Tier::Wood, Tier::Stone, Tier::Iron, Tier::Diamond, Tier::Gold] {
            let t = break_time(Block::STONE, tool(ToolKind::Pickaxe, tier));
            assert!(t < last, "{tier:?}: {t} vs {last}");
            last = t;
        }
        // The wrong tool is no better than a hand.
        assert_eq!(break_time(Block::LOG, tool(ToolKind::Pickaxe, Tier::Diamond)), break_time(Block::LOG, None));
        assert!(break_time(Block::LOG, tool(ToolKind::Axe, Tier::Stone)) < break_time(Block::LOG, None));
        assert_eq!(break_time(Block::TORCH, None), 0.0);
        assert!(break_time(Block::BEDROCK, tool(ToolKind::Pickaxe, Tier::Diamond)).is_infinite());
    }

    #[test]
    fn weapons_and_wear() {
        assert_eq!(attack_damage(None), 1.0);
        assert_eq!(attack_damage(tool(ToolKind::Sword, Tier::Diamond)), 7.0);
        assert_eq!(attack_damage(tool(ToolKind::Sword, Tier::Wood)), 4.0);
        assert!(attack_damage(tool(ToolKind::Axe, Tier::Iron)) < attack_damage(tool(ToolKind::Sword, Tier::Iron)));
        let sword = Item::tool(ToolKind::Sword, Tier::Iron);
        let pick = Item::tool(ToolKind::Pickaxe, Tier::Iron);
        assert_eq!((wear(sword, true), wear(sword, false)), (1, 2));
        assert_eq!((wear(pick, false), wear(pick, true)), (1, 2));
        assert_eq!(wear(Item::STICK, false), 0);
    }
}
