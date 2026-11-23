//! Furnaces, smokers and blast furnaces, following Java 26.3's
//! `AbstractFurnaceBlockEntity.serverTick` tick for tick.
//!
//! A furnace's contents live in [`World`] keyed by position (block
//! entities). They appear when a furnace block is placed, spill into
//! [`World::drops`] when it's removed, keep smelting whenever their chunk
//! is loaded, and swap the block between `FURNACE` and `LIT_FURNACE` (which
//! glows) as the fire starts and goes out.
//!
//! Timers are whole game ticks, as in Java. Every 26.3 smelting, smoking
//! and blasting recipe takes 200 ticks; what makes smokers and blast
//! furnaces fast is the fuel's `cooking/speed_default` multiplier (2 in
//! `#block/fast_cooking` blocks, else 1), which divides the recipe's time,
//! and its `cooking/time_*` provider, which integer-divides the burn time
//! by the same block's reduction factor (2 or 1). Finished items add to a
//! per-recipe tally (Java's `RecipesUsed`) that pays experience only when a
//! player takes from the output or the furnace is broken; hoppers take the
//! output without it.

use glam::IVec3;

use super::World;
use super::block::Block;
use crate::inventory::{Stack, stack_from_str, stack_to_string};
use crate::item::Item;

/// Seconds a normal furnace takes per item (200 ticks), for displays.
pub const COOK_TIME: f32 = 10.0;
/// `cookingtime` of every Java 26.3 smelting, smoking and blasting recipe.
pub const RECIPE_COOK_TICKS: u32 = 200;
/// `AbstractFurnaceBlockEntity.BURN_TIME_STANDARD`: the cooking total when
/// no recipe matches the input.
pub const BURN_TIME_STANDARD: u32 = 200;
/// `AbstractFurnaceBlockEntity.BURN_COOL_SPEED`: how fast progress falls
/// back while the furnace is out and has nothing to light it.
pub const BURN_COOL_SPEED: u32 = 2;
/// `cooking/fast_speed_multiplier` (smokers and blast furnaces).
pub const FAST_SPEED_MULTIPLIER: f32 = 2.0;
/// `cooking/fast_burn_time_reduction_factor`.
pub const FAST_BURN_REDUCTION: u32 = 2;

/// Java's specialized recipe types. Smokers and blast furnaces are
/// `#block/fast_cooking`: twice the speed, half the burn time.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum FurnaceKind {
    #[default]
    Normal,
    Smoker,
    Blast,
}
impl FurnaceKind {
    pub fn of(b: Block) -> Self {
        match b.base() {
            Block::SMOKER | Block::LIT_SMOKER => Self::Smoker,
            Block::BLAST_FURNACE | Block::LIT_BLAST_FURNACE => Self::Blast,
            _ => Self::Normal,
        }
    }
    /// What this furnace makes of `item`: smokers run only Java's `smoking`
    /// recipes and blast furnaces only its `blasting` recipes.
    pub fn smelt(self, item: Item) -> Option<Item> {
        let out = smelt(item)?;
        match self {
            Self::Normal => Some(out),
            Self::Smoker => smokes(item).then_some(out),
            Self::Blast => blasts(item).then_some(out),
        }
    }
    /// Whether the block is in `#block/fast_cooking`.
    pub fn fast(self) -> bool {
        self != Self::Normal
    }
    /// `cooking/speed_default` resolved for this block.
    pub fn speed_multiplier(self) -> f32 {
        if self.fast() { FAST_SPEED_MULTIPLIER } else { 1.0 }
    }
    /// Ticks one of `item` burns here (`cooking/time_*`: the base time
    /// integer-divided by the block's reduction factor), if it's fuel.
    pub fn burn_ticks(self, item: Item) -> Option<u32> {
        let base = crate::content::fuel::ticks(item)?;
        Some(if self.fast() { base / FAST_BURN_REDUCTION } else { base })
    }
    /// Ticks one item takes with fuel burning (`getTotalCookTime`).
    pub fn cook_ticks(self) -> u32 {
        total_cook_ticks(self.speed_multiplier())
    }
    /// Seconds to cook one item with fuel burning.
    pub fn cook_time(self) -> f32 {
        self.cook_ticks() as f32 / 20.0
    }
    fn blocks(self, lit: bool) -> Block {
        match (self, lit) {
            (Self::Normal, false) => Block::FURNACE,
            (Self::Normal, true) => Block::LIT_FURNACE,
            (Self::Smoker, false) => Block::SMOKER,
            (Self::Smoker, true) => Block::LIT_SMOKER,
            (Self::Blast, false) => Block::BLAST_FURNACE,
            (Self::Blast, true) => Block::LIT_BLAST_FURNACE,
        }
    }
}

/// `getTotalCookTime(recipe, entity)`: the recipe's time divided by the
/// speed multiplier, rounded up (`(int) Math.ceil(time / speed)`).
pub fn total_cook_ticks(speed: f32) -> u32 {
    if speed > 0.0 { (RECIPE_COOK_TICKS as f32 / speed).ceil() as u32 } else { RECIPE_COOK_TICKS }
}

/// One entry of the furnace's `RecipesUsed` tally: the recipe (keyed by
/// [`recipe_key`]), its experience per item and how many it has made.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RecipeUse {
    pub recipe: Item,
    pub xp: f32,
    pub count: u32,
}

