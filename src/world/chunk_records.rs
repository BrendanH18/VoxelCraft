//! What a chunk keeps on disk once it unloads (v1.0 Track 3b, owner
//! decision 2026-11-26): its blocks, block entities, scheduled fluid and
//! fire ticks and entities, in its [region](super::region) record. Java
//! 26.3 does the same in `ChunkMap.save` (blocks, block entities and
//! ticks), `EntityStorage.storeEntities` (entities) and
//! `ChunkMap.scheduleUnload`, and reads them back in `ChunkMap.readChunk`.
//!
//! A world opened with region storage ([`World::attach_regions`]) keeps in
//! memory only what its loaded chunks own. When a chunk unloads, its blocks
//! (if the player changed them), its block entity state and its ticks are
//! taken out of the world's tables and queued as its record; the entity pass
//! then stores its entities there too. When the chunk loads again, the
//! worker that would generate it reads the record first: blocks come from
//! it, or from the generator if it has none, and the rest goes back into the
//! tables. A world without region storage keeps every unloaded chunk in
//! memory, as before (tests, benchmarks and LAN replicas).
//!
//! The level file still holds the loaded chunks' state when the game saves
//! (`level.txt` keys, as before 1.0), and `region_chunks` names the chunks it
//! holds it for. Such a chunk's record keeps only its blocks: the rest of it
//! is older than the level file and is ignored when the chunk loads, until
//! the chunk unloads and its whole record is written again. A world saved
//! before region storage has no `region_chunks`, so its level file holds
//! every chunk's state until the first save moves the unloaded ones into
//! records ("owns all"). Autosaves write only the blocks that changed since
//! their record was written, and the level file is written by the I/O
//! thread after every record queued before it, so it never names a record
//! that isn't on disk.
//!
//! Each level file save has a generation (`save_generation`), and every
//! record the world writes, and the header of its region file, carries the
//! generation of the level file save it goes with: the next one, for
//! records written between saves. A region file newer than the level file
//! was written by a session that stopped before its next level file was
//! written. Its entities may have left a chunk the level file holds, so
//! the level file still has them too: opening such a world reads those
//! records, and their entities replace the level file's copies (the
//! records are the newer copies), so each is restored once.

use std::collections::{BTreeMap, BTreeSet};

use glam::IVec3;
use rustc_hash::{FxHashMap, FxHashSet};

use super::World;
use super::chunk::chunk_of;
use super::region::{ChunkRecord, Patch, Props, RegionStore, StateParts};

/// The level file key naming the chunks whose state the level file holds.
pub const REGION_KEY: &str = "region_chunks";
/// The level file key holding the loaded chunks' scheduled fluid ticks.
pub const FLUID_TICKS_KEY: &str = "fluid_ticks";
/// The level file key holding its save's generation (see the module
/// documentation).
pub const GENERATION_KEY: &str = "save_generation";

type At<'a> = &'a dyn Fn(IVec3) -> bool;
type Out = FxHashMap<IVec3, Props>;

/// A table of per-position state that belongs to the chunk holding the
/// position.
pub(super) trait Owned: Default {
    /// Moves out the entries in chunks `at` accepts.
    fn take_where(&mut self, at: At) -> Self;
    /// The entries, by chunk.
    fn by_chunk(self) -> FxHashMap<IVec3, Self>;
    /// Adds `other`'s entries (a chunk's, loading).
    fn merge(&mut self, other: Self);
}

impl<T> Owned for FxHashMap<IVec3, T> {
    fn take_where(&mut self, at: At) -> Self {
        if self.is_empty() {
            return Self::default();
        }
        self.extract_if(|p, _| at(chunk_of(*p))).collect()
    }
    fn by_chunk(self) -> FxHashMap<IVec3, Self> {
        let mut out: FxHashMap<IVec3, Self> = FxHashMap::default();
        for (p, v) in self {
            out.entry(chunk_of(p)).or_default().insert(p, v);
        }
        out
    }
    fn merge(&mut self, other: Self) {
        self.extend(other);
    }
}

