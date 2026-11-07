//! v0.16 Track B: the jukebox and music discs, echo shards, disc fragments,
//! the recovery compass's items and reinforced deepslate.
//!
//! Java 26.3 references: `JukeboxBlock`, `JukeboxBlockEntity`,
//! `JukeboxSongPlayer`, `JukeboxSong`, `JukeboxPlayable` (javap of the
//! server jar) and `data/minecraft/jukebox_song/*.json` for every disc's
//! length and comparator output. The tunes themselves are original: each
//! disc plays a procedural score ([`tune`]) through the note-block voices,
//! for exactly its Java length. No Minecraft recording or melody is used.
//!
//! Identifiers (track B reservation, `docs/architecture.md`):
//!
//! | Ids | Meaning |
//! |---|---|
//! | 11648, 11649 | jukebox, `has_record` false then true |
//! | 11650 | reinforced deepslate |
//! | 11651..=11775 | reserved for track B, unregistered |
//! | items 1430, 1431, 1432 | echo shard, recovery compass, disc fragment 5 |
//! | items 1433..=1454 | the 22 music discs in Java's registration order |
//! | texture layers 3664..=3668 | jukebox side/top, reinforced deepslate side/top/bottom |

use glam::{DVec3, IVec3};
use rustc_hash::FxHashMap;

use super::World;
use super::block::{Block, RenderKind};
use crate::inventory::{Stack, stack_from_str, stack_to_string};
use crate::item::{Item, ItemInfo, ItemKind, Sprite};

pub const FIRST: u16 = 11648;
pub const JUKEBOX: Block = Block(FIRST);
pub const JUKEBOX_WITH_RECORD: Block = Block(FIRST + 1);
pub const REINFORCED_DEEPSLATE: Block = Block(FIRST + 2);
/// The last registered state.
pub const LAST: u16 = FIRST + 2;
/// The end of track B's state reservation.
pub const RESERVED_LAST: u16 = 11775;
const _: () = assert!(LAST <= RESERVED_LAST);

/// Texture layers, offsets from one base (track B owns 3664..=3759).
pub mod t {
    pub const BASE: u16 = 3664;
    pub const JUKEBOX_SIDE: u16 = BASE;
    pub const JUKEBOX_TOP: u16 = BASE + 1;
    pub const REINFORCED_SIDE: u16 = BASE + 2;
    pub const REINFORCED_TOP: u16 = BASE + 3;
    pub const REINFORCED_BOTTOM: u16 = BASE + 4;
    pub const END: u16 = BASE + 5;
    const _: () = assert!(END <= 3760);
}

pub const fn owns(id: u16) -> bool {
    id >= FIRST && id <= LAST
}

pub fn is_jukebox(b: Block) -> bool {
    b == JUKEBOX || b == JUKEBOX_WITH_RECORD
}

/// Name, render kind and faces (`[side, side, top, bottom, side, side]`).
pub const fn registry(id: u16) -> (&'static str, RenderKind, [u16; 6]) {
    let side = t::JUKEBOX_SIDE;
    match id {
        // Java's `jukebox` model is `cube_top`: its bottom uses the side.
        11648 | 11649 => ("jukebox", RenderKind::Opaque, [side, side, t::JUKEBOX_TOP, side, side, side]),
        _ => (
            "reinforced deepslate",
            RenderKind::Opaque,
            [
                t::REINFORCED_SIDE,
                t::REINFORCED_SIDE,
                t::REINFORCED_TOP,
                t::REINFORCED_BOTTOM,
                t::REINFORCED_SIDE,
                t::REINFORCED_SIDE,
            ],
        ),
    }
}

/// Java's destroy time: the jukebox `strength(2.0, 6.0)` (`mineable/axe`);
/// reinforced deepslate `strength(55.0, 1200.0)`. Reinforced deepslate has
/// no tool tag and an empty loot table, so it is slow to break by any
/// means (82.5 seconds) and drops nothing; Java 26.3 does not make it
/// unbreakable.
pub fn hardness(b: Block) -> Option<f32> {
    if is_jukebox(b) {
        Some(2.0)
    } else if b == REINFORCED_DEEPSLATE {
        Some(55.0)
    } else {
        None
    }
}

