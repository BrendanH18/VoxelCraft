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

pub mod fireball;
pub mod item;
mod mob;
pub mod model;
pub mod orb;
pub mod pearl;
mod projectile;
pub mod tnt;

use std::f32::consts::TAU;

use glam::{DVec3, IVec3, Vec3};

use crate::physics::{self, BlockSource};
use crate::world::World;
use crate::world::block::Block;
use crate::world::noise::splitmix64;
use crate::world::terrain::Dimension;
use model::EntityVertex;

pub use item::ItemEntity;
pub use mob::{Mob, MobKind, sky_light};
pub use orb::XpOrb;
pub use projectile::Arrow;

/// Spawns happen this far from the player (blocks).
pub const SPAWN_MIN_DIST: f64 = 24.0;
pub const SPAWN_MAX_DIST: f64 = 64.0;
/// Mobs farther than this are removed.
pub const DESPAWN_DIST: f64 = 96.0;
/// Hostile mobs only spawn when it's darker than this.
pub const HOSTILE_SPAWN_DAYLIGHT: f32 = 0.35;
const SPAWN_INTERVAL: f32 = 0.25;
/// Spawners run while a player is this close (Java's required range), try
/// this many mobs at a time within `SPAWNER_REACH` blocks, and hold off
/// while `SPAWNER_CROWD` of their mob are nearby.
const SPAWNER_RANGE: f64 = 16.0;
const SPAWNER_TRIES: usize = 4;
const SPAWNER_REACH: f64 = 4.0;
const SPAWNER_CROWD: usize = 6;
/// Seconds between a spawner's spawns (Java's 200-799 ticks).
const SPAWNER_DELAY: (f32, f32) = (10.0, 40.0);
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
    /// An idle call (oink, moo, groan).
    Ambient(MobKind),
    Hurt(MobKind),
    Death(MobKind),
    /// An enderman someone looked in the eye.
    Scream,
    /// An enderman (or a pearl's thrower) vanishing or appearing.
    Teleport,
    /// A blaze shooting a fireball.
    Fireball,
}

/// Something an entity did that the game needs to react to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EntityEvent {
    /// A mob or arrow hit `player`: apply `damage` and add `knockback` to
    /// their velocity; `cause` is the death message.
    PlayerHit {
        player: PlayerId,
        damage: f32,
        knockback: Vec3,
        cause: &'static str,
    },
    /// A creeper or TNT exploded: break blocks and hurt everything nearby
    /// (see [`explosion_damage`]). [`Entities::explode`] handles the mobs;
    /// `cause` is the death message.
    Explosion {
        center: DVec3,
        power: f32,
        cause: &'static str,
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
    /// A blaze shot a fireball (turned into a projectile internally).
    Fireball {
        from: DVec3,
        dir: DVec3,
    },
    /// A fireball set `player` alight for `secs`.
    Ignite {
        player: PlayerId,
        secs: f32,
    },
    /// A fireball hit a block next to the empty `cell`: light a fire there.
    IgniteBlock {
        cell: IVec3,
    },
    /// A thrown ender pearl landed at `pos`: teleport its thrower there.
    PearlLanded {
        owner: PlayerId,
        pos: DVec3,
    },
    /// One of the player's arrows hit a mob (loot is dropped internally).
    MobShot {
        kind: MobKind,
        pos: DVec3,
        killed: bool,
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
    pub previous_pos: DVec3,
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
    /// Rain reaching the entity; dry biomes and roofs keep it alight.
    fn rains_on(&self, _p: IVec3) -> bool {
        false
    }
    /// Torch light in this cell, 0..=15.
    fn block_light(&self, _p: IVec3) -> u8 {
        0
    }
    /// Loaded spawner cages and the mob each makes.
    fn spawners(&self) -> Vec<(IVec3, MobKind)> {
        Vec::new()
    }
}

impl MobWorld for World {
    fn rains_on(&self, p: IVec3) -> bool {
        World::rains_on(self, p)
    }
    fn loaded(&self, p: IVec3) -> bool {
        self.is_loaded(p)
    }
    fn block_light(&self, p: IVec3) -> u8 {
        World::block_light(self, p)
    }
    fn surface(&self, x: i32, z: i32) -> Option<i32> {
        self.surface_height(x, z)
    }
    fn exposed(&self, p: IVec3) -> bool {
        self.sky_exposed(p)
    }
    fn spawners(&self) -> Vec<(IVec3, MobKind)> {
        World::spawners(self)
    }
}

/// Stable identity of a player within a world. The local player is
/// [`PlayerId::HOST`]; hosted agent profiles keep their own IDs in saves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlayerId(pub u32);

impl PlayerId {
    pub const HOST: Self = Self(0);
}

/// A player as the entity simulation sees them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub id: PlayerId,
    /// Feet position.
    pub pos: DVec3,
    /// Hostile mobs chase and attack (false in creative or while dead).
    pub targetable: bool,
    /// Experience orbs fly to living players only.
    pub alive: bool,
    /// Unit view direction (zero if unknown): endermen notice stares.
    pub look: DVec3,
}

impl Target {
    /// A living player; set [`Target::alive`] for one waiting to respawn.
    pub fn new(id: PlayerId, pos: DVec3, targetable: bool) -> Self {
        Self { id, pos, targetable, alive: true, look: DVec3::ZERO }
    }

    /// Whether `p` is inside this player's 0.6 x 1.8 box.
    pub fn contains(&self, p: DVec3) -> bool {
        let d = p - self.pos;
        d.x.abs() < crate::player::HALF_WIDTH
            && d.z.abs() < crate::player::HALF_WIDTH
            && (0.0..crate::player::HEIGHT).contains(&d.y)
    }
}

/// Per-tick inputs for the entity update.
pub struct Ctx {
    /// Every player in this world. Mobs spawn around, despawn away from and
    /// chase any of them.
    pub players: Vec<Target>,
    /// Skylight multiplier, 1 at noon.
    pub daylight: f32,
    /// Natural spawning on/off.
    pub spawning: bool,
    /// Rain keeps undead mobs from burning in the sun.
    pub raining: bool,
    /// Which mobs spawn, and where: on the surface under the Overworld sky,
    /// on cavern and island floors elsewhere.
    pub dimension: Dimension,
}

