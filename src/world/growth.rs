//! Things that grow and decay on their own, driven by Minecraft-style random
//! block ticks: wheat and nether wart ripen, saplings become trees, grass
//! spreads over lit dirt (and dies under cover), farmland gets wet near
//! water or dries back to dirt. Leaves cut off from their tree's logs decay
//! a few seconds after the last log near them goes.

use glam::{DVec3, IVec3};

use super::World;
use super::block::Block;
use super::chunk::{CHUNK_SIZE_I, WORLD_HEIGHT_CHUNKS, chunk_of};
use super::noise::splitmix64;
use crate::inventory::Stack;
use crate::item::Item;

/// Random block ticks per chunk per second: Minecraft's 3 per 16³ section
/// per game tick, so each block is picked about once every 68 seconds.
const TICKS_PER_CHUNK: f64 = 480.0;
/// Chunks within this many chunks (horizontally) of the player get random
/// ticks: Minecraft's default simulation distance of 128 blocks.
pub(super) const TICK_RADIUS: i32 = 4;
/// Leaves farther than this (in steps through leaves) from a log decay.
const LEAF_REACH: i32 = 6;

/// One in this many random ticks grows a crop on wet farmland (twice as
/// many on dry), roughly Minecraft's rate for a lone crop.
const CROP_GROWTH: u64 = 7;
/// One in this many random ticks ages nether wart (Java's rate).
const WART_GROWTH: u64 = 10;
/// One in this many random ticks grows a sapling into a tree.
const SAPLING_GROWTH: u64 = 7;

impl World {
    pub(super) fn roll(&mut self) -> u64 {
        splitmix64(&mut self.rng)
    }

