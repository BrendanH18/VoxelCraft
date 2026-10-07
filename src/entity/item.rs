//! Dropped items: stacks lying in the world, like Minecraft's item
//! entities. They fall, slide to a stop, float up in water, burn in lava/fire,
//! merge with matching stacks nearby, and despawn after five minutes in
//! loaded chunks. The game picks them up when the player walks over them.

use glam::{DVec3, IVec3};

use super::{MobWorld, Rng};
use crate::inventory::{Stack, stack_from_str, stack_to_string};
use crate::physics::{self, Shape};

/// Seconds a dropped item lasts (Minecraft's 6000 ticks).
pub const LIFETIME: f32 = 300.0;
/// Seconds before a block or loot drop can be picked up.
pub const PICKUP_DELAY: f32 = 0.5;
/// Seconds before an item the player threw can be picked back up.
pub const THROWN_PICKUP_DELAY: f32 = 2.0;
/// Blocks per second squared (Minecraft's 0.04 per tick²).
const GRAVITY: f64 = 16.0;
const SHAPE: Shape = Shape::new(0.125, 0.25);
/// Matching stacks closer than this merge.
const MERGE_DIST: f64 = 0.75;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemEntity {
    pub stack: Stack,
    /// Bottom centre.
    pub pos: DVec3,
    /// Position at the start of the last simulation step, for rendering.
    pub previous_pos: DVec3,
    pub vel: DVec3,
    /// Seconds since dropped (only counts in loaded chunks).
    pub age: f32,
    /// Seconds until it can be picked up.
    pub pickup_delay: f32,
    /// Random phase for the spin and bob.
    pub phase: f32,
}

impl ItemEntity {
    /// Create a fresh dropped stack with pickup delay in seconds and coincident interpolation positions.
    pub fn new(stack: Stack, pos: DVec3, vel: DVec3, pickup_delay: f32, rng: &mut Rng) -> Self {
        Self {
            stack,
            pos,
            previous_pos: pos,
            vel,
            age: 0.0,
            pickup_delay,
            phase: rng.range(0.0, std::f32::consts::TAU),
        }
    }

    /// Steps physics; returns `false` when the item is gone (burnt or
    /// expired).
    pub fn update<W: MobWorld + ?Sized>(&mut self, dt: f64, world: &W) -> bool {
        let dtf = dt as f32;
        self.age += dtf;
        self.pickup_delay = (self.pickup_delay - dtf).max(0.0);
        if self.age >= LIFETIME
            || (!self.stack.item.fire_resistant()
                && physics::touches_block(world, self.pos, SHAPE, |b| b.is_lava() || b.is_fire()))
        {
            return false;
        }
        if physics::is_fluid_at(world, self.pos + DVec3::Y * 0.1) {
            // Bob up to the surface and drift slowly.
            self.vel.y = (self.vel.y + 10.0 * dt).min(1.2);
            let drag = (1.0 - 3.0 * dt).max(0.0);
            self.vel.x *= drag;
            self.vel.z *= drag;
        } else {
            self.vel.y -= GRAVITY * dt;
            // Air drag: Minecraft's 0.98 per tick.
            self.vel *= 0.98f64.powf(dt * 20.0);
        }
        // Items that end up inside a block (a block placed on them, sand
        // landing) are pushed up out of it, like Minecraft.
        if physics::overlaps_solid(world, self.pos, SHAPE) {
            self.pos.y = self.pos.y.floor() + 1.0;
            self.vel = DVec3::ZERO;
            return true;
        }
        let delta = self.vel * dt;
        let hit = physics::move_box(world, &mut self.pos, &mut self.vel, delta, SHAPE);
        if hit.on_ground {
            // Ground friction: Minecraft's slipperiness 0.6 * 0.98 per tick.
            let f = (0.6f64 * 0.98).powf(dt * 20.0);
            self.vel.x *= f;
            self.vel.z *= f;
        }
        true
    }

    /// Whether the player's box (feet at `player`, 0.6 x 1.8) grown by
    /// Minecraft's pickup margin touches this item.
    pub fn touches_player(&self, player: DVec3) -> bool {
        let (min, max) = SHAPE.aabb(self.pos);
        let (pmin, pmax) = Shape::new(0.3 + 1.0, 1.8 + 0.5).aabb(player - DVec3::Y * 0.5);
        min.cmplt(pmax).all() && max.cmpgt(pmin).all()
    }

    /// `x,y,z,age,stack` for the level file.
    pub fn serialize(&self) -> String {
        let p = self.pos;
        format!("{:.3},{:.3},{:.3},{:.1},{}", p.x, p.y, p.z, self.age, stack_to_string(Some(self.stack)))
    }

