//! Java 26.3 damage types and damage type tags
//! (`tests/fixtures/java/26.3/damage_types.json`, extracted by
//! `tools/extract_java_damage_fixtures.py`) against VoxelCraft's
//! `simulation::damage` table, and the rules that read the tags through the
//! shared entry points: `Vitals::damage` (every adapter), the agent's
//! `hurt` (CLI, controller and LAN players), `enchant::protect`,
//! `PlayerSession::totem_saves` and the player damage game rules.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use glam::{DVec3, IVec3};
use serde_json::Value;
use voxelcraft::{
    agent::Agent,
    enchant::{self, Enchantment, Enchants},
    entity::Entities,
    inventory::Stack,
    item::{ArmorMaterial, ArmorPiece, Item},
    rules::{GameMode, GameRules},
    simulation::{
        damage::{Cause, DamageEffects, DamageScaling, DamageTag, DamageType, DeathMessageType},
        difficulty::Difficulty,
        effects::Effect,
        survival::{self, MAX_HEALTH, Vitals},
    },
    world::{World, block::Block, chunk::ChunkData, terrain::Generator},
};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/damage_types.json")).unwrap()
}

fn cause(kind: DamageType) -> Cause {
    Cause::new(kind, "test")
}

fn tags() -> BTreeMap<String, BTreeSet<String>> {
    let f = fixture();
    f["tags"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(tag, members)| {
            (tag.clone(), members.as_array().unwrap().iter().map(|m| m.as_str().unwrap().to_string()).collect())
        })
        .collect()
}

#[test]
fn damage_types_match_the_jar_both_ways() {
    let f = fixture();
    let java: BTreeMap<&str, &Value> =
        f["damage_types"].as_array().unwrap().iter().map(|t| (t["id"].as_str().unwrap(), t)).collect();
    assert_eq!(java.len(), 51);
    let ours: BTreeSet<&str> = DamageType::ALL.iter().map(|t| t.id()).collect();
    assert_eq!(ours, java.keys().copied().collect::<BTreeSet<_>>(), "every Java type is a variant and no more");
    assert_eq!(DamageType::ALL.len(), ours.len(), "ids are distinct");
    for &t in DamageType::ALL {
        let j = java[t.id()];
        assert_eq!(t.message_id(), j["message_id"].as_str().unwrap(), "{t:?} message_id");
        assert_eq!(t.exhaustion(), j["exhaustion"].as_f64().unwrap() as f32, "{t:?} exhaustion");
        let scaling = match t.scaling() {
            DamageScaling::Never => "never",
            DamageScaling::WhenCausedByLivingNonPlayer => "when_caused_by_living_non_player",
            DamageScaling::Always => "always",
        };
        assert_eq!(scaling, j["scaling"].as_str().unwrap(), "{t:?} scaling");
        let effects = match t.effects() {
            DamageEffects::Hurt => "hurt",
            DamageEffects::Thorns => "thorns",
            DamageEffects::Drowning => "drowning",
            DamageEffects::Burning => "burning",
            DamageEffects::Poking => "poking",
            DamageEffects::Freezing => "freezing",
        };
        assert_eq!(effects, j["effects"].as_str().unwrap(), "{t:?} effects");
        let death = match t.death_message_type() {
            DeathMessageType::Default => "default",
            DeathMessageType::FallVariants => "fall_variants",
            DeathMessageType::IntentionalGameDesign => "intentional_game_design",
        };
        assert_eq!(death, j["death_message_type"].as_str().unwrap(), "{t:?} death_message_type");
    }
}

#[test]
fn damage_type_tags_match_the_jar_both_ways() {
    let java = tags();
    assert_eq!(java.len(), 36);
    let ours: BTreeSet<&str> = DamageTag::ALL.iter().map(|g| g.id()).collect();
    assert_eq!(ours, java.keys().map(String::as_str).collect::<BTreeSet<_>>(), "every Java tag and no more");
    for &tag in DamageTag::ALL {
        let ours: BTreeSet<String> = DamageType::ALL.iter().filter(|t| t.is(tag)).map(|t| t.id().to_string()).collect();
        assert_eq!(ours, java[tag.id()], "#{} members", tag.id());
    }
}

