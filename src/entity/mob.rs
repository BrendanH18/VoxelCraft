//! A single mob: AI, movement physics and combat state.

use std::f32::consts::{PI, TAU};

use glam::{DVec3, IVec3};

use crate::item::Item;
use crate::physics::{self, BlockSource, Shape};
use crate::world::block::Block;
use crate::world::terrain::Dimension;

use super::{Ctx, EntityEvent, MobSound, MobWorld, Rng};

const GRAVITY: f64 = 28.0;
/// Clears a 1-block ledge with a little margin (peak ~1.26 blocks).
const JUMP_VELOCITY: f64 = 8.4;
const MAX_STEP: f64 = 1.0 / 60.0;
/// Seconds of red flash (and knockback immunity) after taking damage.
pub const HURT_TIME: f32 = 0.5;
/// Length of the death animation before the mob is removed.
pub const DEATH_TIME: f32 = 0.9;
/// Hostile mobs notice players within this many blocks.
const CHASE_RANGE: f64 = 24.0;
const ATTACK_RANGE: f64 = 1.2;
const ATTACK_COOLDOWN: f32 = 1.0;
/// Zombies and skeletons burn in sunlight above this daylight level.
const BURN_DAYLIGHT: f32 = 0.45;
/// Spiders only hunt when it's darker than this (or after being hit).
const SPIDER_CALM_DAYLIGHT: f32 = 0.45;
/// Seconds a hit spider stays hostile in daylight.
const PROVOKED_TIME: f32 = 12.0;
/// Seconds zombified piglins stay angry after one of them is hit.
pub const PIGLIN_ANGER_TIME: f32 = 30.0;
/// Skeletons shoot from up to this far, and keep between these distances.
const SHOOT_RANGE: f64 = 16.0;
const SKELETON_NEAR: f64 = 5.0;
const SKELETON_FAR: f64 = 10.0;
/// Creepers light their fuse this close and keep it lit within `FUSE_KEEP`.
const FUSE_START: f64 = 3.0;
const FUSE_KEEP: f64 = 7.0;
/// Seconds from lighting the fuse to the explosion.
pub const FUSE_TIME: f32 = 1.5;
pub const CREEPER_POWER: f32 = 3.0;
/// Seconds an enderman stays angry after a stare or a hit (refreshed
/// while its target stays near).
const ENDERMAN_ANGER_TIME: f32 = 30.0;
/// Seconds of eye contact before an enderman turns on you (Java's 5 ticks).
const STARE_TIME: f32 = 0.25;
/// Endermen notice stares from this far, freeze while watched within
/// `FREEZE_DIST`, flee a stare closer than `FLEE_DIST`, and teleport after
/// targets farther than `FREEZE_DIST`.
const STARE_RANGE: f64 = 64.0;
const FREEZE_DIST: f64 = 16.0;
const FLEE_DIST: f64 = 4.0;
/// Blazes notice players this far away (Java's follow range) and melee
/// within `BLAZE_MELEE`.
const BLAZE_RANGE: f64 = 48.0;
const BLAZE_MELEE: f64 = 2.0;
/// Seconds a blaze charges (glowing) before a burst, between the burst's
/// three fireballs, and resting after it (Java's 60, 6 and 100 ticks).
const BLAZE_CHARGE: f32 = 3.0;
const BLAZE_VOLLEY: f32 = 0.3;
const BLAZE_REST: f32 = 5.0;
/// Enderman eye height (Java's 2.55).
const ENDERMAN_EYE: f64 = 2.55;
/// Falls deeper than this are avoided (blocks).
const MAX_SAFE_DROP: i32 = 3;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MobKind {
    Pig,
    Cow,
    Sheep,
    Chicken,
    Zombie,
    Skeleton,
    Creeper,
    Spider,
    /// Neutral Nether mob: leaves you alone until you hit one of them.
    ZombifiedPiglin,
    /// Neutral until looked in the eye or hit; teleports.
    Enderman,
    /// Fortress spawner mob: hovers and shoots bursts of fireballs.
    Blaze,
    /// Stronghold spawner mob: small, fast, nibbles for 1.
    Silverfish,
}

impl MobKind {
    pub const ALL: [MobKind; 12] = [
        MobKind::Pig,
        MobKind::Cow,
        MobKind::Sheep,
        MobKind::Chicken,
        MobKind::Zombie,
        MobKind::Skeleton,
        MobKind::Creeper,
        MobKind::Spider,
        MobKind::ZombifiedPiglin,
        MobKind::Enderman,
        MobKind::Blaze,
        MobKind::Silverfish,
    ];

