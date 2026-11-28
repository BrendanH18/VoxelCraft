//! `world::nether_surface` against Java 26.3's surface rules and noises
//! (`tests/fixtures/java/26.3/nether_generation.json`, from
//! `tools/extract_java_nether_fixtures.py`).

use serde_json::Value;

use super::*;

fn fixture() -> &'static Value {
    static DATA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/nether_generation.json"))
            .expect("nether generation fixture")
    })
}

/// `NormalNoise.computeParityBaseAmplitude`: the 26.3 base amplitude that
/// gives a 1.21 `NormalNoise(firstOctave, amplitudes)` its values.
fn parity_base_amplitude(amplitudes: &[f64]) -> f64 {
    let n = amplitudes.len() as i32;
    let mut amplitude = 0.5f64.powi(-(n - 1)) / (0.5f64.powi(-n) - 1.0);
    let mut octaves = Vec::new();
    for &a in amplitudes {
        if a != 0.0 {
            octaves.push((amplitude * a).abs());
        }
        amplitude *= 0.5;
    }
    let target: f64 = octaves.iter().sum();
    let deviation = octaves.iter().map(|a| (0.270_224_783_124_521_1 * a).powi(2)).sum::<f64>().sqrt();
    let new = target / 3.0 / (deviation * 2f64.sqrt());
    let lo = amplitudes.iter().position(|&a| a != 0.0).unwrap();
    let hi = amplitudes.iter().rposition(|&a| a != 0.0).unwrap();
    let old = 0.5 / 3.0 / (0.1 * (1.0 + 1.0 / (hi - lo + 1) as f64));
    old / new
}

#[test]
fn noises_are_the_1_21_layouts_26_3_stores_as_parity_parameters() {
    let noises = &fixture()["surface_noises"];
    let ours = [
        ("surface", SURFACE),
        ("soul_sand_layer", SOUL_SAND_LAYER),
        ("gravel_layer", GRAVEL_LAYER),
        ("patch", PATCH),
        ("netherrack", NETHERRACK),
        ("nether_wart", NETHER_WART),
        ("nether_state_selector", NETHER_STATE_SELECTOR),
    ];
    assert_eq!(noises.as_object().unwrap().len(), ours.len());
    for (name, (first, amplitudes)) in ours {
        let n = &noises[name];
        assert_eq!(n["base_octave"].as_i64(), Some(i64::from(first)), "{name}");
        assert_eq!(n["octave_count"].as_u64().unwrap_or(1), amplitudes.len() as u64, "{name}");
        let modifiers: Vec<f64> = n["amplitude_modifiers"]
            .as_array()
            .map_or_else(|| vec![1.0; amplitudes.len()], |a| a.iter().map(|v| v.as_f64().unwrap()).collect());
        assert_eq!(modifiers, amplitudes, "{name}");
        assert_eq!(n["normalize"], Value::Null, "{name}: normalised (the default)");
        let base = n["base_amplitude"].as_f64().unwrap();
        assert!((parity_base_amplitude(amplitudes) - base).abs() < 1e-12, "{name}: {base}");
    }
    let settings = &fixture()["noise_settings"];
    assert_eq!(settings["sea_level"], 32, "the lava sea fills y 31 and below");
    assert_eq!(settings["default_block"], "minecraft:netherrack");
    assert_eq!(settings["default_fluid"], "minecraft:lava");
    assert_eq!(settings["material_rule"], "minecraft:nether");
    assert_eq!(settings["noise"]["min_y"], 0);
    assert_eq!(settings["noise"]["height"], 128);
    assert_eq!(LAVA_SEA, 31);
}

/// One block's view, as `MaterialRuleContext` holds it.
struct Context<'a> {
    biome: NetherBiome,
    y: i32,
    above: i32,
    below: i32,
    depth: i32,
    noise: &'a dyn Fn(&str) -> f64,
}

fn anchor(a: &Value) -> i32 {
    if let Some(y) = a["absolute"].as_i64() {
        y as i32
    } else if let Some(n) = a["above_bottom"].as_i64() {
        n as i32
    } else {
        // `BelowTop`: min y + height - 1 - offset.
        127 - a["below_top"].as_i64().expect("an anchor") as i32
    }
}

