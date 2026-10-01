//! Heads-up display: crosshair, hotbar, item name popup and the F3 debug
//! screen.

use std::time::Instant;

use crate::inventory::{HOTBAR_SLOTS, Stack};
use crate::item::Item;
use crate::render::ui::{Color, Ui, UiVertex, WHITE};
use crate::world::block::tex;
use crate::world::chunk::{chunk_of, local_of};

use super::recipe_book::{self, Control, Layout, Rect};
use super::survival::{self, AIR_BUBBLES, MAX_AIR, MAX_HEALTH};
use super::{Container, Game, GameMode};

/// A clickable slot on the inventory screen.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum SlotRef {
    Inventory(usize),
    /// Creative palette entry.
    Palette(Item),
    /// Crafting grid cell (row-major in the grid's own size).
    Craft(usize),
    CraftResult,
    FurnaceInput,
    FurnaceFuel,
    FurnaceOutput,
    /// Chest slot (row-major).
    Chest(usize),
}

/// Visible rows of the creative palette.
pub(super) const PALETTE_ROWS: usize = 3;

/// Total rows in the creative palette.
pub(super) fn palette_rows() -> usize {
    Item::creative_palette().count().div_ceil(9)
}

const SLOT: f32 = 18.0;
const PANEL_W: f32 = 9.0 * SLOT + 14.0;
const PANEL_H: f32 = 4.0 * SLOT + 38.0;
/// Extra height for the crafting area above the inventory grid.
const CRAFT_H: f32 = 3.0 * SLOT + 14.0;

const DEBUG_TEXT: Color = [0.88, 0.88, 0.88, 1.0];
const HIGHLIGHT: Color = [1.0, 1.0, 0.6, 1.0];
/// Duration of the red screen flash after taking damage.
const HURT_FLASH: f32 = 0.3;

impl Game {
    pub(super) fn build_ui(&self, now: Instant) -> Vec<UiVertex> {
        let (w, h) = self.renderer.size();
        let mut ui = Ui::new(w as f32, h as f32, self.renderer.scale_factor());
        let (sw, sh) = ui.size();

        if self.player.head_in_water(&self.world) {
            ui.rect(0.0, 0.0, sw, sh, [0.05, 0.15, 0.4, 0.3]);
        }

        let hurt = self.vitals.since_damage();
        if hurt < HURT_FLASH {
            ui.rect(0.0, 0.0, sw, sh, [0.8, 0.0, 0.0, 0.35 * (1.0 - hurt / HURT_FLASH)]);
        }

        // Crosshair.
        if !self.vitals.is_dead() {
            let (cx, cy) = ((sw / 2.0).floor(), (sh / 2.0).floor());
            ui.rect(cx - 5.0, cy - 0.5, 10.0, 1.0, [1.0, 1.0, 1.0, 0.85]);
            ui.rect(cx - 0.5, cy - 5.0, 1.0, 10.0, [1.0, 1.0, 1.0, 0.85]);
            if self.actions.eat_timer > 0.0 && !self.inventory_open && self.menu.is_none() {
                let progress = (self.actions.eat_timer / super::EAT_TIME).clamp(0.0, 1.0) as f32;
                ui.text(cx - Ui::text_width("Eating") / 2.0, cy + 14.0, "Eating", WHITE);
                ui.rect(cx - 30.0, cy + 25.0, 60.0, 5.0, [0.0, 0.0, 0.0, 0.7]);
                ui.rect(cx - 29.0, cy + 26.0, 58.0 * progress, 3.0, [0.96, 0.72, 0.3, 1.0]);
            }
        }

        self.hotbar_ui(&mut ui, now);
        if self.show_debug {
            self.debug_ui(&mut ui);
        }
        if let Some(cause) = &self.vitals.death {
            // Blending is in linear space: it takes a high alpha to look dark.
            ui.rect(0.0, 0.0, sw, sh, [0.18, 0.0, 0.0, 0.9]);
            let title = "You died!";
            let k = 3.0;
            let y = (sh * 0.3).floor();
            ui.text_scaled(((sw - Ui::text_width(title) * k) / 2.0).floor(), y, title, WHITE, k);
            let msg = format!("Player {cause}");
            ui.text(((sw - Ui::text_width(&msg)) / 2.0).floor(), y + 36.0, &msg, WHITE);
            let hint = "Click to respawn";
            ui.text(((sw - Ui::text_width(hint)) / 2.0).floor(), y + 64.0, hint, [1.0, 1.0, 0.6, 1.0]);
        } else if self.menu.is_some() {
            self.menu_ui(&mut ui);
        } else if self.inventory_open {
            self.inventory_ui(&mut ui);
        } else if !self.mouse_grabbed && self.screenshot.is_none() {
            ui.rect(0.0, 0.0, sw, sh, [0.0, 0.0, 0.0, 0.35]);
            let msg = "Click to play  -  Esc for the menu";
            ui.text((sw - Ui::text_width(msg)) / 2.0, sh / 2.0 - 24.0, msg, WHITE);
        }
        ui.verts
    }

