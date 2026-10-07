//! The end screen shown the first time a player leaves the End through the
//! exit portal, like Java's win screen: a skippable scroll of credits, then
//! the player respawns. Java's End Poem is copyrighted, so the text here is
//! original. Which players have seen it is saved as `credits_seen`.

use super::Game;
use crate::audio::sounds::Sound;
use crate::render::ui::{Ui, WHITE};
use crate::world::terrain::Dimension;

/// How fast the text scrolls, in UI units per second.
const SPEED: f32 = 22.0;
/// Seconds before Escape, Space or a click may skip (so a held key from the
/// portal fall can't dismiss it).
const SKIP_DELAY: f32 = 0.75;
const LINE: f32 = 14.0;
const TITLE: [f32; 4] = [1.0, 1.0, 0.45, 1.0];
const DIM: [f32; 4] = [0.65, 0.65, 0.75, 1.0];

/// Lines of the scroll: a leading `#` marks a heading and a blank line a gap.
pub(super) const TEXT: &[&str] = &[
    "#VoxelCraft",
    "",
    "A voxel world written in Rust",
    "",
    "",
    "#Design and code",
    "Brendan Hallas",
    "",
    "#Built with",
    "Rust",
    "wgpu and winit",
    "glam, cpal and gilrs",
    "",
    "#Inspired by",
    "Minecraft, by Mojang Studios",
    "VoxelCraft is an unofficial fan project",
    "",
    "",
    "#The End",
    "",
    "The dragon is gone and the sky is quiet.",
    "You carried a pick, a bad idea and a lot of patience",
    "through the dark, the fire and the long fall,",
    "and the island held still for you.",
    "",
    "Nothing here was waiting to be won.",
    "Every block was something somebody placed.",
    "The caves you lit are still lit.",
    "The houses you left have their doors open.",
    "",
    "Go home. There are fields to plant,",
    "rivers to follow and mountains nobody has named.",
    "The world is as big as you decide to make it.",
    "",
    "",
    "Thank you for playing.",
];

impl Game {
    /// Starts the credits if this player hasn't seen them. Returns whether
    /// they did, in which case the caller must not leave the End yet.
    pub(super) fn begin_credits(&mut self) -> bool {
        if self.credits_seen {
            return false;
        }
        self.credits_seen = true;
        self.credits = Some(0.0);
        self.player.vel = glam::DVec3::ZERO;
        self.actions.reset();
        self.keys.clear();
        self.set_grab(false);
        self.audio.play(Sound::PortalSpawn, None, 0.4, (0.6, 0.6));
        true
    }

    /// Scrolls the credits; at the end (or when skipped) the player respawns.
    pub(super) fn update_credits(&mut self, dt: f32) {
        let Some(t) = &mut self.credits else { return };
        *t += dt;
        if *t * SPEED > total_height(self.ui_size().1 as f32 / self.ui_scale()) {
            self.finish_credits();
        }
    }

    pub(super) fn skip_credits(&mut self) {
        if self.credits.is_some_and(|t| t >= SKIP_DELAY) {
            self.finish_credits();
        }
    }

    fn finish_credits(&mut self) {
        self.credits = None;
        self.switch_dimension(Dimension::Overworld, super::dimension::Arrival::Respawn);
        self.set_grab(true);
    }

    pub(super) fn credits_ui(&self, ui: &mut Ui, t: f32) {
        let (sw, sh) = ui.size();
        ui.rect(0.0, 0.0, sw, sh, [0.0, 0.0, 0.0, 1.0]);
        draw(ui, t);
        let hint = "Press Escape to skip";
        ui.text(sw - Ui::text_width(hint) - 6.0, sh - 12.0, hint, DIM);
    }
}

/// Height of a line: headings are drawn twice as big.
fn line_height(line: &str) -> f32 {
    if line.starts_with('#') { LINE * 2.0 } else { LINE }
}

fn total_height(screen_h: f32) -> f32 {
    screen_h + TEXT.iter().map(|l| line_height(l)).sum::<f32>()
}

/// Draws the lines that are on screen `t` seconds into the scroll.
fn draw(ui: &mut Ui, t: f32) {
    let (sw, sh) = ui.size();
    let top = sh - t * SPEED;
    let mut offset = 0.0;
    for line in TEXT {
        let y = (top + offset).floor();
        offset += line_height(line);
        if line.is_empty() || y < -LINE || y > sh {
            continue;
        }
        let (text, scale, color) = match line.strip_prefix('#') {
            Some(h) => (h, 2.0, TITLE),
            None => (*line, 1.0, WHITE),
        };
        let x = ((sw - Ui::text_width(text) * scale) / 2.0).floor();
        ui.text_scaled(x, y, text, color, scale);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_original_and_scroll_finishes() {
        let all = TEXT.join(" ").to_lowercase();
        for phrase in ["i see the player you mean", "dreamed", "julian"] {
            assert!(!all.contains(phrase));
        }
        assert!(total_height(240.0) / SPEED > 10.0, "long enough to read");
        assert!(total_height(240.0) / SPEED < 120.0, "short enough to sit through");
    }

    #[test]
    fn lines_scroll_up_from_the_bottom() {
        let mut ui = Ui::with_scale(320.0, 240.0, 1.0);
        draw(&mut ui, 0.0);
        let empty = ui.verts.len();
        draw(&mut ui, 8.0);
        assert!(ui.verts.len() > empty, "text enters the screen over time");
    }
}
