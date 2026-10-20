//! Java 26.3 turtle goals (`Turtle.registerGoals`), in priority order:
//!
//! | Priority | Goal | Flags | Here |
//! |---|---|---|---|
//! | 0 | `TurtlePanicGoal` (1.2) | move | [`Mob::turtle_steer`] |
//! | 1 | `TurtleBreedGoal` (1.0) | move, look | the shared `BreedGoal` |
//! | 1 | `TurtleLayEggGoal` (1.0, 16 blocks) | move, jump | `goals::Goal::LayEgg` |
//! | 2 | `TemptGoal` (1.1, seagrass) | move, look | the shared `TemptGoal` |
//! | 3 | `TurtleGoToWaterGoal` (1.0, 24 blocks) | move, jump | `goals::Goal::GoToWater` |
//! | 4 | `TurtleGoHomeGoal` (1.0) | none | [`Mob::turtle_side_goals`] |
//! | 7 | `TurtleTravelGoal` (1.0) | none | [`Mob::turtle_side_goals`] |
//! | 8 | `LookAtPlayerGoal` | look | not modelled: it only turns the head |
//! | 9 | `TurtleRandomStrollGoal` (1.0, every 100) | move | [`Mob::turtle_steer`] |
//!
//! The move goals share the one-goal selector in [`super::goals`], which is
//! exact for goals that all hold `MOVE`. Going home and travelling have no
//! flags, so they run beside whatever move goal is running: each replans
//! only when navigation is done, as in Java. They are evaluated after the
//! move goals of priority 3 or better and before the stroll, which is their
//! place in Java's registration order.
//!
//! Java saves only `home_pos` and `has_egg` (VoxelCraft's `animals::State`
//! `home` and `pregnant`); everything here is transient.
use glam::{DVec3, IVec3};

use super::selector::reduced;
use super::{Ai, JavaGoals, Mob};
use crate::entity::{Ctx, MobWorld, Rng};
use crate::world::{block::Block, terrain::SEA_LEVEL};

/// `MoveToBlockGoal`'s search and stay state.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MoveToBlock {
    next_start: i32,
    try_ticks: i32,
    max_stay: i32,
    block: IVec3,
    reached: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Goals {
    /// `Turtle.goingHome`: `TurtleGoHomeGoal` is running.
    going_home: bool,
    home_stuck: bool,
    close_to_home_ticks: i32,
    /// `Turtle.travelPos`: `TurtleTravelGoal` is running while it is set.
    travel: Option<IVec3>,
    travel_running: bool,
    travel_stuck: bool,
    water: MoveToBlock,
    nest: MoveToBlock,
    /// `layEggCounter`; `isLayingEgg` is a counter of at least one. Java
    /// doesn't reset it when the goal stops, so a turtle that wanders off
    /// mid-clutch resumes the count.
    lay_counter: i32,
    /// An egg clutch the lay goal placed this tick: the cell and egg count.
    pub(crate) laid: Option<(IVec3, u8)>,
}

/// `TurtleEggBlock.isSand`.
fn sand<W: MobWorld + ?Sized>(w: &W, p: IVec3) -> bool {
    w.block(p).is_some_and(super::navigate::is_sand)
}

/// `Blocks.WATER` (not waterlogged blocks).
fn water<W: MobWorld + ?Sized>(w: &W, p: IVec3) -> bool {
    w.block(p).is_some_and(Block::is_water)
}

impl Mob {
    fn turtle(&self) -> Option<&Goals> {
        match self.java_goals.as_deref() {
            Some(JavaGoals::Turtle(t)) => Some(t),
            _ => None,
        }
    }

    fn turtle_mut(&mut self) -> &mut Goals {
        let goals = self.java_goals.get_or_insert_with(|| Box::new(JavaGoals::Turtle(Goals::default())));
        match goals.as_mut() {
            JavaGoals::Turtle(t) => t,
            other => {
                *other = JavaGoals::Turtle(Goals::default());
                let JavaGoals::Turtle(t) = other else { unreachable!() };
                t
            }
        }
    }

