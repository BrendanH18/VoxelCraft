//! Brewing stands: three bottles, an ingredient and blaze powder fuel,
//! following Java 26.3's `BrewingStandBlockEntity.serverTick` tick for
//! tick. A blaze powder (`brewing/uses_default`: 20) fuels 20 brews; a brew
//! takes 400 ticks (`brewing/speed_default` is 1), then turns every bottle
//! the ingredient works on and uses up one ingredient, leaving its crafting
//! remainder. Contents live in [`World`] by position (block entities), like
//! furnaces: they keep brewing while their chunk is loaded, spill when the
//! stand is broken and are saved with the level.

use glam::IVec3;

use super::World;
use super::block::Block;
use crate::inventory::{Stack, stack_from_str, stack_to_string};
use crate::item::Item;
use crate::potion::Potion;

/// Ticks per brew: `ceil(400 / speed)` with blaze powder's speed of 1.
pub const BREW_TICKS: u16 = 400;
/// Seconds per brew.
pub const BREW_TIME: f32 = BREW_TICKS as f32 / 20.0;
/// Brews one blaze powder fuels (`brewing/uses_default`).
pub const FUEL_USES: u8 = 20;

#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct BrewingStand {
    pub bottles: [Option<Stack>; 3],
    pub ingredient: Option<Stack>,
    pub fuel: Option<Stack>,
    /// Brews left from the last blaze powder.
    pub fuel_left: u8,
    /// `brewTime`: ticks left on the brew in progress (0 when idle).
    pub brew_ticks: u16,
    /// The ingredient the brew in progress started with; swapping it
    /// stops the brew.
    brewing: Option<Item>,
    /// Fraction of a tick carried between [`BrewingStand::tick`] calls.
    carry: f32,
}

/// What `ingredient` turns `potion` into: Java's brewing mixes for the
/// ingredients that exist so far.
pub fn brew(potion: Potion, ingredient: Item) -> Option<Potion> {
    let to = |id: &str| Potion::from_id(id);
    let glowstone = Item::GLOWSTONE_DUST;
    match (potion.info().id, ingredient) {
        ("water", Item::NETHER_WART) => to("awkward"),
        ("water", i) if i == glowstone => to("thick"),
        ("water", Item::SUGAR | Item::GLISTERING_MELON_SLICE | Item::SPIDER_EYE | Item::BLAZE_POWDER) => to("mundane"),
        ("awkward", Item::GOLDEN_CARROT) => to("night_vision"),
        ("awkward", Item::RABBIT_FOOT) => to("leaping"),
        ("water", Item::FERMENTED_SPIDER_EYE) => to("weakness"),
        ("night_vision", Item::FERMENTED_SPIDER_EYE) => to("invisibility"),
        ("long_night_vision", Item::FERMENTED_SPIDER_EYE) => to("long_invisibility"),
        ("leaping" | "swiftness", Item::FERMENTED_SPIDER_EYE) => to("slowness"),
        ("long_leaping" | "long_swiftness", Item::FERMENTED_SPIDER_EYE) => to("long_slowness"),
        ("healing" | "poison" | "long_poison", Item::FERMENTED_SPIDER_EYE) => to("harming"),
        ("strong_healing" | "strong_poison", Item::FERMENTED_SPIDER_EYE) => to("strong_harming"),
        ("awkward", Item::SUGAR) => to("swiftness"),
        ("awkward", Item::GLISTERING_MELON_SLICE) => to("healing"),
        ("awkward", Item::SPIDER_EYE) => to("poison"),
        ("awkward", Item::BLAZE_POWDER) => to("strength"),
        ("awkward", Item::MAGMA_CREAM) => to("fire_resistance"),
        ("awkward", Item::GHAST_TEAR) => to("regeneration"),
        ("awkward", Item::PUFFERFISH) => to("water_breathing"),
        ("awkward", Item::PHANTOM_MEMBRANE) => to("slow_falling"),
        ("water", Item::RABBIT_FOOT) => to("mundane"),
        ("water", i)
            if [
                crate::world::trial::BREEZE_ROD,
                Item::from_block(crate::world::stone_blocks::SLIME_BLOCK),
                Item::from_block(Block::STONE),
                Item::from_block(Block::COBWEB),
            ]
            .contains(&i) =>
        {
            to("mundane")
        }
        ("awkward", Item::TURTLE_HELMET) => to("turtle_master"),
        // v0.15 Track D: `VanillaBrewingProvider`'s 1.21 start mixes.
        ("awkward", i) if i == crate::world::trial::BREEZE_ROD => to("wind_charged"),
        ("awkward", i) if i == Item::from_block(crate::world::stone_blocks::SLIME_BLOCK) => to("oozing"),
        ("awkward", i) if i == Item::from_block(Block::STONE) => to("infested"),
        ("awkward", i) if i == Item::from_block(Block::COBWEB) => to("weaving"),
        ("water", Item::MAGMA_CREAM | Item::REDSTONE | Item::GHAST_TEAR) => to("mundane"),
        (id, Item::REDSTONE) if to(&format!("long_{id}")).is_some() => to(&format!("long_{id}")),
        // Glowstone strengthens to level II.
        (id, i) if i == glowstone && to(&format!("strong_{id}")).is_some() => to(&format!("strong_{id}")),
        _ => None,
    }
}

