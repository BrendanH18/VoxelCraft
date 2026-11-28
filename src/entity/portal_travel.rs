//! v1.0 final Track F2: entities through Nether portals, End portals and End
//! gateways, after Java 26.3's `Entity.setAsInsidePortal`, `handlePortal`,
//! `teleport` and `teleportCrossDimension`.
//!
//! In Java a block's `entityInside` marks an entity that can use portals
//! (`canUsePortal(false)`: not a passenger, alive, and for living entities
//! not asleep; never the Ender Dragon or the Wither, nor a heart-bound
//! creaking or a fishing hook) inside the portal, or restarts its cooldown
//! if it is on one. The next tick's `handlePortal` counts the cooldown down
//! and sends a marked entity on its way: for anything but a player the
//! Nether portal's `getPortalTransitionTime` is 0, so it goes at once, and
//! `setPortalCooldown` gives it `getDimensionChangingDelay` (300 ticks, or
//! the player's 10 for a vehicle a player rides first). A vehicle takes its
//! passengers with it (`teleportCrossDimension` teleports each, then they
//! start riding again).
//!
//! Only the host's dimension runs here, so an entity bound for another
//! dimension leaves this one as a batch of the level file's entity keys
//! (the same form a chunk record stores) and waits with its trip until
//! that dimension next loads; the game then makes the arrival Java made at
//! the time of the trip (the other dimension hasn't changed meanwhile) and
//! puts it in the level there. See `app::portal_travel`.

use glam::{DVec3, IVec3};
use rustc_hash::FxHashMap;

use super::{Entities, MobKind, PlayerId, mounts::MOB_VEHICLE};
use crate::world::World;
use crate::world::block::Block;
use crate::world::portal::PortalKind;
use crate::world::region::Props;

/// `Entity.getDimensionChangingDelay` for an entity no player rides.
pub const ENTITY_PORTAL_COOLDOWN: u16 = 300;

/// An entity on its way through a portal: taken out of the level with its
/// passengers, as level file entity keys.
#[derive(Clone, Debug, PartialEq)]
pub struct Departure {
    pub kind: PortalKind,
    /// The portal block it was marked inside (`getEntryPosition`).
    pub entry: IVec3,
    /// The root entity's position (bottom centre), motion and yaw.
    pub pos: DVec3,
    pub vel: DVec3,
    pub yaw: f32,
    /// Java's `EntityDimensions` of the root entity.
    pub width: f64,
    pub height: f64,
    pub batch: Props,
}

/// A Nether trip the `allow_entering_nether_using_portals` rule refused:
/// the entity stays (on cooldown), but the exit portal is still owed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Refused {
    pub entry: IVec3,
    pub pos: DVec3,
}

/// The portal block a box from `min` to `max` is inside (`entityInside`):
/// an End gateway or a Nether portal anywhere in its cell, an End portal
/// only in its slab from 6 to 12 sixteenths up (Java's shape).
pub fn touched_portal(world: &World, min: DVec3, max: DVec3) -> Option<(PortalKind, IVec3)> {
    let (lo, hi) = (min.floor().as_ivec3(), (max - DVec3::splat(1e-6)).floor().as_ivec3());
    for y in lo.y..=hi.y {
        for z in lo.z..=hi.z {
            for x in lo.x..=hi.x {
                let p = IVec3::new(x, y, z);
                let Some(b) = world.get_block(p) else { continue };
                if b == Block::END_GATEWAY {
                    return Some((PortalKind::Gateway, p));
                }
                if b == Block::END_PORTAL && min.y < y as f64 + 0.75 && max.y > y as f64 + 6.0 / 16.0 {
                    return Some((PortalKind::End, p));
                }
                if b.is_nether_portal() {
                    return Some((PortalKind::Nether, p));
                }
            }
        }
    }
    None
}

