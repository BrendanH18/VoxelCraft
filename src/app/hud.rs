//! Heads-up display: crosshair, hotbar, item name popup and the F3 debug
//! screen and optional FPS counter.

use std::time::Instant;

use crate::inventory::{HOTBAR_SLOTS, Stack};
use crate::item::{ArmorMaterial, ArmorPiece, Item};
use crate::render::ui::{Color, Ui, UiVertex, WHITE};
use crate::simulation::effects::{self, Effects};
use crate::simulation::experience::Experience;
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
    /// Brewing stand bottle (left, middle, right), ingredient and fuel.
    BrewBottle(usize),
    BrewIngredient,
    BrewFuel,
    /// The enchanting table's item and lapis slots, and its three offers.
    EnchantItem,
    EnchantLapis,
    EnchantOffer(usize),
    /// The anvil's two inputs and its result.
    AnvilLeft,
    AnvilRight,
    AnvilResult,
    /// Smithing template, base, addition and transform result.
    SmithTemplate,
    SmithBase,
    SmithAddition,
    SmithResult,
    /// Worn armor (survival inventory).
    Armor(ArmorPiece),
}

/// Visible rows of the creative palette.
pub(super) const PALETTE_ROWS: usize = 3;

/// Total rows in the creative palette.
pub(super) fn palette_rows(query: &str) -> usize {
    Item::creative_palette().filter(|item| item.matches_query(query)).count().div_ceil(9)
}

const SLOT: f32 = 18.0;
const PANEL_W: f32 = 9.0 * SLOT + 14.0;
const PANEL_H: f32 = 4.0 * SLOT + 38.0;
/// Extra height for the crafting area above the inventory grid.
const CRAFT_H: f32 = 3.0 * SLOT + 14.0;

const DEBUG_TEXT: Color = [0.88, 0.88, 0.88, 1.0];
const HIGHLIGHT: Color = [1.0, 1.0, 0.6, 1.0];
/// Duration of the red screen flash after taking damage.
pub(super) const HURT_FLASH: f32 = 0.3;

/// Whose hotbar, health and hunger a HUD shows.
pub(super) struct HudPlayer<'a> {
    pub inventory: &'a crate::inventory::Inventory,
    pub vitals: &'a survival::Vitals,
    pub selected: usize,
    pub survival: bool,
    pub underwater: bool,
}

