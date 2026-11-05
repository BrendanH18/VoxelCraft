//! Ocelot prey stalking, sprint and leap on shared navigation/air travel.
use super::*;
impl Mob {
    pub(super) fn ocelot_think<W: MobWorld + ?Sized>(
        &mut self,
        w: &W,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> Option<(Option<DVec3>, f64)> {
        let (uid, at) = self.ocelot.as_ref()?.prey?;
        if self.goals.running.is_some_and(|i| goals::table(self.kind)[i as usize].priority < 7) {
            return None;
        }
        self.stop_goal(w);
        let flat = (at - self.pos).with_y(0.0);
        let d = self.pos.distance_squared(at);
        self.look_at(at, 0.52);
        if self.ocelot.as_ref().unwrap().leaping {
            if !self.on_ground {
                return Some((None, 0.0));
            }
            self.ocelot.as_mut().unwrap().leaping = false;
        }
        if self.on_ground && (4.0..=16.0).contains(&d) && rng.next_int(5) == 0 {
            let dir = flat.normalize_or_zero();
            self.vel = self.vel.with_y(0.0) * 0.2 + dir * 8.0;
            self.vel.y = 6.0;
            self.on_ground = false;
            self.ocelot.as_mut().unwrap().leaping = true;
            self.nav.stop();
            return Some((None, 0.0));
        }
        let reach = (self.shape().half_width * 4.0).powi(2);
        let modifier = if d > reach && d < 16.0 {
            1.33
        } else if d < 225.0 {
            0.6
        } else {
            0.8
        };
        let s = self.ocelot.as_mut().unwrap();
        s.crouching = modifier == 0.6;
        s.sprinting = modifier == 1.33;
        let speed = self.goal_speed(modifier);
        self.walk_to(w, at.floor().as_ivec3(), 0, speed, true);
        if d <= reach && self.attack_cooldown <= 0.0 {
            self.attack_cooldown = 1.0;
            events.push(EntityEvent::MobHit {
                target: uid,
                attacker: self.uid,
                damage: 3.0,
                knockback: flat.normalize_or_zero() * 8.0,
            });
        }
        Some((self.follow_path(w, speed, true), speed))
    }
}

impl Mob {
    pub(super) fn ocelot_land_tick<W: MobWorld + ?Sized>(&mut self, w: &W, wish: Option<DVec3>, speed: f64) {
        let mut dm = self.vel * 0.05;
        for v in [&mut dm.x, &mut dm.y, &mut dm.z] {
            if v.abs() < 0.003 {
                *v = 0.0;
            }
        }
        let input = if let Some(dir) = wish.filter(|_| self.alive()) {
            let target = dir.z.atan2(dir.x) as f32;
            self.yaw = turn_toward(self.yaw, target, std::f32::consts::FRAC_PI_2);
            self.move_ctl.cur = crate::entity::path::java_speed(speed) * self.effects.speed_factor();
            DVec3::Z * (self.move_ctl.cur * 0.98)
        } else {
            self.move_ctl.cur = 0.0;
            DVec3::ZERO
        };
        self.travel_in_air(w, &mut dm, input);
        self.vel = dm * 20.0;
        // No fall damage: ocelots are in Java's `fall_damage_immune` tag.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        physics::{BlockSource, test_util::Grid},
        world::{block::Block, terrain::Biome},
    };
    struct Flat(Grid);
    impl BlockSource for Flat {
        fn block(&self, p: IVec3) -> Option<Block> {
            self.0.block(p)
        }
    }
    impl MobWorld for Flat {
        fn loaded(&self, _: IVec3) -> bool {
            true
        }
        fn exposed(&self, _: IVec3) -> bool {
            true
        }
        fn surface(&self, _: i32, _: i32) -> Option<i32> {
            Some(0)
        }
        fn biome(&self, _: i32, _: i32) -> Biome {
            Biome::Forest
        }
    }
    #[test]
    fn ocelots_take_no_fall_damage() {
        let w = Flat(Grid::flat(1));
        let mut m = Mob::new(MobKind::Ocelot, DVec3::new(0.5, 30.0, 0.5), 0.0);
        for _ in 0..200 {
            m.ocelot_land_tick(&w, None, 0.0);
        }
        assert!(m.on_ground && m.pos.y < 2.0, "landed at {}", m.pos.y);
        assert_eq!(m.health, MobKind::Ocelot.max_health());
    }
    #[test]
    fn pandas_and_ocelots_walk_at_java_speed_squared_after_input_scaling() {
        let w = Flat(Grid::flat(1));
        let mut rng = Rng::new(74);
        for kind in [MobKind::Panda, MobKind::Ocelot] {
            for modifier in [0.6, 0.8, 1.0, 1.25, 1.33] {
                let mut m = Mob::new(kind, DVec3::new(0.5, 1.0, 0.5), 0.0);
                m.on_ground = true;
                let speed = m.goal_speed(modifier);
                let mut actual = 0.0;
                for _ in 0..160 {
                    let from = m.pos.x;
                    if kind == MobKind::Panda {
                        m.panda_land_tick(&w, Some(DVec3::X), speed, &mut rng);
                    } else {
                        m.ocelot_land_tick(&w, Some(DVec3::X), speed);
                    }
                    actual = (m.pos.x - from) * 20.0;
                }
                assert!(
                    (actual - speed * 0.98).abs() < 1e-6,
                    "{kind:?} modifier {modifier}: {actual} vs {}",
                    speed * 0.98
                );
            }
        }
    }
}
