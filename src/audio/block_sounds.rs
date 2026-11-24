//! v0.18 Track 9: every block's own sounds, from its Java 26.3 `SoundType`.
//!
//! `tests/fixtures/java/26.3/sounds.json` (from
//! `tools/extract_java_sound_fixtures.py`) names each Java block's
//! `SoundType`, and each `SoundType`'s volume, pitch and five events. A
//! VoxelCraft state finds its Java block by name
//! ([`voxelcraft::java_registry::state_java_ids`]), so no block's sound is
//! guessed; states with no Java block keep the shared [`Material`] sounds.
//!
//! Each Java event family (`block.<family>.break` and its siblings) has its
//! own original procedural recipe here, built from the same granular crunch,
//! knock, ring and squelch voices as the shared materials. As in Java's
//! `sounds.json`, a family's hit and fall events play its step sounds.
//!
//! Playback follows Java's call sites: breaking and placing at
//! `(volume + 1) / 2` and `pitch * 0.8`, footsteps at `volume * 0.15` and
//! `pitch`, mining hits at `(volume + 1) / 8` and `pitch * 0.5`, and falls at
//! `volume * 0.5` and `pitch * 0.75`. [`GAIN`] scales Java's volumes to the
//! loudness of VoxelCraft's synthesized buffers, so a volume-1 type plays
//! exactly as loud as the shared material sounds did.

use std::collections::HashMap;
use std::sync::OnceLock;

use super::dsp::{self, Biquad, Mode, Rng, add_mode, crackle, mix_into, noise, samples};
use super::sounds::{BlockCue, ContainerCue, Crunch, Material, Sound, crunch, crunch_params};
use crate::world::block::{Block, STATE_CAPACITY};

/// Java's event families, sorted (`block.<family>.*`).
const FAMILIES: [&str; 126] = [
    "amethyst_block",
    "amethyst_cluster",
    "ancient_debris",
    "anvil",
    "azalea",
    "azalea_leaves",
    "bamboo",
    "bamboo_sapling",
    "bamboo_wood",
    "bamboo_wood_hanging_sign",
    "basalt",
    "big_dripleaf",
    "bone_block",
    "cactus_flower",
    "calcite",
    "candle",
    "cave_vines",
    "chain",
    "cherry_leaves",
    "cherry_sapling",
    "cherry_wood",
    "cherry_wood_hanging_sign",
    "chiseled_bookshelf",
    "cinnabar",
    "cobweb",
    "copper",
    "copper_bulb",
    "copper_golem_statue",
    "copper_grate",
    "coral_block",
    "creaking_heart",
    "crop",
    "decorated_pot",
    "deepslate",
    "deepslate_bricks",
    "deepslate_tiles",
    "dried_ghast",
    "dripstone_block",
    "flowering_azalea",
    "froglight",
    "frogspawn",
    "fungus",
    "gilded_blackstone",
    "glass",
    "grass",
    "gravel",
    "hanging_roots",
    "hanging_sign",
    "heavy_core",
    "honey_block",
    "iron",
    "ladder",
    "lantern",
    "large_amethyst_bud",
    "leaf_litter",
    "lily_pad",
    "lodestone",
    "mangrove_roots",
    "medium_amethyst_bud",
    "metal",
    "moss",
    "moss_carpet",
    "mud",
    "mud_bricks",
    "muddy_mangrove_roots",
    "nether_bricks",
    "nether_gold_ore",
    "nether_ore",
    "nether_sprouts",
    "nether_wart",
    "nether_wood",
    "nether_wood_hanging_sign",
    "netherite_block",
    "netherrack",
    "nylium",
    "packed_mud",
    "pink_petals",
    "pointed_dripstone",
    "polished_deepslate",
    "polished_tuff",
    "poplar_leaves",
    "potent_sulfur",
    "powder_snow",
    "red_shrub",
    "resin",
    "resin_bricks",
    "rooted_dirt",
    "roots",
    "sand",
    "scaffolding",
    "sculk",
    "sculk_catalyst",
    "sculk_sensor",
    "sculk_shrieker",
    "sculk_vein",
    "shelf",
    "shelf_mushroom",
    "shroomlight",
    "slime_block",
    "small_amethyst_bud",
    "small_dripleaf",
    "snow",
    "soul_sand",
    "soul_soil",
    "spawner",
    "sponge",
    "spore_blossom",
    "stem",
    "stone",
    "straw_bed",
    "sulfur",
    "sulfur_spike",
    "suspicious_gravel",
    "suspicious_sand",
    "sweet_berry_bush",
    "trial_spawner",
    "tuff",
    "tuff_bricks",
    "vault",
    "vine",
    "wart_block",
    "weeping_vines",
    "wet_grass",
    "wet_sponge",
    "wood",
    "wool",
];

pub const FAMILY_COUNT: usize = FAMILIES.len();

/// A Java block sound event family: an index into [`FAMILIES`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Family(u8);

impl Family {
    pub fn index(self) -> usize {
        self.0 as usize
    }

