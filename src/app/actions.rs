//! Hotbar selection and action progress, independent of the window and renderer.

use glam::IVec3;

#[derive(Default)]
pub(super) struct Actions {
    pub selected: usize,
    /// Target block and progress toward breaking it (0..1).
    pub breaking: Option<(IVec3, f32)>,
    /// Seconds spent on the current uninterrupted bite.
    pub eat_timer: f64,
    /// Seconds the bow has been drawn, while drawing.
    pub bow_draw: Option<f64>,
}

impl Actions {
    /// Selecting a different slot interrupts both actions, even if its
    /// item is the same kind as the previous one.
    pub fn select(&mut self, slot: usize) -> bool {
        if self.selected == slot {
            return false;
        }
        self.selected = slot;
        self.reset();
        true
    }

    pub fn reset(&mut self) {
        self.breaking = None;
        self.eat_timer = 0.0;
        self.bow_draw = None;
    }

    /// Advances breaking the block at `pos`, which takes `seconds` in all.
    pub fn mine(&mut self, pos: IVec3, seconds: f32, dt: f64) -> f32 {
        let before = self.breaking.filter(|(p, _)| *p == pos).map_or(0.0, |(_, progress)| progress);
        let progress = before + (dt / seconds as f64) as f32;
        self.breaking = Some((pos, progress));
        progress
    }

    /// Advances an uninterrupted bite and reports when it finishes.
    pub fn eat(&mut self, dt: f64) -> bool {
        self.eat_timer += dt;
        if self.eat_timer >= super::EAT_TIME {
            self.eat_timer = 0.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::{Inventory, Stack};
    use crate::item::Item;
    use crate::item::{Tier, ToolKind};
    use crate::world::block::Block;

    #[test]
    fn switching_tools_starts_mining_again_without_using_the_previous_speed() {
        let mut actions = Actions::default();
        let gold = Item::tool(ToolKind::Pickaxe, Tier::Gold);
        let wood = Item::tool(ToolKind::Pickaxe, Tier::Wood);
        let mut inv = Inventory::default();
        inv.slots[0] = Some(Stack::new(gold, 1));
        inv.slots[1] = Some(Stack::new(wood, 1));
        for _ in 0..17 {
            assert!(actions.mine(IVec3::ZERO, crate::mining::break_time(Block::STONE, Some(gold)), 0.01) < 1.0);
        }
        assert!(actions.select(1));
        for _ in 0..12 {
            assert!(actions.mine(IVec3::ZERO, crate::mining::break_time(Block::STONE, Some(wood)), 0.01) < 1.0);
        }
        // Finish the wooden pickaxe's own attempt, then charge its wear.
        while actions.mine(IVec3::ZERO, crate::mining::break_time(Block::STONE, Some(wood)), 0.01) < 1.0 {}
        inv.wear(actions.selected, crate::mining::wear(wood, false));
        assert_eq!(inv.get(0).unwrap().damage, 0);
        assert_eq!(inv.get(1).unwrap().damage, 1);
    }

    #[test]
    fn switching_food_requires_a_full_new_bite() {
        let mut actions = Actions::default();
        let mut inv = Inventory::default();
        inv.slots[0] = Some(Stack::new(Item::RAW_BEEF, 1));
        inv.slots[1] = Some(Stack::new(Item::STEAK, 1));
        for _ in 0..15 {
            assert!(!actions.eat(0.1));
        }
        actions.select(1);
        assert!(!actions.eat(0.11));
        assert!(!actions.eat(1.48));
        assert!(actions.eat(0.02));
        assert_eq!(inv.take_one(actions.selected), Some(Item::STEAK));
        assert_eq!(inv.get(0), Some(Stack::new(Item::RAW_BEEF, 1)));
        assert_eq!(actions.eat_timer, 0.0);
    }

    #[test]
    fn reselecting_the_same_slot_keeps_progress() {
        let mut actions = Actions::default();
        actions.mine(IVec3::ZERO, crate::mining::break_time(Block::STONE, None), 0.1);
        actions.eat(0.1);
        let before = (actions.breaking, actions.eat_timer);
        assert!(!actions.select(0));
        assert_eq!((actions.breaking, actions.eat_timer), before);
        actions.reset();
        assert_eq!((actions.breaking, actions.eat_timer), (None, 0.0));
    }
}