/// Java's explosion resistance (`strength(_, resistance)`).
pub fn resistance(b: Block) -> Option<f32> {
    if is_jukebox(b) {
        Some(6.0)
    } else if b == REINFORCED_DEEPSLATE {
        Some(1200.0)
    } else {
        None
    }
}

/// What breaking the block drops: the jukebox itself; reinforced deepslate
/// has `noLootTable`, so nothing.
pub fn drop(b: Block) -> Option<Option<Item>> {
    if is_jukebox(b) {
        Some(Some(Item::from_block(JUKEBOX)))
    } else if b == REINFORCED_DEEPSLATE {
        Some(None)
    } else {
        None
    }
}

/// `PushReaction.BLOCK`: reinforced deepslate, and the jukebox (a block
/// entity, `PistonBaseBlock.isPushable`).
pub fn piston_immovable(b: Block) -> bool {
    owns(b.0)
}

pub fn palette_ids() -> impl Iterator<Item = u16> {
    [JUKEBOX.0, REINFORCED_DEEPSLATE.0].into_iter()
}

/// Original textures: a walnut cabinet with brass corners and a dark
/// record slot; dark deepslate tiles framed with pale sculk-bone ribs.
pub fn pixel(layer: u16, x: usize, y: usize) -> [u8; 4] {
    let h = |s: u64| {
        let mut z = (x as u64) << 32 ^ (y as u64) << 8 ^ layer as u64 ^ s.wrapping_mul(0x9E37_79B9);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) & 0xFFFF) as f32 / 65535.0
    };
    let shade = |c: [u8; 3], f: f32| {
        [
            (c[0] as f32 * f).min(255.0) as u8,
            (c[1] as f32 * f).min(255.0) as u8,
            (c[2] as f32 * f).min(255.0) as u8,
            255,
        ]
    };
    let edge = x == 0 || y == 0 || x == 15 || y == 15;
    let corner = !(2..=13).contains(&x) && !(2..=13).contains(&y);
    match layer {
        t::JUKEBOX_SIDE => {
            if corner {
                shade([176, 132, 68], 0.9 + h(1) * 0.2)
            } else if edge {
                shade([74, 46, 30], 0.9 + h(2) * 0.15)
            } else {
                // Vertical planks with a darker seam every four texels.
                let seam = x.is_multiple_of(4);
                shade([112, 72, 46], if seam { 0.75 } else { 0.92 + h(3) * 0.14 })
            }
        }
        t::JUKEBOX_TOP => {
            let (dx, dy) = (x as f32 - 7.5, y as f32 - 7.5);
            if corner {
                shade([176, 132, 68], 0.9 + h(1) * 0.2)
            } else if edge {
                shade([74, 46, 30], 0.9 + h(2) * 0.15)
            } else if dx.abs() < 5.0 && dy.abs() < 1.2 {
                shade([24, 20, 22], 1.0)
            } else {
                shade([112, 72, 46], 0.88 + h(4) * 0.16)
            }
        }
        _ => {
            // Reinforced deepslate: deepslate tiles with pale bony ribs.
            let rib = match layer {
                t::REINFORCED_SIDE => x == 3 || x == 12 || y == 0 || y == 15,
                t::REINFORCED_TOP => {
                    (x == 3 || x == 12) && (2..14).contains(&y) || (y == 3 || y == 12) && (2..14).contains(&x)
                }
                _ => edge,
            };
            if rib {
                shade([196, 190, 166], 0.85 + h(5) * 0.2)
            } else {
                let mortar = x.is_multiple_of(8) || y.is_multiple_of(8);
                shade([62, 64, 70], if mortar { 0.65 } else { 0.85 + h(6) * 0.25 })
            }
        }
    }
}

/// One `jukebox_song`: its id (the disc is `music_disc_<id>`), length and
/// comparator output (Java 26.3 `data/minecraft/jukebox_song`), and whether
/// the disc is `Rarity.RARE` (`Items`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Song {
    pub id: &'static str,
    pub seconds: f32,
    pub comparator: u8,
    pub rare: bool,
}

