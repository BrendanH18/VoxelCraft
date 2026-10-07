//! Classic 64x64 humanoid skin layout and Java's distance-driven limb animation.
//! Model pixels use +Y up, +Z forward (Java's Y/Z signs are reversed).
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use glam::{DVec3, Quat, Vec3};

use super::model::{EntityVertex, cube, push_cuboid};
use crate::item::Item;
use crate::player::Player;

#[derive(Default, Debug)]
pub struct WalkAnimation {
    pub position: f32,
    pub speed: f32,
    pub previous_speed: f32,
    pub body_yaw: f32,
    pub walk_dist: f32,
    pub previous_walk_dist: f32,
    pub bob: f32,
    pub previous_bob: f32,
    /// Direction of the latest knockback in the Java camera frame, radians.
    pub hurt_direction: f32,
    initialized: bool,
}

impl WalkAnimation {
    /// LivingEntity.calculateEntityAnimation: displacement * 4, capped at 1;
    /// WalkAnimationState smooths speed by 0.4 per 20 Hz tick, then accumulates it.
    pub fn update(&mut self, distance: f32, dt: f32, yaw: f32, flying: bool, on_ground: bool) {
        let ticks = dt * 20.0;
        self.previous_walk_dist = self.walk_dist;
        self.walk_dist += distance * 0.6;
        self.previous_bob = self.bob;
        let bob_target = if on_ground && !flying { (distance / ticks.max(1e-4)).min(0.1) } else { 0.0 };
        self.bob += (bob_target - self.bob) * (1.0 - 0.6f32.powf(ticks));
        self.previous_speed = self.speed;
        let target = if flying { 0.0 } else { (distance * 4.0 / ticks.max(1e-4)).min(1.0) };
        self.speed += (target - self.speed) * (1.0 - 0.6f32.powf(ticks));
        self.position += self.speed * ticks;
        if !self.initialized {
            self.body_yaw = yaw;
            self.initialized = true;
        }
        let delta = angle(yaw - self.body_yaw);
        self.body_yaw += delta * (1.0 - 0.7f32.powf(ticks));
        // Java's body/head limit keeps the head from looking through its back.
        self.body_yaw = yaw - angle(yaw - self.body_yaw).clamp(-85f32.to_radians(), 85f32.to_radians());
    }

    pub fn sample(&self, alpha: f32) -> (f32, f32) {
        (self.position - self.speed * (1.0 - alpha), self.previous_speed + (self.speed - self.previous_speed) * alpha)
    }
}

fn angle(a: f32) -> f32 {
    (a + PI).rem_euclid(TAU) - PI
}

/// Render inputs belong to the view/player, never to a global camera.
#[derive(Clone, Copy, Default)]
pub struct PlayerAppearance {
    pub held: Option<Item>,
    pub swing: f32,
    pub using: f32,
    pub hurt: bool,
    /// Seconds since death; absent for a living player.
    pub death: Option<f32>,
    pub alpha: f32,
    /// Helmet, chest, leggings, boots.
    pub armor: [Option<super::armor::Worn>; 4],
}

const PLAYER_SCALE: f32 = 0.9375;
const UNTINTED: [u8; 4] = [255, 255, 255, 0];

#[derive(Clone, Copy)]
pub(crate) struct Limb {
    pub pivot: Vec3,
    pub rot: Quat,
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub uv: [f32; 2],
}

fn limb(pivot: [f32; 3], min: [f32; 3], max: [f32; 3], uv: [f32; 2], rot: Quat) -> Limb {
    Limb { pivot: Vec3::from_array(pivot), min, max, uv, rot }
}

