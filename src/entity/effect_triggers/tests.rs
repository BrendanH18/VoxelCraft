use super::*;
use crate::simulation::effects::Effect;
use crate::world::{World, block::Block};

fn world() -> World {
    let g = std::sync::Arc::new(crate::world::terrain::Generator::new(4));
    let mut w = World::new_headless(g, Default::default(), 2);
    for x in -1..=1 {
        for z in -1..=1 {
            w.load_test_column(glam::IVec2::new(x, z), 63);
        }
    }
    w
}

fn kill_with(e: &mut Entities, kind: MobKind, at: DVec3, effect: Effect) -> u32 {
    e.spawn(kind, at);
    let uid = 900 + e.mobs.len() as u32;
    let m = e.mobs.last_mut().unwrap();
    m.uid = uid;
    m.effects.add(effect, 0, 3600);
    m.health = 0.0;
    m.dying = Some(0.0);
    m.uid
}

/// `WindChargedMobEffect.onMobRemoved`: a burst of strength 3 to 5 at half
/// the body's height, with the wind charge calculator's knockback of 1.
#[test]
fn wind_charged_bodies_burst() {
    let mut w = world();
    let mut e = Entities::new(3);
    kill_with(&mut e, MobKind::Zombie, DVec3::new(4.5, 64.0, 4.5), Effect::WindCharged);
    let mut events = Vec::new();
    e.settle_effect_hooks(&mut w, &mut events);
    let Some(EntityEvent::WindBurst(b)) = events.first() else { panic!("{events:?}") };
    assert!((3.0..5.0).contains(&b.radius), "{}", b.radius);
    assert_eq!((b.knockback, b.resets_fall, b.owner), (1.0, false, None));
    let height = e.mobs[0].shape().height as f32;
    assert_eq!(b.center.y, 64.0 + f64::from(height / 2.0));
    assert!(e.mobs[0].effects.is_empty(), "Java clears the effects on death");
    // Settled once: nothing more next tick.
    events.clear();
    e.settle_effect_hooks(&mut w, &mut events);
    assert!(events.is_empty());
}

/// A creeper that explodes is discarded, not killed: its effects don't fire.
#[test]
fn discarded_bodies_keep_their_effects() {
    let mut w = world();
    let mut e = Entities::new(3);
    kill_with(&mut e, MobKind::Creeper, DVec3::new(4.5, 64.0, 4.5), Effect::WindCharged);
    e.mobs[0].dying = Some(mob::DEATH_TIME);
    let mut events = Vec::new();
    e.settle_effect_hooks(&mut w, &mut events);
    assert!(events.is_empty());
}

/// `WeavingMobEffect`: 2 or 3 cobwebs on sturdy ground around the body,
/// for mobs only with `mobGriefing`.
#[test]
fn weaving_bodies_leave_cobwebs_only_with_mob_griefing() {
    let cobwebs = |w: &World| {
        let mut n = 0;
        for x in 2..=6 {
            for y in 63..=66 {
                for z in 2..=6 {
                    n += usize::from(w.get_block(IVec3::new(x, y, z)) == Some(Block::COBWEB));
                }
            }
        }
        n
    };
    let mut w = world();
    let mut e = Entities::new(3);
    e.villager_griefing = false;
    kill_with(&mut e, MobKind::Zombie, DVec3::new(4.5, 64.0, 4.5), Effect::Weaving);
    e.settle_effect_hooks(&mut w, &mut Vec::new());
    assert_eq!(cobwebs(&w), 0);
    // A player weaves whatever the rule says.
    let mut effects = Effects::default();
    effects.add(Effect::Weaving, 0, 3600);
    e.player_death_effects(&effects, DVec3::new(4.5, 64.0, 4.5));
    e.settle_effect_hooks(&mut w, &mut Vec::new());
    let first = cobwebs(&w);
    assert!((1..=3).contains(&first), "{first}");
    // Only on the floor layer: the cells at y 64 stand on the y 63 ground.
    for x in 3..=5 {
        for z in 3..=5 {
            assert_ne!(w.get_block(IVec3::new(x, 65, z)), Some(Block::COBWEB), "no web without sturdy ground");
        }
    }
    let mut w = world();
    e.villager_griefing = true;
    let mut total = 0;
    for _ in 0..20 {
        kill_with(&mut e, MobKind::Zombie, DVec3::new(4.5, 64.0, 4.5), Effect::Weaving);
        e.settle_effect_hooks(&mut w, &mut Vec::new());
        total = cobwebs(&w);
    }
    assert!(total >= 3, "{total}");
}

