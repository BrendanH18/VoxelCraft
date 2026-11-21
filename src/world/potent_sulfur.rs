//! v0.16 Track H: potent sulfur's behaviour (Java 26.3 `PotentSulfurBlock`,
//! `PotentSulfurBlockEntity` and `PotentSulfurState`). The five states keep
//! Track D's ids (`gen5_blocks::potent`, 16008–16012); this module adds what
//! they do:
//!
//! - `validBlockState` on placement and whenever the cell above or below
//!   changes: dry without a water source above; continuous over a lava
//!   source (`#causes_continuous_geyser_eruptions`); dormant over magma
//!   (`#causes_periodic_geyser_eruptions`, an erupting one keeps erupting);
//!   wet otherwise;
//! - the block entity, kept by position while its chunk is loaded and
//!   ticked only in block-ticking chunks and only when not dry (Java gives a
//!   dry block no ticker): wet and dormant blocks give Nausea (4 s, ambient)
//!   every 10 ticks to living things whose eyes are just over the water
//!   within 3 blocks of the gas source; dormant and erupting blocks count
//!   down every 20 ticks (`waitingCountdown`, saved as `countdown`), with
//!   the lengths drawn from Java's positional `XoroshiroRandomSource`
//!   (seed ^ `GEYSER_SALT`); erupting and continuous blocks lift every
//!   entity in the unobstructed column above by 0.2 a tick while it rises
//!   slower than `0.3 + 0.1 × water depth`;
//! - the client side: the gas cloud every 20 ticks, the plume every 20
//!   ticks of an eruption and its sound every 40, and bubbles through the
//!   water (`animateTick`, in `particles::ambient`).
//!
//! Sounds are original synths (`audio::sounds::Sound::Geyser`); particles
//! reuse the engine's bubble, smoke and splash kinds with sulfur colours.

use std::collections::BTreeMap;

use glam::{DVec3, IVec3};
use serde_json::{Value, json};

use super::block::Block;
use super::gen5_blocks::{self as g, Potent};
use super::loot::{Random, Xoroshiro};

/// `PotentSulfurBlock.ALLOWED_WATER_BLOCKS_ABOVE`.
pub const ALLOWED_WATER_ABOVE: i32 = 4;
/// `EFFECT_APPLICATION_FREQUENCY_TICKS`, `EFFECT_DURATION_IN_TICKS` and
/// `EFFECT_RANGE`.
pub const EFFECT_INTERVAL: u64 = 10;
pub const EFFECT_TICKS: u32 = 80;
pub const EFFECT_RANGE: f64 = 3.0;
/// `PARTICLE_FREQUENCY_TICKS`, `SOUND_FREQUENCY_TICKS`.
pub const PARTICLE_INTERVAL: u64 = 20;
pub const SOUND_INTERVAL: u64 = 40;
/// `GEYSER_BASE_LAUNCH_SPEED` and `GEYSER_LAUNCH_FORCE` (floats, blocks a tick).
pub const BASE_LAUNCH_SPEED: f32 = 0.3;
pub const LAUNCH_FORCE: f32 = 0.2;
/// `GEYSER_SALT`.
pub const GEYSER_SALT: i64 = -904_011_478;
/// The countdown ticks every 20 game ticks.
pub const COUNTDOWN_INTERVAL: u64 = 20;

/// A potent sulfur block entity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geyser {
    /// `waitingCountdown`: -1 until the first countdown tick draws it.
    pub countdown: i32,
    /// `eruptionTick`: the game time of the last eruption start (or of
    /// loading), for the plume's cadence.
    pub eruption_tick: Option<u64>,
}

impl Default for Geyser {
    fn default() -> Self {
        Geyser { countdown: -1, eruption_tick: None }
    }
}

/// Potent sulfur block entities by position.
#[derive(Default, Debug)]
pub struct State {
    pub cells: BTreeMap<[i32; 3], Geyser>,
}

