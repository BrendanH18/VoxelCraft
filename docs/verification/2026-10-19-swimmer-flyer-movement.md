# Swimmer and flyer movement — 2026-10-19

[Roadmap](../roadmap.md) · [Swim and fly navigation](2026-10-14-mob-navigation-swim-fly.md) · [Mob goals and movement](2026-10-17-mob-goals-movement.md)

Scope: the MOB-02b gaps left by the earlier navigation work. Swimmers used
VoxelCraft's water steering instead of Java's `MoveControl` variants, and
ghasts and blazes kept their own movement. Work was done on
`t3/continue-voxelcraft-build` from `2fa609a`. Reference: the Java Edition 26.3
server (`server-26.3-classes.jar`), decompiled with Vineflower 1.11.1 (Maven
Central, SHA-1 checked). Only headless tests were run: no graphical play.

## Reference classes read

- **Controls:** `MoveControl`, `SmoothSwimmingMoveControl`,
  `AbstractFish.FishMoveControl`, `Guardian.GuardianMoveControl`,
  `Turtle.TurtleMoveControl`, `Axolotl.AxolotlMoveControl` and
  `Ghast.GhastMoveControl`.
- **Travel:** `LivingEntity.travel`, `travelInAir`, `travelInWater`,
  `travelFlying` and `aiStep`, plus `Entity.moveRelative` and
  `getInputVector`. Also the `travelInWater` overrides of `AbstractFish`,
  `Dolphin`, `Guardian`, `Turtle` and `Axolotl`, and `Squid.travel` and
  `aiStep`.
- **Goals:**
  - `RandomSwimmingGoal`, `RandomStrollGoal` and `PanicGoal`;
  - `BreathAirGoal`, `FollowFlockLeaderGoal` and `AbstractSchoolingFish`;
  - the squid's `SquidRandomMovementGoal` and `SquidFleeGoal`;
  - the ghast's `RandomFloatAroundGoal`, `GhastLookGoal` and
    `GhastShootFireballGoal`;
  - the blaze's `BlazeAttackGoal`, `customServerAiStep` and `aiStep`;
  - `NearestAttackableTargetGoal` and `TargetGoal`.
- **Brains and positions:** `AxolotlAi` (activities and speed modifiers),
  `RandomStroll.swim`/`getTargetSwimPos`, `BehaviorUtils.getRandomSwimmablePos`,
  `DefaultRandomPos` and `RandomPos`.
- **Navigation:** `PathNavigation.followThePath`,
  `shouldTargetNextNodeInDirection` and `isClearForMovementBetween`, and the
  `canMoveDirectly` and `isStableDestination` overrides in
  `WaterBoundPathNavigation`, `AmphibiousPathNavigation` and
  `FlyingPathNavigation`.

## Delivered behavior

[`entity/mob/swim.rs`](../../src/entity/mob/swim.rs) moves these mobs one Java
tick at a time with `deltaMovement` in blocks a tick. Simulated time builds up
whole ticks, so a 60 Hz caller gets the same motion as the fixed 20 Hz step.

- **Ticks and inputs.**
  - Each tick drops motion below 0.003 (`LivingEntity.aiStep`), runs the kind's
    move control, then its `travel`.
  - `moveRelative` turns the `zza`/`yya` inputs by Java's `yRot`, which is
    VoxelCraft's yaw minus 90 degrees.
  - `Entity.move` steps up `maxUpStep` from the ground: 1 block for turtles and
    axolotls, 0.6 for the others.
- **Fish:** `FishMoveControl` eases the speed an eighth of the way to its
  target each tick. It climbs a tenth of the speed toward the wanted height,
  turns up to 90 degrees, and adds 0.005 while the eye is in water. Travel
  accelerates 0.01 times the speed, keeps 0.9 of the motion and sinks 0.005.
  - Java drops tiny motion before the 0.005 lift, so idle fish rise 0.005 a
    tick until their eye leaves the water. This is what the reference does.
  - Stranded fish flop 0.4 up with up to 0.05 sideways.
- **Dolphins and axolotls:** `SmoothSwimmingMoveControl` turns 10 degrees of yaw
  and 5 of pitch a tick, with pitch limited to 85.
  - In water the speed is 0.02 of the target for dolphins and 0.1 for axolotls,
    driving inputs of `cos`/`-sin` of the pitch. Out of water it is 0.1 and 0.5,
    scaled down while turning more than 10 degrees.
  - Travel accelerates by `getSpeed` and keeps 0.9. Dolphins add 0.005 in water
    and sink 0.005 without a target; axolotls drift.
  - Stranded dolphins flop 0.5 up and face a random way. An axolotl playing dead
    doesn't move.
