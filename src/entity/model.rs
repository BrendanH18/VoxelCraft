//! Minecraft-style box models, animated and flattened into camera-relative
//! triangles for the entity render pass.
//!
//! Models are authored in 1/16-block "pixels" with the origin at the feet,
//! +Y up and +Z forward. Each part has a pivot (in model space) that it
//! rotates around; its cuboids are given relative to that pivot.

use std::f32::consts::{FRAC_PI_2, PI};

use glam::{DVec3, Quat, Vec3};

use super::mob::{Ai, FUSE_TIME, HURT_TIME, Mob, MobKind};
use super::{Arrow, Puff};
use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct EntityVertex {
    /// Camera-relative position.
    pub pos: [f32; 3],
    /// Model-space texel coordinates on the face (drives the pixel noise).
    pub uv: [f32; 2],
    /// rgb, a: strength of the per-texel noise.
    pub color: [u8; 4],
    /// x: sky light, y: face shade, z: hurt tint, w: emissive (flames).
    pub light: [u8; 4],
    /// x: block light (torches); the rest is padding.
    pub torch: [u8; 4],
}

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

// ---------------------------------------------------------------- zombified piglin

const PIGLIN_SKIN: Rgb = [226, 150, 140];
const PIGLIN_ROT: Rgb = [116, 150, 80];
const PIGLIN_CLOTH: Rgb = [112, 78, 46];
const GOLD: Rgb = [250, 210, 60];

const PIGLIN_BODY: &[Cuboid] = &[
    cube([-4.0, 12.0, -2.0], [4.0, 24.0, 2.0], PIGLIN_SKIN, 36),
    // Rotted-through ribs and a loincloth.
    cube([-3.0, 16.0, 2.0], [1.0, 21.0, 2.1], PIGLIN_ROT, 40),
    cube([-4.1, 11.0, -2.1], [4.1, 14.0, 2.1], PIGLIN_CLOTH, 30),
];
const PIGLIN_LEG: &[Cuboid] = &[
    cube([-2.0, -12.0, -2.0], [2.0, 0.0, 2.0], PIGLIN_SKIN, 36),
    cube([-2.05, -3.0, -2.05], [2.05, 0.0, 2.05], PIGLIN_CLOTH, 30),
];
const PIGLIN_ARM: &[Cuboid] = &[cube([-2.0, -10.0, -2.0], [2.0, 2.0, 2.0], PIGLIN_ROT, 40)];
const PIGLIN_SWORD_ARM: &[Cuboid] = &[
    cube([-2.0, -10.0, -2.0], [2.0, 2.0, 2.0], PIGLIN_SKIN, 36),
    // A golden sword gripped in the fist, pointing forward.
    cube([-0.5, -10.0, 1.0], [0.5, -9.0, 3.0], [100, 70, 30], 10),
    cube([-1.5, -10.0, 3.0], [1.5, -9.0, 3.5], GOLD, 10),
    cube([-0.5, -10.0, 3.5], [0.5, -9.0, 11.0], GOLD, 10),
];
const PIGLIN_HEAD: &[Cuboid] = &[
    cube([-5.0, 0.0, -4.0], [5.0, 8.0, 4.0], PIGLIN_SKIN, 36),
    cube([-2.0, 0.0, 4.0], [2.0, 3.0, 5.0], [236, 168, 160], 14),
    cube([-1.5, 1.0, 5.0], [-0.5, 2.0, 5.1], DARK, 0),
    cube([0.5, 1.0, 5.0], [1.5, 2.0, 5.1], DARK, 0),
    cube([-4.0, 4.0, 4.0], [-2.0, 5.0, 4.1], WHITE, 0),
    cube([2.0, 4.0, 4.0], [4.0, 5.0, 4.1], DARK, 0),
    // Floppy ears, one half rotted.
    cube([-6.0, 2.0, -1.0], [-5.0, 7.0, 2.0], PIGLIN_SKIN, 36),
    cube([5.0, 2.0, -1.0], [6.0, 7.0, 2.0], PIGLIN_ROT, 40),
    cube([-1.0, 4.0, -4.1], [3.0, 8.0, 0.0], PIGLIN_ROT, 40),
];

