//! Minecraft-style box models, animated and flattened into camera-relative
//! triangles for the entity render pass.
//!
//! Models are authored in 1/16-block "pixels" with the origin at the feet,
//! +Y up and +Z forward. Each part has a pivot (in model space) that it
//! rotates around; its cuboids are given relative to that pivot.

use std::f32::consts::{FRAC_PI_2, PI};

use glam::{DVec3, Quat, Vec3};

use super::mob::{HURT_TIME, Mob, MobKind};
use crate::render::entity::EntityVertex;

type Rgb = [u8; 3];

struct Cuboid {
    min: [f32; 3],
    max: [f32; 3],
    color: Rgb,
    /// Per-texel brightness noise, 0..255 (255 = ±50%).
    noise: u8,
}

const fn cube(min: [f32; 3], max: [f32; 3], color: Rgb, noise: u8) -> Cuboid {
    Cuboid { min, max, color, noise }
}

// ---------------------------------------------------------------- pig

const PIG_SKIN: Rgb = [238, 160, 158];
const PIG_LEG: Rgb = [222, 142, 142];
const PIG_SNOUT: Rgb = [226, 128, 138];
const DARK: Rgb = [60, 30, 36];
const WHITE: Rgb = [235, 235, 235];

const PIG_BODY: &[Cuboid] = &[cube([-5.0, 6.0, -8.0], [5.0, 14.0, 8.0], PIG_SKIN, 22)];
const PIG_LEG_BOX: &[Cuboid] = &[cube([-2.0, -6.0, -2.0], [2.0, 0.0, 2.0], PIG_LEG, 22)];
const PIG_HEAD: &[Cuboid] = &[
    cube([-4.0, -4.0, 0.0], [4.0, 4.0, 8.0], PIG_SKIN, 22),
    cube([-2.0, -3.0, 8.0], [2.0, 0.0, 9.0], PIG_SNOUT, 14),
    cube([-1.5, -2.0, 9.0], [-0.5, -1.0, 9.1], DARK, 0),
    cube([0.5, -2.0, 9.0], [1.5, -1.0, 9.1], DARK, 0),
    cube([-4.0, 0.0, 8.0], [-3.0, 1.0, 8.1], DARK, 0),
    cube([-3.0, 0.0, 8.0], [-2.0, 1.0, 8.1], WHITE, 0),
    cube([3.0, 0.0, 8.0], [4.0, 1.0, 8.1], DARK, 0),
    cube([2.0, 0.0, 8.0], [3.0, 1.0, 8.1], WHITE, 0),
];

// ---------------------------------------------------------------- zombie

const ZOMBIE_SKIN: Rgb = [92, 146, 74];
const ZOMBIE_SHIRT: Rgb = [0, 158, 160];
const ZOMBIE_PANTS: Rgb = [62, 56, 150];
const ZOMBIE_SHOES: Rgb = [72, 72, 82];
const ZOMBIE_EYES: Rgb = [24, 40, 24];

const ZOMBIE_BODY: &[Cuboid] = &[cube([-4.0, 12.0, -2.0], [4.0, 24.0, 2.0], ZOMBIE_SHIRT, 40)];
const ZOMBIE_LEG: &[Cuboid] = &[
    cube([-2.0, -10.0, -2.0], [2.0, 0.0, 2.0], ZOMBIE_PANTS, 40),
    cube([-2.0, -12.0, -2.0], [2.0, -10.0, 2.0], ZOMBIE_SHOES, 30),
];
const ZOMBIE_ARM: &[Cuboid] = &[
    cube([-2.0, -4.0, -2.0], [2.0, 2.0, 2.0], ZOMBIE_SHIRT, 40),
    cube([-2.0, -10.0, -2.0], [2.0, -4.0, 2.0], ZOMBIE_SKIN, 36),
];
const ZOMBIE_HEAD: &[Cuboid] = &[
    cube([-4.0, 0.0, -4.0], [4.0, 8.0, 4.0], ZOMBIE_SKIN, 36),
    cube([-3.0, 3.0, 4.0], [-1.0, 4.0, 4.1], ZOMBIE_EYES, 0),
    cube([1.0, 3.0, 4.0], [3.0, 4.0, 4.1], ZOMBIE_EYES, 0),
    cube([-2.0, 1.0, 4.0], [2.0, 1.6, 4.05], ZOMBIE_EYES, 10),
];

