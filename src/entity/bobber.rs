//! A fishing bobber. It flies like Java's hook (about 1.1 blocks a tick,
//! 0.03 gravity, 0.92 drag), bobs in water, and bites after 100–600 ticks
//! minus 100 per Lure level. Rain can shorten that wait; a roof can stretch
//! it. Reeling during the bite rolls the fish, junk or treasure table.
//!
//! The bob itself is a simplified spring, and the hook does not snag mobs.
//! Tropical fish, pufferfish and several junk and treasure items are absent,
//! so those weights are left out of the pools.

use glam::{DVec3, IVec3};

use super::{Ctx, MobWorld, PlayerId, Rng};
use crate::enchant::{Enchantment, Enchants};
use crate::inventory::Stack;
use crate::item::Item;
use crate::world::block::Block;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Flying,
    Bobbing,
}

#[derive(Clone, Copy, Debug)]
pub struct Bobber {
    pub owner: PlayerId,
    pub pos: DVec3,
    pub previous_pos: DVec3,
    vel: DVec3,
    state: State,
    on_ground: bool,
    ground_time: f32,
    /// Seconds of fractional ticks waiting to be applied to the bite clock.
    tick_acc: f32,
    time_until_lured: i32,
    time_until_hooked: i32,
    nibble: i32,
    pub biting: bool,
    open_water: bool,
    out_of_water: i32,
    lure: i32,
    luck: i32,
}

/// What reeling a bobber does to the rod and what it brings back.
pub struct Retrieve {
    pub wear: u16,
    pub catch: Option<Stack>,
    pub xp: u32,
}

impl Bobber {
    /// Cast from `eye` along `dir`. `lure` and `luck` are the rod's levels.
    pub fn cast(owner: PlayerId, eye: DVec3, dir: DVec3, lure: u8, luck: u8, rng: &mut Rng) -> Self {
        let dir = dir.normalize_or(DVec3::NEG_Z);
        let mut scale = || 0.6 + triangle(rng, 0.5, 0.0103365);
        let vel = DVec3::new(dir.x * scale(), dir.y * scale(), dir.z * scale()) * 20.0;
        Self {
            owner,
            pos: eye,
            previous_pos: eye,
            vel,
            state: State::Flying,
            on_ground: false,
            ground_time: 0.0,
            tick_acc: 0.0,
            time_until_lured: 0,
            time_until_hooked: 0,
            nibble: 0,
            biting: false,
            open_water: true,
            out_of_water: 0,
            lure: i32::from(lure),
            luck: i32::from(luck),
        }
    }

    pub(super) fn update<W: MobWorld + ?Sized>(&mut self, dt: f64, world: &W, rng: &mut Rng, ctx: &Ctx) -> bool {
        let owner = ctx.players.iter().find(|p| p.id == self.owner && p.alive);
        if owner.is_none_or(|p| p.pos.distance_squared(self.pos) > 1024.0) || self.pos.y < -64.0 {
            return false;
        }
        if self.on_ground {
            self.ground_time += dt as f32;
            if self.ground_time >= 60.0 {
                return false;
            }
        }
        match self.state {
            State::Flying => self.fly(dt, world),
            State::Bobbing => self.bob(dt, world, rng),
        }
        true
    }

    fn fly<W: MobWorld + ?Sized>(&mut self, dt: f64, world: &W) {
        let ticks = dt * 20.0;
        self.vel.y -= 12.0 * dt;
        self.vel *= 0.92_f64.powf(ticks);
        let delta = self.vel * dt;
        let steps = ((delta.length() / 0.2).ceil() as u32).clamp(1, 32);
        for _ in 0..steps {
            let next = self.pos + delta / steps as f64;
            let cell = next.floor().as_ivec3();
            if world.block(cell).is_some_and(|b| b.holds_water()) {
                self.vel.x *= 0.3;
                self.vel.y *= 0.2;
                self.vel.z *= 0.3;
                self.pos = next;
                self.state = State::Bobbing;
                self.on_ground = false;
                return;
            }
            if world.block(cell).is_some_and(|b| b.is_solid()) {
                self.vel = DVec3::ZERO;
                self.on_ground = true;
                return;
            }
            self.pos = next;
        }
    }

