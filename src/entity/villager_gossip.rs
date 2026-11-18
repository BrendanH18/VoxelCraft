//! Per-player gossip after Java 26.3's `GossipContainer` and `GossipType`:
//! weights, caps, daily decay and the weighted sharing between villagers.
use super::{Entities, MobKind, PlayerId};
use rustc_hash::FxHashMap;
use serde_json::{Value, json};

/// Gossip types in their saved order (not Java's enum order).
pub const MAJOR_NEGATIVE: usize = 0;
pub const MINOR_NEGATIVE: usize = 1;
pub const MAJOR_POSITIVE: usize = 2;
pub const MINOR_POSITIVE: usize = 3;
pub const TRADING: usize = 4;

/// `GossipType`: (id, weight, max, decay per day, decay per transfer), in
/// the saved order above.
pub const TYPES: [(&str, i16, u8, u8, u8); 5] = [
    ("major_negative", -5, 100, 10, 10),
    ("minor_negative", -1, 200, 20, 20),
    ("major_positive", 5, 20, 0, 20),
    ("minor_positive", 1, 25, 1, 5),
    ("trading", 1, 25, 2, 20),
];
/// `GossipContainer.DISCARD_THRESHOLD`: values below it are forgotten.
const DISCARD: u8 = 2;
/// `Villager.GOSSIP_DECAY_INTERVAL` (ticks).
pub const DECAY_INTERVAL: i64 = 24000;
/// `Villager.GOSSIP_COOLDOWN` (ticks) between two gossips of a villager.
pub const GOSSIP_COOLDOWN: i64 = 1200;
/// `Villager.MAX_GOSSIP_TOPICS`: weighted picks per gossip.
pub const MAX_TOPICS: usize = 10;
/// `TradeWithVillager.tick`: villagers gossip within `sqrt(5)` blocks.
const GOSSIP_DISTANCE_SQ: f64 = 5.0;

