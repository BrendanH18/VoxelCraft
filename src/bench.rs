//! Headless benchmark: generates and meshes a full render-distance area,
//! single-threaded and then across all cores.

use std::sync::Arc;
use std::time::Instant;

use glam::{DVec3, IVec3};

use crate::mesh::{self, D, MARGIN, MeshInput, NO_HEIGHT, Neighborhood, Region};
use crate::world::World;
use crate::world::chunk::{CHUNK_VOLUME, ChunkData, WORLD_HEIGHT_CHUNKS};
use crate::world::terrain::Generator;

pub fn run(seed: u64, rd: i32) {
    let generator = Generator::new(seed);

    // Single-threaded throughput on a fixed column set.
    let r = 3;
    let positions: Vec<IVec3> = (-r..=r)
        .flat_map(|x| (-r..=r).flat_map(move |z| (0..WORLD_HEIGHT_CHUNKS).map(move |y| IVec3::new(x, y, z))))
        .collect();
    let t = Instant::now();
    let chunks: rustc_hash::FxHashMap<IVec3, Arc<ChunkData>> =
        positions.iter().map(|&p| (p, Arc::new(generator.generate(p)))).collect();
    let gen_time = t.elapsed();
    let dense = chunks.values().filter(|c| c.uniform().is_none()).count();
    println!(
        "generate (1 thread): {} chunks in {:.1} ms -> {:.3} ms/chunk ({} dense, {} uniform)",
        chunks.len(),
        gen_time.as_secs_f64() * 1e3,
        gen_time.as_secs_f64() * 1e3 / chunks.len() as f64,
        dense,
        chunks.len() - dense
    );

    // The Nether: caverns, biome surfaces and features, fortresses and bastions.
    let nether = Generator::for_dimension(seed, crate::world::terrain::Dimension::Nether);
    let t = Instant::now();
    let mut nether_chunks = 0;
    for &p in positions.iter().filter(|p| p.y < 4) {
        std::hint::black_box(nether.generate(p));
        nether_chunks += 1;
    }
    println!(
        "generate nether (1 thread): {nether_chunks} chunks -> {:.3} ms/chunk",
        t.elapsed().as_secs_f64() * 1e3 / nether_chunks as f64
    );

    println!(
        "block storage: {:.2} MiB ({:.0} bytes/dense chunk)",
        chunks.values().map(|c| c.heap_bytes()).sum::<usize>() as f64 / (1024.0 * 1024.0),
        chunks.values().map(|c| c.heap_bytes()).sum::<usize>() as f64 / dense.max(1) as f64
    );
    // Repeated edits to existing states, including the copy-on-write snapshot.
    let t = Instant::now();
    let mut edited = 0;
    for c in chunks.values().filter(|c| c.uniform().is_none()) {
        let mut c = Arc::clone(c);
        for i in 0..1024 {
            let data = Arc::make_mut(&mut c);
            data.set(i & 31, (i >> 5) & 31, 0, crate::world::block::Block::STONE);
            edited += 1;
        }
        std::hint::black_box(c);
    }
    println!(
        "edit (with COW): {edited} writes in {:.2} ms ({:.1} ns/write)",
        t.elapsed().as_secs_f64() * 1e3,
        t.elapsed().as_secs_f64() * 1e9 / edited as f64
    );

    // Column heightmaps, as the world would maintain them.
    let mut heights: rustc_hash::FxHashMap<(i32, i32), [i16; 1024]> = Default::default();
    for (&p, c) in &chunks {
        let h = heights.entry((p.x, p.z)).or_insert([NO_HEIGHT; 1024]);
        for (a, b) in h.iter_mut().zip(mesh::chunk_heights(c, p.y * 32)) {
            *a = (*a).max(b);
        }
    }
    let foliage: rustc_hash::FxHashMap<(i32, i32), _> =
        heights.keys().map(|&(x, z)| ((x, z), generator.foliage(x, z))).collect();
    let mut region = Region::default();
    let (mut meshed, mut quads) = (0, 0u64);
    let t = Instant::now();
    for (&p, c) in &chunks {
        if p.x.abs() == r || p.z.abs() == r || c.uniform().is_some() {
            continue;
        }
        let mut n: Neighborhood = Default::default();
        for (i, slot) in n.iter_mut().enumerate() {
            let i = i as i32;
            let o = IVec3::new(i % 3 - 1, i / 9 - 1, (i / 3) % 3 - 1);
            *slot = chunks.get(&(p + o)).cloned();
        }
        let mut hm = Box::new([NO_HEIGHT; D * D]);
        for rz in 0..D as i32 {
            for rx in 0..D as i32 {
                let (wx, wz) = (p.x * 32 - MARGIN as i32 + rx, p.z * 32 - MARGIN as i32 + rz);
                if let Some(col) = heights.get(&(wx >> 5, wz >> 5)) {
                    hm[(rx + rz * D as i32) as usize] = col[((wx & 31) + (wz & 31) * 32) as usize];
                }
            }
        }
        let foliage = foliage[&(p.x, p.z)].clone();
        let m = mesh::build(&MeshInput { neighbors: n, heights: hm, base_y: p.y * 32, foliage }, &mut region);
        quads += m.quads.len() as u64;
        meshed += 1;
    }
    let mesh_time = t.elapsed();
    println!(
        "light+mesh (1 thread): {} dense chunks in {:.1} ms -> {:.3} ms/chunk, {} quads ({:.0} bytes/chunk GPU)",
        meshed,
        mesh_time.as_secs_f64() * 1e3,
        mesh_time.as_secs_f64() * 1e3 / meshed.max(1) as f64,
        quads,
        (quads as usize * std::mem::size_of::<[u32; 3]>()) as f64 / meshed.max(1) as f64
    );

    // Full streaming pipeline on the worker pool.
    let mut world = World::new(Arc::new(generator), Default::default(), rd);
    let t = Instant::now();
    let mut uploads = 0usize;
    loop {
        world.update(DVec3::new(0.0, 80.0, 0.0));
        uploads += world.mesh_uploads.drain(..).count();
        if world.pending_jobs() == 0 && uploads > 0 {
            // One more tick to make sure nothing new was scheduled.
            world.update(DVec3::new(0.0, 80.0, 0.0));
            uploads += world.mesh_uploads.drain(..).count();
            if world.pending_jobs() == 0 {
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_micros(200));
    }
    println!(
        "stream rd={rd} on {} workers: {} chunks loaded, {} meshed in {:.2} s",
        world.worker_threads(),
        world.loaded_chunks(),
        uploads,
        t.elapsed().as_secs_f64()
    );
    storage_profiles(chunks.values().map(Arc::as_ref).filter(|c| c.uniform().is_none()));
}

/// Compare representation costs using identical generated block contents. These
/// probes are outside the generation/meshing/streaming throughput measurements.
fn storage_profiles<'a>(chunks: impl Iterator<Item = &'a ChunkData>) {
    use crate::world::block::{Block, STATE_CAPACITY};
    let dense: Vec<_> = chunks
        .map(|chunk| {
            let mut blocks = ChunkData::new_dense(Block::AIR);
            let mut i = 0;
            chunk.for_each_block(|block| {
                blocks[i] = block;
                i += 1;
            });
            blocks
        })
        .collect();
    if dense.is_empty() {
        return;
    }
    for representation in ["bytes", "palette", "direct u16"] {
        let t = Instant::now();
        let stored: Vec<_> = dense
            .iter()
            .map(|blocks| {
                let data = match representation {
                    "bytes" => ChunkData::from_dense(blocks.clone()),
                    "palette" => {
                        let mut lookup = [u16::MAX; STATE_CAPACITY];
                        let mut palette = Box::new([Block::AIR; 256]);
                        let mut indices: Box<[u8; CHUNK_VOLUME]> =
                            vec![0; CHUNK_VOLUME].into_boxed_slice().try_into().unwrap();
                        let mut len = 0;
                        for (&block, idx) in blocks.iter().zip(indices.iter_mut()) {
                            let entry = &mut lookup[block.0 as usize];
                            if *entry == u16::MAX {
                                if len == 256 {
                                    return Arc::new(ChunkData::Dense(blocks.clone()));
                                }
                                *entry = len;
                                palette[len as usize] = block;
                                len += 1;
                            }
                            *idx = *entry as u8;
                        }
                        ChunkData::Paletted { indices, palette, len }
                    }
                    _ => ChunkData::Dense(blocks.clone()),
                };
                Arc::new(data)
            })
            .collect();
        let pack = t.elapsed();
        let t = Instant::now();
        let mut scratch = ChunkData::new_dense(Block::AIR);
        for _ in 0..4 {
            for chunk in &stored {
                for row in 0..1024 {
                    chunk.copy_row(row * 32, &mut scratch[row * 32..row * 32 + 32]);
                }
                std::hint::black_box(&scratch);
            }
        }
        let copy = t.elapsed();
        let t = Instant::now();
        for chunk in &stored {
            let mut edited = chunk.clone();
            for i in 0..1024 {
                Arc::make_mut(&mut edited).set(i & 31, i >> 5, 0, Block::STONE);
            }
            std::hint::black_box(edited);
        }
        let edit = t.elapsed();
        println!(
            "storage {representation}: {:.0} B/chunk, pack {:.1} us/chunk, copy {:.1} us/chunk, COW+1024 edits {:.1} us/chunk",
            stored.iter().map(|c| c.heap_bytes()).sum::<usize>() as f64 / stored.len() as f64,
            pack.as_secs_f64() * 1e6 / stored.len() as f64,
            copy.as_secs_f64() * 1e6 / (stored.len() * 4) as f64,
            edit.as_secs_f64() * 1e6 / stored.len() as f64
        );
    }
}