impl Game {
    pub(super) fn build_ui(&self, now: Instant) -> Vec<UiVertex> {
        let (w, h) = self.ui_size();
        let mut ui = Ui::with_scale(w as f32, h as f32, self.ui_scale());
        let (sw, sh) = ui.size();

        if self.player.head_in_water(&self.world) {
            ui.rect(0.0, 0.0, sw, sh, [0.05, 0.15, 0.4, 0.3]);
        }

        if self.vitals.burning() {
            let frame = ((now - self.started).as_secs_f32() * 10.0) as u32 % tex::FIRE_FRAMES as u32;
            for (x, tilt) in [(-sw * 0.1, -sw * 0.12), (sw * 0.55, sw * 0.12)] {
                ui.quad(
                    [[x + tilt, sh * 0.35], [x + sw * 0.55 + tilt, sh * 0.35], [x + sw * 0.55, sh], [x, sh]],
                    [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
                    (tex::FIRE_0 as u32 + frame) as f32,
                    [1.0, 1.0, 1.0, 0.75],
                );
            }
        }

        if self.portal_time > 0.0 {
            // The portal's purple swims in as it takes hold.
            let k = (self.portal_time / self.portal_needed()).min(1.0);
            ui.rect(0.0, 0.0, sw, sh, [0.45, 0.1, 0.75, 0.15 + 0.55 * k]);
        }
        if self.arrival.is_some() {
            ui.rect(0.0, 0.0, sw, sh, [0.03, 0.0, 0.06, 1.0]);
            let msg = "Building terrain";
            ui.text(((sw - Ui::text_width(msg)) / 2.0).floor(), (sh / 2.0).floor(), msg, WHITE);
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
                eating_bar(&mut ui, cx, cy, (self.actions.eat_timer / super::EAT_TIME) as f32);
            }
            if let Some(power) = self.bow_power().filter(|_| !self.inventory_open && self.menu.is_none()) {
                bow_bar(&mut ui, cx, cy, power);
            }
        }

        self.hotbar_ui(&mut ui, now);
        if let Some(health) = self.mobs.entities.fight.as_ref().and_then(|f| f.boss_bar(self.player.pos)) {
            boss_bar_ui(&mut ui, "Ender Dragon", health, sw);
        }
        if self.show_debug {
            self.debug_ui(&mut ui);
        }
        if let Some(t) = self.sleeping {
            // Falling asleep: the screen fades to black.
            let k = (t / super::bed::SLEEP_TIME).min(1.0);
            ui.rect(0.0, 0.0, sw, sh, [0.0, 0.0, 0.02, 0.97 * k]);
            let (asleep, players) = self.sleep_count();
            if k >= 1.0 && asleep < players {
                let msg = format!("{asleep}/{players} players sleeping");
                ui.text(((sw - Ui::text_width(&msg)) / 2.0).floor(), (sh / 2.0 - 10.0).floor(), &msg, WHITE);
                let hint = "Jump to leave the bed";
                ui.text(((sw - Ui::text_width(hint)) / 2.0).floor(), (sh / 2.0 + 4.0).floor(), hint, WHITE);
            }
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
            let score = format!("Score: {}", self.vitals.xp.total);
            let x = ((sw - Ui::text_width(&score)) / 2.0).floor();
            let n = x + Ui::text_width("Score: ") + 1.0;
            ui.text(x, y + 48.0, "Score: ", WHITE);
            ui.text(n, y + 48.0, &self.vitals.xp.total.to_string(), [1.0, 1.0, 0.33, 1.0]);
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
        if self.console.open {
            self.console_ui(&mut ui);
        }
        // F3 already includes the same FPS measurement. Draw last so the
        // compact counter stays readable above menus and screen effects.
        if self.settings.show_fps && self.show_hud && !self.show_debug {
            let text = if self.fps > 0.0 { format!("{:.0} FPS", self.fps) } else { "-- FPS".into() };
            ui.rect(3.0, 3.0, Ui::text_width(&text) + 6.0, 13.0, [0.0, 0.0, 0.0, 0.65]);
            ui.text(6.0, 6.0, &text, WHITE);
        }
        ui.verts
    }

    fn hotbar_ui(&self, ui: &mut Ui, now: Instant) {
        let hud = HudPlayer {
            inventory: &self.inventory,
            vitals: &self.vitals,
            selected: self.actions.selected,
            survival: self.mode == GameMode::Survival,
            underwater: self.player.head_in_water(&self.world),
        };
        let y0 = self.bar_ui(ui, &hud, now);
        let survival = hud.survival;
        let sw = ui.size().0;

        // Popup message (item names, mode changes), fading out.
        let age = (now - self.popup.1).as_secs_f32();
        let alpha = ((2.5 - age) / 0.5).clamp(0.0, 1.0);
        if alpha > 0.0 {
            let text = capitalize(&self.popup.0);
            let y = y0 - if survival { 31.0 } else { 14.0 };
            ui.text((sw - Ui::text_width(&text)) / 2.0, y, &text, [1.0, 1.0, 1.0, alpha]);
        }
    }

    /// Hotbar plus, in survival, health, armor, hunger and air. Returns the
    /// hotbar's top edge.
    pub(super) fn bar_ui(&self, ui: &mut Ui, hud: &HudPlayer, now: Instant) -> f32 {
        let (sw, sh) = ui.size();
        let slot = 20.0;
        let total = slot * 9.0 + 2.0;
        let (x0, y0) = (((sw - total) / 2.0).floor(), sh - slot - 4.0);
        ui.rect(x0, y0, total, slot + 2.0, [0.0, 0.0, 0.0, 0.5]);
        for i in 0..HOTBAR_SLOTS {
            let sx = x0 + 1.0 + i as f32 * slot;
            ui.rect(sx + 1.0, y0 + 2.0, slot - 2.0, slot - 2.0, [0.35, 0.35, 0.35, 0.5]);
            if i == hud.selected {
                let (x, y, s) = (sx - 1.0, y0 - 1.0, slot + 2.0);
                for (rx, ry, rw, rh) in
                    [(x, y, s, 2.0), (x, y + s, s, 2.0), (x, y, 2.0, s + 2.0), (x + s - 2.0, y, 2.0, s + 2.0)]
                {
                    ui.rect(rx, ry, rw, rh, WHITE);
                }
            }
            if let Some(stack) = hud.inventory.get(i) {
                draw_stack(ui, sx + 1.0, y0 + 2.0, stack, hud.survival);
                // Java's item cooldown: a pale veil that drains downward.
                let cooling = hud.vitals.pearl_cooldown / crate::entity::pearl::COOLDOWN;
                if stack.item == Item::ENDER_PEARL && cooling > 0.0 {
                    let h = (16.0 * cooling).ceil();
                    ui.rect(sx + 2.0, y0 + 3.0 + 16.0 - h, 16.0, h, [1.0, 1.0, 1.0, 0.5]);
                }
            }
        }
        if hud.survival {
            xp_bar_ui(ui, hud.vitals.xp, x0, y0 - 7.0, total);
            self.vitals_ui(ui, hud, x0, x0 + total, y0 - 17.0, now);
        }
        effects_ui(ui, &hud.vitals.effects);
        y0
    }

    /// Hearts above the left half of the hotbar and air bubbles above the
    /// right half, like Minecraft. Hearts shake after a hit and at low
    /// health.
    fn vitals_ui(&self, ui: &mut Ui, hud: &HudPlayer, left: f32, right: f32, y: f32, now: Instant) {
        const ICON: f32 = 9.0;
        const STEP: f32 = 8.0;
        let v = hud.vitals;
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

        // Armor points sit above the hearts: a full chestplate per two
        // points, a dim one for an odd point.
        let armor = hud.inventory.armor_points();
        if armor > 0 {
            let layer = Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Iron).icon_layer().unwrap_or(0);
            for i in 0..10 {
                let color = match armor.saturating_sub(i * 2) {
                    0 => [0.0, 0.0, 0.0, 0.35],
                    1 => [0.55, 0.55, 0.55, 1.0],
                    _ => WHITE,
                };
                ui.icon(left + 1.0 + i as f32 * STEP, y - 10.0, ICON, layer, color);
            }
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
        if hud.underwater || v.air < MAX_AIR {
            for i in 0..v.bubbles().min(AIR_BUBBLES) {
                ui.icon(right - 1.0 - ICON - i as f32 * STEP, y - 10.0, ICON, tex::BUBBLE, WHITE);
            }
        }
    }

    /// Item icon, durability bar and stack count in an 18x18 slot at (x, y).
    fn stack_ui(&self, ui: &mut Ui, x: f32, y: f32, stack: Stack) {
        draw_stack(ui, x, y, stack, self.mode == GameMode::Survival);
    }
}

/// Minecraft's experience bar: a thin green fill across the hotbar's
/// width at `y`, with the level in outlined green text over its middle.
/// The brewing stand's gauges: blaze fuel left under the fuel slot, the
/// brew's progress as an arrow down beside the ingredient and rising
/// bubbles on its other side while it brews.
fn brewing_ui(ui: &mut Ui, b: &crate::world::brewing::BrewingStand, px: f32, py: f32, frame: u32) {
    use crate::world::brewing::{BREW_TIME, FUEL_USES};
    let dark = [0.45, 0.45, 0.45, 1.0];
    // Blaze fuel left, under the fuel slot.
    let (fx, fy) = (px + 16.0, py + 39.0);
    ui.rect(fx, fy, 18.0, 4.0, dark);
    let fuel = (18.0 * b.fuel_left as f32 / FUEL_USES as f32).ceil();
    ui.rect(fx, fy, fuel, 4.0, [0.95, 0.6, 0.15, 1.0]);
    // Pipes from the ingredient down to the three bottles.
    ui.rect(px + 86.0, py + 36.0, 2.0, 23.0, dark);
    ui.rect(px + 63.0, py + 41.0, 48.0, 2.0, dark);
    ui.rect(px + 63.0, py + 41.0, 2.0, 11.0, dark);
    ui.rect(px + 109.0, py + 41.0, 2.0, 11.0, dark);
    // Progress: an arrow filling downward right of the ingredient.
    let (ax, ay) = (px + 99.0, py + 18.0);
    ui.rect(ax, ay, 6.0, 18.0, dark);
    if b.is_brewing() {
        let h = (18.0 * (1.0 - b.brew_left / BREW_TIME)).floor();
        ui.rect(ax, ay, 6.0, h, WHITE);
        // Bubbles rising on the left of the ingredient.
        for i in 0..3u32 {
            let y = py + 34.0 - ((frame + i * 6) % 17) as f32;
            ui.rect(px + 64.0 + i as f32 * 4.0, y, 2.0, 2.0, [0.85, 0.9, 1.0, 1.0]);
        }
    }
}

/// `text` cut to fit `width` UI pixels.
fn clip_text(text: &str, width: f32) -> String {
    let mut out = String::new();
    for c in text.chars() {
        out.push(c);
        if Ui::text_width(&out) > width {
            out.pop();
            break;
        }
    }
    out
}

/// Where enchanting offer `i`'s button sits (x, y, width, height) on a
/// panel whose top section starts at `(px, py)`.
pub(super) fn enchant_offer_rect(px: f32, py: f32, i: usize) -> (f32, f32, f32, f32) {
    (px + 60.0, py + 13.0 + 19.0 * i as f32, 108.0, 19.0)
}

/// Java's status effect icons in the top right corner: beneficial ones in
/// the first row, harmful ones below, blinking in their last 10 seconds.
fn effects_ui(ui: &mut Ui, effects: &Effects) {
    let sw = ui.size().0;
    let (mut good, mut bad) = (0.0, 0.0);
    for a in effects.iter() {
        let column = if a.effect.is_harmful() { &mut bad } else { &mut good };
        *column += 1.0;
        let (x, y) = (sw - 25.0 * *column, if a.effect.is_harmful() { 27.0 } else { 1.0 });
        ui.rect(x, y, 24.0, 24.0, [0.1, 0.1, 0.12, 0.7]);
        let alpha = if a.ticks > 200 { 1.0 } else { 0.6 + 0.4 * (a.ticks as f32 * std::f32::consts::PI / 5.0).cos() };
        ui.icon(x + 3.0, y + 3.0, 18.0, crate::item::effect_icon_layer(a.effect), [1.0, 1.0, 1.0, alpha]);
    }
}

/// Java's effect panels beside the inventory: icon, name and level, and
/// time left; just icons when the screen is too narrow.
fn effect_list_ui(ui: &mut Ui, effects: &Effects, right: f32, top: f32) {
    let label = |a: &effects::Active| format!("{} {}", a.effect.name(), effects::level_name(a.amplifier));
    let text_w = effects.iter().map(|a| Ui::text_width(&label(a))).fold(0.0, f32::max);
    let full = (text_w + 34.0).max(120.0);
    let wide = right >= full + 4.0;
    let w = if wide { full } else { 32.0 };
    let x = (right - w - 4.0).max(0.0);
    for (i, a) in effects.iter().enumerate() {
        let y = top + i as f32 * 33.0;
        ui.rect(x, y, w, 32.0, [0.12, 0.12, 0.14, 0.85]);
        ui.icon(x + 7.0, y + 7.0, 18.0, crate::item::effect_icon_layer(a.effect), WHITE);
        if wide {
            ui.text(x + 28.0, y + 7.0, &label(a), WHITE);
            ui.text(x + 28.0, y + 18.0, &effects::duration_text(a.ticks), [0.5, 0.5, 0.5, 1.0]);
        }
    }
}

/// Java's boss bar: the name over a 182-pixel pink bar, top centre.
fn boss_bar_ui(ui: &mut Ui, name: &str, health: f32, sw: f32) {
    const PINK: [f32; 4] = [0.93, 0.22, 0.73, 1.0];
    let (w, x, y) = (182.0, ((sw - 182.0) / 2.0).floor(), 12.0);
    ui.text(((sw - Ui::text_width(name)) / 2.0).floor(), y - 9.0, name, WHITE);
    ui.rect(x, y, w, 5.0, [0.0, 0.0, 0.0, 0.8]);
    ui.rect(x + 1.0, y + 1.0, w - 2.0, 3.0, [0.3, 0.08, 0.24, 0.9]);
    let fill = ((w - 2.0) * health.clamp(0.0, 1.0)).round();
    if fill > 0.0 {
        ui.rect(x + 1.0, y + 1.0, fill, 3.0, PINK);
        ui.rect(x + 1.0, y + 1.0, fill, 1.0, [1.0, 0.6, 0.9, 1.0]);
    }
}

fn xp_bar_ui(ui: &mut Ui, xp: Experience, x: f32, y: f32, w: f32) {
    const GREEN: [f32; 4] = [0.5, 1.0, 0.125, 1.0];
    ui.rect(x, y, w, 5.0, [0.0, 0.0, 0.0, 0.8]);
    ui.rect(x + 1.0, y + 1.0, w - 2.0, 3.0, [0.12, 0.16, 0.1, 0.9]);
    let fill = ((w - 2.0) * xp.progress()).round();
    if fill > 0.0 {
        ui.rect(x + 1.0, y + 1.0, fill, 3.0, GREEN);
        ui.rect(x + 1.0, y + 1.0, fill, 1.0, [0.75, 1.0, 0.5, 1.0]);
    }
    if xp.level > 0 {
        let text = xp.level.to_string();
        let (tx, ty) = ((x + (w - Ui::text_width(&text)) / 2.0).floor(), y - 6.0);
        for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
            ui.text_flat(tx + dx, ty + dy, &text, [0.0, 0.0, 0.0, 1.0]);
        }
        ui.text_flat(tx, ty, &text, GREEN);
    }
}

