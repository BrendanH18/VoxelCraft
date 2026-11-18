//! v0.15 Track B: book editing and signing (`handleEditBook`) and the
//! lectern's buttons (`LecternMenu.clickMenuButton`) as shared session
//! rules, so desktop, controller, CLI and LAN players act alike.

use glam::IVec3;

use super::PlayerSession;
use crate::content::books;
use crate::inventory::Stack;
use crate::world::World;

/// `Inventory.SLOT_OFFHAND`, as `ServerboundEditBookPacket` names it.
pub const OFFHAND_SLOT: usize = 40;

impl PlayerSession {
    fn book_slot(&mut self, slot: usize) -> Option<&mut Option<Stack>> {
        // `handleEditBook` accepts the hotbar and the off-hand only.
        if slot == OFFHAND_SLOT {
            Some(&mut self.inventory.offhand)
        } else if slot < crate::inventory::HOTBAR_SLOTS {
            Some(&mut self.inventory.slots[slot])
        } else {
            None
        }
    }

    /// `updateBookContents`: the book and quill in `slot` gets `pages`.
    pub fn edit_book(&mut self, slot: usize, pages: &[String]) -> Result<(), &'static str> {
        let cell = self.book_slot(slot).ok_or("books are edited from the hotbar or off-hand")?;
        let stack = cell.ok_or("there is no book there")?;
        *cell = Some(books::edit(stack, pages).ok_or("that is not a book and quill")?);
        Ok(())
    }

    /// `signBook`: the book and quill in `slot` becomes a written book.
    pub fn sign_book(&mut self, slot: usize, pages: &[String], title: &str, author: &str) -> Result<(), &'static str> {
        let cell = self.book_slot(slot).ok_or("books are signed from the hotbar or off-hand")?;
        let stack = cell.ok_or("there is no book there")?;
        if stack.item != books::WRITABLE_BOOK {
            return Err("that is not a book and quill");
        }
        *cell = Some(books::sign(stack, pages, title, author).ok_or("a signed book needs a title")?);
        Ok(())
    }

    /// The lectern at `pos`, if this player can use it (`stillValid`: the
    /// block's within reach and it holds a book).
    fn usable_lectern(&self, world: &World, pos: IVec3) -> Result<(), &'static str> {
        world.lectern_at(pos).ok_or("there is no book on a lectern there")?;
        let centre = pos.as_dvec3() + glam::DVec3::splat(0.5);
        if self.player.eye().distance(centre) > self.block_reach() + 4.0 {
            return Err("the lectern is too far away");
        }
        Ok(())
    }

    /// Lectern buttons 1, 2 and 100 + n: turn to `page`.
    pub fn turn_lectern_page(&self, world: &mut World, pos: IVec3, page: i64) -> Result<bool, &'static str> {
        self.usable_lectern(world, pos)?;
        Ok(world.set_lectern_page(pos, page))
    }

    /// Lectern button 3: the book goes to this player's inventory (or the
    /// ground at their feet when it's full). Refused when they can't build.
    pub fn take_lectern_book(&mut self, world: &mut World, pos: IVec3) -> Result<Option<Stack>, &'static str> {
        self.usable_lectern(world, pos)?;
        if !self.mode.can_build() {
            return Err("player cannot take books now");
        }
        let book = world.take_lectern_book(pos).ok_or("there is no book on a lectern there")?;
        let left = self.inventory.add_stack(book);
        Ok((left > 0).then_some(Stack { count: left, ..book }))
    }
}
