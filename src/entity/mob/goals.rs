//! Java's `GoalSelector` for the movement goals that animals (and a few
//! monsters that flee them) register in 26.3: `PanicGoal`, `BreedGoal`,
//! `TemptGoal`, `FollowParentGoal`, `FollowOwnerGoal`, `AvoidEntityGoal`,
//! the strider's lava goal and `RandomStrollGoal`, in each mob's
//! `registerGoals` priority order. Brain-driven animals (goats, camels,
//! axolotls, hoglins) use their idle activity's order instead.
//!
//! Like Java, a goal runs until `canContinueToUse` fails, and only a goal of
//! a strictly better (lower) priority that `canUse` takes over. Goals start
//! and tick on the selector's full update, every other tick
//! (`(tickCount + id) % 2 == 0`); navigation and steering run every tick.
//! Every timer here (`TemptGoal.calmDown`, follow goals'
//! `timeToRecalcPath`, `BreedGoal.loveTime`) counts those updates, which is
//! what `reducedTickDelay`/`adjustedTickDelay` express in Java.
//!
//! Panic and strolling stay in [`Ai`] (`Ai::Panic`, `Ai::Wander` on a path);
//! the selector treats them as running goals of their table priority. The
//! strider's lava goal keeps its state in its `NetherMob`.
use glam::{DVec3, IVec3};
use rustc_hash::FxHashMap;

use super::{Ai, Mob, MobKind, line_of_sight};
use crate::entity::path::{self, PathType};
use crate::entity::{Ctx, MobWorld, PlayerId, Rng};
use crate::item::Item;

/// What an `AvoidEntityGoal` flees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Avoided {
    Player,
    Wolf,
    Llama,
    Cat,
    /// Java's `Monster` class.
    Monster,
    /// `Zombie` and its subclasses (husks, drowned, zombie villagers and
    /// zombified piglins).
    Zombie,
    Pillager,
    Zoglin,
}

/// `AvoidEntityGoal(mob, class, maxDist, walkSpeedModifier, sprintSpeedModifier)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Avoid {
    pub what: Avoided,
    pub dist: f64,
    pub walk: f64,
    pub sprint: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Goal {
    Panic,
    Breed,
    Tempt,
    /// `FollowParentGoal`: babies keep within 3–16 blocks of an adult.
    FollowParent,
    /// Brain `BabyFollowAdult` with `ADULT_FOLLOW_RANGE` 5–16.
    FollowAdult,
    FollowOwner {
        start: f64,
        stop: f64,
    },
    Avoid(Avoid),
    /// `Strider.StriderGoToLavaGoal`.
    Lava,
    /// `TurtleLayEggGoal` (`MoveToBlockGoal`, every tick).
    LayEgg,
    /// `TurtleGoToWaterGoal` (`MoveToBlockGoal`, every tick).
    GoToWater,
    /// `TurtleGoHomeGoal` and `TurtleTravelGoal`: flagless goals that run
    /// beside the move goals (`turtle::Mob::turtle_side_goals`).
    GoHome,
    Travel,
    Stroll,
}

/// One `goalSelector.addGoal(priority, goal)`; `speed` is the goal's speed
/// modifier on the mob's movement speed attribute.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Entry {
    pub priority: u8,
    pub goal: Goal,
    pub speed: f64,
}

const fn e(priority: u8, goal: Goal, speed: f64) -> Entry {
    Entry { priority, goal, speed }
}
const fn avoid(priority: u8, what: Avoided, dist: f64, walk: f64, sprint: f64) -> Entry {
    e(priority, Goal::Avoid(Avoid { what, dist, walk, sprint }), walk)
}
const fn owner(priority: u8, start: f64, stop: f64) -> Entry {
    e(priority, Goal::FollowOwner { start, stop }, 1.0)
}

use Goal as G;
// `AbstractCow.registerGoals`.
const COW: &[Entry] = &[
    e(1, G::Panic, 2.0),
    e(2, G::Breed, 1.0),
    e(3, G::Tempt, 1.25),
    e(4, G::FollowParent, 1.25),
    e(5, G::Stroll, 1.0),
];
const PIG: &[Entry] =
    &[e(1, G::Panic, 1.25), e(3, G::Breed, 1.0), e(4, G::Tempt, 1.2), e(5, G::FollowParent, 1.1), e(6, G::Stroll, 1.0)];
const SHEEP: &[Entry] =
    &[e(1, G::Panic, 1.25), e(2, G::Breed, 1.0), e(3, G::Tempt, 1.1), e(4, G::FollowParent, 1.1), e(6, G::Stroll, 1.0)];
const CHICKEN: &[Entry] =
    &[e(1, G::Panic, 1.4), e(2, G::Breed, 1.0), e(3, G::Tempt, 1.0), e(4, G::FollowParent, 1.1), e(5, G::Stroll, 1.0)];
// Wolves have no tempt goal; `BegGoal` only looks.
const WOLF: &[Entry] = &[
    e(1, G::Panic, 1.5),
    avoid(3, Avoided::Llama, 24.0, 1.5, 1.5),
    owner(6, 10.0, 2.0),
    e(7, G::Breed, 1.0),
    e(8, G::Stroll, 1.0),
];
// `CatTemptGoal` is registered first at 4, then `reassessTameGoals` adds the
// untamed cat's player avoidance at 4.
const CAT: &[Entry] = &[
    e(1, G::Panic, 1.5),
    e(4, G::Tempt, 0.6),
    avoid(4, Avoided::Player, 16.0, 0.8, 1.33),
    owner(6, 10.0, 5.0),
    e(10, G::Breed, 0.8),
    e(11, G::Stroll, 0.8),
];
// Foxes have no tempt goal either.
const FOX: &[Entry] = &[
    e(2, G::Panic, 2.2),
    e(3, G::Breed, 1.0),
    avoid(4, Avoided::Player, 16.0, 1.6, 1.4),
    avoid(4, Avoided::Wolf, 8.0, 1.6, 1.4),
    e(8, G::FollowParent, 1.25),
    e(11, G::Stroll, 1.0),
];
const RABBIT: &[Entry] = &[
    e(1, G::Panic, 2.2),
    e(2, G::Breed, 0.8),
    e(3, G::Tempt, 1.0),
    avoid(4, Avoided::Player, 8.0, 2.2, 2.2),
    avoid(4, Avoided::Wolf, 10.0, 2.2, 2.2),
    avoid(4, Avoided::Monster, 4.0, 2.2, 2.2),
    e(6, G::Stroll, 0.6),
];
// `AbstractHorse.registerGoals` + `addBehaviourGoals` (`MountPanicGoal` 1.2).
const HORSE: &[Entry] =
    &[e(1, G::Panic, 1.2), e(2, G::Breed, 1.0), e(3, G::Tempt, 1.25), e(4, G::FollowParent, 1.0), e(6, G::Stroll, 0.7)];
const LLAMA: &[Entry] =
    &[e(3, G::Panic, 1.2), e(4, G::Breed, 1.0), e(5, G::Tempt, 1.25), e(6, G::FollowParent, 1.0), e(7, G::Stroll, 0.7)];
// `TraderLlama` adds `PanicGoal(2.0)` at 1 to the llama's goals.
const TRADER_LLAMA: &[Entry] =
    &[e(1, G::Panic, 2.0), e(4, G::Breed, 1.0), e(5, G::Tempt, 1.25), e(6, G::FollowParent, 1.0), e(7, G::Stroll, 0.7)];
