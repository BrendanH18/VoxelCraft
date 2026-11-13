//! Java 26.3 `EntityEquipment` and `DropChances` for mobs: one item stack
//! per `EquipmentSlot`, a drop chance per slot, and `Mob.dropCustomDeathLoot`.
//!
//! Slot order, the default chance (`DEFAULT_EQUIPMENT_DROP_CHANCE` 0.085),
//! the preserve threshold (above 1.0) and the guaranteed chance (2.0) are
//! checked against the reference in `tests/java_mob_equipment.rs`.

use super::Rng;
use super::armor::{ArmorKind, Worn};
use crate::enchant::{Enchantment, Enchants};
use crate::inventory::{Stack, stack_from_str, stack_to_string};
use crate::item::{ArmorMaterial, ArmorPiece, Item};
use serde_json::{Map, Value, json};

/// Java's `EquipmentSlot`, in `EquipmentSlot.VALUES` order.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Slot {
    MainHand,
    OffHand,
    Feet,
    Legs,
    Chest,
    Head,
    Body,
    Saddle,
}

impl Slot {
    /// `EquipmentSlot.VALUES`: the order `dropCustomDeathLoot` visits.
    pub const ALL: [Slot; 8] =
        [Slot::MainHand, Slot::OffHand, Slot::Feet, Slot::Legs, Slot::Chest, Slot::Head, Slot::Body, Slot::Saddle];

    /// The serialized name (`getSerializedName`), used in saves.
    pub const fn name(self) -> &'static str {
        match self {
            Slot::MainHand => "mainhand",
            Slot::OffHand => "offhand",
            Slot::Feet => "feet",
            Slot::Legs => "legs",
            Slot::Chest => "chest",
            Slot::Head => "head",
            Slot::Body => "body",
            Slot::Saddle => "saddle",
        }
    }

    pub fn from_name(name: &str) -> Option<Slot> {
        Slot::ALL.into_iter().find(|s| s.name() == name)
    }

    /// `EquipmentSlot.isArmor`: humanoid and animal armor.
    pub const fn is_armor(self) -> bool {
        matches!(self, Slot::Feet | Slot::Legs | Slot::Chest | Slot::Head | Slot::Body)
    }

    /// `EquipmentSlot.countLimit`: hands hold whole stacks (0, no limit),
    /// every other slot one item.
    pub const fn count_limit(self) -> u8 {
        match self {
            Slot::MainHand | Slot::OffHand => 0,
            _ => 1,
        }
    }

    /// `EquipmentSlot.getIndex`: position within its type (hands 0–1,
    /// feet to head 0–3, body and saddle 0).
    pub const fn index(self) -> usize {
        match self {
            Slot::MainHand | Slot::Feet | Slot::Body | Slot::Saddle => 0,
            Slot::OffHand | Slot::Legs => 1,
            Slot::Chest => 2,
            Slot::Head => 3,
        }
    }

    /// `EquipmentSlot.canIncreaseExperience`: every slot but the saddle.
    pub const fn can_increase_experience(self) -> bool {
        !matches!(self, Slot::Saddle)
    }

    /// The humanoid armor piece this slot holds.
    pub const fn piece(self) -> Option<ArmorPiece> {
        match self {
            Slot::Head => Some(ArmorPiece::Helmet),
            Slot::Chest => Some(ArmorPiece::Chestplate),
            Slot::Legs => Some(ArmorPiece::Leggings),
            Slot::Feet => Some(ArmorPiece::Boots),
            _ => None,
        }
    }

    pub const fn of_piece(piece: ArmorPiece) -> Slot {
        match piece {
            ArmorPiece::Helmet => Slot::Head,
            ArmorPiece::Chestplate => Slot::Chest,
            ArmorPiece::Leggings => Slot::Legs,
            ArmorPiece::Boots => Slot::Feet,
        }
    }
}

/// `DropChances.DEFAULT_EQUIPMENT_DROP_CHANCE`.
pub const DEFAULT_DROP_CHANCE: f32 = 0.085;
/// `DropChances.PRESERVE_ITEM_DROP_CHANCE_THRESHOLD`: a chance above this
/// is "preserved" (drops whoever kills, undamaged).
pub const PRESERVE_THRESHOLD: f32 = 1.0;
/// `DropChances.PRESERVE_ITEM_DROP_CHANCE` (`withGuaranteedDrop`).
pub const GUARANTEED_DROP_CHANCE: f32 = 2.0;