impl Ctx {
    /// Squared distance from `pos` to the closest player, if there is any.
    pub fn nearest_player_dist2(&self, pos: DVec3) -> Option<f64> {
        self.players.iter().map(|t| t.pos.distance_squared(pos)).min_by(f64::total_cmp)
    }

    /// The closest player hostile mobs may attack, like Minecraft's
    /// nearest-attackable-player targeting.
    pub fn nearest_target(&self, pos: DVec3) -> Option<&Target> {
        self.players
            .iter()
            .filter(|t| t.targetable)
            .min_by(|a, b| a.pos.distance_squared(pos).total_cmp(&b.pos.distance_squared(pos)))
    }
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
    pub pearls: Vec<pearl::Pearl>,
    pub fireballs: Vec<fireball::Fireball>,
    pub puffs: Vec<Puff>,
    pub tnt: Vec<tnt::PrimedTnt>,
    /// Dropped items. They stay put (and don't age) while their chunk is
    /// unloaded, and are saved with the world.
    pub items: Vec<ItemEntity>,
    /// Experience orbs, kept and saved like dropped items.
    pub orbs: Vec<XpOrb>,
    rng: Rng,
    /// Seconds until each active spawner tries again (not saved, like a
    /// fresh Java spawner's short first delay).
    spawner_delays: rustc_hash::FxHashMap<IVec3, f32>,
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
            pearls: Vec::new(),
            fireballs: Vec::new(),
            puffs: Vec::new(),
            tnt: Vec::new(),
            items: Vec::new(),
            orbs: Vec::new(),
            rng: Rng::new(seed ^ 0x6d6f_6273),
            spawner_delays: Default::default(),
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