- **Guardians:** `GuardianMoveControl` eases the speed by an eighth and adds the
  tail sway: `sin((tickCount + id) × 0.5) × 0.05` sideways and a 0.75-rate term
  vertically. Travel accelerates 0.1 times the speed, keeps 0.9, and sinks only
  while still with no target. Stranded guardians flop 0.5 up. A guardian aiming
  its beam stops its navigation, as `GuardianAttackGoal` does.
- **Turtles in water:** `TurtleMoveControl.updateSpeed` adds 0.005 and lowers the
  speed to at least 0.08 more than 16 blocks from home, and to at least 0.06 for
  babies. It climbs a tenth of the speed toward the target. Travel accelerates
  0.1 times the speed, keeps 0.9, and sinks unless heading for a home within 20
  blocks.
  - On land, turtles keep VoxelCraft's ground movement at the steady speed of
    the control's halving: 0.0525 plus an eighth of the target. Before, they
    walked at their full speed (2.75 blocks a second on a stroll); now they walk
    about 0.31.
- **Squid:** the tentacle cycle advances by `tentacleSpeed`, rerolled one time
  in 10 per cycle. Late in each stroke (past 0.75 of half a cycle) the motion
  becomes the goal's movement vector; between strokes it keeps 0.9. Out of
  water a squid falls straight down.
  - `SquidRandomMovementGoal` picks a 0.2-block vector with a vertical part
    between -0.1 and 0.1. It picks one in 25 selector updates, out of water, or
    when the vector is nearly zero, and zeroes it after 100 idle ticks.
  - `SquidFleeGoal` runs every tick: it sets the vector away from the attacker,
    scaled `3 − (distance − 5) / 5`. Java normalizes into a discarded vector, so
    only the scale applies here too.
- **Ghasts:**
  - `RandomFloatAroundGoal` picks a spot up to 16 blocks away on each axis
    whenever there is no wanted position, or it is within a block or farther
    than 60.
  - Java's height fallback (mirroring a spot above the `MOTION_BLOCKING`
    surface downward) only runs when 64 tries fail `isGoodTarget`, which the
    ghast's `distanceToBlocks` of 0 never does, so it is not applied.
  - `GhastMoveControl` adds 0.1 toward the spot every 2–6 ticks while the swept
    box is clear, and otherwise drops the spot.
  - `travelFlying` has no gravity and keeps 0.91 in air, 0.8 in water and 0.5 in
    lava. Ghasts face their motion, or a target within 64 blocks.
- **Ghast targets and fire:** `NearestAttackableTargetGoal` picks the nearest
  visible survival player within 100 blocks and 4 vertically. It searches on a
  one in 5 selector update and keeps the target for 30 unseen updates.
  `GhastShootFireballGoal` warns at 10 ticks of charge and fires at 20, from 4
  blocks in front, aimed at the target's middle. Then it waits at -40, so shots
  come every 60 ticks while the target is in sight within 64 blocks. Out of sight
  the charge drains.
- **Blazes:**
  - Airborne blazes multiply falling motion by 0.6 a tick (`aiStep`), so they
    settle at 0.19 blocks a tick, not 2.3 blocks a second.
  - `customServerAiStep` adds `(0.3 − motion) × 0.3` while the target's eyes are
    above the blaze's by more than `allowedHeightOffset`. The offset is a
    `triangle(0.5, 6.891)` roll every 100 ticks. They rise at about 0.031 blocks
    a tick, not 6 blocks a second.
  - Travel in the air is `travelInAir` at the 0.02 flying speed. On the ground
    blazes keep VoxelCraft's ground movement.
- **Blaze attack:** `BlazeAttackGoal` with a 48-block `NearestAttackableTargetGoal`
  (in sight).
  - Within 2 blocks and in sight it hits every 20 ticks and walks at the target.
  - Within 48 blocks and in sight it holds still, charges 60 ticks, then fires
    three small fireballs 6 ticks apart. Each is aimed middle to middle with
    `triangle(d, 2.297 × √√distance² × 0.5)` spread on x and z. Then it rests
    100 ticks.
  - Out of sight it walks straight at the target for 5 ticks (`lastSeen < 5`).
  - Without a target it strolls (`WaterAvoidingRandomStrollGoal`, unchanged).

