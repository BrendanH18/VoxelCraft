//! Original riveted copper automaton; cargo uses existing item art, no new layers.
use super::*;
const COPPER: Rgb = [185, 106, 65];
const BODY: &[Cuboid] =
    &[cube([-3., 4., -2.], [3., 10., 2.], COPPER, 23), cube([-2.5, 6., 2.], [2.5, 8.5, 2.3], [85, 64, 48], 6)];
const HEAD: &[Cuboid] = &[
    cube([-3.6, 0., -3.], [3.6, 5., 3.], COPPER, 25),
    cube([-0.7, 1.1, 3.], [0.7, 2.2, 4.6], COPPER, 15),
    cube([-2.6, 2., 3.], [-1.1, 3.3, 3.3], [244, 212, 131], 0),
    cube([1.1, 2., 3.], [2.6, 3.3, 3.3], [244, 212, 131], 0),
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
    let swing = m.limb_phase.sin() * m.limb_amp * 0.65;
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
}
pub(super) fn cargo(m: &Mob, origin: Vec3, body: Quat, light: ([u8; 4], u8), out: &mut Vec<EntityVertex>) {
    let Some(stack) = m.copper_golem.as_ref().and_then(|s| s.cargo) else { return };
    let block = stack.item.block().filter(|b| !b.flat_icon());
    let layer = block
        .map_or_else(|| stack.item.icon_layer().unwrap_or(crate::world::block::tex::ITEM_BASE), |b| b.info().tex[0]);
    let held = super::super::player_model::Limb {
        pivot: Vec3::new(0., 6.5, 5.),
        min: if block.is_some() { [-2.3; 3] } else { [-3., -3., -0.3] },
        max: if block.is_some() { [2.3; 3] } else { [3., 3., 0.3] },
        uv: [0.; 2],
        rot: Quat::IDENTITY,
    };
    super::super::player_model::textured_box(out, &held, 0., origin, body, light, 2, layer, 1., [255; 4]);
    if let Some(b) = block {
        let start = out.len() - 36;
        for (i, face) in out[start..].as_chunks_mut::<6>().0.iter_mut().enumerate() {
            let tex = b.info().tex[i ^ 1];
            for v in face {
                v.torch[2] = tex as u8;
                v.torch[3] = (tex >> 8) as u8;
            }
        }
    }
}
