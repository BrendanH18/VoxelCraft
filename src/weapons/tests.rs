use super::*;
use crate::enchant::{Enchantment, Enchants};
use crate::inventory::Stack;

#[test]
fn items_use_the_reserved_ids_with_java_names_and_stats() {
    assert_eq!((TRIDENT.0, MACE.0), (1495, 1496));
    let names: Vec<_> = all().map(|i| i.name()).collect();
    assert_eq!(names, NAMES);
    assert!(all().all(|i| i.max_stack() == 1 && i.is_valid()));
    // Java ids resolve, for loot tables and recipes.
    for (java, item) in [
        ("trident", TRIDENT),
        ("mace", MACE),
        ("wooden_spear", spear(Tier::Wood)),
        ("stone_spear", spear(Tier::Stone)),
        ("copper_spear", spear(Tier::Copper)),
        ("iron_spear", spear(Tier::Iron)),
        ("golden_spear", spear(Tier::Gold)),
        ("diamond_spear", spear(Tier::Diamond)),
        ("netherite_spear", spear(Tier::Netherite)),
    ] {
        assert_eq!(Item::from_name(java), Some(item), "{java}");
    }
    assert_eq!(TRIDENT.durability(), Some(250));
    assert_eq!(MACE.durability(), Some(500));
    assert_eq!(spear(Tier::Copper).durability(), Some(190));
    assert_eq!(spear(Tier::Netherite).durability(), Some(2031));
    assert!(spear(Tier::Netherite).fire_resistant() && !spear(Tier::Diamond).fire_resistant());
    assert!(TRIDENT.is_rare());
    // Every weapon has its own icon after the earlier item icons.
    let icons: Vec<_> = all().map(|i| i.icon_layer().unwrap()).collect();
    assert!(icons.windows(2).all(|w| w[1] == w[0] + 1));
    for i in all() {
        let index = crate::world::block::tex::item_index(i.icon_layer().unwrap()).unwrap();
        assert_eq!(crate::item::sprite_for_layer(index), Some(i.info().sprite), "{}", i.name());
    }
}

#[test]
fn attack_attributes_follow_items_and_tool_materials() {
    use crate::mining::{attack_damage, attack_speed, attack_strength_delay, minimum_attack_charge};
    assert_eq!(attack_damage(Some(TRIDENT)), 9.0);
    assert_eq!(attack_damage(Some(MACE)), 6.0);
    assert!((attack_speed(Some(TRIDENT)) - 1.1).abs() < 1e-6);
    assert!((attack_speed(Some(MACE)) - 0.6).abs() < 1e-6);
    let damage: Vec<_> = SPEAR_TIERS.iter().map(|&t| attack_damage(Some(spear(t)))).collect();
    assert_eq!(damage, [1.0, 2.0, 2.0, 3.0, 1.0, 4.0, 5.0]);
    // `1 / attackDuration` attacks a second: the wooden spear's 0.65 s jab.
    assert!((attack_speed(Some(spear(Tier::Wood))) - 1.0 / 0.65).abs() < 1e-5);
    assert!((attack_strength_delay(Some(spear(Tier::Netherite))) - 23.0).abs() < 1e-3);
    assert_eq!(minimum_attack_charge(Some(spear(Tier::Iron))), 1.0);
    assert_eq!(minimum_attack_charge(Some(MACE)), 0.0);
    assert!(!crate::mining::can_destroy_in_creative(TRIDENT));
    assert!(!crate::mining::can_destroy_in_creative(MACE));
    // `Weapon(1)` per hit; the tool components' 2 per block, none for spears.
    assert_eq!(crate::mining::wear(MACE, true), 1);
    assert_eq!(crate::mining::wear(TRIDENT, false), 2);
    assert_eq!(crate::mining::wear(spear(Tier::Iron), false), 0);
}

