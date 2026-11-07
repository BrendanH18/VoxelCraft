use super::*;

#[test]
fn spawner_and_vault_states_round_trip() {
    for ominous in [false, true] {
        for s in SpawnerState::ALL {
            let b = spawner_block(s, ominous);
            assert_eq!(spawner(b), Some((s, ominous)));
            assert_eq!(b.base(), TRIAL_SPAWNER);
            assert_eq!(b.emission(), s.light());
            assert_eq!(b.name(), "trial spawner");
        }
        for f in Facing::ALL {
            for s in VaultState::ALL {
                let b = vault_block(f, s, ominous);
                assert_eq!(vault(b), Some((f, s, ominous)));
                assert_eq!(b.emission(), s.light());
                assert_eq!(b.with_facing(Facing::West), vault_block(Facing::West, s, ominous));
            }
        }
    }
    assert_eq!(Block::from_name("vault"), Some(VAULT));
    assert_eq!(Block::from_name("ominous_vault"), Some(OMINOUS_VAULT));
    assert_eq!(Block::from_name("trial_spawner"), Some(TRIAL_SPAWNER));
    assert!((SPAWNER_FIRST..=SPAWNER_LAST).chain(VAULT_FIRST..=VAULT_LAST).all(|i| i <= RESERVED_LAST));
}

#[test]
fn blocks_have_java_strength_and_no_drops() {
    for b in [TRIAL_SPAWNER, VAULT, OMINOUS_VAULT] {
        assert_eq!(b.hardness(), 50.0);
        assert_eq!(super::super::explosion::resistance(b), 50.0);
        assert_eq!(b.drop(), None);
    }
}

#[test]
fn items_resolve_by_java_name() {
    assert_eq!(crate::item::Item::from_name("trial_key"), Some(TRIAL_KEY));
    assert_eq!(crate::item::Item::from_name("ominous_trial_key"), Some(OMINOUS_TRIAL_KEY));
    assert_eq!(crate::item::Item::from_name("breeze_rod"), Some(BREEZE_ROD));
    assert_eq!(crate::item::Item::from_name("wind_charge"), Some(WIND_CHARGE));
    assert_eq!(crate::item::Item::from_name("ominous_bottle"), Some(OMINOUS_BOTTLE));
    for a in 0..5u8 {
        assert_eq!(ominous_amplifier(ominous_bottle(a)), Some(a));
        assert!(ominous_bottle(a).is_drink());
    }
    assert!(TRIAL_CHAMBERS_MAP.is_map());
    assert_eq!(java_item("buried_trial_chambers_map"), Some(TRIAL_CHAMBERS_MAP));
    assert_eq!(super::super::loot::java_item("trial_key"), Some(TRIAL_KEY));
}

#[test]
fn spawner_configs_match_java_numbers() {
    let id = |n: &str| spawner::config_id(n).unwrap();
    let s = spawner::TrialSpawner { config: Some(id("trial_chamber/breeze")), ..Default::default() };
    let (normal, ominous) = (s.config(false), s.config(true));
    // Breeze: 2 total + 1 per extra player, 1 at once + 0.5 per extra player.
    assert_eq!((normal.target_total(0), normal.target_total(2)), (2, 4));
    assert_eq!(
        (normal.target_simultaneous(0), normal.target_simultaneous(1), normal.target_simultaneous(2)),
        (1, 1, 2)
    );
    assert_eq!((ominous.target_total(0), ominous.target_simultaneous(0)), (4, 2));
    assert_eq!(normal.ticks_between_spawn, 20);
    // Defaults (`TrialSpawnerConfig$Builder`): 6 + 2 per player, range 4.
    let z = spawner::TrialSpawner { config: Some(id("trial_chamber/melee/zombie")), ..Default::default() };
    assert_eq!((z.config(false).target_total(1), z.config(false).spawn_range), (8, 4));
    assert_eq!(z.config(true).potentials[0].equipment, Some("equipment/trial_chamber_melee"));
    let slow = spawner::TrialSpawner { config: Some(id("trial_chamber/slow_ranged/stray")), ..Default::default() };
    assert_eq!((slow.config(false).ticks_between_spawn, slow.config(false).target_simultaneous(1)), (160, 6));
    assert_eq!(
        z.config(true).loot,
        &[("spawners/ominous/trial_chamber/key", 3), ("spawners/ominous/trial_chamber/consumables", 7)]
    );
    assert!(spawner::TrialSpawner::default().config(false).potentials.is_empty());
}

#[test]
fn block_entities_save_and_reload() {
    let g = std::sync::Arc::new(super::super::terrain::Generator::new(3));
    let mut w = super::super::World::new_headless(g.clone(), Default::default(), 1);
    let s = spawner::TrialSpawner {
        config: spawner::config_id("trial_chamber/small_melee/slime"),
        detected: vec![0, 4],
        current_mobs: vec![17, 18],
        cooldown_ends_at: 123,
        next_mob_spawns_at: 77,
        total_mobs_spawned: 3,
        next_spawn: Some(1),
        ejecting_loot: Some(0),
    };
    let v = vault::Vault {
        rewarded: vec![0, 9],
        resumes_at: 50,
        items_to_eject: vec![Stack::new(crate::item::Item::DIAMOND, 2)],
        total_ejections: 3,
        connected: vec![9],
        display: Some(Stack::new(WIND_CHARGE, 4)),
        last_insert_fail: 12,
    };
    w.trial.spawners.insert([1, -30, 2], s.clone());
    w.trial.vaults.insert([4, -30, 5], v.clone());
    let saved = w.trial_to_string();
    let mut other = super::super::World::new_headless(g, Default::default(), 1);
    other.load_trial(&saved);
    assert_eq!(other.trial.spawners[&[1, -30, 2]], s);
    assert_eq!(other.trial.vaults[&[4, -30, 5]], v);
    assert_eq!(other.trial_to_string(), saved);
}

#[test]
fn vault_rewards_remember_at_most_128_players() {
    let mut v = vault::Vault::default();
    for p in 0..130 {
        v.reward(p);
    }
    assert_eq!(v.rewarded.len(), 128);
    assert!(!v.rewarded.contains(&0) && v.rewarded.contains(&129));
}

/// v0.16 integration: the trial tables' entries owned by other tracks (the
/// heavy core, discs, trident, Wind Burst books) and the mace's breeze rod
/// resolve once every track is merged.
#[test]
fn cross_track_loot_and_recipe_items_resolve() {
    use crate::enchant::Enchantment;
    use crate::item::Item;
    for name in [
        "heavy_core",
        "music_disc_creator",
        "music_disc_creator_music_box",
        "music_disc_precipice",
        "trident",
        "mace",
        "echo_shard",
    ] {
        assert!(Item::from_name(name).is_some(), "{name} is not registered");
    }
    assert_eq!(Enchantment::from_name("wind_burst"), Some(Enchantment::WindBurst));
    assert_eq!(Enchantment::from_name("swift_sneak"), Some(Enchantment::SwiftSneak));
    assert_eq!(crate::weapons::breeze_rod(), Some(BREEZE_ROD));
}
