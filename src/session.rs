//! Authoritative per-player gameplay state, independent of devices and presentation.
//!
//! Desktop, controller, LAN and CLI adapters own one of these for each player.
//! Camera, hand animation and screen navigation belong to their adapters.
pub mod combat;
pub mod consumption;
pub mod interact;
pub mod mining;
pub mod placement;
pub mod ranged;
use glam::{DVec3, IVec3};

use crate::{
    entity::{Entities, PlayerId},
    inventory::{Inventory, Stack},
    player::Player,
    rules::GameMode,
    simulation::survival::Vitals,
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
    pub spawn_bed: Option<IVec3>,
    pub spawn_point: Option<IVec3>,
    combat: combat::State,
    ranged: ranged::State,
    consumption: consumption::State,
    mining: mining::State,
}

/// Personal spawn selection; adapters supply their existing world-spawn fallback.
pub struct PersonalSpawn {
    pub position: Option<DVec3>,
    pub missing_bed: bool,
}

impl PlayerSession {
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
        world.raycast(self.player.eye(), self.player.forward().as_dvec3(), self.block_reach())
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
            spawn_bed: None,
            spawn_point: None,
            combat: Default::default(),
            ranged: Default::default(),
            consumption: Default::default(),
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
            spawn_bed: None,
            spawn_point: None,
            combat: Default::default(),
            ranged: Default::default(),
            consumption: Default::default(),
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

    /// Java 26.3 `ServerPlayer.die`, shared by every adapter: actions stop
    /// and status effects end; unless the player is a spectator or
    /// `keepInventory` is on, the inventory, workstation inputs and `extra`
    /// (an adapter's crafting grid) drop where it died, except Curse of
    /// Vanishing items, with `min(7 × level, 100)` experience. Kept,
    /// `extra` goes back into the inventory as Java's closing grid does.
    /// Leaving a vehicle is the adapter's job.
    pub fn die(&mut self, entities: &mut Entities, keep_inventory: bool, extra: Vec<Stack>) {
        self.cancel_consumption();
        self.cancel_mining();
        self.sleeping = None;
        self.vitals.effects.clear();
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
    /// the mode's abilities. Experience carries over as death left it (kept
    /// with the inventory, otherwise reset but for the enchantment seed),
    /// and so do the spawn bed and point. The look direction is kept.
    pub fn respawn(&mut self, at: DVec3) {
        self.cancel_consumption();
        self.cancel_mining();
        let (yaw, pitch) = (self.player.yaw, self.player.pitch);
        self.player = Player::new(at);
        self.player.yaw = yaw;
        self.player.pitch = pitch;
        self.player.can_fly = self.mode.can_fly();
        self.player.noclip = self.mode == GameMode::Spectator;
        self.player.flying = self.mode == GameMode::Spectator;
        let xp = self.vitals.xp;
        self.vitals.respawn();
        self.vitals.xp = xp;
        self.sleeping = None;
        self.combat = Default::default();
        self.ranged = Default::default();
    }

    pub fn personal_spawn(&mut self, world: &crate::world::World) -> PersonalSpawn {
        use crate::world::{block::Block, terrain::Dimension};
        if world.generator.dimension != Dimension::Overworld {
            return PersonalSpawn { position: None, missing_bed: false };
        }
        let mut missing_bed = false;
        if let Some(foot) = self.spawn_bed {
            if world.get_block(foot).is_some_and(|b| b.is_bed() && !b.is_bed_head()) {
                return PersonalSpawn {
                    position: Some(foot.as_dvec3() + DVec3::new(0.5, Block::BED_FOOT.height(), 0.5)),
                    missing_bed: false,
                };
            }
            self.spawn_bed = None;
            missing_bed = true;
        }
        PersonalSpawn {
            position: self.spawn_point.map(|point| point.as_dvec3() + DVec3::new(0.5, 0., 0.5)),
            missing_bed,
        }
    }
}
