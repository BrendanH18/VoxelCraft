//! Java grindstone repair/disenchantment. Curses and names survive; non-curse
//! enchantments contribute their minimum cost to the randomized XP award.
use crate::enchant::{AnvilResult, Enchants};
use crate::inventory::Stack;
use crate::item::Item;

pub fn accepts(item: Item) -> bool {
    item.durability().is_some() || item == Item::ENCHANTED_BOOK
}
pub fn result(left: Option<Stack>, right: Option<Stack>) -> Option<AnvilResult> {
    let mut out = left.or(right)?;
    if left.into_iter().chain(right).any(|s| s.count != 1 || !accepts(s.item) && s.enchants.is_empty()) {
        return None;
    }
    if let (Some(a), Some(b)) = (left, right) {
        if a.item != b.item {
            return None;
        }
        if let Some(max) = a.item.durability() {
            let remaining = u32::from(max.saturating_sub(a.damage))
                + u32::from(max.saturating_sub(b.damage))
                + u32::from(max) * 5 / 100;
            out.damage = u32::from(max).saturating_sub(remaining) as u16;
        } else {
            if a != b {
                return None;
            }
            out.count = 2;
        }
        for (e, l) in b.enchants.iter().filter(|(e, _)| e.def().curse) {
            if !out.enchants.has(e) {
                out.enchants.set(e, l);
            }
        }
    } else if out.enchants.is_empty() {
        return None;
    }
    let curses = out.enchants.iter().filter(|(e, _)| e.def().curse).fold(Enchants::NONE, |a, (e, l)| a.with(e, l));
    out.enchants = curses;
    out.repair_cost = (1u16 << curses.iter().count()) - 1;
    if out.item == Item::ENCHANTED_BOOK && curses.is_empty() {
        out.item = Item::BOOK;
    }
    Some(AnvilResult { output: out, cost: 0, uses: None })
}
pub fn xp(left: Option<Stack>, right: Option<Stack>, roll: f32) -> u32 {
    let sum: u32 = left
        .into_iter()
        .chain(right)
        .flat_map(|s| s.enchants.iter())
        .filter(|(e, _)| !e.def().curse)
        .map(|(e, l)| e.minimum_cost(l))
        .sum();
    let half = sum.div_ceil(2);
    half + ((half as f32 * roll.clamp(0.0, 0.999999)) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enchant::Enchantment as E;
    use crate::item::{Tier, ToolKind};
    #[test]
    fn repairs_with_five_percent_bonus_and_keeps_curses_names_only() {
        let item = Item::tool(ToolKind::Pickaxe, Tier::Iron);
        let a = Stack {
            damage: 200,
            enchants: Enchants::NONE.with(E::Efficiency, 3).with(E::VanishingCurse, 1),
            repair_cost: 31,
            ..Stack::new(item, 1).with_name("Old friend").unwrap()
        };
        let b = Stack { damage: 200, enchants: Enchants::NONE.with(E::Unbreaking, 2), ..Stack::new(item, 1) };
        let r = result(Some(a), Some(b)).unwrap().output;
        assert_eq!(r.damage, 138); // 250 - (50 + 50 + floor(250 * .05))
        assert_eq!(r.enchants, Enchants::NONE.with(E::VanishingCurse, 1));
        assert_eq!(r.display_name(), "Old friend");
        assert_eq!(r.repair_cost, 1);
        assert_eq!(xp(Some(a), Some(b), 0.0), 17); // Efficiency III 21 + Unbreaking II 13
        assert_eq!(xp(Some(a), Some(b), 0.9999), 33);
        assert!(result(Some(a), Some(Stack::new(Item::BOOK, 1))).is_none());
    }
    #[test]
    fn handles_either_input_books_and_no_xp_from_curses() {
        let book = Stack { enchants: Enchants::NONE.with(E::Mending, 1), ..Stack::new(Item::ENCHANTED_BOOK, 1) };
        assert_eq!(result(None, Some(book)).unwrap().output.item, Item::BOOK);
        let cursed = Stack { enchants: Enchants::NONE.with(E::BindingCurse, 1), ..book };
        assert_eq!(result(Some(cursed), None).unwrap().output.item, Item::ENCHANTED_BOOK);
        assert_eq!(xp(Some(cursed), None, 0.5), 0);
        assert!(result(Some(Stack::new(Item::tool(ToolKind::Axe, Tier::Iron), 1)), None).is_none());
    }
}
