//! v0.15 Track A: the goals of the illagers, the vex and the ravager
//! (Java 26.3), run from `Mob::think`. Targets: `HurtByTargetGoal(Raider)`
//! first (a mob attacker in `illager::State::hurt_by`, a player through the
//! shared player target), then the nearest player in sight within the
//! follow range, then the villager, golem or (for a Johnny) any mob the
//! entity pass found. `None` leaves movement to the kind's goal table
//! (avoiding creakings, the evoker fleeing players, strolling).
use super::*;
use crate::entity::illager::{
    self as il, Action, BLINDNESS, Crossbow, Cue, INVISIBILITY_TICKS, MIRROR, SUMMON, SpellGoal, WOLOLO,
};
use crate::entity::nether::Foe;
use crate::entity::{PlayerId, Target};
use crate::simulation::effects::Effect;

/// What an illager is after this tick.
#[derive(Clone, Copy)]
enum Aim<'a> {
    Player(&'a Target),
    Mob(u32, DVec3, Shape),
}

impl Aim<'_> {
    fn pos(&self) -> DVec3 {
        match self {
            Aim::Player(t) => t.pos,
            Aim::Mob(_, p, _) => *p,
        }
    }
    fn shape(&self) -> Shape {
        match self {
            Aim::Player(t) => t.shape,
            Aim::Mob(_, _, s) => *s,
        }
    }
    fn eye(&self) -> DVec3 {
        match self {
            Aim::Player(t) => t.pos + DVec3::Y * crate::player::EYE_HEIGHT,
            Aim::Mob(_, p, s) => *p + DVec3::Y * (s.height * 0.85),
        }
    }
    fn foe(&self) -> Foe {
        match self {
            Aim::Player(t) => Foe::Player(t.id),
            Aim::Mob(uid, ..) => Foe::Mob(*uid),
        }
    }
}

/// `Mob.isWithinMeleeAttackRange`: the attack box (its own, or its own and
/// its mount's together, grown by `DEFAULT_ATTACK_REACH` and, for the
/// ravager, shrunk 0.05) overlaps the target's box.
fn in_reach(pos: DVec3, half_width: f64, height: f64, deflate: f64, other: DVec3, other_shape: Shape) -> bool {
    let reach = crate::entity::endermite::ATTACK_REACH - deflate;
    (pos.x - other.x).abs() < half_width + other_shape.half_width + reach
        && (pos.z - other.z).abs() < half_width + other_shape.half_width + reach
        && pos.y < other.y + other_shape.height
        && pos.y + height > other.y
}

impl Mob {
    /// `HurtByTargetGoal.start` against a player, for the entity pass.
    pub(in crate::entity) fn hurt_by_target_player(&mut self, player: PlayerId) {
        self.hurt_by_target(player);
    }

    pub(super) fn illager_think<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> Option<(Option<DVec3>, f64)> {
        let full = self.goals.full(self.uid);
        let ticks = self.goal_ticks(dt);
        let wish = self.illager_goals(full, ticks, world, ctx, rng, events);
        // `run_goals` counts the selector's tick when the table runs.
        if wish.is_some() || goals::table(self.kind).is_empty() {
            self.goals.ticks = self.goals.ticks.wrapping_add(1);
        }
        wish
    }

