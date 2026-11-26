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
//!   so it perches on top of the column at (0, 0), and its 24 path nodes
//!   are Java's absolute ring around (0, 0) at
//!   `max(73, MOTION_BLOCKING_NO_LEAVES height + 5/15/5)`.
//! - With no fight it counts no crystals: `findClosestNode` and `findPath`
//!   use only the inner nodes 12–23, the holding pattern circles the eight
//!   nodes at radius 40 and lands one time in three, and a strafe follows
//!   one time in two.
//! - No boss bar, no exit portal, no gateway: `tickDeath` pays 500
//!   experience and removes it. `checkDespawn` does nothing, so it never
//!   despawns. Crystals within 32 still heal it.

use glam::{DVec3, IVec3};

use super::{Arena, BreathCloud, Dragon, DragonFireball, Hit, Part, Phase, TICK};
use crate::entity::crystal::Crystals;
use crate::entity::{Ctx, EntityEvent, Rng};
use crate::physics::{self, BlockSource};

/// `EnderDragon.findClosestNode`'s lowest node height.
pub const MIN_NODE_Y: i32 = 73;
/// `EntityTypes.ENDER_DRAGON`: 16 wide and 8 tall.
pub const SHAPE: crate::physics::Shape = crate::physics::Shape::new(8.0, 8.0);

/// `EnderDragon.nodeAdjacency`: which nodes each node links to.
const ADJACENCY: [u32; 24] = [
    6146, 8197, 8202, 16404, 32808, 32848, 65696, 131392, 131712, 263424, 526848, 525313, 1581057, 3166214, 2138120,
    6373424, 4358208, 12910976, 9044480, 9706496, 15216640, 13688832, 11763712, 8257536,
];

/// The top of the motion-blocking column at `(x, z)` with leaves left out
/// (`Heightmap.Types.MOTION_BLOCKING_NO_LEAVES`): the cell above the
/// highest block with collision or fluid, scanning down from `top`.
/// `None` while the column isn't loaded.
pub fn surface<W: BlockSource + ?Sized>(world: &W, x: i32, z: i32, top: i32, floor: i32) -> Option<i32> {
    for y in (floor..=top).rev() {
        let b = world.block(IVec3::new(x, y, z))?;
        if (b.is_solid() || b.is_fluid()) && !b.is_leaves() {
            return Some(y + 1);
        }
    }
    Some(floor)
}

/// Java's node `(x, z)` for node `i` (`Mth.floor(r * Mth.cos(2 * (-PI + step * k)))`).
pub fn node_column(i: usize) -> (i32, i32) {
    use crate::knockback::{mth_cos, mth_sin};
    let (radius, step, k) = match i {
        0..12 => (60.0f32, std::f32::consts::PI / 12.0, i),
        12..20 => (40.0, std::f32::consts::PI / 8.0, i - 12),
        _ => (20.0, std::f32::consts::PI / 4.0, i - 20),
    };
    let a = 2.0f32 * (-std::f32::consts::PI + step * k as f32);
    ((radius * mth_cos(a as f64)).floor() as i32, (radius * mth_sin(a as f64)).floor() as i32)
}

/// The 24 nodes, and whether every column was loaded (the result is kept
/// only then; until it is, unloaded columns sit at the lowest height).
pub fn nodes<W: BlockSource + ?Sized>(world: &W, top: i32, floor: i32) -> ([DVec3; 24], bool) {
    let mut complete = true;
    let nodes = std::array::from_fn(|i| {
        let (x, z) = node_column(i);
        let lift = if (12..20).contains(&i) { 15 } else { 5 };
        let ground = surface(world, x, z, top, floor);
        complete &= ground.is_some();
        let y = ground.map_or(MIN_NODE_Y, |g| (g + lift).max(MIN_NODE_Y));
        DVec3::new(x as f64, y as f64, z as f64)
    });
    (nodes, complete)
}

