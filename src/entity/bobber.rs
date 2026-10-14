//! A fishing bobber. It flies like Java's hook (about 1.1 blocks a tick,
//! 0.03 gravity, 0.92 drag), bobs in water, and bites after 100–600 ticks
//! minus 100 per Lure level. Rain can shorten that wait; a roof can stretch
//! it. Reeling during the bite rolls Java 26.3's `gameplay/fishing` loot
//! table ([`crate::content::fishing`]): junk, fish, and treasure in open water.
//!
//! The hook bobs on Java's per-tick spring against the fluid surface and snags
//! the first mob it touches, which reeling drags toward the player.
//! Luck and the jungle biome are read when the line is reeled, as in Java.

use glam::{DVec3, IVec3};

use super::mob_index::MobIndex;
use super::{Ctx, Mob, MobWorld, PlayerId, Rng};
use crate::content::fishing::{self, Condition, Entry, Loot};
use crate::enchant::JavaRandom;
use crate::inventory::Stack;
use crate::world::block::Block;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Flying,
    Bobbing,
    /// Snagged on a mob (Java's `HOOKED_IN_ENTITY`).
    Hooked,
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
    /// The `uid` of the mob the hook is snagged on.
    hooked: Option<u32>,
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
            out_of_water: 0,
            lure: i32::from(lure),
            luck: i32::from(luck),
            hooked: None,
        }
    }

    pub(super) fn update<W: MobWorld + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        rng: &mut Rng,
        ctx: &Ctx,
        mobs: &[Mob],
        index: &MobIndex,
    ) -> bool {
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
            State::Flying => self.fly(dt, world, mobs, index),
            State::Bobbing => self.bob(dt, world, rng),
            State::Hooked => self.follow(mobs),
        }
        true
    }

    fn fly<W: MobWorld + ?Sized>(&mut self, dt: f64, world: &W, mobs: &[Mob], index: &MobIndex) {
        let ticks = dt * 20.0;
        self.vel.y -= 12.0 * dt;
        self.vel *= 0.92_f64.powf(ticks);
        let delta = self.vel * dt;
        let steps = ((delta.length() / 0.2).ceil() as u32).clamp(1, 32);
        for _ in 0..steps {
            let next = self.pos + delta / steps as f64;
            let cell = next.floor().as_ivec3();
            if let Some(mob) = self.snagged(next, mobs, index) {
                self.hooked = Some(mob.uid);
                self.vel = DVec3::ZERO;
                self.state = State::Hooked;
                self.follow(mobs);
                return;
            }
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

    /// Java's `onHitEntity`: the first living mob whose box, widened by the
    /// 0.3 projectile margin, holds the hook.
    fn snagged<'a>(&self, at: DVec3, mobs: &'a [Mob], index: &MobIndex) -> Option<&'a Mob> {
        let mut hit: Option<usize> = None;
        index.visit(at - DVec3::Y * 2.0, 4.0, |i| {
            let m = &mobs[i];
            let (min, max) = m.aabb();
            if m.alive()
                && m.uid != 0
                && at.cmpge(min - 0.3).all()
                && at.cmple(max + 0.3).all()
                && hit.is_none_or(|j| i < j)
            {
                hit = Some(i);
            }
        });
        hit.map(|i| &mobs[i])
    }

    /// Hooked: ride the mob at 80% of its height, or fall free if it is gone.
    fn follow(&mut self, mobs: &[Mob]) {
        match self.hooked.and_then(|uid| mobs.iter().find(|m| m.uid == uid && m.alive())) {
            Some(mob) => {
                let (min, max) = mob.aabb();
                self.pos = DVec3::new(mob.pos.x, min.y + (max.y - min.y) * 0.8, mob.pos.z);
            }
            None => {
                self.hooked = None;
                self.state = State::Flying;
            }
        }
    }

    /// Java's `FishingHook.tick` while bobbing, once per game tick: a spring
    /// toward the fluid surface weighted by a random factor, 0.9 sideways drag,
    /// a dip while a fish bites, gravity out of the water and 0.92 drag. The
    /// velocity is kept in blocks a second.
    fn bob<W: MobWorld + ?Sized>(&mut self, dt: f64, world: &W, rng: &mut Rng) {
        self.tick_acc += dt as f32 * 20.0;
        let mut steps = 0;
        while self.tick_acc >= 1.0 && steps < 40 {
            self.tick_acc -= 1.0;
            steps += 1;
            let cell = self.pos.floor().as_ivec3();
            let in_water = world.block(cell).is_some_and(|b| b.holds_water());
            let mut vy = self.vel.y / 20.0;
            if in_water {
                self.out_of_water = (self.out_of_water - 1).max(0);
                let mut gap = self.pos.y + vy - f64::from(cell.y) - water_height(world, cell);
                if gap.abs() < 0.01 {
                    gap += gap.signum() * 0.1;
                }
                self.vel.x *= 0.9;
                self.vel.z *= 0.9;
                vy -= gap * f64::from(rng.next_f32()) * 0.2;
                if self.biting {
                    vy -= 0.1 * f64::from(rng.next_f32()) * f64::from(rng.next_f32());
                }
                self.tick_bite(world, cell, rng);
            } else {
                self.out_of_water = (self.out_of_water + 1).min(10);
                vy -= 0.03;
            }
            self.vel.y = vy * 20.0;
            self.pos += self.vel / 20.0;
            self.vel *= 0.92;
        }
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
    /// `owner_luck` is the player's Luck attribute (Java adds it to the rod's
    /// Luck of the Sea) and `jungle` whether the hook's biome is a jungle now,
    /// as the loot table's `location_check` reads it at the reel.
    pub fn retrieve(&self, rng: &mut Rng, owner_luck: i32, jungle: bool) -> Retrieve {
        let mut wear = 0;
        let mut catch = None;
        let mut xp = 0;
        if self.hooked.is_some() {
            // Java: a snagged entity costs five uses and is pulled in (no loot).
            wear = 5;
        } else if self.biting {
            wear = 1;
            catch = roll_catch(rng, self.luck + owner_luck, CatchContext { open_water: self.open_water, jungle });
            xp = 1 + (rng.next_f32() * 6.0) as u32;
        }
        if self.on_ground {
            wear = 2;
        }
        Retrieve { wear, catch, xp }
    }

    /// The mob the hook is snagged on, which reeling drags toward the player.
    pub fn hooked_mob(&self) -> Option<u32> {
        self.hooked
    }
}

