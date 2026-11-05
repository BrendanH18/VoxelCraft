//! Atomic transitions between a living golem and its named statue.
use super::*;
use crate::world::{
    block::Facing,
    copper_golem_statue::{self as statue, Pose, Statue},
};

impl Entities {
    /// Java BehaviorUtils.throwItem: a directed toss from eye height, with the
    /// ordinary ten-tick pickup delay rather than a player's drop impulse.
    pub fn return_copper_golem_cargo(&mut self, index: usize, target: DVec3) -> bool {
        let Some(mob) = self.mobs.get_mut(index).filter(|m| m.alive()) else { return false };
        let Some(stack) = mob.copper_golem.as_mut().and_then(|s| s.take_cargo()) else { return false };
        let pos = mob.pos + DVec3::Y * (0.8125 - f64::from(0.3f32));
        let vel = (target - mob.pos).normalize_or_zero() * (f64::from(0.3f32) * 20.0);
        self.items.push(super::super::ItemEntity::new(stack, pos, vel, 0.5, &mut self.rng));
        true
    }

    /// Entity.spawnAtLocation gives preserved equipment its ordinary item
    /// impulse and pickup delay. `pos` includes the caller's vertical offset.
    pub fn drop_copper_golem_equipment(&mut self, stack: Stack, pos: DVec3) {
        let vel = DVec3::new(self.rng.range(-2.0, 2.0) as f64, 4.0, self.rng.range(-2.0, 2.0) as f64);
        self.items.push(super::super::ItemEntity::new(stack, pos, vel, 0.5, &mut self.rng));
    }

    pub fn copper_golem_sound(&mut self, pos: DVec3, sound: Sound, stage: Stage) {
        self.pending_sounds.push(EntityEvent::Sound { sound: MobSound::CopperGolem(sound, stage as u8), pos });
    }

    /// Java creates a fresh golem; only the block's custom name survives.
    pub fn revive_copper_golem(&mut self, world: &mut World, p: IVec3) -> bool {
        let Some(s) = world.get_block(p).and_then(statue::decode) else { return false };
        if s.waxed || s.stage != Stage::Unaffected {
            return false;
        }
        let name = world.copper_golem_statue_name(p);
        if !world.set_block(p, Block::AIR) {
            return false;
        }
        let index = self.spawn_copper_golem(p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5), s.stage, false);
        let mob = &mut self.mobs[index];
        mob.name = name;
        let direction = s.facing.offset();
        mob.yaw = (direction.z as f32).atan2(direction.x as f32);
        true
    }

    /// Accept weathering only once the destination can be replaced. Cargo and
    /// antenna equipment drop intact; the statue keeps only the custom name.
    pub fn finish_copper_statues(&mut self, world: &mut World) {
        let mut i = 0;
        while i < self.mobs.len() {
            let m = &self.mobs[i];
            let p = m.pos.floor().as_ivec3();
            if !m.alive()
                || !world.is_entity_ticking(p)
                || !m.copper_golem.as_ref().is_some_and(|s| s.statue_ready && s.stage == Stage::Oxidized && !s.waxed())
                || world.get_block(p) != Some(Block::AIR)
            {
                i += 1;
                continue;
            }
            let facing = Facing::toward(glam::Vec3::new(m.yaw.cos(), 0.0, m.yaw.sin())).opposite();
            let name = m.name;
            let s = Statue {
                facing,
                pose: Pose::ALL[self.rng.next_int(4) as usize],
                ..Statue::new(Stage::Oxidized, false)
            };
            if !world.set_block(p, statue::block(s)) {
                i += 1;
                continue;
            }
            world.name_copper_golem_statue(p, name);
            let mut mob = self.mobs.swap_remove(i);
            let state = mob.copper_golem.as_mut().unwrap();
            for stack in [state.cargo.take(), state.antenna.take()].into_iter().flatten() {
                self.drop_copper_golem_equipment(stack, mob.pos);
            }
            if mob.leash.is_some() && self.entity_drops {
                self.drop_copper_golem_equipment(Stack::new(crate::item::Item::LEAD, 1), mob.pos);
            }
            self.copper_golem_sound(mob.pos, Sound::Statue, Stage::Oxidized);
        }
        // Conversion cannot leave a chest open on behalf of the removed mob.
        world.set_golem_chest_viewers(
            self.mobs.iter().filter(|m| m.alive()).filter_map(|m| m.copper_golem.as_ref().and_then(|s| s.opened_chest)),
        );
    }
}
