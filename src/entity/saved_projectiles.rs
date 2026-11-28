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
//! v1.0 gap Track 8f: pearls are saved with their owner's player data
//! (`ender_pearls` with each one's dimension, which the game writes), not
//! here; a level file Track 8e wrote with `ender_pearls` still loads them.
//! Eyes of ender are saved here (`ender_eyes`), as Java saves them:
//! position and motion, without their target, life or survival.

use std::collections::BTreeMap;

use glam::DVec3;
use serde_json::{Value, json};

use super::trident::Owner;
use super::{Entities, PlayerId};

/// The level file keys, in the order they are written.
pub const KEYS: [&str; 7] =
    ["arrows", "thrown", "thrown_potions", "fireworks", "fireballs", "primed_tnt", "ender_eyes"];

/// v1.0 gap Track 8e's level file key for ender pearls, read but no longer
/// written (v1.0 gap Track 8f saves them with their owners).
pub const PEARLS_KEY: &str = "ender_pearls";

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
            self.eyes.iter().chain(stored.iter().flat_map(|c| &c.eyes)).map(|e| e.save()).collect(),
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
        self.pearls.extend(list(PEARLS_KEY).iter().filter_map(super::pearl::Pearl::load));
        self.eyes.extend(list("ender_eyes").iter().filter_map(super::eye::EnderEye::load));
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
        e.eyes.push(crate::entity::eye::EnderEye::signalled(eye, DVec3::new(900.0, 30.0, 0.0), true));
        let mut primed = tnt::PrimedTnt::new(eye, DVec3::Y, 2.5);
        primed.owner = Some(Owner::Player(PlayerId::HOST));
        e.tnt.push(primed);

        let mut props = BTreeMap::new();
        e.save_props(&mut props);
        assert!(KEYS.iter().all(|k| props.contains_key(*k)), "{props:?}");
        let mut loaded = Entities::new(2);
        loaded.load_props(&props);
        assert!(loaded.thrown[0].burning() && loaded.potions[0].burning() && loaded.arrows[0].burning());
        // v1.0 gap Track 8f: pearls go with their owners, not the level.
        assert!(!props.contains_key(PEARLS_KEY) && loaded.pearls.is_empty());
        let pearls = vec![e.pearls[0].save()];
        let mut old = BTreeMap::new();
        old.insert(PEARLS_KEY.to_string(), Value::Array(pearls).to_string());
        let mut from_8e = Entities::new(3);
        from_8e.load_props(&old);
        assert!(from_8e.pearls[0].burning() && from_8e.pearls[0].owner == PlayerId(3), "8e's key still loads");
        assert_eq!(loaded.fireworks[0].fire, 37);
        assert_eq!(loaded.fireballs[0].owner, Some(Owner::Mob(5)));
        assert_eq!(loaded.tnt[0].owner, Some(Owner::Player(PlayerId::HOST)));
        assert_eq!(loaded.tnt[0].fuse, 2.5);
        assert_eq!(loaded.eyes[0].pos, eye);
        let mut again = BTreeMap::new();
        loaded.save_props(&mut again);
        for key in KEYS {
            assert_eq!(props[key], again[key], "{key} saves the same after a reload");
        }
    }
}

/// v1.0 gap Track 8f: ender pearls kept with their owners' player data, as
/// Java's `ServerPlayer` keeps them (`enderPearls`, saved as `ender_pearls`
/// with each one's `ender_pearl_dimension`). A pearl is in the level only
/// while its owner is (`PlayerList.remove` takes it out with them,
/// `UNLOADED_WITH_PLAYER`); the others wait here, with the dimension they
/// fly in, until their owner is back in it (`loadAndSpawnEnderPearls`).
#[derive(Clone, Debug, Default)]
pub struct OwnedPearls {
    away: Vec<(crate::world::terrain::Dimension, super::pearl::Pearl)>,
}

impl OwnedPearls {
    /// Moves the pearls whose owner isn't `present` out of `entities` (in
    /// `dimension`) and those waiting in `dimension` for a present owner in.
    pub fn sync(
        &mut self,
        entities: &mut Entities,
        dimension: crate::world::terrain::Dimension,
        present: impl Fn(PlayerId) -> bool,
    ) {
        let mut i = 0;
        while i < entities.pearls.len() {
            if present(entities.pearls[i].owner) {
                i += 1;
            } else {
                self.away.push((dimension, entities.pearls.remove(i)));
            }
        }
        let mut i = 0;
        while i < self.away.len() {
            if self.away[i].0 == dimension && present(self.away[i].1.owner) {
                entities.pearls.push(self.away.remove(i).1);
            } else {
                i += 1;
            }
        }
    }

