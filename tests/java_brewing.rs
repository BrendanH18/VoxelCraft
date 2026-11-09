//! Two-way comparison with the pinned official Java 26.3 brewing recipes.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use voxelcraft::{
    inventory::Stack,
    item::Item,
    potion::Potion,
    world::brewing::{brew, brew_item, brew_stack},
};
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/brewing.json")).unwrap()
}
fn gaps() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/registry_gaps.json")).unwrap()
}
fn bottle(kind: &str, p: Potion) -> Stack {
    match kind {
        "potion" => Stack::new(Item::potion(p), 1),
        "splash_potion" => Stack::new(Item::splash_potion(p), 1),
        "lingering_potion" => Stack::lingering_potion(p),
        _ => panic!("unknown container {kind}"),
    }
}
#[test]
fn every_reference_mix_and_every_registered_mix_match() {
    let f = fixture();
    let g = gaps();
    let mut expected = BTreeMap::new();
    let mut checked = 0;
    let mut pending = 0;
    for r in f["recipes"].as_array().unwrap() {
        let text = |key: &str| r[key].as_str().unwrap();
        let input = Potion::from_id(text("input"));
        let result = Potion::from_id(text("result"));
        let ingredient = voxelcraft::java_registry::item(text("ingredient"), "");
        for (id, potion) in [(text("input"), input), (text("result"), result)] {
            if potion.is_none() {
                assert!(g["potions"].get(id).is_some(), "unowned potion {id}");
            }
        }
        if ingredient.is_none() {
            assert!(g["items"].get(text("ingredient")).is_some(), "unowned reagent {r}");
        }
        let (Some(p), Some(i), Some(out)) = (input, ingredient, result) else {
            pending += 1;
            continue;
        };
        let source = bottle(text("container"), p);
        let dest = bottle(text("result_container"), out);
        assert_eq!(brew_stack(source, i), Some(dest), "{}", text("path"));
        if text("container") != "lingering_potion" {
            assert_eq!(brew_item(source.item, i), Some(dest.item), "{r}");
        }
        if text("container") == "potion" && text("result_container") == "potion" {
            assert_eq!(brew(p, i), Some(out), "{r}");
        }
        assert!(expected.insert((text("container").to_string(), p.0, i.0), dest).is_none());
        checked += 1;
    }
    // Enumerate all registered reagents, including block items: no extra engine mix.
    let ingredients: BTreeSet<_> = Item::creative_palette().map(|i| i.0).collect();
    let mut actual_count = 0;
    for c in ["potion", "splash_potion", "lingering_potion"] {
        for p in Potion::all() {
            for &id in &ingredients {
                let actual = brew_stack(bottle(c, p), Item(id));
                assert_eq!(
                    actual,
                    expected.get(&(c.to_string(), p.0, id)).copied(),
                    "extra/missing mix {c} {} + {}",
                    p.info().id,
                    Item(id).name()
                );
                actual_count += usize::from(actual.is_some());
            }
        }
    }
    assert_eq!(actual_count, checked);
    println!("brewing: {checked} registered recipes checked; {pending} recipes owned by gaps");
}
#[test]
fn invisibility_metadata_and_legacy_item_ids_remain_stable() {
    for r in fixture()["invisibility"].as_array().unwrap() {
        let p = Potion::from_id(r["id"].as_str().unwrap()).unwrap();
        assert_eq!(
            p.info().effect,
            Some((
                voxelcraft::simulation::effects::Effect::Invisibility,
                r["amplifier"].as_u64().unwrap() as u8,
                r["ticks"].as_u64().unwrap() as u32
            ))
        );
    }
    for i in 0..36 {
        assert_eq!(Item::potion(Potion(i)).0, 400 + u16::from(i));
        assert_eq!(Item::splash_potion(Potion(i)).0, 436 + u16::from(i));
    }
    for i in 36..39 {
        assert_eq!(Item::potion(Potion(i)).0, 1041 + u16::from(i) - 36);
        assert_eq!(Item::splash_potion(Potion(i)).0, 1044 + u16::from(i) - 36);
    }
}

#[test]
fn invisibility_hides_skin_but_keeps_equipment_and_reduces_detection() {
    use glam::DVec3;
    use voxelcraft::{
        entity::{
            PlayerId, Target,
            model::{PlayerAppearance, build_player},
        },
        player::Player,
        simulation::effects::Effect,
    };
    let f = fixture();
    let vis = &f["visibility"];
    let mut target = Target::new(PlayerId::HOST, DVec3::ZERO, true);
    assert_eq!(target.visibility_percent(), 1.);
    target.effects = 1 << Effect::Invisibility as u32;
    for n in 0..=4 {
        target.armor_pieces = n;
        // Java float max(armorSlots / 4, 0.1), widened before multiplication.
        let expected = vis["invisible"].as_f64().unwrap()
            * f64::from(
                (f32::from(n) / vis["humanoid_armor_slots"].as_f64().unwrap() as f32)
                    .max(vis["minimum_armor_fraction"].as_f64().unwrap() as f32),
            );
        assert_eq!(target.visibility_percent(), expected);
        assert_eq!(target.detection_range(16.), (16. * expected).max(vis["minimum_detection_range"].as_f64().unwrap()));
    }
    let mut other = Target::new(PlayerId(1), DVec3::new(10., 0., 0.), true);
    target.armor_pieces = 0;
    target.pos = DVec3::new(4., 0., 0.);
    let mut ctx = voxelcraft::entity::Ctx {
        players: vec![target, other],
        daylight: 0.,
        spawn_light: voxelcraft::simulation::lighting::SpawnLight::NIGHT,
        spawning: false,
        raining: false,
        dimension: voxelcraft::world::terrain::Dimension::Overworld,
    };
    assert_eq!(ctx.nearest_detectable_target(DVec3::ZERO, 16.).unwrap().id, other.id);
    other.pos = DVec3::new(1., 0., 0.);
    ctx.players = vec![target, other];
    assert_eq!(ctx.nearest_detectable_target(DVec3::ZERO, 16.).unwrap().id, PlayerId(1));
    target.armor_pieces = 4;
    target.sneaking = true;
    assert_eq!(target.visibility_percent(), 0.8 * 0.7);
    let player = Player::new(DVec3::ZERO);
    let draw = |look: PlayerAppearance| {
        let mut out = Vec::new();
        build_player(&player, DVec3::ZERO, DVec3::new(0., 0., 8.), (1., 0.), 0., look, &mut out);
        out
    };
    assert!(!draw(PlayerAppearance::default()).is_empty());
    assert!(draw(PlayerAppearance { invisible: true, ..Default::default() }).is_empty());
    assert!(!draw(PlayerAppearance { invisible: true, held: Some(Item::STICK), ..Default::default() }).is_empty());
    let mut effects = voxelcraft::simulation::effects::Effects::default();
    effects.add(Effect::Invisibility, 0, 1);
    effects.tick(0.05);
    assert!(!effects.has(Effect::Invisibility));
}
