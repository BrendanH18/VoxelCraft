//! Brewing stands: three bottles, an ingredient and blaze powder fuel, like
//! Java's. A blaze powder fuels 20 brews; a brew takes 20 seconds, then
//! turns every bottle the ingredient works on and uses up one ingredient.
//! Contents live in [`World`] by position (block entities), like furnaces:
//! they keep brewing while their chunk is loaded, spill when the stand is
//! broken and are saved with the level.

use glam::IVec3;

use super::World;
use super::block::Block;
use crate::inventory::{Stack, stack_from_str, stack_to_string};
use crate::item::Item;
use crate::potion::Potion;

/// Seconds per brew (Java's 400 ticks).
pub const BREW_TIME: f32 = 20.0;
/// Brews one blaze powder fuels.
pub const FUEL_USES: u8 = 20;

#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct BrewingStand {
    pub bottles: [Option<Stack>; 3],
    pub ingredient: Option<Stack>,
    pub fuel: Option<Stack>,
    /// Brews left from the last blaze powder.
    pub fuel_left: u8,
    /// Seconds left on the brew in progress (0 when idle).
    pub brew_left: f32,
    /// The ingredient the brew in progress started with; swapping it
    /// stops the brew.
    brewing: Option<Item>,
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
        ("awkward", Item::SUGAR) => to("swiftness"),
        ("awkward", Item::GLISTERING_MELON_SLICE) => to("healing"),
        ("awkward", Item::SPIDER_EYE) => to("poison"),
        ("awkward", Item::BLAZE_POWDER) => to("strength"),
        ("awkward", Item::MAGMA_CREAM) => to("fire_resistance"),
        ("awkward", Item::GHAST_TEAR) => to("regeneration"),
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
    item == Item::GUNPOWDER || Potion::all().any(|p| brew(p, item).is_some())
}

/// What `ingredient` turns the bottle `item` into: gunpowder makes a potion
/// splash, and the other mixes keep a splash potion splash.
pub fn brew_item(item: Item, ingredient: Item) -> Option<Item> {
    if ingredient == Item::GUNPOWDER {
        return item.as_potion().map(Item::splash_potion);
    }
    if let Some(p) = item.as_splash_potion() {
        return brew(p, ingredient).map(Item::splash_potion);
    }
    item.as_potion().and_then(|p| brew(p, ingredient)).map(Item::potion)
}

/// The bottle slots hold potions (and water bottles) and glass bottles.
pub fn fits_bottle_slot(item: Item) -> bool {
    item.as_potion().is_some() || item.as_splash_potion().is_some() || item == Item::GLASS_BOTTLE
}

impl BrewingStand {
    /// Whether the ingredient would change at least one bottle.
    fn can_brew(&self) -> bool {
        let Some(ingredient) = self.ingredient else { return false };
        self.bottles.iter().flatten().any(|b| brew_item(b.item, ingredient.item).is_some())
    }

    pub fn is_brewing(&self) -> bool {
        self.brew_left > 0.0
    }

    /// Advances by `dt` seconds (Java's `serverTick`). Returns whether a
    /// brew finished.
    pub fn tick(&mut self, dt: f32) -> bool {
        if self.fuel_left == 0 && self.fuel.is_some_and(|f| f.item == Item::BLAZE_POWDER) {
            self.fuel_left = FUEL_USES;
            self.fuel = take_one(self.fuel);
        }
        let can = self.can_brew();
        let ingredient = self.ingredient.map(|s| s.item);
        if self.is_brewing() {
            if !can || ingredient != self.brewing {
                self.brew_left = 0.0;
                return false;
            }
            self.brew_left -= dt;
            // The epsilon absorbs float drift so 400 ticks finish a brew.
            if self.brew_left <= 1e-3 {
                self.brew_left = 0.0;
                self.finish();
                return true;
            }
        } else if can && self.fuel_left > 0 {
            self.fuel_left -= 1;
            self.brew_left = BREW_TIME;
            self.brewing = ingredient;
        }
        false
    }

    fn finish(&mut self) {
        let Some(ingredient) = self.ingredient else { return };
        for bottle in self.bottles.iter_mut().flatten() {
            if let Some(out) = brew_item(bottle.item, ingredient.item) {
                *bottle = Stack::new(out, 1);
            }
        }
        self.ingredient = take_one(self.ingredient);
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
            self.brew_left,
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
            brew_left: brew_left.parse::<f32>().ok().filter(|v| v.is_finite())?.clamp(0.0, BREW_TIME),
            brewing: (brewing != 0).then_some(Item(brewing)).filter(|i| i.is_valid()),
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
        self.brewing_stands.get_mut(&p)
    }

    /// Keeps the brewing stand table in step with a block change at `p`.
    pub(super) fn track_brewing_stand(&mut self, p: IVec3, old: Block, new: Block) {
        if old == Block::BREWING_STAND && new != Block::BREWING_STAND {
            if let Some(mut stand) = self.brewing_stands.remove(&p)
                && self.tile_drops
            {
                self.drops.extend(stand.take_all().into_iter().map(|s| (p, s)));
            }
        } else if new == Block::BREWING_STAND {
            self.brewing_stands.entry(p).or_default();
        }
    }

    /// Brews in every stand whose chunk is loaded, noting finished brews
    /// in [`World::brews_done`] for the sound.
    pub fn tick_brewing(&mut self, dt: f64) {
        for (&p, stand) in self.brewing_stands.iter_mut() {
            if self.chunks.contains_key(&super::chunk::chunk_of(p)) && stand.tick(dt as f32) {
                self.brews_done.push(p);
            }
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
        (0..(secs * 20.0).round() as usize).filter(|_| stand.tick(0.05)).count()
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
}
