//! Original riveted copper automaton; cargo uses existing item art, no new layers.
use super::*;
const COPPER: Rgb = [185, 106, 65];
/// v0.18 Track 6 round 4: rivets and seams, a shade darker than the copper
/// at each stage of oxidation.
const RIVET: Rgb = [150, 84, 50];
const BODY: &[Cuboid] = &[
    cube([-3., 4., -2.], [3., 10., 2.], COPPER, 23),
    cube([-2.5, 6., 2.], [2.5, 8.5, 2.3], [85, 64, 48], 6),
    cube([-2.6, 9.0, 2.0], [-2.0, 9.6, 2.05], RIVET, 0),
    cube([2.0, 9.0, 2.0], [2.6, 9.6, 2.05], RIVET, 0),
    cube([-3.05, 4.0, -2.05], [3.05, 4.6, 2.05], RIVET, 0),
];
const HEAD: &[Cuboid] = &[
    cube([-3.6, 0., -3.], [3.6, 5., 3.], COPPER, 25),
    cube([-0.7, 1.1, 3.], [0.7, 2.2, 4.6], COPPER, 15),
    cube([-2.6, 2., 3.], [-1.1, 3.3, 3.3], [244, 212, 131], 0),
    cube([1.1, 2., 3.], [2.6, 3.3, 3.3], [244, 212, 131], 0),
    cube([-2.0, 0.5, 3.], [2.0, 0.9, 3.05], RIVET, 0),
    cube([-3.65, 4.4, -3.05], [3.65, 5.0, 3.05], RIVET, 0),
    cube([-0.5, 5., -0.5], [0.5, 7.5, 0.5], COPPER, 8),
    cube([-1.4, 7., -1.4], [1.4, 7.8, 1.4], COPPER, 12),
];
const LEG: &[Cuboid] = &[cube([-1.3, -4., -1.6], [1.3, 0., 1.6], COPPER, 20)];
const ARM: &[Cuboid] = &[cube([-1., -5., -1.], [1., 0., 1.], COPPER, 20)];
const POPPY: &[Cuboid] = &[
    cube([-0.25, 0., -0.25], [0.25, 3., 0.25], [67, 119, 54], 0),
    cube([-1.4, 2., -1.], [1.4, 3., 1.], [201, 50, 39], 0),
];
pub(super) fn pose(m: &Mob, time: f32) -> Parts {
    let Some(s) = &m.copper_golem else { return Parts::new() };
    // v1.0 gap Track 7: `CopperGolemModel` plays a keyframe walk; these limbs keep
    // its loop and strength (`applyWalk`).
    let (phase, scale) = super::motion::keyframe_phase(m.limb_phase, m.limb_amp, super::motion::COPPER_GOLEM_WALK);
    let swing = phase.sin() * 0.65 * scale;
    let lift = if s.cargo.is_some() {
        -0.9
    } else if s.pose > 0 {
        -0.7 + (time * 4.).sin() * 0.15
    } else {
        0.
    };
    let mut p = parts![
        part(BODY, [0.; 3], Quat::IDENTITY),
        part(HEAD, [0., 10., 0.], Quat::from_rotation_y(-m.head_yaw)),
        part(LEG, [-1.8, 4., 0.], Quat::from_rotation_x(swing)),
        part(LEG, [1.8, 4., 0.], Quat::from_rotation_x(-swing)),
        part(ARM, [-4., 9., 0.], Quat::from_rotation_x(lift - swing * 0.5)),
        part(ARM, [4., 9., 0.], Quat::from_rotation_x(lift + swing * 0.5))
    ];
    if s.antenna.is_some() {
        p.push(part(POPPY, [0., 17., 0.], Quat::IDENTITY));
    }
    p
}
pub(super) fn colour(m: &Mob, c: &mut Cuboid) {
    if c.color == COPPER
        && let Some(s) = &m.copper_golem
    {
        c.color = [[185, 106, 65], [159, 122, 81], [88, 145, 117], [60, 154, 133]][s.stage as usize];
    }
    if c.color == RIVET
        && let Some(s) = &m.copper_golem
    {
        c.color = [[150, 84, 50], [126, 96, 62], [66, 116, 92], [44, 122, 104]][s.stage as usize];
    }
}
/// v1.0 gap track 6d: the cargo in the copper golem's right hand, as
/// Java's `ItemInHandLayer` holds it: `CopperGolemModel.translateToHand`
/// (the body, the right arm, then, idle, a quarter turn about y and 2/16
/// forward in Java's frame) on this model's right arm, then the item's own
/// model (a statue's pose) through its `thirdperson_righthand` transform,
/// drawn by the item model pass with the glint and translucency. It was a
/// box in the entity pass. The game's golem has no interaction states, so
/// it always holds as Java's idle one does.
pub(super) fn cargo(
    m: &Mob,
    posed: &Parts,
    origin: Vec3,
    body: Quat,
    scale: Vec3,
    light: (f32, f32),
    out: &mut EntityMesh,
) {
    let Some(stack) = m.copper_golem.as_ref().and_then(|s| s.cargo) else { return };
    let Some(arm) = posed.get(4) else { return };
    // Java's -90 degrees about y and (0, 0, 2) pixels, in this model's frame
    // (Java's turned half about x): +90 degrees and (0, 0, -2).
    let hand = glam::Mat4::from_translation(origin)
        * glam::Mat4::from_quat(body)
        * glam::Mat4::from_scale(scale / 16.0)
        * glam::Mat4::from_translation(arm.pivot)
        * glam::Mat4::from_quat(arm.rot)
        * glam::Mat4::from_rotation_y(std::f32::consts::FRAC_PI_2)
        * glam::Mat4::from_translation(Vec3::new(0.0, 0.0, -2.0));
    let hand = super::super::player_model::item_in_hand_layer(hand);
    let statue = crate::world::copper_golem_statue::item_model(stack);
    let tint = crate::inventory::components::dye_tint(&stack);
    out.push_held(super::super::player_model::held_item(stack, hand, light, statue, None, tint));
}
