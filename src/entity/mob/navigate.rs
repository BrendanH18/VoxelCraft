//! Mob movement through Java's ground navigation ([`crate::entity::path`]):
//! per-kind navigation capabilities, `MeleeAttackGoal`-style pursuit, and
//! `RandomStrollGoal`/`PanicGoal` targets from `DefaultRandomPos` and
//! `LandRandomPos`.
//!
//! Every pursuit (melee chasers, golems and zombies hunting mobs, tempted,
//! breeding, leashed and following animals) repaths on Java's melee cadence:
//! 4–10 ticks, plus 5 beyond 16 blocks and 10 beyond 32, plus 15 when no path
//! exists, and only when the goal moved a block (or 5% of the time).
//! Java's follow-owner, tempt and breed goals use their own fixed cadences;
//! the path each finds is the same, only how often it is refreshed differs.
use glam::{DVec3, IVec3};

use super::{Ai, Mob, MobKind, yaw_dir};
use crate::entity::path::{self, Caps, Malus, Mover, PathType};
use crate::entity::{Ctx, MobWorld, Rng};
use crate::world::block::Block;

impl MobKind {
    /// Whether this kind walks with Java's ground navigation. Flyers, hopping
    /// slimes, swimmers and lava-walking striders steer themselves.
    pub(crate) fn walks_paths(self) -> bool {
        !self.is_aquatic()
            && !self.is_cube()
            && !matches!(self, MobKind::Ghast | MobKind::Parrot | MobKind::Strider | MobKind::Pillager)
    }

    /// Swims at the surface: Java's `FloatGoal` or brain `Swim` mobs, and the
    /// self-steering kinds that keep VoxelCraft's buoyancy. Others sink.
    pub(crate) fn floats(self) -> bool {
        !self.walks_paths() || self.nav_caps(0.6, 1.8).can_float
    }

    /// `Animal`'s constructor costs (fire avoidance).
    fn is_animal(self) -> bool {
        matches!(
            self,
            MobKind::Pig
                | MobKind::Cow
                | MobKind::Sheep
                | MobKind::Chicken
                | MobKind::Cat
                | MobKind::Wolf
                | MobKind::Fox
                | MobKind::Parrot
                | MobKind::Rabbit
                | MobKind::Goat
                | MobKind::Horse
                | MobKind::Donkey
                | MobKind::Mule
                | MobKind::Llama
                | MobKind::TraderLlama
                | MobKind::Camel
                | MobKind::Turtle
                | MobKind::Hoglin
                | MobKind::Strider
        )
    }