/// Java's `FluidState.getHeight`: a full block under more water, otherwise
/// the fluid amount over nine (8/9 for a source).
fn water_height<W: MobWorld + ?Sized>(world: &W, cell: IVec3) -> f64 {
    if world.block(cell + IVec3::Y).is_some_and(|b| b.is_water()) {
        return 1.0;
    }
    world.block(cell).and_then(|b| b.water_level()).map_or(0.0, |level| f64::from(8 - level.min(7)) / 9.0)
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
            assert!(bobber.update(1.0 / 20.0, &world, &mut rng, &ctx, &[], &MobIndex::default()));
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

    /// Java's spring keeps the hook riding the water surface (8/9 of the cell),
    /// jittering by its random weight, and dips while a fish bites.
    #[test]
    fn the_hook_bobs_around_the_water_surface() {
        let world = pond();
        let mut rng = Rng::new(8);
        let mut bobber = Bobber::cast(PlayerId::HOST, DVec3::new(0.5, 1.5, 0.5), DVec3::NEG_Y, 0, 0, &mut rng);
        bobber.state = State::Bobbing;
        bobber.vel = DVec3::new(0.0, -2.0, 0.0);
        let ctx = host_ctx();
        let mut heights = Vec::new();
        for tick in 0..400 {
            assert!(bobber.update(0.05, &world, &mut rng, &ctx, &[], &MobIndex::default()));
            if tick >= 100 && !bobber.biting {
                heights.push(bobber.pos.y);
            }
        }
        let mean = heights.iter().sum::<f64>() / heights.len() as f64;
        assert!((mean - (1.0 + 8.0 / 9.0)).abs() < 0.15, "rides the surface at {mean}");
        assert!(heights.iter().all(|y| (1.4..2.2).contains(y)), "bounded bob");
    }

    #[test]
    fn luck_and_the_jungle_biome_are_read_when_the_line_is_reeled() {
        let mut rng = Rng::new(5);
        let mut bobber = Bobber::cast(PlayerId::HOST, DVec3::new(0.5, 1.2, 0.5), DVec3::Y, 0, 0, &mut rng);
        bobber.biting = true;
        bobber.open_water = true;
        let junk_bamboo = |jungle: bool, luck: i32| {
            let mut rng = Rng::new(21);
            (0..6000)
                .filter(|_| {
                    bobber
                        .retrieve(&mut rng, luck, jungle)
                        .catch
                        .is_some_and(|s| s.item == Item::from_block(crate::world::overworld_blocks::BAMBOO))
                })
                .count()
        };
        assert_eq!(junk_bamboo(false, 0), 0);
        assert!(junk_bamboo(true, 0) > 0);
        // Bad Luck (-1 per level) shifts weight from treasure to junk; Luck does the opposite.
        let treasure = |luck: i32| {
            let mut rng = Rng::new(22);
            (0..6000)
                .filter(|_| {
                    let catch = bobber.retrieve(&mut rng, luck, false).catch.unwrap();
                    fishing::TREASURE.iter().any(|e| e.value.item == catch.item)
                })
                .count()
        };
        assert!(
            treasure(2) > treasure(0) && treasure(0) > treasure(-2),
            "{} {} {}",
            treasure(2),
            treasure(0),
            treasure(-2)
        );
    }
}
