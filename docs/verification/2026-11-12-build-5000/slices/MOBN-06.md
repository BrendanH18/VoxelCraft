# MOBN-06: zombie reinforcements, leader zombies and random spawn attributes

[← Campaign](../README.md)

## What was built

- `src/entity/zombie.rs` (new): `Attrs`, the spawn-rolled attribute
  modifiers and reinforcement charges every mob now carries
  (`Mob::attrs`), and the attribute values computed from them with Java's
  operation order (`ADD_VALUE`, then `ADD_MULTIPLIED_BASE`, then
  `ADD_MULTIPLIED_TOTAL`) and `RangedAttribute` bounds:
  `Mob::reinforcement_chance`, `Mob::follow_range`, the leader's max health
  (through `Mob::max_health`) and the knockback-resistance bonus (through
  `Mob::knockback_resistance`).
- `Mob.finalizeSpawn`: every mob spawned through `Entities::spawn` rolls the
  follow-range `random_spawn_bonus` (`triangle(0, 0.11485)`,
  `ADD_MULTIPLIED_BASE`). The follow range feeds the navigation budget
  (`Mob::caps`) for every mob.
- `Zombie.finalizeSpawn` and `handleAttributes` for zombies, husks, drowned,
  zombie villagers and zombified piglins: the `CanBreakDoors` roll
  (0.1 × special), reinforcement base `nextDouble() × 0.1F` (0 for zombified
  piglins), knockback bonus `nextDouble() × 0.05F`, follow bonus
  `nextDouble() × 1.5 × special` kept when above 1 (`ADD_MULTIPLIED_TOTAL`),
  and with chance 0.05 × special a leader: +0.5–0.75 reinforcement chance,
  max health ×(2–5), healed to full except on conversion, `CanBreakDoors`.
  The rolls are drawn after all existing spawn rolls, so earlier rolls keep
  their order.
- `Zombie.hurtServer`: any accepted hit on a zombie-family mob flags a call
  (`Mob::reinforce_call`, with the attacker for player melee).
  `Entities::call_reinforcements` runs each update after `resolve_hurt_by`:
  the zombie needs a target (its reinforcement target, its chase target, the
  melee attacker, a recent player hit, or a hunted villager/golem/turtle),
  Hard difficulty, `nextFloat() < chance` and spawn_mobs and spawn_monsters
  on. Up to 50 tries at per-axis offsets `nextInt(7..40) × nextInt(-1..1)`;
  the spot must pass the kind's placement and `REINFORCEMENT` spawn rules
  (on the ground and dark for zombies and zombie villagers, also under open
  sky for husks, not on nether wart for zombified piglins, dark water for
  drowned without their depth and biome rolls), have no living player
  within 7 blocks, no mob or player box and no block collision at the block
  corner Java uses, and be dry unless drowned. The reinforcement is the
  caller's kind, goes through `Entities::spawn` (finalizeSpawn with
  `REINFORCEMENT`), is sent after the player target (`setTarget`), takes the
  -0.05F callee charge; the caller's charge drops by 0.05 each time. The loop
  is bounded to 50 tries per call.
- Zombie targeting (`Mob::zombie_target`): zombies, husks, drowned and
  zombie villagers search their follow range (35 plus bonuses) instead of
  the 24 blocks other monsters use, and keep a reinforcement target while it
  stays within follow range.
- Conversions: an in-place kind change makes the old rolls stale (Java's
  `convertTo` makes a fresh entity). Zombie to drowned and husk to zombie
  re-run `handleAttributes(CONVERSION)` without the spawn follow bonus;
  villager to zombie villager runs the spawn follow bonus and
  `handleAttributes(CONVERSION)`; a leader made by conversion is not healed.
  Natural zombie villagers keep the zombie rolls they spawned with.
- Saves: an optional `attributes` record per mob (kind, both follow bonuses,
  knockback bonus, reinforcement base, caller and callee charges, leader
  bonuses, `can_break_doors`), read before health so a leader keeps health
  above 20. Records without it load with base values. Out-of-range values
  read as none.

## Reference sources read

- `world/entity/monster/zombie/Zombie.java` (`hurtServer`,
  `finalizeSpawn`, `handleAttributes`, `randomizeReinforcementsChance`,
  `createAttributes`, `canSpawnInLiquids`, the drowning `ConversionTracker`
  callback), `ZombifiedPiglin.java` (`randomizeReinforcementsChance`,
  `createAttributes`, `checkZombifiedPiglinSpawnRules`), `Drowned.java`
  (`checkDrownedSpawnRules`, `canSpawnInLiquids`), `Husk.java`,
  `ZombieVillager.java`