#[test]
fn the_smash_bonus_follows_maceitem() {
    use mace::*;
    assert_eq!(smash_bonus(1.5, false, Enchants::NONE), 0.0, "1.5 blocks isn't a smash");
    assert!((smash_bonus(2.0, false, Enchants::NONE) - 8.0).abs() < 1e-5);
    assert!((smash_bonus(3.0, false, Enchants::NONE) - 12.0).abs() < 1e-5);
    assert!((smash_bonus(5.0, false, Enchants::NONE) - 16.0).abs() < 1e-5);
    assert!((smash_bonus(8.0, false, Enchants::NONE) - 22.0).abs() < 1e-5);
    assert!((smash_bonus(20.0, false, Enchants::NONE) - 34.0).abs() < 1e-5);
    assert_eq!(smash_bonus(20.0, true, Enchants::NONE), 0.0, "gliding never smashes");
    // Density adds half a point per level for every block fallen.
    let density = Enchants::NONE.with(Enchantment::Density, 5);
    assert!((smash_bonus(10.0, false, density) - (24.0 + 25.0)).abs() < 1e-4);
    assert!((breach_effectiveness(Enchants::NONE.with(Enchantment::Breach, 4)) + 0.6).abs() < 1e-6);
    // Knockback: 0.7 a block inside 3.5, doubled after five blocks.
    assert!((knockback_power(1.5, false, 0.0) - 2.0 * KNOCKBACK_POWER).abs() < 1e-9);
    assert!((knockback_power(1.5, true, 0.0) - 4.0 * KNOCKBACK_POWER).abs() < 1e-9);
    let push = smash_push(glam::DVec3::ZERO, glam::DVec3::new(1.0, 0.0, 0.0), false, 0.0).unwrap();
    assert!((push.x - 2.5 * KNOCKBACK_POWER).abs() < 1e-9 && (push.y - KNOCKBACK_POWER).abs() < 1e-9);
    assert!(smash_push(glam::DVec3::ZERO, glam::DVec3::new(4.0, 0.0, 0.0), false, 0.0).is_none());
    assert_eq!([1, 2, 3].map(wind_burst_multiplier), [1.2, 1.75, 2.2]);
    assert!((wind_burst_multiplier(4) - 2.55).abs() < 1e-6);
    let wind = Enchants::NONE.with(Enchantment::WindBurst, 1);
    assert!(wind_burst_applies(wind, 1.5, false) && !wind_burst_applies(wind, 1.4, false));
    // The attacker at the blast's centre goes straight up at full strength.
    let up = wind_burst_push(glam::DVec3::ZERO, glam::DVec3::ZERO, glam::DVec3::Y * 1.62, 1.0, 1.2, 0.0).unwrap();
    assert!((up - glam::DVec3::Y * 1.2).length() < 1e-6);
    assert_eq!(SmashSound::of(false, 10.0), SmashSound::Air);
    assert_eq!(SmashSound::of(true, 5.0), SmashSound::Ground);
    assert_eq!(SmashSound::of(true, 5.5), SmashSound::Heavy);
}

#[test]
fn spear_charges_need_speed_within_their_windows() {
    let k = spear::kinetic(Tier::Wood);
    assert_eq!(k.delay_ticks, 15);
    assert_eq!((k.dismount.max_ticks, k.knockback.max_ticks, k.damage.max_ticks), (100, 200, 300));
    assert_eq!(k.damage_use_duration(), 315);
    assert_eq!(k.contact(10, 20.0, 0.0, 1.0, true), None, "still in the delay");
    // A sprinting player (5.6 b/s) at a still target: damage and knockback.
    let c = k.contact(20, 5.6, 0.0, 1.0, true).unwrap();
    assert!(c.damage && c.knockback && !c.dismount);
    assert_eq!(c.amount, 1.0 + (5.6f64 * f64::from(0.7f32)).floor() as f32);
    // Too slow for anything.
    assert_eq!(k.contact(20, 4.0, 0.0, 1.0, true), None);
    // After the damage window only a fast charge still knocks back... until 200 ticks.
    assert_eq!(k.contact(15 + 301, 20.0, 0.0, 1.0, true), None);
    // Mobs need a fifth of the speed.
    assert!(k.contact(20, 1.2, 0.0, 1.0, false).is_some_and(|c| c.damage));
    let n = spear::kinetic(Tier::Netherite);
    assert_eq!(n.delay_ticks, 8);
    assert_eq!(spear::player_range(false), spear::Range { min: 2.0, max: 4.5, margin: 0.125 });
    assert_eq!(spear::player_range(true).max, 6.5);
    let lunge = spear::lunge_impulse(2, glam::DVec3::new(0.6, -0.8, 0.0));
    assert!((lunge.x - 0.6 * f64::from(0.916f32)).abs() < 1e-6 && lunge.y == 0.0);
    assert!(!spear::lunge_allowed(false, false, false, 6, false));
    assert!(spear::lunge_allowed(false, false, false, 6, true));
}

