//! Per-player Java gossip weights, caps, daily decay and village sharing.
use super::{Entities, MobKind, PlayerId};
use rustc_hash::FxHashMap;
use serde_json::{Value, json};

#[derive(Clone, Debug, Default)]
pub(super) struct Gossip {
    pub entries: FxHashMap<PlayerId, [u8; 5]>,
    pub day: i64,
}
impl Gossip {
    pub fn add(&mut self, player: PlayerId, kind: usize, amount: u8) {
        let values = self.entries.entry(player).or_default();
        values[kind] = values[kind].saturating_add(amount).min([100, 200, 20, 200, 25][kind]);
    }
    pub fn reputation(&self, player: PlayerId) -> i16 {
        self.entries
            .get(&player)
            .map_or(0, |v| -5 * v[0] as i16 - v[1] as i16 + 5 * v[2] as i16 + v[3] as i16 + v[4] as i16)
    }
    pub fn decay(&mut self, day: i64) {
        let days = day.saturating_sub(self.day).clamp(0, 255) as u16;
        self.day = day;
        if days == 0 {
            return;
        }
        self.entries.retain(|_, values| {
            for (v, d) in values.iter_mut().zip([10u16, 20, 0, 1, 2]) {
                *v = (*v as u16).saturating_sub(d * days) as u8;
                if *v < 2 {
                    *v = 0;
                }
            }
            values.iter().any(|&v| v > 0)
        });
    }
    pub fn share_from(&mut self, other: &Self) {
        for (&player, values) in &other.entries {
            for (i, (&v, d)) in values.iter().zip([10, 20, 20, 5, 20]).enumerate() {
                let shared = v.saturating_sub(d);
                if shared >= 2 {
                    let values = self.entries.entry(player).or_default();
                    values[i] = values[i].max(shared);
                }
            }
        }
    }
    pub fn save(&self) -> Value {
        json!({"day":self.day,"entries":self.entries.iter().map(|(p,v)|json!([p.0,v])).collect::<Vec<_>>()})
    }
    pub fn load(v: &Value) -> Self {
        let mut out = Self { day: v["day"].as_i64().unwrap_or(0), ..Self::default() };
        if let Some(entries) = v["entries"].as_array() {
            for entry in entries {
                let Some(player) = entry[0].as_u64().and_then(|p| u32::try_from(p).ok()) else {
                    continue;
                };
                if let Some(values) = entry[1].as_array() {
                    for (i, v) in values.iter().take(5).enumerate() {
                        if let Some(v) = v.as_u64() {
                            out.add(PlayerId(player), i, v.min(255) as u8);
                        }
                    }
                }
            }
        }
        out
    }
}
impl Entities {
    pub fn note_villager_hurt(&mut self, index: usize, player: PlayerId, killed: bool) {
        let Some(m) = self.mobs.get(index) else {
            return;
        };
        if m.kind != MobKind::Villager {
            return;
        }
        let pos = m.pos;
        if let Some(v) = &mut self.mobs[index].villager {
            v.gossip.add(player, 1, 25);
        }
        if killed {
            self.mob_index.rebuild(&self.mobs);
            self.mob_index.visit(pos, 16.0, |j| {
                if self.mobs[j].kind == MobKind::Villager
                    && self.mobs[j].pos.distance_squared(pos) <= 16.0 * 16.0
                    && let Some(v) = &mut self.mobs[j].villager
                {
                    v.gossip.add(player, 0, 25);
                }
            });
        }
    }
    pub(super) fn share_gossip(&mut self) {
        self.mob_index.rebuild(&self.mobs);
        for i in 0..self.mobs.len() {
            if self.mobs[i].kind != MobKind::Villager || !self.mobs[i].alive() {
                continue;
            }
            let Some(j) = self.mob_index.nearest(&self.mobs, self.mobs[i].pos, 3.0, |m| {
                m.kind == MobKind::Villager
                    && m.villager.as_ref().is_some_and(|v| v.id != self.mobs[i].villager.as_ref().unwrap().id)
            }) else {
                continue;
            };
            if j <= i {
                continue;
            }
            let (before, after) = self.mobs.split_at_mut(j);
            let a = &mut before[i].villager.as_mut().unwrap().gossip;
            let b = &mut after[0].villager.as_mut().unwrap().gossip;
            a.share_from(b);
            b.share_from(a);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn witnessed_kills_do_not_leak_outside_the_sensor_radius() {
        let mut e = Entities::new(3);
        for x in [0.0, 5.0, 17.0] {
            e.spawn(MobKind::Villager, glam::DVec3::X * x);
        }
        e.note_villager_hurt(0, PlayerId::HOST, true);
        assert_eq!(e.mobs[1].villager.as_ref().unwrap().gossip.reputation(PlayerId::HOST), -125);
        assert_eq!(e.mobs[2].villager.as_ref().unwrap().gossip.reputation(PlayerId::HOST), 0);
    }
    #[test]
    fn cures_are_per_player_caps_decay_and_roundtrip() {
        let mut g = Gossip::default();
        let p = PlayerId(7);
        g.add(p, 2, 20);
        g.add(p, 3, 25);
        assert_eq!(g.reputation(p), 125);
        assert_eq!(g.reputation(PlayerId::HOST), 0);
        g.add(p, 2, 20);
        assert_eq!(g.reputation(p), 125);
        g.decay(25);
        assert_eq!(g.reputation(p), 100);
        let copy = Gossip::load(&g.save());
        assert_eq!(copy.reputation(p), 100);
        let mut other = Gossip::default();
        other.share_from(&g);
        assert_eq!(other.reputation(p), 0, "major positive never transfers");
        g.add(p, 1, 25);
        other.share_from(&g);
        assert_eq!(other.reputation(p), -5);
    }
}