    pub(crate) fn turtle_going_home(&self) -> bool {
        self.turtle().is_some_and(|t| t.going_home)
    }

    pub(crate) fn turtle_travelling(&self) -> bool {
        self.turtle().is_some_and(|t| t.travel.is_some())
    }

    /// `Turtle.hasEgg`.
    fn has_egg(&self) -> bool {
        self.animal.as_ref().is_some_and(|a| a.pregnant)
    }

    fn home(&self) -> IVec3 {
        self.animal.as_ref().map_or(self.pos.floor().as_ivec3(), |a| a.home)
    }

    /// `homePos.closerToCenterThan(position, r)`.
    fn home_within(&self, r: f64) -> bool {
        (self.home().as_dvec3() + DVec3::splat(0.5)).distance_squared(self.pos) < r * r
    }

    /// `MoveToBlockGoal.findNearestBlock` with Java's search order: layers
    /// from `start` (alternating up and down to `vertical`), then square
    /// rings out to `range`, each ring from its middle outward.
    fn find_nearest_block<W: MobWorld + ?Sized>(
        &self,
        w: &W,
        range: i32,
        start: i32,
        vertical: i32,
        valid: impl Fn(&W, IVec3) -> bool,
    ) -> Option<IVec3> {
        let base = self.pos.floor().as_ivec3();
        let next = |v: i32| if v > 0 { -v } else { 1 - v };
        let mut y = start;
        while y <= vertical {
            for r in 0..range {
                let mut x = 0;
                while x <= r {
                    let mut z = if x < r && x > -r { r } else { 0 };
                    while z <= r {
                        let p = base + IVec3::new(x, y - 1, z);
                        if self.within_home(p) && valid(w, p) {
                            return Some(p);
                        }
                        z = next(z);
                    }
                    x = next(x);
                }
            }
            y = next(y);
        }
        None
    }

    /// `MoveToBlockGoal.canUse`: every `nextStartTick` (200 to 399 ticks,
    /// counted in selector updates), search for a target.
    fn move_to_block_can_use<W: MobWorld + ?Sized>(&mut self, w: &W, rng: &mut Rng, nest: bool) -> bool {
        let t = self.turtle_mut();
        let state = if nest { &mut t.nest } else { &mut t.water };
        if state.next_start > 0 {
            state.next_start -= 1;
            return false;
        }
        state.next_start = reduced(200 + rng.next_int(200) as i32);
        let found = if nest {
            self.find_nearest_block(w, 16, 0, 1, |w, p| w.block(p + IVec3::Y) == Some(Block::AIR) && sand(w, p))
        } else {
            self.find_nearest_block(w, 24, -1, 1, water)
        };
        let t = self.turtle_mut();
        let state = if nest { &mut t.nest } else { &mut t.water };
        match found {
            Some(p) => {
                state.block = p;
                true
            }
            None => false,
        }
    }

    /// `TurtleLayEggGoal.canUse`.
    pub(crate) fn lay_egg_can_use<W: MobWorld + ?Sized>(&mut self, w: &W, rng: &mut Rng) -> bool {
        self.has_egg() && self.home_within(9.0) && self.move_to_block_can_use(w, rng, true)
    }

    /// `TurtleGoToWaterGoal.canUse`: babies on land, or adults on land that
    /// aren't heading home or carrying eggs.
    pub(crate) fn go_to_water_can_use<W: MobWorld + ?Sized>(&mut self, w: &W, rng: &mut Rng) -> bool {
        let usable =
            if self.baby { !self.in_water } else { !self.turtle_going_home() && !self.in_water && !self.has_egg() };
        usable && self.move_to_block_can_use(w, rng, false)
    }

