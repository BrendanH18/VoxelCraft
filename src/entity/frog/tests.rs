use super::*;
use crate::{
    entity::{EntityEvent, PlayerId, Target},
    physics::{BlockSource, test_util::Grid},
    simulation::lighting::SpawnLight,
};
use glam::IVec2;
struct Habitat {
    grid: Grid,
    biome: Biome,
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
    fn entity_ticking(&self, _: IVec3) -> bool {
        self.active
    }
    fn surface(&self, _: i32, _: i32) -> Option<i32> {
        Some(0)
    }
    fn exposed(&self, _: IVec3) -> bool {
        true
    }
    fn sky_light_level(&self, _: IVec3) -> Option<u8> {
        Some(15)
    }
    fn biome(&self, _: i32, _: i32) -> Biome {
        self.biome
    }
}
fn habitat() -> Habitat {
    Habitat { grid: Grid::flat(1), biome: Biome::Swamp, active: true }
}
fn context() -> Ctx {
    Ctx {
        players: vec![Target::new(PlayerId::HOST, DVec3::new(10.0, 1.0, 0.5), false)],
        daylight: 1.0,
        spawning: false,
        raining: false,
        dimension: Dimension::Overworld,
        spawn_light: SpawnLight::DAY,
    }
}
fn world() -> World {
    let mut w =
        World::new_headless(std::sync::Arc::new(crate::world::terrain::Generator::new(42)), Default::default(), 2);
    w.load_test_column(IVec2::ZERO, 64);
    w
}
#[test]
fn variants_follow_java_biome_tags_and_not_temperature() {
    let facts: Value = serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/frogs.json")).unwrap();
    for b in Biome::ALL {
        let expected = if facts["cold_biomes"].as_array().unwrap().iter().any(|n| n == b.name()) {
            1
        } else if facts["warm_biomes"].as_array().unwrap().iter().any(|n| n == b.name()) {
            2
        } else {
            0
        };
        assert_eq!(variant(b, Dimension::Overworld), expected, "{}", b.name());
    }
    assert_eq!(variant(Biome::DeepFrozenOcean, Dimension::Overworld), 1);
    assert_eq!(variant(Biome::DeepDark, Dimension::Overworld), 1);
    assert_eq!(variant(Biome::MangroveSwamp, Dimension::Overworld), 2);
    assert_eq!(variant(Biome::Meadow, Dimension::End), 1);
    assert_eq!(variant(Biome::Meadow, Dimension::Nether), 2);
}
#[test]
fn tadpole_age_pauses_unloaded_and_growth_uses_the_destination_biome() {
    let mut e = Entities::new(3);
    let mut w = habitat();
    let ctx = context();
    e.spawn(MobKind::Tadpole, DVec3::new(0.5, 1.0, 0.5));
    e.mobs[0].name = crate::inventory::StackName::new("Puddle").unwrap();
    e.mobs[0].health = 2.0;
    e.ensure_mob_ids();
    let uid = e.mobs[0].uid;
    e.frog_upkeep(1199.95, &w, &ctx);
    assert_eq!(e.mobs[0].frog.as_ref().unwrap().age, 23999);
    w.active = false;
    e.frog_upkeep(100.0, &w, &ctx);
    assert_eq!(e.mobs[0].kind, MobKind::Tadpole);
    w.active = true;
    w.biome = Biome::MangroveSwamp;
    e.frog_upkeep(0.05, &w, &ctx);
    let m = &e.mobs[0];
    assert_eq!((m.kind, m.uid, m.health, m.frog.as_ref().unwrap().variant), (MobKind::Frog, uid, 10.0, 2));
    assert!(m.persistent && !m.baby && m.age == 0);
    assert_eq!(m.name.as_str(), Some("Puddle"));
}
#[test]
fn feeding_bucket_payload_and_species_state_survive_restart() {
    let w = world();
    let mut e = Entities::new(4);
    e.spawn(MobKind::Tadpole, DVec3::new(3.5, 65.0, 3.5));
    assert!(!e.feed_tadpole(0, Item::WHEAT, &w));
    assert!(e.feed_tadpole(0, Item::SLIME_BALL, &w));
    assert_eq!(e.mobs[0].frog.as_ref().unwrap().age, 2400);
    e.mobs[0].frog.as_mut().unwrap().age = 12345;
    e.mobs[0].frog.as_mut().unwrap().age_locked = true;
    assert!(!e.feed_tadpole(0, Item::SLIME_BALL, &w));
    e.mobs[0].health = 3.5;
    let stack =
        Stack { entity_data: e.mobs[0].frog.as_ref().unwrap().bucket(3.5), ..Stack::new(Item::TADPOLE_BUCKET, 1) };
    let mut inv = crate::inventory::Inventory::default();
    inv.slots[0] = Some(stack);
    let saved = crate::inventory::Inventory::deserialize(&inv.serialize()).unwrap();
    assert_eq!(saved.get(0), Some(stack));
    let mut released = Entities::new(5);
    assert!(released.release_water_creature(saved.get(0).unwrap(), DVec3::new(2.5, 65.0, 2.5)));
    let m = &released.mobs[0];
    assert_eq!((m.health, m.frog.as_ref().unwrap().age, m.frog.as_ref().unwrap().age_locked), (3.5, 12345, true));
    e.spawn(MobKind::Frog, DVec3::new(5.5, 65.0, 3.5));
    e.mobs[1].frog.as_mut().unwrap().variant = 1;
    e.mobs[1].animal.as_mut().unwrap().pregnant = true;
    let mut loaded = Entities::new(6);
    loaded.load_nether_mobs(&e.nether_mobs_to_string());
    let tad = loaded.mobs.iter().find(|m| m.kind == MobKind::Tadpole).unwrap();
    assert_eq!((tad.frog.as_ref().unwrap().age, tad.frog.as_ref().unwrap().age_locked), (12345, true));
    let frog = loaded.mobs.iter().find(|m| m.kind == MobKind::Frog).unwrap();
    assert_eq!(frog.frog.as_ref().unwrap().variant, 1);
    assert!(frog.animal.as_ref().unwrap().pregnant);
    let mut replica = Mob::new(MobKind::Frog, DVec3::ZERO, 0.0);
    replica.apply_look(&frog.look());
    assert_eq!(replica.frog.as_ref().unwrap().variant, 1);
}
#[test]
fn frogs_eat_only_small_cubes_after_seven_catch_ticks_and_drop_the_matching_light() {
    let w = habitat();
    let ctx = context();
    for (color, light) in [(0, frog_blocks::OCHRE), (1, frog_blocks::VERDANT), (2, frog_blocks::PEARLESCENT)] {
        let mut e = Entities::new(7);
        e.spawn(MobKind::Frog, DVec3::new(0.5, 1.0, 0.5));
        {
            let s = e.mobs[0].frog.as_mut().unwrap();
            s.initialized = true;
            s.variant = color;
        }
        e.spawn(MobKind::MagmaCube, DVec3::new(1.5, 1.0, 0.5));
        e.mobs[1].set_size(1);
        e.frog_upkeep(0.05, &w, &ctx);
        assert_eq!(e.mobs[0].frog.as_ref().unwrap().pose, 1);
        assert_eq!(e.mobs[1].vel, DVec3::NEG_X * 15.0);
        for _ in 0..6 {
            e.frog_upkeep(0.05, &w, &ctx);
        }
        assert_eq!(e.mobs.len(), 2);
        assert!(e.items.is_empty());
        e.frog_upkeep(0.05, &w, &ctx);
        assert_eq!(e.mobs.len(), 1);
        assert_eq!(e.items.iter().map(|i| i.stack).collect::<Vec<_>>(), vec![Stack::new(light, 1)]);
        assert!(e.orbs.is_empty());
        for _ in 0..4 {
            e.frog_upkeep(0.05, &w, &ctx);
        }
        assert!(e.mobs[0].frog.as_ref().unwrap().tongue.is_none());
    }
    let mut large = Mob::new(MobKind::Slime, DVec3::ZERO, 0.0);
    large.set_size(2);
    assert!(!prey(&large));
    large.set_size(1);
    assert!(prey(&large));
}
#[test]
fn breeding_produces_pregnancy_then_spawn_on_water_instead_of_a_baby_frog() {
    let mut e = Entities::new(9);
    let mut w = habitat();
    let ctx = context();
    for x in [0.5, 1.5] {
        e.spawn(MobKind::Frog, DVec3::new(x, 1.0, 0.5));
        assert_eq!(e.use_animal(e.mobs.len() - 1, Some(Stack::new(Item::SLIME_BALL, 1)), PlayerId::HOST), Some(true));
    }
    for _ in 0..80 {
        e.update(0.05, &w, &ctx);
    }
    assert_eq!(e.mobs.len(), 2);
    let mother = e.mobs.iter().position(|m| m.animal.as_ref().unwrap().pregnant).expect("one pregnant parent");
    assert!(e.mobs.iter().all(|m| !m.baby && m.age > 5900));
    let cell = e.mobs[mother].pos.floor().as_ivec3() + IVec3::Z;
    w.grid.set(cell - IVec3::Y, Block::WATER);
    e.mobs[mother].on_ground = true;
    let events = e.update(0.05, &w, &ctx);
    assert!(
        events
            .iter()
            .any(|e| matches!(e,EntityEvent::AnimalBlock {cell:c,to,..} if *c==cell&&*to==frog_blocks::FROGSPAWN))
    );
    assert!(!e.mobs[mother].animal.as_ref().unwrap().pregnant);
    e.spawn_baby(MobKind::Frog, DVec3::ZERO);
    assert!(!e.mobs.last().unwrap().baby);
}

