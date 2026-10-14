//! Actor-owned right-click ("use") chain shared by every input path.
//!
//! The order follows the desktop chain retained from 0.6, which tracks Java's
//! entity interaction, then block use (skipped while sneaking), then item use.
//! Screens, beds and sleeping stay with the adapter: the chain reports them
//! as [`Used::Screen`], [`Used::Bed`], [`Used::OpenVehicle`] and
//! [`Used::OpenMerchant`] for the adapter to present.
use super::PlayerSession;
use crate::{
    entity::{Entities, PlayerId},
    inventory::HOTBAR_SLOTS,
    item::{Item, ToolKind},
    physics, survival_items,
    survival_items::bucket,
    world::{
        World,
        block::{Block, Facing, Shaped},
        terrain::Dimension,
    },
};
use glam::{DVec3, IVec3};

/// What a use did. Adapters play sounds, show popups and open screens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Used {
    /// A ridden mount was boosted (carrot/fungus on a stick).
    Boosted,
    /// Ridden or targeted vehicle storage should open.
    OpenVehicle(u32),
    /// A lead, name tag, food or taming item was used on an animal.
    Animal,
    /// The player now rides this vehicle.
    Mounted(u32),
    CartPlaced,
    /// A villager with offers was targeted; its trading screen should open.
    OpenMerchant(u64),
    /// A villager with no offers was targeted; nothing else happens.
    MerchantRefused,
    Sheep {
        sheared: bool,
    },
    /// Another held-item interaction with a mob (bucketing, milking, ...).
    Mob,
    Bucket(bucket::Effect),
    PearlThrown {
        from: DVec3,
    },
    PotionThrown {
        from: DVec3,
    },
    EyeReleased {
        from: DVec3,
    },
    ProjectileThrown {
        from: DVec3,
    },
    Rod {
        from: DVec3,
        broke: bool,
    },
    EyeInserted {
        cell: IVec3,
        opened: bool,
    },
    Redstone {
        cell: IVec3,
    },
    Composter {
        cell: IVec3,
    },
    /// A block screen (crafting table, furnace, chest, ...) should open.
    Screen {
        cell: IVec3,
        block: Block,
    },
    EggTeleported {
        destination: Option<IVec3>,
    },
    CakeBitten {
        cell: IVec3,
    },
    /// A bed was used; the adapter applies sleeping rules.
    Bed {
        cell: IVec3,
    },
    Harvested {
        cell: IVec3,
    },
    /// A door or gate toggled (`open` is its new state); `None` for an iron door.
    Door {
        cell: IVec3,
        open: Option<bool>,
    },
    TntPrimed {
        cell: IVec3,
    },
    /// Flint and steel lit `cell`; `portal` if it opened a Nether portal.
    Ignited {
        cell: IVec3,
        portal: bool,
    },
    /// Flint and steel struck without lighting anything.
    FlintFizzled,
    Path {
        cell: IVec3,
    },
    Stripped {
        cell: IVec3,
    },
    Tilled {
        cell: IVec3,
    },
    Carved {
        cell: IVec3,
    },
    BoneMeal {
        cell: IVec3,
    },
    Placed(super::placement::Outcome),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Use {
    pub player: PlayerId,
    pub used: Used,
    /// The held tool broke doing this.
    pub tool_broken: Option<Item>,
}

/// Inputs the session does not own.
pub struct UseContext<'a> {
    pub sneaking: bool,
    /// Other players' positions, which block placement.
    pub others: &'a [DVec3],
}

impl PlayerSession {
    /// Whether the aimed block consumes use before the held item. Cake is
    /// conditional on hunger; sneaking with an item skips block interaction.
    pub fn block_takes_use(&self, slot: usize, world: &World, sneaking: bool) -> bool {
        let held = self.inventory.get(slot).map(|s| s.item);
        let Some((pos, _)) = self.block_target(world) else { return false };
        crate::content::interaction::block_use_precedes_item(world, pos, held, sneaking)
            || (!(sneaking && held.is_some())
                && world.get_block(pos).is_some_and(|b| b.cake_bites().is_some())
                && (self.mode.is_creative() || self.vitals.hunger.can_eat()))
    }

