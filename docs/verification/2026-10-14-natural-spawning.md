# Natural spawning and despawning — 2026-10-14

[Roadmap](../roadmap.md) · [Lighting and spawning (2026-10-13)](2026-10-13-lighting-spawning.md) · [Reference fixtures](../../tests/fixtures/java/26.3/README.md)

Scope: MOB-01 on `t3/v0-8-roadmap-build`. Reference: official Java Edition
26.3 server SHA-1 `33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`: `MobCategory`,
`EntityTypes`, `NaturalSpawner`, `LocalMobCapCalculator`, `Mob.checkDespawn`
and `removeWhenFarAway` overrides, `server.level.ChunkMap` and
`DistanceManager`, decompiled locally under ignored `target/` for reading only.

## Delivered behavior

[`entity/category.rs`](../../src/entity/category.rs) adds Java's mob
categories (each kind's registration category, caps, despawn and no-despawn
distances) and `SpawnState`:

- Counted spawn chunks are the 16-block chunks within chessboard distance 8 of
  any player's chunk (`DistanceManager.naturalSpawnChunkCounter`): 289 for one
  player. A category's global cap is `max * counted chunks / 289`.
- Spawn attempts run only in loaded chunks whose centre is within 128 blocks
  horizontally of a player (`playerIsCloseEnoughForSpawning`).
- Each player has a local cap (`LocalMobCapCalculator`): a mob counts for every
  player near its chunk, and a chunk may spawn while any nearby player is under
  the category's maximum. Mobs that require persistence don't count.

Overworld monsters now use these: a shuffled chunk order each tick, the global
cap checked once a tick (as `getFilteredSpawningCategories` does), the local cap
per chunk, starts out to 128 blocks and no monster placed beyond its despawn
distance. Per-kind monster caps and the 24–64 ring are gone for monsters.

Every despawnable mob follows `Mob.checkDespawn`: removal at once beyond its
category's despawn distance (128 blocks; 64 for fish), and a 1-in-800 chance a
tick beyond 32 blocks once its `noActionTime` passes 600 ticks. `noActionTime`
resets when hurt or within 32 blocks of a player; mobs idle for 100 ticks stop
strolling (`RandomStrollGoal.checkNoActionTime`).

## Evidence

- `category` unit tests: one player has 289 counted chunks and the expected
  128-block disc, the monster cap is exactly 70; caps ignore persistent mobs,
  two distant players double the global cap and each fills their own local cap;
  category assignments match `EntityTypes` for representative types.
- `tests/natural_spawning.rs`: on a dark plain, monsters fill to 60–73 (a pack
  started under the cap may finish over it, as in Java), every new monster
  appears more than 24 and at most 128 blocks away, and some beyond the old
  64-block ring; a zombie 140 blocks away goes at once, idle skeletons 60 blocks
  away mostly despawn within 150 s while penned creepers 20 blocks away all
  remain; persistent mobs never despawn.
- `distant_players_get_their_own_mob_caps` and `spawn_rules` now check the
  category caps; the former's new-mob detection was corrected (ids are assigned
  during the update, so it had stopped checking spawn distances).

## Remaining differences and open work

- Creature, ambient and water categories still use the per-kind caps and the
  24–64 ring; animals still spawn continuously rather than with chunk
  generation and every 400 ticks. The Nether and End keep their existing spawn
  passes.
- The spawn pass runs every quarter second over five ticks' starts with one
  `SpawnState`, rather than recounting every tick.
- Spectators are not distinguished from other players.
- Structure spawn overrides and the missing monsters (strays, bogged, parched,
  zombie horses, phantoms) remain open; ordinary-player and representative
  performance acceptance remain. MOB-01 stays **Partial**.
