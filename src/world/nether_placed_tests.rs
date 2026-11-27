//! `nether_generation_tests`: generation 6's Nether against Java 26.3's
//! data, extracted from the jar by `tools/extract_java_nether_fixtures.py`
//! into `tests/fixtures/java/26.3/nether_generation.json` (v1.0 Track 8,
//! GEN-04a). Every check runs both ways: nothing Java has is missing and
//! nothing is added.

use std::collections::BTreeSet;

use glam::IVec3;
use serde_json::{Value, json};

use super::*;
use crate::world::block::Block;
use crate::world::jigsaw::rules::states_of;
use crate::world::nether_features as features;

fn fixture() -> &'static Value {
    static F: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    F.get_or_init(|| {
        serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/nether_generation.json")).unwrap()
    })
}

fn int_json(p: IntP) -> Value {
    match p {
        IntP::Const(v) => json!(v),
        IntP::Uniform(lo, hi) => json!({"type": "minecraft:uniform", "min_inclusive": lo, "max_inclusive": hi}),
        IntP::Trapezoid(min, max, plateau) => {
            json!({"type": "minecraft:trapezoid", "min": min, "max": max, "plateau": plateau})
        }
        IntP::BiasedToBottom(lo, hi) => {
            json!({"type": "minecraft:biased_to_bottom", "min_inclusive": lo, "max_inclusive": hi})
        }
    }
}

fn anchor_json(a: Anchor) -> Value {
    match a {
        Anchor::Absolute(y) => json!({"absolute": y}),
        Anchor::AboveBottom(n) => json!({"above_bottom": n}),
        Anchor::BelowTop(n) => json!({"below_top": n}),
    }
}

fn height_json(h: Height) -> Value {
    match h {
        Height::Uniform(lo, hi) => {
            json!({"type": "minecraft:uniform", "min_inclusive": anchor_json(lo), "max_inclusive": anchor_json(hi)})
        }
        Height::Trapezoid(lo, hi) => {
            json!({"type": "minecraft:trapezoid", "min_inclusive": anchor_json(lo), "max_inclusive": anchor_json(hi)})
        }
        Height::VeryBiased(lo, hi, inner) => json!({
            "type": "minecraft:very_biased_to_bottom",
            "min_inclusive": anchor_json(lo),
            "max_inclusive": anchor_json(hi),
            "inner": inner,
        }),
    }
}

fn pred_json(p: &Pred) -> Value {
    let offset = |o: IVec3, mut v: Value| {
        if o != IVec3::ZERO {
            v["offset"] = json!([o.x, o.y, o.z]);
        }
        v
    };
    match *p {
        Pred::Tag(tag, o) => {
            offset(o, json!({"type": "minecraft:matching_block_tag", "tag": format!("minecraft:{tag}")}))
        }
        Pred::Blocks(ids, o) => {
            let blocks = match ids {
                [one] => json!(format!("minecraft:{one}")),
                many => json!(many.iter().map(|id| format!("minecraft:{id}")).collect::<Vec<_>>()),
            };
            offset(o, json!({"type": "minecraft:matching_blocks", "blocks": blocks}))
        }
        Pred::Not(inner) => json!({"type": "minecraft:not", "predicate": pred_json(inner)}),
        Pred::All(all) => {
            json!({"type": "minecraft:all_of", "predicates": all.iter().map(pred_json).collect::<Vec<_>>()})
        }
    }
}

/// A modifier as Java's `placed_feature` JSON writes it.
fn mod_json(m: &Mod) -> Value {
    match *m {
        Mod::Count(n) => json!({"type": "minecraft:count", "count": int_json(n)}),
        Mod::CountOnEveryLayer(n) => json!({"type": "minecraft:count_on_every_layer", "count": n}),
        Mod::InSquare => json!({"type": "minecraft:in_square"}),
        Mod::Height(h) => json!({"type": "minecraft:height_range", "height": height_json(h)}),
        Mod::MotionBlocking => json!({"type": "minecraft:heightmap", "heightmap": "MOTION_BLOCKING"}),
        Mod::Biome => json!({"type": "minecraft:biome"}),
        Mod::Rarity(n) => json!({"type": "minecraft:rarity_filter", "chance": n}),
        Mod::Offset(x, y, z) => {
            json!({"type": "minecraft:offset", "x": int_json(x), "y": int_json(y), "z": int_json(z)})
        }
        Mod::Filter(p) => json!({"type": "minecraft:block_predicate_filter", "predicate": pred_json(&p)}),
        Mod::ScanDown(steps, target) => json!({
            "type": "minecraft:environment_scan",
            "direction_of_search": "down",
            "max_steps": steps,
            "target_condition": pred_json(&target),
        }),
    }
}