const fn song(id: &'static str, seconds: f32, comparator: u8, rare: bool) -> Song {
    Song { id, seconds, comparator, rare }
}

/// Every Java 26.3 music disc, in `Items` registration order (the order of
/// their saved ids from [`DISC_FIRST`]).
pub const SONGS: [Song; 22] = [
    song("13", 178.0, 1, false),
    song("cat", 185.0, 2, false),
    song("blocks", 345.0, 3, false),
    song("bounce", 234.0, 8, false),
    song("chirp", 185.0, 4, false),
    song("creator", 176.0, 12, true),
    song("creator_music_box", 73.0, 11, false),
    song("far", 174.0, 5, false),
    song("lava_chicken", 134.0, 9, true),
    song("mall", 197.0, 6, false),
    song("mellohi", 96.0, 7, false),
    song("stal", 150.0, 8, false),
    song("strad", 188.0, 9, false),
    song("ward", 251.0, 10, false),
    song("11", 71.0, 11, false),
    song("wait", 238.0, 12, false),
    song("otherside", 195.0, 14, true),
    song("relic", 218.0, 14, false),
    song("5", 178.0, 15, false),
    song("pigstep", 149.0, 13, true),
    song("precipice", 299.0, 13, false),
    song("tears", 175.0, 10, false),
];

/// Saved item ids (track B: 1430..=1469).
pub const ITEM_FIRST: u16 = 1430;
pub const ECHO_SHARD: Item = Item(1430);
pub const RECOVERY_COMPASS: Item = Item(1431);
pub const DISC_FRAGMENT_5: Item = Item(1432);
pub const DISC_FIRST: u16 = 1433;
/// One past the last registered item.
pub const ITEM_END: u16 = DISC_FIRST + SONGS.len() as u16;
const _: () = assert!(ITEM_END <= 1470);

/// The music disc of song `i` ([`SONGS`] order).
pub const fn disc(i: usize) -> Item {
    Item(DISC_FIRST + i as u16)
}

/// `music_disc_5`, crafted from nine fragments.
pub const DISC_5: Item = disc(18);
pub const DISC_OTHERSIDE: Item = disc(16);

/// The disc named by a Java id (`music_disc_pigstep`), for loot tables.
pub fn disc_named(java: &str) -> Option<Item> {
    let id = java.strip_prefix("minecraft:").unwrap_or(java).strip_prefix("music_disc_")?;
    SONGS.iter().position(|s| s.id == id).map(disc)
}

/// The song a disc plays (`DataComponents.JUKEBOX_PLAYABLE`).
pub fn song_of(item: Item) -> Option<(usize, &'static Song)> {
    let i = item.0.checked_sub(DISC_FIRST)? as usize;
    SONGS.get(i).map(|s| (i, s))
}

impl Song {
    /// `JukeboxSong.lengthInTicks`: `Mth.ceil(lengthInSeconds * 20)`.
    pub fn length_ticks(&self) -> u64 {
        (self.seconds * 20.0).ceil() as u64
    }
    /// `JukeboxSong.hasFinished`: 20 ticks of silence after the song.
    pub fn finished(&self, ticks: u64) -> bool {
        ticks >= self.length_ticks() + 20
    }
}

/// The creeper's `#creeper_drop_music_discs` (Java 26.3 tag order).
pub const CREEPER_DISCS: [&str; 12] =
    ["13", "cat", "blocks", "chirp", "far", "mall", "mellohi", "stal", "strad", "ward", "11", "wait"];

/// Java's `#skeletons` entity tag, whose kills of a creeper drop a disc.
pub fn is_skeleton(kind: crate::entity::MobKind) -> bool {
    use crate::entity::MobKind::*;
    matches!(kind, Skeleton | Stray | WitherSkeleton | SkeletonHorse | Bogged)
}