## Goals for swimmers

[`aquatic.rs`](../../src/entity/aquatic.rs) splits the old `think` into
`upkeep` and `steer`. `upkeep` runs whatever the mob is doing: elder Mining
Fatigue, guardian beams, pufferfish stings and Dolphin's Grace. `steer` holds
the movement goals. Navigation's wanted position feeds the move controls
(`followThePath` → `setWantedPosition`).

- **Fish:**
  - `PanicGoal` (1.25), with a new spot 5 blocks out each time one is reached.
  - `AvoidEntityGoal(Player, 8, 1.6, 1.4)`, through the shared goal selector
    (survival players only).
  - `FishSwimGoal`, a `RandomSwimmingGoal` at 1.0 every 40 ticks; its target is
    `getRandomSwimmablePos(10, 7)`.
  - Movement speed is the 0.7 default.
- **Schools (`AbstractSchoolingFish`):** cod, salmon and tropical fish follow a
  leader. A follower paths to it every 10 ticks and drops it past 11 blocks.
  - A fish that neither leads nor follows counts down `nextStartTick` (100–110
    selector updates). Then it joins a leader with room within 8 blocks or
    gathers the loose fish around it.
  - `addFollowers` limits the loose fish before removing the leader, as Java
    does, so a new leader may take one place fewer.
  - Schools hold 8, salmon 5. Leaders swim randomly; followers don't.
- **Dolphins:** below 140 ticks of air, `BreathAirGoal` finds the first air in
  its column and the four beside it within 8 blocks up. It replans every tick
  and adds its extra 0.02 stroke. Otherwise `RandomSwimmingGoal` at 1.0 every
  10 ticks; movement speed 1.2.
- **Guardians:** `RandomStrollGoal` at 1.0 every 80 ticks, to a
  `DefaultRandomPos.getPos(10, 7)` spot scored by `getWalkTargetValue` (water
  is 10 plus the light cost). Movement speed is 0.5, elders 0.3.
- **Axolotls:**
  - Prey attacks walk at 0.6 in water.
  - When idle with no walk target, `RandomStroll.swim(0.5)` picks one through
    `getTargetSwimPos`'s widening tiers, or `stroll(0.15)` on land. On land
    `SmoothSwimmingMoveControl` halves the speed.
  - The table speeds are now the brain's: love 0.2, temptation 0.5, following
    an adult 0.6, or 0.15 on land.
- **Correction:** `AxolotlAi` has no panic. The 2026-10-17 goal table gave
  axolotls a `Panic(2.0)` entry, so they fled every hit. They no longer do.
  Squid and dolphins don't panic either: squid flee their attacker and
  dolphins have no panic goal.
- **Random positions:** random swim, stroll, panic and avoid spots use each
  navigation's `isStableDestination`. Water navigation needs any cell that
  isn't a full solid cube. Amphibious navigation, for axolotls and turtles,
  needs anything but air below.

## Navigation: `canMoveDirectly`

`shouldTargetNextNodeInDirection` moves on to the next node when, within 2
blocks of the current one, there is a clear line from the mob's middle to that
node. The line is clipped against collision shapes, ignoring fluids. Water
and flying navigation do this always; amphibious navigation only in a liquid;
ground navigation never. VoxelCraft lacked this, so fast swimmers with
dolphins' 10-degree turns circled nodes. A dolphin short of air now surfaces
from 10 blocks down in about 30 ticks, where it took about 70. Flying
navigation's fluid blocking in this check is not modelled.

## Evidence

All on this branch's working tree from `2fa609a`, macOS (M5 MacBook Pro),
release build:

```sh
cargo fmt && cargo clippy --release --all-targets -- -D warnings && cargo test --release
cargo clippy --release --no-default-features --all-targets -- -D warnings
cargo test --release --no-default-features
```

- **`entity::mob::swim` tests:**
  - `swimmers_settle_at_javas_water_speeds`: cod 0.070, dolphins 0.24,
    axolotls 0.25 and guardians about 0.5 blocks a tick;
  - `idle_fish_rise_slowly_and_drag_stops_them`;
  - `fish_flop_on_land_and_squid_fall_straight`;
  - `dolphins_turn_ten_degrees_a_tick`;
  - `ghasts_drift_toward_their_float_target_without_gravity`;
  - `falling_blazes_hover_and_lift_toward_a_target_above`: the 0.0784/0.412
    fall and 0.0098/0.314 rise;
  - `slowness_slows_swimmers`: Slowness II takes 30% off a cod's speed.
