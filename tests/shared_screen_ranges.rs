//! Open-screen ranges against pinned Java 26.3 facts (`fixtures/java/26.3/combat.json`).
use glam::{DVec3, IVec3};
use voxelcraft::{
    entity::{Entities, MobKind, PlayerId},
    rules::GameMode,
    session::PlayerSession,
};

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/combat.json")).unwrap()
}

fn player() -> PlayerSession {
    PlayerSession::new(PlayerId(4), DVec3::new(0.5, 64.0, 0.5))
}

#[test]
fn block_screens_stay_open_within_block_reach_plus_the_reference_extra() {
    let f = fixture();
    let extra = f["screen_ranges"]["block_interaction_range_extra"]["block_container"].as_f64().unwrap();
    let survival = f["player"]["default_block_interaction_range"].as_f64().unwrap() + extra;
    let mut s = player();
    let eye = s.player.eye();
    // The cell's near face sits `gap` blocks along +x from the eye, level with it.
    let cell = |gap: f64| IVec3::new((eye.x + gap) as i32, eye.y as i32, 0);
    let face_gap = |c: IVec3| c.x as f64 - eye.x;
    let inside = cell(survival - 0.6);
    let outside = cell(survival + 0.4);
    assert!(face_gap(inside) < survival && face_gap(outside) >= survival);
    assert!(s.can_keep_using_block(inside));
    assert!(!s.can_keep_using_block(outside));
    // Creative block reach is 5.0, so a cell just past survival range stays usable.
    s.set_mode(GameMode::Creative);
    let creative = s.block_reach() + extra;
    assert_eq!(creative, 9.0);
    let between =
        (survival.ceil() as i32..).map(|x| IVec3::new(x, eye.y as i32, 0)).find(|c| face_gap(*c) >= survival).unwrap();
    assert_eq!(s.can_keep_using_block(between), face_gap(between) < creative);
}

#[test]
fn trading_stays_open_within_entity_reach_plus_the_reference_extra_to_the_box() {
    let f = fixture();
    let extra = f["screen_ranges"]["entity_interaction_range_extra"]["merchant"].as_f64().unwrap();
    let mut s = player();
    assert_eq!(s.entity_screen_range(), f["player"]["default_entity_interaction_range"].as_f64().unwrap() + extra);
    let eye = s.player.eye();
    let mut e = Entities::new(5);
    // A villager whose box (half-width 0.3) starts just inside, then just outside, 7 blocks away.
    for (near_face, open) in [(6.9, true), (7.1, false)] {
        e.mobs.clear();
        e.spawn(MobKind::Villager, DVec3::new(eye.x + near_face + 0.3, 64.0, 0.5));
        let id = e.mobs[0].villager.as_ref().unwrap().id;
        assert_eq!(e.merchant_in_reach(id, eye, s.entity_screen_range()), open, "{near_face}");
    }
    // Creative's +2 entity reach extends the screen range.
    s.set_mode(GameMode::Creative);
    let id = e.mobs[0].villager.as_ref().unwrap().id;
    assert!(e.merchant_in_reach(id, eye, s.entity_screen_range()));
}
