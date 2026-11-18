//! Java 26.3 wandering trader attempts (`WanderingTraderSpawner`), stock,
//! potions and linked llama lifetime.
use super::villager::Villager;
use super::{Ctx, Entities, Mob, MobKind, MobWorld};
use crate::{
    physics,
    world::{block::Block, terrain::Dimension},
};
use glam::{DVec3, IVec3};

/// Java's Invisibility potion: 3600 ticks.
const HIDE_TIME: f32 = 180.0;
#[derive(Clone, Copy)]
pub struct Trader {
    pub despawn: f32,
    pub invisible: bool,
    /// Seconds of the potion's Invisibility left (Java: 3600 ticks); expiry makes it drink again.
    pub hide_left: f32,
    pub drink: f32,
    pub leader: u64,
    pub destination: DVec3,
}
impl Trader {
    pub(super) fn save(&self) -> serde_json::Value {
        serde_json::json!([
            self.despawn,
            self.invisible,
            self.drink,
            self.leader,
            self.destination.to_array(),
            self.hide_left
        ])
    }
    pub(super) fn load(v: &serde_json::Value, pos: DVec3) -> Option<Self> {
        let mut t = Self::new(pos);
        t.despawn = (v[0].as_f64()? as f32).clamp(0.0, 2400.0);
        t.invisible = v[1].as_bool()?;
        t.drink = (v[2].as_f64()? as f32).clamp(0.0, 1.6);
        t.leader = v[3].as_u64()?;
        let p = v[4].as_array()?;
        if p.len() != 3 {
            return None;
        }
        t.destination = DVec3::new(p[0].as_f64()?, p[1].as_f64()?, p[2].as_f64()?);
        // Older saves hold no timer: an invisible trader gets a fresh potion's worth.
        t.hide_left =
            if t.invisible { v[5].as_f64().map_or(HIDE_TIME, |s| (s as f32).clamp(0.0, HIDE_TIME)) } else { 0.0 };
        t.destination.is_finite().then_some(t)
    }
    pub fn new(pos: DVec3) -> Self {
        Self { despawn: 2400.0, invisible: false, hide_left: 0.0, drink: 0.0, leader: 0, destination: pos }
    }
}
/// `WanderingTraderSpawner`: every `DEFAULT_TICK_DELAY` (1200) ticks it
/// counts down a `DEFAULT_SPAWN_DELAY`-tick wait, here counted in seconds
/// as one span; the chance starts at 25, rises by 25 to 75 per failed day
/// and resets on a spawn; a spawn also needs one in ten, and takes ten
/// tries for a spot.
pub(super) const SPAWN_DELAY: u32 = 24000;
pub(super) const MIN_CHANCE: u8 = 25;
pub(super) const MAX_CHANCE: u8 = 75;
pub(super) const CHANCE_STEP: u8 = 25;
pub(super) const ONE_IN: u32 = 10;
pub(super) const SPAWN_ATTEMPTS: u32 = 10;
#[derive(Clone, Copy)]
pub(super) struct Spawner {
    pub delay: f32,
    pub chance: u8,
}
impl Default for Spawner {
    fn default() -> Self {
        Self { delay: SPAWN_DELAY as f32 / 20.0, chance: MIN_CHANCE }
    }
}
impl Spawner {
    fn attempt(&mut self, dt: f32) -> Option<u8> {
        self.delay -= dt;
        if self.delay > 0.0 {
            return None;
        }
        self.delay += SPAWN_DELAY as f32 / 20.0;
        let chance = self.chance;
        self.chance = (self.chance + CHANCE_STEP).clamp(MIN_CHANCE, MAX_CHANCE);
        Some(chance)
    }
}
impl Villager {
    /// Java 26.3 `WanderingTrader.updateTrades`: two `buying`, two
    /// `uncommon` and five `common` offers, each set drawn without
    /// duplicates; entries naming items the game lacks are redrawn.
    pub(super) fn trader_stock(&mut self, rng: &mut super::Rng) {
        self.wandering = true;
        self.offers = [None; 10];
        for (_, amount, set) in super::villager::trades::WANDERING {
            let mut left: Vec<&super::villager::trades::TradeDef> = set.iter().collect();
            let mut found = 0;
            while found < amount as usize && !left.is_empty() {
                let d = left.remove(rng.next_int(left.len() as u32) as usize);
                if let Some(offer) = d.offer(rng.next_int(u32::MAX) as u64, "plains") {
                    self.push_offer(offer);
                    found += 1;
                }
            }
        }
    }
}
fn spot<W: MobWorld + ?Sized>(
    world: &W,
    center: DVec3,
    range: i32,
    kind: MobKind,
    rng: &mut super::Rng,
) -> Option<DVec3> {
    for _ in 0..SPAWN_ATTEMPTS {
        let x = center.x.floor() as i32 + rng.next_int((range * 2) as u32) as i32 - range;
        let z = center.z.floor() as i32 + rng.next_int((range * 2) as u32) as i32 - range;
        let Some(y) = world.surface(x, z) else {
            continue;
        };
        let pos = DVec3::new(x as f64 + 0.5, (y + 1) as f64, z as f64 + 0.5);
        if world.loaded(pos.floor().as_ivec3())
            && world.block(IVec3::new(x, y, z)).is_some_and(Block::is_opaque)
            && !physics::overlaps_solid(world, pos, kind.shape())
            && !physics::is_fluid_at(world, pos)
        {
            return Some(pos);
        }
    }
    None
}
impl Entities {
    pub(super) fn trader_tick<W: MobWorld + ?Sized>(&mut self, dt: f32, world: &W, ctx: &Ctx) {
        if !self.trader_spawning || ctx.dimension != Dimension::Overworld {
            return;
        }
        let Some(chance) = self.trader_spawner.attempt(dt) else {
            return;
        };
        // `random.nextInt(100) <= chance`: 26, 51 or 76 percent.
        if self.rng.next_int(100) > chance as u32 {
            return;
        }
        // `getRandomPlayer` picks among living players; with none, Java's
        // `spawn` reports success, so the chance resets.
        let alive = ctx.players.iter().filter(|p| p.alive).count();
        if alive == 0 {
            self.trader_spawner.chance = MIN_CHANCE;
            return;
        }
        let pick = self.rng.next_int(alive as u32) as usize;
        let player = ctx.players.iter().filter(|p| p.alive).nth(pick).unwrap();
        if self.rng.next_int(ONE_IN) != 0 {
            return;
        }
        let mut center = player.pos;
        let mut nearest = 48.0 * 48.0;
        world.village_pois(&mut |p, b| {
            let d = p.as_dvec3().distance_squared(player.pos);
            if b.base() == Block::BELL && d < nearest {
                nearest = d;
                center = p.as_dvec3() + 0.5;
            }
        });
        self.spawn_trader(world, center);
    }
    fn spawn_trader<W: MobWorld + ?Sized>(&mut self, world: &W, center: DVec3) -> bool {
        let Some(pos) = spot(world, center, 48, MobKind::WanderingTrader, &mut self.rng) else {
            return false;
        };
        // Java requires a 2x3x2 clear space for the trader event.
        let cell = pos.floor().as_ivec3();
        for y in 0..3 {
            for z in 0..2 {
                for x in 0..2 {
                    if world.block(cell + IVec3::new(x, y, z)).is_none_or(|b| b.is_solid()) {
                        return false;
                    }
                }
            }
        }
        self.spawn(MobKind::WanderingTrader, pos);
        let trader = self.mobs.last_mut().unwrap();
        trader.trader.as_mut().unwrap().destination = center;
        let id = trader.villager.as_ref().unwrap().id;
        for _ in 0..2 {
            if let Some(at) = spot(world, pos, 4, MobKind::TraderLlama, &mut self.rng) {
                self.spawn(MobKind::TraderLlama, at);
                self.mobs.last_mut().unwrap().trader.as_mut().unwrap().leader = id;
            }
        }
        self.trader_spawner.chance = MIN_CHANCE;
        true
    }
    pub(super) fn trader_upkeep(&mut self) {
        // Reused ID lookup: no llama-by-trader scan across the mob vector.
        self.trader_leaders.clear();
        for m in &self.mobs {
            if m.alive()
                && m.kind == MobKind::WanderingTrader
                && let Some(v) = &m.villager
            {
                self.trader_leaders.insert(v.id, (m.pos, m.trader.as_ref().unwrap().despawn, v.trading));
            }
        }
        for m in &mut self.mobs {
            if !m.simulation_active || m.kind != MobKind::TraderLlama {
                continue;
            }
            let Some(t) = &mut m.trader else { continue };
            if let Some(&(pos, despawn, _)) = self.trader_leaders.get(&t.leader) {
                t.despawn = despawn;
                t.destination = pos;
                m.hunt = (m.pos.distance_squared(pos) > 4.0).then_some(pos);
                // Use the existing lead renderer and replica projection. An
                // explicit player/fence lead takes precedence over the merchant.
                if m.leash.is_none() {
                    m.leash_pos = Some(pos + DVec3::Y * 1.2);
                }
            } else if m.leash.is_none() {
                m.leash_pos = None;
            }
        }
    }
}
impl Mob {
    /// Returns `Some(vanished)` when a drink finishes: Java's `UseItemGoal` sound.
    pub(super) fn update_trader(&mut self, dt: f32, night: bool) -> Option<bool> {
        let t = self.trader.as_mut()?;
        let trading = self.villager.as_ref().is_some_and(|v| v.trading);
        if !trading {
            t.despawn = (t.despawn - dt).max(0.0);
        }
        if self.kind != MobKind::WanderingTrader {
            return None;
        }
        // The potion's effect runs out on its own; milk clears it early.
        if t.invisible {
            t.hide_left -= dt;
        }
        if t.invisible && t.hide_left <= 0.0 {
            t.invisible = false;
            t.hide_left = 0.0;
        }
        // Java: drink Invisibility when dark and visible, milk when bright and invisible.
        let wants = if night { !t.invisible } else { t.invisible };
        let mut drank = None;
        if wants && !trading {
            if t.drink == 0.0 {
                t.drink = 1.6;
            }
            t.drink = (t.drink - dt).max(0.0);
            if t.drink == 0.0 {
                t.invisible = night;
                t.hide_left = if night { HIDE_TIME } else { 0.0 };
                drank = Some(night);
            }
        } else {
            t.drink = 0.0;
        }
        if let Some(v) = &mut self.villager {
            v.goal = if trading || t.drink > 0.0 {
                None
            } else {
                self.hunt.or_else(|| (self.pos.distance_squared(t.destination) > 4.0).then_some(t.destination))
            };
        }
        drank
    }
}
/// Every Java item name the wandering trader's trades use.
pub fn trade_item_names() -> impl Iterator<Item = &'static str> {
    super::villager::trades::WANDERING
        .into_iter()
        .flat_map(|(_, _, set)| set)
        .flat_map(|t| [Some(t.wants.0), t.also.map(|a| a.0), Some(t.gives.0)].into_iter().flatten())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Item;
    use crate::physics::test_util::Grid;
    #[test]
    fn merchant_leads_render_after_reload_and_clear_when_the_merchant_is_gone() {
        let mut e = Entities::new(6);
        let w = Grid::flat(0);
        assert!(e.spawn_trader(&w, DVec3::ZERO));
        let saved = e.villagers_to_string();
        let mut loaded = Entities::new(7);
        loaded.load_villagers(&saved);
        loaded.trader_upkeep();
        let trader = loaded.mobs.iter().find(|m| m.kind == MobKind::WanderingTrader).unwrap();
        let anchor = trader.pos + DVec3::Y * 1.2;
        for llama in loaded.mobs.iter().filter(|m| m.kind == MobKind::TraderLlama) {
            assert_eq!(llama.leash_pos, Some(anchor));
            assert_eq!(llama.look()["leash"], serde_json::json!(anchor.to_array()));
        }
        loaded.mobs.retain(|m| m.kind != MobKind::WanderingTrader);
        loaded.trader_upkeep();
        assert!(loaded.mobs.iter().all(|m| m.leash_pos.is_none()));
    }

    #[test]
    fn invisibility_lasts_three_minutes_then_the_trader_drinks_again() {
        let mut e = Entities::new(6);
        e.spawn(MobKind::WanderingTrader, DVec3::ZERO);
        let m = &mut e.mobs[0];
        let drank: Vec<_> = (0..33).filter_map(|_| m.update_trader(0.05, true)).collect();
        assert_eq!(drank, [true], "one Invisibility drink, announced once");
        assert!(m.trader.as_ref().unwrap().invisible);
        // 3600 ticks later the effect lapses; it is still night, so the potion is drunk again.
        assert_eq!(m.update_trader(179.0, true), None);
        assert!(m.trader.as_ref().unwrap().invisible);
        assert_eq!(m.update_trader(1.0, true), None);
        assert!(!m.trader.as_ref().unwrap().invisible);
        let again: Vec<_> = (0..33).filter_map(|_| m.update_trader(0.05, true)).collect();
        assert_eq!(again, [true]);
        // Milk at daybreak clears it early and announces the reappearance.
        let milk: Vec<_> = (0..33).filter_map(|_| m.update_trader(0.05, false)).collect();
        assert_eq!(milk, [false]);
        assert!(!m.trader.as_ref().unwrap().invisible);
        // The remaining time survives a save.
        for _ in 0..33 {
            m.update_trader(0.05, true);
        }
        m.update_trader(60.0, true);
        let t = Trader::load(&m.trader.as_ref().unwrap().save(), DVec3::ZERO).unwrap();
        assert!(t.invisible && (t.hide_left - 120.0).abs() < 0.2, "{}", t.hide_left);
    }
    #[test]
    fn chance_progresses_daily_and_caps() {
        let mut s = Spawner::default();
        assert_eq!(s.attempt(1199.0), None);
        assert_eq!(s.attempt(1.0), Some(25));
        assert_eq!(s.attempt(1200.0), Some(50));
        assert_eq!(s.attempt(1200.0), Some(75));
        assert_eq!(s.attempt(1200.0), Some(75));
    }
    #[test]
    fn caravan_stock_lifetime_and_potions() {
        let mut e = Entities::new(6);
        let w = Grid::flat(0);
        assert!(e.spawn_trader(&w, DVec3::ZERO));
        assert_eq!(e.count(MobKind::WanderingTrader), 1);
        assert_eq!(e.count(MobKind::TraderLlama), 2);
        let v = e.mobs[0].villager.as_ref().unwrap();
        // Java 26.3: two `buying` offers (paid in emeralds), then two
        // `uncommon` and five `common` (sold for emeralds), all distinct.
        assert_eq!(v.offers.iter().flatten().count(), 9);
        for (i, o) in v.offers.iter().flatten().enumerate() {
            assert_eq!(o.output.item == Item::EMERALD, i < 2, "{o:?}");
            assert_eq!(o.cost.item == Item::EMERALD, i >= 2, "{o:?}");
            assert!(v.offers[..i].iter().flatten().all(|p| p != o));
        }
        for _ in 0..33 {
            e.mobs[0].update_trader(0.05, true);
        }
        assert!(e.mobs[0].trader.as_ref().unwrap().invisible);
        e.mobs[0].update_trader(0.05, true);
        for _ in 0..33 {
            e.mobs[0].update_trader(0.05, false);
        }
        assert!(!e.mobs[0].trader.as_ref().unwrap().invisible);
        let before = e.mobs[0].trader.as_ref().unwrap().despawn;
        e.mobs[0].villager.as_mut().unwrap().trading = true;
        e.mobs[0].update_trader(10.0, false);
        assert_eq!(e.mobs[0].trader.as_ref().unwrap().despawn, before);
    }
    #[test]
    fn caravan_save_preserves_stock_leaders_timers_and_spawn_progress() {
        let mut e = Entities::new(6);
        let w = Grid::flat(0);
        assert!(e.spawn_trader(&w, DVec3::ZERO));
        e.trader_spawner.delay = 123.5;
        e.trader_spawner.chance = 75;
        e.mobs[0].villager.as_mut().unwrap().offers[0].as_mut().unwrap().uses = 1;
        e.mobs[0].trader.as_mut().unwrap().despawn = 900.25;
        e.mobs[0].trader.as_mut().unwrap().invisible = true;
        e.trader_upkeep();
        let mut copy = Entities::new(9);
        copy.load_villagers(&e.villagers_to_string());
        assert_eq!(copy.mobs.len(), 3);
        assert_eq!(copy.trader_spawner.delay, 123.5);
        assert_eq!(copy.trader_spawner.chance, 75);
        assert!(copy.mobs[0].trader.as_ref().unwrap().invisible);
        assert_eq!(copy.mobs[0].trader.as_ref().unwrap().despawn, 900.25);
        assert_eq!(copy.mobs[0].villager.as_ref().unwrap().offers, e.mobs[0].villager.as_ref().unwrap().offers);
        assert_eq!(copy.mobs[1].trader.as_ref().unwrap().leader, copy.mobs[0].villager.as_ref().unwrap().id);
        let ctx = Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::DAY,
            players: vec![super::super::Target::new(super::super::PlayerId::HOST, DVec3::ZERO, false)],
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
        };
        copy.mobs[0].trader.as_mut().unwrap().despawn = 0.0;
        copy.update(0.05, &w, &ctx);
        assert_eq!(copy.count(MobKind::WanderingTrader), 0);
        assert_eq!(copy.count(MobKind::TraderLlama), 0);
    }
}
