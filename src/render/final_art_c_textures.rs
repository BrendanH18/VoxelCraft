//! v1.0 gap Track 7: the last plant tiles, in the style the owner approved
//! for Tracks 10 and 10b (see `docs/art-and-ui.md`). Original pixel art
//! drawn with the same primitives: the amethyst buds (small, medium and
//! large) and the cluster, standing and hanging; the bamboo's small and
//! large leaves, redrawn as leaf sprites for the crossed planes the leafy
//! bamboo now draws beside its post (as Java's `bamboo_small_leaves` and
//! `bamboo_large_leaves` models do); the potted bamboo's cane, drawn like
//! the bamboo stalk; and the potted bamboo's single leaf, the only new
//! layer (4066, from this track's reserved 4066..=4079). Light comes from
//! the top left.

use super::{Img, Px, leaf, ramp, shades};
use super::{N, Tile};
use crate::world::overworld_blocks::t as ob;

/// Five shades of one colour: dark, shaded, mid, lit, highlight.
type Five = [Px; 5];

fn five(c: Px) -> Five {
    ramp(c, [600, 770, 900, 1040, 1190])
}

/// The potted bamboo's layer.
pub(super) fn potted_bamboo_layer() -> u16 {
    crate::world::utilities::t::POTTED + crate::world::utilities::POTTED_BAMBOO as u16
}

// ---- Amethyst ----------------------------------------------------------------

/// A violet crystal shard rising `height` texels from its base at
/// `(x, bottom)`, `width` texels across (2 or 3), leaning `lean` texels
/// to the side over its height. Faceted like a prism: a lit left face, a
/// pale glint down it, a mid face and a shaded right face; the point is lit
/// on its left facet and shaded on its right, with a bright tip.
fn shard(img: &mut Img, (x, bottom): (i32, i32), height: i32, width: i32, lean: i32, s: &Five, glint: Px) {
    let point = (width + 1) / 2 + 1;
    for t in 0..height {
        let y = bottom - t;
        let left = x + lean * t / height.max(1);
        let from_top = height - 1 - t;
        // The point narrows the shard over its last few rows.
        let w = if from_top < point { (width * (from_top + 1) + point - 1) / point } else { width }.max(1);
        let left = left + (width - w + 1) / 2;
        for i in 0..w {
            let c = if from_top == 0 {
                glint
            } else if from_top < point {
                if i * 2 < w { s[3] } else { s[1] }
            } else if i == 0 {
                s[3]
            } else if i == w - 1 {
                s[1]
            } else if t % 4 == 1 && i == 1 {
                glint
            } else {
                s[2]
            };
            img.set(left + i, y, c);
        }
    }
    // A dark seam where the shard meets the ground, under its shaded side.
    img.set(x + width - 1, bottom, s[0]);
}

/// One amethyst growth stage, standing on the bottom row: `size` 0 small
/// bud, 1 medium, 2 large, 3 the cluster. Each stage keeps the shards of
/// the smaller ones' places and adds taller ones, so a bud reads as
/// growing.
fn amethyst(size: u8) -> Img {
    let s = five([148, 100, 204]);
    let glint = [236, 214, 255];
    let mut img = Img::new();
    // (base x, height, width, lean), back to front: the tallest last.
    let shards: &[(i32, i32, i32, i32)] = match size {
        0 => &[(5, 3, 2, -1), (7, 5, 3, 0)],
        1 => &[(4, 5, 2, -2), (10, 6, 2, 2), (7, 8, 3, 0)],
        2 => &[(3, 6, 2, -2), (11, 7, 2, 2), (5, 4, 2, -1), (7, 11, 3, 0), (9, 4, 2, 1)],
        _ => {
            &[(2, 6, 2, -2), (12, 7, 2, 2), (3, 10, 2, -3), (10, 11, 3, 3), (5, 5, 2, -1), (6, 14, 3, 0), (10, 5, 2, 1)]
        }
    };
    for &(x, h, w, lean) in shards {
        shard(&mut img, (x, 15), h, w, lean, &s, glint);
    }
    img
}

