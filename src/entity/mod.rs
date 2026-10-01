//! Entities: passive pigs, cows, sheep and chickens; hostile zombies,
//! skeletons, creepers and spiders; skeleton arrows, dropped items and
//! explosion smoke.
//!
//! Mobs live in a flat `Vec` (removal is `swap_remove`). Each frame
//! [`Entities::update`] spawns new mobs around the player, runs AI and
//! physics, moves arrows, and despawns far-away or dead ones. Things that
//! affect the rest of the game (a hit on the player, an explosion, a sound)
//! come back as [`EntityEvent`]s so this module stays independent of the
//! player, world edits, health and audio code.
//!
//! Rendering: [`model`] turns mobs, arrows and smoke into camera-relative
//! box-model vertices.

pub mod item;
mod mob;
pub mod model;
mod projectile;

use std::f32::consts::TAU;

use glam::{DVec3, IVec3, Vec3};

use crate::physics::{self, BlockSource};
use crate::render::entity::EntityVertex;
use crate::world::World;
use crate::world::block::Block;
use crate::world::noise::splitmix64;

pub use item::ItemEntity;
pub use mob::{Mob, MobKind, sky_light};
pub use projectile::Arrow;

/// Spawns happen this far from the player (blocks).
pub const SPAWN_MIN_DIST: f64 = 24.0;
pub const SPAWN_MAX_DIST: f64 = 64.0;
/// Mobs farther than this are removed.
pub const DESPAWN_DIST: f64 = 96.0;
/// Hostile mobs only spawn when it's darker than this.
pub const HOSTILE_SPAWN_DAYLIGHT: f32 = 0.35;
const SPAWN_INTERVAL: f32 = 0.25;
/// Seconds between passes that merge dropped items lying together.
const MERGE_INTERVAL: f32 = 0.5;
/// Player melee: cooldown between hits.
pub const ATTACK_COOLDOWN: f64 = 0.5;

/// Sounds entities make (the game maps them to audio).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MobSound {
    /// A creeper lit its fuse.
    Fuse,
    /// A skeleton loosed an arrow.
    Bow,
}

/// Something an entity did that the game needs to react to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EntityEvent {
    /// A mob or arrow hit the player: apply `damage` and add `knockback` to
    /// the player's velocity; `cause` is the death message.
    PlayerHit {
        damage: f32,
        knockback: Vec3,
        cause: &'static str,
    },
    /// A creeper exploded: break blocks and hurt everything nearby (see
    /// [`explosion_damage`]). [`Entities::explode`] handles the mobs.
    Explosion {
        center: DVec3,
        power: f32,
    },
    Sound {
        sound: MobSound,
        pos: DVec3,
    },
    /// A skeleton shot at `target` (turned into an arrow internally).
    Shoot {
        from: DVec3,
        target: DVec3,
    },
}

/// Damage and knockback strength (0..1) of an explosion of `power` at
/// `dist` blocks, following Minecraft: reaches `2 * power` blocks.
pub fn explosion_damage(power: f32, dist: f64) -> Option<(f32, f32)> {
    let reach = 2.0 * power as f64;
    if dist >= reach {
        return None;
    }
    let impact = (1.0 - dist / reach) as f32;
    Some((((impact * impact + impact) / 2.0 * 7.0 * reach as f32 + 1.0).floor(), impact))
}

/// One cube of explosion smoke.
pub struct Puff {
    pub pos: DVec3,
    vel: DVec3,
    pub age: f32,
    pub life: f32,
    pub size: f32,
}

/// World queries mobs need beyond plain block access.
pub trait MobWorld: BlockSource {
    /// Whether the chunk containing `p` is loaded.
    fn loaded(&self, p: IVec3) -> bool;
    /// Highest light-blocking block in a loaded column.
    fn surface(&self, x: i32, z: i32) -> Option<i32>;
    /// Nothing light-blocking above this cell.
    fn exposed(&self, p: IVec3) -> bool;
}

impl MobWorld for World {
    fn loaded(&self, p: IVec3) -> bool {
        self.is_loaded(p)
    }
    fn surface(&self, x: i32, z: i32) -> Option<i32> {
        self.surface_height(x, z)
    }
    fn exposed(&self, p: IVec3) -> bool {
        self.sky_exposed(p)
    }
}

