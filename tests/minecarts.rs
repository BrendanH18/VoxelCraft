//! v0.17 Track D1: the furnace minecart and the rail and cart gaps, against
//! Java 26.3's default (non-experimental) minecart behaviour. Constants come
//! from `fixtures/java/26.3/minecarts.json`
//! (`tools/extract_java_minecart_fixtures.py`).
use std::{collections::BTreeMap, sync::Arc};

use glam::{DVec3, IVec3};
use serde_json::Value;
use voxelcraft::{
    agent::{Agent, Command},
    entity::{
        Entities, MobKind,
        minecart::{self, CartKind, Minecart},
    },
    inventory::Stack,
    item::Item,
    java_registry,
    world::{
        World,
        block::{Block, RailShape},
        chunk::ChunkData,
        rails::{self, RailKind},
        redstone_blocks,
        terrain::Generator,
    },
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/minecarts.json")).unwrap()
}

/// Rails along +x at `Y`, on stone, from x = 0.
const Y: i32 = 140;
const Z: i32 = 5;

fn world() -> World {
    let mut w = World::new_remote(Arc::new(Generator::new(3)), 2);
    w.receive_chunk(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    w
}

fn track(w: &mut World, z: i32, length: i32) {
    for x in 0..length {
        w.set_block(IVec3::new(x, Y - 1, z), Block::STONE);
        w.set_block(IVec3::new(x, Y, z), Block::rail(RailShape::EastWest));
    }
}

fn on_rail(x: f64, z: i32) -> DVec3 {
    DVec3::new(x, Y as f64 + 0.0625, z as f64 + 0.5)
}

fn ticks(w: &mut World, e: &mut Entities, n: usize) {
    for _ in 0..n {
        e.tick_minecarts(w, &[]);
    }
}

fn cart(e: &Entities, id: u32) -> &Minecart {
    e.cart(id).expect("the cart is still there")
}

#[test]
fn fixture_constants_match_the_furnace_cart_and_sounds() {
    let f = fixture();
    let furnace = &f["furnace"];
    assert_eq!(furnace["fuel_ticks_per_item"], u64::from(minecart::FUEL_PER_ITEM));
    assert_eq!(furnace["max_fuel_ticks"], u64::from(minecart::MAX_FUEL));
    for name in furnace["fuel_items"].as_array().unwrap() {
        let item = java_registry::item(name.as_str().unwrap(), "").unwrap();
        assert!(minecart::is_furnace_fuel(item), "{name}");
    }
    assert!(!minecart::is_furnace_fuel(Item::STICK));
    let rolling = &f["sound"]["rolling"];
    let max = rolling["volume_max"].as_f64().unwrap() as f32;
    let clamp = rolling["speed_clamp"].as_f64().unwrap();
    assert_eq!(minecart::rolling_volume(0.005), 0.0, "silent below min_speed");
    assert_eq!(minecart::rolling_volume(2.0), clamp as f32 * max);
    assert_eq!(minecart::rolling_volume(0.2), 0.2 * max);
    assert_eq!(rolling["pitch"], 1.0);
    let inside = &f["sound"]["inside"];
    assert_eq!(minecart::inside_volume(0.4), 0.4 * inside["volume_max"].as_f64().unwrap() as f32);
    assert_eq!(minecart::inside_volume(3.0), inside["volume_max"].as_f64().unwrap() as f32);
    assert_eq!(f["tnt"]["protected_rails"].as_array().unwrap().len(), 4);
    for name in f["tnt"]["protected_rails"].as_array().unwrap() {
        let block = java_registry::block(name.as_str().unwrap(), "").unwrap();
        assert!(block.is_rail(), "{name}");
    }
}

#[test]
fn furnace_minecart_registers_with_its_recipe_item_and_drop() {
    let item = java_registry::item("furnace_minecart", "Minecart with Furnace").expect("registered");
    assert_eq!(item, Item::FURNACE_MINECART);
    assert_eq!(item.max_stack(), 1);
    assert_eq!(CartKind::from_item(item), Some(CartKind::Furnace));
    assert_eq!(CartKind::Furnace.item(), item);
    let mut w = world();
    track(&mut w, Z, 4);
    let mut e = Entities::new(1);
    let id = e.place_cart(&w, CartKind::Furnace, IVec3::new(1, Y, Z)).expect("placed on the rail");
    assert_eq!(cart(&e, id).kind, CartKind::Furnace);
    assert_eq!(cart(&e, id).slot_count(), 0);
    assert!(e.place_cart(&w, CartKind::Furnace, IVec3::new(1, Y + 1, Z)).is_none(), "rails only");
}

#[test]
fn coal_and_charcoal_fuel_up_to_the_cap_and_push_away_from_the_player() {
    let f = fixture();
    let per_item = f["furnace"]["fuel_ticks_per_item"].as_u64().unwrap() as u16;
    let cap = f["furnace"]["max_fuel_ticks"].as_u64().unwrap() as u16;
    let mut c = Minecart::new(1, CartKind::Furnace, on_rail(4.5, Z));
    assert!(!c.add_fuel(on_rail(2.5, Z), Item::STICK), "only #furnace_minecart_fuel");
    assert!(c.add_fuel(on_rail(2.5, Z), Item::COAL));
    assert_eq!(c.fuel, per_item);
    assert_eq!(c.push, DVec3::new(2.0, 0.0, 0.0), "away from the player, horizontal");
    assert!(c.has_fuel());
    let mut added = 1;
    while c.add_fuel(DVec3::new(4.5, 0.0, 9.0), Item::CHARCOAL) {
        added += 1;
    }
    assert_eq!(added, cap / per_item, "refused once it would pass the cap");
    assert_eq!(c.fuel, added * per_item);
    assert_eq!(c.push.with_y(0.0), DVec3::new(0.0, 0.0, Z as f64 + 0.5 - 9.0), "each fuelling resets the push");
    let mut rideable = Minecart::new(2, CartKind::Rideable, on_rail(4.5, Z));
    assert!(!rideable.add_fuel(on_rail(2.5, Z), Item::COAL));
}

#[test]
fn right_clicking_a_furnace_cart_takes_one_coal_and_never_mounts() {
    let mut w = world();
    let mut e = Entities::new(1);
    let mut agent = Agent::new(DVec3::new(5.5, 141.0, 2.5));
    agent.player.yaw = std::f32::consts::FRAC_PI_2;
    let id = e.spawn_cart(CartKind::Furnace, DVec3::new(5.5, 142.2, 4.5));
    agent.inventory.slots[0] = Some(Stack::new(Item::COAL, 3));
    agent.execute(Command::Place, &mut w, &mut e, &[]).unwrap();
    assert_eq!(agent.inventory.get(0), Some(Stack::new(Item::COAL, 2)), "one coal used");
    assert_eq!(cart(&e, id).fuel, minecart::FUEL_PER_ITEM);
    assert_eq!(cart(&e, id).push, DVec3::new(0.0, 0.0, 2.0), "pushed away from the player's feet");
    assert_eq!(agent.player.vehicle, None, "furnace carts carry nobody");
    // Anything else is a used click that changes nothing (`InteractionResult.SUCCESS`).
    let mut agent = Agent::new(DVec3::new(5.5, 141.0, 2.5));
    agent.player.yaw = std::f32::consts::FRAC_PI_2;
    agent.inventory.slots[0] = Some(Stack::new(Item::MINECART, 1));
    agent.execute(Command::Place, &mut w, &mut e, &[]).unwrap();
    assert_eq!(agent.inventory.get(0), Some(Stack::new(Item::MINECART, 1)));
    assert_eq!(cart(&e, id).fuel, minecart::FUEL_PER_ITEM);
}

#[test]
fn a_fuelled_furnace_cart_runs_at_half_speed_then_stops_when_the_fuel_is_gone() {
    let f = fixture();
    let land =
        f["motion"]["max_speed_land"].as_f64().unwrap() * f["furnace"]["max_speed_factor_land"].as_f64().unwrap();
    let mut w = world();
    track(&mut w, Z, 30);
    let mut e = Entities::new(1);
    let id = e.place_cart(&w, CartKind::Furnace, IVec3::new(2, Y, Z)).unwrap();
    let c = e.cart_mut(id).unwrap();
    assert!(c.add_fuel(on_rail(1.5, Z), Item::COAL));
    // A short burn: 40 ticks of fuel.
    c.fuel = 40;
    let mut last = cart(&e, id).pos.x;
    let mut steps = Vec::new();
    for _ in 0..40 {
        ticks(&mut w, &mut e, 1);
        let x = cart(&e, id).pos.x;
        steps.push(x - last);
        last = x;
    }
    assert!(steps.iter().all(|&s| s >= 0.0 && s <= land + 1e-9), "never past {land} a tick: {steps:?}");
    assert!(steps[30..].iter().all(|&s| (s - land).abs() < 1e-9), "held at the cap while it burns: {steps:?}");
    let c = cart(&e, id);
    assert_eq!(c.fuel, 0);
    assert_eq!(c.push, DVec3::ZERO, "out of fuel: no push");
    assert!(!c.has_fuel());
    // Coasting: 0.98 then 0.96 a tick, so it soon stops.
    ticks(&mut w, &mut e, 200);
    assert!(cart(&e, id).vel.with_y(0.0).length() < 1e-3);
}

#[test]
fn a_furnace_cart_pushes_a_cart_ahead_and_keeps_most_of_its_speed() {
    let mut w = world();
    track(&mut w, Z, 30);
    let mut e = Entities::new(1);
    let furnace = e.place_cart(&w, CartKind::Furnace, IVec3::new(2, Y, Z)).unwrap();
    let ahead = e.place_cart(&w, CartKind::Rideable, IVec3::new(5, Y, Z)).unwrap();
    let c = e.cart_mut(furnace).unwrap();
    assert!(c.add_fuel(on_rail(1.5, Z), Item::COAL));
    let start = cart(&e, ahead).pos.x;
    ticks(&mut w, &mut e, 60);
    assert!(cart(&e, ahead).pos.x > start + 2.0, "the cart ahead was pushed along");
    assert!(cart(&e, furnace).pos.x < cart(&e, ahead).pos.x, "the furnace stays behind it");
}

#[test]
fn carts_push_only_along_their_facing_as_java_aligns_them() {
    // Two still carts side by side on parallel tracks face +x (yaw 0), so
    // the line between them is crosswise and neither pushes the other.
    let mut w = world();
    track(&mut w, Z, 8);
    track(&mut w, Z + 1, 8);
    let mut e = Entities::new(1);
    let a = e.spawn_cart(CartKind::Rideable, on_rail(3.5, Z));
    let b = e.spawn_cart(CartKind::Rideable, on_rail(3.5, Z + 1));
    ticks(&mut w, &mut e, 5);
    assert_eq!(cart(&e, a).vel.with_y(0.0), DVec3::ZERO);
    assert_eq!(cart(&e, b).vel.with_y(0.0), DVec3::ZERO);
    // End to end on one track, they push apart along it.
    let mut e = Entities::new(1);
    let a = e.spawn_cart(CartKind::Rideable, on_rail(3.5, Z));
    let b = e.spawn_cart(CartKind::Rideable, on_rail(4.3, Z));
    ticks(&mut w, &mut e, 1);
    assert!(cart(&e, a).vel.x < 0.0 && cart(&e, b).vel.x > 0.0, "pushed apart");
}

#[test]
fn a_moving_cart_takes_in_a_mob_but_never_an_iron_golem() {
    for (kind, rides) in [(MobKind::Zombie, true), (MobKind::IronGolem, false), (MobKind::Ghast, true)] {
        let mut w = world();
        track(&mut w, Z, 20);
        let mut e = Entities::new(1);
        let id = e.spawn_cart(CartKind::Rideable, on_rail(3.5, Z));
        e.cart_mut(id).unwrap().vel = DVec3::new(0.3, 0.0, 0.0);
        e.spawn(kind, DVec3::new(5.0, Y as f64, Z as f64 + 0.5));
        ticks(&mut w, &mut e, 6);
        assert_eq!(e.mobs[0].riding == Some(id), rides, "{kind:?}");
    }
}

#[test]
fn wet_rails_keep_their_water_and_slow_carts_like_water() {
    let f = fixture();
    let water_cap = f["motion"]["max_speed_water"].as_f64().unwrap();
    let mut w = world();
    for x in 0..20 {
        w.set_block(IVec3::new(x, Y - 1, Z), Block::STONE);
        w.set_block(IVec3::new(x, Y, Z), Block::rail(RailShape::EastWest).with_water(true));
    }
    assert!(w.get_block(IVec3::new(3, Y, Z)).unwrap().is_waterlogged());
    let mut e = Entities::new(1);
    let id = e.spawn_cart(CartKind::Rideable, on_rail(2.5, Z));
    e.cart_mut(id).unwrap().vel = DVec3::new(0.4, 0.0, 0.0);
    let mut last = cart(&e, id).pos.x;
    for _ in 0..10 {
        ticks(&mut w, &mut e, 1);
        let x = cart(&e, id).pos.x;
        assert!(x - last <= water_cap + 1e-9, "a cart on a wet rail is in water: {}", x - last);
        last = x;
    }
    // Every rail kind takes the property, and power and shape keep it.
    for kind in [RailKind::Powered, RailKind::Detector, RailKind::Activator] {
        let dry = rails::special(kind, RailShape::NorthSouth, false);
        let wet = dry.with_water(true);
        assert!(wet.is_waterlogged() && wet.is_rail(), "{kind:?}");
        assert_eq!(rails::kind(wet), Some(kind));
        assert_eq!(
            rails::with_shape(wet, RailShape::EastWest),
            rails::with_shape(dry, RailShape::EastWest).with_water(true)
        );
    }
    let at = IVec3::new(10, Y + 4, 10);
    w.set_block(at - IVec3::Y, Block::STONE);
    w.set_block(at, rails::POWERED_RAIL.with_water(true));
    w.set_block(at + IVec3::X, redstone_blocks::REDSTONE_BLOCK);
    for _ in 0..4 {
        w.tick_redstone();
    }
    let powered = w.get_block(at).unwrap();
    assert!(rails::is_powered(powered) && powered.is_waterlogged(), "power keeps the water: {powered:?}");
}

#[test]
fn a_primed_tnt_carts_blast_spares_rails_and_what_holds_them() {
    let mut w = world();
    let floor = Y - 1;
    for x in 0..12 {
        for z in 0..12 {
            w.set_block(IVec3::new(x, floor, z), Block::STONE);
        }
    }
    for x in 0..12 {
        w.set_block(IVec3::new(x, Y, 5), Block::rail(RailShape::EastWest));
    }
    let center = DVec3::new(5.5, Y as f64 + 0.0625, 5.5);
    let plain = w.exploded_cells(center, 4.0);
    assert!(plain.contains(&IVec3::new(5, Y, 5)) && plain.contains(&IVec3::new(5, floor, 5)));
    let spared = w.rail_sparing_exploded_cells(center, 4.0);
    assert!(!spared.is_empty());
    for x in 0..12 {
        assert!(!spared.contains(&IVec3::new(x, Y, 5)), "rail at x = {x}");
        assert!(!spared.contains(&IVec3::new(x, floor, 5)), "the stone under it");
    }
    assert!(spared.contains(&IVec3::new(5, floor, 4)), "the floor beside the track still breaks");
}

#[test]
fn furnace_fuel_push_and_mob_passengers_survive_save_and_reload() {
    let mut w = world();
    track(&mut w, Z, 20);
    let mut e = Entities::new(1);
    let furnace = e.place_cart(&w, CartKind::Furnace, IVec3::new(2, Y, Z)).unwrap();
    assert!(e.cart_mut(furnace).unwrap().add_fuel(on_rail(1.0, Z), Item::COAL));
    ticks(&mut w, &mut e, 3);
    let ridden = e.place_cart(&w, CartKind::Rideable, IVec3::new(12, Y, Z)).unwrap();
    e.spawn(MobKind::Pig, on_rail(12.5, Z));
    e.spawn(MobKind::Villager, on_rail(12.5, Z));
    for m in &mut e.mobs {
        m.riding = Some(ridden);
    }
    let before = cart(&e, furnace);
    let (fuel, push) = (before.fuel, before.push);
    assert!(fuel > 0 && push.x > 0.0);
    // The level file and LAN both carry the serialized cart.
    let text = before.serialize();
    let copy = Minecart::deserialize(&text).unwrap();
    assert_eq!((copy.fuel, copy.push.x, copy.push.z), (fuel, push.x, push.z));
    assert!(copy.has_fuel(), "a guest sees it lit");
    // Older carts keep their fifteen fields.
    assert_eq!(cart(&e, ridden).serialize().split(',').count(), 15);
    let mut props = BTreeMap::new();
    e.save_props(&mut props);
    let mut loaded = Entities::new(1);
    loaded.load_props(&props);
    let c = cart(&loaded, furnace);
    assert_eq!((c.fuel, c.push.x, c.push.z), (fuel, push.x, push.z));
    for kind in [MobKind::Pig, MobKind::Villager] {
        let m = loaded.mobs.iter().find(|m| m.kind == kind).expect("saved");
        assert_eq!(m.riding, Some(ridden), "{kind:?} came back in its cart");
    }
    ticks(&mut w, &mut loaded, 1);
    // v1.0 Track 7b: a mob sits on the cart's attachment point less its
    // own vehicle attachment, not on a player's seat (0.6 lower).
    let c = cart(&loaded, ridden);
    assert!(loaded.mobs.iter().any(|m| m.pos == c.mob_seat(0, m)), "seated after the reload");
}
