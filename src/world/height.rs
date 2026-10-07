//! Maps a Java Edition overworld height onto this world's column.
//!
//! Java's overworld runs from y = -64 to y = 319 with sea level at 63. This
//! world is y = 0..255 with [`super::terrain::SEA_LEVEL`] at 62, so the sea
//! lines up (63 maps to 62) and the surface only shifts by one. The
//! underground is the part that does not fit: Java y in [-64, 63] is 127
//! blocks of span and is laid evenly across our 62 blocks from bedrock to
//! sea level,
//!
//! ```text
//! world = (java + 64) * 62 / 127
//! ```
//!
//! Deepslate, which Java starts at y = 0, therefore lands near y = 31
//! rather than above the sea. Adding 64 (the old shift) put that line at
//! y = 64.
//!
//! Vein attempts are a budget per column. A band that sits fully under Java
//! sea level is only [`UNDERGROUND`] = 62/127 as tall here, so its attempt
//! count is multiplied by that factor; a band above sea level keeps its
//! count (the map is `y - 1` and the span does not change). A column then
//! holds about as much ore per block of depth as Java, instead of Java's
//! full budget squeezed into the shorter underground.

/// Java build-height minimum. Samples below this are discarded, as in vanilla.
pub const JAVA_MIN_Y: i32 = -64;
/// Java overworld sea level. Heights at or above this map with `y - 1`.
pub const JAVA_SEA_Y: i32 = 63;
/// `(-64..=62)` has span 127 and maps onto our `(0..62)`, span 62.
pub const UNDERGROUND: f64 = 62.0 / 127.0;
/// Highest world y that can still be deepslate. Java blends over `[0, 8)`;
/// the inverse of that range ends at y = 35.
pub const DEEPSLATE_BLEND_TOP: i32 = 35;

/// World y for a Java overworld height.
///
/// `y >= 63` becomes `y - 1`. `y` in `[-64, 63]` becomes
/// `(y + 64) * 62 / 127` (so 0 → 31 and 16 → 39). Below -64 the same line
/// continues and is negative, which callers drop.
pub fn java_y(y: i32) -> i32 {
    if y >= JAVA_SEA_Y {
        y - 1
    } else {
        // `div_euclid` so a height below Java's bedrock stays negative.
        ((y - JAVA_MIN_Y) * 62).div_euclid(127)
    }
}

/// How far to move a finished structure so its Java ceiling lands on
/// [`java_y`] of that ceiling. Pieces keep their height; only the anchor
/// moves, so corridors are not squashed by the underground compression.
pub fn ceiling_shift(java_top: i32) -> i32 {
    java_y(java_top) - java_top
}

/// Chance that stone at `world_y` is deepslate.
///
/// Java is certain below y = 0 and fades across `[0, 8)`. Those heights are
/// read back through the inverse of [`java_y`], so the fade sits just above
/// [`java_y`]`(0)` and is gone before y = 40.
pub fn deepslate_chance(world_y: i32) -> f32 {
    if world_y >= 62 {
        return 0.0;
    }
    let j = world_y as f32 * 127.0 / 62.0 - 64.0;
    if j < 0.0 {
        1.0
    } else if j < 8.0 {
        (8.0 - j) / 8.0
    } else {
        0.0
    }
}

/// Share of a Java height span that remains after [`java_y`]. Fully
/// underground bands return [`UNDERGROUND`]; bands at or above sea level
/// return 1. Straddling bands blend the two by how much of the span is
/// below sea level.
pub fn span_scale(min: i32, max: i32) -> f64 {
    let lo = min.max(JAVA_MIN_Y);
    let hi = max;
    if hi <= lo {
        return UNDERGROUND;
    }
    let span = f64::from(hi - lo);
    let below = f64::from((JAVA_SEA_Y.min(hi) - lo).max(0));
    let above = f64::from((hi - JAVA_SEA_Y.max(lo)).max(0));
    (below * UNDERGROUND + above) / span
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sea_level_and_the_underground_anchors() {
        assert_eq!(java_y(-64), 0);
        assert_eq!(java_y(0), 31);
        assert_eq!(java_y(16), 39);
        assert_eq!(java_y(63), 62);
        assert_eq!(java_y(64), 63);
        assert_eq!(java_y(136), 135);
        assert!(java_y(-65) < 0, "below Java bedrock stays out of the world");
        // Continuous at the sea: the linear branch and `y - 1` agree.
        assert_eq!(java_y(63), 62);
    }

    #[test]
    fn underground_scale_is_62_over_127() {
        assert!((UNDERGROUND - 62.0 / 127.0).abs() < 1e-12);
        assert!((span_scale(-24, 56) - UNDERGROUND).abs() < 1e-12, "a band under the sea");
        assert!((span_scale(80, 384) - 1.0).abs() < 1e-12, "a band above the sea keeps its span");
        let copper = span_scale(-16, 112);
        assert!(copper > UNDERGROUND && copper < 1.0, "a band across the sea is in between, got {copper}");
    }

    #[test]
    fn deepslate_fades_out_below_y_40() {
        assert_eq!(deepslate_chance(java_y(0)), 1.0);
        assert_eq!(deepslate_chance(0), 1.0);
        assert!(deepslate_chance(32) < 1.0 && deepslate_chance(32) > 0.0, "the blend is above the line");
        assert_eq!(deepslate_chance(36), 0.0);
        assert_eq!(deepslate_chance(40), 0.0);
        assert_eq!(deepslate_chance(DEEPSLATE_BLEND_TOP), deepslate_chance(35));
        assert!(deepslate_chance(DEEPSLATE_BLEND_TOP) > 0.0);
    }
}
