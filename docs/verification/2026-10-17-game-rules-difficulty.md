# Game rules and difficulty — 2026-10-17

[Roadmap](../roadmap.md) · [Game rules inventory](../game-rules.md) · [Requirement PLY-03d](../requirements.md)

Scope: requirement PLY-03d (experience, difficulty and game rules) on
`agent/game-rules`, based on `77a6885`. The gap it closes is "no inventory of
the 26.3 game rules versus implemented ones": the full registry, a test for the
effect and default of every enforced rule, the cheap missing rules, and the
Java 26.3 difficulty effects for the systems VoxelCraft already has. Reference:
the Java Edition 26.3 server jar, read with `javap -c -p`. Nothing here is
Verified; the rows stay "Implemented, unverified" until the in-game scenarios
below are played.

## Reference classes read

`GameRules` (static initializer: ids, types, defaults, ranges, categories),
`GameRuleRegistryFix` (legacy names, the inverted `disable*` rules, the fire
merge), `Player` (`isInvulnerableTo`, `hurtServer`, `dropEquipment`,
`getBaseExperienceReward`), `DamageSource.scalesWithDifficulty`, the
`damage_type` and `damage_type` tag data (`is_fall`, `is_fire`, `is_drowning`,
`is_freezing`), `FoodData.tick`, `ServerPlayer.tickRegeneration`, `ServerLevel`
(`tick`, `advanceWeatherCycle`, `explode`, `isSpawningMonsters`,
`canSpreadFireAround`, `isAllowedToEnterPortal`, `isSpawnerBlockEnabled`),
`ServerChunkCache.tickChunks`, `MinecraftServer.updateMobSpawningFlags`,
`ChunkMap.anyPlayerCloseEnoughTo`, `FireBlock.tick`,
`NetherPortalBlock.getPortalTransitionTime`, `Entity.teleportToPortalDestination`,
`TntBlock`, `PrimedTnt`, `MinecartTNT`, the TNT dispenser behaviour,
`VehicleEntity.destroy`, `ThrownEnderpearl.tick`, `LivingEntity` (`dropAllDeathLoot`),
`Husk.doHurtTarget`, `AbstractCubeMob.setSpawnSize`, `Pufferfish.playerTouch`,
`Zombie` (`hurtServer`, `killedEntity`), `AbstractSkeleton`, `AbstractArrow.setBaseDamageFromMob`,
`Spider.finalizeSpawn`, `BreakDoorGoal`, `EntityTypes` (`notInPeaceful`) and
`Projectile.mayBreak`.

## Delivered behavior

**Inventory.** [`docs/game-rules.md`](../game-rules.md) lists all 59 registered
rules with id, legacy name, type, default, support level and owning release, and
`src/rules.rs` holds the same table as `JAVA_RULES`. Tests pin the table to the
bytecode (ids, order, defaults, ranges), to the datafixer's legacy names and to
the documentation. 32 rules are enforced (28 Implemented, 4 Partial), 18 await a
system and 9 are server-only or have no single-player effect.

**Names and saves.** Every id, `minecraft:`-prefixed id and legacy camelCase
name works in the console and in saves. Worlds keep saving the camelCase names
they always have, so nothing about the save changed and older builds still read
newer worlds; the 17 rules and `spawnRadius` of the previous registry load
unchanged. `doFireTick` and `allowFireTicksAwayFromPlayer` convert to the new
`fire_spread_radius_around_player` as `GameRuleRegistryFix` does. Rules the
engine lacks are refused with the release that adds them instead of being
stored.

**Damage-type rules.** `fall_damage`, `fire_damage` and `drowning_damage` made
the player immune only at the three call sites that happened to check them.
They now apply wherever the damage lands, as `Player.isInvulnerableTo` does with
the `is_fall`, `is_fire` and `is_drowning` tags: pearl damage, mounted falls,
fireballs and lava, for the host and every agent, with no armor wear when the
hit is ignored. Mobs keep taking the damage, as in Java.

