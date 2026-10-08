//! Living Nether biome blocks: nylium that dies under cover, weeping and
//! twisting vines that grow and keep their head/plant shape, and bone meal
//! on nylium, netherrack, fungi and vines (Java's `NyliumBlock`,
//! `NetherrackBlock`, `FungusBlock` and `GrowingPlantHead/BodyBlock`).

use glam::IVec3;

use super::World;
use super::block::Block;
use super::nether_biome_blocks::{self as nb, NetherWood, VINE_MAX_AGE, Vine};
use super::nether_features::{self as features, Level, Vegetation};
use super::structure::Rng;

/// Java's `growPerTickProbability` for weeping and twisting vines.
const VINE_GROWTH: f64 = 0.1;
/// `NetherVines.BONEMEAL_GROW_PROBABILITY_DECREASE_RATE`.
const VINE_BONEMEAL_CONTINUE: f64 = 0.826;

/// Features grown by bone meal write through the world, like Java's
/// `setBlock` with neighbour updates. Unloaded cells read as bedrock so
/// nothing grows into them.
struct Planting<'a>(&'a mut World);

impl Level for Planting<'_> {
    fn block(&self, p: IVec3) -> Block {
        self.0.get_block(p).unwrap_or(Block::BEDROCK)
    }
    fn place(&mut self, p: IVec3, b: Block) {
        self.0.set_block(p, b);
    }
}

impl World {
    fn flora_rng(&mut self) -> Rng {
        Rng(self.roll())
    }

    fn chance(&mut self, p: f64) -> bool {
        ((self.roll() >> 11) as f64 / (1u64 << 53) as f64) < p
    }

    /// Random ticks of the Nether biome blocks.
    pub(super) fn tick_nether_flora(&mut self, p: IVec3, b: Block) {
        if nb::is_nylium(b) {
            // `NyliumBlock.randomTick`: covered by a light-blocking block, it
            // turns back into netherrack.
            if self.get_block(p + IVec3::Y).is_some_and(|a| a.light_opacity() >= 15) {
                self.edit(p, Block::NETHERRACK, false);
            }
        } else if let Some((vine, Some(age))) = Vine::of(b)
            && age < VINE_MAX_AGE
            && self.chance(VINE_GROWTH)
        {
            let next = p + vine.grows();
            if self.get_block(next) == Some(Block::AIR) {
                self.set_block(next, vine.head(age + 1));
            }
        }
    }

    /// Whether bone meal does anything to `b` at `p` (`None`: not a Nether
    /// biome block, so the caller's own rules apply).
    pub(super) fn nether_bone_meal(&mut self, p: IVec3, b: Block) -> Option<bool> {
        if let Some(wood) = NetherWood::ALL.into_iter().find(|w| w.fungus() == b) {
            // `FungusBlock`: only on its own nylium, and 40% of uses succeed.
            if self.get_block(p - IVec3::Y) != Some(wood.nylium()) {
                return Some(false);
            }
            if self.chance(0.4) {
                let mut rng = self.flora_rng();
                features::huge_fungus(&mut Planting(self), &mut rng, p, wood, true);
            }
            return Some(true);
        }
        if nb::is_nylium(b) {
            if self.get_block(p + IVec3::Y) != Some(Block::AIR) {
                return Some(false);
            }
            let mut rng = self.flora_rng();
            let above = p + IVec3::Y;
            let level = &mut Planting(self);
            if b == nb::CRIMSON_NYLIUM {
                features::vegetation(level, &mut rng, above, Vegetation::Crimson, 3, 1);
            } else {
                features::vegetation(level, &mut rng, above, Vegetation::Warped, 3, 1);
                features::vegetation(level, &mut rng, above, Vegetation::NetherSprouts, 3, 1);
                if rng.below(8) == 0 {
                    features::twisting_vines(level, &mut rng, above, 3, 1, 2);
                }
            }
            return Some(true);
        }
        if b == Block::NETHERRACK {
            return Some(self.spread_nylium(p));
        }
        if let Some((vine, _)) = Vine::of(b) {
            return Some(self.bone_meal_vine(p, vine));
        }
        None
    }