// `Turtle.registerGoals`; turtles have no follow-parent goal. The go-to-
// water goal's speed reads `isBaby` in the constructor, before a hatchling
// is made a baby, so it is always 1.0.
const TURTLE: &[Entry] = &[
    e(0, G::Panic, 1.2),
    e(1, G::Breed, 1.0),
    e(1, G::LayEgg, 1.0),
    e(2, G::Tempt, 1.1),
    e(3, G::GoToWater, 1.0),
    e(4, G::GoHome, 1.0),
    e(7, G::Travel, 1.0),
    e(9, G::Stroll, 1.0),
];
const STRIDER: &[Entry] = &[
    e(1, G::Panic, 1.65),
    e(2, G::Breed, 1.0),
    e(3, G::Tempt, 1.4),
    e(4, G::Lava, 1.0),
    e(5, G::FollowParent, 1.0),
    e(7, G::Stroll, 1.0),
];
const PARROT: &[Entry] = &[e(0, G::Panic, 1.25), owner(2, 5.0, 1.0)];
// Brain animals: core `AnimalPanic`, then the idle activity's
// `AnimalMakeLove`, `FollowTemptation`, `BabyFollowAdult` and stroll.
const GOAT: &[Entry] =
    &[e(0, G::Panic, 2.0), e(1, G::Breed, 1.0), e(2, G::Tempt, 1.25), e(3, G::FollowAdult, 1.25), e(4, G::Stroll, 1.0)];
const CAMEL: &[Entry] =
    &[e(0, G::Panic, 4.0), e(1, G::Breed, 1.0), e(2, G::Tempt, 2.5), e(2, G::FollowAdult, 2.5), e(4, G::Stroll, 2.0)];
// The axolotl's brain has no panic; `AnimalMakeLove` walks at 0.2, and
// `FollowTemptation` and `BabyFollowAdult` at 0.5 and 0.6 in water (see
// `goal_speed` for land).
const AXOLOTL: &[Entry] = &[e(1, G::Breed, 0.2), e(2, G::Tempt, 0.5), e(2, G::FollowAdult, 0.6)];
// `AbstractFish.registerGoals`; `FishSwimGoal` (4) and the schooling fish's
// `FollowFlockLeaderGoal` run in `aquatic::steer`.
const FISH: &[Entry] = &[e(0, G::Panic, 1.25), avoid(2, Avoided::Player, 8.0, 1.6, 1.4), e(4, G::Stroll, 1.0)];
// `Guardian`: its random stroll. Dolphins use the flagged selector in
// `dolphin`.
const GUARDIAN: &[Entry] = &[e(7, G::Stroll, 1.0)];
// Hoglins follow no temptation.
const HOGLIN: &[Entry] = &[e(1, G::Breed, 1.0), e(2, G::FollowAdult, 0.6)];
// `WanderingTrader.registerGoals`: the avoid goals VoxelCraft has mobs for
// (evokers, vindicators, vexes and illusioners aren't registered).
const WANDERING_TRADER: &[Entry] = &[
    avoid(1, Avoided::Zombie, 8.0, 0.5, 0.5),
    avoid(1, Avoided::Pillager, 15.0, 0.5, 0.5),
    avoid(1, Avoided::Zoglin, 10.0, 0.5, 0.5),
];
const CREEPER: &[Entry] = &[avoid(3, Avoided::Cat, 6.0, 1.0, 1.2)];
const SKELETON: &[Entry] = &[avoid(3, Avoided::Wolf, 6.0, 1.0, 1.2)];

/// The movement goals `kind` registers, in registration order.
pub(crate) fn table(kind: MobKind) -> &'static [Entry] {
    use MobKind as K;
    match kind {
        K::Cow => COW,
        K::Pig => PIG,
        K::Sheep => SHEEP,
        K::Chicken => CHICKEN,
        K::Wolf => WOLF,
        K::Cat => CAT,
        K::Fox => FOX,
        K::Rabbit => RABBIT,
        K::Horse | K::Donkey | K::Mule => HORSE,
        K::Llama => LLAMA,
        K::TraderLlama => TRADER_LLAMA,
        K::Turtle => TURTLE,
        K::Strider => STRIDER,
        K::Parrot => PARROT,
        K::Goat => GOAT,
        K::Camel => CAMEL,
        K::Axolotl => AXOLOTL,
        K::Cod | K::Salmon | K::TropicalFish | K::Pufferfish => FISH,
        K::Guardian | K::ElderGuardian => GUARDIAN,
        K::Hoglin => HOGLIN,
        K::Creeper => CREEPER,
        K::Skeleton | K::WitherSkeleton => SKELETON,
        K::WanderingTrader => WANDERING_TRADER,
        _ => &[],
    }
}

/// The `Attributes.MOVEMENT_SPEED` base value of each kind's
/// `createAttributes` (horses roll their own; see `mounts::State::speed`).
pub(crate) fn movement_speed(kind: MobKind) -> f64 {
    use MobKind as K;
    match kind {
        K::Cow | K::Parrot | K::Goat => 0.2,
        K::Pig | K::Chicken | K::Turtle | K::Creeper | K::Skeleton | K::WitherSkeleton => 0.25,
        K::Sheep | K::Zombie => 0.23,
        K::Wolf | K::Cat | K::Fox | K::Rabbit | K::Hoglin => 0.3,
        K::Strider | K::Donkey | K::Mule | K::Llama | K::TraderLlama => 0.175,
        K::Camel => 0.09,
        K::Axolotl => 1.0,
        K::Dolphin => 1.2,
        K::Guardian => 0.5,
        K::ElderGuardian => 0.3,
        // `Mob.createMobAttributes` leaves the 0.7 default.
        K::WanderingTrader | K::Cod | K::Salmon | K::TropicalFish | K::Pufferfish | K::Squid | K::GlowSquid => 0.7,
        _ => 0.25,
    }
}

/// The items each `TemptGoal`/`FollowTemptation` follows.
pub(crate) fn tempted_by(kind: MobKind, item: Item) -> bool {
    use MobKind as K;
    match kind {
        K::Pig => item == Item::CARROT_ON_A_STICK || kind.breeding_food(item),
        K::Strider => item == Item::WARPED_FUNGUS_ON_A_STICK || kind.breeding_food(item),
        // `HORSE_TEMPT_ITEMS`, which mules follow too.
        K::Horse | K::Donkey | K::Mule => matches!(item, Item::GOLDEN_CARROT | Item::GOLDEN_APPLE),
        K::TraderLlama => K::Llama.breeding_food(item),
        K::Wolf | K::Fox | K::Parrot | K::Hoglin => false,
        _ => kind.breeding_food(item),
    }
}

/// Monsters Java's `Monster` class covers (not slimes, ghasts or hoglins).
fn is_monster(kind: MobKind) -> bool {
    kind.is_hostile() && !matches!(kind, MobKind::Slime | MobKind::MagmaCube | MobKind::Ghast | MobKind::Hoglin)
}

/// Whether a mob of `kind` (`tame` for wolves, `strength` for llamas) is
/// what `what` describes, before the goal's own predicate.
pub(crate) fn avoided_kind(what: Avoided, kind: MobKind) -> bool {
    match what {
        Avoided::Player => false,
        Avoided::Wolf => kind == MobKind::Wolf,
        Avoided::Llama => matches!(kind, MobKind::Llama | MobKind::TraderLlama),
        Avoided::Cat => kind == MobKind::Cat,
        Avoided::Monster => is_monster(kind),
        Avoided::Zombie => kind.is_zombie() || kind == MobKind::ZombifiedPiglin,
        Avoided::Pillager => kind == MobKind::Pillager,
        Avoided::Zoglin => kind == MobKind::Zoglin,
    }
}

/// The entity an avoid goal flees: a player or a mob uid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Foe {
    Player(PlayerId),
    Mob(u32),
}

