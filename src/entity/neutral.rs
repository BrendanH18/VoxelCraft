//! Java 26.3 `NeutralMob`'s two game rules for every implementer: bees, iron
//! golems, polar bears, wolves, endermen and zombified piglins.
//!
//! `forgive_dead_players`: `ServerPlayer.die` calls `tellNeutralMobsThatIDied`,
//! and every neutral mob in the box around the dead player's block whose
//! persistent anger target is that player stops being angry
//! (`NeutralMob.playerDied`). With the rule off the grudge outlives the death
//! and the respawned player is still the target.
//!
//! `universal_anger`: a player's hit no longer makes the player the target
//! (`HurtByTargetGoal` refuses it); `ResetUniversalAngerTargetGoal` instead
//! leaves the mob angry with no persistent target, and the bee, wolf and
//! zombified piglin pass that on to their own kind within follow range. An
//! angry mob without a target is angry at every valid player
//! (`isAngryAtAllPlayers`); its player target goal picks the nearest one,
//! which `updatePersistentAnger` then keeps. Polar bears run the same rules
//! in `polar_bear`; piglins, which are not neutral mobs, use their brain's
//! `UNIVERSAL_ANGER` memory in `nether`.
use super::{Ctx, Entities, Mob, MobKind, MobWorld, PlayerId, nether::Foe};
use glam::DVec3;

/// The concrete `NeutralMob` implementers.
pub const KINDS: [MobKind; 6] =
    [MobKind::Bee, MobKind::IronGolem, MobKind::PolarBear, MobKind::Wolf, MobKind::Enderman, MobKind::ZombifiedPiglin];

/// `tellNeutralMobsThatIDied`: `new AABB(blockPosition()).inflate(32, 10, 32)`.
pub const DEATH_NOTICE: [f64; 3] = [32.0, 10.0, 32.0];

/// `PiglinAi.setAngerTarget`: the `UNIVERSAL_ANGER` memory's expiry in ticks.
pub const PIGLIN_UNIVERSAL_ANGER_TICKS: u32 = 600;

/// Whether `kind` implements `NeutralMob`.
pub fn is_neutral(kind: MobKind) -> bool {
    KINDS.contains(&kind)
}

/// The `follow_range` attribute: the universal-anger alert box and the
/// player target search.
pub fn follow_range(kind: MobKind) -> Option<f64> {
    Some(match kind {
        MobKind::Bee | MobKind::IronGolem | MobKind::Wolf => 16.0,
        MobKind::PolarBear => 20.0,
        MobKind::Enderman => 64.0,
        MobKind::ZombifiedPiglin => 35.0,
        _ => return None,
    })
}

/// `ResetUniversalAngerTargetGoal`'s `alertOthersOfSameType`.
pub fn alerts_same_type(kind: MobKind) -> bool {
    matches!(kind, MobKind::Bee | MobKind::Wolf | MobKind::ZombifiedPiglin)
}

/// The anger timer in ticks, where the game uses Java's
/// `PERSISTENT_ANGER_TIME` (`rangeOfSeconds(20, 39)`). `None` marks a kind
/// whose existing timer still differs (see the EFX-RULE-21 record).
pub fn persistent_anger_ticks(kind: MobKind) -> Option<(u32, u32)> {
    matches!(kind, MobKind::Bee | MobKind::PolarBear).then_some((400, 780))
}

impl Mob {
    /// `getPersistentAngerTarget` when it is a player.
    pub fn anger_target(&self) -> Option<PlayerId> {
        match self.kind {
            MobKind::Bee => self.bee.and_then(|s| s.target).map(PlayerId),
            MobKind::PolarBear => match self.polar_bear.as_ref()?.angry_at? {
                Foe::Player(p) => Some(p),
                Foe::Mob(_) => None,
            },
            MobKind::Wolf | MobKind::IronGolem | MobKind::Enderman | MobKind::ZombifiedPiglin => self.angry_player,
            _ => None,
        }
    }

