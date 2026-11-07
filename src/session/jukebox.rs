//! Jukebox use on every input path: Java 26.3 `JukeboxBlock.useItemOn`
//! (`JukeboxPlayable.tryInsertIntoJukebox`) and `useWithoutItem`
//! (`popOutTheItem`).

use glam::IVec3;

use super::{PlayerSession, interact::Used};
use crate::world::{World, jukebox};

impl PlayerSession {
    /// A jukebox holding a record pops it out whatever is held; an empty
    /// one takes a held disc (Creative players keep theirs) and plays it.
    /// `None` passes the use on (an empty jukebox and no disc).
    pub(super) fn use_jukebox(&mut self, slot: usize, world: &mut World, pos: IVec3) -> Option<Used> {
        let block = world.get_block(pos)?;
        if block == jukebox::JUKEBOX_WITH_RECORD {
            world.eject_disc(pos)?;
            return Some(Used::Jukebox { cell: pos, inserted: false });
        }
        if block != jukebox::JUKEBOX {
            return None;
        }
        let held = self.inventory.get(slot)?;
        jukebox::song_of(held.item)?;
        if !world.insert_disc(pos, held) {
            return None;
        }
        // `ItemStack.consumeAndReturn`: infinite materials keep the stack.
        if !self.mode.is_creative() {
            self.inventory.take_one(slot);
        }
        Some(Used::Jukebox { cell: pos, inserted: true })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{Entities, PlayerId};
    use crate::inventory::Stack;
    use crate::rules::GameMode;
    use crate::session::interact::UseContext;
    use crate::world::block::Block;
    use crate::world::terrain::Generator;
    use glam::DVec3;
    use std::sync::Arc;

    #[test]
    fn shared_use_inserts_and_ejects_discs() {
        let mut world = World::new_headless(Arc::new(Generator::new(42)), Default::default(), 2);
        world.load_test_column(glam::IVec2::ZERO, 0);
        let cell = IVec3::new(2, 11, 1);
        world.set_block(cell, jukebox::JUKEBOX);
        let mut e = Entities::new(1);
        // Standing to the west, looking east at the jukebox.
        let mut actor = PlayerSession::new(PlayerId(3), DVec3::new(0.5, 10.0, 1.5));
        actor.player.yaw = 0.0;
        actor.player.pitch = 0.0;
        actor.set_mode(GameMode::Survival);
        let disc = jukebox::disc(10);
        actor.inventory.slots[0] = Some(Stack::new(disc, 1));
        let ctx = UseContext { sneaking: false, others: &[] };
        let used = actor.use_held(0, &mut world, &mut e, &ctx).unwrap().unwrap();
        assert_eq!(used.used, Used::Jukebox { cell, inserted: true });
        assert_eq!(actor.inventory.get(0), None);
        assert_eq!(world.get_block(cell), Some(jukebox::JUKEBOX_WITH_RECORD));
        assert!(world.jukebox_playing(cell));
        assert_eq!(world.jukebox_comparator(cell), jukebox::SONGS[10].comparator);
        // A second use with anything pops the record out above the jukebox.
        actor.inventory.slots[0] = Some(Stack::new(crate::item::Item::STICK, 1));
        let used = actor.use_held(0, &mut world, &mut e, &ctx).unwrap().unwrap();
        assert_eq!(used.used, Used::Jukebox { cell, inserted: false });
        assert_eq!(world.get_block(cell), Some(jukebox::JUKEBOX));
        assert!(!world.jukebox_playing(cell));
        assert!(world.drops.iter().any(|(_, s)| s.item == disc));
        // A non-disc on an empty jukebox passes to placement.
        let used = actor.use_held(0, &mut world, &mut e, &ctx);
        assert!(!matches!(used, Ok(Some(u)) if matches!(u.used, Used::Jukebox { .. })));
        // Creative players keep the disc.
        world.set_block(cell, jukebox::JUKEBOX);
        actor.set_mode(GameMode::Creative);
        actor.inventory.slots[1] = Some(Stack::new(disc, 1));
        let used = actor.use_held(1, &mut world, &mut e, &ctx).unwrap().unwrap();
        assert_eq!(used.used, Used::Jukebox { cell, inserted: true });
        assert_eq!(actor.inventory.get(1), Some(Stack::new(disc, 1)));
        assert_ne!(world.get_block(cell), Some(Block::AIR));
    }
}
