# Bed stand-up positions and scheduled fire ticks — 2026-10-17

[Roadmap](../roadmap.md) · [Death, pause and save scenarios](2026-10-15-death-pause-save-scenarios.md) · [Simulation activity](2026-10-14-simulation-activity-save-integrity.md)

Scope: PLY-03c (bed respawn and wake-up position) and PLY-01b (scheduled fire
ticks), on `agent/small-fixes` after `77a6885`, plus the integrator's
respawn-angle change (`PersonalSpawn::yaw`, `DEFAULT_SPAWN_YAW`,
`PlayerSession::respawn(at, yaw, keep_inventory)`). Reference: Java Edition 26.3
server (`server-26.3-classes.jar`) `AbstractBedBlock`, `DismountHelper`,
`Direction.isFacingAngle`, `LivingEntity.stopSleeping`,
`ServerPlayer.findRespawnAndUseSpawnBlock`, `LevelTicks`, `LevelChunkTicks` and
`ServerLevel.tickBlocks`, read with `javap -c -p`. Headless tests only; no
graphical run or benchmark.

## Delivered behavior

### Bed stand-up position (`src/world/dismount.rs`)

- **`DismountHelper.findSafeDismountLocation` for the player.** A cell is
  usable when the block in it is not dangerous (first pass only), there is a
  floor (`getBlockFloorHeight`: the cell's own non-climbable collision top, else
  the cell below's top minus one, and below a full block), a dangerous floor is
  refused when the player stands on it (first pass only), the 0.6 × 1.8 body
  fits without overlapping a collision shape at the resulting height, and
  neither the cell nor the one above is an end portal or gateway. Open
  trapdoors and climbable blocks count as empty shapes. Dangerous blocks are
  fire, soul fire, lava, magma, cacti and sweet berry bushes. Collision comes
  from VoxelCraft's `physics` shapes.
- **`AbstractBedBlock.findStandUpPosition`.** The side is the bed's clockwise
  direction, or its opposite when the player's yaw faces the clockwise side
  (`Direction.isFacingAngle`, with Java's `Mth.sin`/`Mth.cos` tables). The ten
  `bedSurroundStandUpOffsets` and two `bedAboveStandUpOffsets` (the head block,
  then the foot block) are read from the class and tried in order, first with
  dangerous blocks refused, then allowed. A bunk bed (a bed block below the
  head) tries the head level's surround, the lower bed's surround, then the
  above offsets, safe pass first and the dangerous pass second, as
  `findBunkBedStandUpPosition` does.
- **Respawn at a bed.** `PlayerSession::personal_spawn` finds the stand-up
  position beside the bed's head, assuming a stored player angle of 0 (the
  value Java's data fixer gives spawns saved without an angle, which is all
  VoxelCraft saves hold), and faces the head block's bottom centre
  (`RespawnPosAngle.of`). `spawn_bed` is still saved as the bed foot. A bed that
  is gone, or boxed in so that no cell fits, is the missing-bed outcome: the
  spawn is forgotten, "Your home bed was missing" shows and the world spawn is
  used. Desktop, controller, LAN and CLI respawns all go through it.
- **Waking up.** `PlayerSession::begin_sleep` records the bed's head and the
  lying spot, and `wake` / `settle_wake` apply `LivingEntity.stopSleeping`: the
  player stands at the stand-up position the player's own yaw picks, looking at
  the head with level pitch, else just above the head (`+0.1`). Waking by
  morning, by `wake`, or by any other path that only clears `sleeping` (damage,
  a command) settles on the next tick, and only for a player still lying where
  they lay down, so teleports and dimension changes are left alone.

### Scheduled fire ticks (`src/world/fire.rs`)

- Fire ticks follow `LevelTicks`: one container per 16 × 16 chunk column. A
  container whose chunk is outside the block-ticking region costs one peek per
  game tick and waits without using any of the budget; its ticks stay due
  and run when the player comes back.
- The ticks that are due and in range are collected as `drainContainers` does:
  containers take turns by the sub-tick order of their next tick
  (`CONTAINER_DRAIN_ORDER`), each container yielding by trigger tick then
  sub-tick order, and at most 65,536 are collected per game tick
  (`ServerLevel.tickBlocks`). The rest stay scheduled for the next tick. This
  replaces the old 4096-per-call cap that sleeping fires could use up.
- `tick_fire` now runs one game tick at a time, so a long frame runs each
  tick's collection separately instead of one batch.
- Fire ticks are still not saved: loading a chunk reschedules every fire in it
  (30..39 ticks), as before, so saves and chunk data are unchanged.

## Evidence

- `world::dismount` unit tests: the offset tables for two orientations against
  the bytecode; the yaw/side choice including the quarter-turn table edge;
  blocked sides falling through in Java's order, all ten surround offsets then
  the head; a boxed-in bed; the dangerous-floor second pass; a bunk bed;
  fence-post floors, climbable cells, no floor, low ceilings and portals.
- `tests/death_and_respawn.rs` (13 tests, 7 new), through `PlayerSession` and
  the CLI agent's respawn:
  - `respawns_stand_beside_the_bed_head_and_look_at_it_with_level_pitch`
  - `obstructed_sides_choose_the_next_offset_in_java_order`
  - `a_dangerous_floor_is_only_used_when_nothing_safer_fits`
  - `a_boxed_in_bed_is_a_missing_bed`, including the forgotten bed and the
    world-spawn fallback through the CLI
  - `a_bunk_bed_respawns_beside_either_bed_before_standing_on_the_top`
  - `an_adjacent_bed_does_not_change_the_stored_beds_facing`
  - `waking_stands_the_player_up_beside_the_bed`: yaw picks the side, the
    player looks at the head, a late wake-up settles, a drifted or
    teleported player is left alone, a gone bed leaves the sleeper put, and a
    boxed-in bed puts them above the head
  - `ServerPlayer.die` no longer clears effects (Java leaves them until the
    respawn replaces the body); the existing death test now asserts that.
- `world::fire` unit tests:
  - `thousands_of_sleeping_fires_never_delay_an_active_one`: over 12,000
    sleeping fires 7 chunks from the player plus one active fire scheduled
    last; after 40 ticks the active fire has ticked on time, every sleeper still
    has its original deadline and has not aged, and returning to them runs the
    whole backlog in one game tick.
  - `at_most_65536_due_ticks_are_collected_per_game_tick_in_container_order`:
    70,000 due fires; exactly 65,536 run in (trigger, sub-tick) order and the
    last 4,464 wait for the next game tick.
  - `containers_take_turns_by_the_sub_tick_order_of_their_next_tick`.
  - `scheduled_fire_pauses_at_distance_and_resumes_after_chunk_reload` (updated)
    covers distance pausing, the menu pause and the reschedule on chunk load.
- Chain, run from the worktree: see [Checks](#checks).

## Checks

See the integrated gate in [game rules and difficulty](2026-10-17-game-rules-difficulty.md#evidence); this record was merged with it.

## Remaining differences

- The player's stored spawn angle is not saved: a bed respawn behaves as if it
  were 0 (south), where Java uses the angle stored when the bed was set (which
  can pick the other side of the bed). Set spawn points and the world spawn
  face south as before.
- Unloaded cells count as solid, and the world border is not part of the
  stand-up checks.
- `EntityType.isBlockDangerous` also covers lit campfires, wither roses,
  powder snow and lava cauldrons; those are not blocks in VoxelCraft yet.
- Fire ticks are not written to chunk data, so a fire reloaded from disk gets a
  fresh 30..39-tick delay instead of its saved trigger tick.
- Replacing a scheduled fire leaves its old entry in the container until it
  comes due (it counts toward the per-tick limit, as a Java tick that finds a
  different block would); Java de-duplicates by block and position when
  scheduling.
- Only fire uses the scheduler. Other Java scheduled block ticks (lava, water,
  redstone, plants) keep their existing paths.
- A shared cargo target directory makes every worktree's `voxelcraft`
  artifacts share a filename, so another agent's newer build can be run in
  place of the one just made; touch the sources before building and check the
  test count.