**New enforced rules.** `spawn_monsters`, `spawner_blocks_work`, `tnt_explodes`
(blocks, redstone, flint and steel, chain blasts, dispensers, primed TNT and
TNT minecarts), `water_source_conversion`, `lava_source_conversion`,
`fire_spread_radius_around_player` (distance to the nearest player),
`allow_entering_nether_using_portals`, `players_nether_portal_default_delay`,
`players_nether_portal_creative_delay`, `entity_drops` (boats and minecarts),
`ender_pearls_vanish_on_death`, `send_command_feedback`, and the three explosion
decay rules.

**Differences from Java found and fixed.**

- TNT blasts decayed their drops like creeper blasts. Java's `ServerLevel.explode`
  gives TNT no decay by default (`tnt_explosion_drop_decay` false), while mobs
  and blocks decay at 1/power.
- Creative players waited half a second in a Nether portal; Java 26.3's
  `players_nether_portal_creative_delay` is 0 ticks and survival's is 80.
- With `advance_weather` off the whole weather froze, rain fade included. Java
  freezes only the spell timers. Sleeping through the night also ignored both
  `advance_time` and `advance_weather`; it now sets the clock and clears rain
  only when each is on.
- Peaceful regeneration ignored most of `ServerPlayer.tickRegeneration`: it
  healed and fed regardless of `natural_health_regeneration`, never added
  saturation, and skipped exhaustion entirely. It now heals one point and adds
  one saturation point a second and one food point every half second, all behind
  the rule, with exhaustion spending saturation but never food.
- Starvation on Peaceful could never happen. `FoodData.tick` hurts above 10 health
  on every difficulty but Hard and Normal, whose floors are lower.
- Pufferfish contact and guardian thorns damage was not scaled by difficulty
  although both are mob attacks (`DamageSource.scalesWithDifficulty`).
- A husk's Hunger lasted 140 ticks times the difficulty id (1, 2, 3). Java uses
  the whole part of the local effective difficulty, so a new Easy world gets no
  Hunger, a new Normal one 7 seconds and a new Hard one 14.
- Fire leapt between blocks as if every world were Normal. `FireBlock.tick`
  adds 7 times the difficulty id to the fuel term, so Peaceful gets none and
  Hard 21 instead of 14.
- Slime and magma cube spawn sizes ignored the local difficulty's chance to grow
  a step (`AbstractCubeMob.setSpawnSize`).

## Difficulty effects against Java 26.3

| Effect | Java source | VoxelCraft | Evidence |
|---|---|---|---|
| Damage from mobs, projectiles, explosions, thorns, stings | `Player.hurtServer`, `scalesWithDifficulty` | Peaceful 0, Easy `min(d/2+1, d)`, Hard `1.5 d`; fall, fire, drowning, starvation and magic are not scaled | `java_mob_damage_scaling`; every mob-attack path in `Game::update_mobs` scales |
| Starvation | `FoodData.tick` | Easy hurts above 10 health, Normal above 1, Hard always, Peaceful as Easy | `starvation_floors_follow_java_difficulty`, `peaceful_with_an_empty_bar_starves_only_above_ten_health` |
| Peaceful regeneration | `ServerPlayer.tickRegeneration` | health +1/s, saturation +1/s, food +1 per 0.5 s, behind `natural_health_regeneration` | `peaceful_regeneration_follows_server_player_tick_regeneration`, agent test |
| Peaceful and hostiles | `Mob.checkDespawn`, `EntityType.notInPeaceful` | hostile mobs despawn and stop spawning; piglins stay | existing `peaceful_*` entity tests |
| Local difficulty | `DifficultyInstance` | effective difficulty and special multiplier; monster armor, cube sizes and husk Hunger use it | `effective_difficulty_follows_the_reference_formula`, armor, cube and husk tests |
| Fire spreading | `FireBlock.tick` | `(fuel + 40 + 7 x id) / (age + 30)`, halved when humid | `fire_spreads_more_readily_on_harder_worlds` |
| Zombie villager conversion | `Zombie.killedEntity` | Easy never, Normal 50%, Hard always | existing golem and conversion tests |
| Cave spider poison | `CaveSpider.doHurtTarget` | none on Easy, 7 s Normal, 15 s Hard | existing entity test |

