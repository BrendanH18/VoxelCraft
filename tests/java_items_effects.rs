//! v0.15 Track D against `items_effects.json` (extracted from the pinned
//! Java 26.3 server by `tools/extract_java_items_effects_fixtures.py`):
//! the four effects and potions, frosted ice and Frost Walker, movement
//! constants, trims (patterns, materials, templates and their loot), farm
//! variants and coloured eggs, each checked both ways; then save and reload
//! of the new items, mob variants and frosted ice ticks.
use glam::{DVec3, IVec3};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use voxelcraft::{
    content::{
        farm_variant::{self, FarmVariant},
        trim,
    },
    entity::{Entities, MobKind},
    inventory::{
        Stack,
        components::{Trim, TrimMaterial, TrimPattern},
        stack_from_str, stack_to_string,
    },
    item::{ArmorMaterial, ArmorPiece, Item},
    potion::Potion,
    simulation::effects::{self, Category, Effect},
    world::{World, block::Block, chunk::ChunkData, frosted_ice, terrain::Generator},
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/items_effects.json")).unwrap()
}

fn strings(v: &Value) -> BTreeSet<String> {
    v.as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_owned()).collect()
}

fn item(id: &str) -> Item {
    voxelcraft::java_registry::item(id, "").unwrap_or_else(|| panic!("{id} is not registered"))
}

#[test]
fn effects_and_potions_match_mob_effects_and_potions() {
    let f = fixture();
    for (id, e) in f["effects"].as_object().unwrap() {
        let effect = Effect::from_id(id).unwrap();
        let c = e["colour"].as_u64().unwrap() as u32;
        assert_eq!(effect.colour(), [(c >> 16) as u8, (c >> 8) as u8, c as u8], "{id}");
        assert_eq!(effect.category(), Category::Harmful, "{id}");
        assert_eq!(e["category"], "harmful");
    }
    let e = &f["effects"];
    // A Java float literal, as `MobEffects` passes it.
    assert_eq!(effects::INFESTED_CHANCE, e["infested"]["chance"].as_f64().unwrap() as f32);
    let uniform = |v: &Value| (v["min"].as_u64().unwrap() as u32, v["max"].as_u64().unwrap() as u32);
    assert_eq!(effects::INFESTED_SILVERFISH, uniform(&e["infested"]["count"]));
    assert_eq!(effects::WEAVING_COBWEBS, uniform(&e["weaving"]["count"]));
    assert_eq!(u64::from(effects::OOZING_SLIMES), e["oozing"]["count"]["value"].as_u64().unwrap());
    assert!(e["wind_charged"].get("count").is_none());
    for (id, p) in f["potions"].as_object().unwrap() {
        let potion = Potion::from_id(id).unwrap_or_else(|| panic!("potion {id}"));
        let effect = Effect::from_id(p["effect"].as_str().unwrap()).unwrap();
        assert_eq!(potion.info().effect, Some((effect, 0, p["duration"].as_u64().unwrap() as u32)), "{id}");
        assert_eq!(potion.info().secondary_effect, None);
        // Drinkable 1650.., splash 1654.., no long or strong forms.
        let k = Potion::all().position(|q| q == potion).unwrap() as u16 - 41;
        assert_eq!(Item::potion(potion), Item(1650 + k));
        assert_eq!(Item::splash_potion(potion), Item(1654 + k));
        assert_eq!(Item(1650 + k).as_potion(), Some(potion));
        assert_eq!(Item(1654 + k).as_splash_potion(), Some(potion));
        assert!(Potion::from_id(&format!("long_{id}")).is_none() && Potion::from_id(&format!("strong_{id}")).is_none());
    }
    assert_eq!(f["potions"].as_object().unwrap().len(), 4);
}

