//! What LAN clients draw of the host's non-mob entities: projectiles, fishing bobbers,
//! eyes of ender, fireballs, breath puffs, experience orbs, primed TNT and the dragon fight.
//! Each is a position pair (latest and one tick earlier) plus what its model needs.
use super::{Entities, Puff, XpOrb, bobber::Bobber, eye::EnderEye, fireball::Fireball, pearl::Pearl};
use super::{PlayerId, potion::ThrownPotion, thrown, tnt::PrimedTnt};
use glam::DVec3;
use serde_json::{Value, json};

fn pair(pos: DVec3, previous: DVec3) -> [f64; 6] {
    [pos.x, pos.y, pos.z, previous.x, previous.y, previous.z]
}
fn unpair(v: &Value) -> Option<(DVec3, DVec3)> {
    let a = v.as_array()?;
    let f = |i: usize| a.get(i).and_then(Value::as_f64);
    let pos = DVec3::new(f(0)?, f(1)?, f(2)?);
    let previous = DVec3::new(f(3)?, f(4)?, f(5)?);
    (pos.is_finite() && previous.is_finite()).then_some((pos, previous))
}

impl Entities {
    /// Everything within `radius` of `center` a client should see, beyond mobs, items,
    /// arrows and vehicles (which have their own snapshots).
    pub fn replica_extras(&self, center: DVec3, radius: f64) -> Value {
        let near = |p: DVec3| p.distance_squared(center) < radius * radius;
        const MAX: usize = 128;
        json!({
            "pearls": self.pearls.iter().filter(|p| near(p.pos)).take(MAX).map(|p| pair(p.pos, p.previous_pos)).collect::<Vec<_>>(),
            "potions": self.potions.iter().filter(|p| near(p.pos)).take(MAX)
                .map(|p| json!({"at": pair(p.pos, p.previous_pos), "potion": p.potion.0})).collect::<Vec<_>>(),
            "thrown": self.thrown.iter().filter(|t| near(t.pos)).take(MAX)
                .map(|t| json!({"at": pair(t.pos, t.previous_pos), "kind": t.kind as u8})).collect::<Vec<_>>(),
            "bobbers": self.bobbers.iter().filter(|b| near(b.pos)).take(MAX)
                .map(|b| json!({"at": pair(b.pos, b.previous_pos), "owner": b.owner.0, "biting": b.biting})).collect::<Vec<_>>(),
            "eyes": self.eyes.iter().filter(|e| near(e.pos)).take(MAX).map(|e| pair(e.pos, e.previous_pos)).collect::<Vec<_>>(),
            "fireballs": self.fireballs.iter().filter(|f| near(f.pos)).take(MAX)
                .map(|f| json!({"at": pair(f.pos, f.previous_pos), "large": f.is_large()})).collect::<Vec<_>>(),
            "puffs": self.puffs.iter().filter(|p| near(p.pos)).take(MAX)
                .map(|p| json!({"at": pair(p.pos, p.previous_pos), "age": p.age, "life": p.life, "size": p.size})).collect::<Vec<_>>(),
            "orbs": self.orbs.iter().filter(|o| near(o.pos)).take(MAX)
                .map(|o| json!({"at": pair(o.pos, o.previous_pos), "value": o.value, "count": o.count, "phase": o.phase, "age": o.age})).collect::<Vec<_>>(),
            "tnt": self.tnt.iter().filter(|t| near(t.pos)).take(MAX)
                .map(|t| json!({"at": pair(t.pos, t.previous_pos), "fuse": t.fuse})).collect::<Vec<_>>(),
            "fight": self.fight.as_ref().map(|f| f.replica_json()),
        })
    }

