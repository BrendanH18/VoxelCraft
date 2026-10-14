# Mob ground navigation — 2026-10-14

[Roadmap](../roadmap.md) · [Restructuring](../restructuring.md) · [Reference fixtures](../../tests/fixtures/java/26.3/README.md)

Scope: MOB-02 (navigation) with MOB-01/ENV-01 water interactions, on
`t3/v0-8-roadmap-build` after `da42e50` (v0.7.0). Reference: official Java
Edition 26.3 server SHA-1 `33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`, classes
decompiled locally under ignored `target/` for reading only. Tests ran on the
M5 MacBook Pro.

## Delivered behavior

[`entity/path.rs`](../../src/entity/path.rs) ports Java's ground pathfinding:

- `PathType` (all 27, in ordinal order, with default costs) and
  `WalkNodeEvaluator.getPathTypeFromState` as a once-built per-state table.
  `isPathfindable(LAND)` follows the 26.3 overrides (slabs, stairs, beds,
  chests, farmland, dirt paths, mud, soul sand, panes, fences and walls are
  never walkable cells; open doors, trapdoors and gates are; snow below five
  layers is). Cells above are classified by what lies under and beside them
  (`WATER_BORDER`, `FIRE_IN_NEIGHBOR`, `DAMAGING_IN_NEIGHBOR`, ...).
- `getPathTypeOfMob` over the mob's box (wide mobs check several columns),
  door rules (`WALKABLE_DOOR` for mobs that open doors, open doors blocked for
  mobs that can't pass), rail rules, and the large-mob danger cap.
- `getNeighbors`/`findAcceptedNode`: four straight and four diagonal moves,
  jumps up to `max(1.125, maxUpStep)`, partial-collision sweeps beside fences
  and closed doors, `tryJumpOn` head-room boxes, sinking through water for
  non-swimmers, falls up to `getMaxFallDistance` (3 idle; a hunting mob adds
  the health it can spare, less 4 per difficulty step below Hard).
- `PathFinder`: Java's node identity and cost-to-max updates, the binary heap's
  comparison order, the 1.5 heuristic weight, the `FOLLOW_RANGE * 16`
  visited-node budget, walked-distance limit and best-node partial paths.
- `PathNavigation`/`GroundPathNavigation`: surface targets, cached paths for an
  unchanged target, waypoint advancement and corner cutting, 100-tick stuck
  detection, per-node timeouts and skeleton sun avoidance support; `MoveControl`
  turns at most 90 degrees a tick and jumps onto steps.

[`entity/mob/navigate.rs`](../../src/entity/mob/navigate.rs) gives each kind
Java's step height, door/float flags, follow range and `setPathfindingMalus`
costs, and routes movement through paths:

- Melee chasers, golems and zombies hunting mobs, skeletons closing range,
  piglins and hoglins, tempted, breeding, leashed and owner-following animals
  repath on `MeleeAttackGoal`'s cadence (4–10 ticks, +5/+10 beyond 16/32
  blocks, +15 without a path; only zombies' seen targets, and unseen ones
  every 20 ticks once a path is used up). Spiders climb straight at a goal
  their path can't reach (`WallClimberNavigation`).
- Villagers walk paths to their walk targets and open closed wooden doors on
  the previous and next path nodes (`InteractWithDoor`).
- Strolls follow `WaterAvoidingRandomStrollGoal`: a 1-in-120 chance a tick,
  `LandRandomPos`/`DefaultRandomPos` targets weighted by `getWalkTargetValue`
  (animals favour grass and light, monsters darkness), and no strolling more
  than 32 blocks from every player unless persistent (`noActionTime`).
- Panics follow `PanicGoal` (water when burning, otherwise a spot within 5
  blocks). Wolves panic only from environmental damage and golems never,
  as in 26.3.
- Mobs without a float goal or brain `Swim` behavior (zombies, skeletons,
  piglins, hoglins, golems) now sink and walk the bottom as Java's
  `travelInWater` does; swimmers keep surface buoyancy.

## Evidence

Automated (`cargo test --release`, desktop and `--no-default-features`):

- 13 path unit tests (plus an ignored cost probe): straight and detoured paths, one-block jumps but not
  two, doors per mob, lava avoidance including the neighbour cost (a path
  walks round a lava strip rather than through a gap beside it), three-block
  idle drops, fences, bottom slabs as half steps, neighbourhood types, path
  following, stuck give-up, steering limits and surface targets.
- `tests/mob_navigation.rs`: a zombie reaches a player behind a 13-wide wall,
  never steps into a lava moat round the player, and wandering cows never walk
  off a 12-block cliff. With straight-line steering forced back on, the wall and
  lava scenarios fail, so they detect the old behavior.
- All existing suites pass. One assertion changed: a zoglin now reaches and
  kills the cow in its 4-second window, so the test accepts a dead cow.

Representative terrain (`zombies_chase_across_generated_terrain`, ignored;
30 zombies 12–20 blocks from a still player on open ground, 30 s at night):

| Seed | Reached (paths) | Entity tick mean / p99 / max |
| --- | --- | --- |
| 7 | 13/13 | 0.034 / 0.170 / 6.4 ms |
| 42 | 13/19 | 0.057 / 0.572 / 3.3 ms |
| 1234 | 30/30 | 0.046 / 0.144 / 1.1 ms |
| 99 | 7/7 | 0.013 / 0.046 / 2.8 ms |
| 2026 | 21/21 | 0.030 / 0.062 / 0.5 ms |
| 5 | 17/17 | 0.029 / 0.121 / 2.1 ms |
| 77 | 24/30 | 0.063 / 0.713 / 3.1 ms |

The misses were inspected: zombies on cliff tops 20+ blocks up and beyond the
35-block follow range, and two boxed in by bamboo, where Java mobs also stop at
the closest reachable node. Before the sinking change, a straight-line run on an
earlier harness reached 26/30, 18/30 and 0/30 on seeds 7, 42 and 1234 against
28/30, 21/30 and 9/30 with paths (that harness placed the player on a tree top
for 1234). The max column is the first tick, when every zombie plans at once.

Search cost on a real `World` (`path_search_cost`, ignored): 21 µs for an open
20-block chase, 137 µs for a search that exhausts a zombie's 560-node budget.

## Remaining differences and open work

- No Java RNG sequences; stroll/panic candidates use VoxelCraft's RNG.
- Unloaded cells are blocked rather than read as air; entity collision boxes
  and the world border are not part of the jump/sweep collision checks.
- Block updates don't yet trigger `shouldRecomputePath`; a path through a newly
  placed block is dropped by the stuck/timeout checks instead.
- Swimming (`SwimNodeEvaluator`), flying (`FlyNodeEvaluator`), amphibious and
  wall-climbing evaluators are not ported: fish, ghasts, parrots, slimes and
  striders keep their own steering. Zombies don't convert to drowned.
- Follow-owner, tempt and breed goals share the melee repath cadence instead
  of their own; avoidance (`AvoidEntityGoal`) still runs on a heading.
- Ordinary-player acceptance (watching mobs path in the desktop game) and an
  M5 frame-time scene with many pathing mobs remain open. MOB-02 stays
  **Partial**.