/// Per-frame inputs for the entity update.
pub struct Ctx {
    /// Player feet position.
    pub player_pos: DVec3,
    /// Hostile mobs chase and attack (false in creative, like Minecraft).
    pub player_targetable: bool,
    /// Skylight multiplier, 1 at noon.
    pub daylight: f32,
    /// Natural spawning on/off.
    pub spawning: bool,
}

/// Small deterministic RNG (splitmix64).
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_f32(&mut self) -> f32 {
        (splitmix64(&mut self.0) >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next_f32()
    }

    pub fn chance(&mut self, p: f32) -> bool {
        self.next_f32() < p
    }
}

pub struct Entities {
    pub mobs: Vec<Mob>,
    pub arrows: Vec<Arrow>,
    pub puffs: Vec<Puff>,
    /// Dropped items. They stay put (and don't age) while their chunk is
    /// unloaded, and are saved with the world.
    pub items: Vec<ItemEntity>,
    rng: Rng,
    spawn_timer: f32,
    merge_timer: f32,
    /// Mobs drawn last frame (F3).
    pub rendered: usize,
    verts: Vec<EntityVertex>,
}

impl Entities {
    pub fn new(seed: u64) -> Self {
        Self {
            mobs: Vec::new(),
            arrows: Vec::new(),
            puffs: Vec::new(),
            items: Vec::new(),
            rng: Rng::new(seed ^ 0x6d6f_6273),
            spawn_timer: 0.0,
            merge_timer: 0.0,
            rendered: 0,
            verts: Vec::new(),
        }
    }

    pub fn count(&self, kind: MobKind) -> usize {
        self.mobs.iter().filter(|m| m.kind == kind && m.alive()).count()
    }

    pub fn spawn(&mut self, kind: MobKind, pos: DVec3) {
        let yaw = self.rng.range(0.0, TAU);
        self.mobs.push(Mob::new(kind, pos, yaw));
    }

    pub fn update<W: MobWorld + ?Sized>(&mut self, dt: f64, world: &W, ctx: &Ctx) -> Vec<EntityEvent> {
        let mut events = Vec::new();
        if ctx.spawning {
            self.spawn_timer -= dt as f32;
            if self.spawn_timer <= 0.0 {
                self.spawn_timer = SPAWN_INTERVAL;
                self.natural_spawn(world, ctx);
            }
        }

        let mut i = 0;
        while i < self.mobs.len() {
            let m = &self.mobs[i];
            let gone = m.dying.is_some_and(|t| t >= mob::DEATH_TIME)
                || m.pos.distance_squared(ctx.player_pos) > DESPAWN_DIST * DESPAWN_DIST
                || !world.loaded(m.pos.floor().as_ivec3());
            if gone {
                self.mobs.swap_remove(i);
                continue;
            }
            self.mobs[i].update(dt, world, ctx, &mut self.rng, &mut events);
            i += 1;
        }
        self.separate(dt);

        // Skeleton shots become arrows.
        for e in &events {
            if let EntityEvent::Shoot { from, target } = *e {
                self.arrows.push(Arrow::aimed(from, target, &mut self.rng));
            }
        }
        events.retain(|e| !matches!(e, EntityEvent::Shoot { .. }));
        self.arrows.retain_mut(|a| a.update(dt, world, ctx, &mut events));
        self.items.retain_mut(|item| !world.loaded(item.pos.floor().as_ivec3()) || item.update(dt, world));
        self.merge_timer -= dt as f32;
        if self.merge_timer <= 0.0 {
            self.merge_timer = MERGE_INTERVAL;
            item::merge(&mut self.items);
        }

        let dtf = dt as f32;
        self.puffs.retain_mut(|p| {
            p.age += dtf;
            p.pos += p.vel * dt;
            p.vel *= 1.0 - (dt * 3.0).min(1.0);
            p.vel.y += 1.5 * dt;
            p.age < p.life
        });
        events
    }

    /// Hurts and flings mobs caught in an explosion, and puffs smoke.
    pub fn explode(&mut self, center: DVec3, power: f32) {
        for m in &mut self.mobs {
            let mid = m.pos + DVec3::Y * (m.shape().height * 0.5);
            let Some((damage, impact)) = explosion_damage(power, mid.distance(center)) else { continue };
            let away = (mid - center).normalize_or(DVec3::Y);
            m.damage(damage, Some(away * (impact as f64 * 14.0) + DVec3::Y * 6.0), &mut self.rng);
        }
        for _ in 0..28 {
            let dir = DVec3::new(
                self.rng.range(-1.0, 1.0) as f64,
                self.rng.range(-0.4, 1.0) as f64,
                self.rng.range(-1.0, 1.0) as f64,
            );
            self.puffs.push(Puff {
                pos: center + dir * 0.6,
                vel: dir * self.rng.range(3.0, 8.0) as f64,
                age: 0.0,
                life: self.rng.range(0.6, 1.3),
                size: self.rng.range(0.4, 1.0),
            });
        }
    }

