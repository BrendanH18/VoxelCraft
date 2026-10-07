//! Smithing recipes. Netherite transforms are deliberately separate from
//! crafting: one upgrade template, one piece of diamond gear and one
//! Netherite ingot make the matching Netherite item.

use crate::inventory::Stack;
use crate::item::{ArmorMaterial, Item, Tier};

pub fn is_template(item: Item) -> bool {
    item == Item::NETHERITE_UPGRADE
}

pub fn is_base(item: Item) -> bool {
    matches!(item.as_tool(), Some((_, Tier::Diamond))) || matches!(item.as_armor(), Some((_, ArmorMaterial::Diamond)))
}

pub fn is_addition(item: Item) -> bool {
    item == Item::NETHERITE_INGOT
}

/// Applies the Netherite upgrade recipe. The result copies the base stack's
/// damage, enchantments, prior-work penalty and custom name, like Java's
/// `SmithingTransformRecipe`.
pub fn upgrade(template: Stack, base: Stack, addition: Stack) -> Option<Stack> {
    if !is_template(template.item) || !is_addition(addition.item) {
        return None;
    }
    let item = match base.item.as_tool() {
        Some((kind, Tier::Diamond)) => Item::tool(kind, Tier::Netherite),
        _ => match base.item.as_armor() {
            Some((piece, ArmorMaterial::Diamond)) => Item::armor(piece, ArmorMaterial::Netherite),
            _ => return None,
        },
    };
    Some(Stack { item, count: 1, ..base })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enchant::{Enchantment, Enchants};
    use crate::item::{ArmorPiece, ToolKind};

    fn ingredients(base: Stack) -> (Stack, Stack, Stack) {
        (Stack::new(Item::NETHERITE_UPGRADE, 1), base, Stack::new(Item::NETHERITE_INGOT, 1))
    }

    #[test]
    fn upgrades_every_diamond_tool_and_armor_piece_only() {
        for kind in [ToolKind::Sword, ToolKind::Shovel, ToolKind::Pickaxe, ToolKind::Axe, ToolKind::Hoe] {
            let input = Stack::new(Item::tool(kind, Tier::Diamond), 1);
            let (template, base, addition) = ingredients(input);
            assert_eq!(upgrade(template, base, addition).unwrap().item, Item::tool(kind, Tier::Netherite));
        }
        for piece in ArmorPiece::ALL {
            let input = Stack::new(Item::armor(piece, ArmorMaterial::Diamond), 1);
            let (template, base, addition) = ingredients(input);
            assert_eq!(upgrade(template, base, addition).unwrap().item, Item::armor(piece, ArmorMaterial::Netherite));
        }

        let diamond = Stack::new(Item::DIAMOND, 1);
        let (template, _, addition) = ingredients(diamond);
        assert_eq!(upgrade(template, diamond, addition), None);
        assert_eq!(upgrade(Stack::new(Item::PAPER, 1), diamond, addition), None);
        assert_eq!(upgrade(template, diamond, Stack::new(Item::GOLD_INGOT, 1)), None);
    }

    #[test]
    fn transform_preserves_every_base_stack_component() {
        let base = Stack {
            damage: 417,
            enchants: Enchants::NONE
                .with(Enchantment::Efficiency, 5)
                .with(Enchantment::Unbreaking, 3)
                .with(Enchantment::Mending, 1),
            repair_cost: 31,
            ..Stack::new(Item::tool(ToolKind::Pickaxe, Tier::Diamond), 1).with_name("Silk Digger").unwrap()
        };
        let (template, base, addition) = ingredients(base);
        let result = upgrade(template, base, addition).unwrap();
        assert_eq!(result.item, Item::tool(ToolKind::Pickaxe, Tier::Netherite));
        assert_eq!(
            (result.damage, result.enchants, result.repair_cost),
            (base.damage, base.enchants, base.repair_cost)
        );
        assert_eq!(result.display_name(), "Silk Digger");
    }
}