/// The geyser's sounds (`SoundEvents.GEYSER_*` and `NOXIOUS_GAS`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cue {
    /// `block.potent_sulfur.geyser_eruption`.
    EruptionStart,
    /// `block.potent_sulfur.geyser_eruption_active`.
    EruptionActive,
    /// `block.potent_sulfur.geyser_continuous_eruption`.
    ContinuousStart,
    /// `block.potent_sulfur.geyser_continuous_eruption_active`.
    ContinuousActive,
    /// `block.potent_sulfur.noxious_gas` (ambient).
    NoxiousGas,
}

impl Cue {
    pub const ALL: [Cue; 5] =
        [Cue::EruptionStart, Cue::EruptionActive, Cue::ContinuousStart, Cue::ContinuousActive, Cue::NoxiousGas];

    /// Java's sound event key.
    pub fn key(self) -> &'static str {
        match self {
            Cue::EruptionStart => "block.potent_sulfur.geyser_eruption",
            Cue::EruptionActive => "block.potent_sulfur.geyser_eruption_active",
            Cue::ContinuousStart => "block.potent_sulfur.geyser_continuous_eruption",
            Cue::ContinuousActive => "block.potent_sulfur.geyser_continuous_eruption_active",
            Cue::NoxiousGas => "block.potent_sulfur.noxious_gas",
        }
    }
}

/// What potent sulfur did this tick, for the game to apply and play.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// `SERVER_NAUSEA_EFFECT_TICKER`: Nausea for living things in the box
    /// around `source` whose eyes [`World::noxious_gas_reaches`].
    Nausea { source: IVec3 },
    /// `LAUNCH_ENTITY_TICKER`: entities whose box meets `min`..`max` and
    /// rise slower than `limit` (blocks a tick) gain [`LAUNCH_FORCE`].
    Launch { min: DVec3, max: DVec3, limit: f64 },
    /// A sound at a position.
    Sound { cue: Cue, pos: DVec3 },
}

/// `isSourceOfType(WATER)`: a water source, a waterlogged block or a
/// bubble column.
pub fn water_source(b: Block) -> bool {
    b == Block::WATER || b.is_waterlogged() || super::bubble::is_column(b)
}

/// `isGeyserPassableBlock`: air and water always, otherwise blocks without
/// collision.
pub fn passable(b: Block) -> bool {
    b == Block::AIR || b.is_water() || !b.is_solid()
}

/// `isSourceIfFluid` for the block under the potent sulfur: no fluid, or a
/// source.
fn source_if_fluid(b: Block) -> bool {
    match b.fluid_level() {
        None => true,
        // Falling water and lava (31, 41) are level 0 but not sources.
        Some(0) => b.0 != 31 && b.0 != 41,
        Some(_) => false,
    }
}

/// `PotentSulfurBlock.validBlockState`: the state `current` takes with
/// `above` over it and `below` under it, and whether the countdown resets
/// (a block that wasn't a geyser becoming one).
pub fn valid_state(current: Potent, above: Block, below: Block) -> (Potent, bool) {
    if !water_source(above) {
        return (Potent::Dry, false);
    }
    // `#causes_continuous_geyser_eruptions` is lava, `#causes_periodic_*` magma.
    if below.is_lava() && source_if_fluid(below) {
        (Potent::Continuous, false)
    } else if below == Block::MAGMA {
        let geyser = matches!(current, Potent::Erupting | Potent::Dormant);
        (if current == Potent::Erupting { Potent::Erupting } else { Potent::Dormant }, !geyser)
    } else {
        (Potent::Wet, false)
    }
}

/// `findNoxiousGasSourceBlock`: the first cell over the water above
/// `origin` (at most [`ALLOWED_WATER_ABOVE`] deep) that is open, or `None`.
pub fn gas_source(origin: IVec3, get: impl Fn(IVec3) -> Option<Block>) -> Option<IVec3> {
    let max_y = origin.y + ALLOWED_WATER_ABOVE + 1;
    let mut p = origin + IVec3::Y;
    while p.y <= max_y {
        let b = get(p)?;
        let waterlogged = water_source(b);
        if !waterlogged || !b.is_water() && !passable(b) {
            return passable(b).then_some(p);
        }
        p.y += 1;
    }
    None
}

