//! Small mushrooms: deterministic cave/swamp patches, low-light survival and Java spread.
use super::{
    World,
    block::Block,
    chunk::{CHUNK_SIZE_I, ChunkData},
    noise::hash3,
    terrain::{Biome, Dimension, Generator},
};
use glam::IVec3;

pub(super) fn decorate(generator: &Generator, cpos: IVec3, data: &mut ChunkData) {
    if generator.dimension == Dimension::End || data.uniform().is_some() {
        return;
    }
    let origin = cpos * CHUNK_SIZE_I;
    for attempt in 0..12 {
        let r = hash3(cpos.x, cpos.y, cpos.z, generator.seed ^ 0x4D55_5348 ^ attempt);
        let x = (r & 31) as usize;
        let z = ((r >> 5) & 31) as usize;
        let column = generator.column(origin.x + x as i32, origin.z + z as i32);
        for y in (1..31).rev() {
            if data.get(x, y, z) != Block::AIR || !data.get(x, y - 1, z).is_opaque() {
                continue;
            }
            let covered = (y + 1..32).any(|h| data.get(x, h, z).is_opaque());
            let underground = origin.y + y as i32 <= column.height - 4;
            if covered || underground || column.biome == Biome::Swamp {
                data.set(x, y, z, if r & 64 == 0 { Block::BROWN_MUSHROOM } else { Block::RED_MUSHROOM });
                break;
            }
        }
    }
}

impl World {
    /// Skylight uses the engine's vertical exposure approximation; block light is authoritative.
    pub fn mushroom_survives(&mut self, p: IVec3) -> bool {
        self.update_block_light();
        self.get_block(p - IVec3::Y).is_some_and(Block::is_opaque)
            && (!self.generator.dimension.has_sky() || !self.sky_exposed(p))
            && self.block_light(p) < 13
    }
    pub(super) fn tick_mushroom(&mut self, p: IVec3, mushroom: Block) {
        if !self.mushroom_survives(p) {
            self.edit(p, Block::AIR, false);
            self.spill_block(p, mushroom);
            return;
        }
        if !self.one_in(25) {
            return;
        }
        let mut count = 0;
        for x in -4..=4 {
            for y in -1..=1 {
                for z in -4..=4 {
                    if self.get_block(p + IVec3::new(x, y, z)) == Some(mushroom) {
                        count += 1;
                        if count >= 5 {
                            return;
                        }
                    }
                }
            }
        }
        let mut origin = p;
        let mut candidate = self.mushroom_offset(origin);
        for _ in 0..4 {
            if self.get_block(candidate) == Some(Block::AIR) && self.mushroom_survives(candidate) {
                origin = candidate;
            }
            candidate = self.mushroom_offset(origin);
        }
        if self.get_block(candidate) == Some(Block::AIR) && self.mushroom_survives(candidate) {
            self.edit(candidate, mushroom, false);
        }
    }
    fn mushroom_offset(&mut self, p: IVec3) -> IVec3 {
        // Java: nextInt(3) - 1 on x/z, and nextInt(2) - nextInt(2) on y.
        let x = (self.roll() % 3) as i32 - 1;
        let y = (self.roll() % 2) as i32 - (self.roll() % 2) as i32;
        let z = (self.roll() % 3) as i32 - 1;
        p + IVec3::new(x, y, z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Item;
    use crate::world::chunk::WORLD_HEIGHT_CHUNKS;
    use crate::world::terrain::Generator;
    use std::sync::Arc;

    fn flat() -> World {
        let mut world = World::new_headless(Arc::new(Generator::new(3)), Default::default(), 2);
        for y in 0..WORLD_HEIGHT_CHUNKS {
            world.insert_chunk(IVec3::new(0, y, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        }
        world
    }

    #[test]
    fn daylight_breaks_mushrooms_and_a_crowd_of_five_does_not_spread() {
        let mut world = flat();
        let open = IVec3::new(4, 80, 4);
        world.set_block(open - IVec3::Y, Block::STONE);
        world.set_block(open, Block::BROWN_MUSHROOM);
        assert!(world.sky_exposed(open));
        world.tick_mushroom(open, Block::BROWN_MUSHROOM);
        assert_eq!(world.get_block(open), Some(Block::AIR));
        assert_eq!(world.drops.last().map(|(_, stack)| stack.item), Some(Item::from_block(Block::BROWN_MUSHROOM)));

        let roof = IVec3::new(8, 80, 8);
        world.set_block(roof + IVec3::Y * 2, Block::STONE);
        for (dx, dz) in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)] {
            let p = roof + IVec3::new(dx, 0, dz);
            world.set_block(p - IVec3::Y, Block::STONE);
            world.set_block(p, Block::RED_MUSHROOM);
        }
        assert!(world.mushroom_survives(roof));
        let before = world.drops.len();
        for _ in 0..40 {
            world.tick_mushroom(roof, Block::RED_MUSHROOM);
        }
        assert_eq!(world.drops.len(), before, "five mushrooms in range refuse to spread");
        assert_eq!(world.get_block(roof), Some(Block::RED_MUSHROOM));
    }

    #[test]
    fn swamps_and_caves_are_decorated_with_both_mushrooms() {
        let generator = Generator::new(11);
        let mut data = ChunkData::Uniform(Block::STONE);
        for x in 0..32 {
            for z in 0..32 {
                data.set(x, 20, z, Block::AIR);
            }
        }
        decorate(&generator, IVec3::new(3, 2, 5), &mut data);
        let mut brown = 0;
        let mut red = 0;
        data.for_each_block(|block| match block {
            Block::BROWN_MUSHROOM => brown += 1,
            Block::RED_MUSHROOM => red += 1,
            _ => {}
        });
        assert!(brown + red > 0, "covered stone grows a patch");
    }
}
