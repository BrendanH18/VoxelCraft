//! Java 26.3 `DoorInteractGoal` and `BreakDoorGoal` (v1.0 gap Track 2),
//! shared by zombies (`Zombie.breakDoorGoal`: Hard only, added at priority 1
//! with no flags while `CanBreakDoors` is set) and vindicators
//! (`VindicatorBreakDoorGoal`: Normal or Hard in an active raid, a
//! `reducedTickDelay(10)` start roll and the `MOVE` flag).
//!
//! `DoorInteractGoal.canUse`: a mob with ground navigation, pressed against
//! something (`horizontalCollision`), whose unfinished path has a wooden
//! door one block above one of its nodes up to the one after next within
//! 1.5 blocks (measured to the door cell's corner, as Java does), or above
//! its own block. `BreakDoorGoal` then needs `mobGriefing`, a valid
//! difficulty and a closed door. While it runs (every tick:
//! `requiresUpdateEveryTick`) it plays `zombie.attack_wooden_door` and
//! swings one tick in 20, shows the crack stage `breakTime / 240 × 10`, and
//! at tick 240 removes the door cell: the half above the path node goes
//! silently (`removeBlock`), and the other half breaks through its shape
//! update (`destroyBlock`, with particles, break sound and, for the lower
//! half, the door item), followed by `zombie.break_wooden_door`. It stops
//! when the door opens, the mob is 2 blocks from the cell's centre, the
//! difficulty no longer allows it, or after tick 240.
use glam::{DVec3, IVec3};

use super::{Mob, MobWorld, Rng};
use crate::entity::{EntityEvent, MobKind, MobSound, illager::Cue};
use crate::simulation::difficulty::Difficulty;
use crate::world::block::{Block, Shaped};

/// `BreakDoorGoal.DEFAULT_DOOR_BREAK_TIME` (`getDoorBreakTime` is never
/// shorter: the vindicator's 6 gives 240 too).
pub use crate::entity::illager::DOOR_BREAK_TIME;

/// A running `BreakDoorGoal` (never saved, as goals aren't).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DoorBreak {
    /// `doorPos`: the cell one above the path node.
    pub cell: IVec3,
    /// `doorOpenDirX`/`doorOpenDirZ` at the start, for `passed`.
    open_dir: (f32, f32),
    /// `DoorInteractGoal.passed` (kept for parity; `BreakDoorGoal`'s
    /// `canContinueToUse` doesn't read it).
    pub passed: bool,
    /// `breakTime`.
    pub time: u32,
    /// `lastBreakProgress`: the crack stage shown (`destroyBlockProgress`).
    pub progress: i32,
}

/// `DoorBlock.isWoodenDoor`: any door a hand opens (all but iron doors).
pub(crate) fn wooden_door(b: Block) -> bool {
    b.is_door()
        && !matches!(
            crate::world::redstone_blocks::component(b),
            Some(crate::world::redstone_blocks::Component::IronDoor { .. })
        )
}

/// `Zombie` and its subclasses: zombies, husks, drowned, zombie villagers
/// and zombified piglins. All have ground or amphibious navigation, so all
/// can take the door goal (`canNavigateGround`).
pub(crate) fn zombie_family(kind: MobKind) -> bool {
    kind.is_zombie() || kind == MobKind::ZombifiedPiglin
}

/// The door's `OPEN` property.
fn door_open(b: Block) -> bool {
    matches!(b.shaped(), Some(Shaped::Door { open: true, .. }))
}

impl Mob {
    /// The difficulty predicate of this mob's `BreakDoorGoal`.
    fn door_difficulty_ok(&self) -> bool {
        match self.kind {
            MobKind::Vindicator => crate::entity::illager::breaks_doors(
                self.difficulty,
                self.illager.as_ref().is_some_and(|s| s.raider.raid_active),
            ),
            _ => self.difficulty == Difficulty::Hard,
        }
    }

    /// Whether this mob has a `BreakDoorGoal`: a zombie of any kind with
    /// `CanBreakDoors` (`setCanBreakDoors` needs `canNavigateGround`, true
    /// for ground and amphibious navigation), or a vindicator.
    pub(crate) fn has_door_goal(&self) -> bool {
        match self.kind {
            MobKind::Vindicator => true,
            k => zombie_family(k) && self.attrs.can_break_doors,
        }
    }