/// Head, torso, right arm, left arm, right leg, left leg. Overlays/armor
/// copy these transforms so they cannot lag behind an attack or sneak pose.
pub(super) fn pose(player: &Player, appearance: &PlayerAppearance, time: f32) -> [Limb; 6] {
    let (position, amount) = player.animation.sample(appearance.alpha);
    let phase = position * 0.6662;
    let mut right_arm = (phase + PI).cos() * amount;
    let mut left_arm = phase.cos() * amount;
    if appearance.held.is_some() {
        right_arm = right_arm * 0.5 - PI / 10.0;
    }
    if appearance.using > 0.0 {
        right_arm = -FRAC_PI_2 - player.pitch;
    }
    let attack = appearance.swing.clamp(0.0, 1.0);
    let turn = (attack.sqrt() * TAU).sin() * 0.2;
    let ease = 1.0 - (1.0 - attack).powi(4);
    right_arm -= (ease * PI).sin() * 1.2 + (attack * PI).sin() * (player.pitch + 0.7) * 0.75;
    left_arm += turn;
    let crouch = player.sneaking;
    if crouch {
        right_arm += 0.4;
        left_arm += 0.4;
    }
    let ticks = time * 20.0;
    let idle_x = (ticks * 0.067).sin() * 0.05;
    let idle_z = (ticks * 0.09).cos() * 0.05 + 0.05;
    let arm_y = if crouch { 18.8 } else { 22.0 };
    let head_y = if crouch { 19.8 } else { 24.0 };
    let leg_y = if crouch { 11.8 } else { 12.0 };
    let leg_z = if crouch { -4.0 } else { 0.0 };
    let rx = Quat::from_rotation_x;
    let ry = Quat::from_rotation_y;
    let rz = Quat::from_rotation_z;
    [
        limb(
            [0.0, head_y, 0.0],
            [-4.0, 0.0, -4.0],
            [4.0, 8.0, 4.0],
            [0.0, 0.0],
            ry(angle(player.animation.body_yaw - player.yaw)) * rx(-player.pitch),
        ),
        limb(
            [0.0, if crouch { 20.8 } else { 24.0 }, 0.0],
            [-4.0, -12.0, -2.0],
            [4.0, 0.0, 2.0],
            [16.0, 16.0],
            ry(turn) * rx(if crouch { 0.5 } else { 0.0 }),
        ),
        limb(
            [-turn.cos() * 5.0, arm_y, -turn.sin() * 5.0],
            [-3.0, -10.0, -2.0],
            [1.0, 2.0, 2.0],
            [40.0, 16.0],
            ry(turn * 3.0) * rx(right_arm + idle_x) * rz(-(attack * PI).sin() * 0.4 + idle_z),
        ),
        limb(
            [turn.cos() * 5.0, arm_y, turn.sin() * 5.0],
            [-1.0, -10.0, -2.0],
            [3.0, 2.0, 2.0],
            [32.0, 48.0],
            ry(turn) * rx(left_arm - idle_x) * rz(-idle_z),
        ),
        limb(
            [-1.9, leg_y, leg_z],
            [-2.0, -12.0, -2.0],
            [2.0, 0.0, 2.0],
            [0.0, 16.0],
            rx(phase.cos() * 1.4 * amount) * ry(0.005) * rz(0.005),
        ),
        limb(
            [1.9, leg_y, leg_z],
            [-2.0, -12.0, -2.0],
            [2.0, 0.0, 2.0],
            [16.0, 48.0],
            rx((phase + PI).cos() * 1.4 * amount) * ry(-0.005) * rz(-0.005),
        ),
    ]
}