    /// Every pearl in `entities` leaves with the level (`dimension`) for a
    /// trip to another dimension, to wait there with its owner.
    pub fn leave_level(&mut self, entities: &mut Entities, dimension: crate::world::terrain::Dimension) {
        self.away.extend(entities.pearls.drain(..).map(|p| (dimension, p)));
    }

    /// `owner`'s pearls as their player data saves them (each with its
    /// dimension's name), those in the level (`dimension`) first; None
    /// without any, so a player without pearls saves no key.
    pub fn save(
        &self,
        entities: &Entities,
        dimension: crate::world::terrain::Dimension,
        owner: PlayerId,
    ) -> Option<Value> {
        let list: Vec<Value> = entities
            .pearls
            .iter()
            .map(|p| (dimension, p))
            .chain(self.away.iter().map(|(d, p)| (*d, p)))
            .filter(|(_, p)| p.owner == owner)
            .map(|(d, p)| {
                let mut v = p.save();
                v["dimension"] = json!(d.name());
                v
            })
            .collect();
        (!list.is_empty()).then_some(Value::Array(list))
    }

    /// Restores `owner`'s saved pearls (from [`OwnedPearls::save`]) to wait
    /// for them; the next [`OwnedPearls::sync`] puts those in the level
    /// they're in. A pearl of a dimension this game doesn't know is lost, as
    /// Java's `loadAndSpawnEnderPearl` skips one without its level.
    pub fn load(&mut self, saved: &Value, owner: PlayerId) {
        for v in saved.as_array().into_iter().flatten() {
            let Some(d) = v["dimension"].as_str().and_then(crate::world::terrain::Dimension::from_name) else {
                continue;
            };
            if let Some(mut p) = super::pearl::Pearl::load(v) {
                p.owner = owner;
                self.away.push((d, p));
            }
        }
    }

    /// Pearls waiting for their owners.
    pub fn waiting(&self) -> usize {
        self.away.len()
    }
}

#[cfg(test)]
mod owned_tests {
    use super::*;
    use crate::world::terrain::Dimension;

    /// v1.0 gap Track 8f: a pearl leaves the level with its owner, waits
    /// with them, comes back when they do, and saves with their player data
    /// (and its dimension) whether it's in the level or not.
    #[test]
    fn pearls_go_with_their_owners() {
        let mut e = Entities::new(1);
        let (eye, dir) = (DVec3::new(0.5, 70.0, 0.5), DVec3::X);
        let mine = crate::entity::pearl::Pearl::thrown(PlayerId::HOST, eye, dir, DVec3::ZERO, &mut e.rng);
        let guest = crate::entity::pearl::Pearl::thrown(PlayerId(4), eye, dir, DVec3::ZERO, &mut e.rng);
        e.pearls.extend([mine, guest]);
        let mut owned = OwnedPearls::default();
        assert_eq!(owned.save(&e, Dimension::Overworld, PlayerId(9)), None, "no pearls, no key");
        // The guest leaves: their pearl leaves the level with them.
        owned.sync(&mut e, Dimension::Overworld, |id| id == PlayerId::HOST);
        assert_eq!((e.pearls.len(), owned.waiting()), (1, 1));
        let saved = owned.save(&e, Dimension::Overworld, PlayerId(4)).unwrap();
        assert_eq!(saved[0]["dimension"], "overworld");
        assert_eq!(saved[0]["owner"], 4);
        // Saved and loaded with the guest's profile, it waits until they're back.
        let mut reloaded = OwnedPearls::default();
        reloaded.load(&saved, PlayerId(4));
        let mut fresh = Entities::new(2);
        reloaded.sync(&mut fresh, Dimension::Overworld, |id| id == PlayerId::HOST);
        assert!(fresh.pearls.is_empty());
        reloaded.sync(&mut fresh, Dimension::Nether, |_| true);
        assert!(fresh.pearls.is_empty(), "not in another dimension");
        reloaded.sync(&mut fresh, Dimension::Overworld, |_| true);
        assert_eq!(fresh.pearls.len(), 1);
        assert_eq!(fresh.pearls[0].save(), guest.save());
        // The host travels: their pearl stays with the Overworld.
        owned.sync(&mut e, Dimension::Overworld, |_| true);
        owned.leave_level(&mut e, Dimension::Overworld);
        assert!(e.pearls.is_empty());
        let mut nether = Entities::new(3);
        owned.sync(&mut nether, Dimension::Nether, |_| true);
        assert!(nether.pearls.is_empty());
        assert_eq!(owned.save(&nether, Dimension::Nether, PlayerId::HOST).unwrap()[0]["dimension"], "overworld");
        let mut back = Entities::new(4);
        owned.sync(&mut back, Dimension::Overworld, |_| true);
        assert_eq!(back.pearls.len(), 2);
    }
}