    /// `MoveToBlockGoal.canContinueToUse` with the turtle goals' extras.
    pub(crate) fn move_to_block_can_continue<W: MobWorld + ?Sized>(&self, w: &W, nest: bool) -> bool {
        let Some(t) = self.turtle() else { return false };
        if nest {
            let s = t.nest;
            s.try_ticks >= -s.max_stay
                && s.try_ticks <= 1200
                && w.block(s.block + IVec3::Y) == Some(Block::AIR)
                && sand(w, s.block)
                && self.has_egg()
                && self.home_within(9.0)
        } else {
            !self.in_water && t.water.try_ticks <= 1200 && water(w, t.water.block)
        }
    }

    /// `MoveToBlockGoal.start`: walk to the cell above the block.
    pub(crate) fn move_to_block_start<W: MobWorld + ?Sized>(&mut self, w: &W, rng: &mut Rng, nest: bool, speed: f64) {
        let t = self.turtle_mut();
        let state = if nest { &mut t.nest } else { &mut t.water };
        let target = state.block + IVec3::Y;
        state.try_ticks = 0;
        let bound = rng.next_int(1200) + 1200;
        state.max_stay = rng.next_int(bound) as i32 + 1200;
        self.walk_to(w, target, 1, speed, false);
    }

    /// `MoveToBlockGoal.tick` (every tick), then the lay goal's egg laying.
    pub(crate) fn move_to_block_tick<W: MobWorld + ?Sized>(&mut self, w: &W, rng: &mut Rng, nest: bool, speed: f64) {
        let pos = self.pos;
        let in_water = self.in_water;
        let t = self.turtle_mut();
        let state = if nest { &mut t.nest } else { &mut t.water };
        let target = state.block + IVec3::Y;
        let close = (target.as_dvec3() + DVec3::splat(0.5)).distance_squared(pos) < 1.0;
        let mut replan = false;
        if close {
            state.reached = true;
            state.try_ticks -= 1;
        } else {
            state.reached = false;
            state.try_ticks += 1;
            replan = state.try_ticks % if nest { 40 } else { 160 } == 0;
        }
        let (block, reached) = (state.block, state.reached);
        if replan {
            self.walk_to(w, target, 1, speed, false);
        }
        if !nest || in_water || !reached {
            return;
        }
        let t = self.turtle_mut();
        if t.lay_counter < 1 {
            t.lay_counter = 1;
        } else if t.lay_counter > 200 {
            t.laid = Some((block + IVec3::Y, rng.next_int(4) as u8 + 1));
            t.lay_counter = 0;
            if let Some(a) = &mut self.animal {
                a.pregnant = false;
                // `setInLoveTime(600)`.
                a.love = 30.0;
            }
        }
        let t = self.turtle_mut();
        if t.lay_counter >= 1 {
            t.lay_counter += 1;
        }
    }

    /// `TurtleGoHomeGoal` and `TurtleTravelGoal`, which have no flags: on the
    /// full update, stop them if they can't continue, start them if they can,
    /// and tick the running ones.
    pub(crate) fn turtle_side_goals<W: MobWorld + ?Sized>(&mut self, w: &W, ctx: &Ctx, rng: &mut Rng) {
        let in_love = self.animal.as_ref().is_some_and(|a| a.love > 0.0);
        let has_egg = self.has_egg();
        // Cleanup.
        let nav_done = self.nav.is_done();
        let home_far = !self.home_within(7.0);
        let t = self.turtle_mut();
        if t.going_home && !(home_far && !t.home_stuck && t.close_to_home_ticks <= 300) {
            t.going_home = false;
        }
        if t.travel_running && !(!nav_done && !t.travel_stuck && !t.going_home && !in_love && !has_egg) {
            t.travel_running = false;
            t.travel = None;
        }
        // Start, in registration order.
        if !self.turtle_going_home()
            && !self.baby
            && (has_egg || rng.next_int(reduced(700) as u32) == 0 && !self.home_within(64.0))
        {
            let t = self.turtle_mut();
            t.going_home = true;
            t.home_stuck = false;
            t.close_to_home_ticks = 0;
        }
        let (in_water, pos) = (self.in_water, self.pos);
        let t = self.turtle_mut();
        if !t.travel_running && !t.going_home && !has_egg && in_water {
            let dx = rng.next_int(1025) as i32 - 512;
            let mut dy = rng.next_int(9) as i32 - 4;
            let dz = rng.next_int(1025) as i32 - 512;
            if dy as f64 + pos.y > (SEA_LEVEL - 1) as f64 {
                dy = 0;
            }
            t.travel = Some((pos + IVec3::new(dx, dy, dz).as_dvec3()).floor().as_ivec3());
            t.travel_running = true;
            t.travel_stuck = false;
        }
        // Tick.
        if self.turtle_going_home() {
            self.go_home_tick(w, ctx, rng);
        }
        if self.turtle().is_some_and(|t| t.travel_running) {
            self.travel_tick(w, ctx, rng);
        }
    }

