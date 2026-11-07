//! v0.16 Track D: the Java 26.3 weapon families — the mace, the trident and
//! every spear tier — as item data and the arithmetic their attacks share.
//!
//! Sources (read with `javap`/Vineflower from the pinned 26.3 server jar):
//! `Items` (`MACE`, `TRIDENT`, the `*_SPEAR` registrations), `Item$Properties.spear`,
//! `ToolMaterial`, `MaceItem`, `TridentItem`, `ThrownTrident`, `AbstractArrow`,
//! `KineticWeapon`, `PiercingWeapon`, `AttackRange`, `HeavyCoreBlock`, and the
//! bundled `data/minecraft/enchantment/*.json`, `tags/item/enchantable/*.json`
//! and `recipe/*spear*.json`, `recipe/mace.json`. Nothing from the client's art
//! is used: icons, models and sounds are original.
//!
//! Item ids come from the track's reservation (1495–1529, see
//! `docs/architecture.md#v016-track-reservations`): 1495 trident, 1496 mace,
//! 1497–1503 the spears in Java's registration order. 1504–1529 stay reserved.
//! The older copper-equipment note reserving 1060–1069 for spears is unused.

pub mod mace;
pub mod spear;
#[cfg(test)]
mod tests;
pub mod trident;

use crate::item::{Item, ItemInfo, ItemKind, Sprite, Tier};

pub const ITEM_FIRST: u16 = 1495;
pub const TRIDENT: Item = Item(1495);
pub const MACE: Item = Item(1496);
const SPEAR_FIRST: u16 = 1497;
/// One past the last registered weapon item.
pub const ITEM_END: u16 = 1504;
/// The track's whole item reservation, registered or not.
pub const RESERVED_END: u16 = 1530;
const _: () = assert!(ITEM_FIRST >= 1495 && ITEM_END <= RESERVED_END);

/// Java's spear registration order (`Items.WOODEN_SPEAR` … `NETHERITE_SPEAR`).
pub const SPEAR_TIERS: [Tier; 7] =
    [Tier::Wood, Tier::Stone, Tier::Copper, Tier::Iron, Tier::Gold, Tier::Diamond, Tier::Netherite];

const NAMES: [&str; 9] = [
    "trident",
    "mace",
    "wooden spear",
    "stone spear",
    "copper spear",
    "iron spear",
    "golden spear",
    "diamond spear",
    "netherite spear",
];

/// The spear of `tier`.
pub const fn spear(tier: Tier) -> Item {
    let i = match tier {
        Tier::Wood => 0,
        Tier::Stone => 1,
        Tier::Copper => 2,
        Tier::Iron => 3,
        Tier::Gold => 4,
        Tier::Diamond => 5,
        Tier::Netherite => 6,
    };
    Item(SPEAR_FIRST + i)
}

/// The tier of a spear item.
pub fn spear_tier(item: Item) -> Option<Tier> {
    item.0.checked_sub(SPEAR_FIRST).and_then(|i| SPEAR_TIERS.get(i as usize)).copied()
}

pub fn is_spear(item: Item) -> bool {
    spear_tier(item).is_some()
}

/// Whether this id is one of the track's weapons.
pub const fn owns(item: Item) -> bool {
    item.0 >= ITEM_FIRST && item.0 < ITEM_END
}

/// Every registered weapon, in id order.
pub fn all() -> impl Iterator<Item = Item> {
    (ITEM_FIRST..ITEM_END).map(Item)
}

/// Item data: each weapon stacks to one (`Item$Properties.durability`).
pub fn item_info(item: Item) -> Option<ItemInfo> {
    if !owns(item) {
        return None;
    }
    let i = (item.0 - ITEM_FIRST) as usize;
    Some(ItemInfo { name: NAMES[i], kind: ItemKind::Material, max_stack: 1, sprite: Sprite::Weapon(i as u8) })
}

/// `ToolMaterial.durability`, the trident's 250 and the mace's 500.
pub fn durability(item: Item) -> Option<u16> {
    match item {
        TRIDENT => Some(250),
        MACE => Some(500),
        _ => spear_tier(item).map(Tier::durability),
    }
}

