//! Inventory search input shared by creative filtering and survival/container highlighting.
use super::{Container, Game, GameMode};
use crate::render::ui::{Ui, WHITE};
use winit::event::KeyEvent;
use winit::keyboard::{KeyCode, PhysicalKey};

pub(super) const HEIGHT: f32 = 20.0;
#[derive(Default)]
pub(super) struct Search {
    pub query: String,
    pub focused: bool,
    pub selected: bool,
}
impl Game {
    pub(super) fn search_key(&mut self, event: &KeyEvent) -> bool {
        if !self.inventory_open {
            return false;
        }
        let PhysicalKey::Code(code) = event.physical_key else {
            return self.search.focused;
        };
        if code == KeyCode::KeyF && self.modifiers.control_key() {
            self.search.focused = true;
            self.search.selected = true;
            return true;
        }
        if !self.search.focused {
            return false;
        }
        match code {
            KeyCode::Escape => {
                self.search.focused = false;
                self.search.selected = false;
                self.toggle_inventory();
            }
            KeyCode::Enter | KeyCode::NumpadEnter => {
                self.search.focused = false;
                self.search.selected = false;
            }
            KeyCode::KeyA if self.modifiers.control_key() => self.search.selected = true,
            KeyCode::Backspace | KeyCode::Delete => {
                if self.search.selected {
                    self.search.query.clear();
                    self.search.selected = false;
                } else {
                    self.search.query.pop();
                }
                self.creative_scroll = 0;
            }
            _ => {
                if !self.modifiers.control_key()
                    && !self.modifiers.super_key()
                    && let Some(text) = &event.text
                {
                    if text.chars().any(|c| !c.is_control()) && self.search.selected {
                        self.search.query.clear();
                        self.search.selected = false;
                    }
                    for c in text.chars().filter(|c| !c.is_control()) {
                        if self.search.query.len() + c.len_utf8() <= 64 {
                            self.search.query.push(c);
                        }
                    }
                    self.creative_scroll = 0;
                }
            }
        }
        true
    }
    pub(super) fn search_click(&mut self) -> bool {
        let scale = Ui::scale_for(self.renderer.scale_factor());
        let (w, h) = self.ui_size();
        let (x, y, width) = self.search_bounds((w as f32 / scale, h as f32 / scale));
        let (mx, my) = (self.cursor_px.0 / scale, self.cursor_px.1 / scale);
        let inside = mx >= x && mx < x + width && my >= y && my < y + 14.0;
        self.search.focused = inside;
        self.search.selected = false;
        inside
    }
    pub(super) fn search_ui(&self, ui: &mut Ui) {
        let (x, y, w) = self.search_bounds(ui.size());
        ui.rect(x, y, w, 14.0, if self.search.focused { [0.08, 0.12, 0.17, 1.0] } else { [0.23, 0.23, 0.23, 1.0] });
        if self.search.selected {
            ui.rect(x + 2.0, y + 2.0, (Ui::text_width(&self.search.query)).min(w - 4.0), 10.0, [0.15, 0.3, 0.6, 1.0]);
        }
        let room = w - 6.0;
        let text = if self.search.query.is_empty() && !self.search.focused {
            let hints: &[&str] = if self.mode == GameMode::Creative && self.container == Container::Inventory {
                &["Search items..."]
            } else {
                &["Find items (Ctrl+F)...", "Find items..."]
            };
            // The longest hint that fits, never a clipped one.
            hints.iter().find(|h| Ui::text_width(h) <= room).unwrap_or(&hints[hints.len() - 1]).to_string()
        } else {
            fit_tail(&format!("{}{}", self.search.query, if self.search.focused { "_" } else { "" }), room)
        };
        ui.text_flat(x + 3.0, y + 3.0, &text, WHITE);
    }
}

/// The end of `text` that fits in `width` UI pixels, so the cursor stays
/// visible while typing a long query.
fn fit_tail(text: &str, width: f32) -> String {
    let chars: Vec<char> = text.chars().collect();
    (0..chars.len())
        .map(|start| chars[start..].iter().collect::<String>())
        .find(|tail| Ui::text_width(tail) <= width)
        .unwrap_or_default()
}

#[cfg(test)]
mod fit_tests {
    use super::*;

    #[test]
    fn long_queries_keep_their_end_and_short_ones_stay_whole() {
        assert_eq!(fit_tail("stone_", 200.0), "stone_");
        let tail = fit_tail("a very long search for diamond pickaxes_", 60.0);
        assert!(tail.ends_with("pickaxes_") || tail.ends_with("es_"), "{tail}");
        assert!(Ui::text_width(&tail) <= 60.0);
        assert!(Ui::text_width(&fit_tail("x", 0.0)) <= 0.0);
    }
}