    /// `isAngry` for the kinds `neutral` drives (polar bears keep their own clock).
    fn neutral_angry(&self) -> bool {
        match self.kind {
            MobKind::Bee => self.bee.is_some_and(|s| s.anger > 0),
            MobKind::Wolf | MobKind::Enderman | MobKind::ZombifiedPiglin => self.provoked > 0.0,
            MobKind::IronGolem => self.angry_at_player(),
            _ => false,
        }
    }

    /// Angry, with no persistent target, after a hit under `universal_anger`.
    pub fn angry_at_all_players(&self) -> bool {
        self.neutral_universal && self.neutral_angry()
    }

    /// `NeutralMob.stopBeingAngry`.
    pub(super) fn stop_being_angry(&mut self) {
        self.neutral_universal = false;
        match self.kind {
            MobKind::Bee => {
                if let Some(s) = &mut self.bee {
                    s.anger = 0;
                    s.target = None;
                }
            }
            MobKind::PolarBear => {
                if let Some(s) = &mut self.polar_bear {
                    s.calm();
                }
            }
            MobKind::Wolf | MobKind::Enderman | MobKind::ZombifiedPiglin => {
                self.angry_player = None;
                self.provoked = 0.0;
            }
            MobKind::IronGolem => {
                self.angry_player = None;
                self.player_hit_left = 0.0;
            }
            _ => {}
        }
    }

    /// `startPersistentAngerTimer`, with each kind's existing timer.
    fn start_anger_timer(&mut self, rng: &mut super::Rng) {
        match self.kind {
            MobKind::Bee => {
                if let Some(s) = &mut self.bee {
                    s.anger = 400 + rng.next_int(381) as u16;
                    s.pollination = 0;
                }
            }
            MobKind::Wolf => self.provoked = self.provoked.max(super::mob::PROVOKED_TIME),
            MobKind::Enderman => self.provoked = self.provoked.max(super::mob::ENDERMAN_ANGER_TIME),
            MobKind::ZombifiedPiglin => self.provoked = self.provoked.max(super::mob::PIGLIN_ANGER_TIME),
            MobKind::IronGolem => self.player_hit_left = self.player_hit_left.max(5.0),
            _ => {}
        }
    }

    /// `forgetCurrentTargetAndRefreshUniversalAnger`.
    fn refresh_universal_anger(&mut self, rng: &mut super::Rng) {
        if !self.alive() || self.kind == MobKind::PolarBear {
            return;
        }
        self.stop_being_angry();
        self.start_anger_timer(rng);
        self.neutral_universal = true;
    }

    /// The persistent target becomes `player` (`updatePersistentAnger` with a
    /// new target restarts the timer).
    fn keep_anger_at(&mut self, player: PlayerId, rng: &mut super::Rng) {
        self.neutral_universal = false;
        match self.kind {
            MobKind::Bee => {
                if let Some(s) = &mut self.bee {
                    s.target = Some(player.0);
                }
            }
            _ => self.angry_player = Some(player),
        }
        self.start_anger_timer(rng);
    }
}

impl Entities {
    /// `ServerPlayer.tellNeutralMobsThatIDied` for `player`, dead at `at`,
    /// when `forgive_dead_players` is on: neutral mobs in the box around its
    /// block that hold it as their persistent target stop being angry.
    pub fn player_died(&mut self, player: PlayerId, at: DVec3) {
        if !self.neutral_forgive_dead_players || !at.is_finite() {
            return;
        }
        let block = at.floor();
        let reach = DVec3::from_array(DEATH_NOTICE);
        let (lo, hi) = (block - reach, block + DVec3::ONE + reach);
        for m in &mut self.mobs {
            let (a, b) = m.aabb();
            if is_neutral(m.kind) && a.cmplt(hi).all() && b.cmpgt(lo).all() && m.anger_target() == Some(player) {
                m.stop_being_angry();
            }
        }
    }

