//! Rules for the v0.6 Overworld plants: what holds them up, how they grow
//! on random ticks and with bone meal, what they drop and what a player
//! picks from them (Java's cave vines, kelp, sweet berries, cocoa, bamboo,
//! amethyst buds, coral and double plants).

use glam::IVec3;

use super::World;
use super::block::{Block, Facing};
use super::overworld_blocks::{self as ob, Thickness};
use crate::inventory::Stack;
use crate::item::Item;

/// Java's kelp and cave vine heads stop growing at age 25; this engine
/// keeps no age, so heads stop with this chance per growth tick instead.
const HEAD_STOP: u64 = 26;

impl World {
    /// Re-checks the plants around a changed cell: hanging plants need what
    /// they hang from, wall plants their wall, double plants both halves.
    /// Kelp and cave vines turn into their stem form under more of
    /// themselves and back into a head at the end of the run.
    pub(super) fn update_overworld_plants(&mut self, p: IVec3) {
        for q in [p, p - IVec3::Y, p + IVec3::Y, p + IVec3::X, p - IVec3::X, p + IVec3::Z, p - IVec3::Z] {
            let Some(b) = self.get_block(q) else { continue };
            if !(ob::FIRST..=ob::LAST).contains(&b.0) {
                continue;
            }
            // Ceiling plants: the whole run below drops when unsupported.
            if let Some(false) = self.get_block(q + IVec3::Y).and_then(|above| ob::hangs_from(b, above)) {
                let mut at = q;
                while let Some(h) = self.get_block(at).filter(|h| ob::hangs_from(*h, Block::STONE).is_some()) {
                    self.edit(at, Block::AIR, true);
                    self.spill_block(at, h);
                    at -= IVec3::Y;
                }
                continue;
            }
            if let Some(wall) = ob::wall_of(b)
                && let Some(false) = self.get_block(q + wall.offset()).and_then(|w| ob::wall_ok(b, w))
            {
                self.edit(q, Block::AIR, true);
                self.spill_block(q, b);
                continue;
            }
            if let Some((lower, upper)) = ob::double_of(b) {
                let other = if upper { q - IVec3::Y } else { q + IVec3::Y };
                let partner = Block(if upper { lower.0 } else { lower.0 + 1 });
                if self.get_block(other).is_some_and(|o| o != partner) {
                    self.edit(q, Block::AIR, true);
                    if !upper {
                        self.spill_block(q, b);
                    }
                }
                continue;
            }
            if matches!(b, ob::SMALL_DRIPLEAF_TOP | ob::TALL_SEAGRASS_TOP)
                && self.get_block(q - IVec3::Y).is_some_and(|below| !b.can_stay_on(below))
            {
                self.edit(q, if b.is_waterlogged() { Block::WATER } else { Block::AIR }, true);
                continue;
            }
            self.fix_stem(q, b);
        }
    }

    /// Kelp, cave vines, pale hanging moss and big dripleaf are a head with
    /// stem parts behind it.
    fn fix_stem(&mut self, q: IVec3, b: Block) {
        let (head, stem, grows) = match b {
            ob::KELP | ob::KELP_PLANT => (ob::KELP, ob::KELP_PLANT, IVec3::Y),
            ob::PALE_HANGING_MOSS_TIP | ob::PALE_HANGING_MOSS => {
                (ob::PALE_HANGING_MOSS_TIP, ob::PALE_HANGING_MOSS, IVec3::NEG_Y)
            }
            ob::BIG_DRIPLEAF | ob::BIG_DRIPLEAF_STEM => (ob::BIG_DRIPLEAF, ob::BIG_DRIPLEAF_STEM, IVec3::Y),
            b if matches!(b.0, 1944..=1947) => {
                let lit = matches!(b.0, 1945 | 1947);
                let continues = self.get_block(q - IVec3::Y).is_some_and(|n| matches!(n.0, 1944..=1947));
                let want = Block(if continues { 1946 } else { 1944 } + lit as u16);
                if want != b {
                    self.edit(q, want, true);
                }
                return;
            }
            _ => return,
        };
        let continues = self.get_block(q + grows).is_some_and(|n| n == head || n == stem);
        let want = if continues { stem } else { head };
        if want != b {
            self.edit(q, want, true);
        }
    }