/// One texture-mapped box, still in the single entity vertex batch.
/// `material`: 1 = procedural skin atlas; 2 = existing block/item atlas.
#[allow(clippy::too_many_arguments)]
pub(crate) fn textured_box(
    out: &mut Vec<EntityVertex>,
    p: &Limb,
    inflation: f32,
    origin: Vec3,
    body: Quat,
    light: ([u8; 4], u8),
    material: u8,
    layer: u16,
    scale: f32,
    tint: [u8; 4],
) {
    let c = cube(p.min.map(|v| v - inflation), p.max.map(|v| v + inflation), [255; 3], 0);
    let start = out.len();
    push_cuboid(
        out,
        &c,
        &|v| origin + body * (p.pivot + p.rot * v) * (scale / 16.0),
        body * p.rot,
        light,
        ([0.0; 3], 0.0),
        0.0,
    );
    let [w, h, d] = std::array::from_fn(|i| p.max[i] - p.min[i]);
    let [u, v] = p.uv;
    // Java's cuboid UV net: right/front/left/back around a top/bottom strip.
    let rects = [
        [u + d + w, v + d, d, h],
        [u, v + d, d, h],
        [u + d, v, w, d],
        [u + d + w, v, w, d],
        [u + d + w + d, v + d, w, h],
        [u + d, v + d, w, h],
    ];
    for (face, vertices) in out[start..].as_chunks_mut::<6>().0.iter_mut().enumerate() {
        let axis = face / 2;
        let positive = face % 2 == 1;
        let order = if positive {
            [(0., 0.), (1., 0.), (1., 1.), (0., 0.), (1., 1.), (0., 1.)]
        } else {
            [(0., 0.), (1., 1.), (1., 0.), (0., 0.), (0., 1.), (1., 1.)]
        };
        // rects use -X,+X,+Y,-Y,-Z,+Z order.
        let rect = rects[match axis {
            0 => usize::from(positive),
            1 => {
                if positive {
                    2
                } else {
                    3
                }
            }
            _ => {
                if positive {
                    5
                } else {
                    4
                }
            }
        }];
        for (vertex, (a, b)) in vertices.iter_mut().zip(order) {
            let (x, y) = match axis {
                0 => (b, 1.0 - a),
                1 => (a, b),
                _ => (if positive { a } else { 1.0 - a }, 1.0 - b),
            };
            vertex.uv = if material == 2 { [x, y] } else { [rect[0] + x * rect[2], rect[1] + y * rect[3]] };
            vertex.color = tint;
            vertex.torch[1] = material;
            vertex.torch[2] = layer as u8;
            vertex.torch[3] = (layer >> 8) as u8;
        }
    }
}

pub fn build_player(
    player: &Player,
    position: DVec3,
    camera: DVec3,
    lighting: (f32, f32),
    time: f32,
    appearance: PlayerAppearance,
    out: &mut Vec<EntityVertex>,
) {
    let origin = (position - camera).as_vec3();
    let fall = appearance.death.map_or(0.0, |t| (((t * 20.0 - 1.0).max(0.0) / 20.0 * 1.6).sqrt()).min(1.0) * FRAC_PI_2);
    let body = Quat::from_rotation_y(FRAC_PI_2 - player.animation.body_yaw) * Quat::from_rotation_z(fall);
    let light = (
        [(lighting.0.clamp(0.0, 1.0) * 255.0) as u8, 0, if appearance.hurt { 255 } else { 0 }, 0],
        (lighting.1.clamp(0.0, 1.0) * 255.0) as u8,
    );
    let parts = pose(player, &appearance, time);
    const OVERLAYS: [[f32; 2]; 6] = [[32., 0.], [16., 32.], [40., 32.], [48., 48.], [0., 32.], [0., 48.]];
    for (i, p) in parts.iter().enumerate() {
        textured_box(out, p, 0.0, origin, body, light, 1, 0, PLAYER_SCALE, UNTINTED);
        let mut overlay = *p;
        overlay.uv = OVERLAYS[i];
        textured_box(out, &overlay, if i == 0 { 0.5 } else { 0.25 }, origin, body, light, 1, 0, PLAYER_SCALE, UNTINTED);
    }
    draw_armor(out, &parts, &appearance.armor, origin, body, PLAYER_SCALE, light);
    if let Some(item) = appearance.held {
        let arm = parts[2];
        // ItemInHandLayer's right grip: translated to the fist, then down 90°.
        let mut held = limb([0.; 3], [-4., -4., -0.5], [4., 4., 0.5], [0., 0.], Quat::IDENTITY);
        held.pivot = arm.pivot + arm.rot * Vec3::new(-1.0, -9.0, 1.0);
        held.rot = arm.rot * Quat::from_rotation_x(-FRAC_PI_2) * Quat::from_rotation_y(PI);
        if let Some(block) = item.block().filter(|b| !b.flat_icon()) {
            held.min = [-3.2; 3];
            held.max = [3.2; 3];
            textured_box(out, &held, 0.0, origin, body, light, 2, block.info().tex[0] as u16, PLAYER_SCALE, UNTINTED);
            // Use the appropriate face textures (grass, logs, crafting tables).
            let start = out.len() - 36;
            for (i, face) in out[start..].as_chunks_mut::<6>().0.iter_mut().enumerate() {
                for vertex in face {
                    vertex.torch[2] = block.info().tex[i ^ 1];
                }
            }
        } else {
            // The default thirdperson_righthand display for handheld items.
            held.rot *= Quat::from_rotation_y(-FRAC_PI_2) * Quat::from_rotation_z(55f32.to_radians());
            let layer = item.block().map_or_else(
                || item.icon_layer().unwrap_or(crate::world::block::tex::ITEM_BASE),
                |b| b.info().tex[0] as u16,
            );
            textured_box(out, &held, 0.0, origin, body, light, 2, layer, PLAYER_SCALE, UNTINTED);
        }
    }
}

