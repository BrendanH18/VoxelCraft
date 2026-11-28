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
/// Shared, version-specific composting eligibility.
pub use crate::content::compost::chance;

/// v1.0 gap Track 2h: the composter's sounds (`BLOCKS`, volume and pitch 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cue {
    /// `COMPOSTER_FILL`: level event 1500 that added no layer.
    Fill,
    /// `COMPOSTER_FILL_SUCCESS`: level event 1500 that added a layer.
    FillSuccess,
    /// `COMPOSTER_EMPTY`: `extractProduce` (a player or a farmer).
    Empty,
    /// `COMPOSTER_READY`: the scheduled tick from level 7 to 8.
    Ready,
}

impl Cue {
    pub const ALL: [Cue; 4] = [Cue::Fill, Cue::FillSuccess, Cue::Empty, Cue::Ready];

    /// The Java sound event.
    pub fn key(self) -> &'static str {
        match self {
            Cue::Fill => "block.composter.fill",
            Cue::FillSuccess => "block.composter.fill_success",
            Cue::Empty => "block.composter.empty",
            Cue::Ready => "block.composter.ready",
        }
    }
}

/// The most composter sounds kept for the game between drains.
const CUE_CAPACITY: usize = 256;

/// v1.0 gap Track 2h: the top of the composter's shape over its centre at
/// `level` (`ComposterBlock.SHAPES`: the inner hollow starts at
/// `max(2, 1 + 2 * level)` pixels; a full composter is a whole block).
pub fn centre_height(level: u8) -> f64 {
    if level >= 8 { 1.0 } else { f64::from((1 + 2 * u32::from(level)).max(2)) / 16.0 }
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
        let added = level == 0 || roll < chance;
        if added {
            self.set_block(p, state(level + 1));
            if level == 6 {
                self.schedule_redstone(p, 20, 0);
            }
        }
        // `useItemOn` and `InputContainer.setChanged`: level event 1500.
        self.composter_filled(p, added);
        true
    }

    /// v1.0 gap Track 2h: level event 1500 (`ComposterBlock.handleFill`):
    /// the fill sound (`fill_success` when a layer was added) and ten
    /// `composter` particles.
    pub fn composter_filled(&mut self, p: IVec3, added: bool) {
        self.composter_cue(p, if added { Cue::FillSuccess } else { Cue::Fill });
        self.particles.push(crate::particles::Request::Composter { cell: p });
    }

    /// v1.0 gap Track 2h: queues one of the composter's sounds at `p`.
    pub fn composter_cue(&mut self, p: IVec3, cue: Cue) {
        if self.composter_cues.len() < CUE_CAPACITY {
            self.composter_cues.push((p, cue));
        }
    }

    /// v1.0 gap Track 2h: the composter sounds since the last call.
    pub fn take_composter_cues(&mut self) -> Vec<(IVec3, Cue)> {
        std::mem::take(&mut self.composter_cues)
    }
    /// v1.0 gap Track 2g: a farmer's `WorkAtComposter` took the composter
    /// at `p` from level `from` (8: `extractProduce` emptied it, its bone
    /// meal already dropped) to `to`, if it is still at `from`.
    /// v1.0 gap Track 2h: emptying plays `block.composter.empty`, and every
    /// `compostItems` ends with level event 1500 (`spawnComposterFillEffects`),
    /// a success when it added a layer.
    pub fn villager_compost(&mut self, p: IVec3, from: u8, to: u8) {
        if self.get_block(p).and_then(level) != Some(from) || to > 7 {
            return;
        }
        let mut now = from;
        if from == 8 {
            self.set_block(p, state(0));
            self.composter_cue(p, Cue::Empty);
            now = 0;
        }
        if to != now {
            self.set_block(p, state(to));
            if to == 7 {
                self.schedule_redstone(p, 20, 0);
            }
        }
        self.composter_filled(p, to != now);
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
            // v1.0 gap Track 2h: `tick` plays `block.composter.ready`.
            self.composter_cue(p, Cue::Ready);
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
    /// v1.0 gap Track 2h: level event 1500 for every fill (the player's,
    /// a hopper's, a farmer's): `fill_success` with a layer, `fill`
    /// without, and ten `composter` particles over the compost's centre;
    /// `extractProduce` plays `empty` and the ripening tick `ready`.
    #[test]
    fn composters_sound_and_shed_particles() {
        use crate::particles::{Kind, Request, Setting, System};
        let mut w = world();
        let composter_requests = |w: &mut World| {
            let mut n = 0;
            while let Some(r) = w.particles.pop() {
                n += usize::from(matches!(r, Request::Composter { cell } if cell == P));
            }
            n
        };
        w.set_block(P, Block::COMPOSTER);
        assert!(w.compost(P, Item::WHEAT_SEEDS));
        assert_eq!(w.take_composter_cues(), vec![(P, Cue::FillSuccess)]);
        assert_eq!(composter_requests(&mut w), 1);
        let mut system = System::new(5);
        system.request(Request::Composter { cell: P }, Setting::All, &w);
        let floor = P.as_dvec3().y + centre_height(1) + 0.03125;
        assert_eq!(system.pool.iter().count(), 10);
        for p in system.pool.iter() {
            assert_eq!(p.style, Kind::Composter);
            assert!((3..=7).contains(&p.lifetime), "{}", p.lifetime);
            assert!(p.pos.y >= floor && p.pos.y <= P.as_dvec3().y + 1.0);
            let x = p.pos.x - P.as_dvec3().x;
            assert!((0.1875..=0.8125).contains(&x));
        }
        // A farmer emptying a full composter and adding nothing.
        w.set_block(P, state(8));
        w.villager_compost(P, 8, 0);
        assert_eq!(w.get_block(P), Some(state(0)));
        assert_eq!(w.take_composter_cues(), vec![(P, Cue::Empty), (P, Cue::Fill)]);
        assert_eq!(composter_requests(&mut w), 1);
        // Filling to 7 ripens after 20 ticks with `ready`.
        w.villager_compost(P, 0, 7);
        assert_eq!(w.take_composter_cues(), vec![(P, Cue::FillSuccess)]);
        for _ in 0..20 {
            w.tick_redstone();
        }
        assert_eq!(w.get_block(P), Some(state(8)));
        assert_eq!(w.take_composter_cues(), vec![(P, Cue::Ready)]);
        assert_eq!([0, 1, 6, 7, 8].map(centre_height), [2.0 / 16.0, 3.0 / 16.0, 13.0 / 16.0, 15.0 / 16.0, 1.0]);
    }

    /// v1.0 gap Track 2h: level event 1505 for whoever uses bone meal: the
    /// sound and the happy-villager particles, 15 within a crop's cell up
    /// to its shape's top, `count * 3` over the blocks above grass.
    #[test]
    fn bone_meal_sounds_and_sheds_happy_villager_particles() {
        use crate::particles::{Kind, Request, Setting, System};
        use crate::world::growth::{BoneMealKind, bone_meal_kind};
        let mut w = world();
        w.set_block(P - IVec3::Y, Block::FARMLAND);
        w.set_block(P, Block::wheat(0));
        assert!(w.use_bone_meal(P));
        assert_eq!(w.take_bone_meal_sounds(), vec![P]);
        let mut found = false;
        while let Some(r) = w.particles.pop() {
            found |= matches!(r, Request::Growth { cell, count: 15, bone_meal: true } if cell == P);
        }
        assert!(found);
        let mut system = System::new(9);
        system.request(Request::Growth { cell: P, count: 15, bone_meal: true }, Setting::All, &w);
        let top = f64::from(w.outline(P).1[1]);
        assert_eq!(system.pool.iter().count(), 15);
        for p in system.pool.iter() {
            assert_eq!(p.style, Kind::HappyVillager);
            assert!(p.pos.floor().as_ivec3().x == P.x && p.pos.floor().as_ivec3().z == P.z);
            assert!(p.pos.y >= P.as_dvec3().y && p.pos.y <= P.as_dvec3().y + top);
            assert!((20..=100).contains(&p.lifetime), "{}", p.lifetime);
        }
        // Grass spreads its 45 over the blocks above; only those over a
        // block show (none here, over air beyond the grass).
        w.set_block(P, Block::GRASS);
        let mut system = System::new(9);
        system.request(Request::Growth { cell: P, count: 15, bone_meal: true }, Setting::All, &w);
        assert!(system.pool.iter().all(|p| p.pos.floor().as_ivec3() - IVec3::Y == P));
        // Nothing grew: no bone meal used, no effect.
        w.set_block(P, Block::STONE);
        assert!(!w.use_bone_meal(P));
        assert!(w.take_bone_meal_sounds().is_empty());
        assert_eq!(bone_meal_kind(Block::wheat(3)), Some(BoneMealKind::Grower));
        assert_eq!(bone_meal_kind(Block::GRASS), Some(BoneMealKind::Spreader));
        assert_eq!(bone_meal_kind(Block::WATER), Some(BoneMealKind::Water));
        assert_eq!(bone_meal_kind(crate::world::overworld_blocks::ROOTED_DIRT), Some(BoneMealKind::GrowerBelow));
        assert_eq!(bone_meal_kind(crate::world::sniffer_plants::TORCHFLOWER), None);
        assert_eq!(bone_meal_kind(Block::STONE), None);
        assert_eq!(bone_meal_kind(Block::AIR), None);
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