// ---------------------------------------------------------------- cow

const COW_HIDE: Rgb = [88, 62, 46];
const COW_PATCH: Rgb = [232, 230, 224];
const COW_NOSE: Rgb = [206, 160, 150];
const HORN: Rgb = [222, 216, 196];

const COW_BODY: &[Cuboid] = &[
    cube([-6.0, 12.0, -9.0], [6.0, 22.0, 9.0], COW_HIDE, 24),
    cube([-6.1, 14.0, -2.0], [-5.9, 20.0, 5.0], COW_PATCH, 10),
    cube([5.9, 16.0, -7.0], [6.1, 21.0, -1.0], COW_PATCH, 10),
    cube([-3.0, 21.9, -6.0], [2.0, 22.1, 0.0], COW_PATCH, 10),
];
const COW_LEG: &[Cuboid] = &[cube([-2.0, -12.0, -2.0], [2.0, 0.0, 2.0], COW_HIDE, 24)];
const COW_HEAD: &[Cuboid] = &[
    cube([-4.0, -4.0, 0.0], [4.0, 4.0, 6.0], COW_HIDE, 24),
    cube([-3.0, -4.0, 6.0], [3.0, -1.0, 7.0], COW_NOSE, 12),
    cube([-1.0, -1.0, 6.0], [1.0, 4.0, 6.05], COW_PATCH, 8),
    cube([-5.0, 2.0, 1.0], [-4.0, 5.0, 2.0], HORN, 8),
    cube([4.0, 2.0, 1.0], [5.0, 5.0, 2.0], HORN, 8),
    cube([-3.0, 0.0, 6.0], [-2.0, 1.0, 6.1], DARK, 0),
    cube([2.0, 0.0, 6.0], [3.0, 1.0, 6.1], DARK, 0),
];

// ---------------------------------------------------------------- sheep

const WOOL: Rgb = [232, 232, 226];
const SHEEP_SKIN: Rgb = [214, 190, 170];

const SHEEP_BODY: &[Cuboid] = &[cube([-5.0, 10.0, -8.0], [5.0, 19.0, 8.0], WOOL, 34)];
const SHEEP_LEG: &[Cuboid] =
    &[cube([-2.5, -5.0, -2.5], [2.5, 0.0, 2.5], WOOL, 34), cube([-2.0, -12.0, -2.0], [2.0, -5.0, 2.0], SHEEP_SKIN, 14)];
const SHEEP_HEAD: &[Cuboid] = &[
    cube([-3.0, -3.0, 0.0], [3.0, 3.0, 7.0], SHEEP_SKIN, 14),
    cube([-3.5, 0.5, -0.5], [3.5, 4.0, 5.0], WOOL, 34),
    cube([-2.5, 0.0, 7.0], [-1.5, 1.0, 7.1], DARK, 0),
    cube([1.5, 0.0, 7.0], [2.5, 1.0, 7.1], DARK, 0),
];

// ---------------------------------------------------------------- chicken

const FEATHERS: Rgb = [240, 240, 236];
const BEAK: Rgb = [240, 170, 40];
const WATTLE: Rgb = [205, 30, 30];
const CHICKEN_FEET: Rgb = [226, 176, 52];

const CHICKEN_BODY: &[Cuboid] = &[cube([-3.0, 4.0, -4.0], [3.0, 9.0, 3.0], FEATHERS, 18)];
const CHICKEN_LEG: &[Cuboid] = &[
    cube([-0.5, -4.0, -0.5], [0.5, 0.0, 0.5], CHICKEN_FEET, 8),
    cube([-1.0, -4.0, -1.0], [1.0, -3.6, 1.5], CHICKEN_FEET, 8),
];
const CHICKEN_WING_L: &[Cuboid] = &[cube([-1.0, -4.0, -3.0], [0.0, 0.0, 3.0], FEATHERS, 18)];
const CHICKEN_WING_R: &[Cuboid] = &[cube([0.0, -4.0, -3.0], [1.0, 0.0, 3.0], FEATHERS, 18)];
const CHICKEN_HEAD: &[Cuboid] = &[
    cube([-2.0, 0.0, 0.0], [2.0, 4.0, 3.0], FEATHERS, 18),
    cube([-2.0, 1.0, 3.0], [2.0, 2.5, 6.0], BEAK, 8),
    cube([-1.0, -1.0, 3.0], [1.0, 1.0, 4.0], WATTLE, 8),
    cube([-2.0, 2.5, 3.0], [-1.0, 3.5, 3.1], DARK, 0),
    cube([1.0, 2.5, 3.0], [2.0, 3.5, 3.1], DARK, 0),
];