impl<T> Owned for BTreeMap<[i32; 3], T> {
    fn take_where(&mut self, at: At) -> Self {
        if !self.keys().any(|p| at(chunk_of(IVec3::from_array(*p)))) {
            return Self::default();
        }
        let (taken, kept) = std::mem::take(self).into_iter().partition(|(p, _)| at(chunk_of(IVec3::from_array(*p))));
        *self = kept;
        taken
    }
    fn by_chunk(self) -> FxHashMap<IVec3, Self> {
        let mut out: FxHashMap<IVec3, Self> = FxHashMap::default();
        for (p, v) in self {
            out.entry(chunk_of(IVec3::from_array(p))).or_default().insert(p, v);
        }
        out
    }
    fn merge(&mut self, other: Self) {
        self.extend(other);
    }
}

impl Owned for BTreeSet<[i32; 3]> {
    fn take_where(&mut self, at: At) -> Self {
        if !self.iter().any(|p| at(chunk_of(IVec3::from_array(*p)))) {
            return Self::default();
        }
        let (taken, kept) = std::mem::take(self).into_iter().partition(|p| at(chunk_of(IVec3::from_array(*p))));
        *self = kept;
        taken
    }
    fn by_chunk(self) -> FxHashMap<IVec3, Self> {
        let mut out: FxHashMap<IVec3, Self> = FxHashMap::default();
        for p in self {
            out.entry(chunk_of(IVec3::from_array(p))).or_default().insert(p);
        }
        out
    }
    fn merge(&mut self, other: Self) {
        self.extend(other);
    }
}

/// Lists of scheduled cells keep their order: each chunk's entries stay in
/// the order they had, and a loading chunk's go after the others.
impl<T> Owned for Vec<(IVec3, T)> {
    fn take_where(&mut self, at: At) -> Self {
        if !self.iter().any(|(p, _)| at(chunk_of(*p))) {
            return Self::default();
        }
        let (taken, kept) = std::mem::take(self).into_iter().partition(|(p, _)| at(chunk_of(*p)));
        *self = kept;
        taken
    }
    fn by_chunk(self) -> FxHashMap<IVec3, Self> {
        let mut out: FxHashMap<IVec3, Self> = FxHashMap::default();
        for (p, v) in self {
            out.entry(chunk_of(p)).or_default().push((p, v));
        }
        out
    }
    fn merge(&mut self, other: Self) {
        self.extend(other);
    }
}

impl Owned for FxHashSet<IVec3> {
    fn take_where(&mut self, at: At) -> Self {
        if self.is_empty() {
            return Self::default();
        }
        self.extract_if(|p| at(chunk_of(*p))).collect()
    }
    fn by_chunk(self) -> FxHashMap<IVec3, Self> {
        let mut out: FxHashMap<IVec3, Self> = FxHashMap::default();
        for p in self {
            out.entry(chunk_of(p)).or_default().insert(p);
        }
        out
    }
    fn merge(&mut self, other: Self) {
        self.extend(other);
    }
}

impl Owned for Vec<IVec3> {
    fn take_where(&mut self, at: At) -> Self {
        if !self.iter().any(|p| at(chunk_of(*p))) {
            return Self::default();
        }
        let (taken, kept) = std::mem::take(self).into_iter().partition(|p| at(chunk_of(*p)));
        *self = kept;
        taken
    }
    fn by_chunk(self) -> FxHashMap<IVec3, Self> {
        let mut out: FxHashMap<IVec3, Self> = FxHashMap::default();
        for p in self {
            out.entry(chunk_of(p)).or_default().push(p);
        }
        out
    }
    fn merge(&mut self, other: Self) {
        self.extend(other);
    }
}

/// One block entity system: its level file key, and how to take a set of
/// chunks' entries out as that key's text (one text per chunk) and put a
/// chunk's text back.
struct System {
    key: &'static str,
    take: fn(&mut World, At, &mut Out),
    load: fn(&mut World, &str),
}

