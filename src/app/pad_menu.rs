//! Controller screens for local players, drawn inside their own view: a
//! pause menu (Start), the inventory with armor (Y), crafting, the creative
//! palette, chests and furnaces. The world keeps running, like Bedrock
//! split-screen.
//!
//! The D-pad or left stick moves the cursor. A clicks a slot like a left
//! click (take, place, swap or merge, using the inventory cursor), X like a
//! right click (half a stack, or one item), and Y like a shift-click. LB/RB
//! switch between inventory, crafting and (in creative) the palette; B closes.
//! Slot clicks run the host's own container code as the controller player
//! (see `Game::puppet`), so every rule matches the mouse.

use glam::IVec3;

use super::hud::{SlotRef, draw_stack};
use super::{Container, Game};
use crate::inventory::Stack;
use crate::item::{ArmorPiece, Item};
use crate::render::ui::{Ui, WHITE};
use crate::world::block::Block;
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
    /// Creative players' item palette.
    Palette,
    Chest(IVec3),
    Furnace(IVec3),
    Brewing(IVec3),
    Enchanting(IVec3),
    Anvil(IVec3),
    Grindstone(IVec3),
    Trading(u64),
    Smithing(IVec3),
}

impl Tab {
    /// Whether the workstation this tab was opened for still exists.
    pub fn matches_block(self, block: Block) -> bool {
        match self {
            Tab::Chest(_) => crate::world::chest::is_chest(block),
            Tab::Furnace(_) => crate::world::furnace::is_furnace(block),
            Tab::Brewing(_) => block == Block::BREWING_STAND,
            Tab::Enchanting(_) => block == Block::ENCHANTING_TABLE,
            Tab::Anvil(_) => block.is_anvil(),
            Tab::Grindstone(_) => block.base() == Block::GRINDSTONE,
            Tab::Smithing(_) => block == Block::SMITHING_TABLE,
            _ => false,
        }
    }

    /// The host container screen whose rules this tab follows.
    pub fn container(self) -> Container {
        match self {
            Tab::Chest(p) => Container::Chest(p),
            Tab::Furnace(p) => Container::Furnace(p),
            Tab::Brewing(p) => Container::Brewing(p),
            Tab::Enchanting(p) => Container::Enchanting(p),
            Tab::Anvil(p) => Container::Anvil(p),
            Tab::Grindstone(p) => Container::Grindstone(p),
            Tab::Trading(id) => Container::Trading(id),
            Tab::Smithing(p) => Container::Smithing(p),
            _ => Container::Inventory,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Menu {
    Pause { choice: usize },
    Items { tab: Tab, col: usize, row: usize },
}

/// A slot under the menu cursor.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Slot {
    /// A slot of the host's container screens.
    Ref(SlotRef),
    /// The nth craftable result.
    Recipe(usize),
    /// The nth creative palette item.
    Palette(usize),
}

/// What a button press asks the game to do.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Action {
    None,
    Close,
    Leave,
    Click(Slot, bool),
    QuickMove(Slot),
}

/// Lengths of the list tabs: craftable results, and palette items (zero
/// outside creative, which hides that tab).
#[derive(Clone, Copy, Default, Debug)]
pub(super) struct Lists {
    pub crafts: usize,
    pub palette: usize,
}

const PAUSE_CHOICES: [&str; 2] = ["Resume", "Leave game"];
const COLS: usize = 9;
const FURNACE: [SlotRef; 3] = [SlotRef::FurnaceInput, SlotRef::FurnaceFuel, SlotRef::FurnaceOutput];
const ENCHANTING: [SlotRef; 5] = [
    SlotRef::EnchantItem,
    SlotRef::EnchantLapis,
    SlotRef::EnchantOffer(0),
    SlotRef::EnchantOffer(1),
    SlotRef::EnchantOffer(2),
];
const ANVIL: [SlotRef; 3] = [SlotRef::AnvilLeft, SlotRef::AnvilRight, SlotRef::AnvilResult];
const SMITHING: [SlotRef; 4] =
    [SlotRef::SmithTemplate, SlotRef::SmithBase, SlotRef::SmithAddition, SlotRef::SmithResult];
const BREWING: [SlotRef; 5] = [
    SlotRef::BrewFuel,
    SlotRef::BrewIngredient,
    SlotRef::BrewBottle(0),
    SlotRef::BrewBottle(1),
    SlotRef::BrewBottle(2),
];

impl Menu {
    pub fn items(tab: Tab) -> Self {
        // Start on the hotbar, or the first entry of a list.
        let row = match tab {
            Tab::Crafting | Tab::Palette | Tab::Trading(_) => 0,
            _ => rows(tab, Lists::default()) - 1,
        };
        Menu::Items { tab, col: 0, row }
    }

