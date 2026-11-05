use super::*;
use crate::{
    entity::{
        MobWorld, Rng,
        category::MobCategory,
        monster_spawns::{self, JavaMonster},
        spawn_rules,
    },
    inventory::Inventory,
    item::Item,
    physics::{BlockSource, Shape, test_util::Grid},
    simulation::lighting::SpawnLight,
    world::{
        World,
        block::Block,
        terrain::{Biome, Dimension, Generator},
    },
};
use glam::IVec3;
struct Habitat {
    grid: Grid,
    active: bool,
}
impl BlockSource for Habitat {
    fn block(&self, p: IVec3) -> Option<Block> {
        self.grid.block(p)
    }
}
impl MobWorld for Habitat {
    fn loaded(&self, _: IVec3) -> bool {
        true
    }
    fn exposed(&self, _: IVec3) -> bool {
        true
    }
    fn entity_ticking(&self, _: IVec3) -> bool {
        self.active
    }
    fn surface(&self, _: i32, _: i32) -> Option<i32> {
        Some(63)
    }
    fn biome(&self, _: i32, _: i32) -> Biome {
        Biome::Jungle
    }
    fn sky_light_level(&self, _: IVec3) -> Option<u8> {
        Some(0)
    }
}
fn world() -> Habitat {
    Habitat { grid: Grid::flat(64), active: true }
}
fn ctx() -> Ctx {
    Ctx {
        players: Vec::new(),
        daylight: 0.0,
        spawn_light: SpawnLight::NIGHT,
        spawning: false,
        raining: false,
        dimension: Dimension::Overworld,
    }
}
fn ocelot(e: &mut Entities) {
    e.spawn(MobKind::Ocelot, DVec3::new(0.5, 64.0, 0.5));
    e.ensure_mob_ids();
    e.mobs.last_mut().unwrap().on_ground = true;
}
#[test]
fn ocelot_reference_attributes_food_empty_loot_and_creature_category() {
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/ocelots.json")).unwrap();
    let m = Mob::new(MobKind::Ocelot, DVec3::ZERO, 0.0);
    assert_eq!(m.health, 10.0);
    assert_eq!(m.kind.shape(), Shape::new(0.3, 0.7));
    assert_eq!(goals::movement_speed(m.kind), facts["constants"]["movement_speed"].as_f64().unwrap());
    assert_eq!(m.kind.melee(), (3.0, "was slain by an ocelot"));
    assert_eq!(m.kind.category(), MobCategory::Creature);
    assert!(!m.kind.is_hostile());
    assert!(m.kind.leashable());
    assert!(m.kind.breeding_food(Item::COD) && m.kind.breeding_food(Item::SALMON));
    assert!(!m.kind.breeding_food(Item::COOKED_COD));
    assert!(m.kind.drops(&mut Rng::new(1), 3).is_empty());
    assert_eq!(facts["tags/item/ocelot_food"]["values"].as_array().unwrap().len(), 2);
}
#[test]
fn ocelot_spawns_on_high_grass_or_leaves_even_in_darkness_and_rejects_one_third() {
    let mut w = world();
    let rules = monster_spawns::SpawnRules { dimension: Dimension::Overworld, moon: 0.0, light: SpawnLight::NIGHT };
    let mut rng = Rng::new(42);
    let at = IVec3::new(0, 64, 0);
    for floor in [Block::GRASS, Block::LEAVES] {
        w.grid.set(at - IVec3::Y, floor);
        let n = (0..3000).filter(|_| spawn_rules::spot_ok(&w, MobKind::Ocelot, at, rules, &mut rng)).count();
        assert!((1900..2100).contains(&n), "{n}");
    }
    w.grid.set(at - IVec3::Y, Block::STONE);
    assert!(!spawn_rules::spot_ok(&w, MobKind::Ocelot, at, rules, &mut rng));
    w.grid.set(IVec3::new(0, 61, 0), Block::GRASS);
    w.grid.set(IVec3::new(0, 62, 0), Block::AIR);
    w.grid.set(IVec3::new(0, 63, 0), Block::AIR);
    assert!(!spawn_rules::spot_ok(&w, MobKind::Ocelot, IVec3::new(0, 62, 0), rules, &mut rng));
}
#[test]
fn ocelot_monster_list_weights_and_natural_groups_keep_creature_identity() {
    for (biome, max) in [(Biome::Jungle, 3), (Biome::BambooJungle, 1)] {
        let row = monster_spawns::monster_list(biome).iter().find(|e| e.mob == JavaMonster::Ocelot).unwrap();
        assert_eq!((row.weight, row.min, row.max), (2, 1, max));
        assert!(
            !super::super::spawn_lists::biome_list(biome, MobCategory::Creature)
                .iter()
                .any(|r| r.mob.kind() == Some(MobKind::Ocelot))
        );
    }
    assert_eq!(JavaMonster::Ocelot.kind(), Some(MobKind::Ocelot));
    let mut e = Entities::new(2);
    let mut group = monster_spawns::PackGroup::default();
    for x in 0..3 {
        e.spawn_natural(MobKind::Ocelot, DVec3::new(x as f64 * 2.0, 64.0, 0.0), Biome::Jungle, &mut group);
    }
    assert!(!e.mobs[0].baby);
    assert!(e.mobs[1].baby && e.mobs[2].baby);
    assert_eq!(e.mobs[1].shape(), Shape::new(0.15, 0.35));
    assert!(e.mobs.iter().all(|m| !m.ocelot.as_ref().unwrap().trusting));
}
#[test]
fn ocelot_temptation_is_frightened_by_small_moves_and_turns_until_trusting() {
    let mut m = Mob::new(MobKind::Ocelot, DVec3::ZERO, 0.0);
    let mut t = Target::new(PlayerId::HOST, DVec3::new(2.0, 0.0, 0.0), true);
    t.look = DVec3::Z;
    assert!(m.ocelot_tempt_allowed(&t));
    t.pos.x += 0.09;
    assert!(m.ocelot_tempt_allowed(&t));
    t.pos.x += 0.02;
    assert!(!m.ocelot_tempt_allowed(&t));
    m.ocelot.as_mut().unwrap().tempt_anchor = None;
    t.pos.x = 2.0;
    assert!(m.ocelot_tempt_allowed(&t));
    t.look = DVec3::new(6_f64.to_radians().sin(), 0.0, 6_f64.to_radians().cos());
    assert!(!m.ocelot_tempt_allowed(&t));
    m.ocelot.as_mut().unwrap().trusting = true;
    t.pos.x = 3.0;
    t.look = DVec3::X;
    assert!(m.ocelot_tempt_allowed(&t));
}
#[test]
fn ocelot_trust_requires_running_tempt_goal_and_range_and_does_not_tame_or_grow() {
    let mut e = Entities::new(3);
    ocelot(&mut e);
    let fish = Some(Stack::new(Item::COD, 1));
    let near = e.mobs[0].pos + DVec3::X * 2.0;
    assert!(!e.try_trust_ocelot(0, fish, near));
    e.mobs[0].goals.running = Some(0);
    e.mobs[0].baby = true;
    e.mobs[0].age = -100;
    assert!(!e.try_trust_ocelot(0, fish, e.mobs[0].pos + DVec3::X * 3.0));
    for _ in 0..100 {
        if e.mobs[0].ocelot.as_ref().unwrap().trusting {
            break;
        }
        assert!(e.try_trust_ocelot(0, fish, near));
    }
    let m = &e.mobs[0];
    assert!(m.ocelot.as_ref().unwrap().trusting);
    assert_eq!(m.age, -100);
    assert!(m.animal.as_ref().unwrap().owner.is_none());
    assert!(!m.animal.as_ref().unwrap().sitting);
    assert_eq!(m.animal.as_ref().unwrap().love, 0.0);
}
#[test]
fn ocelot_breeds_with_fish_but_offspring_start_untrusting_and_age() {
    let w = world();
    let mut e = Entities::new(4);
    for x in [0.5, 1.5] {
        ocelot(&mut e);
        let m = e.mobs.last_mut().unwrap();
        m.pos.x = x;
        m.ocelot.as_mut().unwrap().trusting = true;
    }
    for i in 0..2 {
        assert_eq!(e.use_animal(i, Some(Stack::new(Item::SALMON, 1)), PlayerId::HOST), Some(true));
    }
    for _ in 0..100 {
        e.update(0.05, &w, &ctx());
        if e.mobs.len() == 3 {
            break;
        }
    }
    assert_eq!(e.mobs.len(), 3);
    assert!(e.mobs[2].baby);
    assert!(!e.mobs[2].ocelot.as_ref().unwrap().trusting);
    let old = e.mobs[2].age;
    e.use_animal(2, Some(Stack::new(Item::COD, 1)), PlayerId::HOST);
    assert!(e.mobs[2].age > old);
    e.mobs[2].age = -1;
    e.update(0.05, &w, &ctx());
    assert!(!e.mobs[2].baby);
}
#[test]
fn ocelot_trust_saves_and_replicates_while_temptation_and_lifetime_restart() {
    let mut e = Entities::new(5);
    ocelot(&mut e);
    e.mobs[0].age = -432;
    e.mobs[0].baby = true;
    let s = e.mobs[0].ocelot.as_mut().unwrap();
    s.trusting = true;
    s.crouching = true;
    s.ticks = 8000;
    let mut copy = Mob::new(MobKind::Ocelot, DVec3::ZERO, 0.0);
    copy.apply_look(&e.mobs[0].look());
    assert!(copy.ocelot.as_ref().unwrap().trusting && copy.ocelot.as_ref().unwrap().crouching);
    let mut back = Entities::new(6);
    back.load_nether_mobs(&e.nether_mobs_to_string());
    let m = &back.mobs[0];
    assert!(m.ocelot.as_ref().unwrap().trusting);
    assert_eq!(m.ocelot.as_ref().unwrap().ticks, 0);
    assert_eq!(m.age, -432);
    assert!(!m.ocelot.as_ref().unwrap().crouching);
}
#[test]
fn ocelot_despawn_waits_2400_loaded_ticks_and_trusting_cats_stay() {
    let w = world();
    let mut c = ctx();
    c.players.push(Target::new(PlayerId::HOST, DVec3::new(300.0, 64.0, 0.0), true));
    for trust in [false, true] {
        let mut e = Entities::new(6);
        ocelot(&mut e);
        e.mobs[0].persistent = false;
        e.mobs[0].ocelot.as_mut().unwrap().trusting = trust;
        e.mobs[0].ocelot.as_mut().unwrap().ticks = 2399;
        e.update(0.05, &w, &c);
        assert_eq!(e.mobs.len(), 1);
        e.update(0.05, &w, &c);
        assert_eq!(e.mobs.len(), usize::from(trust));
    }
    let mut w = world();
    w.active = false;
    let mut e = Entities::new(7);
    ocelot(&mut e);
    e.update(1.0, &w, &c);
    assert_eq!(e.mobs[0].ocelot.as_ref().unwrap().ticks, 0);
}
#[test]
fn ocelot_hunts_chickens_and_baby_land_turtles_and_repels_creepers() {
    let w = world();
    let mut e = Entities::new(8);
    ocelot(&mut e);
    e.spawn(MobKind::Chicken, DVec3::new(1.2, 64.0, 0.5));
    for _ in 0..30 {
        e.update(0.05, &w, &ctx());
    }
    assert!(e.mobs.iter().filter(|m| m.kind == MobKind::Chicken).all(|m| m.health < 4.0));
    e.mobs.truncate(1);
    e.mobs[0].ocelot.as_mut().unwrap().prey = None;
    e.spawn(MobKind::Turtle, DVec3::new(1.2, 64.0, 0.5));
    e.ocelot_upkeep(0.5, &ctx());
    assert!(e.mobs[0].ocelot.as_ref().unwrap().prey.is_none());
    e.mobs[1].baby = true;
    e.mobs[1].in_water = true;
    e.ocelot_upkeep(0.5, &ctx());
    assert!(e.mobs[0].ocelot.as_ref().unwrap().prey.is_none());
    e.mobs[1].in_water = false;
    e.ocelot_upkeep(0.5, &ctx());
    assert!(e.mobs[0].ocelot.as_ref().unwrap().prey.is_some());
    let mut e = Entities::new(9);
    e.spawn(MobKind::Creeper, DVec3::new(3.5, 64.0, 0.5));
    ocelot(&mut e);
    let mut avoids = false;
    for _ in 0..80 {
        e.update(0.05, &w, &ctx());
        avoids |= e.mobs[0]
            .goals
            .running
            .is_some_and(|i| matches!(goals::table(MobKind::Creeper)[i as usize].goal, Goal::Avoid(_)));
    }
    assert!(avoids);
}
#[test]
fn ocelot_authoritative_fish_use_consumes_survival_only() {
    let w = World::new_headless(std::sync::Arc::new(Generator::new(42)), Default::default(), 2);
    let mut e = Entities::new(10);
    ocelot(&mut e);
    e.mobs[0].pos = DVec3::new(2.5, 150.0, 0.5);
    e.mobs[0].goals.running = Some(0);
    let mut player = crate::player::Player::new(DVec3::new(0.5, 150.0, 0.5));
    player.yaw = 0.0;
    player.pitch = ((150.4 - player.eye().y) / 2.0).atan() as f32;
    let mut inv = Inventory::default();
    inv.slots[0] = Some(Stack::new(Item::COD, 4));
    for creative in [false, true] {
        assert!(crate::survival_items::use_animal_target(
            &player,
            &mut inv,
            0,
            creative,
            &w,
            &mut e,
            PlayerId::HOST,
            crate::survival_items::Reach { block: 4.5, entity: 3.0 }
        ));
        assert_eq!(inv.get(0).unwrap().count, 3);
    }
    assert!(e.mobs[0].animal.as_ref().unwrap().owner.is_none());
}
