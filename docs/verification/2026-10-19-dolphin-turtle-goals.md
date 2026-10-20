# Dolphin and turtle goals — 2026-10-19

[Roadmap](../roadmap.md) · [Swimmer and flyer movement](2026-10-19-swimmer-flyer-movement.md) · [Mob goals and movement](2026-10-17-mob-goals-movement.md)

Scope: the last MOB-02b goal gaps the
[swimmer and flyer record](2026-10-19-swimmer-flyer-movement.md) left open.
Dolphins ran only breathing and random swims, and their Dolphin's Grace was
an approximation. Turtles had none of their travel, go-home or go-to-water
goals. The elder guardian had no home, and blazes didn't retaliate. Work
was done on `t3/finish-v0-8-next-steps` from `d4b784b`.

The reference is the Java Edition 26.3 server (`server-26.3-classes.jar`),
decompiled with Vineflower 1.11.1 (Maven Central, SHA-1 checked). The
extracts are kept outside the repository in
`voxelcraft-java-reference/v0.8-agent-extracts/aquatic-goals`. Only
headless tests were run: there was no graphical play.

## Reference classes read

- **Selector:** `GoalSelector`, `WrappedGoal` and `Goal` (control flags,
  `isInterruptable`, `requiresUpdateEveryTick`, `reducedTickDelay`), and
  `Mob.serverAiStep`'s full and partial updates.
- **Dolphins:**
  - `Dolphin` and its inner `DolphinSwimToTreasureGoal`,
    `DolphinSwimWithPlayerGoal`, `ItemGoal`, `MoveToItemGoal` and
    `PlayWithItemsGoal`;
  - `BreathAirGoal`, `TryFindLiquidGoal`, `RandomSwimmingGoal`,
    `RandomStrollGoal`, `RandomLookAroundGoal`, `LookAtPlayerGoal`,
    `DolphinJumpGoal`, `MeleeAttackGoal`, `FollowPlayerRiddenEntityGoal` and
    `AvoidEntityGoal`;
  - `HurtByTargetGoal` and `TargetGoal`;
  - `Mob.aiStep` (item pickup), `Mob.isWithinMeleeAttackRange`,
    `Entity.hasMovedHorizontallyRecently` and `getMotionDirection`;
  - `ChunkGenerator.findNearestMapStructure` and its random-spread ring search.
- **Turtles:** `Turtle` and its `TurtlePanicGoal`, `TurtleBreedGoal`,
  `TurtleLayEggGoal`, `TurtleGoToWaterGoal`, `TurtleGoHomeGoal`,
  `TurtleTravelGoal`, `TurtleRandomStrollGoal`, `TurtleMoveControl` and
  `TurtlePathNavigation`. Also `MoveToBlockGoal`, `PanicGoal`,
  `BlockPos.withinBoxByManhattanDistance` and `LevelReader`.
- **Positions:** `DefaultRandomPos` (`getPos`, `getPosTowards` and
  `getPosAway`), `RandomPos` and `GoalUtils` (the home restriction).
- **Guardians and blazes:** `Guardian.registerGoals`,
  `ElderGuardian.customServerAiStep`, `MoveTowardsRestrictionGoal`,
  `Mob.isWithinHome`, and `Blaze.registerGoals`.

## The flagged goal selector

[`mob/selector.rs`](../../src/entity/mob/selector.rs) is Java's
`GoalSelector` with control flags. On the full update (`(tickCount + id) % 2
== 0`, or the first two ticks) it does three things:

1. It stops running goals that can't continue, then frees the flags of
   stopped goals.
2. In registration order, it starts every idle goal whose flags are free, or
   held by an interruptible goal of a worse priority. It stops that holder.
3. It ticks every running goal.

Between full updates only `requiresUpdateEveryTick` goals tick. As in Java,
a goal that loses one flag keeps its other flags until the next cleanup, and
flagless goals run beside everything.

The animal tables in `goals.rs` hold only move goals, so they keep the
simpler one-goal selector, which is exact for them.

## Dolphins

[`mob/dolphin.rs`](../../src/entity/mob/dolphin.rs) registers 26.3's
dolphin goals in order:

| Priority | Goal | Flags |
|---|---|---|
| 0 | `BreathAirGoal`, uninterruptible | move, look |
| 1 | `DolphinSwimToTreasureGoal`, uninterruptible | move, look |
| 2 | `DolphinSwimWithPlayerGoal` (4.0) | move, look |
| 4 | `RandomSwimmingGoal` (1.0, every 10) | move |
| 4 | `RandomLookAroundGoal` | move, look |
| 5 | `LookAtPlayerGoal` (6 blocks) | look |
| 5 | `DolphinJumpGoal`, uninterruptible | move, jump |
| 6 | `MeleeAttackGoal` (1.2, follows unseen targets) | move, look |
| 7 | `MoveToItemGoal` | move |
| 8 | `PlayWithItemsGoal` | none |
| 8 | `FollowPlayerRiddenEntityGoal` (boats) | none |
| 9 | `AvoidEntityGoal` (guardians, 8 blocks, 1.0) | move |

- **Swimming with players:** the nearest player within 10 blocks, if
  sprint-swimming and not the dolphin's target, gets 100 ticks of Dolphin's
  Grace when the goal starts. They get it again on one full update in six
  while still swimming. The dolphin paths to them at 4.0 and stops within 2.5
  blocks. The goal ends past 16 blocks or when they stop swimming.
  `Target::swimming` (`Player.isSwimming`) now reaches the entity simulation
  from the desktop, controller, LAN and CLI players.