    /// Random-tick growth for this module's plants. Returns whether `b`
    /// belongs here.
    pub(super) fn tick_overworld_flora(&mut self, p: IVec3, b: Block) -> bool {
        match b.0 {
            // Kelp grows up through water, 14% of ticks (Java's 0.14).
            1960 => {
                if self.roll() % 100 < 14
                    && self.get_block(p + IVec3::Y) == Some(Block::WATER)
                    && !self.one_in(HEAD_STOP)
                {
                    self.edit(p + IVec3::Y, ob::KELP, true);
                    self.edit(p, ob::KELP_PLANT, true);
                }
            }
            // Cave vines grow down 10% of ticks; new heads bear berries 11% of the time.
            1944 | 1945 => {
                if self.roll().is_multiple_of(10)
                    && self.get_block(p - IVec3::Y) == Some(Block::AIR)
                    && !self.one_in(HEAD_STOP)
                {
                    let lit = self.roll() % 100 < 11;
                    self.edit(p - IVec3::Y, if lit { ob::CAVE_VINES_LIT } else { ob::CAVE_VINES }, true);
                    self.edit(p, if b.0 == 1945 { ob::CAVE_VINES_PLANT_LIT } else { ob::CAVE_VINES_PLANT }, true);
                }
            }
            // Sweet berry bushes age 20% of ticks in light 9 or more.
            2058..=2060 => {
                if self.roll().is_multiple_of(5) && self.lit_enough(p) {
                    self.edit(p, ob::berry_bush(ob::berry_age(b).unwrap() + 1), true);
                }
            }
            // Cocoa ripens one tick in five.
            2062..=2069 => {
                if self.one_in(5) {
                    let (age, wall) = ob::cocoa_of(b).unwrap();
                    self.edit(p, ob::cocoa(age + 1, wall), true);
                }
            }
            // Bamboo shoots and stalks grow up one in three ticks, to 12-16 tall.
            2022..=2025 => {
                if self.one_in(3) && self.get_block(p + IVec3::Y) == Some(Block::AIR) && self.lit_enough(p + IVec3::Y) {
                    let height = (0..16)
                        .take_while(|&d| self.get_block(p - IVec3::Y * d).is_some_and(|q| matches!(q.0, 2022..=2025)))
                        .count();
                    if height < 12 + (self.roll() % 5) as usize {
                        self.grow_bamboo(p);
                    }
                }
            }
            // Budding amethyst grows buds on a free face one tick in five.
            1925 => {
                if self.one_in(5) {
                    let face = [IVec3::Y, IVec3::NEG_Y][(self.roll() % 2) as usize];
                    let q = p + face;
                    match self.get_block(q) {
                        Some(Block::AIR) | Some(Block::WATER) => {
                            self.edit(q, ob::amethyst_bud(0, face.y < 0), true);
                        }
                        Some(bud)
                            if let Some((size, down)) = ob::bud_of(bud)
                                && size < 3
                                && down == (face.y < 0) =>
                        {
                            self.edit(q, ob::amethyst_bud(size + 1, down), true);
                        }
                        _ => {}
                    }
                }
            }
            // Live coral away from water dies.
            1980..=1984 | 1990..=1994 | 2000..=2004 => {
                let wet = b.is_waterlogged()
                    || [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z]
                        .iter()
                        .any(|&d| self.get_block(p + d).is_some_and(Block::holds_water));
                if !wet {
                    let (kind, colour, _) = ob::coral_of(b).unwrap();
                    self.edit(p, ob::coral(kind, colour, true), true);
                }
            }
            // Mycelium spreads like grass onto lit dirt.
            1903 => {
                let r = self.roll();
                let q = p + IVec3::new((r % 3) as i32 - 1, (r / 3 % 5) as i32 - 3, (r / 15 % 3) as i32 - 1);
                if self.get_block(q) == Some(Block::DIRT) && self.get_block(q + IVec3::Y) == Some(Block::AIR) {
                    self.edit(q, ob::MYCELIUM, false);
                }
            }
            // Turtle eggs crack and hatch over time (hatching spawns turtles in the game).
            _ => return (ob::FIRST..=ob::LAST).contains(&b.0),
        }
        true
    }

