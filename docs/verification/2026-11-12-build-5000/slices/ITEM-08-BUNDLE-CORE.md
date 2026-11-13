# ITEM-08-BUNDLE-CORE: bundles, 17 items, the weight rule, recipe, 16 dye transmutes, village loot and save

[← Campaign](../README.md)

## What was built

- `src/content/bundle.rs` (new): the plain bundle and the sixteen dyed
  bundles, item ids 1568–1584 (plain, then `DyeColor` order), stack size 1,
  with an original procedural icon (`Sprite::Bundle`, a round leather pouch
  in the bundle's colour, cinched by a pale string tie, its open frilled
  mouth showing the dark inside). It holds Java's `BundleContents` rules on
  the existing interned `Payload::Bundle` slot (ITEM-00-COMPONENTS):
  - exact `Fraction` weights: `1 / max stack size` per item, a bundle
    `1/16` plus its contents, a hive or nest item holding bees `1`; a
    bundle's contents may weigh at most `1`;
  - `try_insert` (`Mutable.tryInsert`): as many as fit, `getMaxAmountToAdd`
    floored, merging with an equal stack (same item and components, only
    for stackable items) that moves to the front, else a new front stack;
  - `remove_one` takes the front stack;
  - `click`: `BundleItem.overrideStackedOnOther` (held bundle) then
    `overrideOtherStackedOnMe` (clicked bundle), as
    `AbstractContainerMenu.tryItemClickBehaviourOverride` orders them;
  - `transmute`: the sixteen `crafting_transmute` recipes, every component
    of the input kept, and Java's refusal when the result would equal the
    input (a bundle dyed its own colour);
  - `bar_width` / `bar_rgb`: `BundleItem`'s fullness bar.
- `src/inventory.rs`: `click_slot`, the click every container screen and
  the desktop, controller and LAN adapters share, runs the bundle override
  first.
- `src/crafting/bundle.rs` (new): `bundle.json` (string over leather) and
  the sixteen transmutes as shapeless `#bundles` + dye entries for the
  recipe guide and the CLI's `craft`; `Grid::result` takes the real result
  from `bundle::transmute`.
- `src/app/hud.rs`: the fullness bar where the durability bar goes.
- `src/item.rs`: `Item::info` dispatch, the bundles appended to `all_items`
  (so the creative palette), icons after the experience bottle.
- Village loot goes live without code: `loot_tables.json` already named
  `minecraft:bundle` in the cartographer and the five house tables, which
  now resolves. `tools/JavaLootVectors.java` (the JDK oracle for the
  cartographer chest) keeps the bundle it used to discard, and
  `src/world/loot.rs` gains its seed-1 vector, which holds one.
- `tests/java_recipes/compare.rs`: a transmute grid holding its own result
  must craft nothing (`TransmuteRecipe.matches`).
- `tests/fixtures/java/26.3/registry_gaps.json`: the seventeen bundle
  entries removed.

## Reference sources read

- `net/minecraft/world/item/component/BundleContents.java` (weights,
  `Mutable.tryInsert`, `tryTransfer`, `removeOne`, `getMaxAmountToAdd`,
  `toggleSelectedItem`, `mergeIdenticalStacks`)
- `net/minecraft/world/item/BundleItem.java` (click overrides, fullness bar,
  `use`/`onUseTick` drop, `onDestroyed`, sounds)
- `net/minecraft/world/inventory/AbstractContainerMenu.java`
  (`doClick`, `tryItemClickBehaviourOverride`), `Slot.safeInsert`,
  `safeTake`, `tryRemove`
- `net/minecraft/world/item/crafting/TransmuteRecipe.java`
- `net/minecraft/world/item/Items.java` (`BUNDLE`, `DYED_BUNDLE`, case item
  stack sizes), `Item.java` (`DEFAULT_MAX_STACK_SIZE`,
  `canFitInsideContainerItems`), `BlockItem.java` (shulker boxes don't fit)
- `data/minecraft/tags/item/bundles.json`, `data/minecraft/recipe/bundle.json`,
  `data/minecraft/recipe/{colour}_bundle.json`
- `data/minecraft/loot_table/chests/village/village_{cartographer,desert_house,plains_house,savanna_house,snowy_house,taiga_house,tannery,weaponsmith}.json`,
  `chests/abandoned_camp_common_chest.json`, `barrels/abandoned_camp_barrel.json`

The brief matched the reference and the code. Two additions found while
reading: Java's carried-bundle override runs before the clicked one (a held
bundle left-clicked on another bundle takes it in), and a transmute whose
result equals its input is not a match, so a red bundle and red dye craft
nothing; `compare.rs` previously expected every transmute grid to craft.

## IDs and orderings