    pub fn name(self) -> &'static str {
        FAMILIES[self.index()]
    }

    pub fn from_name(name: &str) -> Option<Family> {
        FAMILIES.binary_search(&name).ok().map(|i| Family(i as u8))
    }

    /// Every family that some Java `SoundType` plays, with the cues it has.
    pub fn all() -> impl Iterator<Item = Family> {
        (0..FAMILY_COUNT as u8).map(Family)
    }

    /// Whether some `SoundType` plays `block.<family>.<cue>`.
    pub fn has(self, cue: BlockCue) -> bool {
        table().cues[self.index()] & (1 << cue as u8) != 0
    }
}

/// [`Sound`]s of every family and cue. Keys stay dense, so the few pairs
/// no `SoundType` plays (`block.crop.place` is `item.crop.plant` in Java)
/// are synthesized too, though nothing plays them.
pub fn sounds() -> impl Iterator<Item = Sound> {
    Family::all().flat_map(|f| BlockCue::ALL.map(move |c| Sound::Block(f, c)))
}

/// The step sound a hit or fall shares its buffers with (Java's hit and
/// fall events play the step event's files).
pub fn shares(sound: Sound) -> Option<Sound> {
    match sound {
        Sound::Block(f, BlockCue::Hit | BlockCue::Fall) if f.has(BlockCue::Step) => {
            Some(Sound::Block(f, BlockCue::Step))
        }
        _ => None,
    }
}

/// A Java `SoundType` as a block plays it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SoundType {
    /// Family of each [`BlockCue`]'s event; `None` for an empty event
    /// (`intentionally_empty`) or one outside `block.*`.
    pub families: [Option<Family>; 5],
    pub volume: f32,
    pub pitch: f32,
}

struct Table {
    types: Vec<(String, SoundType)>,
    /// Per state, an index into `types` (`u16::MAX` for none).
    by_state: Vec<u16>,
    /// Per family, a bit per [`BlockCue`] some type plays.
    cues: [u8; FAMILY_COUNT],
    /// The parsed fixture: subtitles read it too.
    fixture: serde_json::Value,
}

const FIXTURE: &str = include_str!("../../tests/fixtures/java/26.3/sounds.json");

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| {
        let fixture: serde_json::Value = serde_json::from_str(FIXTURE).expect("sounds fixture");
        let mut types = Vec::new();
        let mut index: HashMap<String, u16> = HashMap::new();
        let mut cues = [0u8; FAMILY_COUNT];
        for (name, t) in fixture["sound_types"].as_object().expect("sound types") {
            let families = BlockCue::ALL.map(|cue| {
                let event = t[cue_key(cue)].as_str().unwrap_or("");
                let family = event
                    .strip_prefix("block.")
                    .and_then(|e| e.strip_suffix(&format!(".{}", cue_key(cue))))
                    .and_then(Family::from_name);
                if let Some(f) = family {
                    cues[f.index()] |= 1 << cue as u8;
                }
                family
            });
            let number = |k: &str| t[k].as_f64().unwrap_or(1.0) as f32;
            index.insert(name.clone(), types.len() as u16);
            types.push((name.clone(), SoundType { families, volume: number("volume"), pitch: number("pitch") }));
        }
        let names = fixture["block_names"].as_object().expect("block names");
        let ids = voxelcraft::java_registry::state_java_ids(
            names.iter().map(|(id, n)| (id.as_str(), n.as_str().unwrap_or(""))),
        );
        let blocks = &fixture["block_sound_types"];
        let by_state = (0..STATE_CAPACITY)
            .map(|i| {
                let name = Block(i as u16).name();
                ids.get(name).and_then(|id| blocks[id].as_str()).and_then(|t| index.get(t).copied()).unwrap_or(u16::MAX)
            })
            .collect();
        Table { types, by_state, cues, fixture }
    })
}

/// The parsed fixture, shared with the subtitles.
pub(super) fn fixture() -> &'static serde_json::Value {
    &table().fixture
}

fn cue_key(cue: BlockCue) -> &'static str {
    match cue {
        BlockCue::Break => "break",
        BlockCue::Place => "place",
        BlockCue::Step => "step",
        BlockCue::Hit => "hit",
        BlockCue::Fall => "fall",
    }
}

/// Builds the table now (startup), so the first sound doesn't wait for it.
pub fn warm() {
    table();
}

/// The Java `SoundType` of a block state and its name, if the state is a
/// Java block's.
pub fn sound_type(block: Block) -> Option<(&'static str, SoundType)> {
    let t = table();
    let i = *t.by_state.get(block.0 as usize)?;
    t.types.get(i as usize).map(|(n, s)| (n.as_str(), *s))
}

/// Calibration from Java's per-cue volume to VoxelCraft's buffers: the
/// shared materials' gains (break 1.0, place 0.9, step 0.55, mining hit
/// 0.45, fall 0.8) over Java's for a volume-1 type (1.0, 1.0, 0.15, 0.25,
/// 0.5).
pub const GAIN: [f32; 5] = [1.0, 0.9, 0.55 / 0.15, 0.45 / 0.25, 0.8 / 0.5];

