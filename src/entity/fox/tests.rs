use super::*;
use crate::{
    entity::{Ctx, Entities, MobWorld, nether},
    physics::{BlockSource, test_util::Grid},
    simulation::lighting::SpawnLight,
    world::{
        block::Block,
        terrain::{Biome, Dimension},
    },
};
use glam::IVec3;

struct Field {
    grid: Grid,
    /// Whether the sky reaches the ground (no cover).
    open: bool,
}
impl BlockSource for Field {
    fn block(&self, p: IVec3) -> Option<Block> {
        self.grid.block(p)
    }
}
impl MobWorld for Field {
    fn loaded(&self, _: IVec3) -> bool {
        true
    }
    fn exposed(&self, _: IVec3) -> bool {
        self.open
    }
    fn surface(&self, _: i32, _: i32) -> Option<i32> {
        Some(63)
    }
    fn biome(&self, _: i32, _: i32) -> Biome {
        Biome::Taiga
    }
    fn sky_light_level(&self, _: IVec3) -> Option<u8> {
        Some(15)
    }
}

fn ctx(daylight: f32) -> Ctx {
    Ctx {
        players: Vec::new(),
        daylight,
        spawn_light: if daylight > 0.5 { SpawnLight::DAY } else { SpawnLight::NIGHT },
        spawning: false,
        raining: false,
        dimension: Dimension::Overworld,
    }
}

fn fox(e: &mut Entities, at: DVec3) -> usize {
    e.spawn(MobKind::Fox, at);
    e.ensure_mob_ids();
    let m = e.mobs.last_mut().unwrap();
    m.on_ground = true;
    e.mobs.len() - 1
}

#[test]
fn foxes_stalk_crouch_and_pounce_on_chickens() {
    let w = Field { grid: Grid::flat(64), open: true };
    let mut e = Entities::new(21);
    fox(&mut e, DVec3::new(0.5, 64.0, 0.5));
    e.spawn(MobKind::Chicken, DVec3::new(14.5, 64.0, 0.5));
    let (mut crouched, mut interested, mut pounced, mut hit) = (false, false, false, false);
    for _ in 0..1200 {
        e.update(0.05, &w, &ctx(0.0));
        let s = e.mobs[0].fox.as_ref().unwrap();
        crouched |= s.crouching;
        interested |= s.interested;
        pounced |= s.pouncing;
        hit |= e.mobs.iter().any(|m| m.kind == MobKind::Chicken && m.health < 4.0)
            || !e.mobs.iter().any(|m| m.kind == MobKind::Chicken && m.alive());
        if hit {
            break;
        }
    }
    assert!(crouched && interested, "the fox never crouched to stalk");
    assert!(pounced, "the fox never pounced");
    assert!(hit, "the chicken was never bitten");
}

#[test]
fn foxes_target_by_variant_and_range() {
    let mut e = Entities::new(5);
    fox(&mut e, DVec3::new(0.5, 64.0, 0.5));
    e.spawn(MobKind::Rabbit, DVec3::new(20.5, 64.0, 0.5));
    e.spawn(MobKind::Cow, DVec3::new(2.5, 64.0, 0.5));
    for _ in 0..60 {
        e.fox_upkeep(&ctx(0.0));
        e.mobs[0].goals.ticks = e.mobs[0].goals.ticks.wrapping_add(1);
    }
    let t = e.mobs[0].fox.as_ref().unwrap().target.expect("a rabbit within 32 blocks is a target");
    assert_eq!((t.kind, t.hunt, t.priority), (MobKind::Rabbit, Hunt::Land, 4));
    // Out of the follow range it is let go.
    e.mobs[1].pos.x = 40.5;
    e.fox_upkeep(&ctx(0.0));
    assert!(e.mobs[0].fox.as_ref().unwrap().target.is_none());
    assert_eq!(hunts(1)[0], (4, Hunt::Fish));
}

