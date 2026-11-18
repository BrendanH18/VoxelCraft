//! Java 26.3 villager and wandering trader trades.
//!
//! The tables follow the default 26.3 data (`data/minecraft/trade_set` and
//! the `#villager_trade/...` tags they draw from, in tag order, with each
//! `data/minecraft/villager_trade` entry); the optional `trade_rebalance`
//! data pack is not applied. Items are Java registry names, resolved with
//! [`crate::java_registry::item`]; an offer whose items the game lacks is
//! treated like one Java discards, and another is drawn in its place.
//! `tests/java_villagers.rs` compares every entry with
//! `tests/fixtures/java/26.3/villagers.json` both ways.
use super::{Offer, Profession};
use crate::color::DyeColor;
use crate::enchant::{Enchantment, JavaRandom};
use crate::inventory::Stack;
use crate::item::Item;
use crate::potion::Potion;

/// The `given_item_modifier` a trade applies to its result.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Modifier {
    None,
    /// `enchant_with_levels` over `#on_traded_equipment` with `levels`
    /// uniform in `min..=max` and the levels added to the price
    /// (`include_additional_cost_component`); discarded unless enchanted.
    Enchant {
        min: u8,
        max: u8,
    },
    /// `enchant_randomly` over `#tradeable`, adding `2 + nextInt(5 + 10 *
    /// level) + 3 * level` to the price, doubled for
    /// `#double_trade_price`; discarded without a stored enchantment.
    Book,
    /// `exploration_map` to a structure tag; resolved when the merchant
    /// screen opens (`Entities::prepare_merchant_maps`).
    Map(&'static str),
    /// `set_random_dyes` with `1 + binomial(2, 0.75)` random dyes.
    Dyes,
    /// `set_potion`.
    Potion(&'static str),
    /// `set_random_potion` over the `#tradeable` potion tag.
    RandomPotion,
    /// `set_stew_effect` over [`STEW_EFFECTS`].
    Stew,
}

/// One `villager_trade` entry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TradeDef {
    /// The Java id, `<profession>/<level>/<name>` or `wandering_trader/<name>`.
    pub id: &'static str,
    pub wants: (&'static str, u8),
    pub also: Option<(&'static str, u8)>,
    pub gives: (&'static str, u8),
    pub max_uses: u8,
    pub xp: u16,
    /// `reputation_discount`, Java's `priceMultiplier`.
    pub mult: f32,
    /// `merchant_predicate`: the villager types offered it; empty for all.
    pub types: &'static [&'static str],
    pub modifier: Modifier,
}

#[allow(clippy::too_many_arguments)]
const fn t(
    id: &'static str,
    wants: &'static str,
    n: u8,
    gives: &'static str,
    count: u8,
    max_uses: u8,
    xp: u16,
    mult: f32,
) -> TradeDef {
    TradeDef {
        id,
        wants: (wants, n),
        also: None,
        gives: (gives, count),
        max_uses,
        xp,
        mult,
        types: &[],
        modifier: Modifier::None,
    }
}

impl TradeDef {
    const fn also(self, item: &'static str, n: u8) -> Self {
        Self { also: Some((item, n)), ..self }
    }
    const fn only(self, types: &'static [&'static str]) -> Self {
        Self { types, ..self }
    }
    const fn with(self, modifier: Modifier) -> Self {
        Self { modifier, ..self }
    }
    /// Whether the game has every item the trade names (a missing one is a
    /// listed registry gap; the offer is then redrawn like a discarded one).
    pub fn available(&self) -> bool {
        item(self.wants.0).is_some()
            && item(self.gives.0).is_some()
            && self.also.is_none_or(|(a, _)| item(a).is_some())
            && match self.modifier {
                Modifier::Potion(p) => Potion::from_id(p).is_some(),
                _ => true,
            }
    }
    /// `VillagerTrade.getOffer`: `None` when the merchant predicate fails,
    /// a modifier discards the result, a cost falls below one, or the game
    /// lacks an item. `variant` is the villager type.
    pub fn offer(&self, seed: u64, variant: &str) -> Option<Offer> {
        if !self.types.is_empty() && !self.types.contains(&variant) {
            return None;
        }
        if !self.available() {
            return None;
        }
        let mut rng = JavaRandom::new(seed as i64);
        let mut output = Stack::new(item(self.gives.0)?, self.gives.1);
        let mut additional = 0i32;
        match self.modifier {
            Modifier::None | Modifier::Map(_) => {}
            Modifier::Enchant { min, max } => {
                let levels = min as i32 + rng.next_bounded((max - min) as i32 + 1);
                for (e, l) in crate::enchant::offer_enchants(seed as i32, output.item, 0, levels as u32) {
                    output.enchants.set(e, l);
                }
                if output.enchants.is_empty() {
                    return None;
                }
                additional = levels;
            }
            Modifier::Book => {
                let tradeable = Enchantment::tradeable();
                let e = tradeable[rng.next_bounded(tradeable.len() as i32) as usize];
                let level = 1 + rng.next_bounded(e.def().max_level as i32);
                output.enchants.set(e, level as u8);
                additional = 2 + rng.next_bounded(5 + level * 10) + 3 * level;
                if double_trade_price(e) {
                    additional *= 2;
                }
            }
            Modifier::Dyes => {
                // `1 + binomial(2, 0.75)`.
                let n = 1 + (0..2).filter(|_| rng.next_float() < 0.75).count();
                let dyes: Vec<DyeColor> =
                    (0..n).map(|_| DyeColor::ALL[rng.next_bounded(DyeColor::ALL.len() as i32) as usize]).collect();
                output = crate::inventory::components::apply_dyes(output, &dyes)?;
            }
            Modifier::Potion(p) => output = Stack::new(Item::potion(Potion::from_id(p)?), self.gives.1),
            Modifier::RandomPotion => {
                let p = TRADEABLE_POTIONS[rng.next_bounded(TRADEABLE_POTIONS.len() as i32) as usize];
                output = Stack::tipped_arrow(Potion::from_id(p)?, self.gives.1);
            }
            Modifier::Stew => {
                output.entity_data = STEW_EFFECTS[rng.next_bounded(STEW_EFFECTS.len() as i32) as usize].2;
            }
        }
        // `TradeCost.toItemCost`: clamped to the item's stack size, then
        // dropped below one.
        let cost = cost_stack(self.wants.0, self.wants.1 as i32 + additional)?;
        let second = match self.also {
            Some((name, n)) => Some(cost_stack(name, n as i32)?),
            None => None,
        };
        Some(Offer {
            cost,
            second,
            output,
            uses: 0,
            max_uses: self.max_uses.max(1),
            xp: self.xp,
            demand: 0,
            multiplier: self.mult.max(0.0),
        })
    }
}

fn cost_stack(name: &str, n: i32) -> Option<Stack> {
    let item = item(name)?;
    let n = n.clamp(0, item.max_stack() as i32);
    (n >= 1).then(|| Stack::new(item, n as u8))
}

/// Whether a villager of type `variant` may hold the explorer map offer
/// for `map` (the `merchant_predicate` of the trade that gives it).
pub fn map_allowed(map: Item, variant: &str) -> bool {
    TRADES
        .iter()
        .filter(|(_, _, d)| matches!(d.modifier, Modifier::Map(_)) && item(d.gives.0) == Some(map))
        .all(|(_, _, d)| d.types.is_empty() || d.types.contains(&variant))
}

/// A trade's Java item name in the game: [`crate::java_registry::item`] with
/// the English names of the trade items whose ids the game doesn't use.
pub fn item(name: &str) -> Option<Item> {
    let english = match name {
        "beef" => "raw beef",
        "chicken" => "raw chicken",
        "hay_block" => "hay bale",
        "leather_chestplate" => "leather tunic",
        "leather_helmet" => "leather cap",
        "leather_leggings" => "leather pants",
        "mutton" => "raw mutton",
        "porkchop" => "raw porkchop",
        "quartz_block" => "block of quartz",
        "rabbit" => "raw rabbit",
        "slime_ball" => "slimeball",
        "vine" => "vines",
        _ => "",
    };
    crate::java_registry::item(name, english)
}

/// `#double_trade_price` (`#treasure`): binding and vanishing curses,
/// swift sneak, soul speed (not in the game), frost walker, mending and
/// wind burst.
pub fn double_trade_price(e: Enchantment) -> bool {
    use Enchantment as E;
    matches!(e, E::BindingCurse | E::VanishingCurse | E::SwiftSneak | E::FrostWalker | E::Mending | E::WindBurst)
}

/// The `#tradeable` potion tag, in tag order (`set_random_potion` on the
/// fletcher's tipped arrows).
pub const TRADEABLE_POTIONS: [&str; 41] = [
    "wind_charged",
    "oozing",
    "infested",
    "weaving",
    "night_vision",
    "long_night_vision",
    "invisibility",
    "long_invisibility",
    "fire_resistance",
    "long_fire_resistance",
    "leaping",
    "long_leaping",
    "strong_leaping",
    "slowness",
    "long_slowness",
    "strong_slowness",
    "turtle_master",
    "long_turtle_master",
    "strong_turtle_master",
    "swiftness",
    "long_swiftness",
    "strong_swiftness",
    "water_breathing",
    "long_water_breathing",
    "healing",
    "strong_healing",
    "harming",
    "strong_harming",
    "poison",
    "long_poison",
    "strong_poison",
    "regeneration",
    "long_regeneration",
    "strong_regeneration",
    "strength",
    "long_strength",
    "strong_strength",
    "weakness",
    "long_weakness",
    "slow_falling",
    "long_slow_falling",
];

/// The farmer's `set_stew_effect` list: effect, duration in ticks (seconds
/// times 20, except the instant saturation) and the stew's effect code
/// (`entity::mooshroom`).
pub const STEW_EFFECTS: [(&str, u32, u32); 6] = [
    ("night_vision", 100, 2),
    ("jump_boost", 160, 12),
    ("weakness", 140, 5),
    ("blindness", 120, 13),
    ("poison", 280, 14),
    ("saturation", 7, 1),
];

/// Offers drawn at `level` (`trade_set/<profession>/level_N` `amount`).
pub fn amount(profession: Profession, level: u8) -> usize {
    AMOUNTS
        .iter()
        .find(|(p, _)| *p == profession)
        .and_then(|(_, a)| a.get(level as usize - 1))
        .map_or(0, |&a| a as usize)
}

/// Every profession trade: profession, level and the entry, in each level
/// tag's order (nested  tags expanded where they appear).
#[rustfmt::skip]
pub const TRADES: &[(Profession, u8, TradeDef)] = &[
    (Profession::Armorer, 1, t("smith/1/coal_emerald", "coal", 15, "emerald", 1, 16, 2, 0.05)),
    (Profession::Armorer, 1, t("armorer/1/emerald_iron_leggings", "emerald", 7, "iron_leggings", 1, 12, 1, 0.2)),
    (Profession::Armorer, 1, t("armorer/1/emerald_iron_boots", "emerald", 4, "iron_boots", 1, 12, 1, 0.2)),
    (Profession::Armorer, 1, t("armorer/1/emerald_iron_helmet", "emerald", 5, "iron_helmet", 1, 12, 1, 0.2)),
    (Profession::Armorer, 1, t("armorer/1/emerald_iron_chestplate", "emerald", 9, "iron_chestplate", 1, 12, 1, 0.2)),
    (Profession::Armorer, 2, t("smith/2/iron_ingot_emerald", "iron_ingot", 4, "emerald", 1, 12, 10, 0.05)),
    (Profession::Armorer, 2, t("smith/2/emerald_bell", "emerald", 36, "bell", 1, 12, 5, 0.2)),
    (Profession::Armorer, 2, t("armorer/2/emerald_chainmail_boots", "emerald", 1, "chainmail_boots", 1, 12, 5, 0.2)),
    (Profession::Armorer, 2, t("armorer/2/emerald_chainmail_leggings", "emerald", 3, "chainmail_leggings", 1, 12, 5, 0.2)),
    (Profession::Armorer, 3, t("armorer/3/lava_bucket_emerald", "lava_bucket", 1, "emerald", 1, 12, 20, 0.05)),
    (Profession::Armorer, 3, t("armorer/3/emerald_chainmail_helmet", "emerald", 1, "chainmail_helmet", 1, 12, 10, 0.2)),
    (Profession::Armorer, 3, t("armorer/3/emerald_chainmail_chestplate", "emerald", 4, "chainmail_chestplate", 1, 12, 10, 0.2)),
    (Profession::Armorer, 3, t("armorer/3/emerald_shield", "emerald", 5, "shield", 1, 12, 10, 0.2)),
    (Profession::Armorer, 3, t("armorer/3/diamond_emerald", "diamond", 1, "emerald", 1, 12, 20, 0.05)),
    (Profession::Armorer, 4, t("armorer/4/emerald_enchanted_diamond_leggings", "emerald", 14, "diamond_leggings", 1, 3, 15, 0.2).with(Modifier::Enchant { min: 5, max: 19 })),
    (Profession::Armorer, 4, t("armorer/4/emerald_enchanted_diamond_boots", "emerald", 8, "diamond_boots", 1, 3, 15, 0.2).with(Modifier::Enchant { min: 5, max: 19 })),
    (Profession::Armorer, 5, t("armorer/5/emerald_enchanted_diamond_helmet", "emerald", 8, "diamond_helmet", 1, 3, 30, 0.2).with(Modifier::Enchant { min: 5, max: 19 })),
    (Profession::Armorer, 5, t("armorer/5/emerald_enchanted_diamond_chestplate", "emerald", 16, "diamond_chestplate", 1, 3, 30, 0.2).with(Modifier::Enchant { min: 5, max: 19 })),
    (Profession::Butcher, 1, t("butcher/1/chicken_emerald", "chicken", 14, "emerald", 1, 16, 2, 0.05)),
    (Profession::Butcher, 1, t("butcher/1/porkchop_emerald", "porkchop", 7, "emerald", 1, 16, 2, 0.05)),
    (Profession::Butcher, 1, t("butcher/1/rabbit_emerald", "rabbit", 4, "emerald", 1, 16, 2, 0.05)),
    (Profession::Butcher, 1, t("butcher/1/emerald_rabbit_stew", "emerald", 1, "rabbit_stew", 1, 12, 1, 0.05)),
    (Profession::Butcher, 2, t("butcher/2/coal_emerald", "coal", 15, "emerald", 1, 16, 2, 0.05)),
    (Profession::Butcher, 2, t("butcher/2/emerald_cooked_porkchop", "emerald", 1, "cooked_porkchop", 5, 16, 5, 0.05)),
    (Profession::Butcher, 2, t("butcher/2/emerald_cooked_chicken", "emerald", 1, "cooked_chicken", 8, 16, 5, 0.05)),
    (Profession::Butcher, 3, t("butcher/3/mutton_emerald", "mutton", 7, "emerald", 1, 16, 20, 0.05)),
    (Profession::Butcher, 3, t("butcher/3/beef_emerald", "beef", 10, "emerald", 1, 16, 20, 0.05)),
    (Profession::Butcher, 4, t("butcher/4/dried_kelp_block_emerald", "dried_kelp_block", 10, "emerald", 1, 12, 30, 0.05)),
    (Profession::Butcher, 5, t("butcher/5/sweet_berries_emerald", "sweet_berries", 10, "emerald", 1, 12, 30, 0.05)),
    (Profession::Cartographer, 1, t("cartographer/1/paper_emerald", "paper", 24, "emerald", 1, 12, 2, 0.05)),
    (Profession::Cartographer, 1, t("cartographer/1/emerald_map", "emerald", 7, "map", 1, 12, 1, 0.05)),
    (Profession::Cartographer, 2, t("cartographer/2/glass_pane_emerald", "glass_pane", 11, "emerald", 1, 12, 10, 0.05)),
    (Profession::Cartographer, 2, t("cartographer/2/emerald_and_compass_village_taiga_map", "emerald", 8, "taiga_village_map", 1, 12, 5, 0.2).also("compass", 1).only(&["swamp", "snow", "plains"]).with(Modifier::Map("on_taiga_village_maps"))),
    (Profession::Cartographer, 2, t("cartographer/2/emerald_and_compass_explorer_swamp_map", "emerald", 8, "swamp_hut_map", 1, 12, 5, 0.2).also("compass", 1).only(&["taiga", "snow", "jungle"]).with(Modifier::Map("on_swamp_hut_maps"))),
    (Profession::Cartographer, 2, t("cartographer/2/emerald_and_compass_village_snowy_map", "emerald", 8, "snowy_village_map", 1, 12, 5, 0.2).also("compass", 1).only(&["taiga", "swamp"]).with(Modifier::Map("on_snowy_village_maps"))),
    (Profession::Cartographer, 2, t("cartographer/2/emerald_and_compass_village_savanna_map", "emerald", 8, "savanna_village_map", 1, 12, 5, 0.2).also("compass", 1).only(&["plains", "jungle", "desert"]).with(Modifier::Map("on_savanna_village_maps"))),
    (Profession::Cartographer, 2, t("cartographer/2/emerald_and_compass_village_plains_map", "emerald", 8, "plains_village_map", 1, 12, 5, 0.2).also("compass", 1).only(&["taiga", "snow", "savanna", "desert"]).with(Modifier::Map("on_plains_village_maps"))),
    (Profession::Cartographer, 2, t("cartographer/2/emerald_and_compass_explorer_jungle_map", "emerald", 8, "jungle_pyramid_map", 1, 12, 5, 0.2).also("compass", 1).only(&["swamp", "savanna", "desert"]).with(Modifier::Map("on_jungle_pyramid_maps"))),
    (Profession::Cartographer, 2, t("cartographer/2/emerald_and_compass_village_desert_map", "emerald", 8, "desert_village_map", 1, 12, 5, 0.2).also("compass", 1).only(&["savanna", "jungle"]).with(Modifier::Map("on_desert_village_maps"))),
    (Profession::Cartographer, 3, t("cartographer/3/compass_emerald", "compass", 1, "emerald", 1, 12, 20, 0.05)),
    (Profession::Cartographer, 3, t("cartographer/3/emerald_and_compass_ocean_monument_map", "emerald", 13, "ocean_monument_map", 1, 12, 10, 0.2).also("compass", 1).with(Modifier::Map("on_ocean_monument_maps"))),
    (Profession::Cartographer, 3, t("cartographer/3/emerald_and_compass_buried_trial_chambers_map", "emerald", 12, "buried_trial_chambers_map", 1, 12, 10, 0.2).also("compass", 1).with(Modifier::Map("on_buried_trial_chambers_maps"))),
    (Profession::Cartographer, 4, t("cartographer/4/emerald_item_frame", "emerald", 7, "item_frame", 1, 12, 15, 0.05)),
    (Profession::Cartographer, 4, t("cartographer/4/emerald_white_banner", "emerald", 2, "white_banner", 1, 12, 15, 0.05).only(&["snow", "plains"])),
    (Profession::Cartographer, 4, t("cartographer/4/emerald_orange_banner", "emerald", 2, "orange_banner", 1, 12, 15, 0.05).only(&["savanna", "desert"])),
    (Profession::Cartographer, 4, t("cartographer/4/emerald_magenta_banner", "emerald", 2, "magenta_banner", 1, 12, 15, 0.05).only(&["savanna"])),
    (Profession::Cartographer, 4, t("cartographer/4/emerald_blue_banner", "emerald", 2, "blue_banner", 1, 12, 15, 0.05).only(&["snow", "taiga"])),
    (Profession::Cartographer, 4, t("cartographer/4/emerald_light_blue_banner", "emerald", 2, "light_blue_banner", 1, 12, 15, 0.05).only(&["snow", "swamp"])),
    (Profession::Cartographer, 4, t("cartographer/4/emerald_yellow_banner", "emerald", 2, "yellow_banner", 1, 12, 15, 0.05).only(&["plains", "jungle"])),
    (Profession::Cartographer, 4, t("cartographer/4/emerald_lime_banner", "emerald", 2, "lime_banner", 1, 12, 15, 0.05).only(&["desert", "taiga"])),
    (Profession::Cartographer, 4, t("cartographer/4/emerald_pink_banner", "emerald", 2, "pink_banner", 1, 12, 15, 0.05).only(&["taiga", "plains"])),
    (Profession::Cartographer, 4, t("cartographer/4/emerald_gray_banner", "emerald", 2, "gray_banner", 1, 12, 15, 0.05).only(&["desert"])),
    (Profession::Cartographer, 4, t("cartographer/4/emerald_cyan_banner", "emerald", 2, "cyan_banner", 1, 12, 15, 0.05).only(&["desert", "snow"])),
    (Profession::Cartographer, 4, t("cartographer/4/emerald_purple_banner", "emerald", 2, "purple_banner", 1, 12, 15, 0.05).only(&["taiga", "swamp"])),
    (Profession::Cartographer, 4, t("cartographer/4/emerald_brown_banner", "emerald", 2, "brown_banner", 1, 12, 15, 0.05).only(&["plains", "jungle"])),
    (Profession::Cartographer, 4, t("cartographer/4/emerald_green_banner", "emerald", 2, "green_banner", 1, 12, 15, 0.05).only(&["desert", "savanna", "jungle"])),
    (Profession::Cartographer, 4, t("cartographer/4/emerald_red_banner", "emerald", 2, "red_banner", 1, 12, 15, 0.05).only(&["snow", "savanna"])),
    (Profession::Cartographer, 4, t("cartographer/4/emerald_black_banner", "emerald", 2, "black_banner", 1, 12, 15, 0.05).only(&["swamp"])),
    (Profession::Cartographer, 5, t("cartographer/5/emerald_globe_banner_pattern", "emerald", 8, "globe_banner_pattern", 1, 12, 30, 0.05)),
    (Profession::Cartographer, 5, t("cartographer/5/emerald_and_compass_woodland_mansion_map", "emerald", 14, "woodland_mansion_map", 1, 12, 30, 0.2).also("compass", 1).with(Modifier::Map("on_woodland_mansion_maps"))),
    (Profession::Cleric, 1, t("cleric/1/rotten_flesh_emerald", "rotten_flesh", 32, "emerald", 1, 16, 2, 0.05)),
    (Profession::Cleric, 1, t("cleric/1/emerald_redstone", "emerald", 1, "redstone", 2, 12, 1, 0.05)),
    (Profession::Cleric, 2, t("cleric/2/gold_ingot_emerald", "gold_ingot", 3, "emerald", 1, 12, 10, 0.05)),
    (Profession::Cleric, 2, t("cleric/2/emerald_lapis_lazuli", "emerald", 1, "lapis_lazuli", 1, 12, 5, 0.05)),
    (Profession::Cleric, 3, t("cleric/3/rabbit_foot_emerald", "rabbit_foot", 2, "emerald", 1, 12, 20, 0.05)),
    (Profession::Cleric, 3, t("cleric/3/emerald_glowstone", "emerald", 4, "glowstone", 1, 12, 10, 0.05)),
    (Profession::Cleric, 4, t("cleric/4/turtle_scute_emerald", "turtle_scute", 4, "emerald", 1, 12, 30, 0.05)),
    (Profession::Cleric, 4, t("cleric/4/glass_bottle_emerald", "glass_bottle", 9, "emerald", 1, 12, 30, 0.05)),
    (Profession::Cleric, 4, t("cleric/4/emerald_ender_pearl", "emerald", 5, "ender_pearl", 1, 12, 15, 0.05)),
    (Profession::Cleric, 5, t("cleric/5/nether_wart_emerald", "nether_wart", 22, "emerald", 1, 12, 30, 0.05)),
    (Profession::Cleric, 5, t("cleric/5/emerald_experience_bottle", "emerald", 3, "experience_bottle", 1, 12, 30, 0.05)),
    (Profession::Farmer, 1, t("farmer/1/wheat_emerald", "wheat", 20, "emerald", 1, 16, 2, 0.05)),
    (Profession::Farmer, 1, t("farmer/1/potato_emerald", "potato", 26, "emerald", 1, 16, 2, 0.05)),
    (Profession::Farmer, 1, t("farmer/1/carrot_emerald", "carrot", 22, "emerald", 1, 16, 2, 0.05)),
    (Profession::Farmer, 1, t("farmer/1/beetroot_emerald", "beetroot", 15, "emerald", 1, 16, 2, 0.05)),
    (Profession::Farmer, 1, t("farmer/1/emerald_bread", "emerald", 1, "bread", 6, 16, 1, 0.05)),
    (Profession::Farmer, 2, t("farmer/2/pumpkin_emerald", "pumpkin", 6, "emerald", 1, 12, 10, 0.05)),
    (Profession::Farmer, 2, t("farmer/2/emerald_pumpkin_pie", "emerald", 1, "pumpkin_pie", 4, 12, 5, 0.05)),
    (Profession::Farmer, 2, t("farmer/2/emerald_apple", "emerald", 1, "apple", 4, 16, 5, 0.05)),
    (Profession::Farmer, 3, t("farmer/3/emerald_cookie", "emerald", 3, "cookie", 18, 12, 10, 0.05)),
    (Profession::Farmer, 3, t("farmer/3/melon_emerald", "melon", 4, "emerald", 1, 12, 20, 0.05)),
    (Profession::Farmer, 4, t("farmer/4/emerald_cake", "emerald", 3, "cake", 1, 12, 15, 0.05)),
    (Profession::Farmer, 4, t("farmer/4/emerald_suspicious_stew", "emerald", 1, "suspicious_stew", 1, 12, 15, 0.05).with(Modifier::Stew)),
    (Profession::Farmer, 5, t("farmer/5/emerald_golden_carrot", "emerald", 3, "golden_carrot", 3, 12, 30, 0.05)),
    (Profession::Farmer, 5, t("farmer/5/emerald_glistening_melon_slice", "emerald", 4, "glistering_melon_slice", 3, 12, 30, 0.05)),
    (Profession::Fisherman, 1, t("fisherman/1/string_emerald", "string", 20, "emerald", 1, 16, 2, 0.05)),
    (Profession::Fisherman, 1, t("fisherman/1/coal_emerald", "coal", 10, "emerald", 1, 16, 2, 0.05)),
    (Profession::Fisherman, 1, t("fisherman/1/raw_cod_and_emerald_cooked_cod", "cod", 6, "cooked_cod", 6, 16, 1, 0.05).also("emerald", 1)),
    (Profession::Fisherman, 1, t("fisherman/1/emerald_cod_bucket", "emerald", 3, "cod_bucket", 1, 16, 1, 0.05)),
    (Profession::Fisherman, 2, t("fisherman/2/cod_emerald", "cod", 15, "emerald", 1, 16, 10, 0.05)),
    (Profession::Fisherman, 2, t("fisherman/2/salmon_and_emerald_cooked_salmon", "salmon", 6, "cooked_salmon", 6, 16, 5, 0.05).also("emerald", 1)),
    (Profession::Fisherman, 2, t("fisherman/2/emerald_campfire", "emerald", 2, "campfire", 1, 12, 5, 0.05)),
    (Profession::Fisherman, 3, t("fisherman/3/salmon_emerald", "salmon", 13, "emerald", 1, 16, 20, 0.05)),
    (Profession::Fisherman, 3, t("fisherman/3/emerald_enchanted_fishing_rod", "emerald", 3, "fishing_rod", 1, 3, 10, 0.2).with(Modifier::Enchant { min: 5, max: 19 })),
    (Profession::Fisherman, 4, t("fisherman/4/tropical_fish_emerald", "tropical_fish", 6, "emerald", 1, 12, 30, 0.05)),
    (Profession::Fisherman, 5, t("fisherman/5/pufferfish_emerald", "pufferfish", 4, "emerald", 1, 12, 30, 0.05)),
    (Profession::Fisherman, 5, t("fisherman/5/oak_boat_emerald", "oak_boat", 1, "emerald", 1, 12, 30, 0.05).only(&["plains"])),
    (Profession::Fisherman, 5, t("fisherman/5/spruce_boat_emerald", "spruce_boat", 1, "emerald", 1, 12, 30, 0.05).only(&["taiga", "snow"])),
    (Profession::Fisherman, 5, t("fisherman/5/jungle_boat_emerald", "jungle_boat", 1, "emerald", 1, 12, 30, 0.05).only(&["desert", "jungle"])),
    (Profession::Fisherman, 5, t("fisherman/5/acacia_boat_emerald", "acacia_boat", 1, "emerald", 1, 12, 30, 0.05).only(&["savanna"])),
    (Profession::Fisherman, 5, t("fisherman/5/dark_oak_boat_emerald", "dark_oak_boat", 1, "emerald", 1, 12, 30, 0.05).only(&["swamp"])),
    (Profession::Fletcher, 1, t("fletcher/1/stick_emerald", "stick", 32, "emerald", 1, 16, 2, 0.05)),
    (Profession::Fletcher, 1, t("fletcher/1/emerald_arrow", "emerald", 1, "arrow", 16, 12, 1, 0.05)),
    (Profession::Fletcher, 1, t("fletcher/1/gravel_and_emerald_flint", "gravel", 10, "flint", 10, 12, 1, 0.05).also("emerald", 1)),
    (Profession::Fletcher, 2, t("fletcher/2/flint_emerald", "flint", 26, "emerald", 1, 12, 10, 0.05)),
    (Profession::Fletcher, 2, t("fletcher/2/emerald_bow", "emerald", 2, "bow", 1, 12, 5, 0.05)),
    (Profession::Fletcher, 3, t("fletcher/3/string_emerald", "string", 14, "emerald", 1, 16, 20, 0.05)),
    (Profession::Fletcher, 3, t("fletcher/3/emerald_crossbow", "emerald", 3, "crossbow", 1, 12, 10, 0.05)),
    (Profession::Fletcher, 4, t("fletcher/4/feather_emerald", "feather", 24, "emerald", 1, 16, 30, 0.05)),
    (Profession::Fletcher, 4, t("fletcher/4/emerald_enchanted_bow", "emerald", 2, "bow", 1, 3, 15, 0.05).with(Modifier::Enchant { min: 5, max: 19 })),
    (Profession::Fletcher, 5, t("fletcher/5/tripwire_hook_emerald", "tripwire_hook", 8, "emerald", 1, 12, 30, 0.05)),
    (Profession::Fletcher, 5, t("fletcher/5/emerald_enchanted_crossbow", "emerald", 3, "crossbow", 1, 3, 15, 0.05).with(Modifier::Enchant { min: 5, max: 19 })),
    (Profession::Fletcher, 5, t("fletcher/5/arrow_and_emerald_tipped_arrow", "emerald", 2, "tipped_arrow", 5, 12, 30, 0.05).also("arrow", 5).with(Modifier::RandomPotion)),
    (Profession::Leatherworker, 1, t("leatherworker/1/leather_emerald", "leather", 6, "emerald", 1, 16, 2, 0.05)),
    (Profession::Leatherworker, 1, t("leatherworker/1/emerald_dyed_leather_leggings", "emerald", 3, "leather_leggings", 1, 12, 1, 0.2).with(Modifier::Dyes)),
    (Profession::Leatherworker, 1, t("leatherworker/1/emerald_dyed_leather_chestplate", "emerald", 7, "leather_chestplate", 1, 12, 1, 0.2).with(Modifier::Dyes)),
    (Profession::Leatherworker, 2, t("leatherworker/2/flint_emerald", "flint", 26, "emerald", 1, 12, 10, 0.05)),
    (Profession::Leatherworker, 2, t("leatherworker/2/emerald_dyed_leather_helmet", "emerald", 5, "leather_helmet", 1, 12, 5, 0.2).with(Modifier::Dyes)),
    (Profession::Leatherworker, 2, t("leatherworker/2/emerald_dyed_leather_boots", "emerald", 4, "leather_boots", 1, 12, 5, 0.2).with(Modifier::Dyes)),
    (Profession::Leatherworker, 3, t("leatherworker/3/rabbit_hide_emerald", "rabbit_hide", 9, "emerald", 1, 12, 20, 0.05)),
    (Profession::Leatherworker, 3, t("leatherworker/3/emerald_dyed_leather_chestplate", "emerald", 7, "leather_chestplate", 1, 12, 1, 0.2).with(Modifier::Dyes)),
    (Profession::Leatherworker, 4, t("leatherworker/4/turtle_scute_emerald", "turtle_scute", 4, "emerald", 1, 12, 30, 0.05)),
    (Profession::Leatherworker, 4, t("leatherworker/4/emerald_dyed_leather_horse_armor", "emerald", 6, "leather_horse_armor", 1, 12, 15, 0.2).with(Modifier::Dyes)),
    (Profession::Leatherworker, 5, t("leatherworker/5/emerald_saddle", "emerald", 6, "saddle", 1, 12, 30, 0.2)),
    (Profession::Leatherworker, 5, t("leatherworker/5/emerald_dyed_leather_helmet", "emerald", 5, "leather_helmet", 1, 12, 5, 0.2).with(Modifier::Dyes)),
    (Profession::Librarian, 1, t("librarian/1/paper_emerald", "paper", 24, "emerald", 1, 16, 2, 0.05)),
    (Profession::Librarian, 1, t("librarian/1/emerald_and_book_enchanted_book", "emerald", 0, "enchanted_book", 1, 12, 1, 0.2).also("book", 1).with(Modifier::Book)),
    (Profession::Librarian, 1, t("librarian/1/emerald_bookshelf", "emerald", 9, "bookshelf", 1, 12, 1, 0.05)),
    (Profession::Librarian, 2, t("librarian/2/book_emerald", "book", 4, "emerald", 1, 12, 10, 0.05)),
    (Profession::Librarian, 2, t("librarian/2/emerald_and_book_enchanted_book", "emerald", 0, "enchanted_book", 1, 12, 5, 0.2).also("book", 1).with(Modifier::Book)),
    (Profession::Librarian, 2, t("librarian/2/emerald_lantern", "emerald", 1, "lantern", 1, 12, 5, 0.05)),
    (Profession::Librarian, 3, t("librarian/3/ink_sac_emerald", "ink_sac", 5, "emerald", 1, 12, 20, 0.05)),
    (Profession::Librarian, 3, t("librarian/3/emerald_and_book_enchanted_book", "emerald", 0, "enchanted_book", 1, 12, 10, 0.2).also("book", 1).with(Modifier::Book)),
    (Profession::Librarian, 3, t("librarian/3/emerald_glass", "emerald", 1, "glass", 4, 12, 10, 0.05)),
    (Profession::Librarian, 4, t("librarian/4/writable_book_emerald", "writable_book", 2, "emerald", 1, 12, 30, 0.05)),
    (Profession::Librarian, 4, t("librarian/4/emerald_book_and_enchanted_book", "emerald", 0, "enchanted_book", 1, 12, 15, 0.2).also("book", 1).with(Modifier::Book)),
    (Profession::Librarian, 4, t("librarian/4/emerald_clock", "emerald", 5, "clock", 1, 12, 15, 0.05)),
    (Profession::Librarian, 4, t("librarian/4/emerald_compass", "emerald", 4, "compass", 1, 12, 15, 0.05)),
    (Profession::Librarian, 5, t("librarian/5/emerald_yellow_candle", "emerald", 3, "yellow_candle", 1, 12, 30, 0.05)),
    (Profession::Librarian, 5, t("librarian/5/emerald_red_candle", "emerald", 3, "red_candle", 1, 12, 30, 0.05)),
    (Profession::Mason, 1, t("mason/1/clay_ball_emerald", "clay_ball", 10, "emerald", 1, 16, 2, 0.05)),
    (Profession::Mason, 1, t("mason/1/emerald_brick", "emerald", 1, "brick", 10, 16, 1, 0.05)),
    (Profession::Mason, 2, t("mason/2/stone_emerald", "stone", 20, "emerald", 1, 16, 10, 0.05)),
    (Profession::Mason, 2, t("mason/2/emerald_chiseled_stone_bricks", "emerald", 1, "chiseled_stone_bricks", 4, 16, 5, 0.05)),
    (Profession::Mason, 3, t("mason/3/granite_emerald", "granite", 16, "emerald", 1, 16, 20, 0.05)),
    (Profession::Mason, 3, t("mason/3/andesite_emerald", "andesite", 16, "emerald", 1, 16, 20, 0.05)),
    (Profession::Mason, 3, t("mason/3/diorite_emerald", "diorite", 16, "emerald", 1, 16, 20, 0.05)),
    (Profession::Mason, 3, t("mason/3/emerald_dripstone_block", "emerald", 1, "dripstone_block", 4, 16, 10, 0.05)),
    (Profession::Mason, 3, t("mason/3/emerald_polished_andesite", "emerald", 1, "polished_andesite", 4, 16, 10, 0.05)),
    (Profession::Mason, 3, t("mason/3/emerald_polished_diorite", "emerald", 1, "polished_diorite", 4, 16, 10, 0.05)),
    (Profession::Mason, 3, t("mason/3/emerald_polished_granite", "emerald", 1, "polished_granite", 4, 16, 10, 0.05)),
    (Profession::Mason, 4, t("mason/4/quartz_emerald", "quartz", 12, "emerald", 1, 12, 30, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_white_terracotta", "emerald", 1, "white_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_orange_terracotta", "emerald", 1, "orange_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_magenta_terracotta", "emerald", 1, "magenta_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_light_blue_terracotta", "emerald", 1, "light_blue_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_yellow_terracotta", "emerald", 1, "yellow_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_lime_terracotta", "emerald", 1, "lime_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_pink_terracotta", "emerald", 1, "pink_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_gray_terracotta", "emerald", 1, "gray_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_light_gray_terracotta", "emerald", 1, "light_gray_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_cyan_terracotta", "emerald", 1, "cyan_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_purple_terracotta", "emerald", 1, "purple_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_blue_terracotta", "emerald", 1, "blue_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_brown_terracotta", "emerald", 1, "brown_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_green_terracotta", "emerald", 1, "green_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_red_terracotta", "emerald", 1, "red_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_black_terracotta", "emerald", 1, "black_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_white_glazed_terracotta", "emerald", 1, "white_glazed_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_orange_glazed_terracotta", "emerald", 1, "orange_glazed_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_magenta_glazed_terracotta", "emerald", 1, "magenta_glazed_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_light_blue_glazed_terracotta", "emerald", 1, "light_blue_glazed_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_yellow_glazed_terracotta", "emerald", 1, "yellow_glazed_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_lime_glazed_terracotta", "emerald", 1, "lime_glazed_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_pink_glazed_terracotta", "emerald", 1, "pink_glazed_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_gray_glazed_terracotta", "emerald", 1, "gray_glazed_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_light_gray_glazed_terracotta", "emerald", 1, "light_gray_glazed_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_cyan_glazed_terracotta", "emerald", 1, "cyan_glazed_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_purple_glazed_terracotta", "emerald", 1, "purple_glazed_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_blue_glazed_terracotta", "emerald", 1, "blue_glazed_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_brown_glazed_terracotta", "emerald", 1, "brown_glazed_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_green_glazed_terracotta", "emerald", 1, "green_glazed_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_red_glazed_terracotta", "emerald", 1, "red_glazed_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 4, t("mason/4/emerald_black_glazed_terracotta", "emerald", 1, "black_glazed_terracotta", 1, 12, 15, 0.05)),
    (Profession::Mason, 5, t("mason/5/emerald_quartz_pillar", "emerald", 1, "quartz_pillar", 1, 12, 30, 0.05)),
    (Profession::Mason, 5, t("mason/5/emerald_quartz_block", "emerald", 1, "quartz_block", 1, 12, 30, 0.05)),
    (Profession::Shepherd, 1, t("shepherd/1/white_wool_emerald", "white_wool", 18, "emerald", 1, 16, 2, 0.05)),
    (Profession::Shepherd, 1, t("shepherd/1/brown_wool_emerald", "brown_wool", 18, "emerald", 1, 16, 2, 0.05)),
    (Profession::Shepherd, 1, t("shepherd/1/gray_wool_emerald", "gray_wool", 18, "emerald", 1, 16, 2, 0.05)),
    (Profession::Shepherd, 1, t("shepherd/1/black_wool_emerald", "black_wool", 18, "emerald", 1, 16, 2, 0.05)),
    (Profession::Shepherd, 1, t("shepherd/1/emerald_shears", "emerald", 2, "shears", 1, 12, 1, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/white_dye_emerald", "white_dye", 12, "emerald", 1, 16, 10, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/gray_dye_emerald", "gray_dye", 12, "emerald", 1, 16, 10, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/black_dye_emerald", "black_dye", 12, "emerald", 1, 16, 10, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/light_blue_dye_emerald", "light_blue_dye", 12, "emerald", 1, 16, 10, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/lime_dye_emerald", "lime_dye", 12, "emerald", 1, 16, 10, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_white_wool", "emerald", 1, "white_wool", 1, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_orange_wool", "emerald", 1, "orange_wool", 1, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_magenta_wool", "emerald", 1, "magenta_wool", 1, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_light_blue_wool", "emerald", 1, "light_blue_wool", 1, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_yellow_wool", "emerald", 1, "yellow_wool", 1, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_lime_wool", "emerald", 1, "lime_wool", 1, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_pink_wool", "emerald", 1, "pink_wool", 1, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_gray_wool", "emerald", 1, "gray_wool", 1, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_light_gray_wool", "emerald", 1, "light_gray_wool", 1, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_cyan_wool", "emerald", 1, "cyan_wool", 1, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_purple_wool", "emerald", 1, "purple_wool", 1, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_blue_wool", "emerald", 1, "blue_wool", 1, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_brown_wool", "emerald", 1, "brown_wool", 1, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_green_wool", "emerald", 1, "green_wool", 1, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_red_wool", "emerald", 1, "red_wool", 1, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_black_wool", "emerald", 1, "black_wool", 1, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_white_carpet", "emerald", 1, "white_carpet", 4, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_orange_carpet", "emerald", 1, "orange_carpet", 4, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_magenta_carpet", "emerald", 1, "magenta_carpet", 4, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_light_blue_carpet", "emerald", 1, "light_blue_carpet", 4, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_yellow_carpet", "emerald", 1, "yellow_carpet", 4, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_lime_carpet", "emerald", 1, "lime_carpet", 4, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_pink_carpet", "emerald", 1, "pink_carpet", 4, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_gray_carpet", "emerald", 1, "gray_carpet", 4, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_light_gray_carpet", "emerald", 1, "light_gray_carpet", 4, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_cyan_carpet", "emerald", 1, "cyan_carpet", 4, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_purple_carpet", "emerald", 1, "purple_carpet", 4, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_blue_carpet", "emerald", 1, "blue_carpet", 4, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_brown_carpet", "emerald", 1, "brown_carpet", 4, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_green_carpet", "emerald", 1, "green_carpet", 4, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_red_carpet", "emerald", 1, "red_carpet", 4, 16, 5, 0.05)),
    (Profession::Shepherd, 2, t("shepherd/2/emerald_black_carpet", "emerald", 1, "black_carpet", 4, 16, 5, 0.05)),
    (Profession::Shepherd, 3, t("shepherd/3/yellow_dye_emerald", "yellow_dye", 12, "emerald", 1, 16, 20, 0.05)),
    (Profession::Shepherd, 3, t("shepherd/3/light_gray_dye_emerald", "light_gray_dye", 12, "emerald", 1, 16, 20, 0.05)),
    (Profession::Shepherd, 3, t("shepherd/3/orange_dye_emerald", "orange_dye", 12, "emerald", 1, 16, 20, 0.05)),
    (Profession::Shepherd, 3, t("shepherd/3/red_dye_emerald", "red_dye", 12, "emerald", 1, 16, 20, 0.05)),
    (Profession::Shepherd, 3, t("shepherd/3/pink_dye_emerald", "pink_dye", 12, "emerald", 1, 16, 20, 0.05)),
    (Profession::Shepherd, 3, t("shepherd/3/emerald_white_bed", "emerald", 3, "white_bed", 1, 12, 10, 0.05)),
    (Profession::Shepherd, 3, t("shepherd/3/emerald_orange_bed", "emerald", 3, "orange_bed", 1, 12, 10, 0.05)),
    (Profession::Shepherd, 3, t("shepherd/3/emerald_magenta_bed", "emerald", 3, "magenta_bed", 1, 12, 10, 0.05)),
    (Profession::Shepherd, 3, t("shepherd/3/emerald_light_blue_bed", "emerald", 3, "light_blue_bed", 1, 12, 10, 0.05)),
    (Profession::Shepherd, 3, t("shepherd/3/emerald_yellow_bed", "emerald", 3, "yellow_bed", 1, 12, 10, 0.05)),
    (Profession::Shepherd, 3, t("shepherd/3/emerald_lime_bed", "emerald", 3, "lime_bed", 1, 12, 10, 0.05)),
    (Profession::Shepherd, 3, t("shepherd/3/emerald_pink_bed", "emerald", 3, "pink_bed", 1, 12, 10, 0.05)),
    (Profession::Shepherd, 3, t("shepherd/3/emerald_gray_bed", "emerald", 3, "gray_bed", 1, 12, 10, 0.05)),
    (Profession::Shepherd, 3, t("shepherd/3/emerald_light_gray_bed", "emerald", 3, "light_gray_bed", 1, 12, 10, 0.05)),
    (Profession::Shepherd, 3, t("shepherd/3/emerald_cyan_bed", "emerald", 3, "cyan_bed", 1, 12, 10, 0.05)),
    (Profession::Shepherd, 3, t("shepherd/3/emerald_purple_bed", "emerald", 3, "purple_bed", 1, 12, 10, 0.05)),
    (Profession::Shepherd, 3, t("shepherd/3/emerald_blue_bed", "emerald", 3, "blue_bed", 1, 12, 10, 0.05)),
    (Profession::Shepherd, 3, t("shepherd/3/emerald_brown_bed", "emerald", 3, "brown_bed", 1, 12, 10, 0.05)),
    (Profession::Shepherd, 3, t("shepherd/3/emerald_green_bed", "emerald", 3, "green_bed", 1, 12, 10, 0.05)),
    (Profession::Shepherd, 3, t("shepherd/3/emerald_red_bed", "emerald", 3, "red_bed", 1, 12, 10, 0.05)),
    (Profession::Shepherd, 3, t("shepherd/3/emerald_black_bed", "emerald", 3, "black_bed", 1, 12, 10, 0.05)),
    (Profession::Shepherd, 4, t("shepherd/4/brown_dye_emerald", "brown_dye", 12, "emerald", 1, 16, 30, 0.05)),
    (Profession::Shepherd, 4, t("shepherd/4/purple_dye_emerald", "purple_dye", 12, "emerald", 1, 16, 30, 0.05)),
    (Profession::Shepherd, 4, t("shepherd/4/blue_dye_emerald", "blue_dye", 12, "emerald", 1, 16, 30, 0.05)),
    (Profession::Shepherd, 4, t("shepherd/4/green_dye_emerald", "green_dye", 12, "emerald", 1, 16, 30, 0.05)),
    (Profession::Shepherd, 4, t("shepherd/4/magenta_dye_emerald", "magenta_dye", 12, "emerald", 1, 16, 30, 0.05)),
    (Profession::Shepherd, 4, t("shepherd/4/cyan_dye_emerald", "cyan_dye", 12, "emerald", 1, 16, 30, 0.05)),
    (Profession::Shepherd, 4, t("shepherd/4/emerald_white_banner", "emerald", 3, "white_banner", 1, 12, 15, 0.05)),
    (Profession::Shepherd, 4, t("shepherd/4/emerald_orange_banner", "emerald", 3, "orange_banner", 1, 12, 15, 0.05)),
    (Profession::Shepherd, 4, t("shepherd/4/emerald_magenta_banner", "emerald", 3, "magenta_banner", 1, 12, 15, 0.05)),
    (Profession::Shepherd, 4, t("shepherd/4/emerald_light_blue_banner", "emerald", 3, "light_blue_banner", 1, 12, 15, 0.05)),
    (Profession::Shepherd, 4, t("shepherd/4/emerald_yellow_banner", "emerald", 3, "yellow_banner", 1, 12, 15, 0.05)),
    (Profession::Shepherd, 4, t("shepherd/4/emerald_lime_banner", "emerald", 3, "lime_banner", 1, 12, 15, 0.05)),
    (Profession::Shepherd, 4, t("shepherd/4/emerald_pink_banner", "emerald", 3, "pink_banner", 1, 12, 15, 0.05)),
    (Profession::Shepherd, 4, t("shepherd/4/emerald_gray_banner", "emerald", 3, "gray_banner", 1, 12, 15, 0.05)),
    (Profession::Shepherd, 4, t("shepherd/4/emerald_light_gray_banner", "emerald", 3, "light_gray_banner", 1, 12, 15, 0.05)),
    (Profession::Shepherd, 4, t("shepherd/4/emerald_cyan_banner", "emerald", 3, "cyan_banner", 1, 12, 15, 0.05)),
    (Profession::Shepherd, 4, t("shepherd/4/emerald_purple_banner", "emerald", 3, "purple_banner", 1, 12, 15, 0.05)),
    (Profession::Shepherd, 4, t("shepherd/4/emerald_blue_banner", "emerald", 3, "blue_banner", 1, 12, 15, 0.05)),
    (Profession::Shepherd, 4, t("shepherd/4/emerald_brown_banner", "emerald", 3, "brown_banner", 1, 12, 15, 0.05)),
    (Profession::Shepherd, 4, t("shepherd/4/emerald_green_banner", "emerald", 3, "green_banner", 1, 12, 15, 0.05)),
    (Profession::Shepherd, 4, t("shepherd/4/emerald_red_banner", "emerald", 3, "red_banner", 1, 12, 15, 0.05)),
    (Profession::Shepherd, 4, t("shepherd/4/emerald_black_banner", "emerald", 3, "black_banner", 1, 12, 15, 0.05)),
    (Profession::Shepherd, 5, t("shepherd/5/emerald_painting", "emerald", 2, "painting", 3, 12, 30, 0.05)),
    (Profession::Toolsmith, 1, t("smith/1/coal_emerald", "coal", 15, "emerald", 1, 16, 2, 0.05)),
    (Profession::Toolsmith, 1, t("toolsmith/1/emerald_stone_axe", "emerald", 1, "stone_axe", 1, 12, 1, 0.2)),
    (Profession::Toolsmith, 1, t("toolsmith/1/emerald_stone_shovel", "emerald", 1, "stone_shovel", 1, 12, 1, 0.2)),
    (Profession::Toolsmith, 1, t("toolsmith/1/emerald_stone_pickaxe", "emerald", 1, "stone_pickaxe", 1, 12, 1, 0.2)),
    (Profession::Toolsmith, 1, t("toolsmith/1/emerald_stone_hoe", "emerald", 1, "stone_hoe", 1, 12, 1, 0.2)),
    (Profession::Toolsmith, 2, t("smith/2/iron_ingot_emerald", "iron_ingot", 4, "emerald", 1, 12, 10, 0.05)),
    (Profession::Toolsmith, 2, t("smith/2/emerald_bell", "emerald", 36, "bell", 1, 12, 5, 0.2)),
    (Profession::Toolsmith, 3, t("toolsmith/3/flint_emerald", "flint", 30, "emerald", 1, 12, 20, 0.05)),
    (Profession::Toolsmith, 3, t("toolsmith/3/emerald_enchanted_iron_axe", "emerald", 1, "iron_axe", 1, 3, 10, 0.2).with(Modifier::Enchant { min: 5, max: 19 })),
    (Profession::Toolsmith, 3, t("toolsmith/3/emerald_enchanted_iron_shovel", "emerald", 2, "iron_shovel", 1, 3, 10, 0.2).with(Modifier::Enchant { min: 5, max: 19 })),
    (Profession::Toolsmith, 3, t("toolsmith/3/emerald_enchanted_iron_pickaxe", "emerald", 3, "iron_pickaxe", 1, 3, 10, 0.2).with(Modifier::Enchant { min: 5, max: 19 })),
    (Profession::Toolsmith, 3, t("toolsmith/3/emerald_diamond_hoe", "emerald", 4, "diamond_hoe", 1, 3, 10, 0.2)),
    (Profession::Toolsmith, 4, t("toolsmith/4/emerald_enchanted_diamond_axe", "emerald", 12, "diamond_axe", 1, 3, 15, 0.2).with(Modifier::Enchant { min: 5, max: 19 })),
    (Profession::Toolsmith, 4, t("toolsmith/4/emerald_enchanted_diamond_shovel", "emerald", 5, "diamond_shovel", 1, 3, 15, 0.2).with(Modifier::Enchant { min: 5, max: 19 })),
    (Profession::Toolsmith, 4, t("toolsmith/4/diamond_emerald", "diamond", 1, "emerald", 1, 12, 30, 0.05)),
    (Profession::Toolsmith, 5, t("toolsmith/5/emerald_enchanted_diamond_pickaxe", "emerald", 13, "diamond_pickaxe", 1, 3, 30, 0.2).with(Modifier::Enchant { min: 5, max: 19 })),
    (Profession::Weaponsmith, 1, t("smith/1/coal_emerald", "coal", 15, "emerald", 1, 16, 2, 0.05)),
    (Profession::Weaponsmith, 1, t("weaponsmith/1/emerald_iron_axe", "emerald", 3, "iron_axe", 1, 12, 1, 0.2)),
    (Profession::Weaponsmith, 1, t("weaponsmith/1/emerald_enchanted_iron_sword", "emerald", 2, "iron_sword", 1, 12, 1, 0.2).with(Modifier::Enchant { min: 5, max: 19 })),
    (Profession::Weaponsmith, 2, t("smith/2/iron_ingot_emerald", "iron_ingot", 4, "emerald", 1, 12, 10, 0.05)),
    (Profession::Weaponsmith, 2, t("smith/2/emerald_bell", "emerald", 36, "bell", 1, 12, 5, 0.2)),
    (Profession::Weaponsmith, 3, t("weaponsmith/3/flint_emerald", "flint", 24, "emerald", 1, 12, 20, 0.05)),
    (Profession::Weaponsmith, 4, t("weaponsmith/4/emerald_enchanted_diamond_axe", "emerald", 12, "diamond_axe", 1, 3, 15, 0.2).with(Modifier::Enchant { min: 5, max: 19 })),
    (Profession::Weaponsmith, 4, t("weaponsmith/4/diamond_emerald", "diamond", 1, "emerald", 1, 12, 30, 0.05)),
    (Profession::Weaponsmith, 5, t("weaponsmith/5/emerald_enchanted_diamond_sword", "emerald", 8, "diamond_sword", 1, 3, 30, 0.2).with(Modifier::Enchant { min: 5, max: 19 })),
];
/// `trade_set/<profession>/level_N` `amount`: offers drawn per level.
#[rustfmt::skip]
pub const AMOUNTS: [(Profession, [u8; 5]); 13] = [
    (Profession::Armorer, [2, 2, 2, 2, 2]),
    (Profession::Butcher, [2, 2, 2, 2, 2]),
    (Profession::Cartographer, [2, 2, 2, 2, 2]),
    (Profession::Cleric, [2, 2, 2, 2, 2]),
    (Profession::Farmer, [2, 2, 2, 2, 2]),
    (Profession::Fisherman, [2, 2, 2, 2, 2]),
    (Profession::Fletcher, [2, 2, 2, 2, 2]),
    (Profession::Leatherworker, [2, 2, 2, 2, 2]),
    (Profession::Librarian, [2, 2, 2, 2, 3]),
    (Profession::Mason, [2, 2, 2, 2, 2]),
    (Profession::Shepherd, [2, 2, 2, 2, 2]),
    (Profession::Toolsmith, [2, 2, 2, 2, 2]),
    (Profession::Weaponsmith, [2, 2, 2, 2, 2]),
];
/// `trade_set/wandering_trader/buying`: 2 drawn.
#[rustfmt::skip]
pub const WANDERING_BUYING: &[TradeDef] = &[
    t("wandering_trader/water_bottle_emerald", "potion", 1, "emerald", 1, 2, 1, 0.05),
    t("wandering_trader/water_bucket_emerald", "water_bucket", 1, "emerald", 2, 2, 1, 0.05),
    t("wandering_trader/milk_bucket_emerald", "milk_bucket", 1, "emerald", 2, 2, 1, 0.05),
    t("wandering_trader/fermented_spider_eye_emerald", "fermented_spider_eye", 1, "emerald", 3, 2, 1, 0.05),
    t("wandering_trader/baked_potato_emerald", "baked_potato", 4, "emerald", 1, 2, 1, 0.05),
    t("wandering_trader/hay_block_emerald", "hay_block", 1, "emerald", 1, 2, 1, 0.05),
];
/// `trade_set/wandering_trader/uncommon`: 2 drawn.
#[rustfmt::skip]
pub const WANDERING_UNCOMMON: &[TradeDef] = &[
    t("wandering_trader/emerald_packed_ice", "emerald", 1, "packed_ice", 1, 6, 1, 0.05),
    t("wandering_trader/emerald_blue_ice", "emerald", 6, "blue_ice", 1, 6, 1, 0.05),
    t("wandering_trader/emerald_gunpowder", "emerald", 1, "gunpowder", 4, 2, 1, 0.05),
    t("wandering_trader/emerald_podzol", "emerald", 3, "podzol", 3, 6, 1, 0.05),
    t("wandering_trader/emerald_acacia_log", "emerald", 1, "acacia_log", 8, 4, 1, 0.05),
    t("wandering_trader/emerald_birch_log", "emerald", 1, "birch_log", 8, 4, 1, 0.05),
    t("wandering_trader/emerald_dark_oak_log", "emerald", 1, "dark_oak_log", 8, 4, 1, 0.05),
    t("wandering_trader/emerald_jungle_log", "emerald", 1, "jungle_log", 8, 4, 1, 0.05),
    t("wandering_trader/emerald_oak_log", "emerald", 1, "oak_log", 8, 4, 1, 0.05),
    t("wandering_trader/emerald_spruce_log", "emerald", 1, "spruce_log", 8, 4, 1, 0.05),
    t("wandering_trader/emerald_cherry_log", "emerald", 1, "cherry_log", 8, 4, 1, 0.05),
    t("wandering_trader/emerald_mangrove_log", "emerald", 1, "mangrove_log", 8, 4, 1, 0.05),
    t("wandering_trader/emerald_pale_oak_log", "emerald", 1, "pale_oak_log", 8, 4, 1, 0.05),
    t("wandering_trader/emerald_poplar_log", "emerald", 1, "poplar_log", 8, 4, 1, 0.05),
    t("wandering_trader/emerald_enchanted_iron_pickaxe", "emerald", 1, "iron_pickaxe", 1, 1, 1, 0.2).with(Modifier::Enchant { min: 5, max: 19 }),
    t("wandering_trader/emerald_long_invisibility_potion", "emerald", 5, "potion", 1, 1, 1, 0.05).with(Modifier::Potion("long_invisibility")),
];
/// `trade_set/wandering_trader/common`: 5 drawn.
#[rustfmt::skip]
pub const WANDERING_COMMON: &[TradeDef] = &[
    t("wandering_trader/emerald_white_dye", "emerald", 1, "white_dye", 3, 12, 1, 0.05),
    t("wandering_trader/emerald_orange_dye", "emerald", 1, "orange_dye", 3, 12, 1, 0.05),
    t("wandering_trader/emerald_magenta_dye", "emerald", 1, "magenta_dye", 3, 12, 1, 0.05),
    t("wandering_trader/emerald_light_blue_dye", "emerald", 1, "light_blue_dye", 3, 12, 1, 0.05),
    t("wandering_trader/emerald_yellow_dye", "emerald", 1, "yellow_dye", 3, 12, 1, 0.05),
    t("wandering_trader/emerald_lime_dye", "emerald", 1, "lime_dye", 3, 12, 1, 0.05),
    t("wandering_trader/emerald_pink_dye", "emerald", 1, "pink_dye", 3, 12, 1, 0.05),
    t("wandering_trader/emerald_gray_dye", "emerald", 1, "gray_dye", 3, 12, 1, 0.05),
    t("wandering_trader/emerald_light_gray_dye", "emerald", 1, "light_gray_dye", 3, 12, 1, 0.05),
    t("wandering_trader/emerald_cyan_dye", "emerald", 1, "cyan_dye", 3, 12, 1, 0.05),
    t("wandering_trader/emerald_purple_dye", "emerald", 1, "purple_dye", 3, 12, 1, 0.05),
    t("wandering_trader/emerald_blue_dye", "emerald", 1, "blue_dye", 3, 12, 1, 0.05),
    t("wandering_trader/emerald_brown_dye", "emerald", 1, "brown_dye", 3, 12, 1, 0.05),
    t("wandering_trader/emerald_green_dye", "emerald", 1, "green_dye", 3, 12, 1, 0.05),
    t("wandering_trader/emerald_red_dye", "emerald", 1, "red_dye", 3, 12, 1, 0.05),
    t("wandering_trader/emerald_black_dye", "emerald", 1, "black_dye", 3, 12, 1, 0.05),
    t("wandering_trader/emerald_fish_bucket", "emerald", 3, "tropical_fish_bucket", 1, 4, 1, 0.05),
    t("wandering_trader/emerald_pufferfish_bucket", "emerald", 3, "pufferfish_bucket", 1, 4, 1, 0.05),
    t("wandering_trader/emerald_sea_pickle", "emerald", 2, "sea_pickle", 1, 5, 1, 0.05),
    t("wandering_trader/emerald_slime_ball", "emerald", 4, "slime_ball", 1, 5, 1, 0.05),
    t("wandering_trader/emerald_glowstone", "emerald", 2, "glowstone", 1, 5, 1, 0.05),
    t("wandering_trader/emerald_nautilus_shell", "emerald", 5, "nautilus_shell", 1, 5, 1, 0.05),
    t("wandering_trader/emerald_fern", "emerald", 1, "fern", 1, 12, 1, 0.05),
    t("wandering_trader/emerald_sugar_cane", "emerald", 1, "sugar_cane", 1, 8, 1, 0.05),
    t("wandering_trader/emerald_pumpkin", "emerald", 1, "pumpkin", 1, 4, 1, 0.05),
    t("wandering_trader/emerald_kelp", "emerald", 3, "kelp", 1, 12, 1, 0.05),
    t("wandering_trader/emerald_cactus", "emerald", 3, "cactus", 1, 8, 1, 0.05),
    t("wandering_trader/emerald_dandelion", "emerald", 1, "dandelion", 1, 12, 1, 0.05),
    t("wandering_trader/emerald_poppy", "emerald", 1, "poppy", 1, 12, 1, 0.05),
    t("wandering_trader/emerald_blue_orchid", "emerald", 1, "blue_orchid", 1, 8, 1, 0.05),
    t("wandering_trader/emerald_allium", "emerald", 1, "allium", 1, 12, 1, 0.05),
    t("wandering_trader/emerald_azure_bluet", "emerald", 1, "azure_bluet", 1, 12, 1, 0.05),
    t("wandering_trader/emerald_red_tulip", "emerald", 1, "red_tulip", 1, 12, 1, 0.05),
    t("wandering_trader/emerald_orange_tulip", "emerald", 1, "orange_tulip", 1, 12, 1, 0.05),
    t("wandering_trader/emerald_white_tulip", "emerald", 1, "white_tulip", 1, 12, 1, 0.05),
    t("wandering_trader/emerald_pink_tulip", "emerald", 1, "pink_tulip", 1, 12, 1, 0.05),
    t("wandering_trader/emerald_oxeye_daisy", "emerald", 1, "oxeye_daisy", 1, 12, 1, 0.05),
    t("wandering_trader/emerald_cornflower", "emerald", 1, "cornflower", 1, 12, 1, 0.05),
    t("wandering_trader/emerald_lily_of_the_valley", "emerald", 1, "lily_of_the_valley", 1, 7, 1, 0.05),
    t("wandering_trader/emerald_open_eyeblossom", "emerald", 1, "open_eyeblossom", 1, 7, 1, 0.05),
    t("wandering_trader/emerald_wheat_seeds", "emerald", 1, "wheat_seeds", 1, 12, 1, 0.05),
    t("wandering_trader/emerald_beetroot_seeds", "emerald", 1, "beetroot_seeds", 1, 12, 1, 0.05),
    t("wandering_trader/emerald_pumpkin_seeds", "emerald", 1, "pumpkin_seeds", 1, 12, 1, 0.05),
    t("wandering_trader/emerald_melon_seeds", "emerald", 1, "melon_seeds", 1, 12, 1, 0.05),
    t("wandering_trader/emerald_acacia_sapling", "emerald", 5, "acacia_sapling", 1, 8, 1, 0.05),
    t("wandering_trader/emerald_birch_sapling", "emerald", 5, "birch_sapling", 1, 8, 1, 0.05),
    t("wandering_trader/emerald_dark_oak_sapling", "emerald", 5, "dark_oak_sapling", 1, 8, 1, 0.05),
    t("wandering_trader/emerald_jungle_sapling", "emerald", 5, "jungle_sapling", 1, 8, 1, 0.05),
    t("wandering_trader/emerald_oak_sapling", "emerald", 5, "oak_sapling", 1, 8, 1, 0.05),
    t("wandering_trader/emerald_spruce_sapling", "emerald", 5, "spruce_sapling", 1, 8, 1, 0.05),
    t("wandering_trader/emerald_cherry_sapling", "emerald", 5, "cherry_sapling", 1, 8, 1, 0.05),
    t("wandering_trader/emerald_pale_oak_sapling", "emerald", 5, "pale_oak_sapling", 1, 8, 1, 0.05),
    t("wandering_trader/emerald_mangrove_propagule", "emerald", 5, "mangrove_propagule", 1, 8, 1, 0.05),
    t("wandering_trader/emerald_poplar_sapling", "emerald", 5, "poplar_sapling", 1, 8, 1, 0.05),
    t("wandering_trader/emerald_brain_coral_block", "emerald", 3, "brain_coral_block", 1, 8, 1, 0.05),
    t("wandering_trader/emerald_bubble_coral_block", "emerald", 3, "bubble_coral_block", 1, 8, 1, 0.05),
    t("wandering_trader/emerald_fire_coral_block", "emerald", 3, "fire_coral_block", 1, 8, 1, 0.05),
    t("wandering_trader/emerald_horn_coral_block", "emerald", 3, "horn_coral_block", 1, 8, 1, 0.05),
    t("wandering_trader/emerald_tube_coral_block", "emerald", 3, "tube_coral_block", 1, 8, 1, 0.05),
    t("wandering_trader/emerald_vine", "emerald", 1, "vine", 3, 4, 1, 0.05),
    t("wandering_trader/emerald_pale_hanging_moss", "emerald", 1, "pale_hanging_moss", 3, 4, 1, 0.05),
    t("wandering_trader/emerald_brown_mushroom", "emerald", 1, "brown_mushroom", 3, 4, 1, 0.05),
    t("wandering_trader/emerald_red_mushroom", "emerald", 1, "red_mushroom", 3, 4, 1, 0.05),
    t("wandering_trader/emerald_lily_pad", "emerald", 1, "lily_pad", 5, 2, 1, 0.05),
    t("wandering_trader/emerald_small_dripleaf", "emerald", 1, "small_dripleaf", 2, 5, 1, 0.05),
    t("wandering_trader/emerald_sand", "emerald", 1, "sand", 8, 8, 1, 0.05),
    t("wandering_trader/emerald_red_sand", "emerald", 1, "red_sand", 4, 6, 1, 0.05),
    t("wandering_trader/emerald_pointed_dripstone", "emerald", 1, "pointed_dripstone", 2, 5, 1, 0.05),
    t("wandering_trader/emerald_sulfur_spike", "emerald", 1, "sulfur_spike", 2, 5, 1, 0.05),
    t("wandering_trader/emerald_rooted_dirt", "emerald", 1, "rooted_dirt", 2, 5, 1, 0.05),
    t("wandering_trader/emerald_moss_block", "emerald", 1, "moss_block", 2, 5, 1, 0.05),
    t("wandering_trader/emerald_pale_moss_block", "emerald", 1, "pale_moss_block", 2, 5, 1, 0.05),
    t("wandering_trader/emerald_wildflowers", "emerald", 1, "wildflowers", 1, 12, 1, 0.05),
    t("wandering_trader/emerald_dry_tall_grass", "emerald", 1, "tall_dry_grass", 1, 12, 1, 0.05),
    t("wandering_trader/emerald_firefly_bush", "emerald", 3, "firefly_bush", 1, 12, 1, 0.05),
    t("wandering_trader/emerald_golden_dandelion", "emerald", 2, "golden_dandelion", 1, 12, 1, 0.05),
    t("wandering_trader/emerald_name_tag", "emerald", 1, "name_tag", 1, 5, 1, 0.05),
    t("wandering_trader/emerald_shelf_mushroom", "emerald", 1, "shelf_mushroom", 3, 12, 1, 0.05),
];
/// `WanderingTrader.updateTrades`: the sets in the order they are added.
#[rustfmt::skip]
pub const WANDERING: [(&str, u8, &[TradeDef]); 3] = [("buying", 2, WANDERING_BUYING), ("uncommon", 2, WANDERING_UNCOMMON), ("common", 5, WANDERING_COMMON)];
