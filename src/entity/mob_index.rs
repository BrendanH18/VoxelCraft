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

/// Pairwise separation of overlapping mobs on a grid sized to the widest mob,
/// so each mob only meets the few mobs whose boxes can touch it; buffers are
/// reused between ticks.
#[derive(Default)]
pub(super) struct Separation {
    heads: FxHashMap<(i32, i32), u32>,
    next: Vec<u32>,
    parts: Vec<Part>,
    push: Vec<DVec3>,
}

#[derive(Clone, Copy)]
struct Part {
    pos: DVec3,
    half_width: f64,
    height: f64,
    /// Takes part in separation (not a sleeping or inactive villager).
    active: bool,
    /// Present in the grid (alive, and not an inactive villager), like [`MobIndex`].
    indexed: bool,
}

const NONE: u32 = u32::MAX;

impl Separation {
    /// Adds each overlapping pair's push to the mobs' velocities.
    pub fn run(&mut self, mobs: &mut [Mob], dt: f64) {
        self.parts.clear();
        self.heads.clear();
        self.next.clear();
        self.push.clear();
        let mut widest = 0.0f64;
        for m in mobs.iter() {
            let shape = m.shape();
            let asleep = m.villager.as_ref().is_some_and(|v| !v.active || v.sleeping);
            let indexed = m.alive() && m.villager.as_ref().is_none_or(|v| v.active);
            widest = widest.max(shape.half_width);
            self.parts.push(Part {
                pos: m.pos,
                half_width: shape.half_width,
                height: shape.height,
                active: !asleep,
                indexed,
            });
        }
        self.next.resize(mobs.len(), NONE);
        self.push.resize(mobs.len(), DVec3::ZERO);
        // Two boxes touch only within the sum of their half-widths.
        let cell = (widest * 2.0).max(1.0);
        let key = |p: DVec3| ((p.x / cell).floor() as i32, (p.z / cell).floor() as i32);
        for i in 0..mobs.len() {
            let part = self.parts[i];
            if part.indexed && part.active {
                self.next[i] = self.heads.insert(key(part.pos), i as u32).unwrap_or(NONE);
            }
        }
        for i in 0..mobs.len() {
            let a = self.parts[i];
            if !a.active {
                continue;
            }
            let (cx, cz) = key(a.pos);
            for dz in -1..=1 {
                for dx in -1..=1 {
                    let mut j = self.heads.get(&(cx + dx, cz + dz)).copied().unwrap_or(NONE);
                    while j != NONE {
                        let b = self.parts[j as usize];
                        let at = j as usize;
                        j = self.next[at];
                        if at <= i {
                            continue;
                        }
                        let d = DVec3::new(b.pos.x - a.pos.x, 0.0, b.pos.z - a.pos.z);
                        let min = a.half_width + b.half_width;
                        let overlap_y = a.pos.y < b.pos.y + b.height && b.pos.y < a.pos.y + a.height;
                        let dist = d.length();
                        if !overlap_y || dist >= min {
                            continue;
                        }
                        let dir = if dist > 1e-4 { d / dist } else { DVec3::X };
                        let push = dir * ((min - dist) * 8.0 * dt).min(0.2) / dt.max(1e-3);
                        self.push[i] -= push * 0.5;
                        self.push[at] += push * 0.5;
                    }
                }
            }
        }
        for (m, push) in mobs.iter_mut().zip(&self.push) {
            m.vel += *push;
        }
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

    /// The grid finds exactly the pairs an all-pairs scan pushes, including the
    /// widest mobs and mobs straddling cell borders or negative coordinates.
    #[test]
    fn separation_matches_an_all_pairs_scan() {
        let kinds = [MobKind::Zombie, MobKind::Pig, MobKind::Ghast, MobKind::Chicken, MobKind::Hoglin, MobKind::Spider];
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut mobs: Vec<Mob> = (0..120)
            .map(|i| {
                let pos = DVec3::new(next() * 14.0 - 7.0, 1.0 + (i % 3) as f64 * 0.8, next() * 14.0 - 7.0);
                Mob::new(kinds[i % kinds.len()], pos, 0.0)
            })
            .collect();
        mobs[7].dying = Some(0.0);
        let dt = 0.05;
        let mut expected = vec![DVec3::ZERO; mobs.len()];
        for i in 0..mobs.len() {
            for j in i + 1..mobs.len() {
                if !mobs[j].alive() {
                    continue;
                }
                let (a, b) = (&mobs[i], &mobs[j]);
                let d = DVec3::new(b.pos.x - a.pos.x, 0.0, b.pos.z - a.pos.z);
                let min = a.shape().half_width + b.shape().half_width;
                let overlap_y = a.pos.y < b.pos.y + b.shape().height && b.pos.y < a.pos.y + a.shape().height;
                let dist = d.length();
                if !overlap_y || dist >= min {
                    continue;
                }
                let dir = if dist > 1e-4 { d / dist } else { DVec3::X };
                let push = dir * ((min - dist) * 8.0 * dt).min(0.2) / dt;
                expected[i] -= push * 0.5;
                expected[j] += push * 0.5;
            }
        }
        assert!(expected.iter().any(|v| *v != DVec3::ZERO));
        Separation::default().run(&mut mobs, dt);
        for (m, want) in mobs.iter().zip(&expected) {
            assert!((m.vel - *want).length() < 1e-9, "{:?} {:?} != {:?}", m.kind, m.vel, want);
        }
    }
}