    /// `DoorInteractGoal.canUse` and `BreakDoorGoal.canUse`; returns the
    /// door cell.
    fn door_can_use<W: MobWorld + ?Sized>(&self, world: &W) -> Option<IVec3> {
        if !self.blocked || !self.griefing || !self.door_difficulty_ok() {
            return None;
        }
        let path = self.nav.path.as_ref().filter(|p| !p.is_done())?;
        let door = |c: IVec3| world.block(c).filter(|&b| wooden_door(b));
        let found = path.nodes[..(path.next + 2).min(path.nodes.len())].iter().find_map(|node| {
            let cell = node.pos + IVec3::Y;
            let d = DVec3::new(f64::from(cell.x), self.pos.y, f64::from(cell.z)) - self.pos;
            (d.length_squared() <= 2.25).then_some(cell).filter(|&c| door(c).is_some())
        });
        let cell = found.or_else(|| {
            let cell = self.pos.floor().as_ivec3() + IVec3::Y;
            door(cell).map(|_| cell)
        })?;
        // `BreakDoorGoal.canUse`: `!isOpen()`.
        door(cell).filter(|&b| !door_open(b)).map(|_| cell)
    }

    /// One tick of the door goal: start it on a full selector update when it
    /// can, run it every tick, and stop it when it can't continue. Returns
    /// whether it is running (a vindicator's holds `MOVE`).
    pub(crate) fn door_goal<W: MobWorld + ?Sized>(
        &mut self,
        world: &W,
        full: bool,
        rng: &mut Rng,
        events: &mut Vec<EntityEvent>,
    ) -> bool {
        if !self.has_door_goal() {
            self.door_break = None;
            return false;
        }
        if let Some(goal) = self.door_break {
            // `BreakDoorGoal.canContinueToUse`, checked on full updates.
            let block = world.block(goal.cell);
            let open = block.is_some_and(|b| wooden_door(b) && door_open(b));
            let centre = goal.cell.as_dvec3() + DVec3::splat(0.5);
            let keep = goal.time <= DOOR_BREAK_TIME
                && !open
                && centre.distance_squared(self.pos) < 4.0
                && self.door_difficulty_ok();
            if full && !keep {
                // `stop`: `destroyBlockProgress(id, doorPos, -1)`.
                self.door_break = None;
            }
        }
        if self.door_break.is_none() && full {
            // The vindicator's own roll comes before `super.canUse`.
            let roll = self.kind != MobKind::Vindicator || rng.next_int(super::selector::reduced(10) as u32) == 0;
            if roll && let Some(cell) = self.door_can_use(world) {
                let dir = cell.as_dvec3() + DVec3::new(0.5, 0.0, 0.5) - self.pos;
                self.door_break = Some(DoorBreak {
                    cell,
                    open_dir: (dir.x as f32, dir.z as f32),
                    passed: false,
                    time: 0,
                    progress: -1,
                });
            }
        }
        let Some(mut goal) = self.door_break else { return false };
        // `DoorInteractGoal.tick`.
        let now = goal.cell.as_dvec3() + DVec3::new(0.5, 0.0, 0.5) - self.pos;
        if goal.open_dir.0 * (now.x as f32) + goal.open_dir.1 * (now.z as f32) < 0.0 {
            goal.passed = true;
        }
        // `BreakDoorGoal.tick`.
        if rng.next_int(20) == 0 {
            events.push(EntityEvent::Sound { sound: MobSound::Illager(Cue::DoorAttack), pos: centre_of(goal.cell) });
            if self.attack_anim <= 0.0 {
                self.attack_anim = 0.35;
            }
        }
        goal.time += 1;
        goal.progress = (goal.time as f32 / DOOR_BREAK_TIME as f32 * 10.0) as i32;
        if goal.time == DOOR_BREAK_TIME && self.door_difficulty_ok() {
            events.push(EntityEvent::DoorBroken { cell: goal.cell });
        }
        self.door_break = Some(goal);
        true
    }

    /// The crack this mob's door goal shows (`destroyBlockProgress`), as a
    /// fraction for the crack overlay: stage `progress` of 0–9.
    pub fn door_crack(&self) -> Option<(IVec3, f32)> {
        let goal = self.door_break?;
        (0..10).contains(&goal.progress).then(|| (goal.cell, (goal.progress as f32 + 0.5) / 10.0))
    }
}

fn centre_of(cell: IVec3) -> DVec3 {
    cell.as_dvec3() + DVec3::splat(0.5)
}