    /// Replaces this client's copies with the host's latest [`Entities::replica_extras`].
    pub fn apply_replica_extras(&mut self, v: &Value) {
        let list = |key: &str| v[key].as_array().map(|a| a.iter().take(128).collect::<Vec<_>>()).unwrap_or_default();
        self.pearls = list("pearls").into_iter().filter_map(unpair).map(|(p, q)| Pearl::replica(p, q)).collect();
        self.potions = list("potions")
            .into_iter()
            .filter_map(|e| {
                let (p, q) = unpair(&e["at"])?;
                // Untrusted host data: only registered potions index the table.
                let index = e["potion"].as_u64().filter(|&i| (i as usize) < crate::potion::Potion::COUNT)?;
                let potion = crate::potion::Potion(index as u8);
                Some(ThrownPotion::replica(potion, p, q))
            })
            .collect();
        self.thrown = list("thrown")
            .into_iter()
            .filter_map(|e| {
                let (p, q) = unpair(&e["at"])?;
                let kind = match e["kind"].as_u64()? {
                    0 => thrown::Kind::Snowball,
                    1 => thrown::Kind::Egg,
                    _ => thrown::Kind::LlamaSpit,
                };
                Some(thrown::Thrown::replica(kind, p, q))
            })
            .collect();
        self.bobbers = list("bobbers")
            .into_iter()
            .filter_map(|e| {
                let (p, q) = unpair(&e["at"])?;
                let owner = PlayerId(u32::try_from(e["owner"].as_u64()?).ok()?);
                Some(Bobber::replica(owner, p, q, e["biting"] == true))
            })
            .collect();
        self.eyes = list("eyes").into_iter().filter_map(unpair).map(|(p, q)| EnderEye::replica(p, q)).collect();
        self.fireballs = list("fireballs")
            .into_iter()
            .filter_map(|e| {
                let (p, q) = unpair(&e["at"])?;
                Some(Fireball::replica(p, q, e["large"] == true))
            })
            .collect();
        self.puffs = list("puffs")
            .into_iter()
            .filter_map(|e| {
                let (pos, previous_pos) = unpair(&e["at"])?;
                Some(Puff {
                    pos,
                    previous_pos,
                    vel: DVec3::ZERO,
                    age: e["age"].as_f64()? as f32,
                    life: e["life"].as_f64()? as f32,
                    size: e["size"].as_f64()? as f32,
                })
            })
            .collect();
        self.orbs = list("orbs")
            .into_iter()
            .filter_map(|e| {
                let (pos, previous_pos) = unpair(&e["at"])?;
                Some(XpOrb {
                    value: e["value"].as_u64()?.min(2477) as u32,
                    count: e["count"].as_u64().unwrap_or(1).clamp(1, 64) as u32,
                    pos,
                    previous_pos,
                    vel: DVec3::ZERO,
                    age: e["age"].as_f64().unwrap_or(0.0) as f32,
                    phase: e["phase"].as_f64().unwrap_or(0.0) as f32,
                    health: super::orb::HEALTH,
                })
            })
            .collect();
        self.tnt = list("tnt")
            .into_iter()
            .filter_map(|e| {
                let (p, q) = unpair(&e["at"])?;
                Some(PrimedTnt::replica(p, q, e["fuse"].as_f64()? as f32))
            })
            .collect();
        match (&mut self.fight, &v["fight"]) {
            (_, Value::Null) => self.fight = None,
            (Some(f), fight) => f.apply_replica(fight),
            (None, fight) => self.fight = super::dragon::Fight::replica(fight),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::MobKind;

    #[test]
    fn extras_round_trip_every_drawn_entity() {
        let mut host = Entities::new(1);
        host.tnt.push(PrimedTnt::new(DVec3::new(1.0, 2.0, 3.0), DVec3::ZERO, 2.5));
        host.thrown.push(thrown::Thrown::replica(thrown::Kind::LlamaSpit, DVec3::X, DVec3::ZERO));
        host.pearls.push(Pearl::replica(DVec3::Y, DVec3::ZERO));
        host.fireballs.push(Fireball::replica(DVec3::Z, DVec3::ZERO, true));
        host.eyes.push(EnderEye::replica(DVec3::ONE, DVec3::ZERO));
        host.spawn_xp(DVec3::new(2.0, 0.0, 0.0), 7);
        host.spawn(MobKind::Cow, DVec3::ZERO);
        let snapshot = host.replica_extras(DVec3::ZERO, 64.0);
        let mut client = Entities::new(2);
        client.apply_replica_extras(&snapshot);
        assert_eq!(client.tnt.len(), 1);
        assert_eq!(client.tnt[0].fuse, 2.5);
        assert_eq!(client.thrown[0].kind, thrown::Kind::LlamaSpit);
        assert_eq!((client.pearls.len(), client.fireballs.len(), client.eyes.len()), (1, 1, 1));
        assert!(client.fireballs[0].is_large());
        let xp = |e: &Entities| e.orbs.iter().map(|o| o.value * o.count).sum::<u32>();
        assert_eq!(xp(&client), xp(&host));
        // Far away: not sent.
        assert!(host.replica_extras(DVec3::splat(1000.0), 64.0)["tnt"].as_array().unwrap().is_empty());
    }
}
