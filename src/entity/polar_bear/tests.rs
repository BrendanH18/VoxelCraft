use super::*;
use crate::{
    entity::{Mob, Target, monster_spawns::PackGroup},
    inventory::Stack,
    item::Item,
    physics::{BlockSource, test_util::Grid},
    simulation::lighting::SpawnLight,
    world::{
        block::Block,
        terrain::{Biome, Dimension},
    },
};
use glam::IVec3;
struct Habitat {
    grid: Grid,
    biome: Biome,
    light: u8,
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
    fn surface(&self, _: i32, _: i32) -> Option<i32> {
        Some(0)
    }
    fn exposed(&self, _: IVec3) -> bool {
        self.light > 0
    }
    fn sky_light_level(&self, _: IVec3) -> Option<u8> {
        Some(self.light)
    }
    fn biome(&self, _: i32, _: i32) -> Biome {
        self.biome
    }
}
fn world() -> Habitat {
    Habitat { grid: Grid::flat(1), biome: Biome::SnowyPlains, light: 15 }
}
fn ctx() -> Ctx {
    Ctx {
        players: vec![Target::new(PlayerId::HOST, DVec3::new(4.5, 1.0, 0.5), true)],
        daylight: 1.0,
        spawning: false,
        raining: false,
        dimension: Dimension::Overworld,
        spawn_light: SpawnLight::DAY,
    }
}
fn bear(e: &mut Entities, p: DVec3) {
    e.spawn(MobKind::PolarBear, p);
    e.mobs.last_mut().unwrap().on_ground = true;
    e.ensure_mob_ids();
}
fn sense(e: &mut Entities, w: &Habitat, c: &Ctx, n: u32) {
    for _ in 0..n {
        e.spawn_ticks += 1;
        e.polar_sense(0.05, w, c);
    }
}
#[test]
fn polar_bear_attributes_food_and_pack_cubs_match_java() {
    let facts: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/polar_bears.json")).unwrap();
    let mut e = Entities::new(31);
    let mut group = PackGroup::default();
    for n in 0..3 {
        e.spawn_natural(MobKind::PolarBear, DVec3::new(n as f64, 1.0, 0.5), Biome::SnowyPlains, &mut group);
    }
    assert_eq!(e.mobs[0].health, facts["attributes"]["max_health"].as_f64().unwrap() as f32);
    assert!(!e.mobs[0].baby && e.mobs[1..].iter().all(|m| m.baby && m.age == -24000));
    assert_eq!(e.mobs[0].shape(), Shape::new(0.7, 1.4));
    assert_eq!(e.mobs[1].shape(), Shape::new(0.35, 0.7));
    assert!(!MobKind::PolarBear.is_breedable());
    for food in [Item::COD, Item::SALMON, Item::SLIME_BALL, Item::WHEAT] {
        assert!(!MobKind::PolarBear.breeding_food(food));
        assert_eq!(e.use_animal(0, Some(Stack::new(food, 1)), PlayerId::HOST), None);
    }
    e.mobs[1].age = -1;
    let w = world();
    e.update(0.05, &w, &ctx());
    assert!(!e.mobs[1].baby && e.mobs[1].age == 0);
}
#[test]
fn polar_bears_are_neutral_until_a_cub_or_retaliation_supplies_a_target() {
    let w = world();
    let c = ctx();
    let mut e = Entities::new(71);
    bear(&mut e, DVec3::new(0.5, 1.0, 0.5));
    sense(&mut e, &w, &c, 200);
    assert!(e.mobs[0].polar_bear.as_ref().unwrap().current.is_none());
    e.spawn_baby(MobKind::PolarBear, DVec3::new(2.5, 1.0, 0.5));
    e.mobs[1].on_ground = true;
    sense(&mut e, &w, &c, 200);
    assert_eq!(e.mobs[0].polar_bear.as_ref().unwrap().current.unwrap().foe, Foe::Player(PlayerId::HOST));
    assert!(e.mobs[1].polar_bear.as_ref().unwrap().current.is_none());
    e.mobs.truncate(1);
    sense(&mut e, &w, &c, 1);
    assert!(e.mobs[0].polar_bear.as_ref().unwrap().current.is_some());
}
#[test]
fn polar_bear_hurt_goal_alerts_adults_only_when_the_victim_is_a_cub() {
    let w = world();
    let c = ctx();
    let mut e = Entities::new(81);
    bear(&mut e, DVec3::new(0.5, 1.0, 0.5));
    bear(&mut e, DVec3::new(18.5, 1.0, 0.5));
    e.hurt_by_player_target(0, PlayerId::HOST);
    assert!(e.mobs[0].polar_bear.as_ref().unwrap().angry_at.is_none());
    e.resolve_hurt_by(&c);
    sense(&mut e, &w, &c, 1);
    assert!(e.mobs[0].polar_bear.as_ref().unwrap().current.is_some());
    assert!(e.mobs[1].polar_bear.as_ref().unwrap().current.is_none());
    e.spawn_baby(MobKind::PolarBear, DVec3::new(2.5, 1.0, 0.5));
    e.ensure_mob_ids();
    e.polar_hurt(2, Foe::Player(PlayerId::HOST));
    sense(&mut e, &w, &c, 1);
    assert!(e.mobs[..2].iter().all(|m| m.polar_bear.as_ref().unwrap().current.is_some()));
    assert!(e.mobs[2].polar_bear.as_ref().unwrap().current.is_none());
}
#[test]
fn polar_bear_anger_and_age_survive_save_but_the_standing_pose_is_transient() {
    for name in ["polar_bear", "polar bear", "minecraft:polar_bear"] {
        assert_eq!(MobKind::from_name(name), Some(MobKind::PolarBear));
    }
    let mut e = Entities::new(47);
    let w = world();
    let mut c = ctx();
    bear(&mut e, DVec3::new(0.5, 1.0, 0.5));
    e.polar_hurt(0, Foe::Player(PlayerId::HOST));
    sense(&mut e, &w, &c, 1);
    let end = e.mobs[0].polar_bear.as_ref().unwrap().anger_end;
    assert!((401..=781).contains(&end));
    {
        let s = e.mobs[0].polar_bear.as_mut().unwrap();
        s.standing = true;
        s.animation = 6.0;
    }
    let mut replica = Mob::new(MobKind::PolarBear, DVec3::ZERO, 0.0);
    replica.apply_look(&e.mobs[0].look());
    assert!(replica.polar_bear.as_ref().unwrap().standing);
    assert_eq!(replica.selection_shape().height, 2.8);
    let mut loaded = Entities::new(48);
    loaded.load_nether_mobs(&e.nether_mobs_to_string());
    assert_eq!(loaded.mobs[0].polar_bear.as_ref().unwrap().anger_end, end);
    assert_eq!(loaded.mobs[0].polar_bear.as_ref().unwrap().angry_at, Some(Foe::Player(PlayerId::HOST)));
    assert!(!loaded.mobs[0].polar_bear.as_ref().unwrap().standing);
    c.players[0].pos.x = 100.0;
    loaded.spawn_ticks = end as u64 + 1;
    sense(&mut loaded, &w, &c, 1);
    assert!(loaded.mobs[0].polar_bear.as_ref().unwrap().angry_at.is_none());
}
#[test]
fn polar_bear_player_targeting_respects_peaceful_creative_death_and_universal_anger() {
    let w = world();
    let mut c = ctx();
    let mut e = Entities::new(92);
    bear(&mut e, DVec3::new(0.5, 1.0, 0.5));
    e.polar_hurt(0, Foe::Player(PlayerId::HOST));
    e.difficulty = Difficulty::Peaceful;
    sense(&mut e, &w, &c, 1);
    assert!(e.mobs[0].polar_bear.as_ref().unwrap().current.is_none());
    e.difficulty = Difficulty::Normal;
    e.polar_hurt(0, Foe::Player(PlayerId::HOST));
    c.players[0].targetable = false;
    sense(&mut e, &w, &c, 1);
    assert!(e.mobs[0].polar_bear.as_ref().unwrap().angry_at.is_none());
    c.players[0].targetable = true;
    e.polar_hurt(0, Foe::Player(PlayerId::HOST));
    c.players[0].alive = false;
    sense(&mut e, &w, &c, 1);
    assert!(e.mobs[0].polar_bear.as_ref().unwrap().angry_at.is_none());
    e.neutral_forgive_dead_players = false;
    e.polar_hurt(0, Foe::Player(PlayerId::HOST));
    sense(&mut e, &w, &c, 1);
    assert_eq!(e.mobs[0].polar_bear.as_ref().unwrap().angry_at, Some(Foe::Player(PlayerId::HOST)));
    e.neutral_universal_anger = true;
    c.players.push(Target::new(PlayerId(9), DVec3::new(3.5, 1.0, 0.5), true));
    e.polar_hurt(0, Foe::Player(PlayerId::HOST));
    sense(&mut e, &w, &c, 100);
    assert_eq!(e.mobs[0].polar_bear.as_ref().unwrap().current.unwrap().foe, Foe::Player(PlayerId(9)));
}
#[test]
fn polar_bear_spawn_floor_is_ice_only_in_the_two_frozen_oceans() {
    let mut w = world();
    let at = IVec3::new(0, 1, 0);
    let rules =
        super::super::monster_spawns::SpawnRules { dimension: Dimension::Overworld, moon: 1.0, light: SpawnLight::DAY };
    let mut r = Rng::new(1);
    for biome in [Biome::FrozenOcean, Biome::DeepFrozenOcean, Biome::SnowyPlains, Biome::IceSpikes] {
        w.biome = biome;
        for floor in [Block::GRASS, Block::ICE, crate::world::gadgets::PACKED_ICE, Block::SNOW] {
            w.grid.set(IVec3::ZERO, floor);
            let ice = matches!(biome, Biome::FrozenOcean | Biome::DeepFrozenOcean);
            assert_eq!(
                super::super::spawn_rules::spot_ok(&w, MobKind::PolarBear, at, rules, &mut r),
                if ice { floor == Block::ICE } else { floor == Block::GRASS },
                "{biome:?} {floor:?}"
            );
        }
    }
    w.biome = Biome::FrozenOcean;
    w.grid.set(IVec3::ZERO, Block::ICE);
    w.light = 0;
    assert!(!super::super::spawn_rules::spot_ok(&w, MobKind::PolarBear, at, rules, &mut r));
}
#[test]
fn polar_bear_loot_uses_one_three_to_one_fish_pool_and_cubs_drop_nothing() {
    let mut rng = Rng::new(55);
    let (mut cod, mut salmon) = (0, 0);
    for _ in 0..6000 {
        for (item, n) in MobKind::PolarBear.drops(&mut rng, 0) {
            assert!((1..=2).contains(&n));
            if item == Item::COD {
                cod += 1
            } else {
                assert_eq!(item, Item::SALMON);
                salmon += 1
            }
        }
    }
    let ratio = cod as f32 / (cod + salmon) as f32;
    assert!((0.70..0.80).contains(&ratio), "{cod}:{salmon}");
    let mut e = Entities::new(1);
    e.spawn_baby(MobKind::PolarBear, DVec3::Y);
    assert!(e.mobs[0].damage(100.0, None, &mut e.rng));
    e.drop_loot(MobKind::PolarBear, DVec3::Y);
    assert!(e.items.is_empty() && e.orbs.is_empty());
}

#[test]
fn polar_bear_stops_anger_when_its_mob_target_dies() {
    let mut e = Entities::new(26);
    let w = world();
    let c = ctx();
    bear(&mut e, DVec3::new(0.5, 1.0, 0.5));
    e.spawn(MobKind::Cow, DVec3::new(3.5, 1.0, 0.5));
    e.ensure_mob_ids();
    e.polar_hurt(0, Foe::Mob(e.mobs[1].uid));
    sense(&mut e, &w, &c, 1);
    assert!(e.mobs[0].polar_bear.as_ref().unwrap().current.is_some());
    e.mobs[1].damage(100.0, None, &mut e.rng);
    sense(&mut e, &w, &c, 1);
    assert!(e.mobs[0].polar_bear.as_ref().unwrap().angry_at.is_none());
}