/// Names, kinds and sprites of the track's non-block items.
pub fn item_info(item: Item) -> Option<ItemInfo> {
    let material = |name, sprite| ItemInfo { name, kind: ItemKind::Material, max_stack: 64, sprite };
    match item {
        ECHO_SHARD => Some(material("echo shard", Sprite::EchoShard)),
        RECOVERY_COMPASS => Some(material("recovery compass", Sprite::RecoveryCompass)),
        DISC_FRAGMENT_5 => Some(material("disc fragment 5", Sprite::DiscFragment)),
        _ => {
            const NAMES: [&str; 22] = [
                "music disc 13",
                "music disc cat",
                "music disc blocks",
                "music disc bounce",
                "music disc chirp",
                "music disc creator",
                "music disc creator music box",
                "music disc far",
                "music disc lava chicken",
                "music disc mall",
                "music disc mellohi",
                "music disc stal",
                "music disc strad",
                "music disc ward",
                "music disc 11",
                "music disc wait",
                "music disc otherside",
                "music disc relic",
                "music disc 5",
                "music disc pigstep",
                "music disc precipice",
                "music disc tears",
            ];
            let (i, _) = song_of(item)?;
            // `Item.Properties.stacksTo(1)`.
            Some(ItemInfo {
                name: NAMES[i],
                kind: ItemKind::Material,
                max_stack: 1,
                sprite: Sprite::MusicDisc(i as u8),
            })
        }
    }
}

/// Every item this track registers, in id order.
pub fn items() -> impl Iterator<Item = Item> {
    (ITEM_FIRST..ITEM_END).map(Item)
}

/// The title shown when a song starts (Java's `Now Playing` overlay names
/// the disc's `description`; these tunes are original, so the disc's name).
pub fn song_title(i: usize) -> String {
    disc(i).name().split(' ').map(|w| w[..1].to_uppercase() + &w[1..]).collect::<Vec<_>>().join(" ")
}

/// `Rarity.RARE` items of this track (aqua names).
pub fn is_rare(item: Item) -> bool {
    song_of(item).is_some_and(|(_, s)| s.rare)
}

/// A jukebox's block entity: its record and, while a song plays, the ticks
/// since it started (`JukeboxSongPlayer.ticksSinceSongStarted`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Jukebox {
    pub record: Option<Stack>,
    pub playing: Option<u64>,
}

/// Jukebox game events for listeners (`GameEvent.JUKEBOX_PLAY` every 20
/// ticks of a song, `JUKEBOX_STOP_PLAY` when it stops or the jukebox goes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JukeboxEvent {
    Play,
    Stop,
}

/// Integration hook for track A's vibrations: the game event a jukebox
/// emits, by Java name. The entity pass that drains
/// [`World::jukebox_events`] forwards each one here; until the vibration
/// system exists it does nothing.
pub fn game_event_name(event: JukeboxEvent) -> &'static str {
    match event {
        JukeboxEvent::Play => "jukebox_play",
        JukeboxEvent::Stop => "jukebox_stop_play",
    }
}

/// Every jukebox game event passes here (from the entity pass that also
/// sets allays dancing) with the jukebox's position, and is posted to the
/// vibration system at the block's centre, as Java's
/// `level.gameEvent(GameEvent.JUKEBOX_PLAY, pos, Context.of(state))`.
pub fn vibration_hook(world: &mut World, pos: IVec3, event: JukeboxEvent) {
    use super::vibration::{EventSource, GameEvent};
    let event = match event {
        JukeboxEvent::Play => GameEvent::JukeboxPlay,
        JukeboxEvent::Stop => GameEvent::JukeboxStopPlay,
    };
    let block = world.get_block(pos);
    world.emit_game_event(pos.as_dvec3() + DVec3::splat(0.5), event, EventSource::default(), block);
}

/// One note of a playing record, for the audio adapters: where, the
/// note-block instrument index, the pitch (0..=24 like a note block) and
/// the gain.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RecordNote {
    pub pos: IVec3,
    pub instrument: u8,
    pub pitch: u8,
    pub gain: f32,
}

/// Jukebox state kept by the world (saved per dimension).
#[derive(Default)]
pub struct State {
    pub(super) boxes: FxHashMap<IVec3, Jukebox>,
}

