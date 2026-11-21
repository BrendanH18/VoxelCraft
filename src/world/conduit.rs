//! v0.16 Track F: the conduit (Java 26.3 `ConduitBlock` and
//! `ConduitBlockEntity`). See the dated verification record for sources and
//! differences.
//!
//! States 30848 (dry) and 30849 (waterlogged) from Track F's reservation
//! 30848–30975; texture layer 989 (the shell) from 989–1028. The block is the
//! 6 × 6 × 6 shell `Block.cube(6)`, strength 3, light 15, mined faster with
//! a pickaxe but dropping itself either way, and placed waterlogged in a
//! water source (`getStateForPlacement`).
//!
//! Each conduit is a block entity kept by position (saved in the
//! dimension's `conduits` property) and ticked in loaded chunks:
//!
//! - every 40 game ticks it re-reads its shape (`updateShape`): the 3 × 3 × 3
//!   around it must all be water, and the blocks of `#conduit_effect_block`
//!   on the three 5 × 5 rings around it (42 cells) make its frame; 16 or
//!   more make it active, 42 make it hunt;
//! - while active, the same pulse gives Conduit Power (260 ticks, ambient,
//!   with particles) to players in water or rain within
//!   `frame / 7 * 16` blocks (32 for 16 frame blocks up to 96 for 42), and a hunting conduit
//!   keeps or picks a hostile mob in water or rain within 8 blocks and hurts
//!   it for 4 (magic);
//! - it plays its activation and deactivation sounds when that changes, an
//!   ambient sound every 80 ticks and a short one every 60–99 ticks while
//!   active, and turns its cage (`activeRotation`) every tick.
//!
//! The world owns the shape and the timers; the game applies the powers,
//! the strikes (it holds the mobs) and the sounds from [`Event`]s.

use std::collections::BTreeMap;

use glam::IVec3;
use serde_json::{Value, json};

use super::block::{Block, Fluid, RenderKind};
use super::shape::{Box16, Boxes};

pub const FIRST: u16 = 30848;
pub const CONDUIT: Block = Block(FIRST);
pub const WATERLOGGED: Block = Block(FIRST + 1);
pub const LAST: u16 = FIRST + 1;

/// The shell's texture layer (the active cage and eye are model boxes).
pub const SHELL_TEX: u16 = 989;
pub const TEX_END: u16 = 990;
// Inside Track F's reservations: states 30848–30975, layers 989–1028.
const _: () = assert!(FIRST >= 30848 && LAST <= 30975 && SHELL_TEX >= 989 && TEX_END <= 1029);

/// `Blocks.CONDUIT`: `strength(3.0F)`.
pub const HARDNESS: f32 = 3.0;
/// `lightLevel(state -> 15)`.
pub const LIGHT: u8 = 15;

/// `ConduitBlockEntity`'s cadence and sizes.
pub const SHAPE_INTERVAL: u64 = 40;
pub const AMBIENT_INTERVAL: u64 = 80;
pub const MIN_ACTIVE_SIZE: usize = 16;
pub const MIN_KILL_SIZE: usize = 42;
pub const KILL_RANGE: f64 = 8.0;
/// `applyEffects`: Conduit Power for 260 ticks.
pub const EFFECT_TICKS: u32 = 260;
/// `updateAndAttackTarget`: 4 magic damage.
pub const ATTACK_DAMAGE: f32 = 4.0;
/// `getActiveRotation`'s degrees per tick.
pub const ROTATION_SPEED: f32 = -0.0375;

pub const fn owns(id: u16) -> bool {
    id >= FIRST && id <= LAST
}

pub fn is_conduit(b: Block) -> bool {
    owns(b.0)
}

/// The other half of the waterlogged pair, and whether `id` is the wet one.
pub const fn water_pair(id: u16) -> Option<(u16, bool)> {
    if owns(id) { Some((id ^ 1, id == LAST)) } else { None }
}

pub const fn registry(_id: u16) -> (&'static str, RenderKind, [u16; 6]) {
    ("conduit", RenderKind::Shaped, [SHELL_TEX; 6])
}

/// `Block.cube(6.0)`: the shape, outline and collision.
pub fn boxes() -> Boxes {
    Boxes::from_box(Box16 { min: [5, 5, 5], max: [11, 11, 11] })
}

/// `#conduit_effect_block`.
pub fn is_frame(b: Block) -> bool {
    use super::overworld_blocks as ob;
    matches!(b, ob::PRISMARINE | ob::PRISMARINE_BRICKS | ob::SEA_LANTERN | ob::DARK_PRISMARINE)
}

