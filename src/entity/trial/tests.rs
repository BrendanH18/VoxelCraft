use super::*;
use crate::world::block::{Block, Facing};
use crate::world::terrain::Generator;
use crate::world::trial::vault::VaultUse;
use crate::world::trial::{self as tr, OMINOUS_TRIAL_KEY, TRIAL_KEY};
use std::sync::Arc;

fn world() -> World {
    let mut w = World::new_headless(Arc::new(Generator::new(91)), Default::default(), 2);
    for x in -1..=1 {
        for z in -1..=1 {
            w.load_test_column(glam::IVec2::new(x, z), 63);
        }
    }
    w
}

const RULES: TrialRules = TrialRules { spawner_blocks: true, spawn_mobs: true, peaceful: false };

fn player(id: u32, pos: DVec3) -> TrialPlayer {
    TrialPlayer { id: PlayerId(id), pos, eye: pos + DVec3::Y * 1.62, counts: true, bad_omen: None, trial_omen: false }
}

fn step(w: &mut World, e: &mut Entities, players: &[TrialPlayer]) -> Vec<TrialOutcome> {
    w.maps.game_ticks += 1;
    e.tick_trial(w, players, RULES)
}

fn state(w: &World, p: IVec3) -> (SpawnerState, bool) {
    spawner(w.get_block(p).unwrap()).unwrap()
}

#[test]
fn a_trial_spawner_runs_its_whole_cycle() {
    let mut w = world();
    let mut e = Entities::new(5);
    let p = IVec3::new(8, 64, 8);
    w.set_block(p, tr::TRIAL_SPAWNER);
    assert!(w.set_trial_spawner_config(p, "trial_chamber/melee/zombie"));
    let players = [player(0, DVec3::new(8.5, 64.0, 13.5))];
    // Inactive -> waiting for players -> active once a scan sees the player.
    for _ in 0..25 {
        step(&mut w, &mut e, &players);
    }
    assert_eq!(state(&w, p).0, SpawnerState::Active);
    assert_eq!(w.trial_spawner(p).unwrap().detected, vec![0]);
    // Six zombies for one player, at most three at once; killing them
    // lets the rest in, then the reward is ejected.
    let mut spawned = 0;
    for _ in 0..4000 {
        step(&mut w, &mut e, &players);
        let tracked = tracked(&w, p);
        assert!(tracked.len() <= 3);
        if !tracked.is_empty() {
            spawned += tracked.len();
            e.mobs.retain(|m| !tracked.contains(&m.uid));
        }
        if state(&w, p).0 != SpawnerState::Active {
            break;
        }
    }
    assert!(spawned >= 6, "{spawned}");
    assert_eq!(state(&w, p).0, SpawnerState::WaitingForRewardEjection);
    let items = e.items.len();
    // The shutter opens 40 ticks after the last kill; ejections land every
    // 30 ticks from then (at 60), and the next one closes it (at 90).
    for _ in 0..100 {
        step(&mut w, &mut e, &players);
    }
    assert_eq!(state(&w, p).0, SpawnerState::Cooldown, "one ejection per detected player, then the shutter closes");
    assert!(e.items.len() > items);
    // Thirty minutes of cooldown, then it waits for players again.
    for _ in 0..36_000 {
        w.maps.game_ticks += 1;
        e.tick_trial(&mut w, &[], RULES);
    }
    assert_eq!(state(&w, p).0, SpawnerState::WaitingForPlayers);
    assert!(w.trial_spawner(p).unwrap().detected.is_empty());
}

#[test]
fn bad_omen_turns_a_spawner_ominous_and_becomes_trial_omen() {
    let mut w = world();
    let mut e = Entities::new(6);
    let p = IVec3::new(8, 64, 8);
    w.set_block(p, tr::TRIAL_SPAWNER);
    w.set_trial_spawner_config(p, "trial_chamber/melee/zombie");
    let mut players = [player(0, DVec3::new(8.5, 64.0, 13.5))];
    for _ in 0..25 {
        step(&mut w, &mut e, &players);
    }
    for _ in 0..60 {
        step(&mut w, &mut e, &players);
    }
    let before = tracked(&w, p);
    assert!(!before.is_empty());
    players[0].bad_omen = Some(2);
    let mut outcomes = Vec::new();
    for _ in 0..25 {
        outcomes.extend(step(&mut w, &mut e, &players));
    }
    assert_eq!(outcomes, vec![TrialOutcome::TrialOmen { player: PlayerId(0), ticks: 18_000 * 3 }]);
    assert!(state(&w, p).1, "ominous");
    // `resetAfterBecomingOminous` discards the mobs it had.
    assert!(before.iter().all(|uid| e.mobs.iter().all(|m| m.uid != *uid)));
}