    fn hotbar_ui(&self, ui: &mut Ui, now: Instant) {
        let (sw, sh) = ui.size();
        let slot = 20.0;
        let total = slot * 9.0 + 2.0;
        let (x0, y0) = (((sw - total) / 2.0).floor(), sh - slot - 4.0);
        ui.rect(x0, y0, total, slot + 2.0, [0.0, 0.0, 0.0, 0.5]);
        for i in 0..HOTBAR_SLOTS {
            let sx = x0 + 1.0 + i as f32 * slot;
            ui.rect(sx + 1.0, y0 + 2.0, slot - 2.0, slot - 2.0, [0.35, 0.35, 0.35, 0.5]);
            if i == self.actions.selected {
                let (x, y, s) = (sx - 1.0, y0 - 1.0, slot + 2.0);
                for (rx, ry, rw, rh) in
                    [(x, y, s, 2.0), (x, y + s, s, 2.0), (x, y, 2.0, s + 2.0), (x + s - 2.0, y, 2.0, s + 2.0)]
                {
                    ui.rect(rx, ry, rw, rh, WHITE);
                }
            }
            if let Some(stack) = self.inventory.get(i) {
                self.stack_ui(ui, sx + 1.0, y0 + 2.0, stack);
            }
        }

        let survival = self.mode == GameMode::Survival;
        if survival {
            self.vitals_ui(ui, x0, x0 + total, y0 - 10.0, now);
        }

        // Popup message (item names, mode changes), fading out.
        let age = (now - self.popup.1).as_secs_f32();
        let alpha = ((2.5 - age) / 0.5).clamp(0.0, 1.0);
        if alpha > 0.0 {
            let text = capitalize(&self.popup.0);
            let y = y0 - if survival { 24.0 } else { 14.0 };
            ui.text((sw - Ui::text_width(&text)) / 2.0, y, &text, [1.0, 1.0, 1.0, alpha]);
        }
    }

    /// Hearts above the left half of the hotbar and air bubbles above the
    /// right half, like Minecraft. Hearts shake after a hit and at low
    /// health.
    fn vitals_ui(&self, ui: &mut Ui, left: f32, right: f32, y: f32, now: Instant) {
        const ICON: f32 = 9.0;
        const STEP: f32 = 8.0;
        let v = &self.vitals;
        let half_hearts = v.health.ceil() as u32;
        let shaking = v.since_damage() < survival::INVULNERABLE || v.health <= 4.0;
        // Re-roll the jitter 20 times a second.
        let tick = ((now - self.started).as_secs_f32() * 20.0) as u32;
        for i in 0..(MAX_HEALTH as u32 / 2) {
            let layer = match half_hearts.saturating_sub(i * 2) {
                0 => tex::HEART_EMPTY,
                1 => tex::HEART_HALF,
                _ => tex::HEART_FULL,
            };
            let jitter = if shaking { (hash(i, tick) % 3) as f32 - 1.0 } else { 0.0 };
            ui.icon(left + 1.0 + i as f32 * STEP, y + jitter, ICON, layer, WHITE);
        }

        // Hunger, right to left; drumsticks shiver when nearly empty.
        let half_food = v.hunger.food.ceil() as u32;
        let starving = v.hunger.food <= 0.0;
        for i in 0..(survival::MAX_FOOD as u32 / 2) {
            let layer = match half_food.saturating_sub(i * 2) {
                0 => tex::FOOD_EMPTY,
                1 => tex::FOOD_HALF,
                _ => tex::FOOD_FULL,
            };
            let jitter = if half_food <= 6 || starving { (hash(i + 50, tick) % 3) as f32 - 1.0 } else { 0.0 };
            ui.icon(right - 1.0 - ICON - i as f32 * STEP, y + jitter, ICON, layer, WHITE);
        }

        // Air bubbles sit above the hunger bar.
        if self.player.head_in_water(&self.world) || v.air < MAX_AIR {
            for i in 0..v.bubbles().min(AIR_BUBBLES) {
                ui.icon(right - 1.0 - ICON - i as f32 * STEP, y - 10.0, ICON, tex::BUBBLE, WHITE);
            }
        }
    }

