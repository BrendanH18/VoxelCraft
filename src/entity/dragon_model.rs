//! Drawing the End fight: the Ender Dragon's box model (after Java's
//! `DragonModel`, with a neck and tail that trail its recent turns and
//! climbs), End crystals, their healing beams, dragon fireballs and clouds
//! of breath.

use std::f32::consts::PI;

use glam::{DVec3, Quat, Vec3};

use super::dragon::{Dragon, Fight, Phase};
use super::model::{Cuboid, EntityVertex, Rgb, cube, push_cuboid};

const SCALE: Rgb = [30, 26, 36];
const BELLY: Rgb = [44, 38, 52];
const SPINE: Rgb = [70, 64, 78];
const MEMBRANE: Rgb = [52, 44, 62];
const EYE: Rgb = [214, 80, 255];

const BODY: &[Cuboid] = &[
    cube([-12.0, 12.0, -32.0], [12.0, 36.0, 32.0], SCALE, 26),
    cube([-11.0, 11.0, -28.0], [11.0, 12.0, 28.0], BELLY, 20),
    cube([-1.0, 36.0, -24.0], [1.0, 42.0, -12.0], SPINE, 16),
    cube([-1.0, 36.0, -6.0], [1.0, 42.0, 6.0], SPINE, 16),
    cube([-1.0, 36.0, 12.0], [1.0, 42.0, 24.0], SPINE, 16),
];
/// One neck or tail segment, centred on its pivot.
const SEGMENT: &[Cuboid] =
    &[cube([-5.0, -5.0, -5.0], [5.0, 5.0, 5.0], SCALE, 26), cube([-1.0, 5.0, -3.0], [1.0, 9.0, 3.0], SPINE, 16)];
/// The head, from where the neck joins it, facing +Z.
const HEAD: &[Cuboid] = &[
    cube([-8.0, -8.0, -2.0], [8.0, 8.0, 14.0], SCALE, 26),
    cube([-6.0, -4.0, 14.0], [6.0, 1.0, 30.0], SCALE, 26),
    cube([-5.0, 1.0, 26.0], [-3.0, 3.0, 30.0], SPINE, 10),
    cube([3.0, 1.0, 26.0], [5.0, 3.0, 30.0], SPINE, 10),
    cube([-5.0, 8.0, 2.0], [-3.0, 12.0, 8.0], SPINE, 16),
    cube([3.0, 8.0, 2.0], [5.0, 12.0, 8.0], SPINE, 16),
];
const EYES: &[Cuboid] =
    &[cube([-8.1, 0.0, 8.0], [-7.9, 2.0, 12.0], EYE, 0), cube([7.9, 0.0, 8.0], [8.1, 2.0, 12.0], EYE, 0)];
const JAW: &[Cuboid] = &[cube([-6.0, -4.0, 0.0], [6.0, 0.0, 16.0], BELLY, 24)];
const WING_R: &[Cuboid] =
    &[cube([0.0, -4.0, -4.0], [56.0, 4.0, 4.0], SCALE, 26), cube([0.0, -0.5, -56.0], [56.0, 0.5, 2.0], MEMBRANE, 30)];
const WING_L: &[Cuboid] =
    &[cube([-56.0, -4.0, -4.0], [0.0, 4.0, 4.0], SCALE, 26), cube([-56.0, -0.5, -56.0], [0.0, 0.5, 2.0], MEMBRANE, 30)];
const TIP_R: &[Cuboid] =
    &[cube([0.0, -2.0, -2.0], [56.0, 2.0, 2.0], SCALE, 26), cube([0.0, -0.5, -56.0], [56.0, 0.5, 0.0], MEMBRANE, 30)];
const TIP_L: &[Cuboid] =
    &[cube([-56.0, -2.0, -2.0], [0.0, 2.0, 2.0], SCALE, 26), cube([-56.0, -0.5, -56.0], [0.0, 0.5, 0.0], MEMBRANE, 30)];
const FRONT_LEG: &[Cuboid] =
    &[cube([-4.0, -22.0, -4.0], [4.0, 2.0, 4.0], SCALE, 26), cube([-4.0, -26.0, -4.0], [4.0, -22.0, 10.0], SCALE, 26)];
const REAR_LEG: &[Cuboid] =
    &[cube([-7.0, -30.0, -7.0], [7.0, 4.0, 7.0], SCALE, 26), cube([-8.0, -36.0, -8.0], [8.0, -30.0, 14.0], SCALE, 26)];

