//! Bell hearing within 32 blocks interrupts work and sends residents home.
use super::{Entities, EntityEvent, MobKind, MobSound};
use glam::{DVec3, IVec3};
impl Entities {
    pub fn ring_bell(&mut self, pos: IVec3) {
        let center = pos.as_dvec3() + DVec3::splat(0.5);
        self.pending_sounds.push(EntityEvent::Sound { sound: MobSound::Bell, pos: center });
        self.mob_index.rebuild(&self.mobs);
        self.mob_index.visit(center, 32.0, |i| {
            let m = &mut self.mobs[i];
            if m.kind == MobKind::Villager
                && m.alive()
                && m.pos.distance_squared(center) < 1024.0
                && let Some(v) = &mut m.villager
            {
                v.bell_hide = 15.0;
                v.sleeping = false;
                v.trading = false;
                v.goal = v.home.map(|p| p.as_dvec3() + DVec3::new(0.5, 0.6, 0.5));
            }
        });
    }
}