/// Whether a VoxelCraft block is a Java block id.
fn is(b: Block, java: &str) -> bool {
    states_of(java.strip_prefix("minecraft:").unwrap_or(java)).contains(&b.0)
}

fn ids(v: &Value) -> Vec<String> {
    match v {
        Value::String(s) => vec![s.clone()],
        Value::Array(a) => a.iter().map(|x| x.as_str().unwrap().to_string()).collect(),
        other => panic!("not a block list: {other}"),
    }
}

#[test]
fn biome_source_parameter_points_match_javas_nether_preset() {
    let points = fixture()["parameter_points"].as_object().unwrap();
    assert_eq!(points.len(), NetherBiome::ALL.len());
    // `Climate.quantizeCoord`: value x 10000.
    let q = |v: f32| (v * 10000.0).round() as i64;
    for b in NetherBiome::ALL {
        let p = &points[b.name()];
        let (t, h, offset) = b.point();
        assert_eq!(p["temperature"], json!([q(t), q(t)]), "{}", b.name());
        assert_eq!(p["humidity"], json!([q(h), q(h)]), "{}", b.name());
        for zero in ["continentalness", "erosion", "depth", "weirdness"] {
            assert_eq!(p[zero], json!([0, 0]), "{} {zero}", b.name());
        }
        assert_eq!(p["offset"], json!(q(offset)), "{}", b.name());
    }
}

#[test]
fn every_biome_lists_javas_placed_features_by_step_in_order() {
    let biomes = fixture()["biomes"].as_object().unwrap();
    assert_eq!(biomes.len(), NetherBiome::ALL.len());
    for b in NetherBiome::ALL {
        let steps = biomes[b.name()]["features"].as_array().unwrap();
        for step in 0..11u8 {
            let java: Vec<&str> = steps
                .get(step as usize)
                .map(|s| s.as_array().unwrap().iter().map(|n| n.as_str().unwrap()).collect())
                .unwrap_or_default();
            let ours: Vec<&str> =
                PLACED.iter().filter(|p| p.step == step && p.biomes.contains(&b)).map(|p| p.name).collect();
            assert_eq!(ours, java, "{} step {step}", b.name());
        }
    }
    let java: BTreeSet<&String> = fixture()["placed_features"].as_object().unwrap().keys().collect();
    let ours: BTreeSet<String> = PLACED.iter().map(|p| p.name.to_string()).collect();
    assert_eq!(ours, java.into_iter().cloned().collect(), "the table holds exactly Java's placed features");
}

#[test]
fn every_placed_feature_keeps_javas_modifiers() {
    let placed = fixture()["placed_features"].as_object().unwrap();
    for p in PLACED {
        let java = &placed[p.name]["placement"];
        let ours = Value::Array(p.mods.iter().map(mod_json).collect());
        assert_eq!(&ours, java, "{}", p.name);
    }
}

