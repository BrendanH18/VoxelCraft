use super::*;
use crate::{
    entity::{
        PlayerId, Target,
        category::MobCategory,
        mob::goals::{self, Avoided, Goal},
    },
    inventory::{Inventory, Stack},
    physics::{BlockSource, test_util::Grid},
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
    ticking: bool,
    light: u8,
    biome: Biome,
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
        self.ticking
    }
    fn surface(&self, _: i32, _: i32) -> Option<i32> {
        Some(0)
    }
    fn sky_light_level(&self, _: IVec3) -> Option<u8> {
        Some(self.light)
    }
    fn biome(&self, _: i32, _: i32) -> Biome {
        self.biome
    }
}
fn world() -> Habitat {
    Habitat { grid: Grid::flat(1), ticking: true, light: 15, biome: Biome::Savanna }
}
fn ctx() -> Ctx {
    Ctx {
        players: vec![Target::new(PlayerId::HOST, DVec3::new(3.5, 1.0, 0.5), true)],
        daylight: 1.0,
        spawn_light: SpawnLight::DAY,
        spawning: false,
        raining: false,
        dimension: Dimension::Overworld,
    }
}
fn arm(e: &mut Entities) {
    e.spawn(MobKind::Armadillo, DVec3::new(0.5, 1.0, 0.5));
    e.ensure_mob_ids();
    let m = e.mobs.last_mut().unwrap();
    m.on_ground = true;
    m.armadillo.as_mut().unwrap().sense = 0;
}
fn tick(e: &mut Entities, w: &Habitat, c: &Ctx, n: u32) {
    for _ in 0..n {
        e.armadillo_upkeep(0.05, w, c, &mut Vec::new());
    }
}
#[test]
fn armadillo_attributes_spawn_weights_food_and_baby_scale_match_the_reference() {
    let facts: Value = serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/armadillos.json")).unwrap();
    let mut e = Entities::new(1);
    arm(&mut e);
    let m = &mut e.mobs[0];
    assert_eq!(m.health, facts["constants"]["armadillo_health"].as_f64().unwrap() as f32);
    assert_eq!(m.shape(), Shape::new(0.35, 0.65));
    assert_eq!(m.kind.category(), MobCategory::Creature);
    assert!(m.kind.leashable() && m.kind.breeding_food(Item::SPIDER_EYE));
    assert!(!m.kind.breeding_food(Item::WHEAT));
    assert!((super::super::path::java_speed(m.goal_speed(1.0)) - 0.14).abs() < 1e-8);
    m.baby = true;
    m.age = -24000;
    assert!((m.shape().height - 0.39).abs() < 1e-8);
    assert_eq!(e.use_animal(0, Some(Stack::new(Item::SPIDER_EYE, 1)), PlayerId::HOST), Some(true));
    assert_eq!(e.mobs[0].age, -21600);
    e.mobs[0].age = -1;
    e.update(0.05, &world(), &ctx());
    assert!(!e.mobs[0].baby);
    for biome in [
        Biome::Savanna,
        Biome::SavannaPlateau,
        Biome::WindsweptSavanna,
        Biome::Badlands,
        Biome::WoodedBadlands,
        Biome::ErodedBadlands,
    ] {
        let entries = super::super::spawn_lists::biome_list(biome, MobCategory::Creature);
        let row = entries.iter().find(|r| r.mob.kind() == Some(MobKind::Armadillo)).unwrap();
        let bad = matches!(biome, Biome::Badlands | Biome::WoodedBadlands | Biome::ErodedBadlands);
        assert_eq!((row.weight, row.min, row.max), if bad { (6, 1, 2) } else { (10, 2, 3) });
    }
}
#[test]
fn armadillo_threats_include_sprinting_and_riding_creative_players_and_undead_through_walls() {
    let mut w = world();
    w.grid.set(IVec3::new(1, 1, 0), Block::STONE);
    let mut e = Entities::new(2);
    arm(&mut e);
    let mut c = ctx();
    tick(&mut e, &w, &c, 20);
    assert!(!e.mobs[0].armadillo.as_ref().unwrap().scared());
    c.players[0].sprinting = true;
    c.players[0].targetable = false;
    tick(&mut e, &w, &c, 5);
    assert!(e.mobs[0].armadillo.as_ref().unwrap().scared());
    c.players[0].sprinting = false;
    c.players[0].pos.x = 100.0;
    tick(&mut e, &w, &c, 80);
    assert!(!e.mobs[0].armadillo.as_ref().unwrap().scared());
    c.players[0].pos.x = 3.5;
    c.players[0].passenger = true;
    tick(&mut e, &w, &c, 5);
    assert!(e.mobs[0].armadillo.as_ref().unwrap().scared());
    c.players[0].spectator = true;
    tick(&mut e, &w, &c, 80);
    assert!(!e.mobs[0].armadillo.as_ref().unwrap().scared());
    e.spawn(MobKind::Zombie, DVec3::new(4.5, 1.0, 0.5));
    tick(&mut e, &w, &c, 5);
    assert!(e.mobs[0].armadillo.as_ref().unwrap().scared());
}
#[test]
fn armadillo_shell_roll_peek_unroll_and_danger_refresh_use_reference_thresholds() {
    let w = world();
    let mut e = Entities::new(3);
    arm(&mut e);
    let mut c = ctx();
    c.players[0].sprinting = true;
    tick(&mut e, &w, &c, 1);
    assert_eq!(e.mobs[0].armadillo.as_ref().unwrap().pose, ROLLING);
    {
        let s = e.mobs[0].armadillo.as_mut().unwrap();
        s.pose_ticks = 5;
    }
    tick(&mut e, &w, &c, 1);
    assert!(e.mobs[0].armadillo.as_ref().unwrap().hide());
    {
        let s = e.mobs[0].armadillo.as_mut().unwrap();
        s.pose_ticks = 10;
    }
    tick(&mut e, &w, &c, 1);
    assert_eq!(e.mobs[0].armadillo.as_ref().unwrap().pose, SCARED);
    {
        let s = e.mobs[0].armadillo.as_mut().unwrap();
        s.peek_in = 1;
        s.danger = 80;
        s.danger_was_around = true;
    }
    tick(&mut e, &w, &c, 1);
    let s = e.mobs[0].armadillo.as_ref().unwrap();
    assert_eq!(s.peeking, 50);
    assert!(!s.hide());
    assert!((150..=450).contains(&s.peek_in));
    tick(&mut e, &w, &c, 200);
    assert!(e.mobs[0].armadillo.as_ref().unwrap().scared());
    c.players[0].sprinting = false;
    tick(&mut e, &w, &c, 60);
    assert_eq!(e.mobs[0].armadillo.as_ref().unwrap().pose, UNROLLING);
    tick(&mut e, &w, &c, 30);
    assert_eq!(e.mobs[0].armadillo.as_ref().unwrap().pose, IDLE);
}
#[test]
fn armadillo_hits_cancel_love_shells_reduce_damage_and_environmental_panic_unrolls() {
    let w = world();
    let mut c = ctx();
    c.players[0].targetable = false;
    let mut e = Entities::new(4);
    arm(&mut e);
    e.mobs[0].animal.as_mut().unwrap().love = 30.0;
    e.hurt_by_player_target(0, PlayerId::HOST);
    e.resolve_hurt_by(&c);
    assert!(e.mobs[0].armadillo.as_ref().unwrap().scared());
    assert_eq!(e.mobs[0].animal.as_ref().unwrap().love, 0.0);
    let before = e.mobs[0].vel;
    e.mobs[0].damage(1.0, Some(DVec3::X * 100.0), &mut e.rng);
    assert_eq!((e.mobs[0].health, e.mobs[0].vel), (12.0, before));
    e.mobs[0].damage(5.0, Some(DVec3::X), &mut e.rng);
    assert_eq!(e.mobs[0].health, 10.0);
    e.mobs[0].damage(4.0, None, &mut e.rng);
    assert_eq!(e.mobs[0].health, 8.5);
    tick(&mut e, &w, &c, 1);
    assert_eq!(e.mobs[0].armadillo.as_ref().unwrap().pose, IDLE);
}
#[test]
fn armadillo_scutes_wait_for_adulthood_and_mob_drops_but_brushing_is_independent() {
    let w = world();
    let c = ctx();
    let mut e = Entities::new(5);
    arm(&mut e);
    e.mob_loot = false;
    e.mobs[0].armadillo.as_mut().unwrap().scute_time = 1;
    tick(&mut e, &w, &c, 3);
    assert!(e.items.is_empty());
    assert_eq!(e.mobs[0].armadillo.as_ref().unwrap().scute_time, -2);
    e.mob_loot = true;
    tick(&mut e, &w, &c, 1);
    assert_eq!(e.items[0].stack, Stack::new(Item::ARMADILLO_SCUTE, 1));
    let timer = e.mobs[0].armadillo.as_ref().unwrap().scute_time;
    assert!((6000..12000).contains(&timer));
    e.mob_loot = false;
    assert!(e.brush_armadillo(0));
    assert_eq!(e.mobs[0].armadillo.as_ref().unwrap().scute_time, timer);
    e.mobs[0].baby = true;
    e.mobs[0].age = -100;
    e.mobs[0].armadillo.as_mut().unwrap().scute_time = 1;
    e.items.clear();
    assert!(!e.brush_armadillo(0));
    tick(&mut e, &w, &c, 3);
    assert!(e.items.is_empty());
    e.mob_loot = true;
    e.mobs[0].baby = false;
    e.mobs[0].age = 0;
    tick(&mut e, &w, &c, 1);
    assert_eq!(e.items.len(), 1);
}
#[test]
fn armadillo_save_preserves_scutes_and_danger_while_pose_age_restarts_and_look_replicates() {
    let mut e = Entities::new(6);
    arm(&mut e);
    e.mobs[0].baby = true;
    e.mobs[0].age = -123;
    let s = e.mobs[0].armadillo.as_mut().unwrap();
    s.pose = SCARED;
    s.pose_ticks = 47;
    s.danger = 73;
    s.scute_time = -14;
    s.peeking = 31;
    let look = e.mobs[0].look();
    let mut copy = Mob::new(MobKind::Armadillo, DVec3::ZERO, 0.0);
    copy.apply_look(&look);
    assert_eq!(copy.armadillo.as_ref().unwrap().look(), e.mobs[0].armadillo.as_ref().unwrap().look());
    let mut back = Entities::new(7);
    back.load_nether_mobs(&e.nether_mobs_to_string());
    let m = &back.mobs[0];
    assert_eq!(m.age, -123);
    let s = m.armadillo.as_ref().unwrap();
    assert_eq!((s.pose, s.pose_ticks, s.danger, s.scute_time, s.peeking), (SCARED, 0, 73, -14, 0));
}
#[test]
fn armadillo_spawn_floor_accepts_badlands_soil_but_requires_animal_brightness() {
    let mut w = world();
    let rules =
        super::super::monster_spawns::SpawnRules { dimension: Dimension::Overworld, moon: 1.0, light: SpawnLight::DAY };
    let mut rng = Rng::new(6);
    for name in ["grass", "red sand", "coarse dirt", "terracotta", "orange terracotta"] {
        let b = Block::from_name(name).unwrap();
        w.grid.set(IVec3::ZERO, b);
        assert!(super::super::spawn_rules::spot_ok(&w, MobKind::Armadillo, IVec3::Y, rules, &mut rng), "{name}");
    }
    w.light = 0;
    assert!(!super::super::spawn_rules::spot_ok(&w, MobKind::Armadillo, IVec3::Y, rules, &mut rng));
}
#[test]
fn armadillo_repels_both_spider_kinds_on_the_shared_avoid_selector() {
    for kind in [MobKind::Spider, MobKind::CaveSpider] {
        let w = world();
        let mut c = ctx();
        c.players[0].targetable = false;
        let mut e = Entities::new(54);
        e.spawn(kind, DVec3::new(3.5, 1.0, 0.5));
        arm(&mut e);
        e.mobs[0].on_ground = true;
        let mut avoiding = false;
        for _ in 0..100 {
            e.update(0.05, &w, &c);
            avoiding |= e.mobs[0].goals.running.is_some_and(
                |i| matches!(goals::table(kind)[i as usize].goal,Goal::Avoid(a) if a.what==Avoided::Armadillo),
            );
        }
        assert!(avoiding, "{kind:?}");
    }
}
#[test]
fn armadillo_brush_recipe_and_authoritative_use_wear_four_times_and_do_not_add_archaeology() {
    let mut g = crate::crafting::Grid::new(3);
    for (n, item) in [(0, Item::FEATHER), (3, Item::COPPER_INGOT), (6, Item::STICK)] {
        g.cells[n] = Some(Stack::new(item, 1));
    }
    assert_eq!(g.result(), Some(Stack::new(Item::BRUSH, 1)));
    assert!(Item::BRUSH.block().is_none() && Item::BRUSH.as_tool().is_none());
    assert_eq!(Item::BRUSH.durability(), Some(64));
    let w = World::new_headless(std::sync::Arc::new(Generator::new(42)), Default::default(), 2);
    let mut e = Entities::new(7);
    e.spawn(MobKind::Armadillo, DVec3::new(2.5, 150.0, 0.5));
    let mut player = crate::player::Player::new(DVec3::new(0.5, 150.0, 0.5));
    player.yaw = 0.0;
    player.pitch = ((150.3 - player.eye().y) / 2.0).atan() as f32;
    let mut inv = Inventory::default();
    inv.slots[0] = Some(Stack::new(Item::BRUSH, 1));
    let reach = crate::survival_items::Reach { block: 4.5, entity: 3.0 };
    for n in 1..=4 {
        assert!(crate::survival_items::use_animal_target(
            &player,
            &mut inv,
            0,
            false,
            &w,
            &mut e,
            PlayerId::HOST,
            reach
        ));
        assert_eq!(inv.get(0).map(|s| s.damage), if n == 4 { None } else { Some(n * 16) });
    }
    assert_eq!(e.items.iter().map(|i| i.stack.count as u32).sum::<u32>(), 4);
    inv.slots[0] = Some(Stack::new(Item::BRUSH, 1));
    assert!(crate::survival_items::use_animal_target(&player, &mut inv, 0, true, &w, &mut e, PlayerId::HOST, reach));
    assert_eq!(inv.get(0).unwrap().damage, 0);
}

#[test]
fn armadillo_spider_avoidance_only_selects_idle_shells_at_priority_two() {
    for kind in [MobKind::Spider, MobKind::CaveSpider] {
        assert_eq!(goals::table(kind)[0].priority, 2);
        for pose in [IDLE, ROLLING, SCARED, UNROLLING] {
            let mut e = Entities::new(64);
            e.spawn(kind, DVec3::new(3.5, 1.0, 0.5));
            arm(&mut e);
            e.mobs[1].armadillo.as_mut().unwrap().pose = pose;
            arm(&mut e);
            e.mobs[2].pos.x = 7.5;
            let expected = if pose == IDLE { e.mobs[1].uid } else { e.mobs[2].uid };
            e.sense_goals(&world());
            assert_eq!(e.mobs[0].goals.seen[0].map(|(id, _)| id), Some(expected));
            e.mobs.pop();
            e.sense_goals(&world());
            assert_eq!(e.mobs[0].goals.seen[0].is_some(), pose == IDLE);
        }
    }
}
