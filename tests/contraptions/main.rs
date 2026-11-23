//! Reference contraptions across unload and restart (AUT-06, the roadmap's
//! M6 exit): clocks, doors, a flying machine, sorting and smelting, farms,
//! minecart storage, an autocrafter and memory circuits, each with its Java
//! 26.3 behaviour written down in `builds.rs`.
//!
//! Every build runs in a headless flat world through the game's tick order
//! (`rig.rs`) and records a probe trace each tick. It then runs again with
//! its chunks unloaded for a while, with a save and a fresh load from disk,
//! and with a restart while pistons move (or ticks are pending). Java pauses
//! an unloaded chunk's scheduled ticks, block entities and entities, and a
//! save keeps all of them, so an interrupted deterministic trace must equal
//! the uninterrupted one tick for tick; items and counted blocks must be
//! conserved and the machine must keep running.
//!
//! Builds that need behaviour still being built elsewhere in v0.17 are
//! `#[ignore]`d with the track and the behaviour they wait for. The record
//! is `docs/verification/2026-11-23-v0.17-reference-contraptions.md`.
mod builds;
mod rig;