/// A system whose state is the listed tables. Taking a chunk's entries
/// swaps them in for the whole tables and runs the system's own level file
/// writer, so a record holds exactly the text the level file would; loading
/// runs its reader on empty tables and merges what it read. `$all` and
/// `$part` name each table's temporaries.
macro_rules! system {
    ($key:literal, $save:ident, $load:ident, [$($all:ident $part:ident: $($f:ident).+),+ $(,)?]) => {
        System {
            key: $key,
            take: |w: &mut World, at: At, out: &mut Out| {
                $(let mut $all = Owned::by_chunk(Owned::take_where(&mut w$(.$f)+, at));)+
                let mut chunks: Vec<IVec3> = Vec::new();
                $(chunks.extend($all.keys().copied());)+
                chunks.sort_unstable_by_key(|c| (c.x, c.y, c.z));
                chunks.dedup();
                for c in chunks {
                    $(
                        let mut $part = $all.remove(&c).unwrap_or_default();
                        std::mem::swap(&mut w$(.$f)+, &mut $part);
                    )+
                    let text = w.$save();
                    $(std::mem::swap(&mut w$(.$f)+, &mut $part);)+
                    out.entry(c).or_default().push(($key.to_string(), text));
                }
            },
            load: |w: &mut World, text: &str| {
                $(
                    let mut $part = Default::default();
                    std::mem::swap(&mut w$(.$f)+, &mut $part);
                )+
                w.$load(text);
                $(
                    std::mem::swap(&mut w$(.$f)+, &mut $part);
                    Owned::merge(&mut w$(.$f)+, $part);
                )+
            },
        }
    };
}

/// Every block entity system a chunk record holds, in the order they are
/// restored (the order `Game::restore_dimension` loads the level file in).
/// Redstone and hopper scheduling (`redstone`, `automation`) stay in the
/// level file: they are scheduled ticks with a world-wide clock.
fn systems() -> [System; 20] {
    [
        System { key: "falling", take: take_falling, load: load_falling },
        system!("brewing", brewing_stands_to_string, load_brewing_stands, [a b: brewing_stands]),
        system!("furnaces", furnaces_to_string, load_furnaces, [a b: furnaces]),
        system!("chests", chests_to_string, load_chests, [a b: chests, c d: chest_positions]),
        system!("archaeology", archaeology_to_string, load_archaeology, [a b: archaeology]),
        system!("jukeboxes", jukeboxes_to_string, load_jukeboxes, [a b: jukeboxes.boxes]),
        system!("beehives", beehives_to_string, load_beehives, [a b: beehives.hives, c d: beehives.released]),
        system!("creaking_hearts", creaking_hearts_to_string, load_creaking_hearts, [a b: creaking_hearts]),
        system!("sculk", sculk_to_string, load_sculk, [a b: sculk.entities]),
        system!(
            "copper_golem_statues",
            copper_golem_statues_to_string,
            load_copper_golem_statues,
            [a b: copper_golem_names]
        ),
        system!("trial", trial_to_string, load_trial, [a b: trial.spawners, c d: trial.vaults, e f: trial.generated]),
        system!("wood", wood_to_string, load_wood, [a b: wood.signs, c d: wood.shelves, e f: wood.bookshelves]),
        system!("spawners", spawners_to_string, load_spawners, [a b: spawners]),
        system!(
            "utilities",
            utilities_to_string,
            load_utilities,
            [a b: utilities.campfires, c d: utilities.decorated, e f: utilities.drips, g h: utilities.tilts]
        ),
        system!(
            "workstations",
            workstations_to_string,
            load_workstations,
            [a b: workstations.banners, c d: workstations.lecterns]
        ),
        system!("beacons", beacons_to_string, load_beacons, [a b: beacons.beacons]),
        system!("conduits", conduits_to_string, load_conduits, [a b: conduits.conduits]),
        system!("crafters", crafters_to_string, load_crafters, [a b: crafters.map, c d: crafters.active]),
        system!("potent_sulfur", potent_sulfur_to_string, load_potent_sulfur, [a b: potent_sulfur.cells]),
        // v1.0 final F9: legacy walls waiting for a neighbouring chunk.
        system!("legacy_walls", legacy_walls_to_string, load_legacy_walls, [a b: legacy_walls]),
    ]
}