#[test]
fn peaceful_and_the_gamerule_stop_trial_spawners() {
    let mut w = world();
    let mut e = Entities::new(7);
    let p = IVec3::new(8, 64, 8);
    w.set_block(p, tr::TRIAL_SPAWNER);
    w.set_trial_spawner_config(p, "trial_chamber/breeze");
    let players = [player(0, DVec3::new(8.5, 64.0, 13.5))];
    for _ in 0..60 {
        w.maps.game_ticks += 1;
        e.tick_trial(&mut w, &players, TrialRules { peaceful: true, ..RULES });
    }
    assert_eq!(state(&w, p).0, SpawnerState::WaitingForPlayers);
    assert!(e.mobs.is_empty());
    // An unconfigured spawner (placed in Creative) stays inactive.
    let q = IVec3::new(20, 64, 20);
    w.set_block(q, tr::TRIAL_SPAWNER);
    for _ in 0..40 {
        step(&mut w, &mut e, &players);
    }
    assert_eq!(state(&w, q).0, SpawnerState::Inactive);
}

#[test]
fn vaults_reward_each_player_once_per_key() {
    let mut w = world();
    let mut e = Entities::new(8);
    let p = IVec3::new(8, 64, 8);
    w.set_block(p, tr::vault_block(Facing::North, VaultState::Inactive, false));
    let players = [player(0, DVec3::new(8.5, 64.0, 10.5)), player(3, DVec3::new(9.5, 64.0, 9.5))];
    for _ in 0..40 {
        step(&mut w, &mut e, &players);
    }
    assert_eq!(vault(w.get_block(p).unwrap()).unwrap().1, VaultState::Active);
    // The wrong key and an empty hand are refused or passed.
    assert_eq!(w.use_vault(p, 0, None), VaultUse::Rejected { rewarded: false, sound: true });
    assert!(matches!(
        w.use_vault(p, 0, Some(Stack::new(OMINOUS_TRIAL_KEY, 1))),
        VaultUse::Rejected { rewarded: false, .. }
    ));
    assert_eq!(w.use_vault(p, 0, Some(Stack::new(TRIAL_KEY, 1))), VaultUse::Unlocked { consumed: 1 });
    let queued = w.trial_vault(p).unwrap().items_to_eject.len();
    assert!(queued >= 2, "reward rolls at least two stacks");
    let items = e.items.len();
    for _ in 0..(20 * (queued as u32 + 4)) {
        step(&mut w, &mut e, &players);
    }
    assert_eq!(e.items.len() - items, queued);
    assert_eq!(vault(w.get_block(p).unwrap()).unwrap().1, VaultState::Active, "player 3 still waits");
    // Player 0 was rewarded; player 3 still can be.
    assert!(matches!(w.use_vault(p, 0, Some(Stack::new(TRIAL_KEY, 1))), VaultUse::Rejected { rewarded: true, .. }));
    assert_eq!(w.use_vault(p, 3, Some(Stack::new(TRIAL_KEY, 1))), VaultUse::Unlocked { consumed: 1 });
    // The rewarded set survives a save.
    let saved = w.trial_to_string();
    let mut other = world();
    other.load_trial(&saved);
    assert_eq!(other.trial_vault(p).unwrap().rewarded, vec![0, 3]);
}

#[test]
fn ominous_vaults_take_ominous_keys() {
    let mut w = world();
    let mut e = Entities::new(9);
    let p = IVec3::new(8, 64, 8);
    w.set_block(p, tr::vault_block(Facing::South, VaultState::Inactive, true));
    let players = [player(0, DVec3::new(8.5, 64.0, 10.5))];
    for _ in 0..40 {
        step(&mut w, &mut e, &players);
    }
    assert!(matches!(w.use_vault(p, 0, Some(Stack::new(TRIAL_KEY, 1))), VaultUse::Rejected { .. }));
    assert_eq!(w.use_vault(p, 0, Some(Stack::new(OMINOUS_TRIAL_KEY, 1))), VaultUse::Unlocked { consumed: 1 });
}

#[test]
fn ominous_item_spawners_save_and_drop_their_item() {
    let mut e = Entities::new(10);
    e.item_spawners.push(ItemSpawner {
        pos: DVec3::new(8.5, 70.5, 8.5),
        item: Stack::new(crate::item::Item::BREAD, 2),
        age: 0,
        lifetime: 60,
    });
    let saved = e.item_spawners_to_value();
    let mut other = Entities::new(11);
    other.load_item_spawners(&saved);
    assert_eq!(other.item_spawners, e.item_spawners);
    let mut w = world();
    for _ in 0..60 {
        e.tick_item_spawners(&w);
    }
    assert!(e.item_spawners.is_empty());
    assert_eq!(e.items.iter().map(|i| i.stack.count as u32).sum::<u32>(), 2);
    let _ = &mut w;
}

#[test]
fn as_long_matches_java_block_pos_packing() {
    assert_eq!(as_long(IVec3::new(0, 0, 0)), 0);
    assert_eq!(as_long(IVec3::new(1, 2, 3)), (1 << 38) | (3 << 12) | 2);
    // Negative coordinates wrap into their 26/12-bit fields.
    assert_eq!(as_long(IVec3::new(-1, -1, -1)) & 0xFFF, 0xFFF);
    assert!(as_long(IVec3::new(-1, 0, 0)) < 0);
    let _ = Block::AIR;
}
