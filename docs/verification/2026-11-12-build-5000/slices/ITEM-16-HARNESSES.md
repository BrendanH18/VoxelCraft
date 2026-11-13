# ITEM-16-HARNESSES: harnesses (item side), 16 items, 16 recipes and 16 re-dye recipes

[← Campaign](../README.md)

## What was built

- `src/content/harness.rs` (new): the sixteen harness items, ids 1585–1600
  in `DyeColor` order (`white harness` … `black harness`), stack size 1,
  material kind, an original procedural icon (`Sprite::Harness`, a quilted
  pad in the dye's colour under a leather strap with two glass goggles,
  `src/render/item_sprites.rs`). It also holds `Equippable.harness`'s data
  for the happy ghast slices: body slot, equip and shearing sound ids,
  `can_equip_harness` (`happy_ghast` only), equip on interact, can be
  sheared. Nothing reads that data yet.
- `src/crafting/harness.rs` (new): `<colour>_harness` (`LLL` over `G#G`:
  leather, glass, the colour's wool) and `dye_<colour>_harness` (the dye and
  any of the other fifteen harnesses), added at the end of
  `crafting::recipes()`.
- `src/item.rs`: `Item::info` dispatch, the harnesses appended to
  `all_items` (and so the creative palette), their icons after the fermented
  spider eye (`fermented_spider_eye_icon()` replaces the
  `icon_count() - 1` lookup so later appends don't move it).
- `tests/fixtures/java/26.3/registry_gaps.json`: the sixteen
  `<colour>_harness` gap entries removed.

## Reference sources read

- `net/minecraft/world/item/Items.java` (`HARNESS`: `ColorCollection.registerItems`,
  `stacksTo(1)`, `Equippable.harness(color)`), `references/ItemIds.java`
  (`createSimpleColored("harness")`)
- `world/item/equipment/Equippable.java` (`harness`, `swapWithEquipmentSlot`,
  `canBeEquippedBy`), `world/item/Item.java` (`use`), `sounds/SoundEvents.java`
  (`HARNESS_EQUIP`, `HARNESS_UNEQUIP`), `world/item/CreativeModeTabs.java`,
  `world/level/block/DispenserBlock.java`
- `data/minecraft/tags/item/harnesses.json`,
  `data/minecraft/tags/entity_type/can_equip_harness.json`,
  `data/minecraft/tags/item/happy_ghast_tempt_items.json`,
  `data/minecraft/recipe/{colour}_harness.json`, `dye_{colour}_harness.json`
- `client/en_us.json` (`item.minecraft.<colour>_harness`), through the existing
  `registry_names.json`

The brief matched the reference. Harnesses appear in no loot table, trade or
other data file (the extractor checks), so crafting is the only survival
source, as in Java.

## IDs and orderings

- Item ids 1585–1600 (reserved for this slice), white to black.
- Appended: the item icon order (`all_items`, `sprite_index`, `icon_count`)
  gains sixteen icons after the fermented spider eye; `Sprite` gains
  `Harness` at the end; `crafting::recipes()` gains 32 recipes at the end.
  No saved enum changed; stacks save as `id:count` as before.

## Tests

- Oracle: `tools/extract_java_harness_fixtures.py` writes
  `tests/fixtures/java/26.3/harnesses.json` from the pinned server jar
  (tags, recipe headers, `javap` of `Items`, `Equippable` and `SoundEvents`).
  The recipe shapes come from the existing `recipes.json`.
- `tests/java_harnesses.rs`:
  - `the_sixteen_harnesses_match_the_java_tag_both_ways`: every tag entry
    resolves to the game's item of that colour, and every game harness is in
    the tag;
  - `harnesses_stack_to_one_and_carry_the_java_equippable_data`;
  - `a_player_cannot_wear_a_harness` (Java's swap fails outside
    `can_equip_harness`; the game's armor equip refuses it);
  - `every_harness_recipe_is_registered_and_matches_java`: none of the 32
    is left unregistered in `compare_crafting`, which also checks the game
    has no recipe Java lacks;
  - `harnesses_craft_from_leather_glass_and_wool_at_a_crafting_table`: all
    colours, both row positions, the grid is consumed, flipped or wrong
    patterns make nothing;
  - `any_other_harness_and_a_dye_make_that_colour_in_the_inventory_grid`:
    all 256 pairs (same colour makes nothing), two harnesses or two dyes
    make nothing;
  - `the_cli_crafts_and_redyes_harnesses_on_the_shared_path`: the CLI's
    `craft` makes a harness at a targeted table and re-dyes one without;
  - `harness_stacks_save_and_load`.
- `tests/java_recipes.rs` and `tests/java_registry.rs` pass with the
  harnesses registered.
- `scripts/remote-check.sh --full` passed (clippy and every test, with and
  without default features); `tests/java_harnesses.rs` also passed on its
  own with default features.
- `examples/id_map.rs -- items:1585-1600` reports exactly 1585–1600
  registered (16) and 1535–1584 and 1601 up free.

## Differences from Java

- Nothing can wear a harness yet: the happy ghast, equipping, riding,
  shearing and the equip/unequip sounds belong to the happy ghast slices.
  The `Equippable` data is constants only.
- Recipe `group` (`harness`, `harness_dye`) and `category` (`equipment`)
  are recorded in the fixture but not modelled; the recipe book has neither.
- The creative palette lists the harnesses after the other items in id
  order, not in Java's Tools & Utilities tab position.
- The recipe advancements (`advancement/recipes/combat/*_harness.json`)
  are not implemented.
- The icon is original procedural art, not Java's texture (FE-02 policy).

## Unverified

- Nobody has looked at the icon in the running game; the shape was checked
  only as an ASCII preview of the sprite function.
- A dispenser firing a harness: Java falls back to dropping it when no happy
  ghast is in front; the game's dispenser was not tested with one.
- The desktop and controller crafting screens use the same
  `crafting::recipes()` as the CLI, but were not driven with a harness.

## Scripted capture

```sh
cargo run --release -- --world harness-shots --new --seed 42 --creative \
    --data-dir target/screenshots --open-inventory \
    --give white_harness --give orange_harness --give magenta_harness --give light_blue_harness \
    --give yellow_harness --give lime_harness --give pink_harness --give gray_harness \
    --give light_gray_harness --give cyan_harness --give purple_harness --give blue_harness \
    --give brown_harness --give green_harness --give red_harness --give black_harness \
    --screenshot target/screenshots/harnesses.png
```