    pub fn deserialize(text: &str, rng: &mut Rng) -> Option<Self> {
        let mut f = text.split(',');
        let mut num = || f.next()?.parse::<f64>().ok();
        let pos = DVec3::new(num()?, num()?, num()?);
        let age = num()? as f32;
        let stack = stack_from_str(f.next()?)??;
        let mut item = Self::new(stack, pos, DVec3::ZERO, 0.0, rng);
        item.age = age;
        Some(item)
    }
}

/// Merges matching stacks lying close together (the older item absorbs the
/// younger), as long as the result fits in one stack.
pub fn merge(items: &mut Vec<ItemEntity>) {
    let mut i = 0;
    while i < items.len() {
        let mut j = i + 1;
        while j < items.len() {
            let (a, b) = (&items[i], &items[j]);
            let fits =
                a.stack.stacks_with(&b.stack) && a.stack.count as u16 + b.stack.count as u16 <= a.stack.max() as u16;
            if fits && a.pos.distance_squared(b.pos) < MERGE_DIST * MERGE_DIST {
                let b = items.swap_remove(j);
                let a = &mut items[i];
                a.stack.count += b.stack.count;
                a.age = a.age.min(b.age);
                a.pickup_delay = a.pickup_delay.max(b.pickup_delay);
                continue;
            }
            j += 1;
        }
        i += 1;
    }
}

