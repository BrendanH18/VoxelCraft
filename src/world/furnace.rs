//! Furnaces: smelt one input at a time while fuel burns, like Minecraft.
//!
//! A furnace's contents live in [`World`] keyed by position (block
//! entities). They appear when a furnace block is placed, spill into
//! [`World::drops`] when it's removed, keep smelting whenever their chunk
//! is loaded, and swap the block between `FURNACE` and `LIT_FURNACE` (which
//! glows) as the fire starts and goes out.

use glam::IVec3;

use super::World;
use super::block::Block;
use crate::inventory::{Stack, stack_from_str, stack_to_string};
use crate::item::{Item, Tier};

/// Seconds to smelt one item.
pub const COOK_TIME: f32 = 10.0;
/// Most experience a furnace stores.
const MAX_XP: f32 = 1e6;

#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Furnace {
    pub input: Option<Stack>,
    pub fuel: Option<Stack>,
    pub output: Option<Stack>,
    /// Seconds the current piece of fuel keeps burning, and its total.
    pub burn_left: f32,
    pub burn_total: f32,
    /// Seconds spent on the current item.
    pub cook: f32,
    /// Experience stored by finished items, released when the output is
    /// taken or the furnace is broken (Java's recipes-used tally).
    pub xp: f32,
}

fn ore_of(item: Item) -> Option<Block> {
    item.block().map(Block::as_stone_ore)
}

/// What smelting `item` produces.
pub fn smelt(item: Item) -> Option<Item> {
    let b = Item::from_block;
    Some(match item {
        i if ore_of(i) == Some(Block::IRON_ORE) || i == Item::RAW_IRON => Item::IRON_INGOT,
        i if ore_of(i) == Some(Block::GOLD_ORE) || i == Item::RAW_GOLD => Item::GOLD_INGOT,
        i if ore_of(i) == Some(Block::COPPER_ORE) || i == Item::RAW_COPPER => Item::COPPER_INGOT,
        i if ore_of(i) == Some(Block::COAL_ORE) => Item::COAL,
        i if ore_of(i) == Some(Block::DIAMOND_ORE) => Item::DIAMOND,
        i if ore_of(i) == Some(Block::LAPIS_ORE) => Item::LAPIS_LAZULI,
        i if i == b(Block::STONE) => b(Block::SMOOTH_STONE),
        i if i == b(Block::COBBLED_DEEPSLATE) => b(Block::DEEPSLATE),
        i if i == b(Block::ANCIENT_DEBRIS) => Item::NETHERITE_SCRAP,
        i if i == b(Block::SAND) => b(Block::GLASS),
        i if i == b(Block::COBBLESTONE) => b(Block::STONE),
        i if i.block().is_some_and(Block::is_log) => Item::CHARCOAL,
        Item::CLAY_BALL => Item::BRICK,
        i if i == b(Block::CLAY) => b(Block::TERRACOTTA),
        i if i == b(Block::NETHERRACK) => Item::NETHER_BRICK,
        i if i == b(Block::QUARTZ_ORE) => Item::NETHER_QUARTZ,
        Item::RAW_PORKCHOP => Item::COOKED_PORKCHOP,
        Item::RAW_BEEF => Item::STEAK,
        Item::RAW_CHICKEN => Item::COOKED_CHICKEN,
        Item::POTATO => Item::BAKED_POTATO,
        _ => return None,
    })
}

/// Experience Java awards per item smelted into `out`.
pub fn smelt_xp(out: Item) -> f32 {
    let b = Item::from_block;
    match out {
        Item::GOLD_INGOT | Item::DIAMOND => 1.0,
        Item::IRON_INGOT | Item::COPPER_INGOT => 0.7,
        Item::NETHERITE_SCRAP => 2.0,
        Item::COOKED_PORKCHOP | Item::STEAK | Item::COOKED_CHICKEN | Item::BAKED_POTATO => 0.35,
        i if i == b(Block::TERRACOTTA) => 0.35,
        Item::BRICK => 0.3,
        Item::NETHER_QUARTZ | Item::LAPIS_LAZULI => 0.2,
        Item::CHARCOAL => 0.15,
        _ => 0.1,
    }
}

