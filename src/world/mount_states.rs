//! v0.9 door hinges, floor/ceiling buttons and levers, wall grindstones and
//! other mounted states (BLD-04c/e; Java 26.3 `DoorBlock` and
//! `FaceAttachedHorizontalDirectionalBlock`), ids 7680–8191.

/// The other state of `id`'s waterlogged pair, and whether `id` is the wet
/// one, for this module's families. See [`super::waterlog::pair`].
pub const fn water_pair(_id: u16) -> Option<(u16, bool)> {
    None
}