/// `Mob.MAX_ENCHANTED_WEAPON_CHANCE` and `MAX_ENCHANTED_ARMOR_CHANCE`.
pub const MAX_ENCHANTED_WEAPON_CHANCE: f32 = 0.25;
pub const MAX_ENCHANTED_ARMOR_CHANCE: f32 = 0.5;

/// `enchantment/looting.json`'s `equipment_drops` effect: `add` a
/// `linear` value with base 0.01 and 0.01 per level above the first, when
/// the attacker is a player.
pub const LOOTING_BASE: f32 = 0.01;
pub const LOOTING_PER_LEVEL_ABOVE_FIRST: f32 = 0.01;

/// The drop chance after `EnchantmentHelper.processEquipmentDropChance`
/// for a player attacker with Looting `level` (0 for none).
pub fn looting_chance(chance: f32, looting: u8) -> f32 {
    if looting == 0 {
        return chance;
    }
    chance + (LOOTING_BASE + LOOTING_PER_LEVEL_ABOVE_FIRST * f32::from(looting - 1))
}

/// `Mob.dropCustomDeathLoot`'s wear for a non-preserved damageable drop:
/// `maxDamage - nextInt(1 + nextInt(max(maxDamage - 3, 1)))`.
pub fn reroll_wear(max: u16, rng: &mut Rng) -> u16 {
    let inner = rng.next_int(u32::from(max.saturating_sub(3).max(1)));
    max - rng.next_int(1 + inner) as u16
}

/// A mob's equipment: a stack per slot and the slot's drop chance.
#[derive(Clone, PartialEq, Debug)]
pub struct Equipment {
    stacks: [Option<Box<Stack>>; 8],
    chances: [f32; 8],
    /// Chainmail worn without an item until the chainmail armor items are
    /// registered (item ids 1550–1553, owned by another v0.14 slice): bit
    /// `ArmorPiece as u8` is worn, bit `4 + piece` its enchantment glint.
    /// Drawn like any armor; drops nothing.
    chainmail: u8,
}

impl Default for Equipment {
    fn default() -> Self {
        Self { stacks: Default::default(), chances: [DEFAULT_DROP_CHANCE; 8], chainmail: 0 }
    }
}

impl Equipment {
    pub fn get(&self, slot: Slot) -> Option<&Stack> {
        self.stacks[slot as usize].as_deref()
    }

    pub fn get_mut(&mut self, slot: Slot) -> Option<&mut Stack> {
        self.stacks[slot as usize].as_deref_mut()
    }

    /// `setItemSlot`, with the slot's count limit (`EquipmentSlot.limit`).
    pub fn set(&mut self, slot: Slot, stack: Option<Stack>) {
        let stack = stack.filter(|s| s.count > 0).map(|mut s| {
            if slot.count_limit() > 0 {
                s.count = s.count.min(slot.count_limit());
            }
            Box::new(s)
        });
        if let Some(piece) = slot.piece() {
            self.chainmail &= !(0x11 << piece as u8);
        }
        self.stacks[slot as usize] = stack;
    }

    pub fn take(&mut self, slot: Slot) -> Option<Stack> {
        self.stacks[slot as usize].take().map(|s| *s)
    }

    pub fn main_hand(&self) -> Option<&Stack> {
        self.get(Slot::MainHand)
    }

    pub fn is_empty(&self) -> bool {
        self.stacks.iter().all(Option::is_none) && self.chainmail == 0
    }

    /// `DropChances.byEquipment`.
    pub fn drop_chance(&self, slot: Slot) -> f32 {
        self.chances[slot as usize]
    }

    /// `Mob.setDropChance` (Java refuses negative chances).
    pub fn set_drop_chance(&mut self, slot: Slot, chance: f32) {
        self.chances[slot as usize] = if chance.is_finite() { chance.max(0.0) } else { 0.0 };
    }

    /// `Mob.setGuaranteedDrop`.
    pub fn set_guaranteed_drop(&mut self, slot: Slot) {
        self.set_drop_chance(slot, GUARANTEED_DROP_CHANCE);
    }

    /// `DropChances.isPreserved`.
    pub fn is_preserved(&self, slot: Slot) -> bool {
        self.drop_chance(slot) > PRESERVE_THRESHOLD
    }

