//! Chunk lighting and greedy meshing.
//!
//! A mesh job copies the chunk plus a 15-block margin from its 26
//! neighbours into a flat [`Region`], flood-fills sky and block light there,
//! then greedily meshes the centre 32³. Light travels at most 15 blocks, so
//! the margin makes every border value exact: adjacent chunks agree without
//! sharing any mutable light state, and jobs run fully in parallel.
//!
//! Each vertex is two `u32`s:
//!
//! | word | bits  | field                                    |
//! |------|-------|------------------------------------------|
//! | 0    | 0-5   | x (0..=32, chunk-local)                  |
//! | 0    | 6-11  | y                                        |
//! | 0    | 12-17 | z                                        |
//! | 0    | 18-20 | face (0 +X, 1 -X, 2 +Y, 3 -Y, 4 +Z, 5 -Z)|
//! | 0    | 21-22 | ambient occlusion (0 dark .. 3 lit)      |
//! | 0    | 23-30 | texture array layer                      |
//! | 1    | 0-3   | smoothed sky light                       |
//! | 1    | 4-7   | smoothed block light                     |
//! | 1    | 8-12  | water surface drop, 1/16 block           |
//!
//! UVs are derived in the shader from the local position, so merged quads
//! tile their texture. All quads share one global index buffer.

use std::sync::Arc;

use crate::world::block::{Block, RenderKind};
use crate::world::chunk::{ChunkData, CHUNK_SIZE, CHUNK_SIZE_I, WORLD_HEIGHT};

/// Margin around the chunk that lighting needs to be exact.
pub const MARGIN: usize = 15;
/// Region edge length.
pub const D: usize = CHUNK_SIZE + 2 * MARGIN;
pub const REGION_VOLUME: usize = D * D * D;
const MAX_LIGHT: u8 = 15;
/// Height value for a column with no light-blocking blocks.
pub const NO_HEIGHT: i16 = i16::MIN;
const _: () = assert!(WORLD_HEIGHT < i16::MAX as i32);

/// Chunk plus its 26 neighbours. Index = (dx+1) + (dz+1)*3 + (dy+1)*9.
pub type Neighborhood = [Option<Arc<ChunkData>>; 27];

/// Everything a worker needs to light and mesh one chunk.
pub struct MeshInput {
    pub neighbors: Neighborhood,
    /// World Y of the highest light-blocking block per region column
    /// (`i16::MIN` if none), indexed `x + z * D`.
    pub heights: Box<[i16; D * D]>,
    /// World Y of the chunk's lowest block.
    pub base_y: i32,
}

/// Render passes, in the order their quads are stored.
pub const PASSES: usize = 3;
pub const OPAQUE: usize = 0;
pub const CUTOUT: usize = 1;
pub const TRANSLUCENT: usize = 2;

/// Per pass, the face directions (0 +X, 1 -X, 2 +Y, 3 -Y, 4 +Z, 5 -Z) in
/// the order their quad groups are stored. A camera sees at most one face
/// of each axis per chunk (both when it's level with the chunk on that
/// axis); +X +Y +Z -X -Y -Z puts every pair of faces from different axes
/// next to each other, so the visible groups form fewer contiguous draws
/// (1.75 per pass on average instead of 2.5). Translucent quads keep the
/// plain order: their blending depends on draw order, and there are few.
pub const FACE_ORDER: [[usize; 6]; PASSES] = [[0, 2, 4, 1, 3, 5], [0, 2, 4, 1, 3, 5], [0, 1, 2, 3, 4, 5]];

#[derive(Default, Debug)]
pub struct MeshData {
    /// Opaque quads, then cutout, then translucent; within each pass,
    /// grouped by face direction in [`FACE_ORDER`] so the renderer can skip
    /// directions that face away from the camera. 4 vertices of 2 words
    /// each per quad.
    pub vertices: Vec<[u32; 2]>,
    /// Quad count per pass and face group (in [`FACE_ORDER`]).
    pub face_quads: [[u32; 6]; PASSES],
}

