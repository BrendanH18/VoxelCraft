# Absorption and golden apples — 2026-10-17

[Roadmap](../roadmap.md) · [Player session](2026-10-13-player-session.md) · [Consumption fixtures](../../tests/fixtures/java/26.3/README.md)

Scope: PLY-03a's open gap, "Absorption missing (golden apple)", on
`agent/absorption` (based on `77a6885`): the Absorption effect and its hearts,
the golden apple's full consume effects, the enchanted golden apple (not
registered before) and the loot tables that kept weighted blanks for both
apples. Reference: Java Edition 26.3 server (SHA-1
`33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`), read with `javap -c -p` from
`target/java-ref/26.3/server-26.3-classes.jar` and its pre-extracted `.javap`
files. Player acceptance (feel, HUD look) belongs to the owner; nothing here
was run graphically.

## Java classes read

`world.effect.AbsorptionMobEffect`, `MobEffects` (`ABSORPTION`), `MobEffect`,
`MobEffectInstance` (`tickServer`), `entity.ai.attributes.Attributes`
(`MAX_ABSORPTION`), `LivingEntity` (`actuallyHurt`, `getAbsorptionAmount`,
`setAbsorptionAmount`, `getMaxAbsorption`, `onAttributeUpdated`, `addEffect`,
`onEffectAdded`, `onEffectUpdated`, `onEffectsRemoved`, `removeAllEffects`),
`entity.player.Player` (`actuallyHurt`), `item.component.Consumables`,
`item.consume_effects.ClearAllStatusEffectsConsumeEffect`, `world.food.Foods`,
`item.Items`, `item.Rarity`, `animal.equine.AbstractHorse` (`handleEating`),
and the `data/minecraft/loot_table/chests/*.json` tables that mention either
apple (extracted from the same jar).

Constants read, not assumed:

- `MAX_ABSORPTION`: `RangedAttribute(0, 0, 2048)`; `ABSORPTION` adds an
  `ADD_VALUE` modifier of 4.0 per level, colour 2445989 (`0x2552A5`),
  beneficial.
- `AbsorptionMobEffect.onEffectStarted`: `setAbsorptionAmount(max(current,
  4 * (1 + amplifier)))`; it runs for every instance given (also one that
  only becomes a hidden effect). `applyEffectTick` returns `absorption > 0`
  and runs every tick, so `MobEffectInstance.tickServer` removes the effect on
  the first tick after the amount hits zero.
- `setAbsorptionAmount` clamps to `0..=getMaxAbsorption()`, and `LivingEntity`
  re-clamps whenever the attribute changes (effect upgraded, expired,
  downgraded to a hidden instance, milk or `removeAllEffects`).
- `LivingEntity`/`Player.actuallyHurt`: damage after armour and magic
  absorption (resistance, protection); `remaining = max(damage - absorption,
  0)`; `absorption -= damage - remaining`; if `remaining == 0` return before
  `causeFoodExhaustion`, otherwise health and exhaustion take it.
- Golden apple: Regeneration II 100 ticks + Absorption I 2400; enchanted:
  Regeneration II 400, Resistance I 6000, Fire Resistance I 6000, Absorption IV
  2400. Both `Foods`: nutrition 4, saturation modifier 1.2 (9.6), always
  edible; default food consume time 1.6 s. The enchanted apple has rarity RARE
  (aqua name) and `ENCHANTMENT_GLINT_OVERRIDE` true. `AbstractHorse` heals 10
  for either apple.
- Loot (weights and counts of 1 unless stated): `simple_dungeon` golden apple 15
  and enchanted 2; `abandoned_mineshaft` 20 and 1; `desert_pyramid` 20 and 2;
  `bastion_treasure` enchanted 2; `bastion_hoglin_stable` golden apple 10;
  `bastion_other` golden apple 9; `igloo_chest` and `underwater_ruin_big`
  golden apple 1 (already present); `stronghold_corridor` golden apple 1.

## What changed

- **Effect and attribute.** `Effect::Absorption` is appended (index 23, id
  `absorption`; `ALL`, masks and icon layers follow the enum order). `Effects`
  now carries the absorption amount, with `max_absorption` derived from the
  active instance, `set_absorption` clamping as Java does, and `absorb`
  returning what is left for health. Adding, removing, clearing, expiring and
  hidden-effect downgrades re-clamp; a spent amount ends the effect on the next
  tick. Mobs share `Effects`, so their direct damage paths call `absorb` too.
- **Damage.** `Vitals::damage` spends absorption after the callers' armour and
  protection reductions and after the hurt-immunity difference. The hit still
  lands (the returned `taken` is the hit before absorption, so immunity,
  knockback and armour wear are unchanged); an entirely absorbed hit costs no
  hunger exhaustion. Poison is magic damage through `hurtServer`, so it
  consumes absorption before health (keeping its "never below half a heart"
  floor). Wither and starvation already go through `damage`.
- **Persistence.** The `effects` save string gains an optional fourth field on
  the active Absorption entry: `absorption:3:1800:11.5`. Player saves, bot
  profiles, LAN snapshots and mob saves already round-trip that string, so the
  amount survives all of them; text without the field restores the starting
  amount, and non-finite or out-of-range amounts are dropped or clamped.
- **Milk, death, respawn.** Milk (`Effects::clear`) and `/effect clear` reset
  the amount; death clears effects (`PlayerSession::die`); respawn is
  `Vitals::default`.