impl World {
    pub fn jukebox(&self, p: IVec3) -> Option<&Jukebox> {
        self.jukeboxes.boxes.get(&p)
    }

    /// Whether the jukebox at `p` is playing a song (`isSignalSource`).
    pub fn jukebox_playing(&self, p: IVec3) -> bool {
        self.jukebox(p).is_some_and(|j| j.playing.is_some())
    }

    /// `JukeboxBlock.ownSignal`: 15 while a song plays.
    pub fn jukebox_signal(&self, p: IVec3) -> u8 {
        if self.jukebox_playing(p) { 15 } else { 0 }
    }

    /// `JukeboxBlockEntity.getComparatorOutput`: the record's song output,
    /// whether or not it still plays.
    pub fn jukebox_comparator(&self, p: IVec3) -> u8 {
        self.jukebox(p).and_then(|j| j.record).and_then(|s| song_of(s.item)).map_or(0, |(_, s)| s.comparator)
    }

    /// `JukeboxPlayable.tryInsertIntoJukebox`: puts one disc into an empty
    /// jukebox and starts its song. `false` (nothing taken) for a non-disc,
    /// a jukebox that already holds one, or no jukebox.
    pub fn insert_disc(&mut self, p: IVec3, disc: Stack) -> bool {
        if self.get_block(p) != Some(JUKEBOX) || song_of(disc.item).is_none() {
            return false;
        }
        let record = Stack { count: 1, ..disc };
        // `setTheItem` → `notifyItemChangedInJukebox(true)` → `play`.
        self.edit(p, JUKEBOX_WITH_RECORD, false);
        self.jukeboxes.boxes.insert(p, Jukebox { record: Some(record), playing: Some(0) });
        if self.records_started.len() < 64 {
            self.records_started.push((p, song_of(disc.item).map_or(0, |(i, _)| i)));
        }
        self.redstone_changed(p);
        true
    }

    /// `JukeboxBlockEntity.popOutTheItem`: the record pops out above the
    /// jukebox and the song stops. Returns what popped out.
    pub fn eject_disc(&mut self, p: IVec3) -> Option<Stack> {
        let record = self.jukeboxes.boxes.get_mut(&p)?.record.take()?;
        self.stop_jukebox(p);
        self.jukeboxes.boxes.insert(p, Jukebox::default());
        if self.get_block(p) == Some(JUKEBOX_WITH_RECORD) {
            self.edit(p, JUKEBOX, false);
        }
        self.drops.push((p + IVec3::Y, record));
        self.redstone_changed(p);
        Some(record)
    }

    /// The jukebox as a one-slot container (`ContainerSingleItem`): hoppers
    /// read its record, take it out (`splitTheItem`: the song stops, no
    /// item pops out) and put a disc in (`setTheItem`: it plays).
    /// `false` if there's no jukebox at `p`.
    pub(super) fn set_jukebox_record(&mut self, p: IVec3, stack: Option<Stack>) -> bool {
        let Some(block) = self.get_block(p).filter(|&b| is_jukebox(b)) else { return false };
        match stack {
            Some(disc) => {
                if block == JUKEBOX {
                    self.insert_disc(p, disc);
                }
            }
            None => {
                self.stop_jukebox(p);
                self.jukeboxes.boxes.insert(p, Jukebox::default());
                if block == JUKEBOX_WITH_RECORD {
                    self.edit(p, JUKEBOX, false);
                }
                self.redstone_changed(p);
            }
        }
        true
    }

    /// `JukeboxSongPlayer.stop`: the stop game event and level event 1011.
    fn stop_jukebox(&mut self, p: IVec3) {
        if let Some(j) = self.jukeboxes.boxes.get_mut(&p)
            && j.playing.take().is_some()
        {
            self.jukebox_events.push((p, JukeboxEvent::Stop));
        }
    }

