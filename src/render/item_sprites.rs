//! Procedural 16x16 inventory icons for non-block items.
//!
//! Each sprite is built from simple shapes (diagonal handles, arcs, blobs)
//! and shaded like Minecraft's item art: light from the top left, a dark rim
//! on the bottom right.

use crate::item::{ArmorMaterial, ArmorPiece, Sprite, Tier, ToolKind};
use crate::world::block::tex;
use crate::world::noise::hash_f;

type Rgba = [u8; 4];

const CLEAR: Rgba = [0, 0, 0, 0];
const HANDLE: [u8; 3] = [137, 103, 39];
const HANDLE_DARK: [u8; 3] = [86, 64, 26];

/// Pixel centre, so shapes are symmetric on the 16x16 grid.
fn centre(x: i32, y: i32) -> (f32, f32) {
    (x as f32 + 0.5, y as f32 + 0.5)
}

/// Deterministic sprite noise; `variation` picks an independent pattern.
/// A procedural graphics helper with no cryptographic purpose.
fn noise(x: i32, y: i32, variation: u64) -> f32 {
    hash_f(x, y, 0, 0x17E3 ^ variation)
}

fn tint(c: [u8; 3], f: f32) -> Rgba {
    let s = |v: u8| (v as f32 * f).clamp(0.0, 255.0) as u8;
    [s(c[0]), s(c[1]), s(c[2]), 255]
}

/// Shades a pixel of a filled shape: a highlight where the shape's top or
/// left edge is, a dark rim on its bottom and right edges.
fn shaded(inside: &dyn Fn(i32, i32) -> bool, x: i32, y: i32, c: [u8; 3], grain: f32) -> Option<Rgba> {
    if !inside(x, y) {
        return None;
    }
    let f = if !inside(x + 1, y) || !inside(x, y + 1) {
        0.62
    } else if !inside(x - 1, y) || !inside(x, y - 1) {
        1.22
    } else {
        1.0
    };
    Some(tint(c, f * (1.0 - grain + noise(x, y, c[0] as u64) * grain * 2.0)))
}

/// Tool material colour.
fn tier_colour(tier: Tier) -> [u8; 3] {
    match tier {
        Tier::Wood => [160, 128, 76],
        Tier::Stone => [128, 128, 128],
        Tier::Iron => [212, 212, 212],
        Tier::Gold => [246, 208, 62],
        Tier::Diamond => [70, 222, 210],
    }
}

/// Two-pixel diagonal stick from the bottom left up to column `top`.
fn handle(x: i32, y: i32, from: i32, top: i32) -> Option<Rgba> {
    if !(from..=top).contains(&x) {
        return None;
    }
    match x + y {
        15 => Some(tint(HANDLE, 0.95 + noise(x, y, 1) * 0.1)),
        16 => Some(tint(HANDLE_DARK, 1.0)),
        _ => None,
    }
}

fn tool(kind: ToolKind, tier: Tier, x: i32, y: i32) -> Option<Rgba> {
    let c = tier_colour(tier);
    // Coordinates along the handle: `u` across it (15..16 on the handle,
    // smaller towards the top left), `v` along it (grows towards the top right).
    let head: Box<dyn Fn(i32, i32) -> bool> = match kind {
        ToolKind::Pickaxe => Box::new(|x, y| {
            // An arc around the grip, tapering to points at both ends.
            let (px, py) = centre(x, y);
            let (dx, dy) = (px - 0.5, 15.5 - py);
            let (r, a) = ((dx * dx + dy * dy).sqrt(), dy.atan2(dx).to_degrees());
            let taper = ((a - 45.0).abs() - 20.0).max(0.0) * 0.1;
            (4.0..=86.0).contains(&a) && r >= 10.6 + taper && r <= 13.6 - taper * 0.4
        }),
        ToolKind::Axe => Box::new(|x, y| {
            let (u, v) = (x + y, x - y);
            let half = 1.6 + (14 - u) as f32 * 0.5;
            (7..=14).contains(&u) && (v as f32 - 4.5).abs() <= half
        }),
        ToolKind::Shovel => Box::new(|x, y| {
            let (u, v) = ((x + y) as f32 - 15.5, (x - y) as f32 - 7.5);
            (u / 3.6).powi(2) + (v / 5.2).powi(2) <= 1.0 && v >= -4.0
        }),
        ToolKind::Hoe => Box::new(|x, y| {
            let (u, v) = (x + y, x - y);
            (8..=16).contains(&u) && (6..=9).contains(&v)
        }),
        ToolKind::Sword => Box::new(|x, y| {
            let (u, v) = (x + y, x - y);
            let blade = (-4..=11).contains(&v) && (14..=16).contains(&u) || v == 12 && u == 15;
            let guard = (-7..=-5).contains(&v) && (11..=19).contains(&u) && (v == -6 || (12..=18).contains(&u));
            blade || guard
        }),
    };
    if let Some(p) = shaded(&*head, x, y, c, 0.05) {
        // A bright ridge down the middle of sword blades.
        if kind == ToolKind::Sword && x + y == 15 && (-4..=10).contains(&(x - y)) {
            return Some(tint(c, 1.3));
        }
        return Some(p);
    }
    let top = match kind {
        ToolKind::Sword => -1,
        ToolKind::Shovel => 8,
        ToolKind::Pickaxe => 9,
        ToolKind::Hoe | ToolKind::Axe => 11,
    };
    if kind == ToolKind::Sword {
        // Short grip and pommel below the guard.
        return handle(x, y, 1, 4);
    }
    handle(x, y, 1, top)
}

