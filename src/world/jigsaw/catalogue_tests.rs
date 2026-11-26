//! `structure_catalogue_tests`: the generation 4 catalogues against Java
//! 26.3's pools, templates and processor lists, extracted from the jar by
//! `tools/extract_java_structure_catalogues.py` into
//! `tests/fixtures/java/26.3/structure_catalogues.json`, and generation 6's
//! bastion catalogue against `tests/fixtures/java/26.3/bastion_catalogue.json`
//! (the same tool's `--set bastion`, v1.0 Track 8). Every check runs both
//! ways: nothing Java has is missing and nothing is added.

use std::collections::BTreeMap;

use serde_json::Value;

use super::super::{Base, Library};
use super::{PieceSpec, parse_pieces};
use crate::world::block::Block;
use crate::world::jigsaw::rules::{self, Step, Test};

fn fixture() -> &'static Value {
    static F: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    F.get_or_init(|| {
        serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/structure_catalogues.json")).unwrap()
    })
}

fn bastion_fixture() -> &'static Value {
    static F: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    F.get_or_init(|| {
        serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/bastion_catalogue.json")).unwrap()
    })
}

/// One catalogue: its pool prefix, file, library and the jar fixture it
/// is checked against.
pub(crate) struct Catalogue {
    pub prefix: &'static str,
    pub text: &'static str,
    pub lib: &'static Library,
    pub fixture: &'static Value,
}

pub(crate) fn catalogues() -> Vec<Catalogue> {
    vec![
        Catalogue {
            prefix: "village/",
            text: include_str!("village_v4.txt"),
            lib: crate::world::village::catalogue::library(),
            fixture: fixture(),
        },
        Catalogue {
            prefix: "trial_chambers/",
            text: include_str!("trial_chambers_v4.txt"),
            lib: crate::world::trial_chambers::catalogue::library(),
            fixture: fixture(),
        },
        Catalogue {
            prefix: "ancient_city/",
            text: include_str!("ancient_city_v4.txt"),
            lib: crate::world::ancient_city::catalogue::library(),
            fixture: fixture(),
        },
        Catalogue {
            prefix: "bastion/",
            text: include_str!("bastion_v6.txt"),
            lib: crate::world::bastion::catalogue::library(),
            fixture: bastion_fixture(),
        },
    ]
}

/// Java's element as the catalogue writes it.
fn java_element(e: &Value) -> String {
    let kind = e["kind"].as_str().unwrap();
    match kind {
        "empty" => "empty".into(),
        "feature" => format!("feature:{}", e["feature"].as_str().unwrap()),
        "list" => {
            let parts = e["elements"].as_array().unwrap();
            let names: Vec<&str> = parts.iter().map(|p| p["location"].as_str().unwrap()).collect();
            let mut s = format!("list:{}", names.join("+"));
            if let Some(p) = parts[0]["processors"].as_str() {
                s += &format!("@{p}");
            }
            s
        }
        _ => {
            let mut s = e["location"].as_str().unwrap().to_string();
            if let Some(p) = e["processors"].as_str() {
                s += &format!("@{p}");
            }
            if e["projection"] == "terrain_matching" {
                s += "~terrain";
            }
            s
        }
    }
}

/// A library template as a pool element.
fn our_element(lib: &Library, t: Option<u16>) -> String {
    let Some(i) = t else { return "empty".into() };
    let t = &lib.templates[i as usize];
    if t.name.starts_with("feature:") {
        return t.name.clone();
    }
    let mut s = t.name.clone();
    if let Some(p) = t.processors {
        s += &format!("@{}", lib.processor_lists[p as usize]);
    }
    if t.projection == super::super::Projection::Terrain {
        s += "~terrain";
    }
    s
}

#[test]
fn pools_match_java_element_by_element_both_ways() {
    for c in catalogues() {
        let pools = c.fixture["pools"].as_object().unwrap();
        let java: BTreeMap<&String, &Value> = pools.iter().filter(|(k, _)| k.starts_with(c.prefix)).collect();
        let ours: Vec<&String> = c.lib.pools.keys().filter(|k| k.starts_with(c.prefix)).collect();
        assert_eq!(ours.len(), java.len(), "{}: pool count", c.prefix);
        assert!(!java.is_empty());
        for (name, pool) in java {
            let mine = c.lib.pools.get(name.as_str()).unwrap_or_else(|| panic!("no pool {name}"));
            let fallback = pool["fallback"].as_str().unwrap();
            assert_eq!(mine.fallback.as_deref().unwrap_or("empty"), fallback, "{name} fallback");
            let want: Vec<(u32, String)> = pool["elements"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| (e["weight"].as_u64().unwrap() as u32, java_element(e)))
                .collect();
            let have: Vec<(u32, String)> = mine.elements.iter().map(|&(t, w)| (w, our_element(c.lib, t))).collect();
            assert_eq!(have, want, "{name}");
        }
    }
}