    /// `TurtleGoHomeGoal.tick`.
    fn go_home_tick<W: MobWorld + ?Sized>(&mut self, w: &W, ctx: &Ctx, rng: &mut Rng) {
        let close = self.home_within(16.0);
        if close {
            self.turtle_mut().close_to_home_ticks += 1;
        }
        if !self.nav.is_done() {
            return;
        }
        let home = self.home().as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
        let mut next = self
            .pos_towards(w, ctx, rng, 16, 3, home, std::f64::consts::PI / 10.0)
            .or_else(|| self.pos_towards(w, ctx, rng, 8, 7, home, std::f64::consts::FRAC_PI_2));
        if next.is_some_and(|p| !close && !water(w, p)) {
            next = self.pos_towards(w, ctx, rng, 16, 5, home, std::f64::consts::FRAC_PI_2);
        }
        match next {
            Some(p) => {
                let s = self.goal_speed(1.0);
                self.walk_to(w, p, 1, s, false);
            }
            None => self.turtle_mut().home_stuck = true,
        }
    }

    /// `TurtleTravelGoal.tick`: a leg toward the far travel spot whenever
    /// navigation is done, within loaded chunks.
    fn travel_tick<W: MobWorld + ?Sized>(&mut self, w: &W, ctx: &Ctx, rng: &mut Rng) {
        let Some(travel) = self.turtle().and_then(|t| t.travel) else {
            self.turtle_mut().travel_stuck = true;
            return;
        };
        if !self.nav.is_done() {
            return;
        }
        let target = travel.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
        let next = self
            .pos_towards(w, ctx, rng, 16, 3, target, std::f64::consts::PI / 10.0)
            .or_else(|| self.pos_towards(w, ctx, rng, 8, 7, target, std::f64::consts::FRAC_PI_2))
            .filter(|p| {
                // `hasChunksAt(x - 34, z - 34, x + 34, z + 34)`.
                let (lo, hi) = ((p.x - 34) >> 4, (p.x + 34) >> 4);
                let (zlo, zhi) = ((p.z - 34) >> 4, (p.z + 34) >> 4);
                (lo..=hi).all(|cx| (zlo..=zhi).all(|cz| w.loaded(IVec3::new(cx * 16, p.y, cz * 16))))
            });
        match next {
            Some(p) => {
                let s = self.goal_speed(1.0);
                self.walk_to(w, p, 1, s, false);
            }
            None => self.turtle_mut().travel_stuck = true,
        }
    }