/// Whether `item` is used by any brewing mix (the ingredient slot takes
/// only these).
pub fn is_ingredient(item: Item) -> bool {
    item == Item::DRAGON_BREATH || item == Item::GUNPOWDER || Potion::all().any(|p| brew(p, item).is_some())
}

/// What `ingredient` turns the bottle `item` into: gunpowder makes a potion
/// splash, and the other mixes keep a splash potion splash.
pub fn brew_item(item: Item, ingredient: Item) -> Option<Item> {
    if ingredient == Item::DRAGON_BREATH {
        return item.as_splash_potion().map(|_| Item::LINGERING_POTION);
    }
    if ingredient == Item::GUNPOWDER {
        return item.as_potion().map(Item::splash_potion);
    }
    if let Some(p) = item.as_splash_potion() {
        return brew(p, ingredient).map(Item::splash_potion);
    }
    item.as_potion().and_then(|p| brew(p, ingredient)).map(Item::potion)
}

/// Brewing preserves the potion component when the container has one.
pub fn brew_stack(stack: Stack, ingredient: Item) -> Option<Stack> {
    let output = if ingredient == Item::DRAGON_BREATH {
        stack.item.as_splash_potion().map(Stack::lingering_potion)
    } else if let Some(p) = stack.lingering_contents() {
        brew(p, ingredient).map(Stack::lingering_potion)
    } else {
        brew_item(stack.item, ingredient).map(|i| Stack::new(i, 1))
    };
    output.map(|out| Stack { item: out.item, entity_data: out.entity_data, count: 1, ..stack })
}

/// The bottle slots hold potions (and water bottles) and glass bottles:
/// Java's `BREWING_INPUTS` recipe property set plus
/// `#brewing_potion_inputs` (`PotionIngredient.isPotionInput`).
pub fn fits_bottle_slot(item: Item) -> bool {
    if item == Item::LINGERING_POTION {
        return true;
    }
    item.as_potion().is_some() || item.as_splash_potion().is_some() || item == Item::GLASS_BOTTLE
}

/// Whether `item` fuels brewing (has Java's `brewing_fuel` component).
pub fn is_fuel(item: Item) -> bool {
    item == Item::BLAZE_POWDER
}

impl BrewingStand {
    /// Whether the ingredient would change at least one bottle.
    fn can_brew(&self) -> bool {
        let Some(ingredient) = self.ingredient else { return false };
        self.bottles.iter().flatten().any(|b| brew_stack(*b, ingredient.item).is_some())
    }

    pub fn is_brewing(&self) -> bool {
        self.brew_ticks > 0
    }

    /// How far the brew in progress is, 0 to 1.
    pub fn brew_progress(&self) -> f32 {
        if self.is_brewing() { 1.0 - self.brew_ticks as f32 / BREW_TICKS as f32 } else { 0.0 }
    }

    /// Java's `canPlaceItem`, which hoppers and droppers obey: the fuel slot
    /// (4) takes brewing fuel, the ingredient slot (3) any brewing reagent,
    /// and a bottle slot (0..3) a potion input only while it's empty.
    pub fn can_place(&self, slot: usize, item: Item) -> bool {
        match slot {
            0..=2 => fits_bottle_slot(item) && self.bottles[slot].is_none(),
            3 => is_ingredient(item),
            4 => is_fuel(item),
            _ => false,
        }
    }