/// `MaterialCondition` from the fixture's JSON.
fn condition(c: &Value, cx: &Context) -> bool {
    if let Some(name) = c.as_str() {
        let name = name.trim_start_matches("minecraft:");
        return condition(&fixture()["material_conditions"][name], cx);
    }
    match c["type"].as_str().unwrap().trim_start_matches("minecraft:") {
        "biome" => c["biome_is"].as_str().unwrap().trim_start_matches("minecraft:") == cx.biome.name(),
        "not" => !condition(&c["invert"], cx),
        "hole" => cx.depth <= 0,
        "y_above" => {
            let add = if c["add_stone_depth"].as_bool().unwrap() { cx.above } else { 0 };
            let m = c["surface_depth_multiplier"].as_i64().unwrap() as i32;
            cx.y + add >= anchor(&c["anchor"]) + cx.depth * m
        }
        "stone_depth" => {
            assert_eq!(c["secondary_depth_range"], 0);
            let depth = if c["surface_type"] == "ceiling" { cx.below } else { cx.above };
            let surface = if c["add_surface_depth"].as_bool().unwrap() { cx.depth } else { 0 };
            depth <= 1 + c["offset"].as_i64().unwrap() as i32 + surface
        }
        "noise_threshold" => {
            assert_ne!(c["is_3d"], true);
            let v = (cx.noise)(c["noise"].as_str().unwrap().trim_start_matches("minecraft:"));
            v >= c["min_threshold"].as_f64().unwrap() && v <= c["max_threshold"].as_f64().unwrap()
        }
        "vertical_gradient" => {
            // Only the bedrock rules; the tested heights are outside their ramps.
            let (t, f) = (anchor(&c["true_at_and_below"]), anchor(&c["false_at_and_above"]));
            assert!(cx.y <= t || cx.y >= f, "y {} inside a gradient", cx.y);
            cx.y <= t
        }
        other => panic!("condition {other} is not in the Nether's rules"),
    }
}

/// `MaterialRule` from the fixture's JSON: a block id, or `None`.
fn apply(r: &Value, cx: &Context) -> Option<String> {
    if let Some(name) = r.as_str() {
        return apply(&fixture()["material_rules"][name.trim_start_matches("minecraft:")], cx);
    }
    match r["type"].as_str().unwrap().trim_start_matches("minecraft:") {
        "sequence" => r["sequence"].as_array().unwrap().iter().find_map(|r| apply(r, cx)),
        "condition" => condition(&r["if_true"], cx).then(|| apply(&r["then_run"], cx)).flatten(),
        "block" => Some(r["result_state"].as_str().unwrap().trim_start_matches("minecraft:").to_string()),
        other => panic!("rule {other} is not in the Nether's rules"),
    }
}

fn java_id(b: Block) -> &'static str {
    match b {
        Block::NETHERRACK => "netherrack",
        Block::BASALT => "basalt",
        Block::BLACKSTONE => "blackstone",
        Block::GRAVEL => "gravel",
        Block::SOUL_SAND => "soul_sand",
        Block::LAVA => "lava",
        nb::SOUL_SOIL => "soul_soil",
        nb::CRIMSON_NYLIUM => "crimson_nylium",
        nb::WARPED_NYLIUM => "warped_nylium",
        nb::NETHER_WART_BLOCK => "nether_wart_block",
        nb::WARPED_WART_BLOCK => "warped_wart_block",
        other => panic!("{} is no surface block", other.name()),
    }
}