    /// Item icon, durability bar and stack count in an 18x18 slot at (x, y).
    fn stack_ui(&self, ui: &mut Ui, x: f32, y: f32, stack: Stack) {
        match (stack.item.block(), stack.item.icon_layer()) {
            (Some(block), _) => ui.block_icon(x + 2.0, y + 2.0, 14.0, block),
            (None, Some(layer)) => ui.icon(x + 1.0, y + 1.0, 16.0, layer, WHITE),
            (None, None) => {}
        }
        if let Some(wear) = stack.wear() {
            // Minecraft's bar: green when new, through yellow to red.
            let w = (13.0 * wear).round().max(1.0);
            let color = [(2.0 - 2.0 * wear).min(1.0), (2.0 * wear).min(1.0), 0.0, 1.0];
            ui.rect(x + 2.0, y + 14.0, 13.0, 2.0, [0.0, 0.0, 0.0, 1.0]);
            ui.rect(x + 2.0, y + 14.0, w, 1.0, color);
        }
        if self.mode == GameMode::Survival && stack.count > 1 {
            let n = stack.count.to_string();
            ui.text(x + 17.0 - Ui::text_width(&n), y + 9.0, &n, WHITE);
        }
    }

    /// Whether the screen has a top section (crafting grid or furnace)
    /// above the inventory; only the creative inventory doesn't.
    fn has_top_section(&self) -> bool {
        self.mode == GameMode::Survival || self.container != Container::Inventory
    }

    /// Whether the middle grid shows the creative palette instead of the
    /// main inventory.
    fn shows_palette(&self) -> bool {
        self.mode == GameMode::Creative && self.container == Container::Inventory
    }

    pub(super) fn shows_recipes(&self) -> bool {
        self.has_top_section() && !matches!(self.container, Container::Furnace(_) | Container::Chest(_))
    }

    /// Top-left corner and height of the inventory panel.
    fn panel(&self, screen: (f32, f32)) -> (f32, f32, f32) {
        let h = PANEL_H + if self.has_top_section() { CRAFT_H } else { 0.0 };
        let extra = if self.recipe_book.open && self.shows_recipes() && recipe_book::fits_beside(screen.0, PANEL_W) {
            recipe_book::WIDTH + recipe_book::GAP
        } else {
            0.0
        };
        (((screen.0 - PANEL_W - extra) / 2.0).floor(), ((screen.1 - h) / 2.0).floor(), h)
    }

    fn recipe_layout(&self, screen: (f32, f32)) -> Layout {
        let (x, y, h) = self.panel(screen);
        Layout::new(screen.0, Rect { x, y, w: PANEL_W, h })
    }

    pub(super) fn recipe_control_under_cursor(&self) -> Option<Control> {
        if !self.shows_recipes() {
            return None;
        }
        let scale = Ui::scale_for(self.renderer.scale_factor());
        let (w, h) = self.renderer.size();
        self.recipe_layout((w as f32 / scale, h as f32 / scale))
            .control((self.cursor_px.0 / scale, self.cursor_px.1 / scale), self.recipe_book.open)
    }

