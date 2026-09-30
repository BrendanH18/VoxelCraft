//! Heads-up display: crosshair, hotbar, item name popup and the F3 debug
//! screen.

use std::time::Instant;

use crate::inventory::{Stack, HOTBAR_SLOTS};
use crate::render::ui::{Color, Ui, UiVertex, WHITE};
use crate::world::block::Block;
use crate::world::chunk::{chunk_of, local_of};

use super::{Game, GameMode};

/// A clickable slot on the inventory screen.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum SlotRef {
    Inventory(usize),
    /// Creative palette entry.
    Palette(Block),
}

const SLOT: f32 = 18.0;
const PANEL_W: f32 = 9.0 * SLOT + 14.0;
const PANEL_H: f32 = 4.0 * SLOT + 38.0;

const DEBUG_TEXT: Color = [0.88, 0.88, 0.88, 1.0];
const HIGHLIGHT: Color = [1.0, 1.0, 0.6, 1.0];

impl Game {
    pub(super) fn build_ui(&self, now: Instant) -> Vec<UiVertex> {
        let (w, h) = self.renderer.size();
        let mut ui = Ui::new(w as f32, h as f32, self.renderer.scale_factor());
        let (sw, sh) = ui.size();

        if self.player.head_in_water(&self.world) {
            ui.rect(0.0, 0.0, sw, sh, [0.05, 0.15, 0.4, 0.3]);
        }

        // Crosshair.
        let (cx, cy) = ((sw / 2.0).floor(), (sh / 2.0).floor());
        ui.rect(cx - 5.0, cy - 0.5, 10.0, 1.0, [1.0, 1.0, 1.0, 0.85]);
        ui.rect(cx - 0.5, cy - 5.0, 1.0, 10.0, [1.0, 1.0, 1.0, 0.85]);

        self.hotbar_ui(&mut ui, now);
        if self.show_debug {
            self.debug_ui(&mut ui);
        }
        if self.inventory_open {
            self.inventory_ui(&mut ui);
        } else if !self.mouse_grabbed && self.screenshot.is_none() {
            ui.rect(0.0, 0.0, sw, sh, [0.0, 0.0, 0.0, 0.35]);
            let msg = "Click to play  -  Esc to save and quit";
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
            if i == self.selected {
                let (x, y, s) = (sx - 1.0, y0 - 1.0, slot + 2.0);
                for (rx, ry, rw, rh) in [(x, y, s, 2.0), (x, y + s, s, 2.0), (x, y, 2.0, s + 2.0), (x + s - 2.0, y, 2.0, s + 2.0)] {
                    ui.rect(rx, ry, rw, rh, WHITE);
                }
            }
            if let Some(stack) = self.inventory.get(i) {
                self.stack_ui(ui, sx + 1.0, y0 + 2.0, stack);
            }
        }

        // Popup message (item names, mode changes), fading out.
        let age = (now - self.popup.1).as_secs_f32();
        let alpha = ((2.5 - age) / 0.5).clamp(0.0, 1.0);
        if alpha > 0.0 {
            let text = capitalize(&self.popup.0);
            ui.text((sw - Ui::text_width(&text)) / 2.0, y0 - 14.0, &text, [1.0, 1.0, 1.0, alpha]);
        }
    }

    /// Item icon plus stack count in an 18x18 slot at (x, y).
    fn stack_ui(&self, ui: &mut Ui, x: f32, y: f32, stack: Stack) {
        ui.block_icon(x + 2.0, y + 2.0, 14.0, stack.block);
        if self.mode == GameMode::Survival && stack.count > 1 {
            let n = stack.count.to_string();
            ui.text(x + 17.0 - Ui::text_width(&n), y + 9.0, &n, WHITE);
        }
    }

    /// Screen positions of every slot on the inventory screen: a 3x9 grid
    /// (main inventory, or the block palette in creative) and the hotbar.
    fn inventory_slots(&self, screen: (f32, f32)) -> Vec<(SlotRef, f32, f32)> {
        let (px, py) = (((screen.0 - PANEL_W) / 2.0).floor(), ((screen.1 - PANEL_H) / 2.0).floor());
        let mut out = Vec::with_capacity(36);
        let grid = |i: usize| (px + 7.0 + (i % 9) as f32 * SLOT, py + 18.0 + (i / 9) as f32 * SLOT);
        match self.mode {
            GameMode::Survival => {
                for i in 0..27 {
                    let (x, y) = grid(i);
                    out.push((SlotRef::Inventory(HOTBAR_SLOTS + i), x, y));
                }
            }
            GameMode::Creative => {
                for (i, b) in Block::creative_palette().enumerate().take(27) {
                    let (x, y) = grid(i);
                    out.push((SlotRef::Palette(b), x, y));
                }
            }
        }
        for i in 0..HOTBAR_SLOTS {
            out.push((SlotRef::Inventory(i), px + 7.0 + i as f32 * SLOT, py + 18.0 + 3.0 * SLOT + 6.0));
        }
        out
    }

