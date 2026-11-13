# MOBN-26: the endermite

[← Campaign](../README.md)

## What was built

- `MobKind::Endermite` (saved as `endermite`), appended at the end of the
  enum and of `MobKind::ALL`: a 0.4 x 0.3 monster with 8 health, movement
  speed 0.25, 2 attack damage ("was slain by an endermite"), follow range
  16, 3 experience and no loot (`entities/endermite` has no pools). It is
  hostile (despawned on Peaceful, never spawned naturally), floats in water
  (`FloatGoal`) and counts as an arthropod.
- `src/entity/endermite.rs` (new): its numbers, its saved `State`
  (`life`, the mob it was hurt by) and the entity-level passes:
  - `pearl_endermites`: for each `PearlLanded` of the update,
    `ThrownEnderpearl.onHit`'s 5% roll for a living player thrower, then
    `spawn_mobs` and `spawn_monsters` and not Peaceful, puts an endermite
    where the thrower stood before teleporting, facing as they looked.
  - `endermite_upkeep`: `aiStep`'s `life` counts one a tick unless
    persistent (a name tag sets it, as `NameTagItem` does) and the endermite
    is discarded (no death, drops or experience) at 2400, persistent or not;
    a mob foe is dropped when it dies or leaves the follow range.
  - `endermen_hunt` and `enderman_strike`: `Enderman`'s
    `NearestAttackableTargetGoal<Endermite>(mustSee)` on the shared hunt
    path: an enderman not angry at a player walks at the nearest endermite
    within 64 blocks that it can see and hits it for 7 within its attack box.
    A stare still angers a hunting enderman, which then drops the hunt. A
    bite from an endermite no longer makes an enderman angry at players
    (the game's enderman anger is player-only; Java's `HurtByTargetGoal`
    would pick the endermite it already targets).
- `src/entity/mob/endermite.rs` (new): the goals. The player target comes
  from the shared `NearestAttackableTargetGoal` helper (`java_target`, must
  see, 16 blocks); `HurtByTargetGoal().setAlertOthers()` turns the hurt
  endermite and every untargeted endermite in
  `unitCubeFromLowerCorner(pos).inflate(16, 10, 16)` on the attacker, a
  player (through `resolve_hurt_by`) or a mob (an enderman's bite, through
  `resolve_mob_hits`). `MeleeAttackGoal(1.0)` paths at the target and bites
  every 20 ticks when the attack box (`DEFAULT_ATTACK_REACH`) touches it and
  it is in sight. Without a target it uses the shared random stroll at
  `ground_speed(0.25)`.
- Bane of Arthropods: `MobKind::creature` now follows
  `#sensitive_to_bane_of_arthropods` (endermite added, and the bee, which
  was missing). Its `post_attack` effect is new for every arthropod: a
  player's direct hit, and each sweep hit, gives Slowness IV for
  `round(uniform(1.5, 1.5 + 0.5 × (level − 1)) × 20)` ticks.
- Original visuals and sound: a four-segment model (`model/hostiles.rs`)
  in original colours with `EndermiteModel`'s segment proportions and sway,
  drawn without a separate head; two portal particles a tick in its box
  (`particles/ambient.rs`, shared with the enderman); `Voice::Endermite`, a
  procedural chitter for ambient, hurt and death, in the Hostile category.
- Saves: endermites are saved with the mob records (so `lifetime` keeps
  counting across a restart) with a `lifetime` key; old records without it
  load as 0.

## Reference sources read

- `net/minecraft/world/entity/monster/Endermite.java` (constructor,
  `registerGoals`, `createAttributes`, `getMovementEmission`, sounds,
  save data, `tick`, `setYBodyRot`, `aiStep`, `checkEndermiteSpawnRules`)
- `projectile/throwableitemprojectile/ThrownEnderpearl.java` (`onHit`,
  `isAllowedToTeleportOwner`)
- `monster/Enderman.java` (`registerGoals`, `createAttributes`),
  `monster/Monster.java`, `Mob.java` (`createMobAttributes`,
  `isWithinMeleeAttackRange`, `getAttackBoundingBox`), `EntityTypes.java`
  (`ENDERMITE`), `Entity.java` (`MovementEmission`, `getRandomX/Y`)
- `ai/goal/MeleeAttackGoal.java`, `ai/goal/target/HurtByTargetGoal.java`,
  `item/NameTagItem.java`, `server/level/ServerLevel.isSpawningMonsters`
- `item/enchantment/effects/ApplyMobEffect.java`, `util/Mth.randomBetween`,
  `world/entity/player/Player.java` (post-attack effects for the target and
  sweep hits)
- `client/model/monster/endermite/EndermiteModel.java` (proportions and
  `setupAnim` only)
- Data: `loot_table/entities/endermite.json`,
  `tags/entity_type/arthropod.json`,
  `tags/entity_type/sensitive_to_bane_of_arthropods.json`,
  `enchantment/bane_of_arthropods.json`
- Fixture: `tests/fixtures/java/26.3/small_mobs.json`, from
  `tools/extract_java_small_mob_fixtures.py` (pinned server jar, `javap`).

## Where the brief, code and reference disagreed

- The brief listed four sound cues including the step. `Endermite` returns
  `MovementEmission.EVENTS`, so Java never plays its step sound (only step
  game events, which the shared sculk path already emits). Three cues are
  voiced.
- The bee is in `#arthropod` but the game treated it as `Other` for Bane of
  Arthropods; fixed, since the two-way tag test requires it.