    /// Screen positions of every slot on the inventory screen: the open
    /// container's slots (a crafting grid and its result outside creative, a
    /// furnace or a chest), a 3x9 grid (main inventory, or the block palette
    /// in creative) and the hotbar.
    fn inventory_slots(&self, screen: (f32, f32)) -> Vec<(SlotRef, f32, f32)> {
        let (px, py, _) = self.panel(screen);
        let mut out = Vec::with_capacity(46);
        let top = if let Container::Chest(_) = self.container {
            for i in 0..crate::world::chest::SLOTS {
                out.push((SlotRef::Chest(i), px + 7.0 + (i % 9) as f32 * SLOT, py + 18.0 + (i / 9) as f32 * SLOT));
            }
            CRAFT_H
        } else if let Container::Furnace(_) = self.container {
            // Input over fuel (with the flame between), the output past the arrow.
            let x = px + 7.0 + 3.0 * SLOT;
            out.push((SlotRef::FurnaceInput, x, py + 18.0));
            out.push((SlotRef::FurnaceFuel, x, py + 18.0 + 2.0 * SLOT));
            out.push((SlotRef::FurnaceOutput, px + 7.0 + 6.0 * SLOT, py + 18.0 + SLOT));
            CRAFT_H
        } else if self.has_top_section() {
            let n = self.craft.size;
            // The grid sits left of centre, the result to its right past an arrow.
            let gx = px + 7.0 + if n == 3 { SLOT } else { 2.0 * SLOT };
            let gy = py + 18.0 + (3 - n) as f32 * SLOT / 2.0;
            for i in 0..n * n {
                out.push((SlotRef::Craft(i), gx + (i % n) as f32 * SLOT, gy + (i / n) as f32 * SLOT));
            }
            out.push((SlotRef::CraftResult, px + 7.0 + 6.0 * SLOT, py + 18.0 + SLOT));
            CRAFT_H
        } else {
            0.0
        };
        let py = py + top;
        let grid = |i: usize| (px + 7.0 + (i % 9) as f32 * SLOT, py + 18.0 + (i / 9) as f32 * SLOT);
        if self.shows_palette() {
            let first = self.creative_scroll * 9;
            for (i, item) in Item::creative_palette().skip(first).take(PALETTE_ROWS * 9).enumerate() {
                let (x, y) = grid(i);
                out.push((SlotRef::Palette(item), x, y));
            }
        } else {
            for i in 0..27 {
                let (x, y) = grid(i);
                out.push((SlotRef::Inventory(HOTBAR_SLOTS + i), x, y));
            }
        }
        for i in 0..HOTBAR_SLOTS {
            out.push((SlotRef::Inventory(i), px + 7.0 + i as f32 * SLOT, py + 18.0 + 3.0 * SLOT + 6.0));
        }
        out
    }

    /// Whether the mouse is off the inventory panel and the recipe book:
    /// clicking there throws the held stack out, like Minecraft.
    pub(super) fn cursor_off_panel(&self) -> bool {
        let scale = Ui::scale_for(self.renderer.scale_factor());
        let (w, h) = self.renderer.size();
        let screen = (w as f32 / scale, h as f32 / scale);
        let mouse = (self.cursor_px.0 / scale, self.cursor_px.1 / scale);
        let (x, y, h) = self.panel(screen);
        let on_book =
            self.shows_recipes() && self.recipe_book.open && self.recipe_layout(screen).bounds.contains(mouse);
        !Rect { x, y, w: PANEL_W, h }.contains(mouse) && !on_book
    }

    pub(super) fn slot_under_cursor(&self) -> Option<SlotRef> {
        let scale = Ui::scale_for(self.renderer.scale_factor());
        let (w, h) = self.renderer.size();
        let (mx, my) = (self.cursor_px.0 / scale, self.cursor_px.1 / scale);
        if self.shows_recipes()
            && self.recipe_book.open
            && self.recipe_layout((w as f32 / scale, h as f32 / scale)).overlay
        {
            return None;
        }
        self.inventory_slots((w as f32 / scale, h as f32 / scale))
            .into_iter()
            .find(|&(_, x, y)| mx >= x && mx < x + SLOT && my >= y && my < y + SLOT)
            .map(|(r, _, _)| r)
    }

