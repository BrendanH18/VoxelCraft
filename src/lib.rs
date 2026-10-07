//! Shared engine and simulation code. Build with `--no-default-features`
//! to exclude desktop window, GPU and audio dependencies.

pub mod agent;
pub mod camera;
pub mod control;
pub mod crafting;
pub mod enchant;
pub mod entity;
pub mod inventory;
pub mod item;
pub mod mesh;
pub mod mining;
#[path = "audio/music.rs"]
pub mod music;
pub mod particles;
pub mod physics;
pub mod player;
pub mod potion;
pub mod rules;
pub mod simulation;
pub mod smithing;
pub mod survival_items;
mod workers;
pub mod world;
