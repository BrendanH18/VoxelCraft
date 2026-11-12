//! GPU time per frame for benchmark runs, from timestamps written at the
//! start and end of each of the frame's render passes.
//!
//! Each frame uses one slot of a small ring. The frame's timestamps are
//! resolved and copied to that slot's readback buffer, which is mapped
//! without waiting; the result is read when the slot comes round again or
//! when the run ends. A frame whose slot is still waiting is left untimed,
//! so the timer never stalls the frame. Only benchmark runs on adapters
//! with `TIMESTAMP_QUERY` create a timer; otherwise there is none and
//! nothing is recorded.

use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

/// Frames in flight the ring can hold; readback is several frames late.
const SLOTS: usize = 8;
/// Render passes timed per frame: one per split-screen view.
pub const MAX_PASSES: u32 = 4;
/// Queries per slot: a start and an end for each pass.
const QUERIES_PER_SLOT: u32 = MAX_PASSES * 2;
/// Bytes between slots in the resolve buffer (its offsets must be aligned).
const SLOT_STRIDE: u64 = wgpu::QUERY_RESOLVE_BUFFER_ALIGNMENT;
/// A slot's readback map states.
const PENDING: u8 = 0;
const MAPPED: u8 = 1;
const FAILED: u8 = 2;

/// The ring slot frame `serial` uses.
pub fn slot_of(serial: u64) -> usize {
    (serial % SLOTS as u64) as usize
}

/// Query indices of `pass` in `slot`: (start, end).
pub fn pass_queries(slot: usize, pass: u32) -> (u32, u32) {
    let first = slot as u32 * QUERIES_PER_SLOT + pass * 2;
    (first, first + 1)
}

/// The queries `passes` passes of `slot` wrote.
pub fn slot_queries(slot: usize, passes: u32) -> Range<u32> {
    let first = slot as u32 * QUERIES_PER_SLOT;
    first..first + passes * 2
}

/// Milliseconds of GPU time in resolved (start, end) tick pairs: the sum of
/// the passes' durations. `None` if a pair is missing or runs backwards.
pub fn frame_ms(ticks: &[u64], period_ns: f32) -> Option<f32> {
    if ticks.is_empty() || !ticks.len().is_multiple_of(2) {
        return None;
    }
    let mut total = 0u64;
    for &[start, end] in ticks.as_chunks::<2>().0 {
        if start == 0 || end < start {
            return None;
        }
        total += end - start;
    }
    Some((total as f64 * f64::from(period_ns) / 1e6) as f32)
}

struct Slot {
    readback: wgpu::Buffer,
    /// Set by the map callback: [`PENDING`], [`MAPPED`] or [`FAILED`].
    map: Arc<AtomicU8>,
    /// The frame whose timestamps are in flight here, and its pass count.
    frame: Option<(u64, u32)>,
}

pub struct GpuTimer {
    pub queries: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    slots: Vec<Slot>,
    period_ns: f32,
    /// The frame being drawn, if it is timed: its serial and slot.
    current: Option<(u64, usize)>,
    /// Passes timed so far in the current frame.
    passes: u32,
    /// (frame serial, GPU milliseconds) of frames read back so far.
    results: Vec<(u64, f32)>,
    /// Frames left untimed because their slot was still waiting for readback.
    pub missed: u64,
    /// Timed frames whose timestamps were unusable (missing or backwards).
    pub invalid: u64,
}

