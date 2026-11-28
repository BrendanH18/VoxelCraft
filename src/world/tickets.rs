//! v1.0 final F11: chunk tickets, Java 26.3's `TicketStorage`, and a
//! level's empty time (`ServerLevel.emptyTime`).
//!
//! In Java every dimension is a `ServerLevel` ticked each server tick,
//! whether or not a player is in it. What keeps a level's chunks loaded
//! and ticking is its tickets: a player's own (`PLAYER_LOADING`,
//! `PLAYER_SIMULATION`), a `PORTAL` ticket where something came through a
//! portal (300 ticks, radius 3), an `ENDER_PEARL` ticket around a flying
//! pearl (40 ticks, radius 2), the dragon fight's and forced chunks'.
//! 26.3 has no spawn chunks (`TicketType` has no `START`).
//!
//! A ticket of level `l` at a chunk gives every chunk at Chebyshev
//! distance `d` (Java's `ChunkTracker` propagates to the 8 neighbours) the
//! level `l + d`; a chunk takes its lowest level. Simulation tickets make a
//! chunk entity-ticking at level 31 or less and block-ticking at 32 or
//! less (`ChunkLevel`); loading tickets load it at 33 (`FULL`) or less.
//! `addTicketWithRadius(type, pos, r)` is level `33 - r`.
//!
//! Chunks here are Java's 16-block chunks, not the 32-block storage chunks.

use glam::IVec2;
use serde_json::{Value, json};

/// `ChunkLevel.byStatus(FullChunkStatus.FULL)`.
pub const FULL: i32 = 33;
/// `ChunkLevel.byStatus(FullChunkStatus.BLOCK_TICKING)`.
pub const BLOCK_TICKING: i32 = 32;
/// `ChunkLevel.byStatus(FullChunkStatus.ENTITY_TICKING)`.
pub const ENTITY_TICKING: i32 = 31;
/// `ServerLevel.tick`: entities and block entities stop once `emptyTime`
/// reaches this.
pub const EMPTY_TIME: u32 = 300;
/// At most this many tickets per level; later ones are dropped (each is a
/// few bytes, but a runaway source must not grow without bound).
pub const MAX_TICKETS: usize = 4096;

/// The ticket types a level nobody is in can hold here (`TicketType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TicketKind {
    /// `PORTAL`: 300 ticks, `FLAG_PERSIST | LOADING | SIMULATION |
    /// KEEP_DIMENSION_ACTIVE` (15). Placed with radius 3 where something
    /// arrives through a portal (`TeleportTransition.PLACE_PORTAL_TICKET`,
    /// `Entity.placePortalTicket`).
    Portal,
    /// `ENDER_PEARL`: 40 ticks, `LOADING | SIMULATION |
    /// KEEP_DIMENSION_ACTIVE` (14), radius 2, around a flying pearl
    /// (`ServerPlayer.placeEnderPearlTicket`).
    EnderPearl,
    /// Java loads a chunk it needs at once (`ServerChunkCache.getChunk`,
    /// with a one-tick `UNKNOWN` ticket: `LOADING |
    /// CAN_EXPIRE_IF_UNLOADED`, 18). Chunks here load on worker threads,
    /// so work that needs ground (an arrival's portal search, an owed
    /// portal) holds this loading-only ticket until it is done, and is
    /// removed by it. It never simulates or keeps the level active.
    Load,
}

impl TicketKind {
    /// `TicketType.timeout`, 0 for none.
    pub fn timeout(self) -> i64 {
        match self {
            TicketKind::Portal => 300,
            TicketKind::EnderPearl => 40,
            TicketKind::Load => 0,
        }
    }

    fn persist(self) -> bool {
        self == TicketKind::Portal
    }

    fn loads(self) -> bool {
        true
    }

    fn simulates(self) -> bool {
        self != TicketKind::Load
    }

    fn keeps_active(self) -> bool {
        self != TicketKind::Load
    }

    fn name(self) -> &'static str {
        match self {
            TicketKind::Portal => "portal",
            TicketKind::EnderPearl => "ender_pearl",
            TicketKind::Load => "load",
        }
    }
}

/// A ticket at a Java chunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket {
    pub chunk: IVec2,
    pub kind: TicketKind,
    pub level: i32,
    /// `Ticket.ticksLeft`: counts down from the timeout while the chunk is
    /// loaded; the ticket goes once it is below 0 (`isTimedOut`).
    pub ticks_left: i64,
}

/// A level's tickets (`TicketStorage`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tickets {
    list: Vec<Ticket>,
}

/// The Java chunk holding block `x`, `z`.
pub fn chunk_of(x: i32, z: i32) -> IVec2 {
    IVec2::new(x >> 4, z >> 4)
}

