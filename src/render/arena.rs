//! Pooled GPU storage for chunk quads.
//!
//! Every chunk mesh lives in one of a few large storage buffers ("pages")
//! that the chunk vertex shader reads quad records from (vertex pulling),
//! so drawing a chunk needs no buffer rebinding: a draw's `base_vertex`
//! selects its quads. Each mesh takes a contiguous range of one page from a
//! best-fit free list. A full page grows by doubling (a GPU-side copy; the
//! ranges keep their offsets) up to the device's binding size limit, after
//! which a new page is started.

use crate::mesh::MeshData;

/// Bytes per quad record (see `mesh.rs`).
pub const QUAD_BYTES: u64 = std::mem::size_of::<[u32; 3]>() as u64;
/// Allocation granularity in quads; avoids slivers too small to reuse.
const GRANULE: u32 = 16;
/// Initial page size in quads (12 MiB).
const INITIAL_PAGE_QUADS: u32 = 1 << 20;
/// Page size cap even when the device allows larger bindings.
const MAX_PAGE_BYTES: u64 = 1 << 30;

/// Where a chunk's quads live.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Allocation {
    pub page: u32,
    /// First quad within the page.
    pub offset: u32,
    /// Reserved quads (rounded up to the granule).
    pub len: u32,
}

/// Free ranges `(start, len)` of a page, sorted by start and never adjacent.
#[derive(Debug)]
struct FreeList {
    ranges: Vec<(u32, u32)>,
}

impl FreeList {
    fn new(capacity: u32) -> Self {
        Self { ranges: vec![(0, capacity)] }
    }

    /// Best fit: the smallest free range that holds `len` quads.
    fn alloc(&mut self, len: u32) -> Option<u32> {
        let (i, &(start, free)) =
            self.ranges.iter().enumerate().filter(|(_, r)| r.1 >= len).min_by_key(|(_, r)| r.1)?;
        if free == len {
            self.ranges.remove(i);
        } else {
            self.ranges[i] = (start + len, free - len);
        }
        Some(start)
    }

    fn free(&mut self, start: u32, len: u32) {
        let i = self.ranges.partition_point(|r| r.0 < start);
        let joins_prev = i > 0 && {
            let p = self.ranges[i - 1];
            debug_assert!(p.0 + p.1 <= start, "double free");
            p.0 + p.1 == start
        };
        let joins_next = i < self.ranges.len() && {
            debug_assert!(start + len <= self.ranges[i].0, "double free");
            start + len == self.ranges[i].0
        };
        match (joins_prev, joins_next) {
            (true, true) => {
                self.ranges[i - 1].1 += len + self.ranges[i].1;
                self.ranges.remove(i);
            }
            (true, false) => self.ranges[i - 1].1 += len,
            (false, true) => self.ranges[i] = (start, len + self.ranges[i].1),
            (false, false) => self.ranges.insert(i, (start, len)),
        }
    }
}

struct Page {
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// Capacity in quads.
    capacity: u32,
    free: FreeList,
}

pub struct QuadArena {
    layout: wgpu::BindGroupLayout,
    pages: Vec<Page>,
    max_page_quads: u32,
    used_quads: u64,
}

impl QuadArena {
    /// `limits` are the device's; pages stay within its storage binding size.
    pub fn new(device: &wgpu::Device, limits: &wgpu::Limits) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("chunk quads layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let max_bytes = limits.max_storage_buffer_binding_size.min(limits.max_buffer_size).min(MAX_PAGE_BYTES);
        Self { layout, pages: Vec::new(), max_page_quads: (max_bytes / QUAD_BYTES) as u32, used_quads: 0 }
    }

