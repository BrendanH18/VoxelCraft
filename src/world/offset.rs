//! v1.0 gap track 6c: Java's random model offset (`BlockBehaviour.OffsetType`).
//!
//! Java 26.3 moves the models of 39 blocks (flowers, grasses, ferns,
//! bamboo, roots, sprouts, pointed dripstone and the rest of [`kind`])
//! sideways within their cell by an amount seeded from the column
//! (`Mth.getSeed(x, 0, z)`): up to a quarter block on x and z in sixteen
//! steps of 1/30 (`XZ`), and for the short grasses, ferns and the small
//! dripleaf also down by up to a fifth (a tenth for the dripleaf) in
//! sixteen steps (`XYZ`). Pointed dripstone and sulfur spikes clamp their
//! side offset to an eighth (`SpeleothemBlock`'s `MAX_HORIZONTAL_OFFSET`,
//! its base's inset). The two halves of a tall plant share their column's
//! offset.
//!
//! The terrain shader computes the same offset from each draw's chunk
//! origin (`chunk.wgsl`'s `block_offset`, checked against [`offset`] by
//! `render::shader_equivalence`); the quads only say which [`Kind`] applies.
//! Flowers, mangrove propagules, bamboo and its sapling and the speleothems
//! also move their outline by it, and bamboo and the speleothems their
//! collision ([`moves_outline`], [`moves_collision`]), as their Java
//! `getShape` and `getCollisionShape` do.

use std::sync::LazyLock;

use glam::DVec3;

use super::block::{Block, STATE_CAPACITY};

/// How a block's model is offset (stored in a cross quad's word 0, bits
/// 24-26).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    /// `XZ`, at most a quarter block.
    Xz = 1,
    /// `XZ` clamped to an eighth (`SpeleothemBlock`).
    XzSpeleothem = 2,
    /// `XYZ`, down at most a fifth.
    Xyz = 3,
    /// `XYZ` down at most a tenth (`SmallDripleafBlock.getMaxVerticalOffset`).
    XyzSmallDripleaf = 4,
}

impl Kind {
    const fn from_u8(v: u8) -> Option<Kind> {
        match v {
            1 => Some(Kind::Xz),
            2 => Some(Kind::XzSpeleothem),
            3 => Some(Kind::Xyz),
            4 => Some(Kind::XyzSmallDripleaf),
            _ => None,
        }
    }

    /// `getMaxHorizontalOffset`.
    pub fn max_horizontal(self) -> f32 {
        if self == Kind::XzSpeleothem { 0.125 } else { 0.25 }
    }

    /// `getMaxVerticalOffset` (for the `XYZ` kinds).
    pub fn max_vertical(self) -> f32 {
        if self == Kind::XyzSmallDripleaf { 0.1 } else { 0.2 }
    }
}

/// The game's names of the blocks Java gives an `offsetType` (`Blocks`),
/// with their kind. "tall grass" is Java's `short_grass` and "tall grass
/// plant" its `tall_grass`.
const OFFSET_BLOCKS: &[(&str, Kind)] = &[
    ("mangrove propagule", Kind::Xz),
    ("hanging mangrove propagule", Kind::Xz),
    ("tall grass", Kind::Xyz),
    ("fern", Kind::Xyz),
    ("short dry grass", Kind::Xyz),
    ("tall dry grass", Kind::Xyz),
    ("tall dry grass top", Kind::Xyz),
    ("tall seagrass", Kind::Xz),
    ("tall seagrass top", Kind::Xz),
    ("dandelion", Kind::Xz),
    ("golden dandelion", Kind::Xz),
    ("torchflower", Kind::Xz),
    ("poppy", Kind::Xz),
    ("blue orchid", Kind::Xz),
    ("allium", Kind::Xz),
    ("azure bluet", Kind::Xz),
    ("red tulip", Kind::Xz),
    ("orange tulip", Kind::Xz),
    ("white tulip", Kind::Xz),
    ("pink tulip", Kind::Xz),
    ("oxeye daisy", Kind::Xz),
    ("cornflower", Kind::Xz),
    ("wither rose", Kind::Xz),
    ("lily of the valley", Kind::Xz),
    ("sunflower", Kind::Xz),
    ("sunflower top", Kind::Xz),
    ("lilac", Kind::Xz),
    ("lilac top", Kind::Xz),
    ("rose bush", Kind::Xz),
    ("rose bush top", Kind::Xz),
    ("peony", Kind::Xz),
    ("peony top", Kind::Xz),
    ("tall grass plant", Kind::Xz),
    ("tall grass plant top", Kind::Xz),
    ("large fern", Kind::Xz),
    ("large fern top", Kind::Xz),
    ("pitcher plant", Kind::Xz),
    ("bamboo sapling", Kind::Xz),
    ("bamboo", Kind::Xz),
    ("warped roots", Kind::Xz),
    ("nether sprouts", Kind::Xz),
    ("crimson roots", Kind::Xz),
    ("pointed dripstone", Kind::XzSpeleothem),
    ("sulfur spike", Kind::XzSpeleothem),
    ("small dripleaf", Kind::XyzSmallDripleaf),
    ("small dripleaf top", Kind::XyzSmallDripleaf),
    ("hanging roots", Kind::Xz),
    ("open eyeblossom", Kind::Xz),
    ("closed eyeblossom", Kind::Xz),
];