/// One entity's portal step, at the start of its tick: the portal its box
/// was inside after last tick's move marks it (or restarts its cooldown),
/// the cooldown counts down (`processPortalCooldown`), and a marked entity
/// goes (`processPortalTeleportation` with a transition time of 0), its
/// cooldown set again. `open` is the End gateway's own check (not cooling
/// down; it starts its cooldown).
fn step(
    world: &World,
    (min, max): (DVec3, DVec3),
    allowed: bool,
    cooldown: &mut u16,
    open: &mut impl FnMut(IVec3) -> bool,
) -> Option<(PortalKind, IVec3)> {
    let mut go = None;
    if allowed
        && let Some((kind, cell)) = touched_portal(world, min, max)
        && (kind != PortalKind::Gateway || open(cell))
    {
        if *cooldown > 0 {
            *cooldown = ENTITY_PORTAL_COOLDOWN;
        } else {
            go = Some((kind, cell));
        }
    }
    *cooldown = cooldown.saturating_sub(1);
    if go.is_some() {
        *cooldown = ENTITY_PORTAL_COOLDOWN;
    }
    go
}

/// Java's `EntityDimensions` (floats) of a box.
fn size((min, max): (DVec3, DVec3)) -> (f64, f64) {
    (f64::from((max.x - min.x) as f32), f64::from((max.y - min.y) as f32))
}

/// What a departing root entity is, by list and index.
#[derive(Clone, Copy)]
enum Root {
    Mob(usize),
    Cart(usize),
    Item(usize),
    Orb(usize),
    Arrow(usize),
    Trident(usize),
    Thrown(usize),
    Potion(usize),
}

impl Entities {
    /// Whether a mob can use a portal (`canUsePortal(false)`).
    fn mob_uses_portals(&self, i: usize) -> bool {
        let m = &self.mobs[i];
        let ridden = m
            .mount
            .as_ref()
            .is_some_and(|s| s.rider.is_some() || s.passenger.is_some() || s.seats.iter().any(Option::is_some));
        m.alive()
            && m.riding.is_none()
            && !ridden
            && !m.animal.as_ref().is_some_and(|a| a.perched.is_some())
            && !m.villager.as_ref().is_some_and(|v| v.sleeping)
            && !matches!(m.kind, MobKind::Wither)
            && !m.creaking.as_ref().is_some_and(|c| c.home.is_some())
    }

    /// v1.0 final Track F2: every entity's portal step this tick. Entities
    /// that go leave the level (with their passengers) as [`Departure`]s;
    /// `nether_allowed` is the `allow_entering_nether_using_portals` rule
    /// for a trip from here, and a refused trip leaves the entity where it
    /// is, on cooldown, and is listed as [`Refused`]. `open` is the End
    /// gateway check. The player's vehicle is the player's to take.
    pub fn portal_departures(
        &mut self,
        world: &World,
        nether_allowed: bool,
        mut open: impl FnMut(IVec3) -> bool,
    ) -> (Vec<Departure>, Vec<Refused>) {
        let mut roots: Vec<(Root, PortalKind, IVec3)> = Vec::new();
        for i in 0..self.mobs.len() {
            let allowed = self.mob_uses_portals(i);
            let m = &mut self.mobs[i];
            let aabb = m.aabb();
            if let Some((kind, cell)) = step(world, aabb, allowed, &mut m.portal_cooldown, &mut open) {
                roots.push((Root::Mob(i), kind, cell));
            }
        }
        for (i, c) in self.minecarts.iter_mut().enumerate() {
            let allowed = c.rider.is_none() && c.second_rider.is_none() && c.kind.cushion().is_none();
            if let Some((kind, cell)) = step(world, c.aabb(), allowed, &mut c.portal_cooldown, &mut open) {
                roots.push((Root::Cart(i), kind, cell));
            }
        }
        for (i, e) in self.items.iter_mut().enumerate() {
            if let Some((kind, cell)) = step(world, e.aabb(), true, &mut e.portal_cooldown, &mut open) {
                roots.push((Root::Item(i), kind, cell));
            }
        }
        for (i, e) in self.orbs.iter_mut().enumerate() {
            let aabb = (e.pos - DVec3::new(0.25, 0.0, 0.25), e.pos + DVec3::new(0.25, 0.5, 0.25));
            if let Some((kind, cell)) = step(world, aabb, true, &mut e.portal_cooldown, &mut open) {
                roots.push((Root::Orb(i), kind, cell));
            }
        }
        for (i, e) in self.arrows.iter_mut().enumerate() {
            let aabb = (e.pos - DVec3::new(0.25, 0.0, 0.25), e.pos + DVec3::new(0.25, 0.5, 0.25));
            if let Some((kind, cell)) = step(world, aabb, true, &mut e.portal_cooldown, &mut open) {
                roots.push((Root::Arrow(i), kind, cell));
            }
        }
        for (i, e) in self.tridents.iter_mut().enumerate() {
            let aabb = (e.pos - DVec3::new(0.25, 0.0, 0.25), e.pos + DVec3::new(0.25, 0.5, 0.25));
            if let Some((kind, cell)) = step(world, aabb, true, &mut e.portal_cooldown, &mut open) {
                roots.push((Root::Trident(i), kind, cell));
            }
        }
        for (i, e) in self.thrown.iter_mut().enumerate() {
            let aabb = (e.pos - DVec3::new(0.125, 0.0, 0.125), e.pos + DVec3::new(0.125, 0.25, 0.125));
            if let Some((kind, cell)) = step(world, aabb, true, &mut e.portal_cooldown, &mut open) {
                roots.push((Root::Thrown(i), kind, cell));
            }
        }
        for (i, e) in self.potions.iter_mut().enumerate() {
            let aabb = (e.pos - DVec3::new(0.125, 0.0, 0.125), e.pos + DVec3::new(0.125, 0.25, 0.125));
            if let Some((kind, cell)) = step(world, aabb, true, &mut e.portal_cooldown, &mut open) {
                roots.push((Root::Potion(i), kind, cell));
            }
        }
        if roots.is_empty() {
            return Default::default();
        }
        let mut refused = Vec::new();
        roots.retain(|&(root, kind, entry)| {
            if kind == PortalKind::Nether && !nether_allowed {
                refused.push(Refused { entry, pos: self.root_pos(root) });
                return false;
            }
            true
        });
        let departures = self.take_roots(roots);
        (departures, refused)
    }