/// The level file keys whose entries move into chunk records. With region
/// storage the level file holds only the loaded chunks' entries for them.
pub const CHUNK_KEYS: [&str; 20] = [
    "falling",
    "brewing",
    "furnaces",
    "chests",
    "archaeology",
    "jukeboxes",
    "beehives",
    "creaking_hearts",
    "sculk",
    "copper_golem_statues",
    "trial",
    "wood",
    "spawners",
    "utilities",
    "workstations",
    "beacons",
    "conduits",
    "crafters",
    "potent_sulfur",
    "legacy_walls",
];

/// Falling blocks stored in unloaded chunks (Java keeps them with the
/// chunk's entities), keyed by their chunk.
fn take_falling(w: &mut World, at: At, out: &mut Out) {
    if w.stored_falling.is_empty() {
        return;
    }
    let taken: Vec<_> = w.stored_falling.extract_if(|c, _| at(*c)).collect();
    for (c, blocks) in taken {
        let live = std::mem::take(&mut w.falling);
        let stored = std::mem::replace(&mut w.stored_falling, [(c, blocks)].into_iter().collect());
        let text = w.falling_to_string();
        w.falling = live;
        w.stored_falling = stored;
        out.entry(c).or_default().push(("falling".to_string(), text));
    }
}

fn load_falling(w: &mut World, text: &str) {
    let live = std::mem::take(&mut w.falling);
    let stored = std::mem::take(&mut w.stored_falling);
    w.load_falling(text);
    let read = std::mem::replace(&mut w.falling, live);
    let read_stored = std::mem::replace(&mut w.stored_falling, stored);
    w.falling.extend(read);
    for (c, blocks) in read_stored {
        w.stored_falling.entry(c).or_default().extend(blocks);
    }
}

/// Region storage bookkeeping (see the module documentation).
#[derive(Default)]
pub(super) struct Records {
    /// The level file holds every chunk's state: a world saved before
    /// region storage, until its first save.
    owns_all: bool,
    /// Chunks whose state the level file holds (it named them when it was
    /// saved, or they have loaded since without their record's state).
    level_owned: FxHashSet<IVec3>,
    /// With `owns_all`: chunks whose whole record was written since.
    released: FxHashSet<IVec3>,
    /// Restored entity batches, for the entity pass.
    entities: Vec<(IVec3, Vec<Props>)>,
    /// Records queued by unloading chunks this session.
    pub(super) evicted: u64,
    /// Records are read but never written (scripted runs): unloaded chunks
    /// stay in memory as without region storage.
    read_only: bool,
    /// The generation of the last level file save queued (or loaded).
    generation: u64,
    /// Entity batches written after the level file by a session that
    /// stopped before its next one: the level file's copies of their
    /// entities are older and go (see the module documentation).
    superseding: Vec<Props>,
}

impl Records {
    pub(super) fn level_owned_len(&self) -> usize {
        self.level_owned.len()
    }

    pub(super) fn entity_batches(&self) -> usize {
        self.entities.len()
    }
}

impl World {
    /// Keeps this world's unloaded chunks in region storage from now on.
    /// `owned` is the level file's `region_chunks` (`None`: a level file
    /// from before region storage, which holds every chunk's state) and
    /// `generation` its `save_generation` (0 if it has none). Saved
    /// `chunks.bin` chunks that already have blocks in a record are older
    /// than it (a crash after the first save wrote the records) and give way.
    pub fn attach_regions(
        &mut self,
        store: RegionStore,
        owned: Option<FxHashSet<IVec3>>,
        generation: u64,
        writable: bool,
    ) {
        let stored = store.stored_at_open();
        if !self.saved.is_empty() && !stored.is_empty() {
            let newer: Vec<IVec3> = self
                .saved
                .keys()
                .copied()
                .filter(|p| stored.contains(p) && store.read(*p).is_some_and(|r| r.blocks.is_some()))
                .collect();
            for p in newer {
                self.saved.remove(&p);
            }
        }
        // Records written after the level file, outside the chunks it holds:
        // their entities may also be in it, and are the newer copies.
        let newest = store.generation();
        let mut superseding = Vec::new();
        if let Some(owned) = owned.as_ref().filter(|_| newest > generation) {
            let after = store.chunks_written_after(generation);
            log::warn!("{} chunk records were written after the level file; reading their entities", after.len());
            for pos in after.into_iter().filter(|p| !owned.contains(p)) {
                if let Some(record) = store.read(pos) {
                    superseding.extend(record.entities.into_iter().filter(|b| !b.is_empty()));
                }
            }
        }
        let mut store = store;
        // Only a level file from before region storage needs the list, at
        // its first save.
        if owned.is_some() || !writable {
            store.forget_stored_at_open();
        }
        self.region_records = Records {
            owns_all: owned.is_none(),
            level_owned: owned.unwrap_or_default(),
            read_only: !writable,
            // Past every generation on disk, so a later level file is newer
            // than all of it.
            generation: generation.max(newest),
            superseding,
            ..Default::default()
        };
        self.regions = Some(store);
    }

