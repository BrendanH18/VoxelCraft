# ITEM-02-EXPERIENCE-BOTTLE: Bottle o' Enchanting

[← Campaign](../README.md)

## What was built

- Item `Item::EXPERIENCE_BOTTLE` (id 1554), named "bottle o' enchanting",
  stack 64, drawn with the enchantment glint (`has_glint`) and a yellow
  tooltip name (`Item::is_uncommon`, Java's `Rarity.UNCOMMON`). A new
  original procedural icon, `Sprite::ExperienceBottle` (the drinkable
  bottle's flask with green and sky-blue swirls and sparkles).
  `Item::from_name("experience_bottle")` resolves it, so trades, chest loot
  and `--give experience_bottle` find it.
- Throwing: `thrown::Kind::ExperienceBottle`. Using the item (session
  `use_held` → `survival_items::throw_held`) spends one bottle in survival
  and launches it from 0.1 below the eye along Java's
  `Projectile.shootFromRotation` heading (`thrown::heading_from_rotation`: the
  look direction with 20 degrees added to the vertical part only), with
  `getMovementToShoot`'s triangle noise (`thrown::shot_movement`) at power
  0.7, plus the thrower's motion. It plays the existing projectile throw
  sound (`Sound::Bow`, `NEUTRAL`) and emits `projectile_shoot`.
- Flight: gravity 0.07 blocks a tick², applied before the 0.99 (water 0.8)
  drag, as in `ThrowableProjectile.tick` (`Kind::gravity`).
- Burst (`ThrownExperienceBottle.onHit`): on a block, a mob or a player
  (its thrower only after a quarter second; creative players too, but not
  spectators or the dead, as `Player.isPickable`) it emits `PotionSplashed` with
  colour `0x385DC6` (splash particles and the glass break sound already used
  by splash potions) and the new `EntityEvent::ExperienceBurst` with
  `3 + nextInt(5) + nextInt(5)` points. The entity layer turns it into orbs
  via `Entities::spawn_xp_toward` / `XpOrb::toward`
  (`ExperienceOrb.awardWithDirection`): Java's orb sizes, the random pop
  flipped onto the side of the direction, the orb set half its box out
  along it. The direction is the hit face's normal (computed by
  `face_hit` from the step that entered the solid cell, with the exact
  crossing point) or minus the bottle's motion for mobs and players. Mobs
  take no damage or knockback (the class has no `onHitEntity`). Breezes
  still turn bottles back, as for every non-wind projectile.
- Dispenser: `Entities::dispense_thrown` shoots the bottle 0.7 out and
  0.1 up from the centre at power 1.375 and uncertainty 3; droppers still
  drop it. The automation output carries the bottle with the existing
  "shot" flag (saved as before).
- Survival path: the master cleric's offer (3 emeralds → 1 bottle, 12 uses,
  30 xp, 0.05 multiplier) is renamed to the item's name and now resolves;
  the jar-derived chest tables (ancient city, pillager outpost, shipwreck
  treasure) now roll it instead of a blank.
- LAN: the replica decoder maps kind 5 to the bottle; the host encodes
  `kind as u8`. Drawn as a small blue-green cube like the other thrown
  items.
- `entity::thrown` is now a public module (tests name the kind).
- Fixture `tests/fixtures/java/26.3/thrown_items.json` from
  `tools/extract_java_thrown_fixtures.py`.

## Reference sources read

- `net/minecraft/world/item/ExperienceBottleItem.java` (`use`,
  `asProjectile`, `createDispenseConfig`), `ProjectileItem.java`
  (`DispenseConfig.Builder`), `Items.java` (`EXPERIENCE_BOTTLE`)
- `world/entity/projectile/throwableitemprojectile/ThrownExperienceBottle.java`,
  `ThrowableItemProjectile.java`, `world/entity/projectile/ThrowableProjectile.java`
  (`tick`, `applyInertia`, `getAirDrag`), `Projectile.java`
  (`shootFromRotation`, `getMovementToShoot`, `spawnProjectileFromRotation`)
- `world/entity/ExperienceOrb.java` (constructor with direction,
  `awardWithDirection`, `tryMergeToExisting`, `getExperienceValue`)
- `world/entity/EntityTypes.java` (bottle 0.25 × 0.25, orb 0.5 × 0.5),
  `world/entity/OminousItemSpawner.java`
- `core/dispenser/DispenseItemBehavior.java`, `ProjectileDispenseBehavior.java`,
  `world/level/block/DispenserBlock.java` (`getDispensePosition`)
- `client/renderer/LevelEventHandler.java` (events 1053 and 2002),
  `world/level/block/LevelEvent.java`
- Data: `villager_trade/cleric/5/emerald_experience_bottle.json`,
  `tags/villager_trade/cleric/level_5.json`,
  `loot_table/chests/{ancient_city,pillager_outpost,shipwreck_treasure}.json`;
  client `sounds.json` (`entity.experience_bottle.throw` is `random/bow`),
  `en_us.json`, `items/experience_bottle.json`

## Where the brief, code and reference disagreed

- The brief says in-flight bottles are saved with the other thrown entities.
  Thrown entities (snowballs, eggs, wind charges) are not saved in this game
  (`examples/id_map.rs`: "saves: not saved"), so neither are bottles; a
  bottle in flight at save time is lost. Stacks are saved by numeric id as
  before (tested).
- The existing `potion::aim_up` used for splash potions rotates the whole
  look direction by 20 degrees. Java's `shootFromRotation` adds the offset
  to the vertical part only and then normalizes, which differs at steep
  pitches. The bottle uses the Java form; splash potions were not changed.