/// What to play for `cue` of `block`: the sound, gain and pitch, by Java's
/// formulas; the shared material sound for states with no Java block.
pub fn play(block: Block, cue: BlockCue) -> Option<(Sound, f32, f32)> {
    let Some((_, t)) = sound_type(block) else {
        let m = super::sounds::material(block);
        let sound = match cue {
            BlockCue::Break => Sound::Break(m),
            BlockCue::Place => Sound::Place(m),
            _ => Sound::Step(m),
        };
        let (gain, pitch) = match cue {
            BlockCue::Break => (1.0, 0.9),
            BlockCue::Place => (0.9, 0.9),
            BlockCue::Step => (0.55, 1.0),
            BlockCue::Hit => (0.45, 0.75),
            BlockCue::Fall => (0.8, 1.0),
        };
        return Some((sound, gain, pitch));
    };
    let family = t.families[cue as usize]?;
    let (volume, pitch) = match cue {
        BlockCue::Break | BlockCue::Place => ((t.volume + 1.0) / 2.0, t.pitch * 0.8),
        BlockCue::Step => (t.volume * 0.15, t.pitch),
        BlockCue::Hit => ((t.volume + 1.0) / 8.0, t.pitch * 0.5),
        BlockCue::Fall => (t.volume * 0.5, t.pitch * 0.75),
    };
    Some((Sound::Block(family, cue), volume * GAIN[cue as usize], pitch))
}

// --- Recipes ------------------------------------------------------------------

/// The voice that colours a family's grit.
#[derive(Clone, Copy)]
enum Tone {
    None,
    /// Hollow wooden knock at `f0` Hz, ringing `damp` times as long as oak.
    Wood {
        f0: f32,
        damp: f32,
    },
    /// Inharmonic metal ring at `f` Hz decaying over `tau` seconds.
    Metal {
        f: f32,
        tau: f32,
    },
    /// Bell-like crystal chime at `f` Hz.
    Crystal {
        f: f32,
    },
    /// Glass pings at `f` Hz.
    Glass {
        f: f32,
    },
    /// A wet squelch with a bubble falling from `f` Hz.
    Squelch {
        f: f32,
    },
    /// Little metal links clinking at `f` Hz.
    Chain {
        f: f32,
    },
    /// The thick, slow honey squelch.
    Honey,
}

struct Spec {
    /// The granular bed (`None` for a pure tone).
    grit: Option<Crunch>,
    tone: Tone,
    /// Tone level against the grit.
    tone_level: f32,
    /// Overall peak level.
    level: f32,
    /// Duration scale.
    length: f32,
}

/// A crunch with its parameters scaled: `pitch` moves every band, `grain`
/// the grain rate, `weight` the low thump, `hiss` and `flutter` set.
fn grit(base: Material, pitch: f32, grain: f32, weight: f32, hiss: f32, flutter: f32) -> Option<Crunch> {
    let c = crunch_params(base);
    Some(Crunch {
        lo: c.lo * pitch,
        hi: (c.hi * pitch).min(11_000.0),
        density: c.density * grain,
        grain_ms: (c.grain_ms.0 / pitch.sqrt(), c.grain_ms.1 / pitch.sqrt()),
        body: (c.body.0 * pitch, c.body.1),
        thump: (if c.thump.0 > 0.0 { c.thump.0 } else { 90.0 } * pitch.sqrt(), c.thump.1.max(0.15) * weight),
        hiss,
        flutter,
    })
}

const fn spec(grit: Option<Crunch>, tone: Tone, tone_level: f32, level: f32, length: f32) -> Spec {
    Spec { grit, tone, tone_level, level, length }
}

fn stone(pitch: f32, grain: f32, weight: f32) -> Spec {
    spec(grit(Material::Stone, pitch, grain, weight, 0.12, 0.0), Tone::None, 0.0, 1.0, 1.0)
}

fn ringing_stone(pitch: f32, grain: f32, weight: f32, f: f32, tau: f32, ring: f32) -> Spec {
    spec(grit(Material::Stone, pitch, grain, weight, 0.1, 0.0), Tone::Metal { f, tau }, ring, 1.0, 1.0)
}

fn plant(base: Material, pitch: f32, grain: f32, flutter: f32, level: f32) -> Spec {
    let hiss = crunch_params(base).hiss;
    spec(grit(base, pitch, grain, 0.6, hiss, flutter), Tone::None, 0.0, level, 1.0)
}

fn soft(base: Material, pitch: f32, grain: f32, level: f32, length: f32) -> Spec {
    let c = crunch_params(base);
    spec(grit(base, pitch, grain, 0.5, c.hiss, c.flutter), Tone::None, 0.0, level, length)
}

fn wood(f0: f32, damp: f32, grain: f32) -> Spec {
    spec(grit(Material::Dirt, 1.6, 0.35 * grain, 0.2, 0.08, 0.0), Tone::Wood { f0, damp }, 1.0, 1.0, 1.0)
}

fn metal(f: f32, tau: f32, weight: f32) -> Spec {
    spec(grit(Material::Stone, 1.2, 0.4, weight, 0.05, 0.0), Tone::Metal { f, tau }, 1.0, 1.0, 1.0)
}