    fn bob<W: MobWorld + ?Sized>(&mut self, dt: f64, world: &W, rng: &mut Rng) {
        let ticks = dt * 20.0;
        let cell = self.pos.floor().as_ivec3();
        let in_water = world.block(cell).is_some_and(|b| b.holds_water());
        if in_water {
            self.out_of_water = (self.out_of_water - 1).max(0);
            let surface = cell.y as f64 + 0.85;
            let mut gap = self.pos.y - surface;
            if gap.abs() < 0.01 {
                gap += gap.signum() * 0.1;
            }
            self.vel.y -= gap * 0.2;
            self.vel.x *= 0.9_f64.powf(ticks);
            self.vel.z *= 0.9_f64.powf(ticks);
            self.tick_acc += dt as f32 * 20.0;
            let mut steps = 0;
            while self.tick_acc >= 1.0 && steps < 40 {
                self.tick_acc -= 1.0;
                self.tick_bite(world, cell, rng);
                steps += 1;
            }
        } else {
            self.out_of_water = (self.out_of_water + 1).min(10);
            self.vel.y -= 12.0 * dt;
        }
        self.vel *= 0.92_f64.powf(ticks);
        self.pos += self.vel * dt;
    }

    /// One Java tick of [`FishingHook.catchingFish`].
    fn tick_bite<W: MobWorld + ?Sized>(&mut self, world: &W, cell: IVec3, rng: &mut Rng) {
        let above = cell + IVec3::Y;
        let mut step = 1;
        if rng.next_f32() < 0.25 && world.rains_on(above) {
            step += 1;
        }
        if rng.next_f32() < 0.5 && !world.exposed(above) {
            step -= 1;
        }
        if self.nibble > 0 {
            self.nibble -= 1;
            if self.nibble <= 0 {
                self.time_until_lured = 0;
                self.time_until_hooked = 0;
                self.biting = false;
            }
        } else if self.time_until_hooked > 0 {
            self.time_until_hooked -= step;
            if self.time_until_hooked <= 0 {
                self.nibble = 20 + (rng.next_f32() * 21.0) as i32;
                self.biting = true;
            }
        } else if self.time_until_lured > 0 {
            self.time_until_lured -= step;
            if self.time_until_lured <= 0 {
                self.time_until_hooked = 20 + (rng.next_f32() * 61.0) as i32;
            }
        } else {
            self.time_until_lured = 100 + (rng.next_f32() * 501.0) as i32 - self.lure * 100;
        }
        if self.nibble <= 0 && self.time_until_hooked <= 0 {
            self.open_water = true;
        } else {
            self.open_water = self.open_water && self.out_of_water < 10 && open_water(world, cell);
        }
    }

    /// Reel in. A bite returns loot; sitting on the ground wears the rod for two.
    pub fn retrieve(&self, rng: &mut Rng) -> Retrieve {
        let mut wear = 0;
        let mut catch = None;
        let mut xp = 0;
        if self.biting {
            wear = 1;
            catch = Some(roll_catch(rng, self.luck, self.open_water));
            xp = 1 + (rng.next_f32() * 6.0) as u32;
        }
        if self.on_ground {
            wear = 2;
        }
        Retrieve { wear, catch, xp }
    }
}

fn triangle(rng: &mut Rng, mean: f64, scale: f64) -> f64 {
    mean + (f64::from(rng.next_f32()) - f64::from(rng.next_f32())) * scale
}

/// Java's `base + quality * luck`, never below zero.
fn weight(base: i32, quality: i32, luck: i32) -> i32 {
    (base + quality * luck).max(0)
}

/// Fish, junk or treasure. Closed water rolls junk only.
pub fn roll_catch(rng: &mut Rng, luck: i32, open_water: bool) -> Stack {
    let fish = if open_water { weight(85, -1, luck) } else { 0 };
    let junk = weight(10, -2, luck);
    let treasure = if open_water { weight(5, 2, luck) } else { 0 };
    let total = fish + junk + treasure;
    let mut roll = if total == 0 { 0 } else { (rng.next_f32() * total as f32) as i32 };
    if roll < fish {
        return Stack::new(pick(rng, &[(Item::COD, 60), (Item::SALMON, 25)]), 1);
    }
    roll -= fish;
    if roll < junk || treasure == 0 {
        return junk_stack(rng);
    }
    let item = pick(
        rng,
        &[
            (Item::ENCHANTED_BOOK, 1),
            (Item::BOW, 1),
            (Item::FISHING_ROD, 1),
            (Item::NAME_TAG, 1),
            (Item::NAUTILUS_SHELL, 1),
        ],
    );
    Stack { enchants: treasure_enchants(rng, item), ..Stack::new(item, 1) }
}