- **`entity::aquatic` tests:**
  - `swimming_fish_stay_in_the_water`;
  - `axolotls_hunt_fish_and_loose_fish_gather_a_school`;
  - `salmon_schools_hold_at_most_five`;
  - `fish_flee_survival_players_but_not_creative_ones`;
  - `dolphins_short_of_air_swim_up_to_breathe`;
  - the beam, sting and Dolphin's Grace tests, now through `upkeep`.
- **`entity::mob::goals`:** `a_hurt_axolotl_never_panics_and_stays_tempted`
  replaces the panic test.
- **`entity` tests:**
  - `ghasts_fire_every_sixty_ticks_while_they_see_their_target`;
  - `ghasts_hover_shoot_and_credit_a_deflected_blast`: a ghast under open sky
    floats about without falling;
  - `blazes_hover_shoot_fireball_bursts_and_shrug_off_fire`, unchanged and
    passing.
- **Behavior probe (not kept):** a minute of a walled 64 × 17 × 64 sea with
  cod, salmon, dolphins, guardians, 40 squid, axolotls, pufferfish and
  turtles.
  - No swimmer left the water.
  - 23 of 33 cod and 10 of 16 salmon were in schools.
  - Squid showed no vertical drift (mean height -0.23 from a uniform start).
  - Mean path speeds were cod 1.2, dolphins 2.8 and guardians 1.8 blocks a
    second, and axolotls 2.1.

## Performance

`mob_movement_cost` (`cargo test --release --lib mob_movement_cost --
--ignored --nocapture`) times two scenes over 400 ticks: 186 swimmers in a
64 × 16 × 64 sea, then 8 ghasts and 8 blazes in a roofed Nether. It ran three
times on this tree and on a detached checkout of `2fa609a` with the same test
added:

| Scene | `2fa609a` (ms/tick) | This change (ms/tick) |
|---|---|---|
| 186 swimmers | 0.125–0.126 | 0.100–0.109 |
| 8 ghasts, 8 blazes | 0.043–0.045 | 0.039–0.041 |

The ghast reachability sweep samples at half the box's smallest extent (at
most a block), not every quarter block, which bounds it to about 60 box tests
for the longest float. No M5 frame-time scene was run.

## Remaining differences

- **Dolphins:** these goals are not ported: `DolphinSwimToTreasureGoal`,
  `DolphinSwimWithPlayerGoal` (4.0), `DolphinJumpGoal`, item play,
  `FollowPlayerRiddenEntityGoal` (boats), avoiding guardians, retaliation
  (`HurtByTargetGoal`/`MeleeAttackGoal`) and `TryFindLiquidGoal`.
- **Turtles:** the goal set is unchanged. `TurtleTravelGoal`,
  `TurtleGoToWaterGoal`, the 100-tick stroll and `TurtlePathNavigation`'s
  travel position are not ported. On land the control's speed halving is a
  steady ground speed.
- **Axolotls:** `TryFindLiquid` and `SetWalkTargetFromLookTarget` are missing,
  and land movement is VoxelCraft's ground model.
- **Guardians:** missing are `MoveTowardsRestrictionGoal` (monument home
  restriction), the stroll's `trigger` on being hurt, the look goals, and
  targeting squid and axolotls.
- **Squid:** the attacker is the nearest player for five seconds after a
  player hit, standing in for `getLastHurtByMob`. Body yaw eases by the
  wrapped difference. Rendering doesn't use the tentacle cycle.
- **Schools:**
  - Leaders' sizes are recounted from live followers each tick. Java's counter
    misses a follower that dies until its 1-in-200 neighbor check.
  - Spawn-group leaders (`SchoolSpawnGroupData`) are approximated by an
    immediate first search, and candidates are taken in entity order.
- **Ghasts:**
  - `canReach` samples the swept box rather than walking each block.
  - The target search uses a sphere rather than the inflated box.
- **Blazes:** `HurtByTargetGoal` (retaliating and alerting others) is not
  ported, and ground walking keeps VoxelCraft's model.
- **All swimmers:** water currents don't push them. Speed and Slowness scale
  the movement speed the controls read, as Java's attribute modifiers do.
  Bees, allays and vexes are not registered.
- **No graphical play** of any of this: fish schools, dolphins breathing and
  breaching, guardian sway, squid strokes, ghast floating and fire, and blaze
  hovering all need ordinary-player acceptance.
