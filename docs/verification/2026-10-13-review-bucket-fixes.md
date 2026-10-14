# Bucket and bottle review fixes — 2026-10-13

[Roadmap](../roadmap.md) · [Restructuring](../restructuring.md) · [Reference fixtures](../../tests/fixtures/java/26.3/README.md)

Scope: STR-03 shared bucket/bottle actions, PLY-04 and UX-04/05 input-path
parity. Based on `ab568fc`. Reference: official Java Edition 26.3 server SHA-1
`33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`, inspected with `javap -c -p`. No
game binaries or disassembly are committed. Checks ran on the M5 MacBook Pro.

## Findings and fixes

| # | Finding | Bytecode check | Result |
| --- | --- | --- | --- |
| 1 | CLI `Place` caught `BUCKET`, `LAVA_BUCKET`, `GLASS_BOTTLE` and fish buckets before mounting, animal use and block use | Confirmed. `ServerPlayerGameMode.useItemOn` runs the block's `useItemOn`/`useWithoutItem` before `Item.use` unless the player sneaks with an item. Entity interaction (`mobInteract`) is a separate, earlier packet path | Fixed |
| 2 | Adventure players could use fluid buckets | Confirmed. `BucketItem.use` returns FAIL unless `Level.mayInteract` and `Player.mayUseItemAt` (`Abilities.mayBuild`, else the `can_place_on` component). `BottleItem.use` checks only `mayInteract`. `Bucketable.bucketMobPickup` and cow milking have no build check | Fixed |
| 3 | Pouring replaced a targeted grass or fern in place, and plain water buckets ignored water-holding blocks | Confirmed. `use` targets `hit.relative(face)` unless the hit block is a `LiquidBlockContainer` and the content is water. `emptyContents` uses `canBeReplaced(fluid)` (replaceable, or not solid). It retries the hit face when sneaking or when the first cell is unplaceable, evaporates Nether water, calls `placeLiquid` for containers, and otherwise calls `destroyBlock(pos, true)` on a replaceable non-liquid block before `setBlock` | Fixed |
| 4 | Stale lead/saddle documentation after the v0.7 merge | Confirmed. `22ad25c` corrected both recipes and retains `retained_transport_recipes_match_java_26_3_and_consume_the_grid` | Fixed in restructuring, fixture README and a dated assessment follow-up. The roadmap is unchanged; see the proposal below |
| 5 | The fast-furnace fuel test restated the formula | Confirmed | Replaced with a simulated burn measured in game ticks |

### 1. Shared priority

`survival_items::bucket::use_held` is the shared ordering step. Each adapter
first runs its entity interactions: animal use and feeding, milking, mounts and
carts. `use_held` then returns `Use::Block(pos)` when
`content::interaction::block_use_precedes_item` says the targeted block acts
first. That covers containers, workstations, beds, doors and gates, dragon
eggs, note blocks, bells, levers, buttons, wooden trapdoors, daylight
detectors, repeaters, comparators, toggleable wire, a full composter, and a
composter with a compostable held item. Iron doors and trapdoors pass, as in
Java. Otherwise `use_held` runs the bucket or bottle use. `World::redstone_usable`
reports what `use_redstone` would do without changing the world.

The desktop/controller `Game::use_bucket` and the CLI `Agent::use_held_bucket`
both call `use_held`. The CLI no longer has a separate bucket arm. Buckets now
reach the generic `Place` path after `use_mob` (animal targets, milking, sheep,
golems), `use_mount`, `mount_cart` and cart placement, and before block use.
A CLI rider who cannot boost still uses buckets, because Java's `Item.use`
has no vehicle check.

### 2. Adventure

`use_bucket` now takes the `GameMode`. Spectators cannot use items. Without
build permission, empty, water, lava and fish buckets return `None` (FAIL).
Glass bottles and water-bucket creature capture still work. The CLI reports
`this game mode cannot use buckets here`.

### 3. Pouring

`bucket::empty_contents(world, hit, normal, fluid, sneaking)` implements the
target choice and `emptyContents`. Water and lava buckets and fish-bucket
releases all use the fluid-ignoring raycast. Fish releases previously stopped
at fluid sources; `MobBucketItem` has water content, so Java uses
`ClipContext.Fluid.NONE`. `Block::fluid_replaceable` approximates
`canBeReplaced(fluid)` as `is_replaceable()` plus cross-shaped non-cobweb blocks.
Cobwebs are forced solid in Java. A replaced non-fluid block spills its loot
through `World::spill_block`.

VoxelCraft's `LiquidBlockContainer` blocks are kelp, seagrass, waterlogged sea
pickles and live coral, and they always hold water. Water poured at one targets
it in place and empties the bucket without a block change, because Java's
`placeLiquid` returns false for a full container. A fish is still released
there. Lava, or sneaking with water, pours on the hit face. Fish releases now
report `Poured`/`Evaporated` instead of `Creature`. `Creature` remains for
capture.

