# MOBN-01: mob equipment slots, drop chances, death drops and save migration

[← Campaign](../README.md)

## What was built

- `src/entity/equipment.rs` (new): Java's `EquipmentSlot` (eight slots in
  `VALUES` order, serialized names, count limits, indexes), `DropChances`
  (0.085 default, preserved above 1.0, guaranteed 2.0) and `Equipment`, a
  full `Stack` per slot with a drop chance per slot. `Mob::equipment`
  replaces `skeleton_bow`, `weapon`, `armor`, `armor_glint` and
  `keeps_equipment`. Rendering (`Equipment::worn`), arrow enchantments, the
  drowned's trident and LAN looks read the slots.
- `Mob.dropCustomDeathLoot` for every mob (`Entities::mob_equipment_drops`,
  replacing the bow-skeleton-only path in `hostiles.rs` and the drowned's
  removal-time drop in `lifecycle.rs`): zero chances skipped, Looting adds
  0.01 per level, Curse of Vanishing blocks the drop, a player kill is needed
  unless the slot is preserved, the wear of a non-preserved drop is rerolled.
  Equipment now drops before the experience, as in
  `LivingEntity.dropAllDeathLoot`, and `Mob.getBaseExperienceReward`'s 1–3
  per still-filled, non-preserved slot is added for hostile mobs.
- Spawning: `Mob.populateDefaultEquipmentSlots` now fills head first
  (`EQUIPMENT_POPULATION_ORDER`); the drowned's override no longer rolls
  armor; zombies, husks and zombie villagers roll Java's iron sword, iron
  spear or iron shovel (5% Hard, 1% otherwise); spawn enchantments are real
  `mob_spawn_equipment` enchantments on the hand (0.25 × special) and each
  piece (0.5 × special), including a drowned's trident.
- Conversions: in-place conversions (zombie to drowned, husk to zombie,
  skeleton to stray, villager to zombie villager) keep the slots; piglin and
  hoglin zombification carry every slot and its chance; villager to witch
  clears it; the zombie villager cure drops preserved slots (new
  `EntityEvent::DropEquipment`) and moves Curse of Binding items into the
  villager's inventory at the slot's index.
