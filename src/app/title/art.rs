//! v0.18 Track 7: VoxelCraft's own title art, drawn here rather than loaded:
//! the wordmark (each letter built from small bevelled cubes, "Voxel" in
//! moss greens and "Craft" in warm ambers, with a dark rim and a soft drop
//! shadow) and the default world icon (a muted pixel landscape shown until
//! a world has its own). Both are original pixel art; neither traces
//! Minecraft's logo, lettering or `unknown_server.png`. Uploaded to the UI
//! image atlas (`render::ui::WORDMARK_RECT`, `DEFAULT_ICON_RECT`).

use crate::render::ui::{DEFAULT_ICON_RECT, WORDMARK_RECT};

/// Pixels per letter cell (one cube).
const CELL: usize = 5;
/// Rows of every glyph; lower-case letters use the bottom five.
const ROWS: usize = 7;
/// The drop shadow's offset in pixels.
const SHADOW: usize = 2;

/// Glyph rows, `#` filled; each row of a glyph has the same width.
const GLYPHS: [(char, [&str; ROWS]); 10] = [
    ('V', ["#...#", "#...#", "#...#", "#...#", ".#.#.", ".#.#.", "..#.."]),
    ('o', ["....", "....", ".##.", "#..#", "#..#", "#..#", ".##."]),
    ('x', [".....", ".....", "#...#", ".#.#.", "..#..", ".#.#.", "#...#"]),
    ('e', ["....", "....", ".##.", "#..#", "####", "#...", ".###"]),
    ('l', ["#.", "#.", "#.", "#.", "#.", "#.", ".#"]),
    ('C', [".####", "#....", "#....", "#....", "#....", "#....", ".####"]),
    ('r', ["....", "....", "#.##", "##..", "#...", "#...", "#..."]),
    ('a', ["....", "....", ".##.", "...#", ".###", "#..#", ".###"]),
    ('f', ["..##", ".#..", "####", ".#..", ".#..", ".#..", ".#.."]),
    ('t', [".#..", ".#..", "###.", ".#..", ".#..", ".#..", "..##"]),
];

/// The wordmark's drawn size inside [`WORDMARK_RECT`], in pixels (and GUI
/// pixels: it is drawn at one GUI pixel per image pixel, as Java's 256 x 44
/// logo is).
pub(in crate::app) fn wordmark_size() -> (u32, u32) {
    let cells: usize = GLYPHS.iter().map(|(_, rows)| rows[0].len()).sum::<usize>() + GLYPHS.len() - 1;
    ((cells * CELL + 2 + SHADOW) as u32, (ROWS * CELL + 2 + SHADOW) as u32)
}

/// Small stable noise for a cell, 0..1.
fn noise(x: usize, y: usize) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B9) ^ (y as u32).wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFF) as f32 / 255.0
}

fn shade([r, g, b]: [f32; 3], k: f32) -> [f32; 3] {
    [r * k, g * k, b * k]
}

/// The wordmark as RGBA, [`WORDMARK_RECT`] sized, transparent around it.
pub(in crate::app) fn wordmark() -> Vec<u8> {
    let (w, h) = (WORDMARK_RECT.w as usize, WORDMARK_RECT.h as usize);
    // Each cell: the glyph index it belongs to, or none.
    let cols: usize = wordmark_size().0 as usize / CELL;
    let mut cells = vec![None; cols * ROWS];
    let mut cx = 0;
    for (i, (_, rows)) in GLYPHS.iter().enumerate() {
        for (y, row) in rows.iter().enumerate() {
            for (x, c) in row.bytes().enumerate() {
                if c == b'#' {
                    cells[y * cols + cx + x] = Some(i);
                }
            }
        }
        cx += rows[0].len() + 1;
    }
    let filled = |x: isize, y: isize| -> bool {
        x >= 0 && y >= 0 && (x as usize) < cols * CELL && (y as usize) < ROWS * CELL && {
            let (cx, cy) = (x as usize / CELL, y as usize / CELL);
            cells[cy * cols + cx].is_some()
        }
    };
    let mut px = vec![[0f32; 4]; w * h];
    // The letters sit one pixel in, leaving room for the rim.
    let (ox, oy) = (1isize, 1isize);
    // Drop shadow, then the rim, then the cubes.
    for y in 0..h as isize {
        for x in 0..w as isize {
            let (lx, ly) = (x - ox, y - oy);
            let p = &mut px[y as usize * w + x as usize];
            if filled(lx, ly) {
                continue;
            }
            let near = (-1..=1).any(|dy| (-1..=1).any(|dx| filled(lx + dx, ly + dy)));
            if near {
                *p = [0.08, 0.07, 0.06, 1.0];
            } else if filled(lx - SHADOW as isize, ly - SHADOW as isize)
                || (-1..=1).any(|d| filled(lx - SHADOW as isize + d, ly - SHADOW as isize + d.abs()))
            {
                *p = [0.0, 0.0, 0.0, 0.45];
            }
        }
    }
    for cy in 0..ROWS {
        for cx in 0..cols {
            let Some(glyph) = cells[cy * cols + cx] else { continue };
            // "Voxel" moss green, "Craft" amber; lighter towards the top.
            let base = if glyph < 5 { [0.42, 0.72, 0.38] } else { [0.93, 0.66, 0.26] };
            let lift = 1.12 - 0.32 * cy as f32 / (ROWS - 1) as f32;
            let base = shade(base, lift * (0.94 + 0.12 * noise(cx, cy)));
            for y in 0..CELL {
                for x in 0..CELL {
                    // A raised cube: lit top and left edges, shaded right and bottom.
                    let k = if y == 0 {
                        1.28
                    } else if x == CELL - 1 || y == CELL - 1 {
                        0.62
                    } else if x == 0 {
                        1.12
                    } else {
                        1.0 - 0.05 * noise(cx * CELL + x, cy * CELL + y)
                    };
                    let [r, g, b] = shade(base, k);
                    let (px_x, px_y) = (ox as usize + cx * CELL + x, oy as usize + cy * CELL + y);
                    px[px_y * w + px_x] = [r.min(1.0), g.min(1.0), b.min(1.0), 1.0];
                }
            }
        }
    }
    to_bytes(&px)
}