/// `getUnobstructedBlockCount`: how many cells from `start` up are
/// passable, at most six per block of water.
pub fn unobstructed(start: IVec3, water: i32, get: impl Fn(IVec3) -> Option<Block>) -> i32 {
    let height = 6 * water;
    for i in 0..height {
        if !get(start + IVec3::Y * i).is_some_and(passable) {
            return i;
        }
    }
    height
}

/// The box `LAUNCH_ENTITY_TICKER` searches: the cell above the block grown
/// up by `height - 1` (`AABB.expandTowards`).
pub fn launch_box(pos: IVec3, height: i32) -> (DVec3, DVec3) {
    let lo = (pos + IVec3::Y).as_dvec3();
    let mut hi = lo + DVec3::ONE;
    let mut lo = lo;
    let grow = f64::from(height - 1);
    if grow >= 0.0 {
        hi.y += grow;
    } else {
        lo.y += grow;
    }
    (lo, hi)
}

/// `getNearbyLivingEntities`: the gas source's cell grown by 2.5 sideways.
pub fn gas_box(source: IVec3) -> (DVec3, DVec3) {
    let lo = source.as_dvec3();
    (lo - DVec3::new(2.5, 0.0, 2.5), lo + DVec3::new(3.5, 1.0, 3.5))
}

/// `Mth.getSeed(x, y, z)`.
pub fn mth_seed(p: IVec3) -> i64 {
    let mut seed = i64::from(p.x.wrapping_mul(3_129_871)) ^ i64::from(p.z).wrapping_mul(116_129_781) ^ i64::from(p.y);
    seed = seed.wrapping_mul(seed).wrapping_mul(42_317_861).wrapping_add(seed.wrapping_mul(11));
    seed >> 16
}

/// `geyserPositional(level, pos)`: `new XoroshiroRandomSource(seed ^
/// GEYSER_SALT).forkPositional().at(pos)`.
pub fn positional(world_seed: u64, p: IVec3) -> Xoroshiro {
    let seed = world_seed as i64 ^ GEYSER_SALT;
    // `RandomSupport.upgradeSeedTo128bit`.
    let mix = |mut z: u64| {
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    };
    let lo = seed as u64 ^ 0x6a09_e667_f3bc_c909;
    let hi = lo.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut base = Xoroshiro::from_state(mix(lo), mix(hi));
    let (seed_lo, seed_hi) = (base.next_long(), base.next_long());
    Xoroshiro::from_state(mth_seed(p) as u64 ^ seed_lo, seed_hi)
}

/// `nextIntBetweenInclusive`.
fn between(r: &mut Xoroshiro, lo: u32, hi: u32) -> i32 {
    (r.bounded(hi - lo + 1) + lo) as i32
}

/// `SERVER_WAITING_COUNTDOWN_TICKER`'s draw: a dormant geyser waits
/// `10 × (water - 1) + 15..=30` countdown ticks, an erupting one
/// `water - 1 + 1..=2`.
pub fn countdown(world_seed: u64, p: IVec3, dormant: bool, water: i32) -> i32 {
    let mut r = positional(world_seed, p);
    if dormant {
        10 * (water - 1) + between(&mut r, 15, 30)
    } else {
        r.next_long();
        water - 1 + between(&mut r, 1, 2)
    }
}

/// Every potent sulfur state, for the chunk scans.
fn is_potent(b: Block) -> bool {
    g::potent_state(b).is_some()
}

impl State {
    fn save(&self) -> String {
        let cells: Vec<Value> = self
            .cells
            .iter()
            .filter(|(_, g)| g.countdown != -1)
            .map(|(p, g)| json!({"pos": p, "countdown": g.countdown}))
            .collect();
        Value::Array(cells).to_string()
    }