/// Every state's [`Kind`] as a byte (0: none), built once from the names.
static KINDS: LazyLock<Box<[u8]>> = LazyLock::new(|| {
    (0..STATE_CAPACITY as u16)
        .map(|id| {
            // Every offset block is a cross or shaped (bamboo): only those
            // states compare names.
            let info = Block(id).canonical().info();
            if !matches!(info.kind, super::block::RenderKind::Cross | super::block::RenderKind::Shaped) {
                return 0;
            }
            OFFSET_BLOCKS.iter().find(|(n, _)| *n == info.name).map_or(0, |&(_, k)| k as u8)
        })
        .collect()
});

/// How `b`'s model is offset in Java, if it is.
pub fn kind(b: Block) -> Option<Kind> {
    Kind::from_u8(KINDS.get(b.0 as usize).copied().unwrap_or(0))
}

/// `Mth.getSeed(x, 0, z)`: Java's `x * 3129871` is an int product (it
/// wraps), the rest long arithmetic.
pub fn seed(x: i32, z: i32) -> i64 {
    let mut seed = i64::from(x.wrapping_mul(3_129_871)) ^ i64::from(z).wrapping_mul(116_129_781);
    seed = seed.wrapping_mul(seed).wrapping_mul(42_317_861).wrapping_add(seed.wrapping_mul(11));
    seed >> 16
}

/// v1.0 gap track 6d: `Mth.getSeed(x, y, z)`, the seed of a block's model
/// choice (`BlockState.getSeed`).
pub fn block_seed(x: i32, y: i32, z: i32) -> i64 {
    let mut seed = i64::from(x.wrapping_mul(3_129_871)) ^ i64::from(z).wrapping_mul(116_129_781) ^ i64::from(y);
    seed = seed.wrapping_mul(seed).wrapping_mul(42_317_861).wrapping_add(seed.wrapping_mul(11));
    seed >> 16
}

/// v1.0 gap track 6d: Java's `java.util.Random` linear congruential
/// generator, as the block renderer's `SingleThreadedRandomSource` runs it.
struct Lcg(u64);

impl Lcg {
    const MULTIPLIER: u64 = 0x5_DEEC_E66D;
    const MASK: u64 = (1 << 48) - 1;

    fn new(seed: i64) -> Self {
        Self((seed as u64 ^ Self::MULTIPLIER) & Self::MASK)
    }

    fn next(&mut self, bits: u32) -> i32 {
        self.0 = (self.0.wrapping_mul(Self::MULTIPLIER).wrapping_add(0xB)) & Self::MASK;
        (self.0 >> (48 - bits)) as i64 as i32
    }

    fn next_long(&mut self) -> i64 {
        (i64::from(self.next(32)) << 32).wrapping_add(i64::from(self.next(32)))
    }

    /// `nextInt(bound)` for a power-of-two bound.
    fn next_int_pow2(&mut self, bound: i32) -> i32 {
        ((i64::from(bound) * i64::from(self.next(31))) >> 31) as i32
    }
}

/// v1.0 gap track 6d: which of its four stalk models (`bamboo1_age*` to
/// `bamboo4_age*`, 0 to 3) Java draws bamboo with at (`x`, `y`, `z`):
/// `ModelBlockRenderer` seeds its random source with the block's seed, the
/// multipart model draws one long and reseeds with it for each part, and
/// the stalk's four equally weighted variants take `nextInt(4)`.
pub fn bamboo_model(x: i32, y: i32, z: i32) -> u8 {
    let mut random = Lcg::new(block_seed(x, y, z));
    let part = random.next_long();
    let mut random = Lcg::new(part);
    random.next_int_pow2(4) as u8
}

/// The offset of a `kind` model in column (`x`, `z`), in blocks, as Java's
/// offset function computes it (float division, double arithmetic, clamped
/// to the kind's limit).
pub fn offset_of(kind: Kind, x: i32, z: i32) -> DVec3 {
    let seed = seed(x, z);
    let max = f64::from(kind.max_horizontal());
    let side = |bits: i64| ((((bits & 15) as f32) / 15.0) as f64 - 0.5) * 0.5;
    let y = match kind {
        Kind::Xyz | Kind::XyzSmallDripleaf => {
            ((((seed >> 4 & 15) as f32) / 15.0) as f64 - 1.0) * f64::from(kind.max_vertical())
        }
        _ => 0.0,
    };
    DVec3::new(side(seed).clamp(-max, max), y, side(seed >> 8).clamp(-max, max))
}

/// `state.getOffset(pos)` for block `b` at `x`, `z` (zero for blocks Java
/// doesn't offset).
pub fn offset(b: Block, x: i32, z: i32) -> DVec3 {
    kind(b).map_or(DVec3::ZERO, |k| offset_of(k, x, z))
}