    /// The mob under the crosshair within entity reach and in front of the targeted block.
    pub fn mob_target(&self, world: &World, entities: &Entities) -> Option<usize> {
        let (eye, dir) = (self.player.eye(), self.player.forward().as_dvec3());
        let block = self
            .block_target(world)
            .and_then(|(b, _)| physics::ray_aabb(eye, dir, b.as_dvec3(), b.as_dvec3() + DVec3::ONE));
        let reach = block.map_or(self.entity_reach(), |d| d.min(self.entity_reach()));
        entities.raycast(eye, dir, reach).map(|(i, _)| i)
    }

    /// Run the use chain for the item in `slot`. `Ok(None)` means nothing
    /// responded; the adapter may still draw a bow or eat.
    pub fn use_held(
        &mut self,
        slot: usize,
        world: &mut World,
        entities: &mut Entities,
        ctx: &UseContext,
    ) -> Result<Option<Use>, &'static str> {
        if !self.mode.can_interact() || self.vitals.is_dead() || self.sleeping.is_some() {
            return Err("player cannot use items now");
        }
        if slot >= HOTBAR_SLOTS {
            return Err("selected slot out of range");
        }
        if !self.player.pos.is_finite() || !self.player.yaw.is_finite() || !self.player.pitch.is_finite() {
            return Err("player view is unavailable");
        }
        let mut tool_broken = None;
        let mut used = self.use_entity(slot, world, entities, ctx, &mut tool_broken)?;
        if used.is_none() {
            used = self.use_item(slot, world, entities, ctx)?;
        }
        // Riders use items (Java `Item.use` has no vehicle check) but not blocks.
        if used.is_none() && self.player.vehicle.is_none() {
            used = self.use_block(slot, world, entities, ctx, &mut tool_broken)?;
        }
        if let Some(Used::Rod { broke: true, .. }) = used {
            tool_broken = Some(Item::FISHING_ROD);
        }
        Ok(used.map(|used| Use { player: self.id, used, tool_broken }))
    }

    fn use_entity(
        &mut self,
        slot: usize,
        world: &World,
        entities: &mut Entities,
        ctx: &UseContext,
        tool_broken: &mut Option<Item>,
    ) -> Result<Option<Used>, &'static str> {
        let creative = self.mode.is_creative();
        if let Some(id) = self.player.vehicle {
            if entities.boost_mount(self.id, &mut self.inventory, slot, creative) {
                return Ok(Some(Used::Boosted));
            }
            if entities.vehicle_slots(id).is_some_and(|slots| !slots.is_empty()) {
                return Ok(Some(Used::OpenVehicle(id)));
            }
            return Ok(None);
        }
        let (eye, dir) = (self.player.eye(), self.player.forward().as_dvec3());
        let reach = crate::entity::minecart::interaction_reach(world, eye, dir, self.entity_reach());
        if let Some(id) = entities.vehicle_container(eye, dir, reach)
            && (ctx.sneaking || entities.cart(id).is_some_and(|c| c.kind.boat().is_none()))
        {
            return Ok(Some(Used::OpenVehicle(id)));
        }
        // Leads, name tags, food and taming come before mounting.
        if survival_items::use_animal_target(
            &self.player,
            &mut self.inventory,
            slot,
            creative,
            world,
            entities,
            self.id,
        ) {
            return Ok(Some(Used::Animal));
        }
        if entities.use_mount(eye, dir, reach, self.id, &mut self.inventory, slot, creative) {
            if let Some(id) = entities.mount_for_player(self.id) {
                self.player.vehicle = Some(id);
                return Ok(Some(Used::Mounted(id)));
            }
            return Ok(Some(Used::Animal));
        }
        if let Some(id) = entities.mount_cart(eye, dir, reach, self.id) {
            self.player.vehicle = Some(id);
            return Ok(Some(Used::Mounted(id)));
        }
        if let Some(kind) = self.inventory.get(slot).and_then(|s| crate::entity::minecart::CartKind::from_item(s.item))
            && let Some((pos, normal)) = self.block_target(world)
        {
            let cell = if world.get_block(pos).is_some_and(|b| b.is_rail()) { pos } else { pos + normal };
            let cell =
                if kind.boat().is_some() && world.get_block(pos).is_some_and(|b| b.is_water()) { pos } else { cell };
            entities.place_cart(world, kind, cell).ok_or("minecarts need an empty rail")?;
            if self.mode.is_survival() {
                self.inventory.take_one(slot);
            }
            return Ok(Some(Used::CartPlaced));
        }
        if !ctx.sneaking
            && let Some(id) = entities.target_merchant(world, eye, dir, self.entity_reach())
        {
            let offers = entities
                .merchant(id)
                .and_then(|m| m.villager.as_ref())
                .is_some_and(|v| v.offers.iter().any(Option::is_some));
            return Ok(Some(if offers { Used::OpenMerchant(id) } else { Used::MerchantRefused }));
        }
        if let Some(index) = self.mob_target(world, entities)
            && let Some(item) = self.inventory.get(slot).map(|s| s.item)
            && let Some(sheared) = entities.use_on_sheep(index, item)
        {
            if self.mode.is_survival() {
                if sheared {
                    if self.inventory.wear(slot, 1) {
                        *tool_broken = Some(item);
                    }
                } else {
                    self.inventory.take_one(slot);
                }
            }
            return Ok(Some(Used::Sheep { sheared }));
        }
        if survival_items::use_mob(&self.player, &mut self.inventory, slot, creative, world, entities, self.id) {
            return Ok(Some(Used::Mob));
        }
        Ok(None)
    }

    /// Held-item uses that act without a block (buckets, throwables, rods).
    /// Aiming at a block that takes right-clicks (containers, beds) defers to it.
    fn use_item(
        &mut self,
        slot: usize,
        world: &mut World,
        entities: &mut Entities,
        ctx: &UseContext,
    ) -> Result<Option<Used>, &'static str> {
        let target = self.block_target(world);
        if self.block_takes_use(slot, world, ctx.sneaking) {
            return Ok(None);
        }
        let creative = self.mode.is_creative();
        let survival = self.mode.is_survival();
        let reach = self.block_reach();
        if let bucket::Use::Item(Some(effect)) =
            bucket::use_held(&self.player, &mut self.inventory, slot, self.mode, ctx.sneaking, world, entities, reach)
        {
            return Ok(Some(Used::Bucket(effect)));
        }
        let Some(held) = self.inventory.get(slot).map(|s| s.item) else { return Ok(None) };
        let p = &self.player;
        let (eye, dir) = (p.eye(), p.forward().as_dvec3());
        // Java adds the thrower's motion, vertical only while airborne.
        let carry = if p.on_ground { p.vel.with_y(0.0) } else { p.vel };
        if held == Item::ENDER_PEARL {
            if self.vitals.pearl_cooldown > 0.0 {
                return Err("ender pearl cooling down");
            }
            self.vitals.pearl_cooldown = crate::entity::pearl::COOLDOWN;
            entities.throw_pearl(self.id, eye, dir, carry);
            if survival {
                self.inventory.take_one(slot);
            }
            return Ok(Some(Used::PearlThrown { from: eye }));
        }
        if let Some(potion) = held.as_splash_potion() {
            entities.throw_potion(self.id, potion, eye, dir, carry);
            if survival {
                self.inventory.take_one(slot);
            }
            return Ok(Some(Used::PotionThrown { from: eye }));
        }
        if held == Item::EYE_OF_ENDER
            && !target.and_then(|(pos, _)| world.get_block(pos)).is_some_and(|b| b.base() == Block::END_PORTAL_FRAME)
        {
            if world.generator.dimension != Dimension::Overworld {
                return Err("eyes of ender only find strongholds in the overworld");
            }
            let from = self.player.pos + DVec3::Y * (crate::player::SHAPE.height * 0.5);
            let stronghold = world.generator.strongholds.nearest(from.floor().as_ivec3()).ok_or("no stronghold")?;
            entities.release_eye(from, stronghold.as_dvec3());
            if survival {
                self.inventory.take_one(slot);
            }
            return Ok(Some(Used::EyeReleased { from }));
        }
        if survival_items::throw_held(&self.player, &mut self.inventory, slot, creative, self.id, entities) {
            return Ok(Some(Used::ProjectileThrown { from: eye }));
        }
        if let Some(broke) =
            survival_items::use_rod(&self.player, &mut self.inventory, slot, creative, self.id, entities)
        {
            return Ok(Some(Used::Rod { from: eye, broke }));
        }
        Ok(None)
    }

    fn use_block(
        &mut self,
        slot: usize,
        world: &mut World,
        entities: &mut Entities,
        ctx: &UseContext,
        tool_broken: &mut Option<Item>,
    ) -> Result<Option<Used>, &'static str> {
        let Some((pos, normal)) = self.block_target(world) else { return Ok(None) };
        let held = self.inventory.get(slot).map(|s| s.item);
        let survival = self.mode.is_survival();
        let creative = self.mode.is_creative();
        let can_build = self.mode.can_build();
        if held == Some(Item::EYE_OF_ENDER) && world.get_block(pos).is_some_and(|b| b.base() == Block::END_PORTAL_FRAME)
        {
            let opened = world.insert_eye(pos).ok_or("that frame already holds an eye, or its ring isn't loaded")?;
            if survival {
                self.inventory.take_one(slot);
            }
            return Ok(Some(Used::EyeInserted { cell: pos, opened }));
        }
        if !ctx.sneaking && world.use_redstone(pos) {
            return Ok(Some(Used::Redstone { cell: pos }));
        }
        world.touch_redstone_ore(pos);
        if !ctx.sneaking && survival_items::use_composter(world, entities, pos, &mut self.inventory, slot, creative) {
            return Ok(Some(Used::Composter { cell: pos }));
        }
        // Containers open on right-click; sneaking builds against them.
        if !ctx.sneaking {
            match world.get_block(pos) {
                Some(b)
                    if b == Block::CRAFTING_TABLE
                        || crate::world::furnace::is_furnace(b)
                        || crate::world::chest::is_chest(b)
                        || b == Block::BREWING_STAND
                        || b == Block::ENCHANTING_TABLE
                        || b.is_anvil()
                        || b.base() == Block::GRINDSTONE
                        || b == Block::SMITHING_TABLE =>
                {
                    return Ok(Some(Used::Screen { cell: pos, block: b }));
                }
                Some(Block::DRAGON_EGG) => {
                    return Ok(Some(Used::EggTeleported { destination: world.teleport_egg(pos) }));
                }
                Some(b) if b.cake_bites().is_some() => {
                    if !survival_items::bite_cake(world, pos, &mut self.vitals, creative) {
                        return Ok(None);
                    }
                    return Ok(Some(Used::CakeBitten { cell: pos }));
                }
                Some(b) if b.is_bed() => return Ok(Some(Used::Bed { cell: pos })),
                Some(_) if held != Some(Item::BONE_MEAL) && world.harvest(pos) => {
                    return Ok(Some(Used::Harvested { cell: pos }));
                }
                Some(b) if b.is_door() || b.is_gate() => {
                    let open = toggle_door(world, pos, self.player.forward());
                    return Ok(Some(Used::Door { cell: pos, open }));
                }
                _ => {}
            }
        }
        if !can_build {
            return Ok(None);
        }
        let wear = |s: &mut Self, amount: u16, broken: &mut Option<Item>| {
            if survival
                && let Some(item) = held
                && s.inventory.wear(slot, amount)
            {
                *broken = Some(item);
            }
        };
        if held == Some(Item::FLINT_AND_STEEL) {
            if world.get_block(pos) == Some(Block::TNT) {
                world.set_block(pos, Block::AIR);
                entities.prime_tnt(pos, false);
                wear(self, 1, tool_broken);
                return Ok(Some(Used::TntPrimed { cell: pos }));
            }
            let at = pos + normal;
            if !world.ignite(at) {
                return Ok(Some(Used::FlintFizzled));
            }
            wear(self, 1, tool_broken);
            let portal = world.get_block(at) == Some(Block::NETHER_PORTAL);
            return Ok(Some(Used::Ignited { cell: at, portal }));
        }
        if let Some(used) = self.use_item_on(slot, world, entities, pos, normal, tool_broken) {
            return Ok(Some(used));
        }
        let result = self.place_held(slot, world, ctx.others)?;
        Ok(Some(Used::Placed(result)))
    }

    /// Shovel paths, axe stripping, hoe tilling, pumpkin carving and bone meal.
    fn use_item_on(
        &mut self,
        slot: usize,
        world: &mut World,
        entities: &mut Entities,
        pos: IVec3,
        normal: IVec3,
        tool_broken: &mut Option<Item>,
    ) -> Option<Used> {
        let item = self.inventory.get(slot)?.item;
        let block = world.get_block(pos)?;
        let survival = self.mode.is_survival();
        let tool = item.as_tool().map(|(kind, _)| kind);
        let mut wear = |s: &mut Self| {
            if survival && s.inventory.wear(slot, crate::mining::wear(item, false)) {
                *tool_broken = Some(item);
            }
        };
        let open_above = normal != IVec3::NEG_Y && world.get_block(pos + IVec3::Y) == Some(Block::AIR);
        if tool == Some(ToolKind::Shovel)
            && matches!(block, Block::GRASS | Block::DIRT | Block::SNOWY_GRASS)
            && open_above
            && world.set_block(pos, Block::DIRT_PATH)
        {
            wear(self);
            return Some(Used::Path { cell: pos });
        }
        // Axes strip crimson and warped stems and hyphae, keeping their axis.
        if tool == Some(ToolKind::Axe)
            && let Some(stripped) = crate::world::nether_biome_blocks::stripped(block)
            && world.set_block(pos, stripped)
        {
            wear(self);
            return Some(Used::Stripped { cell: pos });
        }
        if tool == Some(ToolKind::Hoe) {
            if matches!(block, Block::GRASS | Block::DIRT) && open_above && world.set_block(pos, Block::FARMLAND) {
                wear(self);
                return Some(Used::Tilled { cell: pos });
            }
            return None;
        }
        if item == Item::SHEARS && block == Block::PUMPKIN {
            let facing = Facing::toward(self.player.forward());
            if world.set_block(pos, Block::CARVED_PUMPKIN.with_facing(facing)) {
                if survival && self.inventory.wear(slot, 1) {
                    *tool_broken = Some(item);
                }
                entities.drop_from_block(crate::inventory::Stack::new(Item::PUMPKIN_SEEDS, 4), pos);
                return Some(Used::Carved { cell: pos });
            }
        }
        if item == Item::BONE_MEAL && world.apply_bone_meal(pos) {
            if survival {
                self.inventory.take_one(slot);
            }
            return Some(Used::BoneMeal { cell: pos });
        }
        None
    }
}