#[test]
fn frosted_ice_and_frost_walker_match_the_block_and_enchantment() {
    let f = fixture();
    let ice = &f["frosted_ice"];
    let pair = |v: &Value| (v[0].as_u64().unwrap() as u32, v[1].as_u64().unwrap() as u32);
    assert_eq!(frosted_ice::PLACE_DELAY, pair(&ice["place_delay"]));
    assert_eq!(frosted_ice::TICK_DELAY, pair(&ice["tick_delay"]));
    assert_eq!(u64::from(frosted_ice::MAX_AGE), ice["max_age"].as_u64().unwrap());
    assert_eq!(u64::from(frosted_ice::NEIGHBORS_TO_AGE), ice["neighbors_to_age"].as_u64().unwrap());
    assert_eq!(u64::from(frosted_ice::NEIGHBORS_TO_MELT), ice["neighbors_to_melt"].as_u64().unwrap());
    assert_eq!(ice["drops"], 0);
    for age in 0..=frosted_ice::MAX_AGE {
        assert_eq!(frosted_ice::frosted_ice(age).drop(), None);
    }
    let w = &f["frost_walker"];
    assert_eq!(w["block"], "frosted_ice");
    assert_eq!(w["age"], 0);
    assert_eq!(w["offset"], serde_json::json!([0, -1, 0]));
    assert_eq!(w["height"], 1.0);
    for level in 1..=w["max_level"].as_u64().unwrap() as u8 {
        let linear = w["radius_base"].as_f64().unwrap()
            + w["radius_per_level_above_first"].as_f64().unwrap() * f64::from(level - 1);
        let radius = linear.clamp(w["radius_min"].as_f64().unwrap(), w["radius_max"].as_f64().unwrap());
        assert_eq!(frosted_ice::frost_walker_radius(level), radius as i32, "level {level}");
    }
    assert_eq!(
        w["predicates"],
        serde_json::json!([
            "minecraft:matching_block_tag",
            "minecraft:matching_blocks",
            "minecraft:matching_fluids",
            "minecraft:unobstructed"
        ])
    );
}

#[test]
fn movement_constants_match_living_entity_and_web_block() {
    let m = &fixture()["movement"];
    let v = |k: &str| m[k].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect::<Vec<_>>();
    assert_eq!(v("web_speed_weaving"), effects::WEAVING_WEB_SPEED.to_vec());
    assert_eq!(m["spectral_glowing_ticks"].as_u64().unwrap(), u64::from(effects::SPECTRAL_GLOWING_TICKS));
    assert_eq!((m["levitation_speed_per_level"].as_f64(), m["levitation_ease"].as_f64()), (Some(0.05), Some(0.2)));
}

#[test]
fn trims_match_patterns_materials_templates_and_tags() {
    let f = fixture();
    let patterns: BTreeSet<_> = f["trim_patterns"].as_object().unwrap().keys().cloned().collect();
    let ours: BTreeSet<_> = TrimPattern::ALL.iter().map(|p| p.name().to_owned()).collect();
    assert_eq!(patterns, ours);
    for (name, p) in f["trim_patterns"].as_object().unwrap() {
        assert_eq!(p["decal"], false, "{name}");
    }
    for (name, m) in f["trim_materials"].as_object().unwrap() {
        let material = TrimMaterial::from_name(name).unwrap();
        let [r, g, b] = trim::material_colour(material);
        assert_eq!(m["colour"].as_str().unwrap(), format!("#{r:02X}{g:02X}{b:02X}"), "{name}");
    }
    // `#trim_materials` both ways.
    let tag = strings(&f["item_tags"]["trim_materials"]);
    let provided: BTreeSet<_> = trim::material_items().iter().map(|&(i, _)| i.0).collect();
    assert_eq!(tag.iter().map(|id| item(id).0).collect::<BTreeSet<_>>(), provided);
    for id in &tag {
        assert!(trim::provides_material(item(id)).is_some(), "{id}");
    }
    for (id, rarity) in f["template_rarity"].as_object().unwrap() {
        let t = item(id);
        let pattern = trim::template_pattern(t).unwrap();
        assert_eq!(format!("{:?}", trim::rarity(pattern)).to_lowercase(), rarity.as_str().unwrap(), "{id}");
        assert_eq!(id, &format!("{}_armor_trim_smithing_template", pattern.name()));
        assert!(t.icon_layer().is_some());
        assert_eq!(t.max_stack(), 64);
    }
    assert_eq!(trim::all().count(), f["template_rarity"].as_object().unwrap().len());
    // `#eggs` both ways.
    let eggs = strings(&f["item_tags"]["eggs"]);
    for id in &eggs {
        assert!(farm_variant::is_egg(item(id)), "{id}");
    }
    let ours = Item::all_items().filter(|&i| farm_variant::is_egg(i)).count();
    assert_eq!(ours, eggs.len());
}