- **HUD.** After the ten health hearts, `ceil(absorption / 2)` golden hearts
  follow in Java's order: columns of ten, rows closing to `max(10 - (rows - 2),
  3)` pixels apart, the last one half when the amount is odd; the armour row
  moves up with the extra rows. Two new texture layers (`HEART_ABSORBING_FULL`
  and `_HALF`, appended at 1759–1760, `tex::COUNT` 1761) hold original golden
  heart art that matches the existing heart tile; the container is drawn under
  each. The low-health shake counts health plus absorption. The effect list
  and top-right icons show Absorption with an original heart glyph in its
  colour.
- **Items.** The golden apple gets both Java effects. The enchanted golden
  apple is registered as item **1047** (not 1041: 1041–1046 are the audit
  potion and splash-potion slots, so 1047 is the first unallocated id; 1048–1049
  stay free; `docs/architecture.md` updated). It has no crafting recipe, an
  original sprite (golden apple with violet sparkles), the glint override in
  hotbar and inventory slots, an aqua name tooltip, creative/search/`give`
  lookup through the item table, and `always_edible`. Horses and donkeys take it
  like a golden apple; the piglin-loved list already resolves it by name.
  Zombie-villager curing stays golden apple only, as in Java.
- **Loot.** The blanks became the real items with the weights above:
  simple dungeon, abandoned mineshaft, desert pyramid, bastion treasure. The
  stronghold corridor gained its golden apple. Review found the pools' totals
  were short of the reference (inflating every present item's odds), so the
  missing entries were restored as weighted blanks: dungeon rare pool copper
  horse armor (total 144), bastion treasure two diamond spears (total 112) and
  the stronghold corridor (total 101, with redstone, leather and the three
  horse armors now real items, `fill_chest_with_blanks` for the copper horse
  armor and disc blanks, and the 26.3 table's counts). The tests
  pin these totals.
- **Fixtures.** `consumption.json` is regenerated by
  `tools/extract_java_consumption_fixtures.py` (now also reading the enchanted
  golden apple) from the pinned server jar; the diff is only the new case.

## Tests

- `simulation::effects`: constants (4 per level, 16 for IV, clamp, `max`
  rule), `absorb` arithmetic, effect ends one tick after the amount is spent,
  expiry/removal/clear/hidden-instance clamps, save round trip with the
  fourth field, legacy text, bad amounts.
- `simulation::survival`: absorbed hits keep immunity and cost no exhaustion;
  the stronger-hit difference; partial absorption; creative immunity;
  Resistance applied before absorption; respawn resets; poison absorbed then
  health (natural regeneration off).
- `item`: both apples' food values, effects, rarity/glint flags, ids, no recipe.
- `render::textures`: golden full/half heart tiles, effect and item icons.
- `world::{dungeon,mineshaft,bastion,stronghold}`: Java weights present and
  both apples actually generated; `temples` fixture test no longer lists the
  enchanted apple as absent.
- Integration: `tests/session_consumption.rs` (fixture-driven effects for both
  apples, always edible, eating gives 4 / 16 absorption, damage then milk),
  `tests/death_and_respawn.rs` (death and respawn), `tests/process_persistence.rs`
  (two processes: Absorption II with 5.5 left survives a disk save/reload).

Verified: `cargo fmt && cargo clippy --release --all-targets -- -D warnings &&
cargo test --release && cargo clippy --release --no-default-features
--all-targets -- -D warnings && cargo test --release --no-default-features`
passes. No graphical run, benchmark or controller session was done.

## Remaining differences

- Visual: no heart blink on damage, no hardcore, poisoned, withered or frozen
  heart variants, no regeneration wobble (all pre-existing for health hearts);
  health stays fixed at 20, so absorption always starts a new row at 10. The
  golden heart and apple art are original, not Java's. The enchanted apple's
  glint is drawn only on 2D slot icons, not on dropped or held 3D items; the
  aqua rarity name shows in the hover tooltip only.
- Poison applies Resistance before absorption, but, as before this change, it
  skips enchantment protection and hurt immunity (Java sends it through
  `hurtServer`); only absorption and Resistance were brought in line.
- *Integrator follow-up, 2026-10-17:* 26.3 loot JSON gives counts as each
  entry's `set_count` **`modifier`** (not `functions`). The stronghold corridor
  counts are restored from it (diamond 1–3, iron 1–5, gold 1–3, redstone 4–9,
  bread and apples 1–3, leather 1–5), and the dungeon's saddle became Java's
  leather (1–5) and its redstone blank real redstone (1–4).
- Statistics (`damage_absorbed`, `damage_taken`) are not tracked.
- `getDamageAfterArmorAbsorb` and the resistance cut still happen in the
  callers/`Vitals::damage` order the game already used (resistance applied
  before the immunity difference); absorption follows them.
- Structures with golden-apple loot that VoxelCraft does not generate (ruined
  portal, woodland mansion, ancient city, trial chambers reward tables) have no
  tables to fill. The stronghold corridor still omits its other absent items.
- Nothing in the game can give a mob Absorption (it is not brewable), so the
  mob damage paths are covered by the shared `Effects` unit tests only.
- The older [player session record](2026-10-13-player-session.md) still says
  Absorption and the enchanted golden apple were missing; this record
  supersedes that.
