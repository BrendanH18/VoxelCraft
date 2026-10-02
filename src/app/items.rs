//! Game-side glue for dropped items: turning world drops and inventory
//! spill into item entities, picking them up, throwing items (Q, clicking
//! off the inventory panel), dropping everything on death, and drawing them.

use glam::DVec3;

use crate::audio::sounds::Sound;
use crate::inventory::Stack;
use crate::render::BlockModel;
use crate::world::block::Block;

use super::{Container, Game};

/// Dropped items farther away than this aren't drawn.
const DRAW_DIST: f64 = 64.0;

impl Game {
    /// Spawns what the world dropped and what didn't fit in the inventory,
    /// then picks up items the player is touching. Closes a furnace or chest
    /// screen whose block is gone.
    pub(super) fn update_items(&mut self) {
        for (cell, stack) in std::mem::take(&mut self.world.drops) {
            self.mobs.entities.drop_from_block(stack, cell);
        }
        for stack in self.inventory.take_spill() {
            self.mobs.entities.throw(stack, self.player.eye(), self.player.forward().as_dvec3());
        }
        if !self.vitals.is_dead() {
            self.pick_up_items();
        }
        let gone = match self.container {
            Container::Furnace(pos) => self.world.furnace(pos).is_none(),
            Container::Chest(pos) => self.world.chest(pos).is_none(),
            _ => false,
        };
        if gone && self.inventory_open {
            self.toggle_inventory();
        }
    }

    fn pick_up_items(&mut self) {
        let player = self.player.pos;
        let mut picked = false;
        self.mobs.entities.items.retain_mut(|item| {
            if item.pickup_delay > 0.0 || !item.touches_player(player) {
                return true;
            }
            let left = self.inventory.add_stack(item.stack);
            picked |= left < item.stack.count;
            item.stack.count = left;
            left > 0
        });
        let arrows = self.mobs.entities.collect_arrows(player);
        if arrows > 0 {
            picked = true;
            let left = self.inventory.add(crate::item::Item::ARROW, arrows);
            if left > 0 {
                self.mobs.entities.scatter(Stack::new(crate::item::Item::ARROW, left), player);
            }
        }
        if picked {
            self.audio.play(Sound::Pop, None, 0.35, (0.8, 1.8));
        }
    }

    /// Q: drops one of the selected item, or the whole stack with Ctrl.
    /// With the inventory open, drops from the slot under the mouse.
    pub(super) fn drop_selected(&mut self, whole_stack: bool) {
        let slot = if self.inventory_open {
            match self.slot_under_cursor() {
                Some(super::hud::SlotRef::Inventory(i)) => i,
                _ => return,
            }
        } else {
            self.actions.selected
        };
        let Some(stack) = self.inventory.slots[slot] else { return };
        let count = if whole_stack { stack.count } else { 1 };
        self.inventory.slots[slot] = (stack.count > count).then_some(Stack { count: stack.count - count, ..stack });
        if slot == self.actions.selected {
            self.actions.reset();
        }
        self.throw(Stack { count, ..stack });
    }

    /// Clicking off the inventory panel throws the held stack (or one of
    /// it with the right button). Returns whether the click was used.
    pub(super) fn throw_cursor(&mut self, one: bool) -> bool {
        let Some(stack) = self.inventory.cursor else { return false };
        if !self.cursor_off_panel() {
            return false;
        }
        let count = if one { 1 } else { stack.count };
        self.inventory.cursor = (stack.count > count).then_some(Stack { count: stack.count - count, ..stack });
        self.throw(Stack { count, ..stack });
        true
    }

    fn throw(&mut self, stack: Stack) {
        self.mobs.entities.throw(stack, self.player.eye(), self.player.forward().as_dvec3());
    }

    /// A dying survival player drops their whole inventory, including what
    /// was on the crafting grid and the cursor.
    pub(super) fn drop_everything(&mut self) {
        let mut stacks = self.inventory.take_all();
        stacks.extend(self.craft.take_all());
        for stack in stacks {
            self.mobs.entities.scatter(stack, self.player.pos);
        }
    }

    /// Spinning, bobbing models for nearby dropped items: a small cube for
    /// blocks, a flat icon for everything else, with extra copies for
    /// bigger stacks.
    pub(super) fn item_models(&self) -> Vec<BlockModel> {
        let mut out = Vec::new();
        for item in &self.mobs.entities.items {
            if item.pos.distance_squared(self.player.pos) > DRAW_DIST * DRAW_DIST {
                continue;
            }
            let block = item.stack.item.block().filter(|b| !b.flat_icon());
            let icon = match block {
                Some(_) => None,
                None => item.stack.item.block().map(|b| b.info().tex[0]).or(item.stack.item.icon_layer()),
            };
            let size = if block.is_some() { 0.25 } else { 0.5 };
            let spin = item.age + item.phase;
            let bob = 0.1 + 0.07 * (item.age * 2.0 + item.phase).sin() as f64;
            let sky_light = crate::entity::sky_light(&self.world, item.pos + DVec3::Y * 0.25);
            let copies = match item.stack.count {
                1 => 1,
                2..=16 => 2,
                17..=32 => 3,
                _ => 4,
            };
            for i in 0..copies {
                // Copies sit a little apart, the same way every frame.
                let k = i as f64;
                let offset = DVec3::new((k * 2.3).sin(), 0.0, (k * 1.7).cos()) * 0.06 * k.min(1.0);
                let min = item.pos + offset + DVec3::new(-size / 2.0, bob + k * 0.03, -size / 2.0);
                out.push(BlockModel {
                    min,
                    size: size as f32,
                    block: block.unwrap_or(Block::AIR),
                    sky_light,
                    yaw: spin,
                    icon,
                });
            }
        }
        out
    }
}
