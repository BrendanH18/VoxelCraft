//! v1.0 gap track 6: `--item-review DIR`, contact sheets for reviewing every
//! registered item's icon, dropped model and held model against the world.
//!
//! With `--screenshot`, once the scene has settled each item in
//! [`Item::all_items`] (or the `--item-review-range` part of it) is staged
//! for one frame: held in the first-person hand, lying at rest (the lowest
//! point of its bob, turned to show two sides) on the floor 1.6 blocks
//! ahead of the pose and a little left, and drawn in the corner as its
//! 16-pixel slot icon and the same icon three times as large, under its
//! list index and name. Each frame is read back and laid into a sheet of
//! [`COLUMNS`] x [`ROWS`] tiles, written as `items-NN.png` in DIR; the
//! screenshot is taken after the last sheet. Run it at the smallest
//! `--size`, 320x320 (1920x1600 sheets), with a floor under the stage.
use std::path::PathBuf;

use glam::DVec3;

use super::Game;
use crate::inventory::Stack;
use crate::item::Item;
use crate::render::ui::{Ui, WHITE};

/// Tiles across and down a sheet.
pub(super) const COLUMNS: u32 = 6;
pub(super) const ROWS: u32 = 5;
/// How far ahead of the pose and to its left the dropped item lies.
const AHEAD: f64 = 1.6;
const LEFT: f64 = 0.35;
/// The dropped model's turn: two sides and the top show.
const SPIN: f32 = 0.6;

pub(super) struct ItemReview {
    dir: PathBuf,
    /// Indices into `Item::all_items` and the items, in sheet order.
    items: Vec<(usize, Item)>,
    next: usize,
    /// The item staged for the frame being read back.
    staged: Option<(usize, Item)>,
    sheet: Vec<u8>,
    tile: (u32, u32),
    sheets: u32,
}

impl ItemReview {
    pub(super) fn new(dir: PathBuf, range: Option<(usize, usize)>) -> Self {
        let (from, to) = range.unwrap_or((0, usize::MAX));
        let items = Item::all_items().enumerate().filter(|&(i, _)| (from..=to).contains(&i)).collect();
        Self { dir, items, next: 0, staged: None, sheet: Vec::new(), tile: (0, 0), sheets: 0 }
    }

    /// Lays a read-back frame into the sheet, writing the sheet once full.
    fn paste(&mut self, w: u32, h: u32, pixels: &[u8]) {
        if self.sheet.is_empty() || self.tile != (w, h) {
            self.flush();
            self.tile = (w, h);
            self.sheet = vec![0; (w * COLUMNS * h * ROWS * 4) as usize];
        }
        let n = self.next as u32 % (COLUMNS * ROWS);
        let (tx, ty) = ((n % COLUMNS) * w, (n / COLUMNS) * h);
        let stride = (w * COLUMNS * 4) as usize;
        for row in 0..h {
            let from = (row * w * 4) as usize;
            let to = (ty + row) as usize * stride + (tx * 4) as usize;
            self.sheet[to..to + (w * 4) as usize].copy_from_slice(&pixels[from..from + (w * 4) as usize]);
        }
        self.next += 1;
        if (self.next as u32).is_multiple_of(COLUMNS * ROWS) {
            self.flush();
        }
    }

    /// Writes the sheet so far, if it has any tile.
    fn flush(&mut self) {
        if self.sheet.is_empty() {
            return;
        }
        let (w, h) = (self.tile.0 * COLUMNS, self.tile.1 * ROWS);
        let path = self.dir.join(format!("items-{:02}.png", self.sheets));
        let result = std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string()).and_then(|()| {
            let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
            let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header().and_then(|mut wr| wr.write_image_data(&self.sheet)).map_err(|e| e.to_string())
        });
        match result {
            Ok(()) => log::info!("item review sheet {}", path.display()),
            Err(e) => log::error!("--item-review: {}: {e}", path.display()),
        }
        self.sheet.clear();
        self.sheets += 1;
    }
}

impl Game {
    /// One step of `--item-review`: takes the last frame read back, then
    /// stages the next item and asks for its frame. Done when no item is
    /// left; the screenshot follows.
    pub(super) fn step_item_review(&mut self) {
        let Some(review) = self.item_review.as_mut() else { return };
        if let Some((w, h, pixels)) = self.renderer.take_readback() {
            review.paste(w, h, &pixels);
        }
        if self.renderer.capture_pending() {
            return;
        }
        let Some(&(index, item)) = review.items.get(review.next) else {
            review.flush();
            self.item_review = None;
            return;
        };
        review.staged = Some((index, item));
        let slot = self.body.actions.selected;
        self.session.inventory.slots[slot] = Some(Stack::new(item, 1));
        self.hand.show_now(Some(item));
        self.renderer.request_readback();
    }

    /// The staged item's dropped stack, where it lies and its turn.
    pub(super) fn item_review_drop(&self) -> Option<(Stack, DVec3, f32)> {
        let (_, item) = self.item_review.as_ref()?.staged?;
        let p = &self.session.player;
        let forward = p.forward().as_dvec3().with_y(0.0).normalize_or_zero();
        let left = DVec3::new(forward.z, 0.0, -forward.x);
        Some((Stack::new(item, 1), p.pos + forward * AHEAD + left * LEFT, SPIN))
    }

    /// The tile's label and icons; returns whether the review is drawing
    /// (the rest of the HUD stays hidden).
    pub(super) fn item_review_ui(&self, ui: &mut Ui) -> bool {
        let Some((index, item)) = self.item_review.as_ref().and_then(|r| r.staged) else { return false };
        let stack = Stack::new(item, 1);
        let label = format!("{index} {}", super::capitalize(item.name()));
        let w = Ui::text_width(&label);
        ui.rect(0.0, 0.0, w + 4.0, 11.0, [0.0, 0.0, 0.0, 0.6]);
        ui.text(2.0, 2.0, &label, WHITE);
        let dial = self.dial_of(&self.session.player);
        super::gui::style::slot(ui, 2.0, 14.0);
        super::hud::draw_stack(ui, 2.0, 14.0, stack, false, dial);
        super::hud::draw_model_sized(ui, 2.0, 34.0, 48.0, stack, dial);
        true
    }
}
