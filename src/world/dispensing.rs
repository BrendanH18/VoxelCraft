//! v0.17 Track C2: Java 26.3's dispense table (`DispenserBlock.
//! getDispenseMethod`: the `DispenseItemBehavior.bootStrap` registry, then
//! the equippable, sulfur-cube and spawn-egg fallbacks), checked both ways
//! against `tests/fixtures/java/26.3/dispensing.json` by
//! `tests/dispensing.rs`, and the world half of each behaviour.
//!
//! `World::dispense` (in `automation`) picks the behaviour. Behaviours that
//! only touch blocks run there, in the dispenser's scheduled tick. Those
//! that look for entities (equipment, shears, brushes, chests onto mounts,
//! flint and steel, glass bottles at hives) are queued with the slot they
//! came from and finish in `World::tick_automation_entities`, later in the
//! same game tick, where the entities are; the item leaves the dispenser
//! only then. Spawned entities (projectiles, boats, minecarts, armour
//! stands) leave the dispenser at once and appear in that same pass.
use glam::{DVec3, IVec3};

use super::{World, block::Block, redstone_blocks as r};
use crate::entity::equipment::Slot;
use crate::entity::{Entities, MobKind, dispensed::DispenseTarget};
use crate::inventory::Stack;
use crate::item::Item;

/// The behaviour Java's dispenser gives an item. The names are the
/// fixture's (`tools/extract_java_dispense_fixtures.py`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Behaviour {
    /// `DefaultDispenseItemBehavior`: the item pops out.
    Default,
    /// `ProjectileDispenseBehavior` with the item's `DispenseConfig`.
    Projectile,
    ArmorStand,
    /// A chest onto a tamed donkey, mule or llama.
    ChestOnMount,
    /// `BoatDispenseItemBehavior`.
    Boat,
    /// Water, lava, powder snow and mob buckets emptied in front.
    FilledBucket,
    /// An empty bucket filled from the fluid in front.
    EmptyBucket,
    FlintAndSteel,
    BoneMeal,
    Tnt,
    WitherSkull,
    CarvedPumpkin,
    /// `ShulkerBoxDispenseBehavior`.
    ShulkerBox,
    /// Filled at a full hive or from water.
    GlassBottle,
    /// Charges a respawn anchor.
    Glowstone,
    /// `ShearsDispenseItemBehavior`.
    Shears,
    /// Brushes an armadillo.
    Brush,
    /// Waxes copper.
    Honeycomb,
    /// A water bottle turning dirt into mud.
    WaterPotion,
    /// `MinecartDispenseItemBehavior`.
    Minecart,
    /// `EquipmentDispenseItemBehavior`: any item with `EQUIPPABLE`.
    Equipment,
    /// `SulfurCubeBlockDispenseItemBehavior`: `#sulfur_cube_swallowable`.
    SulfurCubeBlock,
    /// `SpawnEggItemBehavior`. VoxelCraft has no spawn eggs yet (v0.18).
    SpawnEgg,
}

impl Behaviour {
    pub const ALL: [Behaviour; 23] = [
        Behaviour::Default,
        Behaviour::Projectile,
        Behaviour::ArmorStand,
        Behaviour::ChestOnMount,
        Behaviour::Boat,
        Behaviour::FilledBucket,
        Behaviour::EmptyBucket,
        Behaviour::FlintAndSteel,
        Behaviour::BoneMeal,
        Behaviour::Tnt,
        Behaviour::WitherSkull,
        Behaviour::CarvedPumpkin,
        Behaviour::ShulkerBox,
        Behaviour::GlassBottle,
        Behaviour::Glowstone,
        Behaviour::Shears,
        Behaviour::Brush,
        Behaviour::Honeycomb,
        Behaviour::WaterPotion,
        Behaviour::Minecart,
        Behaviour::Equipment,
        Behaviour::SulfurCubeBlock,
        Behaviour::SpawnEgg,
    ];

