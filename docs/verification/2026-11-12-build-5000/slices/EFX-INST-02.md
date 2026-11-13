# EFX-INST-02: effect instances, infinite duration, flags and full `/effect give`

[← Campaign record](../README.md)

## What was built

- `effects::Active` is now the whole `MobEffectInstance`: effect, amplifier,
  ticks, plus `ambient`, `particles` (Java's `visible`/`show_particles`) and
  `show_icon`. `Active::new` gives Java's defaults (not ambient, particles,
  icon), and `with_flags` sets all three.
- Infinite duration is `effects::INFINITE` (`u32::MAX`, Java's -1). It is
  kept as the largest tick count on purpose: the 40-odd places that read
  `ticks` (`> 200` for night vision and the HUD, darkness and blindness fog,
  nausea, the warden's `DARKNESS_DISPLAY_LIMIT`) all read it as a long
  effect, never as one about to end. `Active::is_infinite`, `ends_within`
  and `java_duration` give Java's views of it.
- `Effects::add_instance` is Java's `LivingEntity.addEffect` with
  `MobEffectInstance.update`, and returns whether anything changed.
  `isShorterDurationThan` makes infinite beat every finite duration. The
  instance then takes the newcomer's particle and icon flags, and its
  ambient flag when that clears it or something else changed. The hidden
  chain keeps each instance's own flags, and a hidden instance that
  resurfaces brings them back (`setDetailsFrom`). `Effects::add` still gives
  the default flags.
- `Effects::tick` follows `tickServer`. An infinite effect never counts
  down, and Regeneration, Poison and Wither time their periodic ticks on a
  free-running tick count (`Effects::age`, Java's `tickCount`), incremented
  before the effects tick as in `Entity.commonTick`. Hidden instances under
  an infinite one count down to 0 and stay there, as in Java.
- Ordering: `Active::java_cmp` is `MobEffectInstance.compareTo`, with the
  32147-tick cut-off and Guava's `ComparisonChain`. `Effects::sorted` gives
  the inventory list's natural order. `effects::hud_layout` gives the HUD's
  stable sort on the reversed comparator, skips hidden icons and counts
  each row's columns.
- HUD (`src/app/hud.rs`): icons follow `hud_layout`. An ambient effect gets
  a framed background and never blinks. The blink alpha is now Java's
  formula, using `Mth.cos`'s sine table (`Active::hud_alpha`). The inventory
  effect list is in `compareTo` order, frames ambient effects, and shows the
  level only for amplifiers 1 to 9 (`effects::list_label`).
- Particles (`src/particles/ambient.rs`): swirls come only from effects with
  particles. They are five times rarer when every visible effect is ambient
  (`areAllEffectsAmbient`), and an ambient effect's colour has alpha 38
  (`MobEffect.AMBIENT_ALPHA`).
- `effects::duration_text` is `MobEffectUtil.formatDuration`: `∞`, `mm:ss`
  with a leading zero, and `hh:mm:ss` from one hour (it was `m:ss`). Potion
  tooltips use it too, so long Poison now reads `(01:30)`.
- `/effect give <targets> <effect> [<seconds 1..1000000>|infinite]
  [<amplifier 0..255>] [<hideParticles>]` as in `EffectCommands`. The
  numbers use Brigadier's `readInt` syntax, so `+5` is now rejected as in
  Java. `EffectChange::Give` carries the particles flag, and the instance is
  built as Java's `new MobEffectInstance(effect, duration, amplifier, false,
  particles)`, so a hidden-particle effect also hides its icon. Completion
  offers `infinite` and `true|false`.
- `/effect` feedback is en_us `commands.effect.*`: "Applied effect Speed to
  Host", "Removed effect … from …", "Removed every effect from …". The
  three failures are now errors: give when `update` changes nothing, clear
  of a missing effect, and clear with no effects. `EffectChange::apply` takes
  the target name (the console passes `Host`) and returns `Result`. A CLI
  agent's failed `/effect` is now an `Err`.
- Saves: `id:amp:ticks[:absorption][:a][:p0][:i0]`, ticks `-1` for
  infinite, active entries first and then hidden ones. Loading puts an
  effect's first entry back as its active instance and later ones as its
  hidden chain, so hidden flags no longer merge into the active one. Old
  strings load as before, with default flags.
- `--effect id[,seconds|infinite[,amp[,ambient|hideparticles|hideicon...]]]`
  for scripted scenes (`Active::parse_option`; documented in
  `docs/development.md`).
- Oracle: `tools/extract_java_effect_instance_cases.py` and
  `tests/fixtures/java/26.3/effect_instances.json` (README section added).

## Reference sources read

- `net.minecraft.world.effect.MobEffectInstance` (constructors, `update`,
  `isShorterDurationThan`, `isInfiniteDuration`, `endsWithin`, `mapDuration`,
  `tickServer`, `tickDownDuration`, `downgradeToHiddenEffect`,
  `setDetailsFrom`, `compareTo`, `Details` codec).
- `MobEffect` (`AMBIENT_ALPHA`, particle factory,
  `shouldApplyEffectTickThisTick`), `MobEffects`, `RegenerationMobEffect`,
  `PoisonMobEffect`, `WitherMobEffect`, `InstantaneousMobEffect`,
  `MobEffectUtil.formatDuration`, `util.StringUtil.formatTickDuration`,
  `util.Mth.cos`.
- `world.entity.LivingEntity` (`addEffect`, `tickEffects`,
  `updateSynchronizedMobEffectParticles`, `areAllEffectsAmbient`,
  `removeAllEffects`, `removeEffect`), `Entity.commonTick`.
- `client.gui.Hud.extractEffects`,
  `client.gui.screens.inventory.EffectsInInventory`.
- `server.commands.EffectCommands`, `CommandResponseTracker`.
- Client `en_us.json`: `commands.effect.*`, `effect.duration.infinite`.

Where the brief and the reference differ: Java's `/effect give` has no
separate show-icon argument. `hideParticles` builds the instance with
`showIcon = visible`, so `true` hides the icon too. The brief's "Hero of the
Village is given with particles off, icon on" is a later source (raid
victory), which `Active::with_flags` already supports. The brief's oracle was
to be a new `instance_cases` section in `mob_effects.json`. It became its own
fixture with its own extractor, so `mob_effects.json` and its jar extractor
are unchanged.

## Ids and orderings

No block states, item ids or texture layers. No enum was appended to. Saves
keep storing effect names, with the optional suffixes above.

## Tests

- `tests/java_effect_instances.rs` replays the fixture:
  - 300 `addEffect` sequences (`changed` per add, and the final chain with
    flags).
  - 60 `tickServer` runs from a given tick count (the ticks that applied and
    the chain left).
  - 150 `compareTo` groups (natural and HUD order).
  - HUD alphas, duration texts and inventory level texts.
  - 75 `/effect give` argument lists (parse result or failure).
  - Feedback strings against en_us, and particle constants.
  - The console's `EffectChange::apply`, including the three failures.
  - An agent's `/effect` with `infinite` and `hideParticles`.
  - Infinite effects over 20,000 ticks.
  - The `--effect` option.
  - A `Storage` save and reload of infinite and flagged effects, hidden ones
    included, on a player's level props and a mob, plus a v0.16 string.
- `src/particles/ambient.rs::hidden_and_ambient_effects_follow_java_swirls`:
  hidden particles never swirl, and all-ambient effects swirl at about 1 in
  20 with alpha 38.
- Updated: `src/agent.rs` command tests (4-field `Give`, `Result` from
  `apply`), `tests/java_effects.rs` (4-field `Give`), `src/potion.rs` tooltip
  text and the `duration_text` unit test.
- Commands run: see the final report. The extractor was re-run, and its
  output matches the committed fixture.

## Differences from Java left in place

- `/effect give @s instant_health|instant_damage infinite` heals or hurts
  once. In Java the infinite instance stays, and
  `InstantaneousMobEffect.shouldApplyEffectTickThisTick(tickCount)` fires
  every tick. The finite case's difference is already in EFX-REG-01's
  record.
- Only the executor is a target (`@s`/`@p` or nothing). The `…multiple`
  feedback texts are never used.
- `hideParticles` takes `true` or `false` only. Brigadier also accepts a
  quoted `"true"`.
- `Effects::age` stands in for the entity's `tickCount`. Both restart at 0
  on load. The game counts the ticks its owner ran its effects: every tick
  for players, and every tick a mob is alive. Java's count also runs while a
  mob is dying. Respawn was not compared. Only
  the phase of an infinite Regeneration, Poison or Wither's periodic ticks
  depends on it.
- The ambient HUD and inventory backgrounds are original framed rectangles,
  not Java's ambient sprites.
- No survival source sets `ambient` or hides particles yet. Beacons and
  conduits come in v0.18, the raid's Hero of the Village in EFX-HERO-14 and
  Hard-difficulty spider effects in EFX-SRC-07. Until then the flags come
  from `/effect` in a cheats world, the `--effect` option and saves.
- Ties in `compareTo` (equal colour and duration) keep the game's `Effect`
  order. Java keeps its `HashMap` order. Only Instant Health and Saturation
  share a colour, and Instant Health is never kept.

## Unverified

- Nobody has looked at the ambient frames, the new blink, the reordered HUD
  and inventory list or the faint ambient swirls in a running game. The
  renderer receives the swirl alpha, but whether it draws it translucent
  was not checked. No hands-on run was made.
- LAN clients get the flags through the player state's effect string (the
  same `serialize`). No LAN session was run.

## Scripted captures

HUD with an ambient infinite effect, a hidden-particle effect with its icon,
a hidden icon (Poison, which must not appear) and a blinking one:

```sh
cargo run --release -- --world screenshots --new --seed 42 --data-dir target/screenshots \
    --effect conduit_power,infinite,0,ambient --effect hero_of_the_village,600,0,hideparticles \
    --effect poison,60,0,hideparticles,hideicon --effect speed,8,1 \
    --pose 0,120,0,0,0 --wait 1 --screenshot effects-instances-hud.png
```

Inventory list in `compareTo` order with `∞`, `mm:ss` and level text:

```sh
cargo run --release -- --world screenshots --new --seed 42 --data-dir target/screenshots \
    --effect conduit_power,infinite,0,ambient --effect speed,4000,1 --effect haste,30,0 \
    --effect poison,60,0,hideparticles,hideicon --pose 0,120,0,0,0 --open-inventory \
    --wait 1 --screenshot effects-instances-inventory.png
```
