# Mob swimming, flying and climbing navigation — 2026-10-14

[Roadmap](../roadmap.md) · [Ground navigation](2026-10-14-mob-navigation.md)

Scope: MOB-02 beyond ground paths, on `t3/v0-8-release` after `4fea268`.
Reference: Java Edition 26.3 server (SHA-1
`33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`) `SwimNodeEvaluator`,
`AmphibiousNodeEvaluator`, `FlyNodeEvaluator`, `WaterBoundPathNavigation`,
`AmphibiousPathNavigation`, `FlyingPathNavigation`, `WallClimberNavigation`,
`PathNavigation.shouldRecomputePath/recomputePath` and
`ServerLevel.sendBlockUpdated`, decompiled locally under ignored `target/` for
reading only. Tests ran on the M5 MacBook Pro.

## Delivered behavior

[`entity/path.rs`](../../src/entity/path.rs) gains an `Evaluator` chosen per
mob as Java's `createNavigation` does (transient, never saved):

- **Swim** (cod, salmon, tropical fish, pufferfish, guardians; dolphins with
  surface breaching): the whole mob volume must be water-pathfindable, six
  straight moves plus horizontal diagonals whose two components are accepted,
  `BREACH` only for dolphins, and the extra cost for non-fluid cells. Fish
  wander, school and chase along swim paths (squid keep Java's random
  movement, which has no navigation).
- **Amphibious** (drowned, axolotls, turtles): `WATER` costs 0, `WATER_BORDER` 4
  and walkable land 6; vertical moves through water; start cells from the
  bounding box when in water; floor level half a block into water.
- **Fly** (parrots): 26-direction neighbours in Java's insertion order, with a
  diagonal accepted only when every component move is, so paths never cut a
  blocked edge or corner; `FlyNodeEvaluator` cell types over fences, fire,
  lava and damaging blocks. Following and strolling parrots use it.
- **Wall climbers** (spiders, cave spiders): the navigation keeps the
  unreachable destination after the path ends (`pathToPosition`) and keeps
  pushing at it until above it, so the collision climbs the wall.
- **Recompute on block changes**: the world records collision-relevant block
  edits in a bounded 256-entry history. Each navigator reads it from its own
  cursor and applies Java's midpoint-sphere `shouldRecomputePath`; a recompute
  coalesces edits and searches at most once per 21 ticks. History overflow
  conservatively recomputes; fluid, fire and other non-colliding edits are not
  recorded, as in `sendBlockUpdated`.

## Evidence

- Unit tests in `entity/path.rs`: fish route around an underwater wall
  without leaving water; only dolphins accept breaches and pay the air cost;
  swimmers check their whole volume; amphibians swim vertically and keep the
  water-border/land costs; parrots fly across air and can't cut blocked
  corners; recomputation uses the midpoint radius and coalesces for 20 ticks;
  a wall climber keeps an unreachable destination until it is above it. The
  history keeps independent cursors and reports overflow.
- `tests/mob_navigation_swim_fly.rs` through the real entity tick: drowned swim
  up to a player instead of walking the bottom; a following parrot flies
  round a wall and across a gap; a spider climbs a three-high wall to reach a
  player on top (this also failed before the change: a climbing spider lost
  its wanted position as soon as it left the ground); a wall built across an
  active path is recomputed before the stuck timeout.
- Full `cargo test --release` passed.
- `tests/mob_navigation.rs` `zombies_chase_across_generated_terrain` (ignored
  benchmark, seed 7): 13/13 zombies reach the player; entity tick mean
  0.024 ms, p99 0.113 ms (before: mean 0.025 ms, p99 0.232 ms).

## Remaining differences and open work

- Striders kept their own lava steering here; `StriderPathNavigation` and
  `StriderGoToLavaGoal` followed on 2026-10-15
  ([evidence](2026-10-15-strider-lava-paths.md)).
- Ghasts, blazes, bees, allays and vexes keep their existing movement; squid
  and glow squid have no navigation in Java and are unchanged.
- Fish, dolphins and other swimmers still use VoxelCraft's swim physics, not
  Java's `MoveControl`/`SmoothSwimmingMoveControl`.
- Follow-owner/tempt/breed repath cadences and `AvoidEntityGoal` are unchanged.
- Zombies don't convert to drowned; entity collision boxes still aren't part
  of path checks.
- Ordinary-player acceptance and an M5 scene with many swimming and flying
  mobs remain. MOB-02 stays **Partial**.