/// Chebyshev distance between two chunks, as `ChunkTracker` propagates.
fn distance(a: IVec2, b: IVec2) -> i32 {
    (a - b).abs().max_element()
}

impl Tickets {
    /// `addTicketWithRadius`: a ticket of level `33 - radius`. One of the
    /// same kind and level already there has its time reset instead.
    pub fn add_with_radius(&mut self, kind: TicketKind, chunk: IVec2, radius: i32) {
        let level = FULL - radius;
        if let Some(t) = self.list.iter_mut().find(|t| t.chunk == chunk && t.kind == kind && t.level == level) {
            t.ticks_left = kind.timeout();
            return;
        }
        if self.list.len() < MAX_TICKETS {
            self.list.push(Ticket { chunk, kind, level, ticks_left: kind.timeout() });
        }
    }

    /// `removeTicketWithRadius`.
    pub fn remove_with_radius(&mut self, kind: TicketKind, chunk: IVec2, radius: i32) {
        let level = FULL - radius;
        if let Some(i) = self.list.iter().position(|t| t.chunk == chunk && t.kind == kind && t.level == level) {
            self.list.remove(i);
        }
    }

    /// Removes every ticket of `kind`.
    pub fn remove_kind(&mut self, kind: TicketKind) {
        self.list.retain(|t| t.kind != kind);
    }

    /// `purgeStaleTickets`: each ticket with a timeout whose chunk is
    /// `loaded` (`canTicketExpire`: its holder is ready) counts down, and
    /// those past their time go.
    pub fn purge(&mut self, loaded: impl Fn(IVec2) -> bool) {
        self.list.retain_mut(|t| {
            if t.kind.timeout() == 0 || !loaded(t.chunk) {
                return true;
            }
            t.ticks_left -= 1;
            t.ticks_left >= 0
        });
    }

    /// `TicketStorage.shouldKeepDimensionActive` (`hasActiveTickets`).
    pub fn keep_active(&self) -> bool {
        self.list.iter().any(|t| t.kind.keeps_active())
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Ticket> {
        self.list.iter()
    }

    /// The simulation tickets' sources (chunk, level), for the world's
    /// ticking checks.
    pub fn simulation(&self) -> Vec<(IVec2, i32)> {
        let mut out: Vec<_> = self.list.iter().filter(|t| t.kind.simulates()).map(|t| (t.chunk, t.level)).collect();
        out.sort_unstable_by_key(|&(c, l)| (c.x, c.y, l));
        out.dedup();
        out
    }

    /// The loading tickets' sources (chunk, level), for streaming.
    pub fn loading(&self) -> Vec<(IVec2, i32)> {
        let mut out: Vec<_> = self.list.iter().filter(|t| t.kind.loads()).map(|t| (t.chunk, t.level)).collect();
        out.sort_unstable_by_key(|&(c, l)| (c.x, c.y, l));
        out.dedup();
        out
    }

    /// The simulation level of `chunk` (34 and up: none), as the
    /// `SimulationChunkTracker` holds it.
    pub fn simulation_level(&self, chunk: IVec2) -> i32 {
        level_at(&self.simulation(), chunk)
    }

    /// The persistent tickets (`FLAG_PERSIST`) as the level saves them
    /// (`TicketStorage.packTickets`, `chunk_tickets`); None without any, so
    /// a level without them saves no key.
    pub fn save(&self) -> Option<String> {
        let list: Vec<Value> = self
            .list
            .iter()
            .filter(|t| t.kind.persist())
            .map(|t| json!({"type": t.kind.name(), "chunk": [t.chunk.x, t.chunk.y], "level": t.level, "ticks_left": t.ticks_left}))
            .collect();
        (!list.is_empty()).then(|| Value::Array(list).to_string())
    }

    /// Reads [`Tickets::save`]; damaged entries are skipped.
    pub fn load(text: &str) -> Self {
        let root: Value = serde_json::from_str(text).unwrap_or_default();
        let one = |v: &Value| {
            let kind = match v["type"].as_str()? {
                "portal" => TicketKind::Portal,
                _ => return None,
            };
            let c = v["chunk"].as_array()?;
            let chunk = IVec2::new(i32::try_from(c.first()?.as_i64()?).ok()?, i32::try_from(c.get(1)?.as_i64()?).ok()?);
            let level = i32::try_from(v["level"].as_i64()?).ok()?.clamp(0, FULL);
            let ticks_left = v["ticks_left"].as_i64().unwrap_or(0).clamp(-1, kind.timeout());
            Some(Ticket { chunk, kind, level, ticks_left })
        };
        Tickets { list: root.as_array().into_iter().flatten().filter_map(one).take(MAX_TICKETS).collect() }
    }
}

/// The lowest level the `sources` give `chunk` (34: none).
pub fn level_at(sources: &[(IVec2, i32)], chunk: IVec2) -> i32 {
    sources.iter().map(|&(c, l)| l + distance(c, chunk)).min().unwrap_or(FULL + 1)
}

/// `ServerLevel.emptyTime`: reset by an active ticket (or a player, whose
/// `PLAYER_SIMULATION` ticket keeps the level active), counted up every
/// tick; entities and block entities tick only while it is below 300.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EmptyTime(pub u32);

impl EmptyTime {
    /// One `ServerLevel.tick`: whether entities and block entities run.
    pub fn tick(&mut self, active: bool) -> bool {
        if active {
            self.0 = 0;
        }
        self.0 = self.0.saturating_add(1);
        self.0 < EMPTY_TIME
    }

