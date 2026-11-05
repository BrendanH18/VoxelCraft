use super::*;
use crate::{
    entity::{Target, monster_spawns::PackGroup},
    physics::{BlockSource, test_util::Grid},
    simulation::lighting::SpawnLight,
    world::terrain::{Biome, Dimension},
};
struct Habitat(Grid);
impl BlockSource for Habitat {
    fn block(&self, p: IVec3) -> Option<Block> {
        self.0.block(p)
    }
}
impl MobWorld for Habitat {
    fn exposed(&self, _: IVec3) -> bool {
        true
    }
    fn loaded(&self, _: IVec3) -> bool {
        true
    }
    fn surface(&self, _: i32, _: i32) -> Option<i32> {
        Some(0)
    }
    fn biome(&self, _: i32, _: i32) -> Biome {
        Biome::BambooJungle
    }
    fn sky_light_level(&self, _: IVec3) -> Option<u8> {
        Some(15)
    }
}
fn world() -> Habitat {
    Habitat(Grid::flat(1))
}
fn ctx() -> Ctx {
    Ctx {
        players: vec![Target::new(PlayerId::HOST, DVec3::new(1.5, 1.0, 0.5), true)],
        daylight: 1.0,
        spawn_light: SpawnLight::DAY,
        spawning: false,
        raining: false,
        dimension: Dimension::Overworld,
    }
}
fn panda(e: &mut Entities, at: DVec3) {
    e.spawn(MobKind::Panda, at);
    e.ensure_mob_ids();
    e.mobs.last_mut().unwrap().on_ground = true;
}
fn tick(e: &mut Entities, w: &Habitat, c: &Ctx, n: u32) {
    for _ in 0..n {
        e.panda_upkeep(0.05, w, c, &mut Vec::new());
    }
}
const BAMBOO: Item = Item::from_block(o::BAMBOO);
#[test]
fn panda_genes_use_recessive_expression_and_the_java_weighted_pool() {
    let facts: Value = serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/pandas.json")).unwrap();
    let mut counts = [0; 7];
    for n in 0..16 {
        counts[Gene::from_roll(n) as usize] += 1;
    }
    for main in Gene::ALL {
        assert_eq!(counts[main as usize], facts["constants"]["gene_weights"][main.name()].as_u64().unwrap());
        for hidden in Gene::ALL {
            let s = State { main, hidden, ..Default::default() };
            let expected = if matches!(main, Gene::Brown | Gene::Weak) && main != hidden { Gene::Normal } else { main };
            assert_eq!(s.variant(), expected);
        }
    }
    let mut m = Mob::new(MobKind::Panda, DVec3::ZERO, 0.0);
    assert_eq!((m.max_health(), m.shape()), (20.0, Shape::new(0.65, 1.25)));
    m.panda.as_mut().unwrap().main = Gene::Weak;
    assert_eq!(m.max_health(), 20.0);
    m.panda.as_mut().unwrap().hidden = Gene::Weak;
    assert_eq!(m.max_health(), 10.0);
    m.panda.as_mut().unwrap().main = Gene::Lazy;
    assert!((crate::entity::path::java_speed(m.goal_speed(1.0)) - 0.07).abs() < 1e-6);
    m.baby = true;
    m.age = -24000;
    assert_eq!(m.shape(), Shape::new(0.325, 0.625));
    assert!(!m.kind.leashable());
}
#[test]
fn panda_children_inherit_one_allele_from_each_parent_with_mutation_and_random_order() {
    let mut e = Entities::new(501);
    panda(&mut e, DVec3::ZERO);
    panda(&mut e, DVec3::X);
    {
        let a = e.mobs[0].panda.as_mut().unwrap();
        a.main = Gene::Lazy;
        a.hidden = Gene::Playful;
        let b = e.mobs[1].panda.as_mut().unwrap();
        b.main = Gene::Brown;
        b.hidden = Gene::Weak;
    }
    let (mut ordinary, mut first_parent_main, mut mutations) = (0, 0, 0);
    for _ in 0..4000 {
        let c = e.animal_child(0, 1);
        let s = c.panda.as_ref().unwrap();
        let a = |g| matches!(g, Gene::Lazy | Gene::Playful);
        let b = |g| matches!(g, Gene::Brown | Gene::Weak);
        if a(s.main) && b(s.hidden) || b(s.main) && a(s.hidden) {
            ordinary += 1;
        } else {
            mutations += 1;
        }
        first_parent_main += a(s.main) as usize;
        assert!(c.baby && c.age == -24000);
        assert_eq!(c.health, c.max_health());
    }
    assert!((3750..4000).contains(&ordinary));
    assert!((50..250).contains(&mutations));
    assert!((1800..2200).contains(&first_parent_main));
}
#[test]
fn panda_genes_age_and_held_food_survive_save_and_replication_but_actions_restart() {
    let mut e = Entities::new(63);
    for main in Gene::ALL {
        panda(&mut e, DVec3::new(main as u8 as f64, 1.0, 0.0));
        let m = e.mobs.last_mut().unwrap();
        let s = m.panda.as_mut().unwrap();
        s.main = main;
        s.hidden = main;
        s.sitting = true;
        s.rolling = 12;
        s.scared = true;
        m.animal.as_mut().unwrap().mouth = Some(Stack::new(Item::CAKE, 1));
        m.baby = true;
        m.age = -37;
        m.health = 7.0;
    }
    let mut back = Entities::new(64);
    back.load_nether_mobs(&e.nether_mobs_to_string());
    assert_eq!(back.mobs.len(), 7);
    for (before, after) in e.mobs.iter().zip(&back.mobs) {
        assert_eq!(before.panda.as_ref().unwrap().save(), after.panda.as_ref().unwrap().save());
        assert_eq!((after.age, after.health), (-37, 7.0));
        assert_eq!(after.animal.as_ref().unwrap().mouth, Some(Stack::new(Item::CAKE, 1)));
        assert!(!after.panda.as_ref().unwrap().sitting);
        assert_eq!(after.panda.as_ref().unwrap().rolling, 0);
        let mut replica = Mob::new(MobKind::Panda, DVec3::ZERO, 0.0);
        replica.apply_look(&before.look());
        assert_eq!(replica.panda.as_ref().unwrap().look(), before.panda.as_ref().unwrap().look());
    }
    back.mobs[0].age = -1;
    back.mobs[0].advance_animal_age(0.05);
    assert!(!back.mobs[0].baby);
}
#[test]
fn panda_breeding_needs_one_bamboo_in_the_reference_box_and_reports_unhappiness() {
    for (offset, allowed) in [
        (IVec3::new(7, 2, -7), true),
        (IVec3::new(8, 0, 0), false),
        (IVec3::new(0, -1, 0), false),
        (IVec3::new(0, 3, 0), false),
    ] {
        let mut w = world();
        w.0.set(IVec3::new(0, 1, 0) + offset, o::BAMBOO_LARGE_LEAVES);
        let mut m = Mob::new(MobKind::Panda, DVec3::new(0.5, 1.0, 0.5), 0.0);
        assert_eq!(m.panda_breed_allowed(&w), allowed);
        assert_eq!(m.panda.as_ref().unwrap().unhappy, if allowed { 0 } else { 32 });
    }
    let mut e = Entities::new(13);
    let mut w = world();
    panda(&mut e, DVec3::new(0.5, 1.0, 0.5));
    panda(&mut e, DVec3::new(2.5, 1.0, 0.5));
    for i in 0..2 {
        assert_eq!(e.use_animal(i, Some(Stack::new(BAMBOO, 1)), PlayerId::HOST), Some(true));
    }
    for _ in 0..100 {
        e.update(0.05, &w, &ctx());
    }
    assert_eq!(e.count(MobKind::Panda), 2);
    assert!(e.mobs.iter().all(|m| m.animal.as_ref().unwrap().love > 0.0));
    for (i, m) in e.mobs.iter_mut().enumerate() {
        m.pos = DVec3::new(0.5 + 2.0 * i as f64, 1.0, 0.5);
        m.vel = DVec3::ZERO;
        m.on_ground = true;
        m.ai = super::super::mob::Ai::Idle;
        m.stop_navigation();
    }
    w.0.set(IVec3::new(1, 1, 2), o::BAMBOO);
    for _ in 0..250 {
        e.update(0.05, &w, &ctx());
    }
    assert_eq!(
        e.count(MobKind::Panda),
        3,
        "{:?}",
        e.mobs
            .iter()
            .map(|m| (
                m.pos,
                m.age,
                m.animal.as_ref().map(|a| (a.love, a.courtship, a.partner)),
                m.panda.as_ref().map(|s| s.unhappy),
                m.goals.running
            ))
            .collect::<Vec<_>>()
    );
    assert!(e.mobs.iter().any(|m| m.baby));
    assert!(e.mobs.iter().filter(|m| !m.baby).all(|m| m.age > 0));
}
#[test]
fn panda_feeding_distinguishes_growth_love_snacking_and_creative_replacement() {
    let mut e = Entities::new(44);
    panda(&mut e, DVec3::new(0.5, 1.0, 0.5));
    assert_eq!(e.use_animal(0, Some(Stack::new(Item::CAKE, 1)), PlayerId::HOST), None);
    assert_eq!(e.use_animal(0, Some(Stack::new(BAMBOO, 1)), PlayerId::HOST), Some(true));
    assert_eq!(e.mobs[0].animal.as_ref().unwrap().love, 30.0);
    assert!(!e.mobs[0].panda.as_ref().unwrap().sitting);
    assert_eq!(e.use_animal(0, Some(Stack::new(BAMBOO, 1)), PlayerId::HOST), Some(true));
    assert!(e.mobs[0].panda.as_ref().unwrap().sitting);
    assert_eq!(e.mobs[0].animal.as_ref().unwrap().mouth, Some(Stack::new(BAMBOO, 1)));
    e.mobs[0].baby = true;
    e.mobs[0].age = -24000;
    assert_eq!(e.use_animal(0, Some(Stack::new(BAMBOO, 1)), PlayerId::HOST), Some(true));
    assert_eq!(e.mobs[0].age, -21600);
    e.mobs[0].baby = false;
    e.mobs[0].age = 100;
    e.mobs[0].panda.as_mut().unwrap().sitting = false;
    e.mobs[0].animal.as_mut().unwrap().mouth = Some(Stack::new(Item::CAKE, 1));
    assert_eq!(e.use_animal_with_materials(0, Some(Stack::new(BAMBOO, 1)), PlayerId::HOST, true), Some(true));
    assert!(e.items.is_empty());
    e.mobs[0].panda.as_mut().unwrap().sitting = false;
    assert_eq!(e.use_animal(0, Some(Stack::new(BAMBOO, 1)), PlayerId::HOST), Some(true));
    assert_eq!(e.items.len(), 1);
    e.mobs[0].panda.as_mut().unwrap().on_back = true;
    assert_eq!(e.use_animal(0, None, PlayerId::HOST), Some(false));
    assert!(!e.mobs[0].panda.as_ref().unwrap().on_back);
}
#[test]
fn panda_sneeze_is_a_rare_slime_gift_and_startles_active_adults() {
    let w = world();
    let c = ctx();
    let mut e = Entities::new(700);
    panda(&mut e, DVec3::new(0.5, 1.0, 0.5));
    panda(&mut e, DVec3::new(3.5, 1.0, 0.5));
    e.mobs[0].baby = true;
    e.mobs[0].age = -24000;
    e.mobs[0].panda.as_mut().unwrap().sneezing = 1;
    tick(&mut e, &w, &c, 20);
    assert_eq!(e.mobs[1].vel.y, 0.0);
    tick(&mut e, &w, &c, 1);
    assert_eq!(e.mobs[1].vel.y, 8.4);
    for _ in 0..6000 {
        let s = e.mobs[0].panda.as_mut().unwrap();
        s.sneezing = 21;
        s.on_back = true;
        tick(&mut e, &w, &c, 1);
    }
    let count = e.items.iter().filter(|i| i.stack.item == Item::SLIME_BALL).count();
    assert!((2..25).contains(&count), "{count}");
    e.mob_loot = false;
    e.items.clear();
    for _ in 0..1000 {
        e.mobs[0].panda.as_mut().unwrap().sneezing = 21;
        tick(&mut e, &w, &c, 1);
    }
    assert!(e.items.is_empty());
}
#[test]
fn pandas_retaliate_once_but_aggressive_relatives_keep_fighting_and_accept_bamboo() {
    let w = world();
    let c = ctx();
    let mut e = Entities::new(97);
    for x in [0.5, 4.5, 6.5] {
        panda(&mut e, DVec3::new(x, 1.0, 0.5));
    }
    e.mobs[2].panda.as_mut().unwrap().main = Gene::Aggressive;
    e.panda_hurt(0, Foe::Player(PlayerId::HOST));
    tick(&mut e, &w, &c, 1);
    assert!(e.mobs[0].panda.as_ref().unwrap().target.is_some());
    assert!(e.mobs[1].panda.as_ref().unwrap().target.is_none());
    assert!(e.mobs[2].panda.as_ref().unwrap().target.is_some());
    let mut hits = Vec::new();
    e.mobs[0].panda_think(&w, &mut e.rng, &mut hits);
    assert!(hits.iter().any(|h| matches!(h, super::super::EntityEvent::PlayerHit { damage: 6.0, .. })));
    tick(&mut e, &w, &c, 1);
    assert!(e.mobs[0].panda.as_ref().unwrap().target.is_none());
    assert!(e.mobs[2].panda.as_ref().unwrap().target.is_some());
    assert_eq!(e.use_animal(2, Some(Stack::new(BAMBOO, 1)), PlayerId::HOST), Some(true));
    tick(&mut e, &w, &c, 1);
    assert!(e.mobs[2].panda.as_ref().unwrap().target.is_none());
}
#[test]
fn worried_pandas_use_thunder_instead_of_rain_and_snacks_can_be_interrupted() {
    let w = world();
    let mut c = ctx();
    c.raining = true;
    let mut e = Entities::new(3);
    panda(&mut e, DVec3::new(0.5, 1.0, 0.5));
    e.mobs[0].panda.as_mut().unwrap().main = Gene::Worried;
    tick(&mut e, &w, &c, 1);
    assert!(!e.mobs[0].panda.as_ref().unwrap().scared);
    e.thundering = true;
    tick(&mut e, &w, &c, 1);
    assert!(e.mobs[0].panda.as_ref().unwrap().sitting);
    assert_eq!(e.use_animal(0, Some(Stack::new(BAMBOO, 1)), PlayerId::HOST), None);
    e.thundering = false;
    tick(&mut e, &w, &c, 1);
    assert!(!e.mobs[0].panda.as_ref().unwrap().sitting);
    e.mobs[0].panda.as_mut().unwrap().sitting = true;
    e.mobs[0].damage(1.0, Some(DVec3::X), &mut e.rng);
    assert!(!e.mobs[0].panda.as_ref().unwrap().sitting);
    assert_ne!(e.mobs[0].ai, super::super::mob::Ai::Panic);
}
#[test]
fn panda_ground_food_keeps_the_whole_stack_and_obeys_mob_griefing() {
    let w = world();
    let c = ctx();
    let mut e = Entities::new(15);
    panda(&mut e, DVec3::new(0.5, 1.0, 0.5));
    let h = super::super::item::ItemEntity::new(Stack::new(BAMBOO, 16), e.mobs[0].pos, DVec3::ZERO, 0.0, &mut e.rng);
    e.items.push(h);
    e.villager_griefing = false;
    tick(&mut e, &w, &c, 1);
    assert_eq!(e.items.len(), 1);
    e.villager_griefing = true;
    tick(&mut e, &w, &c, 1);
    assert!(e.items.is_empty());
    assert_eq!(e.mobs[0].animal.as_ref().unwrap().mouth, Some(Stack::new(BAMBOO, 16)));
    tick(&mut e, &w, &c, 3000);
    assert!(e.mobs[0].animal.as_ref().unwrap().mouth.is_none());
    e.items.push(super::super::item::ItemEntity::new(
        Stack::new(Item::CAKE, 1),
        e.mobs[0].pos,
        DVec3::ZERO,
        0.0,
        &mut e.rng,
    ));
    tick(&mut e, &w, &c, 1);
    assert_eq!(e.mobs[0].animal.as_ref().unwrap().mouth, Some(Stack::new(Item::CAKE, 1)));
}
#[test]
fn panda_natural_packs_spawning_and_bamboo_loot_match_the_pinned_data() {
    let mut e = Entities::new(45);
    let mut group = PackGroup::default();
    let mut babies = 0;
    for n in 0..1000 {
        e.spawn_natural(MobKind::Panda, DVec3::new(0.5, 1.0, 0.5), Biome::BambooJungle, &mut group);
        babies += e.mobs.last().unwrap().baby as usize;
        if n == 0 {
            assert!(!e.mobs[0].baby);
        }
    }
    assert!((150..250).contains(&babies));
    assert!(e.mobs.iter().any(|m| m.panda.as_ref().unwrap().variant() == Gene::Brown));
    for level in [0, 3] {
        assert_eq!(MobKind::Panda.drops(&mut e.rng, level), vec![(BAMBOO, 1)]);
    }
    let mut w = world();
    w.0.set(IVec3::new(0, 0, 0), Block::GRASS);
    let rules =
        super::super::monster_spawns::SpawnRules { dimension: Dimension::Overworld, moon: 1.0, light: SpawnLight::DAY };
    assert!(super::super::spawn_rules::spot_ok(&w, MobKind::Panda, IVec3::new(0, 1, 0), rules, &mut e.rng));
}

