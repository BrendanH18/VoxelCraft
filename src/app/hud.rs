//! Heads-up display: crosshair, hotbar, item name popup and the F3 debug
//! screen.

use std::time::Instant;

use crate::render::ui::{Color, Ui, UiVertex, WHITE};
use crate::world::chunk::{chunk_of, local_of};

use super::Game;

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
        if !self.mouse_grabbed && self.screenshot.is_none() {
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
        for (i, &block) in self.hotbar.iter().enumerate() {
            let sx = x0 + 1.0 + i as f32 * slot;
            ui.rect(sx + 1.0, y0 + 2.0, slot - 2.0, slot - 2.0, [0.35, 0.35, 0.35, 0.5]);
            if i == self.selected {
                let (x, y, s) = (sx - 1.0, y0 - 1.0, slot + 2.0);
                for (rx, ry, rw, rh) in [(x, y, s, 2.0), (x, y + s, s, 2.0), (x, y, 2.0, s + 2.0), (x + s - 2.0, y, 2.0, s + 2.0)] {
                    ui.rect(rx, ry, rw, rh, WHITE);
                }
            }
            ui.block_icon(sx + 3.0, y0 + 3.0, slot - 4.0, block);
        }

        // Selected item name, fading out after two seconds.
        let age = (now - self.selection_changed).as_secs_f32();
        let alpha = ((2.5 - age) / 0.5).clamp(0.0, 1.0);
        if alpha > 0.0 {
            let name = capitalize(self.hotbar[self.selected].name());
            ui.text((sw - Ui::text_width(&name)) / 2.0, y0 - 14.0, &name, [1.0, 1.0, 1.0, alpha]);
        }
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
