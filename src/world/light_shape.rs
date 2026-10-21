//! Java 26.3 LightEngine's faceShapeOccludes for partial states that use
//! their shape for light occlusion (slabs, every stairs half and corner).
//! Geometry uses exact 1/16 face cells, interned once per distinct shape;
//! each propagation edge needs two byte lookups and four union comparisons.
use std::sync::LazyLock;

use super::block::{Block, STATE_CAPACITY};
use super::shape::{self, Box16, Boxes};

type Face = [u64; 4];
type Faces = [Face; 6];
const FULL: Face = [u64::MAX; 4];

struct Table {
    states: [u8; STATE_CAPACITY],
    faces: Vec<Faces>,
}

static TABLE: LazyLock<Table> = LazyLock::new(|| {
    let mut table = Table { states: [0; STATE_CAPACITY], faces: vec![[[0; 4]; 6]] };
    // Thousands of stairs and slab states share a few dozen shapes; build
    // each shape's face masks once.
    let mut shapes: Vec<(Boxes, u8)> = Vec::new();
    for id in 0..STATE_CAPACITY {
        let block = Block(id as u16);
        if !block.uses_shape_for_light_occlusion() {
            continue;
        }
        let boxes = if matches!(block.shaped(), Some(super::block::Shaped::Stairs(_) | super::block::Shaped::Slab)) {
            shape::shape(block, |_| Block::AIR, Block::AIR)
        } else {
            Boxes::from_box(Box16 { min: [0; 3], max: [16, 16 - block.top_drop(), 16] })
        };
        if let Some(&(_, index)) = shapes.iter().find(|(b, _)| b.as_slice() == boxes.as_slice()) {
            table.states[id] = index;
            continue;
        }
        let faces = faces(&boxes);
        let index = table.faces.iter().position(|f| *f == faces).unwrap_or_else(|| {
            table.faces.push(faces);
            table.faces.len() - 1
        });
        let index = u8::try_from(index).expect("light shape table exceeds byte index");
        shapes.push((boxes, index));
        table.states[id] = index;
    }
    table
});

fn faces(boxes: &Boxes) -> Faces {
    std::array::from_fn(|face| {
        let axis = face / 2;
        let positive = face % 2 == 0;
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        let mut mask = [0; 4];
        for bx in boxes.as_slice() {
            if if positive { bx.max[axis] == 16 } else { bx.min[axis] == 0 } {
                for y in bx.min[v]..bx.max[v].min(16) {
                    for x in bx.min[u]..bx.max[u].min(16) {
                        let i = usize::from(x) + usize::from(y) * 16;
                        mask[i / 64] |= 1 << (i % 64);
                    }
                }
            }
        }
        mask
    })
}

/// `face`: +X, -X, +Y, -Y, +Z, -Z, as in the mesh format.
#[inline]
pub fn occludes(from: Block, to: Block, face: usize) -> bool {
    let table = &*TABLE;
    let a = table.states[from.0 as usize] as usize;
    let b = table.states[to.0 as usize] as usize;
    if a | b == 0 {
        return false;
    }
    let a = table.faces[a][face];
    let b = table.faces[b][face ^ 1];
    (0..4).all(|i| a[i] | b[i] == u64::MAX)
}

pub fn full_face(block: Block, face: usize) -> bool {
    let table = &*TABLE;
    table.faces[table.states[block.0 as usize] as usize][face] == FULL
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::Facing;
    use crate::world::overworld_blocks::{SNOW_LAYER, snow_layer};

    #[test]
    fn slabs_snow_and_soil_only_cover_their_boundary_faces() {
        for block in
            [Block::slab_of(Block::STONE).unwrap(), SNOW_LAYER, snow_layer(7), Block::FARMLAND, Block::DIRT_PATH]
        {
            assert_eq!(block.light_opacity(), 0, "{block:?}");
            assert!(occludes(block, Block::AIR, 3));
            for face in [0, 1, 2, 4, 5] {
                assert!(!occludes(block, Block::AIR, face), "{block:?} {face}");
            }
        }
        assert_eq!(snow_layer(8).light_opacity(), 15);
        for face in 0..6 {
            assert!(occludes(snow_layer(8), Block::AIR, face));
        }
    }

    #[test]
    fn stairs_faces_rotate_and_union_exactly() {
        for facing in Facing::ALL {
            let stair = Block::STONE_STAIRS.with_facing(facing);
            let d = facing.opposite().offset();
            let back = if d.x != 0 { usize::from(d.x < 0) } else { 4 + usize::from(d.z < 0) };
            assert!(occludes(stair, Block::AIR, 3));
            assert!(occludes(stair, Block::AIR, back));
            assert!(!occludes(stair, Block::AIR, back ^ 1));
            assert!(!occludes(stair, Block::AIR, 2));
        }
        // Facing east has its tall half on the west. Facing west has its
        // tall half on the east: across Z their two L faces cover the plane.
        let east = Block::STONE_STAIRS.with_facing(Facing::East);
        let west = Block::STONE_STAIRS.with_facing(Facing::West);
        assert!(occludes(east, west, 4));
        assert!(!occludes(east, east, 4));
        // Stairs: 24 distinct face sets over facing, half and corner shape.
        assert!(TABLE.faces.len() < 48);
    }

    #[test]
    fn every_slab_and_stairs_state_occludes_exactly_its_full_faces() {
        use crate::world::slab_stair_states::{self as ss, Slab, SlabType, StairShape, Stairs};
        // Mesh face index of the side facing `f`.
        let side = |f: Facing| f.face();
        for front in Facing::ALL {
            for top in [false, true] {
                for shape in StairShape::ALL {
                    let block = ss::stairs_id(Stairs { family: 9, facing: front, top, shape, wet: false });
                    assert!(block.uses_shape_for_light_occlusion());
                    assert_eq!(block.light_opacity(), 0);
                    let facing = front.opposite();
                    let (cw, ccw) = (facing.clockwise(), facing.clockwise().opposite());
                    // The slab side is always whole; the step side never is.
                    assert!(full_face(block, if top { 2 } else { 3 }));
                    assert!(!full_face(block, if top { 3 } else { 2 }));
                    // A side is whole when the step covers both of its quarters.
                    let whole = |f: Facing| match shape {
                        StairShape::Straight => f == facing,
                        StairShape::OuterLeft | StairShape::OuterRight => false,
                        StairShape::InnerLeft => f == facing || f == ccw,
                        StairShape::InnerRight => f == facing || f == cw,
                    };
                    for f in Facing::ALL {
                        assert_eq!(full_face(block, side(f)), whole(f), "{block:?} {f:?}");
                        assert_eq!(occludes(block, Block::AIR, side(f)), whole(f), "{block:?} {f:?}");
                    }
                }
            }
        }
        for family in 0..ss::FAMILY_COUNT as u8 {
            let top = ss::slab_id(Slab { family, kind: SlabType::Top, wet: false });
            assert!(occludes(top, Block::AIR, 2) && !occludes(top, Block::AIR, 3));
            assert_eq!(top.sky_stop_height(5), Some(5));
            let bottom = ss::slab_id(Slab { family, kind: SlabType::Bottom, wet: false });
            // Down from a top slab into a bottom slab: neither covers the
            // plane between them. Down from a bottom slab, its own underside does.
            assert!(!occludes(top, bottom, 3));
            assert!(occludes(bottom, top, 3));
            let double = ss::slab_id(Slab { family, kind: SlabType::Double, wet: false });
            assert!(!double.uses_shape_for_light_occlusion());
            assert_eq!(double.light_opacity(), 15);
        }
    }
}
