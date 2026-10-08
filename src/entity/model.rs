//! Minecraft-style box models, animated and flattened into camera-relative
//! triangles for the entity render pass.
//!
//! Models are authored in 1/16-block "pixels" with the origin at the feet,
//! +Y up and +Z forward. Each part has a pivot (in model space) that it
//! rotates around; its cuboids are given relative to that pivot.

use std::f32::consts::{FRAC_PI_2, PI};

use glam::{DVec3, Quat, Vec3};

use super::mob::{Ai, FUSE_TIME, HURT_TIME, Mob, MobKind};
use super::{Arrow, Puff, XpOrb};
use crate::simulation::experience;
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

pub(super) type Rgb = [u8; 3];

#[derive(Clone, Copy)]
pub(super) struct Cuboid {
    min: [f32; 3],
    max: [f32; 3],
    color: Rgb,
    /// Per-texel brightness noise, 0..255 (255 = ±50%).
    noise: u8,
}

pub(super) const fn cube(min: [f32; 3], max: [f32; 3], color: Rgb, noise: u8) -> Cuboid {
    Cuboid { min, max, color, noise }
}

const MAGMA_BODY: &[Cuboid] = &[cube([-4.08, 0.0, -4.08], [4.08, 8.16, 4.08], [50, 28, 25], 80)];
const MAGMA_GLOW: &[Cuboid] = &[
    cube([-4.1, 1.8, -4.1], [4.1, 2.1, 4.1], [245, 99, 16], 0),
    cube([-4.1, 3.8, -4.1], [4.1, 4.1, 4.1], [245, 99, 16], 0),
    cube([-4.1, 5.8, -4.1], [4.1, 6.1, 4.1], [245, 99, 16], 0),
    cube([-2.6, 4.5, 4.1], [-1.1, 6.0, 4.15], [255, 205, 55], 0),
    cube([1.1, 4.5, 4.1], [2.6, 6.0, 4.15], [255, 205, 55], 0),
];
const GHAST_BODY: &[Cuboid] = &[
    cube([-18.0, 24.0, -18.0], [18.0, 64.0, 18.0], [236, 236, 236], 16),
    cube([-7.0, 42.0, 18.0], [-2.0, 48.0, 18.5], [176, 32, 32], 0),
    cube([2.0, 42.0, 18.0], [7.0, 48.0, 18.5], [176, 32, 32], 0),
    cube([-3.5, 34.0, 18.0], [3.5, 38.0, 18.4], [120, 36, 40], 0),
];
const GHAST_TENTACLE: &[Cuboid] = &[cube([-2.0, -24.0, -2.0], [2.0, 0.0, 2.0], [214, 214, 214], 12)];
const SLIME_BODY: &[Cuboid] = &[
    cube([-4.08, 0.0, -4.08], [4.08, 8.16, 4.08], [105, 180, 78], 55),
    cube([-2.6, 4.5, 4.1], [-1.1, 6.0, 4.15], [29, 60, 24], 0),
    cube([1.1, 4.5, 4.1], [2.6, 6.0, 4.15], [29, 60, 24], 0),
    cube([-1.1, 2.4, 4.1], [1.1, 3.4, 4.15], [29, 60, 24], 0),
];

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

/// Zombie-shaped boxes in another palette.
macro_rules! zombie_boxes {
    ($body:ident, $leg:ident, $arm:ident, $head:ident, $skin:expr, $shirt:expr, $pants:expr) => {
        const $body: &[Cuboid] = &[cube([-4.0, 12.0, -2.0], [4.0, 24.0, 2.0], $shirt, 40)];
        const $leg: &[Cuboid] = &[
            cube([-2.0, -10.0, -2.0], [2.0, 0.0, 2.0], $pants, 40),
            cube([-2.0, -12.0, -2.0], [2.0, -10.0, 2.0], ZOMBIE_SHOES, 30),
        ];
        const $arm: &[Cuboid] = &[
            cube([-2.0, -4.0, -2.0], [2.0, 2.0, 2.0], $shirt, 40),
            cube([-2.0, -10.0, -2.0], [2.0, -4.0, 2.0], $skin, 36),
        ];
        const $head: &[Cuboid] = &[
            cube([-4.0, 0.0, -4.0], [4.0, 8.0, 4.0], $skin, 36),
            cube([-3.0, 3.0, 4.0], [-1.0, 4.0, 4.1], ZOMBIE_EYES, 0),
            cube([1.0, 3.0, 4.0], [3.0, 4.0, 4.1], ZOMBIE_EYES, 0),
            cube([-2.0, 1.0, 4.0], [2.0, 1.6, 4.05], ZOMBIE_EYES, 10),
        ];
    };
}
zombie_boxes!(HUSK_BODY, HUSK_LEG, HUSK_ARM, HUSK_HEAD, [150, 128, 92], [118, 100, 70], [86, 72, 50]);
zombie_boxes!(DROWNED_BODY, DROWNED_LEG, DROWNED_ARM, DROWNED_HEAD, [104, 164, 152], [62, 118, 124], [48, 82, 120]);

// ---------------------------------------------------------------- witch

const WITCH_SKIN: Rgb = [142, 168, 118];
const WITCH_ROBE: Rgb = [84, 44, 118];
const WITCH_HAT: Rgb = [38, 28, 52];
const WITCH_NOSE: Rgb = [118, 96, 78];