#[test]
fn shared_player_use_feeds_captures_and_releases_a_named_tadpole() {
    use crate::session::{PlayerSession, interact::UseContext};
    let mut w = world();
    let mut e = Entities::new(71);
    e.spawn(MobKind::Tadpole, DVec3::new(3.0, 65.0, 0.5));
    e.mobs[0].name = crate::inventory::StackName::new("Sprout").unwrap();
    let mut p = PlayerSession::new(PlayerId::HOST, DVec3::new(0.5, 65.0, 0.5));
    p.player.pitch = (-1.47_f32 / 2.5).atan();
    let ctx = UseContext { sneaking: false, others: &[] };
    p.inventory.slots[0] = Some(Stack::new(Item::SLIME_BALL, 2));
    assert!(p.use_held(0, &mut w, &mut e, &ctx).unwrap().is_some());
    assert_eq!(p.inventory.get(0).unwrap().count, 1);
    assert_eq!(e.mobs[0].frog.as_ref().unwrap().age, 2400);
    p.inventory.slots[0] = Some(Stack::new(Item::WATER_BUCKET, 1));
    assert!(p.use_held(0, &mut w, &mut e, &ctx).unwrap().is_some());
    assert!(e.mobs.is_empty());
    assert_eq!(p.inventory.get(0).unwrap().item, Item::TADPOLE_BUCKET);
    assert_eq!(p.inventory.get(0).unwrap().name.as_str(), Some("Sprout"));
    assert!(p.use_held(0, &mut w, &mut e, &ctx).unwrap().is_some());
    assert_eq!(p.inventory.get(0).unwrap().item, Item::BUCKET);
    assert_eq!(e.mobs.len(), 1);
    assert_eq!(e.mobs[0].frog.as_ref().unwrap().age, 2400);
    assert_eq!(e.mobs[0].name.as_str(), Some("Sprout"));
    assert!(e.mobs[0].persistent);
}