    fn load(&mut self, text: &str) {
        let Ok(Value::Array(cells)) = serde_json::from_str::<Value>(text) else { return };
        for c in cells {
            let Some(p) = c["pos"].as_array().filter(|a| a.len() == 3) else { continue };
            let coords: Option<Vec<i32>> = p.iter().map(|n| n.as_i64().and_then(|n| i32::try_from(n).ok())).collect();
            let Some(p) = coords else { continue };
            let countdown = c["countdown"].as_i64().and_then(|n| i32::try_from(n).ok()).unwrap_or(-1);
            self.cells.insert([p[0], p[1], p[2]], Geyser { countdown, eruption_tick: None });
        }
    }
}

impl super::World {
    /// Keeps the block entity map in step with edits: an entity appears
    /// with the block and goes with it; a state turning erupting or
    /// continuous fires `onPlace`'s block event and start sound.
    pub(super) fn track_potent_sulfur(&mut self, p: IVec3, old: Block, new: Block) {
        let key = p.to_array();
        let (was, now) = (g::potent_state(old), g::potent_state(new));
        match now {
            None => {
                if was.is_some() {
                    self.potent_sulfur.cells.remove(&key);
                }
            }
            Some(state) => {
                let entry = self.potent_sulfur.cells.entry(key).or_default();
                if was != Some(state) && matches!(state, Potent::Erupting | Potent::Continuous) {
                    // `triggerEvent`: the eruption starts now, on both sides.
                    entry.eruption_tick = Some(self.maps.game_ticks);
                    if self.remote {
                        return;
                    }
                    let cue = if state == Potent::Continuous { Cue::ContinuousStart } else { Cue::EruptionStart };
                    let pos = p.as_dvec3() + DVec3::splat(0.5);
                    self.potent_events.push(Event::Sound { cue, pos });
                    self.emit_game_event(
                        pos,
                        super::vibration::GameEvent::BlockActivate,
                        super::vibration::EventSource::NONE,
                        Some(new),
                    );
                }
            }
        }
    }

    /// `updateShape` for potent sulfur above or below a changed cell.
    pub(super) fn potent_sulfur_neighbours(&mut self, p: IVec3) {
        for q in [p + IVec3::Y, p - IVec3::Y] {
            if self.get_block(q).is_some_and(is_potent) {
                self.revalidate_potent_sulfur(q);
            }
        }
    }

    /// `validBlockState` at `p` (a potent sulfur cell), resetting the
    /// countdown of a block that becomes a geyser.
    pub(crate) fn revalidate_potent_sulfur(&mut self, p: IVec3) {
        let Some(current) = self.get_block(p).and_then(g::potent_state) else { return };
        let (Some(above), Some(below)) = (self.get_block(p + IVec3::Y), self.get_block(p - IVec3::Y)) else {
            return;
        };
        let (state, reset) = valid_state(current, above, below);
        if reset && let Some(e) = self.potent_sulfur.cells.get_mut(&p.to_array()) {
            e.countdown = -1;
        }
        if state != current {
            self.edit(p, g::potent(state), true);
        }
    }

