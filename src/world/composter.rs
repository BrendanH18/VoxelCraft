//! Java composter states, food probabilities, delayed readiness and sided automation.
use super::{World, block::Block};
use crate::{inventory::Stack, item::Item};
use glam::IVec3;

/// State zero retains the existing workstation ID; filled levels append after pumpkins.
pub const fn level(b: Block) -> Option<u8> {
    match b.0 {
        904 => Some(0),
        961..=968 => Some((b.0 - 960) as u8),
        _ => None,
    }
}
pub const fn state(level: u8) -> Block {
    if level == 0 { Block::COMPOSTER } else { Block(960 + level as u16) }
}
/// Only families present in this engine are listed. Poisonous potatoes cannot compost.
pub fn chance(item: Item) -> f32 {
    if matches!(item, Item::WHEAT_SEEDS | Item::PUMPKIN_SEEDS) {
        return 0.3;
    }
    if matches!(item, Item::MELON_SLICE) {
        return 0.5;
    }
    if matches!(item, Item::APPLE | Item::CARROT | Item::POTATO | Item::BEETROOT | Item::WHEAT | Item::NETHER_WART) {
        return 0.65;
    }
    if matches!(item, Item::BREAD | Item::BAKED_POTATO) {
        return 0.85;
    }
    if item == Item::CAKE {
        return 1.0;
    }
    let Some(b) = item.block() else { return 0.0 };
    if b.is_leaves() || b.is_sapling() || b == Block::TALL_GRASS {
        return 0.3;
    }
    match b.base() {
        Block::CACTUS | Block::SUGAR_CANE => 0.5,
        Block::PUMPKIN
        | Block::CARVED_PUMPKIN
        | Block::MELON
        | Block::BROWN_MUSHROOM
        | Block::RED_MUSHROOM
        | Block::DANDELION
        | Block::POPPY => 0.65,
        Block::HAY_BALE => 0.85,
        _ => 0.0,
    }
}
impl World {
    /// Consumes one eligible item even on a failed roll. The first level is guaranteed.
    pub fn compost(&mut self, p: IVec3, item: Item) -> bool {
        let Some(level) = self.get_block(p).and_then(level) else { return false };
        let chance = chance(item);
        if level >= 7 || chance == 0.0 {
            return false;
        }
        let seed = self.generator.seed ^ self.redstone.tick ^ self.compost_sequence;
        self.compost_sequence = self.compost_sequence.wrapping_add(1);
        let roll = super::noise::hash_f(p.x, p.y, p.z, seed);
        if level == 0 || roll < chance {
            self.set_block(p, state(level + 1));
            if level == 6 {
                self.schedule_redstone(p, 20, 0);
            }
        }
        true
    }
    pub fn take_compost(&mut self, p: IVec3) -> Option<Stack> {
        if self.get_block(p).and_then(level) != Some(8) {
            return None;
        }
        self.set_block(p, state(0));
        Some(Stack::new(Item::BONE_MEAL, 1))
    }
    pub(super) fn finish_compost(&mut self, p: IVec3) {
        if self.get_block(p).and_then(level) == Some(7) {
            self.set_block(p, state(8));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{chunk::ChunkData, redstone_blocks as r, terrain::Generator};
    use std::sync::Arc;
    const P: IVec3 = IVec3::new(4, 145, 8);
    fn world() -> World {
        let mut w = World::new_headless(Arc::new(Generator::new(7)), Default::default(), 2);
        w.insert_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        w
    }
    #[test]
    fn states_chances_delayed_ready_and_saved_ticks() {
        let mut w = world();
        w.set_block(P, Block::COMPOSTER);
        assert!(!w.compost(P, Item::IRON_INGOT));
        assert!(w.compost(P, Item::WHEAT_SEEDS));
        assert_eq!(w.get_block(P), Some(state(1)));
        for _ in 1..7 {
            assert!(w.compost(P, Item::CAKE));
        }
        assert_eq!(w.get_block(P), Some(state(7)));
        assert!(w.take_compost(P).is_none());
        let saved = w.redstone_to_string();
        let mut loaded = world();
        loaded.set_block(P, state(7));
        loaded.load_redstone(&saved);
        for _ in 0..19 {
            loaded.tick_redstone();
        }
        assert_eq!(loaded.get_block(P), Some(state(7)));
        loaded.tick_redstone();
        assert_eq!(loaded.take_compost(P), Some(Stack::new(Item::BONE_MEAL, 1)));
        assert_eq!(loaded.get_block(P), Some(state(0)));
        assert!(loaded.take_compost(P).is_none());
        assert_eq!(chance(Item::BREAD), 0.85);
        assert_eq!(chance(Item::BEETROOT), 0.65);
        for n in 0..=8 {
            let b = state(n);
            assert_eq!(b.base(), Block::COMPOSTER);
            assert_eq!(b.drop(), Some(Block::COMPOSTER.into()));
            assert_eq!(loaded.container_signal(P), Some(0));
        }
    }
    #[test]
    fn hoppers_insert_only_above_extract_only_below() {
        let mut w = world();
        w.set_block(P, Block::COMPOSTER);
        let top = P + IVec3::Y;
        let below = P - IVec3::Y;
        let side = P + IVec3::X;
        w.set_block(top, r::hopper(4, false));
        w.set_block(below, r::hopper(4, false));
        w.set_block(side, r::hopper(3, false));
        w.chest_mut(top).unwrap().slots[0] = Some(Stack::new(Item::CAKE, 7));
        w.chest_mut(side).unwrap().slots[0] = Some(Stack::new(Item::CAKE, 7));
        for _ in 0..80 {
            w.tick_redstone();
        }
        assert_eq!(w.chest(top).unwrap().slots[0], None);
        assert_eq!(w.chest(side).unwrap().slots[0], Some(Stack::new(Item::CAKE, 7)));
        assert_eq!(w.chest(below).unwrap().slots[0], Some(Stack::new(Item::BONE_MEAL, 1)));
        assert_eq!(w.get_block(P), Some(state(0)));
    }
}