/// `EnderDragon.findPath`: A* over the node links, never through nodes
/// below `min` (12 with no crystals). The nodes to fly through after
/// `from`, nearest last (so `pop` gives the next), with `last` appended.
pub fn find_path(nodes: &[DVec3; 24], from: usize, to: usize, min: usize, last: Option<DVec3>) -> Vec<DVec3> {
    // `Node.distanceTo`: float distance between whole-block positions.
    let dist = |a: usize, b: usize| (nodes[a] - nodes[b]).as_vec3().length();
    let mut g = [f32::INFINITY; 24];
    let mut came = [usize::MAX; 24];
    let mut closed = [false; 24];
    let mut open = vec![from];
    g[from] = 0.0;
    let mut closest = from;
    let mut found = false;
    while !open.is_empty() {
        let (k, _) = open
            .iter()
            .enumerate()
            .min_by(|a, b| (g[*a.1] + dist(*a.1, to)).total_cmp(&(g[*b.1] + dist(*b.1, to))))
            .unwrap();
        let node = open.swap_remove(k);
        if node == to {
            found = true;
            break;
        }
        if dist(node, to) < dist(closest, to) {
            closest = node;
        }
        closed[node] = true;
        for next in min..24 {
            if ADJACENCY[node] & (1 << next) == 0 || closed[next] {
                continue;
            }
            let tentative = g[node] + dist(node, next);
            if tentative < g[next] {
                came[next] = node;
                g[next] = tentative;
                if !open.contains(&next) {
                    open.push(next);
                }
            }
        }
    }
    let end = if found { to } else { closest };
    if end == from {
        return Vec::new();
    }
    let mut path: Vec<DVec3> = last.into_iter().collect();
    let mut at = end;
    while at != from {
        path.push(nodes[at]);
        at = came[at];
    }
    path
}

/// `(i - 12) & 7 + 12`: a node index folded onto the eight inner ring nodes.
fn inner_ring(i: i32) -> usize {
    (((i - 12) & 7) + 12) as usize
}

impl Dragon {
    /// The phases that differ for a dragon with no fight; returns whether
    /// it handled this one.
    pub(super) fn think_free<W: BlockSource + ?Sized>(
        &mut self,
        arena: &mut Arena<'_>,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
    ) -> bool {
        match self.phase {
            // `DragonHoverPhase.doServerTick`: hold where it was.
            Phase::Hovering => {
                if self.target.is_none() {
                    self.target = Some(self.pos);
                }
            }
            Phase::HoldingPattern => {
                if self.target_reached() {
                    self.holding_free(arena, world, ctx, rng);
                }
            }
            Phase::LandingApproach => {
                if self.target_reached() {
                    if self.path.is_empty() {
                        // `findNewTarget`: from the nearest node to the one
                        // away from the player nearest the egg, then the egg.
                        let egg = arena.perch(world);
                        let current = arena.closest_node(self.pos, 12..24);
                        // `NEAR_EGG_TARGETING` has no range.
                        let player = nearest(ctx, egg);
                        let target = match player {
                            Some(p) => {
                                let aim = p.with_y(0.0).normalize_or_zero();
                                arena.closest_node(DVec3::new(-aim.x * 40.0, 105.0, -aim.z * 40.0), 12..24)
                            }
                            None => arena.closest_node(DVec3::new(40.0, egg.y.floor(), 0.0), 12..24),
                        };
                        let last = egg.floor().with_y(egg.y.floor());
                        self.path = find_path(arena.nodes, current, target, 12, Some(last));
                        if self.path.is_empty() {
                            // No path: Java keeps trying.
                            return true;
                        }
                    }
                    self.next_waypoint(rng, true);
                    if self.path.is_empty() {
                        self.set_phase(Phase::Landing);
                    }
                }
            }
            Phase::Takeoff => {
                if self.phase_ticks <= 1 && self.path.is_empty() && self.target.is_none() {
                    // `DragonTakeoffPhase.findNewTarget`: out along the head's
                    // look (pitched down by its distance from the egg).
                    let egg = arena.perch(world);
                    let current = arena.closest_node(self.pos, 12..24);
                    let dist = ((egg.distance(self.pos) / 4.0) as f32).max(1.0);
                    let pitch = (-6.0 / dist * 1.5 * 5.0).to_radians();
                    let out = super::heading(self.yaw) * pitch.cos() as f64;
                    let t = arena.closest_node(DVec3::new(out.x * 40.0, 105.0, out.z * 40.0), 12..24);
                    self.path = find_path(arena.nodes, current, inner_ring(t as i32), 12, None);
                    self.next_waypoint(rng, true);
                } else {
                    let egg = arena.perch(world);
                    if egg.distance_squared(self.pos) >= 100.0 {
                        self.set_phase(Phase::HoldingPattern);
                    }
                }
            }
            _ => return false,
        }
        true
    }

