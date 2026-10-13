//! A fishing bobber. It flies like Java's hook (about 1.1 blocks a tick,
//! 0.03 gravity, 0.92 drag), bobs in water, and bites after 100–600 ticks
//! minus 100 per Lure level. Rain can shorten that wait; a roof can stretch
//! it. Reeling during the bite rolls Java 26.3's `gameplay/fishing` loot
//! table ([`crate::content::fishing`]): junk, fish, and treasure in open water.
//!
//! The bob itself is a simplified spring, and the hook does not snag mobs.

use glam::{DVec3, IVec3};

use super::{Ctx, MobWorld, PlayerId, Rng};
use crate::content::fishing::{self, Condition, Entry, Loot};
use crate::enchant::JavaRandom;
use crate::inventory::Stack;
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
    /// The biome under the hook was a jungle when the fish bit (the junk
    /// table's bamboo `location_check`).
    jungle: bool,
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
    /// A LAN client's copy: drawn only, never simulated.
    pub(super) fn replica(owner: PlayerId, pos: DVec3, previous_pos: DVec3, biting: bool) -> Self {
        let mut b = Self::cast(owner, pos, DVec3::X, 0, 0, &mut Rng::new(0));
        b.pos = pos;
        b.previous_pos = previous_pos;
        b.biting = biting;
        b
    }

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
            jungle: false,
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
                self.jungle = world.cave_biome(cell).is_jungle();
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
            catch = roll_catch(rng, self.luck, CatchContext { open_water: self.open_water, jungle: self.jungle });
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

/// The loot context a reel is rolled in: the conditions the fishing
/// tables test.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CatchContext {
    /// `type_specific/fishing_hook` `in_open_water`.
    pub open_water: bool,
    /// The hook's biome is jungle, sparse jungle or bamboo jungle.
    pub jungle: bool,
}

impl CatchContext {
    fn allows(self, condition: Condition) -> bool {
        match condition {
            Condition::Always => true,
            Condition::InOpenWater => self.open_water,
            Condition::JungleBiome => self.jungle,
        }
    }
}

/// Java 26.3 `gameplay/fishing` with `luck` (Luck of the Sea): junk, fish or
/// (in open water) treasure. Closed water keeps junk and fish; only the
/// treasure entry is conditioned. `None` when every weight is zero.
pub fn roll_catch(rng: &mut Rng, luck: i32, ctx: CatchContext) -> Option<Stack> {
    let category = choose(rng, &fishing::CATEGORIES, luck, ctx)?.value;
    let loot = choose(rng, category.table(), luck, ctx)?.value;
    Some(loot_stack(rng, &loot))
}

/// Java's `LootPool.addRandomItem`: eligible entries with a positive
/// weight; a lone one is taken without a roll, otherwise `nextInt(total)`.
fn choose<'a, T>(rng: &mut Rng, entries: &'a [Entry<T>], luck: i32, ctx: CatchContext) -> Option<&'a Entry<T>> {
    let eligible = |e: &&Entry<T>| ctx.allows(e.condition) && e.weight(luck) > 0;
    let (count, total) = entries.iter().filter(eligible).fold((0, 0), |(n, t), e| (n + 1, t + e.weight(luck)));
    if count == 0 {
        return None;
    }
    if count == 1 {
        return entries.iter().find(eligible);
    }
    let mut roll = rng.next_int(total as u32) as i32;
    for entry in entries.iter().filter(eligible) {
        roll -= entry.weight(luck);
        if roll < 0 {
            return Some(entry);
        }
    }
    None
}

/// An item entry's stack after `set_count`, `set_damage` and
/// `enchant_with_levels`.
fn loot_stack(rng: &mut Rng, loot: &Loot) -> Stack {
    let mut stack = Stack::new(loot.item, loot.count);
    if let (Some((min, max)), Some(durability)) = (loot.damage, loot.item.durability()) {
        stack.damage = set_damage(rng.range(min, max), durability);
    }
    if let Some(levels) = loot.enchant_levels {
        let mut source = JavaRandom::new(rng.next_u64() as i64);
        (stack.item, stack.enchants) = crate::enchant::enchant_with_levels(&mut source, stack.item, levels);
    }
    stack
}

