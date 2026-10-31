//! Authoritative per-player gameplay state, independent of devices and presentation.
//!
//! Desktop, controller, LAN and CLI adapters own one of these for each player.
//! Camera, hand animation and screen navigation belong to their adapters.
pub mod combat;
pub mod consumption;
pub mod hands;
pub mod interact;
mod lodestone;
pub mod mining;
pub mod placement;
pub mod ranged;
mod wood;
use glam::{DVec3, IVec3};

use crate::{
    entity::{Entities, PlayerId},
    inventory::{Inventory, Stack},
    player::Player,
    rules::GameMode,
    simulation::survival::Vitals,
    world::block::Facing,
};

pub struct PlayerSession {
    pub id: PlayerId,
    pub player: Player,
    pub inventory: Inventory,
    pub vitals: Vitals,
    pub mode: GameMode,
    /// Transient workstation inputs are owned by the player, not a device.
    pub work: [Option<Stack>; 3],
    pub sleeping: Option<f32>,
    /// The bed `sleeping` lies in, so waking can stand the player up beside it.
    sleep_spot: Option<SleepSpot>,
    pub spawn_bed: Option<IVec3>,
    pub spawn_point: Option<IVec3>,
    /// The Nether respawn anchor last set as the respawn point (it replaces
    /// a bed, and a bed replaces it).
    pub spawn_anchor: Option<IVec3>,
    /// Damage of a stalactite that landed in this player's box during the
    /// last world tick, taken as it landed (as mobs are hit) and applied by
    /// the next vitals tick.
    pub stalactite: f32,
    /// The player's ender chest contents (`PlayerEnderChestContainer`),
    /// shared by every ender chest and saved with the player.
    pub ender: crate::world::chest::Chest,
    /// Respawns since this session started (not saved). LAN uses it to tell
    /// a client its body was replaced, whichever path respawned it.
    pub respawns: u32,
    combat: combat::State,
    ranged: ranged::State,
    consumption: consumption::State,
    hands: hands::State,
    mining: mining::State,
}

/// Where a sleeper lay down: the bed's head block and the spot on it.
#[derive(Clone, Copy)]
struct SleepSpot {
    head: IVec3,
    /// From the bed's foot to its head.
    facing: Facing,
    lie: DVec3,
}

/// How far a sleeper may drift from where they lay down (collision settles
/// them a hair above the bed) and still count as lying there; a teleport or
/// dimension change moves them further.
const LIE_DRIFT: f64 = 0.05;

/// The angle (Java degrees) a bed respawn assumes the player had when they set
/// it. Java stores that yaw with the spawn; VoxelCraft's saves hold only the
/// bed, and Java's own data fixer gives old spawns 0 as well.
const STORED_BED_ANGLE: f32 = 0.0;

/// Java's default spawn angle (yaw 0, facing south along +Z) in VoxelCraft's
/// yaw convention, where yaw 0 faces +X. Used for the world spawn and set
/// spawn points, which store no angle of their own.
pub const DEFAULT_SPAWN_YAW: f32 = std::f32::consts::FRAC_PI_2;

/// Personal spawn selection; adapters supply their existing world-spawn fallback.
pub struct PersonalSpawn {
    pub position: Option<DVec3>,
    /// The yaw to respawn with: Java's `RespawnPosAngle`, which looks at the
    /// bed from where the player stands, else [`DEFAULT_SPAWN_YAW`].
    pub yaw: f32,
    pub missing_bed: bool,
}

impl PlayerSession {
    /// `ServerExplosion.hurtEntities` for this player and a blast of `power`
    /// at `center`, with the blocks still standing: the damage before
    /// difficulty and armor, and the push in blocks per second. Spectators
    /// and flying Creative players aren't pushed (`hitPlayers`).
    pub fn blast<W: crate::physics::BlockSource + ?Sized>(
        &self,
        world: &W,
        center: DVec3,
        power: f32,
    ) -> Option<(f32, DVec3)> {
        let player = &self.player;
        if self.mode == GameMode::Spectator || player.pos.distance(center) > f64::from(power) * 2.0 {
            return None;
        }
        let (min, max) = player.collision_shape().aabb(player.pos);
        let exposure = crate::world::explosion::seen_percent(world, center, min, max);
        let (damage, knockback) = crate::entity::explosion_damage(power, player.pos.distance(center), exposure)?;
        let pushed = !(self.mode.is_creative() && player.flying);
        let resisted = knockback * (1.0 - self.inventory.explosion_knockback_resistance());
        let push = if pushed { (player.eye() - center).normalize_or_zero() * resisted } else { DVec3::ZERO };
        Some((damage, push * crate::knockback::TICK_RATE))
    }

