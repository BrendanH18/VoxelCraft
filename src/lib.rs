//! Shared engine and simulation code. Build with `--no-default-features`
//! to exclude desktop window, GPU and audio dependencies.

pub mod crafting;
pub mod entity;
pub mod inventory;
pub mod item;
pub mod mesh;
pub mod mining;
pub mod physics;
pub mod player;
pub mod simulation;
mod workers;
pub mod world;