/// Boots carrying `e` at `level` and nothing else.
fn boots(material: ArmorMaterial, e: Option<(Enchantment, u8)>) -> [Option<Stack>; 4] {
    let mut armor = [None; 4];
    let enchants = e.map_or(Enchants::NONE, |(e, level)| Enchants::NONE.with(e, level));
    armor[3] = Some(Stack { enchants, ..Stack::new(Item::armor(ArmorPiece::Boots, material), 1) });
    armor
}

#[test]
fn protection_enchantments_follow_the_jar_requirements_for_every_type() {
    let f = fixture();
    let tags = tags();
    let rules = f["protection"].as_array().unwrap();
    assert_eq!(rules.len(), 5);
    let mut covered = BTreeSet::new();
    for e in Enchantment::ALL {
        let rule = rules.iter().find(|r| Enchantment::from_name(r["enchantment"].as_str().unwrap()) == Some(e));
        let levels = rule.map_or(1, |r| r["max_level"].as_u64().unwrap() as u8);
        for level in 1..=levels {
            let armor = boots(ArmorMaterial::Diamond, Some((e, level)));
            for &t in DamageType::ALL {
                let expected = rule.map_or(0, |r| {
                    let has = |key: &str| {
                        r[key]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|g| tags[g.as_str().unwrap()].contains(t.id()))
                            .collect::<Vec<_>>()
                    };
                    if has("requires").iter().all(|&b| b) && has("excludes").iter().all(|&b| !b) {
                        let value = r["base"].as_f64().unwrap()
                            + r["per_level_above_first"].as_f64().unwrap() * f64::from(level - 1);
                        value as u32
                    } else {
                        0
                    }
                });
                assert_eq!(enchant::protection_factor(&armor, cause(t)), expected, "{e:?} {level} against {t:?}");
            }
        }
        if let Some(r) = rule {
            covered.insert(r["enchantment"].as_str().unwrap());
        }
    }
    assert_eq!(covered.len(), rules.len(), "every Java protection enchantment is registered");
}

/// `getDamageAfterMagicAbsorb`: `#bypasses_effects` and `#bypasses_enchantments`
/// skip protection entirely.
#[test]
fn protection_skips_bypasses_effects_and_bypasses_enchantments() {
    let armor = boots(ArmorMaterial::Diamond, Some((Enchantment::Protection, 4)));
    for &t in DamageType::ALL {
        let skips = t.is(DamageTag::BypassesEffects)
            || t.is(DamageTag::BypassesEnchantments)
            || t.is(DamageTag::BypassesInvulnerability);
        let expected = if skips { 10.0 } else { 10.0 * (1.0 - 4.0 / 25.0) };
        assert!((enchant::protect(10.0, &armor, cause(t)) - expected).abs() < 1e-5, "{t:?}");
    }
    // Starvation was reduced by Protection before the types were typed.
    assert_eq!(enchant::protect(1.0, &armor, Cause::STARVE), 1.0);
}