/// `ToolMaterial.attackDamageBonus`.
fn material_bonus(tier: Tier) -> f32 {
    match tier {
        Tier::Wood | Tier::Gold => 0.0,
        Tier::Stone | Tier::Copper => 1.0,
        Tier::Iron => 2.0,
        Tier::Diamond => 3.0,
        Tier::Netherite => 4.0,
    }
}

/// The attack-damage attribute with the weapon held: the player's base 1
/// plus `TridentItem`'s 8, `MaceItem`'s 5, or a spear's
/// `0 + material.attackDamageBonus()` (`Item$Properties.spear`).
pub fn attack_damage(item: Item) -> Option<f32> {
    match item {
        TRIDENT => Some(9.0),
        MACE => Some(6.0),
        _ => spear_tier(item).map(|t| 1.0 + material_bonus(t)),
    }
}

/// The attack-speed attribute: 4 plus the trident's -2.9, the mace's -3.4 or
/// a spear's `1 / attackDuration - 4`, each modifier a Java `float`
/// widened into the `double` attribute.
pub fn attack_speed(item: Item) -> Option<f64> {
    match item {
        TRIDENT => Some(4.0 + f64::from(-2.9f32)),
        MACE => Some(4.0 + f64::from(-3.4f32)),
        _ => spear_tier(item).map(|t| 4.0 + (f64::from(1.0f32 / spear::stats(t).attack_duration) - 4.0)),
    }
}

/// `Item$Properties.enchantable`: the trident's 1, the mace's 15, a spear's material value.
pub fn enchantability(item: Item) -> Option<u32> {
    match item {
        TRIDENT => Some(1),
        MACE => Some(15),
        _ => spear_tier(item).map(|t| match t {
            Tier::Wood | Tier::Netherite => 15,
            Tier::Stone => 5,
            Tier::Copper => 13,
            Tier::Iron => 14,
            Tier::Gold => 22,
            Tier::Diamond => 10,
        }),
    }
}

/// `Rarity`: the trident is rare (aqua), the mace and heavy core epic.
pub fn is_rare(item: Item) -> bool {
    item == TRIDENT
}

/// Java's `Rarity.EPIC` items of this track (the mace and the heavy core).
pub fn is_epic(item: Item) -> bool {
    item == MACE || item.block().is_some_and(|b| crate::world::heavy_core::owns(b.0))
}

/// Only the netherite spear survives fire and lava (`fireResistant()`).
pub fn fire_resistant(item: Item) -> bool {
    item == spear(Tier::Netherite)
}

/// The breeze rod, Track C's item, looked up by its Java name. `None` until
/// that track's registration is merged; the mace recipe and the mace's
/// anvil repair then appear without further changes.
pub fn breeze_rod() -> Option<Item> {
    Item::from_name("breeze_rod")
}

/// Anvil repair materials (`Item$Properties.repairable`): breeze rods for the
/// mace, a spear's tool material; the trident has none. `None` for other items.
pub fn repairs(item: Item, material: Item) -> Option<bool> {
    if item == TRIDENT {
        return Some(false);
    }
    if item == MACE {
        return Some(breeze_rod() == Some(material));
    }
    let tier = spear_tier(item)?;
    // The same `#*_tool_materials` tags as every other tool of the tier.
    Some(crate::enchant::repairs(Item::tool(crate::item::ToolKind::Sword, tier), material))
}

/// `DataComponents.MINIMUM_ATTACK_CHARGE`: spears refuse attacks until fully charged.
pub fn minimum_attack_charge(item: Item) -> Option<f32> {
    is_spear(item).then_some(1.0)
}

/// `Tool.canDestroyBlocksInCreative`: the trident's and mace's tool
/// components set it false. Spears have no tool component, but their attack
/// key jabs instead of mining (`Minecraft.startAttack`), so they never start
/// a break either.
pub fn can_destroy_in_creative(item: Item) -> Option<bool> {
    owns(item).then_some(false)
}

/// Durability lost per use: `Weapon(1)` on every hit; `Tool.damagePerBlock`
/// 2 for the trident and mace on a block with hardness, none for spears
/// (no tool component).
pub fn wear(item: Item, hitting_mob: bool) -> Option<u16> {
    if !owns(item) {
        return None;
    }
    Some(if hitting_mob {
        1
    } else if is_spear(item) {
        0
    } else {
        2
    })
}