    /// Apply one press.
    pub fn press(&mut self, nav: Nav, lists: Lists) -> Action {
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
                let rows = rows(*tab, lists);
                // Crafting can shrink the list under the cursor.
                *row = (*row).min(rows.saturating_sub(1));
                let len = |r: usize| row_len(*tab, r, lists);
                match nav {
                    Nav::Up | Nav::Down | Nav::Left | Nav::Right | Nav::A | Nav::X | Nav::Y if rows == 0 => {}
                    Nav::Up => *row = (*row + rows - 1) % rows,
                    Nav::Down => *row = (*row + 1) % rows,
                    Nav::Left => *col = (*col + len(*row) - 1) % len(*row),
                    Nav::Right => *col = (*col + 1) % len(*row),
                    Nav::A => return Action::Click(slot(*tab, *col, *row), false),
                    Nav::X => return Action::Click(slot(*tab, *col, *row), true),
                    Nav::Y => return Action::QuickMove(slot(*tab, *col, *row)),
                    Nav::Lb | Nav::Rb => {
                        let mut cycle = vec![Tab::Inventory, Tab::Crafting];
                        if lists.palette > 0 {
                            cycle.push(Tab::Palette);
                        }
                        // Containers keep their own screen.
                        if let Some(i) = cycle.iter().position(|t| t == tab) {
                            let step = if nav == Nav::Rb { 1 } else { cycle.len() - 1 };
                            *self = Menu::items(cycle[(i + step) % cycle.len()]);
                        }
                        return Action::None;
                    }
                    Nav::B | Nav::Start => return Action::Close,
                }
                if rows > 0 {
                    *col = (*col).min(len(*row) - 1);
                }
                Action::None
            }
        }
    }

    /// The slot under the cursor, if any.
    #[cfg(test)]
    pub fn slot(&self, lists: Lists) -> Option<Slot> {
        match *self {
            Menu::Items { tab, col, row } if rows(tab, lists) > 0 => Some(slot(tab, col, row)),
            _ => None,
        }
    }
}

/// Inventory and furnace: a top row (armor, or the furnace's input, fuel
/// and output), three main rows, hotbar. Chest: three chest rows, then the
/// player's. Lists: nine to a row.
fn rows(tab: Tab, lists: Lists) -> usize {
    match tab {
        Tab::Inventory
        | Tab::Furnace(_)
        | Tab::Brewing(_)
        | Tab::Enchanting(_)
        | Tab::Anvil(_)
        | Tab::Grindstone(_)
        | Tab::Smithing(_) => 5,
        Tab::Trading(_) => 6,
        Tab::Chest(_) => 7,
        Tab::Crafting => lists.crafts.div_ceil(COLS),
        Tab::Palette => lists.palette.div_ceil(COLS),
    }
}