/// Whether Java moves `b`'s outline by its offset: `FlowerBlock` (the
/// flowers, torchflower, wither rose and eyeblossoms), mangrove
/// propagules, the bamboo sapling and stalk and the speleothems. Grasses,
/// ferns, tall plants, roots, sprouts, the small dripleaf and hanging roots
/// keep a centred outline.
pub fn moves_outline(b: Block) -> bool {
    const MOVED: &[&str] = &[
        "mangrove propagule",
        "hanging mangrove propagule",
        "dandelion",
        "golden dandelion",
        "torchflower",
        "poppy",
        "blue orchid",
        "allium",
        "azure bluet",
        "red tulip",
        "orange tulip",
        "white tulip",
        "pink tulip",
        "oxeye daisy",
        "cornflower",
        "wither rose",
        "lily of the valley",
        "bamboo sapling",
        "bamboo",
        "pointed dripstone",
        "sulfur spike",
        "open eyeblossom",
        "closed eyeblossom",
    ];
    kind(b).is_some() && MOVED.contains(&b.canonical().info().name)
}

/// Whether Java moves `b`'s collision by its offset: bamboo
/// (`SHAPE_COLLISION.move`) and the speleothems (their collision is their
/// moved outline).
pub fn moves_collision(b: Block) -> bool {
    matches!(b.canonical().info().name, "bamboo" | "pointed dripstone" | "sulfur spike")
}

/// The offset `b`'s outline at `cell` moves by ([`moves_outline`]), else
/// zero.
pub fn outline_offset(b: Block, cell: glam::IVec3) -> DVec3 {
    if moves_outline(b) { offset(b, cell.x, cell.z) } else { DVec3::ZERO }
}

/// A bamboo stalk's outline before its offset, relative to its cell:
/// `BambooStalkBlock`'s `SHAPE_LARGE` (`column(10, 0, 16)`) with large
/// leaves, else `SHAPE_SMALL` (`column(6, 0, 16)`).
pub fn bamboo_outline(b: Block) -> Option<(DVec3, DVec3)> {
    let (_, leaves) = super::overworld_blocks::bamboo_stalk(b.canonical())?;
    let half = if leaves == 2 { 5.0 } else { 3.0 } / 16.0;
    Some((DVec3::new(0.5 - half, 0.0, 0.5 - half), DVec3::new(0.5 + half, 1.0, 0.5 + half)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v1.0 gap track 6d: bamboo's stalk model at these positions, as
    /// Java's `java.util.Random` (the algorithm of the block renderer's
    /// `SingleThreadedRandomSource`) chooses it from `Mth.getSeed`: a
    /// separate Java program's output on Boat.
    #[test]
    fn bamboo_takes_javas_stalk_model() {
        for (x, y, z, model) in [
            (0, 0, 0, 1),
            (1, 64, 1, 1),
            (-7, 70, 13, 0),
            (100, -40, -3, 3),
            (12345, 80, -6789, 1),
            (-1, 0, -1, 0),
            (31, 63, 31, 0),
            (-30_000_000, 319, 29_999_999, 2),
        ] {
            assert_eq!(bamboo_model(x, y, z), model, "({x}, {y}, {z})");
        }
    }

    #[test]
    fn javas_seed_and_offsets() {
        // `Mth.getSeed(0, 0, 0)` is 0: the centre step nearest the middle.
        assert_eq!(seed(0, 0), 0);
        let o = offset_of(Kind::Xz, 0, 0);
        assert!((o.x + 0.25).abs() < 1e-9 && (o.z + 0.25).abs() < 1e-9 && o.y == 0.0, "{o}");
        // Every offset is a sixteenth-step of Java's grid, within its limit.
        for x in -40..40 {
            for z in -40..40 {
                let o = offset_of(Kind::Xz, x, z);
                assert!(o.x.abs() <= 0.25 + 1e-9 && o.z.abs() <= 0.25 + 1e-9);
                let steps = (o.x + 0.25) * 30.0;
                assert!((steps - steps.round()).abs() < 1e-5, "{x} {z} {o}");
                let s = offset_of(Kind::XzSpeleothem, x, z);
                assert!(s.x.abs() <= 0.125 + 1e-9 && s.z.abs() <= 0.125 + 1e-9);
                let y = offset_of(Kind::Xyz, x, z).y;
                assert!((-0.2 - 1e-6..=1e-9).contains(&y), "{y}");
            }
        }
    }

    #[test]
    fn javas_offset_blocks_are_known_by_name() {
        for (name, k) in OFFSET_BLOCKS {
            let b = Block::from_name(name).unwrap_or_else(|| panic!("{name} is not a block"));
            assert_eq!(kind(b), Some(*k), "{name}");
        }
        assert_eq!(kind(Block::STONE), None);
        assert!(moves_outline(super::super::overworld_blocks::BAMBOO));
        assert!(!moves_outline(Block::TALL_GRASS));
    }
}
