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

/// A spawn egg's refusal on Peaceful (`EntityType.canSpawn`).
const EGG_PEACEFUL: &str = "that mob can't spawn on Peaceful";

/// What a use did. Adapters play sounds, show popups and open screens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Used {
    /// A ridden mount was boosted (carrot/fungus on a stick).
    Boosted,
    /// v0.16 Track G: an End crystal now stands here (feet).
    CrystalPlaced {
        at: DVec3,
    },
    /// Ridden or targeted vehicle storage should open.
    OpenVehicle(u32),
    /// A lead, name tag, food or taming item was used on an animal.
    Animal,
    BrushStarted,
    /// v1.0 gap track 6: a bundle is now in use (`BundleItem.use`).
    BundleStarted,
    /// The player now rides this vehicle.
    Mounted(u32),
    CartPlaced,
    /// v0.17 Track D1: a furnace minecart was right-clicked; `fuelled` when
    /// it took the held coal or charcoal (`MinecartFurnace.interact`).
    FurnaceCart {
        fuelled: bool,
        at: DVec3,
    },
    MapCreated,
    /// A cushion was set down at this position (`CushionItem.useOn`).
    CushionPlaced {
        at: DVec3,
    },
    /// v0.18 Track 4: a spawn egg made a mob, now standing here.
    SpawnedMob {
        at: DVec3,
    },
    /// v0.18 Track 4: a spawn egg set the mob of the spawner or trial
    /// spawner in this cell.
    SpawnerSet {
        cell: IVec3,
    },
    /// v0.15 Track B: a lectern holding a book was used; its reading
    /// screen opens (`LecternMenu`).
    Lectern {
        cell: IVec3,
    },
    /// A book and quill or written book in hotbar `slot` was used: its
    /// editor or reader opens.
    OpenBook {
        slot: usize,
    },
    /// A book went onto the lectern at `cell` (`BOOK_PUT`).
    LecternBook {
        cell: IVec3,
    },
    /// A villager with offers was targeted; its trading screen should open.
    OpenMerchant(u64),
    /// A villager with no offers was targeted; nothing else happens.
    MerchantRefused,
    Sheep {
        sheared: bool,
    },
    /// Another held-item interaction with a mob (bucketing, milking, ...).
    Mob,
    /// A statue changed pose or became a golem; its sound is an entity event.
    Statue {
        cell: IVec3,
    },
    Bucket(bucket::Effect),
    PearlThrown {
        from: DVec3,
    },
    /// v0.16 Track B: a firework rocket left the hand (launched from a
    /// block, or held by a gliding player for a boost).
    RocketLaunched {
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
    /// A wind charge left the hand (`WIND_CHARGE_THROW`).
    WindChargeThrown {
        from: DVec3,
    },
    /// v1.0 final Track F4: a goat horn blown (`InstrumentItem.use`): its
    /// instrument (Java registry index) and the player's position.
    HornPlayed {
        instrument: u8,
        from: DVec3,
    },
    /// An item was used on an active vault: `unlocked` with a key, else
    /// refused (`rewarded`: this player already had the reward); `sound`
    /// is false inside the fail sound's 15-tick buffer.
    Vault {
        cell: IVec3,
        unlocked: bool,
        rewarded: bool,
        sound: bool,
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
    /// A lit candle or candle cake was put out by hand.
    CandleExtinguished {
        cell: IVec3,
    },
    /// A candle was set on an uneaten cake.
    CandlePlacedOnCake {
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
    /// v1.0 gap Track 8f: `charge` when a fire charge did
    /// (`FireChargeItem.playSound`'s `FIRECHARGE_USE`).
    Ignited {
        cell: IVec3,
        portal: bool,
        charge: bool,
    },
    /// Flint and steel struck without lighting anything.
    FlintFizzled,
    Path {
        cell: IVec3,
    },
    Stripped {
        cell: IVec3,
    },
    /// A compass now points to the lodestone at `cell`.
    CompassLocked {
        cell: IVec3,
    },
    Waxed {
        cell: IVec3,
    },
    /// An axe removed wax or scraped copper back one oxidation stage.
    Scraped {
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
    /// A v0.12 utility block took the use (pots, cauldrons, campfires,
    /// respawn anchors); adapters play its sound, the host follows up.
    Utility {
        cell: IVec3,
        effect: crate::world::utilities::interact::Effect,
    },
    /// An item frame, painting or armor stand was placed or used.
    Decor {
        at: DVec3,
        effect: crate::entity::decor::Effect,
    },
    Placed(super::placement::Outcome),
    /// A sign's text editor should open for its front or back (the adapter
    /// answers with `PlayerSession::edit_sign`).
    EditSign {
        cell: IVec3,
        front: bool,
    },
    /// A dye, ink sac or glow ink sac recoloured or lit a sign's text.
    SignInked {
        cell: IVec3,
        item: Item,
    },
    /// A disc went into a jukebox (`inserted`) or popped out of one.
    Jukebox {
        cell: IVec3,
        inserted: bool,
    },
    /// A shelf or chiseled bookshelf traded items with the player.
    Shelf {
        cell: IVec3,
        used: crate::world::wood::ShelfUse,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Use {
    pub player: PlayerId,
    pub used: Used,
    /// The held tool broke doing this.
    pub tool_broken: Option<Item>,
    /// The off-hand's item did this (the main hand's use passed).
    pub off_hand: bool,
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
        if world.get_block(pos).and_then(crate::world::copper_golem_statue::decode).is_some() {
            return self.player.vehicle.is_none()
                && !(sneaking && (held.is_some() || self.inventory.offhand.is_some()));
        }
        crate::content::interaction::block_use_precedes_stack(
            world,
            pos,
            self.inventory.get(slot),
            sneaking,
            (self.player.eye(), self.player.forward().as_dvec3()),
        ) || (!(sneaking && held.is_some())
            && world.get_block(pos).is_some_and(|b| {
                b.cake_bites().is_some() || crate::world::colour_blocks::candle::decode_cake(b.0).is_some()
            })
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
        let mut off_hand = false;
        // Try the main hand first, including its empty-hand entity uses.
        let main = self.use_chain(slot, world, entities, ctx, &mut tool_broken);
        let (mut used, main_error) = match main {
            Ok(used) => (used, None),
            Err(e) => (None, Some(e)),
        };
        // Java tries the off-hand when the main hand's use passes; a held use
        // the main hand starts (eating, a bow, a shield) is not passed over.
        let main_holds_use = self.shield_hand(slot) == Some(super::hands::Hand::Main)
            || self.inventory.get(slot).is_some_and(|s| {
                s.item.is_drink() || s.item.food().is_some() && self.can_eat(s.item) || self.starts_draw(s.item)
            });
        // An empty off-hand can still interact with a copper golem. Preserve
        // ordinary item/block errors if that entity use also passes.
        if used.is_none()
            && !main_holds_use
            && self.inventory.offhand.is_none()
            && self.player.vehicle.is_none()
            && let Some(index) = self.mob_target(world, entities)
            && entities.return_copper_golem_cargo(index, self.player.pos)
        {
            used = Some(Used::Mob);
            off_hand = true;
        }
        if used.is_none()
            && let Some(off) = self.inventory.offhand
            && !main_holds_use
            && off.item != Item::SHIELD
        {
            self.inventory.swap_offhand(slot);
            let result = self.use_chain(slot, world, entities, ctx, &mut tool_broken);
            self.inventory.swap_offhand(slot);
            if result.as_ref().is_ok_and(|u| *u == Some(Used::BrushStarted)) {
                self.brushing.off = true;
            }
            if result.as_ref().is_ok_and(|u| *u == Some(Used::BundleStarted)) {
                self.bundle_use.off = true;
            }
            match result {
                Ok(u) => {
                    off_hand = u.is_some();
                    // The book that opened is the off-hand's, not the
                    // hotbar stack swapped back into `slot`.
                    used = match u {
                        Some(Used::OpenBook { .. }) => Some(Used::OpenBook { slot: super::books::OFFHAND_SLOT }),
                        u => u,
                    };
                }
                Err(e) => return Err(main_error.unwrap_or(e)),
            }
        } else if used.is_none()
            && let Some(e) = main_error
        {
            return Err(e);
        }
        if let Some(Used::Rod { broke: true, .. }) = used {
            tool_broken = Some(Item::FISHING_ROD);
        }
        Ok(used.map(|used| Use { player: self.id, used, tool_broken, off_hand }))
    }

    /// The entity, item and block uses for the stack in `slot`, in Java's order.
    fn use_chain(
        &mut self,
        slot: usize,
        world: &mut World,
        entities: &mut Entities,
        ctx: &UseContext,
        tool_broken: &mut Option<Item>,
    ) -> Result<Option<Used>, &'static str> {
        // The mob a use reaches, for `entity_interact` and `shear` at its position.
        let target_mob = self.mob_target(world, entities);
        let target = target_mob.map(|i| entities.mobs[i].pos);
        // v1.0 Track 9: shears a mob's `mobInteract` took (`MushroomCow`,
        // `SnowGolem`, `CopperGolem`, `SulfurCube`, and `Mob.shearItem` for
        // a nautilus's saddle or a happy ghast's harness) post `shear` first.
        use crate::entity::MobKind as K;
        let shearing = self.inventory.get(slot).is_some_and(|s| s.item == Item::SHEARS)
            && target_mob.is_some_and(|i| {
                let kind = entities.mobs[i].kind;
                matches!(kind, K::Mooshroom | K::SnowGolem | K::CopperGolem | K::SulfurCube | K::HappyGhast)
                    || crate::entity::nautilus::is_nautilus(kind)
            });
        let mut used = self.use_entity(slot, world, entities, ctx, tool_broken)?;
        if let Some(u) = used {
            if shearing
                && matches!(u, Used::Animal | Used::Mob)
                && let Some(at) = target
            {
                world.emit_game_event(at, crate::world::vibration::GameEvent::Shear, self.event_source(), None);
            }
            self.entity_use_event(world, entities, u, target);
        }
        if used.is_none()
            && self.inventory.get(slot).is_some_and(|s| s.item == Item::BRUSH)
            && self.start_brushing(slot, world)
        {
            used = Some(Used::BrushStarted);
        }
        if used.is_none() {
            used = self.use_item(slot, world, entities, ctx)?;
        }
        if let Some(u) = used {
            self.item_use_event(world, entities, u);
        }
        // Riders use items (Java `Item.use` has no vehicle check) but not blocks.
        if used.is_none() && self.player.vehicle.is_none() {
            used = self.use_block(slot, world, entities, ctx, tool_broken)?;
        }
        Ok(used)
    }

    /// This player as a game event source.
    pub fn event_source(&self) -> crate::world::vibration::EventSource {
        crate::world::vibration::EventSource {
            untargetable: self.mode.is_creative(),
            spectator: self.mode == crate::rules::GameMode::Spectator,
            ..crate::world::vibration::EventSource::player(self.id, self.player.sneaking)
        }
    }

    /// The game events Java posts for uses on entities: `Mob.interact`'s
    /// `entity_interact` at the mob (after `shear` for a sheared sheep or
    /// bogged), `Entity.startRiding`'s `entity_mount` at the vehicle, and a
    /// chest boat or chest minecart's `container_open`.
    fn entity_use_event(&self, world: &mut World, entities: &Entities, used: Used, target: Option<DVec3>) {
        use crate::world::vibration::GameEvent as E;
        let source = self.event_source();
        match used {
            Used::Sheep { sheared } => {
                if let Some(at) = target {
                    if sheared {
                        world.emit_game_event(at, E::Shear, source, None);
                    }
                    world.emit_game_event(at, E::EntityInteract, source, None);
                }
            }
            Used::Animal | Used::Mob | Used::OpenMerchant(_) | Used::MerchantRefused => {
                if let Some(at) = target {
                    world.emit_game_event(at, E::EntityInteract, source, None);
                }
            }
            // `Player.interactOn`: `MinecartFurnace.interact` consumes the use.
            Used::FurnaceCart { at, .. } => world.emit_game_event(at, E::EntityInteract, source, None),
            Used::Mounted(id) => {
                if let Some(at) = entities.vehicle_pos(id) {
                    world.emit_game_event(at, E::EntityMount, source, None);
                    // A mob's `mobInteract` riding consumes the use.
                    if id & crate::entity::mounts::MOB_VEHICLE != 0 {
                        world.emit_game_event(at, E::EntityInteract, source, None);
                    }
                }
            }
            Used::OpenVehicle(id) => {
                // `AbstractChestBoat.interact` / `MinecartChest.interact`.
                use crate::entity::minecart::CartKind;
                if entities
                    .cart(id)
                    .is_some_and(|c| matches!(c.kind, CartKind::Chest | CartKind::Boat { chest: true, .. }))
                    && let Some(at) = entities.vehicle_pos(id)
                {
                    world.emit_game_event(at, E::ContainerOpen, source, None);
                }
            }
            Used::Decor { at, effect } => {
                use crate::entity::decor::Effect as D;
                let event = match effect {
                    // `HangingEntityItem` / `ArmorStandItem.useOn`.
                    D::Placed(_) => E::EntityPlace,
                    // `ItemFrame.interact`.
                    D::ItemAdded | D::ItemRotated | D::ItemRemoved => E::BlockChange,
                    _ => return,
                };
                world.emit_game_event(at, event, source, None);
            }
            _ => {}
        }
    }

    /// The game events Java posts for item uses: buckets and bottles
    /// (`fluid_pickup`, `fluid_place`), thrown projectiles, eyes of ender,
    /// rockets and wind charges (`projectile_shoot` from their owner), and a
    /// fishing rod's cast (`item_interact_start` then the bobber's shot) and
    /// reel (`item_interact_finish`).
    fn item_use_event(&self, world: &mut World, entities: &Entities, used: Used) {
        use crate::world::vibration::GameEvent as E;
        use survival_items::bucket::Effect as B;
        let source = self.event_source();
        let (pos, event) = match used {
            Used::Bucket(B::Filled { at, .. } | B::BottleFilled { at }) => {
                (at.as_dvec3() + DVec3::splat(0.5), E::FluidPickup)
            }
            Used::Bucket(B::Poured { at, .. } | B::Released { at }) => {
                (at.as_dvec3() + DVec3::splat(0.5), E::FluidPlace)
            }
            Used::ProjectileThrown { from }
            | Used::PearlThrown { from }
            | Used::PotionThrown { from }
            | Used::EyeReleased { from }
            | Used::RocketLaunched { from }
            | Used::WindChargeThrown { from } => (from, E::ProjectileShoot),
            // `InstrumentItem.play`: `instrument_play` at the player.
            Used::HornPlayed { from, .. } => (from, E::InstrumentPlay),
            Used::Rod { from, .. } => {
                // `FishingRodItem.use`: a cast leaves a bobber out.
                let cast = entities.bobbers.iter().any(|b| b.owner == self.id);
                let event = if cast { E::ItemInteractStart } else { E::ItemInteractFinish };
                world.emit_game_event(self.player.pos, event, source, None);
                if cast {
                    (from, E::ProjectileShoot)
                } else {
                    return;
                }
            }
            _ => return,
        };
        world.emit_game_event(pos, event, source, None);
        // v1.0 Track 9: `MobBucketItem.checkExtraContent` posts `entity_place`
        // from the player for the released creature.
        if let Used::Bucket(B::Released { .. }) = used {
            world.emit_game_event(pos, E::EntityPlace, source, None);
        }
    }

    /// Items whose right-click starts drawing (bows and crossbows).
    fn starts_draw(&self, item: Item) -> bool {
        matches!(item, Item::BOW | Item::CROSSBOW | Item::CHARGED_CROSSBOW)
    }

    fn use_entity(
        &mut self,
        slot: usize,
        world: &mut World,
        entities: &mut Entities,
        ctx: &UseContext,
        tool_broken: &mut Option<Item>,
    ) -> Result<Option<Used>, &'static str> {
        let creative = self.mode.is_creative();
        let clicks = survival_items::Reach { block: self.block_reach(), entity: self.entity_reach() };
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
        // `MinecartFurnace.interact` always succeeds: coal or charcoal fuels
        // it and pushes it away from the player's feet.
        if let Some(i) = entities.cart_ray(eye, dir, reach)
            && entities.minecarts[i].kind == crate::entity::minecart::CartKind::Furnace
        {
            let held = self.inventory.get(slot).map(|s| s.item);
            let fuelled = held.is_some_and(|item| entities.minecarts[i].add_fuel(self.player.pos, item));
            if fuelled && !creative {
                self.inventory.take_one(slot);
            }
            return Ok(Some(Used::FurnaceCart { fuelled, at: entities.minecarts[i].pos }));
        }
        // v0.18 Track 4: `Mob.checkAndHandleImportantInteractions` (after
        // name tags, before every other mob use): a spawn egg on a mob of its
        // own type makes a baby (`spawnOffspringFromSpawnEgg`).
        if let Some(egg) =
            self.inventory.get(slot).map(|s| s.item).filter(|&i| crate::content::spawn_egg::is_spawn_egg(i))
            && let Some(index) = self.mob_target(world, entities)
            && entities.spawn_baby_from_egg(index, egg).is_some()
        {
            if !creative {
                self.inventory.take_one(slot);
            }
            return Ok(Some(Used::Mob));
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
            clicks,
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
        if let Some(used) = self.use_allay(slot, world, entities) {
            return Ok(Some(used));
        }
        if let Some(used) = self.use_copper_golem(slot, world, entities, tool_broken) {
            return Ok(Some(used));
        }
        // v1.0 Track 6: a boat's lead (`Entity.interact`) before boarding it.
        let held = self.inventory.get(slot).map(|s| s.item);
        if let Some(used) = entities.use_boat_leash(eye, dir, reach, self.id, held, creative) {
            if used && !creative {
                self.inventory.take_one(slot);
            }
            return Ok(Some(Used::Animal));
        }
        // `Cushion.interact` passes while sneaking.
        let sneaking_at_cushion = ctx.sneaking
            && entities.cart_ray(eye, dir, reach).is_some_and(|i| entities.minecarts[i].kind.cushion().is_some());
        if !sneaking_at_cushion && let Some(id) = entities.mount_cart(eye, dir, reach, self.id) {
            self.player.vehicle = Some(id);
            return Ok(Some(Used::Mounted(id)));
        }
        // Item frames take and turn items; armor stands swap armor.
        if let Some((at, effect)) = entities.use_decor(eye, dir, reach, &mut self.inventory, slot, creative) {
            return Ok(Some(Used::Decor { at, effect }));
        }
        // Cushions go on top faces through `use_item_on` (`CushionItem.useOn`).
        if let Some(kind) = self
            .inventory
            .get(slot)
            .and_then(|s| crate::entity::minecart::CartKind::from_item(s.item))
            .filter(|k| k.cushion().is_none())
            && let Some((pos, normal)) = self.block_target(world)
        {
            let cell = if world.get_block(pos).is_some_and(|b| b.is_rail()) { pos } else { pos + normal };
            let cell =
                if kind.boat().is_some() && world.get_block(pos).is_some_and(|b| b.is_water()) { pos } else { cell };
            entities.place_cart(world, kind, cell).ok_or("minecarts need an empty rail")?;
            if self.mode.is_survival() {
                self.inventory.take_one(slot);
            }
            // `MinecartItem.useOn`: at the rail, naming the block under it;
            // `BoatItem.use`: where it lands.
            let below = if kind.boat().is_some() { None } else { world.get_block(cell - IVec3::Y) };
            let event = crate::world::vibration::GameEvent::EntityPlace;
            world.emit_block_event(cell, event, self.event_source(), below);
            return Ok(Some(Used::CartPlaced));
        }
        if !ctx.sneaking
            && let Some(id) = entities.target_merchant(world, eye, dir, self.entity_reach())
        {
            entities.prepare_merchant_maps(world, id);
            let offers = entities
                .merchant(id)
                .and_then(|m| m.villager.as_ref())
                .is_some_and(|v| v.offers.iter().any(Option::is_some));
            if !offers {
                // v1.0 gap Track 7f: `setUnhappy`, the refusal's head shake.
                entities.refuse_trade(id);
                return Ok(Some(Used::MerchantRefused));
            }
            return Ok(Some(Used::OpenMerchant(id)));
        }
        // v1.0 gap Track 7f: a villager child shakes its head at any use.
        if !ctx.sneaking && entities.pester_child(world, eye, dir, self.entity_reach()) {
            return Ok(Some(Used::MerchantRefused));
        }
        if let Some(index) = self.mob_target(world, entities)
            && let Some(item) = self.inventory.get(slot).map(|s| s.item)
            && let Some(sheared) =
                entities.use_on_sheep(index, item).or_else(|| entities.shear_bogged(index, item).then_some(true))
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
        if survival_items::use_mob(&self.player, &mut self.inventory, slot, creative, world, entities, self.id, clicks)
        {
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
        // v0.15 Track B: `WritableBookItem.use` / `WrittenBookItem.use` open
        // the book (`Player.openItemGui`).
        if self.inventory.get(slot).is_some_and(|s| matches!(s.item, Item::WRITABLE_BOOK | Item::WRITTEN_BOOK)) {
            return Ok(Some(Used::OpenBook { slot }));
        }
        // v1.0 gap track 6: `BundleItem.use` starts using the bundle; held
        // use then throws its contents out (`session::bundles`).
        if self.inventory.get(slot).is_some_and(|s| crate::content::bundle::is_bundle(s.item))
            && self.start_bundle_use(slot, entities)
        {
            return Ok(Some(Used::BundleStarted));
        }
        if self.inventory.get(slot).is_some_and(|s| s.item == Item::MAP) {
            let pos = self.player.pos.floor().as_ivec3();
            let data = crate::world::maps::MapData::fresh(pos.x, pos.z, 0, world.generator.dimension, true, false);
            let filled = world.maps.create(data, Item::FILLED_MAP);
            if !self.mode.is_creative() && self.inventory.get(slot).unwrap().count == 1 {
                self.inventory.slots[slot] = Some(filled);
            } else {
                if !self.mode.is_creative() {
                    self.inventory.take_one(slot);
                }
                if self.inventory.add_stack(filled) > 0 {
                    entities.throw(filled, self.player.eye(), self.player.forward().as_dvec3());
                }
            }
            return Ok(Some(Used::MapCreated));
        }
        // v0.18 Track 4: `SpawnEggItem.useOn` and `use`.
        if let Some(kind) = self.inventory.get(slot).and_then(|s| crate::content::spawn_egg::spawns(s.item)) {
            return self.use_spawn_egg(slot, kind, world, entities);
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
        // v0.16 Track B: `FireworkRocketItem.use` boosts a gliding player;
        // otherwise `useOn` launches it 0.15 off the clicked face.
        if held == crate::content::fireworks::FIREWORK_ROCKET {
            let stack = self.inventory.get(slot).ok_or("selected slot empty")?;
            let from = if self.player.fall_flying {
                entities.boost_firework(&stack, self.id, self.player.pos);
                self.player.pos
            } else if let Some(hit) = self.block_hit(world) {
                let at = hit.point + hit.normal.as_dvec3() * crate::entity::firework::PLACEMENT_OFFSET;
                entities.launch_firework(&stack, at, Some(self.id));
                at
            } else {
                return Ok(None);
            };
            if survival {
                self.inventory.take_one(slot);
            }
            return Ok(Some(Used::RocketLaunched { from }));
        }
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
        if held == Item::GLASS_BOTTLE && entities.collect_dragon_breath(p.pos, p.collision_shape()) {
            if survival {
                self.inventory.take_one(slot);
            }
            let bottle = crate::inventory::Stack::new(Item::DRAGON_BREATH, 1);
            if (!creative || self.inventory.find(Item::DRAGON_BREATH).is_none()) && self.inventory.add_stack(bottle) > 0
            {
                entities.throw(bottle, eye, dir);
            }
            return Ok(Some(Used::Bucket(bucket::Effect::BottleFilled { at: p.pos.floor().as_ivec3() })));
        }
        if held == Item::LINGERING_POTION {
            let potion =
                self.inventory.get(slot).and_then(|s| s.lingering_contents()).unwrap_or(crate::potion::Potion(0));
            entities.throw_potion(self.id, potion, eye, dir, carry);
            entities.potions.last_mut().unwrap().lingering = true;
            if survival {
                self.inventory.take_one(slot);
            }
            return Ok(Some(Used::PotionThrown { from: eye }));
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
            let stronghold = world
                .generator
                .strongholds
                .nearest(&*world.generator, from.floor().as_ivec3())
                .ok_or("no stronghold")?;
            entities.release_eye(from, stronghold.as_dvec3());
            if survival {
                self.inventory.take_one(slot);
            }
            return Ok(Some(Used::EyeReleased { from }));
        }
        // `WindChargeItem.use`: throw at 1.5 (inaccuracy 1), then a
        // 10-tick cooldown for every wind charge.
        if held == crate::world::trial::WIND_CHARGE {
            if self.vitals.wind_charge_cooldown > 0.0 {
                return Err("wind charge cooling down");
            }
            self.vitals.wind_charge_cooldown =
                crate::entity::wind::COOLDOWN_TICKS as f32 * crate::simulation::TICK_SECONDS as f32;
            entities.throw_projectile(held, self.id, eye, dir, carry);
            if !creative {
                self.inventory.take_one(slot);
            }
            return Ok(Some(Used::WindChargeThrown { from: eye }));
        }
        // v1.0 final Track F4: `InstrumentItem.use` plays the instrument
        // and puts every goat horn on its 140-tick cooldown.
        if let Some(instrument) = crate::content::goat_horn::instrument(held) {
            if self.vitals.horn_cooldown > 0.0 {
                return Err("goat horn cooling down");
            }
            self.vitals.horn_cooldown = crate::content::goat_horn::cooldown_seconds();
            self.start_tooting();
            return Ok(Some(Used::HornPlayed { instrument, from: self.player.pos }));
        }
        if survival_items::throw_held(&self.player, &mut self.inventory, slot, creative, self.id, entities) {
            return Ok(Some(Used::ProjectileThrown { from: eye }));
        }
        let luck = self.vitals.effects.luck();
        if let Some(broke) =
            survival_items::use_rod(&self.player, &mut self.inventory, slot, creative, self.id, entities, world, luck)
        {
            return Ok(Some(Used::Rod { from: eye, broke }));
        }
        Ok(None)
    }

    /// v0.18 Track 4: `SpawnEggItem.use`, the item's own use: with no block
    /// (or a mob) under the crosshair, a water or lava source in reach
    /// (`ClipContext.Fluid.SOURCE_ONLY`) gets the mob. A targeted block goes
    /// through the block's own use first (`use_block_inner`, which then
    /// calls [`Self::spawn_egg_on_block`]), as Java's `useItemOn` does.
    fn use_spawn_egg(
        &mut self,
        slot: usize,
        kind: crate::content::spawn_egg::Spawns,
        world: &mut World,
        entities: &mut Entities,
    ) -> Result<Option<Used>, &'static str> {
        // `mayUseItemAt` and `mayInteract`: Adventure and Spectator can't.
        if !self.mode.can_build() {
            return Ok(None);
        }
        // With a mob under the crosshair, Java's client skips `useOn`.
        if self.mob_target(world, entities).is_none() && self.block_target(world).is_some() {
            return Ok(None);
        }
        let (eye, dir) = (self.player.eye(), self.player.forward().as_dvec3());
        let Some((pos, _)) = world.raycast_sources(eye, dir, self.block_reach()) else { return Ok(None) };
        // `instanceof LiquidBlock`: a water or lava source, not a waterlogged block.
        if !matches!(world.get_block(pos), Some(Block::WATER | Block::LAVA)) {
            return Ok(None);
        }
        let at = entities.spawn_egg_spawns(world, kind, pos, false, false).ok_or(EGG_PEACEFUL)?;
        self.consume_egg(slot);
        world.emit_block_event(pos, crate::world::vibration::GameEvent::EntityPlace, self.event_source(), None);
        Ok(Some(Used::SpawnedMob { at }))
    }

    /// `ItemStack.consume` / `shrink`: creative keeps its eggs.
    fn consume_egg(&mut self, slot: usize) {
        if !self.mode.is_creative() {
            self.inventory.take_one(slot);
        }
    }

    /// v0.18 Track 4: `SpawnEggItem.useOn` on the block at `pos`, once the
    /// block's own use has passed: a spawner or trial spawner takes the
    /// egg's mob; otherwise the mob spawns in the cell (no collision) or in
    /// front of the clicked face, dropped onto what's below.
    fn spawn_egg_on_block(
        &mut self,
        slot: usize,
        spawns: crate::content::spawn_egg::Spawns,
        world: &mut World,
        entities: &mut Entities,
        (pos, normal): (IVec3, IVec3),
    ) -> Result<Option<Used>, &'static str> {
        use crate::content::spawn_egg::Spawns;
        use crate::world::vibration::GameEvent as E;
        if !entities.egg_spawns_allowed(spawns) {
            return Err(EGG_PEACEFUL);
        }
        let source = self.event_source();
        let block = world.get_block(pos).ok_or("that block isn't loaded")?;
        if let Spawns::Mob(kind) = spawns
            && (block == Block::SPAWNER || crate::world::trial::spawner(block).is_some())
        {
            // `Spawner.setEntityId`, unless `spawner_blocks_work` is off.
            // (Spawners hold mobs only here; the dragon's egg on one spawns
            // a dragon in front, a recorded difference.)
            if !entities.spawner_blocks {
                return Err("Spawner blocks are not enabled");
            }
            let set = if block == Block::SPAWNER {
                world.set_spawner(pos, kind)
            } else {
                world.override_trial_spawner_entity(pos, kind)
            };
            if !set {
                return Ok(None);
            }
            self.consume_egg(slot);
            world.emit_block_event(pos, E::BlockChange, source, None);
            return Ok(Some(Used::SpawnerSet { cell: pos }));
        }
        let mut empty = true;
        crate::physics::block_collision_boxes(block, |_, _| empty = false);
        let at = if empty { pos } else { pos + normal };
        let moved_up = at != pos && normal == IVec3::Y;
        let spawned = entities.spawn_egg_spawns(world, spawns, at, true, moved_up).ok_or(EGG_PEACEFUL)?;
        self.consume_egg(slot);
        world.emit_block_event(at, E::EntityPlace, source, None);
        Ok(Some(Used::SpawnedMob { at: spawned }))
    }

    /// [`Self::use_block_inner`], then the game event Java's block posts
    /// for what the use did (doors, levers, buttons, trapdoors, note blocks,
    /// TNT, cake, containers, composters, harvests and fire).
    fn use_block(
        &mut self,
        slot: usize,
        world: &mut World,
        entities: &mut Entities,
        ctx: &UseContext,
        tool_broken: &mut Option<Item>,
    ) -> Result<Option<Used>, &'static str> {
        let used = self.use_block_inner(slot, world, entities, ctx, tool_broken)?;
        if let Some(used) = used {
            self.block_use_event(world, used);
        }
        Ok(used)
    }

    /// The game events Java posts for block uses, with this player as the
    /// source unless Java names none. Redstone uses post theirs in
    /// [`World::use_redstone_by`]; placement in the placement path.
    fn block_use_event(&self, world: &mut World, used: Used) {
        use crate::world::utilities::interact::Effect as U;
        use crate::world::vibration::{EventSource, GameEvent as E};
        use crate::world::wood::ShelfUse;
        let source = self.event_source();
        let none = EventSource::NONE;
        let state = |w: &World, cell: IVec3| w.get_block(cell);
        let (cell, event, source, affected) = match used {
            Used::Door { cell, open: Some(open) } => {
                (cell, if open { E::BlockOpen } else { E::BlockClose }, source, None)
            }
            // `TntBlock.prime(level, pos, player, ...)`.
            Used::TntPrimed { cell } => (cell, E::PrimeFuse, source, None),
            Used::CakeBitten { cell } => {
                // `CakeBlock.eat`: `eat`, then `block_destroy` for the last slice.
                world.emit_block_event(cell, E::Eat, source, None);
                if world.get_block(cell).is_none_or(|b| b.cake_bites().is_none()) {
                    world.emit_block_event(cell, E::BlockDestroy, source, None);
                }
                return;
            }
            // `ContainerOpenersCounter.incrementOpeners` (chests, trapped and
            // copper chests, barrels) and `ShulkerBoxBlockEntity.startOpen`;
            // dispensers, droppers and hoppers post nothing.
            Used::Screen { cell, block } if opens_with_event(world, cell, block) => {
                (cell, E::ContainerOpen, source, None)
            }
            // `ComposterBlock.addLayer` / `empty`, `CaveVines.use` and
            // `SweetBerryBushBlock.useWithoutItem`, `LecternBlock.placeBook`:
            // the new state is the context.
            // A harvested beehive posted its own event at the use.
            Used::Harvested { cell } if state(world, cell).and_then(crate::world::bees::hive).is_some() => return,
            Used::Composter { cell } | Used::Harvested { cell } | Used::LecternBook { cell } => {
                (cell, E::BlockChange, source, state(world, cell))
            }
            // `FlintAndSteelItem` / `FireChargeItem.useOn`: lighting a candle,
            // candle cake or campfire changes it; fire beside is placed.
            Used::Ignited { cell, .. } => {
                let fire = world.get_block(cell).is_some_and(|b| b.is_fire() || b.is_nether_portal());
                (cell, if fire { E::BlockPlace } else { E::BlockChange }, source, None)
            }
            // `BlockTransformer.transformBlock` (shovels, axes, hoes) and
            // `HoneycombItem` / `SignBlock` waxing: the new state.
            Used::Path { cell }
            | Used::Stripped { cell }
            | Used::Scraped { cell }
            | Used::Tilled { cell }
            | Used::Waxed { cell }
            | Used::SignInked { cell, .. } => (cell, E::BlockChange, source, state(world, cell)),
            // `PumpkinBlock.useItemOn`.
            Used::Carved { cell } => (cell, E::Shear, source, None),
            // `AbstractCandleBlock.extinguish`, `CakeBlock.useItemOn`,
            // `CopperGolemStatueBlock.updatePose`.
            Used::CandleExtinguished { cell } | Used::CandlePlacedOnCake { cell } | Used::Statue { cell } => {
                (cell, E::BlockChange, source, None)
            }
            // `BoneMealItem.useOn`: `causeUseVibration` at the player.
            Used::BoneMeal { .. } => {
                world.emit_game_event(self.player.pos, E::ItemInteractFinish, source, None);
                return;
            }
            // `EndCrystalItem.useOn`: the cell above the base.
            Used::CrystalPlaced { at } => (at.floor().as_ivec3(), E::EntityPlace, source, None),
            // `CushionItem.useOn`: at the cushion.
            Used::CushionPlaced { at } => {
                world.emit_game_event(at, E::EntityPlace, source, None);
                return;
            }
            Used::Decor { at, effect } => {
                use crate::entity::decor::Effect as D;
                if matches!(effect, D::Placed(_)) {
                    world.emit_game_event(at, E::EntityPlace, source, None);
                }
                return;
            }
            // `JukeboxPlayable.tryInsertIntoJukebox` (from the player), then
            // `JukeboxBlockEntity.notifyItemChangedInJukebox` (no source).
            Used::Jukebox { cell, inserted } => {
                let block = world.get_block(cell);
                if inserted {
                    world.emit_block_event(cell, E::BlockChange, source, block);
                }
                (cell, E::BlockChange, none, block)
            }
            Used::Shelf { cell, used } => match used {
                // `ShelfBlock.swapSingleItem` / `swapHotbar`: the shelf's state.
                ShelfUse::Placed | ShelfUse::Swapped { .. } => (cell, E::ItemInteractFinish, none, state(world, cell)),
                ShelfUse::Hotbar => (cell, E::EntityInteract, none, state(world, cell)),
                ShelfUse::Nothing => return,
                // `ChiseledBookShelfBlockEntity.updateState`, and
                // `ChiseledBookShelfBlock.removeBook` from the player.
                ShelfUse::BookIn { .. } => (cell, E::BlockChange, none, state(world, cell)),
                ShelfUse::BookOut { .. } => {
                    world.emit_block_event(cell, E::BlockChange, none, state(world, cell));
                    (cell, E::BlockChange, source, None)
                }
            },
            Used::Utility { cell, effect } => match effect {
                // `FlowerPotBlock`, `DecoratedPotBlock.useItemOn`,
                // `CampfireBlockEntity.placeFood`, lighting and dousing,
                // `RespawnAnchorBlock.charge`.
                U::PotPlanted
                | U::PotEmptied
                | U::PotFilled
                | U::FoodPlaced
                | U::Lit
                | U::Doused
                | U::AnchorCharged => (cell, E::BlockChange, source, state(world, cell)),
                // `CauldronInteractions`: none of them names the player.
                U::Cauldron { fill: true, .. } => (cell, E::FluidPlace, none, None),
                U::Cauldron { fill: false, bottle, .. } => {
                    if bottle {
                        // `LayeredCauldronBlock.lowerFillLevel` first.
                        world.emit_block_event(cell, E::BlockChange, none, state(world, cell));
                    }
                    (cell, E::FluidPickup, none, None)
                }
                U::Washed => (cell, E::BlockChange, none, state(world, cell)),
                U::EnderChest => (cell, E::ContainerOpen, source, None),
                // v1.0 Track 9: `DecoratedPotBlock.useWithoutItem`: the
                // wobble posts `block_change` from the player.
                U::Consumed if state(world, cell).and_then(crate::world::utilities::decorated_pot_of).is_some() => {
                    (cell, E::BlockChange, source, None)
                }
                _ => return,
            },
            _ => return,
        };
        world.emit_block_event(cell, event, source, affected);
    }

    fn use_block_inner(
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
        if let Some(used) = self.use_copper_statue(slot, world, entities, pos, ctx, tool_broken) {
            return Ok(Some(used));
        }
        // `VaultBlock.useItemOn`: a held item on an active vault.
        if let Some(stack) = self.inventory.get(slot).filter(|_| self.mode.can_interact()) {
            use crate::world::trial::vault::VaultUse;
            match world.use_vault(pos, self.id.0, Some(stack)) {
                VaultUse::Pass => {}
                VaultUse::Rejected { rewarded, sound } => {
                    return Ok(Some(Used::Vault { cell: pos, unlocked: false, rewarded, sound }));
                }
                VaultUse::Unlocked { consumed } => {
                    if !creative {
                        for _ in 0..consumed {
                            self.inventory.take_one(slot);
                        }
                    }
                    return Ok(Some(Used::Vault { cell: pos, unlocked: true, rewarded: false, sound: true }));
                }
            }
        }
        if can_build
            && !ctx.sneaking
            && matches!(held, Some(Item::SHEARS | Item::GLASS_BOTTLE))
            && world.get_block(pos).and_then(crate::world::bees::hive).is_some_and(|(_, _, h)| h == 5)
            && world.harvest_beehive(entities, pos, self.id)
        {
            // v1.0 Track 9: `BeehiveBlock.useItemOn` posts `shear` or
            // `fluid_pickup` from the player (and no `block_change`).
            let event = if held == Some(Item::SHEARS) {
                crate::world::vibration::GameEvent::Shear
            } else {
                crate::world::vibration::GameEvent::FluidPickup
            };
            world.emit_block_event(pos, event, self.event_source(), None);
            if held == Some(Item::SHEARS) {
                world.drop_honeycomb(pos);
                if survival && self.inventory.wear(slot, 1) {
                    *tool_broken = Some(Item::SHEARS);
                }
            } else {
                // BeehiveBlock shrinks bottles even for creative players.
                survival_items::exchange(&mut self.inventory, slot, Item::HONEY_BOTTLE, false, entities, &self.player);
            }
            return Ok(Some(Used::Harvested { cell: pos }));
        }
        if held == Some(Item::EYE_OF_ENDER) && world.get_block(pos).is_some_and(|b| b.base() == Block::END_PORTAL_FRAME)
        {
            let opened = world.insert_eye(pos).ok_or("that frame already holds an eye, or its ring isn't loaded")?;
            if survival {
                self.inventory.take_one(slot);
            }
            return Ok(Some(Used::EyeInserted { cell: pos, opened }));
        }
        // A bell rings only for a proper hit; otherwise the click passes on.
        let rings = world.get_block(pos).is_none_or(|b| b.base() != Block::BELL)
            || world.bell_hit_rings(pos, self.player.eye(), self.player.forward().as_dvec3());
        // `use_redstone_by` posts the use's game event from this player.
        let source = crate::world::vibration::EventSource::player(self.id, self.player.sneaking);
        // Iron doors and trapdoors pass (`canOpenByHand`), so with an item
        // held its use follows (`redstone_usable` leaves them out).
        if !ctx.sneaking
            && rings
            && (held.is_none() || world.redstone_usable(pos))
            && world.use_redstone_by(pos, source)
        {
            return Ok(Some(Used::Redstone { cell: pos }));
        }
        world.touch_redstone_ore(pos);
        if let Some(used) = self.use_colour_block(slot, world, pos, ctx) {
            return Ok(Some(used));
        }
        // v0.15 Track B: `LecternBlock.useItemOn` / `useWithoutItem`. A
        // lectern with a book opens; an empty one takes a held book, and
        // otherwise swallows the click.
        if !(ctx.sneaking && held.is_some())
            && let Some((_, has_book, _)) = world.get_block(pos).and_then(|b| crate::world::workstations::lectern(b.0))
        {
            if has_book {
                return Ok(Some(Used::Lectern { cell: pos }));
            }
            if let Some(book) = self.inventory.get(slot).filter(|s| crate::content::books::lectern_book(s.item))
                && can_build
                && world.place_lectern_book(pos, book)
            {
                // `consumeAndReturn`: players with infinite materials keep it.
                if !creative {
                    self.inventory.take_one(slot);
                }
                return Ok(Some(Used::LecternBook { cell: pos }));
            }
            let effect = crate::world::utilities::interact::Effect::Consumed;
            return Ok(Some(Used::Utility { cell: pos, effect }));
        }
        // Java skips block use only when sneaking with something in hand.
        if !(ctx.sneaking && held.is_some())
            && let Some(effect) = crate::world::utilities::interact::use_block(
                world,
                entities,
                &self.player,
                &mut self.inventory,
                slot,
                self.mode,
                pos,
                normal,
            )
        {
            use crate::world::utilities::interact::Effect;
            // `RespawnAnchorBlock.useWithoutItem` in the Nether: the anchor
            // becomes the respawn point, replacing any bed.
            let effect = match effect {
                Effect::AnchorSpawn if self.spawn_anchor == Some(pos) => Effect::Consumed,
                Effect::AnchorSpawn => {
                    (self.spawn_anchor, self.spawn_bed, self.spawn_point) = (Some(pos), None, None);
                    effect
                }
                other => other,
            };
            return Ok(Some(Used::Utility { cell: pos, effect }));
        }
        // Java skips block use only when sneaking with something in hand.
        if !(ctx.sneaking && held.is_some())
            && let Some(used) = self.use_jukebox(slot, world, pos)
        {
            return Ok(Some(used));
        }
        if !ctx.sneaking && survival_items::use_composter(world, entities, pos, &mut self.inventory, slot, creative) {
            return Ok(Some(Used::Composter { cell: pos }));
        }
        if let Some(used) = self.use_wood_block(slot, world, ctx.sneaking) {
            return Ok(Some(used));
        }
        // Containers open on right-click; sneaking builds against them.
        if !ctx.sneaking {
            match world.get_block(pos) {
                // `ChestBlock.getMenuProvider`: a chest blocked from above stays shut.
                Some(b) if crate::world::double_chest::state(b).is_some() && !world.chest_opens(pos) => {
                    let effect = crate::world::utilities::interact::Effect::Consumed;
                    return Ok(Some(Used::Utility { cell: pos, effect }));
                }
                // v0.16 Track B: `ShulkerBoxBlock.canOpen`: no room for the lid.
                Some(b) if crate::world::shulker_box::owns(b.0) && !world.shulker_box_opens(pos) => {
                    let effect = crate::world::utilities::interact::Effect::Consumed;
                    return Ok(Some(Used::Utility { cell: pos, effect }));
                }
                Some(b)
                    if b == Block::CRAFTING_TABLE
                        || crate::world::furnace::is_furnace(b)
                        || crate::world::chest::is_chest(b)
                        || b == Block::BREWING_STAND
                        || b == Block::ENCHANTING_TABLE
                        || b.is_anvil()
                        || b.base() == Block::GRINDSTONE
                        || b == Block::SMITHING_TABLE
                        || crate::content::interaction::workstation_screen(b) =>
                {
                    return Ok(Some(Used::Screen { cell: pos, block: b }));
                }
                Some(Block::DRAGON_EGG) => {
                    return Ok(Some(Used::EggTeleported { destination: world.teleport_egg(pos) }));
                }
                // `CakeBlock.eat` passes when the player can't eat; with an
                // item held, that item's use follows (a spawn egg, a block).
                Some(b) if b.cake_bites().is_some() && held.is_none() => {
                    if !survival_items::bite_cake(world, pos, &mut self.vitals, creative) {
                        return Ok(None);
                    }
                    return Ok(Some(Used::CakeBitten { cell: pos }));
                }
                Some(b)
                    if b.cake_bites().is_some()
                        && survival_items::bite_cake(world, pos, &mut self.vitals, creative) =>
                {
                    return Ok(Some(Used::CakeBitten { cell: pos }));
                }
                Some(b) if b.is_bed() => return Ok(Some(Used::Bed { cell: pos })),
                Some(_) if held != Some(Item::BONE_MEAL) && world.harvest(pos) => {
                    return Ok(Some(Used::Harvested { cell: pos }));
                }
                // `DoorBlock.useWithoutItem` passes for a door hands can't
                // open (iron); with an item held, that item's use follows.
                Some(b)
                    if (b.is_door() || b.is_gate())
                        && (held.is_none()
                            || !matches!(
                                crate::world::redstone_blocks::component(b),
                                Some(crate::world::redstone_blocks::Component::IronDoor { .. })
                            )) =>
                {
                    let open = toggle_door(world, pos, self.player.forward());
                    return Ok(Some(Used::Door { cell: pos, open }));
                }
                _ => {}
            }
        }
        if !can_build {
            return Ok(None);
        }
        // v0.18 Track 4: `SpawnEggItem.useOn`, after the block's own use
        // passed (containers, doors, berry bushes and the rest come first).
        // A mob under the crosshair that wouldn't breed leaves it unused.
        if let Some(kind) = held.and_then(crate::content::spawn_egg::spawns) {
            if self.mob_target(world, entities).is_some() {
                return Ok(None);
            }
            return self.spawn_egg_on_block(slot, kind, world, entities, (pos, normal));
        }
        let wear = |s: &mut Self, amount: u16, broken: &mut Option<Item>| {
            if survival
                && let Some(item) = held
                && s.inventory.wear(slot, amount)
            {
                *broken = Some(item);
            }
        };
        // `FlintAndSteelItem.useOn` and `FireChargeItem.useOn` light candles
        // and candle cakes before setting fire to the cell beside them.
        if matches!(held, Some(Item::FLINT_AND_STEEL | Item::FIRE_CHARGE)) && world.light_candle(pos) {
            if held == Some(Item::FIRE_CHARGE) {
                if survival {
                    self.inventory.take_one(slot);
                }
            } else {
                wear(self, 1, tool_broken);
            }
            return Ok(Some(Used::Ignited { cell: pos, portal: false, charge: held == Some(Item::FIRE_CHARGE) }));
        }
        // `TntBlock.useItemOn`: flint and steel or a fire charge primes TNT
        // in the player's name (`prime(level, pos, player, ..)`, v1.0 gap
        // Track 8e), the charge used up outside creative.
        if matches!(held, Some(Item::FLINT_AND_STEEL | Item::FIRE_CHARGE))
            && world.get_block(pos) == Some(Block::TNT)
            && world.rules.tnt_explodes
        {
            world.set_block(pos, Block::AIR);
            entities.prime_tnt(pos, false, Some(crate::entity::trident::Owner::Player(self.id)));
            if held == Some(Item::FIRE_CHARGE) {
                if survival {
                    self.inventory.take_one(slot);
                }
            } else {
                wear(self, 1, tool_broken);
            }
            return Ok(Some(Used::TntPrimed { cell: pos }));
        }
        if held == Some(Item::FLINT_AND_STEEL) {
            let at = pos + normal;
            if !world.ignite(at) {
                return Ok(Some(Used::FlintFizzled));
            }
            wear(self, 1, tool_broken);
            let portal = world.get_block(at).is_some_and(Block::is_nether_portal);
            return Ok(Some(Used::Ignited { cell: at, portal, charge: false }));
        }
        // v1.0 gap Track 8f: `FireChargeItem.useOn` sets fire in the cell
        // beside the clicked face where `BaseFireBlock.canBePlacedAt` allows
        // (lighting a portal frame there too), the charge used up outside
        // creative; where fire can't go it does nothing and keeps the
        // charge (`InteractionResult.FAIL`).
        if held == Some(Item::FIRE_CHARGE) {
            let at = pos + normal;
            if !world.ignite(at) {
                return Ok(None);
            }
            if survival {
                self.inventory.take_one(slot);
            }
            let portal = world.get_block(at).is_some_and(Block::is_nether_portal);
            return Ok(Some(Used::Ignited { cell: at, portal, charge: true }));
        }
        if let Some(used) = self.use_item_on(slot, world, entities, pos, normal, tool_broken) {
            return Ok(Some(used));
        }
        // v0.16 Track G: `EndCrystalItem.useOn`, on the clicked block
        // whatever face was hit.
        if held == Some(crate::entity::crystal::END_CRYSTAL) {
            let mut players = ctx.others.to_vec();
            players.push(self.player.pos);
            if !entities.place_crystal(world, pos, &players) {
                return Err("an End crystal needs obsidian or bedrock with room above it");
            }
            if survival {
                self.inventory.take_one(slot);
            }
            let at = pos.as_dvec3() + DVec3::new(0.5, 1.0, 0.5);
            return Ok(Some(Used::CrystalPlaced { at }));
        }
        // `HangingEntityItem` and `ArmorStandItem`: decorations hang on or
        // stand against the clicked face.
        if let Some(item) = held
            && let Some(placed) = {
                let mut players = ctx.others.to_vec();
                players.push(self.player.pos);
                let yaw = crate::knockback::java_yaw_degrees(self.player.yaw);
                entities.place_decor(world, item, pos, normal, yaw, &players)
            }
        {
            let effect = placed.ok_or("that decoration doesn't fit there")?;
            if survival {
                self.inventory.take_one(slot);
            }
            return Ok(Some(Used::Decor { at: pos.as_dvec3() + DVec3::splat(0.5) + normal.as_dvec3() * 0.5, effect }));
        }
        let result = self.place_held(slot, world, ctx.others)?;
        Ok(Some(Used::Placed(result)))
    }

    /// Candle and cake uses (Java 26.3 `CandleBlock`, `CandleCakeBlock` and
    /// `CakeBlock.useItemOn`): a bare hand puts out a lit candle (or a candle
    /// cake's flame, clicked on the candle), a candle goes onto an uneaten
    /// cake, and a candle cake is eaten from like a cake, dropping its
    /// candle. Flint and steel and fire charges pass to the item.
    fn use_colour_block(&mut self, slot: usize, world: &mut World, pos: IVec3, ctx: &UseContext) -> Option<Used> {
        use crate::world::colour_blocks::candle;
        let block = world.get_block(pos)?;
        let held = self.inventory.get(slot).map(|s| s.item);
        // Sneaking with an item skips block use (`ServerPlayerGameMode.useItemOn`).
        if ctx.sneaking && held.is_some() {
            return None;
        }
        let creative = self.mode.is_creative();
        // `AbstractBedBlock.useWithoutItem`: the adapter applies the straw bed's rules.
        if crate::world::colour_blocks::straw_bed::decode(block.0).is_some() && !ctx.sneaking {
            return Some(Used::Bed { cell: pos });
        }
        if candle::decode(block.0).is_some() {
            if held.is_none() && self.mode.can_build() && candle::is_lit(block) && world.extinguish_candle(pos) {
                return Some(Used::CandleExtinguished { cell: pos });
            }
            return None;
        }
        if block == Block::cake(0)
            && let Some(item) = held.filter(|&i| candle::colour_of_item(i).is_some())
        {
            if !self.mode.can_build() || !world.add_candle_to_cake(pos, item) {
                return None;
            }
            if !creative {
                self.inventory.take_one(slot);
            }
            return Some(Used::CandlePlacedOnCake { cell: pos });
        }
        candle::decode_cake(block.0)?;
        if matches!(held, Some(Item::FLINT_AND_STEEL | Item::FIRE_CHARGE)) {
            return None;
        }
        let click_y = self.block_hit(world).map_or(0.0, |hit| hit.point.y - pos.y as f64);
        if held.is_none() && candle::candle_hit(click_y) && candle::is_lit(block) && world.extinguish_candle(pos) {
            return Some(Used::CandleExtinguished { cell: pos });
        }
        // `CakeBlock.eat` with the plain cake's state, then the candle drops.
        if !creative && !self.vitals.hunger.can_eat() {
            return None;
        }
        self.vitals.hunger.eat(2, 0.4);
        world.bite_candle_cake(pos).then_some(Used::CakeBitten { cell: pos })
    }

    /// Shovel paths, axe stripping and scraping, hoe tilling, pumpkin carving and bone meal.
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
        // `CushionItem.useOn`: a cushion sits on the top face clicked.
        if let Some(colour) = item.cushion_color() {
            let hit = self.block_hit(world)?;
            let at = crate::world::colour_blocks::cushion::placement(world, hit.cell, hit.normal, hit.point)?;
            let quarter = std::f32::consts::FRAC_PI_2;
            let yaw = (self.player.yaw / quarter).round() * quarter;
            let id = entities.place_cushion(colour as u8, at, yaw)?;
            entities.burn_cushion(world, id);
            if survival {
                self.inventory.take_one(slot);
            }
            return Some(Used::CushionPlaced { at });
        }
        if self.lock_compass(world, entities, slot, pos) {
            return Some(Used::CompassLocked { cell: pos });
        }
        if item == Item::HONEYCOMB && world.wax_copper(pos) {
            if survival {
                self.inventory.take_one(slot);
            }
            return Some(Used::Waxed { cell: pos });
        }
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
        // Axes strip stems, hyphae, logs, wood and bamboo blocks, keeping
        // their axis (Java's `block_transformer/axe.json`).
        if tool == Some(ToolKind::Axe)
            && let Some(stripped) =
                crate::world::nether_biome_blocks::stripped(block).or_else(|| crate::world::wood::stripped(block))
            && world.set_block(pos, stripped)
        {
            wear(self);
            return Some(Used::Stripped { cell: pos });
        }
        // The axe's copper `scrape` transformer (Java's block_transformer/axe.json).
        if tool == Some(ToolKind::Axe) && world.scrape_copper(pos) {
            wear(self);
            return Some(Used::Scraped { cell: pos });
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
        if item == Item::BONE_MEAL && world.use_bone_meal(pos) {
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
    if let Some(item) = crate::world::utilities::picked(block) {
        return item;
    }
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
        b if crate::world::sniffer_plants::picked(b).is_some() => crate::world::sniffer_plants::picked(b).unwrap(),
        b if crate::world::crops::stem(b).is_some() => crate::world::crops::stem(b).unwrap().0.seed(),
        b if matches!(b.as_crop(), Some((crate::world::block::Crop::Beetroot, _))) => Item::BEETROOT_SEEDS,
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
        // `CandleCakeBlock.getCloneItemStack`.
        b if crate::world::colour_blocks::candle::decode_cake(b.0).is_some() => Item::CAKE,
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
    /// The hotbar slot already holding the targeted block's item, if any: the
    /// part of pick-block that only changes the selection. Read-only, so a LAN
    /// client can select locally while the host performs any swap.
    pub fn pick_hotbar_slot(&self, world: &World) -> Option<usize> {
        let (pos, _) = self.block_target(world)?;
        let b = world.get_block(pos)?;
        if b.is_fire() {
            return None;
        }
        let found = if let Some(stack) = world.copper_golem_statue_item(pos) {
            self.inventory.slots.iter().position(|s| s.is_some_and(|s| s.stacks_with(&stack)))
        } else {
            self.inventory.find(picked_item(b))
        };
        found.filter(|&i| i < HOTBAR_SLOTS)
    }

    /// Middle click on the targeted block: find its item in the inventory,
    /// or conjure a stack in Creative.
    pub fn pick_block(&mut self, selected: usize, world: &World) -> Option<Pick> {
        let (pos, _) = self.block_target(world)?;
        let b = world.get_block(pos)?;
        if b.is_fire() || selected >= HOTBAR_SLOTS {
            return None;
        }
        let item = picked_item(b);
        let statue = world.copper_golem_statue_item(pos);
        let found = if let Some(stack) = statue {
            self.inventory.slots.iter().position(|s| s.is_some_and(|s| s.stacks_with(&stack)))
        } else {
            self.inventory.find(item)
        };
        match found {
            Some(i) if i < HOTBAR_SLOTS => Some(Pick::Select(i)),
            Some(i) => {
                self.cancel_consumption();
                self.inventory.slots.swap(i, selected);
                Some(Pick::Replaced)
            }
            None if self.mode == crate::rules::GameMode::Creative => {
                self.cancel_consumption();
                self.inventory.slots[selected] =
                    Some(statue.unwrap_or_else(|| crate::inventory::Stack::new(item, item.max_stack())));
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

/// Whether opening the container at `cell` posts `container_open`: the
/// first viewer of a chest, trapped, copper or ender chest or a barrel
/// (`ContainerOpenersCounter`), or a shulker box (`startOpen`).
fn opens_with_event(world: &World, cell: IVec3, block: Block) -> bool {
    let counted = crate::world::chest::is_chest(block)
        && !matches!(
            crate::world::redstone_blocks::component(block),
            Some(
                crate::world::redstone_blocks::Component::Dispenser { .. }
                    | crate::world::redstone_blocks::Component::Hopper { .. }
            )
        );
    counted && !world.container_has_viewers(cell)
}
