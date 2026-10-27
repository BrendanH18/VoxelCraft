//! Campfires and soul campfires (`CampfireBlock`, `CampfireBlockEntity`):
//! four cooking slots, lighting and dousing, damage and smoke signals over
//! hay bales.

use glam::{DVec3, IVec3};

use super::{Campfire, campfire, campfire_of};
use crate::inventory::{Stack, stack_from_str, stack_to_string};
use crate::item::Item;
use crate::world::{World, block::Block, overworld_blocks as ob};

/// Every `*_from_campfire_cooking` recipe takes 600 ticks (26.3 data).
pub const COOK_TICKS: u16 = 600;

/// Java's campfire cooking recipes: what each raw item becomes.
pub fn cooked(item: Item) -> Option<Item> {
    Some(match item {
        Item::RAW_BEEF => Item::STEAK,
        Item::RAW_PORKCHOP => Item::COOKED_PORKCHOP,
        Item::RAW_CHICKEN => Item::COOKED_CHICKEN,
        Item::RAW_MUTTON => Item::COOKED_MUTTON,
        Item::RAW_RABBIT => Item::COOKED_RABBIT,
        Item::COD => Item::COOKED_COD,
        Item::SALMON => Item::COOKED_SALMON,
        Item::POTATO => Item::BAKED_POTATO,
        i if i == Item::from_block(ob::KELP) => Item::DRIED_KELP,
        _ => return None,
    })
}

/// A campfire's four slots and their cooking progress in ticks.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub struct Slots {
    pub items: [Option<Stack>; 4],
    pub progress: [u16; 4],
}

impl Slots {
    fn serialize(&self) -> String {
        (0..4).map(|i| format!("{}/{}", stack_to_string(self.items[i]), self.progress[i])).collect::<Vec<_>>().join(";")
    }

    fn deserialize(text: &str) -> Option<Self> {
        let mut slots = Slots::default();
        let parts: Vec<&str> = text.split(';').collect();
        if parts.len() != 4 {
            return None;
        }
        for (i, part) in parts.into_iter().enumerate() {
            let (stack, progress) = part.split_once('/')?;
            slots.items[i] = stack_from_str(stack)?;
            slots.progress[i] = progress.parse::<u16>().ok()?.min(COOK_TICKS);
        }
        Some(slots)
    }

    pub fn is_empty(&self) -> bool {
        self.items.iter().all(Option::is_none)
    }
}

/// `CampfireBlock.isSmokeSource`: a hay bale (any axis) under the fire.
pub fn smoke_source(below: Block) -> bool {
    below.base() == Block::HAY_BALE
}

/// `CampfireBlock.canLight`: a dry campfire that isn't burning.
pub fn can_light(b: Block) -> bool {
    campfire_of(b).is_some_and(|c| !c.lit && !c.wet)
}

/// Fire damage a lit campfire deals to whoever stands in it (Java's
/// `fireDamage`: 1 for a campfire, 2 for a soul campfire).
pub fn damage(b: Block) -> Option<f32> {
    campfire_of(b).filter(|c| c.lit).map(|c| if c.soul { 2.0 } else { 1.0 })
}

/// The fire damage of the hottest lit campfire whose cell `player`'s body
/// overlaps (0 when none).
pub fn contact_damage(world: &World, player: &crate::player::Player) -> f32 {
    let (min, max) = player.collision_shape().aabb(player.pos);
    let (lo, hi) = (min.floor().as_ivec3(), (max - DVec3::splat(1e-6)).floor().as_ivec3());
    let mut best = 0.0f32;
    for y in lo.y..=hi.y {
        for z in lo.z..=hi.z {
            for x in lo.x..=hi.x {
                if let Some(d) = world.get_block(IVec3::new(x, y, z)).and_then(damage) {
                    best = best.max(d);
                }
            }
        }
    }
    best
}

impl World {
    /// The cooking slots of the campfire at `p`.
    pub fn campfire_slots(&self, p: IVec3) -> Option<&Slots> {
        self.utilities.campfires.get(&p)
    }

    /// `CampfireBlockEntity.placeFood`: `item` goes into the first free
    /// slot of the campfire at `p` if it has a campfire recipe. Returns
    /// whether it went in.
    pub fn campfire_place_food(&mut self, p: IVec3, item: Item) -> bool {
        if cooked(item).is_none() || !self.get_block(p).is_some_and(|b| campfire_of(b).is_some()) {
            return false;
        }
        let slots = self.utilities.campfires.entry(p).or_default();
        let Some(i) = slots.items.iter().position(Option::is_none) else { return false };
        slots.items[i] = Some(Stack::new(item, 1));
        slots.progress[i] = 0;
        true
    }

    /// Lights the campfire at `p` (flint and steel, fire charges).
    pub fn light_campfire(&mut self, p: IVec3) -> bool {
        match self.get_block(p).and_then(campfire_of) {
            Some(c) if !c.lit && !c.wet => self.set_block(p, campfire(Campfire { lit: true, ..c })),
            _ => false,
        }
    }

