//! v0.18 Track 1: the shared font (`voxelcraft::font`) against Java 26.3's
//! default font metrics (`fixtures/java/26.3/font_metrics.json`, from
//! `tools/extract_java_font_fixtures.py`), and accented text through signs,
//! saves, the LAN copy and books.
use std::sync::Arc;

use glam::{DVec3, IVec3};
use serde_json::Value;
use voxelcraft::{
    color::DyeColor,
    content::books,
    font,
    inventory::{Stack, stack_from_str, stack_to_string},
    world::{
        World,
        block::Block,
        chunk::ChunkData,
        storage::{LevelInfo, Storage},
        terrain::Generator,
        wood::{self, signs},
    },
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/font_metrics.json")).unwrap()
}

#[test]
fn every_glyph_advances_as_javas_default_font() {
    let java = fixture();
    let mut checked = 0;
    for (code, glyph) in java["glyphs"].as_object().unwrap() {
        let c = char::from_u32(u32::from_str_radix(code, 16).unwrap()).unwrap();
        assert!(font::covers(c), "{c:?} ({code}) has a glyph");
        // Two characters come from Java's Unifont fallback, which isn't in
        // the jar: the no-break space (a space here) and the cedilla.
        let Some(advance) = glyph["advance"].as_i64() else { continue };
        assert_eq!(font::advance(c) as i64, advance, "{c:?} ({code}), Java's {}", glyph["provider"]);
        checked += 1;
    }
    assert_eq!(checked, 317, "ASCII, Latin-1 and Latin Extended-A less the two Unifont characters");
    assert_eq!(font::advance('\u{a0}'), font::advance(' '));
}

#[test]
fn line_metrics_are_javas() {
    let java = fixture();
    assert_eq!(java["line_height"], font::LINE_HEIGHT);
    assert_eq!(java["shadow_offset"], font::SHADOW_OFFSET);
    // Java's accented glyphs are 12 rows with ascent 10: 3 rows above the line.
    let accented = java["glyphs"]["00C9"].clone();
    assert_eq!((accented["height"].as_i64(), accented["ascent"].as_i64()), (Some(12), Some(10)));
    assert_eq!(font::TOP, 7 - 10);
    assert_eq!(font::ROWS as i64, 12);
    for (hanging, key) in [(false, "sign"), (true, "hanging_sign")] {
        let mount = if hanging {
            signs::Mount::Hanging { rotation: 0, attached: false }
        } else {
            signs::Mount::Standing { rotation: 0 }
        };
        let sign = signs::Sign { kind: wood::Kind::Oak, mount, wet: false };
        assert_eq!(java[key]["max_line_width"], sign.max_line_width(), "{key}");
        assert_eq!(java[key]["line_height"], sign.line_height(), "{key}");
    }
}

const AT: IVec3 = IVec3::new(8, 145, 8);

fn world_with_sign() -> World {
    let mut w = World::new_remote(Arc::new(Generator::new(7)), 2);
    w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    w.set_block(AT - IVec3::Y, Block::STONE);
    let sign = signs::Sign { kind: wood::Kind::Birch, mount: signs::Mount::Standing { rotation: 4 }, wet: false };
    w.set_block(AT, sign.block());
    w
}

fn saved_and_loaded(world: &World, name: &str) -> World {
    let dir = std::env::temp_dir().join(format!("voxelcraft-typography-{name}-{}", std::process::id()));
    let storage = Storage::new(&dir);
    let props = [("wood".to_string(), world.wood_to_string())].into_iter().collect();
    storage.save(&LevelInfo { seed: 7, player: None, props }, &world.modified_chunks()).unwrap();
    let mut loaded = World::new_remote(Arc::new(Generator::new(7)), 2);
    for (pos, data) in storage.load_chunks().unwrap() {
        loaded.receive_chunk(pos, data);
    }
    loaded.load_wood(&storage.load_level().unwrap().props["wood"]);
    std::fs::remove_dir_all(dir).unwrap();
    loaded
}

