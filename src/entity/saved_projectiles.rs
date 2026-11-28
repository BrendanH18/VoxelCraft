//! v1.0 gap Track 8e: projectiles and primed TNT in the level file.
//!
//! Java saves every projectile but the fishing hook (`EntityTypes`'
//! `noSave`) with its chunk, and a thrown ender pearl with its owner
//! (`ServerPlayer.saveEnderPearls`). Each keeps `Entity.saveWithoutId`'s
//! `Fire` (`remainingFireTicks`), so a burning snowball saved mid-flight
//! reloads burning. VoxelCraft had saved only tridents (with their fire)
//! and Wither skulls; arrows, thrown items, thrown potions, firework
//! rockets, blaze and ghast fireballs, primed TNT (with its `owner`) and
//! ender pearls now go into the level file too, each under its own key
//! written only when there is one, so older saves resave unchanged. Like
//! tridents, those stored in unloaded chunks are saved with the dimension.
//! Pearls are saved with the dimension they fly in, not with their owner.

use std::collections::BTreeMap;

use glam::DVec3;
use serde_json::{Value, json};

use super::trident::Owner;
use super::{Entities, PlayerId};

/// The level file keys, in the order they are written.
pub const KEYS: [&str; 7] =
    ["arrows", "thrown", "thrown_potions", "fireworks", "fireballs", "primed_tnt", "ender_pearls"];

pub(super) fn v3(v: DVec3) -> Value {
    json!([v.x, v.y, v.z])
}

/// A finite vector saved by [`v3`].
pub(super) fn read_v3(v: &Value) -> Option<DVec3> {
    let a = v.as_array()?;
    let p = DVec3::new(a.first()?.as_f64()?, a.get(1)?.as_f64()?, a.get(2)?.as_f64()?);
    p.is_finite().then_some(p)
}

/// An owner as tridents save it: `{"player": id}` or `{"mob": uid}`.
pub(super) fn owner_json(owner: Option<Owner>) -> Value {
    match owner {
        Some(Owner::Player(id)) => json!({"player": id.0}),
        Some(Owner::Mob(uid)) => json!({"mob": uid}),
        None => Value::Null,
    }
}

pub(super) fn read_owner(v: &Value) -> Option<Owner> {
    if let Some(id) = v["player"].as_u64() {
        return u32::try_from(id).ok().map(|id| Owner::Player(PlayerId(id)));
    }
    v["mob"].as_u64().and_then(|u| u32::try_from(u).ok()).map(Owner::Mob)
}

pub(super) fn player_of(owner: Option<Owner>) -> Option<PlayerId> {
    match owner {
        Some(Owner::Player(id)) => Some(id),
        _ => None,
    }
}

pub(super) fn mob_of(owner: Option<Owner>) -> Option<u32> {
    match owner {
        Some(Owner::Mob(uid)) => Some(uid),
        _ => None,
    }
}

/// `remainingFireTicks` as saved (`Fire`, a short).
pub(super) fn read_fire(v: &Value) -> u16 {
    v["fire"].as_u64().unwrap_or(0).min(u64::from(u16::MAX)) as u16
}

pub(super) fn read_f32(v: &Value, key: &str, default: f32) -> f32 {
    v[key].as_f64().map(|f| f as f32).filter(|f| f.is_finite()).unwrap_or(default)
}

impl Entities {
    /// Writes the [`KEYS`] that have entities, stored ones included.
    pub(super) fn save_projectiles(&self, props: &mut BTreeMap<String, String>) {
        let stored = self.storage.chunks();
        let lists: [Vec<Value>; 7] = [
            self.arrows.iter().chain(stored.iter().flat_map(|c| &c.arrows)).map(|a| a.save()).collect(),
            self.thrown.iter().chain(stored.iter().flat_map(|c| &c.thrown)).map(|t| t.save()).collect(),
            self.potions.iter().chain(stored.iter().flat_map(|c| &c.potions)).map(|p| p.save()).collect(),
            self.fireworks.iter().filter_map(|r| r.save()).collect(),
            self.fireballs.iter().chain(stored.iter().flat_map(|c| &c.fireballs)).map(|f| f.save()).collect(),
            self.tnt.iter().chain(stored.iter().flat_map(|c| &c.tnt)).map(|t| t.save()).collect(),
            self.pearls.iter().map(|p| p.save()).collect(),
        ];
        for (key, list) in KEYS.iter().zip(lists) {
            if !list.is_empty() {
                props.insert(key.to_string(), Value::Array(list).to_string());
            }
        }
    }