    fn lit_enough(&mut self, p: IVec3) -> bool {
        self.update_block_light();
        self.sky_exposed(p) || self.block_light(p) >= 9
    }

    /// Adds one stalk on top of the bamboo at `p`, moving its leaves up.
    fn grow_bamboo(&mut self, p: IVec3) {
        let top = p + IVec3::Y;
        self.edit(top, ob::BAMBOO_LARGE_LEAVES, true);
        if self.get_block(p) == Some(ob::BAMBOO_SAPLING) {
            self.edit(p, ob::BAMBOO, true);
        } else {
            self.edit(p, ob::BAMBOO_SMALL_LEAVES, true);
            if self.get_block(p - IVec3::Y).is_some_and(|b| matches!(b.0, 2023 | 2024)) {
                self.edit(p - IVec3::Y, ob::BAMBOO, true);
            }
        }
    }

    /// Bone meal on one of this module's blocks (`None`: not ours).
    pub(super) fn overworld_bone_meal(&mut self, p: IVec3, b: Block) -> Option<bool> {
        Some(match b.0 {
            // Azaleas grow into azalea trees 45% of the time.
            1940 | 1941 => {
                if self.roll() % 100 < 45 {
                    let v = self.roll() as u32;
                    let mut blocks = Vec::new();
                    super::trees::azalea_tree(p - IVec3::Y, v, &mut |q, b| blocks.push((q, b)));
                    for (q, b) in blocks {
                        let free = q == p || self.get_block(q).is_some_and(|c| c == Block::AIR || c.is_replaceable());
                        if free {
                            self.edit(q, b, false);
                        }
                    }
                    self.edit(p - IVec3::Y, ob::ROOTED_DIRT, false);
                }
                true
            }
            // Moss spreads over nearby stone and dirt, with plants on top.
            1907 => {
                for _ in 0..24 {
                    let r = self.roll();
                    let q = p + IVec3::new((r % 7) as i32 - 3, (r / 7 % 3) as i32 - 1, (r / 21 % 7) as i32 - 3);
                    let Some(g) = self.get_block(q) else { continue };
                    let mossable =
                        matches!(g, Block::STONE | Block::DIRT | Block::GRASS | Block::DEEPSLATE | Block::TUFF)
                            || g == Block::GRANITE
                            || g == Block::DIORITE
                            || g == Block::ANDESITE;
                    if mossable && self.get_block(q + IVec3::Y) == Some(Block::AIR) {
                        self.edit(q, ob::MOSS_BLOCK, true);
                        let roll = self.roll() % 10;
                        if roll < 3 {
                            let plant = [ob::MOSS_CARPET, Block::TALL_GRASS, ob::AZALEA][(roll % 3) as usize];
                            self.edit(q + IVec3::Y, plant, true);
                        }
                    }
                }
                true
            }
            // Kelp and cave vines grow; cave vines also ripen.
            1960 | 1961 => {
                let mut top = p;
                while self.get_block(top + IVec3::Y).is_some_and(|q| q == ob::KELP || q == ob::KELP_PLANT) {
                    top += IVec3::Y;
                }
                if self.get_block(top + IVec3::Y) == Some(Block::WATER) {
                    self.edit(top + IVec3::Y, ob::KELP, true);
                    self.edit(top, ob::KELP_PLANT, true);
                    true
                } else {
                    false
                }
            }
            1944 | 1946 => {
                self.edit(p, Block(b.0 + 1), true);
                true
            }
            1945 | 1947 => false,
            // Seagrass becomes tall seagrass if there is water above.
            1962 => {
                if self.get_block(p + IVec3::Y) == Some(Block::WATER) {
                    self.edit(p, ob::TALL_SEAGRASS, true);
                    self.edit(p + IVec3::Y, ob::TALL_SEAGRASS_TOP, true);
                    true
                } else {
                    false
                }
            }
            // Sea pickles on coral spread more pickles around them.
            1965..=1968 => {
                if let Some(n) = ob::pickle_count(b).filter(|&n| n < 4) {
                    self.edit(p, ob::sea_pickles(n + 1), true);
                    true
                } else {
                    false
                }
            }
            2058..=2060 => {
                self.edit(p, ob::berry_bush(ob::berry_age(b).unwrap() + 1), true);
                true
            }
            2061 => false,
            2062..=2069 => {
                let (age, wall) = ob::cocoa_of(b).unwrap();
                self.edit(p, ob::cocoa(age + 1, wall), true);
                true
            }
            2070..=2073 => false,
            2022..=2025 => {
                let mut top = p;
                while self.get_block(top + IVec3::Y).is_some_and(|q| matches!(q.0, 2022..=2025)) {
                    top += IVec3::Y;
                }
                if self.get_block(top + IVec3::Y) == Some(Block::AIR) {
                    self.grow_bamboo(top);
                    true
                } else {
                    false
                }
            }
            // Small dripleaf grows into a big one; big dripleaf grows taller.
            1952 | 1953 => {
                let base = if b.0 == 1953 { p - IVec3::Y } else { p };
                self.edit(base + IVec3::Y, Block::AIR, true);
                self.edit(base, ob::BIG_DRIPLEAF_STEM, true);
                self.edit(base + IVec3::Y, ob::BIG_DRIPLEAF, true);
                true
            }
            1950 => {
                if self.get_block(p + IVec3::Y) == Some(Block::AIR) {
                    self.edit(p, ob::BIG_DRIPLEAF_STEM, true);
                    self.edit(p + IVec3::Y, ob::BIG_DRIPLEAF, true);
                    true
                } else {
                    false
                }
            }
            // Flowers that grow tall drop a copy of themselves.
            2041..=2048 => {
                let (lower, _) = ob::double_of(b).unwrap();
                self.drops.push((p, Stack::new(lower, 1)));
                true
            }
            2029 => {
                self.drops.push((p, Stack::new(ob::PINK_PETALS, 1)));
                true
            }
            2014..=2017 => {
                if self.roll() % 100 < 45 {
                    self.grow_tree(p);
                }
                true
            }
            _ => return None,
        })
    }