    /// The fixture's name for the behaviour.
    pub const fn name(self) -> &'static str {
        match self {
            Behaviour::Default => "default",
            Behaviour::Projectile => "projectile",
            Behaviour::ArmorStand => "armor_stand",
            Behaviour::ChestOnMount => "chest_on_mount",
            Behaviour::Boat => "boat",
            Behaviour::FilledBucket => "filled_bucket",
            Behaviour::EmptyBucket => "empty_bucket",
            Behaviour::FlintAndSteel => "flint_and_steel",
            Behaviour::BoneMeal => "bone_meal",
            Behaviour::Tnt => "tnt",
            Behaviour::WitherSkull => "wither_skull",
            Behaviour::CarvedPumpkin => "carved_pumpkin",
            Behaviour::ShulkerBox => "shulker_box",
            Behaviour::GlassBottle => "glass_bottle",
            Behaviour::Glowstone => "glowstone",
            Behaviour::Shears => "shears",
            Behaviour::Brush => "brush",
            Behaviour::Honeycomb => "honeycomb",
            Behaviour::WaterPotion => "water_potion",
            Behaviour::Minecart => "minecart",
            Behaviour::Equipment => "equipment",
            Behaviour::SulfurCubeBlock => "sulfur_cube_block",
            Behaviour::SpawnEgg => "spawn_egg",
        }
    }

    /// Behaviours that look for entities in front, so they finish in the
    /// entity pass.
    pub const fn needs_entities(self) -> bool {
        matches!(
            self,
            Behaviour::ChestOnMount
                | Behaviour::FlintAndSteel
                | Behaviour::GlassBottle
                | Behaviour::Shears
                | Behaviour::Brush
                | Behaviour::Equipment
        )
    }
}

fn is_block(item: Item, block: Block) -> bool {
    item.block() == Some(block)
}

/// `DispenserBlock.getDispenseMethod` for `item`.
pub fn behaviour(item: Item) -> Behaviour {
    use crate::entity::minecart::CartKind;
    if projectile(item).is_some() {
        return Behaviour::Projectile;
    }
    let registered = match item {
        Item::ARMOR_STAND => Some(Behaviour::ArmorStand),
        Item::BUCKET => Some(Behaviour::EmptyBucket),
        Item::WATER_BUCKET | Item::LAVA_BUCKET | Item::POWDER_SNOW_BUCKET | crate::entity::sulfur_cube::BUCKET => {
            Some(Behaviour::FilledBucket)
        }
        Item::FLINT_AND_STEEL => Some(Behaviour::FlintAndSteel),
        Item::BONE_MEAL => Some(Behaviour::BoneMeal),
        Item::GLASS_BOTTLE => Some(Behaviour::GlassBottle),
        Item::SHEARS => Some(Behaviour::Shears),
        Item::BRUSH => Some(Behaviour::Brush),
        Item::HONEYCOMB => Some(Behaviour::Honeycomb),
        _ => None,
    };
    if let Some(b) = registered {
        return b;
    }
    if crate::entity::aquatic::bucket_kind(item).is_some() {
        return Behaviour::FilledBucket;
    }
    if is_block(item, Block::CHEST) {
        return Behaviour::ChestOnMount;
    }
    if item.boat_of().is_some() {
        return Behaviour::Boat;
    }
    // Every cart item but boats and cushions (Track D1's furnace minecart
    // included once it exists).
    if CartKind::from_item(item).is_some_and(|k| k.boat().is_none() && k.cushion().is_none()) {
        return Behaviour::Minecart;
    }
    if is_block(item, Block::TNT) {
        return Behaviour::Tnt;
    }
    if item == super::skulls::item(super::skulls::Kind::WitherSkeleton) {
        return Behaviour::WitherSkull;
    }
    if is_block(item, Block::CARVED_PUMPKIN) {
        return Behaviour::CarvedPumpkin;
    }
    if crate::content::shulker_box::is_shulker_box(item) {
        return Behaviour::ShulkerBox;
    }
    if is_block(item, Block::GLOWSTONE) {
        return Behaviour::Glowstone;
    }
    // Java registers `Items.POTION`; VoxelCraft gives each potion an id.
    if item.as_potion().is_some() {
        return Behaviour::WaterPotion;
    }
    if equippable(item).is_some() {
        return Behaviour::Equipment;
    }
    if crate::entity::sulfur_cube::swallowable(item) {
        return Behaviour::SulfurCubeBlock;
    }
    Behaviour::Default
}

