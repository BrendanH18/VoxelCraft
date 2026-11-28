//! v1.0 gap Track 2e: the fox's goals outside the shared table
//! (`Fox.registerGoals`): `FaceplantGoal` (1), `StalkPreyGoal` (5),
//! `FoxPounceGoal` and `SeekShelterGoal` (6), `FoxMeleeAttackGoal` and
//! `SleepGoal` (7), `FoxEatBerriesGoal` and `LeapAtTargetGoal` (10),
//! `FoxSearchForItemsGoal` (11)
//! and `PerchAndSearchGoal` (13), each holding `MOVE`, beside the table's
//! panic (2), breeding (3), avoiding (4), following a parent (8) and
//! strolling (11). A goal starts only over goals of a strictly worse
//! priority, and the pounce can't be interrupted.
use super::*;
use crate::entity::fox::{Prey, Running};

/// `BlockPos.closerToCenterThan(pos, d)`.
fn closer_to_centre(block: IVec3, pos: DVec3, d: f64) -> bool {
    (block.as_dvec3() + 0.5).distance_squared(pos) < d * d
}

/// `Mth.rotLerp`.
fn rot_lerp(delta: f32, from: f32, to: f32) -> f32 {
    from + delta * super::head::wrap_degrees(to - from)
}

/// `adjustedTickDelay` for a goal ticked on full updates.
fn adjusted(ticks: i32) -> i32 {
    (ticks + 1) / 2
}

/// `Fox.isPathClear`: six steps toward the target, one to three blocks up,
/// all replaceable.
pub(crate) fn path_clear<W: MobWorld + ?Sized>(world: &W, fox: DVec3, target: DVec3) -> bool {
    let (zd, xd) = (target.z - fox.z, target.x - fox.x);
    let slope = zd / xd;
    for i in 0..6 {
        let f = f64::from(i as f32 / 6.0);
        let z = if slope == 0.0 { 0.0 } else { zd * f };
        let x = if slope == 0.0 { xd * f } else { z / slope };
        for j in 1..4 {
            let p = DVec3::new(fox.x + x, fox.y + f64::from(j), fox.z + z).floor().as_ivec3();
            if !world.block(p).is_some_and(Block::is_replaceable) {
                return false;
            }
        }
    }
    true
}

impl Mob {
    fn fox_asleep(&self) -> bool {
        self.animal.as_ref().is_some_and(|a| a.sleeping)
    }

    fn set_fox_asleep(&mut self, sleeping: bool) {
        if let Some(a) = &mut self.animal {
            a.sleeping = sleeping;
        }
    }

    /// v1.0 gap Track 2f: `Fox.getAmbientSound`'s own calls: a sleeping
    /// fox's breathing (`FOX_SLEEP`), and at night, one call in ten with no
    /// player within 16 blocks of its box, the screech (`FOX_SCREECH`,
    /// played at volume 2). `None` is its usual `FOX_AMBIENT`.
    pub(crate) fn fox_ambient(&self, ctx: &Ctx, rng: &mut Rng) -> Option<MobSound> {
        use crate::entity::fox::Cue;
        self.fox.as_ref()?;
        if self.fox_asleep() {
            return Some(MobSound::Fox(Cue::Sleep));
        }
        if ctx.daylight <= 0.5 && rng.next_f32() < 0.1 {
            let (lo, hi) = self.aabb();
            let (lo, hi) = (lo - DVec3::splat(16.0), hi + DVec3::splat(16.0));
            let near = ctx.players.iter().any(|t| {
                !t.spectator
                    && t.pos.cmpge(lo - DVec3::new(0.3, 1.8, 0.3)).all()
                    && t.pos.cmple(hi + DVec3::new(0.3, 0.0, 0.3)).all()
            });
            if !near {
                return Some(MobSound::Fox(Cue::Screech));
            }
        }
        None
    }

    /// `Fox.clearStates`.
    pub(crate) fn fox_clear_states(&mut self) {
        if let Some(s) = &mut self.fox {
            s.clear();
        }
        self.set_fox_asleep(false);
    }

    fn fox_target(&self) -> Option<Prey> {
        self.fox.as_ref()?.target
    }

    /// The `MOVE` holder of the fox's goals, for the shared table: the
    /// running goal's priority (0 for the pounce, which nothing interrupts).
    pub(crate) fn fox_move_priority(&self) -> Option<u8> {
        let r = self.fox.as_ref()?.running?;
        Some(if r.interruptable() { r.priority() } else { 0 })
    }