    /// Puts a `kind` armor piece in its slot, enchanted with `enchants`.
    /// Chainmail (no item yet) is recorded for drawing only; a material
    /// without that piece (turtle below the head) is skipped.
    pub fn wear(&mut self, piece: ArmorPiece, kind: ArmorKind, enchants: Enchants) {
        let material = match kind {
            ArmorKind::Leather => ArmorMaterial::Leather,
            ArmorKind::Iron => ArmorMaterial::Iron,
            ArmorKind::Gold => ArmorMaterial::Gold,
            ArmorKind::Diamond => ArmorMaterial::Diamond,
            ArmorKind::Netherite => ArmorMaterial::Netherite,
            ArmorKind::Copper => ArmorMaterial::Copper,
            ArmorKind::Turtle if piece == ArmorPiece::Helmet => ArmorMaterial::Turtle,
            ArmorKind::Turtle => return,
            ArmorKind::Chain => return self.wear_chainmail(piece, !enchants.is_empty()),
        };
        let mut stack = Stack::new(Item::armor(piece, material), 1);
        stack.enchants = enchants;
        self.set(Slot::of_piece(piece), Some(stack));
    }

    /// A whole `kind` set, as `--spawn` armor and scenes ask for (`glint`
    /// enchants each piece with Protection I).
    pub fn wear_set(&mut self, kind: ArmorKind, glint: bool) {
        let enchants = if glint { Enchants::NONE.with(Enchantment::Protection, 1) } else { Enchants::NONE };
        for piece in ArmorPiece::ALL {
            self.wear(piece, kind, enchants);
        }
    }

    /// A chainmail piece (drawn only; see [`Equipment`]), `glint` if enchanted.
    pub fn wear_chainmail(&mut self, piece: ArmorPiece, glint: bool) {
        self.set(Slot::of_piece(piece), None);
        self.chainmail |= (1 | (u8::from(glint) << 4)) << (piece as u8);
    }

    /// The armor material worn on `piece`, if any.
    pub fn armor_kind(&self, piece: ArmorPiece) -> Option<ArmorKind> {
        self.worn()[piece as usize].map(|w| w.kind)
    }

    /// What `HumanoidArmorLayer` draws, indexed by [`ArmorPiece`].
    pub fn worn(&self) -> [Option<Worn>; 4] {
        std::array::from_fn(|i| {
            let piece = ArmorPiece::ALL[i];
            if self.chainmail & (1 << i) != 0 {
                return Some(Worn { kind: ArmorKind::Chain, glint: self.chainmail & (0x10 << i) != 0, dye: None });
            }
            let stack = self.get(Slot::of_piece(piece))?;
            let (worn_piece, _) = stack.item.as_armor()?;
            if worn_piece == piece { Worn::of(stack) } else { None }
        })
    }

    /// `Mob.populateDefaultEquipmentEnchantments`: the main hand with chance
    /// `0.25 * special` (`MAX_ENCHANTED_WEAPON_CHANCE`), then each worn
    /// humanoid armor piece in `EquipmentSlot.VALUES` order (feet to head)
    /// with `0.5 * special` (`MAX_ENCHANTED_ARMOR_CHANCE`), each enchanted
    /// by the `mob_spawn_equipment` provider. Chainmail, which has no item
    /// yet, only gains its glint.
    pub fn enchant_spawned(&mut self, rng: &mut Rng, special: f32) {
        let enchant = |stack: &mut Stack, rng: &mut Rng| {
            let mut random = crate::enchant::JavaRandom::new(i64::from(rng.next_int(i32::MAX as u32)));
            stack.enchants = crate::enchant::enchant_mob_equipment(&mut random, stack.item, special);
        };
        if let Some(stack) = self.get_mut(Slot::MainHand)
            && rng.chance(MAX_ENCHANTED_WEAPON_CHANCE * special)
        {
            enchant(stack, rng);
        }
        for slot in [Slot::Feet, Slot::Legs, Slot::Chest, Slot::Head] {
            let piece = slot.piece().unwrap();
            if let Some(stack) = self.get_mut(slot) {
                if rng.chance(MAX_ENCHANTED_ARMOR_CHANCE * special) {
                    enchant(stack, rng);
                }
            } else if self.chainmail & (1 << piece as u8) != 0 && rng.chance(MAX_ENCHANTED_ARMOR_CHANCE * special) {
                self.chainmail |= 0x10 << piece as u8;
            }
        }
    }