/// The item pick-block (middle click) looks for: crops give seeds, and
/// farmland and lit furnaces their plain blocks.
pub fn picked_item(block: Block) -> Item {
    if matches!(
        crate::world::redstone_blocks::component(block),
        Some(crate::world::redstone_blocks::Component::Wire(_))
    ) {
        return Item::REDSTONE;
    }
    match block.base() {
        Block::LIT_FURNACE => Block::FURNACE.into(),
        Block::LIT_SMOKER => Block::SMOKER.into(),
        Block::LIT_BLAST_FURNACE => Block::BLAST_FURNACE.into(),
        Block::FARMLAND | Block::WET_FARMLAND => Block::DIRT.into(),
        b if matches!(b.as_crop(), Some((crate::world::block::Crop::Wheat, _))) => Item::WHEAT_SEEDS,
        b if matches!(b.as_crop(), Some((crate::world::block::Crop::Carrot, _))) => Item::CARROT,
        b if matches!(b.as_crop(), Some((crate::world::block::Crop::Potato, _))) => Item::POTATO,
        b if b.wart_age().is_some() => Item::NETHER_WART,
        crate::world::nether_biome_blocks::WEEPING_VINES_PLANT => {
            crate::world::nether_biome_blocks::WEEPING_VINES.into()
        }
        crate::world::nether_biome_blocks::TWISTING_VINES_PLANT => {
            crate::world::nether_biome_blocks::TWISTING_VINES.into()
        }
        Block::OAK_DOOR => Item::OAK_DOOR,
        b if let Some(c) = b.bed_color() => c.bed(),
        b => b.into(),
    }
}

