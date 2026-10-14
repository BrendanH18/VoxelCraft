# Review fixes: saplings, fishing, sponge, armor reach and lock evidence — 2026-10-13

Scope: confirmed review findings against the content/resource slice, on
`work/review-content-2` from `ab568fc`. Reference: Minecraft Java Edition 26.3,
official server SHA-1 `33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`, inspected
with `javap -c -p` and the bundled `data/minecraft` JSON. No game files are
committed.

## Findings and fixes

| # | Finding | Reference evidence | Fix |
|---|---------|--------------------|-----|
| 1 | Pale oak grew from one sapling; bone meal was always consumed on saplings | `TreeGrower.<clinit>`: `PALE_OAK` has only `TreeFeatures.PALE_OAK_BONEMEAL` as its mega feature (like `DARK_OAK`). `TreeGrower.canGrow` returns `findTwoByTwoSaplingPos(...).isPresent()` when there is no single-tree feature; `SaplingBlock.isValidBonemealTarget` uses it | `src/world/growth.rs`: dark and pale oak require a 2x2; bone meal on a lone one returns unused. A duplicate overworld sapling arm in `src/world/overworld_flora.rs` was removed so every sapling uses the shared check |
| 2 | 2x2 search order | `findTwoByTwoSaplingPos`: outer x from 0 to -1, inner z from 0 to -1 → (0,0), (0,-1), (-1,0), (-1,-1) | Same order in `World::two_by_two_corner` |
| 3 | Closed water caught only junk | `gameplay/fishing.json`: junk 10/q-2, treasure 5/q2 with `in_open_water`, fish 85/q-1; only treasure is conditioned. `LootPool.addRandomItem` keeps entries with positive weight, takes a lone entry without a roll, returns nothing when total is 0 | `src/entity/bobber.rs` rolls the typed tables with that algorithm |
| 4 | Junk table incomplete; wrong rod damage | `fishing/junk.json` (13 entries incl. lily pad 17, ink sac ×10, tripwire hook, jungle-only bamboo). `SetItemDamageFunction.run`: `setDamageValue(floor((1 - clamp(value + 0, 0, 1)) * max))` | `src/content/fishing.rs` typed rows; damage via `set_damage`. The bobber records whether its biome (including cave biomes) is a jungle when the fish bites |
| 5 | Treasure enchantment ignored levels and curses | `fishing/treasure.json`: bow/rod `set_damage` 0–0.25 then `enchant_with_levels` 30, book `enchant_with_levels` 30, options `#on_random_loot` (non-treasure + both curses, frost walker, mending). `EnchantmentHelper.enchantItem` applies every `selectEnchantment` pick and turns a book into an enchanted book | `enchant::enchant_with_levels` reuses the existing Java `selectEnchantment` port with a random-loot option set |
| 6 | Sponge skipped waterlogged blocks; wet sponges dried on neighbour change | `SpongeBlock.lambda$removeWaterBreadthFirstSearch$1`: water fluid required; `BucketPickup.pickupBlock` non-empty → ACCEPT; else `LiquidBlock` → air; kelp/seagrass → drop + air. `WetSpongeBlock.onPlace` only | `src/world/sponge.rs`: live coral plants/fans drain, count and continue the search; wet sponges dry only when placed in the Nether |
| 7 | Armor equip blocked by usable blocks 6 blocks away | Block interaction range 4.5 (Creative 5.0), as recorded in `mining.json` | `equip_armor` uses `GameMode::block_reach`, shared with `PlayerSession::block_reach` |
| 8 | Docs | — | `docs/survival-items.md` fishing paragraph rewritten; the content report's lock claim is now backed by a separate probe process |

## Tests

- `tests/fishing_loot.rs` (new, 5 tests) reads
  `tests/fixtures/java/26.3/fishing.json`, produced by the new
  `tools/extract_java_fishing_fixtures.py` from the pinned server jar. The
  extractor checks the SHA-1, fails on unknown entry types, keys, conditions
  or loot functions, and checks the `LootPool`, `SetItemDamageFunction` and
  `enchantItem` bytecode forms. Tests: every category/table row; every
  registered enchantment is in `#on_random_loot`; category shares for closed
  water/luck 0 and open water/Luck of the Sea III within 1 percentage point
  over 40,000 rolls; `None` when every weight is zero; ink sac count, junk damage
  ranges and jungle-only bamboo; treasure damage range, enchanted books, curses
  and treasure enchantments appearing.
