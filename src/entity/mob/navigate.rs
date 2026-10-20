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
use crate::entity::path::{self, Caps, Evaluator, Malus, Mover, PathType};
use crate::entity::{Ctx, MobWorld, Rng};
use crate::world::block::Block;

impl MobKind {
    /// Whether this kind walks with Java's ground navigation. Flyers, hopping
    /// slimes and swimmers steer themselves. Striders walk paths across lava
    /// (`StriderPathNavigation`).
    pub(crate) fn walks_paths(self) -> bool {
        !self.is_aquatic() && !self.is_cube() && !matches!(self, MobKind::Ghast | MobKind::Pillager)
    }

    /// `RandomStrollGoal.interval`: the mean ticks between stroll starts.
    pub(crate) fn stroll_interval(self) -> f32 {
        if self == MobKind::Strider { 60.0 } else { 120.0 }
    }

    /// Swims at the surface: Java's `FloatGoal` or brain `Swim` mobs, and the
    /// self-steering kinds that keep VoxelCraft's buoyancy. Others sink.
    /// Striders keep VoxelCraft's buoyancy in water, which hurts them.
    pub(crate) fn floats(self) -> bool {
        !self.walks_paths() || self == MobKind::Strider || self.nav_caps(0.6, 1.8).can_float
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
        caps.evaluator = match self {
            K::Cod | K::Salmon | K::TropicalFish | K::Pufferfish | K::Guardian | K::ElderGuardian => {
                Evaluator::Swim { breaching: false }
            }
            K::Dolphin => Evaluator::Swim { breaching: true },
            K::Drowned | K::Axolotl | K::Turtle => Evaluator::Amphibious { shallow: false },
            K::Parrot => Evaluator::Fly,
            _ => Evaluator::Walk,
        };
        if matches!(caps.evaluator, Evaluator::Swim { .. }) {
            caps.can_pass_doors = false;
        }
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
            K::Drowned
            | K::Axolotl
            | K::Guardian
            | K::ElderGuardian
            | K::Cod
            | K::Salmon
            | K::TropicalFish
            | K::Pufferfish
            | K::Dolphin => m.with(T::Water, 0.0),
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
        // `FollowOwnerGoal.start` sets the water malus to 0 until it stops.
        if self.goals.water_free {
            caps.malus = caps.malus.with(PathType::Water, 0.0);
        }
        caps
    }

    pub(crate) fn mover(&self, speed: f64) -> Mover {
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
        let wanted = self.nav.tick(world, &caps, &mover);
        // `followThePath` sets the move control's wanted position; swimmers
        // steer by it in `swim`.
        if self.kind.java_swimmer() {
            self.set_wanted(wanted, speed);
        }
        let wanted = wanted?;
        if caps.evaluator == Evaluator::Fly
            || (matches!(caps.evaluator, Evaluator::Swim { .. }) || caps.evaluator.amphibious()) && self.in_water
        {
            self.pathing = true;
            return Some((wanted - self.pos).normalize_or_zero());
        }
        let (yaw, jump) = path::steer(world, self.pos, self.yaw, wanted, caps.step, caps.width)?;
        self.yaw = yaw;
        self.jump |= jump;
        self.pathing = true;
        Some(yaw_dir(yaw))
    }