/// `OozingMobEffect`: two size-2 slimes, capped by `max_entity_cramming`
/// counting other slimes within 2 blocks.
#[test]
fn oozing_bodies_split_into_slimes_under_the_cramming_cap() {
    let mut w = world();
    let at = DVec3::new(4.5, 64.0, 4.5);
    let mut e = Entities::new(3);
    let uid = kill_with(&mut e, MobKind::Zombie, at, Effect::Oozing);
    e.settle_effect_hooks(&mut w, &mut Vec::new());
    let slimes: Vec<_> = e.mobs.iter().filter(|m| m.kind == MobKind::Slime).collect();
    assert_eq!(slimes.len(), 2);
    assert!(slimes.iter().all(|m| m.size == 2 && m.health == 4.0 && m.pos == at + DVec3::Y * 0.5));
    assert!(e.mobs.iter().any(|m| m.uid == uid));
    // Cramming 3 with two slimes already beside it: only one more.
    e.max_entity_cramming = 3;
    kill_with(&mut e, MobKind::Zombie, at, Effect::Oozing);
    e.settle_effect_hooks(&mut w, &mut Vec::new());
    assert_eq!(e.mobs.iter().filter(|m| m.kind == MobKind::Slime).count(), 3);
    // Below 1, the rule doesn't limit them.
    e.max_entity_cramming = 0;
    kill_with(&mut e, MobKind::Zombie, at, Effect::Oozing);
    e.settle_effect_hooks(&mut w, &mut Vec::new());
    assert_eq!(e.mobs.iter().filter(|m| m.kind == MobKind::Slime).count(), 5);
    // Far slimes don't count.
    let mut e = Entities::new(4);
    e.max_entity_cramming = 2;
    e.spawn(MobKind::Slime, at + DVec3::X * 6.0);
    e.spawn(MobKind::Slime, at + DVec3::X * 6.0);
    kill_with(&mut e, MobKind::Zombie, at, Effect::Oozing);
    e.settle_effect_hooks(&mut w, &mut Vec::new());
    assert_eq!(e.mobs.iter().filter(|m| m.kind == MobKind::Slime).count(), 4);
}

/// `InfestedMobEffect`: a 10% chance per hit of one or two silverfish.
#[test]
fn infested_hits_release_silverfish() {
    let mut w = world();
    let mut e = Entities::new(5);
    e.spawn(MobKind::IronGolem, DVec3::new(4.5, 64.0, 4.5));
    e.mobs[0].effects.add(Effect::Infested, 0, 3600);
    let mut rng = Rng::new(7);
    for _ in 0..200 {
        e.mobs[0].health = 100.0;
        // v1.0 gap Track 8d: separate hits, each past the last one's cooldown.
        e.mobs[0].damage_cooldown = 0;
        e.mobs[0].damage(0.5, None, &mut rng);
    }
    let rolled = e.mobs[0].infested;
    // 200 hits at 0.1, one or two each.
    assert!((8..=60).contains(&rolled), "{rolled}");
    e.settle_effect_hooks(&mut w, &mut Vec::new());
    assert_eq!(e.mobs.iter().filter(|m| m.kind == MobKind::Silverfish).count(), usize::from(rolled));
    assert_eq!(e.mobs[0].infested, 0);
    // Without the effect nothing is rolled.
    e.mobs[0].effects.clear();
    for _ in 0..100 {
        e.mobs[0].damage(0.5, None, &mut rng);
    }
    assert_eq!(e.mobs[0].infested, 0);
    // A player's hits roll the same chance.
    let before = e.mobs.len();
    e.player_infested_hits(200, DVec3::new(4.5, 65.0, 4.5), DVec3::X);
    let fish = e.mobs.len() - before;
    assert!((8..=60).contains(&fish), "{fish}");
}

