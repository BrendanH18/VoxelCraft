//! Bounded block-change history read independently by each active navigator.
//! Overflow conservatively invalidates an active path; it never loses an edit
//! silently or grows with world age. Nothing here is serialized.
use std::collections::VecDeque;

use glam::IVec3;

#[derive(Default)]
pub(super) struct Updates {
    revision: u64,
    positions: VecDeque<IVec3>,
}

impl Updates {
    const CAPACITY: usize = 256;

    pub(super) fn record(&mut self, pos: IVec3) {
        self.revision += 1;
        if self.positions.len() == Self::CAPACITY {
            self.positions.pop_front();
        }
        self.positions.push_back(pos);
    }

    pub(super) fn visit(&self, since: u64, visit: &mut dyn FnMut(IVec3)) -> (u64, bool) {
        let count = self.revision.saturating_sub(since);
        let overflow = count > self.positions.len() as u64;
        if !overflow {
            for &pos in self.positions.iter().skip(self.positions.len() - count as usize) {
                visit(pos);
            }
        }
        (self.revision, overflow)
    }
}

impl super::World {
    pub(crate) fn navigation_updates(&self, since: u64, visit: &mut dyn FnMut(IVec3)) -> (u64, bool) {
        self.navigation_updates.visit(since, visit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readers_have_independent_cursors_and_overflow_is_explicit() {
        let mut updates = Updates::default();
        updates.record(IVec3::X);
        let mut seen = Vec::new();
        assert_eq!(updates.visit(0, &mut |p| seen.push(p)), (1, false));
        assert_eq!(seen, [IVec3::X]);
        assert_eq!(updates.visit(1, &mut |_| panic!("no new edits")), (1, false));
        for i in 0..300 {
            updates.record(IVec3::new(i, 0, 0));
        }
        assert_eq!(updates.positions.len(), Updates::CAPACITY);
        assert_eq!(updates.visit(0, &mut |_| panic!("overflow recomputes instead")), (301, true));
        assert_eq!(updates.visit(300, &mut |p| assert_eq!(p.x, 299)), (301, false));
    }
}