/// The piece line tokens Java's facts give a template (the catalogue's own
/// canonical form).
fn java_tokens(location: &str, t: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let blocks = &t["blocks"];
    let count = |k: &str| blocks[k].as_u64().unwrap_or(0);
    for k in [
        "bell",
        "bed",
        "cobweb",
        "decorated_pot",
        "chest",
        "barrel",
        "trapped_chest",
        "spawner",
        "trial_spawner",
        "vault",
    ] {
        if count(k) > 0 {
            out.push(format!("{k}={}", count(k)));
        }
    }
    // Only the bastion fixture counts these, and spawners by their mob.
    for k in ["gold_block", "gilded_blackstone", "lava"] {
        if count(k) > 0 {
            out.push(format!("{k}={}", count(k)));
        }
    }
    for (k, n) in blocks.as_object().unwrap() {
        if let Some(mob) = k.strip_prefix("spawner ") {
            let n = n.as_u64().unwrap();
            out.push(format!("spawner_mob={mob}") + &if n > 1 { format!("*{n}") } else { String::new() });
        }
    }
    let jobs = [
        "blast_furnace",
        "brewing_stand",
        "cartography_table",
        "cauldron",
        "water_cauldron",
        "composter",
        "fletching_table",
        "grindstone",
        "lectern",
        "loom",
        "smithing_table",
        "smoker",
        "stonecutter",
    ];
    let key = if location.starts_with("village/") { "job" } else { "block" };
    for j in jobs {
        match count(j) {
            0 => {}
            1 => out.push(format!("{key}={j}")),
            n => out.push(format!("{key}={j}*{n}")),
        }
    }
    if let Some(loot) = t["loot_tables"].as_object() {
        for (k, n) in loot {
            let (block, table) = k.split_once(' ').unwrap();
            let n = n.as_u64().unwrap();
            out.push(format!("loot={block}:{table}") + &if n > 1 { format!("*{n}") } else { String::new() });
        }
    }
    for s in t["trial_spawners"].as_array().into_iter().flatten() {
        out.push(format!("trial_spawner_config={}", s["normal"].as_str().unwrap()));
    }
    for v in t["vaults"].as_array().into_iter().flatten() {
        let ominous = if v["ominous"].as_bool().unwrap() { ":ominous" } else { "" };
        out.push(format!("vault_loot={}{ominous}", v["loot_table"].as_str().unwrap()));
    }
    let mut mobs: BTreeMap<String, u32> = BTreeMap::new();
    for e in t["entities"].as_array().unwrap() {
        let mut s = e["id"].as_str().unwrap().to_string();
        if let Some(p) = e["profession"].as_str() {
            s += &format!(":{p}:{}", e["type"].as_str().unwrap());
        }
        if e["baby"].as_bool() == Some(true) {
            s += ":baby";
        }
        match &e["variant"] {
            Value::String(v) => s += &format!(":{v}"),
            Value::Number(v) => s += &format!(":{v}"),
            _ => {}
        }
        if e["persistent"].as_bool() == Some(true) {
            s += ":persistent";
        }
        *mobs.entry(s).or_default() += 1;
    }
    for (m, n) in mobs {
        out.push(format!("mob={m}") + &if n > 1 { format!("*{n}") } else { String::new() });
    }
    let letter = |d: &str| match d {
        "north" => "n",
        "south" => "s",
        "east" => "e",
        "west" => "w",
        "up" => "u",
        _ => "d",
    };
    let mut conns: BTreeMap<String, u32> = BTreeMap::new();
    for c in t["connectors"].as_array().unwrap() {
        let (front, top) = c["orientation"].as_str().unwrap().split_once('_').unwrap();
        let mut f = letter(front).to_string();
        if f == "u" || f == "d" {
            f += letter(top);
            if c["joint"] == "aligned" {
                f += "!";
            }
        }
        let (name, target) = (c["name"].as_str().unwrap(), c["target"].as_str().unwrap());
        let names = if name == target { name.to_string() } else { format!("{name}/{target}") };
        let (p, s) = (c["placement_priority"].as_i64().unwrap_or(0), c["selection_priority"].as_i64().unwrap_or(0));
        let pr = if p != 0 || s != 0 { format!("^{p},{s}") } else { String::new() };
        // The bastion fixture keeps each connector's position.
        let pos = match c["pos"].as_array() {
            Some(v) => format!("@{},{},{}", v[0], v[1], v[2]),
            None => String::new(),
        };
        *conns.entry(format!("{f}:{names}>{}{pr}{pos}", c["pool"].as_str().unwrap())).or_default() += 1;
    }
    for (c, n) in conns {
        out.push(c + &if n > 1 { format!("*{n}") } else { String::new() });
    }
    out.sort();
    out
}

