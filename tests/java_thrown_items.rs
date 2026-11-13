//! Java 26.3 thrown items against `fixtures/java/26.3/thrown_items.json`,
//! extracted by `tools/extract_java_thrown_fixtures.py`: the Bottle o'
//! Enchanting's registration, its throw through the shared use chain (the
//! session the desktop and controller drive, and the CLI agent), its orbs,
//! and the chest tables that name it. Flight, burst and dispenser cases are
//! unit tests in `entity/thrown.rs` and `world/automation.rs`; the cleric
//! offer is in `entity/villager.rs`.
use glam::{DVec3, IVec3};
use serde_json::Value;
use std::sync::Arc;
use voxelcraft::{
    agent::{Agent, Command},
    entity::{Entities, PlayerId, Rng, XpOrb, thrown},
    inventory::Stack,
    item::Item,
    session::{
        PlayerSession,
        interact::{UseContext, Used},
    },
    world::{
        World,
        block::Block,
        chunk::ChunkData,
        loot::{self, LootRandom},
        terrain::Generator,
    },
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/thrown_items.json")).unwrap()
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

#[test]
fn experience_bottle_registers_like_java() {
    let item = &fixture()["experience_bottle"]["item"];
    let bottle = voxelcraft::java_registry::item("experience_bottle", "Bottle o' Enchanting");
    assert_eq!(bottle, Some(Item::EXPERIENCE_BOTTLE));
    assert_eq!(Item::EXPERIENCE_BOTTLE, Item(1554));
    // `--give experience_bottle` and the trade table's display name both resolve.
    assert_eq!(Item::from_name("minecraft:experience_bottle"), Some(Item::EXPERIENCE_BOTTLE));
    assert_eq!(Item::from_name("bottle o' enchanting"), Some(Item::EXPERIENCE_BOTTLE));
    assert_eq!(loot::java_item("experience_bottle"), Some(Item::EXPERIENCE_BOTTLE));
    assert_eq!(u64::from(Item::EXPERIENCE_BOTTLE.max_stack()), item["max_stack"].as_u64().unwrap());
    assert_eq!(item["glint_override"], true);
    assert!(Item::EXPERIENCE_BOTTLE.has_glint());
    assert_eq!(item["rarity"], "uncommon");
    assert!(Item::EXPERIENCE_BOTTLE.is_uncommon() && !Item::EXPERIENCE_BOTTLE.is_rare());
    assert!(Item::creative_palette().any(|i| i == Item::EXPERIENCE_BOTTLE));
    assert!(Item::EXPERIENCE_BOTTLE.icon_layer().is_some());
    assert_eq!(thrown::Kind::from_item(Item::EXPERIENCE_BOTTLE), Some(thrown::Kind::ExperienceBottle));
    // Saves keep the numeric id: a stack round-trips.
    let stack = Some(Stack::new(Item::EXPERIENCE_BOTTLE, 64));
    let text = voxelcraft::inventory::stack_to_string(stack);
    assert_eq!(voxelcraft::inventory::stack_from_str(&text), Some(stack));
}

/// `ExperienceBottleItem.use` through `Projectile.shootFromRotation`: from
/// 0.1 below the eye, the look direction with 20 degrees added to the
/// vertical part, normalized, plus triangle noise, at 0.7 blocks a tick.
#[test]
fn thrown_bottles_follow_java_shoot_from_rotation() {
    let fx = fixture();
    let usage = &fx["experience_bottle"]["use"];
    let (offset, power, uncertainty) = (f(&usage["y_offset_degrees"]), f(&usage["power"]), f(&usage["uncertainty"]));
    let below = f(&fx["experience_bottle"]["entity"]["spawn_below_eye"]);
    let bound = f(&fx["throwable"]["shoot_noise"]) * uncertainty * power + 1e-6;
    let mut rng = Rng::new(3);
    let eye = DVec3::new(0.5, 70.0, 0.5);
    for pitch in [-80.0f64, -30.0, 0.0, 25.0, 60.0] {
        for yaw in [0.0f64, 70.0, 200.0] {
            let (p, y) = (pitch.to_radians(), yaw.to_radians());
            // Java: xd = -sin(yRot)cos(xRot), yd = -sin(xRot + offset), zd = cos(yRot)cos(xRot).
            let look = DVec3::new(-y.sin() * p.cos(), -p.sin(), y.cos() * p.cos());
            let heading = DVec3::new(look.x, -(p + offset.to_radians()).sin(), look.z).normalize() * power;
            let mut mean = DVec3::ZERO;
            const N: usize = 400;
            for _ in 0..N {
                let t = thrown::Thrown::launch(
                    thrown::Kind::ExperienceBottle,
                    PlayerId::HOST,
                    eye,
                    look,
                    DVec3::ZERO,
                    &mut rng,
                );
                assert!((t.pos - (eye - DVec3::Y * below)).length() < 1e-6);
                let v = t.velocity() / 20.0;
                assert!((v - heading).abs().max_element() <= bound, "pitch {pitch} yaw {yaw}: {v} vs {heading}");
                mean += v / N as f64;
            }
            assert!((mean - heading).length() < bound * 0.3, "pitch {pitch} yaw {yaw}: mean {mean} vs {heading}");
        }
    }
}

fn scene(count: u8) -> (World, PlayerSession, Entities) {
    let mut world = World::new_remote(Arc::new(Generator::new(7)), 2);
    world.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    world.set_block(IVec3::new(4, 144, 4), Block::STONE);
    let mut session = PlayerSession::new(PlayerId(6), DVec3::new(4.5, 145., 4.5));
    session.player.on_ground = true;
    session.inventory.slots[0] = Some(Stack::new(Item::EXPERIENCE_BOTTLE, count));
    (world, session, Entities::new(7))
}

/// The survival use: one bottle spent, one bottle thrown, through the
/// session's use chain and the CLI agent's `place` (its use command).
#[test]
fn players_and_agents_throw_bottles_through_the_shared_use_chain() {
    let (mut w, mut s, mut e) = scene(2);
    let used = s.use_held(0, &mut w, &mut e, &UseContext { sneaking: false, others: &[] }).unwrap().map(|u| u.used);
    assert!(matches!(used, Some(Used::ProjectileThrown { .. })), "{used:?}");
    assert_eq!(s.inventory.get(0).map(|s| s.count), Some(1));
    assert_eq!(e.thrown.len(), 1);
    assert_eq!(e.thrown[0].kind, thrown::Kind::ExperienceBottle);
    assert_eq!(e.thrown[0].owner, Some(PlayerId(6)));
    // Aimed level, the bottle climbs at about 20 degrees.
    let v = e.thrown[0].velocity();
    let climb = v.y.atan2(v.with_y(0.0).length()).to_degrees();
    assert!((15.0..25.0).contains(&climb), "{climb}");

    let (mut w, s, mut e) = scene(1);
    let mut cli = Agent::new(s.player.pos);
    cli.session.player.on_ground = true;
    cli.session.inventory.slots[0] = Some(Stack::new(Item::EXPERIENCE_BOTTLE, 1));
    cli.execute(Command::Place, &mut w, &mut e, &[]).unwrap();
    assert!(cli.session.inventory.get(0).is_none(), "the last bottle is spent");
    assert_eq!(e.thrown.len(), 1);
    assert_eq!(e.thrown[0].kind, thrown::Kind::ExperienceBottle);
}

/// `ExperienceOrb(level, pos, roughly, value)`: the random pop (±0.2
/// sideways, 0..0.4 up, blocks a tick) is flipped onto the side of the
/// direction, and the orb starts half its 0.5 box out along it.
#[test]
fn burst_orbs_pop_out_along_their_direction() {
    let orb = &fixture()["experience_orb"];
    let half = f(&orb["size"][0]) * 0.5;
    let (side, up) = (f(&orb["scatter"]["horizontal"]), f(&orb["scatter"]["vertical_max"]));
    let mut rng = Rng::new(11);
    let at = DVec3::new(1.0, 2.0, 3.0);
    for toward in [DVec3::Y, DVec3::NEG_X, DVec3::new(0.3, -0.8, 0.2)] {
        let mut flipped = false;
        for _ in 0..300 {
            let o = XpOrb::toward(7, at, toward, &mut rng);
            assert!((o.pos - (at + toward.normalize() * half)).length() < 1e-12);
            assert!(o.vel.dot(toward) >= 0.0, "{toward}: {}", o.vel);
            let v = o.vel.abs() / 20.0;
            assert!(v.x <= side + 1e-6 && v.z <= side + 1e-6 && v.y <= up + 1e-6, "{v}");
            flipped |= o.vel.y < 0.0;
        }
        // Straight up never needs a flip; sideways and downward ones do.
        assert_eq!(flipped, toward != DVec3::Y, "{toward}");
    }
    // No direction: Java's plain pop at the point.
    let o = XpOrb::toward(3, at, DVec3::ZERO, &mut rng);
    assert_eq!(o.pos, at);
}

/// Every chest table Java gives the bottle names it in the game, and none
/// more; rolled, each entry turns up at its weight share with Java's count.
#[test]
fn chest_loot_rolls_bottles_at_java_weights() {
    let fx = fixture();
    let bottle = &fx["experience_bottle"];
    let java: Vec<String> = bottle["loot_tables"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap().trim_start_matches("chests/").to_string())
        .collect();
    let game: Value = serde_json::from_str(include_str!("../src/world/loot_tables.json")).unwrap();
    let mut named: Vec<String> = game["tables"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(_, t)| t.to_string().contains("minecraft:experience_bottle"))
        .map(|(n, _)| n.clone())
        .collect();
    named.sort();
    let mut java_sorted = java.clone();
    java_sorted.sort();
    assert_eq!(named, java_sorted);
    for entry in bottle["loot"].as_array().unwrap() {
        let table = entry["table"].as_str().unwrap().trim_start_matches("chests/");
        let share = f(&entry["weight"]) / f(&entry["pool_weight"]);
        let rolls = (f(&entry["rolls"]["min"]) + f(&entry["rolls"]["max"])) / 2.0;
        let (lo, hi) = (entry["count"]["min"].as_u64().unwrap() as u8, entry["count"]["max"].as_u64().unwrap() as u8);
        const TABLES: u64 = 20_000;
        let (mut stacks, mut items) = (0u64, 0u64);
        let mut seen = [false; 65];
        for seed in 0..TABLES {
            for s in loot::roll_items(table, &mut LootRandom::new(seed * 7919 + 13), |_| None) {
                if s.item == Item::EXPERIENCE_BOTTLE {
                    assert!((lo..=hi).contains(&s.count), "{table}: {}", s.count);
                    seen[s.count as usize] = true;
                    stacks += 1;
                    items += u64::from(s.count);
                }
            }
        }
        let expected = rolls * share;
        let per_table = stacks as f64 / TABLES as f64;
        // Poisson-like spread: five standard errors.
        let tolerance = 5.0 * (expected / TABLES as f64).sqrt();
        assert!((per_table - expected).abs() < tolerance, "{table}: {per_table} per chest vs {expected}");
        assert!((lo..=hi).all(|c| seen[c as usize]), "{table}: every count {lo}..={hi}");
        let mean_count = items as f64 / stacks as f64;
        assert!((mean_count - f64::from(lo + hi) / 2.0).abs() < 0.05, "{table}: mean count {mean_count}");
    }
}