/// A part placed in model space.
struct Part {
    boxes: &'static [Cuboid],
    pivot: Vec3,
    rot: Quat,
}

fn part(boxes: &'static [Cuboid], pivot: [f32; 3], rot: Quat) -> Part {
    Part { boxes, pivot: Vec3::from_array(pivot), rot }
}

/// Animated parts for a mob, in model space (pixels).
fn pose(m: &Mob, time: f32) -> Vec<Part> {
    let swing = m.limb_phase.sin() * m.limb_amp * 0.9;
    let head = Quat::from_rotation_y(-m.head_yaw) * Quat::from_rotation_x(-m.head_pitch);
    let rx = Quat::from_rotation_x;
    match m.kind {
        MobKind::Pig => vec![
            part(PIG_BODY, [0.0; 3], Quat::IDENTITY),
            part(PIG_LEG_BOX, [-3.0, 6.0, 5.0], rx(swing)),
            part(PIG_LEG_BOX, [3.0, 6.0, 5.0], rx(-swing)),
            part(PIG_LEG_BOX, [-3.0, 6.0, -5.0], rx(-swing)),
            part(PIG_LEG_BOX, [3.0, 6.0, -5.0], rx(swing)),
            part(PIG_HEAD, [0.0, 12.0, 8.0], head),
        ],
        MobKind::Zombie => {
            // Arms held forward, bobbing a little, chopping down on attack.
            let chop = if m.attack_anim > 0.0 { (m.attack_anim / 0.35 * PI).sin() * 0.7 } else { 0.0 };
            let bob = (time * 1.6).sin() * 0.05;
            let arm = |s: f32| rx(-FRAC_PI_2 + bob * s + swing * 0.25 * s + chop);
            vec![
                part(ZOMBIE_BODY, [0.0; 3], Quat::IDENTITY),
                part(ZOMBIE_LEG, [-2.0, 12.0, 0.0], rx(swing)),
                part(ZOMBIE_LEG, [2.0, 12.0, 0.0], rx(-swing)),
                part(ZOMBIE_ARM, [-6.0, 22.0, 0.0], arm(1.0)),
                part(ZOMBIE_ARM, [6.0, 22.0, 0.0], arm(-1.0)),
                part(ZOMBIE_HEAD, [0.0, 24.0, 0.0], head),
            ]
        }
    }
}

/// Appends triangles for every mob within `max_dist` of the camera that
/// isn't behind it; returns how many mobs were drawn.
pub fn build(
    mobs: &[Mob],
    camera: DVec3,
    forward: Vec3,
    max_dist: f32,
    time: f32,
    out: &mut Vec<EntityVertex>,
) -> usize {
    let mut drawn = 0;
    for m in mobs {
        let rel = (m.pos - camera).as_vec3();
        let center = rel + Vec3::Y * m.shape().height as f32 * 0.5;
        let radius = 1.5;
        if center.length() > max_dist + radius || center.dot(forward) < -radius {
            continue;
        }
        drawn += 1;

        // Death: topple onto the side, lifted so it doesn't sink into the
        // ground.
        let death = m.death_progress();
        let body = Quat::from_rotation_y(FRAC_PI_2 - m.yaw) * Quat::from_rotation_z(death * FRAC_PI_2);
        let origin = rel + Vec3::Y * (m.shape().half_width as f32 * death);
        let hurt = if m.dying.is_some() { 1.0 } else { m.hurt / HURT_TIME };
        let light = [(m.sky_light.clamp(0.0, 1.0) * 255.0) as u8, 0, (hurt.clamp(0.0, 1.0) * 255.0) as u8, 0];
        let tint = if m.burning { 0.3 } else { 0.0 };
        for (pi, p) in pose(m, time).iter().enumerate() {
            let rot = body * p.rot;
            let xf = |v: Vec3| origin + body * (p.pivot + p.rot * v) / 16.0;
            for (ci, c) in p.boxes.iter().enumerate() {
                push_cuboid(out, c, &xf, rot, light, tint, (pi * 8 + ci) as f32);
            }
        }
        if m.burning {
            flames(out, m, rel, time);
        }
    }
    drawn
}