/// `Equippable.allowedEntities`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Allowed {
    /// No restriction.
    Any,
    /// Carpets: llamas and trader llamas.
    Llamas,
    /// `#can_equip_harness`.
    Harness,
    /// `#can_wear_horse_armor`.
    HorseArmor,
    /// `#can_wear_nautilus_armor`.
    NautilusArmor,
    /// `#can_equip_saddle`.
    Saddle,
    /// Wolf armour.
    Wolf,
}

impl Allowed {
    /// Whether a mob of `kind` may wear the item (`canBeEquippedBy`).
    pub fn admits(self, kind: MobKind) -> bool {
        use crate::entity::nautilus::is_nautilus;
        match self {
            Allowed::Any => true,
            Allowed::Llamas => matches!(kind, MobKind::Llama | MobKind::TraderLlama),
            Allowed::Harness => kind == MobKind::HappyGhast,
            Allowed::HorseArmor => matches!(kind, MobKind::Horse | MobKind::ZombieHorse),
            Allowed::NautilusArmor => is_nautilus(kind),
            Allowed::Saddle => {
                matches!(
                    kind,
                    MobKind::Camel
                        | MobKind::CamelHusk
                        | MobKind::Donkey
                        | MobKind::Horse
                        | MobKind::Mule
                        | MobKind::Pig
                        | MobKind::SkeletonHorse
                        | MobKind::Strider
                        | MobKind::ZombieHorse
                ) || is_nautilus(kind)
            }
            Allowed::Wolf => kind == MobKind::Wolf,
        }
    }
}

/// An item's `EQUIPPABLE` component: its slot and who may wear it. Every
/// one in 26.3 is `dispensable`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Equippable {
    pub slot: Slot,
    pub allowed: Allowed,
}

/// The `EQUIPPABLE` component of `item`, if it has one.
pub fn equippable(item: Item) -> Option<Equippable> {
    use crate::item::ArmorPiece;
    let any = |slot| Some(Equippable { slot, allowed: Allowed::Any });
    if let Some(piece) = item.equip_piece() {
        return any(match piece {
            ArmorPiece::Helmet => Slot::Head,
            ArmorPiece::Chestplate => Slot::Chest,
            ArmorPiece::Leggings => Slot::Legs,
            ArmorPiece::Boots => Slot::Feet,
        });
    }
    if super::skulls::kind_of_item(item).is_some() || is_block(item, Block::CARVED_PUMPKIN) {
        return any(Slot::Head);
    }
    if item == Item::SHIELD {
        return any(Slot::OffHand);
    }
    let body = |allowed| Some(Equippable { slot: Slot::Body, allowed });
    if item == Item::SADDLE {
        return Some(Equippable { slot: Slot::Saddle, allowed: Allowed::Saddle });
    }
    if item == Item::WOLF_ARMOR {
        return body(Allowed::Wolf);
    }
    if let Some((fits, _)) = item.body_armor() {
        return body(match fits {
            crate::equipment::BodyArmor::Horse => Allowed::HorseArmor,
            crate::equipment::BodyArmor::Nautilus => Allowed::NautilusArmor,
        });
    }
    if crate::entity::mounts::is_carpet(item) {
        return body(Allowed::Llamas);
    }
    if crate::content::harness::colour(item).is_some() {
        return body(Allowed::Harness);
    }
    None
}

/// A projectile item's `ProjectileItem.DispenseConfig`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Projectile {
    /// Blocks per tick.
    pub power: f64,
    pub uncertainty: f64,
    /// The level event the dispenser plays (`overrideDispenseEvent`, else 1002).
    pub event: u16,
}

/// The `DispenseConfig` of `item`, if the dispenser shoots it.
pub fn projectile(item: Item) -> Option<Projectile> {
    use crate::content::farm_variant::{BLUE_EGG, BROWN_EGG};
    let config = |power, uncertainty, event| Some(Projectile { power, uncertainty, event });
    match item {
        Item::ARROW | Item::SPECTRAL_ARROW | Item::TIPPED_ARROW | Item::EGG | Item::SNOWBALL => config(1.1, 6.0, 1002),
        BLUE_EGG | BROWN_EGG => config(1.1, 6.0, 1002),
        // `uncertainty * 0.5`, `power * 1.25`.
        Item::EXPERIENCE_BOTTLE | Item::LINGERING_POTION => config(1.375, 3.0, 1002),
        _ if item.as_splash_potion().is_some() => config(1.375, 3.0, 1002),
        Item::FIRE_CHARGE => config(1.0, f64::from(6.666_666_5_f32), 1018),
        crate::world::trial::WIND_CHARGE => config(1.0, f64::from(6.666_666_5_f32), 1051),
        crate::content::fireworks::FIREWORK_ROCKET => config(0.5, 1.0, 1004),
        _ => None,
    }
}

