# Game rules

Inventory of the game rules registered by Java Edition 26.3 and what VoxelCraft
enforces, for requirement PLY-03d. The registry, types, defaults and ranges were
read from `net.minecraft.world.level.gamerules.GameRules` in the 26.3 server jar
(`javap -c -p`); legacy names come from `GameRuleRegistryFix`; where each rule
is applied comes from the classes that read it. [`src/rules.rs`](../src/rules.rs)
holds the same table (`JAVA_RULES`) and a test keeps this file identical to it.

## Names and saves

- Java 26.1 renamed every rule to a snake_case registry id and made `/gamerule`
  take ids (`/gamerule keep_inventory true`, with or without `minecraft:`).
  VoxelCraft accepts the id, the namespaced id and the pre-26.1 camelCase name
  everywhere (console, saves, scripts) and completes ids.
- A world saves the `gamerules` property as `legacyName=value;...`, listing only
  non-default values, exactly as it always has, so existing worlds and older
  builds keep working. New rules use their datafixer legacy name.
- `doFireTick` and `allowFireTicksAwayFromPlayer` became one integer rule,
  `fire_spread_radius_around_player` (128: fire ticks within 128 blocks of a
  player, 0: never, -1: everywhere). Old saves convert as Java's datafixer does;
  new saves write `fireSpreadRadiusAroundPlayer`.
- Rules the engine lacks are not stored: `/gamerule raids false` reports that the
  rule is not implemented and names the release that adds it.
- A rule never changes saved IDs. Adding an implemented rule means moving its row
  from Missing to Implemented here, in `JAVA_RULES` and in a test; the test
  `docs_inventory_lists_every_rule_with_its_support_level` fails until they agree.

## Support levels

- **Implemented**: the engine enforces the rule wherever Java does for systems
  VoxelCraft has, and a test shows the effect and the Java default.