    /// Uniform in `0..=n`, without a roll for 0 (fortune bonuses).
    fn up_to(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.roll() % (n + 1) }
    }

    /// True with probability `1 / n`.
    pub(super) fn one_in(&mut self, n: u64) -> bool {
        self.roll().is_multiple_of(n)
    }

    /// Queues what `block`, gone from `p`, drops: its usual item, plus
    /// Minecraft's chance drops (seeds from grass and ripe wheat, saplings
    /// and apples from leaves).
    pub fn spill_block(&mut self, p: IVec3, block: Block) {
        self.spill_mined(p, block, crate::enchant::Enchants::NONE);
    }

    /// [`World::spill_block`] for a block mined with a tool enchanted with
    /// `tool`: silk touch drops the block itself, fortune adds to ore and
    /// crop drops (Java's loot tables).
    pub fn spill_with_item(&mut self, p: IVec3, block: Block, held: Option<Stack>) {
        if self.tile_drops
            && held.is_some_and(|s| s.item == Item::SHEARS)
            && (block.is_leaves() || block == Block::COBWEB)
        {
            self.drops.push((p, Stack::new(block.base(), 1)));
        } else {
            self.spill_mined(p, block, held.map_or(Default::default(), |s| s.active_enchants()));
        }
    }

    pub fn spill_mined(&mut self, p: IVec3, block: Block, tool: crate::enchant::Enchants) {
        if !self.tile_drops {
            return;
        }
        use crate::enchant::Enchantment;
        if tool.has(Enchantment::SilkTouch)
            && let Some(item) = crate::mining::silk_drop(block)
        {
            self.drops.push((p, Stack::new(item, 1)));
            return;
        }
        let fortune = tool.level(Enchantment::Fortune) as u64;
        let mut out = Vec::new();
        out.extend(block.drop().map(|item| Stack::new(item, 1)));
        match block.as_stone_ore() {
            // Java's ore bonus: the drop times 1 + max(0, rand(fortune + 2) - 1).
            Block::COAL_ORE | Block::IRON_ORE | Block::GOLD_ORE | Block::DIAMOND_ORE | Block::EMERALD_ORE
                if fortune > 0 =>
            {
                let times = 1 + (self.roll() % (fortune + 2)).saturating_sub(1) as u8;
                if let Some(s) = out.first_mut() {
                    s.count = times;
                }
            }
            b if b.as_crop() == Some((crate::world::block::Crop::Wheat, 7)) => {
                // One seed, plus three tries (and one more per fortune
                // level) at 4 in 7.
                let seeds = 1 + (0..3 + fortune).filter(|_| self.roll() % 7 < 4).count() as u8;
                out.push(Stack::new(Item::WHEAT_SEEDS, seeds));
            }
            b if matches!(
                b.as_crop(),
                Some((crate::world::block::Crop::Carrot | crate::world::block::Crop::Potato, 7))
            ) =>
            {
                // Java's crop bonus: one item, plus three (plus fortune) tries at 4 in 7.
                let extra = (0..3 + fortune).filter(|_| self.roll() % 7 < 4).count() as u8;
                if let Some(stack) = out.first_mut() {
                    stack.count = stack.count.saturating_add(extra);
                }
                if b.as_crop().is_some_and(|(crop, _)| crop == crate::world::block::Crop::Potato) && self.one_in(50) {
                    out.push(Stack::new(Item::POISONOUS_POTATO, 1));
                }
            }
            // Ripe wart drops 2-4 in all, plus 0..fortune.
            b if b.wart_age() == Some(3) => {
                let n = 1 + self.roll() % 3 + self.up_to(fortune);
                out.push(Stack::new(Item::NETHER_WART, n as u8));
            }
            // A seed one time in eight, plus 0..2 per fortune level.
            Block::TALL_GRASS | Block::FERN if self.one_in(8) => {
                let n = 1 + self.up_to(2 * fortune);
                out.push(Stack::new(Item::WHEAT_SEEDS, n as u8))
            }
            // Copper: 2-5 raw copper, times Java's ore bonus.
            Block::COPPER_ORE => {
                let times = 1 + (self.roll() % (fortune + 2)).saturating_sub(1);
                let n = ((2 + self.roll() % 4) * times).min(64);
                out.push(Stack::new(Item::RAW_COPPER, n as u8));
            }
            // Redstone: 4-5, plus a uniform 0..=fortune (Java's uniform_bonus_count).
            Block::REDSTONE_ORE => {
                let n = (4 + self.roll() % 2 + self.up_to(fortune)).min(64);
                out.push(Stack::new(Item::REDSTONE, n as u8));
            }
            // Lapis: 4-9, times Java's ore bonus with fortune.
            Block::LAPIS_ORE => {
                let times = 1 + (self.roll() % (fortune + 2)).saturating_sub(1);
                let n = ((4 + self.roll() % 6) * times).min(64);
                out.push(Stack::new(Item::LAPIS_LAZULI, n as u8));
            }
            Block::CLAY => out.push(Stack::new(Item::CLAY_BALL, 3)),
            Block::SNOW => out.push(Stack::new(Item::SNOWBALL, 4)),
            Block::BOOKSHELF => out.push(Stack::new(Item::BOOK, 3)),
            // 2-4 dust, plus 0..fortune, at most 4.
            Block::GLOWSTONE => {
                let n = (2 + self.roll() % 3 + self.up_to(fortune)).min(4);
                out.push(Stack::new(Item::GLOWSTONE_DUST, n as u8));
            }
            // Gravel sometimes gives flint instead of itself (10%, 14%,
            // 25%, then always with fortune).
            Block::GRAVEL if self.one_in([10, 7, 4, 1][fortune.min(3) as usize]) => {
                out = vec![Stack::new(Item::FLINT, 1)]
            }
            // 3-7 slices in all, plus 0..fortune, at most 9.
            Block::MELON => {
                let n = (3 + self.roll() % 5 + self.up_to(fortune)).min(9) - 1;
                out.push(Stack::new(Item::MELON_SLICE, n as u8));
            }
            b if b.is_leaves() => {
                // Jungle leaves drop saplings less often, like Minecraft;
                // fortune raises both chances.
                let f = fortune.min(3) as usize;
                let odds = if b == Block::JUNGLE_LEAVES { [40, 36, 32, 24][f] } else { [20, 16, 12, 10][f] };
                if self.one_in(odds)
                    && let Some(wood) = b.wood()
                {
                    out.push(Stack::new(wood.sapling(), 1));
                }
                if block == Block::LEAVES && self.one_in([200, 180, 160, 120][f]) {
                    out.push(Stack::new(Item::APPLE, 1));
                }
            }
            _ => {}
        }
        self.drops.extend(out.into_iter().map(|s| (p, s)));
    }

    /// Runs random block ticks in the chunks around `player`.
    pub fn tick_random(&mut self, dt: f64, player: DVec3) {
        self.tick_random_speed(dt, player, 3);
    }

    /// Random ticks at Java's `randomTickSpeed` (three by default).
    pub fn tick_random_speed(&mut self, dt: f64, player: DVec3, speed: u32) {
        self.tick_random_rules(dt, player, speed, true);
    }

    /// Random block ticks with Java's `doFireTick` controlling fire and lava
    /// ignition while crops and other blocks continue ticking.
    pub fn tick_random_rules(&mut self, dt: f64, player: DVec3, speed: u32, fire_tick: bool) {
        self.random_ticks += dt * TICKS_PER_CHUNK * speed as f64 / 3.0;
        let n = self.random_ticks as u32;
        self.random_ticks -= n as f64;
        if n == 0 {
            return;
        }
        let mut centers = self.agent_centers.clone();
        centers.push(chunk_of(player.floor().as_ivec3()));
        // Visit the union once: overlapping sessions never accelerate random ticks.
        let mut chunks = Vec::new();
        for c in centers {
            for dz in -TICK_RADIUS..=TICK_RADIUS {
                for dx in -TICK_RADIUS..=TICK_RADIUS {
                    for y in 0..WORLD_HEIGHT_CHUNKS {
                        let p = IVec3::new(c.x + dx, y, c.z + dz);
                        if self.chunks.contains_key(&p) {
                            chunks.push(p);
                        }
                    }
                }
            }
        }
        chunks.sort_unstable_by_key(|p| (p.x, p.y, p.z));
        chunks.dedup();
        for cpos in chunks {
            if self.chunks.get(&cpos).is_none_or(|s| s.data.uniform().is_some_and(|b| !b.is_lava() && !b.is_fire())) {
                continue;
            }
            for _ in 0..n {
                let r = self.roll();
                let l = IVec3::new((r & 31) as i32, (r >> 5 & 31) as i32, (r >> 10 & 31) as i32);
                self.random_tick_rules(cpos * CHUNK_SIZE_I + l, fire_tick);
            }
        }
    }

    pub(super) fn random_tick(&mut self, p: IVec3) {
        self.random_tick_rules(p, true);
    }

    fn random_tick_rules(&mut self, p: IVec3, fire_tick: bool) {
        let Some(b) = self.get_block(p) else { return };
        match b {
            b if fire_tick && b.is_fire() => self.tick_fire_block(p, b.fire_age().unwrap()),
            b if fire_tick && b.is_lava() => self.tick_lava_fire(p),
            Block::GRASS => self.tick_grass(p),
            Block::FARMLAND | Block::WET_FARMLAND => self.tick_farmland(p, b),
            b if b.is_sapling() => {
                if self.grows_here(p) && self.one_in(SAPLING_GROWTH) {
                    self.grow_tree(p);
                }
            }
            Block::SUGAR_CANE => self.tick_cane(p),
            b if b.is_mushroom() => self.tick_mushroom(p, b),
            b if b.crop_stage().is_some_and(|s| s < 7) => {
                let wet = self.get_block(p - IVec3::Y) == Some(Block::WET_FARMLAND);
                if self.grows_here(p) && self.one_in(if wet { CROP_GROWTH } else { 2 * CROP_GROWTH }) {
                    let (crop, stage) = b.as_crop().unwrap();
                    self.edit(p, Block::crop(crop, stage + 1), false);
                }
            }
            // Java: one in ten random ticks, whatever the light.
            b if b.wart_age().is_some_and(|a| a < 3) && self.one_in(WART_GROWTH) => {
                self.edit(p, Block::nether_wart(b.wart_age().unwrap() + 1), false);
            }
            _ => {}
        }
    }

    /// Java crops need light >=9 in their cell; saplings sample above it.
    /// Preserve the current open-sky approximation for skylight.
    fn grows_here(&mut self, p: IVec3) -> bool {
        self.update_block_light();
        let light_at = if self.get_block(p).is_some_and(Block::is_sapling) { p + IVec3::Y } else { p };
        self.sky_exposed(light_at) || self.block_light(light_at) >= 9
    }

    /// Grass dies under opaque blocks and spreads to lit dirt nearby (one
    /// block across, three down or one up).
    fn tick_grass(&mut self, p: IVec3) {
        let covered = |w: &World, q: IVec3| w.get_block(q + IVec3::Y).is_some_and(|a| a.is_opaque() || a.is_fluid());
        if covered(self, p) {
            self.edit(p, Block::DIRT, false);
            return;
        }
        let r = self.roll();
        let q = p + IVec3::new((r % 3) as i32 - 1, (r / 3 % 5) as i32 - 3, (r / 15 % 3) as i32 - 1);
        if self.get_block(q) == Some(Block::DIRT) && !covered(self, q) && self.sky_exposed(q + IVec3::Y) {
            self.edit(q, Block::GRASS, false);
        }
    }

    /// Farmland is wet with water within 4 blocks across (at its level or
    /// one up); dry farmland with no crop on it turns back to dirt.
    fn tick_farmland(&mut self, p: IVec3, b: Block) {
        let above = self.get_block(p + IVec3::Y);
        if above.is_some_and(|a| a.is_solid()) {
            self.edit(p, Block::DIRT, false);
            return;
        }
        let wet = self.rains_on(p + IVec3::Y)
            || (-4..=4).any(|dx| {
                (-4..=4).any(|dz| {
                    (0..=1).any(|dy| self.get_block(p + IVec3::new(dx, dy, dz)).is_some_and(|w| w.is_water()))
                })
            });
        let crop = above.is_some_and(|a| a.crop_stage().is_some());
        match (wet, b) {
            (true, Block::FARMLAND) => {
                self.edit(p, Block::WET_FARMLAND, false);
            }
            (false, Block::WET_FARMLAND) => {
                self.edit(p, Block::FARMLAND, false);
            }
            (false, _) if !crop => {
                self.edit(p, Block::DIRT, false);
            }
            _ => {}
        }
    }

    /// Grows the sapling at `p` into a tree if there's room for its trunk.
    /// Leaves only fill air, like world generation. Returns whether it grew.
    pub fn grow_tree(&mut self, p: IVec3) -> bool {
        let Some(sapling) = self.get_block(p) else { return false };
        let v = self.roll() as u32;
        let mut blocks = Vec::new();
        let ground = p - IVec3::Y;
        let Some(wood) = sapling.wood().filter(|_| sapling.is_sapling()) else { return false };
        super::terrain::tree(wood, ground, v, &mut |q, b| blocks.push((q, b)));
        let room = blocks.iter().filter(|(_, b)| b.is_log()).all(|&(q, _)| {
            q == p || self.get_block(q).is_some_and(|b| b == Block::AIR || b.is_replaceable() || b.is_leaves())
        });
        if !room {
            return false;
        }
        for (q, b) in blocks {
            let free = self.get_block(q).is_some_and(|cur| cur == Block::AIR || (b.is_log() && !cur.is_log()));
            if free {
                self.edit(q, b, false);
            }
        }
        if self.get_block(ground) == Some(Block::GRASS) || self.get_block(ground).is_some_and(Block::is_farmland) {
            self.edit(ground, Block::DIRT, false);
        }
        true
    }

    /// Bone meal on the block at `p`: crops grow 2-5 stages, saplings have a
    /// 45% chance to grow, and grass sprouts tall grass and flowers around.
    /// Returns whether the bone meal was used.
    pub fn apply_bone_meal(&mut self, p: IVec3) -> bool {
        let Some(b) = self.get_block(p) else { return false };
        match b {
            b if b.crop_stage().is_some_and(|s| s < 7) => {
                let (crop, stage) = b.as_crop().unwrap();
                let stage = (stage + 2 + (self.roll() % 4) as u8).min(7);
                self.edit(p, Block::crop(crop, stage), true);
                true
            }
            b if b.is_sapling() => {
                if self.roll() % 100 < 45 {
                    self.grow_tree(p);
                }
                true
            }
            Block::GRASS if self.get_block(p + IVec3::Y) == Some(Block::AIR) => {
                for _ in 0..16 {
                    let r = self.roll();
                    let q = p + IVec3::new((r % 7) as i32 - 3, (r / 7 % 3) as i32 - 1, (r / 21 % 7) as i32 - 3);
                    if self.get_block(q) == Some(Block::GRASS) && self.get_block(q + IVec3::Y) == Some(Block::AIR) {
                        let plant = match r / 147 % 10 {
                            0 => Block::DANDELION,
                            1 => Block::POPPY,
                            _ => Block::TALL_GRASS,
                        };
                        self.edit(q + IVec3::Y, plant, true);
                    }
                }
                true
            }
            _ => false,
        }
    }

    /// A log at `p` is gone: leaves around it that no longer reach a log
    /// will decay after a short random delay.
    pub(super) fn log_removed(&mut self, p: IVec3) {
        const R: i32 = LEAF_REACH - 1;
        for dy in -R..=R {
            for dz in -R..=R {
                for dx in -R..=R {
                    let q = p + IVec3::new(dx, dy, dz);
                    if self.get_block(q).is_some_and(Block::is_leaves) && !self.leaf_decay.contains_key(&q) {
                        let delay = 1.0 + (self.roll() % 1000) as f32 / 1000.0 * 9.0;
                        self.leaf_decay.insert(q, delay);
                    }
                }
            }
        }
    }

    /// Decays leaves whose timer ran out and that still can't reach a log.
    pub fn tick_leaf_decay(&mut self, dt: f64) {
        if self.leaf_decay.is_empty() {
            return;
        }
        let mut due = Vec::new();
        self.leaf_decay.retain(|&p, t| {
            *t -= dt as f32;
            if *t <= 0.0 {
                due.push(p);
            }
            *t > 0.0
        });
        for p in due {
            match self.get_block(p) {
                Some(b) if b.is_leaves() && !self.reaches_log(p) => {
                    self.edit(p, Block::AIR, false);
                    self.spill_block(p, b);
                    self.settle(p);
                }
                // Unloaded: try again later.
                None => {
                    self.leaf_decay.insert(p, 5.0);
                }
                _ => {}
            }
        }
    }

    /// Whether a log is within [`LEAF_REACH`] steps of `p` through leaves.
    fn reaches_log(&self, p: IVec3) -> bool {
        let mut seen = rustc_hash::FxHashSet::default();
        let mut frontier = vec![p];
        seen.insert(p);
        for _ in 0..LEAF_REACH {
            let mut next = Vec::new();
            for q in frontier {
                for d in [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z] {
                    let n = q + d;
                    if !seen.insert(n) {
                        continue;
                    }
                    match self.get_block(n) {
                        None => return true,
                        Some(b) if b.is_log() => return true,
                        Some(b) if b.is_leaves() => next.push(n),
                        _ => {}
                    }
                }
            }
            frontier = next;
        }
        false
    }
}