/// The chance and counts match `MobEffects`.
#[test]
fn constants_match_mob_effects() {
    assert_eq!(effects::INFESTED_CHANCE, 0.1);
    assert_eq!(effects::INFESTED_SILVERFISH, (1, 2));
    assert_eq!(effects::WEAVING_COBWEBS, (2, 3));
    assert_eq!((effects::OOZING_SLIMES, effects::OOZING_SLIME_SIZE), (2, 2));
}

fn quiet_ctx() -> Ctx {
    Ctx {
        players: vec![Target::new(PlayerId::HOST, DVec3::new(60.0, 10.0, 0.0), true)],
        daylight: 1.0,
        spawn_light: crate::simulation::lighting::SpawnLight::NIGHT,
        spawning: false,
        raining: false,
        dimension: crate::world::terrain::Dimension::Overworld,
    }
}

/// `ThrownEgg.onHit`: chicks hatch with the egg's `chicken/variant`.
#[test]
fn thrown_eggs_hatch_their_variant() {
    use crate::content::farm_variant::{BLUE_EGG, BROWN_EGG, FarmVariant};
    let world = crate::physics::test_util::Grid::flat(10);
    for (item, variant) in [
        (BLUE_EGG, FarmVariant::Cold),
        (BROWN_EGG, FarmVariant::Warm),
        (crate::item::Item::EGG, FarmVariant::Temperate),
    ] {
        let kind = thrown::Kind::from_item(item).unwrap();
        assert_eq!(kind.egg_variant(), Some(variant));
        let mut e = Entities::new(11);
        for i in 0..160 {
            let at = DVec3::new(0.5 + f64::from(i % 16), 12.0, 0.5 + f64::from(i / 16));
            let t = thrown::Thrown::launch(kind, PlayerId::HOST, at, DVec3::NEG_Y, DVec3::ZERO, &mut e.rng);
            e.thrown.push(t);
        }
        for _ in 0..40 {
            e.update(0.05, &world, &quiet_ctx());
        }
        let chicks: Vec<_> = e.mobs.iter().filter(|m| m.kind == MobKind::Chicken).collect();
        assert!(!chicks.is_empty(), "about one egg in eight hatches");
        assert!(chicks.iter().all(|m| m.baby && m.farm_variant() == variant), "{variant:?}");
    }
}

/// `gameplay/chicken_lay`: warm chickens lay brown eggs, cold ones blue.
#[test]
fn chickens_lay_their_variants_egg() {
    use crate::content::farm_variant::FarmVariant;
    let world = crate::physics::test_util::Grid::flat(10);
    for variant in FarmVariant::ALL {
        let mut e = Entities::new(3);
        e.spawn(MobKind::Chicken, DVec3::new(0.5, 10.0, 0.5));
        e.mobs[0].set_farm_variant(variant);
        e.mobs[0].egg_timer = 0.01;
        e.update(0.05, &world, &quiet_ctx());
        assert!(e.items.iter().any(|i| i.stack.item == variant.egg()), "{variant:?}");
        // A baby bred from it takes a parent's variant.
        e.spawn(MobKind::Chicken, DVec3::new(1.5, 10.0, 0.5));
        e.mobs[1].set_farm_variant(variant);
        let child = e.animal_child(0, 1);
        assert_eq!(child.farm_variant(), variant);
    }
    // Other mobs have none.
    let mut e = Entities::new(3);
    e.spawn(MobKind::Sheep, DVec3::ZERO);
    e.mobs[0].set_farm_variant(FarmVariant::Cold);
    assert_eq!(e.mobs[0].farm_variant(), FarmVariant::Temperate);
}

