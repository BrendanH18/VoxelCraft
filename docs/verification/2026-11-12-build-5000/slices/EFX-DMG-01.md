# EFX-DMG-01: typed damage types and tags

[← Campaign record](../README.md)

## What was built

- `src/simulation/damage.rs`: `DamageType` (all 51 Java types, with
  `message_id`, `exhaustion`, `scaling`, `effects`, `death_message_type`),
  `DamageTag` (all 36 tags, `#tag` references expanded in `DamageType::is`),
  and `Cause { kind, text }`: the Java type plus the death text VoxelCraft
  shows. Every damage source at the base commit is a named `Cause` constant
  (`Cause::KNOWN`) or `Cause::mob(text)` for a mob's melee hit.
- The rules read tags, not text:
  - armor (`after_armor`, host `damage_player_armored`, agent `hurt` and
    `hurt_knocked`): reduced and worn only outside `#bypasses_armor`; a
    zero wear no longer wears the agent's armor by one point;
  - `Vitals::damage`: creative stops all but `#bypasses_invulnerability`,
    Fire Resistance refuses `#is_fire` at the entry point, Resistance skips
    `#bypasses_effects`/`#bypasses_resistance`, and a hit costs its type's
    food exhaustion (0.0 or 0.1) instead of a flat 0.1;
  - `enchant::protect`/`protection_factor`: the five protection
    enchantments' fixture requirements (`#is_fire`, `#is_explosion`,
    `#is_projectile`, `#is_fall`, never `#bypasses_invulnerability`), and no
    protection for `#bypasses_effects` or `#bypasses_enchantments`;
  - `DamageImmunity::blocks` (the four player damage game rules);
  - `PlayerSession::totem_saves` (`Vitals::killed_by`, not saved).
- `EntityEvent::PlayerHit`/`PlayerSting`/`Explosion`/`Shoot` and arrows carry
  a `Cause`. `Hurts` gains `hot_floor` and `campfire`, so standing in fire,
  on magma and in a campfire are `in_fire`, `hot_floor` and `campfire` with
  Java's messages ("went up in flames", "discovered the floor was lava");
  lingering burning stays `on_fire` ("burned to death").
- The agent's survival tick now hurts in the host's order through `hurt`, so
  lava, fire, magma, campfire and stalactite damage go through armor for CLI,
  controller and LAN players as they do for the host.
- Compatibility: `damage_player*`, `Agent::hurt*`, `Vitals::damage`,
  `protect`, `DamageImmunity::blocks` and `Arrow::aimed` take
  `impl Into<Cause>`. A bare `&'static str` resolves through `Cause::legacy`:
  the named cause with exactly that text, else `mob_attack`.

## Reference sources

- `data/minecraft/damage_type/*.json` (51), `data/minecraft/tags/damage_type/*.json` (36),
  `data/minecraft/enchantment/{protection,fire_protection,blast_protection,projectile_protection,feather_falling}.json`,
  client `assets/minecraft/lang/en_us.json` (`death.attack.*`, `death.fell.*`).
- `net.minecraft.world.damagesource.DamageType`, `DamageTypes`, `DamageSources`
  (which type each source gets; `explosion(entity, cause)` picks
  `player_explosion` when both are set, so creeper and ghast-fireball blasts
  are `player_explosion`; `fireball(owner == null)` is `unattributed_fireball`),
  `DamageSource.getLocalizedDeathMessage`, `DamageSource.scalesWithDifficulty`.
- `LivingEntity.hurtServer` (Fire Resistance, cooldown, totem),
  `getDamageAfterArmorAbsorb`, `getDamageAfterMagicAbsorb`,
  `checkTotemDeathProtection`, `doHurtEquipment`; `Player.hurtServer`,
  `Player.isInvulnerableTo`, `Player.actuallyHurt` (exhaustion);
  `Entity.isInvulnerableToBase`; `Explosion.getIndirectSourceEntity`;
  `LargeFireball.onHit`; `HealOrHarmMobEffect` (dragon's breath cloud is
  `indirect_magic`).

## Ids and orderings

No block states, item ids or texture layers. New enums `DamageType` and
`DamageTag`, in Java id (alphabetical) order; neither is saved. Saves keep the
death text in the player's `death` field as before, so old saves load
unchanged. No existing enum was appended to.

## Tests