/// Impaling's `minecraft:damage` bonus, 2.5 per level, against
/// `#sensitive_to_impaling` (= `#aquatic`): turtle, axolotl, guardian, elder
/// guardian, cod, pufferfish, salmon, tropical fish, dolphin, squid, glow
/// squid, tadpole (the nautiluses aren't in this game).
pub fn impaling_bonus(enchants: crate::enchant::Enchants, kind: crate::entity::MobKind) -> f32 {
    use crate::entity::MobKind as K;
    let level = enchants.level(crate::enchant::Enchantment::Impaling);
    let aquatic = matches!(
        kind,
        K::Turtle
            | K::Axolotl
            | K::Guardian
            | K::ElderGuardian
            | K::Cod
            | K::Pufferfish
            | K::Salmon
            | K::TropicalFish
            | K::Dolphin
            | K::Squid
            | K::GlowSquid
            | K::Tadpole
    );
    if aquatic { 2.5 * f32::from(level) } else { 0.0 }
}

/// The weapons' sound events (Java `SoundEvents.MACE_SMASH_*`, `TRIDENT_*`,
/// `SPEAR_*`, `ITEM_SPEAR_LUNGE_*`, `WIND_CHARGE_WIND_BURST`,
/// `DROWNED_SHOOT`), synthesized originally by the audio bank.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cue {
    SmashAir,
    SmashGround,
    SmashHeavy,
    WindBurst,
    TridentThrow,
    TridentHit,
    TridentHitGround,
    TridentReturn,
    Riptide1,
    Riptide2,
    Riptide3,
    TridentThunder,
    SpearUse,
    SpearWoodUse,
    SpearHit,
    SpearWoodHit,
    SpearAttack,
    SpearWoodAttack,
    Lunge1,
    Lunge2,
    Lunge3,
    DrownedShoot,
}

impl Cue {
    pub const ALL: [Cue; 22] = [
        Cue::SmashAir,
        Cue::SmashGround,
        Cue::SmashHeavy,
        Cue::WindBurst,
        Cue::TridentThrow,
        Cue::TridentHit,
        Cue::TridentHitGround,
        Cue::TridentReturn,
        Cue::Riptide1,
        Cue::Riptide2,
        Cue::Riptide3,
        Cue::TridentThunder,
        Cue::SpearUse,
        Cue::SpearWoodUse,
        Cue::SpearHit,
        Cue::SpearWoodHit,
        Cue::SpearAttack,
        Cue::SpearWoodAttack,
        Cue::Lunge1,
        Cue::Lunge2,
        Cue::Lunge3,
        Cue::DrownedShoot,
    ];

    /// Riptide's `trident_sound` for its level (the plain throw without it).
    pub fn riptide(level: u8) -> Self {
        match level {
            0 => Cue::TridentThrow,
            1 => Cue::Riptide1,
            2 => Cue::Riptide2,
            _ => Cue::Riptide3,
        }
    }

    /// Lunge's `play_sound` list, indexed by level.
    pub fn lunge(level: u8) -> Self {
        match level {
            0 | 1 => Cue::Lunge1,
            2 => Cue::Lunge2,
            _ => Cue::Lunge3,
        }
    }

    /// A spear's use, hit and jab sounds: the wooden spear has its own.
    pub fn spear(item: Item, use_: bool, hit: bool) -> Self {
        let wood = spear_tier(item) == Some(Tier::Wood);
        match (use_, hit, wood) {
            (true, _, true) => Cue::SpearWoodUse,
            (true, _, false) => Cue::SpearUse,
            (false, true, true) => Cue::SpearWoodHit,
            (false, true, false) => Cue::SpearHit,
            (false, false, true) => Cue::SpearWoodAttack,
            (false, false, false) => Cue::SpearAttack,
        }
    }

    /// The smash's sound (`MaceItem.hurtEnemy`).
    pub fn smash(sound: mace::SmashSound) -> Self {
        match sound {
            mace::SmashSound::Air => Cue::SmashAir,
            mace::SmashSound::Ground => Cue::SmashGround,
            mace::SmashSound::Heavy => Cue::SmashHeavy,
        }
    }
}