#[test]
fn foxes_sleep_by_day_under_cover_when_nothing_is_near() {
    let mut e = Entities::new(3);
    fox(&mut e, DVec3::new(0.5, 64.0, 0.5));
    // Open sky: no sleep.
    let open = Field { grid: Grid::flat(64), open: true };
    for _ in 0..400 {
        e.update(0.05, &open, &ctx(1.0));
        assert!(!e.mobs[0].animal.as_ref().unwrap().sleeping);
    }
    let covered = Field { grid: Grid::flat(64), open: false };
    let mut slept = false;
    for _ in 0..1200 {
        e.update(0.05, &covered, &ctx(1.0));
        slept |= e.mobs[0].animal.as_ref().unwrap().sleeping;
    }
    assert!(slept, "a covered fox by day should fall asleep");
    // A cow wandering close alerts it awake.
    let at = e.mobs[0].pos + DVec3::new(3.0, 0.0, 0.0);
    e.spawn(MobKind::Cow, at);
    for _ in 0..4 {
        e.update(0.05, &covered, &ctx(1.0));
    }
    assert!(!e.mobs[0].animal.as_ref().unwrap().sleeping, "an alert fox wakes");
}

#[test]
fn foxes_perch_and_look_around_at_night() {
    let w = Field { grid: Grid::flat(64), open: true };
    let mut e = Entities::new(11);
    fox(&mut e, DVec3::new(0.5, 64.0, 0.5));
    let mut sat = false;
    for _ in 0..4000 {
        e.update(0.05, &w, &ctx(0.0));
        let s = e.mobs[0].fox.as_ref().unwrap();
        if s.sitting {
            sat = true;
            assert!(matches!(s.running, Some(Running::Perch { .. })));
        }
    }
    assert!(sat, "a fox with nothing around perches now and then");
}

#[test]
fn fox_states_save_only_when_set() {
    let mut m = Mob::new(MobKind::Fox, DVec3::new(0.5, 64.0, 0.5), 0.0);
    let plain = nether::save_mob(&m);
    assert!(plain.get("fox_sitting").is_none() && plain.get("fox_crouching").is_none());
    assert!(plain.get("fox_sleeping").is_none());
    m.fox.as_mut().unwrap().sitting = true;
    m.fox.as_mut().unwrap().crouching = true;
    m.animal.as_mut().unwrap().sleeping = true;
    let saved = nether::save_mob(&m);
    let back = nether::load_mob(&saved).unwrap();
    let s = back.fox.as_ref().unwrap();
    assert!(s.sitting && s.crouching && back.animal.as_ref().unwrap().sleeping);
}

#[test]
fn the_crouch_and_interest_ease_as_java_ticks_them() {
    let mut s = State { crouching: true, interested: true, ..State::default() };
    // 0.2 a tick in float reaches 5 on the 25th or 26th tick.
    let mut ticks = 0;
    while !s.fully_crouched() {
        s.animate();
        ticks += 1;
    }
    assert!((25..=26).contains(&ticks), "{ticks}");
    assert!((s.interested_angle - (1.0 - 0.6_f32.powi(ticks))).abs() < 1e-5);
    s.crouching = false;
    s.animate();
    assert_eq!(s.crouch_amount, 0.0);
    assert!((s.crouch(0.5) - 2.5).abs() < 1e-6);
}

/// `FoxEatBerriesGoal`: a fox finds a ripe bush within 12 blocks, waits 40
/// ticks beside it, takes a berry into its empty mouth and sets the bush
/// back to age 1.
#[test]
fn foxes_eat_sweet_berries_from_ripe_bushes() {
    use crate::{entity::EntityEvent, item::Item, world::overworld_blocks as ob};
    let mut grid = Grid::flat(64);
    let bush = IVec3::new(6, 64, 0);
    grid.set(bush, ob::berry_bush(3));
    let w = Field { grid, open: true };
    let mut e = Entities::new(17);
    fox(&mut e, DVec3::new(0.5, 64.0, 0.5));
    let mut picked = false;
    for _ in 0..2400 {
        let events = e.update(0.05, &w, &ctx(0.0));
        if events.iter().any(
            |ev| matches!(ev, EntityEvent::AnimalBlock { cell, to, .. } if *cell == bush && *to == ob::berry_bush(1)),
        ) {
            picked = true;
            break;
        }
    }
    assert!(picked, "the fox never picked the bush");
    assert_eq!(e.mobs[0].animal.as_ref().unwrap().mouth.map(|s| s.item), Some(Item::SWEET_BERRIES));
}
