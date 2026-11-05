//! Polar bear melee and its high water inertia on shared navigation.
use super::*;
use crate::{
    entity::{
        nether::Foe,
        path::{self, Start},
    },
    physics::Shape,
};
impl Mob {
    /// Client selection/labels use the standing dimensions; server collisions
    /// keep the ordinary dimensions, as in PolarBear.tick/getDefaultDimensions.
    pub(crate) fn selection_shape(&self) -> Shape {
        let s = self.shape();
        Shape::new(
            s.half_width,
            s.height * (1.0 + self.polar_bear.as_ref().map_or(0.0, |b| f64::from(b.animation) / 6.0)),
        )
    }
    pub(crate) fn polar_reachable<W: MobWorld + ?Sized>(&self, w: &W, pos: DVec3) -> bool {
        let target = pos.floor().as_ivec3();
        path::find_path(
            w,
            &self.caps(true),
            Start { pos: self.pos, on_ground: self.on_ground, in_water: self.in_water },
            &[target],
            0,
        )
        .and_then(|p| p.end())
        .is_some_and(|p| {
            let d = p - target;
            d.x * d.x + d.z * d.z <= 2
        })
    }
    pub(super) fn polar_think<W: MobWorld + ?Sized>(
        &mut self,
        w: &W,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> Option<(Option<DVec3>, f64)> {
        let target = self.polar_bear.as_ref()?.current?;
        if self.baby
            || self
                .goals
                .running
                .is_some_and(|i| matches!(super::goals::table(self.kind)[i as usize].goal, super::goals::Goal::Panic))
        {
            return None;
        }
        self.stop_goal(w);
        let delta = target.pos - self.pos;
        let flat = delta.with_y(0.0);
        let dir = flat.normalize_or(DVec3::X);
        self.head_target = (wrap(dir.z.atan2(dir.x) as f32 - self.yaw).clamp(-1.2, 1.2), 0.0);
        let sees = line_of_sight(
            w,
            self.pos + DVec3::Y * self.shape().height * 0.85,
            target.pos + DVec3::Y * target.shape.height * 0.85,
        );
        let can_attack = polar_melee(self.pos, self.shape(), target.pos, target.shape) && sees;
        let s = self.polar_bear.as_mut().unwrap();
        if can_attack && self.attack_cooldown <= 0.0 {
            self.attack_cooldown = 1.0;
            self.attack_anim = 0.35;
            s.standing = false;
            let (damage, cause) = self.melee_attack();
            match target.foe {
                Foe::Player(player) => {
                    let extra = crate::knockback::attack_strength(self.attack_knockback(), 0, false);
                    let yaw = (-dir.x).atan2(dir.z).to_degrees() as f32;
                    events.push(EntityEvent::PlayerHit {
                        java: Some(crate::knockback::Hit::melee(self.pos, target.pos, yaw, extra)),
                        player,
                        damage,
                        knockback: (dir * 6.0 + DVec3::Y * 5.0).as_vec3(),
                        cause,
                    });
                    events.push(EntityEvent::OwnerHurt { player, attacker: self.uid });
                }
                Foe::Mob(target) => events.push(EntityEvent::MobHit {
                    target,
                    attacker: self.uid,
                    damage,
                    knockback: dir * 4.0 + DVec3::Y * 2.0,
                }),
            }
        } else if delta.length_squared() < (target.shape.half_width * 2.0 + 3.0).powi(2) {
            if self.attack_cooldown <= 0.0 {
                s.standing = false;
                self.attack_cooldown = 1.0;
            }
            if self.attack_cooldown <= 0.5 {
                s.standing = true;
                if s.warning == 0 {
                    s.warning = 40;
                    events.push(EntityEvent::Sound { sound: MobSound::Ambient(MobKind::PolarBear), pos: self.pos });
                }
            }
        } else {
            self.attack_cooldown = 1.0;
            s.standing = false;
        }
        let speed = self.goal_speed(1.25);
        Some((self.pursue(w, target.pos, speed, sees, true, rng), speed))
    }
    pub(super) fn polar_water_tick<W: BlockSource + ?Sized>(
        &mut self,
        w: &W,
        wish: Option<DVec3>,
        speed: f64,
        rng: &mut Rng,
    ) -> bool {
        if self.kind != MobKind::PolarBear {
            return false;
        }
        let mut dm = self.vel * 0.05;
        for v in [&mut dm.x, &mut dm.y, &mut dm.z] {
            if v.abs() < 0.003 {
                *v = 0.0;
            }
        }
        if self.alive() {
            if let Some(dir) = wish {
                let yaw = dir.z.atan2(dir.x) as f32;
                self.yaw = turn_toward(self.yaw, yaw, std::f32::consts::FRAC_PI_2);
                let input = path::java_speed(speed) * self.effects.speed_factor();
                dm += DVec3::new(f64::from(self.yaw.cos()), 0.0, f64::from(self.yaw.sin())) * input * 0.02;
            }
            if physics::is_fluid_at(w, self.pos + DVec3::Y * 0.4) && rng.next_f32() < 0.8 {
                dm.y += 0.04;
            }
        }
        let from = self.pos;
        let delta = dm;
        let shape = self.shape();
        let hit = if self.on_ground {
            physics::move_box_stepping(w, &mut self.pos, &mut dm, delta, shape, 0.6)
        } else {
            physics::move_box(w, &mut self.pos, &mut dm, delta, shape)
        };
        self.on_ground = hit.on_ground;
        self.blocked = hit.horizontal;
        dm.x *= f64::from(0.98_f32);
        dm.z *= f64::from(0.98_f32);
        dm.y *= f64::from(0.8_f32);
        dm.y -= 0.005;
        let bank = self.pos + DVec3::new(dm.x, dm.y + 0.6000000238418579 - self.pos.y + from.y, dm.z);
        if hit.horizontal
            && !physics::overlaps_solid(w, bank, shape)
            && !physics::touches_block(w, bank, shape, Block::holds_water)
        {
            dm.y = 0.30000001192092896;
        }
        self.vel = dm * 20.0;
        true
    }
}
fn polar_melee(pos: DVec3, s: Shape, other: DVec3, o: Shape) -> bool {
    let reach = 2.0399999618530273_f64.sqrt() - 0.6000000238418579;
    (pos.x - other.x).abs() < s.half_width + o.half_width + reach
        && (pos.z - other.z).abs() < s.half_width + o.half_width + reach
        && pos.y < other.y + o.height
        && pos.y + s.height > other.y
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{entity::polar_bear::Aim, physics::test_util::Grid};
    use glam::IVec3;
    struct Pool(Grid);
    impl BlockSource for Pool {
        fn block(&self, p: IVec3) -> Option<Block> {
            self.0.block(p)
        }
    }
    impl MobWorld for Pool {
        fn loaded(&self, _: IVec3) -> bool {
            true
        }
        fn surface(&self, _: i32, _: i32) -> Option<i32> {
            Some(0)
        }
        fn exposed(&self, _: IVec3) -> bool {
            true
        }
    }
    #[test]
    fn polar_bear_warns_stands_and_uses_twenty_tick_attacks() {
        let w = Pool(Grid::flat(1));
        let mut m = Mob::new(MobKind::PolarBear, DVec3::new(0.5, 1.0, 0.5), 0.0);
        m.on_ground = true;
        m.polar_bear.as_mut().unwrap().current = Some(Aim {
            foe: Foe::Player(crate::entity::PlayerId::HOST),
            pos: DVec3::new(3.5, 1.0, 0.5),
            shape: crate::player::SHAPE,
        });
        m.attack_cooldown = 0.5;
        let mut rng = Rng::new(2);
        let mut events = Vec::new();
        m.polar_think(&w, &mut rng, &mut events);
        assert!(m.polar_bear.as_ref().unwrap().standing);
        assert_eq!(events.iter().filter(|e| matches!(e, EntityEvent::Sound { .. })).count(), 1);
        m.polar_think(&w, &mut rng, &mut events);
        assert_eq!(events.len(), 1);
        m.polar_bear.as_mut().unwrap().current.as_mut().unwrap().pos.x = 1.5;
        m.attack_cooldown = 0.0;
        m.polar_think(&w, &mut rng, &mut events);
        assert!(!m.polar_bear.as_ref().unwrap().standing);
        assert_eq!(m.attack_cooldown, 1.0);
        assert!(events.iter().any(|e| matches!(e,EntityEvent::PlayerHit{damage,..} if *damage==6.0)));
    }
    #[test]
    fn polar_bear_water_uses_point_nine_eight_horizontal_drag_and_standing_selection_is_separate() {
        let mut w = Pool(Grid::flat(-10));
        for x in -3..4 {
            for z in -3..4 {
                w.0.set(IVec3::new(x, 0, z), Block::WATER);
            }
        }
        let mut m = Mob::new(MobKind::PolarBear, DVec3::new(0.5, 0.1, 0.5), 0.0);
        m.vel.x = 10.0;
        let mut rng = Rng::new(44);
        assert!(m.polar_water_tick(&w, None, 0.0, &mut rng));
        assert!((m.vel.x - 9.800000190734863).abs() < 1e-12);
        m.polar_bear.as_mut().unwrap().animation = 6.0;
        assert_eq!(m.shape().height, 1.4);
        assert_eq!(m.selection_shape().height, 2.8);
    }
}