#[test]
fn every_piece_line_holds_javas_facts_and_every_java_template_has_one() {
    for c in catalogues() {
        let templates = c.fixture["templates"].as_object().unwrap();
        let mut lines = BTreeMap::new();
        for line in c.text.lines().filter(|l| l.starts_with("piece ")) {
            let words: Vec<&str> = line.split_whitespace().collect();
            lines.insert(words[1].to_string(), words);
        }
        let java: Vec<(&String, &Value)> = templates.iter().filter(|(k, _)| k.starts_with(c.prefix)).collect();
        for (location, t) in &java {
            let words = lines.get(location.as_str()).unwrap_or_else(|| panic!("no piece line for {location}"));
            if t["missing"].as_bool() == Some(true) {
                assert_eq!(words.len(), 3, "{location}");
                assert_eq!(words[2], "missing");
                continue;
            }
            let size: Vec<i64> = t["size"].as_array().unwrap().iter().map(|v| v.as_i64().unwrap()).collect();
            let ours: Vec<i64> = words[3..6].iter().map(|w| w.parse().unwrap()).collect();
            assert_eq!(ours, size, "{location} size");
            let mut tokens: Vec<String> = words[6..].iter().map(|w| w.to_string()).collect();
            tokens.sort();
            assert_eq!(tokens, java_tokens(location, t), "{location}");
        }
        let extra: Vec<&String> =
            lines.keys().filter(|l| !l.starts_with("feature:") && !templates.contains_key(l.as_str())).collect();
        assert!(extra.is_empty(), "{}: lines for locations Java lacks: {extra:?}", c.prefix);
        // Feature pieces take `FeaturePoolElement`'s default connector: down,
        // top south, no name, target and pool `empty`, rollable.
        for (loc, words) in lines.iter().filter(|(l, _)| l.starts_with("feature:")) {
            assert_eq!(words[2..].join(" "), "feature 1 1 1 ds:/empty>empty", "{loc}");
        }
    }
}

/// What a painted cell becomes, for counting.
fn cell_block(lib: &Library, base: &Base) -> Option<Block> {
    Some(match *base {
        Base::Block(b) => b,
        Base::Loot(_) | Base::Chest => Block::CHEST,
        Base::LootIn(_, b) => b,
        Base::BedHead => Block::BED_HEAD,
        _ => {
            let _ = lib;
            return None;
        }
    })
}

fn java_name_matches(b: Block, java: &str) -> bool {
    rules::states_of(java).contains(&b.0)
}