    /// `ConversionType.SINGLE` with `keepEquipment`: every non-empty slot
    /// moves over with its drop chance.
    pub fn carry_from(&mut self, from: &mut Equipment) {
        for slot in Slot::ALL {
            if let Some(stack) = from.take(slot) {
                self.set(slot, Some(stack));
                self.set_drop_chance(slot, from.drop_chance(slot));
            }
        }
        for piece in ArmorPiece::ALL {
            let bits = from.chainmail & (0x11 << piece as u8);
            if bits != 0 {
                self.set(Slot::of_piece(piece), None);
                self.chainmail |= bits;
                self.set_drop_chance(Slot::of_piece(piece), from.drop_chance(Slot::of_piece(piece)));
            }
        }
        from.chainmail = 0;
    }

    /// `Mob.dropCustomDeathLoot`: the stacks that drop, their slots emptied.
    /// A zero chance never drops; otherwise Looting raises the chance, and
    /// an item without Curse of Vanishing (`prevent_equipment_drop`) drops
    /// on a player kill, or any kill when preserved, if `nextFloat() <
    /// chance`. A non-preserved damageable drop has its wear rerolled.
    pub fn death_drops(&mut self, player_kill: bool, looting: u8, rng: &mut Rng) -> Vec<Stack> {
        let mut out = Vec::new();
        for slot in Slot::ALL {
            let chance = self.drop_chance(slot);
            if chance == 0.0 {
                continue;
            }
            let preserve = self.is_preserved(slot);
            let chance = looting_chance(chance, looting);
            let Some(stack) = self.get(slot).copied() else { continue };
            if stack.enchants.has(Enchantment::VanishingCurse) || !(player_kill || preserve) || rng.next_f32() >= chance
            {
                continue;
            }
            let mut stack = stack;
            if !preserve && let Some(max) = stack.item.durability() {
                stack.damage = reroll_wear(max, rng);
            }
            out.push(stack);
            self.set(slot, None);
        }
        out
    }

    /// `Mob.getBaseExperienceReward`'s addition for a mob with experience:
    /// `1 + nextInt(3)` for each filled slot that can increase experience
    /// (all but the saddle) and is not preserved.
    pub fn experience_bonus(&self, rng: &mut Rng) -> u32 {
        let mut bonus = 0;
        for slot in Slot::ALL {
            let chainmail = slot.piece().is_some_and(|p| self.chainmail & (1 << p as u8) != 0);
            if slot.can_increase_experience()
                && (self.get(slot).is_some() || chainmail)
                && self.drop_chance(slot) <= PRESERVE_THRESHOLD
            {
                bonus += 1 + rng.next_int(3);
            }
        }
        bonus
    }

    /// `Mob.dropPreservedEquipment`: preserved stacks leave their slots.
    pub fn take_preserved(&mut self) -> Vec<Stack> {
        let preserved: Vec<Slot> = Slot::ALL.into_iter().filter(|&s| self.is_preserved(s)).collect();
        preserved.into_iter().filter_map(|s| self.take(s)).collect()
    }

    /// The `equipment` save value: slot name to stack, chainmail pieces as
    /// `"chainmail"` (`"chainmail*"` enchanted).
    pub fn save(&self) -> Value {
        let mut out = Map::new();
        for slot in Slot::ALL {
            if let Some(stack) = self.get(slot) {
                out.insert(slot.name().into(), json!(stack_to_string(Some(*stack))));
            }
        }
        for piece in ArmorPiece::ALL {
            if self.chainmail & (1 << piece as u8) != 0 {
                let glint = self.chainmail & (0x10 << piece as u8) != 0;
                out.insert(Slot::of_piece(piece).name().into(), json!(if glint { "chainmail*" } else { "chainmail" }));
            }
        }
        Value::Object(out)
    }

    /// The `drop_chances` save value, `None` when every chance is the
    /// default (Java's `DropChances.CODEC` writes only the others).
    pub fn save_drop_chances(&self) -> Option<Value> {
        let mut out = Map::new();
        for slot in Slot::ALL {
            let chance = self.drop_chance(slot);
            if chance != DEFAULT_DROP_CHANCE {
                out.insert(slot.name().into(), json!(chance));
            }
        }
        (!out.is_empty()).then_some(Value::Object(out))
    }