#[test]
fn trident_numbers_follow_tridentitem() {
    assert_eq!(trident::riptide_strength(Enchants::NONE), 0.0);
    let r = |l| trident::riptide_strength(Enchants::NONE.with(Enchantment::Riptide, l));
    assert_eq!([r(1), r(2), r(3)], [1.5, 2.25, 3.0]);
    assert_eq!(trident::loyalty(Enchants::NONE.with(Enchantment::Loyalty, 3)), 3);
    assert_eq!(trident::mob_uncertainty(3), 2.0);
    let ahead = trident::rotation_vector(0.0, 0.0);
    assert!((ahead - glam::DVec3::Z).length() < 1e-6, "Java yaw 0 faces +Z");
    assert_eq!(Cue::riptide(2), Cue::Riptide2);
    assert_eq!(Cue::lunge(3), Cue::Lunge3);
    assert_eq!(Cue::spear(spear(Tier::Wood), true, false), Cue::SpearWoodUse);
    assert_eq!(Cue::spear(spear(Tier::Iron), false, true), Cue::SpearHit);
}

#[test]
fn enchantments_fit_the_java_26_3_tags() {
    let sword = Item::tool(crate::item::ToolKind::Sword, Tier::Iron);
    let axe = Item::tool(crate::item::ToolKind::Axe, Tier::Iron);
    let iron = spear(Tier::Iron);
    for (e, yes, no) in [
        (Enchantment::Loyalty, vec![TRIDENT], vec![MACE, iron, sword]),
        (Enchantment::Impaling, vec![TRIDENT], vec![MACE, iron]),
        (Enchantment::Riptide, vec![TRIDENT], vec![iron]),
        (Enchantment::Channeling, vec![TRIDENT], vec![sword]),
        (Enchantment::Density, vec![MACE], vec![TRIDENT, sword]),
        (Enchantment::Breach, vec![MACE], vec![iron]),
        (Enchantment::WindBurst, vec![MACE], vec![TRIDENT, iron]),
        (Enchantment::Lunge, vec![iron, spear(Tier::Wood)], vec![sword, MACE]),
        (Enchantment::Sharpness, vec![sword, axe, iron], vec![MACE, TRIDENT]),
        (Enchantment::Smite, vec![sword, axe, iron, MACE], vec![TRIDENT]),
        (Enchantment::FireAspect, vec![sword, iron, MACE], vec![axe, TRIDENT]),
        (Enchantment::Knockback, vec![sword, iron], vec![MACE, axe]),
        (Enchantment::Looting, vec![sword, iron], vec![MACE]),
        (Enchantment::SweepingEdge, vec![sword], vec![iron]),
        (Enchantment::Unbreaking, vec![TRIDENT, MACE, iron], vec![]),
        (Enchantment::Mending, vec![TRIDENT, MACE, iron], vec![]),
    ] {
        for item in yes {
            assert!(e.fits(item), "{e:?} on {}", item.name());
        }
        for item in no {
            assert!(!e.fits(item), "{e:?} not on {}", item.name());
        }
    }
    assert!(!Enchantment::Riptide.compatible(Enchantment::Loyalty));
    assert!(!Enchantment::Density.compatible(Enchantment::Breach));
    assert!(!Enchantment::Impaling.compatible(Enchantment::Sharpness));
    assert!(Enchantment::WindBurst.compatible(Enchantment::Density));
    assert_eq!(crate::enchant::enchantability(TRIDENT), 1);
    assert_eq!(crate::enchant::enchantability(MACE), 15);
    assert_eq!(crate::enchant::enchantability(spear(Tier::Gold)), 22);
    assert!(Enchantment::WindBurst.def().treasure && !Enchantment::WindBurst.on_random_loot());
    assert!(!Enchantment::tradeable().contains(&Enchantment::WindBurst));
    // The enchanting table never offers Wind Burst.
    for seed in 0..40 {
        for slot in 0..3 {
            let offered = crate::enchant::offer_enchants(seed, MACE, slot, 30);
            assert!(offered.iter().all(|(e, _)| *e != Enchantment::WindBurst));
        }
    }
}