    /// Lowercase mob name used by commands and saved spawner entries.
    pub fn name(self) -> &'static str {
        match self {
            MobKind::Pig => "pig",
            MobKind::Cow => "cow",
            MobKind::Sheep => "sheep",
            MobKind::Chicken => "chicken",
            MobKind::Zombie => "zombie",
            MobKind::Skeleton => "skeleton",
            MobKind::Creeper => "creeper",
            MobKind::Spider => "spider",
            MobKind::ZombifiedPiglin => "zombified piglin",
            MobKind::Enderman => "enderman",
            MobKind::Blaze => "blaze",
            MobKind::Silverfish => "silverfish",
        }
    }

    /// Looks a mob up by name (spaces or underscores).
    pub fn from_name(name: &str) -> Option<MobKind> {
        let name = name.replace('_', " ");
        Self::ALL.into_iter().find(|k| k.name() == name)
    }

    /// Collision box dimensions for movement and spawn-space checks.
    pub fn shape(self) -> Shape {
        match self {
            MobKind::Pig => Shape::new(0.45, 0.9),
            MobKind::Cow => Shape::new(0.45, 1.4),
            MobKind::Sheep => Shape::new(0.45, 1.3),
            MobKind::Chicken => Shape::new(0.2, 0.7),
            MobKind::Zombie | MobKind::ZombifiedPiglin => Shape::new(0.3, 1.95),
            MobKind::Skeleton => Shape::new(0.3, 1.99),
            MobKind::Creeper => Shape::new(0.3, 1.7),
            MobKind::Spider => Shape::new(0.7, 0.9),
            MobKind::Enderman => Shape::new(0.3, 2.9),
            MobKind::Blaze => Shape::new(0.3, 1.8),
            MobKind::Silverfish => Shape::new(0.2, 0.3),
        }
    }

    /// Starting health in damage points, with two points per heart.
    pub fn max_health(self) -> f32 {
        match self {
            MobKind::Pig | MobKind::Cow => 10.0,
            MobKind::Sheep | MobKind::Silverfish => 8.0,
            MobKind::Chicken => 4.0,
            MobKind::Zombie | MobKind::Skeleton | MobKind::Creeper | MobKind::ZombifiedPiglin | MobKind::Blaze => 20.0,
            MobKind::Spider => 16.0,
            MobKind::Enderman => 40.0,
        }
    }

    /// Whether this kind belongs to the hostile spawning group, including neutral monsters.
    pub fn is_hostile(self) -> bool {
        matches!(
            self,
            MobKind::Zombie
                | MobKind::Skeleton
                | MobKind::Creeper
                | MobKind::Spider
                | MobKind::ZombifiedPiglin
                | MobKind::Enderman
                | MobKind::Blaze
                | MobKind::Silverfish
        )
    }

    /// Unharmed by fire and lava.
    pub fn fire_immune(self) -> bool {
        matches!(self, MobKind::ZombifiedPiglin | MobKind::Blaze)
    }

    /// Hurt by water and rain, like Java's endermen and blazes.
    pub fn hurt_by_water(self) -> bool {
        matches!(self, MobKind::Enderman | MobKind::Blaze)
    }

    /// Whether this kind spawns naturally in `dimension`.
    pub fn spawns_in(self, dimension: Dimension) -> bool {
        match self {
            MobKind::Enderman => true,
            // Only from spawners and inside fortresses (`fortress_spawn`).
            MobKind::Blaze => false,
            // Only from stronghold spawners (and infested blocks, later).
            MobKind::Silverfish => false,
            MobKind::ZombifiedPiglin => dimension == Dimension::Nether,
            _ => dimension == Dimension::Overworld,
        }
    }

    /// Chance that a spawn attempt for this kind goes ahead in `dimension`:
    /// Java's spawn weights make endermen a rare sight except in the End.
    pub fn spawn_chance(self, dimension: Dimension) -> f32 {
        match (self, dimension) {
            (MobKind::Enderman, Dimension::Overworld) => 0.1,
            (MobKind::Enderman, Dimension::Nether) => 0.02,
            _ => 1.0,
        }
    }

    /// Most mobs of this kind that spawn naturally around the player.
    pub fn spawn_cap(self, dimension: Dimension) -> usize {
        match self {
            MobKind::Zombie => 4,
            MobKind::ZombifiedPiglin => 8,
            MobKind::Enderman if dimension == Dimension::End => 12,
            MobKind::Enderman => 1,
            k if k.is_hostile() => 3,
            _ => 4,
        }
    }

    fn burns_in_sun(self) -> bool {
        matches!(self, MobKind::Zombie | MobKind::Skeleton)
    }

    fn wander_speed(self) -> f64 {
        match self {
            MobKind::Pig => 1.3,
            MobKind::Cow | MobKind::Zombie | MobKind::Creeper | MobKind::ZombifiedPiglin => 1.1,
            MobKind::Sheep | MobKind::Skeleton | MobKind::Blaze => 1.2,
            MobKind::Chicken => 1.0,
            MobKind::Spider | MobKind::Enderman | MobKind::Silverfish => 1.4,
        }
    }

    fn chase_speed(self) -> f64 {
        match self {
            MobKind::Enderman => 4.5,
            MobKind::Spider => 3.0,
            MobKind::ZombifiedPiglin | MobKind::Silverfish => 2.8,
            MobKind::Skeleton => 2.2,
            MobKind::Creeper => 2.0,
            _ => 2.4,
        }
    }

    /// Melee damage and the death message it gives.
    fn melee(self) -> (f32, &'static str) {
        match self {
            MobKind::Spider => (2.0, "was slain by a spider"),
            MobKind::ZombifiedPiglin => (5.0, "was slain by a zombified piglin"),
            MobKind::Enderman => (7.0, "was slain by an enderman"),
            MobKind::Blaze => (6.0, "was slain by a blaze"),
            MobKind::Silverfish => (1.0, "was slain by a silverfish"),
            _ => (3.0, "was slain by a zombie"),
        }
    }

    /// Loot for a player kill: (item, min, max) rolls. A negative minimum
    /// makes the drop a chance (Java's spider eye: -1..1 is one in three).
    pub(super) fn loot(self) -> &'static [(Item, i8, u8)] {
        const WOOL: Item = Item::from_block(Block::WOOL);
        match self {
            MobKind::Pig => &[(Item::RAW_PORKCHOP, 1, 3)],
            MobKind::Cow => &[(Item::RAW_BEEF, 1, 3), (Item::LEATHER, 0, 2)],
            MobKind::Sheep => &[(WOOL, 1, 1)],
            MobKind::Chicken => &[(Item::RAW_CHICKEN, 1, 1), (Item::FEATHER, 0, 2)],
            MobKind::Zombie => &[(Item::ROTTEN_FLESH, 0, 2)],
            MobKind::Skeleton => &[(Item::BONE, 0, 2), (Item::ARROW, 0, 2)],
            MobKind::Creeper => &[(Item::GUNPOWDER, 0, 2)],
            MobKind::Spider => &[(Item::STRING, 0, 2), (Item::SPIDER_EYE, -1, 1)],
            MobKind::ZombifiedPiglin => &[(Item::ROTTEN_FLESH, 0, 1), (Item::GOLD_NUGGET, 0, 1)],
            MobKind::Enderman => &[(Item::ENDER_PEARL, 0, 1)],
            MobKind::Blaze => &[(Item::BLAZE_ROD, 0, 1)],
            MobKind::Silverfish => &[],
        }
    }

    /// Rolls the drops for killing one of these.
    pub fn drops(self, rng: &mut Rng) -> Vec<(Item, u8)> {
        self.loot()
            .iter()
            .map(|&(item, lo, hi)| {
                let span = hi as i32 - lo as i32 + 1;
                (item, (lo as i32 + (rng.next_f32() * span as f32) as i32).max(0) as u8)
            })
            .filter(|&(_, n)| n > 0)
            .collect()
    }

    /// Experience for killing one (Java's: 5 for monsters, 1-3 for animals).
    pub fn xp(self, rng: &mut Rng) -> u32 {
        match self {
            MobKind::Blaze => 10,
            k if k.is_hostile() => 5,
            _ => 1 + (rng.next_f32() * 3.0) as u32,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Ai {
    /// Standing still, glancing around.
    Idle,
    /// Walking in `move_yaw`.
    Wander,
    /// Running from an attacker (passive mobs), changing direction every so often.
    Panic,
    /// Going after the player (hostile mobs), each in its own way.
    Chase,
}

pub struct Mob {
    pub kind: MobKind,
    /// Feet position (bottom centre of the box).
    pub pos: DVec3,
    /// Position at the start of the last simulation step, for rendering.
    pub previous_pos: DVec3,
    pub vel: DVec3,
    /// Body facing, same convention as the player: forward = (cos, 0, sin).
    pub yaw: f32,
    /// Head yaw relative to the body, and pitch (positive looks up).
    pub head_yaw: f32,
    pub head_pitch: f32,
    pub health: f32,
    /// Seconds of hurt flash left.
    pub hurt: f32,
    /// Seconds since death, while the death animation plays.
    pub dying: Option<f32>,
    pub on_ground: bool,
    pub in_water: bool,
    pub burning: bool,
    /// Walk cycle phase (radians) and amplitude (0..1).
    pub limb_phase: f32,
    pub limb_amp: f32,
    /// Seconds left of the arm swing after an attack.
    pub attack_anim: f32,
    /// Sky light estimate at the mob, 0..1 (refreshed a few times a second).
    pub sky_light: f32,
    /// Torch light where it stands, 0..1.
    pub block_light: f32,
    /// Creeper fuse: seconds lit, 0 when not fusing.
    pub fuse: f32,
    /// Seconds a hit spider stays angry in daylight.
    provoked: f32,
    pub(super) ai: Ai,
    pub(super) ai_timer: f32,
    pub(super) move_yaw: f32,
    head_target: (f32, f32),
    head_timer: f32,
    attack_cooldown: f32,
    burn_timer: f32,
    fire_left: f32,
    light_timer: f32,
    /// Blocked horizontally on the last physics step.
    blocked: bool,
    /// Seconds left sidestepping around an obstacle while chasing, and
    /// which side (+1 / -1).
    detour: f32,
    detour_side: f64,
    /// Seconds until the next idle call.
    ambient_timer: f32,
    /// A hurt or death cry waiting to be reported by the next update.
    cry: Option<MobSound>,
    /// Endermen: teleport away at the next update (an arrow, water, fire).
    pub teleport_pending: bool,
    /// Endermen: seconds a player has been looking it in the eye.
    stare: f32,
    /// Endermen: seconds spent far from the target, and since the last
    /// time it turned on someone.
    teleport_timer: f32,
    since_anger: f32,
    /// Blazes: glowing while charging a burst; which shot of the burst is
    /// next; rising to stay `height_offset` above the target's eyes.
    pub charged: bool,
    attack_step: u8,
    lift: bool,
    height_offset: f32,
    offset_timer: f32,
}

impl Mob {
    /// Create a healthy, idle mob at `pos` with `yaw` in radians; snap its initial render position.
    pub fn new(kind: MobKind, pos: DVec3, yaw: f32) -> Self {
        Self {
            kind,
            pos,
            previous_pos: pos,
            vel: DVec3::ZERO,
            yaw,
            head_yaw: 0.0,
            head_pitch: 0.0,
            health: kind.max_health(),
            hurt: 0.0,
            dying: None,
            on_ground: false,
            in_water: false,
            burning: false,
            limb_phase: 0.0,
            limb_amp: 0.0,
            attack_anim: 0.0,
            sky_light: 1.0,
            block_light: 0.0,
            ai: Ai::Idle,
            ai_timer: 1.0,
            move_yaw: yaw,
            head_target: (0.0, 0.0),
            head_timer: 0.0,
            attack_cooldown: 0.0,
            burn_timer: 0.0,
            fire_left: 0.0,
            light_timer: 0.0,
            fuse: 0.0,
            provoked: 0.0,
            blocked: false,
            detour: 0.0,
            detour_side: 1.0,
            ambient_timer: 2.0 + (yaw * 1000.0).rem_euclid(10.0),
            cry: None,
            teleport_pending: false,
            stare: 0.0,
            teleport_timer: 0.0,
            since_anger: 1e3,
            charged: false,
            attack_step: 0,
            lift: false,
            height_offset: 0.5,
            offset_timer: 0.0,
        }
    }

    pub fn shape(&self) -> Shape {
        self.kind.shape()
    }

    pub fn aabb(&self) -> (DVec3, DVec3) {
        self.shape().aabb(self.pos)
    }

    pub fn alive(&self) -> bool {
        self.dying.is_none()
    }

    /// Turns a neutral mob hostile for `secs`.
    pub fn anger(&mut self, secs: f32) {
        self.provoked = self.provoked.max(secs);
    }

    /// Takes a hit. `knockback` replaces the horizontal velocity (its y is
    /// the upward pop). Returns `true` if this killed the mob.
    pub fn damage(&mut self, amount: f32, knockback: Option<DVec3>, rng: &mut Rng) -> bool {
        if !self.alive() {
            return false;
        }
        self.health -= amount;
        self.hurt = HURT_TIME;
        if let Some(kb) = knockback {
            self.vel.x = kb.x;
            self.vel.z = kb.z;
            self.vel.y = self.vel.y.max(kb.y);
        }
        if !self.kind.is_hostile() {
            self.ai = Ai::Panic;
            self.ai_timer = rng.range(3.0, 5.0);
            self.move_yaw = rng.range(0.0, TAU);
        }
        self.provoked =
            self.provoked.max(if self.kind == MobKind::Enderman { ENDERMAN_ANGER_TIME } else { PROVOKED_TIME });
        if self.health <= 0.0 {
            self.dying = Some(0.0);
            self.cry = Some(MobSound::Death(self.kind));
            return true;
        }
        self.cry = Some(MobSound::Hurt(self.kind));
        false
    }

    /// Advances the mob by `dt` seconds.
    pub fn update<W: MobWorld + ?Sized>(
        &mut self,
        dt: f64,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) {
        let dtf = dt as f32;
        if let Some(sound) = self.cry.take() {
            events.push(EntityEvent::Sound { sound, pos: self.pos + DVec3::Y * (self.shape().height * 0.8) });
        }
        self.ambient_timer -= dtf;
        if self.ambient_timer <= 0.0 {
            self.ambient_timer = rng.range(7.0, 18.0);
            if self.alive() && self.kind != MobKind::Creeper {
                let sound = MobSound::Ambient(self.kind);
                events.push(EntityEvent::Sound { sound, pos: self.pos + DVec3::Y * (self.shape().height * 0.8) });
            }
        }
        self.hurt = (self.hurt - dtf).max(0.0);
        self.provoked = (self.provoked - dtf).max(0.0);
        self.attack_cooldown -= dtf;
        self.attack_anim = (self.attack_anim - dtf).max(0.0);

        self.light_timer -= dtf;
        if self.light_timer <= 0.0 {
            self.light_timer = 0.25;
            let at = self.pos + DVec3::new(0.0, self.shape().height * 0.6, 0.0);
            self.sky_light = sky_light(world, at);
            self.block_light = world.block_light(at.floor().as_ivec3()) as f32 / 15.0;
        }

        if self.teleport_pending && self.alive() {
            self.teleport_pending = false;
            self.teleport(world, rng, None, events);
        }

        let (wish, speed) = if let Some(t) = &mut self.dying {
            *t += dtf;
            (None, 0.0)
        } else {
            self.think(dtf, world, ctx, rng, events)
        };

        // Don't walk off tall drops; wanderers pick another direction.
        let wish = wish.filter(|&dir| {
            let safe = !self.on_ground || self.in_water || !is_cliff(world, self.pos, dir, self.shape());
            if !safe && matches!(self.ai, Ai::Wander | Ai::Panic) {
                self.ai_timer = self.ai_timer.min(0.3);
                self.move_yaw = rng.range(0.0, TAU);
            }
            safe
        });

        // Turn the body toward the direction of travel.
        if let Some(dir) = wish {
            let target = (dir.z as f32).atan2(dir.x as f32);
            self.yaw = turn_toward(self.yaw, target, 8.0 * dtf);
        }

        let steps = (dt / MAX_STEP).ceil().max(1.0) as u32;
        let h = dt / steps as f64;
        for _ in 0..steps {
            self.physics_step(h, world, wish, speed);
        }

        // Walk cycle follows ground speed.
        let hspeed = self.vel.x.hypot(self.vel.z) as f32;
        self.limb_phase = (self.limb_phase + hspeed * dtf * 3.2) % (TAU * 64.0);
        let amp = (hspeed / 2.0).min(1.0);
        self.limb_amp += (amp - self.limb_amp) * (dtf * 10.0).min(1.0);

        // Head eases toward its target.
        let k = (dtf * 6.0).min(1.0);
        self.head_yaw += (self.head_target.0 - self.head_yaw) * k;
        self.head_pitch += (self.head_target.1 - self.head_pitch) * k;

        let (health, provoked) = (self.health, self.provoked);
        self.burn(dtf, world, ctx, rng);
        // Endermen hurt by anything but a mob or player usually teleport,
        // and only get angry at attackers.
        if self.kind == MobKind::Enderman {
            self.provoked = provoked;
            if self.health < health && rng.chance(0.9) {
                self.teleport_pending = true;
            }
        }
    }

    /// Picks a movement direction and speed for this tick.
    fn think<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> (Option<DVec3>, f64) {
        self.ai_timer -= dt;
        let target = ctx.nearest_target(self.pos);
        let to_player = target.map_or(DVec3::ZERO, |t| t.pos - self.pos);
        let flat = DVec3::new(to_player.x, 0.0, to_player.z);
        let hdist = flat.length();

        if self.kind.is_hostile() {
            let aggressive = match self.kind {
                MobKind::Spider => ctx.daylight < SPIDER_CALM_DAYLIGHT || self.provoked > 0.0,
                MobKind::ZombifiedPiglin => self.provoked > 0.0,
                MobKind::Enderman => self.enderman_anger(dt, world, ctx, rng, events),
                _ => true,
            };
            let (range, height) = if self.kind == MobKind::Blaze { (BLAZE_RANGE, 24.0) } else { (CHASE_RANGE, 12.0) };
            let chasing = aggressive && target.is_some() && hdist < range && to_player.y.abs() < height;
            if chasing {
                self.ai = Ai::Chase;
            } else if self.ai == Ai::Chase {
                self.ai = Ai::Idle;
                self.ai_timer = 1.0;
            }
        }
        if self.ai != Ai::Chase {
            self.fuse = (self.fuse - dt).max(0.0);
            self.lift = false;
            self.charged = false;
            self.attack_step = 0;
        }

        match self.ai {
            Ai::Chase => {
                let Some(&target) = target else { return (None, 0.0) };
                // Look at the player's face.
                let eye = to_player.y + 1.62 - self.shape().height * 0.9;
                let face_yaw = (to_player.z as f32).atan2(to_player.x as f32);
                self.head_target = (wrap(face_yaw - self.yaw).clamp(-1.2, 1.2), (eye as f32).atan2(hdist as f32));
                let dir = if hdist > 1e-6 { flat / hdist } else { DVec3::X };
                if self.kind == MobKind::Enderman && self.enderman_tactics(dt, world, &target, rng, events) {
                    return (None, 0.0);
                }
                match self.kind {
                    MobKind::Skeleton => {
                        return self.skeleton_tactics(dt, world, target.pos, dir, hdist, rng, events);
                    }
                    MobKind::Blaze => {
                        if let Some(stop) = self.blaze_tactics(dt, world, &target, rng, events) {
                            return stop;
                        }
                    }
                    MobKind::Creeper => {
                        if let Some(stop) = self.creeper_fuse(dt, hdist, events) {
                            return stop;
                        }
                    }
                    _ => {
                        if hdist <= ATTACK_RANGE && to_player.y.abs() < 1.6 && self.attack_cooldown <= 0.0 {
                            self.attack_cooldown = ATTACK_COOLDOWN;
                            self.attack_anim = 0.35;
                            let knockback = dir * 6.0 + DVec3::Y * 5.0;
                            let (damage, cause) = self.kind.melee();
                            events.push(EntityEvent::PlayerHit {
                                player: target.id,
                                damage,
                                knockback: knockback.as_vec3(),
                                cause,
                            });
                        }
                    }
                }
                // Stop just short so we don't stand inside the player.
                if hdist <= 0.8 {
                    return (None, 0.0);
                }
                // No pathfinding: head straight for the player, and if a
                // wall we can't jump is in the way, sidestep for a moment.
                self.detour -= dt;
                if self.detour <= 0.0 && self.blocked && self.on_ground && !self.can_step_up(world, dir) {
                    self.detour = rng.range(0.6, 1.2);
                    self.detour_side = if rng.chance(0.5) { 1.0 } else { -1.0 };
                }
                let dir = if self.detour > 0.0 {
                    let side = DVec3::new(-dir.z, 0.0, dir.x) * self.detour_side;
                    (side + dir * 0.35).normalize()
                } else {
                    dir
                };
                (Some(dir), self.kind.chase_speed())
            }
            Ai::Panic => {
                if self.ai_timer <= 0.0 {
                    self.ai = Ai::Idle;
                    self.ai_timer = rng.range(1.0, 3.0);
                    return (None, 0.0);
                }
                if rng.chance(dt * 1.2) || self.blocked && rng.chance(dt * 4.0) {
                    self.move_yaw = rng.range(0.0, TAU);
                }
                self.head_target = (0.0, 0.0);
                (Some(yaw_dir(self.move_yaw)), 3.4)
            }
            Ai::Wander => {
                // Turn away from walls we can't hop over.
                if self.blocked && self.on_ground && !self.in_water && !self.can_step_up(world, yaw_dir(self.move_yaw))
                {
                    self.move_yaw = rng.range(0.0, TAU);
                }
                if self.ai_timer <= 0.0 {
                    self.ai = Ai::Idle;
                    self.ai_timer = rng.range(2.0, 6.0);
                }
                self.head_target = (0.0, 0.0);
                (Some(yaw_dir(self.move_yaw)), self.kind.wander_speed())
            }
            Ai::Idle => {
                // Occasionally look around.
                self.head_timer -= dt;
                if self.head_timer <= 0.0 {
                    self.head_timer = rng.range(1.0, 3.5);
                    self.head_target =
                        if rng.chance(0.6) { (rng.range(-1.1, 1.1), rng.range(-0.35, 0.3)) } else { (0.0, 0.0) };
                }
                if self.ai_timer <= 0.0 {
                    if rng.chance(0.7) {
                        self.ai = Ai::Wander;
                        self.ai_timer = rng.range(2.0, 5.0);
                        self.move_yaw = rng.range(0.0, TAU);
                    } else {
                        self.ai_timer = rng.range(2.0, 5.0);
                    }
                }
                (None, 0.0)
            }
        }
    }

    /// Java's blaze attack: rise to hover a little above the target, melee
    /// when within 2 blocks, otherwise (when it can see the target) charge
    /// for 3 s and fire three fireballs 0.3 s apart, then rest 5 s. Returns
    /// the movement to use instead of closing in, if any.
    fn blaze_tactics<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        world: &W,
        target: &super::Target,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> Option<(Option<DVec3>, f64)> {
        self.offset_timer -= dt;
        if self.offset_timer <= 0.0 {
            self.offset_timer = 5.0;
            self.height_offset = 0.5 + rng.range(-1.0, 1.0) * 3.0;
        }
        let eye = self.pos + DVec3::Y * (self.shape().height * 0.85);
        let target_eye = target.pos + DVec3::Y * crate::player::EYE_HEIGHT;
        self.lift = target_eye.y > eye.y + self.height_offset as f64;
        let to = target_eye - eye;
        let dist = to.length();
        if dist < BLAZE_MELEE {
            // Close enough to hit: the usual melee code handles it.
            return None;
        }
        if !line_of_sight(world, eye, target_eye) {
            return None;
        }
        if self.attack_cooldown <= 0.0 {
            self.attack_step += 1;
            match self.attack_step {
                1 => {
                    self.attack_cooldown = BLAZE_CHARGE;
                    self.charged = true;
                }
                2..=4 => {
                    self.attack_cooldown = BLAZE_VOLLEY;
                    // Java's spread grows with the square root of distance.
                    let spread = dist.sqrt() * 0.5 * 0.1;
                    let mut r = || rng.range(-1.0, 1.0) as f64 * spread;
                    let dir = (to / dist + DVec3::new(r(), 0.0, r())).normalize();
                    events.push(EntityEvent::Fireball { from: eye + dir * 0.5, dir });
                    events.push(EntityEvent::Sound { sound: MobSound::Fireball, pos: eye });
                }
                _ => {
                    self.attack_cooldown = BLAZE_REST;
                    self.attack_step = 0;
                    self.charged = false;
                }
            }
        }
        Some((None, 0.0))
    }

    /// Where an enderman's eyes are.
    fn eye(&self) -> DVec3 {
        self.pos + DVec3::Y * ENDERMAN_EYE
    }

    /// Java's `isLookingAtMe`: `target` looks within a narrowing cone of
    /// this enderman's eyes, with nothing in between.
    fn watched_by<W: MobWorld + ?Sized>(&self, world: &W, target: &super::Target) -> bool {
        let from = target.pos + DVec3::Y * crate::player::EYE_HEIGHT;
        let to = self.eye() - from;
        let d = to.length();
        target.look != DVec3::ZERO
            && d > 1e-3
            && d < STARE_RANGE
            && target.look.dot(to / d) > 1.0 - 0.025 / d
            && line_of_sight(world, from, self.eye())
    }

    /// Updates an enderman's anger: a long enough stare from a targetable
    /// player sets it off with a scream; a near target keeps it going; and
    /// in daylight under the open sky a calm one wanders off by teleporting.
    /// Returns whether it is angry.
    fn enderman_anger<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> bool {
        self.since_anger += dt;
        let staring = ctx.players.iter().any(|t| t.targetable && self.watched_by(world, t));
        self.stare = if staring { self.stare + dt } else { 0.0 };
        if self.stare >= STARE_TIME && self.provoked <= 0.0 {
            self.provoked = ENDERMAN_ANGER_TIME;
            self.since_anger = 0.0;
            events.push(EntityEvent::Sound { sound: MobSound::Scream, pos: self.eye() });
        }
        if self.provoked > 0.0 && ctx.nearest_target(self.pos).is_some_and(|t| t.pos.distance(self.pos) < 32.0) {
            self.provoked = self.provoked.max(5.0);
        }
        let head = self.eye().floor().as_ivec3();
        if ctx.daylight > 0.5 && self.since_anger > 30.0 && world.exposed(head) {
            // Java: random * 30 < (brightness - 0.4) * 2, each tick.
            if rng.chance(dt * 20.0 * (ctx.daylight - 0.4) * 2.0 / 30.0) {
                self.provoked = 0.0;
                self.teleport_pending = true;
            }
        }
        self.provoked > 0.0
    }

    /// An angry enderman freezes while its target watches it (and flees a
    /// stare up close), and teleports toward a target that keeps its
    /// distance. Returns whether it should stand still this tick.
    fn enderman_tactics<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        world: &W,
        target: &super::Target,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> bool {
        let dist = target.pos.distance(self.pos);
        if self.watched_by(world, target) {
            self.teleport_timer = 0.0;
            if dist < FLEE_DIST {
                self.teleport(world, rng, None, events);
            }
            return dist < FREEZE_DIST;
        }
        if dist > FREEZE_DIST {
            self.teleport_timer += dt;
            if self.teleport_timer >= 1.5 {
                self.teleport_timer = 0.0;
                self.teleport(world, rng, Some(target.pos), events);
            }
        }
        false
    }

    /// Java's enderman teleport: a random spot up to 32 blocks away (or,
    /// `toward` a target, near the side of it facing this enderman), dropped
    /// onto the ground below, dry and with room to stand. Tries a few spots.
    pub fn teleport<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        rng: &mut Rng,
        toward: Option<DVec3>,
        events: &mut Vec<EntityEvent>,
    ) -> bool {
        let shape = self.shape();
        for _ in 0..16 {
            let mut r = || rng.range(-0.5, 0.5) as f64;
            let goal = match toward {
                Some(t) => {
                    let back = (self.pos - t).normalize_or(DVec3::X);
                    self.pos + DVec3::new(r() * 8.0, r() * 16.0, r() * 8.0) - back * 16.0
                }
                None => self.pos + DVec3::new(r() * 64.0, r() * 64.0, r() * 64.0),
            };
            let (x, z) = (goal.x.floor() as i32, goal.z.floor() as i32);
            let Some(ground) = (goal.y.floor() as i32 - 32..=goal.y.floor() as i32).rev().find(|&y| {
                world.block(IVec3::new(x, y, z)).is_some_and(|b| b.is_solid())
                    && !world.block(IVec3::new(x, y + 1, z)).is_some_and(|b| b.is_solid())
            }) else {
                continue;
            };
            let to = DVec3::new(x as f64 + 0.5, ground as f64 + 1.0, z as f64 + 0.5);
            let loaded = world.loaded(to.floor().as_ivec3());
            let dry = !physics::touches_block(world, to, shape, |b| b.is_water() || b.is_lava());
            if loaded && dry && !physics::overlaps_solid(world, to, shape) {
                events.push(EntityEvent::Sound { sound: MobSound::Teleport, pos: self.pos + DVec3::Y });
                events.push(EntityEvent::Sound { sound: MobSound::Teleport, pos: to + DVec3::Y });
                self.pos = to;
                self.previous_pos = to;
                self.vel = DVec3::ZERO;
                return true;
            }
        }
        false
    }

    /// Skeletons keep their distance and shoot when they can see the
    /// player whose feet are at `player`.
    #[allow(clippy::too_many_arguments)]
    fn skeleton_tactics<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        world: &W,
        player: DVec3,
        dir: DVec3,
        hdist: f64,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> (Option<DVec3>, f64) {
        let eye = self.pos + DVec3::Y * (self.shape().height * 0.9);
        let target = player + DVec3::Y * 1.2;
        if self.attack_cooldown <= 0.0 && hdist < SHOOT_RANGE && line_of_sight(world, eye, target) {
            self.attack_cooldown = rng.range(1.6, 2.4);
            self.attack_anim = 0.35;
            events.push(EntityEvent::Shoot { from: eye + dir * 0.5, target });
            events.push(EntityEvent::Sound { sound: MobSound::Bow, pos: eye });
        }
        if rng.chance(dt * 0.4) {
            self.detour_side = -self.detour_side;
        }
        let side = DVec3::new(-dir.z, 0.0, dir.x) * self.detour_side;
        if hdist > SKELETON_FAR {
            (Some(dir), self.kind.chase_speed())
        } else if hdist < SKELETON_NEAR {
            (Some((-dir + side * 0.3).normalize()), 2.0)
        } else {
            (Some(side), 1.0)
        }
    }

    /// Creepers stop and hiss when close; returns the movement to use
    /// while fusing (or `None` to keep approaching).
    fn creeper_fuse(&mut self, dt: f32, hdist: f64, events: &mut Vec<EntityEvent>) -> Option<(Option<DVec3>, f64)> {
        let fusing = if self.fuse > 0.0 { hdist < FUSE_KEEP } else { hdist < FUSE_START };
        if !fusing {
            self.fuse = (self.fuse - dt).max(0.0);
            return None;
        }
        if self.fuse == 0.0 {
            events.push(EntityEvent::Sound { sound: MobSound::Fuse, pos: self.pos });
        }
        self.fuse += dt;
        if self.fuse >= FUSE_TIME {
            let center = self.pos + DVec3::Y * (self.shape().height * 0.5);
            events.push(EntityEvent::Explosion { center, power: CREEPER_POWER, cause: "was blown up by a creeper" });
            // Gone in the blast: no death animation, no loot.
            self.health = 0.0;
            self.dying = Some(DEATH_TIME);
        }
        Some((None, 0.0))
    }

    fn physics_step<W: BlockSource + ?Sized>(&mut self, dt: f64, world: &W, wish: Option<DVec3>, speed: f64) {
        let shape = self.shape();
        self.in_water = physics::is_fluid_at(world, self.pos + DVec3::new(0.0, 0.3, 0.0));
        let target = wish.map_or(DVec3::ZERO, |d| d * speed);

        if self.in_water {
            let k = (dt * 4.0).min(1.0);
            self.vel.x += (target.x * 0.6 - self.vel.x) * k;
            self.vel.z += (target.z * 0.6 - self.vel.z) * k;
            // Buoyant below ~0.6 blocks of depth, so mobs bob at the surface.
            if physics::is_fluid_at(world, self.pos + DVec3::new(0.0, 0.6, 0.0)) {
                self.vel.y = (self.vel.y + 22.0 * dt).min(2.0);
            } else {
                self.vel.y -= GRAVITY * 0.25 * dt;
            }
            self.vel.y = self.vel.y.max(-4.0);
            // Climb out onto the shore.
            if self.blocked && wish.is_some() {
                self.vel.y = self.vel.y.max(4.5);
            }
        } else {
            // Knockback isn't cancelled instantly: little control in the air.
            let k = (dt * if self.on_ground { 10.0 } else { 1.5 }).min(1.0);
            if self.on_ground || wish.is_some() {
                self.vel.x += (target.x - self.vel.x) * k;
                self.vel.z += (target.z - self.vel.z) * k;
            }
            self.vel.y = (self.vel.y - GRAVITY * dt).max(-78.0);
            if self.kind == MobKind::Chicken {
                self.vel.y = self.vel.y.max(-2.5); // flaps its way down
            }
            if self.kind == MobKind::Blaze && self.alive() {
                // Java blazes sink slowly and rise toward a target above.
                self.vel.y = self.vel.y.max(-2.3);
                if self.lift {
                    self.vel.y += (6.0 - self.vel.y) * (1.0 - 0.7f64.powf(dt * 20.0));
                }
            }
            if self.kind == MobKind::Spider && self.blocked && wish.is_some() && self.alive() {
                self.vel.y = self.vel.y.max(3.0); // climbs walls
            } else if let Some(dir) = wish
                && self.on_ground
                && self.blocked
                && self.can_step_up(world, dir)
            {
                self.vel.y = JUMP_VELOCITY;
            }
        }
        if self.dying.is_some() && self.on_ground {
            // Corpses slide to a stop.
            self.vel.x *= 1.0 - (dt * 8.0).min(1.0);
            self.vel.z *= 1.0 - (dt * 8.0).min(1.0);
        }

        let delta = self.vel * dt;
        let hit = if self.on_ground {
            physics::move_box_stepping(world, &mut self.pos, &mut self.vel, delta, shape, crate::player::STEP_HEIGHT)
        } else {
            physics::move_box(world, &mut self.pos, &mut self.vel, delta, shape)
        };
        self.on_ground = hit.on_ground;
        self.blocked = hit.horizontal;
    }

    /// Whether the obstacle ahead is a single-block ledge we can jump onto.
    fn can_step_up<W: BlockSource + ?Sized>(&self, world: &W, dir: DVec3) -> bool {
        let raised = self.pos + DVec3::new(0.0, 1.02, 0.0);
        !physics::overlaps_solid(world, raised, self.shape())
            && !physics::overlaps_solid(world, raised + dir * 0.4, self.shape())
    }

    /// Fire keeps burning after contact; water/rain extinguishes it. Nether
    /// piglins resist fire and lava. Undead also ignite in direct sunlight.
    fn burn<W: MobWorld + ?Sized>(&mut self, dt: f32, world: &W, ctx: &Ctx, rng: &mut Rng) {
        let head = (self.pos + DVec3::new(0.0, self.shape().height - 0.1, 0.0)).floor().as_ivec3();
        let in_lava = physics::touches_block(world, self.pos, self.shape(), Block::is_lava);
        let in_fire = physics::touches_block(world, self.pos, self.shape(), Block::is_fire);
        let sunburn = self.kind.burns_in_sun()
            && ctx.daylight > BURN_DAYLIGHT
            && !ctx.raining
            && !self.in_water
            && world.exposed(head);
        if self.kind.fire_immune() {
            self.fire_left = 0.0;
        } else if in_lava {
            self.fire_left = 15.0;
        } else if self.in_water || world.rains_on(head) {
            self.fire_left = 0.0;
        } else if sunburn || in_fire {
            self.fire_left = 8.0;
            if in_fire && !self.burning {
                self.damage(1.0, None, rng);
            }
        } else {
            self.fire_left = (self.fire_left - dt).max(0.0);
        }
        let wet = physics::touches_block(world, self.pos, self.shape(), Block::is_water) || world.rains_on(head);
        if self.kind.hurt_by_water() && wet && self.hurt <= 0.0 {
            // Water hurts endermen and blazes (Java's 1 damage, spaced by hurt time).
            self.damage(1.0, None, rng);
        }
        self.burning = self.alive() && self.fire_left > 0.0;
        if self.kind == MobKind::Blaze {
            // A charging blaze is wreathed in flames.
            self.burning = self.alive() && self.charged;
            return;
        }
        if !self.burning {
            self.burn_timer = 0.0;
            return;
        }
        let (interval, amount) = if in_lava {
            (0.5, 4.0)
        } else if in_fire {
            (0.5, 1.0)
        } else {
            (1.0, if sunburn { 2.0 } else { 1.0 })
        };
        self.burn_timer += dt;
        if self.burn_timer >= interval {
            self.burn_timer -= interval;
            self.damage(amount, None, rng);
        }
    }

    /// Visual rotation of the whole body for the death animation (0..1).
    pub fn death_progress(&self) -> f32 {
        self.dying.map_or(0.0, |t| (t / (DEATH_TIME * 0.6)).min(1.0))
    }
}

/// Whether stepping in `dir` would drop more than [`MAX_SAFE_DROP`] blocks.
pub fn is_cliff<W: BlockSource + ?Sized>(world: &W, pos: DVec3, dir: DVec3, shape: Shape) -> bool {
    let probe = pos + dir * (shape.half_width + 0.35);
    let (x, z) = (probe.x.floor() as i32, probe.z.floor() as i32);
    let y = (pos.y + 0.01).floor() as i32;
    // A block at foot level ahead is a step up, not a drop.
    for dy in 0..=MAX_SAFE_DROP + 1 {
        match world.block(IVec3::new(x, y - dy, z)) {
            None => return true,
            Some(b) if b.is_solid() || b.is_water() => return false,
            Some(_) => {}
        }
    }
    true
}

/// Rough sky light (0..1) at a point: 1 in the open, dimmed under water or
/// leaves, and falling off with distance from the nearest open column
/// under overhangs; 0 deep underground.
pub fn sky_light<W: MobWorld + ?Sized>(world: &W, p: DVec3) -> f32 {
    let cell = p.floor().as_ivec3();
    if world.exposed(cell) {
        return 1.0;
    }
    // Straight up through translucent blocks (water, leaves)?
    let mut level = 15i32;
    let mut y = cell.y + 1;
    let top = world.surface(cell.x, cell.z).unwrap_or(cell.y);
    while y <= top && level > 0 {
        match world.block(IVec3::new(cell.x, y, cell.z)) {
            Some(b) if b.is_opaque() => {
                level = 0;
                break;
            }
            Some(b) if b.light_opacity() > 0 => level -= 2,
            _ => {}
        }
        y += 1;
    }
    // Otherwise light spilling in from the side.
    for d in 1..=4i32 {
        let side = 15 - 2 * d;
        if side <= level {
            break;
        }
        let ring = (-d..=d).flat_map(|a| [(a, -d), (a, d), (-d, a), (d, a)]);
        if ring.into_iter().any(|(dx, dz)| world.exposed(cell + IVec3::new(dx, 0, dz))) {
            level = side;
            break;
        }
    }
    level.max(0) as f32 / 15.0
}

/// Whether nothing solid lies on the straight line between two points.
pub(super) fn line_of_sight<W: BlockSource + ?Sized>(world: &W, from: DVec3, to: DVec3) -> bool {
    let d = to - from;
    let steps = (d.length() * 4.0).ceil().max(1.0) as i32;
    (1..steps).all(|i| {
        let p = from + d * (i as f64 / steps as f64);
        !world.block(p.floor().as_ivec3()).is_some_and(|b| b.is_solid())
    })
}

pub fn yaw_dir(yaw: f32) -> DVec3 {
    let (s, c) = yaw.sin_cos();
    DVec3::new(c as f64, 0.0, s as f64)
}

/// Angle wrapped to [-PI, PI).
fn wrap(a: f32) -> f32 {
    (a + PI).rem_euclid(TAU) - PI
}

fn turn_toward(from: f32, to: f32, max: f32) -> f32 {
    let d = wrap(to - from);
    wrap(from + d.clamp(-max, max))
}