// ---------------------------------------------------------------- skeleton

const BONE: Rgb = [214, 214, 208];
const RIBS: Rgb = [176, 176, 170];
const BOW: Rgb = [112, 80, 42];

const SKELETON_BODY: &[Cuboid] = &[
    cube([-1.0, 12.0, -1.0], [1.0, 24.0, 1.0], RIBS, 20),
    cube([-4.0, 21.0, -1.5], [4.0, 23.0, 1.5], BONE, 24),
    cube([-3.5, 18.0, -1.5], [3.5, 19.5, 1.5], BONE, 24),
    cube([-3.0, 15.0, -1.5], [3.0, 16.5, 1.5], BONE, 24),
    cube([-4.0, 12.0, -1.5], [4.0, 13.5, 1.5], BONE, 24),
];
const SKELETON_LEG: &[Cuboid] = &[cube([-1.0, -12.0, -1.0], [1.0, 0.0, 1.0], BONE, 24)];
const SKELETON_ARM: &[Cuboid] = &[cube([-1.0, -12.0, -1.0], [1.0, 0.0, 1.0], BONE, 24)];
const SKELETON_BOW_ARM: &[Cuboid] =
    &[cube([-1.0, -12.0, -1.0], [1.0, 0.0, 1.0], BONE, 24), cube([-0.5, -13.0, -5.0], [0.5, -12.0, 5.0], BOW, 16)];
const SKELETON_HEAD: &[Cuboid] = &[
    cube([-4.0, 0.0, -4.0], [4.0, 8.0, 4.0], BONE, 24),
    cube([-3.0, 3.0, 4.0], [-1.0, 5.0, 4.1], DARK, 0),
    cube([1.0, 3.0, 4.0], [3.0, 5.0, 4.1], DARK, 0),
    cube([-0.5, 2.0, 4.0], [0.5, 3.0, 4.1], DARK, 0),
    cube([-3.0, 1.0, 4.0], [3.0, 1.5, 4.05], DARK, 0),
];

// ---------------------------------------------------------------- creeper

const CREEPER: Rgb = [94, 172, 80];
const CREEPER_FACE: Rgb = [20, 26, 20];

const CREEPER_BODY: &[Cuboid] = &[cube([-4.0, 6.0, -2.0], [4.0, 18.0, 2.0], CREEPER, 70)];
const CREEPER_LEG: &[Cuboid] = &[cube([-2.0, -6.0, -2.0], [2.0, 0.0, 2.0], CREEPER, 70)];
const CREEPER_HEAD: &[Cuboid] = &[
    cube([-4.0, 0.0, -4.0], [4.0, 8.0, 4.0], CREEPER, 70),
    cube([-3.0, 4.0, 4.0], [-1.0, 6.0, 4.1], CREEPER_FACE, 0),
    cube([1.0, 4.0, 4.0], [3.0, 6.0, 4.1], CREEPER_FACE, 0),
    cube([-1.0, 1.0, 4.0], [1.0, 4.0, 4.1], CREEPER_FACE, 0),
    cube([-2.0, 0.0, 4.0], [-1.0, 3.0, 4.1], CREEPER_FACE, 0),
    cube([1.0, 0.0, 4.0], [2.0, 3.0, 4.1], CREEPER_FACE, 0),
];

// ---------------------------------------------------------------- spider

