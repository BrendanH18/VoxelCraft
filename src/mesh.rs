//! Greedy chunk mesher with per-vertex ambient occlusion.
//!
//! Output vertices are a single packed `u32`:
//!
//! | bits  | field                              |
//! |-------|------------------------------------|
//! | 0-5   | x (0..=32, chunk-local)            |
//! | 6-11  | y                                  |
//! | 12-17 | z                                  |
//! | 18-20 | face (0 +X, 1 -X, 2 +Y, 3 -Y, 4 +Z, 5 -Z) |
//! | 21-22 | ambient occlusion level (0 dark .. 3 lit) |
//! | 23-30 | texture array layer                |
//!
//! UVs are derived in the shader from the local position, so greedily merged
//! quads tile their texture for free. Quads share one global index buffer.

use std::sync::Arc;

use crate::world::block::{Block, RenderKind};
use crate::world::chunk::{ChunkData, CHUNK_SIZE};

/// Padded edge length: one block of neighbour data on every side.
pub const P: usize = CHUNK_SIZE + 2;
pub const PADDED_VOLUME: usize = P * P * P;

/// Chunk plus its 26 neighbours. Index = (dx+1) + (dz+1)*3 + (dy+1)*9.
pub type Neighborhood = [Option<Arc<ChunkData>>; 27];

#[derive(Default, Debug)]
pub struct MeshData {
    /// Opaque quads, then cutout quads, then translucent quads (4 vertices each).
    pub vertices: Vec<u32>,
    pub opaque_quads: u32,
    pub cutout_quads: u32,
    pub translucent_quads: u32,
}

impl MeshData {
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }
}

#[inline(always)]
fn pidx(x: usize, y: usize, z: usize) -> usize {
    x + z * P + y * P * P
}

/// Copies a chunk and a one-block border from its neighbours into a flat
/// padded array. Missing neighbours are treated as air, except below the
/// world where they're solid so the bedrock floor isn't meshed.
pub fn build_padded(n: &Neighborhood, out: &mut [Block; PADDED_VOLUME]) {
    // For each padded coordinate: which neighbour (0..3) and its local coord.
    let (mut off, mut loc) = ([0usize; P], [0usize; P]);
    for i in 0..P {
        let l = i as isize - 1;
        off[i] = if l < 0 { 0 } else if l >= CHUNK_SIZE as isize { 2 } else { 1 };
        loc[i] = l.rem_euclid(CHUNK_SIZE as isize) as usize;
    }
    for py in 0..P {
        for pz in 0..P {
            for px in 0..P {
                let ni = off[px] + off[pz] * 3 + off[py] * 9;
                let b = match &n[ni] {
                    Some(c) => c.get(loc[px], loc[py], loc[pz]),
                    None if off[py] == 0 => Block::BEDROCK,
                    None => Block::AIR,
                };
                out[pidx(px, py, pz)] = b;
            }
        }
    }
}

pub fn new_padded() -> Box<[Block; PADDED_VOLUME]> {
    vec![Block::AIR; PADDED_VOLUME].into_boxed_slice().try_into().unwrap()
}

#[inline(always)]
fn face_visible(b: Block, n: Block) -> bool {
    match n.kind() {
        RenderKind::Opaque => false,
        RenderKind::Invisible => true,
        _ => !(b == n && b.info().self_cull),
    }
}

#[inline(always)]
fn pack(p: [usize; 3], face: usize, ao: u32, layer: u32) -> u32 {
    p[0] as u32
        | (p[1] as u32) << 6
        | (p[2] as u32) << 12
        | (face as u32) << 18
        | ao << 21
        | layer << 23
}

const KIND_SHIFT: u32 = 24;
const AO_SHIFT: u32 = 16;
const PRESENT: u32 = 1 << 31;

