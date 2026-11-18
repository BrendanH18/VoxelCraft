//! v0.15 Track D: armor trims (Java 26.3 `SmithingTemplateItem`,
//! `SmithingTrimRecipe`, `TrimMaterials`, `TrimPatterns` and
//! `data/minecraft/recipe/*_armor_trim_smithing_template*.json`).
//!
//! The eighteen trim smithing templates are items 1601..=1618 in
//! [`TrimPattern::ALL`] order (alphabetical, as Java's registry names sort).
//! A template, a piece of `#trimmable_armor` and a `#trim_materials` item
//! make the armor with the template's pattern in the material's colour
//! (the stack's `trim` component, [`Trim`]); seven diamonds, the template
//! and the template's own block copy it. Patterns are drawn from original
//! masks (see [`mask`]); materials tint them with Java's palette colours
//! and the darker palette on armor of the same material
//! (`assets/minecraft/equipment/*.json` `trim_overrides`).

use crate::inventory::Stack;
use crate::inventory::components::{Trim, TrimMaterial, TrimPattern, trimmable};
use crate::item::{ArmorMaterial, Item, ItemInfo, ItemKind, Sprite};

/// The bolt template; the other seventeen follow in [`TrimPattern::ALL`] order.
pub const ITEM_FIRST: u16 = 1601;
pub const ITEM_END: u16 = ITEM_FIRST + 18;
const _: () = assert!(ITEM_END == 1619);

/// The template that applies `pattern`.
pub const fn template(pattern: TrimPattern) -> Item {
    Item(ITEM_FIRST + pattern as u16)
}

/// The pattern a trim template applies.
pub fn template_pattern(item: Item) -> Option<TrimPattern> {
    let i = item.0.checked_sub(ITEM_FIRST).filter(|&i| i < 18)?;
    Some(TrimPattern::ALL[i as usize])
}

/// Every template, in id order.
pub fn all() -> impl Iterator<Item = Item> {
    (ITEM_FIRST..ITEM_END).map(Item)
}

/// Java's `Rarity` of a template (`Items`): ward, eye, vex and spire are
/// rare, silence epic, the rest uncommon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rarity {
    Uncommon,
    Rare,
    Epic,
}

impl Rarity {
    /// `Rarity.color`: yellow, aqua, light purple.
    pub fn colour(self) -> [u8; 3] {
        match self {
            Rarity::Uncommon => [0xFF, 0xFF, 0x55],
            Rarity::Rare => [0x55, 0xFF, 0xFF],
            Rarity::Epic => [0xFF, 0x55, 0xFF],
        }
    }
}

pub fn rarity(pattern: TrimPattern) -> Rarity {
    match pattern {
        TrimPattern::Ward | TrimPattern::Eye | TrimPattern::Vex | TrimPattern::Spire => Rarity::Rare,
        TrimPattern::Silence => Rarity::Epic,
        _ => Rarity::Uncommon,
    }
}

const NAMES: [&str; 18] = [
    "bolt armor trim smithing template",
    "coast armor trim smithing template",
    "dune armor trim smithing template",
    "eye armor trim smithing template",
    "flow armor trim smithing template",
    "host armor trim smithing template",
    "raiser armor trim smithing template",
    "rib armor trim smithing template",
    "sentry armor trim smithing template",
    "shaper armor trim smithing template",
    "silence armor trim smithing template",
    "snout armor trim smithing template",
    "spire armor trim smithing template",
    "tide armor trim smithing template",
    "vex armor trim smithing template",
    "ward armor trim smithing template",
    "wayfinder armor trim smithing template",
    "wild armor trim smithing template",
];

/// A template's item info: 64 to a stack, like Java's smithing templates.
pub(crate) fn item_info(item: Item) -> Option<ItemInfo> {
    let pattern = template_pattern(item)?;
    Some(ItemInfo {
        name: NAMES[pattern as usize],
        kind: ItemKind::Material,
        max_stack: 64,
        sprite: Sprite::TrimTemplate(pattern as u8),
    })
}