    /// Block changes: a jukebox that goes (`preRemoveSideEffects`) pops its
    /// record and stops; a new jukebox gets an empty entity.
    pub(super) fn track_jukebox(&mut self, p: IVec3, old: Block, new: Block) {
        if is_jukebox(old) && !is_jukebox(new) {
            if let Some(j) = self.jukeboxes.boxes.remove(&p) {
                if j.playing.is_some() {
                    self.jukebox_events.push((p, JukeboxEvent::Stop));
                }
                if let Some(record) = j.record
                    && self.tile_drops
                {
                    self.drops.push((p, record));
                }
            }
        } else if is_jukebox(new) {
            self.jukeboxes.boxes.entry(p).or_default();
        }
    }

    /// `JukeboxBlockEntity.tick` for every jukebox in a loaded chunk: the
    /// song ends 20 ticks after its length; while it plays, `jukebox_play`
    /// and a note particle every 20 ticks, and the tune's notes.
    pub fn tick_jukeboxes(&mut self) {
        if self.records.len() > 4096 {
            self.records.clear();
        }
        let playing: Vec<IVec3> =
            self.jukeboxes.boxes.iter().filter(|(_, j)| j.playing.is_some()).map(|(p, _)| *p).collect();
        for p in playing {
            if !self.is_loaded(p) {
                continue;
            }
            let Some(j) = self.jukeboxes.boxes.get(&p).copied() else { continue };
            let (Some(ticks), Some(record)) = (j.playing, j.record) else { continue };
            let Some((index, song)) = song_of(record.item) else { continue };
            if song.finished(ticks) {
                self.stop_jukebox(p);
                self.redstone_changed(p);
                continue;
            }
            if ticks.is_multiple_of(20) {
                self.jukebox_events.push((p, JukeboxEvent::Play));
                // `spawnMusicParticles`: `nextInt(4) / 24` tints the note.
                let tint = (self.roll() % 4) as u8;
                let mut burst = crate::particles::Burst::new(
                    crate::particles::Kind::Note,
                    p.as_dvec3() + DVec3::new(0.5, 1.2, 0.5),
                    1,
                );
                burst.color = Some(super::gadgets::note_color(tint));
                self.particles.push(crate::particles::Request::Burst(burst));
            }
            if ticks < song.length_ticks() {
                for note in tune(index, ticks) {
                    self.records.push(RecordNote { pos: p, ..note });
                }
            }
            if let Some(j) = self.jukeboxes.boxes.get_mut(&p) {
                j.playing = Some(ticks + 1);
            }
        }
    }

    /// `[[x,y,z],"stack",ticks|null]` entries for the level file.
    pub fn jukeboxes_to_string(&self) -> String {
        let mut entries: Vec<_> = self
            .jukeboxes
            .boxes
            .iter()
            .filter(|(_, j)| j.record.is_some())
            .map(|(p, j)| (p.to_array(), stack_to_string(j.record), j.playing))
            .collect();
        entries.sort_by_key(|e| e.0);
        serde_json::to_string(&entries).expect("jukebox serialization")
    }

    /// Restores saved jukeboxes. A song that had finished doesn't resume
    /// (`setSongWithoutPlaying` ignores finished songs).
    pub fn load_jukeboxes(&mut self, text: &str) {
        type Entry = ([i32; 3], String, Option<u64>);
        let Ok(entries) = serde_json::from_str::<Vec<Entry>>(text) else { return };
        for (p, stack, playing) in entries {
            let Some(Some(record)) = stack_from_str(&stack) else { continue };
            let playing = playing.filter(|&t| song_of(record.item).is_some_and(|(_, s)| !s.finished(t)));
            self.jukeboxes.boxes.insert(IVec3::from_array(p), Jukebox { record: Some(record), playing });
        }
    }
}

/// A note of a song's score at `tick`: instrument, pitch and gain (the
/// position is filled in by the caller).
fn note(instrument: crate::world::gadgets::Instrument, pitch: i32, gain: f32) -> RecordNote {
    RecordNote { pos: IVec3::ZERO, instrument: instrument as u8, pitch: pitch.clamp(0, 24) as u8, gain }
}

fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The notes song `index` sounds at `tick` (0-based from the song's start).
///
/// Each disc has an original seeded arrangement: a key, a mode, a tempo in
/// whole ticks per step, a lead and a bass instrument, a chord progression
/// and two eight-step motifs laid out as A A B A phrases, with an intro and
/// a closing bar. The score is a pure function of the disc and tick, so a
/// reloaded jukebox resumes in place and every listener hears the same.
pub fn tune(index: usize, tick: u64) -> impl Iterator<Item = RecordNote> {
    use crate::world::gadgets::Instrument as I;
    let seed = mix(0x5EC0_4D00 ^ index as u64);
    let song = SONGS[index.min(SONGS.len() - 1)];
    let step = [2u64, 3, 3, 4, 4, 5][(seed % 6) as usize];
    let mut out: [Option<RecordNote>; 4] = [None; 4];
    if !tick.is_multiple_of(step) {
        return out.into_iter().flatten();
    }
    let s = tick / step;
    let total_steps = song.length_ticks() / step;
    let (bar, pos) = (s / 8, (s % 8) as i32);
    const MODES: [[i32; 7]; 4] =
        [[0, 2, 4, 5, 7, 9, 11], [0, 2, 3, 5, 7, 8, 10], [0, 2, 3, 5, 7, 9, 10], [0, 2, 4, 7, 9, 12, 14]];
    let mode = MODES[(seed >> 8) as usize % 4];
    let key = ((seed >> 12) % 7) as i32;
    let degree = |d: i32| -> i32 {
        let octave = d.div_euclid(7);
        mode[d.rem_euclid(7) as usize] + 12 * octave
    };
    const PROGRESSIONS: [[i32; 4]; 6] =
        [[0, 4, 5, 3], [0, 3, 4, 0], [5, 3, 0, 4], [0, 5, 3, 4], [1, 4, 0, 0], [0, 2, 3, 4]];
    let section = bar / 16;
    let progression = PROGRESSIONS[(mix(seed ^ section) % 6) as usize];
    let root = progression[(bar % 4) as usize];
    let leads = [I::Harp, I::Bell, I::Flute, I::Chime, I::Xylophone, I::Pling, I::Guitar, I::IronXylophone, I::Banjo];
    let lead = leads[((seed >> 20) % leads.len() as u64) as usize];
    let bass = if (seed >> 24) & 1 == 0 { I::Bass } else { I::Didgeridoo };
    let closing = s + 8 >= total_steps;
    let intro = bar < 2;
    // Bass: the chord root on the bar's first and fifth steps.
    if pos == 0 || pos == 4 && !closing {
        out[0] = Some(note(bass, key + degree(root) + if pos == 4 { 7 } else { 0 }, 0.8));
    }
    // Lead: phrase motifs A A B A, each an eight-step rhythm and contour.
    let phrase = (bar / 4) % 4;
    let motif = mix(seed ^ (section << 8) ^ if phrase == 2 { 0xB } else { 0xA });
    let rhythm = (motif & 0xFF) as u8 | 1;
    if !intro && !closing && rhythm & (1u8 << pos) != 0 {
        let contour = ((motif >> (8 + pos as u64 * 3)) & 7) as i32 - 2;
        let pitch = key + degree(root + contour + if bar % 2 == 1 { 1 } else { 0 }) + 7;
        let pitch = if pitch > 24 { pitch - 12 } else { pitch };
        out[1] = Some(note(lead, pitch, 0.7));
    }
    // A broken chord on the off-steps, softer.
    if (seed >> 28) & 1 == 1 && pos % 2 == 1 && !closing {
        let tone = [0, 2, 4][(pos as usize / 2) % 3];
        out[2] = Some(note(I::Chime, key + degree(root + tone), 0.35));
    }
    // Percussion for the livelier discs.
    if !(seed >> 30).is_multiple_of(3) && !intro && !closing {
        out[3] = match pos {
            0 | 4 => Some(note(I::BassDrum, 6, 0.55)),
            2 | 6 => Some(note(I::Snare, 12, 0.4)),
            _ => Some(note(I::Hat, 18, 0.25)),
        };
    }
    if closing && pos == 0 && s + 8 >= total_steps && s + 8 < total_steps + 8 {
        out[1] = Some(note(lead, key + degree(0) + 12, 0.7));
    }
    out.into_iter().flatten()
}

#[cfg(test)]
mod tests;