    /// Restores [`Entities::save_projectiles`], skipping malformed entries.
    pub(super) fn load_projectiles(&mut self, props: &BTreeMap<String, String>) {
        let list = |key: &str| -> Vec<Value> {
            match props.get(key).map(|t| serde_json::from_str::<Value>(t)) {
                Some(Ok(Value::Array(list))) => list,
                _ => Vec::new(),
            }
        };
        self.arrows.extend(list("arrows").iter().filter_map(super::Arrow::load));
        self.thrown.extend(list("thrown").iter().filter_map(super::thrown::Thrown::load));
        self.potions.extend(list("thrown_potions").iter().filter_map(super::potion::ThrownPotion::load));
        self.fireworks.extend(list("fireworks").iter().filter_map(super::firework::Rocket::load));
        self.fireballs.extend(list("fireballs").iter().filter_map(super::fireball::Fireball::load));
        self.tnt.extend(list("primed_tnt").iter().filter_map(super::tnt::PrimedTnt::load));
        self.pearls.extend(list("ender_pearls").iter().filter_map(super::pearl::Pearl::load));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{Arrow, fireball, firework, potion, thrown, tnt};

    #[test]
    fn burning_projectiles_and_primed_tnt_save_and_load_with_their_fire_and_owner() {
        let mut e = Entities::new(1);
        let mut none = BTreeMap::new();
        e.save_props(&mut none);
        assert!(KEYS.iter().all(|k| !none.contains_key(*k)), "nothing to save writes no key: {none:?}");

        let (eye, dir) = (DVec3::new(0.5, 70.0, 0.5), DVec3::X);
        let mut snowball =
            thrown::Thrown::launch(thrown::Kind::Snowball, PlayerId::HOST, eye, dir, DVec3::ZERO, &mut e.rng);
        snowball.set_burning(true);
        e.thrown.push(snowball);
        let mut splash =
            potion::ThrownPotion::thrown(crate::potion::Potion::AWKWARD, PlayerId::HOST, eye, dir, DVec3::ZERO);
        splash.set_burning(true);
        e.potions.push(splash);
        let mut arrow = Arrow::shot(eye, dir, 1.0, true);
        arrow.set_burning(true);
        e.arrows.push(arrow);
        let stack = crate::inventory::Stack::new(crate::content::fireworks::FIREWORK_ROCKET, 1);
        let mut rocket = firework::Rocket::new(eye, &stack, Some(PlayerId::HOST), &mut e.rng);
        rocket.fire = 37;
        e.fireworks.push(rocket);
        let mut ball = fireball::Fireball::large(eye, dir);
        ball.owner = Some(Owner::Mob(5));
        e.fireballs.push(ball);
        let mut pearl = crate::entity::pearl::Pearl::thrown(PlayerId(3), eye, dir, DVec3::ZERO, &mut e.rng);
        pearl.set_burning(true);
        e.pearls.push(pearl);
        let mut primed = tnt::PrimedTnt::new(eye, DVec3::Y, 2.5);
        primed.owner = Some(Owner::Player(PlayerId::HOST));
        e.tnt.push(primed);

        let mut props = BTreeMap::new();
        e.save_props(&mut props);
        assert!(KEYS.iter().all(|k| props.contains_key(*k)), "{props:?}");
        let mut loaded = Entities::new(2);
        loaded.load_props(&props);
        assert!(loaded.thrown[0].burning() && loaded.potions[0].burning() && loaded.arrows[0].burning());
        assert!(loaded.pearls[0].burning() && loaded.pearls[0].owner == PlayerId(3));
        assert_eq!(loaded.fireworks[0].fire, 37);
        assert_eq!(loaded.fireballs[0].owner, Some(Owner::Mob(5)));
        assert_eq!(loaded.tnt[0].owner, Some(Owner::Player(PlayerId::HOST)));
        assert_eq!(loaded.tnt[0].fuse, 2.5);
        let mut again = BTreeMap::new();
        loaded.save_props(&mut again);
        for key in KEYS {
            assert_eq!(props[key], again[key], "{key} saves the same after a reload");
        }
    }
}