    /// Default Java 26.3 block interaction range, including Creative's modifier.
    pub fn block_reach(&self) -> f64 {
        self.mode.block_reach()
    }

    /// Java 26.3 `Container.stillValidBlockEntity`: an open block screen stays
    /// usable while the block's cell is within block reach + 4 of the eye.
    pub fn can_keep_using_block(&self, pos: IVec3) -> bool {
        let range = self.block_reach() + 4.0;
        let min = pos.as_dvec3();
        crate::physics::box_distance_squared(self.player.eye(), min, min + DVec3::ONE) < range * range
    }

    /// Java 26.3 range for an open entity screen (vehicle storage, trading):
    /// entity reach + 4, measured to the entity's box.
    pub fn entity_screen_range(&self) -> f64 {
        self.entity_reach() + 4.0
    }

    pub fn block_target(&self, world: &crate::world::World) -> Option<(IVec3, IVec3)> {
        self.block_hit(world).map(|h| (h.cell, h.normal))
    }

    /// The targeted block with the exact point the crosshair meets it.
    pub fn block_hit(&self, world: &crate::world::World) -> Option<crate::world::BlockHit> {
        world.raycast_hit(self.player.eye(), self.player.forward().as_dvec3(), self.block_reach())
    }

    pub fn new(id: PlayerId, pos: DVec3) -> Self {
        Self {
            id,
            player: Player::new(pos),
            inventory: Inventory::default(),
            vitals: Vitals::default(),
            mode: GameMode::Survival,
            work: [None; 3],
            sleeping: None,
            sleep_spot: None,
            spawn_bed: None,
            spawn_point: None,
            spawn_anchor: None,
            stalactite: 0.0,
            ender: Default::default(),
            respawns: 0,
            combat: Default::default(),
            ranged: Default::default(),
            consumption: Default::default(),
            hands: Default::default(),
            mining: Default::default(),
        }
    }

    pub fn from_parts(id: PlayerId, player: Player, inventory: Inventory, vitals: Vitals, mode: GameMode) -> Self {
        Self {
            id,
            player,
            inventory,
            vitals,
            mode,
            work: [None; 3],
            sleeping: None,
            sleep_spot: None,
            spawn_bed: None,
            spawn_point: None,
            spawn_anchor: None,
            stalactite: 0.0,
            ender: Default::default(),
            respawns: 0,
            combat: Default::default(),
            ranged: Default::default(),
            consumption: Default::default(),
            hands: Default::default(),
            mining: Default::default(),
        }
    }

    /// Keep the mode and its movement abilities together for every adapter.
    pub fn set_mode(&mut self, mode: GameMode) {
        self.cancel_consumption();
        self.mode = mode;
        self.player.can_fly = mode.can_fly();
        self.player.noclip = mode == GameMode::Spectator;
        self.player.flying = mode == GameMode::Spectator || (self.player.flying && self.player.can_fly);
    }

    /// Java 26.3 `ServerPlayer.die`, shared by every adapter: actions stop;
    /// status effects stay until the respawn replaces the body. Unless the player is a spectator or
    /// `keepInventory` is on, the inventory, workstation inputs and `extra`
    /// (an adapter's crafting grid) drop where it died, except Curse of
    /// Vanishing items, with `min(7 × level, 100)` experience. Kept,
    /// `extra` goes back into the inventory as Java's closing grid does.
    /// Leaving a vehicle is the adapter's job.
    pub fn die(&mut self, entities: &mut Entities, keep_inventory: bool, extra: Vec<Stack>) {
        self.cancel_consumption();
        self.cancel_mining();
        self.sleeping = None;
        self.sleep_spot = None;
        let pos = self.player.pos;
        if keep_inventory || self.mode == GameMode::Spectator {
            for stack in extra {
                let left = self.inventory.add_stack(stack);
                if left > 0 {
                    entities.scatter(Stack { count: left, ..stack }, pos);
                }
            }
            return;
        }
        let stacks = self.inventory.take_all().into_iter().chain(self.work.iter_mut().filter_map(Option::take));
        for stack in stacks.chain(extra) {
            if !stack.active_enchants().has(crate::enchant::Enchantment::VanishingCurse) {
                entities.scatter(stack, pos);
            }
        }
        let xp = self.vitals.xp.die();
        entities.spawn_xp(pos, xp);
    }

