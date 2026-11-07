use super::*;
use crate::world::terrain::Generator;
use std::sync::Arc;

fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(42)), Default::default(), 2);
    w.load_test_column(glam::IVec2::ZERO, 0);
    w
}

const P: IVec3 = IVec3::new(3, 10, 3);

#[test]
fn songs_match_java_26_3_jukebox_song_data() {
    // `data/minecraft/jukebox_song/*.json`: (id, length_in_seconds, comparator_output).
    let java = [
        ("11", 71.0, 11),
        ("13", 178.0, 1),
        ("5", 178.0, 15),
        ("blocks", 345.0, 3),
        ("bounce", 234.0, 8),
        ("cat", 185.0, 2),
        ("chirp", 185.0, 4),
        ("creator", 176.0, 12),
        ("creator_music_box", 73.0, 11),
        ("far", 174.0, 5),
        ("lava_chicken", 134.0, 9),
        ("mall", 197.0, 6),
        ("mellohi", 96.0, 7),
        ("otherside", 195.0, 14),
        ("pigstep", 149.0, 13),
        ("precipice", 299.0, 13),
        ("relic", 218.0, 14),
        ("stal", 150.0, 8),
        ("strad", 188.0, 9),
        ("tears", 175.0, 10),
        ("wait", 238.0, 12),
        ("ward", 251.0, 10),
    ];
    assert_eq!(java.len(), SONGS.len());
    for (id, seconds, comparator) in java {
        let disc = disc_named(&format!("minecraft:music_disc_{id}")).unwrap_or_else(|| panic!("{id}"));
        let (_, song) = song_of(disc).unwrap();
        assert_eq!((song.seconds, song.comparator), (seconds, comparator), "{id}");
        assert_eq!(Item::from_name(&format!("music_disc_{id}")), Some(disc), "{id}");
        assert_eq!(disc.max_stack(), 1);
    }
    // `lengthInTicks` is `ceil(seconds * 20)`; a song has finished 20 ticks after it.
    let (_, eleven) = song_of(disc_named("music_disc_11").unwrap()).unwrap();
    assert_eq!(eleven.length_ticks(), 1420);
    assert!(!eleven.finished(1439) && eleven.finished(1440));
    // Java's rare discs, and the ids of the other track B items.
    for (item, rare) in [(DISC_OTHERSIDE, true), (disc_named("music_disc_pigstep").unwrap(), true), (DISC_5, false)] {
        assert_eq!(item.is_rare(), rare);
    }
    assert_eq!(Item::from_name("echo_shard"), Some(ECHO_SHARD));
    assert_eq!(Item::from_name("recovery_compass"), Some(RECOVERY_COMPASS));
    assert_eq!(Item::from_name("disc_fragment_5"), Some(DISC_FRAGMENT_5));
    assert_eq!(
        (ECHO_SHARD.0, RECOVERY_COMPASS.0, DISC_FRAGMENT_5.0, DISC_FIRST, ITEM_END),
        (1430, 1431, 1432, 1433, 1455)
    );
    assert_eq!(DISC_5, disc_named("music_disc_5").unwrap());
    for item in items() {
        assert!(item.is_valid() && item.icon_layer().is_some(), "{}", item.name());
        assert!(Item::creative_palette().any(|i| i == item));
    }
}