/// The same stage hanging from a ceiling: the standing one turned upside
/// down (the light still from the left).
fn hanging(img: &Img) -> Img {
    let mut out = Img::new();
    for y in 0..N as i32 {
        for x in 0..N as i32 {
            if let Some(c) = img.get(x, y) {
                out.set(x, N as i32 - 1 - y, c);
            }
        }
    }
    out
}

// ---- Bamboo -------------------------------------------------------------------

/// The bamboo's leaves for its crossed planes (texture columns 1-14, as
/// the planes run 1-15): narrow leaves fanning up and out from the middle,
/// where the post stands, each lit along its upper edge with a bright tip;
/// `large` has more and longer ones, reaching lower down the cell.
fn bamboo_leaves(large: bool) -> Tile {
    let l = shades([74, 136, 44]);
    let mut img = Img::new();
    // (base x, base y, direction, length, lift)
    let sprigs: &[(i32, i32, i32, i32, i32)] = if large {
        &[
            (8, 2, 1, 6, 1),
            (7, 3, -1, 6, 1),
            (8, 6, 1, 6, 0),
            (7, 7, -1, 6, 0),
            (8, 10, 1, 5, -1),
            (7, 11, -1, 5, -1),
            (8, 0, 1, 3, 1),
            (7, 1, -1, 3, 1),
        ]
    } else {
        &[(8, 2, 1, 4, 1), (7, 3, -1, 4, 1), (8, 6, 1, 3, 0), (7, 7, -1, 3, 0)]
    };
    for &(x, y, dir, len, lift) in sprigs {
        leaf(&mut img, (x, y), dir, len, lift, &l);
        // Where the leaf leaves the stalk, its stem.
        img.set(x, y, l[1]);
    }
    img.tile()
}

/// The potted bamboo: the pot drawn as every other potted plant's
/// (`utility::potted_side`) and the cane its post samples (columns 7-8 of
/// the sides, and the top's middle) drawn like the bamboo stalk: lit down
/// its left, shaded on its right, a node (a dark groove over a lit bulge)
/// at row 2.
fn potted_bamboo() -> Tile {
    let layer = potted_bamboo_layer();
    let s = five([116, 160, 52]);
    std::array::from_fn(|i| {
        let (x, y) = (i % N, i / N);
        if (y >= 10 && (5..11).contains(&x)) || y >= 12 {
            return super::super::super::utility::potted_side(layer, crate::world::block::Block::AIR, x, y);
        }
        if !(7..9).contains(&x) {
            return [0; 4];
        }
        let k = match y {
            2 => 0,
            3 if x == 7 => 4,
            _ if x == 7 => 3,
            _ => 1,
        };
        let [r, g, b] = s[k];
        [r, g, b, 255]
    })
}

/// The potted bamboo's single leaf, on the plane through the cane (rows
/// 0-13 show; the plane rises from 2 to 16): one long leaf arching out to
/// the right from the cane and a short one to the left.
fn potted_bamboo_leaf() -> Tile {
    let l = shades([74, 136, 44]);
    let mut img = Img::new();
    leaf(&mut img, (8, 4), 1, 6, 2, &l);
    leaf(&mut img, (7, 7), -1, 3, 1, &l);
    img.set(8, 4, l[1]);
    img.set(7, 7, l[1]);
    img.tile()
}

/// Draws every tile of this round through `put`.
pub(super) fn build(put: &mut impl FnMut(u16, Tile)) {
    for size in 0..4u8 {
        let up = amethyst(size);
        put(ob::BUDS + u16::from(size), up.tile());
        put(ob::BUDS + 4 + u16::from(size), hanging(&up).tile());
    }
    put(ob::BAMBOO_SMALL_LEAVES, bamboo_leaves(false));
    put(ob::BAMBOO_LARGE_LEAVES, bamboo_leaves(true));
    put(potted_bamboo_layer(), potted_bamboo());
    put(crate::world::utilities::POTTED_BAMBOO_LEAF, potted_bamboo_leaf());
}