/// en_us `trim_pattern.minecraft.*`.
pub fn pattern_title(pattern: TrimPattern) -> &'static str {
    const TITLES: [&str; 18] = [
        "Bolt Armor Trim",
        "Coast Armor Trim",
        "Dune Armor Trim",
        "Eye Armor Trim",
        "Flow Armor Trim",
        "Host Armor Trim",
        "Raiser Armor Trim",
        "Rib Armor Trim",
        "Sentry Armor Trim",
        "Shaper Armor Trim",
        "Silence Armor Trim",
        "Snout Armor Trim",
        "Spire Armor Trim",
        "Tide Armor Trim",
        "Vex Armor Trim",
        "Ward Armor Trim",
        "Wayfinder Armor Trim",
        "Wild Armor Trim",
    ];
    TITLES[pattern as usize]
}

/// en_us `trim_material.minecraft.*`.
pub fn material_title(material: TrimMaterial) -> &'static str {
    const TITLES: [&str; 11] = [
        "Amethyst Material",
        "Copper Material",
        "Diamond Material",
        "Emerald Material",
        "Gold Material",
        "Iron Material",
        "Lapis Material",
        "Netherite Material",
        "Quartz Material",
        "Redstone Material",
        "Resin Material",
    ];
    TITLES[material as usize]
}

/// `TrimMaterials.bootstrap`: the description colour of each material,
/// which its tooltip lines take.
pub fn material_colour(material: TrimMaterial) -> [u8; 3] {
    let c: u32 = match material {
        TrimMaterial::Amethyst => 10116294,
        TrimMaterial::Copper => 11823181,
        TrimMaterial::Diamond => 7269586,
        TrimMaterial::Emerald => 1155126,
        TrimMaterial::Gold => 14594349,
        TrimMaterial::Iron => 15527148,
        TrimMaterial::Lapis => 4288151,
        TrimMaterial::Netherite => 6445145,
        TrimMaterial::Quartz => 14931140,
        TrimMaterial::Redstone => 9901575,
        TrimMaterial::Resin => 16545810,
    };
    [(c >> 16) as u8, (c >> 8) as u8, c as u8]
}

/// The armor material whose equipment asset swaps `material`'s palette
/// for its `_darker` one (`trim_overrides` in `equipment/iron.json`,
/// `gold.json`, `diamond.json`, `netherite.json` and `copper.json`).
pub fn darker_on(material: TrimMaterial) -> Option<ArmorMaterial> {
    Some(match material {
        TrimMaterial::Iron => ArmorMaterial::Iron,
        TrimMaterial::Gold => ArmorMaterial::Gold,
        TrimMaterial::Diamond => ArmorMaterial::Diamond,
        TrimMaterial::Netherite => ArmorMaterial::Netherite,
        TrimMaterial::Copper => ArmorMaterial::Copper,
        _ => return None,
    })
}

/// The colour the trim layer is tinted with on armor of `armor`: the
/// material's colour, three quarters as bright for the darker palette.
pub fn layer_colour(trim: Trim, armor: ArmorMaterial) -> [u8; 3] {
    let c = material_colour(trim.material);
    if darker_on(trim.material) == Some(armor) { c.map(|v| (u16::from(v) * 3 / 4) as u8) } else { c }
}

