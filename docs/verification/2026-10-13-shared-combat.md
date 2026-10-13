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

## Remaining differences

- Fall distance is approximated as downward velocity while airborne; Blindness
  does not exist, so it cannot block criticals.
- Java's per-item `minimum_attack_charge`, spear/mace/trident and other
  kinetic/piercing weapon components, copper tools and attack-speed attribute
  modifiers from effects/enchantments are not implemented.
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