    /// Reads [`Equipment::save`] and [`Equipment::save_drop_chances`].
    pub fn load(equipment: &Value, chances: &Value) -> Equipment {
        let mut out = Equipment::default();
        for (name, value) in equipment.as_object().into_iter().flatten() {
            let Some(slot) = Slot::from_name(name) else { continue };
            match (value.as_str(), slot.piece()) {
                (Some(text @ ("chainmail" | "chainmail*")), Some(piece)) => {
                    out.wear_chainmail(piece, text.ends_with('*'));
                }
                (Some(text), _) => out.set(slot, stack_from_str(text).flatten()),
                _ => {}
            }
        }
        for (name, value) in chances.as_object().into_iter().flatten() {
            if let (Some(slot), Some(chance)) = (Slot::from_name(name), value.as_f64()) {
                out.set_drop_chance(slot, chance as f32);
            }
        }
        out
    }

    /// Writes `equipment` and `drop_chances` into a mob record.
    /// `always` writes an empty `equipment` too, for mobs whose fresh
    /// state is not empty (bow skeletons), so an emptied hand stays empty.
    pub fn save_into(&self, v: &mut Value, always: bool) {
        if always || !self.is_empty() {
            v["equipment"] = self.save();
        }
        if let Some(chances) = self.save_drop_chances() {
            v["drop_chances"] = chances;
        }
    }
}

/// The equipment keys of a mob record saved before v0.14.
#[derive(Clone, Debug, Default)]
pub struct Legacy {
    /// Helmet, chestplate, leggings, boots.
    pub armor: [Option<ArmorKind>; 4],
    /// Bit per armor piece: the piece was enchanted (which enchantments was
    /// never saved).
    pub glint: u8,
    /// A bow skeleton's `skeleton_bow` or a drowned's `main_hand`.
    pub main_hand: Option<Stack>,
    /// `keeps_equipment`: a trial spawner's table set every drop chance to 0.
    pub keeps_equipment: bool,
    /// Seeds the stand-in enchantments of glinted pieces.
    pub seed: i64,
}

impl Legacy {
    /// A `nether_mobs` record: `armor` (gold when true), `armor_materials`,
    /// `armor_glint`, `skeleton_bow`, `main_hand` and `keeps_equipment`.
    pub fn nether(v: &Value, kind: super::MobKind) -> Legacy {
        let mut armor = [None; 4];
        for (slot, worn) in armor.iter_mut().zip(v["armor"].as_array().into_iter().flatten()) {
            *slot = worn.as_bool().unwrap_or(false).then_some(ArmorKind::Gold);
        }
        for (slot, material) in armor.iter_mut().zip(v["armor_materials"].as_array().into_iter().flatten()) {
            *slot = material.as_u64().and_then(|i| ArmorKind::ALL.get(i as usize).copied());
        }
        let hand = |key: &str| v[key].as_str().and_then(|t| stack_from_str(t).flatten());
        let main_hand = if kind.is_bow_skeleton() {
            hand("skeleton_bow").filter(|s| s.item == Item::BOW)
        } else if kind == super::MobKind::Drowned {
            hand("main_hand")
        } else {
            None
        };
        Legacy {
            armor,
            glint: v["armor_glint"].as_u64().unwrap_or(0).min(15) as u8,
            main_hand,
            keeps_equipment: v["keeps_equipment"] == true,
            seed: v["uid"].as_i64().unwrap_or(0),
        }
    }

    /// A `villagers` record (zombie villagers): `armor` materials and `glint`.
    pub fn villager(v: &Value) -> Legacy {
        let mut armor = [None; 4];
        for (slot, value) in armor.iter_mut().zip(v["armor"].as_array().into_iter().flatten()) {
            *slot = value.as_u64().and_then(|i| ArmorKind::ALL.get(i as usize).copied());
        }
        Legacy {
            armor,
            glint: v["glint"].as_u64().unwrap_or(0).min(15) as u8,
            seed: v["id"].as_i64().unwrap_or(0),
            ..Legacy::default()
        }
    }

