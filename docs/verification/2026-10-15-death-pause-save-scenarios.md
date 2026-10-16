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

- *Dying* stops eating, mining and sleeping and ends status effects. Unless
  the player is a spectator or `keepInventory` is on, the inventory,
  workstation inputs and the desktop crafting grid drop where the player
  died. Curse of Vanishing items are destroyed. Experience drops as
  `min(7 × level, 100)` points. With `keepInventory` the grid goes back into
  the inventory.
- *Respawning* creates a fresh body: full health, food and air; no effects,
  fire or fall distance; no velocity, vehicle or sneaking; and the game
  mode's flight abilities. The spawn bed and point are kept. Experience
  carries over as death left it: kept with the inventory, otherwise zero,
  keeping the enchantment seed as `restoreFrom` does.

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

- **Look direction.** Respawns keep the player's look direction; Java faces
  the spawn angle.
- **Effects.** They end at death rather than on respawn, which shows only on
  the death screen.
- **Desktop adapter.** It is still exercised through the shared session
  steps, not a headless desktop-adapter harness (PLY-04).
- **Acceptance.** Ordinary-player acceptance of death, respawn and
  pause on a physical controller remains. PLY-01/03/04 stay **Partial**.
