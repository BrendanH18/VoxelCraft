//! Chunk lighting and greedy meshing.
//!
//! A mesh job copies the chunk plus a 15-block margin from its 26
//! neighbours into a flat [`Region`], flood-fills sky and block light there,
//! then greedily meshes the centre 32³. Light travels at most 15 blocks, so
//! the margin makes every border value exact: adjacent chunks agree without
//! sharing any mutable light state, and jobs run fully in parallel.
//!
//! Each quad is one 12-byte record (three `u32`s) that the vertex shader
//! expands into its four corners (vertex pulling):
//!
//! | word | bits  | field                                              |
//! |------|-------|----------------------------------------------------|
//! | 0    | 0-5   | x of corner 0 (0..=32, chunk-local)                |
//! | 0    | 6-11  | y                                                  |
//! | 0    | 12-17 | z                                                  |
//! | 0    | 18-20 | face (0 +X, 1 -X, 2 +Y, 3 -Y, 4 +Z, 5 -Z, 6-7 X)   |
//! | 0    | 21-25 | width - 1 (along the face's u axis)                |
//! | 0    | 26-30 | height - 1 (along v)                               |
//! | 0    | 31    | flip: triangulate along the other diagonal         |
//! | 1    | 0-7   | texture layer, low 8 bits                          |
//! | 1    | 8-15  | ambient occlusion per corner, 2 bits (0 dark .. 3) |
//! | 1    | 16-20 | water surface drop of the upper edge, 1/16 block   |
//! | 1    | 21-23 | texture layer, high 3 bits (normal and cross quads) |
//! | 2    | 0-31  | per corner: sky light (low 4 bits), block light    |
//!
//! For a face on axis `d` the u and v axes are `(d + 1) % 3` and
//! `(d + 2) % 3`; corners 0-3 are (0,0), (w,0), (w,h), (0,h) in (u,v).
//! UVs are derived in the shader from the local position, so merged quads
//! tile their texture. All quads share one global index buffer.
//!
//! Shaped blocks (stairs, fences, doors) are drawn with *detail* quads,
//! flagged by bit 31 of word 1, that cover part of one cell in 1/16 steps.
//! Word 0 then holds the cell (not a corner) and, in place of the size,
//! the face's lower u and v bounds (bits 21-25 and 26-30); word 1 holds the
//! upper u and v bounds (bits 16-20 and 21-25) and the plane's offset along
//! the face axis (bits 26-30), all in 0..=16. Detail cells are 0..31,
//! so their three unused coordinate bits in word 0 (5, 11, 17) store the
//! layer's high 3 bits instead. All passes address 2048 texture layers.
//!
//! Faces 6 and 7 are the two diagonal planes of a cross-shaped block
//! (plants, torches) at the cell `(x, y, z)`: face 6 runs from (0, 0) to
//! (1, 1) in (x, z), face 7 from (1, 0) to (0, 1). They are always unit
//! sized, lit by the cell itself, and live in their own double-sided pass.

use std::sync::Arc;

use crate::world::block::{Block, Facing, RenderKind, tex};
use crate::world::chunk::{CHUNK_SIZE, CHUNK_SIZE_I, ChunkData, WORLD_HEIGHT};
use crate::world::shape::{self, Boxes};

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
    /// Foliage colour group of each of the chunk's columns, indexed
    /// `x + z * CHUNK_SIZE` (see `block::tex::tinted`).
    pub foliage: Box<[u8; CHUNK_SIZE * CHUNK_SIZE]>,
}

/// Render passes, in the order their quads are stored.
pub const PASSES: usize = 4;
pub const OPAQUE: usize = 0;
pub const CUTOUT: usize = 1;
pub const TRANSLUCENT: usize = 2;
/// Cross-shaped blocks: all quads in face group 0, drawn without culling.
pub const CROSS: usize = 3;

/// Per pass, the face directions (0 +X, 1 -X, 2 +Y, 3 -Y, 4 +Z, 5 -Z) in
/// the order their quad groups are stored. A camera sees at most one face
/// of each axis per chunk (both when it's level with the chunk on that
/// axis); +X +Y +Z -X -Y -Z puts every pair of faces from different axes
/// next to each other, so the visible groups form fewer contiguous draws
/// (1.75 per pass on average instead of 2.5). Translucent quads keep the
/// plain order: their blending depends on draw order, and there are few.
/// Cross quads face no particular direction and sit entirely in group 0.
pub const FACE_ORDER: [[usize; 6]; PASSES] =
    [[0, 2, 4, 1, 3, 5], [0, 2, 4, 1, 3, 5], [0, 1, 2, 3, 4, 5], [0, 1, 2, 3, 4, 5]];