    /// One `serverTick`. Returns whether a brew finished.
    ///
    /// A spent stand takes a blaze powder first. A brew starts on a tick
    /// with a brewable ingredient and fuel, then counts down 400 ticks; it
    /// stops early if the ingredient changes or nothing can be brewed any
    /// more, except that on its last tick it brews whatever ingredient is
    /// in the slot then, as long as it brews something.
    pub fn step(&mut self) -> bool {
        if self.fuel_left == 0 && self.fuel.is_some_and(|f| is_fuel(f.item)) {
            self.fuel_left = FUEL_USES;
            self.fuel = take_one(self.fuel);
        }
        let brewable = self.can_brew();
        let ingredient = self.ingredient.map(|s| s.item);
        if self.is_brewing() {
            self.brew_ticks -= 1;
            if self.brew_ticks == 0 && brewable {
                self.finish();
                return true;
            } else if !brewable || ingredient != self.brewing {
                self.brew_ticks = 0;
            }
        } else if brewable && self.fuel_left > 0 {
            self.fuel_left -= 1;
            self.brew_ticks = BREW_TICKS;
            self.brewing = ingredient;
        }
        false
    }

    /// Runs the whole game ticks in `dt` seconds (20 per second), carrying
    /// any fraction to the next call. Returns how many brews finished.
    pub fn tick(&mut self, dt: f32) -> usize {
        let ticks = self.carry + dt * 20.0;
        let whole = (ticks + 1e-3).floor().max(0.0);
        self.carry = ticks - whole;
        (0..whole as u32).filter(|_| self.step()).count()
    }

    fn finish(&mut self) {
        let Some(ingredient) = self.ingredient else { return };
        for bottle in self.bottles.iter_mut().flatten() {
            if let Some(out) = brew_stack(*bottle, ingredient.item) {
                *bottle = out;
            }
        }
        self.ingredient = if ingredient.count == 1 {
            ingredient.item.remainder().map(|i| Stack::new(i, 1))
        } else {
            take_one(self.ingredient)
        };
    }

    /// Everything inside, for when the stand is broken.
    pub fn take_all(&mut self) -> Vec<Stack> {
        let mut out: Vec<Stack> = self.bottles.iter_mut().filter_map(Option::take).collect();
        out.extend(self.ingredient.take());
        out.extend(self.fuel.take());
        out
    }

    pub fn serialize(&self) -> String {
        format!(
            "{};{};{};{};{};{};{:.2};{}",
            stack_to_string(self.bottles[0]),
            stack_to_string(self.bottles[1]),
            stack_to_string(self.bottles[2]),
            stack_to_string(self.ingredient),
            stack_to_string(self.fuel),
            self.fuel_left,
            // Seconds, as saves have always stored it (ticks / 20 is exact
            // to two places).
            self.brew_ticks as f32 / 20.0,
            self.brewing.map_or(0, |i| i.0),
        )
    }

    pub fn deserialize(text: &str) -> Option<Self> {
        let f: Vec<&str> = text.split(';').collect();
        let &[b0, b1, b2, ingredient, fuel, fuel_left, brew_left, brewing] = &f[..] else { return None };
        let brewing: u16 = brewing.parse().ok()?;
        Some(Self {
            bottles: [stack_from_str(b0)?, stack_from_str(b1)?, stack_from_str(b2)?],
            ingredient: stack_from_str(ingredient)?,
            fuel: stack_from_str(fuel)?,
            fuel_left: fuel_left.parse::<u8>().ok()?.min(FUEL_USES),
            brew_ticks: (brew_left.parse::<f32>().ok().filter(|v| v.is_finite())?.clamp(0.0, BREW_TIME) * 20.0).round()
                as u16,
            brewing: (brewing != 0).then_some(Item(brewing)).filter(|i| i.is_valid()),
            carry: 0.0,
        })
    }
}

fn take_one(stack: Option<Stack>) -> Option<Stack> {
    stack.filter(|s| s.count > 1).map(|s| Stack { count: s.count - 1, ..s })
}

impl World {
    pub fn brewing_stand(&self, p: IVec3) -> Option<&BrewingStand> {
        self.brewing_stands.get(&p)
    }

    pub fn brewing_stand_mut(&mut self, p: IVec3) -> Option<&mut BrewingStand> {
        self.redstone_changed(p);
        self.brewing_stands.get_mut(&p)
    }

    /// Keeps the brewing stand table in step with a block change at `p`.
    pub(super) fn track_brewing_stand(&mut self, p: IVec3, old: Block, new: Block) {
        if old == Block::BREWING_STAND && new != Block::BREWING_STAND {
            // Contents don't depend on `block_drops` (`Containers.dropContents`).
            if let Some(mut stand) = self.brewing_stands.remove(&p) {
                self.drops.extend(stand.take_all().into_iter().map(|s| (p, s)));
            }
        } else if new == Block::BREWING_STAND {
            self.brewing_stands.entry(p).or_default();
        }
    }