/// HumanoidArmorLayer part order: head, body, right arm, left arm, right leg, left leg.
/// Outer pieces inflate by 1; leggings by 0.5. Leather draws an undyed trim just outside.
pub(crate) fn draw_armor(
    out: &mut Vec<EntityVertex>,
    parts: &[Limb; 6],
    worn: &[Option<super::armor::Worn>; 4],
    origin: Vec3,
    body: Quat,
    scale: f32,
    light: ([u8; 4], u8),
) {
    for (slot, piece) in worn.iter().enumerate() {
        let Some(piece) = *piece else { continue };
        let (indices, inflation, inner): (&[usize], f32, bool) = match slot {
            0 => (&[0], 1.0, false),
            1 => (&[1, 2, 3], 1.0, false),
            2 => (&[1, 4, 5], 0.5, true),
            _ => (&[4, 5], 1.0, false),
        };
        let paint = |out: &mut Vec<EntityVertex>, extra: f32, layer: u16, tint: [u8; 4]| {
            for &i in indices {
                textured_box(out, &parts[i], inflation + extra, origin, body, light, 3, layer, scale, tint);
            }
        };
        paint(out, 0.0, super::armor::layer(piece.kind, inner), super::armor::tint(&piece, false));
        if piece.kind == super::armor::ArmorKind::Leather {
            paint(out, 0.08, super::armor::leather_overlay(inner), super::armor::tint(&piece, true));
        }
    }
}

