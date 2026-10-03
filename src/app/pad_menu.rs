//! Controller screens for local players, drawn inside their own view: a
//! pause menu (Start), the inventory with armor (Y), crafting and chests.
//! The world keeps running, like Bedrock split-screen.
//!
//! The D-pad or left stick moves the cursor. A clicks a slot like a left
//! click (take, place, swap or merge, using the inventory cursor), X like a
//! right click (half a stack, or one item), and Y moves the stack to the
//! other side. LB/RB switch between inventory and crafting; B closes.

use glam::IVec3;

use super::Game;
use super::hud::draw_stack;
use crate::inventory::{Stack, click_slot, move_into};
use crate::item::ArmorPiece;
use crate::render::ui::{Ui, WHITE};
use voxelcraft::agent::Command;

/// Controller buttons as menu navigation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Nav {
    Up,
    Down,
    Left,
    Right,
    A,
    B,
    X,
    Y,
    Lb,
    Rb,
    Start,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Tab {
    Inventory,
    Crafting,
    Chest(IVec3),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Menu {
    Pause { choice: usize },
    Items { tab: Tab, col: usize, row: usize },
}

/// A slot under the menu cursor.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Slot {
    Armor(usize),
    Inv(usize),
    Chest(usize),
    Craft(usize),
}

/// What a button press asks the game to do.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Action {
    None,
    Close,
    Leave,
    Click(Slot, bool),
    QuickMove(Slot),
}

const PAUSE_CHOICES: [&str; 2] = ["Resume", "Leave game"];
const COLS: usize = 9;

impl Menu {
    pub fn items(tab: Tab) -> Self {
        // Start on the hotbar, or the first recipe.
        let row = if tab == Tab::Crafting { 0 } else { rows(tab, 0) - 1 };
        Menu::Items { tab, col: 0, row }
    }

    /// Apply one press. `crafts` is how many recipes are craftable now.
    pub fn press(&mut self, nav: Nav, crafts: usize) -> Action {
        match self {
            Menu::Pause { choice } => match nav {
                Nav::Up | Nav::Down => {
                    *choice = 1 - *choice;
                    Action::None
                }
                Nav::A if *choice == 1 => Action::Leave,
                Nav::A | Nav::B | Nav::Start => Action::Close,
                _ => Action::None,
            },
            Menu::Items { tab, col, row } => {
                let rows = rows(*tab, crafts);
                // Crafting can shrink the list under the cursor.
                *row = (*row).min(rows.saturating_sub(1));
                match nav {
                    Nav::Up | Nav::Down | Nav::Left | Nav::Right if rows == 0 => {}
                    Nav::Up => *row = (*row + rows - 1) % rows,
                    Nav::Down => *row = (*row + 1) % rows,
                    Nav::Left => *col = (*col + row_len(*tab, *row, crafts) - 1) % row_len(*tab, *row, crafts),
                    Nav::Right => *col = (*col + 1) % row_len(*tab, *row, crafts),
                    Nav::A | Nav::X | Nav::Y if rows == 0 => {}
                    Nav::A => return Action::Click(slot(*tab, *col, *row), false),
                    Nav::X => return Action::Click(slot(*tab, *col, *row), true),
                    Nav::Y => return Action::QuickMove(slot(*tab, *col, *row)),
                    Nav::Lb | Nav::Rb => {
                        let next = match *tab {
                            Tab::Inventory => Tab::Crafting,
                            Tab::Crafting => Tab::Inventory,
                            chest => chest,
                        };
                        *self = Menu::items(next);
                        return Action::None;
                    }
                    Nav::B | Nav::Start => return Action::Close,
                }
                if rows > 0 {
                    *col = (*col).min(row_len(*tab, *row, crafts) - 1);
                }
                Action::None
            }
        }
    }

    /// The slot under the cursor, if any.
    #[cfg(test)]
    pub fn slot(&self, crafts: usize) -> Option<Slot> {
        match *self {
            Menu::Items { tab, col, row } if rows(tab, crafts) > 0 => Some(slot(tab, col, row)),
            _ => None,
        }
    }
}