/// sRGB-encoded RGBA bytes from colours already in display (sRGB) terms.
fn to_bytes(px: &[[f32; 4]]) -> Vec<u8> {
    px.iter().flat_map(|c| c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)).collect()
}

/// The icon a world shows until it has its own: a dim 16 x 16 landscape
/// (sky, a hill, grass over earth) drawn at four pixels per texel.
pub(in crate::app) fn default_icon() -> Vec<u8> {
    let n = DEFAULT_ICON_RECT.w as usize;
    let k = n / 16;
    let mut px = vec![[0f32; 4]; n * n];
    // Height of the ground at each texel column (from the top).
    let ground = [10, 10, 9, 9, 8, 8, 8, 9, 9, 10, 10, 10, 9, 9, 10, 10];
    for ty in 0..16 {
        for tx in 0..16 {
            let g = ground[tx];
            let jitter = 0.92 + 0.16 * noise(tx + 31, ty + 7);
            let c = if ty < g {
                // Sky: slate blue fading lighter towards the horizon.
                let t = ty as f32 / 10.0;
                [0.20 + 0.12 * t, 0.25 + 0.14 * t, 0.36 + 0.12 * t]
            } else if ty == g {
                shade([0.30, 0.46, 0.26], jitter)
            } else if ty == g + 1 && noise(tx, ty) > 0.5 {
                shade([0.26, 0.38, 0.22], jitter)
            } else {
                shade([0.34, 0.26, 0.19], jitter)
            };
            for y in 0..k {
                for x in 0..k {
                    px[(ty * k + y) * n + tx * k + x] = [c[0], c[1], c[2], 1.0];
                }
            }
        }
    }
    to_bytes(&px)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wordmark_fits_javas_logo_box_and_is_drawn() {
        let (w, h) = wordmark_size();
        // Java's logo is 256 x 44 GUI pixels.
        assert!(w <= 256 && h <= 44, "{w} x {h}");
        assert!(w as usize <= WORDMARK_RECT.w as usize && h as usize <= WORDMARK_RECT.h as usize);
        let px = wordmark();
        assert_eq!(px.len(), (WORDMARK_RECT.w * WORDMARK_RECT.h * 4) as usize);
        let opaque = px.as_chunks::<4>().0.iter().filter(|p| p[3] == 255).count();
        assert!(opaque > 2000, "{opaque} opaque pixels");
        // Nothing drawn outside the measured size.
        for y in 0..WORDMARK_RECT.h {
            for x in 0..WORDMARK_RECT.w {
                if x >= w || y >= h {
                    assert_eq!(px[((y * WORDMARK_RECT.w + x) * 4 + 3) as usize], 0, "{x},{y}");
                }
            }
        }
        for (c, rows) in GLYPHS {
            assert!(rows.iter().all(|r| r.len() == rows[0].len()), "{c}");
        }
        assert_eq!(GLYPHS.iter().map(|g| g.0).collect::<String>(), "VoxelCraft");
    }

    #[test]
    fn default_icon_is_an_opaque_picture() {
        let px = default_icon();
        assert_eq!(px.len(), 64 * 64 * 4);
        assert!(px.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
        assert_ne!(px[..4], px[px.len() - 4..], "sky over ground");
    }
}