- **Treasure:** a fish from `ItemTags.FISHES` (cod, salmon, tropical fish,
  pufferfish, cooked cod and cooked salmon) used on a dolphin sets `GotFish`.
  This goes through the shared `use_animal` action, so every input path
  feeds it. The fish is consumed.
  - With 100 or more ticks of air, the goal looks up the nearest ocean ruin
    or shipwreck. `Temples::locate_rings` ports `findNearestMapStructure`'s
    random-spread search: 50 rings of placement regions, the first valid
    start per kind in Java's x-then-z order, the nearer kind winning, and the
    search ending on the first ring with a find. Validity is VoxelCraft's
    biome and ground check, without building the structure. The position is
    the start chunk's lowest corner at y 0.
  - The dolphin then swims legs toward it (`getPosTowards(16, 1, π/8)`, then
    `(8, 4, π/2)`, then `(8, 5, π/2)` if the spot isn't swimmable water) at
    1.3 whenever navigation is done or its target is within 12 blocks.
  - It forgets the fish within 4 blocks of the treasure's column, or at once
    when there is no treasure or no next spot.
- **Item play:** under mobGriefing, a dolphin with an empty main hand picks
  up any loose item its box touches (grown a block across). That is
  `Mob.aiStep`, which runs every tick. `MoveToItemGoal` paths at 1.2 to the
  first item in water within 8 blocks that has no pickup delay.
  `PlayWithItemsGoal` throws a held item on the next full update. The item
  leaves 0.3 forward along the dolphin's yaw and pitch, with Java's random
  spread, and can't be picked up for 40 ticks. The main hand saves, and drops
  on death (`setGuaranteedDrop`).
- **Leaps:** on one full update in five, a dolphin whose block and the steps
  0, 1 and 4 to 7 ahead along its facing are water, with two cells of air
  above each, gains (0.6 × step, 0.7, 0.6 × step) a tick of motion. While it
  is in the air its pitch follows the arc and eases back to level near the
  top. The goal ends back in the water or on land.
- **Retaliation:** a player who hurts an adult dolphin, in melee or with a
  projectile, becomes its target. They also become the target of every
  dolphin without one within its 16-block follow range and 10 blocks up or
  down (`HurtByTargetGoal(Guardian).setAlertOthers()`).
  - A hit is judged on the next update, as `HurtByTargetGoal.canUse` runs
    on the next selector update: if the attacker is then creative or
    spectating (`canAttack(attacker, HURT_BY_TARGETING)`), no target changes
    and nobody is alerted. The same holds for blazes.
  - The target is dropped once it is out of survival, beyond 16 blocks, or
    unseen for 150 selector updates (`TargetGoal`).
  - `MeleeAttackGoal` re-checks `canUse` every 20 ticks. It paths at 1.2 with
    Java's recalculation delays, and bites for 3 every 20 ticks when the
    target is within `DEFAULT_ATTACK_REACH` and in sight.
  - Dolphins with a target no longer sink in `travelInWater`.
  - Random swims (4) outrank the attack (6), so an angry pod bites in short
    bouts between swims. That is the reference's priority order.
- **Boats:** a dolphin within 5 blocks of a boat whose rider moved
  horizontally since the last update follows that rider. Every full update
  it adds an extra 0.015 stroke, or 0.01 once ahead. Every 5 updates it paths
  to the cell behind and below the rider, or, once within 4 blocks, to 10
  blocks ahead along the rider's facing, until it falls 12 behind.
- **Guardians:** a dolphin that sees a guardian within 8 blocks flees to a
  `getPosAway(16, 7)` spot farther from it.
- **Look goals:** `RandomLookAroundGoal` and `LookAtPlayerGoal` only turn
  the head, and a dolphin's head turns one degree, so only their hold on the
  flags is modelled. While the look-around goal holds the move flag, random
  swims can't start, as in Java.

## Turtles

[`mob/turtle.rs`](../../src/entity/mob/turtle.rs) ports 26.3's turtle
table, apart from the head-only `LookAtPlayerGoal`:

| Priority | Goal | Flags | Here |
|---|---|---|---|
| 0 | `TurtlePanicGoal` (1.2) | move | `turtle_steer` |
| 1 | `TurtleBreedGoal` (1.0) | move, look | the shared `BreedGoal` |
| 1 | `TurtleLayEggGoal` (1.0) | move, jump | `Goal::LayEgg` |
| 2 | `TemptGoal` (1.1, seagrass) | move, look | the shared `TemptGoal` |
| 3 | `TurtleGoToWaterGoal` (1.0) | move, jump | `Goal::GoToWater` |
| 4 | `TurtleGoHomeGoal` (1.0) | none | `turtle_side_goals` |
| 7 | `TurtleTravelGoal` (1.0) | none | `turtle_side_goals` |
| 9 | `TurtleRandomStrollGoal` (1.0, every 100) | move | `turtle_steer` |

- **Panic:** a hurt turtle runs to the first water by Java's Manhattan
  order within 7 blocks across and 1 up or down, unless it stands inside a
  block. Otherwise it runs to a `getPos(5, 4)` spot. It picks a fresh target
  each time one is reached while the hit is fresh.
- **Laying:** with eggs and within 9 blocks of home, a turtle runs
  `MoveToBlockGoal`'s search for sand with air above, 16 blocks out.
  - The search runs every 200 to 399 ticks, counted in selector updates. It
    checks the layer below the feet first, then the feet layer, then two
    below, each in square rings from the middle outward.
  - Each tick (`requiresUpdateEveryTick`) it walks there, re-planning every
    40 ticks.
  - Once reached and out of water it counts 200 ticks, then lays one to four
    eggs on the cell above the sand. It loses its eggs and is in love for 600
    ticks.
  - It gives up after 1200 ticks of trying, or after its random stay time.
  - As in Java, the count isn't reset when the goal stops.
- **Going to water:** babies on land, and adults on land that aren't heading
  home and carry no eggs, search 24 blocks out for a water block. The search
  starts at layer -1, so it covers only the layer two below the feet. They
  walk to the cell above it, re-planning every 160 ticks, and give up after
  1200. The goal's speed reads `isBaby` in the constructor, before a
  hatchling is made a baby, so it is always 1.0.
- **Going home:** adults with eggs, or one in 350 full updates when home is
  64 or more blocks away, set `goingHome`. Whenever navigation is done they
  swim or walk a leg toward home:
  - `getPosTowards(16, 3, π/10)`, then `(8, 7, π/2)`;
  - away from home, a spot that isn't water is replaced by a
    `(16, 5, π/2)` one.
  - The goal stops within 7 blocks of home, when no spot is found, or after
    300 updates within 16 blocks of home.
  - While going home, water no longer scores 10 in `getWalkTargetValue`, and
    `travelInWater` doesn't sink within 20 blocks of home.
- **Travelling:** in water, not going home, without eggs, a turtle picks a
  spot up to 512 blocks away across and 4 up or down. The height is capped
  at sea level. It swims legs toward the spot whenever navigation is done,
  only into loaded chunks (`hasChunksAt` ±34 blocks). While it travels,
  `TurtlePathNavigation` accepts only water as a destination. The goal ends
  when a leg's path ends, or when the turtle is going home, in love or
  carrying eggs, so the next full update picks a new spot.
- **Strolling:** only on land, not going home, without eggs, every 100 ticks,
  to a `getPos(10, 7)` spot.
- **Walk values:** water scores 10 unless the turtle is going home, sand
  below scores 10, and everything else is the light cost. Grass is not
  preferred.
- GoHome and Travel have no flags. They are evaluated after the move goals
  of priority 3 or better and before the stroll, which is their place in
  registration order, and they replan only when navigation is done.

## Guardians and blazes

- **Elder guardians:** an elder guardian sets a 16-block home where it first
  acts (`ElderGuardian.customServerAiStep`). Players outside the home aren't
  beam targets (`TargetGoal.canAttack`).
  - Out of the home, `MoveTowardsRestrictionGoal` (5) outranks the stroll (7)
    and heads back along `getPosTowards(16, 7, π/2)` spots.
  - Random spots now apply `GoalUtils`' home restriction, near home, and
    `RandomPos`' pull of up to half the range toward home.
  - `Mob.homePosition`/`homeRadius` save as `restrict`. Regular guardians
    have no home, as in Java.
- **Blazes:** a player who hurts a blaze, in melee or with a projectile,
  becomes its target and the target of every blaze without one within 48
  blocks and 10 blocks up or down (`HurtByTargetGoal(this).setAlertOthers()`).
  The target stays while unseen for up to 150 updates instead of the nearest
  target goal's 30.

## Corrections to earlier behavior

- **Dolphin's Grace:** before, every dolphin gave it each second to any
  player in water within 9 blocks. Now only the swim-with-player goal gives
  it, to the nearest player within 10 if they are sprint-swimming.
- **Breathing:** `BreathAirGoal` doesn't `requiresUpdateEveryTick`, so its
  re-plan and extra 0.02 stroke now come on full updates only, not every
  tick. Dolphins short of air still surface within the existing test's 40
  ticks.
- **Turtle panic:** turtles never panicked before, because the generic
  path's random spot ignored water and the aquatic steering reset their
  panic. They now run for water.
- **Turtle strolls:** strolls used VoxelCraft's 120-tick water-avoiding
  stroll on land and in water. They now run every 100 ticks on land only.
- **Nesting:** nesting was VoxelCraft's search 4 blocks out and 3 layers
  deep, with a 10-second dig and a walk to the exact home cell. It is now the
  two goals above. The old `nest_time` save key is no longer written. The
  entity-storage fixture test ignores it when comparing a resaved v0.8 save.
- **Save keys:** the new keys `restrict`, `got_fish` and `hand` are written
  only when set, so other mobs save exactly as before.

## Evidence

All on this branch's working tree from `d4b784b`, on macOS (M5 MacBook Pro),
in a release build:

```sh
cargo fmt && cargo clippy --release --all-targets -- -D warnings && cargo test --release
cargo clippy --release --no-default-features --all-targets -- -D warnings
cargo test --release --no-default-features
```

All of them pass: 1,374 tests, and 1,282 headless. The new tests:

- **`entity::mob::selector`:** `flags_follow_javas_goal_selector`
  (replacement by priority, uninterruptible goals, flagless goals, and the
  ticks between full updates).
- **`entity::mob::dolphin`:**
  - `dolphins_swim_with_swimming_players_and_grant_their_grace`: 6 to 40
    graces in 200 ticks, closer than 3 blocks, and none for a player who
    isn't swimming;
  - `fed_dolphins_lead_to_treasure`: a treasure 70 blocks away is reached and
    the fish forgotten; with no treasure it is forgotten at once;
  - `hurt_dolphins_alert_their_pod_and_bite`: only dolphins in the follow
    range are alerted, both attack, and a creative player is dropped;
  - `creative_hits_leave_a_dolphins_target_alone`: a hit judged when its
    attacker is creative changes no target and alerts nobody, and one judged
    in survival counts;
  - `dolphins_avoid_the_nearest_visible_guardian`: a guardian behind a wall
    doesn't hide a farther visible one;
  - `dolphins_fetch_and_toss_items`;
  - `dolphins_leap_from_the_surface`;
  - `dolphins_swim_away_from_guardians`;
  - `dolphins_follow_a_moving_boat`;
  - `dolphin_and_home_state_round_trips_through_saves`;
  - `cardinal_matches_javas_direction_from_yaw`.
- **`entity::mob::turtle`:**
  - `turtles_travel_through_open_water`;
  - `turtles_on_the_beach_walk_back_into_the_sea`;
  - `turtles_with_eggs_head_home`: home from 17 blocks out on eight seeds;
  - `turtles_lay_eggs_on_sand_at_home`: one to four eggs on sand, placed in
    the same update that takes the eggs, then 30 seconds of love, on eight
    seeds (a walled pit keeps the turtle on its nest);
  - `a_later_clutch_on_the_same_cell_overwrites_the_earlier`;
  - `hurt_turtles_heading_home_still_run_for_water`: panic plans before the
    flagless go-home goal ticks;
  - `hurt_turtles_run_for_the_water`: the panic path targets water;
  - `block_searches_follow_javas_order`.
- **`entity::aquatic`:** `elder_guardians_keep_to_their_home`.
- **`entity`:** `hurt_blazes_alert_blazes_within_their_follow_range`.
- **`tests/entity_storage`:** the v0.8 fixture still resaves to the same
  entities.

## Performance

`mob_movement_cost` (`cargo test --release --lib mob_movement_cost --
--ignored --nocapture`) gains a third scene: 40 dolphins and 10 turtles at
sea and 10 turtles on a sand beach, over 400 ticks. The same test ran three
times on this tree, and three times on a temporary detached checkout of
`d4b784b` with the test copied in:

| Scene | `d4b784b` (ms/tick) | This change (ms/tick) |
|---|---|---|
| 186 swimmers | 0.100–0.160 | 0.104–0.114 |
| 8 ghasts, 8 blazes | 0.042–0.044 | 0.044–0.046 |
| 40 dolphins, 20 turtles | 0.039 | 0.042–0.045 |

The dolphins' entity searches (items, boats and guardians) run only on each
dolphin's full selector update and use the mob index. Without that, the
coastal scene cost 0.047–0.050 ms. The turtle block searches run once every
200 to 399 ticks per goal, as in Java. No M5 frame-time scene was run.

## Remaining differences

- **Dolphins:**
  - Babies are missing. 26.3 spawns one in ten as a baby (`AgeableMobGroupData(0.1)`),
    and fish grow them instead of setting `GotFish`.
  - Only player attackers are credited, and `universalAnger` is unimplemented.
  - `BLOCKS_DOLPHIN_JUMP` isn't checked.
  - The jump, play and eat sounds aren't played. The treasure's happy-villager
    particles are green effect particles.
  - The item pickup box is approximate.
  - A rider "moved recently" is measured between entity updates.
  - Riders of mobs count as passengers.
  - `TryFindLiquidGoal` and the nautilus follow goal are left out (see
    above).
- **Turtles:**
  - **Laying time on an open beach:** a turtle with eggs can take a long
    time to lay there. The go-home goal restarts on every update while the
    turtle carries eggs (`canUse` is true with an egg), and walks it away
    from its nest whenever its path ends. A path to the nest can also end on
    the corner Java's wide-mob node position gives a 1.2-block turtle, out of
    the lay goal's 1.0 reach.
    - A 60-seed probe from 17 blocks out (the beach in the turtle tests) put
      every turtle home within 1,283 ticks. About one in five hadn't laid
      after 3,000 ticks; the rest laid after about 2,000.
    - Whether Java's turtles behave the same here is unverified. The earlier
      single-seed test was flaky across platforms for this reason, and was
      split into the two deterministic tests above.
  - `LookAtPlayerGoal` isn't modelled.
  - The digging particles and the laying sound aren't played.
  - On land a turtle walks with VoxelCraft's ground movement at the control's
    halved speed, as before.
- **Guardians:** leashed mobs don't get a home (`PathfinderMob.whenLeashedTo`),
  and guardians still don't target squid or axolotls.
- **Blazes:** only player attackers are credited.
- **All swimmers:** water currents still don't push them.
- **Treasure search:** the structures themselves still differ from Java (v0.12).
- **No graphical play** of any of this. Swimming with dolphins, feeding them
  and following them to treasure, their leaps and item play, boats, turtles
  travelling, returning home and laying eggs, and the elder guardian's home
  all need ordinary-player acceptance.