const WITCH_BODY: &[Cuboid] = &[
    cube([-4.0, 8.0, -3.0], [4.0, 19.0, 3.0], WITCH_ROBE, 36),
    cube([-4.5, 5.0, -3.5], [4.5, 10.0, 3.5], WITCH_ROBE, 36),
    // Crossed arms.
    cube([-7.0, 11.0, 3.0], [7.0, 15.0, 6.0], WITCH_ROBE, 30),
    cube([-1.5, 11.0, 5.5], [1.5, 14.0, 6.5], WITCH_SKIN, 30),
];
const WITCH_LEG: &[Cuboid] = &[cube([-2.0, -8.0, -2.0], [2.0, 0.0, 2.0], WITCH_ROBE, 36)];
const WITCH_HEAD: &[Cuboid] = &[
    cube([-4.0, 0.0, -4.0], [4.0, 8.0, 4.0], WITCH_SKIN, 36),
    cube([-1.0, 1.5, 4.0], [1.0, 4.5, 6.0], WITCH_NOSE, 30),
    cube([0.2, 2.0, 5.5], [1.2, 3.0, 6.4], [90, 130, 80], 0),
    cube([-3.0, 4.5, 4.0], [-1.0, 6.0, 4.1], [30, 40, 24], 0),
    cube([1.0, 4.5, 4.0], [3.0, 6.0, 4.1], [30, 40, 24], 0),
    // The pointed hat.
    cube([-5.0, 8.0, -5.0], [5.0, 9.0, 5.0], WITCH_HAT, 24),
    cube([-3.5, 9.0, -3.5], [3.5, 10.5, 3.5], WITCH_HAT, 24),
    cube([-2.5, 10.5, -2.5], [2.5, 11.5, 2.5], WITCH_HAT, 24),
    cube([-1.5, 11.5, -1.5], [1.5, 12.0, 1.5], WITCH_HAT, 24),
];

// ---------------------------------------------------------------- villager
const VILLAGER_BODY: &[Cuboid] = &[
    cube([-4.0, 8.0, -3.0], [4.0, 24.0, 3.0], [114, 78, 50], 30),
    cube([-4.5, 5.0, -3.5], [4.5, 11.0, 3.5], [98, 66, 44], 30),
    cube([-7.0, 15.0, 3.0], [7.0, 19.0, 6.0], [114, 78, 50], 28),
    cube([-1.5, 15.0, 5.5], [1.5, 18.0, 6.5], [177, 131, 101], 24),
];
const VILLAGER_HEAD: &[Cuboid] = &[
    cube([-4.0, 0.0, -4.0], [4.0, 8.0, 4.0], [177, 131, 101], 24),
    cube([-1.0, 1.0, 4.0], [1.0, 5.0, 6.0], [162, 117, 86], 22),
    cube([-3.0, 4.0, 4.0], [-1.0, 5.0, 4.1], [49, 103, 43], 0),
    cube([1.0, 4.0, 4.0], [3.0, 5.0, 4.1], [49, 103, 43], 0),
    cube([-3.0, 5.5, 4.0], [3.0, 6.5, 4.1], [62, 43, 28], 0),
];
const VILLAGER_LEG: &[Cuboid] = &[cube([-2.0, -8.0, -2.0], [2.0, 0.0, 2.0], [68, 49, 36], 28)];
const IRON_BODY: &[Cuboid] = &[
    cube([-9.0, 0.0, -6.0], [9.0, 18.0, 6.0], [188, 188, 192], 18),
    cube([-4.0, 16.0, 6.0], [4.0, 20.0, 8.0], [160, 160, 164], 12),
];
const IRON_LEG: &[Cuboid] = &[cube([-3.0, -12.0, -3.0], [3.0, 0.0, 3.0], [168, 168, 172], 16)];
const IRON_ARM: &[Cuboid] = &[cube([-3.0, -16.0, -3.0], [3.0, 2.0, 3.0], [176, 176, 180], 16)];
const IRON_HEAD: &[Cuboid] = &[
    cube([-4.0, 0.0, -4.0], [4.0, 10.0, 4.0], [200, 200, 204], 14),
    cube([-1.0, 2.0, 4.0], [1.0, 6.0, 8.0], [150, 150, 154], 10),
    cube([-2.0, 6.0, 4.0], [-0.5, 7.0, 4.2], [120, 36, 32], 0),
    cube([0.5, 6.0, 4.0], [2.0, 7.0, 4.2], [120, 36, 32], 0),
];
const IRON_CRACK: &[Cuboid] = &[cube([-7.0, 4.0, 6.05], [6.0, 5.0, 6.2], [70, 70, 74], 0)];
const SNOW_BALL: &[Cuboid] = &[cube([-5.0, -8.0, -5.0], [5.0, 2.0, 5.0], [244, 248, 252], 12)];
const SNOW_HEAD: &[Cuboid] = &[
    cube([-4.0, 0.0, -4.0], [4.0, 8.0, 4.0], [226, 140, 28], 20),
    cube([-1.0, 2.0, 4.0], [1.0, 5.0, 7.0], [90, 50, 16], 8),
    cube([-2.0, 5.0, 4.0], [-0.6, 6.2, 4.2], [40, 24, 12], 0),
    cube([0.6, 5.0, 4.0], [2.0, 6.2, 4.2], [40, 24, 12], 0),
];
const SNOW_STICK: &[Cuboid] = &[cube([-0.5, -8.0, -0.5], [0.5, 2.0, 0.5], [120, 78, 42], 16)];
const FARMER_HAT: &[Cuboid] = &[
    cube([-6.0, 7.0, -6.0], [6.0, 8.0, 6.0], [213, 177, 88], 25),
    cube([-4.0, 8.0, -4.0], [4.0, 10.0, 4.0], [199, 159, 72], 25),
];
const VILLAGER_APRONS: [[Cuboid; 1]; 15] = {
    let colours = [
        [114, 78, 50],
        [79, 124, 46],
        [155, 121, 51],
        [166, 134, 71],
        [223, 220, 202],
        [139, 106, 73],
        [226, 221, 200],
        [224, 180, 119],
        [135, 71, 149],
        [90, 91, 93],
        [69, 58, 48],
        [86, 66, 52],
        [218, 216, 210],
        [159, 109, 67],
        [204, 191, 180],
    ];
    let mut a = [[cube([-3.5, 8.0, 3.05], [3.5, 15.0, 3.15], [0, 0, 0], 20)]; 15];
    let mut i = 0;
    while i < 15 {
        a[i][0] = cube([-3.5, 8.0, 3.05], [3.5, 15.0, 3.15], colours[i], 20);
        i += 1;
    }
    a
};

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