fn row_len(tab: Tab, row: usize, lists: Lists) -> usize {
    let list = match tab {
        Tab::Trading(_) if row < 2 => return 5,
        Tab::Inventory if row == 0 => return ArmorPiece::ALL.len(),
        Tab::Furnace(_) if row == 0 => return FURNACE.len(),
        Tab::Brewing(_) if row == 0 => return BREWING.len(),
        Tab::Enchanting(_) if row == 0 => return ENCHANTING.len(),
        Tab::Anvil(_) | Tab::Grindstone(_) if row == 0 => return ANVIL.len(),
        Tab::Smithing(_) if row == 0 => return SMITHING.len(),
        Tab::Crafting => lists.crafts,
        Tab::Palette => lists.palette,
        _ => return COLS,
    };
    if row + 1 == list.div_ceil(COLS) && !list.is_multiple_of(COLS) { list % COLS } else { COLS }
}

fn slot(tab: Tab, col: usize, row: usize) -> Slot {
    let inv = |i: usize| Slot::Ref(SlotRef::Inventory(i));
    match tab {
        Tab::Trading(_) if row < 2 => Slot::Ref(SlotRef::Trade(row * 5 + col)),
        Tab::Trading(_) if row == 5 => inv(col),
        Tab::Trading(_) => inv(COLS * (row - 1) + col),
        Tab::Inventory if row == 0 => Slot::Ref(SlotRef::Armor(ArmorPiece::ALL[col])),
        Tab::Furnace(_) if row == 0 => Slot::Ref(FURNACE[col]),
        Tab::Brewing(_) if row == 0 => Slot::Ref(BREWING[col]),
        Tab::Enchanting(_) if row == 0 => Slot::Ref(ENCHANTING[col]),
        Tab::Anvil(_) | Tab::Grindstone(_) if row == 0 => Slot::Ref(ANVIL[col]),
        Tab::Smithing(_) if row == 0 => Slot::Ref(SMITHING[col]),
        Tab::Inventory
        | Tab::Furnace(_)
        | Tab::Brewing(_)
        | Tab::Enchanting(_)
        | Tab::Anvil(_)
        | Tab::Grindstone(_)
        | Tab::Smithing(_)
            if row == 4 =>
        {
            inv(col)
        }
        Tab::Inventory
        | Tab::Furnace(_)
        | Tab::Brewing(_)
        | Tab::Enchanting(_)
        | Tab::Anvil(_)
        | Tab::Grindstone(_)
        | Tab::Smithing(_) => inv(COLS * row + col),
        Tab::Chest(_) if row < 3 => Slot::Ref(SlotRef::Chest(COLS * row + col)),
        Tab::Chest(_) if row == 6 => inv(col),
        Tab::Chest(_) => inv(COLS * (row - 2) + col),
        Tab::Crafting => Slot::Recipe(COLS * row + col),
        Tab::Palette => Slot::Palette(COLS * row + col),
    }
}

/// Every item the creative palette offers, in order.
fn palette() -> Vec<Item> {
    Item::creative_palette().collect()
}

impl Game {
    /// Results the profile can craft now (empty for unknown names).
    pub(super) fn pad_crafts(&self, name: &str) -> Vec<Stack> {
        self.agents.players.get(name).map_or_else(Vec::new, |b| b.agent.craftable(&self.world))
    }

    /// List lengths for `name`'s screens.
    pub(super) fn pad_lists(&self, name: &str, tab: Tab) -> Lists {
        let creative = self.agents.players.get(name).is_some_and(|b| b.agent.creative);
        Lists {
            crafts: if tab == Tab::Crafting { self.pad_crafts(name).len() } else { 0 },
            palette: if creative { Item::creative_palette().count() } else { 0 },
        }
    }