    /// The fox's `tick`, `aiStep` and its goals that outrank the shared table
    /// (the faceplant and a running pounce), before the table runs.
    pub(crate) fn fox_pre_think<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        rng: &mut Rng,
        full: bool,
        events: &mut Vec<EntityEvent>,
    ) -> Option<(Option<DVec3>, f64)> {
        let in_water = self.in_water;
        let at = self.pos + DVec3::Y * (self.shape().height * 0.8);
        let cell = self.pos.floor().as_ivec3();
        let s = self.fox.as_mut()?;
        s.ticks = s.ticks.wrapping_add(1);
        s.animate();
        // v1.0 gap Track 2f: `aiStep`: one tick in 20 a defending fox
        // snarls; `tick`: a faceplanted one kicks up the block it is in
        // (level event 2001) one tick in five.
        if s.defending && rng.next_f32() < 0.05 {
            events.push(EntityEvent::Sound { sound: MobSound::Fox(crate::entity::fox::Cue::Aggro), pos: at });
        }
        if s.faceplanted && rng.next_f32() < 0.2 {
            events.push(EntityEvent::DestroyEffect { cell });
        }
        // `aiStep`: no living target ends the crouch and the interest.
        if s.target.is_none() {
            s.crouching = false;
            s.interested = false;
        }
        // `FoxFloatGoal.start` clears the states.
        if in_water && s.running != Some(Running::Pounce) {
            s.clear();
        }
        // `tick`: water, a target or thunder wakes it; water or sleep ends
        // sitting.
        let woken = in_water || s.target.is_some() || s.thundering;
        if woken {
            self.set_fox_asleep(false);
        }
        if in_water || self.fox_asleep() {
            self.fox.as_mut().unwrap().sitting = false;
        }
        let s = self.fox.as_ref().unwrap();
        // `travelInAir`'s 0.91 a tick on a leap's or pounce's flight, which
        // the game's air steering leaves undamped.
        if matches!(s.running, Some(Running::Pounce | Running::Leap)) && !self.on_ground {
            self.vel.x *= 0.91;
            self.vel.z *= 0.91;
        }
        match s.running {
            Some(Running::Pounce) => {
                if full && !self.pounce_continues() {
                    self.fox_stop(world);
                } else {
                    if full {
                        self.pounce_tick(world, events);
                    }
                    self.stop_goal(world);
                    return Some((None, 0.0));
                }
            }
            Some(Running::Faceplant { countdown }) => {
                if full {
                    let left = countdown - 1;
                    if !s.faceplanted || countdown <= 0 {
                        self.fox_stop(world);
                    } else {
                        self.fox.as_mut().unwrap().running = Some(Running::Faceplant { countdown: left });
                    }
                }
                if self.fox.as_ref().unwrap().running.is_some() {
                    self.stop_goal(world);
                    self.stop_navigation();
                    self.hold_look();
                    return Some((None, 0.0));
                }
            }
            _ => {}
        }
        // `FaceplantGoal.canUse` (priority 1 takes MOVE from everything).
        if full && self.fox.as_ref().unwrap().faceplanted {
            self.fox_stop(world);
            self.stop_goal(world);
            self.stop_navigation();
            self.fox.as_mut().unwrap().running = Some(Running::Faceplant { countdown: adjusted(40) });
            self.hold_look();
            return Some((None, 0.0));
        }
        None
    }

    /// The fox's goals at priority 5 and worse, after the shared table ran:
    /// `Some` is this tick's movement from one of them.
    pub(super) fn fox_think<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        full: bool,
        events: &mut Vec<EntityEvent>,
    ) -> Option<(Option<DVec3>, f64)> {
        self.fox.as_ref()?;
        let table = self.table_move_priority();
        // A better table goal (or panic) took `MOVE`: the running goal stops.
        if let Some(r) = self.fox.as_ref().unwrap().running
            && table.is_some_and(|p| p < r.priority())
        {
            self.fox_stop(world);
        }
        if full {
            if let Some(r) = self.fox.as_ref().unwrap().running
                && !self.fox_continues(world, ctx, rng, r)
            {
                self.fox_stop(world);
            }
            let held = self.fox.as_ref().unwrap().running.map(|r| r.priority());
            let blocker = held.into_iter().chain(table).min();
            for (priority, goal) in CANDIDATES {
                if blocker.is_some_and(|b| b <= *priority) {
                    break;
                }
                if let Some(r) = self.fox_can_use(world, ctx, rng, *goal) {
                    self.fox_stop(world);
                    self.stop_goal(world);
                    self.stop_navigation();
                    if self.ai == Ai::Wander {
                        self.ai = Ai::Idle;
                        self.path_move = false;
                    }
                    self.fox_start(world, ctx, rng, r);
                    break;
                }
            }
        }
        let r = self.fox.as_ref().unwrap().running?;
        Some(self.fox_tick(world, ctx, rng, full, r, events))
    }

    /// The priority of the shared table's goal holding `MOVE` (a running
    /// table goal, panic or a stroll).
    fn table_move_priority(&self) -> Option<u8> {
        self.table_holder().map(|(p, _)| p)
    }

    fn fox_can_use<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        goal: Candidate,
    ) -> Option<Running> {
        let asleep = self.fox_asleep();
        let target = self.fox_target();
        let s = self.fox.as_ref().unwrap();
        match goal {
            Candidate::Stalk => {
                let t = target?;
                (!asleep
                    && matches!(t.kind, MobKind::Chicken | MobKind::Rabbit)
                    && self.pos.distance_squared(t.pos) > 36.0
                    && !s.crouching
                    && !s.interested
                    && !self.jump)
                    .then_some(Running::Stalk)
            }
            Candidate::Pounce => {
                if !s.fully_crouched() {
                    return None;
                }
                let t = target?;
                // `getMotionDirection` is `getDirection` for every animal.
                if path_clear(world, self.pos, t.pos) {
                    Some(Running::Pounce)
                } else {
                    let _ = self.walk_to(world, t.pos.floor().as_ivec3(), 0, 0.0, true);
                    self.nav.stop();
                    let s = self.fox.as_mut().unwrap();
                    s.crouching = false;
                    s.interested = false;
                    None
                }
            }
            Candidate::Shelter => {
                if asleep || target.is_some() {
                    return None;
                }
                let s = self.fox.as_mut().unwrap();
                let here = self.pos.floor().as_ivec3();
                // v1.0 gap Track 2f: under open sky in a thunderstorm it
                // looks for shelter at once, without the interval.
                let storm = s.thundering && world.exposed(here);
                if !storm {
                    let interval = s.shelter_interval.get_or_insert(selector::reduced(100));
                    if *interval > 0 {
                        *interval -= 1;
                        return None;
                    }
                    *interval = 100;
                    if ctx.daylight <= 0.5 || !world.exposed(here) || s.in_village {
                        return None;
                    }
                }
                // `FleeSunGoal.getHidePos`: ten tries for a dark spot under cover.
                for _ in 0..10 {
                    let p = here
                        + IVec3::new(
                            rng.next_int(20) as i32 - 10,
                            rng.next_int(6) as i32 - 3,
                            rng.next_int(20) as i32 - 10,
                        );
                    if !world.exposed(p) && self.walk_value(world, ctx, p) < 0.0 {
                        self.fox.as_mut().unwrap().shelter_at = Some(p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
                        return Some(Running::Shelter);
                    }
                }
                None
            }
            Candidate::Melee => {
                if s.sitting || asleep || s.crouching || s.faceplanted {
                    return None;
                }
                // `MeleeAttackGoal.canUse` checks at most every 20 ticks.
                let now = s.ticks;
                if s.melee_check.is_some_and(|t| now.wrapping_sub(t) < 20) {
                    return None;
                }
                self.fox.as_mut().unwrap().melee_check = Some(now);
                let t = target?;
                let speed = self.goal_speed(1.2);
                let reaches = self.walk_to(world, t.pos.floor().as_ivec3(), 0, speed, true);
                (reaches || self.fox_in_reach(t)).then_some(Running::Melee { repath: 0, attack: 0 })
            }
            Candidate::Sleep => {
                // `xxa == yya == zza == 0`: it didn't move last tick.
                if !s.still {
                    return None;
                }
                (self.fox_can_sleep(world, ctx, rng) || asleep).then_some(Running::Sleep)
            }
            Candidate::VillageStroll => {
                // `StrollThroughVillageGoal.canUse` (its roll and spot from the
                // entity pass) and `canFoxMove`.
                let wanted = self.fox.as_mut().unwrap().village_stroll.take()?;
                let s = self.fox.as_ref().unwrap();
                (!asleep && !s.sitting && !s.defending && target.is_none() && self.riding.is_none())
                    .then_some(Running::VillageStroll { wanted })
            }
            Candidate::Berries => {
                if asleep {
                    return None;
                }
                // `MoveToBlockGoal.canUse`.
                let s = self.fox.as_mut().unwrap();
                if s.berries_next > 0 {
                    s.berries_next -= 1;
                    return None;
                }
                s.berries_next = selector::reduced(200 + rng.next_int(200) as i32);
                let block = self.nearest_berries(world)?;
                Some(Running::Berries { block, tries: 0, stay: 0, waited: 0 })
            }
            Candidate::Leap => {
                let t = target?;
                let d = self.pos.distance_squared(t.pos);
                ((4.0..=16.0).contains(&d)
                    && self.on_ground
                    && self.riding.is_none()
                    && rng.next_int(selector::reduced(5) as u32) == 0)
                    .then_some(Running::Leap)
            }
            Candidate::Items => {
                let mouth = self.animal.as_ref().is_some_and(|a| a.mouth.is_some());
                if mouth || target.is_some() || self.hurt_age < 100 || !s.can_move(asleep) {
                    return None;
                }
                if rng.next_int(selector::reduced(10) as u32) != 0 {
                    return None;
                }
                s.item_near.map(|_| Running::Items)
            }
            Candidate::Perch => {
                let ok = self.hurt_age >= 100
                    && rng.next_f32() < 0.02
                    && !asleep
                    && target.is_none()
                    && self.nav.is_done()
                    && !s.alertable
                    && !s.pouncing
                    && !s.crouching;
                ok.then_some(Running::Perch { rel: (0.0, 0.0), look: 0, looks: 0 })
            }
        }
    }

    /// `SleepGoal.canSleep`: after its countdown, in daylight under cover
    /// with nothing alerting it and out of powder snow.
    fn fox_can_sleep<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx, rng: &mut Rng) -> bool {
        let s = self.fox.as_mut().unwrap();
        let countdown = s.sleep_countdown.get_or_insert_with(|| rng.next_int(selector::reduced(140) as u32) as i32);
        if *countdown > 0 {
            *countdown -= 1;
            return false;
        }
        let alertable = s.alertable;
        // `hasShelter`: no sky over the top of its box, and a walk value
        // of at least 0 there.
        let top = DVec3::new(self.pos.x, self.pos.y + self.shape().height, self.pos.z).floor().as_ivec3();
        let shelter = !world.exposed(top) && self.walk_value(world, ctx, top) >= 0.0;
        let powder = world.block(self.pos.floor().as_ivec3()) == Some(crate::world::scaffolding::POWDER_SNOW);
        ctx.daylight > 0.5 && shelter && !alertable && !powder
    }

    fn fox_in_reach(&self, t: Prey) -> bool {
        // `Mob.isWithinMeleeAttackRange`: the box inflated by
        // `DEFAULT_ATTACK_REACH` (sqrt(2.04) - 0.6) across the target's box.
        let reach = 2.04_f64.sqrt() - 0.6;
        let (lo, hi) = self.aabb();
        let (lo, hi) = (lo - DVec3::new(reach, 0.0, reach), hi + DVec3::new(reach, 0.0, reach));
        let (tl, th) = (
            t.pos - DVec3::new(t.half_width, 0.0, t.half_width),
            t.pos + DVec3::new(t.half_width, t.height, t.half_width),
        );
        lo.x < th.x && hi.x > tl.x && lo.y < th.y && hi.y > tl.y && lo.z < th.z && hi.z > tl.z
    }

    fn fox_continues<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx, rng: &mut Rng, r: Running) -> bool {
        let target = self.fox_target();
        match r {
            // `canContinueToUse` is `canUse`.
            Running::Stalk => self.fox_can_use(world, ctx, rng, Candidate::Stalk).is_some(),
            Running::Items => self.fox_can_use(world, ctx, rng, Candidate::Items).is_some(),
            Running::Shelter => !self.nav.is_done(),
            Running::Melee { .. } => target.is_some(),
            Running::Sleep => self.fox_can_sleep(world, ctx, rng),
            Running::Leap => !self.on_ground,
            Running::Berries { block, tries, stay, .. } => {
                tries >= -stay && tries <= 1200 && world.block(block).is_some_and(ripe_berries)
            }
            Running::Perch { looks, .. } => looks > 0,
            // Java's goal ends at its next update, but leaves the path it
            // set walking; here it holds while that path lasts.
            Running::VillageStroll { .. } => {
                let s = self.fox.as_ref().unwrap();
                !self.nav.is_done() && !self.fox_asleep() && !s.sitting && !s.defending && s.target.is_none()
            }
            Running::Faceplant { .. } | Running::Pounce => true,
        }
    }

    /// `FoxPounceGoal.canContinueToUse`.
    fn pounce_continues(&self) -> bool {
        let Some(s) = &self.fox else { return false };
        if s.target.is_none() {
            return false;
        }
        let yd = self.vel.y * f64::from(crate::entity::motion::TICK);
        let pitch = self.look.pitch;
        (yd * yd >= f64::from(0.05_f32) || pitch.abs() >= 15.0 || !self.on_ground) && !s.faceplanted
    }

    fn fox_start<W: MobWorld + ?Sized>(&mut self, world: &W, ctx: &Ctx, rng: &mut Rng, r: Running) {
        let target = self.fox_target();
        let mut r = r;
        match r {
            Running::Stalk => {
                let s = self.fox.as_mut().unwrap();
                s.sitting = false;
                s.faceplanted = false;
            }
            Running::Pounce => {
                let s = self.fox.as_mut().unwrap();
                s.pouncing = true;
                s.interested = false;
                if let Some(t) = target {
                    self.set_look_at(t.pos + DVec3::Y * t.eye, 60.0, 30.0);
                    let uv = (t.pos - self.pos).normalize_or_zero();
                    self.vel += DVec3::new(uv.x * 0.8, 0.9, uv.z * 0.8) / f64::from(crate::entity::motion::TICK);
                    self.on_ground = false;
                }
                self.stop_navigation();
            }
            Running::Shelter => {
                self.fox_clear_states();
                if let Some(at) = self.fox.as_ref().unwrap().shelter_at {
                    let speed = self.goal_speed(1.25);
                    self.walk_to(world, at.floor().as_ivec3(), 0, speed, false);
                }
            }
            Running::Melee { .. } => {
                self.fox.as_mut().unwrap().interested = false;
                self.pathed = None;
                self.repath_in = 0;
            }
            Running::Sleep => {
                let s = self.fox.as_mut().unwrap();
                s.sitting = false;
                s.crouching = false;
                s.interested = false;
                self.jump = false;
                self.set_fox_asleep(true);
                self.stop_navigation();
            }
            Running::Leap => {
                if let Some(t) = target {
                    // `LeapAtTargetGoal.start`.
                    let tick = f64::from(crate::entity::motion::TICK);
                    let mut v = DVec3::new(t.pos.x - self.pos.x, 0.0, t.pos.z - self.pos.z);
                    let dm = self.vel * tick;
                    if v.length_squared() > 1.0e-7 {
                        v = v.normalize() * 0.4 + dm * 0.2;
                    }
                    self.vel = DVec3::new(v.x, 0.4, v.z) / tick;
                    self.on_ground = false;
                }
            }
            Running::Berries { block, .. } => {
                // `start`: no longer sitting; `moveMobToBlock`.
                self.fox.as_mut().unwrap().sitting = false;
                let stay = {
                    let bound = rng.next_int(1200) + 1200;
                    rng.next_int(bound) as i32 + 1200
                };
                let speed = self.goal_speed(1.2);
                self.walk_to(world, block + IVec3::Y, 1, speed, false);
                r = Running::Berries { block, tries: 0, stay, waited: 0 };
            }
            Running::Items => {
                if let Some(at) = self.fox.as_ref().unwrap().item_near {
                    let speed = self.goal_speed(1.2);
                    self.walk_to(world, at.floor().as_ivec3(), 0, speed, false);
                }
            }
            Running::VillageStroll { wanted } => {
                // `start`: `clearStates`; the first `tick` sets the path.
                self.fox_clear_states();
                self.village_stroll_tick(world, rng, wanted);
            }
            Running::Perch { .. } => {
                let rel = perch_look(rng);
                let looks = 2 + rng.next_int(3) as i32;
                r = Running::Perch { rel: rel.0, look: rel.1, looks };
                self.fox.as_mut().unwrap().sitting = true;
                self.stop_navigation();
            }
            Running::Faceplant { .. } => {}
        }
        let _ = ctx;
        self.fox.as_mut().unwrap().running = Some(r);
    }

    /// `Goal.stop` for the running fox goal.
    fn fox_stop<W: MobWorld + ?Sized>(&mut self, world: &W) {
        let Some(r) = self.fox.as_mut().and_then(|s| s.running.take()) else { return };
        let target = self.fox_target();
        match r {
            Running::Faceplant { .. } => self.fox.as_mut().unwrap().faceplanted = false,
            Running::Stalk => {
                let clear = target.is_some_and(|t| path_clear(world, self.pos, t.pos));
                let s = self.fox.as_mut().unwrap();
                s.interested = clear;
                s.crouching = clear;
                if let Some(t) = target.filter(|_| clear) {
                    self.stop_navigation();
                    let (y, x) = (self.max_head_y_rot(), self.max_head_x_rot());
                    self.set_look_at(t.pos + DVec3::Y * t.eye, y, x);
                }
            }
            Running::Pounce => {
                let s = self.fox.as_mut().unwrap();
                s.crouching = false;
                s.crouch_amount = 0.0;
                s.crouch_amount_o = 0.0;
                s.interested = false;
                s.pouncing = false;
            }
            Running::Sleep => {
                let s = self.fox.as_mut().unwrap();
                s.sleep_countdown = None;
                self.fox_clear_states();
            }
            Running::Perch { .. } => self.fox.as_mut().unwrap().sitting = false,
            Running::Melee { .. } | Running::Shelter | Running::Items | Running::Berries { .. } => {
                self.stop_navigation()
            }
            Running::Leap | Running::VillageStroll { .. } => {}
        }
    }

    fn fox_tick<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        full: bool,
        r: Running,
        events: &mut Vec<EntityEvent>,
    ) -> (Option<DVec3>, f64) {
        let target = self.fox_target();
        match r {
            Running::Stalk => {
                let Some(t) = target else { return (None, 0.0) };
                let speed = self.goal_speed(1.5);
                if full {
                    let (y, x) = (self.max_head_y_rot(), self.max_head_x_rot());
                    self.set_look_at(t.pos + DVec3::Y * t.eye, y, x);
                    if self.pos.distance_squared(t.pos) <= 36.0 {
                        let s = self.fox.as_mut().unwrap();
                        s.interested = true;
                        s.crouching = true;
                        self.stop_navigation();
                        return (None, 0.0);
                    }
                    // `getNavigation().moveTo(target, 1.5)` plans again
                    // when the target moved.
                    if self.nav.is_done() || self.pathed.is_none_or(|p| p.distance_squared(t.pos) >= 1.0) {
                        self.pathed = Some(t.pos);
                        self.walk_to(world, t.pos.floor().as_ivec3(), 0, speed, true);
                    }
                } else {
                    self.hold_look();
                }
                (self.follow_path(world, speed, true), speed)
            }
            Running::VillageStroll { .. } => {
                let speed = self.goal_speed(1.0);
                match self.follow_path(world, speed, false) {
                    Some(dir) => (Some(dir), speed),
                    None => (None, 0.0),
                }
            }
            Running::Shelter | Running::Items => {
                let speed = self.goal_speed(if r == Running::Shelter { 1.25 } else { 1.2 });
                if r == Running::Items
                    && full
                    && let Some(at) = self.fox.as_ref().unwrap().item_near
                    && self.animal.as_ref().is_some_and(|a| a.mouth.is_none())
                {
                    self.walk_to(world, at.floor().as_ivec3(), 0, speed, false);
                }
                match self.follow_path(world, speed, false) {
                    Some(dir) => (Some(dir), speed),
                    None => (None, 0.0),
                }
            }
            Running::Melee { attack, .. } => {
                let Some(t) = target else { return (None, 0.0) };
                // `MeleeAttackGoal.tick` (every tick): look, chase, bite.
                let eye = t.pos + DVec3::Y * t.eye;
                self.set_look_at(eye, super::head::ATTACK_LOOK, super::head::ATTACK_LOOK);
                let speed = self.goal_speed(1.2);
                let sees = line_of_sight(world, self.look_eye(), eye);
                let wish = self.pursue(world, t.pos, speed, sees, true, rng);
                let attack = (attack - 1).max(0);
                let attack = if attack <= 0 && sees && self.fox_in_reach(t) {
                    self.fox_bite(t, events);
                    // v1.0 gap Track 2f: `FOX_BITE`.
                    let at = self.pos + DVec3::Y * (self.shape().height * 0.8);
                    events.push(EntityEvent::Sound { sound: MobSound::Fox(crate::entity::fox::Cue::Bite), pos: at });
                    20
                } else {
                    attack
                };
                self.fox.as_mut().unwrap().running = Some(Running::Melee { repath: 0, attack });
                (wish, speed)
            }
            Running::Sleep => {
                self.hold_look();
                (None, 0.0)
            }
            Running::Leap => (None, 0.0),
            Running::Berries { block, tries, stay, waited } => {
                // `FoxEatBerriesGoal.tick` and `MoveToBlockGoal.tick` (every tick).
                let speed = self.goal_speed(1.2);
                let above = block + IVec3::Y;
                let reached = (above.as_dvec3() + 0.5).distance_squared(self.pos) < 4.0;
                let mut waited = waited;
                if reached {
                    if waited >= 40 {
                        self.eat_berries(world, block, rng, events);
                    } else {
                        waited += 1;
                    }
                } else if rng.next_f32() < 0.05 {
                    // v1.0 gap Track 2f: `FOX_SNIFF` on the way.
                    let at = self.pos + DVec3::Y * (self.shape().height * 0.8);
                    events.push(EntityEvent::Sound { sound: MobSound::Fox(crate::entity::fox::Cue::Sniff), pos: at });
                }
                let tries = if reached { tries - 1 } else { tries + 1 };
                if !reached && tries % 100 == 0 {
                    self.walk_to(world, above, 1, speed, false);
                }
                self.fox.as_mut().unwrap().running = Some(Running::Berries { block, tries, stay, waited });
                if reached {
                    return (None, 0.0);
                }
                (self.follow_path(world, speed, false), speed)
            }
            Running::Perch { rel, look, looks } => {
                if full {
                    let (mut rel, mut look, mut looks) = (rel, look - 1, looks);
                    if look <= 0 {
                        looks -= 1;
                        (rel, look) = perch_look(rng);
                    }
                    self.fox.as_mut().unwrap().running = Some(Running::Perch { rel, look, looks });
                    let at = DVec3::new(self.pos.x + rel.0, self.look_eye().y, self.pos.z + rel.1);
                    let (y, x) = (self.max_head_y_rot(), self.max_head_x_rot());
                    self.set_look_at(at, y, x);
                } else {
                    self.hold_look();
                }
                (None, 0.0)
            }
            Running::Pounce => {
                // A pounce that started this update ticks with it.
                if full {
                    self.pounce_tick(world, events);
                }
                (None, 0.0)
            }
            Running::Faceplant { .. } => {
                let _ = ctx;
                (None, 0.0)
            }
        }
    }

    /// `FoxPounceGoal.tick`: look at the target, tilt with the leap, bite
    /// within 2 blocks, or land face first in snow.
    fn pounce_tick<W: MobWorld + ?Sized>(&mut self, world: &W, events: &mut Vec<EntityEvent>) {
        let target = self.fox_target();
        if let Some(t) = target {
            self.set_look_at(t.pos + DVec3::Y * t.eye, 60.0, 30.0);
        }
        let dm = self.vel * f64::from(crate::entity::motion::TICK);
        if !self.fox.as_ref().unwrap().faceplanted {
            if dm.y * dm.y < f64::from(0.03_f32) && self.look.pitch != 0.0 {
                self.look.pitch = rot_lerp(0.2, self.look.pitch, 0.0);
            } else {
                let h = dm.with_y(0.0).length();
                // `jumping` is false by the goal's tick (`JumpControl`).
                let biased = dm.y;
                let len = (h * h + biased * biased).sqrt();
                if len > f64::from(1.0e-5_f32) {
                    let rotation = (-biased).signum() * (h / len).acos() * 180.0 / std::f64::consts::PI;
                    self.look.pitch = rotation as f32;
                }
            }
        }
        if let Some(t) = target
            && self.pos.distance(t.pos) <= 2.0
        {
            self.fox_bite(t, events);
        } else if self.look.pitch > 0.0
            && self.on_ground
            && dm.y as f32 != 0.0
            && world
                .block(self.pos.floor().as_ivec3())
                .is_some_and(|b| crate::world::overworld_blocks::snow_layers(b).is_some())
        {
            self.look.pitch = 60.0;
            let s = self.fox.as_mut().unwrap();
            s.target = None;
            s.faceplanted = true;
        }
    }

    /// v1.0 gap Track 2f: `StrollThroughVillageGoal.tick`: with no path and
    /// `wantedPos` more than 10 away, a path at speed 1 to the surface 10
    /// blocks toward a point 60% of the way there, else to a random surface
    /// spot within 8 (`moveRandomly`).
    fn village_stroll_tick<W: MobWorld + ?Sized>(&mut self, world: &W, rng: &mut Rng, wanted: IVec3) {
        if !self.nav.is_done() || closer_to_centre(wanted, self.pos, 10.0) {
            return;
        }
        let speed = self.goal_speed(1.0);
        let far = wanted.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
        let mid = (self.pos - far) * 0.4 + far;
        let to = (mid - self.pos).normalize_or_zero() * 10.0 + self.pos;
        let surface = |x: i32, z: i32| world.surface(x, z).map(|y| IVec3::new(x, y + 1, z));
        let to = to.floor().as_ivec3();
        let reaches =
            surface(to.x, to.z).is_some_and(|p| self.walk_to(world, p, 0, speed, false) || self.nav.path.is_some());
        if !reaches {
            let here = self.pos.floor().as_ivec3();
            let (dx, dz) = (rng.next_int(16) as i32 - 8, rng.next_int(16) as i32 - 8);
            if let Some(p) = surface(here.x + dx, here.z + dz) {
                self.walk_to(world, p, 0, speed, false);
            }
        }
    }

    /// `MoveToBlockGoal.findNearestBlock` (search range 12, vertical 1):
    /// rings outward from the fox, one block down, level, then two down.
    fn nearest_berries<W: MobWorld + ?Sized>(&self, world: &W) -> Option<IVec3> {
        let at = self.pos.floor().as_ivec3();
        let next = |v: i32| if v > 0 { -v } else { 1 - v };
        let mut k = 0;
        while k <= 1 {
            for l in 0..12 {
                let mut m = 0;
                while m <= l {
                    let mut n = if m < l && m > -l { l } else { 0 };
                    while n <= l {
                        let p = at + IVec3::new(m, k - 1, n);
                        if self.within_home(p) && world.block(p).is_some_and(ripe_berries) {
                            return Some(p);
                        }
                        n = next(n);
                    }
                    m = next(m);
                }
            }
            k = next(k);
        }
        None
    }

    /// `FoxEatBerriesGoal.onReachedTarget` (with `mobGriefing`): a ripe bush
    /// gives one to three berries (one more at age 3), the first into an
    /// empty mouth, and goes back to age 1; glow berries drop off the vine.
    fn eat_berries<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        cell: IVec3,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) {
        use crate::{inventory::Stack, item::Item, world::overworld_blocks as ob};
        if !self.griefing {
            return;
        }
        let Some(b) = world.block(cell) else { return };
        let pos = cell.as_dvec3() + 0.5;
        if let Some(age) = ob::berry_age(b).filter(|&a| a >= 2) {
            let mut count = 1 + rng.next_int(2) as u8 + u8::from(age == 3);
            if let Some(a) = self.animal.as_mut().filter(|a| a.mouth.is_none()) {
                a.mouth = Some(Stack::new(Item::SWEET_BERRIES, 1));
                a.mouth_time = 0.0;
                count -= 1;
            }
            if count > 0 {
                events.push(EntityEvent::DropEquipment { stack: Stack::new(Item::SWEET_BERRIES, count), pos });
            }
            events.push(EntityEvent::AnimalBlock { cell, from: b, to: ob::berry_bush(1) });
        } else if glow_berries(b) {
            events.push(EntityEvent::DropEquipment { stack: Stack::new(Item::GLOW_BERRIES, 1), pos });
            events.push(EntityEvent::AnimalBlock { cell, from: b, to: Block(b.0 - 1) });
        }
    }

    /// `doHurtTarget` with the fox's 2 attack damage.
    fn fox_bite(&mut self, t: Prey, events: &mut Vec<EntityEvent>) {
        let flat = (t.pos - self.pos).with_y(0.0);
        self.attack_anim = 0.3;
        if let Some(player) = t.player {
            // v1.0 gap Track 2g: biting the player it defends against
            // (`Mob.doHurtTarget`: 2 damage, Java's melee knockback).
            let dir = flat.normalize_or(DVec3::X);
            let yaw = (-dir.x).atan2(dir.z).to_degrees() as f32;
            let extra = crate::knockback::attack_strength(self.attack_knockback(), 0, false);
            events.push(EntityEvent::PlayerHit {
                java: Some(crate::knockback::Hit::melee(self.pos, t.pos, yaw, extra)),
                player,
                damage: 2.0,
                knockback: (dir * 6.0 + DVec3::Y * 5.0).as_vec3(),
                cause: self.melee_attack().1,
            });
            events.push(EntityEvent::OwnerHurt { player, attacker: self.uid });
            return;
        }
        events.push(EntityEvent::MobHit {
            target: t.uid,
            attacker: self.uid,
            damage: 2.0,
            knockback: flat.normalize_or_zero() * 8.0,
            melee: true,
        });
    }
}