/// Transient goal-selector state (never saved, as in Java).
#[derive(Clone, Debug, Default)]
pub(crate) struct Goals {
    /// `Mob.tickCount`.
    pub ticks: u32,
    /// Index into [`table`] of the running goal, unless panic or stroll.
    pub running: Option<u8>,
    /// `timeToRecalcPath` of the follow goals.
    pub recalc: i32,
    /// `TemptGoal.calmDown`.
    pub calm_down: i32,
    pub tempt_player: Option<PlayerId>,
    /// The follow goals' adult: chosen when the goal starts, then tracked by
    /// uid. The entity pass refreshes its position, or clears it once gone.
    pub parent: Option<(u32, DVec3)>,
    /// The nearest adult the entity pass saw this tick (the `canUse` search).
    pub adult: Option<(u32, DVec3)>,
    /// Per avoid entry, the nearest visible mob the entity pass found.
    pub seen: [Option<(u32, DVec3)>; 3],
    /// The running avoid goal's entity and its latest position.
    pub avoiding: Option<(Foe, DVec3)>,
    /// `FollowOwnerGoal.start` zeroes the water malus until `stop`.
    pub water_free: bool,
    /// `MoveTowardsRestrictionGoal` is running (guardians).
    pub restricting: bool,
}

impl Goals {
    /// Whether this tick is the selector's full update.
    pub(crate) fn full(&self, uid: u32) -> bool {
        self.ticks.wrapping_add(uid).is_multiple_of(2) || self.ticks <= 1
    }
}

impl Mob {
    /// Whether `BreedGoal` is running.
    pub(crate) fn breeding(&self) -> bool {
        self.goals.running.is_some_and(|i| matches!(table(self.kind)[i as usize].goal, G::Breed))
    }

    /// Movement speed attribute (`mounts::State::speed` for horses).
    fn attribute_speed(&self) -> f64 {
        match &self.mount {
            Some(s) if super::super::mounts::equine(self.kind) && s.speed > 0.0 => s.speed,
            _ => movement_speed(self.kind),
        }
    }

    /// Ground speed (blocks per second) for a goal speed modifier: the
    /// speed `MoveControl` would set (movement speed times the modifier) as
    /// steady ground travel. Swimmers convert it back for their own move
    /// controls (`mob::swim`). On land, an axolotl's brain asks 0.15 (except
    /// `AnimalMakeLove`'s 0.2) and `SmoothSwimmingMoveControl` halves it, and
    /// `TurtleMoveControl` holds a turtle near 0.06 eased an eighth of the
    /// way toward its speed.
    pub(crate) fn goal_speed(&self, modifier: f64) -> f64 {
        let raw = self.attribute_speed() * modifier;
        let raw = match self.kind {
            MobKind::Axolotl if !self.in_water => {
                self.attribute_speed() * if modifier <= 0.2 { modifier } else { 0.15 } * 0.5
            }
            MobKind::Turtle if !self.in_water => 0.875 * 0.06 + 0.125 * raw,
            _ => raw,
        };
        path::ground_speed(raw)
    }

    /// The table's speed for `goal`, as a ground speed.
    pub(crate) fn table_speed(&self, goal: fn(&Goal) -> bool) -> Option<f64> {
        table(self.kind).iter().find(|e| goal(&e.goal)).map(|e| self.goal_speed(e.speed))
    }

    fn priority_of_running(&self, t: &[Entry]) -> Option<u8> {
        if let Some(i) = self.goals.running {
            return Some(t[i as usize].priority);
        }
        let find = |g: fn(&Goal) -> bool| t.iter().find(|e| g(&e.goal)).map(|e| e.priority);
        if self.ai == Ai::Panic {
            return find(|g| matches!(g, G::Panic));
        }
        if self.ai == Ai::Wander && self.path_move {
            return find(|g| matches!(g, G::Stroll));
        }
        None
    }