    /// `resetEmptyTime`: an entity arrived from another level
    /// (`teleportCrossDimension`) or a pearl renewed its ticket.
    pub fn reset(&mut self) {
        self.0 = 0;
    }

    /// Whether entities ran on the last tick (or will on the next, for a
    /// level that has never ticked).
    pub fn running(self) -> bool {
        self.0 < EMPTY_TIME
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A portal ticket (radius 3, level 30): its chunk and the 8 around it
    /// entity-tick, the next ring block-ticks, the one after that is only
    /// loaded; it counts down only while loaded and lasts 301 loaded ticks.
    #[test]
    fn a_portal_ticket_ticks_three_by_three_chunks_for_its_time() {
        let mut t = Tickets::default();
        let at = IVec2::new(4, -2);
        t.add_with_radius(TicketKind::Portal, at, 3);
        assert_eq!(t.simulation_level(at), 30);
        assert!(t.simulation_level(at + IVec2::new(1, -1)) <= ENTITY_TICKING);
        assert_eq!(t.simulation_level(at + IVec2::new(2, 0)), BLOCK_TICKING);
        assert_eq!(t.simulation_level(at + IVec2::new(-3, 3)), FULL);
        assert!(t.keep_active());
        // Not loaded yet: no countdown.
        for _ in 0..1000 {
            t.purge(|_| false);
        }
        assert_eq!(t.len(), 1);
        for _ in 0..=300 {
            assert_eq!(t.len(), 1);
            t.purge(|_| true);
        }
        assert!(t.is_empty(), "gone once below zero");
        // Adding the same ticket again resets its time instead of doubling it.
        t.add_with_radius(TicketKind::Portal, at, 3);
        t.purge(|_| true);
        t.add_with_radius(TicketKind::Portal, at, 3);
        assert_eq!((t.len(), t.iter().next().unwrap().ticks_left), (1, 300));
    }

    /// A pearl's ticket (radius 2, level 31) entity-ticks only its own
    /// chunk; a loading ticket keeps nothing active; only portal tickets
    /// are saved, with their time.
    #[test]
    fn pearl_and_load_tickets_and_what_is_saved() {
        let mut t = Tickets::default();
        t.add_with_radius(TicketKind::EnderPearl, IVec2::ZERO, 2);
        assert_eq!(t.simulation_level(IVec2::ZERO), ENTITY_TICKING);
        assert_eq!(t.simulation_level(IVec2::X), BLOCK_TICKING);
        assert_eq!(t.save(), None, "pearl tickets aren't saved");
        let mut load = Tickets::default();
        load.add_with_radius(TicketKind::Load, IVec2::ZERO, 2);
        assert!(!load.keep_active() && load.simulation().is_empty());
        assert_eq!(level_at(&load.loading(), IVec2::new(2, 2)), FULL);
        for _ in 0..1000 {
            load.purge(|_| true);
        }
        assert_eq!(load.len(), 1, "no timeout: removed by its owner");
        t.add_with_radius(TicketKind::Portal, IVec2::new(-7, 9), 3);
        t.purge(|_| true);
        let saved = t.save().unwrap();
        let back = Tickets::load(&saved);
        assert_eq!(back.len(), 1);
        let p = back.iter().next().unwrap();
        assert_eq!((p.chunk, p.level, p.ticks_left, p.kind), (IVec2::new(-7, 9), 30, 299, TicketKind::Portal));
        assert!(Tickets::load("not json").is_empty());
    }

    /// `emptyTime`: entities run on the tick an active ticket goes and for
    /// 298 ticks after it, then stop until a ticket is active again.
    #[test]
    fn empty_time_stops_entities_three_hundred_ticks_after_the_last_ticket() {
        let mut e = EmptyTime::default();
        assert!(e.tick(true));
        let mut ran = 0;
        while e.tick(false) {
            ran += 1;
        }
        assert_eq!(ran, 298);
        assert!(!e.tick(false) && !e.running());
        assert!(e.tick(true));
    }
}