/// Meshes the centre 32³ of a padded block array.
pub fn mesh_chunk(blocks: &[Block; PADDED_VOLUME]) -> MeshData {
    let mut out: [Vec<u32>; 3] = Default::default();
    let strides = [1isize, (P * P) as isize, P as isize]; // x, y, z
    let mut mask = [0u32; CHUNK_SIZE * CHUNK_SIZE];

    for face in 0..6 {
        let d = face / 2;
        let positive = face % 2 == 0;
        let (u, v) = ((d + 1) % 3, (d + 2) % 3);
        let sd = if positive { strides[d] } else { -strides[d] };
        let (su, sv) = (strides[u], strides[v]);

        for slice in 0..CHUNK_SIZE {
            let mut any = false;
            for vv in 0..CHUNK_SIZE {
                let mut p = [0usize; 3];
                p[d] = slice + 1;
                p[u] = 1;
                p[v] = vv + 1;
                let mut i = pidx(p[0], p[1], p[2]) as isize;
                for uu in 0..CHUNK_SIZE {
                    let b = blocks[i as usize];
                    let n = blocks[(i + sd) as usize];
                    let mut key = 0;
                    if b.kind() != RenderKind::Invisible && face_visible(b, n) {
                        let kind = match b.kind() {
                            RenderKind::Cutout => 1,
                            RenderKind::Translucent => 2,
                            _ => 0,
                        };
                        let ao = if kind == 2 {
                            0xFF // no occlusion on water
                        } else {
                            let ni = i + sd;
                            let o = |off: isize| blocks[(ni + off) as usize].is_opaque() as u32;
                            let corner = |s1: u32, s2: u32, c: u32| {
                                if s1 == 1 && s2 == 1 { 0 } else { 3 - (s1 + s2 + c) }
                            };
                            let (um, up, vm, vp) = (o(-su), o(su), o(-sv), o(sv));
                            corner(um, vm, o(-su - sv))
                                | corner(up, vm, o(su - sv)) << 2
                                | corner(up, vp, o(su + sv)) << 4
                                | corner(um, vp, o(-su + sv)) << 6
                        };
                        let layer = b.info().tex[face] as u32;
                        key = PRESENT | kind << KIND_SHIFT | ao << AO_SHIFT | layer;
                        any = true;
                    }
                    mask[vv * CHUNK_SIZE + uu] = key;
                    i += su;
                }
            }
            if !any {
                continue;
            }

            // Greedy merge of identical keys into rectangles.
            for vv in 0..CHUNK_SIZE {
                let mut uu = 0;
                while uu < CHUNK_SIZE {
                    let key = mask[vv * CHUNK_SIZE + uu];
                    if key == 0 {
                        uu += 1;
                        continue;
                    }
                    let mut w = 1;
                    while uu + w < CHUNK_SIZE && mask[vv * CHUNK_SIZE + uu + w] == key {
                        w += 1;
                    }
                    let mut h = 1;
                    'grow: while vv + h < CHUNK_SIZE {
                        let row = (vv + h) * CHUNK_SIZE + uu;
                        if mask[row..row + w].iter().any(|&k| k != key) {
                            break 'grow;
                        }
                        h += 1;
                    }
                    for j in 0..h {
                        let row = (vv + j) * CHUNK_SIZE + uu;
                        mask[row..row + w].fill(0);
                    }

                    let plane = slice + positive as usize;
                    let corners = [(uu, vv), (uu + w, vv), (uu + w, vv + h), (uu, vv + h)];
                    let ao_bits = (key >> AO_SHIFT) & 0xFF;
                    let ao = |c: usize| (ao_bits >> (c * 2)) & 3;
                    let mut order = if positive { [0, 1, 2, 3] } else { [0, 3, 2, 1] };
                    // Split the quad along the diagonal that keeps AO
                    // gradients symmetric (avoids the classic anisotropy).
                    if ao(order[0]) + ao(order[2]) < ao(order[1]) + ao(order[3]) {
                        order.rotate_left(1);
                    }
                    let kind = ((key >> KIND_SHIFT) & 3) as usize;
                    let layer = key & 0xFF;
                    for c in order {
                        let mut pos = [0usize; 3];
                        pos[d] = plane;
                        pos[u] = corners[c].0;
                        pos[v] = corners[c].1;
                        out[kind].push(pack(pos, face, ao(c), layer));
                    }
                    uu += w;
                }
            }
        }
    }

    let [opaque, cutout, translucent] = out;
    let mut mesh = MeshData {
        opaque_quads: (opaque.len() / 4) as u32,
        cutout_quads: (cutout.len() / 4) as u32,
        translucent_quads: (translucent.len() / 4) as u32,
        vertices: opaque,
    };
    mesh.vertices.extend_from_slice(&cutout);
    mesh.vertices.extend_from_slice(&translucent);
    mesh
}

/// Convenience wrapper: gather neighbours and mesh.
pub fn mesh_neighborhood(n: &Neighborhood, scratch: &mut [Block; PADDED_VOLUME]) -> MeshData {
    build_padded(n, scratch);
    mesh_chunk(scratch)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn padded_with(blocks: &[([usize; 3], Block)]) -> Box<[Block; PADDED_VOLUME]> {
        let mut p = new_padded();
        for &(pos, b) in blocks {
            p[pidx(pos[0] + 1, pos[1] + 1, pos[2] + 1)] = b;
        }
        p
    }

    #[test]
    fn single_block_has_six_faces() {
        let m = mesh_chunk(&padded_with(&[([5, 5, 5], Block::STONE)]));
        assert_eq!(m.opaque_quads, 6);
        assert_eq!(m.vertices.len(), 24);
    }

    #[test]
    fn adjacent_blocks_merge() {
        let m = mesh_chunk(&padded_with(&[([5, 5, 5], Block::STONE), ([6, 5, 5], Block::STONE)]));
        assert_eq!(m.opaque_quads, 6);
    }

    #[test]
    fn full_layer_is_two_big_quads_plus_edges() {
        let blocks: Vec<_> = (0..32)
            .flat_map(|x| (0..32).map(move |z| ([x, 0, z], Block::STONE)))
            .collect();
        let m = mesh_chunk(&padded_with(&blocks));
        // Top, bottom, and one strip per side.
        assert_eq!(m.opaque_quads, 6);
    }

    #[test]
    fn water_and_leaves_go_to_their_passes() {
        let m = mesh_chunk(&padded_with(&[
            ([1, 1, 1], Block::WATER),
            ([2, 1, 1], Block::WATER),
            ([10, 10, 10], Block::LEAVES),
        ]));
        assert_eq!(m.opaque_quads, 0);
        assert_eq!(m.cutout_quads, 6);
        assert_eq!(m.translucent_quads, 6);
    }

    #[test]
    fn ambient_occlusion_darkens_corners() {
        // A block on the ground next to a wall: its top face touches the wall.
        let m = mesh_chunk(&padded_with(&[([5, 0, 5], Block::STONE), ([6, 1, 5], Block::STONE)]));
        let has_dark = m.vertices.iter().any(|v| (v >> 21) & 3 < 3);
        assert!(has_dark);
    }
}