#[derive(Default, Debug)]
pub struct MeshData {
    /// Opaque quads, then cutout, then translucent; within each pass,
    /// grouped by face direction in [`FACE_ORDER`] so the renderer can skip
    /// directions that face away from the camera.
    pub quads: Vec<[u32; 3]>,
    /// Quad count per pass and face group (in [`FACE_ORDER`]).
    pub face_quads: [[u32; 6]; PASSES],
    /// Packed reference light for checking simulation/mesh agreement.
    #[cfg(test)]
    pub(crate) block_light: Option<crate::world::lighting::BlockLight>,
}

impl MeshData {
    pub fn is_empty(&self) -> bool {
        self.quads.is_empty()
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
        let spans = [
            (0usize, CHUNK_SIZE - MARGIN, 0usize, MARGIN),
            (1, 0, MARGIN, CHUNK_SIZE),
            (2, 0, MARGIN + CHUNK_SIZE, MARGIN),
        ];
        for &(oy, ly0, ry0, hy) in &spans {
            for &(oz, lz0, rz0, hz) in &spans {
                for &(ox, lx0, rx0, hx) in &spans {
                    let chunk = &n[ox + oz * 3 + oy * 9];
                    for y in 0..hy {
                        for z in 0..hz {
                            let row = ridx(rx0, ry0 + y, rz0 + z);
                            let dst = &mut self.blocks[row..row + hx];
                            match chunk.as_deref() {
                                Some(data) => {
                                    let src = crate::world::chunk::index(lx0, ly0 + y, lz0 + z);
                                    data.copy_row(src, dst);
                                }
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

    /// The centre chunk's block light, if any of it is lit.
    #[cfg(test)]
    fn centre_block_light(&self) -> Option<crate::world::lighting::BlockLight> {
        let mut out = Box::new([0u8; CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE / 2]);
        let mut any = 0;
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let row = ridx(MARGIN, y + MARGIN, z + MARGIN);
                let src = &self.block_light[row..row + CHUNK_SIZE];
                let dst = (z * CHUNK_SIZE + y * CHUNK_SIZE * CHUNK_SIZE) / 2;
                for (o, pair) in out[dst..dst + CHUNK_SIZE / 2].iter_mut().zip(src.as_chunks::<2>().0) {
                    *o = pair[0] | pair[1] << 4;
                    any |= *o;
                }
            }
        }
        (any != 0).then(|| crate::world::lighting::BlockLight::from_nibbles(out))
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
                let next = level.saturating_sub(op.max(1));
                if next > light[i] {
                    light[i] = next;
                    queue.push(pack_q(nx, ny, nz));
                }
            };
            if x > 0 {
                visit(x - 1, y, z)
            }
            if x + 1 < D {
                visit(x + 1, y, z)
            }
            if y > 0 {
                visit(x, y - 1, z)
            }
            if y + 1 < D {
                visit(x, y + 1, z)
            }
            if z > 0 {
                visit(x, y, z - 1)
            }
            if z + 1 < D {
                visit(x, y, z + 1)
            }
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

        // Slabs and stairs stop light but don't fill their cell: they take the
        // brightest light beside or above them, so their own faces and the
        // faces next to them aren't drawn black.
        for y in 1..D - 1 {
            for z in 1..D - 1 {
                for x in 1..D - 1 {
                    let i = ridx(x, y, z);
                    if !self.blocks[i].borrows_light() {
                        continue;
                    }
                    let near = [i + D * D, i - 1, i + 1, i - D, i + D];
                    self.sky[i] = near.iter().map(|&n| self.sky[n]).max().unwrap_or(0);
                    self.block_light[i] = near.iter().map(|&n| self.block_light[n]).max().unwrap_or(0);
                }
            }
        }
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

/// Smooth lighting and ambient occlusion for the four corners (-u-v, +u-v,
/// +u+v, -u+v) of a face whose front cell is region index `front`: 2 bits
/// of AO (3 = none, also used when `!occlude`) and 8 bits of light per
/// corner.
#[inline(always)]
fn shade_corners(
    blocks: &[Block],
    sky: &[u8],
    blk: &[u8],
    front: isize,
    su: isize,
    sv: isize,
    occlude: bool,
) -> (u64, u64) {
    let at = |off: isize| (front + off) as usize;
    let o = |off: isize| blocks[at(off)].is_opaque();
    let (um, up, vm, vp) = (o(-su), o(su), o(-sv), o(sv));
    // Per corner: side offsets.
    let corners = [(um, vm, -su, -sv), (up, vm, su, -sv), (up, vp, su, sv), (um, vp, -su, sv)];
    let (mut ao, mut light) = (0u64, 0u64);
    for (c, &(s1, s2, du, dv)) in corners.iter().enumerate() {
        let corner = o(du + dv);
        let a = if !occlude {
            3
        } else if s1 && s2 {
            0
        } else {
            3 - (s1 as u64 + s2 as u64 + corner as u64)
        };
        // Smooth lighting: average the transparent cells touching this
        // vertex in front of the face.
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
    (ao, light)
}

/// Brightness proxy per corner for picking the triangulation diagonal:
/// whether to split along the other diagonal so AO and light gradients
/// stay symmetric (avoids the classic anisotropy artefact).
#[inline(always)]
fn flip_diagonal(ao: u32, light: u32) -> u32 {
    let bright = |c: u32| (ao >> (c * 2) & 3) * 16 + (light >> (c * 8) & 15) + (light >> (c * 8 + 4) & 15);
    (bright(0) + bright(2) < bright(1) + bright(3)) as u32
}

/// Word 1 flag of a detail quad (see the module docs).
const DETAIL: u32 = 1 << 31;

const KIND_SHIFT: u64 = 24;
const AO_SHIFT: u64 = 16;
const LIGHT_SHIFT: u64 = 32;
const PRESENT: u64 = 1 << 31;
/// Top drop of a low block (see `Block::top_drop`), in 1/16 block.
const DROP_SHIFT: u64 = 11;

/// Lights and meshes one chunk.
pub fn build(input: &MeshInput, region: &mut Region) -> MeshData {
    region.fill(&input.neighbors, input.base_y);
    region.light(&input.heights, input.base_y);
    let mesh = mesh_region(region, &input.foliage);
    #[cfg(test)]
    let mesh = MeshData { block_light: region.centre_block_light(), ..mesh };
    mesh
}

/// Greedy-meshes the centre chunk of a lit region.
fn mesh_region(r: &Region, foliage: &[u8; CHUNK_SIZE * CHUNK_SIZE]) -> MeshData {
    let blocks = &r.blocks[..];
    let (sky, blk) = (&r.sky[..], &r.block_light[..]);
    let mut out: [Vec<[u32; 3]>; PASSES] = Default::default();
    // Where each (pass, face) run of `out` starts, in face order.
    let mut face_start = [[0usize; 7]; PASSES];
    let strides = [1isize, (D * D) as isize, D as isize]; // x, y, z
    let mut mask = [0u64; CHUNK_SIZE * CHUNK_SIZE];
    // Region indices of cross-shaped blocks, and of shaped blocks with
    // their boxes, collected during the +X sweep.
    let mut cross_cells = Vec::new();
    let mut shaped_cells: Vec<(usize, Boxes)> = Vec::new();

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
                    // Fluid surfaces sit lower than a full block unless more
                    // of the same fluid is stacked on top.
                    let drop_at = |idx: isize| -> u64 {
                        let f = blocks[idx as usize];
                        if blocks[(idx + strides[1]) as usize].fluid() == f.fluid() { 0 } else { f.fluid_drop() as u64 }
                    };
                    let visible = if let Some(fluid) = b.fluid() {
                        if n.fluid() == Some(fluid) {
                            // Only a side stepping down to a lower level shows.
                            d != 1 && drop_at(ni) > drop_at(i)
                        } else {
                            !n.is_opaque()
                        }
                    } else {
                        match b.kind() {
                            // Low blocks (beds) show their lowered top under anything.
                            RenderKind::Cutout if face == 2 && b.top_drop() > 0 => true,
                            // Ice: the only translucent block that isn't a fluid.
                            RenderKind::Opaque | RenderKind::Cutout | RenderKind::Translucent => face_visible(b, n),
                            RenderKind::Cross if face == 0 => {
                                cross_cells.push(i as usize);
                                false
                            }
                            RenderKind::Shaped if face == 0 => {
                                let i = i as usize;
                                let neighbour = |f: Facing| {
                                    let o = f.offset();
                                    blocks[(i as isize + o.x as isize + o.z as isize * D as isize) as usize]
                                };
                                let below = i.checked_sub(D * D).map(|j| blocks[j]).unwrap_or(Block::AIR);
                                shaped_cells.push((i, shape::shape(b, neighbour, below)));
                                false
                            }
                            _ => false,
                        }
                    };
                    if visible {
                        let kind: u64 = match b.kind() {
                            RenderKind::Cutout => 1,
                            RenderKind::Translucent => 2,
                            _ => 0,
                        };
                        let (mut ao, light) = shade_corners(blocks, sky, blk, ni, su, sv, kind != 2);
                        if kind == 2 {
                            ao = drop_at(i); // fluids have no AO; carry the surface drop instead
                        }
                        let (x, z) = match d {
                            0 => (slice, vv),
                            1 => (vv, uu),
                            _ => (uu, slice),
                        };
                        let layer = tex::tinted(b.info().tex[face], foliage[x + z * CHUNK_SIZE]) as u64;
                        let drop = (b.top_drop() as u64) << DROP_SHIFT;
                        key = PRESENT | kind << KIND_SHIFT | ao << AO_SHIFT | layer | drop | light << LIGHT_SHIFT;
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
                    // Low blocks never merge: the shader only lowers a
                    // quad's upper edge.
                    let low = (key >> DROP_SHIFT) & 31 != 0;
                    let mut w = 1;
                    while !low && uu + w < CHUNK_SIZE && mask[vv * CHUNK_SIZE + uu + w] == key {
                        w += 1;
                    }
                    let mut h = 1;
                    while !low && vv + h < CHUNK_SIZE {
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

                    // (For water the AO bits hold the surface drop.)
                    let flip = flip_diagonal(((key >> AO_SHIFT) & 0xFF) as u32, (key >> LIGHT_SHIFT) as u32);
                    let kind = ((key >> KIND_SHIFT) & 3) as usize;
                    let layer = (key & 0x7FF) as u32;
                    // Water has no AO; its upper edge is lowered instead.
                    let (corner_ao, drop) = if kind == 2 {
                        (0xFF, ((key >> AO_SHIFT) & 31) as u32)
                    } else {
                        (((key >> AO_SHIFT) & 0xFF) as u32, ((key >> DROP_SHIFT) & 31) as u32)
                    };
                    let mut pos = [0u32; 3];
                    pos[d] = (slice + positive as usize) as u32;
                    pos[u] = uu as u32;
                    pos[v] = vv as u32;
                    out[kind].push([
                        pos[0]
                            | pos[1] << 6
                            | pos[2] << 12
                            | (face as u32) << 18
                            | (w as u32 - 1) << 21
                            | (h as u32 - 1) << 26
                            | flip << 31,
                        (layer & 255) | corner_ao << 8 | drop << 16 | (layer >> 8) << 21,
                        (key >> LIGHT_SHIFT) as u32,
                    ]);
                    uu += w;
                }
            }
        }

        // Shaped blocks: each box face pointing this way, unless it's hidden
        // by another box or lies against an opaque neighbour.
        for (i, boxes) in &shaped_cells {
            let (i, b) = (*i, blocks[*i]);
            // See-through textures need the alpha test.
            let see_through = b.is_ladder()
                || b.is_door()
                || matches!(b, Block::IRON_BARS | Block::BREWING_STAND)
                || matches!(b.shaped(), Some(crate::world::block::Shaped::Frame { .. }));
            let pass = if see_through { CUTOUT } else { OPAQUE };
            let (x, y, z) = (i % D - MARGIN, i / (D * D) - MARGIN, i / D % D - MARGIN);
            let layer = tex::tinted(b.info().tex[face], foliage[x + z * CHUNK_SIZE]) as u32;
            for (j, bx) in boxes.as_slice().iter().enumerate() {
                let depth = if positive { bx.max[d] } else { bx.min[d] };
                let r = [bx.min[u], bx.max[u], bx.min[v], bx.max[v]];
                let on_edge = depth == if positive { 16 } else { 0 };
                let front = if on_edge { i as isize + sd } else { i as isize };
                if (on_edge && blocks[front as usize].is_opaque()) || boxes.face_hidden(j, d, positive, r) {
                    continue;
                }
                let (ao, light) = shade_corners(blocks, sky, blk, front, su, sv, true);
                let (ao, light) = (ao as u32, light as u32);
                let r = r.map(u32::from);
                out[pass].push([
                    (x | y << 6 | z << 12) as u32
                        | (layer >> 8 & 1) << 5
                        | (layer >> 9 & 1) << 11
                        | (layer >> 10 & 1) << 17
                        | (face as u32) << 18
                        | r[0] << 21
                        | r[2] << 26
                        | flip_diagonal(ao, light) << 31,
                    (layer & 255) | ao << 8 | r[1] << 16 | r[3] << 21 | (depth as u32) << 26 | DETAIL,
                    light,
                ]);
            }
        }
    }

    // Cross-shaped blocks: two diagonal planes each, lit by their own cell.
    let cross = &mut out[CROSS];
    for &i in &cross_cells {
        let b = blocks[i];
        let (x, y, z) = (i % D - MARGIN, i / (D * D) - MARGIN, i / D % D - MARGIN);
        let l = (sky[i] | blk[i] << 4) as u32;
        let pos = (x | y << 6 | z << 12) as u32;
        let layer = tex::tinted(b.info().tex[0], foliage[x + z * CHUNK_SIZE]) as u32;
        for face in [6u32, 7] {
            cross.push([pos | face << 18, (layer & 255) | 0xFF << 8 | (layer >> 8) << 21, l * 0x0101_0101]);
        }
    }
    // Every cross quad belongs to face group 0.
    face_start[CROSS] = [0, cross.len(), cross.len(), cross.len(), cross.len(), cross.len(), 0];

    // Concatenate the passes, reordering face groups into FACE_ORDER.
    let mut mesh = MeshData { quads: Vec::with_capacity(out.iter().map(Vec::len).sum()), ..Default::default() };
    for pass in 0..PASSES {
        face_start[pass][6] = out[pass].len();
        for (group, &face) in FACE_ORDER[pass].iter().enumerate() {
            let run = &out[pass][face_start[pass][face]..face_start[pass][face + 1]];
            mesh.quads.extend_from_slice(run);
            mesh.face_quads[pass][group] = run.len() as u32;
        }
    }
    mesh
}

/// World Y of the highest light-blocking block in each column of a chunk,
/// indexed `x + z * 32`; `i16::MIN` where the column is clear.
pub fn chunk_heights(data: &ChunkData, base_y: i32) -> [i16; CHUNK_SIZE * CHUNK_SIZE] {
    // Select storage once per chunk, rather than dispatching on the enum for
    // every candidate cell of each heightmap column during streaming.
    fn scan(base_y: i32, at: impl Fn(usize) -> Block) -> [i16; CHUNK_SIZE * CHUNK_SIZE] {
        let mut out = [NO_HEIGHT; CHUNK_SIZE * CHUNK_SIZE];
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                if let Some(y) =
                    (0..CHUNK_SIZE).rev().find(|&y| at(crate::world::chunk::index(x, y, z)).light_opacity() > 0)
                {
                    out[x + z * CHUNK_SIZE] = (base_y + y as i32) as i16;
                }
            }
        }
        out
    }
    match data {
        ChunkData::Uniform(b) => {
            let height = if b.light_opacity() > 0 { (base_y + CHUNK_SIZE_I - 1) as i16 } else { NO_HEIGHT };
            [height; CHUNK_SIZE * CHUNK_SIZE]
        }
        ChunkData::Bytes(blocks) => scan(base_y, |i| Block(blocks[i] as u16)),
        ChunkData::Dense(blocks) => scan(base_y, |i| blocks[i]),
        ChunkData::Paletted { indices, palette, .. } => scan(base_y, |i| palette[indices[i] as usize]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One corner of a quad, decoded the way `chunk.wgsl` does it.
    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Corner {
        pos: [f32; 3],
        face: u32,
        ao: u32,
        sky: u32,
        block: u32,
    }

    /// The quad's corners in index-buffer order (triangles 0-1-2, 2-3-0).
    fn corners(q: [u32; 3]) -> [Corner; 4] {
        let face = (q[0] >> 18) & 7;
        let d = (face / 2) as usize;
        let (u, v) = ((d + 1) % 3, (d + 2) % 3);
        let (w, h) = ((q[0] >> 21 & 31) + 1, (q[0] >> 26 & 31) + 1);
        let flip = q[0] >> 31;
        let detail = q[1] >> 31 == 1;
        std::array::from_fn(|k| {
            let j = (k as u32 + flip) & 3;
            let c = if face.is_multiple_of(2) { j } else { (4 - j) & 3 };
            let light = q[2] >> (c * 8) & 0xFF;
            let coord_mask = if detail { 31 } else { 63 };
            let base = [q[0] & coord_mask, q[0] >> 6 & coord_mask, q[0] >> 12 & coord_mask].map(|x| x as f32);
            let pos = if detail {
                let sixteenths = |shift: u32, word: u32| (word >> shift & 31) as f32 / 16.0;
                let mut p = base;
                p[d] += sixteenths(26, q[1]);
                p[u] += if c == 1 || c == 2 { sixteenths(16, q[1]) } else { sixteenths(21, q[0]) };
                p[v] += if c >= 2 { sixteenths(21, q[1]) } else { sixteenths(26, q[0]) };
                p
            } else {
                let du = if c == 1 || c == 2 { w } else { 0 };
                let dv = if c >= 2 { h } else { 0 };
                let mut p = base;
                p[u] += du as f32;
                p[v] += dv as f32;
                let (y_off, y_ext) = match d {
                    0 => (du, w),
                    2 => (dv, h),
                    _ => (0, 0),
                };
                let drop = if face != 3 && y_off == y_ext { q[1] >> 16 & 31 } else { 0 };
                p[1] -= drop as f32 / 16.0;
                p
            };
            Corner { pos, face, ao: q[1] >> (8 + c * 2) & 3, sky: light & 15, block: light >> 4 }
        })
    }

    fn all_corners(m: &MeshData) -> impl Iterator<Item = Corner> + '_ {
        m.quads.iter().flat_map(|&q| corners(q))
    }

    impl MeshData {
        /// A mesh holding only one pass's quads.
        fn quads_of(&self, pass: usize) -> MeshData {
            let start: u32 = self.face_quads[..pass].iter().flatten().sum();
            let n = self.pass_quads(pass);
            let quads = self.quads[start as usize..(start + n) as usize].to_vec();
            MeshData { quads, ..Default::default() }
        }
    }

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
        let foliage = Box::new([0; CHUNK_SIZE * CHUNK_SIZE]);
        build(&MeshInput { neighbors: n, heights, base_y: 64, foliage }, &mut Region::default())
    }

    #[test]
    fn heightmaps_agree_across_storage_representations() {
        let mut blocks = ChunkData::new_dense(Block::AIR);
        blocks[crate::world::chunk::index(1, 4, 3)] = Block::STONE;
        blocks[crate::world::chunk::index(31, 31, 31)] = Block::STONE;
        let indices = blocks.iter().map(|b| b.0 as u8).collect::<Vec<_>>().into_boxed_slice().try_into().unwrap();
        let mut palette = Box::new([Block::AIR; 256]);
        palette[1] = Block::STONE;
        let paletted = ChunkData::Paletted { indices, palette, len: 2 };
        let compact = ChunkData::from_dense(blocks.clone());
        let direct = ChunkData::Dense(blocks);
        let heights = chunk_heights(&direct, 64);
        assert_eq!(heights[1 + 3 * CHUNK_SIZE], 68);
        assert_eq!(heights[31 + 31 * CHUNK_SIZE], 95);
        assert_eq!(heights[0], NO_HEIGHT);
        assert_eq!(chunk_heights(&compact, 64), heights);
        assert_eq!(chunk_heights(&paletted, 64), heights);
    }

    #[test]
    fn high_layers_render_cube_detail_and_cross_without_corrupting_geometry() {
        // Test-only registry entries mirror the render paths at the registry/layer limits.
        for (high, low, layer, pass) in [
            (Block(4095), Block::STONE, 2047, OPAQUE),
            (Block(4094), Block::STONE_STAIRS, 1536, OPAQUE),
            (Block(4093), Block::TALL_GRASS, 1024, CROSS),
        ] {
            let high = mesh_blocks(&[([31, 31, 31], high)]).quads_of(pass);
            let low = mesh_blocks(&[([31, 31, 31], low)]).quads_of(pass);
            assert_eq!(high.quads.len(), low.quads.len());
            for (high, low) in high.quads.iter().zip(&low.quads) {
                let detail = high[1] & DETAIL != 0;
                let high_bits = if detail {
                    (high[0] >> 5 & 1) | (high[0] >> 11 & 1) << 1 | (high[0] >> 17 & 1) << 2
                } else {
                    high[1] >> 21 & 7
                };
                assert_eq!((high[1] & 255) | high_bits << 8, layer);
                let coord_extras = if detail { (1 << 5) | (1 << 11) | (1 << 17) } else { 0 };
                assert_eq!(high[0] & !coord_extras, low[0] & !coord_extras);
                let layer_bits = 255 | if detail { 0 } else { 7 << 21 };
                assert_eq!(high[1] & !layer_bits, low[1] & !layer_bits);
                assert_eq!(high[2], low[2]);
                if pass != CROSS {
                    assert_eq!(corners(*high), corners(*low));
                }
            }
        }
    }

    #[test]
    fn single_block_has_six_faces() {
        let m = mesh_blocks(&[([5, 5, 5], Block::STONE)]);
        assert_eq!(m.pass_quads(OPAQUE), 6);
        assert_eq!(m.quads.len(), 6);
        assert_eq!(m.face_quads[OPAQUE], [1; 6]);
        // Every face is a unit square on the block's surface.
        for q in &m.quads {
            for c in corners(*q) {
                assert!(c.pos.iter().all(|&x| x == 5.0 || x == 6.0), "{c:?}");
            }
        }
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
        assert_eq!(m.face_quads, [[2; 6], [1; 6], [1; 6], [0; 6]]);
        let faces: Vec<u32> = m.quads.iter().map(|q| (q[0] >> 18) & 7).collect();
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
        let quads_on = |face: u32| m.quads.iter().filter(|q| (q[0] >> 18) & 7 == face).count();
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
        assert!(all_corners(&m).any(|c| c.ao < 3));
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
        let sky_levels: Vec<u32> = all_corners(&m).map(|c| c.sky).collect();
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
        assert!(all_corners(&m).any(|c| c.block >= 12));
    }

    #[test]
    fn corners_wind_counter_clockwise_seen_from_outside() {
        // Mixed AO and light so both triangulation diagonals occur.
        let m = mesh_blocks(&[([5, 5, 5], Block::STONE), ([6, 6, 5], Block::STONE), ([5, 6, 6], Block::GLOWSTONE)]);
        let mut flips = [0; 2];
        for &q in &m.quads {
            flips[(q[0] >> 31) as usize] += 1;
            let c = corners(q);
            let normal = [[1., 0., 0.], [-1., 0., 0.], [0., 1., 0.], [0., -1., 0.], [0., 0., 1.], [0., 0., -1.]]
                [c[0].face as usize];
            let v = |i: usize| glam::Vec3::from_array(c[i].pos);
            for (a, b, t) in [(0, 1, 2), (2, 3, 0)] {
                let n = (v(b) - v(a)).cross(v(t) - v(a));
                assert!(n.dot(glam::Vec3::from_array(normal)) > 0.0, "{q:x?}");
            }
        }
        assert!(flips[0] > 0 && flips[1] > 0, "{flips:?}");
    }

    #[test]
    fn plants_are_two_crossed_quads_that_hide_nothing() {
        let m = mesh_blocks(&[([5, 5, 5], Block::STONE), ([6, 5, 5], Block::TALL_GRASS), ([9, 9, 9], Block::TORCH)]);
        // The stone keeps all six faces, including the one behind the grass.
        assert_eq!(m.pass_quads(OPAQUE), 6);
        assert_eq!(m.pass_quads(CROSS), 4);
        assert_eq!(m.face_quads[CROSS], [4, 0, 0, 0, 0, 0]);
        let cross = &m.quads[m.quads.len() - 4..];
        let faces: Vec<u32> = cross.iter().map(|q| (q[0] >> 18) & 7).collect();
        assert_eq!(faces, [6, 7, 6, 7]);
        let grass = cross[0];
        assert_eq!([grass[0] & 63, grass[0] >> 6 & 63, grass[0] >> 12 & 63], [6, 5, 5]);
        assert_eq!(grass[1] & 0xFF, crate::world::block::tex::TALL_GRASS as u32);
        // Open sky: full skylight on every corner. The torch lights its own cell.
        assert_eq!(grass[2] & 15, 15);
        assert_eq!(cross[2][2] >> 4 & 15, 14);
    }

    #[test]
    fn fire_lights_its_cell_and_neighbours_with_identical_meshes_at_every_age() {
        let mesh = |age| mesh_blocks(&[([8, 8, 8], Block::STONE), ([8, 9, 8], Block::fire(age))]);
        let young = mesh(0);
        assert_eq!(young.pass_quads(CROSS), 2);
        assert_eq!(young.quads, mesh(15).quads);
        let fire = young.quads.last().unwrap();
        assert_eq!((fire[2] >> 4) & 15, 15);
        assert!(young.quads.iter().any(|q| q[1] & 255 != tex::FIRE_0 as u32 && (q[2] >> 4) & 15 >= 14));
    }

    #[test]
    fn torch_lights_a_sealed_room() {
        let mut blocks = Vec::new();
        for x in 8..15 {
            for y in 8..15 {
                for z in 8..15 {
                    if x == 8 || x == 14 || y == 8 || y == 14 || z == 8 || z == 14 {
                        blocks.push(([x, y, z], Block::STONE));
                    }
                }
            }
        }
        blocks.push(([11, 9, 11], Block::TORCH));
        let m = mesh_blocks(&blocks);
        assert!(all_corners(&m.quads_of(OPAQUE)).any(|c| c.block >= 11));
    }

    #[test]
    fn lava_and_water_show_faces_to_each_other() {
        let m = mesh_blocks(&[([5, 5, 5], Block::LAVA), ([6, 5, 5], Block::WATER)]);
        // Each keeps the face it shares with the other fluid: no merged blob.
        assert_eq!(m.pass_quads(TRANSLUCENT), 12);
        let top =
            m.quads.iter().find(|q| (q[0] >> 18) & 7 == 2 && q[1] & 0xFF == crate::world::block::tex::LAVA as u32);
        assert_eq!(top.unwrap()[1] >> 16 & 31, Block::LAVA.fluid_drop() as u32);
    }

    #[test]
    fn beds_are_low_and_unmerged() {
        let m = mesh_blocks(&[([1, 1, 1], Block::BED_FOOT), ([2, 1, 1], Block::BED_HEAD), ([1, 2, 1], Block::STONE)]);
        let cutout = m.quads_of(CUTOUT);
        // Six faces per half: the top shows under the stone, and the
        // faces between the halves stay.
        assert_eq!(cutout.quads.len(), 12);
        assert!(cutout.quads.iter().all(|q| (q[1] >> 16) & 31 == 7));
    }

    #[test]
    fn water_lowers_only_the_upper_edge() {
        // A single water block with a partial level: tops and the upper edge
        // of the sides are lowered, the bottom face is not.
        let m = mesh_blocks(&[([5, 5, 5], Block::flowing_water(3))]);
        assert_eq!(m.pass_quads(TRANSLUCENT), 6);
        let drop = Block::flowing_water(3).fluid_drop() as f32 / 16.0;
        assert!(drop > 0.0);
        for &q in &m.quads {
            for c in corners(q) {
                assert_eq!(c.ao, 3);
                let expected = match c.face {
                    2 => &[6.0 - drop][..],
                    3 => &[5.0][..],
                    _ => &[5.0, 6.0 - drop][..],
                };
                assert!(expected.contains(&c.pos[1]), "{c:?}");
            }
        }
    }

    #[test]
    fn stairs_are_detail_quads_inside_their_cell() {
        let stairs = Block::STONE_STAIRS.with_facing(Facing::East);
        let m = mesh_blocks(&[([5, 5, 5], stairs), ([5, 4, 5], Block::STONE)]);
        let opaque = m.quads_of(OPAQUE);
        let detail: Vec<_> = opaque.quads.iter().filter(|q| q[1] >> 31 == 1).copied().collect();
        // Slab: 6 faces but its bottom sits on stone. Tall half: 5 faces
        // (its underside is inside the slab). The stone keeps all six.
        assert_eq!(detail.len(), 5 + 5);
        assert_eq!(opaque.quads.len() - detail.len(), 6);
        for &q in &detail {
            assert_eq!(q[1] & 0xFF, tex::STONE as u32);
            for c in corners(q) {
                assert!(c.pos.iter().all(|&x| (5.0..=6.0).contains(&x)), "{c:?}");
                assert!(c.sky > 0, "lit: {c:?}");
            }
        }
        // The step is at half height, and the tall half's top covers only
        // the west (back) half.
        let tops: Vec<_> = detail.iter().map(|&q| corners(q)).filter(|c| c[0].face == 2).collect();
        assert!(tops.iter().any(|c| c[0].pos[1] == 5.5));
        let top = tops.iter().find(|c| c[0].pos[1] == 6.0).unwrap();
        assert!(top.iter().all(|c| c.pos[0] <= 5.5));
    }

    #[test]
    fn fences_join_and_ladders_are_cutout() {
        let m =
            mesh_blocks(&[([5, 5, 5], Block::OAK_FENCE), ([6, 5, 5], Block::OAK_FENCE), ([9, 5, 5], Block::LADDER)]);
        // A lone post has 6 faces; joined posts add rails between them.
        let opaque = m.pass_quads(OPAQUE);
        let lone = mesh_blocks(&[([5, 5, 5], Block::OAK_FENCE)]).pass_quads(OPAQUE);
        assert_eq!(lone, 6);
        assert!(opaque > 2 * lone, "{opaque}");
        // A ladder is a thin cutout panel.
        let ladder = m.quads_of(CUTOUT);
        assert_eq!(ladder.quads.len(), 6);
        assert!(ladder.quads.iter().all(|q| q[1] & 0xFF == tex::LADDER as u32));
    }

    #[test]
    fn slabs_block_skylight_but_stay_lit() {
        // A slab roof over a stone floor: the floor under it is darker than
        // open sky, but the slab's own top is fully lit.
        let mut blocks = Vec::new();
        for x in 4..9 {
            for z in 4..9 {
                blocks.push(([x, 4, z], Block::STONE));
                blocks.push(([x, 8, z], Block::slab_of(Block::STONE).unwrap()));
            }
        }
        let m = mesh_blocks(&blocks);
        let floor = all_corners(&m).filter(|c| c.face == 2 && c.pos == [6.0, 5.0, 6.0]).map(|c| c.sky).max().unwrap();
        assert!(floor < 15, "floor sky {floor}");
        let roof = all_corners(&m).filter(|c| c.face == 2 && c.pos[1] == 8.5).map(|c| c.sky).min().unwrap();
        assert_eq!(roof, 15);
    }

    #[test]
    fn packed_reference_block_light_matches_mesh_lighting() {
        assert!(mesh_blocks(&[([5, 5, 5], Block::STONE)]).block_light.is_none());
        let m = mesh_blocks(&[([5, 5, 5], Block::TORCH)]);
        let light = m.block_light.expect("a torch lights its chunk");
        assert_eq!(light.get(5, 5, 5), 14);
        assert_eq!(light.get(6, 5, 5), 13);
        assert_eq!(light.get(5, 8, 7), 9);
        assert_eq!(light.get(30, 30, 30), 0);
    }
}
