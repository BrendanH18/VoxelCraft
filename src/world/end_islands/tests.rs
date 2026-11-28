//! The ported End terrain against Java 26.3's own End noise router
//! (`tests/fixtures/java/26.3/end_islands.json`, written by
//! `tools/extract_java_end_islands_fixtures.py` from the pinned jar).
use super::EndNoise;
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/end_islands.json")).unwrap()
}

fn seeds(v: &Value) -> impl Iterator<Item = (u64, &Value)> {
    v["seeds"].as_object().unwrap().iter().map(|(s, v)| (s.parse::<i64>().unwrap() as u64, v))
}

fn samples(v: &Value) -> impl Iterator<Item = (i32, i32, &str)> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|e| (e[0].as_i64().unwrap() as i32, e[1].as_i64().unwrap() as i32, e[2].as_str().unwrap()))
}

#[test]
fn island_functions_match_javas_floats_bit_for_bit() {
    let f = fixture();
    let mut checked = 0;
    for (seed, v) in seeds(&f) {
        let noise = EndNoise::new(seed);
        for (key, value) in
            [("outer", EndNoise::outer_islands as fn(&EndNoise, i32, i32) -> f32), ("islands", EndNoise::islands)]
        {
            for (x, z, bits) in samples(&v[key]) {
                let want = u32::from_str_radix(bits, 16).unwrap();
                let got = value(&noise, x, z);
                assert_eq!(got.to_bits(), want, "seed {seed} {key} at {x}, {z}: {got} vs {}", f32::from_bits(want));
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 3 * 2 * 961);
}

/// Solid ranges written as the fixture writes them.
fn runs(mask: u128) -> String {
    let mut out = Vec::new();
    let mut y = 0;
    while y < 128 {
        if mask & (1 << y) != 0 {
            let start = y;
            while y < 128 && mask & (1 << y) != 0 {
                y += 1;
            }
            out.push(format!("{start}-{}", y - 1));
        } else {
            y += 1;
        }
    }
    if out.is_empty() { "-".to_string() } else { out.join(",") }
}

#[test]
fn chunk_columns_match_javas_noise_columns() {
    let f = fixture();
    let mut checked = 0;
    let mut solid = 0;
    for (seed, v) in seeds(&f) {
        let noise = EndNoise::new(seed);
        let mut chunks = std::collections::HashMap::new();
        for (x, z, want) in samples(&v["columns"]) {
            let mask = chunks.entry((x >> 4, z >> 4)).or_insert_with(|| noise.chunk(x >> 4, z >> 4));
            let got = runs(mask[((z & 15) * 16 + (x & 15)) as usize]);
            assert_eq!(got, want, "seed {seed} column {x}, {z}");
            checked += 1;
            solid += usize::from(want != "-");
        }
    }
    assert_eq!(checked, 3 * 5 * 256);
    // The chunks hold the central island and outer islands, not only void.
    assert!(solid > checked / 2, "{solid} of {checked}");
}