    /// `CampfireBlock.dowse` by a shovel: puts out the fire at `p`; the
    /// food stays on it.
    pub fn douse_campfire(&mut self, p: IVec3) -> bool {
        match self.get_block(p).and_then(campfire_of) {
            Some(c) if c.lit => self.set_block(p, campfire(Campfire { lit: false, ..c })),
            _ => false,
        }
    }

    /// One tick of every campfire in a block-ticking chunk: lit ones cook
    /// (`cookTick`), unlit ones let their progress cool by two
    /// (`cooldownTick`); cooked food drops at the fire. Lit ones smoke.
    pub(super) fn tick_campfires(&mut self) {
        if self.utilities.campfires.is_empty() {
            return;
        }
        let mut done = Vec::new();
        let mut smoke = Vec::new();
        let positions: Vec<IVec3> = self.utilities.campfires.keys().copied().collect();
        for p in positions {
            if !self.activity.contains(p, true) {
                continue;
            }
            let Some(c) = self.get_block(p).and_then(campfire_of) else {
                self.utilities.campfires.remove(&p);
                continue;
            };
            let Some(slots) = self.utilities.campfires.get_mut(&p) else { continue };
            for i in 0..4 {
                let Some(stack) = slots.items[i] else { continue };
                if c.lit {
                    slots.progress[i] += 1;
                    if slots.progress[i] >= COOK_TICKS {
                        slots.items[i] = None;
                        slots.progress[i] = 0;
                        if let Some(out) = cooked(stack.item) {
                            done.push((p, Stack::new(out, 1)));
                        }
                    }
                } else {
                    slots.progress[i] = slots.progress[i].saturating_sub(2);
                }
            }
            if c.lit {
                smoke.push((p, c.signal));
            }
            if slots.is_empty() {
                self.utilities.campfires.remove(&p);
            }
        }
        self.drops.extend(done);
        for (p, signal) in smoke {
            self.campfire_smoke(p, signal);
        }
    }

    /// `CampfireBlockEntity.particleTick`: 11% of ticks, two or three puffs
    /// rise from the fire, much higher for a signal fire.
    fn campfire_smoke(&mut self, p: IVec3, signal: bool) {
        use crate::particles::{Burst, Kind, Request};
        if self.roll() % 100 >= 11 {
            return;
        }
        let n = 2 + self.roll() % 2;
        for _ in 0..n {
            let jitter = |r: u64| (r % 1000) as f64 / 1000.0 / 3.0 * if r.is_multiple_of(2) { 1.0 } else { -1.0 };
            let (a, b, c) = (self.roll(), self.roll(), self.roll());
            let pos = p.as_dvec3() + DVec3::new(0.5 + jitter(a), 0.6 + (b % 1000) as f64 / 1000.0, 0.5 + jitter(c));
            let mut burst = Burst::new(Kind::LargeSmoke, pos, 1);
            burst.velocity = DVec3::new(0.0, if signal { 0.14 } else { 0.07 }, 0.0);
            burst.forced = true;
            self.particles.push(Request::Burst(burst));
        }
    }

    /// A campfire changed: a wet campfire goes out (`placeLiquid` dowses
    /// it), and one that is gone spills its food.
    pub(super) fn track_campfire(&mut self, p: IVec3, old: Block, new: Block) {
        if campfire_of(old).is_some()
            && campfire_of(new).is_none()
            && let Some(slots) = self.utilities.campfires.remove(&p)
            && self.tile_drops
        {
            self.drops.extend(slots.items.into_iter().flatten().map(|s| (p, s)));
        }
        if let Some(c) = campfire_of(new)
            && c.lit
            && c.wet
        {
            self.edit_exact(p, campfire(Campfire { lit: false, ..c }), true);
        }
    }

    /// `CampfireBlock.updateShape` from below: a hay bale makes it a signal fire.
    pub(super) fn update_campfire_signal(&mut self, p: IVec3) {
        let Some(c) = self.get_block(p).and_then(campfire_of) else { return };
        let signal = self.get_block(p - IVec3::Y).is_some_and(smoke_source);
        if signal != c.signal {
            self.edit_exact(p, campfire(Campfire { signal, ..c }), true);
        }
    }

    pub(super) fn campfires_to_string(&self) -> String {
        self.utilities
            .campfires
            .iter()
            .map(|(p, s)| format!("c{},{},{}={}", p.x, p.y, p.z, s.serialize()))
            .collect::<Vec<_>>()
            .join("|")
    }

    pub(super) fn load_campfire(&mut self, pos: IVec3, text: &str) {
        if let Some(slots) = Slots::deserialize(text)
            && !slots.is_empty()
        {
            self.utilities.campfires.insert(pos, slots);
        }
    }
}