### 5. Fuel test

`fast_furnaces_apply_java_integer_fuel_reduction` asserts the fixture's 4001
ticks for a dried kelp block. It then ticks each furnace kind in 0.05 s steps
with one fuel item and 64 inputs until the fire goes out. The burn lasts 4001
game ticks in a furnace and 2000 in a smoker or blast furnace, matching
4001 / 2 = 2000.

## Tests

New or changed in `tests/shared_buckets.rs`:

- `block_use_precedes_buckets_and_bottles_in_the_shared_order_and_on_the_cli`:
  five held items aimed at a lever. `use_held` returns `Use::Block`. CLI `Place`
  flips the lever, keeps the held item, releases no fish and emits no bucket event.
- `sneaking_skips_block_use_and_only_a_full_composter_claims_a_bucket`:
  sneaking at a lever pours on its face. A level-3 composter lets the bucket
  pour. A full composter yields its bone meal first.
- `cli_buckets_and_bottles_mount_carts_before_their_own_use`
- `cli_tropical_fish_bucket_feeds_an_axolotl_instead_of_releasing_the_fish`:
  the held item becomes a water bucket and no fish spawns.
- `adventure_fails_fluid_buckets_but_fills_bottles_and_captures_creatures`
- `buckets_pour_on_the_hit_face_of_targeted_grass_and_ferns`
- `emptying_into_a_plant_cell_destroys_it_with_its_drops`: covers dandelion
  (exact drop), fern and grass, with both water and lava.
- `water_fills_water_holding_plants_in_place_but_lava_and_sneaking_use_the_face`:
  covers seagrass with water, cod, lava and sneaking.
- Existing bucket tests use the `GameMode` signature.

`src/entity/aquatic.rs`: the capture/payload test uses the split
`capture_water_creature` and `release_water_creature`.
`tests/java_item_integrations.rs`: the simulated fuel-duration test.

Commands run on the final tree, both passing:

- `cargo fmt && cargo clippy --release --all-targets -- -D warnings && cargo test --release`:
  920 passed, 3 ignored.
- `cargo clippy --release --no-default-features --all-targets -- -D warnings && cargo test --release --no-default-features`:
  838 passed, 2 ignored.

Not run: interactive desktop/controller play, LAN, save/reload by a real
player, or frame-time measurement. The change adds one raycast and a block
lookup per bucket click and does not touch per-frame paths.

## Remaining differences

- `fluid_replaceable` covers only replaceable and cross-shaped blocks. Java's
  non-solid set also includes rails, redstone wire, buttons, levers and
  pressure plates. Poured fluid does not destroy those here.
- VoxelCraft targets cross-shaped plants as full cells, so a player ray always
  hits the plant rather than the block behind it. The destroy-with-drops path
  is reachable through `empty_contents` but not yet through ordinary aiming.
  Java's outline shapes let the ray pass beside a flower.
- VoxelCraft has no dry/waterlogged state pairs. Slabs, stairs, fences, dry
  sea pickles and dead coral cannot be waterlogged. Water poured at them goes
  to the hit face.
- `Level.mayInteract` (spawn protection, world border) and the Adventure
  `can_place_on` component are not modeled.
- Implemented with automated evidence on `work/fix-w1-building`: Adventure
  uses redstone components and composters; the shared block-use predicate covers
  every held item and drawing bows. Hungry players bite cake before item use;
  full players pass through to the held item. CLI door use was already moved
  into the shared session chain. Ordinary-player acceptance remains open.
- The desktop blocks bucket use while riding (boost or vehicle inventory). The
  CLI now allows it, as Java does.
- Bells ring from any face. Fish releases play the pour sound, not
  `BUCKET_EMPTY_FISH`. `use_held` uses a 6-block raycast for block priority;
  the CLI's own block target uses the session reach.

## Proposed roadmap updates

`docs/roadmap.md` was not edited. Proposed replacement for §3.2, lines 276-279:

> classifies reviewed animal/transport work as **Partial**. The 2026-10-12
> assessment found modern lead/saddle acquisition failing independent 26.3
> cases. The `22ad25c` merge corrected both recipes, and
> `retained_transport_recipes_match_java_26_3_and_consume_the_grid` now passes.
> Interaction, persistence and performance acceptance remain. Other behaviors
> remain Unassessed, and none of these groups is Verified.

Proposed addition under the STR-03/PLY-04 evidence: "Buckets and bottles share
Java's entity, then block, then item priority on desktop and CLI. Fluid buckets
fail in Adventure. Pouring targets the hit face except water into a water
holder, and destroys replaceable blocks with drops
([evidence](verification/2026-10-13-review-bucket-fixes.md)). Waterloggable
dry states, Shaped non-solid replacement and Adventure block use remain gaps."