- Trial spawner equipment fills head/chest/main hand with the table's items
  (Protection, Projectile Protection and Fire Protection IV on armor; the
  melee table's sword is now held) and sets only those slots to 0.
- Piglins: spawn gold armor into the slots; an admired armor piece is
  equipped as the stack itself with a guaranteed drop; the weapon's death
  drop uses the Java rule and wear.
- Skeleton-trap horsemen get an iron helmet only when the head is empty, and
  the hand and head re-enchanted (`SkeletonTrapGoal.createSkeleton`).
- Saves: per mob `equipment` (slot name to stack string) and
  `drop_chances` (non-default values only, like Java's codec), in both the
  `nether_mobs` and `villagers` records. Old keys load through
  `equipment::Legacy` with the same items and the same drop rule.

## Reference sources read

- `net/minecraft/world/entity/Mob.java` (`dropCustomDeathLoot`,
  `dropPreservedEquipment`, `setDropChance`, `setGuaranteedDrop`,
  `equipItemIfPossible`, `setItemSlotAndDropWhenKilled`,
  `getBaseExperienceReward`, `populateDefaultEquipmentSlots`,
  `getEquipmentForSlot`, `populateDefaultEquipmentEnchantments`,
  `enchantSpawnedEquipment`, `convertTo`)
- `DropChances.java`, `EquipmentSlot.java`, `EntityEquipment.java`,
  `EquipmentUser.java`, `EquipmentTable.java`, `ConversionParams.java`,
  `ConversionType.java`, `ConversionTracker.java`, `LivingEntity.java`
  (`dropAllDeathLoot`, `shouldDropLoot`)
- `monster/zombie/Zombie.java`, `Drowned.java`, `ZombieVillager.java`
  (`finishConversion`), `monster/skeleton/AbstractSkeleton.java`,
  `npc/villager/Villager.java` (`thunderHit`), `AbstractVillager.getSlot`,
  `monster/piglin/AbstractPiglin.java`, `Piglin.java`,
  `monster/hoglin/Hoglin.java`, `animal/equine/SkeletonTrapGoal.java`
- `item/enchantment/EnchantmentHelper.processEquipmentDropChance`,
  `TargetedConditionalEffect.equipmentDropsCodec`
- Data: `enchantment/looting.json`, `vanishing_curse.json`,
  `binding_curse.json`, `loot_table/equipment/trial_chamber*.json`,
  `trial_spawner/**`
- Fixture: `tests/fixtures/java/26.3/mob_equipment.json`, from
  `tools/extract_java_mob_equipment_fixtures.py` (constants from
  `javap -constants`, instruction sequences checked).

## Where the brief, code and reference disagreed

- The brief says cures carry every slot. `ZombieVillager.finishConversion`
  passes `keepEquipment = false`; the reference was followed (preserved
  slots drop, binding-cursed items go to the villager's inventory).
- The code filled monster armor from the boots up and gave drowned armor;
  26.3 fills head first and the drowned override skips armor. Both fixed.
- The drowned's old removal-time drop clamped wear to `max - 1`; Java's
  reroll can leave `damage == max` (the item breaks on next use).
- Old trial mobs never dropped anything (`keeps_equipment`); Java zeroes
  only the slots the table filled. New trial mobs follow Java; migrated
  saves keep the old rule (all chances 0).

## IDs and orderings

No block states, item ids or texture layers. Appended: `EntityEvent::DropEquipment`
(end of the enum). New save keys `equipment` and `drop_chances`; retired
keys (`armor`, `armor_materials`, `armor_glint`, `skeleton_bow`,
`main_hand`, `keeps_equipment`, villagers' `armor` and `glint`) are read
but no longer written. LAN look: `equipment` replaces `armor`/`glint`.

## Tests

- `tests/java_mob_equipment.rs`: slot table both ways and constants against
  the fixture; Looting formula; population order and armor tiers; drowned
  without armor; zombie spawn weapons (rate within five standard errors,
  shovel share 4/6); player kills with and without Looting, drop rate per
  slot within five standard errors; non-player kills drop only preserved
  gear, unchanged; zero chance and Curse of Vanishing never drop; the wear
  distribution's mean and `P(r = 0)` against the reroll model; netherite
  drops its stack; spawn enchantment rates; conversion carrying; a
  hand-written pre-v0.17 `nether_mobs` record (trial skeleton with
  chainmail/iron/enchanted bow, gold-armored zombie, bow-less legacy
  skeleton) and `villagers` record (zombie villager) loaded, checked and
  re-saved; LAN look round trip.
- Unit tests: `equipment.rs` (round trip, preserved drops), `armor.rs`
  (head-first order, Hard skip rate, special multiplier),
  `zombie_villager.rs` (cure drops and binding), `trial/tests.rs` (table
  slots only), `nether.rs` (admired armor stack, piglin weapon wear,
  round trips), `hostiles.rs`, `lightning.rs`, `breeze.rs`, `trident.rs`.
- `tests/entity_storage.rs` normalizes the retired `armor`/`glint` keys.
- Run: `cargo fmt`, then `VC_REMOTE_RETRIES=60 scripts/remote-check.sh --full`
  (clippy with `-D warnings` and every test, default and
  `--no-default-features`), passed on commit `64c36534`;
  `python3 tools/campaign_ledger.py` and `python3 tools/check_doc_links.py`
  pass. The extractor was run against the pinned server jar and its output
  is the committed fixture.

## Differences from Java left in place

- Chainmail armor has no item yet (ids 1550–1553 belong to another v0.17
  slice). A chainmail piece is recorded per slot (`"chainmail"` in saves),
  drawn and counted for experience, but drops nothing and has no real
  enchantments (only its glint). The chainmail slice should turn these into
  stacks in `Equipment::load` and `Equipment::wear`.
- Worn mob armor gives no armor points or toughness and takes no durability
  damage; that needs damage types (`BYPASSES_ARMOR`) from EFX-DMG-01.
- Held weapons do not change a mob's attack damage (Java's attribute
  modifiers), and zombies are not drawn holding them.
- The zombified piglin's `held` weapon, the piglin's `nether.weapon`, wolf
  armor, mount saddles and armor, the fox's mouth, the dolphin's hand and the
  allay's item keep their own fields and drop paths; only the piglin
  weapon's drop rule was aligned. The piglin brute's axe gives no experience
  bonus.
- Copper armor trims on trial armor are not represented.
- Legacy glinted pieces get stand-in enchantments (trial: the table's three
  Protection IV; otherwise a lowest-cost `mob_spawn_equipment` roll seeded
  by the mob), since old saves never recorded them.
- Mobs do not pick up items (`CanPickUpLoot`), so guaranteed drops arise
  only from piglins equipping admired armor.

## Unverified

Nothing was looked at in the running game. Natural spawning rates were
checked through `Entities::spawn`, not through the natural spawner.

## Scripted captures

Equipment looks the same as before; to see armored and armed mobs:

```sh
cargo run --release -- --world screenshots --new --seed 42 --creative \
    --data-dir target/screenshots --screenshot target/mobn01-armor.png \
    --spawn zombie,2,~,0,netherite,glint --spawn skeleton,4,~,0,chainmail \
    --spawn husk,6,~,0,iron --pose 4,80,-6,90,-20 --wait 1
```
