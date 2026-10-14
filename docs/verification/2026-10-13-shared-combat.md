# Shared melee and Java attack strength — 2026-10-13

[Roadmap](../roadmap.md) · [Restructuring](../restructuring.md) · [Reference fixtures](../../tests/fixtures/java/26.3/README.md)

Scope: STR-03, PLY-04 and PLY-05, based on `853e57b` on `work/v0.7-assessment`.
Reference: official Java Edition 26.3 server SHA-1
`33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`. Checks ran on the M5 MacBook Pro.

## Delivered behavior

`PlayerSession::attack` owns player melee against fence knots, minecarts, ghast
fireballs, End crystals/dragon parts and mobs. Desktop, controller/LAN hands and
CLI agents call it; adapters keep sounds, hand animation and the LAN player-vs-
player path. `Outcome` identifies the actor, target, landing point, charge,
critical, kill and a broken tool for presentation.

The previous fixed per-weapon cooldown, which ignored clicks during cooldown,
is replaced by Java's attack-strength charge. Every attack lands. A per-player
ticker advances once per fixed tick (the host in its client tick; every agent,
including controller/LAN hands, in its agent tick) and restarts on an attack, a
miss or a different main-hand item. Damage follows `Player.attack`: base damage
times `0.2 + 0.8 * charge²`, enchantment damage times the charge, a 1.5×
critical only for strong (> 0.9) falling, unsprinted hits on living targets,
sprint knockback (+0.5 Knockback level) and sword sweeps only on strong hits,
with sweeps also requiring ground contact and Java's 2.5×-speed movement limit.
Swept mobs take `(1 + ratio × base + enchantment) × charge`. Food exhaustion
(0.1) and durability apply in Survival only. LAN PvP uses the same charge,
critical, sprint-knockback and sweep rules.

Corrected reference values:

- Entity reach is 3.0 blocks (5.0 in Creative) instead of six, for melee and
  for mob targeting used by entity interactions.
- All 30 registered vanilla tools now use their 26.3 attack-damage and attack-
  speed modifiers plus material bonus (for example axes 7/9/9/7/9/10 damage,
  shovels 1.5 + bonus, hoe speeds 1–4, fists speed 4). Previously every
  non-Netherite tool attacked twice a second and axes/shovels had wrong damage.
- CLI melee previously had no criticals, no exhaustion and six-block reach.

### Crosshair attack indicator

`PlayerSession::attack_indicator` reports what Java's default crosshair attack
indicator shows: while the charge (`getAttackStrengthScale(0)`) is below 1, a
16-pixel bar under the crosshair fills in 17ths; when aimed at a living target
with a full charge and a weapon slower than 5 ticks (so not bare hands), a
small full mark replaces it. The desktop HUD draws it (not while drawing a bow,
in screens or as a spectator) with original sprites. Source limit: the client
`Gui` class is not in the pinned server artifact, so this follows the
documented client behavior rather than extracted bytecode. Controller views do
not show it yet.

## Evidence

`tools/extract_java_combat_fixtures.py` derives `combat.json` from `Items`,
`ToolMaterial`, `Attributes` and `Player` bytecode and checks the
`Player.attack`, `baseDamageScaleFactor`, `getCurrentItemAttackStrengthDelay`
and `isSweepAttack` constants. `tests/shared_combat.rs` (seven tests) checks
every registered tool against the fixture, reference reach in Survival and
Creative, damage at several charge levels, critical gating, charge restarts on
miss and item change, Survival-only exhaustion/wear and that the CLI adapter
produces the same damage as the session for an equally charged attack. The
existing sweep regression now charges the agent first.

The required formatting, strict release Clippy and release tests passed:
**912 passed, zero failed, three ignored**. The device-independent configuration
passed strict Clippy and **830 tests, zero failed, two ignored**. The real
desktop LAN harness passed **13/13** checks, including PvP with sword and bow.

## Copper tools and nuggets