const SHEARED_BODY: &[Cuboid] = &[cube([-4.0, 11.0, -7.0], [4.0, 18.0, 7.0], SHEEP_SKIN, 14)];
const SHEARED_HEAD: &[Cuboid] = &[cube([-3.0, -3.0, 0.0], [3.0, 3.0, 7.0], SHEEP_SKIN, 14)];
const SHEARED_LEG: &[Cuboid] = &[cube([-2.0, -12.0, -2.0], [2.0, 0.0, 2.0], SHEEP_SKIN, 14)];
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

const WITHER_BONE: Rgb = [58, 58, 60];
const WITHER_RIBS: Rgb = [44, 44, 46];
const STONE_BLADE: Rgb = [128, 128, 128];
const WITHER_BODY: &[Cuboid] = &[
    cube([-1.0, 12.0, -1.0], [1.0, 24.0, 1.0], WITHER_RIBS, 20),
    cube([-4.0, 21.0, -1.5], [4.0, 23.0, 1.5], WITHER_BONE, 24),
    cube([-3.5, 18.0, -1.5], [3.5, 19.5, 1.5], WITHER_BONE, 24),
    cube([-3.0, 15.0, -1.5], [3.0, 16.5, 1.5], WITHER_BONE, 24),
    cube([-4.0, 12.0, -1.5], [4.0, 13.5, 1.5], WITHER_BONE, 24),
];
const WITHER_LIMB: &[Cuboid] = &[cube([-1.0, -12.0, -1.0], [1.0, 0.0, 1.0], WITHER_BONE, 24)];
/// The sword arm holds a stone sword out in front.
const WITHER_SWORD_ARM: &[Cuboid] = &[
    cube([-1.0, -12.0, -1.0], [1.0, 0.0, 1.0], WITHER_BONE, 24),
    cube([-0.5, -13.0, -1.0], [0.5, -12.0, 4.0], BOW, 16),
    cube([-0.5, -13.0, 4.0], [0.5, -9.0, 5.0], STONE_BLADE, 8),
];
const WITHER_HEAD: &[Cuboid] = &[
    cube([-4.0, 0.0, -4.0], [4.0, 8.0, 4.0], WITHER_BONE, 24),
    cube([-3.0, 3.0, 4.0], [-1.0, 5.0, 4.1], DARK, 0),
    cube([1.0, 3.0, 4.0], [3.0, 5.0, 4.1], DARK, 0),
    cube([-0.5, 2.0, 4.0], [0.5, 3.0, 4.1], DARK, 0),
    cube([-3.0, 1.0, 4.0], [3.0, 1.5, 4.05], DARK, 0),
];
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

// ---------------------------------------------------------------- enderman

const ENDER: Rgb = [22, 20, 26];
const ENDER_JAW: Rgb = [14, 12, 18];

const ENDERMAN_BODY: &[Cuboid] = &[cube([-4.0, 28.0, -2.0], [4.0, 40.0, 2.0], ENDER, 18)];
const ENDERMAN_LIMB: &[Cuboid] = &[cube([-1.0, -28.0, -1.0], [1.0, 2.0, 1.0], ENDER, 18)];
const ENDERMAN_HEAD: &[Cuboid] = &[cube([-4.0, 0.0, -4.0], [4.0, 8.0, 4.0], ENDER, 18)];
/// The lower jaw left behind when an angry enderman's head lifts.
const ENDERMAN_JAW: &[Cuboid] = &[cube([-4.2, -0.2, -4.2], [4.2, 3.0, 4.2], ENDER_JAW, 10)];
/// Drawn glowing, like Java's eye layer.
const ENDERMAN_EYES: &[Cuboid] = &[
    cube([-3.5, 3.0, 4.0], [-1.0, 4.0, 4.1], [204, 0, 250], 0),
    cube([-2.5, 3.0, 4.1], [-1.5, 4.0, 4.15], [240, 150, 255], 0),
    cube([1.0, 3.0, 4.0], [3.5, 4.0, 4.1], [204, 0, 250], 0),
    cube([1.5, 3.0, 4.1], [2.5, 4.0, 4.15], [240, 150, 255], 0),
];

// ---------------------------------------------------------------- silverfish

const SILVERFISH: Rgb = [138, 138, 146];
/// Java's seven segments (width, height, depth in pixels), head first.
const fn segment(w: f32, h: f32, d: f32) -> Cuboid {
    cube([-w / 2.0, 0.0, -d / 2.0], [w / 2.0, h, d / 2.0], SILVERFISH, 34)
}
const SILVERFISH_SEGMENTS: [&[Cuboid]; 7] = [
    &[segment(3.0, 2.0, 2.0)],
    &[segment(4.0, 3.0, 2.0)],
    &[segment(6.0, 4.0, 3.0)],
    &[segment(3.0, 3.0, 3.0)],
    &[segment(2.0, 2.0, 3.0)],
    &[segment(2.0, 1.0, 2.0)],
    &[segment(1.0, 1.0, 2.0)],
];
const SILVERFISH_DEPTHS: [f32; 7] = [2.0, 2.0, 3.0, 3.0, 3.0, 2.0, 2.0];

// ---------------------------------------------------------------- blaze

const BLAZE_SKIN: Rgb = [236, 172, 42];
const BLAZE_ROD: Rgb = [252, 206, 70];
const BLAZE_FACE: Rgb = [92, 40, 10];

const BLAZE_HEAD: &[Cuboid] = &[
    cube([-4.0, 0.0, -4.0], [4.0, 8.0, 4.0], BLAZE_SKIN, 40),
    cube([-3.0, 3.0, 4.0], [-1.0, 4.0, 4.1], BLAZE_FACE, 0),
    cube([1.0, 3.0, 4.0], [3.0, 4.0, 4.1], BLAZE_FACE, 0),
    cube([-2.0, 1.0, 4.0], [2.0, 1.5, 4.05], BLAZE_FACE, 0),
];
const BLAZE_ROD_BOX: &[Cuboid] = &[cube([-1.0, -4.0, -1.0], [1.0, 4.0, 1.0], BLAZE_ROD, 30)];

