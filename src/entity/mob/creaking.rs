//! Creaking's fixed-cone watch sensor and bounded pursuit brain.
use super::*;
impl Mob {
    pub(super) fn creaking_think<W: MobWorld + ?Sized>(
        &mut self,
        w: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> (Option<DVec3>, f64) {
        let home = self.creaking.as_ref().unwrap().home;
        if self.creaking.as_ref().unwrap().teardown > 0. {
            return (None, 0.);
        }
        let candidates: Vec<_> = ctx
            .players
            .iter()
            .filter(|p| p.targetable && p.alive && !p.spectator && p.pos.distance_squared(self.pos) < 1024.)
            .collect();
        let mut active = self.creaking.as_ref().unwrap().active;
        if candidates.is_empty() {
            active = false;
        }
        let watched = candidates.iter().any(|p| {
            let eye = p.pos + DVec3::Y * (p.shape.height - 0.18);
            [self.pos.y + 2.3, self.pos.y + 0.5, self.pos.y + 1.15].into_iter().any(|y| {
                let to = DVec3::new(self.pos.x, y, self.pos.z);
                p.look.normalize_or_zero().dot((to - eye).normalize_or_zero()) > 0.5 && line_of_sight(w, eye, to)
            }) && (active || p.pos.distance_squared(self.pos) < 144.)
        });
        if watched {
            active = true;
        }
        let s = self.creaking.as_mut().unwrap();
        s.active = active;
        s.can_move = !watched;
        if watched {
            self.stop_navigation();
            self.vel = DVec3::ZERO;
            return (None, 0.);
        }
        if !active {
            return (None, 0.);
        }
        let Some(target) = candidates
            .into_iter()
            .min_by(|a, b| a.pos.distance_squared(self.pos).total_cmp(&b.pos.distance_squared(self.pos)))
        else {
            return (None, 0.);
        };
        let goal = if home.is_some_and(|h| target.pos.distance_squared(h.as_dvec3() + DVec3::splat(0.5)) > 1024.) {
            home.unwrap().as_dvec3() + DVec3::splat(0.5)
        } else {
            target.pos
        };
        let sees = line_of_sight(w, self.pos + DVec3::Y * 2.3, target.pos + DVec3::Y * 1.62);
        let to = target.pos - self.pos;
        if goal == target.pos && to.length_squared() < 4. && sees && self.attack_cooldown <= 0. {
            self.attack_cooldown = 2.;
            self.attack_anim = 0.75;
            events.push(EntityEvent::PlayerHit {
                player: target.id,
                damage: 3.,
                knockback: ((to * DVec3::new(1., 0., 1.)).normalize_or_zero() * 6.).as_vec3(),
                cause: crate::simulation::damage::Cause::mob("was slain by a creaking"),
                java: Some(crate::knockback::Hit::melee(self.pos, target.pos, 0., 0.)),
            });
            events.push(EntityEvent::OwnerHurt { player: target.id, attacker: self.uid });
        }
        let speed = crate::entity::path::ground_speed(0.4);
        (self.pursue(w, goal, speed, sees, true, rng), speed)
    }
}