    /// Run the selector. `Some` is this tick's movement from a running table
    /// goal; `None` leaves movement to panic, strolling and the mob's other
    /// behaviour.
    pub(crate) fn run_goals<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
    ) -> Option<(Option<DVec3>, f64)> {
        let t = table(self.kind);
        if t.is_empty() {
            return None;
        }
        let full = self.goals.full(self.uid);
        self.goals.ticks = self.goals.ticks.wrapping_add(1);
        // A creeper swelling (`SwellGoal`, priority 2) holds off its avoid goal.
        if self.kind == MobKind::Creeper
            && (self.fuse > 0.0
                || ctx.nearest_target(self.pos).is_some_and(|p| p.targetable && p.pos.distance_squared(self.pos) < 9.0))
        {
            self.stop_goal(world);
            return None;
        }
        // Damage starts panic directly; it takes over any goal it outranks.
        if self.ai == Ai::Panic
            && let Some(i) = self.goals.running
        {
            let panic = t.iter().find(|e| matches!(e.goal, G::Panic)).map(|e| e.priority);
            if panic.is_some_and(|p| p <= t[i as usize].priority) {
                self.stop_goal(world);
            } else {
                self.ai = Ai::Idle;
            }
        }
        if full {
            if let Some(i) = self.goals.running
                && !self.can_continue(world, ctx, t[i as usize])
            {
                self.stop_goal(world);
            }
            for (i, entry) in t.iter().enumerate() {
                if self.goals.running == Some(i as u8)
                    || matches!(entry.goal, G::Panic | G::Stroll | G::GoHome | G::Travel)
                    || self.priority_of_running(t).is_some_and(|p| entry.priority >= p)
                {
                    continue;
                }
                if entry.goal == G::Lava {
                    // The lava goal keeps its own `canUse` state; it runs
                    // while it has a target.
                    if self.go_to_lava_can_use(world, rng) {
                        self.stop_goal(world);
                        self.stop_stroll();
                        self.goals.running = Some(i as u8);
                    }
                    continue;
                }
                if self.can_use(world, ctx, rng, *entry) {
                    self.stop_goal(world);
                    self.stop_stroll();
                    self.goals.running = Some(i as u8);
                    self.start_goal(world, rng, *entry);
                }
            }
        }
        // The sheep's `EatBlockGoal` (5) holds it still while it eats; panic
        // and the goals that outrank it end the meal (`stop` resets the timer).
        if self.animal.as_ref().is_some_and(|a| a.grazing > 0.0) {
            if self.ai == Ai::Panic || self.goals.running.is_some() {
                self.animal.as_mut().unwrap().grazing = 0.0;
            } else {
                self.stop_stroll();
                self.nav.stop();
                return Some((None, 0.0));
            }
        }
        if self.kind == MobKind::Turtle {
            self.turtle_panic_plan(world, ctx, rng);
            if full {
                self.turtle_side_goals(world, ctx, rng);
            }
        }
        let i = self.goals.running?;
        let entry = t[i as usize];
        if entry.goal == G::Lava {
            return match self.go_to_lava_tick(world) {
                Some(wish) => Some(wish),
                None => {
                    self.goals.running = None;
                    None
                }
            };
        }
        // `MoveToBlockGoal.requiresUpdateEveryTick`.
        if full || matches!(entry.goal, G::LayEgg | G::GoToWater) {
            self.tick_goal(world, ctx, rng, entry);
        }
        let speed = match entry.goal {
            // Turtles follow whichever goal last planned, flagless ones too.
            _ if self.kind == MobKind::Turtle && !self.nav.is_done() => self.nav.speed,
            G::FollowOwner { .. } if self.kind == MobKind::Parrot => 2.5,
            G::Avoid(a) => {
                self.goal_speed(if self.goals.avoiding.is_some_and(|(_, at)| at.distance_squared(self.pos) < 49.0) {
                    a.sprint
                } else {
                    a.walk
                })
            }
            _ => self.goal_speed(entry.speed),
        };
        Some((self.follow_path(world, speed, false), speed))
    }

    fn stop_stroll(&mut self) {
        if self.ai == Ai::Wander && self.path_move {
            self.ai = Ai::Idle;
            self.ai_timer = 0.0;
            self.path_move = false;
            self.nav.stop();
        }
    }

    /// `Goal.stop` for the running table goal.
    pub(crate) fn stop_goal<W: MobWorld + ?Sized>(&mut self, _world: &W) {
        let Some(i) = self.goals.running.take() else { return };
        match table(self.kind)[i as usize].goal {
            G::Breed => {
                if let Some(a) = &mut self.animal {
                    a.courtship = 0.0;
                    a.breed_now = None;
                }
            }
            G::Tempt => {
                self.goals.tempt_player = None;
                self.nav.stop();
                // `reducedTickDelay(100)`.
                self.goals.calm_down = 50;
            }
            G::FollowParent | G::FollowAdult => self.goals.parent = None,
            G::FollowOwner { .. } => {
                self.nav.stop();
                self.goals.water_free = false;
            }
            G::Avoid(_) => self.goals.avoiding = None,
            G::Lava => {
                if let Some(n) = &mut self.nether {
                    n.lava_goal = None;
                }
                self.nav.stop();
            }
            G::LayEgg | G::GoToWater | G::GoHome | G::Travel | G::Panic | G::Stroll => {}
        }
    }

    fn owner_target<'a>(&self, ctx: &'a Ctx) -> Option<&'a super::super::Target> {
        let owner = self.animal.as_ref()?.owner?;
        ctx.players.iter().find(|t| t.id == owner && t.alive && !t.spectator)
    }

    /// `TamableAnimal.unableToMoveToOwner`: sitting, riding or leashed.
    fn unable_to_move_to_owner(&self) -> bool {
        self.animal.as_ref().is_some_and(|a| a.sitting) || self.riding.is_some() || self.leash_pos.is_some()
    }

    /// `TemptGoal.canUse` (also its `canContinueToUse`): the nearest player
    /// within `TEMPT_RANGE` (10) holding a tempting item.
    fn find_tempter(&mut self, ctx: &Ctx) -> bool {
        if self.goals.calm_down > 0 {
            self.goals.calm_down -= 1;
            return false;
        }
        if self.kind == MobKind::Cat && self.animal.as_ref().is_some_and(|a| a.owner.is_some()) {
            return false;
        }
        let kind = self.kind;
        self.goals.tempt_player = ctx
            .players
            .iter()
            .filter(|t| {
                t.alive
                    && !t.spectator
                    && t.pos.distance_squared(self.pos) <= 100.0
                    && t.held_item.is_some_and(|i| tempted_by(kind, i))
            })
            .min_by(|a, b| a.pos.distance_squared(self.pos).total_cmp(&b.pos.distance_squared(self.pos)))
            .map(|t| t.id);
        self.goals.tempt_player.is_some()
    }

    fn can_use<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx, rng: &mut Rng, entry: Entry) -> bool {
        match entry.goal {
            G::Breed => {
                self.animal.as_ref().is_some_and(|a| a.love > 0.0 && a.partner.is_some()) && self.ready_to_breed()
            }
            G::Tempt => self.find_tempter(ctx),
            G::FollowParent => {
                self.age < 0 && self.goals.adult.is_some_and(|(_, at)| at.distance_squared(self.pos) >= 9.0)
            }
            G::FollowAdult => {
                self.age < 0
                    && self.goals.adult.is_some_and(|(_, at)| {
                        let d = at.distance_squared(self.pos);
                        (25.0..289.0).contains(&d)
                    })
            }
            G::FollowOwner { start, .. } => {
                !self.unable_to_move_to_owner()
                    && self.owner_target(ctx).is_some_and(|t| t.pos.distance_squared(self.pos) >= start * start)
            }
            G::Avoid(a) => self.avoid_can_use(world, ctx, rng, entry, a),
            G::LayEgg => self.lay_egg_can_use(world, rng),
            G::GoToWater => self.go_to_water_can_use(world, rng),
            G::Lava | G::Panic | G::Stroll | G::GoHome | G::Travel => false,
        }
    }

    fn can_continue<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx, entry: Entry) -> bool {
        match entry.goal {
            // Partner alive, in love and not panicking (the entity pass keeps
            // `partner` only while that holds), and `loveTime < 60`.
            G::Breed => self.animal.as_ref().is_some_and(|a| a.partner.is_some() && a.courtship < 6.0 - 1e-4),
            G::Tempt => self.find_tempter(ctx),
            G::FollowParent => {
                self.age < 0
                    && self.goals.parent.is_some_and(|(_, at)| {
                        let d = at.distance_squared(self.pos);
                        (9.0..=256.0).contains(&d)
                    })
            }
            G::FollowAdult => {
                self.age < 0
                    && self.goals.parent.is_some_and(|(_, at)| {
                        let d = at.distance_squared(self.pos);
                        (25.0..289.0).contains(&d)
                    })
            }
            G::FollowOwner { stop, .. } => {
                !self.nav.is_done()
                    && !self.unable_to_move_to_owner()
                    && self.owner_target(ctx).is_some_and(|t| t.pos.distance_squared(self.pos) > stop * stop)
            }
            G::Avoid(_) => !self.nav.is_done() && self.goals.avoiding.is_some(),
            G::LayEgg => self.move_to_block_can_continue(world, true),
            G::GoToWater => self.move_to_block_can_continue(world, false),
            // Its tick checks Strider's `canContinueToUse` every tick.
            G::Lava => self.nether.as_ref().is_some_and(|n| n.lava_goal.is_some()),
            G::Panic | G::Stroll | G::GoHome | G::Travel => false,
        }
    }

    fn start_goal<W: MobWorld + ?Sized>(&mut self, world: &W, rng: &mut Rng, entry: Entry) {
        match entry.goal {
            G::LayEgg | G::GoToWater => {
                let speed = self.goal_speed(entry.speed);
                self.move_to_block_start(world, rng, entry.goal == G::LayEgg, speed);
            }
            G::FollowParent | G::FollowAdult => {
                self.goals.parent = self.goals.adult;
                self.goals.recalc = 0;
            }
            G::FollowOwner { .. } => {
                self.goals.recalc = 0;
                self.goals.water_free = true;
            }
            G::Breed => {
                if let Some(a) = &mut self.animal {
                    a.courtship = 0.0;
                }
            }
            _ => {}
        }
    }

    /// `Goal.tick` on the selector's full update.
    fn tick_goal<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx, rng: &mut Rng, entry: Entry) {
        let speed = self.goal_speed(entry.speed);
        match entry.goal {
            G::Breed => {
                let Some((uid, at)) = self.animal.as_ref().and_then(|a| a.partner) else { return };
                self.look_at(at, 1.2);
                self.walk_to(world, at.floor().as_ivec3(), 1, speed, false);
                let a = self.animal.as_mut().unwrap();
                a.courtship += 0.1;
                // `loveTime >= adjustedTickDelay(60)` and within 3 blocks.
                if a.courtship >= super::super::animals::COURTSHIP_SECONDS - 1e-4 && at.distance_squared(self.pos) < 9.0
                {
                    a.breed_now = Some(uid);
                }
            }
            G::Tempt => {
                let Some(t) = self.goals.tempt_player.and_then(|id| ctx.players.iter().find(|t| t.id == id)) else {
                    return;
                };
                let at = t.pos;
                self.look_at(at, 1.2);
                if at.distance_squared(self.pos) < 2.5 * 2.5 {
                    self.nav.stop();
                } else {
                    self.walk_to(world, at.floor().as_ivec3(), 1, speed, false);
                }
            }
            G::FollowParent => {
                let Some((_, at)) = self.goals.parent else { return };
                self.goals.recalc -= 1;
                if self.goals.recalc <= 0 {
                    // `adjustedTickDelay(10)`.
                    self.goals.recalc = 5;
                    self.walk_to(world, at.floor().as_ivec3(), 1, speed, false);
                }
            }
            G::FollowAdult => {
                // The walk target tracks the adult; close enough within 4.
                let Some((_, at)) = self.goals.parent else { return };
                self.walk_to(world, at.floor().as_ivec3(), 4, speed, false);
            }
            G::FollowOwner { .. } => {
                let Some(t) = self.owner_target(ctx) else { return };
                let at = t.pos;
                let teleport = at.distance_squared(self.pos) >= 144.0;
                if !teleport {
                    self.look_at(at, 1.2);
                }
                self.goals.recalc -= 1;
                if self.goals.recalc <= 0 {
                    self.goals.recalc = 5;
                    if teleport {
                        self.teleport_to_owner(world, at, rng);
                    } else {
                        let speed = if self.kind == MobKind::Parrot { 2.5 } else { speed };
                        let goal = if self.kind == MobKind::Parrot { at + DVec3::Y * 1.5 } else { at };
                        self.walk_to(world, goal.floor().as_ivec3(), 1, speed, false);
                    }
                }
            }
            G::Avoid(a) => {
                // `setSpeedModifier`: sprint inside 7 blocks.
                let close = self.goals.avoiding.is_some_and(|(_, at)| at.distance_squared(self.pos) < 49.0);
                self.nav.speed = self.goal_speed(if close { a.sprint } else { a.walk });
            }
            G::LayEgg => self.move_to_block_tick(world, rng, true, speed),
            G::GoToWater => self.move_to_block_tick(world, rng, false, speed),
            G::Lava | G::Panic | G::Stroll | G::GoHome | G::Travel => {}
        }
    }

    /// `TamableAnimal.teleportToAroundBlockPos`: ten tries 2–3 blocks out
    /// (±1 vertically) onto a walkable cell, not leaves, with room for the box.
    fn teleport_to_owner<W: MobWorld + ?Sized>(&mut self, w: &W, owner: DVec3, rng: &mut Rng) {
        let base = owner.floor().as_ivec3();
        for _ in 0..10 {
            let dx = rng.next_int(7) as i32 - 3;
            let dz = rng.next_int(7) as i32 - 3;
            if dx.abs() < 2 && dz.abs() < 2 {
                continue;
            }
            let p = base + IVec3::new(dx, rng.next_int(3) as i32 - 1, dz);
            let at = p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
            if w.loaded(p)
                && path::static_type(w, p) == PathType::Walkable
                && (self.kind == MobKind::Parrot || !w.block(p - IVec3::Y).is_some_and(|b| b.is_leaves()))
                && !crate::physics::overlaps_solid(w, at, self.shape())
            {
                self.pos = at;
                self.previous_pos = at;
                self.vel = DVec3::ZERO;
                self.nav.stop();
                return;
            }
        }
    }

    /// `AvoidEntityGoal.canUse`: the nearest visible entity to flee within
    /// `maxDist` (and 3 blocks vertically), a spot up to 16 blocks away from
    /// it (`DefaultRandomPos.getPosAway(16, 7)`) farther from it than the mob
    /// is, and a path there.
    fn avoid_can_use<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        entry: Entry,
        a: Avoid,
    ) -> bool {
        if !self.avoid_allowed(a) {
            return false;
        }
        let foe = match a.what {
            Avoided::Player => {
                let eye = self.pos + DVec3::Y * (self.shape().height * 0.85);
                let trusted = self.animal.as_ref().map(|s| s.trusted).unwrap_or_default();
                ctx.players
                    .iter()
                    .filter(|t| {
                        let d = t.pos - self.pos;
                        // `forCombat`: survival players only.
                        t.targetable
                            && t.alive
                            && !trusted.contains(&Some(t.id))
                            && d.x.abs() <= a.dist + self.shape().half_width
                            && d.z.abs() <= a.dist + self.shape().half_width
                            && d.y.abs() <= 3.0 + self.shape().height
                            && d.length_squared() <= a.dist * a.dist
                            && line_of_sight(world, eye, t.pos + DVec3::Y * 1.62)
                    })
                    .min_by(|l, r| l.pos.distance_squared(self.pos).total_cmp(&r.pos.distance_squared(self.pos)))
                    .map(|t| (Foe::Player(t.id), t.pos))
            }
            _ => {
                let slot = table(self.kind)
                    .iter()
                    .filter(|e| matches!(e.goal, G::Avoid(_)))
                    .position(|e| e.goal == entry.goal)
                    .unwrap_or(0);
                self.goals.seen.get(slot).copied().flatten().map(|(uid, pos)| (Foe::Mob(uid), pos))
            }
        };
        let Some((foe, from)) = foe else { return false };
        let Some(away) = self.pos_away(world, ctx, rng, 16, 7, from) else { return false };
        let target = away.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
        if from.distance_squared(target) < from.distance_squared(self.pos) {
            return false;
        }
        let speed = self.goal_speed(a.walk);
        let caps = self.caps(false);
        let mover = self.mover(speed);
        let Some(path) = self.nav.create_path(world, &caps, &mover, away, 0) else { return false };
        self.nav.move_to(world, Some(path), &mover);
        self.goals.avoiding = Some((foe, from));
        true
    }

    /// Goal-specific `canUse` conditions on top of `AvoidEntityGoal`'s.
    fn avoid_allowed(&self, a: Avoid) -> bool {
        let tame = self.animal.as_ref().is_some_and(|s| s.owner.is_some());
        match (self.kind, a.what) {
            // `CatAvoidEntityGoal` and `WolfAvoidEntityGoal`: untamed only.
            (MobKind::Cat | MobKind::Wolf, _) => !tame,
            // Foxes asleep don't flee.
            (MobKind::Fox, _) => !self.animal.as_ref().is_some_and(|s| s.sleeping),
            // `TradeWithPlayerGoal` shares priority 1 and was registered first.
            (MobKind::WanderingTrader, _) => !self.villager.as_ref().is_some_and(|v| v.trading || v.sleeping),
            _ => true,
        }
    }
}