    /// Registers the potent sulfur of a chunk being installed and settles
    /// each one's state against its neighbours (Java's template and
    /// feature placement run `updateShape`).
    pub(super) fn register_potent_sulfur(&mut self, pos: IVec3, data: &super::chunk::ChunkData) {
        use super::chunk::{CHUNK_SIZE, CHUNK_SIZE_I, ChunkData};
        let any = match data {
            ChunkData::Uniform(b) => is_potent(*b),
            ChunkData::Bytes(_) => false,
            ChunkData::Paletted { palette, len, .. } => palette[..*len as usize].iter().any(|&b| is_potent(b)),
            ChunkData::Dense(v) => v.iter().any(|&b| is_potent(b)),
        };
        if !any {
            return;
        }
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for x in 0..CHUNK_SIZE {
                    if is_potent(data.get(x, y, z)) {
                        let p = pos * CHUNK_SIZE_I + IVec3::new(x as i32, y as i32, z as i32);
                        self.potent_sulfur.cells.entry(p.to_array()).or_default();
                        if !self.remote {
                            self.potent_settle.push(p);
                        }
                    }
                }
            }
        }
    }

    pub fn potent_sulfur(&self, p: IVec3) -> Option<&Geyser> {
        self.potent_sulfur.cells.get(&p.to_array())
    }

    /// Takes what potent sulfur did since the last call.
    pub fn take_potent_sulfur_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.potent_events)
    }

    pub fn potent_sulfur_to_string(&self) -> String {
        self.potent_sulfur.save()
    }

    pub fn load_potent_sulfur(&mut self, text: &str) {
        self.potent_sulfur.load(text);
    }

    /// `canBeReachedByNoxiousGas(level, source, eye)`: the eye's cell is
    /// open, within [`EFFECT_RANGE`] of the source's centre, over a water
    /// source, with a clear line under the surface.
    pub fn noxious_gas_reaches(&self, source: IVec3, eye: DVec3) -> bool {
        let cell = eye.floor().as_ivec3();
        if !self.get_block(cell).is_some_and(passable) {
            return false;
        }
        let centre = source.as_dvec3() + DVec3::splat(0.5);
        if eye.distance_squared(centre) > EFFECT_RANGE * EFFECT_RANGE {
            return false;
        }
        let below_source = centre - DVec3::Y;
        let below_eye = eye - DVec3::Y;
        self.get_block(below_eye.floor().as_ivec3()).is_some_and(water_source)
            && crate::entity::clear_line(self, below_source, below_eye)
    }

    /// Every potent sulfur block entity in a block-ticking chunk, at game
    /// time `time` (the ticker each state selects in `getTicker`).
    pub fn tick_potent_sulfur(&mut self, time: u64) {
        for p in std::mem::take(&mut self.potent_settle) {
            if !self.remote && self.get_block(p).is_some_and(is_potent) {
                self.revalidate_potent_sulfur(p);
            }
        }
        if self.potent_sulfur.cells.is_empty() {
            return;
        }
        let keys: Vec<[i32; 3]> = self.potent_sulfur.cells.keys().copied().collect();
        for key in keys {
            let p = IVec3::from_array(key);
            let Some(state) = self.get_block(p).and_then(g::potent_state) else {
                // Gone, or in an unloaded chunk with nothing to save (it is
                // registered again when the chunk loads).
                let idle = self.potent_sulfur.cells.get(&key).is_some_and(|g| g.countdown == -1);
                if self.is_loaded(p) || idle {
                    self.potent_sulfur.cells.remove(&key);
                }
                continue;
            };
            if state == Potent::Dry || !self.remote && !self.is_block_ticking(p) {
                continue;
            }
            let entry = self.potent_sulfur.cells.entry(key).or_default();
            let eruption = *entry.eruption_tick.get_or_insert(time);
            if self.remote {
                self.potent_sulfur_client(p, state, time, eruption);
                continue;
            }
            if matches!(state, Potent::Erupting | Potent::Continuous) {
                self.launch_ticker(p);
            }
            if matches!(state, Potent::Dormant | Potent::Erupting) {
                self.countdown_ticker(p, state, time);
            }
            if matches!(state, Potent::Wet | Potent::Dormant) && time.is_multiple_of(EFFECT_INTERVAL) {
                let get = |q: IVec3| self.get_block(q);
                if let Some(source) = gas_source(p, get) {
                    self.potent_events.push(Event::Nausea { source });
                }
            }
            self.potent_sulfur_client(p, state, time, eruption);
        }
    }

    /// `LAUNCH_ENTITY_TICKER`.
    fn launch_ticker(&mut self, p: IVec3) {
        let get = |q: IVec3| self.get_block(q);
        let Some(source) = gas_source(p, get) else { return };
        let water = source.y - p.y - 1;
        let height = unobstructed(p + IVec3::Y, water, get);
        let (min, max) = launch_box(p, height);
        let limit = f64::from(BASE_LAUNCH_SPEED) + f64::from(water) * 0.1;
        self.potent_events.push(Event::Launch { min, max, limit });
    }

    /// `SERVER_WAITING_COUNTDOWN_TICKER`.
    fn countdown_ticker(&mut self, p: IVec3, state: Potent, time: u64) {
        if !time.is_multiple_of(COUNTDOWN_INTERVAL) {
            return;
        }
        let get = |q: IVec3| self.get_block(q);
        let Some(source) = gas_source(p, get) else { return };
        let seed = self.generator.seed;
        let entry = self.potent_sulfur.cells.entry(p.to_array()).or_default();
        if entry.countdown <= 0 {
            let water = source.y - p.y - 1;
            entry.countdown = countdown(seed, p, state == Potent::Dormant, water);
        }
        if entry.countdown > 0 {
            entry.countdown -= 1;
        }
        if entry.countdown == 0 {
            let next = if state == Potent::Dormant { Potent::Erupting } else { Potent::Dormant };
            self.edit(p, g::potent(next), true);
            if next == Potent::Dormant {
                self.emit_game_event(
                    p.as_dvec3() + DVec3::splat(0.5),
                    super::vibration::GameEvent::BlockDeactivate,
                    super::vibration::EventSource::NONE,
                    Some(g::potent(state)),
                );
            }
        }
    }

    /// `CLIENT_NOXIOUS_GAS_TICKER` and `CLIENT_GEYSER_PLUME_TICKER`: the gas
    /// cloud, the plume and the plume's sound.
    fn potent_sulfur_client(&mut self, p: IVec3, state: Potent, time: u64, eruption: u64) {
        use crate::particles::{Burst, Kind, Request};
        let get = |q: IVec3| self.get_block(q);
        let Some(source) = gas_source(p, get) else { return };
        match state {
            Potent::Wet | Potent::Dormant => {
                if time.is_multiple_of(PARTICLE_INTERVAL) {
                    let mut b = Burst::new(Kind::LargeSmoke, source.as_dvec3() + DVec3::splat(0.5), 4);
                    b.spread = DVec3::new(0.6, 0.2, 0.6);
                    b.color = Some(GAS_COLOUR);
                    self.particles.push(Request::Burst(b));
                }
            }
            Potent::Erupting | Potent::Continuous => {
                let since = time.wrapping_sub(eruption);
                let water = source.y - p.y - 1;
                if since.is_multiple_of(PARTICLE_INTERVAL) {
                    // `GeyserParticleOptions(GEYSER, waterBlocks)`: a column
                    // of spray from the surface, taller with deeper water.
                    let base = DVec3::new(f64::from(source.x) + 0.5, f64::from(source.y), f64::from(source.z) + 0.5);
                    let mut spray = Burst::new(Kind::Splash, base, 12 + 4 * water.max(0) as u16);
                    spray.spread = DVec3::new(0.25, 0.1, 0.25);
                    spray.velocity = DVec3::new(0.0, 0.25 + 0.12 * f64::from(water), 0.0);
                    spray.velocity_spread = DVec3::new(0.04, 0.05, 0.04);
                    self.particles.push(Request::Burst(spray));
                    let mut steam = Burst::new(Kind::LargeSmoke, base + DVec3::Y * 0.5, 3);
                    steam.spread = DVec3::new(0.3, 0.4, 0.3);
                    steam.velocity = DVec3::new(0.0, 0.08, 0.0);
                    steam.color = Some(GAS_COLOUR);
                    self.particles.push(Request::Burst(steam));
                }
                if since.is_multiple_of(SOUND_INTERVAL) {
                    let cue = if state == Potent::Continuous { Cue::ContinuousActive } else { Cue::EruptionActive };
                    self.potent_events.push(Event::Sound { cue, pos: source.as_dvec3() + DVec3::splat(0.5) });
                }
            }
            Potent::Dry => {}
        }
    }
}