fn squelch(base: Material, f: f32, wet: f32, level: f32) -> Spec {
    let hiss = crunch_params(base).hiss;
    spec(grit(base, 0.8, 0.6, 0.8, hiss, 0.0), Tone::Squelch { f }, wet, level, 1.0)
}

fn crystal(f: f32, grit_pitch: f32) -> Spec {
    spec(grit(Material::Glass, grit_pitch, 0.4, 0.2, 0.04, 0.0), Tone::Crystal { f }, 1.0, 0.9, 1.0)
}

/// Each family's recipe. Parameters are chosen for VoxelCraft's own sounds
/// (nobody has listened yet: see the verification record); nothing is
/// derived from Java's audio.
fn recipe(f: Family) -> Option<Spec> {
    use Material as M;
    Some(match f.name() {
        // Stone and its relatives: brighter for smooth cut stone, lower and
        // denser for deepslate, softer and duller for tuff and mud bricks.
        "stone" => stone(1.0, 1.0, 1.0),
        "deepslate" => stone(0.72, 1.25, 1.5),
        "deepslate_bricks" => stone(0.85, 0.8, 1.3),
        "deepslate_tiles" => stone(1.05, 0.7, 1.1),
        "polished_deepslate" => stone(0.95, 0.55, 1.4),
        "tuff" => stone(0.65, 0.9, 1.1),
        "tuff_bricks" => stone(0.78, 0.75, 1.2),
        "polished_tuff" => stone(0.85, 0.6, 1.1),
        "calcite" => stone(1.35, 1.2, 0.7),
        "dripstone_block" => stone(0.9, 1.4, 0.9),
        "pointed_dripstone" => stone(1.5, 0.7, 0.4),
        "basalt" => ringing_stone(0.8, 0.9, 1.3, 620.0, 0.05, 0.25),
        "netherrack" => soft(M::Gravel, 1.1, 1.3, 0.9, 1.0),
        "nether_bricks" => stone(1.15, 0.6, 1.2),
        "nether_ore" => ringing_stone(1.0, 1.1, 0.9, 1800.0, 0.04, 0.2),
        "nether_gold_ore" => ringing_stone(1.05, 1.0, 0.9, 2300.0, 0.06, 0.3),
        "gilded_blackstone" => ringing_stone(0.9, 0.9, 1.1, 2100.0, 0.07, 0.3),
        "ancient_debris" => ringing_stone(0.6, 1.0, 1.8, 420.0, 0.09, 0.4),
        "bone_block" => wood(520.0, 0.5, 1.4),
        "lodestone" => ringing_stone(0.8, 0.8, 1.4, 760.0, 0.1, 0.45),
        "mud_bricks" => stone(0.6, 0.8, 1.0),
        "packed_mud" => soft(M::Dirt, 0.8, 0.9, 0.85, 1.0),
        "resin_bricks" => stone(1.2, 0.5, 0.8),
        "resin" => squelch(M::Sand, 700.0, 0.4, 0.7),
        "cinnabar" => stone(1.1, 1.3, 0.9),
        "sulfur" => soft(M::Gravel, 1.25, 1.1, 0.85, 0.9),
        "sulfur_spike" => stone(1.4, 0.8, 0.5),
        "potent_sulfur" => squelch(M::Gravel, 520.0, 0.35, 0.9),
        "coral_block" => soft(M::Gravel, 1.35, 1.5, 0.9, 0.9),
        "cactus_flower" => plant(M::Leaves, 1.3, 0.5, 0.3, 0.6),
        // Metals.
        "metal" => metal(1450.0, 0.18, 0.6),
        "iron" => metal(980.0, 0.22, 0.9),
        "netherite_block" => metal(620.0, 0.16, 1.6),
        "anvil" => metal(820.0, 0.4, 1.8),
        "heavy_core" => metal(360.0, 0.25, 2.2),
        "copper" => metal(1250.0, 0.1, 0.8),
        "copper_bulb" => metal(1550.0, 0.12, 0.6),
        "copper_grate" => metal(1900.0, 0.06, 0.5),
        "copper_golem_statue" => metal(1100.0, 0.14, 1.0),
        "lantern" => metal(2400.0, 0.09, 0.4),
        "chain" => spec(None, Tone::Chain { f: 3200.0 }, 1.0, 0.8, 1.0),
        "spawner" => metal(1700.0, 0.08, 0.7),
        "trial_spawner" => metal(1350.0, 0.11, 1.0),
        "vault" => metal(900.0, 0.14, 1.2),
        // Crystal.
        "amethyst_block" => crystal(1760.0, 1.1),
        "amethyst_cluster" => crystal(2640.0, 1.4),
        "large_amethyst_bud" => crystal(2350.0, 1.3),
        "medium_amethyst_bud" => crystal(2900.0, 1.4),
        "small_amethyst_bud" => crystal(3500.0, 1.5),
        "glass" => spec(None, Tone::Glass { f: 2200.0 }, 1.0, 1.0, 1.0),
        "froglight" => spec(grit(M::Glass, 0.7, 0.5, 0.8, 0.1, 0.0), Tone::Glass { f: 900.0 }, 0.5, 0.8, 1.0),
        // Earth, sand and snow.
        "gravel" => soft(M::Gravel, 1.0, 1.0, 1.0, 1.0),
        "suspicious_gravel" => soft(M::Gravel, 0.9, 0.8, 0.9, 1.1),
        "sand" => soft(M::Sand, 1.0, 1.0, 1.0, 1.0),
        "suspicious_sand" => soft(M::Sand, 0.9, 0.8, 0.9, 1.1),
        "soul_sand" => soft(M::Sand, 0.7, 0.7, 0.9, 1.15),
        "soul_soil" => soft(M::Dirt, 0.75, 0.8, 0.9, 1.1),
        "rooted_dirt" => soft(M::Dirt, 1.1, 1.2, 0.95, 1.0),
        "snow" => soft(M::Snow, 1.0, 1.0, 1.0, 1.0),
        "powder_snow" => soft(M::Snow, 1.25, 1.4, 0.8, 1.2),
        "wool" => soft(M::Snow, 0.55, 0.6, 0.75, 1.1),
        "candle" => soft(M::Snow, 1.4, 0.5, 0.6, 0.7),
        "straw_bed" => plant(M::Grass, 0.9, 1.2, 0.5, 0.85),
        "cobweb" => plant(M::Leaves, 1.6, 0.4, 0.6, 0.55),
        "mud" => squelch(M::Dirt, 260.0, 1.0, 0.9),
        "muddy_mangrove_roots" => squelch(M::Dirt, 340.0, 0.7, 0.9),
        "slime_block" => squelch(M::Sand, 420.0, 1.2, 0.9),
        "honey_block" => spec(None, Tone::Honey, 1.0, 1.0, 1.0),
        "sculk" => squelch(M::Dirt, 380.0, 0.6, 0.85),
        "sculk_vein" => squelch(M::Leaves, 520.0, 0.4, 0.7),
        "sculk_catalyst" => squelch(M::Gravel, 300.0, 0.5, 0.9),
        "sculk_sensor" => squelch(M::Dirt, 460.0, 0.5, 0.85),
        "sculk_shrieker" => squelch(M::Gravel, 240.0, 0.55, 0.95),
        "frogspawn" => squelch(M::Leaves, 900.0, 0.8, 0.6),
        "wart_block" => squelch(M::Dirt, 330.0, 0.35, 0.85),
        "shroomlight" => squelch(M::Snow, 480.0, 0.3, 0.8),
        "sponge" => soft(M::Snow, 1.2, 1.3, 0.8, 0.9),
        "wet_sponge" => squelch(M::Snow, 600.0, 0.7, 0.8),
        "dried_ghast" => soft(M::Gravel, 0.8, 0.6, 0.8, 1.0),
        "decorated_pot" => wood(410.0, 0.35, 0.6),
        "nylium" => plant(M::Grass, 0.85, 1.1, 0.2, 0.95),
        // Plants.
        "grass" => plant(M::Grass, 1.0, 1.0, 0.3, 1.0),
        "wet_grass" => squelch(M::Grass, 700.0, 0.3, 0.9),
        "crop" => plant(M::Grass, 1.2, 0.8, 0.4, 0.85),
        "moss" => plant(M::Grass, 0.7, 0.8, 0.2, 0.85),
        "moss_carpet" => plant(M::Grass, 0.75, 0.6, 0.2, 0.7),
        "azalea" => plant(M::Leaves, 0.9, 1.0, 0.6, 0.85),
        "flowering_azalea" => plant(M::Leaves, 1.0, 1.0, 0.7, 0.85),
        "azalea_leaves" => plant(M::Leaves, 0.85, 1.1, 0.8, 0.95),
        "cherry_leaves" => plant(M::Leaves, 1.15, 0.9, 0.9, 0.9),
        "poplar_leaves" => plant(M::Leaves, 1.05, 1.0, 0.85, 0.9),
        "leaf_litter" => plant(M::Leaves, 1.25, 1.4, 0.5, 0.8),
        "pink_petals" => plant(M::Leaves, 1.35, 0.6, 0.7, 0.7),
        "spore_blossom" => plant(M::Leaves, 0.95, 0.5, 0.9, 0.7),
        "small_dripleaf" => plant(M::Leaves, 0.9, 0.6, 0.5, 0.8),
        "big_dripleaf" => plant(M::Leaves, 0.75, 0.7, 0.4, 0.9),
        "lily_pad" => squelch(M::Leaves, 800.0, 0.4, 0.75),
        "cave_vines" => plant(M::Leaves, 0.8, 0.8, 0.6, 0.8),
        "vine" => plant(M::Leaves, 1.1, 0.7, 0.7, 0.75),
        "weeping_vines" => plant(M::Leaves, 0.7, 0.8, 0.5, 0.8),
        "hanging_roots" => plant(M::Dirt, 1.2, 0.6, 0.4, 0.7),
        "roots" => plant(M::Dirt, 1.3, 0.7, 0.3, 0.75),
        "mangrove_roots" => wood(300.0, 0.4, 1.6),
        "fungus" => plant(M::Grass, 0.8, 0.5, 0.2, 0.75),
        "nether_sprouts" => plant(M::Grass, 1.4, 0.6, 0.3, 0.7),
        "nether_wart" => plant(M::Dirt, 1.1, 0.6, 0.2, 0.75),
        "sweet_berry_bush" => plant(M::Leaves, 1.0, 1.2, 0.5, 0.85),
        "red_shrub" => plant(M::Grass, 1.1, 0.7, 0.4, 0.75),
        "shelf_mushroom" => soft(M::Snow, 1.1, 0.6, 0.7, 0.8),
        "bamboo" => wood(720.0, 1.4, 0.5),
        "bamboo_sapling" => plant(M::Leaves, 1.2, 0.5, 0.4, 0.7),
        "cherry_sapling" => plant(M::Leaves, 1.3, 0.5, 0.6, 0.7),
        // Woods: oak's knock, cherry's brighter and lighter, bamboo's
        // hollow tube, the nether stems' dense and dry.
        "wood" => wood(275.0, 1.0, 1.0),
        "cherry_wood" => wood(340.0, 0.8, 0.8),
        "bamboo_wood" => wood(460.0, 1.6, 0.6),
        "nether_wood" => wood(225.0, 0.6, 1.3),
        "stem" => wood(250.0, 0.7, 1.1),
        "hanging_sign" => wood(390.0, 1.2, 0.5),
        "cherry_wood_hanging_sign" => wood(450.0, 1.0, 0.45),
        "bamboo_wood_hanging_sign" => wood(560.0, 1.5, 0.4),
        "nether_wood_hanging_sign" => wood(330.0, 0.7, 0.6),
        "ladder" => wood(360.0, 0.6, 0.7),
        "scaffolding" => wood(600.0, 1.2, 0.4),
        "chiseled_bookshelf" => wood(240.0, 0.5, 1.4),
        "shelf" => wood(310.0, 0.9, 0.8),
        "creaking_heart" => wood(190.0, 0.9, 1.2),
        _ => return None,
    })
}