impl crate::entity::Entities {
    /// The goal selector's entity searches, before mobs think: the adult a
    /// baby follows (`FollowParentGoal`: nearest adult of its kind within its
    /// box grown 8 across and 4 up and down; brain `BabyFollowAdult`: within
    /// 16) and the mobs each avoid goal flees (`AvoidEntityGoal`: nearest
    /// visible within `maxDist`, its box grown `maxDist` across and 3 up and
    /// down). Searches run only on a mob's full selector update; running
    /// goals only refresh their entity's position.
    pub(crate) fn sense_goals<W: MobWorld + ?Sized>(&mut self, world: &W) {
        self.mob_index.rebuild(&self.mobs);
        // Followed and avoided mobs are tracked by uid; one map per tick keeps
        // each lookup constant instead of a scan of every mob.
        let by_uid: FxHashMap<u32, usize> =
            self.mobs.iter().enumerate().filter(|(_, b)| b.alive()).map(|(j, b)| (b.uid, j)).collect();
        for i in 0..self.mobs.len() {
            let m = &self.mobs[i];
            let t = table(m.kind);
            if t.is_empty() || !m.simulation_active || !m.alive() {
                continue;
            }
            let full = m.goals.full(m.uid);
            let find = |uid: u32| by_uid.get(&uid).map(|&j| self.mobs[j].pos);
            let parent = m.goals.parent.and_then(|(uid, _)| find(uid).map(|p| (uid, p)));
            let follows = t.iter().find(|e| matches!(e.goal, G::FollowParent | G::FollowAdult)).map(|e| e.goal);
            let adult = match follows {
                Some(goal) if full && m.age < 0 => {
                    let shape = m.shape();
                    let mut best: Option<(f64, usize)> = None;
                    self.mob_index.visit(m.pos, 20.0, |j| {
                        let b = &self.mobs[j];
                        if b.kind != m.kind || b.age < 0 || !b.alive() || j == i {
                            return;
                        }
                        let d = b.pos - m.pos;
                        let reach = if goal == G::FollowParent {
                            let across = 8.0 + shape.half_width + b.shape().half_width;
                            d.x.abs() < across
                                && d.z.abs() < across
                                && b.pos.y < m.pos.y + shape.height + 4.0
                                && b.pos.y + b.shape().height > m.pos.y - 4.0
                        } else {
                            d.length_squared() <= 256.0
                        };
                        let d2 = d.length_squared();
                        if reach && best.is_none_or(|(bd, bj)| d2 < bd || d2 == bd && j < bj) {
                            best = Some((d2, j));
                        }
                    });
                    best.map(|(_, j)| (self.mobs[j].uid, self.mobs[j].pos))
                }
                _ => None,
            };
            let mut seen = [None; 3];
            let avoiding = match m.goals.avoiding {
                Some((Foe::Mob(uid), at)) => Some((Foe::Mob(uid), find(uid).unwrap_or(at))),
                other => other,
            };
            if full && avoiding.is_none() {
                let eye = m.pos + DVec3::Y * (m.shape().height * 0.85);
                let tame = |b: &Mob| b.animal.as_ref().is_some_and(|a| a.owner.is_some());
                for (slot, entry) in t.iter().filter(|e| matches!(e.goal, G::Avoid(_))).take(3).enumerate() {
                    let G::Avoid(a) = entry.goal else { continue };
                    if a.what == Avoided::Player {
                        continue;
                    }
                    let mut best: Option<(f64, usize)> = None;
                    self.mob_index.visit(m.pos, a.dist + 3.0, |j| {
                        let b = &self.mobs[j];
                        if j == i || !b.alive() || !avoided_kind(a.what, b.kind) {
                            return;
                        }
                        let d = b.pos - m.pos;
                        let across = a.dist + m.shape().half_width + b.shape().half_width;
                        let d2 = d.length_squared();
                        let wild_wolf = !(m.kind == MobKind::Fox && a.what == Avoided::Wolf && tame(b));
                        if d.x.abs() < across
                            && d.z.abs() < across
                            && b.pos.y < m.pos.y + m.shape().height + 3.0
                            && b.pos.y + b.shape().height > m.pos.y - 3.0
                            && d2 <= a.dist * a.dist
                            && wild_wolf
                            && best.is_none_or(|(bd, bj)| d2 < bd || d2 == bd && j < bj)
                        {
                            best = Some((d2, j));
                        }
                    });
                    seen[slot] = best
                        .filter(|&(_, j)| {
                            let b = &self.mobs[j];
                            line_of_sight(world, eye, b.pos + DVec3::Y * (b.shape().height * 0.85))
                        })
                        .map(|(_, j)| (self.mobs[j].uid, self.mobs[j].pos));
                }
            }
            // `WolfAvoidEntityGoal.avoidLlama`: `strength >= nextInt(5)`.
            let llama_strength = (m.kind == MobKind::Wolf)
                .then(|| seen[0].and_then(|(uid, _)| by_uid.get(&uid).map(|&j| &self.mobs[j])))
                .flatten()
                .map(|l| l.mount.as_ref().map_or(1, |s| s.strength) as u32);
            if let Some(strength) = llama_strength
                && strength < self.rng.next_int(5)
            {
                seen[0] = None;
            }
            let g = &mut self.mobs[i].goals;
            g.parent = parent;
            g.adult = adult;
            g.seen = seen;
            g.avoiding = avoiding;
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::DVec3;

    use super::{G, table};
    use crate::entity::mob::Ai;
    use crate::entity::{Ctx, Entities, MobKind, PlayerId, Target};
    use crate::item::Item;
    use crate::physics::test_util::Grid;
    use crate::world::terrain::Dimension;

    fn ctx(players: Vec<Target>) -> Ctx {
        Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::DAY,
            players,
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
        }
    }
    fn player(id: u32, pos: DVec3, held: Option<Item>) -> Target {
        let mut t = Target::new(PlayerId(id), pos, true);
        t.held_item = held;
        t
    }
    fn spawn(e: &mut Entities, kind: MobKind, pos: DVec3) -> usize {
        e.spawn(kind, pos);
        e.ensure_mob_ids();
        e.mobs.len() - 1
    }
    fn running(e: &Entities, i: usize) -> Option<G> {
        let m = &e.mobs[i];
        m.goals.running.map(|r| table(m.kind)[r as usize].goal)
    }
    fn flat(a: DVec3, b: DVec3) -> f64 {
        (a - b).with_y(0.0).length()
    }

    /// Every animal table follows its 26.3 `registerGoals` order: panic
    /// first, breeding before temptation, temptation before following a
    /// parent, and strolling last.
    #[test]
    fn tables_keep_javas_priority_order() {
        let order =
            |kind: MobKind, goal: fn(&G) -> bool| table(kind).iter().find(|e| goal(&e.goal)).map(|e| e.priority);
        for kind in MobKind::ALL {
            let t = table(kind);
            assert!(t.windows(2).all(|w| w[0].priority <= w[1].priority), "{kind:?} registers in priority order");
            let (panic, breed, tempt, parent, stroll) = (
                order(kind, |g| matches!(g, G::Panic)),
                order(kind, |g| matches!(g, G::Breed)),
                order(kind, |g| matches!(g, G::Tempt)),
                order(kind, |g| matches!(g, G::FollowParent | G::FollowAdult)),
                order(kind, |g| matches!(g, G::Stroll)),
            );
            for (hi, lo) in [(panic, breed), (panic, tempt), (tempt, parent), (parent, stroll), (breed, parent)] {
                if let (Some(hi), Some(lo)) = (hi, lo) {
                    assert!(hi <= lo, "{kind:?}: {hi} before {lo}");
                }
            }
        }
        // Farm animals breed before following food; cats (tempt 4, breed 10) don't.
        for kind in [MobKind::Cow, MobKind::Pig, MobKind::Sheep, MobKind::Chicken, MobKind::Horse, MobKind::Strider] {
            let (breed, tempt) = (order(kind, |g| matches!(g, G::Breed)), order(kind, |g| matches!(g, G::Tempt)));
            assert!(breed < tempt, "{kind:?}");
        }
        assert_eq!(order(MobKind::Cat, |g| matches!(g, G::Tempt)), Some(4));
        assert_eq!(order(MobKind::Cat, |g| matches!(g, G::Breed)), Some(10));
        // The strider's lava goal (4) outranks following its parent (5).
        assert_eq!(order(MobKind::Strider, |g| matches!(g, G::Lava)), Some(4));
        assert_eq!(order(MobKind::Strider, |g| matches!(g, G::FollowParent)), Some(5));
        // Wolves follow their owner (6) before breeding (7) and have no tempt goal.
        assert_eq!(order(MobKind::Wolf, |g| matches!(g, G::FollowOwner { .. })), Some(6));
        assert_eq!(order(MobKind::Wolf, |g| matches!(g, G::Breed)), Some(7));
        assert_eq!(order(MobKind::Wolf, |g| matches!(g, G::Tempt)), None);
    }

    /// `PanicGoal` (1) outranks `TemptGoal` (3): a hurt cow runs at its
    /// panic speed instead of following the wheat.
    #[test]
    fn panic_outranks_temptation() {
        let w = Grid::flat(0);
        let c = ctx(vec![player(1, DVec3::new(6.5, 0.0, 0.5), Some(Item::WHEAT))]);
        let mut e = Entities::new(3);
        let i = spawn(&mut e, MobKind::Cow, DVec3::new(0.5, 0.0, 0.5));
        for _ in 0..20 {
            e.update(0.05, &w, &c);
        }
        assert_eq!(running(&e, i), Some(G::Tempt));
        let mut rng = crate::entity::Rng::new(1);
        e.mobs[i].damage(1.0, None, &mut rng);
        let mut fastest = 0.0f64;
        for _ in 0..20 {
            let before = e.mobs[i].pos;
            e.update(0.05, &w, &c);
            fastest = fastest.max(flat(e.mobs[i].pos, before) * 20.0);
            assert_ne!(running(&e, i), Some(G::Tempt));
        }
        assert_eq!(e.mobs[i].ai, Ai::Panic);
        // `PanicGoal(2.0)` on 0.2 movement speed: 0.4² / 0.454 blocks a tick.
        assert!(fastest > 5.0, "panic speed, not tempt speed: {fastest}");
    }

    /// `getLastDamageSource` forgets the hit after 40 ticks, and the panic
    /// run ends with its current path.
    /// Aquatic mobs steer without a panic run, but they still forget the hit
    /// after 40 ticks: a hurt axolotl follows the tropical fish bucket again.
    #[test]
    fn a_hurt_axolotl_never_panics_and_stays_tempted() {
        let w = Grid::flat(0);
        let c = ctx(vec![player(1, DVec3::new(5.5, 0.0, 0.5), Some(Item::TROPICAL_FISH_BUCKET))]);
        let mut e = Entities::new(5);
        let i = spawn(&mut e, MobKind::Axolotl, DVec3::new(0.5, 0.0, 0.5));
        let mut rng = crate::entity::Rng::new(2);
        // `AxolotlAi` has no panic behaviour: a hit doesn't scare it.
        e.mobs[i].damage(1.0, None, &mut rng);
        assert_eq!(e.mobs[i].ai, Ai::Idle);
        for _ in 0..60 {
            e.update(0.05, &w, &c);
        }
        assert_ne!(e.mobs[i].ai, Ai::Panic);
        assert_eq!(running(&e, i), Some(G::Tempt));
        // Playing dead outranks temptation; the timer and healing still run.
        e.mobs[i].health = 5.0;
        e.mobs[i].aquatic.as_mut().unwrap().play_dead = 1.0;
        for _ in 0..10 {
            e.update(0.05, &w, &c);
        }
        assert_eq!(running(&e, i), None, "no goal while playing dead");
        let a = e.mobs[i].aquatic.as_ref().unwrap();
        assert!(a.play_dead < 0.6 && e.mobs[i].health > 5.0, "{} {}", a.play_dead, e.mobs[i].health);
        // Stopping temptation starts its 100-tick `calmDown`.
        for _ in 0..20 {
            e.update(0.05, &w, &c);
        }
        assert_eq!(running(&e, i), None, "calming down after playing dead");
        for _ in 0..100 {
            e.update(0.05, &w, &c);
        }
        assert_eq!(running(&e, i), Some(G::Tempt), "tempted again once it has calmed down");
    }

    /// The sheep's `EatBlockGoal` (5) holds it still while it eats, and
    /// panic (1) ends the meal.
    #[test]
    fn grazing_sheep_stand_still_until_panic_ends_the_meal() {
        let w = Grid::flat(0);
        let c = ctx(vec![player(1, DVec3::new(40.5, 0.0, 0.5), None)]);
        let mut e = Entities::new(5);
        let i = spawn(&mut e, MobKind::Sheep, DVec3::new(0.5, 0.0, 0.5));
        let mut rng = crate::entity::Rng::new(3);
        e.mobs[i].animal.as_mut().unwrap().grazing = 100.0;
        let start = e.mobs[i].pos;
        for _ in 0..200 {
            e.update(0.05, &w, &c);
        }
        assert!(flat(e.mobs[i].pos, start) < 1e-6, "moved {} while eating", flat(e.mobs[i].pos, start));
        e.mobs[i].damage(1.0, None, &mut rng);
        for _ in 0..20 {
            e.update(0.05, &w, &c);
        }
        assert_eq!(e.mobs[i].animal.as_ref().unwrap().grazing, 0.0, "panic stops the meal");
        assert!(flat(e.mobs[i].pos, start) > 0.5, "and the sheep runs");
    }

    #[test]
    fn panic_lasts_two_seconds_then_finishes_its_path() {
        let w = Grid::flat(0);
        let c = ctx(vec![player(1, DVec3::new(40.5, 0.0, 0.5), None)]);
        let mut e = Entities::new(5);
        let i = spawn(&mut e, MobKind::Sheep, DVec3::new(0.5, 0.0, 0.5));
        for _ in 0..10 {
            e.update(0.05, &w, &c);
        }
        let mut rng = crate::entity::Rng::new(2);
        e.mobs[i].damage(1.0, None, &mut rng);
        let mut ticks = 0;
        while e.mobs[i].ai == Ai::Panic && ticks < 200 {
            e.update(0.05, &w, &c);
            ticks += 1;
        }
        assert!((40..100).contains(&ticks), "panicked for {ticks} ticks");
        // Penned in, with nowhere to run to, it stops as soon as it forgets.
        let mut pen = Grid::flat(0);
        for x in -1..=1 {
            for z in -1..=1 {
                if (x, z) != (0, 0) {
                    for y in 0..3 {
                        pen.set(glam::IVec3::new(x, y, z), crate::world::block::Block::STONE);
                    }
                }
            }
        }
        let mut e = Entities::new(5);
        let i = spawn(&mut e, MobKind::Sheep, DVec3::new(0.5, 0.0, 0.5));
        for _ in 0..10 {
            e.update(0.05, &pen, &c);
        }
        e.mobs[i].damage(1.0, None, &mut rng);
        let mut ticks = 0;
        while e.mobs[i].ai == Ai::Panic && ticks < 200 {
            e.update(0.05, &pen, &c);
            ticks += 1;
        }
        assert!((40..=42).contains(&ticks), "penned panic lasted {ticks} ticks");
    }

    /// `FollowOwnerGoal(1.0, 10, 2)`: a tamed wolf waits until its owner is
    /// ten blocks away, then follows until within two, and food doesn't
    /// tempt wolves.
    #[test]
    fn wolves_follow_their_owner_from_ten_blocks_and_ignore_food() {
        let w = Grid::flat(0);
        let mut e = Entities::new(11);
        let i = spawn(&mut e, MobKind::Wolf, DVec3::new(0.5, 0.0, 0.5));
        e.mobs[i].animal.as_mut().unwrap().owner = Some(PlayerId(1));
        let near = ctx(vec![player(1, DVec3::new(7.5, 0.0, 0.5), Some(Item::RAW_BEEF))]);
        for _ in 0..80 {
            e.update(0.05, &w, &near);
            assert!(!matches!(running(&e, i), Some(G::FollowOwner { .. } | G::Tempt)));
        }
        let start = e.mobs[i].pos;
        let owner = (start + DVec3::new(11.0, 0.0, 0.0)).with_y(0.0);
        let far = ctx(vec![player(1, owner, None)]);
        let mut closest = f64::MAX;
        for _ in 0..80 {
            e.update(0.05, &w, &far);
            closest = closest.min(flat(e.mobs[i].pos, owner));
        }
        assert!(closest <= 2.5, "followed from {start:?}, came within {closest}");
    }

    /// The follow goals replan every `adjustedTickDelay(10)` = 10 ticks,
    /// not on the melee chase's 4–10 tick cadence.
    #[test]
    fn following_the_owner_replans_every_ten_ticks() {
        let w = Grid::flat(0);
        let mut e = Entities::new(12);
        let i = spawn(&mut e, MobKind::Wolf, DVec3::new(0.5, 0.0, 0.5));
        e.mobs[i].animal.as_mut().unwrap().owner = Some(PlayerId(1));
        let mut last = None;
        let mut changes = Vec::new();
        for tick in 0..120 {
            // The owner walks away at a steady pace, 10–11 blocks ahead.
            let owner = DVec3::new(e.mobs[i].pos.x + 10.5 + (tick % 3) as f64 * 0.4, 0.0, 0.5);
            e.update(0.05, &w, &ctx(vec![player(1, owner, None)]));
            let target = e.mobs[i].nav.target();
            if target != last {
                changes.push(tick);
                last = target;
            }
        }
        assert!(changes.len() > 3, "{changes:?}");
        assert!(changes.windows(2).all(|w| w[1] - w[0] >= 10), "repaths at {changes:?}");
    }

    /// `FollowParentGoal`: a baby walks back to an adult of its kind.
    #[test]
    fn babies_follow_an_adult_of_their_kind() {
        let w = Grid::flat(0);
        let c = ctx(vec![player(1, DVec3::new(60.5, 0.0, 0.5), None)]);
        let mut e = Entities::new(7);
        let adult = spawn(&mut e, MobKind::Pig, DVec3::new(0.5, 0.0, 0.5));
        // Inside the adult's box grown by 8 (`getEntitiesOfClass(bb.inflate(8, 4, 8))`).
        let baby = spawn(&mut e, MobKind::Pig, DVec3::new(8.0, 0.0, 0.5));
        e.mobs[baby].age = crate::entity::animals::BABY_AGE;
        e.mobs[baby].baby = true;
        let mut closest = f64::MAX;
        for _ in 0..120 {
            e.update(0.05, &w, &c);
            closest = closest.min(flat(e.mobs[baby].pos, e.mobs[adult].pos));
        }
        assert!(closest < 3.5, "the baby came within {closest}");
    }

    /// `TemptGoal.stop` sets `calmDown = reducedTickDelay(100)`: an animal
    /// that loses interest isn't tempted again for 100 ticks.
    #[test]
    fn tempted_animals_calm_down_for_a_hundred_ticks() {
        let w = Grid::flat(0);
        let food = ctx(vec![player(1, DVec3::new(6.5, 0.0, 0.5), Some(Item::WHEAT))]);
        let none = ctx(vec![player(1, DVec3::new(6.5, 0.0, 0.5), None)]);
        let mut e = Entities::new(9);
        let i = spawn(&mut e, MobKind::Sheep, DVec3::new(0.5, 0.0, 0.5));
        for _ in 0..6 {
            e.update(0.05, &w, &food);
        }
        assert_eq!(running(&e, i), Some(G::Tempt));
        for _ in 0..2 {
            e.update(0.05, &w, &none);
        }
        assert_ne!(running(&e, i), Some(G::Tempt));
        let mut again = None;
        for tick in 0..140 {
            e.update(0.05, &w, &food);
            if again.is_none() && running(&e, i) == Some(G::Tempt) {
                again = Some(tick);
            }
        }
        let again = again.expect("tempted again");
        assert!((96..=104).contains(&again), "tempted again after {again} ticks");
    }

    /// Hoglins have no temptation goal in 26.3 (crimson fungus only breeds them).
    #[test]
    fn hoglins_and_foxes_are_not_tempted() {
        assert!(!super::tempted_by(
            MobKind::Hoglin,
            Item::from_block(crate::world::nether_biome_blocks::CRIMSON_FUNGUS)
        ));
        assert!(!super::tempted_by(MobKind::Fox, Item::SWEET_BERRIES));
        assert!(super::tempted_by(MobKind::Pig, Item::CARROT_ON_A_STICK));
        assert!(super::tempted_by(MobKind::Mule, Item::GOLDEN_CARROT));
    }

    /// `AbstractSkeleton` avoids wolves within 6 blocks (`AvoidEntityGoal`,
    /// walk 1.0, sprint 1.2 inside 7 blocks), tame or not.
    #[test]
    fn skeletons_back_away_from_wolves() {
        let w = Grid::flat(0);
        let c = ctx(vec![player(1, DVec3::new(80.5, 0.0, 0.5), None)]);
        let mut e = Entities::new(13);
        let s = spawn(&mut e, MobKind::Skeleton, DVec3::new(0.5, 0.0, 0.5));
        let wolf = spawn(&mut e, MobKind::Wolf, DVec3::new(3.5, 0.0, 0.5));
        let a = e.mobs[wolf].animal.as_mut().unwrap();
        a.owner = Some(PlayerId(9));
        a.sitting = true;
        let mut avoided = false;
        for _ in 0..60 {
            e.update(0.05, &w, &c);
            avoided |= matches!(running(&e, s), Some(G::Avoid(_)));
        }
        assert!(avoided);
        assert!(flat(e.mobs[s].pos, e.mobs[wolf].pos) > 6.0, "{:?}", e.mobs[s].pos);
    }

    /// Rabbits flee survival players within 8 blocks, not creative ones
    /// (`forCombat` targeting).
    #[test]
    fn rabbits_flee_survival_players_only() {
        let w = Grid::flat(0);
        for (survival, flees) in [(true, true), (false, false)] {
            let mut t = player(1, DVec3::new(4.5, 0.0, 0.5), None);
            t.targetable = survival;
            let c = ctx(vec![t]);
            let mut e = Entities::new(17);
            let r = spawn(&mut e, MobKind::Rabbit, DVec3::new(0.5, 0.0, 0.5));
            let mut avoided = false;
            for _ in 0..40 {
                e.update(0.05, &w, &c);
                avoided |= matches!(running(&e, r), Some(G::Avoid(_)));
            }
            assert_eq!(avoided, flees, "survival player: {survival}");
        }
    }

    /// Creepers flee cats within 6 blocks, unless already swelling at a
    /// player (`SwellGoal` at 2 outranks the avoid goal at 3).
    #[test]
    fn creepers_flee_cats_unless_swelling() {
        let w = Grid::flat(0);
        let far = ctx(vec![player(1, DVec3::new(80.5, 0.0, 0.5), None)]);
        let mut e = Entities::new(19);
        let cr = spawn(&mut e, MobKind::Creeper, DVec3::new(0.5, 0.0, 0.5));
        let cat = spawn(&mut e, MobKind::Cat, DVec3::new(4.5, 0.0, 0.5));
        e.mobs[cat].animal.as_mut().unwrap().sitting = true;
        let mut avoided = false;
        for _ in 0..40 {
            e.update(0.05, &w, &far);
            avoided |= matches!(running(&e, cr), Some(G::Avoid(_)));
        }
        assert!(avoided);
        let mut e = Entities::new(19);
        let cr = spawn(&mut e, MobKind::Creeper, DVec3::new(0.5, 0.0, 0.5));
        let cat = spawn(&mut e, MobKind::Cat, DVec3::new(3.5, 0.0, 0.5));
        e.mobs[cat].animal.as_mut().unwrap().sitting = true;
        let close = ctx(vec![player(1, DVec3::new(1.5, 0.0, 2.0), None)]);
        for _ in 0..20 {
            e.update(0.05, &w, &close);
            assert!(!matches!(running(&e, cr), Some(G::Avoid(_))));
        }
    }
}