#[test]
fn every_configured_feature_matches_javas_configuration() {
    let placed = fixture()["placed_features"].as_object().unwrap();
    let configured = fixture()["features"].as_object().unwrap();
    for p in PLACED {
        let name = placed[p.name]["feature"].as_str().unwrap().strip_prefix("minecraft:").unwrap();
        let f = &configured[name];
        let kind = f["type"].as_str().unwrap();
        match p.feature {
            Feature::Spring { valid, requires_below, rock, hole } => {
                assert_eq!(kind, "minecraft:spring_feature", "{name}");
                assert_eq!(f["state"], json!({"id": "minecraft:lava", "properties": {"falling": "true"}}));
                assert_eq!(f["requires_block_below"].as_bool().unwrap_or(true), requires_below, "{name}");
                assert_eq!(f["rock_count"].as_u64().unwrap_or(4), u64::from(rock), "{name}");
                assert_eq!(f["hole_count"].as_u64().unwrap_or(1), u64::from(hole), "{name}");
                let java: Vec<String> = ids(&f["valid_blocks"]);
                let ours: Vec<String> = valid.iter().map(|v| format!("minecraft:{v}")).collect();
                assert_eq!(ours, java, "{name}");
                for v in valid {
                    assert!(is(java_block(v), v), "{v} resolves to its block");
                }
            }
            Feature::SimpleBlock(what) => {
                assert_eq!(kind, "minecraft:simple_block", "{name}");
                let to_place = &f["to_place"];
                let table = match what {
                    Simple::CrimsonVegetation => features::Vegetation::Crimson.table(),
                    Simple::WarpedVegetation => features::Vegetation::Warped.table(),
                    _ => &[],
                };
                if table.is_empty() {
                    let id = to_place["id"].as_str().unwrap();
                    let want = match what {
                        Simple::Fire => "minecraft:fire",
                        Simple::SoulFire => "minecraft:soul_fire",
                        Simple::BrownMushroom => "minecraft:brown_mushroom",
                        Simple::RedMushroom => "minecraft:red_mushroom",
                        Simple::CrimsonRoots => "minecraft:crimson_roots",
                        Simple::NetherSprouts => "minecraft:nether_sprouts",
                        _ => unreachable!(),
                    };
                    assert_eq!(id, want, "{name}");
                    if what == Simple::Fire {
                        assert_eq!(to_place["properties"]["age"], "0", "fire of age 0, as Block::fire(0)");
                    }
                } else {
                    assert_eq!(to_place["type"], "minecraft:weighted", "{name}");
                    let entries = to_place["entries"].as_array().unwrap();
                    assert_eq!(entries.len(), table.len(), "{name}");
                    for (e, &(b, w)) in entries.iter().zip(table) {
                        assert!(is(b, e["data"].as_str().unwrap()), "{name}: {e}");
                        assert_eq!(e["weight"].as_i64().unwrap(), i64::from(w), "{name}: {e}");
                    }
                }
            }
            Feature::Glowstone => {
                let (attempts, xz, y) = features::GLOWSTONE_SPREAD;
                assert_eq!(kind, "minecraft:random_neighbor_spread");
                assert_eq!(f["attempts"], json!(attempts));
                assert_eq!(f["xz_offset"], int_json(xz));
                assert_eq!(f["y_offset"], int_json(y));
                assert_eq!(f["block"], json!({"id": "minecraft:glowstone"}));
                assert_eq!(f["accepted_neighbors"], "minecraft:glowstone");
                assert_eq!(f["can_replace"], pred_json(&Pred::Tag("air", IVec3::ZERO)));
            }
            Feature::Ore { size, state } => {
                assert_eq!(kind, "minecraft:ore", "{name}");
                assert_eq!(f["size"], json!(size), "{name}");
                assert_eq!(f["discard_chance_on_air_exposure"], json!(0.0), "{name}");
                let targets = f["targets"].as_array().unwrap();
                assert_eq!(targets.len(), 1);
                assert_eq!(targets[0]["state"], format!("minecraft:{state}"), "{name}");
                assert_eq!(
                    targets[0]["target"],
                    json!({"block": "minecraft:netherrack", "predicate_type": "minecraft:block_match"})
                );
                assert!(is(java_block(state), state), "{state}");
            }
            Feature::Debris { size } => {
                assert_eq!(kind, "minecraft:scattered_ore", "{name}");
                assert_eq!(f["size"], json!(size), "{name}");
                assert_eq!(f["discard_chance_on_air_exposure"], json!(1.0));
                assert_eq!(f["targets"][0]["state"], "minecraft:ancient_debris");
                assert_eq!(f["targets"][0]["target"]["tag"], "minecraft:base_stone_nether");
            }
            Feature::Blobs { state, radius } => {
                assert_eq!(kind, "minecraft:netherrack_replace_blobs", "{name}");
                assert_eq!(f["state"], format!("minecraft:{state}"));
                assert_eq!(f["target"], "minecraft:netherrack");
                assert_eq!(f["radius"], int_json(IntP::Uniform(radius.0, radius.1)));
            }
            Feature::HugeFungus(wood) => {
                assert_eq!(kind, "minecraft:huge_fungus", "{name}");
                assert!(is(wood.stem(), f["stem_state"].as_str().unwrap()));
                assert!(is(wood.wart(), f["hat_state"].as_str().unwrap()));
                assert!(is(wood.nylium(), f["valid_base_block"].as_str().unwrap()));
                assert_eq!(f["decor_state"], "minecraft:shroomlight");
            }
            Feature::WeepingVines => {
                assert_eq!(kind, "minecraft:overlay");
                let parts = f["features"].as_array().unwrap();
                let spread = &parts[0]["feature"];
                let (attempts, xz, y) = features::WART_SPREAD;
                assert_eq!(spread["type"], "minecraft:random_neighbor_spread");
                assert_eq!(spread["attempts"], json!(attempts));
                assert_eq!(spread["xz_offset"], int_json(xz));
                assert_eq!(spread["y_offset"], int_json(y));
                assert_eq!(spread["block"], json!({"id": "minecraft:nether_wart_block"}));
                assert_eq!(
                    spread["accepted_neighbors"],
                    json!(["minecraft:netherrack", "minecraft:nether_wart_block"])
                );
                assert_eq!(parts[0]["placement"], json!([]));
                let (count, xz, y) = features::VINE_PLACEMENT;
                let want = [
                    Mod::Count(IntP::Const(count)),
                    Mod::Offset(xz, y, xz),
                    Mod::Filter(Pred::All(&[
                        Pred::Tag("air", IVec3::ZERO),
                        Pred::Blocks(&["netherrack", "nether_wart_block"], IVec3::Y),
                    ])),
                ];
                assert_eq!(parts[1]["placement"], Value::Array(want.iter().map(mod_json).collect()));
                column(&parts[1]["feature"], "down", "weeping_vines", &features::WEEPING_LENGTHS);
            }
            Feature::TwistingVines => column(f, "up", "twisting_vines", &features::TWISTING_LENGTHS),
            Feature::Delta => {
                assert_eq!(kind, "minecraft:delta_feature");
                assert_eq!(f["contents"], "minecraft:lava");
                assert_eq!(f["rim"], "minecraft:magma_block");
                // `nether_features::delta` draws these.
                assert_eq!(f["rim_size"], int_json(IntP::Uniform(0, 2)));
                assert_eq!(f["size"], int_json(IntP::Uniform(3, 7)));
            }
            Feature::Columns { large } => {
                // `nether_features::basalt_columns`: 9 in 10 clusters reach 5
                // with 50 columns, else 8 with 15; columns reach 1 (small) or
                // 2..=3 (large) and stand 1..=4 or 5..=10 high.
                assert_eq!(kind, "minecraft:weighted_random_selector");
                let variants = f["features"].as_array().unwrap();
                let (reach, height) = if large {
                    (int_json(IntP::Uniform(2, 3)), int_json(IntP::Uniform(5, 10)))
                } else {
                    (json!(1), int_json(IntP::Uniform(1, 4)))
                };
                for (v, (weight, cluster, count)) in variants.iter().zip([(9, 5, 50), (1, 8, 15)]) {
                    assert_eq!(v["weight"], json!(weight));
                    let c = &v["data"]["feature"];
                    assert_eq!(c["type"], "minecraft:stepped_column_cluster");
                    assert_eq!(c["cluster_reach"], json!(cluster));
                    assert_eq!(c["column_count"], json!(count));
                    assert_eq!(c["column_reach"], reach);
                    assert_eq!(c["height"], height);
                }
            }
            Feature::BasaltPillar => {
                assert_eq!(kind, "minecraft:overlay");
                let parts = f["features"].as_array().unwrap();
                assert_eq!(parts.len(), 5, "the pillar and its four ragged sides");
                for side in &parts[1..] {
                    assert_eq!(side["feature"]["chance_to_continue"], json!(0.9));
                }
            }
            Feature::Inert => {
                // Java's overworld lava spring: no valid block is Nether terrain.
                assert_eq!(kind, "minecraft:spring_feature");
                let nether: Vec<&str> = fixture()["block_tags"]["base_stone_nether"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap())
                    .collect();
                for v in ids(&f["valid_blocks"]) {
                    let v = v.strip_prefix("minecraft:").unwrap();
                    assert!(
                        !nether.contains(&v) && !matches!(v, "soul_sand" | "soul_soil" | "gravel" | "magma_block"),
                        "{name}: {v} occurs in the Nether"
                    );
                }
            }
        }
    }
}