/// The fix: Projectile Protection covers tridents, wind charges, thrown items
/// and mob projectiles, not only arrows, through the agent's armored hit.
#[test]
fn projectile_protection_covers_every_is_projectile_type_through_the_agent() {
    let mut entities = Entities::new(1);
    let hit = |protected: bool, t: DamageType, entities: &mut Entities| {
        let mut a = Agent::new(DVec3::new(0.5, 100.0, 0.5));
        a.set_mode(GameMode::Survival);
        a.inventory.armor = boots(ArmorMaterial::Iron, protected.then_some((Enchantment::ProjectileProtection, 4)));
        a.hurt(10.0, cause(t), DVec3::ZERO, entities)
    };
    let projectiles: Vec<_> = DamageType::ALL.iter().copied().filter(|t| t.is(DamageTag::IsProjectile)).collect();
    assert_eq!(projectiles.len(), 8);
    for t in [DamageType::Trident, DamageType::WindCharge, DamageType::Thrown, DamageType::MobProjectile] {
        assert!(projectiles.contains(&t));
    }
    for &t in DamageType::ALL {
        let (bare, protected) = (hit(false, t, &mut entities), hit(true, t, &mut entities));
        let factor = if t.is(DamageTag::IsProjectile) { 1.0 - 8.0 / 25.0 } else { 1.0 };
        assert!((protected - bare * factor).abs() < 1e-4, "{t:?}: {bare} -> {protected}");
    }
    // The named causes the game deals.
    for c in [Cause::DROWNED_TRIDENT, Cause::PLAYER_TRIDENT, Cause::WIND_CHARGE, Cause::BREEZE_WIND_CHARGE] {
        assert!(c.is(DamageTag::IsProjectile), "{c:?}");
    }
    assert!(!Cause::LLAMA_SPIT.is(DamageTag::IsProjectile), "Java's spit is not #is_projectile");
}

/// `getDamageAfterArmorAbsorb`: armor reduces and wears only for types
/// outside `#bypasses_armor`, read from the type for every damage type.
#[test]
fn armor_reduces_and_wears_only_outside_bypasses_armor() {
    let mut entities = Entities::new(2);
    for &t in DamageType::ALL {
        let mut a = Agent::new(DVec3::new(0.5, 100.0, 0.5));
        a.set_mode(GameMode::Survival);
        for (slot, piece) in [ArmorPiece::Helmet, ArmorPiece::Chestplate, ArmorPiece::Leggings, ArmorPiece::Boots]
            .into_iter()
            .enumerate()
        {
            a.inventory.armor[slot] = Some(Stack::new(Item::armor(piece, ArmorMaterial::Diamond), 1));
        }
        let (points, toughness) = (a.inventory.armor_points(), a.inventory.armor_toughness());
        let taken = a.hurt(8.0, cause(t), DVec3::ZERO, &mut entities);
        let worn = a.inventory.armor.iter().flatten().any(|s| s.damage > 0);
        if t.is(DamageTag::BypassesArmor) {
            assert_eq!((taken, worn), (8.0, false), "{t:?} bypasses armor");
        } else {
            assert_eq!(taken, survival::armor_reduce(8.0, points, toughness), "{t:?} goes through armor");
            assert!(worn, "{t:?} wears the armor");
        }
    }
    // Dragon's breath is Instant Damage (`indirect_magic`), which ignores armor.
    assert!(Cause::DRAGON_BREATH.is(DamageTag::BypassesArmor));
}

/// `getDamageAfterMagicAbsorb`: Resistance I cuts a fifth, except for
/// `#bypasses_effects` and `#bypasses_resistance`.
#[test]
fn resistance_skips_bypasses_effects_and_bypasses_resistance() {
    for &t in DamageType::ALL {
        let mut v = Vitals::default();
        v.effects.add(Effect::Resistance, 0, 600);
        let taken = v.damage(10.0, cause(t), false);
        let full = t.is(DamageTag::BypassesEffects) || t.is(DamageTag::BypassesResistance);
        assert_eq!(taken, if full { 10.0 } else { 8.0 }, "{t:?}");
    }
}

/// The fix: `LivingEntity.hurtServer` refuses `#is_fire` damage under Fire
/// Resistance at the entry point, so fireball and unattributed fireball
/// direct hits no longer hurt; nothing else is refused.
#[test]
fn fire_resistance_refuses_every_is_fire_type_at_the_entry_point() {
    let mut entities = Entities::new(3);
    for &t in DamageType::ALL {
        let mut a = Agent::new(DVec3::new(0.5, 100.0, 0.5));
        a.set_mode(GameMode::Survival);
        a.vitals.effects.add(Effect::FireResistance, 0, 600);
        let taken = a.hurt(4.0, cause(t), DVec3::ZERO, &mut entities);
        if t.is(DamageTag::IsFire) {
            assert_eq!((taken, a.vitals.health), (0.0, MAX_HEALTH), "{t:?} is refused");
        } else {
            assert!(taken > 0.0, "{t:?} still hurts");
        }
    }
    let mut v = Vitals::default();
    v.effects.add(Effect::FireResistance, 0, 600);
    assert_eq!(v.damage(5.0, Cause::BLAZE_FIREBALL, false), 0.0);
    assert!(v.damage(5.0, Cause::GHAST_FIREBALL, false) > 0.0, "the ghast blast is an explosion");
}

