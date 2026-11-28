//! v1.0 gap Track 2e: the brains' idle movement in place of the game's
//! wandering: `WALK_TARGET`, the core `MoveToTargetSink` that walks it, and
//! the activities' movement `RunOne`s, with the look and walk targets they
//! set:
//!
//! - villagers' idle package (priority 2: `InteractWith` a villager, a
//!   villager that can breed or a cat within 8, `VillageBoundRandomStroll`,
//!   `SetWalkTargetFromLookTarget`, `JumpOnBed`, `DoNothing(30, 60)`), the
//!   babies' play package (5, after the look `RunOne`) and the meet
//!   package's `TriggerGate` of `StrollAroundPoi(MEETING_POINT, 0.4, 40)`
//!   and `SocializeAtBell` (2), and the rest package's `RunOne` for a
//!   villager without a home (`SetClosestHomeAsWalkTarget`,
//!   `InsideBrownianWalk`, `GoToClosestVillage`, `DoNothing(20, 40)`);
//! - piglins' idle movement (`RandomStroll.stroll(0.6)`, `InteractWith` a
//!   piglin within 8, `SetWalkTargetFromLookTarget(0.6, 3)` unless a player
//!   holds a loved item, `DoNothing(30, 60)`) after their look `RunOne`, and
//!   the celebrating piglin's `RunOne` (`SetEntityLookTarget(PIGLIN, 8)`,
//!   `RandomStroll.stroll(0.6, 2, 1)`, `DoNothing(10, 20)`);
//! - the sniffer's idle `RunOne` (`SetWalkTargetFromLookTarget(1.0, 3)`,
//!   `Scenting(40, 80)`, `Sniffing(40, 80)`, `SetEntityLookTarget(PLAYER,
//!   6)`, `RandomStroll.stroll(1.0)`, `DoNothing(5, 20)`), which starts its
//!   scent and sniff states (`sniffer.rs` runs them).
//!
//! `RunOne` is a `GateBehavior` (`SHUFFLED`, `RUN_ONE`): each start it
//! shuffles its options by weight (`ShufflingList`) and starts the first that
//! can; one-shot options stop at once, so it picks again the next tick, and a
//! running `DoNothing`, `JumpOnBed`, scent or sniff holds it. Nothing here
//! is saved, as Java saves none of it.
use super::head::{Sel, Tracked};
use crate::inventory::Stack;
use super::*;
use crate::entity::PlayerId;

/// What a `WALK_TARGET` follows (`EntityTracker(entity, false)` walks to the
/// entity's feet; `BlockPosTracker` to a block).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum WalkTo {
    Point(DVec3),
    /// A mob uid and its position, refreshed by the sense pass.
    Mob(u32, DVec3),
    Player(PlayerId),
}

/// `WalkTarget`: where, the speed modifier and `closeEnoughDist`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Walk {
    pub to: WalkTo,
    pub speed: f32,
    pub close: i32,
}

/// A running `JumpOnBed`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct BedJump {
    bed: IVec3,
    /// `remainingTimeToReachBed`, `remainingJumps`, `remainingCooldownUntilNextJump`.
    reach: i32,
    jumps: i32,
    cooldown: i32,
}

/// A running `HarvestFarmland`: `aboveFarmlandPos`, `timeWorkedSoFar` and
/// the ticks before its 60-tick run times out.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Harvest {
    cell: Option<IVec3>,
    worked: i32,
    left: i32,
}

/// The option a movement `RunOne` holds running.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Hold {
    /// `DoNothing`: ticks left.
    Nothing(i32),
    JumpOnBed(BedJump),
    /// The sniffer's scent or sniff state it started.
    Sniffer,
    /// Track 2f: `WorkAtPoi` (and `WorkAtComposter`): ticks left of its
    /// 60-tick run.
    Work(i32),
    /// Track 2f: `HarvestFarmland`.
    Harvest(Harvest),
    /// Track 2g: `UseBonemeal`.
    Bonemeal(Bonemeal),
}

/// Track 2g: a running `UseBonemeal`: `cropPos`, `nextWorkCycleTime`,
/// `timeWorkedSoFar` and the ticks before its 60-tick run times out.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Bonemeal {
    cell: Option<IVec3>,
    next: u32,
    worked: i32,
    left: i32,
}

/// v1.0 gap Track 2g: what a villager's work does to the world or drops
/// this tick; the mob's update sends each as an [`crate::entity::EntityEvent`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum FarmAct {
    /// `HarvestFarmland`'s `destroyBlock` of a ripe crop.
    Harvest(IVec3),
    /// `HarvestFarmland` planting a seed from the inventory on farmland.
    Plant(IVec3, Block),
    /// `UseBonemeal`: `BoneMealItem.growCrop` and level event 1505.
    Bonemeal(IVec3),
    /// `WorkAtComposter.compostItems`: the composter's level from (8 when
    /// `extractProduce` emptied it first) and to.
    Compost { cell: IVec3, from: u8, to: u8 },
    /// An item entity: bread it can't carry (`spawnAtLocation`), the
    /// composter's bone meal, or a half stack thrown to a trading partner
    /// (`BehaviorUtils.throwItem`); position, velocity (blocks a second).
    Item(Stack, DVec3, DVec3),
}

/// `ItemEntity`'s own toss (`nextDouble() * 0.2 - 0.1`, 0.2 up a tick), in
/// blocks a second.
fn item_toss(rng: &mut Rng) -> DVec3 {
    DVec3::new(rng.next_f64() * 0.2 - 0.1, 0.2, rng.next_f64() * 0.2 - 0.1) * 20.0
}

/// `WorkAtComposter.COMPOSTABLE_ITEMS`.
const COMPOSTABLE: [Item; 2] = [Item::WHEAT_SEEDS, Item::BEETROOT_SEEDS];

/// Track 2f: the point of interest a stroll option reads: the villager's
/// `JOB_SITE`, or the brute's `HOME` (where it spawned in its bastion).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Poi {
    Job,
    BruteHome,
}

/// `BlockPos.closerToCenterThan(pos, d)`: within `d` of the block's centre.
fn closer_to_centre(block: IVec3, pos: DVec3, d: f64) -> bool {
    (block.as_dvec3() + 0.5).distance_squared(pos) < d * d
}

/// `HarvestFarmland.validPos`: a crop at its maximum age (`CropBlock`), or
/// air over farmland.
pub(crate) fn harvestable<W: MobWorld + ?Sized>(world: &W, cell: IVec3) -> bool {
    let Some(b) = world.block(cell) else { return false };
    b.as_crop().is_some_and(|(crop, age)| age == crop.max_age())
        || b == Block::AIR && world.block(cell - IVec3::Y).is_some_and(|b| b.is_farmland())
}

/// Track 2g: `UseBonemeal.validPos`: a crop short of its last age.
fn growing_crop<W: MobWorld + ?Sized>(world: &W, cell: IVec3) -> bool {
    world.block(cell).and_then(|b| b.as_crop()).is_some_and(|(crop, age)| age < crop.max_age())
}

/// Track 2g: `UseBonemeal.pickNextTarget`: a growing crop among the 27
/// cells round the villager's block, each found replacing the pick one
/// time in its count.
fn pick_bonemeal_target<W: MobWorld + ?Sized>(world: &W, pos: DVec3, rng: &mut Rng) -> Option<IVec3> {
    let at = pos.floor().as_ivec3();
    let mut found = None;
    let mut count = 0;
    for x in -1..=1 {
        for y in -1..=1 {
            for z in -1..=1 {
                let p = at + IVec3::new(x, y, z);
                if growing_crop(world, p) {
                    count += 1;
                    if rng.next_int(count) == 0 {
                        found = Some(p);
                    }
                }
            }
        }
    }
    found
}