#[test]
fn painted_templates_hold_every_job_site_bed_bell_container_and_resident() {
    for c in catalogues() {
        let (pieces, _) = parse_pieces(c.text).unwrap();
        let specs: BTreeMap<&str, &PieceSpec> = pieces.iter().map(|p| (p.location.as_str(), p)).collect();
        let mut checked = 0;
        for t in &c.lib.templates {
            if t.name.starts_with("feature:") || t.name.starts_with("list:") {
                continue;
            }
            let spec = specs[t.name.as_str()];
            if spec.missing {
                assert!(t.jigsaws.is_empty() && t.cells.is_empty(), "{}", t.name);
                continue;
            }
            assert_eq!(t.size, spec.size, "{}", t.name);
            let blocks: Vec<Block> = t.cells.iter().filter_map(|cell| cell_block(c.lib, &cell.spec.base)).collect();
            let count = |f: &dyn Fn(Block) -> bool| blocks.iter().filter(|&&b| f(b)).count() as i32;
            assert_eq!(count(&|b| b == Block::BED_HEAD), spec.count("bed"), "{} beds", t.name);
            assert_eq!(count(&|b| b.base() == Block::BELL), spec.count("bell"), "{} bells", t.name);
            for (job, n) in spec.counted("job").chain(spec.counted("block")) {
                assert_eq!(count(&|b| java_name_matches(b, job)), n, "{} {job}", t.name);
            }
            for (loot, n) in spec.counted("loot") {
                let (container, table) = loot.split_once(':').unwrap();
                let cells = t
                    .cells
                    .iter()
                    .filter(|cell| match cell.spec.base {
                        Base::Loot(i) => c.lib.loot[i as usize] == table && container == "chest",
                        Base::LootIn(i, b) => c.lib.loot[i as usize] == table && java_name_matches(b, container),
                        _ => false,
                    })
                    .count() as i32;
                assert_eq!(cells, n, "{} {loot}", t.name);
            }
            let mut mobs: BTreeMap<&str, i32> = BTreeMap::new();
            for (_, m) in &t.mobs {
                *mobs.entry(m.as_str()).or_default() += 1;
            }
            let want: BTreeMap<&str, i32> = spec.counted("mob").collect();
            assert_eq!(mobs, want, "{} residents", t.name);
            if c.prefix == "bastion/" {
                // Bastions also count their gold, gilded blackstone, lava and
                // spawners (by mob), and keep Java's connector positions.
                // `bastion/blocks/gold` is gold by its jigsaw's final state,
                // which the fixture doesn't count.
                let gold = spec.count("gold_block") + i32::from(spec.location == "bastion/blocks/gold");
                assert_eq!(count(&|x| x == Block::GOLD_BLOCK), gold, "{} gold", t.name);
                let gilded = count(&|x| x == Block::GILDED_BLACKSTONE);
                assert_eq!(gilded, spec.count("gilded_blackstone"), "{} gilded blackstone", t.name);
                assert_eq!(count(&|x| x.is_lava()), spec.count("lava"), "{} lava", t.name);
                let spawners: Vec<&str> = t
                    .cells
                    .iter()
                    .filter_map(|cell| match cell.spec.base {
                        Base::Spawner(i) => Some(c.lib.mobs[i as usize].as_str()),
                        _ => None,
                    })
                    .collect();
                assert_eq!(spawners.len() as i32, spec.count("spawner"), "{} spawners", t.name);
                let want: Vec<&str> =
                    spec.counted("spawner_mob").flat_map(|(m, n)| std::iter::repeat_n(m, n as usize)).collect();
                assert_eq!(spawners, want, "{} spawner mobs", t.name);
                let mut have: Vec<String> =
                    t.jigsaws.iter().map(|j| format!("{}>{}@{}", j.name, j.pool, j.pos)).collect();
                let mut want: Vec<String> =
                    spec.conns.iter().map(|j| format!("{}>{}@{}", j.name, j.pool, j.pos.unwrap())).collect();
                have.sort();
                want.sort();
                assert_eq!(have, want, "{} connector positions", t.name);
            }
            checked += 1;
        }
        assert!(checked > 40, "{}: {checked} templates", c.prefix);
    }
}

/// Every block the painters name exists (a missing one would draw stone).
#[test]
fn every_block_the_painters_name_exists() {
    let _ = catalogues();
    assert_eq!(crate::world::jigsaw::draw::missing_names(), Vec::<String>::new());
}