/// `Player.hurtServer`: creative (`abilities.invulnerable`) stops everything
/// but `#bypasses_invulnerability`.
#[test]
fn creative_players_take_only_bypasses_invulnerability() {
    for &t in DamageType::ALL {
        let mut v = Vitals::default();
        let taken = v.damage(5.0, cause(t), true);
        assert_eq!(taken > 0.0, t.is(DamageTag::BypassesInvulnerability), "{t:?}");
    }
}

/// `checkTotemDeathProtection` returns false for `#bypasses_invulnerability`.
#[test]
fn totems_save_from_everything_but_bypasses_invulnerability() {
    for &t in DamageType::ALL {
        let mut a = Agent::new(DVec3::new(0.5, 100.0, 0.5));
        a.set_mode(GameMode::Survival);
        a.inventory.offhand = Some(Stack::new(Item::TOTEM_OF_UNDYING, 1));
        a.vitals.damage(f32::MAX, cause(t), false);
        assert!(a.vitals.is_dead(), "{t:?}");
        assert_eq!(a.vitals.killed_by(), Some(t));
        assert_eq!(a.totem_saves(0), !t.is(DamageTag::BypassesInvulnerability), "{t:?}");
    }
}

/// `Player.actuallyHurt`: a hit that takes health costs the type's exhaustion
/// (0.1 or nothing), not a flat 0.1.
#[test]
fn food_exhaustion_follows_the_damage_type() {
    let f = fixture();
    for j in f["damage_types"].as_array().unwrap() {
        let t = DamageType::from_id(j["id"].as_str().unwrap()).unwrap();
        let mut v = Vitals::default();
        v.damage(2.0, cause(t), false);
        assert_eq!(v.hunger.exhaustion, j["exhaustion"].as_f64().unwrap() as f32, "{t:?}");
    }
    let mut v = Vitals::default();
    v.damage(3.0, Cause::FALL, false);
    assert_eq!((v.health, v.hunger.exhaustion), (MAX_HEALTH - 3.0, 0.0), "falls cost no food");
}

fn world() -> World {
    let mut saved = rustc_hash::FxHashMap::default();
    saved.insert(IVec3::new(0, 4, 0), Arc::new(ChunkData::Uniform(Block::AIR)));
    let mut world = World::new_headless(Arc::new(Generator::new(1)), saved, 2);
    for _ in 0..10_000 {
        world.update(DVec3::new(1.0, 150.0, 1.0));
        if world.is_loaded(IVec3::new(1, 150, 1)) {
            for x in -3..=5 {
                for z in -3..=5 {
                    world.set_block(IVec3::new(x, 149, z), Block::STONE);
                }
            }
            return world;
        }
        std::thread::yield_now();
    }
    panic!("world failed to load");
}

