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
use rustc_hash::{FxHashMap, FxHashSet};

use super::{Arrow, Entities, ItemEntity, Mob, MobKind, XpOrb, decor, eye, fireball, minecart, potion, thrown, tnt};
use crate::world::World;
use crate::world::region::Props;

/// The entities of one unloaded storage chunk (Java's `ChunkEntities`).
#[derive(Default)]
pub(super) struct ChunkEntities {
    pub(super) mobs: Vec<Mob>,
    pub(super) items: Vec<ItemEntity>,
    pub(super) orbs: Vec<XpOrb>,
    pub(super) minecarts: Vec<minecart::Minecart>,
    pub(super) arrows: Vec<Arrow>,
    pub(super) clouds: Vec<super::lingering::Cloud>,
    pub(super) potions: Vec<potion::ThrownPotion>,
    pub(super) thrown: Vec<thrown::Thrown>,
    pub(super) eyes: Vec<eye::EnderEye>,
    pub(super) fireballs: Vec<fireball::Fireball>,
    pub(super) tnt: Vec<tnt::PrimedTnt>,
    pub(super) decor: Vec<decor::Decor>,
    pub(super) tridents: Vec<super::trident::Trident>,
}

impl ChunkEntities {
    fn len(&self) -> usize {
        self.mobs.len()
            + self.items.len()
            + self.orbs.len()
            + self.minecarts.len()
            + self.arrows.len()
            + self.clouds.len()
            + self.potions.len()
            + self.thrown.len()
            + self.eyes.len()
            + self.fireballs.len()
            + self.tnt.len()
            + self.decor.len()
            + self.tridents.len()
    }
}

/// Entities of chunks that aren't loaded, by storage chunk.
#[derive(Default)]
pub(super) struct EntityStorage {
    chunks: FxHashMap<IVec3, ChunkEntities>,
    /// Beds and job sites claimed by stored villagers, with claim counts.
    beds: FxHashMap<IVec3, u32>,
    jobs: FxHashMap<IVec3, u32>,
    /// The same claims by 16-block cell, one entry per claim, so a raid's
    /// bell can find stored claims near it without walking them all.
    claim_cells: FxHashMap<IVec3, Vec<IVec3>>,
    len: usize,
    /// v1.0 Track 3b: the claims of villagers stored in region records
    /// (Java keeps them in its POI records while the villager is unloaded),
    /// and the heart-bound creakings there, saved in the level file
    /// (`stored_index`) so they last until the chunk loads again.
    disk: Claims,
    disk_creakings: FxHashSet<u32>,
    /// Mob uids and cart ids of newer copies in region records, whose
    /// level file copies the next sync drops ([`Entities::recover_ids`]).
    superseded: Option<(FxHashSet<u32>, FxHashSet<u32>)>,
}

/// Bed and job claims with counts, by position and by 16-block cell.
#[derive(Default)]
struct Claims {
    beds: FxHashMap<IVec3, u32>,
    jobs: FxHashMap<IVec3, u32>,
    cells: FxHashMap<IVec3, Vec<IVec3>>,
}

impl Claims {
    fn add(&mut self, home: Option<IVec3>, job: Option<IVec3>) {
        for (map, p) in [(&mut self.beds, home), (&mut self.jobs, job)] {
            if let Some(p) = p {
                *map.entry(p).or_default() += 1;
                self.cells.entry(p.div_euclid(IVec3::splat(16))).or_default().push(p);
            }
        }
    }

    fn release(&mut self, home: Option<IVec3>, job: Option<IVec3>) {
        for (map, p) in [(&mut self.beds, home), (&mut self.jobs, job)] {
            let Some(p) = p else { continue };
            let Some(n) = map.get_mut(&p) else { continue };
            *n -= 1;
            if *n == 0 {
                map.remove(&p);
            }
            let cell = p.div_euclid(IVec3::splat(16));
            if let Some(claims) = self.cells.get_mut(&cell) {
                if let Some(i) = claims.iter().position(|&c| c == p) {
                    claims.swap_remove(i);
                }
                if claims.is_empty() {
                    self.cells.remove(&cell);
                }
            }
        }
    }

