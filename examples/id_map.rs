//! Cold-path ID map: which block states, non-block item ids and block texture
//! layers are registered or free, and how long each append-only ordering is.
//! Saved worlds store these numbers, so concurrent content work is handed
//! disjoint ranges from this map; compare it with the reservation tables in
//! docs/architecture.md before reserving anything. Prints one JSON object.
//!
//!   cargo run --release --no-default-features --example id_map
//!   cargo run --release --no-default-features --example id_map -- states:16000-16383 items:1550-1649
//!
//! Each `states:`, `items:` or `layers:` argument (an inclusive range) adds a
//! `queries` entry, and the run exits with status 1 if a queried range holds
//! a registered id: a gate for "is this pool still free?".
//!
//! A free id is only unregistered here. Whether it is also unreserved is
//! recorded in the documents, and pending branches are not visible to a build.
//! Orderings of the client binary (`src/audio`, `src/app`) and of private
//! modules can't be linked from an example, so their lengths are read from the
//! source text and marked `"from": "source text"`.
use std::collections::BTreeSet;

use serde_json::{Value, json};
use voxelcraft::{
    enchant::Enchantment,
    entity::{self, MobKind},
    item::{self, Item},
    potion::Potion,
    simulation::effects::Effect,
    world::{
        self,
        block::{Block, INFO, RenderKind, STATE_CAPACITY, tex},
        terrain::Generation,
    },
};

type Run = (u32, u32);

/// The ids of `first..end` for which `member` holds, as inclusive runs.
fn runs(first: u32, end: u32, member: impl Fn(u32) -> bool) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    for id in (first..end).filter(|&id| member(id)) {
        match out.last_mut() {
            Some(run) if run.1 + 1 == id => run.1 = id,
            _ => out.push((id, id)),
        }
    }
    out
}

fn len(run: &Run) -> u32 {
    run.1 - run.0 + 1
}

fn total(runs: &[Run]) -> u32 {
    runs.iter().map(len).sum()
}

/// Each run as `"first-last"`.
fn text(runs: &[Run]) -> Vec<String> {
    runs.iter().map(|(a, b)| format!("{a}-{b}")).collect()
}

/// The number that follows `before` in `source`, such as an array's length.
fn declared(source: &str, before: &str) -> Option<usize> {
    let rest = source.split_once(before)?.1;
    rest[..rest.find(|c: char| !c.is_ascii_digit())?].parse().ok()
}

/// How many variants the rustfmt-formatted `pub enum name` in `source` has.
fn variants(source: &str, name: &str) -> Option<usize> {
    let body = source.split_once(&format!("pub enum {name} {{\n"))?.1.split_once("\n}")?.0;
    let variant =
        |line: &str| line.strip_prefix("    ").is_some_and(|v| v.starts_with(|c: char| c.is_ascii_uppercase()));
    Some(body.lines().filter(|line| variant(line)).count())
}

/// The quoted entries of `DIMENSION_KEYS` in `src/app/dimension.rs`.
fn dimension_keys(source: &str) -> Vec<&str> {
    let body = source.split_once("DIMENSION_KEYS: [&str; ").and_then(|(_, rest)| rest.split_once("];"));
    body.map_or_else(Vec::new, |(body, _)| body.split('"').skip(1).step_by(2).collect())
}

fn ordering(name: &str, count: Value, from: &str, stored_by: &str, note: String) -> Value {
    json!({"name": name, "count": count, "from": from, "stored_by": stored_by, "note": note})
}