/// Whether a family has a recipe of its own (every one should).
#[cfg(test)]
pub fn has_recipe(f: Family) -> bool {
    recipe(f).is_some()
}

/// Kind-dependent shape: (seconds, grain density decay, thump, peak).
fn shape(cue: BlockCue) -> (f32, f32, f32, f32) {
    match cue {
        BlockCue::Break => (0.38, 0.09, 1.0, 0.8),
        BlockCue::Place => (0.18, 0.035, 1.6, 0.7),
        _ => (0.13, 0.025, 1.0, 0.42),
    }
}

/// Synthesizes one variant of a family's cue.
pub fn render(f: Family, cue: BlockCue, rng: &mut Rng) -> Vec<f32> {
    let spec = recipe(f).unwrap_or_else(|| stone(1.0, 1.0, 1.0));
    let (secs, tau, thump, peak) = shape(cue);
    let (secs, tau) = (secs * spec.length, tau * spec.length);
    let mut out = match &spec.grit {
        Some(c) => crunch(c, rng, secs, tau, thump),
        None => vec![0.0; samples(secs)],
    };
    let level = spec.tone_level;
    match spec.tone {
        Tone::None => {}
        Tone::Wood { f0, damp } => wood_tone(&mut out, rng, cue, f0, damp, level),
        Tone::Metal { f, tau } => metal_tone(&mut out, rng, cue, f, tau, level),
        Tone::Crystal { f } => crystal_tone(&mut out, rng, cue, f, level),
        Tone::Glass { f } => glass_tone(&mut out, rng, cue, f, level),
        Tone::Squelch { f } => squelch_tone(&mut out, rng, cue, f, level),
        Tone::Chain { f } => chain_tone(&mut out, rng, cue, f, level),
        Tone::Honey => {
            let secs = match cue {
                BlockCue::Break => 0.28,
                BlockCue::Place => 0.18,
                _ => 0.12,
            };
            let mut body = noise(rng, samples(secs), |t| (1.0 - t / secs).max(0.0).powi(2));
            Biquad::lowpass(650.0, 1.1).run(&mut body);
            for (i, s) in body.iter_mut().enumerate() {
                let t = i as f32 / dsp::RATE;
                *s += (std::f32::consts::TAU * (180.0 * t - 150.0 * t * t)).sin() * (-18.0 * t).exp() * 0.2;
            }
            grow(&mut out, body.len());
            mix_into(&mut out, &body, 1.0, 0);
        }
    }
    // A short fade-in: the first grains and modes start at full strength.
    dsp::fade(&mut out, samples(0.003), 0);
    dsp::finish(out, peak * spec.level)
}