Item ids 1050..=1069 are reserved for Java 26.3 copper equipment: the five
copper tools take 1050..=1054 and the copper nugget 1055. `Tier::Copper` is
appended (it is not saved by ordinal). From `combat.json`: durability 190,
speed 5.0, enchantability 13, attack bonus 1 (the axe deals 7 at -3.2, the hoe
-1 at -2.0), stone's harvest level (`incorrect_for_copper_tool` holds the same
tags as stone's), repair with copper ingots. Recipes follow the reference tool
patterns, `copper_nugget` (one ingot to nine) and `copper_ingot_from_nuggets`.
Iron, golden and copper tools, armor and horse armor now smelt and blast to one
nugget (0.1 experience), as in `*_nugget_from_smelting`/`_blasting`; Java's
spears, chainmail and nautilus armor in those lists are not registered.
Copper armor (1056..=1059) follows `ArmorMaterials.COPPER`: durability
multiplier 11 (121/176/165/143), defense 2/4/3/1, enchantability 8, no
toughness or knockback resistance, copper-ingot repair and the reference armor
recipes; it has its own worn-armor sheet (array layers 17-18) and melts to a
copper nugget. Monster armor now rolls Java 26.3's tiers (start `nextInt(3)`
over leather/copper/gold, three 0.1087 climbs toward chainmail, iron and
diamond); the later-piece skip chance is Java's 25%, or 10% on Hard (read from
`Entities::difficulty`, set by each update; covered by a statistical test).
`combat.json` pins every armor material and slot durability unit, and
`tests/copper_tools.rs` checks every tool and armor material against it plus
copper harvesting, recipes, repair and nugget smelting. Loot-table copper gear,
the copper equip sound, spears and ordinary-player acceptance remain open;
icons use the procedural sprites in copper colour.

Validation with copper: release **972 passed, zero failed**; headless **890
passed, zero failed**. The desktop LAN harness failed once on "PvP on" (the
sword hit missed: 20.0 -> 20.0, the bow then hit) and passed 13/13 on the next
two runs. The players stand about 2.7 blocks apart against a 3.0 survival reach,
and any mob crossing the ray blocks a PvP hit, so the check is timing- and
mob-sensitive; it is recorded as intermittent, not fixed.

## Remaining differences

- Blindness is not yet represented, so it cannot block criticals.
- Java's per-item `minimum_attack_charge`, spear/mace/trident and other
  kinetic/piercing weapon components are not implemented. Haste and Mining
  Fatigue attack-speed modifiers are now applied; Java 26.3 has no attack-speed
  enchantment definitions. Copper tools and armor are now
  registered (see below); spears are not.
- Damage to non-living targets (crystals, dragon parts) uses the scaled base
  plus scaled generic enchantment bonus; dragon-part damage routing is the
  existing fight model.
- The HUD has no attack-strength indicator yet; first-person hand cooldown
  animation is unchanged.
- Container, vehicle and block-interaction ranges still use the adapters' older
  six-block constant; other item use and containers remain outside the shared
  session, and `Game::puppet` still swaps state.

State: **Partial** for STR-03, PLY-04 and PLY-05. Ordinary keyboard/controller
play, visual/audio feedback and performance comparison remain unverified.

## v0.7 defect fixes

Criticals now read the session's accumulated fall distance instead of vertical
velocity, including the LAN PvP path. The pinned `Player.canCriticalAttack`
bytecode and regenerated `combat.json` establish its predicates. Regression
cases cover momentarily zero velocity after a fall, teleport/reset, water,
climbing and passengers. Ordinary-player acceptance remains outstanding.

Critical-fix checks: required fmt/strict release Clippy/release tests passed
(977 passed, zero failed, four ignored); fixture regeneration matched exactly.

Haste and Mining Fatigue scale attack speed by their reference total multipliers
(+10%/-10% per level, float constants widened to doubles). Charge, damage and
the indicator read the modified attribute, clamped to 0..1024. The fixture
extractor scans all reference enchantment data: none modifies attack speed.
Tests cover each effect, combined modifiers, zero speed and charge timing.

Attack-speed-fix checks: required fmt/strict release Clippy/release tests passed
(978 passed, zero failed, four ignored); expanded fixture reproduced exactly.
