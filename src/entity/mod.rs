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

pub mod dragon;
mod dragon_model;
pub mod eye;
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
/// Java's Nether fortress spawns: (mob, weight, smallest and largest group).
const FORTRESS_SPAWNS: [(MobKind, u32, u32, u32); 3] =
    [(MobKind::Blaze, 10, 2, 3), (MobKind::ZombifiedPiglin, 5, 4, 4), (MobKind::Skeleton, 2, 5, 5)];
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
    /// A thrown eye of ender dropping or shattering.
    EyeDeath,
    DragonFlap,
    DragonGrowl,
    /// The dragon spitting a fireball or breathing fire.
    DragonShoot,
    /// The dragon crashing through blocks.
    DragonSmash,
    DragonDeath,
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
        burning: bool,
    },
    /// Thorns or the environment killed a mob (loot is dropped internally).
    MobKilled {
        kind: MobKind,
        pos: DVec3,
        burning: bool,
        player_kill: bool,
        looting: u8,
    },
    /// The Ender Dragon flew through this block: remove it, without drops.
    BreakBlock {
        cell: IVec3,
    },
    /// The dragon's death is over: open the exit portal, and on the `first`
    /// kill put the egg on top.
    DragonKilled {
        first: bool,
    },
    /// Push `player` away at no less than `velocity` (the perched dragon's
    /// wings), without hurting them.
    Shove {
        player: PlayerId,
        velocity: Vec3,
    },
    /// A kill opened an End gateway here: build it.
    BuildGateway {
        pos: IVec3,
    },
    /// `owner`'s ender pearl flew into the End gateway at `cell`, from
    /// `pos` just outside it.
    PearlGateway {
        owner: PlayerId,
        cell: IVec3,
        pos: DVec3,
    },
    /// The dying dragon spills experience (turned into orbs internally).
    DragonXp {
        pos: DVec3,
        points: u32,
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
    /// Inside a Nether fortress piece, where fortress mobs spawn.
    fn in_fortress(&self, _p: IVec3) -> bool {
        false
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
    fn in_fortress(&self, p: IVec3) -> bool {
        self.generator.in_fortress(p)
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
    /// Thorns level of each worn armor piece: melee attackers get hurt.
    pub thorns: [u8; 4],
    /// Mainhand enchantments apply to kills caused by this player's Thorns.
    pub held_enchants: crate::enchant::Enchants,
}

impl Target {
    /// A living player; set [`Target::alive`] for one waiting to respawn.
    pub fn new(id: PlayerId, pos: DVec3, targetable: bool) -> Self {
        Self { id, pos, targetable, alive: true, look: DVec3::ZERO, thorns: [0; 4], held_enchants: Default::default() }
    }

    /// Thorns levels from worn armor.
    pub fn thorns_of(armor: &[Option<crate::inventory::Stack>; 4]) -> [u8; 4] {
        armor.map(|s| s.map_or(0, |s| s.enchants.level(crate::enchant::Enchantment::Thorns)))
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
    pub eyes: Vec<eye::EnderEye>,
    pub fireballs: Vec<fireball::Fireball>,
    pub puffs: Vec<Puff>,
    pub tnt: Vec<tnt::PrimedTnt>,
    /// Dropped items. They stay put (and don't age) while their chunk is
    /// unloaded, and are saved with the world.
    pub items: Vec<ItemEntity>,
    /// Experience orbs, kept and saved like dropped items.
    pub orbs: Vec<XpOrb>,
    /// The dragon fight, in the End.
    pub fight: Option<dragon::Fight>,
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
    /// Creates an empty entity simulation with deterministic randomness from `seed`.
    pub fn new(seed: u64) -> Self {
        Self {
            mobs: Vec::new(),
            arrows: Vec::new(),
            pearls: Vec::new(),
            eyes: Vec::new(),
            fireballs: Vec::new(),
            puffs: Vec::new(),
            tnt: Vec::new(),
            items: Vec::new(),
            orbs: Vec::new(),
            fight: None,
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

    /// Removes monsters which Java does not allow to exist on Peaceful.
    pub fn despawn_hostiles(&mut self) {
        self.mobs.retain(|mob| !mob.kind.is_hostile());
    }

    /// Snapshot positions and advance entity simulation on Normal difficulty.
    /// Return events for the caller to apply world edits, damage, loot and sounds.
    pub fn update<W: MobWorld + ?Sized>(&mut self, dt: f64, world: &W, ctx: &Ctx) -> Vec<EntityEvent> {
        self.update_difficulty(dt, world, ctx, crate::simulation::difficulty::Difficulty::Normal)
    }

    /// [`Entities::update`] under this world's difficulty.
    pub fn update_difficulty<W: MobWorld + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        ctx: &Ctx,
        difficulty: crate::simulation::difficulty::Difficulty,
    ) -> Vec<EntityEvent> {
        self.snapshot_positions();
        let mut events = Vec::new();
        if difficulty == crate::simulation::difficulty::Difficulty::Peaceful {
            self.despawn_hostiles();
        } else if ctx.spawning {
            self.spawn_timer -= dt as f32;
            if self.spawn_timer <= 0.0 {
                self.spawn_timer = SPAWN_INTERVAL;
                self.natural_spawn(world, ctx);
            }
        }
        if difficulty != crate::simulation::difficulty::Difficulty::Peaceful {
            self.run_spawners(dt as f32, world, ctx);
        }

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
        if let Some(fight) = &mut self.fight {
            fight.update(dt, world, ctx, &mut self.rng, &mut events);
        }

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
        let (mobs, rng, fight) = (&mut self.mobs, &mut self.rng, &mut self.fight);
        self.arrows.retain_mut(|a| a.update(dt, world, ctx, mobs, fight.as_mut(), rng, &mut events));
        self.pearls.retain_mut(|p| p.update(dt, world, mobs, rng, &mut events));
        self.update_eyes(dt, &mut events);
        for e in &events {
            match *e {
                EntityEvent::MobShot { kind, pos, killed, burning } => {
                    if killed {
                        self.drop_loot_with_fire(kind, pos, 0, burning, true);
                    }
                    if kind == MobKind::ZombifiedPiglin {
                        self.anger_piglins(pos);
                    }
                }
                EntityEvent::DragonXp { pos, points } => self.spawn_xp(pos, points),
                EntityEvent::MobKilled { kind, pos, burning, player_kill, looting } => {
                    self.drop_loot_with_fire(kind, pos, looting, burning, player_kill)
                }
                _ => {}
            }
        }
        events.retain(|e| !matches!(e, EntityEvent::DragonXp { .. } | EntityEvent::MobKilled { .. }));
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
        if let Some(fight) = &mut self.fight {
            fight.explode(center, power);
        }
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
        self.drop_loot_with(kind, pos, 0);
    }

    /// [`Entities::drop_loot`] for a kill with a looting weapon: each drop
    /// gains up to `looting` more.
    pub fn drop_loot_with(&mut self, kind: MobKind, pos: DVec3, looting: u8) {
        self.drop_loot_with_fire(kind, pos, looting, false, true);
    }

    fn drop_loot_with_fire(&mut self, kind: MobKind, pos: DVec3, looting: u8, burning: bool, player_kill: bool) {
        if player_kill {
            let xp = kind.xp(&mut self.rng);
            self.spawn_xp(pos, xp);
        }
        for (item, count) in kind.drops(&mut self.rng, looting) {
            if !player_kill && matches!(item, crate::item::Item::SPIDER_EYE | crate::item::Item::BLAZE_ROD) {
                continue;
            }
            let item = if burning {
                match item {
                    crate::item::Item::RAW_PORKCHOP => crate::item::Item::COOKED_PORKCHOP,
                    crate::item::Item::RAW_BEEF => crate::item::Item::STEAK,
                    crate::item::Item::RAW_CHICKEN => crate::item::Item::COOKED_CHICKEN,
                    i => i,
                }
            } else {
                item
            };
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
        self.drop_mined_xp(block, cell, Default::default());
    }

    /// Block experience after applying the mining tool's enchantments.
    pub fn drop_mined_xp(&mut self, block: Block, cell: IVec3, tool: crate::enchant::Enchants) {
        let xp = crate::mining::mined_xp(block, tool, &mut self.rng);
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

    /// Appends saved experience orbs, skipping empty or malformed entries.
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
            if ctx.dimension == Dimension::Nether {
                self.fortress_spawn(world, ctx, center);
            }
        }
    }

    /// Java's fortress spawn list, used for spots inside fortress pieces:
    /// blazes, zombified piglins and skeletons in groups. Light doesn't
    /// matter. (Wither skeletons and magma cubes don't exist yet.)
    fn fortress_spawn<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx, center: DVec3) {
        let total: u32 = FORTRESS_SPAWNS.iter().map(|s| s.1).sum();
        let mut r = (self.rng.next_f32() * total as f32) as u32;
        let &(kind, _, lo, hi) = FORTRESS_SPAWNS
            .iter()
            .find(|s| {
                let hit = r < s.1;
                r = r.saturating_sub(s.1);
                hit
            })
            .unwrap_or(&FORTRESS_SPAWNS[0]);
        let cap = kind.spawn_cap(Dimension::Nether);
        if self.count_near(kind, center) >= cap {
            return;
        }
        let angle = self.rng.range(0.0, TAU) as f64;
        let dist = self.rng.range(SPAWN_MIN_DIST as f32, SPAWN_MAX_DIST as f32) as f64;
        let x = (center.x + angle.cos() * dist).floor() as i32;
        let z = (center.z + angle.sin() * dist).floor() as i32;
        let top = self.rng.range(48.0, 100.0) as i32;
        let Some(pos) = cavern_spot(world, kind, x, z, top) else { return };
        if !in_spawn_ring(center, pos) || !clear_of_players(ctx, pos) || !world.in_fortress(pos.floor().as_ivec3()) {
            return;
        }
        self.spawn(kind, pos);
        let group = self.rng.range(lo as f32, hi as f32 + 1.0) as u32;
        for _ in 1..group {
            let (dx, dz) = (self.rng.range(-3.0, 3.0) as i32, self.rng.range(-3.0, 3.0) as i32);
            if self.count_near(kind, center) < cap
                && let Some(p) = cavern_spot(world, kind, x + dx, z + dz, pos.y as i32 + 1)
                && world.in_fortress(p.floor().as_ivec3())
                && clear_of_players(ctx, p)
            {
                self.spawn(kind, p);
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
        for e in &mut self.eyes {
            e.previous_pos = e.pos;
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
        model::build_eyes(&self.eyes, camera, time, alpha, &mut self.verts);
        model::build_fireballs(&self.fireballs, camera, time, alpha, &mut self.verts);
        model::build_puffs(&self.puffs, camera, alpha, &mut self.verts);
        model::build_orbs(&self.orbs, camera, max_dist, time, alpha, &mut self.verts);
        if let Some(fight) = &self.fight {
            dragon_model::build_fight(fight, camera, time, alpha, &mut self.verts);
        }
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

    /// The End crystal or dragon part a ray hits within `max_dist`, if it's
    /// nearer than any mob.
    pub fn fight_raycast(&self, origin: DVec3, dir: DVec3, max_dist: f64) -> Option<(dragon::Hit, f64)> {
        let hit = self.fight.as_ref()?.raycast(origin, dir, max_dist)?;
        let mob = self.raycast(origin, dir, max_dist).map_or(f64::INFINITY, |(_, t)| t);
        (hit.1 < mob).then_some(hit)
    }

    /// A player's melee `hit` in the fight for `damage`.
    pub fn strike(&mut self, hit: dragon::Hit, damage: f32, by: PlayerId) -> bool {
        self.fight.as_mut().is_some_and(|f| f.strike(hit, damage, Some(by), false))
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

    /// An eye of ender released at `pos` to fly toward the stronghold at
    /// `stronghold`; it survives four times in five (Java).
    pub fn release_eye(&mut self, pos: DVec3, stronghold: DVec3) {
        let survives = self.rng.next_f32() >= 0.2;
        self.eyes.push(eye::EnderEye::signalled(pos, stronghold, survives));
    }

    /// Flies thrown eyes; those done drop back as an item or shatter.
    fn update_eyes(&mut self, dt: f64, events: &mut Vec<EntityEvent>) {
        let mut i = 0;
        while i < self.eyes.len() {
            if self.eyes[i].update(dt) {
                i += 1;
                continue;
            }
            let e = self.eyes.swap_remove(i);
            events.push(EntityEvent::Sound { sound: MobSound::EyeDeath, pos: e.pos });
            if e.survives {
                let stack = crate::inventory::Stack::new(crate::item::Item::EYE_OF_ENDER, 1);
                self.items.push(ItemEntity::new(stack, e.pos, DVec3::ZERO, item::PICKUP_DELAY, &mut self.rng));
            } else {
                for _ in 0..8 {
                    let dir = DVec3::new(
                        self.rng.range(-1.0, 1.0) as f64,
                        self.rng.range(0.0, 1.0) as f64,
                        self.rng.range(-1.0, 1.0) as f64,
                    );
                    let pos = e.pos + dir * 0.1;
                    let vel = dir * 1.5;
                    self.puffs.push(Puff { pos, previous_pos: pos, vel, age: 0.0, life: 0.5, size: 0.12 });
                }
            }
        }
    }

    /// The player looses an arrow with bow `power` 0..1.
    pub fn shoot_arrow(&mut self, eye: DVec3, dir: DVec3, power: f32, pickup: bool) {
        self.shoot_enchanted(eye, dir, power, pickup, Default::default());
    }

    /// [`Entities::shoot_arrow`] from a bow with `enchants` (power, punch,
    /// flame).
    pub fn shoot_enchanted(
        &mut self,
        eye: DVec3,
        dir: DVec3,
        power: f32,
        pickup: bool,
        enchants: crate::enchant::Enchants,
    ) {
        let mut arrow = Arrow::shot(eye, dir, power, pickup);
        arrow.enchants = enchants;
        self.arrows.push(arrow);
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
        self.knock(index, dir, damage, 0)
    }

    /// Hits mob `index` for `damage`, pushing it along `dir` harder for
    /// each `extra` knockback level (Java: 0.4 + 0.5 per level).
    fn knock(&mut self, index: usize, dir: DVec3, damage: f32, extra: u8) -> Option<MobKind> {
        let flat = DVec3::new(dir.x, 0.0, dir.z).normalize_or_zero();
        let knockback = flat * 6.0 * (1.0 + 1.25 * extra as f64) + DVec3::Y * 5.0;
        let mob = self.mobs.get_mut(index)?;
        let (kind, pos) = (mob.kind, mob.pos);
        mob.player_hit();
        let killed = mob.damage(damage, Some(knockback), &mut self.rng);
        if kind == MobKind::ZombifiedPiglin {
            self.anger_piglins(pos);
        }
        killed.then_some(kind)
    }

    /// A player's melee hit on mob `index` with `held`, Java's way: the
    /// weapon's damage plus `bonus` (strength), times 1.5 if `critical`,
    /// then the enchantment bonus for that mob; knockback and fire aspect
    /// apply, and a kill drops loot with looting. When `sweep` (a sword
    /// swung on the ground, not critical), mobs right next to
    /// the target and within three blocks of the player's feet take
    /// `1 + ratio * base damage` plus their own enchantment bonus.
    /// `sweep` supplies the player's feet when sweeping is allowed.
    /// Returns the kind killed, if it was.
    pub fn melee(
        &mut self,
        index: usize,
        dir: DVec3,
        held: Option<crate::inventory::Stack>,
        bonus: f32,
        critical: bool,
        sweep: Option<DVec3>,
    ) -> Option<MobKind> {
        use crate::enchant::Enchantment;
        let target = self.mobs.get(index).filter(|m| m.alive())?;
        let (kind, pos, shape) = (target.kind, target.pos, target.shape());
        let base = (crate::mining::attack_damage(held.map(|s| s.item)) + bonus).max(0.0);
        let enchants = held.map_or(Default::default(), |s| s.active_enchants());
        let damage = base * if critical { 1.5 } else { 1.0 } + crate::enchant::damage_bonus(enchants, kind.creature());
        let (knockback, fire) = crate::mining::weapon_extras(held);
        if fire > 0.0 {
            self.mobs[index].ignite(fire);
        }
        let killed = self.knock(index, dir, damage, knockback);
        if let Some(kind) = killed {
            self.drop_loot_with_fire(
                kind,
                pos,
                enchants.level(Enchantment::Looting),
                self.mobs[index].burning || fire > 0.0,
                true,
            );
        }
        let sword = held.and_then(|s| s.item.as_tool()).is_some_and(|(k, _)| k == crate::item::ToolKind::Sword);
        if let Some(origin) = sweep.filter(|_| sword) {
            let level = enchants.level(Enchantment::SweepingEdge) as f32;
            let swept = 1.0 + level / (level + 1.0) * base;
            let (lo, hi) = (
                pos - DVec3::new(shape.half_width + 1.0, 0.25, shape.half_width + 1.0),
                pos + DVec3::new(shape.half_width + 1.0, shape.height + 0.25, shape.half_width + 1.0),
            );
            let near: Vec<usize> = (0..self.mobs.len())
                .filter(|&i| {
                    let m = &self.mobs[i];
                    let s = m.shape();
                    i != index
                        && m.alive()
                        && m.pos.distance_squared(origin) < 9.0
                        && m.pos.x + s.half_width > lo.x
                        && m.pos.x - s.half_width < hi.x
                        && m.pos.z + s.half_width > lo.z
                        && m.pos.z - s.half_width < hi.z
                        && m.pos.y + s.height > lo.y
                        && m.pos.y < hi.y
                })
                .collect();
            for i in near {
                let away = self.mobs[i].pos - pos;
                let damage = swept + crate::enchant::damage_bonus(enchants, self.mobs[i].kind.creature());
                if fire > 0.0 {
                    self.mobs[i].ignite(fire);
                }
                let killed = self.knock(i, away, damage, 0);
                if let Some(kind) = killed {
                    let at = self.mobs[i].pos;
                    self.drop_loot_with_fire(
                        kind,
                        at,
                        enchants.level(Enchantment::Looting),
                        self.mobs[i].burning || fire > 0.0,
                        true,
                    );
                }
            }
        }
        killed
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
    fn huge_looting_saturates_drop_counts() {
        // Before the clamp, 255 levels overflowed the u8 count.
        let mut rng = Rng::new(3);
        let mut expected_rng = Rng::new(3);
        for _ in 0..200 {
            let extra = (u8::MAX as f32 * expected_rng.next_f32()).round() as u32;
            let base = (expected_rng.next_f32() * 3.0) as u32;
            let drops = MobKind::Zombie.drops(&mut rng, u8::MAX);
            assert!(!drops.is_empty(), "maximum looting produces a drop for these rolls");
            assert_eq!(drops, vec![(crate::item::Item::ROTTEN_FLESH, (base + extra).min(255) as u8)]);
        }
    }

    #[test]
    fn spider_eyes_clamp_negative_base_before_looting() {
        // Java's set_count clamps to zero before enchanted_count_increase.
        // A -1 base followed by a +1 bonus therefore drops one eye.
        let seed = (0..1000)
            .find(|&seed| {
                let mut r = Rng::new(seed);
                r.next_f32(); // string looting
                r.next_f32(); // string base
                let extra = r.next_f32().round() as i32;
                let base = -1 + (r.next_f32() * 3.0) as i32;
                base == -1 && extra == 1
            })
            .unwrap();
        let drops = MobKind::Spider.drops(&mut Rng::new(seed), 1);
        assert!(drops.contains(&(crate::item::Item::SPIDER_EYE, 1)));
    }

    #[test]
    fn fire_aspect_kills_drop_cooked_loot_and_xp_once() {
        use crate::enchant::{Enchantment, Enchants};
        use crate::inventory::Stack;
        use crate::item::{Item, Tier, ToolKind};
        let world = Grid::flat(10);
        let c = ctx(DVec3::new(30.0, 10.0, 0.0));
        let sword = Stack {
            enchants: Enchants::NONE.with(Enchantment::FireAspect, 1).with(Enchantment::Looting, 3),
            ..Stack::new(Item::tool(ToolKind::Sword, Tier::Wood), 1)
        };
        // Immediate kill: ignition must happen before the loot is rolled.
        let mut e = Entities::new(5);
        e.spawn(MobKind::Pig, DVec3::new(0.5, 10.0, 0.5));
        e.mobs[0].health = 1.0;
        assert_eq!(e.melee(0, DVec3::X, Some(sword), 0.0, false, None), Some(MobKind::Pig));
        assert!(!e.items.is_empty());
        assert!(e.items.iter().all(|i| i.stack.item == Item::COOKED_PORKCHOP));

        // Survives the sword, then fire delivers the finishing damage.
        let mut e = Entities::new(5);
        e.spawn(MobKind::Pig, DVec3::new(0.5, 10.0, 0.5));
        e.mobs[0].health = crate::mining::attack_damage(Some(sword.item)) + 0.5;
        assert_eq!(e.melee(0, DVec3::X, Some(sword), 0.0, false, None), None);
        assert!(e.items.is_empty() && e.orbs.is_empty());
        run(&mut e, &world, &c, 1.1);
        let loot = e.items.iter().map(|i| i.stack.count as u32).sum::<u32>();
        let xp = e.orbs.iter().map(|o| o.value * o.count).sum::<u32>();
        assert!(loot > 0 && xp > 0, "the follow-up kill awards loot and XP");
        assert!(loot <= 3, "fire has no attacking entity, so does not add the sword's Looting bonus");
        assert!(e.items.iter().all(|i| i.stack.item == Item::COOKED_PORKCHOP));
        run(&mut e, &world, &c, 1.1);
        assert_eq!(e.items.iter().map(|i| i.stack.count as u32).sum::<u32>(), loot);
        assert_eq!(e.orbs.iter().map(|o| o.value * o.count).sum::<u32>(), xp);
    }

    #[test]
    fn fire_kills_after_player_credit_expires_still_drop_loot() {
        use crate::enchant::{Enchantment, Enchants};
        use crate::inventory::Stack;
        use crate::item::{Item, Tier, ToolKind};
        let world = Grid::flat(10);
        let c = ctx(DVec3::new(30.0, 10.0, 0.0));
        let mut e = Entities::new(5);
        e.spawn(MobKind::Cow, DVec3::new(0.5, 10.0, 0.5));
        let sword = Stack {
            enchants: Enchants::NONE.with(Enchantment::FireAspect, 2),
            ..Stack::new(Item::tool(ToolKind::Sword, Tier::Wood), 1)
        };
        e.mobs[0].health = crate::mining::attack_damage(Some(sword.item)) + 5.5;
        e.melee(0, DVec3::X, Some(sword), 0.0, false, None);
        run(&mut e, &world, &c, 6.5);
        assert!(e.items.iter().any(|i| i.stack.item == Item::STEAK), "loot does not need player credit");
        assert!(e.orbs.is_empty(), "XP requires a player hit within five seconds");
    }

    #[test]
    fn flame_arrows_credit_later_fire_kills() {
        let world = Grid::flat(10);
        let c = ctx(DVec3::new(30.0, 10.0, 0.0));
        let mut e = Entities::new(5);
        e.spawn(MobKind::Cow, DVec3::new(2.5, 10.0, 0.5));
        e.shoot_enchanted(
            DVec3::new(0.5, 11.0, 0.5),
            DVec3::X,
            0.5,
            false,
            crate::enchant::Enchants::NONE.with(crate::enchant::Enchantment::Flame, 1),
        );
        run(&mut e, &world, &c, 0.1);
        assert!(e.mobs[0].alive() && e.mobs[0].burning);
        e.mobs[0].health = 0.5;
        run(&mut e, &world, &c, 1.1);
        assert!(!e.orbs.is_empty());
        assert!(e.items.iter().any(|i| i.stack.item == crate::item::Item::STEAK));
        assert!(e.items.iter().all(|i| i.stack.item != crate::item::Item::RAW_BEEF));
    }

    #[test]
    fn looting_does_not_increase_sheep_wool() {
        for seed in 0..100 {
            assert_eq!(MobKind::Sheep.drops(&mut Rng::new(seed), 7), vec![(crate::item::Item::from(Block::WOOL), 1)]);
        }
    }

    #[test]
    fn sweep_damage_uses_each_targets_enchantments_and_player_reach() {
        use crate::enchant::{Enchantment, Enchants};
        use crate::inventory::Stack;
        use crate::item::{Item, Tier, ToolKind};
        let origin = DVec3::new(0.0, 10.0, 0.0);
        for (enchantment, level, expected) in [(Enchantment::Sharpness, 0, 4.0), (Enchantment::Smite, 3, 6.25)] {
            let mut e = Entities::new(5);
            e.spawn(MobKind::Zombie, origin + DVec3::X * 2.0);
            e.spawn(MobKind::Cow, origin + DVec3::new(2.0, 0.0, 0.8));
            e.spawn(MobKind::Zombie, origin + DVec3::new(2.0, 0.0, -0.8));
            e.spawn(MobKind::Cow, origin + DVec3::X * 3.0);
            let sword = Stack {
                enchants: Enchants::NONE
                    .with(enchantment, 5)
                    .with(Enchantment::SweepingEdge, level)
                    .with(Enchantment::FireAspect, 1),
                ..Stack::new(Item::tool(ToolKind::Sword, Tier::Diamond), 1)
            };
            e.melee(0, DVec3::X, Some(sword), 0.0, false, Some(origin));
            assert_eq!(e.mobs[1].health, MobKind::Cow.max_health() - expected);
            let undead_damage = if enchantment == Enchantment::Smite { expected + 12.5 } else { expected };
            assert_eq!(e.mobs[2].health, MobKind::Zombie.max_health() - undead_damage);
            assert!(e.mobs[1].burning && e.mobs[2].burning);
            assert_eq!(e.mobs[3].health, MobKind::Cow.max_health());
            assert!(!e.mobs[3].burning, "sweeps stop short of three blocks from the player");
        }
    }

    #[test]
    fn fire_aspect_cooks_melee_and_sweep_loot_in_water() {
        use crate::enchant::{Enchantment, Enchants};
        use crate::inventory::Stack;
        use crate::item::{Item, Tier, ToolKind};
        let mut e = Entities::new(5);
        let origin = DVec3::new(0.0, 10.0, 0.0);
        for z in [0.0, 0.8] {
            e.spawn(MobKind::Pig, origin + DVec3::new(1.0, 0.0, z));
            let m = e.mobs.last_mut().unwrap();
            m.health = 0.5;
            m.in_water = true;
        }
        let sword = Stack {
            enchants: Enchants::NONE.with(Enchantment::FireAspect, 1),
            ..Stack::new(Item::tool(ToolKind::Sword, Tier::Wood), 1)
        };
        e.melee(0, DVec3::X, Some(sword), 0.0, false, Some(origin));
        assert!(e.mobs.iter().all(|m| !m.alive() && !m.burning));
        assert!(e.items.len() >= 2);
        assert!(e.items.iter().all(|i| i.stack.item == Item::COOKED_PORKCHOP));
    }

    #[test]
    fn thorns_rolls_fractional_damage_up_to_five() {
        let world = Grid::flat(10);
        let c = Ctx {
            players: vec![Target {
                thorns: [7, 0, 0, 0],
                ..Target::new(PlayerId::HOST, DVec3::new(1.5, 10.0, 0.5), true)
            }],
            ..ctx(DVec3::ZERO)
        };
        let mut max_damage = 0.0f32;
        for seed in 0..32 {
            let mut e = Entities::new(seed);
            e.spawn(MobKind::Zombie, DVec3::new(0.5, 10.0, 0.5));
            e.update(1.0 / 60.0, &world, &c);
            let damage = MobKind::Zombie.max_health() - e.mobs[0].health;
            assert!((1.0..5.0).contains(&damage));
            assert_ne!(damage.fract(), 0.0);
            max_damage = max_damage.max(damage);
        }
        assert!(max_damage > 4.0);
    }

    #[test]
    fn thorns_kills_drop_loot_and_experience() {
        let world = Grid::flat(10);
        let mut e = Entities::new(5);
        e.spawn(MobKind::Zombie, DVec3::new(0.5, 10.0, 0.5));
        e.mobs[0].health = 0.5;
        let c = Ctx {
            // Thorns III on all four pieces: a hit back is near certain.
            players: vec![Target {
                thorns: [3; 4],
                held_enchants: crate::enchant::Enchants::NONE.with(crate::enchant::Enchantment::Looting, 7),
                ..Target::new(PlayerId::HOST, DVec3::new(1.5, 10.0, 0.5), true)
            }],
            daylight: 0.1,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
        };
        for _ in 0..90 {
            e.update(1.0 / 60.0, &world, &c);
        }
        assert!(!e.mobs.first().is_some_and(|m| m.alive()), "thorns killed the zombie");
        assert!(!e.orbs.is_empty(), "the kill drops experience like a melee kill");
        assert!(e.items.iter().map(|i| i.stack.count as u32).sum::<u32>() > 2, "Thorns uses mainhand Looting");
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

    const BOW_SPEED_FOR_TEST: f64 = projectile::BOW_SPEED;

    #[test]
    fn player_arrows_hit_whatever_they_reach_first() {
        let world = Grid::flat(10);
        let end = crate::world::end::EndGen::new(5);
        let mut fight = dragon::Fight::new(&end);
        fight.dragon = None;
        let c = fight.crystals[0].pos;
        // A cow standing just in front of the crystal: one arrow step ends
        // inside both, but it enters the cow first.
        let mut mobs = [Mob::new(MobKind::Cow, DVec3::new(c.x - 0.6, c.y, c.z), 0.0)];
        let per_step = BOW_SPEED_FOR_TEST / 60.0 / 5.0;
        let eye = DVec3::new(c.x - 0.95 - 10.0 * per_step - 0.3, c.y + 1.0, c.z);
        let mut arrow = Arrow::shot(eye, DVec3::X, 1.0, false);
        let mut events = Vec::new();
        for _ in 0..4 {
            let keep = arrow.update(
                1.0 / 60.0,
                &world,
                &ctx(DVec3::new(0.0, 10.0, 0.0)),
                &mut mobs,
                Some(&mut fight),
                &mut Rng::new(1),
                &mut events,
            );
            if !keep {
                break;
            }
        }
        assert!(events.iter().any(|e| matches!(e, EntityEvent::MobShot { kind: MobKind::Cow, .. })), "{events:?}");
        assert!(mobs[0].health < MobKind::Cow.max_health());
        let mut events = Vec::new();
        fight.update(0.05, &world, &ctx(DVec3::new(0.0, 10.0, 0.0)), &mut Rng::new(1), &mut events);
        assert_eq!(fight.crystals.len(), 10, "the crystal behind the cow survives");
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
            arrow.update(1.0 / 60.0, &world, &c, &mut [], None, &mut Rng::new(1), &mut Vec::new());
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
    fn peaceful_removes_monsters_and_suppresses_spawners() {
        let cell = IVec3::new(0, 11, 0);
        let world = Caged(Grid::flat(10), vec![(cell, MobKind::Blaze)]);
        let mut e = Entities::new(17);
        e.spawn(MobKind::Zombie, DVec3::new(2.5, 11.0, 0.5));
        e.spawn(MobKind::Cow, DVec3::new(3.5, 11.0, 0.5));
        let c = ctx(DVec3::new(10.5, 10.0, 0.5));
        for _ in 0..40 {
            e.update_difficulty(0.05, &world, &c, crate::simulation::difficulty::Difficulty::Peaceful);
        }
        assert_eq!(e.count(MobKind::Cow), 1);
        assert!(e.mobs.iter().all(|mob| !mob.kind.is_hostile()));
        assert!(e.spawner_delays.is_empty());
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

    /// A Nether cavern where everything with x > 20 is fortress.
    struct Fortressed(Grid);

    impl BlockSource for Fortressed {
        fn block(&self, p: IVec3) -> Option<Block> {
            self.0.block(p)
        }
    }

    impl MobWorld for Fortressed {
        fn loaded(&self, _: IVec3) -> bool {
            true
        }
        fn surface(&self, x: i32, z: i32) -> Option<i32> {
            self.0.surface(x, z)
        }
        fn exposed(&self, _: IVec3) -> bool {
            false
        }
        fn in_fortress(&self, p: IVec3) -> bool {
            p.x > 20
        }
    }

    #[test]
    fn fortresses_spawn_blazes_and_skeletons_only_inside() {
        let mut e = Entities::new(21);
        let world = Fortressed(Grid::flat(61));
        let c = Ctx { spawning: true, dimension: Dimension::Nether, ..night(DVec3::new(0.0, 61.0, 0.0)) };
        for _ in 0..2000 {
            e.update(0.05, &world, &c);
        }
        assert!(e.count(MobKind::Blaze) > 0, "no fortress blazes");
        for m in e.mobs.iter().filter(|m| matches!(m.kind, MobKind::Blaze | MobKind::Skeleton)) {
            assert!(m.pos.x > 20.0, "{:?} spawned outside the fortress at {:?}", m.kind, m.pos);
        }
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
        for kind in MobKind::ALL.into_iter().filter(|k| !k.loot().is_empty()) {
            let mut seen_any = false;
            for _ in 0..200 {
                for (item, n) in kind.drops(&mut rng, 0) {
                    let &(_, lo, hi) = kind.loot().iter().find(|l| l.0 == item).unwrap();
                    assert!((lo.max(1) as u8..=hi).contains(&n), "{kind:?} dropped {n} of {}", item.name());
                    seen_any = true;
                }
            }
            assert!(seen_any, "{kind:?} never dropped anything");
        }
    }
}
