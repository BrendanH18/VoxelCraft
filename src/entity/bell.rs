//! Bell hearing within 32 blocks interrupts work and sends residents home.
use super::{Entities, EntityEvent, MobKind, MobSound};
use glam::{DVec3, IVec3};
/// Seconds with nobody near before a door a villager opened swings shut.
const DOOR_CLOSE_DELAY: f32 = 1.0;
/// Horizontal reach of "still passing through" (Java's door-close check is the path's next node).
const DOOR_CLEARANCE: f64 = 1.6;
impl Entities {
    /// Java's `InteractWithDoor` closes a door once the villager has passed. Doors a
    /// villager opened are watched until no villager or trader stands within
    /// [`DOOR_CLEARANCE`] for [`DOOR_CLOSE_DELAY`] seconds; then they get a close event.
    pub(super) fn watch_doors(&mut self, dt: f32, events: &mut Vec<EntityEvent>) {
        for e in events.iter() {
            if let EntityEvent::VillagerDoor { cell } = *e
                && self.door_watch.len() < 64
                && !self.door_watch.iter().any(|d| d.0 == cell)
            {
                self.door_watch.push((cell, 0.0));
            }
        }
        if self.door_watch.is_empty() {
            return;
        }
        let mut watch = std::mem::take(&mut self.door_watch);
        watch.retain_mut(|(cell, idle)| {
            let center = cell.as_dvec3() + DVec3::new(0.5, 1.0, 0.5);
            let mut near = false;
            self.mob_index.visit(center, DOOR_CLEARANCE + 1.0, |i| {
                let m = &self.mobs[i];
                near |= matches!(m.kind, MobKind::Villager | MobKind::WanderingTrader)
                    && m.alive()
                    && (m.pos - center).abs().max_element() < DOOR_CLEARANCE
                    && (m.pos.y - cell.y as f64).abs() < 2.0;
            });
            *idle = if near { 0.0 } else { *idle + dt };
            if *idle < DOOR_CLOSE_DELAY {
                return true;
            }
            events.push(EntityEvent::VillagerDoorClose { cell: *cell });
            false
        });
        self.door_watch = watch;
    }
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn doors_villagers_opened_close_once_nobody_is_near() {
        let mut e = Entities::new(1);
        let door = IVec3::new(5, 10, 5);
        e.spawn(MobKind::Villager, DVec3::new(5.5, 10.0, 5.5));
        e.mob_index.rebuild(&e.mobs);
        let closes = |events: &[EntityEvent]| {
            events.iter().filter(|ev| matches!(ev, EntityEvent::VillagerDoorClose { cell } if *cell == door)).count()
        };
        let mut opened = vec![EntityEvent::VillagerDoor { cell: door }];
        e.watch_doors(0.5, &mut opened);
        let mut events = Vec::new();
        for _ in 0..4 {
            e.watch_doors(0.5, &mut events);
        }
        assert_eq!(closes(&events), 0, "still in the doorway");
        e.mobs[0].pos = DVec3::new(9.5, 10.0, 5.5);
        e.mob_index.rebuild(&e.mobs);
        e.watch_doors(0.6, &mut events);
        assert_eq!(closes(&events), 0, "one second grace");
        e.watch_doors(0.6, &mut events);
        assert_eq!(closes(&events), 1);
        e.watch_doors(5.0, &mut events);
        assert_eq!(closes(&events), 1, "closed once");
    }
}