/// Original default skin, authored procedurally in Java's 64x64 layout.
/// All overlays are cutouts, so bare hands/face never become opaque shells.
pub fn skin_pixels() -> Vec<u8> {
    let mut pixels = vec![0u8; 64 * 64 * 4];
    let mut paint = |uv: [usize; 2], size: [usize; 3], kind: u8, overlay: bool| {
        let [w, h, d] = size;
        let faces = [
            [uv[0] + d, uv[1], w, d],
            [uv[0] + d + w, uv[1], w, d],
            [uv[0], uv[1] + d, d, h],
            [uv[0] + d, uv[1] + d, w, h],
            [uv[0] + d + w, uv[1] + d, d, h],
            [uv[0] + 2 * d + w, uv[1] + d, w, h],
        ];
        for (face, [u, v, width, height]) in faces.into_iter().enumerate() {
            for y in 0..height {
                for x in 0..width {
                    let mut color = match kind {
                        0 => {
                            if face == 0 || face == 5 || y < 2 {
                                [66, 43, 31]
                            } else {
                                [184, 129, 91]
                            }
                        }
                        1 => [34, 143, 159],
                        2 => {
                            if y < 4 {
                                [34, 143, 159]
                            } else {
                                [184, 129, 91]
                            }
                        }
                        _ => {
                            if y >= height - 2 {
                                [64, 57, 48]
                            } else {
                                [48, 58, 112]
                            }
                        }
                    };
                    if kind == 0 && face == 3 {
                        if y == 4 && (x == 1 || x == 6) {
                            color = [230, 228, 218];
                        }
                        if y == 4 && (x == 2 || x == 5) {
                            color = [57, 75, 111];
                        }
                        if y == 6 && (2..6).contains(&x) {
                            color = [110, 64, 44];
                        }
                    }
                    let visible = !overlay
                        || match kind {
                            0 => face == 0 || face == 5 || y == 0,
                            1 => y == height - 1 || (face == 3 && (x == 0 || x == width - 1)),
                            2 => y == 3,
                            _ => y == height - 3,
                        };
                    if !visible {
                        continue;
                    }
                    let n = ((x * 13 + y * 7 + face * 17) % 7) as i16 - 3;
                    let i = ((v + y) * 64 + u + x) * 4;
                    for c in 0..3 {
                        pixels[i + c] = (color[c] as i16 + n).clamp(0, 255) as u8;
                    }
                    pixels[i + 3] = 255;
                }
            }
        }
    };
    for (uv, size, kind) in [
        ([0, 0], [8, 8, 8], 0),
        ([16, 16], [8, 12, 4], 1),
        ([40, 16], [4, 12, 4], 2),
        ([32, 48], [4, 12, 4], 2),
        ([0, 16], [4, 12, 4], 3),
        ([16, 48], [4, 12, 4], 3),
    ] {
        paint(uv, size, kind, false);
    }
    for (uv, size, kind) in [
        ([32, 0], [8, 8, 8], 0),
        ([16, 32], [8, 12, 4], 1),
        ([40, 32], [4, 12, 4], 2),
        ([48, 48], [4, 12, 4], 2),
        ([0, 32], [4, 12, 4], 3),
        ([0, 48], [4, 12, 4], 3),
    ] {
        paint(uv, size, kind, true);
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn walk_uses_distance_and_java_tick_smoothing() {
        let mut w = WalkAnimation::default();
        w.update(0.2, 0.05, 0., false, true);
        assert!((w.speed - 0.32).abs() < 1e-6);
        assert!((w.position - 0.32).abs() < 1e-6);
        assert!((w.bob - 0.04).abs() < 1e-6);
        assert!((w.walk_dist - 0.12).abs() < 1e-6);
        w.update(0., 0.05, 0., false, true);
        assert!((w.speed - 0.192).abs() < 1e-6);
    }
    #[test]
    fn opposing_limbs_and_sneak_pivots_match_java() {
        let mut p = Player::new(DVec3::ZERO);
        p.animation.speed = 1.;
        p.animation.position = 0.;
        let a = PlayerAppearance { alpha: 1., ..Default::default() };
        let parts = pose(&p, &a, 0.);
        assert!((parts[4].rot * Vec3::NEG_Y).z < -0.9);
        assert!((parts[5].rot * Vec3::NEG_Y).z > 0.9);
        assert_eq!(parts[0].pivot.y, 24.);
        p.sneaking = true;
        let parts = pose(&p, &a, 0.);
        assert_eq!(parts[0].pivot.y, 19.8);
        assert_eq!(parts[4].pivot.z, -4.);
    }
    #[test]
    fn outer_armor_sits_one_pixel_outside_leggings() {
        let body = limb([0.; 3], [-4., -12., -2.], [4., 0., 2.], [16., 16.], Quat::IDENTITY);
        let parts = [body; 6];
        let mut worn = [None; 4];
        worn[1] = Some(crate::entity::armor::Worn { kind: crate::entity::armor::ArmorKind::Iron, glint: false });
        worn[2] = Some(crate::entity::armor::Worn { kind: crate::entity::armor::ArmorKind::Iron, glint: true });
        let mut out = Vec::new();
        draw_armor(&mut out, &parts, &worn, Vec3::ZERO, Quat::IDENTITY, 1.0, ([255, 0, 0, 0], 0));
        let extent = |inner: bool| {
            out.iter()
                .filter(|v| v.torch[1] == 3 && (v.torch[2].is_multiple_of(2) == inner))
                .map(|v| v.pos[0])
                .fold(0.0f32, f32::max)
        };
        assert!((extent(false) - 5.0 / 16.0).abs() < 1e-5);
        assert!((extent(true) - 4.5 / 16.0).abs() < 1e-5);
        assert!(out.iter().any(|v| v.color[3] == 255));
    }

    #[test]
    fn skin_has_a_face_and_transparent_overlays() {
        let skin = skin_pixels();
        assert_eq!(skin.len(), 64 * 64 * 4);
        assert_eq!(skin[(12 * 64 + 10) * 4 + 3], 255);
        assert_eq!(skin[(12 * 64 + 42) * 4 + 3], 0);
    }
}