const SPIDER: Rgb = [52, 44, 40];
const SPIDER_BACK: Rgb = [64, 52, 46];
const SPIDER_EYES: Rgb = [230, 30, 24];
/// Leg length (pixels) and droop; the pivot height puts the tips on the ground.
const SPIDER_LEG_LEN: f32 = 14.0;
const SPIDER_LEG_DROOP: f32 = std::f32::consts::FRAC_PI_6;

const SPIDER_BODY: &[Cuboid] = &[
    cube([-3.0, 4.0, -3.0], [3.0, 10.0, 3.0], SPIDER, 30),
    cube([-5.0, 5.0, -15.0], [5.0, 14.0, -3.0], SPIDER_BACK, 40),
];
const SPIDER_LEG_R: &[Cuboid] = &[cube([0.0, -1.0, -1.0], [SPIDER_LEG_LEN, 1.0, 1.0], SPIDER, 30)];
const SPIDER_LEG_L: &[Cuboid] = &[cube([-SPIDER_LEG_LEN, -1.0, -1.0], [0.0, 1.0, 1.0], SPIDER, 30)];
const SPIDER_HEAD: &[Cuboid] = &[
    cube([-4.0, -4.0, 0.0], [4.0, 4.0, 8.0], SPIDER, 30),
    cube([-3.0, 1.0, 8.0], [-1.0, 2.0, 8.1], SPIDER_EYES, 0),
    cube([1.0, 1.0, 8.0], [3.0, 2.0, 8.1], SPIDER_EYES, 0),
    cube([-2.0, 2.5, 8.0], [-1.0, 3.5, 8.1], SPIDER_EYES, 0),
    cube([1.0, 2.5, 8.0], [2.0, 3.5, 8.1], SPIDER_EYES, 0),
];

// ---------------------------------------------------------------- arrow