#[test]
fn blocks_match_java_properties() {
    assert_eq!(Block::from_name("jukebox"), Some(JUKEBOX));
    assert_eq!(Block::from_name("reinforced_deepslate"), Some(REINFORCED_DEEPSLATE));
    assert_eq!((JUKEBOX.hardness(), JUKEBOX.blast_resistance()), (2.0, 6.0));
    assert_eq!((REINFORCED_DEEPSLATE.hardness(), REINFORCED_DEEPSLATE.blast_resistance()), (55.0, 1200.0));
    assert_eq!(JUKEBOX_WITH_RECORD.drop(), Some(Item::from_block(JUKEBOX)));
    assert_eq!(REINFORCED_DEEPSLATE.drop(), None);
    assert_eq!(JUKEBOX.best_tool(), Some(crate::item::ToolKind::Axe));
    assert!(JUKEBOX.ignited_by_lava() && !REINFORCED_DEEPSLATE.ignited_by_lava());
    assert!(JUKEBOX.is_opaque() && REINFORCED_DEEPSLATE.is_opaque());
    assert!(piston_immovable(REINFORCED_DEEPSLATE) && piston_immovable(JUKEBOX_WITH_RECORD));
    assert!(Block::creative_palette().any(|b| b == JUKEBOX));
    assert!(Block::creative_palette().any(|b| b == REINFORCED_DEEPSLATE));
    assert!(!Block::creative_palette().any(|b| b == JUKEBOX_WITH_RECORD));
    for layer in t::BASE..t::END {
        assert!((layer as u32) < crate::world::block::tex::COUNT);
        assert_eq!(pixel(layer, 3, 3)[3], 255);
    }
}

#[test]
fn a_record_plays_for_its_length_with_signal_events_and_notes() {
    let mut w = world();
    w.set_block(P, JUKEBOX);
    let disc = disc_named("music_disc_11").unwrap();
    assert!(!w.insert_disc(P, Stack::new(Item::STICK, 1)));
    assert!(w.insert_disc(P, Stack::new(disc, 1)));
    assert!(!w.insert_disc(P, Stack::new(disc, 1)), "already holds a record");
    assert_eq!(w.get_block(P), Some(JUKEBOX_WITH_RECORD));
    assert_eq!(w.records_started, vec![(P, 14)]);
    assert_eq!(w.redstone_power(P + IVec3::X), 15);
    assert_eq!(w.container_signal(P), Some(11));
    let mut plays = 0;
    let mut notes = 0;
    for tick in 0..1440u64 {
        w.tick_jukeboxes();
        assert!(w.jukebox_playing(P), "tick {tick}");
        for (p, e) in std::mem::take(&mut w.jukebox_events) {
            assert_eq!((p, e), (P, JukeboxEvent::Play));
            assert!(tick.is_multiple_of(20));
            plays += 1;
        }
        notes += w.records.len();
        assert!(w.records.iter().all(|n| n.pos == P && n.pitch <= 24 && n.instrument < 16));
        assert!(tick < 1420 || w.records.is_empty(), "silence after the song's length");
        w.records.clear();
    }
    assert_eq!(plays, 72);
    assert!(notes > 100, "the tune plays: {notes} notes");
    w.tick_jukeboxes();
    assert!(!w.jukebox_playing(P));
    assert_eq!(w.jukebox_events, vec![(P, JukeboxEvent::Stop)]);
    // The record stays; the signal stops; the comparator still reads it.
    assert_eq!(w.get_block(P), Some(JUKEBOX_WITH_RECORD));
    assert_eq!(w.redstone_power(P + IVec3::X), 0);
    assert_eq!(w.container_signal(P), Some(11));
    assert_eq!(w.eject_disc(P).map(|s| s.item), Some(disc));
    assert_eq!(w.container_signal(P), Some(0));
    assert_eq!(w.get_block(P), Some(JUKEBOX));
}