#[test]
fn farm_variants_spawn_in_javas_biomes_and_chickens_lay_their_eggs() {
    let f = fixture();
    for mob in ["chicken", "cow", "pig"] {
        let variants = f["farm_variants"][mob].as_object().unwrap();
        let names: BTreeSet<_> = variants.keys().cloned().collect();
        assert_eq!(names, FarmVariant::ALL.iter().map(|v| v.name().to_owned()).collect(), "{mob}");
        for (name, v) in variants {
            let conditions = v["spawn_conditions"].as_array().unwrap();
            assert_eq!(conditions.len(), 1);
            let expected = match conditions[0]["biome_tag"].as_str() {
                None => {
                    assert_eq!(conditions[0]["priority"], 0, "{mob} {name} is the fallback");
                    continue;
                }
                Some(tag) => strings(&f["biome_tags"][tag]),
            };
            let variant = FarmVariant::from_name(name).unwrap();
            let ours: BTreeSet<_> = match variant {
                FarmVariant::Warm => farm_variant::WARM_BIOMES.iter().map(|s| s.to_string()).collect(),
                FarmVariant::Cold => farm_variant::COLD_BIOMES.iter().map(|s| s.to_string()).collect(),
                FarmVariant::Temperate => unreachable!(),
            };
            assert_eq!(ours, expected, "{mob} {name}");
            for biome in &expected {
                assert_eq!(farm_variant::for_biome(biome), variant, "{biome}");
            }
        }
    }
    assert_eq!(farm_variant::for_biome("plains"), FarmVariant::Temperate);
    for (variant, egg) in f["chicken_lay"].as_object().unwrap() {
        assert_eq!(FarmVariant::from_name(variant).unwrap().egg(), item(egg.as_str().unwrap()));
    }
}

/// Every loot table naming a Track D item: those the game carries name the
/// same entries (both ways) and resolve; the rest belong to content other
/// tracks or releases own.
#[test]
fn loot_tables_carry_the_new_items() {
    let f = fixture();
    let ours: Value = serde_json::from_str(include_str!("../src/world/loot_tables.json")).unwrap();
    let tables = ours["tables"].as_object().unwrap();
    // Not data-driven here: implemented in code and checked below, or owned elsewhere.
    let elsewhere = BTreeMap::from([
        ("archaeology/trail_ruins_rare", "trail ruin suspicious gravel has no loot yet (archaeology gap)"),
        ("chests/end_city_treasure", "end cities' chests are not generated yet (v0.16 End content)"),
        ("chests/woodland_mansion", "woodland mansions are v0.15 Track F (wave 2)"),
        ("entities/elder_guardian", "coded in Entities::drop_loot_with_fire"),
        ("gameplay/chicken_lay", "coded as FarmVariant::egg"),
        ("gameplay/piglin_bartering", "coded as entity::nether::BARTERING"),
    ]);
    let names = |v: &Value| {
        let mut out = Vec::new();
        fn walk(v: &Value, out: &mut Vec<String>) {
            match v {
                Value::Object(o) => {
                    if let Some(n) = o.get("name").and_then(Value::as_str) {
                        let n = n.trim_start_matches("minecraft:");
                        if matches!(n, "spectral_arrow" | "blue_egg" | "brown_egg")
                            || n.ends_with("_armor_trim_smithing_template")
                        {
                            out.push(n.to_owned());
                        }
                    }
                    o.values().for_each(|v| walk(v, out));
                }
                Value::Array(a) => a.iter().for_each(|v| walk(v, out)),
                _ => {}
            }
        }
        walk(v, &mut out);
        out.sort();
        out
    };
    let mut seen = 0;
    for (table, entries) in f["loot"].as_object().unwrap() {
        let mut expected: Vec<String> =
            entries.as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap().to_owned()).collect();
        expected.sort();
        for id in &expected {
            item(id);
        }
        let key = table.strip_prefix("chests/").unwrap_or(table);
        match (tables.get(key), elsewhere.get(table.as_str())) {
            (Some(t), None) => {
                assert_eq!(names(t), expected, "{table}");
                seen += 1;
            }
            (None, Some(_)) => {}
            (t, e) => panic!("{table}: game table {} and gap {:?}", t.is_some(), e),
        }
    }
    assert!(seen >= 15, "{seen}");
    // The coded tables.
    assert!(voxelcraft::entity::nether::BARTERING.iter().any(|e| e.0
        == voxelcraft::entity::nether::Barter::Item(Item::SPECTRAL_ARROW)
        && (e.1, e.2, e.3) == (40, 6, 12)));
}

