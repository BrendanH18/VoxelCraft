# EFX-REG-01: effect registry from the jar, and the 11 missing effects

[← Campaign record](../README.md)

## What was built

- `tools/extract_java_effect_fixtures.py` reads the pinned server jar's
  `MobEffects` static initializer with javap and writes
  `tests/fixtures/java/26.3/mob_effects.json`: all 40 effects in registry order,
  with id, field, class, category, colour, `isInstantaneous`, attribute
  modifiers, particle override, sound on added, blend durations, extra
  constructor arguments (including the Weaving, Oozing and Infested int
  providers read from their lambdas), en_us names from the pinned client and
  `MobEffectCategory` tooltip formatting. It fails on any instruction form it
  doesn't know.
- `Effect` gains Health Boost, Glowing, Levitation, Conduit Power, Hero of the
  Village, Raid Omen, Wind Charged, Weaving, Oozing, Infested and Breath of
  the Nautilus. Each has a Java id, an en_us name, the `MobEffects` colour, a
  category and an original 8×8 icon glyph. `/effect`, saves, LAN player state
  and the HUD all handle them.
- Three-way `Category` (beneficial, harmful, neutral), with
  `Effect::category`, `is_beneficial` and `is_harmful`. Potion tooltips stay
  red only for harmful effects. The HUD puts non-beneficial effects, neutral
  ones included, in its second row (`Hud.extractEffects` uses `isBeneficial`).
  Before, Bad Omen and Trial Omen showed in the beneficial row.
- `Effect::modifiers` holds the 12 attribute modifiers `MobEffects` registers.
  `Effect::is_instantaneous` is Java's flag, which also covers Saturation.
- Fixes: Wither's colour is 0x736156, which also changes its swirls, tipped
  arrows and clouds (it was 0x352A27). Dolphin's Grace is spelled with
  en_us's plain apostrophe.
- `/effect give` follows `EffectCommands.computeDurationInTicks`. For
  instantaneous effects (Instant Health, Instant Damage, Saturation) the
  number counts ticks and defaults to 1; for other effects it counts seconds
  and defaults to 30. `EffectChange::Give` now carries ticks.
- The console completes effect ids after `/effect give|clear [@s|@p]`, with or
  without `minecraft:`.
- `Effects::mask` and `Target::effects` are now `u64`, so 24 bits are free. The
  mask exists only in memory: saves and LAN carry Java id strings
  (`Effects::serialize`), so no format or LAN version changes. The brief's
  worry about the LAN replica format doesn't apply.

## Reference sources read

- `net.minecraft.world.effect.MobEffects` (source and bytecode), `MobEffect`
  (`setBlendDuration`, `isBeneficial`, `withSoundOnAdded`),
  `MobEffectCategory`, `InstantaneousMobEffect`, `SaturationMobEffect`.
- `net.minecraft.world.entity.ai.attributes.Attributes` (attribute ids).
- `net.minecraft.client.gui.Hud.extractEffects` (HUD rows).
- `net.minecraft.world.item.alchemy.PotionContents.addPotionTooltip`
  (tooltip colour by category).
- `net.minecraft.server.commands.EffectCommands.computeDurationInTicks`.
- Client `assets/minecraft/lang/en_us.json`, `effect.minecraft.*` (40 keys).

Where the brief and the reference differ: the brief lists Saturation as an
ordinary effect, but Java's `SaturationMobEffect` extends
`InstantaneousMobEffect`. The game keeps applying Saturation every tick (Java
does the same for an instance with a duration). The new
`is_instantaneous` flag drives only the `/effect` duration rule.

## Ids and orderings

No block states, item ids or texture layers. Appended `Effect` and
`Effect::ALL` after `Invisibility`, in this order: HealthBoost, Glowing,
Levitation, ConduitPower, HeroOfTheVillage, RaidOmen, WindCharged, Weaving,
Oozing, Infested, BreathOfTheNautilus. Their icons follow in the item icon
array after the animated frames (`effect_icon_layer`, `item_layers` and
`texel` all derive from `Effect::ALL`). Saves store names, as before.

## Tests

- `tests/java_effects.rs`:
  - The registry matches the fixture in both directions, comparing id, name,
    colour, category, instantaneous flag and modifiers.
  - Neutral category and tooltip colour.
  - HUD row.
  - Wither colour.
  - Dolphin's Grace name.
  - The implemented modifier behaviour (speed, attack speed, strength and
    weakness, absorption, luck) uses the table's amounts.
  - Save and reload: a v0.16 string loads, and every new id survives a
    `Storage` level-props save and a mob save.
  - The u64 target mask holds every effect.
  - `/effect` durations follow Java.
  - Every effect is given and cleared through the CLI agent's `/effect`, and
    completion works.
- `src/render/item_sprites.rs::every_effect_icon_draws_in_its_colour`: every
  effect has a lit icon in its colour, and each new glyph differs from every
  other effect's glyph.
- `src/agent.rs` command parsing tests updated to the tick durations, plus
  Saturation and Instant Health defaults.
- Run: the implementer handed the slice over before its first compile
  finished. The review re-ran the extractor (its output is identical to the
  committed fixture), compiled and tested it (clippy clean, nothing to fix),
  checked that `java_effects` fails when Health Boost's colour in
  `Effect::colour` changes by one, and ran
  `scripts/remote-check.sh --full`, `tools/check_doc_links.py` and
  `tools/campaign_ledger.py` on the final tree.

## Differences from Java left in place

- The 11 new effects do nothing yet beyond being held, shown, saved and timed.
  Health Boost doesn't raise max health, Glowing doesn't outline, Levitation
  doesn't lift, and so on. Their behaviour and survival sources belong to the
  later EFX slices (Conduit Power in v0.18). Until then they can only be
  obtained with `/effect give` in a cheats world.
- The fixture's particle overrides (Trial Omen, Raid Omen), added sounds (the
  three omens) and blend durations (Nausea, Darkness) are recorded but not
  compared, because the game doesn't model them per effect.
- `/effect give instant_health N` (or `instant_damage`) applies once,
  whatever N is. Java keeps the instance for N ticks and
  `InstantaneousMobEffect.shouldApplyEffectTickThisTick` applies it on each
  of them, so Instant Health heals N times (repeat Instant Damage is held
  back by invulnerability ticks). The default of one tick matches Java.
- The HUD orders icons by `Effect` order, not by Java's
  `MobEffectInstance` ordering.
- Icons are original procedural glyphs, not Java's sprites.

## Unverified

Nobody has looked at the new icons or the moved Bad Omen and Trial Omen
icons in a running game. No hands-on run was made.

## Scripted captures

HUD rows (beneficial: Health Boost, Conduit Power, Hero of the Village, Breath
of the Nautilus; second row: the harmful ones plus neutral Glowing and Raid
Omen):

```sh
cargo run --release -- --world screenshots --new --seed 42 --data-dir target/screenshots \
    --effect health_boost,600 --effect conduit_power,600 --effect hero_of_the_village,600 \
    --effect breath_of_the_nautilus,600 --effect glowing,600 --effect raid_omen,600 \
    --effect levitation,600 --effect wind_charged,600 --effect weaving,600 \
    --effect oozing,600 --effect infested,600 --pose 0,120,0,0,0 --screenshot effects-hud.png
```

Inventory effect list (names and times):

```sh
cargo run --release -- --world screenshots --new --seed 42 --data-dir target/screenshots \
    --effect health_boost,600 --effect raid_omen,600 --effect weaving,600 \
    --open-inventory --pose 0,120,0,0,0 --screenshot effects-inventory.png
```

Note: with Levitation given, the effect icon shows but the player doesn't float.
