//! v0.18 Track 6 against Java 26.3's `entity_shadows.json` fixture (from
//! `tools/extract_java_shadow_fixtures.py`): every mob's shadow radius and
//! strength, the baby and size rules of the renderers that override
//! `getShadowRadius`, the dropped-item, orb and player shadows,
//! `extractShadow`'s constants and the Entity Shadows option. The quads
//! themselves are unit tests of `entity::shadow`.
use glam::DVec3;
use serde_json::Value;
use voxelcraft::entity::shadow::{self, ITEM, PLAYER_RADIUS};
use voxelcraft::entity::{Mob, MobKind};

fn fixture() -> Value {
    let path = format!("{}/tests/fixtures/java/26.3/entity_shadows.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn java_id(kind: MobKind) -> String {
    kind.name().replace(' ', "_")
}

fn close(a: f32, b: f64) -> bool {
    (f64::from(a) - b).abs() < 1e-6
}

/// `HappyGhast` -> `happy_ghast`.
fn snake(class: &str) -> String {
    let mut out = String::new();
    for (i, c) in class.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

#[test]
fn every_mob_has_its_java_renderers_radius_and_strength() {
    let java = fixture();
    let entities = java["entities"].as_object().unwrap();
    for kind in MobKind::ALL {
        let id = java_id(kind);
        let e = entities.get(&id).unwrap_or_else(|| panic!("{id} has a Java renderer"));
        let radius = e["radius"].as_f64().unwrap();
        assert!(close(shadow::base_radius(kind), radius), "{id}: {} vs Java {radius}", shadow::base_radius(kind));
        assert!(e["strength_overrides"].as_array().unwrap().is_empty(), "{id}: no strength override");
        assert_eq!(e["strength"].as_f64(), Some(1.0), "{id}");
        let adult = Mob::new(kind, DVec3::ZERO, 0.0);
        let overrides: Vec<&str> =
            e["radius_overrides"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        match overrides.first().copied() {
            // `AbstractCubeMobRenderer`: size * 0.25, nothing else.
            Some("AbstractCubeMobRenderer") => {
                let k = java["overrides"]["AbstractCubeMobRenderer"]["literals"][0].as_f64().unwrap();
                let mut big = Mob::new(kind, DVec3::ZERO, 0.0);
                big.size = 4;
                assert!(close(shadow::mob_radius(&big), 4.0 * k), "{id}");
            }
            // `PufferfishRenderer`: 0.1 + 0.1 * puff state.
            Some("PufferfishRenderer") => {
                let l = java["overrides"]["PufferfishRenderer"]["literals"].as_array().unwrap();
                assert!(close(shadow::mob_radius(&adult), l[0].as_f64().unwrap()), "{id}");
            }
            // `MobRenderer`: times the age scale, then a renderer's own baby factor.
            Some(first) => {
                assert!(overrides.contains(&"MobRenderer"), "{id}: {overrides:?}");
                assert!(close(shadow::mob_radius(&adult), radius), "{id} adult");
                let factor = match first {
                    "MobRenderer" => 1.0,
                    own => java["overrides"][own]["literals"][0].as_f64().unwrap(),
                };
                let ages = java["age_scales"].as_object().unwrap();
                let age = ages
                    .iter()
                    .find(|(class, _)| snake(class) == id)
                    .map_or(ages["LivingEntity"].as_f64().unwrap(), |(_, v)| v.as_f64().unwrap());
                assert!(close(shadow::baby_age_scale(kind), age), "{id} baby age scale");
                let mut baby = Mob::new(kind, DVec3::ZERO, 0.0);
                baby.baby = true;
                assert!(
                    (f64::from(shadow::mob_radius(&baby)) - radius * age * factor).abs() < 1e-5,
                    "{id} baby: {} vs {}",
                    shadow::mob_radius(&baby),
                    radius * age * factor
                );
            }
            None => panic!("{id}: a mob renderer applies the age scale"),
        }
    }
}

#[test]
fn items_orbs_and_players_cast_javas_shadows() {
    let java = fixture();
    for id in ["item", "experience_orb"] {
        let e = &java["entities"][id];
        assert!(close(ITEM.0, e["radius"].as_f64().unwrap()) && close(ITEM.1, e["strength"].as_f64().unwrap()), "{id}");
    }
    assert!(close(PLAYER_RADIUS, java["entities"]["player"]["radius"].as_f64().unwrap()));
    assert_eq!(java["entities"]["player"]["strength"].as_f64(), Some(1.0));
}

#[test]
fn shadow_constants_and_option_match_java() {
    let java = fixture();
    let c = &java["constants"];
    assert!(close(shadow::MAX_RADIUS, c["max_radius"].as_f64().unwrap()));
    assert_eq!(shadow::FADE_DISTANCE_SQ, c["distance_squared"].as_f64().unwrap());
    assert!(close(shadow::FALLOFF_PER_BLOCK, c["falloff_per_block"].as_f64().unwrap()));
    assert_eq!(u64::from(shadow::MIN_BRIGHTNESS), c["min_brightness"].as_u64().unwrap());
    // `Lightmap.getBrightness` at full light and with no ambient light is 1.
    assert_eq!(shadow::brightness(15, 0.0), 1.0);
    assert!((shadow::brightness(8, 0.0) - (8.0 / 15.0) / (4.0 - 3.0 * 8.0 / 15.0)).abs() < 1e-6);
    let o = &java["option"];
    assert_eq!(o["key"], shadow::OPTION_KEY);
    assert_eq!(o["label"], shadow::OPTION_LABEL);
    assert_eq!(o["default"].as_bool(), Some(shadow::OPTION_DEFAULT));
    // Java lists it right after Particles and Mipmap Levels in Quality & Performance.
    let order: Vec<&str> = o["quality_order"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    let at = order.iter().position(|&k| k == "entityShadows").unwrap();
    assert_eq!(&order[at - 2..at], ["particles", "mipmapLevels"]);
}