#[test]
fn wind_burst_saves_at_bit_41_and_keeps_bit_40_for_swift_sneak() {
    assert_eq!(Enchantment::WindBurst as u32, 41);
    assert_eq!(Enchantment::ALL[40], Enchantment::WindBurst);
    assert_eq!(Enchantment::from_name("wind_burst"), Some(Enchantment::WindBurst));
    let e = Enchants::NONE.with(Enchantment::WindBurst, 3).with(Enchantment::Lunge, 2);
    assert_eq!(e.0 >> (41 * 3) & 7, 3);
    let saved = e.to_hex();
    assert_eq!(Enchants::from_hex(&saved), Some(e));
    // A save from a build with Swift Sneak (bit 40) keeps it through this one.
    let swift = Enchants(e.0 | (2u128 << (40 * 3)));
    assert_eq!(Enchants::from_hex(&swift.to_hex()), Some(swift));
    assert_eq!(e.iter().count(), 2);
    assert_eq!(Enchantment::WindBurst.describe(2), "Wind Burst II");
}

#[test]
fn spears_and_maces_are_crafted_and_smithed_as_in_java() {
    use crate::crafting::Grid;
    let craft = |material: Item| {
        let mut g = Grid::new(3);
        g.cells[2] = Some(Stack::new(material, 1));
        g.cells[4] = Some(Stack::new(Item::STICK, 1));
        g.cells[6] = Some(Stack::new(Item::STICK, 1));
        g.result().map(|s| s.item)
    };
    assert_eq!(craft(Item::from(crate::world::block::Block::PLANKS)), Some(spear(Tier::Wood)));
    assert_eq!(craft(Item::from(crate::world::block::Block::COBBLESTONE)), Some(spear(Tier::Stone)));
    assert_eq!(craft(Item::COPPER_INGOT), Some(spear(Tier::Copper)));
    assert_eq!(craft(Item::IRON_INGOT), Some(spear(Tier::Iron)));
    assert_eq!(craft(Item::GOLD_INGOT), Some(spear(Tier::Gold)));
    assert_eq!(craft(Item::DIAMOND), Some(spear(Tier::Diamond)));
    // The netherite spear only comes from the smithing table.
    assert_eq!(craft(Item::NETHERITE_INGOT), None);
    let base = Stack { damage: 40, ..Stack::new(spear(Tier::Diamond), 1) };
    let upgraded =
        crate::smithing::upgrade(Stack::new(Item::NETHERITE_UPGRADE, 1), base, Stack::new(Item::NETHERITE_INGOT, 1))
            .unwrap();
    assert_eq!((upgraded.item, upgraded.damage), (spear(Tier::Netherite), 40));
    assert!(crate::smithing::is_base(spear(Tier::Diamond)));
    // The mace needs Track C's breeze rod; until then there is no recipe.
    let mut g = Grid::new(3);
    g.cells[1] = Some(Stack::new(crate::world::heavy_core::HEAVY_CORE, 1));
    g.cells[4] = Some(Stack::new(Item::BLAZE_ROD, 1));
    assert_eq!(g.result(), None);
    assert_eq!(breeze_rod().is_some(), crate::crafting::recipes().iter().any(|r| r.result.item == MACE));
    // Repairs: the tier's material for spears, nothing for the trident.
    assert!(crate::enchant::repairs(spear(Tier::Iron), Item::IRON_INGOT));
    assert!(crate::enchant::repairs(spear(Tier::Netherite), Item::NETHERITE_INGOT));
    assert!(!crate::enchant::repairs(TRIDENT, Item::PRISMARINE_SHARD));
    assert!(!crate::enchant::repairs(MACE, Item::IRON_INGOT));
    // The wooden spear burns like other large wooden items.
    assert_eq!(crate::content::fuel::ticks(spear(Tier::Wood)), Some(200));
}