/// The new items, trimmed armor and stacks with the new potions survive the
/// stack save codec.
#[test]
fn new_items_and_trims_survive_the_stack_codec() {
    let mut stacks = vec![
        Stack::new(Item::SPECTRAL_ARROW, 64),
        Stack::new(farm_variant::BLUE_EGG, 16),
        Stack::new(farm_variant::BROWN_EGG, 3),
    ];
    stacks.extend(trim::all().map(|t| Stack::new(t, 2)));
    for id in ["wind_charged", "weaving", "oozing", "infested"] {
        let p = Potion::from_id(id).unwrap();
        stacks.extend([
            Stack::new(Item::potion(p), 1),
            Stack::new(Item::splash_potion(p), 1),
            Stack::lingering_potion(p),
            Stack::tipped_arrow(p, 8),
        ]);
    }
    for pattern in TrimPattern::ALL {
        let mut s = Stack::new(Item::armor(ArmorPiece::Helmet, ArmorMaterial::Iron), 1);
        s.trim = Some(Trim { pattern, material: TrimMaterial::ALL[pattern as usize % 11] });
        stacks.push(s);
    }
    for s in stacks {
        assert_eq!(stack_from_str(&stack_to_string(Some(s))), Some(Some(s)), "{}", s.display_name());
        assert!(s.item.is_valid());
    }
    assert_eq!(Stack::tipped_arrow(Potion::from_id("infested").unwrap(), 1).display_name(), "arrow of infestation");
    assert_eq!(
        Stack::lingering_potion(Potion::from_id("wind_charged").unwrap()).display_name(),
        "lingering potion of wind charging"
    );
}

/// Farm variants, the chicken's included, survive a mob save; older records
/// without the name load temperate.
#[test]
fn farm_variants_survive_a_mob_save() {
    let mut e = Entities::new(3);
    let kinds = [MobKind::Chicken, MobKind::Cow, MobKind::Pig];
    for (i, kind) in kinds.into_iter().enumerate() {
        for (j, v) in FarmVariant::ALL.into_iter().enumerate() {
            e.spawn(kind, DVec3::new(i as f64 * 3.0 + 0.5, 64.0, j as f64 * 3.0 + 0.5));
            let m = e.mobs.last_mut().unwrap();
            m.uid = 100 + (i * 3 + j) as u32;
            m.set_farm_variant(v);
        }
    }
    let text = e.nether_mobs_to_string();
    assert!(text.contains("\"farm_variant\":\"warm\"") && text.contains("\"farm_variant\":\"cold\""));
    let mut back = Entities::new(4);
    back.load_nether_mobs(&text);
    assert_eq!(back.mobs.len(), 9);
    for (a, b) in e.mobs.iter().zip(&back.mobs) {
        assert_eq!((a.kind, a.farm_variant()), (b.kind, b.farm_variant()));
    }
    // An older record: temperate.
    let legacy = text.replace("\"farm_variant\":\"warm\",", "").replace("\"farm_variant\":\"cold\",", "");
    let mut old = Entities::new(5);
    old.load_nether_mobs(&legacy);
    assert!(old.mobs.iter().all(|m| m.farm_variant() == FarmVariant::Temperate), "{legacy}");
}

/// Frosted ice and its pending tick survive a save and keep ageing.
#[test]
fn frosted_ice_and_its_tick_survive_a_save() {
    let mut w = World::new_remote(Arc::new(Generator::new(7)), 2);
    w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    let p = IVec3::new(4, 140, 4);
    w.set_block(p, frosted_ice::frosted_ice(1));
    let redstone = w.redstone_to_string();
    let chunks = w.modified_chunks();
    let mut back = World::new_remote(Arc::new(Generator::new(7)), 2);
    for (pos, data) in chunks {
        back.receive_chunk(pos, data);
    }
    back.load_redstone(&redstone);
    assert_eq!(back.get_block(p), Some(frosted_ice::frosted_ice(1)));
    assert!(back.redstone_to_string().contains(&format!("{},{},{}", p.x, p.y, p.z)), "the tick is kept");
}