fn pick(rng: &mut Rng, table: &[(Item, i32)]) -> Item {
    let total: i32 = table.iter().map(|(_, w)| w).sum();
    let mut roll = (rng.next_f32() * total.max(1) as f32) as i32;
    for &(item, w) in table {
        if roll < w {
            return item;
        }
        roll -= w;
    }
    table.last().map_or(Item::STICK, |t| t.0)
}

fn junk_stack(rng: &mut Rng) -> Stack {
    let boots = Item::armor(crate::item::ArmorPiece::Boots, crate::item::ArmorMaterial::Leather);
    let bottle = Item::potion(crate::potion::Potion::WATER);
    let table = [
        (Item::LEATHER, 10),
        (boots, 10),
        (Item::BONE, 10),
        (Item::ROTTEN_FLESH, 10),
        (Item::STRING, 5),
        (Item::STICK, 5),
        (Item::BOWL, 10),
        (Item::FISHING_ROD, 2),
        (bottle, 10),
    ];
    let item = pick(rng, &table);
    let mut stack = Stack::new(item, 1);
    if item == Item::FISHING_ROD {
        stack.damage = (rng.next_f32() * 63.0) as u16;
    }
    stack
}

fn treasure_enchants(rng: &mut Rng, item: Item) -> Enchants {
    let mut chosen = None;
    let mut seen = 0u32;
    for enchantment in Enchantment::ALL {
        if enchantment.fits(item) && !enchantment.def().curse {
            seen += 1;
            if rng.next_f32() < 1.0 / seen as f32 {
                chosen = Some(enchantment);
            }
        }
    }
    let Some(enchantment) = chosen else { return Enchants::NONE };
    let level = 1 + (rng.next_f32() * enchantment.def().max_level as f32) as u8;
    Enchants::NONE.with(enchantment, level.min(enchantment.def().max_level).max(1))
}