#[test]
fn impaling_hurts_the_aquatic_tag_only() {
    use crate::entity::MobKind;
    let e = Enchants::NONE.with(Enchantment::Impaling, 5);
    assert_eq!(impaling_bonus(e, MobKind::Guardian), 12.5);
    assert_eq!(impaling_bonus(e, MobKind::Axolotl), 12.5);
    assert_eq!(impaling_bonus(e, MobKind::Drowned), 0.0, "drowned aren't #aquatic");
    assert_eq!(impaling_bonus(Enchants::NONE, MobKind::Cod), 0.0);
}

#[test]
fn the_heavy_core_is_a_waterloggable_eight_pixel_core() {
    use crate::world::block::{Block, Shaped};
    use crate::world::heavy_core::*;
    assert_eq!((HEAVY_CORE.0, WATERLOGGED.0), (15744, 15745));
    assert_eq!(HEAVY_CORE.name(), "heavy core");
    assert_eq!(HEAVY_CORE.shaped(), Some(Shaped::Custom));
    assert_eq!(HEAVY_CORE.with_water(true), WATERLOGGED);
    assert_eq!(WATERLOGGED.with_water(false), HEAVY_CORE);
    assert!(WATERLOGGED.holds_water() && !HEAVY_CORE.holds_water());
    assert_eq!(HEAVY_CORE.hardness(), 10.0);
    assert_eq!(HEAVY_CORE.blast_resistance(), 1200.0);
    assert_eq!(WATERLOGGED.drop(), Some(Item::from_block(HEAVY_CORE)));
    assert_eq!(WATERLOGGED.base(), HEAVY_CORE);
    assert!(crate::mining::can_harvest(HEAVY_CORE, None), "no tool needed for the drop");
    assert_eq!(Item::from_name("heavy_core"), Some(Item::from_block(HEAVY_CORE)));
    assert!(is_epic(Item::from_block(HEAVY_CORE)) && is_epic(MACE));
    let shape = crate::world::shape::collision(HEAVY_CORE, |_| Block::AIR, Block::STONE);
    assert_eq!(shape.as_slice().len(), 1);
    assert!(Block::creative_palette().any(|b| b == HEAVY_CORE));
}

#[test]
fn smash_impulse_measures_later_falls_from_the_impact() {
    use crate::simulation::survival::{Env, Vitals};
    let mut v = Vitals::default();
    let air = |y: f64| Env { y, ..Default::default() };
    let ground = |y: f64| Env { y, on_ground: true, ..Default::default() };
    // Fall 20 blocks, smash at 80, bounce up 6 (Wind Burst), land at 79.
    for y in [100.0, 90.0, 80.0] {
        v.tick(0.05, &air(y), false);
    }
    v.smash_impulse(80.0);
    v.end_impulse_grace();
    assert_eq!(v.fall_distance(80.0), 0.0);
    for y in [83.0, 86.0, 84.0, 81.0] {
        v.tick(0.05, &air(y), false);
    }
    let hurts = v.tick(0.05, &ground(79.0), false);
    assert_eq!(hurts.fall, 0.0, "only 1 block below the impact counts");
    // Without the impulse the same 7-block drop would hurt.
    let mut plain = Vitals::default();
    for y in [86.0, 84.0, 81.0] {
        plain.tick(0.05, &air(y), false);
    }
    assert!(plain.tick(0.05, &ground(79.0), false).fall > 0.0);
}
