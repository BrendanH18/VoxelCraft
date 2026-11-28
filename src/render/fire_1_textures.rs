//! v1.0 final track F3: Java's second fire texture, original art.
//!
//! Java's fire models draw `fire_0` or `fire_1` (soul fire `soul_fire_0`
//! or `soul_fire_1`), picked per block by the position seed
//! (`mesh::variants::multipart_picks`), and a burning entity's flame
//! alternates them quad by quad. VoxelCraft's `fire_0` is seven frames
//! (`tex::FIRE_0`, `tex::SOUL_FIRE_0`); this is the second set in the same
//! style: narrower, single-column tongues that lean the other way and reach
//! higher, hot cores at their roots.
//!
//! The track has four texture layers, so the seven frames are packed: frame
//! `f` is colour channel `f % 3` of layer `tex::FIRE_1 + f / 3` (alpha
//! always 255, so mip levels average plainly), 0 where see-through and
//! otherwise `128 + heat * 127`. Soul fire's second texture shares these
//! tongues in its own colours. The shaders decode it (`chunk.wgsl`'s and
//! `entity.wgsl`'s `fire_1_color`, which [`color`] mirrors).

use super::{Rgba, SIZE, rnd};
use crate::world::block::tex;

/// Frame `frame`'s heat (0 to 1) at texel `(x, y)`, or `None` where it is
/// see-through.
pub(crate) fn heat(frame: usize, x: usize, y: usize) -> Option<f32> {
    let seed = tex::FIRE_1;
    // Rows lean two texels per four, the other way from `fire_0`.
    let lean = (rnd(seed, y / 4, frame, 2) * 2.0) as usize;
    let column = (x + SIZE - lean) % SIZE;
    // One tip per column, taller than `fire_0`'s pairs (0 to 10 rows down).
    let tip = (rnd(seed, column, frame, 3) * 11.0) as usize;
    // Embers break the upper flame apart.
    if y < tip || (y < 8 && rnd(seed + frame as u16, x, y, 4) < 0.2) {
        return None;
    }
    let r = rnd(seed + frame as u16, x, y, 0);
    Some(((y - tip) as f32 / (SIZE - tip) as f32 + r * 0.25).min(1.0))
}

/// The packed texel of layer `tex::FIRE_1 + index`.
pub(super) fn packed(index: u16, x: usize, y: usize) -> Rgba {
    let mut out = [0, 0, 0, 255];
    for (c, byte) in out.iter_mut().take(3).enumerate() {
        let frame = usize::from(index) * 3 + c;
        if frame < usize::from(tex::FIRE_FRAMES) {
            *byte = heat(frame, x, y).map_or(0, |h| 128 + (h * 127.0).round() as u8);
        }
    }
    out
}

/// A packed byte's colour (sRGB) as the shaders decode it: fire's or soul
/// fire's, the same ramps as `fire_0` and `soul_fire_0`; `None` where
/// see-through.
#[cfg(test)]
fn color(byte: u8, soul: bool) -> Option<[u8; 3]> {
    if byte < 128 {
        return None;
    }
    let h = f32::from(byte - 128) / 127.0;
    let c = if soul {
        [40.0 + 120.0 * h * h, 170.0 + 85.0 * h, 215.0 + 40.0 * h]
    } else {
        [255.0, 75.0 + 180.0 * h, 15.0 + 105.0 * h * h]
    };
    Some(c.map(|v: f32| v.round() as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every frame is drawn into its own channel, decodes to flame where it
    /// has heat, has see-through texels between its tongues and differs
    /// from `fire_0`'s frame and from its neighbours.
    #[test]
    fn the_second_fire_texture_is_packed_one_frame_a_channel() {
        let frames = usize::from(tex::FIRE_FRAMES);
        let mut images = Vec::new();
        for frame in 0..frames {
            let mut image = Vec::new();
            for y in 0..SIZE {
                for x in 0..SIZE {
                    let px = packed((frame / 3) as u16, x, y);
                    assert_eq!(px[3], 255);
                    let byte = px[frame % 3];
                    assert_eq!(color(byte, false).is_some(), heat(frame, x, y).is_some());
                    image.push(color(byte, false).is_some());
                }
            }
            let shown = image.iter().filter(|&&s| s).count();
            assert!(shown > SIZE * SIZE / 4 && shown < SIZE * SIZE * 7 / 8, "frame {frame}: {shown}");
            let fire_0: Vec<bool> = (0..SIZE * SIZE)
                .map(|i| super::super::pixel(tex::FIRE_0 + frame as u16, i % SIZE, i / SIZE)[3] > 0)
                .collect();
            assert_ne!(image, fire_0, "frame {frame} is fire_0's");
            images.push(image);
        }
        for f in 1..frames {
            assert_ne!(images[f], images[f - 1], "frames {f} and {}", f - 1);
        }
        // The last layer's unused channels are see-through.
        assert_eq!(packed(2, 3, 15)[1..3], [0, 0]);
        // The bottom row burns in every frame.
        assert!((0..frames).all(|f| (0..SIZE).all(|x| heat(f, x, SIZE - 1).is_some())));
    }

    /// Both shaders decode with the same threshold and ramps as [`color`].
    #[test]
    fn decoded_colours_follow_the_fire_ramps() {
        for shader in [include_str!("shaders/chunk.wgsl"), include_str!("shaders/entity.wgsl")] {
            for text in [
                "if e < 127.5",
                "(round(e) - 128.0) / 127.0",
                "vec3<f32>(255.0, 75.0 + 180.0 * h, 15.0 + 105.0 * h * h)",
            ] {
                assert!(shader.contains(text), "{text}");
            }
        }
        assert!(
            include_str!("shaders/chunk.wgsl")
                .contains("vec3<f32>(40.0 + 120.0 * h * h, 170.0 + 85.0 * h, 215.0 + 40.0 * h)")
        );
        assert_eq!(color(0, false), None);
        assert_eq!(color(127, true), None);
        assert_eq!(color(128, false), Some([255, 75, 15]));
        assert_eq!(color(255, false), Some([255, 255, 120]));
        assert_eq!(color(255, true), Some([160, 255, 255]));
    }
}
