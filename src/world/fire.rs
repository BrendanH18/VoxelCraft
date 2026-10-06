//! Fire and lava ignition. Random ticks share the growth dispatch; active
//! fires also tick every 30..39 game ticks, as in Java Minecraft. Only
//! scheduled fires cost work, and age-only edits never rebuild meshes.
//!
//! Reference: MinecraftForge's 1.21.1 FireBlock/LavaFluid patches at
//! https://github.com/MinecraftForge/MinecraftForge/tree/1.21.1/patches/minecraft/net/minecraft/world/level

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use glam::{DVec3, IVec3};
use rustc_hash::FxHashMap;

use super::World;
use super::block::Block;
use super::chunk::{CHUNK_SIZE_I, ChunkData, WORLD_HEIGHT, chunk_of};

const SIDES: [IVec3; 6] = [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z];
const HORIZONTAL: [IVec3; 4] = [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z];
const MAX_UPDATES: usize = 4096;

#[derive(Default)]
pub(super) struct FireState {
    /// Authoritative deadlines; removed/replaced fires leave harmless
    /// stale heap entries that disappear at their original deadline.
    pending: FxHashMap<IVec3, u64>,
    queue: BinaryHeap<Reverse<(u64, [i32; 3])>>,
    tick: u64,
    remainder: f64,
}

impl World {
    fn schedule_fire(&mut self, p: IVec3) {
        if self.fire.pending.contains_key(&p) {
            return;
        }
        let due = self.fire.tick + 30 + self.roll() % 10;
        self.fire.pending.insert(p, due);
        self.fire.queue.push(Reverse((due, p.to_array())));
    }

    pub(super) fn track_fire(&mut self, p: IVec3, old: Block, new: Block) {
        if new.is_fire() {
            self.schedule_fire(p);
        } else if old.is_fire() {
            self.fire.pending.remove(&p);
        }
    }

    /// Generated terrain contains no fire. Scan saved chunks once on load
    /// to resume their fires, preserving ages stored in the block ids.
    pub(super) fn load_fires(&mut self, cpos: IVec3, data: &ChunkData) {
        if data.uniform().is_some_and(|b| !b.is_fire()) {
            return;
        }
        let mut i = 0;
        data.for_each_block(|b| {
            if b.is_fire() {
                let l =
                    IVec3::new(i % CHUNK_SIZE_I, i / (CHUNK_SIZE_I * CHUNK_SIZE_I), i / CHUNK_SIZE_I % CHUNK_SIZE_I);
                self.schedule_fire(cpos * CHUNK_SIZE_I + l);
            }
            i += 1;
        });
    }

    /// Advances only due fires in the same simulation radius as growth.
    /// Distant fires keep their ages until the player returns.
    pub fn tick_fire(&mut self, dt: f64, player: DVec3) {
        if dt <= 0.0 {
            return;
        }
        self.fire.remainder += dt * 20.0;
        let ticks = self.fire.remainder as u64;
        self.fire.remainder -= ticks as f64;
        self.fire.tick += ticks;
        let center = chunk_of(player.floor().as_ivec3());
        for _ in 0..MAX_UPDATES {
            let Some(&Reverse((due, xyz))) = self.fire.queue.peek() else { break };
            if due > self.fire.tick {
                break;
            }
            self.fire.queue.pop();
            let p = IVec3::from_array(xyz);
            if self.fire.pending.get(&p) != Some(&due) {
                continue;
            }
            self.fire.pending.remove(&p);
            if !self.get_block(p).is_some_and(|b| b.is_fire()) {
                continue;
            }
            let distance = (chunk_of(p) - center).abs();
            if (distance.x <= super::growth::TICK_RADIUS && distance.z <= super::growth::TICK_RADIUS)
                || self.agent_centers.iter().any(|c| {
                    let d = (chunk_of(p) - *c).abs();
                    d.x <= super::growth::TICK_RADIUS && d.z <= super::growth::TICK_RADIUS
                })
            {
                self.random_tick(p);
            }
            if self.get_block(p).is_some_and(|b| b.is_fire()) {
                self.schedule_fire(p);
            }
        }
    }

    fn fire_fuel(&self, p: IVec3) -> u8 {
        SIDES.iter().filter_map(|&d| self.get_block(p + d)).map(|b| b.fire_odds().0).max().unwrap_or(0)
    }