/// Where `item`'s projectile appears relative to the dispenser's centre
/// facing `step` (`DispenseConfig.positionFunction`): 0.7 out and 0.1 up
/// by default, a block out for charges, half a block out for rockets.
pub fn projectile_offset(item: Item, step: IVec3) -> DVec3 {
    let d = step.as_dvec3();
    match item {
        Item::FIRE_CHARGE | crate::world::trial::WIND_CHARGE => d,
        crate::content::fireworks::FIREWORK_ROCKET => d * 0.500_009_999_999_747_4,
        _ => d * 0.7 + DVec3::Y * 0.1,
    }
}

/// What an entity-pass behaviour did with its item.
enum Outcome {
    /// Nothing happened; the item stays (`OptionalDispenseItemBehavior` failed).
    Keep,
    /// One item was used up.
    Consume,
    /// One item was used up, leaving `remainder` (`consumeWithRemainder`).
    Remainder(Item),
    /// The tool took this much wear (`hurtAndBreak`).
    Wear(u16),
    /// The item popped out as a plain drop.
    Drop,
}

/// An overlay with a carved pumpkin at `head`, for `canSpawnGolem`.
struct WithPumpkin<'a> {
    world: &'a World,
    head: IVec3,
}

impl crate::physics::BlockSource for WithPumpkin<'_> {
    fn block(&self, p: IVec3) -> Option<Block> {
        if p == self.head { Some(Block::CARVED_PUMPKIN) } else { self.world.get_block(p) }
    }
}

impl World {
    /// `CarvedPumpkinBlock.canSpawnGolem`: a snow, iron or copper golem
    /// base waits under `head`.
    fn golem_base_at(&self, head: IVec3) -> bool {
        let overlay = WithPumpkin { world: self, head };
        if crate::entity::golem::pattern_at(&overlay, head).is_some() {
            return true;
        }
        let below = self.get_block(head - IVec3::Y).and_then(super::copper::state);
        below.is_some_and(|c| c.family == super::copper::Family::Block)
    }

    /// The carved pumpkin behaviour's block half: into an empty cell that
    /// completes a golem (which `entity::golem::finish_golems` then builds).
    pub(super) fn place_dispensed_pumpkin(&mut self, front: IVec3) -> bool {
        if self.get_block(front) != Some(Block::AIR) || !self.golem_base_at(front) {
            return false;
        }
        self.set_block(front, Block::CARVED_PUMPKIN);
        self.emit_game_event(
            front.as_dvec3() + DVec3::splat(0.5),
            super::vibration::GameEvent::BlockPlace,
            super::vibration::EventSource::NONE,
            None,
        );
        true
    }

    /// The glowstone behaviour: `Some(true)` charged a respawn anchor,
    /// `Some(false)` met a full one (the glowstone stays), `None` met
    /// something else (it pops out).
    pub(super) fn dispense_glowstone(&mut self, front: IVec3) -> Option<bool> {
        let charges = self.get_block(front).and_then(super::utilities::anchor_charges)?;
        if charges == 4 {
            return Some(false);
        }
        self.set_block(front, super::utilities::respawn_anchor(charges + 1));
        Some(true)
    }

    /// The water bottle behaviour's block half: `#convertible_to_mud`
    /// (dirt, coarse dirt, rooted dirt) in front becomes mud.
    pub(super) fn dispense_water_onto_dirt(&mut self, front: IVec3) -> bool {
        use super::overworld_blocks::{COARSE_DIRT, MUD, ROOTED_DIRT};
        if !matches!(self.get_block(front), Some(b) if b == Block::DIRT || b == COARSE_DIRT || b == ROOTED_DIRT) {
            return false;
        }
        self.set_block(front, MUD);
        true
    }