/// Seconds one of `item` burns as fuel (Minecraft's values / 20).
pub fn burn_time(item: Item) -> Option<f32> {
    let b = Item::from_block;
    match item {
        Item::LAVA_BUCKET => Some(1000.0),
        Item::COAL | Item::CHARCOAL => Some(80.0),
        i if i.block().is_some_and(|b| b.is_log() || b.is_planks()) => Some(15.0),
        i if [Block::CRAFTING_TABLE, Block::CHEST, Block::SMITHING_TABLE].map(b).contains(&i) => Some(15.0),
        Item::STICK => Some(5.0),
        i if i.as_tool().is_some_and(|(_, tier)| tier == Tier::Wood) => Some(10.0),
        _ => None,
    }
}

impl Furnace {
    /// Changes the input through the same slot interaction as the inventory.
    /// Adding or removing some of the same item keeps cooking progress;
    /// replacing it or emptying the slot starts a fresh cook.
    pub fn click_input(&mut self, cursor: &mut Option<Stack>, right: bool) {
        let before = self.input.map(|s| (s.item, s.damage));
        crate::inventory::click_slot(&mut self.input, cursor, right);
        if self.input.map(|s| (s.item, s.damage)) != before {
            self.cook = 0.0;
        }
    }

    pub fn is_lit(&self) -> bool {
        self.burn_left > 0.0
    }

    /// The smelted result of the current input, if it can go to the output.
    fn product(&self) -> Option<Item> {
        let out = smelt(self.input?.item)?;
        match self.output {
            None => Some(out),
            Some(o) if o.item == out && o.count < o.max() => Some(out),
            Some(_) => None,
        }
    }

    /// Advances burning and smelting by `dt` seconds.
    pub fn tick(&mut self, mut dt: f32) {
        // Step through fuel changes so a long dt can't skip lighting the next piece.
        while dt > 0.0 {
            if !self.is_lit()
                && self.product().is_some()
                && let Some(t) = self.fuel.and_then(|f| burn_time(f.item))
            {
                // A lava bucket burns down to an empty bucket.
                let bucket = self.fuel.is_some_and(|f| f.item == Item::LAVA_BUCKET);
                self.fuel = if bucket { Some(Stack::new(Item::BUCKET, 1)) } else { take_one(self.fuel) };
                self.burn_left = t;
                self.burn_total = t;
            }
            if !self.is_lit() {
                // Cold: progress slips back.
                self.cook = (self.cook - 2.0 * dt).max(0.0);
                return;
            }
            // Stop at the next finished item so big steps smelt everything.
            let product = self.product();
            let mut step = dt.min(self.burn_left);
            if product.is_some() {
                step = step.min(COOK_TIME - self.cook);
            }
            self.burn_left -= step;
            dt -= step;
            match product {
                Some(out) => {
                    self.cook += step;
                    if self.cook >= COOK_TIME {
                        self.cook -= COOK_TIME;
                        self.input = take_one(self.input);
                        self.xp = (self.xp + smelt_xp(out)).min(MAX_XP);
                        match &mut self.output {
                            Some(o) => o.count += 1,
                            None => self.output = Some(Stack::new(out, 1)),
                        }
                    }
                }
                None => self.cook = 0.0,
            }
        }
    }

    /// Releases the stored experience as whole points, rounding the
    /// fraction up with that chance (`roll` is uniform in 0..1).
    pub fn take_xp(&mut self, roll: f32) -> u32 {
        crate::simulation::experience::round_award(std::mem::take(&mut self.xp), roll)
    }

    /// Releases all stored XP if the output shrank from `before`, including
    /// on a partial transfer. No transfer leaves the tally intact; later
    /// takes earn nothing until more items have been smelted.
    pub fn take_output_xp(&mut self, before: u8, roll: f32) -> Option<u32> {
        (self.output.map_or(0, |s| s.count) < before).then(|| self.take_xp(roll))
    }

