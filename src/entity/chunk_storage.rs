//! Per-chunk entity storage, after Java 26.3's `PersistentEntitySectionManager`,
//! `EntitySection`, `EntityStorage` and `ChunkEntities`.
//!
//! Java keeps an entity in the level only while its chunk is at least
//! `FULL` (`Visibility.TRACKED`). When the chunk drops to `INACCESSIBLE`
//! (`HIDDEN`), `storeChunkSections` hands every entity that `shouldBeSaved`
//! to entity storage and unloads it together with its passengers; the
//! entities come back when the chunk loads. An entity that moves into a
//! hidden chunk stops ticking and is stored at the next save.
//!
//! Here the unit is the 32-block storage chunk the world streams, and an
//! entity is live exactly while that chunk is loaded. Entities that leave
//! the loaded area, or are restored from a save into chunks that haven't
//! loaded yet, move into [`EntityStorage`] untouched; their chunk loading
//! brings them back. Stored entities are saved with the dimension, so the
//! save layout and its identifiers are unchanged.
//!
//! As in Java, a vehicle with a player aboard travels with that player
//! (`Entity.shouldBeSaved` is false for it) and passengers are stored with
//! their root vehicle. A parrot on a player's shoulder belongs to the
//! player, and thrown ender pearls and fishing bobbers stay bound to their
//! owner. Stored villagers keep their bed and job claims, as Java's POI
//! records keep their tickets while the villager is unloaded.

use glam::{DVec3, IVec3};
use rustc_hash::FxHashMap;

use super::{Arrow, Entities, ItemEntity, Mob, MobKind, XpOrb, decor, eye, fireball, minecart, potion, thrown, tnt};
use crate::world::World;

/// The entities of one unloaded storage chunk (Java's `ChunkEntities`).
#[derive(Default)]
pub(super) struct ChunkEntities {
    pub(super) mobs: Vec<Mob>,
    pub(super) items: Vec<ItemEntity>,
    pub(super) orbs: Vec<XpOrb>,
    pub(super) minecarts: Vec<minecart::Minecart>,
    pub(super) arrows: Vec<Arrow>,
    pub(super) potions: Vec<potion::ThrownPotion>,
    pub(super) thrown: Vec<thrown::Thrown>,
    pub(super) eyes: Vec<eye::EnderEye>,
    pub(super) fireballs: Vec<fireball::Fireball>,
    pub(super) tnt: Vec<tnt::PrimedTnt>,
    pub(super) decor: Vec<decor::Decor>,
}

impl ChunkEntities {
    fn len(&self) -> usize {
        self.mobs.len()
            + self.items.len()
            + self.orbs.len()
            + self.minecarts.len()
            + self.arrows.len()
            + self.potions.len()
            + self.thrown.len()
            + self.eyes.len()
            + self.fireballs.len()
            + self.tnt.len()
            + self.decor.len()
    }
}

/// Entities of chunks that aren't loaded, by storage chunk.
#[derive(Default)]
pub(super) struct EntityStorage {
    chunks: FxHashMap<IVec3, ChunkEntities>,
    /// Beds and job sites claimed by stored villagers, with claim counts.
    beds: FxHashMap<IVec3, u32>,
    jobs: FxHashMap<IVec3, u32>,
    len: usize,
}

impl EntityStorage {
    /// Stored chunks in a fixed order, so saves don't depend on hashing.
    pub(super) fn chunks(&self) -> Vec<&ChunkEntities> {
        let mut keys: Vec<_> = self.chunks.keys().copied().collect();
        keys.sort_unstable_by_key(|p| (p.x, p.y, p.z));
        keys.iter().map(|k| &self.chunks[k]).collect()
    }

    pub(super) fn mobs(&self) -> impl Iterator<Item = &Mob> {
        self.chunks().into_iter().flat_map(|c| &c.mobs)
    }