    pub(super) fn slot_under_cursor(&self) -> Option<SlotRef> {
        let scale = Ui::scale_for(self.renderer.scale_factor());
        let (w, h) = self.renderer.size();
        let (mx, my) = (self.cursor_px.0 / scale, self.cursor_px.1 / scale);
        self.inventory_slots((w as f32 / scale, h as f32 / scale))
            .into_iter()
            .find(|&(_, x, y)| mx >= x && mx < x + SLOT && my >= y && my < y + SLOT)
            .map(|(r, _, _)| r)
    }

    fn inventory_ui(&self, ui: &mut Ui) {
        let (sw, sh) = ui.size();
        ui.rect(0.0, 0.0, sw, sh, [0.0, 0.0, 0.0, 0.45]);
        let (px, py) = (((sw - PANEL_W) / 2.0).floor(), ((sh - PANEL_H) / 2.0).floor());
        ui.rect(px, py, PANEL_W, PANEL_H, [0.78, 0.78, 0.78, 1.0]);
        ui.rect(px, py, PANEL_W, 1.0, WHITE);
        ui.rect(px, py, 1.0, PANEL_H, WHITE);
        ui.rect(px, py + PANEL_H - 1.0, PANEL_W, 1.0, [0.33, 0.33, 0.33, 1.0]);
        ui.rect(px + PANEL_W - 1.0, py, 1.0, PANEL_H, [0.33, 0.33, 0.33, 1.0]);
        let title = match self.mode {
            GameMode::Survival => "Inventory",
            GameMode::Creative => "Creative - pick blocks",
        };
        ui.text_flat(px + 8.0, py + 6.0, title, [0.25, 0.25, 0.25, 1.0]);

        let hovered = self.slot_under_cursor();
        for (r, x, y) in self.inventory_slots((sw, sh)) {
            ui.rect(x, y, SLOT, SLOT, [0.55, 0.55, 0.55, 1.0]);
            ui.rect(x + 1.0, y + 1.0, SLOT - 1.0, SLOT - 1.0, [0.45, 0.45, 0.45, 1.0]);
            let stack = match r {
                SlotRef::Inventory(i) => self.inventory.get(i),
                SlotRef::Palette(b) => Some(Stack::new(b, 1)),
            };
            if let Some(stack) = stack {
                self.stack_ui(ui, x, y, stack);
            }
            if hovered == Some(r) {
                ui.rect(x + 1.0, y + 1.0, SLOT - 2.0, SLOT - 2.0, [1.0, 1.0, 1.0, 0.35]);
            }
        }
        if let Some(SlotRef::Inventory(i)) = hovered
            && let Some(s) = self.inventory.get(i)
        {
            self.tooltip(ui, s.block.name());
        } else if let Some(SlotRef::Palette(b)) = hovered {
            self.tooltip(ui, b.name());
        }
        // The held stack follows the mouse.
        if let Some(stack) = self.inventory.cursor {
            let scale = ui.scale;
            self.stack_ui(ui, self.cursor_px.0 / scale - 9.0, self.cursor_px.1 / scale - 9.0, stack);
        }
    }

    fn tooltip(&self, ui: &mut Ui, text: &str) {
        let text = capitalize(text);
        let (x, y) = (self.cursor_px.0 / ui.scale + 10.0, self.cursor_px.1 / ui.scale - 12.0);
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
            format!("{:.0} fps ({:.2} ms cpu){}", self.fps, self.cpu_ms, if self.renderer.vsync() { " vsync" } else { "" }),
            String::new(),
            format!("XYZ: {:.3} / {:.3} / {:.3}", p.x, p.y, p.z),
            format!("Block: {} {} {}", b.x, b.y, b.z),
            format!("Chunk: {} {} {} in {} {} {}", l.x, l.y, l.z, c.x, c.y, c.z),
            format!("Facing: {facing} ({:.1} / {:.1})", self.player.yaw.to_degrees(), self.player.pitch.to_degrees()),
            format!("Biome: {biome:?}"),
            format!("Game mode: {}", self.mode.name()),
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
            format!("Render distance: {} chunks ({} blocks)", self.world.render_distance(), self.world.render_distance() * 32),
            format!("Chunks: {} loaded, {} meshed, {} visible", self.world.loaded_chunks(), s.meshes, s.visible),
            format!("Draw calls: {}, quads: {:.2}M", s.draw_calls, s.quads as f64 / 1e6),
            format!("Vertex memory: {:.1} MB", s.gpu_bytes as f64 / 1e6),
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

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}
