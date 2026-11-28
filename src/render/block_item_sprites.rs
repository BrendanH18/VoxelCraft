//! v1.0 gap track 6b: original 16 × 16 sprites for the block items Java
//! 26.3 draws flat (`content::block_item_sprites`). They follow the item
//! art's rules (light from the top left, a dark rim on the bottom and right)
//! and take their materials from the game's own block textures, so an icon
//! matches the block it places: a door is its two halves' faces narrowed,
//! a lantern and a chain are their block's front face with the lantern's
//! loop handle added, and the rest are drawn from simple shapes in the
//! colours of their block. Only the footprint of each Java sprite (its
//! bounding box, measured from the 26.3 client) was used; no texture was
//! copied or traced.

use std::sync::OnceLock;

use super::item_sprites::{centre, noise, shaded, tint};
use crate::world::block::{Block, Facing};
use voxelcraft::content::block_item_sprites::{self as table, Art};

type Rgba = [u8; 4];
type Tile = [Rgba; 256];

const CLEAR: Rgba = [0, 0, 0, 0];
const STICK: [u8; 3] = [137, 103, 39];
const REDSTONE: [u8; 3] = [214, 34, 24];
const REDSTONE_HOT: [u8; 3] = [255, 132, 104];

/// Pixel `(x, y)` of entry `index`'s sprite.
pub(super) fn pixel(index: u16, x: usize, y: usize) -> Rgba {
    static TILES: OnceLock<Vec<Tile>> = OnceLock::new();
    let tiles = TILES.get_or_init(|| (0..table::ITEMS.len() as u16).map(draw).collect());
    tiles.get(index as usize).map_or(CLEAR, |t| t[(y % 16) * 16 + x % 16])
}

fn draw(index: u16) -> Tile {
    let (Some(art), Some(item)) = (table::art(index), table::item(index)) else { return [CLEAR; 256] };
    if art == Art::Door {
        return door(item);
    }
    let Some(block) = item.block() else { return [CLEAR; 256] };
    match art {
        Art::Door => door(item),
        Art::Lantern => lantern(block),
        Art::Chain => chain(block),
        art => std::array::from_fn(|i| shapes(art, block, (i % 16) as i32, (i / 16) as i32).unwrap_or(CLEAR)),
    }
}

fn texel(layer: u16, x: i32, y: i32) -> Rgba {
    super::textures::pixel(layer, x.clamp(0, 15) as usize, y.clamp(0, 15) as usize)
}

/// The mean colour of a block layer's opaque texels.
fn mean(layer: u16) -> [u8; 3] {
    let (mut sum, mut n) = ([0u32; 3], 0u32);
    for y in 0..16 {
        for x in 0..16 {
            let p = texel(layer, x, y);
            if p[3] >= 128 {
                (0..3).for_each(|c| sum[c] += p[c] as u32);
                n += 1;
            }
        }
    }
    if n == 0 { [128; 3] } else { sum.map(|s| (s / n) as u8) }
}

/// A door: Java's footprint (columns 3–12, rows 2–15), its upper half's
/// face over its lower half's, each texel of the sprite the average of the
/// block texels it covers (clear where most of them are, as in the windows
/// of iron and copper doors).
fn door(item: crate::item::Item) -> Tile {
    let Some([lower, upper]) = voxelcraft::content::placement::door_halves(item, Facing::North) else {
        return [CLEAR; 256];
    };
    // A north-facing door's broad face is its -Z face.
    let (top, bottom) = (upper.info().tex[5], lower.info().tex[5]);
    let (x0, x1, y0, y1) = (3, 13, 2, 16);
    std::array::from_fn(|i| {
        let (x, y) = ((i % 16) as i32, (i / 16) as i32);
        if !(x0..x1).contains(&x) || !(y0..y1).contains(&y) {
            return CLEAR;
        }
        // Texel span of this pixel, in the 16 × 32 two-block face.
        let sx = |p: i32| (p - x0) as f32 * 16.0 / (x1 - x0) as f32;
        let sy = |p: i32| (p - y0) as f32 * 32.0 / (y1 - y0) as f32;
        let (tx0, tx1) = (sx(x).floor() as i32, (sx(x + 1).ceil() as i32).max(sx(x).floor() as i32 + 1));
        let (ty0, ty1) = (sy(y).floor() as i32, (sy(y + 1).ceil() as i32).max(sy(y).floor() as i32 + 1));
        let (mut sum, mut solid, mut all) = ([0u32; 3], 0u32, 0u32);
        for ty in ty0..ty1.min(32) {
            for tx in tx0..tx1.min(16) {
                let p = if ty < 16 { texel(top, tx, ty) } else { texel(bottom, tx, ty - 16) };
                all += 1;
                if p[3] >= 128 {
                    solid += 1;
                    (0..3).for_each(|c| sum[c] += p[c] as u32);
                }
            }
        }
        if solid * 2 < all || solid == 0 {
            return CLEAR;
        }
        let c = sum.map(|s| (s / solid) as u8);
        // The door's outline: a darker rim on the bottom and right, lighter
        // on the top and left, as the item art is lit.
        let f = if x == x1 - 1 || y == y1 - 1 {
            0.72
        } else if x == x0 || y == y0 {
            1.12
        } else {
            1.0
        };
        tint(c, f)
    })
}

