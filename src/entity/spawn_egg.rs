//! v0.18 Track 4: what a spawn egg does (Java 26.3 `SpawnEggItem`,
//! `EntityType.spawn` and `SpawnEggItemBehavior`). The items themselves are
//! in `content::spawn_egg`; the player's use chain (`session::interact`)
//! and the dispenser (`world::dispensing`) call in here.
//!
//! - `EntityType.spawn(level, stack, user, pos, reason, tryMoveDown,
//!   movedUp)`: the mob stands at the centre of `pos`; with `tryMoveDown`
//!   it is first put a block higher and dropped onto whatever collision
//!   shapes are in that cell (and the cell below when `movedUp`), at most
//!   one block (two when `movedUp`): `getYOffset`.
//! - `EntityType.canSpawn`: types not allowed in Peaceful fail there.
//! - `SpawnEggItem.spawnOffspringFromSpawnEgg`: an egg used on a mob of its
//!   own type makes a baby at that mob's feet, facing south, when the type
//!   can be one.

use glam::{DVec3, IVec3};

use super::{Entities, Mob, MobKind};
use crate::content::spawn_egg::{self, Baby, Spawns};
use crate::physics::BlockSource;
use crate::simulation::difficulty::Difficulty;

/// `AgeableMob.BABY_START_AGE`, which `setBaby(true)` gives.
pub const BABY_START_AGE: i32 = -24000;

/// `EntityType.getYOffset`: how far above `cell`'s floor a mob of `kind`
/// lands when `EntityType.create` is told to try moving it down.
pub fn y_offset<W: BlockSource + ?Sized>(world: &W, kind: MobKind, cell: IVec3, moved_up: bool) -> f64 {
    y_offset_for(world, kind.shape(), cell, moved_up)
}

/// [`y_offset`] for an entity of `shape`.
pub fn y_offset_for<W: BlockSource + ?Sized>(
    world: &W,
    shape: crate::physics::Shape,
    cell: IVec3,
    moved_up: bool,
) -> f64 {
    let base = cell.as_dvec3();
    // The cell, stretched one block down when the egg went on a top face.
    let (lo, hi) = (base - DVec3::Y * if moved_up { 1.0 } else { 0.0 }, base + DVec3::ONE);
    let mut colliders = Vec::new();
    crate::physics::colliders(world, lo, hi, &mut colliders);
    // The mob's box, set a block higher (`setPos(x + 0.5, y + 1, z + 0.5)`).
    let (min, max) = shape.aabb(base + DVec3::new(0.5, 1.0, 0.5));
    1.0 + crate::physics::collide_axis(&colliders, min, max, 1, if moved_up { -2.0 } else { -1.0 })
}

impl Entities {
    /// `EntityType.canSpawn` for a spawn egg's mob.
    pub fn egg_can_spawn(&self, kind: MobKind) -> bool {
        self.difficulty != Difficulty::Peaceful
            || spawn_egg::EGGS.iter().find(|e| e.kind == kind).is_none_or(|e| e.peaceful)
    }

    /// Whether a spawn egg of `spawns` can spawn (`EntityType.canSpawn`).
    pub fn egg_spawns_allowed(&self, spawns: Spawns) -> bool {
        match spawns {
            Spawns::Mob(kind) => self.egg_can_spawn(kind),
            Spawns::EnderDragon => self.difficulty != Difficulty::Peaceful || spawn_egg::ENDER_DRAGON_PEACEFUL,
        }
    }

    /// [`Entities::spawn_from_egg`] for any egg: returns where the mob or
    /// dragon stands.
    pub fn spawn_egg_spawns<W: BlockSource + ?Sized>(
        &mut self,
        world: &W,
        spawns: Spawns,
        cell: IVec3,
        try_move_down: bool,
        moved_up: bool,
    ) -> Option<DVec3> {
        match spawns {
            Spawns::Mob(kind) => {
                self.spawn_from_egg(world, kind, cell, try_move_down, moved_up).map(|i| self.mobs[i].pos)
            }
            Spawns::EnderDragon => self.spawn_dragon_from_egg(world, cell, try_move_down, moved_up),
        }
    }