/// Healing beams, crystals, the dragon, its fireballs and breath.
pub fn build_fight(fight: &Fight, camera: DVec3, time: f32, alpha: f64, out: &mut Vec<EntityVertex>) {
    for (i, c) in fight.crystals.iter().enumerate() {
        crystal(out, (crystal_center(c, alpha) - camera).as_vec3(), c.age as f32 + alpha as f32, i);
    }
    if let Some(d) = &fight.dragon {
        let pos = d.previous_pos.lerp(d.pos, alpha);
        if let Some(c) = d.healer.and_then(|i| fight.crystals.get(i)) {
            let from = (crystal_center(c, alpha) - camera).as_vec3();
            let to = (pos + DVec3::Y * 1.5 - camera).as_vec3();
            beam(out, from, to, time);
        }
        dragon(out, d, (pos - camera).as_vec3(), alpha as f32);
    }
    let glow = ([230, 0, 0, 255], 0);
    for f in &fight.fireballs {
        let rel = (f.previous_pos.lerp(f.pos, alpha) - camera).as_vec3();
        let rot =
            Quat::from_rotation_arc(Vec3::Z, f.heading().normalize_or(Vec3::Z)) * Quat::from_rotation_z(time * 6.0);
        for (j, c) in [
            cube([-3.0; 3], [3.0; 3], [150, 40, 200], 60),
            cube([-2.0, -2.0, -4.0], [2.0, 2.0, 4.0], [240, 160, 255], 30),
        ]
        .iter()
        .enumerate()
        {
            push_cuboid(out, c, &|v: Vec3| rel + rot * v / 16.0, rot, glow, ([0.0; 3], 0.0), j as f32);
        }
    }
    for (ci, cloud) in fight.clouds.iter().enumerate() {
        breath(out, cloud, (cloud.pos - camera).as_vec3(), time, ci);
    }
}

fn crystal_center(c: &super::dragon::Crystal, alpha: f64) -> DVec3 {
    // Java's bob: up and down by about half a block.
    let t = c.age as f64 + alpha;
    let b = (t * 0.2).sin() / 2.0 + 0.5;
    c.pos + DVec3::Y * (0.9 + (b * b + b) * 0.4)
}

/// Twelve thin bars outlining a cube of half-size `h`.
fn frame(h: f32, color: Rgb) -> [Cuboid; 12] {
    let t = 0.6;
    let bar = |axis: usize, a: f32, b: f32| {
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        let mut min = [0.0; 3];
        let mut max = [0.0; 3];
        min[axis] = -h;
        max[axis] = h;
        min[u] = a - t;
        max[u] = a + t;
        min[v] = b - t;
        max[v] = b + t;
        cube(min, max, color, 0)
    };
    let mut out = std::array::from_fn(|_| cube([0.0; 3], [0.0; 3], color, 0));
    let mut i = 0;
    for axis in 0..3 {
        for (a, b) in [(-h, -h), (h, -h), (-h, h), (h, h)] {
            out[i] = bar(axis, a, b);
            i += 1;
        }
    }
    out
}

/// A pink core inside two glassy frames turning on tilted axes.
fn crystal(out: &mut Vec<EntityVertex>, rel: Vec3, t: f32, seed: usize) {
    let spin = (t * 3.0).to_radians();
    let tilt = Vec3::new(1.0, 1.0, 0.0).normalize();
    let outer = Quat::from_rotation_y(spin) * Quat::from_axis_angle(tilt, PI / 3.0);
    let inner = outer * Quat::from_axis_angle(tilt, spin * 1.6);
    let core = inner * Quat::from_axis_angle(tilt, spin * 1.6);
    let glow = ([230, 0, 0, 255], 0);
    let none = ([0.0; 3], 0.0);
    for (rot, cubes) in [(outer, frame(6.5, [236, 226, 255])), (inner, frame(4.8, [222, 200, 250]))] {
        for (j, c) in cubes.iter().enumerate() {
            push_cuboid(out, c, &|v: Vec3| rel + rot * v / 16.0, rot, glow, none, (seed * 31 + j) as f32);
        }
    }
    let c = cube([-2.8; 3], [2.8; 3], [226, 96, 210], 70);
    push_cuboid(out, &c, &|v: Vec3| rel + core * v / 16.0, core, glow, none, seed as f32);
}

/// The healing beam from a crystal to the dragon.
fn beam(out: &mut Vec<EntityVertex>, from: Vec3, to: Vec3, time: f32) {
    let d = to - from;
    let len = d.length() * 16.0;
    let rot = Quat::from_rotation_arc(Vec3::Z, d.normalize_or(Vec3::Z)) * Quat::from_rotation_z(time * 2.0);
    let c = cube([-0.7, -0.7, 0.0], [0.7, 0.7, len], [250, 210, 255], 90);
    push_cuboid(out, &c, &|v: Vec3| from + rot * v / 16.0, rot, ([230, 0, 0, 255], 0), ([0.0; 3], 0.0), time);
}