    pub fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.layout
    }

    pub fn bind_group(&self, page: u32) -> &wgpu::BindGroup {
        &self.pages[page as usize].bind_group
    }

    /// Bytes of live quad data.
    pub fn used_bytes(&self) -> u64 {
        self.used_quads * QUAD_BYTES
    }

    /// Bytes of GPU memory held by the pages.
    pub fn capacity_bytes(&self) -> u64 {
        self.pages.iter().map(|p| p.capacity as u64 * QUAD_BYTES).sum()
    }

    fn create_page(&self, device: &wgpu::Device, capacity: u32) -> (wgpu::Buffer, wgpu::BindGroup) {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("chunk quads"),
            size: capacity as u64 * QUAD_BYTES,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("chunk quads"),
            layout: &self.layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: buffer.as_entire_binding() }],
        });
        (buffer, bind_group)
    }

    /// Reserves `len` quads, growing or adding a page if needed.
    fn reserve(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, len: u32) -> Allocation {
        for (i, page) in self.pages.iter_mut().enumerate() {
            if let Some(offset) = page.free.alloc(len) {
                return Allocation { page: i as u32, offset, len };
            }
        }
        // Grow the newest page if it can still fit this range, otherwise
        // start a new one.
        if let Some(last) = self.pages.last()
            && last.capacity < self.max_page_quads
        {
            let tail_free = last.free.ranges.last().filter(|r| r.0 + r.1 == last.capacity).map_or(0, |r| r.1);
            let needed = last.capacity - tail_free + len;
            if needed <= self.max_page_quads {
                let capacity = (last.capacity * 2).max(needed).min(self.max_page_quads);
                let (buffer, bind_group) = self.create_page(device, capacity);
                let index = self.pages.len() - 1;
                let last = &mut self.pages[index];
                let mut encoder = device.create_command_encoder(&Default::default());
                encoder.copy_buffer_to_buffer(&last.buffer, 0, &buffer, 0, last.buffer.size());
                queue.submit([encoder.finish()]);
                last.free.free(last.capacity, capacity - last.capacity);
                last.capacity = capacity;
                last.buffer = buffer;
                last.bind_group = bind_group;
                log::debug!("chunk quad page {index} grown to {} MB", (capacity as u64 * QUAD_BYTES) >> 20);
                let offset = last.free.alloc(len).expect("grown page fits the range");
                return Allocation { page: index as u32, offset, len };
            }
        }
        let capacity = INITIAL_PAGE_QUADS.max(len).min(self.max_page_quads);
        assert!(len <= capacity, "chunk mesh larger than a storage binding");
        let (buffer, bind_group) = self.create_page(device, capacity);
        let mut free = FreeList::new(capacity);
        let offset = free.alloc(len).unwrap();
        self.pages.push(Page { buffer, bind_group, capacity, free });
        Allocation { page: self.pages.len() as u32 - 1, offset, len }
    }

    /// Uploads a mesh's quads.
    pub fn alloc(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, mesh: &MeshData) -> Allocation {
        let quads = mesh.quads.len() as u32;
        let a = self.reserve(device, queue, quads.next_multiple_of(GRANULE));
        queue.write_buffer(
            &self.pages[a.page as usize].buffer,
            a.offset as u64 * QUAD_BYTES,
            bytemuck::cast_slice(&mesh.quads),
        );
        self.used_quads += quads as u64;
        a
    }

    /// Releases a range; `quads` is the mesh's actual quad count.
    pub fn free(&mut self, a: Allocation, quads: u32) {
        self.pages[a.page as usize].free.free(a.offset, a.len);
        self.used_quads -= quads as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_list_best_fit_and_coalescing() {
        let mut f = FreeList::new(100);
        let a = f.alloc(10).unwrap();
        let b = f.alloc(20).unwrap();
        let c = f.alloc(30).unwrap();
        assert_eq!((a, b, c), (0, 10, 30));
        f.free(a, 10);
        // Best fit takes the 10-quad hole rather than the 40-quad tail.
        assert_eq!(f.alloc(8), Some(0));
        f.free(0, 8);
        f.free(c, 30);
        assert_eq!(f.ranges, [(0, 10), (30, 70)]);
        f.free(b, 20);
        assert_eq!(f.ranges, [(0, 100)]);
        assert_eq!(f.alloc(101), None);
        assert_eq!(f.alloc(100), Some(0));
        assert!(f.ranges.is_empty());
    }
}
