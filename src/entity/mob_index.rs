//! Reused spatial buckets for local mob sensors and collision broad phase.
use super::Mob;
use glam::{DVec3, IVec3};
use rustc_hash::FxHashMap;

#[derive(Default)]
pub(super) struct MobIndex {
    heads: FxHashMap<IVec3, usize>,
    next: Vec<usize>,
}
impl MobIndex {
    pub fn rebuild(&mut self, mobs: &[Mob]) {
        self.heads.clear();
        self.next.clear();
        self.next.resize(mobs.len(), usize::MAX);
        for (i, m) in mobs.iter().enumerate() {
            if m.alive() && m.villager.as_ref().is_none_or(|v| v.active) {
                let cell = (m.pos / 8.0).floor().as_ivec3();
                self.next[i] = self.heads.insert(cell, i).unwrap_or(usize::MAX);
            }
        }
    }
    pub fn visit(&self, pos: DVec3, radius: f64, mut visit: impl FnMut(usize)) {
        let lo = ((pos - radius) / 8.0).floor().as_ivec3();
        let hi = ((pos + radius) / 8.0).floor().as_ivec3();
        for y in lo.y..=hi.y {
            for z in lo.z..=hi.z {
                for x in lo.x..=hi.x {
                    let mut i = self.heads.get(&IVec3::new(x, y, z)).copied().unwrap_or(usize::MAX);
                    while i != usize::MAX {
                        visit(i);
                        i = self.next[i];
                    }
                }
            }
        }
    }
    pub fn nearest(&self, mobs: &[Mob], pos: DVec3, radius: f64, predicate: impl Fn(&Mob) -> bool) -> Option<usize> {
        let mut best = radius * radius;
        let mut result = None;
        self.visit(pos, radius, |i| {
            let m = &mobs[i];
            let d = m.pos.distance_squared(pos);
            if m.alive() && predicate(m) && (d < best || d == best && result.is_some_and(|j| i < j)) {
                best = d;
                result = Some(i);
            }
        });
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::MobKind;
    #[test]
    fn buckets_handle_negative_cells_and_removal() {
        let mut mobs = vec![
            Mob::new(MobKind::Zombie, DVec3::new(-8.1, 1.0, -0.1), 0.0),
            Mob::new(MobKind::Zombie, DVec3::new(-7.9, 1.0, 0.1), 0.0),
            Mob::new(MobKind::Zombie, DVec3::new(100.0, 1.0, 0.1), 0.0),
        ];
        let mut index = MobIndex::default();
        index.rebuild(&mobs);
        assert_eq!(index.nearest(&mobs, DVec3::new(-8.05, 1.0, 0.0), 1.0, |_| true), Some(0));
        mobs.swap_remove(0);
        index.rebuild(&mobs);
        assert_eq!(index.nearest(&mobs, DVec3::new(-8.05, 1.0, 0.0), 1.0, |_| true), Some(1));
        mobs[1].dying = Some(0.0);
        index.rebuild(&mobs);
        assert_eq!(index.nearest(&mobs, DVec3::new(-8.05, 1.0, 0.0), 1.0, |_| true), None);
    }
}