/// Inventory: armor, three main rows, hotbar. Chest: three chest rows, then
/// the player's. Crafting: nine results to a row.
fn rows(tab: Tab, crafts: usize) -> usize {
    match tab {
        Tab::Inventory => 5,
        Tab::Chest(_) => 7,
        Tab::Crafting => crafts.div_ceil(COLS),
    }
}

fn row_len(tab: Tab, row: usize, crafts: usize) -> usize {
    match tab {
        Tab::Inventory if row == 0 => 4,
        Tab::Crafting if row + 1 == rows(tab, crafts) && !crafts.is_multiple_of(COLS) => crafts % COLS,
        _ => COLS,
    }
}

fn slot(tab: Tab, col: usize, row: usize) -> Slot {
    match tab {
        Tab::Inventory if row == 0 => Slot::Armor(col),
        Tab::Inventory if row == 4 => Slot::Inv(col),
        Tab::Inventory => Slot::Inv(COLS * row + col),
        Tab::Chest(_) if row < 3 => Slot::Chest(COLS * row + col),
        Tab::Chest(_) if row == 6 => Slot::Inv(col),
        Tab::Chest(_) => Slot::Inv(COLS * (row - 2) + col),
        Tab::Crafting => Slot::Craft(COLS * row + col),
    }
}

impl Game {
    /// Results the profile can craft now (empty for unknown names).
    pub(super) fn pad_crafts(&self, name: &str) -> Vec<Stack> {
        self.agents.players.get(name).map_or_else(Vec::new, |b| b.agent.craftable(&self.world))
    }

    /// Do what a menu press asked for `name`. Returns false to close the menu.
    pub(super) fn apply_menu(&mut self, name: &str, menu: Menu, action: Action) -> bool {
        let crafts =
            if matches!(action, Action::Click(Slot::Craft(_), _)) { self.pad_crafts(name) } else { Vec::new() };
        let Some(bot) = self.agents.players.get_mut(name) else { return false };
        let agent = &mut bot.agent;
        let inv = &mut agent.inventory;
        let chest_pos = match menu {
            Menu::Items { tab: Tab::Chest(pos), .. } => Some(pos),
            _ => None,
        };
        match action {
            Action::None => {}
            Action::Close | Action::Leave => return false,
            Action::Click(Slot::Armor(i), right) => inv.click_armor(ArmorPiece::ALL[i], right),
            Action::Click(Slot::Inv(i), right) => inv.click(i, right),
            Action::Click(Slot::Chest(i), right) => {
                if let Some(chest) = chest_pos.and_then(|p| self.world.chest_mut(p)) {
                    click_slot(&mut chest.slots[i], &mut inv.cursor, right);
                }
            }
            Action::Click(Slot::Craft(i), _) => {
                if let Some(stack) = crafts.get(i) {
                    let _ = agent.execute(Command::Craft(stack.item), &mut self.world, &mut self.mobs.entities, &[]);
                }
            }
            Action::QuickMove(Slot::Armor(i)) => {
                if let Some(stack) = inv.armor[i].take() {
                    let left = inv.add_stack(stack);
                    inv.armor[i] = (left > 0).then_some(Stack { count: left, ..stack });
                }
            }
            Action::QuickMove(Slot::Inv(i)) => {
                let Some(stack) = inv.slots[i].take() else { return true };
                if let Some(chest) = chest_pos.and_then(|p| self.world.chest_mut(p)) {
                    let order: Vec<usize> = (0..chest.slots.len()).collect();
                    inv.slots[i] = move_into(stack, &mut chest.slots, &order);
                } else if stack.item.as_armor().is_some() {
                    inv.slots[i] = Some(stack);
                    inv.equip(i);
                } else {
                    // Between the hotbar and the main grid, like shift-click.
                    let order: Vec<usize> = if i < COLS { (COLS..36).collect() } else { (0..COLS).collect() };
                    inv.slots[i] = move_into(stack, &mut inv.slots, &order);
                }
            }
            Action::QuickMove(Slot::Chest(i)) => {
                if let Some(chest) = chest_pos.and_then(|p| self.world.chest_mut(p))
                    && let Some(stack) = chest.slots[i]
                {
                    let left = inv.add_stack(stack);
                    chest.slots[i] = (left > 0).then_some(Stack { count: left, ..stack });
                }
            }
            Action::QuickMove(Slot::Craft(_)) => {}
        }
        true
    }