    /// Drops the loot of a mob of `kind` the player killed at `pos`.
    pub fn drop_loot(&mut self, kind: MobKind, pos: DVec3) {
        for (item, count) in kind.drops(&mut self.rng) {
            let vel = DVec3::new(self.rng.range(-1.5, 1.5) as f64, 4.0, self.rng.range(-1.5, 1.5) as f64);
            let stack = crate::inventory::Stack::new(item, count);
            self.items.push(ItemEntity::new(stack, pos + DVec3::Y * 0.5, vel, item::PICKUP_DELAY, &mut self.rng));
        }
    }

    /// Drops a stack that popped out of the block at `cell` (mined, spilled
    /// from a container, blown up).
    pub fn drop_from_block(&mut self, stack: crate::inventory::Stack, cell: IVec3) {
        self.items.push(item::block_drop(stack, cell, &mut self.rng));
    }

    /// Throws a stack from `eye` along `dir` (the player dropping items).
    pub fn throw(&mut self, stack: crate::inventory::Stack, eye: DVec3, dir: DVec3) {
        let spread = DVec3::new(self.rng.range(-0.3, 0.3) as f64, 0.0, self.rng.range(-0.3, 0.3) as f64);
        let vel = dir * 6.0 + DVec3::Y * 2.0 + spread;
        let pos = eye - DVec3::Y * 0.3;
        self.items.push(ItemEntity::new(stack, pos, vel, item::THROWN_PICKUP_DELAY, &mut self.rng));
    }

    /// Scatters a stack around `pos` in a random direction (a dying
    /// player's inventory).
    pub fn scatter(&mut self, stack: crate::inventory::Stack, pos: DVec3) {
        let a = self.rng.range(0.0, TAU);
        let speed = self.rng.range(0.5, 3.0) as f64;
        let vel = DVec3::new(a.cos() as f64 * speed, 4.0, a.sin() as f64 * speed);
        self.items.push(ItemEntity::new(stack, pos + DVec3::Y, vel, item::THROWN_PICKUP_DELAY, &mut self.rng));
    }

    /// `;`-separated dropped items for the level file.
    pub fn items_to_string(&self) -> String {
        self.items.iter().map(ItemEntity::serialize).collect::<Vec<_>>().join(";")
    }

    pub fn load_items(&mut self, text: &str) {
        let rng = &mut self.rng;
        self.items.extend(text.split(';').filter(|s| !s.is_empty()).filter_map(|s| ItemEntity::deserialize(s, rng)));
    }

    /// Gently pushes overlapping mobs apart.
    fn separate(&mut self, dt: f64) {
        let n = self.mobs.len();
        for i in 0..n {
            for j in i + 1..n {
                let (a, b) = (&self.mobs[i], &self.mobs[j]);
                let d = DVec3::new(b.pos.x - a.pos.x, 0.0, b.pos.z - a.pos.z);
                let min = a.shape().half_width + b.shape().half_width;
                let overlap_y = a.pos.y < b.pos.y + b.shape().height && b.pos.y < a.pos.y + a.shape().height;
                let dist = d.length();
                if !overlap_y || dist >= min {
                    continue;
                }
                let dir = if dist > 1e-4 { d / dist } else { DVec3::X };
                let push = dir * ((min - dist) * 8.0 * dt).min(0.2) / dt.max(1e-3);
                self.mobs[i].vel -= push * 0.5;
                self.mobs[j].vel += push * 0.5;
            }
        }
    }

