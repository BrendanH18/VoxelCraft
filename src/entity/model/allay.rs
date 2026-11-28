//! Original cyan sprite: luminous eyes, tunic, hands and beating veined wings.
use super::*;
use std::f32::consts::FRAC_PI_4;
const BODY: &[Cuboid] = &[
    cube([-1.4, 2., -1.], [1.4, 5., 1.], [38, 161, 202], 18),
    cube([-2., 4.8, -1.8], [2., 8.8, 1.8], [78, 202, 228], 12),
    cube([-1.5, 6.1, 1.81], [-0.4, 7.4, 1.9], [218, 252, 255], 0),
    cube([0.4, 6.1, 1.81], [1.5, 7.4, 1.9], [218, 252, 255], 0),
    cube([-0.6, 5.4, 1.81], [0.6, 5.7, 1.9], [29, 112, 158], 0),
    // v0.18 Track 6 round 4: deep blue pupils and a fringe of hair.
    cube([-1.1, 6.4, 1.9], [-0.7, 7.0, 1.95], [29, 112, 158], 0),
    cube([0.7, 6.4, 1.9], [1.1, 7.0, 1.95], [29, 112, 158], 0),
    cube([-2.05, 7.9, -1.85], [2.05, 8.85, 1.95], [60, 180, 214], 0),
    cube([-1., 0., -0.6], [-0.2, 2., 0.6], [57, 181, 211], 12),
    cube([0.2, 0., -0.6], [1., 2., 0.6], [57, 181, 211], 12),
];
const ARM: &[Cuboid] = &[cube([-0.4, -2.8, -0.4], [0.4, 0., 0.4], [87, 212, 235], 12)];
const WING: &[Cuboid] = &[
    cube([0., -1.9, -0.1], [3.8, 2., 0.1], [128, 224, 237], 8),
    cube([3.8, -1.3, -0.1], [4.8, 1.2, 0.1], [101, 210, 228], 8),
    cube([0., -0.12, 0.11], [4.5, 0.12, 0.17], [48, 167, 202], 0),
];
/// v1.0 gap Track 7b: `AllayModel.setupAnim`. The wings beat on
/// `cos(ageInTicks * 20 degrees + walk position) * 0.15 pi` plus the walk
/// speed, tipped back while idle; flying (`min(speed / 0.3, 1)`) the body
/// leans forward 45 degrees; idle, it bobs a quarter pixel and its arms
/// hang 25 degrees out, swaying; holding an item (`holdingAnimationProgress`
/// over 5 ticks) the arms reach forward. Dancing, it sways 16 degrees and
/// spins two turns over the first 15 of every 55 ticks (`Allay.tick`'s
/// state).
pub(super) fn pose(m: &Mob, time: f32, alpha: f32) -> Parts {
    use crate::entity::mob_anim::lerp;
    let (rx, ry, rz) = (Quat::from_rotation_x, Quat::from_rotation_y, Quat::from_rotation_z);
    let age = age_ticks(time);
    let (pos, speed) = (m.limb_phase, m.limb_amp);
    let flap = ((age * 20.0).to_radians() + pos).cos() * PI * 0.15 + speed;
    let idle_speed = (age * 9.0).to_radians();
    let flying = (speed / 0.3).min(1.0);
    let idle = 1.0 - flying;
    let a = &m.anim;
    let holding = lerp(alpha, a.o_hold, a.hold) / 5.0;
    let mut root = Quat::IDENTITY;
    if m.allay.as_ref().is_some_and(|s| s.dancing) {
        let dance = (age * 8.0).to_radians() + speed;
        let spin = lerp(alpha, a.o_spin, a.spin) / 15.0;
        // Java's `zRot` then `yRot` (`ModelPart` turns z, y, x), on this
        // model's sides.
        let sway = rz(-dance.cos() * 16f32.to_radians() * (1.0 - spin));
        root = if a.spinning() { sway * ry(-4.0 * PI * spin) } else { sway };
    }
    let arm_x = holding * lerp(flying, -PI / 3.0, -1.134_464);
    let bob_amount = 0.436_332_32 - (idle_speed + PI * 1.5).cos() * PI * 0.075 * idle * (1.0 - holding);
    let tilt = 0.436_332_32 * idle;
    let mut p = parts![
        part(BODY, [0.; 3], rx(flying * FRAC_PI_4)),
        // The right arm on -X: Java's turns on this model's sides.
        part(ARM, [-1.9, 4.5, 0.], rz(-bob_amount) * ry(-0.279_252_68 * holding) * rx(arm_x)),
        part(ARM, [1.9, 4.5, 0.], rz(bob_amount) * ry(0.279_252_68 * holding) * rx(arm_x)),
        part(WING, [1., 4.5, -1.2], ry(flap) * rx(tilt)),
        part(WING, [-1., 4.5, -1.2], ry(PI - flap) * rx(-tilt))
    ];
    p.turn_about(Vec3::new(0.0, 4.5, 0.0), root);
    p.transform(Vec3::new(0.0, -idle_speed.cos() * 0.25 * idle, 0.0), Quat::IDENTITY);
    p
}
/// v1.0 gap track 6d: the liked item in the allay's hands, as Java's
/// `ItemInHandLayer` holds it: `AllayModel.translateToHand` (the body, then
/// 1/16 down and 3/16 back in Java's frame, the right arm's x turn, 0.7 of
/// the size and 1/16 to the side) on this model's body at its Java pivot
/// 4.5 pixels up, then the item's own model through its
/// `thirdperson_righthand` transform, drawn by the item model pass (glint
/// and translucency included). It was a small box in the entity pass.
#[allow(clippy::too_many_arguments)]
pub(super) fn held(
    m: &Mob,
    posed: &Parts,
    origin: Vec3,
    body: Quat,
    scale: Vec3,
    alpha: f32,
    light: (f32, f32),
    out: &mut EntityMesh,
) {
    use crate::entity::mob_anim::lerp;
    let Some(stack) = m.allay.as_ref().and_then(|s| s.liked) else { return };
    let Some(torso) = posed.first() else { return };
    let a = &m.anim;
    let holding = lerp(alpha, a.o_hold, a.hold) / 5.0;
    let flying = (m.limb_amp / 0.3).min(1.0);
    let arm_x = holding * lerp(flying, -PI / 3.0, -1.134_464);
    // Java's frame is this one turned half about x: its (0, 1, 3) is (0, -1, -3).
    let pivot = torso.pivot + torso.rot * Vec3::new(0.0, 4.5, 0.0);
    let hand = glam::Mat4::from_translation(origin)
        * glam::Mat4::from_quat(body)
        * glam::Mat4::from_scale(scale / 16.0)
        * glam::Mat4::from_translation(pivot)
        * glam::Mat4::from_quat(torso.rot)
        * glam::Mat4::from_translation(Vec3::new(0.0, -1.0, -3.0))
        * glam::Mat4::from_rotation_x(arm_x)
        * glam::Mat4::from_scale(Vec3::splat(0.7))
        * glam::Mat4::from_translation(Vec3::new(1.0, 0.0, 0.0));
    let hand = super::super::player_model::item_in_hand_layer(hand);
    let statue = crate::world::copper_golem_statue::item_model(stack);
    let tint = crate::inventory::components::dye_tint(&stack);
    out.push_held(super::super::player_model::held_item(stack, hand, light, statue, None, tint));
}