/// Small glowing cubes rising around a burning mob.
fn flames(out: &mut Vec<EntityVertex>, m: &Mob, rel: Vec3, time: f32) {
    let shape = m.shape();
    let (r, h) = (shape.half_width as f32 * 16.0 + 0.5, shape.height as f32 * 16.0);
    for i in 0..10 {
        let fi = i as f32;
        let t = (time * 1.3 + fi * 0.618).fract();
        let angle = fi * 2.4 + (time * 1.3 + fi * 0.618).floor() * 1.7;
        let size = 3.0 * (1.0 - t) + 0.5;
        let center = Vec3::new(angle.cos() * r, t * h * 1.1, angle.sin() * r);
        let color = if t < 0.4 {
            [255, 214, 80]
        } else if t < 0.75 {
            [255, 140, 30]
        } else {
            [220, 60, 20]
        };
        let c = Cuboid {
            min: (center - Vec3::splat(size / 2.0)).to_array(),
            max: (center + Vec3::splat(size / 2.0)).to_array(),
            color,
            noise: 0,
        };
        push_cuboid(out, &c, &|v: Vec3| rel + v / 16.0, Quat::IDENTITY, [255, 255, 0, 255], 0.0, 0.0);
    }
}

/// Appends one cuboid; `tint` blends its colour toward fire orange.
fn push_cuboid(
    out: &mut Vec<EntityVertex>,
    c: &Cuboid,
    xf: &impl Fn(Vec3) -> Vec3,
    rot: Quat,
    light: [u8; 4],
    tint: f32,
    seed: f32,
) {
    let (min, max) = (Vec3::from_array(c.min), Vec3::from_array(c.max));
    let fire = [255.0, 120.0, 30.0];
    let ch = |i: usize| (c.color[i] as f32 + (fire[i] - c.color[i] as f32) * tint) as u8;
    let color = [ch(0), ch(1), ch(2), c.noise];
    for axis in 0..3 {
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        for side in [false, true] {
            let mut n = Vec3::ZERO;
            n[axis] = if side { 1.0 } else { -1.0 };
            let wn = rot * n;
            // Same directional shading as terrain faces.
            let shade = 0.8 * wn.x * wn.x + 0.68 * wn.z * wn.z + if wn.y > 0.0 { 1.0 } else { 0.55 } * wn.y * wn.y;
            let mut l = light;
            l[1] = (shade * 255.0) as u8;
            // Counter-clockwise seen from outside (u x v = +axis).
            let order: [(f32, f32); 6] = if side {
                [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
            } else {
                [(0.0, 0.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
            };
            let offset = [seed * 7.0 + axis as f32 * 31.0 + side as u8 as f32 * 17.0, seed * 3.0];
            for (a, b) in order {
                let mut p = Vec3::ZERO;
                p[axis] = if side { max[axis] } else { min[axis] };
                p[u] = min[u] + (max[u] - min[u]) * a;
                p[v] = min[v] + (max[v] - min[v]) * b;
                out.push(EntityVertex {
                    pos: xf(p).to_array(),
                    uv: [p[u] + offset[0], p[v] + offset[1]],
                    color,
                    light: l,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn models_face_their_yaw_and_stand_on_the_ground() {
        for kind in MobKind::ALL {
            // Facing +X (yaw 0): the head is the part furthest along +X.
            let mob = Mob::new(kind, DVec3::new(10.0, 64.0, 0.0), 0.0);
            let mut out = Vec::new();
            assert_eq!(build(std::slice::from_ref(&mob), DVec3::new(0.0, 64.0, 0.0), Vec3::X, 100.0, 0.0, &mut out), 1);
            let (lo, hi) = out.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), v| {
                let p = Vec3::from_array(v.pos);
                (lo.min(p), hi.max(p))
            });
            assert!(lo.y.abs() < 1e-4, "{kind:?} feet at {}", lo.y);
            let h = kind.shape().height as f32;
            assert!((hi.y - h).abs() < 0.15, "{kind:?} top {} vs box {h}", hi.y);
            assert!(hi.x > 10.3, "{kind:?} should extend forward along +X");
            // Behind the camera: culled.
            out.clear();
            assert_eq!(
                build(std::slice::from_ref(&mob), DVec3::new(0.0, 64.0, 0.0), Vec3::NEG_X, 100.0, 0.0, &mut out),
                0
            );
        }
    }
}
