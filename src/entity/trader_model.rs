//! Original trader llama geometry (v1.0 gap Track 7e: the trader's clothes moved to `villager_outfit`); fixed arrays keep posing allocation-free.
use super::{Cuboid, cube};
pub(super) const LLAMA_Y: f32 = 1.87 * 16.0 / 41.0;
const fn llama_cube(mut lo: [f32; 3], mut hi: [f32; 3], color: [u8; 3], noise: u8) -> Cuboid {
    lo[1] *= LLAMA_Y;
    hi[1] *= LLAMA_Y;
    cube(lo, hi, color, noise)
}
pub(super) const LLAMA_BODY: &[Cuboid] = &[
    llama_cube([-4.5, 14.0, -7.0], [4.5, 23.0, 7.0], [221, 207, 178], 20),
    llama_cube([-4.7, 21.0, -7.2], [4.7, 23.5, 7.2], [40, 76, 158], 10),
    llama_cube([-4.7, 16.0, -5.0], [-4.5, 22.0, 5.0], [205, 155, 45], 10),
    llama_cube([4.5, 16.0, -5.0], [4.7, 22.0, 5.0], [205, 155, 45], 10),
];
pub(super) const LLAMA_HEAD: &[Cuboid] = &[
    llama_cube([-2.0, 0.0, -2.0], [2.0, 12.0, 2.0], [221, 207, 178], 20),
    llama_cube([-3.0, 10.0, -3.0], [3.0, 16.0, 4.0], [221, 207, 178], 18),
    llama_cube([-3.0, 9.0, 3.0], [3.0, 13.0, 7.0], [221, 207, 178], 18),
    llama_cube([-2.7, 16.0, -1.0], [-1.3, 21.0, 1.0], [221, 207, 178], 16),
    llama_cube([1.3, 16.0, -1.0], [2.7, 21.0, 1.0], [221, 207, 178], 16),
    llama_cube([-3.05, 12.0, 0.5], [-3.0, 14.0, 2.0], [24, 24, 20], 0),
    llama_cube([3.0, 12.0, 0.5], [3.05, 14.0, 2.0], [24, 24, 20], 0),
    // v0.18 Track 6: nostrils, a split lip, eye glints and dark ear tips.
    llama_cube([-1.8, 11.5, 7.0], [-0.8, 12.2, 7.05], [70, 58, 46], 0),
    llama_cube([0.8, 11.5, 7.0], [1.8, 12.2, 7.05], [70, 58, 46], 0),
    llama_cube([-0.2, 9.0, 7.0], [0.2, 11.0, 7.05], [120, 104, 86], 0),
    llama_cube([-3.1, 13.2, 1.4], [-3.05, 13.7, 1.8], [226, 222, 212], 0),
    llama_cube([3.05, 13.2, 1.4], [3.1, 13.7, 1.8], [226, 222, 212], 0),
    llama_cube([-2.75, 19.5, -1.05], [-1.25, 21.05, 1.05], [96, 80, 64], 0),
    llama_cube([1.25, 19.5, -1.05], [2.75, 21.05, 1.05], [96, 80, 64], 0),
];
pub(super) const LLAMA_LEG: &[Cuboid] = &[
    llama_cube([-1.5, -14.0, -1.5], [1.5, 0.0, 1.5], [221, 207, 178], 18),
    llama_cube([-1.5, -14.0, -1.5], [1.5, -12.0, 1.5], [110, 96, 73], 12),
];
