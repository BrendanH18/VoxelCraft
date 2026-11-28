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

/// The option a movement `RunOne` holds running.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Hold {
    /// `DoNothing`: ticks left.
    Nothing(i32),
    JumpOnBed(BedJump),
    /// The sniffer's scent or sniff state it started.
    Sniffer,
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
    /// `TradeWithVillager` holds a villager `INTERACTION_TARGET`: ticks left.
    pub(crate) trading_villager: i32,
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
    PiglinIdle,
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
            return true;
        }
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
        match m {
            Movement::VillagerMeet => self.meet_gate(world, ctx, rng),
            Movement::VillagerIdle => self.run_one(world, ctx, rng, VILLAGER_IDLE),
            Movement::VillagerPlay => self.run_one(world, ctx, rng, VILLAGER_PLAY),
            Movement::VillagerHomeless => self.run_one(world, ctx, rng, VILLAGER_REST),
            Movement::PiglinIdle => self.run_one(world, ctx, rng, PIGLIN_IDLE),
            Movement::PiglinCelebrate => self.run_one(world, ctx, rng, PIGLIN_CELEBRATE),
            Movement::SnifferIdle => self.run_one(world, ctx, rng, SNIFFER_IDLE),
        }
    }

    /// Ends a running movement option when the activity changes.
    pub(crate) fn stop_brain_movement(&mut self) {
        if let Some(Hold::Sniffer) = self.walking.hold
            && let Some(s) = &mut self.sniffer
            && matches!(s.pose, crate::entity::sniffer::SCENTING | crate::entity::sniffer::SNIFFING)
        {
            s.stop_early();
        }
        self.walking.hold = None;
    }

    /// A `RunOne` of `options`: ticks its running option, else shuffles and
    /// starts the first that can.
    fn run_one<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx, rng: &mut Rng, options: &[(Opt, u8)]) {
        if let Some(hold) = self.walking.hold {
            if self.tick_hold(world, hold) {
                return;
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
    fn tick_hold<W: MobWorld + ?Sized>(&mut self, world: &W, hold: Hold) -> bool {
        match hold {
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
                    if self.kind == MobKind::Villager && kind == MobKind::Villager && !breed {
                        // `INTERACTION_TARGET` (a villager): `TradeWithVillager`
                        // runs its 60 ticks before it erases it.
                        self.walking.trading_villager = 60;
                    }
                }
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
                && self.walking.trading_villager <= 0
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
                self.walking.trading_villager = 60;
                self.head.target = Some(Tracked::Mob(s.uid, s.eye));
                self.head.fresh_target = true;
                self.set_walk(Some(Walk { to: WalkTo::Mob(s.uid, s.pos), speed: 0.3, close: 1 }));
            }
        }
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