    fn inventory_ui(&self, ui: &mut Ui) {
        let (sw, sh) = ui.size();
        ui.rect(0.0, 0.0, sw, sh, [0.0, 0.0, 0.0, 0.45]);
        let (px, py, panel_h) = self.panel((sw, sh));
        ui.rect(px, py, PANEL_W, panel_h, [0.78, 0.78, 0.78, 1.0]);
        ui.rect(px, py, PANEL_W, 1.0, WHITE);
        ui.rect(px, py, 1.0, panel_h, WHITE);
        ui.rect(px, py + panel_h - 1.0, PANEL_W, 1.0, [0.33, 0.33, 0.33, 1.0]);
        ui.rect(px + PANEL_W - 1.0, py, 1.0, panel_h, [0.33, 0.33, 0.33, 1.0]);
        let title = match (self.container, self.mode) {
            (Container::CraftingTable, _) => "Crafting",
            (Container::Furnace(_), _) => "Furnace",
            (Container::Chest(_), _) => "Chest",
            (Container::Inventory, GameMode::Survival) => "Inventory",
            (Container::Inventory, GameMode::Creative) => "Creative",
        };
        ui.text_flat(px + 8.0, py + 6.0, title, [0.25, 0.25, 0.25, 1.0]);
        if self.shows_recipes() {
            let layout = self.recipe_layout((sw, sh));
            self.recipe_button(ui, layout.toggle, if self.recipe_book.open { "Hide" } else { "Recipes" });
        }
        if self.has_top_section() && !matches!(self.container, Container::Chest(_)) {
            // Arrow toward the result; in a furnace it fills with progress
            // and a flame between input and fuel shows the fuel left.
            let (ax, ay) = (px + 7.0 + 4.0 * SLOT + 9.0, py + 18.0 + SLOT + 5.0);
            let furnace = match self.container {
                Container::Furnace(p) => self.world.furnace(p).copied(),
                _ => None,
            };
            let progress = furnace.map_or(0.0, |f| f.cook / crate::world::furnace::COOK_TIME);
            let dark = [0.45, 0.45, 0.45, 1.0];
            let lit = [1.0, 1.0, 1.0, 1.0];
            let filled = |x: f32| if (x - ax) / 17.0 < progress { lit } else { dark };
            for i in 0..12 {
                let x = ax + i as f32;
                ui.rect(x, ay + 3.0, 1.0, 2.0, filled(x));
            }
            for i in 0..5 {
                let x = ax + 12.0 + i as f32;
                ui.rect(x, ay + i as f32 - 1.0, 1.0, 10.0 - 2.0 * i as f32, filled(x));
            }
            if let Some(f) = furnace {
                let (fx, fy) = (px + 7.0 + 3.0 * SLOT + 4.0, py + 18.0 + SLOT + 2.0);
                let left = if f.burn_total > 0.0 { f.burn_left / f.burn_total } else { 0.0 };
                ui.rect(fx, fy, 10.0, 13.0, [0.6, 0.6, 0.6, 1.0]);
                let h = (13.0 * left).ceil();
                ui.rect(fx + 1.0, fy + 13.0 - h, 8.0, h, [1.0, 0.55, 0.1, 1.0]);
                ui.rect(fx + 3.0, fy + 13.0 - h * 0.6, 4.0, h * 0.6, [1.0, 0.9, 0.3, 1.0]);
            }
        }
        let py = py + if self.has_top_section() { CRAFT_H } else { 0.0 };
        if matches!(self.container, Container::Chest(_)) {
            ui.text_flat(px + 8.0, py + 6.0, "Inventory", [0.25, 0.25, 0.25, 1.0]);
        }
        if self.shows_palette() {
            // Scrollbar beside the palette grid.
            let (x, y, h) = (px + PANEL_W - 6.0, py + 18.0, PALETTE_ROWS as f32 * SLOT);
            let rows = palette_rows().max(1) as f32;
            let thumb = h * (PALETTE_ROWS as f32 / rows).min(1.0);
            let top = y + (h - thumb) * self.creative_scroll as f32 / (rows - PALETTE_ROWS as f32).max(1.0);
            ui.rect(x, y, 4.0, h, [0.45, 0.45, 0.45, 1.0]);
            ui.rect(x, top, 4.0, thumb, WHITE);
        }

        let hovered = self.slot_under_cursor();
        for (r, x, y) in self.inventory_slots((sw, sh)) {
            ui.rect(x, y, SLOT, SLOT, [0.55, 0.55, 0.55, 1.0]);
            ui.rect(x + 1.0, y + 1.0, SLOT - 1.0, SLOT - 1.0, [0.45, 0.45, 0.45, 1.0]);
            let stack = match r {
                SlotRef::Inventory(i) => self.inventory.get(i),
                SlotRef::Palette(b) => Some(Stack::new(b, 1)),
                SlotRef::Craft(i) => self.craft.cells[i],
                SlotRef::CraftResult => self.craft.result(),
                f => self.container_slot(f),
            };
            if let Some(stack) = stack {
                self.stack_ui(ui, x, y, stack);
            }
            if hovered == Some(r) {
                ui.rect(x + 1.0, y + 1.0, SLOT - 2.0, SLOT - 2.0, [1.0, 1.0, 1.0, 0.35]);
            }
        }
        let hovered_item = match hovered {
            Some(SlotRef::Inventory(i)) => self.inventory.get(i).map(|s| s.item),
            Some(SlotRef::Palette(item)) => Some(item),
            Some(SlotRef::Craft(i)) => self.craft.cells[i].map(|s| s.item),
            Some(SlotRef::CraftResult) => self.craft.result().map(|s| s.item),
            Some(f) => self.container_slot(f).map(|s| s.item),
            None => None,
        };
        let recipe_hint = if self.recipe_book.open && self.shows_recipes() { self.recipe_book_ui(ui) } else { None };
        if let Some(hint) = recipe_hint {
            self.tooltip(ui, &hint);
        } else if let Some(item) = hovered_item {
            self.tooltip(ui, item.name());
        }
        // The held stack follows the mouse.
        if let Some(stack) = self.inventory.cursor {
            let scale = ui.scale;
            self.stack_ui(ui, self.cursor_px.0 / scale - 9.0, self.cursor_px.1 / scale - 9.0, stack);
        }
    }