`tests/java_damage_types.rs` (fixture `tests/fixtures/java/26.3/damage_types.json`,
extractor `tools/extract_java_damage_fixtures.py`):

- types and tags against the jar in both directions, every field and membership;
- every protection enchantment, level and type against the fixture's requirements;
- through `Vitals::damage`, the agent's `hurt` and survival tick: armor by
  `#bypasses_armor` for all 51 types (reduction and wear), Resistance,
  Fire Resistance, creative, totems and exhaustion for all 51 types,
  Projectile Protection for every type, the ghast blast and blaze fireball
  protections, the four game rules for all 51 types plus one agent hit each,
  and the agent's lava (armored) and magma (`hot_floor` death) ticks;
- every named `Cause` says one of its type's Java death messages, or is on a
  listed wording difference that the test checks still differs.

Unit tests were updated in `src/simulation/survival.rs`, `src/enchant.rs`,
`src/rules.rs`, `src/session/hands/tests.rs` and the entity tests that match
causes. Review (2026-11-13): `scripts/remote-check.sh --full` passed on
b377529d (clippy and every test, default and `--no-default-features`); the
extractor re-run from the pinned jars reproduces the fixture byte for byte;
removing `fall` from `DamageType::exhaustion` and disabling the Fire
Resistance check made `damage_types_match_the_jar_both_ways`,
`food_exhaustion_follows_the_damage_type` and
`fire_resistance_refuses_every_is_fire_type_at_the_entry_point` fail.

## Differences from Java left in place

- Wording: nine named causes keep VoxelCraft's text (listed with reasons in
  `WORDING` in the test): the ghast blast, llama spit, a player's wind
  charge, guardian spikes, Thorns, bee sting, pufferfish, goat ram and the
  dragon's breath cloud. Their types are Java's. Java builds messages from
  entity names and fall locations; VoxelCraft keeps one text per cause.
- Splash and arrow-borne Instant Damage are typed `magic` (Java:
  `indirect_magic` when thrown); both bypass armor, so the player takes the
  same damage. The guardian beam stays one `mob_attack` hit carrying its
  magic part (Java: an `indirect_magic` hit then a `mob_attack` hit).
- Primed TNT never records who lit it, so its blast is `explosion` with the
  text "was blown up by TNT" (Java: `player_explosion` when lit by a player,
  otherwise "blew up").
- Explosion block interaction and fire (`ExplosionSource::of_cause`,
  `GameRules::explosion_fire`) still read the blast text. In Java they come
  from the exploding entity, not the damage type, so they are out of this
  slice's scope.
- Difficulty scaling still uses the existing per-event `Difficulty::mob_damage`
  calls; `DamageType::scaling` is recorded but not yet read.
- Mob damage (`Mob::take_damage`) carries no damage type, so the mob-side
  tags (`no_anger`, `panic_causes`, `witch_resistant_to`, `wither_immune_to`,
  `bypasses_wolf_armor`, armor-stand tags, `no_knockback`, `no_impact`,
  `bypasses_shield`, `damages_helmet`, `is_freezing`'s ×5 for freeze-hurts-extra
  mobs) are compared as data only. `ItemStack.canBeHurtBy` (netherite armor
  ignoring fire wear) is not modelled.
- A blaze fireball still sets a player alight when its hit is refused
  (Fire Resistance, a shield, the hurt-immunity window). Java's
  `SmallFireball.onHitEntity` ignites first and restores the old fire ticks
  when `hurtServer` returns false; VoxelCraft sends `Ignite` regardless. With
  Fire Resistance the burning does no damage, so only the flames show.
- Creative players now take `#bypasses_invulnerability` damage in
  `Vitals::damage`, as in Java, but the game already matched there: `/kill`
  passes no creative flag and nothing deals `out_of_world` damage yet, so it
  is not counted in the ledger.
- Host thorns (`app/lan.rs`) now goes through armor, as the agent's already
  did; the host path is in the binary crate and is compiled, not tested.

## Unverified

The desktop host paths in `src/app/` (tick damage loop, `damage_player*`,
event handling, explosions) build in CI but no test drives them. Nobody has
played this. No frame-time run was made; the change adds tag matches per hit,
not per frame.

## Scripted capture

Nothing new is drawn. The only visible change is the death screen's text
(for example "Player went up in flames" after dying in fire,
"Player discovered the floor was lava" on magma); no scripted option opens
the death screen, so it is seen only in play.