    /// Right-click harvesting: ripe sweet berries and glow berries come off
    /// the plant, which stays. Returns whether anything was picked.
    pub fn harvest(&mut self, p: IVec3) -> bool {
        let Some(b) = self.get_block(p) else { return false };
        match b.0 {
            2060 | 2061 => {
                let n = 1 + self.roll() % 2 + (b.0 == 2061) as u64;
                self.drops.push((p, Stack::new(Item::SWEET_BERRIES, n as u8)));
                self.edit(p, ob::berry_bush(1), true);
                true
            }
            1945 | 1947 => {
                self.drops.push((p, Stack::new(Item::GLOW_BERRIES, 1)));
                self.edit(p, Block(b.0 - 1), true);
                true
            }
            _ => false,
        }
    }

    /// Extra drops of this module's blocks (fruit, shards, pickles,
    /// saplings from the new leaves). Pushes onto `out`.
    pub(super) fn overworld_drops(&mut self, b: Block, fortune: u64, out: &mut Vec<Stack>) {
        match b.0 {
            1910 => out.push(Stack::new(Item::SNOWBALL, 1)),
            1929 | 1933 => {
                // Clusters: 4 shards, times Java's ore bonus with fortune.
                let times = 1 + (self.roll() % (fortune + 2)).saturating_sub(1);
                out.push(Stack::new(Item::AMETHYST_SHARD, (4 * times).min(64) as u8));
            }
            1945 | 1947 => out.push(Stack::new(Item::GLOW_BERRIES, 1)),
            1965..=1968 => out.push(Stack::new(ob::SEA_PICKLE, ob::pickle_count(b).unwrap())),
            1973 => {
                let n = (2 + self.roll() % 2 + self.up_to(fortune)).min(5);
                out.push(Stack::new(Item::PRISMARINE_CRYSTALS, n as u8));
            }
            2060 | 2061 => {
                let n = 1 + self.roll() % 2 + (b.0 == 2061) as u64;
                out.push(Stack::new(Item::SWEET_BERRIES, n as u8));
            }
            2058 | 2059 => out.push(Stack::new(Item::SWEET_BERRIES, 1)),
            2062..=2073 => {
                let (age, _) = ob::cocoa_of(b).unwrap();
                let n = if age == 2 { 2 + self.roll() % 2 } else { 1 };
                out.push(Stack::new(Item::COCOA_BEANS, n as u8));
            }
            2074 | 2075 => {
                // 0-2 mushrooms (Java: uniform -6..2, floored at 0).
                let n = (self.roll() % 9).saturating_sub(6);
                if n > 0 {
                    let mushroom = if b.0 == 2074 { Block::RED_MUSHROOM } else { Block::BROWN_MUSHROOM };
                    out.push(Stack::new(mushroom, n as u8));
                }
            }
            1942 | 1943 => {
                if self.one_in([20, 16, 12, 10][fortune.min(3) as usize]) {
                    out.push(Stack::new(if b.0 == 1943 { ob::FLOWERING_AZALEA } else { ob::AZALEA }, 1));
                }
            }
            // Mangrove leaves drop no propagules in Java (they grow under leaves).
            2010 | 2012 | 2013 => {
                let f = fortune.min(3) as usize;
                if self.one_in([20, 16, 12, 10][f])
                    && let Some(wood) = b.wood()
                {
                    out.push(Stack::new(wood.sapling(), 1));
                }
                if b.0 == 2010 && self.one_in([200, 180, 160, 120][f]) {
                    out.push(Stack::new(Item::APPLE, 1));
                }
            }
            1962..=1964 | 2049..=2052 if self.one_in(8) && b.0 >= 2049 => out.push(Stack::new(Item::WHEAT_SEEDS, 1)),
            _ => {}
        }
    }

