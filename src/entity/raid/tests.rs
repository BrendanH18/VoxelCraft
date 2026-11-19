//! Raids, patrols, omens, bells and heroes against
//! `tests/fixtures/java/26.3/raids.json`, in both directions: each constant
//! and table is read back from the fixture, and the behaviour is checked
//! through the entity update (a raid from Bad Omen to Hero of the Village,
//! and a save and reload in the middle of one).
use super::*;
use crate::entity::villager::Profession;
use crate::entity::{Mob, Rng, Target};
use crate::physics::test_util::Grid;
use crate::session::PlayerSession;
use crate::simulation::effects::Active;
use crate::world::block::Block;
use crate::world::terrain::Dimension;

fn facts() -> Value {
    serde_json::from_str(include_str!("../../../tests/fixtures/java/26.3/raids.json")).unwrap()
}

fn int(v: &Value) -> i64 {
    v.as_i64().unwrap_or_else(|| panic!("not an integer: {v}"))
}

fn float(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

/// A flat village: stone below `FLOOR`, beds (village POIs) where the
/// villagers live, everything loaded.
struct Village {
    grid: Grid,
    pois: Vec<(IVec3, Block)>,
    biome: crate::world::terrain::Biome,
}

const FLOOR: i32 = 64;

impl crate::physics::BlockSource for Village {
    fn block(&self, p: IVec3) -> Option<Block> {
        self.grid.block(p)
    }
}

impl MobWorld for Village {
    fn loaded(&self, _: IVec3) -> bool {
        true
    }
    fn surface(&self, _: i32, _: i32) -> Option<i32> {
        Some(FLOOR - 1)
    }
    fn exposed(&self, p: IVec3) -> bool {
        p.y >= FLOOR
    }
    fn world_surface(&self, _: i32, _: i32) -> Option<i32> {
        Some(FLOOR - 1)
    }
    fn village_pois(&self, visit: &mut dyn FnMut(IVec3, Block)) {
        for &(p, b) in &self.pois {
            visit(p, b);
        }
    }
    fn biome(&self, _: i32, _: i32) -> crate::world::terrain::Biome {
        self.biome
    }
}

/// A village of four homes around the origin with a villager in each.
fn village(e: &mut Entities) -> Village {
    let mut pois = Vec::new();
    for (i, (x, z)) in [(-6, -6), (6, -6), (-6, 6), (6, 6)].into_iter().enumerate() {
        let bed = IVec3::new(x, FLOOR, z);
        pois.push((bed, Block::BED_HEAD));
        e.spawn(MobKind::Villager, bed.as_dvec3() + DVec3::new(1.5, 0.0, 0.5));
        let v = e.mobs.last_mut().unwrap().villager.as_mut().unwrap();
        v.home = Some(bed);
        v.set_profession([Profession::Armorer, Profession::Farmer, Profession::Librarian, Profession::Fletcher][i]);
    }
    let mut grid = Grid::flat(FLOOR);
    for &(p, b) in &pois {
        grid.set(p, b);
    }
    Village { grid, pois, biome: crate::world::terrain::Biome::Plains }
}

fn player_at(pos: DVec3) -> Target {
    Target::new(PlayerId::HOST, pos, false)
}

fn ctx(players: Vec<Target>) -> Ctx {
    Ctx {
        spawn_light: crate::simulation::lighting::SpawnLight::DAY,
        players,
        daylight: 1.0,
        spawning: false,
        raining: false,
        dimension: crate::world::terrain::Dimension::Overworld,
    }
}

fn tick<W: MobWorld + ?Sized>(e: &mut Entities, w: &W, c: &Ctx, d: Difficulty) -> Vec<EntityEvent> {
    e.update_difficulty(0.05, w, c, d)
}

/// The host kills every living member of raid `id` with a blow.
fn kill_raiders(e: &mut Entities, id: u32) -> usize {
    let members: Vec<usize> = e
        .mobs
        .iter()
        .enumerate()
        .filter(|(_, m)| m.alive() && m.illager.as_ref().is_some_and(|s| s.raider.raid == Some(id)))
        .map(|(i, _)| i)
        .collect();
    for &i in &members {
        e.hurt_by_player(i, 1000.0, &[None, None], PlayerId::HOST);
    }
    members.len()
}

// ------------------------------------------------------------ constants

#[test]
fn raid_constants_match_java() {
    let f = facts();
    let c = &f["raid"]["constants"];
    assert_eq!(int(&c["DEFAULT_PRE_RAID_TICKS"]), i64::from(PRE_RAID_TICKS));
    assert_eq!(int(&c["RAID_TIMEOUT_TICKS"]), RAID_TIMEOUT_TICKS);
    assert_eq!(int(&c["NUM_SPAWN_ATTEMPTS"]), i64::from(NUM_SPAWN_ATTEMPTS));
    assert_eq!(int(&c["POST_RAID_TICK_LIMIT"]), i64::from(POST_RAID_TICK_LIMIT));
    assert_eq!(int(&c["MAX_NO_ACTION_TIME"]), i64::from(MAX_NO_ACTION_TIME));
    assert_eq!(int(&c["MAX_CELEBRATION_TICKS"]), i64::from(MAX_CELEBRATION_TICKS));
    assert_eq!(int(&c["OUTSIDE_RAID_BOUNDS_TIMEOUT"]), i64::from(OUTSIDE_RAID_BOUNDS_TIMEOUT));
    assert_eq!(int(&c["DEFAULT_MAX_RAID_OMEN_LEVEL"]), i64::from(MAX_RAID_OMEN_LEVEL));
    assert_eq!(int(&c["LOW_MOB_THRESHOLD"]), i64::from(LOW_MOB_THRESHOLD));
    assert_eq!(int(&c["HERO_OF_THE_VILLAGE_DURATION"]), i64::from(HERO_OF_THE_VILLAGE_TICKS));
    assert_eq!(int(&c["VALID_RAID_RADIUS_SQR"]), VALID_RAID_RADIUS_SQR);
    assert_eq!(int(&c["RAID_REMOVAL_THRESHOLD_SQR"]), RAID_REMOVAL_THRESHOLD_SQR);
    assert_eq!(int(&c["ALLOW_SPAWNING_WITHIN_VILLAGE_SECONDS_THRESHOLD"]), i64::from(VILLAGE_SPAWN_SECONDS));
    assert_eq!(int(&c["SECTION_RADIUS_FOR_FINDING_NEW_VILLAGE_CENTER"]), i64::from(CENTER_SECTION_RADIUS));
    assert_eq!(int(&f["raids"]["radius_sqr"]), VALID_RAID_RADIUS_SQR);
    assert_eq!(int(&f["raids"]["poi_radius"]), i64::from(POI_RADIUS));
    assert_eq!(int(&f["raids"]["max_no_action"]), i64::from(MAX_NO_ACTION_TIME));
    assert_eq!(int(&f["raider"]["no_action_step"]), i64::from(RAIDER_NO_ACTION_STEP));
    let g = &f["raid"]["groups"];
    assert_eq!(
        [0, int(&g["easy"]), int(&g["normal"]), int(&g["hard"])],
        GROUPS.map(i64::from),
        "getNumGroups by difficulty"
    );
    for d in Difficulty::ALL {
        assert_eq!(num_groups(d), GROUPS[d as usize]);
    }
    let o = &f["raid"]["enchant_odds"];
    for level in 2..=5 {
        assert_eq!(f64::from(enchant_odds(level)) as f32, float(&o[format!("level{level}")]) as f32, "odds {level}");
    }
    assert_eq!(enchant_odds(1), 0.0);
    let s = &f["raid"]["spawn_search"];
    assert_eq!(float(&s["distance_scale"]) as f32, SPAWN_DISTANCE_SCALE);
    assert_eq!(float(&s["distance_offset"]) as f32, SPAWN_DISTANCE_OFFSET);
    assert_eq!(float(&s["ring_radius"]) as f32, SPAWN_RING_RADIUS);
    assert_eq!(float(&s["ring_radius_z"]) as f32, SPAWN_RING_RADIUS);
    assert_eq!(float(&s["angle_step_divisor"]) as f32, SPAWN_ANGLE_DIVISOR);
    assert_eq!(int(&s["jitter_bound"]), i64::from(SPAWN_JITTER_BOUND));
    assert_eq!(int(&s["max_height_difference"]), i64::from(SPAWN_MAX_HEIGHT_DIFFERENCE));
    assert_eq!(int(&s["village_seconds"]), i64::from(VILLAGE_SPAWN_SECONDS));
    assert_eq!(int(&s["loaded_margin"]), i64::from(SPAWN_LOADED_MARGIN));
    assert_eq!(float(&f["raid"]["captain_drop_chance"]) as f32, CAPTAIN_BANNER_DROP_CHANCE);
    let leaders = &f["raider"]["can_be_leader"];
    assert_eq!(leaders["ravager"].as_bool().unwrap(), can_lead(MobKind::Ravager));
    assert_eq!(leaders["witch"].as_bool().unwrap(), can_lead(MobKind::Witch));
    assert!(can_lead(MobKind::Pillager) && can_lead(MobKind::Vindicator) && can_lead(MobKind::Evoker));
    let walk = f["raider"]["goals"].as_array().unwrap();
    let through = walk.iter().find(|g| g["goal"] == "Raider$RaiderMoveThroughVillageGoal").unwrap();
    assert_eq!(float(&through["args"][0]) as f32, VILLAGE_WALK_SPEED as f32);
}

#[test]
fn wave_table_is_javas_both_ways() {
    let f = facts();
    let waves = f["raid"]["waves"].as_object().unwrap();
    assert_eq!(waves.len(), RaiderType::ALL.len());
    for kind in RaiderType::ALL {
        let name = kind.kind().name();
        let java: Vec<i32> = waves[name].as_array().unwrap().iter().map(|v| int(v) as i32).collect();
        assert_eq!(java, kind.spawns().to_vec(), "{name} spawns per wave");
    }
    for name in waves.keys() {
        assert!(RaiderType::ALL.iter().any(|k| k.kind().name() == name), "{name} has a raider type");
    }
    // The spawn order is the enum order.
    let order: Vec<&str> = RaiderType::ALL.iter().map(|k| k.kind().name()).collect();
    assert_eq!(order, ["vindicator", "evoker", "pillager", "witch", "ravager"]);
}

#[test]
fn bonus_spawns_follow_get_potential_bonus_spawns() {
    let b = &facts()["raid"]["bonus_spawns"];
    let (easy_bound, normal, hard) = (int(&b["easy_bound"]), int(&b["normal"]), int(&b["hard"]));
    let (witch, early, skipped) = (int(&b["witch"]), int(&b["witch_last_early_wave"]), int(&b["witch_skipped_wave"]));
    let range = |kind, wave, d, bonus| {
        let mut rng = Rng::new(3);
        let rolls: Vec<i32> = (0..400).map(|_| bonus_spawns(kind, wave, d, bonus, &mut rng)).collect();
        (*rolls.iter().min().unwrap() as i64, *rolls.iter().max().unwrap() as i64)
    };
    use Difficulty::*;
    // `nextInt(2)` then `nextInt(n + 1)`: at most 1 on Easy.
    assert_eq!(range(RaiderType::Pillager, 1, Easy, false), (0, easy_bound - 1));
    assert_eq!(range(RaiderType::Vindicator, 3, Normal, false), (0, normal));
    assert_eq!(range(RaiderType::Pillager, 6, Hard, false), (0, hard));
    assert_eq!(range(RaiderType::Evoker, 6, Hard, true), (0, 0));
    for wave in 1..=7 {
        let expect = if wave <= early || wave == skipped { 0 } else { witch };
        assert_eq!(range(RaiderType::Witch, wave as i32, Hard, false), (0, expect), "witch wave {wave}");
        assert_eq!(range(RaiderType::Witch, wave as i32, Easy, false), (0, 0));
    }
    assert_eq!(range(RaiderType::Ravager, 5, Normal, true), (0, int(&b["ravager_bonus_wave"])));
    assert_eq!(range(RaiderType::Ravager, 5, Normal, false), (0, 0));
    assert_eq!(range(RaiderType::Ravager, 5, Easy, true), (0, 0));
}

#[test]
fn patrol_omen_bell_and_gift_constants_match_java() {
    let f = facts();
    let p = &f["patrol"];
    let s = &p["spawner"];
    assert_eq!(int(&s["interval"]), i64::from(PATROL_INTERVAL));
    assert_eq!(int(&s["interval_spread"]), i64::from(PATROL_INTERVAL_SPREAD));
    assert_eq!(int(&s["chance"]), i64::from(PATROL_CHANCE));
    assert_eq!(int(&s["village_sections"]), i64::from(PATROL_VILLAGE_SECTIONS));
    assert_eq!(int(&s["min_offset"]), i64::from(PATROL_MIN_OFFSET));
    assert_eq!(int(&s["offset_spread"]), i64::from(PATROL_OFFSET_SPREAD));
    assert_eq!(int(&s["loaded_margin"]), i64::from(PATROL_LOADED_MARGIN));
    assert_eq!(int(&s["wander"]), i64::from(PATROL_MEMBER_STEP));
    assert_eq!(float(&p["captain_chance"]) as f32, CAPTAIN_CHANCE);
    assert_eq!(int(&p["max_block_light"]), i64::from(PATROL_MAX_BLOCK_LIGHT));
    assert_eq!(float(&p["despawn_distance_sqr"]), PATROL_DESPAWN_SQR);
    assert_eq!(int(&p["target"]["offset"]), i64::from(PATROL_TARGET_OFFSET));
    assert_eq!(int(&p["target"]["spread"]), i64::from(PATROL_TARGET_SPREAD));
    let g = &p["goal"];
    assert_eq!(float(&g["speed"]), PATROL_SPEED);
    assert_eq!(float(&g["leader_speed"]), PATROL_LEADER_SPEED);
    assert_eq!(float(&g["arrive_distance"]), PATROL_ARRIVE);
    assert_eq!(float(&g["turn_degrees"]) as f32, PATROL_TURN_DEGREES);
    assert_eq!(float(&g["turn_scale"]), PATROL_TURN_SCALE);
    assert_eq!(float(&g["step"]), PATROL_STEP);
    assert_eq!(float(&g["companion_radius"]), PATROL_COMPANION_RADIUS);
    assert_eq!(int(&g["wander"]["offset"]), i64::from(PATROL_WANDER_OFFSET));
    assert_eq!(int(&g["wander"]["spread"]), i64::from(PATROL_WANDER_SPREAD));
    assert_eq!(int(&p["failed_cooldown"]), i64::from(PATROL_FAILED_COOLDOWN));
    let env = &f["environment"];
    assert_eq!(env["patrol_keyframes"], json!([[0, false], [PATROL_START_TICKS, true]]));
    assert_eq!(env["patrol_forbidden_biomes"], json!(["mushroom_fields"]));
    assert_eq!(env["raid_forbidden_dimensions"], json!(["the_nether"]));
    assert_eq!(int(&f["omen"]["raid_omen_ticks"]), i64::from(RAID_OMEN_TICKS));
    assert_eq!(int(&f["omen"]["fires_at"]), i64::from(RAID_OMEN_FIRES_AT));
    let b = &f["bell"];
    assert_eq!(int(&b["TICKS_BEFORE_RESONATION"]), i64::from(BELL_TICKS_BEFORE_RESONATION));
    assert_eq!(int(&b["MAX_RESONATION_TICKS"]), i64::from(BELL_MAX_RESONATION_TICKS));
    assert_eq!(int(&b["GLOW_DURATION"]), i64::from(BELL_GLOW_TICKS));
    assert_eq!(int(&b["HEAR_BELL_RADIUS"]) as f64, BELL_HEAR_RADIUS);
    assert_eq!(int(&b["HIGHLIGHT_RAIDERS_RADIUS"]) as f64, BELL_HIGHLIGHT_RADIUS);
    assert_eq!(int(&b["SEARCH_RADIUS"]) as f64, BELL_SEARCH_RADIUS);
    assert_eq!(int(&b["DURATION"]), i64::from(BELL_DURATION));
    let gift = &f["gift"];
    assert_eq!(int(&gift["MIN_TIME_BETWEEN_GIFTS"]), i64::from(GIFT_MIN_WAIT));
    assert_eq!(int(&gift["MIN_TIME_BETWEEN_GIFTS"]), i64::from(GIFT_FIRST_WAIT));
    assert_eq!(int(&gift["MAX_TIME_BETWEEN_GIFTS"]), i64::from(GIFT_MAX_WAIT));
    assert_eq!(int(&gift["THROW_GIFT_AT_DISTANCE"]) as f64, GIFT_THROW_DISTANCE);
    assert_eq!(int(&gift["TIME_TO_DELAY_FOR_HEAD_TO_FINISH_TURNING"]), GIFT_TURN_DELAY);
    let price = &f["hero_price"];
    assert_eq!(float(&price["base"]) as f32, HERO_BASE_DISCOUNT);
    assert_eq!(float(&price["per_level"]) as f32, HERO_DISCOUNT_PER_LEVEL);
    assert_eq!(int(&price["minimum"]), 1);
    let providers = &f["enchantment_providers"];
    assert_eq!(providers["vindicator"], json!({"enchantment": "sharpness", "level": 1}));
    assert_eq!(providers["vindicator_post_wave_5"], json!({"enchantment": "sharpness", "level": 2}));
    assert_eq!(providers["pillager_post_wave_3"], json!({"enchantment": "quick_charge", "level": 1}));
    assert_eq!(providers["pillager_post_wave_5"], json!({"enchantment": "quick_charge", "level": 2}));
    let captain = &f["captain_loot"];
    assert_eq!(f["captain_loot_tables"], json!(["pillager"]));
    assert_eq!(captain["item"], "ominous_bottle");
    assert_eq!(captain["amplifier"], json!([0, 4]));
}

// ------------------------------------------------------------ gifts and prices

#[test]
fn gift_tables_are_javas_both_ways() {
    let f = facts();
    let loot = f["gift"]["loot"].as_object().unwrap();
    for (name, pools) in loot {
        let pools = pools.as_array().unwrap();
        assert_eq!(pools.len(), 1, "{name}: one pool");
        assert_eq!(pools[0]["rolls"], 1);
        let java = pools[0]["entries"].as_array().unwrap();
        let ours = gift_table(name);
        assert_eq!(java.len(), ours.len(), "{name} entries");
        for (j, o) in java.iter().zip(ours) {
            assert_eq!(j["item"], o.item, "{name}");
            assert_eq!(int(&j["weight"]), i64::from(o.weight), "{name} {}", o.item);
            let count = j.get("count").map_or([1, 1], |c| [int(&c[0]), int(&c[1])]);
            assert_eq!(count, [i64::from(o.count.0), i64::from(o.count.1)], "{name} {}", o.item);
            assert_eq!(j.get("potion").and_then(Value::as_str), o.potion, "{name}");
            match o.potion {
                Some(p) => assert!(crate::potion::Potion::from_id(p).is_some(), "potion {p}"),
                None => assert!(crate::java_registry::item(o.item, "").is_some(), "item {}", o.item),
            }
        }
    }
    let tables = f["gift"]["tables"].as_object().unwrap();
    for p in Profession::ALL {
        let name = gift_table_name(p, false);
        match tables.get(&p.name().to_lowercase()) {
            Some(table) => assert_eq!(table.as_str().unwrap(), format!("{name}_gift"), "{p:?}"),
            None => assert_eq!(name, "unemployed", "{p:?} has no gift table"),
        }
    }
    assert_eq!(gift_table_name(Profession::Armorer, true), "baby");
    assert!(loot.contains_key("baby") && loot.contains_key("unemployed"));
    // Rolls land on the table: a fletcher throws arrows two times in three.
    let mut rng = Rng::new(9);
    let arrows =
        (0..400).filter_map(|_| roll_gift(gift_table("fletcher"), &mut rng)).filter(|s| s.item == Item::ARROW).count();
    // Arrows weigh 26 of 39: about 267 of 400.
    assert!((235..300).contains(&arrows), "{arrows}");
}

#[test]
fn hero_of_the_village_cuts_villager_prices() {
    assert_eq!(hero_price_cut(10, None), 0);
    assert_eq!(hero_price_cut(10, Some(0)), 3, "floor(0.3 * 10)");
    assert_eq!(hero_price_cut(20, Some(4)), 11, "floor(0.55 * 20)");
    assert_eq!(hero_price_cut(1, Some(0)), 1, "at least one");
    let mut e = Entities::new(5);
    e.spawn(MobKind::Villager, DVec3::ZERO);
    let v = e.mobs[0].villager.as_mut().unwrap();
    v.set_profession(Profession::Farmer);
    let offer = v.offers.iter().flatten().next().copied().unwrap();
    let plain = v.priced_for(offer, PlayerId::HOST).count;
    v.heroes = vec![(PlayerId::HOST, 0)];
    let hero = v.priced_for(offer, PlayerId::HOST).count;
    let expect = (i32::from(plain) - hero_price_cut(offer.cost.count, Some(0))).max(1) as u8;
    assert_eq!(hero, expect);
    assert!(hero < plain || plain == 1);
    // Another player pays full price.
    assert_eq!(v.priced_for(offer, PlayerId(7)).count, plain);
}

// ------------------------------------------------------------ villages and omens

#[test]
fn village_sections_follow_section_distance() {
    let mut v = VillageSections::default();
    v.insert(IVec3::new(5, 64, 5));
    assert!(v.is_village(IVec3::new(5, 64, 5)));
    assert!(v.is_village(IVec3::new(31, 64, 5)), "one section away");
    assert!(!v.is_village(IVec3::new(33, 64, 5)), "two sections away");
    assert!(v.close_to_village(IVec3::new(47, 64, 5), 2));
    assert_eq!(v.sections_to_village(IVec3::new(5 + 16 * 5, 64, 5)), 5);
    assert_eq!(VillageSections::default().sections_to_village(IVec3::ZERO), 7);
    assert!(!v.close_to_village(IVec3::new(5, 64, 5), 7), "beyond six sections is never close");
}

#[test]
fn bad_omen_becomes_raid_omen_only_in_a_village() {
    let mut e = Entities::new(11);
    let w = village(&mut e);
    let mut s = PlayerSession::new(PlayerId::HOST, DVec3::new(200.5, f64::from(FLOOR), 0.5));
    s.vitals.give_effect(Active::new(Effect::BadOmen, 2, 120_000));
    assert_eq!(s.tick_raid_omens(&mut e, &w, false, true), None, "outside the village");
    s.player.pos = DVec3::new(0.5, f64::from(FLOOR), 0.5);
    assert_eq!(s.tick_raid_omens(&mut e, &w, true, true), None, "peaceful");
    let mut spectator = PlayerSession::new(PlayerId::HOST, s.player.pos);
    spectator.vitals.give_effect(Active::new(Effect::BadOmen, 2, 120_000));
    spectator.set_mode(crate::rules::GameMode::Spectator);
    assert_eq!(spectator.tick_raid_omens(&mut e, &w, false, true), None, "spectators");
    assert_eq!(
        s.tick_raid_omens(&mut e, &w, false, true),
        Some(crate::session::raid::Omen::Gathering(IVec3::new(0, FLOOR, 0)))
    );
    assert!(!s.vitals.effects.has(Effect::BadOmen));
    let omen = *s.vitals.effects.get(Effect::RaidOmen).unwrap();
    assert_eq!((omen.amplifier, omen.ticks), (2, RAID_OMEN_TICKS));
    // Saved with the player.
    let text = s.raid_omen_to_string().unwrap();
    let mut back = PlayerSession::new(PlayerId::HOST, DVec3::ZERO);
    back.load_raid_omen(&text);
    assert_eq!(back.raid_omen_pos, s.raid_omen_pos);
    back.load_raid_omen("1,2");
    assert_eq!(back.raid_omen_pos, s.raid_omen_pos, "malformed text is ignored");
    // Not yet: the raid starts when one tick is left.
    s.vitals.effects.add_instance(Active::new(Effect::RaidOmen, 2, 2));
    assert!(e.raids.raids.is_empty());
    s.vitals.effects.remove(Effect::RaidOmen);
    s.vitals.effects.add(Effect::RaidOmen, 2, 1);
    let Some(crate::session::raid::Omen::Raid(id)) = s.tick_raid_omens(&mut e, &w, false, true) else {
        panic!("the raid should start")
    };
    assert!(!s.vitals.effects.has(Effect::RaidOmen) && s.raid_omen_pos.is_none());
    let raid = e.raids.get(id).unwrap();
    assert_eq!(raid.omen_level, 3, "amplifier 2 is level 3");
    assert_eq!(raid.num_groups, num_groups(Difficulty::Normal));
    // The centre is the average of the four claimed homes.
    assert_eq!(raid.center, IVec3::new(0, FLOOR, 0));
    assert!(e.raids.outcomes.contains(&Outcome::Triggered { player: PlayerId::HOST }));
    // The Nether forbids raids; so does the rule.
    let mut nether = Entities::new(12);
    let w2 = village(&mut nether);
    assert_eq!(nether.create_or_extend_raid(&w2, PlayerId::HOST, IVec3::new(0, FLOOR, 0), 0, false), None);
    nether.raids.enabled = false;
    assert_eq!(nether.create_or_extend_raid(&w2, PlayerId::HOST, IVec3::new(0, FLOOR, 0), 0, true), None);
}

#[test]
fn omens_add_up_to_level_five_and_then_stop() {
    let mut e = Entities::new(13);
    let w = village(&mut e);
    let at = IVec3::new(0, FLOOR, 0);
    let id = e.create_or_extend_raid(&w, PlayerId::HOST, at, 1, true).unwrap();
    assert_eq!(e.raids.get(id).unwrap().omen_level, 2);
    assert_eq!(e.create_or_extend_raid(&w, PlayerId::HOST, at, 4, true), Some(id), "same raid");
    assert_eq!(e.raids.get(id).unwrap().omen_level, 5, "clamped to 5");
    // A level 5 raid turns Bad Omen away.
    let mut s = PlayerSession::new(PlayerId::HOST, DVec3::new(0.5, f64::from(FLOOR), 0.5));
    s.vitals.give_effect(Active::new(Effect::BadOmen, 0, 100));
    assert_eq!(s.tick_raid_omens(&mut e, &w, false, true), None);
    assert!(s.vitals.effects.has(Effect::BadOmen));
}

// ------------------------------------------------------------ the scenario

/// Counts each kind among a wave's spawns.
fn wave_counts(e: &Entities, id: u32, wave: i32) -> [i32; 5] {
    let mut out = [0; 5];
    for m in &e.mobs {
        let Some(s) = &m.illager else { continue };
        if m.alive() && s.raider.raid == Some(id) && s.raider.wave == wave {
            let k = RaiderType::ALL.iter().position(|t| t.kind() == m.kind).unwrap();
            out[k] += 1;
        }
    }
    out
}

/// Easy, level 2 (one bonus wave): the horn, each wave's composition, the
/// captain, riders, the bar, victory and Hero of the Village with its
/// discount and gifts.
#[test]
fn a_raid_from_bad_omen_to_hero_of_the_village() {
    let mut e = Entities::new(21);
    let w = village(&mut e);
    let d = Difficulty::Easy;
    e.difficulty = d;
    let host = player_at(DVec3::new(0.5, f64::from(FLOOR), 0.5));
    let c = ctx(vec![host]);
    let id = e.create_or_extend_raid(&w, PlayerId::HOST, IVec3::new(0, FLOOR, 0), 1, true).unwrap();
    assert_eq!(e.raids.get(id).unwrap().num_groups, 3);
    let mut waves_seen = Vec::new();
    let mut horns = 0;
    let mut victory_at = None;
    let mut bar_seen = false;
    for t in 0..20_000 {
        tick(&mut e, &w, &c, d);
        horns += e.raids.outcomes.iter().filter(|o| matches!(o, Outcome::Horn { .. })).count();
        e.raids.outcomes.retain(|o| !matches!(o, Outcome::Horn { .. }));
        if let Some(Outcome::Hero { player, amplifier }) =
            e.raids.outcomes.iter().copied().find(|o| matches!(o, Outcome::Hero { .. }))
        {
            assert_eq!((player, amplifier), (PlayerId::HOST, 1), "level 2 gives Hero II");
            victory_at = Some(t);
            break;
        }
        e.raids.outcomes.retain(|o| !matches!(o, Outcome::Hero { .. }));
        let raid = e.raids.get(id).expect("the raid lasts until it is won").clone();
        bar_seen |= e.raids.bar_for(PlayerId::HOST).is_some();
        let alive = e.raiders_alive(id);
        if alive > 0 && !waves_seen.contains(&raid.groups_spawned) {
            let wave = raid.groups_spawned;
            waves_seen.push(wave);
            let counts = wave_counts(&e, id, wave);
            let bonus = wave > raid.num_groups;
            for (k, kind) in RaiderType::ALL.into_iter().enumerate() {
                let base = raid.default_spawns(kind, wave, bonus);
                // Easy adds at most one vindicator or pillager, no witch or ravager.
                let extra = i32::from(matches!(kind, RaiderType::Vindicator | RaiderType::Pillager));
                assert!(
                    (base..=base + extra).contains(&counts[k]),
                    "wave {wave} {kind:?}: {} not in {base}..={}",
                    counts[k],
                    base + extra
                );
            }
            // One captain per wave, wearing the banner, and never a ravager.
            let captains: Vec<&Mob> = e
                .mobs
                .iter()
                .filter(|m| m.alive() && m.illager.as_ref().is_some_and(|s| s.raider.raid == Some(id)))
                .filter(|m| is_captain(m))
                .collect();
            assert_eq!(captains.len(), 1, "wave {wave} captain");
            assert!(can_lead(captains[0].kind));
            assert_eq!(captains[0].equipment.drop_chance(Slot::Head), CAPTAIN_BANNER_DROP_CHANCE);
            // Every member counts toward the bar's total.
            let health = e.raider_health(id);
            assert!((raid.total_health - health).abs() < 1e-3, "wave {wave} total health");
            // The wave appears far from the village, within the removal radius.
            for m in e.mobs.iter().filter(|m| m.illager.as_ref().is_some_and(|s| s.raider.raid == Some(id))) {
                let d2 = dist_sqr(raid.center, m.pos.floor().as_ivec3());
                assert!(d2 < RAID_REMOVAL_THRESHOLD_SQR, "{d2}");
                assert!(!e.raids.villages.is_village(m.pos.floor().as_ivec3()), "spawned outside the village");
            }
            assert!(kill_raiders(&mut e, id) > 0);
        }
    }
    assert!(victory_at.is_some(), "the raid was won");
    assert_eq!(waves_seen, vec![1, 2, 3, 4], "three Easy waves and the bonus wave");
    assert_eq!(horns, 4, "a horn per wave");
    assert!(bar_seen, "the host saw the raid bar");
    let raid = e.raids.get(id).unwrap();
    assert!(raid.is_victory());
    assert_eq!(raid.heroes, vec![PlayerId::HOST]);
    // The game crowns the hero.
    let mut s = PlayerSession::new(PlayerId::HOST, host.pos);
    s.crown_hero(1);
    let hero = *s.vitals.effects.get(Effect::HeroOfTheVillage).unwrap();
    assert_eq!((hero.amplifier, hero.ticks, hero.particles, hero.show_icon), (1, 48_000, false, true));
    // The bar names the victory while the raid lingers, then it ends.
    for _ in 0..40 {
        tick(&mut e, &w, &c, d);
    }
    assert_eq!(e.raids.bar_for(PlayerId::HOST).map(|b| b.title), Some(BarTitle::Victory));
    for _ in 0..MAX_CELEBRATION_TICKS {
        tick(&mut e, &w, &c, d);
    }
    assert!(e.raids.raids.is_empty(), "the won raid stops after 600 ticks");
    // Villagers now see a hero: lower prices, and a gift once the wait is over.
    e.raids.heroes = vec![(PlayerId::HOST, 1)];
    e.items.clear();
    let items_before = 0;
    let near = player_at(e.mobs.iter().find(|m| m.kind == MobKind::Villager).unwrap().pos + DVec3::X);
    let c = ctx(vec![near]);
    for _ in 0..(GIFT_FIRST_WAIT + 200) {
        tick(&mut e, &w, &c, d);
    }
    let v = e.mobs.iter().find_map(|m| m.villager.as_ref()).unwrap();
    assert_eq!(v.heroes, vec![(PlayerId::HOST, 1)]);
    assert!(e.items.len() > items_before, "a villager threw a gift to the hero");
}

/// Saved between waves with raiders alive: the raid, its members, their
/// waves and the captain come back, and the raid goes on to victory.
#[test]
fn a_raid_survives_save_and_reload() {
    let mut e = Entities::new(31);
    let w = village(&mut e);
    let d = Difficulty::Normal;
    e.difficulty = d;
    let c = ctx(vec![player_at(DVec3::new(0.5, f64::from(FLOOR), 0.5))]);
    let id = e.create_or_extend_raid(&w, PlayerId::HOST, IVec3::new(0, FLOOR, 0), 0, true).unwrap();
    // Wave 1, cleared; then wave 2 out.
    let mut cleared = false;
    for _ in 0..5_000 {
        tick(&mut e, &w, &c, d);
        if e.raiders_alive(id) > 0 {
            if cleared && e.raids.get(id).unwrap().groups_spawned == 2 {
                break;
            }
            kill_raiders(&mut e, id);
            cleared = true;
        }
    }
    let raid = e.raids.get(id).unwrap().clone();
    assert_eq!(raid.groups_spawned, 2);
    let alive = e.raiders_alive(id);
    assert!(alive > 0);
    let members: Vec<(MobKind, i32, bool)> = e
        .mobs
        .iter()
        .filter(|m| m.alive() && m.illager.as_ref().is_some_and(|s| s.raider.raid == Some(id)))
        .map(|m| (m.kind, m.illager.as_ref().unwrap().raider.wave, is_captain(m)))
        .collect();
    let mut props = std::collections::BTreeMap::new();
    e.save_props(&mut props);
    let saved: Value = serde_json::from_str(&props["raids"]).unwrap();
    // Java's `Raids` and `Raid` codec fields.
    for key in ["raids", "next_id", "tick"] {
        assert!(saved.get(key).is_some(), "{key}");
    }
    for key in [
        "id",
        "started",
        "active",
        "ticks_active",
        "raid_omen_level",
        "groups_spawned",
        "cooldown_ticks",
        "post_raid_ticks",
        "total_health",
        "group_count",
        "status",
        "center",
        "heroes_of_the_village",
    ] {
        assert!(saved["raids"][0].get(key).is_some(), "{key}");
    }
    let mut back = Entities::new(32);
    back.difficulty = d;
    back.load_props(&props);
    let restored = back.raids.get(id).unwrap();
    assert_eq!(
        (restored.started, restored.omen_level, restored.groups_spawned, restored.num_groups, restored.status),
        (raid.started, raid.omen_level, raid.groups_spawned, raid.num_groups, raid.status)
    );
    assert_eq!((restored.center, restored.heroes.clone()), (raid.center, raid.heroes.clone()));
    assert_eq!(restored.total_health, raid.total_health);
    assert_eq!(back.raiders_alive(id), alive, "membership came back with the raiders");
    let mut again: Vec<(MobKind, i32, bool)> = back
        .mobs
        .iter()
        .filter(|m| m.alive() && m.illager.as_ref().is_some_and(|s| s.raider.raid == Some(id)))
        .map(|m| (m.kind, m.illager.as_ref().unwrap().raider.wave, is_captain(m)))
        .collect();
    let mut members = members;
    members.sort_by_key(|m| (m.0.name(), m.1, m.2));
    again.sort_by_key(|m| (m.0.name(), m.1, m.2));
    assert_eq!(again, members);
    // It goes on: the next raid id is fresh, and the waves continue to a win.
    assert!(back.raids.next_id >= id);
    let mut won = false;
    for _ in 0..20_000 {
        tick(&mut back, &w, &c, d);
        if back.raids.outcomes.iter().any(|o| matches!(o, Outcome::Hero { .. })) {
            won = true;
            break;
        }
        if back.raiders_alive(id) > 0 {
            kill_raiders(&mut back, id);
        }
    }
    assert!(won, "the reloaded raid was won");
    assert_eq!(back.raids.get(id).unwrap().groups_spawned, num_groups(d));
}

#[test]
fn raids_stop_when_the_rule_is_off_and_on_peaceful() {
    let mut e = Entities::new(41);
    let w = village(&mut e);
    let c = ctx(vec![player_at(DVec3::new(0.5, f64::from(FLOOR), 0.5))]);
    e.create_or_extend_raid(&w, PlayerId::HOST, IVec3::new(0, FLOOR, 0), 0, true).unwrap();
    e.raids.enabled = false;
    tick(&mut e, &w, &c, Difficulty::Normal);
    tick(&mut e, &w, &c, Difficulty::Normal);
    assert!(e.raids.raids.is_empty());
    e.raids.enabled = true;
    e.create_or_extend_raid(&w, PlayerId::HOST, IVec3::new(0, FLOOR, 0), 0, true).unwrap();
    tick(&mut e, &w, &c, Difficulty::Peaceful);
    tick(&mut e, &w, &c, Difficulty::Peaceful);
    assert!(e.raids.raids.is_empty());
}

#[test]
fn a_raid_with_no_village_left_is_lost_or_stopped() {
    let mut e = Entities::new(43);
    let w = village(&mut e);
    let c = ctx(vec![player_at(DVec3::new(0.5, f64::from(FLOOR), 0.5))]);
    let id = e.create_or_extend_raid(&w, PlayerId::HOST, IVec3::new(0, FLOOR, 0), 0, true).unwrap();
    // The villagers die before the first wave: the raid stops.
    for m in &mut e.mobs {
        if m.kind == MobKind::Villager {
            m.health = 0.0;
            m.dying = Some(0.0);
        }
    }
    e.refresh_villages(&w, true);
    tick(&mut e, &w, &c, Difficulty::Normal);
    tick(&mut e, &w, &c, Difficulty::Normal);
    assert!(e.raids.get(id).is_none());
}

// ------------------------------------------------------------ captains and patrols

#[test]
fn natural_raiders_lead_about_one_in_seventeen_times() {
    let mut e = Entities::new(51);
    let mut captains = 0;
    for i in 0..2000 {
        e.spawn(MobKind::Pillager, DVec3::new(i as f64 * 0.37, 64.0, (i % 13) as f64));
        captains += usize::from(is_captain(e.mobs.last().unwrap()));
    }
    let share = captains as f64 / 2000.0;
    assert!((share - f64::from(CAPTAIN_CHANCE)).abs() < 0.02, "{share}");
    // Never ravagers or witches; never raid or patrol spawns.
    for _ in 0..200 {
        e.spawn(MobKind::Ravager, DVec3::new(3.0, 64.0, 3.0));
        assert!(!is_captain(e.mobs.last().unwrap()));
    }
    e.raids_event_spawn = true;
    for i in 0..300 {
        e.spawn(MobKind::Pillager, DVec3::new(i as f64, 64.0, 0.0));
        assert!(!is_captain(e.mobs.last().unwrap()));
    }
}

#[test]
fn only_pillager_captains_drop_an_ominous_bottle() {
    let mut e = Entities::new(61);
    let w = Village { grid: Grid::flat(FLOOR), pois: Vec::new(), biome: crate::world::terrain::Biome::Plains };
    let c = ctx(vec![player_at(DVec3::new(0.5, f64::from(FLOOR), 40.5))]);
    let at = DVec3::new(0.5, f64::from(FLOOR), 0.5);
    for (kind, captain) in
        [(MobKind::Pillager, true), (MobKind::Pillager, false), (MobKind::Vindicator, true), (MobKind::Evoker, true)]
    {
        e.mobs.clear();
        e.items.clear();
        e.raids_event_spawn = true;
        e.spawn(kind, at);
        e.raids_event_spawn = false;
        e.ensure_mob_ids();
        if captain {
            make_captain(e.mobs.last_mut().unwrap());
        }
        e.hurt_by_player(0, 1000.0, &[None, None], PlayerId::HOST);
        // The game drops a player's kill's loot (`drop_loot_killed`).
        e.drop_loot_killed(kind, at, 0, true);
        for _ in 0..3 {
            tick(&mut e, &w, &c, Difficulty::Normal);
        }
        let bottles: Vec<_> =
            e.items.iter().filter(|it| crate::world::trial::ominous_amplifier(it.stack.item).is_some()).collect();
        assert_eq!(bottles.len(), usize::from(kind == MobKind::Pillager && captain), "{kind:?} captain {captain}");
        let banners = e.items.iter().filter(|it| crate::content::banner_patterns::is_ominous(&it.stack)).count();
        assert_eq!(banners, usize::from(captain), "{kind:?}: the banner always drops");
    }
    let mut rng = Rng::new(3);
    let amps: FxHashSet<u8> = (0..500)
        .filter_map(|_| captain_loot(MobKind::Pillager, &mut rng))
        .filter_map(|s| crate::world::trial::ominous_amplifier(s.item))
        .collect();
    assert_eq!(amps.len(), 5, "amplifiers 0 to 4");
}

#[test]
fn patrols_spawn_by_day_after_five_days_away_from_villages() {
    let mut e = Entities::new(71);
    let mut w = Village { grid: Grid::flat(FLOOR), pois: Vec::new(), biome: crate::world::terrain::Biome::Plains };
    let c = ctx(vec![player_at(DVec3::new(0.5, f64::from(FLOOR), 0.5))]);
    e.monster_spawning = true;
    let mut c_spawn = ctx(c.players.clone());
    c_spawn.spawning = true;
    let attempt = |e: &mut Entities, w: &Village, c: &Ctx| {
        for _ in 0..200 {
            e.raids.patrol_next = 1;
            e.patrol_tick(w, c);
        }
        e.mobs.iter().filter(|m| m.kind == MobKind::Pillager).count()
    };
    e.clock_time = PATROL_START_TICKS - 1;
    assert_eq!(attempt(&mut e, &w, &c_spawn), 0, "the early_game timeline");
    e.clock_time = PATROL_START_TICKS;
    e.raids.patrols = false;
    assert_eq!(attempt(&mut e, &w, &c_spawn), 0, "spawn_patrols off");
    e.raids.patrols = true;
    w.biome = crate::world::terrain::Biome::MushroomFields;
    assert_eq!(attempt(&mut e, &w, &c_spawn), 0, "mushroom fields");
    w.biome = crate::world::terrain::Biome::Plains;
    let mut night = ctx(c.players.clone());
    night.spawning = true;
    night.spawn_light = crate::simulation::lighting::SpawnLight::NIGHT;
    assert_eq!(attempt(&mut e, &w, &night), 0, "only by day");
    e.difficulty = Difficulty::Normal;
    let n = attempt(&mut e, &w, &c_spawn);
    assert!(n > 0, "a patrol spawned");
    let leaders: Vec<&Mob> = e.mobs.iter().filter(|m| is_captain(m)).collect();
    assert!(!leaders.is_empty());
    for l in &leaders {
        let r = &l.illager.as_ref().unwrap().raider;
        assert!(r.patrolling);
        let t = r.patrol_target.expect("the leader heads somewhere");
        let off = t - l.pos.floor().as_ivec3();
        assert!(off.x.abs() <= 500 && off.z.abs() <= 500, "{off}");
        // 24 to 47 blocks from the player on each axis.
        let d = (l.pos - c.players[0].pos).abs();
        assert!((23.0..48.0).contains(&d.x) && (23.0..48.0).contains(&d.z), "{d}");
    }
    let effective = Difficulty::Normal.effective(e.clock_time, 0, e.moon_brightness);
    let most = effective.ceil() as usize + 1;
    assert!(n <= most * leaders.len(), "{n} pillagers in {} patrols of at most {most}", leaders.len());
    assert!(e.mobs.iter().all(|m| m.illager.as_ref().unwrap().raider.patrolling));
    // Near a village there are none.
    let mut v = Entities::new(72);
    let vw = village(&mut v);
    v.monster_spawning = true;
    v.clock_time = PATROL_START_TICKS;
    v.difficulty = Difficulty::Normal;
    assert_eq!(attempt(&mut v, &vw, &c_spawn), 0, "close to a village");
}

#[test]
fn patrol_members_follow_their_leader_and_reload_their_target() {
    let mut e = Entities::new(81);
    e.raids_event_spawn = true;
    e.spawn(MobKind::Pillager, DVec3::new(0.5, 64.0, 0.5));
    e.spawn(MobKind::Pillager, DVec3::new(3.5, 64.0, 0.5));
    e.raids_event_spawn = false;
    make_captain(&mut e.mobs[0]);
    for m in &mut e.mobs {
        m.illager.as_mut().unwrap().raider.patrolling = true;
    }
    e.mobs[0].illager.as_mut().unwrap().raider.patrol_target = Some(IVec3::new(300, 64, 0));
    e.mobs[0].illager.as_mut().unwrap().raider.patrol_step = Some(IVec3::new(10, 64, 0));
    e.patrol_companions();
    let follower = &e.mobs[1].illager.as_ref().unwrap().raider;
    assert!(follower.companions);
    assert_eq!(follower.patrol_target, Some(IVec3::new(10, 64, 0)));
    let saved = e.nether_mobs_to_string();
    let mut back = Entities::new(82);
    back.load_nether_mobs(&saved);
    let leader = back.mobs.iter().find(|m| is_captain(m)).unwrap();
    assert_eq!(leader.illager.as_ref().unwrap().raider.patrol_target, Some(IVec3::new(300, 64, 0)));
    assert!(leader.illager.as_ref().unwrap().raider.patrolling);
}

// ------------------------------------------------------------ bells and witches

#[test]
fn a_ringing_bell_lights_up_nearby_raiders() {
    let mut e = Entities::new(91);
    let w = Village { grid: Grid::flat(FLOOR), pois: Vec::new(), biome: crate::world::terrain::Biome::Plains };
    // Close enough that nobody despawns (128 blocks).
    let c = ctx(vec![player_at(DVec3::new(30.5, f64::from(FLOOR), 40.5))]);
    e.raids_event_spawn = true;
    e.spawn(MobKind::Pillager, DVec3::new(20.5, f64::from(FLOOR), 0.5));
    e.spawn(MobKind::Vindicator, DVec3::new(40.5, f64::from(FLOOR), 0.5));
    e.spawn(MobKind::Pillager, DVec3::new(60.5, f64::from(FLOOR), 0.5));
    e.raids_event_spawn = false;
    e.spawn(MobKind::Cow, DVec3::new(5.5, f64::from(FLOOR), 0.5));
    e.ensure_mob_ids();
    e.ring_bell(IVec3::new(0, FLOOR, 0));
    let mut resonated = false;
    for t in 1..=(BELL_TICKS_BEFORE_RESONATION + BELL_MAX_RESONATION_TICKS + 2) {
        let events = tick(&mut e, &w, &c, Difficulty::Normal);
        eprintln!(
            "bell t={t} bells={:?} mobs={:?}",
            e.raids.bells,
            e.mobs.iter().map(|m| (m.uid, m.kind, m.pos, m.alive())).collect::<Vec<_>>()
        );
        let rang =
            events.iter().any(|ev| matches!(ev, EntityEvent::Sound { sound: MobSound::Raid(Cue::BellResonate), .. }));
        if rang {
            assert_eq!(t, BELL_TICKS_BEFORE_RESONATION, "resonance five ticks in");
            resonated = true;
        }
    }
    assert!(resonated);
    let glowing = |kind: MobKind, x: f64| {
        e.mobs.iter().find(|m| m.kind == kind && (m.pos.x - x).abs() < 3.0).unwrap().effects.has(Effect::Glowing)
    };
    assert!(glowing(MobKind::Pillager, 20.5), "within 48");
    assert!(glowing(MobKind::Vindicator, 40.5), "within 48");
    assert!(!glowing(MobKind::Pillager, 60.5), "beyond 48");
    assert!(!glowing(MobKind::Cow, 5.5), "not a raider");
    // Without raiders within 32 there's no resonance.
    let mut far = Entities::new(92);
    far.raids_event_spawn = true;
    far.spawn(MobKind::Pillager, DVec3::new(40.5, f64::from(FLOOR), 0.5));
    far.ensure_mob_ids();
    far.ring_bell(IVec3::new(0, FLOOR, 0));
    for _ in 0..60 {
        let events = tick(&mut far, &w, &c, Difficulty::Normal);
        assert!(!events.iter().any(|ev| matches!(ev, EntityEvent::Sound { sound: MobSound::Raid(_), .. })));
    }
    assert!(!far.mobs[0].effects.has(Effect::Glowing));
}

#[test]
fn raid_witches_keep_their_membership_and_natural_ones_cannot_join() {
    let mut e = Entities::new(101);
    e.spawn(MobKind::Witch, DVec3::new(0.5, 64.0, 0.5));
    assert!(e.mobs[0].illager.is_none(), "natural witches can't join raids");
    e.raids_event_spawn = true;
    e.spawn(MobKind::Witch, DVec3::new(2.5, 64.0, 0.5));
    e.raids_event_spawn = false;
    let s = e.mobs[1].illager.as_mut().expect("a raid witch is a raider");
    assert!(s.raider.can_join_raid);
    s.raider.raid = Some(4);
    s.raider.wave = 4;
    let saved = e.nether_mobs_to_string();
    let mut back = Entities::new(102);
    back.load_nether_mobs(&saved);
    let witch =
        back.mobs.iter().find(|m| m.kind == MobKind::Witch && m.illager.is_some()).expect("the raid witch is saved");
    assert_eq!(witch.illager.as_ref().unwrap().raider.raid, Some(4));
    assert_eq!(witch.illager.as_ref().unwrap().raider.wave, 4);
}

#[test]
fn raid_buffs_follow_the_providers() {
    let mut rng = Rng::new(5);
    let mut m = Mob::new(MobKind::Vindicator, DVec3::ZERO, 0.0);
    // Level 5: three in four enchanted; wave 6 is past Normal's five.
    let mut sharp = [0; 3];
    for _ in 0..400 {
        apply_raid_buffs(&mut m, 6, 5, &mut rng);
        let axe = m.equipment.get(Slot::MainHand).unwrap();
        assert_eq!(axe.item, Item::tool(crate::item::ToolKind::Axe, crate::item::Tier::Iron));
        sharp[axe.enchants.level(crate::enchant::Enchantment::Sharpness) as usize] += 1;
    }
    assert_eq!(sharp[1], 0, "post wave 5 gives Sharpness II");
    assert!((250..350).contains(&sharp[2]), "{sharp:?}");
    apply_raid_buffs(&mut m, 2, 1, &mut rng);
    assert_eq!(m.equipment.get(Slot::MainHand).unwrap().enchants.level(crate::enchant::Enchantment::Sharpness), 0);
    let mut p = Mob::new(MobKind::Pillager, DVec3::ZERO, 0.0);
    for (wave, level) in [(2, 0), (4, 1), (6, 2)] {
        p.equipment.set(Slot::MainHand, Some(Stack::new(Item::CROSSBOW, 1)));
        // Odds 0.75: some of 50 tries are enchanted.
        let mut seen = 0;
        for _ in 0..50 {
            p.equipment.set(Slot::MainHand, Some(Stack::new(Item::CROSSBOW, 1)));
            apply_raid_buffs(&mut p, wave, 5, &mut rng);
            seen = seen
                .max(p.equipment.get(Slot::MainHand).unwrap().enchants.level(crate::enchant::Enchantment::QuickCharge));
        }
        assert_eq!(seen, level, "wave {wave}");
    }
}

#[test]
fn bar_titles_round_trip_and_read_like_java() {
    for title in [BarTitle::Raid, BarTitle::Remaining(2), BarTitle::Victory, BarTitle::Defeat] {
        assert_eq!(BarTitle::from_code(i64::from(title.code())), title);
    }
    assert_eq!(BarTitle::Remaining(1).text(), "Raid - Raiders Remaining: 1");
    assert_eq!(BarTitle::Victory.text(), "Raid - Victory");
    assert_eq!(BarTitle::Defeat.text(), "Raid - Defeat");
    for s in Status::ALL {
        assert_eq!(Status::from_name(s.name()), Some(s));
    }
    let _ = Dimension::Overworld;
}