    /// `DragonHoldingPatternPhase.findNewTarget` with no crystals.
    fn holding_free<W: BlockSource + ?Sized>(&mut self, arena: &Arena<'_>, world: &W, ctx: &Ctx, rng: &mut Rng) {
        if self.path.is_empty() && self.target.is_some() {
            if rng.next_int(3) == 0 {
                self.set_phase(Phase::LandingApproach);
                return;
            }
            let egg = arena.perch(world);
            // `NEW_TARGET_TARGETING` has no range.
            if let Some(t) = ctx
                .players
                .iter()
                .filter(|t| t.targetable)
                .min_by(|a, b| a.pos.distance_squared(egg).total_cmp(&b.pos.distance_squared(egg)))
            {
                let center = egg.floor() + DVec3::splat(0.5);
                let odds = (center.distance_squared(t.pos) / 512.0 + 2.0) as u32;
                if rng.next_int(odds.max(1)) == 0 || rng.next_int(2) == 0 {
                    self.set_phase(Phase::Strafe(t.id));
                    return;
                }
            }
        }
        if self.path.is_empty() {
            let current = arena.closest_node(self.pos, 12..24) as i32;
            let mut to = current;
            if rng.next_int(8) == 0 {
                self.clockwise = !self.clockwise;
                to += 6;
            }
            to += if self.clockwise { 1 } else { -1 };
            self.path = find_path(arena.nodes, current as usize, inner_ring(to), 12, None);
        }
        self.next_waypoint(rng, true);
    }
}

/// The nearest targetable player to `at`.
fn nearest(ctx: &Ctx, at: DVec3) -> Option<DVec3> {
    ctx.players
        .iter()
        .filter(|t| t.targetable)
        .min_by(|a, b| a.pos.distance_squared(at).total_cmp(&b.pos.distance_squared(at)))
        .map(|t| t.pos)
}

