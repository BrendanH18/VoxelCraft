//! Java 26.3 armor items the earlier releases lacked: the four chainmail
//! pieces (`ArmorMaterials.CHAINMAIL`, worn like any humanoid armor), copper
//! and netherite horse armor, and the five nautilus armors, together with the
//! body-armor rules every horse and nautilus armor shares.
//!
//! Sources (read with `javap`/Vineflower from the pinned 26.3 server jar):
//! `ArmorMaterials`, `ArmorMaterial.createAttributes`, `ArmorType`,
//! `Item$Properties.humanoidArmor/horseArmor/nautilusArmor`, `Items`
//! (`CHAINMAIL_*`, `*_HORSE_ARMOR`, `*_NAUTILUS_ARMOR`), `SkeletonHorse`,
//! `LivingEntity.isEquippableInSlot` and `tags/entity_type/can_wear_*_armor.json`.
//! The facts are pinned in `tests/fixtures/java/26.3/equipment.json`;
//! icons and the worn looks are original.
//!
//! Item ids come from the v0.17 build-5000 reservations
//! (`docs/architecture.md`): 1550–1553 chainmail helmet, chestplate,
//! leggings and boots; 1561 copper and 1562 netherite horse armor; 1563–1567
//! copper, iron, golden, diamond and netherite nautilus armor (Java's
//! material order, not its registration order).

use crate::item::{ArmorMaterial, ArmorPiece, Item, ItemInfo, ItemKind, Sprite};

const CHAINMAIL_FIRST: u16 = 1550;
pub const COPPER_HORSE_ARMOR: Item = Item(1561);
pub const NETHERITE_HORSE_ARMOR: Item = Item(1562);
const NAUTILUS_FIRST: u16 = 1563;
pub const COPPER_NAUTILUS_ARMOR: Item = Item(NAUTILUS_FIRST);
pub const IRON_NAUTILUS_ARMOR: Item = Item(NAUTILUS_FIRST + 1);
pub const GOLDEN_NAUTILUS_ARMOR: Item = Item(NAUTILUS_FIRST + 2);
pub const DIAMOND_NAUTILUS_ARMOR: Item = Item(NAUTILUS_FIRST + 3);
pub const NETHERITE_NAUTILUS_ARMOR: Item = Item(NAUTILUS_FIRST + 4);
/// The nautilus armors in id order.
pub const NAUTILUS_MATERIALS: [ArmorMaterial; 5] =
    [ArmorMaterial::Copper, ArmorMaterial::Iron, ArmorMaterial::Gold, ArmorMaterial::Diamond, ArmorMaterial::Netherite];

/// What a body armor item fits (`Equippable.allowedEntities`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BodyArmor {
    /// `#can_wear_horse_armor`: horses and zombie horses.
    Horse,
    /// `#can_wear_nautilus_armor`: nautiluses and zombie nautiluses, which
    /// the game does not have yet.
    Nautilus,
}

/// The chainmail piece's item.
pub const fn chainmail(piece: ArmorPiece) -> Item {
    Item(CHAINMAIL_FIRST + piece as u16)
}

/// The nautilus armor of `material`, if Java has one.
pub fn nautilus_armor(material: ArmorMaterial) -> Option<Item> {
    NAUTILUS_MATERIALS.iter().position(|&m| m == material).map(|i| Item(NAUTILUS_FIRST + i as u16))
}

/// The horse armor of `material`, if Java has one.
pub fn horse_armor(material: ArmorMaterial) -> Option<Item> {
    Some(match material {
        ArmorMaterial::Leather => Item::LEATHER_HORSE_ARMOR,
        ArmorMaterial::Copper => COPPER_HORSE_ARMOR,
        ArmorMaterial::Iron => Item::IRON_HORSE_ARMOR,
        ArmorMaterial::Gold => Item::GOLDEN_HORSE_ARMOR,
        ArmorMaterial::Diamond => Item::DIAMOND_HORSE_ARMOR,
        ArmorMaterial::Netherite => NETHERITE_HORSE_ARMOR,
        ArmorMaterial::Turtle | ArmorMaterial::Chainmail => return None,
    })
}