fn grow(out: &mut Vec<f32>, len: usize) {
    if out.len() < len {
        out.resize(len, 0.0);
    }
}

/// Number of impacts and their spacing: a break crumbles in several.
fn impacts(cue: BlockCue, rng: &mut Rng) -> Vec<(f32, f32)> {
    match cue {
        BlockCue::Break => vec![(0.0, 1.0), (rng.range(0.04, 0.08), 0.6), (rng.range(0.1, 0.16), 0.35)],
        BlockCue::Place => vec![(0.0, 1.0)],
        _ => vec![(0.0, 0.7)],
    }
}

fn wood_tone(out: &mut Vec<f32>, rng: &mut Rng, cue: BlockCue, f0: f32, damp: f32, level: f32) {
    let damp = if matches!(cue, BlockCue::Break | BlockCue::Place) { damp } else { damp * 0.55 };
    for (t, a) in impacts(cue, rng) {
        let start = samples(t);
        let f = f0 * rng.range(0.92, 1.08);
        for (ratio, amp, tau) in [(1.0, 1.0, 0.07), (2.32, 0.55, 0.045), (3.93, 0.3, 0.028), (5.61, 0.16, 0.016)] {
            grow(out, start + samples(tau * damp * 6.0));
            let m = Mode {
                freq: f * ratio * rng.range(0.97, 1.03),
                amp: amp * a * level,
                tau: tau * damp,
                glide: 1.0,
                glide_tau: 1.0,
            };
            add_mode(out, start, m);
        }
        let mut click = noise(rng, samples(0.012), |t| (-t / 0.0015).exp());
        Biquad::bandpass(f * 8.5, 0.9).run(&mut click);
        mix_into(out, &click, 0.8 * a * level, start);
    }
    if cue == BlockCue::Break {
        let mut sp = crackle(rng, out.len(), (0.5, 2.5), |t| 900.0 * (-t / 0.07).exp());
        Biquad::bandpass(f0 * 8.0, 0.8).run(&mut sp);
        mix_into(out, &sp, 0.6 * level, 0);
    }
}