    /// Closing a screen returns the stack on the cursor to the inventory,
    /// throwing whatever doesn't fit.
    pub(super) fn close_pad_menu(&mut self, name: &str) {
        let Some(bot) = self.agents.players.get_mut(name) else { return };
        let agent = &mut bot.agent;
        if let Some(stack) = agent.inventory.cursor.take() {
            agent.inventory.return_stacks([stack]);
        }
        for stack in agent.inventory.take_spill() {
            self.mobs.entities.throw(stack, agent.player.eye(), agent.player.forward().as_dvec3());
        }
    }

    /// Draws `menu` over a controller player's view.
    pub(super) fn pad_menu_ui(&self, ui: &mut Ui, name: &str, menu: Menu) {
        let (sw, sh) = ui.size();
        ui.rect(0.0, 0.0, sw, sh, [0.0, 0.0, 0.0, 0.45]);
        let Some(bot) = self.agents.players.get(name) else { return };
        let inv = &bot.agent.inventory;
        let (tab, col, row) = match menu {
            Menu::Pause { choice } => {
                let title = format!("{name}: paused");
                ui.text(((sw - Ui::text_width(&title)) / 2.0).floor(), (sh / 2.0 - 30.0).floor(), &title, WHITE);
                for (i, label) in PAUSE_CHOICES.iter().enumerate() {
                    let (w, y) = (100.0, (sh / 2.0 - 10.0 + 22.0 * i as f32).floor());
                    let x = ((sw - w) / 2.0).floor();
                    let lit = i == choice;
                    ui.rect(x, y, w, 18.0, if lit { [0.45, 0.5, 0.75, 0.95] } else { [0.2, 0.2, 0.2, 0.9] });
                    ui.text(((sw - Ui::text_width(label)) / 2.0).floor(), y + 5.0, label, WHITE);
                }
                return;
            }
            Menu::Items { tab, col, row } => (tab, col, row),
        };
        let crafts = if tab == Tab::Crafting { self.pad_crafts(name) } else { Vec::new() };
        let n = rows(tab, crafts.len());
        // Hotbar rows sit a little apart, like Minecraft's.
        let gap = |r: usize| match tab {
            Tab::Inventory => 4.0 * ((r >= 1) as u8 + (r >= 4) as u8) as f32,
            Tab::Chest(_) => 4.0 * ((r >= 3) as u8 + (r >= 6) as u8) as f32,
            Tab::Crafting => 0.0,
        };
        let pw = COLS as f32 * 18.0 + 12.0;
        let ph = 18.0 * n.max(1) as f32 + gap(n.saturating_sub(1)) + 26.0;
        let (px, py) = (((sw - pw) / 2.0).floor(), ((sh - ph) / 2.0).floor().max(0.0));
        ui.rect(px, py, pw, ph, [0.12, 0.12, 0.14, 0.92]);
        let title = match tab {
            Tab::Inventory => "Inventory  (RB: crafting)",
            Tab::Crafting => "Crafting  (LB: inventory)",
            Tab::Chest(_) => "Chest",
        };
        ui.text(px + 6.0, py + 5.0, title, WHITE);
        let chest = match tab {
            Tab::Chest(pos) => self.world.chest(pos),
            _ => None,
        };
        let mut hovered = None;
        for r in 0..n {
            for c in 0..row_len(tab, r, crafts.len()) {
                let (x, y) = (px + 6.0 + 18.0 * c as f32, py + 18.0 + 18.0 * r as f32 + gap(r));
                ui.rect(x, y, 17.0, 17.0, [0.3, 0.3, 0.32, 0.95]);
                let stack = match slot(tab, c, r) {
                    Slot::Armor(i) => inv.armor[i],
                    Slot::Inv(i) => inv.slots[i],
                    Slot::Chest(i) => chest.and_then(|ch| ch.slots[i]),
                    Slot::Craft(i) => crafts.get(i).copied(),
                };
                if let Some(stack) = stack {
                    draw_stack(ui, x - 0.5, y - 0.5, stack, true);
                }
                if (c, r) == (col, row) {
                    ui.rect(x, y, 17.0, 17.0, [1.0, 1.0, 1.0, 0.35]);
                    hovered = Some((stack, x, y));
                }
            }
        }
        if n == 0 {
            ui.text(px + 6.0, py + 20.0, "Nothing to craft", [0.7, 0.7, 0.7, 1.0]);
        }
        if let Some((stack, x, y)) = hovered {
            if let Some(held) = inv.cursor {
                draw_stack(ui, x + 6.0, y + 6.0, held, true);
            } else if let Some(stack) = stack {
                let name = stack.item.name();
                let tx = (x + 9.0 - Ui::text_width(name) / 2.0).clamp(0.0, (sw - Ui::text_width(name)).max(0.0));
                let ty = (py + ph + 2.0).min(sh - 10.0);
                ui.rect(tx - 2.0, ty - 1.0, Ui::text_width(name) + 4.0, 10.0, [0.0, 0.0, 0.0, 0.8]);
                ui.text(tx, ty, name, WHITE);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_cursor_wraps_and_clamps_to_the_armor_row() {
        let mut m = Menu::items(Tab::Inventory);
        assert_eq!(m.slot(0), Some(Slot::Inv(0)), "starts on the hotbar");
        m.press(Nav::Left, 0);
        assert_eq!(m.slot(0), Some(Slot::Inv(8)));
        m.press(Nav::Down, 0);
        assert_eq!(m.slot(0), Some(Slot::Armor(3)), "wraps to armor, clamped to four columns");
        m.press(Nav::Down, 0);
        assert_eq!(m.slot(0), Some(Slot::Inv(9 + 3)));
        assert_eq!(m.press(Nav::A, 0), Action::Click(Slot::Inv(12), false));
        assert_eq!(m.press(Nav::X, 0), Action::Click(Slot::Inv(12), true));
        assert_eq!(m.press(Nav::B, 0), Action::Close);
    }

    #[test]
    fn chest_rows_map_to_chest_then_player_slots() {
        let pos = IVec3::new(1, 2, 3);
        let mut m = Menu::items(Tab::Chest(pos));
        assert_eq!(m.slot(0), Some(Slot::Inv(0)));
        m.press(Nav::Down, 0);
        assert_eq!(m.slot(0), Some(Slot::Chest(0)));
        m.press(Nav::Up, 0);
        m.press(Nav::Up, 0);
        assert_eq!(m.slot(0), Some(Slot::Inv(27)), "bottom main row");
        assert_eq!(m.press(Nav::Y, 0), Action::QuickMove(Slot::Inv(27)));
        m.press(Nav::Rb, 0);
        assert!(matches!(m, Menu::Items { tab: Tab::Chest(_), .. }), "chests don't switch tabs");
    }

    #[test]
    fn crafting_grid_follows_the_recipe_count() {
        let mut m = Menu::items(Tab::Inventory);
        m.press(Nav::Rb, 11);
        assert_eq!(m.slot(11), Some(Slot::Craft(0)));
        m.press(Nav::Down, 11);
        m.press(Nav::Right, 11);
        m.press(Nav::Right, 11);
        assert_eq!(m.slot(11), Some(Slot::Craft(9)), "second row has two results and wraps");
        assert_eq!(m.press(Nav::A, 11), Action::Click(Slot::Craft(9), false));
        // Nothing craftable: no slot, and presses do nothing.
        assert_eq!(m.slot(0), None);
        assert_eq!(m.press(Nav::A, 0), Action::None);
        m.press(Nav::Lb, 0);
        assert!(matches!(m, Menu::Items { tab: Tab::Inventory, .. }));
    }

    #[test]
    fn pause_offers_resume_and_leave() {
        let mut m = Menu::Pause { choice: 0 };
        assert_eq!(m.press(Nav::A, 0), Action::Close);
        m.press(Nav::Down, 0);
        assert_eq!(m.press(Nav::A, 0), Action::Leave);
        assert_eq!(m.press(Nav::Start, 0), Action::Close);
    }
}