/// Item icon, durability bar and (when `counts`) stack size in an 18x18
/// slot at (x, y).
pub(super) fn draw_stack(ui: &mut Ui, x: f32, y: f32, stack: Stack, counts: bool) {
    match (stack.item.block(), stack.item.icon_layer()) {
        (Some(block), _) => ui.block_icon(x + 2.0, y + 2.0, 14.0, block),
        (None, Some(layer)) => {
            ui.icon(x + 1.0, y + 1.0, 16.0, layer, WHITE);
            if !stack.enchants.is_empty() {
                glint(ui, x + 1.0, y + 1.0, 16.0, layer);
            }
        }
        (None, None) => {}
    }
    if let Some(wear) = stack.wear() {
        // Minecraft's bar: green when new, through yellow to red.
        let w = (13.0 * wear).round().max(1.0);
        let color = [(2.0 - 2.0 * wear).min(1.0), (2.0 * wear).min(1.0), 0.0, 1.0];
        ui.rect(x + 2.0, y + 14.0, 13.0, 2.0, [0.0, 0.0, 0.0, 1.0]);
        ui.rect(x + 2.0, y + 14.0, w, 1.0, color);
    }
    if counts && stack.count > 1 {
        let n = stack.count.to_string();
        ui.text(x + 17.0 - Ui::text_width(&n), y + 9.0, &n, WHITE);
    }
}