    /// Movement when no move goal of priority 3 or better runs: panic, the
    /// land stroll, then whatever path the go-home or travel goal planned.
    pub(crate) fn turtle_steer<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        w: &W,
        ctx: &Ctx,
        rng: &mut Rng,
    ) -> (Option<DVec3>, f64) {
        if let Some(wish) = self.turtle_panic(dt, w, ctx, rng) {
            return wish;
        }
        if self.ai == Ai::Panic {
            self.ai = Ai::Idle;
        }
        // `TurtleRandomStrollGoal` (`RandomStrollGoal`, every 100 ticks): on
        // land, not heading home, without eggs; it runs until its path ends.
        if self.ai == Ai::Wander && self.path_move && self.nav.is_done() {
            self.ai = Ai::Idle;
            self.path_move = false;
        }
        let full = self.goals.ticks.wrapping_sub(1).wrapping_add(self.uid).is_multiple_of(2)
            || self.goals.ticks.wrapping_sub(1) <= 1;
        if self.ai == Ai::Idle
            && full
            && self.turtle_may_stroll()
            && self.idle < 5.0
            && rng.next_int(reduced(100) as u32) == 0
            && let Some(p) = self.default_random_pos(w, ctx, rng, 10, 7)
        {
            let s = self.stroll_speed();
            if self.walk_to(w, p, 1, s, false) {
                self.ai = Ai::Wander;
                self.ai_timer = 30.0;
                self.path_move = true;
            }
        }
        if self.nav.is_done() {
            return (None, 0.0);
        }
        let s = self.nav.speed;
        (self.follow_path(w, s, false), s)
    }

    /// `TurtlePanicGoal`: run to water within 7 blocks (Manhattan, one up or
    /// down) unless standing inside a block, else to a random spot within 5;
    /// a fresh target each time the last is reached while the hit is fresh.
    fn turtle_panic<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        w: &W,
        ctx: &Ctx,
        rng: &mut Rng,
    ) -> Option<(Option<DVec3>, f64)> {
        if self.ai != Ai::Panic {
            return None;
        }
        self.ai_timer -= dt;
        if self.ai_timer <= 0.0 && self.nav.is_done() {
            self.ai = Ai::Idle;
            self.path_move = false;
            self.stop_navigation();
            return None;
        }
        self.turtle_panic_plan(w, ctx, rng);
        let s = self.panic_speed();
        Some((self.follow_path(w, s, false), s))
    }

    /// `TurtlePanicGoal.canUse`/`start`: while the hit is fresh and the last
    /// run is done, plan the next one. Runs before the flagless goals tick
    /// (priority 0 starts before they do), so a going-home turtle that is
    /// hurt runs for water rather than along a path home planned that tick.
    pub(crate) fn turtle_panic_plan<W: MobWorld + ?Sized>(&mut self, w: &W, ctx: &Ctx, rng: &mut Rng) {
        if self.ai != Ai::Panic || self.ai_timer <= 0.0 || !self.nav.is_done() {
            return;
        }
        let feet = self.pos.floor().as_ivec3();
        let open = crate::physics::collision_top(w, feet).is_none();
        let target = open
            .then(|| {
                super::navigate::manhattan_box(7, 1, 7)
                    .map(|d| feet + d)
                    .find(|&p| w.block(p).is_some_and(Block::holds_water))
            })
            .flatten()
            .or_else(|| self.default_random_pos(w, ctx, rng, 5, 4));
        if let Some(p) = target {
            let s = self.panic_speed();
            self.walk_to(w, p, 1, s, false);
        }
    }

    /// The egg clutch the lay goal placed this tick, if any.
    pub(crate) fn take_laid_eggs(&mut self) -> Option<(IVec3, u8)> {
        match self.java_goals.as_deref_mut() {
            Some(JavaGoals::Turtle(t)) => t.laid.take(),
            _ => None,
        }
    }

    /// `TurtleRandomStrollGoal.canUse`'s extra conditions.
    fn turtle_may_stroll(&self) -> bool {
        !self.in_water && !self.turtle_going_home() && !self.has_egg()
    }
}

#[cfg(test)]
mod tests {
    use glam::{DVec3, IVec3};

    use crate::entity::{Ctx, Entities, EntityEvent, MobKind, MobWorld, PlayerId, Rng, Target};
    use crate::physics::{BlockSource, test_util::Grid};
    use crate::world::{block::Block, overworld_blocks as ob, terrain::Dimension};