    /// Java's navigation setup for this kind: `maxUpStep`, door and float
    /// flags, `FOLLOW_RANGE` (and `setRequiredPathLength`) and the
    /// `setPathfindingMalus` calls in each mob's constructor.
    pub(crate) fn nav_caps(self, width: f32, height: f32) -> Caps {
        use MobKind as K;
        use PathType as T;
        let mut caps = Caps::new(width, height);
        caps.step = match self {
            K::Camel => 1.5,
            K::Horse
            | K::Donkey
            | K::Mule
            | K::Llama
            | K::TraderLlama
            | K::IronGolem
            | K::Enderman
            | K::Drowned
            | K::Turtle
            | K::Axolotl => 1.0,
            _ => 0.6,
        };
        caps.can_open_doors = matches!(self, K::Villager | K::WanderingTrader | K::Piglin | K::PiglinBrute);
        caps.can_float = matches!(
            self,
            K::Pig
                | K::Cow
                | K::Sheep
                | K::Chicken
                | K::Horse
                | K::Donkey
                | K::Mule
                | K::Llama
                | K::TraderLlama
                | K::Cat
                | K::Wolf
                | K::Fox
                | K::Parrot
                | K::Rabbit
                | K::Goat
                | K::Camel
                | K::Creeper
                | K::Spider
                | K::CaveSpider
                | K::Witch
                | K::Enderman
                | K::Silverfish
                | K::Villager
                | K::WanderingTrader
                | K::Pillager
                | K::Slime
                | K::MagmaCube
        );
        caps.can_walk_over_fences = self == K::Camel;
        caps.stand_on_lava = self == K::Strider;
        caps = caps.follow_range(match self {
            K::Zombie | K::Husk | K::Drowned | K::ZombieVillager | K::ZombifiedPiglin => 35.0,
            K::Enderman => 64.0,
            K::Blaze => 48.0,
            K::Ghast => 100.0,
            K::Fox | K::Pillager => 32.0,
            K::Llama | K::TraderLlama => 40.0,
            K::PiglinBrute => 12.0,
            _ => 16.0,
        });
        let mut m = Malus::default();
        if self.is_animal() || matches!(self, K::Villager | K::WanderingTrader | K::Piglin | K::PiglinBrute) {
            m = m.with(T::FireInNeighbor, 16.0).with(T::Fire, -1.0);
        }
        m = match self {
            K::Chicken => m.with(T::Water, 0.0),
            K::Turtle => m
                .with(T::Water, 0.0)
                .with(T::DoorIronClosed, -1.0)
                .with(T::DoorWoodClosed, -1.0)
                .with(T::DoorOpen, -1.0),
            K::Wolf | K::Goat => m.with(T::PowderSnow, -1.0).with(T::OnTopOfPowderSnow, -1.0),
            K::Fox => m.with(T::DamagingInNeighbor, 0.0).with(T::Damaging, 0.0),
            K::Parrot => m.with(T::FireInNeighbor, -1.0).with(T::Fire, -1.0).with(T::Cocoa, -1.0),
            K::Blaze => m.with(T::Water, -1.0).with(T::Lava, 8.0).with(T::FireInNeighbor, 0.0).with(T::Fire, 0.0),
            K::Strider => m.with(T::Water, -1.0).with(T::Lava, 0.0).with(T::FireInNeighbor, 0.0).with(T::Fire, 0.0),
            K::Enderman => m.with(T::Water, -1.0),
            K::WitherSkeleton | K::ZombifiedPiglin => m.with(T::Lava, 8.0),
            K::Drowned | K::Axolotl | K::Guardian | K::ElderGuardian => m.with(T::Water, 0.0),
            _ => m,
        };
        caps.malus = m;
        caps
    }
}

impl Mob {
    /// Navigation capabilities right now: `getMaxFallDistance` lets a mob
    /// with a target risk the health it can spare.
    pub(crate) fn caps(&self, hunting: bool) -> Caps {
        let shape = self.shape();
        let mut caps = self.kind.nav_caps((shape.half_width * 2.0) as f32, shape.height as f32);
        if hunting {
            let id = match self.difficulty {
                crate::simulation::difficulty::Difficulty::Peaceful => 0,
                crate::simulation::difficulty::Difficulty::Easy => 1,
                crate::simulation::difficulty::Difficulty::Normal => 2,
                crate::simulation::difficulty::Difficulty::Hard => 3,
            };
            let spare = (self.health - self.max_health() * 0.33) as i32 - (3 - id) * 4;
            caps.max_fall = (spare.max(0) as f32 + 3.0).floor() as i32;
        }
        caps
    }

    fn mover(&self, speed: f64) -> Mover {
        Mover {
            pos: self.pos,
            on_ground: self.on_ground,
            in_water: self.in_water,
            floating: self.in_water && self.kind.floats(),
            riding: self.riding.is_some(),
            speed,
        }
    }