/// The enchantment glint: a faint purple sheen over the icon's shape with
/// a brighter band sweeping down it.
fn glint(ui: &mut Ui, x: f32, y: f32, size: f32, layer: u16) {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let t = START.get_or_init(Instant::now).elapsed().as_secs_f32();
    ui.icon_shape(x, y, size, layer, [0.0, 0.0, 1.0, 1.0], [0.55, 0.25, 1.0, 0.3]);
    let band = 0.3;
    let top = (t * 0.6).fract() * (1.0 + band) - band;
    let (v0, v1) = (top.max(0.0), (top + band).min(1.0));
    if v1 > v0 {
        ui.icon_shape(x, y, size, layer, [0.0, v0, 1.0, v1], [0.8, 0.6, 1.0, 0.35]);
    }
}

impl Game {
    /// Whether the screen has a top section (crafting grid or furnace)
    /// above the inventory; only the creative inventory doesn't.
    fn has_top_section(&self) -> bool {
        self.mode == GameMode::Survival || self.container != Container::Inventory
    }

    /// Whether the middle grid shows the creative palette instead of the
    /// main inventory.
    fn shows_armor(&self) -> bool {
        self.mode == GameMode::Survival && self.container == Container::Inventory
    }

    /// Height of the top section: room for the four armor slots in the
    /// survival inventory, three rows of slots otherwise.
    fn top_h(&self) -> f32 {
        match (self.has_top_section(), self.shows_armor()) {
            (false, _) => 0.0,
            (true, true) => CRAFT_H + SLOT,
            (true, false) => CRAFT_H,
        }
    }

    /// Vertical offset of the top section's middle row of slots.
    fn top_mid(&self) -> f32 {
        if self.shows_armor() { 1.5 * SLOT } else { SLOT }
    }

    fn shows_palette(&self) -> bool {
        self.mode == GameMode::Creative && self.container == Container::Inventory
    }

    pub(super) fn shows_recipes(&self) -> bool {
        self.has_top_section()
            && !matches!(
                self.container,
                Container::Furnace(_)
                    | Container::Chest(_)
                    | Container::Enchanting(_)
                    | Container::Anvil(_)
                    | Container::Smithing(_)
            )
    }

    /// Top-left corner and height of the inventory panel.
    fn panel(&self, screen: (f32, f32)) -> (f32, f32, f32) {
        let h = PANEL_H + self.top_h() + super::search::HEIGHT;
        let extra = if self.recipe_book.open && self.shows_recipes() && recipe_book::fits_beside(screen.0, PANEL_W) {
            recipe_book::WIDTH + recipe_book::GAP
        } else {
            0.0
        };
        (((screen.0 - PANEL_W - extra) / 2.0).floor(), ((screen.1 - h) / 2.0).floor(), h)
    }

    pub(super) fn search_bounds(&self, screen: (f32, f32)) -> (f32, f32, f32) {
        let (x, y, _) = self.panel(screen);
        (x + 7.0, y + 4.0, PANEL_W - 14.0)
    }

    fn recipe_layout(&self, screen: (f32, f32)) -> Layout {
        let (x, y, h) = self.panel(screen);
        Layout::new(screen.0, Rect { x, y: y + super::search::HEIGHT, w: PANEL_W, h: h - super::search::HEIGHT })
    }

    pub(super) fn recipe_control_under_cursor(&self) -> Option<Control> {
        if !self.shows_recipes() {
            return None;
        }
        let scale = self.ui_scale();
        let (w, h) = self.ui_size();
        self.recipe_layout((w as f32 / scale, h as f32 / scale))
            .control((self.cursor_px.0 / scale, self.cursor_px.1 / scale), self.recipe_book.open)
    }