/// Every item this module registers, in icon order: chainmail, then the
/// horse armors, then the nautilus armors.
pub fn all() -> impl Iterator<Item = Item> {
    (CHAINMAIL_FIRST..CHAINMAIL_FIRST + 4).chain(1561..=1562).chain(NAUTILUS_FIRST..NAUTILUS_FIRST + 5).map(Item)
}

/// How many item icons [`all`] adds.
pub const ICON_COUNT: u16 = 4 + 2 + 5;

/// This item's place among [`all`]'s icons.
pub fn icon_offset(item: Item) -> Option<u16> {
    match item.0 {
        i @ 1550..=1553 => Some(i - CHAINMAIL_FIRST),
        i @ 1561..=1562 => Some(4 + i - 1561),
        i @ 1563..=1567 => Some(6 + i - NAUTILUS_FIRST),
        _ => None,
    }
}

/// The horse or nautilus armor `item` is, and its material.
pub fn body_armor(item: Item) -> Option<(BodyArmor, ArmorMaterial)> {
    Some(match item {
        Item::LEATHER_HORSE_ARMOR => (BodyArmor::Horse, ArmorMaterial::Leather),
        Item::IRON_HORSE_ARMOR => (BodyArmor::Horse, ArmorMaterial::Iron),
        Item::GOLDEN_HORSE_ARMOR => (BodyArmor::Horse, ArmorMaterial::Gold),
        Item::DIAMOND_HORSE_ARMOR => (BodyArmor::Horse, ArmorMaterial::Diamond),
        COPPER_HORSE_ARMOR => (BodyArmor::Horse, ArmorMaterial::Copper),
        NETHERITE_HORSE_ARMOR => (BodyArmor::Horse, ArmorMaterial::Netherite),
        Item(i @ 1563..=1567) => (BodyArmor::Nautilus, NAUTILUS_MATERIALS[(i - NAUTILUS_FIRST) as usize]),
        _ => return None,
    })
}

/// `Items` registers the netherite horse and nautilus armors `fireResistant()`.
pub fn fire_resistant(item: Item) -> bool {
    body_armor(item).is_some_and(|(_, m)| m == ArmorMaterial::Netherite)
}

/// `Rarity.UNCOMMON` among this module's items: the chainmail pieces.
pub fn is_uncommon(item: Item) -> bool {
    (CHAINMAIL_FIRST..CHAINMAIL_FIRST + 4).contains(&item.0)
}

