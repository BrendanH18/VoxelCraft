//! v1.0 gap track 6: a bundle's contents in a container screen, as Java
//! 26.3's client shows them (`BundleMouseActions`, `ClientBundleTooltip`,
//! `BundleSelectedItemSpecialRenderer`).
//!
//! Hovering a bundle shows its contents tooltip: a grid of up to twelve
//! stacks, four across, filled from the bottom right with the front stack
//! last ("+n" in the bottom-right cell when more don't fit), over a
//! fullness bar reading "Empty" or "Full" at the ends; an empty bundle
//! shows "Can hold a mixed stack of items" over its empty bar. The wheel
//! over a bundle selects one of the shown stacks (down the next, up the
//! previous, wrapping); the selected cell is highlighted, its name shows
//! above the tooltip, the slot draws the bundle open around it, and a
//! right click with an empty cursor takes that stack out instead of the
//! front one. The selection lasts while the bundle stays hovered: moving
//! off it, taking a stack out, a shift-click or any click other than
//! putting a stack in clears it (`onStopHovering`, `removeOne`,
//! `toggleSelectedItem(-1)`).
use super::Game;
use super::gui::style;
use super::hud::{SlotRef, draw_stack};
use crate::inventory::Stack;
use crate::render::ui::{Ui, WHITE};
use voxelcraft::content::bundle;

/// `ClientBundleTooltip`'s cell and grid sizes.
const CELL: f32 = 24.0;
const GRID_W: f32 = 96.0;
/// The fullness bar: 96 x 13, filled up to 94 pixels one in.
const BAR_H: f32 = 13.0;
const BAR_FILL: f32 = 94.0;
/// `item.minecraft.bundle.empty.description`, wrapped to the grid's width
/// in Java's grey (-5592406, `0xFFAAAAAA`).
const EMPTY_DESCRIPTION: &str = "Can hold a mixed stack of items";
const DESCRIPTION_GREY: [f32; 4] = [0.667, 0.667, 0.667, 1.0];

impl Game {
    /// The selected stack of the bundle in `slot` while it is the hovered
    /// one.
    pub(super) fn bundle_selected_at(&self, slot: SlotRef) -> Option<usize> {
        self.bundle_selection.filter(|&(s, _)| s == slot).map(|(_, i)| i)
    }

    /// Once a frame: the selection goes when its bundle is no longer
    /// hovered or no longer holds the stack (`onStopHovering`).
    pub(super) fn track_bundle_hover(&mut self) {
        let Some((slot, i)) = self.bundle_selection else { return };
        let keep = self.inventory_open
            && self.slot_under_cursor() == Some(slot)
            && self.slot_stack(slot).is_some_and(|s| bundle::is_bundle(s.item) && i < s.bundle_contents().len());
        if !keep {
            self.bundle_selection = None;
        }
    }

    /// The wheel over a hovered bundle that shows stacks moves its
    /// selection (`BundleMouseActions.onMouseScrolled`, whole notches with
    /// its own `ScrollWheelHandler`). Returns whether the wheel was taken.
    pub(super) fn bundle_wheel(&mut self, lines: f64) -> bool {
        let Some(slot) = self.slot_under_cursor() else { return false };
        let Some(stack) = self.slot_stack(slot).filter(|s| bundle::is_bundle(s.item)) else { return false };
        let limit = bundle::shown_count(stack.bundle_contents().len());
        if limit == 0 {
            return false;
        }
        if self.bundle_scroll != 0.0 && lines.signum() != self.bundle_scroll.signum() {
            self.bundle_scroll = 0.0;
        }
        self.bundle_scroll += lines;
        let wheel = self.bundle_scroll.trunc();
        if wheel != 0.0 {
            self.bundle_scroll -= wheel;
            let current = self.bundle_selected_at(slot);
            let next = bundle::next_selection(wheel, current, limit);
            self.bundle_selection = next.map(|i| (slot, i));
        }
        true
    }

    /// After a click on `slot`: putting a stack into the hovered bundle
    /// keeps its selection; anything else clears it.
    pub(super) fn bundle_clicked(&mut self, click: Option<bundle::Click>) {
        if !click.is_some_and(bundle::Click::keeps_selection) {
            self.bundle_selection = None;
        }
    }

    /// The stack the hovered bundle in `slot` holds up: its selection.
    pub(super) fn bundle_selected_stack(&self, slot: SlotRef, stack: &Stack) -> Option<Stack> {
        let i = self.bundle_selected_at(slot)?;
        bundle::is_bundle(stack.item).then(|| stack.bundle_contents().get(i).copied()).flatten()
    }