    /// Whether an upward pointed dripstone tip is at `p` (landing on one
    /// doubles fall damage in Java).
    pub fn dripstone_spike(&self, p: IVec3) -> bool {
        matches!(self.get_block(p).and_then(ob::dripstone_of), Some((false, Thickness::Tip)))
    }

    /// Whether a block placed into `at` against `normal` stays: hanging
    /// plants need a ceiling, wall plants a wall, waterlogged plants water.
    pub fn overworld_placement_ok(&self, at: IVec3, block: Block) -> Option<bool> {
        if !(ob::FIRST..=ob::LAST).contains(&block.0) {
            return None;
        }
        if block.is_waterlogged() && self.get_block(at) != Some(Block::WATER) {
            return Some(false);
        }
        if let Some(above) = self.get_block(at + IVec3::Y)
            && let Some(ok) = ob::hangs_from(block, above)
        {
            return Some(ok);
        }
        if let Some(wall) = ob::wall_of(block) {
            return Some(self.get_block(at + wall.offset()).and_then(|w| ob::wall_ok(block, w)).unwrap_or(false));
        }
        if matches!(ob::double_of(block), Some((_, false))) || block == ob::TALL_SEAGRASS {
            let room = self.get_block(at + IVec3::Y).is_some_and(|b| b.is_replaceable() && b != Block::LAVA);
            if !room {
                return Some(false);
            }
        }
        None
    }