    fn recipe_button(&self, ui: &mut Ui, rect: Rect, label: &str) {
        let hover = rect.contains((self.cursor_px.0 / ui.scale, self.cursor_px.1 / ui.scale));
        ui.rect(rect.x, rect.y, rect.w, rect.h, if hover { [0.95, 0.9, 0.65, 1.0] } else { [0.6, 0.6, 0.6, 1.0] });
        ui.text_flat(rect.x + (rect.w - Ui::text_width(label)) / 2.0, rect.y + 2.0, label, [0.15, 0.15, 0.15, 1.0]);
    }

    /// Draws ingredient hints, never editable slots or an automatic craft.
    fn recipe_book_ui(&self, ui: &mut Ui) -> Option<String> {
        let layout = self.recipe_layout(ui.size());
        let Rect { x, y, w, h } = layout.bounds;
        ui.rect(x, y, w, h, [0.72, 0.75, 0.69, 1.0]);
        ui.rect(x, y, w, 1.0, WHITE);
        ui.rect(x, y, 1.0, h, WHITE);
        ui.rect(x + w - 1.0, y, 1.0, h, [0.3, 0.35, 0.28, 1.0]);
        ui.rect(x, y + h - 1.0, w, 1.0, [0.3, 0.35, 0.28, 1.0]);
        ui.text_flat(x + 8.0, y + 6.0, "Recipes", [0.2, 0.25, 0.15, 1.0]);
        if layout.overlay {
            self.recipe_button(ui, layout.toggle, "Back");
        }
        self.recipe_button(ui, layout.previous, "<");
        self.recipe_button(ui, layout.next, ">");
        let recipes = crate::crafting::recipes();
        let recipe = &recipes[self.recipe_book.selected];
        let page = format!("{} / {}", self.recipe_book.selected + 1, recipes.len());
        ui.text_flat(x + (w - Ui::text_width(&page)) / 2.0, y + 22.0, &page, [0.25, 0.25, 0.25, 1.0]);
        let name = capitalize(recipe.result.item.name());
        ui.text_flat(x + (w - Ui::text_width(&name)) / 2.0, y + 42.0, &name, [0.15, 0.2, 0.1, 1.0]);

        let preview = recipe.preview();
        let (gx, gy) = (x + 10.0, y + 60.0);
        let mouse = (self.cursor_px.0 / ui.scale, self.cursor_px.1 / ui.scale);
        let mut hint = None;
        for i in 0..preview.size * preview.size {
            let rect = Rect {
                x: gx + (i % preview.size) as f32 * SLOT,
                y: gy + (i / preview.size) as f32 * SLOT,
                w: SLOT,
                h: SLOT,
            };
            ui.rect(rect.x, rect.y, SLOT - 1.0, SLOT - 1.0, [0.4, 0.45, 0.37, 1.0]);
            if let Some(stack) = preview.cells[i] {
                self.stack_ui(ui, rect.x, rect.y, stack);
                if rect.contains(mouse) {
                    hint = recipe
                        .alternatives(i)
                        .map(|options| options.iter().map(|i| i.name()).collect::<Vec<_>>().join(" or "));
                }
            }
        }
        let output = Rect { x: x + w - 28.0, y: gy + (preview.size - 1) as f32 * SLOT / 2.0, w: SLOT, h: SLOT };
        ui.text_flat(output.x - 20.0, output.y + 5.0, "->", [0.25, 0.3, 0.2, 1.0]);
        ui.rect(output.x, output.y, SLOT, SLOT, [0.4, 0.45, 0.37, 1.0]);
        self.stack_ui(ui, output.x, output.y, recipe.result);
        if output.contains(mouse) {
            hint = Some(format!("Makes {} {}", recipe.result.count, recipe.result.item.name()));
        }
        let (line1, line2) = if preview.size > self.craft.size {
            ("Use a crafting table", "for this recipe")
        } else {
            ("Copy to your grid", "Click your result")
        };
        for (text, dy) in [(line1, 122.0), (line2, 134.0), ("Preview only", 150.0)] {
            ui.text_flat(x + (w - Ui::text_width(text)) / 2.0, y + dy, text, [0.25, 0.3, 0.2, 1.0]);
        }
        hint
    }