/// `Player.isInvulnerableTo`: each player damage rule turned off makes exactly
/// its tag's types harmless, through the agent after a rules tick.
#[test]
fn player_damage_rules_follow_the_tags() {
    let mut world = world();
    let mut entities = Entities::new(4);
    for (rule, tag) in [
        ("fall_damage", DamageTag::IsFall),
        ("fire_damage", DamageTag::IsFire),
        ("drowning_damage", DamageTag::IsDrowning),
        ("freeze_damage", DamageTag::IsFreezing),
    ] {
        let mut rules = GameRules::default();
        rules.set(rule, "false").unwrap();
        for &t in DamageType::ALL {
            assert_eq!(rules.damage_immunity().blocks(cause(t)), t.is(tag), "{rule} and {t:?}");
            assert!(!GameRules::default().damage_immunity().blocks(cause(t)), "{t:?} hurts by default");
        }
        let representative = DamageType::ALL.iter().copied().find(|t| t.is(tag)).unwrap();
        let mut agent = Agent::new(DVec3::new(1.5, 150.0, 1.5));
        agent.set_mode(GameMode::Survival);
        agent.tick_rules(&mut world, &mut entities, Difficulty::Normal, &rules);
        agent.vitals.health = MAX_HEALTH;
        assert_eq!(agent.hurt(4.0, cause(representative), DVec3::ZERO, &mut entities), 0.0, "{rule}");
        assert!(agent.hurt(4.0, Cause::mob("was slain by a zombie"), DVec3::ZERO, &mut entities) > 0.0);
    }
}