    /// One spawn attempt per mob type that is under its cap.
    fn natural_spawn<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx) {
        for kind in MobKind::ALL {
            let cap = kind.spawn_cap();
            if self.count(kind) >= cap {
                continue;
            }
            let angle = self.rng.range(0.0, TAU) as f64;
            let dist = self.rng.range(SPAWN_MIN_DIST as f32, SPAWN_MAX_DIST as f32) as f64;
            let x = (ctx.player_pos.x + angle.cos() * dist).floor() as i32;
            let z = (ctx.player_pos.z + angle.sin() * dist).floor() as i32;
            let Some(pos) = spawn_spot(world, kind, x, z, ctx.daylight) else { continue };
            if !in_spawn_ring(ctx.player_pos, pos) {
                continue;
            }
            self.spawn(kind, pos);
            // Animals come in small herds.
            if !kind.is_hostile() {
                let extra = (self.rng.next_f32() * 3.0) as i32;
                for _ in 0..extra {
                    let (dx, dz) = ((self.rng.range(-3.0, 3.0)) as i32, (self.rng.range(-3.0, 3.0)) as i32);
                    if self.count(kind) < cap
                        && let Some(p) = spawn_spot(world, kind, x + dx, z + dz, ctx.daylight)
                    {
                        self.spawn(kind, p);
                    }
                }
            }
        }
    }

    /// Camera-relative triangles for every visible mob.
    pub fn mesh(&mut self, camera: DVec3, forward: Vec3, max_dist: f32, time: f32) -> &[EntityVertex] {
        self.verts.clear();
        self.rendered = model::build(&self.mobs, camera, forward, max_dist, time, &mut self.verts);
        model::build_arrows(&self.arrows, camera, &mut self.verts);
        model::build_puffs(&self.puffs, camera, &mut self.verts);
        &self.verts
    }

    /// Nearest living mob hit by a ray within `max_dist` (in units of
    /// `dir`), with the hit distance.
    pub fn raycast(&self, origin: DVec3, dir: DVec3, max_dist: f64) -> Option<(usize, f64)> {
        self.mobs
            .iter()
            .enumerate()
            .filter(|(_, m)| m.alive())
            .filter_map(|(i, m)| {
                let (min, max) = m.aabb();
                physics::ray_aabb(origin, dir, min, max).map(|t| (i, t))
            })
            .filter(|&(_, t)| t <= max_dist)
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// Player melee hit for `damage` on mob `index`, pushed along `dir`.
    /// Returns the kind of mob if this killed it.
    pub fn attack(&mut self, index: usize, dir: DVec3, damage: f32) -> Option<MobKind> {
        let flat = DVec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
        let knockback = flat * 6.0 + DVec3::Y * 5.0;
        let mob = self.mobs.get_mut(index)?;
        mob.damage(damage, Some(knockback), &mut self.rng).then_some(mob.kind)
    }
}

/// Whether `kind` may spawn standing on `ground` at this daylight level.
pub fn can_spawn_on(kind: MobKind, ground: Block, daylight: f32) -> bool {
    if kind.is_hostile() {
        daylight < HOSTILE_SPAWN_DAYLIGHT && ground.is_solid() && ground.is_opaque()
    } else {
        ground == Block::GRASS
    }
}

/// Spawns happen between [`SPAWN_MIN_DIST`] and [`SPAWN_MAX_DIST`].
pub fn in_spawn_ring(player: DVec3, pos: DVec3) -> bool {
    let d2 = player.distance_squared(pos);
    (SPAWN_MIN_DIST * SPAWN_MIN_DIST..=SPAWN_MAX_DIST * SPAWN_MAX_DIST).contains(&d2)
}

/// Feet position for a mob on the sky-exposed surface of column (x, z),
/// if the rules allow it there.
fn spawn_spot<W: MobWorld + ?Sized>(world: &W, kind: MobKind, x: i32, z: i32, daylight: f32) -> Option<DVec3> {
    let h = world.surface(x, z)?;
    let ground = world.block(IVec3::new(x, h, z))?;
    if !can_spawn_on(kind, ground, daylight) {
        return None;
    }
    let pos = DVec3::new(x as f64 + 0.5, h as f64 + 1.0, z as f64 + 0.5);
    let clear = !physics::overlaps_solid(world, pos, kind.shape()) && !physics::is_fluid_at(world, pos);
    clear.then_some(pos)
}

#[cfg(test)]
mod tests {
    use super::mob::{Ai, is_cliff};
    use super::*;
    use crate::physics::test_util::Grid;

    impl MobWorld for Grid {
        fn loaded(&self, _: IVec3) -> bool {
            true
        }
        fn surface(&self, _: i32, _: i32) -> Option<i32> {
            Some(self.floor_y - 1)
        }
        fn exposed(&self, p: IVec3) -> bool {
            p.y >= self.floor_y
        }
    }

