//! Background thread pool for terrain generation and meshing.
//!
//! The main thread keeps the job queue short and submits the most urgent
//! (nearest) chunks first, so workers need no priority queue of their own.

use std::sync::Arc;
use std::thread;

use crossbeam_channel::{Receiver, Sender, unbounded};
use glam::{IVec2, IVec3};

use crate::mesh::{self, MeshData, MeshInput, Region};
use crate::world::GeneratedChunk;
use crate::world::chunk::CHUNK_SIZE;
use crate::world::region::{ChunkRecord, RegionReader};
use crate::world::terrain::Generator;

pub enum Job {
    Generate(IVec3),
    /// v1.0 Track 3b: read a chunk's region record, then generate what it
    /// doesn't hold (Java's `ChunkMap.scheduleChunkLoad`).
    Load(IVec3, RegionReader),
    /// Biome colours of a chunk column (see `Generator::foliage`).
    Foliage(IVec2),
    Mesh {
        pos: IVec3,
        version: u32,
        input: Box<MeshInput>,
    },
}

pub enum JobResult {
    /// A generated chunk, with its region record's state when it has one.
    Generated(IVec3, Box<GeneratedChunk>, Option<Box<ChunkRecord>>),
    /// A chunk whose record holds its blocks.
    Loaded(IVec3, Box<ChunkRecord>),
    /// Colours, snow lines and original terrain heights for biome queries.
    Foliage {
        pos: IVec2,
        colours: Box<[u8; CHUNK_SIZE * CHUNK_SIZE]>,
        snow: Box<[i16; CHUNK_SIZE * CHUNK_SIZE]>,
        biome_heights: Option<Box<[i16; CHUNK_SIZE * CHUNK_SIZE]>>,
    },
    Meshed {
        pos: IVec3,
        version: u32,
        mesh: MeshData,
    },
}

pub struct Workers {
    jobs: Sender<Job>,
    results: Receiver<JobResult>,
    pub threads: usize,
}

impl Workers {
    /// Start terrain/mesh workers, leaving one available core for the main thread when possible.
    /// Each worker allocates meshing scratch space only when it receives a mesh job.
    pub fn new(generator: Arc<Generator>) -> Self {
        let (job_tx, job_rx) = unbounded::<Job>();
        let (res_tx, res_rx) = unbounded::<JobResult>();
        // Leave one core for the render/main thread.
        let threads = thread::available_parallelism().map(|n| n.get()).unwrap_or(4).saturating_sub(1).max(1);
        for i in 0..threads {
            let job_rx = job_rx.clone();
            let res_tx = res_tx.clone();
            let generator = generator.clone();
            thread::Builder::new()
                .name(format!("worker-{i}"))
                .spawn(move || {
                    let mut region = None;
                    while let Ok(job) = job_rx.recv() {
                        let result = match job {
                            Job::Generate(pos) => JobResult::Generated(
                                pos,
                                Box::new(GeneratedChunk::new(&generator, pos, generator.generate(pos))),
                                None,
                            ),
                            Job::Load(pos, reader) => match reader.read(pos) {
                                Some(record) if record.blocks.is_some() => JobResult::Loaded(pos, Box::new(record)),
                                record => JobResult::Generated(
                                    pos,
                                    Box::new(GeneratedChunk::new(&generator, pos, generator.generate(pos))),
                                    record.map(Box::new),
                                ),
                            },
                            Job::Foliage(col) => JobResult::Foliage {
                                pos: col,
                                colours: generator.foliage(col.x, col.y),
                                snow: generator.snow_lines(col.x, col.y),
                                biome_heights: generator.biome_heights(col),
                            },
                            Job::Mesh { pos, version, mut input } => {
                                // The light snapshot is read here, off the main thread.
                                input.resolve_light();
                                JobResult::Meshed {
                                    pos,
                                    version,
                                    mesh: mesh::build(&input, region.get_or_insert_with(Region::default)),
                                }
                            }
                        };
                        if res_tx.send(result).is_err() {
                            break;
                        }
                    }
                })
                .expect("spawn worker thread");
        }
        Self { jobs: job_tx, results: res_rx, threads }
    }

    pub fn submit(&self, job: Job) {
        let _ = self.jobs.send(job);
    }

    pub fn try_recv(&self) -> Option<JobResult> {
        self.results.try_recv().ok()
    }
}