pub fn item_info(item: Item) -> Option<ItemInfo> {
    const CHAINMAIL: [&str; 4] = ["chainmail helmet", "chainmail chestplate", "chainmail leggings", "chainmail boots"];
    const NAUTILUS: [&str; 5] = [
        "copper nautilus armor",
        "iron nautilus armor",
        "golden nautilus armor",
        "diamond nautilus armor",
        "netherite nautilus armor",
    ];
    let body = |name, sprite| ItemInfo { name, kind: ItemKind::Material, max_stack: 1, sprite };
    Some(match item.0 {
        i @ 1550..=1553 => {
            let piece = ArmorPiece::ALL[(i - CHAINMAIL_FIRST) as usize];
            ItemInfo {
                name: CHAINMAIL[piece as usize],
                kind: ItemKind::Armor(piece, ArmorMaterial::Chainmail),
                max_stack: 1,
                sprite: Sprite::Armor(piece, ArmorMaterial::Chainmail),
            }
        }
        1561 => body("copper horse armor", Sprite::HorseArmor(ArmorMaterial::Copper)),
        1562 => body("netherite horse armor", Sprite::HorseArmor(ArmorMaterial::Netherite)),
        i @ 1563..=1567 => {
            let k = (i - NAUTILUS_FIRST) as usize;
            body(NAUTILUS[k], Sprite::NautilusArmor(NAUTILUS_MATERIALS[k]))
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{Entities, Mob, MobKind, PlayerId, Rng};
    use crate::inventory::{Inventory, Stack};
    use glam::DVec3;

    fn fixture() -> serde_json::Value {
        serde_json::from_str(include_str!("../tests/fixtures/java/26.3/equipment.json")).unwrap()
    }

    fn material(name: &str) -> ArmorMaterial {
        match name {
            "leather" => ArmorMaterial::Leather,
            "copper" => ArmorMaterial::Copper,
            "iron" => ArmorMaterial::Iron,
            "gold" => ArmorMaterial::Gold,
            "diamond" => ArmorMaterial::Diamond,
            "netherite" => ArmorMaterial::Netherite,
            other => panic!("no horse armor material {other}"),
        }
    }

    fn tame_horse(kind: MobKind, armor: Option<Item>) -> Mob {
        let mut m = Mob::new(kind, DVec3::new(5., 140., 5.), 0.);
        let s = m.mount.as_mut().unwrap();
        s.tame = true;
        s.slots[1] = armor.map(|i| Stack::new(i, 1));
        m
    }

    /// `ArmorMaterial.createAttributes(BODY)` on a horse: armor and toughness
    /// in `CombatRules.getDamageAfterAbsorb`, and netherite's knockback
    /// resistance in `LivingEntity.knockback`. Horse armor never wears
    /// (`setDamageOnHurt(false)`).
    #[test]
    fn horse_armor_body_attributes_protect_the_wearing_horse() {
        let f = fixture();
        let mut checked = 0;
        for (id, facts) in f["items"].as_object().unwrap().iter().filter(|(_, v)| v["form"] == "horse") {
            let item = Item::from_name(id).unwrap();
            let m = &f["materials"][facts["material"].as_str().unwrap()];
            let (armor, toughness) = (m["defense"]["body"].as_u64().unwrap() as u32, m["toughness"].as_f64().unwrap());
            let kb = m["knockback_resistance"].as_f64().unwrap() as f32;
            assert_eq!(body_armor(item), Some((BodyArmor::Horse, material(facts["material"].as_str().unwrap()))));
            let mut horse = tame_horse(MobKind::Horse, Some(item));
            let before = horse.health;
            horse.damage(10.0, Some(DVec3::new(1.0, 0.4, 0.0)), &mut Rng::new(3));
            let expected = crate::simulation::survival::armor_reduce(10.0, armor, toughness as f32);
            assert!((before - horse.health - expected).abs() < 1e-5, "{id}: took {}", before - horse.health);
            assert!((horse.vel.x - (1.0 - f64::from(kb))).abs() < 1e-9, "{id}: knockback {}", horse.vel.x);
            assert_eq!(horse.mount.as_ref().unwrap().slots[1].unwrap().damage, 0, "{id} wears");
            checked += 1;
        }
        assert_eq!(checked, 6);
        let mut bare = tame_horse(MobKind::Horse, None);
        bare.damage(10.0, Some(DVec3::X), &mut Rng::new(3));
        assert_eq!(bare.vel.x, 1.0);
    }

    /// `Equippable.allowedEntities` is `#can_wear_horse_armor` (horse, zombie
    /// horse); `SkeletonHorse.canUseSlot` alone does not let armor on.
    #[test]
    fn horse_armor_fits_horses_and_zombie_horses_only() {
        let f = fixture();
        let tag: Vec<MobKind> = f["entity_tags"]["can_wear_horse_armor"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| MobKind::from_name(n.as_str().unwrap()).unwrap())
            .collect();
        for kind in MobKind::ALL.into_iter().filter(|&k| crate::entity::mounts::rideable(k)) {
            let mut s = crate::entity::mounts::State::new(kind, &mut Rng::new(1));
            s.tame = true;
            for item in [COPPER_HORSE_ARMOR, NETHERITE_HORSE_ARMOR, Item::DIAMOND_HORSE_ARMOR] {
                assert_eq!(s.accepts(kind, 1, item), tag.contains(&kind), "{kind:?} {}", item.name());
            }
            // No animal the game has wears nautilus armor (`#can_wear_nautilus_armor`
            // names the nautilus and zombie nautilus).
            for item in [COPPER_NAUTILUS_ARMOR, NETHERITE_NAUTILUS_ARMOR] {
                assert!(!s.accepts(kind, 1, item), "{kind:?} {}", item.name());
            }
        }
        let nautilus = f["entity_tags"]["can_wear_nautilus_armor"].as_array().unwrap();
        assert!(nautilus.iter().all(|n| MobKind::from_name(n.as_str().unwrap()).is_none()));
    }

    /// A tamed horse takes copper and netherite armor from the player's hand
    /// (`AbstractHorse.mobInteract`) and keeps it across a save.
    #[test]
    fn new_horse_armor_equips_from_the_hand_and_survives_a_save() {
        for armor in [COPPER_HORSE_ARMOR, NETHERITE_HORSE_ARMOR] {
            let mut e = Entities::new(5);
            e.spawn(MobKind::Horse, DVec3::new(5., 140., 5.));
            e.mobs[0].mount.as_mut().unwrap().tame = true;
            let mut inv = Inventory::default();
            inv.slots[0] = Some(Stack::new(armor, 1));
            assert!(e.use_mount(DVec3::new(5., 140.5, 3.), DVec3::Z, 5., PlayerId::HOST, &mut inv, 0, false));
            assert_eq!(inv.get(0), None);
            assert_eq!(e.mobs[0].mount.as_ref().unwrap().slots[1], Some(Stack::new(armor, 1)));
            let saved = e.nether_mobs_to_string();
            let mut restored = Entities::new(5);
            restored.load_nether_mobs(&saved);
            assert_eq!(restored.mobs[0].mount.as_ref().unwrap().slots[1], Some(Stack::new(armor, 1)));
            assert!(restored.items.is_empty());
        }
    }

    /// Older builds let skeleton horses wear horse armor. Such a save still
    /// loads its horse; the armor drops beside it.
    #[test]
    fn skeleton_horse_armor_from_an_older_save_drops_beside_it() {
        let mut e = Entities::new(5);
        e.spawn(MobKind::SkeletonHorse, DVec3::new(5., 140., 5.));
        let s = e.mobs[0].mount.as_mut().unwrap();
        s.tame = true;
        assert!(!s.accepts(MobKind::SkeletonHorse, 1, Item::IRON_HORSE_ARMOR));
        s.slots[1] = Some(Stack::new(Item::IRON_HORSE_ARMOR, 1));
        let saved = e.nether_mobs_to_string();
        let mut restored = Entities::new(5);
        restored.load_nether_mobs(&saved);
        assert_eq!(restored.mobs.len(), 1, "the horse still loads");
        assert_eq!(restored.mobs[0].kind, MobKind::SkeletonHorse);
        assert_eq!(restored.mobs[0].mount.as_ref().unwrap().slots[1], None);
        let dropped: Vec<_> = restored.items.iter().map(|i| i.stack).collect();
        assert_eq!(dropped, [Stack::new(Item::IRON_HORSE_ARMOR, 1)]);
        // A tame skeleton horse refuses armor from the hand.
        let mut inv = Inventory::default();
        inv.slots[0] = Some(Stack::new(COPPER_HORSE_ARMOR, 1));
        restored.use_mount(DVec3::new(5., 140.5, 3.), DVec3::Z, 5., PlayerId::HOST, &mut inv, 0, false);
        assert_eq!(inv.get(0), Some(Stack::new(COPPER_HORSE_ARMOR, 1)));
        assert_eq!(restored.mobs[0].mount.as_ref().unwrap().slots[1], None);
    }

    #[test]
    fn ids_icons_and_names_stay_in_the_reserved_ranges() {
        let ids: Vec<u16> = all().map(|i| i.0).collect();
        assert_eq!(ids, [1550, 1551, 1552, 1553, 1561, 1562, 1563, 1564, 1565, 1566, 1567]);
        for (k, item) in all().enumerate() {
            assert_eq!(icon_offset(item), Some(k as u16));
            assert_eq!(Item::from_name(item.name()), Some(item));
        }
        assert_eq!(ICON_COUNT as usize, ids.len());
        for piece in ArmorPiece::ALL {
            assert_eq!(Item::armor(piece, ArmorMaterial::Chainmail), chainmail(piece));
        }
    }
}