fn metal_tone(out: &mut Vec<f32>, rng: &mut Rng, cue: BlockCue, f: f32, tau: f32, level: f32) {
    let tau = match cue {
        BlockCue::Break => tau,
        BlockCue::Place => tau * 0.7,
        _ => tau * 0.3,
    };
    for (t, a) in impacts(cue, rng) {
        let start = samples(t);
        let base = f * rng.range(0.96, 1.04);
        grow(out, start + samples(tau * 5.0));
        for (ratio, amp, k) in [(1.0, 1.0, 1.0), (2.76, 0.6, 0.7), (5.4, 0.35, 0.45), (8.93, 0.18, 0.3)] {
            let m = Mode { freq: base * ratio, amp: amp * a * level * 0.6, tau: tau * k, glide: 1.0, glide_tau: 1.0 };
            add_mode(out, start, m);
        }
    }
}

fn crystal_tone(out: &mut Vec<f32>, rng: &mut Rng, cue: BlockCue, f: f32, level: f32) {
    let (count, tau) = match cue {
        BlockCue::Break => (7, 0.5),
        BlockCue::Place => (3, 0.4),
        _ => (2, 0.25),
    };
    let span = if cue == BlockCue::Break { 0.25 } else { 0.05 };
    for i in 0..count {
        let t = if i == 0 { 0.0 } else { rng.range(0.0, span) };
        let start = samples(t);
        grow(out, start + samples(tau * 4.0));
        // Partials near the harmonic series give the bell its clear pitch.
        let base = f * [1.0, 1.189, 1.335, 1.498][rng.next_u32() as usize % 4];
        let a = if i == 0 { 1.0 } else { rng.range(0.3, 0.6) };
        for (ratio, amp, k) in [(1.0, 1.0, 1.0), (2.01, 0.4, 0.6), (3.03, 0.2, 0.4), (4.17, 0.1, 0.25)] {
            let m = Mode { freq: base * ratio, amp: amp * a * level * 0.5, tau: tau * k, glide: 1.0, glide_tau: 1.0 };
            add_mode(out, start, m);
        }
    }
}

fn glass_tone(out: &mut Vec<f32>, rng: &mut Rng, cue: BlockCue, f: f32, level: f32) {
    match cue {
        BlockCue::Break => {
            grow(out, samples(0.7));
            let mut burst = noise(rng, out.len(), |t| (-t / 0.018).exp());
            Biquad::highpass(f * 0.8, 0.7).run(&mut burst);
            mix_into(out, &burst, level, 0);
            for _ in 0..28 {
                let t = (-rng.f32().max(1e-4).ln() * 0.12).min(0.55);
                let a = rng.range(0.15, 0.5) * (-t / 0.35).exp() * level;
                super::sounds::ping(out, samples(t), rng.range(f * 1.2, f * 3.0), a, rng.range(0.015, 0.05));
            }
        }
        _ => {
            let tau = if cue == BlockCue::Place { 0.07 } else { 0.03 };
            grow(out, samples(tau * 5.0));
            let base = f * rng.range(0.9, 1.1);
            for (ratio, amp, k) in [(1.0, 1.0, 1.0), (2.41, 0.5, 0.65), (4.13, 0.25, 0.35)] {
                add_mode(
                    out,
                    0,
                    Mode { freq: base * ratio, amp: amp * level, tau: tau * k, glide: 1.0, glide_tau: 1.0 },
                );
            }
        }
    }
}