    fn root_pos(&self, root: Root) -> DVec3 {
        match root {
            Root::Mob(i) => self.mobs[i].pos,
            Root::Cart(i) => self.minecarts[i].pos,
            Root::Item(i) => self.items[i].pos,
            Root::Orb(i) => self.orbs[i].pos,
            Root::Arrow(i) => self.arrows[i].pos,
            Root::Trident(i) => self.tridents[i].pos,
            Root::Thrown(i) => self.thrown[i].pos,
            Root::Potion(i) => self.potions[i].pos,
        }
    }

    /// v1.0 final Track F2: takes the vehicle `id` (a cart, boat or mount)
    /// the host rides out of the level with its other passengers, for the
    /// host's own trip. `None` if there's no such vehicle.
    pub fn take_vehicle(&mut self, id: u32) -> Option<Departure> {
        let root = if id & MOB_VEHICLE != 0 {
            Root::Mob(self.mobs.iter().position(|m| m.uid == id & !MOB_VEHICLE)?)
        } else {
            Root::Cart(self.minecarts.iter().position(|c| c.id == id)?)
        };
        self.take_roots(vec![(root, PortalKind::Nether, IVec3::ZERO)]).pop()
    }

    /// Takes each root (and the mobs riding it) out of the level as a batch.
    fn take_roots(&mut self, roots: Vec<(Root, PortalKind, IVec3)>) -> Vec<Departure> {
        let mut empty = std::collections::BTreeMap::new();
        Entities::new(0).save_props(&mut empty);
        let small = |pos: DVec3, r: f64| (pos - DVec3::new(r, 0.0, r), pos + DVec3::new(r, 2.0 * r, r));
        // What each root is, and the mobs riding it (`getPassengers`).
        let mut plans = Vec::with_capacity(roots.len());
        let mut riders_taken = rustc_hash::FxHashSet::default();
        for &(root, kind, entry) in &roots {
            let (pos, vel, yaw, aabb, vehicle) = match root {
                Root::Mob(i) => {
                    let m = &self.mobs[i];
                    (m.pos, m.vel, m.yaw, m.aabb(), (m.uid != 0).then_some(MOB_VEHICLE | m.uid))
                }
                Root::Cart(i) => {
                    let c = &self.minecarts[i];
                    (c.pos, c.vel, c.yaw, c.aabb(), Some(c.id))
                }
                Root::Item(i) => (self.items[i].pos, self.items[i].vel, 0.0, self.items[i].aabb(), None),
                Root::Orb(i) => (self.orbs[i].pos, self.orbs[i].vel, 0.0, small(self.orbs[i].pos, 0.25), None),
                Root::Arrow(i) => (self.arrows[i].pos, self.arrows[i].vel, 0.0, small(self.arrows[i].pos, 0.25), None),
                Root::Trident(i) => {
                    (self.tridents[i].pos, self.tridents[i].vel, 0.0, small(self.tridents[i].pos, 0.25), None)
                }
                Root::Thrown(i) => {
                    (self.thrown[i].pos, self.thrown[i].vel, 0.0, small(self.thrown[i].pos, 0.125), None)
                }
                Root::Potion(i) => {
                    (self.potions[i].pos, self.potions[i].vel, 0.0, small(self.potions[i].pos, 0.125), None)
                }
            };
            // `teleportCrossDimension`: the passengers go with their vehicle.
            let riders: Vec<usize> = vehicle
                .map(|v| {
                    (0..self.mobs.len()).filter(|&j| self.mobs[j].riding == Some(v) && riders_taken.insert(j)).collect()
                })
                .unwrap_or_default();
            let (width, height) = size(aabb);
            plans.push((root, riders, Departure { kind, entry, pos, vel, yaw, width, height, batch: Vec::new() }));
        }
        let mut mobs: Vec<Option<_>> = self.mobs.drain(..).map(Some).collect();
        let mut carts: Vec<Option<_>> = self.minecarts.drain(..).map(Some).collect();
        let mut items: Vec<Option<_>> = self.items.drain(..).map(Some).collect();
        let mut orbs: Vec<Option<_>> = self.orbs.drain(..).map(Some).collect();
        let mut arrows: Vec<Option<_>> = self.arrows.drain(..).map(Some).collect();
        let mut tridents: Vec<Option<_>> = self.tridents.drain(..).map(Some).collect();
        let mut thrown: Vec<Option<_>> = self.thrown.drain(..).map(Some).collect();
        let mut potions: Vec<Option<_>> = self.potions.drain(..).map(Some).collect();
        let mut out = Vec::with_capacity(plans.len());
        for (root, riders, mut departure) in plans {
            let mut scratch = Entities::new(0);
            match root {
                Root::Mob(i) => scratch.mobs.extend(mobs[i].take()),
                Root::Cart(i) => scratch.minecarts.extend(carts[i].take()),
                Root::Item(i) => scratch.items.extend(items[i].take()),
                Root::Orb(i) => scratch.orbs.extend(orbs[i].take()),
                Root::Arrow(i) => scratch.arrows.extend(arrows[i].take()),
                Root::Trident(i) => scratch.tridents.extend(tridents[i].take()),
                Root::Thrown(i) => scratch.thrown.extend(thrown[i].take()),
                Root::Potion(i) => scratch.potions.extend(potions[i].take()),
            }
            for j in riders {
                scratch.mobs.extend(mobs[j].take());
            }
            // `removeAfterChangingDimensions`: `Leashable.removeLeash`, which
            // drops no lead.
            for m in &mut scratch.mobs {
                m.leash = None;
                m.leash_pos = None;
            }
            let mut props = std::collections::BTreeMap::new();
            scratch.save_props(&mut props);
            departure.batch = props.into_iter().filter(|(k, v)| empty.get(k) != Some(v)).collect();
            out.push(departure);
        }
        self.mobs = mobs.into_iter().flatten().collect();
        self.minecarts = carts.into_iter().flatten().collect();
        self.items = items.into_iter().flatten().collect();
        self.orbs = orbs.into_iter().flatten().collect();
        self.arrows = arrows.into_iter().flatten().collect();
        self.tridents = tridents.into_iter().flatten().collect();
        self.thrown = thrown.into_iter().flatten().collect();
        self.potions = potions.into_iter().flatten().collect();
        out
    }