/// The brain walking state of a mob (never saved).
#[derive(Clone, Debug, Default)]
pub(crate) struct Walking {
    /// `WALK_TARGET`.
    pub(crate) walk: Option<Walk>,
    /// A running `MoveToTargetSink`: ticks before it times out and the cell
    /// its path was planned to (`lastTargetPos`).
    sink: Option<(i32, IVec3)>,
    /// `MoveToTargetSink.remainingCooldown`.
    cooldown: i32,
    /// The movement `RunOne`'s running option.
    hold: Option<Hold>,
    /// `StrollAroundPoi`'s `nextOkStartTime` (in the mob's ticks).
    stroll_ok: u32,
    /// `SetClosestHomeAsWalkTarget`'s `lastUpdate` (in the mob's ticks).
    home_next: u32,
    ticks: u32,
    /// Sensed by the entity pass for `VillageBoundRandomStroll`: whether the
    /// mob is in a village, and the section centre it heads for otherwise.
    pub(crate) in_village: bool,
    pub(crate) toward_village: Option<DVec3>,
    /// `NEAREST_BED` (villager babies, `NearestBedSensor`).
    pub(crate) nearest_bed: Option<IVec3>,
    /// A running `TradeWithVillager`: ticks left (0: not running).
    pub(crate) trading_villager: i32,
    /// Track 2f: `INTERACTION_TARGET` when it is a mob (a villager, or the
    /// baby a playing baby chases): its uid. `InteractWith`,
    /// `SocializeAtBell` and `PlayTagWithOtherKids` set it; the idle and
    /// meet packages' trade gate erases it.
    pub(crate) interaction: Option<u32>,
    /// Track 2f: `BehaviorUtils.lockGazeAndWalkToEachOther` this tick: the
    /// partner whose look and walk targets the sense pass points back here.
    pub(crate) lock_with: Option<u32>,
    /// Track 2f: the movement `RunOne` the running option belongs to.
    hold_of: Option<Movement>,
    /// Track 2f: the brain's clock (`gameTime` for its timers), one a tick.
    pub(crate) now: u32,
    /// Track 2f: `WorkAtPoi.lastCheck`.
    work_last: Option<u32>,
    /// Track 2f: `WorkAtPoi.start` ran; the village pass restocks
    /// (`shouldRestock`, `restock`).
    pub(crate) work_due: bool,
    /// Track 2f: the `nextOkStartTime`s of `StrollToPoi`, `StrollAroundPoi`
    /// and `StrollToPoiList` (the work package's, or the brute's on `HOME`).
    poi_to_ok: u32,
    poi_around_ok: u32,
    poi_list_ok: u32,
    /// Track 2f: `SECONDARY_JOB_SITE` (`SecondaryPoiSensor`, every 40
    /// ticks: a farmer's farmland within 4 across and 2 up or down).
    secondary: Vec<IVec3>,
    /// Track 2f: `HarvestFarmland.nextOkStartTime` and
    /// `validFarmlandAroundVillager`.
    harvest_ok: u32,
    harvest_valid: Vec<IVec3>,
    /// Track 2f: a ripe crop `HarvestFarmland` breaks this tick; Track 2g:
    /// everything the work does to the world this tick (the mob's update
    /// sends them as events).
    pub(crate) farm_acts: Vec<FarmAct>,
    /// Track 2g: `UseBonemeal.lastBonemealingSession` (brain clock; 0 before
    /// the first).
    bonemeal_last: u32,
    /// Track 2f: `CANT_REACH_WALK_TARGET_SINCE` (brain clock).
    pub(crate) cant_reach_since: Option<u32>,
    /// Track 2f: the village sections within three of the mob, for a
    /// playing baby's `level.isVillage` test (refreshed by the sense pass).
    pub(crate) villages: Vec<IVec3>,
}

/// One option of a movement `RunOne`.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Opt {
    /// `InteractWith.of(kind, range, target, speed, stopDistance)`; `breed`:
    /// both sides `AgeableMob.canBreed`.
    Interact {
        kind: MobKind,
        range: f64,
        speed: f32,
        close: i32,
        breed: bool,
    },
    /// `VillageBoundRandomStroll.create(speed)`.
    VillageStroll(f32),
    /// `RandomStroll.stroll(speed, h, v)`.
    Stroll {
        speed: f32,
        h: i32,
        v: i32,
    },
    /// `SetWalkTargetFromLookTarget.create(speed, close)`; `loved`: only
    /// while no player holding a loved item is seen (`PiglinAi`).
    FromLook {
        speed: f32,
        close: i32,
        loved: bool,
    },
    /// `JumpOnBed(speed)`.
    JumpOnBed(f32),
    /// `SetClosestHomeAsWalkTarget.create(speed)`.
    ClosestHome(f32),
    /// `InsideBrownianWalk.create(speed)`.
    InsideBrownian(f32),
    /// `GoToClosestVillage.create(speed, closeEnough)`.
    ToVillage(f32, i32),
    /// `SetEntityLookTarget.create(sel, range)`.
    Look(Sel, f32),
    Scenting,
    Sniffing,
    Nothing(i32, i32),
    /// Track 2f: `WorkAtPoi` (a farmer's `WorkAtComposter`).
    WorkAtPoi,
    /// Track 2f: `StrollToPoi.create(poi, speed, closeEnoughDist,
    /// maxDistanceFromPoi)`.
    StrollToPoi {
        poi: Poi,
        speed: f32,
        close: i32,
        max: f64,
    },
    /// Track 2f: `StrollAroundPoi.create(poi, speed, maxDistanceFromPoi)`.
    StrollAroundPoi {
        poi: Poi,
        speed: f32,
        max: f64,
    },
    /// Track 2f: `StrollToPoiList.create(SECONDARY_JOB_SITE, speed,
    /// closeEnoughDist, maxDistanceFromPoi, JOB_SITE)`.
    StrollToPoiList {
        speed: f32,
        close: i32,
        max: f64,
    },
    /// Track 2f: `HarvestFarmland`.
    Harvest,
    /// Track 2f: `UseBonemeal`, with bone meal in the villager's inventory
    /// (Track 2g).
    UseBonemeal,
}