/// Purple motes drifting up over the cloud's disc.
fn breath(out: &mut Vec<EntityVertex>, cloud: &super::dragon::BreathCloud, rel: Vec3, time: f32, seed: usize) {
    let fade = (1.0 - cloud.age as f32 / cloud.duration as f32).clamp(0.2, 1.0);
    let n = (cloud.radius * cloud.radius * 4.0) as usize;
    for i in 0..n.min(220) {
        let h = hash(i as u32, seed as u32);
        let angle = (h & 0xFFFF) as f32 / 65535.0 * std::f32::consts::TAU;
        let r = (((h >> 16) & 0xFFFF) as f32 / 65535.0).sqrt() * cloud.radius;
        let phase = (time * 0.6 + (h % 97) as f32 / 97.0).fract();
        let at = Vec3::new(angle.cos() * r, phase * 1.4, angle.sin() * r) * 16.0;
        let size = 1.6 * (1.0 - phase) * fade + 0.3;
        let color = if h & 1 == 0 { [200, 70, 240] } else { [150, 40, 200] };
        let c = cube((at - Vec3::splat(size)).to_array(), (at + Vec3::splat(size)).to_array(), color, 0);
        push_cuboid(out, &c, &|v: Vec3| rel + v / 16.0, Quat::IDENTITY, ([230, 255, 0, 220], 0), ([0.0; 3], 0.0), 0.0);
    }
}

fn hash(a: u32, b: u32) -> u32 {
    let mut h = a.wrapping_mul(0x9E37_79B9) ^ b.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^ (h >> 12)
}

/// A part of the dragon placed in model space.
struct Placed {
    boxes: &'static [Cuboid],
    pivot: Vec3,
    rot: Quat,
}

/// Unit direction of a segment chain step: `yaw` turns it about Y from
/// `forward`, `pitch` raises it.
fn step(forward: f32, yaw: f32, pitch: f32) -> Vec3 {
    Vec3::new(yaw.sin() * pitch.cos() * forward, pitch.sin(), yaw.cos() * pitch.cos() * forward)
}

