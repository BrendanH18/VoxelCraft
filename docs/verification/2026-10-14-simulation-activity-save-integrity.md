# Loaded-region activity and save integrity — 2026-10-14

[Roadmap](../roadmap.md) · [Restructuring](../restructuring.md) · [Mob navigation](2026-10-14-mob-navigation.md)

Scope: PLY-01 (loaded-region activity, frame-rate independence, save
integrity), on `t3/v0-8-release` after `4fea268`. Reference: Java Edition 26.3
server (SHA-1 `33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`) `DistanceManager`,
`ChunkLevel`, `ServerChunkCache.tickChunks`, `LevelTicks` and
`LevelChunkTicks`, decompiled locally under ignored `target/` for reading only.
Tests ran on the M5 MacBook Pro.

## Delivered behavior

[`world/activity.rs`](../../src/world/activity.rs) adds Java's player
simulation tickets in 16-block Java chunks, independent of 32-block storage
chunks and of the render distance:

- Ticket level 31 (entity ticking) covers Java chunks within the simulation
  distance (Chebyshev) of any player; level 32 (block ticking) one chunk
  farther. The union of the desktop player and every active agent/controller
  player counts once, so overlapping players never accelerate work.
- Simulation distance is a saved option (`simulation_distance`, 5–32, default
  10) with an Options slider. Chunks out to the full ticket border stay loaded
  even when the render distance is smaller.
- Block-ticking gates: random ticks (now per 16³ section, Java's 3 per section
  per tick, so a storage chunk can be half active), fire, leaf decay, fluids,
  furnaces, brewing stands, hoppers, piston motion, redstone scheduled ticks and
  neighbour updates. Entity-ticking gates: mobs (AI, despawn checks, effects,
  ageing, love timers), item entities and merging, XP orbs, minecarts, TNT,
  mounts, golems and villager schedules.
- Work outside the simulation region sleeps rather than being dropped or
  retried each tick. Redstone ticks in loaded but non-ticking chunks keep their
  absolute due time (overdue work runs when the region wakes, as in
  `LevelTicks`); ticks in unloaded chunks keep their *remaining delay* and resume
  from it on load, which is how Java saves `LevelChunkTicks`. The v1 redstone
  save encoding is unchanged; delays are now relative to the unload time.
- Falling blocks in flight are saved with the dimension (`falling` property)
  instead of being lost when the game saves mid-fall.
- Mobs whose chunks unload are frozen in place instead of discarded.

## Evidence

- `tests/simulation_activity.rs`:
  - `java_ticket_tiers_split_storage_chunks_and_use_floor_for_negative_positions`:
    entity/block tiers at distance 5 and 10, including a split storage chunk and
    negative coordinates (blocks 159/160 at distance 10).
  - `rendering_only_chunks_freeze_containers_mobs_items_fluids_fire_and_falling_blocks`:
    in one storage chunk, a furnace, brewing stand, hopper, fire, falling sand,
    items, orbs, TNT and five mobs (effects, baby age, love) advance in the
    block-ticking ring and stay bit-identical just past it; a returning player
    wakes them, and two overlapping players don't double the rate.
  - `random_ticks_respect_the_java_section_boundary_and_overlapping_players`.
  - `seeded_world_outcomes_match_frame_pacing_and_pause_produces_no_ticks`:
    the fixed 20 TPS clock gives identical seeded results for different frame
    pacings; a paused clock advances no ticks.
- Unit tests: fire sleeps outside the region and reschedules after chunk
  reload; random ticks include distant agents once; redstone ticks in an
  unloaded chunk keep their 20-tick button delay through save/load.
- Full `cargo test --release` passed.
- Tick cost (`representative_tick_cost`, ignored manual benchmark): 81 loaded
  platform chunks, 81 burning furnaces and 160 cows — mean 0.147 ms, p99
  0.174 ms, max 0.335 ms per tick. No pre-change figure exists for this fixture.

## Remaining differences and open work

- Java's client default simulation distance could not be checked against the
  server artifact (the dedicated server default is 10); VoxelCraft uses 10.
- Sleeping fire updates re-enter the queue each tick; more than 4096 overdue
  sleeping fires could delay active ones (Java keeps per-chunk tick containers).
- Mobs in unloaded chunks stay in memory (frozen) rather than being written to
  per-chunk entity storage.
- Pause/death/respawn scenarios through the desktop session, controller puppet
  and CLI agent, and piston/repeater save-mid-motion duplication tests, remain.
- Ordinary-player acceptance and an M5 frame-time scene remain. PLY-01 stays
  **Partial**.
