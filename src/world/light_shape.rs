//! Java 26.3 LightEngine's faceShapeOccludes for existing dry partial states.
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
    for id in 0..STATE_CAPACITY {
        let block = Block(id as u16);
        if !block.uses_shape_for_light_occlusion() {
            continue;
        }
        let boxes = if matches!(block.shaped(), Some(super::block::Shaped::Stairs(_))) {
            shape::shape(block, |_| Block::AIR, Block::AIR)
        } else {
            Boxes::from_box(Box16 { min: [0; 3], max: [16, 16 - block.top_drop(), 16] })
        };
        let faces = faces(&boxes);
        let index = table.faces.iter().position(|f| *f == faces).unwrap_or_else(|| {
            table.faces.push(faces);
            table.faces.len() - 1
        });
        table.states[id] = u8::try_from(index).expect("light shape table exceeds byte index");
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
        assert!(TABLE.faces.len() < 20);
    }
}