const ARROW: &[Cuboid] = &[
    cube([-0.5, -0.5, -4.0], [0.5, 0.5, 4.0], [140, 104, 64], 10),
    cube([-0.75, -0.75, 4.0], [0.75, 0.75, 5.5], [150, 150, 150], 10),
    cube([-1.5, -0.2, -4.5], [1.5, 0.2, -2.0], [235, 235, 235], 0),
    cube([-0.2, -1.5, -4.5], [0.2, 1.5, -2.0], [235, 235, 235], 0),
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
        MobKind::Cow => vec![
            part(COW_BODY, [0.0; 3], Quat::IDENTITY),
            part(COW_LEG, [-4.0, 12.0, 6.0], rx(swing)),
            part(COW_LEG, [4.0, 12.0, 6.0], rx(-swing)),
            part(COW_LEG, [-4.0, 12.0, -6.0], rx(-swing)),
            part(COW_LEG, [4.0, 12.0, -6.0], rx(swing)),
            part(COW_HEAD, [0.0, 18.0, 9.0], head),
        ],
        MobKind::Sheep => vec![
            part(SHEEP_BODY, [0.0; 3], Quat::IDENTITY),
            part(SHEEP_LEG, [-3.0, 12.0, 5.0], rx(swing)),
            part(SHEEP_LEG, [3.0, 12.0, 5.0], rx(-swing)),
            part(SHEEP_LEG, [-3.0, 12.0, -5.0], rx(-swing)),
            part(SHEEP_LEG, [3.0, 12.0, -5.0], rx(swing)),
            part(SHEEP_HEAD, [0.0, 16.0, 7.0], head),
        ],
        MobKind::Chicken => {
            // Wings flap while airborne.
            let flap = if m.on_ground { 0.0 } else { ((time * 22.0).sin() * 0.5 + 0.5) * 1.2 };
            let rz = Quat::from_rotation_z;
            vec![
                part(CHICKEN_BODY, [0.0; 3], Quat::IDENTITY),
                part(CHICKEN_LEG, [-1.5, 4.0, 0.0], rx(swing)),
                part(CHICKEN_LEG, [1.5, 4.0, 0.0], rx(-swing)),
                part(CHICKEN_WING_L, [-3.0, 8.5, 0.0], rz(-flap)),
                part(CHICKEN_WING_R, [3.0, 8.5, 0.0], rz(flap)),
                part(CHICKEN_HEAD, [0.0, 8.0, 2.0], head),
            ]
        }
        MobKind::Skeleton => {
            // Bow raised toward the target while hunting, arms swinging otherwise.
            let aiming = m.ai == Ai::Chase;
            let arm = |s: f32| if aiming { rx(-FRAC_PI_2 - m.head_pitch) } else { rx(-swing * s * 0.6) };
            vec![
                part(SKELETON_BODY, [0.0; 3], Quat::IDENTITY),
                part(SKELETON_LEG, [-2.0, 12.0, 0.0], rx(swing)),
                part(SKELETON_LEG, [2.0, 12.0, 0.0], rx(-swing)),
                part(SKELETON_ARM, [-5.0, 22.0, 0.0], arm(1.0)),
                part(SKELETON_BOW_ARM, [5.0, 22.0, 0.0], arm(-1.0)),
                part(SKELETON_HEAD, [0.0, 24.0, 0.0], head),
            ]
        }
        MobKind::Creeper => vec![
            part(CREEPER_BODY, [0.0; 3], Quat::IDENTITY),
            part(CREEPER_LEG, [-2.0, 6.0, 4.0], rx(swing)),
            part(CREEPER_LEG, [2.0, 6.0, 4.0], rx(-swing)),
            part(CREEPER_LEG, [-2.0, 6.0, -4.0], rx(-swing)),
            part(CREEPER_LEG, [2.0, 6.0, -4.0], rx(swing)),
            part(CREEPER_HEAD, [0.0, 18.0, 0.0], head),
        ],
        MobKind::Spider => {
            // Four legs a side, fanned out and drooping onto the ground; the
            // pivot height puts each tip exactly at the feet.
            let pivot_y = SPIDER_LEG_LEN * SPIDER_LEG_DROOP.sin() + SPIDER_LEG_DROOP.cos();
            let mut parts = vec![part(SPIDER_BODY, [0.0; 3], Quat::IDENTITY)];
            for (i, &z) in [2.0f32, 0.7, -0.7, -2.0].iter().enumerate() {
                let fan = 0.6 - i as f32 * 0.4;
                let step = swing * 0.4 * if i % 2 == 0 { 1.0 } else { -1.0 };
                let ry = Quat::from_rotation_y;
                let rz = Quat::from_rotation_z;
                parts.push(part(SPIDER_LEG_R, [3.0, pivot_y, z], ry(-fan + step) * rz(-SPIDER_LEG_DROOP)));
                parts.push(part(SPIDER_LEG_L, [-3.0, pivot_y, z], ry(fan - step) * rz(SPIDER_LEG_DROOP)));
            }
            parts.push(part(SPIDER_HEAD, [0.0, 8.0, 3.0], head));
            parts
        }
        MobKind::ZombifiedPiglin => {
            // Sword raised while angry, swinging down to strike; arms at
            // the sides otherwise.
            let chop = if m.attack_anim > 0.0 { (m.attack_anim / 0.35 * PI).sin() * 0.9 } else { 0.0 };
            let sword = if m.ai == Ai::Chase { rx(-1.1 + chop) } else { rx(-0.4 - swing * 0.5) };
            vec![
                part(PIGLIN_BODY, [0.0; 3], Quat::IDENTITY),
                part(PIGLIN_LEG, [-2.0, 12.0, 0.0], rx(swing)),
                part(PIGLIN_LEG, [2.0, 12.0, 0.0], rx(-swing)),
                part(PIGLIN_ARM, [-6.0, 22.0, 0.0], rx(swing * 0.6)),
                part(PIGLIN_SWORD_ARM, [6.0, 22.0, 0.0], sword),
                part(PIGLIN_HEAD, [0.0, 24.0, 0.0], head),
            ]
        }
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
    alpha: f64,
    out: &mut Vec<EntityVertex>,
) -> usize {
    let mut drawn = 0;
    for m in mobs {
        let rel = (m.previous_pos.lerp(m.pos, alpha) - camera).as_vec3();
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
        let torch = (m.block_light.clamp(0.0, 1.0) * 255.0) as u8;
        // A lit creeper swells and flashes white; burning mobs glow orange.
        let fuse = m.fuse / FUSE_TIME;
        let scale = 1.0 + fuse * 0.18;
        let tint = if m.fuse > 0.0 {
            ([255.0; 3], ((m.fuse * (8.0 + 16.0 * fuse)).sin() * 0.5 + 0.5) * 0.7)
        } else if m.burning {
            (FIRE, 0.3)
        } else {
            (FIRE, 0.0)
        };
        for (pi, p) in pose(m, time).iter().enumerate() {
            let rot = body * p.rot;
            let xf = |v: Vec3| origin + body * (p.pivot + p.rot * v) * scale / 16.0;
            for (ci, c) in p.boxes.iter().enumerate() {
                push_cuboid(out, c, &xf, rot, (light, torch), tint, (pi * 8 + ci) as f32);
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
        push_cuboid(out, &c, &|v: Vec3| rel + v / 16.0, Quat::IDENTITY, ([255, 255, 0, 255], 0), (FIRE, 0.0), 0.0);
    }
}

/// Arrows, pointing along their flight.
pub fn build_arrows(arrows: &[Arrow], camera: DVec3, alpha: f64, out: &mut Vec<EntityVertex>) {
    for a in arrows {
        let rel = (a.previous_pos.lerp(a.pos, alpha) - camera).as_vec3();
        let rot = Quat::from_rotation_arc(Vec3::Z, a.dir.normalize_or(Vec3::Z));
        // Stuck arrows sit with the tip buried.
        let origin = if a.is_stuck() { rel - a.dir * 0.2 } else { rel };
        for (i, c) in ARROW.iter().enumerate() {
            push_cuboid(out, c, &|v: Vec3| origin + rot * v / 16.0, rot, ([230, 0, 0, 0], 0), (FIRE, 0.0), i as f32);
        }
    }
}

/// Explosion smoke: grey cubes that swell, then shrink as they fade.
pub fn build_puffs(puffs: &[Puff], camera: DVec3, alpha: f64, out: &mut Vec<EntityVertex>) {
    for (i, p) in puffs.iter().enumerate() {
        let t = p.age / p.life;
        let size = p.size * 16.0 * (t * 4.0).min(1.0) * (1.0 - t);
        let grey = (230.0 - 120.0 * t) as u8;
        let c = Cuboid { min: [-size / 2.0; 3], max: [size / 2.0; 3], color: [grey; 3], noise: 30 };
        let rel = (p.previous_pos.lerp(p.pos, alpha) - camera).as_vec3();
        // Emissive while hot, so the flash reads at night too.
        let glow = ((1.0 - t * 3.0).max(0.0) * 255.0) as u8;
        push_cuboid(
            out,
            &c,
            &|v: Vec3| rel + v / 16.0,
            Quat::IDENTITY,
            ([255, 255, 0, glow], 0),
            (FIRE, 0.0),
            i as f32,
        );
    }
}

const FIRE: [f32; 3] = [255.0, 120.0, 30.0];

/// Appends one cuboid; `tint` blends its colour toward a colour by an amount.
fn push_cuboid(
    out: &mut Vec<EntityVertex>,
    c: &Cuboid,
    xf: &impl Fn(Vec3) -> Vec3,
    rot: Quat,
    (light, torch): ([u8; 4], u8),
    tint: ([f32; 3], f32),
    seed: f32,
) {
    let (min, max) = (Vec3::from_array(c.min), Vec3::from_array(c.max));
    let (to, amount) = tint;
    let ch = |i: usize| (c.color[i] as f32 + (to[i] - c.color[i] as f32) * amount) as u8;
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
                    torch: [torch, 0, 0, 0],
                });
            }
        }
    }
}

