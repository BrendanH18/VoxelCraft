//! Original cyan sprite: luminous eyes, tunic, hands and beating veined wings.
use super::*;
const BODY: &[Cuboid] = &[
    cube([-1.4, 2., -1.], [1.4, 5., 1.], [38, 161, 202], 18),
    cube([-2., 4.8, -1.8], [2., 8.8, 1.8], [78, 202, 228], 12),
    cube([-1.5, 6.1, 1.81], [-0.4, 7.4, 1.9], [218, 252, 255], 0),
    cube([0.4, 6.1, 1.81], [1.5, 7.4, 1.9], [218, 252, 255], 0),
    cube([-0.6, 5.4, 1.81], [0.6, 5.7, 1.9], [29, 112, 158], 0),
    cube([-1., 0., -0.6], [-0.2, 2., 0.6], [57, 181, 211], 12),
    cube([0.2, 0., -0.6], [1., 2., 0.6], [57, 181, 211], 12),
];
const ARM: &[Cuboid] = &[cube([-0.4, -2.8, -0.4], [0.4, 0., 0.4], [87, 212, 235], 12)];
const WING: &[Cuboid] = &[
    cube([0., -1.9, -0.1], [3.8, 2., 0.1], [128, 224, 237], 8),
    cube([3.8, -1.3, -0.1], [4.8, 1.2, 0.1], [101, 210, 228], 8),
    cube([0., -0.12, 0.11], [4.5, 0.12, 0.17], [48, 167, 202], 0),
];
pub(super) fn pose(m: &Mob, time: f32) -> Parts {
    let dancing = m.allay.as_ref().is_some_and(|s| s.dancing);
    let rot = if dancing { Quat::from_rotation_y(time * 3.) } else { Quat::IDENTITY };
    let flap = (time * 32.).sin() * 0.65;
    let arm = if m.allay.as_ref().is_some_and(|s| s.liked.is_some()) { -1.1 } else { 0.15 };
    parts![
        part(BODY, [0.; 3], rot),
        part(ARM, [-1.9, 4.5, 0.], rot * Quat::from_rotation_x(arm)),
        part(ARM, [1.9, 4.5, 0.], rot * Quat::from_rotation_x(arm)),
        part(WING, [1., 4.5, -1.2], rot * Quat::from_rotation_y(flap)),
        part(WING, [-1., 4.5, -1.2], rot * Quat::from_rotation_y(PI - flap))
    ]
}
pub(super) fn held(m: &Mob, origin: Vec3, body: Quat, light: ([u8; 4], u8), out: &mut Vec<EntityVertex>) {
    let Some(stack) = m.allay.as_ref().and_then(|s| s.liked) else { return };
    let block = stack.item.block().filter(|b| !b.flat_icon());
    // A flat-icon block (a flower) has no item icon: draw its block texture flat.
    let layer = stack
        .item
        .block()
        .map(|b| b.info().tex[0])
        .or_else(|| stack.item.icon_layer())
        .unwrap_or(crate::world::block::tex::ITEM_BASE);
    let limb = super::super::player_model::Limb {
        pivot: Vec3::new(0., 2.7, 2.4),
        min: if block.is_some() { [-1.2; 3] } else { [-1.7, -1.7, -0.15] },
        max: if block.is_some() { [1.2; 3] } else { [1.7, 1.7, 0.15] },
        uv: [0.; 2],
        rot: Quat::IDENTITY,
    };
    let tint = crate::inventory::components::dye_tint_bytes(stack);
    super::super::player_model::textured_box(out, &limb, 0., origin, body, light, 2, layer, 1., tint);
}
