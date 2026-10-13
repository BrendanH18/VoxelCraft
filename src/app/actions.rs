//! Hotbar selection and action progress, independent of the window and renderer.

use glam::IVec3;

#[derive(Default)]
pub(super) struct Actions {
    pub selected: usize,
    /// Target block and progress toward breaking it (0..1).
    pub breaking: Option<(IVec3, f32)>,
    /// Presentation mirror of the session's normalized consumption progress.
    pub eat_timer: f64,
    /// Seconds the bow has been drawn, while drawing.
    pub bow_draw: Option<f64>,
}

impl Actions {
    /// Selecting a different slot interrupts both actions, even if its
    /// item is the same kind as the previous one.
    pub fn select(&mut self, slot: usize) -> bool {
        if self.selected == slot {
            return false;
        }
        self.selected = slot;
        self.reset();
        true
    }

    pub fn reset(&mut self) {
        self.breaking = None;
        self.eat_timer = 0.0;
        self.bow_draw = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reselecting_the_same_slot_keeps_progress() {
        let mut actions = Actions { breaking: Some((IVec3::ZERO, 0.1)), eat_timer: 0.1, ..Default::default() };
        let before = (actions.breaking, actions.eat_timer);
        assert!(!actions.select(0));
        assert_eq!((actions.breaking, actions.eat_timer), before);
        actions.reset();
        assert_eq!((actions.breaking, actions.eat_timer), (None, 0.0));
    }
}