/// Whether boxes `a` and `b` overlap (`AABB.intersects`).
fn overlaps(a: (DVec3, DVec3), b: (DVec3, DVec3)) -> bool {
    a.0.cmplt(b.1).all() && a.1.cmpgt(b.0).all()
}

impl crate::session::PlayerSession {
    /// What a potent sulfur [`Event`] does to this player: Nausea from the
    /// gas, or the geyser's lift (`EFFECT_PREDICATE`: alive, not a
    /// spectator).
    pub fn potent_sulfur(&mut self, world: &super::World, event: &Event) {
        if self.vitals.is_dead() || self.mode == crate::rules::GameMode::Spectator {
            return;
        }
        let aabb = self.player.collision_shape().aabb(self.player.pos);
        match *event {
            Event::Nausea { source } => {
                if overlaps(aabb, gas_box(source)) && world.noxious_gas_reaches(source, self.player.eye()) {
                    use crate::simulation::effects::{Active, Effect};
                    self.vitals.give_effect(Active::new(Effect::Nausea, 0, EFFECT_TICKS).with_flags(true, true, true));
                }
            }
            Event::Launch { min, max, limit } => {
                if overlaps(aabb, (min, max)) && self.player.vehicle.is_none() {
                    if self.player.vel.y / 20.0 > -0.5 {
                        self.vitals.cap_fall(self.player.pos.y + 1.0);
                    }
                    self.player.geyser_launch(limit);
                }
            }
            Event::Sound { .. } => {}
        }
    }
}

