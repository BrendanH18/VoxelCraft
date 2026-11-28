use super::*;
use crate::{
    entity::fox::{Hunt, Prey, Running},
    physics::test_util::Grid,
    world::terrain::Biome,
};

struct Snow(Grid);
impl crate::physics::BlockSource for Snow {
    fn block(&self, p: IVec3) -> Option<Block> {
        self.0.block(p)
    }
}
impl MobWorld for Snow {
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
        Biome::SnowyTaiga
    }
}

fn prey(at: DVec3) -> Prey {
    Prey {
        uid: 77,
        kind: MobKind::Rabbit,
        pos: at,
        vel: DVec3::ZERO,
        eye: 0.4,
        half_width: 0.2,
        height: 0.5,
        hunt: Hunt::Land,
        priority: 4,
        player: None,
    }
}

#[test]
fn a_pounce_landing_in_snow_faceplants_the_fox_for_forty_ticks() {
    let mut grid = Grid::flat(1);
    grid.set(IVec3::new(0, 1, 0), crate::world::overworld_blocks::snow_layer(1));
    let w = Snow(grid);
    let mut m = Mob::new(MobKind::Fox, DVec3::new(0.5, 1.0, 0.5), 0.0);
    m.on_ground = true;
    m.vel = DVec3::new(0.0, -1.0, 0.0);
    m.look.pitch = 20.0;
    let s = m.fox.as_mut().unwrap();
    s.target = Some(prey(DVec3::new(8.5, 1.0, 0.5)));
    s.running = Some(Running::Pounce);
    s.pouncing = true;
    let mut events = Vec::new();
    m.pounce_tick(&w, &mut events);
    let s = m.fox.as_ref().unwrap();
    assert!(s.faceplanted && s.target.is_none());
    assert_eq!(m.look.pitch, 60.0);
    // The pounce ends and `FaceplantGoal` holds it for `adjustedTickDelay(40)`.
    let mut planted = 0;
    for tick in 0..60 {
        let full = tick % 2 == 0;
        m.fox_pre_think(&w, &mut Rng::new(tick), full, &mut events);
        if m.fox.as_ref().unwrap().faceplanted {
            planted += 1;
        }
    }
    assert!((38..=42).contains(&planted), "{planted}");
    let s = m.fox.as_ref().unwrap();
    assert!(!s.pouncing && !s.crouching && s.running.is_none());
}

#[test]
fn the_path_to_prey_must_be_clear_to_pounce() {
    let w = Snow(Grid::flat(1));
    assert!(path_clear(&w, DVec3::new(0.5, 1.0, 0.5), DVec3::new(4.5, 1.0, 0.5)));
    let mut grid = Grid::flat(1);
    grid.set(IVec3::new(2, 2, 0), Block::STONE);
    let w = Snow(grid);
    assert!(!path_clear(&w, DVec3::new(0.5, 1.0, 0.5), DVec3::new(4.5, 1.0, 0.5)));
}

#[test]
fn a_crouching_fox_keeps_its_pitch_and_skips_its_player_look() {
    let mut m = Mob::new(MobKind::Fox, DVec3::new(0.5, 1.0, 0.5), 0.0);
    m.look_tick();
    m.look.pitch = 20.0;
    m.fox.as_mut().unwrap().crouching = true;
    m.look_tick();
    assert_eq!(m.look.pitch, 20.0, "`resetXRotOnTick` is false while crouching");
    m.fox.as_mut().unwrap().crouching = false;
    m.look_tick();
    assert_eq!(m.look.pitch, 0.0);
    m.fox.as_mut().unwrap().interested = true;
    assert!(m.fox_busy());
}