    /// Do what a menu press asked for seat `seat`, playing `name`. Returns
    /// false to close the menu.
    pub(super) fn apply_menu(&mut self, seat: usize, name: &str, menu: Menu, action: Action) -> bool {
        let Menu::Items { tab, .. } = menu else {
            return !matches!(action, Action::Close | Action::Leave);
        };
        let (slot, quick, right) = match action {
            Action::None => return true,
            Action::Close | Action::Leave => return false,
            Action::Click(slot, right) => (slot, false, right),
            Action::QuickMove(slot) => (slot, true, false),
        };
        let slot = match slot {
            Slot::Ref(s) => s,
            Slot::Palette(i) => match palette().get(i) {
                Some(&item) => SlotRef::Palette(item),
                None => return true,
            },
            Slot::Recipe(i) => {
                // Shift-crafting makes as many as the ingredients allow.
                let Some(item) = self.pad_crafts(name).get(i).map(|s| s.item) else { return true };
                let Some(bot) = self.agents.players.get_mut(name) else { return false };
                for _ in 0..if quick { 64 } else { 1 } {
                    let craft = Command::Craft(item);
                    if bot.agent.execute(craft, &mut self.world, &mut self.mobs.entities, &[]).is_err() {
                        break;
                    }
                }
                self.audio.ui_click();
                return true;
            }
        };
        self.puppet(seat, |g| {
            g.container = tab.container();
            if quick {
                g.quick_move(slot);
            } else {
                g.click_slot(slot, right);
            }
        });
        self.audio.ui_click();
        true
    }

