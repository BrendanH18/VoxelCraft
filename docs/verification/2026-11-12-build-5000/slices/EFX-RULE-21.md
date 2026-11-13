# EFX-RULE-21: forgive_dead_players and universal_anger for every neutral mob

[← Campaign record](../README.md)

## What was built

- `src/entity/neutral.rs` (new): Java's `NeutralMob` rules shared by every
  implementer. `KINDS` (bee, iron golem, polar bear, wolf, enderman,
  zombified piglin), `follow_range`, `alerts_same_type`, `DEATH_NOTICE`,
  `PIGLIN_UNIVERSAL_ANGER_TICKS`; on `Mob`: `anger_target` (the persistent
  player target), `angry_at_all_players`, `stop_being_angry`, the anger
  timer restart and the universal refresh; on `Entities`:
  - `player_died(player, at)`: `tellNeutralMobsThatIDied`. With
    `forgive_dead_players` on, every neutral mob whose box meets the dead
    player's block inflated by 32, 10, 32 and whose persistent target is
    that player stops being angry. `PlayerSession::die`, the death step
    every adapter shares (desktop host, CLI agent, controller and LAN
    players), calls it.
  - `universal_anger_hit`: `ResetUniversalAngerTargetGoal.start`. Under
    `universal_anger` a player's hit (melee or projectile, creative or
    not, as the goal has no attack check) leaves the mob angry with no
    target; bees, wolves and zombified piglins refresh their kind in the
    `unitCubeFromLowerCorner(pos).inflate(range, 10, range)` box.
  - `neutral_sense`: the player `NearestAttackableTargetGoal` with
    `isAngryAt` for a mob angry at everyone: every other tick, one chance
    in five, the nearest targetable living player within the visibility-
    scaled follow range it can see (endermen need no sight line, tame
    wolves skip their owner, player-made golems never pick) becomes the
    persistent target, as `updatePersistentAnger` keeps it.
  - `neutral_hurt_by`: `HurtByTargetGoal` for wolves, iron golems,
    endermen and zombified piglins, run from the shared hurt-by queue
    (`resolve_hurt_by`), so creative players' hits change no target. The
    zombified piglin alert therefore lands on the next update, as Java's
    target selector does, not inside the hit
    (`tests/java_knockback.rs::a_sweep_angers_zombified_piglins_like_any_player_hit`
    now runs that update and checks the alerted piglin's target).
- Endermen and zombified piglins now hold a persistent player target
  (`Mob::angry_player`): the attacker, the starer (`EndermanLookForPlayerGoal`),
  or the alerting piglin's attacker. An angry one goes only for that
  player; before, it attacked whichever player was nearest. The target
  clears with the anger timer.
- Wolves no longer drop a grudge because its player died; only
  `forgive_dead_players` (or creative/spectator, as `updatePersistentAnger`)
  clears it. Iron golems no longer fall back to the nearest player when
  angry without a target.
- Polar bears forgive a dead player only through `player_died` (before:
  any dead player anywhere, every sense step).
- Piglins (`src/entity/nether.rs`): `maybeRetaliate` under the rule makes a
  hit adult (piglin or brute) angry at the nearest visible targetable
  player (the attacker if none) and `broadcastUniversalAnger` sends every
  adult piglin within 16 blocks after the player nearest it;
  `angerNearbyPiglins` (chests, guarded blocks, gold) does the same. The
  `UNIVERSAL_ANGER` memory (600 ticks) lets a piglin attack the nearest
  visible player even in gold armour (`findNearestValidAttackTarget`;
  brutes don't read it).
- Saves: wolves, endermen and zombified piglins write `neutral_anger`
  (`left`, `player`, `all_players`) when angry; iron golems add
  `anger_all` to their record when angry at everyone; bees store
  `all_players` in `neutral_anger`. All are written only when set, and
  older records load calm, as before.
- `Agent::tick_rules` hands both rules to `Entities`, so a CLI-only world
  applies them as the desktop host does.
- `src/rules.rs` and `docs/game-rules.md`: both rules Implemented.

## Reference sources

- `net.minecraft.world.entity.NeutralMob` (`playerDied`,
  `isAngryAtAllPlayers`, `isAngryAt`, `updatePersistentAnger`,
  `stopBeingAngry`, `forgetCurrentTargetAndRefreshUniversalAnger`).
- `net.minecraft.server.level.ServerPlayer.die`, `tellNeutralMobsThatIDied`.
- `net.minecraft.world.entity.ai.goal.target.ResetUniversalAngerTargetGoal`,
  `HurtByTargetGoal`, `NearestAttackableTargetGoal`.
- `Bee`, `Wolf`, `IronGolem`, `PolarBear`, `Enderman` (`EndermanLookForPlayerGoal`),
  `ZombifiedPiglin`, `TamableAnimal.canAttack`, `IronGolem.canAttack`.
- `net.minecraft.world.entity.monster.piglin.PiglinAi` (`maybeRetaliate`,
  `broadcastUniversalAnger`, `setAngerTarget`,
  `setAngerTargetToNearestTargetablePlayerIfFound`, `angerNearbyPiglins`,
  `findNearestValidAttackTarget`), `PiglinBruteAi.wasHurtBy`,
  `StopBeingAngryIfTargetDead`.
- Fixture `tests/fixtures/java/26.3/neutral_mobs.json` from
  `tools/extract_java_neutral_mob_fixtures.py`, which reads the official
  server jar: the implementers from the class files' super classes and
  interfaces, each one's `ResetUniversalAngerTargetGoal` flag,
  `PERSISTENT_ANGER_TIME` and follow range, the death-notice box and its
  rule gates, and the piglin memory expiries.

## Ids and orderings

None. No block states, item ids, texture layers or appended enum entries.
New save keys only (`neutral_anger`, `anger_all`), written when set.

## Tests

- `tests/game_rules.rs::neutral_mob_rules_match_the_java_fixture`: the
  implementer list both ways against every `MobKind`, alert flags, follow
  ranges, anger timers (bee and polar bear match; the rest are listed
  differences), the death box and the piglin memory.
- `tests/game_rules.rs::forgive_dead_players_reaches_neutral_mobs_through_the_shared_death_step`
  and `universal_anger_reaches_neutral_mobs_through_the_agents_rules`:
  the agent's game rules and `Agent::hurt` death reach a zombified piglin.
- `src/entity/neutral/tests.rs`: per kind, rule on and off, the host
  provokes and a bystander stands nearer: `<kind>_forgives_a_dead_player_only_under_forgive_dead_players`,
  `<kind>_under_universal_anger_turns_on_the_nearest_player`, the death box
  edges, a held grudge attacking the respawned player (and a forgiven one
  not), endermen and zombified piglins attacking their attacker rather than
  a nearer bystander, valid/visible/range/owner limits, same-kind alerts,
  built golems, timer lapse, a mid-anger save and reload per kind (rule on
  and off), and old records; piglins: universal retaliation and rally,
  the gold-armour memory, and chest anger.
- `src/entity/polar_bear/tests.rs`: a dead player stays the target until
  `player_died`.

Run with `VC_REMOTE_RETRIES=60 scripts/remote-check.sh --full` (results in
the hand-off report).

## Differences from Java left in place

- Anger timers: wolves keep the game's 12 s, endermen 30 s (plus the 5 s
  keep-alive near a player), zombified piglins 30 s and iron golems the
  5 s player-hit window, where Java samples 20–39 s for all six. Bees and
  polar bears already used Java's 400–780 ticks.
- Zombified piglins still alert every zombified piglin within 32 blocks at
  once when one is hit, not Java's follow-range box and repeating
  `maybeAlertOthers` timer; bees still use the 8, 6, 8 box from
  `BeehiveBlock` for their own alert instead of `BeeHurtByOtherGoal`'s.
  Wolves have no `HurtByTargetGoal.setAlertOthers` pack alert.
- A neutral mob angry at a mob (not a player) is not modelled for wolves,
  endermen, zombified piglins or golems' player anger; those kinds' timers
  can also be started by non-player damage, which now leaves them angry at
  nobody (Java: at the damaging mob).
- Piglins: `ANGRY_AT` and `UNIVERSAL_ANGER` are not saved (Java saves brain
  memories); a piglin still drops a dead player foe whatever
  `forgive_dead_players` says (`StopBeingAngryIfTargetDead` keeps it when
  the rule is off). The universal pick waits for the piglin's next look
  around (it is due at once after the hit) instead of reading the
  sensor's last `NEAREST_VISIBLE_ATTACKABLE_PLAYER`.
- A zombified piglin killed by the hit still alerts the others (as the
  game did before); Java's dead piglin alerts nobody.

## Unverified

- No hands-on play. Nothing here is visible beyond who a mob chases.
- Projectile hits by players now queue the hurt-by response for wolves,
  iron golems and endermen too (the zombified piglin one moved there from
  `anger_piglins`); no test fires a projectile at those kinds.

## Scripted capture

Nothing new is drawn. A scene that would show the behaviour needs a death
and a respawn, which scripted captures do not drive; the closest is a
zombified piglin beside the player in the Nether:
`cargo run --release -- --data-dir target/screenshots --spawn zombified_piglin,2,~,0 --wait 2 --screenshot target/screenshots/efx-rule-21.png`.