/// The append-only orderings: where position is identity. `stored_by` says
/// whether saves and the LAN protocol carry the name or the number.
fn orderings() -> Vec<Value> {
    let (linked, scanned) = ("registry", "source text");
    let voices = include_str!("../src/audio/voices.rs");
    let sounds = include_str!("../src/audio/sounds.rs");
    let dimension = include_str!("../src/app/dimension.rs");
    let keys = dimension_keys(dimension);
    let enchant_bits = Enchantment::SAVE_SLOTS * 3;
    let (first_partner, partners) = (world::waterlog::FIRST, world::waterlog::COUNT);
    vec![
        ordering(
            "entity::MobKind::ALL",
            json!(MobKind::ALL.len()),
            linked,
            "saves: name (MobKind::name; spawners and commands too). LAN: number (position in ALL, MobKind::lan_index)",
            "Append to the enum and to ALL in the same place. Mob tests pin LAN indices.".into(),
        ),
        ordering(
            "simulation::effects::Effect::ALL",
            json!(Effect::ALL.len()),
            linked,
            "saves and LAN player state: Java id string. In memory: bit number in the u32 Effects::mask and \
             Target::effects, and the icon layer after the item icons",
            format!(
                "{} of 32 mask bits are left; widen the mask before a 33rd effect. Suspicious stew saves its own \
                 codes (entity::mooshroom), not this order.",
                32 - Effect::ALL.len()
            ),
        ),
        ordering(
            "enchant::Enchantment",
            json!(Enchantment::COUNT),
            linked,
            "saves and LAN stacks: number (level in 3 bits at bit 3 * index of a u128, written as hex)",
            format!(
                "{enchant_bits} of 128 bits used, so {} more 3-bit slots fit; another enchantment needs a wider \
                 encoding first. SAVE_SLOTS masks what from_hex keeps.",
                (128 - enchant_bits) / 3
            ),
        ),
        ordering(
            "potion::POTIONS",
            json!(Potion::COUNT),
            linked,
            "saves and LAN: number. Drinkable and splash potions are item ids; tipped arrows, lingering potions, \
             potion clouds and thrown potions carry the index (stacks as index + 1)",
            "Item::potion and Item::splash_potion place index 39 and later at 1531+ and 1533+, two ids each, so a \
             42nd potion needs new item ids and mapping arms (item.rs asserts the count). Recipes and loot use Java ids."
                .into(),
        ),
        ordering(
            "entity::thrown::Kind",
            json!(variants(include_str!("../src/entity/thrown.rs"), "Kind")),
            scanned,
            "saves: not saved. LAN: number (kind as u8)",
            "entity/replica.rs decodes the number with a match whose default arm is llama spit: add each new kind there."
                .into(),
        ),
        ordering(
            "particles::Kind::ALL",
            json!(voxelcraft::particles::Kind::ALL.len()),
            linked,
            "saves: not saved. LAN: number (position in ALL)",
            "ALL must follow declaration order (tested).".into(),
        ),
        json!({
            "name": "sound enums",
            "count": variants(sounds, "Sound"),
            "from": scanned,
            "stored_by": "saves: not saved (volume options use SoundSource::option_key names). LAN: number (Sound::key \
                          and SoundSource::index)",
            "note": "count is the variants of audio::sounds::Sound. Its key is a dense index over the parts below, so a \
                     new Material, Voice or Instrument shifts later keys; harmless while keys are never saved and LAN \
                     needs the same build at both ends. Append cue variants at the end of their enum.",
            "parts": {
                "audio::sounds::Material::ALL (source text)": declared(sounds, "const ALL: [Material; "),
                "audio::voices::Voice::ALL (source text)": declared(voices, "const ALL: [Voice; "),
                "audio::voices::Call::ALL (source text)": declared(voices, "const ALL: [Call; "),
                "audio::source::SoundSource::COUNT (source text)":
                    declared(include_str!("../src/audio/source.rs"), "const COUNT: usize = "),
                "world::gadgets::Instrument::ALL": world::gadgets::Instrument::ALL.len(),
                "entity::copper_golem::Sound (source text)":
                    variants(include_str!("../src/entity/copper_golem.rs"), "Sound"),
                "entity::allay::Sound (source text)": variants(include_str!("../src/entity/allay.rs"), "Sound"),
                "entity::frog::FrogSound::ALL": entity::frog::FrogSound::ALL.len(),
                "entity::sniffer::Cue::ALL": entity::sniffer::Cue::ALL.len(),
                "entity::warden::Cue::ALL": entity::warden::Cue::ALL.len(),
                "world::sculk::SculkCue::ALL": world::sculk::SculkCue::ALL.len(),
                "weapons::Cue::ALL": voxelcraft::weapons::Cue::ALL.len(),
                "entity::breeze::Cue::ALL": entity::breeze::Cue::ALL.len(),
            },
        }),
        json!({
            "name": "app::dimension::DIMENSION_KEYS",
            "count": declared(dimension, "DIMENSION_KEYS: [&str; "),
            "from": scanned,
            "stored_by": "saves: name (level.txt key=value lines). LAN: not sent",
            "note": "Order is free. A per-dimension property missing from this list is saved with the player's global \
                     properties instead of its dimension.",
            "keys": keys,
        }),
        ordering(
            "entity::villager::Profession::ALL",
            json!(entity::villager::Profession::ALL.len()),
            linked,
            "saves: number (the villager's profession; it also seeds trade rolls). LAN: number",
            "Append only.".into(),
        ),
        ordering(
            "entity::armor::ArmorKind::ALL",
            json!(entity::armor::ArmorKind::ALL.len()),
            linked,
            "saves: number (mob armor slots). LAN: number (mob look)",
            "Explicit discriminants; append only.".into(),
        ),
        ordering(
            "item::Tier::ALL",
            json!(item::Tier::ALL.len()),
            linked,
            "saves and LAN: number, through item ids (320 + tier * 5 + kind for the first five tiers, 345 + material * 4 \
             + piece for the first four armor materials)",
            "Never reorder Tier, ToolKind, ArmorMaterial or ArmorPiece; later tiers and materials take explicit ids."
                .into(),
        ),
        ordering(
            "world::terrain::Generation (latest number)",
            json!(Generation::LATEST.number()),
            linked,
            "saves: number (worldgen= in level.txt). LAN: number (Welcome.generation)",
            "A number a build doesn't know loads as its latest generation, so one change alone may claim the next number."
                .into(),
        ),
        ordering(
            "world::slab_stair_states::FAMILIES",
            json!(world::slab_stair_states::FAMILY_COUNT),
            linked,
            "neither: in-memory packing, seven bits",
            format!(
                "{} families are left before the packing must widen. The family constants chain, so concurrent appends \
                 conflict in the text.",
                128 - world::slab_stair_states::FAMILY_COUNT
            ),
        ),
        ordering(
            "world::waterlog::RUNS (partner states)",
            json!(partners),
            linked,
            "saves and LAN: number (a partner's state id is its position in RUNS order)",
            format!(
                "Positional: a run appended to RUNS takes the next ids, from {}. Only states up to 2415 can join; newer \
                 blocks keep their wet states in their own range.",
                first_partner + partners
            ),
        ),
        ordering(
            "world::utilities::POTTED",
            json!(world::utilities::POTTED.len()),
            linked,
            "saves and LAN: number (a potted plant's state id and texture layer are its position)",
            "Closed: the layer after the last entry is the potted cactus top. Later potted plants have their own states."
                .into(),
        ),
        ordering(
            "item icons (Item::all_items order)",
            json!(item::icon_count()),
            linked,
            "neither: positions in the item texture array, rebuilt at every start",
            format!(
                "Icons sit at layer {} + index, then {} animated frames and one icon per effect. all_items, \
                 sprite_index and icon_count hold matching offsets, so concurrent appends conflict in the text.",
                tex::ITEM_BASE,
                item::animated_icons()
            ),
        ),
        ordering(
            "lan::PROTOCOL",
            json!(voxelcraft::lan::PROTOCOL),
            linked,
            "LAN: number, with the game version; the handshake refuses a different protocol or version",
            "Numbers sent over LAN only need to agree within one build.".into(),
        ),
    ]
}