/// A lantern: its block's front face (the cage over rows 6–12, the cap over
/// 4–5, as the placed lantern's boxes show them) under a loop handle in the
/// cage's darkest colour.
fn lantern(block: Block) -> Tile {
    let side = block.info().tex[0];
    let dark = darkest(side);
    std::array::from_fn(|i| {
        let (x, y) = ((i % 16) as i32, (i / 16) as i32);
        let body = (5..=10).contains(&x) && (6..=12).contains(&y);
        let cap = (6..=9).contains(&x) && (4..=5).contains(&y);
        if body || cap {
            let p = texel(side, x, y + 3);
            return if p[3] >= 128 { p } else { tint(dark, 1.0) };
        }
        // The handle: a loop over the cap.
        let handle = matches!((x, y), (6 | 9, 2..=3) | (7..=8, 1));
        if handle { tint(dark, if y == 1 { 1.25 } else { 1.0 }) } else { CLEAR }
    })
}

/// A chain: its block's front face (columns 6–8, as the placed chain's box
/// shows it) over Java's rows 1–14, the links' holes see-through.
fn chain(block: Block) -> Tile {
    let side = block.info().tex[0];
    std::array::from_fn(|i| {
        let (x, y) = ((i % 16) as i32, (i / 16) as i32);
        if !(6..=8).contains(&x) || !(1..=14).contains(&y) {
            return CLEAR;
        }
        let p = texel(side, x, y);
        if p[3] >= 128 { p } else { CLEAR }
    })
}

/// The darkest opaque colour of a layer.
fn darkest(layer: u16) -> [u8; 3] {
    let mut best = ([255u8; 3], u32::MAX);
    for y in 0..16 {
        for x in 0..16 {
            let p = texel(layer, x, y);
            let l = p[0] as u32 * 3 + p[1] as u32 * 6 + p[2] as u32;
            if p[3] >= 128 && l < best.1 {
                best = ([p[0], p[1], p[2]], l);
            }
        }
    }
    best.0
}

/// The first of `layers` (front to back) that covers `(x, y)`, shaded at
/// its own edges.
/// A shape, its colour and its grain.
type Layer<'a> = (&'a dyn Fn(i32, i32) -> bool, [u8; 3], f32);

fn first(layers: &[Layer], x: i32, y: i32) -> Option<Rgba> {
    layers.iter().find_map(|&(inside, c, grain)| shaded(inside, x, y, c, grain))
}

fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> impl Fn(i32, i32) -> bool {
    move |x, y| {
        let (px, py) = centre(x, y);
        ((px - cx) / rx).powi(2) + ((py - cy) / ry).powi(2) <= 1.0
    }
}

fn rect(x0: i32, y0: i32, x1: i32, y1: i32) -> impl Fn(i32, i32) -> bool {
    move |x, y| (x0..=x1).contains(&x) && (y0..=y1).contains(&y)
}

fn scale(c: [u8; 3], f: f32) -> [u8; 3] {
    c.map(|v| (v as f32 * f).clamp(0.0, 255.0) as u8)
}

/// The block of a Java id (for another block's colours).
fn block_of(id: &str) -> Option<Block> {
    voxelcraft::java_registry::block(id, "")
}