/// Every combination of biome, height, stone depths, surface depth and
/// noise values around the rules' thresholds gives Java's block.
#[test]
fn surface_rules_match_javas_material_rule() {
    // Each threshold and the value just below it.
    let values = [-0.5, -0.012, 0.0, 0.539, 0.54, 1.169, 1.17];
    let names = |b: NetherBiome| match b {
        NetherBiome::NetherWastes => ["soul_sand_layer", "gravel_layer"],
        NetherBiome::SoulSandValley | NetherBiome::BasaltDeltas => ["patch", "nether_state_selector"],
        _ => ["netherrack", "nether_wart"],
    };
    let mut checked = 0;
    for biome in NetherBiome::ALL {
        for y in [5, 28, 29, 30, 31, 32, 33, 34, 35, 36, 60, ROOF - 6] {
            for above in [1, 2, 3, 4, 5, 6, 9, 30] {
                for below in [1, 2, 4, 6, 9, 40] {
                    for depth in [-1, 0, 1, 2, 3, 5, 8] {
                        for &a in &values {
                            for &b in &values {
                                let [na, nb_] = names(biome);
                                let noise = |n: &str| {
                                    if n == na {
                                        a
                                    } else if n == nb_ {
                                        b
                                    } else {
                                        panic!("{biome:?} reads {n}")
                                    }
                                };
                                let cx = Context { biome, y, above, below, depth, noise: &noise };
                                let java = apply(&fixture()["material_rules"]["nether"], &cx).expect("a block");
                                let ours = rule(biome, y, above, below, SurfaceColumn { depth, noise: [a, b] });
                                assert_eq!(
                                    java_id(ours),
                                    java,
                                    "{biome:?} y {y} above {above} below {below} depth {depth} noises {a} {b}"
                                );
                                checked += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(checked > 500_000, "{checked}");
}

/// `MaterialSystem.buildSurface`'s column walk, for a column of rock
/// (`true`) and open cells (lava at y 31 and below, air above).
fn java_depths(solid: &[bool; 128]) -> Vec<Option<(i32, i32)>> {
    let block = |y: i32| -> u8 {
        // 0 air, 1 lava, 2 stone.
        if !(0..128).contains(&y) {
            0
        } else if solid[y as usize] {
            2
        } else if y <= LAVA_SEA {
            1
        } else {
            0
        }
    };
    let mut out = vec![None; 128];
    let mut above = 0;
    let mut next_ceiling = i32::MAX;
    for y in (0..=128).rev() {
        match block(y) {
            0 => above = 0,
            1 => {}
            _ => {
                if next_ceiling >= y {
                    next_ceiling = i32::MIN;
                    for look in (-1..y).rev() {
                        if block(look) != 2 {
                            next_ceiling = look + 1;
                            break;
                        }
                    }
                }
                above += 1;
                out[y as usize] = Some((above, y - next_ceiling + 1));
            }
        }
    }
    out
}

#[test]
fn stone_depths_follow_javas_column_walk() {
    let mut rng = crate::world::structure::Rng(7);
    for case in 0..400 {
        // Runs of rock and open cells, denser near the lava sea.
        let mut solid = [false; 128];
        let mut y = 0;
        let mut rock = true;
        while y < 128 {
            let run = 1 + rng.below(if case % 2 == 0 { 6 } else { 20 }) as usize;
            for s in solid.iter_mut().skip(y).take(run) {
                *s = rock;
            }
            y += run;
            rock = !rock;
        }
        solid[0] = true;
        solid[127] = true;
        let bits = (0..128).filter(|&y| solid[y]).fold(0u128, |b, y| b | 1 << y);
        for (y, java) in java_depths(&solid).into_iter().enumerate() {
            let Some((above, below)) = java else { continue };
            let y = y as i32;
            assert_eq!(stone_depth_above(bits, y), above, "case {case} y {y} above");
            assert_eq!(stone_depth_below(bits, y), below, "case {case} y {y} below");
        }
    }
}

#[test]
fn surface_depth_holes_and_noise_spread_look_like_javas() {
    let noises = SurfaceNoises::new(12345);
    let (mut holes, mut deep, mut n) = (0, 0, 0);
    for z in (-4000..4000).step_by(37) {
        for x in (-4000..4000).step_by(41) {
            let d = noises.depth(x, z);
            holes += usize::from(d <= 0);
            deep += usize::from(d >= 5);
            n += 1;
        }
    }
    // Java's surface noise spreads the depth around 3: holes and deep
    // patches are both a few percent of columns.
    let share = |k: usize| k as f64 / n as f64;
    assert!((0.005..0.2).contains(&share(holes)), "holes {}", share(holes));
    assert!((0.005..0.3).contains(&share(deep)), "deep {}", share(deep));
}