    /// `NetherrackBlock`: netherrack under an open cell beside nylium turns
    /// into it (a coin toss when both kinds touch it).
    fn spread_nylium(&mut self, p: IVec3) -> bool {
        if self.get_block(p + IVec3::Y).is_none_or(|a| a.light_opacity() >= 15) {
            return false;
        }
        let (mut crimson, mut warped) = (false, false);
        for d in (-1..=1).flat_map(|x| (-1..=1).flat_map(move |y| (-1..=1).map(move |z| IVec3::new(x, y, z)))) {
            match self.get_block(p + d) {
                Some(nb::CRIMSON_NYLIUM) => crimson = true,
                Some(nb::WARPED_NYLIUM) => warped = true,
                _ => {}
            }
        }
        let nylium = match (crimson, warped) {
            (true, true) => {
                if self.roll() & 1 == 0 {
                    nb::WARPED_NYLIUM
                } else {
                    nb::CRIMSON_NYLIUM
                }
            }
            (true, false) => nb::CRIMSON_NYLIUM,
            (false, true) => nb::WARPED_NYLIUM,
            (false, false) => return false,
        };
        self.set_block(p, nylium);
        true
    }

    /// `GrowingPlantHeadBlock.performBonemeal`: 1+ blocks (each further one
    /// 82.6% likely) grow from the vine's head, ageing as they go.
    fn bone_meal_vine(&mut self, p: IVec3, vine: Vine) -> bool {
        // A plant block grows from its head.
        let mut head = p;
        while matches!(self.get_block(head).and_then(Vine::of), Some((v, None)) if v == vine) {
            head += vine.grows();
        }
        let Some((_, Some(age))) = self.get_block(head).and_then(Vine::of) else { return false };
        let mut next = head + vine.grows();
        if self.get_block(next) != Some(Block::AIR) {
            return false;
        }
        let mut blocks = 1;
        while self.chance(VINE_BONEMEAL_CONTINUE) {
            blocks += 1;
        }
        let mut age = (age + 1).min(VINE_MAX_AGE);
        for _ in 0..blocks {
            if self.get_block(next) != Some(Block::AIR) {
                break;
            }
            self.set_block(next, vine.head(age));
            next += vine.grows();
            age = (age + 1).min(VINE_MAX_AGE);
        }
        true
    }

