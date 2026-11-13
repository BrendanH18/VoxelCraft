//! The endermite's 26.3 goals: `HurtByTargetGoal().setAlertOthers()` (1)
//! and `NearestAttackableTargetGoal<Player>(mustSee)` (2) choose a target;
//! `MeleeAttackGoal(1.0, false)` (2) walks at it and bites every 20 ticks
//! when in reach and in sight. Without a target it falls back to the shared
//! stroll (`WaterAvoidingRandomStrollGoal(1.0)`, 3) and idle looking.
use super::*;
use crate::entity::endermite::{ATTACK_INTERVAL, EYE_HEIGHT, FOLLOW_RANGE, within_melee};
use crate::entity::nether::Foe;

impl Mob {
    /// Seconds of anger an enderman holds (any hit raises it).
    pub(in crate::entity) fn provocation(&self) -> f32 {
        self.provoked
    }

    /// Puts back an enderman's anger after a bite from the endermite it
    /// hunts: its `HurtByTargetGoal` picks that endermite, which it already
    /// targets, rather than a player.
    pub(in crate::entity) fn set_provocation(&mut self, seconds: f32) {
        self.provoked = seconds;
    }

    pub(super) fn endermite_think<W: MobWorld + ?Sized>(
        &mut self,
        dt: f32,
        world: &W,
        ctx: &Ctx,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> Option<(Option<DVec3>, f64)> {
        let full = self.selector_full();
        let ticks = self.goal_ticks(dt);
        let s = self.endermite.as_mut()?;
        s.attack_wait = s.attack_wait.saturating_sub(ticks);
        // A mob the endermite was hurt by comes first; otherwise the player
        // target selector (`HurtByTargetGoal` or the nearest visible player).
        let aim = match (s.foe, s.foe_at) {
            (Some(uid), Some((pos, shape))) => Some((Foe::Mob(uid), pos, shape, None)),
            _ => self
                .java_target(world, ctx, rng, FOLLOW_RANGE, full, |_| true)
                .map(|&t| (Foe::Player(t.id), t.pos, t.shape, Some(t))),
        };
        let Some((foe, pos, shape, player)) = aim else {
            if self.ai == Ai::Chase {
                // `MeleeAttackGoal.stop`.
                self.ai = Ai::Idle;
                self.ai_timer = 1.0;
                self.stop_navigation();
            }
            return None;
        };
        if self.ai != Ai::Chase {
            // `MeleeAttackGoal.start`: a fresh path and no wait to bite.
            self.ai = Ai::Chase;
            self.path_move = false;
            self.stop_navigation();
            self.endermite.as_mut().unwrap().attack_wait = 0;
        }
        // The model has no separate head: `Endermite.tick` keeps the body
        // on the entity's own rotation.
        self.head_target = (0.0, 0.0);
        let to = pos - self.pos;
        let dir = to.with_y(0.0).normalize_or(DVec3::X);
        let eye = self.pos + DVec3::Y * EYE_HEIGHT;
        let target_eye =
            pos + DVec3::Y * if player.is_some() { crate::player::EYE_HEIGHT } else { shape.height * 0.85 };
        let sees = line_of_sight(world, eye, target_eye);
        let in_reach = within_melee(self.pos, self.shape(), pos, shape);
        let s = self.endermite.as_mut().unwrap();
        if s.attack_wait == 0 && sees && in_reach {
            s.attack_wait = ATTACK_INTERVAL;
            self.attack_anim = 0.35;
            let (damage, cause) = self.melee_attack();
            match (foe, player) {
                (Foe::Player(id), Some(target)) => {
                    let knockback = dir * 6.0 + DVec3::Y * 5.0;
                    let extra = crate::knockback::attack_strength(self.attack_knockback(), 0, false);
                    let yaw = (-dir.x).atan2(dir.z).to_degrees() as f32;
                    events.push(EntityEvent::PlayerHit {
                        java: Some(crate::knockback::Hit::melee(self.pos, pos, yaw, extra)),
                        player: id,
                        damage,
                        knockback: knockback.as_vec3(),
                        cause,
                    });
                    events.push(EntityEvent::OwnerHurt { player: id, attacker: self.uid });
                    self.thorns_response(&target, knockback, rng, events);
                }
                (Foe::Mob(target), _) => events.push(EntityEvent::MobHit {
                    target,
                    attacker: self.uid,
                    damage,
                    knockback: dir * 4.0 + DVec3::Y * 2.0,
                }),
                (Foe::Player(_), None) => {}
            }
        }
        let speed = self.goal_speed(1.0);
        Some((self.pursue(world, pos, speed, sees, true, rng), speed))
    }
}

impl crate::entity::Entities {
    /// `HurtByTargetGoal.start` with `setAlertOthers()`: the endermite at
    /// `index` turns on `foe`, and so does every endermite without a target
    /// in the box `unitCubeFromLowerCorner(position).inflate(16, 10, 16)`.
    pub(in crate::entity) fn endermite_hurt(&mut self, index: usize, foe: Foe) {
        let Some(pos) = self.mobs.get(index).filter(|m| m.kind == MobKind::Endermite && m.alive()).map(|m| m.pos)
        else {
            return;
        };
        let reach = FOLLOW_RANGE;
        for (j, m) in self.mobs.iter_mut().enumerate() {
            let Some(untargeted) = m.endermite.as_ref().map(|s| s.foe.is_none()) else { continue };
            let free = untargeted && m.move_ctl.target.is_none();
            let d = m.pos - pos;
            let near = (-reach..reach + 1.0).contains(&d.x)
                && (-reach..reach + 1.0).contains(&d.z)
                && (-10.0..11.0).contains(&d.y);
            if !m.alive() || !(j == index || near && free) || foe == Foe::Mob(m.uid) {
                continue;
            }
            match foe {
                Foe::Player(player) => m.hurt_by_target(player),
                Foe::Mob(_) => m.move_ctl.target = None,
            }
            let s = m.endermite.as_mut().unwrap();
            s.foe = match foe {
                Foe::Mob(uid) => Some(uid),
                Foe::Player(_) => None,
            };
            s.foe_at = None;
        }
    }
}