/// The layers this round draws, in review order (tests pin each one).
#[cfg(test)]
pub(super) fn layers() -> Vec<u16> {
    let mut out: Vec<u16> = (ob::BUDS..ob::BUDS + 8).collect();
    out.extend([ob::BAMBOO_SMALL_LEAVES, ob::BAMBOO_LARGE_LEAVES]);
    out.extend([potted_bamboo_layer(), crate::world::utilities::POTTED_BAMBOO_LEAF]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(layer: u16) -> Tile {
        std::array::from_fn(|i| super::super::super::pixel(layer, i % N, i / N).expect("drawn by this round"))
    }

    fn top(t: &Tile) -> usize {
        (0..N).find(|&y| (0..N).any(|x| t[y * N + x][3] > 0)).unwrap_or(N)
    }

    /// The buds grow taller stage by stage; the hanging ones are the
    /// standing ones upside down; every stage stands on the bottom row.
    #[test]
    fn amethyst_grows_by_stage_and_hangs_upside_down() {
        let heights: Vec<usize> = (0..4).map(|s| N - top(&tile(ob::BUDS + s))).collect();
        assert!(heights.windows(2).all(|w| w[0] < w[1]), "{heights:?}");
        assert_eq!(heights, [5, 8, 11, 14]);
        for size in 0..4 {
            let (up, down) = (tile(ob::BUDS + size), tile(ob::BUDS + 4 + size));
            for y in 0..N {
                for x in 0..N {
                    assert_eq!(up[y * N + x], down[(N - 1 - y) * N + x]);
                }
            }
            assert!((0..N).any(|x| up[(N - 1) * N + x][3] > 0));
            // Lit, mid, shaded and glinting facets: at least four shades.
            let shades: std::collections::BTreeSet<_> = up.iter().filter(|p| p[3] > 0).collect();
            assert!(shades.len() >= 4, "{size}: {}", shades.len());
        }
    }

    /// The leaves the crossed planes show stay inside the planes' columns
    /// 1-14, leave the post's middle clear above their stems, and the large
    /// leaves cover more than the small.
    #[test]
    fn bamboo_leaves_fit_their_planes() {
        let (small, large) = (tile(ob::BAMBOO_SMALL_LEAVES), tile(ob::BAMBOO_LARGE_LEAVES));
        for t in [&small, &large] {
            assert!((0..N).all(|y| t[y * N][3] == 0 && t[y * N + 15][3] == 0));
        }
        let count = |t: &Tile| t.iter().filter(|p| p[3] > 0).count();
        assert!(count(&large) > count(&small) + 10, "{} {}", count(&large), count(&small));
    }

    /// The potted bamboo's pot is the pot the other potted plants share;
    /// the cane fills the columns the post samples, above the pot.
    #[test]
    fn the_potted_bamboo_shares_the_pot_and_has_a_cane() {
        let layer = potted_bamboo_layer();
        let t = tile(layer);
        for y in 0..N {
            for x in 0..N {
                if (y >= 10 && (5..11).contains(&x)) || y >= 12 {
                    let pot =
                        super::super::super::super::utility::potted_side(layer, crate::world::block::Block::AIR, x, y);
                    assert_eq!(t[y * N + x], pot);
                } else {
                    assert_eq!(t[y * N + x][3] > 0, (7..9).contains(&x), "{x} {y}");
                }
            }
        }
        let leaf = tile(crate::world::utilities::POTTED_BAMBOO_LEAF);
        assert!(leaf.iter().filter(|p| p[3] > 0).count() > 12);
        assert!((14..N).all(|y| (0..N).all(|x| leaf[y * N + x][3] == 0)), "rows 14-15 are above the plane");
    }
}