    /// The target this tick, in the target selector's priority order.
    fn illager_aim<'a, W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        ctx: &'a Ctx,
        rng: &mut Rng,
        full: bool,
    ) -> Option<Aim<'a>> {
        let s = self.illager.as_ref()?;
        if let Some((uid, pos, shape)) = s.foe_at.filter(|f| s.hurt_by == Some(f.0)) {
            return Some(Aim::Mob(uid, pos, shape));
        }
        let foe_at = s.foe_at;
        // Stunned or roaring ravagers see nothing (`hasLineOfSight`).
        if self.illager.as_ref().is_some_and(|s| s.stun_tick > 0 || s.roar_tick > 0) {
            return self.move_ctl.target.and_then(|id| ctx.players.iter().find(|t| t.id == id)).map(Aim::Player);
        }
        let range = il::follow_range(self.kind);
        let player = self.java_target(world, ctx, rng, range, full, |_| true);
        if player.is_some() && matches!(self.kind, MobKind::Evoker | MobKind::Illusioner) {
            // `setUnseenMemoryTicks(300)`.
            self.move_ctl.unseen_limit = selector::reduced(300);
        }
        if let Some(t) = player {
            return Some(Aim::Player(t));
        }
        foe_at.map(|(uid, pos, shape)| Aim::Mob(uid, pos, shape))
    }

    fn illager_goals<W: MobWorld + ?Sized>(
        &mut self,
        full: bool,
        ticks: u32,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> Option<(Option<DVec3>, f64)> {
        // `customServerAiStep`: the spell timer.
        {
            let s = self.illager.as_mut()?;
            s.spell_ticks = (s.spell_ticks - ticks as i32).max(0);
            if s.spell_ticks == 0 {
                s.spell = il::Spell::None;
            }
        }
        if self.kind == MobKind::Ravager && self.ravager_timers(ticks, world, ctx, events) {
            self.stop_navigation();
            return Some((None, 0.0));
        }
        let aim = self.illager_aim(world, ctx, rng, full);
        if self.illager.as_ref().is_some_and(|s| s.raider.celebrating) && aim.is_none() {
            self.celebrate(rng, events);
        }
        // v0.15 Track E: `HoldGroundAttackGoal` before the attack goals,
        // and the raid and patrol goals when there is nothing to attack.
        if self.hold_ground(aim.map(|a| a.pos())) {
            if let Some(a) = aim {
                self.face(a.eye());
            }
            return Some((None, 0.0));
        }
        if aim.is_none()
            && !self.illager.as_ref().is_some_and(|s| s.casting_spell())
            && let Some(wish) = self.raider_goals(world, rng)
        {
            return Some(wish);
        }
        // `Raider.aiStep`: a raid member after a player or golem is busy.
        if let Some(a) = aim
            && self.illager.as_ref().is_some_and(|s| s.raider.raid.is_some())
            && matches!(a, Aim::Player(_))
        {
            self.idle = 0.0;
        }
        match self.kind {
            MobKind::Vex => {
                self.vex_goals(full, aim, world, rng, events);
                Some((None, 0.0))
            }
            MobKind::Evoker | MobKind::Illusioner => self.spell_goals(full, ticks, aim, world, ctx, rng, events),
            MobKind::Pillager => self.crossbow_goal(ticks, aim, world, ctx, rng, events),
            MobKind::Vindicator | MobKind::Ravager => {
                if self.kind == MobKind::Vindicator
                    && let Some(wish) = self.break_door(full, ticks, world, rng, events)
                {
                    return Some(wish);
                }
                self.melee_goal(ticks, aim, world, rng, events)
            }
            _ => None,
        }
    }

    /// `RaiderCelebration.tick`: an occasional cheer and hop.
    fn celebrate(&mut self, rng: &mut Rng, events: &mut Vec<EntityEvent>) {
        if rng.next_int(100) == 0 {
            let kind = match self.kind {
                MobKind::Evoker | MobKind::Illusioner => MobKind::Evoker,
                MobKind::Pillager | MobKind::Ravager => MobKind::Pillager,
                _ => MobKind::Vindicator,
            };
            events.push(EntityEvent::Sound { sound: MobSound::Illager(Cue::Celebrate(kind)), pos: self.pos });
        }
        if self.riding.is_none() && self.on_ground && rng.next_int(50) == 0 {
            self.jump = true;
        }
    }

    /// Turn the head toward `at`.
    fn face(&mut self, at: DVec3) {
        let to = at - self.pos;
        let flat = to.with_y(0.0).length();
        let yaw = (to.z as f32).atan2(to.x as f32);
        let eye = self.shape().height * 0.85;
        self.head_target = (wrap(yaw - self.yaw).clamp(-1.2, 1.2), ((to.y - eye) as f32).atan2(flat as f32));
    }

    /// The melee hit `Mob.doHurtTarget` lands on `aim`: the attack damage
    /// attribute with the held weapon, its knockback and Fire Aspect.
    fn melee_hit(&mut self, aim: Aim, rng: &mut Rng, events: &mut Vec<EntityEvent>) {
        let dir = (aim.pos() - self.pos).with_y(0.0).normalize_or(DVec3::X);
        let (damage, cause) = self.melee_attack();
        let fire = self.weapon_fire_seconds();
        self.attack_anim = 0.35;
        match aim {
            Aim::Player(target) => {
                let knockback = dir * 6.0 + DVec3::Y * 5.0;
                let extra = crate::knockback::attack_strength(self.attack_knockback(), 0, false);
                let yaw = (-dir.x).atan2(dir.z).to_degrees() as f32;
                events.push(EntityEvent::PlayerHit {
                    java: Some(crate::knockback::Hit::melee(self.pos, target.pos, yaw, extra)),
                    player: target.id,
                    damage,
                    knockback: knockback.as_vec3(),
                    cause,
                });
                events.push(EntityEvent::OwnerHurt { player: target.id, attacker: self.uid });
                if fire > 0.0 {
                    events.push(EntityEvent::Ignite { player: target.id, secs: fire });
                }
                self.thorns_response(target, knockback, rng, events);
                // A raised shield facing the ravager blocks it
                // (`blockedByItem`).
                if self.kind == MobKind::Ravager && target.blocking && target.look.with_y(0.0).dot(-dir) > 0.0 {
                    self.illager.as_mut().unwrap().actions.push(Action::Blocked(target.id));
                }
            }
            Aim::Mob(uid, ..) => {
                let push = 2.0 + self.attack_knockback() * 2.0;
                events.push(EntityEvent::MobHit {
                    target: uid,
                    attacker: self.uid,
                    damage,
                    knockback: dir * (2.0 + push) + DVec3::Y * 2.0,
                    melee: true,
                });
                if fire > 0.0 {
                    self.illager.as_mut().unwrap().actions.push(Action::Ignite(uid, fire));
                }
            }
        }
        if self.kind == MobKind::Ravager {
            let s = self.illager.as_mut().unwrap();
            s.attack_tick = il::RAVAGER_ATTACK_TICKS;
            events.push(EntityEvent::Sound { sound: MobSound::Illager(Cue::RavagerAttack), pos: self.pos });
        }
    }

    /// `MeleeAttackGoal(1.0)` for the vindicator (`false`: only targets in
    /// sight) and the ravager (`true`): walk a path at the target and hit
    /// every 20 ticks within the attack box and in sight.
    fn melee_goal<W: MobWorld + ?Sized>(
        &mut self,
        ticks: u32,
        aim: Option<Aim>,
        world: &W,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> Option<(Option<DVec3>, f64)> {
        let Some(aim) = aim else {
            let s = self.illager.as_mut()?;
            if s.aggressive {
                // `MeleeAttackGoal.stop`.
                s.aggressive = false;
                self.stop_navigation();
            }
            return None;
        };
        let s = self.illager.as_mut()?;
        if !s.aggressive {
            // `MeleeAttackGoal.start`.
            s.aggressive = true;
            s.attack_wait = 0;
            self.path_move = false;
            self.stop_navigation();
        }
        let s = self.illager.as_mut().unwrap();
        s.attack_wait = s.attack_wait.saturating_sub(ticks);
        self.face(aim.eye());
        let eye = self.pos + DVec3::Y * (self.shape().height * 0.85);
        let sees = line_of_sight(world, eye, aim.eye());
        // Riding a ravager, the attack box spans the mount's too.
        let (hw, deflate) = match (self.kind, self.riding) {
            (MobKind::Ravager, _) => (self.shape().half_width, il::RAVAGER_ATTACK_DEFLATE),
            (_, Some(_)) => (MobKind::Ravager.shape().half_width, 0.0),
            _ => (self.shape().half_width, 0.0),
        };
        let reach = in_reach(self.pos, hw, self.shape().height, deflate, aim.pos(), aim.shape());
        if self.illager.as_ref().unwrap().attack_wait == 0 && sees && reach {
            self.illager.as_mut().unwrap().attack_wait = 20;
            self.melee_hit(aim, rng, events);
        }
        if self.riding.is_some() {
            return Some((None, 0.0));
        }
        let follow_unseen = self.kind == MobKind::Ravager;
        let speed = self.goal_speed(1.0);
        Some((self.pursue(world, aim.pos(), speed, sees || follow_unseen, true, rng), speed))
    }

    /// `VindicatorBreakDoorGoal` (`BreakDoorGoal(mob, 6, Normal or Hard)`,
    /// [`super::door_break`]): in an active raid, pressed against a closed
    /// wooden door its path crosses, a 1 in `reducedTickDelay(10)` start;
    /// then 240 ticks of blows before the door breaks. It holds `MOVE`, so
    /// the vindicator stands facing the door meanwhile.
    fn break_door<W: MobWorld + ?Sized>(
        &mut self,
        full: bool,
        ticks: u32,
        world: &W,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> Option<(Option<DVec3>, f64)> {
        let mut running = false;
        for n in 0..ticks.max(1) {
            running = self.door_goal(world, full && n == 0, rng, events);
            if !running {
                break;
            }
        }
        if !running {
            return None;
        }
        let cell = self.door_break?.cell;
        self.stop_navigation();
        self.face(cell.as_dvec3() + DVec3::splat(0.5));
        Some((None, 0.0))
    }

    /// The ravager's `aiStep` timers, speed and griefing. Returns whether it
    /// is immobile (`isImmobile`: attacking, stunned or roaring).
    fn ravager_timers<W: MobWorld + ?Sized>(
        &mut self,
        ticks: u32,
        world: &W,
        ctx: &Ctx,
        events: &mut Vec<EntityEvent>,
    ) -> bool {
        let _ = ctx;
        let has_target = self.illager.as_ref().is_some_and(|s| s.foe_at.is_some()) || self.move_ctl.target.is_some();
        for _ in 0..ticks {
            let s = self.illager.as_mut().unwrap();
            if s.ravager_busy() {
                s.ravager_speed = 0.0;
            } else {
                let max = if has_target { il::RAVAGER_ATTACK_SPEED } else { il::RAVAGER_BASE_SPEED };
                s.ravager_speed += (max - s.ravager_speed) * il::RAVAGER_SPEED_LERP;
            }
            if s.roar_tick > 0 {
                s.roar_tick -= 1;
                if s.roar_tick == il::RAVAGER_ROAR_AT {
                    s.actions.push(Action::Roar);
                }
            }
            if s.attack_tick > 0 {
                s.attack_tick -= 1;
            }
            if s.stun_tick > 0 {
                s.stun_tick -= 1;
                if s.stun_tick == 0 {
                    s.roar_tick = il::RAVAGER_ROAR_TICKS;
                    events.push(EntityEvent::Sound { sound: MobSound::Illager(Cue::Roar), pos: self.pos });
                }
            }
        }
        // Leaves it pushes against (with `mobGriefing`, checked where the
        // world applies the event); a jump when there are none.
        if self.blocked && ticks > 0 {
            let (min, max) = self.aabb();
            let grow = DVec3::splat(il::RAVAGER_GRIEF_INFLATE);
            let (lo, hi) = ((min - grow).floor().as_ivec3(), (max + grow).floor().as_ivec3());
            let mut broke = false;
            for y in lo.y..=hi.y {
                for z in lo.z..=hi.z {
                    for x in lo.x..=hi.x {
                        let c = IVec3::new(x, y, z);
                        if let Some(block) = world.block(c).filter(|b| b.is_leaves()) {
                            events.push(EntityEvent::MobGrief { cell: c, block, drops: true });
                            broke = true;
                        }
                    }
                }
            }
            if !broke && self.on_ground {
                self.jump = true;
            }
        }
        // `CropBlock.entityInside`: crops it stands in break.
        if ticks > 0 {
            let (min, max) = self.aabb();
            let eps = DVec3::splat(1e-5);
            let (lo, hi) = ((min + eps).floor().as_ivec3(), (max - eps).floor().as_ivec3());
            for y in lo.y..=hi.y {
                for z in lo.z..=hi.z {
                    for x in lo.x..=hi.x {
                        let c = IVec3::new(x, y, z);
                        if let Some(block) = world.block(c).filter(|b| b.as_crop().is_some()) {
                            events.push(EntityEvent::MobGrief { cell: c, block, drops: true });
                        }
                    }
                }
            }
        }
        self.illager.as_ref().unwrap().ravager_busy()
    }

    /// The vex's `VexChargeAttackGoal` (4) and `VexRandomMoveGoal` (8),
    /// which aim its `MoveControl`; `illager::vex_travel` flies it.
    fn vex_goals<W: MobWorld + ?Sized>(
        &mut self,
        full: bool,
        aim: Option<Aim>,
        world: &W,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) {
        let pos = self.pos;
        let s = self.illager.as_mut().unwrap();
        if s.charging {
            match aim {
                Some(aim) if s.wanted.is_some() => {
                    let (min, max) = self.kind.shape().aabb(pos);
                    let (tmin, tmax) = aim.shape().aabb(aim.pos());
                    if min.cmplt(tmax).all() && max.cmpgt(tmin).all() {
                        s.charging = false;
                        self.melee_hit(aim, rng, events);
                    } else if pos.distance_squared(aim.pos()) < il::VEX_REAIM_SQR {
                        s.wanted = Some((aim.eye(), 1.0));
                    }
                }
                _ => s.charging = false,
            }
        }
        let s = self.illager.as_mut().unwrap();
        if !s.charging
            && s.wanted.is_none()
            && full
            && let Some(aim) = aim
            && rng.next_int(selector::reduced(il::VEX_CHARGE_ROLL as i32) as u32) == 0
            && pos.distance_squared(aim.pos()) > il::VEX_CHARGE_MIN_SQR
        {
            s.wanted = Some((aim.eye(), 1.0));
            s.charging = true;
            events.push(EntityEvent::Sound { sound: MobSound::Illager(Cue::VexCharge), pos });
        }
        let s = self.illager.as_mut().unwrap();
        if !s.charging && s.wanted.is_none() && full && rng.next_int(selector::reduced(7) as u32) == 0 {
            let origin = s.bound.unwrap_or(pos.floor().as_ivec3());
            for _ in 0..3 {
                let c = origin
                    + IVec3::new(rng.next_int(15) as i32 - 7, rng.next_int(11) as i32 - 5, rng.next_int(15) as i32 - 7);
                if world.block(c) == Some(crate::world::block::Block::AIR) {
                    s.wanted = Some((c.as_dvec3() + DVec3::splat(0.5), il::VEX_WANDER_SPEED));
                    break;
                }
            }
        }
        // `VexMoveControl`: face the target, or the way it flies.
        let s = self.illager.as_ref().unwrap();
        let toward = match (aim, s.wanted) {
            (Some(aim), Some(_)) => Some(aim.pos() - pos),
            (None, Some(_)) => Some(self.vel),
            _ => None,
        };
        if let Some(d) = toward.filter(|d| d.x != 0.0 || d.z != 0.0) {
            self.yaw = (d.z as f32).atan2(d.x as f32);
        }
        if let Some(aim) = aim {
            self.face(aim.eye());
        }
    }

    /// The spellcasters' goals: `SpellcasterCastingSpellGoal` (1) holds still
    /// while a spell is cast; the use-spell goals start in registration
    /// order on the selector's update, warm up and cast. The illusioner's
    /// `RangedBowAttackGoal` (6) runs beside them.
    #[allow(clippy::too_many_arguments)]
    fn spell_goals<W: MobWorld + ?Sized>(
        &mut self,
        full: bool,
        ticks: u32,
        aim: Option<Aim>,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> Option<(Option<DVec3>, f64)> {
        let now = self.illager.as_ref()?.ticks;
        // The running spell goal: `canContinueToUse`, then `tick`.
        if let Some((goal, warmup)) = self.illager.as_ref()?.casting {
            let keep =
                if goal.spell == il::Spell::Wololo { self.illager.as_ref()?.wololo.is_some() } else { aim.is_some() }
                    && warmup > 0;
            if !keep {
                self.stop_spell();
            } else {
                let left = warmup.saturating_sub(ticks);
                self.illager.as_mut()?.casting = Some((goal, left));
                if left == 0 {
                    self.cast(goal, aim, world, ctx, events);
                    self.stop_spell();
                }
            }
        }
        if full && self.illager.as_ref()?.casting.is_none() && !self.illager.as_ref()?.casting_spell() {
            for (k, &goal) in il::spell_goals(self.kind).iter().enumerate() {
                if self.can_cast(goal, k, now, aim, ctx) {
                    let s = self.illager.as_mut()?;
                    s.casting = Some((goal, goal.warmup));
                    s.spell_ticks = goal.casting;
                    s.next_cast[k] = now + goal.interval;
                    s.spell = goal.spell;
                    if goal == BLINDNESS {
                        s.last_blind = aim.map(|a| a.foe());
                    }
                    events
                        .push(EntityEvent::Sound { sound: MobSound::Illager(Cue::Prepare(goal.spell)), pos: self.pos });
                    break;
                }
            }
        }
        if self.illager.as_ref()?.casting_spell() {
            // `SpellcasterCastingSpellGoal`: stand and look at the target
            // (or the sheep).
            self.stop_navigation();
            let look = aim.map(|a| a.eye()).or(self.illager.as_ref()?.wololo.map(|(_, p)| p + DVec3::Y));
            if let Some(at) = look {
                self.face(at);
            }
            return Some((None, 0.0));
        }
        if self.kind == MobKind::Illusioner {
            return self.bow_goal(ticks, aim, world, ctx, rng, events);
        }
        if let Some(aim) = aim {
            self.face(aim.eye());
        }
        None
    }

    fn stop_spell(&mut self) {
        if let Some(s) = &mut self.illager {
            if s.casting.is_some_and(|(g, _)| g.spell == il::Spell::Wololo) {
                s.wololo = None;
            }
            s.casting = None;
        }
    }

    /// Each spell goal's `canUse`.
    fn can_cast(&self, goal: SpellGoal, k: usize, now: u32, aim: Option<Aim>, ctx: &Ctx) -> bool {
        let Some(s) = self.illager.as_ref() else { return false };
        if s.casting_spell() || now < s.next_cast[k] {
            return false;
        }
        if goal == WOLOLO {
            // Only without a target, with `mobGriefing` and a blue sheep.
            return aim.is_none() && s.wololo.is_some();
        }
        let _ = ctx;
        let Some(aim) = aim else { return false };
        match goal {
            g if g == SUMMON => s.vexes_seen < s.summon_roll,
            g if g == MIRROR => !self.effects.has(Effect::Invisibility),
            g if g == BLINDNESS => {
                s.last_blind != Some(aim.foe())
                    && self.local_difficulty > crate::simulation::difficulty::Difficulty::Normal as u8 as f32
            }
            _ => true,
        }
    }

    /// `performSpellCasting`.
    fn cast<W: MobWorld + ?Sized>(
        &mut self,
        goal: SpellGoal,
        aim: Option<Aim>,
        world: &W,
        ctx: &Ctx,
        events: &mut Vec<EntityEvent>,
    ) {
        let _ = ctx;
        events.push(EntityEvent::Sound { sound: MobSound::Illager(Cue::Cast), pos: self.pos });
        let s = self.illager.as_mut().unwrap();
        match goal.spell {
            il::Spell::SummonVex => s.actions.push(Action::SummonVexes),
            il::Spell::Wololo => {
                if let Some((uid, _)) = s.wololo {
                    s.actions.push(Action::Wololo(uid));
                }
            }
            il::Spell::Fangs => {
                let Some(aim) = aim else { return };
                let target = aim.pos();
                let min_y = target.y.min(self.pos.y);
                let max_y = target.y.max(self.pos.y) + 1.0;
                let angle = ((target.z - self.pos.z) as f32).atan2((target.x - self.pos.x) as f32);
                let dist = self.pos.distance_squared(target);
                for (dx, dz, yaw, warmup) in il::fang_layout(angle, dist) {
                    let (x, z) = (self.pos.x + dx, self.pos.z + dz);
                    if let Some(y) = il::fang_ground(world, x, z, min_y, max_y) {
                        s.actions.push(Action::Fang { pos: DVec3::new(x, y, z), yaw, warmup });
                    }
                }
            }
            il::Spell::Disappear => {
                self.effects.add(Effect::Invisibility, 0, INVISIBILITY_TICKS);
            }
            il::Spell::Blindness => match aim {
                Some(Aim::Player(t)) => events.push(EntityEvent::PlayerEffect {
                    player: t.id,
                    effect: Effect::Blindness,
                    amplifier: 0,
                    ticks: il::BLINDNESS_TICKS,
                }),
                Some(Aim::Mob(uid, ..)) => s.actions.push(Action::Blind(uid)),
                None => {}
            },
            il::Spell::None => {}
        }
    }

    /// The illusioner's `RangedBowAttackGoal(0.5, 20, 15)`: walk toward a
    /// target farther than 15 blocks or seen less than 20 ticks, else stand;
    /// draw for 20 ticks, loose at full power, wait 20.
    #[allow(clippy::too_many_arguments)]
    fn bow_goal<W: MobWorld + ?Sized>(
        &mut self,
        ticks: u32,
        aim: Option<Aim>,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> Option<(Option<DVec3>, f64)> {
        let holds_bow = self.main_hand().is_some_and(|s| s.item == Item::BOW);
        let Some(aim) = aim.filter(|_| holds_bow) else {
            let s = self.illager.as_mut()?;
            if s.aggressive {
                s.aggressive = false;
                s.see_time = 0;
                s.use_ticks = 0;
            }
            return None;
        };
        let eye = self.pos + DVec3::Y * (self.shape().height * 0.85);
        let sees = line_of_sight(world, eye, aim.eye());
        let dist2 = self.pos.distance_squared(aim.pos());
        let s = self.illager.as_mut()?;
        s.aggressive = true;
        for _ in 0..ticks {
            if sees != (s.see_time > 0) {
                s.see_time = 0;
            }
            s.see_time += if sees { 1 } else { -1 };
        }
        let stand = dist2 <= 15.0 * 15.0 && s.see_time >= 20;
        let mut shoot = false;
        if s.use_ticks > 0 {
            if !sees && s.see_time < -60 {
                s.use_ticks = 0;
            } else if sees {
                s.use_ticks += ticks;
                if s.use_ticks >= 21 {
                    s.use_ticks = 0;
                    s.attack_wait = 20;
                    shoot = true;
                }
            }
        } else {
            s.attack_wait = s.attack_wait.saturating_sub(ticks);
            if s.attack_wait == 0 && s.see_time >= -60 {
                s.use_ticks = 1;
            }
        }
        self.face(aim.eye());
        if shoot {
            self.loose_arrows(aim, 1, ctx, rng, events);
        }
        if stand {
            self.stop_navigation();
            return Some((None, 0.0));
        }
        let speed = self.goal_speed(0.5);
        Some((self.pursue(world, aim.pos(), speed, sees, true, rng), speed))
    }

    /// Arrows Java's way (`Illusioner.performRangedAttack`,
    /// `CrossbowItem.shootProjectile`): from the eye less 0.1 toward the
    /// target's lower third, lifted by a fifth of the horizontal distance,
    /// at 1.6 blocks a tick with the difficulty's uncertainty; `count`
    /// arrows spread by Multishot's angles.
    fn loose_arrows(&mut self, aim: Aim, count: u32, ctx: &Ctx, rng: &mut Rng, events: &mut Vec<EntityEvent>) {
        let _ = ctx;
        let from = self.pos + DVec3::Y * (self.shape().height * 0.85 - 0.1);
        let target = aim.pos() + DVec3::Y * (aim.shape().height / 3.0);
        let d = target - from;
        let horizontal = d.with_y(0.0).length();
        let aim_vec = DVec3::new(d.x, d.y + horizontal * f64::from(0.2_f32), d.z).normalize_or(DVec3::X);
        let inaccuracy = il::uncertainty(self.difficulty);
        let enchants = self.main_hand().map_or(Default::default(), |s| s.active_enchants());
        let cause = if self.kind == MobKind::Illusioner { il::ILLUSIONER_ARROW } else { Cause::PILLAGER_ARROW };
        let right = aim_vec.cross(DVec3::Y).normalize_or(DVec3::X);
        let up = right.cross(aim_vec);
        for angle in il::multishot_angles(count, il::MULTISHOT_SPREAD) {
            let dir = glam::DQuat::from_axis_angle(up, f64::from(angle).to_radians()) * aim_vec;
            let spread = 0.0172275 * inaccuracy;
            let noisy = dir.normalize()
                + DVec3::new(rng.triangle(0.0, spread), rng.triangle(0.0, spread), rng.triangle(0.0, spread));
            let vel = noisy * il::MOB_ARROW_POWER * crate::knockback::TICK_RATE;
            events.push(EntityEvent::MobArrow { from, vel, cause, shooter: self.uid, enchants });
        }
        self.attack_anim = 0.35;
        let cue = if self.kind == MobKind::Pillager { Cue::CrossbowShoot } else { Cue::Cast };
        if self.kind == MobKind::Pillager {
            events.push(EntityEvent::Sound { sound: MobSound::Illager(cue), pos: from });
        } else {
            events.push(EntityEvent::Sound { sound: MobSound::Bow, pos: from });
        }
    }

    /// The pillager's `RangedCrossbowAttackGoal(1.0, 8.0)`: move in while
    /// farther than 8 blocks or seen under 5 ticks; charge for the
    /// crossbow's charge time once it may stand, hold it loaded 20-39 ticks,
    /// then fire when in sight (three arrows with Multishot).
    fn crossbow_goal<W: MobWorld + ?Sized>(
        &mut self,
        ticks: u32,
        aim: Option<Aim>,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> Option<(Option<DVec3>, f64)> {
        let crossbow = self.main_hand().filter(|s| s.item == Item::CROSSBOW || s.item == Item::CHARGED_CROSSBOW);
        let (Some(aim), Some(crossbow)) = (aim, crossbow) else {
            // `stop`: no longer aggressive, the target dropped, a charge
            // abandoned.
            let s = self.illager.as_mut()?;
            if s.aggressive {
                s.aggressive = false;
                s.see_time = 0;
                s.charging_crossbow = false;
                s.use_ticks = 0;
                if s.crossbow == Crossbow::Charging {
                    s.crossbow = Crossbow::Uncharged;
                }
                self.move_ctl.target = None;
            }
            return None;
        };
        let enchants = crossbow.active_enchants();
        let eye = self.pos + DVec3::Y * (self.shape().height * 0.85);
        let sees = line_of_sight(world, eye, aim.eye());
        let dist2 = self.pos.distance_squared(aim.pos());
        let charge = il::charge_ticks(enchants.level(crate::enchant::Enchantment::QuickCharge));
        let mut fire = false;
        let mut path = None;
        {
            let s = self.illager.as_mut()?;
            s.aggressive = true;
            for _ in 0..ticks.max(1) {
                if sees != (s.see_time > 0) {
                    s.see_time = 0;
                }
                s.see_time += if sees { 1 } else { -1 };
                let needs_move = (dist2 > il::CROSSBOW_RADIUS * il::CROSSBOW_RADIUS
                    || s.see_time < il::CROSSBOW_SEE_TIME)
                    && s.attack_delay == 0;
                if needs_move {
                    s.path_delay -= 1;
                    if s.path_delay <= 0 {
                        let walk = if s.crossbow == Crossbow::Uncharged { 1.0 } else { 0.5 };
                        path = Some(walk);
                        let (lo, hi) = il::CROSSBOW_PATH_DELAY;
                        s.path_delay = (lo + rng.next_int(hi - lo + 1)) as i32;
                    } else if path.is_none() {
                        path = Some(if s.crossbow == Crossbow::Uncharged { 1.0 } else { 0.5 });
                    }
                } else {
                    s.path_delay = 0;
                    path = None;
                }
                match s.crossbow {
                    Crossbow::Uncharged if !needs_move => {
                        s.crossbow = Crossbow::Charging;
                        s.charging_crossbow = true;
                        s.use_ticks = 0;
                    }
                    Crossbow::Charging => {
                        s.use_ticks += 1;
                        if s.use_ticks >= charge {
                            s.crossbow = Crossbow::Charged;
                            s.attack_delay = il::CROSSBOW_DELAY_BASE + rng.next_int(il::CROSSBOW_DELAY_SPREAD);
                            s.charging_crossbow = false;
                            s.use_ticks = 0;
                            events.push(EntityEvent::Sound {
                                sound: MobSound::Illager(Cue::CrossbowLoaded),
                                pos: self.pos,
                            });
                        }
                    }
                    Crossbow::Charged => {
                        s.attack_delay = s.attack_delay.saturating_sub(1);
                        if s.attack_delay == 0 {
                            s.crossbow = Crossbow::ReadyToAttack;
                        }
                    }
                    Crossbow::ReadyToAttack if sees => {
                        s.crossbow = Crossbow::Uncharged;
                        fire = true;
                    }
                    _ => {}
                }
            }
        }
        self.face(aim.eye());
        if fire {
            let count = 1 + 2 * u32::from(enchants.level(crate::enchant::Enchantment::Multishot));
            self.loose_arrows(aim, count, ctx, rng, events);
            // `onCrossbowAttackPerformed`: `noActionTime = 0`.
            self.idle = 0.0;
        }
        match path {
            Some(modifier) => {
                let speed = self.goal_speed(modifier);
                Some((self.pursue(world, aim.pos(), speed, sees, true, rng), speed))
            }
            None => {
                self.stop_navigation();
                Some((None, 0.0))
            }
        }
    }
}