/// `template` ("%1$s was shot by %2$s") without its victim, as a matcher:
/// the literal parts in order, each placeholder standing for some text.
fn says(template: &str, text: &str) -> bool {
    let Some(rest) = template.strip_prefix("%1$s ") else { return false };
    let mut parts = Vec::new();
    let mut tail = rest;
    while let Some(i) = tail.find('%') {
        parts.push(&tail[..i]);
        tail = &tail[i + 4..];
    }
    parts.push(tail);
    if parts.len() == 1 {
        return text == parts[0];
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    let Some(mut middle) = text.strip_prefix(first).and_then(|t| t.strip_suffix(last)) else { return false };
    for part in &parts[1..parts.len() - 1] {
        let Some(at) = middle.get(1..).and_then(|m| m.find(part)) else { return false };
        middle = &middle[at + 1 + part.len()..];
    }
    !middle.is_empty()
}

/// Named causes whose VoxelCraft wording differs from the Java death message
/// for their type. The type (and every rule it drives) is still Java's.
const WORDING: &[(&str, &str)] = &[
    ("was fireballed by a ghast", "Java names the blast's owner: death.attack.explosion.player"),
    ("was spat on by a llama", "spit uses death.attack.mob"),
    ("was blasted by a wind charge", "a player's wind charge uses death.attack.mob"),
    ("was pricked by a guardian", "guardian spikes are thorns damage: death.attack.thorns"),
    ("was killed trying to hurt a player", "death.attack.thorns says 'while trying to hurt'"),
    ("was stung by a bee", "death.attack.sting says 'was stung to death by'"),
    ("was stung by a pufferfish", "pufferfish contact is mob_attack: death.attack.mob"),
    ("was rammed by a goat", "a goat's ram is mob_attack: death.attack.mob"),
    ("was killed by dragon's breath", "the breath cloud's Instant Damage is death.attack.indirectMagic"),
];

/// Every named cause the game deals says (one of) its Java type's death
/// messages, so a cause typed wrongly fails here; the wording differences
/// above are listed, and the list holds nothing that matches.
#[test]
fn named_causes_say_their_java_types_death_message() {
    let f = fixture();
    let messages = f["death_messages"].as_object().unwrap();
    let mut texts = BTreeSet::new();
    for c in Cause::KNOWN {
        texts.insert((c.text, c.kind));
        let base = format!("death.attack.{}", c.kind.message_id());
        let mut keys = vec![base.clone(), format!("{base}.player")];
        match c.kind.death_message_type() {
            DeathMessageType::FallVariants => {
                keys.extend(messages.keys().filter(|k| k.starts_with("death.fell.accident.")).cloned())
            }
            DeathMessageType::IntentionalGameDesign => keys.push(format!("{base}.message")),
            DeathMessageType::Default => {}
        }
        let matches = keys.iter().filter_map(|k| messages.get(k)?.as_str()).any(|m| says(m, c.text));
        let listed = WORDING.iter().find(|(text, _)| *text == c.text);
        match (matches, listed) {
            (true, None) | (false, Some(_)) => {}
            (true, Some(_)) => panic!("{c:?} now says its Java message; drop it from WORDING"),
            (false, None) => panic!("{c:?} says none of {keys:?}"),
        }
    }
    assert_eq!(texts.len(), Cause::KNOWN.len(), "named causes are distinct");
    for (text, _) in WORDING {
        assert!(Cause::KNOWN.iter().any(|c| c.text == *text), "{text} is not a named cause");
    }
    // Bare text from code that has not named its cause yet resolves to the
    // named cause, or a mob's melee hit.
    assert_eq!(Cause::from("drowned"), Cause::DROWN);
    assert_eq!(Cause::from("was slain by a husk").kind, DamageType::MobAttack);
}

/// The fix: a ghast fireball's blast is `player_explosion`
/// (`LargeFireball.onHit` explodes with the ghast as its indirect source), so
/// Blast Protection reduces it and Fire and Projectile Protection do not, and
/// a blaze fireball's direct hit takes Fire and Projectile Protection.
#[test]
fn ghast_fireball_blast_takes_blast_protection_not_fire_or_projectile() {
    assert_eq!(Cause::GHAST_FIREBALL.kind, DamageType::PlayerExplosion);
    assert_eq!(Cause::BLAZE_FIREBALL.kind, DamageType::Fireball);
    let factor = |e: Enchantment, c: Cause| enchant::protection_factor(&boots(ArmorMaterial::Iron, Some((e, 4))), c);
    assert_eq!(factor(Enchantment::BlastProtection, Cause::GHAST_FIREBALL), 8);
    assert_eq!(factor(Enchantment::FireProtection, Cause::GHAST_FIREBALL), 0);
    assert_eq!(factor(Enchantment::ProjectileProtection, Cause::GHAST_FIREBALL), 0);
    assert_eq!(factor(Enchantment::FireProtection, Cause::BLAZE_FIREBALL), 8);
    assert_eq!(factor(Enchantment::ProjectileProtection, Cause::BLAZE_FIREBALL), 8);
    assert_eq!(factor(Enchantment::BlastProtection, Cause::BLAZE_FIREBALL), 0);
}

/// The agent's own survival tick (CLI, controller and LAN players) types its
/// environmental damage as the host does: lava goes through armor, and magma
/// is `hot_floor` with Java's death message.
#[test]
fn agent_environment_damage_is_typed_and_goes_through_armor() {
    let mut world = world();
    let mut entities = Entities::new(5);
    let rules = GameRules::default();
    world.set_block(IVec3::new(1, 150, 1), Block::LAVA);
    let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
    a.set_mode(GameMode::Survival);
    for (slot, piece) in
        [ArmorPiece::Helmet, ArmorPiece::Chestplate, ArmorPiece::Leggings, ArmorPiece::Boots].into_iter().enumerate()
    {
        a.inventory.armor[slot] = Some(Stack::new(Item::armor(piece, ArmorMaterial::Diamond), 1));
    }
    let (points, toughness) = (a.inventory.armor_points(), a.inventory.armor_toughness());
    a.tick_rules(&mut world, &mut entities, Difficulty::Normal, &rules);
    let lost = MAX_HEALTH - a.vitals.health;
    assert!((lost - survival::armor_reduce(4.0, points, toughness)).abs() < 1e-5, "lava through armor: {lost}");
    assert!(a.inventory.armor.iter().flatten().all(|s| s.damage > 0), "lava wears the armor");

    world.set_block(IVec3::new(1, 150, 1), Block::AIR);
    world.set_block(IVec3::new(1, 149, 1), Block::MAGMA);
    let mut a = Agent::new(DVec3::new(1.5, 150.0, 1.5));
    a.set_mode(GameMode::Survival);
    a.vitals.health = 1.0;
    for _ in 0..5 {
        a.tick_rules(&mut world, &mut entities, Difficulty::Normal, &rules);
    }
    assert_eq!(a.vitals.death.as_deref(), Some(Cause::HOT_FLOOR.text));
    assert_eq!(a.vitals.killed_by(), Some(DamageType::HotFloor));
}
