//! v1.0 Track 7: Ender Dragons no fight owns, as Java 26.3's spawn egg
//! (and `/summon`) makes them: an `EnderDragon` whose `dragonFight` stays
//! `null` because its UUID isn't the End fight's.
//!
//! What Java does with such a dragon (`EnderDragon`, its phase manager and
//! phases, read from the 26.3 jar):
//!
//! - It starts `HOVERING` (`EnderDragonPhaseManager`'s first phase): it
//!   holds where it appeared (`DragonHoverPhase`, fly speed 1). Hovering
//!   counts as sitting, so it shoves rather than hurts with its wings and
//!   ignores knockback, but arrows still hurt it. Once more than a quarter
//!   of its health (50) has been taken while sitting, it takes off.
//! - Its fight origin is `BlockPos.ZERO` (nothing calls `setFightOrigin`),
//!   so it perches on top of the column at (0, 0). Its path nodes are the
//!   same 24 nodes around (0, 0) as the End fight's (`super::path`).
//! - With no fight it counts no crystals: `findClosestNode` and `findPath`
//!   use only the inner nodes 12–23, the holding pattern circles the eight
//!   nodes at radius 40 and lands one time in three, and a strafe follows
//!   one time in two. The phases are the fight dragon's, in `super`.
//! - No boss bar, no exit portal, no gateway: `tickDeath` pays 500
//!   experience and removes it. `checkDespawn` does nothing, so it never
//!   despawns. Crystals within 32 still heal it.

use glam::{DVec3, IVec3};

use super::{Arena, BreathCloud, Dragon, DragonFireball, Hit, Part, Phase, TICK};
use crate::entity::crystal::Crystals;
use crate::entity::{Ctx, EntityEvent, Rng};
use crate::physics::{self, BlockSource};

/// `EntityTypes.ENDER_DRAGON`: 16 wide and 8 tall.
pub const SHAPE: crate::physics::Shape = crate::physics::Shape::new(8.0, 8.0);

/// One dragon no fight owns.
pub struct WildDragon {
    pub id: u32,
    pub dragon: Dragon,
}

/// The dragons no fight owns in this dimension, with their fireballs and
/// breath.
#[derive(Default)]
pub struct Wild {
    pub dragons: Vec<WildDragon>,
    pub fireballs: Vec<DragonFireball>,
    pub clouds: Vec<BreathCloud>,
    pending: f64,
    next_id: u32,
    /// The `mobGriefing` game rule is off; set each tick by the entities.
    pub griefing_off: bool,
}

impl Wild {
    pub fn is_empty(&self) -> bool {
        self.dragons.is_empty() && self.fireballs.is_empty() && self.clouds.is_empty()
    }

    /// A new dragon hovering at `pos` (its feet) with Java yaw `yaw`.
    pub fn spawn(&mut self, pos: DVec3, yaw: f32) -> u32 {
        let mut dragon = Dragon::new(pos);
        dragon.phase = Phase::Hovering;
        dragon.yaw = super::wrap_degrees(yaw);
        dragon.previous_yaw = dragon.yaw;
        dragon.history = [(dragon.yaw, pos.y); 64];
        dragon.place_parts();
        let id = self.next_id;
        self.next_id += 1;
        self.dragons.push(WildDragon { id, dragon });
        id
    }