const VILLAGER_IDLE: &[(Opt, u8)] = &[
    (Opt::Interact { kind: MobKind::Villager, range: 8.0, speed: 0.5, close: 2, breed: false }, 2),
    (Opt::Interact { kind: MobKind::Villager, range: 8.0, speed: 0.5, close: 2, breed: true }, 1),
    (Opt::Interact { kind: MobKind::Cat, range: 8.0, speed: 0.5, close: 2, breed: false }, 1),
    (Opt::VillageStroll(0.5), 1),
    (Opt::FromLook { speed: 0.5, close: 2, loved: false }, 1),
    (Opt::JumpOnBed(0.5), 1),
    (Opt::Nothing(30, 60), 1),
];
const VILLAGER_PLAY: &[(Opt, u8)] = &[
    (Opt::Interact { kind: MobKind::Villager, range: 8.0, speed: 0.5, close: 2, breed: false }, 2),
    (Opt::Interact { kind: MobKind::Cat, range: 8.0, speed: 0.5, close: 2, breed: false }, 1),
    (Opt::VillageStroll(0.5), 1),
    (Opt::FromLook { speed: 0.5, close: 2, loved: false }, 1),
    (Opt::JumpOnBed(0.5), 2),
    (Opt::Nothing(20, 40), 2),
];
const VILLAGER_REST: &[(Opt, u8)] = &[
    (Opt::ClosestHome(0.5), 1),
    (Opt::InsideBrownian(0.5), 4),
    (Opt::ToVillage(0.5, 4), 2),
    (Opt::Nothing(20, 40), 2),
];
/// Track 2f: the work package's `RunOne` (`VillagerGoalPackages.getWorkPackage`,
/// speed 0.5), for every profession but the farmer.
const VILLAGER_WORK: &[(Opt, u8)] = &[
    (Opt::WorkAtPoi, 7),
    (Opt::StrollAroundPoi { poi: Poi::Job, speed: 0.4, max: 4.0 }, 2),
    (Opt::StrollToPoi { poi: Poi::Job, speed: 0.4, close: 1, max: 10.0 }, 5),
    (Opt::StrollToPoiList { speed: 0.5, close: 1, max: 6.0 }, 5),
    (Opt::Harvest, 5),
    (Opt::UseBonemeal, 7),
];
/// The farmer's: `HarvestFarmland` 2 and `UseBonemeal` 4.
const FARMER_WORK: &[(Opt, u8)] = &[
    (Opt::WorkAtPoi, 7),
    (Opt::StrollAroundPoi { poi: Poi::Job, speed: 0.4, max: 4.0 }, 2),
    (Opt::StrollToPoi { poi: Poi::Job, speed: 0.4, close: 1, max: 10.0 }, 5),
    (Opt::StrollToPoiList { speed: 0.5, close: 1, max: 6.0 }, 5),
    (Opt::Harvest, 2),
    (Opt::UseBonemeal, 4),
];
/// Track 2f: `PiglinBruteAi.createIdleMovementBehaviors`.
const BRUTE_IDLE: &[(Opt, u8)] = &[
    (Opt::Stroll { speed: 0.6, h: 10, v: 7 }, 2),
    (Opt::Interact { kind: MobKind::Piglin, range: 8.0, speed: 0.6, close: 2, breed: false }, 2),
    (Opt::Interact { kind: MobKind::PiglinBrute, range: 8.0, speed: 0.6, close: 2, breed: false }, 2),
    (Opt::StrollToPoi { poi: Poi::BruteHome, speed: 0.6, close: 2, max: 100.0 }, 2),
    (Opt::StrollAroundPoi { poi: Poi::BruteHome, speed: 0.6, max: 5.0 }, 2),
    (Opt::Nothing(30, 60), 1),
];
const PIGLIN_IDLE: &[(Opt, u8)] = &[
    (Opt::Stroll { speed: 0.6, h: 10, v: 7 }, 2),
    (Opt::Interact { kind: MobKind::Piglin, range: 8.0, speed: 0.6, close: 2, breed: false }, 2),
    (Opt::FromLook { speed: 0.6, close: 3, loved: true }, 2),
    (Opt::Nothing(30, 60), 1),
];
const PIGLIN_CELEBRATE: &[(Opt, u8)] = &[
    (Opt::Look(Sel::Kind(MobKind::Piglin), 8.0), 1),
    (Opt::Stroll { speed: 0.6, h: 2, v: 1 }, 1),
    (Opt::Nothing(10, 20), 1),
];
const SNIFFER_IDLE: &[(Opt, u8)] = &[
    (Opt::FromLook { speed: 1.0, close: 3, loved: false }, 2),
    (Opt::Scenting, 1),
    (Opt::Sniffing, 1),
    (Opt::Look(Sel::Player, 6.0), 1),
    (Opt::Stroll { speed: 1.0, h: 10, v: 7 }, 1),
    (Opt::Nothing(5, 20), 2),
];

/// Which movement `RunOne` (or gate) a brain mob runs this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Movement {
    VillagerIdle,
    VillagerPlay,
    VillagerMeet,
    /// The rest package's `RunOne` (`HOME` absent).
    VillagerHomeless,
    /// Track 2f: the work package's `RunOne` (priority 5, after the look
    /// `RunOne`).
    VillagerWork,
    PiglinIdle,
    /// Track 2f: the brute's idle movement `RunOne`.
    BruteIdle,
    PiglinCelebrate,
    SnifferIdle,
}

/// `BlockPos.distManhattan`.
fn manhattan(a: IVec3, b: IVec3) -> i32 {
    let d = (a - b).abs();
    d.x + d.y + d.z
}

impl Mob {
    /// The walk target's block (`currentBlockPosition`), or `None` once
    /// what it follows is gone.
    fn walk_cell(&self, w: Walk, ctx: &Ctx) -> Option<IVec3> {
        let at = match w.to {
            WalkTo::Point(p) | WalkTo::Mob(_, p) => p,
            WalkTo::Player(id) => ctx.players.iter().find(|t| t.id == id && t.alive && !t.spectator)?.pos,
        };
        Some(at.floor().as_ivec3())
    }

    /// The ground speed of a walk's speed modifier (a baby piglin's
    /// `SPEED_MODIFIER_BABY` raises its base by 0.2).
    fn walk_speed(&self, modifier: f32) -> f64 {
        let baby = if self.kind == MobKind::Piglin && self.is_baby() { nether_ai::BABY_SPEED } else { 1.0 };
        self.goal_speed(f64::from(modifier)) * baby * baby
    }

    /// Sets `WALK_TARGET` (`setOrErase`).
    pub(crate) fn set_walk(&mut self, walk: Option<Walk>) {
        self.walking.walk = walk;
    }