    /// Whether a stored villager has claimed bed `p`. Checked where a bed is
    /// chosen, so upkeep never walks the stored claims.
    pub(super) fn claims_bed(&self, p: IVec3) -> bool {
        self.beds.contains_key(&p)
    }

    pub(super) fn claims_job(&self, p: IVec3) -> bool {
        self.jobs.contains_key(&p)
    }

    fn entry(&mut self, chunk: IVec3) -> &mut ChunkEntities {
        self.len += 1;
        self.chunks.entry(chunk).or_default()
    }

    fn store_mob(&mut self, chunk: IVec3, m: Mob) {
        if let Some((home, job)) = villager_claims(&m) {
            for (map, p) in [(&mut self.beds, home), (&mut self.jobs, job)] {
                if let Some(p) = p {
                    *map.entry(p).or_default() += 1;
                }
            }
        }
        self.entry(chunk).mobs.push(m);
    }

    fn release_claims(&mut self, m: &Mob) {
        let Some((home, job)) = villager_claims(m) else { return };
        for (map, p) in [(&mut self.beds, home), (&mut self.jobs, job)] {
            if let Some(p) = p
                && let Some(n) = map.get_mut(&p)
            {
                *n -= 1;
                if *n == 0 {
                    map.remove(&p);
                }
            }
        }
    }
}

/// The claims `village_upkeep` counts for a villager.
fn villager_claims(m: &Mob) -> Option<(Option<IVec3>, Option<IVec3>)> {
    let v = m.villager.as_ref().filter(|_| m.kind == MobKind::Villager && m.alive())?;
    Some((v.home, v.job))
}

/// Moves the entries of `list` that `away` places in an unloaded chunk into
/// storage, keeping the rest in order. Nothing is allocated unless something leaves.
fn unload<T>(list: &mut Vec<T>, away: impl Fn(&T) -> Option<IVec3>, mut store: impl FnMut(IVec3, T)) {
    if !list.iter().any(|e| away(e).is_some()) {
        return;
    }
    let mut kept = Vec::with_capacity(list.len());
    for e in list.drain(..) {
        match away(&e) {
            Some(chunk) => store(chunk, e),
            None => kept.push(e),
        }
    }
    *list = kept;
}

impl Entities {
    /// Applies chunk loads and unloads to the level's entities: entities of
    /// chunks that loaded since the last call come back, and entities whose
    /// chunk isn't loaded move into storage. Bounded by the live entities and
    /// the chunks that just loaded; stored entities are never visited.
    pub fn sync_chunk_entities(&mut self, world: &mut World) {
        for chunk in world.take_entity_loads() {
            self.restore_chunk(chunk);
        }
        self.store_unloaded(world);
    }

    /// Entities waiting in unloaded chunks.
    pub fn stored_entities(&self) -> usize {
        self.storage.len
    }

    /// Unloaded chunks that hold stored entities.
    pub fn stored_chunks(&self) -> usize {
        self.storage.chunks.len()
    }

    /// Stored mobs, in save order.
    pub fn stored_mobs(&self) -> impl Iterator<Item = &Mob> {
        self.storage.mobs()
    }

    fn restore_chunk(&mut self, chunk: IVec3) {
        let Some(c) = self.storage.chunks.remove(&chunk) else { return };
        self.storage.len -= c.len();
        for m in &c.mobs {
            self.storage.release_claims(m);
        }
        self.mobs.extend(c.mobs);
        self.items.extend(c.items);
        self.orbs.extend(c.orbs);
        self.minecarts.extend(c.minecarts);
        self.arrows.extend(c.arrows);
        self.potions.extend(c.potions);
        self.thrown.extend(c.thrown);
        self.eyes.extend(c.eyes);
        self.fireballs.extend(c.fireballs);
        self.tnt.extend(c.tnt);
        self.decor.extend(c.decor);
    }