    /// Plan a path to `goal` and start walking it (`moveTo(x, y, z, speed)`).
    pub(crate) fn walk_to<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        goal: IVec3,
        reach: i32,
        speed: f64,
        hunting: bool,
    ) -> bool {
        let caps = self.caps(hunting);
        let mover = self.mover(speed);
        self.nav.walk_to(world, &caps, &mover, goal, reach)
    }

    /// One tick along the current path: `PathNavigation.tick` then
    /// `MoveControl` (turn up to 90 degrees, jump onto steps). Returns the
    /// direction to walk, or `None` once the path is finished.
    pub(crate) fn follow_path<W: MobWorld + ?Sized>(&mut self, world: &W, speed: f64, hunting: bool) -> Option<DVec3> {
        let caps = self.caps(hunting);
        let mover = self.mover(speed);
        let wanted = self.nav.tick(world, &caps, &mover)?;
        let (yaw, jump) = path::steer(world, self.pos, self.yaw, wanted, caps.step, caps.width)?;
        self.yaw = yaw;
        self.jump |= jump;
        self.pathing = true;
        Some(yaw_dir(yaw))
    }

    /// Chase a moving goal the way `MeleeAttackGoal.tick` does, returning the
    /// direction to walk this tick. Spiders climb straight at a goal their
    /// path can't reach (`WallClimberNavigation`).
    pub(crate) fn pursue<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        goal: DVec3,
        speed: f64,
        sees: bool,
        rng: &mut Rng,
    ) -> Option<DVec3> {
        if !self.kind.walks_paths() {
            let flat = (goal - self.pos) * DVec3::new(1.0, 0.0, 1.0);
            return Some(flat.normalize_or(DVec3::X));
        }
        self.repath_in = (self.repath_in - 1).max(0);
        // Airborne with nothing to follow: `createPath` would fail and cost
        // the 15-tick penalty, which a mob spawned this tick (not yet marked
        // on the ground) must not pay.
        let airborne = !self.on_ground && !self.in_water && self.riding.is_none();
        if airborne && self.nav.is_done() {
            return None;
        }
        // `MeleeAttackGoal.canUse` retries an unseen target every 20 ticks
        // once the last path is used up.
        let retry = !sees && self.nav.is_done() && self.repath_in == 0;
        if (sees || retry)
            && self.repath_in <= 0
            && self.pathed.is_none_or(|p| goal.distance_squared(p) >= 1.0 || rng.next_f32() < 0.05)
        {
            self.pathed = Some(goal);
            self.repath_in = 4 + rng.next_int(7) as i32;
            let d2 = self.pos.distance_squared(goal);
            if d2 > 1024.0 {
                self.repath_in += 10;
            } else if d2 > 256.0 {
                self.repath_in += 5;
            }
            if !self.walk_to(world, goal.floor().as_ivec3(), 0, speed, true) {
                self.repath_in += 15;
            }
            if retry {
                self.repath_in = self.repath_in.max(20);
            }
        }
        if let Some(dir) = self.follow_path(world, speed, true) {
            return Some(dir);
        }
        if matches!(self.kind, MobKind::Spider | MobKind::CaveSpider) {
            let flat = (goal - self.pos) * DVec3::new(1.0, 0.0, 1.0);
            if flat.length_squared() > (self.shape().half_width * 2.0).powi(2) {
                return Some(flat.normalize_or(DVec3::X));
            }
        }
        None
    }

    /// Forget the pursuit so the next chase plans afresh.
    pub(crate) fn stop_navigation(&mut self) {
        self.nav.stop();
        self.pathed = None;
        self.repath_in = 0;
    }

    /// `Mob.getWalkTargetValue`: animals favour grass and light, monsters
    /// darkness, everyone else is indifferent.
    fn walk_value<W: MobWorld + ?Sized>(&self, world: &W, ctx: &Ctx, p: IVec3) -> f32 {
        let animal = self.kind.is_animal();
        if !animal && !self.kind.is_hostile() {
            return 0.0;
        }
        if animal && world.block(p - IVec3::Y) == Some(Block::GRASS) {
            return 10.0;
        }
        // `getPathfindingCostFromLightLevels`: brightness - 0.5 over the raw light.
        let sky = world.sky_light_level(p).unwrap_or(0).saturating_sub(ctx.spawn_light.sky_darken);
        let raw = sky.max(world.block_light(p)) as f32 / 15.0;
        let curve = raw / (4.0 - 3.0 * raw);
        let ambient = ctx.dimension.ambient_light();
        let cost = ambient + (1.0 - ambient) * curve - 0.5;
        if animal { cost } else { -cost }
    }

    /// `RandomPos.generateRandomPos`: the best-valued of ten candidates.
    fn random_pos<W: MobWorld + ?Sized>(
        &self,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        h: i32,
        v: i32,
        land: bool,
    ) -> Option<IVec3> {
        let caps = self.caps(false);
        let mut best: Option<(f32, IVec3)> = None;
        for _ in 0..10 {
            let dx = rng.next_int((2 * h + 1) as u32) as i32 - h;
            let dy = rng.next_int((2 * v + 1) as u32) as i32 - v;
            let dz = rng.next_int((2 * h + 1) as u32) as i32 - h;
            let mut p = (self.pos + IVec3::new(dx, dy, dz).as_dvec3()).floor().as_ivec3();
            if !(crate::world::chunk::WORLD_MIN_Y..=319).contains(&p.y) || !stable(world, p) {
                continue;
            }
            if land {
                // `LandRandomPos.movePosUpOutOfSolid`, then no water.
                while p.y <= 319 && world.block(p).is_none_or(|b| b.is_solid()) {
                    p.y += 1;
                }
                if world.block(p).is_some_and(|b| b.holds_water() || b.is_water()) {
                    continue;
                }
            }
            if caps.malus.get(path::static_type(world, p)) != 0.0 {
                continue;
            }
            let value = self.walk_value(world, ctx, p);
            if best.is_none_or(|(b, _)| value > b) {
                best = Some((value, p));
            }
        }
        best.map(|(_, p)| p)
    }

    /// `WaterAvoidingRandomStrollGoal.getPosition`.
    fn stroll_target<W: MobWorld + ?Sized>(&self, world: &W, ctx: &Ctx, rng: &mut Rng) -> Option<IVec3> {
        if self.in_water {
            return self
                .random_pos(world, ctx, rng, 15, 7, true)
                .or_else(|| self.random_pos(world, ctx, rng, 10, 7, false));
        }
        let land = rng.next_f32() >= 0.001;
        self.random_pos(world, ctx, rng, 10, 7, land)
    }

    /// Start a stroll (`RandomStrollGoal.start`): pick a target and set off.
    /// Mobs more than 32 blocks from every player stop strolling, as Java's
    /// `noActionTime` does, unless they are persistent.
    pub(crate) fn try_stroll<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx, rng: &mut Rng) -> bool {
        let idle_far = !self.persistent && ctx.nearest_player_dist2(self.pos).is_none_or(|d| d > 32.0 * 32.0);
        if idle_far || !self.on_ground && !self.in_water {
            return false;
        }
        let Some(goal) = self.stroll_target(world, ctx, rng) else { return false };
        let speed = self.kind.wander_speed();
        if self.walk_to(world, goal, 1, speed, false) {
            self.ai = Ai::Wander;
            self.ai_timer = 30.0;
            return true;
        }
        false
    }

    /// `PanicGoal`: run for water when burning, otherwise to a random spot
    /// within five blocks; a new one each time the last is reached.
    pub(crate) fn panic_target<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx, rng: &mut Rng, speed: f64) {
        if !self.nav.is_done() {
            return;
        }
        let feet = self.pos.floor().as_ivec3();
        let water = (self.burning && crate::physics::collision_top(world, feet).is_none())
            .then(|| {
                (0..=6i32).find_map(|r| {
                    let mut found = None;
                    'search: for dy in [0, -1, 1] {
                        for dx in -r..=r {
                            let dz = r - dx.abs();
                            for dz in [dz, -dz] {
                                let p = feet + IVec3::new(dx, dy, dz);
                                if (dx.abs() + dz.abs() + dy.abs()) <= 6
                                    && world.block(p).is_some_and(|b| b.holds_water() || b.is_water())
                                {
                                    found = Some(p);
                                    break 'search;
                                }
                            }
                        }
                    }
                    found
                })
            })
            .flatten();
        if let Some(goal) = water.or_else(|| self.random_pos(world, ctx, rng, 5, 4, false)) {
            self.walk_to(world, goal, 1, speed, false);
        }
    }
}

/// `PathNavigation.isStableDestination`: something solid to stand on.
fn stable<W: MobWorld + ?Sized>(world: &W, p: IVec3) -> bool {
    world.block(p - IVec3::Y).is_some_and(|b| b.is_solid() && b.is_opaque())
}