    /// A beach: sand from x -30 to 2 with its top at y 0, then sea from x 3
    /// to 60 (water from y -6 to 0), all across z -30..=30.
    struct Beach(Grid);
    impl BlockSource for Beach {
        fn block(&self, p: IVec3) -> Option<Block> {
            self.0.block(p)
        }
    }
    impl MobWorld for Beach {
        fn loaded(&self, _: IVec3) -> bool {
            true
        }
        fn surface(&self, _: i32, _: i32) -> Option<i32> {
            Some(0)
        }
        fn exposed(&self, p: IVec3) -> bool {
            p.y > 0
        }
    }
    fn beach() -> Beach {
        let mut g = Grid::flat(-7);
        for x in -31..=61 {
            for z in -31..=31 {
                for y in -7..=0 {
                    let b = if x == -31 || x == 61 || z == -31 || z == 31 || y == -7 {
                        Block::STONE
                    } else if x <= 2 {
                        Block::SAND
                    } else {
                        Block::WATER
                    };
                    g.set(IVec3::new(x, y, z), b);
                }
            }
        }
        Beach(g)
    }
    fn ctx() -> Ctx {
        Ctx {
            spawn_light: crate::simulation::lighting::SpawnLight::DAY,
            // A player nearby keeps `noActionTime` at zero.
            players: vec![Target { targetable: false, ..Target::new(PlayerId(1), DVec3::new(0.5, 30.0, 0.5), false) }],
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
        }
    }
    fn turtle(at: DVec3) -> Entities {
        let mut e = Entities::new(5);
        e.spawn(MobKind::Turtle, at);
        e.ensure_mob_ids();
        e
    }

    /// `TurtleTravelGoal`: in the water, a turtle picks a spot up to 512
    /// blocks away and swims there leg by leg (`TurtlePathNavigation` only
    /// accepts water while it travels).
    #[test]
    fn turtles_travel_through_open_water() {
        let w = beach();
        let start = DVec3::new(30.5, -3.0, 0.5);
        let mut e = turtle(start);
        let mut travelled = false;
        for _ in 0..1200 {
            e.update(0.05, &w, &ctx());
            travelled |= e.mobs[0].turtle_travelling();
        }
        let m = &e.mobs[0];
        assert!(travelled);
        assert!(m.pos.distance(start) > 4.0, "swam to {}", m.pos);
        assert!(w.block(m.pos.floor().as_ivec3()).is_some_and(Block::holds_water), "{}", m.pos);
    }

    /// `TurtleGoToWaterGoal`: a turtle on the beach finds water within 24
    /// blocks (searching the layer two below its feet) and walks into it.
    #[test]
    fn turtles_on_the_beach_walk_back_into_the_sea() {
        let w = beach();
        let mut e = turtle(DVec3::new(-6.5, 1.0, 0.5));
        let mut wet = false;
        for _ in 0..1200 {
            e.update(0.05, &w, &ctx());
            wet |= e.mobs[0].in_water;
            if wet {
                break;
            }
        }
        assert!(wet, "still at {}", e.mobs[0].pos);
    }

    /// `TurtleGoHomeGoal` and `TurtleLayEggGoal`: a turtle with eggs swims
    /// to its home beach, and once within 9 blocks of home lays one to four
    /// eggs on sand after 200 ticks there, then falls in love for 600 ticks.
    #[test]
    fn turtles_with_eggs_go_home_and_lay_them_on_sand() {
        let w = beach();
        let mut e = turtle(DVec3::new(14.5, -1.0, 0.5));
        {
            let a = e.mobs[0].animal.as_mut().unwrap();
            a.home = IVec3::new(-3, 1, 0);
            a.pregnant = true;
        }
        let mut laid = None;
        let mut went_home = false;
        for _ in 0..3000 {
            let events = e.update(0.05, &w, &ctx());
            went_home |= e.mobs[0].turtle_going_home();
            laid = laid.or(events.into_iter().find_map(|ev| match ev {
                EntityEvent::AnimalBlock { cell, to, .. } if ob::egg_count(to).is_some() => Some((cell, to)),
                _ => None,
            }));
            // The clutch is placed in the update that takes the eggs, so no
            // save can fall between the two.
            assert_eq!(laid.is_some(), !e.mobs[0].animal.as_ref().unwrap().pregnant);
            if laid.is_some() {
                break;
            }
        }
        let (cell, eggs) = laid.expect("laid");
        assert!(went_home);
        assert!(w.block(cell - IVec3::Y) == Some(Block::SAND) && (1..=4).contains(&ob::egg_count(eggs).unwrap()));
        assert!((cell - IVec3::new(-3, 1, 0)).as_dvec3().length() < 10.0, "laid at {cell}");
        let a = e.mobs[0].animal.as_ref().unwrap();
        assert!(!a.pregnant && a.love > 29.0);
    }