    /// This world's region storage, if it has any.
    pub fn regions(&self) -> Option<&RegionStore> {
        self.regions.as_ref()
    }

    /// Whether unloading chunks write their records (region storage that
    /// isn't read-only, on a world that isn't a LAN replica).
    pub fn writes_records(&self) -> bool {
        self.regions.is_some() && !self.region_records.read_only && !self.remote
    }

    /// Whether the level file holds chunk `pos`'s state, so its record's
    /// state (all but the blocks) is older and isn't used.
    pub(super) fn level_owns(&self, pos: IVec3) -> bool {
        (self.region_records.owns_all && !self.region_records.released.contains(&pos))
            || self.region_records.level_owned.contains(&pos)
    }

    /// Whether entities stored in unloaded chunk `pos` stay in memory rather
    /// than going to its record: without region storage, while the chunk is
    /// loading (its record is being read), and while the level file holds
    /// its state (they go with the next save).
    pub fn keeps_entities(&self, pos: IVec3) -> bool {
        !self.writes_records() || self.gen_in_flight.contains(&pos) || self.level_owns(pos)
    }

    /// Whether chunk `pos` is being loaded or generated.
    pub fn chunk_loading(&self, pos: IVec3) -> bool {
        self.gen_in_flight.contains(&pos)
    }

    /// Queues entity batches for unloaded chunks' records, after what each
    /// record already has (Java's `EntityStorage.storeEntities`).
    pub fn store_entities(&mut self, batches: Vec<(IVec3, Props)>) {
        if !self.writes_records() {
            return;
        }
        let generation = self.region_records.generation.saturating_add(1);
        let Some(store) = &self.regions else { return };
        store.queue(
            batches
                .into_iter()
                .map(|(pos, batch)| (pos, Patch { append: vec![batch], generation, ..Default::default() })),
        );
    }

    /// Entity batches of chunks that loaded from records since the last
    /// call, for the entity pass to bring back.
    pub fn take_entity_records(&mut self) -> Vec<(IVec3, Vec<Props>)> {
        std::mem::take(&mut self.region_records.entities)
    }

    /// Entity batches whose entities replace the level file's copies (see
    /// the module documentation), once, for the entity pass.
    pub fn take_superseding_entities(&mut self) -> Vec<Props> {
        std::mem::take(&mut self.region_records.superseding)
    }

    /// The `save_generation` text: the generation of the last level file
    /// save queued.
    pub fn save_generation(&self) -> u64 {
        self.region_records.generation
    }

    /// Takes the block entity, falling block, fluid and fire state of the
    /// chunks `at` accepts out of the world's tables, by chunk. Fluid cells
    /// are kept only for chunks the player changed (or `keep_fluids`
    /// accepts): generating a chunk again wakes its own.
    fn take_chunk_state(&mut self, at: At, keep_fluids: At) -> FxHashMap<IVec3, StateParts> {
        let mut texts: Out = FxHashMap::default();
        for system in systems() {
            (system.take)(self, at, &mut texts);
        }
        let mut out: FxHashMap<IVec3, StateParts> = FxHashMap::default();
        for (c, state) in texts {
            out.entry(c).or_default().state = state;
        }
        for (c, (water, lava, bubbles)) in self.take_fluid_cells(at) {
            if keep_fluids(c) {
                let parts = out.entry(c).or_default();
                parts.water = water;
                parts.lava = lava;
                parts.bubbles = bubbles;
            }
        }
        for (c, fires) in self.fire_ticks(at) {
            out.entry(c).or_default().fires = fires;
        }
        out
    }