fn shapes(art: Art, block: Block, x: i32, y: i32) -> Option<Rgba> {
    let tex = block.info().tex;
    match art {
        Art::Cauldron => {
            // A pot seen a little from above: the rim, its dark inside, the
            // body and four stubby legs.
            let iron = mean(tex[0]);
            let rim = rect(1, 2, 14, 3);
            let inside = rect(3, 3, 12, 4);
            let body = |x: i32, y: i32| (2..=13).contains(&x) && (4..=12).contains(&y);
            let legs = |x: i32, y: i32| (13..=14).contains(&y) && ((2..=4).contains(&x) || (11..=13).contains(&x));
            first(
                &[
                    (&inside, scale(iron, 0.32), 0.04),
                    (&rim, iron, 0.05),
                    (&body, iron, 0.06),
                    (&legs, scale(iron, 0.8), 0.05),
                ],
                x,
                y,
            )
        }
        Art::Hopper => {
            // A funnel: the wide bowl with its dark mouth, the narrowing
            // middle and the spout.
            let iron = mean(tex[2]);
            let mouth = rect(3, 3, 12, 4);
            let bowl = rect(1, 2, 14, 7);
            let middle = rect(4, 8, 11, 10);
            let spout = rect(6, 11, 9, 14);
            first(
                &[
                    (&mouth, scale(iron, 0.3), 0.04),
                    (&bowl, iron, 0.05),
                    (&middle, iron, 0.05),
                    (&spout, scale(iron, 0.85), 0.05),
                ],
                x,
                y,
            )
        }
        Art::BrewingStand => {
            // A blaze rod on a stone base, an arm to each side with a bottle
            // hanging from it.
            let stone = block_of("cobblestone").map_or([120; 3], |b| mean(b.info().tex[0]));
            let rod = rect(7, 1, 8, 12);
            let arms = rect(3, 6, 12, 6);
            let base = rect(2, 13, 13, 14);
            let bottle = |x: i32, y: i32| {
                let x = if x > 7 { 15 - x } else { x };
                (2..=3).contains(&x) && (7..=8).contains(&y) || (1..=4).contains(&x) && (9..=12).contains(&y)
            };
            first(
                &[
                    (&rod, [250, 190, 40], 0.06),
                    (&bottle, [196, 214, 230], 0.03),
                    (&arms, [92, 92, 96], 0.04),
                    (&base, stone, 0.08),
                ],
                x,
                y,
            )
        }
        Art::Comparator | Art::Repeater => {
            // The slab seen from the front and a little above, its redstone
            // line on top and its torches standing on it: two for the
            // repeater, two short ones and a tall one for the comparator.
            let slab = block_of("smooth_stone").map_or([160; 3], |b| mean(b.info().tex[2]));
            let top = rect(0, 10, 15, 11);
            let front = rect(0, 12, 15, 14);
            let dust = rect(2, 10, 13, 10);
            let torches: &[(i32, i32)] =
                if art == Art::Repeater { &[(3, 5), (11, 5)] } else { &[(2, 6), (12, 6), (7, 3)] };
            let stick = |x: i32, y: i32| {
                torches.iter().any(|&(tx, ty)| (tx..=tx + 1).contains(&x) && (ty + 2..=9).contains(&y))
            };
            let head = |x: i32, y: i32| {
                torches.iter().any(|&(tx, ty)| (tx..=tx + 1).contains(&x) && (ty..=ty + 1).contains(&y))
            };
            if head(x, y) {
                let hot = torches.iter().any(|&(tx, ty)| (x, y) == (tx, ty));
                return Some(tint(if hot { REDSTONE_HOT } else { REDSTONE }, 1.0));
            }
            first(
                &[
                    (&stick, STICK, 0.05),
                    (&dust, scale(REDSTONE, 0.8), 0.02),
                    (&top, scale(slab, 1.1), 0.05),
                    (&front, slab, 0.06),
                ],
                x,
                y,
            )
        }
        Art::Campfire { soul } => {
            // Two crossed logs, their cut ends showing, under a flame.
            let log = block_of("oak_log").map(|b| b.info().tex);
            let (bark, end) = log.map_or(([96, 74, 44], [172, 140, 88]), |t| (mean(t[0]), mean(t[2])));
            let back = rect(2, 10, 13, 12);
            let front = rect(1, 12, 14, 14);
            let ends = |x: i32, y: i32| (x == 1 || x == 14) && (12..=14).contains(&y);
            let flame = |x: i32, y: i32| {
                let (px, py) = centre(x, y);
                (1.0..=11.0).contains(&py)
                    && (px - 8.0).abs() <= ((py - 1.0) * 0.5).min(4.2) - ((py - 8.0).max(0.0) * 0.7)
            };
            if flame(x, y) && !front(x, y) {
                let (px, py) = centre(x, y);
                let heat = 1.0 - ((px - 8.0).abs() / 4.2).min(1.0) * 0.6 - ((10.0 - py) / 9.0).clamp(0.0, 1.0) * 0.4;
                let c = match (soul, heat) {
                    (false, h) if h > 0.7 => [255, 238, 150],
                    (false, h) if h > 0.45 => [255, 184, 60],
                    (false, _) => [236, 110, 30],
                    (true, h) if h > 0.7 => [200, 250, 255],
                    (true, h) if h > 0.45 => [96, 214, 236],
                    (true, _) => [40, 132, 188],
                };
                return Some(tint(c, 0.96 + noise(x, y, 61) * 0.08));
            }
            first(&[(&ends, end, 0.05), (&front, bark, 0.08), (&back, scale(bark, 0.85), 0.08)], x, y)
        }
        Art::FlowerPot => {
            // A clay pot: the rim with soil inside, the tapering body.
            let clay = mean(tex[0]);
            let soil = rect(4, 4, 11, 4);
            let rim = rect(3, 4, 12, 5);
            let body = |x: i32, y: i32| (6..=12).contains(&y) && (4 + (y - 6) / 4..=11 - (y - 6) / 4).contains(&x);
            first(&[(&soil, [74, 52, 36], 0.08), (&rim, scale(clay, 1.08), 0.05), (&body, clay, 0.06)], x, y)
        }
        Art::Bell => {
            // A bell: the knob, the dome widening to the lip, the clapper.
            let gold = mean(tex[0]);
            let gold = if gold.iter().map(|&c| c as u32).sum::<u32>() < 300 { [236, 190, 60] } else { gold };
            let knob = rect(7, 1, 8, 2);
            let dome = |x: i32, y: i32| {
                let (px, _) = centre(x, y);
                (3..=11).contains(&y) && (px - 8.0).abs() <= 2.0 + (y - 3) as f32 * 0.45
            };
            let lip = rect(2, 12, 13, 12);
            let clapper = rect(7, 13, 8, 14);
            first(
                &[
                    (&knob, scale(gold, 0.85), 0.04),
                    (&dome, gold, 0.05),
                    (&lip, scale(gold, 0.9), 0.04),
                    (&clapper, scale(gold, 0.6), 0.04),
                ],
                x,
                y,
            )
        }
        Art::SnifferEgg => {
            // A large egg with mossy patches.
            let shell = mean(tex[0]);
            let egg = ellipse(8.0, 8.5, 6.9, 7.6);
            let p = shaded(&egg, x, y, shell, 0.06)?;
            let patch = noise(x / 2, y / 2, 71) > 0.62 && egg(x + 1, y) && egg(x, y + 1) && egg(x - 1, y);
            Some(if patch { tint([98, 124, 70], 0.95 + noise(x, y, 72) * 0.1) } else { p })
        }
        Art::TurtleEgg => {
            // Two small speckled eggs.
            let shell = mean(tex[0]);
            let big = ellipse(6.5, 9.5, 3.3, 4.2);
            let small = ellipse(11.0, 11.5, 2.5, 3.1);
            let p = first(&[(&small, scale(shell, 0.96), 0.04), (&big, shell, 0.04)], x, y)?;
            let speck = (x * 3 + y * 5) % 7 == 0 && (big(x + 1, y) || small(x - 1, y));
            Some(if speck { tint([104, 150, 86], 1.0) } else { p })
        }
        Art::PinkPetals | Art::Wildflowers => {
            // Small blossoms on stems that meet at the bottom.
            let blossoms: &[(i32, i32, [u8; 3])] = if art == Art::PinkPetals {
                &[(4, 4, [244, 160, 202]), (11, 3, [236, 138, 186]), (3, 10, [236, 138, 186]), (12, 9, [244, 160, 202])]
            } else {
                &[(4, 3, [244, 214, 64]), (11, 4, [236, 236, 236]), (4, 10, [176, 132, 220]), (12, 10, [244, 214, 64])]
            };
            for &(bx, by, c) in blossoms {
                let (dx, dy) = (x - bx, y - by);
                if dx == 0 && dy == 0 {
                    return Some(tint(if art == Art::PinkPetals { [250, 226, 110] } else { [214, 150, 44] }, 1.0));
                }
                if dx.abs() + dy.abs() == 1 || (dx.abs() == 1 && dy.abs() == 1 && (bx + by) % 2 == 0) {
                    return Some(tint(c, if dx > 0 || dy > 0 { 0.82 } else { 1.08 }));
                }
            }
            // Stems: straight lines from each blossom to (8, 15).
            let stem = blossoms.iter().any(|&(bx, by, _)| {
                let t = (y - by - 2) as f32 / (15 - by - 2).max(1) as f32;
                (0.0..=1.0).contains(&t) && ((bx as f32 + (8.0 - bx as f32) * t).round() as i32) == x
            });
            stem.then(|| tint([82, 138, 52], 0.95 + noise(x, y, 81) * 0.1))
        }
        Art::LeafLitter => {
            // Fallen leaves, each a pointed oval with a darker midrib.
            let leaves: &[(f32, f32, f32, [u8; 3])] = &[
                (5.0, 5.0, 0.6, [164, 104, 46]),
                (11.0, 4.5, -0.5, [188, 132, 58]),
                (6.0, 11.0, -0.7, [176, 116, 50]),
                (11.5, 11.0, 0.9, [150, 92, 42]),
            ];
            for &(cx, cy, angle, c) in leaves {
                let (px, py) = centre(x, y);
                let (s, co) = angle.sin_cos();
                let (u, v) = ((px - cx) * co + (py - cy) * s, -(px - cx) * s + (py - cy) * co);
                let along = 1.0 - (u / 3.8).powi(2);
                if along > 0.0 && v.abs() <= 1.9 * along.sqrt() {
                    let rib = v.abs() < 0.5;
                    return Some(tint(c, if rib { 0.72 } else { 0.95 + noise(x, y, 91) * 0.1 }));
                }
            }
            None
        }
        Art::Bamboo => {
            // A stalk rising to the top right, darker at its nodes, with a
            // leaf near the top.
            let green = mean(tex[0]);
            let (u, v) = (x + y, x - y);
            let stalk = (15..=16).contains(&u) && (-11..=11).contains(&v);
            if stalk {
                let node = v.rem_euclid(6) == 0;
                return Some(tint(
                    green,
                    if node {
                        0.7
                    } else if u == 15 {
                        1.12
                    } else {
                        0.86
                    },
                ));
            }
            let leaf = |x: i32, y: i32| {
                let (u, v) = (x + y, x - y);
                (11..=14).contains(&u) && (4..=9).contains(&v) && (v - 6).abs() + (u - 13).abs() <= 3
            };
            shaded(&leaf, x, y, [104, 168, 52], 0.05)
        }
        Art::Lever => {
            // The handle, leaning from its stone base.
            let stone = block_of("cobblestone").map_or([120; 3], |b| mean(b.info().tex[0]));
            let base = rect(4, 12, 11, 14);
            let handle = |x: i32, y: i32| {
                let (u, v) = (x + y, x - y);
                (18..=19).contains(&u) && (-6..=6).contains(&v) && y <= 11
            };
            if (x, y) == (11, 4) || (x, y) == (12, 5) {
                return Some(tint(scale(STICK, 1.2), 1.0));
            }
            first(&[(&handle, STICK, 0.05), (&base, stone, 0.1)], x, y)
        }
        Art::RedstoneTorch => {
            // The game's torch with a glowing redstone head and its halo.
            match (x, y) {
                (7..=8, 4) => Some(tint(REDSTONE_HOT, 1.0)),
                (7..=8, 5..=6) => Some(tint(REDSTONE, if x == 7 { 1.1 } else { 0.9 })),
                (6 | 9, 5) => Some(tint(scale(REDSTONE, 0.62), 1.0)),
                (7..=8, 7..=15) => Some(tint(STICK, if x == 7 { 1.0 } else { 0.78 })),
                _ => None,
            }
        }
        Art::TripwireHook => {
            // An iron ring on a short wooden stake.
            let planks = block_of("oak_planks").map_or([162, 130, 78], |b| mean(b.info().tex[0]));
            let ring = |x: i32, y: i32| {
                let (px, py) = centre(x, y);
                let r = ((px - 8.0).powi(2) + (py - 5.0).powi(2)).sqrt();
                (1.6..=3.4).contains(&r)
            };
            let neck = rect(7, 8, 8, 9);
            let stake = rect(6, 10, 9, 15);
            first(&[(&ring, [190, 194, 196], 0.04), (&neck, [150, 152, 156], 0.04), (&stake, planks, 0.08)], x, y)
        }
        Art::Door | Art::Lantern | Art::Chain => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sprite_draws_something_inside_javas_frame() {
        for i in 0..table::ITEMS.len() as u16 {
            let solid = (0..256).filter(|&p| pixel(i, p % 16, p / 16)[3] > 0).count();
            assert!(solid >= 12, "{:?} draws {solid} pixels", table::java_id(i));
        }
    }
}