// ---------------------------------------------------------------- arrow

const ARROW: &[Cuboid] = &[
    cube([-0.5, -0.5, -4.0], [0.5, 0.5, 4.0], [140, 104, 64], 10),
    cube([-0.75, -0.75, 4.0], [0.75, 0.75, 5.5], [150, 150, 150], 10),
    cube([-1.5, -0.2, -4.5], [1.5, 0.2, -2.0], [235, 235, 235], 0),
    cube([-0.2, -1.5, -4.5], [0.2, 1.5, -2.0], [235, 235, 235], 0),
];

/// A part placed in model space.
#[derive(Clone, Copy)]
struct Part {
    boxes: &'static [Cuboid],
    pivot: Vec3,
    rot: Quat,
}

fn part(boxes: &'static [Cuboid], pivot: [f32; 3], rot: Quat) -> Part {
    Part { boxes, pivot: Vec3::from_array(pivot), rot }
}

// Fixed-capacity poses keep every mob, including villagers, allocation-free per frame.
struct Parts {
    data: [Part; 16],
    len: usize,
}
impl Parts {
    fn new() -> Self {
        Self { data: [Part { boxes: &[], pivot: Vec3::ZERO, rot: Quat::IDENTITY }; 16], len: 0 }
    }
    fn push(&mut self, p: Part) {
        self.data[self.len] = p;
        self.len += 1;
    }
}
impl FromIterator<Part> for Parts {
    fn from_iter<T: IntoIterator<Item = Part>>(items: T) -> Self {
        let mut out = Self::new();
        for p in items {
            out.push(p);
        }
        out
    }
}
impl std::ops::Deref for Parts {
    type Target = [Part];
    fn deref(&self) -> &[Part] {
        &self.data[..self.len]
    }
}
macro_rules! parts {($($p:expr),* $(,)?) => {{let mut out=Parts::new();$(out.push($p);)*out}}}