impl MeshData {
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    #[cfg(test)]
    pub fn pass_quads(&self, pass: usize) -> u32 {
        self.face_quads[pass].iter().sum()
    }
}

#[inline(always)]
fn ridx(x: usize, y: usize, z: usize) -> usize {
    x + z * D + y * D * D
}

/// Reusable per-worker scratch space (≈700 KiB).
pub struct Region {
    blocks: Vec<Block>,
    sky: Vec<u8>,
    block_light: Vec<u8>,
    queue: Vec<u32>,
}

impl Default for Region {
    fn default() -> Self {
        Self {
            blocks: vec![Block::AIR; REGION_VOLUME],
            sky: vec![0; REGION_VOLUME],
            block_light: vec![0; REGION_VOLUME],
            queue: Vec::with_capacity(1 << 16),
        }
    }
}

#[inline(always)]
fn pack_q(x: usize, y: usize, z: usize) -> u32 {
    (x | y << 8 | z << 16) as u32
}

impl Region {
    /// Copies blocks from the neighbourhood. Missing neighbours are air
    /// above the world and bedrock below it.
    fn fill(&mut self, n: &Neighborhood, base_y: i32) {
        // Per axis: (neighbour offset index, first local coord, first region coord, len).
        let spans = [(0usize, CHUNK_SIZE - MARGIN, 0usize, MARGIN), (1, 0, MARGIN, CHUNK_SIZE), (2, 0, MARGIN + CHUNK_SIZE, MARGIN)];
        for &(oy, ly0, ry0, hy) in &spans {
            for &(oz, lz0, rz0, hz) in &spans {
                for &(ox, lx0, rx0, hx) in &spans {
                    let chunk = &n[ox + oz * 3 + oy * 9];
                    for y in 0..hy {
                        for z in 0..hz {
                            let row = ridx(rx0, ry0 + y, rz0 + z);
                            let dst = &mut self.blocks[row..row + hx];
                            match chunk.as_deref() {
                                Some(ChunkData::Dense(b)) => {
                                    let src = crate::world::chunk::index(lx0, ly0 + y, lz0 + z);
                                    dst.copy_from_slice(&b[src..src + hx]);
                                }
                                Some(ChunkData::Uniform(b)) => dst.fill(*b),
                                None => {
                                    let wy = base_y + (ry0 + y) as i32 - MARGIN as i32;
                                    dst.fill(if wy < 0 { Block::BEDROCK } else { Block::AIR });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Breadth-first light propagation from everything already in the queue.
    fn propagate(blocks: &[Block], light: &mut [u8], queue: &mut Vec<u32>) {
        let mut head = 0;
        while head < queue.len() {
            let q = queue[head];
            head += 1;
            let (x, y, z) = ((q & 0xFF) as usize, (q >> 8 & 0xFF) as usize, (q >> 16) as usize);
            let level = light[ridx(x, y, z)];
            if level <= 1 {
                continue;
            }
            let mut visit = |nx: usize, ny: usize, nz: usize| {
                let i = ridx(nx, ny, nz);
                let op = blocks[i].light_opacity();
                if op >= MAX_LIGHT {
                    return;
                }
                let next = level.saturating_sub(1 + op);
                if next > light[i] {
                    light[i] = next;
                    queue.push(pack_q(nx, ny, nz));
                }
            };
            if x > 0 { visit(x - 1, y, z) }
            if x + 1 < D { visit(x + 1, y, z) }
            if y > 0 { visit(x, y - 1, z) }
            if y + 1 < D { visit(x, y + 1, z) }
            if z > 0 { visit(x, y, z - 1) }
            if z + 1 < D { visit(x, y, z + 1) }
        }
        queue.clear();
    }

    fn light(&mut self, heights: &[i16; D * D], base_y: i32) {
        let y0 = base_y - MARGIN as i32; // world Y of region row 0
        // Region row from which each column sees the sky directly.
        let exposed_from = |x: usize, z: usize| -> usize {
            let h = heights[x + z * D] as i32;
            (h + 1 - y0).clamp(0, D as i32) as usize
        };

        // Skylight: direct exposure, then spread sideways/down from the
        // boundary of the exposed volume.
        self.sky.fill(0);
        for z in 0..D {
            for x in 0..D {
                let from = exposed_from(x, z);
                for y in from..D {
                    self.sky[ridx(x, y, z)] = MAX_LIGHT;
                }
                if from < D && from > 0 {
                    self.queue.push(pack_q(x, from, z)); // spreads downward through leaves/water
                }
                // Exposed cells beside a taller neighbouring column light its overhangs.
                let mut top = from;
                for (nx, nz) in [(x.wrapping_sub(1), z), (x + 1, z), (x, z.wrapping_sub(1)), (x, z + 1)] {
                    if nx < D && nz < D {
                        top = top.max(exposed_from(nx, nz));
                    }
                }
                for y in from..top.min(D) {
                    self.queue.push(pack_q(x, y, z));
                }
            }
        }
        Self::propagate(&self.blocks, &mut self.sky, &mut self.queue);

        // Block light from emitters.
        self.block_light.fill(0);
        for y in 0..D {
            for z in 0..D {
                let row = ridx(0, y, z);
                for x in 0..D {
                    let e = self.blocks[row + x].emission();
                    if e > 0 {
                        self.block_light[row + x] = e;
                        self.queue.push(pack_q(x, y, z));
                    }
                }
            }
        }
        Self::propagate(&self.blocks, &mut self.block_light, &mut self.queue);
    }
}

#[inline(always)]
fn face_visible(b: Block, n: Block) -> bool {
    match n.kind() {
        RenderKind::Opaque => false,
        RenderKind::Invisible => true,
        _ => !(b == n && b.info().self_cull),
    }
}

const KIND_SHIFT: u64 = 24;
const AO_SHIFT: u64 = 16;
const LIGHT_SHIFT: u64 = 32;
const PRESENT: u64 = 1 << 31;

/// Lights and meshes one chunk.
pub fn build(input: &MeshInput, region: &mut Region) -> MeshData {
    region.fill(&input.neighbors, input.base_y);
    region.light(&input.heights, input.base_y);
    mesh_region(region)
}

/// Greedy-meshes the centre chunk of a lit region.
fn mesh_region(r: &Region) -> MeshData {
    let blocks = &r.blocks[..];
    let (sky, blk) = (&r.sky[..], &r.block_light[..]);
    let mut out: [Vec<[u32; 2]>; PASSES] = Default::default();
    // Where each (pass, face) run of `out` starts, in face order.
    let mut face_start = [[0usize; 7]; PASSES];
    let strides = [1isize, (D * D) as isize, D as isize]; // x, y, z
    let mut mask = [0u64; CHUNK_SIZE * CHUNK_SIZE];

    for face in 0..6 {
        for (starts, quads) in face_start.iter_mut().zip(&out) {
            starts[face] = quads.len();
        }
        let d = face / 2;
        let positive = face % 2 == 0;
        let (u, v) = ((d + 1) % 3, (d + 2) % 3);
        let sd = if positive { strides[d] } else { -strides[d] };
        let (su, sv) = (strides[u], strides[v]);

        for slice in 0..CHUNK_SIZE {
            let mut any = false;
            for vv in 0..CHUNK_SIZE {
                let mut p = [0usize; 3];
                p[d] = slice + MARGIN;
                p[u] = MARGIN;
                p[v] = vv + MARGIN;
                let mut i = ridx(p[0], p[1], p[2]) as isize;
                for uu in 0..CHUNK_SIZE {
                    let b = blocks[i as usize];
                    let ni = i + sd;
                    let n = blocks[ni as usize];
                    let mut key = 0;
                    // Water surfaces sit lower than a full block unless more
                    // water is stacked on top.
                    let drop_at = |idx: isize| -> u64 {
                        if blocks[(idx + strides[1]) as usize].is_water() { 0 } else { blocks[idx as usize].water_drop() as u64 }
                    };
                    let visible = if b.is_water() {
                        if n.is_water() {
                            // Only a side stepping down to lower water shows.
                            d != 1 && drop_at(ni) > drop_at(i)
                        } else {
                            !n.is_opaque()
                        }
                    } else {
                        b.kind() != RenderKind::Invisible && face_visible(b, n)
                    };
                    if visible {
                        let kind: u64 = match b.kind() {
                            RenderKind::Cutout => 1,
                            RenderKind::Translucent => 2,
                            _ => 0,
                        };
                        let at = |off: isize| (ni + off) as usize;
                        let o = |off: isize| blocks[at(off)].is_opaque();
                        let (um, up, vm, vp) = (o(-su), o(su), o(-sv), o(sv));
                        // Per corner (-u-v, +u-v, +u+v, -u+v): side offsets.
                        let corners = [
                            (um, vm, -su, -sv),
                            (up, vm, su, -sv),
                            (up, vp, su, sv),
                            (um, vp, -su, sv),
                        ];
                        let (mut ao, mut light) = (0u64, 0u64);
                        for (c, &(s1, s2, du, dv)) in corners.iter().enumerate() {
                            let corner = o(du + dv);
                            let a = if kind == 2 {
                                3 // no occlusion on water
                            } else if s1 && s2 {
                                0
                            } else {
                                3 - (s1 as u64 + s2 as u64 + corner as u64)
                            };
                            // Smooth lighting: average the transparent cells
                            // touching this vertex in front of the face.
                            let (mut ls, mut lb, mut cnt) = (sky[at(0)] as u32, blk[at(0)] as u32, 1u32);
                            if !s1 {
                                ls += sky[at(du)] as u32;
                                lb += blk[at(du)] as u32;
                                cnt += 1;
                            }
                            if !s2 {
                                ls += sky[at(dv)] as u32;
                                lb += blk[at(dv)] as u32;
                                cnt += 1;
                            }
                            if !corner && !(s1 && s2) {
                                ls += sky[at(du + dv)] as u32;
                                lb += blk[at(du + dv)] as u32;
                                cnt += 1;
                            }
                            let ls = ((ls + cnt / 2) / cnt) as u64;
                            let lb = ((lb + cnt / 2) / cnt) as u64;
                            ao |= a << (c * 2);
                            light |= (ls | lb << 4) << (c * 8);
                        }
                        if kind == 2 {
                            ao = drop_at(i); // water has no AO; carry the surface drop instead
                        }
                        let layer = b.info().tex[face] as u64;
                        key = PRESENT | kind << KIND_SHIFT | ao << AO_SHIFT | layer | light << LIGHT_SHIFT;
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
                    while vv + h < CHUNK_SIZE {
                        let row = (vv + h) * CHUNK_SIZE + uu;
                        if mask[row..row + w].iter().any(|&k| k != key) {
                            break;
                        }
                        h += 1;
                    }
                    for j in 0..h {
                        let row = (vv + j) * CHUNK_SIZE + uu;
                        mask[row..row + w].fill(0);
                    }

                    let plane = slice + positive as usize;
                    let corners = [(uu, vv), (uu + w, vv), (uu + w, vv + h), (uu, vv + h)];
                    let ao = |c: usize| ((key >> AO_SHIFT) >> (c * 2)) as u32 & 3;
                    let light = |c: usize| ((key >> LIGHT_SHIFT) >> (c * 8)) as u32 & 0xFF;
                    // Brightness proxy for picking the triangulation diagonal.
                    let bright = |c: usize| ao(c) * 16 + (light(c) & 15) + (light(c) >> 4);
                    // (For water the AO bits hold the surface drop; it's the
                    // same for all corners so it doesn't affect this choice.)
                    let mut order = if positive { [0, 1, 2, 3] } else { [0, 3, 2, 1] };
                    // Split along the diagonal that keeps AO/light gradients
                    // symmetric (avoids the classic anisotropy artefact).
                    if bright(order[0]) + bright(order[2]) < bright(order[1]) + bright(order[3]) {
                        order.rotate_left(1);
                    }
                    let kind = ((key >> KIND_SHIFT) & 3) as usize;
                    let layer = (key & 0xFF) as u32;
                    let water_drop = if kind == 2 { ((key >> AO_SHIFT) & 0xFF) as u32 } else { 0 };
                    let top_y = (if d == 1 { plane } else if u == 1 { uu + w } else { vv + h }) as u32;
                    for c in order {
                        let mut pos = [0u32; 3];
                        pos[d] = plane as u32;
                        pos[u] = corners[c].0 as u32;
                        pos[v] = corners[c].1 as u32;
                        let (vertex_ao, drop) = if kind == 2 {
                            // Lower the upper edge of water faces (never the bottom face).
                            (3, if face != 3 && pos[1] == top_y { water_drop } else { 0 })
                        } else {
                            (ao(c), 0)
                        };
                        let w0 = pos[0]
                            | pos[1] << 6
                            | pos[2] << 12
                            | (face as u32) << 18
                            | vertex_ao << 21
                            | layer << 23;
                        out[kind].push([w0, light(c) | drop << 8]);
                    }
                    uu += w;
                }
            }
        }
    }

    // Concatenate the passes, reordering face groups into FACE_ORDER.
    let mut mesh = MeshData {
        vertices: Vec::with_capacity(out.iter().map(Vec::len).sum()),
        face_quads: [[0; 6]; PASSES],
    };
    for pass in 0..PASSES {
        face_start[pass][6] = out[pass].len();
        for (group, &face) in FACE_ORDER[pass].iter().enumerate() {
            let run = &out[pass][face_start[pass][face]..face_start[pass][face + 1]];
            mesh.vertices.extend_from_slice(run);
            mesh.face_quads[pass][group] = (run.len() / 4) as u32;
        }
    }
    mesh
}

/// World Y of the highest light-blocking block in each column of a chunk,
/// indexed `x + z * 32`; `i16::MIN` where the column is clear.
pub fn chunk_heights(data: &ChunkData, base_y: i32) -> [i16; CHUNK_SIZE * CHUNK_SIZE] {
    let mut out = [NO_HEIGHT; CHUNK_SIZE * CHUNK_SIZE];
    match data {
        ChunkData::Uniform(b) => {
            if b.light_opacity() > 0 {
                out.fill((base_y + CHUNK_SIZE_I - 1) as i16);
            }
        }
        ChunkData::Dense(_) => {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    if let Some(y) = (0..CHUNK_SIZE).rev().find(|&y| data.get(x, y, z).light_opacity() > 0) {
                        out[x + z * CHUNK_SIZE] = (base_y + y as i32) as i16;
                    }
                }
            }
        }
    }
    out
}


#[cfg(test)]
mod tests {
    use super::*;

    /// Meshes a single chunk surrounded by air with open sky.
    fn mesh_blocks(blocks: &[([usize; 3], Block)]) -> MeshData {
        let mut data = ChunkData::Uniform(Block::AIR);
        for &(p, b) in blocks {
            data.set(p[0], p[1], p[2], b);
        }
        let mut n: Neighborhood = Default::default();
        for (i, slot) in n.iter_mut().enumerate() {
            *slot = Some(Arc::new(if i == 13 { data.clone() } else { ChunkData::Uniform(Block::AIR) }));
        }
        let hm = chunk_heights(&data, 64);
        let mut heights = Box::new([NO_HEIGHT; D * D]);
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                heights[(x + MARGIN) + (z + MARGIN) * D] = hm[x + z * CHUNK_SIZE];
            }
        }
        build(&MeshInput { neighbors: n, heights, base_y: 64 }, &mut Region::default())
    }

    #[test]
    fn single_block_has_six_faces() {
        let m = mesh_blocks(&[([5, 5, 5], Block::STONE)]);
        assert_eq!(m.pass_quads(OPAQUE), 6);
        assert_eq!(m.vertices.len(), 24);
        assert_eq!(m.face_quads[OPAQUE], [1; 6]);
    }

    #[test]
    fn quads_are_grouped_by_face_in_face_order() {
        let blocks = [
            ([5, 5, 5], Block::STONE),
            ([9, 5, 5], Block::STONE),
            ([9, 9, 9], Block::LEAVES),
            ([20, 20, 20], Block::WATER),
        ];
        let m = mesh_blocks(&blocks);
        assert_eq!(m.face_quads, [[2; 6], [1; 6], [1; 6]]);
        let faces: Vec<u32> = m.vertices.chunks(4).map(|q| (q[0][0] >> 18) & 7).collect();
        let fq = m.face_quads;
        let expected: Vec<u32> = (0..PASSES)
            .flat_map(|pass| FACE_ORDER[pass].iter().flat_map(move |&f| vec![f as u32; fq[pass][0] as usize]))
            .collect();
        assert_eq!(faces, expected);
    }

    #[test]
    fn adjacent_blocks_merge() {
        let m = mesh_blocks(&[([5, 5, 5], Block::STONE), ([6, 5, 5], Block::STONE)]);
        assert_eq!(m.pass_quads(OPAQUE), 6);
    }

    #[test]
    fn full_layer_merges_lit_faces_and_shades_underside() {
        let blocks: Vec<_> = (0..32).flat_map(|x| (0..32).map(move |z| ([x, 0, z], Block::STONE))).collect();
        let m = mesh_blocks(&blocks);
        let quads_on = |face: u32| m.vertices.iter().filter(|v| (v[0] >> 18) & 7 == face).count() / 4;
        // Sunlit top and sides merge into one quad each...
        for face in [0, 1, 2, 4, 5] {
            assert_eq!(quads_on(face), 1, "face {face}");
        }
        // ...while skylight fades toward the middle of the underside.
        assert!(quads_on(3) > 1);
    }

    #[test]
    fn water_and_leaves_go_to_their_passes() {
        let m = mesh_blocks(&[([1, 1, 1], Block::WATER), ([2, 1, 1], Block::WATER), ([10, 10, 10], Block::LEAVES)]);
        assert_eq!(m.pass_quads(OPAQUE), 0);
        assert_eq!(m.pass_quads(CUTOUT), 6);
        assert_eq!(m.pass_quads(TRANSLUCENT), 6);
    }

    #[test]
    fn ambient_occlusion_darkens_corners() {
        let m = mesh_blocks(&[([5, 0, 5], Block::STONE), ([6, 1, 5], Block::STONE)]);
        assert!(m.vertices.iter().any(|v| (v[0] >> 21) & 3 < 3));
    }

    #[test]
    fn sky_is_bright_and_enclosed_space_is_dark() {
        // A sealed 3x3x3 stone box with an air cell inside: the inner faces
        // must be unlit while the outer top face sees full sky.
        let mut blocks = Vec::new();
        for x in 10..13 {
            for y in 10..13 {
                for z in 10..13 {
                    if (x, y, z) != (11, 11, 11) {
                        blocks.push(([x, y, z], Block::STONE));
                    }
                }
            }
        }
        let m = mesh_blocks(&blocks);
        let sky_levels: Vec<u32> = m.vertices.iter().map(|v| v[1] & 15).collect();
        assert!(sky_levels.contains(&15));
        assert!(sky_levels.contains(&0));
    }

    #[test]
    fn glowstone_lights_its_surroundings() {
        // Glowstone in a sealed room lights the inside walls.
        let mut blocks = Vec::new();
        for x in 8..15 {
            for y in 8..15 {
                for z in 8..15 {
                    let edge = x == 8 || x == 14 || y == 8 || y == 14 || z == 8 || z == 14;
                    if edge {
                        blocks.push(([x, y, z], Block::STONE));
                    }
                }
            }
        }
        blocks.push(([11, 11, 11], Block::GLOWSTONE));
        let m = mesh_blocks(&blocks);
        assert!(m.vertices.iter().any(|v| (v[1] >> 4) & 15 >= 12));
    }
}
