# Strider lava paths — 2026-10-15

[Roadmap](../roadmap.md) · [Swim and fly navigation](2026-10-14-mob-navigation-swim-fly.md) · [Ground navigation](2026-10-14-mob-navigation.md)

Scope: MOB-02, the strider part left open by the swim/fly work, on
`t3/roadmap-next-stage-v0-8` after `c152eb0`. Reference: Java Edition 26.3
server (SHA-1 `33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`) `Strider`,
`Strider$StriderPathNavigation`, `Strider$StriderGoToLavaGoal`,
`MoveToBlockGoal`, `RandomStrollGoal`, `RandomPos`, `DefaultRandomPos`,
`PathNavigation`, `PathFinder`, `WalkNodeEvaluator` and `LiquidBlock`, read
with `javap` from `~/Code/voxelcraft-java-reference/26.3/`. Tests ran on the
M5 MacBook Pro.

## Delivered behavior

- **Ground navigation for striders.** Striders now walk Java paths instead of
  steering in straight lines. `StriderPathNavigation` is a
  `GroundPathNavigation` whose `isStableDestination` also accepts lava. The
  strider's costs were already ported: water impassable, and lava, fire and
  fire-adjacent cells free. `WalkNodeEvaluator.getStart` starts a strider
  standing on lava from the top lava cell (`canStandOnFluid`).
- **Java's lava standing height.** A lava source with no lava above offers
  mobs that can stand on it `LiquidBlock.STABLE_SHAPE`, the bottom half of the
  cell. Striders now stand half a block up the top lava cell, legs in the
  lava, instead of on the cell's top face. They fall onto that height through
  ordinary collision, so a shore block under part of the box holds them up.
  They step onto a shore half a block higher. Sunk deeper, or in flowing lava,
  they float up as before (`floatStrider`).
- **`StriderGoToLavaGoal`.** This is a port of `MoveToBlockGoal` (range 8,
  vertical 2) and replaces the old one-second nearest-lava scan:
  - Only a strider out of lava searches. It looks in Java's ring order: one
    below the feet first, then level, two below, one above, three below,
    alternating sides in each ring. A target is any lava block under a
    LAND-pathfindable block that isn't lava.
  - It walks to the cell above the lava (accuracy 1), then replans to the lava
    cell every 20 ticks.
  - It counts as arrived within one block of the lava cell's centre. It stops
    only once the strider is in lava or the lava is gone: Strider's
    `canContinueToUse` override skips `MoveToBlockGoal`'s give-up and stay
    timers.
  - It searches again `reducedTickDelay(200 + nextInt(200))` goal updates
    after each search. Panic preempts it.
- **`RandomStrollGoal(1.0, 60)`.** Striders stroll twice as often as the
  water-avoiding animals, using `DefaultRandomPos(10, 7)`. A fix shared with
  the other kinds: `RandomPos.generateRandomPos` starts from negative infinity
  with a strict comparison. A strider in lava, which scores every non-lava
  spot `-Infinity`, therefore never strolls ashore. Previously the first
  candidate won regardless of its score.

## Evidence

- `entity::path` unit test
  `striders_path_across_lava_and_into_it_but_not_onto_a_full_block_shore`:
  - A strider standing at Java's stable height routes round a stone island
    on lava nodes only. Mobs that can't stand on lava cannot cross.
  - From the shore it paths down into the lava.
  - It cannot path out onto ground one block above the lava's surface: Java
    reads the floor under a lava node as the lava below it, since there is no
    collision without the mob's context, so that is a two-block jump. A shore
    level with the top lava cell is reachable.
- `entity::mob::nether_ai` unit test `strider_lava_search_follows_javas_ring_order`.
- `entity::nether::tests::the_lava_goal_has_no_give_up_timer`: a strider aiming
  at walled-off lava still holds the goal after 1300 ticks (review fix).
- `entity::nether` tests through the real entity tick:
  - A cold strider walks a path round a wall to the lake and then stays on it
    for 30 s.
  - Striders on a lake stand at exactly 9.5 (top lava cell 9) and never leave it.
  - Existing shivering, spawning, drop and riding tests still pass.
- Representative Nether scene, `tests/mob_navigation_swim_fly.rs`
  `striders_walk_lava_seas_in_generated_nether` (ignored, `--nocapture`).
  Forty striders, 20 on generated lava seas and 20 ashore beside lava, run for
  30 s at rd 2:

  | Seed | In lava after 30 s, this change | Old straight steering | Entity tick mean / p99, this change | Old |
  |---|---|---|---|---|
  | 7 | 38/40 | 40/40 | 0.032 / 0.076 ms | 0.025 / 0.038 ms |
  | 11 | 35/40 | 28/40 | 0.031 / 0.095 ms | 0.025 / 0.033 ms |
  | 23 | 38/43 | 40/44 | 0.031 / 0.077 ms | 0.025 / 0.038 ms |

  The old column comes from the same test on `c152eb0`, with the striders
  started a block up as the old physics expected. Path searches add about
  0.006 ms mean and up to 0.06 ms p99 for forty striders.
- Full `cargo test --release` passed.

## Remaining differences and open work

- **Shore stops.** Some striders stop on the shore inside the one-block
  arrival radius but outside the lava; most of the shortfall in the table
  is these. Java's `MoveToBlockGoal` counts that as arrived, and the goal
  holds them there, shivering, until panic, temptation or breeding moves them.
  Java does the same, but VoxelCraft's movement isn't Java's
  `MoveControl`/`travel`, so where a strider comes to rest differs slightly.
- **Goal priorities.** VoxelCraft runs tempt, breed and follow-parent before
  the lava goal; in Java, follow-parent (priority 5) yields to it (4).
- **Collision context.** Collision with the stable shape is VoxelCraft's own
  check, not Java's entity `CollisionContext`. Riders and mounted striders
  use the same standing rule.
- **Acceptance.** Ordinary-player acceptance in the Nether (riding, luring,
  breeding) remains, so MOB-02 stays **Partial**.