    /// `BoatDispenseItemBehavior`: where a boat goes from the dispenser at
    /// `p` facing `step`, or `None` when it pops out instead. Water in
    /// front floats it a block up; air over water sets it on the surface.
    pub fn dispensed_boat_pos(&self, p: IVec3, step: IVec3) -> Option<DVec3> {
        // `0.5625 + EntityType.getWidth() / 2`; every boat is 1.375 wide.
        let out = 0.5625 + 1.375 / 2.0;
        let front = p + step;
        let y_offset = if self.get_block(front).is_some_and(|b| b.holds_water()) {
            1.0
        } else if self.get_block(front) == Some(Block::AIR)
            && self.get_block(front - IVec3::Y).is_some_and(|b| b.holds_water())
        {
            0.0
        } else {
            return None;
        };
        let centre = p.as_dvec3() + DVec3::splat(0.5);
        let d = step.as_dvec3();
        Some(DVec3::new(centre.x + d.x * out, centre.y + d.y * 1.125 + y_offset, centre.z + d.z * out))
    }

    /// `MinecartDispenseItemBehavior`: where a cart goes from the dispenser
    /// at `p` facing `step` (onto a rail in front, or into the air over
    /// one), or `None` when it pops out instead.
    pub fn dispensed_cart_pos(&self, p: IVec3, step: IVec3) -> Option<DVec3> {
        let front = p + step;
        let block = self.get_block(front)?;
        let y_offset = if let Some(shape) = block.rail_shape() {
            if shape.is_ascending() { 0.6 } else { 0.1 }
        } else {
            if block != Block::AIR {
                return None;
            }
            let shape = self.get_block(front - IVec3::Y)?.rail_shape()?;
            if step != IVec3::NEG_Y && shape.is_ascending() { -0.4 } else { -0.9 }
        };
        let centre = p.as_dvec3() + DVec3::splat(0.5);
        let d = step.as_dvec3();
        Some(DVec3::new(centre.x + d.x * 1.125, centre.y.floor() + d.y + y_offset, centre.z + d.z * 1.125))
    }

    /// `FlintAndSteelDispenseItemBehavior`'s block half, after no sulfur
    /// cube took the spark: fire in front, else a campfire, candle or
    /// candle cake lit, else TNT primed. Like Java, a spark at nothing still
    /// counts (and wears the flint and steel); only TNT that can't explode
    /// fails.
    fn ignite_dispensed_block(&mut self, front: IVec3) -> bool {
        if self.ignite(front) || self.light_campfire(front) || self.light_candle(front) {
            return true;
        }
        if self.get_block(front) == Some(Block::TNT) {
            if !self.rules.tnt_explodes {
                return false;
            }
            self.set_block(front, Block::AIR);
            self.primed_tnt.push((front, false));
        }
        true
    }

    /// `BeehiveBlock.releaseBeesAndResetHoneyLevel` with no player: a hive
    /// at honey level 5 in front empties of honey and lets its bees out.
    fn harvest_dispensed_hive(&mut self, entities: &mut Entities, front: IVec3) -> bool {
        let Some((nest, facing, 5)) = self.get_block(front).and_then(super::bees::hive) else {
            return false;
        };
        self.release_hive_bees(entities, front);
        self.set_block(front, super::bees::block(nest, facing, 0));
        true
    }

    /// `consumeWithRemainder`: one item of the stack in `slot` is used up
    /// and `remainder` takes its place, or goes into another slot, or pops
    /// out.
    pub(super) fn consume_with_remainder(&mut self, p: IVec3, facing: u8, slot: usize, remainder: Item) {
        let Some(stack) = self.container_stack(p, slot) else { return };
        if stack.count <= 1 {
            self.set_container_stack(p, slot, Some(Stack::new(remainder, 1)));
            return;
        }
        self.set_container_stack(p, slot, Some(Stack { count: stack.count - 1, ..stack }));
        if let Some(left) = self.insert_dispenser(p, Stack::new(remainder, 1)) {
            self.queue_dispensed(p, facing, super::automation::Output::Item, left);
        }
    }

    /// `ItemStack.hurtAndBreak` with no holder for the tool in `slot`:
    /// Unbreaking may spare each point; a worn-out tool is gone.
    fn wear_dispensed(&mut self, p: IVec3, slot: usize, amount: u16) {
        let Some(mut stack) = self.container_stack(p, slot) else { return };
        let Some(max) = stack.item.durability() else { return };
        let uses = (0..amount).filter(|_| crate::enchant::wears(stack.enchants, false, crate::enchant::roll())).count();
        stack.damage = stack.damage.saturating_add(uses as u16);
        self.set_container_stack(p, slot, (stack.damage < max).then_some(stack));
    }