    /// Brews in every stand whose chunk is block-ticking, noting finished brews
    /// in [`World::brews_done`] for the sound.
    pub fn tick_brewing(&mut self, dt: f64) {
        let mut changed = Vec::new();
        for (&p, stand) in self.brewing_stands.iter_mut() {
            if !self.activity.contains(p, true) || !self.chunks.contains_key(&super::chunk::chunk_of(p)) {
                continue;
            }
            let before = (stand.bottles, stand.ingredient, stand.fuel);
            for _ in 0..stand.tick(dt as f32) {
                self.brews_done.push(p);
                // `doBrew`: an ingredient's remainder (dragon's breath leaves
                // a glass bottle) is dropped while some ingredient is left.
                if let Some(rest) = stand.ingredient.and_then(|s| s.item.remainder()) {
                    self.drops.push((p, Stack::new(rest, 1)));
                }
            }
            // `BrewingStandBlockEntity.serverTick` calls `setChanged` when
            // it loads blaze powder as well as when a brew finishes.
            if (stand.bottles, stand.ingredient, stand.fuel) != before {
                changed.push(p);
            }
        }
        for p in changed {
            self.redstone_changed(p);
        }
    }

    /// `x,y,z=stand|...` for the level file.
    pub fn brewing_stands_to_string(&self) -> String {
        self.brewing_stands
            .iter()
            .map(|(p, b)| format!("{},{},{}={}", p.x, p.y, p.z, b.serialize()))
            .collect::<Vec<_>>()
            .join("|")
    }