fn squelch_tone(out: &mut Vec<f32>, rng: &mut Rng, cue: BlockCue, f: f32, level: f32) {
    let secs = match cue {
        BlockCue::Break => 0.32,
        BlockCue::Place => 0.2,
        _ => 0.14,
    };
    grow(out, samples(secs));
    let mut body = noise(rng, samples(secs), |t| dsp::ad(t, 0.006, secs * 0.3));
    Biquad::lowpass(f * 2.2, 1.2).run(&mut body);
    mix_into(out, &body, 0.8 * level, 0);
    // A bubble that sinks as the material closes over it.
    let bubbles = if cue == BlockCue::Break { 3 } else { 1 };
    for i in 0..bubbles {
        let t = if i == 0 { 0.01 } else { rng.range(0.03, secs * 0.6) };
        let m = Mode { freq: f * rng.range(0.9, 1.15), amp: 0.5 * level, tau: 0.04, glide: 0.55, glide_tau: 0.05 };
        add_mode(out, samples(t), m);
    }
}

fn chain_tone(out: &mut Vec<f32>, rng: &mut Rng, cue: BlockCue, f: f32, level: f32) {
    let (count, span) = match cue {
        BlockCue::Break => (9, 0.25),
        BlockCue::Place => (5, 0.1),
        _ => (3, 0.06),
    };
    grow(out, samples(span + 0.12));
    for i in 0..count {
        let t = if i == 0 { 0.0 } else { rng.range(0.0, span) };
        let base = f * rng.range(0.8, 1.25);
        let a = if i == 0 { 1.0 } else { rng.range(0.3, 0.8) } * level;
        for (ratio, amp, tau) in [(1.0, 1.0, 0.03), (2.7, 0.5, 0.018), (4.9, 0.25, 0.01)] {
            add_mode(out, samples(t), Mode { freq: base * ratio, amp: amp * a * 0.5, tau, glide: 1.0, glide_tau: 1.0 });
        }
    }
}

/// Container lids: a chest's hinge creak and lid, an ender chest's deep
/// stone lid and shimmer, a barrel's wooden lid, a copper chest's metal
/// hinge (lower and rougher as it ages).
pub fn render_container(cue: ContainerCue, rng: &mut Rng) -> Vec<f32> {
    use ContainerCue as C;
    let open = cue.is_open();
    let mut out = vec![0.0; samples(if open { 0.55 } else { 0.35 })];
    let (creak_f, knock_f, metal) = match cue {
        C::ChestOpen | C::ChestClose => (rng.range(80.0, 100.0), 260.0, 0.0),
        C::BarrelOpen | C::BarrelClose => (rng.range(60.0, 75.0), 200.0, 0.0),
        C::EnderChestOpen | C::EnderChestClose => (rng.range(45.0, 55.0), 150.0, 0.0),
        C::CopperChestOpen | C::CopperChestClose => (rng.range(110.0, 130.0), 300.0, 1.0),
        C::CopperChestWeatheredOpen | C::CopperChestWeatheredClose => (rng.range(95.0, 110.0), 270.0, 0.8),
        C::CopperChestOxidizedOpen | C::CopperChestOxidizedClose => (rng.range(80.0, 95.0), 240.0, 0.6),
    };
    if open {
        // The hinge: stick-slip pulses speeding up as the lid swings.
        let mut creak = vec![0.0; out.len()];
        let end = 0.4;
        let mut t = 0.02;
        while t < end {
            creak[samples(t)] = (1.0 - t / end) * rng.range(0.6, 1.0);
            t += rng.range(0.9, 1.1) / (creak_f * (1.0 + 1.2 * t / end));
        }
        let band = if metal > 0.0 { 2400.0 } else { 900.0 };
        Biquad::bandpass(band * rng.range(0.9, 1.1), 5.0).run(&mut creak);
        mix_into(&mut out, &creak, 3.5, 0);
    }
    // The lid meeting the body (soft when opening, firm when closing).
    let at = samples(if open { 0.01 } else { 0.005 });
    let amp = if open { 0.35 } else { 1.0 };
    for (ratio, a, tau) in [(1.0, 1.0, 0.08), (2.3, 0.45, 0.04), (3.9, 0.2, 0.02)] {
        add_mode(
            &mut out,
            at,
            Mode { freq: knock_f * ratio * rng.range(0.97, 1.03), amp: amp * a, tau, glide: 0.95, glide_tau: 0.05 },
        );
    }
    if metal > 0.0 {
        add_mode(&mut out, at, Mode { freq: 1300.0 * metal, amp: amp * 0.3, tau: 0.12, glide: 1.0, glide_tau: 1.0 });
    }
    if matches!(cue, C::EnderChestOpen | C::EnderChestClose) {
        // The ender chest's shimmer: a soft rising or falling whoosh.
        let len = out.len();
        let mut air = noise(rng, len, |t| dsp::ad(t, 0.05, 0.15));
        let mut lp = super::dsp::OnePole::new(800.0);
        for (i, s) in air.iter_mut().enumerate() {
            let t = i as f32 / dsp::RATE;
            let fc = if open { 400.0 + 2400.0 * t } else { 2600.0 - 2200.0 * t };
            lp.a = super::dsp::OnePole::coef(fc.max(200.0), dsp::RATE);
            *s = lp.process(*s);
        }
        mix_into(&mut out, &air, 0.8, 0);
    }
    dsp::finish(out, 0.55)
}
