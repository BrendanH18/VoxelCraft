//! What the held item does to blocks and mobs: mining speed, whether a
//! block drops anything, tool wear and melee damage. Minecraft's rules.

use crate::enchant::{self, Enchantment, Enchants};
use crate::inventory::Stack;
use crate::item::{Item, Tier, ToolKind};
use crate::world::block::Block;

/// Whether mining `block` with `held` yields its drop.
pub fn can_harvest(block: Block, held: Option<Item>) -> bool {
    if held == Some(Item::SHEARS) && block == Block::COBWEB {
        return true;
    }
    let Some(level) = block.harvest_level() else { return true };
    match held.and_then(Item::as_tool) {
        Some((kind, tier)) => Some(kind) == block.best_tool() && tier.level() >= level,
        None => false,
    }
}

/// Seconds to mine `block` holding `held` (infinite if unbreakable).
pub fn break_time(block: Block, held: Option<Item>) -> f32 {
    block.hardness() * harvest_factor(block, held) / tool_speed(block, held)
}

/// How fast the held item digs `block` (1 for a hand or the wrong tool).
fn tool_speed(block: Block, held: Option<Item>) -> f32 {
    if held == Some(Item::SHEARS) {
        if block.is_leaves() || block == Block::COBWEB {
            return 15.0;
        }
        if block == Block::WOOL {
            return 5.0;
        }
    }
    match held.and_then(Item::as_tool) {
        // Swords cut cobwebs fifteen times as fast (Java).
        Some((ToolKind::Sword, _)) if block == Block::COBWEB => 15.0,
        Some((kind, tier)) if Some(kind) == block.best_tool() => tier.speed(),
        _ => 1.0,
    }
}

fn harvest_factor(block: Block, held: Option<Item>) -> f32 {
    if can_harvest(block, held) { 1.5 } else { 5.0 }
}

/// Where the miner is, for Java's digging penalties.
#[derive(Clone, Copy, Debug)]
pub struct Digger {
    pub held: Option<Stack>,
    /// The worn helmet's enchantments (aqua affinity).
    pub helmet: Enchants,
    pub eyes_in_water: bool,
    pub on_ground: bool,
}

/// [`break_time`] with Java's modifiers: efficiency speeds up the right
/// tool; eyes underwater (without aqua affinity) and leaving the ground
/// each make digging five times slower.
pub fn dig_time(block: Block, digger: Digger) -> f32 {
    let held = digger.held.map(|s| s.item);
    let mut speed = tool_speed(block, held);
    if speed > 1.0 {
        speed += enchant::efficiency_bonus(digger.held.map_or(Enchants::NONE, |s| s.active_enchants()));
    }
    if digger.eyes_in_water && !digger.helmet.has(Enchantment::AquaAffinity) {
        speed /= 5.0;
    }
    if !digger.on_ground {
        speed /= 5.0;
    }
    block.hardness() * harvest_factor(block, held) / speed
}

/// Durability a tool loses for breaking a block (swords wear faster, as
/// they're not meant for it) or for hitting a mob (the reverse).
pub fn wear(held: Item, hitting_mob: bool) -> u16 {
    if held == Item::SHEARS && !hitting_mob {
        return 1;
    }
    match (held.as_tool(), hitting_mob) {
        (Some((ToolKind::Sword, _)), false)
        | (Some((ToolKind::Pickaxe | ToolKind::Shovel | ToolKind::Axe, _)), true) => 2,
        (Some(_), _) => 1,
        (None, _) => 0,
    }
}

/// Java's melee knockback and fire from the held weapon: extra knockback
/// levels and seconds of fire (fire aspect sets 4 s per level).
pub fn weapon_extras(held: Option<Stack>) -> (u8, f32) {
    let e = held.map_or(Enchants::NONE, |s| s.active_enchants());
    (e.level(Enchantment::Knockback), 4.0 * e.level(Enchantment::FireAspect) as f32)
}

/// Melee damage of a hit with `held` on `target`, enchantments included.
pub fn hit_damage(held: Option<Stack>, target: enchant::Creature) -> f32 {
    let enchants = held.map_or(Enchants::NONE, |s| s.active_enchants());
    attack_damage(held.map(|s| s.item)) + enchant::damage_bonus(enchants, target)
}