/// One dragon no fight owns.
pub struct WildDragon {
    pub id: u32,
    pub dragon: Dragon,
    /// Its path nodes once every column was loaded (Java computes them
    /// once, on the first `findClosestNode`).
    nodes: Option<[DVec3; 24]>,
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
        self.dragons.push(WildDragon { id, dragon, nodes: None });
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
        events: &mut Vec<EntityEvent>,
    ) {
        if self.is_empty() {
            self.pending = 0.0;
            return;
        }
        self.pending += dt;
        while self.pending >= TICK - 1e-9 {
            self.pending -= TICK;
            self.tick(world, ctx, rng, crystals, events);
        }
    }

    fn tick<W: BlockSource + ?Sized>(
        &mut self,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        crystals: &Crystals,
        events: &mut Vec<EntityEvent>,
    ) {
        let (floor, top) = (ctx.dimension.min_y(), ctx.dimension.max_y());
        let (fireballs, clouds) = (&mut self.fireballs, &mut self.clouds);
        self.dragons.retain_mut(|w| {
            // Entities in unloaded chunks don't tick.
            if world.block(w.dragon.pos.floor().as_ivec3()).is_none() {
                return true;
            }
            let nodes = match w.nodes {
                Some(n) => n,
                None => {
                    let (n, complete) = nodes(world, top, floor);
                    if complete {
                        w.nodes = Some(n);
                    }
                    n
                }
            };
            let mut arena = Arena {
                fight: false,
                alive_crystals: 0,
                previously_killed: true,
                nodes: &nodes,
                podium: IVec3::ZERO,
                fireballs,
                clouds,
                ceiling: top as f64 + 16.0,
                floor,
            };
            w.dragon.tick(&mut arena, world, ctx, rng, crystals, events)
        });
        self.fireballs.retain_mut(|f| f.tick(world, ctx, &mut self.clouds));
        self.clouds.retain_mut(|c| c.tick(ctx, events));
    }

    /// The dragon part a ray hits within `max_dist`.
    pub fn raycast(&self, origin: DVec3, dir: DVec3, max_dist: f64) -> Option<(Hit, f64)> {
        self.dragons
            .iter()
            .filter(|w| w.dragon.phase != Phase::Dying)
            .flat_map(|w| {
                w.dragon.parts.iter().filter_map(move |&(part, min, max)| {
                    physics::ray_aabb(origin, dir, min, max).map(|t| (Hit::Wild(w.id, part), t))
                })
            })
            .filter(|&(_, t)| t <= max_dist)
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// A hit on a part of dragon `id`; arrows bounce off it while perched.
    pub fn strike(&mut self, id: u32, part: Part, damage: f32, arrow: bool) -> bool {
        self.dragons.iter_mut().find(|w| w.id == id).is_some_and(|w| {
            if arrow && w.dragon.phase.deflects_arrows() {
                return false;
            }
            w.dragon.hurt_by(part, damage, true)
        })
    }

    /// A blast at `center`: each dragon takes the damage of its worst-hit part.
    pub fn explode(&mut self, center: DVec3, power: f32) {
        for w in &mut self.dragons {
            w.dragon.blast(center, power);
        }
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
            if d.phase == Phase::Dying || d.health <= 0.0 {
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
                    self.dragons.push(WildDragon { id, dragon: Dragon::new(at), nodes: None });
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
            wild.update(TICK, &Ground, ctx, &mut rng, &crystals, events);
        }
    }

    #[test]
    fn nodes_follow_javas_ring_around_the_origin() {
        // `findClosestNode`: radius 60, 40 and 20, at `max(73, ground + 5/15/5)`.
        assert_eq!(node_column(0), (60, 0));
        assert_eq!(node_column(6), (-60, 0));
        assert_eq!(node_column(12), (40, 0));
        assert_eq!(node_column(20), (20, 0));
        let (nodes, complete) = nodes(&Ground, 320, -64);
        assert!(complete);
        assert_eq!(nodes[0].y, 73.0, "ground 64 + 5 is under the floor of 73");
        assert_eq!(nodes[12].y, 79.0, "the middle ring is 15 over the ground");
        // The A* path between two inner nodes stays on the inner nodes.
        let path = find_path(&nodes, 12, 16, 12, None);
        assert!(!path.is_empty());
        assert!(path.iter().all(|p| nodes[12..].contains(p)));
        assert_eq!(path.first(), Some(&nodes[16]), "the goal is popped last");
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
        assert!(wild.strike(id, Part::Head, 20.0, true), "arrows hurt a hovering dragon");
        assert_eq!(wild.dragons[0].dragon.phase, Phase::Hovering);
        run(&mut wild, 11, &ctx, &mut events);
        assert!(wild.strike(id, Part::Head, 40.0, false));
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
        assert!(wild.strike(id, Part::Head, 500.0, false));
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
        wild.strike(id, Part::Head, 30.0, false);
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