    /// Advances the dragons by `dt` game seconds, in whole 20 Hz ticks.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::entity) fn update<W: BlockSource + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        crystals: &Crystals,
        fight: Option<&super::Fight>,
        events: &mut Vec<EntityEvent>,
    ) {
        if self.is_empty() {
            self.pending = 0.0;
            return;
        }
        self.pending += dt;
        while self.pending >= TICK - 1e-9 {
            self.pending -= TICK;
            self.tick(world, ctx, rng, crystals, fight, events);
        }
    }

    fn tick<W: BlockSource + ?Sized>(
        &mut self,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        crystals: &Crystals,
        fight: Option<&super::Fight>,
        events: &mut Vec<EntityEvent>,
    ) {
        let (floor, top) = (ctx.dimension.min_y(), ctx.dimension.max_y());
        let griefing = !self.griefing_off;
        let (fireballs, clouds) = (&mut self.fireballs, &mut self.clouds);
        self.dragons.retain_mut(|w| {
            // Entities in unloaded chunks don't tick.
            if world.block(w.dragon.pos.floor().as_ivec3()).is_none() {
                return true;
            }
            let mut arena = Arena {
                fight: false,
                alive_crystals: 0,
                previously_killed: true,
                podium: IVec3::ZERO,
                fireballs,
                clouds,
                top,
                floor,
                griefing,
            };
            w.dragon.tick(&mut arena, world, ctx, rng, crystals, events)
        });
        if !self.fireballs.is_empty() {
            // The fight's dragon's parts too (v1.0 gap Track 8).
            let boxes = super::Boxes::new(fight, self, crystals);
            self.fireballs.retain_mut(|f| f.tick(world, ctx, &boxes, &mut self.clouds));
        }
        self.clouds.retain_mut(|c| c.tick(ctx, events));
    }

    /// The dragon part a ray hits within `max_dist`. A dying dragon's
    /// parts are still there to hit (they can't be hurt; see
    /// [`super::Fight::raycast`]).
    pub fn raycast(&self, origin: DVec3, dir: DVec3, max_dist: f64) -> Option<(Hit, f64)> {
        self.dragons
            .iter()
            .flat_map(|w| {
                w.dragon.parts.iter().filter_map(move |&(part, min, max)| {
                    physics::ray_aabb(origin, dir, min, max).map(|t| (Hit::Wild(w.id, part), t))
                })
            })
            .filter(|&(_, t)| t <= max_dist)
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// A hit on a part of dragon `id`: whether it landed
    /// ([`super::Dragon::hurt`]; arrows bounce off it while perched).
    pub fn strike(&mut self, id: u32, part: Part, blow: super::Blow) -> bool {
        self.dragons.iter_mut().find(|w| w.id == id).is_some_and(|w| w.dragon.hurt(part, blow))
    }

    /// A blast at `center` on each dragon ([`super::Dragon::blast`]).
    pub fn explode<W: BlockSource + ?Sized>(&mut self, world: &W, center: DVec3, power: f32) {
        for w in &mut self.dragons {
            w.dragon.blast(world, center, power);
        }
    }

    /// A wind burst's push on each dragon.
    pub fn wind_burst<W: BlockSource + ?Sized>(&mut self, world: &W, burst: &crate::entity::wind::WindBurst) {
        for w in &mut self.dragons {
            w.dragon.push_from_blast(world, burst.center, burst.radius, burst.knockback);
        }
    }

    /// The dragon part `p` is inside of.
    pub fn hit_at(&self, p: DVec3) -> Option<Hit> {
        self.dragons.iter().find_map(|w| {
            w.dragon
                .parts
                .iter()
                .find(|&&(_, min, max)| p.cmpge(min).all() && p.cmple(max).all())
                .map(|&(part, ..)| Hit::Wild(w.id, part))
        })
    }

    /// The dragons for the level file (`EnderDragon.addAdditionalSaveData`:
    /// its phase, death time and sitting damage, besides position, yaw and
    /// health).
    pub fn serialize(&self) -> String {
        let list: Vec<serde_json::Value> = self
            .dragons
            .iter()
            .map(|w| {
                let d = &w.dragon;
                serde_json::json!({
                    "pos": [d.pos.x, d.pos.y, d.pos.z],
                    "yaw": d.yaw,
                    "health": d.health,
                    "phase": super::phase_code(d.phase),
                    "death": d.death_ticks,
                    "sitting_damage": d.sitting_damage,
                })
            })
            .collect();
        serde_json::Value::Array(list).to_string()
    }

    /// Restores [`Wild::serialize`]. A strafe or charge loses its target,
    /// as in Java, and goes back to circling.
    pub fn load(text: &str) -> Self {
        let mut wild = Self::default();
        let Ok(serde_json::Value::Array(list)) = serde_json::from_str::<serde_json::Value>(text) else {
            return wild;
        };
        for v in list.iter().take(64) {
            let Some(p) = v["pos"].as_array() else { continue };
            let pos = DVec3::new(
                p.first().and_then(|x| x.as_f64()).unwrap_or(f64::NAN),
                p.get(1).and_then(|x| x.as_f64()).unwrap_or(f64::NAN),
                p.get(2).and_then(|x| x.as_f64()).unwrap_or(f64::NAN),
            );
            if !pos.is_finite() {
                continue;
            }
            let id = wild.spawn(pos, v["yaw"].as_f64().unwrap_or(0.0) as f32);
            let d = &mut wild.dragons.iter_mut().find(|w| w.id == id).unwrap().dragon;
            d.health = v["health"].as_f64().unwrap_or(f64::from(super::MAX_HEALTH)).clamp(0.0, 200.0) as f32;
            d.sitting_damage = v["sitting_damage"].as_f64().unwrap_or(0.0).max(0.0) as f32;
            d.phase = match super::phase_from_code(v["phase"].as_u64().unwrap_or(10)) {
                Phase::Strafe(_) | Phase::Charging => Phase::HoldingPattern,
                p => p,
            };
            // Dying in the flight to its origin's column (`DYING` at one
            // health), or dead (saves from before the flight was modelled
            // wrote `DYING` with no health).
            if d.phase == Phase::Dying && d.health > 0.0 {
                d.health = 1.0;
            } else if d.phase == Phase::Dying || d.health <= 0.0 {
                d.phase = Phase::Dying;
                d.health = 0.0;
                d.death_ticks = v["death"].as_u64().unwrap_or(0).min(u64::from(super::DEATH_TICKS - 1)) as u32;
            }
        }
        wild
    }

    /// What a LAN client needs to draw the dragons.
    pub fn replica_json(&self) -> serde_json::Value {
        serde_json::json!({
            "dragons": self.dragons.iter().map(|w| serde_json::json!({
                "id": w.id,
                "dragon": super::dragon_replica_json(&w.dragon),
            })).collect::<Vec<_>>(),
            "fireballs": self.fireballs.iter().map(|f| {
                serde_json::json!([[f.pos.x, f.pos.y, f.pos.z], [f.previous_pos.x, f.previous_pos.y, f.previous_pos.z], [f.dir.x, f.dir.y, f.dir.z]])
            }).collect::<Vec<_>>(),
        })
    }

    /// Updates a client's dragons from the host's [`Wild::replica_json`].
    pub fn apply_replica(&mut self, v: &serde_json::Value) {
        let list = v["dragons"].as_array().map(Vec::as_slice).unwrap_or_default();
        let ids: Vec<u32> = list.iter().filter_map(|d| d["id"].as_u64()).map(|i| i as u32).collect();
        self.dragons.retain(|w| ids.contains(&w.id));
        for d in list.iter().take(64) {
            let Some(id) = d["id"].as_u64().map(|i| i as u32) else { continue };
            let i = match self.dragons.iter().position(|w| w.id == id) {
                Some(i) => i,
                None => {
                    let at = super::replica_v3(&d["dragon"]["pos"]).unwrap_or_default();
                    self.dragons.push(WildDragon { id, dragon: Dragon::new(at) });
                    self.dragons.len() - 1
                }
            };
            if !super::apply_dragon_replica(&mut self.dragons[i].dragon, &d["dragon"]) {
                self.dragons.swap_remove(i);
            }
        }
        self.fireballs = super::fireballs_from_replica(&v["fireballs"]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{PlayerId, Target};
    use crate::world::block::Block;
    use crate::world::terrain::Dimension;

    /// Stone up to y = 64, air above.
    struct Ground;
    impl BlockSource for Ground {
        fn block(&self, p: IVec3) -> Option<Block> {
            Some(if p.y < 64 { Block::STONE } else { Block::AIR })
        }
    }

    fn ctx(players: Vec<Target>) -> Ctx {
        Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::NIGHT,
            players,
            daylight: 0.0,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
        }
    }

    fn run(wild: &mut Wild, ticks: u32, ctx: &Ctx, events: &mut Vec<EntityEvent>) {
        let crystals = Crystals::default();
        let mut rng = Rng::new(9);
        for _ in 0..ticks {
            wild.update(TICK, &Ground, ctx, &mut rng, &crystals, None, events);
        }
    }

    #[test]
    fn an_egg_dragon_hovers_until_hurt_then_circles_the_origin() {
        let mut wild = Wild::default();
        let at = DVec3::new(300.0, 100.0, -200.0);
        wild.spawn(at, 45.0);
        let ctx = ctx(Vec::new());
        let mut events = Vec::new();
        run(&mut wild, 100, &ctx, &mut events);
        let d = &wild.dragons[0].dragon;
        assert_eq!(d.phase, Phase::Hovering);
        // Java's flight toward a target it sits on thrusts slightly
        // backwards (`0.06 * (dot * span + 1 - span)` with span 2), so it
        // wobbles about the spot rather than holding it exactly.
        assert!(d.pos.distance(at) < 3.0, "it holds where it appeared: {:?}", d.pos);
        // Hovering counts as sitting: arrows still hurt it, and more than a
        // quarter of its health taken makes it take off.
        let id = wild.dragons[0].id;
        assert!(wild.strike(id, Part::Head, super::super::Blow::arrow(20.0, true)), "arrows hurt a hovering dragon");
        assert_eq!(wild.dragons[0].dragon.phase, Phase::Hovering);
        run(&mut wild, 11, &ctx, &mut events);
        assert!(wild.strike(id, Part::Head, super::super::Blow::melee(40.0)));
        assert_eq!(wild.dragons[0].dragon.phase, Phase::Takeoff);
        // Far from the origin's perch, the takeoff becomes the holding pattern,
        // which heads for the inner ring around (0, 0).
        run(&mut wild, 600, &ctx, &mut events);
        let d = &wild.dragons[0].dragon;
        assert!(d.pos.with_y(0.0).length() < at.with_y(0.0).length(), "closer to the origin: {:?}", d.pos);
        assert!(!events.iter().any(|e| matches!(e, EntityEvent::DragonKilled { .. })));
    }

    #[test]
    fn a_dragon_with_no_fight_pays_500_experience_and_opens_no_portal() {
        let mut wild = Wild::default();
        wild.spawn(DVec3::new(0.0, 120.0, 0.0), 0.0);
        let id = wild.dragons[0].id;
        assert!(wild.strike(id, Part::Head, super::super::Blow::melee(500.0)));
        let mut events = Vec::new();
        run(&mut wild, 210, &ctx(Vec::new()), &mut events);
        assert!(wild.dragons.is_empty(), "removed once dying ends");
        let xp: u32 = events
            .iter()
            .filter_map(|e| if let EntityEvent::DragonXp { points, .. } = e { Some(*points) } else { None })
            .sum();
        // `tickDeath`: floor(500 * 0.08) every fifth tick past 150, then floor(500 * 0.2).
        assert_eq!(xp, 40 * 10 + 100);
        assert!(!events.iter().any(|e| matches!(e, EntityEvent::DragonKilled { .. } | EntityEvent::ExitPortal { .. })));
    }

    #[test]
    fn wild_dragons_save_their_phase_health_and_sitting_damage() {
        let mut wild = Wild::default();
        wild.spawn(DVec3::new(5.0, 90.0, 6.0), 30.0);
        let id = wild.dragons[0].id;
        wild.strike(id, Part::Head, super::super::Blow::melee(30.0));
        let back = Wild::load(&wild.serialize());
        let (a, b) = (&wild.dragons[0].dragon, &back.dragons[0].dragon);
        assert_eq!(
            (a.pos, a.yaw, a.health, a.phase, a.sitting_damage),
            (b.pos, b.yaw, b.health, b.phase, b.sitting_damage)
        );
        // A strafe loses its target on reload, as Java's does.
        wild.dragons[0].dragon.phase = Phase::Strafe(PlayerId::HOST);
        assert_eq!(Wild::load(&wild.serialize()).dragons[0].dragon.phase, Phase::HoldingPattern);
        assert!(Wild::load("not json").dragons.is_empty());
    }

    #[test]
    fn wild_dragons_replicate_to_lan_clients() {
        let mut wild = Wild::default();
        wild.spawn(DVec3::new(1.0, 80.0, 2.0), 10.0);
        let mut client = Wild::default();
        client.apply_replica(&wild.replica_json());
        assert_eq!(client.dragons.len(), 1);
        assert_eq!(client.dragons[0].dragon.pos, wild.dragons[0].dragon.pos);
        client.apply_replica(&Wild::default().replica_json());
        assert!(client.dragons.is_empty());
    }
}