    /// Queues the records of loaded chunks about to unload (Java's
    /// `ChunkMap.scheduleUnload`): their blocks if changed, block entities,
    /// ticks, and no entities (the entity pass stores those next).
    pub(super) fn evict_chunks(&mut self, chunks: &[IVec3]) {
        if !self.writes_records() || chunks.is_empty() {
            return;
        }
        let set: FxHashSet<IVec3> = chunks.iter().copied().collect();
        for &pos in chunks {
            self.store_falling(pos);
        }
        let modified: FxHashSet<IVec3> =
            chunks.iter().copied().filter(|p| self.chunks.get(p).is_some_and(|s| s.modified)).collect();
        let mut state = self.take_chunk_state(&|c| set.contains(&c), &|c| modified.contains(&c));
        let mut patches = Vec::new();
        let generation = self.region_records.generation.saturating_add(1);
        for &pos in chunks {
            let Some(slot) = self.chunks.get(&pos) else { continue };
            let parts = state.remove(&pos).unwrap_or_default();
            // The record written now is the chunk's whole state.
            self.region_records.level_owned.remove(&pos);
            if self.region_records.owns_all {
                self.region_records.released.insert(pos);
            }
            if !slot.modified && !slot.has_record && parts.is_empty() {
                continue;
            }
            let blocks = slot.modified.then(|| slot.data.clone());
            patches.push((
                pos,
                Patch {
                    blocks: Some(blocks),
                    state: Some(parts),
                    entities: Some(Vec::new()),
                    append: Vec::new(),
                    generation,
                },
            ));
        }
        self.region_records.evicted += patches.len() as u64;
        if let Some(store) = &self.regions {
            store.queue(patches);
        }
    }

    /// Puts a loading chunk's record back into the world's tables (Java's
    /// `ChunkMap.readChunk`): its block entities, ticks and entities, unless
    /// the level file holds them.
    pub(super) fn restore_record(&mut self, pos: IVec3, record: ChunkRecord) {
        if self.level_owns(pos) {
            return;
        }
        let ChunkRecord { state, water, lava, bubbles, fires, entities, .. } = record;
        if !state.is_empty() {
            for system in systems() {
                for (_, text) in state.iter().filter(|(k, _)| k == system.key) {
                    (system.load)(self, text);
                }
            }
        }
        self.suspend_fluid_cells(pos, water, lava, bubbles);
        for (p, delay) in fires {
            self.schedule_fire_in(p, delay);
        }
        if entities.iter().any(|b| !b.is_empty()) {
            self.region_records.entities.push((pos, entities));
        }
    }