#[test]
fn processor_lists_match_java_both_ways() {
    let mut java = fixture()["processor_lists"].as_object().unwrap().clone();
    let bastion = bastion_fixture()["processor_lists"].as_object().unwrap();
    for (k, v) in bastion {
        assert!(java.insert(k.clone(), v.clone()).is_none(), "{k} in both fixtures");
    }
    let java = &java;
    assert_eq!(rules::LISTS.len(), java.len());
    let test = |t: &Test, v: &Value| {
        let kind = v["type"].as_str().unwrap();
        match *t {
            Test::Always => assert_eq!(kind, "always_true"),
            Test::Block(b) => assert_eq!((kind, b), ("block_match", v["block"].as_str().unwrap())),
            Test::Random(b, p) => {
                assert_eq!((kind, b), ("random_block_match", v["block"].as_str().unwrap()));
                assert_eq!(p, v["probability"].as_f64().unwrap() as f32);
            }
            Test::Tag(tag) => assert_eq!((kind, tag), ("tag_match", v["tag"].as_str().unwrap())),
            Test::State(b, props) => {
                assert_eq!((kind, b), ("blockstate_match", v["block_state"]["id"].as_str().unwrap()));
                assert_eq!(props.len() + 1, v["block_state"].as_object().unwrap().len());
                for (k, val) in props {
                    assert_eq!(v["block_state"][*k], *val);
                }
            }
        }
    };
    for (name, steps) in rules::LISTS {
        let list = java[*name].as_array().unwrap_or_else(|| panic!("Java has no list {name}"));
        assert_eq!(steps.len(), list.len(), "{name}");
        for (step, v) in steps.iter().zip(list) {
            match *step {
                Step::Rot { integrity, rottable } => {
                    assert_eq!(v["type"], "block_rot");
                    assert_eq!(integrity, v["integrity"].as_f64().unwrap() as f32);
                    assert_eq!(format!("#minecraft:{rottable}"), v["rottable_blocks"].as_str().unwrap());
                }
                Step::Protected(tag) => {
                    assert_eq!(v["type"], "protected_blocks");
                    assert_eq!(format!("#minecraft:{tag}"), v["value"].as_str().unwrap());
                }
                Step::Rules(rules) => {
                    let java_rules = v["rules"].as_array().unwrap();
                    assert_eq!(rules.len(), java_rules.len(), "{name}");
                    for (r, jr) in rules.iter().zip(java_rules) {
                        test(&r.input, &jr["input"]);
                        test(&r.location, &jr["location"]);
                        match (r.pos, &jr["position"]) {
                            (None, Value::Null) => {}
                            (
                                Some(rules::PosTest::LinearY { min_chance, max_chance, min_dist, max_dist }),
                                Value::Object(p),
                            ) => {
                                assert_eq!(p["predicate_type"], "minecraft:axis_aligned_linear_pos");
                                assert!(p.get("axis").is_none_or(|a| a == "y"), "{name}");
                                let f = |k: &str| p.get(k).and_then(Value::as_f64).unwrap_or(0.0) as f32;
                                assert_eq!((min_chance, max_chance), (f("min_chance"), f("max_chance")), "{name}");
                                assert_eq!(
                                    (min_dist, max_dist),
                                    (f("min_dist") as i32, f("max_dist") as i32),
                                    "{name}"
                                );
                            }
                            (ours, theirs) => panic!("{name}: position predicate {ours:?} against {theirs}"),
                        }
                        match &jr["output"] {
                            Value::String(o) => assert!(r.output == o && r.props.is_empty(), "{name} {o}"),
                            o => {
                                assert_eq!(r.output, o["id"].as_str().unwrap());
                                assert_eq!(r.props.len() + 1, o.as_object().unwrap().len());
                            }
                        }
                    }
                }
            }
        }
        // Every list compiles: its blocks resolve.
        assert!(rules::list(name).is_some());
    }
}

/// Blocks Java's village processors name that VoxelCraft lacks; their rules
/// never fire.
const UNRESOLVED: &[&str] = &[];

#[test]
fn processor_blocks_resolve() {
    let mut missing = Vec::new();
    for (_, steps) in rules::LISTS {
        for step in *steps {
            if let Step::Rules(rules) = step {
                for r in *rules {
                    for t in [r.input, r.location] {
                        if let Test::Block(b) | Test::Random(b, _) | Test::State(b, _) = t
                            && rules::states_of(b).is_empty()
                        {
                            missing.push(b);
                        }
                    }
                    if rules::output_block(r.output, r.props).is_none() {
                        missing.push(r.output);
                    }
                }
            }
        }
    }
    missing.sort();
    missing.dedup();
    assert_eq!(missing, UNRESOLVED);
}

#[test]
fn mossify_follows_javas_position_random() {
    let list = rules::list("mossify_70_percent").unwrap();
    let cobble = Block::COBBLESTONE;
    let mut mossy = 0;
    for x in 0..64 {
        for z in 0..64 {
            let at = glam::IVec3::new(x, 70, z);
            let b = list.apply(cobble, at, Block::AIR).unwrap();
            let want = crate::world::jigsaw::processor_random(at).next_float() < 0.7;
            assert_eq!(b == crate::world::jigsaw::draw::named("mossy cobblestone"), want, "{at}");
            mossy += usize::from(want);
        }
    }
    assert!((2600..3100).contains(&mossy), "{mossy}");
    // Other blocks pass untouched; streets bridge water on planks.
    assert_eq!(list.apply(Block::DIRT, glam::IVec3::ZERO, Block::AIR), Some(Block::DIRT));
    let street = rules::list("street_plains").unwrap();
    assert_eq!(
        street.apply(Block::DIRT_PATH, glam::IVec3::ZERO, Block::WATER),
        Some(crate::world::jigsaw::draw::named("oak planks"))
    );
    // Abandoned villages lose doors and torches.
    let zombie = rules::list("zombie_plains").unwrap();
    assert_eq!(zombie.apply(Block::TORCH, glam::IVec3::ZERO, Block::AIR), Some(Block::AIR));
}