    fn ctx(player: DVec3) -> Ctx {
        Ctx { player_pos: player, player_targetable: false, daylight: 1.0, spawning: false }
    }

    /// Runs one mob for `secs` at 60 Hz, forcing it to walk along +X.
    fn walk_east(world: &Grid, mut mob: Mob, secs: f64) -> Mob {
        let mut rng = Rng::new(1);
        let mut events = Vec::new();
        let c = ctx(DVec3::new(0.0, 100.0, 0.0));
        for _ in 0..(secs * 60.0) as usize {
            mob.ai = Ai::Wander;
            mob.ai_timer = 10.0;
            mob.move_yaw = 0.0;
            mob.update(1.0 / 60.0, world, &c, &mut rng, &mut events);
        }
        mob
    }

    #[test]
    fn mob_falls_and_lands() {
        let world = Grid::flat(10);
        let mut e = Entities::new(3);
        e.spawn(MobKind::Pig, DVec3::new(0.5, 20.0, 0.5));
        for _ in 0..120 {
            e.update(1.0 / 60.0, &world, &ctx(DVec3::new(0.0, 12.0, 0.0)));
        }
        let pig = &e.mobs[0];
        assert!(pig.on_ground, "pig should land");
        assert!((pig.pos.y - 10.0).abs() < 1e-3, "feet at {}", pig.pos.y);
    }

    #[test]
    fn mob_stops_at_walls_and_jumps_ledges() {
        // A 3-high wall at x = 4.
        let mut world = Grid::flat(10);
        for y in 10..13 {
            for z in -2..=2 {
                world.set(IVec3::new(4, y, z), Block::STONE);
            }
        }
        let zombie = walk_east(&world, Mob::new(MobKind::Zombie, DVec3::new(0.5, 10.0, 0.5), 0.0), 4.0);
        assert!(zombie.pos.x <= 4.0 - 0.3 + 1e-3, "walked through wall: x = {}", zombie.pos.x);
        assert!(zombie.pos.x > 3.5, "should reach the wall: x = {}", zombie.pos.x);

        // A 1-block step at x = 4 is jumped onto.
        let mut world = Grid::flat(10);
        for x in 4..12 {
            for z in -2..=2 {
                world.set(IVec3::new(x, 10, z), Block::STONE);
            }
        }
        let pig = walk_east(&world, Mob::new(MobKind::Pig, DVec3::new(0.5, 10.0, 0.5), 0.0), 5.0);
        assert!(pig.pos.x > 5.0, "should climb the ledge: x = {}", pig.pos.x);
        assert!((pig.pos.y - 11.0).abs() < 1e-3, "on top of ledge: y = {}", pig.pos.y);
    }

    #[test]
    fn mob_avoids_cliffs_but_takes_small_drops() {
        // A 10-deep pit starting at x = 4.
        let mut world = Grid::flat(10);
        for x in 4..10 {
            for z in -3..=3 {
                for y in 0..10 {
                    world.set(IVec3::new(x, y, z), Block::AIR);
                }
            }
        }
        let shape = MobKind::Pig.shape();
        assert!(is_cliff(&world, DVec3::new(3.5, 10.0, 0.5), DVec3::X, shape));
        assert!(!is_cliff(&world, DVec3::new(3.5, 10.0, 0.5), DVec3::NEG_X, shape));
        let pig = walk_east(&world, Mob::new(MobKind::Pig, DVec3::new(0.5, 10.0, 0.5), 0.0), 4.0);
        assert!(pig.pos.y > 9.99 && pig.pos.x < 4.45, "fell off the cliff: {:?}", pig.pos);

        // A 2-block drop is fine.
        let mut world = Grid::flat(10);
        for x in 4..10 {
            for z in -3..=3 {
                world.set(IVec3::new(x, 9, z), Block::AIR);
                world.set(IVec3::new(x, 8, z), Block::AIR);
            }
        }
        assert!(!is_cliff(&world, DVec3::new(3.5, 10.0, 0.5), DVec3::X, shape));
        let pig = walk_east(&world, Mob::new(MobKind::Pig, DVec3::new(0.5, 10.0, 0.5), 0.0), 4.0);
        assert!(pig.pos.x > 5.0 && (pig.pos.y - 8.0).abs() < 1e-3, "should step down: {:?}", pig.pos);
    }