    /// The same items and drop rule in the new slots, on top of the mob's
    /// fresh equipment (a bow skeleton's default bow). A trial mob's glinted
    /// piece gets its table's Protection, Projectile Protection and Fire
    /// Protection IV; any other glinted piece a `mob_spawn_equipment` roll
    /// at the lowest cost, seeded by the mob, since the old save never
    /// recorded which enchantments it had.
    pub fn apply(&self, mut equipment: Equipment) -> Equipment {
        if let Some(stack) = self.main_hand {
            equipment.set(Slot::MainHand, Some(stack));
        }
        for (i, piece) in ArmorPiece::ALL.into_iter().enumerate() {
            let Some(kind) = self.armor[i] else { continue };
            let glint = self.glint & (1 << i) != 0;
            equipment.wear(piece, kind, Enchants::NONE);
            if !glint {
                continue;
            }
            let slot = Slot::of_piece(piece);
            match equipment.get_mut(slot) {
                Some(stack) if self.keeps_equipment => stack.enchants = trial_armor_enchants(),
                Some(stack) => {
                    let mut random = crate::enchant::JavaRandom::new(self.seed.wrapping_mul(4).wrapping_add(i as i64));
                    stack.enchants = crate::enchant::enchant_mob_equipment(&mut random, stack.item, 0.0);
                }
                None => equipment.wear_chainmail(piece, true),
            }
        }
        if self.keeps_equipment {
            for slot in Slot::ALL {
                equipment.set_drop_chance(slot, 0.0);
            }
        }
        equipment
    }
}

/// `loot_table/equipment/trial_chamber`'s `set_enchantments` on every armor
/// piece: Protection, Projectile Protection and Fire Protection, each IV.
pub fn trial_armor_enchants() -> Enchants {
    Enchants::NONE
        .with(Enchantment::Protection, 4)
        .with(Enchantment::ProjectileProtection, 4)
        .with(Enchantment::FireProtection, 4)
}

/// A mob record's equipment: the v0.14 `equipment` and `drop_chances`
/// keys when either is present, otherwise `legacy` applied to `fresh`.
pub fn load_mob_equipment(v: &Value, fresh: Equipment, legacy: &Legacy) -> Equipment {
    if v.get("equipment").is_some() || v.get("drop_chances").is_some() {
        Equipment::load(&v["equipment"], &v["drop_chances"])
    } else {
        legacy.apply(fresh)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_limit_counts_and_names_round_trip() {
        let mut e = Equipment::default();
        e.set(Slot::MainHand, Some(Stack::new(Item::ARROW, 16)));
        e.set(Slot::Head, Some(Stack::new(Item::armor(ArmorPiece::Helmet, ArmorMaterial::Iron), 3)));
        assert_eq!(e.get(Slot::MainHand).unwrap().count, 16, "hands hold whole stacks");
        assert_eq!(e.get(Slot::Head).unwrap().count, 1, "armor slots hold one");
        for slot in Slot::ALL {
            assert_eq!(Slot::from_name(slot.name()), Some(slot));
        }
        e.wear(ArmorPiece::Boots, ArmorKind::Chain, Enchants::NONE.with(Enchantment::Protection, 1));
        e.set_drop_chance(Slot::Head, 0.0);
        e.set_guaranteed_drop(Slot::MainHand);
        let back = Equipment::load(&e.save(), &e.save_drop_chances().unwrap());
        assert_eq!(back, e);
        assert_eq!(
            back.worn()[ArmorPiece::Boots as usize],
            Some(Worn { kind: ArmorKind::Chain, glint: true, dye: None })
        );
        assert_eq!(Equipment::default().save_drop_chances(), None);
    }

    #[test]
    fn preserved_slots_drop_on_any_kill_undamaged_and_zero_never_drops() {
        let sword = Stack::new(Item::tool(crate::item::ToolKind::Sword, crate::item::Tier::Diamond), 1);
        for seed in 0..50 {
            let mut e = Equipment::default();
            e.set(Slot::MainHand, Some(sword));
            e.set_guaranteed_drop(Slot::MainHand);
            e.set(Slot::OffHand, Some(sword));
            e.set_drop_chance(Slot::OffHand, 0.0);
            let drops = e.death_drops(false, 3, &mut Rng::new(seed));
            assert_eq!(drops, vec![sword]);
            assert!(e.get(Slot::MainHand).is_none() && e.get(Slot::OffHand).is_some());
        }
    }
}