    /// v1.0 Track 7: `EntityType.spawn` for the Ender Dragon's egg: a
    /// dragon no fight owns, hovering at the centre of `cell` (dropped as
    /// [`y_offset_for`] says), facing a random way (`EntityType.create`
    /// snaps it to `wrapDegrees(random * 360)`), then its ambient growl.
    pub fn spawn_dragon_from_egg<W: BlockSource + ?Sized>(
        &mut self,
        world: &W,
        cell: IVec3,
        try_move_down: bool,
        moved_up: bool,
    ) -> Option<DVec3> {
        if !self.egg_spawns_allowed(Spawns::EnderDragon) {
            return None;
        }
        let y = if try_move_down { y_offset_for(world, super::dragon::WILD_SHAPE, cell, moved_up) } else { 0.0 };
        let at = DVec3::new(cell.x as f64 + 0.5, cell.y as f64 + y, cell.z as f64 + 0.5);
        let yaw = self.rng.next_f32() * 360.0;
        self.wild_dragons.spawn(at, yaw);
        self.pending_sounds.push(super::EntityEvent::Sound { sound: super::MobSound::DragonGrowl, pos: at });
        Some(at)
    }

    /// `EntityType.spawn` with a spawn egg's reasons (`SPAWN_ITEM_USE` or
    /// `DISPENSER`): the mob of `kind` at the centre of `cell`, dropped as
    /// [`y_offset`] says when `try_move_down`. Returns its index, or `None`
    /// when the type can't spawn here (Peaceful).
    pub fn spawn_from_egg<W: BlockSource + ?Sized>(
        &mut self,
        world: &W,
        kind: MobKind,
        cell: IVec3,
        try_move_down: bool,
        moved_up: bool,
    ) -> Option<usize> {
        if !self.egg_can_spawn(kind) {
            return None;
        }
        let y = if try_move_down { y_offset(world, kind, cell, moved_up) } else { 0.0 };
        let at = DVec3::new(cell.x as f64 + 0.5, cell.y as f64 + y, cell.z as f64 + 0.5);
        self.spawn(kind, at);
        // `EntityType.spawn` then `Mob.playAmbientSound`.
        self.pending_sounds.push(super::EntityEvent::Sound { sound: super::MobSound::Ambient(kind), pos: at });
        Some(self.mobs.len() - 1)
    }

    /// `SpawnEggItem.spawnOffspringFromSpawnEgg`: `egg` used on mob `index`.
    /// A mob of the egg's own type that can be a baby gets one at its feet;
    /// returns the baby's index, or `None` (the use passes on).
    pub fn spawn_baby_from_egg(&mut self, index: usize, egg: crate::item::Item) -> Option<usize> {
        let e = spawn_egg::egg_of(egg)?;
        let parent = self.mobs.get(index).filter(|m| m.alive() && m.kind == e.kind)?;
        let (kind, pos) = (parent.kind, parent.pos);
        let mut baby = match e.baby {
            Baby::None => return None,
            // `getBreedOffspring(level, parent)`: the parent stands in for
            // both mates, so its variant, owner and colours carry over.
            Baby::Breed if kind.is_breedable() => self.animal_child(index, index),
            // Other ageable mobs and the `setBaby` mobs: a fresh entity
            // (`EntityType.create`, without `finalizeSpawn`'s gear rolls).
            Baby::Breed | Baby::SetBaby => {
                self.spawn(kind, pos);
                let mut m: Mob = self.mobs.pop().expect("spawn pushes the mob");
                m.equipment = Default::default();
                m
            }
        };
        // `snapTo(pos, 0, 0)` and `setBaby(true)`.
        baby.pos = pos;
        baby.yaw = 0.0;
        baby.baby = true;
        if e.baby == Baby::Breed || kind == MobKind::ZombieVillager {
            baby.age = BABY_START_AGE;
        }
        self.mobs.push(baby);
        Some(self.mobs.len() - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::test_util::Grid;

    #[test]
    fn eggs_drop_mobs_onto_the_floor_below() {
        // Stone below y = 64.
        let mut w = Grid::flat(64);
        // A top face: the cell above the floor, stretched down a block.
        assert_eq!(y_offset(&w, MobKind::Pig, IVec3::new(0, 64, 0), true), 0.0);
        // A side face against a wall, one block above the floor: no
        // further than a block down, so it lands on the floor.
        assert_eq!(y_offset(&w, MobKind::Pig, IVec3::new(0, 65, 0), false), 0.0);
        // A bottom slab in the cell: the mob stands on it.
        w.set(IVec3::new(0, 64, 0), crate::world::block::Block::STONE_SLAB);
        assert_eq!(y_offset(&w, MobKind::Pig, IVec3::new(0, 64, 0), false), 0.5);
    }
}
