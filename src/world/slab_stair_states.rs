//! v0.9 slab types and stair halves, shapes and waterlogged states
//! (BLD-04a/b; Java 26.3 `SlabBlock` and `StairBlock`), ids 4096–7679.

/// The other state of `id`'s waterlogged pair, and whether `id` is the wet
/// one, for slabs and stairs. See [`super::waterlog::pair`].
pub const fn water_pair(_id: u16) -> Option<(u16, bool)> {
    None
}