    /// Prepares placing one of this module's blocks at `at`: the upper half
    /// of a double plant goes on top first, so the lower half finds it.
    pub fn before_overworld_place(&mut self, at: IVec3, block: Block) {
        if let Some((lower, false)) = ob::double_of(block) {
            self.edit(at + IVec3::Y, Block(lower.0 + 1), true);
        }
        if block == ob::TALL_SEAGRASS {
            self.edit(at + IVec3::Y, ob::TALL_SEAGRASS_TOP, true);
        }
    }

    /// The side a vine placed against `normal` hangs on.
    pub fn vine_side(normal: IVec3) -> Option<Facing> {
        Facing::from_offset(-normal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::chunk::ChunkData;
    use crate::world::terrain::Generator;
    use std::sync::Arc;

    fn world() -> World {
        let mut w = World::new_headless(Arc::new(Generator::new(5)), Default::default(), 2);
        for y in w.generator.dimension.chunk_rows() {
            w.insert_chunk(IVec3::new(0, y, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        }
        w
    }

    #[test]
    fn hanging_plants_drop_with_their_ceiling() {
        let mut w = world();
        let top = IVec3::new(5, 100, 5);
        w.set_block(top, Block::STONE);
        w.set_block(top - IVec3::Y, ob::CAVE_VINES_PLANT);
        w.set_block(top - IVec3::Y * 2, ob::CAVE_VINES_LIT);
        w.set_block(top, Block::AIR);
        assert_eq!(w.get_block(top - IVec3::Y), Some(Block::AIR));
        assert_eq!(w.get_block(top - IVec3::Y * 2), Some(Block::AIR));
        assert!(w.drops.iter().any(|(_, s)| s.item == Item::GLOW_BERRIES));
    }

    #[test]
    fn breaking_kelp_leaves_water_and_double_plants_go_together() {
        let mut w = world();
        let at = IVec3::new(3, 60, 3);
        w.set_block(at - IVec3::Y, Block::SAND);
        w.set_block(at, ob::KELP);
        w.set_block(at, Block::AIR);
        assert_eq!(w.get_block(at), Some(Block::WATER));

        let flower = IVec3::new(8, 70, 8);
        w.set_block(flower - IVec3::Y, Block::GRASS);
        w.before_overworld_place(flower, ob::SUNFLOWER);
        w.set_block(flower, ob::SUNFLOWER);
        assert_eq!(w.get_block(flower + IVec3::Y), Some(Block(ob::SUNFLOWER.0 + 1)));
        w.set_block(flower + IVec3::Y, Block::AIR);
        assert_eq!(w.get_block(flower), Some(Block::AIR), "the lower half goes with the upper");
    }

    #[test]
    fn berries_ripen_and_are_picked() {
        let mut w = world();
        let at = IVec3::new(4, 70, 4);
        w.set_block(at - IVec3::Y, Block::GRASS);
        w.set_block(at, ob::berry_bush(2));
        assert!(w.harvest(at));
        assert_eq!(w.get_block(at), Some(ob::berry_bush(1)));
        assert!(w.drops.iter().any(|(_, s)| s.item == Item::SWEET_BERRIES));
        assert!(w.overworld_bone_meal(at, ob::berry_bush(1)).unwrap());
        assert_eq!(w.get_block(at), Some(ob::berry_bush(2)));
    }

    #[test]
    fn vines_fall_without_their_wall() {
        let mut w = world();
        let wall = IVec3::new(6, 80, 6);
        w.set_block(wall, Block::STONE);
        let vine = wall + IVec3::Z;
        w.set_block(vine, ob::vine(Facing::North));
        assert_eq!(w.overworld_placement_ok(vine, ob::vine(Facing::North)), Some(true));
        w.set_block(wall, Block::AIR);
        assert_eq!(w.get_block(vine), Some(Block::AIR));
    }
}