- The trade table named the offer "bottle o enchanting" (with an alias in
  `tests/java_registry.rs`); both now use the item's name and the alias is
  gone.

## IDs and orderings

- Item id 1554 (the slice's range). No block states or texture layers.
- Appended: `entity::thrown::Kind::ExperienceBottle` (LAN index 5 today, by
  declaration order) and its arm in `entity/replica.rs`'s decoder; one item
  icon after the fermented spider eye (`all_items`, `sprite_index` via the
  new `fermented_spider_eye_icon()`, `icon_count`); `Sprite::ExperienceBottle`
  at the end of `Sprite`; `EntityEvent::ExperienceBurst` (not saved or sent).

## Tests

- `tests/java_thrown_items.rs`: registration against the fixture (stack,
  glint, rarity, names, creative palette, icon, stack save round trip);
  400 launches at each of 15 look directions inside Java's noise bound
  around `shootFromRotation`, with the mean on it; a throw through
  `PlayerSession::use_held` (spends one, about 20 degrees of climb) and
  through the CLI agent's `place`; the orb scatter and flip for three
  directions; the set of Java chest tables naming the bottle equals the
  game's, and 20,000 rolls of each table give its weight share within five
  standard errors and every count in Java's range.
- `src/entity/thrown.rs`: 40 ticks of flight equal Java's recurrence
  (gravity, then drag) to 1e-9; bursts on a floor (top face, normal +Y),
  a wall (west face, −X), a pig (no damage or knockback, direction minus the
  motion) and another player; 2,000 experience totals all in 3..=11, every
  total seen, mean 7, middle share 0.2; the rotation heading at level and
  straight-down aim.
- `src/world/automation.rs`: a powered dispenser shoots one bottle (none
  dropped), 500 shots sit at Java's position with velocity inside the
  1.375 × 3 noise bound, both ways; a dropper drops it.
- `src/entity/villager.rs`: 50 master clerics all offer the bottle on the
  fixture's terms, and level 5 has the fixture's two offers.
- `tests/java_registry.rs` passes with the gap removed.

## Review (2026-11-13)

A second agent checked the slice against the 26.3 source and data. The
extractor reran to an identical fixture, and the trade, loot weights and
rolls, registration, entity size, dispense constants, gravity, experience
roll and colour were checked by hand. Raising the throw power to 0.75 made
`thrown_bottles_follow_java_shoot_from_rotation` fail. Repairs:

- Bottles only broke on targetable players, so creative players let them
  pass. Java's `Projectile.canHitEntity` uses `Player.isPickable`, which
  excludes only spectators; bottles now break on any living player who is
  not a spectator (`experience_bottles_break_on_creative_players_but_not_spectators`).
- A bottle landing at a shallow angle could cross a floor boundary and a
  column boundary in the same 0.2-block substep. `face_hit` took the last
  boundary, so it reported the next column's side face, a point just under
  the floor and orbs thrown sideways. It now walks the crossings in order and
  stops at the first solid cell, the floor's top face as in Java's block
  clip (asserted in the creative-player test's passive cases).
- New tests: `bursting_bottles_leave_experience_orbs_in_the_world` runs
  `Entities::update` until a bottle breaks and checks the orbs and the splash
  event; `entity/replica.rs` `every_thrown_kind_round_trips` covers the LAN
  decoder arm.
- Ledger: the separate `experience_roll` row was part of the burst
  behaviour (one `onHit`) and was folded into it; 10 rows remain.

## Differences from Java left in place

- The throw sound reuses the game's shared projectile throw pitch range
  (0.42–0.62); Java's is `0.4 / (nextFloat * 0.4 + 0.8)` (0.33–0.5). The
  same approximation already applies to snowballs and eggs.
- Orbs from a burst do not try `tryMergeToExisting` (Java's 1-in-40 id
  merge into an existing orb of the same value); the game's periodic orb
  merge still joins them later. Orbs are not moved out of blocks
  (`unstuckIfPossible`); the game's orb update lifts an orb that overlaps a
  solid block.
- Hit tests use the game's 0.2-block substeps and a point projectile, not
  Java's 0.25 box swept against block shapes and inflated entity boxes; a
  mob or player hit bursts at the substep point inside the box rather
  than Java's clip point on it. The thrower is ignored for a quarter
  second, standing in for Java's `leftOwner` box test.
- Rendering: a small cube, not the item sprite on a billboard.
- No dispenser launch sound (level event 1002): the game plays none for any
  dispenser shot yet.
- The particle burst reuses the splash potion's, at the hit point, not
  Java's 8 item-break plus 100 spell particles at the block's bottom centre.

## Unverified

- Not launched or played: no screenshot, hands-on run or frame-time
  measurement. Glint and yellow name were not seen on screen.
- LAN: only the replica round trip is tested (`entity/replica.rs`
  `every_thrown_kind_round_trips`); no guest was run. The host-side use path
  is the shared session.
- The controller and desktop paths call the same `PlayerSession::use_held`
  as the tested session; they were not driven directly.

## Scripted captures

```sh
# The icon in the inventory (glint) and a hotbar of bottles.
cargo run --release -- --data-dir target/screenshots --world screenshots --new --seed 42 \
    --give experience_bottle,64 --open-inventory --screenshot bottle-inventory.png
# Every icon, including the new one, to target/item_icons.png.
cargo test --release icon_sheet -- --ignored
# Thrown bottles need a use input; `--drop experience_bottle,8` shows the dropped items.
cargo run --release -- --data-dir target/screenshots --world screenshots --new --seed 42 --creative \
    --drop experience_bottle,8 --wait 1 --screenshot bottle-dropped.png
```

There is no scripted flag to throw a projectile, so the flight and burst
need a hands-on run.