/// `Level.isWaterAt`: the cell's fluid is water (water, flowing water, and
/// blocks that hold it).
pub fn is_water(b: Block) -> bool {
    b.fluid() == Some(Fluid::Water) || b.is_waterlogged()
}

/// The 42 frame cells of `updateShape`: outside the 3 × 3 × 3 core, on one
/// of the three axis planes through the conduit, at distance 2 along one of
/// that plane's axes.
pub fn frame_offsets() -> impl Iterator<Item = IVec3> {
    (-2..=2i32).flat_map(|x| {
        (-2..=2i32).flat_map(move |y| {
            (-2..=2i32).filter_map(move |z| {
                let (ax, ay, az) = (x.abs(), y.abs(), z.abs());
                let outside = ax > 1 || ay > 1 || az > 1;
                let ring = (x == 0 && (ay == 2 || az == 2))
                    || (y == 0 && (ax == 2 || az == 2))
                    || (z == 0 && (ax == 2 || ay == 2));
                (outside && ring).then_some(IVec3::new(x, y, z))
            })
        })
    })
}

/// `applyEffects`: the power's range for a frame of `blocks`.
pub fn range(blocks: usize) -> i32 {
    blocks as i32 / 7 * 16
}

/// `BlockPos.closerThan(Vec3i, double)`: squared distance between the two
/// integer positions under `range` squared.
pub fn closer_than(a: IVec3, b: IVec3, range: f64) -> bool {
    ((a - b).as_dvec3()).length_squared() < range * range
}

/// What a conduit remembers between ticks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Conduit {
    pub tick_count: u32,
    pub active: bool,
    pub hunting: bool,
    /// The frame cells `updateShape` found, in its order.
    pub frame: Vec<IVec3>,
    /// The mob it is hurting (`destroyTarget`), by uid; not saved.
    pub target: Option<u32>,
    /// `activeRotation`, counted in ticks.
    pub rotation: f32,
    next_ambient: u64,
}

impl Conduit {
    /// `getActiveRotation(partial)`, in degrees.
    pub fn active_rotation(&self, partial: f32) -> f32 {
        (self.rotation + partial) * ROTATION_SPEED
    }
}

/// Conduits by position, kept while their chunk is unloaded.
#[derive(Default, Debug)]
pub struct State {
    pub conduits: BTreeMap<[i32; 3], Conduit>,
    /// A LAN replica's own tick count, for its conduits' cadence.
    replica_ticks: u64,
}

/// The conduit's sounds (`SoundEvents.CONDUIT_*`, played as `BLOCKS`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cue {
    Activate,
    Deactivate,
    Ambient,
    AmbientShort,
    AttackTarget,
}

impl Cue {
    pub const ALL: [Cue; 5] = [Cue::Activate, Cue::Deactivate, Cue::Ambient, Cue::AmbientShort, Cue::AttackTarget];
}

/// What conduits did this tick, for the game to apply and play.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Sound(Cue, IVec3),
    /// `applyEffects`: Conduit Power to players in water or rain whose block
    /// is closer than `range` to `pos`.
    Power {
        pos: IVec3,
        range: i32,
    },
    /// `updateAndAttackTarget`: keep or choose the target and strike it;
    /// `hunting` is false when the frame is short of 42 (the target is
    /// dropped).
    Hunt {
        pos: IVec3,
        hunting: bool,
    },
}

impl super::World {
    pub(super) fn track_conduit(&mut self, p: IVec3, old: Block, new: Block) {
        let key = p.to_array();
        if is_conduit(new) {
            if !is_conduit(old) {
                self.conduits.conduits.entry(key).or_default();
            }
        } else if is_conduit(old) {
            self.conduits.conduits.remove(&key);
        }
    }

    pub fn conduit(&self, p: IVec3) -> Option<&Conduit> {
        self.conduits.conduits.get(&p.to_array())
    }