/// Sugar cane stops growing at this height.
const CANE_HEIGHT: i32 = 3;

impl World {
    /// Sugar cane grows a block taller now and then, up to [`CANE_HEIGHT`],
    /// while water touches the soil it stands on.
    fn tick_cane(&mut self, p: IVec3) {
        if self.get_block(p + IVec3::Y) != Some(Block::AIR) || !self.one_in(4) {
            return;
        }
        let mut base = p;
        while self.get_block(base - IVec3::Y) == Some(Block::SUGAR_CANE) {
            base -= IVec3::Y;
        }
        if p.y - base.y + 1 < CANE_HEIGHT && self.cane_has_water(base) {
            self.edit(p + IVec3::Y, Block::SUGAR_CANE, false);
        }
    }

    /// Whether sugar cane at `p` (its lowest block) has water beside the
    /// block it's planted on.
    pub fn cane_has_water(&self, p: IVec3) -> bool {
        let soil = p - IVec3::Y;
        [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z]
            .iter()
            .any(|&d| self.get_block(soil + d).is_some_and(|b| b.is_water() || b == Block::ICE))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::chunk::ChunkData;
    use crate::world::terrain::Generator;
    use std::sync::Arc;

    #[test]
    fn ripe_potatoes_can_drop_a_poisonous_one() {
        let mut world = World::new_headless(Arc::new(Generator::new(11)), Default::default(), 2);
        let mut poison = 0;
        for _ in 0..400 {
            world.drops.clear();
            world.spill_block(IVec3::ZERO, Block::crop(crate::world::block::Crop::Potato, 7));
            let potatoes = world.drops.iter().find(|(_, s)| s.item == Item::POTATO).map(|(_, s)| s.count).unwrap();
            assert!((1..=4).contains(&potatoes));
            if world.drops.iter().any(|(_, s)| s.item == Item::POISONOUS_POTATO) {
                poison += 1;
            }
        }
        assert!((1..30).contains(&poison), "about 2% of 400, got {poison}");
    }

    #[test]
    fn snow_drops_four_snowballs_unless_silk_touched() {
        let mut world = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
        world.spill_block(IVec3::ZERO, Block::SNOW);
        assert_eq!(world.drops, vec![(IVec3::ZERO, Stack::new(Item::SNOWBALL, 4))]);
        world.drops.clear();
        let silk = crate::enchant::Enchants::NONE.with(crate::enchant::Enchantment::SilkTouch, 1);
        world.spill_mined(IVec3::ZERO, Block::SNOW, silk);
        assert_eq!(world.drops, vec![(IVec3::ZERO, Stack::new(Block::SNOW, 1))]);
    }

    #[test]
    fn silk_touch_gravel_never_drops_flint() {
        let mut world = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
        let silk = crate::enchant::Enchants::NONE.with(crate::enchant::Enchantment::SilkTouch, 1);
        for _ in 0..100 {
            world.spill_mined(IVec3::ZERO, Block::GRAVEL, silk);
        }
        assert_eq!(world.drops, vec![(IVec3::ZERO, Stack::new(Block::GRAVEL, 1)); 100]);
    }

    #[test]
    fn disabled_tile_drops_suppresses_block_loot() {
        let mut world = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
        world.set_tile_drops(false);
        world.spill_block(IVec3::ZERO, Block::STONE);
        assert!(world.drops.is_empty());
    }

    #[test]
    fn random_ticks_include_distant_agents_without_accelerating_overlaps() {
        fn world(centers: &[IVec3]) -> World {
            let mut world = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
            for x in [0, 10, 100] {
                let mut data = ChunkData::Uniform(Block::STONE);
                data.set(0, 0, 0, Block::AIR); // Non-uniform, with no random-tick side effects.
                world.insert_chunk(IVec3::new(x, 4, 0), Arc::new(data), false);
            }
            world.agent_centers = centers.to_vec();
            world
        }
        let far = IVec3::new(10, 4, 0);
        let mut host_only = world(&[]);
        let mut union = world(&[far]);
        let mut overlapping = world(&[IVec3::ZERO, IVec3::X, far, far, far + IVec3::X]);
        for w in [&mut host_only, &mut union, &mut overlapping] {
            w.tick_random(crate::simulation::TICK_SECONDS, DVec3::new(1.0, 150.0, 1.0));
        }
        assert_ne!(host_only.rng, union.rng, "distant agents must get random ticks");
        assert_eq!(union.rng, overlapping.rng, "overlapping regions must tick once");
    }

    #[test]
    fn covered_crops_need_nine_block_light_and_saplings_sample_above() {
        let mut world = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
        for y in 0..WORLD_HEIGHT_CHUNKS {
            world.insert_chunk(IVec3::new(0, y, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        }
        let crop = IVec3::new(15, 150, 16);
        world.set_block(crop - IVec3::Y, Block::WET_FARMLAND);
        world.set_block(crop, Block::wheat(0));
        world.set_block(crop + IVec3::Y * 2, Block::STONE);
        assert!(!world.sky_exposed(crop));
        assert!(!world.grows_here(crop));
        let near = crop - IVec3::X * 5;
        let far = crop - IVec3::X * 6;
        for p in [near, far] {
            world.set_block(p - IVec3::Y, Block::STONE);
        }
        world.set_block(far, Block::TORCH);
        assert_eq!(world.block_light(crop), 8);
        for _ in 0..100 {
            world.random_tick(crop);
        }
        assert_eq!(world.get_block(crop), Some(Block::wheat(0)));
        world.set_block(far, Block::AIR);
        world.set_block(near, Block::TORCH);
        assert_eq!(world.block_light(crop), 9);
        assert!(world.grows_here(crop));
        for _ in 0..200 {
            world.random_tick(crop);
        }
        assert_eq!(world.get_block(crop), Some(Block::wheat(7)));
        world.set_block(crop - IVec3::Y, Block::DIRT);
        world.set_block(crop, Block::OAK_SAPLING);
        assert_eq!(world.block_light(crop + IVec3::Y), 8);
        assert!(!world.grows_here(crop), "saplings require nine in the cell above");
        world.set_block(near + IVec3::Y, Block::GLOWSTONE);
        assert!(world.grows_here(crop));
        assert!(world.mesh_uploads.is_empty());
    }

    #[test]
    fn metal_ores_drop_raw_materials_and_gems() {
        let mut world = World::new_headless(Arc::new(Generator::new(3)), Default::default(), 2);
        let silk = crate::enchant::Enchants::NONE.with(crate::enchant::Enchantment::SilkTouch, 1);
        world.spill_block(IVec3::ZERO, Block::IRON_ORE);
        world.spill_block(IVec3::ZERO, Block::GOLD_ORE);
        world.spill_block(IVec3::ZERO, Block::EMERALD_ORE);
        assert_eq!(
            world.drops,
            vec![
                (IVec3::ZERO, Stack::new(Item::RAW_IRON, 1)),
                (IVec3::ZERO, Stack::new(Item::RAW_GOLD, 1)),
                (IVec3::ZERO, Stack::new(Item::EMERALD, 1)),
            ]
        );
        world.drops.clear();
        world.spill_mined(IVec3::ZERO, Block::IRON_ORE, silk);
        world.spill_mined(IVec3::ZERO, Block::COPPER_ORE, silk);
        assert_eq!(
            world.drops,
            vec![(IVec3::ZERO, Stack::new(Block::IRON_ORE, 1)), (IVec3::ZERO, Stack::new(Block::COPPER_ORE, 1)),]
        );
        world.drops.clear();
        for _ in 0..30 {
            world.spill_block(IVec3::ZERO, Block::COPPER_ORE);
            world.spill_block(IVec3::ZERO, Block::REDSTONE_ORE);
        }
        assert!(world.drops.iter().any(|(_, s)| s.item == Item::RAW_COPPER && (2..=5).contains(&s.count)));
        assert!(world.drops.iter().any(|(_, s)| s.item == Item::REDSTONE && (4..=5).contains(&s.count)));
        assert!(world.drops.iter().all(|(_, s)| {
            (s.item == Item::RAW_COPPER && (2..=5).contains(&s.count))
                || (s.item == Item::REDSTONE && (4..=5).contains(&s.count))
        }));
    }
}