/// A Java 26.3 `block_column` vine: plant layer of weighted heights, then a
/// head aged 17..25, through air, tip kept first.
fn column(f: &Value, direction: &str, vine: &str, lengths: &[(IntP, i32)]) {
    assert_eq!(f["type"], "minecraft:block_column");
    assert_eq!(f["direction"], direction);
    assert_eq!(f["prioritize_tip"], true);
    assert_eq!(f["allowed_placement"], pred_json(&Pred::Tag("air", IVec3::ZERO)));
    let layers = f["layers"].as_array().unwrap();
    let dist: Vec<Value> = lengths.iter().map(|&(p, w)| json!({"data": int_json(p), "weight": w})).collect();
    assert_eq!(layers[0]["height"], json!({"type": "minecraft:weighted_list", "distribution": dist}));
    assert_eq!(layers[0]["provider"], json!({"id": format!("minecraft:{vine}_plant")}));
    assert_eq!(layers[1]["height"], json!(1));
    let (lo, hi) = features::VINE_HEAD_AGE;
    assert_eq!(layers[1]["provider"]["values"], int_json(IntP::Uniform(lo, hi)));
    assert_eq!(layers[1]["provider"]["source"]["id"], format!("minecraft:{vine}"));
}

#[test]
fn structure_placement_matches_javas_nether_sets_and_biome_tags() {
    use crate::world::{bastion, nether_complexes as nc, nether_fossil};
    let sets = &fixture()["structure_sets"];
    let complexes = &sets["nether_complexes"];
    assert_eq!(complexes["placement"]["type"], "minecraft:random_spread");
    assert_eq!(complexes["placement"]["spacing"], json!(nc::SPACING));
    assert_eq!(complexes["placement"]["separation"], json!(nc::SEPARATION));
    assert_eq!(complexes["placement"]["salt"], json!(nc::SALT));
    let weights: Vec<(String, u64)> = complexes["structures"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (s["structure"].as_str().unwrap().to_string(), s["weight"].as_u64().unwrap()))
        .collect();
    let ours: Vec<(String, u64)> = nc::WEIGHTS
        .iter()
        .map(|&(k, w)| {
            let name = if k == nc::Complex::Fortress { "minecraft:fortress" } else { "minecraft:bastion_remnant" };
            (name.to_string(), u64::from(w))
        })
        .collect();
    assert_eq!(ours, weights);
    let fossils = &sets["nether_fossils"];
    assert_eq!(fossils["placement"]["spacing"], json!(nether_fossil::SPACING));
    assert_eq!(fossils["placement"]["separation"], json!(nether_fossil::SEPARATION));
    assert_eq!(fossils["placement"]["salt"], json!(nether_fossil::SALT));

    let bastion = &fixture()["structures"]["bastion_remnant"];
    let s = bastion::JIGSAW;
    assert_eq!(bastion["start_pool"], format!("minecraft:{}", s.start_pool));
    assert_eq!(bastion["size"], json!(s.max_depth));
    assert_eq!(bastion["max_distance_from_center"], json!(s.max_distance));
    assert_eq!(bastion["start_height"], json!({"absolute": 33}));
    // The biome tags: bastions shun basalt deltas, fossils keep to soul sand
    // valleys, fortresses and ruined portals go anywhere.
    let tags = &fixture()["structure_biomes"];
    let names = |pred: &dyn Fn(NetherBiome) -> bool| -> Vec<String> {
        let mut v: Vec<String> =
            NetherBiome::ALL.into_iter().filter(|&b| pred(b)).map(|b| b.name().to_string()).collect();
        v.sort();
        v
    };
    let java = |tag: &str| -> Vec<String> {
        tags[tag].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect()
    };
    assert_eq!(java("has_structure/bastion_remnant"), names(&|b| b.has_bastions()));
    assert_eq!(java("has_structure/nether_fossil"), names(&|b| b == NetherBiome::SoulSandValley));
    assert_eq!(java("has_structure/nether_fortress"), names(&|_| true));
    assert_eq!(java("has_structure/ruined_portal_nether"), names(&|_| true));
    let fortress = &fixture()["structures"]["fortress"];
    assert_eq!(fortress["type"], "minecraft:fortress");
    let fossil = &fixture()["structures"]["nether_fossil"];
    assert_eq!(fossil["terrain_adaptation"], "beard_thin");
}