    /// Finishes the entity-pass behaviours queued by `dispense`, in order.
    /// A queued item that left its slot since (a player took it) is skipped.
    pub(super) fn run_deferred_dispenses(&mut self, entities: &mut Entities, players: &mut [DispenseTarget<'_>]) {
        for (p, facing, slot, stack) in std::mem::take(&mut self.automation.deferred) {
            let slot = slot as usize;
            if !matches!(self.get_block(p).and_then(r::component), Some(r::Component::Dispenser { dropper: false, .. }))
                || self.container_stack(p, slot) != Some(stack)
            {
                continue;
            }
            let front = p + r::direction(facing);
            let one = Stack { count: 1, ..stack };
            let outcome = match behaviour(stack.item) {
                Behaviour::Equipment => {
                    if entities.equip_dispensed(front, one, players) {
                        Outcome::Consume
                    } else {
                        Outcome::Drop
                    }
                }
                // The block half already failed in `dispense`.
                Behaviour::WitherSkull | Behaviour::CarvedPumpkin => {
                    if entities.equip_dispensed(front, one, players) {
                        Outcome::Consume
                    } else {
                        Outcome::Keep
                    }
                }
                Behaviour::Shears => {
                    if self.get_block(front).and_then(super::bees::hive).is_some_and(|(_, _, honey)| honey == 5) {
                        self.drop_honeycomb(front);
                        self.harvest_dispensed_hive(entities, front);
                        Outcome::Wear(1)
                    } else if entities.shear_dispensed(front) {
                        Outcome::Wear(1)
                    } else {
                        Outcome::Keep
                    }
                }
                Behaviour::Brush => {
                    if entities.brush_dispensed(front) {
                        Outcome::Wear(16)
                    } else {
                        Outcome::Keep
                    }
                }
                Behaviour::ChestOnMount => {
                    if entities.chest_onto_mount(front) {
                        Outcome::Consume
                    } else {
                        Outcome::Drop
                    }
                }
                Behaviour::FlintAndSteel => {
                    if entities.ignite_dispensed(front, self.rules.tnt_explodes) || self.ignite_dispensed_block(front)
                    {
                        Outcome::Wear(1)
                    } else {
                        Outcome::Keep
                    }
                }
                Behaviour::GlassBottle => {
                    if self.harvest_dispensed_hive(entities, front) {
                        Outcome::Remainder(Item::HONEY_BOTTLE)
                    } else if self.get_block(front).is_some_and(|b| b.holds_water()) {
                        Outcome::Remainder(Item::potion(crate::potion::Potion::WATER))
                    } else {
                        Outcome::Drop
                    }
                }
                _ => Outcome::Drop,
            };
            match outcome {
                Outcome::Keep => {}
                Outcome::Consume => {
                    self.set_container_stack(p, slot, (stack.count > 1).then(|| Stack { count: stack.count - 1, ..stack }))
                }
                Outcome::Remainder(item) => self.consume_with_remainder(p, facing, slot, item),
                Outcome::Wear(amount) => self.wear_dispensed(p, slot, amount),
                Outcome::Drop => {
                    self.set_container_stack(p, slot, (stack.count > 1).then(|| Stack { count: stack.count - 1, ..stack }));
                    entities.dispense_item(one, p, r::direction(facing));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_registers_before_the_equippable_fallback() {
        // Carved pumpkins and wither skulls are equippable, but their own
        // bootstrap behaviours win.
        assert_eq!(behaviour(Item::from_block(Block::CARVED_PUMPKIN)), Behaviour::CarvedPumpkin);
        assert!(equippable(Item::from_block(Block::CARVED_PUMPKIN)).is_some());
        assert_eq!(behaviour(Item::SADDLE), Behaviour::Equipment);
        assert_eq!(behaviour(Item::from_block(Block::TNT)), Behaviour::Tnt);
        assert_eq!(behaviour(Item::potion(crate::potion::Potion::WATER)), Behaviour::WaterPotion);
        assert_eq!(behaviour(Item::STICK), Behaviour::Default);
        for b in Behaviour::ALL {
            assert_eq!(Behaviour::ALL.iter().filter(|o| o.name() == b.name()).count(), 1);
        }
    }
}