    /// `ClientBundleTooltip` under the bundle's name, as our tooltips draw a
    /// name: the box fits the wider of the name and the 96-pixel grid.
    pub(super) fn bundle_tooltip(&self, ui: &mut Ui, stack: Stack, selected: Option<usize>, title_colour: [f32; 4]) {
        let at = (self.cursor_px.0 / ui.scale, self.cursor_px.1 / ui.scale);
        self.bundle_tooltip_at(ui, stack, selected, title_colour, at, self.dial_of(&self.session.player));
    }

    /// [`Game::bundle_tooltip`] beside `at` (UI pixels: the mouse, or a
    /// controller screen's cursor, v1.0 gap track 6b).
    pub(super) fn bundle_tooltip_at(
        &self,
        ui: &mut Ui,
        stack: Stack,
        selected: Option<usize>,
        title_colour: [f32; 4],
        at: (f32, f32),
        dial: crate::item::Dial,
    ) {
        let title = super::capitalize(stack.display_name());
        let contents = stack.bundle_contents();
        let lines = if contents.is_empty() { wrap(EMPTY_DESCRIPTION, GRID_W) } else { Vec::new() };
        let image_h = image_height(contents.len(), lines.len());
        // v1.0 gap track 6b: the advanced lines (F3+H) follow the image, as
        // Java puts a tooltip image right after the name.
        let extra = self.advanced_lines(&stack);
        let w = extra.iter().map(|(l, _)| Ui::text_width(l)).fold(Ui::text_width(&title).max(GRID_W), f32::max);
        // The name, a 2-pixel gap, then the image (`ClientTooltipComponent`
        // heights: 10 for a line of text).
        let h = 12.0 + image_h + 10.0 * extra.len() as f32;
        let (sw, sh) = ui.size();
        let x = (at.0 + 10.0).min(sw - w - 6.0).max(4.0).floor();
        let y = (at.1 - 12.0).clamp(4.0, (sh - h).max(4.0)).floor();
        style::tooltip_box(ui, x, y, w, h - 2.0);
        ui.text(x, y, &title, title_colour);
        let left = x + ((w - GRID_W) / 2.0).floor();
        let top = y + 12.0;
        for (k, (line, c)) in extra.iter().enumerate() {
            ui.text(x, top + image_h + 10.0 * k as f32, line, *c);
        }
        if contents.is_empty() {
            for (k, line) in lines.iter().enumerate() {
                ui.text(left, top + 9.0 * k as f32, line, DESCRIPTION_GREY);
            }
            fullness_bar(ui, left, top + 9.0 * lines.len() as f32 + 4.0, bundle::Fraction::ZERO);
            return;
        }
        let shown = bundle::shown_count(contents.len());
        let overflowing = contents.len() > bundle::SHOWN_MAX;
        let rows = contents.len().min(bundle::SHOWN_MAX).div_ceil(bundle::SHOWN_COLUMNS);
        let mut cell = 1;
        for row in 1..=rows {
            for column in 1..=bundle::SHOWN_COLUMNS {
                let cx = left + GRID_W - column as f32 * CELL;
                let cy = top + rows as f32 * CELL - row as f32 * CELL;
                if overflowing && column * row == 1 {
                    let hidden: u32 = contents[shown..].iter().map(|s| u32::from(s.count)).sum();
                    let text = format!("+{hidden}");
                    ui.text((cx + 12.0 - Ui::text_width(&text) / 2.0).floor(), cy + 10.0, &text, WHITE);
                } else if cell <= shown {
                    // The front stack is the last cell drawn.
                    let index = shown - cell;
                    let lit = selected == Some(index);
                    cell_background(ui, cx, cy, lit);
                    draw_stack(ui, cx + 3.0, cy + 3.0, contents[index], true, dial);
                    if lit {
                        cell_highlight(ui, cx, cy);
                    }
                    cell += 1;
                }
            }
        }
        let weight = bundle::contents_weight(contents).unwrap_or(bundle::Fraction::ONE);
        fullness_bar(ui, left, top + rows as f32 * CELL + 4.0, weight);
        // `extractSelectedItemTooltip`: the selected stack's name in its own
        // box, centred 12 pixels left of the tooltip's middle, 15 above it.
        if let Some(chosen) = selected.and_then(|i| contents.get(i)) {
            let name = super::capitalize(chosen.display_name());
            let nw = Ui::text_width(&name);
            let nx = (x + w / 2.0 - 12.0 - nw / 2.0).floor();
            let ny = y - 15.0;
            style::tooltip_box(ui, nx, ny, nw, 8.0);
            ui.text(nx, ny, &name, WHITE);
        }
    }
}