    /// Conduits in loaded chunks, for drawing and particles.
    pub fn loaded_conduits(&self) -> impl Iterator<Item = (IVec3, &Conduit)> + '_ {
        self.conduits
            .conduits
            .iter()
            .map(|(p, c)| (IVec3::from_array(*p), c))
            .filter(|(p, _)| self.chunk_loaded(super::chunk::chunk_of(*p)))
    }

    /// Records the mob a hunting conduit now hurts (the game resolves it).
    pub fn set_conduit_target(&mut self, p: IVec3, target: Option<u32>) {
        if let Some(c) = self.conduits.conduits.get_mut(&p.to_array()) {
            c.target = target;
        }
    }

    /// Takes the conduit events since the last call.
    pub fn take_conduit_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.conduit_events)
    }

    /// `updateShape`: whether the conduit at `p` is active, filling `frame`.
    pub fn conduit_shape(&self, p: IVec3, frame: &mut Vec<IVec3>) -> bool {
        frame.clear();
        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    if !self.get_block(p + IVec3::new(x, y, z)).is_some_and(is_water) {
                        return false;
                    }
                }
            }
        }
        for o in frame_offsets() {
            if self.get_block(p + o).is_some_and(is_frame) {
                frame.push(p + o);
            }
        }
        frame.len() >= MIN_ACTIVE_SIZE
    }

    /// `ConduitBlockEntity.serverTick` (and `clientTick` on a LAN replica,
    /// which only animates) for every conduit in a loaded chunk, at game
    /// time `time`. `roll` is `level.getRandom().nextInt(40)` for the short
    /// ambient sound's next time.
    pub fn tick_conduits(&mut self, time: u64, mut roll: impl FnMut() -> u64) {
        if self.remote {
            let present: Vec<[i32; 3]> =
                self.banners().filter(|(_, b)| is_conduit(*b)).map(|(p, _)| p.to_array()).collect();
            self.conduits.conduits.retain(|k, _| present.contains(k));
            for k in present {
                self.conduits.conduits.entry(k).or_default();
            }
        }
        let keys: Vec<[i32; 3]> = self.conduits.conduits.keys().copied().collect();
        for key in keys {
            let p = IVec3::from_array(key);
            if !self.chunk_loaded(super::chunk::chunk_of(p)) || !self.get_block(p).is_some_and(is_conduit) {
                continue;
            }
            let Some(mut c) = self.conduits.conduits.remove(&key) else { continue };
            c.tick_count = c.tick_count.wrapping_add(1);
            if time.is_multiple_of(SHAPE_INTERVAL) {
                let mut frame = std::mem::take(&mut c.frame);
                let active = self.conduit_shape(p, &mut frame);
                c.frame = frame;
                if active != c.active && !self.remote {
                    self.conduit_events.push(Event::Sound(if active { Cue::Activate } else { Cue::Deactivate }, p));
                }
                c.active = active;
                c.hunting = c.frame.len() >= MIN_KILL_SIZE;
                if active && !self.remote {
                    self.conduit_events.push(Event::Power { pos: p, range: range(c.frame.len()) });
                    self.conduit_events.push(Event::Hunt { pos: p, hunting: c.hunting });
                }
            }
            if c.active {
                c.rotation += 1.0;
                if !self.remote {
                    if time.is_multiple_of(AMBIENT_INTERVAL) {
                        self.conduit_events.push(Event::Sound(Cue::Ambient, p));
                    }
                    if time > c.next_ambient {
                        c.next_ambient = time + 60 + roll();
                        self.conduit_events.push(Event::Sound(Cue::AmbientShort, p));
                    }
                }
            }
            self.conduits.conduits.insert(key, c);
        }
    }

    /// A LAN replica's conduits tick on its own clock (no powers, no sounds).
    pub fn tick_conduit_replicas(&mut self) {
        self.conduits.replica_ticks = self.conduits.replica_ticks.wrapping_add(1);
        let t = self.conduits.replica_ticks;
        self.tick_conduits(t, || 0);
    }

    /// `conduits` dimension property: each conduit's position.
    pub fn conduits_to_string(&self) -> String {
        let list: Vec<Value> = self.conduits.conduits.keys().map(|p| json!(p)).collect();
        json!({"version": 1, "conduits": list}).to_string()
    }

    pub fn load_conduits(&mut self, text: &str) {
        let Ok(v) = serde_json::from_str::<Value>(text) else { return };
        for e in v["conduits"].as_array().into_iter().flatten() {
            let Some(p) = e.as_array().filter(|a| a.len() == 3).and_then(|a| {
                let c = |i: usize| a[i].as_i64().and_then(|n| i32::try_from(n).ok());
                Some([c(0)?, c(1)?, c(2)?])
            }) else {
                continue;
            };
            self.conduits.conduits.insert(p, Conduit::default());
        }
    }
}

/// Original art: a curled cream shell banded in tan.
pub fn pixel(_layer: u16, x: usize, y: usize) -> [u8; 4] {
    let (cx, cy) = (x as f32 - 7.5, y as f32 - 7.5);
    let n = ((x * 7 + y * 13 + x * y) % 9) as u8;
    // A spiral of growth bands around the middle.
    let band = ((cx * cx + cy * cy).sqrt() + cy.atan2(cx) * 1.6).rem_euclid(3.0) < 1.0;
    let c: [u8; 3] = if band { [196, 160, 118] } else { [236, 220, 188] };
    [c[0] - n, c[1] - n, c[2] - n, 255]
}

#[cfg(test)]
mod tests;
