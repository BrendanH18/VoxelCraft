//! v0.16 Track D: the drowned's trident (`Drowned.DrownedTridentAttackGoal`,
//! a `RangedAttackGoal(this, 1.0, 40, 10.0F)` that runs while a trident is
//! in its main hand, ahead of its melee goal at the same priority).

use glam::DVec3;

use super::{EntityEvent, Mob, MobKind, MobSound, MobWorld, Rng, line_of_sight};
use crate::weapons::{self, Cue, trident as tr};

impl Mob {
    /// A drowned holding a trident in its main hand.
    pub fn holds_trident(&self) -> bool {
        self.kind == MobKind::Drowned && self.equipment.main_hand().is_some_and(|s| s.item == weapons::TRIDENT)
    }

    /// `RangedAttackGoal.tick` with min = max interval 40 ticks and radius
    /// 10: within range and in sight it stands and throws every 2 seconds,
    /// otherwise it closes in. Returns the movement like the other tactics.
    pub(super) fn drowned_trident_tactics<W: MobWorld + ?Sized>(
        &mut self,
        _dt: f32,
        world: &W,
        target: &super::super::Target,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> (Option<DVec3>, f64) {
        let eye = self.pos + DVec3::Y * (self.shape().height * 0.85);
        let aim = target.pos + DVec3::Y * (target.shape.height / 3.0);
        let sees = line_of_sight(world, eye, target.pos + DVec3::Y * (target.shape.height * 0.9));
        let dist = self.pos.distance(target.pos);
        if self.attack_cooldown <= 0.0 && sees && dist <= tr::DROWNED_ATTACK_RADIUS {
            self.attack_cooldown = tr::DROWNED_ATTACK_INTERVAL as f32 / 20.0;
            self.attack_anim = 0.35;
            events.push(EntityEvent::ThrowTrident { from: eye, target: aim, shooter: self.uid });
            events.push(EntityEvent::Sound { sound: MobSound::Weapon(Cue::DrownedShoot), pos: eye });
        }
        if dist <= tr::DROWNED_ATTACK_RADIUS && sees {
            return (None, 0.0);
        }
        let speed = self.kind.chase_speed();
        (self.pursue(world, target.pos, speed, sees, true, rng), speed)
    }
}
