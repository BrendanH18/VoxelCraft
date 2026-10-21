//! Things that grow and decay on their own, driven by Minecraft-style random
//! block ticks: wheat and nether wart ripen, saplings become trees, grass
//! spreads over lit dirt (and dies under cover), farmland gets wet near
//! water or dries back to dirt. Leaves cut off from their tree's logs decay
//! a few seconds after the last log near them goes.

use glam::{DVec3, IVec3};

use super::World;
use super::block::Block;
use super::chunk::{CHUNK_BITS, CHUNK_SIZE_I, chunk_of};
use super::noise::splitmix64;
use crate::inventory::Stack;
use crate::item::Item;

/// Random block ticks per chunk per second: Minecraft's 3 per 16³ section
/// per game tick, so each block is picked about once every 68 seconds.
const TICKS_PER_SECTION: f64 = 60.0;
/// Leaves farther than this (in steps through leaves) from a log decay.
const LEAF_REACH: i32 = 6;

/// One in this many random ticks grows a crop on wet farmland (twice as
/// many on dry), roughly Minecraft's rate for a lone crop.
const CROP_GROWTH: u64 = 7;
/// One in this many random ticks ages nether wart (Java's rate).
const WART_GROWTH: u64 = 10;
/// One in this many random ticks grows a sapling into a tree.
const SAPLING_GROWTH: u64 = 7;

/// Java 26.3 `TreeGrower.DARK_OAK` and `PALE_OAK` have only a 2x2 (mega)
/// feature, so a lone sapling of either can never grow.
fn needs_two_by_two(sapling: Block) -> bool {
    let sapling = sapling.base();
    sapling == super::overworld_blocks::DARK_OAK_SAPLING || sapling == super::overworld_blocks::PALE_OAK_SAPLING
}

impl World {
    pub(super) fn roll(&mut self) -> u64 {
        splitmix64(&mut self.rng)
    }

    /// Uniform in `0..=n`, without a roll for 0 (fortune bonuses).
    pub(super) fn up_to(&mut self, n: u64) -> u64 {
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
        let block = block.canonical();
        if self.tile_drops
            && held.is_some_and(|s| s.item == Item::SHEARS)
            && let Some(stack) = Self::shear_loot(block)
        {
            self.drops.push((p, stack));
        } else {
            self.spill_mined(p, block, held.map_or(Default::default(), |s| s.active_enchants()));
        }
    }

    /// What Java's loot tables give `block` under `tool/can_shear` instead of
    /// the usual roll: the plant or leaf itself, and two for the tall grass,
    /// large fern and tall seagrass pools (`tests/fixtures/java/26.3/mining.json`).
    fn shear_loot(block: Block) -> Option<Stack> {
        let count = if matches!(block.0, 1963 | 1964 | 2049..=2052) { 2 } else { 1 };
        let item = match block {
            Block::TALL_GRASS | Block::FERN | Block::DEAD_BUSH | Block::COBWEB => block.into(),
            b if b.is_leaves() => b.base().into(),
            b => super::nether_biome_blocks::sheared_drop(b).or_else(|| super::overworld_blocks::sheared_drop(b))?,
        };
        Some(Stack::new(item, count))
    }

