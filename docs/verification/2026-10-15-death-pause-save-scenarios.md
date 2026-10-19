# Death, pause and mid-motion save scenarios — 2026-10-15

[Roadmap](../roadmap.md) · [Loaded-region activity and save integrity](2026-10-14-simulation-activity-save-integrity.md) · [Player session](2026-10-13-player-session.md)

Scope: the PLY-01/PLY-03/PLY-04 regression scenarios left open by the
simulation-distance work, on `t3/roadmap-next-stage-v0-8` after `c152eb0`:
death and respawn on the desktop, controller, LAN and CLI paths, pause, and
saves taken in the middle of piston and repeater motion. Reference: Java
Edition 26.3 server (SHA-1 `33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`)
`ServerPlayer.die`, `ServerPlayer.restoreFrom`, `Player.dropEquipment` and
`Player.getBaseExperienceReward`, read with `javap`. Tests ran on the M5
MacBook Pro.

## Delivered behavior

**One death step and one respawn step for every adapter.**
`PlayerSession::die` and `PlayerSession::respawn` replace four separate
implementations: the desktop's `on_death`/`respawn`, the CLI agent's drop and
`respawn` command, and the controller and LAN respawn patches.

- *Dying* stops eating, mining and sleeping. Status effects last until the
  respawn ([follow-up](#follow-up-respawn-angle-and-effects--2026-10-17)). Unless
  the player is a spectator or `keepInventory` is on, the inventory,
  workstation inputs and the desktop crafting grid drop where the player
  died. Curse of Vanishing items are destroyed. Experience drops as
  `min(7 × level, 100)` points. With `keepInventory` the grid goes back into
  the inventory.
- *Respawning* creates a fresh body: full health, food and air; no effects,
  fire or fall distance; no velocity, vehicle or sneaking; and the game
  mode's flight abilities. The spawn bed and point are kept. Experience and
  the death-screen score carry over only with `keepInventory` or for a
  spectator (`transferInventoryXpAndScore`); otherwise only the enchantment
  seed does, as `restoreFrom` copies it.

Defects this fixed:

- The desktop dropped nothing for Creative or Adventure deaths (`/kill`, the
  void). Java drops in every mode but Spectator.
- The CLI `respawn` command reset experience even with `keepInventory`. The
  controller, LAN and immediate-respawn paths each patched it back
  separately.
- The CLI agent dropped a spectator's inventory.
- Every respawn reset the enchantment seed.
- The desktop death step didn't leave the player's vehicle. It now does, as
  the agent path already did.

**Pause.** The desktop rule is now a tested function: an open game menu, the
console or the credits stop ticks only in an unshared world. The CLI agent
server or a LAN host or client keeps the world running, and controller
menus never pause it. Resuming doesn't charge the paused time.

## Evidence

- `tests/death_and_respawn.rs`, through the CLI agent that controller and LAN
  players also run on:
  - Deaths in Survival, Adventure and Creative drop everything but vanishing
    items, including work inputs, plus 70 experience for level 10; a
    spectator drops nothing.
  - With `keepInventory`, items and experience survive a CLI respawn.
  - The death-screen score resets on a normal respawn and survives a
    `keepInventory` one (review fix).
  - A respawn is a fresh body at the spawn point that keeps the enchantment
    seed and spawn point.
  - The crafting grid drops, or returns under `keepInventory`.
  - A player saved dead loads dead. The desktop's second death step on load
    duplicates nothing, and the player can then respawn.
- `tests/redstone_motion_persistence.rs` runs two processes against the real
  disk save. The first saves a sticky piston mid-push, a sticky piston
  mid-pull and a 4-tick repeater with its pulse in flight, then keeps running
  for 12 ticks. The second loads the save and runs the same 12 ticks. Every
  tick's blocks match the first process. The pushed and pulled blocks each
  exist exactly once, and no moving-block placeholder remains.
- `app::pause_tests::menus_pause_only_an_unshared_world_and_resume_without_catching_up`.
- Existing coverage still passes:
  - in-process piston and repeater save round trips;
  - `seeded_world_outcomes_match_frame_pacing_and_pause_produces_no_ticks`;
  - the agent's bed-respawn, hardcore and death-drop tests.
- Full `cargo test --release` passed in the desktop and headless
  configurations.

## Remaining differences and open work

- **Look direction** *(fixed 2026-10-17, below)*. Respawns kept the
  player's look direction; Java faces the spawn angle.
- **Effects** *(fixed 2026-10-17, below)*. They ended at death rather than on
  respawn, which showed only on the death screen.
- **Desktop adapter.** It is still exercised through the shared session
  steps, not a headless desktop-adapter harness (PLY-04).
- **Acceptance.** Ordinary-player acceptance of death, respawn and
  pause on a physical controller remains. PLY-01/03/04 stay **Partial**.

## Follow-up: respawn angle and effects — 2026-10-17

Reference: `ServerPlayer.findRespawnPositionAndUseSpawnBlock`,
`findRespawnAndUseSpawnBlock` and `ServerPlayer$RespawnPosAngle.of` in the
26.3 server jar, read with `javap`. A bed or anchor respawn builds
`RespawnPosAngle.of(standUp, bedPos)`: yaw
`wrapDegrees(atan2(dz, dx) × 57.2957763671875 − 90)` of the vector from the
stand-up position to the bed block's bottom centre, pitch 0. A forced spawn
point uses the stored respawn angle, and the world spawn
(`TeleportTransition.createDefault`) the level's, which default to 0.

- `PlayerSession::respawn` takes the yaw and sets level pitch.
  `PersonalSpawn::yaw` looks at the bed's head from the respawn spot;
  set spawn points and the world spawn use `DEFAULT_SPAWN_YAW`, Java's
  angle 0 (south, +Z). The desktop, End/Nether respawn arrival, controller
  seats, LAN peers and the CLI all apply it.
- `PlayerSession::die` no longer clears effects; the respawn's fresh vitals
  drop them, as `restoreFrom` copies none. Like `LivingEntity.baseTick`,
  effects keep counting down on the death screen (desktop and agents) without
  healing or harming the body.
- LAN clients own their look. Every respawn (the respawn command or
  `doImmediateRespawn`) bumps `PlayerSession::respawns`, which snapshots
  carry; the client adopts the snapshot's angle when the count changes, and
  the host holds that angle against the peer's stale look input until the
  client reports it (or two seconds pass).
- Tests: `respawns_face_the_bed_or_the_default_spawn_angle_with_level_pitch`
  (four bed facings, a spawn point),
  `effects_keep_counting_down_on_the_death_screen_without_acting` and
  `a_respawn_angle_holds_against_stale_peer_look_until_the_client_adopts_it`;
  the death test now expects effects to last until the respawn. Full desktop and headless suites pass.
- Difference, superseded by the
  [bed stand-up record](2026-10-17-bed-standup-fire-ticks.md): VoxelCraft
  stood the player on the bed's foot rather than searching
  `findStandUpPosition`, so the angle was measured from there. Respawns now
  use `findStandUpPosition`. Spawn points set with the console store no angle (Java's
  `/spawnpoint` can).

