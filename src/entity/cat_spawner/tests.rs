use super::*;
use crate::{
    entity::{PlayerId, Target},
    physics::{BlockSource, test_util::Grid},
    simulation::{difficulty::Difficulty, lighting::SpawnLight},
    world::{block::Block, terrain::Biome},
};
use std::sync::Arc;
struct Habitat {
    grid: Grid,
    pois: Vec<IVec3>,
    hut: Option<Arc<Built>>,
    available: bool,
}
impl BlockSource for Habitat {
    fn block(&self, p: IVec3) -> Option<Block> {
        self.grid.block(p)
    }
}
impl MobWorld for Habitat {
    fn loaded(&self, _: IVec3) -> bool {
        self.available
    }
    fn surface(&self, _: i32, _: i32) -> Option<i32> {
        Some(0)
    }
    fn exposed(&self, _: IVec3) -> bool {
        false
    }
    fn biome(&self, _: i32, _: i32) -> Biome {
        Biome::Swamp
    }
    fn village_pois(&self, visit: &mut dyn FnMut(IVec3, Block)) {
        for &p in &self.pois {
            visit(p, Block::BED_HEAD);
        }
    }
    fn structures_near(&self, _: IVec3, _: i32) -> Vec<Arc<Built>> {
        self.hut.iter().cloned().collect()
    }
}
fn world() -> Habitat {
    Habitat { grid: Grid::flat(1), pois: vec![], hut: None, available: true }
}
fn ctx() -> Ctx {
    Ctx {
        players: vec![Target::new(PlayerId::HOST, DVec3::new(0.5, 1.0, 0.5), true)],
        daylight: 0.0,
        spawning: true,
        raining: false,
        dimension: Dimension::Overworld,
        spawn_light: SpawnLight::NIGHT,
    }
}
fn village(e: &mut Entities, w: &mut Habitat, count: usize) {
    for n in 0..count {
        let p = IVec3::new(n as i32 % 6, 1, n as i32 / 6);
        w.pois.push(p);
        e.spawn(MobKind::Villager, p.as_dvec3());
        e.mobs.last_mut().unwrap().villager.as_mut().unwrap().home = Some(p);
    }
}
#[test]
fn cat_spawner_requires_five_occupied_beds_and_caps_at_five_even_with_thirty() {
    let facts: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/cat_spawner.json")).unwrap();
    let mut e = Entities::new(71);
    let mut w = world();
    let p = IVec3::new(10, 1, 10);
    village(&mut e, &mut w, 4);
    assert!(!e.try_cat_spawn(&w, p));
    w.pois.push(IVec3::new(0, 1, 6));
    assert!(!e.try_cat_spawn(&w, p), "unclaimed beds do not count");
    village(&mut e, &mut w, 30);
    let cap = facts["village_cat_cap"].as_u64().unwrap() as usize;
    for _ in 0..cap {
        assert!(e.try_cat_spawn(&w, p));
    }
    assert!(!e.try_cat_spawn(&w, p));
    assert_eq!(e.count(MobKind::Cat), cap);
    assert!(
        e.mobs
            .iter()
            .filter(|m| m.kind == MobKind::Cat)
            .all(|m| !m.persistent && !m.baby && m.animal.as_ref().unwrap().owner.is_none())
    );
}
#[test]
fn cat_spawner_uses_player_height_checks_the_loaded_window_and_has_no_animal_light_gate() {
    let mut e = Entities::new(8);
    let mut w = world();
    village(&mut e, &mut w, 5);
    let p = IVec3::new(10, 1, 10);
    w.available = false;
    assert!(!e.try_cat_spawn(&w, p));
    w.available = true;
    assert!(!e.try_cat_spawn(&w, p + IVec3::Y), "no heightmap correction");
    w.grid.set(p, Block::STONE);
    assert!(!e.try_cat_spawn(&w, p));
    w.grid.set(p, Block::AIR);
    assert!(e.try_cat_spawn(&w, p), "stone floor in darkness is valid");
}
#[test]
fn cat_spawner_runs_on_the_first_then_every_twelve_hundred_ticks_and_gates_the_rule() {
    let mut e = Entities::new(55);
    e.moon_brightness = 0.0;
    let mut w = world();
    village(&mut e, &mut w, 5);
    let mut c = ctx();
    c.spawning = false;
    e.cat_spawner_tick(60.0, &w, &c);
    assert_eq!(e.count(MobKind::Cat), 0);
    c.spawning = true;
    c.players[0].spectator = true;
    e.cat_spawner_tick(0.05, &w, &c);
    assert_eq!(e.count(MobKind::Cat), 1);
    let m = e.mobs.iter().find(|m| m.kind == MobKind::Cat).unwrap();
    assert!((8..=31).contains(&(m.pos.x.floor() as i32).abs()));
    assert!((8..=31).contains(&(m.pos.z.floor() as i32).abs()));
    assert!(!m.persistent);
    assert!(m.animal.as_ref().unwrap().variant > 0);
    for _ in 0..1199 {
        e.cat_spawner_tick(0.05, &w, &c);
    }
    assert_eq!(e.count(MobKind::Cat), 1);
    e.cat_spawner_tick(0.05, &w, &c);
    assert_eq!(e.count(MobKind::Cat), 2);
}
#[test]
fn cat_variants_preserve_all_eleven_through_save_replication_and_parental_inheritance() {
    let mut e = Entities::new(23);
    for n in 0..11 {
        e.spawn(MobKind::Cat, DVec3::new(n as f64, 1.0, 0.0));
        e.mobs[n].animal.as_mut().unwrap().variant = n as u8;
    }
    let mut loaded = Entities::new(24);
    loaded.load_nether_mobs(&e.nether_mobs_to_string());
    for (n, m) in loaded.mobs.iter().enumerate() {
        assert_eq!(m.animal.as_ref().unwrap().variant, n as u8);
        let mut replica = Mob::new(MobKind::Cat, DVec3::ZERO, 0.0);
        replica.apply_look(&m.look());
        assert_eq!(replica.animal.as_ref().unwrap().variant, n as u8);
    }
    for _ in 0..50 {
        assert!(matches!(e.animal_child(9, 10).animal.as_ref().unwrap().variant, 9 | 10));
    }
    let mut rng = Rng::new(93);
    let w = world();
    for _ in 0..200 {
        assert_ne!(variant(&w, IVec3::ZERO, 0.89, &mut rng), 0);
    }
    assert!((0..200).any(|_| variant(&w, IVec3::ZERO, 1.0, &mut rng) == 0));
}
#[test]
fn swamp_hut_cat_is_black_persistent_once_and_old_partial_markers_migrate() {
    for rot in 0..4 {
        let built = Arc::new(crate::world::temples::test_swamp_hut(rot));
        let pos = built.residents.iter().find(|(k, _)| *k == MobKind::Cat).unwrap().1;
        let mut w = world();
        w.grid = Grid::flat(pos.y);
        w.hut = Some(built.clone());
        let mut c = ctx();
        c.players[0].pos = pos.as_dvec3();
        let mut e = Entities::new(7);
        e.populate_structures(1.0, &w, &c, Difficulty::Peaceful);
        assert_eq!(e.count(MobKind::Cat), 1);
        let m = e.mobs.iter().find(|m| m.kind == MobKind::Cat).unwrap();
        assert!(m.persistent);
        assert_eq!(m.animal.as_ref().unwrap().variant, 0);
        assert_eq!(variant(&w, pos, 0.0, &mut e.rng), 0);
        // The periodic spawner's finalize-before-snap ordering uses the origin.
        assert_ne!(variant(&w, IVec3::ZERO, 0.0, &mut e.rng), 0);
        e.mobs.clear();
        let saved = e.nether_mobs_to_string();
        let mut loaded = Entities::new(8);
        loaded.load_nether_mobs(&saved);
        loaded.populate_structures(1.0, &w, &c, Difficulty::Peaceful);
        assert_eq!(loaded.count(MobKind::Cat), 0);
        let mut old = Entities::new(9);
        old.structures_populated.insert(legacy_hut_marker(&built, pos).unwrap());
        old.populate_structures(1.0, &w, &c, Difficulty::Peaceful);
        assert_eq!(old.count(MobKind::Cat), 0);
        assert!(old.structures_populated.contains(&pos));
    }
}

#[test]
fn cat_spawner_periodic_hut_replacement_is_persistent_and_locally_limited() {
    let built = Arc::new(crate::world::temples::test_swamp_hut(0));
    let p = built.residents.iter().find(|(k, _)| *k == MobKind::Cat).unwrap().1;
    let mut w = world();
    w.grid = Grid::flat(p.y);
    w.hut = Some(built);
    let mut e = Entities::new(51);
    e.moon_brightness = 0.0;
    assert!(e.try_cat_spawn(&w, p));
    assert!(e.mobs[0].persistent);
    assert_ne!(e.mobs[0].animal.as_ref().unwrap().variant, 0, "pinned finalize-before-snap order");
    assert!(!e.try_cat_spawn(&w, p));
    e.mobs.clear();
    assert!(e.try_cat_spawn(&w, p));
}