/// The random pop a block drop gets when it appears at the centre of `cell`.
pub fn block_drop(stack: Stack, cell: IVec3, rng: &mut Rng) -> ItemEntity {
    let pos = cell.as_dvec3() + DVec3::new(rng.range(0.25, 0.75) as f64, 0.25, rng.range(0.25, 0.75) as f64);
    let vel = DVec3::new(rng.range(-2.0, 2.0) as f64, 4.0, rng.range(-2.0, 2.0) as f64);
    ItemEntity::new(stack, pos, vel, PICKUP_DELAY, rng)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Item;
    use crate::physics::test_util::Grid;
    use crate::world::block::Block;

    fn settle(item: &mut ItemEntity, world: &Grid, secs: f64) -> bool {
        (0..(secs * 60.0) as usize).all(|_| item.update(1.0 / 60.0, world))
    }

    #[test]
    fn drops_fall_slide_to_a_stop_and_expire() {
        let world = Grid::flat(10);
        let mut rng = Rng::new(1);
        let mut item = block_drop(Stack::new(Block::DIRT, 1), IVec3::new(0, 10, 0), &mut rng);
        assert!(settle(&mut item, &world, 3.0));
        assert!((item.pos.y - 10.0).abs() < 1e-3, "rests on the floor: {:?}", item.pos);
        assert!(item.vel.length() < 0.01, "stopped: {:?}", item.vel);
        let hop = (item.pos - DVec3::new(0.5, 10.0, 0.5)).length();
        assert!(hop < 1.6, "only a small hop: {:?}", item.pos);
        assert_eq!(item.pickup_delay, 0.0);
        item.age = LIFETIME - 0.5;
        assert!(!settle(&mut item, &world, 1.0), "despawns after five minutes");
    }

    #[test]
    fn netherite_materials_survive_fire_and_lava_but_still_expire() {
        let mut world = Grid::flat(0);
        let mut rng = Rng::new(31);
        for hazard in [Block::LAVA, Block::FIRE] {
            world.set(IVec3::ZERO, hazard);
            for material in [
                Block::ANCIENT_DEBRIS.into(),
                Item::NETHERITE_SCRAP,
                Item::NETHERITE_INGOT,
                Block::NETHERITE_BLOCK.into(),
                Item::GOLD_INGOT,
            ] {
                let mut item = ItemEntity::new(Stack::new(material, 1), DVec3::splat(0.5), DVec3::ZERO, 0.0, &mut rng);
                assert_eq!(item.update(0.05, &world), material != Item::GOLD_INGOT);
                if material != Item::GOLD_INGOT {
                    item.age = LIFETIME;
                    assert!(!item.update(0.05, &world), "fire resistance does not prevent despawning");
                }
            }
        }
    }

    #[test]
    fn netherite_materials_float_to_the_lava_surface() {
        let mut world = Grid::flat(0);
        for x in -2..=2 {
            for z in -2..=2 {
                for y in 0..5 {
                    world.set(IVec3::new(x, y, z), Block::LAVA);
                }
            }
        }
        let mut rng = Rng::new(4);
        for material in
            [Block::ANCIENT_DEBRIS.into(), Item::NETHERITE_SCRAP, Item::NETHERITE_INGOT, Block::NETHERITE_BLOCK.into()]
        {
            let mut item =
                ItemEntity::new(Stack::new(material, 1), DVec3::new(0.5, 1.0, 0.5), DVec3::ZERO, 0.0, &mut rng);
            assert!(settle(&mut item, &world, 5.0));
            assert!(item.pos.y > 4.0 && item.pos.y < 5.5, "floats near the lava surface: {:?}", item.pos);
        }
    }

    #[test]
    fn items_float_burn_and_get_pushed_out_of_blocks() {
        let mut world = Grid::flat(0);
        for x in -2..=2 {
            for z in -2..=2 {
                for y in 0..5 {
                    world.set(IVec3::new(x, y, z), Block::WATER);
                }
                world.set(IVec3::new(x, 0, z + 10), Block::LAVA);
            }
        }
        let mut rng = Rng::new(2);
        let mut item =
            ItemEntity::new(Stack::new(Item::STICK, 1), DVec3::new(0.5, 1.0, 0.5), DVec3::ZERO, 0.0, &mut rng);
        settle(&mut item, &world, 3.0);
        assert!(item.pos.y > 4.0 && item.pos.y < 5.5, "floats near the surface: {:?}", item.pos);

        let mut item =
            ItemEntity::new(Stack::new(Item::STICK, 1), DVec3::new(0.5, 3.0, 10.5), DVec3::ZERO, 0.0, &mut rng);
        assert!(!settle(&mut item, &world, 2.0), "burns in lava");
        world.set(IVec3::new(0, 10, 0), crate::world::block::Block::FIRE);
        let mut item =
            ItemEntity::new(Stack::new(Item::COAL, 1), DVec3::new(0.5, 10.1, 0.5), DVec3::ZERO, 0.0, &mut rng);
        assert!(!item.update(0.05, &world), "burns in fire");

        let mut world = Grid::flat(10);
        world.set(IVec3::new(0, 10, 0), Block::STONE);
        let mut item =
            ItemEntity::new(Stack::new(Item::STICK, 1), DVec3::new(0.5, 10.2, 0.5), DVec3::ZERO, 0.0, &mut rng);
        settle(&mut item, &world, 1.0);
        assert!((item.pos.y - 11.0).abs() < 1e-3, "pushed on top: {:?}", item.pos);
    }

    #[test]
    fn matching_stacks_merge_up_to_a_full_stack() {
        let mut rng = Rng::new(3);
        let at = |stack, x: f64, rng: &mut Rng| ItemEntity::new(stack, DVec3::new(x, 10.0, 0.0), DVec3::ZERO, 0.0, rng);
        let mut items = vec![
            at(Stack::new(Block::DIRT, 40), 0.0, &mut rng),
            at(Stack::new(Block::DIRT, 20), 0.3, &mut rng),
            at(Stack::new(Block::DIRT, 10), 0.5, &mut rng),
            at(Stack::new(Block::STONE, 1), 0.2, &mut rng),
            at(Stack::new(Block::DIRT, 1), 5.0, &mut rng),
        ];
        merge(&mut items);
        let mut counts: Vec<(Item, u8)> = items.iter().map(|i| (i.stack.item, i.stack.count)).collect();
        counts.sort_by_key(|&(i, n)| (i.0, n));
        let dirt = Item::from_block(Block::DIRT);
        // 40 + 20 = 60; the 10 doesn't fit on top, the far one stays apart.
        assert_eq!(counts, [(Item::from_block(Block::STONE), 1), (dirt, 1), (dirt, 10), (dirt, 60)]);
    }

    #[test]
    fn pickup_range_and_saving() {
        let mut rng = Rng::new(4);
        let item = ItemEntity::new(Stack::new(Item::COAL, 3), DVec3::new(1.2, 10.0, 0.0), DVec3::ZERO, 0.0, &mut rng);
        assert!(item.touches_player(DVec3::new(0.0, 10.0, 0.0)));
        assert!(item.touches_player(DVec3::new(0.0, 10.5, 0.0)), "half a block above");
        assert!(!item.touches_player(DVec3::new(0.0, 11.0, 0.0)), "a full block above");
        assert!(!item.touches_player(DVec3::new(-1.5, 10.0, 0.0)));
        assert!(!item.touches_player(DVec3::new(0.0, 13.0, 0.0)));

        let text = item.serialize();
        let back = ItemEntity::deserialize(&text, &mut rng).unwrap();
        assert_eq!((back.stack, back.age), (item.stack, item.age));
        assert!(back.pos.distance(item.pos) < 1e-3);
        assert!(ItemEntity::deserialize("1,2,3", &mut rng).is_none());
    }
}