/// The tooltip image's height (`getHeight`): the grid's rows and the bar
/// with its margins, or the description's lines and the bar.
fn image_height(stacks: usize, description_lines: usize) -> f32 {
    if stacks == 0 {
        9.0 * description_lines as f32 + BAR_H + 8.0
    } else {
        stacks.min(bundle::SHOWN_MAX).div_ceil(bundle::SHOWN_COLUMNS) as f32 * CELL + BAR_H + 8.0
    }
}

/// Words of `text` in lines at most `width` wide (`Font.split`).
fn wrap(text: &str, width: f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split(' ') {
        match lines.last_mut() {
            Some(line) if Ui::text_width(&format!("{line} {word}")) <= width => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    lines
}

/// A cell's backing (`slot_background`, or `slot_highlight_back` when
/// selected): an original sunken 24-pixel square.
fn cell_background(ui: &mut Ui, x: f32, y: f32, lit: bool) {
    let fill = if lit { 0xA0A0C8 } else { 0x8B8B8B };
    style::inset(ui, x + 1.0, y + 1.0, CELL - 2.0, CELL - 2.0, fill);
}

/// `slot_highlight_front`: a white frame over the selected cell.
fn cell_highlight(ui: &mut Ui, x: f32, y: f32) {
    style::outline(ui, [x, y, CELL, CELL], WHITE);
    style::outline(ui, [x + 1.0, y + 1.0, CELL - 2.0, CELL - 2.0], style::veil(0xFFFFFF, 0.5));
}

/// The fullness bar (`extractProgressbar`): blue up to the weight, red once
/// full, in a light frame, reading "Empty" or "Full" at the ends.
fn fullness_bar(ui: &mut Ui, x: f32, y: f32, weight: bundle::Fraction) {
    let full = weight >= bundle::Fraction::ONE;
    // `Mth.mulAndTruncate(weight, 94)`, clamped.
    let fill =
        ((weight.numerator() as u128 * BAR_FILL as u128 / weight.denominator().max(1) as u128) as f32).min(BAR_FILL);
    let colour = if full { [1.0, 0.33, 0.33, 1.0] } else { [0.44, 0.53, 1.0, 1.0] };
    ui.rect(x + 1.0, y + 1.0, BAR_FILL, BAR_H - 2.0, style::veil(0x000000, 0.5));
    if fill > 0.0 {
        ui.rect(x + 1.0, y + 1.0, fill, BAR_H - 2.0, colour);
    }
    style::outline(ui, [x, y, GRID_W, BAR_H], [0.85, 0.85, 0.85, 1.0]);
    let label = if weight == bundle::Fraction::ZERO {
        Some("Empty")
    } else if full {
        Some("Full")
    } else {
        None
    };
    if let Some(text) = label {
        ui.text((x + 48.0 - Ui::text_width(text) / 2.0).floor(), y + 3.0, text, WHITE);
    }
}

/// The slot drawing of a hovered bundle with a selected stack: the open
/// bundle's back, the stack, then its front, with the bundle's own bar
/// (`bundle/selected_item` between `bundle_open_back` and `_front`).
pub(super) fn draw_open_bundle(ui: &mut Ui, x: f32, y: f32, stack: Stack, chosen: Stack, dial: crate::item::Dial) {
    let Some((back, front)) = crate::item::open_bundle_layers(stack.item) else { return };
    ui.icon(x + 1.0, y + 1.0, 16.0, back, WHITE);
    super::hud::draw_model(ui, x, y, chosen, dial);
    ui.icon(x + 1.0, y + 1.0, 16.0, front, WHITE);
    super::hud::draw_decorations(ui, x, y, stack, false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tooltip_image_is_as_tall_as_javas() {
        // `backgroundHeight`: rows of 24 plus the 13-pixel bar and 8 of margins.
        assert_eq!(image_height(1, 0), 24.0 + 21.0);
        assert_eq!(image_height(5, 0), 48.0 + 21.0);
        assert_eq!(image_height(40, 0), 72.0 + 21.0);
        // An empty bundle: the description's lines of 9 instead.
        assert_eq!(image_height(0, 2), 18.0 + 21.0);
        let lines = wrap(EMPTY_DESCRIPTION, GRID_W);
        assert!(lines.iter().all(|l| Ui::text_width(l) <= GRID_W), "{lines:?}");
        assert_eq!(lines.join(" "), EMPTY_DESCRIPTION);
    }
}