#[test]
fn jukeboxes_save_and_reload_mid_song_and_breaking_pops_the_record() {
    let mut w = world();
    w.set_block(P, JUKEBOX);
    let disc = disc_named("music_disc_cat").unwrap();
    let mut named = Stack::new(disc, 1);
    named.name = crate::inventory::StackName::new("Kitty").unwrap();
    assert!(w.insert_disc(P, named));
    for _ in 0..300 {
        w.tick_jukeboxes();
    }
    let saved = w.jukeboxes_to_string();
    let mut loaded = world();
    loaded.set_block(P, JUKEBOX_WITH_RECORD);
    loaded.load_jukeboxes(&saved);
    assert_eq!(loaded.jukebox(P), w.jukebox(P));
    assert_eq!(loaded.jukebox(P).unwrap().playing, Some(300));
    assert_eq!(loaded.jukebox(P).unwrap().record, Some(named));
    assert_eq!(loaded.jukeboxes_to_string(), saved);
    // The same notes continue from where they were.
    w.records.clear();
    w.tick_jukeboxes();
    loaded.tick_jukeboxes();
    assert_eq!(w.records, loaded.records);
    // A song saved after it had finished doesn't resume, but keeps its record.
    let done = format!("[[{},{},{}],\"{}:1\",{}]", P.x, P.y, P.z, disc.0, 185 * 20 + 20);
    let mut late = world();
    late.load_jukeboxes(&format!("[{done}]"));
    assert_eq!(late.jukebox(P).unwrap().playing, None);
    assert_eq!(late.jukebox(P).unwrap().record.map(|s| s.item), Some(disc));
    // Breaking the jukebox drops it and its record and stops the song.
    w.jukebox_events.clear();
    w.drops.clear();
    w.set_block(P, Block::AIR);
    assert!(w.drops.iter().any(|(_, s)| s.item == disc && s.name.as_str() == Some("Kitty")));
    assert_eq!(w.jukebox_events, vec![(P, JukeboxEvent::Stop)]);
    assert!(w.jukebox(P).is_none());
    // Empty jukeboxes aren't saved.
    w.set_block(P, JUKEBOX);
    assert_eq!(w.jukeboxes_to_string(), "[]");
}

#[test]
fn every_tune_is_original_deterministic_and_in_range() {
    for (i, song) in SONGS.iter().enumerate() {
        let a: Vec<_> = (0..song.length_ticks()).flat_map(|t| tune(i, t)).collect();
        let b: Vec<_> = (0..song.length_ticks()).flat_map(|t| tune(i, t)).collect();
        assert_eq!(a, b);
        assert!(a.len() as u64 > song.length_ticks() / 20, "{}: {} notes", song.id, a.len());
        assert!(a.iter().all(|n| n.pitch <= 24 && n.gain > 0.0 && n.gain <= 1.0));
    }
    // Discs don't share a score.
    let first: Vec<Vec<_>> = (0..SONGS.len()).map(|i| (0..400).flat_map(|t| tune(i, t)).collect()).collect();
    for i in 0..first.len() {
        for j in i + 1..first.len() {
            assert_ne!(first[i], first[j], "{} and {}", SONGS[i].id, SONGS[j].id);
        }
    }
}

#[test]
fn ancient_city_chests_roll_their_java_tables() {
    use crate::world::loot::table;
    let mut seen = rustc_hash::FxHashSet::default();
    let mut swift = false;
    for seed in 0..400 {
        let chest = table(seed, "ancient_city");
        for s in chest.slots.iter().flatten() {
            assert!(s.item.is_valid() && s.count <= s.item.max_stack());
            seen.insert(s.item);
            swift |= s.item == Item::ENCHANTED_BOOK && s.enchants.level(crate::enchant::Enchantment::SwiftSneak) > 0;
        }
        for s in table(seed, "ancient_city_ice_box").slots.iter().flatten() {
            assert!(
                [Item::SUSPICIOUS_STEW, Item::GOLDEN_CARROT, Item::BAKED_POTATO, Item::SNOWBALL].contains(&s.item)
                    || s.item == Item::from_block(crate::world::gadgets::PACKED_ICE),
                "{}",
                s.item.name()
            );
        }
    }
    for item in [ECHO_SHARD, DISC_FRAGMENT_5, DISC_OTHERSIDE, disc_named("music_disc_13").unwrap()] {
        assert!(seen.contains(&item), "{}", item.name());
    }
    assert!(seen.contains(&Item::from_block(crate::world::cave_blocks::SCULK_CATALYST)));
    assert!(swift, "swift sneak books");
}