impl crate::entity::Entities {
    /// What a potent sulfur [`Event`] does to mobs (Nausea, the lift) and
    /// to items and experience orbs (the lift).
    pub fn potent_sulfur(&mut self, world: &super::World, event: &Event) {
        let tick = crate::simulation::TICK_SECONDS;
        let force = f64::from(LAUNCH_FORCE);
        match *event {
            Event::Nausea { source } => {
                let area = gas_box(source);
                for m in &mut self.mobs {
                    if !m.alive() || !overlaps(m.aabb(), area) {
                        continue;
                    }
                    let eye = m.pos + DVec3::Y * (m.shape().height * 0.85);
                    if world.noxious_gas_reaches(source, eye) && m.kind != crate::entity::MobKind::Wither {
                        m.effects.add(crate::simulation::effects::Effect::Nausea, 0, EFFECT_TICKS);
                    }
                }
            }
            Event::Launch { min, max, limit } => {
                // `canSimulateMovement`, not riding, not `#not_affected_by_geysers`
                // (the Ender dragon, which isn't a `Mob` here).
                for m in &mut self.mobs {
                    if m.alive() && m.riding.is_none() && overlaps(m.aabb(), (min, max)) && m.vel.y * tick < limit {
                        m.vel.y += force / tick;
                    }
                }
                for it in &mut self.items {
                    let half = DVec3::new(0.125, 0.0, 0.125);
                    let b = (it.pos - half, it.pos + half + DVec3::Y * 0.25);
                    if overlaps(b, (min, max)) && it.vel.y * tick < limit {
                        it.vel.y += force / tick;
                    }
                }
                for o in &mut self.orbs {
                    let half = DVec3::new(0.25, 0.0, 0.25);
                    let b = (o.pos - half, o.pos + half + DVec3::Y * 0.5);
                    if overlaps(b, (min, max)) && o.vel.y * tick < limit {
                        o.vel.y += force / tick;
                    }
                }
            }
            Event::Sound { .. } => {}
        }
    }
}

/// The yellow-green tint of the noxious gas.
pub const GAS_COLOUR: [f32; 4] = [0.78, 0.82, 0.32, 0.8];
/// The pale yellow of sulfur bubbles.
pub const BUBBLE_COLOUR: [f32; 4] = [0.93, 0.9, 0.45, 1.0];

#[cfg(test)]
mod tests;
