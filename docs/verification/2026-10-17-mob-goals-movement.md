# Mob goals and movement — 2026-10-17

[Roadmap](../roadmap.md) · [Ground navigation](2026-10-14-mob-navigation.md) · [Swim and fly navigation](2026-10-14-mob-navigation-swim-fly.md) · [Strider lava paths](2026-10-15-strider-lava-paths.md)

Scope: the MOB-02 gaps the navigation records left open: goal priority order
(MOB-02c), the follow, tempt, breed and avoid goals' cadences, and entity
collision during movement (MOB-02a). Work began on `agent/mob-ai` after
`77a6885` and the integrator finished it on `t3/roadmap-next-stage-v0-8`.
Reference: the Java Edition 26.3 server
(`server-26.3-classes.jar`), read with `javap -c -p`. Headless tests only:
there was no graphical run or benchmark.

## Reference classes read

- **Animal goals:** `registerGoals` of `AbstractCow`, `Pig`, `Sheep`,
  `Chicken`, `Wolf`, `Cat`, `Fox`, `Rabbit`, `AbstractHorse`, `Llama`,
  `TraderLlama`, `Turtle`, `Strider` and `Parrot`.
- **Brain animals:** the idle activities of `GoatAi`, `CamelAi`, `AxolotlAi`
  and `HoglinAi`.
- **Other mobs:** `WanderingTrader`, `Creeper` and `AbstractSkeleton`, plus
  each kind's `createAttributes` for movement speed.
- **Goal machinery:** `GoalSelector`, `Goal.adjustedTickDelay` and
  `reducedTickDelay`.
- **Movement goals:** `PanicGoal` (`shouldPanic`, `getLastDamageSource`),
  `BreedGoal`, `TemptGoal`, `FollowParentGoal`, the brain's `BabyFollowAdult`,
  `FollowOwnerGoal`, `TamableAnimal.teleportToAroundBlockPos` and
  `unableToMoveToOwner`, `AvoidEntityGoal`, `WolfAvoidEntityGoal` (llama
  strength), `CatAvoidEntityGoal` and `DefaultRandomPos.getPosAway`.
- **Steering:** `MoveControl` and `Mob.setSpeed`, plus the ground branch of
  `LivingEntity.travel`.
- **Pushing:** `Entity.push(Entity)`, `LivingEntity.pushEntities` and
  `doPush`.
- **Collision in paths:** `PathNavigationRegion.getEntityCollisions`.

## Delivered behavior

[`entity/mob/goals.rs`](../../src/entity/mob/goals.rs) is Java's goal
selector for the shared movement goals:

- **Priority tables.** Each animal VoxelCraft has gets its 26.3
  `registerGoals` order, with each goal's speed modifier:
  - panic, breed, tempt, follow parent, follow owner, the avoid goals, the
    strider's lava goal, the turtle's nest goal and random stroll;
  - goats, camels, axolotls and hoglins use their brain idle activity's order;
  - wandering traders, creepers and skeletons carry their avoid goals.

  A running goal keeps going until its `canContinueToUse` fails. Only a goal
  with a strictly better priority whose `canUse` passes takes over. Goals
  start and tick on the selector's full update, every other tick, while
  navigation runs every tick.
- **Follow-parent order (MOB-02c).** Babies follow an adult after breeding
  and temptation, as Java registers them. Striders' lava goal (4) outranks
  follow parent (5), and turtles have no follow-parent goal.
- **Speeds.** A goal's speed is the movement speed attribute times its
  modifier, converted to a steady ground speed with Java's ground travel. This
  inverts the existing `path::java_speed`: `20 s² / (1 − 0.546)` blocks a
  second. Strolling and panic now use each kind's table speeds instead of
  VoxelCraft's fixed wander speed and 3.4 blocks a second.
- **Panic.** `getLastDamageSource` remembers a hit for 40 ticks, then the
  run ends once its current path is done. It used to last a random 3–5
  seconds.
- **Breeding.** A pair courts for `adjustedTickDelay(60)` goal updates within
  3 blocks of each other, about 60 ticks, then breeds. The partner is chosen
  by `getFreePartner` (nearest within 8 blocks, in love, not panicking) and
  kept while it stays valid.
- **Temptation.** The nearest player within 10 blocks holding a tempt item
  attracts the animal. It stops 2.5 blocks away, and `calmDown` is
  100 ticks after the goal stops. Tempt items follow Java: horses and mules
  follow golden carrots and apples, and wolves, foxes, parrots and hoglins
  are never tempted.
- **Following.** `FollowParentGoal` follows from 3 blocks out and gives up
  past 16. `BabyFollowAdult` covers 5–16 blocks. `FollowOwnerGoal` starts at
  10 blocks for wolves and cats and 5 for parrots, and stops at 2, 5 and 1
  respectively. Follow goals replan every `adjustedTickDelay(10)`. The owner
  goal zeroes the water malus while it runs and teleports to the owner from
  12 blocks out (ten tries, 2–3 blocks out, not onto leaves). Sitting,
  riding or a leash stops it.