/// The item that identifies the Java cooking recipe turning `input` into
/// `out`. Most 26.3 cooking recipes take one item, so the input names the
/// recipe; the ones with a list or tag ingredient (`charcoal`, `glass`,
/// `leaf_litter` and the `*_nugget_from_*` melt-downs) are one recipe for
/// all of their inputs, so their result names them.
pub fn recipe_key(input: Item, out: Item) -> Item {
    let b = Item::from_block;
    let shared = matches!(out, Item::CHARCOAL | Item::IRON_NUGGET | Item::GOLD_NUGGET | Item::COPPER_NUGGET)
        || out == b(Block::GLASS)
        || out == b(super::biome_plants::LEAF_LITTER);
    if shared { out } else { input }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Furnace {
    pub kind: FurnaceKind,
    pub input: Option<Stack>,
    pub fuel: Option<Stack>,
    pub output: Option<Stack>,
    /// `litTimeRemaining` and `litTotalTime`, in ticks.
    pub lit: u32,
    pub lit_total: u32,
    /// `cookingTimer` and `cookingTotalTime`, in ticks.
    pub cook: u32,
    pub cook_total: u32,
    /// `speedMultiplier`: set from each fuel as it's lit.
    pub speed: f32,
    /// `RecipesUsed`: experience owed for finished items, paid when a
    /// player takes from the output or the furnace is broken.
    pub recipes_used: Vec<RecipeUse>,
    /// Fraction of a tick carried between [`Furnace::tick`] calls (not
    /// saved; public so callers can build furnaces with struct update).
    pub carry: f32,
}

impl Default for Furnace {
    fn default() -> Self {
        Self {
            kind: FurnaceKind::Normal,
            input: None,
            fuel: None,
            output: None,
            lit: 0,
            lit_total: 0,
            cook: 0,
            cook_total: 0,
            speed: 1.0,
            recipes_used: Vec::new(),
            carry: 0.0,
        }
    }
}

/// Java's `*_from_smoking` inputs: raw meat and fish, potatoes and kelp.
fn smokes(item: Item) -> bool {
    matches!(
        item,
        Item::RAW_PORKCHOP
            | Item::RAW_BEEF
            | Item::RAW_CHICKEN
            | Item::RAW_RABBIT
            | Item::RAW_MUTTON
            | Item::POTATO
            | Item::COD
            | Item::SALMON
    ) || item == Item::from_block(crate::world::overworld_blocks::KELP)
}

/// Java's `*_from_blasting` inputs: ores, raw metal, ancient debris and the
/// metal gear that melts into nuggets.
fn blasts(item: Item) -> bool {
    if item == Item::from_block(super::gen5_blocks::NETHER_GOLD_ORE) {
        return true;
    }
    ore_of(item).is_some_and(|b| {
        matches!(
            b,
            Block::IRON_ORE
                | Block::GOLD_ORE
                | Block::COPPER_ORE
                | Block::COAL_ORE
                | Block::DIAMOND_ORE
                | Block::EMERALD_ORE
                | Block::LAPIS_ORE
                | Block::REDSTONE_ORE
                | Block::QUARTZ_ORE
                | Block::ANCIENT_DEBRIS
        )
    }) || matches!(item, Item::RAW_IRON | Item::RAW_GOLD | Item::RAW_COPPER)
        || nugget_of(item).is_some()
}

fn ore_of(item: Item) -> Option<Block> {
    item.block().map(Block::as_stone_ore)
}

/// What smelting `item` produces.
/// Java's `*_nugget_from_smelting`/`_blasting`: iron, golden and copper
/// tools, armor, horse armor and nautilus armor, and chainmail armor, melt
/// down to one nugget.
fn nugget_of(item: Item) -> Option<Item> {
    use crate::item::{ArmorMaterial, Tier};
    // Spears melt down like the tier's other tools.
    let tier = item.as_tool().map(|(_, t)| t).or_else(|| crate::weapons::spear_tier(item));
    let material = item.as_armor().map(|(_, m)| m).or(item.body_armor().map(|(_, m)| m));
    Some(match (tier, material) {
        (Some(Tier::Iron), _) | (_, Some(ArmorMaterial::Iron | ArmorMaterial::Chainmail)) => Item::IRON_NUGGET,
        (Some(Tier::Gold), _) | (_, Some(ArmorMaterial::Gold)) => Item::GOLD_NUGGET,
        (Some(Tier::Copper), _) | (_, Some(ArmorMaterial::Copper)) => Item::COPPER_NUGGET,
        _ => return None,
    })
}

pub fn smelt(item: Item) -> Option<Item> {
    let b = Item::from_block;
    if let Some(nugget) = nugget_of(item) {
        return Some(nugget);
    }
    if let Some(out) = super::stone_blocks::smelt(item) {
        return Some(out);
    }
    Some(match item {
        i if i == b(super::creaking::RESIN_CLUMP) => Item::RESIN_BRICK,
        // `smelting/popped_chorus_fruit` (smelting only).
        crate::content::chorus::CHORUS_FRUIT => crate::content::chorus::POPPED_CHORUS_FRUIT,
        // v0.16 Track D: `gold_ingot_from_smelting_nether_gold_ore`.
        i if i == b(super::gen5_blocks::NETHER_GOLD_ORE) => Item::GOLD_INGOT,
        i if ore_of(i) == Some(Block::IRON_ORE) || i == Item::RAW_IRON => Item::IRON_INGOT,
        i if ore_of(i) == Some(Block::GOLD_ORE) || i == Item::RAW_GOLD => Item::GOLD_INGOT,
        i if ore_of(i) == Some(Block::COPPER_ORE) || i == Item::RAW_COPPER => Item::COPPER_INGOT,
        i if ore_of(i) == Some(Block::COAL_ORE) => Item::COAL,
        i if ore_of(i) == Some(Block::DIAMOND_ORE) => Item::DIAMOND,
        i if ore_of(i) == Some(Block::LAPIS_ORE) => Item::LAPIS_LAZULI,
        i if ore_of(i) == Some(Block::REDSTONE_ORE) => Item::REDSTONE,
        i if ore_of(i) == Some(Block::EMERALD_ORE) => Item::EMERALD,
        i if i == b(Block::POLISHED_BLACKSTONE_BRICKS) => b(Block::CRACKED_POLISHED_BLACKSTONE_BRICKS),
        i if i == b(Block::STONE) => b(Block::SMOOTH_STONE),
        i if i == b(Block::COBBLED_DEEPSLATE) => b(Block::DEEPSLATE),
        i if i == b(Block::ANCIENT_DEBRIS) => Item::NETHERITE_SCRAP,
        i if i == b(Block::CACTUS) => crate::color::DyeColor::Green.dye(),
        // `#smelts_to_glass`.
        i if i == b(Block::SAND) || i == b(Block::RED_SAND) => b(Block::GLASS),
        i if i == b(Block::COBBLESTONE) => b(Block::STONE),
        i if i.block().is_some_and(Block::is_log) => Item::CHARCOAL,
        // `#leaves` smelt to leaf litter.
        i if i.block().is_some_and(Block::is_leaves) => b(crate::world::biome_plants::LEAF_LITTER),
        Item::CLAY_BALL => Item::BRICK,
        i if i == b(Block::CLAY) => b(Block::TERRACOTTA),
        i if let Some(c) = i.block().and_then(Block::stained_terracotta_color) => b(Block::glazed(c)),
        i if i == b(Block::NETHERRACK) => Item::NETHER_BRICK,
        i if i == b(Block::QUARTZ_ORE) => Item::NETHER_QUARTZ,
        Item::RAW_PORKCHOP => Item::COOKED_PORKCHOP,
        Item::RAW_BEEF => Item::STEAK,
        Item::RAW_CHICKEN => Item::COOKED_CHICKEN,
        Item::RAW_RABBIT => Item::COOKED_RABBIT,
        Item::RAW_MUTTON => Item::COOKED_MUTTON,
        Item::POTATO => Item::BAKED_POTATO,
        Item::COD => Item::COOKED_COD,
        Item::SALMON => Item::COOKED_SALMON,
        i if i == b(crate::world::overworld_blocks::KELP) => Item::DRIED_KELP,
        i if i == b(crate::world::overworld_blocks::WET_SPONGE) => b(crate::world::overworld_blocks::SPONGE),
        i if i == b(Block::BASALT) => b(crate::world::overworld_blocks::SMOOTH_BASALT),
        i if i == b(crate::world::overworld_blocks::PALE_OAK_LOG) => Item::CHARCOAL,
        i if i == b(crate::world::overworld_blocks::SEA_PICKLE) => crate::color::DyeColor::Lime.dye(),
        _ => return None,
    })
}

/// Experience Java awards per item smelted into `out`.
pub fn smelt_xp(out: Item) -> f32 {
    let b = Item::from_block;
    match out {
        Item::GOLD_INGOT | Item::DIAMOND | Item::EMERALD => 1.0,
        i if i == crate::color::DyeColor::Green.dye() => 1.0,
        Item::IRON_INGOT | Item::COPPER_INGOT | Item::REDSTONE => 0.7,
        Item::NETHERITE_SCRAP => 2.0,
        Item::COOKED_PORKCHOP
        | Item::STEAK
        | Item::COOKED_CHICKEN
        | Item::COOKED_MUTTON
        | Item::COOKED_RABBIT
        | Item::BAKED_POTATO
        | Item::COOKED_COD
        | Item::COOKED_SALMON => 0.35,
        i if i == b(Block::TERRACOTTA) => 0.35,
        Item::BRICK => 0.3,
        Item::NETHER_QUARTZ | Item::LAPIS_LAZULI => 0.2,
        Item::CHARCOAL => 0.15,
        // `smelting/resin_brick`: 0.1.
        Item::RESIN_BRICK => 0.1,
        i if i == b(crate::world::overworld_blocks::SPONGE) => 0.15,
        _ => 0.1,
    }
}

/// Seconds one of `item` burns as fuel in a normal furnace (ticks / 20).
pub fn burn_time(item: Item) -> Option<f32> {
    crate::content::fuel::ticks(item).map(|ticks| ticks as f32 / 20.0)
}

/// `ItemStack.isSameItemSameComponents`: whether two stacks stack.
fn same_item(a: Stack, b: Stack) -> bool {
    a.stacks_with(&b)
}

/// Pays out experience for `count` finished items worth `xp` each
/// (`createExperience`): the whole part, plus one with the fractional
/// part as its chance, all in Java's float arithmetic.
pub fn award(count: u32, xp: f32, roll: f32) -> u32 {
    let total = count as f32 * xp;
    let whole = total.floor();
    let fraction = total - whole;
    whole as u32 + (fraction != 0.0 && roll < fraction) as u32
}

impl Furnace {
    /// `setItem` on the input slot: a different item (or an emptied slot)
    /// restarts cooking, with the total worked out for the new input.
    pub fn set_input(&mut self, stack: Option<Stack>) {
        let same = matches!((self.input, stack), (Some(old), Some(new)) if same_item(old, new));
        self.input = stack;
        if !same {
            self.cook_total = self.total_cook_time();
            self.cook = 0;
        }
    }

    /// Changes the input through the same slot interaction as the inventory.
    /// Adding or removing some of the same item keeps cooking progress;
    /// putting in a different item, or refilling an emptied slot, restarts
    /// it (Java calls `setItem` then; taking items out doesn't).
    pub fn click_input(&mut self, cursor: &mut Option<Stack>, right: bool) {
        let before = self.input;
        let mut slot = before;
        crate::inventory::click_slot(&mut slot, cursor, right);
        match (before, slot) {
            (Some(old), Some(new)) if same_item(old, new) => self.input = slot,
            (_, Some(_)) => self.set_input(slot),
            (_, None) => self.input = None,
        }
    }

    pub fn is_lit(&self) -> bool {
        self.lit > 0
    }

    /// How far the current item is (`cookingTimer / cookingTotalTime`).
    pub fn cook_progress(&self) -> f32 {
        if self.cook_total == 0 { 0.0 } else { (self.cook as f32 / self.cook_total as f32).clamp(0.0, 1.0) }
    }

    /// How much of the current fuel is left.
    pub fn burn_progress(&self) -> f32 {
        if self.lit_total == 0 { 0.0 } else { (self.lit as f32 / self.lit_total as f32).clamp(0.0, 1.0) }
    }

    /// Experience the tally holds, before rounding.
    pub fn stored_xp(&self) -> f32 {
        self.recipes_used.iter().map(|r| r.count as f32 * r.xp).sum()
    }

    /// `getTotalCookTime(level, entity)`: the input's recipe time at the
    /// current speed, or [`BURN_TIME_STANDARD`] without a recipe.
    fn total_cook_time(&self) -> u32 {
        match self.input.and_then(|s| self.kind.smelt(s.item)) {
            Some(_) => total_cook_ticks(self.speed),
            None => BURN_TIME_STANDARD,
        }
    }

    /// `canBurn`: the result fits in the output slot.
    fn can_burn(&self, out: Item) -> bool {
        match self.output {
            None => true,
            Some(o) => same_item(o, Stack::new(out, 1)) && o.count < o.max(),
        }
    }

    fn record(&mut self, recipe: Item, xp: f32) {
        match self.recipes_used.iter_mut().find(|r| r.recipe == recipe) {
            Some(r) => r.count = r.count.saturating_add(1),
            None => self.recipes_used.push(RecipeUse { recipe, xp, count: 1 }),
        }
    }

    /// One `serverTick`. Returns a fuel remainder that had no room in the
    /// fuel slot, which Java drops at the furnace.
    pub fn step(&mut self) -> Option<Stack> {
        let mut dropped = None;
        let mut is_lit = false;
        if self.lit > 0 {
            self.lit -= 1;
            is_lit = self.lit > 0;
        }
        let has_input = self.input.is_some();
        let has_fuel = self.fuel.is_some();
        if is_lit || has_fuel && has_input {
            let recipe = self.input.and_then(|input| Some((input, self.kind.smelt(input.item)?)));
            match recipe {
                Some((input, out)) if self.can_burn(out) => {
                    if !is_lit {
                        let fuel = self.fuel.map(|f| f.item);
                        let new_lit = fuel.and_then(|f| self.kind.burn_ticks(f)).unwrap_or(0);
                        // `getSpeedMultiplier`: the fuel's `speed_default`, or
                        // 1 for something with no cooking fuel component.
                        let speed = match fuel {
                            Some(f) if crate::content::fuel::ticks(f).is_some() => self.kind.speed_multiplier(),
                            _ => 1.0,
                        };
                        self.lit = new_lit;
                        self.lit_total = new_lit;
                        self.speed = speed;
                        if self.cook_total > 0 && self.cook < self.cook_total {
                            let ratio = self.cook as f32 / self.cook_total as f32;
                            self.cook_total = total_cook_ticks(self.speed);
                            self.cook = (ratio * self.cook_total as f32).ceil() as u32;
                        }
                        if new_lit > 0 {
                            if self.cook_total == 0 {
                                // Java sets the total whenever `setItem` changes
                                // the input; inputs written straight into the
                                // slot (world generation, tests) get it here.
                                self.cook_total = total_cook_ticks(self.speed);
                            }
                            dropped = self.consume_fuel();
                            is_lit = true;
                        }
                    }
                    if is_lit {
                        self.cook += 1;
                        if self.cook >= self.cook_total {
                            self.cook = 0;
                            self.cook_total = total_cook_ticks(self.speed);
                            self.burn(input, out);
                            self.record(recipe_key(input.item, out), smelt_xp(out));
                        }
                    } else {
                        self.cook = 0;
                    }
                }
                Some(_) => self.cook = 0,
                // An input with no recipe here leaves the timer alone.
                None if has_input => {}
                None => self.cook = 0,
            }
        } else if self.cook > 0 {
            self.cook = self.cook.saturating_sub(BURN_COOL_SPEED).min(self.cook_total);
        }
        dropped
    }

    /// `consumeFuel`: one fuel item goes; its crafting remainder (a lava
    /// bucket's bucket) takes the emptied slot or is dropped.
    fn consume_fuel(&mut self) -> Option<Stack> {
        let fuel = self.fuel?;
        let remainder = if fuel.item == Item::LAVA_BUCKET { Some(Item::BUCKET) } else { fuel.item.remainder() };
        self.fuel = take_one(self.fuel);
        let remainder = remainder.map(|i| Stack::new(i, 1));
        if self.fuel.is_none() {
            self.fuel = remainder;
            None
        } else {
            remainder
        }
    }

    /// `burn`: the result goes to the output and one input is used; a wet
    /// sponge also fills a bucket waiting in the fuel slot.
    fn burn(&mut self, input: Stack, out: Item) {
        match &mut self.output {
            Some(o) => o.count += 1,
            None => self.output = Some(Stack::new(out, 1)),
        }
        if input.item == Item::from_block(crate::world::overworld_blocks::WET_SPONGE)
            && self.fuel.is_some_and(|f| f.item == Item::BUCKET)
        {
            self.fuel = Some(Stack::new(Item::WATER_BUCKET, 1));
        }
        self.input = take_one(self.input);
    }

    /// Runs the whole game ticks in `dt` seconds (20 per second), carrying
    /// any fraction to the next call. Returns dropped fuel remainders.
    pub fn tick(&mut self, dt: f32) -> Vec<Stack> {
        let ticks = self.carry + dt * 20.0;
        let whole = (ticks + 1e-3).floor().max(0.0);
        self.carry = ticks - whole;
        (0..whole as u32).filter_map(|_| self.step()).collect()
    }

    /// Pays out and clears the tally (`getRecipesToAwardAndPopExperience`):
    /// each recipe rounds on its own, drawing a fresh `roll` in 0..1.
    pub fn take_xp(&mut self, mut roll: impl FnMut() -> f32) -> u32 {
        std::mem::take(&mut self.recipes_used).into_iter().map(|r| award(r.count, r.xp, roll())).sum()
    }

    /// A player took from the output (`FurnaceResultSlot`): if the output
    /// shrank from `before`, even partly, the whole tally is paid. A take
    /// that moved nothing pays nothing and keeps it.
    pub fn take_output_xp(&mut self, before: u8, roll: impl FnMut() -> f32) -> Option<u32> {
        (self.output.map_or(0, |s| s.count) < before).then(|| self.take_xp(roll))
    }

    /// Everything inside, for when the furnace is broken.
    pub fn take_all(&mut self) -> Vec<Stack> {
        [self.input.take(), self.fuel.take(), self.output.take()].into_iter().flatten().collect()
    }

    /// Saves the slots, the four timers, the speed, the kind and the tally:
    /// `input;fuel;output;lit;lit_total;cook;cook_total;speed;kind;recipes`.
    pub fn serialize(&self) -> String {
        let recipes = if self.recipes_used.is_empty() {
            "-".to_string()
        } else {
            self.recipes_used
                .iter()
                .map(|r| format!("{}:{}:{}", r.recipe.0, r.xp, r.count))
                .collect::<Vec<_>>()
                .join(",")
        };
        format!(
            "{};{};{};{};{};{};{};{};{};{}",
            stack_to_string(self.input),
            stack_to_string(self.fuel),
            stack_to_string(self.output),
            self.lit,
            self.lit_total,
            self.cook,
            self.cook_total,
            self.speed,
            self.kind as u8,
            recipes
        )
    }

    /// Restores saved furnace state. Saves from before v0.17 kept seconds
    /// (doubled in fast furnaces) and one experience total; they convert to
    /// ticks, and the total becomes one tally entry that pays the same.
    pub fn deserialize(text: &str) -> Option<Self> {
        let f: Vec<&str> = text.split(';').collect();
        let kind = |s: &str| match s {
            "0" => Some(FurnaceKind::Normal),
            "1" => Some(FurnaceKind::Smoker),
            "2" => Some(FurnaceKind::Blast),
            _ => None,
        };
        if let [input, fuel, output, lit, lit_total, cook, cook_total, speed, k, recipes] = f[..] {
            let int = |s: &str| s.parse::<u32>().ok();
            let speed = speed.parse::<f32>().ok().filter(|v| v.is_finite() && *v >= 0.0)?;
            let mut recipes_used = Vec::new();
            for entry in recipes.split(',').filter(|e| !e.is_empty() && *e != "-") {
                let [recipe, xp, count] = entry.split(':').collect::<Vec<_>>()[..] else { return None };
                let xp = xp.parse::<f32>().ok().filter(|v| v.is_finite() && *v >= 0.0)?;
                recipes_used.push(RecipeUse { recipe: Item(recipe.parse().ok()?), xp, count: int(count)? });
            }
            let cook_total = int(cook_total)?;
            return Some(Self {
                kind: kind(k)?,
                input: stack_from_str(input)?,
                fuel: stack_from_str(fuel)?,
                output: stack_from_str(output)?,
                lit: int(lit)?,
                lit_total: int(lit_total)?,
                cook: int(cook)?.min(cook_total.max(BURN_TIME_STANDARD)),
                cook_total,
                speed,
                recipes_used,
                carry: 0.0,
            });
        }
        // Saves from before experience have six fields, before kinds seven.
        let (input, fuel, output, left, total, cook, xp, k) = match f[..] {
            [input, fuel, output, left, total, cook] => (input, fuel, output, left, total, cook, "0", "0"),
            [input, fuel, output, left, total, cook, xp] => (input, fuel, output, left, total, cook, xp, "0"),
            [input, fuel, output, left, total, cook, xp, kind] => (input, fuel, output, left, total, cook, xp, kind),
            _ => return None,
        };
        let num = |s: &str| s.parse::<f32>().ok().filter(|v| v.is_finite() && *v >= 0.0);
        let kind = kind(k)?;
        // Fast furnaces ran their timers at double speed in seconds.
        let scale = if kind.fast() { 10.0 } else { 20.0 };
        let ticks = |secs: f32| (secs * scale).round() as u32;
        let cook_total = kind.cook_ticks();
        let xp = num(xp)?.min(1e6);
        Some(Self {
            kind,
            input: stack_from_str(input)?,
            fuel: stack_from_str(fuel)?,
            output: stack_from_str(output)?,
            lit: ticks(num(left)?),
            lit_total: ticks(num(total)?),
            cook: ticks(num(cook)?).min(cook_total),
            cook_total,
            speed: kind.speed_multiplier(),
            recipes_used: if xp > 0.0 { vec![RecipeUse { recipe: Item(0), xp, count: 1 }] } else { Vec::new() },
            carry: 0.0,
        })
    }
}

fn take_one(stack: Option<Stack>) -> Option<Stack> {
    stack.filter(|s| s.count > 1).map(|s| Stack { count: s.count - 1, ..s })
}

pub fn is_furnace(b: Block) -> bool {
    matches!(
        b.base(),
        Block::FURNACE
            | Block::LIT_FURNACE
            | Block::SMOKER
            | Block::LIT_SMOKER
            | Block::BLAST_FURNACE
            | Block::LIT_BLAST_FURNACE
    )
}

impl World {
    pub fn furnace(&self, p: IVec3) -> Option<&Furnace> {
        self.furnaces.get(&p)
    }

    pub fn furnace_mut(&mut self, p: IVec3) -> Option<&mut Furnace> {
        self.redstone_changed(p);
        self.furnaces.get_mut(&p)
    }

    /// Keeps the furnace table in step with a block change at `p`.
    pub(super) fn track_furnace(&mut self, p: IVec3, old: Block, new: Block) {
        if is_furnace(old) && !is_furnace(new) {
            if let Some(mut f) = self.furnaces.remove(&p)
                && self.tile_drops
            {
                self.drops.extend(f.take_all().into_iter().map(|s| (p, s)));
                // `preRemoveSideEffects` pops the tally at the block's centre.
                let xp = f.take_xp(|| (self.roll() >> 40) as f32 / (1u64 << 24) as f32);
                if xp > 0 {
                    self.xp_drops.push((p, xp));
                }
            }
        } else if is_furnace(new) {
            self.furnaces.entry(p).or_default().kind = FurnaceKind::of(new);
        }
    }

    /// Smelts in every furnace whose chunk is block-ticking and lights or puts out
    /// their blocks to match.
    pub fn tick_furnaces(&mut self, dt: f64) {
        let mut relight = Vec::new();
        let activity = &self.activity;
        for (&p, f) in self.furnaces.iter_mut() {
            if !activity.contains(p, true) || !self.chunks.contains_key(&super::chunk::chunk_of(p)) {
                continue;
            }
            if let Some(b) = self.chunks.get(&super::chunk::chunk_of(p)).map(|c| {
                c.data.get(
                    super::chunk::local_of(p).x as usize,
                    super::chunk::local_of(p).y as usize,
                    super::chunk::local_of(p).z as usize,
                )
            }) {
                f.kind = FurnaceKind::of(b);
            }
            let before = [f.input, f.fuel, f.output];
            let dropped = f.tick(dt as f32);
            relight.push((p, f.is_lit(), before != [f.input, f.fuel, f.output], dropped));
        }
        for (p, lit, changed, dropped) in relight {
            self.drops.extend(dropped.into_iter().map(|s| (p, s)));
            if changed {
                self.redstone_changed(p);
            }
            let Some((_, facing)) = self.get_block(p).filter(|&b| is_furnace(b)).and_then(Block::oriented) else {
                continue;
            };
            let want = FurnaceKind::of(self.get_block(p).unwrap()).blocks(lit).with_facing(facing);
            if self.get_block(p) != Some(want) {
                self.edit(p, want, false);
            }
        }
    }

    /// `x,y,z=furnace|...` for the level file.
    pub fn furnaces_to_string(&self) -> String {
        self.furnaces
            .iter()
            .map(|(p, f)| format!("{},{},{}={}", p.x, p.y, p.z, f.serialize()))
            .collect::<Vec<_>>()
            .join("|")
    }

    /// Restores furnaces saved by [`World::furnaces_to_string`]; malformed
    /// entries are skipped.
    pub fn load_furnaces(&mut self, text: &str) {
        for entry in text.split('|').filter(|e| !e.is_empty()) {
            let Some((pos, state)) = entry.split_once('=') else { continue };
            let c: Vec<i32> = pos.split(',').filter_map(|v| v.parse().ok()).collect();
            if let (&[x, y, z], Some(f)) = (&c[..], Furnace::deserialize(state)) {
                self.furnaces.insert(IVec3::new(x, y, z), f);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn furnace(input: Item, n: u8, fuel: Item, m: u8) -> Furnace {
        Furnace { input: Some(Stack::new(input, n)), fuel: Some(Stack::new(fuel, m)), ..Default::default() }
    }

    fn run(f: &mut Furnace, ticks: u32) {
        for _ in 0..ticks {
            f.step();
        }
    }

    #[test]
    fn fast_furnaces_filter_recipes_double_speed_and_halve_fuel_and_save_kind() {
        for (kind, input, output, reject) in [
            (FurnaceKind::Smoker, Item::RAW_BEEF, Item::STEAK, Item::RAW_IRON),
            (FurnaceKind::Blast, Item::RAW_IRON, Item::IRON_INGOT, Item::RAW_BEEF),
        ] {
            let mut f = furnace(input, 64, Item::COAL, 1);
            f.kind = kind;
            run(&mut f, 100);
            assert_eq!(f.output, Some(Stack::new(output, 1)), "100 ticks per item");
            assert_eq!((f.lit_total, f.speed), (800, 2.0), "coal burns 1600 / 2 ticks at double speed");
            run(&mut f, 800);
            assert!(!f.is_lit());
            assert_eq!(f.output.unwrap().count, 8, "one coal always smelts eight items");
            let saved = Furnace::deserialize(&f.serialize()).unwrap();
            assert_eq!(saved, Furnace { carry: saved.carry, ..f.clone() });
            let mut g = furnace(reject, 1, Item::COAL, 1);
            g.kind = kind;
            run(&mut g, 400);
            assert_eq!(g.fuel.unwrap().count, 1);
            assert!(g.output.is_none());
        }
        assert!(FurnaceKind::Blast.smelt(Block::SAND.into()).is_none());
        assert!(FurnaceKind::Blast.smelt(Block::ANCIENT_DEBRIS.into()).is_some());
        assert_eq!(FurnaceKind::Smoker.burn_ticks(Item::from_name("dried_kelp_block").unwrap()), Some(2000));
    }

    #[test]
    fn ancient_debris_smelts_into_scrap_and_awards_two_xp_per_item() {
        let mut f = furnace(Block::ANCIENT_DEBRIS.into(), 2, Item::COAL, 1);
        run(&mut f, 400);
        assert_eq!(f.input, None);
        assert_eq!(f.output, Some(Stack::new(Item::NETHERITE_SCRAP, 2)));
        assert_eq!(f.stored_xp(), 4.0);
        assert_eq!(f.take_output_xp(2, || 0.5), None, "no XP before taking the output");
        f.output = None;
        assert_eq!(f.take_output_xp(2, || 0.5), Some(4));
        assert_eq!(f.stored_xp(), 0.0);
    }

    #[test]
    fn lava_buckets_burn_long_and_leave_the_bucket() {
        let mut f = furnace(Item::from_block(Block::COBBLESTONE), 64, Item::LAVA_BUCKET, 1);
        f.step();
        assert!(f.is_lit());
        assert_eq!(f.fuel, Some(Stack::new(Item::BUCKET, 1)));
        run(&mut f, 12_000);
        assert_eq!(f.output.map(|s| s.count), Some(60), "still burning after 600 s");
    }

    #[test]
    fn a_fuel_of_n_ticks_cooks_n_ticks_and_the_lit_state_follows_java() {
        let ore = Item::from_block(Block::IRON_ORE);
        // A stick burns 100 ticks: half an ingot's worth, then the fire is out.
        let mut f = furnace(ore, 3, Item::STICK, 1);
        f.step();
        assert!(f.is_lit() && f.fuel.is_none() && f.cook == 1);
        run(&mut f, 99);
        assert!(f.is_lit() && f.cook == 100, "the 100th tick still cooks");
        f.step();
        assert!(!f.is_lit(), "it goes out on the tick the timer reaches 0");
        assert_eq!(f.cook, 98, "with no fuel left, progress cools by two");
        // Coal burns 1600 ticks: all three ores, then idles on until it burns out.
        let mut f = furnace(ore, 3, Item::COAL, 1);
        run(&mut f, 200);
        assert_eq!(f.output, Some(Stack::new(Item::IRON_INGOT, 1)));
        run(&mut f, 400);
        assert_eq!(f.output, Some(Stack::new(Item::IRON_INGOT, 3)));
        assert!(f.input.is_none() && f.is_lit());
        // A big time step behaves the same as small ones.
        let mut g = furnace(ore, 3, Item::COAL, 1);
        g.tick(30.0);
        assert_eq!(g.output, f.output);
    }

    #[test]
    fn progress_cools_by_two_a_tick_with_nothing_to_burn() {
        let mut f = furnace(Item::RAW_BEEF, 2, Item::STICK, 1);
        run(&mut f, 50);
        assert_eq!((f.cook, f.lit), (50, 51));
        f.input = None;
        f.step();
        assert_eq!(f.cook, 0, "burning with an empty input slot resets");
        let mut f = furnace(Item::RAW_BEEF, 2, Item::STICK, 1);
        run(&mut f, 100);
        assert_eq!((f.cook, f.lit), (100, 1));
        f.step();
        assert!(!f.is_lit());
        assert_eq!(f.cook, 98, "out, with nothing to light it: `BURN_COOL_SPEED`");
        let mut f = Furnace { cook: 7, cook_total: 200, ..Default::default() };
        f.step();
        assert_eq!(f.cook, 5);
        run(&mut f, 3);
        assert_eq!(f.cook, 0);
    }

    #[test]
    fn relighting_rescales_progress_to_the_new_speed() {
        // A smoker part way through a 200-tick total (set while unlit at
        // speed 1) rescales to its fast total when lit: ceil(ratio * 100).
        let mut f = Furnace { kind: FurnaceKind::Smoker, ..Default::default() };
        f.set_input(Some(Stack::new(Item::RAW_BEEF, 1)));
        assert_eq!(f.cook_total, 200);
        f.cook = 51;
        f.fuel = Some(Stack::new(Item::STICK, 1));
        f.step();
        assert_eq!((f.cook_total, f.cook), (100, 27), "ceil(51 / 200 * 100) then one tick");
    }

    #[test]
    fn needs_a_smeltable_input_and_room_for_the_output() {
        // Nothing to smelt: the fuel isn't wasted.
        let mut f = furnace(Item::STICK, 1, Item::COAL, 1);
        run(&mut f, 20);
        assert!(!f.is_lit() && f.fuel.is_some());
        // Output full of something else: no burning either.
        let mut f = furnace(Item::RAW_BEEF, 1, Item::COAL, 1);
        f.output = Some(Stack::new(Item::COAL, 1));
        run(&mut f, 20);
        assert!(!f.is_lit());
        // A renamed result doesn't stack with fresh ones.
        let mut f = furnace(Item::RAW_BEEF, 1, Item::COAL, 1);
        f.output = Some(Stack::new(Item::STEAK, 1).with_name("Dinner").unwrap());
        run(&mut f, 20);
        assert!(!f.is_lit());
        assert_eq!(smelt(Item::RAW_BEEF), Some(Item::STEAK));
        assert!(burn_time(Item::IRON_INGOT).is_none());
        assert_eq!(burn_time(Item::from(Block::SMITHING_TABLE)), Some(15.0));
    }

    #[test]
    fn a_wet_sponge_fills_a_bucket_in_the_fuel_slot() {
        let sponge = Item::from_block(crate::world::overworld_blocks::WET_SPONGE);
        let mut f = furnace(sponge, 2, Item::COAL, 1);
        f.step();
        f.fuel = Some(Stack::new(Item::BUCKET, 1));
        run(&mut f, 199);
        assert_eq!(f.fuel, Some(Stack::new(Item::WATER_BUCKET, 1)));
        assert_eq!(f.output.map(|s| s.count), Some(1));
    }

    #[test]
    fn output_transfers_release_stored_xp_once() {
        use crate::inventory::move_into;

        let mut f = furnace(Item::from_block(Block::GOLD_ORE), 8, Item::COAL, 2);
        run(&mut f, 1601);
        assert_eq!(f.stored_xp(), 8.0);
        let mut inventory = [Some(Stack::new(Item::GOLD_INGOT, 64))];
        let stack = f.output.take().unwrap();
        f.output = move_into(stack, &mut inventory, &[0]);
        assert_eq!(f.take_output_xp(stack.count, || 0.5), None, "a full inventory does not release XP");
        assert_eq!(f.stored_xp(), 8.0);

        inventory[0].as_mut().unwrap().count = 63;
        let stack = f.output.take().unwrap();
        f.output = move_into(stack, &mut inventory, &[0]);
        assert_eq!(f.output.unwrap().count, 7, "only one item fits");
        assert_eq!(f.take_output_xp(stack.count, || 0.5), Some(8), "a partial take releases the entire tally");
        assert_eq!(f.stored_xp(), 0.0);

        inventory[0] = None;
        let stack = f.output.take().unwrap();
        f.output = move_into(stack, &mut inventory, &[0]);
        assert_eq!(f.output, None);
        assert_eq!(f.take_output_xp(stack.count, || 0.5), Some(0), "taking the leftovers cannot award XP twice");
        f.set_input(Some(Stack::new(Block::GOLD_ORE, 1)));
        f.fuel = Some(Stack::new(Item::COAL, 1));
        run(&mut f, 201);
        f.output.take();
        assert_eq!(f.take_output_xp(1, || 0.5), Some(1), "new smelts earn fresh XP");
    }

    #[test]
    fn each_recipe_rounds_its_own_fraction() {
        // Iron ore and raw iron are separate Java recipes (0.7 each): two
        // tallies of 0.7, each rounding up on a roll below 0.7.
        let mut f = furnace(Item::from_block(Block::IRON_ORE), 1, Item::COAL, 1);
        run(&mut f, 200);
        f.set_input(Some(Stack::new(Item::RAW_IRON, 1)));
        run(&mut f, 200);
        assert_eq!(f.recipes_used.len(), 2);
        let mut rolls = [0.69, 0.71].into_iter();
        assert_eq!(f.clone().take_xp(|| rolls.next().unwrap()), 1);
        assert_eq!(f.clone().take_xp(|| 0.5), 2);
        assert_eq!(f.take_xp(|| 0.9), 0);
        // Sand and red sand share Java's `glass` recipe.
        assert_eq!(
            recipe_key(Block::SAND.into(), Block::GLASS.into()),
            recipe_key(Block::RED_SAND.into(), Block::GLASS.into())
        );
        assert_eq!(award(3, 0.1, 0.5), 0, "3 * 0.1 = 0.3 rounds up only on a roll below 0.3");
        assert_eq!(award(3, 0.1, 0.2), 1);
        assert_eq!(award(10, 0.7, 0.99), 7, "a whole total never rounds up");
    }

    #[test]
    fn old_saves_convert_seconds_to_ticks_and_keep_their_experience() {
        let old = "-;-;-;0.00;0.00;0.00";
        assert_eq!(Furnace::deserialize(old).map(|f| f.stored_xp()), Some(0.0), "older saves load");
        let raw = stack_to_string(Some(Stack::new(Item::RAW_IRON, 3)));
        let coal = stack_to_string(Some(Stack::new(Item::COAL, 1)));
        let normal = Furnace::deserialize(&format!("{raw};{coal};-;40.00;80.00;4.00;2.10;0")).unwrap();
        assert_eq!((normal.lit, normal.lit_total, normal.cook, normal.cook_total), (800, 1600, 80, 200));
        assert_eq!(normal.clone().take_xp(|| 0.05), 3, "0.1 rounds up with a 10% chance");
        assert_eq!(normal.clone().take_xp(|| 0.5), 2);
        // Fast furnaces used doubled seconds: 80 s of coal was 800 ticks.
        let blast = Furnace::deserialize(&format!("{raw};-;-;80.00;80.00;5.00;0.00;2")).unwrap();
        assert_eq!((blast.lit, blast.lit_total, blast.cook, blast.cook_total, blast.speed), (800, 800, 50, 100, 2.0));
        assert!(Furnace::deserialize("junk").is_none());
    }

    #[test]
    fn round_trips_through_text() {
        let mut f = furnace(Item::from_block(Block::SAND), 5, Item::COAL, 2);
        run(&mut f, 240);
        let g = Furnace::deserialize(&f.serialize()).unwrap();
        assert_eq!(g, f);
        let mut f2 = f.clone();
        let mut g2 = g.clone();
        run(&mut f2, 500);
        run(&mut g2, 500);
        assert_eq!(f2, g2, "a reload continues tick for tick");
    }

    #[test]
    fn replacing_input_requires_a_full_new_cook() {
        let mut f = furnace(Item::from_block(Block::SAND), 1, Item::COAL, 1);
        run(&mut f, 198);
        let burn = f.lit;
        let mut cursor = Some(Stack::new(Block::IRON_ORE, 1));
        f.click_input(&mut cursor, false);
        assert_eq!(cursor, Some(Stack::new(Block::SAND, 1)));
        assert_eq!(f.cook, 0);
        assert_eq!(f.lit, burn, "replacing input keeps the fire burning");
        run(&mut f, 4);
        assert!(f.output.is_none());
        run(&mut f, 196);
        assert_eq!(f.output, Some(Stack::new(Item::IRON_INGOT, 1)));
    }

    #[test]
    fn same_input_keeps_progress_but_emptying_the_slot_resets_it() {
        let mut f = furnace(Item::from_block(Block::SAND), 2, Item::COAL, 1);
        run(&mut f, 100);
        let mut cursor = Some(Stack::new(Block::SAND, 1));
        f.click_input(&mut cursor, true);
        assert_eq!(f.cook, 100, "adding the same ingredient preserves progress");
        assert_eq!(f.input.unwrap().count, 3);
        f.click_input(&mut cursor, true);
        assert_eq!(f.cook, 100, "taking half also leaves the same ingredient");
        // Put the picked-up sand back, then remove the whole stack.
        f.click_input(&mut cursor, false);
        f.click_input(&mut cursor, false);
        assert!(f.input.is_none());
        f.step();
        assert_eq!(f.cook, 0, "lit with an empty input slot");
        f.click_input(&mut cursor, false);
        run(&mut f, 4);
        assert!(f.output.is_none());
        assert_eq!(f.cook, 4);
    }
}