/// The item of each trim material (`Items`, `Properties.trimMaterial`):
/// the `#trim_materials` tag.
pub fn material_items() -> &'static [(Item, TrimMaterial)] {
    static TABLE: std::sync::OnceLock<Vec<(Item, TrimMaterial)>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        [
            ("amethyst_shard", TrimMaterial::Amethyst),
            ("copper_ingot", TrimMaterial::Copper),
            ("diamond", TrimMaterial::Diamond),
            ("emerald", TrimMaterial::Emerald),
            ("gold_ingot", TrimMaterial::Gold),
            ("iron_ingot", TrimMaterial::Iron),
            ("lapis_lazuli", TrimMaterial::Lapis),
            ("netherite_ingot", TrimMaterial::Netherite),
            ("quartz", TrimMaterial::Quartz),
            ("redstone", TrimMaterial::Redstone),
            ("resin_brick", TrimMaterial::Resin),
        ]
        .into_iter()
        .map(|(id, m)| (crate::java_registry::item(id, "").unwrap_or_else(|| panic!("trim material {id}")), m))
        .collect()
    })
}

/// `DataComponents.PROVIDES_TRIM_MATERIAL`.
pub fn provides_material(item: Item) -> Option<TrimMaterial> {
    material_items().iter().find(|(i, _)| *i == item).map(|&(_, m)| m)
}

/// `SmithingTrimRecipe.applyTrim`: the base armor, one of it, with the
/// template's pattern in the addition's material, keeping every other
/// component. Nothing when the base already has that very trim.
pub fn apply(template: Stack, base: Stack, addition: Stack) -> Option<Stack> {
    let pattern = template_pattern(template.item)?;
    if !trimmable(base.item) {
        return None;
    }
    let material = provides_material(addition.item)?;
    let trim = Trim { pattern, material };
    (base.trim != Some(trim)).then_some(Stack { count: 1, trim: Some(trim), ..base })
}

/// The block each template's copy recipe needs besides seven diamonds
/// (`<pattern>_armor_trim_smithing_template.json`, key `C`), as Java ids and
/// en_us names; bolt takes either copper block.
pub fn copy_ingredients(pattern: TrimPattern) -> &'static [(&'static str, &'static str)] {
    match pattern {
        TrimPattern::Bolt => &[("copper_block", "Block of Copper"), ("waxed_copper_block", "Waxed Block of Copper")],
        TrimPattern::Coast | TrimPattern::Sentry | TrimPattern::Vex => &[("cobblestone", "Cobblestone")],
        TrimPattern::Dune => &[("sandstone", "Sandstone")],
        TrimPattern::Eye => &[("end_stone", "End Stone")],
        TrimPattern::Flow => &[("breeze_rod", "Breeze Rod")],
        TrimPattern::Host | TrimPattern::Raiser | TrimPattern::Shaper | TrimPattern::Wayfinder => {
            &[("terracotta", "Terracotta")]
        }
        TrimPattern::Rib => &[("netherrack", "Netherrack")],
        TrimPattern::Silence | TrimPattern::Ward => &[("cobbled_deepslate", "Cobbled Deepslate")],
        TrimPattern::Snout => &[("blackstone", "Blackstone")],
        TrimPattern::Spire => &[("purpur_block", "Purpur Block")],
        TrimPattern::Tide => &[("prismarine", "Prismarine")],
        TrimPattern::Wild => &[("mossy_cobblestone", "Mossy Cobblestone")],
    }
}

/// The copy recipe's block items, or `None` while one isn't registered
/// (the purpur block is a registry gap until End content lands).
pub fn copy_items(pattern: TrimPattern) -> Option<Vec<Item>> {
    copy_ingredients(pattern).iter().map(|(id, english)| crate::java_registry::item(id, english)).collect()
}

/// One tooltip line and its colour.
pub type Line = (String, [u8; 3]);

const GRAY: [u8; 3] = [0xAA, 0xAA, 0xAA];
const BLUE: [u8; 3] = [0x55, 0x55, 0xFF];

