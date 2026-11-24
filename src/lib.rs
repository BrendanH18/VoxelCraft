//! Shared engine and simulation code. Build with `--no-default-features`
//! to exclude desktop window, GPU and audio dependencies.

pub mod agent;
pub mod assets;
pub mod atmosphere;
pub mod camera;
pub mod capture;
pub mod color;
pub mod content;
pub mod control;
pub mod crafting;
pub mod creative;
pub mod enchant;
pub mod entity;
pub mod equipment;
pub mod font;
pub mod grindstone;
pub mod inventory;
pub mod item;
pub mod java_registry;
pub mod knockback;
pub mod lan;
pub mod mesh;
pub mod mining;
#[path = "audio/music.rs"]
pub mod music;
pub mod particles;
pub mod performance;
pub mod physics;
pub mod player;
pub mod potion;
pub mod rules;
pub mod session;
pub mod simulation;
pub mod smithing;
pub mod survival_items;
pub mod tick_bench;
pub mod weapons;
mod workers;
pub mod world;