- **Partial**: enforced for some systems; the note lists the gaps.
- **Missing**: governs a system VoxelCraft lacks; the release that adds it, from
  the [release plan](roadmap.md#release-plan). "unscheduled" means the roadmap has
  no owner yet and needs an owner decision.
- **Not applicable**: server or multiplayer behaviour, or a reference-only
  experiment, with no single-player effect today.

## Inventory

Types show the Java range for integers. Release is `-` where the rule is
enforced.

| Rule (26.3 id) | Legacy name | Type | Default | Status | Release | Notes |
|---|---|---|---|---|---|---|
| `advance_time` | `doDaylightCycle` | boolean | `true` | Implemented | - | Stops the day clock (`simulation::advance_day`); the moon phase follows the day count. Sleeping skips the night only when on (`Game::update_sleep`, graphical session). |
| `advance_weather` | `doWeatherCycle` | boolean | `true` | Implemented | - | Freezes the rain and thunder spell timers; the levels keep fading as in `ServerLevel.advanceWeatherCycle`. Sleeping clears rain only when on. |
| `allow_entering_nether_using_portals` | `allowEnteringNetherUsingPortals` | boolean | `true` | Implemented | - | Blocks Overworld to Nether travel only (`ServerLevel.isAllowedToEnterPortal`). Java computes the destination portal before the check; VoxelCraft checks first. |
| `block_drops` | `doTileDrops` | boolean | `true` | Partial | - | Gates mined blocks, explosions, beds, furnace, chest and brewing contents and growth drops. Java gates every `Block.popResource` and `popExperience`; the other pop sites (composter, crop and bush harvest) are not audited. |
| `block_explosion_drop_decay` | `blockExplosionDropDecay` | boolean | `true` | Implemented | - | Beds, respawn anchors and end crystals: destroyed blocks drop with chance 1/power (`DESTROY_WITH_DECAY`). |
| `command_blocks_work` | `enableCommandBlocks` | boolean | `true` | Missing | unscheduled | No command blocks, so the rule has nothing to govern. |
| `command_block_output` | `commandBlockOutput` | boolean | `true` | Missing | unscheduled | No command blocks. |
| `drowning_damage` | `drowningDamage` | boolean | `true` | Implemented | - | Player-only, as `Player.isInvulnerableTo` with the `is_drowning` tag; mobs still drown. |
| `elytra_movement_check` | `disableElytraMovementCheck` | boolean | `true` | Not applicable | n/a | Server-side movement validation of remote players. |
| `ender_pearls_vanish_on_death` | `enderPearlsVanishOnDeath` | boolean | `true` | Implemented | - | A dead owner's pearls are discarded each tick (`ThrownEnderpearl.tick`). |
| `entity_drops` | `doEntityDrops` | boolean | `true` | Partial | - | Gates the item and contents of broken boats and minecarts. Falling blocks, leads, paintings and item frames are not gated yet (frames and paintings arrive in v0.10). |
| `fall_damage` | `fallDamage` | boolean | `true` | Implemented | - | Player-only, as `Player.isInvulnerableTo` with the `is_fall` tag (fall, ender pearl, stalagmite); mobs still take fall damage. |
| `fire_damage` | `fireDamage` | boolean | `true` | Implemented | - | Player-only, `is_fire` tag: fire, burning, lava, magma, campfire and blaze fireballs. A ghast fireball's blast is explosion damage and is not blocked (its direct 6-damage hit, which is fire-tagged, is not modelled). |
| `fire_spread_radius_around_player` | `fireSpreadRadiusAroundPlayer` | integer -1..2147483647 | `128` | Implemented | - | Replaces `doFireTick` and `allowFireTicksAwayFromPlayer` (0: no fire ticks, -1: everywhere, else within the radius of a player). VoxelCraft saves it as `fireSpreadRadiusAroundPlayer` and still reads and accepts both older booleans. |
| `forgive_dead_players` | `forgiveDeadPlayers` | boolean | `true` | Missing | v0.15 | Neutral-mob anger after the target dies is not modelled (the Java default, forgiving, is what happens). |
| `freeze_damage` | `freezeDamage` | boolean | `true` | Implemented | - | Powder snow freezing damage to players; off makes them immune to it (PLY-02c). |
| `global_sound_events` | `globalSoundEvents` | boolean | `true` | Not applicable | n/a | Only chooses whether dragon, wither and similar sounds reach players in other dimensions or far away. |
| `immediate_respawn` | `doImmediateRespawn` | boolean | `false` | Implemented | - | Skips the death screen (`GameRules::respawns_immediately`); Hardcore still becomes Spectator. |
| `keep_inventory` | `keepInventory` | boolean | `false` | Implemented | - | Keeps inventory, vanishing items and experience; no orbs drop (`Player.dropEquipment`, `getBaseExperienceReward`). |
| `lava_source_conversion` | `lavaSourceConversion` | boolean | `false` | Implemented | - | Two lava sources over a solid floor make a source when on; off by default, unlike water. |
| `limited_crafting` | `doLimitedCrafting` | boolean | `false` | Missing | v0.18 | Needs recipe unlocking (UX-03c). |
| `locator_bar` | `locatorBar` | boolean | `true` | Missing | unscheduled | No waypoint HUD. The roadmap does not schedule it; owner decision needed. |
| `log_admin_commands` | `logAdminCommands` | boolean | `true` | Not applicable | n/a | Server log only. |
| `max_block_modifications` | `commandModificationBlockLimit` | integer 1..2147483647 | `32768` | Not applicable | n/a | There is no `/fill` or `/clone`. |
| `max_command_forks` | `maxCommandForkCount` | integer 0..2147483647 | `65536` | Not applicable | n/a | There are no functions or `/execute`. |
| `max_command_sequence_length` | `maxCommandChainLength` | integer 0..2147483647 | `65536` | Not applicable | n/a | There are no functions. |
| `max_entity_cramming` | `maxEntityCramming` | integer 0..2147483647 | `24` | Missing | v0.17 | No cramming damage; farm designs rely on it (reference farms, v0.17). |
| `max_minecart_speed` | `minecartMaxSpeed` | integer 1..1000 | `8` | Not applicable | n/a | Registered behind Java's experimental minecart feature flag; VoxelCraft keeps the stable minecart speed. |
| `max_snow_accumulation_height` | `snowAccumulationHeight` | integer 0..8 | `1` | Missing | v0.13 | Snow does not pile up during snowfall (ENV-03). |
| `mob_drops` | `doMobLoot` | boolean | `true` | Implemented | - | Gates the loot and experience of mobs that die (`LivingEntity.dropAllDeathLoot`). |
| `mob_explosion_drop_decay` | `mobExplosionDropDecay` | boolean | `true` | Implemented | - | Creeper and ghast blasts: drops survive with chance 1/power. |
| `mob_griefing` | `mobGriefing` | boolean | `true` | Partial | - | Gates creeper and ghast block damage, fireball fires, the dragon's smashing, and villager and piglin item pickup. Enderman block carrying, zombie door breaking, silverfish, mob farmland and turtle-egg trampling and snow golem trails are not modelled. |
| `natural_health_regeneration` | `naturalRegeneration` | boolean | `true` | Implemented | - | Gates food regeneration and Peaceful regeneration (health, saturation and food). |
| `player_movement_check` | `disablePlayerMovementCheck` | boolean | `true` | Not applicable | n/a | Server-side movement validation of remote players. |
| `players_nether_portal_creative_delay` | `playersNetherPortalCreativeDelay` | integer 0..2147483647 | `0` | Implemented | - | Ticks in a portal for invulnerable players (Creative and Spectator). |
| `players_nether_portal_default_delay` | `playersNetherPortalDefaultDelay` | integer 0..2147483647 | `80` | Implemented | - | Ticks in a portal for survival and Adventure players (80 = 4 s). |
| `players_sleeping_percentage` | `playersSleepingPercentage` | integer 0..2147483647 | `100` | Implemented | - | Share of players who must sleep, rounded up and at least one. |
| `projectiles_can_break_blocks` | `projectilesCanBreakBlocks` | boolean | `true` | Missing | v0.10 | Governs arrows and similar hitting chorus flowers, pointed dripstone and decorated pots (v0.10, v0.16). |
| `pvp` | `pvp` | boolean | `true` | Missing | v1.x | LAN sessions have their own player-versus-player toggle, off unless chosen; the rule joins the registry with multiplayer. |
| `raids` | `disableRaids` | boolean | `true` | Missing | v0.15 | Raids arrive with illagers. |
| `random_tick_speed` | `randomTickSpeed` | integer 0..2147483647 | `3` | Implemented | - | Random ticks per section per tick (`ServerChunkCache.tickChunks`). |
| `reduced_debug_info` | `reducedDebugInfo` | boolean | `false` | Missing | v0.18 | Client-side F3 content; the server jar cannot confirm the exact reduced screen. |
| `respawn_radius` | `spawnRadius` | integer 0..2147483647 | `10` | Implemented | - | Spread of the world spawn search (`PlayerSpawnFinder`). |
| `send_command_feedback` | `sendCommandFeedback` | boolean | `true` | Implemented | - | Quiet successful console commands; failures still print. CLI agents always receive results. |
| `show_advancement_messages` | `announceAdvancements` | boolean | `true` | Not applicable | n/a | There are no advancements (side feature, roadmap section 1.4). |
| `show_death_messages` | `showDeathMessages` | boolean | `true` | Partial | - | VoxelCraft writes death messages to the log only; there is no chat, so the rule has nothing visible to hide. |
| `spawner_blocks_work` | `spawnerBlocksEnabled` | boolean | `true` | Implemented | - | Stops monster cages from ticking (`BaseSpawner.serverTick`); spawn eggs on spawners are not modelled. |
| `spawn_mobs` | `doMobSpawning` | boolean | `true` | Implemented | - | Natural spawning and custom spawners; chunk-generation herds are not gated, as in Java. |
| `spawn_monsters` | `spawnMonsters` | boolean | `true` | Implemented | - | Hostile spawn categories only; animals and water creatures still spawn. |
| `spawn_patrols` | `doPatrolSpawning` | boolean | `true` | Missing | v0.15 | Pillager patrols arrive with illagers. |
| `spawn_phantoms` | `doInsomnia` | boolean | `true` | Missing | v0.13 | Phantoms do not exist yet. |
| `spawn_wandering_traders` | `doTraderSpawning` | boolean | `true` | Implemented | - | Gates the trader spawner. Like Java it also needs `spawn_mobs`. |
| `spawn_wardens` | `doWardenSpawning` | boolean | `true` | Missing | v0.14 | Wardens and sculk shriekers arrive with the deep dark. |
| `spectators_generate_chunks` | `spectatorsGenerateChunks` | boolean | `true` | Missing | v0.11 | VoxelCraft streams around the spectator like any player; ungenerated-chunk limits come with the terrain rewrite. |
| `spread_vines` | `doVinesSpread` | boolean | `true` | Missing | v0.11 | Overworld vines do not spread at all yet. |
| `tnt_explodes` | `tntExplodes` | boolean | `true` | Implemented | - | Without it TNT cannot be primed by fire, redstone, flint and steel, explosions or dispensers, and primed TNT and TNT minecarts fizzle. |
| `tnt_explosion_drop_decay` | `tntExplosionDropDecay` | boolean | `false` | Implemented | - | TNT blasts drop every destroyed block by default (decay is off). |
| `universal_anger` | `universalAnger` | boolean | `false` | Missing | v1.x | Needs more than one player to matter; LAN is a maintenance path until v1.x. |
| `water_source_conversion` | `waterSourceConversion` | boolean | `true` | Implemented | - | Two water sources beside flowing water over a solid floor make a source. |

## Difficulty

Difficulty is world state rather than a rule. The effects VoxelCraft implements,
with the Java class they were read from, are listed in
[the 2026-10-17 verification record](verification/2026-10-17-game-rules-difficulty.md).