    /// The core `MoveToTargetSink(min, max)` for a brain mob: plans a path
    /// to `WALK_TARGET` (erasing it once reached, or when no path or partial
    /// step exists) and walks it until the path is done, the target is
    /// reached, or it times out after `min..=max` ticks; a stuck walk waits
    /// `nextInt(40)` ticks before planning again. `None` while it isn't
    /// running.
    pub(crate) fn walk_sink<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        (min, max): (i32, i32),
    ) -> Option<(Option<DVec3>, f64)> {
        self.walking.ticks = self.walking.ticks.wrapping_add(1);
        let Some(walk) = self.walking.walk else {
            if self.walking.sink.take().is_some() {
                self.stop_navigation();
            }
            return None;
        };
        let here = self.pos.floor().as_ivec3();
        let Some(cell) = self.walk_cell(walk, ctx) else {
            self.walking.walk = None;
            self.walking.sink = None;
            self.stop_navigation();
            return None;
        };
        let reached = manhattan(cell, here) <= walk.close;
        let speed = self.walk_speed(walk.speed);
        match self.walking.sink {
            None => {
                // `checkExtraStartConditions`.
                if self.walking.cooldown > 0 {
                    self.walking.cooldown -= 1;
                    return None;
                }
                if reached || !self.plan_walk(world, ctx, rng, cell, speed) {
                    if reached {
                        self.walking.cant_reach_since = None;
                    }
                    self.walking.walk = None;
                    return None;
                }
                self.walking.sink = Some((min + rng.next_int((max - min + 1) as u32) as i32, cell));
            }
            Some((left, last)) => {
                // `canStillUse`, then the timeout.
                if self.nav.is_done() || reached || left <= 0 {
                    if !reached && self.blocked {
                        self.walking.cooldown = rng.next_int(40) as i32;
                    }
                    self.stop_navigation();
                    self.walking.walk = None;
                    self.walking.sink = None;
                    return None;
                }
                // `tick`: plan again when the target moved over 2 blocks.
                let last = if (cell - last).as_dvec3().length_squared() > 4.0
                    && self.plan_walk(world, ctx, rng, cell, speed)
                {
                    cell
                } else {
                    last
                };
                self.walking.sink = Some((left - 1, last));
            }
        }
        Some((self.follow_path(world, speed, false), speed))
    }

    /// `tryComputePath`: a path to the cell, or toward it
    /// (`DefaultRandomPos.getPosTowards(10, 7, .., pi/2)`) when none exists.
    fn plan_walk<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        cell: IVec3,
        speed: f64,
    ) -> bool {
        if self.walk_to(world, cell, 0, speed, false) {
            self.walking.cant_reach_since = None;
            return true;
        }
        // Track 2f: `CANT_REACH_WALK_TARGET_SINCE`, from the first path that
        // falls short (or none).
        let now = self.walking.now;
        self.walking.cant_reach_since.get_or_insert(now);
        if self.nav.path.is_some() {
            return true;
        }
        let towards = cell.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
        match self.pos_towards(world, ctx, rng, 10, 7, towards, std::f64::consts::FRAC_PI_2) {
            Some(step) => self.walk_to(world, step, 0, speed, false) || self.nav.path.is_some(),
            None => false,
        }
    }

    /// The movement behaviours of a brain mob's activity this tick, in their
    /// place among its look behaviours (`head::brain_looks`).
    pub(crate) fn brain_movement<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx, rng: &mut Rng, m: Movement) {
        // Track 2f: another activity's `RunOne` stops the running option.
        if self.walking.hold_of != Some(m) {
            self.stop_brain_movement();
            self.walking.hold_of = Some(m);
        }
        match m {
            Movement::VillagerMeet => self.meet_gate(world, ctx, rng),
            Movement::VillagerIdle => self.run_one(world, ctx, rng, VILLAGER_IDLE),
            Movement::VillagerPlay => self.run_one(world, ctx, rng, VILLAGER_PLAY),
            Movement::VillagerHomeless => self.run_one(world, ctx, rng, VILLAGER_REST),
            Movement::VillagerWork => {
                self.sense_secondary(world);
                let farmer =
                    self.villager.as_ref().is_some_and(|v| v.profession == crate::entity::villager::Profession::Farmer);
                self.run_one(world, ctx, rng, if farmer { FARMER_WORK } else { VILLAGER_WORK })
            }
            Movement::BruteIdle => self.run_one(world, ctx, rng, BRUTE_IDLE),
            Movement::PiglinIdle => self.run_one(world, ctx, rng, PIGLIN_IDLE),
            Movement::PiglinCelebrate => self.run_one(world, ctx, rng, PIGLIN_CELEBRATE),
            Movement::SnifferIdle => self.run_one(world, ctx, rng, SNIFFER_IDLE),
        }
    }

    /// Whether `m`'s `RunOne` holds a running option.
    pub(crate) fn running_option_of(&self, m: Movement) -> bool {
        self.walking.hold.is_some() && self.walking.hold_of == Some(m)
    }

    /// Ends a running movement option when the activity changes.
    pub(crate) fn stop_brain_movement(&mut self) {
        if let Some(Hold::Sniffer) = self.walking.hold
            && let Some(s) = &mut self.sniffer
            && matches!(s.pose, crate::entity::sniffer::SCENTING | crate::entity::sniffer::SNIFFING)
        {
            s.stop_early();
        }
        if let Some(Hold::Bonemeal(_)) = self.walking.hold {
            self.walking.bonemeal_last = self.walking.now;
        }
        if let Some(Hold::Harvest(_)) = self.walking.hold {
            self.stop_harvest();
        }
        self.walking.hold = None;
    }

    /// `HarvestFarmland.stop`: erases `LOOK_TARGET` and `WALK_TARGET`, and
    /// waits 40 ticks before it starts again.
    fn stop_harvest(&mut self) {
        self.head.target = None;
        self.walking.walk = None;
        self.walking.harvest_ok = self.walking.now.wrapping_add(40);
    }

    /// Track 2f: `SecondaryPoiSensor` (every 40 ticks): the farmland within
    /// 4 blocks across and 2 up or down of a farmer (the only profession
    /// with a secondary point of interest).
    fn sense_secondary<W: MobWorld + ?Sized>(&mut self, world: &W) {
        if !self.walking.now.wrapping_add(self.uid).is_multiple_of(40) && self.walking.now > 1 {
            return;
        }
        let farmer =
            self.villager.as_ref().is_some_and(|v| v.profession == crate::entity::villager::Profession::Farmer);
        self.walking.secondary.clear();
        if !farmer {
            return;
        }
        let here = self.pos.floor().as_ivec3();
        for x in -4..=4 {
            for y in -2..=2 {
                for z in -4..=4 {
                    let p = here + IVec3::new(x, y, z);
                    if world.block(p).is_some_and(|b| b.is_farmland()) {
                        self.walking.secondary.push(p);
                    }
                }
            }
        }
    }

    /// The block a stroll option's point of interest names.
    fn poi(&self, poi: Poi) -> Option<IVec3> {
        match poi {
            Poi::Job => self.villager.as_ref()?.job,
            Poi::BruteHome => self.nether.as_ref()?.home.map(|h| h.floor().as_ivec3()),
        }
    }

    /// A `RunOne` of `options`: ticks its running option, else shuffles and
    /// starts the first that can.
    fn run_one<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx, rng: &mut Rng, options: &[(Opt, u8)]) {
        if let Some(hold) = self.walking.hold {
            if self.tick_hold(world, rng, hold) {
                return;
            }
            if let Hold::Harvest(_) = hold {
                self.stop_harvest();
            }
            if let Hold::Bonemeal(_) = hold {
                // `UseBonemeal.stop`.
                self.walking.bonemeal_last = self.walking.now;
            }
            self.walking.hold = None;
            // The gate stops; it starts again on the next tick.
            return;
        }
        let mut order: Vec<(f32, usize)> = options
            .iter()
            .enumerate()
            .map(|(i, (_, w))| (-rng.next_f32().powf(1.0 / f32::from((*w).max(1))), i))
            .collect();
        order.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, i) in order {
            if self.try_option(world, ctx, rng, options[i].0) {
                break;
            }
        }
    }

    /// Ticks a running option; `false` once it stops.
    fn tick_hold<W: MobWorld + ?Sized>(&mut self, world: &W, rng: &mut Rng, hold: Hold) -> bool {
        match hold {
            Hold::Work(left) => {
                // `canStillUse`: still within 1.73 of the job site's centre.
                let near = self
                    .poi(Poi::Job)
                    .is_some_and(|j| closer_to_centre(j, self.pos, crate::entity::villager::WORK_DISTANCE));
                if left <= 0 || !near {
                    return false;
                }
                self.walking.hold = Some(Hold::Work(left - 1));
                true
            }
            Hold::Harvest(mut h) => {
                // `canStillUse` (`timeWorkedSoFar < 200`) and the timeout.
                if h.left <= 0 || h.worked >= 200 {
                    return false;
                }
                h.left -= 1;
                // `tick`: once within 1 of the cell's centre.
                if h.cell.is_none_or(|c| closer_to_centre(c, self.pos, 1.0)) {
                    let now = self.walking.now;
                    if let Some(c) = h.cell
                        && now > self.walking.harvest_ok
                    {
                        let b = world.block(c);
                        if b.is_some_and(|b| b.as_crop().is_some_and(|(crop, age)| age == crop.max_age())) {
                            // `destroyBlock` (its drops fall for the farmer
                            // to pick up).
                            self.walking.farm_acts.push(FarmAct::Harvest(c));
                        }
                        // Track 2g: air over farmland and `hasFarmSeeds`: the
                        // first `VILLAGER_PLANTABLE_SEEDS` stack is planted.
                        if b == Some(Block::AIR) && world.block(c - IVec3::Y).is_some_and(|b| b.is_farmland()) {
                            self.plant_seed(c);
                        }
                        if b.is_some_and(|b| b.as_crop().is_some_and(|(crop, age)| age < crop.max_age())) {
                            self.walking.harvest_valid.retain(|&p| p != c);
                            h.cell = (!self.walking.harvest_valid.is_empty()).then(|| {
                                self.walking.harvest_valid
                                    [rng.next_int(self.walking.harvest_valid.len() as u32) as usize]
                            });
                            if let Some(next) = h.cell {
                                self.walking.harvest_ok = now.wrapping_add(20);
                                self.harvest_walk(next);
                            }
                        }
                    }
                    h.worked += 1;
                }
                self.walking.hold = Some(Hold::Harvest(h));
                true
            }
            Hold::Bonemeal(mut h) => {
                // `canStillUse` and the 60-tick timeout.
                if h.left <= 0 || h.worked >= 80 || h.cell.is_none() {
                    return false;
                }
                h.left -= 1;
                let now = self.walking.now;
                if let Some(c) = h.cell
                    && now >= h.next
                    && closer_to_centre(c, self.pos, 1.0)
                {
                    // `BoneMealItem.growCrop`: a crop short of its last age.
                    let slot = self.villager.as_ref().and_then(|v| {
                        v.inventory().iter().position(|s| s.is_some_and(|s| s.item == Item::BONE_MEAL))
                    });
                    if let Some(slot) = slot
                        && growing_crop(world, c)
                    {
                        self.villager.as_mut().unwrap().shrink_slot(slot);
                        self.walking.farm_acts.push(FarmAct::Bonemeal(c));
                        h.cell = pick_bonemeal_target(world, self.pos, rng);
                        if let Some(next) = h.cell {
                            self.harvest_walk(next);
                        }
                        h.next = now.wrapping_add(40);
                    }
                    h.worked += 1;
                }
                self.walking.hold = Some(Hold::Bonemeal(h));
                true
            }
            Hold::Nothing(left) => {
                // `tickOrStop`: stops once past its end tick.
                if left <= 0 {
                    return false;
                }
                self.walking.hold = Some(Hold::Nothing(left - 1));
                true
            }
            Hold::Sniffer => self
                .sniffer
                .as_ref()
                .is_some_and(|s| matches!(s.pose, crate::entity::sniffer::SCENTING | crate::entity::sniffer::SNIFFING)),
            Hold::JumpOnBed(mut j) => {
                // `canStillUse`.
                let on_or_over = self.on_or_over_bed(world);
                let tired_walking = !on_or_over && j.reach <= 0;
                let tired_jumping = on_or_over && j.jumps <= 0;
                if !self.is_baby() || !jumpable(world, j.bed) || tired_walking || tired_jumping {
                    return false;
                }
                // `tick`.
                if !on_or_over {
                    j.reach -= 1;
                } else if j.cooldown > 0 {
                    j.cooldown -= 1;
                } else if jumpable(world, self.pos.floor().as_ivec3()) {
                    self.jump = true;
                    j.jumps -= 1;
                    j.cooldown = 5;
                }
                self.walking.hold = Some(Hold::JumpOnBed(j));
                true
            }
        }
    }

    fn on_or_over_bed<W: MobWorld + ?Sized>(&self, world: &W) -> bool {
        let p = self.pos.floor().as_ivec3();
        jumpable(world, p) || jumpable(world, p - IVec3::Y)
    }

    /// `tryStart` of one option: whether it started (one-shots that ran
    /// count, as `OneShot.tryStart` returns their trigger's result).
    fn try_option<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx, rng: &mut Rng, opt: Opt) -> bool {
        let walking = self.walking.walk.is_some();
        match opt {
            Opt::Nothing(min, max) => {
                let left = min + rng.next_int((max + 1 - min) as u32) as i32;
                self.walking.hold = Some(Hold::Nothing(left));
                true
            }
            Opt::Look(sel, range) => {
                // `SetEntityLookTarget` needs `LOOK_TARGET` absent.
                if self.head.target.is_some() {
                    return false;
                }
                match self.brain_find(world, ctx, sel, range) {
                    Some(at) => {
                        self.head.target = Some(at);
                        true
                    }
                    None => false,
                }
            }
            Opt::Interact { kind, range, speed, close, breed } => {
                if walking {
                    return false;
                }
                // `entities.contains(isTargetValid)`, then the closest within
                // range; true even when none is close enough.
                let valid =
                    |s: &super::head::Seen, me: &Mob| s.kind == kind && (!breed || me.villager_can_breed_seen(s));
                if !self.head.seen.iter().any(|s| valid(s, self)) {
                    return false;
                }
                if breed && !self.villager_can_breed() {
                    return false;
                }
                let r2 = range * range;
                let found =
                    self.head.seen.iter().find(|s| s.pos.distance_squared(self.pos) <= r2 && valid(s, self)).copied();
                if let Some(s) = found {
                    self.head.target = Some(Tracked::Mob(s.uid, s.eye));
                    self.head.fresh_target = true;
                    self.set_walk(Some(Walk { to: WalkTo::Mob(s.uid, s.pos), speed, close }));
                    if self.kind == MobKind::Villager && !breed {
                        // `INTERACTION_TARGET`: the trade gate runs
                        // `TradeWithVillager` with a villager, and erases a cat.
                        self.walking.interaction = Some(s.uid);
                    }
                }
                true
            }
            Opt::WorkAtPoi => {
                let Some(job) = self.poi(Poi::Job) else { return false };
                // `checkExtraStartConditions`: every 300 ticks, on a coin
                // flip, within 1.73 of the job site's centre.
                let now = self.walking.now;
                if self
                    .walking
                    .work_last
                    .is_some_and(|t| i64::from(now.wrapping_sub(t)) < crate::entity::villager::WORK_CHECK_COOLDOWN)
                {
                    return false;
                }
                if rng.next_int(2) != 0 {
                    return false;
                }
                self.walking.work_last = Some(now);
                if !closer_to_centre(job, self.pos, crate::entity::villager::WORK_DISTANCE) {
                    return false;
                }
                // `start`: looks at the job site (`BlockPosTracker`), uses it
                // and restocks if it should (the village pass does that).
                self.head.target = Some(Tracked::Point(job.as_dvec3() + 0.5));
                // Track 2g: a farmer's `WorkAtComposter.useWorkstation`.
                let farmer =
                    self.villager.as_ref().is_some_and(|v| v.profession == crate::entity::villager::Profession::Farmer);
                if farmer
                    && let Some(level) = world.block(job).and_then(crate::world::composter::level)
                {
                    self.make_bread(rng);
                    self.compost_items(job, level, rng);
                }
                self.walking.work_due = true;
                self.walking.hold = Some(Hold::Work(60));
                true
            }
            Opt::StrollToPoi { poi, speed, close, max } => {
                let Some(p) = self.poi(poi) else { return false };
                if !closer_to_centre(p, self.pos, max) {
                    return false;
                }
                let now = self.walking.now;
                if now > self.walking.poi_to_ok {
                    self.set_walk(Some(Walk {
                        to: WalkTo::Point(p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)),
                        speed,
                        close,
                    }));
                    self.walking.poi_to_ok = now.wrapping_add(80);
                }
                true
            }
            Opt::StrollAroundPoi { poi, speed, max } => {
                let Some(p) = self.poi(poi) else { return false };
                if !closer_to_centre(p, self.pos, max) {
                    return false;
                }
                let now = self.walking.now;
                if now > self.walking.poi_around_ok {
                    // `LandRandomPos.getPos(body, 8, 6)`, `setOrErase`.
                    let at = self.random_pos(world, ctx, rng, 8, 6, true);
                    self.set_walk(at.map(|q| Walk {
                        to: WalkTo::Point(q.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)),
                        speed,
                        close: 1,
                    }));
                    self.walking.poi_around_ok = now.wrapping_add(180);
                }
                true
            }
            Opt::StrollToPoiList { speed, close, max } => {
                let Some(job) = self.poi(Poi::Job) else { return false };
                if self.walking.secondary.is_empty() {
                    return false;
                }
                let at = self.walking.secondary[rng.next_int(self.walking.secondary.len() as u32) as usize];
                if !closer_to_centre(job, self.pos, max) {
                    return false;
                }
                let now = self.walking.now;
                if now > self.walking.poi_list_ok {
                    self.set_walk(Some(Walk {
                        to: WalkTo::Point(at.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)),
                        speed,
                        close,
                    }));
                    self.walking.poi_list_ok = now.wrapping_add(100);
                }
                true
            }
            Opt::Harvest => {
                // `LOOK_TARGET` and `WALK_TARGET` absent, `SECONDARY_JOB_SITE`
                // present, `mobGriefing`, a farmer.
                let farmer =
                    self.villager.as_ref().is_some_and(|v| v.profession == crate::entity::villager::Profession::Farmer);
                if walking
                    || self.head.target.is_some()
                    || self.walking.secondary.is_empty()
                    || !self.griefing
                    || !farmer
                {
                    return false;
                }
                let mut valid = Vec::new();
                for x in -1..=1 {
                    for y in -1..=1 {
                        for z in -1..=1 {
                            let p =
                                (self.pos + DVec3::new(f64::from(x), f64::from(y), f64::from(z))).floor().as_ivec3();
                            if harvestable(world, p) {
                                valid.push(p);
                            }
                        }
                    }
                }
                if valid.is_empty() {
                    self.walking.harvest_valid = valid;
                    return false;
                }
                let cell = valid[rng.next_int(valid.len() as u32) as usize];
                self.walking.harvest_valid = valid;
                // `start`.
                if self.walking.now > self.walking.harvest_ok {
                    self.harvest_walk(cell);
                }
                self.walking.hold = Some(Hold::Harvest(Harvest { cell: Some(cell), worked: 0, left: 60 }));
                true
            }
            Opt::UseBonemeal => {
                // Track 2g: `LOOK_TARGET` and `WALK_TARGET` absent; every 10
                // ticks, 160 after the last session, with bone meal.
                let now = self.walking.now;
                let last = self.walking.bonemeal_last;
                if walking
                    || self.head.target.is_some()
                    || !now.is_multiple_of(10)
                    || last != 0 && last.wrapping_add(160) > now
                    || self.villager.as_ref().is_none_or(|v| v.count_item(Item::BONE_MEAL) == 0)
                {
                    return false;
                }
                let Some(cell) = pick_bonemeal_target(world, self.pos, rng) else { return false };
                // `start`: looks and walks at the crop (0.5, 1 short), holding
                // bone meal.
                self.harvest_walk(cell);
                self.walking.hold = Some(Hold::Bonemeal(Bonemeal { cell: Some(cell), next: now, worked: 0, left: 60 }));
                true
            }
            Opt::VillageStroll(speed) => {
                if walking {
                    return false;
                }
                let at = match self.walking.toward_village.filter(|_| !self.walking.in_village) {
                    Some(c) => self.pos_towards(world, ctx, rng, 10, 7, c, std::f64::consts::FRAC_PI_2),
                    None => self.random_pos(world, ctx, rng, 10, 7, true),
                };
                self.set_walk(at.map(|p| Walk {
                    to: WalkTo::Point(p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)),
                    speed,
                    close: 0,
                }));
                true
            }
            Opt::Stroll { speed, h, v } => {
                if walking {
                    return false;
                }
                let at = self.random_pos(world, ctx, rng, h, v, true);
                self.set_walk(at.map(|p| Walk {
                    to: WalkTo::Point(p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)),
                    speed,
                    close: 0,
                }));
                true
            }
            Opt::FromLook { speed, close, loved } => {
                if walking {
                    return false;
                }
                let Some(at) = self.head.target else { return false };
                if loved && self.find_closest_loved(world, ctx) {
                    return false;
                }
                let to = match at {
                    Tracked::Player(id) => WalkTo::Player(id),
                    Tracked::Mob(uid, _) => {
                        let feet = self.head.seen.iter().find(|s| s.uid == uid).map_or(self.pos, |s| s.pos);
                        WalkTo::Mob(uid, feet)
                    }
                    Tracked::Point(p) => WalkTo::Point(p),
                };
                self.set_walk(Some(Walk { to, speed, close }));
                true
            }
            Opt::JumpOnBed(speed) => {
                if walking || !self.is_baby() {
                    return false;
                }
                let Some(bed) = self.walking.nearest_bed else {
                    if !self.on_or_over_bed(world) {
                        return false;
                    }
                    // `start` finds no `NEAREST_BED`: it runs with no bed and
                    // stops at once.
                    return true;
                };
                let jumps = 3 + rng.next_int(4) as i32;
                self.walking.hold = Some(Hold::JumpOnBed(BedJump { bed, reach: 100, jumps, cooldown: 0 }));
                self.set_walk(Some(Walk {
                    to: WalkTo::Point(bed.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)),
                    speed,
                    close: 0,
                }));
                true
            }
            Opt::ClosestHome(speed) => {
                // At most every 20 ticks (plus up to 19 more after a search):
                // the closest bed within 48, claimed or not, more than two
                // blocks off, that a path reaches.
                if walking || self.walking.ticks < self.walking.home_next {
                    return false;
                }
                let here = self.pos.floor().as_ivec3();
                let mut best: Option<(i32, IVec3)> = None;
                world.village_pois(&mut |p, b| {
                    let d = (p - here).length_squared();
                    if b.is_bed_head() && d <= 48 * 48 && best.is_none_or(|(e, _)| d < e) {
                        best = Some((d, p));
                    }
                });
                let Some((d, bed)) = best.filter(|&(d, _)| d > 4) else { return false };
                let _ = d;
                self.walking.home_next = self.walking.ticks + 20 + rng.next_int(20);
                let speed_now = self.walk_speed(speed);
                if self.walk_to(world, bed, 1, speed_now, false) {
                    self.set_walk(Some(Walk {
                        to: WalkTo::Point(bed.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)),
                        speed,
                        close: 1,
                    }));
                }
                self.stop_navigation();
                true
            }
            Opt::InsideBrownian(speed) => {
                let here = self.pos.floor().as_ivec3();
                if walking || world.exposed(here) {
                    return false;
                }
                let mut cells: Vec<IVec3> = (-1..=1)
                    .flat_map(|y| (-1..=1).flat_map(move |z| (-1..=1).map(move |x| here + IVec3::new(x, y, z))))
                    .collect();
                // `Collections.shuffle`.
                for i in (1..cells.len()).rev() {
                    cells.swap(i, rng.next_int(i as u32 + 1) as usize);
                }
                if let Some(p) =
                    cells.into_iter().find(|&p| !world.exposed(p) && world.block(p).is_some_and(|b| b.is_solid()))
                {
                    self.set_walk(Some(Walk {
                        to: WalkTo::Point(p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)),
                        speed,
                        close: 0,
                    }));
                }
                true
            }
            Opt::ToVillage(speed, close) => {
                if walking || self.walking.in_village {
                    return false;
                }
                if let Some(at) = self
                    .walking
                    .toward_village
                    .and_then(|c| self.pos_towards(world, ctx, rng, 15, 7, c, std::f64::consts::FRAC_PI_2))
                {
                    self.set_walk(Some(Walk {
                        to: WalkTo::Point(at.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)),
                        speed,
                        close,
                    }));
                }
                true
            }
            Opt::Scenting => {
                let tempted = self.goals.tempt_player.is_some();
                let Some(s) = self.sniffer.as_mut() else { return false };
                if s.pose != crate::entity::sniffer::IDLE || s.target.is_some() || tempted || self.ai == Ai::Panic {
                    return false;
                }
                if self.animal.as_ref().is_some_and(|a| a.partner.is_some()) {
                    return false;
                }
                let d = 40 + rng.next_int(41) as u16;
                s.start(crate::entity::sniffer::SCENTING, d);
                self.walking.hold = Some(Hold::Sniffer);
                true
            }
            Opt::Sniffing => {
                if walking || self.is_baby() || !self.sniffer_can_sniff(ctx) {
                    return false;
                }
                let Some(s) = self.sniffer.as_mut() else { return false };
                if s.pose != crate::entity::sniffer::IDLE || s.target.is_some() || s.cooldown > 0 {
                    return false;
                }
                let d = 40 + rng.next_int(41) as u16;
                s.start(crate::entity::sniffer::SNIFFING, d);
                self.walking.hold = Some(Hold::Sniffer);
                true
            }
        }
    }

    /// Track 2g: `HarvestFarmland`'s planting: the first inventory stack in
    /// `VILLAGER_PLANTABLE_SEEDS` that places a block is planted at `cell`
    /// (the world plants it and plays `item.crop.plant`) and shrinks by one.
    fn plant_seed(&mut self, cell: IVec3) {
        let Some(v) = self.villager.as_mut() else { return };
        let found = v.inventory().iter().enumerate().find_map(|(i, s)| {
            let s = (*s)?;
            if !crate::entity::villager_breeding::plantable_seed(s.item) {
                return None;
            }
            s.item.places().map(|b| (i, b))
        });
        if let Some((slot, block)) = found {
            v.shrink_slot(slot);
            self.walking.farm_acts.push(FarmAct::Plant(cell, block));
        }
    }

    /// Track 2g: `WorkAtComposter.makeBread`: with 36 bread or fewer, up to
    /// three bread from three wheat each; what doesn't fit drops half a block
    /// up (`spawnAtLocation(stack, 0.5)`).
    fn make_bread(&mut self, rng: &mut Rng) {
        let Some(v) = self.villager.as_mut() else { return };
        if v.count_item(Item::BREAD) > 36 {
            return;
        }
        let bread = (v.count_item(Item::WHEAT) / 3).min(3);
        if bread == 0 {
            return;
        }
        v.remove_item_type(Item::WHEAT, bread * 3);
        if let Some(left) = v.accept_food(Stack::new(Item::BREAD, bread as u8)) {
            let at = self.pos + DVec3::Y * 0.5;
            self.walking.farm_acts.push(FarmAct::Item(left, at, item_toss(rng)));
        }
    }

    /// Track 2g: `WorkAtComposter.compostItems`: a full composter is emptied
    /// first (`extractProduce`: one bone meal at its top, within 0.35 across);
    /// then, from the last slot back, wheat and beetroot seeds beyond ten of
    /// each kind go in, at most 20, each `ComposterBlock.insertItem` (a layer
    /// on its chance, the first always), stopping at level 7.
    fn compost_items(&mut self, cell: IVec3, mut level: u8, rng: &mut Rng) {
        let from = level;
        if level == 8 {
            let at = cell.as_dvec3()
                + DVec3::new(0.5 + (rng.next_f64() - 0.5) * 0.7, 1.01, 0.5 + (rng.next_f64() - 0.5) * 0.7);
            let toss = item_toss(rng);
            self.walking.farm_acts.push(FarmAct::Item(Stack::new(Item::BONE_MEAL, 1), at, toss));
            level = 0;
        }
        let Some(v) = self.villager.as_mut() else { return };
        let mut total = 20u32;
        let mut seen = [0u32; 2];
        'slots: for i in (0..8).rev() {
            if total == 0 {
                break;
            }
            let Some(s) = v.inventory()[i] else { continue };
            let Some(kind) = COMPOSTABLE.iter().position(|&c| c == s.item) else { continue };
            let count = u32::from(s.count);
            seen[kind] += count;
            let to_use = seen[kind].saturating_sub(10).min(total).min(count);
            total -= to_use;
            for _ in 0..to_use {
                if level < 7 {
                    let chance = crate::world::composter::chance(s.item);
                    if level == 0 && chance > 0.0 || rng.next_f64() < f64::from(chance) {
                        level += 1;
                    }
                    v.shrink_slot(i);
                }
                if level == 7 {
                    break 'slots;
                }
            }
        }
        if level != from {
            self.walking.farm_acts.push(FarmAct::Compost { cell, from, to: level });
        }
    }

    /// Track 2g: whether a `UseBonemeal` is running (the villager holds
    /// bone meal in its main hand, for the model).
    pub fn using_bone_meal(&self) -> bool {
        matches!(self.walking.hold, Some(Hold::Bonemeal(_)))
    }

    /// `HarvestFarmland`'s look and walk to a cell (`BlockPosTracker`, 0.5,
    /// 1 block short).
    fn harvest_walk(&mut self, cell: IVec3) {
        self.head.target = Some(Tracked::Point(cell.as_dvec3() + 0.5));
        self.set_walk(Some(Walk {
            to: WalkTo::Point(cell.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)),
            speed: 0.5,
            close: 1,
        }));
    }

    /// The meet package's `TriggerGate.triggerOneShuffled` of
    /// `StrollAroundPoi(MEETING_POINT, 0.4, 40)` and `SocializeAtBell` (2
    /// each): both are tried in a shuffled order every tick.
    fn meet_gate<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx, rng: &mut Rng) {
        let Some(bell) = self.villager.as_ref().and_then(|v| v.meeting) else { return };
        let centre = bell.as_dvec3() + 0.5;
        let first_stroll = -rng.next_f32().powf(0.5) <= -rng.next_f32().powf(0.5);
        for stroll in [first_stroll, !first_stroll] {
            if stroll {
                // `StrollAroundPoi`: within 40 of the bell, every 180 ticks.
                if centre.distance_squared(self.pos) < 40.0 * 40.0 && self.walking.ticks > self.walking.stroll_ok {
                    let at = self.random_pos(world, ctx, rng, 8, 6, true);
                    self.set_walk(at.map(|p| Walk {
                        to: WalkTo::Point(p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)),
                        speed: 0.4,
                        close: 1,
                    }));
                    self.walking.stroll_ok = self.walking.ticks + 180;
                }
            } else if self.head.interaction.is_none()
                && self.walking.interaction.is_none()
                && rng.next_int(100) == 0
                && centre.distance_squared(self.pos) < 16.0
                && self.head.seen.iter().any(|s| s.kind == MobKind::Villager)
                && let Some(s) = self
                    .head
                    .seen
                    .iter()
                    .find(|s| s.kind == MobKind::Villager && s.pos.distance_squared(self.pos) <= 32.0)
                    .copied()
            {
                // `SocializeAtBell`.
                self.walking.interaction = Some(s.uid);
                self.head.target = Some(Tracked::Mob(s.uid, s.eye));
                self.head.fresh_target = true;
                self.set_walk(Some(Walk { to: WalkTo::Mob(s.uid, s.pos), speed: 0.3, close: 1 }));
            }
        }
    }

    /// Track 2f: `SetWalkTargetFromBlockMemory.create(JOB_SITE, 0.5, 9, 100,
    /// 1200)` (work package, priority 2): with no walk target, a working
    /// villager more than 9 blocks (`distManhattan`) from its job site walks
    /// back within 9 of it, more than 100 heads 15 blocks toward it, and
    /// one unable to reach it for 1200 ticks gives it up.
    pub(crate) fn walk_to_job<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx, rng: &mut Rng) {
        if self.walking.walk.is_some() {
            return;
        }
        let Some(job) = self.poi(Poi::Job) else { return };
        let now = self.walking.now;
        let here = self.pos.floor().as_ivec3();
        let release = |m: &mut Mob| {
            if let Some(v) = m.villager.as_mut() {
                v.job = None;
            }
            m.walking.cant_reach_since = Some(now);
        };
        if self.walking.cant_reach_since.is_some_and(|t| now.wrapping_sub(t) > 1200) {
            release(self);
            return;
        }
        let d = manhattan(job, here);
        if d > 100 {
            let towards = job.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
            let mut found = None;
            for _ in 0..1000 {
                if let Some(p) = self.pos_towards(world, ctx, rng, 15, 7, towards, std::f64::consts::FRAC_PI_2)
                    && manhattan(p, here) <= 100
                {
                    found = Some(p);
                    break;
                }
            }
            match found {
                Some(p) => self.set_walk(Some(Walk {
                    to: WalkTo::Point(p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)),
                    speed: 0.5,
                    close: 9,
                })),
                None => release(self),
            }
        } else if d > 9 {
            self.set_walk(Some(Walk {
                to: WalkTo::Point(job.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)),
                speed: 0.5,
                close: 9,
            }));
        }
    }

    /// Track 2f: the idle and meet packages' trade gate (priority 3): a
    /// `GateBehavior` that erases `INTERACTION_TARGET` once it stops, around
    /// `TradeWithVillager`, which runs 60 ticks while that target is a
    /// villager it sees, locking both villagers' gazes and walks on each
    /// other (0.5, 2 short) when it starts and while they are within
    /// `sqrt(5)`.
    pub(crate) fn trade_gate(&mut self) {
        let partner = self
            .walking
            .interaction
            .and_then(|uid| self.head.seen.iter().find(|s| s.uid == uid && s.kind == MobKind::Villager).copied());
        if self.walking.trading_villager <= 0 {
            let Some(p) = partner else {
                // The gate starts, nothing in it does, and it stops.
                self.walking.interaction = None;
                return;
            };
            // `start` (`Behavior`'s 60-tick run: it ticks from this tick to
            // 60 later and stops the tick after): `lockGazeAndWalkToEachOther`.
            self.walking.trading_villager = 62;
            self.lock_gaze_and_walk(p);
        }
        // `tickOrStop`: `canStillUse` and the 60-tick timeout.
        let left = self.walking.trading_villager - 1;
        match partner {
            Some(p) if left > 0 => {
                if self.pos.distance_squared(p.pos) <= 5.0 {
                    self.lock_gaze_and_walk(p);
                    self.trade_throws(p);
                }
                self.walking.trading_villager = left;
            }
            _ => {
                self.walking.trading_villager = 0;
                self.walking.interaction = None;
            }
        }
    }

    /// Track 2g: `TradeWithVillager.tick`'s throws (the game's gossip pass
    /// gossips): excess food to a farmer or a villager that wants more, a
    /// farmer's wheat beyond half a stack, and the partner's requested items
    /// it doesn't request itself (only farmers request any), each
    /// `throwHalfStack`.
    fn trade_throws(&mut self, p: super::head::Seen) {
        use crate::entity::villager::Profession;
        let Some(v) = self.villager.as_ref() else { return };
        let farmer = v.profession == Profession::Farmer;
        let mut throws: Vec<fn(&Stack) -> bool> = Vec::new();
        if v.has_excess_food() && (farmer || p.wants_food) {
            throws.push(|s: &Stack| crate::entity::villager_breeding::food_points(s.item) > 0);
        }
        if farmer && v.count_item(Item::WHEAT) > 32 {
            throws.push(|s: &Stack| s.item == Item::WHEAT);
        }
        if p.farmer && !farmer {
            throws.push(|s: &Stack| crate::entity::villager_breeding::requested(Profession::Farmer, s.item));
        }
        for wanted in throws {
            let v = self.villager.as_mut().unwrap();
            let Some((i, count)) = v.inventory().iter().enumerate().find_map(|(i, s)| {
                let s = (*s).filter(wanted)?;
                if s.count > s.max() / 2 {
                    Some((i, s.count / 2))
                } else {
                    (s.count > 24).then_some((i, s.count - 24))
                }
            }) else {
                continue;
            };
            let stack = v.inventory()[i].unwrap();
            for _ in 0..count {
                v.shrink_slot(i);
            }
            // `BehaviorUtils.throwItem`: from 0.3 below the eyes, 0.3 a tick
            // toward the partner's feet, 10 ticks before pickup.
            let from = DVec3::new(self.pos.x, self.pos.y + self.eye_height() - 0.3, self.pos.z);
            let vel = (p.pos - self.pos).normalize_or_zero() * 0.3 * 20.0;
            self.walking.farm_acts.push(FarmAct::Item(Stack { count, ..stack }, from, vel));
        }
    }

    /// `BehaviorUtils.lockGazeAndWalkToEachOther(.., 0.5, 2)`: this side,
    /// and the partner's through the sense pass.
    fn lock_gaze_and_walk(&mut self, p: super::head::Seen) {
        self.head.target = Some(Tracked::Mob(p.uid, p.eye));
        self.set_walk(Some(Walk { to: WalkTo::Mob(p.uid, p.pos), speed: 0.5, close: 2 }));
        self.walking.lock_with = Some(p.uid);
    }

    /// Track 2f: `PlayTagWithOtherKids` (play package, priority 5, before
    /// its `RunOne`): with other babies in sight and no walk target, one
    /// tick in ten a baby chased by one of them runs off (a land spot in a
    /// village within 20 and 8, ten tries, speed 0.6); otherwise it chases
    /// the least-chased baby that one to five of them chase, or else the
    /// closest one (0.6, 1 short), as its `INTERACTION_TARGET`. Returns
    /// whether babies are in sight, which keeps the play `RunOne` off.
    pub(crate) fn play_tag<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx, rng: &mut Rng) -> bool {
        let babies: Vec<super::head::Seen> =
            self.head.seen.iter().filter(|s| s.kind == MobKind::Villager && s.baby).copied().collect();
        if babies.is_empty() {
            return false;
        }
        if self.walking.walk.is_some() || rng.next_int(10) != 0 {
            return true;
        }
        if babies.iter().any(|b| b.chasing == Some(self.uid)) {
            for _ in 0..10 {
                if let Some(p) = self.random_pos(world, ctx, rng, 20, 8, true)
                    && self.in_village_snapshot(p)
                {
                    self.set_walk(Some(Walk {
                        to: WalkTo::Point(p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)),
                        speed: 0.6,
                        close: 0,
                    }));
                    break;
                }
            }
            return true;
        }
        // `checkHowManyChasersEachFriendHas`, in sight order.
        let mut chased: Vec<(u32, i32)> = Vec::new();
        for b in &babies {
            if let Some(t) = b.chasing {
                match chased.iter_mut().find(|(u, _)| *u == t) {
                    Some((_, n)) => *n += 1,
                    None => chased.push((t, 1)),
                }
            }
        }
        let least = chased.iter().filter(|&&(_, n)| (1..=5).contains(&n)).min_by_key(|&&(_, n)| n).map(|&(u, _)| u);
        let kid = least.unwrap_or(babies[0].uid);
        // `chaseKid`: `INTERACTION_TARGET`, `LOOK_TARGET` (its eyes) and
        // `WALK_TARGET` (its feet, 0.6, 1 short); the sense pass follows it.
        let (pos, eye) =
            self.head.seen.iter().find(|s| s.uid == kid).map_or((self.pos, self.look_eye()), |s| (s.pos, s.eye));
        self.walking.interaction = Some(kid);
        self.head.target = Some(Tracked::Mob(kid, eye));
        self.set_walk(Some(Walk { to: WalkTo::Mob(kid, pos), speed: 0.6, close: 1 }));
        true
    }

    /// `ServerLevel.isVillage(p)` from the sections the sense pass noted
    /// around the mob: a village section within one of `p`'s.
    fn in_village_snapshot(&self, p: IVec3) -> bool {
        let s = p.div_euclid(IVec3::splat(16));
        self.walking.villages.iter().any(|v| (*v - s).abs().max_element() <= 1)
    }

    /// `AgeableMob.canBreed` for a villager: grown, fed enough and awake.
    fn villager_can_breed(&self) -> bool {
        self.age == 0 && self.villager.as_ref().is_some_and(|v| !v.sleeping && v.food_points() >= 12)
    }

    /// The sensed side of `InteractWith`'s breeding filter: a grown villager
    /// (grown, fed enough and awake at the last scan).
    fn villager_can_breed_seen(&self, s: &super::head::Seen) -> bool {
        s.kind == MobKind::Villager && s.breedable
    }

    /// `PiglinAi.doesntSeeAnyPlayerHoldingLovedItem`'s negation.
    fn find_closest_loved<W: MobWorld + ?Sized>(&self, world: &W, ctx: &Ctx) -> bool {
        self.brain_find(world, ctx, Sel::LovedItemPlayer, 16.0).is_some()
    }

    /// `Sniffer.canSniff`'s conditions the game tracks: not tempted,
    /// panicking, in water, in love, riding or leashed, and on the ground.
    fn sniffer_can_sniff(&self, ctx: &Ctx) -> bool {
        let tempted = ctx.players.iter().any(|p| {
            p.alive
                && !p.spectator
                && p.held_item == Some(crate::item::Item::TORCHFLOWER_SEEDS)
                && p.pos.distance_squared(self.pos) < 100.0
        });
        !tempted
            && self.ai != Ai::Panic
            && !self.in_water
            && self.animal.as_ref().is_none_or(|a| a.love <= 0.0)
            && self.riding.is_none()
            && self.leash.is_none()
            && self.on_ground
    }
}

/// `BlockTags.VILLAGER_BABIES_CAN_JUMP_ON_BED` (the beds).
fn jumpable<W: MobWorld + ?Sized>(world: &W, p: IVec3) -> bool {
    world.block(p).is_some_and(|b| b.is_bed_head() || b.is_bed())
}

#[cfg(test)]
mod tests;