fn armor_colour(material: ArmorMaterial) -> [u8; 3] {
    match material {
        ArmorMaterial::Leather => [160, 101, 64],
        ArmorMaterial::Iron => [206, 206, 206],
        ArmorMaterial::Gold => [246, 208, 62],
        ArmorMaterial::Diamond => [70, 222, 210],
    }
}

/// Armor icons, symmetric about the vertical centre line.
fn armor(piece: ArmorPiece, material: ArmorMaterial, x: i32, y: i32) -> Option<Rgba> {
    let shape = |x: i32, y: i32| {
        // Distance from the centre line: 0 for the middle two columns.
        let d = if x < 8 { 7 - x } else { x - 8 };
        match piece {
            // A dome, open at the bottom for the face.
            ArmorPiece::Helmet => (3..=11).contains(&y) && d <= if y == 3 { 3 } else { 5 } && !(y >= 8 && d <= 2),
            // Wide shoulders, a neck cutout and a narrower body.
            ArmorPiece::Chestplate => {
                ((2..=5).contains(&y) && d <= 6 && !(y <= 3 && d <= 1)) || ((6..=13).contains(&y) && d <= 4)
            }
            // A waistband over two legs.
            ArmorPiece::Leggings => ((2..=4).contains(&y) && d <= 5) || ((5..=13).contains(&y) && (2..=5).contains(&d)),
            // Two boots, toes pointing out.
            ArmorPiece::Boots => {
                ((7..=10).contains(&y) && (2..=4).contains(&d)) || ((11..=13).contains(&y) && (2..=6).contains(&d))
            }
        }
    };
    shaded(&shape, x, y, armor_colour(material), 0.06)
}