#[test]
fn block_tags_the_features_test_match_java() {
    let tags = &fixture()["block_tags"];
    let list = |t: &str| -> Vec<String> {
        tags[t].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect()
    };
    assert_eq!(list("nylium"), ["crimson_nylium", "warped_nylium"]);
    for b in [nb::CRIMSON_NYLIUM, nb::WARPED_NYLIUM] {
        assert!(nb::is_nylium(b));
    }
    // Mushrooms on nylium need no light check; VoxelCraft's mushroom test
    // takes nylium or any opaque block (mycelium and podzol are opaque).
    assert!(
        list("overrides_mushroom_light_requirement")
            .iter()
            .all(|n| n.ends_with("nylium") || n == "mycelium" || n == "podzol")
    );
}

#[test]
fn providers_sample_within_javas_bounds() {
    let mut rng = Rng(7);
    for p in [IntP::Trapezoid(-7, 7, 0), IntP::Trapezoid(-4, 1, 3), IntP::Uniform(-11, 0), IntP::BiasedToBottom(0, 9)] {
        let (lo, hi) = p.bounds();
        let mut seen = BTreeSet::new();
        for _ in 0..20_000 {
            let v = p.sample(&mut rng);
            assert!((lo..=hi).contains(&v), "{p:?} gave {v}");
            seen.insert(v);
        }
        assert_eq!(seen.len() as i32, hi - lo + 1, "{p:?} reaches every value");
    }
    // Trapezoids peak in the middle; biased_to_bottom favours its floor.
    let mut counts = [0; 15];
    for _ in 0..30_000 {
        counts[(IntP::Trapezoid(-7, 7, 0).sample(&mut rng) + 7) as usize] += 1;
    }
    assert!(counts[7] > counts[0] * 5 && counts[7] > counts[14] * 5, "{counts:?}");
    for h in [
        Height::Uniform(Anchor::AboveBottom(4), Anchor::BelowTop(4)),
        Height::Trapezoid(Anchor::Absolute(8), Anchor::Absolute(24)),
    ] {
        let (lo, hi) = h.bounds();
        for _ in 0..5000 {
            assert!((lo..=hi).contains(&h.sample(&mut rng)));
        }
    }
    assert_eq!(Height::Uniform(Anchor::AboveBottom(4), Anchor::BelowTop(4)).bounds(), (4, 123));
}

#[test]
fn paint_order_moves_only_the_tall_features_ahead() {
    let order = paint_order();
    assert_eq!(order.len(), PLACED.len());
    let names: Vec<&str> = order.iter().map(|&i| PLACED[i].name).collect();
    assert_eq!(names[..PAINT_FIRST.len()], PAINT_FIRST);
    // The rest keep Java's order.
    let rest: Vec<&str> = PLACED.iter().map(|p| p.name).filter(|n| !PAINT_FIRST.contains(n)).collect();
    assert_eq!(names[PAINT_FIRST.len()..], rest[..]);
}
