# Shared right-click use chain — 2026-10-13

STR-03 step: one actor-owned use chain, `PlayerSession::use_held`
([src/session/interact.rs](../../src/session/interact.rs)), replaces the desktop's
`Game::place_block` body and the CLI's separate `place` handlers. Controller and
LAN hands reach it through the desktop path (still via `Game::puppet`); the
headless LAN server reaches it through CLI commands. Nothing here is Verified.

## Behavior

The order is the desktop chain retained from 0.6, which tracks Java's entity
interaction, then item use, then block use:

1. **Entities.** Boosting a ridden mount, opening ridden or targeted vehicle
   storage (boats only while sneaking), animal items (leads, name tags, food,
   taming), mounting animals and carts, placing carts and boats, villagers
   (skipped while sneaking), sheep dye/shears and other mob items.
2. **Items**, unless the targeted block takes right-clicks (containers, beds):
   buckets and bottles, ender pearls (one-second cooldown), splash potions, eyes
   of ender (Overworld), snowballs/eggs and fishing rods.
3. **Blocks** (not while riding): eyes into portal frames, redstone components,
   composters, workstation/container screens, the dragon egg, cake, beds,
   right-click harvests, doors and gates, then (build modes only) flint and
   steel, shovel paths, axe stripping, hoe tilling, pumpkin carving, bone meal,
   and finally validated placement.

Screens, beds, vehicle storage and trading are reported as outcomes; adapters
open their own screens or apply sleeping rules. Tool breakage is reported with
the outcome.

## Behavior changes

- **CLI order and coverage.** `place` previously tried held items first (armor,
  rod, throwables, pearl, potion, eye) and lacked vehicle storage, villagers,
  sheep, doors, harvests, the dragon egg, flint and steel, tilling, paths,
  stripping, carving and bone meal. It now runs the shared chain. Armor equipping
  stays first, as on the desktop. Aiming at a screen block, bed, vehicle storage
  or villager returns an error that names the matching command (sneak to build
  against a screen block). Buckets, bottles, pearls and splash potions keep their
  previous cooldown exemptions.
- **Riders.** The desktop now lets riders use items such as buckets and
  throwables (Java's `Item.use` has no vehicle check), as the CLI already did.
  Block use while riding remains unavailable on every path.
- **Ignition sound** plays at the lit cell instead of the clicked block.

Adventure-mode gating is unchanged: redstone components and composters still
require a build-capable mode. Java lets Adventure players use blocks. That is a
recorded difference, not a fidelity exception.

## Evidence

`tests/shared_use.rs` covers screen outcomes and sneaking placement, door
toggling through the session and the CLI, flint and steel on fire and TNT with
Survival-only wear, tilling and paths, and the CLI's screen-block error. The
existing bucket, placement, mining, combat and CLI suites pass unchanged except
for the intended cooldown exemptions. Release tests: **965 passed, zero failed**;
headless: **881 passed, zero failed**; desktop LAN harness **13/13** on one Mac. Ordinary keyboard/controller play, LAN play and performance
acceptance remain unverified.