fn dragon(out: &mut Vec<EntityVertex>, d: &Dragon, rel: Vec3, alpha: f32) {
    let flap = (d.previous_flap + (d.flap - d.previous_flap) * alpha) * std::f32::consts::TAU;
    // Like Java, the whole model turns with the yaw of seven ticks ago, and
    // the neck and tail bend toward newer and older headings.
    let lerp_yaw = |older: (f32, f64), newer: (f32, f64)| older.0 + wrap(newer.0 - older.0) * alpha;
    let body_yaw = lerp_yaw(d.latency(8), d.latency(7));
    let reference = d.latency(6);
    let climb = ((d.latency(5).1 - d.latency(10).1) as f32 * 10.0).to_radians().clamp(-0.8, 0.8);
    let sitting = d.phase.sitting();
    let body = Quat::from_rotation_y(PI - body_yaw.to_radians()) * Quat::from_rotation_x(-climb);
    let mut parts = vec![Placed { boxes: BODY, pivot: Vec3::ZERO, rot: Quat::IDENTITY }];

    // Neck forward from the chest, then the head.
    let offset = |sample: (f32, f64), extra: f32| {
        let yaw = -(wrap(sample.0 - reference.0) * 1.5).to_radians();
        let pitch = ((sample.1 - reference.1) as f32 * 7.5).to_radians().clamp(-0.9, 0.9) + extra;
        (yaw, pitch)
    };
    let mut at = Vec3::new(0.0, 28.0, 32.0);
    let droop = if sitting { -0.18 } else { 0.0 };
    for i in 0..5 {
        let (yaw, pitch) = offset(d.latency(5 - i), (i as f32 * 0.45 + flap).cos() * 0.05 + droop);
        let dir = step(1.0, yaw, pitch);
        parts.push(Placed { boxes: SEGMENT, pivot: at + dir * 5.0, rot: Quat::from_rotation_y(yaw) });
        at += dir * 10.0;
    }
    let (yaw, pitch) = offset(d.latency(0), droop * 2.0);
    let head = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-pitch);
    parts.push(Placed { boxes: HEAD, pivot: at, rot: head });
    parts.push(Placed { boxes: EYES, pivot: at, rot: head });
    let jaw = (flap.sin() + 1.0) * 0.12
        + if d.phase == Phase::SittingFlaming || d.phase == Phase::SittingAttacking { 0.45 } else { 0.0 };
    parts.push(Placed {
        boxes: JAW,
        pivot: at + head * Vec3::new(0.0, -4.0, 14.0),
        rot: head * Quat::from_rotation_x(jaw),
    });

    // Tail back from the rump, swinging with older headings.
    let mut at = Vec3::new(0.0, 26.0, -32.0);
    for i in 0..12 {
        let sample = d.latency(12 + i);
        let sway = (i as f32 * 0.45 + flap).sin() * 0.05;
        let yaw = -(wrap(sample.0 - reference.0) * 1.5).to_radians();
        let pitch = ((sample.1 - reference.1) as f32 * 7.5).to_radians().clamp(-0.9, 0.9) + sway - 0.03;
        let dir = step(-1.0, yaw, pitch);
        parts.push(Placed { boxes: SEGMENT, pivot: at + dir * 5.0, rot: Quat::from_rotation_y(yaw) });
        at += dir * 10.0;
    }

    // Wings: beat up and down, the tips lagging behind.
    let lift = (flap.sin() + 0.125) * 0.8;
    let tip = -(((flap + 2.0).sin() + 0.5) * 0.75);
    for side in [1.0f32, -1.0] {
        let wing = Quat::from_rotation_y(0.25 * side) * Quat::from_rotation_z(lift * side);
        let pivot = Vec3::new(12.0 * side, 32.0, 20.0);
        let (bone, end) = if side > 0.0 { (WING_R, TIP_R) } else { (WING_L, TIP_L) };
        parts.push(Placed { boxes: bone, pivot, rot: wing });
        parts.push(Placed {
            boxes: end,
            pivot: pivot + wing * Vec3::new(56.0 * side, 0.0, 0.0),
            rot: wing * Quat::from_rotation_z(tip * side),
        });
        // Legs tucked back in flight, lowered when perched.
        let tuck = if sitting { 0.2 } else { 1.0 };
        parts.push(Placed {
            boxes: FRONT_LEG,
            pivot: Vec3::new(12.0 * side, 16.0, 20.0),
            rot: Quat::from_rotation_x(tuck),
        });
        parts.push(Placed {
            boxes: REAR_LEG,
            pivot: Vec3::new(16.0 * side, 18.0, -22.0),
            rot: Quat::from_rotation_x(tuck * 0.8),
        });
    }

    let hurt = if d.phase == Phase::Dying { 0.0 } else { d.hurt / 0.5 };
    let light = [230, 0, (hurt.clamp(0.0, 1.0) * 255.0) as u8, 0];
    // Dying, it fades toward the light pouring out of it.
    let death = d.death_ticks as f32 / super::dragon::DEATH_TICKS as f32;
    let tint = ([255.0, 230.0, 255.0], death * 0.6);
    for (pi, p) in parts.iter().enumerate() {
        let rot = body * p.rot;
        let xf = |v: Vec3| rel + body * (p.pivot + p.rot * v) / 16.0;
        let glow = std::ptr::eq(p.boxes, EYES);
        let l = if glow { [light[0], 0, light[2], 255] } else { light };
        for (ci, c) in p.boxes.iter().enumerate() {
            push_cuboid(out, c, &xf, rot, (l, 0), if glow { ([0.0; 3], 0.0) } else { tint }, (pi * 8 + ci) as f32);
        }
    }
    if d.phase == Phase::Dying {
        rays(out, rel + Vec3::Y * 1.5, death);
    }
}

/// Java's death rays: more and longer as the death goes on.
fn rays(out: &mut Vec<EntityVertex>, center: Vec3, t: f32) {
    let count = ((t + t * t) / 2.0 * 60.0) as u32;
    let fade = if t > 0.8 { (1.0 - t) / 0.2 } else { 1.0 };
    for i in 0..count {
        let h = hash(i, 0xD2A6);
        let a = (h & 0x3FF) as f32 / 1023.0 * std::f32::consts::TAU;
        let b = ((h >> 10) & 0x3FF) as f32 / 1023.0 * 2.0 - 1.0;
        let dir = Vec3::new(a.cos() * (1.0 - b * b).sqrt(), b, a.sin() * (1.0 - b * b).sqrt());
        let len = ((h >> 20) % 20) as f32 + 5.0 + t * 10.0;
        let width = (((h >> 25) % 3) as f32 + 1.0 + t * 2.0) * 0.6;
        let rot = Quat::from_rotation_arc(Vec3::Z, dir) * Quat::from_rotation_z(t * 40.0 + i as f32);
        let color = [255, (220.0 + 35.0 * fade) as u8, 255];
        let c = cube([-width, -width, 0.0], [width, width, len * 16.0 * fade.max(0.3)], color, 0);
        push_cuboid(out, &c, &|v: Vec3| center + rot * v / 16.0, rot, ([255, 255, 0, 255], 0), ([0.0; 3], 0.0), 0.0);
    }
}

fn wrap(a: f32) -> f32 {
    (a + 180.0).rem_euclid(360.0) - 180.0
}