pub fn pixel(sprite: Sprite, x: usize, y: usize) -> Rgba {
    let (x, y) = (x as i32, y as i32);
    let (px, py) = centre(x, y);
    let disc = |cx: f32, cy: f32, r: f32| {
        move |x: i32, y: i32| {
            let (px, py) = centre(x, y);
            (px - cx).powi(2) + (py - cy).powi(2) <= r * r
        }
    };
    let out = match sprite {
        Sprite::Stick => handle(x, y, 2, 13),
        Sprite::Tool(kind, tier) => tool(kind, tier, x, y),
        Sprite::Armor(piece, material) => armor(piece, material, x, y),
        Sprite::Lump(c) => {
            let lump = |x: i32, y: i32| {
                let (px, py) = centre(x, y);
                let d = ((px - 8.0).powi(2) + (py - 8.5).powi(2)).sqrt();
                d < 4.6 + noise(x / 2, y / 2, 3) * 1.8
            };
            let fleck = noise(x, y, 4) < 0.12;
            shaded(&lump, x, y, if fleck { [c[0] + 40, c[1] + 40, c[2] + 40] } else { c }, 0.1)
        }
        Sprite::Ingot(c) => {
            // A bar seen from above at an angle: a lighter top face and a
            // darker front face.
            let bar = |x: i32, y: i32| {
                let skew = x as f32 - (11 - y) as f32 * 0.5;
                (5..=11).contains(&y) && (1.5..=11.5).contains(&skew)
            };
            let face = if y <= 7 { 1.1 } else { 0.86 };
            shaded(&bar, x, y, c, 0.03).map(|p| tint([p[0], p[1], p[2]], face))
        }
        Sprite::Gem(c) => {
            let gem = |x: i32, y: i32| {
                let (px, py) = centre(x, y);
                (px - 8.0).abs() / 6.5 + (py - 8.0).abs() / 7.0 <= 1.0 && py >= 2.5
            };
            let facet = if py < 5.0 {
                1.25
            } else if px + py > 17.0 {
                0.8
            } else {
                1.0
            };
            shaded(&gem, x, y, c, 0.03).map(|p| tint([p[0], p[1], p[2]], facet))
        }
        Sprite::Apple => {
            let stem = (x == 8 && (2..=4).contains(&y)).then_some(tint([90, 60, 25], 1.0));
            let leaf = ((x == 9 || x == 10) && y == 3 || x == 10 && y == 2).then_some(tint([70, 150, 40], 1.0));
            let body = |x: i32, y: i32| disc(6.5, 9.5, 4.6)(x, y) || disc(9.5, 9.5, 4.6)(x, y);
            let shine = (5..=6).contains(&x) && (7..=8).contains(&y);
            stem.or(leaf).or_else(|| {
                shaded(&body, x, y, [210, 30, 35], 0.06).map(|p| if shine { [255, 200, 200, 255] } else { p })
            })
        }
        Sprite::Bread => {
            let loaf = |x: i32, y: i32| {
                let (px, py) = centre(x, y);
                let (u, v) = ((px + py - 16.0) / 1.414, (px - py) / 1.414);
                (u / 3.2).powi(2) + (v / 7.0).powi(2) <= 1.0
            };
            let score = (x - y).rem_euclid(4) == 0 && (x + y - 15).abs() <= 1;
            shaded(&loaf, x, y, if score { [230, 190, 110] } else { [190, 125, 50] }, 0.06)
        }
        Sprite::Meat(flesh, fat) => {
            let cut = |x: i32, y: i32| {
                let (px, py) = centre(x, y);
                let (u, v) = ((px + py - 16.0) / 1.414, (px - py) / 1.414);
                (u / 4.6).powi(2) + (v / 6.6).powi(2) <= 1.0
            };
            // Fat along the upper-right rim.
            let rim = !cut(x + 1, y - 1) || !cut(x + 2, y - 2);
            shaded(&cut, x, y, if rim { fat } else { flesh }, 0.08)
        }
        Sprite::Drumstick(c) => {
            let meat = disc(6.5, 6.5, 4.6);
            let bone = |x: i32, y: i32| {
                (x - y).abs() <= 1 && (8..=12).contains(&x) && (8..=12).contains(&y)
                    || disc(13.0, 12.0, 1.3)(x, y)
                    || disc(12.0, 13.0, 1.3)(x, y)
            };
            shaded(&meat, x, y, c, 0.08).or_else(|| shaded(&bone, x, y, [235, 230, 215], 0.02))
        }
        Sprite::Bone => {
            let bone = |x: i32, y: i32| {
                let shaft = (x + y == 15 || x + y == 16) && (4..=11).contains(&x);
                let knob = |cx: i32, cy: i32| (x - cx).abs() <= 1 && (y - cy).abs() <= 1 && (x - cx) + (y - cy) != 2;
                shaft || knob(3, 12) || knob(12, 3) || (x, y) == (2, 11) || (x, y) == (4, 13) || (x, y) == (11, 2)
            };
            shaded(&bone, x, y, [232, 228, 210], 0.03)
        }
        Sprite::String => {
            let wave = 7.5 + (px * 0.9).sin() * 3.0 + (px - 8.0) * 0.3;
            ((py - wave).abs() < 0.8 && (1..=14).contains(&x))
                .then_some(tint([235, 235, 235], 0.9 + noise(x, y, 5) * 0.2))
        }
        Sprite::Feather => {
            let quill = (x + y == 15) && (2..=13).contains(&x);
            let vane = |x: i32, y: i32| {
                let (u, v) = ((x + y) as f32 - 15.0, (x - y) as f32 - 1.0);
                u <= 0.0 && (u / 3.8).powi(2) + (v / 10.0).powi(2) <= 1.0
            };
            if quill { Some(tint([200, 200, 200], 1.0)) } else { shaded(&vane, x, y, [245, 245, 245], 0.04) }
        }
        Sprite::Powder(c) => {
            let pile = |x: i32, y: i32| {
                let (px, py) = centre(x, y);
                py >= 6.5 + (px - 8.0).abs() * 0.7 && py <= 13.5 && (1..=14).contains(&x)
            };
            let grain = if noise(x, y, 6) < 0.3 { 0.6 } else { 1.0 };
            shaded(&pile, x, y, c, 0.15).map(|p| tint([p[0], p[1], p[2]], grain))
        }
        Sprite::Leather => {
            let hide = |x: i32, y: i32| {
                let notch = noise(x / 3, y / 3, 7) < 0.25 && (x == 3 || x == 12 || y == 3 || y == 12);
                (3..=12).contains(&x) && (3..=12).contains(&y) && !notch
            };
            shaded(&hide, x, y, [150, 82, 42], 0.1)
        }
        Sprite::Seeds => {
            // A scatter of small grains, lit from the top left.
            const GRAINS: [(i32, i32); 7] = [(4, 4), (9, 3), (12, 7), (6, 8), (10, 11), (3, 12), (7, 13)];
            GRAINS.iter().find_map(|&(gx, gy)| {
                let shade = match (x - gx, y - gy) {
                    (0, 0) => 1.3,
                    (1, 0) => 1.0,
                    (1, 1) => 0.7,
                    _ => return None,
                };
                Some(tint([110, 140, 48], shade))
            })
        }
        Sprite::Wheat => {
            // Three stalks fanning up from a tied base, with grain heads.
            (-1..=1).find_map(|k: i32| {
                let sx = 8.0 + k as f32 * (15 - y) as f32 / 3.0;
                let col = sx.round() as i32;
                let head = (2..=7).contains(&y);
                let on = (2..=15).contains(&y) && (x == col || head && x == col + 1);
                on.then(|| match y {
                    11 => tint([120, 82, 40], 1.0),
                    _ if head => tint([226, 184, 72], if (x + y) % 2 == 0 { 1.1 } else { 0.85 }),
                    _ => tint([196, 168, 84], 0.95 + noise(x, y, 8) * 0.1),
                })
            })
        }
        Sprite::Bed => {
            // Side view: pillow on the left, red blanket, wooden frame, legs.
            let bed = |x: i32, y: i32| {
                ((5..=11).contains(&y) && (1..=14).contains(&x))
                    || ((12..=13).contains(&y) && ((1..=2).contains(&x) || (13..=14).contains(&x)))
            };
            let c = match (x, y) {
                (_, 10..) => [137, 103, 39],
                (1..=4, _) => [236, 236, 230],
                _ => [178, 34, 34],
            };
            shaded(&bed, x, y, c, 0.05)
        }
        Sprite::MelonSlice => {
            // A half-disc wedge: green rind on the curve, red flesh with seeds.
            let wedge = |x: i32, y: i32| {
                let (px, py) = centre(x, y);
                py >= 5.0 && (px - 8.0).powi(2) + (py - 5.0).powi(2) <= 7.0 * 7.0
            };
            let (px, py) = centre(x, y);
            let r = ((px - 8.0).powi(2) + (py - 5.0).powi(2)).sqrt();
            let seed = (x + 2 * y) % 5 == 0 && r < 4.5 && py > 6.0;
            let c = if r > 5.6 {
                [70, 140, 40]
            } else if r > 4.9 {
                [210, 230, 150]
            } else if seed {
                [40, 30, 20]
            } else {
                [220, 60, 50]
            };
            shaded(&wedge, x, y, c, 0.04)
        }
        Sprite::Bow => {
            // A wooden limb bowed toward the top left, strung corner to
            // corner along the other diagonal.
            let bend = |s: i32| 7.5 * (1.0 - (s as f32 / 11.0).powi(2));
            let limb = |x: i32, y: i32| {
                let (s, k) = (x - y, 15 - (x + y));
                s.abs() <= 11 && (k as f32 - bend(s)).abs() <= 1.0
            };
            let (s, k) = (x - y, 15 - (x + y));
            let grip = s.abs() <= 1 && (k as f32 - bend(s)).abs() <= 1.0;
            if grip {
                Some(tint(HANDLE_DARK, 1.0))
            } else if limb(x, y) {
                shaded(&limb, x, y, HANDLE, 0.05)
            } else {
                (k == 0 && s.abs() <= 11).then_some(tint([225, 225, 225], 0.95))
            }
        }
        Sprite::FlintAndSteel => {
            // A curled steel striker (top right) and a flint chip (bottom left).
            let ring = |x: i32, y: i32| {
                let (px, py) = centre(x, y);
                let d = ((px - 10.0).powi(2) + (py - 6.0).powi(2)).sqrt();
                (2.2..=4.2).contains(&d) && !(px < 9.0 && py > 7.0)
            };
            let flint = |x: i32, y: i32| {
                let (px, py) = centre(x, y);
                (px - 5.0).abs() / 3.8 + (py - 11.0).abs() / 3.0 <= 1.0
            };
            shaded(&ring, x, y, [200, 200, 205], 0.04).or_else(|| shaded(&flint, x, y, [70, 70, 74], 0.08))
        }
        Sprite::Nugget(c) => {
            let nugget = |x: i32, y: i32| {
                let (px, py) = centre(x, y);
                ((px - 8.0) / 4.2).powi(2) + ((py - 9.0) / 3.4).powi(2) <= 1.0
            };
            shaded(&nugget, x, y, c, 0.08)
        }
        Sprite::Door => {
            // The door's own two textures, squeezed to half width.
            let (layer, ty) = if y < 8 { (tex::DOOR_TOP, y * 2) } else { (tex::DOOR_BOTTOM, (y - 8) * 2) };
            (4..12).contains(&x).then(|| super::textures::pixel(layer, ((x - 4) * 2) as usize, ty as usize))
        }
        Sprite::Bucket(fluid) => {
            // A tapered pail seen from slightly above: rim, inside, handle.
            let pail = |x: i32, y: i32| {
                let (px, py) = centre(x, y);
                let half = 5.8 - (py - 6.0) * 0.18;
                (6.0..=14.0).contains(&py) && (px - 8.0).abs() <= half
            };
            let (px, py) = centre(x, y);
            let inside = (5.0..7.5).contains(&py) && (px - 8.0).abs() <= 4.6;
            let handle = py < 6.0 && ((px - 8.0).powi(2) / 36.0 + (py - 6.0).powi(2) / 20.0 - 1.0).abs() < 0.18;
            if inside {
                Some(match fluid {
                    Some(c) => tint(c, 0.9 + noise(x, y, 9) * 0.2),
                    None => tint([70, 70, 74], 1.0),
                })
            } else if handle {
                Some(tint([150, 150, 155], 1.0))
            } else {
                shaded(&pail, x, y, [200, 200, 205], 0.03)
            }
        }
        Sprite::Arrow => {
            let head = |x: i32, y: i32| {
                let (u, v) = (x + y, x - y);
                (6..=11).contains(&v) && (u - 15).abs() <= (12 - v) / 2
            };
            let fletch = |x: i32, y: i32| {
                let (u, v) = (x + y, x - y);
                (-12..=-7).contains(&v) && (13..=17).contains(&u) && u != 15
            };
            shaded(&head, x, y, [160, 160, 160], 0.02)
                .or_else(|| (x + y == 15 && (1..=10).contains(&x)).then_some(tint(HANDLE, 1.0)))
                .or_else(|| fletch(x, y).then_some(tint([235, 235, 235], if (x + y) % 2 == 0 { 1.0 } else { 0.8 })))
        }
    };
    out.unwrap_or(CLEAR)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Item;

    #[test]
    fn every_sprite_draws_something_and_leaves_a_border() {
        for item in Item::all_items() {
            let sprite = item.info().sprite;
            let opaque =
                (0..16).flat_map(|y| (0..16).map(move |x| (x, y))).filter(|&(x, y)| pixel(sprite, x, y)[3] > 0);
            let count = opaque.clone().count();
            assert!(count > 12, "{} draws only {count} pixels", item.name());
        }
    }

    /// `cargo test --release icon_sheet -- --ignored` writes every item icon,
    /// magnified, to target/item_icons.png for eyeballing.
    #[test]
    #[ignore]
    fn icon_sheet() {
        const K: usize = 8;
        let items: Vec<Item> = Item::all_items().collect();
        let cols = 8;
        let rows = items.len().div_ceil(cols);
        let (w, h) = (cols * 17 * K, rows * 17 * K);
        let mut img = vec![60u8; w * h * 4];
        for (i, item) in items.iter().enumerate() {
            let (ox, oy) = ((i % cols) * 17 * K, (i / cols) * 17 * K);
            for y in 0..16 * K {
                for x in 0..16 * K {
                    let p = pixel(item.info().sprite, x / K, y / K);
                    let o = ((oy + y) * w + ox + x) * 4;
                    let bg = if (x / K + y / K).is_multiple_of(2) { 90 } else { 110 };
                    for c in 0..3 {
                        img[o + c] = ((p[c] as u32 * p[3] as u32 + bg * (255 - p[3] as u32)) / 255) as u8;
                    }
                    img[o + 3] = 255;
                }
            }
        }
        let file = std::fs::File::create("target/item_icons.png").unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.write_header().unwrap().write_image_data(&img).unwrap();
    }
}