impl GpuTimer {
    /// A timer for a device created with `Features::TIMESTAMP_QUERY`.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let queries = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("frame timestamps"),
            ty: wgpu::QueryType::Timestamp,
            count: SLOTS as u32 * QUERIES_PER_SLOT,
        });
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame timestamp resolve"),
            size: SLOTS as u64 * SLOT_STRIDE,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let slots = (0..SLOTS)
            .map(|_| Slot {
                readback: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("frame timestamp readback"),
                    size: u64::from(QUERIES_PER_SLOT) * u64::from(wgpu::QUERY_SIZE),
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                map: Arc::new(AtomicU8::new(PENDING)),
                frame: None,
            })
            .collect();
        Self {
            queries,
            resolve,
            slots,
            period_ns: queue.get_timestamp_period(),
            current: None,
            passes: 0,
            results: Vec::new(),
            missed: 0,
            invalid: 0,
        }
    }

    /// Reads every slot whose readback has arrived, freeing it.
    pub fn collect(&mut self) {
        for slot in &mut self.slots {
            let Some((serial, passes)) = slot.frame else { continue };
            let ms = match slot.map.load(Ordering::Acquire) {
                PENDING => continue,
                MAPPED => {
                    let bytes = u64::from(passes) * 2 * u64::from(wgpu::QUERY_SIZE);
                    let ms = slot.readback.get_mapped_range(..bytes).ok().and_then(|view| {
                        let ticks: Vec<u64> = view.as_chunks::<8>().0.iter().map(|b| u64::from_le_bytes(*b)).collect();
                        frame_ms(&ticks, self.period_ns)
                    });
                    slot.readback.unmap();
                    ms
                }
                _ => None,
            };
            slot.map.store(PENDING, Ordering::Release);
            slot.frame = None;
            match ms {
                Some(ms) => self.results.push((serial, ms)),
                None => self.invalid += 1,
            }
        }
    }

    /// Starts frame `serial`: it is timed if its slot is free.
    pub fn begin(&mut self, serial: u64) {
        self.collect();
        let slot = slot_of(serial);
        self.passes = 0;
        self.current = if self.slots[slot].frame.is_none() {
            Some((serial, slot))
        } else {
            self.missed += 1;
            None
        };
    }

    /// Query indices for the next render pass of a timed frame.
    pub fn next_pass(&mut self) -> Option<(u32, u32)> {
        let (_, slot) = self.current?;
        if self.passes >= MAX_PASSES {
            return None;
        }
        self.passes += 1;
        Some(pass_queries(slot, self.passes - 1))
    }

    /// Commands copying the frame's timestamps to its readback buffer, if
    /// any pass was timed. Submit them, then call [`GpuTimer::submitted`].
    pub fn resolve(&mut self, device: &wgpu::Device) -> Option<wgpu::CommandBuffer> {
        let (serial, slot) = self.current.take()?;
        if self.passes == 0 {
            return None;
        }
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("timestamps") });
        let offset = slot as u64 * SLOT_STRIDE;
        encoder.resolve_query_set(&self.queries, slot_queries(slot, self.passes), &self.resolve, offset);
        let bytes = u64::from(self.passes) * 2 * u64::from(wgpu::QUERY_SIZE);
        encoder.copy_buffer_to_buffer(&self.resolve, offset, &self.slots[slot].readback, 0, bytes);
        self.slots[slot].frame = Some((serial, self.passes));
        Some(encoder.finish())
    }

    /// Maps the readback of the frame just resolved; after its submission,
    /// since a buffer with a pending map can't be used by commands.
    pub fn submitted(&mut self, serial: u64) {
        let slot = &self.slots[slot_of(serial)];
        if slot.frame.is_some_and(|(s, _)| s == serial) {
            let map = slot.map.clone();
            let bytes = u64::from(slot.frame.map_or(0, |(_, p)| p)) * 2 * u64::from(wgpu::QUERY_SIZE);
            slot.readback.map_async(wgpu::MapMode::Read, ..bytes, move |result| {
                map.store(if result.is_ok() { MAPPED } else { FAILED }, Ordering::Release);
            });
        }
    }

    /// Takes the frames read back so far.
    pub fn take_results(&mut self) -> Vec<(u64, f32)> {
        std::mem::take(&mut self.results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_cycle_through_disjoint_slots() {
        assert_eq!((slot_of(0), slot_of(7), slot_of(8), slot_of(13)), (0, 7, 0, 5));
        let mut used = std::collections::HashSet::new();
        for slot in 0..SLOTS {
            for pass in 0..MAX_PASSES {
                let (start, end) = pass_queries(slot, pass);
                assert_eq!(end, start + 1);
                assert!(used.insert(start) && used.insert(end), "slot {slot} pass {pass}");
                assert!(slot_queries(slot, MAX_PASSES).contains(&end));
            }
        }
        assert_eq!(used.len() as u32, SLOTS as u32 * QUERIES_PER_SLOT);
        assert_eq!(slot_queries(2, 1), 16..18);
        // Each slot's results fit before the next aligned resolve offset.
        assert!(u64::from(QUERIES_PER_SLOT) * u64::from(wgpu::QUERY_SIZE) <= SLOT_STRIDE);
    }

    #[test]
    fn frame_time_sums_passes_and_rejects_bad_pairs() {
        // Apple GPUs count nanoseconds (period 1).
        assert_eq!(frame_ms(&[1_000, 1_501_000], 1.0), Some(1.5));
        assert_eq!(frame_ms(&[10, 1_000_010, 2_000_000, 2_500_000], 1.0), Some(1.5));
        // Intel Macs' period of 83.333 ns per tick.
        let ms = frame_ms(&[100, 12_100], 83.333).unwrap();
        assert!((ms - 1.0).abs() < 1e-3, "{ms}");
        assert_eq!(frame_ms(&[5_000, 4_000], 1.0), None, "end before start");
        assert_eq!(frame_ms(&[0, 0], 1.0), None, "never written");
        assert_eq!(frame_ms(&[1, 2, 3], 1.0), None);
        assert_eq!(frame_ms(&[], 1.0), None);
    }
}