    /// Shape updates around a changed cell `p`: weeping vines that lost the
    /// block they hang from break (all the way down), heads with more vine
    /// past them become plant, and plant left at the end becomes a head.
    pub(super) fn update_nether_vines(&mut self, p: IVec3) {
        for q in [p, p - IVec3::Y, p + IVec3::Y] {
            let Some(b) = self.get_block(q) else { continue };
            let Some((vine, age)) = Vine::of(b) else { continue };
            if vine == Vine::Weeping && !self.get_block(q + IVec3::Y).is_some_and(nb::hangs_from) {
                // Unsupported: the whole hanging run below drops.
                let mut at = q;
                while let Some(v) = self.get_block(at).filter(|v| matches!(Vine::of(*v), Some((Vine::Weeping, _)))) {
                    self.edit(at, Block::AIR, true);
                    self.spill_block(at, v);
                    at -= IVec3::Y;
                }
                continue;
            }
            let continues = self.get_block(q + vine.grows()).and_then(Vine::of).is_some_and(|(v, _)| v == vine);
            match age {
                Some(_) if continues => {
                    self.edit(q, vine.plant(), true);
                }
                None if !continues && self.get_block(q + vine.grows()).is_some() => {
                    let age = (self.roll() % VINE_MAX_AGE as u64) as u8;
                    self.edit(q, vine.head(age), true);
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::nether_biome_blocks::{
        CRIMSON_FUNGUS, CRIMSON_NYLIUM, CRIMSON_ROOTS, NETHER_SPROUTS, WARPED_FUNGUS, WARPED_NYLIUM, WARPED_ROOTS,
    };

    /// Loaded empty chunks around [`AT`]; no terrain generation needed.
    fn world() -> World {
        use crate::world::chunk::{ChunkData, WORLD_HEIGHT_CHUNKS};
        use std::sync::Arc;
        let generator = Arc::new(crate::world::terrain::Generator::new(42));
        let mut w = World::new_headless(generator, Default::default(), 2);
        for x in -1..=1 {
            for z in -1..=1 {
                for y in 0..WORLD_HEIGHT_CHUNKS {
                    w.insert_chunk(IVec3::new(x, y, z), Arc::new(ChunkData::Uniform(Block::AIR)), false);
                }
            }
        }
        w
    }

    const AT: IVec3 = IVec3::new(4, 200, 4);

    fn clear(w: &mut World) {
        for x in -8..=8 {
            for z in -8..=8 {
                for y in -3..=30 {
                    w.set_block(AT + IVec3::new(x, y, z), Block::AIR);
                }
                w.set_block(AT + IVec3::new(x, -1, z), Block::NETHERRACK);
            }
        }
    }

    #[test]
    fn nylium_dies_under_cover_and_spreads_onto_netherrack_by_bone_meal() {
        let mut w = world();
        clear(&mut w);
        w.set_block(AT - IVec3::Y, CRIMSON_NYLIUM);
        w.tick_nether_flora(AT - IVec3::Y, CRIMSON_NYLIUM);
        assert_eq!(w.get_block(AT - IVec3::Y), Some(CRIMSON_NYLIUM), "uncovered nylium lives");
        w.set_block(AT, Block::STONE);
        w.tick_nether_flora(AT - IVec3::Y, CRIMSON_NYLIUM);
        assert_eq!(w.get_block(AT - IVec3::Y), Some(Block::NETHERRACK));
        w.set_block(AT, Block::AIR);
        w.set_block(AT - IVec3::Y, WARPED_NYLIUM);
        let beside = AT - IVec3::Y + IVec3::X;
        assert!(w.apply_bone_meal(beside));
        assert_eq!(w.get_block(beside), Some(WARPED_NYLIUM));
        assert!(!w.apply_bone_meal(AT + IVec3::new(8, -1, 8)), "no nylium nearby");
    }

    #[test]
    fn bone_meal_on_nylium_grows_its_forest_floor() {
        let mut w = world();
        clear(&mut w);
        for x in -3..=3 {
            for z in -3..=3 {
                w.set_block(AT + IVec3::new(x, -1, z), CRIMSON_NYLIUM);
            }
        }
        assert!(w.apply_bone_meal(AT - IVec3::Y));
        let plants: Vec<Block> = (-2..=2)
            .flat_map(|x| (-2..=2).map(move |z| AT + IVec3::new(x, 0, z)))
            .filter_map(|q| w.get_block(q))
            .filter(|b| *b != Block::AIR)
            .collect();
        assert!(!plants.is_empty());
        assert!(plants.iter().all(|b| [CRIMSON_ROOTS, CRIMSON_FUNGUS, WARPED_FUNGUS].contains(b)), "{plants:?}");
        for x in -3..=3 {
            for z in -3..=3 {
                w.set_block(AT + IVec3::new(x, 0, z), Block::AIR);
                w.set_block(AT + IVec3::new(x, -1, z), WARPED_NYLIUM);
            }
        }
        assert!(w.apply_bone_meal(AT - IVec3::Y));
        let warped = (-2..=2)
            .flat_map(|x| (-2..=2).map(move |z| AT + IVec3::new(x, 0, z)))
            .filter_map(|q| w.get_block(q))
            .filter(|b| [WARPED_ROOTS, NETHER_SPROUTS, WARPED_FUNGUS].contains(b))
            .count();
        assert!(warped > 0);
    }

    #[test]
    fn fungi_grow_into_huge_fungi_only_on_their_nylium() {
        let mut w = world();
        clear(&mut w);
        w.set_block(AT - IVec3::Y, WARPED_NYLIUM);
        w.set_block(AT, CRIMSON_FUNGUS);
        assert!(!w.apply_bone_meal(AT), "crimson fungus on warped nylium");
        w.set_block(AT - IVec3::Y, CRIMSON_NYLIUM);
        let mut tries = 0;
        while w.get_block(AT) == Some(CRIMSON_FUNGUS) && tries < 50 {
            assert!(w.apply_bone_meal(AT));
            tries += 1;
        }
        assert_eq!(w.get_block(AT), Some(nb::CRIMSON_STEM), "grew after {tries} uses");
        assert!(tries > 0 && tries < 50);
        let wart = (-3..=3)
            .flat_map(|x| (-3..=3).flat_map(move |z| (3..28).map(move |y| AT + IVec3::new(x, y, z))))
            .filter(|&q| w.get_block(q) == Some(nb::NETHER_WART_BLOCK))
            .count();
        assert!(wart > 5, "a hat of {wart} wart blocks");
    }

    #[test]
    fn vines_grow_keep_their_shape_and_fall_without_support() {
        let mut w = world();
        clear(&mut w);
        let roof = AT + IVec3::Y * 10;
        w.set_block(roof, Block::NETHERRACK);
        w.set_block(roof - IVec3::Y, Vine::Weeping.head(0));
        assert!(w.apply_bone_meal(roof - IVec3::Y));
        let column: Vec<Block> = (1..=12).map(|d| w.get_block(roof - IVec3::Y * d).unwrap()).collect();
        let len = column.iter().take_while(|b| Vine::of(**b).is_some()).count();
        assert!(len >= 2, "{column:?}");
        assert!(column[..len - 1].iter().all(|b| *b == nb::WEEPING_VINES_PLANT), "{column:?}");
        assert!(matches!(Vine::of(column[len - 1]), Some((Vine::Weeping, Some(_)))));
        // Random ticks grow the head down by one, ageing it.
        let head = roof - IVec3::Y * len as i32;
        w.set_block(head, Vine::Weeping.head(3));
        for _ in 0..200 {
            w.tick_nether_flora(head, Vine::Weeping.head(3));
            if w.get_block(head - IVec3::Y) != Some(Block::AIR) {
                break;
            }
        }
        assert_eq!(w.get_block(head - IVec3::Y), Some(Vine::Weeping.head(4)));
        assert_eq!(w.get_block(head), Some(nb::WEEPING_VINES_PLANT), "the old head became plant");
        // Cutting the middle leaves a head above and drops everything below.
        w.set_block(roof - IVec3::Y * 2, Block::AIR);
        assert!(matches!(Vine::of(w.get_block(roof - IVec3::Y).unwrap()), Some((Vine::Weeping, Some(_)))));
        assert!((3..=10).all(|d| w.get_block(roof - IVec3::Y * d) == Some(Block::AIR)), "down to the floor");
        // Removing the roof drops the rest.
        w.set_block(roof, Block::AIR);
        assert_eq!(w.get_block(roof - IVec3::Y), Some(Block::AIR));
        // Twisting vines stop growing at the maximum age.
        w.set_block(AT, Vine::Twisting.head(VINE_MAX_AGE));
        for _ in 0..200 {
            w.tick_nether_flora(AT, Vine::Twisting.head(VINE_MAX_AGE));
        }
        assert_eq!(w.get_block(AT + IVec3::Y), Some(Block::AIR));
    }

    #[test]
    fn soul_fire_lights_on_soul_blocks_and_goes_out_off_them() {
        let mut w = world();
        clear(&mut w);
        for (base, fire) in
            [(nb::SOUL_SOIL, nb::SOUL_FIRE), (Block::SOUL_SAND, nb::SOUL_FIRE), (Block::NETHERRACK, Block::FIRE)]
        {
            w.set_block(AT, Block::AIR);
            w.set_block(AT - IVec3::Y, base);
            assert!(w.ignite(AT));
            assert_eq!(w.get_block(AT), Some(fire), "{}", base.name());
        }
        w.set_block(AT, Block::AIR);
        w.set_block(AT - IVec3::Y, nb::SOUL_SOIL);
        assert!(w.ignite(AT));
        w.tick_fire(60.0, glam::DVec3::new(4.0, 200.0, 4.0));
        assert_eq!(w.get_block(AT), Some(nb::SOUL_FIRE), "soul fire never burns out");
        w.set_block(AT - IVec3::Y, Block::NETHERRACK);
        assert_eq!(w.get_block(AT), Some(Block::AIR), "soul fire needs its soul base");
    }
}