    #[test]
    fn mob_floats_in_water() {
        let mut world = Grid::flat(0);
        for y in 0..10 {
            for x in -16..=16 {
                for z in -16..=16 {
                    world.set(IVec3::new(x, y, z), Block::WATER);
                }
            }
        }
        let mut pig = Mob::new(MobKind::Pig, DVec3::new(0.5, 3.0, 0.5), 0.0);
        let (mut rng, mut events) = (Rng::new(2), Vec::new());
        for _ in 0..600 {
            pig.update(1.0 / 60.0, &world, &ctx(DVec3::ZERO), &mut rng, &mut events);
        }
        assert!(pig.pos.y > 9.0 && pig.pos.y < 10.0, "should bob at the surface: y = {}", pig.pos.y);
    }

    #[test]
    fn zombie_attacks_with_cooldown() {
        let world = Grid::flat(10);
        let mut e = Entities::new(5);
        e.spawn(MobKind::Zombie, DVec3::new(0.5, 10.0, 0.5));
        let player = DVec3::new(1.5, 10.0, 0.5);
        let c = Ctx { player_pos: player, player_targetable: true, daylight: 0.1, spawning: false };
        let mut hits = Vec::new();
        for _ in 0..90 {
            hits.extend(e.update(1.0 / 60.0, &world, &c));
        }
        // 1.5 s: an immediate hit plus one after the 1 s cooldown.
        assert_eq!(hits.len(), 2, "{hits:?}");
        let EntityEvent::PlayerHit { damage, knockback, cause } = hits[0] else { panic!("{hits:?}") };
        assert!(damage > 0.0 && knockback.x > 0.0 && knockback.y > 0.0);
        assert_eq!(cause, "was slain by a zombie");

        // Creative players are ignored.
        let c = Ctx { player_targetable: false, ..c };
        assert!((0..120).all(|_| e.update(1.0 / 60.0, &world, &c).is_empty()));
    }

    #[test]
    fn zombie_gets_around_a_pillar() {
        let mut world = Grid::flat(10);
        for y in 10..14 {
            world.set(IVec3::new(3, y, 0), Block::STONE);
        }
        let mut e = Entities::new(8);
        e.spawn(MobKind::Zombie, DVec3::new(0.5, 10.0, 0.5));
        let c = Ctx { player_pos: DVec3::new(8.5, 10.0, 0.5), player_targetable: true, daylight: 0.1, spawning: false };
        let hit = (0..60 * 10).any(|_| !e.update(1.0 / 60.0, &world, &c).is_empty());
        assert!(hit, "zombie stuck at {:?}", e.mobs[0].pos);
    }

    #[test]
    fn zombies_burn_in_daylight() {
        let world = Grid::flat(10);
        let mut e = Entities::new(5);
        e.spawn(MobKind::Zombie, DVec3::new(0.5, 10.0, 0.5));
        let c = ctx(DVec3::new(30.0, 10.0, 0.0));
        for _ in 0..60 * 12 {
            e.update(1.0 / 60.0, &world, &c);
        }
        assert!(e.mobs.is_empty(), "zombie should burn up");
    }

    #[test]
    fn raycast_picks_nearest_mob_and_attacks_kill() {
        let mut e = Entities::new(9);
        e.spawn(MobKind::Pig, DVec3::new(5.0, 0.0, 0.0));
        e.spawn(MobKind::Pig, DVec3::new(3.0, 0.0, 0.0));
        e.spawn(MobKind::Zombie, DVec3::new(3.0, 0.0, 3.0));
        let eye = DVec3::new(0.0, 0.5, 0.0);
        let (i, t) = e.raycast(eye, DVec3::X, 6.0).unwrap();
        assert_eq!(i, 1);
        assert!((t - 2.55).abs() < 1e-9, "t = {t}");
        assert!(e.raycast(eye, DVec3::X, 2.0).is_none(), "out of reach");
        assert!(e.raycast(eye, DVec3::NEG_X, 6.0).is_none());

        // Pigs have 10 HP: three hits with a wooden sword (4 each).
        let killed = (0..3).any(|_| {
            e.mobs[1].hurt = 0.0;
            e.attack(1, DVec3::X, 4.0).is_some()
        });
        assert!(killed && !e.mobs[1].alive());
        assert!(e.mobs[1].vel.x > 0.0, "knocked back along the hit");
        // Dying mobs can't be targeted; the ray now reaches the far pig.
        assert_eq!(e.raycast(eye, DVec3::X, 6.0).unwrap().0, 0);
    }