Not modelled, with the track that owns each:

- Zombie door breaking on Hard (`BreakDoorGoal`) and zombie reinforcements
  (`Zombie.hurtServer`, which also needs `isSpawningMonsters`): mob AI goals.
- Skeleton fire rate (20 ticks on Hard, 40 otherwise), arrow inaccuracy
  (`14 - 4 * difficulty id`) and arrow damage (`triangle(0.11 * id, 0.57425)`):
  mob arrows still use one fixed spread and rate. Mob AI and projectile work.
- Spider spawn effects on Hard and skeleton jockeys (`Spider.finalizeSpawn`): the
  roadmap has no owner; mob effects exist, the spawn path does not roll them.
- Chunk inhabited time, so regional difficulty never rises with time spent in an
  area; the clock and moon terms do.
- Neutral mobs refusing to target players on Peaceful (`LivingEntity.canAttack`):
  their hits do no damage, but they still chase.
- Wither skull effect duration and lightning-lit fires on Normal and Hard:
  neither the wither nor lightning fires exist yet. Zombified piglins from
  Nether portals (difficulty-scaled, `isSpawningMonsters`-gated) arrive with
  portal mobs.

## Evidence

Automated, all headless:

- `src/rules.rs`: registry equals the 26.3 initializer (59 ids, order, defaults,
  ranges), legacy names equal the datafixer's, every spelling resolves, the
  save format and old 17-rule worlds, the fire-boolean conversion, refusal of
  unimplemented rules, docs table equal to the registry, damage-type immunity,
  explosion interactions, portal delays and nether entry, immediate respawn.
- `src/world/rule_tests.rs`: water and lava source conversion, explosion decay
  against TNT, chained TNT, random tick speed.
- `src/world/fire.rs`, `redstone.rs`, `automation.rs`: fire radius, fire spread by
  difficulty, TNT from fire, redstone and dispensers.
- `src/entity`: primed TNT and TNT carts, boat and cart drops, dead owners'
  pearls, spawner cages, trader spawning, mob drops, hostile spawn gating, cube
  sizes, husk Hunger.
- `src/simulation`: clock under `advance_time`, weather fade under
  `advance_weather`, Peaceful regeneration, exhaustion and starvation.
- `tests/game_rules.rs`: damage rules and regeneration on a real agent.
- `src/app/console.rs`: feedback suppression.

Commands run in `target/wt/game-rules`, one chain, all steps passing with no
clippy warnings:

```sh
cargo fmt && cargo clippy --release --all-targets -- -D warnings && cargo test --release \
  && cargo clippy --release --no-default-features --all-targets -- -D warnings \
  && cargo test --release --no-default-features
```

`cargo test --release`: 32 test binaries, 1235 passed, 0 failed, 9 ignored.
`cargo test --release --no-default-features`: 31 binaries, 1146 passed, 0
failed, 8 ignored. The machine was shared and heavily loaded, so no timing
or benchmark result is claimed.

## Integration review fix

`ender_pearls_vanish_on_death` also holds under `immediate_respawn`. Pearls
were discarded only by a per-tick check for a dead owner, which an immediate
respawn skipped. The host's and agents' death step now discards the owner's
pearls (`a_dead_players_pearls_vanish_even_with_immediate_respawn`).

## Not verified

- No graphical play: portal delays and the Nether block, `advance_time` and
  sleeping, `immediate_respawn`, `send_command_feedback` and the explosion rule
  wiring run in `Game` and are covered only through the pure functions they call.
- Save and reload of a changed rule through a real world directory is covered
  by the (de)serializer tests, not by a process-restart scenario.
- `block_drops`, `entity_drops` and `mob_griefing` are Partial; the notes in
  [game-rules.md](../game-rules.md) list the Java sites that are not gated yet.
- A ghast fireball's direct hit (6 damage, `fireball` type, fire-tagged) is not
  modelled: VoxelCraft's ghast fireball only explodes, so `fire_damage` does not
  apply to it (blaze fireballs are covered).