/// Melee damage of a hit with `held` (a fist does 1).
pub fn attack_damage(held: Option<Item>) -> f32 {
    let Some((kind, tier)) = held.and_then(Item::as_tool) else { return 1.0 };
    if tier == Tier::Netherite {
        return match kind {
            ToolKind::Sword => 8.0,
            ToolKind::Shovel => 6.5,
            ToolKind::Pickaxe => 6.0,
            ToolKind::Axe => 10.0,
            ToolKind::Hoe => 1.0,
        };
    }
    let bonus = match tier {
        Tier::Wood | Tier::Gold => 0.0,
        Tier::Stone => 1.0,
        Tier::Iron => 2.0,
        Tier::Diamond => 3.0,
        Tier::Netherite => unreachable!(),
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

/// Fully charged attacks per second. Existing tiers retain VoxelCraft's
/// two-per-second combat until their broader Java stat pass; Netherite
/// implements Java's per-tool attributes.
pub fn attack_speed(held: Option<Item>) -> f64 {
    match held.and_then(Item::as_tool) {
        Some((ToolKind::Sword, Tier::Netherite)) => 1.6,
        Some((ToolKind::Shovel | ToolKind::Axe, Tier::Netherite)) => 1.0,
        Some((ToolKind::Pickaxe, Tier::Netherite)) => 1.2,
        Some((ToolKind::Hoe, Tier::Netherite)) => 4.0,
        _ => 2.0,
    }
}

pub fn attack_cooldown(held: Option<Item>) -> f64 {
    1.0 / attack_speed(held)
}

/// Experience a harvested block drops (Java's ore ranges; silk touch
/// skips it: see [`mined_xp`]).
pub fn ore_xp(block: Block, rng: &mut crate::entity::Rng) -> u32 {
    let (lo, hi) = match block.as_stone_ore() {
        Block::COAL_ORE => (0, 2),
        Block::DIAMOND_ORE | Block::EMERALD_ORE => (3, 7),
        Block::REDSTONE_ORE => (1, 5),
        Block::QUARTZ_ORE | Block::LAPIS_ORE => (2, 5),
        Block::SPAWNER => (15, 43),
        _ => return 0,
    };
    lo + ((rng.next_f32() * (hi - lo + 1) as f32) as u32).min(hi - lo)
}

/// Silk Touch suppresses XP only when it actually harvests the block
/// itself. Spawners cannot be collected, and still award XP with it.
pub fn mined_xp(block: Block, tool: Enchants, rng: &mut crate::entity::Rng) -> u32 {
    if tool.has(Enchantment::SilkTouch) && silk_drop(block).is_some() { 0 } else { ore_xp(block, rng) }
}

/// Whether a block mined with silk touch drops itself (Java's
/// silk-touchable blocks among ours).
pub fn silk_drop(block: Block) -> Option<Item> {
    let b = if block.base() == Block::SNOWY_GRASS { Block::GRASS } else { block.base() };
    let silky = matches!(
        b,
        Block::STONE
            | Block::GRASS
            | Block::COAL_ORE
            | Block::IRON_ORE
            | Block::GOLD_ORE
            | Block::DIAMOND_ORE
            | Block::COPPER_ORE
            | Block::REDSTONE_ORE
            | Block::EMERALD_ORE
            | Block::QUARTZ_ORE
            | Block::LAPIS_ORE
            | Block::GLASS
            | Block::GRAVEL
            | Block::GLOWSTONE
            | Block::ICE
            | Block::BOOKSHELF
            | Block::CLAY
            | Block::MELON
            | Block::COBWEB
            | Block::DEEPSLATE
    ) || b.is_leaves()
        || b.is_deepslate_ore();
    (silky && Item::from(b).is_valid()).then(|| Item::from(b))
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
    fn netherite_blocks_need_diamond_pickaxes_and_debris_gives_no_mining_xp() {
        for block in [Block::ANCIENT_DEBRIS, Block::NETHERITE_BLOCK] {
            for tier in [Tier::Wood, Tier::Stone, Tier::Iron, Tier::Gold] {
                assert!(!can_harvest(block, tool(ToolKind::Pickaxe, tier)));
            }
            assert!(!can_harvest(block, None));
            assert!(!can_harvest(block, tool(ToolKind::Axe, Tier::Diamond)));
            assert!(can_harvest(block, tool(ToolKind::Pickaxe, Tier::Diamond)));
            assert_eq!(block.drop(), Some(block.into()));
        }
        assert_eq!(break_time(Block::ANCIENT_DEBRIS, tool(ToolKind::Pickaxe, Tier::Diamond)), 5.625);
        // Netherite harvests everything diamond does, a little faster.
        let netherite = tool(ToolKind::Pickaxe, Tier::Netherite);
        for block in [Block::OBSIDIAN, Block::ANCIENT_DEBRIS, Block::NETHERITE_BLOCK, Block::DIAMOND_ORE] {
            assert!(can_harvest(block, netherite), "{block:?}");
        }
        assert_eq!(break_time(Block::OBSIDIAN, netherite), 50.0 * 1.5 / 9.0);
        assert_eq!(attack_damage(tool(ToolKind::Sword, Tier::Netherite)), 8.0);
        assert_eq!(attack_damage(tool(ToolKind::Shovel, Tier::Netherite)), 6.5);
        assert_eq!(attack_damage(tool(ToolKind::Pickaxe, Tier::Netherite)), 6.0);
        assert_eq!(attack_damage(tool(ToolKind::Axe, Tier::Netherite)), 10.0);
        assert_eq!(attack_damage(tool(ToolKind::Hoe, Tier::Netherite)), 1.0);
        assert_eq!(attack_speed(tool(ToolKind::Sword, Tier::Netherite)), 1.6);
        assert_eq!(attack_speed(tool(ToolKind::Shovel, Tier::Netherite)), 1.0);
        assert_eq!(attack_speed(tool(ToolKind::Pickaxe, Tier::Netherite)), 1.2);
        assert_eq!(attack_speed(tool(ToolKind::Axe, Tier::Netherite)), 1.0);
        assert_eq!(attack_speed(tool(ToolKind::Hoe, Tier::Netherite)), 4.0);
        assert_eq!(break_time(Block::NETHERITE_BLOCK, tool(ToolKind::Pickaxe, Tier::Diamond)), 9.375);
        let mut rng = crate::entity::Rng::new(17);
        assert_eq!(ore_xp(Block::ANCIENT_DEBRIS, &mut rng), 0);
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
    fn silk_touch_keeps_spawner_xp_but_suppresses_ore_xp() {
        let silk = Enchants::NONE.with(Enchantment::SilkTouch, 1);
        let mut rng = crate::entity::Rng::new(1);
        assert_eq!(mined_xp(Block::DIAMOND_ORE, silk, &mut rng), 0);
        assert!((3..=7).contains(&mined_xp(Block::DIAMOND_ORE, Enchants::NONE, &mut rng)));
        assert!((15..=43).contains(&mined_xp(Block::SPAWNER, silk, &mut rng)));
    }

    #[test]
    fn efficiency_aqua_affinity_and_footing() {
        use crate::enchant::Enchantment;
        let pick = Item::tool(ToolKind::Pickaxe, Tier::Diamond);
        let dig = |enchants: Enchants, helmet: Enchants, eyes_in_water: bool, on_ground: bool| {
            let held = Some(Stack { enchants, ..Stack::new(pick, 1) });
            dig_time(Block::STONE, Digger { held, helmet, eyes_in_water, on_ground })
        };
        let none = Enchants::NONE;
        assert_eq!(dig(none, none, false, true), break_time(Block::STONE, Some(pick)));
        // Efficiency V adds 26 to a diamond pickaxe's 8.
        let eff = none.with(Enchantment::Efficiency, 5);
        assert!((dig(eff, none, false, true) - 1.5 * 1.5 / 34.0).abs() < 1e-6);
        let base = dig(none, none, false, true);
        assert!((dig(none, none, true, true) - base * 5.0).abs() < 1e-5, "underwater");
        assert!((dig(none, none, false, false) - base * 5.0).abs() < 1e-5, "airborne");
        let aqua = none.with(Enchantment::AquaAffinity, 1);
        assert!((dig(none, aqua, true, true) - base).abs() < 1e-6);
        // Efficiency doesn't help the wrong tool.
        let held = Some(Stack { enchants: eff, ..Stack::new(pick, 1) });
        let d = Digger { held, helmet: none, eyes_in_water: false, on_ground: true };
        assert_eq!(dig_time(Block::LOG, d), break_time(Block::LOG, None));
        assert_eq!(silk_drop(Block::DIAMOND_ORE), Some(Item::from(Block::DIAMOND_ORE)));
        assert_eq!(silk_drop(Block::SNOWY_GRASS), Some(Item::from(Block::GRASS)));
        assert_eq!(silk_drop(Block::DIRT), None);
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

    #[test]
    fn only_some_ores_give_experience() {
        let mut rng = crate::entity::Rng::new(1);
        let rolls: Vec<u32> = (0..500).map(|_| ore_xp(Block::DIAMOND_ORE, &mut rng)).collect();
        assert_eq!((rolls.iter().min(), rolls.iter().max()), (Some(&3), Some(&7)));
        assert!((0..100).all(|_| ore_xp(Block::COAL_ORE, &mut rng) <= 2));
        assert_eq!(ore_xp(Block::IRON_ORE, &mut rng) + ore_xp(Block::STONE, &mut rng), 0);
    }
}