    /// `TurtlePanicGoal`: a hurt turtle runs for water within 7 blocks,
    /// at the land speed `TurtleMoveControl` halves (about 0.36 blocks a
    /// second), and keeps going until the path ends.
    #[test]
    fn hurt_turtles_run_for_the_water() {
        let w = beach();
        let mut e = turtle(DVec3::new(-2.5, 1.0, 0.5));
        e.mobs[0].damage(1.0, None, &mut Rng::new(3));
        e.update(0.05, &w, &ctx());
        assert_eq!(e.mobs[0].ai, super::Ai::Panic);
        let target = e.mobs[0].nav.target().expect("a panic path");
        assert!(w.block(target).is_some_and(Block::holds_water), "ran for {target}");
        let mut wet = false;
        for _ in 0..600 {
            e.update(0.05, &w, &ctx());
            wet |= e.mobs[0].in_water;
        }
        assert!(wet, "at {}", e.mobs[0].pos);
    }

    /// `TurtlePanicGoal` (0) starts before the flagless go-home goal ticks:
    /// a turtle hurt on its way home runs for the nearby water, not home.
    #[test]
    fn hurt_turtles_heading_home_still_run_for_water() {
        let w = beach();
        let mut e = turtle(DVec3::new(-1.5, 1.0, 0.5));
        {
            let a = e.mobs[0].animal.as_mut().unwrap();
            a.home = IVec3::new(-20, 1, 0);
            a.pregnant = true;
        }
        for _ in 0..4 {
            e.update(0.05, &w, &ctx());
        }
        assert!(e.mobs[0].turtle_going_home());
        for _ in 0..2 {
            e.mobs[0].damage(1.0, None, &mut Rng::new(3));
            e.update(0.05, &w, &ctx());
            let target = e.mobs[0].nav.target().expect("a panic path");
            assert!(w.block(target).is_some_and(Block::holds_water), "ran for {target}");
        }
    }

    /// `MoveToBlockGoal.findNearestBlock` searches its layers and rings in
    /// Java's order; `withinBoxByManhattanDistance` goes out by depth.
    #[test]
    fn block_searches_follow_javas_order() {
        let m = crate::entity::Mob::new(MobKind::Turtle, DVec3::new(0.5, 1.0, 0.5), 0.0);
        let mut g = Grid::flat(-64);
        for p in [IVec3::new(2, 0, 1), IVec3::new(-1, 0, 0), IVec3::new(0, -1, 0)] {
            g.set(p, Block::WATER);
        }
        let w = Beach(g);
        // Layer y - 1 (0) first, ring 1 before ring 2; then layer y (1).
        assert_eq!(
            m.find_nearest_block(&w, 4, 0, 1, |w, p| w.block(p) == Some(Block::WATER)),
            Some(IVec3::new(-1, 0, 0))
        );
        // The go-to-water goal starts at -1: only the layer two below.
        assert_eq!(
            m.find_nearest_block(&w, 4, -1, 1, |w, p| w.block(p) == Some(Block::WATER)),
            Some(IVec3::new(0, -1, 0))
        );
        let order: Vec<_> = super::super::navigate::manhattan_box(1, 1, 1).take(4).collect();
        assert_eq!(order, vec![IVec3::ZERO, IVec3::new(-1, 0, 0), IVec3::new(0, -1, 0), IVec3::new(0, 0, 1)]);
        // The depth runs to the sum of the reaches, so it covers the box.
        assert_eq!(super::super::navigate::manhattan_box(7, 1, 7).count(), 15 * 3 * 15);
    }
}