/// Visible hosted player avatar, using the same box model pass and lighting as mobs.
pub fn build_player(
    player: &crate::player::Player,
    position: DVec3,
    camera: DVec3,
    sky: f32,
    torch: f32,
    time: f32,
    out: &mut Vec<EntityVertex>,
) {
    const HEAD: &[Cuboid] = &[
        cube([-4.0, 0.0, -4.0], [4.0, 8.0, 4.0], [186, 132, 94], 18),
        cube([-3.0, 3.0, 4.0], [-1.0, 4.0, 4.1], [50, 60, 90], 0),
        cube([1.0, 3.0, 4.0], [3.0, 4.0, 4.1], [50, 60, 90], 0),
    ];
    const BODY: &[Cuboid] = &[cube([-4.0, 12.0, -2.0], [4.0, 24.0, 2.0], [28, 154, 176], 26)];
    const ARM: &[Cuboid] = &[cube([-2.0, -10.0, -2.0], [2.0, 2.0, 2.0], [186, 132, 94], 18)];
    const LEG: &[Cuboid] = &[cube([-2.0, -12.0, -2.0], [2.0, 0.0, 2.0], [48, 57, 120], 26)];
    let swing = (time * 9.0).sin() * (player.vel.with_y(0.0).length() as f32 / 5.0).min(1.0) * 0.6;
    let parts = [
        part(BODY, [0.0; 3], Quat::IDENTITY),
        part(HEAD, [0.0, 24.0, 0.0], Quat::from_rotation_x(-player.pitch)),
        part(ARM, [-6.0, 22.0, 0.0], Quat::from_rotation_x(swing)),
        part(ARM, [6.0, 22.0, 0.0], Quat::from_rotation_x(-swing)),
        part(LEG, [-2.0, 12.0, 0.0], Quat::from_rotation_x(-swing)),
        part(LEG, [2.0, 12.0, 0.0], Quat::from_rotation_x(swing)),
    ];
    let origin = (position - camera).as_vec3();
    let body = Quat::from_rotation_y(FRAC_PI_2 - player.yaw);
    let light = [(sky.clamp(0.0, 1.0) * 255.0) as u8, 0, 0, 0];
    for (i, p) in parts.iter().enumerate() {
        for c in p.boxes {
            push_cuboid(
                out,
                c,
                &|v| origin + body * (p.pivot + p.rot * v) / 16.0,
                body * p.rot,
                (light, (torch.clamp(0.0, 1.0) * 255.0) as u8),
                (FIRE, 0.0),
                i as f32,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolated_models_leave_simulation_positions_unchanged() {
        let mut mob = Mob::new(MobKind::Pig, DVec3::new(10.0, 64.0, 0.0), 0.0);
        mob.pos.x += 2.0;
        let mut halfway = Vec::new();
        let mut current = Vec::new();
        let camera = DVec3::new(0.0, 64.0, 0.0);
        build(std::slice::from_ref(&mob), camera, Vec3::X, 100.0, 0.0, 0.5, &mut halfway);
        build(std::slice::from_ref(&mob), camera, Vec3::X, 100.0, 0.0, 1.0, &mut current);
        assert!(!current.is_empty());
        assert_eq!(halfway.len(), current.len());
        for (a, b) in halfway.iter().zip(&current) {
            assert!((b.pos[0] - a.pos[0] - 1.0).abs() < 1e-5);
            assert_eq!(a.pos[1..], b.pos[1..]);
        }
        assert_eq!(mob.pos, DVec3::new(12.0, 64.0, 0.0));
        assert_eq!(mob.previous_pos, DVec3::new(10.0, 64.0, 0.0));
    }

    #[test]
    fn models_face_their_yaw_and_stand_on_the_ground() {
        for kind in MobKind::ALL {
            // Facing +X (yaw 0): the head is the part furthest along +X.
            let mob = Mob::new(kind, DVec3::new(10.0, 64.0, 0.0), 0.0);
            let mut out = Vec::new();
            assert_eq!(
                build(std::slice::from_ref(&mob), DVec3::new(0.0, 64.0, 0.0), Vec3::X, 100.0, 0.0, 1.0, &mut out),
                1
            );
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
                build(std::slice::from_ref(&mob), DVec3::new(0.0, 64.0, 0.0), Vec3::NEG_X, 100.0, 0.0, 1.0, &mut out),
                0
            );
        }
    }
}