/// `states:16000-16383`, `items:1550-1649` or `layers:3968-4031`: the
/// registered ids inside the range.
fn query(arg: &str, spaces: &[(&str, &dyn Fn(u32) -> bool)]) -> Option<Value> {
    let (space, range) = arg.split_once(':')?;
    let (first, last) = range.split_once('-')?;
    let (first, last) = (first.parse::<u32>().ok()?, last.parse::<u32>().ok()?);
    let member = spaces.iter().find(|(name, _)| *name == space)?.1;
    let found = runs(first, last.checked_add(1)?.min(1 << 16), member);
    Some(json!({"range": arg, "registered": total(&found), "registered_runs": text(&found)}))
}

fn main() {
    // A state the registry defines: anything drawn, plus the named
    // invisible ones (air and the moving piston).
    let defined: Vec<bool> = INFO.iter().map(|i| i.kind != RenderKind::Invisible || i.name != "unknown").collect();
    let state = |id: u32| defined.get(id as usize).copied().unwrap_or(false);
    let capacity = STATE_CAPACITY as u32;
    let (used, free) = (runs(0, capacity, state), runs(0, capacity, |id| !state(id)));
    let long: Vec<Run> = free.iter().copied().filter(|run| len(run) >= 8).collect();
    // Named but invisible (`true`), or drawn under the name "unknown".
    let odd = |named: bool| -> Vec<usize> {
        let fits = |info: &voxelcraft::world::block::BlockInfo| {
            (info.kind == RenderKind::Invisible) == named && (info.name != "unknown") == named
        };
        (0..STATE_CAPACITY).filter(|&id| fits(&INFO[id])).collect()
    };

    // Texture layers the faces of registered states name.
    let layers: BTreeSet<u32> =
        INFO.iter().zip(&defined).filter(|(_, used)| **used).flat_map(|(info, _)| info.tex).map(u32::from).collect();
    let layer = |l: u32| layers.contains(&l);
    let (referenced, unreferenced) = (runs(0, tex::COUNT, layer), runs(0, tex::COUNT, |l| !layer(l)));
    let beyond: Vec<u32> = layers.iter().copied().filter(|&l| l >= tex::COUNT).collect();

    // Non-block items: every id between the low block items and the
    // extended block items that names something.
    let (first, base) = (u32::from(item::FIRST_ITEM), u32::from(item::BLOCK_ITEM_BASE));
    let valid = |id: u32| u16::try_from(id).is_ok_and(|id| Item(id).is_valid());
    let (items, free_items) = (runs(first, base, valid), runs(first, base, |id| !valid(id)));
    let listed: Vec<u32> = Item::all_items().map(|i| u32::from(i.0)).collect();
    let mut seen = BTreeSet::new();
    let twice: Vec<u32> = listed.iter().copied().filter(|&id| !seen.insert(id)).collect();
    let unlisted: Vec<u32> = (first..base).filter(|&id| valid(id) && !seen.contains(&id)).collect();
    let stray: Vec<u32> = seen.iter().copied().filter(|&id| !(first..base).contains(&id) || !valid(id)).collect();
    let placed_by: BTreeSet<u32> = (0..capacity)
        .filter(|&id| state(id))
        .map(|id| u32::from(Item::from_block(Block(id as u16)).0))
        .filter(|id| (first..base).contains(id))
        .collect();

    let spaces: [(&str, &dyn Fn(u32) -> bool); 3] = [("states", &state), ("items", &valid), ("layers", &layer)];
    let mut queries = Vec::new();
    for arg in std::env::args().skip(1) {
        match query(&arg, &spaces) {
            Some(q) => queries.push(q),
            None => {
                eprintln!("id_map: expected states:A-B, items:A-B or layers:A-B, got {arg}");
                std::process::exit(2);
            }
        }
    }
    let taken = queries.iter().any(|q| q["registered"] != 0);

    let map = json!({
        "version": env!("CARGO_PKG_VERSION"),
        "block_states": {
            "capacity": STATE_CAPACITY,
            "capacity_limit": "32768 is the most the encodings hold: block items are BLOCK_ITEM_BASE (0x8000) + state \
                               in a u16, and waterlog pairs keep the partner state in 15 bits",
            "stored_by": "saves: number (u16 in chunk runs, under the VXC2 header, VXC3 from state 4096 and VXC4 from \
                          16384; falling blocks too). LAN: number (chunk and edit packets, falling blocks, break effects)",
            "registered": total(&used),
            "highest_registered": used.last().map(|run| run.1),
            "registered_runs": text(&used),
            "free_total": total(&free),
            "free_runs_of_8_or_more": text(&long),
            "free_in_shorter_gaps": total(&free) - total(&long),
            "named_but_invisible": odd(true),
            "drawn_but_unnamed": odd(false),
        },
        "item_ids": {
            "first": first,
            "limit": "non-block ids must stay below BLOCK_ITEM_BASE (32768); from there up an id is 0x8000 + block state",
            "stored_by": "saves and LAN: number (the decimal id that starts every stack string). An id a build doesn't \
                          know loads as an empty slot",
            "registered": total(&items),
            "highest_registered": items.last().map(|run| run.1),
            "registered_runs": text(&items),
            "free_total": total(&free_items),
            "free_runs": text(&free_items),
            "valid_but_not_in_all_items": unlisted,
            "all_items_twice": twice,
            "all_items_invalid_or_out_of_range": stray,
            "placed_by_block_states": placed_by,
        },
        "texture_layers": {
            "count": tex::COUNT,
            "capacity": tex::CAPACITY,
            "capacity_limit": "twelve layer bits in a quad; item icons start at ITEM_BASE (4096), a literal in the shaders",
            "stored_by": "neither saves nor LAN: layers may be repacked when branches merge",
            "referenced_by_registered_blocks": layers.len(),
            "referenced_runs": text(&referenced),
            "unreferenced_runs_below_count": text(&unreferenced),
            "unreferenced_below_count": total(&unreferenced),
            "referenced_at_or_above_count": beyond,
            "free_above_count": (tex::COUNT < tex::CAPACITY).then(|| format!("{}-{}", tex::COUNT, tex::CAPACITY - 1)),
            "note": "Unreferenced is not free: the HUD, crack and fire frames, biome tints, entity and block-entity \
                     models and map sprites use layers no block face names. Only layers from count up are certainly unused.",
        },
        "orderings": orderings(),
        "queries": queries,
    });
    println!("{}", serde_json::to_string_pretty(&map).unwrap());
    if taken {
        std::process::exit(1);
    }
}