/// Animated parts for a mob, in model space (pixels).
fn pose(m: &Mob, time: f32) -> Parts {
    let swing = m.limb_phase.sin() * m.limb_amp * 0.9;
    let head = Quat::from_rotation_y(-m.head_yaw) * Quat::from_rotation_x(-m.head_pitch);
    let rx = Quat::from_rotation_x;
    match m.kind {
        MobKind::Pig => parts![
            part(PIG_BODY, [0.0; 3], Quat::IDENTITY),
            part(PIG_LEG_BOX, [-3.0, 6.0, 5.0], rx(swing)),
            part(PIG_LEG_BOX, [3.0, 6.0, 5.0], rx(-swing)),
            part(PIG_LEG_BOX, [-3.0, 6.0, -5.0], rx(-swing)),
            part(PIG_LEG_BOX, [3.0, 6.0, -5.0], rx(swing)),
            part(PIG_HEAD, [0.0, 12.0, 8.0], head),
        ],
        MobKind::Cow => parts![
            part(COW_BODY, [0.0; 3], Quat::IDENTITY),
            part(COW_LEG, [-4.0, 12.0, 6.0], rx(swing)),
            part(COW_LEG, [4.0, 12.0, 6.0], rx(-swing)),
            part(COW_LEG, [-4.0, 12.0, -6.0], rx(-swing)),
            part(COW_LEG, [4.0, 12.0, -6.0], rx(swing)),
            part(COW_HEAD, [0.0, 18.0, 9.0], head),
        ],
        MobKind::Sheep => parts![
            part(if m.sheared { SHEARED_BODY } else { SHEEP_BODY }, [0.0; 3], Quat::IDENTITY),
            part(if m.sheared { SHEARED_LEG } else { SHEEP_LEG }, [-3.0, 12.0, 5.0], rx(swing)),
            part(if m.sheared { SHEARED_LEG } else { SHEEP_LEG }, [3.0, 12.0, 5.0], rx(-swing)),
            part(if m.sheared { SHEARED_LEG } else { SHEEP_LEG }, [-3.0, 12.0, -5.0], rx(-swing)),
            part(if m.sheared { SHEARED_LEG } else { SHEEP_LEG }, [3.0, 12.0, -5.0], rx(swing)),
            part(if m.sheared { SHEARED_HEAD } else { SHEEP_HEAD }, [0.0, 16.0, 7.0], head),
        ],
        MobKind::Chicken => {
            // Wings flap while airborne.
            let flap = if m.on_ground { 0.0 } else { ((time * 22.0).sin() * 0.5 + 0.5) * 1.2 };
            let rz = Quat::from_rotation_z;
            parts![
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
            parts![
                part(SKELETON_BODY, [0.0; 3], Quat::IDENTITY),
                part(SKELETON_LEG, [-2.0, 12.0, 0.0], rx(swing)),
                part(SKELETON_LEG, [2.0, 12.0, 0.0], rx(-swing)),
                part(SKELETON_ARM, [-5.0, 22.0, 0.0], arm(1.0)),
                part(SKELETON_BOW_ARM, [5.0, 22.0, 0.0], arm(-1.0)),
                part(SKELETON_HEAD, [0.0, 24.0, 0.0], head),
            ]
        }
        MobKind::WitherSkeleton => {
            // Sword raised toward the target while hunting.
            let striking = m.ai == Ai::Chase;
            let arm = |s: f32| if striking { rx(-FRAC_PI_2 * 0.9 + m.attack_anim * 1.2) } else { rx(-swing * s * 0.6) };
            parts![
                part(WITHER_BODY, [0.0; 3], Quat::IDENTITY),
                part(WITHER_LIMB, [-2.0, 12.0, 0.0], rx(swing)),
                part(WITHER_LIMB, [2.0, 12.0, 0.0], rx(-swing)),
                part(WITHER_LIMB, [-5.0, 22.0, 0.0], arm(1.0)),
                part(WITHER_SWORD_ARM, [5.0, 22.0, 0.0], arm(-1.0)),
                part(WITHER_HEAD, [0.0, 24.0, 0.0], head),
            ]
        }
        MobKind::Villager => {
            let profession = m.villager.as_ref().map_or(super::villager::Profession::None, |v| v.profession);
            let mut parts = parts![
                part(VILLAGER_BODY, [0.0; 3], Quat::IDENTITY),
                part(VILLAGER_LEG, [-2.0, 8.0, 0.0], rx(swing)),
                part(VILLAGER_LEG, [2.0, 8.0, 0.0], rx(-swing)),
                part(VILLAGER_HEAD, [0.0, 24.0, 0.0], head),
                part(&VILLAGER_APRONS[profession as usize], [0.0; 3], Quat::IDENTITY)
            ];
            if profession == super::villager::Profession::Farmer {
                parts.push(part(FARMER_HAT, [0.0, 24.0, 0.0], head));
            }
            parts
        }
        MobKind::IronGolem => {
            // Cracks at Java's 75 / 50 / 25 percent health.
            let cracks = if m.health > 75.0 {
                0
            } else if m.health > 50.0 {
                1
            } else if m.health > 25.0 {
                2
            } else {
                3
            };
            let mut parts = parts![
                part(IRON_BODY, [0.0, 12.0, 0.0], Quat::IDENTITY),
                part(IRON_LEG, [-4.0, 12.0, 0.0], rx(swing)),
                part(IRON_LEG, [4.0, 12.0, 0.0], rx(-swing)),
                part(IRON_ARM, [-11.0, 28.0, 0.0], rx(0.4 + swing * 0.3)),
                part(IRON_ARM, [11.0, 28.0, 0.0], rx(0.4 - swing * 0.3)),
                part(IRON_HEAD, [0.0, 32.0, 2.0], head),
            ];
            for i in 0..cracks {
                let y = 16.0 - i as f32 * 4.0;
                parts.push(part(IRON_CRACK, [0.0, y, 0.0], Quat::IDENTITY));
            }
            parts
        }
        MobKind::SnowGolem => parts![
            part(SNOW_BALL, [0.0, 8.0, 0.0], Quat::IDENTITY),
            part(SNOW_BALL, [0.0, 16.0, 0.0], Quat::IDENTITY),
            part(SNOW_HEAD, [0.0, 22.0, 0.0], head),
            part(SNOW_STICK, [-6.0, 16.0, 0.0], rx(0.6)),
            part(SNOW_STICK, [6.0, 16.0, 0.0], rx(0.6)),
        ],
        MobKind::Witch => parts![
            part(WITCH_BODY, [0.0; 3], Quat::IDENTITY),
            part(WITCH_LEG, [-2.0, 8.0, 0.0], rx(swing)),
            part(WITCH_LEG, [2.0, 8.0, 0.0], rx(-swing)),
            part(WITCH_HEAD, [0.0, 19.0, 0.0], head),
        ],
        MobKind::Creeper => parts![
            part(CREEPER_BODY, [0.0; 3], Quat::IDENTITY),
            part(CREEPER_LEG, [-2.0, 6.0, 4.0], rx(swing)),
            part(CREEPER_LEG, [2.0, 6.0, 4.0], rx(-swing)),
            part(CREEPER_LEG, [-2.0, 6.0, -4.0], rx(-swing)),
            part(CREEPER_LEG, [2.0, 6.0, -4.0], rx(swing)),
            part(CREEPER_HEAD, [0.0, 18.0, 0.0], head),
        ],
        MobKind::MagmaCube => {
            parts![part(MAGMA_BODY, [0.0; 3], Quat::IDENTITY), part(MAGMA_GLOW, [0.0; 3], Quat::IDENTITY)]
        }
        MobKind::Ghast => {
            // Nine-tenths of the body is the cube; eight tentacles hang to
            // the feet and twist around their own axis so they stay on the ground.
            let mut parts = parts![part(GHAST_BODY, [0.0; 3], Quat::IDENTITY)];
            for i in 0..8 {
                let a = i as f32 * FRAC_PI_2 * 0.5;
                let twist = (time * 1.4 + i as f32 * 0.7).sin() * 0.45;
                parts.push(part(GHAST_TENTACLE, [a.cos() * 12.0, 24.0, a.sin() * 12.0], Quat::from_rotation_y(twist)));
            }
            parts
        }
        MobKind::Slime => parts![part(SLIME_BODY, [0.0; 3], Quat::IDENTITY)],
        MobKind::Spider | MobKind::CaveSpider => {
            // Four legs a side, fanned out and drooping onto the ground; the
            // pivot height puts each tip exactly at the feet.
            let pivot_y = SPIDER_LEG_LEN * SPIDER_LEG_DROOP.sin() + SPIDER_LEG_DROOP.cos();
            let mut parts = parts![part(SPIDER_BODY, [0.0; 3], Quat::IDENTITY)];
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
            parts![
                part(PIGLIN_BODY, [0.0; 3], Quat::IDENTITY),
                part(PIGLIN_LEG, [-2.0, 12.0, 0.0], rx(swing)),
                part(PIGLIN_LEG, [2.0, 12.0, 0.0], rx(-swing)),
                part(PIGLIN_ARM, [-6.0, 22.0, 0.0], rx(swing * 0.6)),
                part(PIGLIN_SWORD_ARM, [6.0, 22.0, 0.0], sword),
                part(PIGLIN_HEAD, [0.0, 24.0, 0.0], head),
            ]
        }
        MobKind::Silverfish => {
            // Segments head to tail, wiggling side to side like Java's.
            let mut z = SILVERFISH_DEPTHS.iter().sum::<f32>() / 2.0;
            let t = time * 14.0 + m.limb_phase;
            SILVERFISH_SEGMENTS
                .iter()
                .zip(SILVERFISH_DEPTHS)
                .enumerate()
                .map(|(i, (&boxes, d))| {
                    z -= d / 2.0;
                    let k = (i as f32 - 2.0).abs();
                    let sway = (t + i as f32 * 0.47).sin();
                    let at = [sway * 0.6 * k * 0.5, 0.0, z];
                    z -= d / 2.0;
                    part(boxes, at, Quat::from_rotation_y(sway * 0.16 * (1.0 + k)))
                })
                .collect()
        }
        MobKind::Blaze => {
            // Java's three rings of four rods: the top two turn one way,
            // the bottom the other, each bobbing on its own phase.
            let mut parts = parts![part(BLAZE_HEAD, [0.0, 20.0, 0.0], head)];
            let t = time * 2.0 + m.limb_phase * 0.1;
            for (ring, (radius, y, speed)) in
                [(9.0, 15.0, 1.0f32), (7.0, 8.0, 1.0), (5.0, 2.0, -1.0)].into_iter().enumerate()
            {
                for i in 0..4 {
                    let a = t * speed + i as f32 * FRAC_PI_2 + ring as f32 * 0.4;
                    let bob = (t * 1.5 + i as f32 + ring as f32 * 2.0).cos() * 1.5;
                    parts.push(part(
                        BLAZE_ROD_BOX,
                        [a.cos() * radius, y + 4.0 + bob, a.sin() * radius],
                        Quat::IDENTITY,
                    ));
                }
            }
            parts
        }
        MobKind::Enderman => {
            // Long, slow strides; an angry one opens its jaw (the head
            // lifts) and holds its arms a little forward.
            let angry = m.ai == Ai::Chase;
            let stride = swing * 0.5;
            let lift = if angry { 3.0 } else { 0.0 };
            let arm = |s: f32| rx(if angry { -0.35 } else { 0.0 } - stride * s);
            let mut parts = parts![
                part(ENDERMAN_BODY, [0.0; 3], Quat::IDENTITY),
                part(ENDERMAN_LIMB, [-2.0, 28.0, 0.0], rx(stride)),
                part(ENDERMAN_LIMB, [2.0, 28.0, 0.0], rx(-stride)),
                part(ENDERMAN_LIMB, [-5.0, 38.0, 0.0], arm(1.0)),
                part(ENDERMAN_LIMB, [5.0, 38.0, 0.0], arm(-1.0)),
                part(ENDERMAN_HEAD, [0.0, 40.0 + lift, 0.0], head),
                part(ENDERMAN_EYES, [0.0, 40.0 + lift, 0.0], head),
            ];
            if angry {
                parts.push(part(ENDERMAN_JAW, [0.0, 40.0, 0.0], head));
            }
            parts
        }
        MobKind::Zombie | MobKind::Husk | MobKind::Drowned => {
            let (body, leg, arm_box, head_box) = match m.kind {
                MobKind::Husk => (HUSK_BODY, HUSK_LEG, HUSK_ARM, HUSK_HEAD),
                MobKind::Drowned => (DROWNED_BODY, DROWNED_LEG, DROWNED_ARM, DROWNED_HEAD),
                _ => (ZOMBIE_BODY, ZOMBIE_LEG, ZOMBIE_ARM, ZOMBIE_HEAD),
            };
            // Arms held forward, bobbing a little, chopping down on attack.
            let chop = if m.attack_anim > 0.0 { (m.attack_anim / 0.35 * PI).sin() * 0.7 } else { 0.0 };
            let bob = (time * 1.6).sin() * 0.05;
            let arm = |s: f32| rx(-FRAC_PI_2 + bob * s + swing * 0.25 * s + chop);
            parts![
                part(body, [0.0; 3], Quat::IDENTITY),
                part(leg, [-2.0, 12.0, 0.0], rx(swing)),
                part(leg, [2.0, 12.0, 0.0], rx(-swing)),
                part(arm_box, [-6.0, 22.0, 0.0], arm(1.0)),
                part(arm_box, [6.0, 22.0, 0.0], arm(-1.0)),
                part(head_box, [0.0, 24.0, 0.0], head),
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
        let asleep = m.villager.as_ref().is_some_and(|v| v.sleeping);
        let death = if asleep { 1.0 } else { m.death_progress() };
        let body = Quat::from_rotation_y(FRAC_PI_2 - m.yaw) * Quat::from_rotation_z(death * FRAC_PI_2);
        let origin = rel + Vec3::Y * (m.shape().half_width as f32 * death);
        let hurt = if m.dying.is_some() { 1.0 } else { m.hurt / HURT_TIME };
        let light = [(m.sky_light.clamp(0.0, 1.0) * 255.0) as u8, 0, (hurt.clamp(0.0, 1.0) * 255.0) as u8, 0];
        let torch = (m.block_light.clamp(0.0, 1.0) * 255.0) as u8;
        // A lit creeper swells and flashes white; burning mobs glow orange.
        let fuse = m.fuse / FUSE_TIME;
        let scale = (1.0 + fuse * 0.18)
            * if m.kind == MobKind::CaveSpider {
                0.55
            } else if m.kind.is_cube() {
                m.size as f32
            } else if m.kind == MobKind::WitherSkeleton {
                1.2
            } else if m.baby || m.age < 0 {
                0.5
            } else {
                1.0
            };
        let tint = if m.fuse > 0.0 {
            ([255.0; 3], ((m.fuse * (8.0 + 16.0 * fuse)).sin() * 0.5 + 0.5) * 0.7)
        } else if m.kind == MobKind::CaveSpider {
            ([40.0, 80.0, 95.0], 0.65)
        } else if m.kind == MobKind::Ghast && m.charged {
            ([210.0, 48.0, 36.0], 0.55)
        } else if m.burning {
            (FIRE, 0.3)
        } else {
            (FIRE, 0.0)
        };
        let posed = pose(m, time);
        for (pi, p) in posed.iter().enumerate() {
            let rot = body * p.rot;
            let xf = |v: Vec3| origin + body * (p.pivot + p.rot * v) * scale / 16.0;
            // Endermen eyes and blazes glow at full brightness.
            let glow =
                std::ptr::eq(p.boxes, ENDERMAN_EYES) || std::ptr::eq(p.boxes, MAGMA_GLOW) || m.kind == MobKind::Blaze;
            let light = if glow { [light[0], 0, light[2], 255] } else { light };
            for (ci, c) in p.boxes.iter().enumerate() {
                let mut cuboid = *c;
                if m.kind == MobKind::Sheep && c.color == WOOL {
                    cuboid.color = if m.sheared { SHEEP_SKIN } else { m.wool_color.sheep_rgb() };
                    if m.sheared {
                        cuboid.noise = 14;
                    }
                }
                push_cuboid(out, &cuboid, &xf, rot, (light, torch), tint, (pi * 8 + ci) as f32);
            }
        }
        if m.kind.is_zombie() || m.kind == MobKind::Skeleton {
            let limbs = humanoid_armor(&posed);
            let worn = super::armor::worn_pieces(m.armor, m.armor_glint);
            super::player_model::draw_armor(out, &limbs, &worn, origin, body, scale, (light, torch));
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

/// Thrown snowballs and eggs: small cubes that tumble with no facing.
pub fn build_thrown(thrown: &[super::thrown::Thrown], camera: DVec3, alpha: f64, out: &mut Vec<EntityVertex>) {
    for t in thrown {
        let rel = (t.previous_pos.lerp(t.pos, alpha) - camera).as_vec3();
        let (color, size) = match t.kind {
            super::thrown::Kind::Snowball => ([245, 245, 250], 2.0),
            super::thrown::Kind::Egg => ([236, 224, 196], 2.2),
        };
        let c = cube([-size, -size, -size], [size, size, size], color, 16);
        push_cuboid(out, &c, &|v: Vec3| rel + v / 16.0, Quat::IDENTITY, ([230, 0, 0, 0], 0), (FIRE, 0.0), 0.0);
    }
}

/// A fishing bobber: a white float with a red tip. It dips while a fish bites.
pub fn build_bobbers(bobbers: &[super::bobber::Bobber], camera: DVec3, alpha: f64, out: &mut Vec<EntityVertex>) {
    for b in bobbers {
        let mut rel = (b.previous_pos.lerp(b.pos, alpha) - camera).as_vec3();
        if b.biting {
            rel.y -= 0.25;
        }
        let body = cube([-1.2, -1.2, -1.2], [1.2, 0.4, 1.2], [236, 236, 232], 12);
        let tip = cube([-0.7, 0.4, -0.7], [0.7, 1.5, 0.7], [176, 40, 36], 8);
        for (i, c) in [&body, &tip].into_iter().enumerate() {
            push_cuboid(out, c, &|v: Vec3| rel + v / 16.0, Quat::IDENTITY, ([230, 0, 0, 0], 0), (FIRE, 0.0), i as f32);
        }
    }
}

/// Thrown ender pearls: small dark teal cubes with a pale glint.
pub fn build_pearls(pearls: &[super::pearl::Pearl], camera: DVec3, alpha: f64, out: &mut Vec<EntityVertex>) {
    const PEARL: &[Cuboid] = &[
        cube([-1.5, -1.5, -1.5], [1.5, 1.5, 1.5], [20, 92, 80], 20),
        cube([-1.6, 0.4, -1.6], [-0.4, 1.6, -0.4], [120, 220, 190], 0),
    ];
    for p in pearls {
        let rel = (p.previous_pos.lerp(p.pos, alpha) - camera).as_vec3();
        for (i, c) in PEARL.iter().enumerate() {
            push_cuboid(out, c, &|v: Vec3| rel + v / 16.0, Quat::IDENTITY, ([230, 0, 0, 0], 0), (FIRE, 0.0), i as f32);
        }
    }
}

/// Thrown splash potions: a small tumbling flask tinted like its liquid.
pub fn build_potions(potions: &[super::potion::ThrownPotion], camera: DVec3, alpha: f64, out: &mut Vec<EntityVertex>) {
    for p in potions {
        let rel = (p.previous_pos.lerp(p.pos, alpha) - camera).as_vec3();
        let colour = p.potion.colour();
        let flask = [
            cube([-2.0, -2.0, -2.0], [2.0, 2.0, 2.0], colour, 16),
            cube([-1.0, 2.0, -1.0], [1.0, 3.5, 1.0], [150, 150, 158], 8),
            cube([-0.8, 3.5, -0.8], [0.8, 4.3, 0.8], [140, 98, 58], 8),
        ];
        let spin = Quat::from_rotation_z(p.pos.x as f32 * 2.0) * Quat::from_rotation_x(p.pos.z as f32 * 2.0);
        for (i, c) in flask.iter().enumerate() {
            push_cuboid(out, c, &|v: Vec3| rel + spin * v / 16.0, spin, ([230, 0, 0, 0], 0), (FIRE, 0.0), i as f32);
        }
    }
}

/// Thrown eyes of ender: a green pearl with a dark pupil, spinning slowly
/// and glowing faintly so it can be followed at night.
pub fn build_eyes(eyes: &[super::eye::EnderEye], camera: DVec3, time: f32, alpha: f64, out: &mut Vec<EntityVertex>) {
    const EYE: &[Cuboid] = &[
        cube([-1.5, -1.5, -1.5], [1.5, 1.5, 1.5], [70, 160, 110], 20),
        cube([-0.5, -1.0, -1.6], [0.5, 1.0, 1.6], [16, 36, 26], 0),
    ];
    let rot = Quat::from_rotation_y(time * 3.0);
    for e in eyes {
        let rel = (e.previous_pos.lerp(e.pos, alpha) - camera).as_vec3();
        for (i, c) in EYE.iter().enumerate() {
            push_cuboid(out, c, &|v: Vec3| rel + rot * v / 16.0, rot, ([230, 0, 0, 60], 0), (FIRE, 0.0), i as f32);
        }
    }
}

/// Blaze fireballs: a glowing orange cube around a yellow core, tumbling.
pub fn build_fireballs(
    fireballs: &[super::fireball::Fireball],
    camera: DVec3,
    time: f32,
    alpha: f64,
    out: &mut Vec<EntityVertex>,
) {
    const BALL: &[Cuboid] = &[
        cube([-2.5, -2.5, -2.5], [2.5, 2.5, 2.5], [250, 120, 20], 60),
        cube([-3.0, -1.5, -1.5], [3.0, 1.5, 1.5], [255, 220, 90], 30),
    ];
    for (i, f) in fireballs.iter().enumerate() {
        let rel = (f.previous_pos.lerp(f.pos, alpha) - camera).as_vec3();
        let rot = Quat::from_rotation_arc(Vec3::X, f.heading().normalize_or(Vec3::X))
            * Quat::from_rotation_x(time * 9.0 + i as f32);
        let scale = if f.is_large() { 3.0 } else { 1.0 };
        for (j, c) in BALL.iter().enumerate() {
            push_cuboid(
                out,
                c,
                &|v: Vec3| rel + rot * v * scale / 16.0,
                rot,
                ([255, 255, 0, 255], 0),
                (FIRE, 0.0),
                j as f32,
            );
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

/// Experience orbs: small glowing cubes, bigger for bigger values, that
/// shimmer between yellow and green like Java's tinted sprites.
pub fn build_orbs(orbs: &[XpOrb], camera: DVec3, max_dist: f32, time: f32, alpha: f64, out: &mut Vec<EntityVertex>) {
    for (i, o) in orbs.iter().enumerate() {
        let pos = o.previous_pos.lerp(o.pos, alpha);
        if pos.distance_squared(camera) > (max_dist as f64).powi(2) {
            continue;
        }
        // Java's sprites span about 4 to 12 texels at 0.3 scale; in model
        // units (1/16 block) that's roughly 1.2 to 3.6.
        let size = 1.2 + 0.24 * experience::orb_icon(o.value) as f32;
        // Java's colour: red follows a sine, green stays full.
        let h = time * 10.0 + o.phase;
        let red = ((h.sin() + 1.0) * 0.5 * 200.0 + 55.0) as u8;
        let c = Cuboid { min: [-size / 2.0; 3], max: [size / 2.0; 3], color: [red, 255, 40], noise: 40 };
        // Bob a little above the ground, and spin.
        let lift = 0.15 + 0.03 * (time * 3.0 + o.phase).sin();
        let rel = (pos - camera).as_vec3() + Vec3::Y * lift;
        let rot = Quat::from_rotation_y(time * 2.0 + o.phase);
        push_cuboid(
            out,
            &c,
            &|v: Vec3| rel + rot * (v / 16.0),
            rot,
            ([255, 255, 0, 220], 0),
            ([0.0; 3], 0.0),
            i as f32,
        );
    }
}

const FIRE: [f32; 3] = [255.0, 120.0, 30.0];

/// Standard humanoid boxes on this mob's pivots, so armor follows the pose
/// and inflates like HumanoidArmorLayer rather than the thin bone mesh.
fn humanoid_armor(parts: &[Part]) -> [super::player_model::Limb; 6] {
    let box_at = |p: &Part, min: [f32; 3], max: [f32; 3], uv: [f32; 2]| super::player_model::Limb {
        pivot: p.pivot,
        rot: p.rot,
        min,
        max,
        uv,
    };
    let arm = |p: &Part, uv: [f32; 2]| box_at(p, [-2.0, -10.0, -2.0], [2.0, 2.0, 2.0], uv);
    let leg = |p: &Part, uv: [f32; 2]| box_at(p, [-2.0, -12.0, -2.0], [2.0, 0.0, 2.0], uv);
    [
        box_at(&parts[5], [-4.0, 0.0, -4.0], [4.0, 8.0, 4.0], [0.0, 0.0]),
        box_at(&parts[0], [-4.0, 12.0, -2.0], [4.0, 24.0, 2.0], [16.0, 16.0]),
        arm(&parts[3], [40.0, 16.0]),
        arm(&parts[4], [32.0, 48.0]),
        leg(&parts[1], [0.0, 16.0]),
        leg(&parts[2], [16.0, 48.0]),
    ]
}

/// Appends one cuboid; `tint` blends its colour toward a colour by an amount.
pub(super) fn push_cuboid(
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

pub use super::player_model::{PlayerAppearance, build_player};
pub use super::player_pose::{PlayerPose, fits_at, resolve_pose};

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
    fn zombies_and_skeletons_wear_inflated_armor() {
        let mut zombie = Mob::new(MobKind::Zombie, DVec3::new(3.0, 0.0, 0.0), 0.0);
        zombie.armor = [Some(crate::entity::armor::ArmorKind::Chain); 4];
        let mut out = Vec::new();
        build(std::slice::from_ref(&zombie), DVec3::ZERO, Vec3::X, 100.0, 0.0, 1.0, &mut out);
        assert!(out.iter().any(|v| v.torch[1] == 3));
        let mut pig = Mob::new(MobKind::Pig, DVec3::new(3.0, 0.0, 0.0), 0.0);
        pig.armor = zombie.armor;
        out.clear();
        build(std::slice::from_ref(&pig), DVec3::ZERO, Vec3::X, 100.0, 0.0, 1.0, &mut out);
        assert!(out.iter().all(|v| v.torch[1] == 0));
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
            // Blazes float above the ground on their rods.
            let floats = kind == MobKind::Blaze;
            assert!(lo.y.abs() < 1e-4 || floats && lo.y < 0.25, "{kind:?} feet at {}", lo.y);
            let h = kind.shape().height as f32;
            assert!((hi.y - h).abs() < 0.15, "{kind:?} top {} vs box {h}", hi.y);
            assert!(hi.x > 10.25, "{kind:?} should extend forward along +X");
            // Behind the camera: culled.
            out.clear();
            assert_eq!(
                build(std::slice::from_ref(&mob), DVec3::new(0.0, 64.0, 0.0), Vec3::NEG_X, 100.0, 0.0, 1.0, &mut out),
                0
            );
        }
    }
}