    pub fn spill_mined(&mut self, p: IVec3, block: Block, tool: crate::enchant::Enchants) {
        if !self.tile_drops {
            return;
        }
        // A waterlogged block drops what its dry original does.
        let block = block.canonical();
        use crate::enchant::Enchantment;
        if tool.has(Enchantment::SilkTouch)
            && let Some(item) = crate::mining::silk_drop(block)
        {
            // A silk-touched snow block keeps every layer; a double slab is two slabs.
            let count = super::overworld_blocks::snow_layers(block).unwrap_or(1) * double_slab_count(block);
            self.drops.push((p, Stack::new(item, count)));
            return;
        }
        let fortune = tool.level(Enchantment::Fortune) as u64;
        let mut out = Vec::new();
        // Java's slab loot table: two for `type=double`.
        out.extend(block.drop().map(|item| Stack::new(item, double_slab_count(block))));
        if (super::overworld_blocks::FIRST..=super::overworld_blocks::LAST).contains(&block.0)
            || super::overworld_blocks::snow_layers(block).is_some()
            || super::overworld_blocks::bud_facing(block).is_some()
        {
            self.overworld_drops(block, fortune, &mut out);
        }
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
            b if let Some((item, chance)) = super::nether_biome_blocks::vine_drop_chance(b, fortune as u8) => {
                if (self.roll() % 1_000_000) as f32 / 1_000_000.0 < chance {
                    out.push(Stack::new(item, 1));
                }
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
            Block::GILDED_BLACKSTONE if self.one_in([10, 7, 4, 1][fortune.min(3) as usize]) => {
                out = vec![Stack::new(Item::GOLD_NUGGET, (2 + self.roll() % 4) as u8)];
            }
            Block::GRAVEL if self.one_in([10, 7, 4, 1][fortune.min(3) as usize]) => {
                out = vec![Stack::new(Item::FLINT, 1)]
            }
            // 3-7 slices in all, plus 0..fortune, at most 9.
            Block::MELON => {
                let n = (3 + self.roll() % 5 + self.up_to(fortune)).min(9) - 1;
                out.push(Stack::new(Item::MELON_SLICE, n as u8));
            }
            // The Overworld module's leaves roll their own drops above.
            b if b.is_leaves() && !(super::overworld_blocks::FIRST..=super::overworld_blocks::LAST).contains(&b.0) => {
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
        self.simulation_player(player);
        self.random_ticks += dt * TICKS_PER_SECTION * speed as f64 / 3.0;
        let n = self.random_ticks as u32;
        self.random_ticks -= n as f64;
        if n == 0 {
            return;
        }
        // A storage chunk contains eight Java 16³ sections. Tick only the
        // sections whose horizontal Java chunk is block-ticking; the boundary
        // may split a storage chunk in half.
        let mut chunks: Vec<_> = match self.activity.block_columns() {
            // Only storage columns that reach the block-ticking border, not every loaded chunk.
            Some(bounds) => {
                let lo_y = self.generator.dimension.min_y() >> CHUNK_BITS;
                let hi_y = lo_y + self.generator.dimension.column_chunks();
                let mut keys = Vec::new();
                for (lo, hi) in bounds {
                    for x in lo.x..=hi.x {
                        for z in lo.y..=hi.y {
                            keys.extend(
                                (lo_y..hi_y).map(|y| IVec3::new(x, y, z)).filter(|p| self.chunks.contains_key(p)),
                            );
                        }
                    }
                }
                keys
            }
            None => self.chunks.keys().copied().collect(),
        };
        chunks.sort_unstable_by_key(|p| (p.x, p.y, p.z));
        chunks.dedup();
        for cpos in chunks {
            if self
                .chunks
                .get(&cpos)
                .is_none_or(|s| s.data.uniform().is_some_and(|b| !b.is_lava() && b.fire_age().is_none()))
            {
                continue;
            }
            for sy in 0..2 {
                for sz in 0..2 {
                    for sx in 0..2 {
                        let base = cpos * CHUNK_SIZE_I + IVec3::new(sx, sy, sz) * 16;
                        if !self.activity.contains(base, true) {
                            continue;
                        }
                        for _ in 0..n {
                            let r = self.roll();
                            let l = IVec3::new((r & 15) as i32, (r >> 4 & 15) as i32, (r >> 8 & 15) as i32);
                            self.random_tick_rules(base + l, fire_tick);
                        }
                    }
                }
            }
        }
    }

    pub(super) fn random_tick(&mut self, p: IVec3) {
        self.random_tick_rules(p, true);
    }

    fn random_tick_rules(&mut self, p: IVec3, fire_tick: bool) {
        let Some(b) = self.get_block(p) else { return };
        match b {
            b if fire_tick && b.fire_age().is_some() && self.fire_may_spread(p) => {
                self.tick_fire_block(p, b.fire_age().unwrap())
            }
            b if fire_tick && b.is_lava() && self.fire_may_spread(p) => self.tick_lava_fire(p),
            Block::GRASS => self.tick_grass(p),
            Block::FARMLAND | Block::WET_FARMLAND => self.tick_farmland(p, b),
            b if b.is_sapling() => {
                if self.grows_here(p) && self.one_in(SAPLING_GROWTH) {
                    self.advance_tree(p);
                }
            }
            // `SnowLayerBlock.randomTick`: block light above 11 melts it, with drops.
            b if super::overworld_blocks::snow_layers(b).is_some() => {
                self.update_block_light();
                if self.block_light(p) > 11 {
                    self.spill_block(p, b);
                    self.edit(p, Block::AIR, true);
                }
            }
            // `RedStoneOreBlock.randomTick`: a lit ore goes dark.
            b if matches!(
                super::redstone_blocks::component(b),
                Some(super::redstone_blocks::Component::GlowingOre(_))
            ) =>
            {
                self.extinguish_ore(p, b);
            }
            Block::SUGAR_CANE => self.tick_cane(p),
            b if b.is_mushroom() => self.tick_mushroom(p, b),
            b if (1600..=1687).contains(&b.0) => self.tick_nether_flora(p, b),
            b if (super::overworld_blocks::FIRST..=super::overworld_blocks::LAST).contains(&b.canonical().0)
                || (2150..=2157).contains(&b.0)
                || (2160..=2164).contains(&b.canonical().0) =>
            {
                self.tick_overworld_flora(p, b);
            }
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

    /// Brightness for growth rules on the gameplay light arrays (block light
    /// and propagated skylight), not the heightmap. `darkened` selects Java's
    /// `getMaxLocalRawBrightness(p)`, which subtracts the time-of-day and
    /// weather `skyDarken`; otherwise `getRawBrightness(p, 0)`, the stored
    /// skylight at any hour. Where skylight is not exact yet (a partly loaded
    /// border) only block light counts.
    pub(super) fn growth_light(&mut self, p: IVec3, darkened: bool) -> u8 {
        self.update_block_light();
        let darken = if darkened { self.sky_darken() } else { 0 };
        self.raw_brightness(p, darken).unwrap_or_else(|| self.block_light(p))
    }

    /// Java `CropBlock.randomTick`: `getRawBrightness(pos, 0) >= 9` in the
    /// crop's cell, so open sky counts at night too. `SaplingBlock.randomTick`
    /// instead samples `getMaxLocalRawBrightness(pos.above())`, which night
    /// and storms darken.
    fn grows_here(&mut self, p: IVec3) -> bool {
        if self.get_block(p).is_some_and(Block::is_sapling) {
            self.growth_light(p + IVec3::Y, true) >= 9
        } else {
            self.growth_light(p, false) >= 9
        }
    }

    /// Whether a grass-like block under `above` survives: Java's
    /// `SpreadingSnowyBlock.canStayAlive` (a full fluid kills it; otherwise
    /// the block above must not dampen light completely).
    fn spreader_stays(&self, above: Option<Block>) -> bool {
        above.is_none_or(|a| a.light_opacity() < 15 && !a.is_fluid())
    }

    /// Java `SpreadingSnowyBlock.randomTick` for grass and mycelium: it dies
    /// to dirt when covered; otherwise, if `getMaxLocalRawBrightness(above)`
    /// is at least 9, four attempts pick a cell in x/z -1..=1 and y -3..=1
    /// and turn dirt there into `spread` when it could stay alive and has no
    /// water above.
    pub(super) fn tick_spreader(&mut self, p: IVec3, spread: Block) {
        if !self.spreader_stays(self.get_block(p + IVec3::Y)) {
            self.edit(p, Block::DIRT, false);
            return;
        }
        if self.growth_light(p + IVec3::Y, true) < 9 {
            return;
        }
        for _ in 0..4 {
            let r = self.roll();
            let q = p + IVec3::new((r % 3) as i32 - 1, (r / 3 % 5) as i32 - 3, (r / 15 % 3) as i32 - 1);
            let above = self.get_block(q + IVec3::Y);
            if self.get_block(q) == Some(Block::DIRT)
                && self.spreader_stays(above)
                && !above.is_some_and(Block::is_water)
            {
                self.edit(q, spread, false);
            }
        }
    }

    /// Grass spreads like Java's `SpreadingSnowyBlock` (see [`World::tick_spreader`]).
    fn tick_grass(&mut self, p: IVec3) {
        self.tick_spreader(p, Block::GRASS);
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
                    (0..=1).any(|dy| self.get_block(p + IVec3::new(dx, dy, dz)).is_some_and(|w| w.holds_water()))
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

    /// Java's `SaplingBlock.advanceTree`: a stage-0 sapling becomes stage 1,
    /// and a stage-1 sapling grows its tree. Returns whether a tree grew.
    pub fn advance_tree(&mut self, p: IVec3) -> bool {
        let Some(sapling) = self.get_block(p) else { return false };
        if let Some(next) = super::overworld_blocks::sapling_stage_one(sapling) {
            self.edit(p, next, false);
            return false;
        }
        self.grow_tree(p)
    }

    /// Grows the sapling at `p` into a tree if there's room for its trunk.
    /// Leaves only fill air, like world generation. Returns whether it grew.
    pub fn grow_tree(&mut self, mut p: IVec3) -> bool {
        let Some(sapling) = self.get_block(p) else { return false };
        if needs_two_by_two(sapling) {
            let Some(corner) = self.two_by_two_corner(p, sapling) else { return false };
            p = corner;
        }
        let v = self.roll() as u32;
        let mut blocks = Vec::new();
        let ground = p - IVec3::Y;
        let Some(wood) = sapling.wood().filter(|_| sapling.is_sapling()) else { return false };
        let sapling = sapling.base();
        super::trees::tree(wood, ground, v, &mut |q, b| blocks.push((q, b)));
        let room = blocks.iter().filter(|(_, b)| b.is_log()).all(|&(q, _)| {
            q == p
                || self
                    .get_block(q)
                    .is_some_and(|b| b.base() == sapling || b == Block::AIR || b.is_replaceable() || b.is_leaves())
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

    /// Java's `TreeGrower.findTwoByTwoSaplingPos`: the north-west corner of
    /// the first complete 2x2 of `sapling` containing `p`, trying x and then
    /// z offsets from 0 down to -1.
    fn two_by_two_corner(&self, p: IVec3, sapling: Block) -> Option<IVec3> {
        [0, -1].into_iter().flat_map(|x| [0, -1].map(move |z| p + IVec3::new(x, 0, z))).find(|&q| {
            [IVec3::ZERO, IVec3::X, IVec3::Z, IVec3::X + IVec3::Z]
                .into_iter()
                .all(|d| self.get_block(q + d).is_some_and(|b| b.base() == sapling.base()))
        })
    }

    /// Bone meal on the block at `p`: crops grow 2-5 stages, saplings have a
    /// 45% chance to grow, and grass sprouts tall grass and flowers around.
    /// Returns whether the bone meal was used.
    pub fn apply_bone_meal(&mut self, p: IVec3) -> bool {
        let Some(b) = self.get_block(p) else { return false };
        if let Some(used) = self.nether_bone_meal(p, b) {
            return used;
        }
        if let Some(used) = self.overworld_bone_meal(p, b) {
            return used;
        }
        match b {
            b if b.crop_stage().is_some_and(|s| s < 7) => {
                let (crop, stage) = b.as_crop().unwrap();
                let stage = (stage + 2 + (self.roll() % 4) as u8).min(7);
                self.edit(p, Block::crop(crop, stage), true);
                true
            }
            // Java's `SaplingBlock.isValidBonemealTarget` is `TreeGrower.canGrow`:
            // a dark or pale oak sapling outside a 2x2 keeps the bone meal.
            b if b.is_sapling() => {
                if needs_two_by_two(b) && self.two_by_two_corner(p, b).is_none() {
                    return false;
                }
                if self.roll() % 100 < 45 {
                    self.advance_tree(p);
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
        let activity = &self.activity;
        let chunks = &self.chunks;
        self.leaf_decay.retain(|&p, t| {
            if !activity.contains(p, true) || !chunks.contains_key(&chunk_of(p)) {
                return true;
            }
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
            .any(|&d| self.get_block(soil + d).is_some_and(|b| b.holds_water() || b == Block::ICE))
    }
}

/// Items a slab's loot table gives per drop: two for a double slab.
fn double_slab_count(block: Block) -> u8 {
    if super::slab_stair_states::slab_type(block) == Some(super::slab_stair_states::SlabType::Double) { 2 } else { 1 }
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
    fn gilded_blackstone_fortune_and_silk_follow_java() {
        let mut world = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
        let silk = crate::enchant::Enchants::NONE.with(crate::enchant::Enchantment::SilkTouch, 1);
        world.spill_mined(IVec3::ZERO, Block::GILDED_BLACKSTONE, silk);
        assert_eq!(world.drops.pop().unwrap().1, Stack::new(Block::GILDED_BLACKSTONE, 1));
        let fortune = crate::enchant::Enchants::NONE.with(crate::enchant::Enchantment::Fortune, 3);
        for _ in 0..100 {
            world.spill_mined(IVec3::ZERO, Block::GILDED_BLACKSTONE, fortune);
        }
        assert!(world.drops.iter().all(|(_, s)| s.item == Item::GOLD_NUGGET && (2..=5).contains(&s.count)));
        world.drops.clear();
        for _ in 0..10000 {
            world.spill_block(IVec3::ZERO, Block::GILDED_BLACKSTONE);
        }
        let nuggets = world.drops.iter().filter(|(_, s)| s.item == Item::GOLD_NUGGET).count();
        assert!((850..1150).contains(&nuggets), "10 percent chance, got {nuggets}");
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
        fn world(others: &[DVec3]) -> World {
            let mut world = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
            for x in [0, 10, 100] {
                let mut data = ChunkData::Uniform(Block::STONE);
                data.set(0, 0, 0, Block::AIR); // Non-uniform, with no random-tick side effects.
                world.insert_chunk(IVec3::new(x, 4, 0), Arc::new(data), false);
            }
            let host = DVec3::new(1.0, 150.0, 1.0);
            world.set_simulation_players(std::iter::once(host).chain(others.iter().copied()));
            world
        }
        // Storage chunk x = 10 starts at block 320, beyond the host's simulation distance.
        let far = DVec3::new(330.0, 150.0, 5.0);
        let mut host_only = world(&[]);
        let mut union = world(&[far]);
        let mut overlapping = world(&[DVec3::new(20.0, 150.0, 3.0), far, far, far + DVec3::X * 4.0]);
        for w in [&mut host_only, &mut union, &mut overlapping] {
            w.tick_random(crate::simulation::TICK_SECONDS, DVec3::new(1.0, 150.0, 1.0));
        }
        assert_ne!(host_only.rng, union.rng, "distant agents must get random ticks");
        assert_eq!(union.rng, overlapping.rng, "overlapping regions must tick once");
    }

    /// Three by three lit flat columns (stone to y 63) with the player-modified
    /// chunks the gameplay light arrays need, optionally under a stone roof
    /// wide enough that its centre is more than six steps from open sky.
    fn growth_world(roofed: bool) -> World {
        let mut world = crate::world::lighting::flat_test_world(
            (-1..=1).flat_map(|x| (-1..=1).map(move |z| glam::IVec2::new(x, z))),
            63,
        );
        if roofed {
            for x in 4..=28 {
                for z in 4..=28 {
                    world.set_block(IVec3::new(x, 67, z), Block::STONE);
                }
            }
        }
        world.update_block_light();
        world
    }

    #[test]
    fn covered_crops_need_nine_block_light_and_saplings_sample_above() {
        let mut world = growth_world(true);
        let crop = IVec3::new(16, 64, 16);
        world.set_block(crop - IVec3::Y, Block::WET_FARMLAND);
        world.set_block(crop, Block::wheat(0));
        world.update_block_light();
        assert!(world.sky_light(crop).unwrap() < 9, "{:?}", world.sky_light(crop));
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
    fn crops_read_stored_skylight_at_night_and_saplings_the_darkened_value() {
        // Java: CropBlock uses getRawBrightness(pos, 0) (no time of day) while
        // SaplingBlock uses getMaxLocalRawBrightness(pos.above()), which
        // subtracts skyDarken (11 at midnight, 5 on a clear dusk).
        let mut world = growth_world(false);
        let crop = IVec3::new(16, 64, 16);
        world.set_block(crop - IVec3::Y, Block::WET_FARMLAND);
        world.set_block(crop, Block::wheat(0));
        world.update_block_light();
        assert_eq!(world.sky_light(crop), Some(15));
        world.set_redstone_daylight(0.75, 11, 0.0);
        assert!(world.grows_here(crop), "wheat grows under a night sky");
        world.set_block(crop - IVec3::Y, Block::DIRT);
        world.set_block(crop, Block::OAK_SAPLING);
        assert!(!world.grows_here(crop), "15 - 11 = 4 darkens the sapling's sample");
        world.set_redstone_daylight(0.52, 6, 0.0);
        assert!(world.grows_here(crop), "15 - 6 = 9 is enough");
        world.set_redstone_daylight(0.55, 7, 0.0);
        assert!(!world.grows_here(crop));
        // A lamp beside it counts at any hour.
        world.set_block(crop + IVec3::new(1, 1, 0), Block::GLOWSTONE);
        assert!(world.grows_here(crop));
    }

    #[test]
    fn berries_and_bamboo_test_the_cell_above_with_stored_skylight() {
        let mut world = growth_world(true);
        let bush = IVec3::new(16, 64, 16);
        world.set_block(bush - IVec3::Y, Block::DIRT);
        world.update_block_light();
        assert!(!world.lit_enough(bush + IVec3::Y), "dark under the wide roof");
        let mut open = growth_world(false);
        open.update_block_light();
        assert!(open.lit_enough(bush + IVec3::Y));
        open.set_redstone_daylight(0.75, 11, 0.0);
        assert!(open.lit_enough(bush + IVec3::Y), "stored skylight, no night darkening");
    }

    #[test]
    fn grass_spreads_only_with_brightness_nine_above_and_dies_when_covered() {
        // Java SpreadingSnowyBlock.randomTick: getMaxLocalRawBrightness(above) >= 9
        // (skyDarken applies) before four spread attempts at dirt; a
        // light-opaque block above turns it to dirt.
        let spread = |darken: u8, roofed: bool| {
            let mut world = growth_world(roofed);
            let grass = IVec3::new(16, 63, 16);
            world.set_block(grass, Block::GRASS);
            for d in [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z] {
                world.set_block(grass + d, Block::DIRT);
            }
            world.update_block_light();
            world.set_redstone_daylight(0.5, darken, 0.0);
            for _ in 0..600 {
                world.random_tick(grass);
            }
            let n = [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z]
                .into_iter()
                .filter(|&d| world.get_block(grass + d) == Some(Block::GRASS))
                .count();
            (n, world.get_block(grass))
        };
        assert_eq!(spread(0, false), (4, Some(Block::GRASS)), "day spreads");
        assert_eq!(spread(11, false), (0, Some(Block::GRASS)), "15 - 11 < 9: night does not");
        assert_eq!(spread(0, true), (0, Some(Block::GRASS)), "dark under a roof, but not covered");
        let mut world = growth_world(false);
        let grass = IVec3::new(16, 63, 16);
        world.set_block(grass, Block::GRASS);
        world.set_block(grass + IVec3::Y, Block::STONE);
        world.random_tick(grass);
        assert_eq!(world.get_block(grass), Some(Block::DIRT));
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