    /// Everything inside, for when the furnace is broken.
    pub fn take_all(&mut self) -> Vec<Stack> {
        [self.input.take(), self.fuel.take(), self.output.take()].into_iter().flatten().collect()
    }

    /// Saves the slots, cooking and fuel timers, and unclaimed XP tally.
    pub fn serialize(&self) -> String {
        format!(
            "{};{};{};{:.2};{:.2};{:.2};{:.2}",
            stack_to_string(self.input),
            stack_to_string(self.fuel),
            stack_to_string(self.output),
            self.burn_left,
            self.burn_total,
            self.cook,
            self.xp
        )
    }

    /// Restores saved furnace state, accepting older saves without an XP field.
    pub fn deserialize(text: &str) -> Option<Self> {
        let f: Vec<&str> = text.split(';').collect();
        // Saves from before experience have six fields.
        let (input, fuel, output, left, total, cook, xp) = match f[..] {
            [input, fuel, output, left, total, cook] => (input, fuel, output, left, total, cook, "0"),
            [input, fuel, output, left, total, cook, xp] => (input, fuel, output, left, total, cook, xp),
            _ => return None,
        };
        let num = |s: &str| s.parse::<f32>().ok().filter(|v| v.is_finite() && *v >= 0.0);
        Some(Self {
            input: stack_from_str(input)?,
            fuel: stack_from_str(fuel)?,
            output: stack_from_str(output)?,
            burn_left: num(left)?,
            burn_total: num(total)?,
            cook: num(cook)?.min(COOK_TIME),
            xp: num(xp)?.min(MAX_XP),
        })
    }
}

fn take_one(stack: Option<Stack>) -> Option<Stack> {
    stack.filter(|s| s.count > 1).map(|s| Stack { count: s.count - 1, ..s })
}

pub fn is_furnace(b: Block) -> bool {
    matches!(b.base(), Block::FURNACE | Block::LIT_FURNACE)
}

impl World {
    pub fn furnace(&self, p: IVec3) -> Option<&Furnace> {
        self.furnaces.get(&p)
    }

    pub fn furnace_mut(&mut self, p: IVec3) -> Option<&mut Furnace> {
        self.furnaces.get_mut(&p)
    }

    /// Keeps the furnace table in step with a block change at `p`.
    pub(super) fn track_furnace(&mut self, p: IVec3, old: Block, new: Block) {
        if is_furnace(old) && !is_furnace(new) {
            if let Some(mut f) = self.furnaces.remove(&p)
                && self.tile_drops
            {
                self.drops.extend(f.take_all().into_iter().map(|s| (p, s)));
                let roll = (self.roll() >> 40) as f32 / (1u64 << 24) as f32;
                let xp = f.take_xp(roll);
                if xp > 0 {
                    self.xp_drops.push((p, xp));
                }
            }
        } else if is_furnace(new) {
            self.furnaces.entry(p).or_default();
        }
    }