#[test]
fn panda_roll_travel_keeps_its_launch_and_lethal_falls_drop_once() {
    let w = world();
    let mut m = Mob::new(MobKind::Panda, DVec3::new(0.5, 1.0, 0.5), 0.0);
    m.on_ground = true;
    m.panda.as_mut().unwrap().rolling = 1;
    m.vel = DVec3::new(4.0, 5.4, 0.0);
    m.panda_land_tick(&w, None, 0.0, &mut Rng::new(8));
    assert!((m.pos.x - 0.7).abs() < 1e-8);
    assert!((m.pos.y - 1.27).abs() < 1e-8);
    assert!((m.vel.x - 2.184).abs() < 1e-8);
    assert!((m.vel.y - 3.724).abs() < 1e-8);
    let mut e = Entities::new(8);
    panda(&mut e, DVec3::new(0.5, 1.0, 0.5));
    e.mobs[0].panda.as_mut().unwrap().rolling = 1;
    tick(&mut e, &w, &ctx(), 32);
    assert_eq!(e.mobs[0].panda.as_ref().unwrap().rolling, 0);
    let mut e = Entities::new(77);
    panda(&mut e, DVec3::new(0.5, 40.0, 0.5));
    e.mobs[0].on_ground = false;
    e.mobs[0].health = 1.0;
    let mut falling = Mob::new(MobKind::Panda, DVec3::new(0.5, 40.0, 0.5), 0.0);
    falling.health = 1.0;
    falling.on_ground = false;
    let mut deaths = 0;
    let mut rng = Rng::new(77);
    for _ in 0..100 {
        let mut events = Vec::new();
        falling.update(0.05, &w, &ctx(), &mut rng, &mut events);
        deaths += events
            .iter()
            .filter(|v| matches!(v, super::super::EntityEvent::MobKilled { kind: MobKind::Panda, .. }))
            .count();
        e.update(0.05, &w, &ctx());
    }
    assert_eq!(deaths, 1);
    assert_eq!(e.count(MobKind::Panda), 0);
    assert_eq!(e.items.iter().filter(|h| h.stack.item == BAMBOO).map(|h| h.stack.count as u32).sum::<u32>(), 1);
}
