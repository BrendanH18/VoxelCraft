//! Read-only recipe navigation and layout shared by drawing and hit testing.

pub(super) const WIDTH: f32 = 144.0;
pub(super) const GAP: f32 = 6.0;

#[derive(Default)]
pub(super) struct RecipeBook {
    pub open: bool,
    pub selected: usize,
}

impl RecipeBook {
    pub fn step(&mut self, direction: i32) {
        let count = crate::crafting::recipes().len() as i32;
        self.selected = (self.selected as i32 + direction).rem_euclid(count) as usize;
    }
}

#[derive(Clone, Copy)]
pub(super) struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn contains(self, (x, y): (f32, f32)) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Control {
    Toggle,
    Previous,
    Next,
}

pub(super) fn fits_beside(screen_width: f32, panel_width: f32) -> bool {
    screen_width >= panel_width + GAP + WIDTH + 12.0
}

pub(super) struct Layout {
    pub bounds: Rect,
    pub overlay: bool,
    pub toggle: Rect,
    pub previous: Rect,
    pub next: Rect,
}

impl Layout {
    pub fn new(screen_width: f32, panel: Rect) -> Self {
        let overlay = !fits_beside(screen_width, panel.w);
        let bounds = if overlay { panel } else { Rect { x: panel.x + panel.w + GAP, w: WIDTH, ..panel } };
        Self {
            bounds,
            overlay,
            toggle: Rect { x: panel.x + panel.w - 53.0, y: panel.y + 4.0, w: 46.0, h: 12.0 },
            previous: Rect { x: bounds.x + 10.0, y: bounds.y + 20.0, w: 18.0, h: 12.0 },
            next: Rect { x: bounds.x + bounds.w - 28.0, y: bounds.y + 20.0, w: 18.0, h: 12.0 },
        }
    }

    pub fn control(&self, mouse: (f32, f32), open: bool) -> Option<Control> {
        if self.toggle.contains(mouse) {
            Some(Control::Toggle)
        } else if open && self.previous.contains(mouse) {
            Some(Control::Previous)
        } else if open && self.next.contains(mouse) {
            Some(Control::Next)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_wraps_through_every_recipe() {
        let mut book = RecipeBook::default();
        book.step(-1);
        assert_eq!(book.selected, crate::crafting::recipes().len() - 1);
        book.step(1);
        assert_eq!(book.selected, 0);
    }

    #[test]
    fn recipe_controls_and_overlay_follow_window_width() {
        let panel = Rect { x: 40.0, y: 20.0, w: 176.0, h: 178.0 };
        for width in [260.0, 400.0] {
            let layout = Layout::new(width, panel);
            assert_eq!(layout.overlay, width == 260.0);
            if layout.overlay {
                assert_eq!(layout.bounds.x, panel.x);
            } else {
                assert!(layout.bounds.x > panel.x + panel.w);
            }
            let center = |r: Rect| (r.x + r.w / 2.0, r.y + r.h / 2.0);
            assert_eq!(layout.control(center(layout.toggle), false), Some(Control::Toggle));
            assert_eq!(layout.control(center(layout.previous), true), Some(Control::Previous));
            assert_eq!(layout.control(center(layout.next), true), Some(Control::Next));
            assert_eq!(layout.control(center(layout.next), false), None);
        }
    }
}