/// Java's `SetItemDamageFunction` (not additive): `fraction` is the
/// durability *remaining*, so damage is `floor((1 - clamp(fraction)) * max)`.
pub fn set_damage(fraction: f32, durability: u16) -> u16 {
    ((1.0 - fraction.clamp(0.0, 1.0)) * f32::from(durability)).floor() as u16
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
    use crate::item::Item;
    use crate::physics::{BlockSource, test_util::Grid};
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
            spawn_light: crate::simulation::lighting::SpawnLight::DAY,
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
    fn all_four_java_fish_are_obtainable_by_reeling() {
        let mut rng = Rng::new(99);
        let mut seen = [false; 4];
        let ctx = CatchContext { open_water: true, jungle: false };
        for _ in 0..4000 {
            let catch = roll_catch(&mut rng, 0, ctx).unwrap();
            for (i, entry) in fishing::FISH.iter().enumerate() {
                seen[i] |= catch.item == entry.value.item;
            }
        }
        assert!(seen.into_iter().all(|b| b));
    }

    #[test]
    fn closed_water_still_catches_fish_but_never_treasure() {
        assert_eq!((Item::COD.food(), Item::COOKED_SALMON.food()), (Some((2, 0.4)), Some((6, 9.6))));
        assert_eq!(Item::FISHING_ROD.durability(), Some(64));
        assert_eq!(crate::world::furnace::smelt(Item::SALMON), Some(Item::COOKED_SALMON));
        let mut rng = Rng::new(9);
        let mut fish = 0;
        for _ in 0..2000 {
            let stack = roll_catch(&mut rng, 0, CatchContext::default()).unwrap();
            assert!(!fishing::TREASURE.iter().any(|e| e.value.item == stack.item && e.value.enchant_levels.is_none()));
            assert!(stack.enchants.is_empty() && stack.item != Item::ENCHANTED_BOOK, "{}", stack.item.name());
            fish += usize::from(fishing::FISH.iter().any(|e| e.value.item == stack.item));
        }
        // Java: 85 of 95 weight in closed water.
        assert!((1700..1880).contains(&fish), "closed-water fish {fish}");
        // Luck past both junk and fish weights leaves nothing in closed water.
        assert_eq!(roll_catch(&mut rng, 85, CatchContext::default()), None);
        assert_eq!(set_damage(0.0, 64), 64);
        assert_eq!(set_damage(0.9, 64), 6);
        assert_eq!(set_damage(1.0, 64), 0);
    }

    #[test]
    fn a_bite_records_whether_the_hook_is_in_a_jungle() {
        struct Jungle(Pond);
        impl BlockSource for Jungle {
            fn block(&self, p: IVec3) -> Option<Block> {
                self.0.block(p)
            }
        }
        impl MobWorld for Jungle {
            fn loaded(&self, p: IVec3) -> bool {
                self.0.loaded(p)
            }
            fn surface(&self, x: i32, z: i32) -> Option<i32> {
                self.0.surface(x, z)
            }
            fn exposed(&self, p: IVec3) -> bool {
                self.0.exposed(p)
            }
            fn biome(&self, _: i32, _: i32) -> crate::world::terrain::Biome {
                crate::world::terrain::Biome::BambooJungle
            }
        }
        for (jungle, expected) in [(false, false), (true, true)] {
            let mut rng = Rng::new(5);
            let mut bobber = Bobber::cast(PlayerId::HOST, DVec3::new(0.5, 1.2, 0.5), DVec3::Y, 0, 0, &mut rng);
            bobber.state = State::Bobbing;
            bobber.time_until_hooked = 1;
            if jungle {
                bobber.tick_bite(&Jungle(pond()), IVec3::new(0, 1, 0), &mut rng);
            } else {
                bobber.tick_bite(&pond(), IVec3::new(0, 1, 0), &mut rng);
            }
            assert!(bobber.biting);
            assert_eq!(bobber.jungle, expected);
        }
    }
}