    /// Chase a moving goal the way `MeleeAttackGoal.tick` does, returning the
    /// direction to walk this tick. `hunting` is whether the goal is an attack
    /// target (`getTarget() != null`), which lets `getMaxFallDistance` risk
    /// spare health; followers without a target keep the 3-block limit.
    /// Spiders climb straight at a goal their path can't reach
    /// (`WallClimberNavigation`).
    pub(crate) fn pursue<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        goal: DVec3,
        speed: f64,
        sees: bool,
        hunting: bool,
        rng: &mut Rng,
    ) -> Option<DVec3> {
        let caps = self.caps(hunting);
        if !self.kind.walks_paths() && caps.evaluator == Evaluator::Walk {
            let flat = (goal - self.pos) * DVec3::new(1.0, 0.0, 1.0);
            return Some(flat.normalize_or(DVec3::X));
        }
        self.repath_in = (self.repath_in - 1).max(0);
        // Airborne with nothing to follow: `createPath` would fail and cost
        // the 15-tick penalty, which a mob spawned this tick (not yet marked
        // on the ground) must not pay.
        let airborne = !caps.evaluator.can_update(&self.mover(speed), caps.can_float);
        // A spider pressed against a wall keeps its wanted position while it
        // climbs (`Spider.onClimbable` follows the horizontal collision).
        let climbing = matches!(self.kind, MobKind::Spider | MobKind::CaveSpider) && self.blocked;
        if airborne && self.nav.is_done() && !climbing {
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
            if matches!(self.kind, MobKind::Spider | MobKind::CaveSpider) {
                self.nav.climb_target = Some(goal.floor().as_ivec3());
            }
            self.repath_in = 4 + rng.next_int(7) as i32;
            let d2 = self.pos.distance_squared(goal);
            if d2 > 1024.0 {
                self.repath_in += 10;
            } else if d2 > 256.0 {
                self.repath_in += 5;
            }
            if !self.walk_to(world, goal.floor().as_ivec3(), 0, speed, hunting) {
                self.repath_in += 15;
            }
            if retry {
                self.repath_in = self.repath_in.max(20);
            }
        }
        if let Some(dir) = self.follow_path(world, speed, hunting) {
            return Some(dir);
        }
        // `WallClimberNavigation`: past the path's end, keep pushing toward
        // the destination so the horizontal collision starts a climb.
        if matches!(self.kind, MobKind::Spider | MobKind::CaveSpider) {
            let flat = (goal - self.pos) * DVec3::new(1.0, 0.0, 1.0);
            if flat.length_squared() > (self.shape().half_width * 2.0).powi(2) {
                return Some(flat.normalize_or(DVec3::X));
            }
        }
        None
    }

    /// The stroll speed: the table's `RandomStrollGoal` speed modifier on the
    /// movement speed attribute, else VoxelCraft's wander speed.
    pub(crate) fn stroll_speed(&self) -> f64 {
        self.table_speed(|g| matches!(g, super::goals::Goal::Stroll)).unwrap_or_else(|| self.kind.wander_speed())
    }

    /// The panic speed: the table's `PanicGoal` speed modifier, else
    /// VoxelCraft's 3.4 blocks a second.
    pub(crate) fn panic_speed(&self) -> f64 {
        self.table_speed(|g| matches!(g, super::goals::Goal::Panic)).unwrap_or(3.4)
    }

    /// Forget the pursuit so the next chase plans afresh.
    pub(crate) fn stop_navigation(&mut self) {
        self.nav.stop();
        self.nav.climb_target = None;
        self.pathed = None;
        self.repath_in = 0;
    }

    /// `Mob.getWalkTargetValue`: animals favour grass and light, monsters
    /// darkness, everyone else is indifferent.
    fn walk_value<W: MobWorld + ?Sized>(&self, world: &W, ctx: &Ctx, p: IVec3) -> f32 {
        if self.kind == MobKind::Strider {
            if world.block(p).is_some_and(Block::is_lava) {
                return 10.0;
            }
            if world.block(self.pos.floor().as_ivec3()).is_some_and(Block::is_lava) {
                return f32::NEG_INFINITY;
            }
            return 0.0;
        }
        let animal = self.kind.is_animal();
        if !animal && !self.kind.is_hostile() {
            return 0.0;
        }
        // `Guardian.getWalkTargetValue`: water scores 10 plus the light cost.
        let guardian_water = matches!(self.kind, MobKind::Guardian | MobKind::ElderGuardian)
            && world.block(p).is_some_and(|b| b.holds_water());
        if animal && world.block(p - IVec3::Y) == Some(Block::GRASS) {
            return 10.0;
        }
        // `getPathfindingCostFromLightLevels`: brightness - 0.5 over the raw light.
        let sky = world.sky_light_level(p).unwrap_or(0).saturating_sub(ctx.spawn_light.sky_darken);
        let raw = sky.max(world.block_light(p)) as f32 / 15.0;
        let curve = raw / (4.0 - 3.0 * raw);
        let ambient = ctx.dimension.ambient_light();
        let cost = ambient + (1.0 - ambient) * curve - 0.5;
        if guardian_water {
            10.0 + cost
        } else if animal {
            cost
        } else {
            -cost
        }
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
            if !(crate::world::chunk::WORLD_MIN_Y..=319).contains(&p.y)
                || !(self.kind == MobKind::Strider && world.block(p).is_some_and(Block::is_lava)
                    || self.stable_destination(world, p))
            {
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
            // Java starts from negative infinity with a strict comparison, so
            // a strider in lava never picks dry land.
            let value = self.walk_value(world, ctx, p);
            if value > best.map_or(f32::NEG_INFINITY, |(b, _)| b) {
                best = Some((value, p));
            }
        }
        best.map(|(_, p)| p)
    }

    /// `DefaultRandomPos.getPosAway(h, v, from)`: the best-valued of ten
    /// spots in the half-plane facing away from `from`
    /// (`RandomPos.generateRandomDirectionWithinRadians` with a quarter turn
    /// either side), on a stable cell without a path malus.
    pub(crate) fn pos_away<W: MobWorld + ?Sized>(
        &self,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        h: i32,
        v: i32,
        from: DVec3,
    ) -> Option<IVec3> {
        let caps = self.caps(false);
        let away = self.pos - from;
        let mut best: Option<(f32, IVec3)> = None;
        for _ in 0..10 {
            let angle = away.z.atan2(away.x) - std::f64::consts::FRAC_PI_2;
            let angle = angle + (2.0 * rng.next_f32() - 1.0) as f64 * std::f64::consts::FRAC_PI_2;
            let r = (rng.next_f32() as f64).sqrt() * h as f64 * std::f64::consts::SQRT_2;
            let (x, z) = (-r * angle.sin(), r * angle.cos());
            if x.abs() > h as f64 || z.abs() > h as f64 {
                continue;
            }
            let y = rng.next_int((2 * v + 1) as u32) as i32 - v;
            let p = (self.pos + DVec3::new(x.floor(), y as f64, z.floor())).floor().as_ivec3();
            if !(crate::world::chunk::WORLD_MIN_Y..=319).contains(&p.y)
                || !self.stable_destination(world, p)
                || caps.malus.get(path::static_type(world, p)) != 0.0
            {
                continue;
            }
            let value = self.walk_value(world, ctx, p);
            if value > best.map_or(f32::NEG_INFINITY, |(b, _)| b) {
                best = Some((value, p));
            }
        }
        best.map(|(_, p)| p)
    }

    /// `WaterAvoidingRandomStrollGoal.getPosition`, or the plain
    /// `RandomStrollGoal`'s `DefaultRandomPos` for striders.
    fn stroll_target<W: MobWorld + ?Sized>(&self, world: &W, ctx: &Ctx, rng: &mut Rng) -> Option<IVec3> {
        if self.kind == MobKind::Strider {
            return self.random_pos(world, ctx, rng, 10, 7, false);
        }
        if self.in_water {
            return self
                .random_pos(world, ctx, rng, 15, 7, true)
                .or_else(|| self.random_pos(world, ctx, rng, 10, 7, false));
        }
        let land = rng.next_f32() >= 0.001;
        self.random_pos(world, ctx, rng, 10, 7, land)
    }

    /// Start a stroll (`RandomStrollGoal.start`): pick a target and set off.
    /// Mobs idle for 100 ticks (`noActionTime`: no player within 32 blocks
    /// and unhurt) stop strolling.
    pub(crate) fn try_stroll<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx, rng: &mut Rng) -> bool {
        if self.idle >= 5.0
            || !self.caps(false).evaluator.can_update(&self.mover(self.stroll_speed()), self.caps(false).can_float)
        {
            return false;
        }
        let Some(goal) = (if self.kind == MobKind::Parrot {
            (0..10).find_map(|_| {
                let p = self.pos.floor().as_ivec3()
                    + IVec3::new(
                        rng.next_int(21) as i32 - 10,
                        rng.next_int(15) as i32 - 7,
                        rng.next_int(21) as i32 - 10,
                    );
                (world.block(p) == Some(Block::AIR)).then_some(p)
            })
        } else {
            self.stroll_target(world, ctx, rng)
        }) else {
            return false;
        };
        let speed = self.stroll_speed();
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

impl Mob {
    /// `isStableDestination` of the mob's navigation: something solid to
    /// stand on (`PathNavigation`), anything below but air
    /// (`AmphibiousPathNavigation`, for axolotls and turtles), or not a solid
    /// cube itself (`WaterBoundPathNavigation`).
    fn stable_destination<W: MobWorld + ?Sized>(&self, world: &W, p: IVec3) -> bool {
        match self.kind {
            MobKind::Axolotl | MobKind::Turtle => world.block(p - IVec3::Y).is_some_and(|b| b != Block::AIR),
            _ if matches!(self.caps(false).evaluator, Evaluator::Swim { .. }) => {
                world.block(p).is_some_and(|b| !(b.is_solid() && b.is_opaque()))
            }
            _ => world.block(p - IVec3::Y).is_some_and(|b| b.is_solid() && b.is_opaque()),
        }
    }

    /// `BehaviorUtils.getRandomSwimmablePos`: `DefaultRandomPos.getPos`,
    /// retried up to ten more times until it lands in water.
    pub(crate) fn swimmable_pos<W: MobWorld + ?Sized>(
        &self,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        h: i32,
        v: i32,
    ) -> Option<IVec3> {
        let mut pos = self.random_pos(world, ctx, rng, h, v, false);
        let mut count = 0;
        while let Some(p) = pos {
            if world.block(p).is_some_and(|b| b.holds_water()) || count >= 10 {
                break;
            }
            count += 1;
            pos = self.random_pos(world, ctx, rng, h, v, false);
        }
        pos
    }

    /// `DefaultRandomPos.getPos(h, v)`.
    pub(crate) fn default_random_pos<W: MobWorld + ?Sized>(
        &self,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        h: i32,
        v: i32,
    ) -> Option<IVec3> {
        self.random_pos(world, ctx, rng, h, v, false)
    }

    /// `LandRandomPos.getPos(10, 7)`.
    pub(crate) fn land_random_pos<W: MobWorld + ?Sized>(&self, world: &W, ctx: &Ctx, rng: &mut Rng) -> Option<IVec3> {
        self.random_pos(world, ctx, rng, 10, 7, true)
    }

    /// `RandomStroll.getTargetSwimPos`: swimmable spots in widening tiers,
    /// each tier after the first continuing the direction of the last.
    pub(crate) fn swim_stroll_pos<W: MobWorld + ?Sized>(&self, world: &W, ctx: &Ctx, rng: &mut Rng) -> Option<DVec3> {
        const TIERS: [(i32, i32); 6] = [(1, 1), (3, 3), (5, 5), (6, 5), (7, 7), (10, 7)];
        let mut fallback: Option<DVec3> = None;
        let mut target = None;
        for (h, v) in TIERS {
            target = match fallback {
                None => self.swimmable_pos(world, ctx, rng, h, v).map(|p| p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)),
                Some(f) => {
                    let dir = (f - self.pos).normalize_or_zero();
                    Some(self.pos + dir * DVec3::new(h as f64, v as f64, h as f64))
                }
            };
            match target {
                Some(t) if world.block(t.floor().as_ivec3()).is_some_and(|b| b.holds_water()) => fallback = Some(t),
                _ => return fallback,
            }
        }
        target
    }
}