- `world/entity/Mob.java` (`finalizeSpawn`, `createMobAttributes`,
  `convertTo`, `checkMobSpawnRules`), `monster/Monster.java`
  (`checkMonsterSpawnRules`, `checkSurfaceMonstersSpawnRules`),
  `SpawnPlacements.java`, `SpawnPlacementTypes.java`,
  `EntitySpawnReason.java`, `ConversionType.java`
- `world/entity/ai/attributes/Attributes.java`, `AttributeInstance.java`
  (`calculateValue`), `DefaultAttributes.java`
- `util/RandomSource.java` (`triangle`), `util/Mth.java` (`nextInt`),
  `level/EntityGetter.java` (`hasNearbyAlivePlayer`),
  `server/level/ServerLevel.java` (`isSpawningMonsters`)
- Fixture `tests/fixtures/java/26.3/zombie_attributes.json`, from
  `tools/extract_java_zombie_attribute_fixtures.py` (javap of the pinned
  server jar; instruction sequences checked, constants read from them).

## Where the brief, code and reference disagreed

- The brief lists Zombie and its subclasses; the reference's
  `DefaultAttributes` gives `spawn_reinforcements` to exactly zombie, husk,
  drowned, zombie villager and zombified piglin. Zombified piglins roll the
  leader bonus too, so a leader zombified piglin calls reinforcements on Hard.
- The knockback bonus uses `Mob.RANDOM_SPAWN_BONUS_ID`, not a zombie id.
- The code used 24 blocks for every monster's target search; Java's
  `NearestAttackableTargetGoal` uses the follow range (35 for the zombie
  family). Changed for zombies, husks, drowned and zombie villagers only.

## Ids and orderings

No block states, item ids or texture layers. No enum variants appended.
New optional save key `attributes` in the mob record (`nether::save_mob`).

## Tests

- `tests/java_zombie_attributes.rs`: carriers in both directions with
  `zombie::calls_reinforcements`, every constant against the fixture, the
  follow-range bases, and the leader rate and health range of 20 000
  zombies spawned through `Entities::spawn` on Hard.
- `src/entity/zombie/tests.rs`: roll ranges and rates of
  `handleAttributes` (leaders 5%, follow bonus 1/3 at special 1, base mean
  0.05, leaders healed); no follow bonus at special 0.5; zombified piglins'
  zero chance; the follow triangle's mean and variance on spawned cows;
  operation order and clamps; the call's gates (Normal, spawn_monsters off,
  spawn_mobs off, no target, zero chance); a player melee hit through
  `Entities::melee`; placement over 3000 seeded calls (offsets 0 or 7–40 on
  each axis, both signs and both ends seen, block corners, never within 7
  blocks of a player, and a second player removes the spots near it);
  flooded ground refused; drowned in dark water and husks under the sky;
  the reinforcement count after 20 hits at chance 0.6 against an exact model
  of the charges; follow-range targeting at 30 and 36 blocks and a
  reinforcement keeping a sneaking target it would not have noticed; the
  drowning re-roll; save, reload, pre-v0.17 records and bad values.

## Differences from Java left

- Reinforcements are called on the update after the hit (one tick late),
  once per update however many hits landed.
- Attacker identity is known only for player melee. For arrows, tridents
  and potions the nearest targetable player stands in while the 5-second
  player-hit credit lasts; a mob attacker (golem, wolf) counts only when the
  zombie is already hunting a villager, golem or turtle, and then the
  reinforcement gets no target.
- `isUnobstructed` checks living mobs and players only (not boats, carts or
  armor stands). Husks' `canSeeSky` is skylight 15, as natural spawning uses.
- `CanBreakDoors` is rolled and saved but the game has no door-breaking goal.
- Zombie targeting still has no line-of-sight check and keeps the 12-block
  height window of the existing monster chase; Java's search box is 4 high.
- Curing a zombie villager does not roll the villager's spawn follow bonus
  (the cure path has no random source); its zombie rolls are dropped.
- Inhabited time is not tracked, so the special multiplier tops out at
  0.875 on Hard and leaders are at most 4.4% of Hard spawns.
- Baby zombies are not rolled as chicken jockeys (not in the game).

## Unverified

- No hands-on play: reinforcements arriving during a fight on Hard, and
  their pathing to the player, were not watched in the running game.
- The drowning re-roll test calls the functions directly rather than
  through `Mob::update`'s conversion path.
- Frame time with many zombies calling at once was not measured; each call
  is at most 50 bounded tries.

## Scripted capture

Nothing new is visible: reinforcements and leaders look like ordinary
zombies, as in Java, and the scripted-scene flags set no difficulty or
damage. A zombie can be placed for a look with
`cargo run --release -- --world screenshots --new --seed 42 --data-dir target/screenshots --spawn zombie,0,~,4 --screenshot zombie.png --pose 0,80,0,90,-20`.