- Item ids 1568–1584 (this slice's reservation), plain then white to black.
- Appended: the item icon order (`all_items`, `sprite_index`, `icon_count`)
  gains seventeen icons after the experience bottle; `Sprite` gains
  `Bundle` at the end; `crafting::recipes()` gains seventeen recipes at the
  end. No saved enum changed. Bundles save through the existing payload
  field: a plain bundle is `id:count`; contents are base64 JSON of stack
  strings, nested for nested bundles.

## Tests

- Oracle: `tools/extract_java_bundle_fixtures.py` writes
  `tests/fixtures/java/26.3/bundles.json` from the pinned server jar (tags,
  recipes, loot pools, javap of `Items`, `Item`, `BundleContents` and
  `BundleContents$Mutable`) and runs its own exact-fraction model of
  `BundleContents.Mutable` and the two click overrides over 15 insert/remove
  sequences and 19 clicks.
- `tests/java_bundles.rs` (15 tests):
  - `the_seventeen_bundles_match_the_java_tag_both_ways`;
  - `weights_follow_bundle_contents`: the constants, and the case items'
    stack sizes against the jar's;
  - `bundles_hold_at_most_one_unit_of_weight`,
    `a_nested_bundle_weighs_a_sixteenth_plus_its_contents`,
    `a_hive_holding_bees_weighs_one`,
    `inserting_merges_to_the_front_and_removing_takes_the_front`, and
    `every_fixture_sequence_is_replayed`: every step's accepted count,
    leftover, order and exact weight;
  - `container_clicks_match_the_java_overrides_on_the_shared_click_path`:
    the 19 clicks through `bundle::click`, `inventory::click_slot` and
    `Inventory::click`;
  - `filling_and_emptying_through_the_inventory_keeps_every_item`;
  - `the_fullness_bar_follows_bundle_item`;
  - `the_bundle_recipe_and_its_dye_transmutes_match_java`: the shared
    `compare_crafting` both ways (no bundle recipe unregistered, none
    invented), and string over leather in both grids;
  - `dyeing_any_bundle_keeps_its_contents_and_name`: all 272 bundle and dye
    pairs in two grid positions, nested contents and name kept, own colour
    refused, one dye and one bundle only, consumption;
  - `the_cli_crafts_and_dyes_a_filled_bundle_on_the_shared_path`;
  - `village_chests_hold_a_bundle_one_time_in_three`: the six live tables
    rolled 6,000 times each (within five standard deviations of 1/3), one
    plain bundle at most per chest; the four Java tables the game lacks are
    asserted unrolled;
  - `bundles_with_nested_contents_save_and_load`: nested bundles, a damaged
    enchanted sword, names and a nest holding a bee through stack strings,
    the inventory, a chest and a dropped item; plain bundles keep `id:count`.
- `src/content/bundle.rs`: fraction arithmetic unit test.
- `src/world/loot.rs`: the cartographer JDK vector for seed 1, with a bundle.
- `VC_REMOTE_RETRIES=60 scripts/remote-check.sh --full` passed at
  `5c7e0d09` (clippy and every test, with and without default features;
  `java_bundles`, `java_recipes` and `java_registry` among them).
- `examples/id_map.rs -- items:1568-1584` reports exactly 1568–1584
  registered (17).
- `tools/JavaLootVectors.java` was rerun: seeds 0, 42 and 12345 are
  unchanged by keeping the bundle; seed 1 holds one.
- `python3 tools/campaign_ledger.py` (47 rows) and
  `python3 tools/check_doc_links.py` pass.

## Differences from Java

- No sounds: `bundle.insert`, `insert_fail`, `remove_one` and
  `drop_contents` have no cues yet. `bundle::click` returns which one Java
  would play, for the audio work to hook up.
- Using a bundle (holding use to throw its contents out one stack at a time,
  `onUseTick`) and a dropped bundle spilling its contents when destroyed
  (`onDestroyed`) are not implemented.
- No contents tooltip and no selected item: Java's client lets the scroll
  wheel pick which stack comes out (`toggleSelectedItem`); here the front
  stack always comes out, which is Java's behaviour with nothing selected.
- Restricted slots: screens that check the held item before calling
  `click_slot` (furnace fuel, brewing, enchanting lapis, smithing inputs)
  ignore a held bundle, where Java would let it take that slot's items or
  try (and fail) to put one in. Result slots (crafting, furnace, anvil,
  smithing) don't run the override either; in Java a held bundle can scoop
  a crafting result.
- Two Java village tables that name a bundle (tannery, weaponsmith) and the
  two abandoned-camp tables are not in the game's loot data yet (villages
  area; v0.18); none is counted here.
- Shulker boxes, the only items that cannot go in a bundle, aren't
  registered (v0.18); `bundle::can_fit` accepts everything until they are.
- Recipe `group` (`bundle_dye`) and `category` are recorded, not modelled.
- Creative palette position: after the other items in id order, not Java's
  Tools & Utilities tab position.
- The CLI's `craft` takes the first matching stack in the inventory: with a
  bundle of the target colour ahead of another bundle it reports missing
  ingredients rather than choosing the other bundle.

## Unverified

- Nobody has looked at the icons or the fullness bar in the running game;
  the icon shape was checked only as an ASCII preview of the sprite mask.
- The desktop, controller and LAN screens reach the override through the
  same `click_slot`; only the library click paths were driven by tests.

## Scripted capture

The icons in the creative inventory (a filled bundle's bar needs play; no
scripted flag gives a stack contents):

```sh
cargo run --release -- --world bundle-shots --new --seed 42 --creative \
    --data-dir target/screenshots --open-inventory \
    --give bundle --give white_bundle --give orange_bundle --give magenta_bundle \
    --give light_blue_bundle --give yellow_bundle --give lime_bundle --give pink_bundle \
    --give gray_bundle --give light_gray_bundle --give cyan_bundle --give purple_bundle \
    --give blue_bundle --give brown_bundle --give green_bundle --give red_bundle --give black_bundle \
    --screenshot target/screenshots/bundles.png
```