    fn near(&self, p: IVec3, radius: f64) -> bool {
        let r = radius.ceil() as i32;
        let (lo, hi) =
            ((p - IVec3::splat(r)).div_euclid(IVec3::splat(16)), (p + IVec3::splat(r)).div_euclid(IVec3::splat(16)));
        (lo.x..=hi.x).any(|x| {
            (lo.y..=hi.y).any(|y| {
                (lo.z..=hi.z).any(|z| {
                    self.cells.get(&IVec3::new(x, y, z)).is_some_and(|claims| {
                        claims.iter().any(|c| c.as_dvec3().distance_squared(p.as_dvec3()) <= radius * radius)
                    })
                })
            })
        })
    }
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
        self.beds.contains_key(&p) || self.disk.beds.contains_key(&p)
    }

    pub(super) fn claims_job(&self, p: IVec3) -> bool {
        self.jobs.contains_key(&p) || self.disk.jobs.contains_key(&p)
    }

    /// Every bed stored villagers have claimed.
    pub(super) fn claimed_beds(&self) -> impl Iterator<Item = IVec3> + '_ {
        self.beds.keys().chain(self.disk.beds.keys()).copied()
    }

    /// Every job site stored villagers have claimed.
    pub(super) fn claimed_jobs(&self) -> impl Iterator<Item = IVec3> + '_ {
        self.jobs.keys().chain(self.disk.jobs.keys()).copied()
    }

    /// Whether a mob with uid `id` waits in storage, in memory or in a record.
    pub(super) fn holds_mob(&self, id: u32) -> bool {
        self.disk_creakings.contains(&id) || self.mobs().any(|m| m.uid == id)
    }

    /// Whether a stored villager has claimed a bed or job site within
    /// `radius` blocks of `p`.
    pub(super) fn claimed_near(&self, p: IVec3, radius: f64) -> bool {
        if self.disk.near(p, radius) {
            return true;
        }
        let r = radius.ceil() as i32;
        let (lo, hi) =
            ((p - IVec3::splat(r)).div_euclid(IVec3::splat(16)), (p + IVec3::splat(r)).div_euclid(IVec3::splat(16)));
        (lo.x..=hi.x).any(|x| {
            (lo.y..=hi.y).any(|y| {
                (lo.z..=hi.z).any(|z| {
                    self.claim_cells.get(&IVec3::new(x, y, z)).is_some_and(|claims| {
                        claims.iter().any(|c| c.as_dvec3().distance_squared(p.as_dvec3()) <= radius * radius)
                    })
                })
            })
        })
    }

    fn entry(&mut self, chunk: IVec3) -> &mut ChunkEntities {
        self.len += 1;
        self.chunks.entry(chunk).or_default()
    }

    pub(super) fn store_mob(&mut self, chunk: IVec3, m: Mob) {
        if let Some((home, job)) = villager_claims(&m) {
            for (map, p) in [(&mut self.beds, home), (&mut self.jobs, job)] {
                if let Some(p) = p {
                    *map.entry(p).or_default() += 1;
                    self.claim_cells.entry(p.div_euclid(IVec3::splat(16))).or_default().push(p);
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
                let cell = p.div_euclid(IVec3::splat(16));
                if let Some(claims) = self.claim_cells.get_mut(&cell) {
                    if let Some(i) = claims.iter().position(|&c| c == p) {
                        claims.swap_remove(i);
                    }
                    if claims.is_empty() {
                        self.claim_cells.remove(&cell);
                    }
                }
            }
        }
    }
}