    /// Java's respawn (`ServerPlayer.restoreFrom` after a death): a fresh
    /// body at `at` with full health and food, no effects, fire or fall, and
    /// the mode's abilities. Experience and the death-screen score carry
    /// over only with `keepInventory` or for a spectator
    /// (`transferInventoryXpAndScore`); otherwise only the enchantment seed
    /// does. The spawn bed and point are kept. The new body faces `yaw` with
    /// level pitch, as Java's respawn transition applies the respawn angle.
    pub fn respawn(&mut self, at: DVec3, yaw: f32, keep_inventory: bool) {
        self.cancel_consumption();
        self.cancel_mining();
        // A stalactite that hit the body before respawning stays with it.
        self.stalactite = 0.0;
        self.respawns = self.respawns.wrapping_add(1);
        self.player = Player::new(at);
        self.player.yaw = yaw.rem_euclid(std::f32::consts::TAU);
        self.player.pitch = 0.0;
        self.player.can_fly = self.mode.can_fly();
        self.player.noclip = self.mode == GameMode::Spectator;
        self.player.flying = self.mode == GameMode::Spectator;
        let xp = self.vitals.xp;
        self.vitals.respawn();
        if keep_inventory || self.mode == GameMode::Spectator {
            self.vitals.xp = xp;
        } else {
            self.vitals.xp.seed = xp.seed;
        }
        self.sleeping = None;
        self.sleep_spot = None;
        self.combat = Default::default();
        self.ranged = Default::default();
        self.hands = Default::default();
    }

    /// Lie down at `at` on the bed with this head block and foot-to-head
    /// direction (none for a bed that lost its other half), stopping any
    /// movement.
    pub fn begin_sleep(&mut self, bed: Option<(IVec3, Facing)>, at: DVec3) {
        self.player.pos = at;
        self.player.vel = DVec3::ZERO;
        self.player.flying = false;
        self.sleeping = Some(0.0);
        self.sleep_spot = bed.map(|(head, facing)| SleepSpot { head, facing, lie: at });
    }

    /// Get out of bed now (`LivingEntity.stopSleeping`).
    pub fn wake(&mut self, world: &crate::world::World) {
        self.sleeping = None;
        self.settle_wake(world);
    }

    /// Java's `LivingEntity.stopSleeping` position change, for wake-ups that
    /// only clear `sleeping` (damage, jumping, a command): a player still
    /// lying where they lay down moves to the bed's stand-up position and
    /// looks at the bed, level. The spot is the first free one round the bed,
    /// else just above the head; a bed that is gone leaves the player put.
    pub fn settle_wake(&mut self, world: &crate::world::World) {
        use crate::world::dismount;
        if self.sleeping.is_some() {
            return;
        }
        let Some(spot) = self.sleep_spot.take().filter(|s| s.lie.distance(self.player.pos) < LIE_DRIFT) else { return };
        if !world.get_block(spot.head).is_some_and(dismount::is_sleep_head) {
            return;
        }
        let java_yaw = dismount::java_yaw_degrees(self.player.yaw);
        let (stand, yaw) = dismount::bed_stand_up(world, spot.head, spot.facing, java_yaw).unwrap_or_else(|| {
            let above = spot.head + IVec3::Y;
            let stand = DVec3::new(above.x as f64 + 0.5, above.y as f64 + 0.1, above.z as f64 + 0.5);
            (stand, dismount::look_at_bed(stand, spot.head))
        });
        self.player.pos = stand;
        self.player.yaw = yaw.rem_euclid(std::f32::consts::TAU);
        self.player.pitch = 0.0;
    }

    /// Java's `ServerPlayer.findRespawnAndUseSpawnBlock` for a bed: the spot
    /// beside the bed's head where `AbstractBedBlock.findStandUpPosition`
    /// finds room, facing the bed. A bed that is gone, or boxed in so nobody
    /// fits, is the missing-bed outcome: the spawn is forgotten and the
    /// adapter falls back to the world spawn.
    pub fn personal_spawn(&mut self, world: &crate::world::World) -> PersonalSpawn {
        use crate::world::{dismount, terrain::Dimension};
        if world.generator.dimension != Dimension::Overworld {
            return PersonalSpawn { position: None, yaw: DEFAULT_SPAWN_YAW, missing_bed: false };
        }
        let mut missing_bed = false;
        if let Some(foot) = self.spawn_bed {
            // VoxelCraft stores the foot; Java stores (and checks) the head.
            let bed = world
                .get_block(foot)
                .filter(|b| b.is_bed() && !b.is_bed_head())
                .and_then(|half| world.bed_partner(foot, half))
                .and_then(|head| dismount::bed_orientation(foot, head));
            if let Some((position, yaw)) =
                bed.and_then(|(head, facing)| dismount::bed_stand_up(world, head, facing, STORED_BED_ANGLE))
            {
                return PersonalSpawn { position: Some(position), yaw, missing_bed: false };
            }
            self.spawn_bed = None;
            missing_bed = true;
        }
        PersonalSpawn {
            position: self.spawn_point.map(|point| point.as_dvec3() + DVec3::new(0.5, 0., 0.5)),
            yaw: DEFAULT_SPAWN_YAW,
            missing_bed,
        }
    }
}