#[derive(Clone, Debug, Default)]
pub(super) struct Gossip {
    pub entries: FxHashMap<PlayerId, [u8; 5]>,
    /// Java `lastGossipDecayTime` (absolute ticks; 0 until first seen).
    pub last_decay: i64,
}
impl Gossip {
    /// `GossipContainer.add`: a sum over the cap keeps the larger of the
    /// cap and the old value; values below two are dropped.
    pub fn add(&mut self, player: PlayerId, kind: usize, amount: u8) {
        let max = TYPES[kind].2;
        let values = self.entries.entry(player).or_default();
        let old = values[kind];
        let sum = old as u16 + amount as u16;
        let v = if sum > max as u16 { max.max(old) } else { sum as u8 };
        values[kind] = if v < DISCARD { 0 } else { v.min(max) };
        if values.iter().all(|&v| v == 0) {
            self.entries.remove(&player);
        }
    }
    /// `getReputation`: each value times its type's weight.
    pub fn reputation(&self, player: PlayerId) -> i16 {
        self.entries.get(&player).map_or(0, |v| v.iter().zip(TYPES).map(|(&v, t)| v as i16 * t.1).sum())
    }
    /// `Villager.maybeDecayGossip`: one decay once 24000 ticks have passed
    /// since the last; ticks a villager spends unloaded don't add more.
    pub fn maybe_decay(&mut self, now: i64) {
        if self.last_decay == 0 {
            self.last_decay = now;
        } else if now >= self.last_decay + DECAY_INTERVAL {
            self.decay();
            self.last_decay = now;
        }
    }
    /// `GossipContainer.decay`.
    pub fn decay(&mut self) {
        self.entries.retain(|_, values| {
            for (v, t) in values.iter_mut().zip(TYPES) {
                *v = v.saturating_sub(t.3);
                if *v < DISCARD {
                    *v = 0;
                }
            }
            values.iter().any(|&v| v > 0)
        });
    }
    /// `GossipContainer.transferFrom`: up to `max` picks from `source`,
    /// weighted by `|value * weight|` (repeats collapse); each pick arrives
    /// less its type's transfer decay, if still two or more, and keeps the
    /// larger of the old and new values. Returns the distinct picks.
    pub fn transfer_from(&mut self, source: &Self, rng: &mut super::Rng, max: usize) -> usize {
        // Entries in a stable order: player, then type.
        let mut players: Vec<PlayerId> = source.entries.keys().copied().collect();
        players.sort_unstable_by_key(|p| p.0);
        let mut entries: Vec<(PlayerId, usize, u8, u32)> = Vec::new();
        let mut total = 0u32;
        for p in players {
            for (kind, &v) in source.entries[&p].iter().enumerate() {
                if v > 0 {
                    total += (v as i32 * TYPES[kind].1 as i32).unsigned_abs();
                    entries.push((p, kind, v, total));
                }
            }
        }
        if total == 0 {
            return 0;
        }
        let mut picked = [false; 64];
        let mut chosen = 0;
        for _ in 0..max {
            let roll = rng.next_int(total);
            let i = entries.iter().position(|e| roll < e.3).unwrap();
            if i < picked.len() && !picked[i] {
                picked[i] = true;
                chosen += 1;
            }
        }
        for (i, &(player, kind, v, _)) in entries.iter().enumerate() {
            if i < picked.len() && picked[i] {
                let shared = v.saturating_sub(TYPES[kind].4);
                if shared >= DISCARD {
                    let values = self.entries.entry(player).or_default();
                    values[kind] = values[kind].max(shared);
                }
            }
        }
        chosen
    }
    pub fn save(&self) -> Value {
        json!({"decay":self.last_decay,"entries":self.entries.iter().map(|(p,v)|json!([p.0,v])).collect::<Vec<_>>()})
    }
    pub fn load(v: &Value) -> Self {
        // Saves before v0.15 Track H kept the day of the last decay.
        let last_decay = v["decay"].as_i64().unwrap_or_else(|| v["day"].as_i64().unwrap_or(0).saturating_mul(24000));
        let mut out = Self { last_decay, ..Self::default() };
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
            v.gossip.add(player, MINOR_NEGATIVE, 25);
        }
        if killed {
            self.mob_index.rebuild(&self.mobs);
            self.mob_index.visit(pos, 16.0, |j| {
                if self.mobs[j].kind == MobKind::Villager
                    && self.mobs[j].pos.distance_squared(pos) <= 16.0 * 16.0
                    && let Some(v) = &mut self.mobs[j].villager
                {
                    v.gossip.add(player, MAJOR_NEGATIVE, 25);
                }
            });
        }
    }
    /// `TradeWithVillager` and `Villager.gossip`: a villager next to another
    /// (within `sqrt(5)` blocks) hears its gossip when neither has gossiped
    /// in the last 1200 ticks; both then wait. Returns the villagers that
    /// gossiped, for the golem check `Villager.gossip` runs after.
    pub(super) fn share_gossip(&mut self, now: i64, gossiped: &mut Vec<usize>) {
        gossiped.clear();
        self.mob_index.rebuild(&self.mobs);
        let ready = |t: Option<i64>| t.is_none_or(|t| now < t || now >= t + GOSSIP_COOLDOWN);
        for i in 0..self.mobs.len() {
            let me = &self.mobs[i];
            if me.kind != MobKind::Villager
                || !me.alive()
                || !me.villager.as_ref().is_some_and(|v| ready(v.last_gossip))
            {
                continue;
            }
            let id = me.villager.as_ref().unwrap().id;
            let Some(j) = self.mob_index.nearest(&self.mobs, me.pos, GOSSIP_DISTANCE_SQ.sqrt(), |m| {
                m.kind == MobKind::Villager
                    && m.alive()
                    && m.villager.as_ref().is_some_and(|v| v.id != id && ready(v.last_gossip))
            }) else {
                continue;
            };
            if self.mobs[j].pos.distance_squared(self.mobs[i].pos) > GOSSIP_DISTANCE_SQ {
                continue;
            }
            let source = self.mobs[j].villager.as_ref().unwrap().gossip.clone();
            let v = self.mobs[i].villager.as_mut().unwrap();
            v.gossip.transfer_from(&source, &mut self.rng, MAX_TOPICS);
            v.last_gossip = Some(now);
            self.mobs[j].villager.as_mut().unwrap().last_gossip = Some(now);
            gossiped.push(i);
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
        g.add(p, MAJOR_POSITIVE, 20);
        g.add(p, MINOR_POSITIVE, 25);
        assert_eq!(g.reputation(p), 125);
        assert_eq!(g.reputation(PlayerId::HOST), 0);
        g.add(p, MAJOR_POSITIVE, 20);
        assert_eq!(g.reputation(p), 125);
        // Minor positive is capped at Java's 25, not 200.
        g.add(p, MINOR_POSITIVE, 25);
        assert_eq!(g.reputation(p), 125);
        for _ in 0..25 {
            g.decay();
        }
        assert_eq!(g.reputation(p), 100);
        let copy = Gossip::load(&g.save());
        assert_eq!(copy.reputation(p), 100);
        let mut rng = crate::entity::Rng::new(3);
        let mut other = Gossip::default();
        other.transfer_from(&g, &mut rng, MAX_TOPICS);
        assert_eq!(other.reputation(p), 0, "major positive never transfers");
        g.add(p, MINOR_NEGATIVE, 25);
        other.transfer_from(&g, &mut rng, MAX_TOPICS);
        assert_eq!(other.reputation(p), -5);
    }
    #[test]
    fn decay_runs_once_per_interval_whatever_the_gap() {
        let mut g = Gossip::default();
        let p = PlayerId(1);
        g.add(p, TRADING, 20);
        g.maybe_decay(500);
        assert_eq!(g.last_decay, 500);
        g.maybe_decay(500 + DECAY_INTERVAL - 1);
        assert_eq!(g.reputation(p), 20);
        // Ten days unloaded still decay once (Java ticks only loaded villagers).
        g.maybe_decay(500 + 10 * DECAY_INTERVAL);
        assert_eq!(g.reputation(p), 18);
        let mut old = serde_json::json!({"day": 3, "entries": []});
        assert_eq!(Gossip::load(&old).last_decay, 72000);
        old["decay"] = 5.into();
        assert_eq!(Gossip::load(&old).last_decay, 5);
    }
    #[test]
    fn gossip_needs_closeness_and_waits_out_the_cooldown() {
        let mut e = Entities::new(3);
        e.spawn(MobKind::Villager, glam::DVec3::ZERO);
        e.spawn(MobKind::Villager, glam::DVec3::X * 2.0);
        e.spawn(MobKind::Villager, glam::DVec3::X * 30.0);
        e.mobs[1].villager.as_mut().unwrap().gossip.add(PlayerId(4), MINOR_NEGATIVE, 100);
        let mut who = Vec::new();
        e.share_gossip(1000, &mut who);
        assert_eq!(who, [0], "one villager hears; both wait");
        assert_eq!(e.mobs[0].villager.as_ref().unwrap().gossip.reputation(PlayerId(4)), -80);
        e.mobs[1].villager.as_mut().unwrap().gossip = Gossip::default();
        e.mobs[1].villager.as_mut().unwrap().gossip.add(PlayerId(5), MINOR_NEGATIVE, 100);
        e.share_gossip(1000 + GOSSIP_COOLDOWN - 1, &mut who);
        assert!(who.is_empty());
        e.share_gossip(1000 + GOSSIP_COOLDOWN, &mut who);
        assert_eq!(who, [0]);
        assert_eq!(e.mobs[0].villager.as_ref().unwrap().gossip.reputation(PlayerId(5)), -80);
        assert_eq!(e.mobs[2].villager.as_ref().unwrap().gossip.reputation(PlayerId(4)), 0, "too far");
    }
}
