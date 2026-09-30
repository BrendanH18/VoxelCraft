//! Background thread pool for terrain generation and meshing.
//!
//! The main thread keeps the job queue short and submits the most urgent
//! (nearest) chunks first, so workers need no priority queue of their own.

use std::sync::Arc;
use std::thread;

use crossbeam_channel::{unbounded, Receiver, Sender};
use glam::IVec3;

use crate::mesh::{mesh_neighborhood, new_padded, MeshData, Neighborhood};
use crate::world::chunk::ChunkData;
use crate::world::terrain::Generator;

pub enum Job {
    Generate(IVec3),
    Mesh { pos: IVec3, version: u32, neighbors: Box<Neighborhood> },
}

pub enum JobResult {
    Generated(IVec3, ChunkData),
    Meshed { pos: IVec3, version: u32, mesh: MeshData },
}

pub struct Workers {
    jobs: Sender<Job>,
    results: Receiver<JobResult>,
    pub threads: usize,
}

impl Workers {
    pub fn new(generator: Arc<Generator>) -> Self {
        let (job_tx, job_rx) = unbounded::<Job>();
        let (res_tx, res_rx) = unbounded::<JobResult>();
        // Leave one core for the render/main thread.
        let threads = thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .saturating_sub(1)
            .max(1);
        for i in 0..threads {
            let job_rx = job_rx.clone();
            let res_tx = res_tx.clone();
            let generator = generator.clone();
            thread::Builder::new()
                .name(format!("worker-{i}"))
                .spawn(move || {
                    let mut scratch = new_padded();
                    while let Ok(job) = job_rx.recv() {
                        let result = match job {
                            Job::Generate(pos) => JobResult::Generated(pos, generator.generate(pos)),
                            Job::Mesh { pos, version, neighbors } => JobResult::Meshed {
                                pos,
                                version,
                                mesh: mesh_neighborhood(&neighbors, &mut scratch),
                            },
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