    /// v1.0 final Track F2: a traveller arrives (`addDuringTeleport`): the
    /// batch's root entity stands at `pos` with its passengers around it as
    /// they were, its motion `vel` (kept when `None`), turned by `turn`
    /// radians, on its portal cooldown. Mobs and carts take this level's
    /// next ids, and villagers forget their bed and job site, which were
    /// in the other dimension. A player riding the root stays aboard only
    /// with `keep_riders`. Returns the root's vehicle id, if it is a cart,
    /// boat or mount.
    pub fn arrive(
        &mut self,
        batch: &Props,
        pos: DVec3,
        vel: Option<DVec3>,
        turn: f32,
        keep_riders: bool,
    ) -> Option<u32> {
        let mut scratch = Entities::new(0);
        scratch.load_props(&batch.iter().cloned().collect());
        let root_pos = scratch
            .minecarts
            .first()
            .map(|c| c.pos)
            .or_else(|| scratch.mobs.iter().find(|m| m.riding.is_none()).map(|m| m.pos))
            .or_else(|| scratch.items.first().map(|e| e.pos))
            .or_else(|| scratch.orbs.first().map(|e| e.pos))
            .or_else(|| scratch.arrows.first().map(|e| e.pos))
            .or_else(|| scratch.tridents.first().map(|e| e.pos))
            .or_else(|| scratch.thrown.first().map(|e| e.pos))
            .or_else(|| scratch.potions.first().map(|e| e.pos))?;
        let delta = pos - root_pos;
        let turned = |v: DVec3| {
            let (s, c) = (-turn as f64).sin_cos();
            DVec3::new(v.x * c - v.z * s, v.y, v.x * s + v.z * c)
        };
        let mut carts: FxHashMap<u32, u32> = FxHashMap::default();
        let mut uids: FxHashMap<u32, u32> = FxHashMap::default();
        let mut vehicle = None;
        for c in &mut scratch.minecarts {
            let id = self.next_cart;
            self.next_cart = self.next_cart.saturating_add(1);
            carts.insert(c.id, id);
            c.id = id;
            c.pos += delta;
            c.previous_pos = c.pos;
            c.vel = vel.unwrap_or_else(|| turned(c.vel));
            c.yaw = (c.yaw + turn).rem_euclid(std::f32::consts::TAU);
            c.portal_cooldown = ENTITY_PORTAL_COOLDOWN;
            if !keep_riders {
                c.rider = None;
                c.second_rider = None;
            }
            vehicle = Some(id);
        }
        for m in &mut scratch.mobs {
            let uid = self.next_uid;
            self.next_uid = self.next_uid.saturating_add(1);
            if m.uid != 0 {
                uids.insert(m.uid, uid);
            }
            m.uid = uid;
            m.pos += delta;
            m.yaw = (m.yaw + turn).rem_euclid(std::f32::consts::TAU);
            if let Some(v) = &mut m.villager {
                v.home = None;
                v.job = None;
                v.id = self.next_villager_id;
                self.next_villager_id = self.next_villager_id.saturating_add(1);
            }
            if !keep_riders && let Some(s) = &mut m.mount {
                s.rider = None;
                s.passenger = None;
                s.seats.iter_mut().for_each(|seat| *seat = None);
            }
        }
        for m in &mut scratch.mobs {
            match m.riding {
                Some(r) if r & MOB_VEHICLE != 0 => m.riding = uids.get(&(r & !MOB_VEHICLE)).map(|&u| MOB_VEHICLE | u),
                Some(r) => m.riding = carts.get(&r).copied(),
                None => {
                    m.vel = vel.unwrap_or_else(|| turned(m.vel));
                    m.portal_cooldown = ENTITY_PORTAL_COOLDOWN;
                    if m.mount.is_some() && vehicle.is_none() {
                        vehicle = Some(MOB_VEHICLE | m.uid);
                    }
                }
            }
        }
        for e in &mut scratch.items {
            e.pos += delta;
            e.previous_pos = e.pos;
            e.vel = vel.unwrap_or_else(|| turned(e.vel));
            e.portal_cooldown = ENTITY_PORTAL_COOLDOWN;
        }
        for e in &mut scratch.orbs {
            e.pos += delta;
            e.previous_pos = e.pos;
            e.vel = vel.unwrap_or_else(|| turned(e.vel));
            e.portal_cooldown = ENTITY_PORTAL_COOLDOWN;
        }
        for e in &mut scratch.arrows {
            e.pos += delta;
            e.previous_pos = e.pos;
            e.vel = vel.unwrap_or_else(|| turned(e.vel));
            e.portal_cooldown = ENTITY_PORTAL_COOLDOWN;
        }
        for e in &mut scratch.tridents {
            e.pos += delta;
            e.previous_pos = e.pos;
            e.vel = vel.unwrap_or_else(|| turned(e.vel));
            e.portal_cooldown = ENTITY_PORTAL_COOLDOWN;
        }
        for e in &mut scratch.thrown {
            e.pos += delta;
            e.previous_pos = e.pos;
            e.vel = vel.unwrap_or_else(|| turned(e.vel));
            e.portal_cooldown = ENTITY_PORTAL_COOLDOWN;
        }
        for e in &mut scratch.potions {
            e.pos += delta;
            e.previous_pos = e.pos;
            e.vel = vel.unwrap_or_else(|| turned(e.vel));
            e.portal_cooldown = ENTITY_PORTAL_COOLDOWN;
        }
        self.mobs.append(&mut scratch.mobs);
        self.minecarts.append(&mut scratch.minecarts);
        self.items.append(&mut scratch.items);
        self.orbs.append(&mut scratch.orbs);
        self.arrows.append(&mut scratch.arrows);
        self.tridents.append(&mut scratch.tridents);
        self.thrown.append(&mut scratch.thrown);
        self.potions.append(&mut scratch.potions);
        vehicle
    }