/// `CaveVines.hasGlowBerries`: the two cave vine states with berries.
fn glow_berries(b: Block) -> bool {
    use crate::world::overworld_blocks as ob;
    b == ob::CAVE_VINES_LIT || b == ob::CAVE_VINES_PLANT_LIT
}

/// `FoxEatBerriesGoal.isValidTarget`: a sweet berry bush of age 2 or more,
/// or a cave vine with glow berries.
fn ripe_berries(b: Block) -> bool {
    crate::world::overworld_blocks::berry_age(b).is_some_and(|a| a >= 2) || glow_berries(b)
}

/// `PerchAndSearchGoal.resetLook`: a random direction and
/// `adjustedTickDelay(80 + nextInt(20))` full updates.
fn perch_look(rng: &mut Rng) -> ((f64, f64), i32) {
    let a = std::f64::consts::TAU * f64::from(rng.next_f32());
    ((a.cos(), a.sin()), adjusted(80 + rng.next_int(20) as i32))
}

/// The fox goals the selector tries, in priority order (the faceplant
/// runs before the table).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Candidate {
    Stalk,
    Pounce,
    Shelter,
    Melee,
    Sleep,
    VillageStroll,
    Berries,
    Leap,
    Items,
    Perch,
}

const CANDIDATES: &[(u8, Candidate)] = &[
    (5, Candidate::Stalk),
    (6, Candidate::Pounce),
    (6, Candidate::Shelter),
    (7, Candidate::Melee),
    (7, Candidate::Sleep),
    (9, Candidate::VillageStroll),
    (10, Candidate::Berries),
    (10, Candidate::Leap),
    (11, Candidate::Items),
    (13, Candidate::Perch),
];

#[cfg(test)]
mod tests;