- `tests/plant_integrations.rs`: pale/dark oak 2x2 requirement, bone meal not
  consumed on a lone sapling, (0,0) corner chosen in a 3x3 field; sponge drains
  coral fans/plants and continues past them; a Nether wet sponge survives
  neighbour changes but a placed one dries.
- `tests/turtle_integrations.rs`: a crafting table 3.5 blocks away blocks
  equipping; one 5.5 blocks away no longer does.
- `tests/process_persistence.rs`: the writer process spawns `lock_probe_worker`
  as a separate process while holding the lock; it must observe `WouldBlock`.
- `src/entity/bobber.rs` unit tests: fish obtainable, closed water yields fish
  but never treasure, `set_damage` endpoints, jungle flag recorded at the bite.

Each new regression would fail on `ab568fc` (lone-pale-oak growth, (-1,-1)
corner, junk-only closed water, sponge stopping at coral, Nether drying on
neighbour change, 6-block armor check).

Checks run on the final tree: `cargo fmt`, `cargo clippy --release
--all-targets -D warnings`, `cargo test --release`, and the same clippy/test
with `--no-default-features`.

## Remaining differences

- Loot rolls use VoxelCraft's RNG rather than Java's random sequences;
  shares and ranges match but individual rolls do not. The enchantment
  candidate order follows VoxelCraft's enum order rather than tag order, which
  changes which roll maps to which pick but not the distribution.
- Since the v0.7 fixes the reel adds the player's Luck attribute (the new Luck
  and Bad Luck effects, ±1 per level; ids `luck`/`unluck`) to Luck of the Sea and
  reads the hook's biome for the bamboo `location_check` at the reel, as Java's
  loot context does. Both effects are appended to the effect registry and icon
  array; they have no other gameplay yet (chest loot ignores the attribute).
- Sea pickles are `BucketPickup` in Java but have no unwaterlogged state here,
  so the sponge still stops at them. Drained live coral becomes dead coral at
  once unless water still touches it. In Java it stays live but dry and dies
  60–100 ticks later. Both need new, separately reserved block states.
- Saplings have no `stage` property: one successful bone meal grows the tree,
  where Java first advances stage 0→1. Spruce and jungle 2x2 mega trees are
  not implemented.
- Creative armor use is fixed in this branch: empty slots receive a copy,
  occupied slots return their old piece, and Binding is bypassed only in
  Creative (`Equippable.swapWithEquipmentSlot`, pinned 26.3 bytecode).
  A shared-action regression covers copying, swapping, Binding and Spectator.
- No ordinary-player playtest or performance measurement was run for these
  fixes; none of them is marked Verified.

## Proposed roadmap updates

For `docs/roadmap.md` §3.1 (not edited here):

- **FOO-03:** replace "fishing pools still omit existing tropical fish,
  pufferfish and nautilus shells" with: "Fishing rolls Java 26.3's complete
  `gameplay/fishing` tables (fish in closed water, open-water treasure,
  jungle bamboo, set_damage, enchant_with_levels 30 over `#on_random_loot`),
  checked against `fishing.json`. Remaining: Java random sequences, the player
  luck attribute and ordinary-player acceptance."
- **FOO-02:** "Dark and pale oak saplings need a 2x2 (reference search order);
  bone meal on a lone one is not consumed. Sapling `stage` and spruce/jungle
  mega trees remain open."
- **ENV-04:** "Sponges drain waterlogged live coral and continue the search.
  Wet sponges dry only on Nether placement. Sea pickle draining and dry
  live-coral states need reserved block states."
- Add under the shared-input notes: "Armor-equip block checks use the Java
  block interaction range."
- Link this report beside the content/resource report.
