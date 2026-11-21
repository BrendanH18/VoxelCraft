//! v0.16 Track F: a hunting conduit's strike (`ConduitBlockEntity
//! .updateAndAttackTarget`, `updateDestroyTarget` and `selectNewTarget`).

use glam::{DVec3, IVec3};

use super::{Entities, MobKind};
use crate::world::World;
use crate::world::conduit::{ATTACK_DAMAGE, KILL_RANGE, closer_than};

/// Java's `Enemy`: every hostile-group mob but the sulfur cube (an
/// `AgeableMob` that doesn't implement `Enemy` in 26.3).
pub fn is_enemy(kind: MobKind) -> bool {
    kind.is_hostile() && kind != MobKind::SulfurCube
}

impl Entities {
    /// One pulse of the conduit at `pos`: keeps `target` while it is alive
    /// and its block is closer than 8, else (with no target) picks a random
    /// enemy in water or rain inside the cell grown by 8; then hurts the
    /// target for 4 (magic). Not `hunting` drops the target. Returns the
    /// target and where it was struck.
    pub fn conduit_hunt(
        &mut self,
        world: &World,
        pos: IVec3,
        hunting: bool,
        target: Option<u32>,
    ) -> (Option<u32>, Option<DVec3>) {
        let target = if !hunting {
            None
        } else if let Some(uid) = target {
            self.mobs
                .iter()
                .find(|m| m.uid == uid && m.alive() && closer_than(pos, m.pos.floor().as_ivec3(), KILL_RANGE))
                .map(|m| m.uid)
        } else {
            let lo = pos.as_dvec3() - DVec3::splat(KILL_RANGE);
            let hi = pos.as_dvec3() + DVec3::splat(1.0 + KILL_RANGE);
            let wet = |m: &super::Mob| {
                let top = m.pos + DVec3::Y * m.shape().height;
                m.in_water || world.rains_on(m.pos.floor().as_ivec3()) || world.rains_on(top.floor().as_ivec3())
            };
            let candidates: Vec<u32> = self
                .mobs
                .iter()
                .filter(|m| m.alive() && is_enemy(m.kind) && wet(m))
                .filter(|m| {
                    let (min, max) = m.aabb();
                    min.cmplt(hi).all() && max.cmpgt(lo).all()
                })
                .map(|m| m.uid)
                .collect();
            (!candidates.is_empty()).then(|| candidates[self.rng.next_int(candidates.len() as u32) as usize])
        };
        let mut struck = None;
        if let Some(uid) = target
            && let Some(i) = self.mobs.iter().position(|m| m.uid == uid)
        {
            struck = Some(self.mobs[i].pos);
            let m = &self.mobs[i];
            let (kind, pos, burning, player_kill) = (m.kind, m.pos, m.burning, m.player_hit_left > 0.0);
            if self.mobs[i].damage_bypassing_wolf_armor(ATTACK_DAMAGE, None, &mut self.rng) {
                self.drop_loot_with_fire(kind, pos, 0, burning, player_kill);
            }
        }
        (target, struck)
    }
}