/// Java's 5×4×5 open-water column: two water layers under two of air.
fn open_water<W: MobWorld + ?Sized>(world: &W, origin: IVec3) -> bool {
    #[derive(Clone, Copy, PartialEq)]
    enum Kind {
        Above,
        Inside,
        Invalid,
    }
    let mut previous = Kind::Invalid;
    for dy in -1..=2 {
        let mut layer = None;
        for dz in -2..=2 {
            for dx in -2..=2 {
                let cell = match world.block(origin + IVec3::new(dx, dy, dz)) {
                    Some(Block::WATER) => Kind::Inside,
                    Some(Block::AIR) => Kind::Above,
                    _ => Kind::Invalid,
                };
                layer = Some(match layer {
                    None => cell,
                    Some(kind) if kind == cell => kind,
                    _ => Kind::Invalid,
                });
            }
        }
        let layer = layer.unwrap_or(Kind::Invalid);
        let bad = layer == Kind::Invalid
            || (layer == Kind::Above && previous == Kind::Invalid)
            || (layer == Kind::Inside && previous == Kind::Above);
        if bad {
            return false;
        }
        previous = layer;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::{BlockSource, test_util::Grid};
    use crate::potion::Potion;
    use crate::world::block::Block;

    struct Pond {
        grid: Grid,
        rain: bool,
        sky: bool,
    }

    impl BlockSource for Pond {
        fn block(&self, p: IVec3) -> Option<Block> {
            self.grid.block(p)
        }
    }

    impl MobWorld for Pond {
        fn loaded(&self, _: IVec3) -> bool {
            true
        }
        fn surface(&self, _: i32, _: i32) -> Option<i32> {
            Some(1)
        }
        fn exposed(&self, p: IVec3) -> bool {
            self.sky && p.y >= 2
        }
        fn rains_on(&self, _: IVec3) -> bool {
            self.rain
        }
    }

    fn pond() -> Pond {
        let mut grid = Grid::flat(0);
        for y in 0..=1 {
            for z in -2..=2 {
                for x in -2..=2 {
                    grid.set(IVec3::new(x, y, z), Block::WATER);
                }
            }
        }
        Pond { grid, rain: false, sky: true }
    }

    fn host_ctx() -> Ctx {
        Ctx {
            players: vec![super::super::Target::new(PlayerId::HOST, DVec3::new(0.5, 2.0, 0.5), true)],
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: crate::world::terrain::Dimension::Overworld,
        }
    }

    #[test]
    fn lure_shortens_the_wait_and_a_bite_is_a_short_window() {
        let world = pond();
        let mut rng = Rng::new(4);
        let mut bobber = Bobber::cast(PlayerId::HOST, DVec3::new(0.5, 1.2, 0.5), DVec3::Y, 0, 0, &mut rng);
        bobber.state = State::Bobbing;
        bobber.tick_bite(&world, IVec3::new(0, 1, 0), &mut rng);
        assert!((100..=600).contains(&(bobber.time_until_lured)), "plain wait {}", bobber.time_until_lured);
        bobber.time_until_lured = 0;
        bobber.lure = 3;
        bobber.tick_bite(&world, IVec3::new(0, 1, 0), &mut rng);
        assert!((-200..=300).contains(&bobber.time_until_lured), "lure iii subtracts 300: {}", bobber.time_until_lured);

        bobber.lure = 0;
        bobber.time_until_lured = 1;
        bobber.time_until_hooked = 0;
        bobber.nibble = 0;
        bobber.tick_bite(&world, IVec3::new(0, 1, 0), &mut rng);
        let hops = bobber.time_until_hooked;
        assert!((20..=80).contains(&hops));
        for _ in 0..hops {
            bobber.tick_bite(&world, IVec3::new(0, 1, 0), &mut rng);
        }
        assert!(bobber.biting);
        assert!((20..=40).contains(&bobber.nibble));
    }

    #[test]
    fn the_bobber_lands_in_water_and_open_water_is_a_clear_column() {
        let world = pond();
        let mut rng = Rng::new(2);
        let mut bobber =
            Bobber::cast(PlayerId::HOST, DVec3::new(0.5, 4.0, 0.5), DVec3::new(0.0, -1.0, 0.0), 0, 0, &mut rng);
        let ctx = host_ctx();
        for _ in 0..40 {
            assert!(bobber.update(1.0 / 20.0, &world, &mut rng, &ctx));
        }
        assert_eq!(bobber.state, State::Bobbing, "pos {:?}", bobber.pos);
        assert!(open_water(&world, IVec3::new(0, 1, 0)));
        let mut blocked = pond();
        blocked.grid.set(IVec3::new(0, 0, 0), Block::STONE);
        assert!(!open_water(&blocked, IVec3::new(0, 1, 0)));
    }

    #[test]
    fn closed_water_is_junk_and_luck_favours_treasure() {
        assert_eq!(weight(85, -1, 3), 82);
        assert_eq!(weight(10, -2, 3), 4);
        assert_eq!(weight(5, 2, 3), 11);
        assert_eq!((Item::COD.food(), Item::COOKED_SALMON.food()), (Some((2, 0.4)), Some((6, 9.6))));
        assert_eq!(Item::FISHING_ROD.durability(), Some(64));
        assert_eq!(crate::enchant::enchantability(Item::FISHING_ROD), 1);
        assert!(Enchantment::Lure.fits(Item::FISHING_ROD) && Enchantment::LuckOfTheSea.fits(Item::FISHING_ROD));
        assert!(!Enchantment::Lure.fits(Item::STICK));
        assert_eq!(Enchantment::ALL[26], Enchantment::VanishingCurse);
        assert_eq!(Enchantment::ALL[27], Enchantment::FrostWalker);
        assert_eq!(Enchantment::ALL[28], Enchantment::Lure);
        assert_eq!(crate::world::furnace::smelt(Item::SALMON), Some(Item::COOKED_SALMON));

        let mut rng = Rng::new(9);
        for _ in 0..80 {
            let stack = roll_catch(&mut rng, 0, false);
            assert!(
                matches!(
                    stack.item,
                    Item::LEATHER
                        | Item::BONE
                        | Item::ROTTEN_FLESH
                        | Item::STRING
                        | Item::STICK
                        | Item::BOWL
                        | Item::FISHING_ROD
                ) || stack.item == Item::armor(crate::item::ArmorPiece::Boots, crate::item::ArmorMaterial::Leather)
                    || stack.item == Item::potion(Potion::WATER),
                "{}",
                stack.item.name()
            );
        }
        let mut treasure = 0;
        let mut rng = Rng::new(3);
        for _ in 0..400 {
            let stack = roll_catch(&mut rng, 3, true);
            if matches!(stack.item, Item::ENCHANTED_BOOK | Item::BOW | Item::FISHING_ROD) && !stack.enchants.is_empty()
            {
                treasure += 1;
            }
        }
        assert!((15..100).contains(&treasure), "luck iii treasure rolls: {treasure}");
    }
}