/// The level file entity keys a chunk record's entity batches hold.
pub const RECORD_KEYS: [&str; 8] =
    ["potion_clouds", "items", "orbs", "minecarts", "decor", "villagers", "nether_mobs", "tridents"];

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
        // Normally done before the level file loaded (`recover_ids`).
        self.recover_ids(world);
        if let Some(ids) = self.storage.superseded.take() {
            self.drop_superseded(ids);
        }
        for (_, batches) in world.take_entity_records() {
            self.restore_batches(batches);
        }
        for chunk in world.take_entity_loads() {
            self.restore_chunk(chunk);
        }
        self.store_unloaded(world);
        // v1.0 Track 3b: with region storage, stored entities go on to their
        // chunk's record (Java's `EntityStorage.storeEntities`).
        if world.writes_records() {
            let chunks: Vec<IVec3> =
                self.storage.chunks.keys().copied().filter(|&c| !world.keeps_entities(c)).collect();
            let batches = self.serialize_stored(chunks);
            world.store_entities(batches);
        }
    }

    /// Reads the mob uids and cart ids saved in the region records written
    /// after the level file by a session that stopped before its next save
    /// (see `world::chunk_records`), and moves the next ids past them, so
    /// nothing the level file loads or spawns afterwards takes one. Only
    /// ids the records hold count: a mob saved without a uid has none to
    /// match. Called before the level file's entities load; the first
    /// [`Entities::sync_chunk_entities`] then drops the level file's copies.
    pub fn recover_ids(&mut self, world: &mut World) {
        let batches = world.take_superseding_entities();
        if batches.is_empty() {
            return;
        }
        let (mut uids, mut carts) = (FxHashSet::default(), FxHashSet::default());
        for (key, text) in batches.iter().flatten() {
            match key.as_str() {
                "villagers" | "nether_mobs" => {
                    let root: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
                    let saved = root["mobs"].as_array().into_iter().flatten();
                    uids.extend(saved.filter_map(|m| u32::try_from(m["uid"].as_u64()?).ok()).filter(|&u| u != 0));
                }
                "minecarts" => carts.extend(
                    text.split(';').filter(|s| !s.is_empty()).filter_map(minecart::Minecart::deserialize).map(|c| c.id),
                ),
                _ => {}
            }
        }
        if let Some(&u) = uids.iter().max() {
            self.next_uid = self.next_uid.max(u.saturating_add(1));
        }
        if let Some(&c) = carts.iter().max() {
            self.next_cart = self.next_cart.max(c.saturating_add(1));
        }
        self.storage.superseded = Some((uids, carts));
    }

    /// Drops the level file's copies of the mobs and carts
    /// [`Entities::recover_ids`] found newer copies of; those come back
    /// when their chunks load.
    fn drop_superseded(&mut self, (uids, carts): (FxHashSet<u32>, FxHashSet<u32>)) {
        let before = self.mobs.len() + self.minecarts.len() + self.storage.len;
        self.mobs.retain(|m| !uids.contains(&m.uid));
        self.minecarts.retain(|c| !carts.contains(&c.id));
        let (mut gone, mut carts_gone) = (Vec::new(), 0);
        for chunk in self.storage.chunks.values_mut() {
            gone.extend(chunk.mobs.extract_if(.., |m| uids.contains(&m.uid)));
            carts_gone += chunk.minecarts.extract_if(.., |c| carts.contains(&c.id)).count();
        }
        self.storage.len -= gone.len() + carts_gone;
        for m in &gone {
            self.storage.release_claims(m);
        }
        let dropped = before - self.mobs.len() - self.minecarts.len() - self.storage.len;
        if dropped > 0 {
            log::warn!("{dropped} entities were saved again after the level file; keeping only the newer copies");
        }
    }

    /// v1.0 Track 3b: at a save with region storage, every stored chunk's
    /// entities but those of chunks still loading, for
    /// [`World::save_records`]. Chunks that loaded since the last tick
    /// bring their entities back first (streaming goes on while the game
    /// is paused, and a loaded chunk's entities are the level file's).
    pub fn take_stored_batches(&mut self, world: &mut World) -> Vec<(IVec3, Props)> {
        self.sync_chunk_entities(world);
        let chunks: Vec<IVec3> = self.storage.chunks.keys().copied().filter(|&c| !world.chunk_loading(c)).collect();
        self.serialize_stored(chunks)
    }

    /// Takes `chunks`' stored entities out of memory as level file entity
    /// keys ([`RECORD_KEYS`]), one batch per chunk. Arrows, thrown items,
    /// fireballs and primed TNT were never saved, so they stay in memory.
    /// Villagers' claims and heart-bound creakings stay indexed.
    fn serialize_stored(&mut self, mut chunks: Vec<IVec3>) -> Vec<(IVec3, Props)> {
        if chunks.is_empty() {
            return Vec::new();
        }
        chunks.sort_unstable_by_key(|c| (c.x, c.y, c.z));
        let mut scratch = Entities::new(0);
        let mut empty = std::collections::BTreeMap::new();
        scratch.save_props(&mut empty);
        let mut out = Vec::new();
        for c in chunks {
            let Some(mut ce) = self.storage.chunks.remove(&c) else { continue };
            self.storage.len -= ce.len();
            let kept = ChunkEntities {
                arrows: std::mem::take(&mut ce.arrows),
                potions: std::mem::take(&mut ce.potions),
                thrown: std::mem::take(&mut ce.thrown),
                eyes: std::mem::take(&mut ce.eyes),
                fireballs: std::mem::take(&mut ce.fireballs),
                tnt: std::mem::take(&mut ce.tnt),
                ..Default::default()
            };
            if kept.len() > 0 {
                self.storage.len += kept.len();
                self.storage.chunks.insert(c, kept);
            }
            for m in &ce.mobs {
                self.storage.release_claims(m);
                if let Some((home, job)) = villager_claims(m) {
                    self.storage.disk.add(home, job);
                }
                if m.alive() && m.creaking.as_ref().is_some_and(|s| s.home.is_some()) {
                    self.storage.disk_creakings.insert(m.uid);
                }
            }
            scratch.mobs = ce.mobs;
            scratch.items = ce.items;
            scratch.orbs = ce.orbs;
            scratch.minecarts = ce.minecarts;
            scratch.clouds = ce.clouds;
            scratch.decor = ce.decor;
            scratch.tridents = ce.tridents;
            let mut props = std::collections::BTreeMap::new();
            scratch.save_props(&mut props);
            let batch: Props = props
                .into_iter()
                .filter(|(k, v)| RECORD_KEYS.contains(&k.as_str()) && empty.get(k) != Some(v))
                .collect();
            scratch.mobs.clear();
            scratch.items.clear();
            scratch.orbs.clear();
            scratch.minecarts.clear();
            scratch.clouds.clear();
            scratch.decor.clear();
            scratch.tridents.clear();
            if !batch.is_empty() {
                out.push((c, batch));
            }
        }
        out
    }

    /// Brings back a loading chunk's entities from its record: each batch
    /// is read as the level file would be, then joins the level.
    fn restore_batches(&mut self, batches: Vec<Props>) {
        for batch in batches {
            let mut scratch = Entities::new(0);
            scratch.next_uid = self.next_uid;
            scratch.next_cart = self.next_cart;
            scratch.next_villager_id = self.next_villager_id;
            scratch.load_props(&batch.into_iter().collect());
            self.next_uid = self.next_uid.max(scratch.next_uid);
            self.next_cart = self.next_cart.max(scratch.next_cart);
            self.next_villager_id = self.next_villager_id.max(scratch.next_villager_id);
            for m in &scratch.mobs {
                if let Some((home, job)) = villager_claims(m) {
                    self.storage.disk.release(home, job);
                }
                self.storage.disk_creakings.remove(&m.uid);
            }
            self.mobs.append(&mut scratch.mobs);
            self.items.append(&mut scratch.items);
            self.orbs.append(&mut scratch.orbs);
            self.minecarts.append(&mut scratch.minecarts);
            self.clouds.append(&mut scratch.clouds);
            self.decor.append(&mut scratch.decor);
            self.tridents.append(&mut scratch.tridents);
        }
    }

    /// v1.0 Track 3b: the `stored_index` level file text: the claims of
    /// villagers and the heart-bound creakings stored in region records.
    pub fn stored_index_to_string(&self) -> String {
        let mut claims: Vec<String> = Vec::new();
        let pos = |p: &IVec3| format!("{},{},{}", p.x, p.y, p.z);
        for (kind, map) in [("b", &self.storage.disk.beds), ("j", &self.storage.disk.jobs)] {
            for (p, n) in map {
                claims.push(format!("{kind}{}x{n}", pos(p)));
            }
        }
        let mut uids: Vec<u32> = self.storage.disk_creakings.iter().copied().collect();
        uids.sort_unstable();
        claims.extend(uids.iter().map(|u| format!("c{u}")));
        claims.sort();
        claims.join("|")
    }

    /// Restores [`Entities::stored_index_to_string`].
    pub fn load_stored_index(&mut self, text: &str) {
        for entry in text.split('|').filter(|e| !e.is_empty()) {
            let (kind, rest) = entry.split_at_checked(1).unwrap_or(("", ""));
            if kind == "c" {
                if let Ok(uid) = rest.parse() {
                    self.storage.disk_creakings.insert(uid);
                }
                continue;
            }
            let Some((at, n)) = rest.split_once('x') else { continue };
            let c: Vec<i32> = at.split(',').filter_map(|v| v.parse().ok()).collect();
            let (&[x, y, z], Ok(n)) = (&c[..], n.parse::<u32>()) else { continue };
            let p = Some(IVec3::new(x, y, z));
            for _ in 0..n.min(64) {
                match kind {
                    "b" => self.storage.disk.add(p, None),
                    "j" => self.storage.disk.add(None, p),
                    _ => {}
                }
            }
        }
    }

    /// The `entity_ids` level file text: the next mob uid and cart id, which
    /// entities in region records may already use.
    pub fn entity_ids_to_string(&self) -> String {
        format!("{},{}", self.next_uid, self.next_cart)
    }

    pub fn load_entity_ids(&mut self, text: &str) {
        if let Some((uid, cart)) = text.split_once(',')
            && let (Ok(uid), Ok(cart)) = (uid.parse::<u32>(), cart.parse::<u32>())
        {
            self.next_uid = self.next_uid.max(uid);
            self.next_cart = self.next_cart.max(cart);
        }
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
        self.clouds.extend(c.clouds);
        self.thrown.extend(c.thrown);
        self.eyes.extend(c.eyes);
        self.fireballs.extend(c.fireballs);
        self.tnt.extend(c.tnt);
        self.decor.extend(c.decor);
        self.tridents.extend(c.tridents);
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
        let horse_chunks: FxHashMap<_, _> = self
            .mobs
            .iter()
            .filter(|m| super::camel_husk::mob_mount(m.kind))
            .map(|m| {
                (
                    super::mounts::MOB_VEHICLE | m.uid,
                    (
                        crate::world::chunk::chunk_of(m.pos.floor().as_ivec3()),
                        m.mount.as_ref().is_some_and(|s| s.rider.is_some()),
                    ),
                )
            })
            .collect();
        unload(
            &mut self.mobs,
            |m| {
                if let Some(vehicle) = m.riding
                    && let Some(&(chunk, ridden)) = horse_chunks.get(&vehicle)
                {
                    return if ridden || world.chunk_loaded(chunk) { None } else { Some(chunk) };
                }
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
        unload(&mut self.clouds, |e| away(e.pos), |chunk, e| storage.entry(chunk).clouds.push(e));
        unload(&mut self.potions, |e| away(e.pos), |chunk, e| storage.entry(chunk).potions.push(e));
        unload(&mut self.thrown, |e| away(e.pos), |chunk, e| storage.entry(chunk).thrown.push(e));
        unload(&mut self.eyes, |e| away(e.pos), |chunk, e| storage.entry(chunk).eyes.push(e));
        unload(&mut self.fireballs, |e| away(e.pos), |chunk, e| storage.entry(chunk).fireballs.push(e));
        unload(&mut self.tnt, |e| away(e.pos), |chunk, e| storage.entry(chunk).tnt.push(e));
        unload(&mut self.tridents, |e| away(e.pos), |chunk, e| storage.entry(chunk).tridents.push(e));
        // Hanging decorations belong to the cell they hang in.
        unload(
            &mut self.decor,
            |d| away(if d.is_hanging() { d.cell.as_dvec3() + DVec3::splat(0.5) } else { d.pos }),
            |chunk, d| storage.entry(chunk).decor.push(d),
        );
    }
}

/// v1.0 Track 3: entry counts for the long-session soak (`crate::soak`).
impl EntityStorage {
    pub(super) fn soak_counts(&self, out: &mut crate::world::diagnostics::Counts) {
        out.extend([
            ("stored.chunks", self.chunks.len()),
            ("stored.entities", self.len),
            ("stored.mobs", self.chunks.values().map(|c| c.mobs.len()).sum()),
            ("stored.items", self.chunks.values().map(|c| c.items.len()).sum()),
            ("stored.bed_claims", self.beds.len()),
            ("stored.job_claims", self.jobs.len()),
            ("stored.claim_cells", self.claim_cells.values().map(Vec::len).sum()),
            ("stored.disk_bed_claims", self.disk.beds.len()),
            ("stored.disk_job_claims", self.disk.jobs.len()),
            ("stored.disk_creakings", self.disk_creakings.len()),
        ]);
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
            m.equipment,
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
        // A raid's bell finds the stored claims within 48 blocks, and only those.
        assert!(e.storage.claimed_near(IVec3::new(20, GROUND + 1, 28), 48.0));
        assert!(!e.storage.claimed_near(IVec3::new(10, GROUND + 1, 28), 48.0));
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
        assert!(e.storage.claim_cells.is_empty());
        assert_eq!(
            e.items.len() + e.orbs.len() + e.minecarts.len() + e.arrows.len() + e.tnt.len() + after.len(),
            count
        );
    }

    /// v1.0 Track 3b: the same world, its unloaded chunks in region files.
    fn region_world(dir: &std::path::Path, columns: impl IntoIterator<Item = IVec2>) -> World {
        let _ = std::fs::remove_dir_all(dir);
        let mut w = world(columns);
        let (store, damage) = crate::world::region::RegionStore::open(dir, true).unwrap();
        assert!(damage.is_empty());
        w.attach_regions(store, Some(Default::default()), 0, true);
        w
    }

    #[test]
    fn with_region_storage_stored_entities_leave_memory_and_come_back_unchanged() {
        let dir = std::env::temp_dir().join(format!("voxelcraft-entity-regions-{}", std::process::id()));
        let columns = || (-1..=2).flat_map(|x| (-1..=1).map(move |z| IVec2::new(x, z)));
        let mut w = region_world(&dir, columns());
        let mut e = Entities::new(5);
        let count = populate(&mut e);
        e.spawn(MobKind::Cow, at(10.5, 10.5));
        e.ensure_mob_ids();
        e.sync_chunk_entities(&mut w);
        let before: Vec<String> = e.mobs.iter().filter(|m| m.pos.x >= 64.0).map(state).collect();
        let items = e.items.len() + e.orbs.len() + e.minecarts.len();

        for z in -1..=1 {
            w.unload_test_column(IVec2::new(2, z));
        }
        e.sync_chunk_entities(&mut w);
        // Arrows and primed TNT were never saved: they wait in memory.
        assert_eq!(e.stored_entities(), 2, "only the arrow and the primed TNT, never saved, stay in memory");
        assert_eq!(e.mobs.len(), 1);
        assert!(e.items.is_empty() && e.orbs.is_empty() && e.minecarts.is_empty());
        // The stored villager's claims outlast it in memory.
        assert!(e.storage.claims_bed(IVec3::new(67, GROUND + 1, 28)));
        assert!(e.storage.claims_job(IVec3::new(65, GROUND + 1, 24)));
        assert!(e.storage.claimed_near(IVec3::new(20, GROUND + 1, 28), 48.0));
        assert!(!e.stored_index_to_string().is_empty());
        w.flush_regions().unwrap();

        for z in -1..=1 {
            w.load_test_column(IVec2::new(2, z), GROUND);
        }
        e.sync_chunk_entities(&mut w);
        assert_eq!(e.stored_entities(), 0);
        let mut after: Vec<String> = e.mobs.iter().filter(|m| m.pos.x >= 64.0).map(state).collect();
        let mut want = before.clone();
        after.sort();
        want.sort();
        assert_eq!(after, want, "every mob as it was");
        assert_eq!(e.items.len() + e.orbs.len() + e.minecarts.len(), items);
        let live = e.mobs.len() + e.items.len() + e.orbs.len() + e.minecarts.len() + e.arrows.len() + e.tnt.len();
        assert_eq!(live, count + 1, "everything, and the cow that stayed");
        assert!(e.storage.disk.beds.is_empty() && e.storage.disk.jobs.is_empty(), "the villager has its claims again");
        assert_eq!(e.stored_index_to_string(), "");
        let pig = e.mobs.iter().find(|m| m.kind == MobKind::Pig).unwrap();
        assert!(e.minecarts.iter().any(|c| Some(c.id) == pig.riding), "the pig still rides its cart");
        drop(w);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Review fix S2: streaming goes on while the game is paused, so a save
    /// can come before any tick brought back a loaded chunk's entities.
    #[test]
    fn a_save_while_paused_keeps_the_entities_of_chunks_that_just_loaded() {
        let dir = std::env::temp_dir().join(format!("voxelcraft-entity-paused-{}", std::process::id()));
        let mut w = region_world(&dir, [IVec2::new(0, 0), IVec2::new(2, 0)]);
        let mut e = Entities::new(5);
        e.spawn(MobKind::Cow, at(70.5, 4.5));
        e.ensure_mob_ids();
        e.sync_chunk_entities(&mut w);
        w.unload_test_column(IVec2::new(2, 0));
        e.sync_chunk_entities(&mut w);
        assert!(e.mobs.is_empty());
        w.flush_regions().unwrap();
        // Paused: the column loads again, and the player saves.
        w.load_test_column(IVec2::new(2, 0), GROUND);
        let stored = e.take_stored_batches(&mut w);
        w.save_records(stored);
        let owned = World::parse_region_chunks(&w.region_chunks_to_string());
        assert!(owned.contains(&crate::world::chunk::chunk_of(IVec3::new(70, GROUND + 1, 4))));
        let mut props = BTreeMap::new();
        e.save_props(&mut props);
        let mut back = Entities::new(5);
        back.load_props(&props);
        assert_eq!(back.mobs.len(), 1, "the level file now holds the cow's chunk, and the cow");
        drop(w);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Review fix S6: a mob the level file holds walks into a chunk that
    /// unloads, its record is written, and the game stops before the next
    /// save. Reopened, the mob comes back once: the record's newer copy.
    #[test]
    fn an_interrupted_session_restores_each_saved_mob_once() {
        use crate::world::{GENERATION_KEY, REGION_KEY};
        let dir = std::env::temp_dir().join(format!("voxelcraft-entity-interrupted-{}", std::process::id()));
        let mut w = region_world(&dir, [IVec2::new(0, 0)]);
        let mut e = Entities::new(5);
        e.spawn(MobKind::Cow, at(10.5, 4.5));
        e.ensure_mob_ids();
        let uid = e.mobs[0].uid;
        e.sync_chunk_entities(&mut w);
        // The last complete save: the cow is in the level file.
        let stored = e.take_stored_batches(&mut w);
        w.save_records(stored);
        let mut props = BTreeMap::new();
        e.save_props(&mut props);
        props.insert(REGION_KEY.to_string(), w.region_chunks_to_string());
        props.insert(GENERATION_KEY.to_string(), w.save_generation().to_string());
        props.insert("entity_ids".to_string(), e.entity_ids_to_string());
        w.flush_regions().unwrap();
        // Then a column the level file doesn't hold loads, the cow walks
        // into it, and it unloads: its record gets the cow. The game stops
        // before saving again.
        w.load_test_column(IVec2::new(2, 0), GROUND);
        e.mobs[0].pos.x = 70.5;
        e.sync_chunk_entities(&mut w);
        w.unload_test_column(IVec2::new(2, 0));
        e.sync_chunk_entities(&mut w);
        assert!(e.mobs.is_empty() && e.stored_entities() == 0, "the cow is in the record");
        drop(w);

        // Reopened from that level file and the region files.
        let mut w = world([IVec2::new(0, 0)]);
        let (store, damage) = crate::world::region::RegionStore::open(&dir, true).unwrap();
        assert!(damage.is_empty());
        let owned = World::parse_region_chunks(&props[REGION_KEY]);
        w.attach_regions(store, Some(owned), props[GENERATION_KEY].parse().unwrap(), true);
        let mut e = Entities::new(5);
        e.load_props(&props);
        e.load_entity_ids(&props["entity_ids"]);
        assert_eq!(e.mobs.len(), 1, "the level file's copy");
        e.sync_chunk_entities(&mut w);
        w.load_test_column(IVec2::new(2, 0), GROUND);
        e.sync_chunk_entities(&mut w);
        let cows: Vec<&Mob> = e.mobs.iter().chain(e.storage.mobs()).filter(|m| m.uid == uid).collect();
        assert_eq!(cows.len(), 1, "restored once");
        assert_eq!(cows[0].pos.x, 70.5, "the record's copy, the newer one");
        assert!(w.save_generation() > props[GENERATION_KEY].parse::<u64>().unwrap());
        drop(w);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A complete save of `w` and `e` (the level file's entity keys).
    fn checkpoint(w: &mut World, e: &mut Entities) -> BTreeMap<String, String> {
        use crate::world::{GENERATION_KEY, REGION_KEY};
        let stored = e.take_stored_batches(w);
        w.save_records(stored);
        let mut props = BTreeMap::new();
        e.save_props(&mut props);
        props.insert(REGION_KEY.to_string(), w.region_chunks_to_string());
        props.insert(GENERATION_KEY.to_string(), w.save_generation().to_string());
        props.insert("entity_ids".to_string(), e.entity_ids_to_string());
        w.flush_regions().unwrap();
        props
    }

    /// Reopens `dir` from `props` with column 0 loaded, entities restored
    /// as `Game::restore_dimension` does.
    fn reopen(dir: &std::path::Path, props: &BTreeMap<String, String>) -> (World, Entities) {
        use crate::world::{GENERATION_KEY, REGION_KEY};
        let mut w = world([IVec2::new(0, 0)]);
        let (store, damage) = crate::world::region::RegionStore::open(dir, true).unwrap();
        assert!(damage.is_empty());
        let owned = World::parse_region_chunks(&props[REGION_KEY]);
        w.attach_regions(store, Some(owned), props[GENERATION_KEY].parse().unwrap(), true);
        let mut e = Entities::new(5);
        e.recover_ids(&mut w);
        e.load_props(props);
        e.load_entity_ids(&props["entity_ids"]);
        (w, e)
    }

    /// Review fix S6, round 2: a mob made after the last save and stored in
    /// a record keeps its uid to itself when the session that made it
    /// stopped before saving again.
    #[test]
    fn recovery_reserves_the_ids_of_newer_records() {
        let dir = std::env::temp_dir().join(format!("voxelcraft-entity-recover-ids-{}", std::process::id()));
        let mut w = region_world(&dir, [IVec2::new(0, 0)]);
        let mut e = Entities::new(5);
        e.spawn(MobKind::Cow, at(10.5, 4.5));
        e.ensure_mob_ids();
        e.sync_chunk_entities(&mut w);
        let props = checkpoint(&mut w, &mut e);
        // After the save: a pig and a cart in a column that then unloads.
        w.load_test_column(IVec2::new(2, 0), GROUND);
        e.spawn(MobKind::Pig, at(70.5, 4.5));
        e.ensure_mob_ids();
        let cart = e.spawn_cart(CartKind::Rideable, at(72.5, 4.5));
        let pig = e.mobs.last().unwrap().uid;
        e.sync_chunk_entities(&mut w);
        w.unload_test_column(IVec2::new(2, 0));
        e.sync_chunk_entities(&mut w);
        drop(w);

        let (mut w, mut e) = reopen(&dir, &props);
        e.sync_chunk_entities(&mut w);
        e.spawn(MobKind::Sheep, at(12.5, 4.5));
        e.ensure_mob_ids();
        let new_cart = e.spawn_cart(CartKind::Rideable, at(14.5, 4.5));
        assert_ne!(e.mobs.last().unwrap().uid, pig, "a new mob doesn't take the stored pig's uid");
        assert_ne!(new_cart, cart, "nor a new cart the stored cart's id");
        w.load_test_column(IVec2::new(2, 0), GROUND);
        e.sync_chunk_entities(&mut w);
        let mut uids: Vec<u32> = e.mobs.iter().map(|m| m.uid).collect();
        uids.sort_unstable();
        uids.dedup();
        assert_eq!(uids.len(), 3, "the cow, the pig and the sheep, each its own uid");
        drop(w);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Review fix S7: a mob saved without a uid in a newer record matches
    /// nothing in the level file (loading would hand it a fresh uid, which
    /// another mob may already have).
    #[test]
    fn recovery_matches_only_saved_uids() {
        let dir = std::env::temp_dir().join(format!("voxelcraft-entity-recover-zero-{}", std::process::id()));
        let mut w = region_world(&dir, [IVec2::new(0, 0)]);
        let mut e = Entities::new(5);
        e.spawn(MobKind::Cow, at(10.5, 4.5));
        e.ensure_mob_ids();
        let cow = e.mobs[0].uid;
        e.sync_chunk_entities(&mut w);
        let props = checkpoint(&mut w, &mut e);
        // A pig that never got a uid, stored after the save.
        w.load_test_column(IVec2::new(2, 0), GROUND);
        e.spawn(MobKind::Pig, at(70.5, 4.5));
        e.mobs.last_mut().unwrap().uid = 0;
        e.sync_chunk_entities(&mut w);
        w.unload_test_column(IVec2::new(2, 0));
        e.sync_chunk_entities(&mut w);
        drop(w);

        let (mut w, mut e) = reopen(&dir, &props);
        e.sync_chunk_entities(&mut w);
        assert!(e.mobs.iter().any(|m| m.uid == cow && m.kind == MobKind::Cow), "the level file's cow stays");
        w.load_test_column(IVec2::new(2, 0), GROUND);
        e.sync_chunk_entities(&mut w);
        let kinds: Vec<MobKind> = e.mobs.iter().map(|m| m.kind).collect();
        assert_eq!(kinds.len(), 2, "{kinds:?}");
        assert!(kinds.contains(&MobKind::Cow) && kinds.contains(&MobKind::Pig));
        drop(w);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_reload_keeps_the_stored_index_and_the_next_ids() {
        let mut e = Entities::new(5);
        e.storage.disk.add(Some(IVec3::new(1, 2, 3)), Some(IVec3::new(4, 5, 6)));
        e.storage.disk.add(Some(IVec3::new(1, 2, 3)), None);
        e.storage.disk_creakings.insert(77);
        e.next_uid = 900;
        e.next_cart = 40;
        let (index, ids) = (e.stored_index_to_string(), e.entity_ids_to_string());
        let mut f = Entities::new(5);
        f.load_stored_index(&index);
        f.load_entity_ids(&ids);
        assert_eq!(f.stored_index_to_string(), index);
        assert_eq!(f.storage.disk.beds[&IVec3::new(1, 2, 3)], 2);
        assert!(f.storage.holds_mob(77));
        assert_eq!((f.next_uid, f.next_cart), (900, 40));
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