    /// The region side of a save. `stored` are the entities stored in
    /// unloaded chunks (all of them but those of chunks still loading). What
    /// the level file held for chunks that aren't loaded goes into their
    /// records (the first save of a world from before region storage moves
    /// it all), and loaded chunks whose blocks changed since their record
    /// was written write them. Returns how many records were queued.
    /// Afterwards the level file holds exactly the loaded chunks' state,
    /// [`World::region_chunks_to_string`] names them and
    /// [`World::save_generation`] is this save's.
    pub fn save_records(&mut self, stored: Vec<(IVec3, Props)>) -> usize {
        if !self.writes_records() {
            return 0;
        }
        let loaded: FxHashSet<IVec3> = self.chunks.keys().copied().collect();
        let loading = self.gen_in_flight.clone();
        let orphan = |c: IVec3| !loaded.contains(&c) && !loading.contains(&c);
        let level_owned = std::mem::take(&mut self.region_records.level_owned);
        let owns_all = self.region_records.owns_all;
        let released = std::mem::take(&mut self.region_records.released);
        let owns = |c: IVec3| (owns_all && !released.contains(&c)) || level_owned.contains(&c);
        let mut patches: FxHashMap<IVec3, Patch> = FxHashMap::default();
        // Every chunk the level file held that isn't here: its record gets
        // the level file's state and entities for it (none, if the level
        // file had none), replacing older ones.
        let mut owned: FxHashSet<IVec3> = level_owned.iter().copied().filter(|&c| orphan(c)).collect();
        if owns_all {
            let at_open = self.regions.as_ref().map(|s| s.stored_at_open().clone()).unwrap_or_default();
            owned.extend(at_open.into_iter().filter(|&c| orphan(c) && !released.contains(&c)));
        }
        for (c, batch) in stored {
            let patch = patches.entry(c).or_default();
            if owns(c) {
                owned.insert(c);
                patch.entities.get_or_insert_with(Vec::new).push(batch);
            } else {
                patch.append.push(batch);
            }
        }
        for (c, parts) in self.take_chunk_state(&orphan, &|_| true) {
            if !owns(c) {
                // Only the level file's chunks have state here unloaded.
                log::warn!("chunk {c} had block entity state without its chunk; writing it to its record");
            }
            owned.insert(c);
            patches.entry(c).or_default().state = Some(parts);
        }
        let saved: Vec<IVec3> = self.saved.keys().copied().filter(|&c| orphan(c)).collect();
        for c in saved {
            if let Some(data) = self.saved.remove(&c) {
                owned.insert(c);
                patches.entry(c).or_default().blocks = Some(Some(data));
            }
        }
        for &c in &owned {
            let patch = patches.entry(c).or_default();
            patch.state.get_or_insert_with(StateParts::default);
            patch.entities.get_or_insert_with(Vec::new);
        }
        for (&pos, slot) in self.chunks.iter_mut() {
            if slot.modified && slot.unsaved {
                slot.unsaved = false;
                slot.has_record = true;
                patches.entry(pos).or_default().blocks = Some(Some(slot.data.clone()));
            }
        }
        // From now on the level file holds the loaded chunks' state, and
        // that of chunks still loading whose state it held.
        let mut now_owned = loaded.clone();
        now_owned.extend(loading.iter().copied().filter(|&c| owns(c)));
        self.region_records.level_owned = now_owned;
        self.region_records.owns_all = false;
        // These records go with the level file this save writes.
        self.region_records.generation = self.region_records.generation.saturating_add(1);
        for patch in patches.values_mut() {
            patch.generation = self.region_records.generation;
        }
        let n = patches.len();
        if let Some(store) = &mut self.regions {
            store.queue(patches);
            store.forget_stored_at_open();
        }
        n
    }

    /// The `region_chunks` text: the chunks whose state the level file holds.
    pub fn region_chunks_to_string(&self) -> String {
        let mut chunks: Vec<IVec3> = self.region_records.level_owned.iter().copied().collect();
        chunks.sort_unstable_by_key(|c| (c.x, c.y, c.z));
        chunks.iter().map(|c| format!("{},{},{}", c.x, c.y, c.z)).collect::<Vec<_>>().join(";")
    }

    /// Reads [`World::region_chunks_to_string`]'s text.
    pub fn parse_region_chunks(text: &str) -> FxHashSet<IVec3> {
        text.split(';')
            .filter_map(|c| {
                let v: Vec<i32> = c.split(',').filter_map(|n| n.trim().parse().ok()).collect();
                (v.len() == 3).then(|| IVec3::new(v[0], v[1], v[2]))
            })
            .collect()
    }

    /// Waits until every queued record and level file is on disk; the
    /// errors since the last report, if any.
    pub fn flush_regions(&self) -> std::io::Result<()> {
        self.regions.as_ref().map_or(Ok(()), RegionStore::flush)
    }

    /// Record write errors since the last call.
    pub fn take_region_errors(&self) -> Vec<String> {
        self.regions.as_ref().map(RegionStore::take_errors).unwrap_or_default()
    }

    /// Chunks that couldn't be read since the last call, for the player.
    pub fn take_region_notices(&self) -> Vec<String> {
        self.regions.as_ref().map(RegionStore::take_notices).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests;