    /// Closing a screen returns the stack on the cursor to the inventory,
    /// throwing whatever doesn't fit.
    pub(super) fn close_pad_menu(&mut self, name: &str) {
        let work = self.take_pad_work(name);
        let Some(bot) = self.agents.players.get_mut(name) else { return };
        let agent = &mut bot.agent;
        let cursor = agent.inventory.cursor.take();
        agent.inventory.return_stacks(cursor.into_iter().chain(work));
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
        let dial = crate::item::Dial {
            yaw: bot.agent.player.yaw,
            x: bot.agent.player.pos.x,
            z: bot.agent.player.pos.z,
            spawn_x: self.world_spawn.x as f64 + 0.5,
            spawn_z: self.world_spawn.z as f64 + 0.5,
            overworld: self.dimension == crate::world::terrain::Dimension::Overworld,
            day_time: self.day_time as f32,
            spin: self.started.elapsed().as_secs_f32(),
        };
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
        let items = if tab == Tab::Palette { palette() } else { Vec::new() };
        let lists = Lists { crafts: crafts.len(), palette: items.len() };
        let n = rows(tab, lists);
        // Long lists scroll to keep the cursor in view.
        let footer = if matches!(tab, Tab::Trading(_)) { 62.0 } else { 0.0 };
        let visible = (((sh - 40.0 - footer) / 18.0).floor() as usize).max(1);
        let first = if n > visible { (row + 1).saturating_sub(visible / 2 + 1).min(n - visible) } else { 0 };
        let shown = n.min(visible);
        // The hotbar and the row above the main grid sit a little apart.
        let gap = |r: usize| match tab {
            Tab::Inventory
            | Tab::Furnace(_)
            | Tab::Brewing(_)
            | Tab::Enchanting(_)
            | Tab::Anvil(_)
            | Tab::Grindstone(_)
            | Tab::Smithing(_) => 4.0 * ((r >= 1) as u8 + (r >= 4) as u8) as f32,
            Tab::Trading(_) => 4.0 * ((r >= 2) as u8 + (r >= 5) as u8) as f32,
            Tab::Chest(_) => 4.0 * ((r >= 3) as u8 + (r >= 6) as u8) as f32,
            Tab::Crafting | Tab::Palette => 0.0,
        };
        let pw = COLS as f32 * 18.0 + 12.0;
        let ph = 18.0 * shown.max(1) as f32 + gap(n.saturating_sub(1)) + 26.0;
        let (px, py) = (((sw - pw) / 2.0).floor(), ((sh - ph - footer) / 2.0).floor().max(0.0));
        ui.rect(px, py, pw, ph, [0.12, 0.12, 0.14, 0.92]);
        let creative = bot.agent.creative;
        let title = match tab {
            Tab::Inventory => "Inventory  (RB: crafting)",
            Tab::Crafting if creative => "Crafting  (RB: items)",
            Tab::Crafting => "Crafting  (LB: inventory)",
            Tab::Palette => "Items  (RB: inventory)",
            Tab::Chest(_) => "Chest",
            Tab::Furnace(_) => "Furnace",
            Tab::Brewing(_) => "Brewing Stand  (fuel, ingredient, bottles)",
            Tab::Enchanting(_) => "Enchant  (item, lapis, offers)",
            Tab::Anvil(_) => "Anvil  (item, material or book, result)",
            Tab::Grindstone(_) => "Repair & Disenchant  (item, item, result)",
            Tab::Trading(_) => "Trading  (select an offer)",
            Tab::Smithing(_) => "Smithing  (template, diamond gear, ingot, result)",
        };
        ui.text(px + 6.0, py + 5.0, title, WHITE);
        let chest = match tab {
            Tab::Chest(pos) => self.world.chest(pos),
            _ => None,
        };
        let furnace = match tab {
            Tab::Furnace(pos) => self.world.furnace(pos),
            _ => None,
        };
        let brewing = match tab {
            Tab::Brewing(pos) => self.world.brewing_stand(pos),
            _ => None,
        };
        // The table's slots and offers, as this player sees them.
        let work = self.pad_work(name);
        let offers = match (tab, work[0]) {
            (Tab::Enchanting(pos), Some(item)) if crate::enchant::table_accepts(item) => crate::enchant::offers(
                bot.agent.vitals.xp.seed,
                item.item,
                crate::enchant::bookshelves(&self.world, pos),
            ),
            _ => Default::default(),
        };
        let anvil = match (tab, work[0]) {
            (Tab::Grindstone(_), _) => voxelcraft::grindstone::result(work[0], work[1]),
            (Tab::Anvil(_), Some(left)) => crate::enchant::anvil_any_cost(left, work[1], bot.agent.creative),
            _ => None,
        };
        let smithing = match (tab, work[0], work[1], work[2]) {
            (Tab::Smithing(_), Some(template), Some(base), Some(addition)) => {
                crate::smithing::upgrade(template, base, addition)
            }
            _ => None,
        };
        if let Some(r) = anvil.filter(|_| !matches!(tab, Tab::Grindstone(_))) {
            // Java's cost line, beside the anvil's slots.
            let short = !bot.agent.creative && bot.agent.vitals.xp.level < r.cost;
            let (text, colour) = if !bot.agent.creative && r.cost >= crate::enchant::TOO_EXPENSIVE {
                ("Too Expensive!".to_string(), [1.0, 0.38, 0.38, 1.0])
            } else {
                (format!("Cost: {}", r.cost), if short { [1.0, 0.38, 0.38, 1.0] } else { [0.5, 1.0, 0.13, 1.0] })
            };
            ui.text(px + 6.0 + 18.0 * 3.0 + 6.0, py + 23.0, &text, colour);
        }
        let mut hovered = None;
        let mut clue = None;
        for r in first..first + shown {
            for c in 0..row_len(tab, r, lists) {
                let (x, y) = (px + 6.0 + 18.0 * c as f32, py + 18.0 + 18.0 * (r - first) as f32 + gap(r));
                ui.rect(x, y, 17.0, 17.0, [0.3, 0.3, 0.32, 0.95]);
                let stack = match slot(tab, c, r) {
                    Slot::Ref(SlotRef::Armor(p)) => inv.armor[p as usize],
                    Slot::Ref(SlotRef::Trade(i)) => {
                        if let Tab::Trading(id) = tab {
                            self.mobs
                                .entities
                                .merchant(id)
                                .and_then(|m| m.villager.as_ref())
                                .and_then(|v| v.offers.get(i).copied().flatten())
                                .map(|o| o.output)
                        } else {
                            None
                        }
                    }
                    Slot::Ref(SlotRef::Inventory(i)) => inv.slots[i],
                    Slot::Ref(SlotRef::Chest(i)) => chest.and_then(|ch| ch.slots[i]),
                    Slot::Ref(SlotRef::FurnaceInput) => furnace.and_then(|f| f.input),
                    Slot::Ref(SlotRef::FurnaceFuel) => furnace.and_then(|f| f.fuel),
                    Slot::Ref(SlotRef::FurnaceOutput) => furnace.and_then(|f| f.output),
                    Slot::Ref(SlotRef::BrewBottle(i)) => brewing.and_then(|b| b.bottles[i]),
                    Slot::Ref(SlotRef::BrewIngredient) => brewing.and_then(|b| b.ingredient),
                    Slot::Ref(SlotRef::BrewFuel) => brewing.and_then(|b| b.fuel),
                    Slot::Ref(SlotRef::EnchantItem | SlotRef::AnvilLeft) => work[0],
                    Slot::Ref(SlotRef::EnchantLapis | SlotRef::AnvilRight) => work[1],
                    Slot::Ref(SlotRef::AnvilResult) => anvil.map(|r| r.output),
                    Slot::Ref(SlotRef::SmithTemplate) => work[0],
                    Slot::Ref(SlotRef::SmithBase) => work[1],
                    Slot::Ref(SlotRef::SmithAddition) => work[2],
                    Slot::Ref(SlotRef::SmithResult) => smithing,
                    Slot::Ref(_) => None,
                    Slot::Recipe(i) => crafts.get(i).copied(),
                    Slot::Palette(i) => items.get(i).map(|&item| Stack::new(item, 1)),
                };
                if let Some(stack) = stack {
                    draw_stack(ui, x - 0.5, y - 0.5, stack, true, dial);
                }
                if let Slot::Ref(SlotRef::EnchantOffer(i)) = slot(tab, c, r)
                    && offers[i].cost > 0
                {
                    // The level cost, green when this player can pay it.
                    let a = &bot.agent;
                    let lapis = work[1].map_or(0, |s| s.count) as usize;
                    let paid = a.creative || (lapis > i && a.vitals.xp.level >= offers[i].cost);
                    let cost = offers[i].cost.to_string();
                    let colour = if paid { [0.5, 1.0, 0.13, 1.0] } else { [0.6, 0.35, 0.35, 1.0] };
                    ui.text(x + 9.0 - Ui::text_width(&cost) / 2.0, y + 5.0, &cost, colour);
                    if (c, r) == (col, row) {
                        clue = offers[i].clue;
                    }
                }
                if (c, r) == (col, row) {
                    ui.rect(x, y, 17.0, 17.0, [1.0, 1.0, 1.0, 0.35]);
                    hovered = Some((stack, x, y));
                }
            }
        }
        if let Some(f) = furnace {
            // Flame (fuel left) and arrow (cooking) beside the furnace slots.
            let (x, y) = (px + 6.0 + 18.0 * 3.0 + 6.0, py + 18.0);
            let burn = if f.burn_total > 0.0 { f.burn_left / f.burn_total } else { 0.0 };
            ui.rect(x, y + 2.0, 40.0, 5.0, [0.0, 0.0, 0.0, 0.7]);
            ui.rect(x + 1.0, y + 3.0, 38.0 * burn.clamp(0.0, 1.0), 3.0, [1.0, 0.55, 0.1, 1.0]);
            let cook = f.cook / crate::world::furnace::COOK_TIME;
            ui.rect(x, y + 10.0, 40.0, 5.0, [0.0, 0.0, 0.0, 0.7]);
            ui.rect(x + 1.0, y + 11.0, 38.0 * cook.clamp(0.0, 1.0), 3.0, WHITE);
        }
        if n == 0 {
            ui.text(px + 6.0, py + 20.0, "Nothing to craft", [0.7, 0.7, 0.7, 1.0]);
        }
        if let Tab::Trading(id) = tab
            && row < 2
        {
            let i = row * 5 + col;
            self.pad_trade_details(ui, id, i, px, py + ph + 18.0);
        }
        if let Some((stack, x, y)) = hovered {
            if let Some(held) = inv.cursor {
                draw_stack(ui, x + 6.0, y + 6.0, held, true, dial);
            } else if let Some((e, level)) = clue {
                let text = format!("{} . . . ?", e.describe(level));
                let tx = (x + 9.0 - Ui::text_width(&text) / 2.0).clamp(0.0, (sw - Ui::text_width(&text)).max(0.0));
                let ty = (py + ph + 2.0).min(sh - 10.0);
                ui.rect(tx - 2.0, ty - 1.0, Ui::text_width(&text) + 4.0, 10.0, [0.0, 0.0, 0.0, 0.8]);
                ui.text(tx, ty, &text, WHITE);
            } else if let Some(stack) = stack {
                let name = stack.display_name();
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

    const NONE: Lists = Lists { crafts: 0, palette: 0 };
    fn inv(i: usize) -> Option<Slot> {
        Some(Slot::Ref(SlotRef::Inventory(i)))
    }

    #[test]
    fn workstation_tabs_reject_replacements_of_another_type() {
        let cases = [
            (Tab::Chest(IVec3::ZERO), Block::CHEST),
            (Tab::Furnace(IVec3::ZERO), Block::FURNACE),
            (Tab::Brewing(IVec3::ZERO), Block::BREWING_STAND),
            (Tab::Enchanting(IVec3::ZERO), Block::ENCHANTING_TABLE),
            (Tab::Anvil(IVec3::ZERO), Block::ANVIL),
            (Tab::Smithing(IVec3::ZERO), Block::SMITHING_TABLE),
        ];
        for (tab, expected) in cases {
            for (_, block) in cases {
                assert_eq!(tab.matches_block(block), block == expected, "{tab:?}: {block:?}");
            }
            assert!(!tab.matches_block(Block::AIR));
        }
        assert!(Tab::Anvil(IVec3::ZERO).matches_block(Block::CHIPPED_ANVIL));
        assert!(Tab::Anvil(IVec3::ZERO).matches_block(Block::DAMAGED_ANVIL));
        assert!(Tab::Furnace(IVec3::ZERO).matches_block(Block::LIT_FURNACE));
    }

    #[test]
    fn inventory_cursor_wraps_and_clamps_to_the_armor_row() {
        let mut m = Menu::items(Tab::Inventory);
        assert_eq!(m.slot(NONE), inv(0), "starts on the hotbar");
        m.press(Nav::Left, NONE);
        assert_eq!(m.slot(NONE), inv(8));
        m.press(Nav::Down, NONE);
        assert_eq!(m.slot(NONE), Some(Slot::Ref(SlotRef::Armor(ArmorPiece::Boots))), "wraps to armor, clamped");
        m.press(Nav::Down, NONE);
        assert_eq!(m.slot(NONE), inv(9 + 3));
        assert_eq!(m.press(Nav::A, NONE), Action::Click(Slot::Ref(SlotRef::Inventory(12)), false));
        assert_eq!(m.press(Nav::X, NONE), Action::Click(Slot::Ref(SlotRef::Inventory(12)), true));
        assert_eq!(m.press(Nav::B, NONE), Action::Close);
    }

    #[test]
    fn container_rows_map_to_their_slots_then_the_player() {
        let pos = IVec3::new(1, 2, 3);
        let mut m = Menu::items(Tab::Chest(pos));
        assert_eq!(m.slot(NONE), inv(0));
        m.press(Nav::Down, NONE);
        assert_eq!(m.slot(NONE), Some(Slot::Ref(SlotRef::Chest(0))));
        m.press(Nav::Up, NONE);
        m.press(Nav::Up, NONE);
        assert_eq!(m.slot(NONE), inv(27), "bottom main row");
        assert_eq!(m.press(Nav::Y, NONE), Action::QuickMove(Slot::Ref(SlotRef::Inventory(27))));
        m.press(Nav::Rb, NONE);
        assert!(matches!(m, Menu::Items { tab: Tab::Chest(_), .. }), "containers don't switch tabs");

        let mut f = Menu::items(Tab::Furnace(pos));
        f.press(Nav::Left, NONE);
        f.press(Nav::Down, NONE);
        assert_eq!(f.slot(NONE), Some(Slot::Ref(SlotRef::FurnaceOutput)), "clamped to three slots");
        f.press(Nav::Down, NONE);
        assert_eq!(f.slot(NONE), inv(9 + 2));

        let mut s = Menu::items(Tab::Smithing(pos));
        s.press(Nav::Down, NONE);
        assert_eq!(s.slot(NONE), Some(Slot::Ref(SlotRef::SmithTemplate)));
        s.press(Nav::Right, NONE);
        assert_eq!(s.slot(NONE), Some(Slot::Ref(SlotRef::SmithBase)));
        s.press(Nav::Right, NONE);
        assert_eq!(s.slot(NONE), Some(Slot::Ref(SlotRef::SmithAddition)));
        s.press(Nav::Right, NONE);
        assert_eq!(s.slot(NONE), Some(Slot::Ref(SlotRef::SmithResult)));
    }

    #[test]
    fn list_tabs_follow_their_lengths_and_cycle() {
        let lists = Lists { crafts: 11, palette: 0 };
        let mut m = Menu::items(Tab::Inventory);
        m.press(Nav::Rb, lists);
        assert_eq!(m.slot(lists), Some(Slot::Recipe(0)));
        m.press(Nav::Down, lists);
        m.press(Nav::Right, lists);
        m.press(Nav::Right, lists);
        assert_eq!(m.slot(lists), Some(Slot::Recipe(9)), "second row has two results and wraps");
        // Nothing craftable: no slot, and presses do nothing.
        assert_eq!(m.slot(NONE), None);
        assert_eq!(m.press(Nav::A, NONE), Action::None);
        m.press(Nav::Rb, NONE);
        assert!(matches!(m, Menu::Items { tab: Tab::Inventory, .. }), "no palette in survival");

        let creative = Lists { crafts: 0, palette: 30 };
        m.press(Nav::Lb, creative);
        assert!(matches!(m, Menu::Items { tab: Tab::Palette, .. }), "LB wraps back to the palette");
        m.press(Nav::Up, creative);
        m.press(Nav::Right, creative);
        assert_eq!(m.slot(creative), Some(Slot::Palette(28)), "last row");
    }

    #[test]
    fn pause_offers_resume_and_leave() {
        let mut m = Menu::Pause { choice: 0 };
        assert_eq!(m.press(Nav::A, NONE), Action::Close);
        m.press(Nav::Down, NONE);
        assert_eq!(m.press(Nav::A, NONE), Action::Leave);
        assert_eq!(m.press(Nav::Start, NONE), Action::Close);
    }
}

#[cfg(test)]
mod trading_navigation_tests {
    use super::*;
    #[test]
    fn trading_offers_and_inventory_follow_the_controller_cursor() {
        let mut menu = Menu::items(Tab::Trading(7));
        let lists = Lists::default();
        assert_eq!(menu.slot(lists), Some(Slot::Ref(SlotRef::Trade(0))));
        menu.press(Nav::Right, lists);
        assert_eq!(menu.press(Nav::A, lists), Action::Click(Slot::Ref(SlotRef::Trade(1)), false));
        menu.press(Nav::Down, lists);
        assert_eq!(menu.slot(lists), Some(Slot::Ref(SlotRef::Trade(6))));
        menu.press(Nav::Down, lists);
        assert_eq!(menu.slot(lists), Some(Slot::Ref(SlotRef::Inventory(10))));
        for _ in 0..3 {
            menu.press(Nav::Down, lists);
        }
        assert_eq!(menu.slot(lists), Some(Slot::Ref(SlotRef::Inventory(1))));
        assert_eq!(Tab::Trading(7).container(), Container::Trading(7));
    }
}
