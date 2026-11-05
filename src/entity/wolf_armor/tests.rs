use super::*;
use crate::{
    entity::{Ctx, MobWorld, Target},
    inventory::{Inventory, Stack},
    physics::{BlockSource, test_util::Grid},
    simulation::{effects::Effect, lighting::SpawnLight},
    world::{
        World,
        terrain::{Biome, Dimension, Generator},
    },
};
use glam::IVec3;
struct Flat(Grid);
impl BlockSource for Flat {
    fn block(&self, p: IVec3) -> Option<Block> {
        self.0.block(p)
    }
}
impl MobWorld for Flat {
    fn loaded(&self, _: IVec3) -> bool {
        true
    }
    fn exposed(&self, _: IVec3) -> bool {
        true
    }
    fn surface(&self, _: i32, _: i32) -> Option<i32> {
        Some(0)
    }
    fn biome(&self, _: i32, _: i32) -> Biome {
        Biome::Forest
    }
}
fn ctx() -> Ctx {
    Ctx {
        players: Vec::<Target>::new(),
        daylight: 1.0,
        spawn_light: SpawnLight::DAY,
        spawning: false,
        raining: false,
        dimension: Dimension::Overworld,
    }
}
fn wolf(e: &mut Entities) {
    e.spawn(MobKind::Wolf, DVec3::new(0.5, 1.0, 0.5));
    e.ensure_mob_ids();
    let m = e.mobs.last_mut().unwrap();
    m.animal.as_mut().unwrap().owner = Some(PlayerId::HOST);
    m.health = 40.0;
    m.on_ground = true;
}
fn equip(e: &mut Entities, stack: Stack) {
    assert_eq!(e.use_wolf_armor(0, Some(stack), PlayerId::HOST, false), Some(Use::Equip));
}
#[test]
fn wolf_armor_only_the_owner_equips_adults_and_repairs_sitting_wolves() {
    let mut e = Entities::new(1);
    wolf(&mut e);
    let item = Some(Stack::new(Item::WOLF_ARMOR, 1));
    assert_eq!(e.use_wolf_armor(0, item, PlayerId(1), true), None);
    e.mobs[0].baby = true;
    assert_eq!(e.use_wolf_armor(0, item, PlayerId::HOST, false), None);
    e.mobs[0].baby = false;
    equip(&mut e, Stack::new(Item::WOLF_ARMOR, 1));
    assert_eq!(e.use_wolf_armor(0, item, PlayerId::HOST, false), None);
    e.mobs[0].wolf_armor.as_mut().unwrap().armor.as_mut().unwrap().damage = 10;
    let scute = Some(Stack::new(Item::ARMADILLO_SCUTE, 1));
    assert_eq!(e.use_wolf_armor(0, scute, PlayerId::HOST, false), None);
    e.mobs[0].animal.as_mut().unwrap().sitting = true;
    assert_eq!(e.use_wolf_armor(0, scute, PlayerId(1), false), None);
    assert_eq!(e.use_wolf_armor(0, scute, PlayerId::HOST, false), Some(Use::Repair));
    assert_eq!(e.mobs[0].wolf_armor.as_ref().unwrap().armor.unwrap().damage, 2);
    assert_eq!(e.use_wolf_armor(0, scute, PlayerId::HOST, false), Some(Use::Repair));
    assert_eq!(e.mobs[0].wolf_armor.as_ref().unwrap().armor.unwrap().damage, 0);
}
#[test]
fn wolf_armor_absorbs_the_whole_breaking_hit_before_resistance_and_wakes_the_wolf() {
    let mut e = Entities::new(2);
    wolf(&mut e);
    equip(&mut e, Stack::new(Item::WOLF_ARMOR, 1));
    e.mobs[0].animal.as_mut().unwrap().sitting = true;
    assert!(!e.mobs[0].damage(7.1, Some(DVec3::X), &mut e.rng));
    assert_eq!(e.mobs[0].health, 40.0);
    assert!(!e.mobs[0].animal.as_ref().unwrap().sitting);
    assert_eq!(e.mobs[0].wolf_armor.as_ref().unwrap().armor.unwrap().damage, 8);
    e.mobs[0].effects.add(Effect::Resistance, 3, 200);
    e.mobs[0].damage(10.0, None, &mut e.rng);
    assert_eq!(e.mobs[0].wolf_armor.as_ref().unwrap().armor.unwrap().damage, 18);
    assert_eq!(e.mobs[0].health, 40.0);
    e.mobs[0].effects = Default::default();
    e.mobs[0].damage_bypassing_wolf_armor(2.0, None, &mut e.rng);
    assert_eq!(e.mobs[0].health, 38.0);
    assert_eq!(e.mobs[0].wolf_armor.as_ref().unwrap().armor.unwrap().damage, 18);
    e.mobs[0].damage(1000.0, Some(DVec3::X), &mut e.rng);
    assert_eq!(e.mobs[0].health, 38.0);
    assert!(e.mobs[0].wolf_armor.as_ref().unwrap().armor.is_none());
    e.mobs[0].damage(1.0, None, &mut e.rng);
    assert_eq!(e.mobs[0].health, 37.0);
}
#[test]
fn wolf_armor_uses_full_melee_damage_in_26_3_and_magic_status_bypasses_it() {
    let mut e = Entities::new(3);
    wolf(&mut e);
    equip(&mut e, Stack::new(Item::WOLF_ARMOR, 1));
    e.spawn(MobKind::Zombie, DVec3::new(3.5, 1.0, 0.5));
    e.ensure_mob_ids();
    let mut events = vec![EntityEvent::MobHit {
        target: e.mobs[0].uid,
        attacker: e.mobs[1].uid,
        damage: 5.0,
        knockback: DVec3::ZERO,
    }];
    e.resolve_mob_hits(&mut events);
    assert_eq!(e.mobs[0].wolf_armor.as_ref().unwrap().armor.unwrap().damage, 5);
    e.mobs.truncate(1);
    e.mobs[0].wolf_armor.as_mut().unwrap().armor.as_mut().unwrap().damage = 0;
    e.mobs[0].effects.add(Effect::Wither, 0, 100);
    let w = Flat(Grid::flat(1));
    for _ in 0..80 {
        e.update(0.05, &w, &ctx());
    }
    assert!(e.mobs[0].health < 40.0);
    assert_eq!(e.mobs[0].wolf_armor.as_ref().unwrap().armor.unwrap().damage, 0);
    assert_eq!((cracks(3), cracks(4), cracks(19), cracks(20), cracks(43), cracks(44)), (0, 1, 1, 2, 2, 3));
}
#[test]
fn wolf_armor_absorbs_fall_damage_while_an_unarmored_wolf_loses_health() {
    let w = Flat(Grid::flat(1));
    let mut outcomes = Vec::new();
    for armored in [false, true] {
        let mut e = Entities::new(4);
        wolf(&mut e);
        e.mobs[0].pos.y = 20.0;
        e.mobs[0].on_ground = false;
        if armored {
            equip(&mut e, Stack::new(Item::WOLF_ARMOR, 1));
        }
        for _ in 0..100 {
            e.update(0.05, &w, &ctx());
        }
        let m = &e.mobs[0];
        outcomes.push((m.health, m.wolf_armor.as_ref().unwrap().armor.map(|a| a.damage)));
    }
    assert!(outcomes[0].0 < 40.0);
    assert_eq!(outcomes[1].0, 40.0);
    assert!(outcomes[1].1.is_some_and(|n| n > 0 && n < 30));
}
#[test]
fn wolf_armor_dyes_mix_native_rgb_preserve_components_and_wash_without_repairing() {
    let mut original = Stack::new(Item::WOLF_ARMOR, 1).with_name("Cedar").unwrap();
    original.damage = 17;
    original.repair_cost = 3;
    original.enchants.set(Enchantment::BindingCurse, 1);
    let red = dye(original, &[DyeColor::Red]).unwrap();
    assert_eq!(color(red), Some(DyeColor::Red.rgb()));
    let mixed = dye(red, &[DyeColor::Blue]).unwrap();
    assert_eq!(color(mixed), Some([173, 83, 152]));
    assert_eq!(
        (mixed.damage, mixed.repair_cost, mixed.name, mixed.enchants),
        (original.damage, original.repair_cost, original.name, original.enchants)
    );
    let (water, clean) =
        crate::world::utilities::cauldron::wash_wolf_armor(crate::world::utilities::Contents::Water(2), mixed).unwrap();
    assert_eq!(water, crate::world::utilities::Contents::Water(1));
    assert_eq!(clean, original);
    assert!(washed(original).is_none());
    let mut g = crate::crafting::Grid::new(2);
    g.cells[0] = Some(original);
    g.cells[1] = Some(Stack::new(DyeColor::Red.dye(), 1));
    g.cells[2] = Some(Stack::new(DyeColor::Blue.dye(), 1));
    assert_eq!(color(g.result().unwrap()), Some([173, 83, 152]));
    g.cells[3] = Some(Stack::new(Item::WOLF_ARMOR, 1));
    assert!(g.result().is_none());
}
#[test]
fn wolf_armor_crafting_pair_repair_and_anvil_scutes_follow_separate_bonuses() {
    let mut g = crate::crafting::Grid::new(3);
    for n in [0, 3, 4, 5, 6, 8] {
        g.cells[n] = Some(Stack::new(Item::ARMADILLO_SCUTE, 1));
    }
    assert_eq!(g.result(), Some(Stack::new(Item::WOLF_ARMOR, 1)));
    assert_eq!((Item::WOLF_ARMOR.durability(), Item::WOLF_ARMOR.max_stack()), (Some(64), 1));
    assert!(Item::WOLF_ARMOR.as_armor().is_none());
    let mut a = dye(Stack::new(Item::WOLF_ARMOR, 1), &[DyeColor::Blue]).unwrap().with_name("Dusk").unwrap();
    a.damage = 40;
    a.enchants.set(Enchantment::BindingCurse, 1);
    a.enchants.set(Enchantment::Unbreaking, 3);
    let mut b = Stack::new(Item::WOLF_ARMOR, 1);
    b.damage = 40;
    b.enchants.set(Enchantment::VanishingCurse, 1);
    let mut g = crate::crafting::Grid::new(2);
    g.cells[0] = Some(a);
    g.cells[1] = Some(b);
    let repaired = g.result().unwrap();
    assert_eq!(repaired.damage, 13);
    assert_eq!(repaired.entity_data, 0);
    assert!(repaired.name.as_str().is_none());
    assert!(
        repaired.enchants.has(Enchantment::BindingCurse)
            && repaired.enchants.has(Enchantment::VanishingCurse)
            && !repaired.enchants.has(Enchantment::Unbreaking)
    );
    let anvil = crate::enchant::anvil(a, Some(Stack::new(Item::ARMADILLO_SCUTE, 4)), false).unwrap();
    assert_eq!((anvil.output.damage, anvil.uses), (0, Some(3)));
    assert_eq!(anvil.output.entity_data, a.entity_data);
}
#[test]
fn wolf_armor_restart_replication_and_shearing_keep_the_stack_and_binding_rules() {
    let mut e = Entities::new(5);
    wolf(&mut e);
    let mut armor = dye(Stack::new(Item::WOLF_ARMOR, 1), &[DyeColor::Green]).unwrap().with_name("Fern").unwrap();
    armor.damage = 23;
    armor.enchants.set(Enchantment::BindingCurse, 1);
    equip(&mut e, armor);
    let mut back = Entities::new(6);
    back.load_nether_mobs(&e.nether_mobs_to_string());
    assert_eq!(back.mobs[0].wolf_armor.as_ref().unwrap().armor, Some(armor));
    let mut copy = Mob::new(MobKind::Wolf, DVec3::ZERO, 0.0);
    copy.apply_look(&e.mobs[0].look());
    assert_eq!(copy.wolf_armor.as_ref().unwrap().armor, Some(armor));
    let shears = Some(Stack::new(Item::SHEARS, 1));
    assert_eq!(e.use_wolf_armor(0, shears, PlayerId::HOST, false), None);
    assert_eq!(e.use_wolf_armor(0, shears, PlayerId(9), true), None);
    assert_eq!(e.use_wolf_armor(0, shears, PlayerId::HOST, true), Some(Use::Remove));
    assert_eq!(e.items[0].stack, armor);
    copy.apply_look(&e.mobs[0].look());
    assert!(copy.wolf_armor.as_ref().unwrap().armor.is_none());
}
#[test]
fn wolf_armor_drops_when_the_wolf_dies_unless_the_stack_has_vanishing() {
    let w = Flat(Grid::flat(1));
    for vanishing in [false, true] {
        let mut e = Entities::new(6);
        wolf(&mut e);
        let mut armor = dye(Stack::new(Item::WOLF_ARMOR, 1), &[DyeColor::Cyan]).unwrap();
        armor.damage = 7;
        if vanishing {
            armor.enchants.set(Enchantment::VanishingCurse, 1);
        }
        equip(&mut e, armor);
        e.mobs[0].damage_bypassing_wolf_armor(1000.0, None, &mut e.rng);
        for _ in 0..40 {
            e.update(0.05, &w, &ctx());
        }
        assert_eq!(e.items.iter().filter(|h| h.stack == armor).count(), usize::from(!vanishing));
    }
}
#[test]
fn wolf_armor_creative_repairs_consume_scutes_on_the_authoritative_adapter() {
    let w = World::new_headless(std::sync::Arc::new(Generator::new(42)), Default::default(), 2);
    let mut e = Entities::new(7);
    wolf(&mut e);
    e.mobs[0].pos = DVec3::new(2.5, 150.0, 0.5);
    let mut armor = Stack::new(Item::WOLF_ARMOR, 1);
    armor.damage = 12;
    equip(&mut e, armor);
    e.mobs[0].animal.as_mut().unwrap().sitting = true;
    let mut player = crate::player::Player::new(DVec3::new(0.5, 150.0, 0.5));
    player.yaw = 0.0;
    player.pitch = ((150.4 - player.eye().y) / 2.0).atan() as f32;
    let mut inv = Inventory::default();
    inv.slots[0] = Some(Stack::new(Item::ARMADILLO_SCUTE, 2));
    assert!(crate::survival_items::use_animal_target(
        &player,
        &mut inv,
        0,
        true,
        &w,
        &mut e,
        PlayerId::HOST,
        crate::survival_items::Reach { block: 4.5, entity: 3.0 }
    ));
    assert_eq!(inv.get(0).unwrap().count, 1);
    assert_eq!(e.mobs[0].wolf_armor.as_ref().unwrap().armor.unwrap().damage, 4);
}