- **Avoiding.** `AvoidEntityGoal` flees the nearest visible entity within
  `maxDist` (and 3 blocks vertically) to a `getPosAway(16, 7)` spot farther
  from that entity. It walks at the walk modifier and sprints within 7
  blocks. The goals present:
  - rabbits flee survival players, wolves and monsters;
  - foxes flee players and wild wolves;
  - untamed cats flee players;
  - wild wolves flee llamas, by the llama's strength roll;
  - creepers flee cats, unless swelling;
  - skeletons and wither skeletons flee wolves;
  - wandering traders flee zombies, pillagers and zoglins.

  These replace the earlier ad-hoc creeper, wolf and trader flight code.
- **Pushing (MOB-02a).** Separation now follows `Entity.push`: every ticking
  mob pushes each pushable mob whose box intersects its own, by 0.05 blocks
  a tick scaled by the square root of the dominant axis. An overlapping pair
  is pushed twice a tick, once by each mob. Sleeping or inactive villagers and
  vehicles with riders are not pushed.
- **Entity boxes in paths.** Java builds paths over a
  `PathNavigationRegion`, whose `getEntityCollisions` returns an empty list,
  so entity boxes never affect path checks. The MOB-02a gap text ("entity
  collision boxes not in path checks") described a difference that does not
  exist; the movement-time pushing above is what Java has.

Integration review fixes (Codex review of the merge):

- Aquatic mobs forget a hit after 40 ticks as well. Their steering has no
  panic run, so a hurt axolotl used to stay panicking, and could never be
  tempted or bred again.
- An axolotl playing dead (`PLAY_DEAD`, its top activity) runs no goals until
  the timer ends; the countdown and healing continue.
- The sheep's eating (`EatBlockGoal`, priority 5) holds it still and stops
  strolling. Panic and every goal that outranks eating end the meal, and
  eating starts only when no table goal is running.

Per-tick work is bounded:
- the entity searches (follow targets and avoided mobs) use the mob grid and
  run only on a mob's full selector update;
- tracked entities are looked up through one uid map per tick;
- goal selection is a scan of a table of at most seven entries.

## Evidence

- **Unit tests in `entity::mob::goals`:**
  - `tables_keep_javas_priority_order`
  - `panic_outranks_temptation`
  - `panic_lasts_two_seconds_then_finishes_its_path`
  - `wolves_follow_their_owner_from_ten_blocks_and_ignore_food`
  - `following_the_owner_replans_every_ten_ticks`
  - `babies_follow_an_adult_of_their_kind`
  - `tempted_animals_calm_down_for_a_hundred_ticks`
  - `hoglins_and_foxes_are_not_tempted`
  - `skeletons_back_away_from_wolves`
  - `rabbits_flee_survival_players_only`
  - `creepers_flee_cats_unless_swelling`
  - `a_hurt_axolotl_forgets_the_hit_and_is_tempted_again` (including playing dead)
  - `grazing_sheep_stand_still_until_panic_ends_the_meal`
- **`entity::mob_index`:** `pushes_follow_javas_entity_push` checks the push
  constants against `Entity.push`. The existing separation test now expects
  Java's pairwise pushes.
- **Breeding tests** (cow and turtle in `animals`, horse and donkey in
  `mounts`) run through the goal selector. Courtship now takes about 60 ticks
  (56–62 asserted) rather than a fixed 61-tick loop of `tick_animals`.
- The integrated gate run is recorded in the
  [requirement inventory](../requirements.md) change that merged this work.

## Remaining differences

- **Swimmer move controls (MOB-02b).** `FishMoveControl`,
  `SmoothSwimmingMoveControl` and the turtle, guardian, dolphin and axolotl
  controls are not ported. Swimmers keep VoxelCraft's water steering on
  Java's paths. Axolotls keep VoxelCraft's 2 blocks a second in place of
  `AxolotlAi.getSpeedModifier`.
- **Flyers (MOB-02b).** Ghasts (`GhastMoveControl`, `RandomFloatAroundGoal`,
  `GhastLookGoal`) and blazes (their goals and `aiStep` hover) keep their own
  movement. Bees, allays and vexes are not registered.
- **Missing goals.** `LookAtPlayerGoal`, `RandomLookAroundGoal`, `FloatGoal`
  and `BegGoal` are not part of the selector, and head movement keeps its
  existing code. Grazing is fitted around the selector at the eating goal's
  priority, but still starts on VoxelCraft's random chance.
- **Missing avoid goals.** The wandering trader's avoid goals for evokers,
  vindicators, vexes and illusioners are missing, because those mobs are not
  registered.
- **Cramming.** `maxEntityCramming` damage is not modelled. Pushing against
  boats, minecarts and shulkers is not part of mob separation.
- **No graphical play.** Breeding pairs, tempting with wheat, a tamed wolf
  following and teleporting, and rabbits fleeing have not been played.