- The game had no Bane of Arthropods Slowness at all; added for every
  arthropod, as the brief's "damage and slowness" needs.
- The brief's LAN index 78 assumed other mobs first; the endermite is
  appended after the breeze, and nothing hard-codes its position.

## Ids and orderings

No block states, item ids or texture layers. Appended: `MobKind::Endermite`
(end of the enum and `MobKind::ALL`, LAN index = its position),
`Voice::Endermite` (end of the enum and `Voice::ALL`), mob save key
`lifetime`.

## Tests

- `src/entity/endermite/tests.rs`: registration, attributes, experience,
  loot, lifetime and pearl constants and the enderman's numbers against the
  fixture; goal speeds against the fixture's goal list; the bane tag both
  ways against `MobKind::creature`, the damage per level and 200 seeded
  Bane V hits for the Slowness amplifier and duration range; 20,000 pearl
  landings (5% within five standard errors; none on Peaceful, with
  `spawn_mobs` off, with `spawn_monsters` off or for a dead thrower); 300
  real thrown pearls through `Entities::update`; the 2400-tick discard of an
  untouched endermite beside a name-tagged one, and the discard of a
  persistent one whose saved life has run out; save and reload; a visible
  targetable player bitten for 2 at most every 20 ticks and none through a
  pen wall; the hurt-by alert box; an enderman killing an endermite in sight
  (which turns on it) but not hunting through a wall or while angry.
- `src/particles/ambient.rs::endermites_emit_two_portal_particles_within_their_box`,
  `src/entity/model/hostiles.rs::the_endermite_turns_as_one_body_and_its_segments_sway`,
  `src/audio/tests.rs` (the voice renders clean buffers and plays as Hostile).
- `tests/id_map.rs` checks the appended `MobKind` keeps distinct positions
  and names.
- Run: `scripts/remote-check.sh --full` (clippy and every test, default and
  `--no-default-features`) passed on the final tree; `tools/check_doc_links.py`
  and `tools/campaign_ledger.py` pass. The `id_map` example was not run: this
  slice took no id ranges.

## Differences left

- `checkEndermiteSpawnRules` is not implemented: nothing in the game spawns
  an endermite through spawn rules (no biome lists it, there is no
  endermite spawner or spawn egg yet; `endermite_spawn_egg` stays a v0.20
  registry gap). The fixture records the rule (5 blocks) for the slice that
  adds such a path.
- Target memory: a mob foe is dropped when it dies or is more than 16 blocks
  away, without `TargetGoal`'s unseen-ticks memory; the enderman picks the
  nearest visible endermite each tick instead of `NearestAttackableTargetGoal`'s
  random 1-in-10 start and keeps no unseen memory. Enderman chase speed and
  its knockback on endermites are the game's existing enderman values.
- `ClimbOnTopOfPowderSnowGoal`, `LookAtPlayerGoal` and
  `RandomLookAroundGoal` have no effect on the drawn endermite (no head); the
  water avoidance of the stroll is the shared stroll's.
- The pearl roll cannot see whether the thrower is asleep (Java's
  `isAllowedToTeleportOwner`); the game's own teleport ignores a sleeping
  host, but an endermite may still be rolled for one.
- Sweep hits apply Bane's Slowness like the main target; enchanted arrows
  and thrown tridents don't (Bane is a melee enchantment, so Java has no
  other path either).
- The parrot's `entity.parrot.imitate.endermite` is not added.

## Review

A second agent re-ran `tools/extract_java_small_mob_fixtures.py` on the
pinned server jar (output identical to the committed fixture), checked the
size, eye height, attributes, experience, `MAX_LIFE`, pearl chance and
conditions, loot table, both tags, Bane's effects and the enderman's goals
and attributes by hand against the decompiled source and data, and saw
`cargo test --release --lib endermite` fail with `PEARL_CHANCE` set to 0.06.
It repaired:

- `ATTACK_REACH` was `2√2 − 2` (0.828427…), not Java's
  `Math.sqrt(2.04F) - 0.6F` (0.828285…); now the Java value, with a test
  that computes it from the float literals.
- The hurt-by alert compared positions with the search box; Java's
  `getEntitiesOfClass` tests box overlap, so an endermite whose own box
  reaches in (up to 0.2 further out horizontally, 0.3 further below) is
  alerted too. The alert test now covers both edges.
- The ledger's `loot:entities/endermite` row repeated the mob row (the
  brief lists the empty loot table under `mob:endermite`) and was removed.

## Unverified

No one has played it. The model, particles and voice were not looked at or
listened to in the running game; the AI was checked only through the
entity update in tests, not in a loaded world with real terrain.

## Scripted captures

```sh
# An endermite in front of the camera (creative, so it doesn't attack).
cargo run --release -- --data-dir target/screenshots --world endermite --new --seed 42 --creative \
    --spawn endermite,2,~,0 --pose 0.5,92,-1.5,90,-35 --wait 1 --screenshot endermite.png
# An enderman and an endermite at night: the enderman turns on it.
cargo run --release -- --data-dir target/screenshots --world endermite-hunt --new --seed 42 --creative \
    --spawn enderman,4,~,0 --spawn endermite,9,~,0 --time 0.75 \
    --pose 6.5,92,-6,90,-20 --wait 3 --screenshot endermite-hunt.png
```

In survival, throwing ender pearls leaves an endermite one landing in twenty.