/// What pick-block did to the hotbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// The item is already in this hotbar slot; select it.
    Select(usize),
    /// The selected slot now holds it (swapped from the main grid, or a
    /// fresh Creative stack).
    Replaced,
}

impl PlayerSession {
    /// Middle click on the targeted block: find its item in the inventory,
    /// or conjure a stack in Creative.
    pub fn pick_block(&mut self, selected: usize, world: &World) -> Option<Pick> {
        let b = self.block_target(world).and_then(|(pos, _)| world.get_block(pos))?;
        if b.is_fire() || selected >= HOTBAR_SLOTS {
            return None;
        }
        let item = picked_item(b);
        match self.inventory.find(item) {
            Some(i) if i < HOTBAR_SLOTS => Some(Pick::Select(i)),
            Some(i) => {
                self.cancel_mining();
                self.cancel_consumption();
                self.inventory.slots.swap(i, selected);
                Some(Pick::Replaced)
            }
            None if self.mode == crate::rules::GameMode::Creative => {
                self.cancel_mining();
                self.cancel_consumption();
                self.inventory.slots[selected] = Some(crate::inventory::Stack::new(item, item.max_stack()));
                Some(Pick::Replaced)
            }
            None => None,
        }
    }
}

/// Opens or closes a door (both halves) or fence gate for a player looking
/// along `forward`. Returns the new open state, or `None` for an iron door,
/// which only redstone moves.
pub fn toggle_door(world: &mut World, pos: IVec3, forward: glam::Vec3) -> Option<bool> {
    let b = world.get_block(pos)?;
    if matches!(
        crate::world::redstone_blocks::component(b),
        Some(crate::world::redstone_blocks::Component::IronDoor { .. })
    ) {
        return None;
    }
    match b.shaped() {
        Some(Shaped::Door { mut facing, mut open, upper }) => {
            let other = if upper { pos - IVec3::Y } else { pos + IVec3::Y };
            let partner = world.get_block(other).filter(|o| o.is_door());
            // A wood door's upper half does not store facing, so the
            // lower half is the one that flips.
            if upper
                && let Some(lower) = partner
                && let Some(Shaped::Door { facing: f, open: o, upper: false }) = lower.shaped()
            {
                facing = f;
                open = o;
            }
            world.set_block(pos, b.toggled(facing));
            if let Some(o) = partner {
                world.set_block(other, o.toggled(facing));
            }
            Some(!open)
        }
        Some(Shaped::Gate { facing, open }) => {
            // A gate swings away from whoever opens it.
            let toward = Facing::toward(forward);
            let facing = if !open && toward.along_x() == facing.along_x() { toward } else { facing };
            world.set_block(pos, b.toggled(facing));
            Some(!open)
        }
        _ => None,
    }
}