/// The lines under a stack's name: Java's `ArmorTrim.addToTooltip` for
/// trimmed armor ("Upgrade:", the pattern and the material in the
/// material's colour) and `SmithingTemplateItem.appendHoverText` for a
/// template.
pub fn tooltip_lines(stack: &Stack) -> Vec<Line> {
    if let Some(pattern) = template_pattern(stack.item) {
        return vec![
            (pattern_title(pattern).into(), GRAY),
            (String::new(), GRAY),
            ("Applies to:".into(), GRAY),
            (" Armor".into(), BLUE),
            ("Ingredients:".into(), GRAY),
            (" Ingots & Crystals".into(), BLUE),
        ];
    }
    let Some(trim) = stack.trim else { return Vec::new() };
    let colour = material_colour(trim.material);
    vec![
        ("Upgrade: ".into(), GRAY),
        (format!(" {}", pattern_title(trim.pattern)), colour),
        (format!(" {}", material_title(trim.material)), colour),
    ]
}

/// The tooltip title's colour: a template's rarity, else `None`.
pub fn title_colour(item: Item) -> Option<[u8; 3]> {
    template_pattern(item).map(|p| rarity(p).colour())
}

/// Which armor sheet a trim pattern's mask covers: the outer sheet
/// (helmet, chestplate, boots) or the inner one (leggings).
pub fn mask(pattern: TrimPattern, inner: bool, at: [usize; 2]) -> Option<u8> {
    let [x, y] = at;
    if x >= 64 || y >= 32 {
        return None;
    }
    let p = pattern as u32;
    // An original motif per pattern: a few line families over the 64x32
    // armor net, picked and phased by the pattern so that no two patterns
    // share a mask. Brightness varies along the line like Java's grey
    // trim masks, which the palette maps to light and shade.
    let (xi, yi) = (x as i32, y as i32);
    let h = |a: i32, b: i32| ((a.wrapping_mul(73) ^ b.wrapping_mul(151) ^ (p as i32 * 37)) & 7) as u8;
    let phase = (p * 5 + if inner { 3 } else { 0 }) as i32;
    let family = PATTERN_FAMILIES[pattern as usize];
    let hit = family.iter().any(|&f| match f {
        Motif::Bands(step) => (yi + phase).rem_euclid(step) == 0,
        Motif::Columns(step) => (xi + phase).rem_euclid(step) == 0,
        Motif::Diagonal(step) => (xi + yi + phase).rem_euclid(step) == 0,
        Motif::Anti(step) => (xi - yi + phase).rem_euclid(step) == 0,
        Motif::Dots(step) => (xi + phase).rem_euclid(step) == 1 && (yi + phase).rem_euclid(step) == 1,
        Motif::Rim => {
            // Seams of each box face in the armor net.
            let lx = xi.rem_euclid(8);
            let ly = yi.rem_euclid(8);
            lx == 0 || ly == 0
        }
        Motif::Chevron(step) => (yi - (xi.rem_euclid(8) - 4).abs() + phase).rem_euclid(step) == 0,
    });
    hit.then(|| 150 + h(xi, yi) * 13)
}

/// The line families of each pattern's original mask.
#[derive(Clone, Copy)]
enum Motif {
    Bands(i32),
    Columns(i32),
    Diagonal(i32),
    Anti(i32),
    Dots(i32),
    Rim,
    Chevron(i32),
}