#[test]
fn accented_sign_text_survives_editing_saving_and_the_lan_copy() {
    let mut w = world_with_sign();
    let lines = ["Ærøskøbing".to_string(), "Łódź – Žďár".into(), "Grüße, Façade".into(), "Ŋ ő ŀ ſ ĳ".into()];
    w.set_sign_text(AT, true, lines.clone()).unwrap();
    let text = &w.sign_at(AT).unwrap().1.front.lines;
    // The en dash is outside the font's ranges, so the sign drops it as it
    // drops any character it can't draw.
    assert_eq!(text[1], "Łódź  Žďár");
    let expected = [lines[0].clone(), "Łódź  Žďár".into(), lines[2].clone(), lines[3].clone()];
    assert_eq!(text, &expected);
    let loaded = saved_and_loaded(&w, "sign");
    assert_eq!(loaded.sign_at(AT).unwrap().1.front.lines, expected);
    // A LAN client gets the same text.
    let mut client = world_with_sign();
    client.replace_wood(&w.wood_near(AT.as_dvec3() + DVec3::splat(0.5), 64.0, 1 << 20));
    assert_eq!(client.sign_at(AT).unwrap().1.front.lines, expected);
}

#[test]
fn signs_saved_before_v0_18_load_unchanged() {
    // A pre-v0.18 save entry, as `wood_to_string` wrote it then: the colour
    // index, the glow flag and each line's bytes in hex. Lines 0 and 1
    // filled the old editor's 90 pixels by the old font's widths (11 'W's,
    // 18 'i's).
    let hex = |s: &str| s.bytes().map(|b| format!("{b:02x}")).collect::<String>();
    let old = ["W".repeat(11), "i".repeat(18), "Hello, world!".to_string(), "~{[(<|>)]}`^_".to_string()];
    let side = format!("{}:0:{}", DyeColor::Black as u8, old.iter().map(|l| hex(l)).collect::<Vec<_>>().join("."));
    let back = format!("{}:0:...", DyeColor::Black as u8);
    let entry = format!("s:{},{},{}={side};{back};0", AT.x, AT.y, AT.z);
    let mut w = world_with_sign();
    w.load_wood(&entry);
    let loaded = w.sign_at(AT).expect("the old sign loads").1;
    assert_eq!(loaded.front.lines, old);
    assert!(loaded.back.lines.iter().all(String::is_empty));
    // Saved again in the new build, it reads back the same.
    assert_eq!(saved_and_loaded(&w, "old").sign_at(AT).unwrap().1.front.lines, old);
}

#[test]
fn accented_book_pages_and_titles_keep_their_characters() {
    let pages = vec!["Ça commence ici: «naïve» Ærø, ŁÓDŹ.".to_string(), "Ĳsselmeer ŉ ő ű".to_string()];
    let book = books::edit(Stack::new(books::WRITABLE_BOOK, 1), &pages).unwrap();
    assert_eq!(books::pages(&book), pages);
    let reloaded = stack_from_str(&stack_to_string(Some(book))).unwrap().unwrap();
    assert_eq!(books::pages(&reloaded), pages);
    let signed = books::sign(book, &pages, "Ölçü", "Zoë").unwrap();
    let reloaded = stack_from_str(&stack_to_string(Some(signed))).unwrap().unwrap();
    assert_eq!(books::pages(&reloaded), pages);
    assert_eq!(books::written(&reloaded).unwrap().title, "Ölçü");
    // The book screens wrap pages with the shared measurement.
    let lines = books::wrap(&pages[0], books::PAGE_WIDTH, |s| font::ink_width(s) as f32);
    assert!(lines.len() > 1 && lines.iter().all(|l| font::ink_width(l) as f32 <= books::PAGE_WIDTH));
    assert_eq!(lines.concat().replace(' ', ""), pages[0].replace(' ', ""));
}
