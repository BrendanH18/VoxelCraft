//! Right-clicks on the v0.12 wood blocks, shared by every input path:
//! signs (Java 26.3 `SignBlock.useItemOn`/`useWithoutItem` with dyes, ink
//! sacs and glow ink sacs), shelves (`ShelfBlock.useItemOn`) and chiseled
//! bookshelves (`ChiseledBookShelfBlock`). Editing a sign's text is the
//! adapter's screen; it reports back through [`PlayerSession::edit_sign`].
use super::PlayerSession;
use super::interact::Used;
use crate::inventory::{HOTBAR_SLOTS, Stack};
use crate::item::Item;
use crate::world::World;
use crate::world::wood::{bookshelf, shelves, signs};
use glam::IVec3;

impl PlayerSession {
    /// Whether the use at `pos` belongs to a wood block before the held item
    /// (`content::interaction::block_use_precedes_item`).
    pub fn wood_block_takes_use(&self, world: &World, pos: IVec3) -> bool {
        world
            .get_block(pos)
            .is_some_and(|b| signs::sign(b).is_some() || shelves::shelf(b).is_some() || bookshelf::slots(b).is_some())
    }

    /// The use chain's step for signs, shelves and chiseled bookshelves;
    /// `None` passes the click on (to placing the held item).
    pub(super) fn use_wood_block(&mut self, slot: usize, world: &mut World, sneaking: bool) -> Option<Used> {
        let hit = self.block_hit(world)?;
        let (pos, face, point) = (hit.cell, hit.normal, hit.point);
        let block = world.get_block(pos)?;
        let held = self.inventory.get(slot);
        // Sneaking with an item skips block use (`ServerPlayerGameMode.useItemOn`).
        if sneaking && held.is_some() {
            return None;
        }
        let creative = self.mode.is_creative();
        if let Some(sign) = signs::sign(block) {
            return self.use_sign(slot, world, pos, face, sign);
        }
        if shelves::shelf(block).is_some() {
            if !self.mode.can_build() {
                return None;
            }
            let hotbar = &mut self.inventory.slots[..HOTBAR_SLOTS];
            let used = world.use_shelf(pos, face, point, hotbar, slot, creative)?;
            return Some(Used::Shelf { cell: pos, used });
        }
        if bookshelf::slots(block).is_some() {
            if !self.mode.can_build() {
                return None;
            }
            let (used, book) = world.use_bookshelf(pos, face, point, held)?;
            // `consumeAndReturn(1)`: creative players keep their book.
            if matches!(used, crate::world::wood::ShelfUse::BookIn { .. }) && !creative {
                self.inventory.take_one(slot);
            }
            // `removeBook`: into the inventory, or dropped at the player.
            if let Some(book) = book {
                let left = self.inventory.add_stack(book);
                if left > 0 {
                    let feet = self.player.pos.floor().as_ivec3();
                    world.drops.push((feet, Stack { count: left, ..book }));
                }
            }
            return Some(Used::Shelf { cell: pos, used });
        }
        None
    }

    fn use_sign(&mut self, slot: usize, world: &mut World, pos: IVec3, face: IVec3, sign: signs::Sign) -> Option<Used> {
        let held = self.inventory.get(slot).map(|s| s.item);
        // A hanging sign item aimed under a hanging sign (or off a wall
        // hanging sign's faces) places another one below it.
        if held.and_then(Item::places).and_then(signs::sign).is_some_and(|s| s.hanging()) {
            match sign.mount {
                signs::Mount::Hanging { .. } if face == IVec3::NEG_Y => return None,
                signs::Mount::WallHanging { facing }
                    if !(face.x != 0 && facing.along_x() || face.z != 0 && !facing.along_x()) =>
                {
                    return None;
                }
                _ => {}
            }
        }
        let front = sign.faces_front(pos, self.player.pos);
        let (_, entity) = world.sign_at(pos)?;
        let waxed = entity.waxed;
        let has_message = entity.side(front).has_message();
        let may_build = self.mode.can_build();
        let applicator = held.filter(|&i| i == Item::INK_SAC || i == Item::GLOW_INK_SAC || i.dye_color().is_some());
        if let Some(item) = applicator
            && may_build
            && !waxed
            && has_message
        {
            let changed = world.update_sign(pos, front, |t| match item {
                Item::INK_SAC => t.glowing = false,
                Item::GLOW_INK_SAC => t.glowing = true,
                _ => t.color = item.dye_color().unwrap_or(t.color),
            });
            if changed {
                if !self.mode.is_creative() {
                    self.inventory.take_one(slot);
                }
                return Some(Used::SignInked { cell: pos, item });
            }
        }
        // `useWithoutItem`: open the text editor for the side faced.
        if !waxed && may_build {
            return Some(Used::EditSign { cell: pos, front });
        }
        None
    }

    /// The edit screen's result (Java's `ServerboundSignUpdatePacket`):
    /// the player must still be able to build and be within reach.
    pub fn edit_sign(
        &mut self,
        world: &mut World,
        pos: IVec3,
        front: bool,
        lines: [String; 4],
    ) -> Result<(), &'static str> {
        if !self.mode.can_build() {
            return Err("player cannot edit signs now");
        }
        let centre = pos.as_dvec3() + glam::DVec3::splat(0.5);
        // `playerIsTooFarAwayToEdit`: block interaction range plus 4.
        if self.player.eye().distance(centre) > self.block_reach() + 4.0 {
            return Err("sign is too far away");
        }
        world.set_sign_text(pos, front, lines)
    }
}
