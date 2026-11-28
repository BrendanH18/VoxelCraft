//! v1.0 gap track 6b: Java 26.3's advanced item tooltips (F3+H,
//! `Options.advancedItemTooltips`). `ItemStack.addDetailsToTooltip` ends a
//! tooltip with, when the flag is on: "Durability: left / max" for a damaged
//! stack (white), the item's registry id (dark grey) and
//! "N component(s)" (dark grey), N being the size of the stack's component
//! map: its item's default components
//! (`tests/fixtures/java/26.3/item_components.json`, from the data
//! generator's reports) plus each component the stack sets that the item
//! doesn't have by default.
//!
//! Java 26.3's lang file still has `item.minecraft.bundle.fullness`
//! ("%s/%s"), but no code uses it: a bundle's advanced tooltip shows no
//! fullness line, so this doesn't either.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde_json::Value;

use crate::inventory::Stack;
use crate::inventory::components::Payload;
use crate::item::Item;

/// The colour of a line: Java's default white, or `ChatFormatting.DARK_GRAY`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Colour {
    White,
    DarkGray,
}

impl Colour {
    /// sRGB, as the UI takes colours.
    pub fn rgb(self) -> [u8; 3] {
        match self {
            Colour::White => [255, 255, 255],
            Colour::DarkGray => [0x55, 0x55, 0x55],
        }
    }
}

struct Tables {
    ids: HashMap<Item, &'static str>,
    defaults: HashMap<String, (usize, Vec<String>)>,
}

fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        static NAMES: OnceLock<Value> = OnceLock::new();
        let names = NAMES.get_or_init(|| {
            serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/registry_names.json")).unwrap_or_default()
        });
        let mut ids = HashMap::new();
        for (id, english) in names["items"].as_object().into_iter().flatten() {
            if let Some(item) = crate::java_registry::item(id, english.as_str().unwrap_or_default()) {
                ids.entry(item).or_insert(id.as_str());
            }
        }
        let fixture: Value = serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/item_components.json"))
            .unwrap_or_default();
        let defaults = fixture["items"]
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(id, v)| {
                let count = v[0].as_u64()? as usize;
                let tracked = v[1].as_array()?.iter().filter_map(|c| c.as_str().map(str::to_string)).collect();
                Some((id.clone(), (count, tracked)))
            })
            .collect();
        Tables { ids, defaults }
    })
}

/// The Java registry id (without `minecraft:`) of a stack's item: the id
/// it resolves from, the potion families' shared ids, or its name with
/// underscores.
pub fn java_id(stack: &Stack) -> String {
    let item = stack.item;
    if let Some(id) = tables().ids.get(&item) {
        return (*id).to_string();
    }
    if item.as_potion().is_some() {
        "potion".into()
    } else if item.as_splash_potion().is_some() {
        "splash_potion".into()
    } else if stack.lingering_contents().is_some() || item == Item::LINGERING_POTION {
        "lingering_potion".into()
    } else if stack.arrow_potion().is_some() {
        "tipped_arrow".into()
    } else {
        item.name().replace(' ', "_")
    }
}

/// The components a stack sets, by Java name (only those the fixture tracks).
pub fn set_components(stack: &Stack) -> Vec<&'static str> {
    let mut out = Vec::new();
    if stack.name.as_str().is_some_and(|n| !n.is_empty()) {
        out.push("custom_name");
    }
    if stack.damage > 0 {
        out.push("damage");
    }
    if !stack.enchants.is_empty() {
        out.push(if stack.item == Item::ENCHANTED_BOOK { "stored_enchantments" } else { "enchantments" });
    }
    if stack.repair_cost > 0 {
        out.push("repair_cost");
    }
    if stack.lodestone.is_some() {
        out.push("lodestone_tracker");
    }
    if stack.map_id.is_some() {
        out.push("map_id");
    }
    if stack.trim.is_some() {
        out.push("trim");
    }
    match stack.payload() {
        Some(Payload::Bundle(_)) => out.push("bundle_contents"),
        Some(Payload::WritableBook(_)) => out.push("writable_book_content"),
        Some(Payload::WrittenBook(_)) => out.push("written_book_content"),
        Some(Payload::Banner(p)) => {
            if !p.layers.is_empty() {
                out.push("banner_patterns");
            }
            if p.base.is_some() {
                out.push("base_color");
            }
        }
        Some(Payload::Container(_)) => out.push("container"),
        Some(Payload::Fireworks(_)) if stack.item == Item::CHARGED_CROSSBOW => out.push("charged_projectiles"),
        Some(Payload::Fireworks(_)) => out.push("fireworks"),
        Some(Payload::Explosion(_)) => out.push("firework_explosion"),
        None => {}
    }
    out
}

/// `components.size()`: the item's defaults plus what the stack adds.
/// `None` for an item the fixture doesn't list.
pub fn component_count(stack: &Stack) -> Option<usize> {
    let (count, tracked) = tables().defaults.get(&java_id(stack))?;
    Some(count + set_components(stack).into_iter().filter(|c| !tracked.iter().any(|t| t == c)).count())
}

/// The advanced lines Java adds at the end of a stack's tooltip.
pub fn lines(stack: &Stack) -> Vec<(String, Colour)> {
    let mut out = Vec::new();
    if let Some(max) = stack.item.durability().filter(|_| stack.damage > 0) {
        out.push((format!("Durability: {} / {max}", max.saturating_sub(stack.damage)), Colour::White));
    }
    out.push((format!("minecraft:{}", java_id(stack)), Colour::DarkGray));
    if let Some(n) = component_count(stack).filter(|&n| n > 0) {
        out.push((format!("{n} component(s)"), Colour::DarkGray));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_and_patched_stacks_count_javas_components() {
        // Stone's 13 defaults (`minecraft/components/item/stone.json`).
        let stone = Stack::new(Item::from_block(crate::world::block::Block::STONE), 1);
        assert_eq!(
            lines(&stone),
            [("minecraft:stone".to_string(), Colour::DarkGray), ("13 component(s)".to_string(), Colour::DarkGray)]
        );
        // A diamond sword has `damage` by default, so wear doesn't add one,
        // but its durability line shows.
        let mut sword = Stack::new(Item::tool(crate::item::ToolKind::Sword, crate::item::Tier::Diamond), 1);
        let plain = component_count(&sword).unwrap();
        sword.damage = 61;
        assert_eq!(component_count(&sword), Some(plain));
        let max = sword.item.durability().unwrap();
        assert_eq!(lines(&sword)[0], (format!("Durability: {} / {max}", max - 61), Colour::White));
        // A custom name is a component no item has by default.
        let named = Stack { name: crate::inventory::StackName::new("Pick").unwrap(), ..stone };
        assert_eq!(component_count(&named), Some(14));
        // A bundle's contents replace its default (empty) contents.
        let bundle = Stack::new(crate::content::bundle::BUNDLE, 1);
        let filled = bundle.with_payload(Payload::Bundle(vec![stone]));
        assert_eq!(component_count(&filled), component_count(&bundle));
        assert!(lines(&filled).iter().all(|(l, _)| !l.contains('/')), "no fullness line");
    }

    #[test]
    fn every_resolved_item_has_its_java_id_and_count() {
        let mut checked = 0;
        for (item, id) in &tables().ids {
            assert_eq!(java_id(&Stack::new(*item, 1)), *id);
            assert!(component_count(&Stack::new(*item, 1)).is_some(), "{id}");
            checked += 1;
        }
        assert!(checked > 1600, "{checked}");
    }
}