const PATTERN_FAMILIES: [&[Motif]; 18] = [
    &[Motif::Chevron(5), Motif::Columns(9)],       // bolt
    &[Motif::Bands(4)],                            // coast
    &[Motif::Diagonal(5), Motif::Bands(7)],        // dune
    &[Motif::Dots(4), Motif::Rim],                 // eye
    &[Motif::Anti(4)],                             // flow
    &[Motif::Rim, Motif::Bands(6)],                // host
    &[Motif::Columns(5), Motif::Dots(3)],          // raiser
    &[Motif::Bands(3), Motif::Columns(4)],         // rib
    &[Motif::Rim, Motif::Diagonal(6)],             // sentry
    &[Motif::Chevron(4)],                          // shaper
    &[Motif::Rim, Motif::Anti(5), Motif::Dots(5)], // silence
    &[Motif::Dots(3), Motif::Bands(5)],            // snout
    &[Motif::Columns(3), Motif::Chevron(6)],       // spire
    &[Motif::Anti(6), Motif::Bands(4)],            // tide
    &[Motif::Diagonal(4), Motif::Anti(4)],         // vex
    &[Motif::Rim, Motif::Columns(6)],              // ward
    &[Motif::Diagonal(3)],                         // wayfinder
    &[Motif::Chevron(3), Motif::Dots(6)],          // wild
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::ArmorPiece;

    #[test]
    fn templates_round_trip_and_masks_are_distinct() {
        for (i, pattern) in TrimPattern::ALL.into_iter().enumerate() {
            let t = template(pattern);
            assert_eq!(t, Item(1601 + i as u16));
            assert_eq!(template_pattern(t), Some(pattern));
            assert_eq!(t.name(), format!("{}_armor_trim_smithing_template", pattern.name()).replace('_', " "));
            assert_eq!(Item::from_name(&format!("{}_armor_trim_smithing_template", pattern.name())), Some(t));
        }
        let sheet = |p: TrimPattern, inner: bool| {
            (0..32).flat_map(|y| (0..64).map(move |x| mask(p, inner, [x, y]).is_some())).collect::<Vec<_>>()
        };
        for a in TrimPattern::ALL {
            assert!(sheet(a, false).iter().filter(|&&b| b).count() > 64, "{a:?} is too sparse");
            for b in TrimPattern::ALL.into_iter().filter(|&b| b != a) {
                assert_ne!(sheet(a, false), sheet(b, false), "{a:?} and {b:?} share a mask");
            }
        }
    }

    #[test]
    fn applying_a_trim_keeps_the_armor_and_refuses_the_same_trim() {
        let base = Stack {
            damage: 12,
            ..Stack::new(Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Iron), 1).with_name("Mine").unwrap()
        };
        let template = Stack::new(template(TrimPattern::Coast), 3);
        let gold = Stack::new(crate::java_registry::item("gold_ingot", "").unwrap(), 5);
        let out = apply(template, base, gold).unwrap();
        assert_eq!(out.trim, Some(Trim { pattern: TrimPattern::Coast, material: TrimMaterial::Gold }));
        assert_eq!((out.count, out.damage, out.display_name()), (1, 12, "Mine"));
        assert_eq!(apply(template, out, gold), None, "the same trim again gives nothing");
        let redstone = Stack::new(crate::java_registry::item("redstone", "").unwrap(), 1);
        assert_eq!(apply(template, out, redstone).unwrap().trim.unwrap().material, TrimMaterial::Redstone);
        assert_eq!(apply(template, Stack::new(Item::STICK, 1), gold), None);
        assert_eq!(apply(Stack::new(Item::STICK, 1), base, gold), None);
        assert_eq!(apply(template, base, Stack::new(Item::STICK, 1)), None);
        // The darker palette on iron armor only for an iron trim.
        let iron = Trim { pattern: TrimPattern::Eye, material: TrimMaterial::Iron };
        assert!(layer_colour(iron, ArmorMaterial::Iron)[0] < layer_colour(iron, ArmorMaterial::Gold)[0]);
    }

    #[test]
    fn tooltips_name_pattern_and_material() {
        let mut s = Stack::new(Item::armor(ArmorPiece::Boots, ArmorMaterial::Diamond), 1);
        s.trim = Some(Trim { pattern: TrimPattern::Silence, material: TrimMaterial::Amethyst });
        let lines = tooltip_lines(&s);
        assert_eq!(lines[1].0, " Silence Armor Trim");
        assert_eq!(lines[2], (" Amethyst Material".into(), [0x9A, 0x5C, 0xC6]));
        let t = tooltip_lines(&Stack::new(template(TrimPattern::Ward), 1));
        assert_eq!(t[0].0, "Ward Armor Trim");
        assert_eq!(title_colour(template(TrimPattern::Silence)), Some([0xFF, 0x55, 0xFF]));
    }
}
