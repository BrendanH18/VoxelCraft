//! v0.16 Track A: the shulker and its bullet, original box models in the
//! proportions of Java's `ShulkerModel` (a 16×8×16 base, a 16×12×16 lid
//! that rises and turns as it peeks, a 6×6×6 head inside) and
//! `ShulkerBulletModel` (an 8×8×2 core in crossed slabs).
use super::*;

const SHELL: Rgb = [150, 106, 150];
const SHELL_RIM: Rgb = [120, 82, 124];
const HEAD: Rgb = [236, 222, 176];
const EYE: Rgb = [40, 40, 56];

const BASE: &[Cuboid] =
    &[cube([-8.0, 0.0, -8.0], [8.0, 8.0, 8.0], SHELL, 34), cube([-8.05, 6.0, -8.05], [8.05, 8.0, 8.05], SHELL_RIM, 20)];
// v0.18 Track 6 round 4: plate seams across the lid, which take the dye.
const LID: &[Cuboid] = &[
    cube([-8.0, 0.0, -8.0], [8.0, 12.0, 8.0], SHELL, 34),
    cube([-8.05, 2.0, -0.5], [8.05, 12.05, 0.5], SHELL_RIM, 0),
    cube([-0.5, 2.0, -8.05], [0.5, 12.05, 8.05], SHELL_RIM, 0),
];
const SHULKER_HEAD: &[Cuboid] = &[
    cube([-3.0, 0.0, -3.0], [3.0, 6.0, 3.0], HEAD, 18),
    cube([-2.0, 2.5, 3.0], [-0.5, 4.0, 3.05], EYE, 0),
    cube([0.5, 2.5, 3.0], [2.0, 4.0, 3.05], EYE, 0),
    // v0.18 Track 6 round 4: eye glints and a small mouth.
    cube([-1.1, 3.4, 3.05], [-0.7, 3.8, 3.1], [230, 226, 214], 0),
    cube([1.3, 3.4, 3.05], [1.7, 3.8, 3.1], [230, 226, 214], 0),
    cube([-1.0, 1.0, 3.0], [1.0, 1.5, 3.05], [150, 120, 96], 0),
];

/// The whole shell turned so its base clings to `attach` (around the
/// cell's centre, 8 pixels up).
fn clinging(attach: crate::world::end_blocks::Dir) -> Quat {
    let down = attach.offset().as_vec3();
    Quat::from_rotation_arc(Vec3::NEG_Y, down)
}

pub(super) fn pose(m: &Mob, time: f32) -> Parts {
    let (attach, amount) = m.shulker().map_or((crate::world::end_blocks::Dir::Down, 0.0), |s| {
        let a = s.amount_before + (s.amount - s.amount_before) * (time * 20.0).fract().clamp(0.0, 1.0);
        (s.attach, a)
    });
    // `ShulkerModel.setupAnim`: the lid lifts with the physical peek and
    // twists a quarter turn as it opens.
    let lift = crate::entity::shulker::physical_peek(amount) * 8.0;
    let twist = Quat::from_rotation_y(amount * std::f32::consts::FRAC_PI_2 * 0.5);
    let head = Quat::from_rotation_y(-m.head_yaw) * Quat::from_rotation_x(-m.head_pitch * amount);
    let whole = clinging(attach);
    let centre = Vec3::new(0.0, 8.0, 0.0);
    let turn = |pivot: [f32; 3], rot: Quat| {
        let p = whole * (Vec3::from_array(pivot) - centre) + centre;
        (p.to_array(), whole * rot)
    };
    let (bp, br) = turn([0.0, 0.0, 0.0], Quat::IDENTITY);
    let (lp, lr) = turn([0.0, 4.0 + lift, 0.0], twist);
    let (hp, hr) = turn([0.0, 6.0, 0.0], head);
    parts![part(BASE, bp, br), part(LID, lp, lr), part(SHULKER_HEAD, hp, hr)]
}

/// A dyed shulker's shell takes its colour.
pub(super) fn colour(m: &Mob, c: &mut Cuboid) {
    let Some(color) = m.shulker().and_then(|s| s.color) else { return };
    let rgb = color.rgb();
    if c.color == SHELL {
        c.color = rgb;
    } else if c.color == SHELL_RIM {
        c.color = rgb.map(|v| (u16::from(v) * 4 / 5) as u8);
    }
}

/// Shulker bullets: a pale core spinning in its flight.
pub fn build_shulker_bullets(
    bullets: &[crate::entity::shulker::Bullet],
    camera: DVec3,
    time: f32,
    alpha: f64,
    out: &mut EntityMesh,
) {
    const BULLET: &[Cuboid] = &[
        cube([-4.0, -4.0, -1.0], [4.0, 4.0, 1.0], [246, 238, 220], 20),
        cube([-1.0, -4.0, -4.0], [1.0, 4.0, 4.0], [226, 210, 236], 20),
        cube([-4.0, -1.0, -4.0], [4.0, 1.0, 4.0], [236, 224, 230], 20),
    ];
    for (i, b) in bullets.iter().enumerate() {
        let rel = (b.previous_pos.lerp(b.pos, alpha) - camera).as_vec3();
        let rot = Quat::from_rotation_y(time * 6.0 + i as f32) * Quat::from_rotation_x(time * 4.0);
        for (j, c) in BULLET.iter().enumerate() {
            push_cuboid(
                out,
                c,
                &|v: Vec3| rel + rot * v * 0.5 / 16.0,
                rot,
                ([255, 255, 0, 255], 0),
                ([0.0; 3], 0.0),
                j as f32,
            );
        }
    }
}