    fn store_unloaded(&mut self, world: &World) {
        let away = |p: DVec3| {
            let chunk = world.entity_chunk(p);
            (!world.chunk_loaded(chunk)).then_some(chunk)
        };
        let storage = &mut self.storage;
        // Carts and boats are root vehicles; one carrying a player stays with them.
        let mut stored_carts: FxHashMap<u32, IVec3> = FxHashMap::default();
        unload(
            &mut self.minecarts,
            |c| if c.rider.is_some() || c.second_rider.is_some() { None } else { away(c.pos) },
            |chunk, c| {
                stored_carts.insert(c.id, chunk);
                storage.entry(chunk).minecarts.push(c);
            },
        );
        unload(
            &mut self.mobs,
            |m| {
                if let Some(cart) = m.riding {
                    // Passengers go with their vehicle, wherever they stand.
                    if let Some(&chunk) = stored_carts.get(&cart) {
                        return Some(chunk);
                    }
                    if self.minecarts.iter().any(|c| c.id == cart) {
                        return None;
                    }
                }
                let ridden = m.mount.as_ref().is_some_and(|s| s.rider.is_some() || s.passenger.is_some());
                let perched = m.animal.as_ref().is_some_and(|a| a.perched.is_some());
                if ridden || perched { None } else { away(m.pos) }
            },
            |chunk, m| storage.store_mob(chunk, m),
        );
        unload(&mut self.items, |e| away(e.pos), |chunk, e| storage.entry(chunk).items.push(e));
        unload(&mut self.orbs, |e| away(e.pos), |chunk, e| storage.entry(chunk).orbs.push(e));
        unload(&mut self.arrows, |e| away(e.pos), |chunk, e| storage.entry(chunk).arrows.push(e));
        unload(&mut self.potions, |e| away(e.pos), |chunk, e| storage.entry(chunk).potions.push(e));
        unload(&mut self.thrown, |e| away(e.pos), |chunk, e| storage.entry(chunk).thrown.push(e));
        unload(&mut self.eyes, |e| away(e.pos), |chunk, e| storage.entry(chunk).eyes.push(e));
        unload(&mut self.fireballs, |e| away(e.pos), |chunk, e| storage.entry(chunk).fireballs.push(e));
        unload(&mut self.tnt, |e| away(e.pos), |chunk, e| storage.entry(chunk).tnt.push(e));
        // Hanging decorations belong to the cell they hang in.
        unload(
            &mut self.decor,
            |d| away(if d.is_hanging() { d.cell.as_dvec3() + DVec3::splat(0.5) } else { d.pos }),
            |chunk, d| storage.entry(chunk).decor.push(d),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{Ctx, PlayerId, Target, leash::Leash, minecart::CartKind};
    use crate::inventory::{Stack, StackName};
    use crate::item::Item;
    use crate::world::block::Block;
    use crate::world::terrain::{Dimension, Generator};
    use glam::IVec2;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    const GROUND: i32 = 64;

    fn world(columns: impl IntoIterator<Item = IVec2>) -> World {
        let mut w = World::new_headless(Arc::new(Generator::new(3)), Default::default(), 2);
        for c in columns {
            w.load_test_column(c, GROUND);
        }
        w
    }

    fn at(x: f64, z: f64) -> DVec3 {
        DVec3::new(x, GROUND as f64 + 1.0, z)
    }

    fn ctx(pos: DVec3) -> Ctx {
        Ctx {
            players: vec![Target::new(PlayerId::HOST, pos, true)],
            daylight: 1.0,
            spawn_light: crate::simulation::lighting::SpawnLight::NIGHT,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
        }
    }

    /// Everything a reload must keep, for one mob.
    fn state(m: &Mob) -> String {
        format!(
            "{} {:?} {:?} {:?} {} {:?} {:?} {:?} {} {} {} {:?} {:?} {:?} {:?} {:?} {:?}",
            m.uid,
            m.kind,
            m.pos,
            m.vel,
            m.health,
            m.name,
            m.leash,
            m.riding,
            m.age,
            m.baby,
            m.persistent,
            m.animal.as_ref().map(|a| (a.owner, a.sitting, a.collar)),
            m.mount.as_ref().map(|s| (s.tame, s.owner, s.chest, s.slots)),
            m.villager.as_ref().map(|v| (v.id, v.home, v.job, v.profession, v.xp)),
            m.armor,
            m.effects.serialize(),
            m.built,
        )
    }

    /// Save properties with each list in a fixed order, since reloaded
    /// entities rejoin the level after those that stayed.
    fn normalized(e: &Entities) -> BTreeMap<String, String> {
        let mut props = BTreeMap::new();
        e.save_props(&mut props);
        for key in ["items", "orbs", "minecarts"] {
            let mut parts: Vec<_> = props[key].split(';').map(str::to_string).collect();
            parts.sort();
            props.insert(key.to_string(), parts.join(";"));
        }
        for key in ["villagers", "nether_mobs"] {
            let mut root: serde_json::Value = serde_json::from_str(&props[key]).unwrap();
            let mut mobs: Vec<String> = root["mobs"].as_array().unwrap().iter().map(|m| m.to_string()).collect();
            mobs.sort();
            root["mobs"] = serde_json::Value::from(mobs);
            props.insert(key.to_string(), root.to_string());
        }
        props
    }

    /// A tamed named wolf, a baby cow, a leashed sheep, a chested donkey, a
    /// villager with claims, a golem, a cart with a pig aboard, a chest boat,
    /// items, orbs, an arrow and primed TNT, all in storage column x = 2.
    fn populate(e: &mut Entities) -> usize {
        e.spawn(MobKind::Wolf, at(70.5, 5.5));
        let m = e.mobs.last_mut().unwrap();
        let a = m.animal.as_mut().unwrap();
        a.owner = Some(PlayerId::HOST);
        a.sitting = true;
        m.name = StackName::new("Rex").unwrap();
        m.health = 13.0;
        e.spawn_baby(MobKind::Cow, at(72.5, 9.5));
        e.mobs.last_mut().unwrap().age = -12_000;
        e.spawn(MobKind::Sheep, at(80.5, 20.5));
        e.mobs.last_mut().unwrap().leash = Some(Leash::Fence(IVec3::new(82, GROUND + 1, 20)));
        e.spawn(MobKind::Donkey, at(90.5, 3.5));
        let s = e.mobs.last_mut().unwrap().mount.as_mut().unwrap();
        s.tame = true;
        s.owner = Some(PlayerId::HOST);
        s.chest = true;
        s.slots[2] = Some(Stack::new(Item::DIAMOND, 7));
        e.spawn(MobKind::Villager, at(66.5, 26.5));
        let v = e.mobs.last_mut().unwrap().villager.as_mut().unwrap();
        v.home = Some(IVec3::new(67, GROUND + 1, 28));
        v.job = Some(IVec3::new(65, GROUND + 1, 24));
        e.spawn(MobKind::IronGolem, at(76.5, 28.5));
        e.mobs.last_mut().unwrap().built = true;
        let cart = e.spawn_cart(CartKind::Rideable, at(84.5, 12.5));
        e.spawn(MobKind::Pig, at(84.5, 12.5));
        e.mobs.last_mut().unwrap().riding = Some(cart);
        e.spawn_cart(CartKind::Boat { wood: 2, chest: true }, at(88.5, 30.5));
        e.minecarts.last_mut().unwrap().slots[0] = Some(Stack::new(Item::from(Block::STONE), 64));
        e.ensure_mob_ids();
        e.drop_from_block(Stack::new(Item::DIAMOND, 3), IVec3::new(78, GROUND + 1, 4));
        e.spawn_xp(at(74.5, 14.5), 17);
        e.arrows.push(super::super::Arrow::shot(at(70.5, 18.5), DVec3::X, 0.0, true));
        e.prime_tnt(IVec3::new(94, GROUND + 1, 2), false);
        e.mobs.len() + e.items.len() + e.orbs.len() + e.minecarts.len() + e.arrows.len() + e.tnt.len()
    }

    #[test]
    fn unloading_a_chunk_stores_its_entities_and_reloading_restores_them_unchanged() {
        let columns = || (-1..=2).flat_map(|x| (-1..=1).map(move |z| IVec2::new(x, z)));
        let mut w = world(columns());
        let mut e = Entities::new(5);
        let count = populate(&mut e);
        e.spawn(MobKind::Cow, at(10.5, 10.5));
        e.ensure_mob_ids();
        e.sync_chunk_entities(&mut w);
        assert_eq!(e.stored_entities(), 0, "everything is in loaded chunks");
        let before: Vec<String> = e.mobs.iter().filter(|m| m.pos.x >= 64.0).map(state).collect();
        let saved = normalized(&e);

        for z in -1..=1 {
            w.unload_test_column(IVec2::new(2, z));
        }
        e.sync_chunk_entities(&mut w);
        assert_eq!(e.stored_entities(), count);
        assert_eq!(e.mobs.len(), 1, "only the cow in the loaded column stays in the level");
        assert!(e.items.is_empty() && e.orbs.is_empty() && e.minecarts.is_empty());
        assert!(e.arrows.is_empty() && e.tnt.is_empty());
        assert_eq!(normalized(&e), saved, "a save while unloaded keeps every entity");
        assert!(e.storage.claims_bed(IVec3::new(67, GROUND + 1, 28)));
        assert!(e.storage.claims_job(IVec3::new(65, GROUND + 1, 24)));
        let c = ctx(at(10.5, 0.5));
        for _ in 0..40 {
            e.update(0.05, &w, &c);
            e.sync_chunk_entities(&mut w);
        }
        assert_eq!(e.stored_entities(), count, "stored entities don't tick, despawn or explode");

        for z in -1..=1 {
            w.load_test_column(IVec2::new(2, z), GROUND);
        }
        e.sync_chunk_entities(&mut w);
        assert_eq!(e.stored_entities(), 0);
        assert_eq!(e.stored_chunks(), 0);
        let after: Vec<String> = e.mobs.iter().filter(|m| m.pos.x >= 64.0).map(state).collect();
        assert_eq!(after, before);
        assert!(e.storage.beds.is_empty() && e.storage.jobs.is_empty(), "restored villagers own their claims again");
        assert_eq!(
            e.items.len() + e.orbs.len() + e.minecarts.len() + e.arrows.len() + e.tnt.len() + after.len(),
            count
        );
    }

    #[test]
    fn passengers_go_with_their_vehicle_and_player_vehicles_stay_with_the_player() {
        let columns = || (0..=2).map(|x| IVec2::new(x, 0));
        let mut w = world(columns());
        let mut e = Entities::new(5);
        // The pig stands in a column that stays loaded but rides a cart that unloads.
        let cart = e.spawn_cart(CartKind::Rideable, at(70.5, 4.5));
        e.spawn(MobKind::Pig, at(60.5, 4.5));
        e.mobs[0].riding = Some(cart);
        let ridden = e.spawn_cart(CartKind::Boat { wood: 0, chest: false }, at(75.5, 4.5));
        e.minecarts.last_mut().unwrap().rider = Some(PlayerId::HOST);
        e.spawn(MobKind::Horse, at(80.5, 4.5));
        let s = e.mobs.last_mut().unwrap().mount.as_mut().unwrap();
        s.tame = true;
        s.rider = Some(PlayerId(2));
        e.spawn(MobKind::Parrot, at(85.5, 4.5));
        e.mobs.last_mut().unwrap().animal.as_mut().unwrap().perched = Some(0);
        e.ensure_mob_ids();
        w.unload_test_column(IVec2::new(2, 0));
        e.sync_chunk_entities(&mut w);
        assert_eq!(e.stored_entities(), 2, "the cart and its passenger");
        assert_eq!(e.minecarts.iter().map(|c| c.id).collect::<Vec<_>>(), [ridden]);
        assert_eq!(e.mobs.iter().map(|m| m.kind).collect::<Vec<_>>(), [MobKind::Horse, MobKind::Parrot]);
        w.load_test_column(IVec2::new(2, 0), GROUND);
        e.sync_chunk_entities(&mut w);
        let pig = e.mobs.iter().find(|m| m.kind == MobKind::Pig).unwrap();
        assert_eq!(pig.riding, Some(cart));
        assert!(e.minecarts.iter().any(|c| c.id == cart));
    }

    #[test]
    fn entities_entering_an_unloaded_chunk_are_stored_there() {
        let mut w = world([IVec2::new(0, 0), IVec2::new(1, 0)]);
        let mut e = Entities::new(5);
        e.spawn(MobKind::Cow, at(60.5, 4.5));
        e.drop_from_block(Stack::new(Item::DIAMOND, 1), IVec3::new(61, GROUND + 1, 4));
        e.sync_chunk_entities(&mut w);
        assert_eq!(e.stored_entities(), 0);
        e.mobs[0].pos.x = 66.5;
        e.items[0].pos.x = 130.0;
        e.sync_chunk_entities(&mut w);
        assert_eq!((e.mobs.len(), e.items.len(), e.stored_entities(), e.stored_chunks()), (0, 0, 2, 2));
        w.load_test_column(IVec2::new(2, 0), GROUND);
        e.sync_chunk_entities(&mut w);
        assert_eq!((e.mobs.len(), e.items.len(), e.stored_entities()), (1, 0, 1));
        w.load_test_column(IVec2::new(4, 0), GROUND);
        e.sync_chunk_entities(&mut w);
        assert_eq!((e.items.len(), e.stored_entities()), (1, 0));
    }

    #[test]
    fn entities_beyond_the_build_limit_unload_with_their_column() {
        let mut w = world([IVec2::new(0, 0), IVec2::new(1, 0)]);
        let mut e = Entities::new(5);
        e.drop_from_block(Stack::new(Item::DIAMOND, 1), IVec3::new(40, 400, 4));
        e.drop_from_block(Stack::new(Item::DIAMOND, 1), IVec3::new(41, -100, 4));
        e.sync_chunk_entities(&mut w);
        assert_eq!(e.items.len(), 2);
        w.unload_test_column(IVec2::new(1, 0));
        e.sync_chunk_entities(&mut w);
        assert_eq!((e.items.len(), e.stored_entities()), (0, 2));
    }

    #[test]
    fn stored_villagers_keep_their_bed_and_job_claims() {
        let mut w = world([IVec2::new(0, 0), IVec2::new(2, 0)]);
        let home = IVec3::new(4, GROUND + 1, 4);
        assert!(w.set_block(home, Block::BED_HEAD));
        let mut e = Entities::new(5);
        e.spawn(MobKind::Villager, at(70.5, 4.5));
        e.mobs[0].villager.as_mut().unwrap().home = Some(home);
        w.unload_test_column(IVec2::new(2, 0));
        e.sync_chunk_entities(&mut w);
        assert!(e.mobs.is_empty());
        // A homeless villager next to the bed doesn't take the stored villager's.
        e.spawn(MobKind::Villager, at(6.5, 4.5));
        e.village_upkeep(1.0, &w);
        assert_eq!(e.mobs[0].villager.as_ref().unwrap().home, None);
        e.mobs.clear();
        w.load_test_column(IVec2::new(2, 0), GROUND);
        e.sync_chunk_entities(&mut w);
        e.village_upkeep(1.0, &w);
        assert_eq!(e.mobs[0].villager.as_ref().unwrap().home, Some(home), "the owner kept it");
        // Once the owner gives the bed up, another villager may claim it.
        e.mobs.clear();
        e.spawn(MobKind::Villager, at(6.5, 4.5));
        e.village_upkeep(1.0, &w);
        assert_eq!(e.mobs[0].villager.as_ref().unwrap().home, Some(home));
    }

    #[test]
    fn saved_entities_of_unloaded_chunks_wait_for_their_chunk_after_loading() {
        let mut w = world([IVec2::new(0, 0), IVec2::new(2, 0)]);
        let mut e = Entities::new(5);
        let count = populate(&mut e);
        w.unload_test_column(IVec2::new(2, 0));
        e.sync_chunk_entities(&mut w);
        let mut props = BTreeMap::new();
        e.save_props(&mut props);
        let saved = normalized(&e);

        // A fresh session: everything loads into the level, then waits in storage.
        let mut w = world([IVec2::new(0, 0)]);
        let mut e = Entities::new(5);
        e.load_props(&props);
        e.sync_chunk_entities(&mut w);
        // Arrows and primed TNT were never part of the save format.
        assert_eq!(e.stored_entities(), count - 2);
        assert_eq!(normalized(&e), saved);
        w.load_test_column(IVec2::new(2, 0), GROUND);
        e.sync_chunk_entities(&mut w);
        assert_eq!(e.stored_entities(), 0);
        assert_eq!(normalized(&e), saved);
        let wolf = e.mobs.iter().find(|m| m.kind == MobKind::Wolf).unwrap();
        assert_eq!(wolf.name.as_str(), Some("Rex"));
        assert_eq!(wolf.animal.as_ref().unwrap().owner, Some(PlayerId::HOST));
    }

    #[test]
    fn lan_replicas_never_store_falling_blocks() {
        let mut w = World::new_remote(Arc::new(Generator::new(3)), 2);
        for cy in Dimension::Overworld.chunk_rows() {
            w.receive_chunk(IVec3::new(2, cy, 0), Arc::new(crate::world::chunk::ChunkData::Uniform(Block::AIR)));
        }
        let p = DVec3::new(70.0, 90.0, 5.0);
        w.set_falling_replica([(Block::SAND, p, p)]);
        w.unload_test_column(IVec2::new(2, 0));
        assert_eq!(w.stored_falling_blocks(), 0, "the host's next snapshot replaces the replica");
        w.set_falling_replica([]);
        assert!(w.falling_blocks().is_empty());
    }

    #[test]
    fn falling_blocks_unload_and_reload_with_their_chunk() {
        let mut w = world([IVec2::new(0, 0), IVec2::new(2, 0)]);
        w.load_falling(&format!("v1|{},70,90,5,2.5|{},40,95,5,0", Block::SAND.0, Block::SAND.0));
        assert_eq!((w.falling_blocks().len(), w.stored_falling_blocks()), (1, 1), "x = 40 is not loaded");
        let saved = w.falling_to_string();
        w.unload_test_column(IVec2::new(2, 0));
        assert_eq!((w.falling_blocks().len(), w.stored_falling_blocks()), (0, 2));
        assert_eq!(w.falling_to_string().split('|').count(), saved.split('|').count());
        w.tick_falling(0.05);
        w.load_test_column(IVec2::new(1, 0), GROUND);
        w.load_test_column(IVec2::new(2, 0), GROUND);
        assert_eq!((w.falling_blocks().len(), w.stored_falling_blocks()), (2, 0));
        let mut entries: Vec<_> = w.falling_to_string().split('|').map(str::to_string).collect();
        let mut expected: Vec<_> = saved.split('|').map(str::to_string).collect();
        entries.sort();
        expected.sort();
        assert_eq!(entries, expected, "stored blocks kept their height and speed");
    }
}