    /// Restores stands saved by [`World::brewing_stands_to_string`];
    /// malformed entries are skipped.
    pub fn load_brewing_stands(&mut self, text: &str) {
        for entry in text.split('|').filter(|e| !e.is_empty()) {
            let Some((pos, state)) = entry.split_once('=') else { continue };
            let c: Vec<i32> = pos.split(',').filter_map(|v| v.parse().ok()).collect();
            if let (&[x, y, z], Some(b)) = (&c[..], BrewingStand::deserialize(state)) {
                self.brewing_stands.insert(IVec3::new(x, y, z), b);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn gunpowder_makes_splash_potions_that_keep_brewing() {
        use super::*;
        let poison = Potion::from_id("poison").unwrap();
        let splash = brew_item(Item::potion(poison), Item::GUNPOWDER).unwrap();
        assert_eq!(splash.as_splash_potion(), Some(poison));
        assert_eq!(splash.name(), "splash potion of poison");
        assert_eq!(
            brew_item(splash, Item::REDSTONE),
            Some(Item::splash_potion(Potion::from_id("long_poison").unwrap()))
        );
        assert_eq!(brew_item(splash, Item::GUNPOWDER), None, "already splash");
        assert_eq!(brew_item(Item::potion(Potion::WATER), Item::GUNPOWDER), Some(Item::splash_potion(Potion::WATER)));
        assert!(is_ingredient(Item::GUNPOWDER) && fits_bottle_slot(splash));
    }

    use super::*;

    fn potion(id: &str) -> Option<Stack> {
        Some(Stack::new(Item::potion(Potion::from_id(id).unwrap()), 1))
    }

    fn run(stand: &mut BrewingStand, secs: f32) -> usize {
        (0..(secs * 20.0).round() as usize).filter(|_| stand.step()).count()
    }

    #[test]
    fn magma_cream_and_redstone_unlock_fire_resistance() {
        let fire = Potion::from_id("fire_resistance").unwrap();
        assert_eq!(brew(Potion::AWKWARD, Item::MAGMA_CREAM), Some(fire));
        assert_eq!(brew(fire, Item::REDSTONE), Potion::from_id("long_fire_resistance"));
        assert_eq!(brew(fire, Item::GLOWSTONE_DUST), None);
        assert_eq!(brew(Potion::WATER, Item::MAGMA_CREAM), Some(Potion::MUNDANE));
        assert!(is_ingredient(Item::REDSTONE));
    }

    #[test]
    fn ghast_tear_brews_regeneration() {
        let regen = Potion::from_id("regeneration").unwrap();
        assert_eq!(brew(Potion::AWKWARD, Item::GHAST_TEAR), Some(regen));
        assert_eq!(brew(regen, Item::REDSTONE), Potion::from_id("long_regeneration"));
        assert_eq!(brew(regen, Item::GLOWSTONE_DUST), Potion::from_id("strong_regeneration"));
        assert_eq!(brew(Potion::WATER, Item::GHAST_TEAR), Some(Potion::MUNDANE));
        assert!(is_ingredient(Item::GHAST_TEAR));
    }

    #[test]
    fn brews_awkward_then_strength_on_one_blaze_powder() {
        let mut s = BrewingStand {
            bottles: [potion("water"), potion("water"), None],
            ingredient: Some(Stack::new(Item::NETHER_WART, 2)),
            fuel: Some(Stack::new(Item::BLAZE_POWDER, 1)),
            ..Default::default()
        };
        assert_eq!(run(&mut s, 19.9), 0, "a brew takes 20 s");
        assert_eq!(s.fuel_left, FUEL_USES - 1);
        assert!(s.fuel.is_none());
        assert_eq!(run(&mut s, 0.2), 1);
        assert_eq!(s.bottles, [potion("awkward"), potion("awkward"), None]);
        assert_eq!(s.ingredient.map(|i| i.count), Some(1));
        // Nether wart does nothing more to awkward potions.
        assert_eq!(run(&mut s, 30.0), 0);
        assert!(!s.is_brewing());
        s.ingredient = Some(Stack::new(Item::BLAZE_POWDER, 1));
        run(&mut s, 20.1);
        assert_eq!(s.bottles[0], potion("strength"));
        s.ingredient = Some(Stack::new(Item::GLOWSTONE_DUST, 1));
        run(&mut s, 20.1);
        assert_eq!(s.bottles[1], potion("strong_strength"));
        assert_eq!(s.fuel_left, FUEL_USES - 3);
    }

    #[test]
    fn swapping_the_ingredient_stops_and_no_fuel_never_starts() {
        let mut s = BrewingStand {
            bottles: [potion("awkward"), None, None],
            ingredient: Some(Stack::new(Item::SUGAR, 1)),
            ..Default::default()
        };
        run(&mut s, 30.0);
        assert_eq!(s.bottles[0], potion("awkward"), "no fuel");
        s.fuel = Some(Stack::new(Item::BLAZE_POWDER, 1));
        run(&mut s, 10.0);
        assert!(s.is_brewing());
        s.ingredient = Some(Stack::new(Item::SPIDER_EYE, 1));
        s.tick(0.05);
        assert!(!s.is_brewing(), "a new ingredient restarts the brew");
        run(&mut s, 20.1);
        assert_eq!(s.bottles[0], potion("poison"));
        let back = BrewingStand::deserialize(&s.serialize()).unwrap();
        assert_eq!(back, s);
        assert!(is_ingredient(Item::NETHER_WART) && !is_ingredient(Item::STICK));
        assert!(fits_bottle_slot(Item::GLASS_BOTTLE) && !fits_bottle_slot(Item::NETHER_WART));
    }

    #[test]
    fn the_last_tick_brews_a_swapped_ingredient_that_still_brews() {
        let mut s = BrewingStand {
            bottles: [potion("awkward"), None, None],
            ingredient: Some(Stack::new(Item::SUGAR, 1)),
            fuel: Some(Stack::new(Item::BLAZE_POWDER, 1)),
            ..Default::default()
        };
        assert_eq!(run(&mut s, 0.05), 0);
        assert_eq!(s.brew_ticks, BREW_TICKS, "the start tick doesn't count down");
        for _ in 0..399 {
            assert!(!s.step());
        }
        assert_eq!(s.brew_ticks, 1);
        // Swapped on the final tick: Java brews what's there now.
        s.ingredient = Some(Stack::new(Item::SPIDER_EYE, 1));
        assert!(s.step());
        assert_eq!(s.bottles[0], potion("poison"));
        assert!(s.ingredient.is_none());
    }

    #[test]
    fn bottle_slots_take_one_potion_input_each_when_empty() {
        let mut s = BrewingStand::default();
        assert!(s.can_place(0, Item::GLASS_BOTTLE) && s.can_place(2, Item::LINGERING_POTION));
        s.bottles[0] = Some(Stack::new(Item::GLASS_BOTTLE, 1));
        assert!(!s.can_place(0, Item::GLASS_BOTTLE), "only into an empty bottle slot");
        assert!(s.can_place(3, Item::NETHER_WART) && !s.can_place(3, Item::STICK));
        assert!(s.can_place(4, Item::BLAZE_POWDER) && !s.can_place(4, Item::COAL));
    }
}