    /// Snapshot positions and advance entity simulation by `dt` game seconds.
    /// Return events for the caller to apply world edits, damage, loot and sounds.
    pub fn update<W: MobWorld + ?Sized>(&mut self, dt: f64, world: &W, ctx: &Ctx) -> Vec<EntityEvent> {
        self.snapshot_positions();
        let mut events = Vec::new();
        if ctx.spawning {
            self.spawn_timer -= dt as f32;
            if self.spawn_timer <= 0.0 {
                self.spawn_timer = SPAWN_INTERVAL;
                self.natural_spawn(world, ctx);
            }
        }
        self.run_spawners(dt as f32, world, ctx);

        let mut i = 0;
        while i < self.mobs.len() {
            let m = &self.mobs[i];
            let gone = m.dying.is_some_and(|t| t >= mob::DEATH_TIME)
                || ctx.nearest_player_dist2(m.pos).is_some_and(|d| d > DESPAWN_DIST * DESPAWN_DIST)
                || !world.loaded(m.pos.floor().as_ivec3());
            if gone {
                self.mobs.swap_remove(i);
                continue;
            }
            self.mobs[i].update(dt, world, ctx, &mut self.rng, &mut events);
            i += 1;
        }
        self.separate(dt);

        // Skeleton shots become arrows, blaze shots fireballs.
        for e in &events {
            match *e {
                EntityEvent::Shoot { from, target } => self.arrows.push(Arrow::aimed(from, target, &mut self.rng)),
                EntityEvent::Fireball { from, dir } => self.fireballs.push(fireball::Fireball::new(from, dir)),
                _ => {}
            }
        }
        events.retain(|e| !matches!(e, EntityEvent::Shoot { .. } | EntityEvent::Fireball { .. }));
        self.fireballs.retain_mut(|f| f.update(dt, world, ctx, &mut events));
        let (mobs, rng) = (&mut self.mobs, &mut self.rng);
        self.arrows.retain_mut(|a| a.update(dt, world, ctx, mobs, rng, &mut events));
        self.pearls.retain_mut(|p| p.update(dt, world, mobs, rng, &mut events));
        for e in &events {
            if let EntityEvent::MobShot { kind, pos, killed } = *e {
                if killed {
                    self.drop_loot(kind, pos);
                }
                if kind == MobKind::ZombifiedPiglin {
                    self.anger_piglins(pos);
                }
            }
        }
        self.items.retain_mut(|item| !world.loaded(item.pos.floor().as_ivec3()) || item.update(dt, world));
        self.orbs.retain_mut(|orb| !world.loaded(orb.pos.floor().as_ivec3()) || orb.update(dt, world, ctx));
        self.tnt.retain_mut(|t| t.update(dt, world, &mut events));
        self.merge_timer -= dt as f32;
        if self.merge_timer <= 0.0 {
            self.merge_timer = MERGE_INTERVAL;
            item::merge(&mut self.items);
            orb::merge(&mut self.orbs);
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
                previous_pos: center + dir * 0.6,
                vel: dir * self.rng.range(3.0, 8.0) as f64,
                age: 0.0,
                life: self.rng.range(0.6, 1.3),
                size: self.rng.range(0.4, 1.0),
            });
        }
    }

    /// Drops the loot and experience of a mob of `kind` the player killed
    /// at `pos`.
    pub fn drop_loot(&mut self, kind: MobKind, pos: DVec3) {
        let xp = kind.xp(&mut self.rng);
        self.spawn_xp(pos, xp);
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

    /// Experience orbs worth `points` in all, split into Java's orb sizes,
    /// popping out at `pos`.
    pub fn spawn_xp(&mut self, pos: DVec3, points: u32) {
        for value in crate::simulation::experience::orb_values(points) {
            self.orbs.push(XpOrb::new(value, pos, &mut self.rng));
        }
    }

    /// Experience for a block a player harvested at `cell` (ores).
    pub fn drop_block_xp(&mut self, block: Block, cell: IVec3) {
        let xp = crate::mining::ore_xp(block, &mut self.rng);
        self.spawn_xp(cell.as_dvec3() + DVec3::new(0.5, 0.25, 0.5), xp);
    }

    /// A uniform number in 0..1 for rounding fractional awards.
    pub fn roll(&mut self) -> f32 {
        self.rng.next_f32()
    }

    /// `;`-separated experience orbs for the level file.
    pub fn orbs_to_string(&self) -> String {
        self.orbs.iter().map(XpOrb::serialize).collect::<Vec<_>>().join(";")
    }

    pub fn load_orbs(&mut self, text: &str) {
        let rng = &mut self.rng;
        self.orbs.extend(text.split(';').filter(|s| !s.is_empty()).filter_map(|s| XpOrb::deserialize(s, rng)));
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

    /// One spawn attempt per mob type around each player whose local cap
    /// isn't full. Like Java Edition's per-player mob caps, mobs only count
    /// against the players they are near, so distant players don't starve
    /// each other's spawns. Mobs never appear close to any player.
    fn natural_spawn<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx) {
        for center in ctx.players.iter().map(|t| t.pos) {
            for kind in MobKind::ALL {
                let cap = kind.spawn_cap(ctx.dimension);
                if self.count_near(kind, center) >= cap
                    || !kind.spawns_in(ctx.dimension)
                    || !self.rng.chance(kind.spawn_chance(ctx.dimension))
                {
                    continue;
                }
                let angle = self.rng.range(0.0, TAU) as f64;
                let dist = self.rng.range(SPAWN_MIN_DIST as f32, SPAWN_MAX_DIST as f32) as f64;
                let x = (center.x + angle.cos() * dist).floor() as i32;
                let z = (center.z + angle.sin() * dist).floor() as i32;
                let spot = match ctx.dimension {
                    Dimension::Overworld => spawn_spot(world, kind, x, z, ctx.daylight),
                    Dimension::Nether => cavern_spot(world, kind, x, z, self.rng.range(40.0, 118.0) as i32),
                    Dimension::End => cavern_spot(world, kind, x, z, self.rng.range(30.0, 90.0) as i32),
                };
                let Some(pos) = spot else { continue };
                if !in_spawn_ring(center, pos) || !clear_of_players(ctx, pos) {
                    continue;
                }
                self.spawn(kind, pos);
                // Animals come in small herds, zombified piglins and End
                // endermen in packs.
                if !kind.is_hostile() || ctx.dimension != Dimension::Overworld {
                    let extra = (self.rng.next_f32() * 3.0) as i32;
                    for _ in 0..extra {
                        let (dx, dz) = ((self.rng.range(-3.0, 3.0)) as i32, (self.rng.range(-3.0, 3.0)) as i32);
                        let spot = if ctx.dimension.has_sky() {
                            spawn_spot(world, kind, x + dx, z + dz, ctx.daylight)
                        } else {
                            cavern_spot(world, kind, x + dx, z + dz, pos.y as i32 + 2)
                        };
                        if self.count_near(kind, center) < cap
                            && let Some(p) = spot
                            && clear_of_players(ctx, p)
                        {
                            self.spawn(kind, p);
                        }
                    }
                }
            }
        }
    }

    /// Java's spawner logic: a spawner with a player within 16 blocks waits
    /// out its delay, then tries four spots up to 4 blocks away (and a block
    /// up or down) with room for its mob, unless six are already around.
    /// Light and ground don't matter. Active cages give off flames.
    fn run_spawners<W: MobWorld + ?Sized>(&mut self, dt: f32, world: &W, ctx: &Ctx) {
        let spawners = world.spawners();
        self.spawner_delays.retain(|p, _| spawners.iter().any(|(q, _)| q == p));
        for (cell, kind) in spawners {
            let centre = cell.as_dvec3() + DVec3::splat(0.5);
            if !ctx.players.iter().any(|t| t.alive && t.pos.distance_squared(centre) < SPAWNER_RANGE * SPAWNER_RANGE) {
                continue;
            }
            if self.rng.chance(dt * 6.0) {
                let p = centre + DVec3::new(self.rng.range(-0.4, 0.4) as f64, self.rng.range(-0.4, 0.4) as f64, 0.0);
                self.puffs.push(Puff { pos: p, previous_pos: p, vel: DVec3::Y * 0.5, age: 0.0, life: 0.6, size: 0.15 });
            }
            let delay = self.spawner_delays.entry(cell).or_insert(1.0);
            *delay -= dt;
            if *delay > 0.0 {
                continue;
            }
            *delay = 0.0;
            let reach = SPAWNER_REACH + 0.5;
            let crowd = self
                .mobs
                .iter()
                .filter(|m| m.kind == kind && m.alive() && (m.pos - centre).abs().cmple(DVec3::splat(reach)).all())
                .count();
            let mut spawned = crowd >= SPAWNER_CROWD;
            for _ in 0..SPAWNER_TRIES.min(SPAWNER_CROWD.saturating_sub(crowd)) {
                let mut r = || (self.rng.next_f32() - self.rng.next_f32()) as f64 * SPAWNER_REACH;
                let (dx, dz) = (r(), r());
                let dy = (self.rng.next_f32() * 3.0).floor() as f64 - 1.0;
                let pos = DVec3::new(centre.x + dx, cell.y as f64 + dy, centre.z + dz);
                let shape = kind.shape();
                if !world.loaded(pos.floor().as_ivec3())
                    || physics::overlaps_solid(world, pos, shape)
                    || physics::is_fluid_at(world, pos)
                {
                    continue;
                }
                self.spawn(kind, pos);
                spawned = true;
            }
            if spawned {
                let next = self.rng.range(SPAWNER_DELAY.0, SPAWNER_DELAY.1);
                self.spawner_delays.insert(cell, next);
            }
        }
    }

    /// Living mobs of `kind` that count against the cap of a player at
    /// `center` (those within despawn range).
    fn count_near(&self, kind: MobKind, center: DVec3) -> usize {
        let r2 = DESPAWN_DIST * DESPAWN_DIST;
        self.mobs.iter().filter(|m| m.kind == kind && m.alive() && m.pos.distance_squared(center) <= r2).count()
    }

    /// Capture positions before a game tick, or snap them while paused.
    pub fn snapshot_positions(&mut self) {
        for m in &mut self.mobs {
            m.previous_pos = m.pos;
        }
        for a in &mut self.arrows {
            a.previous_pos = a.pos;
        }
        for p in &mut self.pearls {
            p.previous_pos = p.pos;
        }
        for f in &mut self.fireballs {
            f.previous_pos = f.pos;
        }
        for item in &mut self.items {
            item.previous_pos = item.pos;
        }
        for orb in &mut self.orbs {
            orb.previous_pos = orb.pos;
        }
        for t in &mut self.tnt {
            t.previous_pos = t.pos;
        }
        for p in &mut self.puffs {
            p.previous_pos = p.pos;
        }
    }

    /// Camera-relative triangles for every visible mob.
    pub fn mesh(
        &mut self,
        camera: DVec3,
        forward: Vec3,
        max_dist: f32,
        time: f32,
        alpha: f64,
    ) -> &mut Vec<EntityVertex> {
        self.verts.clear();
        self.rendered = model::build(&self.mobs, camera, forward, max_dist, time, alpha, &mut self.verts);
        model::build_arrows(&self.arrows, camera, alpha, &mut self.verts);
        model::build_pearls(&self.pearls, camera, alpha, &mut self.verts);
        model::build_fireballs(&self.fireballs, camera, time, alpha, &mut self.verts);
        model::build_puffs(&self.puffs, camera, alpha, &mut self.verts);
        model::build_orbs(&self.orbs, camera, max_dist, time, alpha, &mut self.verts);
        &mut self.verts
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

    /// Lights the TNT block at `cell` (now gone from the world): a short
    /// random fuse if a blast `chained` it, the full four seconds otherwise.
    pub fn prime_tnt(&mut self, cell: IVec3, chained: bool) {
        let fuse = if chained { self.rng.range(0.5, 1.5) } else { tnt::FUSE };
        let a = self.rng.range(0.0, TAU);
        let vel = DVec3::new(a.cos() as f64 * 0.4, 2.0, a.sin() as f64 * 0.4);
        self.tnt.push(tnt::PrimedTnt::new(cell.as_dvec3() + DVec3::new(0.5, 0.0, 0.5), vel, fuse));
    }

    /// Player `owner` throws an ender pearl from `eye` along `dir`, carrying
    /// their velocity `carry`.
    pub fn throw_pearl(&mut self, owner: PlayerId, eye: DVec3, dir: DVec3, carry: DVec3) {
        self.pearls.push(pearl::Pearl::thrown(owner, eye, dir, carry, &mut self.rng));
    }

    /// The player looses an arrow with bow `power` 0..1.
    pub fn shoot_arrow(&mut self, eye: DVec3, dir: DVec3, power: f32, pickup: bool) {
        self.arrows.push(Arrow::shot(eye, dir, power, pickup));
    }

    /// Stuck player arrows within reach of a player at `feet`, removed.
    /// Returns how many were collected (at most 255 at a time; the rest
    /// stay for the next call).
    pub fn collect_arrows(&mut self, feet: DVec3) -> u8 {
        let mut taken: u8 = 0;
        self.arrows.retain(|a| {
            let near = (a.pos - (feet + DVec3::Y * 0.9)).abs().cmple(DVec3::new(1.3, 1.5, 1.3)).all();
            let take = a.pickup && a.is_stuck() && near && taken < u8::MAX;
            taken += take as u8;
            !take
        });
        taken
    }

    /// Player melee hit for `damage` on mob `index`, pushed along `dir`.
    /// Returns the kind of mob if this killed it.
    pub fn attack(&mut self, index: usize, dir: DVec3, damage: f32) -> Option<MobKind> {
        let flat = DVec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
        let knockback = flat * 6.0 + DVec3::Y * 5.0;
        let mob = self.mobs.get_mut(index)?;
        let (kind, pos) = (mob.kind, mob.pos);
        let killed = mob.damage(damage, Some(knockback), &mut self.rng);
        if kind == MobKind::ZombifiedPiglin {
            self.anger_piglins(pos);
        }
        killed.then_some(kind)
    }

    /// Hitting one zombified piglin angers every one nearby.
    fn anger_piglins(&mut self, at: DVec3) {
        for m in &mut self.mobs {
            if m.kind == MobKind::ZombifiedPiglin && m.pos.distance_squared(at) < 32.0 * 32.0 {
                m.anger(mob::PIGLIN_ANGER_TIME);
            }
        }
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

/// No player is closer than [`SPAWN_MIN_DIST`] to `pos`.
fn clear_of_players(ctx: &Ctx, pos: DVec3) -> bool {
    ctx.nearest_player_dist2(pos).is_none_or(|d| d >= SPAWN_MIN_DIST * SPAWN_MIN_DIST)
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

/// Feet position on the first floor at or below `top` in column (x, z)
/// with room to stand (for cavern dimensions with no sky).
fn cavern_spot<W: MobWorld + ?Sized>(world: &W, kind: MobKind, x: i32, z: i32, top: i32) -> Option<DVec3> {
    let floor = (top - 24..=top).rev().find(|&y| {
        world.block(IVec3::new(x, y, z)).is_some_and(|b| b.is_solid() && b.is_opaque())
            && world.block(IVec3::new(x, y + 1, z)) == Some(Block::AIR)
    })?;
    let pos = DVec3::new(x as f64 + 0.5, floor as f64 + 1.0, z as f64 + 0.5);
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
        Ctx {
            players: vec![Target::new(PlayerId::HOST, player, false)],
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
        }
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

    fn is_hit(ev: &EntityEvent) -> bool {
        matches!(ev, EntityEvent::PlayerHit { .. })
    }

    #[test]
    fn mobs_cry_when_hurt_and_call_when_idle() {
        let world = Grid::flat(10);
        let mut e = Entities::new(6);
        e.spawn(MobKind::Cow, DVec3::new(0.5, 10.0, 0.5));
        let c = ctx(DVec3::new(30.0, 10.0, 0.0));
        let sounds = |events: Vec<EntityEvent>| -> Vec<MobSound> {
            events
                .into_iter()
                .filter_map(|ev| if let EntityEvent::Sound { sound, .. } = ev { Some(sound) } else { None })
                .collect()
        };
        let idle = sounds(run(&mut e, &world, &c, 20.0));
        assert!(idle.contains(&MobSound::Ambient(MobKind::Cow)), "{idle:?}");
        e.attack(0, DVec3::X, 1.0);
        assert_eq!(sounds(e.update(1.0 / 60.0, &world, &c)), vec![MobSound::Hurt(MobKind::Cow)]);
        e.mobs[0].hurt = 0.0;
        e.attack(0, DVec3::X, 100.0);
        assert!(sounds(e.update(1.0 / 60.0, &world, &c)).contains(&MobSound::Death(MobKind::Cow)));
    }

    #[test]
    fn zombie_attacks_with_cooldown() {
        let world = Grid::flat(10);
        let mut e = Entities::new(5);
        e.spawn(MobKind::Zombie, DVec3::new(0.5, 10.0, 0.5));
        let player = DVec3::new(1.5, 10.0, 0.5);
        let c = Ctx {
            players: vec![Target::new(PlayerId::HOST, player, true)],
            daylight: 0.1,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
        };
        let mut hits = Vec::new();
        for _ in 0..90 {
            hits.extend(e.update(1.0 / 60.0, &world, &c).into_iter().filter(is_hit));
        }
        // 1.5 s: an immediate hit plus one after the 1 s cooldown.
        assert_eq!(hits.len(), 2, "{hits:?}");
        let EntityEvent::PlayerHit { player: id, damage, knockback, cause } = hits[0] else { panic!("{hits:?}") };
        assert_eq!(id, PlayerId::HOST);
        assert!(damage > 0.0 && knockback.x > 0.0 && knockback.y > 0.0);
        assert_eq!(cause, "was slain by a zombie");

        // Creative players are ignored.
        let c = Ctx { players: vec![Target::new(PlayerId::HOST, player, false)], ..c };
        assert!((0..120).all(|_| !e.update(1.0 / 60.0, &world, &c).iter().any(is_hit)));
    }

    #[test]
    fn zombie_gets_around_a_pillar() {
        let mut world = Grid::flat(10);
        for y in 10..14 {
            world.set(IVec3::new(3, y, 0), Block::STONE);
        }
        let mut e = Entities::new(8);
        e.spawn(MobKind::Zombie, DVec3::new(0.5, 10.0, 0.5));
        let c = Ctx {
            players: vec![Target::new(PlayerId::HOST, DVec3::new(8.5, 10.0, 0.5), true)],
            daylight: 0.1,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
        };
        let hit = (0..60 * 10).any(|_| e.update(1.0 / 60.0, &world, &c).iter().any(is_hit));
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
    fn fire_ignites_mobs_persists_after_contact_and_spares_nether_piglins() {
        let mut world = Grid::flat(10);
        let at = DVec3::new(0.5, 10.0, 0.5);
        world.set(at.floor().as_ivec3(), Block::FIRE);
        let mut e = Entities::new(5);
        e.spawn(MobKind::Pig, at);
        let c = Ctx { daylight: 0.0, ..ctx(DVec3::new(30.0, 10.0, 0.0)) };
        e.update(1.0 / 60.0, &world, &c);
        assert!(e.mobs[0].burning);
        assert_eq!(e.mobs[0].health, 9.0, "contact hit does not apply every frame");
        e.update(1.0 / 60.0, &world, &c);
        assert_eq!(e.mobs[0].health, 9.0);
        e.mobs[0].pos = DVec3::new(10.5, 10.0, 10.5);
        e.mobs[0].vel = DVec3::ZERO;
        run(&mut e, &world, &c, 1.1);
        assert!(e.mobs[0].burning && e.mobs[0].health < 9.0, "burns after walking away");
        e.mobs[0].pos = DVec3::new(20.5, 10.0, 20.5);
        world.set(e.mobs[0].pos.floor().as_ivec3(), Block::WATER);
        e.update(1.0 / 60.0, &world, &c);
        assert!(!e.mobs[0].burning, "water extinguishes");

        for b in [Block::FIRE, Block::LAVA] {
            world.set(at.floor().as_ivec3(), b);
            e.spawn(MobKind::ZombifiedPiglin, at);
            e.update(0.05, &world, &c);
            let piglin = e.mobs.last().unwrap();
            assert!(!piglin.burning);
            assert_eq!(piglin.health, MobKind::ZombifiedPiglin.max_health());
        }
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
        let c = Ctx {
            players: vec![Target::new(PlayerId::HOST, player, false)],
            daylight: 0.12,
            spawning: true,
            raining: false,
            dimension: Dimension::Overworld,
        };
        for _ in 0..600 {
            e.update(0.05, &world, &c);
        }
        // The grid is stone, so only hostile mobs spawn, each up to its cap.
        for kind in MobKind::ALL {
            let o = Dimension::Overworld;
            let expected = if kind.is_hostile() && kind.spawns_in(o) { kind.spawn_cap(o) } else { 0 };
            assert_eq!(e.count(kind), expected, "{kind:?}");
        }
    }

    #[test]
    fn hostile_mobs_chase_the_nearest_targetable_player() {
        let world = Grid::flat(10);
        let mut e = Entities::new(5);
        e.spawn(MobKind::Zombie, DVec3::new(0.5, 10.0, 0.5));
        let (host, agent) = (PlayerId::HOST, PlayerId(7));
        // A creative host right next to the zombie is ignored in favour of
        // the survival agent a few blocks away.
        let c = Ctx {
            players: vec![
                Target::new(host, DVec3::new(1.5, 10.0, 0.5), false),
                Target::new(agent, DVec3::new(-4.5, 10.0, 0.5), true),
            ],
            ..night(DVec3::ZERO)
        };
        let hits: Vec<_> = run(&mut e, &world, &c, 3.0).into_iter().filter(is_hit).collect();
        assert!(!hits.is_empty(), "zombie at {:?}", e.mobs[0].pos);
        assert!(hits.iter().all(|h| matches!(h, EntityEvent::PlayerHit { player, .. } if *player == agent)));
        assert!(e.mobs[0].pos.x < 0.0, "walked toward the agent: {:?}", e.mobs[0].pos);
    }

    #[test]
    fn skeleton_arrows_hit_whichever_player_they_reach() {
        let world = Grid::flat(10);
        let mut e = Entities::new(4);
        e.spawn(MobKind::Skeleton, DVec3::new(0.5, 10.0, 0.5));
        let agent = PlayerId(3);
        let c = Ctx {
            players: vec![
                Target::new(PlayerId::HOST, DVec3::new(60.5, 10.0, 0.5), true),
                Target::new(agent, DVec3::new(9.5, 10.0, 0.5), true),
            ],
            ..night(DVec3::ZERO)
        };
        let events = run(&mut e, &world, &c, 5.0);
        let shots: Vec<_> = events
            .iter()
            .filter_map(|ev| match ev {
                EntityEvent::PlayerHit { player, cause: "was shot by a skeleton", .. } => Some(*player),
                _ => None,
            })
            .collect();
        assert!(!shots.is_empty() && shots.iter().all(|&p| p == agent), "{events:?}");
    }

    #[test]
    fn mobs_stay_while_any_player_is_near() {
        let world = Grid::flat(10);
        let mut e = Entities::new(5);
        e.spawn(MobKind::Pig, DVec3::new(500.5, 10.0, 0.5));
        let far_host = Target::new(PlayerId::HOST, DVec3::new(0.5, 10.0, 0.5), false);
        let c = Ctx {
            players: vec![far_host, Target::new(PlayerId(1), DVec3::new(510.5, 10.0, 0.5), false)],
            ..ctx(DVec3::ZERO)
        };
        e.update(0.05, &world, &c);
        assert_eq!(e.mobs.len(), 1, "kept by the nearby agent");
        let c = Ctx { players: vec![far_host], ..c };
        e.update(0.05, &world, &c);
        assert!(e.mobs.is_empty(), "despawns once nobody is near");
    }

    #[test]
    fn distant_players_get_their_own_mob_caps() {
        let world = Grid::flat(64);
        let mut e = Entities::new(11);
        let (a, b) = (DVec3::new(0.0, 64.0, 0.0), DVec3::new(1000.0, 64.0, 0.0));
        let c = Ctx {
            players: vec![Target::new(PlayerId::HOST, a, false), Target::new(PlayerId(1), b, false)],
            daylight: 0.12,
            spawning: true,
            raining: false,
            dimension: Dimension::Overworld,
        };
        for _ in 0..1200 {
            let before = e.mobs.len();
            e.update(0.05, &world, &c);
            // Nothing despawns here, so new mobs are the ones at the end;
            // they appear outside the spawn radius (and may wander in later).
            for m in &e.mobs[before..] {
                let d = c.nearest_player_dist2(m.pos).unwrap().sqrt();
                assert!(d >= SPAWN_MIN_DIST, "{:?} spawned {d} from a player", m.kind);
            }
        }
        let o = Dimension::Overworld;
        for kind in MobKind::ALL.into_iter().filter(|k| k.is_hostile() && k.spawns_in(o)) {
            for center in [a, b] {
                assert_eq!(e.count_near(kind, center), kind.spawn_cap(o), "{kind:?} near {center}");
            }
        }
    }

    #[test]
    fn spawning_avoids_every_player() {
        let world = Grid::flat(64);
        // A second player stands in the first one's spawn ring.
        let c = Ctx {
            players: vec![
                Target::new(PlayerId::HOST, DVec3::new(0.0, 64.0, 0.0), false),
                Target::new(PlayerId(1), DVec3::new(40.0, 64.0, 0.0), false),
            ],
            daylight: 0.12,
            spawning: true,
            raining: false,
            dimension: Dimension::Overworld,
        };
        let mut e = Entities::new(21);
        let mut spawned = 0;
        for _ in 0..1200 {
            let before = e.mobs.len();
            e.update(0.05, &world, &c);
            // Removals only shrink the list, so anything past `before` is new.
            for m in e.mobs.iter().skip(before) {
                let d = c.nearest_player_dist2(m.pos).unwrap().sqrt();
                assert!(d >= SPAWN_MIN_DIST - 0.5, "{:?} spawned {d:.1} blocks from a player", m.kind);
                spawned += 1;
            }
        }
        assert!(spawned > 10, "{spawned}");
    }

    fn night(player: DVec3) -> Ctx {
        Ctx {
            players: vec![Target::new(PlayerId::HOST, player, true)],
            daylight: 0.1,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
        }
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
            arrow.update(1.0 / 60.0, &world, &c, &mut [], &mut Rng::new(1), &mut Vec::new());
        }
        assert!(arrow.is_stuck() && (9.5..10.5).contains(&arrow.pos.y), "{:?}", arrow.pos);
    }

    #[test]
    fn player_arrows_hit_mobs_and_can_be_collected() {
        let world = Grid::flat(10);
        let mut e = Entities::new(4);
        e.spawn(MobKind::Zombie, DVec3::new(10.5, 10.0, 0.5));
        let c = night(DVec3::new(0.5, 10.0, 0.5));
        e.shoot_arrow(DVec3::new(0.5, 11.6, 0.5), DVec3::X, 1.0, true);
        let events = run(&mut e, &world, &c, 0.5);
        let hit = events.iter().any(|ev| matches!(ev, EntityEvent::MobShot { kind: MobKind::Zombie, .. }));
        assert!(hit, "{events:?}");
        assert!(e.mobs[0].health < MobKind::Zombie.max_health(), "zombie hurt");
        assert!(!events.iter().any(is_hit), "the shooter isn't hit");

        // A miss sticks in the ground and is picked up by walking over it.
        let mut e = Entities::new(5);
        e.shoot_arrow(DVec3::new(0.5, 11.6, 0.5), DVec3::new(1.0, -0.6, 0.0), 0.5, true);
        run(&mut e, &world, &c, 2.0);
        assert!(e.arrows[0].is_stuck());
        let spot = e.arrows[0].pos.with_y(10.0);
        assert_eq!(e.collect_arrows(spot + DVec3::X * 5.0), 0);
        assert_eq!(e.collect_arrows(spot), 1);
        assert!(e.arrows.is_empty());

        // More than 255 at once: the rest wait for the next pickup.
        for _ in 0..300 {
            e.shoot_arrow(DVec3::new(0.5, 11.6, 0.5), DVec3::new(1.0, -0.6, 0.0), 0.5, true);
        }
        run(&mut e, &world, &c, 2.0);
        assert_eq!(e.collect_arrows(spot), 255);
        assert_eq!(e.collect_arrows(spot), 45);
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
    fn zombified_piglins_ignore_you_until_one_is_hit() {
        let world = Grid::flat(10);
        let mut e = Entities::new(12);
        e.spawn(MobKind::ZombifiedPiglin, DVec3::new(0.5, 10.0, 0.5));
        e.spawn(MobKind::ZombifiedPiglin, DVec3::new(8.5, 10.0, 8.5));
        let c = Ctx { dimension: Dimension::Nether, ..night(DVec3::new(2.5, 10.0, 0.5)) };
        assert!(!run(&mut e, &world, &c, 3.0).iter().any(is_hit), "neutral");
        e.attack(0, DVec3::X, 1.0);
        let events = run(&mut e, &world, &c, 3.0);
        let slain =
            |ev: &EntityEvent| matches!(ev, EntityEvent::PlayerHit { cause: "was slain by a zombified piglin", .. });
        assert!(events.iter().any(slain), "{events:?}");
        assert_eq!(e.mobs[1].ai, Ai::Chase, "the whole pack is angry");
        assert_eq!(MobKind::from_name("zombified_piglin"), Some(MobKind::ZombifiedPiglin));
    }

    #[test]
    fn endermen_turn_on_a_stare_freeze_while_watched_and_dodge_arrows() {
        let world = Grid::flat(10);
        let mut e = Entities::new(14);
        e.spawn(MobKind::Enderman, DVec3::new(10.5, 10.0, 0.5));
        let player = DVec3::new(0.5, 10.0, 0.5);
        let away = Target { look: DVec3::NEG_X, ..Target::new(PlayerId::HOST, player, true) };
        let calm = Ctx { players: vec![away], ..night(player) };
        let events = run(&mut e, &world, &calm, 2.0);
        assert!(e.mobs[0].ai != Ai::Chase && !events.iter().any(is_hit), "neutral until looked at");

        // Look it in the eyes: it screams and turns hostile.
        let eye = player + DVec3::Y * crate::player::EYE_HEIGHT;
        let gaze = |m: &Mob| (m.pos + DVec3::Y * 2.55 - eye).normalize();
        let staring = Ctx { players: vec![Target { look: gaze(&e.mobs[0]), ..away }], ..night(player) };
        let events = run(&mut e, &world, &staring, 0.5);
        let scream = |ev: &EntityEvent| matches!(ev, EntityEvent::Sound { sound: MobSound::Scream, .. });
        assert!(events.iter().any(scream), "{events:?}");
        assert_eq!(e.mobs[0].ai, Ai::Chase);
        // Frozen while watched from within 16 blocks.
        let at = e.mobs[0].pos;
        let staring = Ctx { players: vec![Target { look: gaze(&e.mobs[0]), ..away }], ..night(player) };
        run(&mut e, &world, &staring, 1.0);
        assert!(e.mobs[0].pos.distance(at) < 0.05, "froze: {at} -> {}", e.mobs[0].pos);
        // Look away and it closes in to hit hard.
        let events = run(&mut e, &world, &calm, 4.0);
        let slain = |ev: &EntityEvent| matches!(ev, EntityEvent::PlayerHit { damage: 7.0, .. });
        assert!(events.iter().any(slain), "{events:?}");

        // Arrows make it teleport away unhurt.
        let mut e = Entities::new(15);
        e.spawn(MobKind::Enderman, DVec3::new(6.5, 10.0, 0.5));
        e.shoot_arrow(DVec3::new(0.5, 11.5, 0.5), DVec3::X, 1.0, false);
        let events = run(&mut e, &world, &calm, 0.5);
        let m = &e.mobs[0];
        assert_eq!(m.health, MobKind::Enderman.max_health());
        assert!(m.pos.distance(DVec3::new(6.5, 10.0, 0.5)) > 1.0, "teleported: {}", m.pos);
        assert!((m.pos.y - 10.0).abs() < 1e-3, "onto the ground: {}", m.pos);
        let vwoop = |ev: &EntityEvent| matches!(ev, EntityEvent::Sound { sound: MobSound::Teleport, .. });
        assert!(events.iter().any(vwoop));
        assert_eq!(MobKind::Enderman.loot(), [(crate::item::Item::ENDER_PEARL, 0, 1)]);
    }

    #[test]
    fn water_hurts_endermen_and_makes_them_teleport_without_anger() {
        let mut world = Grid::flat(10);
        for x in -2..=2 {
            for z in -2..=2 {
                world.set(IVec3::new(x, 10, z), Block::WATER);
                world.set(IVec3::new(x, 11, z), Block::WATER);
            }
        }
        let mut e = Entities::new(16);
        e.spawn(MobKind::Enderman, DVec3::new(0.5, 10.0, 0.5));
        let c = night(DVec3::new(40.5, 10.0, 0.5));
        run(&mut e, &world, &c, 3.0);
        let m = &e.mobs[0];
        assert!(m.health < MobKind::Enderman.max_health(), "water hurts");
        assert!(!m.in_water && m.pos.distance(DVec3::new(0.5, 10.0, 0.5)) > 2.0, "escaped: {}", m.pos);
        assert_ne!(m.ai, Ai::Chase, "water doesn't anger it");
    }

    /// A test world with spawner cages.
    struct Caged(Grid, Vec<(IVec3, MobKind)>);

    impl BlockSource for Caged {
        fn block(&self, p: IVec3) -> Option<Block> {
            self.0.block(p)
        }
    }

    impl MobWorld for Caged {
        fn loaded(&self, _: IVec3) -> bool {
            true
        }
        fn surface(&self, x: i32, z: i32) -> Option<i32> {
            self.0.surface(x, z)
        }
        fn exposed(&self, p: IVec3) -> bool {
            self.0.exposed(p)
        }
        fn spawners(&self) -> Vec<(IVec3, MobKind)> {
            self.1.clone()
        }
    }

    #[test]
    fn spawners_ignore_dead_players_and_pause_the_delay() {
        let cell = IVec3::new(0, 11, 0);
        let world = Caged(Grid::flat(10), vec![(cell, MobKind::Blaze)]);
        let mut e = Entities::new(17);
        let mut c = ctx(DVec3::new(10.5, 10.0, 0.5));
        c.players[0].alive = false;
        e.run_spawners(2.0, &world, &c);
        assert!(e.mobs.is_empty() && e.puffs.is_empty() && e.spawner_delays.is_empty());

        c.players[0].alive = true;
        e.run_spawners(0.5, &world, &c);
        assert_eq!(e.spawner_delays[&cell], 0.5);
        e.puffs.clear();
        c.players[0].alive = false;
        e.run_spawners(20.0, &world, &c);
        assert_eq!(e.spawner_delays[&cell], 0.5, "death pauses an active cage");
        assert!(e.mobs.is_empty() && e.puffs.is_empty());

        c.players[0].alive = true;
        e.run_spawners(0.6, &world, &c);
        assert!((1..=4).contains(&e.count(MobKind::Blaze)), "living creative players activate cages too");
    }

    #[test]
    fn spawners_work_near_players_up_to_six_mobs() {
        // A closed room around the cage, so nothing wanders off.
        let mut room = Grid::flat(10);
        for a in -5..=5 {
            for y in 10..=17 {
                for (x, z) in [(a, -5), (a, 5), (-5, a), (5, a)] {
                    room.set(IVec3::new(x, y, z), Block::STONE);
                }
            }
            for b in -5..=5 {
                room.set(IVec3::new(a, 17, b), Block::STONE);
            }
        }
        let world = Caged(room, vec![(IVec3::new(0, 11, 0), MobKind::Blaze)]);
        let mut e = Entities::new(17);
        let far = Ctx {
            players: vec![Target::new(PlayerId::HOST, DVec3::new(30.5, 10.0, 0.5), false)],
            ..night(DVec3::ZERO)
        };
        for _ in 0..1200 {
            e.update(0.05, &world, &far);
        }
        assert_eq!(e.count(MobKind::Blaze), 0, "idle with nobody within 16 blocks");
        let near = Ctx { players: vec![Target::new(PlayerId::HOST, DVec3::new(10.5, 10.0, 0.5), false)], ..far };
        for _ in 0..30 {
            e.update(0.05, &world, &near);
        }
        let first = e.count(MobKind::Blaze);
        assert!((1..=4).contains(&first), "a first batch after a second: {first}");
        assert!(e.mobs.iter().all(|m| (m.pos - DVec3::new(0.5, 11.0, 0.5)).abs().max_element() <= 4.6));
        for _ in 0..20 * 300 {
            e.update(0.05, &world, &near);
        }
        assert_eq!(e.count(MobKind::Blaze), 6, "stops at six nearby");
    }

    #[test]
    fn blazes_hover_shoot_fireball_bursts_and_shrug_off_fire() {
        let mut world = Grid::flat(10);
        for x in -1..=1 {
            for z in -1..=1 {
                world.set(IVec3::new(x, 9, z), Block::LAVA);
            }
        }
        let mut e = Entities::new(18);
        e.spawn(MobKind::Blaze, DVec3::new(0.5, 14.0, 0.5));
        let c = night(DVec3::new(12.5, 10.0, 0.5));
        let events = run(&mut e, &world, &c, 0.5);
        assert!(e.mobs[0].pos.y > 12.5, "sinks slowly: {}", e.mobs[0].pos);
        let events: Vec<_> = events.into_iter().chain(run(&mut e, &world, &c, 6.0)).collect();
        let shots =
            events.iter().filter(|ev| matches!(ev, EntityEvent::Sound { sound: MobSound::Fireball, .. })).count();
        assert_eq!(shots, 3, "one burst of three after charging");
        let burnt = |ev: &&EntityEvent| matches!(ev, EntityEvent::Ignite { player: PlayerId::HOST, .. });
        assert!(events.iter().filter(burnt).count() >= 1, "{events:?}");
        assert_eq!(e.mobs[0].health, MobKind::Blaze.max_health(), "lava below never hurt it");
        assert_eq!(MobKind::Blaze.loot(), [(crate::item::Item::BLAZE_ROD, 0, 1)]);
        assert_eq!(MobKind::Blaze.xp(&mut Rng::new(1)), 10);
    }

    #[test]
    fn nether_spawns_only_nether_mobs_in_caverns() {
        // A cavern: floor at y = 40, roof of stone above y = 60.
        let mut world = Grid::flat(41);
        for x in -80..=80 {
            for z in -80..=80 {
                world.set(IVec3::new(x, 60, z), Block::STONE);
            }
        }
        let mut e = Entities::new(13);
        let c = Ctx { spawning: true, dimension: Dimension::Nether, ..night(DVec3::new(0.0, 41.0, 0.0)) };
        for _ in 0..600 {
            e.update(0.05, &world, &c);
        }
        assert!(e.count(MobKind::ZombifiedPiglin) > 0);
        assert!(e.mobs.iter().all(|m| matches!(m.kind, MobKind::ZombifiedPiglin | MobKind::Enderman)));
    }

    #[test]
    fn tnt_blows_after_its_fuse() {
        let world = Grid::flat(10);
        let mut e = Entities::new(14);
        e.prime_tnt(IVec3::new(0, 10, 0), false);
        let c = ctx(DVec3::new(30.0, 10.0, 0.0));
        let early = run(&mut e, &world, &c, 3.5);
        assert!(!early.iter().any(|ev| matches!(ev, EntityEvent::Explosion { .. })));
        assert!((e.tnt[0].pos.y - 10.0).abs() < 1e-3, "landed: {:?}", e.tnt[0].pos);
        let late = run(&mut e, &world, &c, 1.0);
        let boom = |ev: &EntityEvent| {
            matches!(ev, EntityEvent::Explosion { power: tnt::POWER, cause: "was blown up by TNT", .. })
        };
        assert!(late.iter().any(boom), "{late:?}");
        assert!(e.tnt.is_empty());
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