/// Natural spawns pick the biome's variant (`VariantUtils.selectVariantToSpawn`).
#[test]
fn natural_farm_animals_take_the_biome_variant() {
    use crate::content::farm_variant::FarmVariant;
    use crate::world::terrain::Biome;
    for (biome, variant) in [
        (Biome::Plains, FarmVariant::Temperate),
        (Biome::Savanna, FarmVariant::Warm),
        (Biome::Taiga, FarmVariant::Cold),
    ] {
        for kind in [MobKind::Chicken, MobKind::Cow, MobKind::Pig] {
            let mut e = Entities::new(2);
            e.spawn_natural(kind, DVec3::new(0.5, 64.0, 0.5), biome, &mut Default::default());
            assert_eq!(e.mobs[0].farm_variant(), variant, "{kind:?} in {biome:?}");
        }
    }
}

/// `SpectralArrow`: Glowing for 200 ticks on what it hits, picked up as a
/// spectral arrow, shot from dispensers.
#[test]
fn spectral_arrows_make_their_target_glow() {
    let mut arrow = Arrow::shot(DVec3::ZERO, DVec3::X, 1.0, true);
    arrow.spectral = true;
    assert_eq!(arrow.pickup_stack(), crate::inventory::Stack::new(crate::item::Item::SPECTRAL_ARROW, 1));
    let mut m = Mob::new(MobKind::Zombie, DVec3::ZERO, 0.0);
    arrow.effects_on_mob(&mut m, &mut Rng::new(1));
    let glow = m.effects.get(Effect::Glowing).unwrap();
    assert_eq!((glow.amplifier, glow.ticks), (0, effects::SPECTRAL_GLOWING_TICKS));
    assert!(m.effects.glowing());
    let mut events = Vec::new();
    arrow.effects_on_player(PlayerId::HOST, &mut events);
    assert!(matches!(
        events[..],
        [EntityEvent::PlayerEffect { effect: Effect::Glowing, amplifier: 0, ticks: 200, .. }]
    ));
    let mut e = Entities::new(1);
    e.dispense_arrow_stack(crate::inventory::Stack::new(crate::item::Item::SPECTRAL_ARROW, 1), IVec3::ZERO, IVec3::X);
    assert!(e.arrows[0].spectral && e.arrows[0].potion.is_none());
}

/// Glowing entities' geometry is copied for the outline mask, and nothing
/// else is.
#[test]
fn glowing_mobs_feed_the_outline_mask() {
    let mut mesh = model::EntityMesh::default();
    let mut mobs = vec![Mob::new(MobKind::Zombie, DVec3::new(2.0, 0.0, 0.0), 0.0)];
    model::build(&mobs, DVec3::ZERO, glam::Vec3::X, 64.0, 1.0, &mut mesh);
    assert!(mesh.glow_boxes().is_empty() && mesh.glow_verts().is_empty());
    mobs[0].effects.add(Effect::Glowing, 0, 200);
    mesh.clear();
    model::build(&mobs, DVec3::ZERO, glam::Vec3::X, 64.0, 1.0, &mut mesh);
    assert_eq!(mesh.glow_boxes().len(), mesh.boxes().len());
    assert_eq!(mesh.glow_verts().len(), mesh.len());
    assert!(!mesh.glow_boxes().is_empty() || !mesh.glow_verts().is_empty());
}

/// Health Boost raises a mob's maximum health by 4 a level; when it ends,
/// health above the plain maximum goes.
#[test]
fn health_boost_raises_and_then_caps_mob_health() {
    let mut m = Mob::new(MobKind::Cow, DVec3::ZERO, 0.0);
    let base = m.max_health();
    m.effects.add(Effect::HealthBoost, 1, 2);
    assert_eq!(m.max_health(), base + 8.0);
    m.health = base + 8.0;
    let world = crate::physics::test_util::Grid::flat(-10);
    let mut e = Entities::new(1);
    e.mobs.push(m);
    for _ in 0..4 {
        e.update(0.05, &world, &quiet_ctx());
    }
    assert!(!e.mobs[0].effects.has(Effect::HealthBoost));
    assert!(e.mobs[0].health <= base, "{}", e.mobs[0].health);
}