    #[test]
    fn spawn_rules() {
        assert!(can_spawn_on(MobKind::Pig, Block::GRASS, 1.0));
        assert!(can_spawn_on(MobKind::Pig, Block::GRASS, 0.1));
        assert!(!can_spawn_on(MobKind::Pig, Block::SAND, 1.0));
        assert!(can_spawn_on(MobKind::Zombie, Block::SAND, 0.12));
        assert!(!can_spawn_on(MobKind::Zombie, Block::SAND, 1.0), "not in daylight");
        assert!(!can_spawn_on(MobKind::Zombie, Block::LEAVES, 0.12));
        assert!(!can_spawn_on(MobKind::Zombie, Block::WATER, 0.12));
        let p = DVec3::ZERO;
        assert!(!in_spawn_ring(p, DVec3::new(10.0, 0.0, 0.0)));
        assert!(in_spawn_ring(p, DVec3::new(30.0, 0.0, 0.0)));
        assert!(!in_spawn_ring(p, DVec3::new(70.0, 0.0, 0.0)));

        // Natural spawning respects caps and the ring.
        let world = Grid::flat(64);
        let mut e = Entities::new(11);
        let player = DVec3::new(0.0, 64.0, 0.0);
        let c = Ctx { player_pos: player, player_targetable: false, daylight: 0.12, spawning: true };
        for _ in 0..600 {
            e.update(0.05, &world, &c);
        }
        // The grid is stone, so only hostile mobs spawn, each up to its cap.
        for kind in MobKind::ALL {
            let expected = if kind.is_hostile() { kind.spawn_cap() } else { 0 };
            assert_eq!(e.count(kind), expected, "{kind:?}");
        }
    }

    fn night(player: DVec3) -> Ctx {
        Ctx { player_pos: player, player_targetable: true, daylight: 0.1, spawning: false }
    }

    fn run(e: &mut Entities, world: &Grid, c: &Ctx, secs: f64) -> Vec<EntityEvent> {
        (0..(secs * 60.0) as usize).flat_map(|_| e.update(1.0 / 60.0, world, c)).collect()
    }

    #[test]
    fn creeper_hisses_then_explodes_unless_you_run() {
        let world = Grid::flat(10);
        let mut e = Entities::new(3);
        e.spawn(MobKind::Creeper, DVec3::new(0.5, 10.0, 0.5));
        let events = run(&mut e, &world, &night(DVec3::new(2.5, 10.0, 0.5)), 2.0);
        let fuse = events.iter().position(|ev| matches!(ev, EntityEvent::Sound { sound: MobSound::Fuse, .. }));
        let boom = events.iter().position(|ev| matches!(ev, EntityEvent::Explosion { .. }));
        assert!(fuse.unwrap() < boom.unwrap(), "{events:?}");
        assert!(e.mobs.is_empty(), "the creeper is gone");

        // Running out of range puts the fuse out.
        e.spawn(MobKind::Creeper, DVec3::new(0.5, 10.0, 0.5));
        run(&mut e, &world, &night(DVec3::new(2.5, 10.0, 0.5)), 0.5);
        assert!(e.mobs[0].fuse > 0.0);
        let events = run(&mut e, &world, &night(DVec3::new(20.5, 10.0, 0.5)), 1.5);
        assert!(!events.iter().any(|ev| matches!(ev, EntityEvent::Explosion { .. })));
        assert_eq!(e.mobs[0].fuse, 0.0);
    }

    #[test]
    fn skeleton_arrows_hit_the_player() {
        let world = Grid::flat(10);
        let mut e = Entities::new(4);
        e.spawn(MobKind::Skeleton, DVec3::new(0.5, 10.0, 0.5));
        let events = run(&mut e, &world, &night(DVec3::new(9.5, 10.0, 0.5)), 5.0);
        assert!(events.iter().any(|ev| matches!(ev, EntityEvent::Sound { sound: MobSound::Bow, .. })));
        let shot = |ev: &EntityEvent| matches!(ev, EntityEvent::PlayerHit { cause: "was shot by a skeleton", .. });
        assert!(events.iter().any(shot), "{events:?}");

        // Arrows that miss stick in the ground.
        let mut arrow = Arrow::aimed(DVec3::new(0.5, 12.0, 0.5), DVec3::new(6.0, 10.0, 0.5), &mut Rng::new(1));
        let c = ctx(DVec3::new(50.0, 10.0, 0.0));
        for _ in 0..120 {
            arrow.update(1.0 / 60.0, &world, &c, &mut Vec::new());
        }
        assert!(arrow.is_stuck() && (9.5..10.5).contains(&arrow.pos.y), "{:?}", arrow.pos);
    }