    /// The box of the vehicle `id` (a cart, boat or mount).
    pub fn vehicle_aabb(&self, id: u32) -> Option<(DVec3, DVec3)> {
        if id & MOB_VEHICLE != 0 { self.mount(id).map(|m| m.aabb()) } else { self.cart(id).map(|c| c.aabb()) }
    }

    /// The motion of the vehicle `id` (a cart, boat or mount).
    pub fn vehicle_vel(&self, id: u32) -> Option<DVec3> {
        if id & MOB_VEHICLE != 0 { self.mount(id).map(|m| m.vel) } else { self.cart(id).map(|c| c.vel) }
    }

    /// A player is aboard vehicle `id` (only the host's vehicle travels with
    /// its rider; another player's stays).
    pub fn vehicle_has_other_riders(&self, id: u32, host: PlayerId) -> bool {
        let other = |p: Option<PlayerId>| p.is_some_and(|p| p != host);
        if id & MOB_VEHICLE != 0 {
            self.mount(id)
                .and_then(|m| m.mount.as_ref())
                .is_some_and(|s| other(s.rider) || other(s.passenger) || s.seats.iter().any(|&p| other(p)))
        } else {
            self.cart(id).is_some_and(|c| other(c.rider) || other(c.second_rider))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::Stack;
    use crate::item::Item;

    #[test]
    fn a_departure_arrives_with_new_ids_and_its_cooldown() {
        let mut from = Entities::new(1);
        from.spawn(MobKind::Pig, DVec3::new(0.5, 70.0, 0.5));
        from.ensure_mob_ids();
        let cart = from.spawn_cart(super::super::minecart::CartKind::Rideable, DVec3::new(4.5, 70.0, 0.5));
        from.mobs[0].riding = Some(cart);
        from.drop_from_block(Stack::new(Item::DIAMOND, 3), IVec3::new(8, 70, 0));
        let i = from.minecarts.iter().position(|c| c.id == cart).unwrap();
        let departures = from.take_roots(vec![(Root::Cart(i), PortalKind::Nether, IVec3::new(4, 70, 0))]);
        assert_eq!(departures.len(), 1);
        assert!(from.minecarts.is_empty() && from.mobs.is_empty(), "the cart takes its passenger");
        assert_eq!(from.items.len(), 1, "the item stays");
        let mut to = Entities::new(2);
        to.spawn(MobKind::Cow, DVec3::new(0.5, 70.0, 0.5));
        to.ensure_mob_ids();
        let cow = to.mobs[0].uid;
        let at = DVec3::new(100.5, 40.0, -7.5);
        let id = to.arrive(&departures[0].batch, at, None, 0.0, false).expect("a cart");
        let c = to.cart(id).unwrap();
        assert_eq!((c.pos, c.portal_cooldown), (at, ENTITY_PORTAL_COOLDOWN));
        let pig = to.mobs.iter().find(|m| m.kind == MobKind::Pig).unwrap();
        assert_ne!(pig.uid, cow, "the pig takes a new uid here");
        assert_eq!(pig.riding, Some(id), "and still rides its cart");
        assert!((pig.pos - DVec3::new(96.5, 40.0, -7.5)).length() < 1e-9, "beside it as before");
    }

    fn world() -> World {
        let mut w =
            World::new_headless(std::sync::Arc::new(crate::world::terrain::Generator::new(3)), Default::default(), 2);
        w.load_test_column(glam::IVec2::ZERO, 64);
        w
    }

    #[test]
    fn mobs_and_items_go_through_a_portal_on_the_next_tick_then_cool_down() {
        let mut w = world();
        let portal = IVec3::new(4, 65, 4);
        w.set_block(portal, Block::NETHER_PORTAL);
        let mut e = Entities::new(1);
        e.spawn(MobKind::Cow, DVec3::new(4.5, 65.0, 4.5));
        e.drop_from_block(Stack::new(Item::DIAMOND, 1), portal);
        e.items[0].pos = DVec3::new(4.5, 65.0, 4.5);
        e.ensure_mob_ids();
        let (gone, refused) = e.portal_departures(&w, false, |_| true);
        assert!(gone.is_empty(), "the rule refuses the trip into the Nether");
        assert_eq!(refused.len(), 2);
        assert_eq!(e.mobs[0].portal_cooldown, ENTITY_PORTAL_COOLDOWN, "set again as it is refused");
        // On cooldown, standing inside only restarts it.
        let (gone, _) = e.portal_departures(&w, true, |_| true);
        assert!(gone.is_empty());
        assert_eq!(e.mobs[0].portal_cooldown, ENTITY_PORTAL_COOLDOWN - 1);
        e.mobs[0].portal_cooldown = 0;
        e.items[0].portal_cooldown = 0;
        let (gone, _) = e.portal_departures(&w, true, |_| true);
        assert_eq!(gone.len(), 2);
        assert!(gone.iter().all(|d| d.kind == PortalKind::Nether && d.entry == portal));
        assert!(e.mobs.is_empty() && e.items.is_empty());
        // The Wither, a mob riding and a sleeping villager never use portals.
        e.spawn(MobKind::Wither, DVec3::new(4.5, 65.0, 4.5));
        assert!(!e.mob_uses_portals(0));
    }

    #[test]
    fn the_portal_step_counts_its_cooldown_down_outside() {
        let w = world();
        let mut cd = 5u16;
        let b = (DVec3::new(0.2, 66.0, 0.2), DVec3::new(0.8, 67.0, 0.8));
        assert_eq!(step(&w, b, true, &mut cd, &mut |_| true), None);
        assert_eq!(cd, 4);
    }

    #[test]
    fn the_hosts_mount_keeps_its_rider_only_with_the_host() {
        let mut e = Entities::new(1);
        e.spawn(MobKind::Horse, DVec3::new(0.5, 70.0, 0.5));
        e.ensure_mob_ids();
        let s = e.mobs[0].mount.as_mut().unwrap();
        s.tame = true;
        s.rider = Some(PlayerId::HOST);
        let id = MOB_VEHICLE | e.mobs[0].uid;
        let v = e.take_vehicle(id).expect("the horse");
        assert!(e.mobs.is_empty());
        assert!(v.width > 1.0 && v.height > 1.0, "the horse's size: {} x {}", v.width, v.height);
        let mut to = Entities::new(2);
        let at = DVec3::new(10.5, 64.0, 10.5);
        let new_id = to.arrive(&v.batch, at, Some(DVec3::ZERO), 0.0, true).expect("a mount");
        assert_eq!(to.mount_for_player(PlayerId::HOST), Some(new_id), "the host rides it again");
        assert_eq!(to.mobs[0].pos, at);
        // Arriving on its own, nobody rides it.
        let mut alone = Entities::new(3);
        alone.arrive(&v.batch, at, None, 0.0, false);
        assert_eq!(alone.mount_for_player(PlayerId::HOST), None);
    }
}