    /// `ResetUniversalAngerTargetGoal.start` for mob `index`, hurt by a player
    /// under `universal_anger`: it, and for bees, wolves and zombified piglins
    /// every one of its kind in the box its follow range spans
    /// (`unitCubeFromLowerCorner(position).inflate(range, 10, range)`), turn
    /// angry with no target.
    pub(super) fn universal_anger_hit(&mut self, index: usize) {
        let Some(m) = self.mobs.get(index).filter(|m| m.alive()) else { return };
        let (kind, pos) = (m.kind, m.pos);
        self.mobs[index].refresh_universal_anger(&mut self.rng);
        if !alerts_same_type(kind) {
            return;
        }
        let r = follow_range(kind).unwrap_or(16.0);
        let reach = DVec3::new(r, 10.0, r);
        let (lo, hi) = (pos - reach, pos + DVec3::ONE + reach);
        for (j, m) in self.mobs.iter_mut().enumerate() {
            let (a, b) = m.aabb();
            if j != index && m.kind == kind && a.cmplt(hi).all() && b.cmpgt(lo).all() {
                m.refresh_universal_anger(&mut self.rng);
            }
        }
    }

    /// `HurtByTargetGoal` (and, for zombified piglins, its alert) for a
    /// wolf, iron golem, enderman or zombified piglin a player hurt.
    pub(super) fn neutral_hurt_by(&mut self, index: usize, player: PlayerId) {
        let Some(m) = self.mobs.get_mut(index) else { return };
        match m.kind {
            MobKind::Wolf if m.animal.as_ref().is_none_or(|a| a.owner != Some(player)) => {
                m.angry_player = Some(player);
                m.neutral_universal = false;
            }
            MobKind::IronGolem if !m.built => {
                m.angry_player = Some(player);
                m.neutral_universal = false;
            }
            MobKind::Enderman if m.alive() => {
                m.angry_player = Some(player);
                m.neutral_universal = false;
                m.provoked = m.provoked.max(super::mob::ENDERMAN_ANGER_TIME);
            }
            MobKind::ZombifiedPiglin => {
                let at = m.pos;
                self.anger_piglins(at, player);
            }
            _ => {}
        }
    }

    /// Each target selector's `NearestAttackableTargetGoal<Player>` with
    /// `isAngryAt` for a mob angry at every player: every other tick, one
    /// chance in five (`reducedTickDelay(10)`), the nearest valid player in
    /// follow range it can see (endermen need no sight line) becomes its
    /// persistent target.
    pub(super) fn neutral_sense<W: MobWorld + ?Sized>(&mut self, w: &W, ctx: &Ctx) {
        if !self.mobs.iter().any(|m| m.neutral_universal) {
            return;
        }
        self.ensure_mob_ids();
        for i in 0..self.mobs.len() {
            let m = &self.mobs[i];
            if !m.neutral_universal || !m.simulation_active {
                continue;
            }
            if !m.alive() || !m.angry_at_all_players() || m.anger_target().is_some() {
                self.mobs[i].neutral_universal = false;
                continue;
            }
            if !self.neutral_universal_anger
                || !(self.spawn_ticks + u64::from(m.uid)).is_multiple_of(2)
                || self.rng.next_int(5) != 0
            {
                continue;
            }
            let m = &self.mobs[i];
            let range = follow_range(m.kind).unwrap_or(16.0);
            let eye = m.pos + DVec3::Y * m.shape().height * 0.85;
            let owner = m.animal.as_ref().and_then(|a| a.owner);
            let pick = ctx
                .players
                .iter()
                .filter(|p| {
                    p.targetable
                        && p.alive
                        && owner != Some(p.id)
                        && p.pos.distance_squared(m.pos) <= p.detection_range(range).powi(2)
                        && (m.kind == MobKind::Enderman || super::mob::line_of_sight(w, eye, p.pos + DVec3::Y * 1.62))
                })
                .min_by(|a, b| a.pos.distance_squared(m.pos).total_cmp(&b.pos.distance_squared(m.pos)))
                .map(|p| p.id);
            if let Some(p) = pick {
                self.mobs[i].keep_anger_at(p, &mut self.rng);
            }
        }
    }
}

#[cfg(test)]
mod tests;