#[test]
fn frog_slime_loot_is_exactly_one_and_respects_mob_loot() {
    let w = habitat();
    let ctx = context();
    let facts: Value = serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/frogs.json")).unwrap();
    let entry = &facts["data"]["loot_table/entities/slime"]["pools"][0]["entries"][1];
    let count = entry["modifier"]["count"].as_u64().unwrap() as u8;
    for mob_loot in [true, false] {
        let mut e = Entities::new(34);
        e.mob_loot = mob_loot;
        e.spawn(MobKind::Frog, DVec3::new(0.5, 1.0, 0.5));
        e.spawn(MobKind::Slime, DVec3::new(1.5, 1.0, 0.5));
        e.mobs[1].set_size(1);
        for _ in 0..8 {
            e.frog_upkeep(0.05, &w, &ctx);
        }
        assert_eq!(e.mobs.len(), 1);
        assert_eq!(e.items.iter().map(|i| i.stack.count).sum::<u8>(), if mob_loot { count } else { 0 });
    }
}

#[test]
fn a_shielded_tongue_target_receives_a_knockback_not_the_frogs_world_position() {
    let w = habitat();
    let ctx = context();
    let mut e = Entities::new(58);
    e.spawn(MobKind::Frog, DVec3::new(1000.5, 1.0, 1000.5));
    e.spawn(MobKind::Slime, DVec3::new(1001.5, 1.0, 1000.5));
    e.mobs[1].set_size(1);
    e.mobs[1].effects.add(crate::simulation::effects::Effect::Absorption, 3, 200);
    for _ in 0..8 {
        e.frog_upkeep(0.05, &w, &ctx);
    }
    assert_eq!(e.mobs.len(), 2);
    assert_eq!(e.mobs[1].health, 1.0);
    assert!((e.mobs[1].vel.x - 0.5).abs() < 1e-6);
    assert_eq!(e.mobs[1].vel.z, 0.0);
    assert!(e.items.is_empty());
}