    #[test]
    fn spiders_hunt_at_night_or_when_hit_and_climb_walls() {
        let world = Grid::flat(10);
        let day = Ctx { daylight: 1.0, ..night(DVec3::new(5.5, 10.0, 0.5)) };
        let mut e = Entities::new(6);
        e.spawn(MobKind::Spider, DVec3::new(0.5, 10.0, 0.5));
        run(&mut e, &world, &day, 1.0);
        assert_ne!(e.mobs[0].ai, Ai::Chase, "calm in daylight");
        e.mobs[0].damage(1.0, None, &mut Rng::new(2));
        run(&mut e, &world, &day, 0.2);
        assert_eq!(e.mobs[0].ai, Ai::Chase, "provoked");
        let mut e = Entities::new(6);
        e.spawn(MobKind::Spider, DVec3::new(0.5, 10.0, 0.5));
        run(&mut e, &world, &night(DVec3::new(5.5, 10.0, 0.5)), 0.2);
        assert_eq!(e.mobs[0].ai, Ai::Chase, "hunts at night");

        // A 4-high wall stops a pig but not a spider, which climbs onto it
        // (and, wandering, won't jump off the far side).
        let mut world = Grid::flat(10);
        for y in 10..14 {
            for z in -3..=3 {
                world.set(IVec3::new(3, y, z), Block::STONE);
            }
        }
        let spider = walk_east(&world, Mob::new(MobKind::Spider, DVec3::new(0.5, 10.0, 0.5), 0.0), 4.0);
        assert!(spider.pos.y > 13.9 && spider.pos.x > 2.5, "spider at {:?}", spider.pos);
        let pig = walk_east(&world, Mob::new(MobKind::Pig, DVec3::new(0.5, 10.0, 0.5), 0.0), 4.0);
        assert!(pig.pos.x < 3.0);
    }

    #[test]
    fn chickens_flutter_down() {
        let world = Grid::flat(10);
        let mut chicken = Mob::new(MobKind::Chicken, DVec3::new(0.5, 30.0, 0.5), 0.0);
        let mut rng = Rng::new(1);
        for _ in 0..60 {
            chicken.update(1.0 / 60.0, &world, &ctx(DVec3::new(50.0, 10.0, 0.0)), &mut rng, &mut Vec::new());
        }
        assert!(chicken.vel.y >= -2.5 && chicken.pos.y > 26.0, "{:?}", chicken.pos);
    }

    #[test]
    fn explosions_fall_off_with_distance_and_kill_nearby_mobs() {
        assert_eq!(explosion_damage(3.0, 0.0), Some((43.0, 1.0)));
        assert_eq!(explosion_damage(3.0, 3.0).unwrap().0, 16.0);
        assert_eq!(explosion_damage(3.0, 6.0), None);

        let mut e = Entities::new(7);
        e.spawn(MobKind::Pig, DVec3::new(1.0, 10.0, 0.0));
        e.spawn(MobKind::Pig, DVec3::new(20.0, 10.0, 0.0));
        e.explode(DVec3::new(0.0, 10.5, 0.0), 3.0);
        assert!(!e.mobs[0].alive() && e.mobs[0].vel.x > 0.0);
        assert!(e.mobs[1].alive());
        assert!(!e.puffs.is_empty());
    }

    #[test]
    fn loot_rolls_stay_in_range() {
        let mut rng = Rng::new(9);
        for kind in MobKind::ALL {
            let mut seen_any = false;
            for _ in 0..200 {
                for (item, n) in kind.drops(&mut rng) {
                    let &(_, lo, hi) = kind.loot().iter().find(|l| l.0 == item).unwrap();
                    assert!((lo.max(1)..=hi).contains(&n), "{kind:?} dropped {n} of {}", item.name());
                    seen_any = true;
                }
            }
            assert!(seen_any, "{kind:?} never dropped anything");
        }
    }
}
