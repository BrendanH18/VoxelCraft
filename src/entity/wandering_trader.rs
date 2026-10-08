//! Java 1.21 wandering trader attempts, stock, potions and linked llama lifetime.
use super::villager::{Offer, Villager};
use super::{Ctx, Entities, Mob, MobKind, MobWorld};
use crate::{
    inventory::Stack,
    item::Item,
    physics,
    world::{block::Block, terrain::Dimension},
};
use glam::{DVec3, IVec3};

#[derive(Clone, Copy)]
pub struct Trader {
    pub despawn: f32,
    pub invisible: bool,
    pub drink: f32,
    pub leader: u64,
    pub destination: DVec3,
}
impl Trader {
    pub(super) fn save(&self) -> serde_json::Value {
        serde_json::json!([self.despawn, self.invisible, self.drink, self.leader, self.destination.to_array()])
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
        t.destination.is_finite().then_some(t)
    }
    pub fn new(pos: DVec3) -> Self {
        Self { despawn: 2400.0, invisible: false, drink: 0.0, leader: 0, destination: pos }
    }
}
#[derive(Clone, Copy)]
pub(super) struct Spawner {
    pub delay: f32,
    pub chance: u8,
}
impl Default for Spawner {
    fn default() -> Self {
        Self { delay: 1200.0, chance: 25 }
    }
}
impl Spawner {
    fn attempt(&mut self, dt: f32) -> Option<u8> {
        self.delay -= dt;
        if self.delay > 0.0 {
            return None;
        }
        self.delay += 1200.0;
        let chance = self.chance;
        self.chance = (self.chance + 25).min(75);
        Some(chance)
    }
}
// Five common offers plus one rare, skipping families unavailable in the engine.
const COMMON: &[(&str, u8, u8, u8)] = &[
    ("slime ball", 4, 1, 5),
    ("glowstone", 2, 1, 5),
    ("sugar cane", 1, 1, 8),
    ("pumpkin", 1, 1, 4),
    ("cactus", 3, 1, 8),
    ("dandelion", 1, 1, 12),
    ("poppy", 1, 1, 12),
    ("wheat seeds", 1, 1, 12),
    ("pumpkin seeds", 1, 1, 12),
    ("oak sapling", 5, 1, 8),
    ("spruce sapling", 5, 1, 8),
    ("birch sapling", 5, 1, 8),
    ("jungle sapling", 5, 1, 8),
    ("acacia sapling", 5, 1, 8),
    ("red dye", 1, 3, 12),
    ("white dye", 1, 3, 12),
    ("blue dye", 1, 3, 12),
    ("pink dye", 1, 3, 12),
    ("black dye", 1, 3, 12),
    ("green dye", 1, 3, 12),
    ("light gray dye", 1, 3, 12),
    ("magenta dye", 1, 3, 12),
    ("yellow dye", 1, 3, 12),
    ("gray dye", 1, 3, 12),
    ("purple dye", 1, 3, 12),
    ("light blue dye", 1, 3, 12),
    ("lime dye", 1, 3, 12),
    ("orange dye", 1, 3, 12),
    ("brown dye", 1, 3, 12),
    ("cyan dye", 1, 3, 12),
    ("brown mushroom", 1, 1, 12),
    ("red mushroom", 1, 1, 12),
    ("sand", 1, 8, 8),
];
const RARE: &[(&str, u8, u8, u8)] = &[("gunpowder", 1, 1, 8), ("podzol", 3, 3, 6), ("packed ice", 3, 1, 6)];
impl Villager {
    pub(super) fn trader_stock(&mut self, rng: &mut super::Rng) {
        self.wandering = true;
        self.offers = [None; 10];
        let mut picks = [None; 5];
        let mut seen = 0;
        for &d in COMMON {
            if Item::from_name(d.0).is_none() {
                continue;
            }
            seen += 1;
            let i = rng.next_int(seen) as usize;
            if seen <= 5 {
                picks[(seen - 1) as usize] = Some(d);
            } else if i < 5 {
                picks[i] = Some(d);
            }
        }
        let mut rare = None;
        let mut seen = 0;
        for &d in RARE {
            if Item::from_name(d.0).is_some() {
                seen += 1;
                if rng.next_int(seen) == 0 {
                    rare = Some(d);
                }
            }
        }
        for (i, d) in picks.into_iter().chain([rare]).enumerate() {
            self.offers[i] = d.and_then(|(name, price, count, uses)| {
                Some(Offer {
                    cost: Stack::new(Item::EMERALD, price),
                    second: None,
                    output: Stack::new(Item::from_name(name)?, count),
                    uses: 0,
                    max_uses: uses,
                    xp: 0,
                    demand: 0,
                    multiplier: 0.05,
                })
            });
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
    for _ in 0..10 {
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
        if !self.rng.chance(chance as f32 / 100.0) || self.rng.next_int(10) != 0 || ctx.players.is_empty() {
            return;
        }
        let player = &ctx.players[self.rng.next_int(ctx.players.len() as u32) as usize];
        if !player.alive {
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
        self.trader_spawner.chance = 25;
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
            if m.kind == MobKind::TraderLlama
                && let Some(t) = &mut m.trader
                && let Some(&(pos, despawn, _)) = self.trader_leaders.get(&t.leader)
            {
                t.despawn = despawn;
                t.destination = pos;
                m.hunt = (m.pos.distance_squared(pos) > 4.0).then_some(pos);
            }
        }
    }
}
impl Mob {
    pub(super) fn update_trader(&mut self, dt: f32, night: bool) {
        let Some(t) = &mut self.trader else {
            return;
        };
        let trading = self.villager.as_ref().is_some_and(|v| v.trading);
        if !trading {
            t.despawn = (t.despawn - dt).max(0.0);
        }
        if self.kind != MobKind::WanderingTrader {
            return;
        }
        if t.invisible != night && !trading {
            if t.drink == 0.0 {
                t.drink = 1.6;
            }
            t.drink = (t.drink - dt).max(0.0);
            if t.drink == 0.0 {
                t.invisible = night;
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
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::test_util::Grid;
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
        assert_eq!(v.offers.iter().flatten().count(), 6);
        for (i, o) in v.offers.iter().flatten().enumerate() {
            assert_eq!(o.cost.item, Item::EMERALD);
            if i < 5 {
                assert!(v.offers[..i].iter().flatten().all(|p| p.output.item != o.output.item));
            }
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
        e.mobs[0].villager.as_mut().unwrap().offers[0].as_mut().unwrap().uses = 3;
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