    /// Contents of a furnace or chest slot on the open screen.
    fn container_slot(&self, slot: SlotRef) -> Option<Stack> {
        if let (Container::Chest(p), SlotRef::Chest(i)) = (self.container, slot) {
            return self.world.chest(p)?.slots[i];
        }
        let Container::Furnace(p) = self.container else { return None };
        let f = self.world.furnace(p)?;
        match slot {
            SlotRef::FurnaceInput => f.input,
            SlotRef::FurnaceFuel => f.fuel,
            SlotRef::FurnaceOutput => f.output,
            _ => None,
        }
    }

    fn tooltip(&self, ui: &mut Ui, text: &str) {
        let text = capitalize(text);
        let (sw, sh) = ui.size();
        let x = (self.cursor_px.0 / ui.scale + 10.0).min(sw - Ui::text_width(&text) - 6.0).max(3.0);
        let y = (self.cursor_px.1 / ui.scale - 12.0).clamp(3.0, (sh - 11.0).max(3.0));
        ui.rect(x - 3.0, y - 3.0, Ui::text_width(&text) + 6.0, 14.0, [0.08, 0.02, 0.12, 0.92]);
        ui.text(x, y, &text, WHITE);
    }

    fn debug_ui(&self, ui: &mut Ui) {
        let p = self.player.pos;
        let b = p.floor().as_ivec3();
        let c = chunk_of(b);
        let l = local_of(b);
        let f = self.player.forward();
        let facing = if f.x.abs() > f.z.abs() {
            if f.x > 0.0 { "east (+X)" } else { "west (-X)" }
        } else if f.z > 0.0 {
            "south (+Z)"
        } else {
            "north (-Z)"
        };
        let hours = (self.day_time * 24.0 + 6.0) % 24.0;
        let s = self.renderer.stats;
        let biome = self.world.generator.column(b.x, b.z).biome;
        let target = self
            .target()
            .and_then(|(pos, _)| self.world.get_block(pos).map(|blk| (pos, blk)))
            .map(|(pos, blk)| format!("Looking at: {} @ {} {} {}", blk.name(), pos.x, pos.y, pos.z))
            .unwrap_or_else(|| "Looking at: nothing".into());

        let left = [
            format!("VoxelCraft {}", env!("CARGO_PKG_VERSION")),
            format!(
                "{:.0} fps ({:.2} ms cpu){}",
                self.fps,
                self.cpu_ms,
                if self.renderer.vsync() { " vsync" } else { "" }
            ),
            String::new(),
            format!("XYZ: {:.3} / {:.3} / {:.3}", p.x, p.y, p.z),
            format!("Block: {} {} {}", b.x, b.y, b.z),
            format!("Chunk: {} {} {} in {} {} {}", l.x, l.y, l.z, c.x, c.y, c.z),
            format!("Facing: {facing} ({:.1} / {:.1})", self.player.yaw.to_degrees(), self.player.pitch.to_degrees()),
            format!("Biome: {biome:?}"),
            format!(
                "Weather: {} ({:.0} s left)",
                if self.weather.raining { "rain" } else { "clear" },
                self.weather.timer
            ),
            format!("Game mode: {}", self.mode.name()),
            format!(
                "Health: {:.1} / {}, air: {:.1} / {}{}",
                self.vitals.health,
                MAX_HEALTH,
                self.vitals.air,
                MAX_AIR,
                if self.vitals.is_dead() { " (dead)" } else { "" }
            ),
            format!("Time: {:02}:{:02}", hours as u32, (hours.fract() * 60.0) as u32),
            format!(
                "Mode: {}{}{}",
                if self.player.flying { "flying" } else { "walking" },
                if self.player.on_ground { ", on ground" } else { "" },
                if self.player.in_water { ", swimming" } else { "" },
            ),
            String::new(),
            target,
        ];
        let right = [
            self.renderer.gpu_name.clone(),
            format!(
                "Render distance: {} chunks ({} blocks)",
                self.world.render_distance(),
                self.world.render_distance() * 32
            ),
            format!("Chunks: {} loaded, {} meshed, {} visible", self.world.loaded_chunks(), s.meshes, s.visible),
            format!("Draw calls: {}, quads: {:.2}M", s.draw_calls, s.quads as f64 / 1e6),
            format!(
                "Quad memory: {:.1} MB used, {:.1} MB reserved",
                s.gpu_used_bytes as f64 / 1e6,
                s.gpu_bytes as f64 / 1e6
            ),
            format!("Workers: {}, jobs in flight: {}", self.world.worker_threads(), self.world.pending_jobs()),
            format!("Water: {} active, last tick {:.2} ms", self.world.active_fluids(), self.world.fluid_tick_ms()),
            self.mobs_debug_line(),
            format!("Seed: {}", self.world.generator.seed),
        ];

        for (i, line) in left.iter().enumerate() {
            if !line.is_empty() {
                let color = if i == 0 { HIGHLIGHT } else { DEBUG_TEXT };
                ui.label(2.0, 2.0 + i as f32 * 10.0, line, color);
            }
        }
        let sw = ui.size().0;
        for (i, line) in right.iter().enumerate() {
            ui.label(sw - Ui::text_width(line) - 2.0, 2.0 + i as f32 * 10.0, line, DEBUG_TEXT);
        }
    }
}

/// Small integer hash for HUD jitter.
fn hash(a: u32, b: u32) -> u32 {
    let mut h = a.wrapping_mul(0x9E37_79B9) ^ b.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}