    /// Smelts in every furnace whose chunk is loaded and lights or puts out
    /// their blocks to match.
    pub fn tick_furnaces(&mut self, dt: f64) {
        let mut relight = Vec::new();
        for (&p, f) in self.furnaces.iter_mut() {
            if !self.chunks.contains_key(&super::chunk::chunk_of(p)) {
                continue;
            }
            f.tick(dt as f32);
            relight.push((p, f.is_lit()));
        }
        for (p, lit) in relight {
            let Some((_, facing)) = self.get_block(p).filter(|&b| is_furnace(b)).and_then(Block::oriented) else {
                continue;
            };
            let want = if lit { Block::LIT_FURNACE } else { Block::FURNACE }.with_facing(facing);
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

    fn run(f: &mut Furnace, secs: f32) {
        for _ in 0..(secs * 20.0) as usize {
            f.tick(0.05);
        }
    }

    #[test]
    fn ancient_debris_smelts_into_scrap_and_awards_two_xp_per_item() {
        let mut f = furnace(Block::ANCIENT_DEBRIS.into(), 2, Item::COAL, 1);
        run(&mut f, 20.1);
        assert_eq!(f.input, None);
        assert_eq!(f.output, Some(Stack::new(Item::NETHERITE_SCRAP, 2)));
        assert_eq!(f.xp, 4.0);
        assert_eq!(f.take_output_xp(2, 0.5), None, "no XP before taking the output");
        f.output = None;
        assert_eq!(f.take_output_xp(2, 0.5), Some(4));
        assert_eq!(f.xp, 0.0);
    }

    #[test]
    fn lava_buckets_burn_long_and_leave_the_bucket() {
        let mut f = furnace(Item::from_block(Block::COBBLESTONE), 64, Item::LAVA_BUCKET, 1);
        f.tick(1.0);
        assert!(f.is_lit());
        assert_eq!(f.fuel, Some(Stack::new(Item::BUCKET, 1)));
        for _ in 0..60 {
            f.tick(10.0);
        }
        assert_eq!(f.output.map(|s| s.count), Some(60), "still burning after 600 s");
    }

    #[test]
    fn smelts_one_item_per_cook_time_while_fuel_lasts() {
        let ore = Item::from_block(Block::IRON_ORE);
        // A stick burns 5 s: half an ingot's worth, then the fire is out.
        let mut f = furnace(ore, 3, Item::STICK, 1);
        run(&mut f, 1.0);
        assert!(f.is_lit() && f.fuel.is_none());
        run(&mut f, 6.0);
        assert!(!f.is_lit() && f.output.is_none() && f.cook < 5.0);

        // Coal burns 80 s: all three ores, then idles on until it burns out.
        let mut f = furnace(ore, 3, Item::COAL, 1);
        run(&mut f, 10.1);
        assert_eq!(f.output, Some(Stack::new(Item::IRON_INGOT, 1)));
        run(&mut f, 20.0);
        assert_eq!(f.output, Some(Stack::new(Item::IRON_INGOT, 3)));
        assert!(f.input.is_none() && f.is_lit());
        // A big time step behaves the same as small ones.
        let mut g = furnace(ore, 3, Item::COAL, 1);
        g.tick(30.1);
        assert_eq!(g.output, f.output);
    }

    #[test]
    fn needs_a_smeltable_input_and_room_for_the_output() {
        // Nothing to smelt: the fuel isn't wasted.
        let mut f = furnace(Item::STICK, 1, Item::COAL, 1);
        run(&mut f, 1.0);
        assert!(!f.is_lit() && f.fuel.is_some());
        // Output full of something else: no burning either.
        let mut f = furnace(Item::RAW_BEEF, 1, Item::COAL, 1);
        f.output = Some(Stack::new(Item::COAL, 1));
        run(&mut f, 1.0);
        assert!(!f.is_lit());
        assert_eq!(smelt(Item::RAW_BEEF), Some(Item::STEAK));
        assert!(burn_time(Item::IRON_INGOT).is_none());
        assert_eq!(burn_time(Item::from(Block::SMITHING_TABLE)), Some(15.0));
    }

    #[test]
    fn output_transfers_release_stored_xp_once() {
        use crate::inventory::move_into;

        let mut f = furnace(Item::from_block(Block::GOLD_ORE), 8, Item::COAL, 2);
        run(&mut f, COOK_TIME * 8.0 + 1.0);
        assert_eq!(f.xp, 8.0);
        let mut inventory = [Some(Stack::new(Item::GOLD_INGOT, 64))];
        let stack = f.output.take().unwrap();
        f.output = move_into(stack, &mut inventory, &[0]);
        assert_eq!(f.take_output_xp(stack.count, 0.5), None, "a full inventory does not release XP");
        assert_eq!(f.xp, 8.0);

        inventory[0].as_mut().unwrap().count = 63;
        let stack = f.output.take().unwrap();
        f.output = move_into(stack, &mut inventory, &[0]);
        assert_eq!(f.output.unwrap().count, 7, "only one item fits");
        assert_eq!(f.take_output_xp(stack.count, 0.5), Some(8), "a partial take releases the entire tally");
        assert_eq!(f.xp, 0.0);

        inventory[0] = None;
        let stack = f.output.take().unwrap();
        f.output = move_into(stack, &mut inventory, &[0]);
        assert_eq!(f.output, None);
        assert_eq!(f.take_output_xp(stack.count, 0.5), Some(0), "taking the leftovers cannot award XP twice");
        f.input = Some(Stack::new(Block::GOLD_ORE, 1));
        f.fuel = Some(Stack::new(Item::COAL, 1));
        run(&mut f, COOK_TIME + 1.0);
        f.output.take();
        assert_eq!(f.take_output_xp(1, 0.5), Some(1), "new smelts earn fresh XP");
    }

    #[test]
    fn smelting_stores_experience_until_taken() {
        let mut f = furnace(Item::from_block(Block::IRON_ORE), 3, Item::COAL, 1);
        run(&mut f, COOK_TIME * 3.0 + 1.0);
        assert!((f.xp - 2.1).abs() < 1e-4, "{}", f.xp);
        assert_eq!(f.take_xp(0.05), 3, "0.1 rounds up with 10% chance");
        assert_eq!(f.xp, 0.0);
        f.xp = 2.1;
        assert_eq!(f.take_xp(0.5), 2);
        assert_eq!(smelt_xp(Item::from_block(Block::GLASS)), 0.1);
        f.xp = 1.25;
        let g = Furnace::deserialize(&f.serialize()).unwrap();
        assert_eq!(g.xp, 1.25);
        let old = "-;-;-;0.00;0.00;0.00";
        assert_eq!(Furnace::deserialize(old).map(|f| f.xp), Some(0.0), "older saves load");
    }

    #[test]
    fn round_trips_through_text() {
        let mut f = furnace(Item::from_block(Block::SAND), 5, Item::COAL, 2);
        run(&mut f, 12.0);
        let g = Furnace::deserialize(&f.serialize()).unwrap();
        assert_eq!(g.output, f.output);
        assert_eq!((g.input, g.fuel), (f.input, f.fuel));
        assert!((g.burn_left - f.burn_left).abs() < 0.01 && (g.cook - f.cook).abs() < 0.01);
        assert!(Furnace::deserialize("junk").is_none());
    }

    #[test]
    fn replacing_input_requires_a_full_new_cook() {
        let mut f = furnace(Item::from_block(Block::SAND), 1, Item::COAL, 1);
        f.tick(9.9);
        let burn = f.burn_left;
        let mut cursor = Some(Stack::new(Block::IRON_ORE, 1));
        f.click_input(&mut cursor, false);
        assert_eq!(cursor, Some(Stack::new(Block::SAND, 1)));
        assert_eq!(f.cook, 0.0);
        assert_eq!(f.burn_left, burn, "replacing input keeps the fire burning");
        f.tick(0.2);
        assert!(f.output.is_none());
        f.tick(9.9);
        assert_eq!(f.output, Some(Stack::new(Item::IRON_INGOT, 1)));
    }

    #[test]
    fn same_input_keeps_progress_but_emptying_the_slot_resets_it() {
        let mut f = furnace(Item::from_block(Block::SAND), 2, Item::COAL, 1);
        f.tick(5.0);
        let mut cursor = Some(Stack::new(Block::SAND, 1));
        f.click_input(&mut cursor, true);
        assert_eq!(f.cook, 5.0, "adding the same ingredient preserves progress");
        assert_eq!(f.input.unwrap().count, 3);
        f.click_input(&mut cursor, true);
        assert_eq!(f.cook, 5.0, "taking half also leaves the same ingredient");
        // Put the picked-up sand back, then remove the whole stack.
        f.click_input(&mut cursor, false);
        f.click_input(&mut cursor, false);
        assert!(f.input.is_none());
        assert_eq!(f.cook, 0.0);
        f.click_input(&mut cursor, false);
        f.tick(0.2);
        assert!(f.output.is_none());
        assert!(f.cook < 1.0);
    }
}