    /// Screen positions of every slot on the inventory screen: the open
    /// container's slots (a crafting grid and its result outside creative, a
    /// furnace or a chest), a 3x9 grid (main inventory, or the block palette
    /// in creative) and the hotbar.
    fn inventory_slots(&self, screen: (f32, f32)) -> Vec<(SlotRef, f32, f32)> {
        let (px, py, _) = self.panel(screen);
        let py = py + super::search::HEIGHT;
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
        } else if let Container::Enchanting(_) = self.container {
            // Java's layout: item and lapis bottom left, offers to the right
            // (drawn as buttons; see `enchant_offer_rect`).
            out.push((SlotRef::EnchantItem, px + 14.0, py + 44.0));
            out.push((SlotRef::EnchantLapis, px + 34.0, py + 44.0));
            CRAFT_H
        } else if let Container::Anvil(_) = self.container {
            // Java's layout: input + input -> result.
            out.push((SlotRef::AnvilLeft, px + 26.0, py + 36.0));
            out.push((SlotRef::AnvilRight, px + 75.0, py + 36.0));
            out.push((SlotRef::AnvilResult, px + 133.0, py + 36.0));
            CRAFT_H
        } else if let Container::Smithing(_) = self.container {
            // Java 1.21: template, base, addition -> result.
            out.push((SlotRef::SmithTemplate, px + 15.0, py + 48.0));
            out.push((SlotRef::SmithBase, px + 33.0, py + 48.0));
            out.push((SlotRef::SmithAddition, px + 51.0, py + 48.0));
            out.push((SlotRef::SmithResult, px + 105.0, py + 48.0));
            CRAFT_H
        } else if let Container::Brewing(_) = self.container {
            // Java's layout: fuel top left, the ingredient over three
            // bottles in a fan.
            out.push((SlotRef::BrewFuel, px + 16.0, py + 18.0));
            out.push((SlotRef::BrewIngredient, px + 78.0, py + 18.0));
            for (i, (x, y)) in [(55.0, 52.0), (78.0, 59.0), (101.0, 52.0)].into_iter().enumerate() {
                out.push((SlotRef::BrewBottle(i), px + x, py + y));
            }
            CRAFT_H
        } else if self.has_top_section() {
            let n = self.craft.size;
            // The grid sits left of centre, the result to its right past an
            // arrow; worn armor runs down the left edge.
            let gx = px + 7.0 + if n == 3 { SLOT } else { 2.0 * SLOT };
            let gy = py + 18.0 + self.top_mid() - (n - 1) as f32 * SLOT / 2.0;
            for i in 0..n * n {
                out.push((SlotRef::Craft(i), gx + (i % n) as f32 * SLOT, gy + (i / n) as f32 * SLOT));
            }
            out.push((SlotRef::CraftResult, px + 7.0 + 6.0 * SLOT, py + 18.0 + self.top_mid()));
            if self.shows_armor() {
                for (i, piece) in ArmorPiece::ALL.into_iter().enumerate() {
                    out.push((SlotRef::Armor(piece), px + 7.0, py + 18.0 + i as f32 * SLOT));
                }
            }
            self.top_h()
        } else {
            0.0
        };
        let py = py + top;
        let grid = |i: usize| (px + 7.0 + (i % 9) as f32 * SLOT, py + 18.0 + (i / 9) as f32 * SLOT);
        if self.shows_palette() {
            let first = self.creative_scroll * 9;
            for (i, item) in Item::creative_palette()
                .filter(|item| item.matches_query(&self.search.query))
                .skip(first)
                .take(PALETTE_ROWS * 9)
                .enumerate()
            {
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
        let scale = self.ui_scale();
        let (w, h) = self.ui_size();
        let screen = (w as f32 / scale, h as f32 / scale);
        let mouse = (self.cursor_px.0 / scale, self.cursor_px.1 / scale);
        let (x, y, h) = self.panel(screen);
        let on_book =
            self.shows_recipes() && self.recipe_book.open && self.recipe_layout(screen).bounds.contains(mouse);
        !Rect { x, y, w: PANEL_W, h }.contains(mouse) && !on_book
    }

    pub(super) fn slot_under_cursor(&self) -> Option<SlotRef> {
        let scale = self.ui_scale();
        let (w, h) = self.ui_size();
        let (mx, my) = (self.cursor_px.0 / scale, self.cursor_px.1 / scale);
        if self.shows_recipes()
            && self.recipe_book.open
            && self.recipe_layout((w as f32 / scale, h as f32 / scale)).overlay
        {
            return None;
        }
        if let Container::Enchanting(_) = self.container {
            let (px, py, _) = self.panel((w as f32 / scale, h as f32 / scale));
            for i in 0..3 {
                let (x, y, bw, bh) = enchant_offer_rect(px, py + super::search::HEIGHT, i);
                if mx >= x && mx < x + bw && my >= y && my < y + bh {
                    return Some(SlotRef::EnchantOffer(i));
                }
            }
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
        effect_list_ui(ui, &self.vitals.effects, px, py);
        ui.rect(px, py, PANEL_W, panel_h, [0.78, 0.78, 0.78, 1.0]);
        ui.rect(px, py, PANEL_W, 1.0, WHITE);
        ui.rect(px, py, 1.0, panel_h, WHITE);
        ui.rect(px, py + panel_h - 1.0, PANEL_W, 1.0, [0.33, 0.33, 0.33, 1.0]);
        ui.rect(px + PANEL_W - 1.0, py, 1.0, panel_h, [0.33, 0.33, 0.33, 1.0]);
        self.search_ui(ui);
        let py = py + super::search::HEIGHT;
        let title = match (self.container, self.mode) {
            (Container::CraftingTable, _) => "Crafting",
            (Container::Furnace(_), _) => "Furnace",
            (Container::Chest(_), _) => "Chest",
            (Container::Brewing(_), _) => "Brewing Stand",
            (Container::Enchanting(_), _) => "Enchant",
            (Container::Anvil(_), _) => "Anvil",
            (Container::Smithing(_), _) => "Upgrade Gear",
            (Container::Inventory, GameMode::Survival) => "Inventory",
            (Container::Inventory, GameMode::Creative) => "Creative",
        };
        ui.text_flat(px + 8.0, py + 6.0, title, [0.25, 0.25, 0.25, 1.0]);
        if self.shows_recipes() {
            let layout = self.recipe_layout((sw, sh));
            self.recipe_button(ui, layout.toggle, if self.recipe_book.open { "Hide" } else { "Recipes" });
        }
        if let Container::Brewing(p) = self.container
            && let Some(b) = self.world.brewing_stand(p)
        {
            brewing_ui(ui, b, px, py, (self.started.elapsed().as_secs_f32() * 8.0) as u32);
        } else if let Container::Enchanting(_) = self.container {
            self.enchanting_ui(ui, px, py);
        } else if let Container::Anvil(_) = self.container {
            self.anvil_ui(ui, px, py);
        } else if let Container::Smithing(_) = self.container {
            self.smithing_ui(ui, px, py);
        } else if self.has_top_section() && !matches!(self.container, Container::Chest(_)) {
            // Arrow toward the result; in a furnace it fills with progress
            // and a flame between input and fuel shows the fuel left.
            let (ax, ay) = (px + 7.0 + 4.0 * SLOT + 9.0, py + 18.0 + self.top_mid() + 5.0);
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
        let py = py + self.top_h();
        if matches!(self.container, Container::Chest(_)) {
            ui.text_flat(px + 8.0, py + 6.0, "Inventory", [0.25, 0.25, 0.25, 1.0]);
        }
        if self.shows_palette() {
            // Scrollbar beside the palette grid.
            let (x, y, h) = (px + PANEL_W - 6.0, py + 18.0, PALETTE_ROWS as f32 * SLOT);
            let rows = palette_rows(&self.search.query).max(1) as f32;
            let thumb = h * (PALETTE_ROWS as f32 / rows).min(1.0);
            let top = y + (h - thumb) * self.creative_scroll as f32 / (rows - PALETTE_ROWS as f32).max(1.0);
            ui.rect(x, y, 4.0, h, [0.45, 0.45, 0.45, 1.0]);
            ui.rect(x, top, 4.0, thumb, WHITE);
        }

        if self.shows_palette() && palette_rows(&self.search.query) == 0 {
            ui.text_flat(px + 8.0, py + 30.0, "No matching items", [0.3, 0.3, 0.3, 1.0]);
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
                SlotRef::Armor(p) => self.inventory.armor[p as usize],
                f => self.container_slot(f),
            };
            // Faint outline of what goes in an empty armor or lapis slot.
            let outline = match (r, stack) {
                (SlotRef::Armor(p), None) => Item::armor(p, ArmorMaterial::Iron).icon_layer(),
                (SlotRef::EnchantLapis, None) => Item::LAPIS_LAZULI.icon_layer(),
                (SlotRef::SmithTemplate, None) => Item::NETHERITE_UPGRADE.icon_layer(),
                (SlotRef::SmithBase, None) => Item::armor(ArmorPiece::Chestplate, ArmorMaterial::Diamond).icon_layer(),
                (SlotRef::SmithAddition, None) => Item::NETHERITE_INGOT.icon_layer(),
                _ => None,
            };
            if let Some(layer) = outline {
                ui.icon(x + 1.0, y + 1.0, 16.0, layer, [0.0, 0.0, 0.0, 0.25]);
            }
            if let Some(stack) = stack {
                self.stack_ui(ui, x, y, stack);
            }
            if !self.search.query.trim().is_empty()
                && let Some(stack) = stack
            {
                if stack.item.matches_query(&self.search.query) {
                    ui.rect(x, y, SLOT, 1.0, [1.0, 0.85, 0.2, 1.0]);
                } else {
                    ui.rect(x + 1.0, y + 1.0, SLOT - 2.0, SLOT - 2.0, [0.1, 0.1, 0.1, 0.7]);
                }
            }

            if hovered == Some(r) {
                ui.rect(x + 1.0, y + 1.0, SLOT - 2.0, SLOT - 2.0, [1.0, 1.0, 1.0, 0.35]);
            }
        }
        let hovered_stack = match hovered {
            Some(SlotRef::Inventory(i)) => self.inventory.get(i),
            Some(SlotRef::Palette(item)) => Some(Stack::new(item, 1)),
            Some(SlotRef::Craft(i)) => self.craft.cells[i],
            Some(SlotRef::CraftResult) => self.craft.result(),
            Some(SlotRef::Armor(p)) => self.inventory.armor[p as usize],
            Some(f) => self.container_slot(f),
            None => None,
        };
        let recipe_hint = if self.recipe_book.open && self.shows_recipes() { self.recipe_book_ui(ui) } else { None };
        if let Some(hint) = recipe_hint {
            self.tooltip(ui, &hint);
        } else if let Some(SlotRef::EnchantOffer(i)) = hovered {
            self.offer_tooltip(ui, i);
        } else if let Some(stack) = hovered_stack {
            match stack.item.as_potion() {
                Some(potion) => self.potion_tooltip(ui, stack.display_name(), potion),
                None if !stack.enchants.is_empty() => self.enchant_tooltip(ui, stack),
                None => self.tooltip(ui, stack.display_name()),
            }
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
        match slot {
            SlotRef::EnchantItem => return self.work[0],
            SlotRef::EnchantLapis | SlotRef::AnvilRight => return self.work[1],
            SlotRef::AnvilLeft => return self.work[0],
            SlotRef::AnvilResult => return self.anvil_preview().map(|(r, _)| r.output),
            SlotRef::SmithTemplate => return self.work[0],
            SlotRef::SmithBase => return self.work[1],
            SlotRef::SmithAddition => return self.work[2],
            SlotRef::SmithResult => return self.smithing_result(),
            _ => {}
        }
        if let Container::Brewing(p) = self.container {
            let b = self.world.brewing_stand(p)?;
            return match slot {
                SlotRef::BrewBottle(i) => b.bottles[i],
                SlotRef::BrewIngredient => b.ingredient,
                SlotRef::BrewFuel => b.fuel,
                _ => None,
            };
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

    /// The enchanting table's offers: rune words, a lapis count badge and
    /// the level cost (green if affordable); greyed out when it can't be
    /// taken. Hovering one shows its clue and price.
    fn enchanting_ui(&self, ui: &mut Ui, px: f32, py: f32) {
        let offers = self.enchant_offers();
        let hovered = self.slot_under_cursor();
        for (i, offer) in offers.into_iter().enumerate() {
            let (x, y, w, h) = enchant_offer_rect(px, py, i);
            let usable = self.can_take_offer(i, offer);
            let lit = usable && hovered == Some(SlotRef::EnchantOffer(i));
            let face = match (offer.cost > 0, usable, lit) {
                (false, _, _) => [0.55, 0.5, 0.5, 1.0],
                (true, false, _) => [0.6, 0.55, 0.5, 1.0],
                (true, true, false) => [0.72, 0.66, 0.55, 1.0],
                (true, true, true) => [0.82, 0.76, 0.6, 1.0],
            };
            ui.rect(x, y, w, h, [0.3, 0.3, 0.3, 1.0]);
            ui.rect(x + 1.0, y + 1.0, w - 2.0, h - 2.0, face);
            if offer.cost == 0 {
                continue;
            }
            // The lapis price: a dark badge with 1-3 pips.
            ui.rect(x + 2.0, y + 2.0, 15.0, 15.0, if usable { [0.16, 0.3, 0.7, 1.0] } else { [0.3, 0.3, 0.35, 1.0] });
            for p in 0..=i {
                ui.rect(x + 4.0 + 4.0 * p as f32, y + 8.0, 3.0, 3.0, [0.85, 0.9, 1.0, 1.0]);
            }
            let words = super::enchanting::rune_words(self.vitals.xp.seed, i);
            let ink = if usable { [0.42, 0.36, 0.25, 1.0] } else { [0.4, 0.37, 0.33, 1.0] };
            ui.text_flat(x + 20.0, y + 6.0, &clip_text(&words, w - 44.0), ink);
            let cost = offer.cost.to_string();
            let colour = if usable { [0.5, 1.0, 0.13, 1.0] } else { [0.25, 0.4, 0.13, 1.0] };
            ui.text(x + w - 3.0 - Ui::text_width(&cost), y + 9.0, &cost, colour);
        }
    }

    /// The anvil's plus and arrow, and Java's cost line: green "Enchantment
    /// Cost: n" (red if short of levels) or "Too Expensive!".
    fn anvil_ui(&self, ui: &mut Ui, px: f32, py: f32) {
        let dark = [0.45, 0.45, 0.45, 1.0];
        // A plus between the inputs and an arrow to the result.
        ui.rect(px + 55.0, py + 44.0, 13.0, 3.0, dark);
        ui.rect(px + 60.0, py + 39.0, 3.0, 13.0, dark);
        ui.rect(px + 102.0, py + 44.0, 18.0, 3.0, dark);
        for i in 0..5 {
            ui.rect(px + 120.0 + i as f32, py + 41.0 + i as f32, 1.0, 9.0 - 2.0 * i as f32, dark);
        }
        let Some((result, too_expensive)) = self.anvil_preview() else {
            if self.work[0].is_some() && self.work[1].is_some() {
                // Inputs that can't combine: Java crosses out the arrow.
                ui.rect(px + 104.0, py + 38.0, 14.0, 2.0, [0.75, 0.2, 0.2, 1.0]);
            }
            return;
        };
        let (text, colour) = if too_expensive {
            ("Too Expensive!".to_string(), [1.0, 0.38, 0.38, 1.0])
        } else if self.mode == GameMode::Survival && self.vitals.xp.level < result.cost {
            (format!("Enchantment Cost: {}", result.cost), [1.0, 0.38, 0.38, 1.0])
        } else {
            (format!("Enchantment Cost: {}", result.cost), [0.5, 1.0, 0.13, 1.0])
        };
        let w = Ui::text_width(&text);
        let x = px + PANEL_W - 8.0 - w;
        ui.rect(x - 2.0, py + 58.0, w + 4.0, 11.0, [0.25, 0.25, 0.25, 0.7]);
        ui.text(x, py + 60.0, &text, colour);
    }

    /// Smithing's arrow and invalid-recipe cross. Slot outlines explain the
    /// template/base/addition order without reproducing Java's full artwork.
    fn smithing_ui(&self, ui: &mut Ui, px: f32, py: f32) {
        let dark = [0.45, 0.45, 0.45, 1.0];
        ui.rect(px + 76.0, py + 56.0, 18.0, 3.0, dark);
        for i in 0..5 {
            ui.rect(px + 94.0 + i as f32, py + 53.0 + i as f32, 1.0, 9.0 - 2.0 * i as f32, dark);
        }
        if self.work.iter().all(Option::is_some) && self.smithing_result().is_none() {
            ui.rect(px + 79.0, py + 49.0, 14.0, 2.0, [0.75, 0.2, 0.2, 1.0]);
        }
    }

    /// Java's offer tooltip: the clue enchantment with "...?", then the
    /// lapis and levels it costs (red when short).
    pub(super) fn offer_tooltip(&self, ui: &mut Ui, i: usize) {
        let offer = self.enchant_offers()[i];
        let Some((e, level)) = offer.clue else { return };
        let creative = self.mode == GameMode::Creative;
        let mut lines = vec![(format!("{} . . . ?", e.describe(level)), WHITE)];
        if !creative {
            let lapis = self.work[1].map_or(0, |s| s.count) as usize;
            let red = [1.0, 0.33, 0.33, 1.0];
            let grey = [0.67, 0.67, 0.67, 1.0];
            if self.vitals.xp.level < offer.cost {
                lines.push((format!("Enchantment Level Requirement: {}", offer.cost), red));
            } else {
                let n = i + 1;
                let plural = |s: &str| if n == 1 { s.to_string() } else { format!("{s}s") };
                lines.push((format!("{n} {}", plural("Lapis Lazuli")), if lapis > i { grey } else { red }));
                lines.push((format!("{n} {}", plural("Enchantment Level")), grey));
            }
        }
        let w = lines.iter().map(|(l, _)| Ui::text_width(l)).fold(0.0, f32::max);
        let h = 3.0 + 11.0 * lines.len() as f32;
        let (sw, sh) = ui.size();
        let x = (self.cursor_px.0 / ui.scale + 10.0).min(sw - w - 6.0).max(3.0);
        let y = (self.cursor_px.1 / ui.scale - 12.0).clamp(3.0, (sh - h).max(3.0));
        ui.rect(x - 3.0, y - 3.0, w + 6.0, h + 3.0, [0.08, 0.02, 0.12, 0.92]);
        for (n, (line, colour)) in lines.iter().enumerate() {
            ui.text(x, y + 11.0 * n as f32, line, *colour);
        }
    }

    /// An enchanted item's name (aqua; an enchanted book's yellow) with a
    /// grey line per enchantment, curses in red, like Java.
    pub(super) fn enchant_tooltip(&self, ui: &mut Ui, stack: Stack) {
        let title = capitalize(stack.display_name());
        let lines = stack.enchants.lines();
        let w = lines.iter().map(|(l, _)| Ui::text_width(l)).fold(Ui::text_width(&title), f32::max);
        let h = 14.0 + 11.0 * lines.len() as f32;
        let (sw, sh) = ui.size();
        let x = (self.cursor_px.0 / ui.scale + 10.0).min(sw - w - 6.0).max(3.0);
        let y = (self.cursor_px.1 / ui.scale - 12.0).clamp(3.0, (sh - h + 3.0).max(3.0));
        ui.rect(x - 3.0, y - 3.0, w + 6.0, h, [0.08, 0.02, 0.12, 0.92]);
        let colour = if stack.item == Item::ENCHANTED_BOOK { [1.0, 1.0, 0.33, 1.0] } else { [0.33, 1.0, 1.0, 1.0] };
        ui.text(x, y, &title, colour);
        for (i, (line, curse)) in lines.iter().enumerate() {
            let c = if *curse { [1.0, 0.33, 0.33, 1.0] } else { [0.67, 0.67, 0.67, 1.0] };
            ui.text(x, y + 11.0 * (i + 1) as f32, line, c);
        }
    }

    /// A potion's name with Java's effect line under it: blue for good
    /// effects, red for harmful, grey for none.
    fn potion_tooltip(&self, ui: &mut Ui, name: &str, potion: voxelcraft::potion::Potion) {
        let title = capitalize(name);
        let detail = potion.describe();
        let colour = match potion.info().effect {
            None => [0.6, 0.6, 0.6, 1.0],
            Some((e, _, _)) if e.is_harmful() => [1.0, 0.33, 0.33, 1.0],
            Some(_) => [0.33, 0.33, 1.0, 1.0],
        };
        let w = Ui::text_width(&title).max(Ui::text_width(&detail));
        let (sw, sh) = ui.size();
        let x = (self.cursor_px.0 / ui.scale + 10.0).min(sw - w - 6.0).max(3.0);
        let y = (self.cursor_px.1 / ui.scale - 12.0).clamp(3.0, (sh - 22.0).max(3.0));
        ui.rect(x - 3.0, y - 3.0, w + 6.0, 25.0, [0.08, 0.02, 0.12, 0.92]);
        ui.text(x, y, &title, WHITE);
        ui.text(x, y + 11.0, &detail, colour);
    }

    /// Draw F3 world/player statistics, including completed gameplay ticks and offline pause state.
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
            match self.dimension {
                crate::world::terrain::Dimension::Overworld => format!("Biome: {biome:?}"),
                d => format!("Dimension: {}", d.name()),
            },
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
                "Tick: {} (20 Hz{})",
                self.clock.ticks(),
                if (self.menu.is_some() || self.console.open) && self.agents.host.is_none() { ", paused" } else { "" }
            ),
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

/// "Eating" and the bite's progress, below the crosshair at (cx, cy).
pub(super) fn eating_bar(ui: &mut Ui, cx: f32, cy: f32, progress: f32) {
    ui.text(cx - Ui::text_width("Eating") / 2.0, cy + 14.0, "Eating", WHITE);
    ui.rect(cx - 30.0, cy + 25.0, 60.0, 5.0, [0.0, 0.0, 0.0, 0.7]);
    ui.rect(cx - 29.0, cy + 26.0, 58.0 * progress.clamp(0.0, 1.0), 3.0, [0.96, 0.72, 0.3, 1.0]);
}

/// Bow draw strength under the crosshair at (cx, cy); gold when full.
pub(super) fn bow_bar(ui: &mut Ui, cx: f32, cy: f32, power: f32) {
    let full = power >= 1.0;
    ui.rect(cx - 12.0, cy + 12.0, 24.0, 4.0, [0.0, 0.0, 0.0, 0.7]);
    let colour = if full { [1.0, 0.95, 0.5, 1.0] } else { [0.85, 0.85, 0.85, 1.0] };
    ui.rect(cx - 11.0, cy + 13.0, 22.0 * power, 2.0, colour);
}