    fn fire_supported(&self, p: IVec3) -> bool {
        self.get_block(p - IVec3::Y).is_some_and(|b| b.supports_fire()) || self.fire_fuel(p) > 0
    }

    fn rain_near_fire(&self, p: IVec3) -> bool {
        self.rains_on(p) || HORIZONTAL.iter().any(|&d| self.rains_on(p + d))
    }

    pub(super) fn extinguish_unsupported_fire(&mut self, p: IVec3) {
        for q in std::iter::once(p).chain(SIDES.map(|d| p + d)) {
            if self.get_block(q).is_some_and(|b| b.is_fire())
                && SIDES.iter().all(|&d| self.get_block(q + d).is_some())
                && !self.fire_supported(q)
            {
                self.edit(q, Block::AIR, false);
            }
        }
    }

    /// Flint and steel lights a portal first, otherwise a supported empty
    /// cell. Failed uses neither place fire nor spend tool durability.
    pub fn ignite(&mut self, p: IVec3) -> bool {
        if !(0..WORLD_HEIGHT).contains(&p.y) {
            return false;
        }
        self.light_portal(p)
            || (self.get_block(p) == Some(Block::AIR) && self.fire_supported(p) && self.set_block(p, Block::FIRE))
    }

    pub(super) fn tick_fire_block(&mut self, p: IVec3, age: u8) {
        if SIDES.iter().any(|&d| self.get_block(p + d).is_none()) {
            return;
        }
        if !self.fire_supported(p) {
            self.edit(p, Block::AIR, false);
            return;
        }
        let below = self.get_block(p - IVec3::Y).unwrap_or(Block::AIR);
        // Bedrock burns forever in the End, like the crystals' flames.
        let eternal = below == Block::NETHERRACK
            || (below == Block::BEDROCK && self.generator.dimension == super::terrain::Dimension::End);
        if !eternal && self.rain_near_fire(p) && self.roll() % 100 < 20 + age as u64 * 3 {
            self.edit(p, Block::AIR, false);
            return;
        }
        let next_age = (age + (self.roll() % 3 / 2) as u8).min(15);
        if next_age != age {
            self.edit(p, Block::fire(next_age), false);
        }
        if !eternal {
            if self.fire_fuel(p) == 0 {
                if !below.supports_fire() || age > 3 {
                    self.edit(p, Block::AIR, false);
                }
                return;
            }
            if age == 15 && below.fire_odds().0 == 0 && self.one_in(4) {
                self.edit(p, Block::AIR, false);
                return;
            }
        }

        let humid = matches!(self.foliage_at(p.x, p.z), Some(1 | 3));
        // Direct neighbours are consumed, faster vertically. TNT primes
        // instead of dropping as an item; burned blocks never drop loot.
        for d in SIDES {
            let q = p + d;
            let Some(b) = self.get_block(q) else { continue };
            let chance = if d.y == 0 { 300 } else { 250 } - if humid { 50 } else { 0 };
            if b.fire_odds().1 == 0 || self.roll() % chance >= b.fire_odds().1 as u64 {
                continue;
            }
            let new = if b != Block::TNT && self.roll() % (age as u64 + 10) < 5 && !self.rains_on(q) {
                Block::fire((age + (self.roll() % 5 / 4) as u8).min(15))
            } else {
                Block::AIR
            };
            if self.edit(q, new, false) {
                if b == Block::TNT {
                    self.primed_tnt.push((q, false));
                }
                self.settle(q);
            }
        }

        // Fire can leap one block horizontally and up to four upward,
        // provided the empty destination is beside fuel. Normal difficulty
        // adds 14 encouragement; humid biomes halve the spread chance.
        for dy in -1..=4 {
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let q = p + IVec3::new(dx, dy, dz);
                    if q == p || self.get_block(q) != Some(Block::AIR) || !(0..WORLD_HEIGHT).contains(&q.y) {
                        continue;
                    }
                    let fuel = self.fire_fuel(q);
                    if fuel == 0 {
                        continue;
                    }
                    let odds = (fuel as u64 + 40 + 14) / (age as u64 + 30) / if humid { 2 } else { 1 };
                    let bound = 100 + (dy - 1).max(0) as u64 * 100;
                    if odds > 0 && self.roll() % bound <= odds && !self.rain_near_fire(q) {
                        let age = (age + (self.roll() % 5 / 4) as u8).min(15);
                        if !self.light_portal(q) {
                            self.edit(q, Block::fire(age), false);
                        }
                    }
                }
            }
        }
    }

    /// Minecraft's lava random tick: walk upward 1..2 cells, looking for
    /// air beside fuel, or try three nearby blocks at the lava's height.
    /// Unloaded chunks and solid blocks stop the upward walk.
    pub(super) fn tick_lava_fire(&mut self, p: IVec3) {
        let steps = self.roll() % 3;
        if steps > 0 {
            let mut q = p;
            for _ in 0..steps {
                q += IVec3::new((self.roll() % 3) as i32 - 1, 1, (self.roll() % 3) as i32 - 1);
                match self.get_block(q) {
                    Some(Block::AIR) if q.y < WORLD_HEIGHT => {
                        if SIDES.iter().any(|&d| self.get_block(q + d).is_some_and(|b| b.ignited_by_lava())) {
                            if !self.light_portal(q) {
                                self.edit(q, Block::FIRE, false);
                            }
                            return;
                        }
                    }
                    None => return,
                    Some(b) if b.is_solid() => return,
                    _ => {}
                }
            }
        } else {
            for _ in 0..3 {
                let q = p + IVec3::new((self.roll() % 3) as i32 - 1, 0, (self.roll() % 3) as i32 - 1);
                let above = q + IVec3::Y;
                if above.y < WORLD_HEIGHT
                    && self.get_block(q).is_some_and(|b| b.ignited_by_lava())
                    && self.get_block(above) == Some(Block::AIR)
                    && !self.light_portal(above)
                {
                    self.edit(above, Block::FIRE, false);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::IVec2;

    use super::*;
    use crate::world::chunk::{CHUNK_SIZE, WORLD_HEIGHT_CHUNKS};
    use crate::world::terrain::Generator;

    const AT: IVec3 = IVec3::new(8, 145, 8);

    /// Empty loaded column with a known rainy biome; no terrain generation
    /// or mesh jobs are needed to exercise the simulation.
    fn world() -> World {
        let mut world = World::new(Arc::new(Generator::new(7)), Default::default(), 2);
        for y in 0..WORLD_HEIGHT_CHUNKS {
            world.insert_chunk(IVec3::new(0, y, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        }
        world.columns.get_mut(&IVec2::ZERO).unwrap().foliage = Some(Box::new([0; CHUNK_SIZE * CHUNK_SIZE]));
        world
    }

    fn light(world: &mut World, p: IVec3, base: Block) {
        world.edit(p - IVec3::Y, base, false);
        assert!(world.ignite(p));
    }

    #[test]
    fn ignition_needs_empty_space_and_support() {
        let mut w = world();
        assert!(!w.ignite(AT), "air has no support");
        light(&mut w, AT, Block::STONE);
        assert_eq!(w.get_block(AT), Some(Block::FIRE));
        assert_eq!(w.fire.pending.len(), 1);
        assert!(!w.ignite(AT), "already lit");
        assert!(!w.ignite(AT - IVec3::Y), "occupied");
        assert!(!w.ignite(IVec3::new(8, WORLD_HEIGHT, 8)));
        assert!(!w.ignite(IVec3::new(8, -1, 8)));
        assert!(!w.ignite(AT + IVec3::X * CHUNK_SIZE_I), "unloaded chunk");
        w.set_block(AT - IVec3::Y, Block::AIR);
        assert_eq!(w.get_block(AT), Some(Block::AIR), "support updates extinguish immediately");
        assert!(w.fire.pending.is_empty());

        w.edit(AT, Block::PLANKS, false);
        assert!(w.ignite(AT + IVec3::X), "can cling to flammable sides");
        w.set_block(AT, Block::AIR);
        assert_eq!(w.get_block(AT + IVec3::X), Some(Block::AIR));
        light(&mut w, AT, Block::GLASS);
        assert_eq!(w.get_block(AT), Some(Block::FIRE), "glass has a full top face");
    }

    #[test]
    fn ignition_fills_portal_frames_even_with_existing_fire_inside() {
        let mut w = world();
        for dx in -1..=2 {
            w.edit(AT + IVec3::new(dx, -1, 0), Block::OBSIDIAN, false);
            w.edit(AT + IVec3::new(dx, 3, 0), Block::OBSIDIAN, false);
        }
        for y in 0..3 {
            for dx in [-1, 2] {
                w.edit(AT + IVec3::new(dx, y, 0), Block::OBSIDIAN, false);
            }
        }
        w.edit(AT, Block::fire(8), false);
        assert!(w.ignite(AT));
        for y in 0..3 {
            for dx in 0..2 {
                assert_eq!(w.get_block(AT + IVec3::new(dx, y, 0)), Some(Block::NETHER_PORTAL));
            }
        }
        assert!(w.fire.pending.is_empty());
    }

    #[test]
    fn netherrack_burns_forever_but_stone_burns_out() {
        let mut w = world();
        let stone = AT + IVec3::X * 8;
        light(&mut w, AT, Block::NETHERRACK);
        light(&mut w, stone, Block::STONE);
        for _ in 0..200 {
            w.random_tick(AT);
            w.random_tick(stone);
        }
        assert_eq!(w.get_block(AT), Some(Block::fire(15)));
        assert_eq!(w.get_block(stone), Some(Block::AIR));
        assert!(w.drops.is_empty());
        w.raining = true;
        assert!(w.rains_on(AT));
        for _ in 0..100 {
            w.random_tick(AT);
        }
        assert_eq!(w.get_block(AT), Some(Block::fire(15)), "even rain leaves netherrack lit");
    }

    #[test]
    fn rain_reaches_fire_and_adjacent_cells_but_not_roofs_or_dry_biomes() {
        let mut w = world();
        light(&mut w, AT, Block::STONE);
        w.raining = true;
        // A roof over just the fire doesn't shelter the adjacent cells.
        w.edit(AT + IVec3::Y * 2, Block::STONE, false);
        assert!(!w.rains_on(AT));
        assert!(w.rain_near_fire(AT));
        for d in HORIZONTAL {
            w.edit(AT + d + IVec3::Y * 2, Block::STONE, false);
        }
        assert!(!w.rain_near_fire(AT));
        for d in std::iter::once(IVec3::ZERO).chain(HORIZONTAL) {
            w.edit(AT + d + IVec3::Y * 2, Block::AIR, false);
        }
        for _ in 0..20 {
            w.random_tick(AT);
        }
        assert_eq!(w.get_block(AT), Some(Block::AIR));
        for group in [2, 4] {
            w.columns.get_mut(&IVec2::ZERO).unwrap().foliage = Some(Box::new([group; CHUNK_SIZE * CHUNK_SIZE]));
            assert!(!w.rains_on(AT), "dry/snowy biome {group}");
        }
        w.columns.get_mut(&IVec2::ZERO).unwrap().foliage = Some(Box::new([0; CHUNK_SIZE * CHUNK_SIZE]));
        let high = AT.with_y(170);
        w.edit(high - IVec3::Y, Block::STONE, false);
        assert!(!w.rains_on(high), "snow on high terrain");
        // Rain is a per-column effect like the rendered weather sheets;
        // flying high above a low plain doesn't turn its rain into snow.
        assert!(w.rains_on(high + IVec3::X));
    }

    #[test]
    fn fire_consumes_fuel_without_loot_and_primes_tnt_with_a_full_fuse() {
        let mut w = world();
        light(&mut w, AT, Block::NETHERRACK);
        for (d, b) in [(IVec3::X, Block::TNT), (IVec3::Z, Block::WOOL), (IVec3::NEG_X, Block::LEAVES)] {
            w.edit(AT + d, b, false);
        }
        w.edit(AT + IVec3::Y, Block::STONE, false);
        for _ in 0..100 {
            w.random_tick(AT);
        }
        assert_ne!(w.get_block(AT + IVec3::Z), Some(Block::WOOL));
        assert_ne!(w.get_block(AT - IVec3::X), Some(Block::LEAVES));
        assert_eq!(w.get_block(AT + IVec3::Y), Some(Block::STONE));
        assert_eq!(w.primed_tnt, vec![(AT + IVec3::X, false)]);
        assert!(w.drops.is_empty());
    }

    #[test]
    fn fire_spreads_upward_into_air_beside_fuel() {
        let mut w = world();
        light(&mut w, AT, Block::NETHERRACK);
        let destination = AT + IVec3::new(1, 1, 1);
        w.edit(destination + IVec3::Y, Block::WOOL, false);
        let mut spread = false;
        for _ in 0..400 {
            w.random_tick(AT);
            spread |= w.get_block(destination).is_some_and(|b| b.is_fire());
        }
        assert!(spread, "fire leaps to nearby fuel without consuming it first");
    }

    #[test]
    fn lava_random_ticks_ignite_nearby_wood_and_respect_solid_roofs() {
        let mut w = world();
        for dz in -1..=1 {
            for dx in -1..=1 {
                w.edit(AT + IVec3::new(dx, 0, dz), Block::PLANKS, false);
            }
        }
        w.edit(AT, Block::LAVA, false);
        for _ in 0..40 {
            w.random_tick(AT);
        }
        assert!(
            (-1..=1)
                .any(|dz| { (-1..=1).any(|dx| w.get_block(AT + IVec3::new(dx, 1, dz)).is_some_and(|b| b.is_fire())) })
        );
        let enclosed = AT + IVec3::X * 8;
        for dz in -1..=1 {
            for dx in -1..=1 {
                w.edit(enclosed + IVec3::new(dx, 0, dz), Block::PLANKS, false);
                w.edit(enclosed + IVec3::new(dx, 1, dz), Block::STONE, false);
            }
        }
        w.edit(enclosed, Block::LAVA, false);
        for _ in 0..40 {
            w.random_tick(enclosed);
        }
        assert!((-2..=2).all(|dz| {
            (-2..=2).all(|dx| {
                !(1..=2).any(|dy| w.get_block(enclosed + IVec3::new(dx, dy, dz)).is_some_and(|b| b.is_fire()))
            })
        }));
    }

    #[test]
    fn water_extinguishes_fire_without_dropping_it() {
        let mut w = world();
        for dz in -2..=2 {
            for dx in -2..=2 {
                w.edit(AT + IVec3::new(dx, -1, dz), Block::STONE, false);
            }
        }
        assert!(w.ignite(AT));
        w.set_block(AT + IVec3::Y, Block::WATER);
        for _ in 0..8 {
            w.tick_fluids(0.25);
        }
        assert!(w.get_block(AT).unwrap().is_water());
        assert!(!w.fire.pending.contains_key(&AT));
        assert!(w.drops.is_empty());
    }

    #[test]
    fn scheduled_fire_pauses_at_distance_and_resumes_after_chunk_reload() {
        let mut w = world();
        light(&mut w, AT, Block::NETHERRACK);
        let due = w.fire.pending[&AT];
        assert!((30..=39).contains(&due));
        w.tick_fire(1.49, AT.as_dvec3());
        assert_eq!(w.fire.pending[&AT], due);
        w.tick_fire(0.51, AT.as_dvec3());
        assert!(w.fire.pending[&AT] > due);
        w.edit(AT, Block::fire(9), false);
        w.tick_fire(20.0, (AT + IVec3::X * 256).as_dvec3());
        assert_eq!(w.get_block(AT), Some(Block::fire(9)));

        let cpos = chunk_of(AT);
        w.remove_chunk(cpos);
        w.tick_fire(2.0, AT.as_dvec3());
        assert!(!w.fire.pending.contains_key(&AT));
        let data = w.saved.remove(&cpos).unwrap();
        w.insert_chunk(cpos, data, true);
        assert_eq!(w.get_block(AT), Some(Block::fire(9)));
        assert!(w.fire.pending.contains_key(&AT), "saved fire reschedules on load");
        let deadline = w.fire.pending[&AT];
        w.tick_fire(2.0, AT.as_dvec3());
        assert!(w.fire.pending[&AT] > deadline);
        let overdue = w.fire.pending[&AT];
        w.fire.tick = overdue;
        w.tick_fire(0.0, AT.as_dvec3());
        assert_eq!(w.fire.pending[&AT], overdue, "menus pause even an overdue update backlog");
        w.tick_fire(0.05, AT.as_dvec3());
        assert!(w.fire.pending[&AT] > overdue);
    }

    #[test]
    fn age_changes_preserve_mesh_versions_and_copy_on_write_snapshots() {
        let mut w = world();
        light(&mut w, AT, Block::NETHERRACK);
        let cpos = chunk_of(AT);
        let slot = &w.chunks[&cpos];
        let version = slot.version;
        let snapshot = slot.data.clone();
        w.edit(AT, Block::fire(15), false);
        assert_eq!(w.chunks[&cpos].version, version);
        assert_eq!(snapshot.get(8, 17, 8), Block::FIRE);
        assert_eq!(w.get_block(AT), Some(Block::fire(15)));
        assert!(w.chunks[&cpos].modified);
    }
}
